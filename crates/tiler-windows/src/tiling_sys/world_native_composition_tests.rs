#![forbid(unsafe_code)]
//! End-to-end world composition over the production actuator.
//!
//! Every test drives the real `LiveNative` actuator plus the real `WorldOwner`
//! sequencing (capture, sequence, deliver, diff-apply, receipt, merge, ack)
//! with a simulated desktop through the shared harness in the parent module
//! (`live_tick` with `moving_prims`): `set_outer` physically moves the observed
//! frame, hide/reveal flip live visibility, and setters may lie. Shared rig
//! helpers (`env_with`, `admit_bare`, `script_live`, `one_area`, `live_tick`
//! and the scripted-native maps) come from the parent via `use super::*`;
//! composition-local helpers live here alongside the tests.

use super::*;
use tiler_core::world_report::FrameSetter;

#[test]
fn bootstrap_first_binding_proves_homing_and_views_then_settles() {
    let mut env = env_with(201_001);
    let out = env.state.output_token(DEV1);
    let key = win_key(201_001, 2201, "proc-101");
    admit_bare(&mut env, &key, "w1");
    script_live(&key, "w1", rect(0, 0, 800, 600), plain_facts());
    let mut owner = WorldOwner::bootstrap_ids(ADAPTER, GENERATION).expect("owner");
    let areas = one_area();
    let fulls = env.fulls.clone();
    // Tick 1: bootstrap. Geometry places the core slot; the exact
    // post-write frame plus visibility proves the first homing and view
    // with no prior receipt to re-prove.
    let observed = live_visible_observed();
    let tick1 = live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        0,
        None,
    );
    assert!(tick1.reply.ready);
    let home = env
        .state
        .applied_homing
        .get("w1")
        .expect("first homing proves")
        .clone();
    assert_eq!(home.0, out, "homed on the live output: {home:?}");
    let view = env
        .state
        .applied_views
        .get(&out)
        .expect("first view proves")
        .clone();
    assert_eq!(home.1, view, "homing names the proven view");
    // The placed frame is the reply slot, not the pre-move rectangle.
    let slot = tick1
        .reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "w1")
        .and_then(|row| row.effective_frame)
        .expect("tiled slot");
    let live = live_visible_observed();
    assert_eq!(live.len(), 1);
    assert_eq!(live[0].visible, slot, "slot physically placed");
    // Tick 2: the next report carries the receipts and settles with no
    // retained flights.
    let observed = live_visible_observed();
    let tick2 = live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        0,
        None,
    );
    assert!(tick2.reply.ready);
    assert!(
        tick2.reply.transfers.is_empty(),
        "no flights survive proof: {:?}",
        tick2.reply.transfers
    );
    assert_eq!(env.state.applied_homing.get("w1"), Some(&home));
    assert_eq!(env.state.applied_views.get(&out), Some(&view));
}

/// Mover+sibling pair for follow tests: both admitted bare with scripted
/// live frames plus working hide/reveal claims.
fn admit_follow_pair(env: &mut Env, base_hwnd: u64) -> (WindowKey, WindowKey) {
    let mover = win_key(base_hwnd, 2301, "proc-201");
    let sibling = win_key(base_hwnd + 1, 2302, "proc-202");
    for (key, token) in [(&mover, "w1"), (&sibling, "w2")] {
        admit_bare(env, key, token);
        script_live(key, token, rect(0, 0, 800, 600), plain_facts());
        S_ADMIT.with(|m| {
            m.borrow_mut()
                .insert(key.hwnd, Ok(script_claim(key.hwnd, &identity_of(key))))
        });
        S_HIDE.with(|m| {
            m.borrow_mut()
                .insert(key.hwnd, Ok(valid_claim(key.hwnd, &identity_of(key))))
        });
        S_REVEAL.with(|m| m.borrow_mut().insert(key.hwnd, Ok(Revealed::Shown)));
    }
    (mover, sibling)
}

/// Current view, another ring workspace, and its numbered index.
fn follow_target(owner: &WorldOwner, out: &str) -> (String, String, u8) {
    let current = owner.engine().logical_view(out).expect("current view");
    let target = second_workspace(owner, out, &current);
    let ring = owner.engine().logical_ring(out);
    let index = ring
        .iter()
        .position(|id| id == &target)
        .expect("target in ring") as u8
        + 1;
    (current, target, index)
}

fn published_view(reply: &WorldReply, output: &str) -> Option<String> {
    reply
        .outputs
        .iter()
        .find(|o| o.output.0 == output)
        .and_then(|o| o.logical_view.clone())
}

fn has_member_row(tick: &LiveTick, op: &str) -> bool {
    tick.members.iter().any(|a| a.operation_id == op)
}

/// Ambiguous member evidence: a raw attempt for `op` that proves no
/// arrival (post unreadable, off-target, or a refused/throwing setter).
/// Never a verdict; the core owns the hold.
fn has_ambiguous_member(tick: &LiveTick, op: &str, out: &str, ws: &str) -> bool {
    tick.members.iter().any(|a| {
        a.operation_id == op
            && a.post
                .as_ref()
                .is_none_or(|(post_out, post_ws)| post_out.0 != out || post_ws != ws)
    })
}

fn has_view_row(tick: &LiveTick, op: &str) -> bool {
    tick.views.iter().any(|a| a.operation_id == op)
}

/// Proven view evidence: a raw view attempt for the owning phase id with
/// post proof at the target.
fn has_proven_view(tick: &LiveTick, op: &str, ws: &str) -> bool {
    tick.views
        .iter()
        .any(|a| a.operation_id == op && a.post.as_deref() == Some(ws))
}

fn has_any_attempt(tick: &LiveTick, op: &str) -> bool {
    has_member_row(tick, op) || has_view_row(tick, op)
}

/// Pre-proven singleton follow: the mover already shows full-frame on the
/// target with no sibling to hide, so the diff proves arrival with no
/// setter, emits no raw attempts, merges the receipts, and the next report
/// settles both flights. A raw-attempt oracle must not require a member row
/// here: the no-write proof is the correct production behavior.
#[test]
fn singleton_follow_preproven_settles_without_attempts() {
    let mut env = env_with(202_901);
    let out = env.state.output_token(DEV1);
    let mover = win_key(202_901, 2391, "proc-291");
    admit_bare(&mut env, &mover, "w1");
    script_live(&mover, "w1", rect(0, 0, 800, 600), plain_facts());
    S_ADMIT.with(|m| {
        m.borrow_mut().insert(
            mover.hwnd,
            Ok(script_claim(mover.hwnd, &identity_of(&mover))),
        )
    });
    S_HIDE.with(|m| {
        m.borrow_mut().insert(
            mover.hwnd,
            Ok(valid_claim(mover.hwnd, &identity_of(&mover))),
        )
    });
    S_REVEAL.with(|m| m.borrow_mut().insert(mover.hwnd, Ok(Revealed::Shown)));
    let mut owner = WorldOwner::bootstrap_ids(ADAPTER, GENERATION).expect("owner");
    let areas = one_area();
    let fulls = env.fulls.clone();
    let observed = live_visible_observed();
    live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        0,
        None,
    );
    let (current, target, index) = follow_target(&owner, &out);
    // Force the live frame to already equal the published target slot: the
    // singleton already shows full-frame, so no geometry write is required.
    // The bootstrap placed the slot; the trailing target reuses the same full
    // slot shape, so the pre-move frame already proves.
    let observed = live_visible_observed();
    let tick = live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        mover.hwnd,
        Some(WorldCommand::SendNumbered {
            window: WindowId("w1".to_owned()),
            output: OutputId(out.clone()),
            index,
            follow: true,
        }),
    );
    let op = window_op(&tick.reply, "w1");
    let view_op = tick
        .reply
        .transfers
        .iter()
        .find(|t| matches!(t.kind, TransferKind::View))
        .expect("follow retains its view flight")
        .operation_id
        .clone();
    assert_eq!(view_op, format!("{op}-follow"));
    // No setter ran: the native already proves the target.
    assert!(
        tick.members.is_empty() && tick.views.is_empty(),
        "pre-proven emits no attempts: {:?} {:?}",
        tick.members,
        tick.views
    );
    // Receipts still merge from independent fresh proof.
    assert_eq!(
        env.state.applied_homing.get("w1"),
        Some(&(out.clone(), target.clone())),
        "homing merges without a setter"
    );
    assert_eq!(
        env.state.applied_views.get(&out),
        Some(&target),
        "view merges without a setter"
    );
    assert_ne!(current, target);
    // Next observation settles both flights with no replay.
    let observed = live_visible_observed();
    let settled = live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        0,
        None,
    );
    assert!(
        settled.reply.transfers.is_empty(),
        "pre-proven follow settles: {:?}",
        settled.reply.transfers
    );
    assert!(
        settled.members.is_empty() && settled.views.is_empty(),
        "no replay: {:?} {:?}",
        settled.members,
        settled.views
    );
}

/// Scopes of one tick reply as `scope=authority` pairs for failure output.
fn scope_summary(reply: &WorldReply) -> Vec<String> {
    reply
        .scopes
        .iter()
        .map(|s| format!("{}={:?}", s.scope, s.authority))
        .collect()
}

/// Assert every published scope is controlled; otherwise panic with context.
fn assert_all_controlled(label: &str, reply: &WorldReply) {
    assert!(
        reply.scopes.iter().all(|s| s.authority == Auth::Controlled),
        "{label}: held scopes: {:?}",
        scope_summary(reply)
    );
}

#[test]
fn same_output_numbered_send_follow_selects_proves_and_settles() {
    let mut env = env_with(202_011);
    let out = env.state.output_token(DEV1);
    let (mover, sibling) = admit_follow_pair(&mut env, 202_011);
    let mut owner = WorldOwner::bootstrap_ids(ADAPTER, GENERATION).expect("owner");
    let areas = one_area();
    let fulls = env.fulls.clone();
    let observed = live_visible_observed();
    live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        0,
        None,
    );
    let (current, target, index) = follow_target(&owner, &out);
    // Follow with a source sibling and no external select: the reply
    // publishes the target view immediately with the commit.
    let observed = live_visible_observed();
    let tick = live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        mover.hwnd,
        Some(WorldCommand::SendNumbered {
            window: WindowId("w1".to_owned()),
            output: OutputId(out.clone()),
            index,
            follow: true,
        }),
    );
    assert_eq!(
        published_view(&tick.reply, &out).as_deref(),
        Some(target.as_str())
    );
    let op = window_op(&tick.reply, "w1");
    let view_op = tick
        .reply
        .transfers
        .iter()
        .find(|t| matches!(t.kind, TransferKind::View))
        .expect("follow retains its view flight")
        .operation_id
        .clone();
    assert_eq!(view_op, format!("{op}-follow"));
    // Two-phase proof: the window transfer reports one ambiguous member
    // attempt until the switch proves, while the follow's own view flight
    // actuates and proves the switch in the same tick.
    assert!(
        has_ambiguous_member(&tick, &op, &out, &target),
        "membership unproven pre-switch: {:?} {:?}",
        tick.members,
        tick.views
    );
    assert!(!applied_of_members(&tick.members, &tick.views, &op));
    assert!(
        has_proven_view(&tick, &view_op, &target),
        "view flight proves the switch: {:?}",
        tick.views
    );
    assert!(
        tick.reply.effects.iter().any(|e| e.target.0 == "w1"),
        "mover focus: {:?}",
        tick.reply.effects
    );
    assert!(
        S_VISIBLE.with(|m| m.borrow().contains(&mover.hwnd)),
        "mover visible"
    );
    assert!(
        !S_VISIBLE.with(|m| m.borrow().contains(&sibling.hwnd)),
        "sibling hidden"
    );
    // Target/member prove independently: the mover rides the target, the
    // sibling stays homed at the source, the view names the target.
    assert_eq!(
        env.state.applied_homing.get("w1"),
        Some(&(out.clone(), target.clone()))
    );
    assert_eq!(
        env.state.applied_homing.get("w2"),
        Some(&(out.clone(), current.clone()))
    );
    assert_eq!(env.state.applied_views.get(&out), Some(&target));
    // Next observation settles with no replay.
    let observed = live_visible_observed();
    assert_eq!(observed.len(), 1, "only the mover reads visible");
    assert_eq!(observed[0].hwnd, mover.hwnd);
    let settled = live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        0,
        None,
    );
    assert!(
        settled.reply.transfers.is_empty(),
        "follow settles: {:?}",
        settled.reply.transfers
    );
    assert!(
        !has_any_attempt(&settled, &op),
        "no replay: {:?} {:?}",
        settled.members,
        settled.views
    );
    assert!(
        !has_any_attempt(&settled, &view_op),
        "view flight settles too: {:?} {:?}",
        settled.members,
        settled.views
    );
    assert_eq!(
        env.state.applied_homing.get("w1"),
        Some(&(out.clone(), target.clone()))
    );
    assert_eq!(env.state.applied_views.get(&out), Some(&target));
    assert!(
        !S_VISIBLE.with(|m| m.borrow().contains(&sibling.hwnd)),
        "sibling stays hidden"
    );
}

/// Lying frame setter: the write reports success but the frame never lands.
/// Frameless follow rows take no writes, so the lie cannot engage at send
/// time: the view switches and proves on honest visibility while the window
/// holds ambiguous without membership proof. When the sweep later publishes
/// the frame, the lie surfaces exactly once as a Clamp hold. Never Applied.
#[test]
fn same_output_follow_lying_frame_setter_holds_without_applied() {
    fn lying_prims() -> NativePrimitives {
        let mut prims = moving_prims();
        prims.set_outer = |hwnd, outer| {
            S_SET_CALLS.with(|m| m.borrow_mut().push((hwnd, outer)));
            S_SET_OK.with(|m| m.borrow().get(&hwnd).copied().unwrap_or(true))
        };
        prims
    }
    let mut env = env_with(202_021);
    let out = env.state.output_token(DEV1);
    let (mover, sibling) = admit_follow_pair(&mut env, 202_021);
    let mut owner = WorldOwner::bootstrap_ids(ADAPTER, GENERATION).expect("owner");
    let areas = one_area();
    let fulls = env.fulls.clone();
    // Honest bootstrap places both windows so later proofs start truthful;
    // only the send and its aftermath run under the lying setter.
    let observed = live_visible_observed();
    live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        0,
        None,
    );
    let (_current, target, index) = follow_target(&owner, &out);
    let calls_before = set_calls();
    let observed = live_visible_observed();
    let tick = live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        lying_prims(),
        mover.hwnd,
        Some(WorldCommand::SendNumbered {
            window: WindowId("w1".to_owned()),
            output: OutputId(out.clone()),
            index,
            follow: true,
        }),
    );
    // The reply still publishes the follow target view (logical authority).
    // The window holds ambiguous without membership proof while the
    // follow's own view flight actuates and proves the switch in the same
    // tick; frameless rows take no writes, so the lie cannot engage yet.
    assert_eq!(
        published_view(&tick.reply, &out).as_deref(),
        Some(target.as_str())
    );
    let op = window_op(&tick.reply, "w1");
    let view_op = tick
        .reply
        .transfers
        .iter()
        .find(|t| matches!(t.kind, TransferKind::View))
        .expect("follow retains its view flight")
        .operation_id
        .clone();
    assert!(
        has_ambiguous_member(&tick, &op, &out, &target),
        "membership unproven pre-switch: {:?} {:?}",
        tick.members,
        tick.views
    );
    assert!(!applied_of_members(&tick.members, &tick.views, &op));
    assert!(
        has_proven_view(&tick, &view_op, &target),
        "honest hide leg proves the view: {:?}",
        tick.views
    );
    assert_eq!(
        set_calls(),
        calls_before + 1,
        "source collapse re-tiles the sibling"
    );
    assert!(S_VISIBLE.with(|m| m.borrow().contains(&mover.hwnd)));
    assert!(
        !S_VISIBLE.with(|m| m.borrow().contains(&sibling.hwnd)),
        "sibling hides under the proven view"
    );
    // Visibility plus the proven view merges homing even though no frame
    // ever moved: the mover is genuinely showing on the target.
    assert_eq!(env.state.applied_views.get(&out), Some(&target));
    assert_eq!(
        env.state.applied_homing.get("w1"),
        Some(&(out.clone(), target.clone()))
    );
    // Next observation settles both flights on their independent proofs,
    // but the sweep now publishes the mover's frame and the lie surfaces
    // as a write attempt the read-back refuses. Still never Applied.
    let observed = live_visible_observed();
    let held = live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        lying_prims(),
        0,
        None,
    );
    assert!(
        held.reply.transfers.is_empty(),
        "visibility proofs settle: {:?}",
        held.reply.transfers
    );
    assert!(
        held.members.is_empty() && held.views.is_empty(),
        "clamp holds without attempts: {:?} {:?}",
        held.members,
        held.views
    );
    assert_eq!(
        set_calls(),
        calls_before + 2,
        "sibling re-tile plus one mover attempt, then clamp-held"
    );
    let live_frame = S_OBSERVE.with(|m| m.borrow().get(&mover.hwnd).map(|w| w.visible));
    let published = held
        .reply
        .windows
        .iter()
        .find(|w| w.lifetime.0 == "w1")
        .and_then(|w| w.effective_frame);
    assert_ne!(
        live_frame, published,
        "held size keeps the live frame off the published slot"
    );
    // A third tick attempts nothing further: the clamp is recorded once
    // and later identical mismatches skip silently.
    let observed = live_visible_observed();
    let quiet = live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        lying_prims(),
        0,
        None,
    );
    assert!(quiet.reply.transfers.is_empty());
    assert!(
        quiet.members.is_empty() && quiet.views.is_empty(),
        "clamp recorded once: {:?} {:?}",
        quiet.members,
        quiet.views
    );
    assert_eq!(set_calls(), calls_before + 2);
}

/// Partial batch: the sibling hide leg refuses mid-batch while the mover
/// row is frameless. Both transfers hold ambiguous exactly once, nothing
/// moves, and nothing ever proves: later ticks hold silently with both
/// flights retained and everything static.
#[test]
fn same_output_follow_partial_sibling_hide_holds_view_unproven() {
    let mut env = env_with(202_031);
    let out = env.state.output_token(DEV1);
    let (mover, sibling) = admit_follow_pair(&mut env, 202_031);
    S_HIDE.with(|m| {
        m.borrow_mut()
            .insert(sibling.hwnd, Err("refused: scripted-partial".to_owned()))
    });
    let mut owner = WorldOwner::bootstrap_ids(ADAPTER, GENERATION).expect("owner");
    let areas = one_area();
    let fulls = env.fulls.clone();
    let observed = live_visible_observed();
    live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        0,
        None,
    );
    let (current, target, index) = follow_target(&owner, &out);
    let calls_before = set_calls();
    let observed = live_visible_observed();
    let tick = live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        mover.hwnd,
        Some(WorldCommand::SendNumbered {
            window: WindowId("w1".to_owned()),
            output: OutputId(out.clone()),
            index,
            follow: true,
        }),
    );
    let op = window_op(&tick.reply, "w1");
    let view_op = tick
        .reply
        .transfers
        .iter()
        .find(|t| matches!(t.kind, TransferKind::View))
        .expect("follow retains its view flight")
        .operation_id
        .clone();
    // Both transfers report ambiguous attempts exactly once: the refused
    // hide leg proves nothing, so the member attempt and the view attempt
    // each ride once and the spent phases stay silent after.
    assert!(
        has_ambiguous_member(&tick, &op, &out, &target),
        "partial holds ambiguous: {:?} {:?}",
        tick.members,
        tick.views
    );
    assert!(
        tick.views
            .iter()
            .any(|a| a.operation_id == view_op && a.post.as_deref() != Some(target.as_str())),
        "thrown switch leg holds ambiguous: {:?}",
        tick.views
    );
    assert!(!applied_of_members(&tick.members, &tick.views, &op));
    assert!(S_VISIBLE.with(|m| m.borrow().contains(&mover.hwnd)));
    assert!(
        S_VISIBLE.with(|m| m.borrow().contains(&sibling.hwnd)),
        "refused hide leaves the sibling visible"
    );
    // Truthful split: the refused hide leaves everything unproven, so both
    // applied maps stay at their bootstrap values and nothing moves: the
    // frameless mover row takes no writes and the sibling still matches.
    assert_eq!(
        env.state.applied_homing.get("w1"),
        Some(&(out.clone(), current.clone()))
    );
    assert_eq!(env.state.applied_views.get(&out), Some(&current));
    assert_eq!(
        set_calls(),
        calls_before + 1,
        "source collapse re-tiles the sibling"
    );
    // The carried ambiguity marks both flights held, so later ticks
    // hold silently: no re-attempt, no replay, no proof. The spent phases
    // stay silent too. Both flights stay retained with everything static.
    for _ in 0..2 {
        let observed = live_visible_observed();
        let held = live_tick(
            &mut env,
            &mut owner,
            &observed,
            &[],
            &areas,
            &fulls,
            moving_prims(),
            0,
            None,
        );
        assert!(
            held.members.is_empty() && held.views.is_empty(),
            "held flights stay silent: {:?} {:?}",
            held.members,
            held.views
        );
        assert_eq!(
            held.reply
                .transfers
                .iter()
                .filter(|t| t.operation_id == op || t.operation_id == view_op)
                .count(),
            2,
            "both flights retained: {:?}",
            held.reply.transfers
        );
        assert_eq!(
            env.state.applied_homing.get("w1"),
            Some(&(out.clone(), current.clone()))
        );
        assert_eq!(env.state.applied_views.get(&out), Some(&current));
        assert!(S_VISIBLE.with(|m| m.borrow().contains(&mover.hwnd)));
        assert!(S_VISIBLE.with(|m| m.borrow().contains(&sibling.hwnd)));
        assert_eq!(
            set_calls(),
            calls_before + 1,
            "deferred frames take no writes"
        );
    }
}

/// Gap refusal: a busy native hook stops the pass before any setter runs.
/// No attempt is made, so no phase is spent and the feedback is empty; the
/// commit still stands. Clearing the gap lets the next tick run the full
/// attempt cleanly, prove both flights, and settle with no replay.
#[test]
fn same_output_follow_hook_busy_stops_then_recovers_without_replay() {
    let mut env = env_with(202_041);
    let out = env.state.output_token(DEV1);
    let (mover, sibling) = admit_follow_pair(&mut env, 202_041);
    let mut owner = WorldOwner::bootstrap_ids(ADAPTER, GENERATION).expect("owner");
    let areas = one_area();
    let fulls = env.fulls.clone();
    let observed = live_visible_observed();
    live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        0,
        None,
    );
    let (current, target, index) = follow_target(&owner, &out);
    S_HOOK.with(|m| *m.borrow_mut() = true);
    let observed = live_visible_observed();
    let tick = live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        mover.hwnd,
        Some(WorldCommand::SendNumbered {
            window: WindowId("w1".to_owned()),
            output: OutputId(out.clone()),
            index,
            follow: true,
        }),
    );
    let op = window_op(&tick.reply, "w1");
    let view_op = tick
        .reply
        .transfers
        .iter()
        .find(|t| matches!(t.kind, TransferKind::View))
        .expect("follow retains its view flight")
        .operation_id
        .clone();
    // The busy hook stops the pass before any setter: no attempt runs, so
    // nothing is spent and the attempt rows are empty. The commit still
    // stands (both flights retained, target view published).
    assert!(
        tick.members.is_empty() && tick.views.is_empty(),
        "busy native stops the pass: {:?} {:?}",
        tick.members,
        tick.views
    );
    assert!(
        !applied_of_members(&tick.members, &tick.views, &op),
        "nothing proves on refusal: {:?} {:?}",
        tick.members,
        tick.views
    );
    assert!(S_VISIBLE.with(|m| m.borrow().contains(&mover.hwnd)));
    assert!(S_VISIBLE.with(|m| m.borrow().contains(&sibling.hwnd)));
    assert_eq!(env.state.applied_views.get(&out), Some(&current));
    assert_eq!(
        env.state.applied_homing.get("w1"),
        Some(&(out.clone(), current.clone()))
    );
    // Gap cleared: nothing was spent, so the next tick runs the full
    // two-phase proof cleanly (window ambiguous pre-switch, view proves),
    // then the following observation settles with no replay.
    S_HOOK.with(|m| *m.borrow_mut() = false);
    let observed = live_visible_observed();
    let recovered = live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        0,
        None,
    );
    assert!(
        has_ambiguous_member(&recovered, &op, &out, &target),
        "clean two-phase proof: {:?} {:?}",
        recovered.members,
        recovered.views
    );
    assert!(
        has_proven_view(&recovered, &view_op, &target),
        "recovery proves the view: {:?}",
        recovered.views
    );
    assert_eq!(
        env.state.applied_homing.get("w1"),
        Some(&(out.clone(), target.clone()))
    );
    assert_eq!(env.state.applied_views.get(&out), Some(&target));
    assert!(
        !S_VISIBLE.with(|m| m.borrow().contains(&sibling.hwnd)),
        "sibling hides on recovery"
    );
    let observed = live_visible_observed();
    let settled = live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        0,
        None,
    );
    assert!(
        settled.reply.transfers.is_empty(),
        "recovery settles: {:?}",
        settled.reply.transfers
    );
    assert!(
        settled.members.is_empty() && settled.views.is_empty(),
        "no replay: {:?} {:?}",
        settled.members,
        settled.views
    );
}

/// Iconic-visible mover follow (minimize/restore race: the native iconic
/// flag reads while the window still enumerates visible). The logical view
/// still switches and the iconic arrival proof still merges homing
/// truthfully; no frame is ever written to the iconic mover, and the mover
/// focus effect never dispatches (held for the unproven view, and the
/// foreground gate could never prove an iconic subject). The view stays
/// applied-source while iconic membership vetoes its proof.
#[test]
fn same_output_follow_iconic_mover_proves_homing_without_writes_or_focus() {
    fn focus_seeded(hwnd: u64) -> bool {
        S_FOCUS_OK.with(|m| m.borrow().contains_key(&hwnd))
    }
    fn set_calls_for(hwnd: u64) -> usize {
        S_SET_CALLS.with(|m| m.borrow().iter().filter(|(h, _)| *h == hwnd).count())
    }
    let mut env = env_with(202_051);
    let out = env.state.output_token(DEV1);
    let (mover, sibling) = admit_follow_pair(&mut env, 202_051);
    let mut owner = WorldOwner::bootstrap_ids(ADAPTER, GENERATION).expect("owner");
    let areas = one_area();
    let fulls = env.fulls.clone();
    let observed = live_visible_observed();
    live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        0,
        None,
    );
    let (current, target, index) = follow_target(&owner, &out);
    // Native iconic flag lands while the mover still enumerates visible.
    S_ICONIC.with(|m| m.borrow_mut().insert(mover.hwnd));
    let mover_calls_before = set_calls_for(mover.hwnd);
    let observed = live_visible_observed();
    assert_eq!(observed.len(), 2, "iconic mover still enumerates");
    let tick = live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        mover.hwnd,
        Some(WorldCommand::SendNumbered {
            window: WindowId("w1".to_owned()),
            output: OutputId(out.clone()),
            index,
            follow: true,
        }),
    );
    // Logical view switches with the commit, and the reply names mover
    // focus honestly (never a fallback); nativeness proves separately.
    assert_eq!(
        published_view(&tick.reply, &out).as_deref(),
        Some(target.as_str())
    );
    assert!(
        tick.reply.effects.iter().any(|e| e.target.0 == "w1"),
        "follow names mover focus: {:?}",
        tick.reply.effects
    );
    let op = window_op(&tick.reply, "w1");
    let view_op = tick
        .reply
        .transfers
        .iter()
        .find(|t| matches!(t.kind, TransferKind::View))
        .expect("follow retains its view flight")
        .operation_id
        .clone();
    // The iconic member vetoes both proofs: no member row is dispatched
    // under the iconic veto, while the follow phase and the retained
    // follow view each report the missed switch with unreadable proof.
    // Nothing fabricates arrival.
    assert!(tick.members.iter().all(|a| a.operation_id != op));
    assert!(
        tick.views
            .iter()
            .any(|a| a.operation_id == op && a.post.as_deref() != Some(target.as_str())),
        "unproven follow view: {:?}",
        tick.views
    );
    assert!(
        tick.views
            .iter()
            .any(|a| a.operation_id == view_op && a.post.as_deref() != Some(target.as_str())),
        "iconic member vetoes the view proof: {:?}",
        tick.views
    );
    assert!(
        !applied_of_members(&tick.members, &tick.views, &op),
        "nothing fabricates arrival: {:?} {:?}",
        tick.members,
        tick.views
    );
    // The switch still runs (sibling hides, mover stays visible), the
    // iconic arrival proof merges homing truthfully, and the view stays
    // applied-source under the iconic veto.
    assert!(S_VISIBLE.with(|m| m.borrow().contains(&mover.hwnd)));
    assert!(!S_VISIBLE.with(|m| m.borrow().contains(&sibling.hwnd)));
    assert_eq!(
        env.state.applied_homing.get("w1"),
        Some(&(out.clone(), target.clone()))
    );
    assert_eq!(
        env.state.applied_homing.get("w2"),
        Some(&(out.clone(), current.clone()))
    );
    assert_eq!(env.state.applied_views.get(&out), Some(&current));
    // No frame write ever names the iconic mover (reveal short-circuits,
    // frameless rows take no writes); the sibling re-tile is the only
    // setter call. Focus never dispatches for the unproven view, so the
    // foreground gate sees nothing to fake.
    assert_eq!(
        set_calls_for(mover.hwnd),
        mover_calls_before,
        "no iconic frame write"
    );
    assert!(!focus_seeded(mover.hwnd), "focus never dispatched");
    // The mover minimizes for real: omitted, invisible, still iconic. The
    // minimized fact retains homing with no geometry lane, so still no
    // writes name it; receipts merge nothing beyond evidence and both
    // flights hold silently.
    S_VISIBLE.with(|m| m.borrow_mut().remove(&mover.hwnd));
    env.state.minimized_omitted.insert(mover.clone());
    let observed = live_visible_observed();
    assert!(observed.is_empty(), "mover omitted, sibling hidden");
    let held = live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        0,
        None,
    );
    assert!(
        held.members.is_empty() && held.views.is_empty(),
        "held flights stay silent: {:?} {:?}",
        held.members,
        held.views
    );
    assert_eq!(
        held.reply
            .transfers
            .iter()
            .filter(|t| t.operation_id == op || t.operation_id == view_op)
            .count(),
        2,
        "both flights retained: {:?}",
        held.reply.transfers
    );
    assert_eq!(
        env.state.applied_homing.get("w1"),
        Some(&(out.clone(), target.clone()))
    );
    assert_eq!(env.state.applied_views.get(&out), Some(&current));
    assert_eq!(
        set_calls_for(mover.hwnd),
        mover_calls_before,
        "still no iconic frame write"
    );
    assert!(!focus_seeded(mover.hwnd), "still no focus dispatch");
    assert!(!S_VISIBLE.with(|m| m.borrow().contains(&sibling.hwnd)));
}

#[test]
fn same_output_send_stay_hides_and_settles() {
    let mut env = env_with(203_001);
    let out = env.state.output_token(DEV1);
    let key = win_key(203_001, 2203, "proc-103");
    admit_bare(&mut env, &key, "w1");
    script_live(&key, "w1", rect(0, 0, 800, 600), plain_facts());
    S_ADMIT.with(|m| {
        m.borrow_mut()
            .insert(key.hwnd, Ok(script_claim(key.hwnd, &identity_of(&key))))
    });
    S_HIDE.with(|m| {
        m.borrow_mut()
            .insert(key.hwnd, Ok(valid_claim(key.hwnd, &identity_of(&key))))
    });
    let mut owner = WorldOwner::bootstrap_ids(ADAPTER, GENERATION).expect("owner");
    let areas = one_area();
    let fulls = env.fulls.clone();
    let observed = live_visible_observed();
    live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        0,
        None,
    );
    let current = owner.engine().logical_view(&out).expect("current view");
    let target = second_workspace(&owner, &out, &current);
    // Stay send: the mover leaves the visible set; the view never follows.
    let observed = live_visible_observed();
    let tick = live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        key.hwnd,
        Some(WorldCommand::Send {
            window: WindowId("w1".to_owned()),
            target_output: OutputId(out.clone()),
            target_workspace: target.clone(),
            follow: false,
        }),
    );
    let op = window_op(&tick.reply, "w1");
    assert!(
        applied_of_members(&tick.members, &tick.views, &op),
        "stay homing proves: {:?} {:?}",
        tick.members,
        tick.views
    );
    assert_eq!(
        env.state.applied_homing.get("w1"),
        Some(&(out.clone(), target.clone())),
        "stay homing proves through the hide"
    );
    assert!(
        !S_VISIBLE.with(|m| m.borrow().contains(&key.hwnd)),
        "stayed window reads invisible"
    );
    assert_eq!(
        env.state.applied_views.get(&out),
        Some(&current),
        "stay never moves the view"
    );
    // Next report settles on the hidden receipt.
    let observed = live_visible_observed();
    assert!(observed.is_empty(), "nothing visible while stayed away");
    let settled = live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        0,
        None,
    );
    assert!(
        settled.reply.transfers.is_empty(),
        "stay settles: {:?}",
        settled.reply.transfers
    );
}

/// Live leg1 prefix as a production-seam regression: bootstrap two source
/// tiles plus a sticky anchor, fresh-trailing FOLLOW, observed convergence,
/// select-back, then STAY to trailing. The stay mover must hide through the
/// per-member `move_membership` path (stay retains no view flight by design)
/// while the followed-away tile stays hidden with its homing retained, and
/// the source view never moves. Receipts prove only through the scripted
/// desktop (`S_VISIBLE` flips on hide, frames move on write): the assertions
/// below read those independent readbacks, never setter echoes.
#[test]
fn composed_follow_selectback_stay_hides_and_settles() {
    let mut env = env_with(203_101);
    let out = env.state.output_token(DEV1);
    let n1 = win_key(203_101, 2311, "proc-311");
    let n2 = win_key(203_102, 2312, "proc-312");
    let anchor = win_key(203_103, 2313, "proc-313");
    for (key, token) in [(&n1, "w1"), (&n2, "w2"), (&anchor, "w0")] {
        admit_bare(&mut env, key, token);
        script_live(key, token, rect(0, 0, 800, 600), plain_facts());
        S_ADMIT.with(|m| {
            m.borrow_mut()
                .insert(key.hwnd, Ok(script_claim(key.hwnd, &identity_of(key))))
        });
        S_HIDE.with(|m| {
            m.borrow_mut()
                .insert(key.hwnd, Ok(valid_claim(key.hwnd, &identity_of(key))))
        });
        S_REVEAL.with(|m| m.borrow_mut().insert(key.hwnd, Ok(Revealed::Shown)));
    }
    env.state.sticky.insert(anchor.clone(), false);
    let mut owner = WorldOwner::bootstrap_ids(ADAPTER, GENERATION).expect("owner");
    let areas = one_area();
    let fulls = env.fulls.clone();
    // Bootstrap: all three prove homing; the anchor never leaves.
    let observed = live_visible_observed();
    live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        0,
        None,
    );
    let current = owner.engine().logical_view(&out).expect("current view");
    // Fresh trailing FOLLOW of n1 (leg1 `send0-fresh-o0`): mover visible on
    // the trailing view, sibling hidden, anchor held.
    let observed = live_visible_observed();
    let follow = live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        n1.hwnd,
        Some(WorldCommand::SendNumbered {
            window: WindowId("w1".to_owned()),
            output: OutputId(out.clone()),
            index: 0,
            follow: true,
        }),
    );
    let follow_op = window_op(&follow.reply, "w1");
    let trail1 = owner
        .engine()
        .logical_view(&out)
        .expect("follow commits the trailing view");
    assert_ne!(trail1, current, "follow moves the view: {trail1:?}");
    assert!(
        S_VISIBLE.with(|m| m.borrow().contains(&n1.hwnd)),
        "mover visible"
    );
    assert!(
        !S_VISIBLE.with(|m| m.borrow().contains(&n2.hwnd)),
        "sibling hidden: {:?} {:?}",
        follow.members,
        follow.views
    );
    assert!(
        S_VISIBLE.with(|m| m.borrow().contains(&anchor.hwnd)),
        "sticky anchor holds"
    );
    // Observed convergence: the follow settles with no replay.
    let observed = live_visible_observed();
    assert_eq!(observed.len(), 2, "mover plus anchor read visible");
    let converged = live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        0,
        None,
    );
    assert!(
        converged.reply.transfers.is_empty(),
        "follow settles: {:?}",
        converged.reply.transfers
    );
    assert!(
        !has_any_attempt(&converged, &follow_op),
        "no replay: {:?} {:?}",
        converged.members,
        converged.views
    );
    assert_eq!(
        env.state.applied_homing.get("w1"),
        Some(&(out.clone(), trail1.clone())),
        "followed-away tile retains its hidden-target homing"
    );
    // Select back to the source view (leg1 `sel1-o0`): n1 hidden, n2 back.
    let ring = owner.engine().logical_ring(&out);
    let back_index = ring
        .iter()
        .position(|id| id == &current)
        .expect("source in ring") as u8
        + 1;
    let observed = live_visible_observed();
    live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        n1.hwnd,
        Some(WorldCommand::SelectNumbered {
            output: Some(OutputId(out.clone())),
            index: back_index,
        }),
    );
    assert!(
        !S_VISIBLE.with(|m| m.borrow().contains(&n1.hwnd)),
        "followed-away tile hidden after select-back"
    );
    assert!(
        S_VISIBLE.with(|m| m.borrow().contains(&n2.hwnd)),
        "sibling back"
    );
    let observed = live_visible_observed();
    let reseated = live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        0,
        None,
    );
    assert!(
        reseated.reply.transfers.is_empty(),
        "select-back settles: {:?}",
        reseated.reply.transfers
    );
    assert_eq!(
        env.state.applied_views.get(&out),
        Some(&current),
        "source view restored"
    );
    // STAY n2 to trailing (leg1 `stay0-o0`): the mover leaves the visible
    // set through the per-member hide path; the view never follows and the
    // hidden n1 keeps its homing.
    let observed = live_visible_observed();
    let stay = live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        n2.hwnd,
        Some(WorldCommand::SendNumbered {
            window: WindowId("w2".to_owned()),
            output: OutputId(out.clone()),
            index: 0,
            follow: false,
        }),
    );
    let stay_op = window_op(&stay.reply, "w2");
    assert!(
        applied_of_members(&stay.members, &stay.views, &stay_op),
        "stay transfer applies: {:?} {:?}",
        stay.members,
        stay.views
    );
    assert!(
        !S_VISIBLE.with(|m| m.borrow().contains(&n2.hwnd)),
        "stayed window reads invisible"
    );
    assert!(
        !S_VISIBLE.with(|m| m.borrow().contains(&n1.hwnd)),
        "followed-away tile stays hidden"
    );
    assert!(
        S_VISIBLE.with(|m| m.borrow().contains(&anchor.hwnd)),
        "anchor holds through stay"
    );
    let stay_home = env
        .state
        .applied_homing
        .get("w2")
        .expect("stay homing proves through the hide")
        .clone();
    assert_ne!(stay_home.1, current, "stay leaves the source view");
    assert_eq!(
        env.state.applied_views.get(&out),
        Some(&current),
        "stay never moves the view"
    );
    assert_eq!(
        env.state.applied_homing.get("w1"),
        Some(&(out.clone(), trail1.clone())),
        "hidden target member retains its homing"
    );
    // Next report settles on the hidden receipts.
    let observed = live_visible_observed();
    assert_eq!(observed.len(), 1, "only the anchor reads visible");
    assert_eq!(observed[0].hwnd, anchor.hwnd);
    let settled = live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        0,
        None,
    );
    assert!(
        settled.reply.transfers.is_empty(),
        "stay settles: {:?}",
        settled.reply.transfers
    );
    assert_eq!(
        env.state.applied_homing.get("w2"),
        Some(&stay_home),
        "stay homing retained"
    );
}

/// Live stage2b return leg as a production-seam regression (stay-fix
/// follow-up): after follow/select-back/stay settles, select back to the
/// follow domain (reveals n1), follow-send n1 home, then select into the
/// stay domain whose sole member is still hidden. The select must reveal
/// n2 through the batch path and settle with no unreadable hold and no
/// manual rescue. Drives the real owner plus the real actuator with a
/// scripted desktop; assertions read independent readbacks only.
#[test]
fn composed_stay_return_select_reveals_hidden_and_settles() {
    let mut env = env_with(203_201);
    let out = env.state.output_token(DEV1);
    let n1 = win_key(203_201, 2321, "proc-321");
    let n2 = win_key(203_202, 2322, "proc-322");
    let anchor = win_key(203_203, 2323, "proc-323");
    for (key, token) in [(&n1, "w1"), (&n2, "w2"), (&anchor, "w0")] {
        admit_bare(&mut env, key, token);
        script_live(key, token, rect(0, 0, 800, 600), plain_facts());
        S_ADMIT.with(|m| {
            m.borrow_mut()
                .insert(key.hwnd, Ok(script_claim(key.hwnd, &identity_of(key))))
        });
        S_HIDE.with(|m| {
            m.borrow_mut()
                .insert(key.hwnd, Ok(valid_claim(key.hwnd, &identity_of(key))))
        });
        S_REVEAL.with(|m| m.borrow_mut().insert(key.hwnd, Ok(Revealed::Shown)));
    }
    env.state.sticky.insert(anchor.clone(), false);
    let mut owner = WorldOwner::bootstrap_ids(ADAPTER, GENERATION).expect("owner");
    let areas = one_area();
    let fulls = env.fulls.clone();
    let index_of = |owner: &WorldOwner, id: &str| {
        owner
            .engine()
            .logical_ring(&out)
            .iter()
            .position(|w| w == id)
            .expect("in ring") as u8
            + 1
    };
    // Bootstrap.
    let observed = live_visible_observed();
    live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        0,
        None,
    );
    let current = owner.engine().logical_view(&out).expect("current view");
    // Follow n1 to trailing.
    let observed = live_visible_observed();
    let follow = live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        n1.hwnd,
        Some(WorldCommand::SendNumbered {
            window: WindowId("w1".to_owned()),
            output: OutputId(out.clone()),
            index: 0,
            follow: true,
        }),
    );
    let trail1 = owner.engine().logical_view(&out).expect("trailing view");
    assert_ne!(trail1, current);
    let follow_op = window_op(&follow.reply, "w1");
    let observed = live_visible_observed();
    let converged = live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        0,
        None,
    );
    assert!(
        converged.reply.transfers.is_empty(),
        "follow settles: {:?}",
        converged.reply.transfers
    );
    assert!(
        !has_any_attempt(&converged, &follow_op),
        "no replay: {:?} {:?}",
        converged.members,
        converged.views
    );
    assert_all_controlled("follow-settle", &converged.reply);
    // Select back to source: n1 hidden, n2 back.
    let observed = live_visible_observed();
    let back_index = index_of(&owner, &current);
    live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        n1.hwnd,
        Some(WorldCommand::SelectNumbered {
            output: Some(OutputId(out.clone())),
            index: back_index,
        }),
    );
    assert!(
        !S_VISIBLE.with(|m| m.borrow().contains(&n1.hwnd)),
        "n1 hidden"
    );
    assert!(S_VISIBLE.with(|m| m.borrow().contains(&n2.hwnd)), "n2 back");
    let observed = live_visible_observed();
    let reseated = live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        0,
        None,
    );
    assert!(
        reseated.reply.transfers.is_empty(),
        "select-back settles: {:?}",
        reseated.reply.transfers
    );
    assert_all_controlled("select-back-settle", &reseated.reply);
    // Stay n2 to trailing: hides, applies, view stays.
    let observed = live_visible_observed();
    let stay = live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        n2.hwnd,
        Some(WorldCommand::SendNumbered {
            window: WindowId("w2".to_owned()),
            output: OutputId(out.clone()),
            index: 0,
            follow: false,
        }),
    );
    let stay_op = window_op(&stay.reply, "w2");
    assert!(
        applied_of_members(&stay.members, &stay.views, &stay_op),
        "stay applies: {:?} {:?}",
        stay.members,
        stay.views
    );
    assert!(
        !S_VISIBLE.with(|m| m.borrow().contains(&n2.hwnd)),
        "n2 hidden by stay"
    );
    let stay_home = env
        .state
        .applied_homing
        .get("w2")
        .expect("stay homing proves")
        .clone();
    assert_ne!(stay_home.1, current);
    let observed = live_visible_observed();
    let settled = live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        0,
        None,
    );
    assert!(
        settled.reply.transfers.is_empty(),
        "stay settles: {:?}",
        settled.reply.transfers
    );
    assert_all_controlled("stay-settle", &settled.reply);
    // Return leg 1: select back to the follow domain (reveals n1).
    let observed = live_visible_observed();
    let trail_index = index_of(&owner, &trail1);
    let back = live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        n1.hwnd,
        Some(WorldCommand::SelectNumbered {
            output: Some(OutputId(out.clone())),
            index: trail_index,
        }),
    );
    assert_eq!(
        owner.engine().logical_view(&out).as_deref(),
        Some(trail1.as_str())
    );
    assert!(
        S_VISIBLE.with(|m| m.borrow().contains(&n1.hwnd)),
        "n1 revealed: {:?} {:?}",
        back.members,
        back.views
    );
    assert!(
        !S_VISIBLE.with(|m| m.borrow().contains(&n2.hwnd)),
        "n2 stays hidden"
    );
    let observed = live_visible_observed();
    let reseated = live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        0,
        None,
    );
    assert!(
        reseated.reply.transfers.is_empty(),
        "return select settles: {:?}",
        reseated.reply.transfers
    );
    assert_all_controlled("return-select-settle", &reseated.reply);
    // Sendback: follow-send n1 home to the source view.
    let observed = live_visible_observed();
    let home_index = index_of(&owner, &current);
    let sendback = live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        n1.hwnd,
        Some(WorldCommand::SendNumbered {
            window: WindowId("w1".to_owned()),
            output: OutputId(out.clone()),
            index: home_index,
            follow: true,
        }),
    );
    let sendback_op = window_op(&sendback.reply, "w1");
    // Pre-proven hidden arrival: no member or view setter runs, so no raw
    // attempt rides. Settlement happens on the next report from the staged
    // hidden receipt; the transfer is retained meanwhile.
    assert!(sendback.members.is_empty() && sendback.views.is_empty());
    assert!(
        sendback
            .reply
            .transfers
            .iter()
            .any(|t| t.operation_id == sendback_op),
        "sendback retained for settlement: {:?}",
        sendback.reply.transfers
    );
    assert!(
        S_VISIBLE.with(|m| m.borrow().contains(&n1.hwnd)),
        "n1 visible after sendback"
    );
    assert_eq!(
        env.state.applied_homing.get("w1"),
        Some(&(out.clone(), current.clone())),
        "n1 homing home"
    );
    let observed = live_visible_observed();
    let reseated = live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        0,
        None,
    );
    assert!(
        reseated.reply.transfers.is_empty(),
        "sendback settles: {:?}",
        reseated.reply.transfers
    );
    assert_all_controlled("sendback-settle", &reseated.reply);
    // Select into the stay domain: sole hidden n2 must reveal and settle.
    let observed = live_visible_observed();
    let stay_index = index_of(&owner, &stay_home.1);
    let select = live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        n1.hwnd,
        Some(WorldCommand::SelectNumbered {
            output: Some(OutputId(out.clone())),
            index: stay_index,
        }),
    );
    let scope = format!("{out}/{}", stay_home.1);
    assert!(
        select
            .reply
            .scopes
            .iter()
            .all(|s| s.scope != scope || s.authority == Auth::Controlled),
        "stay domain stays controlled, got {:?}",
        select.reply.scopes
    );
    assert!(
        S_VISIBLE.with(|m| m.borrow().contains(&n2.hwnd)),
        "n2 revealed by select into stay domain: scopes {:?} members {:?} views {:?}",
        select.reply.scopes,
        select.members,
        select.views
    );
    assert_eq!(
        owner.engine().logical_view(&out).as_deref(),
        Some(stay_home.1.as_str())
    );
    let observed = live_visible_observed();
    let reseated = live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        0,
        None,
    );
    assert!(
        reseated.reply.transfers.is_empty(),
        "stay select settles: {:?}",
        reseated.reply.transfers
    );
}

fn fake_backed_visible(hwnd: u64) -> bool {
    crate::product_hide::sys::fake::with(hwnd, |w| w.alive && w.visible).unwrap_or(false)
}

fn fake_backed_iconic(hwnd: u64) -> bool {
    crate::product_hide::sys::fake::with(hwnd, |w| w.alive && w.iconic).unwrap_or(false)
}

fn fake_backed_pid(hwnd: u64) -> u32 {
    crate::product_hide::sys::fake::with(hwnd, |w| if w.alive { w.pid } else { 0 }).unwrap_or(0)
}

fn fake_backed_identity(hwnd: u64) -> Option<ProcessIdentity> {
    crate::product_hide::sys::fake::with(hwnd, |w| {
        if w.alive {
            Some(w.process.clone())
        } else {
            None
        }
    })
    .flatten()
}

/// Production claim lanes with scripted leaves: the real admit/hide/reveal
/// plus ledger and nonce lifecycle over the `fake` backend, while frames,
/// visibility proofs, and identity reads ride the scripted desktop. This is
/// the narrow seam the scripted `S_ADMIT`/`S_HIDE` maps bypass.
fn cycle_prims() -> NativePrimitives {
    let real = NativePrimitives::default();
    NativePrimitives {
        observe: s_observe,
        window_pid: fake_backed_pid,
        is_visible: fake_backed_visible,
        is_iconic: fake_backed_iconic,
        set_outer: s_set_outer_moving,
        foreground: || S_FG.with(|m| *m.borrow()),
        focus: |hwnd| S_FOCUS_OK.with(|m| m.borrow().get(&hwnd).copied().unwrap_or(true)),
        member_tag: crate::product_hide::sys::read_member_tag,
        hook_pending: || false,
        read_cursor: || None,
        warp_cursor: |_, _| true,
        dpi: s_dpi,
        process_identity: fake_backed_identity,
        read_topmost: |_| false,
        admit: real.admit,
        hide_claim: real.hide_claim,
        reveal_claim: real.reveal_claim,
    }
}

/// Live leg1 stay shape at the real claim seam: hide, reveal, then hide the
/// same window again through the production admit plus ledger claim. The
/// second hide must re-admit cleanly (no already-claimed refusal, no
/// duplicate ledger note) with receipts proving through fresh readbacks.
#[test]
fn second_hide_after_reveal_readmits_through_real_claims() {
    use crate::model::LEDGER_SCHEMA_VERSION;
    use crate::product_hide::sys::fake::{self, FakeWindow};
    let mut env = env_with(203_301);
    let out = env.state.output_token(DEV1);
    let ws1 = "ws-1".to_owned();
    let ws2 = "ws-2".to_owned();
    let key = win_key(203_201, 2321, "proc-321");
    admit_bare(&mut env, &key, "w1");
    script_live(&key, "w1", rect(0, 0, 800, 600), plain_facts());
    env.state
        .applied_homing
        .insert("w1".to_owned(), (out.clone(), ws1.clone()));
    env.state.applied_views.insert(out.clone(), ws1.clone());
    env.store
        .commit(&crate::model::RecoveryLedger {
            v: LEDGER_SCHEMA_VERSION,
            owner: env.me.clone(),
            windows: Vec::new(),
            mouse_snap: None,
        })
        .expect("seed owner ledger");
    fake::register(
        key.hwnd,
        FakeWindow {
            pid: key.pid,
            process: identity_of(&key),
            integrity_rid: 0x2000,
            nonce: None,
            member_tag: Some("member-tag-1".to_owned()),
            visible: true,
            iconic: false,
            maximized: false,
            alive: true,
            post_ok: true,
            defer: false,
        },
    );
    let mut native = assemble(&mut env, cycle_prims());
    // Stay target names a ring workspace, like the core projection would.
    native
        .core_rings
        .entry(out.clone())
        .or_default()
        .push(ws2.clone());
    native
        .core_homing
        .insert("w1".to_owned(), (out.clone(), ws2.clone()));
    // First hide through the real admit plus ledger claim.
    assert_eq!(native.move_membership("w1", &out, &ws2, None), Ok(true));
    assert!(!fake_backed_visible(key.hwnd), "first hide conceals");
    assert_eq!(
        native.read_homing("w1"),
        Some((out.clone(), ws2.clone())),
        "first hide proves through the ledger plus invisibility"
    );
    assert!(native.hidden_claims.contains_key(&key));
    native
        .proven_homing
        .insert("w1".to_owned(), (out.clone(), ws2.clone()));
    // Reveal back through the real claim teardown.
    native
        .core_homing
        .insert("w1".to_owned(), (out.clone(), ws1.clone()));
    assert_eq!(native.move_membership("w1", &out, &ws1, None), Ok(true));
    assert!(fake_backed_visible(key.hwnd), "reveal shows");
    assert!(!native.hidden_claims.contains_key(&key));
    assert_eq!(
        native.read_homing("w1"),
        Some((out.clone(), ws1.clone())),
        "reveal re-proves the source home"
    );
    native
        .proven_homing
        .insert("w1".to_owned(), (out.clone(), ws1.clone()));
    // Second hide must re-admit cleanly: no already-claimed refusal, no
    // duplicate ledger note, receipts from fresh readbacks only.
    native
        .core_homing
        .insert("w1".to_owned(), (out.clone(), ws2.clone()));
    assert_eq!(
        native.move_membership("w1", &out, &ws2, None),
        Ok(true),
        "second hide re-admits: nonce={:?}",
        fake::with(key.hwnd, |w| w.nonce.clone())
    );
    assert!(!fake_backed_visible(key.hwnd), "second hide conceals");
    assert_eq!(
        native.read_homing("w1"),
        Some((out.clone(), ws2.clone())),
        "second hide proves through the fresh claim"
    );
    fake::unregister_all();
}

/// Unknown observation preimages never prove third-location supersession:
/// a missing receipt holds the transfer retained (and retries on the next
/// proven observation), while a known preimage that genuinely moved still
/// supersedes. The stay defect was a false supersede off an empty baseline.
#[test]
fn unknown_baseline_holds_transfer_without_supersession() {
    use super::super::super::world_diff::{FreshHost, FreshWindow};
    use std::collections::HashMap;
    reset_script();
    let mut state = test_state();
    state.ensure_device(DEV1);
    let out = state.output_token(DEV1);
    let key = win_key(203_301, 2331, "proc-331");
    let frame = rect(0, 0, 800, 600);
    fn host_for(key: &WindowKey, out: &str, workspace: &str, frame: Rect) -> FreshHost {
        let mut baseline = HashMap::new();
        baseline.insert(
            "w1".to_owned(),
            FreshWindow {
                key: key.clone(),
                token: "w1".to_owned(),
                frame: Some(frame),
                observed_frame: Some(frame),
                facts: Some(plain_facts()),
                output: out.to_owned(),
                workspace: workspace.to_owned(),
                iconic: false,
                hidden: false,
                sticky: false,
                floating: false,
                fit_excluded: false,
                live_tag: Some("member-tag-1".to_owned()),
            },
        );
        let mut stored_tags = HashMap::new();
        stored_tags.insert("w1".to_owned(), "member-tag-1".to_owned());
        FreshHost {
            baseline,
            stored_tags,
            views: HashMap::new(),
        }
    }
    fn reply_for(out: &str) -> WorldReply {
        let mut reply = reply_shell(7, 1, &[(out, "ws-1"), (out, "ws-2")]);
        reply.windows.push(WindowDesire {
            lifetime: WindowId("w1".to_owned()),
            output: OutputId(out.to_owned()),
            workspace: "ws-2".to_owned(),
            allocation: None,
            effective_frame: None,
            frame_authority: Auth::Untouched,
            membership_authority: Auth::Controlled,
            floating: false,
            float_origin: None,
            frame_operation: None,
            clear_operation: None,
        });
        reply.transfers.push(TransferTarget {
            operation_id: "op-9".to_owned(),
            kind: TransferKind::Window(WindowId("w1".to_owned())),
            target_output: OutputId(out.to_owned()),
            target_workspace: "ws-2".to_owned(),
            follow: false,
            held: false,
        });
        reply
    }
    // Unknown baseline ("") with live homing anywhere else: hold, never
    // report, and never settle the transfer. The core derives any genuine
    // supersession from staged observation; the transport reports nothing
    // off an unknown preimage.
    let mut diff = HostDiff::new(ADAPTER, GENERATION);
    diff.gate.current_obs = 7;
    let reply = reply_for(&out);
    let fresh = host_for(&key, &out, "", frame);
    let mut native = FakeNative::seeded(&fresh);
    native
        .windows
        .get_mut("w1")
        .expect("seeded mover")
        .workspace = "ws-X".to_owned();
    let mut members = Vec::new();
    let mut views = Vec::new();
    let stats = apply_world_reply(
        &mut diff,
        &reply,
        &fresh,
        &mut native,
        &mut Vec::new(),
        &mut members,
        &mut views,
        &mut Vec::new(),
        0,
    );
    assert!(
        members.is_empty() && views.is_empty(),
        "unknown preimage reports nothing"
    );
    assert!(stats.held > 0, "{stats:?}");
    // The retained transfer retries once the preimage proves: home at the
    // source actuates and reports one raw attempt.
    let fresh = host_for(&key, &out, "ws-X", frame);
    let mut members = Vec::new();
    let mut views = Vec::new();
    let stats = apply_world_reply(
        &mut diff,
        &reply,
        &fresh,
        &mut native,
        &mut Vec::new(),
        &mut members,
        &mut views,
        &mut Vec::new(),
        0,
    );
    assert_eq!(members.len(), 1);
    assert_eq!(members[0].operation_id, "op-9");
    assert_eq!(members[0].setter, FrameSetter::Accepted);
    assert_eq!(stats.membership_writes, 1, "{stats:?}");
    // A prior dispatch plus an unknown baseline still holds: the empty
    // preimage cannot contradict anything either.
    let mut diff = HostDiff::new(ADAPTER, GENERATION);
    diff.gate.current_obs = 7;
    diff.effects.mark_member_attempted("op-9", "w1");
    let fresh = host_for(&key, &out, "", frame);
    let mut native = FakeNative::seeded(&fresh);
    native
        .windows
        .get_mut("w1")
        .expect("seeded mover")
        .workspace = "ws-X".to_owned();
    let mut members = Vec::new();
    let mut views = Vec::new();
    let stats = apply_world_reply(
        &mut diff,
        &reply,
        &fresh,
        &mut native,
        &mut Vec::new(),
        &mut members,
        &mut views,
        &mut Vec::new(),
        0,
    );
    assert!(members.is_empty() && views.is_empty());
    assert!(stats.held > 0, "{stats:?}");
    // Genuine third location with a known preimage still holds without
    // dispatch: the core derives the supersession from staged
    // observation, never from a verdict here.
    let mut diff = HostDiff::new(ADAPTER, GENERATION);
    diff.gate.current_obs = 7;
    let fresh = host_for(&key, &out, "ws-1", frame);
    let mut native = FakeNative::seeded(&fresh);
    native
        .windows
        .get_mut("w1")
        .expect("seeded mover")
        .workspace = "ws-X".to_owned();
    let mut members = Vec::new();
    let mut views = Vec::new();
    let stats = apply_world_reply(
        &mut diff,
        &reply,
        &fresh,
        &mut native,
        &mut Vec::new(),
        &mut members,
        &mut views,
        &mut Vec::new(),
        0,
    );
    assert!(members.is_empty() && views.is_empty());
    assert!(stats.held > 0, "{stats:?}");
}

fn cross_output_target(owner: &WorldOwner, output: &str) -> String {
    // The output's own active view: arrival needs no view switch, so the
    // frame plus visibility proof alone settles the crossing.
    owner
        .engine()
        .logical_view(output)
        .expect("target output view")
}

/// Live stage3a-send topology (20261022-1640): asymmetric FULL/work
/// geometries with a y-offset second output. A blind FULL-delta
/// translation of an edge tile cannot land here, so the core must publish
/// the actionable target slot with the desire; the seam writes it as part
/// of the move and proves arrival by the exact frame plus affinity.
fn asymmetric_send_areas() -> (Vec<super::super::MonitorArea>, Vec<Rect>) {
    let areas = vec![
        super::super::MonitorArea {
            device: DEV1.to_owned(),
            work: rect(0, 0, 2560, 1380),
            full: rect(0, 0, 2560, 1440),
        },
        super::super::MonitorArea {
            device: DEV2.to_owned(),
            work: rect(2560, 355, 1920, 1032),
            full: rect(2560, 355, 1920, 1080),
        },
    ];
    let fulls = vec![rect(0, 0, 2560, 1440), rect(2560, 355, 1920, 1080)];
    (areas, fulls)
}

/// The published slot must sit fully inside the target work area: the
/// live defect stranded the mover beyond it via FULL-delta translation.
fn assert_slot_in_work(slot: Rect, work: Rect) {
    assert!(
        slot.x >= work.x
            && slot.y >= work.y
            && slot.x + slot.w <= work.x + work.w
            && slot.y + slot.h <= work.y + work.h,
        "slot {slot:?} inside work {work:?}"
    );
}

fn mover_slot(reply: &WorldReply, window: &str) -> Rect {
    let row = reply
        .windows
        .iter()
        .find(|w| w.lifetime.0 == window)
        .expect("mover row");
    assert_eq!(
        row.membership_authority,
        Auth::Controlled,
        "mover membership desired"
    );
    assert_eq!(
        row.frame_authority,
        Auth::Controlled,
        "core owns the target geometry, never a native remap guess"
    );
    row.effective_frame
        .expect("core publishes the actionable target slot pre-arrival")
}

fn mover_live(token: &str) -> Rect {
    live_visible_observed()
        .iter()
        .find(|w| w.token == token)
        .expect("mover stays visible")
        .visible
}

#[test]
fn tiled_cross_output_send_writes_target_frame_both_ways() {
    let mut env = env_with(204_001);
    env.state.ensure_device(DEV2);
    let o1 = env.state.output_token(DEV1);
    let o2 = env.state.output_token(DEV2);
    let (areas, fulls) = asymmetric_send_areas();
    let key = win_key(204_001, 2204, "proc-104");
    // Residents keep both outputs' engine domains live: the mover never
    // targets an empty domain the session cannot plan.
    let stay1 = win_key(204_011, 2214, "proc-114");
    let stay2 = win_key(204_012, 2215, "proc-115");
    admit_bare(&mut env, &key, "w1");
    admit_bare(&mut env, &stay1, "w0");
    admit_bare(&mut env, &stay2, "w2");
    script_live(&key, "w1", rect(100, 100, 400, 300), plain_facts());
    script_live(&stay1, "w0", rect(200, 200, 300, 200), plain_facts());
    script_live(&stay2, "w2", rect(2700, 400, 400, 300), plain_facts());
    let mut owner = WorldOwner::bootstrap_ids(ADAPTER, GENERATION).expect("owner");
    let observed = live_visible_observed();
    live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        0,
        None,
    );
    // Forward: o1 -> o2 active view with follow. The core publishes the
    // exact target slot with the desire (never a FULL-delta remap of the
    // source position); the seam writes it as part of the move and the
    // sweep converges around the same slot after arrival proves.
    let target_b = cross_output_target(&owner, &o2);
    let observed = live_visible_observed();
    let before_writes = S_SET_CALLS.with(|m| m.borrow().len());
    let forward = live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        key.hwnd,
        Some(WorldCommand::SendDirectional {
            window: WindowId("w1".to_owned()),
            direction: "right".to_owned(),
            follow: true,
        }),
    );
    let op = window_op(&forward.reply, "w1");
    let slot = mover_slot(&forward.reply, "w1");
    assert_slot_in_work(slot, areas[1].work);
    assert!(
        applied_of_members(&forward.members, &forward.views, &op),
        "{:?} {:?}",
        forward.members,
        forward.views
    );
    let expected_outer = Rect {
        x: slot.x - 8,
        y: slot.y - 8,
        w: slot.w + 16,
        h: slot.h + 16,
    };
    let writes: Vec<(u64, Rect)> = S_SET_CALLS.with(|m| m.borrow().clone());
    assert!(
        writes.len() > before_writes,
        "cross-output physically writes: {writes:?}"
    );
    assert!(
        writes
            .iter()
            .any(|(hwnd, outer)| *hwnd == key.hwnd && *outer == expected_outer),
        "DWM-inset outer write for the published slot: {writes:?} vs {expected_outer:?}"
    );
    assert_eq!(mover_live("w1"), slot, "frame lands exactly on target");
    assert_eq!(
        env.state.applied_homing.get("w1"),
        Some(&(o2.clone(), target_b.clone()))
    );
    let observed = live_visible_observed();
    let settled = live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        0,
        None,
    );
    assert!(
        settled.reply.transfers.is_empty(),
        "forward crossing settles: {:?}",
        settled.reply.transfers
    );
    // Back: o2 -> o1 active view with follow (same slot-owned proof).
    let target_a = cross_output_target(&owner, &o1);
    let observed = live_visible_observed();
    let back = live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        key.hwnd,
        Some(WorldCommand::SendDirectional {
            window: WindowId("w1".to_owned()),
            direction: "left".to_owned(),
            follow: true,
        }),
    );
    let back_op = window_op(&back.reply, "w1");
    let slot_back = mover_slot(&back.reply, "w1");
    assert_slot_in_work(slot_back, areas[0].work);
    assert!(
        applied_of_members(&back.members, &back.views, &back_op),
        "{:?} {:?}",
        back.members,
        back.views
    );
    assert_eq!(mover_live("w1"), slot_back, "return frame lands exactly");
    assert_eq!(
        env.state.applied_homing.get("w1"),
        Some(&(o1.clone(), target_a.clone()))
    );
    let observed = live_visible_observed();
    let resettled = live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        0,
        None,
    );
    assert!(
        resettled.reply.transfers.is_empty(),
        "return crossing settles: {:?}",
        resettled.reply.transfers
    );
}

#[test]
fn lying_setter_keeps_no_receipt_and_flight_pending() {
    let mut env = env_with(205_001);
    env.state.ensure_device(DEV2);
    let (areas, fulls) = two_output_areas();
    let key = win_key(205_001, 2205, "proc-105");
    let stay2 = win_key(205_012, 2215, "proc-115");
    admit_bare(&mut env, &key, "w1");
    admit_bare(&mut env, &stay2, "w2");
    script_live(&key, "w1", rect(100, 100, 400, 300), plain_facts());
    script_live(&stay2, "w2", rect(2000, 100, 400, 300), plain_facts());
    let mut owner = WorldOwner::bootstrap_ids(ADAPTER, GENERATION).expect("owner");
    let observed = live_visible_observed();
    live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        0,
        None,
    );
    let home_before = env.state.applied_homing.get("w1").cloned().unwrap();
    // Lying desktop: the setter reports success but the frame never moves
    // (base scripted seam records the call without moving). Directional
    // send owns the crossing; explicit Send stays same-output only.
    S_SET_OK.with(|m| m.borrow_mut().insert(key.hwnd, true));
    let observed = live_visible_observed();
    let before_mover =
        S_SET_CALLS.with(|m| m.borrow().iter().filter(|(h, _)| *h == key.hwnd).count());
    let tick = live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        scripted(),
        key.hwnd,
        Some(WorldCommand::SendDirectional {
            window: WindowId("w1".to_owned()),
            direction: "right".to_owned(),
            follow: true,
        }),
    );
    let op = window_op(&tick.reply, "w1");
    // The lie reports one raw accepted attempt whose post proof never
    // reaches the target: ambiguous, never an arrival.
    assert!(
        tick.members
            .iter()
            .any(|a| a.operation_id == op
                && a.setter == tiler_core::world_report::FrameSetter::Accepted),
        "lie holds ambiguous: {:?}",
        tick.members
    );
    assert!(
        !applied_of_members(&tick.members, &tick.views, &op),
        "no fabricated arrival: {:?} {:?}",
        tick.members,
        tick.views
    );
    assert_eq!(
        S_SET_CALLS.with(|m| m.borrow().iter().filter(|(h, _)| *h == key.hwnd).count()),
        before_mover + 1,
        "one attempted mover write, never repeated: {:?}",
        S_SET_CALLS.with(|m| m.borrow().clone())
    );
    assert_eq!(
        env.state.applied_homing.get("w1"),
        Some(&home_before),
        "no receipt merges on a lie"
    );
    // The flight stays pending: the next report retains the transfer.
    let observed = live_visible_observed();
    let pending = live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        0,
        None,
    );
    assert!(
        pending.reply.transfers.iter().any(|t| t.operation_id == op),
        "flight pending after lie: {:?}",
        pending.reply.transfers
    );
}

#[test]
fn floating_cross_output_send_preserves_rectangle_and_settles() {
    let mut env = env_with(206_001);
    env.state.ensure_device(DEV2);
    let o2 = env.state.output_token(DEV2);
    let (areas, fulls) = two_output_areas();
    let key = win_key(206_001, 2206, "proc-106");
    // Tiled resident keeps the target output's engine domain plannable.
    let stay2 = win_key(206_012, 2216, "proc-116");
    admit_bare(&mut env, &key, "w1");
    admit_bare(&mut env, &stay2, "w2");
    env.state.floated.insert(key.clone());
    let float_rect = rect(120, 120, 400, 300);
    script_live(&key, "w1", float_rect, plain_facts());
    script_live(&stay2, "w2", rect(2000, 100, 400, 300), plain_facts());
    let mut owner = WorldOwner::bootstrap_ids(ADAPTER, GENERATION).expect("owner");
    let observed = live_visible_observed();
    live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        0,
        None,
    );
    assert!(
        env.state.applied_homing.contains_key("w1"),
        "float bootstraps a receipt"
    );
    let target_b = cross_output_target(&owner, &o2);
    let writes_before = S_SET_CALLS.with(|m| m.borrow().len());
    let observed = live_visible_observed();
    let tick = live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        key.hwnd,
        Some(WorldCommand::SendDirectional {
            window: WindowId("w1".to_owned()),
            direction: "right".to_owned(),
            follow: true,
        }),
    );
    let op = window_op(&tick.reply, "w1");
    // Membership-only crossing: the accepted attempt preserves the physical
    // frame (pre == post on the source output) while the logical home
    // arrives through the no-effect receipt. Never a frame write, never a
    // reinterpreted physical output.
    let attempt = tick
        .members
        .iter()
        .find(|a| a.operation_id == op)
        .expect("float crossing reports one member attempt");
    assert_eq!(
        attempt.setter,
        tiler_core::world_report::FrameSetter::Accepted
    );
    assert_eq!(attempt.pre, attempt.post);
    assert!(attempt.post.is_some());
    assert_eq!(
        S_SET_CALLS.with(|m| m.borrow().len()),
        writes_before,
        "membership-only crossing writes no frame"
    );
    let live = live_visible_observed();
    let w1live = live
        .iter()
        .find(|w| w.token == "w1")
        .expect("floater stays visible");
    assert_eq!(
        w1live.visible, float_rect,
        "physical rectangle preserved across the logical crossing"
    );
    assert_eq!(
        env.state.applied_homing.get("w1"),
        Some(&(o2.clone(), target_b.clone())),
        "logical home still arrives"
    );
    // Next report settles on the logical receipt even though the frame
    // physically stays on the source output.
    let observed = live_visible_observed();
    let settled = live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        0,
        None,
    );
    assert!(
        settled.reply.transfers.is_empty(),
        "float crossing settles: {:?}",
        settled.reply.transfers
    );
}

#[test]
fn switch_view_two_entering_two_leaving_proves_only_when_complete() {
    let mut env = env_with(207_001);
    let out = env.state.output_token(DEV1);
    let ws1 = "ws1".to_owned();
    let ws2 = "ws2".to_owned();
    let leaving1 = win_key(207_001, 2211, "proc-111");
    let leaving2 = win_key(207_002, 2212, "proc-112");
    let entering1 = win_key(207_003, 2213, "proc-113");
    let entering2 = win_key(207_004, 2214, "proc-114");
    admit_bare(&mut env, &leaving1, "a1");
    admit_bare(&mut env, &leaving2, "a2");
    admit_bare(&mut env, &entering1, "b1");
    admit_bare(&mut env, &entering2, "b2");
    env.state
        .applied_homing
        .insert("a1".to_owned(), (out.clone(), ws1.clone()));
    env.state
        .applied_homing
        .insert("a2".to_owned(), (out.clone(), ws1.clone()));
    env.state
        .applied_homing
        .insert("b1".to_owned(), (out.clone(), ws2.clone()));
    env.state
        .applied_homing
        .insert("b2".to_owned(), (out.clone(), ws2.clone()));
    env.state.applied_views.insert(out.clone(), ws1.clone());
    let frozen = rect(0, 0, 800, 600);
    let observed = vec![
        observed_for(&leaving1, "a1", frozen, plain_facts()),
        observed_for(&leaving2, "a2", rect(0, 610, 800, 400), plain_facts()),
    ];
    for key in [&leaving1, &leaving2, &entering1, &entering2] {
        S_TAG.with(|m| m.borrow_mut().insert(key.hwnd, "member-tag-1".to_owned()));
        S_IDENT.with(|m| m.borrow_mut().insert(key.hwnd, identity_of(key)));
        S_ADMIT.with(|m| {
            m.borrow_mut()
                .insert(key.hwnd, Ok(script_claim(key.hwnd, &identity_of(key))))
        });
        S_HIDE.with(|m| {
            m.borrow_mut()
                .insert(key.hwnd, Ok(script_claim(key.hwnd, &identity_of(key))))
        });
        S_REVEAL.with(|m| m.borrow_mut().insert(key.hwnd, Ok(Revealed::Shown)));
    }
    script_live(&leaving1, "a1", frozen, plain_facts());
    script_live(&leaving2, "a2", rect(0, 610, 800, 400), plain_facts());
    script_live(&entering1, "b1", frozen, plain_facts());
    script_live(&entering2, "b2", rect(0, 610, 800, 400), plain_facts());
    S_VISIBLE.with(|m| {
        m.borrow_mut().remove(&entering1.hwnd);
        m.borrow_mut().remove(&entering2.hwnd);
    });
    for key in [&entering1, &entering2] {
        env.state.hidden_claims.insert(
            key.clone(),
            super::super::super::HiddenRecord {
                claim: script_claim(key.hwnd, &identity_of(key)),
                iconic: false,
            },
        );
    }
    let inputs = env.state.inputs();
    let captured = capture_world(&inputs, Some(&observed), &[], &one_area(), 207_001);
    let fresh = FreshHost::from_sources(&inputs, &observed, &[], &captured);
    let mut diff = HostDiff::new(ADAPTER, GENERATION);
    diff.gate.current_obs = 7;
    let mut reply = reply_shell(
        7,
        1,
        &[(out.as_str(), ws1.as_str()), (out.as_str(), ws2.as_str())],
    );
    reply.outputs.push(tiler_core::world_report::OutputDesire {
        output: OutputId(out.clone()),
        logical_view: Some(ws2.clone()),
        tiling_mode: tiler_core::world_report::TilingMode::Tiled,
        hold: None,
    });
    reply.transfers.push(TransferTarget {
        operation_id: "op-view-2x2".to_owned(),
        kind: TransferKind::View,
        target_output: OutputId(out.clone()),
        target_workspace: ws2.clone(),
        follow: false,
        held: false,
    });
    // Full completion: both hide legs and both reveal legs land.
    let mut native =
        assemble_with_views(&mut env, moving_prims(), Some((out.clone(), ws2.clone())));
    native
        .core_rings
        .entry(out.clone())
        .or_default()
        .extend([ws1.clone(), ws2.clone()]);
    let mut members = Vec::new();
    let mut views = Vec::new();
    let stats = apply_world_reply(
        &mut diff,
        &reply,
        &fresh,
        &mut native,
        &mut Vec::new(),
        &mut members,
        &mut views,
        &mut Vec::new(),
        0,
    );
    assert!(members.is_empty());
    assert_eq!(views.len(), 1);
    assert_eq!(views[0].operation_id, "op-view-2x2");
    assert_eq!(views[0].post.as_deref(), Some(ws2.as_str()));
    assert_eq!(stats.view_writes, 1, "{stats:?}");
    drop(native);
    // Partial completion: the second reveal refuses, so nothing proves.
    reset_script();
    for key in [&leaving1, &leaving2, &entering1, &entering2] {
        S_TAG.with(|m| m.borrow_mut().insert(key.hwnd, "member-tag-1".to_owned()));
        S_IDENT.with(|m| m.borrow_mut().insert(key.hwnd, identity_of(key)));
        S_ADMIT.with(|m| {
            m.borrow_mut()
                .insert(key.hwnd, Ok(script_claim(key.hwnd, &identity_of(key))))
        });
        S_HIDE.with(|m| {
            m.borrow_mut()
                .insert(key.hwnd, Ok(script_claim(key.hwnd, &identity_of(key))))
        });
    }
    S_REVEAL.with(|m| m.borrow_mut().insert(entering1.hwnd, Ok(Revealed::Shown)));
    S_REVEAL.with(|m| {
        m.borrow_mut()
            .insert(entering2.hwnd, Err("refused: scripted".to_owned()))
    });
    let mut env2 = env_with(207_011);
    let out2 = env2.state.output_token(DEV1);
    for (key, token, ws) in [
        (&leaving1, "a1", ws1.clone()),
        (&leaving2, "a2", ws1.clone()),
        (&entering1, "b1", ws2.clone()),
        (&entering2, "b2", ws2.clone()),
    ] {
        admit_bare(&mut env2, key, token);
        env2.state
            .applied_homing
            .insert(token.to_owned(), (out2.clone(), ws));
    }
    env2.state.applied_views.insert(out2.clone(), ws1.clone());
    script_live(&leaving1, "a1", frozen, plain_facts());
    script_live(&leaving2, "a2", rect(0, 610, 800, 400), plain_facts());
    script_live(&entering1, "b1", frozen, plain_facts());
    script_live(&entering2, "b2", rect(0, 610, 800, 400), plain_facts());
    S_VISIBLE.with(|m| {
        m.borrow_mut().remove(&entering1.hwnd);
        m.borrow_mut().remove(&entering2.hwnd);
    });
    for key in [&entering1, &entering2] {
        env2.state.hidden_claims.insert(
            key.clone(),
            super::super::super::HiddenRecord {
                claim: script_claim(key.hwnd, &identity_of(key)),
                iconic: false,
            },
        );
    }
    let inputs = env2.state.inputs();
    let captured = capture_world(&inputs, Some(&observed), &[], &one_area(), 207_001);
    let fresh = FreshHost::from_sources(&inputs, &observed, &[], &captured);
    let mut diff = HostDiff::new(ADAPTER, GENERATION);
    diff.gate.current_obs = 7;
    let mut reply = reply_shell(
        7,
        1,
        &[(out2.as_str(), ws1.as_str()), (out2.as_str(), ws2.as_str())],
    );
    reply.outputs.push(tiler_core::world_report::OutputDesire {
        output: OutputId(out2.clone()),
        logical_view: Some(ws2.clone()),
        tiling_mode: tiler_core::world_report::TilingMode::Tiled,
        hold: None,
    });
    reply.transfers.push(TransferTarget {
        operation_id: "op-view-2x2-partial".to_owned(),
        kind: TransferKind::View,
        target_output: OutputId(out2.clone()),
        target_workspace: ws2.clone(),
        follow: false,
        held: false,
    });
    let mut native =
        assemble_with_views(&mut env2, moving_prims(), Some((out2.clone(), ws2.clone())));
    native
        .core_rings
        .entry(out2.clone())
        .or_default()
        .extend([ws1.clone(), ws2.clone()]);
    let mut members = Vec::new();
    let mut views = Vec::new();
    let stats = apply_world_reply(
        &mut diff,
        &reply,
        &fresh,
        &mut native,
        &mut Vec::new(),
        &mut members,
        &mut views,
        &mut Vec::new(),
        0,
    );
    assert!(members.is_empty());
    // Partial batch proves nothing: the refused reveal leaves the view
    // off-target, so the raw attempt holds truthfully with no proof row
    // that names the target.
    assert_eq!(views.len(), 1);
    assert_eq!(views[0].operation_id, "op-view-2x2-partial");
    assert_ne!(views[0].post.as_deref(), Some(ws2.as_str()));
    assert_eq!(stats.view_writes, 0, "{stats:?}");
}

// ---------------------------------------------------------------------------
// Stage 2b admission/startup/restart truth hardening: named regressions for
// the deleted Windows accepted fixed/admission/hint/first-exit/restart
// coverage (producer-shaped hints, recycled-HWND retirement, born capture,
// minimized/hidden truth, marker restore, first exit, token-scoped forget).
// ---------------------------------------------------------------------------

#[test]
fn world_fixed_hint_producer_shapes_fixed_admission() {
    // Producer-shaped hints: one bounded `WM_GETMINMAXINFO` query per bound
    // visible member stores into `hint_logged` and rides the captured fact,
    // so fixed-size admission (min=max pinned) classifies without invention.
    reset_script();
    let mut state = test_state();
    state.ensure_device(DEV1);
    let key = win_key(301_001, 3201, "proc-301");
    state.member_tokens.insert(key.clone(), "w1".to_owned());
    state
        .member_tags
        .insert(key.clone(), "member-tag-1".to_owned());
    state.member_identity.insert(key.clone(), identity_of(&key));
    let pinned = tiler_core::size_hints::WindowSizeHints {
        min_w: Some(400),
        min_h: Some(300),
        max_w: Some(400),
        max_h: Some(300),
    };
    super::super::super::inject_test_hints(301_001, pinned);
    let observed = vec![observed_for(
        &key,
        "w1",
        rect(0, 0, 800, 600),
        plain_facts(),
    )];
    let areas = one_area();
    let reply = super::super::super::world_tick::dispatch_test_observation(
        &mut state,
        &observed,
        &[],
        &areas,
        0,
        None,
        0,
    )
    .expect("dispatch converges");
    super::super::super::clear_test_hints();
    assert_eq!(
        state.hint_logged.get("w1"),
        Some(&pinned),
        "producer syncs hint_logged, got {:?}",
        state.hint_logged
    );
    let captured = capture_world(&state.inputs(), Some(&observed), &[], &areas, 0);
    let fact = captured
        .windows
        .iter()
        .find(|f| f.window.0 == "w1")
        .expect("w1 fact");
    assert_eq!(fact.hints, pinned, "captured fact carries producer hints");
    assert!(reply.ready, "fixed admission still converges");
    assert!(
        state
            .hint_fallback_uses
            .get("w1")
            .is_some_and(|use_| !use_.reused),
        "fresh query reports fresh, got {:?}",
        state.hint_fallback_uses.get("w1")
    );
}

#[test]
fn world_recycled_hwnd_stale_holds_without_dropping_live() {
    // Recycled HWND: the live lifetime rides its row while the stale
    // same-HWND lifetime holds as unreadable (never a fabricated row, never
    // a close, never a dropped live claim).
    reset_script();
    let mut state = test_state();
    state.ensure_device(DEV1);
    let live = win_key(302_001, 401, "proc-live");
    let stale = win_key(302_001, 402, "proc-stale");
    for (key, token) in [(&live, "w-live"), (&stale, "w-stale")] {
        state.member_tokens.insert(key.clone(), token.to_owned());
        state
            .member_tags
            .insert(key.clone(), "member-tag-1".to_owned());
        state.member_identity.insert(key.clone(), identity_of(key));
    }
    let observed = vec![observed_for(
        &live,
        "w-live",
        rect(0, 0, 800, 600),
        plain_facts(),
    )];
    let areas = one_area();
    let captured = capture_world(&state.inputs(), Some(&observed), &[], &areas, 0);
    let live_fact = captured
        .windows
        .iter()
        .find(|f| f.window.0 == "w-live")
        .expect("live fact");
    assert!(live_fact.rect.is_some(), "live lifetime rides its row");
    let stale_fact = captured
        .windows
        .iter()
        .find(|f| f.window.0 == "w-stale")
        .expect("stale fact");
    assert!(
        stale_fact.rect.is_none() && !stale_fact.minimized,
        "stale lifetime holds unreadable"
    );
    assert!(
        captured.closed.is_empty(),
        "omission is never a close: {:?}",
        captured.closed
    );
}

#[test]
fn world_forget_member_clears_token_scoped_caches() {
    // Causal stale cleanup: forgetting a member drops every token-scoped
    // cache (tile slot, float frame, hints, fallback, overlay) with the
    // token, not a retain-true no-op. Unknown keys report false.
    reset_script();
    let mut state = test_state();
    state.ensure_device(DEV1);
    let key = win_key(303_001, 501, "proc-303");
    state.member_tokens.insert(key.clone(), "w1".to_owned());
    state
        .member_tags
        .insert(key.clone(), "member-tag-1".to_owned());
    state.member_identity.insert(key.clone(), identity_of(&key));
    state
        .member_rects
        .insert("w1".to_owned(), rect(0, 0, 100, 100));
    state
        .float_rects
        .insert("w1".to_owned(), rect(0, 0, 100, 100));
    state.hint_logged.insert(
        "w1".to_owned(),
        tiler_core::size_hints::WindowSizeHints::none(),
    );
    state.overlay_now.insert("w1".to_owned(), (false, false));
    assert!(state.forget_member(&key), "known member reports true");
    assert!(!state.member_tokens.contains_key(&key));
    assert!(!state.member_rects.contains_key("w1"), "tile slot clears");
    assert!(!state.float_rects.contains_key("w1"), "float frame clears");
    assert!(!state.hint_logged.contains_key("w1"), "hints clear");
    assert!(!state.overlay_now.contains_key("w1"), "overlay clears");
    let unknown = win_key(303_009, 599, "proc-unknown");
    assert!(!state.forget_member(&unknown), "unknown reports false");
}

#[test]
fn world_born_fullscreen_captures_quiet_with_slot_truth() {
    // Newly observed born fullscreen: floating fullscreen fact on the live
    // frame with no invented tile slot, quiet first admission (no close, no
    // restoration ingress).
    reset_script();
    let mut state = test_state();
    state.ensure_device(DEV1);
    let key = win_key(304_001, 601, "proc-304");
    state.member_tokens.insert(key.clone(), "w1".to_owned());
    state
        .member_tags
        .insert(key.clone(), "member-tag-1".to_owned());
    state.member_identity.insert(key.clone(), identity_of(&key));
    state.born_fullscreen.insert(key.clone());
    let mut facts = plain_facts();
    facts.captionless_fullscreen = true;
    let observed = vec![observed_for(&key, "w1", rect(0, 0, 800, 600), facts)];
    let areas = one_area();
    let captured = capture_world(&state.inputs(), Some(&observed), &[], &areas, 304_001);
    let fact = captured
        .windows
        .iter()
        .find(|f| f.window.0 == "w1")
        .expect("w1 fact");
    assert!(
        fact.floating && fact.fullscreen,
        "born hold floats fullscreen: {fact:?}"
    );
    assert_eq!(fact.rect, Some(rect(0, 0, 800, 600)), "live frame rides");
    assert!(
        captured.closed.is_empty(),
        "quiet admission: {:?}",
        captured.closed
    );
}

#[test]
fn world_minimized_omission_never_closes_hidden_keeps_fullscreen() {
    // Minimized omission rides a minimized fact (never a close); a hidden
    // ordinary fullscreen keeps its fullscreen truth through the retained
    // overlay flags (durable preimage carries maximized only).
    reset_script();
    let mut state = test_state();
    state.ensure_device(DEV1);
    let min_key = win_key(305_001, 701, "proc-min");
    state
        .member_tokens
        .insert(min_key.clone(), "w-min".to_owned());
    state
        .member_tags
        .insert(min_key.clone(), "member-tag-1".to_owned());
    state
        .member_identity
        .insert(min_key.clone(), identity_of(&min_key));
    state
        .member_rects
        .insert("w-min".to_owned(), rect(0, 0, 400, 300));
    state.minimized_omitted.insert(min_key.clone());
    let hid_key = win_key(305_002, 702, "proc-hid");
    state
        .member_tokens
        .insert(hid_key.clone(), "w-hid".to_owned());
    state
        .member_tags
        .insert(hid_key.clone(), "member-tag-1".to_owned());
    state
        .member_identity
        .insert(hid_key.clone(), identity_of(&hid_key));
    state
        .member_rects
        .insert("w-hid".to_owned(), rect(0, 0, 800, 600));
    let mut claim = valid_claim(305_002, &identity_of(&hid_key));
    claim.show = WindowShowState {
        minimized: false,
        maximized: false,
    };
    state.hidden_claims.insert(
        hid_key.clone(),
        super::super::super::HiddenRecord {
            claim,
            iconic: false,
        },
    );
    let retained = vec![super::super::super::RetainedRow {
        key: hid_key.clone(),
        token: "w-hid".to_owned(),
        rect: Some(rect(0, 0, 800, 600)),
        maximized: false,
        fullscreen: true,
        facts: Some(plain_facts()),
    }];
    let areas = one_area();
    let captured = capture_world(&state.inputs(), Some(&[]), &retained, &areas, 0);
    let min_fact = captured
        .windows
        .iter()
        .find(|f| f.window.0 == "w-min")
        .expect("min fact");
    assert!(
        min_fact.minimized && min_fact.rect.is_none(),
        "minimized rides: {min_fact:?}"
    );
    let hid_fact = captured
        .windows
        .iter()
        .find(|f| f.window.0 == "w-hid")
        .expect("hidden fact");
    assert!(
        hid_fact.fullscreen,
        "hidden keeps fullscreen truth: {hid_fact:?}"
    );
    assert!(!hid_fact.minimized, "hidden snapshot is not minimized");
    assert!(
        captured.closed.is_empty(),
        "omission is never a close: {:?}",
        captured.closed
    );
}

#[test]
fn world_marker_intent_restores_quiet_without_receipts() {
    // Fresh-owner marker restore: staged float/sticky lanes ride floating
    // facts on the first observation with no fabricated restoration and no
    // proven receipts (quiet bootstrap).
    reset_script();
    let mut state = test_state();
    state.ensure_device(DEV1);
    let float_key = win_key(306_001, 801, "proc-float");
    let sticky_key = win_key(306_002, 802, "proc-sticky");
    for (key, token) in [(&float_key, "w-float"), (&sticky_key, "w-sticky")] {
        state.member_tokens.insert(key.clone(), token.to_owned());
        state
            .member_tags
            .insert(key.clone(), "member-tag-1".to_owned());
        state.member_identity.insert(key.clone(), identity_of(key));
        state
            .float_rects
            .insert(token.to_owned(), rect(10, 10, 400, 300));
    }
    state.floated.insert(float_key.clone());
    state.sticky.insert(sticky_key.clone(), false);
    let observed = vec![
        observed_for(&float_key, "w-float", rect(10, 10, 400, 300), plain_facts()),
        observed_for(
            &sticky_key,
            "w-sticky",
            rect(10, 10, 400, 300),
            plain_facts(),
        ),
    ];
    let areas = one_area();
    let captured = capture_world(&state.inputs(), Some(&observed), &[], &areas, 0);
    for token in ["w-float", "w-sticky"] {
        let fact = captured
            .windows
            .iter()
            .find(|f| f.window.0 == token)
            .expect("fact");
        assert!(fact.floating, "{token} restores floating: {fact:?}");
    }
    assert!(captured.closed.is_empty(), "quiet: {:?}", captured.closed);
    assert!(
        captured.observed_views.is_empty(),
        "marker-only restore proves no view: {:?}",
        captured.observed_views
    );
}

#[test]
fn world_first_exit_tiles_quietly() {
    // First born-fullscreen exit with non-fixed hints tiles on the fresh
    // query output (floating false), carrying exactly the fresh hints.
    reset_script();
    let mut state = test_state();
    state.ensure_device(DEV1);
    let key = win_key(307_001, 901, "proc-exit");
    state.member_tokens.insert(key.clone(), "w1".to_owned());
    state
        .member_tags
        .insert(key.clone(), "member-tag-1".to_owned());
    state.member_identity.insert(key.clone(), identity_of(&key));
    state.born_fullscreen.insert(key.clone());
    state.born_first_exit.insert(key.clone());
    super::super::super::inject_test_hints(
        307_001,
        tiler_core::size_hints::WindowSizeHints::none(),
    );
    let observed = vec![observed_for(
        &key,
        "w1",
        rect(0, 0, 800, 600),
        plain_facts(),
    )];
    let areas = one_area();
    let reply = super::super::super::world_tick::dispatch_test_observation(
        &mut state,
        &observed,
        &[],
        &areas,
        0,
        None,
        0,
    )
    .expect("dispatch converges");
    super::super::super::clear_test_hints();
    let captured = capture_world(&state.inputs(), Some(&observed), &[], &areas, 0);
    let fact = captured
        .windows
        .iter()
        .find(|f| f.window.0 == "w1")
        .expect("w1 fact");
    assert!(!fact.floating, "first exit tiles: {fact:?}");
    assert!(
        fact.hints.is_empty(),
        "fresh empty hints ride: {:?}",
        fact.hints
    );
    assert!(reply.ready);
}

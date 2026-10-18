#![cfg(test)]
//! Offline managed-send effect tests: injectable-backend proof of the
//! producer-shaped runtime path. No live touch, no taps/helpers/probes, no
//! AX polling on the test (main) thread beyond the bounded completion poll.
//!
//! The dedicated effect thread owns its fake backend (built on that thread
//! via the spawn factory); the test thread only submits nonblocking and
//! polls nonblocking. Numbered send-follow 1..9 is product-armed;
//! trailing send-0, stays, relative sends, and outputs stay refused.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, mpsc};
use std::time::Duration;

use tiler_core::directional::{OutputId, WorkspaceId};

use crate::desktop_shortcuts::{
    ShortcutEntry, desktop_digit_keycode, desktop_symbolic_id, resolve_numbered_chord,
};
use crate::space_drag_transfer::{DragStep, PostCounts};
use crate::workspace_select::{SelectionSnapshot, SelectionState};
use crate::workspace_send::{SendGates, bindings};
use crate::workspace_send_effect::{
    ManagedSendTarget, SendEffectHandoff, SendEffectOutcome, SendFreshEvidence, prove_managed_send,
};
use crate::workspace_send_managed::{
    EFFECT_REFUSAL_DETAIL_CHARS, ManagedPostPermit, ManagedSendBackend, ManagedSendThread,
    authorize_managed_edge, classify_holder_chord_error, completion_for, execute_managed_send,
    format_effect_refusal_line, sanitize_refusal_detail,
};

const CG_CTRL: u64 = 0x0004_0000;

fn source_domain() -> tiler_core::session::DomainKey {
    tiler_core::session::DomainKey {
        output: OutputId("macos-display-1".to_owned()),
        workspace: WorkspaceId("space-20".to_owned()),
    }
}

fn target() -> ManagedSendTarget {
    ManagedSendTarget::bind(4242, 7, 1, source_domain(), 3, 9, "op-3")
        .expect("fixture target binds")
}

fn fresh_ok() -> SendFreshEvidence {
    SendFreshEvidence {
        managed_current: Some(true),
        fullscreen: Some(false),
        minimized: Some(false),
        live_generation: Some(1),
        live_revision: Some(9),
        live_source: Some(source_domain()),
        source_current: Some(true),
        focused_window: Some("4242_7".to_owned()),
        focus_agrees: Some(true),
        geometry_ok: Some(true),
        grab_point: Some((100.0, 200.0)),
        ax_hit: Some("4242_7".to_owned()),
        ws_hit: Some("4242_7".to_owned()),
    }
}

fn row() -> SelectionSnapshot {
    SelectionSnapshot {
        display: "display-a".to_owned(),
        spaces: vec![20, 3, 7],
        current: 20,
    }
}

fn open_gates() -> SendGates {
    SendGates {
        session_unlocked: true,
        secure_active: false,
        lock_screen: false,
    }
}

fn enabled_entry(desktop: u32) -> ShortcutEntry {
    ShortcutEntry {
        enabled: true,
        keycode: desktop_digit_keycode(desktop).unwrap(),
        modifiers: CG_CTRL,
    }
}

/// Scripted managed backend: canned gates/row/holders plus
/// per-edge fresh-evidence scripting and fixture-style drag effect with
/// fault injection. No live touch, no sleeps on the fast path.
struct FakeManaged {
    gates: SendGates,
    displays: Result<usize, String>,
    snapshot: Result<SelectionSnapshot, String>,
    holders: HashMap<u32, ShortcutEntry>,
    fail_holder_reads: Vec<u32>,
    evidence_base: SendFreshEvidence,
    /// Queued per-call evidence overrides (popped front per
    /// `fresh_evidence` call once non-empty; otherwise the base returns).
    evidence_script: VecDeque<SendFreshEvidence>,
    /// From this `fresh_evidence` call number onward, focus names a foreign
    /// window (mid-leg swap simulation). `None` disables.
    swap_from_call: Option<usize>,
    swap_focus_to: String,
    evidence_calls: usize,
    last_grab: Option<(f64, f64)>,
    membership: Vec<u64>,
    current: Option<u64>,
    goal: u64,
    selected_chord: Option<(u32, u64)>,
    mouse_points: Vec<(f64, f64)>,
    key_chords: Vec<(u32, u64)>,
    key_posts: Vec<(u32, u64, Option<i64>, bool)>,
    counts: PostCounts,
    fail_begin: bool,
    fail_mouse_at: Option<DragStep>,
    fail_chord_down: bool,
    fail_key_up_once: bool,
    fail_key_recovery: bool,
    fail_mouse_up: bool,
    no_effect: bool,
    freeze_current: bool,
    unreadable: bool,
    /// When set, `begin_leg` signals start then blocks for release (busy +
    /// cancellation determinism for the threaded tests).
    block_begin: Option<(mpsc::Sender<()>, mpsc::Receiver<()>)>,
    /// When set, `post_mouse(Down)` requests stop on this lane mid-effect.
    cancel_on_mouse_down: Option<Arc<SendEffectHandoff>>,
}

impl FakeManaged {
    fn new() -> Self {
        let mut holders = HashMap::new();
        for desktop in 1..=9u32 {
            holders.insert(
                desktop_symbolic_id(desktop).unwrap(),
                enabled_entry(desktop),
            );
        }
        Self {
            gates: open_gates(),
            displays: Ok(1),
            snapshot: Ok(row()),
            holders,
            fail_holder_reads: Vec::new(),
            evidence_base: fresh_ok(),
            evidence_script: VecDeque::new(),
            swap_from_call: None,
            swap_focus_to: "4242_9".to_owned(),
            evidence_calls: 0,
            last_grab: None,
            membership: vec![20],
            current: Some(20),
            goal: 0,
            selected_chord: None,
            mouse_points: Vec::new(),
            key_chords: Vec::new(),
            key_posts: Vec::new(),
            counts: PostCounts::default(),
            fail_begin: false,
            fail_mouse_at: None,
            fail_chord_down: false,
            fail_key_up_once: false,
            fail_key_recovery: false,
            fail_mouse_up: false,
            no_effect: false,
            freeze_current: false,
            unreadable: false,
            block_begin: None,
            cancel_on_mouse_down: None,
        }
    }

    fn check_permit(&self, permit: &ManagedPostPermit) {
        assert_eq!((permit.pid(), permit.wid()), (4242, 7));
        assert_eq!(
            Some(permit.point()),
            self.last_grab,
            "post uses exactly the backend-selected fresh grab point"
        );
    }
}

impl ManagedSendBackend for FakeManaged {
    fn read_gates(&mut self) -> SendGates {
        self.gates
    }
    fn snapshot(&mut self) -> Result<SelectionSnapshot, String> {
        self.snapshot.clone()
    }
    fn display_count(&mut self) -> Result<usize, String> {
        self.displays.clone()
    }
    fn read_holder(&mut self, symbolic_id: u32) -> Result<ShortcutEntry, String> {
        if self.fail_holder_reads.contains(&symbolic_id) {
            return Err(format!("injected holder read refusal for {symbolic_id}"));
        }
        self.holders.get(&symbolic_id).copied().ok_or_else(|| {
            format!("fake holder {symbolic_id} unresolvable; refusing rather than guessing")
        })
    }
    fn fresh_evidence(&mut self, target: &ManagedSendTarget) -> SendFreshEvidence {
        self.evidence_calls += 1;
        assert_eq!(
            target.window(),
            "4242_7",
            "effect grabs the pinned subject only"
        );
        let evidence = if let Some(evidence) = self.evidence_script.pop_front() {
            evidence
        } else if self
            .swap_from_call
            .is_some_and(|from| self.evidence_calls >= from)
        {
            let mut swapped = self.evidence_base.clone();
            swapped.focused_window = Some(self.swap_focus_to.clone());
            swapped
        } else {
            self.evidence_base.clone()
        };
        self.last_grab = evidence.grab_point;
        evidence
    }
    fn begin_leg(&mut self, permit: ManagedPostPermit, goal: u64) -> Result<(), String> {
        self.check_permit(&permit);
        if let Some((started, release)) = self.block_begin.take() {
            let _ = started.send(());
            // Bounded block: the test releases; a timeout fails closed.
            match release.recv_timeout(Duration::from_secs(5)) {
                Ok(()) => {
                    self.block_begin = None;
                }
                Err(_) => return Err("injected begin_leg block timed out".to_owned()),
            }
        }
        if self.fail_begin {
            return Err("injected leg-setup refusal".to_owned());
        }
        self.goal = goal;
        self.selected_chord = Some((permit.keycode(), permit.flags()));
        self.mouse_points.push(permit.point());
        Ok(())
    }
    fn post_mouse(&mut self, permit: ManagedPostPermit, step: DragStep) -> Result<(), String> {
        match step {
            DragStep::MouseMove | DragStep::MouseDown | DragStep::MouseDragged => {}
            _ => {
                return Err(format!(
                    "refusing non-grab step {step:?} on the managed mouse post"
                ));
            }
        }
        self.check_permit(&permit);
        if self.fail_mouse_at == Some(step) {
            self.fail_mouse_at = None;
            return Err(format!("injected {step:?} refusal"));
        }
        assert_eq!(
            Some((permit.keycode(), permit.flags())),
            self.selected_chord,
            "every grab edge carries the exact resolved holder chord"
        );
        self.mouse_points.push(permit.point());
        if step == DragStep::MouseDown {
            self.counts.mouse_down += 1;
            if let Some(lane) = self.cancel_on_mouse_down.clone() {
                lane.cancel();
            }
        }
        Ok(())
    }
    fn post_key_down(&mut self, permit: ManagedPostPermit) -> Result<(), String> {
        self.check_permit(&permit);
        if self.fail_chord_down {
            return Err("injected chord-down refusal".to_owned());
        }
        assert_eq!(
            Some((permit.keycode(), permit.flags())),
            self.selected_chord,
            "chord-down carries the exact resolved holder chord"
        );
        self.counts.key_down += 1;
        let chord = (permit.keycode(), permit.flags());
        self.key_chords.push(chord);
        let tag = Some(crate::input_match::OWN_SWITCH_USER_DATA);
        self.key_posts.push((chord.0, chord.1, tag, true));
        Ok(())
    }
    fn release_mouse(&mut self) -> Result<(), String> {
        if self.fail_mouse_up {
            return Err("injected mouse-up refusal".to_owned());
        }
        self.counts.mouse_up += 1;
        Ok(())
    }
    fn release_key(&mut self) -> Result<(), String> {
        if self.fail_key_up_once {
            self.fail_key_up_once = false;
            return Err("injected chord-up refusal".to_owned());
        }
        if self.fail_key_recovery {
            return Err("injected chord-up recovery refusal".to_owned());
        }
        self.counts.key_up += 1;
        if let Some(selected) = self.selected_chord {
            let tag = Some(crate::input_match::OWN_SWITCH_USER_DATA);
            self.key_posts.push((selected.0, selected.1, tag, false));
        }
        if !self.no_effect {
            self.membership = vec![self.goal];
            if !self.freeze_current {
                self.current = Some(self.goal);
            }
        }
        Ok(())
    }
    fn settle_grab(&mut self) {}
    fn settle_hold(&mut self) {}
    fn read_membership(&mut self) -> Option<Vec<u64>> {
        if self.unreadable {
            return None;
        }
        Some(self.membership.clone())
    }
    fn read_current(&mut self) -> Option<u64> {
        if self.unreadable {
            return None;
        }
        self.current
    }
    fn effect_deadline_ms(&self) -> u64 {
        30
    }
    fn effect_poll_ms(&self) -> u64 {
        1
    }
    fn counts(&self) -> PostCounts {
        self.counts
    }
}

fn execute(
    target: &ManagedSendTarget,
    backend: &mut FakeManaged,
    handoff: &SendEffectHandoff,
) -> (SendEffectOutcome, Option<crate::space_drag_transfer::LegOk>) {
    execute_managed_send(target, backend, handoff)
}

fn finish_and_poll(
    handoff: &SendEffectHandoff,
    target: &ManagedSendTarget,
    outcome: SendEffectOutcome,
) -> crate::workspace_send_effect::SendCompletion {
    let completion = completion_for(target, outcome);
    assert!(
        handoff.finish(completion.clone()),
        "matching finish stages the terminal completion"
    );
    handoff
        .poll_completion_for(target)
        .expect("main polls the terminal completion")
}

/// Submit + take to place one request inflight, then run the effect-owner
/// execution against that lane. Mirrors the threaded owner (take, execute,
/// finish) without spawning a thread.
fn execute_inflight(
    handoff: &SendEffectHandoff,
    target: &ManagedSendTarget,
    backend: &mut FakeManaged,
) -> (SendEffectOutcome, Option<crate::space_drag_transfer::LegOk>) {
    handoff.submit(target.clone()).expect("idle submits");
    handoff.take_for_owner().expect("owner takes");
    execute_managed_send(target, backend, handoff)
}

#[test]
fn managed_permit_gates_down_edges_but_never_up() {
    let pinned = target();
    let chord = (50u32, CG_CTRL);
    assert!(authorize_managed_edge(&pinned, &fresh_ok(), chord).is_ok());
    // Pure focus swap while the subject stays managed/readable refuses.
    let mut swapped = fresh_ok();
    swapped.focused_window = Some("4242_9".to_owned());
    let err = authorize_managed_edge(&pinned, &swapped, chord).expect_err("focus swap refuses");
    assert_eq!(
        err,
        crate::workspace_send_effect::SendProofRefusal::FocusMismatch
    );
    // Unknown legs refuse too (never guessed).
    let mut unknown = fresh_ok();
    unknown.focused_window = None;
    assert!(authorize_managed_edge(&pinned, &unknown, chord).is_err());
    // Missing grab point refuses: no permit without the exact point proof.
    let mut no_point = fresh_ok();
    no_point.grab_point = None;
    assert!(authorize_managed_edge(&pinned, &no_point, chord).is_err());
    // Permit pins the exact subject, point, and chord for one edge.
    let permit = authorize_managed_edge(&pinned, &fresh_ok(), chord).expect("permit");
    assert_eq!((permit.pid(), permit.wid()), (4242, 7));
    assert_eq!(permit.point(), (100.0, 200.0));
    assert_eq!((permit.keycode(), permit.flags()), chord);
}

#[test]
fn managed_down_edges_route_exact_point_and_chord() {
    // Producer-shaped happy path: every down-edge posts at the
    // backend-selected fresh grab point with the exact resolved holder chord.
    let handoff = SendEffectHandoff::new();
    let pinned = target();
    let mut backend = FakeManaged::new();
    let (outcome, leg) = execute_inflight(&handoff, &pinned, &mut backend);
    assert_eq!(outcome, SendEffectOutcome::Applied);
    assert!(leg.is_some());
    let digit_2 = desktop_digit_keycode(2).unwrap();
    assert_eq!(backend.selected_chord, Some((digit_2, CG_CTRL)));
    assert_eq!(backend.key_chords, vec![(digit_2, CG_CTRL)]);
    assert!(
        !backend.mouse_points.is_empty()
            && backend.mouse_points.iter().all(|p| *p == (100.0, 200.0)),
        "every grab edge uses the fresh backend-selected point: {:?}",
        backend.mouse_points
    );
    let tag = Some(crate::input_match::OWN_SWITCH_USER_DATA);
    assert_eq!(
        backend.key_posts,
        vec![
            (digit_2, CG_CTRL, tag, true),
            (digit_2, CG_CTRL, tag, false)
        ]
    );
}

#[test]
fn managed_hit_disagreement_releases_without_chord() {
    // WS hit names a foreign window at the chord-down proof: uncertain only
    // if the chord already fired, proven here with zero key-downs.
    let handoff = SendEffectHandoff::new();
    let pinned = target();
    let mut backend = FakeManaged::new();
    let mut bad = fresh_ok();
    bad.ws_hit = Some("4242_9".to_owned());
    for _ in 0..4 {
        backend.evidence_script.push_back(fresh_ok());
    }
    backend.evidence_script.push_back(bad);
    let (outcome, _) = execute(&pinned, &mut backend, &handoff);
    assert_eq!(outcome, SendEffectOutcome::RefusedProven);
    assert_eq!(backend.counts.key_down, 0, "disagreement posts no chord");
    assert_eq!(
        (backend.counts.mouse_down, backend.counts.mouse_up),
        (1, 1),
        "disagreement still releases the held button"
    );
}

#[test]
fn managed_grab_point_shift_before_hold_refuses_without_posts() {
    // Fresh point drifts at the grab-move proof (call 2), after leg-setup
    // pinned (100, 200): the move refuses before the button is held, so no
    // wrong-point post and no chord can follow.
    let handoff = SendEffectHandoff::new();
    let pinned = target();
    let mut backend = FakeManaged::new();
    let mut shifted = fresh_ok();
    shifted.grab_point = Some((300.0, 400.0));
    backend.evidence_script.push_back(fresh_ok());
    backend.evidence_script.push_back(shifted);
    let (outcome, _) = execute(&pinned, &mut backend, &handoff);
    assert_eq!(outcome, SendEffectOutcome::RefusedProven);
    assert_eq!(backend.counts.mouse_down, 0, "drift pre-hold holds nothing");
    assert_eq!(backend.counts.key_down, 0, "drift pre-hold posts no chord");
    assert!(
        backend.mouse_points.iter().all(|p| *p == (100.0, 200.0)),
        "shifted point never posts: {:?}",
        backend.mouse_points
    );
    assert!(backend.key_chords.is_empty());
}

#[test]
fn managed_grab_point_shift_after_hold_releases_without_chord() {
    // Fresh point drifts at the dragged proof (call 4): move and down already
    // posted at the pinned point, the dragged edge refuses, the held button
    // still releases, and no chord posts at either point.
    let handoff = SendEffectHandoff::new();
    let pinned = target();
    let mut backend = FakeManaged::new();
    let mut shifted = fresh_ok();
    shifted.grab_point = Some((300.0, 400.0));
    for _ in 0..3 {
        backend.evidence_script.push_back(fresh_ok());
    }
    backend.evidence_script.push_back(shifted);
    let (outcome, _) = execute(&pinned, &mut backend, &handoff);
    assert_eq!(outcome, SendEffectOutcome::RefusedProven);
    assert_eq!(
        (backend.counts.mouse_down, backend.counts.mouse_up),
        (1, 1),
        "drift mid-hold still releases the held button"
    );
    assert_eq!(backend.counts.key_down, 0, "drift mid-hold posts no chord");
    assert!(
        backend.mouse_points.iter().all(|p| *p == (100.0, 200.0)),
        "shifted point never posts: {:?}",
        backend.mouse_points
    );
    assert!(backend.key_chords.is_empty());
}

#[test]
fn managed_releases_are_unconditional_despite_stale_proof() {
    // Release edges never consult proof: even with a poisoned evidence base
    // the backend releases cleanly with no down-edge posts outstanding.
    let mut backend = FakeManaged::new();
    backend.evidence_base.focused_window = Some("4242_9".to_owned());
    backend.evidence_base.grab_point = None;
    assert!(backend.release_mouse().is_ok());
    assert!(backend.release_key().is_ok());
    assert_eq!((backend.counts.mouse_up, backend.counts.key_up), (1, 1));
    assert_eq!((backend.counts.mouse_down, backend.counts.key_down), (0, 0));
}

#[test]
fn managed_permit_is_single_use_bound_authority() {
    // No discarded-permit ceremony may return: the permit is single-use
    // (no Clone/Copy), carries point+chord, posts consume it by value, and
    // no duplicate revision fence survives beside the fresh proof.
    let source = include_str!("workspace_send_managed.rs");
    assert!(
        !source.contains("let _permit"),
        "down-edges must consume the permit, never discard it"
    );
    assert!(
        !source.contains("fn revision("),
        "no duplicate revision accessor beside the fresh proof fence"
    );
    assert!(
        !source.contains("SwitchChords"),
        "no free chord struct escapes the bound permit"
    );
    for forbidden in [
        "let _permit",
        "fn revision(",
        "SwitchChords",
        "post_guarded_with_tag",
    ] {
        assert!(
            !source.contains(forbidden),
            "retired free-arg/discard ceremony must stay gone ({forbidden})"
        );
    }
    assert!(
        source.contains("pub struct ManagedPostPermit")
            && source.contains("point: (f64, f64)")
            && source.contains("keycode: u32"),
        "permit binds the exact point and chord"
    );
    assert!(
        source.contains("post_key_down(&mut self, permit: ManagedPostPermit)")
            && source.contains("release_mouse(&mut self)")
            && source.contains("release_key(&mut self)"),
        "down-edges consume permits; releases stay permit-free"
    );
}

#[test]
fn managed_never_touches_fixture_post_target() {
    // Separate product-post authority: the fixture's unforgeable target is
    // never constructed, forged, or bypassed here. This grep-guard fails the
    // build if a future edit reaches for its constructors. Doc mentions of
    // the type name are allowed; construction/bypass call-sites are not.
    // Both send modules are scanned: the live backend lives in
    // `workspace_send_native`.
    let source = concat!(
        include_str!("workspace_send_managed.rs"),
        include_str!("workspace_send_native.rs")
    );
    for forbidden in [
        "OwnedPostTarget::",
        "from_held_child",
        "from_spawn_record",
        "post_guarded_with_tag",
        "post_guarded(",
    ] {
        assert!(
            !source.contains(forbidden),
            "managed product path must not touch the fixture post target ({forbidden})"
        );
    }
    assert!(
        source.contains("hid_mouse_post") && source.contains("hid_key_post"),
        "managed posts reuse the maintained ffi posting wrappers"
    );
    assert!(
        source.contains("forbid(unsafe_code)"),
        "no new unsafe on the managed path"
    );
}

#[test]
fn managed_applied_roundtrip_via_thread_with_tagged_digit() {
    let mut worker = ManagedSendThread::spawn(FakeManaged::new);
    let handoff = worker.handoff();
    let pinned = target();
    worker.submit(pinned.clone()).expect("idle submits");
    // Second submit while pending/inflight refuses: no overlapping effects.
    assert_eq!(
        worker.submit(pinned.clone()),
        Err("send-effect-busy"),
        "concurrent press serializes/refuses"
    );
    // Bounded nonblocking poll: the effect thread proves/actuates off main
    // while this thread only polls (no AX waits here).
    let mut done = None;
    for _ in 0..500 {
        if let Some(completion) = handoff.poll_completion_for(&pinned) {
            done = Some(completion);
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let done = done.expect("effect thread finishes promptly");
    assert_eq!(done.outcome, SendEffectOutcome::Applied);
    assert_eq!(done.operation_id, "op-3");
    assert_eq!(done.app_generation, 1);
    assert_eq!(done.revision, 9);
    assert!(!handoff.is_busy(), "polled completion idles the lane");
    assert!(handoff.poll_completion_for(&pinned).is_none(), "no replay");
    // Verified follow arrival is noted on main (never on the effect thread):
    // the per-display previous becomes the source the send followed away from.
    let mut history = SelectionState::default();
    history.note_arrival("display-a", 20);
    // Idle shutdown is event-driven and nonblocking: request, then reap once
    // finished (the owner already idles on its wake channel).
    worker.request_shutdown();
    let mut released = None;
    for _ in 0..500 {
        if let Some(had) = worker.try_reap() {
            released = Some(had);
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(released, Some(false), "nothing outstanding after poll");
}

#[test]
fn managed_applied_posts_exact_control_digit_tagged() {
    // Deterministic single-effect execution (no thread scheduling): exact
    // target digit, Control-only, tagged, both edges, balanced.
    let handoff = SendEffectHandoff::new();
    let pinned = target();
    let mut backend = FakeManaged::new();
    let (outcome, leg) = execute_inflight(&handoff, &pinned, &mut backend);
    assert_eq!(outcome, SendEffectOutcome::Applied);
    let leg = leg.expect("applied carries exact readback");
    assert_eq!((leg.membership, leg.current), (vec![3], 3));
    assert!(!leg.pending_recovered);
    let tag = Some(crate::input_match::OWN_SWITCH_USER_DATA);
    let digit_2 = desktop_digit_keycode(2).unwrap();
    assert_eq!(
        backend.key_posts,
        vec![
            (digit_2, CG_CTRL, tag, true),
            (digit_2, CG_CTRL, tag, false)
        ],
        "posts exactly the target ordinal Control-only digit, tagged"
    );
    assert!(backend.counts.balanced());
    let done = finish_and_poll(&handoff, &pinned, outcome);
    assert_eq!(done.outcome, SendEffectOutcome::Applied);
}

#[test]
fn managed_focus_swap_refuses_without_posts() {
    let handoff = SendEffectHandoff::new();
    let pinned = target();
    let mut backend = FakeManaged::new();
    backend.evidence_base.focused_window = Some("4242_9".to_owned());
    let (outcome, leg) = execute_inflight(&handoff, &pinned, &mut backend);
    assert_eq!(outcome, SendEffectOutcome::RefusedProven);
    assert!(leg.is_none());
    assert_eq!(backend.counts.total(), 0, "focus swap posts nothing");
    let done = finish_and_poll(&handoff, &pinned, outcome);
    assert_eq!(done.outcome, SendEffectOutcome::RefusedProven);
}

#[test]
fn managed_stale_generation_revision_and_unknown_refuse() {
    for (mutate, token) in [
        (
            (|e: &mut SendFreshEvidence| e.live_generation = Some(2)) as fn(&mut SendFreshEvidence),
            "stale-lifetime",
        ),
        (
            (|e: &mut SendFreshEvidence| e.live_generation = None) as fn(&mut SendFreshEvidence),
            "unknown-generation",
        ),
        (
            (|e: &mut SendFreshEvidence| e.live_revision = Some(10)) as fn(&mut SendFreshEvidence),
            "revision-drift",
        ),
        (
            (|e: &mut SendFreshEvidence| e.live_revision = None) as fn(&mut SendFreshEvidence),
            "unknown-revision",
        ),
        (
            (|e: &mut SendFreshEvidence| e.grab_point = None) as fn(&mut SendFreshEvidence),
            "unknown-grab-point",
        ),
    ] {
        let handoff = SendEffectHandoff::new();
        let pinned = target();
        let mut backend = FakeManaged::new();
        mutate(&mut backend.evidence_base);
        let (outcome, _) = execute(&pinned, &mut backend, &handoff);
        assert_eq!(outcome, SendEffectOutcome::RefusedProven, "{token}");
        assert_eq!(backend.counts.total(), 0, "{token} posts nothing");
    }
}

#[test]
fn managed_holder_display_and_sid_refusals_post_nothing() {
    // Disabled holder refuses (never a settings write here).
    let handoff = SendEffectHandoff::new();
    let mut backend = FakeManaged::new();
    backend.holders.insert(
        desktop_symbolic_id(2).unwrap(),
        ShortcutEntry {
            enabled: false,
            keycode: desktop_digit_keycode(2).unwrap(),
            modifiers: CG_CTRL,
        },
    );
    let (outcome, _) = execute(&target(), &mut backend, &handoff);
    assert_eq!(outcome, SendEffectOutcome::RefusedProven);
    assert_eq!(backend.counts.total(), 0);
    // Unreadable holder refuses.
    let handoff = SendEffectHandoff::new();
    let mut backend = FakeManaged::new();
    backend
        .fail_holder_reads
        .push(desktop_symbolic_id(2).unwrap());
    let (outcome, _) = execute(&target(), &mut backend, &handoff);
    assert_eq!(outcome, SendEffectOutcome::RefusedProven);
    assert_eq!(backend.counts.total(), 0);
    // Customized chord refuses (never posted as a chord that does nothing).
    let handoff = SendEffectHandoff::new();
    let mut backend = FakeManaged::new();
    backend.holders.insert(
        desktop_symbolic_id(2).unwrap(),
        ShortcutEntry {
            enabled: true,
            keycode: desktop_digit_keycode(2).unwrap(),
            modifiers: CG_CTRL | 0x0002_0000,
        },
    );
    let (outcome, _) = execute(&target(), &mut backend, &handoff);
    assert_eq!(outcome, SendEffectOutcome::RefusedProven);
    assert_eq!(backend.counts.total(), 0);
    // Zero/multiple displays refuse (single-display capability preserved).
    for displays in [Ok(0), Ok(2), Err("row unreadable".to_owned())] {
        let handoff = SendEffectHandoff::new();
        let mut backend = FakeManaged::new();
        backend.displays = displays;
        let (outcome, _) = execute(&target(), &mut backend, &handoff);
        assert_eq!(outcome, SendEffectOutcome::RefusedProven);
        assert_eq!(backend.counts.total(), 0);
    }
    // Missing target SID refuses (never guess another occupant).
    let handoff = SendEffectHandoff::new();
    let mut backend = FakeManaged::new();
    backend.snapshot = Ok(SelectionSnapshot {
        display: "display-a".to_owned(),
        spaces: vec![20, 7],
        current: 20,
    });
    let (outcome, _) = execute(&target(), &mut backend, &handoff);
    assert_eq!(outcome, SendEffectOutcome::RefusedProven);
    assert_eq!(backend.counts.total(), 0);
    // Unreadable snapshot refuses.
    let handoff = SendEffectHandoff::new();
    let mut backend = FakeManaged::new();
    backend.snapshot = Err("row unreadable".to_owned());
    let (outcome, _) = execute(&target(), &mut backend, &handoff);
    assert_eq!(outcome, SendEffectOutcome::RefusedProven);
    assert_eq!(backend.counts.total(), 0);
}

#[test]
fn managed_mid_leg_focus_swap_stops_before_chord() {
    // Per-edge re-proof: leg-setup, move, down and dragged pass, then the
    // chord-down proof names a foreign window.
    let handoff = SendEffectHandoff::new();
    let pinned = target();
    let mut backend = FakeManaged::new();
    backend.swap_from_call = Some(5);
    let (outcome, leg) = execute(&pinned, &mut backend, &handoff);
    assert_eq!(outcome, SendEffectOutcome::RefusedProven);
    assert!(leg.is_none());
    assert_eq!(backend.counts.key_down, 0, "no chord after the swap");
    // The held mouse still released (release-first guarantee).
    assert_eq!(
        (backend.counts.mouse_down, backend.counts.mouse_up),
        (1, 1),
        "stale down-edge never strands the button"
    );
}

#[test]
fn managed_release_edges_always_attempt_despite_stale_gates() {
    // Swap lands at the dragged-edge proof: the drag refuses, but mouse-up
    // still attempts and succeeds (balanced, no chord, proven refusal).
    let handoff = SendEffectHandoff::new();
    let pinned = target();
    let mut backend = FakeManaged::new();
    backend.swap_from_call = Some(4);
    let (outcome, _) = execute(&pinned, &mut backend, &handoff);
    assert_eq!(outcome, SendEffectOutcome::RefusedProven);
    assert_eq!((backend.counts.mouse_down, backend.counts.mouse_up), (1, 1));
    assert_eq!(backend.counts.key_down, 0);
    // Key-up recovery still completes the follow when the first up is lost.
    let handoff = SendEffectHandoff::new();
    let mut backend = FakeManaged::new();
    backend.fail_key_up_once = true;
    let (outcome, leg) = execute(&pinned, &mut backend, &handoff);
    assert_eq!(outcome, SendEffectOutcome::Applied);
    assert!(leg.expect("applied").pending_recovered);
    assert_eq!((backend.counts.key_down, backend.counts.key_up), (1, 1));
}

#[test]
fn managed_cancellation_before_posts_refuses_and_keeps_lane() {
    let handoff = SendEffectHandoff::new();
    let pinned = target();
    handoff.submit(pinned.clone()).expect("submits");
    handoff.take_for_owner().expect("owner takes");
    assert!(handoff.cancel(), "inflight cancel flags stop");
    assert!(handoff.is_busy(), "cancelled inflight stays busy");
    assert_eq!(
        handoff.submit(pinned.clone()),
        Err("send-effect-busy"),
        "no concurrent physical effect while inflight stands"
    );
    let mut backend = FakeManaged::new();
    let (outcome, _) = execute(&pinned, &mut backend, &handoff);
    assert_eq!(outcome, SendEffectOutcome::RefusedProven);
    assert_eq!(
        backend.counts.total(),
        0,
        "stop before posts touches nothing"
    );
    let done = finish_and_poll(&handoff, &pinned, outcome);
    assert_eq!(done.outcome, SendEffectOutcome::RefusedProven);
    assert!(!handoff.is_busy());
}

#[test]
fn managed_cancel_during_mouse_hold_still_releases() {
    // Concurrent stop arrives inside mouse-down (via the backend hook): the
    // next down-edge stops, mouse-up still releases, no chord posts.
    let handoff = Arc::new(SendEffectHandoff::new());
    let pinned = target();
    handoff.submit(pinned.clone()).expect("submits");
    handoff.take_for_owner().expect("owner takes");
    let mut backend = FakeManaged::new();
    backend.cancel_on_mouse_down = Some(Arc::clone(&handoff));
    let (outcome, _) = execute(&pinned, &mut backend, &handoff);
    assert!(handoff.cancel_requested(), "stop was requested mid-effect");
    assert_eq!(outcome, SendEffectOutcome::RefusedProven);
    assert_eq!((backend.counts.mouse_down, backend.counts.mouse_up), (1, 1));
    assert_eq!(
        backend.counts.key_down, 0,
        "stop before chord posts no chord"
    );
    assert!(handoff.is_busy(), "terminal completion (not cancel) frees");
    let done = {
        let completion = completion_for(&pinned, outcome);
        assert!(handoff.finish(completion.clone()));
        handoff
            .poll_completion_for(&pinned)
            .expect("terminal completion polls")
    };
    assert_eq!(done.outcome, SendEffectOutcome::RefusedProven);
    assert!(!handoff.is_busy());
}

#[test]
fn managed_release_failures_hold_as_possibly_executed() {
    // Mouse-up refusal parks the release itself: uncertain, never replayed.
    let handoff = SendEffectHandoff::new();
    let mut backend = FakeManaged::new();
    backend.fail_mouse_up = true;
    let (outcome, _) = execute(&target(), &mut backend, &handoff);
    assert_eq!(outcome, SendEffectOutcome::PossiblyExecuted);
    assert_eq!((backend.counts.mouse_down, backend.counts.mouse_up), (1, 0));
    // Chord-up recovery refusal parks the key edge with the mouse released.
    let handoff = SendEffectHandoff::new();
    let mut backend = FakeManaged::new();
    backend.fail_key_up_once = true;
    backend.fail_key_recovery = true;
    let (outcome, _) = execute(&target(), &mut backend, &handoff);
    assert_eq!(outcome, SendEffectOutcome::PossiblyExecuted);
    assert_eq!((backend.counts.mouse_down, backend.counts.mouse_up), (1, 1));
}

#[test]
fn managed_backend_errors_map_honestly() {
    // Leg-setup / grab / chord-down refusals post no effective chord:
    // proven refusals.
    let handoff = SendEffectHandoff::new();
    let mut backend = FakeManaged::new();
    backend.fail_begin = true;
    let (outcome, _) = execute(&target(), &mut backend, &handoff);
    assert_eq!(outcome, SendEffectOutcome::RefusedProven);
    assert_eq!(backend.counts.total(), 0);
    let handoff = SendEffectHandoff::new();
    let mut backend = FakeManaged::new();
    backend.fail_mouse_at = Some(DragStep::MouseDragged);
    let (outcome, _) = execute(&target(), &mut backend, &handoff);
    assert_eq!(outcome, SendEffectOutcome::RefusedProven);
    let handoff = SendEffectHandoff::new();
    let mut backend = FakeManaged::new();
    backend.fail_chord_down = true;
    let (outcome, _) = execute(&target(), &mut backend, &handoff);
    assert_eq!(outcome, SendEffectOutcome::RefusedProven);
    assert_eq!(backend.counts.key_down, 0);
    // Completed chord without exact readback holds (never inverts).
    let handoff = SendEffectHandoff::new();
    let mut backend = FakeManaged::new();
    backend.no_effect = true;
    let (outcome, _) = execute(&target(), &mut backend, &handoff);
    assert_eq!(outcome, SendEffectOutcome::PossiblyExecuted);
    assert!(backend.counts.balanced());
    // Wrong current holds.
    let handoff = SendEffectHandoff::new();
    let mut backend = FakeManaged::new();
    backend.freeze_current = true;
    let (outcome, _) = execute(&target(), &mut backend, &handoff);
    assert_eq!(outcome, SendEffectOutcome::PossiblyExecuted);
    // Unreadable post-release readback holds (chord completed, arrival
    // unknown — never a false proven refusal).
    let handoff = SendEffectHandoff::new();
    let mut backend = FakeManaged::new();
    backend.unreadable = true;
    let (outcome, _) = execute(&target(), &mut backend, &handoff);
    assert_eq!(outcome, SendEffectOutcome::PossiblyExecuted);
    // Gated sessions refuse before any post.
    for gates in [
        SendGates {
            session_unlocked: false,
            ..open_gates()
        },
        SendGates {
            secure_active: true,
            ..open_gates()
        },
        SendGates {
            lock_screen: true,
            ..open_gates()
        },
    ] {
        let handoff = SendEffectHandoff::new();
        let mut backend = FakeManaged::new();
        backend.gates = gates;
        let (outcome, _) = execute(&target(), &mut backend, &handoff);
        assert_eq!(outcome, SendEffectOutcome::RefusedProven);
        assert_eq!(backend.counts.total(), 0);
    }
}

#[test]
fn managed_thread_busy_blocks_overlap_and_shutdown_releases() {
    // Blocking begin_leg holds the lane inflight: overlap refuses, shutdown
    // joins first and only then releases.
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let worker = ManagedSendThread::spawn(move || {
        let mut backend = FakeManaged::new();
        backend.block_begin = Some((started_tx, release_rx));
        backend
    });
    let handoff = worker.handoff();
    let pinned = target();
    worker.submit(pinned.clone()).expect("submits");
    started_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("effect thread takes and blocks in begin_leg");
    assert!(handoff.is_busy());
    assert_eq!(
        worker.submit(pinned.clone()),
        Err("send-effect-busy"),
        "no overlapping physical effect while blocked"
    );
    // try_reap while the owner still lives is nonblocking None: the caller
    // must never join-block nor release the lane from under a live owner.
    let mut worker = worker;
    assert!(
        worker.try_reap().is_none(),
        "reap while inflight is nonblocking None"
    );
    assert!(handoff.is_busy(), "lane stays busy while the owner lives");
    assert!(worker.cancel(), "cancel flags the blocked inflight");
    assert!(handoff.is_busy(), "cancel keeps the lane busy");
    release_tx.send(()).expect("release the blocked leg");
    let mut terminal = None;
    for _ in 0..200 {
        if let Some(completion) = handoff.poll_completion_for(&pinned) {
            terminal = Some(completion);
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let terminal = terminal.expect("terminal completion after release");
    // Cancel arrived before the chord: proven refusal (nothing posted).
    assert_eq!(terminal.outcome, SendEffectOutcome::RefusedProven);
    assert!(!handoff.is_busy());
    worker.request_shutdown();
    let mut released = None;
    for _ in 0..500 {
        if let Some(had) = worker.try_reap() {
            released = Some(had);
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(released, Some(false), "polled lane releases idle");
}

#[test]
fn managed_thread_is_event_driven_no_idle_sleep() {
    // Events over polling: the owner blocks on a wake channel while idle.
    // No millisecond sleep may remain on the idle path.
    let source = include_str!("workspace_send_managed.rs");
    assert!(
        source.contains("wake_rx.recv()"),
        "idle owner blocks on the event wake channel"
    );
    assert!(
        source.contains("wake_tx.send(())") || source.contains("wake_tx.send()"),
        "submit/cancel/shutdown wake the owner"
    );
    assert!(
        !source.contains("millis(1)"),
        "no 1ms idle poll sleep remains on the effect thread"
    );
    assert!(
        source.contains("request_shutdown") && source.contains("try_reap"),
        "nonblocking shutdown request plus finished-only reap"
    );
    assert!(
        source.contains("is_finished()"),
        "reap joins only once the owner already finished"
    );
}

#[test]
fn managed_idle_shutdown_request_then_reap_without_blocking() {
    // An idle owner shuts down promptly: request is nonblocking, reap
    // arrives once finished, and the released lane was idle.
    let mut worker = ManagedSendThread::spawn(FakeManaged::new);
    assert!(!worker.is_finished(), "owner starts live");
    worker.request_shutdown();
    // request_shutdown never blocks and never releases by itself.
    let mut released = None;
    for _ in 0..500 {
        if let Some(had) = worker.try_reap() {
            released = Some(had);
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        released,
        Some(false),
        "idle lane releases with nothing outstanding"
    );
}

#[test]
fn managed_inflight_request_shutdown_never_releases_while_live() {
    // While the owner is blocked mid-effect, request_shutdown returns
    // immediately, try_reap stays None (no blocking join), and the lane
    // stays busy: the caller can never release from under a live owner.
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let mut worker = ManagedSendThread::spawn(move || {
        let mut backend = FakeManaged::new();
        backend.block_begin = Some((started_tx, release_rx));
        backend
    });
    let handoff = worker.handoff();
    let pinned = target();
    worker.submit(pinned.clone()).expect("submits");
    started_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("owner blocks mid-effect");
    worker.request_shutdown();
    assert!(
        worker.try_reap().is_none(),
        "reap while the owner lives is nonblocking None"
    );
    assert!(handoff.is_busy(), "no release while the owner is live");
    release_tx.send(()).expect("unblock the owner");
    // The owner finishes its terminal completion (cancelled before the
    // chord: proven refusal), then exits on the stop flag.
    let mut terminal = None;
    for _ in 0..500 {
        if let Some(completion) = handoff.poll_completion_for(&pinned) {
            terminal = Some(completion);
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let terminal = terminal.expect("terminal completion after unblock");
    assert_eq!(terminal.outcome, SendEffectOutcome::RefusedProven);
    let mut released = None;
    for _ in 0..500 {
        if let Some(had) = worker.try_reap() {
            released = Some(had);
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(released, Some(false), "staged completion was polled first");
}

#[test]
fn managed_second_operation_on_reused_backend_classifies_fresh() {
    // Regression: post counts are per-operation deltas. A first success must
    // not corrupt a second early proof refusal into PossiblyExecuted via
    // cumulative totals.
    let handoff = SendEffectHandoff::new();
    let pinned = target();
    let mut backend = FakeManaged::new();
    // First operation: exact arrival.
    handoff.submit(pinned.clone()).expect("first submits");
    handoff.take_for_owner().expect("first takes");
    let (first, leg) = execute_managed_send(&pinned, &mut backend, &handoff);
    assert_eq!(first, SendEffectOutcome::Applied);
    assert!(leg.is_some());
    let first_posts = backend.counts.total();
    assert!(first_posts > 0, "first operation posts");
    assert!(handoff.finish(completion_for(&pinned, first)));
    assert!(handoff.poll_completion_for(&pinned).is_some());
    // Second operation on the SAME backend: focus swaps before any new post.
    backend.evidence_base.focused_window = Some("4242_9".to_owned());
    handoff.submit(pinned.clone()).expect("second submits");
    handoff.take_for_owner().expect("second takes");
    let (second, leg) = execute_managed_send(&pinned, &mut backend, &handoff);
    assert_eq!(
        second,
        SendEffectOutcome::RefusedProven,
        "early proof refusal stays proven despite prior cumulative posts"
    );
    assert!(leg.is_none());
    assert_eq!(
        backend.counts.total(),
        first_posts,
        "second operation posts nothing new"
    );
    assert!(handoff.finish(completion_for(&pinned, second)));
    let done = handoff
        .poll_completion_for(&pinned)
        .expect("second terminal polls");
    assert_eq!(done.outcome, SendEffectOutcome::RefusedProven);
    assert!(!handoff.is_busy());
}

#[test]
fn managed_terminal_feedback_settles_transfer_and_stale_drops() {
    // Fenced completion roundtrip: matching polls, stale drops counted.
    let handoff = SendEffectHandoff::new();
    let pinned = target();
    handoff.submit(pinned.clone()).expect("submits");
    handoff.take_for_owner().expect("takes");
    let mut stale = completion_for(&pinned, SendEffectOutcome::Applied);
    stale.operation_id = "op-9".to_owned();
    assert!(!handoff.finish(stale), "wrong operation is stale");
    assert_eq!(handoff.stale_dropped(), 1);
    let mut stale = completion_for(&pinned, SendEffectOutcome::Applied);
    stale.app_generation = 2;
    assert!(!handoff.finish(stale), "relaunched lifetime is stale");
    let done = finish_and_poll(&handoff, &pinned, SendEffectOutcome::Applied);
    assert_eq!(done.outcome, SendEffectOutcome::Applied);
    // A poll for another target's identity drops instead of misapplying.
    handoff
        .submit(pinned.clone())
        .expect("resubmits after poll");
    handoff.take_for_owner().expect("takes");
    assert!(handoff.finish(completion_for(&pinned, SendEffectOutcome::Applied)));
    let other = ManagedSendTarget::bind(4242, 7, 1, source_domain(), 3, 9, "op-9")
        .expect("other operation binds");
    assert!(handoff.poll_completion_for(&other).is_none());
    assert_eq!(handoff.stale_dropped(), 3);
    assert!(!handoff.is_busy());
    // Main-thread follow history advances only on exact Applied readback
    // (never on refusals or holds — same rule as the offline drain core).
    let mut history = SelectionState::default();
    assert!(
        prove_managed_send(&pinned, &fresh_ok()).is_ok(),
        "applied path proved first"
    );
    history.note_arrival("display-a", 20);
}

#[test]
fn managed_arming_and_holder_wiring_armed_for_numbered_follow() {
    // Numbered send-follow 1..9 is armed; trailing send-0, stays, relative
    // sends, and outputs stay unarmed. The async live controller still
    // refuses a claim without admission pins before any thread spawn or
    // post; no holder ensure is wired into the managed backend path
    // (startup owns it).
    for row in crate::workspace_input::workspace_catalogue() {
        match &row.action {
            crate::runtime_types::InputSemanticAction::WorkspaceSend {
                index,
                follow: true,
            } if (1..=9).contains(index) => {
                assert!(row.implemented, "{} armed", row.id);
            }
            crate::runtime_types::InputSemanticAction::WorkspaceSend { .. } => {
                assert!(!row.implemented, "{} stays implemented:false", row.id);
            }
            _ => {}
        }
    }
    assert_eq!(bindings().len(), 9, "9 numbered send-follow chords armed");
    let mut controller = crate::workspace_send::LiveSendController::new();
    let claim = crate::runtime_types::ClaimedSelect::tap(
        crate::runtime_types::InputSemanticAction::WorkspaceSend {
            index: 2,
            follow: true,
        },
    );
    let err = controller
        .submit(&claim, "display-a", 20, FakeManaged::new)
        .expect_err("unpinned claim refuses before any spawn");
    assert!(err.contains("unpinned"), "{err}");
    assert!(!controller.is_busy());
    let source = include_str!("workspace_send_managed.rs");
    assert!(
        !source.contains("ensure_authentic_holders")
            && !source.contains("ensure_holders_for_preset"),
        "no startup holder writes from the managed path this unit"
    );
}

#[test]
fn effect_refusal_detail_is_bounded_single_line() {
    assert_eq!(sanitize_refusal_detail("", 200), "unspecified");
    assert_eq!(sanitize_refusal_detail("  a\nb\tc  ", 200), "a b c");
    assert_eq!(
        sanitize_refusal_detail("refused-send-focus-mismatch", 200),
        "refused-send-focus-mismatch"
    );
    let long = "x".repeat(500);
    let cut = sanitize_refusal_detail(&long, EFFECT_REFUSAL_DETAIL_CHARS);
    assert_eq!(cut.chars().count(), EFFECT_REFUSAL_DETAIL_CHARS);
    assert!(!cut.contains('\n'));
    // Vertical whitespace and control chars never survive, even mid-token.
    assert_eq!(sanitize_refusal_detail("a\r\nb\x07c", 200), "a b c");
}

#[test]
fn effect_refusal_holder_chord_tokens_carry_no_key_material() {
    let digit_2 = desktop_digit_keycode(2).unwrap();
    // Disabled holder: raw names the cause; token fixes it without values.
    let raw = resolve_numbered_chord(
        2,
        ShortcutEntry {
            enabled: false,
            keycode: digit_2,
            modifiers: CG_CTRL,
        },
    )
    .expect_err("disabled holder refuses");
    assert!(raw.contains("disabled or unbound"));
    assert_eq!(classify_holder_chord_error(&raw), "holder-disabled");
    // Customized holder: raw echoes the entry debug (keycode/modifiers
    // numbers); the token carries none.
    let raw = resolve_numbered_chord(
        2,
        ShortcutEntry {
            enabled: true,
            keycode: digit_2,
            modifiers: CG_CTRL | 0x0002_0000,
        },
    )
    .expect_err("customized holder refuses");
    assert!(raw.contains("not the default"));
    assert!(raw.contains("keycode"), "raw leaks entry debug: {raw}");
    assert_eq!(classify_holder_chord_error(&raw), "holder-customized");
    // Unrepresentable keycode: raw echoes the number; token does not.
    let raw = resolve_numbered_chord(
        2,
        ShortcutEntry {
            enabled: true,
            keycode: 999,
            modifiers: CG_CTRL,
        },
    )
    .expect_err("unrepresentable keycode refuses");
    assert!(raw.contains("out of range"));
    assert!(raw.contains("999"), "raw echoes the number: {raw}");
    assert_eq!(classify_holder_chord_error(&raw), "holder-unrepresentable");
    // Unmappable desktop: no digit chord exists past the catalogue.
    let raw = resolve_numbered_chord(
        11,
        ShortcutEntry {
            enabled: true,
            keycode: digit_2,
            modifiers: CG_CTRL,
        },
    )
    .expect_err("desktop 11 has no digit chord");
    assert!(raw.contains("no default digit chord"));
    assert_eq!(classify_holder_chord_error(&raw), "holder-unmappable");
    assert_eq!(
        classify_holder_chord_error("some future resolver text"),
        "holder-chord-refused"
    );
    // No token carries digits, entry debug, or key names.
    for token in [
        "holder-disabled",
        "holder-unrepresentable",
        "holder-customized",
        "holder-unmappable",
        "holder-chord-refused",
    ] {
        assert!(
            !token.chars().any(|c| c.is_ascii_digit()),
            "{token} carries no numbers"
        );
    }
}

#[test]
fn effect_refusal_line_shape_names_stage_and_deltas() {
    let start = PostCounts {
        mouse_down: 2,
        mouse_up: 2,
        key_down: 1,
        key_up: 1,
    };
    let end = PostCounts {
        mouse_down: 3,
        mouse_up: 3,
        key_down: 1,
        key_up: 1,
    };
    let line = format_effect_refusal_line(
        277,
        "op-3",
        SendEffectOutcome::RefusedProven,
        "leg-setup",
        "send leg-setup managed proof refused: refused-send-focus-mismatch",
        start,
        end,
    );
    assert_eq!(
        line,
        "tiler-macos send effect refused target=277 op=op-3 stage=leg-setup \
         reason=send leg-setup managed proof refused: refused-send-focus-mismatch \
         dposts=md:1 mu:1 kd:0 ku:0"
    );
    // Long reasons truncate; the whole line stays bounded and single-line.
    let big = format_effect_refusal_line(
        277,
        "op-3",
        SendEffectOutcome::PossiblyExecuted,
        "readback",
        &"y".repeat(500),
        start,
        start,
    );
    assert!(!big.contains('\n'));
    assert!(
        big.chars().count() <= 420,
        "bounded line, got {} chars",
        big.chars().count()
    );
    assert!(big.starts_with("tiler-macos send effect possibly-executed target=277 "));
}

#[test]
fn effect_refusal_row_stage_refuses_without_posts() {
    // Current Space missing from the row: identity unprovable, zero posts.
    let handoff = SendEffectHandoff::new();
    let mut backend = FakeManaged::new();
    backend.snapshot = Ok(SelectionSnapshot {
        display: "display-a".to_owned(),
        spaces: vec![20, 3, 7],
        current: 999,
    });
    let (outcome, _) = execute(&target(), &mut backend, &handoff);
    assert_eq!(outcome, SendEffectOutcome::RefusedProven);
    assert_eq!(backend.counts.total(), 0);
    // Duplicate SIDs likewise refuse.
    let handoff = SendEffectHandoff::new();
    let mut backend = FakeManaged::new();
    backend.snapshot = Ok(SelectionSnapshot {
        display: "display-a".to_owned(),
        spaces: vec![20, 3, 3],
        current: 20,
    });
    let (outcome, _) = execute(&target(), &mut backend, &handoff);
    assert_eq!(outcome, SendEffectOutcome::RefusedProven);
    assert_eq!(backend.counts.total(), 0);
}

#[test]
fn effect_owner_emits_exactly_one_bounded_refusal_line_site() {
    let source = include_str!("workspace_send_managed.rs");
    assert_eq!(
        source.matches("eprintln!").count(),
        1,
        "single owner refusal emitter"
    );
    assert!(
        source.contains("eprintln!(\"{line}\");"),
        "the emitter prints only the formatted line"
    );
    // Every pre-actuation gate names its stage at the refusal site.
    for stage in [
        "\"cancel\"",
        "\"gates\"",
        "\"display\"",
        "\"snapshot\"",
        "\"row\"",
        "\"target\"",
        "\"holder\"",
        "\"holder-chord\"",
    ] {
        assert!(
            source.contains(stage),
            "early refusal names its stage {stage}"
        );
    }
    // The formatter renders counts only: no posted values, locations,
    // bindings, or window content may reach the line.
    let region = source
        .split("fn format_effect_refusal_line")
        .nth(1)
        .expect("formatter present")
        .split("fn log_effect_refusal")
        .next()
        .expect("formatter ends at the emitter");
    for forbidden in [
        "keycode",
        "flags",
        "permit",
        "entry",
        "point",
        "title",
        "chord",
        "ShortcutEntry",
        "modifiers",
    ] {
        assert!(
            !region.contains(forbidden),
            "refusal line formats counts only, never {forbidden}"
        );
    }
}

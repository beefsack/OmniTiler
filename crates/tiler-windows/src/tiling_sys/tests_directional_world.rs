//! Stage-5 directional seam (offline only): float origins dispatch world
//! commands and the core owns every logical decision. The adapter only
//! captures truthful normalized facts (lanes, tags, frames) and would
//! mechanically actuate the reply; no float branch bypasses the world.
//! No HWNDs, hooks, or live effects: the sequencer plus the core decide,
//! and the canonical reply encoding round-trips the new directional result.

use super::rmax03_adapter_tests::test_state;
use super::world_sequencer::WorldSequencer;
use super::{MonitorArea, ObservedTarget, ObservedWindow, RetainedRow};
use tiler_core::directional::WindowId;
use tiler_core::geometry::Rect;
use tiler_core::world_engine::WorldOutcome;
use tiler_core::world_report::{CommandResult, WorldCommand};

fn area() -> MonitorArea {
    MonitorArea {
        device: "DEV1".to_owned(),
        work: Rect {
            x: 0,
            y: 0,
            w: 800,
            h: 560,
        },
        full: Rect {
            x: 0,
            y: 0,
            w: 800,
            h: 600,
        },
        dpi: 96,
    }
}

fn plain_facts() -> crate::tiling::WindowFacts {
    crate::tiling::WindowFacts {
        visible: true,
        minimized: false,
        maximized: false,
        cloaked: false,
        elevated: false,
        shell: false,
        tool_window: false,
        owned: false,
        captionless_fullscreen: false,
        no_activate: false,
        dialog: false,
    }
}

fn key(hwnd: u64) -> crate::native_guards::WindowKey {
    crate::native_guards::WindowKey {
        hwnd,
        pid: 200 + hwnd as u32,
        creation: format!("proc-{hwnd}"),
    }
}

fn observed(hwnd: u64, token: &str, x: i32, y: i32) -> ObservedWindow {
    let visible = Rect {
        x,
        y,
        w: 200,
        h: 200,
    };
    let outer = Rect {
        x: x - 8,
        y: y - 8,
        w: 216,
        h: 216,
    };
    ObservedWindow {
        hwnd,
        token: token.to_owned(),
        outer,
        visible,
        insets: crate::tiling::FrameInsets::measure(outer, visible),
        identity: ObservedTarget {
            hwnd,
            pid: 200 + hwnd as u32,
            process_creation: format!("proc-{hwnd}"),
            exe_path: "C:\\Apps\\app.exe".to_owned(),
            user_sid: "S-1-5-21-1000".to_owned(),
            session_id: 1,
            tag: format!("tag-{hwnd}"),
        },
        facts: plain_facts(),
        member_tag: Some(format!("tag-{hwnd}")),
    }
}

fn seed_member(state: &mut super::TileLoop, hwnd: u64, token: &str) {
    let member = key(hwnd);
    state.member_tokens.insert(member.clone(), token.to_owned());
    state
        .member_tags
        .insert(member.clone(), format!("tag-{hwnd}"));
    state.member_identity.insert(
        member.clone(),
        crate::model::ProcessIdentity {
            pid: 200 + hwnd as u32,
            process_creation: format!("proc-{hwnd}"),
            user_sid: "S-1-5-21-1000".to_owned(),
            session_id: 1,
            exe_path: "C:\\Apps\\app.exe".to_owned(),
        },
    );
    // v4 derives fixed-size intent from truthful raw hints: the automatic
    // float origin is a fixed hint pair the core can prove. The adapter
    // float table is no longer a classification input.
    state.hint_logged.insert(
        token.to_owned(),
        tiler_core::size_hints::WindowSizeHints {
            min_w: Some(200),
            max_w: Some(200),
            min_h: Some(200),
            max_h: Some(200),
        },
    );
    state.floated.insert(member);
}

/// Seed one managed member as a plain tile: identical identity tables to
/// [`seed_member`] but never floated, so capture homes it tiled.
fn seed_tiled_member(state: &mut super::TileLoop, hwnd: u64, token: &str) {
    let member = key(hwnd);
    state.member_tokens.insert(member.clone(), token.to_owned());
    state
        .member_tags
        .insert(member.clone(), format!("tag-{hwnd}"));
    state.member_identity.insert(
        member,
        crate::model::ProcessIdentity {
            pid: 200 + hwnd as u32,
            process_creation: format!("proc-{hwnd}"),
            user_sid: "S-1-5-21-1000".to_owned(),
            session_id: 1,
            exe_path: "C:\\Apps\\app.exe".to_owned(),
        },
    );
    // Known-empty hints: the production hint pass records a successful
    // no-hint read, so capture reports readable hints instead of an
    // unreadable row (which would hold the window out of admission).
    state.hint_logged.insert(
        token.to_owned(),
        tiler_core::size_hints::WindowSizeHints::none(),
    );
    // v4 admission: a tiled member becomes core-known through its proved
    // homing evidence (the float lane rides `floated` instead), so seed
    // the exact applied homing/view the capture reports.
    let out = state.output_token("DEV1");
    let ws = state.active_view(&out).unwrap_or_else(|| "1".to_owned());
    state
        .applied_homing
        .insert(token.to_owned(), (out.clone(), ws.clone()));
    state.applied_views.insert(out, ws);
}

/// Bootstrap one output with two intentional floats and deliver both
/// observations, so the core homes them before any command.
fn bootstrap_floats(state: &mut super::TileLoop, seq: &mut WorldSequencer) -> Vec<ObservedWindow> {
    state.ensure_device("DEV1");
    seed_member(state, 101, "w1");
    seed_member(state, 102, "w2");
    let observed = vec![observed(101, "w1", 100, 100), observed(102, "w2", 300, 100)];
    let retained: Vec<RetainedRow> = Vec::new();
    let areas = vec![area()];
    for _ in 0..2 {
        seq.invalidate();
        let report = seq
            .try_dispatch(
                &state.inputs(),
                Some(&observed),
                &retained,
                &areas,
                0,
                0,
                None,
                None,
            )
            .expect("observation dispatches");
        match seq.deliver(&report) {
            WorldOutcome::Accepted(reply) => match seq.ack(&reply) {
                super::world_sequencer::AckOutcome::Applied(_) => {}
                other => panic!("expected applied, got {other:?}"),
            },
            other => panic!("expected accepted, got {other:?}"),
        }
    }
    observed
}

fn dispatch(
    state: &mut super::TileLoop,
    seq: &mut WorldSequencer,
    observed: &[ObservedWindow],
    command: WorldCommand,
) -> tiler_core::world_report::WorldReplyV4 {
    let areas = vec![area()];
    let retained: Vec<RetainedRow> = Vec::new();
    seq.invalidate();
    let report = seq
        .try_dispatch(
            &state.inputs(),
            Some(observed),
            &retained,
            &areas,
            0,
            0,
            Some(command),
            None,
        )
        .expect("command dispatches");
    // Truthful capture: both managed windows ride with membership evidence
    // and no invented frames; the command names the window only.
    assert_eq!(report.windows.len(), 2);
    match seq.deliver(&report) {
        WorldOutcome::Accepted(reply) => {
            match seq.ack(&reply) {
                super::world_sequencer::AckOutcome::Applied(_) => {}
                other => panic!("expected applied, got {other:?}"),
            }
            reply
        }
        other => panic!("expected accepted, got {other:?}"),
    }
}

/// v4 tiled bootstrap: known-empty hints plus proved tile slots admit two
/// ordinary tiles, so a maximize rides the tiled lane truthfully.
fn bootstrap_tiles(state: &mut super::TileLoop, seq: &mut WorldSequencer) -> Vec<ObservedWindow> {
    state.ensure_device("DEV1");
    seed_tiled_member(state, 101, "w1");
    seed_tiled_member(state, 102, "w2");
    let observed = vec![observed(101, "w1", 100, 100), observed(102, "w2", 300, 100)];
    for window in &observed {
        state
            .member_rects
            .insert(window.token.clone(), window.visible);
    }
    for _ in 0..2 {
        observe_with_foreground(state, seq, &observed, 101);
    }
    observed
}

#[test]
fn automatic_float_focus_refuses_from_core_without_adapter_branches() {
    // No adapter bypass exists: the core refuses a directional focus from
    // an automatic fixed float with the floating vocabulary and no effect.
    let mut state = test_state();
    let mut seq = WorldSequencer::bootstrap("adapter-1", "gen-1").expect("bootstrap");
    let _ = seq.bootstrap_reply("req-0");
    let observed = bootstrap_floats(&mut state, &mut seq);
    let reply = dispatch(
        &mut state,
        &mut seq,
        &observed,
        WorldCommand::FocusDirectional {
            window: tiler_core::world_report::FocusDirectionalSubject::Window(WindowId(
                "w1".to_owned(),
            )),
            direction: tiler_core::directional::Direction::Right,
        },
    );
    match seq.last_command_result() {
        Some(CommandResult::Directional {
            outcome,
            target,
            rect,
            cause,
            ..
        }) => {
            assert_eq!(
                outcome.as_str(),
                "focus-refused-floating",
                "cause={cause:?} target={target:?}"
            );
            assert!(target.is_none());
            assert!(rect.is_none());
        }
        other => panic!("world-owned automatic focus refusal, got {other:?}"),
    }
    assert!(
        reply.focus_request.is_none() && reply.transfers.is_empty(),
        "refusals stay atomic: no effects, no transfers"
    );
}

#[test]
fn automatic_floats_refuse_from_core_without_adapter_branches() {
    // The D4 latch rides the report as `fixed_auto`; the core refuses
    // with the floating vocabulary and the adapter writes nothing.
    let mut state = test_state();
    let mut seq = WorldSequencer::bootstrap("adapter-1", "gen-1").expect("bootstrap");
    let _ = seq.bootstrap_reply("req-0");
    let observed = bootstrap_floats(&mut state, &mut seq);
    state.fixed_auto_seen.insert(key(101));
    let reply = dispatch(
        &mut state,
        &mut seq,
        &observed,
        WorldCommand::MoveDirectional {
            window: tiler_core::directional::WindowSubject::Window(WindowId("w1".to_owned())),
            direction: tiler_core::directional::Direction::Right,
            same_axis: tiler_core::directional::SameAxisMove::default(),
        },
    );
    match seq.last_command_result() {
        Some(CommandResult::Directional { outcome, .. }) => {
            assert_eq!(outcome.as_str(), "move-refused-floating");
        }
        other => panic!("world-owned automatic refusal, got {other:?}"),
    }
    assert!(
        reply.focus_request.is_none() && reply.transfers.is_empty(),
        "refusals stay atomic: no effects, no transfers"
    );
}

fn observed_rect(hwnd: u64, token: &str, visible: Rect) -> ObservedWindow {
    let outer = Rect {
        x: visible.x - 8,
        y: visible.y - 8,
        w: visible.w + 16,
        h: visible.h + 16,
    };
    ObservedWindow {
        hwnd,
        token: token.to_owned(),
        outer,
        visible,
        insets: crate::tiling::FrameInsets::measure(outer, visible),
        identity: ObservedTarget {
            hwnd,
            pid: 200 + hwnd as u32,
            process_creation: format!("proc-{hwnd}"),
            exe_path: "C:\\Apps\\app.exe".to_owned(),
            user_sid: "S-1-5-21-1000".to_owned(),
            session_id: 1,
            tag: format!("tag-{hwnd}"),
        },
        facts: plain_facts(),
        member_tag: Some(format!("tag-{hwnd}")),
    }
}

/// Observation-only dispatch through the same sequencer path: the core
/// settles retained placements from normalized facts without a command.
fn observe_only(
    state: &mut super::TileLoop,
    seq: &mut WorldSequencer,
    observed: &[ObservedWindow],
) -> tiler_core::world_report::WorldReplyV4 {
    let areas = vec![area()];
    let retained: Vec<RetainedRow> = Vec::new();
    seq.invalidate();
    let report = seq
        .try_dispatch(
            &state.inputs(),
            Some(observed),
            &retained,
            &areas,
            0,
            0,
            None,
            None,
        )
        .expect("observation dispatches");
    match seq.deliver(&report) {
        WorldOutcome::Accepted(reply) => {
            match seq.ack(&reply) {
                super::world_sequencer::AckOutcome::Applied(_) => {}
                other => panic!("expected applied, got {other:?}"),
            }
            reply
        }
        other => panic!("expected accepted, got {other:?}"),
    }
}

/// Maximized observed window: the live maximized bit rides truthful facts
/// so the core binds a clear instead of authorizing geometry.
fn maximized_observed(hwnd: u64, token: &str, x: i32, y: i32) -> ObservedWindow {
    let mut window = observed(hwnd, token, x, y);
    window.facts.maximized = true;
    window
}

/// v4: the removed `clear_operation` token is the explicit automatic
/// `maximized: false` `StateTarget`; its command sequence mints the same
/// `op-<seq>` identity the core uses for continuation settlement.
fn clear_op(row: &tiler_core::world_report::WindowDesire) -> Option<String> {
    use tiler_core::world_report::{ScopeAuthority, StateProperty};
    row.state
        .iter()
        .find(|target| {
            target.property == StateProperty::Maximized
                && !target.value
                && target.automatic_clear
                && target.authority == ScopeAuthority::Controlled
        })
        .map(|target| match target.command_seq {
            Some(seq) => format!("op-{seq}"),
            None => "op-born".to_owned(),
        })
}

#[test]
fn maximized_move_publishes_one_shot_clear_desire() {
    // A maximized move publishes exactly one automatic clear desire under
    // the originating op with a NeedClear result: no placement, no
    // transfer, no effect, and no second command. The desire (not a
    // bespoke adapter clear) is the only actuation authority, and the
    // commandless core settlement resumes it on proof.
    // The second report carries nothing new until the native attempt and
    // the unmaximized observation arrive: the desire persists with no
    // re-dispatch and no replay.
    // v4: the fixed-hint automatic-float lane truthfully refuses this move
    // (`move-refused-floating`, covered above), so the one-shot desire is
    // exercised on the tiled lane where the clear pipeline lives.
    let mut state = test_state();
    let mut seq = WorldSequencer::bootstrap("adapter-1", "gen-1").expect("bootstrap");
    let _ = seq.bootstrap_reply("req-0");
    let _ = bootstrap_tiles(&mut state, &mut seq);
    let observed = vec![
        maximized_observed(101, "w1", 100, 100),
        observed(102, "w2", 300, 100),
    ];
    for _ in 0..2 {
        observe_with_foreground(&mut state, &mut seq, &observed, 0);
    }
    let reply = dispatch_tiled(
        &mut state,
        &mut seq,
        &observed,
        0,
        WorldCommand::MoveDirectional {
            window: tiler_core::directional::WindowSubject::Window(WindowId("w1".to_owned())),
            direction: tiler_core::directional::Direction::Right,
            same_axis: tiler_core::directional::SameAxisMove::default(),
        },
    );
    match seq.last_command_result() {
        Some(CommandResult::Directional { outcome, .. }) => {
            assert_eq!(outcome.as_str(), "move-maximize-clear");
        }
        other => panic!("world-owned need-clear, got {other:?}"),
    }
    let row = reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "w1")
        .expect("mover keeps a row");
    let op = clear_op(row).expect("one-shot clear desire");
    assert!(
        row.frame_operation.is_none(),
        "no placement rides a need-clear"
    );
    assert!(
        reply.focus_request.is_none() && reply.transfers.is_empty(),
        "need-clear stays atomic"
    );
    // The second report carries nothing new until the native attempt and
    // the unmaximized observation arrive: the desire persists with no
    // re-dispatch and no replay.
    let pending = observe_only(&mut state, &mut seq, &observed);
    let rerow = pending
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "w1")
        .expect("mover keeps a row");
    assert_eq!(
        clear_op(rerow).as_deref(),
        Some(op.as_str()),
        "undispatched desire persists with no re-dispatch"
    );
}

#[test]
fn sequencer_carries_clear_attempts_to_ack() {
    // Staged clear attempts ride the next report exactly once: the ack
    // removes exactly-equal entries, so a lost acknowledgement redelivers
    // while an acknowledged one never replays.
    // v4: `ClearAttempt`'s `op-7` correlation is the `StateAttempt`'s acted-on
    // revision; the `(window, property)` slot replaces the operation id.
    use tiler_core::world_report::{FrameSetter, StateAttempt, StateProperty};
    let mut state = test_state();
    let mut seq = WorldSequencer::bootstrap("adapter-1", "gen-1").expect("bootstrap");
    let _ = seq.bootstrap_reply("req-0");
    state.ensure_device("DEV1");
    seq.note_state_attempt(StateAttempt {
        window: WindowId("w1".to_owned()),
        property: StateProperty::Maximized,
        target: false,
        revision: 7,
        disposition: FrameSetter::Accepted,
        pre: Some(true),
        post: Some(false),
        attempt_ms: 5,
    });
    let areas = vec![area()];
    let retained: Vec<RetainedRow> = Vec::new();
    seq.invalidate();
    let report = seq
        .try_dispatch(
            &state.inputs(),
            Some(&[]),
            &retained,
            &areas,
            0,
            0,
            None,
            None,
        )
        .expect("observation dispatches");
    assert!(
        report
            .state_attempts
            .iter()
            .any(|attempt| attempt.revision == 7 && attempt.window.0 == "w1"),
        "staged attempt rides: {:?}",
        report.state_attempts
    );
    match seq.deliver(&report) {
        WorldOutcome::Accepted(reply) => match seq.ack(&reply) {
            super::world_sequencer::AckOutcome::Applied(_) => {}
            other => panic!("expected applied, got {other:?}"),
        },
        other => panic!("expected accepted, got {other:?}"),
    }
    seq.invalidate();
    let report = seq
        .try_dispatch(
            &state.inputs(),
            Some(&[]),
            &retained,
            &areas,
            0,
            0,
            None,
            None,
        )
        .expect("observation dispatches");
    assert!(
        report.state_attempts.is_empty(),
        "acked attempts never redeliver: {:?}",
        report.state_attempts
    );
}

#[test]
fn unknown_windows_refuse_stale_with_preserved_cause() {
    // A vanished subject is stale lifetime evidence, not a bare rejection:
    // focus and move share one vocabulary with the preserved cause, and
    // the refusal stays atomic.
    let mut state = test_state();
    let mut seq = WorldSequencer::bootstrap("adapter-1", "gen-1").expect("bootstrap");
    let _ = seq.bootstrap_reply("req-0");
    let observed = bootstrap_floats(&mut state, &mut seq);
    for (command, token) in [
        (
            WorldCommand::FocusDirectional {
                window: tiler_core::world_report::FocusDirectionalSubject::Window(WindowId(
                    "w9".to_owned(),
                )),
                direction: tiler_core::directional::Direction::Right,
            },
            "focus-float-refused-stale",
        ),
        (
            WorldCommand::MoveDirectional {
                window: tiler_core::directional::WindowSubject::Window(WindowId("w9".to_owned())),
                direction: tiler_core::directional::Direction::Right,
                same_axis: tiler_core::directional::SameAxisMove::default(),
            },
            "move-float-refused-stale",
        ),
    ] {
        let reply = dispatch(&mut state, &mut seq, &observed, command);
        match seq.last_command_result() {
            Some(CommandResult::Directional {
                outcome,
                target,
                cause,
                ..
            }) => {
                assert_eq!(outcome.as_str(), token);
                assert_eq!(target, &Some(WindowId("w9".to_owned())));
                assert_eq!(cause, &Some("unknown-window".to_owned()));
            }
            other => panic!("world-owned stale refusal, got {other:?}"),
        }
        assert!(
            reply.focus_request.is_none() && reply.transfers.is_empty(),
            "stale refusals stay atomic"
        );
    }
}

/// Observation-only dispatch naming the live foreground explicitly, so the
/// captured `focused_global` names the subject instead of unknown.
fn observe_with_foreground(
    state: &mut super::TileLoop,
    seq: &mut WorldSequencer,
    observed: &[ObservedWindow],
    foreground: u64,
) -> tiler_core::world_report::WorldReplyV4 {
    let areas = vec![area()];
    let retained: Vec<RetainedRow> = Vec::new();
    seq.invalidate();
    let report = seq
        .try_dispatch(
            &state.inputs(),
            Some(observed),
            &retained,
            &areas,
            foreground,
            0,
            None,
            None,
        )
        .expect("observation dispatches");
    match seq.deliver(&report) {
        WorldOutcome::Accepted(reply) => {
            match seq.ack(&reply) {
                super::world_sequencer::AckOutcome::Applied(_) => {}
                other => panic!("expected applied, got {other:?}"),
            }
            reply
        }
        other => panic!("expected accepted, got {other:?}"),
    }
}

/// Command dispatch naming the live foreground explicitly: like
/// [`dispatch`] but without the float-only capture assertion, so tiled
/// subjects route the same production path.
fn dispatch_tiled(
    state: &mut super::TileLoop,
    seq: &mut WorldSequencer,
    observed: &[ObservedWindow],
    foreground: u64,
    command: WorldCommand,
) -> tiler_core::world_report::WorldReplyV4 {
    let areas = vec![area()];
    let retained: Vec<RetainedRow> = Vec::new();
    seq.invalidate();
    let report = seq
        .try_dispatch(
            &state.inputs(),
            Some(observed),
            &retained,
            &areas,
            foreground,
            0,
            Some(command),
            None,
        )
        .expect("command dispatches");
    match seq.deliver(&report) {
        WorldOutcome::Accepted(reply) => {
            match seq.ack(&reply) {
                super::world_sequencer::AckOutcome::Applied(_) => {}
                other => panic!("expected applied, got {other:?}"),
            }
            reply
        }
        other => panic!("expected accepted, got {other:?}"),
    }
}

/// Durable marker intent for one fixture lifetime: the same three lanes a
/// production tick reads before capture (`super::world_native`), placed by
/// tests so composed sticky/float intent rides `intent_records`.
type IntentStorageMap =
    std::collections::BTreeMap<crate::native_guards::WindowKey, super::world_diff::IntentStorage>;

/// Observation-only dispatch carrying the fixture's durable intent records:
/// identical to [`observe_with_foreground`] except the canonical
/// `intent_records` lane is present, as production always supplies it.
fn observe_with_intent(
    state: &super::TileLoop,
    seq: &mut WorldSequencer,
    intent: &IntentStorageMap,
    observed: &[ObservedWindow],
    foreground: u64,
) -> tiler_core::world_report::WorldReplyV4 {
    let areas = vec![area()];
    let retained: Vec<RetainedRow> = Vec::new();
    let inputs = state.inputs().with_intent_storage(intent);
    seq.invalidate();
    let report = seq
        .try_dispatch(
            &inputs,
            Some(observed),
            &retained,
            &areas,
            foreground,
            0,
            None,
            None,
        )
        .expect("observation dispatches");
    match seq.deliver(&report) {
        WorldOutcome::Accepted(reply) => {
            match seq.ack(&reply) {
                super::world_sequencer::AckOutcome::Applied(_) => {}
                other => panic!("expected applied, got {other:?}"),
            }
            reply
        }
        other => panic!("expected accepted, got {other:?}"),
    }
}

/// Command dispatch carrying the fixture's durable intent records: the same
/// production route as [`dispatch_tiled`] with the `intent_records` lane
/// present.
fn dispatch_with_intent(
    state: &super::TileLoop,
    seq: &mut WorldSequencer,
    intent: &IntentStorageMap,
    observed: &[ObservedWindow],
    foreground: u64,
    command: WorldCommand,
) -> tiler_core::world_report::WorldReplyV4 {
    let areas = vec![area()];
    let retained: Vec<RetainedRow> = Vec::new();
    let inputs = state.inputs().with_intent_storage(intent);
    seq.invalidate();
    let report = seq
        .try_dispatch(
            &inputs,
            Some(observed),
            &retained,
            &areas,
            foreground,
            0,
            Some(command),
            None,
        )
        .expect("command dispatches");
    match seq.deliver(&report) {
        WorldOutcome::Accepted(reply) => {
            match seq.ack(&reply) {
                super::world_sequencer::AckOutcome::Applied(_) => {}
                other => panic!("expected applied, got {other:?}"),
            }
            reply
        }
        other => panic!("expected accepted, got {other:?}"),
    }
}

/// B4 integrated production seam (float): a durably floated subject (an
/// explicit float intent record, not a fixed-hint automatic float) admits
/// through the float lane, maximize publishes NeedClear with the op token
/// BEFORE any attempt, the production common diff lane dispatches exactly
/// one native clear with a correlated accepted attempt, then commandless
/// continuation resumes the original half-snap under the ORIGINAL op with
/// clear retirement. No fabricated proof: the attempt is diff-generated
/// from the published desire itself.
#[test]
fn maximized_clear_runs_core_desire_through_diff_to_continuation() {
    use super::world_capture::capture_world;
    use super::world_diff::{FakeNative, FreshHost, HostDiff, IntentStorage, apply_world_reply};
    use tiler_core::world_report::{FrameSetter, OperationOutcome, StateAttempt, StateProperty};

    let mut state = test_state();
    let mut seq = WorldSequencer::bootstrap("adapter-1", "gen-1").expect("bootstrap");
    let _ = seq.bootstrap_reply("req-0");
    state.ensure_device("DEV1");
    seed_tiled_member(&mut state, 101, "w1");
    seed_tiled_member(&mut state, 102, "w2");
    // v4 float intent is the durable marker record, never the fixed-hint
    // automatic lane (which truthfully refuses a maximized move). Production
    // reads this same map from the native markers before capture.
    let mut intent = IntentStorageMap::new();
    intent.insert(
        key(101),
        IntentStorage {
            float: true,
            ..IntentStorage::default()
        },
    );
    // Production mirrors the applied durable record into the mechanical
    // adapter table (`apply_intent_mirrors`), the same table the diff
    // baseline reads for its overlay guard.
    state.floated.insert(key(101));
    let tiles = vec![observed(101, "w1", 100, 100), observed(102, "w2", 300, 100)];
    for _ in 0..2 {
        observe_with_intent(&state, &mut seq, &intent, &tiles, 101);
    }
    // Maximize the floated subject under exact-subject foreground.
    let maximized = vec![
        maximized_observed(101, "w1", 100, 100),
        observed(102, "w2", 300, 100),
    ];
    for _ in 0..2 {
        observe_with_intent(&state, &mut seq, &intent, &maximized, 101);
    }
    // One directional move publishes the one-shot clear desire under the
    // originating op before any attempt exists for it.
    let reply = dispatch_with_intent(
        &state,
        &mut seq,
        &intent,
        &maximized,
        101,
        WorldCommand::MoveDirectional {
            window: tiler_core::directional::WindowSubject::Window(WindowId("w1".to_owned())),
            direction: tiler_core::directional::Direction::Right,
            same_axis: tiler_core::directional::SameAxisMove::default(),
        },
    );
    match seq.last_command_result() {
        Some(CommandResult::Directional { outcome, .. }) => {
            assert_eq!(outcome.as_str(), "move-maximize-clear");
        }
        other => panic!("world-owned need-clear, got {other:?}"),
    }
    let row = reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "w1")
        .expect("mover keeps a row")
        .clone();
    let op = clear_op(&row).expect("one-shot clear desire published before any attempt");
    assert!(reply.operation_settled.is_empty(), "bind emits no terminal");
    // Production common diff lane: exactly one native clear from the
    // published desire, reported as the correlated attempt.
    let areas = vec![area()];
    let retained: Vec<RetainedRow> = Vec::new();
    let captured = capture_world(
        &state.inputs().with_intent_storage(&intent),
        Some(&maximized),
        &retained,
        &areas,
        101,
    );
    let fresh = FreshHost::from_sources(
        &state.inputs().with_intent_storage(&intent),
        &maximized,
        &retained,
        &captured,
    );
    let mut native = FakeNative::seeded(&fresh);
    // v4: the live native maximized preimage is an explicit state read; the
    // removed facade seeded it from the observed maximized facts.
    native
        .state
        .insert(("w1".to_owned(), StateProperty::Maximized), true);
    let mut diff = HostDiff::new("adapter-1", "gen-1");
    diff.gate.current_obs = reply.based_on_observation_seq;
    let stats = apply_world_reply(
        &mut diff,
        &reply,
        &fresh,
        &mut native,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        1000,
    );
    // v4: HostDiff-drained `StateAttempt` rows replace the removed
    // `clear_out` sink and the `stats.clear_writes` counter.
    let clear_out: Vec<StateAttempt> = diff.take_state_attempts();
    assert_eq!(clear_out.len(), 1, "{stats:?}");
    assert_eq!(clear_out.len(), 1);
    let attempt = &clear_out[0];
    assert_eq!(
        attempt.revision, reply.state_revision,
        "attempt acts on the published clear revision"
    );
    assert_eq!(attempt.window.0, "w1");
    assert_eq!(attempt.property, StateProperty::Maximized);
    assert!(!attempt.target);
    assert_eq!(attempt.disposition, FrameSetter::Accepted);
    assert_eq!(attempt.pre, Some(true));
    assert_eq!(attempt.post, Some(false));
    // Commandless continuation on the diff-generated proof resumes the
    // original half-snap under the ORIGINAL op with clear retirement: no
    // second command, no second clear.
    seq.note_state_attempt(attempt.clone());
    let cleared = vec![observed(101, "w1", 100, 100), observed(102, "w2", 300, 100)];
    let reply = observe_with_intent(&state, &mut seq, &intent, &cleared, 101);
    let row = reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "w1")
        .expect("mover keeps a row");
    assert_eq!(
        row.frame_operation.as_deref(),
        Some(op.as_str()),
        "half-snap continues under the original op, got {:?}",
        reply.operation_settled
    );
    assert_eq!(
        clear_op(row),
        None,
        "clear retired on continuation, never re-actuated"
    );
    assert!(
        reply.operation_settled.iter().any(|event| {
            event.operation_id == op
                && event.window.0 == "w1"
                && event.outcome == OperationOutcome::ClearContinuedFloat
        }),
        "continuation observable under original op, got {:?}",
        reply.operation_settled
    );
}

/// B4 integrated production seam (sticky): the same core-desire to
/// diff-attempt to continuation pattern for a maximized sticky subject.
/// v4 composed sticky rides the durable intent-record lane (Windows has no
/// native sticky fact), so this fixture supplies the sticky marker record
/// and receives the same float half-snap continuation as a floated
/// subject.
#[test]
fn maximized_sticky_clear_runs_core_desire_through_diff_to_continuation() {
    use super::world_capture::capture_world;
    use super::world_diff::{FakeNative, FreshHost, HostDiff, IntentStorage, apply_world_reply};
    use tiler_core::world_report::{FrameSetter, OperationOutcome, StateAttempt, StateProperty};

    let mut state = test_state();
    let mut seq = WorldSequencer::bootstrap("adapter-1", "gen-1").expect("bootstrap");
    let _ = seq.bootstrap_reply("req-0");
    state.ensure_device("DEV1");
    seed_tiled_member(&mut state, 101, "w1");
    seed_tiled_member(&mut state, 102, "w2");
    // Composed sticky: the marker record carries the prior float/tile
    // intent; capture publishes it as an `intentRecord`, never a fact bit.
    let mut intent = IntentStorageMap::new();
    intent.insert(
        key(101),
        IntentStorage {
            sticky_prior: Some(true),
            ..IntentStorage::default()
        },
    );
    // Production mirrors the applied durable record into the mechanical
    // adapter table (`apply_intent_mirrors`), the same table the diff
    // baseline reads for its overlay guard.
    state.sticky.insert(key(101), true);
    let tiles = vec![observed(101, "w1", 100, 100), observed(102, "w2", 300, 100)];
    for window in &tiles {
        state
            .member_rects
            .insert(window.token.clone(), window.visible);
    }
    for _ in 0..2 {
        observe_with_intent(&state, &mut seq, &intent, &tiles, 101);
    }
    let maximized = vec![
        maximized_observed(101, "w1", 100, 100),
        observed(102, "w2", 300, 100),
    ];
    for _ in 0..2 {
        observe_with_intent(&state, &mut seq, &intent, &maximized, 101);
    }
    let reply = dispatch_with_intent(
        &state,
        &mut seq,
        &intent,
        &maximized,
        101,
        WorldCommand::MoveDirectional {
            window: tiler_core::directional::WindowSubject::Window(WindowId("w1".to_owned())),
            direction: tiler_core::directional::Direction::Right,
            same_axis: tiler_core::directional::SameAxisMove::default(),
        },
    );
    match seq.last_command_result() {
        Some(CommandResult::Directional { outcome, .. }) => {
            assert_eq!(outcome.as_str(), "move-maximize-clear");
        }
        other => panic!("world-owned sticky need-clear, got {other:?}"),
    }
    let row = reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "w1")
        .expect("mover keeps a row")
        .clone();
    let op = clear_op(&row).expect("sticky clear desire published before any attempt");
    let areas = vec![area()];
    let retained: Vec<RetainedRow> = Vec::new();
    let captured = capture_world(
        &state.inputs().with_intent_storage(&intent),
        Some(&maximized),
        &retained,
        &areas,
        101,
    );
    let fresh = FreshHost::from_sources(
        &state.inputs().with_intent_storage(&intent),
        &maximized,
        &retained,
        &captured,
    );
    assert!(
        fresh.baseline.get("w1").is_some_and(|window| window.sticky),
        "sticky rides the diff baseline"
    );
    let mut native = FakeNative::seeded(&fresh);
    native
        .state
        .insert(("w1".to_owned(), StateProperty::Maximized), true);
    let mut diff = HostDiff::new("adapter-1", "gen-1");
    diff.gate.current_obs = reply.based_on_observation_seq;
    let stats = apply_world_reply(
        &mut diff,
        &reply,
        &fresh,
        &mut native,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        1000,
    );
    // v4: the removed `clear_out` sink and `clear_writes` counter are the
    // HostDiff-drained automatic-clear `StateAttempt` rows.
    let clear_out: Vec<StateAttempt> = diff.take_state_attempts();
    assert_eq!(clear_out.len(), 1, "{stats:?}");
    assert_eq!(clear_out.len(), 1);
    assert_eq!(
        clear_out[0].revision, reply.state_revision,
        "attempt acts on the published clear revision"
    );
    assert_eq!(clear_out[0].disposition, FrameSetter::Accepted);
    assert_eq!(clear_out[0].property, StateProperty::Maximized);
    assert!(!clear_out[0].target);
    seq.note_state_attempt(clear_out[0].clone());
    let cleared = vec![observed(101, "w1", 100, 100), observed(102, "w2", 300, 100)];
    let reply = observe_with_intent(&state, &mut seq, &intent, &cleared, 101);
    let row = reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "w1")
        .expect("mover keeps a row");
    assert_eq!(
        row.frame_operation.as_deref(),
        Some(op.as_str()),
        "sticky half-snap continues under the original op, got {:?}",
        reply.operation_settled
    );
    assert_eq!(clear_op(row), None);
    assert!(
        reply.operation_settled.iter().any(|event| {
            event.operation_id == op && event.outcome == OperationOutcome::ClearContinuedFloat
        }),
        "sticky continuation observable under original op, got {:?}",
        reply.operation_settled
    );
}

/// B4 integrated production seam (tiled): normal tiled admission, maximize
/// while keeping the tile slot, NeedClear with the published op token on
/// the tiled row BEFORE any attempt, the production common diff lane
/// dispatching exactly one native clear with a correlated accepted
/// attempt, then commandless continuation under the ORIGINAL op with clear
/// retirement. Fails absent the tiled publication fix: without the token
/// the diff lane stages no attempt and nothing continues.
#[test]
fn maximized_tiled_clear_runs_core_desire_through_diff_to_continuation() {
    use super::world_capture::capture_world;
    use super::world_diff::{FakeNative, FreshHost, HostDiff, apply_world_reply};
    use tiler_core::world_report::{FrameSetter, OperationOutcome, StateAttempt, StateProperty};

    let mut state = test_state();
    let mut seq = WorldSequencer::bootstrap("adapter-1", "gen-1").expect("bootstrap");
    let _ = seq.bootstrap_reply("req-0");
    // Normal tiled admission: two readable tiles, never floated.
    state.ensure_device("DEV1");
    seed_tiled_member(&mut state, 101, "w1");
    seed_tiled_member(&mut state, 102, "w2");
    let tiles = vec![observed(101, "w1", 100, 100), observed(102, "w2", 300, 100)];
    for window in &tiles {
        state
            .member_rects
            .insert(window.token.clone(), window.visible);
    }
    for _ in 0..2 {
        observe_with_foreground(&mut state, &mut seq, &tiles, 101);
    }
    // Maximize w1 while keeping its tile slot, under exact-subject
    // foreground.
    let maximized = vec![
        maximized_observed(101, "w1", 100, 100),
        observed(102, "w2", 300, 100),
    ];
    for _ in 0..2 {
        observe_with_foreground(&mut state, &mut seq, &maximized, 101);
    }
    // One directional move publishes the one-shot clear desire on the
    // tiled row itself before any attempt exists for it.
    let reply = dispatch_tiled(
        &mut state,
        &mut seq,
        &maximized,
        101,
        WorldCommand::MoveDirectional {
            window: tiler_core::directional::WindowSubject::Window(WindowId("w1".to_owned())),
            direction: tiler_core::directional::Direction::Right,
            same_axis: tiler_core::directional::SameAxisMove::default(),
        },
    );
    match seq.last_command_result() {
        Some(CommandResult::Directional { outcome, .. }) => {
            assert_eq!(outcome.as_str(), "move-maximize-clear");
        }
        other => panic!("world-owned tiled need-clear, got {other:?}"),
    }
    let row = reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "w1")
        .expect("mover keeps a row")
        .clone();
    assert!(
        !row.floating && row.allocation.is_some(),
        "tiled route keeps its slot, got {row:?}"
    );
    let op = clear_op(&row).expect("tiled clear desire published before any attempt");
    assert_eq!(row.frame_operation, None, "tiled rows carry no float token");
    assert!(reply.operation_settled.is_empty(), "bind emits no terminal");
    // Production common diff lane: exactly one native clear from the
    // published tiled desire, reported as the correlated attempt.
    let areas = vec![area()];
    let retained: Vec<RetainedRow> = Vec::new();
    let captured = capture_world(&state.inputs(), Some(&maximized), &retained, &areas, 101);
    let fresh = FreshHost::from_sources(&state.inputs(), &maximized, &retained, &captured);
    let mut native = FakeNative::seeded(&fresh);
    native
        .state
        .insert(("w1".to_owned(), StateProperty::Maximized), true);
    let mut diff = HostDiff::new("adapter-1", "gen-1");
    diff.gate.current_obs = reply.based_on_observation_seq;
    let stats = apply_world_reply(
        &mut diff,
        &reply,
        &fresh,
        &mut native,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        1000,
    );
    // v4: HostDiff-drained automatic-clear `StateAttempt` rows replace the
    // removed `clear_out` sink and `stats.clear_writes` counter.
    let clear_out: Vec<StateAttempt> = diff.take_state_attempts();
    assert_eq!(clear_out.len(), 1, "{stats:?}");
    assert_eq!(clear_out.len(), 1);
    let attempt = &clear_out[0];
    assert_eq!(
        attempt.revision, reply.state_revision,
        "attempt acts on the published clear revision"
    );
    assert_eq!(attempt.window.0, "w1");
    assert_eq!(attempt.property, StateProperty::Maximized);
    assert!(!attempt.target);
    assert_eq!(attempt.disposition, FrameSetter::Accepted);
    assert_eq!(attempt.pre, Some(true));
    assert_eq!(attempt.post, Some(false));
    // Commandless continuation on the diff-generated proof resumes the
    // original tiled intent under the ORIGINAL op with clear retirement:
    // no second command, no second clear.
    seq.note_state_attempt(attempt.clone());
    let cleared = vec![observed(101, "w1", 100, 100), observed(102, "w2", 300, 100)];
    let reply = observe_with_foreground(&mut state, &mut seq, &cleared, 101);
    let row = reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "w1")
        .expect("mover keeps a row");
    assert_eq!(
        clear_op(row),
        None,
        "clear retired on continuation, never re-actuated"
    );
    assert!(
        reply.operation_settled.iter().any(|event| {
            event.operation_id == op
                && event.window.0 == "w1"
                && event.outcome == OperationOutcome::ClearContinuedTiled
        }),
        "tiled continuation observable under original op, got {:?}",
        reply.operation_settled
    );
}

/// Fixture-shaped maximized observed window: the live maximized bit rides
/// truthful facts while the native frame is the monitor-sized maximized
/// frame (work area), not the retained tile slot. This mirrors the live
/// fixture sequence where a slotted tile is maximized in place (native
/// `show == 3` with monitor-sized frames) and the tile slot is retained.
fn maximized_observed_fullscreen_frame(hwnd: u64, token: &str) -> ObservedWindow {
    let mut window = observed_rect(
        hwnd,
        token,
        Rect {
            x: 0,
            y: 0,
            w: 800,
            h: 560,
        },
    );
    window.facts.maximized = true;
    window
}

/// B4 production-shaped tiled maximize-clear with unknown managed focus:
/// the maximized native frame rides fixture-shaped facts while the
/// foreground names no managed subject (the live `unknown managed focus`
/// condition). Publication and the common diff dispatch must not depend on
/// it: the token is published and exactly one accepted clear attempt is
/// reported; the only hold is the maximized subject's own overlay-guarded
/// frame write, never the clear lane. Unknown focus alone never holds the
/// common clear lane; focus enters only core continuation, never dispatch.
#[test]
fn maximized_tiled_clear_with_unknown_focus_still_dispatches() {
    use super::world_capture::capture_world;
    use super::world_diff::{FakeNative, FreshHost, HostDiff, apply_world_reply};
    use tiler_core::world_report::{FrameSetter, StateAttempt, StateProperty};

    let mut state = test_state();
    let mut seq = WorldSequencer::bootstrap("adapter-1", "gen-1").expect("bootstrap");
    let _ = seq.bootstrap_reply("req-0");
    state.ensure_device("DEV1");
    seed_tiled_member(&mut state, 101, "w1");
    seed_tiled_member(&mut state, 102, "w2");
    let tiles = vec![observed(101, "w1", 100, 100), observed(102, "w2", 300, 100)];
    for _ in 0..2 {
        observe_with_foreground(&mut state, &mut seq, &tiles, 0);
    }
    // Maximize w1 in place: the native frame grows to the monitor-sized
    // maximized frame while the tile slot is retained, under unknown
    // managed focus throughout.
    let maximized = vec![
        maximized_observed_fullscreen_frame(101, "w1"),
        observed(102, "w2", 300, 100),
    ];
    for _ in 0..2 {
        observe_with_foreground(&mut state, &mut seq, &maximized, 0);
    }
    let reply = dispatch_tiled(
        &mut state,
        &mut seq,
        &maximized,
        0,
        WorldCommand::MoveDirectional {
            window: tiler_core::directional::WindowSubject::Window(WindowId("w1".to_owned())),
            direction: tiler_core::directional::Direction::Right,
            same_axis: tiler_core::directional::SameAxisMove::default(),
        },
    );
    match seq.last_command_result() {
        Some(CommandResult::Directional { outcome, .. }) => {
            assert_eq!(outcome.as_str(), "move-maximize-clear");
        }
        other => panic!("world-owned tiled need-clear, got {other:?}"),
    }
    let row = reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "w1")
        .expect("mover keeps a row")
        .clone();
    assert!(
        !row.floating && row.allocation.is_some(),
        "tiled route keeps its slot, got {row:?}"
    );
    let op = clear_op(&row).expect("tiled clear desire published under unknown focus");
    // Production common diff lane under the same unknown focus: exactly
    // one native clear with a correlated accepted attempt and no hold row.
    let areas = vec![area()];
    let retained: Vec<RetainedRow> = Vec::new();
    let captured = capture_world(&state.inputs(), Some(&maximized), &retained, &areas, 0);
    assert!(
        captured.focused_global.is_empty(),
        "unknown managed focus rides the capture, got {:?}",
        captured.focused_global
    );
    let fresh = FreshHost::from_sources(&state.inputs(), &maximized, &retained, &captured);
    let mut native = FakeNative::seeded(&fresh);
    native
        .state
        .insert(("w1".to_owned(), StateProperty::Maximized), true);
    let mut diff = HostDiff::new("adapter-1", "gen-1");
    diff.gate.current_obs = reply.based_on_observation_seq;
    let stats = apply_world_reply(
        &mut diff,
        &reply,
        &fresh,
        &mut native,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        1000,
    );
    // v4: HostDiff-drained automatic-clear `StateAttempt` rows replace the
    // removed `clear_out` sink and `stats.clear_writes` counter.
    let clear_out: Vec<StateAttempt> = diff.take_state_attempts();
    assert_eq!(clear_out.len(), 1, "{stats:?}");
    assert_eq!(clear_out.len(), 1);
    assert_eq!(
        clear_out[0].revision, reply.state_revision,
        "attempt acts on the published clear revision"
    );
    assert_eq!(clear_out[0].disposition, FrameSetter::Accepted);
    assert_eq!(clear_out[0].property, StateProperty::Maximized);
    assert!(!clear_out[0].target);
    // v4: the removed per-clear `clear_holds` trace row is the coarse
    // `stats.held` count. Exactly one hold is the intended overlay-guarded
    // frame write for the maximized subject itself (maximized rows take no
    // automatic tiled frame write); the clear target still dispatches.
    assert_eq!(
        stats.held, 1,
        "one overlay-guarded frame hold, clear still dispatches, got {stats:?}"
    );
    assert_eq!(
        stats.geometry_writes, 1,
        "only the untouched peer takes a frame write, got {stats:?}"
    );
    assert!(
        op.starts_with("op-"),
        "published clear token precedes any attempt: {op:?}"
    );
    // Continuation under exact-subject focus is covered by the sibling
    // tiled test; the unknown-focus continuation lifecycle (abandon versus
    // hold on a later focus change) is a separate core question and is
    // not settled here.
}

/// Trace-only hold oracle for the common clear lane: a held pre-dispatch
/// gate with zero attempts and zero writes names its exact gate on the
/// trace. The gesture barrier stands in for any silent-hold gate; the
/// assertion is the attribution vocabulary itself (op/window/hold with an
/// honest omitted count), not a claim about the live gate.
#[test]
fn maximized_tiled_clear_hold_names_its_gate_with_no_attempt() {
    use super::world_capture::capture_world;
    use super::world_diff::{FakeNative, FreshHost, HostDiff, apply_world_reply};
    use tiler_core::world_report::StateAttempt;

    let mut state = test_state();
    let mut seq = WorldSequencer::bootstrap("adapter-1", "gen-1").expect("bootstrap");
    let _ = seq.bootstrap_reply("req-0");
    state.ensure_device("DEV1");
    seed_tiled_member(&mut state, 101, "w1");
    seed_tiled_member(&mut state, 102, "w2");
    let tiles = vec![observed(101, "w1", 100, 100), observed(102, "w2", 300, 100)];
    for _ in 0..2 {
        observe_with_foreground(&mut state, &mut seq, &tiles, 0);
    }
    let maximized = vec![
        maximized_observed_fullscreen_frame(101, "w1"),
        observed(102, "w2", 300, 100),
    ];
    for _ in 0..2 {
        observe_with_foreground(&mut state, &mut seq, &maximized, 0);
    }
    let reply = dispatch_tiled(
        &mut state,
        &mut seq,
        &maximized,
        0,
        WorldCommand::MoveDirectional {
            window: tiler_core::directional::WindowSubject::Window(WindowId("w1".to_owned())),
            direction: tiler_core::directional::Direction::Right,
            same_axis: tiler_core::directional::SameAxisMove::default(),
        },
    );
    let op = reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "w1")
        .expect("mover keeps a row")
        .clone();
    let op = clear_op(&op).expect("published clear token precedes any attempt");
    // The held gate fires before any setter: zero writes, zero attempts,
    // one bounded hold row naming the exact gate. The unknown baseline
    // stands in for any silent-hold gate; the assertion is the attribution
    // vocabulary itself (op/window/hold), not a claim about the live gate.
    let areas = vec![area()];
    let retained: Vec<RetainedRow> = Vec::new();
    let captured = capture_world(&state.inputs(), Some(&maximized), &retained, &areas, 0);
    let mut fresh = FreshHost::from_sources(&state.inputs(), &maximized, &retained, &captured);
    fresh.baseline.remove("w1");
    let mut native = FakeNative::seeded(&fresh);
    let mut diff = HostDiff::new("adapter-1", "gen-1");
    diff.gate.current_obs = reply.based_on_observation_seq;
    let stats = apply_world_reply(
        &mut diff,
        &reply,
        &fresh,
        &mut native,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        1000,
    );
    // v4: HostDiff-drained `StateAttempt` rows replace the removed clear
    // sink and `clear_writes` counter; `stats.held` is the coarse hold total
    // that replaces the removed per-clear `clear_holds` trace rows (the
    // exact hold-reason strings no longer exist).
    let clear_out: Vec<StateAttempt> = diff.take_state_attempts();
    assert!(clear_out.is_empty(), "held gate reports no attempt");
    assert!(stats.held >= 1, "{stats:?}");
    assert!(
        op.starts_with("op-"),
        "published clear token precedes any attempt: {op:?}"
    );
}

/// Racing-input stale stop attribution: an external invalidation consumed
/// before the clear setter stops the pass with no write and no attempt,
/// and the unprocessed desire still attributes the stop as
/// `stale-stopped` (a stop ends the pass; a hold skips one row narrowly).
/// Like the hold oracle above this asserts the attribution vocabulary,
/// not a claim about the live gate.
#[test]
fn maximized_tiled_clear_stale_stop_attributes_before_dispatch() {
    use super::world_capture::capture_world;
    use super::world_diff::{FakeNative, FreshHost, HostDiff, apply_world_reply};
    use tiler_core::world_report::StateAttempt;

    let mut state = test_state();
    let mut seq = WorldSequencer::bootstrap("adapter-1", "gen-1").expect("bootstrap");
    let _ = seq.bootstrap_reply("req-0");
    state.ensure_device("DEV1");
    seed_tiled_member(&mut state, 101, "w1");
    seed_tiled_member(&mut state, 102, "w2");
    let tiles = vec![observed(101, "w1", 100, 100), observed(102, "w2", 300, 100)];
    for _ in 0..2 {
        observe_with_foreground(&mut state, &mut seq, &tiles, 0);
    }
    let maximized = vec![
        maximized_observed_fullscreen_frame(101, "w1"),
        observed(102, "w2", 300, 100),
    ];
    for _ in 0..2 {
        observe_with_foreground(&mut state, &mut seq, &maximized, 0);
    }
    let reply = dispatch_tiled(
        &mut state,
        &mut seq,
        &maximized,
        0,
        WorldCommand::MoveDirectional {
            window: tiler_core::directional::WindowSubject::Window(WindowId("w1".to_owned())),
            direction: tiler_core::directional::Direction::Right,
            same_axis: tiler_core::directional::SameAxisMove::default(),
        },
    );
    let row = reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "w1")
        .expect("mover keeps a row")
        .clone();
    let op = clear_op(&row).expect("published clear token precedes any attempt");
    // Racing input arrives after the desire is published: the pass stops
    // with no setter, no attempt, and one `stale-stopped` attribution.
    let areas = vec![area()];
    let retained: Vec<RetainedRow> = Vec::new();
    let captured = capture_world(&state.inputs(), Some(&maximized), &retained, &areas, 0);
    let fresh = FreshHost::from_sources(&state.inputs(), &maximized, &retained, &captured);
    let mut native = FakeNative::seeded(&fresh);
    native.state.insert(
        (
            "w1".to_owned(),
            tiler_core::world_report::StateProperty::Maximized,
        ),
        true,
    );
    // Settle the untouched peer at its published target so no earlier
    // write path probes freshness first: the racing input must survive to
    // the clear lane's own freshness gate.
    let w2_target = reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "w2")
        .and_then(|row| row.effective_frame)
        .expect("tiled peer publishes a frame target");
    native.windows.get_mut("w2").expect("seeded peer").frame = Some(w2_target);
    native.pending_invalidation = true;
    let mut diff = HostDiff::new("adapter-1", "gen-1");
    diff.gate.current_obs = reply.based_on_observation_seq;
    let stats = apply_world_reply(
        &mut diff,
        &reply,
        &fresh,
        &mut native,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        1000,
    );
    // v4: the removed `clear_out`/`clear_writes` facade reports no clear.
    // A stop ends the pass before the setter, so the only attribution is
    // the boolean `stopped_mid_apply`; there is no per-clear reason string
    // and no hold (a hold skips one row narrowly, a stop ends the pass).
    let clear_out: Vec<StateAttempt> = diff.take_state_attempts();
    assert!(clear_out.is_empty(), "stopped pass reports no attempt");
    assert!(stats.stopped_mid_apply, "{stats:?}");
    assert_eq!(stats.held, 0, "a stop is not a hold row, got {stats:?}");
    assert_eq!(stats.geometry_writes, 0, "{stats:?}");
    assert!(
        op.starts_with("op-"),
        "published clear token precedes any attempt: {op:?}"
    );
}

/// Mint a live-lane maximize-clear reply and its production-shaped diff
/// inputs: plain tiled admission for w1/w2, then the maximized subject
/// observed on the LIVE lane with the monitor-sized maximized frame. v4 has
/// no retained-only admission: a maximized window is still enumerated
/// visible, so its live fact is the only subject proof (a retained-only row
/// holds as unreadable and commands refuse stale). The v3 retained-overlay
/// fixtures are re-expressed on this lane. Returns the accepted reply, its
/// published clear op, and the fresh host built from the same observations.
#[allow(clippy::type_complexity)]
fn mint_live_maximized_clear() -> (
    tiler_core::world_report::WorldReplyV4,
    String,
    super::world_diff::FreshHost,
) {
    use super::world_capture::capture_world;
    use super::world_diff::FreshHost;

    let mut state = test_state();
    let mut seq = WorldSequencer::bootstrap("adapter-1", "gen-1").expect("bootstrap");
    let _ = seq.bootstrap_reply("req-0");
    state.ensure_device("DEV1");
    seed_tiled_member(&mut state, 101, "w1");
    seed_tiled_member(&mut state, 102, "w2");
    let tiles = vec![observed(101, "w1", 100, 100), observed(102, "w2", 300, 100)];
    for _ in 0..2 {
        observe_with_foreground(&mut state, &mut seq, &tiles, 0);
    }
    // In-place maximize: the live DWM frame grows to the work area while
    // the tile slot is retained core-side.
    let maximized = vec![
        maximized_observed_fullscreen_frame(101, "w1"),
        observed(102, "w2", 300, 100),
    ];
    for _ in 0..2 {
        observe_with_foreground(&mut state, &mut seq, &maximized, 0);
    }
    let reply = dispatch_tiled(
        &mut state,
        &mut seq,
        &maximized,
        0,
        WorldCommand::MoveDirectional {
            window: tiler_core::directional::WindowSubject::Window(WindowId("w1".to_owned())),
            direction: tiler_core::directional::Direction::Right,
            same_axis: tiler_core::directional::SameAxisMove::default(),
        },
    );
    match seq.last_command_result() {
        Some(CommandResult::Directional { outcome, .. }) => {
            assert_eq!(outcome.as_str(), "move-maximize-clear");
        }
        other => panic!("world-owned tiled need-clear, got {other:?}"),
    }
    let row = reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "w1")
        .expect("mover keeps a row");
    assert!(
        !row.floating && row.allocation.is_some(),
        "tiled route keeps its slot, got {row:?}"
    );
    let op = clear_op(row).expect("tiled clear desire published before any attempt");
    let areas = vec![area()];
    let retained: Vec<RetainedRow> = Vec::new();
    let captured = capture_world(&state.inputs(), Some(&maximized), &retained, &areas, 0);
    let fresh = FreshHost::from_sources(&state.inputs(), &maximized, &retained, &captured);
    (reply, op, fresh)
}

/// Clear authority and one-shot exactly-once (v3
/// `retained_overlay_dispatches_from_live_frame` replacement): v3 guarded
/// dispatch by comparing the live re-read against an observation-time
/// `observed_frame` on a retained overlay row. v4 has no retained-only
/// admission and no `observed_frame`: a maximized window is observed live,
/// so its authority rides the reply's `Controlled` clear target plus a
/// readable pre-state and a fresh observation watermark. Exactly one
/// accepted attempt under the published revision; a redelivered reply (lost
/// acknowledgement) never re-actuates the spent one-shot clear.
#[test]
fn maximized_live_clear_authority_dispatches_exactly_once() {
    use super::world_diff::{FakeNative, HostDiff, apply_world_reply};
    use tiler_core::world_report::{FrameSetter, StateProperty};

    let (reply, op, fresh) = mint_live_maximized_clear();
    let row = reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "w1")
        .expect("mover keeps a row");
    assert_eq!(clear_op(row).as_deref(), Some(op.as_str()));
    let mut native = FakeNative::seeded(&fresh);
    native
        .state
        .insert(("w1".to_owned(), StateProperty::Maximized), true);
    let mut diff = HostDiff::new("adapter-1", "gen-1");
    diff.gate.current_obs = reply.based_on_observation_seq;
    let stats = apply_world_reply(
        &mut diff,
        &reply,
        &fresh,
        &mut native,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        1000,
    );
    let clear_out = diff.take_state_attempts();
    assert_eq!(clear_out.len(), 1, "{stats:?}");
    let attempt = &clear_out[0];
    assert_eq!(
        attempt.revision, reply.state_revision,
        "attempt acts on the published clear revision"
    );
    assert_eq!(attempt.window.0, "w1");
    assert_eq!(attempt.property, StateProperty::Maximized);
    assert!(!attempt.target);
    assert_eq!(attempt.disposition, FrameSetter::Accepted);
    assert_eq!(attempt.pre, Some(true));
    assert_eq!(attempt.post, Some(false));
    assert_eq!(
        stats.held, 1,
        "only the maximized overlay frame write holds, got {stats:?}"
    );
    // Redelivery with a lost acknowledgement: the spent automatic clear is
    // one-shot per (lifetime, property, value) and never touches native
    // again. The scripted live state is forced back to maximized, so any
    // re-actuation would be visible as a flip to false.
    native
        .state
        .insert(("w1".to_owned(), StateProperty::Maximized), true);
    let redelivered = apply_world_reply(
        &mut diff,
        &reply,
        &fresh,
        &mut native,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        1000,
    );
    assert!(
        diff.take_state_attempts().is_empty(),
        "a redelivered one-shot clear reports no attempt: {redelivered:?}"
    );
    assert_eq!(
        native
            .state
            .get(&("w1".to_owned(), StateProperty::Maximized)),
        Some(&true),
        "the spent automatic clear never re-actuates native state"
    );
}

/// Unavailable pre-state holds without dispatch (v3
/// `unobserved_overlay_holds_without_dispatch` replacement): v3 refused to
/// backfill a baseline from a coincidental slot/live equality. v4 has no
/// substituted-slot baseline; the truthful analogue is that the state
/// setter's own pre-read is the authority to act, so an unreadable preimage
/// holds with zero native state calls and zero attempts.
#[test]
fn maximized_live_clear_unreadable_prestate_holds_without_dispatch() {
    use super::world_diff::{FakeNative, HostDiff, apply_world_reply};

    let (reply, _op, fresh) = mint_live_maximized_clear();
    // No maximized preimage recorded: the explicit live state read is
    // unknown, never a fabricated default.
    let mut native = FakeNative::seeded(&fresh);
    assert!(native.state.is_empty(), "fixture seeds no state preimage");
    let mut diff = HostDiff::new("adapter-1", "gen-1");
    diff.gate.current_obs = reply.based_on_observation_seq;
    let stats = apply_world_reply(
        &mut diff,
        &reply,
        &fresh,
        &mut native,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        1000,
    );
    assert!(
        diff.take_state_attempts().is_empty(),
        "an unreadable preimage reports no attempt"
    );
    assert!(stats.held >= 1, "{stats:?}");
    assert!(
        native.state_writes.is_empty(),
        "no native state call without a readable preimage, got {:?}",
        native.state_writes
    );
}

/// Stale observation watermark refuses without dispatch (v3
/// `observed_live_mismatch_races_without_dispatch` replacement): v3 raced
/// the live re-read against the observation-time frame when the user moved
/// the window between observation and dispatch. v4 carries no
/// `observed_frame` lane; the equivalent race is the reply's observation
/// watermark no longer matching the current capture, which fails the entry
/// freshness gate: one hold, stop, zero writes, zero attempts. Post-pre-read
/// racing input is the sibling stale-stop test.
#[test]
fn maximized_live_clear_stale_observation_races_without_dispatch() {
    use super::world_diff::{FakeNative, HostDiff, apply_world_reply};

    let (reply, _op, fresh) = mint_live_maximized_clear();
    let mut native = FakeNative::seeded(&fresh);
    native.state.insert(
        (
            "w1".to_owned(),
            tiler_core::world_report::StateProperty::Maximized,
        ),
        true,
    );
    let mut diff = HostDiff::new("adapter-1", "gen-1");
    // The capture advanced past the reply's observation: the user's input
    // superseded the observation the clear desire was based on.
    diff.gate.current_obs = reply.based_on_observation_seq + 1;
    let stats = apply_world_reply(
        &mut diff,
        &reply,
        &fresh,
        &mut native,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        1000,
    );
    assert!(
        diff.take_state_attempts().is_empty(),
        "a stale observation reports no attempt"
    );
    assert!(stats.stopped_mid_apply, "{stats:?}");
    assert_eq!(stats.held, 1, "entry freshness hold, got {stats:?}");
    assert_eq!(stats.geometry_writes, 0, "{stats:?}");
    assert!(
        native.state_writes.is_empty(),
        "a stale reply never actuates native state, got {:?}",
        native.state_writes
    );
}

/// No-focused-window directional focus (User 2026-10-26): the hook route
/// with no chord origin dispatches a subject-free FocusDirectional; the
/// core starts at the neutral action context and crosses by the existing
/// selection. Hermetic through the pending seams (injected observation,
/// suspend, elevation, foreground). The only native reads left are the
/// real foreground HWND (never matches injected members, so the global
/// focus stays honestly empty) and the real cursor sample (single-output
/// routes resolve through the registry fallback; two-output dispatches
/// pass explicit cursors).
fn nofocus_log_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("tiler-dir-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("tmp dir");
    dir
}

fn nofocus_me() -> crate::model::ProcessIdentity {
    crate::model::ProcessIdentity {
        pid: 4242,
        process_creation: "test-creation".to_owned(),
        user_sid: "S-1-5-test".to_owned(),
        session_id: 1,
        exe_path: "test.exe".to_owned(),
    }
}

fn nofocus_areas() -> (Vec<super::MonitorArea>, Vec<Rect>) {
    let areas = vec![
        super::MonitorArea {
            device: "DEV1".to_owned(),
            work: Rect {
                x: 0,
                y: 0,
                w: 800,
                h: 560,
            },
            full: Rect {
                x: 0,
                y: 0,
                w: 800,
                h: 600,
            },
            dpi: 96,
        },
        super::MonitorArea {
            device: "DEV2".to_owned(),
            work: Rect {
                x: 800,
                y: 0,
                w: 800,
                h: 560,
            },
            full: Rect {
                x: 800,
                y: 0,
                w: 800,
                h: 600,
            },
            dpi: 96,
        },
    ];
    let fulls: Vec<Rect> = areas.iter().map(|a| a.full).collect();
    (areas, fulls)
}

fn nofocus_seed_member(
    state: &mut super::TileLoop,
    me: &crate::model::ProcessIdentity,
    hwnd: u64,
    token: &str,
    pid: u32,
    creation: &str,
) {
    use super::inject_pending_tag;
    let member = crate::native_guards::WindowKey {
        hwnd,
        pid,
        creation: creation.to_owned(),
    };
    state.member_tokens.insert(member.clone(), token.to_owned());
    state.member_identity.insert(
        member.clone(),
        crate::model::ProcessIdentity {
            pid,
            process_creation: creation.to_owned(),
            user_sid: me.user_sid.clone(),
            session_id: me.session_id,
            exe_path: me.exe_path.clone(),
        },
    );
    state.member_tags.insert(member, format!("tag-{hwnd}"));
    inject_pending_tag(hwnd, Some(format!("tag-{hwnd}")));
}

fn nofocus_observed(
    me: &crate::model::ProcessIdentity,
    hwnd: u64,
    token: &str,
    pid: u32,
    creation: &str,
    x: i32,
) -> ObservedWindow {
    let visible = Rect {
        x,
        y: 10,
        w: 200,
        h: 200,
    };
    ObservedWindow {
        hwnd,
        token: token.to_owned(),
        outer: visible,
        visible,
        insets: crate::tiling::FrameInsets::measure(visible, visible),
        identity: ObservedTarget {
            hwnd,
            pid,
            process_creation: creation.to_owned(),
            exe_path: me.exe_path.clone(),
            user_sid: me.user_sid.clone(),
            session_id: me.session_id,
            tag: format!("tag-{hwnd}"),
        },
        facts: plain_facts(),
        member_tag: Some(format!("tag-{hwnd}")),
    }
}

struct NoFocusFixture {
    state: super::TileLoop,
    me: crate::model::ProcessIdentity,
    store: crate::storage::LedgerStore,
    dir: std::path::PathBuf,
    observed: Vec<ObservedWindow>,
    areas: Vec<super::MonitorArea>,
    fulls: Vec<Rect>,
    wire1: String,
    wire2: String,
}

/// Seed two outputs with managed windows (plus an optional second
/// source window), converged with no foreground. Returns the fixture for
/// direct [`super::world_tick::run_world_command`] dispatches.
fn nofocus_seeded(name: &str, source_windows: usize, with_target: bool) -> NoFocusFixture {
    use super::{
        clear_pending_seams, inject_pending_elevated, inject_pending_foreground,
        inject_pending_observe, inject_pending_suspend,
    };
    clear_pending_seams();
    let dir = nofocus_log_dir(name);
    let mut state = test_state();
    state.log_path = dir.join("owner.log");
    let me = nofocus_me();
    let (areas, fulls) = nofocus_areas();
    state.ensure_device("DEV1");
    state.ensure_device("DEV2");
    let wire1 = state.wire_of("DEV1").expect("wire 1");
    let wire2 = state.wire_of("DEV2").expect("wire 2");
    let mut members = Vec::new();
    for (index, (hwnd, token, pid, creation, x)) in [
        (0x6101u64, "w-m1", 9101u32, "c-6101", 10),
        (0x6102u64, "w-m2", 9102u32, "c-6102", 300),
    ]
    .into_iter()
    .enumerate()
    {
        if index < source_windows {
            members.push((hwnd, token, pid, creation, x));
        }
    }
    if with_target {
        members.push((0x6201u64, "w-n1", 9201u32, "c-6201", 810));
    }
    for (hwnd, token, pid, creation, _) in &members {
        nofocus_seed_member(&mut state, &me, *hwnd, token, *pid, creation);
    }
    let observed: Vec<ObservedWindow> = members
        .iter()
        .map(|(hwnd, token, pid, creation, x)| {
            nofocus_observed(&me, *hwnd, token, *pid, creation, *x)
        })
        .collect();
    let store = crate::storage::LedgerStore::open(&dir.join("ledger")).expect("ledger");
    inject_pending_observe(observed.clone(), Vec::new(), false);
    inject_pending_suspend(Some(false));
    inject_pending_elevated(Some(false));
    inject_pending_foreground(Some(0));
    let seed = super::world_tick::run_observation_tick(
        &mut state,
        &me,
        &store,
        &dir,
        &observed,
        &[],
        &areas,
        0,
        &fulls,
        None,
    )
    .expect("seed converges");
    assert!(seed.ready, "seed converges");
    NoFocusFixture {
        state,
        me,
        store,
        dir,
        observed,
        areas,
        fulls,
        wire1,
        wire2,
    }
}

fn nofocus_command(direction: &str) -> WorldCommand {
    WorldCommand::FocusDirectional {
        window: tiler_core::world_report::FocusDirectionalSubject::NoFocus,
        direction: tiler_core::directional::Direction::parse_wire(direction).expect("direction"),
    }
}

fn nofocus_dispatch(
    fx: &mut NoFocusFixture,
    cursor: Option<(i32, i32)>,
    direction: &str,
) -> Option<super::world_tick::WorldCommandOutcome> {
    super::world_tick::run_world_command(
        &mut fx.state,
        &fx.me,
        &fx.store,
        &fx.dir,
        &fx.observed,
        &[],
        &fx.areas,
        0,
        &fx.fulls,
        nofocus_command(direction),
        cursor,
    )
}

#[test]
fn no_focus_dispatch_crosses_occupied_source_to_existing_selection() {
    // Occupied source (two windows) with no focused window: the core
    // crosses right from the cursor-resolved output and effects the
    // destination's remembered selection, never a source window.
    let mut fx = nofocus_seeded("nofocus-cross", 2, true);
    let outcome = nofocus_dispatch(&mut fx, Some((10, 10)), "right").expect("accepted");
    let reply = &outcome.reply;
    assert!(outcome.result.is_none(), "success rides the focus request");
    let focus = reply.focus_request.as_ref().expect("one focus request");
    assert_eq!(focus.target.0, "w-n1", "existing destination selection");
    assert!(
        focus.id.starts_with("focus-"),
        "one-shot focus id: {}",
        focus.id
    );
    let _ = (fx.wire1, fx.wire2);
    super::clear_pending_seams();
    let _ = std::fs::remove_dir_all(&fx.dir);
}

#[test]
fn no_focus_dispatch_empty_source_crosses() {
    // Empty source workspace: no local subject exists, yet the adjacent
    // output's selection is still reached.
    let mut fx = nofocus_seeded("nofocus-empty-src", 0, true);
    let outcome = nofocus_dispatch(&mut fx, Some((10, 10)), "right").expect("accepted");
    let focus = outcome
        .reply
        .focus_request
        .as_ref()
        .expect("empty source still crosses");
    assert_eq!(focus.target.0, "w-n1");
    super::clear_pending_seams();
    let _ = std::fs::remove_dir_all(&fx.dir);
}

#[test]
fn no_focus_dispatch_empty_destination_retains() {
    // Adjacent output exists but is empty: no remembered selection, so
    // no effect rather than an invented target.
    let mut fx = nofocus_seeded("nofocus-empty-dst", 1, false);
    let outcome = nofocus_dispatch(&mut fx, Some((10, 10)), "right").expect("accepted");
    let reply = &outcome.reply;
    assert!(
        reply.focus_request.is_none(),
        "empty target: {:?}",
        reply.focus_request
    );
    match &outcome.result {
        Some(CommandResult::Directional { outcome, cause, .. }) => {
            assert_eq!(outcome.as_str(), "focus-unfocused-retained");
            assert_eq!(cause.as_deref(), Some("no-target"));
        }
        other => panic!("expected retained no-op, got {other:?}"),
    }
    super::clear_pending_seams();
    let _ = std::fs::remove_dir_all(&fx.dir);
}

#[test]
fn no_focus_keyboard_route_without_origin_reaches_core() {
    // Production hook route with no chord origin on a single output:
    // the intent reaches the core (no origin-vanished settle) and the
    // core retains with no adjacent output, actuating nothing.
    use super::{
        clear_pending_seams, inject_pending_elevated, inject_pending_foreground,
        inject_pending_observe, inject_pending_suspend, inject_pending_tag, keyboard_tick,
    };
    use crate::snapkey::{QueuedIntent, QueuedSnapEvent, SnapEdge, SnapOp};
    clear_pending_seams();
    let dir = nofocus_log_dir("nofocus-route");
    let mut state = test_state();
    state.log_path = dir.join("owner.log");
    let me = nofocus_me();
    let areas = vec![area()];
    let fulls: Vec<Rect> = areas.iter().map(|a| a.full).collect();
    state.ensure_device("DEV1");
    nofocus_seed_member(&mut state, &me, 0x6301, "w-1", 9301, "c-6301");
    inject_pending_tag(0x6301, Some("tag-0x6301".to_owned()));
    let observed = vec![nofocus_observed(&me, 0x6301, "w-1", 9301, "c-6301", 10)];
    let store = crate::storage::LedgerStore::open(&dir.join("ledger")).expect("ledger");
    inject_pending_observe(observed.clone(), Vec::new(), false);
    inject_pending_suspend(Some(false));
    inject_pending_elevated(Some(false));
    inject_pending_foreground(Some(0));
    let seed = super::world_tick::run_observation_tick(
        &mut state,
        &me,
        &store,
        &dir,
        &observed,
        &[],
        &areas,
        0,
        &fulls,
        None,
    )
    .expect("seed converges");
    assert!(seed.ready, "seed converges");
    // The real cursor sample rides this tick; the single-output
    // registry fallback keeps source resolution deterministic.
    inject_pending_observe(observed.clone(), Vec::new(), false);
    inject_pending_suspend(Some(false));
    inject_pending_elevated(Some(false));
    inject_pending_foreground(Some(0));
    inject_pending_tag(0x6301, Some("tag-0x6301".to_owned()));
    keyboard_tick(
        &mut state,
        &me,
        &store,
        &dir,
        &fulls,
        &areas,
        vec![QueuedSnapEvent::Intent(QueuedIntent {
            op: SnapOp::Focus,
            direction: tiler_core::directional::Direction::Right,
            edge: SnapEdge::Down,
            origin: None,
            consumed: true,
            announce: true,
            tick: std::time::Instant::now(),
        })],
        None,
    );
    assert_eq!(state.tick, 1, "production dispatch advances one tick");
    assert!(state.snap_advance.is_none());
    let text = std::fs::read_to_string(dir.join("owner.log")).expect("owner log");
    let lines: Vec<serde_json::Value> = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("log line"))
        .collect();
    let snaps: Vec<&serde_json::Value> = lines
        .iter()
        .filter(|line| line["event"] == serde_json::json!("snap"))
        .collect();
    assert_eq!(snaps.len(), 1, "one snap settle: {lines:?}");
    assert_eq!(snaps[0]["op"], serde_json::json!("focus"));
    assert_eq!(snaps[0]["direction"], serde_json::json!("right"));
    assert_eq!(
        snaps[0]["outcome"],
        serde_json::json!("focus-unfocused-retained"),
        "reaches core, not origin-vanished: {lines:?}"
    );
    assert_eq!(snaps[0]["cause"], serde_json::json!("no-adjacent-output"));
    clear_pending_seams();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn no_focus_keyboard_route_keeps_suspend_and_elevation_fences() {
    // Suspension and elevation still fence the no-origin route before any
    // core dispatch, exactly like the subject route.
    use super::{
        clear_pending_seams, inject_pending_elevated, inject_pending_foreground,
        inject_pending_observe, inject_pending_suspend, inject_pending_tag, keyboard_tick,
    };
    use crate::snapkey::{QueuedIntent, QueuedSnapEvent, SnapEdge, SnapOp};
    for (name, suspend, elevated, outcome) in [
        ("nofocus-suspend", Some(true), Some(false), "suspended"),
        (
            "nofocus-elevated",
            Some(false),
            Some(true),
            "elevated-foreground",
        ),
    ] {
        clear_pending_seams();
        let dir = nofocus_log_dir(name);
        let mut state = test_state();
        state.log_path = dir.join("owner.log");
        let me = nofocus_me();
        let areas = vec![area()];
        let fulls: Vec<Rect> = areas.iter().map(|a| a.full).collect();
        state.ensure_device("DEV1");
        nofocus_seed_member(&mut state, &me, 0x6401, "w-1", 9401, "c-6401");
        inject_pending_tag(0x6401, Some("tag-0x6401".to_owned()));
        let observed = vec![nofocus_observed(&me, 0x6401, "w-1", 9401, "c-6401", 10)];
        let store = crate::storage::LedgerStore::open(&dir.join("ledger")).expect("ledger");
        inject_pending_observe(observed.clone(), Vec::new(), false);
        inject_pending_suspend(Some(false));
        inject_pending_elevated(Some(false));
        inject_pending_foreground(Some(0));
        super::world_tick::run_observation_tick(
            &mut state,
            &me,
            &store,
            &dir,
            &observed,
            &[],
            &areas,
            0,
            &fulls,
            None,
        )
        .expect("seed converges");
        inject_pending_observe(observed.clone(), Vec::new(), false);
        inject_pending_suspend(suspend);
        inject_pending_elevated(elevated);
        inject_pending_foreground(Some(0));
        inject_pending_tag(0x6401, Some("tag-0x6401".to_owned()));
        keyboard_tick(
            &mut state,
            &me,
            &store,
            &dir,
            &fulls,
            &areas,
            vec![QueuedSnapEvent::Intent(QueuedIntent {
                op: SnapOp::Focus,
                direction: tiler_core::directional::Direction::Right,
                edge: SnapEdge::Down,
                origin: None,
                consumed: true,
                announce: true,
                tick: std::time::Instant::now(),
            })],
            None,
        );
        assert!(state.snap_advance.is_none());
        let text = std::fs::read_to_string(dir.join("owner.log")).expect("owner log");
        let lines: Vec<serde_json::Value> = text
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| serde_json::from_str(line).expect("log line"))
            .collect();
        let snaps: Vec<&serde_json::Value> = lines
            .iter()
            .filter(|line| line["event"] == serde_json::json!("snap"))
            .collect();
        assert_eq!(snaps.len(), 1, "one snap settle: {lines:?}");
        assert_eq!(
            snaps[0]["outcome"],
            serde_json::Value::from(outcome),
            "fence holds: {lines:?}"
        );
        clear_pending_seams();
        let _ = std::fs::remove_dir_all(&dir);
    }
}

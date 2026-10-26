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
) -> tiler_core::world_report::WorldReply {
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
        )
        .expect("command dispatches");
    // Truthful capture: both floats ride with membership evidence and no
    // invented frames; the command names the window only.
    assert_eq!(report.windows.len(), 2);
    assert!(
        report.windows.iter().all(|fact| fact.floating),
        "lanes capture float truth, got {:?}",
        report.windows
    );
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

#[test]
fn float_origins_dispatch_world_commands_with_core_owned_decisions() {
    // No adapter bypass exists: the float-focus target, the constrained
    // half-snap rect, and the automatic-float refusal all arrive as the
    // core reply to a dispatched world command.
    let mut state = test_state();
    let mut seq = WorldSequencer::bootstrap("adapter-1", "gen-1").expect("bootstrap");
    let _ = seq.bootstrap_reply("req-0");
    let observed = bootstrap_floats(&mut state, &mut seq);

    // Float focus resolves the core-selected target with an explicit
    // focus effect; the canonical encoding round-trips the result.
    let reply = dispatch(
        &mut state,
        &mut seq,
        &observed,
        WorldCommand::FocusDirectional {
            window: WindowId("w1".to_owned()),
            direction: "right".to_owned(),
        },
    );
    match &reply.command_result {
        Some(CommandResult::Directional {
            outcome,
            target,
            rect,
            ..
        }) => {
            assert_eq!(outcome.as_str(), "focus-float-applied");
            assert_eq!(target, &Some(WindowId("w2".to_owned())));
            assert!(rect.is_none());
        }
        other => panic!("world-owned float focus, got {other:?}"),
    }
    assert!(
        reply
            .effects
            .iter()
            .any(|effect| effect.target == WindowId("w2".to_owned())),
        "explicit focus effect for the core-selected float"
    );
    assert_eq!(
        reply
            .effects
            .iter()
            .filter(|effect| effect.kind == tiler_core::world_report::EffectKind::Focus)
            .count(),
        1,
        "a single spent focus effect, never a duplicate actuation lane"
    );
    let wire = tiler_core::world_sizing::encode_reply_canonical(&reply);
    assert!(
        wire.contains("\"op\":\"directional\",\"outcome\":\"focus-float-applied\""),
        "directional result round-trips, got {wire}"
    );

    // Float move authorizes the bounded half-snap in work-area space
    // (gap 8 over 800x560) through the uniform retained placement: the
    // reply row carries the stable frame operation with the authorized
    // half as its desire, actuated by the generic float lane, never a
    // bespoke adapter write.
    let reply = dispatch(
        &mut state,
        &mut seq,
        &observed,
        WorldCommand::MoveDirectional {
            window: WindowId("w1".to_owned()),
            direction: "right".to_owned(),
            same_axis: tiler_core::directional::SameAxisMove::default(),
        },
    );
    let half = Rect {
        x: 404,
        y: 8,
        w: 388,
        h: 544,
    };
    match &reply.command_result {
        Some(CommandResult::Directional { outcome, rect, .. }) => {
            assert_eq!(outcome.as_str(), "move-float-applied");
            assert_eq!(*rect, Some(half));
        }
        other => panic!("world-owned half-snap, got {other:?}"),
    }
    let row = reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "w1")
        .expect("mover keeps a row");
    assert!(
        row.frame_operation.is_some(),
        "uniform retained placement rides the row, got {:?}",
        row.frame_operation
    );
    assert_eq!(
        row.effective_frame,
        Some(half),
        "the published desire IS the authorized half"
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
            window: WindowId("w1".to_owned()),
            direction: "right".to_owned(),
            same_axis: tiler_core::directional::SameAxisMove::default(),
        },
    );
    match &reply.command_result {
        Some(CommandResult::Directional { outcome, .. }) => {
            assert_eq!(outcome.as_str(), "move-refused-floating");
        }
        other => panic!("world-owned automatic refusal, got {other:?}"),
    }
    assert!(
        reply.effects.is_empty() && reply.transfers.is_empty(),
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
) -> tiler_core::world_report::WorldReply {
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

#[test]
fn maximized_move_publishes_one_shot_clear_desire() {
    // A maximized move publishes exactly one `clear_operation` desire
    // under the originating op with a NeedClear result: no placement, no
    // transfer, no effect, and no second command. The desire (not a
    // bespoke adapter clear) is the only actuation authority, and the
    // commandless core settlement resumes it on proof.
    let mut state = test_state();
    let mut seq = WorldSequencer::bootstrap("adapter-1", "gen-1").expect("bootstrap");
    let _ = seq.bootstrap_reply("req-0");
    state.ensure_device("DEV1");
    seed_member(&mut state, 101, "w1");
    seed_member(&mut state, 102, "w2");
    let observed = vec![
        maximized_observed(101, "w1", 100, 100),
        observed(102, "w2", 300, 100),
    ];
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
    let reply = dispatch(
        &mut state,
        &mut seq,
        &observed,
        WorldCommand::MoveDirectional {
            window: WindowId("w1".to_owned()),
            direction: "right".to_owned(),
            same_axis: tiler_core::directional::SameAxisMove::default(),
        },
    );
    match &reply.command_result {
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
    let op = row.clear_operation.clone().expect("one-shot clear desire");
    assert!(
        row.frame_operation.is_none(),
        "no placement rides a need-clear"
    );
    assert!(
        reply.effects.is_empty() && reply.transfers.is_empty(),
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
        rerow.clear_operation.as_deref(),
        Some(op.as_str()),
        "undispatched desire persists with no re-dispatch"
    );
}

#[test]
fn sequencer_carries_clear_attempts_to_ack() {
    // Staged clear attempts ride the next report exactly once: the ack
    // removes exactly-equal entries, so a lost acknowledgement redelivers
    // while an acknowledged one never replays.
    use tiler_core::world_report::{ClearAttempt, FrameSetter};
    let mut state = test_state();
    let mut seq = WorldSequencer::bootstrap("adapter-1", "gen-1").expect("bootstrap");
    let _ = seq.bootstrap_reply("req-0");
    state.ensure_device("DEV1");
    seq.note_clear_attempt(ClearAttempt {
        operation_id: "op-7".to_owned(),
        window: WindowId("w1".to_owned()),
        setter: FrameSetter::Accepted,
        pre_maximized: Some(true),
        post_maximized: Some(false),
        attempt_ms: 5,
    });
    let areas = vec![area()];
    let retained: Vec<RetainedRow> = Vec::new();
    seq.invalidate();
    let report = seq
        .try_dispatch(&state.inputs(), Some(&[]), &retained, &areas, 0, 0, None)
        .expect("observation dispatches");
    assert!(
        report
            .clear_attempts
            .iter()
            .any(|attempt| attempt.operation_id == "op-7"),
        "staged attempt rides: {:?}",
        report.clear_attempts
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
        .try_dispatch(&state.inputs(), Some(&[]), &retained, &areas, 0, 0, None)
        .expect("observation dispatches");
    assert!(
        report.clear_attempts.is_empty(),
        "acked attempts never redeliver: {:?}",
        report.clear_attempts
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
                window: WindowId("w9".to_owned()),
                direction: "right".to_owned(),
            },
            "focus-float-refused-stale",
        ),
        (
            WorldCommand::MoveDirectional {
                window: WindowId("w9".to_owned()),
                direction: "right".to_owned(),
                same_axis: tiler_core::directional::SameAxisMove::default(),
            },
            "move-float-refused-stale",
        ),
    ] {
        let reply = dispatch(&mut state, &mut seq, &observed, command);
        match &reply.command_result {
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
            reply.effects.is_empty() && reply.transfers.is_empty(),
            "stale refusals stay atomic"
        );
    }
}

#[test]
fn sticky_moves_authorize_through_the_retained_lane_at_the_seam() {
    // Sticky half-snaps ride the same uniform retained float-placement
    // lane: the core authorizes the half and the seam actuates it exactly
    // once under the stable identity; ordinary unsolicited sticky writes
    // stay prohibited (rows without a `frame_operation` token never take
    // frame writes).
    let mut state = test_state();
    let mut seq = WorldSequencer::bootstrap("adapter-1", "gen-1").expect("bootstrap");
    let _ = seq.bootstrap_reply("req-0");
    let observed = bootstrap_floats(&mut state, &mut seq);
    state.sticky.insert(key(101), true);
    let reply = dispatch(
        &mut state,
        &mut seq,
        &observed,
        WorldCommand::MoveDirectional {
            window: WindowId("w1".to_owned()),
            direction: "right".to_owned(),
            same_axis: tiler_core::directional::SameAxisMove::default(),
        },
    );
    let half = match &reply.command_result {
        Some(CommandResult::Directional { outcome, rect, .. }) => {
            assert_eq!(outcome.as_str(), "move-float-applied");
            rect.expect("authorized half")
        }
        other => panic!("world-owned sticky half-snap, got {other:?}"),
    };
    assert!(
        reply.effects.is_empty() && reply.transfers.is_empty(),
        "half-snaps stay effect/transfer-free"
    );
    assert!(
        reply
            .windows
            .iter()
            .any(|row| row.lifetime.0 == "w1" && row.frame_operation.is_some()),
        "retained placement rides the half {half:?}"
    );
}

#[test]
fn float_placement_receipt_settles_the_retained_target() {
    // One shot, then truthfully settled: the authorized half rides the
    // retained placement exactly once, and native observation at the half
    // confirms it so the next reply carries no operation and never
    // re-fights the float.
    let mut state = test_state();
    let mut seq = WorldSequencer::bootstrap("adapter-1", "gen-1").expect("bootstrap");
    let _ = seq.bootstrap_reply("req-0");
    let floats = bootstrap_floats(&mut state, &mut seq);
    let reply = dispatch(
        &mut state,
        &mut seq,
        &floats,
        WorldCommand::MoveDirectional {
            window: WindowId("w1".to_owned()),
            direction: "right".to_owned(),
            same_axis: tiler_core::directional::SameAxisMove::default(),
        },
    );
    let half = match &reply.command_result {
        Some(CommandResult::Directional { outcome, rect, .. }) => {
            assert_eq!(outcome.as_str(), "move-float-applied");
            rect.expect("authorized half")
        }
        other => panic!("world-owned half-snap, got {other:?}"),
    };
    let placed = reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "w1")
        .expect("mover keeps a row");
    let operation = placed.frame_operation.clone().expect("retained operation");
    // The native write lands (offline: the next observation reads the
    // half): the core confirms from normalized facts and drops the
    // retained target with no retry and no replacement.
    let settled_observed = vec![
        observed_rect(101, "w1", half),
        observed(102, "w2", 300, 100),
    ];
    let reply = observe_only(&mut state, &mut seq, &settled_observed);
    assert!(
        reply
            .windows
            .iter()
            .find(|row| row.lifetime.0 == "w1")
            .is_some_and(|row| row.frame_operation.is_none()),
        "confirmed placement settles the operation {operation}"
    );
}

/// Observation-only round naming the live foreground explicitly, so the
/// captured `focused_global` names the subject instead of unknown.
fn observe_with_foreground(
    state: &mut super::TileLoop,
    seq: &mut WorldSequencer,
    observed: &[ObservedWindow],
    foreground: u64,
) -> tiler_core::world_report::WorldReply {
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

/// B4 integrated production seam (float): actual engine admission, maximize
/// of the retained subject, NeedClear with the published op token BEFORE
/// any attempt, the production common diff lane dispatching exactly one
/// native clear with a correlated accepted attempt, then commandless
/// continuation under the ORIGINAL op with clear retirement. No fabricated
/// proof: the attempt is diff-generated from the published desire itself.
#[test]
fn maximized_clear_runs_core_desire_through_diff_to_continuation() {
    use super::world_capture::capture_world;
    use super::world_diff::{FakeNative, FreshHost, HostDiff, apply_world_reply};
    use tiler_core::world_report::{ClearAttempt, FrameSetter, OperationOutcome};

    let mut state = test_state();
    let mut seq = WorldSequencer::bootstrap("adapter-1", "gen-1").expect("bootstrap");
    let _ = seq.bootstrap_reply("req-0");
    // Normal admission of two ordinary floats.
    let _ = bootstrap_floats(&mut state, &mut seq);
    // Maximize the retained subject with no admission clear for its
    // lifetime, under exact-subject foreground.
    let maximized = vec![
        maximized_observed(101, "w1", 100, 100),
        observed(102, "w2", 300, 100),
    ];
    for _ in 0..2 {
        observe_with_foreground(&mut state, &mut seq, &maximized, 101);
    }
    // One directional move publishes the one-shot clear desire under the
    // originating op before any attempt exists for it.
    let reply = dispatch(
        &mut state,
        &mut seq,
        &maximized,
        WorldCommand::MoveDirectional {
            window: WindowId("w1".to_owned()),
            direction: "right".to_owned(),
            same_axis: tiler_core::directional::SameAxisMove::default(),
        },
    );
    match &reply.command_result {
        Some(CommandResult::Directional { outcome, .. }) => {
            assert_eq!(outcome.as_str(), "move-maximize-clear");
        }
        other => panic!("world-owned need-clear, got {other:?}"),
    }
    let op = reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "w1")
        .expect("mover keeps a row")
        .clear_operation
        .clone()
        .expect("one-shot clear desire published before any attempt");
    assert!(reply.operation_settled.is_empty(), "bind emits no terminal");
    // Production common diff lane: exactly one native clear from the
    // published desire, reported as the correlated attempt.
    let areas = vec![area()];
    let retained: Vec<RetainedRow> = Vec::new();
    let captured = capture_world(&state.inputs(), Some(&maximized), &retained, &areas, 101);
    let fresh = FreshHost::from_sources(&state.inputs(), &maximized, &retained, &captured);
    let mut native = FakeNative::seeded(&fresh);
    let mut diff = HostDiff::new("adapter-1", "gen-1");
    diff.gate.current_obs = reply.based_on_observation_seq;
    let mut clear_out: Vec<ClearAttempt> = Vec::new();
    let stats = apply_world_reply(
        &mut diff,
        &reply,
        &fresh,
        &mut native,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        &mut clear_out,
        1000,
    );
    assert_eq!(stats.clear_writes, 1, "{stats:?}");
    assert_eq!(clear_out.len(), 1);
    let attempt = &clear_out[0];
    assert_eq!(attempt.operation_id, op);
    assert_eq!(attempt.window.0, "w1");
    assert_eq!(attempt.setter, FrameSetter::Accepted);
    assert_eq!(attempt.pre_maximized, Some(true));
    assert_eq!(attempt.post_maximized, Some(false));
    // Commandless continuation on the diff-generated proof resumes the
    // original half-snap under the ORIGINAL op with clear retirement: no
    // second command, no second clear.
    seq.note_clear_attempt(attempt.clone());
    let cleared = vec![observed(101, "w1", 100, 100), observed(102, "w2", 300, 100)];
    let reply = observe_with_foreground(&mut state, &mut seq, &cleared, 101);
    let row = reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "w1")
        .expect("mover keeps a row");
    assert_eq!(row.frame_operation.as_deref(), Some(op.as_str()));
    assert_eq!(
        row.clear_operation, None,
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
/// diff-attempt to continuation pattern for a maximized sticky subject,
/// through the common retained frame lane.
#[test]
fn maximized_sticky_clear_runs_core_desire_through_diff_to_continuation() {
    use super::world_capture::capture_world;
    use super::world_diff::{FakeNative, FreshHost, HostDiff, apply_world_reply};
    use tiler_core::world_report::{ClearAttempt, FrameSetter, OperationOutcome};

    let mut state = test_state();
    let mut seq = WorldSequencer::bootstrap("adapter-1", "gen-1").expect("bootstrap");
    let _ = seq.bootstrap_reply("req-0");
    let _ = bootstrap_floats(&mut state, &mut seq);
    state.sticky.insert(key(101), true);
    let maximized = vec![
        maximized_observed(101, "w1", 100, 100),
        observed(102, "w2", 300, 100),
    ];
    for _ in 0..2 {
        observe_with_foreground(&mut state, &mut seq, &maximized, 101);
    }
    let reply = dispatch(
        &mut state,
        &mut seq,
        &maximized,
        WorldCommand::MoveDirectional {
            window: WindowId("w1".to_owned()),
            direction: "right".to_owned(),
            same_axis: tiler_core::directional::SameAxisMove::default(),
        },
    );
    match &reply.command_result {
        Some(CommandResult::Directional { outcome, .. }) => {
            assert_eq!(outcome.as_str(), "move-maximize-clear");
        }
        other => panic!("world-owned sticky need-clear, got {other:?}"),
    }
    let op = reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "w1")
        .expect("mover keeps a row")
        .clear_operation
        .clone()
        .expect("sticky clear desire published before any attempt");
    let areas = vec![area()];
    let retained: Vec<RetainedRow> = Vec::new();
    let captured = capture_world(&state.inputs(), Some(&maximized), &retained, &areas, 101);
    let fresh = FreshHost::from_sources(&state.inputs(), &maximized, &retained, &captured);
    assert!(
        fresh.baseline.get("w1").is_some_and(|window| window.sticky),
        "sticky rides the diff baseline"
    );
    let mut native = FakeNative::seeded(&fresh);
    let mut diff = HostDiff::new("adapter-1", "gen-1");
    diff.gate.current_obs = reply.based_on_observation_seq;
    let mut clear_out: Vec<ClearAttempt> = Vec::new();
    let stats = apply_world_reply(
        &mut diff,
        &reply,
        &fresh,
        &mut native,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        &mut clear_out,
        1000,
    );
    assert_eq!(stats.clear_writes, 1, "{stats:?}");
    assert_eq!(clear_out.len(), 1);
    assert_eq!(clear_out[0].operation_id, op);
    assert_eq!(clear_out[0].setter, FrameSetter::Accepted);
    seq.note_clear_attempt(clear_out[0].clone());
    let cleared = vec![observed(101, "w1", 100, 100), observed(102, "w2", 300, 100)];
    let reply = observe_with_foreground(&mut state, &mut seq, &cleared, 101);
    let row = reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "w1")
        .expect("mover keeps a row");
    assert_eq!(row.frame_operation.as_deref(), Some(op.as_str()));
    assert_eq!(row.clear_operation, None);
    assert!(
        reply.operation_settled.iter().any(|event| {
            event.operation_id == op && event.outcome == OperationOutcome::ClearContinuedFloat
        }),
        "sticky continuation observable under original op, got {:?}",
        reply.operation_settled
    );
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
) -> tiler_core::world_report::WorldReply {
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
    use tiler_core::world_report::{ClearAttempt, FrameSetter, OperationOutcome};

    let mut state = test_state();
    let mut seq = WorldSequencer::bootstrap("adapter-1", "gen-1").expect("bootstrap");
    let _ = seq.bootstrap_reply("req-0");
    // Normal tiled admission: two readable tiles, never floated.
    state.ensure_device("DEV1");
    seed_tiled_member(&mut state, 101, "w1");
    seed_tiled_member(&mut state, 102, "w2");
    let tiles = vec![observed(101, "w1", 100, 100), observed(102, "w2", 300, 100)];
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
            window: WindowId("w1".to_owned()),
            direction: "right".to_owned(),
            same_axis: tiler_core::directional::SameAxisMove::default(),
        },
    );
    match &reply.command_result {
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
    let op = row
        .clear_operation
        .clone()
        .expect("tiled clear desire published before any attempt");
    assert_eq!(row.frame_operation, None, "tiled rows carry no float token");
    assert!(reply.operation_settled.is_empty(), "bind emits no terminal");
    // Production common diff lane: exactly one native clear from the
    // published tiled desire, reported as the correlated attempt.
    let areas = vec![area()];
    let retained: Vec<RetainedRow> = Vec::new();
    let captured = capture_world(&state.inputs(), Some(&maximized), &retained, &areas, 101);
    let fresh = FreshHost::from_sources(&state.inputs(), &maximized, &retained, &captured);
    let mut native = FakeNative::seeded(&fresh);
    let mut diff = HostDiff::new("adapter-1", "gen-1");
    diff.gate.current_obs = reply.based_on_observation_seq;
    let mut clear_out: Vec<ClearAttempt> = Vec::new();
    let stats = apply_world_reply(
        &mut diff,
        &reply,
        &fresh,
        &mut native,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        &mut clear_out,
        1000,
    );
    assert_eq!(stats.clear_writes, 1, "{stats:?}");
    assert_eq!(clear_out.len(), 1);
    let attempt = &clear_out[0];
    assert_eq!(attempt.operation_id, op);
    assert_eq!(attempt.window.0, "w1");
    assert_eq!(attempt.setter, FrameSetter::Accepted);
    assert_eq!(attempt.pre_maximized, Some(true));
    assert_eq!(attempt.post_maximized, Some(false));
    // Commandless continuation on the diff-generated proof resumes the
    // original tiled intent under the ORIGINAL op with clear retirement:
    // no second command, no second clear.
    seq.note_clear_attempt(attempt.clone());
    let cleared = vec![observed(101, "w1", 100, 100), observed(102, "w2", 300, 100)];
    let reply = observe_with_foreground(&mut state, &mut seq, &cleared, 101);
    let row = reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "w1")
        .expect("mover keeps a row");
    assert_eq!(
        row.clear_operation, None,
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
/// reported, with no hold row. Unknown focus alone never holds the common
/// clear lane; focus enters only core continuation, never dispatch.
#[test]
fn maximized_tiled_clear_with_unknown_focus_still_dispatches() {
    use super::world_capture::capture_world;
    use super::world_diff::{FakeNative, FreshHost, HostDiff, apply_world_reply};
    use tiler_core::world_report::{ClearAttempt, FrameSetter};

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
            window: WindowId("w1".to_owned()),
            direction: "right".to_owned(),
            same_axis: tiler_core::directional::SameAxisMove::default(),
        },
    );
    match &reply.command_result {
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
    let op = row
        .clear_operation
        .clone()
        .expect("tiled clear desire published under unknown focus");
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
    let mut diff = HostDiff::new("adapter-1", "gen-1");
    diff.gate.current_obs = reply.based_on_observation_seq;
    let mut clear_out: Vec<ClearAttempt> = Vec::new();
    let stats = apply_world_reply(
        &mut diff,
        &reply,
        &fresh,
        &mut native,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        &mut clear_out,
        1000,
    );
    assert_eq!(stats.clear_writes, 1, "{stats:?}");
    assert_eq!(clear_out.len(), 1);
    assert_eq!(clear_out[0].operation_id, op);
    assert_eq!(clear_out[0].setter, FrameSetter::Accepted);
    assert!(
        stats.clear_holds.is_empty(),
        "no hold row on a clean dispatch, got {:?}",
        stats.clear_holds
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
            window: WindowId("w1".to_owned()),
            direction: "right".to_owned(),
            same_axis: tiler_core::directional::SameAxisMove::default(),
        },
    );
    let op = reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "w1")
        .expect("mover keeps a row")
        .clear_operation
        .clone()
        .expect("published clear token precedes any attempt");
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
    let mut clear_out: Vec<tiler_core::world_report::ClearAttempt> = Vec::new();
    let stats = apply_world_reply(
        &mut diff,
        &reply,
        &fresh,
        &mut native,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        &mut clear_out,
        1000,
    );
    assert_eq!(stats.clear_writes, 0, "{stats:?}");
    assert!(clear_out.is_empty(), "held gate reports no attempt");
    assert!(stats.held >= 1, "{stats:?}");
    assert_eq!(stats.clear_holds.len(), 1, "{stats:?}");
    assert_eq!(stats.clear_holds[0].op, op);
    assert_eq!(stats.clear_holds[0].window, "w1");
    assert_eq!(stats.clear_holds[0].hold, "unknown-baseline");
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
            window: WindowId("w1".to_owned()),
            direction: "right".to_owned(),
            same_axis: tiler_core::directional::SameAxisMove::default(),
        },
    );
    let op = reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "w1")
        .expect("mover keeps a row")
        .clear_operation
        .clone()
        .expect("published clear token precedes any attempt");
    // Racing input arrives after the desire is published: the pass stops
    // with no setter, no attempt, and one `stale-stopped` attribution.
    let areas = vec![area()];
    let retained: Vec<RetainedRow> = Vec::new();
    let captured = capture_world(&state.inputs(), Some(&maximized), &retained, &areas, 0);
    let fresh = FreshHost::from_sources(&state.inputs(), &maximized, &retained, &captured);
    let mut native = FakeNative::seeded(&fresh);
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
    let mut clear_out: Vec<tiler_core::world_report::ClearAttempt> = Vec::new();
    let stats = apply_world_reply(
        &mut diff,
        &reply,
        &fresh,
        &mut native,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        &mut clear_out,
        1000,
    );
    assert_eq!(stats.clear_writes, 0, "{stats:?}");
    assert!(clear_out.is_empty(), "stopped pass reports no attempt");
    assert!(stats.stopped_mid_apply, "{stats:?}");
    assert_eq!(stats.clear_holds.len(), 1, "{stats:?}");
    assert_eq!(stats.clear_holds_total, 1, "{stats:?}");
    assert_eq!(stats.clear_holds[0].op, op);
    assert_eq!(stats.clear_holds[0].window, "w1");
    assert_eq!(stats.clear_holds[0].hold, "stale-stopped");
}

/// Mint a retained-overlay maximize-clear reply: tiled admission for w1/w2,
/// retained tile slot for w1, maximized subject through the retained lane
/// with the given observed rect (`None` models an unreadable overlay row),
/// one directional intent. Returns the owner, sequencer, accepted reply,
/// published clear op, live-lane peer observations and retained rows for
/// production-shaped diff inputs.
#[allow(clippy::type_complexity)]
fn mint_retained_overlay_clear(
    retained_rect: Option<Rect>,
    slot: Rect,
) -> (
    super::TileLoop,
    WorldSequencer,
    tiler_core::world_report::WorldReply,
    String,
    Vec<ObservedWindow>,
    Vec<RetainedRow>,
) {
    let mut state = test_state();
    let mut seq = WorldSequencer::bootstrap("adapter-1", "gen-1").expect("bootstrap");
    let _ = seq.bootstrap_reply("req-0");
    state.ensure_device("DEV1");
    seed_tiled_member(&mut state, 101, "w1");
    seed_tiled_member(&mut state, 102, "w2");
    let tiles = vec![observed(101, "w1", 100, 100), observed(102, "w2", 300, 100)];
    for _ in 0..2 {
        observe_with_foreground(&mut state, &mut seq, &tiles, 101);
    }
    state.member_rects.insert("w1".to_owned(), slot);
    let mut maximized_facts = plain_facts();
    maximized_facts.maximized = true;
    let retained = vec![RetainedRow {
        key: key(101),
        token: "w1".to_owned(),
        rect: retained_rect,
        maximized: true,
        fullscreen: false,
        facts: Some(maximized_facts),
    }];
    let peer = vec![observed(102, "w2", 300, 100)];
    let areas = vec![area()];
    seq.invalidate();
    let report = seq
        .try_dispatch(
            &state.inputs(),
            Some(&peer),
            &retained,
            &areas,
            101,
            0,
            Some(WorldCommand::MoveDirectional {
                window: WindowId("w1".to_owned()),
                direction: "right".to_owned(),
                same_axis: tiler_core::directional::SameAxisMove::default(),
            }),
        )
        .expect("command dispatches");
    let reply = match seq.deliver(&report) {
        WorldOutcome::Accepted(reply) => match seq.ack(&reply) {
            super::world_sequencer::AckOutcome::Applied(_) => reply,
            other => panic!("expected applied, got {other:?}"),
        },
        other => panic!("expected accepted, got {other:?}"),
    };
    match &reply.command_result {
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
    let op = row
        .clear_operation
        .clone()
        .expect("tiled clear desire published before any attempt");
    (state, seq, reply, op, peer, retained)
}

/// Production-shaped retained-overlay clear: the maximized subject arrives
/// through the retained lane (`classify` Maximized keeps it out of the live
/// lane) while the capture substitutes the retained tile slot, but the
/// production live re-read (`LiveNative::read_window`) returns the maximized
/// DWM frame. The clear race guard must compare against the observation-time
/// live frame, never the slot: one accepted attempt under the original op,
/// then commandless continuation with clear retirement. Fails before the
/// `observed_frame` baseline correction with a `frame-race` hold and zero
/// attempts (the live `op-1`/`w29` signature: published token, zero writes,
/// maximized subject held).
#[test]
fn maximized_tiled_clear_retained_overlay_dispatches_from_live_frame() {
    use super::world_capture::capture_world;
    use super::world_diff::{FakeNative, FreshHost, HostDiff, apply_world_reply};
    use tiler_core::world_report::{ClearAttempt, FrameSetter, OperationOutcome};

    let mut state = test_state();
    let mut seq = WorldSequencer::bootstrap("adapter-1", "gen-1").expect("bootstrap");
    let _ = seq.bootstrap_reply("req-0");
    state.ensure_device("DEV1");
    seed_tiled_member(&mut state, 101, "w1");
    seed_tiled_member(&mut state, 102, "w2");
    let tiles = vec![observed(101, "w1", 100, 100), observed(102, "w2", 300, 100)];
    for _ in 0..2 {
        observe_with_foreground(&mut state, &mut seq, &tiles, 101);
    }
    // Production admission slot for w1: the last tiled snapshot the capture
    // keeps while the subject is maximized (`snapshot_frames` only tracks
    // live-lane windows, so the maximized subject keeps its tile slot).
    let slot = Rect {
        x: 100,
        y: 100,
        w: 200,
        h: 200,
    };
    state.member_rects.insert("w1".to_owned(), slot);
    // Production maximized shape: the DWM live frame grows to the work area
    // while the tile slot is retained. `observe` classifies this out of the
    // live lane into a retained overlay row carrying the LIVE frame.
    let maximized_live = Rect {
        x: 0,
        y: 0,
        w: 800,
        h: 560,
    };
    let mut maximized_facts = plain_facts();
    maximized_facts.maximized = true;
    let retained = vec![RetainedRow {
        key: key(101),
        token: "w1".to_owned(),
        rect: Some(maximized_live),
        maximized: true,
        fullscreen: false,
        facts: Some(maximized_facts),
    }];
    let peer = vec![observed(102, "w2", 300, 100)];
    // One directional intent against the retained-overlay report intake: the
    // core still publishes the actionable clear token under its op.
    let areas = vec![area()];
    seq.invalidate();
    let report = seq
        .try_dispatch(
            &state.inputs(),
            Some(&peer),
            &retained,
            &areas,
            101,
            0,
            Some(WorldCommand::MoveDirectional {
                window: WindowId("w1".to_owned()),
                direction: "right".to_owned(),
                same_axis: tiler_core::directional::SameAxisMove::default(),
            }),
        )
        .expect("command dispatches");
    let reply = match seq.deliver(&report) {
        WorldOutcome::Accepted(reply) => match seq.ack(&reply) {
            super::world_sequencer::AckOutcome::Applied(_) => reply,
            other => panic!("expected applied, got {other:?}"),
        },
        other => panic!("expected accepted, got {other:?}"),
    };
    match &reply.command_result {
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
    let op = row
        .clear_operation
        .clone()
        .expect("tiled clear desire published before any attempt");
    // Production-shaped diff baseline: the capture substitutes the retained
    // slot for the overlay row, exactly as the live leg observed.
    let captured = capture_world(&state.inputs(), Some(&peer), &retained, &areas, 101);
    let fresh = FreshHost::from_sources(&state.inputs(), &peer, &retained, &captured);
    assert_eq!(
        fresh.baseline.get("w1").and_then(|window| window.frame),
        Some(slot),
        "overlay baseline rides the retained slot, got {:?}",
        fresh.baseline.get("w1")
    );
    let mut native = FakeNative::seeded(&fresh);
    // The production live re-read returns the maximized DWM frame, not the
    // slot: `LiveNative::read_window` re-observes through `observe_window`.
    native.windows.get_mut("w1").expect("seeded subject").frame = Some(maximized_live);
    let mut diff = HostDiff::new("adapter-1", "gen-1");
    diff.gate.current_obs = reply.based_on_observation_seq;
    let mut clear_out: Vec<ClearAttempt> = Vec::new();
    let stats = apply_world_reply(
        &mut diff,
        &reply,
        &fresh,
        &mut native,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        &mut clear_out,
        1000,
    );
    assert_eq!(stats.clear_writes, 1, "{stats:?}");
    assert_eq!(clear_out.len(), 1);
    let attempt = &clear_out[0];
    assert_eq!(attempt.operation_id, op);
    assert_eq!(attempt.window.0, "w1");
    assert_eq!(attempt.setter, FrameSetter::Accepted);
    assert_eq!(attempt.pre_maximized, Some(true));
    assert_eq!(attempt.post_maximized, Some(false));
    assert!(
        stats.clear_holds.is_empty(),
        "live frame proves stability, no hold, got {:?}",
        stats.clear_holds
    );
    // Commandless continuation on the diff-generated proof resumes the
    // original tiled intent under the ORIGINAL op with clear retirement:
    // no second command, no second clear.
    seq.note_clear_attempt(attempt.clone());
    let reply = observe_with_foreground(&mut state, &mut seq, &tiles, 101);
    let row = reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "w1")
        .expect("mover keeps a row");
    assert_eq!(
        row.clear_operation, None,
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

/// Unreadable retained overlay: the retained row carries no observed live
/// frame while the capture substitutes the slot, and the live re-read
/// coincidentally equals that slot. Unavailable evidence must hold as
/// unreadable without dispatching: a slot/live coincidence is not observed
/// proof and never backfills the baseline. Zero writes, zero attempts, one
/// honest `hidden-or-unreadable` hold under the original op.
#[test]
fn maximized_tiled_clear_unobserved_overlay_holds_without_dispatch() {
    use super::world_capture::capture_world;
    use super::world_diff::{FakeNative, FreshHost, HostDiff, apply_world_reply};
    use tiler_core::world_report::ClearAttempt;

    let slot = Rect {
        x: 100,
        y: 100,
        w: 200,
        h: 200,
    };
    let (state, _seq, reply, op, peer, retained) = mint_retained_overlay_clear(None, slot);
    let areas = vec![area()];
    let captured = capture_world(&state.inputs(), Some(&peer), &retained, &areas, 101);
    let fresh = FreshHost::from_sources(&state.inputs(), &peer, &retained, &captured);
    assert_eq!(
        fresh.baseline.get("w1").and_then(|window| window.frame),
        Some(slot),
        "overlay baseline rides the retained slot, got {:?}",
        fresh.baseline.get("w1")
    );
    assert_eq!(
        fresh
            .baseline
            .get("w1")
            .and_then(|window| window.observed_frame),
        None,
        "no observed live frame rides an unreadable overlay, got {:?}",
        fresh.baseline.get("w1")
    );
    let mut native = FakeNative::seeded(&fresh);
    // Coincidental equality: the live re-read happens to equal the retained
    // slot. This is not observed proof and must not dispatch.
    native.windows.get_mut("w1").expect("seeded subject").frame = Some(slot);
    let mut diff = HostDiff::new("adapter-1", "gen-1");
    diff.gate.current_obs = reply.based_on_observation_seq;
    let mut clear_out: Vec<ClearAttempt> = Vec::new();
    let stats = apply_world_reply(
        &mut diff,
        &reply,
        &fresh,
        &mut native,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        &mut clear_out,
        1000,
    );
    assert_eq!(stats.clear_writes, 0, "{stats:?}");
    assert!(
        clear_out.is_empty(),
        "unreadable baseline reports no attempt"
    );
    assert!(stats.held >= 1, "{stats:?}");
    assert_eq!(stats.clear_holds.len(), 1, "{stats:?}");
    assert_eq!(stats.clear_holds_total, 1, "{stats:?}");
    assert_eq!(stats.clear_holds[0].op, op);
    assert_eq!(stats.clear_holds[0].window, "w1");
    assert_eq!(stats.clear_holds[0].hold, "hidden-or-unreadable");
}

/// Genuine observed/live disagreement still races: the retained row carries
/// an observed live frame, the slot is substituted, and the live re-read
/// returns a different frame (the user moved the window between observation
/// and dispatch). One honest `frame-race` hold, zero writes, zero attempts.
#[test]
fn maximized_tiled_clear_observed_live_mismatch_races_without_dispatch() {
    use super::world_capture::capture_world;
    use super::world_diff::{FakeNative, FreshHost, HostDiff, apply_world_reply};
    use tiler_core::world_report::ClearAttempt;

    let slot = Rect {
        x: 100,
        y: 100,
        w: 200,
        h: 200,
    };
    let observed_live = Rect {
        x: 0,
        y: 0,
        w: 800,
        h: 560,
    };
    let raced_live = Rect {
        x: 16,
        y: 0,
        w: 800,
        h: 560,
    };
    let (state, _seq, reply, op, peer, retained) =
        mint_retained_overlay_clear(Some(observed_live), slot);
    let areas = vec![area()];
    let captured = capture_world(&state.inputs(), Some(&peer), &retained, &areas, 101);
    let fresh = FreshHost::from_sources(&state.inputs(), &peer, &retained, &captured);
    assert_eq!(
        fresh.baseline.get("w1").and_then(|window| window.frame),
        Some(slot),
        "overlay baseline rides the retained slot, got {:?}",
        fresh.baseline.get("w1")
    );
    assert_eq!(
        fresh
            .baseline
            .get("w1")
            .and_then(|window| window.observed_frame),
        Some(observed_live),
        "race baseline rides the observed live frame, got {:?}",
        fresh.baseline.get("w1")
    );
    let mut native = FakeNative::seeded(&fresh);
    // The user moved the window after the observation: the live re-read no
    // longer matches the observed baseline. The veto must still fire.
    native.windows.get_mut("w1").expect("seeded subject").frame = Some(raced_live);
    let mut diff = HostDiff::new("adapter-1", "gen-1");
    diff.gate.current_obs = reply.based_on_observation_seq;
    let mut clear_out: Vec<ClearAttempt> = Vec::new();
    let stats = apply_world_reply(
        &mut diff,
        &reply,
        &fresh,
        &mut native,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        &mut clear_out,
        1000,
    );
    assert_eq!(stats.clear_writes, 0, "{stats:?}");
    assert!(clear_out.is_empty(), "raced baseline reports no attempt");
    assert!(stats.held >= 1, "{stats:?}");
    assert_eq!(stats.clear_holds.len(), 1, "{stats:?}");
    assert_eq!(stats.clear_holds_total, 1, "{stats:?}");
    assert_eq!(stats.clear_holds[0].op, op);
    assert_eq!(stats.clear_holds[0].window, "w1");
    assert_eq!(stats.clear_holds[0].hold, "frame-race");
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
        false,
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
        window: WindowId(String::new()),
        direction: direction.to_owned(),
    }
}

fn nofocus_dispatch(
    fx: &mut NoFocusFixture,
    cursor: Option<(i32, i32)>,
    direction: &str,
) -> Option<tiler_core::world_report::WorldReply> {
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
    let reply = nofocus_dispatch(&mut fx, Some((10, 10)), "right").expect("accepted");
    assert!(
        reply.command_result.is_none(),
        "success rides the effect: {:?}",
        reply.command_result
    );
    let focus: Vec<&tiler_core::world_report::OneShotEffect> = reply
        .effects
        .iter()
        .filter(|effect| effect.kind == tiler_core::world_report::EffectKind::Focus)
        .collect();
    assert_eq!(focus.len(), 1, "one focus effect: {:?}", reply.effects);
    assert_eq!(focus[0].target.0, "w-n1", "existing destination selection");
    assert!(
        focus[0].id.starts_with("fx-"),
        "one-shot effect id: {}",
        focus[0].id
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
    let reply = nofocus_dispatch(&mut fx, Some((10, 10)), "right").expect("accepted");
    let focus: Vec<&tiler_core::world_report::OneShotEffect> = reply
        .effects
        .iter()
        .filter(|effect| effect.kind == tiler_core::world_report::EffectKind::Focus)
        .collect();
    assert_eq!(
        focus.len(),
        1,
        "empty source still crosses: {:?}",
        reply.effects
    );
    assert_eq!(focus[0].target.0, "w-n1");
    super::clear_pending_seams();
    let _ = std::fs::remove_dir_all(&fx.dir);
}

#[test]
fn no_focus_dispatch_empty_destination_retains() {
    // Adjacent output exists but is empty: no remembered selection, so
    // no effect rather than an invented target.
    let mut fx = nofocus_seeded("nofocus-empty-dst", 1, false);
    let reply = nofocus_dispatch(&mut fx, Some((10, 10)), "right").expect("accepted");
    assert!(
        reply.effects.is_empty(),
        "empty target: {:?}",
        reply.effects
    );
    match &reply.command_result {
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
        false,
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
            false,
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

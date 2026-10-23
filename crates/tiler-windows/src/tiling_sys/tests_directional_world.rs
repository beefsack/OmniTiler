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

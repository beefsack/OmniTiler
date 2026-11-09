//! Core-composed maximize foundation (non-fullscreen): with
//! `caps.maximize == false` the toggle owns a lifetime-local bit that
//! selects the output work-area frame under the existing per-row
//! `Controlled` frame lane. Raw `maximized` stays null, no
//! `Maximized` state target is emitted, the tile slot/allocation (or
//! float base) is retained, and clear restores the base exactly.

use std::collections::BTreeSet;

use crate::boundary::CoreCommand;
use crate::connection::HostCapabilities;
use crate::directional::{SameAxisMove, WindowId};
use crate::geometry::Rect;
use crate::gesture::{GestureIngress, GestureKind, GesturePhase};
use crate::world_commands::CommandApplied;
use crate::world_engine::{KIND_OPERATION, WorldEngine, WorldOutcome};
use crate::world_report::{
    CommandRejectKind, CommandResult, EffectKind, FrameAttempt, FrameSetter, HoldReason,
    RowDiagnosticKind, RowDiagnosticLane, ScopeAuthority, StateAttempt, StateProperty,
    WindowDesire, WindowFact, WorldCommand, WorldReportV4,
};
use crate::world_v4_support_tests::{accepted, fact, report_full, screen};

fn no_max_caps() -> HostCapabilities {
    HostCapabilities {
        maximize: false,
        fullscreen: true,
        ..Default::default()
    }
}

fn engine() -> WorldEngine {
    WorldEngine::new_test_bound_with_capabilities("gen-1", "adapter-1", no_max_caps())
        .expect("generation")
}

fn test_issue(report: &WorldReportV4) -> crate::world_engine::EventIssue {
    crate::world_engine::EventIssue {
        owner: crate::ids::OwnerId::parse("adapter-1").expect("owner"),
        correlation: crate::ids::CorrelationId::parse(&report.request_id).expect("correlation"),
    }
}

/// macOS-style fact: native maximize unsupported, absent value. Sticky
/// is likewise null on the composed route (a non-null sticky with
/// `native_sticky: false` holds admission).
fn cfact(window: &str, output: &str, x: i32) -> WindowFact {
    let mut row = fact(window, output, x);
    row.maximized = None;
    row.sticky = None;
    row
}

fn work_area() -> Rect {
    Rect {
        x: 0,
        y: 0,
        w: 800,
        h: 560,
    }
}

fn bootstrap(seq: u64, windows: Vec<WindowFact>, focused: &str) -> WorldReportV4 {
    report_full(
        seq,
        seq,
        vec![screen("out-a", 0)],
        windows,
        vec![],
        focused,
        None,
        None,
    )
}

fn cmd(
    seq: u64,
    command_seq: u64,
    windows: Vec<WindowFact>,
    focused: &str,
    command: WorldCommand,
) -> WorldReportV4 {
    report_full(
        seq,
        seq,
        vec![screen("out-a", 0)],
        windows,
        vec![],
        focused,
        Some(command_seq),
        Some(command),
    )
}

fn toggle(window: &str) -> WorldCommand {
    WorldCommand::ToggleMaximize {
        window: WindowId(window.to_owned()),
    }
}

fn set_fullscreen(window: &str, value: bool) -> WorldCommand {
    WorldCommand::SetFullscreen {
        window: WindowId(window.to_owned()),
        value,
    }
}

fn toggle_float(window: &str) -> WorldCommand {
    WorldCommand::Operation(CoreCommand::ToggleFloat {
        window: window.to_owned(),
        float_rect: None,
    })
}

fn desire<'a>(reply: &'a crate::world_report::WorldReplyV4, win: &str) -> &'a WindowDesire {
    reply
        .windows
        .iter()
        .find(|row| row.lifetime == WindowId(win.to_owned()))
        .unwrap_or_else(|| panic!("row for {win}"))
}

fn maximize_result(reply: &crate::world_report::WorldReplyV4) -> (bool, bool, bool) {
    match &reply.command_result {
        Some(CommandResult::Maximize {
            maximized,
            clear_fullscreen,
            was_maximized,
            ..
        }) => (*maximized, *clear_fullscreen, *was_maximized),
        other => panic!("expected maximize result, got {other:?}"),
    }
}

#[test]
fn composed_tile_toggle_selects_work_area_and_restores_base() {
    let mut engine = engine();
    let base = accepted(
        &mut engine,
        &bootstrap(
            1,
            vec![cfact("win-a", "out-a", 0), cfact("win-b", "out-a", 100)],
            "win-a",
        ),
    );
    let base_a = desire(&base, "win-a").clone();
    let base_b = desire(&base, "win-b").clone();
    // Null native `maximized` with unsupported caps is the contract-valid
    // absent value: the ordinary row stays fully controlled, never held.
    assert_eq!(base_a.frame_authority, ScopeAuthority::Controlled);
    assert!(base_a.state.is_empty());

    let toggled = accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![cfact("win-a", "out-a", 0), cfact("win-b", "out-a", 100)],
            "win-a",
            toggle("win-a"),
        ),
    );
    assert_eq!(maximize_result(&toggled), (true, false, false));
    let row = desire(&toggled, "win-a");
    assert_eq!(row.frame_authority, ScopeAuthority::Controlled);
    assert_eq!(row.allocation, base_a.allocation, "tile slot retained");
    assert_eq!(row.effective_frame, Some(work_area()));
    assert!(row.state.is_empty(), "no native maximize target");
    assert!(!row.floating);
    // Sibling untouched: no reflow.
    assert_eq!(desire(&toggled, "win-b").allocation, base_b.allocation);
    assert_eq!(
        desire(&toggled, "win-b").effective_frame,
        base_b.effective_frame
    );
    // Raw facts never rewritten.
    assert_eq!(
        engine
            .staged
            .facts
            .get(&WindowId("win-a".to_owned()))
            .and_then(|fact| fact.maximized),
        None
    );

    // Observed at the actual work area: the overlay holds, the base stays.
    let mut observed_fact = cfact("win-a", "out-a", 0);
    observed_fact.rect = Some(work_area());
    let observed = accepted(
        &mut engine,
        &bootstrap(
            3,
            vec![observed_fact, cfact("win-b", "out-a", 100)],
            "win-a",
        ),
    );
    let row = desire(&observed, "win-a");
    assert_eq!(row.effective_frame, Some(work_area()));
    assert_eq!(row.allocation, base_a.allocation);
    assert_eq!(row.frame_authority, ScopeAuthority::Controlled);

    // Clear restores the retained base exactly.
    let mut clear_fact = cfact("win-a", "out-a", 0);
    clear_fact.rect = Some(work_area());
    let cleared = accepted(
        &mut engine,
        &cmd(
            4,
            2,
            vec![clear_fact, cfact("win-b", "out-a", 100)],
            "win-a",
            toggle("win-a"),
        ),
    );
    assert_eq!(maximize_result(&cleared), (false, false, true));
    let row = desire(&cleared, "win-a");
    assert_eq!(row.effective_frame, base_a.effective_frame);
    assert_eq!(row.allocation, base_a.allocation);
    assert_eq!(
        row.frame_authority,
        ScopeAuthority::Controlled,
        "clear authorizes the actual base write"
    );
    assert!(row.state.is_empty());
    assert_eq!(desire(&cleared, "win-b").allocation, base_b.allocation);
}

#[test]
fn composed_float_toggle_uses_controlled_lane_and_restores_base() {
    let mut engine = engine();
    accepted(
        &mut engine,
        &bootstrap(1, vec![cfact("win-a", "out-a", 0)], "win-a"),
    );
    let floated = accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![cfact("win-a", "out-a", 0)],
            "win-a",
            toggle_float("win-a"),
        ),
    );
    let float_base = match &floated.command_result {
        Some(CommandResult::Float { rect, .. }) => rect.expect("float placement"),
        other => panic!("expected float result, got {other:?}"),
    };
    // Settle the one-shot placement at its base frame.
    let mut settled_fact = cfact("win-a", "out-a", 0);
    settled_fact.rect = Some(float_base);
    let settled = accepted(&mut engine, &bootstrap(3, vec![settled_fact], "win-a"));
    let settled_row = desire(&settled, "win-a").clone();
    assert!(settled_row.floating);
    assert_eq!(settled_row.frame_authority, ScopeAuthority::Untouched);
    let settled_base = settled_row.effective_frame.expect("float base");

    let toggled = accepted(
        &mut engine,
        &cmd(
            4,
            2,
            vec![{
                let mut row = cfact("win-a", "out-a", 0);
                row.rect = Some(float_base);
                row
            }],
            "win-a",
            toggle("win-a"),
        ),
    );
    assert_eq!(maximize_result(&toggled), (true, false, false));
    let row = desire(&toggled, "win-a");
    assert!(row.floating, "float membership retained");
    assert_eq!(row.frame_authority, ScopeAuthority::Controlled);
    assert_eq!(row.effective_frame, Some(work_area()));
    assert!(row.frame_operation.is_none(), "no new placement token");
    assert!(row.state.is_empty(), "no native maximize target");

    // Observed at the work area: the overlay holds, the float base is not
    // overwritten by the observed composed frame.
    let mut observed_fact = cfact("win-a", "out-a", 0);
    observed_fact.rect = Some(work_area());
    let observed = accepted(&mut engine, &bootstrap(5, vec![observed_fact], "win-a"));
    assert_eq!(
        desire(&observed, "win-a").effective_frame,
        Some(work_area())
    );

    // Clear restores the retained float base exactly.
    let mut clear_fact = cfact("win-a", "out-a", 0);
    clear_fact.rect = Some(work_area());
    let cleared = accepted(
        &mut engine,
        &cmd(6, 3, vec![clear_fact], "win-a", toggle("win-a")),
    );
    assert_eq!(maximize_result(&cleared), (false, false, true));
    let row = desire(&cleared, "win-a");
    assert!(row.floating);
    assert_eq!(row.effective_frame, Some(settled_base));
    assert_eq!(
        row.frame_operation,
        Some("op-3".to_owned()),
        "clear mints the existing one-shot placement token for the base"
    );
    assert_eq!(row.command_seq, Some(3));
    assert_eq!(row.frame_authority, ScopeAuthority::Untouched);
}

#[test]
fn composed_unavailable_frame_authority_refuses_without_recovery() {
    let mut engine = engine();
    let base = accepted(
        &mut engine,
        &bootstrap(1, vec![cfact("win-a", "out-a", 0)], "win-a"),
    );
    let base_frame = desire(&base, "win-a").effective_frame;

    // Gesture barrier withholds the frame write: existing refusal, no
    // overlay intent change.
    let mut barrier = cmd(
        2,
        1,
        vec![cfact("win-a", "out-a", 0)],
        "win-a",
        toggle("win-a"),
    );
    barrier.gesture_barrier = true;
    match engine.handle_report(&barrier) {
        WorldOutcome::Accepted(reply) => assert_eq!(
            reply.command_result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::OperationRefused
            })
        ),
        other => panic!("expected refusal, got {other:?}"),
    }

    // No autorecovery: the next commandless report keeps the base frame.
    let quiet = accepted(
        &mut engine,
        &bootstrap(3, vec![cfact("win-a", "out-a", 0)], "win-a"),
    );
    assert_eq!(desire(&quiet, "win-a").effective_frame, base_frame);
    assert!(quiet.command_result.is_none());

    // A fresh press composes once authority returns.
    let toggled = accepted(
        &mut engine,
        &cmd(
            4,
            2,
            vec![cfact("win-a", "out-a", 0)],
            "win-a",
            toggle("win-a"),
        ),
    );
    assert_eq!(maximize_result(&toggled), (true, false, false));
    assert_eq!(desire(&toggled, "win-a").effective_frame, Some(work_area()));
}

#[test]
fn composed_null_maximized_admits_ordinarily_and_true_conflicts_fail_closed() {
    let mut engine = engine();
    // Null is the contract-valid absent value: ordinary admission keeps
    // membership and frame controlled, never a composition.
    let base = accepted(
        &mut engine,
        &bootstrap(1, vec![cfact("win-a", "out-a", 0)], "win-a"),
    );
    let base_row = desire(&base, "win-a");
    assert_eq!(base_row.membership_authority, ScopeAuthority::Controlled);
    assert_eq!(base_row.frame_authority, ScopeAuthority::Controlled);
    // A fresh toggle still composes over the null: the owned bit selects
    // the work-area frame while the base stays retained.
    let toggled = accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![cfact("win-a", "out-a", 0)],
            "win-a",
            toggle("win-a"),
        ),
    );
    assert_eq!(maximize_result(&toggled), (true, false, false));
    assert_eq!(desire(&toggled, "win-a").effective_frame, Some(work_area()));

    // A non-null native claim conflicts with the core-owned overlay: the
    // graded row carries no homing, so the existing focus gate refuses
    // with no intent change.
    let mut conflict = cfact("win-a", "out-a", 0);
    conflict.maximized = Some(true);
    match engine.handle_report(&cmd(3, 2, vec![conflict], "win-a", toggle("win-a"))) {
        WorldOutcome::Accepted(reply) => assert_eq!(
            reply.command_result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::FocusUnknown
            })
        ),
        other => panic!("expected refusal, got {other:?}"),
    }
    let quiet = accepted(
        &mut engine,
        &bootstrap(4, vec![cfact("win-a", "out-a", 0)], "win-a"),
    );
    assert!(quiet.command_result.is_none());
    assert_eq!(
        desire(&quiet, "win-a").effective_frame,
        Some(work_area()),
        "refusal changes no overlay intent: the prior bit holds"
    );
}

#[test]
fn composed_some_false_conflicts_fail_closed() {
    // Any non-null native `maximized` (including false) conflicts with the
    // core-owned overlay on unsupported caps: the graded row carries no
    // homing, so the existing focus gate refuses with no intent change.
    let mut engine = engine();
    accepted(
        &mut engine,
        &bootstrap(1, vec![cfact("win-a", "out-a", 0)], "win-a"),
    );
    let mut conflict = cfact("win-a", "out-a", 0);
    conflict.maximized = Some(false);
    match engine.handle_report(&cmd(3, 2, vec![conflict], "win-a", toggle("win-a"))) {
        WorldOutcome::Accepted(reply) => assert_eq!(
            reply.command_result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::FocusUnknown
            })
        ),
        other => panic!("expected refusal, got {other:?}"),
    }
    assert!(
        !engine
            .composed_maximized
            .contains(&WindowId("win-a".to_owned())),
        "refusal mints no overlay intent"
    );
}

#[test]
fn unsupported_nonnull_maximized_grades_conflicting_and_recovers_on_null() {
    // Observation-only regression: with unsupported caps any non-null
    // native `maximized` grades to a narrow conflicting hold (never a
    // Controlled frame), the active composed bit retains without a write,
    // and a later valid null recovers the overlay.
    let mut engine = engine();
    accepted(
        &mut engine,
        &bootstrap(1, vec![cfact("win-a", "out-a", 0)], "win-a"),
    );
    let toggled = accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![cfact("win-a", "out-a", 0)],
            "win-a",
            toggle("win-a"),
        ),
    );
    assert_eq!(maximize_result(&toggled), (true, false, false));
    assert!(
        engine
            .composed_maximized
            .contains(&WindowId("win-a".to_owned())),
        "overlay bit set over the legitimate null"
    );

    for (seq, value) in [(3u64, Some(false)), (4u64, Some(true))] {
        let mut row = cfact("win-a", "out-a", 0);
        row.maximized = value;
        let reply = accepted(&mut engine, &bootstrap(seq, vec![row], "win-a"));
        assert!(
            !reply
                .windows
                .iter()
                .any(|desired| desired.lifetime == WindowId("win-a".to_owned())
                    && desired.frame_authority == ScopeAuthority::Controlled
                    && desired.membership_authority == ScopeAuthority::Controlled),
            "conflicting {value:?} authorizes no controlled frame: {:?}",
            reply.windows
        );
        assert!(
            engine
                .staged
                .held_windows
                .iter()
                .any(|held| held.window == WindowId("win-a".to_owned())),
            "conflicting {value:?} grades to a held row"
        );
        assert!(
            reply
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.row == RowDiagnosticLane::Window
                    && diagnostic.id.as_deref() == Some("win-a")
                    && diagnostic.kind == RowDiagnosticKind::ConflictingFact),
            "conflicting-fact diagnostic for {value:?}: {:?}",
            reply.diagnostics
        );
        assert!(
            engine
                .composed_maximized
                .contains(&WindowId("win-a".to_owned())),
            "graded conflict retains the bit without a write"
        );
    }

    // A command against the graded row refuses through the existing
    // applicable gate (kind follows the structural gate, never reordered).
    let mut conflict = cfact("win-a", "out-a", 0);
    conflict.maximized = Some(false);
    match engine.handle_report(&cmd(5, 2, vec![conflict], "win-a", toggle("win-a"))) {
        WorldOutcome::Accepted(reply) => assert_eq!(
            reply.command_result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::FocusUnknown
            }),
            "expected refusal against the graded row"
        ),
        other => panic!("expected refusal against the graded row, got {other:?}"),
    }

    // A later valid null recovers: the retained bit still selects the
    // work area with the base retained.
    let recovered = accepted(
        &mut engine,
        &bootstrap(6, vec![cfact("win-a", "out-a", 0)], "win-a"),
    );
    assert_eq!(
        desire(&recovered, "win-a").effective_frame,
        Some(work_area())
    );
}

/// Fullscreen fact helper: native fullscreen active with staged
/// exitability for the R-MAX-11 replacement leg.
fn cfull(window: &str, output: &str, x: i32, exitable: Option<bool>) -> WindowFact {
    let mut row = cfact(window, output, x);
    row.fullscreen = Some(true);
    row.fullscreen_exitable = exitable;
    row
}

fn fullscreen_state<'a>(
    reply: &'a crate::world_report::WorldReplyV4,
    win: &str,
) -> Option<&'a crate::world_report::StateTarget> {
    desire(reply, win)
        .state
        .iter()
        .find(|row| row.property == StateProperty::Fullscreen)
}

#[test]
fn composed_tiled_fullscreen_toggle_clears_native_and_withholds_frame_until_exit() {
    let mut engine = engine();
    accepted(
        &mut engine,
        &bootstrap(
            1,
            vec![cfact("win-a", "out-a", 0), cfact("win-b", "out-a", 100)],
            "win-a",
        ),
    );
    // Explicit maximize on native fullscreen: only the native clear is
    // issued (no Maximized target); the owned bit is retained but no
    // composed frame is written while raw fullscreen stays true.
    let replaced = accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![
                cfull("win-a", "out-a", 0, Some(true)),
                cfact("win-b", "out-a", 100),
            ],
            "win-a",
            toggle("win-a"),
        ),
    );
    assert_eq!(maximize_result(&replaced), (true, true, false));
    assert!(
        engine
            .composed_maximized
            .contains(&WindowId("win-a".to_owned())),
        "requested composed intent retained while the native clear is pending"
    );
    let row = desire(&replaced, "win-a");
    assert_eq!(
        row.effective_frame, None,
        "native overlay owns pixels while raw fullscreen stays true"
    );
    assert_eq!(row.frame_authority, ScopeAuthority::Untouched);
    let clear = fullscreen_state(&replaced, "win-a").expect("native fullscreen-false target");
    assert!(!clear.value);
    assert_eq!(clear.authority, ScopeAuthority::Controlled);
    assert_eq!(clear.command_seq, Some(1));
    assert!(
        row.state
            .iter()
            .all(|target| target.property != StateProperty::Maximized),
        "composed route mints no Maximized target"
    );
    // The first readable exit releases the overlay over the retained slot.
    let exited = accepted(
        &mut engine,
        &bootstrap(
            3,
            vec![cfact("win-a", "out-a", 0), cfact("win-b", "out-a", 100)],
            "win-a",
        ),
    );
    let row = desire(&exited, "win-a");
    assert_eq!(row.effective_frame, Some(work_area()));
    assert_eq!(row.frame_authority, ScopeAuthority::Controlled);
    // The overlay recomputes the work area on exit: a changed area is
    // picked up with no new command.
    let changed = Rect {
        x: 0,
        y: 0,
        w: 400,
        h: 300,
    };
    let mut rescreen = bootstrap(
        4,
        vec![cfact("win-a", "out-a", 0), cfact("win-b", "out-a", 100)],
        "win-a",
    );
    rescreen.screens = vec![crate::world_report::ScreenReport {
        work_area: Some(changed),
        ..rescreen.screens[0].clone()
    }];
    let recomputed = accepted(&mut engine, &rescreen);
    assert_eq!(desire(&recomputed, "win-a").effective_frame, Some(changed));
}

#[test]
fn composed_fullscreen_unexitable_or_unknown_refuses_without_intent_change() {
    for (exitable, kind) in [
        (Some(false), CommandRejectKind::FullscreenNotExitable),
        (None, CommandRejectKind::StateUnknown),
    ] {
        let mut engine = engine();
        accepted(
            &mut engine,
            &bootstrap(1, vec![cfact("win-a", "out-a", 0)], "win-a"),
        );
        let reply = accepted(
            &mut engine,
            &cmd(
                2,
                1,
                vec![cfull("win-a", "out-a", 0, exitable)],
                "win-a",
                toggle("win-a"),
            ),
        );
        assert!(
            matches!(&reply.command_result, Some(CommandResult::Rejected { kind: got }) if *got == kind),
            "exitable {exitable:?} rejects as {kind:?}, got {:?}",
            reply.command_result
        );
        assert!(
            !engine
                .composed_maximized
                .contains(&WindowId("win-a".to_owned())),
            "refusal mints no overlay intent"
        );
        assert!(
            fullscreen_state(&reply, "win-a").is_none(),
            "refusal issues no native clear"
        );
        let quiet = accepted(
            &mut engine,
            &bootstrap(3, vec![cfact("win-a", "out-a", 0)], "win-a"),
        );
        assert!(quiet.command_result.is_none());
    }
}

#[test]
fn composed_fullscreen_without_caps_refuses_unsupported_without_intent_change() {
    let caps = HostCapabilities {
        maximize: false,
        ..Default::default()
    };
    let mut engine = WorldEngine::new_test_bound_with_capabilities("gen-1", "adapter-1", caps)
        .expect("generation");
    accepted(
        &mut engine,
        &bootstrap(1, vec![cfact("win-a", "out-a", 0)], "win-a"),
    );
    let reply = accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![cfull("win-a", "out-a", 0, Some(true))],
            "win-a",
            toggle("win-a"),
        ),
    );
    assert!(
        matches!(
            &reply.command_result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::StateUnsupported
            })
        ),
        "missing fullscreen caps rejects unsupported, got {:?}",
        reply.command_result
    );
    assert!(
        !engine
            .composed_maximized
            .contains(&WindowId("win-a".to_owned()))
    );
}

#[test]
fn composed_float_fullscreen_toggle_withholds_overlay_until_raw_exit() {
    let mut engine = engine();
    accepted(
        &mut engine,
        &bootstrap(1, vec![cfact("win-a", "out-a", 0)], "win-a"),
    );
    let floated = accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![cfact("win-a", "out-a", 0)],
            "win-a",
            toggle_float("win-a"),
        ),
    );
    let float_base = match &floated.command_result {
        Some(CommandResult::Float { rect, .. }) => rect.expect("float placement"),
        other => panic!("expected float result, got {other:?}"),
    };
    let mut settled_fact = cfact("win-a", "out-a", 0);
    settled_fact.rect = Some(float_base);
    accepted(&mut engine, &bootstrap(3, vec![settled_fact], "win-a"));
    // Explicit maximize on native fullscreen over the float base: the
    // native clear issues Controlled while the overlay stays withheld at
    // the untouched base with no placement token.
    let mut full_fact = cfact("win-a", "out-a", 0);
    full_fact.rect = Some(float_base);
    full_fact.fullscreen = Some(true);
    full_fact.fullscreen_exitable = Some(true);
    let replaced = accepted(
        &mut engine,
        &cmd(4, 2, vec![full_fact], "win-a", toggle("win-a")),
    );
    assert_eq!(maximize_result(&replaced), (true, true, false));
    let row = desire(&replaced, "win-a");
    assert!(row.floating);
    assert_eq!(row.effective_frame, Some(float_base));
    assert_eq!(row.frame_authority, ScopeAuthority::Untouched);
    assert!(row.frame_operation.is_none());
    let clear = fullscreen_state(&replaced, "win-a").expect("native fullscreen-false target");
    assert!(!clear.value);
    assert_eq!(clear.authority, ScopeAuthority::Controlled);
    // Raw exit releases the work-area overlay; the float base is untouched.
    let mut exit_fact = cfact("win-a", "out-a", 0);
    exit_fact.rect = Some(float_base);
    let exited = accepted(&mut engine, &bootstrap(5, vec![exit_fact], "win-a"));
    let row = desire(&exited, "win-a");
    assert_eq!(row.effective_frame, Some(work_area()));
    assert_eq!(row.frame_authority, ScopeAuthority::Controlled);
}

#[test]
fn composed_max_on_revokes_stale_same_window_float_placement() {
    let mut engine = engine();
    accepted(
        &mut engine,
        &bootstrap(
            1,
            vec![cfact("win-a", "out-a", 0), cfact("win-b", "out-a", 100)],
            "win-a",
        ),
    );
    accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![cfact("win-a", "out-a", 0), cfact("win-b", "out-a", 100)],
            "win-a",
            toggle_float("win-a"),
        ),
    );
    let float_b = accepted(
        &mut engine,
        &cmd(
            3,
            2,
            vec![cfact("win-a", "out-a", 0), cfact("win-b", "out-a", 100)],
            "win-b",
            toggle_float("win-b"),
        ),
    );
    assert!(
        float_b
            .windows
            .iter()
            .find(|row| row.lifetime == WindowId("win-b".to_owned()))
            .and_then(|row| row.frame_operation.clone())
            .is_some(),
        "win-b placement pending before the maximize"
    );
    // Explicit max-on revokes the obsolete same-window placement token
    // before the work-area frame; other windows keep theirs.
    let toggled = accepted(
        &mut engine,
        &cmd(
            4,
            3,
            vec![cfact("win-a", "out-a", 0), cfact("win-b", "out-a", 100)],
            "win-a",
            toggle("win-a"),
        ),
    );
    assert_eq!(maximize_result(&toggled), (true, false, false));
    let row = desire(&toggled, "win-a");
    assert_eq!(row.effective_frame, Some(work_area()));
    assert_eq!(row.frame_authority, ScopeAuthority::Controlled);
    assert!(row.frame_operation.is_none());
    assert!(
        engine
            .retained_floats
            .iter()
            .all(|held| held.window != WindowId("win-a".to_owned())),
        "stale same-window placement revoked"
    );
    assert!(
        engine
            .retained_floats
            .iter()
            .any(|held| held.window == WindowId("win-b".to_owned())),
        "other windows keep their placements"
    );
}

#[test]
fn composed_float_suppressed_pair_holds_clamp_without_retry() {
    let mut engine = engine();
    accepted(
        &mut engine,
        &bootstrap(1, vec![cfact("win-a", "out-a", 0)], "win-a"),
    );
    accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![cfact("win-a", "out-a", 0)],
            "win-a",
            toggle_float("win-a"),
        ),
    );
    let floated = accepted(
        &mut engine,
        &cmd(
            3,
            2,
            vec![cfact("win-a", "out-a", 0)],
            "win-a",
            toggle("win-a"),
        ),
    );
    assert_eq!(maximize_result(&floated), (true, false, false));
    // Mechanical clamp facts: accepted setter, stable mismatched readback
    // against the work-area target.
    let clamped = Rect {
        x: 0,
        y: 0,
        w: work_area().w - 100,
        h: work_area().h,
    };
    let mut clamped_fact = cfact("win-a", "out-a", 0);
    clamped_fact.rect = Some(clamped);
    let mut clamped_report = bootstrap(4, vec![clamped_fact], "win-a");
    clamped_report.monotonic_ms = 1000;
    clamped_report.frames = vec![FrameAttempt {
        window: WindowId("win-a".to_owned()),
        target: work_area(),
        setter: FrameSetter::Accepted,
        readback: Some(clamped),
        dpi_before: None,
        dpi_after: None,
        attempt_ms: 1000,
        operation_id: None,
        prewrite: None,
    }];
    let reply = accepted(&mut engine, &clamped_report);
    let row = desire(&reply, "win-a");
    assert_eq!(
        row.effective_frame,
        Some(work_area()),
        "clamp hold keeps the desire intact"
    );
    assert_eq!(
        row.frame_authority,
        ScopeAuthority::Hold(crate::world_report::HoldReason::ConstraintClamp)
    );
}

fn fullscreen_revision(engine: &WorldEngine, window: &str) -> u64 {
    engine
        .retained_states
        .get(&(WindowId(window.to_owned()), StateProperty::Fullscreen))
        .expect("pending fullscreen entry")
        .target
        .revision
}

fn fullscreen_applied_report(
    seq: u64,
    windows: Vec<WindowFact>,
    focused: &str,
    window: &str,
    revision: u64,
) -> WorldReportV4 {
    let mut report = bootstrap(seq, windows, focused);
    report.state_attempts = vec![StateAttempt {
        window: WindowId(window.to_owned()),
        property: StateProperty::Fullscreen,
        target: true,
        revision,
        setter: FrameSetter::Accepted,
        pre: Some(false),
        post: Some(true),
        attempt_ms: 1,
    }];
    report
}

#[test]
fn composed_tiled_full_entry_pending_withholds_base_frame_until_exit() {
    let mut engine = engine();
    let base = accepted(
        &mut engine,
        &bootstrap(
            1,
            vec![cfact("win-a", "out-a", 0), cfact("win-b", "out-a", 100)],
            "win-a",
        ),
    );
    let base_slot = desire(&base, "win-a").allocation.expect("tile slot");
    let toggled = accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![cfact("win-a", "out-a", 0), cfact("win-b", "out-a", 100)],
            "win-a",
            toggle("win-a"),
        ),
    );
    assert_eq!(desire(&toggled, "win-a").effective_frame, Some(work_area()));
    // Native fullscreen entry queues while raw still reads false: the
    // overlay must withhold every frame write, so the queued entry never
    // shares a reply with a tile write.
    let pending = accepted(
        &mut engine,
        &cmd(
            3,
            2,
            vec![cfact("win-a", "out-a", 0), cfact("win-b", "out-a", 100)],
            "win-a",
            set_fullscreen("win-a", true),
        ),
    );
    assert!(
        engine
            .composed_maximized
            .contains(&WindowId("win-a".to_owned())),
        "pending entry retains the composed bit"
    );
    let row = desire(&pending, "win-a");
    assert_eq!(
        row.frame_authority,
        ScopeAuthority::Hold(HoldReason::ConstraintClamp),
        "pending entry withholds the base frame write: {row:?}"
    );
    assert_eq!(row.effective_frame, Some(base_slot));
    assert_eq!(row.allocation, Some(base_slot));
    let clear = fullscreen_state(&pending, "win-a").expect("pending fullscreen-true target");
    assert!(clear.value);
    assert_eq!(clear.authority, ScopeAuthority::Controlled);
    // The applied fullscreen owns pixels while raw reads true.
    let revision = fullscreen_revision(&engine, "win-a");
    let mut applied_fact = cfact("win-a", "out-a", 0);
    applied_fact.fullscreen = Some(true);
    applied_fact.fullscreen_exitable = Some(true);
    let applied = accepted(
        &mut engine,
        &fullscreen_applied_report(
            4,
            vec![applied_fact, cfact("win-b", "out-a", 100)],
            "win-a",
            "win-a",
            revision,
        ),
    );
    let row = desire(&applied, "win-a");
    assert_eq!(row.effective_frame, None);
    assert_eq!(row.frame_authority, ScopeAuthority::Untouched);
    // The first readable exit recomputes the overlay over the retained slot.
    let changed = Rect {
        x: 0,
        y: 0,
        w: 400,
        h: 300,
    };
    let mut rescreen = bootstrap(
        5,
        vec![cfact("win-a", "out-a", 0), cfact("win-b", "out-a", 100)],
        "win-a",
    );
    rescreen.screens = vec![crate::world_report::ScreenReport {
        work_area: Some(changed),
        ..rescreen.screens[0].clone()
    }];
    let exited = accepted(&mut engine, &rescreen);
    let row = desire(&exited, "win-a");
    assert_eq!(row.effective_frame, Some(changed));
    assert_eq!(row.frame_authority, ScopeAuthority::Controlled);
    assert!(
        row.allocation.is_some() && row.allocation != Some(base_slot),
        "the overlay recomputes the retained slot under the new area: {row:?}"
    );
}

#[test]
fn composed_float_full_entry_pending_withholds_restore_token_until_exit() {
    let mut engine = engine();
    accepted(
        &mut engine,
        &bootstrap(1, vec![cfact("win-a", "out-a", 0)], "win-a"),
    );
    let floated = accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![cfact("win-a", "out-a", 0)],
            "win-a",
            toggle_float("win-a"),
        ),
    );
    let float_base = match &floated.command_result {
        Some(CommandResult::Float { rect, .. }) => rect.expect("float placement"),
        other => panic!("expected float result, got {other:?}"),
    };
    let mut settled_fact = cfact("win-a", "out-a", 0);
    settled_fact.rect = Some(float_base);
    accepted(&mut engine, &bootstrap(3, vec![settled_fact], "win-a"));
    let toggled = accepted(
        &mut engine,
        &cmd(
            4,
            2,
            vec![{
                let mut row = cfact("win-a", "out-a", 0);
                row.rect = Some(float_base);
                row
            }],
            "win-a",
            toggle("win-a"),
        ),
    );
    assert_eq!(desire(&toggled, "win-a").effective_frame, Some(work_area()));
    // Queued fullscreen entry withholds the overlay and any restore token.
    let pending = accepted(
        &mut engine,
        &cmd(
            5,
            3,
            vec![{
                let mut row = cfact("win-a", "out-a", 0);
                row.rect = Some(float_base);
                row
            }],
            "win-a",
            set_fullscreen("win-a", true),
        ),
    );
    assert!(
        engine
            .composed_maximized
            .contains(&WindowId("win-a".to_owned())),
        "pending entry retains the composed bit"
    );
    let row = desire(&pending, "win-a");
    assert!(row.floating);
    assert_eq!(row.effective_frame, Some(float_base));
    assert_eq!(row.frame_authority, ScopeAuthority::Untouched);
    assert!(
        row.frame_operation.is_none(),
        "pending entry withholds the restore token: {row:?}"
    );
    // Clearing the overlay while the entry pends refuses: the bit removal
    // itself is a withheld frame write.
    match engine.handle_report(&cmd(
        6,
        4,
        vec![{
            let mut row = cfact("win-a", "out-a", 0);
            row.rect = Some(float_base);
            row
        }],
        "win-a",
        toggle("win-a"),
    )) {
        crate::world_engine::WorldOutcome::Accepted(reply) => assert_eq!(
            reply.command_result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::OperationRefused
            })
        ),
        other => panic!("expected refusal while the entry pends, got {other:?}"),
    }
    assert!(
        engine
            .composed_maximized
            .contains(&WindowId("win-a".to_owned())),
        "refusal retains the bit"
    );
    // Applied fullscreen owns pixels; the readable exit restores the overlay.
    let revision = fullscreen_revision(&engine, "win-a");
    let mut applied_fact = cfact("win-a", "out-a", 0);
    applied_fact.rect = Some(float_base);
    applied_fact.fullscreen = Some(true);
    applied_fact.fullscreen_exitable = Some(true);
    let applied = accepted(
        &mut engine,
        &fullscreen_applied_report(7, vec![applied_fact], "win-a", "win-a", revision),
    );
    let row = desire(&applied, "win-a");
    assert_eq!(row.effective_frame, Some(float_base));
    assert_eq!(row.frame_authority, ScopeAuthority::Untouched);
    let mut exit_fact = cfact("win-a", "out-a", 0);
    exit_fact.rect = Some(float_base);
    let exited = accepted(&mut engine, &bootstrap(8, vec![exit_fact], "win-a"));
    let row = desire(&exited, "win-a");
    assert_eq!(row.effective_frame, Some(work_area()));
    assert_eq!(row.frame_authority, ScopeAuthority::Controlled);
}

#[test]
fn composed_tiled_max_off_while_full_entry_pending_refuses_without_intent_change() {
    let mut engine = engine();
    accepted(
        &mut engine,
        &bootstrap(
            1,
            vec![cfact("win-a", "out-a", 0), cfact("win-b", "out-a", 100)],
            "win-a",
        ),
    );
    accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![cfact("win-a", "out-a", 0), cfact("win-b", "out-a", 100)],
            "win-a",
            toggle("win-a"),
        ),
    );
    accepted(
        &mut engine,
        &cmd(
            3,
            2,
            vec![cfact("win-a", "out-a", 0), cfact("win-b", "out-a", 100)],
            "win-a",
            set_fullscreen("win-a", true),
        ),
    );
    // Max-off removes the overlay bit, which would write the base frame
    // beside the queued entry: refuse with the bit retained.
    match engine.handle_report(&cmd(
        4,
        3,
        vec![cfact("win-a", "out-a", 0), cfact("win-b", "out-a", 100)],
        "win-a",
        toggle("win-a"),
    )) {
        crate::world_engine::WorldOutcome::Accepted(reply) => assert_eq!(
            reply.command_result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::OperationRefused
            })
        ),
        other => panic!("expected refusal while the entry pends, got {other:?}"),
    }
    assert!(
        engine
            .composed_maximized
            .contains(&WindowId("win-a".to_owned())),
        "refusal retains the bit"
    );
    let quiet = accepted(
        &mut engine,
        &bootstrap(
            5,
            vec![cfact("win-a", "out-a", 0), cfact("win-b", "out-a", 100)],
            "win-a",
        ),
    );
    assert_eq!(
        desire(&quiet, "win-a").frame_authority,
        ScopeAuthority::Hold(HoldReason::ConstraintClamp),
        "no autorecovery while the entry pends"
    );
}

#[test]
fn composed_max_on_revokes_stale_pending_fullscreen_entry() {
    // Latest-explicit precedence through the existing native lane: an
    // ordinary composed max-on supersedes an older queued fullscreen entry
    // exactly like an ordinary native maximize, so the overlay activates
    // immediately with no new policy fork.
    let mut engine = engine();
    accepted(
        &mut engine,
        &bootstrap(
            1,
            vec![cfact("win-a", "out-a", 0), cfact("win-b", "out-a", 100)],
            "win-a",
        ),
    );
    let pending = accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![cfact("win-a", "out-a", 0), cfact("win-b", "out-a", 100)],
            "win-a",
            set_fullscreen("win-a", true),
        ),
    );
    let entry = fullscreen_state(&pending, "win-a").expect("pending fullscreen-true target");
    assert!(entry.value);
    let toggled = accepted(
        &mut engine,
        &cmd(
            3,
            2,
            vec![cfact("win-a", "out-a", 0), cfact("win-b", "out-a", 100)],
            "win-a",
            toggle("win-a"),
        ),
    );
    assert_eq!(maximize_result(&toggled), (true, false, false));
    assert!(
        !engine
            .retained_states
            .contains_key(&(WindowId("win-a".to_owned()), StateProperty::Fullscreen)),
        "stale fullscreen entry revoked by the explicit composed command"
    );
    let row = desire(&toggled, "win-a");
    assert_eq!(row.effective_frame, Some(work_area()));
    assert_eq!(row.frame_authority, ScopeAuthority::Controlled);
    assert!(
        fullscreen_state(&toggled, "win-a").is_none(),
        "revoked entry publishes no fullscreen target"
    );
}

/// Gesture helpers: explicit `GestureIngress` commit attribution only, never
/// geometry heuristics. All facts stay `caps.maximize == false` null facts.
fn gingress(
    window: &str,
    kind: GestureKind,
    cursor: Option<(i32, i32)>,
    phase: GesturePhase,
    start_frame: Rect,
) -> GestureIngress {
    GestureIngress {
        id: 1,
        window: WindowId(window.to_owned()),
        kind,
        cursor,
        start_cursor: None,
        phase,
        esc: false,
        modal_move: kind == GestureKind::Move,
        start_frame,
    }
}

fn greport(
    seq: u64,
    windows: Vec<WindowFact>,
    focused: &str,
    ingress: GestureIngress,
) -> WorldReportV4 {
    let mut report = bootstrap(seq, windows, focused);
    report.gesture_barrier = true;
    report.gesture = Some(ingress);
    report
}

fn toggle_on(engine: &mut WorldEngine, base_a_allocation: &mut Option<Rect>) {
    let toggled = accepted(
        engine,
        &cmd(
            2,
            1,
            vec![cfact("win-a", "out-a", 0), cfact("win-b", "out-a", 100)],
            "win-a",
            toggle("win-a"),
        ),
    );
    assert_eq!(maximize_result(&toggled), (true, false, false));
    assert_eq!(desire(&toggled, "win-a").effective_frame, Some(work_area()));
    *base_a_allocation = desire(&toggled, "win-a").allocation;
    assert!(
        engine
            .composed_maximized
            .contains(&WindowId("win-a".to_owned())),
        "overlay bit set before the gesture"
    );
}

#[test]
fn composed_committed_move_clears_bit_before_reflow() {
    let mut engine = engine();
    accepted(
        &mut engine,
        &bootstrap(
            1,
            vec![cfact("win-a", "out-a", 0), cfact("win-b", "out-a", 100)],
            "win-a",
        ),
    );
    let mut _alloc = None;
    toggle_on(&mut engine, &mut _alloc);
    // START binds while the overlay shows the work area.
    let mut start_fact = cfact("win-a", "out-a", 0);
    start_fact.rect = Some(work_area());
    let start = accepted(
        &mut engine,
        &greport(
            3,
            vec![start_fact, cfact("win-b", "out-a", 100)],
            "win-a",
            gingress(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Started,
                work_area(),
            ),
        ),
    );
    assert!(start.command_result.is_none());
    assert!(
        engine
            .composed_maximized
            .contains(&WindowId("win-a".to_owned())),
        "start binds without clearing"
    );
    // RELEASE moves: same size, new origin, drop point inside the inset.
    let mut moved_fact = cfact("win-a", "out-a", 0);
    moved_fact.rect = Some(Rect {
        x: 10,
        y: 0,
        w: work_area().w,
        h: work_area().h,
    });
    let released = accepted(
        &mut engine,
        &greport(
            4,
            vec![moved_fact, cfact("win-b", "out-a", 100)],
            "win-a",
            gingress(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Released,
                work_area(),
            ),
        ),
    );
    assert!(
        !engine
            .composed_maximized
            .contains(&WindowId("win-a".to_owned())),
        "committed move clears the bit"
    );
    assert!(
        released
            .gesture_settled
            .is_some_and(|settled| settled.committed),
        "commit attribution rides GestureSettled, got {:?}",
        released.gesture_settled
    );
    let row = desire(&released, "win-a");
    assert_ne!(
        row.effective_frame,
        Some(work_area()),
        "reflow publishes the committed base, not the overlay"
    );
    assert!(
        row.allocation.is_some(),
        "tile slot retained through the gesture clear"
    );
    assert!(row.state.is_empty(), "no native maximize target");
    // The terminal report still carries the gesture barrier, so the base
    // write authorizes on the next quiet report.
    let quiet = accepted(
        &mut engine,
        &bootstrap(
            5,
            vec![cfact("win-a", "out-a", 0), cfact("win-b", "out-a", 100)],
            "win-a",
        ),
    );
    let row = desire(&quiet, "win-a");
    assert_ne!(
        row.effective_frame,
        Some(work_area()),
        "quiet follow-up publishes the committed base"
    );
    assert_eq!(row.frame_authority, ScopeAuthority::Controlled);
}

#[test]
fn composed_committed_resize_clears_bit() {
    let mut engine = engine();
    accepted(
        &mut engine,
        &bootstrap(
            1,
            vec![cfact("win-a", "out-a", 0), cfact("win-b", "out-a", 100)],
            "win-a",
        ),
    );
    let mut _alloc = None;
    toggle_on(&mut engine, &mut _alloc);
    let mut start_fact = cfact("win-a", "out-a", 0);
    start_fact.rect = Some(work_area());
    accepted(
        &mut engine,
        &greport(
            3,
            vec![start_fact, cfact("win-b", "out-a", 100)],
            "win-a",
            gingress(
                "win-a",
                GestureKind::Resize,
                Some((350, 50)),
                GesturePhase::Started,
                work_area(),
            ),
        ),
    );
    // Left-edge share change: origin moves in while the far edge stays,
    // deriving a `left` resize near the tiled shared edge.
    let mut resized_fact = cfact("win-a", "out-a", 0);
    resized_fact.rect = Some(Rect {
        x: 350,
        y: 0,
        w: 450,
        h: work_area().h,
    });
    let released = accepted(
        &mut engine,
        &greport(
            4,
            vec![resized_fact, cfact("win-b", "out-a", 100)],
            "win-a",
            gingress(
                "win-a",
                GestureKind::Resize,
                Some((350, 50)),
                GesturePhase::Released,
                work_area(),
            ),
        ),
    );
    assert!(
        !engine
            .composed_maximized
            .contains(&WindowId("win-a".to_owned())),
        "committed resize clears the bit"
    );
    assert!(
        released
            .gesture_settled
            .is_some_and(|settled| settled.committed),
        "resize commit attribution, got {:?}",
        released.gesture_settled
    );
    assert_ne!(
        desire(&released, "win-a").effective_frame,
        Some(work_area())
    );
}

#[test]
fn composed_noop_release_retains_bit() {
    let mut engine = engine();
    accepted(
        &mut engine,
        &bootstrap(
            1,
            vec![cfact("win-a", "out-a", 0), cfact("win-b", "out-a", 100)],
            "win-a",
        ),
    );
    let mut _alloc = None;
    toggle_on(&mut engine, &mut _alloc);
    // Same frame plus a cursor is no change: snap back, no commit.
    let mut same_fact = cfact("win-a", "out-a", 0);
    same_fact.rect = Some(work_area());
    let released = accepted(
        &mut engine,
        &greport(
            3,
            vec![same_fact, cfact("win-b", "out-a", 100)],
            "win-a",
            gingress(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Released,
                work_area(),
            ),
        ),
    );
    assert!(
        engine
            .composed_maximized
            .contains(&WindowId("win-a".to_owned())),
        "no-op snap-back retains the bit"
    );
    assert!(
        released
            .gesture_settled
            .is_none_or(|settled| !settled.committed),
        "no-op never commits, got {:?}",
        released.gesture_settled
    );
    assert_eq!(
        desire(&released, "win-a").effective_frame,
        Some(work_area())
    );
}

#[test]
fn composed_cancel_retains_bit() {
    let mut engine = engine();
    accepted(
        &mut engine,
        &bootstrap(
            1,
            vec![cfact("win-a", "out-a", 0), cfact("win-b", "out-a", 100)],
            "win-a",
        ),
    );
    let mut _alloc = None;
    toggle_on(&mut engine, &mut _alloc);
    let mut start_fact = cfact("win-a", "out-a", 0);
    start_fact.rect = Some(work_area());
    accepted(
        &mut engine,
        &greport(
            3,
            vec![start_fact, cfact("win-b", "out-a", 100)],
            "win-a",
            gingress(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Updated,
                work_area(),
            ),
        ),
    );
    let cancelled = accepted(
        &mut engine,
        &greport(
            4,
            vec![
                {
                    let mut row = cfact("win-a", "out-a", 0);
                    row.rect = Some(work_area());
                    row
                },
                cfact("win-b", "out-a", 100),
            ],
            "win-a",
            gingress(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Cancelled,
                work_area(),
            ),
        ),
    );
    assert!(
        engine
            .composed_maximized
            .contains(&WindowId("win-a".to_owned())),
        "cancel retains the bit"
    );
    assert!(
        cancelled
            .gesture_settled
            .is_none_or(|settled| !settled.committed),
        "cancel never commits, got {:?}",
        cancelled.gesture_settled
    );
    assert_eq!(
        desire(&cancelled, "win-a").effective_frame,
        Some(work_area())
    );
}

#[test]
fn composed_refused_drop_retains_bit() {
    let mut engine = engine();
    accepted(
        &mut engine,
        &bootstrap(
            1,
            vec![cfact("win-a", "out-a", 0), cfact("win-b", "out-a", 100)],
            "win-a",
        ),
    );
    let mut _alloc = None;
    toggle_on(&mut engine, &mut _alloc);
    // Drop outside every output refuses with snap-back and no transfer.
    let mut moved_fact = cfact("win-a", "out-a", 0);
    moved_fact.rect = Some(Rect {
        x: 10,
        y: 0,
        w: work_area().w,
        h: work_area().h,
    });
    let released = accepted(
        &mut engine,
        &greport(
            3,
            vec![moved_fact, cfact("win-b", "out-a", 100)],
            "win-a",
            gingress(
                "win-a",
                GestureKind::Move,
                Some((5000, 100)),
                GesturePhase::Released,
                work_area(),
            ),
        ),
    );
    assert!(
        engine
            .composed_maximized
            .contains(&WindowId("win-a".to_owned())),
        "refused drop retains the bit"
    );
    assert!(
        released
            .gesture_settled
            .is_none_or(|settled| !settled.committed),
        "refused gesture never commits, got {:?}",
        released.gesture_settled
    );
    assert_eq!(
        desire(&released, "win-a").effective_frame,
        Some(work_area())
    );
}

#[test]
fn composed_clamped_frame_readback_retains_bit() {
    let mut engine = engine();
    accepted(
        &mut engine,
        &bootstrap(
            1,
            vec![cfact("win-a", "out-a", 0), cfact("win-b", "out-a", 100)],
            "win-a",
        ),
    );
    let mut _alloc = None;
    toggle_on(&mut engine, &mut _alloc);
    // Mechanical clamp facts never clear: accepted setter, stable
    // mismatched readback against the work-area target.
    let clamped = Rect {
        x: 0,
        y: 0,
        w: work_area().w - 100,
        h: work_area().h,
    };
    let mut clamped_fact = cfact("win-a", "out-a", 0);
    clamped_fact.rect = Some(clamped);
    let mut clamped_report =
        bootstrap(3, vec![clamped_fact, cfact("win-b", "out-a", 100)], "win-a");
    clamped_report.monotonic_ms = 1000;
    clamped_report.frames = vec![FrameAttempt {
        window: WindowId("win-a".to_owned()),
        target: work_area(),
        setter: FrameSetter::Accepted,
        readback: Some(clamped),
        dpi_before: None,
        dpi_after: None,
        attempt_ms: 1000,
        operation_id: None,
        prewrite: None,
    }];
    let reply = accepted(&mut engine, &clamped_report);
    assert!(
        engine
            .composed_maximized
            .contains(&WindowId("win-a".to_owned())),
        "clamped readback retains the bit"
    );
    assert_eq!(desire(&reply, "win-a").effective_frame, Some(work_area()));
}

#[test]
fn composed_close_retires_bit_and_restart_births_ordinary() {
    let mut eng = engine();
    accepted(
        &mut eng,
        &bootstrap(
            1,
            vec![cfact("win-a", "out-a", 0), cfact("win-b", "out-a", 100)],
            "win-a",
        ),
    );
    let mut _alloc = None;
    toggle_on(&mut eng, &mut _alloc);
    // Explicit close retires the lifetime-local bit at runtime.
    let mut closed_report = bootstrap(3, vec![cfact("win-b", "out-a", 100)], "win-b");
    closed_report.closed = vec![WindowId("win-a".to_owned())];
    accepted(&mut eng, &closed_report);
    assert!(
        !eng.composed_maximized
            .contains(&WindowId("win-a".to_owned())),
        "close retires the bit"
    );
    // Restart drops the bit with the engine: a fresh backend births the
    // same lifetimes ordinary with base frames, never the overlay.
    let mut restarted = engine();
    assert!(
        !restarted
            .composed_maximized
            .contains(&WindowId("win-a".to_owned())),
        "fresh engine carries no overlay"
    );
    let reborn = accepted(
        &mut restarted,
        &bootstrap(
            1,
            vec![cfact("win-a", "out-a", 0), cfact("win-b", "out-a", 100)],
            "win-a",
        ),
    );
    assert_ne!(
        desire(&reborn, "win-a").effective_frame,
        Some(work_area()),
        "restart births the ordinary base frame"
    );
    assert!(reborn.command_result.is_none());
}

#[test]
fn composed_rehandshake_drops_bit() {
    let mut engine = engine();
    accepted(
        &mut engine,
        &bootstrap(
            1,
            vec![cfact("win-a", "out-a", 0), cfact("win-b", "out-a", 100)],
            "win-a",
        ),
    );
    let mut _alloc = None;
    toggle_on(&mut engine, &mut _alloc);
    let reply = engine.handshake(&crate::connection::HandshakeRequest {
        adapter: "adapter-1".to_owned(),
        request: "reconnect-1".to_owned(),
        application_identity: crate::connection::ApplicationIdentity {
            app_id: None,
            executable: Some("test-own-executable".to_owned()),
            process_id: Some("test-own-pid".to_owned()),
        },
        capabilities: no_max_caps(),
    });
    assert!(
        reply.kind.is_none(),
        "rehandshake binds, got {:?}",
        reply.kind
    );
    assert!(
        !engine
            .composed_maximized
            .contains(&WindowId("win-a".to_owned())),
        "rehandshake runtime drops the bit"
    );
}

/// Composed-float gesture setup: float win-a, settle its base, then compose
/// the overlay on top. Returns the settled float base.
fn float_compose_on(engine: &mut WorldEngine) -> Rect {
    accepted(
        &mut *engine,
        &bootstrap(1, vec![cfact("win-a", "out-a", 0)], "win-a"),
    );
    let floated = accepted(
        &mut *engine,
        &cmd(
            2,
            1,
            vec![cfact("win-a", "out-a", 0)],
            "win-a",
            toggle_float("win-a"),
        ),
    );
    let float_base = match &floated.command_result {
        Some(CommandResult::Float { rect, .. }) => rect.expect("float placement"),
        other => panic!("expected float result, got {other:?}"),
    };
    let mut settled_fact = cfact("win-a", "out-a", 0);
    settled_fact.rect = Some(float_base);
    accepted(&mut *engine, &bootstrap(3, vec![settled_fact], "win-a"));
    let mut toggle_fact = cfact("win-a", "out-a", 0);
    toggle_fact.rect = Some(float_base);
    let toggled = accepted(
        &mut *engine,
        &cmd(4, 2, vec![toggle_fact], "win-a", toggle("win-a")),
    );
    assert_eq!(maximize_result(&toggled), (true, false, false));
    assert_eq!(desire(&toggled, "win-a").effective_frame, Some(work_area()));
    assert!(
        engine
            .composed_maximized
            .contains(&WindowId("win-a".to_owned())),
        "overlay bit set before the float gesture"
    );
    float_base
}

#[test]
fn composed_float_committed_move_adopts_release() {
    let mut engine = engine();
    let _base = float_compose_on(&mut engine);
    let mut start_fact = cfact("win-a", "out-a", 0);
    start_fact.rect = Some(work_area());
    accepted(
        &mut engine,
        &greport(
            5,
            vec![start_fact],
            "win-a",
            gingress(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Started,
                work_area(),
            ),
        ),
    );
    assert!(
        engine
            .composed_maximized
            .contains(&WindowId("win-a".to_owned())),
        "start binds without clearing"
    );
    let release = Rect {
        x: 100,
        y: 50,
        w: 400,
        h: 300,
    };
    let mut moved_fact = cfact("win-a", "out-a", 0);
    moved_fact.rect = Some(release);
    let released = accepted(
        &mut engine,
        &greport(
            6,
            vec![moved_fact],
            "win-a",
            gingress(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Released,
                work_area(),
            ),
        ),
    );
    assert!(
        !engine
            .composed_maximized
            .contains(&WindowId("win-a".to_owned())),
        "committed float move clears the bit"
    );
    assert!(
        released
            .gesture_settled
            .is_some_and(|settled| settled.committed),
        "commit attribution rides GestureSettled, got {:?}",
        released.gesture_settled
    );
    let row = desire(&released, "win-a").clone();
    assert!(row.floating, "float membership retained");
    assert_eq!(row.effective_frame, Some(release));
    assert_eq!(row.frame_authority, ScopeAuthority::Untouched);
    assert!(row.frame_operation.is_none(), "no placement token");
    assert!(
        engine
            .retained_floats
            .iter()
            .all(|held| held.window != WindowId("win-a".to_owned())),
        "no stale placement token survives the adoption"
    );
    let mut quiet_fact = cfact("win-a", "out-a", 0);
    quiet_fact.rect = Some(release);
    let quiet = accepted(&mut engine, &bootstrap(7, vec![quiet_fact], "win-a"));
    let row = desire(&quiet, "win-a").clone();
    assert!(row.floating);
    assert_eq!(row.effective_frame, Some(release));
    assert_eq!(row.frame_authority, ScopeAuthority::Untouched);
}

#[test]
fn composed_float_committed_resize_adopts_release() {
    let mut engine = engine();
    let _base = float_compose_on(&mut engine);
    let mut start_fact = cfact("win-a", "out-a", 0);
    start_fact.rect = Some(work_area());
    accepted(
        &mut engine,
        &greport(
            5,
            vec![start_fact],
            "win-a",
            gingress(
                "win-a",
                GestureKind::Resize,
                Some((350, 50)),
                GesturePhase::Started,
                work_area(),
            ),
        ),
    );
    let release = Rect {
        x: 0,
        y: 0,
        w: 400,
        h: 300,
    };
    let mut resized_fact = cfact("win-a", "out-a", 0);
    resized_fact.rect = Some(release);
    let released = accepted(
        &mut engine,
        &greport(
            6,
            vec![resized_fact],
            "win-a",
            gingress(
                "win-a",
                GestureKind::Resize,
                Some((400, 300)),
                GesturePhase::Released,
                work_area(),
            ),
        ),
    );
    assert!(
        !engine
            .composed_maximized
            .contains(&WindowId("win-a".to_owned())),
        "committed float resize clears the bit"
    );
    assert!(
        released
            .gesture_settled
            .is_some_and(|settled| settled.committed),
        "resize commit attribution, got {:?}",
        released.gesture_settled
    );
    let row = desire(&released, "win-a").clone();
    assert!(row.floating);
    assert_eq!(row.effective_frame, Some(release));
}

#[test]
fn composed_float_noop_release_retains_bit() {
    let mut engine = engine();
    let _base = float_compose_on(&mut engine);
    let mut same_fact = cfact("win-a", "out-a", 0);
    same_fact.rect = Some(work_area());
    let released = accepted(
        &mut engine,
        &greport(
            5,
            vec![same_fact],
            "win-a",
            gingress(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Released,
                work_area(),
            ),
        ),
    );
    assert!(
        engine
            .composed_maximized
            .contains(&WindowId("win-a".to_owned())),
        "no-op snap-back retains the bit"
    );
    assert!(
        released
            .gesture_settled
            .is_none_or(|settled| !settled.committed),
        "no-op never commits, got {:?}",
        released.gesture_settled
    );
    assert_eq!(
        desire(&released, "win-a").effective_frame,
        Some(work_area())
    );
}

#[test]
fn composed_float_cancel_retains_bit() {
    let mut engine = engine();
    let _base = float_compose_on(&mut engine);
    let mut start_fact = cfact("win-a", "out-a", 0);
    start_fact.rect = Some(work_area());
    accepted(
        &mut engine,
        &greport(
            5,
            vec![start_fact],
            "win-a",
            gingress(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Updated,
                work_area(),
            ),
        ),
    );
    let mut cancel_fact = cfact("win-a", "out-a", 0);
    cancel_fact.rect = Some(work_area());
    let cancelled = accepted(
        &mut engine,
        &greport(
            6,
            vec![cancel_fact],
            "win-a",
            gingress(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Cancelled,
                work_area(),
            ),
        ),
    );
    assert!(
        engine
            .composed_maximized
            .contains(&WindowId("win-a".to_owned())),
        "cancel retains the bit"
    );
    assert!(
        cancelled
            .gesture_settled
            .is_none_or(|settled| !settled.committed),
        "cancel never commits, got {:?}",
        cancelled.gesture_settled
    );
    assert_eq!(
        desire(&cancelled, "win-a").effective_frame,
        Some(work_area())
    );
}

#[test]
fn composed_float_cursorless_resize_retains_bit() {
    let mut engine = engine();
    let _base = float_compose_on(&mut engine);
    let mut start_fact = cfact("win-a", "out-a", 0);
    start_fact.rect = Some(work_area());
    accepted(
        &mut engine,
        &greport(
            5,
            vec![start_fact],
            "win-a",
            gingress(
                "win-a",
                GestureKind::Resize,
                Some((350, 50)),
                GesturePhase::Started,
                work_area(),
            ),
        ),
    );
    // A native resize always carries a pointer: without one fail closed.
    let resized = Rect {
        x: 0,
        y: 0,
        w: 400,
        h: 300,
    };
    let mut resized_fact = cfact("win-a", "out-a", 0);
    resized_fact.rect = Some(resized);
    let released = accepted(
        &mut engine,
        &greport(
            6,
            vec![resized_fact],
            "win-a",
            gingress(
                "win-a",
                GestureKind::Resize,
                None,
                GesturePhase::Released,
                work_area(),
            ),
        ),
    );
    assert!(
        engine
            .composed_maximized
            .contains(&WindowId("win-a".to_owned())),
        "cursor-less resize retains the bit"
    );
    assert!(
        released
            .gesture_settled
            .is_none_or(|settled| !settled.committed),
        "cursor-less resize never commits, got {:?}",
        released.gesture_settled
    );
}

#[test]
fn composed_float_unknown_output_release_retains_bit() {
    let mut engine = engine();
    let _base = float_compose_on(&mut engine);
    let mut start_fact = cfact("win-a", "out-a", 0);
    start_fact.rect = Some(work_area());
    accepted(
        &mut engine,
        &greport(
            5,
            vec![start_fact],
            "win-a",
            gingress(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Started,
                work_area(),
            ),
        ),
    );
    // Only out-a is reported: a release naming unknown out-b refuses
    // with the bit retained, unlike a valid cross-output release.
    let mut moved_fact = cfact("win-a", "out-b", 0);
    moved_fact.rect = Some(Rect {
        x: 900,
        y: 50,
        w: 400,
        h: 300,
    });
    let released = accepted(
        &mut engine,
        &greport(
            6,
            vec![moved_fact],
            "win-a",
            gingress(
                "win-a",
                GestureKind::Move,
                Some((950, 60)),
                GesturePhase::Released,
                work_area(),
            ),
        ),
    );
    assert!(
        engine
            .composed_maximized
            .contains(&WindowId("win-a".to_owned())),
        "unknown-output float release retains the bit"
    );
    assert!(
        released
            .gesture_settled
            .is_none_or(|settled| !settled.committed),
        "unknown-output float release never commits, got {:?}",
        released.gesture_settled
    );
}

fn greport_two(
    seq: u64,
    windows: Vec<WindowFact>,
    focused: &str,
    ingress: GestureIngress,
) -> WorldReportV4 {
    let mut report = bootstrap_two(seq, windows, focused);
    report.gesture_barrier = true;
    report.gesture = Some(ingress);
    report
}

#[test]
fn composed_float_cross_output_release_clears_and_rehomes() {
    let mut engine = engine();
    let windows = || vec![cfact("win-a", "out-a", 0), cfact("win-c", "out-b", 900)];
    accepted(&mut engine, &bootstrap_two(1, windows(), "win-a"));
    let floated = accepted(
        &mut engine,
        &cmd_two(2, 1, windows(), "win-a", toggle_float("win-a")),
    );
    let float_base = match &floated.command_result {
        Some(CommandResult::Float { rect, .. }) => rect.expect("float placement"),
        other => panic!("expected float result, got {other:?}"),
    };
    let mut settled_fact = cfact("win-a", "out-a", 0);
    settled_fact.rect = Some(float_base);
    accepted(
        &mut engine,
        &bootstrap_two(3, vec![settled_fact, cfact("win-c", "out-b", 900)], "win-a"),
    );
    let mut toggle_fact = cfact("win-a", "out-a", 0);
    toggle_fact.rect = Some(float_base);
    let toggled = accepted(
        &mut engine,
        &cmd_two(
            4,
            2,
            vec![toggle_fact, cfact("win-c", "out-b", 900)],
            "win-a",
            toggle("win-a"),
        ),
    );
    assert_eq!(maximize_result(&toggled), (true, false, false));
    assert_eq!(desire(&toggled, "win-a").effective_frame, Some(work_area()));
    assert!(
        engine
            .composed_maximized
            .contains(&WindowId("win-a".to_owned()))
    );
    let mut start_fact = cfact("win-a", "out-a", 0);
    start_fact.rect = Some(work_area());
    accepted(
        &mut engine,
        &greport_two(
            5,
            vec![start_fact, cfact("win-c", "out-b", 900)],
            "win-a",
            gingress(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Started,
                work_area(),
            ),
        ),
    );
    assert!(
        engine
            .composed_maximized
            .contains(&WindowId("win-a".to_owned())),
        "start binds without clearing"
    );
    // Physically dragged to the other output: the release adopts the live
    // frame there and rehomes through the existing membership owner.
    let release = Rect {
        x: 900,
        y: 50,
        w: 400,
        h: 300,
    };
    let mut moved_fact = cfact("win-a", "out-b", 0);
    moved_fact.rect = Some(release);
    let released = accepted(
        &mut engine,
        &greport_two(
            6,
            vec![moved_fact, cfact("win-c", "out-b", 900)],
            "win-a",
            gingress(
                "win-a",
                GestureKind::Move,
                Some((950, 60)),
                GesturePhase::Released,
                work_area(),
            ),
        ),
    );
    assert!(
        !engine
            .composed_maximized
            .contains(&WindowId("win-a".to_owned())),
        "cross-output float move clears the bit"
    );
    assert!(
        released
            .gesture_settled
            .is_some_and(|settled| settled.committed),
        "commit attribution rides GestureSettled, got {:?}",
        released.gesture_settled
    );
    let row = desire(&released, "win-a").clone();
    assert!(row.floating, "float membership retained");
    assert_eq!(row.effective_frame, Some(release));
    assert_eq!(row.frame_authority, ScopeAuthority::Untouched);
    assert!(row.frame_operation.is_none(), "no placement token");
    assert!(
        engine
            .retained_floats
            .iter()
            .all(|held| held.window != WindowId("win-a".to_owned())),
        "no stale placement token survives the adoption"
    );
    assert_eq!(
        engine
            .staged_homing(&WindowId("win-a".to_owned()))
            .expect("homing")
            .output
            .0,
        "out-b",
        "membership rehomes to the release output"
    );
    // Quiet follow-up: no overlay reassert, the adopted base publishes.
    let mut quiet_fact = cfact("win-a", "out-b", 0);
    quiet_fact.rect = Some(release);
    let quiet = accepted(
        &mut engine,
        &bootstrap_two(7, vec![quiet_fact, cfact("win-c", "out-b", 900)], "win-a"),
    );
    let row = desire(&quiet, "win-a").clone();
    assert!(row.floating, "float membership survives the quiet report");
    assert_eq!(row.effective_frame, Some(release));
    assert_ne!(
        row.effective_frame,
        Some(work_area_out_b()),
        "no old overlay reasserts on the destination"
    );
    assert_eq!(row.frame_authority, ScopeAuthority::Untouched);
}

#[test]
fn composed_empty_output_drag_matches_ordinary_float_outcome() {
    // Parity regression: a composed float dragged to a valid but empty
    // output must end exactly where an ordinary float dragged to the same
    // empty output ends. Same fixture, same release observation; the only
    // difference is the composed bit at bind time.
    let release = Rect {
        x: 900,
        y: 50,
        w: 400,
        h: 300,
    };
    let drag_release = |engine: &mut WorldEngine, seq: u64, base: Rect| {
        let mut moved_fact = cfact("win-a", "out-b", 0);
        moved_fact.rect = Some(release);
        accepted(
            engine,
            &greport_two(
                seq,
                vec![moved_fact],
                "win-a",
                gingress(
                    "win-a",
                    GestureKind::Move,
                    Some((950, 60)),
                    GesturePhase::Released,
                    base,
                ),
            ),
        )
    };
    // Ordinary arm: float on out-a, then the native drag observes the
    // release frame on the empty out-b.
    let mut ordinary = engine();
    accepted(
        &mut ordinary,
        &bootstrap_two(1, vec![cfact("win-a", "out-a", 0)], "win-a"),
    );
    let floated = accepted(
        &mut ordinary,
        &cmd_two(
            2,
            1,
            vec![cfact("win-a", "out-a", 0)],
            "win-a",
            toggle_float("win-a"),
        ),
    );
    let float_base = match &floated.command_result {
        Some(CommandResult::Float { rect, .. }) => rect.expect("float placement"),
        other => panic!("expected float result, got {other:?}"),
    };
    let mut settled_fact = cfact("win-a", "out-a", 0);
    settled_fact.rect = Some(float_base);
    accepted(
        &mut ordinary,
        &bootstrap_two(3, vec![settled_fact], "win-a"),
    );
    let ordinary_reply = drag_release(&mut ordinary, 4, float_base);
    // Composed arm: identical fixture plus the composed overlay.
    let mut composed = engine();
    accepted(
        &mut composed,
        &bootstrap_two(1, vec![cfact("win-a", "out-a", 0)], "win-a"),
    );
    let floated = accepted(
        &mut composed,
        &cmd_two(
            2,
            1,
            vec![cfact("win-a", "out-a", 0)],
            "win-a",
            toggle_float("win-a"),
        ),
    );
    let float_base = match &floated.command_result {
        Some(CommandResult::Float { rect, .. }) => rect.expect("float placement"),
        other => panic!("expected float result, got {other:?}"),
    };
    let mut settled_fact = cfact("win-a", "out-a", 0);
    settled_fact.rect = Some(float_base);
    accepted(
        &mut composed,
        &bootstrap_two(3, vec![settled_fact], "win-a"),
    );
    let mut toggle_fact = cfact("win-a", "out-a", 0);
    toggle_fact.rect = Some(float_base);
    let toggled = accepted(
        &mut composed,
        &cmd_two(4, 2, vec![toggle_fact], "win-a", toggle("win-a")),
    );
    assert_eq!(maximize_result(&toggled), (true, false, false));
    assert!(
        composed
            .composed_maximized
            .contains(&WindowId("win-a".to_owned()))
    );
    let mut start_fact = cfact("win-a", "out-a", 0);
    start_fact.rect = Some(work_area());
    accepted(
        &mut composed,
        &greport_two(
            5,
            vec![start_fact],
            "win-a",
            gingress(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Started,
                work_area(),
            ),
        ),
    );
    let composed_reply = drag_release(&mut composed, 6, work_area());
    // The explicit drag clears the bit regardless of destination session.
    assert!(
        !composed
            .composed_maximized
            .contains(&WindowId("win-a".to_owned())),
        "explicit empty-output drag clears the bit"
    );
    // Parity with the ordinary dragged outcome on the same fixture.
    let ordinary_row = desire(&ordinary_reply, "win-a").clone();
    let composed_row = desire(&composed_reply, "win-a").clone();
    assert_eq!(
        (
            ordinary_row.output.clone(),
            ordinary_row.workspace.clone(),
            ordinary_row.floating,
            ordinary_row.effective_frame,
            ordinary_row.frame_authority,
            ordinary_row.membership_authority,
        ),
        (
            composed_row.output.clone(),
            composed_row.workspace.clone(),
            composed_row.floating,
            composed_row.effective_frame,
            composed_row.frame_authority,
            composed_row.membership_authority,
        ),
        "composed-then-dragged matches ordinary-float-dragged, ordinary={ordinary_row:?} composed={composed_row:?}",
    );
    assert_eq!(
        ordinary_reply
            .gesture_settled
            .map(|settled| settled.committed),
        composed_reply
            .gesture_settled
            .map(|settled| settled.committed),
        "commit attribution matches the ordinary outcome",
    );
    // Quiet follow-up on the same observation: no overlay reasserts, and
    // the base stays whatever the ordinary path produces.
    let mut ordinary_quiet_fact = cfact("win-a", "out-b", 0);
    ordinary_quiet_fact.rect = Some(release);
    let ordinary_quiet = accepted(
        &mut ordinary,
        &bootstrap_two(5, vec![ordinary_quiet_fact], "win-a"),
    );
    let mut composed_quiet_fact = cfact("win-a", "out-b", 0);
    composed_quiet_fact.rect = Some(release);
    let composed_quiet = accepted(
        &mut composed,
        &bootstrap_two(7, vec![composed_quiet_fact], "win-a"),
    );
    assert!(
        !composed
            .composed_maximized
            .contains(&WindowId("win-a".to_owned())),
        "quiet follow-up keeps the bit cleared"
    );
    let ordinary_quiet_row = desire(&ordinary_quiet, "win-a").clone();
    let composed_quiet_row = desire(&composed_quiet, "win-a").clone();
    assert_eq!(
        (
            ordinary_quiet_row.output.clone(),
            ordinary_quiet_row.workspace.clone(),
            ordinary_quiet_row.floating,
            ordinary_quiet_row.effective_frame,
            ordinary_quiet_row.frame_authority,
            ordinary_quiet_row.membership_authority,
        ),
        (
            composed_quiet_row.output.clone(),
            composed_quiet_row.workspace.clone(),
            composed_quiet_row.floating,
            composed_quiet_row.effective_frame,
            composed_quiet_row.frame_authority,
            composed_quiet_row.membership_authority,
        ),
        "quiet base matches the ordinary outcome, ordinary={ordinary_quiet_row:?} composed={composed_quiet_row:?}",
    );
}

#[test]
fn ordinary_float_gesture_refuses_without_session() {
    let mut engine = engine();
    accepted(
        &mut engine,
        &bootstrap(1, vec![cfact("win-a", "out-a", 0)], "win-a"),
    );
    let floated = accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![cfact("win-a", "out-a", 0)],
            "win-a",
            toggle_float("win-a"),
        ),
    );
    let float_base = match &floated.command_result {
        Some(CommandResult::Float { rect, .. }) => rect.expect("float placement"),
        other => panic!("expected float result, got {other:?}"),
    };
    // No composed overlay: ordinary float gestures never bind.
    let mut settled_fact = cfact("win-a", "out-a", 0);
    settled_fact.rect = Some(float_base);
    accepted(&mut engine, &bootstrap(3, vec![settled_fact], "win-a"));
    assert!(
        !engine
            .composed_maximized
            .contains(&WindowId("win-a".to_owned())),
        "ordinary float carries no overlay bit"
    );
    let mut start_fact = cfact("win-a", "out-a", 0);
    start_fact.rect = Some(float_base);
    let reply = accepted(
        &mut engine,
        &greport(
            4,
            vec![start_fact],
            "win-a",
            gingress(
                "win-a",
                GestureKind::Move,
                Some((150, 50)),
                GesturePhase::Started,
                float_base,
            ),
        ),
    );
    assert!(
        reply.gesture_settled.is_none(),
        "ordinary float binds no session"
    );
    assert!(
        !engine
            .composed_maximized
            .contains(&WindowId("win-a".to_owned())),
        "ordinary float refusal mints no overlay"
    );
}

#[test]
fn composed_float_clamped_frame_readback_retains_bit() {
    let mut engine = engine();
    let _base = float_compose_on(&mut engine);
    let clamped = Rect {
        x: 0,
        y: 0,
        w: work_area().w - 100,
        h: work_area().h,
    };
    let mut clamped_fact = cfact("win-a", "out-a", 0);
    clamped_fact.rect = Some(clamped);
    let mut clamped_report = bootstrap(5, vec![clamped_fact], "win-a");
    clamped_report.monotonic_ms = 1000;
    clamped_report.frames = vec![FrameAttempt {
        window: WindowId("win-a".to_owned()),
        target: work_area(),
        setter: FrameSetter::Accepted,
        readback: Some(clamped),
        dpi_before: None,
        dpi_after: None,
        attempt_ms: 1000,
        operation_id: None,
        prewrite: None,
    }];
    let reply = accepted(&mut engine, &clamped_report);
    assert!(
        engine
            .composed_maximized
            .contains(&WindowId("win-a".to_owned())),
        "clamped readback retains the bit"
    );
    assert_eq!(desire(&reply, "win-a").effective_frame, Some(work_area()));
}

// ---- Item3: composed directional focus/move + B9 intentional unfloat. ----

fn two_screens() -> Vec<crate::world_report::ScreenReport> {
    vec![screen("out-a", 0), screen("out-b", 800)]
}

fn bootstrap_two(seq: u64, windows: Vec<WindowFact>, focused: &str) -> WorldReportV4 {
    report_full(
        seq,
        seq,
        two_screens(),
        windows,
        vec![],
        focused,
        None,
        None,
    )
}

fn cmd_two(
    seq: u64,
    command_seq: u64,
    windows: Vec<WindowFact>,
    focused: &str,
    command: WorldCommand,
) -> WorldReportV4 {
    report_full(
        seq,
        seq,
        two_screens(),
        windows,
        vec![],
        focused,
        Some(command_seq),
        Some(command),
    )
}

fn focus_dir(window: &str, direction: &str) -> WorldCommand {
    WorldCommand::FocusDirectional {
        window: WindowId(window.to_owned()),
        direction: direction.to_owned(),
    }
}

fn move_dir(window: &str, direction: &str) -> WorldCommand {
    WorldCommand::MoveDirectional {
        window: WindowId(window.to_owned()),
        direction: direction.to_owned(),
        same_axis: SameAxisMove::default(),
    }
}

fn directional_outcome(reply: &crate::world_report::WorldReplyV4) -> Option<&'static str> {
    match &reply.command_result {
        Some(CommandResult::Directional { outcome, .. }) => Some(outcome.as_str()),
        _ => None,
    }
}

fn focus_effect_targets(reply: &crate::world_report::WorldReplyV4) -> Vec<WindowId> {
    reply
        .effects
        .iter()
        .filter(|effect| effect.kind == EffectKind::Focus)
        .map(|effect| effect.target.clone())
        .collect()
}

#[test]
fn composed_tiled_focus_crosses_adjacent_output() {
    let mut engine = engine();
    let windows = || {
        vec![
            cfact("win-a", "out-a", 0),
            cfact("win-b", "out-a", 100),
            cfact("win-c", "out-b", 900),
        ]
    };
    accepted(&mut engine, &bootstrap_two(1, windows(), "win-a"));
    let toggled = accepted(
        &mut engine,
        &cmd_two(2, 1, windows(), "win-a", toggle("win-a")),
    );
    assert_eq!(maximize_result(&toggled), (true, false, false));
    assert!(
        engine
            .composed_maximized
            .contains(&WindowId("win-a".to_owned()))
    );
    // Same-output sibling win-b waits in the press direction; the composed
    // subject skips it like the native overlay and continues to out-b.
    let reply = accepted(
        &mut engine,
        &cmd_two(3, 2, windows(), "win-a", focus_dir("win-a", "right")),
    );
    assert_eq!(
        focus_effect_targets(&reply),
        vec![WindowId("win-c".to_owned())]
    );
    assert!(
        engine
            .composed_maximized
            .contains(&WindowId("win-a".to_owned())),
        "cross focus retains the bit"
    );
    let row = desire(&reply, "win-a");
    assert_eq!(row.clear_operation, None, "no native clear minted");
    assert!(row.state.is_empty(), "no native state target");
    assert!(engine.retained_clears.is_empty());
}

#[test]
fn composed_float_focus_refuses_maximize() {
    // Native parity: `early_verdict` fences a maximized float subject with
    // `FocusRefusedMaximize` before the float lattice, so the composed
    // overlay refuses a float/sticky subject the same way.
    let mut engine = engine();
    let _base = float_compose_on(&mut engine);
    let mut overlay_fact = cfact("win-a", "out-a", 0);
    overlay_fact.rect = Some(work_area());
    let reply = accepted(
        &mut engine,
        &cmd(
            5,
            3,
            vec![overlay_fact],
            "win-a",
            focus_dir("win-a", "right"),
        ),
    );
    assert_eq!(directional_outcome(&reply), Some("focus-refused-maximize"));
    assert!(focus_effect_targets(&reply).is_empty());
    assert!(
        engine
            .composed_maximized
            .contains(&WindowId("win-a".to_owned())),
        "refusal retains the bit"
    );
}

#[test]
fn composed_tiled_focus_vertical_and_single_output_refuse_unchanged() {
    let mut engine = engine();
    accepted(
        &mut engine,
        &bootstrap(
            1,
            vec![cfact("win-a", "out-a", 0), cfact("win-b", "out-a", 100)],
            "win-a",
        ),
    );
    let mut _alloc = None;
    toggle_on(&mut engine, &mut _alloc);
    let windows = || vec![cfact("win-a", "out-a", 0), cfact("win-b", "out-a", 100)];
    match engine.handle_report(&cmd(3, 2, windows(), "win-a", focus_dir("win-a", "up"))) {
        WorldOutcome::Accepted(reply) => assert_eq!(
            reply.command_result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::OperationRefused
            })
        ),
        other => panic!("vertical composed focus refuses unchanged, got {other:?}"),
    }
    assert!(
        engine
            .composed_maximized
            .contains(&WindowId("win-a".to_owned()))
    );
    match engine.handle_report(&cmd(4, 3, windows(), "win-a", focus_dir("win-a", "right"))) {
        WorldOutcome::Accepted(reply) => assert_eq!(
            reply.command_result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::OperationRefused
            })
        ),
        other => panic!("single-output composed focus refuses unchanged, got {other:?}"),
    }
    assert!(
        engine
            .composed_maximized
            .contains(&WindowId("win-a".to_owned()))
    );
}

#[test]
fn composed_tiled_move_clears_bit_without_native_clear() {
    let mut engine = engine();
    let windows = || {
        vec![
            cfact("win-a", "out-a", 0),
            cfact("win-b", "out-a", 100),
            cfact("win-c", "out-a", 200),
        ]
    };
    accepted(&mut engine, &bootstrap(1, windows(), "win-a"));
    let toggled = accepted(&mut engine, &cmd(2, 1, windows(), "win-a", toggle("win-a")));
    assert_eq!(maximize_result(&toggled), (true, false, false));
    assert_eq!(desire(&toggled, "win-a").effective_frame, Some(work_area()));
    assert!(
        engine
            .composed_maximized
            .contains(&WindowId("win-a".to_owned())),
        "overlay bit set before the move"
    );
    // Swap-with-neighbor guarantees a same-output reorder here (plain
    // group-with-neighbor nests, which can report planner-noop); the
    // composed clear policy is mode-independent.
    let reply = accepted(
        &mut engine,
        &cmd(
            3,
            2,
            windows(),
            "win-a",
            WorldCommand::MoveDirectional {
                window: WindowId("win-a".to_owned()),
                direction: "right".to_owned(),
                same_axis: SameAxisMove::SwapWithNeighbor,
            },
        ),
    );
    assert!(
        !engine
            .composed_maximized
            .contains(&WindowId("win-a".to_owned())),
        "ordinary move clears the bit locally"
    );
    assert!(engine.retained_clears.is_empty(), "no native clear bound");
    assert!(
        reply.command_result.is_none(),
        "tiled success carries no result"
    );
    assert_eq!(
        focus_effect_targets(&reply),
        vec![WindowId("win-a".to_owned())],
        "mover keeps focus through one effect"
    );
    let row = desire(&reply, "win-a");
    assert_eq!(row.clear_operation, None, "no clear desire published");
    assert_ne!(
        row.effective_frame,
        Some(work_area()),
        "reflow publishes the base, not the overlay"
    );
}

#[test]
fn composed_move_fullscreen_refuses_without_clear() {
    let mut engine = engine();
    accepted(
        &mut engine,
        &bootstrap(
            1,
            vec![cfact("win-a", "out-a", 0), cfact("win-b", "out-a", 100)],
            "win-a",
        ),
    );
    let mut _alloc = None;
    toggle_on(&mut engine, &mut _alloc);
    let reply = accepted(
        &mut engine,
        &cmd(
            3,
            2,
            vec![
                cfull("win-a", "out-a", 0, Some(true)),
                cfact("win-b", "out-a", 100),
            ],
            "win-a",
            move_dir("win-a", "right"),
        ),
    );
    assert_eq!(directional_outcome(&reply), Some("move-refused-fullscreen"));
    assert!(
        engine
            .composed_maximized
            .contains(&WindowId("win-a".to_owned())),
        "fullscreen refusal retains the bit"
    );
    assert!(engine.retained_clears.is_empty());
}

#[test]
fn composed_move_in_flight_restores_bit() {
    let mut engine = engine();
    let windows = || {
        vec![
            cfact("win-a", "out-a", 0),
            cfact("win-b", "out-a", 100),
            cfact("win-c", "out-b", 900),
        ]
    };
    accepted(&mut engine, &bootstrap_two(1, windows(), "win-a"));
    accepted(
        &mut engine,
        &cmd_two(2, 1, windows(), "win-a", toggle("win-a")),
    );
    assert!(
        engine
            .composed_maximized
            .contains(&WindowId("win-a".to_owned()))
    );
    let sent = accepted(
        &mut engine,
        &cmd_two(
            3,
            2,
            windows(),
            "win-a",
            WorldCommand::SendDirectional {
                window: WindowId("win-a".to_owned()),
                direction: "right".to_owned(),
                follow: false,
            },
        ),
    );
    assert!(
        !sent.transfers.is_empty(),
        "send retains a flight, got {:?}",
        sent.transfers
    );
    assert!(
        engine
            .composed_maximized
            .contains(&WindowId("win-a".to_owned()))
    );
    match engine.handle_report(&cmd_two(
        4,
        3,
        windows(),
        "win-a",
        move_dir("win-a", "right"),
    )) {
        WorldOutcome::Accepted(reply) => assert_eq!(
            reply.command_result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::TransferInFlight
            })
        ),
        other => panic!("move under a flight refuses, got {other:?}"),
    }
    assert!(
        engine
            .composed_maximized
            .contains(&WindowId("win-a".to_owned())),
        "in-flight refusal restores the cleared bit"
    );
}

#[test]
fn b9_float_unfloat_clears_bit_and_tiles() {
    let mut engine = engine();
    let _base = float_compose_on(&mut engine);
    let mut overlay_fact = cfact("win-a", "out-a", 0);
    overlay_fact.rect = Some(work_area());
    let reply = accepted(
        &mut engine,
        &cmd(5, 3, vec![overlay_fact], "win-a", toggle_float("win-a")),
    );
    assert!(
        !engine
            .composed_maximized
            .contains(&WindowId("win-a".to_owned())),
        "intentional unfloat clears the bit"
    );
    match &reply.command_result {
        Some(CommandResult::Float { window, rect, .. }) => {
            assert_eq!(window, &WindowId("win-a".to_owned()));
            assert_eq!(*rect, None, "unfloat-to-tile carries no rect");
        }
        other => panic!("expected unfloat float result, got {other:?}"),
    }
    let row = desire(&reply, "win-a");
    assert!(!row.floating, "unfloat tiles");
    assert_eq!(row.clear_operation, None);
    assert_eq!(row.frame_operation, None);
    assert!(engine.retained_clears.is_empty());
}

#[test]
fn b9_unreadable_hints_refuses_with_bit_retained() {
    let mut engine = engine();
    let _base = float_compose_on(&mut engine);
    let mut unreadable = cfact("win-a", "out-a", 0);
    unreadable.rect = Some(work_area());
    unreadable.hints = None;
    match engine.handle_report(&cmd(5, 3, vec![unreadable], "win-a", toggle_float("win-a"))) {
        WorldOutcome::Accepted(reply) => assert_eq!(
            reply.command_result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::OperationRefused
            })
        ),
        other => panic!("unreadable hints refuses, got {other:?}"),
    }
    assert!(
        engine
            .composed_maximized
            .contains(&WindowId("win-a".to_owned())),
        "hints refusal retains the bit"
    );
}

#[test]
fn b9_held_scope_refusal_retains_cleared_bit() {
    let mut engine = engine();
    let _base = float_compose_on(&mut engine);
    // Unreadable staging domain: the operation envelope has no readable
    // scope, so the intentional-unfloat path refuses before clearing.
    // Direct command seam: `handle_report` discards the staged candidate
    // on refusal, so only `execute_command` observes the bit.
    let mut bad_screen = screen("out-a", 0);
    bad_screen.complete = false;
    bad_screen.full_rect = None;
    bad_screen.work_area = None;
    let mut overlay_fact = cfact("win-a", "out-a", 0);
    overlay_fact.rect = Some(work_area());
    let probe = report_full(
        5,
        5,
        vec![bad_screen],
        vec![overlay_fact],
        Vec::new(),
        "win-a",
        None,
        None,
    );
    engine.stage_facts_v4(&probe);
    engine.build_staged_domains();
    match engine.execute_command(
        &test_issue(&probe),
        3,
        &toggle_float("win-a"),
        &BTreeSet::new(),
        None,
        None,
    ) {
        CommandApplied::Rejected { kind, .. } => assert_eq!(kind, KIND_OPERATION),
        _ => panic!("unreadable scope refuses"),
    }
    assert!(
        engine
            .composed_maximized
            .contains(&WindowId("win-a".to_owned())),
        "scope refusal retains the bit"
    );
}

#[test]
fn composed_tiled_move_unreadable_topology_retains_bit() {
    let mut engine = engine();
    accepted(
        &mut engine,
        &bootstrap(
            1,
            vec![cfact("win-a", "out-a", 0), cfact("win-b", "out-a", 100)],
            "win-a",
        ),
    );
    let mut _alloc = None;
    toggle_on(&mut engine, &mut _alloc);
    assert!(
        engine
            .composed_maximized
            .contains(&WindowId("win-a".to_owned()))
    );
    // Direct command seam: `handle_report` discards the staged candidate
    // on refusal, so only `execute_command` observes the bit.
    let mut bad_screen = screen("out-a", 0);
    bad_screen.complete = false;
    bad_screen.full_rect = None;
    bad_screen.work_area = None;
    let probe = report_full(
        3,
        3,
        vec![bad_screen],
        vec![cfact("win-a", "out-a", 0), cfact("win-b", "out-a", 100)],
        Vec::new(),
        "win-a",
        None,
        None,
    );
    engine.stage_facts_v4(&probe);
    engine.build_staged_domains();
    engine.retain_live_topology();
    match engine.execute_command(
        &test_issue(&probe),
        2,
        &move_dir("win-a", "right"),
        &BTreeSet::new(),
        None,
        None,
    ) {
        CommandApplied::Rejected { kind, .. } => assert_eq!(kind, KIND_OPERATION),
        _ => panic!("unreadable topology refuses"),
    }
    assert!(
        engine
            .composed_maximized
            .contains(&WindowId("win-a".to_owned())),
        "topology refusal retains the bit"
    );
}

// ---- Item3b: send/output-transfer + work-area recompute + authority absence. ----

fn work_area_out_b() -> Rect {
    Rect {
        x: 800,
        y: 0,
        w: 800,
        h: 560,
    }
}

fn send_right(window: &str, follow: bool) -> WorldCommand {
    WorldCommand::SendDirectional {
        window: WindowId(window.to_owned()),
        direction: "right".to_owned(),
        follow,
    }
}

#[test]
fn composed_tiled_send_preserves_bit_and_recomputes_destination() {
    let mut engine = engine();
    let windows = || {
        vec![
            cfact("win-a", "out-a", 0),
            cfact("win-b", "out-a", 100),
            cfact("win-c", "out-b", 900),
        ]
    };
    accepted(&mut engine, &bootstrap_two(1, windows(), "win-a"));
    let toggled = accepted(
        &mut engine,
        &cmd_two(2, 1, windows(), "win-a", toggle("win-a")),
    );
    assert_eq!(maximize_result(&toggled), (true, false, false));
    assert_eq!(desire(&toggled, "win-a").effective_frame, Some(work_area()));
    // Cross-output send retains a flight with the bit held.
    let sent = accepted(
        &mut engine,
        &cmd_two(3, 2, windows(), "win-a", send_right("win-a", false)),
    );
    assert!(
        !sent.transfers.is_empty(),
        "send retains a flight: {sent:?}"
    );
    assert!(
        engine
            .composed_maximized
            .contains(&WindowId("win-a".to_owned())),
        "send preserves the bit"
    );
    // Arrival on the destination output settles the flight and recomputes
    // the destination work-area frame under the retained slot.
    let arrived_windows = vec![
        cfact("win-a", "out-b", 900),
        cfact("win-b", "out-a", 100),
        cfact("win-c", "out-b", 900),
    ];
    let arrived = accepted(
        &mut engine,
        &bootstrap_two(4, arrived_windows.clone(), "win-a"),
    );
    assert!(
        arrived.transfers.is_empty(),
        "arrival settles: {:?}",
        arrived.transfers
    );
    assert!(
        engine
            .composed_maximized
            .contains(&WindowId("win-a".to_owned())),
        "arrival preserves the bit"
    );
    let row = desire(&arrived, "win-a").clone();
    assert_eq!(row.effective_frame, Some(work_area_out_b()));
    assert_eq!(row.frame_authority, ScopeAuthority::Controlled);
    assert!(row.state.is_empty(), "no native maximize target");
    let dest_slot = row.allocation.expect("destination slot");
    // Clear restores the destination base, not the source slot.
    let cleared = accepted(
        &mut engine,
        &cmd_two(5, 3, arrived_windows, "win-a", toggle("win-a")),
    );
    assert_eq!(maximize_result(&cleared), (false, false, true));
    let row = desire(&cleared, "win-a");
    assert_eq!(row.effective_frame, Some(dest_slot));
    assert_eq!(row.allocation, Some(dest_slot));
    assert_eq!(row.frame_authority, ScopeAuthority::Controlled);
    assert!(row.state.is_empty());
}

#[test]
fn composed_float_send_preserves_bit_and_recomputes_destination() {
    let mut engine = engine();
    let windows = || vec![cfact("win-a", "out-a", 0), cfact("win-c", "out-b", 900)];
    accepted(&mut engine, &bootstrap_two(1, windows(), "win-a"));
    let floated = accepted(
        &mut engine,
        &cmd_two(2, 1, windows(), "win-a", toggle_float("win-a")),
    );
    let float_base = match &floated.command_result {
        Some(CommandResult::Float { rect, .. }) => rect.expect("float placement"),
        other => panic!("expected float result, got {other:?}"),
    };
    let mut settled_fact = cfact("win-a", "out-a", 0);
    settled_fact.rect = Some(float_base);
    accepted(
        &mut engine,
        &bootstrap_two(3, vec![settled_fact, cfact("win-c", "out-b", 900)], "win-a"),
    );
    let mut toggle_fact = cfact("win-a", "out-a", 0);
    toggle_fact.rect = Some(float_base);
    let toggled = accepted(
        &mut engine,
        &cmd_two(
            4,
            2,
            vec![toggle_fact.clone(), cfact("win-c", "out-b", 900)],
            "win-a",
            toggle("win-a"),
        ),
    );
    assert_eq!(maximize_result(&toggled), (true, false, false));
    assert_eq!(desire(&toggled, "win-a").effective_frame, Some(work_area()));
    // Cross-output float send retains a flight with the bit held.
    let sent = accepted(
        &mut engine,
        &cmd_two(
            5,
            3,
            vec![toggle_fact, cfact("win-c", "out-b", 900)],
            "win-a",
            send_right("win-a", false),
        ),
    );
    assert!(
        !sent.transfers.is_empty(),
        "send retains a flight: {sent:?}"
    );
    assert!(
        engine
            .composed_maximized
            .contains(&WindowId("win-a".to_owned())),
        "send preserves the bit"
    );
    // Arrival recomputes the destination work-area overlay; the session
    // float base stays retained underneath.
    let mut arrival_fact = cfact("win-a", "out-b", 900);
    arrival_fact.rect = Some(float_base);
    let arrived = accepted(
        &mut engine,
        &bootstrap_two(6, vec![arrival_fact, cfact("win-c", "out-b", 900)], "win-a"),
    );
    assert!(
        arrived.transfers.is_empty(),
        "arrival settles: {:?}",
        arrived.transfers
    );
    assert!(
        engine
            .composed_maximized
            .contains(&WindowId("win-a".to_owned())),
        "arrival preserves the bit"
    );
    let row = desire(&arrived, "win-a").clone();
    assert!(row.floating, "float membership retained");
    assert_eq!(row.effective_frame, Some(work_area_out_b()));
    assert_eq!(row.frame_authority, ScopeAuthority::Controlled);
    // Clear restores the retained float base exactly via the one-shot token.
    let mut clear_fact = cfact("win-a", "out-b", 900);
    clear_fact.rect = Some(work_area_out_b());
    let cleared = accepted(
        &mut engine,
        &cmd_two(
            7,
            4,
            vec![clear_fact, cfact("win-c", "out-b", 900)],
            "win-a",
            toggle("win-a"),
        ),
    );
    assert_eq!(maximize_result(&cleared), (false, false, true));
    let row = desire(&cleared, "win-a");
    assert!(row.floating);
    assert_eq!(row.effective_frame, Some(float_base));
    assert_eq!(
        row.frame_operation,
        Some("op-4".to_owned()),
        "clear mints the one-shot restore token"
    );
    assert_eq!(row.frame_authority, ScopeAuthority::Untouched);
}

#[test]
fn composed_work_area_change_recomputes_tile_in_place() {
    let mut engine = engine();
    accepted(
        &mut engine,
        &bootstrap(
            1,
            vec![cfact("win-a", "out-a", 0), cfact("win-b", "out-a", 100)],
            "win-a",
        ),
    );
    let mut _alloc = None;
    toggle_on(&mut engine, &mut _alloc);
    let changed = Rect {
        x: 0,
        y: 0,
        w: 400,
        h: 300,
    };
    let mut rescreen = bootstrap(
        3,
        vec![cfact("win-a", "out-a", 0), cfact("win-b", "out-a", 100)],
        "win-a",
    );
    rescreen.screens = vec![crate::world_report::ScreenReport {
        work_area: Some(changed),
        ..rescreen.screens[0].clone()
    }];
    let recomputed = accepted(&mut engine, &rescreen);
    assert!(
        engine
            .composed_maximized
            .contains(&WindowId("win-a".to_owned())),
        "work-area change preserves the bit"
    );
    let row = desire(&recomputed, "win-a");
    assert_eq!(row.effective_frame, Some(changed));
    assert_eq!(row.frame_authority, ScopeAuthority::Controlled);
    assert!(row.allocation.is_some(), "slot retained under the new area");
}

#[test]
fn composed_work_area_change_recomputes_float_and_retains_base() {
    let mut engine = engine();
    let float_base = float_compose_on(&mut engine);
    let changed = Rect {
        x: 0,
        y: 0,
        w: 400,
        h: 300,
    };
    let mut rescreen = bootstrap(5, vec![cfact("win-a", "out-a", 0)], "win-a");
    rescreen.screens = vec![crate::world_report::ScreenReport {
        work_area: Some(changed),
        ..rescreen.screens[0].clone()
    }];
    let recomputed = accepted(&mut engine, &rescreen);
    assert!(
        engine
            .composed_maximized
            .contains(&WindowId("win-a".to_owned())),
        "work-area change preserves the bit"
    );
    let row = desire(&recomputed, "win-a").clone();
    assert!(row.floating, "float membership retained");
    assert_eq!(row.effective_frame, Some(changed));
    assert_eq!(row.frame_authority, ScopeAuthority::Controlled);
    // The session float base is untouched by the recompute: clear restores it.
    let mut clear_fact = cfact("win-a", "out-a", 0);
    clear_fact.rect = Some(changed);
    let cleared = accepted(
        &mut engine,
        &cmd(6, 3, vec![clear_fact], "win-a", toggle("win-a")),
    );
    assert_eq!(maximize_result(&cleared), (false, false, true));
    assert_eq!(desire(&cleared, "win-a").effective_frame, Some(float_base));
}

#[test]
fn composed_incomplete_inventory_refuses_without_recovery() {
    let mut engine = engine();
    let base = accepted(
        &mut engine,
        &bootstrap(1, vec![cfact("win-a", "out-a", 0)], "win-a"),
    );
    let base_frame = desire(&base, "win-a").effective_frame;
    // Incomplete inventory withholds frame authority: existing refusal, no
    // overlay intent change.
    let mut gapped = cmd(
        2,
        1,
        vec![cfact("win-a", "out-a", 0)],
        "win-a",
        toggle("win-a"),
    );
    gapped.inventory_complete = false;
    match engine.handle_report(&gapped) {
        WorldOutcome::Accepted(reply) => assert_eq!(
            reply.command_result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::OperationRefused
            })
        ),
        other => panic!("expected refusal, got {other:?}"),
    }
    assert!(
        !engine
            .composed_maximized
            .contains(&WindowId("win-a".to_owned())),
        "refusal mints no overlay intent"
    );
    // No autorecovery: the next commandless report keeps the base frame.
    let quiet = accepted(
        &mut engine,
        &bootstrap(3, vec![cfact("win-a", "out-a", 0)], "win-a"),
    );
    assert_eq!(desire(&quiet, "win-a").effective_frame, base_frame);
    assert!(quiet.command_result.is_none());
    // A fresh press composes once authority returns.
    let toggled = accepted(
        &mut engine,
        &cmd(
            4,
            2,
            vec![cfact("win-a", "out-a", 0)],
            "win-a",
            toggle("win-a"),
        ),
    );
    assert_eq!(maximize_result(&toggled), (true, false, false));
    assert_eq!(desire(&toggled, "win-a").effective_frame, Some(work_area()));
}

#[test]
fn composed_float_unreadable_hints_refuses_toggle_without_intent_change() {
    // Publish-lane parity: the overlay grants `Controlled` over a float
    // base only with readable hints. Max-on and max-off with hints `None`
    // refuse with no intent change and no autorecovery; tiled rows carry
    // no hints requirement and still compose.
    let mut engine = engine();
    accepted(
        &mut engine,
        &bootstrap(1, vec![cfact("win-a", "out-a", 0)], "win-a"),
    );
    let floated = accepted(
        &mut engine,
        &cmd(
            2,
            1,
            vec![cfact("win-a", "out-a", 0)],
            "win-a",
            toggle_float("win-a"),
        ),
    );
    let float_base = match &floated.command_result {
        Some(CommandResult::Float { rect, .. }) => rect.expect("float placement"),
        other => panic!("expected float result, got {other:?}"),
    };
    // Max-on with unreadable hints refuses and mints no intent.
    let mut hintless = cfact("win-a", "out-a", 0);
    hintless.rect = Some(float_base);
    hintless.hints = None;
    match engine.handle_report(&cmd(3, 2, vec![hintless.clone()], "win-a", toggle("win-a"))) {
        WorldOutcome::Accepted(reply) => assert_eq!(
            reply.command_result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::OperationRefused
            })
        ),
        other => panic!("hintless float max-on refuses, got {other:?}"),
    }
    assert!(
        !engine
            .composed_maximized
            .contains(&WindowId("win-a".to_owned())),
        "refusal mints no overlay intent"
    );
    // No autorecovery while hints stay unreadable: the base frame holds.
    let quiet = accepted(&mut engine, &bootstrap(4, vec![hintless], "win-a"));
    assert!(quiet.command_result.is_none());
    assert_eq!(desire(&quiet, "win-a").effective_frame, Some(float_base));
    // A fresh press with readable hints composes.
    let mut settled_fact = cfact("win-a", "out-a", 0);
    settled_fact.rect = Some(float_base);
    accepted(&mut engine, &bootstrap(5, vec![settled_fact], "win-a"));
    let mut toggle_fact = cfact("win-a", "out-a", 0);
    toggle_fact.rect = Some(float_base);
    let toggled = accepted(
        &mut engine,
        &cmd(6, 3, vec![toggle_fact], "win-a", toggle("win-a")),
    );
    assert_eq!(maximize_result(&toggled), (true, false, false));
    // Max-off with unreadable hints refuses with the bit retained.
    let mut hintless_off = cfact("win-a", "out-a", 0);
    hintless_off.rect = Some(work_area());
    hintless_off.hints = None;
    match engine.handle_report(&cmd(7, 4, vec![hintless_off], "win-a", toggle("win-a"))) {
        WorldOutcome::Accepted(reply) => assert_eq!(
            reply.command_result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::OperationRefused
            })
        ),
        other => panic!("hintless float max-off refuses, got {other:?}"),
    }
    assert!(
        engine
            .composed_maximized
            .contains(&WindowId("win-a".to_owned())),
        "refusal retains the bit"
    );
}

#[test]
fn composed_tiled_unreadable_hints_holds_admission() {
    // Ordinary tiled newcomers with hints `None` hold admission upstream
    // (`derived.hold`), so the toggle refuses before any composed gate:
    // hintless tile never composes, hintless float never composes.
    let mut engine = engine();
    let mut hintless = cfact("win-a", "out-a", 0);
    hintless.hints = None;
    accepted(
        &mut engine,
        &bootstrap(
            1,
            vec![hintless.clone(), cfact("win-b", "out-a", 100)],
            "win-a",
        ),
    );
    match engine.handle_report(&cmd(
        2,
        1,
        vec![hintless, cfact("win-b", "out-a", 100)],
        "win-a",
        toggle("win-a"),
    )) {
        WorldOutcome::Accepted(reply) => assert_eq!(
            reply.command_result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::OperationRefused
            })
        ),
        other => panic!("hintless tile refuses, got {other:?}"),
    }
    assert!(
        !engine
            .composed_maximized
            .contains(&WindowId("win-a".to_owned())),
        "refusal mints no overlay intent"
    );
}

#[test]
fn tile_with_bit_toggle_float_refuses_not_tiled() {
    let mut engine = engine();
    accepted(
        &mut engine,
        &bootstrap(
            1,
            vec![cfact("win-a", "out-a", 0), cfact("win-b", "out-a", 100)],
            "win-a",
        ),
    );
    let mut _alloc = None;
    toggle_on(&mut engine, &mut _alloc);
    match engine.handle_report(&cmd(
        3,
        2,
        vec![cfact("win-a", "out-a", 0), cfact("win-b", "out-a", 100)],
        "win-a",
        toggle_float("win-a"),
    )) {
        WorldOutcome::Accepted(reply) => assert_eq!(
            reply.command_result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::OperationRefused
            })
        ),
        other => panic!("tiled subject with the bit refuses not-tiled, got {other:?}"),
    }
    assert!(
        engine
            .composed_maximized
            .contains(&WindowId("win-a".to_owned())),
        "refusal retains the bit"
    );
}

#[test]
fn composed_float_move_constrained_halfsnap_restores_base_with_token() {
    let mut engine = engine();
    let float_base = float_compose_on(&mut engine);
    // Constraining but readable hints: the pre-clear preflight passes, the
    // post-clear halfsnap refuses.
    let mut overlay_fact = cfact("win-a", "out-a", 0);
    overlay_fact.rect = Some(work_area());
    overlay_fact.hints = Some(crate::size_hints::WindowSizeHints {
        min_w: Some(10_000),
        min_h: None,
        max_w: None,
        max_h: None,
    });
    let reply = accepted(
        &mut engine,
        &cmd(
            5,
            3,
            vec![overlay_fact],
            "win-a",
            move_dir("win-a", "right"),
        ),
    );
    assert_eq!(
        directional_outcome(&reply),
        Some("move-float-refused-constraints")
    );
    assert!(
        !engine
            .composed_maximized
            .contains(&WindowId("win-a".to_owned())),
        "native parity: the overlay clears before ordinary policy, no remaximize"
    );
    assert!(engine.retained_clears.is_empty(), "no native clear bound");
    let row = desire(&reply, "win-a");
    assert!(row.state.is_empty(), "no native state target");
    assert_eq!(row.clear_operation, None, "no clear desire published");
    assert_eq!(
        engine.retained_floats.len(),
        1,
        "base restore authorized: {:?}",
        engine.retained_floats
    );
    let held = &engine.retained_floats[0];
    assert_eq!(held.window, WindowId("win-a".to_owned()));
    assert_eq!(held.target, float_base, "restore returns to the base");
    assert_eq!(held.operation_id, "op-3");
    assert_eq!(held.command_seq, 3);
    assert_eq!(
        row.frame_operation.as_deref(),
        Some("op-3"),
        "restore carries the one-shot token"
    );
    assert_eq!(row.effective_frame, Some(float_base));
    assert_eq!(row.frame_authority, ScopeAuthority::Untouched);
}

#[test]
fn composed_float_move_halfsnap_applies_half_token() {
    let mut engine = engine();
    let float_base = float_compose_on(&mut engine);
    let mut overlay_fact = cfact("win-a", "out-a", 0);
    overlay_fact.rect = Some(work_area());
    let reply = accepted(
        &mut engine,
        &cmd(
            5,
            3,
            vec![overlay_fact],
            "win-a",
            move_dir("win-a", "right"),
        ),
    );
    assert_eq!(directional_outcome(&reply), Some("move-float-applied"));
    assert!(
        !engine
            .composed_maximized
            .contains(&WindowId("win-a".to_owned())),
        "ordinary composed float move clears the bit"
    );
    assert!(engine.retained_clears.is_empty(), "no native clear bound");
    let half =
        crate::world_directional::float_half_snap_rect(work_area(), 8, "right").expect("half");
    assert_ne!(half, float_base, "half differs from the base");
    assert_eq!(engine.retained_floats.len(), 1);
    let held = &engine.retained_floats[0];
    assert_eq!(held.target, half, "success carries exactly the half");
    assert_eq!(held.operation_id, "op-3");
    assert_eq!(held.command_seq, 3);
    let row = desire(&reply, "win-a");
    assert_eq!(row.frame_operation.as_deref(), Some("op-3"));
    assert_eq!(row.effective_frame, Some(half));
}

#[test]
fn composed_float_move_unreadable_work_area_retains_bit_without_token() {
    let mut engine = engine();
    let _base = float_compose_on(&mut engine);
    // Work area unreadable: unavailable-authority preflight retains the
    // bit with no token and no deferred clear.
    let mut bad = screen("out-a", 0);
    bad.complete = false;
    bad.full_rect = None;
    bad.work_area = None;
    let mut overlay_fact = cfact("win-a", "out-a", 0);
    overlay_fact.rect = Some(work_area());
    let probe = report_full(
        5,
        5,
        vec![bad],
        vec![overlay_fact.clone()],
        vec![],
        "win-a",
        Some(3),
        Some(move_dir("win-a", "right")),
    );
    let reply = accepted(&mut engine, &probe);
    assert_eq!(
        directional_outcome(&reply),
        Some("move-float-unknown-output")
    );
    assert!(
        engine
            .composed_maximized
            .contains(&WindowId("win-a".to_owned())),
        "unavailable-authority preflight retains the bit"
    );
    assert!(
        engine.retained_floats.is_empty(),
        "no token without authority: {:?}",
        engine.retained_floats
    );
    assert!(engine.retained_clears.is_empty(), "no deferred clear");
    // Commandless recovery retains the bit: no autorecovery, no deferred
    // work.
    let mut observed_fact = cfact("win-a", "out-a", 0);
    observed_fact.rect = Some(work_area());
    let quiet = accepted(&mut engine, &bootstrap(6, vec![observed_fact], "win-a"));
    assert!(quiet.command_result.is_none());
    assert!(
        engine
            .composed_maximized
            .contains(&WindowId("win-a".to_owned())),
        "recovery retains the bit"
    );
    assert!(engine.retained_floats.is_empty());
    assert!(engine.retained_clears.is_empty());
    // A fresh move with readable geometry clears normally.
    let mut fresh_fact = cfact("win-a", "out-a", 0);
    fresh_fact.rect = Some(work_area());
    let reply = accepted(
        &mut engine,
        &cmd(7, 4, vec![fresh_fact], "win-a", move_dir("win-a", "right")),
    );
    assert_eq!(directional_outcome(&reply), Some("move-float-applied"));
    assert!(
        !engine
            .composed_maximized
            .contains(&WindowId("win-a".to_owned())),
        "fresh move clears normally"
    );
}

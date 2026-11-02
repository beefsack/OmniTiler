//! World report shape tests (contract v3): digest stability, validation
//! gates, and typed fixtures through the report constructors.
//!
//! Split from [`crate::world_report`]: submodule of the report module with
//! full access to its internals.

use super::*;
use crate::directional::WorkspaceId;
use crate::geometry::Rect;
use crate::ids::NativeWorkspaceId;

fn logical_ref(output: &str, workspace: &str) -> WorkspaceRef {
    WorkspaceRef::Logical {
        output: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
    }
}

fn native_ref(output: &str, workspace: &str) -> WorkspaceRef {
    WorkspaceRef::Native {
        output: OutputId(output.to_owned()),
        workspace: NativeWorkspaceId::parse(workspace).expect("valid native id"),
    }
}

fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

fn test_report() -> WorldReport {
    WorldReport {
        contract_version: WORLD_CONTRACT_VERSION,
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        observation_seq: 1,
        request_seq: 1,
        request_id: "req-1".to_owned(),
        command_seq: None,
        command: None,
        screens: vec![ScreenReport {
            output: OutputId("out".to_owned()),
            full_rect: Some(rect(0, 0, 800, 600)),
            work_area: Some(rect(0, 0, 800, 560)),
            complete: true,
            scale: 1.0,
            identity: crate::world_report::OutputIdentity::unknown(),
        }],
        windows: vec![WindowFact {
            window: WindowId("win-a".to_owned()),
            output: OutputId("out".to_owned()),
            rect: Some(rect(0, 0, 40, 90)),
            floating: false,
            fit_excluded: false,
            fullscreen: false,
            maximized: false,
            sticky: false,
            fixed_auto: false,
            fixed_suppress: false,
            hints: crate::size_hints::WindowSizeHints::none(),
            minimized: false,
            observed_workspace: None,
            native_workspace: None,
        }],
        views: vec![ScreenView {
            output: OutputId("out".to_owned()),
            view: String::new(),
        }],
        closed: Vec::new(),
        native: Vec::new(),
        cursor: None,
        cursor_hit: None,
        focused_global: String::new(),
        action_foreground: None,
        action_cached: None,
        action_outputs: Vec::new(),
        primary_output: None,
        held_screens: Vec::new(),
        inventory_complete: true,
        gesture_barrier: false,
        gesture: None,
        monotonic_ms: 0,
        frames: Vec::new(),
        member_attempts: Vec::new(),
        view_attempts: Vec::new(),
        clear_attempts: Vec::new(),
        creation_attempts: Vec::new(),
        presentation: crate::world_report::PresentationIngress::disabled(),
    }
}

#[test]
fn digest_covers_every_section() {
    let base = test_report();
    assert!(validate_report(&base));
    let digest = report_digest(&base);
    // Any single-section change alters the digest: duplicates are
    // decided by typed equality, but the digest must never alias two
    // different inputs silently.
    let mut changed = base.clone();
    changed.screens[0].complete = false;
    assert_ne!(report_digest(&changed), digest);
    let mut changed = base.clone();
    changed.screens[0].full_rect = Some(rect(0, 0, 1024, 768));
    assert_ne!(report_digest(&changed), digest);
    let mut changed = base.clone();
    changed.windows[0].rect = Some(rect(1, 0, 40, 90));
    assert_ne!(report_digest(&changed), digest);
    let mut changed = base.clone();
    changed.windows[0].floating = true;
    assert_ne!(report_digest(&changed), digest);
    let mut changed = base.clone();
    changed.windows[0].minimized = true;
    assert_ne!(report_digest(&changed), digest);
    // The capability-gated native membership moves the digest too.
    let capable = base.clone();
    let capable_digest = report_digest(&capable);
    let mut changed = capable;
    changed.windows[0].native_workspace = Some("d1".to_owned());
    assert!(validate_report(&changed));
    assert_ne!(report_digest(&changed), capable_digest);
    let mut changed = base.clone();
    changed.views[0].view = "ws-1".to_owned();
    assert_ne!(report_digest(&changed), digest);
    let mut changed = base.clone();
    changed.closed.push(WindowId("win-b".to_owned()));
    assert_ne!(report_digest(&changed), digest);
    let mut changed = base.clone();
    changed.native.push(NativeObservation {
        output: OutputId("out".to_owned()),
        native_order: vec!["ws".to_owned()],
        current_native: Some("ws".to_owned()),
        occupancy: vec![NativeOccupancy {
            native: "ws".to_owned(),
            count: None,
        }],
    });
    assert_ne!(report_digest(&changed), digest);
    assert_eq!(
        report_digest(&base),
        digest,
        "connection capabilities are not report facts"
    );
    let mut changed = base.clone();
    changed.inventory_complete = false;
    assert_ne!(report_digest(&changed), digest);
    let mut changed = base.clone();
    changed.focused_global = "win-a".to_owned();
    assert_ne!(report_digest(&changed), digest);
    let mut changed = base.clone();
    changed.action_foreground = Some(crate::directional::OutputId("out".to_owned()));
    assert_ne!(report_digest(&changed), digest);
    let mut changed = base.clone();
    changed.action_cached = Some(crate::directional::OutputId("out".to_owned()));
    assert_ne!(report_digest(&changed), digest);
    let mut changed = base.clone();
    changed.action_outputs = vec![crate::directional::OutputId("out".to_owned())];
    assert_ne!(report_digest(&changed), digest);
    let mut changed = base.clone();
    changed.gesture_barrier = true;
    assert_ne!(report_digest(&changed), digest);
    let mut progress = base.clone();
    progress.gesture = Some(crate::gesture::GestureIngress {
        id: 3,
        window: WindowId("win-a".to_owned()),
        kind: crate::gesture::GestureKind::Move,
        cursor: Some((4, 5)),
        start_cursor: Some((1, 2)),
        phase: crate::gesture::GesturePhase::Updated,
        esc: false,
        modal_move: true,
        start_frame: rect(0, 0, 40, 90),
    });
    assert!(validate_report(&progress));
    let progress_digest = report_digest(&progress);
    // The stable gesture id, live cursor, and START cursor each move the
    // digest: replays with a different id or down point never alias.
    let mut changed = progress.clone();
    changed.gesture.as_mut().expect("gesture rides").id = 4;
    assert_ne!(report_digest(&changed), progress_digest);
    let mut changed = progress.clone();
    changed.gesture.as_mut().expect("gesture rides").cursor = Some((6, 5));
    assert_ne!(report_digest(&changed), progress_digest);
    let mut changed = progress.clone();
    changed
        .gesture
        .as_mut()
        .expect("gesture rides")
        .start_cursor = Some((1, 3));
    assert_ne!(report_digest(&changed), progress_digest);
    let mut changed = base.clone();
    changed.monotonic_ms = 1;
    assert_ne!(report_digest(&changed), digest);
    let mut changed = base.clone();
    changed.member_attempts.push(MemberAttempt {
        operation_id: "op-9".to_owned(),
        window: WindowId("win-a".to_owned()),
        target_output: OutputId("out".to_owned()),
        target_workspace: "ws-1".to_owned(),
        setter: FrameSetter::Accepted,
        pre: None,
        post: None,
        attempt_ms: 9,
    });
    assert_ne!(report_digest(&changed), digest);
    // Native attempt evidence separates the digest like any other proof.
    let mut changed = base.clone();
    changed.member_attempts.push(MemberAttempt {
        operation_id: "op-9".to_owned(),
        window: WindowId("win-a".to_owned()),
        target_output: OutputId("out".to_owned()),
        target_workspace: "ws-1".to_owned(),
        setter: FrameSetter::Accepted,
        pre: Some(native_ref("out", "native-1")),
        post: Some(native_ref("out", "native-2")),
        attempt_ms: 9,
    });
    assert_ne!(report_digest(&changed), digest);
    let mut changed = base.clone();
    changed.view_attempts.push(ViewAttempt {
        operation_id: "op-9".to_owned(),
        target_output: OutputId("out".to_owned()),
        target_workspace: "ws-1".to_owned(),
        setter: FrameSetter::Accepted,
        pre: None,
        post: None,
        baseline: None,
        attempt_ms: 9,
    });
    assert_ne!(report_digest(&changed), digest);
    let mut changed = base.clone();
    changed.view_attempts.push(ViewAttempt {
        operation_id: "op-9".to_owned(),
        target_output: OutputId("out".to_owned()),
        target_workspace: "ws-1".to_owned(),
        setter: FrameSetter::Accepted,
        pre: Some(native_ref("out", "native-1")),
        post: Some(native_ref("out", "native-2")),
        baseline: Some(native_ref("out", "native-1")),
        attempt_ms: 9,
    });
    assert_ne!(report_digest(&changed), digest);
    let mut changed = base.clone();
    changed.observation_seq = 2;
    assert_ne!(report_digest(&changed), digest);
    let mut changed = base.clone();
    changed.request_seq = 2;
    assert_ne!(report_digest(&changed), digest);
    // The cursor hit rides beside the cursor: a changed hit alone
    // separates the digest.
    let mut changed = base.clone();
    changed.cursor = Some((4, 5));
    changed.cursor_hit = Some(WindowId("win-a".to_owned()));
    assert_ne!(report_digest(&changed), digest);
}

#[test]
fn validation_gates_shapes() {
    let base = test_report();
    assert!(validate_report(&base));
    // Contract version mismatch.
    let mut bad = base.clone();
    bad.contract_version = 1;
    assert_eq!(validate_report_reason(&bad), Some("version-mismatch"));
    // Unknown screen duplicates.
    let mut bad = base.clone();
    bad.screens.push(bad.screens[0].clone());
    assert_eq!(validate_report_reason(&bad), Some("duplicate-screen"));
    // Fact on an unknown screen.
    let mut bad = base.clone();
    bad.windows[0].output = OutputId("nope".to_owned());
    assert_eq!(validate_report_reason(&bad), Some("window-output-unknown"));
    // Duplicate window.
    let mut bad = base.clone();
    bad.windows.push(bad.windows[0].clone());
    assert_eq!(validate_report_reason(&bad), Some("duplicate-window"));
    // Closed overlapping live.
    let mut bad = base.clone();
    bad.closed.push(WindowId("win-a".to_owned()));
    assert_eq!(validate_report_reason(&bad), Some("closed-overlaps-live"));
    // Duplicate close.
    let mut bad = base.clone();
    bad.closed.push(WindowId("win-b".to_owned()));
    bad.closed.push(WindowId("win-b".to_owned()));
    assert_eq!(validate_report_reason(&bad), Some("duplicate-closed"));
    // View on unknown screen.
    let mut bad = base.clone();
    bad.views.push(ScreenView {
        output: OutputId("nope".to_owned()),
        view: String::new(),
    });
    assert_eq!(validate_report_reason(&bad), Some("view-output-unknown"));
    // Duplicate view.
    let mut bad = base.clone();
    bad.views.push(ScreenView {
        output: OutputId("out".to_owned()),
        view: String::new(),
    });
    assert_eq!(validate_report_reason(&bad), Some("duplicate-view"));
    // Report shape validation is independent of handshake capabilities.
    let mut bad = base.clone();
    bad.native.push(NativeObservation {
        output: OutputId("out".to_owned()),
        native_order: Vec::new(),
        current_native: None,
        occupancy: Vec::new(),
    });
    assert!(validate_report(&bad));
    // Native member evidence is decoded as a fact; Engine ingress gates it.
    let mut bad = base.clone();
    bad.windows[0].native_workspace = Some("d1".to_owned());
    assert!(validate_report(&bad));
    // Native member evidence must be a valid opaque id.
    let mut bad = base.clone();
    bad.windows[0].native_workspace = Some("not an id".to_owned());
    assert_eq!(validate_report_reason(&bad), Some("window-native-invalid"));
    // Native member evidence with the capability validates.
    let mut good = base.clone();
    good.windows[0].native_workspace = Some("d1".to_owned());
    assert!(validate_report(&good));
    // Duplicate native ids within one output refuse.
    let mut bad = good.clone();
    bad.native.push(NativeObservation {
        output: OutputId("out".to_owned()),
        native_order: vec!["d1".to_owned(), "d1".to_owned()],
        current_native: Some("d1".to_owned()),
        occupancy: vec![
            NativeOccupancy {
                native: "d1".to_owned(),
                count: None,
            },
            NativeOccupancy {
                native: "d1".to_owned(),
                count: None,
            },
        ],
    });
    assert_eq!(validate_report_reason(&bad), Some("duplicate-native"));
    // Contradictory native rows for the same output refuse.
    let mut bad = good.clone();
    bad.native.push(NativeObservation {
        output: OutputId("out".to_owned()),
        native_order: vec!["d1".to_owned()],
        current_native: None,
        occupancy: vec![NativeOccupancy {
            native: "d1".to_owned(),
            count: None,
        }],
    });
    bad.native.push(NativeObservation {
        output: OutputId("out".to_owned()),
        native_order: vec!["d2".to_owned()],
        current_native: None,
        occupancy: vec![NativeOccupancy {
            native: "d2".to_owned(),
            count: None,
        }],
    });
    assert_eq!(validate_report_reason(&bad), Some("duplicate-native"));
    // Native on unknown screen refuses.
    let mut bad = good.clone();
    bad.native.push(NativeObservation {
        output: OutputId("out".to_owned()),
        native_order: Vec::new(),
        current_native: None,
        occupancy: Vec::new(),
    });
    bad.native[0].output = OutputId("nope".to_owned());
    assert_eq!(validate_report_reason(&bad), Some("native-output-unknown"));
    // Cursor hit names a reported window and rides with the cursor.
    let mut good = base.clone();
    good.cursor = Some((4, 5));
    good.cursor_hit = Some(WindowId("win-a".to_owned()));
    assert!(validate_report(&good));
    // A hit without the cursor it was resolved from refuses.
    let mut bad = base.clone();
    bad.cursor_hit = Some(WindowId("win-a".to_owned()));
    assert_eq!(
        validate_report_reason(&bad),
        Some("cursor-hit-without-cursor")
    );
    // A hit naming no reported window refuses (stale or foreign id).
    let mut bad = base.clone();
    bad.cursor = Some((4, 5));
    bad.cursor_hit = Some(WindowId("win-ghost".to_owned()));
    assert_eq!(validate_report_reason(&bad), Some("cursor-hit-unknown"));
    // Action-context facts carry opaque ids only; liveness filters at use.
    let mut good = base.clone();
    good.action_foreground = Some(OutputId("out".to_owned()));
    good.action_cached = Some(OutputId("out".to_owned()));
    assert!(validate_report(&good));
    let mut bad = base.clone();
    bad.action_foreground = Some(OutputId(String::new()));
    assert_eq!(validate_report_reason(&bad), Some("action-output-invalid"));
    let mut bad = base.clone();
    bad.action_cached = Some(OutputId(String::new()));
    assert_eq!(validate_report_reason(&bad), Some("action-output-invalid"));
    // Registry entries are opaque unique ids in adapter order.
    let mut good = base.clone();
    good.action_outputs = vec![OutputId("out-a".to_owned()), OutputId("out-b".to_owned())];
    assert!(validate_report(&good));
    let mut bad = base.clone();
    bad.action_outputs = vec![OutputId(String::new())];
    assert_eq!(validate_report_reason(&bad), Some("action-outputs-invalid"));
    let mut bad = base.clone();
    bad.action_outputs = vec![OutputId("out".to_owned()), OutputId("out".to_owned())];
    assert_eq!(
        validate_report_reason(&bad),
        Some("duplicate-action-output")
    );
    // Bad window rect refuses; hints never fail.
    let mut bad = base.clone();
    bad.windows[0].rect = Some(rect(0, 0, 0, 0));
    assert_eq!(validate_report_reason(&bad), Some("window-rect-invalid"));
    let mut good = base.clone();
    good.windows[0].hints.min_w = Some(0);
    good.windows[0].hints.max_w = Some(i32::MAX);
    assert!(validate_report(&good));
    // Unreadable frames ride None without failing.
    let mut good = base.clone();
    good.windows[0].rect = None;
    assert!(validate_report(&good));
    // Gesture ids are nonzero: a zero id can never bind.
    let mut bad = base.clone();
    bad.gesture = Some(crate::gesture::GestureIngress {
        id: 0,
        window: WindowId("win-a".to_owned()),
        kind: crate::gesture::GestureKind::Move,
        cursor: None,
        start_cursor: None,
        phase: crate::gesture::GesturePhase::Updated,
        esc: false,
        modal_move: true,
        start_frame: rect(0, 0, 40, 90),
    });
    assert_eq!(validate_report_reason(&bad), Some("gesture-id-invalid"));
    // Out-of-bound START cursors refuse like live cursors.
    let mut bad = base.clone();
    bad.gesture = Some(crate::gesture::GestureIngress {
        id: 1,
        window: WindowId("win-a".to_owned()),
        kind: crate::gesture::GestureKind::Move,
        cursor: None,
        start_cursor: Some((99999999, 0)),
        phase: crate::gesture::GesturePhase::Updated,
        esc: false,
        modal_move: true,
        start_frame: rect(0, 0, 40, 90),
    });
    assert_eq!(validate_report_reason(&bad), Some("gesture-cursor-invalid"));
    // Member attempt shapes: bad operation, target, and homing ids refuse;
    // unknown pre/post homings ride None; duplicate operation windows
    // refuse.
    let mut bad = base.clone();
    bad.member_attempts.push(MemberAttempt {
        operation_id: String::new(),
        window: WindowId("win-a".to_owned()),
        target_output: OutputId("out".to_owned()),
        target_workspace: "ws-1".to_owned(),
        setter: FrameSetter::Accepted,
        pre: None,
        post: None,
        attempt_ms: 1,
    });
    assert_eq!(validate_report_reason(&bad), Some("member-window-invalid"));
    let mut bad = base.clone();
    bad.member_attempts.push(MemberAttempt {
        operation_id: "op-9".to_owned(),
        window: WindowId("win-a".to_owned()),
        target_output: OutputId("out".to_owned()),
        target_workspace: "ws-1".to_owned(),
        setter: FrameSetter::Accepted,
        pre: Some(WorkspaceRef::Logical {
            output: OutputId(String::new()),
            workspace: WorkspaceId("ws-0".to_owned()),
        }),
        post: None,
        attempt_ms: 1,
    });
    assert_eq!(validate_report_reason(&bad), Some("member-homing-invalid"));
    // Native attempt ids are typed report facts; Engine ingress gates them.
    let mut bad = base.clone();
    bad.member_attempts.push(MemberAttempt {
        operation_id: "op-9".to_owned(),
        window: WindowId("win-a".to_owned()),
        target_output: OutputId("out".to_owned()),
        target_workspace: "ws-1".to_owned(),
        setter: FrameSetter::Accepted,
        pre: Some(native_ref("out", "native-1")),
        post: None,
        attempt_ms: 1,
    });
    assert!(validate_report(&bad));
    let mut capable = base.clone();
    capable.member_attempts.push(MemberAttempt {
        operation_id: "op-9".to_owned(),
        window: WindowId("win-a".to_owned()),
        target_output: OutputId("out".to_owned()),
        target_workspace: "ws-1".to_owned(),
        setter: FrameSetter::Accepted,
        pre: Some(logical_ref("out", "ws-0")),
        post: Some(logical_ref("out", "")),
        attempt_ms: 1,
    });
    assert_eq!(
        validate_report_reason(&capable),
        Some("member-homing-invalid")
    );
    let mut bad = base.clone();
    for _ in 0..2 {
        bad.member_attempts.push(MemberAttempt {
            operation_id: "op-9".to_owned(),
            window: WindowId("win-a".to_owned()),
            target_output: OutputId("out".to_owned()),
            target_workspace: "ws-1".to_owned(),
            setter: FrameSetter::Accepted,
            pre: None,
            post: None,
            attempt_ms: 1,
        });
    }
    assert_eq!(validate_report_reason(&bad), Some("duplicate-member"));
    // View attempt shapes mirror the member rules over exact-output views.
    let mut bad = base.clone();
    bad.view_attempts.push(ViewAttempt {
        operation_id: String::new(),
        target_output: OutputId("out".to_owned()),
        target_workspace: "ws-1".to_owned(),
        setter: FrameSetter::Accepted,
        pre: None,
        post: None,
        baseline: None,
        attempt_ms: 1,
    });
    assert_eq!(validate_report_reason(&bad), Some("view-operation-invalid"));
    let mut bad = base.clone();
    bad.view_attempts.push(ViewAttempt {
        operation_id: "op-9".to_owned(),
        target_output: OutputId("out".to_owned()),
        target_workspace: "ws-1".to_owned(),
        setter: FrameSetter::Accepted,
        pre: None,
        post: Some(native_ref("out", "native-2")),
        baseline: None,
        attempt_ms: 1,
    });
    assert!(validate_report(&bad));
    let mut capable = base.clone();
    capable.view_attempts.push(ViewAttempt {
        operation_id: "op-9".to_owned(),
        target_output: OutputId("out".to_owned()),
        target_workspace: "ws-1".to_owned(),
        setter: FrameSetter::Accepted,
        pre: None,
        post: None,
        baseline: Some(logical_ref("out", "ws-0")),
        attempt_ms: 1,
    });
    assert_eq!(validate_report_reason(&capable), None);
    let mut bad = base.clone();
    for _ in 0..2 {
        bad.view_attempts.push(ViewAttempt {
            operation_id: "op-9".to_owned(),
            target_output: OutputId("out".to_owned()),
            target_workspace: "ws-1".to_owned(),
            setter: FrameSetter::Accepted,
            pre: None,
            post: None,
            baseline: None,
            attempt_ms: 1,
        });
    }
    assert_eq!(validate_report_reason(&bad), Some("duplicate-view-attempt"));
    // Command pair mismatch.
    let mut bad = base.clone();
    bad.command_seq = Some(1);
    assert_eq!(validate_report_reason(&bad), Some("command-pair-invalid"));
    // Over-cap sections.
    let mut bad = base.clone();
    bad.screens.clear();
    let filler = ScreenReport {
        output: OutputId("out".to_owned()),
        full_rect: None,
        work_area: None,
        complete: false,
        scale: 1.0,
        identity: crate::world_report::OutputIdentity::unknown(),
    };
    bad.screens.extend((0..100).map(|_| filler.clone()));
    assert_eq!(validate_report_reason(&bad), Some("section-over-cap"));
}

#[test]
fn command_validation_keeps_closed_vocabularies() {
    use WorldCommand as C;
    let out = OutputId("out".to_owned());
    let win = WindowId("win-a".to_owned());
    assert!(valid_command(&C::Focus {
        window: win.clone(),
        direction: "left".to_owned(),
    }));
    assert!(!valid_command(&C::Focus {
        window: win.clone(),
        direction: "sideways".to_owned(),
    }));
    assert!(valid_command(&C::MigrateWorkspace {
        source_output: out.clone(),
        target_output: OutputId("out-b".to_owned()),
        workspace: "ws-1".to_owned(),
    }));
    assert!(!valid_command(&C::MigrateWorkspace {
        source_output: out.clone(),
        target_output: out.clone(),
        workspace: "ws-1".to_owned(),
    }));
    assert!(valid_command(&C::SwitchView {
        output: out.clone(),
        workspace: "ws-1".to_owned(),
    }));
    assert!(valid_command(&C::SendNumbered {
        window: win.clone(),
        source: SendSource::Explicit(out.clone()),
        index: 0,
        follow: false,
    }));
    assert!(!valid_command(&C::SendNumbered {
        window: win.clone(),
        source: SendSource::Explicit(out.clone()),
        index: 10,
        follow: false,
    }));
    // Implicit sources are always well-formed: the core resolves or
    // refuses them without inventing focus.
    assert!(valid_command(&C::SendNumbered {
        window: win.clone(),
        source: SendSource::HomeThenAction,
        index: 9,
        follow: false,
    }));
    assert!(valid_command(&C::SendRelative {
        window: win.clone(),
        source: SendSource::HomeThenAction,
        delta: -1,
        follow: true,
    }));
    assert!(valid_command(&C::SelectNumbered {
        output: Some(out.clone()),
        index: 9,
    }));
    assert!(!valid_command(&C::SelectNumbered {
        output: Some(out.clone()),
        index: 0,
    }));
    assert!(valid_command(&C::SelectRelative {
        output: Some(out.clone()),
        delta: -1,
    }));
    assert!(!valid_command(&C::SelectRelative {
        output: Some(out.clone()),
        delta: 2,
    }));
    // A direction-only migration carries no source: the core resolves the
    // action source and its current view from the neutral report facts.
    assert!(valid_command(&C::MigrateDirectional {
        direction: "right".to_owned(),
    }));
    assert!(!valid_command(&C::MigrateDirectional {
        direction: "sideways".to_owned(),
    }));
}

#[test]
fn estimator_stays_total_on_nonfinite_typed_facts_before_validation() {
    // Finding A: the request estimator runs before validation, so a
    // malicious typed carrier (non-finite scale, non-finite held number)
    // must bound without panic and without encoding a `null`.
    use crate::connection::RawSettingValue;
    let mut report = test_report();
    report.screens[0].scale = f64::NAN;
    report.held_screens.push(HeldScreenRow {
        output: OutputId("out-held".to_owned()),
        ordinal: 1,
        fields: vec![("scale".to_owned(), RawSettingValue::Number(f64::INFINITY))],
        diagnostic: RowDiagnostic {
            scope: AuthorityScope::Output(OutputId("out-held".to_owned())),
            row: RowDiagnosticLane::Screen,
            id: Some("out-held".to_owned()),
            kind: RowDiagnosticKind::InvalidFact,
        },
    });
    let bound = report_estimate(&report);
    assert!(bound > 0);
    assert_eq!(
        validate_report_reason(&report),
        Some("screen-scale-invalid")
    );
}

#[test]
fn grade_invalid_screens_leaves_valid_reports_untouched() {
    let report = test_report();
    assert!(validate_report(&report));
    assert!(grade_invalid_screens(&report).is_none());
}

#[test]
fn grade_invalid_screens_retains_nonfinite_fact_without_null_spelling() {
    // Finding B: a directly constructed non-finite scale grades to a
    // narrow hold with the original fact retained. The checked canonical
    // writers refuse to spell the retained fact (never a silent `null`).
    let mut report = test_report();
    report.screens[0].scale = f64::NAN;
    assert_eq!(
        validate_report_reason(&report),
        Some("screen-scale-invalid")
    );
    let graded = grade_invalid_screens(&report).expect("grades to hold");
    assert!(graded.screens.is_empty());
    assert_eq!(graded.held_screens.len(), 1);
    let held = &graded.held_screens[0];
    assert_eq!(held.output.0, "out");
    assert_eq!(held.ordinal, 0);
    assert_eq!(held.diagnostic.kind, RowDiagnosticKind::InvalidFact);
    assert_eq!(held.diagnostic.row, RowDiagnosticLane::Screen);
    assert_eq!(held.diagnostic.id.as_deref(), Some("out"));
    let scale = held
        .fields
        .iter()
        .find(|(key, _)| key == "scale")
        .expect("scale fact retained");
    assert!(matches!(scale.1, crate::connection::RawSettingValue::Number(v) if v.is_nan()));
    assert!(encode_held_screen_canonical(held).is_err());
    assert!(encode_screen_canonical(&report.screens[0]).is_err());
    assert!(validate_report(&graded));
}

#[test]
fn grade_invalid_screens_grades_overbound_identity() {
    let mut report = test_report();
    report.screens[0].identity.serial = Some("s".repeat(257));
    assert_eq!(
        validate_report_reason(&report),
        Some("screen-identity-invalid")
    );
    let graded = grade_invalid_screens(&report).expect("grades to hold");
    assert!(graded.screens.is_empty());
    assert_eq!(graded.held_screens.len(), 1);
    assert_eq!(
        graded.held_screens[0].diagnostic.kind,
        RowDiagnosticKind::InvalidFact
    );
    assert!(validate_report(&graded));
}

#[test]
fn grade_invalid_screens_preserves_existing_held_ordinals() {
    // Mixed lane: the pre-existing held row keeps its original ordinal
    // while the newly graded row takes the free carried-array slot.
    use crate::connection::RawSettingValue;
    let mut report = test_report();
    report.screens.push(ScreenReport {
        output: OutputId("out-bad".to_owned()),
        full_rect: None,
        work_area: None,
        complete: false,
        scale: 0.0,
        identity: crate::world_report::OutputIdentity::unknown(),
    });
    report.held_screens.push(HeldScreenRow {
        output: OutputId("out-held".to_owned()),
        ordinal: 1,
        fields: vec![(
            "out".to_owned(),
            RawSettingValue::String("out-held".to_owned()),
        )],
        diagnostic: RowDiagnostic {
            scope: AuthorityScope::Output(OutputId("out-held".to_owned())),
            row: RowDiagnosticLane::Screen,
            id: Some("out-held".to_owned()),
            kind: RowDiagnosticKind::Unreadable,
        },
    });
    let graded = grade_invalid_screens(&report).expect("grades to hold");
    assert_eq!(graded.screens.len(), 1);
    assert_eq!(graded.screens[0].output.0, "out");
    let ordinals: Vec<usize> = graded
        .held_screens
        .iter()
        .map(|held| held.ordinal)
        .collect();
    assert_eq!(ordinals, vec![1, 2]);
    assert_eq!(graded.held_screens[1].output.0, "out-bad");
    assert!(validate_report(&graded));
}

#[test]
fn grade_invalid_screens_refuses_structural_violations() {
    // Structural gates are never graded away: bad ids, duplicates, and
    // bad geometry still refuse even beside a gradable scale.
    let mut bad_id = test_report();
    bad_id.screens[0].output = OutputId(String::new());
    bad_id.screens[0].scale = 0.0;
    assert_eq!(validate_report_reason(&bad_id), Some("screen-id-invalid"));
    assert!(grade_invalid_screens(&bad_id).is_none());

    let mut duplicate = test_report();
    duplicate.screens.push(ScreenReport {
        output: OutputId("out".to_owned()),
        full_rect: None,
        work_area: None,
        complete: false,
        scale: 0.0,
        identity: crate::world_report::OutputIdentity::unknown(),
    });
    duplicate.screens[0].scale = 0.0;
    assert!(grade_invalid_screens(&duplicate).is_none());

    let mut bad_rect = test_report();
    bad_rect.screens[0].full_rect = Some(rect(0, 0, -1, 2));
    bad_rect.screens[0].scale = 0.0;
    assert_eq!(
        validate_report_reason(&bad_rect),
        Some("screen-rect-invalid")
    );
    assert!(grade_invalid_screens(&bad_rect).is_none());
}

#[test]
fn grade_invalid_screens_refuses_structural_violations_hidden_behind_first_scale_failure() {
    // Loophole witness: validation stops at the first scale failure, so a
    // structural violation in a LATER row must still refuse the whole
    // report instead of being erased by removal. First row has valid
    // geometry with scale 0; the second row carries scale 0 PLUS a
    // structural violation (bad geometry, partial rects, or a
    // complete-topology claim without rects).
    fn second_row(
        output: &str,
        full_rect: Option<Rect>,
        work_area: Option<Rect>,
        complete: bool,
    ) -> ScreenReport {
        ScreenReport {
            output: OutputId(output.to_owned()),
            full_rect,
            work_area,
            complete,
            scale: 0.0,
            identity: crate::world_report::OutputIdentity::unknown(),
        }
    }
    let cases: &[(&str, ScreenReport)] = &[
        (
            "invalid-geometry",
            second_row(
                "out-bad",
                Some(rect(0, 0, -1, 2)),
                Some(rect(0, 0, 4, 4)),
                false,
            ),
        ),
        (
            "partial-rects",
            second_row("out-bad", Some(rect(0, 0, 4, 4)), None, false),
        ),
        (
            "complete-without-rects",
            second_row("out-bad", None, None, true),
        ),
    ];
    for (name, second) in cases {
        let mut report = test_report();
        report.screens[0].scale = 0.0;
        report.screens.push(second.clone());
        // The validator exits at the first row's scale failure, hiding the
        // later structural violation: this pins the loophole premise.
        assert_eq!(
            validate_report_reason(&report),
            Some("screen-scale-invalid"),
            "{name}: validator exits early"
        );
        assert!(
            grade_invalid_screens(&report).is_none(),
            "{name}: hidden structural violation must refuse, not grade"
        );
    }
}

#[test]
fn screen_section_cap_counts_valid_plus_held_rows() {
    // Direct core held rows must not bypass the section cap: the bound
    // covers the TOTAL carried screen rows (valid plus held), matching the
    // decoder's carried-array bound.
    fn numbered_screen(n: usize) -> ScreenReport {
        ScreenReport {
            output: OutputId(format!("out-{n:02}")),
            full_rect: Some(rect(0, 0, 800, 600)),
            work_area: Some(rect(0, 0, 800, 560)),
            complete: true,
            scale: 1.0,
            identity: crate::world_report::OutputIdentity::unknown(),
        }
    }
    fn held_row(output: &str, ordinal: usize) -> HeldScreenRow {
        HeldScreenRow {
            output: OutputId(output.to_owned()),
            ordinal,
            fields: vec![(
                "out".to_owned(),
                crate::connection::RawSettingValue::String(output.to_owned()),
            )],
            diagnostic: RowDiagnostic {
                scope: AuthorityScope::Output(OutputId(output.to_owned())),
                row: RowDiagnosticLane::Screen,
                id: Some(output.to_owned()),
                kind: RowDiagnosticKind::Unreadable,
            },
        }
    }
    let mut report = test_report();
    for n in 1..MAX_WORLD_SCREENS {
        report.screens.push(numbered_screen(n));
    }
    assert_eq!(report.screens.len(), MAX_WORLD_SCREENS);
    assert!(validate_report(&report));
    // One held row tips the total over the cap: whole-refusal, no grading.
    report
        .held_screens
        .push(held_row("out-held", MAX_WORLD_SCREENS));
    assert_eq!(validate_report_reason(&report), Some("section-over-cap"));
    assert!(grade_invalid_screens(&report).is_none());
    // Oversized all-held ingress refuses the same way.
    let mut all_held = test_report();
    all_held.screens.clear();
    all_held.windows.clear();
    all_held.views.clear();
    for n in 0..MAX_WORLD_SCREENS + 1 {
        all_held
            .held_screens
            .push(held_row(&format!("out-h{n:02}"), n));
    }
    assert_eq!(validate_report_reason(&all_held), Some("section-over-cap"));
    assert!(grade_invalid_screens(&all_held).is_none());
}

#[test]
fn estimator_does_not_collapse_structure_around_nonfinite_held_scalars() {
    // A held value mixing a large carried string with one non-finite
    // scalar must still count every carried structure/text byte: only the
    // bad scalar itself falls back to the `null` width. Measured as the
    // exact held-row delta so the surrounding report bytes cannot mask an
    // undercount.
    use crate::connection::RawSettingValue;
    let big = "s".repeat(1000);
    let nested = RawSettingValue::Object(vec![
        ("big".to_owned(), RawSettingValue::String(big.clone())),
        ("bad".to_owned(), RawSettingValue::Number(f64::NAN)),
    ]);
    let mut report = test_report();
    let before = report_estimate(&report);
    report.held_screens.push(HeldScreenRow {
        output: OutputId("out-held".to_owned()),
        ordinal: 1,
        fields: vec![("extra".to_owned(), nested)],
        diagnostic: RowDiagnostic {
            scope: AuthorityScope::Output(OutputId("out-held".to_owned())),
            row: RowDiagnosticLane::Screen,
            id: Some("out-held".to_owned()),
            kind: RowDiagnosticKind::InvalidFact,
        },
    });
    let nested_estimate = 2
        + (json_string_len("big") + 1 + json_string_len(&big))
        + 1
        + (json_string_len("bad") + 1 + 4);
    let expected_delta = 2 + (json_string_len("extra") + 1 + nested_estimate);
    assert_eq!(report_estimate(&report) - before, expected_delta);
}

//! World report shape tests (contract v3): digest stability, validation
//! gates, and typed fixtures through the report constructors.
//!
//! Split from [`crate::world_report`]: submodule of the report module with
//! full access to its internals.

use super::*;
use crate::geometry::Rect;

fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

fn test_report() -> WorldReport {
    WorldReport {
        contract_version: WORLD_CONTRACT_VERSION,
        owner: "owner-a".to_owned(),
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        observation_seq: 1,
        request_seq: 1,
        request_id: "req-1".to_owned(),
        command_seq: None,
        command: None,
        workspace_mode: crate::workspace_scope::WorkspaceMode::PerOutputLocal,
        screens: vec![ScreenReport {
            output: OutputId("out".to_owned()),
            full_rect: Some(rect(0, 0, 800, 600)),
            work_area: Some(rect(0, 0, 800, 560)),
            gap: 0,
            outer_gap: 0,
            complete: true,
            primary: true,
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
        host_owned_workspace_domains: false,
        equal_workspace_count: false,
        birth_clear: false,
        native: Vec::new(),
        fixed_size_predicate: crate::size_hints::FixedSizePredicate::BothAxes,
        migration_source_refill: crate::output_selection::MigrationSourceRefill::LastRemaining,
        anchor_fallback: crate::admission::AnchorFallback::default(),
        no_space_result: crate::admission::NoSpaceResult::default(),
        born_maximized: crate::born_maximized::BornMaximized::default(),
        switch_target: crate::cursor_targeting::SwitchTarget::default(),
        fresh_placement: crate::cursor_targeting::FreshPlacement::default(),
        focus_follows_cursor: false,
        cursor_warp: false,
        cursor: None,
        cursor_hit: None,
        default_tiled: true,
        focused_global: String::new(),
        action_foreground: None,
        action_cached: None,
        action_outputs: Vec::new(),
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
    let mut capable = base.clone();
    capable.host_owned_workspace_domains = true;
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
    changed.host_owned_workspace_domains = true;
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
    let mut changed = base.clone();
    changed.equal_workspace_count = true;
    assert_ne!(report_digest(&changed), digest);
    let mut changed = base.clone();
    changed.birth_clear = true;
    assert_ne!(report_digest(&changed), digest);
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
    changed.default_tiled = false;
    assert_ne!(report_digest(&changed), digest);
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
        native_pre: None,
        native_post: None,
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
        pre: None,
        post: None,
        native_pre: Some("native-1".to_owned()),
        native_post: Some("native-2".to_owned()),
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
        native_pre: None,
        native_post: None,
        native_baseline: None,
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
        native_pre: Some("native-1".to_owned()),
        native_post: Some("native-2".to_owned()),
        native_baseline: Some("native-1".to_owned()),
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
    // Native payload without the capability.
    let mut bad = base.clone();
    bad.native.push(NativeObservation {
        output: OutputId("out".to_owned()),
        native_order: Vec::new(),
        current_native: None,
        occupancy: Vec::new(),
    });
    assert_eq!(
        validate_report_reason(&bad),
        Some("native-without-capability")
    );
    // Native member evidence without the capability refuses, even with
    // no native list at all.
    let mut bad = base.clone();
    bad.windows[0].native_workspace = Some("d1".to_owned());
    assert_eq!(
        validate_report_reason(&bad),
        Some("native-without-capability")
    );
    // Native member evidence must be a valid opaque id.
    let mut bad = base.clone();
    bad.host_owned_workspace_domains = true;
    bad.windows[0].native_workspace = Some("not an id".to_owned());
    assert_eq!(validate_report_reason(&bad), Some("window-native-invalid"));
    // Native member evidence with the capability validates.
    let mut good = base.clone();
    good.host_owned_workspace_domains = true;
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
        native_pre: None,
        native_post: None,
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
        pre: Some((OutputId(String::new()), "ws-0".to_owned())),
        post: None,
        native_pre: None,
        native_post: None,
        attempt_ms: 1,
    });
    assert_eq!(validate_report_reason(&bad), Some("member-homing-invalid"));
    // Native attempt ids ride only with the capability, like nativeWs.
    let mut bad = base.clone();
    bad.member_attempts.push(MemberAttempt {
        operation_id: "op-9".to_owned(),
        window: WindowId("win-a".to_owned()),
        target_output: OutputId("out".to_owned()),
        target_workspace: "ws-1".to_owned(),
        setter: FrameSetter::Accepted,
        pre: None,
        post: None,
        native_pre: Some("native-1".to_owned()),
        native_post: None,
        attempt_ms: 1,
    });
    assert_eq!(
        validate_report_reason(&bad),
        Some("native-without-capability")
    );
    let mut capable = base.clone();
    capable.host_owned_workspace_domains = true;
    capable.member_attempts.push(MemberAttempt {
        operation_id: "op-9".to_owned(),
        window: WindowId("win-a".to_owned()),
        target_output: OutputId("out".to_owned()),
        target_workspace: "ws-1".to_owned(),
        setter: FrameSetter::Accepted,
        pre: None,
        post: None,
        native_pre: Some("native-1".to_owned()),
        native_post: Some(String::new()),
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
            native_pre: None,
            native_post: None,
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
        native_pre: None,
        native_post: None,
        native_baseline: None,
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
        post: None,
        baseline: None,
        native_pre: None,
        native_post: Some("native-2".to_owned()),
        native_baseline: None,
        attempt_ms: 1,
    });
    assert_eq!(
        validate_report_reason(&bad),
        Some("native-without-capability")
    );
    let mut capable = base.clone();
    capable.host_owned_workspace_domains = true;
    capable.view_attempts.push(ViewAttempt {
        operation_id: "op-9".to_owned(),
        target_output: OutputId("out".to_owned()),
        target_workspace: "ws-1".to_owned(),
        setter: FrameSetter::Accepted,
        pre: None,
        post: None,
        baseline: None,
        native_pre: None,
        native_post: None,
        native_baseline: Some(String::new()),
        attempt_ms: 1,
    });
    assert_eq!(validate_report_reason(&capable), Some("view-proof-invalid"));
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
            native_pre: None,
            native_post: None,
            native_baseline: None,
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
        gap: 0,
        outer_gap: 0,
        complete: false,
        primary: false,
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
        source: SendSource::HomeThenCachedLive,
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

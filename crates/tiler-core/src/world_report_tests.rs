//! World report shape tests (contract v2): digest stability, validation
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
        restored: None,
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
        }],
        views: vec![ScreenView {
            output: OutputId("out".to_owned()),
            view: String::new(),
        }],
        closed: Vec::new(),
        host_owned_workspace_domains: false,
        equal_workspace_count: false,
        native: Vec::new(),
        fixed_size_predicate: crate::size_hints::FixedSizePredicate::BothAxes,
        migration_source_refill: crate::output_selection::MigrationSourceRefill::LastRemaining,
        anchor_fallback: crate::admission::AnchorFallback::default(),
        no_space_result: crate::admission::NoSpaceResult::default(),
        default_tiled: true,
        focused_global: String::new(),
        inventory_complete: true,
        gesture_barrier: false,
        feedback: vec![AttemptFeedback {
            operation_id: "op-1".to_owned(),
            kind: AttemptKind::Transfer,
            outcome: AttemptOutcome::Unattempted,
            hold: None,
        }],
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
        current_native: "ws".to_owned(),
    });
    assert_ne!(report_digest(&changed), digest);
    let mut changed = base.clone();
    changed.equal_workspace_count = true;
    assert_ne!(report_digest(&changed), digest);
    let mut changed = base.clone();
    changed.inventory_complete = false;
    assert_ne!(report_digest(&changed), digest);
    let mut changed = base.clone();
    changed.focused_global = "win-a".to_owned();
    assert_ne!(report_digest(&changed), digest);
    let mut changed = base.clone();
    changed.gesture_barrier = true;
    assert_ne!(report_digest(&changed), digest);
    let mut changed = base.clone();
    changed.default_tiled = false;
    assert_ne!(report_digest(&changed), digest);
    let mut changed = base.clone();
    changed.feedback[0].outcome = AttemptOutcome::Applied;
    assert_ne!(report_digest(&changed), digest);
    let mut changed = base.clone();
    changed.observation_seq = 2;
    assert_ne!(report_digest(&changed), digest);
    let mut changed = base.clone();
    changed.request_seq = 2;
    assert_ne!(report_digest(&changed), digest);
    // Restoration ingress alters the digest: duplicates compare typed
    // equality, but the digest must never alias restored vs fresh inputs.
    let mut changed = base.clone();
    changed.restored = Some(RestoredWorld {
        rings: vec![RestoredRing {
            output: OutputId("out".to_owned()),
            workspaces: vec!["ws-1".to_owned()],
        }],
        views: Vec::new(),
        tiling: Vec::new(),
        members: Vec::new(),
    });
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
        current_native: String::new(),
    });
    assert_eq!(
        validate_report_reason(&bad),
        Some("native-without-capability")
    );
    // Native payload with the capability validates.
    let mut good = base.clone();
    good.host_owned_workspace_domains = true;
    good.native.push(NativeObservation {
        output: OutputId("out".to_owned()),
        native_order: vec!["ws".to_owned()],
        current_native: "ws".to_owned(),
    });
    assert!(validate_report(&good));
    // Native on unknown screen refuses.
    let mut bad = good.clone();
    bad.native[0].output = OutputId("nope".to_owned());
    assert_eq!(validate_report_reason(&bad), Some("native-output-unknown"));
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
    // Restoration ingress validates when present.
    let mut good = base.clone();
    good.restored = Some(RestoredWorld {
        rings: vec![RestoredRing {
            output: OutputId("out".to_owned()),
            workspaces: vec!["ws-1".to_owned()],
        }],
        views: Vec::new(),
        tiling: Vec::new(),
        members: vec![RestoredMember {
            window: WindowId("win-a".to_owned()),
            output: OutputId("out".to_owned()),
            workspace: "ws-1".to_owned(),
        }],
    });
    assert!(validate_report(&good));
    // Restored members must ride the facts: unknown windows are stale.
    let mut bad = good.clone();
    bad.restored.as_mut().expect("restored").members[0].window = WindowId("win-gone".to_owned());
    assert_eq!(
        validate_report_reason(&bad),
        Some("restored-member-unknown")
    );
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
        output: out.clone(),
        index: 0,
        follow: false,
    }));
    assert!(!valid_command(&C::SendNumbered {
        window: win.clone(),
        output: out.clone(),
        index: 10,
        follow: false,
    }));
    assert!(valid_command(&C::SelectNumbered {
        output: out.clone(),
        index: 9,
    }));
    assert!(!valid_command(&C::SelectNumbered {
        output: out.clone(),
        index: 0,
    }));
    assert!(valid_command(&C::SelectRelative {
        output: out.clone(),
        delta: -1,
    }));
    assert!(!valid_command(&C::SelectRelative {
        output: out.clone(),
        delta: 2,
    }));
}

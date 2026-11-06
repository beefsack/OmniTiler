//! Supported-null applicability narrowing (contract 468-482): a
//! supported-but-null input holds ONLY the dependent predicate. Real-engine
//! coverage through `WorldEngine` (not the predicate enum alone):
//! - A: generic-XAML class + null executable + foreign pid keeps ordinary
//!   tile membership; the border signal fails closed (hidden, shell).
//! - B: recognized float origins (dialog/sheet kind, known modal) keep
//!   float membership when the layer is null and the irrelevant
//!   transient/modal counterpart is null.
//! - Guards: unknown own pid still holds regardless of executable,
//!   unowned `#32770` refusal and utility/splash holds are unchanged.

use tiler_core::directional::{OutputId, WindowId};
use tiler_core::geometry::Rect;
use tiler_core::size_hints::WindowSizeHints;
use tiler_core::visual::BorderHideReason;
use tiler_core::world_engine::{LegacyWorldOutcome, WorldEngine};
use tiler_core::world_report::{
    ActivationPolicy, PresentationIngress, ScopeAuthority, ScreenReport, VisibilityFact,
    WORLD_CONTRACT_VERSION_LEGACY, WindowFact, WindowType, WorldReply, WorldReport,
};

fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

fn screen(output: &str, x: i32) -> ScreenReport {
    ScreenReport {
        output: OutputId(output.to_owned()),
        full_rect: Some(rect(x, 0, 800, 600)),
        work_area: Some(rect(x, 0, 800, 560)),
        complete: true,
        scale: 1.0,
        identity: tiler_core::world_report::OutputIdentity::unknown(),
    }
}

fn healthy(window: &str) -> WindowFact {
    WindowFact {
        window: WindowId(window.to_owned()),
        output: Some(OutputId("out-a".to_owned())),
        rect: Some(rect(0, 0, 400, 560)),
        fullscreen: Some(false),
        maximized: Some(false),
        sticky: Some(false),
        hints: Some(WindowSizeHints::none()),
        minimized: Some(false),
        native_workspace: tiler_core::world_report::NativeWorkspaceFact::Omitted,
        host_hidden: Some(false),
        privileged: Some(false),
        managed: Some(true),
        accepts_input: Some(true),
        activation_inhibited: Some(false),
        tool_window: Some(false),
        owned: Some(false),
        transient: Some(false),
        modal: Some(false),
        resizable: Some(true),
        above: Some(false),
        below: Some(false),
        owner: None,
        parent: None,
        role: None,
        app_id: None,
        executable: Some("c:\\synthetic\\example.exe".to_owned()),
        process_id: Some("pid-foreign".to_owned()),
        class: Some("Synthetic".to_owned()),
        title: None,
        window_type: Some(WindowType::Normal),
        activation_policy: Some(ActivationPolicy::Regular),
        layer: Some(0),
        fullscreen_owner: None,
        fullscreen_exitable: None,
        suppression_available: None,
        visibility: VisibilityFact {
            visible: Some(true),
            suppression_owned: None,
        },
        last_write: None,
    }
}

fn report(
    request_seq: u64,
    observation_seq: u64,
    windows: Vec<WindowFact>,
    presentation: PresentationIngress,
) -> WorldReport {
    WorldReport {
        contract_version: WORLD_CONTRACT_VERSION_LEGACY,
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        observation_seq,
        request_seq,
        request_id: format!("req-{request_seq}"),
        command_seq: None,
        command: None,
        screens: vec![screen("out-a", 0)],
        windows,
        views: vec![],
        closed: vec![],
        native: vec![],
        focused_global: String::new(),
        action_foreground: None,
        action_cached: None,
        action_outputs: Vec::new(),
        primary_output: None,
        held_screens: Vec::new(),
        held_windows: Vec::new(),
        inventory_complete: true,
        gesture_barrier: false,
        gesture: None,
        cursor: None,
        cursor_hit: None,
        monotonic_ms: 0,
        frames: Vec::new(),
        member_attempts: Vec::new(),
        view_attempts: Vec::new(),
        clear_attempts: Vec::new(),
        creation_attempts: Vec::new(),
        state_attempts: Vec::new(),
        focus_feedback: Vec::new(),
        presentation,

        intent_records: Vec::new(),
    }
}

fn accepted(engine: &mut WorldEngine, report: &WorldReport) -> WorldReply {
    match engine.handle_report_legacy(report) {
        LegacyWorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    }
}

fn row_of<'a>(reply: &'a WorldReply, window: &str) -> &'a tiler_core::world_report::WindowDesire {
    reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == window)
        .unwrap_or_else(|| panic!("missing row {window}"))
}

fn foreground_presentation(window: &str) -> PresentationIngress {
    PresentationIngress {
        foreground: window.to_owned(),
        foreground_frame: Some(rect(0, 0, 400, 560)),
        border_enabled: true,
        underlay_enabled: false,
        border_gap_px: 0,
        border_width_px: 4,
        underlay_extension_px: 0,
        chord_held: false,
        ..PresentationIngress::disabled()
    }
}

#[test]
fn xaml_unknown_executable_tiles_with_hidden_border() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let mut target = healthy("win-xaml");
    target.class = Some("Windows.UI.Core.CoreWindow".to_owned());
    target.executable = None;
    target.process_id = Some("pid-foreign-xaml".to_owned());
    let sibling = healthy("win-control");
    let reply = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![target.clone(), sibling],
            PresentationIngress::disabled(),
        ),
    );
    // Ordinary foreign XAML with unreadable executable keeps tile
    // membership next to the healthy same-output sibling.
    for win in ["win-xaml", "win-control"] {
        let row = row_of(&reply, win);
        assert_eq!(
            row.membership_authority,
            ScopeAuthority::Controlled,
            "{win}"
        );
        assert!(!row.floating, "{win}");
        assert!(row.allocation.is_some(), "{win}: tiled, got {row:?}");
    }
    // The undecidable shell-host match fails the border closed (hidden
    // with the shell reason) instead of holding the member/frame.
    let hidden = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![target, healthy("win-control")],
            foreground_presentation("win-xaml"),
        ),
    );
    assert!(!hidden.presentation.border.visible);
    assert_eq!(
        hidden.presentation.border.reason,
        Some(BorderHideReason::Shell)
    );
    // A known foreign executable on the same class shows the border.
    let mut known = healthy("win-xaml-known");
    known.class = Some("Windows.UI.Core.CoreWindow".to_owned());
    known.executable = Some("c:\\synthetic\\example.exe".to_owned());
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let reply = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![known, healthy("win-control")],
            PresentationIngress::disabled(),
        ),
    );
    assert!(row_of(&reply, "win-xaml-known").allocation.is_some());
    let shown = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![
                {
                    let mut fact = healthy("win-xaml-known");
                    fact.class = Some("Windows.UI.Core.CoreWindow".to_owned());
                    fact.executable = Some("c:\\synthetic\\example.exe".to_owned());
                    fact
                },
                healthy("win-control"),
            ],
            foreground_presentation("win-xaml-known"),
        ),
    );
    assert!(shown.presentation.border.visible);
    assert_eq!(shown.presentation.border.reason, None);
    // A known shell host executable keeps the existing hidden border.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let mut shell = healthy("win-shell");
    shell.class = Some("Windows.UI.Core.CoreWindow".to_owned());
    shell.executable = Some("c:\\synthetic\\searchhost.exe".to_owned());
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![shell, healthy("win-control")],
            PresentationIngress::disabled(),
        ),
    );
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![
                {
                    let mut fact = healthy("win-shell");
                    fact.class = Some("Windows.UI.Core.CoreWindow".to_owned());
                    fact.executable = Some("c:\\synthetic\\searchhost.exe".to_owned());
                    fact
                },
                healthy("win-control"),
            ],
            foreground_presentation("win-shell"),
        ),
    );
    assert!(!reply.presentation.border.visible);
    assert_eq!(
        reply.presentation.border.reason,
        Some(BorderHideReason::Shell)
    );
}

#[test]
fn float_origin_survives_unknown_layer_and_irrelevant_transient() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    // Dialog kind proves float: null transient and null layer are
    // irrelevant to the float verdict.
    let mut dialog = healthy("win-dialog");
    dialog.window_type = Some(WindowType::Dialog);
    dialog.transient = None;
    dialog.layer = None;
    // Known modal proves float: null transient and null layer are
    // irrelevant to the float verdict.
    let mut modal = healthy("win-modal");
    modal.modal = Some(true);
    modal.transient = None;
    modal.layer = None;
    let reply = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![dialog, modal, healthy("win-control")],
            PresentationIngress::disabled(),
        ),
    );
    for win in ["win-dialog", "win-modal"] {
        let row = row_of(&reply, win);
        assert!(row.floating, "{win}: stays a float member, got {row:?}");
        assert_eq!(row.allocation, None, "{win}: no tile geometry");
    }
    let control = row_of(&reply, "win-control");
    assert_eq!(control.membership_authority, ScopeAuthority::Controlled);
    assert!(!control.floating);
    assert!(control.allocation.is_some());
}

#[test]
fn unknown_own_pid_holds_and_dialog_refusal_unchanged() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    // Supported-but-unreadable process identity holds regardless of the
    // executable state (here also unreadable on a generic-XAML class).
    let mut unknown_own = healthy("win-unknown-own");
    unknown_own.class = Some("Windows.UI.Core.CoreWindow".to_owned());
    unknown_own.executable = None;
    unknown_own.process_id = None;
    // Known-unowned generic dialog still refuses membership.
    let mut dialog = healthy("win-32770");
    dialog.class = Some("#32770".to_owned());
    dialog.window_type = Some(WindowType::Dialog);
    // Utility/splash kinds stay in the TBD hold.
    let mut utility = healthy("win-utility");
    utility.window_type = Some(WindowType::Utility);
    let reply = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![unknown_own, dialog, utility, healthy("win-control")],
            PresentationIngress::disabled(),
        ),
    );
    // Held/excluded newcomers publish no controlled membership and no
    // managed geometry (a retained member would publish a Hold row; a
    // newcomer has no home to retain).
    assert!(
        reply.diagnostics.is_empty(),
        "no refusal: {:?}",
        reply.diagnostics
    );
    for win in ["win-unknown-own", "win-32770", "win-utility"] {
        for row in reply.windows.iter().filter(|row| row.lifetime.0 == win) {
            assert_ne!(
                row.membership_authority,
                ScopeAuthority::Controlled,
                "{win} never controlled, got {row:?}"
            );
            assert_eq!(row.allocation, None, "{win}: no geometry, got {row:?}");
        }
    }
    let control = row_of(&reply, "win-control");
    assert_eq!(control.membership_authority, ScopeAuthority::Controlled);
    assert!(control.allocation.is_some());
}

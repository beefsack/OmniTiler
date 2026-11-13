//! Supported-null applicability narrowing (contract 468-482): a
//! supported-but-null input holds ONLY the dependent predicate. Real-engine
//! coverage through `WorldEngine` (not the predicate enum alone):
//! - A: generic-XAML class + null executable + foreign pid keeps ordinary
//!   tile membership; the border signal fails closed (hidden, shell).
//! - B: recognized float origins (dialog/sheet kind, known modal) keep
//!   float membership when the layer is null and the irrelevant
//!   transient/modal counterpart is null.
//! - Guards: unknown own pid still holds regardless of executable,
//!   unowned `#32770` refusal and remaining TBD nonordinary kind holds are
//!   unchanged; utility/splash float as ordinary members with safety gates
//!   retained; PiP title/class alone tiles while ordinary float facts float;
//!   already-tiled infeasible min-hint raises and geometry drift retain
//!   authoritative allocations.

use tiler_core::directional::{OutputId, WindowId};
use tiler_core::geometry::Rect;
use tiler_core::size_hints::WindowSizeHints;
use tiler_core::world_engine::{WorldEngine, WorldOutcome};
use tiler_core::world_report::{
    ActivationPolicy, BorderHideReasonV4, PresentationIngressV4, PresentationRowV4, ScopeAuthority,
    ScreenReport, VisibilityFact, WORLD_CONTRACT_VERSION_V4, WindowFact, WindowType, WorldReplyV4,
    WorldReportV4,
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
    presentation: PresentationIngressV4,
) -> WorldReportV4 {
    WorldReportV4 {
        contract_version: WORLD_CONTRACT_VERSION_V4,
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        observation_seq,
        request_seq,
        request_id: format!("req-{request_seq}"),
        command_seq: None,
        command: None,
        screens: vec![screen("out-a", 0)],
        held_screens: Vec::new(),
        windows,
        held_windows: Vec::new(),
        closed: vec![],
        native: vec![],
        held_native: Vec::new(),
        focused_global: String::new(),
        foreground_known: false,
        action_foreground: None,
        action_cached: None,
        action_outputs: Vec::new(),
        primary_output: None,
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
        held_attempts: Vec::new(),
        state_attempts: Vec::new(),
        focus_feedback: Vec::new(),
        presentation: PresentationRowV4::Valid(presentation),
        intent_records: Vec::new(),
        renderer_input: None,
    }
}

fn accepted(engine: &mut WorldEngine, report: &WorldReportV4) -> WorldReplyV4 {
    match engine.handle_report(report, &[]).0 {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    }
}

fn row_of<'a>(reply: &'a WorldReplyV4, window: &str) -> &'a tiler_core::world_report::WindowDesire {
    reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == window)
        .unwrap_or_else(|| panic!("missing row {window}"))
}

fn foreground_presentation(window: &str) -> PresentationIngressV4 {
    // v4 ingress carries only the foreground lanes; style sizes live in
    // composed settings (contract 261-272).
    PresentationIngressV4 {
        foreground: window.to_owned(),
        foreground_minimized: Some(false),
        foreground_maximized: Some(false),
        foreground_fullscreen: Some(false),
        foreground_host_hidden: Some(false),
        foreground_hidden_workspace: Some(false),
        foreground_frame: Some(rect(0, 0, 400, 560)),
        chord_held: false,
        ..PresentationIngressV4::default()
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
            PresentationIngressV4::default(),
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
        Some(BorderHideReasonV4::Shell)
    );
    // A known foreign executable on the same class still hides without a
    // joined witness (R20 anchored border; eligible but unjoined). Visible
    // rows port via settings+witness in
    // world_presentation_v4_tests::genuine_descriptor_positive.
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
            PresentationIngressV4::default(),
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
    assert!(!shown.presentation.border.visible);
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
            PresentationIngressV4::default(),
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
        Some(BorderHideReasonV4::Shell)
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
            PresentationIngressV4::default(),
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
    // Remaining nonordinary kinds (e.g. menu) stay in the TBD hold.
    let mut menu = healthy("win-menu");
    menu.window_type = Some(WindowType::Menu);
    let reply = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![unknown_own, dialog, menu, healthy("win-control")],
            PresentationIngressV4::default(),
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
    for win in ["win-unknown-own", "win-32770", "win-menu"] {
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

#[test]
fn utility_and_splash_float_as_ordinary_members_including_nullable_unrelated_facts() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let mut utility = healthy("win-utility");
    utility.window_type = Some(WindowType::Utility);
    utility.transient = None;
    utility.modal = None;
    utility.layer = None;
    utility.hints = None;
    utility.executable = None;
    utility.title = None;
    utility.role = None;

    let mut splash = healthy("win-splash");
    splash.window_type = Some(WindowType::Splash);
    splash.transient = None;
    splash.modal = None;
    splash.layer = None;
    splash.hints = None;
    splash.executable = None;
    splash.title = None;
    splash.role = None;

    let reply = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![utility, splash, healthy("win-control")],
            PresentationIngressV4::default(),
        ),
    );
    for win in ["win-utility", "win-splash"] {
        let row = row_of(&reply, win);
        assert_eq!(
            row.membership_authority,
            ScopeAuthority::Untouched,
            "{win}: ordinary float member keeps untouched membership authority, got {row:?}"
        );
        assert_eq!(
            row.frame_authority,
            ScopeAuthority::Untouched,
            "{win}: ordinary float member keeps untouched frame authority, got {row:?}"
        );
        assert!(row.floating, "{win}: must float, got {row:?}");
        assert_eq!(
            row.float_origin, None,
            "{win}: ordinary float, not fixed-auto"
        );
        assert_eq!(row.allocation, None, "{win}: no tile geometry");
    }
    let control = row_of(&reply, "win-control");
    assert_eq!(control.membership_authority, ScopeAuthority::Controlled);
    assert!(!control.floating);
    assert!(control.allocation.is_some());
}

#[test]
fn utility_and_splash_retain_safety_owned_and_tool_window_gates() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let mut owned_utility = healthy("win-owned-utility");
    owned_utility.window_type = Some(WindowType::Utility);
    owned_utility.owned = Some(true);

    let mut tool_splash = healthy("win-tool-splash");
    tool_splash.window_type = Some(WindowType::Splash);
    tool_splash.tool_window = Some(true);

    let mut priv_utility = healthy("win-priv-utility");
    priv_utility.window_type = Some(WindowType::Utility);
    priv_utility.privileged = Some(true);

    let mut unmanaged_splash = healthy("win-unmanaged-splash");
    unmanaged_splash.window_type = Some(WindowType::Splash);
    unmanaged_splash.managed = Some(false);

    let mut null_owned_utility = healthy("win-null-owned-utility");
    null_owned_utility.window_type = Some(WindowType::Utility);
    null_owned_utility.owned = None;

    let reply = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![
                owned_utility,
                tool_splash,
                priv_utility,
                unmanaged_splash,
                null_owned_utility,
                healthy("win-control"),
            ],
            PresentationIngressV4::default(),
        ),
    );
    for win in [
        "win-owned-utility",
        "win-tool-splash",
        "win-priv-utility",
        "win-unmanaged-splash",
        "win-null-owned-utility",
    ] {
        for row in reply.windows.iter().filter(|row| row.lifetime.0 == win) {
            assert_ne!(
                row.membership_authority,
                ScopeAuthority::Controlled,
                "{win} must not gain controlled membership, got {row:?}"
            );
            assert!(!row.floating, "{win} must not float, got {row:?}");
            assert_eq!(row.allocation, None, "{win}: no geometry, got {row:?}");
        }
    }
    let control = row_of(&reply, "win-control");
    assert_eq!(control.membership_authority, ScopeAuthority::Controlled);
    assert!(control.allocation.is_some());
}

#[test]
fn pip_title_and_class_alone_tile_while_ordinary_float_facts_float() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    // PiP-like title, class, and keepAbove alone on a Normal window do not float.
    let mut pip_normal = healthy("win-pip-normal");
    pip_normal.title = Some("Picture-in-Picture".to_owned());
    pip_normal.class = Some("PictureInPicture".to_owned());
    pip_normal.above = Some(true);

    // Same PiP metadata with a positive ordinary float fact (transient, modal,
    // or utility kind) floats via the ordinary fact.
    let mut pip_transient = healthy("win-pip-transient");
    pip_transient.title = Some("Picture-in-Picture".to_owned());
    pip_transient.class = Some("PictureInPicture".to_owned());
    pip_transient.transient = Some(true);

    let mut pip_utility = healthy("win-pip-utility");
    pip_utility.title = Some("Picture-in-Picture".to_owned());
    pip_utility.class = Some("PictureInPicture".to_owned());
    pip_utility.window_type = Some(WindowType::Utility);

    let reply = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![pip_normal, pip_transient, pip_utility],
            PresentationIngressV4::default(),
        ),
    );
    let normal_row = row_of(&reply, "win-pip-normal");
    assert_eq!(normal_row.membership_authority, ScopeAuthority::Controlled);
    assert!(!normal_row.floating, "PiP metadata alone must not float");
    assert!(normal_row.allocation.is_some());

    for win in ["win-pip-transient", "win-pip-utility"] {
        let row = row_of(&reply, win);
        assert_eq!(
            row.membership_authority,
            ScopeAuthority::Untouched,
            "{win}: ordinary float member keeps untouched membership authority, got {row:?}"
        );
        assert_eq!(
            row.frame_authority,
            ScopeAuthority::Untouched,
            "{win}: ordinary float member keeps untouched frame authority, got {row:?}"
        );
        assert!(row.floating, "{win} must float via ordinary float facts");
        assert_eq!(row.float_origin, None);
        assert_eq!(row.allocation, None);
    }
}

#[test]
fn already_tiled_infeasible_min_raise_and_geometry_drift_retain_allocations() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let r1 = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![healthy("win-a"), healthy("win-b")],
            PresentationIngressV4::default(),
        ),
    );
    let alloc_a1 = row_of(&r1, "win-a").allocation.expect("win-a tiled");
    let alloc_b1 = row_of(&r1, "win-b").allocation.expect("win-b tiled");

    // Runtime infeasible non-fixed minimum-hint raise + app resize geometry
    // drift on win-b: retains authoritative allocations with no eviction, no
    // reflow, and no float.
    let mut raised_b = healthy("win-b");
    raised_b.hints = Some(WindowSizeHints {
        min_w: Some(900),
        min_h: Some(700),
        max_w: None,
        max_h: None,
    });
    raised_b.rect = Some(rect(0, 0, 900, 700));
    let r2 = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![healthy("win-a"), raised_b],
            PresentationIngressV4::default(),
        ),
    );
    let row_a2 = row_of(&r2, "win-a");
    let row_b2 = row_of(&r2, "win-b");
    assert_eq!(row_a2.membership_authority, ScopeAuthority::Controlled);
    assert_eq!(row_b2.membership_authority, ScopeAuthority::Controlled);
    assert!(!row_a2.floating);
    assert!(!row_b2.floating);
    assert_eq!(row_a2.allocation, Some(alloc_a1));
    assert_eq!(row_b2.allocation, Some(alloc_b1));

    // Prior fixed-size admission decision for already-tiled win-b stays
    // unchanged even if win-b later reports equal min/max hints.
    let mut fixed_b = healthy("win-b");
    fixed_b.hints = Some(WindowSizeHints {
        min_w: Some(400),
        min_h: Some(400),
        max_w: Some(400),
        max_h: Some(400),
    });
    let r3 = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![healthy("win-a"), fixed_b],
            PresentationIngressV4::default(),
        ),
    );
    let row_b3 = row_of(&r3, "win-b");
    assert_eq!(row_b3.membership_authority, ScopeAuthority::Controlled);
    assert!(!row_b3.floating, "retained tiled stays tiled");
    assert_eq!(row_b3.float_origin, None, "no fixed-size float admission");
    assert!(row_b3.allocation.is_some(), "win-b remains tiled");
}

//! Core-internal v4 presentation producer regressions: genuine descriptors
//! from staged v4 facts, fail-closed hides, scale-once styling with the
//! existing style cap, output-local suspension, and retained pager sources.

use crate::connection::{FactName, HostCapabilities, RawSettingValue};
use crate::directional::{OutputId, WindowId};
use crate::geometry::Rect;
use crate::ids::{CorrelationId, OwnerId};
use crate::world_engine::{EventIssue, WorldEngine};
use crate::world_presentation::{GroupHideReason, PresentationUnderlay};
use crate::world_report::{
    BorderHideReasonV4, PresentationIngressV4, PresentationPagerFrameV4, PresentationRowV4,
    RowDiagnosticKind, RowDiagnosticLane, ScreenReport, WORLD_CONTRACT_VERSION_V4, WindowFact,
    WorldReplyV4, WorldReportV4,
};
use crate::world_run_v4::V4RunOutcome;
use crate::world_v4_support_tests::{fact, report_full as report_full_v4, screen};

fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

fn caps_full() -> HostCapabilities {
    HostCapabilities {
        maximize: true,
        fullscreen: true,
        native_sticky: true,
        above: true,
        window_suppression: true,
        ..HostCapabilities::default()
    }
}

fn caps_composed() -> HostCapabilities {
    HostCapabilities {
        maximize: false,
        ..caps_full()
    }
}

fn engine_with(settings: Vec<(String, RawSettingValue)>) -> WorldEngine {
    WorldEngine::new_test_bound_with_initial_settings("gen-1", "adapter-1", caps_full(), settings)
        .expect("valid generation")
}

fn engine_with_caps(
    caps: HostCapabilities,
    settings: Vec<(String, RawSettingValue)>,
) -> WorldEngine {
    WorldEngine::new_test_bound_with_initial_settings("gen-1", "adapter-1", caps, settings)
        .expect("valid generation")
}

fn witness(fg: &str, frame: Rect) -> crate::world_report::FrameWitness {
    crate::world_report::FrameWitness {
        win: WindowId(fg.to_owned()),
        obs: 3,
        frame,
        foreground: WindowId(fg.to_owned()),
    }
}

fn ingress_v4(fg: &str) -> PresentationIngressV4 {
    let frame = if fg.is_empty() {
        None
    } else {
        Some(rect(0, 0, 40, 90))
    };
    PresentationIngressV4 {
        foreground: fg.to_owned(),
        foreground_minimized: Some(false),
        foreground_maximized: Some(false),
        foreground_fullscreen: Some(false),
        foreground_host_hidden: Some(false),
        foreground_hidden_workspace: Some(false),
        foreground_frame: frame,
        chord_held: false,
        move_window: String::new(),
        pager_order: Vec::new(),
        pager_frames: Vec::new(),
        witness: frame.map(|_| witness(fg, rect(0, 0, 40, 90))),
        foreground_out: if fg.is_empty() {
            None
        } else {
            Some(OutputId("out-a".to_owned()))
        },
    }
}

fn report_v4(
    seq: u64,
    screens: Vec<ScreenReport>,
    windows: Vec<WindowFact>,
    focused: &str,
    ingress: PresentationIngressV4,
) -> WorldReportV4 {
    WorldReportV4 {
        contract_version: WORLD_CONTRACT_VERSION_V4,
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        observation_seq: seq,
        request_seq: seq,
        request_id: format!("req-{seq}"),
        command_seq: None,
        command: None,
        screens,
        held_screens: Vec::new(),
        windows,
        held_windows: Vec::new(),
        closed: Vec::new(),
        native: Vec::new(),
        held_native: Vec::new(),
        cursor: None,
        cursor_hit: None,
        focused_global: focused.to_owned(),
        action_foreground: None,
        action_cached: None,
        action_outputs: Vec::new(),
        primary_output: None,
        inventory_complete: true,
        gesture_barrier: false,
        gesture: None,
        monotonic_ms: 0,
        frames: Vec::new(),
        member_attempts: Vec::new(),
        view_attempts: Vec::new(),
        clear_attempts: Vec::new(),
        creation_attempts: Vec::new(),
        held_attempts: Vec::new(),
        state_attempts: Vec::new(),
        focus_feedback: Vec::new(),
        presentation: PresentationRowV4::Valid(ingress),
        intent_records: Vec::new(),
        renderer_input: None,
    }
}

fn facts_pair() -> Vec<WindowFact> {
    vec![
        fact("win-a", "out-a", 0),
        fact("win-b", "out-a", 100),
        fact("win-c", "out-b", 800),
    ]
}

fn screens_pair() -> Vec<ScreenReport> {
    vec![screen("out-a", 0), screen("out-b", 800)]
}

fn seeded(
    engine: &mut WorldEngine,
    base: u64,
    screens: Vec<ScreenReport>,
    windows: Vec<WindowFact>,
    focused: &str,
    ingress: PresentationIngressV4,
) -> String {
    let first = accepted_v4(
        engine,
        &report_full_v4(
            base,
            base,
            screens.clone(),
            windows.clone(),
            Vec::new(),
            focused,
            None,
            None,
        ),
    );
    let view_a = first
        .outputs
        .iter()
        .find(|row| row.output.0 == "out-a")
        .and_then(|row| row.logical_view.clone())
        .expect("view a");
    accepted_v4(
        engine,
        &report_full_v4(
            base + 1,
            base + 1,
            screens.clone(),
            windows.clone(),
            Vec::new(),
            focused,
            None,
            None,
        ),
    );
    let staged = report_v4(base + 2, screens, windows, focused, ingress);
    engine.stage_facts_v4(&staged);
    engine.retain_pager_presentation_v4();
    view_a
}

fn stage_v4(engine: &mut WorldEngine, report: &WorldReportV4) {
    engine.stage_facts_v4(report);
    engine.retain_pager_presentation_v4();
}

fn staged_ingress(engine: &WorldEngine) -> PresentationIngressV4 {
    match &engine.staged.presentation_v4 {
        Some(PresentationRowV4::Valid(ingress)) => ingress.clone(),
        other => panic!("expected valid staged ingress, got {other:?}"),
    }
}

#[test]
fn genuine_descriptor_positive() {
    let mut engine = engine_with(vec![
        ("borderWidth".to_owned(), RawSettingValue::Number(4.0)),
        ("borderGap".to_owned(), RawSettingValue::Number(2.0)),
        ("borderRadius".to_owned(), RawSettingValue::Number(6.0)),
    ]);
    seeded(
        &mut engine,
        1,
        screens_pair(),
        facts_pair(),
        "win-a",
        ingress_v4("win-a"),
    );
    let ingress = staged_ingress(&engine);
    let border = engine.presentation_border_v4(&ingress, None);
    assert!(border.visible);
    assert_eq!(border.reason, None);
    assert_eq!(border.subject, Some(WindowId("win-a".to_owned())));
    assert_eq!(border.outsets, Some([6, 6, 6, 6]));
    assert_eq!(border.width, Some(4));
    assert_eq!(border.radius, Some(6));
    assert_eq!(border.witness, ingress.witness);
    let published = engine.presentation_for_publish_v4();
    assert_eq!(published.foreground, "win-a");
    assert!(published.border.visible);
    assert_eq!(published.border.reason, None);
    assert_eq!(published.border.subject, Some(WindowId("win-a".to_owned())));
    assert_eq!(published.border.outsets, Some([6, 6, 6, 6]));
    assert_eq!(published.border.width, Some(4));
    assert_eq!(published.border.radius, Some(6));
    assert_eq!(published.border.witness, ingress.witness);
}

#[test]
fn scale_once_fractional_and_boundary() {
    let mut engine = engine_with(vec![
        ("borderWidth".to_owned(), RawSettingValue::Number(3.0)),
        ("borderGap".to_owned(), RawSettingValue::Number(2.0)),
        ("borderRadius".to_owned(), RawSettingValue::Number(1.0)),
    ]);
    let mut scaled = screens_pair();
    scaled[0].scale = 1.5;
    seeded(
        &mut engine,
        1,
        scaled,
        facts_pair(),
        "win-a",
        ingress_v4("win-a"),
    );
    let ingress = staged_ingress(&engine);
    let border = engine.presentation_border_v4(&ingress, None);
    assert!(border.visible);
    assert_eq!(border.width, Some(5));
    assert_eq!(border.radius, Some(2));
    assert_eq!(border.outsets, Some([8, 8, 8, 8]));
}

#[test]
fn scale_halves_upward() {
    let mut engine = engine_with(vec![
        ("borderWidth".to_owned(), RawSettingValue::Number(1.0)),
        ("borderGap".to_owned(), RawSettingValue::Number(0.0)),
        ("borderRadius".to_owned(), RawSettingValue::Number(0.5)),
    ]);
    let mut scaled = screens_pair();
    scaled[0].scale = 1.5;
    seeded(
        &mut engine,
        1,
        scaled,
        facts_pair(),
        "win-a",
        ingress_v4("win-a"),
    );
    let ingress = staged_ingress(&engine);
    let border = engine.presentation_border_v4(&ingress, None);
    assert!(border.visible);
    assert_eq!(border.width, Some(2));
    assert_eq!(border.radius, Some(1));
    assert_eq!(border.outsets, Some([2, 2, 2, 2]));
}

#[test]
fn scale_change_same_engine() {
    let mut engine = engine_with(vec![
        ("borderWidth".to_owned(), RawSettingValue::Number(4.0)),
        ("borderGap".to_owned(), RawSettingValue::Number(2.0)),
        ("borderRadius".to_owned(), RawSettingValue::Number(6.0)),
    ]);
    seeded(
        &mut engine,
        1,
        screens_pair(),
        facts_pair(),
        "win-a",
        ingress_v4("win-a"),
    );
    let ingress = staged_ingress(&engine);
    assert_eq!(engine.presentation_border_v4(&ingress, None).width, Some(4));
    let mut scaled = screens_pair();
    scaled[0].scale = 2.0;
    let next = report_v4(4, scaled, facts_pair(), "win-a", ingress_v4("win-a"));
    stage_v4(&mut engine, &next);
    let ingress = staged_ingress(&engine);
    let border = engine.presentation_border_v4(&ingress, None);
    assert!(border.visible);
    assert_eq!(border.width, Some(8));
    assert_eq!(border.outsets, Some([12, 12, 12, 12]));
}

#[test]
fn style_cap_boundary_holds_output() {
    let mut engine = engine_with(vec![
        ("borderWidth".to_owned(), RawSettingValue::Number(32.0)),
        ("borderGap".to_owned(), RawSettingValue::Number(0.0)),
        ("borderRadius".to_owned(), RawSettingValue::Number(0.0)),
    ]);
    let mut at_cap = screens_pair();
    at_cap[0].scale = 32.0;
    seeded(
        &mut engine,
        1,
        at_cap,
        facts_pair(),
        "win-a",
        ingress_v4("win-a"),
    );
    let ingress = staged_ingress(&engine);
    assert!(engine.presentation_border_v4(&ingress, None).visible);
    assert!(engine.presentation_style_hold_v4().is_none());

    let mut over = engine_with(vec![
        ("borderWidth".to_owned(), RawSettingValue::Number(32.0)),
        ("borderGap".to_owned(), RawSettingValue::Number(0.0)),
        ("borderRadius".to_owned(), RawSettingValue::Number(0.0)),
    ]);
    let mut past_cap = screens_pair();
    past_cap[0].scale = 33.0;
    seeded(
        &mut over,
        1,
        past_cap,
        facts_pair(),
        "win-a",
        ingress_v4("win-a"),
    );
    let ingress = staged_ingress(&over);
    assert!(!over.presentation_border_v4(&ingress, None).visible);
    let (output, _) = over.presentation_style_hold_v4().expect("hold");
    assert_eq!(output, OutputId("out-a".to_owned()));

    // Exact halfway boundary through the staged scale: 32 * 32.015625 =
    // 1024.5 rounds to 1025 and holds without publishing any wire claim.
    let mut halfway = engine_with(vec![
        ("borderWidth".to_owned(), RawSettingValue::Number(32.0)),
        ("borderGap".to_owned(), RawSettingValue::Number(0.0)),
        ("borderRadius".to_owned(), RawSettingValue::Number(0.0)),
    ]);
    let mut halfway_screens = screens_pair();
    halfway_screens[0].scale = 32.015625;
    seeded(
        &mut halfway,
        1,
        halfway_screens,
        facts_pair(),
        "win-a",
        ingress_v4("win-a"),
    );
    let ingress = staged_ingress(&halfway);
    assert!(!halfway.presentation_border_v4(&ingress, None).visible);
    assert!(halfway.presentation_style_hold_v4().is_some());
}

#[test]
fn witness_hides_via_intake() {
    let mut engine = engine_with(vec![
        ("borderWidth".to_owned(), RawSettingValue::Number(4.0)),
        ("borderGap".to_owned(), RawSettingValue::Number(2.0)),
        ("borderRadius".to_owned(), RawSettingValue::Number(6.0)),
    ]);
    seeded(
        &mut engine,
        1,
        screens_pair(),
        facts_pair(),
        "win-a",
        ingress_v4("win-a"),
    );
    let good = staged_ingress(&engine);
    assert!(engine.presentation_border_v4(&good, None).visible);

    for bad in [
        PresentationIngressV4 {
            witness: None,
            ..ingress_v4("win-a")
        },
        PresentationIngressV4 {
            witness: Some(witness("win-b", rect(100, 0, 40, 90))),
            ..ingress_v4("win-a")
        },
        PresentationIngressV4 {
            witness: Some(crate::world_report::FrameWitness {
                win: WindowId("win-a".to_owned()),
                obs: 3,
                frame: rect(0, 0, 40, 90),
                foreground: WindowId("win-b".to_owned()),
            }),
            ..ingress_v4("win-a")
        },
    ] {
        let staged = report_v4(10, screens_pair(), facts_pair(), "win-a", bad);
        engine.stage_facts_v4(&staged);
        engine.retain_pager_presentation_v4();
        let ingress = staged_ingress(&engine);
        let border = engine.presentation_border_v4(&ingress, None);
        assert!(!border.visible);
        assert_eq!(border.reason, None);
        assert_eq!(border.subject, None);
        assert_eq!(border.outsets, None);
        assert_eq!(border.width, None);
        assert_eq!(border.radius, None);
        assert_eq!(border.witness, None);
    }
}

#[test]
fn unknown_fact_fails_closed_via_intake() {
    let mut engine = engine_with(vec![
        ("borderWidth".to_owned(), RawSettingValue::Number(4.0)),
        ("borderGap".to_owned(), RawSettingValue::Number(2.0)),
        ("borderRadius".to_owned(), RawSettingValue::Number(6.0)),
    ]);
    seeded(
        &mut engine,
        1,
        screens_pair(),
        facts_pair(),
        "win-a",
        ingress_v4("win-a"),
    );
    let mut unknown = ingress_v4("win-a");
    unknown.foreground_minimized = None;
    let staged = report_v4(10, screens_pair(), facts_pair(), "win-a", unknown);
    stage_v4(&mut engine, &staged);
    let ingress = staged_ingress(&engine);
    let border = engine.presentation_border_v4(&ingress, None);
    assert!(!border.visible);
    assert_eq!(border.reason, None);

    let mut mixed = ingress_v4("win-a");
    mixed.foreground_minimized = Some(true);
    mixed.foreground_maximized = None;
    let staged = report_v4(11, screens_pair(), facts_pair(), "win-a", mixed);
    stage_v4(&mut engine, &staged);
    let ingress = staged_ingress(&engine);
    assert_eq!(
        engine.presentation_border_v4(&ingress, None).reason,
        Some(BorderHideReasonV4::Minimized)
    );
}

#[test]
fn known_hide_reasons_via_intake() {
    let mut engine = engine_with(vec![
        ("borderWidth".to_owned(), RawSettingValue::Number(4.0)),
        ("borderGap".to_owned(), RawSettingValue::Number(2.0)),
        ("borderRadius".to_owned(), RawSettingValue::Number(6.0)),
    ]);
    seeded(
        &mut engine,
        1,
        screens_pair(),
        facts_pair(),
        "win-a",
        ingress_v4("win-a"),
    );
    let cases: Vec<(PresentationIngressV4, BorderHideReasonV4)> = vec![
        (
            PresentationIngressV4 {
                foreground_minimized: Some(true),
                ..ingress_v4("win-a")
            },
            BorderHideReasonV4::Minimized,
        ),
        (
            PresentationIngressV4 {
                foreground_fullscreen: Some(true),
                ..ingress_v4("win-a")
            },
            BorderHideReasonV4::Fullscreen,
        ),
        (
            PresentationIngressV4 {
                foreground_maximized: Some(true),
                ..ingress_v4("win-a")
            },
            BorderHideReasonV4::Maximized,
        ),
        (
            PresentationIngressV4 {
                foreground_host_hidden: Some(true),
                ..ingress_v4("win-a")
            },
            BorderHideReasonV4::HostHidden,
        ),
        (
            PresentationIngressV4 {
                foreground_hidden_workspace: Some(true),
                ..ingress_v4("win-a")
            },
            BorderHideReasonV4::HiddenWorkspace,
        ),
        (
            PresentationIngressV4 {
                foreground: String::new(),
                foreground_frame: None,
                witness: None,
                foreground_out: None,
                ..ingress_v4("win-a")
            },
            BorderHideReasonV4::NoTarget,
        ),
        (
            PresentationIngressV4 {
                foreground_frame: None,
                ..ingress_v4("win-a")
            },
            BorderHideReasonV4::NoTarget,
        ),
    ];
    for (seq, (bad, reason)) in cases.into_iter().enumerate() {
        let staged = report_v4(10 + seq as u64, screens_pair(), facts_pair(), "win-a", bad);
        stage_v4(&mut engine, &staged);
        let ingress = staged_ingress(&engine);
        let border = engine.presentation_border_v4(&ingress, None);
        assert!(!border.visible, "seq {seq}");
        assert_eq!(border.reason, Some(reason), "seq {seq}");
        assert_eq!(border.subject, None, "seq {seq}");
        assert_eq!(border.outsets, None, "seq {seq}");
        assert_eq!(border.width, None, "seq {seq}");
        assert_eq!(border.radius, None, "seq {seq}");
        assert_eq!(border.witness, None, "seq {seq}");
    }
}

#[test]
fn shell_and_disabled_hides() {
    let mut engine = engine_with(vec![
        ("borderWidth".to_owned(), RawSettingValue::Number(4.0)),
        ("borderGap".to_owned(), RawSettingValue::Number(2.0)),
        ("borderRadius".to_owned(), RawSettingValue::Number(6.0)),
    ]);
    let mut shell_fact = fact("win-a", "out-a", 0);
    shell_fact.class = Some("Progman".to_owned());
    let mut windows = facts_pair();
    windows[0] = shell_fact;
    seeded(
        &mut engine,
        1,
        screens_pair(),
        windows,
        "win-a",
        ingress_v4("win-a"),
    );
    let ingress = staged_ingress(&engine);
    let border = engine.presentation_border_v4(&ingress, None);
    assert!(!border.visible);
    assert_eq!(border.reason, Some(BorderHideReasonV4::Shell));

    let mut off = engine_with(vec![(
        "borderEnabled".to_owned(),
        RawSettingValue::Bool(false),
    )]);
    seeded(
        &mut off,
        1,
        screens_pair(),
        facts_pair(),
        "win-a",
        ingress_v4("win-a"),
    );
    let ingress = staged_ingress(&off);
    assert_eq!(
        off.presentation_border_v4(&ingress, None).reason,
        Some(BorderHideReasonV4::Disabled)
    );
}

#[test]
fn zero_width_and_missing_scale_hide_empty() {
    let mut engine = engine_with(vec![(
        "borderWidth".to_owned(),
        RawSettingValue::Number(0.0),
    )]);
    seeded(
        &mut engine,
        1,
        screens_pair(),
        facts_pair(),
        "win-a",
        ingress_v4("win-a"),
    );
    let ingress = staged_ingress(&engine);
    let border = engine.presentation_border_v4(&ingress, None);
    assert!(!border.visible);
    assert_eq!(border.reason, None);

    let mut engine = engine_with(vec![
        ("borderWidth".to_owned(), RawSettingValue::Number(4.0)),
        ("borderGap".to_owned(), RawSettingValue::Number(2.0)),
        ("borderRadius".to_owned(), RawSettingValue::Number(6.0)),
    ]);
    let mut off_out = ingress_v4("win-a");
    off_out.foreground_out = Some(OutputId("out-ghost".to_owned()));
    seeded(
        &mut engine,
        1,
        screens_pair(),
        facts_pair(),
        "win-a",
        off_out,
    );
    let ingress = staged_ingress(&engine);
    let border = engine.presentation_border_v4(&ingress, None);
    assert!(!border.visible);
    assert_eq!(border.reason, None);
}

#[test]
fn held_screen_missing_scale_hides_without_hold() {
    use crate::world_report::{HeldScreenRow, RowDiagnostic};
    let mut engine = engine_with(vec![
        ("borderWidth".to_owned(), RawSettingValue::Number(4.0)),
        ("borderGap".to_owned(), RawSettingValue::Number(2.0)),
        ("borderRadius".to_owned(), RawSettingValue::Number(6.0)),
    ]);
    seeded(
        &mut engine,
        1,
        screens_pair(),
        facts_pair(),
        "win-a",
        ingress_v4("win-a"),
    );
    let held = HeldScreenRow {
        output: OutputId("out-a".to_owned()),
        ordinal: 0,
        fields: Vec::new(),
        diagnostic: RowDiagnostic {
            scope: crate::world_report::AuthorityScope::Output(OutputId("out-a".to_owned())),
            row: RowDiagnosticLane::Screen,
            id: Some("out-a".to_owned()),
            kind: RowDiagnosticKind::InvalidFact,
        },
    };
    let mut report = report_v4(10, Vec::new(), facts_pair(), "win-a", ingress_v4("win-a"));
    report.held_screens = vec![held];
    stage_v4(&mut engine, &report);
    let ingress = staged_ingress(&engine);
    let border = engine.presentation_border_v4(&ingress, None);
    assert!(!border.visible);
    assert_eq!(border.reason, None);
    assert!(engine.presentation_style_hold_v4().is_none());
}

#[test]
fn foreground_out_contradiction_fails_closed() {
    let mut engine = engine_with(vec![
        ("borderWidth".to_owned(), RawSettingValue::Number(4.0)),
        ("borderGap".to_owned(), RawSettingValue::Number(2.0)),
        ("borderRadius".to_owned(), RawSettingValue::Number(6.0)),
    ]);
    let mut windows = facts_pair();
    windows[0].fullscreen = Some(true);
    let mut bad = ingress_v4("win-a");
    bad.foreground_fullscreen = Some(true);
    bad.foreground_out = Some(OutputId("out-b".to_owned()));
    seeded(&mut engine, 1, screens_pair(), windows, "win-a", bad);
    let ingress = staged_ingress(&engine);
    // Border hides on the truthful fullscreen verdict (priority preserved);
    // the raw/staged out contradiction no longer vetoes suspension.
    let border = engine.presentation_border_v4(&ingress, None);
    assert!(!border.visible);
    assert_eq!(border.reason, Some(BorderHideReasonV4::Fullscreen));
    // ... while pager suspension stays unconditional on the truthful fact
    // output, omitting the suspended screen but preserving the other.
    assert_eq!(
        engine.focused_fullscreen_suspended_output_v4(),
        Some(OutputId("out-a".to_owned()))
    );
    let published = engine.presentation_for_publish_v4();
    assert_eq!(
        published.suspended_output,
        Some(OutputId("out-a".to_owned()))
    );
    assert_eq!(published.pager.len(), 1);
    assert_eq!(published.pager[0].output, "out-b");
}

#[test]
fn suspended_output_hides_locally_preserving_other_pager() {
    let mut engine = engine_with(vec![
        ("borderWidth".to_owned(), RawSettingValue::Number(4.0)),
        ("borderGap".to_owned(), RawSettingValue::Number(2.0)),
        ("borderRadius".to_owned(), RawSettingValue::Number(6.0)),
        ("underlayExtension".to_owned(), RawSettingValue::Number(3.0)),
    ]);
    let mut full = facts_pair();
    full[0].fullscreen = Some(true);
    let mut ingress = ingress_v4("win-a");
    ingress.foreground_fullscreen = Some(true);
    ingress.chord_held = true;
    ingress.pager_order = vec![WindowId("win-a".to_owned()), WindowId("win-c".to_owned())];
    ingress.pager_frames = vec![
        PresentationPagerFrameV4 {
            window: WindowId("win-a".to_owned()),
            live: Some(rect(0, 0, 40, 90)),
        },
        PresentationPagerFrameV4 {
            window: WindowId("win-c".to_owned()),
            live: Some(rect(800, 0, 40, 90)),
        },
    ];
    seeded(&mut engine, 1, screens_pair(), full, "win-a", ingress);
    let published = engine.presentation_for_publish_v4();
    assert_eq!(
        published.suspended_output,
        Some(OutputId("out-a".to_owned()))
    );
    assert!(!published.border.visible);
    assert_eq!(published.border.reason, Some(BorderHideReasonV4::Suspended));
    assert!(matches!(
        published.underlay,
        PresentationUnderlay::Hidden {
            reason: GroupHideReason::Suspended,
            ..
        }
    ));
    assert_eq!(published.pager.len(), 1);
    assert_eq!(published.pager[0].output, "out-b");
    let paints_c = published.pager[0]
        .workspaces
        .iter()
        .flat_map(|ws| ws.windows.iter())
        .find(|w| w.window.0 == "win-c")
        .expect("eligible out-b frame remains drawn");
    assert_eq!(paints_c.frame, rect(800, 0, 40, 90));
}

#[test]
fn unmanaged_fullscreen_suspends_via_raw_foreground_out() {
    let mut engine = engine_with(vec![
        ("borderWidth".to_owned(), RawSettingValue::Number(4.0)),
        ("borderGap".to_owned(), RawSettingValue::Number(2.0)),
        ("borderRadius".to_owned(), RawSettingValue::Number(6.0)),
    ]);
    let mut ingress = ingress_v4("ghost");
    ingress.foreground_fullscreen = Some(true);
    ingress.foreground_out = Some(OutputId("out-b".to_owned()));
    seeded(
        &mut engine,
        1,
        screens_pair(),
        facts_pair(),
        "ghost",
        ingress,
    );
    assert_eq!(
        engine.focused_fullscreen_suspended_output_v4(),
        Some(OutputId("out-b".to_owned()))
    );
    let published = engine.presentation_for_publish_v4();
    assert_eq!(
        published.suspended_output,
        Some(OutputId("out-b".to_owned()))
    );
    assert_eq!(published.pager.len(), 1);
    assert_eq!(published.pager[0].output, "out-a");
}

#[test]
fn no_suspension_without_truthful_fullscreen() {
    let mut engine = engine_with(vec![]);
    seeded(
        &mut engine,
        1,
        screens_pair(),
        facts_pair(),
        "win-a",
        ingress_v4("win-a"),
    );
    assert_eq!(engine.focused_fullscreen_suspended_output_v4(), None);
    let second = report_v4(10, screens_pair(), facts_pair(), "", ingress_v4(""));
    stage_v4(&mut engine, &second);
    assert_eq!(engine.focused_fullscreen_suspended_output_v4(), None);
}

#[test]
fn underlay_found_scaled() {
    let mut engine = engine_with(vec![
        ("borderWidth".to_owned(), RawSettingValue::Number(4.0)),
        ("borderGap".to_owned(), RawSettingValue::Number(2.0)),
        ("borderRadius".to_owned(), RawSettingValue::Number(6.0)),
        ("underlayExtension".to_owned(), RawSettingValue::Number(3.0)),
    ]);
    seeded(
        &mut engine,
        1,
        screens_pair(),
        facts_pair(),
        "win-a",
        PresentationIngressV4 {
            chord_held: true,
            ..ingress_v4("win-a")
        },
    );
    let ingress = staged_ingress(&engine);
    match engine.presentation_underlay_v4(&ingress, None) {
        PresentationUnderlay::Found(found) => {
            assert_eq!(found.focused_window, WindowId("win-a".to_owned()));
            assert_eq!(found.members.len(), 2);
            assert_eq!(
                found.outer_rect,
                Rect {
                    x: found.union_rect.x - 9,
                    y: found.union_rect.y - 9,
                    w: found.union_rect.w + 18,
                    h: found.union_rect.h + 18,
                }
            );
        }
        hidden => panic!("expected found, got {hidden:?}"),
    }
}

#[test]
fn underlay_scaled_extension_at_fractional_scale() {
    let mut engine = engine_with(vec![
        ("borderWidth".to_owned(), RawSettingValue::Number(4.0)),
        ("borderGap".to_owned(), RawSettingValue::Number(2.0)),
        ("borderRadius".to_owned(), RawSettingValue::Number(6.0)),
        ("underlayExtension".to_owned(), RawSettingValue::Number(3.0)),
    ]);
    let mut scaled = screens_pair();
    scaled[0].scale = 2.0;
    seeded(
        &mut engine,
        1,
        scaled,
        facts_pair(),
        "win-a",
        PresentationIngressV4 {
            chord_held: true,
            ..ingress_v4("win-a")
        },
    );
    let ingress = staged_ingress(&engine);
    match engine.presentation_underlay_v4(&ingress, None) {
        PresentationUnderlay::Found(found) => {
            assert_eq!(
                found.outer_rect,
                Rect {
                    x: found.union_rect.x - 18,
                    y: found.union_rect.y - 18,
                    w: found.union_rect.w + 36,
                    h: found.union_rect.h + 36,
                }
            );
        }
        hidden => panic!("expected found, got {hidden:?}"),
    }
}

#[test]
fn underlay_sentinel_follows_border_width_before_scale() {
    let mut engine = engine_with(vec![
        ("borderWidth".to_owned(), RawSettingValue::Number(4.0)),
        ("borderGap".to_owned(), RawSettingValue::Number(2.0)),
        (
            "underlayExtension".to_owned(),
            RawSettingValue::Number(-1.0),
        ),
    ]);
    let mut scaled = screens_pair();
    scaled[0].scale = 2.0;
    seeded(
        &mut engine,
        1,
        scaled,
        facts_pair(),
        "win-a",
        PresentationIngressV4 {
            chord_held: true,
            ..ingress_v4("win-a")
        },
    );
    let ingress = staged_ingress(&engine);
    match engine.presentation_underlay_v4(&ingress, None) {
        PresentationUnderlay::Found(found) => {
            // gap 4 + width 8 + sentinel-width 8 = 20.
            assert_eq!(
                found.outer_rect,
                Rect {
                    x: found.union_rect.x - 20,
                    y: found.union_rect.y - 20,
                    w: found.union_rect.w + 40,
                    h: found.union_rect.h + 40,
                }
            );
        }
        hidden => panic!("expected found, got {hidden:?}"),
    }

    let mut explicit = engine_with(vec![
        ("borderWidth".to_owned(), RawSettingValue::Number(4.0)),
        ("borderGap".to_owned(), RawSettingValue::Number(2.0)),
        ("underlayExtension".to_owned(), RawSettingValue::Number(1.5)),
    ]);
    let mut scaled = screens_pair();
    scaled[0].scale = 2.0;
    seeded(
        &mut explicit,
        1,
        scaled,
        facts_pair(),
        "win-a",
        PresentationIngressV4 {
            chord_held: true,
            ..ingress_v4("win-a")
        },
    );
    let ingress = staged_ingress(&explicit);
    match explicit.presentation_underlay_v4(&ingress, None) {
        PresentationUnderlay::Found(found) => {
            // gap 4 + width 8 + extension floor(1.5*2+0.5)=3 => 15.
            assert_eq!(
                found.outer_rect,
                Rect {
                    x: found.union_rect.x - 15,
                    y: found.union_rect.y - 15,
                    w: found.union_rect.w + 30,
                    h: found.union_rect.h + 30,
                }
            );
        }
        hidden => panic!("expected found, got {hidden:?}"),
    }
}

#[test]
fn underlay_fractional_negative_extension_follows_width() {
    let mut engine = engine_with(vec![
        ("borderWidth".to_owned(), RawSettingValue::Number(4.0)),
        ("borderGap".to_owned(), RawSettingValue::Number(2.0)),
        (
            "underlayExtension".to_owned(),
            RawSettingValue::Number(-0.5),
        ),
    ]);
    let mut scaled = screens_pair();
    scaled[0].scale = 2.0;
    seeded(
        &mut engine,
        1,
        scaled,
        facts_pair(),
        "win-a",
        PresentationIngressV4 {
            chord_held: true,
            ..ingress_v4("win-a")
        },
    );
    let ingress = staged_ingress(&engine);
    match engine.presentation_underlay_v4(&ingress, None) {
        PresentationUnderlay::Found(found) => {
            // -0.5 follows the border width exactly like -1 through the
            // shared helper: gap 4 + width 8 + extension 8 = 20.
            assert_eq!(
                found.outer_rect,
                Rect {
                    x: found.union_rect.x - 20,
                    y: found.union_rect.y - 20,
                    w: found.union_rect.w + 40,
                    h: found.union_rect.h + 40,
                }
            );
        }
        hidden => panic!("expected found, got {hidden:?}"),
    }
    // The style-hold path resolves through the same helper: representable,
    // so no hold is raised.
    assert!(engine.presentation_style_hold_v4().is_none());
}

fn composed_null_windows() -> Vec<WindowFact> {
    let mut windows = facts_pair();
    for window in &mut windows {
        window.maximized = None;
    }
    windows
}

fn composed_styles() -> Vec<(String, RawSettingValue)> {
    vec![
        ("borderWidth".to_owned(), RawSettingValue::Number(4.0)),
        ("borderGap".to_owned(), RawSettingValue::Number(2.0)),
        ("borderRadius".to_owned(), RawSettingValue::Number(6.0)),
        ("underlayExtension".to_owned(), RawSettingValue::Number(3.0)),
    ]
}

#[test]
fn composed_route_null_maximize_stays_eligible() {
    let mut engine = engine_with_caps(caps_composed(), composed_styles());
    let mut ingress = ingress_v4("win-a");
    ingress.foreground_maximized = None;
    ingress.chord_held = true;
    seeded(
        &mut engine,
        1,
        screens_pair(),
        composed_null_windows(),
        "win-a",
        ingress,
    );
    let ingress = staged_ingress(&engine);
    // Native null is valid with no native support: the border renders and
    // the underlay resolves instead of hiding Maximized.
    let border = engine.presentation_border_v4(&ingress, None);
    assert!(border.visible);
    assert_eq!(border.reason, None);
    assert!(matches!(
        engine.presentation_underlay_v4(&ingress, None),
        PresentationUnderlay::Found(_)
    ));
}

#[test]
fn composed_route_owned_bit_hides_maximized() {
    let mut engine = engine_with_caps(caps_composed(), composed_styles());
    let mut ingress = ingress_v4("win-a");
    ingress.foreground_maximized = None;
    ingress.chord_held = true;
    seeded(
        &mut engine,
        1,
        screens_pair(),
        composed_null_windows(),
        "win-a",
        ingress,
    );
    // Owned-bit toggle path is covered by the command tests; preset the
    // settled bit to drive the producer verdict.
    engine
        .composed_maximized
        .insert(WindowId("win-a".to_owned()));
    let ingress = staged_ingress(&engine);
    let border = engine.presentation_border_v4(&ingress, None);
    assert!(!border.visible);
    assert_eq!(border.reason, Some(BorderHideReasonV4::Maximized));
    assert!(matches!(
        engine.presentation_underlay_v4(&ingress, None),
        PresentationUnderlay::Hidden {
            reason: GroupHideReason::Maximized,
            ..
        }
    ));
}

#[test]
fn maximize_unknown_fails_closed_on_both_routes() {
    // Native route: null stays unknown and fail-closes.
    let mut native = engine_with(composed_styles());
    let mut null_ingress = ingress_v4("win-a");
    null_ingress.foreground_maximized = None;
    seeded(
        &mut native,
        1,
        screens_pair(),
        facts_pair(),
        "win-a",
        null_ingress,
    );
    let ingress = staged_ingress(&native);
    let border = native.presentation_border_v4(&ingress, None);
    assert!(!border.visible);
    assert_eq!(border.reason, None);

    // Composed route: a non-null native claim is conflicting input and
    // fail-closes as unknown, never a Maximized verdict off untrusted input.
    let mut composed = engine_with_caps(caps_composed(), composed_styles());
    let mut claimed = ingress_v4("win-a");
    claimed.foreground_maximized = Some(true);
    seeded(
        &mut composed,
        1,
        screens_pair(),
        composed_null_windows(),
        "win-a",
        claimed,
    );
    let ingress = staged_ingress(&composed);
    let border = composed.presentation_border_v4(&ingress, None);
    assert!(!border.visible);
    assert_eq!(border.reason, None);
}

#[test]
fn underlay_trigger_and_idle() {
    let mut engine = engine_with(vec![(
        "underlayExtension".to_owned(),
        RawSettingValue::Number(3.0),
    )]);
    seeded(
        &mut engine,
        1,
        screens_pair(),
        facts_pair(),
        "win-a",
        PresentationIngressV4 {
            move_window: "win-a".to_owned(),
            ..ingress_v4("win-a")
        },
    );
    let ingress = staged_ingress(&engine);
    assert!(matches!(
        engine.presentation_underlay_v4(&ingress, None),
        PresentationUnderlay::Found(_)
    ));
    let idle = report_v4(
        10,
        screens_pair(),
        facts_pair(),
        "win-a",
        ingress_v4("win-a"),
    );
    stage_v4(&mut engine, &idle);
    let ingress = staged_ingress(&engine);
    assert_eq!(
        engine.presentation_underlay_v4(&ingress, None),
        PresentationUnderlay::Hidden {
            base_revision: None,
            reason: GroupHideReason::Idle,
        }
    );
}

#[test]
fn underlay_unknown_fact_fails_closed() {
    let mut engine = engine_with(vec![]);
    let mut unknown = ingress_v4("win-a");
    unknown.foreground_minimized = None;
    unknown.chord_held = true;
    seeded(
        &mut engine,
        1,
        screens_pair(),
        facts_pair(),
        "win-a",
        unknown,
    );
    let ingress = staged_ingress(&engine);
    assert_eq!(
        engine.presentation_underlay_v4(&ingress, None),
        PresentationUnderlay::Hidden {
            base_revision: None,
            reason: GroupHideReason::NoGroup,
        }
    );
}

#[test]
fn held_presentation_publishes_neutral_shapes() {
    let mut engine = engine_with(vec![]);
    seeded(
        &mut engine,
        1,
        screens_pair(),
        facts_pair(),
        "win-a",
        ingress_v4("win-a"),
    );
    let held = WorldReportV4 {
        presentation: PresentationRowV4::Held(crate::world_report::HeldPresentationRow {
            fields: Vec::new(),
            kind: crate::world_report::RowDiagnosticKind::InvalidFact,
            diagnostic: None,
        }),
        ..report_v4(
            4,
            screens_pair(),
            facts_pair(),
            "win-a",
            ingress_v4("win-a"),
        )
    };
    engine.stage_facts_v4(&held);
    engine.retain_pager_presentation_v4();
    let published = engine.presentation_for_publish_v4();
    assert_eq!(published.foreground, String::new());
    assert!(!published.border.visible);
    assert_eq!(published.border.reason, None);
    assert_eq!(
        published.underlay,
        PresentationUnderlay::Hidden {
            base_revision: None,
            reason: GroupHideReason::NoGroup,
        }
    );
}

#[test]
fn pager_fresh_and_retained_sources() {
    let mut engine = engine_with(vec![]);
    let mut first = ingress_v4("win-a");
    first.pager_order = vec![WindowId("win-a".to_owned()), WindowId("win-b".to_owned())];
    first.pager_frames = vec![
        PresentationPagerFrameV4 {
            window: WindowId("win-a".to_owned()),
            live: Some(rect(0, 0, 40, 90)),
        },
        PresentationPagerFrameV4 {
            window: WindowId("win-b".to_owned()),
            live: Some(rect(100, 0, 40, 90)),
        },
    ];
    seeded(&mut engine, 1, screens_pair(), facts_pair(), "win-a", first);
    let mut second = ingress_v4("win-a");
    second.pager_order = vec![
        WindowId("win-a".to_owned()),
        WindowId("win-new".to_owned()),
        WindowId("ghost".to_owned()),
    ];
    second.pager_frames = vec![
        PresentationPagerFrameV4 {
            window: WindowId("win-a".to_owned()),
            live: Some(rect(5, 5, 40, 90)),
        },
        PresentationPagerFrameV4 {
            window: WindowId("ghost".to_owned()),
            live: Some(rect(9, 9, 10, 10)),
        },
    ];
    let staged = report_v4(4, screens_pair(), facts_pair(), "win-a", second);
    stage_v4(&mut engine, &staged);
    let pager = engine.presentation_pager_v4(None);
    let ws_a = pager
        .iter()
        .find(|screen| screen.output == "out-a")
        .expect("out-a screen")
        .workspaces
        .iter()
        .find(|ws| ws.windows.iter().any(|w| w.window.0 == "win-a"))
        .expect("workspace painting win-a");
    let frame_a = ws_a
        .windows
        .iter()
        .find(|w| w.window.0 == "win-a")
        .expect("win-a paints")
        .frame;
    assert_eq!(frame_a, rect(5, 5, 40, 90));
    let frame_b = ws_a
        .windows
        .iter()
        .find(|w| w.window.0 == "win-b")
        .expect("win-b retained")
        .frame;
    assert_eq!(frame_b, rect(100, 0, 40, 90));
    assert!(!ws_a.windows.iter().any(|w| w.window.0 == "win-new"));
    assert!(!ws_a.windows.iter().any(|w| w.window.0 == "ghost"));
}

#[test]
fn pager_close_prunes_order_frame_and_focus() {
    let mut engine = engine_with(vec![]);
    let mut first = ingress_v4("win-a");
    first.pager_order = vec![WindowId("win-a".to_owned()), WindowId("win-b".to_owned())];
    first.pager_frames = vec![
        PresentationPagerFrameV4 {
            window: WindowId("win-a".to_owned()),
            live: Some(rect(0, 0, 40, 90)),
        },
        PresentationPagerFrameV4 {
            window: WindowId("win-b".to_owned()),
            live: Some(rect(100, 0, 40, 90)),
        },
    ];
    seeded(&mut engine, 1, screens_pair(), facts_pair(), "win-b", first);
    assert!(
        engine
            .presentation_order_v4
            .contains(&WindowId("win-b".to_owned()))
    );
    let mut closed = report_v4(
        4,
        screens_pair(),
        facts_pair(),
        "win-b",
        ingress_v4("win-a"),
    );
    closed.closed = vec![WindowId("win-b".to_owned())];
    stage_v4(&mut engine, &closed);
    assert!(
        !engine
            .presentation_order_v4
            .contains(&WindowId("win-b".to_owned()))
    );
    assert!(
        !engine
            .presentation_live_v4
            .contains_key(&WindowId("win-b".to_owned()))
    );
    assert_eq!(engine.presentation_focused_v4, None);
}

#[test]
fn pager_complete_absence_prunes_incomplete_retains() {
    let mut engine = engine_with(vec![]);
    let mut first = ingress_v4("win-a");
    first.pager_order = vec![WindowId("win-a".to_owned()), WindowId("win-b".to_owned())];
    first.pager_frames = vec![
        PresentationPagerFrameV4 {
            window: WindowId("win-a".to_owned()),
            live: Some(rect(0, 0, 40, 90)),
        },
        PresentationPagerFrameV4 {
            window: WindowId("win-b".to_owned()),
            live: Some(rect(100, 0, 40, 90)),
        },
    ];
    seeded(&mut engine, 1, screens_pair(), facts_pair(), "win-a", first);
    // Incomplete gap: win-b absent from facts but inventory incomplete, so
    // the last-known frame is retained.
    let mut gap = report_v4(
        4,
        screens_pair(),
        vec![fact("win-a", "out-a", 0), fact("win-c", "out-b", 800)],
        "win-a",
        ingress_v4("win-a"),
    );
    gap.inventory_complete = false;
    stage_v4(&mut engine, &gap);
    assert!(
        engine
            .presentation_live_v4
            .contains_key(&WindowId("win-b".to_owned()))
    );

    // Complete absence: win-b gone from a complete inventory prunes.
    let pruned = report_v4(
        5,
        screens_pair(),
        vec![fact("win-a", "out-a", 0), fact("win-c", "out-b", 800)],
        "win-a",
        ingress_v4("win-a"),
    );
    stage_v4(&mut engine, &pruned);
    assert!(
        !engine
            .presentation_order_v4
            .contains(&WindowId("win-b".to_owned()))
    );
    assert!(
        !engine
            .presentation_live_v4
            .contains_key(&WindowId("win-b".to_owned()))
    );
}

#[test]
fn pager_capability_held_raw_presence_retains() {
    let mut engine = engine_with_caps(
        HostCapabilities {
            unavailable_facts: vec![FactName::Executable],
            ..caps_full()
        },
        vec![],
    );
    let mut first = ingress_v4("win-a");
    first.pager_order = vec![WindowId("win-a".to_owned()), WindowId("win-b".to_owned())];
    first.pager_frames = vec![
        PresentationPagerFrameV4 {
            window: WindowId("win-a".to_owned()),
            live: Some(rect(0, 0, 40, 90)),
        },
        PresentationPagerFrameV4 {
            window: WindowId("win-b".to_owned()),
            live: Some(rect(100, 0, 40, 90)),
        },
    ];
    let mut plain_a = fact("win-a", "out-a", 0);
    plain_a.executable = None;
    let mut contradictory = fact("win-b", "out-a", 100);
    contradictory.executable = Some("c:\\synthetic\\example.exe".to_owned());
    let windows = vec![plain_a, contradictory];
    seeded(
        &mut engine,
        1,
        screens_pair(),
        windows.clone(),
        "win-a",
        first,
    );
    assert!(
        !engine
            .staged
            .facts
            .contains_key(&WindowId("win-b".to_owned()))
    );
    assert_eq!(engine.staged.held_windows.len(), 1);
    let retained = report_v4(4, screens_pair(), windows, "win-a", ingress_v4("win-a"));
    stage_v4(&mut engine, &retained);
    assert!(
        engine
            .presentation_order_v4
            .contains(&WindowId("win-b".to_owned()))
    );
    assert!(
        engine
            .presentation_live_v4
            .contains_key(&WindowId("win-b".to_owned()))
    );
}

#[test]
fn pager_own_process_exclusion_raw_presence_retains() {
    let mut engine = engine_with(vec![]);
    let mut first = ingress_v4("win-a");
    first.pager_order = vec![WindowId("win-a".to_owned()), WindowId("win-b".to_owned())];
    first.pager_frames = vec![
        PresentationPagerFrameV4 {
            window: WindowId("win-a".to_owned()),
            live: Some(rect(0, 0, 40, 90)),
        },
        PresentationPagerFrameV4 {
            window: WindowId("win-b".to_owned()),
            live: Some(rect(100, 0, 40, 90)),
        },
    ];
    seeded(&mut engine, 1, screens_pair(), facts_pair(), "win-a", first);
    let mut own = fact("win-b", "out-a", 100);
    own.process_id = Some("test-own-pid".to_owned());
    let windows = vec![fact("win-a", "out-a", 0), own];
    let excluded = report_v4(4, screens_pair(), windows, "win-a", ingress_v4("win-a"));
    stage_v4(&mut engine, &excluded);
    assert!(
        engine
            .staged
            .excluded
            .contains(&WindowId("win-b".to_owned()))
    );
    assert!(
        !engine
            .staged
            .facts
            .contains_key(&WindowId("win-b".to_owned()))
    );
    assert!(
        engine
            .presentation_order_v4
            .contains(&WindowId("win-b".to_owned()))
    );
    assert!(
        engine
            .presentation_live_v4
            .contains_key(&WindowId("win-b".to_owned()))
    );
}

#[test]
fn held_presentation_still_prunes_closed_lifetimes() {
    let mut engine = engine_with(vec![]);
    let mut first = ingress_v4("win-a");
    first.pager_order = vec![WindowId("win-a".to_owned()), WindowId("win-b".to_owned())];
    first.pager_frames = vec![
        PresentationPagerFrameV4 {
            window: WindowId("win-a".to_owned()),
            live: Some(rect(0, 0, 40, 90)),
        },
        PresentationPagerFrameV4 {
            window: WindowId("win-b".to_owned()),
            live: Some(rect(100, 0, 40, 90)),
        },
    ];
    seeded(&mut engine, 1, screens_pair(), facts_pair(), "win-a", first);
    let mut held = report_v4(
        4,
        screens_pair(),
        facts_pair(),
        "win-a",
        ingress_v4("win-a"),
    );
    held.presentation = PresentationRowV4::Held(crate::world_report::HeldPresentationRow {
        fields: Vec::new(),
        kind: crate::world_report::RowDiagnosticKind::InvalidFact,
        diagnostic: None,
    });
    held.closed = vec![WindowId("win-b".to_owned())];
    stage_v4(&mut engine, &held);
    assert!(
        !engine
            .presentation_order_v4
            .contains(&WindowId("win-b".to_owned()))
    );
    assert!(
        !engine
            .presentation_live_v4
            .contains_key(&WindowId("win-b".to_owned()))
    );
    let published = engine.presentation_for_publish_v4();
    assert_eq!(published.foreground, String::new());
    assert!(!published.border.visible);
}

#[test]
fn producer_does_not_mutate_staged_or_gesture() {
    let mut engine = engine_with(vec![
        ("borderWidth".to_owned(), RawSettingValue::Number(4.0)),
        ("borderGap".to_owned(), RawSettingValue::Number(2.0)),
    ]);
    let mut ingress = ingress_v4("win-a");
    ingress.chord_held = true;
    seeded(
        &mut engine,
        1,
        screens_pair(),
        facts_pair(),
        "win-a",
        ingress,
    );
    let staged_before = engine.staged.clone();
    let members_before = engine.members.clone();
    let order_before = engine.presentation_order_v4.clone();
    let live_before = engine.presentation_live_v4.clone();
    let focused_before = engine.presentation_focused_v4.clone();
    let holds_before = engine.hold_reasons.clone();
    let revision_before = engine.state_revision;
    let first = engine.presentation_for_publish_v4();
    let second = engine.presentation_for_publish_v4();
    assert_eq!(first, second);
    assert_eq!(engine.staged, staged_before);
    assert_eq!(engine.members, members_before);
    assert_eq!(engine.presentation_order_v4, order_before);
    assert_eq!(engine.presentation_live_v4, live_before);
    assert_eq!(engine.presentation_focused_v4, focused_before);
    assert_eq!(engine.hold_reasons, holds_before);
    assert_eq!(engine.state_revision, revision_before);
}

#[test]
fn scale_float_once_rejects_non_finite_overflow_and_cap() {
    assert_eq!(WorldEngine::scale_float_once(3.0, 1.5), Some(5));
    assert_eq!(WorldEngine::scale_float_once(0.5, 1.0), Some(1));
    assert_eq!(WorldEngine::scale_float_once(1024.0, 1.0), Some(1024));
    assert_eq!(WorldEngine::scale_float_once(1024.5, 1.0), None);
    assert_eq!(WorldEngine::scale_float_once(3.0, f64::INFINITY), None);
    assert_eq!(WorldEngine::scale_float_once(f64::NAN, 1.0), None);
    assert_eq!(WorldEngine::scale_float_once(f64::MAX, 2.0), None);
}

fn accepted_v4(engine: &mut WorldEngine, report: &WorldReportV4) -> WorldReplyV4 {
    let outcome = engine.handle_report_v4(
        report,
        &EventIssue {
            owner: OwnerId::parse("adapter-1").expect("owner"),
            correlation: CorrelationId::parse(&report.request_id).expect("correlation"),
        },
    );
    match outcome {
        V4RunOutcome::Accepted(reply) => {
            assert_eq!(reply.based_on_observation_seq, report.observation_seq);
            assert!(!reply.command_consumed);
            reply
        }
        other => panic!("expected accepted v4 admission, got {other:?}"),
    }
}

fn chord_ingress_v4(fg: &str, frame: Rect, seq: u64) -> PresentationIngressV4 {
    let mut ingress = ingress_v4(fg);
    ingress.foreground_frame = Some(frame);
    ingress.chord_held = true;
    ingress.witness = Some(crate::world_report::FrameWitness {
        win: WindowId(fg.to_owned()),
        obs: seq,
        frame,
        foreground: WindowId(fg.to_owned()),
    });
    ingress
}

fn settled_fact(window: &str, output: &str, frame: Rect) -> WindowFact {
    let mut row = fact(window, output, 0);
    row.rect = Some(frame);
    row
}

fn effective_frame(reply: &WorldReplyV4, window: &str) -> Rect {
    reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == window)
        .unwrap_or_else(|| panic!("expected planned row for {window}"))
        .effective_frame
        .unwrap_or_else(|| panic!("expected effective frame for {window}"))
}

#[test]
fn logical_topology_reflow_at_scale_one_through_admit() {
    // Genuine v4 handle/report route (never stage-only): an ordinary tiled
    // pair at scale 1, a logical bounds shrink, then settled native facts
    // so the found underlay recomputes. Engine defaults hold throughout:
    // inner/outer gap 8, border width 3, border gap 0, radius 6, and the
    // underlay sentinel -1 (follows the border width).
    let mut engine = engine_with(vec![]);
    // out-a work (0,0,800,560) insets by outer 8 to (8,8,784,544); the pair
    // splits the 784 width with one inner 8: (784-8)/2 = 388 per tile.
    // Stack order puts win-b left and the focused win-a right.
    let initial_a = rect(404, 8, 388, 544);
    let initial_b = rect(8, 8, 388, 544);
    let first = report_v4(
        1,
        screens_pair(),
        facts_pair(),
        "win-a",
        chord_ingress_v4("win-a", rect(0, 0, 40, 90), 1),
    );
    assert_eq!(
        first.screens[0].scale, 1.0,
        "initial topology reports scale 1"
    );
    accepted_v4(&mut engine, &first);
    // A second identical report latches managed focus before presentation
    // reads the retained focus.
    let second = report_v4(
        2,
        screens_pair(),
        facts_pair(),
        "win-a",
        chord_ingress_v4("win-a", rect(0, 0, 40, 90), 2),
    );
    let settled = accepted_v4(&mut engine, &second);
    assert_eq!(effective_frame(&settled, "win-a"), initial_a);
    assert_eq!(effective_frame(&settled, "win-b"), initial_b);
    assert!(settled.presentation.border.visible);
    assert_eq!(
        settled.presentation.border.subject,
        Some(WindowId("win-a".to_owned()))
    );
    assert_eq!(settled.presentation.border.outsets, Some([3, 3, 3, 3]));
    assert_eq!(settled.presentation.border.width, Some(3));
    assert_eq!(settled.presentation.border.radius, Some(6));
    match settled.presentation.underlay.clone() {
        PresentationUnderlay::Found(found) => {
            assert_eq!(found.focused_window, WindowId("win-a".to_owned()));
            assert_eq!(found.members.len(), 2);
            let mut member_rects: Vec<Rect> =
                found.members.iter().map(|member| member.rect).collect();
            member_rects.sort_by_key(|frame| (frame.x, frame.y));
            let mut expected = vec![initial_a, initial_b];
            expected.sort_by_key(|frame| (frame.x, frame.y));
            assert_eq!(member_rects, expected);
            assert_eq!(found.union_rect, rect(8, 8, 784, 544));
            // gap 0 + width 3 + sentinel-width 3 = 6 at scale 1.
            assert_eq!(found.outer_rect, rect(2, 2, 796, 556));
        }
        hidden => panic!("expected found underlay, got {hidden:?}"),
    }

    // Logical shrink (a bounds change as a scale/mode reconfiguration can
    // produce, with no exact physical ratio proof): same scale 1, new
    // bounds. The reply reflows to the exact shrunk tile.
    let mut shrunk = screens_pair();
    shrunk[0].full_rect = Some(rect(0, 0, 640, 480));
    shrunk[0].work_area = Some(rect(0, 0, 640, 440));
    assert_eq!(
        shrunk[0].scale, 1.0,
        "shrunk topology still reports scale 1"
    );
    // work (0,0,640,440) insets to (8,8,624,424); (624-8)/2 = 308 per tile.
    let shrunk_a = rect(324, 8, 308, 424);
    let shrunk_b = rect(8, 8, 308, 424);
    let third = report_v4(
        3,
        shrunk.clone(),
        facts_pair(),
        "win-a",
        chord_ingress_v4("win-a", rect(0, 0, 40, 90), 3),
    );
    let reflow = accepted_v4(&mut engine, &third);
    assert_eq!(effective_frame(&reflow, "win-a"), shrunk_a);
    assert_eq!(effective_frame(&reflow, "win-b"), shrunk_b);

    // Settle the native facts to the reflowed plan so the found underlay
    // recomputes on truthful observations instead of stale pre-shrink ones.
    let fourth = report_v4(
        4,
        shrunk,
        vec![
            settled_fact("win-a", "out-a", shrunk_a),
            settled_fact("win-b", "out-a", shrunk_b),
            fact("win-c", "out-b", 800),
        ],
        "win-a",
        chord_ingress_v4("win-a", shrunk_a, 4),
    );
    let reconverged = accepted_v4(&mut engine, &fourth);
    assert_eq!(effective_frame(&reconverged, "win-a"), shrunk_a);
    assert_eq!(effective_frame(&reconverged, "win-b"), shrunk_b);
    assert!(reconverged.presentation.border.visible);
    assert_eq!(reconverged.presentation.border.outsets, Some([3, 3, 3, 3]));
    assert_eq!(reconverged.presentation.border.width, Some(3));
    assert_eq!(reconverged.presentation.border.radius, Some(6));
    match reconverged.presentation.underlay.clone() {
        PresentationUnderlay::Found(found) => {
            assert_eq!(found.focused_window, WindowId("win-a".to_owned()));
            assert_eq!(found.members.len(), 2);
            let mut member_rects: Vec<Rect> =
                found.members.iter().map(|member| member.rect).collect();
            member_rects.sort_by_key(|frame| (frame.x, frame.y));
            let mut expected = vec![shrunk_a, shrunk_b];
            expected.sort_by_key(|frame| (frame.x, frame.y));
            assert_eq!(member_rects, expected);
            assert_eq!(found.union_rect, rect(8, 8, 624, 424));
            assert_eq!(found.outer_rect, rect(2, 2, 636, 436));
        }
        hidden => panic!("expected recomputed found underlay, got {hidden:?}"),
    }
}

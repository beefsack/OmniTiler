//! Direct-state trailing-marker coverage: shared-union suppression,
//! occupied-tail suppression, bound/visible-tail marking, parity-final
//! marking, and the fail-closed gates, all over hand-built engine state
//! (mapping, staged homing/facts, views, native map) plus a minimal
//! report. No production or lifecycle paths run here.

use crate::directional::{OutputId, WindowId, WorkspaceId};
use crate::geometry::Rect;
use crate::session::DomainKey;
use crate::size_hints::WindowSizeHints;
use crate::workspace_scope::WorkspaceMode;
use crate::world_engine::WorldEngine;
use crate::world_report::{
    PresentationIngress, ScreenReport, WORLD_CONTRACT_VERSION, WindowFact, WorldReport,
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
        identity: crate::world_report::OutputIdentity::unknown(),
    }
}

fn fact(window: &str, output: &str, x: i32) -> WindowFact {
    WindowFact {
        window: WindowId(window.to_owned()),
        output: Some(OutputId(output.to_owned())),
        rect: Some(rect(x, 0, 40, 90)),
        fullscreen: Some(false),
        maximized: Some(false),
        sticky: Some(false),
        hints: Some(WindowSizeHints::none()),
        minimized: Some(false),
        native_workspace: crate::world_report::NativeWorkspaceFact::Omitted,
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
        executable: Some("synthetic.exe".to_owned()),
        process_id: Some("foreign-pid".to_owned()),
        class: Some("Synthetic".to_owned()),
        title: None,
        window_type: Some(crate::world_report::WindowType::Normal),
        activation_policy: Some(crate::world_report::ActivationPolicy::Regular),
        layer: Some(0),
        fullscreen_owner: None,
        fullscreen_exitable: None,
        suppression_available: None,
        visibility: crate::world_report::VisibilityFact {
            visible: Some(true),
            suppression_owned: None,
        },
        last_write: None,
    }
}

fn report(
    _mode: WorkspaceMode,
    screens: Vec<ScreenReport>,
    complete: bool,
    _equal_count: bool,
) -> WorldReport {
    WorldReport {
        contract_version: WORLD_CONTRACT_VERSION,
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        observation_seq: 1,
        request_seq: 1,
        request_id: "req-1".to_owned(),
        command_seq: None,
        command: None,
        screens,
        windows: Vec::new(),
        views: Vec::new(),
        closed: vec![],
        native: vec![],
        cursor: None,
        cursor_hit: None,
        focused_global: String::new(),
        action_foreground: None,
        action_cached: None,
        action_outputs: Vec::new(),
        primary_output: None,
        held_screens: Vec::new(),
        held_windows: Vec::new(),
        inventory_complete: complete,
        gesture_barrier: false,
        gesture: None,
        monotonic_ms: 0,
        frames: Vec::new(),
        member_attempts: Vec::new(),
        view_attempts: Vec::new(),
        clear_attempts: Vec::new(),
        creation_attempts: Vec::new(),
        presentation: PresentationIngress::disabled(),
    }
}

fn engine_with(rings: &[(&str, Vec<&str>)], host_owned: bool) -> WorldEngine {
    engine_with_mode(rings, WorkspaceMode::PerOutputLocal, false, host_owned)
}

fn engine_with_mode(
    rings: &[(&str, Vec<&str>)],
    mode: WorkspaceMode,
    equal_count: bool,
    host_owned: bool,
) -> WorldEngine {
    let capabilities = crate::connection::HostCapabilities {
        host_owned,
        equal_count,
        ..Default::default()
    };
    let mode_token = match mode {
        WorkspaceMode::PerOutputLocal => "per-output-local",
        WorkspaceMode::GlobalUnique => "global-unique",
        WorkspaceMode::Shared => "shared",
    };
    let mut engine = WorldEngine::new_test_bound_with_initial_settings(
        "gen-1",
        "adapter-1",
        capabilities,
        vec![(
            "mode".to_owned(),
            crate::connection::RawSettingValue::String(mode_token.to_owned()),
        )],
    )
    .expect("generation");
    for (output, ids) in rings {
        engine.mapping.insert(
            (*output).to_owned(),
            ids.iter().map(|id| (*id).to_owned()).collect(),
        );
    }
    engine
}

fn home(engine: &mut WorldEngine, window: &str, output: &str, workspace: &str) {
    let window_id = WindowId(window.to_owned());
    engine
        .staged
        .facts
        .insert(window_id.clone(), fact(window, output, 0));
    engine.staged.homing.insert(
        window_id,
        DomainKey {
            output: OutputId(output.to_owned()),
            workspace: WorkspaceId(workspace.to_owned()),
        },
    );
}

fn plan_marks(engine: &mut WorldEngine, report: &WorldReport, output: &str) -> Vec<(String, bool)> {
    engine
        .plan_workspaces(report, &[])
        .into_iter()
        .filter(|row| row.output.0 == output)
        .map(|row| (row.id.clone(), row.trailing))
        .collect()
}

fn pager_marks(report: &WorldReport, engine: &WorldEngine, output: &str) -> Vec<(String, bool)> {
    engine
        .presentation_for_publish(report)
        .pager
        .iter()
        .find(|screen| screen.output == output)
        .map(|screen| {
            screen
                .workspaces
                .iter()
                .map(|workspace| (workspace.id.clone(), workspace.trailing))
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn shared_union_occupied_elsewhere_suppresses_both_outputs() {
    let mut engine = engine_with_mode(
        &[("out-a", vec!["w1", "w2"]), ("out-b", vec!["w1", "w2"])],
        WorkspaceMode::Shared,
        false,
        false,
    );
    home(&mut engine, "win", "out-b", "w2");
    let report = report(
        WorkspaceMode::Shared,
        vec![screen("out-a", 0), screen("out-b", 800)],
        true,
        false,
    );
    assert_eq!(engine.trailing_marked_id(&report, "out-a"), None);
    assert_eq!(engine.trailing_marked_id(&report, "out-b"), None);
    assert!(
        plan_marks(&mut engine, &report, "out-a")
            .iter()
            .all(|(_, marked)| !marked)
    );
    assert!(
        plan_marks(&mut engine, &report, "out-b")
            .iter()
            .all(|(_, marked)| !marked)
    );
    assert!(
        pager_marks(&report, &engine, "out-a")
            .iter()
            .all(|(_, marked)| !marked)
    );
    assert!(
        pager_marks(&report, &engine, "out-b")
            .iter()
            .all(|(_, marked)| !marked)
    );
}

#[test]
fn occupied_tail_marks_nothing() {
    let mut engine = engine_with(&[("o", vec!["w1", "w2"])], false);
    home(&mut engine, "win", "o", "w2");
    let report = report(
        WorkspaceMode::PerOutputLocal,
        vec![screen("o", 0)],
        true,
        false,
    );
    assert_eq!(engine.trailing_marked_id(&report, "o"), None);
    assert!(
        plan_marks(&mut engine, &report, "o")
            .iter()
            .all(|(_, marked)| !marked)
    );
    assert!(
        pager_marks(&report, &engine, "o")
            .iter()
            .all(|(_, marked)| !marked)
    );
}

#[test]
fn bound_visible_tail_still_marks() {
    let mut engine = engine_with(&[("o", vec!["w1", "w2"])], true);
    engine.views.insert("o".to_owned(), "w2".to_owned());
    engine
        .native_map
        .insert(("o".to_owned(), "n1".to_owned()), "w2".to_owned());
    let report = report(
        WorkspaceMode::PerOutputLocal,
        vec![screen("o", 0)],
        true,
        false,
    );
    assert_eq!(
        engine.trailing_marked_id(&report, "o"),
        Some("w2".to_owned())
    );
    let bound = engine
        .plan_workspaces(&report, &[])
        .into_iter()
        .find(|row| row.output.0 == "o" && row.id == "w2")
        .expect("bound tail row");
    assert_eq!(bound.native_binding.as_deref(), Some("n1"));
    assert_eq!(
        plan_marks(&mut engine, &report, "o"),
        vec![("w1".to_owned(), false), ("w2".to_owned(), true)]
    );
    assert_eq!(
        pager_marks(&report, &engine, "o"),
        vec![("w1".to_owned(), false), ("w2".to_owned(), true)]
    );
}

#[test]
fn publication_final_only_over_unequal_rings() {
    let mut engine = engine_with(
        &[("a", vec!["w1", "w2", "w3"]), ("b", vec!["w1", "w2"])],
        false,
    );
    let report = report(
        WorkspaceMode::PerOutputLocal,
        vec![screen("a", 0), screen("b", 800)],
        true,
        true,
    );
    assert_eq!(
        engine.trailing_marked_id(&report, "a"),
        Some("w3".to_owned())
    );
    assert_eq!(
        engine.trailing_marked_id(&report, "b"),
        Some("w2".to_owned())
    );
    assert_eq!(
        plan_marks(&mut engine, &report, "a"),
        vec![
            ("w1".to_owned(), false),
            ("w2".to_owned(), false),
            ("w3".to_owned(), true)
        ]
    );
    assert_eq!(
        plan_marks(&mut engine, &report, "b"),
        vec![("w1".to_owned(), false), ("w2".to_owned(), true)]
    );
    assert_eq!(
        pager_marks(&report, &engine, "a"),
        vec![
            ("w1".to_owned(), false),
            ("w2".to_owned(), false),
            ("w3".to_owned(), true)
        ]
    );
    assert_eq!(
        pager_marks(&report, &engine, "b"),
        vec![("w1".to_owned(), false), ("w2".to_owned(), true)]
    );
}

#[test]
fn empty_tail_marks_the_final_spare_in_both_streams() {
    let mut engine = engine_with(&[("o", vec!["w1", "w2"])], false);
    let report = report(
        WorkspaceMode::PerOutputLocal,
        vec![screen("o", 0)],
        true,
        false,
    );
    assert_eq!(
        engine.trailing_marked_id(&report, "o"),
        Some("w2".to_owned())
    );
    assert_eq!(
        plan_marks(&mut engine, &report, "o"),
        vec![("w1".to_owned(), false), ("w2".to_owned(), true)]
    );
    assert_eq!(
        pager_marks(&report, &engine, "o"),
        vec![("w1".to_owned(), false), ("w2".to_owned(), true)]
    );
}

#[test]
fn incomplete_inventory_fails_closed_at_the_helper() {
    let mut engine = engine_with(&[("o", vec!["w1", "w2"])], false);
    let report = report(
        WorkspaceMode::PerOutputLocal,
        vec![screen("o", 0)],
        false,
        false,
    );
    assert_eq!(engine.trailing_marked_id(&report, "o"), None);
    assert!(
        plan_marks(&mut engine, &report, "o")
            .iter()
            .all(|(_, marked)| !marked)
    );
    assert!(
        pager_marks(&report, &engine, "o")
            .iter()
            .all(|(_, marked)| !marked)
    );
}

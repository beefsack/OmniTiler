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
        gap: 0,
        outer_gap: 0,
        complete: true,
        primary: x == 0,
    }
}

fn fact(window: &str, output: &str, x: i32) -> WindowFact {
    WindowFact {
        window: WindowId(window.to_owned()),
        output: OutputId(output.to_owned()),
        rect: Some(rect(x, 0, 40, 90)),
        floating: false,
        fit_excluded: false,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: WindowSizeHints::none(),
        minimized: false,
        observed_workspace: None,
        native_workspace: None,
    }
}

fn report(
    mode: WorkspaceMode,
    screens: Vec<ScreenReport>,
    complete: bool,
    equal_count: bool,
) -> WorldReport {
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
        workspace_mode: mode,
        screens,
        windows: Vec::new(),
        views: Vec::new(),
        closed: vec![],
        host_owned_workspace_domains: false,
        equal_workspace_count: equal_count,
        birth_clear: false,
        native: vec![],
        fixed_size_predicate: crate::size_hints::FixedSizePredicate::BothAxes,
        migration_source_refill: crate::output_selection::MigrationSourceRefill::LastRemaining,
        anchor_fallback: crate::admission::AnchorFallback::default(),
        no_space_result: crate::admission::NoSpaceResult::default(),
        switch_target: crate::cursor_targeting::SwitchTarget::default(),
        fresh_placement: crate::cursor_targeting::FreshPlacement::default(),
        focus_follows_cursor: false,
        cursor_warp: false,
        cursor: None,
        cursor_hit: None,
        focused_global: String::new(),
        action_foreground: None,
        action_cached: None,
        action_outputs: Vec::new(),
        default_tiled: true,
        born_maximized: crate::born_maximized::BornMaximized::default(),
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

fn engine_with(rings: &[(&str, Vec<&str>)]) -> WorldEngine {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
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
        .plan_workspaces(report)
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
    let mut engine = engine_with(&[("out-a", vec!["w1", "w2"]), ("out-b", vec!["w1", "w2"])]);
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
    let mut engine = engine_with(&[("o", vec!["w1", "w2"])]);
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
    let mut engine = engine_with(&[("o", vec!["w1", "w2"])]);
    engine.views.insert("o".to_owned(), "w2".to_owned());
    engine
        .native_map
        .insert(("o".to_owned(), "n1".to_owned()), "w2".to_owned());
    let mut report = report(
        WorkspaceMode::PerOutputLocal,
        vec![screen("o", 0)],
        true,
        false,
    );
    report.host_owned_workspace_domains = true;
    assert_eq!(
        engine.trailing_marked_id(&report, "o"),
        Some("w2".to_owned())
    );
    let bound = engine
        .plan_workspaces(&report)
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
    let mut engine = engine_with(&[("a", vec!["w1", "w2", "w3"]), ("b", vec!["w1", "w2"])]);
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
    let mut engine = engine_with(&[("o", vec!["w1", "w2"])]);
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
    let mut engine = engine_with(&[("o", vec!["w1", "w2"])]);
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

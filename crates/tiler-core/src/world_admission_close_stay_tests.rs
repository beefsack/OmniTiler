//! Admission focus, close-survivor focus, and send-stay source focus at the
//! world boundary: bootstrap seeds stay quiet, genuinely new lifetimes focus
//! at most once (reopen refocuses), overlays suppress automatic activation,
//! closes fall back to a surviving resident, view returns preserve the
//! remembered leaf, and stay sends pin exact source focus. Logical-first:
//! screens plus window facts only.

use crate::directional::{OutputId, WindowId};
use crate::geometry::Rect;
use crate::size_hints::WindowSizeHints;
use crate::world_engine::{StaySourceFocus, WorldEngine, WorldOutcome};
use crate::world_report::{
    ScreenReport, ScreenView, WORLD_CONTRACT_VERSION, WindowFact, WorldCommand, WorldReport,
};

fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

fn screen(output: &str, x: i32, primary: bool) -> ScreenReport {
    ScreenReport {
        output: OutputId(output.to_owned()),
        full_rect: Some(rect(x, 0, 800, 600)),
        work_area: Some(rect(x, 0, 800, 560)),
        gap: 0,
        outer_gap: 0,
        complete: true,
        primary,
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

fn overlay(window: &str, output: &str, x: i32, fullscreen: bool, maximized: bool) -> WindowFact {
    WindowFact {
        fullscreen,
        maximized,
        ..fact(window, output, x)
    }
}

#[allow(clippy::too_many_arguments)]
fn report(
    request_seq: u64,
    windows: Vec<WindowFact>,
    views: Vec<(&str, &str)>,
    closed: Vec<&str>,
    focused: &str,
    command_seq: Option<u64>,
    command: Option<WorldCommand>,
) -> WorldReport {
    WorldReport {
        contract_version: WORLD_CONTRACT_VERSION,
        owner: "owner-a".to_owned(),
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        observation_seq: request_seq,
        request_seq,
        request_id: format!("req-{request_seq}"),
        command_seq,
        command,
        workspace_mode: crate::workspace_scope::WorkspaceMode::PerOutputLocal,
        screens: vec![screen("out-a", 0, true), screen("out-b", 800, false)],
        windows,
        views: views
            .into_iter()
            .map(|(output, view)| ScreenView {
                output: OutputId(output.to_owned()),
                view: view.to_owned(),
            })
            .collect(),
        closed: closed
            .into_iter()
            .map(|id| WindowId(id.to_owned()))
            .collect(),
        host_owned_workspace_domains: false,
        equal_workspace_count: false,
        native: Vec::new(),
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
        default_tiled: true,
        focused_global: focused.to_owned(),
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

fn accepted(engine: &mut WorldEngine, report: &WorldReport) -> crate::world_report::WorldReply {
    match engine.handle_report(report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    }
}

fn view_of(reply: &crate::world_report::WorldReply, output: &str) -> String {
    reply
        .outputs
        .iter()
        .find(|row| row.output.0 == output)
        .and_then(|row| row.logical_view.clone())
        .expect("logical view")
}

/// Committed focus window for one logical workspace, via the retained
/// session snapshot (leaf identity to window link).
fn committed_focus(engine: &WorldEngine, output: &str, workspace: &str) -> Option<String> {
    let key = crate::session::DomainKey {
        output: OutputId(output.to_owned()),
        workspace: crate::directional::WorkspaceId(workspace.to_owned()),
    };
    let session = engine.engine().session(&key)?;
    let (domain, leaf) = session.focus();
    let (domain, leaf) = (domain?, leaf?);
    if domain != key {
        return None;
    }
    session
        .snapshot()
        .windows
        .iter()
        .find(|link| link.leaf == leaf)
        .map(|link| link.window.0.clone())
}

#[test]
fn bootstrap_seed_is_quiet_then_first_newcomer_focuses_once() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    // Fresh seeds emit nothing: the observed native focus stays authoritative.
    let first = accepted(
        &mut engine,
        &report(
            1,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "",
            None,
            None,
        ),
    );
    assert!(
        first.effects.is_empty(),
        "bootstrap quiet: {:?}",
        first.effects
    );
    let second = accepted(
        &mut engine,
        &report(
            2,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "",
            None,
            None,
        ),
    );
    assert!(second.effects.is_empty(), "re-observation quiet");
    // A genuinely new lifetime mints exactly one admission intent.
    let third = accepted(
        &mut engine,
        &report(
            3,
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 100),
                fact("win-c", "out-a", 200),
            ],
            vec![],
            vec![],
            "",
            None,
            None,
        ),
    );
    assert_eq!(third.effects.len(), 1, "{:?}", third.effects);
    assert_eq!(third.effects[0].target.0, "win-c");
    assert!(third.effects[0].id.starts_with("af-"));
    // Never replayed.
    let fourth = accepted(
        &mut engine,
        &report(
            4,
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 100),
                fact("win-c", "out-a", 200),
            ],
            vec![],
            vec![],
            "",
            None,
            None,
        ),
    );
    assert!(
        fourth.effects.is_empty(),
        "at-most-once: {:?}",
        fourth.effects
    );
}

#[test]
fn reopened_window_focuses_again_after_close() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    accepted(
        &mut engine,
        &report(
            1,
            vec![fact("win-a", "out-a", 0)],
            vec![],
            vec![],
            "",
            None,
            None,
        ),
    );
    let second = accepted(
        &mut engine,
        &report(
            2,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "",
            None,
            None,
        ),
    );
    assert_eq!(second.effects.len(), 1);
    // Explicit close spends the intent: no focus on the closing report.
    let third = accepted(
        &mut engine,
        &report(
            3,
            vec![fact("win-a", "out-a", 0)],
            vec![],
            vec!["win-b"],
            "",
            None,
            None,
        ),
    );
    assert!(third.effects.is_empty());
    let fourth = accepted(
        &mut engine,
        &report(
            4,
            vec![fact("win-a", "out-a", 0)],
            vec![],
            vec![],
            "",
            None,
            None,
        ),
    );
    assert!(fourth.effects.is_empty());
    // Re-adding the same id is a fresh newcomer again.
    let fifth = accepted(
        &mut engine,
        &report(
            5,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "",
            None,
            None,
        ),
    );
    assert_eq!(fifth.effects.len(), 1, "{:?}", fifth.effects);
    assert_eq!(fifth.effects[0].target.0, "win-b");
}

#[test]
fn newcomer_behind_fullscreen_spends_without_emitting() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    accepted(
        &mut engine,
        &report(
            1,
            vec![
                overlay("win-full", "out-a", 0, true, false),
                fact("win-a", "out-a", 100),
            ],
            vec![],
            vec![],
            "",
            None,
            None,
        ),
    );
    // A newcomer admitted behind a protected overlay never takes automatic
    // activation; the intent spends without emitting and never replays.
    let second = accepted(
        &mut engine,
        &report(
            2,
            vec![
                overlay("win-full", "out-a", 0, true, false),
                fact("win-a", "out-a", 100),
                fact("win-b", "out-a", 200),
            ],
            vec![],
            vec![],
            "",
            None,
            None,
        ),
    );
    assert!(
        second.effects.is_empty(),
        "suppressed: {:?}",
        second.effects
    );
    let third = accepted(
        &mut engine,
        &report(
            3,
            vec![
                overlay("win-full", "out-a", 0, true, false),
                fact("win-a", "out-a", 100),
                fact("win-b", "out-a", 200),
            ],
            vec![],
            vec![],
            "",
            None,
            None,
        ),
    );
    assert!(
        third.effects.is_empty(),
        "never retried: {:?}",
        third.effects
    );
}

#[test]
fn close_focused_holds_until_survivor_focus_recovers() {
    use crate::world_report::ScopeAuthority;
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    accepted(
        &mut engine,
        &report(
            1,
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 100),
                fact("win-c", "out-a", 200),
            ],
            vec![],
            vec![],
            "win-b",
            None,
            None,
        ),
    );
    let view_a = view_of(
        &accepted(
            &mut engine,
            &report(
                2,
                vec![
                    fact("win-a", "out-a", 0),
                    fact("win-b", "out-a", 100),
                    fact("win-c", "out-a", 200),
                ],
                vec![],
                vec![],
                "win-b",
                None,
                None,
            ),
        ),
        "out-a",
    );
    assert_eq!(
        committed_focus(&engine, "out-a", &view_a).as_deref(),
        Some("win-b")
    );
    // Closing the focused window with no observed focus holds the scope
    // narrowly: the core never invents activation. Survivors keep held
    // rows (no vanishing, no wedge).
    let held = accepted(
        &mut engine,
        &report(
            3,
            vec![fact("win-a", "out-a", 0), fact("win-c", "out-a", 200)],
            vec![],
            vec!["win-b"],
            "",
            None,
            None,
        ),
    );
    assert!(held.windows.iter().all(|row| row.lifetime.0 != "win-b"));
    let scope = held
        .scopes
        .iter()
        .find(|status| status.scope == format!("out-a/{view_a}"))
        .expect("scope row");
    assert_eq!(
        scope.authority,
        ScopeAuthority::Hold(crate::world_report::HoldReason::Unreadable)
    );
    for win in ["win-a", "win-c"] {
        assert!(
            held.windows.iter().any(|row| row.lifetime.0 == win),
            "survivor retained: {:?}",
            held.windows
        );
    }
    // The adapter reporting the survivor recovers to Controlled with
    // reflowed survivors and the survivor committed.
    let recovered = accepted(
        &mut engine,
        &report(
            4,
            vec![fact("win-a", "out-a", 0), fact("win-c", "out-a", 200)],
            vec![],
            vec![],
            "win-a",
            None,
            None,
        ),
    );
    let scope = recovered
        .scopes
        .iter()
        .find(|status| status.scope == format!("out-a/{view_a}"))
        .expect("scope row");
    assert_eq!(scope.authority, ScopeAuthority::Controlled);
    for win in ["win-a", "win-c"] {
        let row = recovered
            .windows
            .iter()
            .find(|row| row.lifetime.0 == win)
            .expect("row");
        assert!(row.allocation.is_some(), "{win} reflows: {row:?}");
    }
    assert_eq!(
        committed_focus(&engine, "out-a", &view_a).as_deref(),
        Some("win-a")
    );
    // Closing with the survivor observed focuses it directly.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    accepted(
        &mut engine,
        &report(
            1,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "win-b",
            None,
            None,
        ),
    );
    let view_a = view_of(
        &accepted(
            &mut engine,
            &report(
                2,
                vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
                vec![],
                vec![],
                "win-b",
                None,
                None,
            ),
        ),
        "out-a",
    );
    let direct = accepted(
        &mut engine,
        &report(
            3,
            vec![fact("win-a", "out-a", 0)],
            vec![],
            vec!["win-b"],
            "win-a",
            None,
            None,
        ),
    );
    let scope = direct
        .scopes
        .iter()
        .find(|status| status.scope == format!("out-a/{view_a}"))
        .expect("scope row");
    assert_eq!(scope.authority, ScopeAuthority::Controlled);
    assert_eq!(
        committed_focus(&engine, "out-a", &view_a).as_deref(),
        Some("win-a")
    );
}

#[test]
fn close_overlay_survivor_keeps_focus_and_truthful_rows() {
    for (name, fullscreen, maximized) in [("fullscreen", true, false), ("maximized", false, true)] {
        let mut engine = WorldEngine::new("gen-1").expect("generation");
        accepted(
            &mut engine,
            &report(
                1,
                vec![
                    fact("win-a", "out-a", 0),
                    overlay("win-o", "out-a", 100, fullscreen, maximized),
                ],
                vec![],
                vec![],
                "win-o",
                None,
                None,
            ),
        );
        let view_a = view_of(
            &accepted(
                &mut engine,
                &report(
                    2,
                    vec![
                        fact("win-a", "out-a", 0),
                        overlay("win-o", "out-a", 100, fullscreen, maximized),
                    ],
                    vec![],
                    vec![],
                    "win-o",
                    None,
                    None,
                ),
            ),
            "out-a",
        );
        // Closing the overlay while the survivor is observed recovers
        // directly with truthful tiled rows.
        let third = accepted(
            &mut engine,
            &report(
                3,
                vec![fact("win-a", "out-a", 0)],
                vec![],
                vec!["win-o"],
                "win-a",
                None,
                None,
            ),
        );
        assert!(
            third.windows.iter().all(|row| row.lifetime.0 != "win-o"),
            "{name}"
        );
        assert_eq!(
            committed_focus(&engine, "out-a", &view_a).as_deref(),
            Some("win-a"),
            "{name}"
        );
        let scope = third
            .scopes
            .iter()
            .find(|status| status.scope == format!("out-a/{view_a}"))
            .expect("scope row");
        assert_eq!(
            scope.authority,
            crate::world_report::ScopeAuthority::Controlled,
            "{name}"
        );
        let row = third
            .windows
            .iter()
            .find(|row| row.lifetime.0 == "win-a")
            .expect("row");
        assert!(row.allocation.is_some(), "{name}: {row:?}");
    }
}

#[test]
fn view_switch_return_preserves_remembered_focus() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "",
            None,
            None,
        ),
    );
    let view_a = view_of(&first, "out-a");
    let ring: Vec<String> = first
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    let other = ring
        .iter()
        .find(|id| *id != &view_a)
        .expect("spare")
        .clone();
    // Remember focus on win-b through the observed global focus.
    accepted(
        &mut engine,
        &report(
            2,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "win-b",
            None,
            None,
        ),
    );
    assert_eq!(
        committed_focus(&engine, "out-a", &view_a).as_deref(),
        Some("win-b")
    );
    // Switch away and back with truthful echoes: the leaf survives.
    accepted(
        &mut engine,
        &report(
            3,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "",
            Some(11),
            Some(WorldCommand::SelectWorkspace {
                output: OutputId("out-a".to_owned()),
                workspace: other.clone(),
            }),
        ),
    );
    accepted(
        &mut engine,
        &report(
            4,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![("out-a", other.as_str())],
            vec![],
            "",
            Some(12),
            Some(WorldCommand::SelectWorkspace {
                output: OutputId("out-a".to_owned()),
                workspace: view_a.clone(),
            }),
        ),
    );
    let calm = accepted(
        &mut engine,
        &report(
            5,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![("out-a", view_a.as_str())],
            vec![],
            "",
            None,
            None,
        ),
    );
    assert_eq!(view_of(&calm, "out-a"), view_a);
    assert_eq!(
        committed_focus(&engine, "out-a", &view_a).as_deref(),
        Some("win-b"),
        "return preserves the leaf"
    );
}

#[test]
fn numbered_select_return_emits_remembered_focus() {
    // R-WS-09 over the numbered path (the dogfood SelectNumbered route):
    // switching away stays quiet when the target holds no remembered
    // focus, returning emits one explicit at-most-once effect to the
    // remembered leaf. A redundant select on the shown view stays quiet.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "",
            None,
            None,
        ),
    );
    let view_a = view_of(&first, "out-a");
    let ring: Vec<String> = first
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    let other = ring
        .iter()
        .find(|id| *id != &view_a)
        .expect("spare")
        .clone();
    let index_of = |id: &str| -> u8 {
        u8::try_from(ring.iter().position(|row| row == id).expect("ring member") + 1)
            .expect("ordinal fits")
    };
    let other_index = index_of(&other);
    let view_index = index_of(&view_a);
    // Remember focus on win-b through the observed global focus. The
    // pre-switch departing foreground stays truthful in the pager feed:
    // the shown view carries the focus mark.
    let focused = accepted(
        &mut engine,
        &report(
            2,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "win-b",
            None,
            None,
        ),
    );
    assert_eq!(
        committed_focus(&engine, "out-a", &view_a).as_deref(),
        Some("win-b")
    );
    assert!(pager_contains_focused(&focused, "out-a", &view_a));
    // Switch away to the empty spare: no remembered focus there, so the
    // accepted reply carries admission-only effects (none here).
    let away = accepted(
        &mut engine,
        &report(
            3,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "",
            Some(11),
            Some(WorldCommand::SelectNumbered {
                output: Some(OutputId("out-a".to_owned())),
                index: other_index,
            }),
        ),
    );
    assert!(
        away.effects.is_empty(),
        "empty target stays quiet: {:?}",
        away.effects
    );
    assert_eq!(view_of(&away, "out-a"), other);
    // Return: one explicit focus effect to the remembered leaf.
    let back = accepted(
        &mut engine,
        &report(
            4,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![("out-a", other.as_str())],
            vec![],
            "",
            Some(12),
            Some(WorldCommand::SelectNumbered {
                output: Some(OutputId("out-a".to_owned())),
                index: view_index,
            }),
        ),
    );
    assert_eq!(view_of(&back, "out-a"), view_a);
    assert_eq!(back.effects.len(), 1, "{:?}", back.effects);
    assert_eq!(back.effects[0].target.0, "win-b");
    assert_eq!(back.effects[0].id, "fx-12");
    // Post-hide desktop observation: native focus is empty while the
    // returned view is current, so the pager marks no workspace focused.
    // The reply focus mark never invents the remembered leaf.
    assert!(!pager_contains_focused_any(&back, "out-a"));
    // Redundant select on the shown view stays quiet.
    let calm = accepted(
        &mut engine,
        &report(
            5,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![("out-a", view_a.as_str())],
            vec![],
            "",
            Some(13),
            Some(WorldCommand::SelectNumbered {
                output: Some(OutputId("out-a".to_owned())),
                index: view_index,
            }),
        ),
    );
    assert!(
        calm.effects.is_empty(),
        "redundant select stays quiet: {:?}",
        calm.effects
    );
}

#[test]
fn numbered_select_empty_and_minimized_targets_stay_quiet() {
    // R-CLOSE-03: selecting an empty workspace emits nothing. A minimized
    // remembered member never takes focus itself: the return follows the
    // updated remembered survivor, and a sole-minimized workspace (REQ-MNZ-03)
    // stays quiet.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "",
            None,
            None,
        ),
    );
    let view_a = view_of(&first, "out-a");
    let ring: Vec<String> = first
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    let other = ring
        .iter()
        .find(|id| *id != &view_a)
        .expect("spare")
        .clone();
    let index_of = |id: &str| -> u8 {
        u8::try_from(ring.iter().position(|row| row == id).expect("ring member") + 1)
            .expect("ordinal fits")
    };
    let other_index = index_of(&other);
    let view_index = index_of(&view_a);
    accepted(
        &mut engine,
        &report(
            2,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "win-b",
            None,
            None,
        ),
    );
    let away = accepted(
        &mut engine,
        &report(
            3,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "",
            Some(11),
            Some(WorldCommand::SelectNumbered {
                output: Some(OutputId("out-a".to_owned())),
                index: other_index,
            }),
        ),
    );
    assert!(away.effects.is_empty());
    // Minimize the remembered win-b; the return must not target the
    // minimized window. The updated remembered survivor win-a is the valid
    // focus target.
    let mut minimized_b = fact("win-b", "out-a", 100);
    minimized_b.minimized = true;
    minimized_b.rect = None;
    let back = accepted(
        &mut engine,
        &report(
            4,
            vec![fact("win-a", "out-a", 0), minimized_b],
            vec![("out-a", other.as_str())],
            vec![],
            "",
            Some(12),
            Some(WorldCommand::SelectNumbered {
                output: Some(OutputId("out-a".to_owned())),
                index: view_index,
            }),
        ),
    );
    assert_eq!(view_of(&back, "out-a"), view_a);
    assert_eq!(back.effects.len(), 1, "{:?}", back.effects);
    assert_eq!(back.effects[0].target.0, "win-a");
    // Sole-minimized workspace (REQ-MNZ-03): no tiled survivor exists, so
    // the return stays quiet.
    let mut sole = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut sole,
        &report(
            1,
            vec![fact("win-a", "out-a", 0)],
            vec![],
            vec![],
            "",
            None,
            None,
        ),
    );
    let view_a = view_of(&first, "out-a");
    let ring: Vec<String> = first
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    let other = ring
        .iter()
        .find(|id| *id != &view_a)
        .expect("spare")
        .clone();
    let index_of = |id: &str| -> u8 {
        u8::try_from(ring.iter().position(|row| row == id).expect("ring member") + 1)
            .expect("ordinal fits")
    };
    accepted(
        &mut sole,
        &report(
            2,
            vec![fact("win-a", "out-a", 0)],
            vec![],
            vec![],
            "win-a",
            None,
            None,
        ),
    );
    let mut minimized_a = fact("win-a", "out-a", 0);
    minimized_a.minimized = true;
    minimized_a.rect = None;
    accepted(
        &mut sole,
        &report(
            3,
            vec![minimized_a.clone()],
            vec![],
            vec![],
            "",
            Some(11),
            Some(WorldCommand::SelectNumbered {
                output: Some(OutputId("out-a".to_owned())),
                index: index_of(&other),
            }),
        ),
    );
    let back = accepted(
        &mut sole,
        &report(
            4,
            vec![minimized_a],
            vec![("out-a", other.as_str())],
            vec![],
            "",
            Some(12),
            Some(WorldCommand::SelectNumbered {
                output: Some(OutputId("out-a".to_owned())),
                index: index_of(&view_a),
            }),
        ),
    );
    assert_eq!(view_of(&back, "out-a"), view_a);
    assert!(
        back.effects.is_empty(),
        "sole-minimized stays quiet: {:?}",
        back.effects
    );
}

fn pager_contains_focused(
    reply: &crate::world_report::WorldReply,
    output: &str,
    workspace: &str,
) -> bool {
    reply
        .presentation
        .pager
        .iter()
        .find(|screen| screen.output == output)
        .is_some_and(|screen| {
            screen
                .workspaces
                .iter()
                .any(|row| row.id == workspace && row.contains_focused)
        })
}

fn pager_contains_focused_any(reply: &crate::world_report::WorldReply, output: &str) -> bool {
    reply
        .presentation
        .pager
        .iter()
        .find(|screen| screen.output == output)
        .is_some_and(|screen| screen.workspaces.iter().any(|row| row.contains_focused))
}

#[test]
fn numbered_select_occupied_return_both_directions_stale_then_desktop() {
    // Incident shape (occupied WS1 <-> occupied WS2, R-WS-09): the return
    // target must already hold a remembered leaf. A focused singleton
    // newly admitted on WS2 records the leaf; a second WS2 arrival
    // updates it. Returns in both directions emit the destination's
    // remembered window. The two reports ride the realistic observation
    // orderings: a stale departing foreground, then a post-hide desktop
    // (empty) read. Pager assertions expose the existing mixed-epoch feed:
    // a reply carries pre-switch focus on the departed view while logical
    // current has already moved. This diagnoses the reported stale mark;
    // it does not establish that presentation as the desired behavior.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "",
            None,
            None,
        ),
    );
    let view_a = view_of(&first, "out-a");
    let ring: Vec<String> = first
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    let view_b = ring
        .iter()
        .find(|id| *id != &view_a)
        .expect("spare")
        .clone();
    let index_of = |id: &str| -> u8 {
        u8::try_from(ring.iter().position(|row| row == id).expect("ring member") + 1)
            .expect("ordinal fits")
    };
    let index_a = index_of(&view_a);
    let index_b = index_of(&view_b);
    // Remember win-b on WS1 through the observed global focus.
    accepted(
        &mut engine,
        &report(
            2,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "win-b",
            None,
            None,
        ),
    );
    assert_eq!(
        committed_focus(&engine, "out-a", &view_a).as_deref(),
        Some("win-b")
    );
    // Occupy WS2: select the spare, then admit a focused singleton.
    let away = accepted(
        &mut engine,
        &report(
            3,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "",
            Some(11),
            Some(WorldCommand::SelectNumbered {
                output: Some(OutputId("out-a".to_owned())),
                index: index_b,
            }),
        ),
    );
    assert!(away.effects.is_empty());
    assert_eq!(view_of(&away, "out-a"), view_b);
    let admitted = accepted(
        &mut engine,
        &report(
            4,
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 100),
                fact("win-c", "out-a", 200),
            ],
            vec![("out-a", view_b.as_str())],
            vec![],
            "win-c",
            None,
            None,
        ),
    );
    assert_eq!(view_of(&admitted, "out-a"), view_b);
    // The focused singleton newly admitted on WS2 records the leaf.
    assert_eq!(
        committed_focus(&engine, "out-a", &view_b).as_deref(),
        Some("win-c")
    );
    // A second WS2 arrival, focused, updates the leaf.
    accepted(
        &mut engine,
        &report(
            5,
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 100),
                fact("win-c", "out-a", 200),
                fact("win-d", "out-a", 300),
            ],
            vec![("out-a", view_b.as_str())],
            vec![],
            "win-d",
            None,
            None,
        ),
    );
    assert_eq!(
        committed_focus(&engine, "out-a", &view_b).as_deref(),
        Some("win-d")
    );
    // Return to WS1 under a stale departing foreground: native still
    // reports win-d focused although the view moves now.
    let back_a = accepted(
        &mut engine,
        &report(
            6,
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 100),
                fact("win-c", "out-a", 200),
                fact("win-d", "out-a", 300),
            ],
            vec![("out-a", view_b.as_str())],
            vec![],
            "win-d",
            Some(12),
            Some(WorldCommand::SelectNumbered {
                output: Some(OutputId("out-a".to_owned())),
                index: index_a,
            }),
        ),
    );
    assert_eq!(view_of(&back_a, "out-a"), view_a);
    assert_eq!(back_a.effects.len(), 1, "{:?}", back_a.effects);
    assert_eq!(back_a.effects[0].target.0, "win-b");
    assert_eq!(back_a.effects[0].id, "fx-12");
    assert_eq!(
        committed_focus(&engine, "out-a", &view_a).as_deref(),
        Some("win-b")
    );
    assert_eq!(
        committed_focus(&engine, "out-a", &view_b).as_deref(),
        Some("win-d")
    );
    // Stale departing foreground: native still reports win-d focused
    // although the view moves now. The departed WS2 never keeps the focus
    // mark and the not-yet-observed WS1 carries none either: no workspace
    // is marked until native observation catches up.
    assert!(!pager_contains_focused(&back_a, "out-a", &view_a));
    assert!(!pager_contains_focused(&back_a, "out-a", &view_b));
    assert!(!pager_contains_focused_any(&back_a, "out-a"));
    // Post-hide desktop observation: native focus is empty while WS1 is
    // current, so no workspace carries a focus mark.
    let calm_a = accepted(
        &mut engine,
        &report(
            7,
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 100),
                fact("win-c", "out-a", 200),
                fact("win-d", "out-a", 300),
            ],
            vec![("out-a", view_a.as_str())],
            vec![],
            "",
            None,
            None,
        ),
    );
    assert_eq!(view_of(&calm_a, "out-a"), view_a);
    assert!(!pager_contains_focused_any(&calm_a, "out-a"));
    // Native observation catches up: the current-workspace focus win-b is
    // genuinely observed, so the returned-to WS1 carries the mark and the
    // departed WS2 carries none. This is observed focus, never the
    // remembered desire painted early.
    let caught_up = accepted(
        &mut engine,
        &report(
            8,
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 100),
                fact("win-c", "out-a", 200),
                fact("win-d", "out-a", 300),
            ],
            vec![("out-a", view_a.as_str())],
            vec![],
            "win-b",
            None,
            None,
        ),
    );
    assert_eq!(view_of(&caught_up, "out-a"), view_a);
    assert!(pager_contains_focused(&caught_up, "out-a", &view_a));
    assert!(!pager_contains_focused(&caught_up, "out-a", &view_b));
    // Foreign focus (unmanaged lifetime) marks nothing.
    let foreign = accepted(
        &mut engine,
        &report(
            9,
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 100),
                fact("win-c", "out-a", 200),
                fact("win-d", "out-a", 300),
            ],
            vec![("out-a", view_a.as_str())],
            vec![],
            "ghost",
            None,
            None,
        ),
    );
    assert_eq!(view_of(&foreign, "out-a"), view_a);
    assert!(!pager_contains_focused_any(&foreign, "out-a"));
    // Return to WS2 under the desktop observation: the remembered win-d
    // is still the emitted target.
    let back_b = accepted(
        &mut engine,
        &report(
            10,
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 100),
                fact("win-c", "out-a", 200),
                fact("win-d", "out-a", 300),
            ],
            vec![("out-a", view_a.as_str())],
            vec![],
            "",
            Some(13),
            Some(WorldCommand::SelectNumbered {
                output: Some(OutputId("out-a".to_owned())),
                index: index_b,
            }),
        ),
    );
    assert_eq!(view_of(&back_b, "out-a"), view_b);
    assert_eq!(back_b.effects.len(), 1, "{:?}", back_b.effects);
    assert_eq!(back_b.effects[0].target.0, "win-d");
    assert_eq!(back_b.effects[0].id, "fx-13");
    assert!(!pager_contains_focused_any(&back_b, "out-a"));
    // Memory on both workspaces survives the roundtrip.
    assert_eq!(
        committed_focus(&engine, "out-a", &view_a).as_deref(),
        Some("win-b")
    );
    assert_eq!(
        committed_focus(&engine, "out-a", &view_b).as_deref(),
        Some("win-d")
    );
}

#[test]
fn stay_send_pins_exact_source_focus_or_empty_or_unknown() {
    // Survivor with the mover focused: the collapsed single-leaf tree
    // cannot name the pre-send stack, so the sole survivor is exact by
    // elimination.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "",
            None,
            None,
        ),
    );
    let view_a = view_of(&first, "out-a");
    let source = crate::session::DomainKey {
        output: OutputId("out-a".to_owned()),
        workspace: crate::directional::WorkspaceId(view_a.clone()),
    };
    let reply = accepted(
        &mut engine,
        &report(
            2,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "win-a",
            Some(11),
            Some(WorldCommand::SendDirectional {
                window: WindowId("win-a".to_owned()),
                direction: "right".to_owned(),
                follow: false,
            }),
        ),
    );
    assert_eq!(reply.transfers.len(), 1);
    assert_eq!(reply.transfers[0].operation_id, "op-11");
    // The stay names the engine's source-MRU survivor as an explicit focus
    // outcome; an emptied source names nothing.
    assert_eq!(reply.effects.len(), 1, "{:?}", reply.effects);
    assert_eq!(reply.effects[0].target.0, "win-b");
    assert!(reply.effects[0].is_explicit_focus_for(11));
    match engine.stay_source_focus("op-11", &source) {
        StaySourceFocus::Resident(window) => assert_eq!(window.0, "win-b"),
        other => panic!("sole survivor exact, got {other:?}"),
    }
    // Emptied source proves empty (never unknown): the session retired.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            vec![fact("win-a", "out-a", 0)],
            vec![],
            vec![],
            "",
            None,
            None,
        ),
    );
    let view_a = view_of(&first, "out-a");
    let source = crate::session::DomainKey {
        output: OutputId("out-a".to_owned()),
        workspace: crate::directional::WorkspaceId(view_a),
    };
    let reply = accepted(
        &mut engine,
        &report(
            2,
            vec![fact("win-a", "out-a", 0)],
            vec![],
            vec![],
            "win-a",
            Some(11),
            Some(WorldCommand::SendDirectional {
                window: WindowId("win-a".to_owned()),
                direction: "right".to_owned(),
                follow: false,
            }),
        ),
    );
    assert_eq!(reply.transfers.len(), 1);
    assert_eq!(
        engine.stay_source_focus("op-11", &source),
        StaySourceFocus::Empty,
        "retired source is proven empty"
    );
    // Follow transfers never pin source focus. The mover must be the
    // observed focus to send at all; the follow still pins nothing.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "",
            None,
            None,
        ),
    );
    let view_a = view_of(&first, "out-a");
    let source = crate::session::DomainKey {
        output: OutputId("out-a".to_owned()),
        workspace: crate::directional::WorkspaceId(view_a),
    };
    accepted(
        &mut engine,
        &report(
            2,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "win-a",
            Some(11),
            Some(WorldCommand::SendDirectional {
                window: WindowId("win-a".to_owned()),
                direction: "right".to_owned(),
                follow: true,
            }),
        ),
    );
    assert_eq!(
        engine.stay_source_focus("op-11", &source),
        StaySourceFocus::Unknown,
        "follow never pins"
    );
}

#[test]
fn stay_hidden_mover_scope_converges_without_staged_focus_or_close() {
    use crate::world_report::{ScopeAuthority, TransferKind};
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "",
            None,
            None,
        ),
    );
    let view_a = view_of(&first, "out-a");
    // Stay the focused mover to a fresh trailing workspace: the target
    // session is born focusless (stay keeps the source selected).
    let stay = accepted(
        &mut engine,
        &report(
            2,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "win-b",
            Some(11),
            Some(WorldCommand::SendNumbered {
                window: WindowId("win-b".to_owned()),
                source: crate::world_report::SendSource::Explicit(OutputId("out-a".to_owned())),
                index: 0,
                follow: false,
            }),
        ),
    );
    let target = stay
        .transfers
        .iter()
        .find_map(|t| match &t.kind {
            TransferKind::Window(w) if w.0 == "win-b" => Some(t.target_workspace.clone()),
            _ => None,
        })
        .expect("stay transfer");
    assert_ne!(target, view_a);
    // Next observation: the mover rides its snapshot rect (hidden
    // adapter-side, an ordinary readable fact here) with no staged focus
    // on the fresh domain and no close pending. The scope must converge
    // instead of wedging on focus-mismatch: no report can resolve the
    // focus of a hidden sole member, and the hold would block the reveal
    // that makes it focusable.
    let settled = accepted(
        &mut engine,
        &report(
            3,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "win-a",
            None,
            None,
        ),
    );
    let scope = settled
        .scopes
        .iter()
        .find(|status| status.scope == format!("out-a/{target}"))
        .expect("stay scope row");
    assert_eq!(scope.authority, ScopeAuthority::Controlled);
    assert!(
        settled.windows.iter().any(|row| row.lifetime.0 == "win-b"),
        "mover row retained: {:?}",
        settled.windows
    );
}

#[test]
fn select_hidden_stay_arrival_without_history_focuses_first_leaf() {
    // Hidden arrival with no focus history: a stay-send parks win-b on
    // the hidden spare without touching its focus memory. Selecting the
    // spare must focus the first (only) layout leaf instead of staying
    // quiet; an empty select stays quiet (R-CLOSE-03).
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "",
            None,
            None,
        ),
    );
    let view_a = view_of(&first, "out-a");
    let ring: Vec<String> = first
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    let other = ring
        .iter()
        .find(|id| *id != &view_a)
        .expect("spare")
        .clone();
    let index_of = |id: &str| -> u8 {
        u8::try_from(ring.iter().position(|row| row == id).expect("ring member") + 1)
            .expect("ordinal fits")
    };
    // Stay-send the focused win-b to the hidden spare.
    accepted(
        &mut engine,
        &report(
            2,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "win-b",
            Some(11),
            Some(WorldCommand::Send {
                window: WindowId("win-b".to_owned()),
                target_output: OutputId("out-a".to_owned()),
                target_workspace: other.clone(),
                follow: false,
            }),
        ),
    );
    // Membership receipt settles the stay flight; the spare session is
    // born focusless (stay keeps the source selected).
    let mut arrived_b = fact("win-b", "out-a", 100);
    arrived_b.observed_workspace = Some(other.clone());
    let settled = accepted(
        &mut engine,
        &report(
            3,
            vec![fact("win-a", "out-a", 0), arrived_b],
            vec![("out-a", view_a.as_str())],
            vec![],
            "win-a",
            None,
            None,
        ),
    );
    assert!(
        settled.transfers.is_empty(),
        "stay flight settles: {:?}",
        settled.transfers
    );
    assert_eq!(
        committed_focus(&engine, "out-a", &other),
        None,
        "hidden arrival records no focus memory"
    );
    // Selecting the arrival workspace focuses its first layout leaf.
    let shown = accepted(
        &mut engine,
        &report(
            4,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![("out-a", view_a.as_str())],
            vec![],
            "win-a",
            Some(12),
            Some(WorldCommand::SelectNumbered {
                output: Some(OutputId("out-a".to_owned())),
                index: index_of(&other),
            }),
        ),
    );
    assert_eq!(view_of(&shown, "out-a"), other);
    assert_eq!(shown.effects.len(), 1, "{:?}", shown.effects);
    assert_eq!(shown.effects[0].target.0, "win-b");
    assert_eq!(shown.effects[0].id, "fx-12");
}

#[test]
fn select_remembered_sticky_skips_to_eligible_later() {
    // Ineligible top candidate falls through: win-b is the remembered
    // leaf on WS1, but its staged row turns sticky while WS1 is hidden.
    // Returning must skip win-b and focus eligible win-a instead of
    // staying quiet.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "",
            None,
            None,
        ),
    );
    let view_a = view_of(&first, "out-a");
    let ring: Vec<String> = first
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    let other = ring
        .iter()
        .find(|id| *id != &view_a)
        .expect("spare")
        .clone();
    let index_of = |id: &str| -> u8 {
        u8::try_from(ring.iter().position(|row| row == id).expect("ring member") + 1)
            .expect("ordinal fits")
    };
    // Remember win-b through the observed global focus.
    accepted(
        &mut engine,
        &report(
            2,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "win-b",
            None,
            None,
        ),
    );
    // Switch away to the empty spare: no remembered focus there.
    let away = accepted(
        &mut engine,
        &report(
            3,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "",
            Some(11),
            Some(WorldCommand::SelectNumbered {
                output: Some(OutputId("out-a".to_owned())),
                index: index_of(&other),
            }),
        ),
    );
    assert!(away.effects.is_empty());
    // win-b turns sticky natively while hidden; it stays a linked tiled
    // member, so the return must skip it rather than lose focus.
    let mut sticky_b = fact("win-b", "out-a", 100);
    sticky_b.sticky = true;
    let back = accepted(
        &mut engine,
        &report(
            4,
            vec![fact("win-a", "out-a", 0), sticky_b],
            vec![("out-a", other.as_str())],
            vec![],
            "",
            Some(12),
            Some(WorldCommand::SelectNumbered {
                output: Some(OutputId("out-a".to_owned())),
                index: index_of(&view_a),
            }),
        ),
    );
    assert_eq!(view_of(&back, "out-a"), view_a);
    assert_eq!(back.effects.len(), 1, "{:?}", back.effects);
    assert_eq!(back.effects[0].target.0, "win-a");
    assert_eq!(back.effects[0].id, "fx-12");
}

#[test]
fn select_first_layout_sticky_skips_to_later_arrival() {
    // Hidden stay-arrivals carry no focus memory; the first layout leaf
    // (win-b, admitted first) turns sticky before the reveal. Selecting
    // the spare must skip win-b and focus win-a.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "",
            None,
            None,
        ),
    );
    let view_a = view_of(&first, "out-a");
    let ring: Vec<String> = first
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    let other = ring
        .iter()
        .find(|id| *id != &view_a)
        .expect("spare")
        .clone();
    let index_of = |id: &str| -> u8 {
        u8::try_from(ring.iter().position(|row| row == id).expect("ring member") + 1)
            .expect("ordinal fits")
    };
    // Stay-send win-b to the hidden spare, then settle the flight.
    accepted(
        &mut engine,
        &report(
            2,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "win-b",
            Some(11),
            Some(WorldCommand::Send {
                window: WindowId("win-b".to_owned()),
                target_output: OutputId("out-a".to_owned()),
                target_workspace: other.clone(),
                follow: false,
            }),
        ),
    );
    let mut arrived_b = fact("win-b", "out-a", 100);
    arrived_b.observed_workspace = Some(other.clone());
    let settled = accepted(
        &mut engine,
        &report(
            3,
            vec![fact("win-a", "out-a", 0), arrived_b],
            vec![("out-a", view_a.as_str())],
            vec![],
            "win-a",
            None,
            None,
        ),
    );
    assert!(
        settled.transfers.is_empty(),
        "stay flight settles: {:?}",
        settled.transfers
    );
    // Stay-send win-a to the same hidden spare, then settle with win-b
    // sticky. Both arrivals stay focusless (stay keeps source selected).
    accepted(
        &mut engine,
        &report(
            4,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![("out-a", view_a.as_str())],
            vec![],
            "win-a",
            Some(12),
            Some(WorldCommand::Send {
                window: WindowId("win-a".to_owned()),
                target_output: OutputId("out-a".to_owned()),
                target_workspace: other.clone(),
                follow: false,
            }),
        ),
    );
    let mut sticky_b = fact("win-b", "out-a", 100);
    sticky_b.observed_workspace = Some(other.clone());
    sticky_b.sticky = true;
    let mut arrived_a = fact("win-a", "out-a", 0);
    arrived_a.observed_workspace = Some(other.clone());
    let settled = accepted(
        &mut engine,
        &report(
            5,
            vec![sticky_b, arrived_a],
            vec![("out-a", view_a.as_str())],
            vec![],
            "",
            None,
            None,
        ),
    );
    assert!(
        settled.transfers.is_empty(),
        "second stay flight settles: {:?}",
        settled.transfers
    );
    assert_eq!(
        first_leaf_window(&engine, "out-a", &other).as_deref(),
        Some("win-b"),
        "layout order starts at the first arrival"
    );
    // Selecting the arrival workspace skips sticky win-b for win-a.
    let mut sticky_select = fact("win-b", "out-a", 100);
    sticky_select.sticky = true;
    let shown = accepted(
        &mut engine,
        &report(
            6,
            vec![sticky_select, fact("win-a", "out-a", 0)],
            vec![("out-a", view_a.as_str())],
            vec![],
            "",
            Some(13),
            Some(WorldCommand::SelectNumbered {
                output: Some(OutputId("out-a".to_owned())),
                index: index_of(&other),
            }),
        ),
    );
    assert_eq!(view_of(&shown, "out-a"), other);
    assert_eq!(shown.effects.len(), 1, "{:?}", shown.effects);
    assert_eq!(shown.effects[0].target.0, "win-a");
    assert_eq!(shown.effects[0].id, "fx-13");
}

#[test]
fn select_held_scope_stays_quiet_despite_eligible() {
    // Held scopes never take view-return focus: WS1 carries seeded memory
    // for win-a, but a bounds change under the gesture barrier holds the
    // scope, so returning moves the view with no focus effect.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "",
            None,
            None,
        ),
    );
    let view_a = view_of(&first, "out-a");
    let ring: Vec<String> = first
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    let other = ring
        .iter()
        .find(|id| *id != &view_a)
        .expect("spare")
        .clone();
    let index_of = |id: &str| -> u8 {
        u8::try_from(ring.iter().position(|row| row == id).expect("ring member") + 1)
            .expect("ordinal fits")
    };
    let away = accepted(
        &mut engine,
        &report(
            2,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "",
            Some(11),
            Some(WorldCommand::SelectNumbered {
                output: Some(OutputId("out-a".to_owned())),
                index: index_of(&other),
            }),
        ),
    );
    assert!(away.effects.is_empty());
    // Same geometry with a changed gap under the barrier: the scope
    // holds, so the return moves the view with no focus effect.
    let mut held = report(
        3,
        vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
        vec![("out-a", other.as_str())],
        vec![],
        "",
        Some(12),
        Some(WorldCommand::SelectNumbered {
            output: Some(OutputId("out-a".to_owned())),
            index: index_of(&view_a),
        }),
    );
    held.gesture_barrier = true;
    for screen in &mut held.screens {
        screen.gap = 8;
    }
    let back = accepted(&mut engine, &held);
    assert_eq!(view_of(&back, "out-a"), view_a);
    assert!(
        back.effects.is_empty(),
        "held scope stays quiet: {:?}",
        back.effects
    );
}

#[test]
fn select_without_native_focus_returns_recorded_leaf() {
    // Seeded memory without native focus: WS1 is bootstrapped without
    // native focus yet carries seeded memory for win-a, and the spare's
    // unfocused admissions record newcomer memory for win-c. Returns name
    // the recorded leaves; the MRU-vs-first discrimination lives in the
    // session chain test (world seeds always record memory here).
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "",
            None,
            None,
        ),
    );
    let view_a = view_of(&first, "out-a");
    let ring: Vec<String> = first
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    let other = ring
        .iter()
        .find(|id| *id != &view_a)
        .expect("spare")
        .clone();
    let index_of = |id: &str| -> u8 {
        u8::try_from(ring.iter().position(|row| row == id).expect("ring member") + 1)
            .expect("ordinal fits")
    };
    // Occupy the spare; the empty select stays quiet (R-CLOSE-03).
    let away = accepted(
        &mut engine,
        &report(
            2,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "",
            Some(11),
            Some(WorldCommand::SelectNumbered {
                output: Some(OutputId("out-a".to_owned())),
                index: index_of(&other),
            }),
        ),
    );
    assert!(away.effects.is_empty());
    assert_eq!(view_of(&away, "out-a"), other);
    // Two unfocused arrivals while the spare is shown; newcomer memory
    // records the latest arrival.
    for seq in [3u64, 4u64] {
        let mut windows = vec![
            fact("win-a", "out-a", 0),
            fact("win-b", "out-a", 100),
            fact("win-c", "out-a", 200),
        ];
        if seq == 4 {
            windows.push(fact("win-d", "out-a", 300));
        }
        accepted(
            &mut engine,
            &report(
                seq,
                windows,
                vec![("out-a", other.as_str())],
                vec![],
                "",
                None,
                None,
            ),
        );
    }
    assert_eq!(
        committed_focus(&engine, "out-a", &other).as_deref(),
        Some("win-c"),
        "newcomer memory records the first arrival"
    );
    assert_eq!(
        first_leaf_window(&engine, "out-a", &other).as_deref(),
        Some("win-c"),
        "layout order starts at the first admission"
    );
    // Probe: what focus memory does the never-natively-focused WS1 carry?
    // (Bootstrap seeds win-a; the return below guards the recorded leaf.)
    assert_eq!(
        committed_focus(&engine, "out-a", &view_a).as_deref(),
        Some("win-a")
    );
    // Return to WS1 names the seeded win-a (first layout leaf here is
    // win-b, so this guards the recorded leaf, not the layout order).
    let back = accepted(
        &mut engine,
        &report(
            5,
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 100),
                fact("win-c", "out-a", 200),
                fact("win-d", "out-a", 300),
            ],
            vec![("out-a", other.as_str())],
            vec![],
            "",
            Some(12),
            Some(WorldCommand::SelectNumbered {
                output: Some(OutputId("out-a".to_owned())),
                index: index_of(&view_a),
            }),
        ),
    );
    assert_eq!(view_of(&back, "out-a"), view_a);
    assert_eq!(back.effects.len(), 1, "{:?}", back.effects);
    assert_eq!(back.effects[0].id, "fx-12", "{:?}", back.effects);
    assert_eq!(back.effects[0].target.0, "win-a", "{:?}", back.effects);
    // And back again: the spare return still names remembered win-c.
    let back_other = accepted(
        &mut engine,
        &report(
            6,
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 100),
                fact("win-c", "out-a", 200),
                fact("win-d", "out-a", 300),
            ],
            vec![("out-a", view_a.as_str())],
            vec![],
            "",
            Some(13),
            Some(WorldCommand::SelectNumbered {
                output: Some(OutputId("out-a".to_owned())),
                index: index_of(&other),
            }),
        ),
    );
    assert_eq!(view_of(&back_other, "out-a"), other);
    assert_eq!(back_other.effects.len(), 1, "{:?}", back_other.effects);
    assert_eq!(back_other.effects[0].target.0, "win-c");
    assert_eq!(back_other.effects[0].id, "fx-13");
}

/// First layout leaf window for one workspace domain, in deterministic
/// child order (tentative no-history tie-break).
fn first_leaf_window(engine: &WorldEngine, output: &str, workspace: &str) -> Option<String> {
    let key = crate::session::DomainKey {
        output: OutputId(output.to_owned()),
        workspace: crate::directional::WorkspaceId(workspace.to_owned()),
    };
    let session = engine.engine().session(&key)?;
    let mut node = session.tree_for(&key)?;
    loop {
        match node {
            crate::directional::Node::Leaf { id } => {
                return session
                    .snapshot()
                    .windows
                    .iter()
                    .find(|link| &link.leaf == id)
                    .map(|link| link.window.0.clone());
            }
            crate::directional::Node::Group { children, .. } => {
                node = children.first()?;
            }
        }
    }
}

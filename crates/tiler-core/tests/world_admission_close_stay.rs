//! Admission focus, close-survivor focus, and send-stay source focus at the
//! world boundary: bootstrap seeds stay quiet, genuinely new lifetimes focus
//! at most once (reopen refocuses), overlays suppress automatic activation,
//! closes fall back to a surviving resident, view returns preserve the
//! remembered leaf, and stay sends pin exact source focus. Logical-first:
//! screens plus window facts only.

use tiler_core::directional::{OutputId, WindowId};
use tiler_core::geometry::Rect;
use tiler_core::size_hints::WindowSizeHints;
use tiler_core::world_engine::{StaySourceFocus, WorldEngine, WorldOutcome};
use tiler_core::world_report::{
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
        restored: None,
        contract_version: WORLD_CONTRACT_VERSION,
        owner: "owner-a".to_owned(),
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        observation_seq: request_seq,
        request_seq,
        request_id: format!("req-{request_seq}"),
        command_seq,
        command,
        workspace_mode: tiler_core::workspace_scope::WorkspaceMode::PerOutputLocal,
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
        fixed_size_predicate: tiler_core::size_hints::FixedSizePredicate::BothAxes,
        migration_source_refill: tiler_core::output_selection::MigrationSourceRefill::LastRemaining,
        anchor_fallback: tiler_core::admission::AnchorFallback::default(),
        no_space_result: tiler_core::admission::NoSpaceResult::default(),
        default_tiled: true,
        focused_global: focused.to_owned(),
        inventory_complete: true,
        gesture_barrier: false,
        feedback: Vec::new(),
    }
}

fn accepted(
    engine: &mut WorldEngine,
    report: &WorldReport,
) -> tiler_core::world_report::WorldReply {
    match engine.handle_report(report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    }
}

fn view_of(reply: &tiler_core::world_report::WorldReply, output: &str) -> String {
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
    let key = tiler_core::session::DomainKey {
        output: OutputId(output.to_owned()),
        workspace: tiler_core::directional::WorkspaceId(workspace.to_owned()),
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
    use tiler_core::world_report::ScopeAuthority;
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
        ScopeAuthority::Hold(tiler_core::world_report::HoldReason::Unreadable)
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
            tiler_core::world_report::ScopeAuthority::Controlled,
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
    let source = tiler_core::session::DomainKey {
        output: OutputId("out-a".to_owned()),
        workspace: tiler_core::directional::WorkspaceId(view_a.clone()),
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
    let source = tiler_core::session::DomainKey {
        output: OutputId("out-a".to_owned()),
        workspace: tiler_core::directional::WorkspaceId(view_a),
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
    let source = tiler_core::session::DomainKey {
        output: OutputId("out-a".to_owned()),
        workspace: tiler_core::directional::WorkspaceId(view_a),
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

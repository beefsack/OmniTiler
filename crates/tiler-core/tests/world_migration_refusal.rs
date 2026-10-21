//! Migration refusal rollback: wholly-unarrived refusal restores the
//! retained logical unit (ring position, views, history, membership) and
//! relocates the Engine session back so tree/shares/focus survive modulo
//! homing. Partial refusal holds persistently with identities preserved.

use tiler_core::directional::{OutputId, WindowId};
use tiler_core::geometry::Rect;
use tiler_core::size_hints::WindowSizeHints;
use tiler_core::world_engine::{WorldEngine, WorldOutcome};
use tiler_core::world_report::{
    AttemptFeedback, AttemptKind, AttemptOutcome, ScopeAuthority, ScreenReport, TransferKind,
    WORLD_CONTRACT_VERSION, WindowFact, WorldCommand, WorldReport,
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

#[allow(clippy::too_many_arguments)]
fn report(
    request_seq: u64,
    observation_seq: u64,
    windows: Vec<WindowFact>,
    focused: &str,
    command_seq: Option<u64>,
    command: Option<WorldCommand>,
    feedback: Vec<AttemptFeedback>,
) -> WorldReport {
    WorldReport {
        restored: None,
        contract_version: WORLD_CONTRACT_VERSION,
        owner: "owner-a".to_owned(),
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        observation_seq,
        request_seq,
        request_id: format!("req-{request_seq}"),
        command_seq,
        command,
        workspace_mode: tiler_core::workspace_scope::WorkspaceMode::PerOutputLocal,
        screens: vec![screen("out-a", 0, true), screen("out-b", 800, false)],
        windows,
        views: vec![],
        closed: vec![],
        host_owned_workspace_domains: false,
        equal_workspace_count: false,
        native: vec![],
        fixed_size_predicate: tiler_core::size_hints::FixedSizePredicate::BothAxes,
        migration_source_refill: tiler_core::output_selection::MigrationSourceRefill::LastRemaining,
        anchor_fallback: tiler_core::admission::AnchorFallback::default(),
        no_space_result: tiler_core::admission::NoSpaceResult::default(),
        default_tiled: true,
        focused_global: focused.to_owned(),
        inventory_complete: true,
        gesture_barrier: false,
        feedback,
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

#[test]
fn wholly_unarrived_refusal_restores_unit_and_focus() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 100),
                fact("win-c", "out-a", 200),
            ],
            "win-b",
            None,
            None,
            vec![],
        ),
    );
    let view_a = first
        .outputs
        .iter()
        .find(|row| row.output.0 == "out-a")
        .and_then(|row| row.logical_view.clone())
        .expect("view a");
    let view_b = first
        .outputs
        .iter()
        .find(|row| row.output.0 == "out-b")
        .and_then(|row| row.logical_view.clone())
        .expect("view b");
    // Remember focus on win-b.
    accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 100),
                fact("win-c", "out-a", 200),
            ],
            "win-b",
            None,
            None,
            vec![],
        ),
    );
    let src_key = tiler_core::session::DomainKey {
        output: OutputId("out-a".to_owned()),
        workspace: tiler_core::directional::WorkspaceId(view_a.clone()),
    };
    let pre_snapshot = engine
        .engine()
        .session(&src_key)
        .expect("pre session")
        .snapshot();
    assert!(
        pre_snapshot.windows.len() >= 3,
        "three members: {pre_snapshot:?}"
    );
    let pre_tree = engine
        .engine()
        .session(&src_key)
        .unwrap()
        .tree_for(&src_key)
        .cloned();
    let pre_focus = engine.engine().session(&src_key).unwrap().focus();
    let pre_ring: Vec<String> = first
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    let pre_index = pre_ring
        .iter()
        .position(|id| id == &view_a)
        .expect("pre index");
    // Commit the migration.
    accepted(
        &mut engine,
        &report(
            4,
            4,
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 100),
                fact("win-c", "out-a", 200),
            ],
            "",
            Some(11),
            Some(WorldCommand::MigrateWorkspace {
                source_output: OutputId("out-a".to_owned()),
                target_output: OutputId("out-b".to_owned()),
                workspace: view_a.clone(),
            }),
            vec![],
        ),
    );
    // Wholly-unarrived refusal: members still observed at the source.
    let settled = accepted(
        &mut engine,
        &report(
            5,
            5,
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 100),
                fact("win-c", "out-a", 200),
            ],
            "",
            None,
            None,
            vec![AttemptFeedback {
                operation_id: "op-11".to_owned(),
                kind: AttemptKind::Transfer,
                outcome: AttemptOutcome::RefusedProven,
                hold: None,
            }],
        ),
    );
    assert!(
        settled.transfers.is_empty(),
        "refusal settles: {:?}",
        settled.transfers
    );
    assert!(
        settled.effects.is_empty(),
        "no focus writes on rollback reply: {:?}",
        settled.effects
    );
    assert!(
        settled.windows.iter().all(|w| w.frame_operation.is_none()),
        "no replayed operations on rollback reply: {:?}",
        settled.windows
    );
    // SAME-reply coherence: the rollback reply itself must publish the
    // restored source scope as Controlled with members homed there, and
    // no stale target scope/member/slot rows.
    let source_scope = format!("out-a/{view_a}");
    let target_scope = format!("out-b/{view_a}");
    let source_status = settled
        .scopes
        .iter()
        .find(|s| s.scope == source_scope)
        .expect("source scope on rollback reply");
    assert_eq!(
        source_status.authority,
        ScopeAuthority::Controlled,
        "source Controlled on rollback reply: {source_status:?}"
    );
    assert!(
        settled.scopes.iter().all(|s| s.scope != target_scope),
        "no stale target scope on rollback reply: {:?}",
        settled.scopes
    );
    for window in ["win-a", "win-b", "win-c"] {
        let row = settled
            .windows
            .iter()
            .find(|row| row.lifetime.0 == window)
            .unwrap_or_else(|| panic!("missing row {window} on rollback reply"));
        assert_eq!(
            (row.output.0.as_str(), row.workspace.as_str()),
            ("out-a", view_a.as_str()),
            "{window} homed at source on rollback reply: {row:?}"
        );
        assert_eq!(
            row.membership_authority,
            ScopeAuthority::Controlled,
            "{window} membership Controlled on rollback reply: {row:?}"
        );
    }
    assert!(
        settled
            .windows
            .iter()
            .all(|row| row.workspace != view_a || row.output.0 == "out-a"),
        "no stale target member rows on rollback reply: {:?}",
        settled.windows
    );
    let row = settled
        .workspaces
        .iter()
        .find(|row| row.id == view_a)
        .expect("mapping restored");
    assert_eq!(row.output.0, "out-a");
    assert_eq!(
        settled
            .outputs
            .iter()
            .find(|row| row.output.0 == "out-a")
            .and_then(|row| row.logical_view.clone())
            .as_deref(),
        Some(view_a.as_str())
    );
    let post_ring: Vec<String> = settled
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    assert_eq!(
        post_ring.iter().position(|id| id == &view_a),
        Some(pre_index),
        "ring position restored: {post_ring:?} vs {pre_index}"
    );
    // The retained unit survives modulo homing: same window set, same tree,
    // same remembered focus leaf.
    let restored = engine.engine().session(&src_key).expect("rolled back");
    let mut pre_ids: Vec<String> = pre_snapshot
        .windows
        .iter()
        .map(|link| link.window.0.clone())
        .collect();
    let mut post_ids: Vec<String> = restored
        .snapshot()
        .windows
        .iter()
        .map(|link| link.window.0.clone())
        .collect();
    pre_ids.sort();
    post_ids.sort();
    assert_eq!(pre_ids, post_ids, "membership preserved");
    assert_eq!(
        restored.tree_for(&src_key),
        pre_tree.as_ref(),
        "tree/shares preserved"
    );
    assert_eq!(restored.focus().1, pre_focus.1, "focus leaf preserved");
    let _ = view_b;
}

#[test]
fn partial_refusal_holds_persistently_without_retry() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            "",
            None,
            None,
            vec![],
        ),
    );
    let view_a = first
        .outputs
        .iter()
        .find(|row| row.output.0 == "out-a")
        .and_then(|row| row.logical_view.clone())
        .expect("view a");
    accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            "",
            Some(11),
            Some(WorldCommand::MigrateWorkspace {
                source_output: OutputId("out-a".to_owned()),
                target_output: OutputId("out-b".to_owned()),
                workspace: view_a.clone(),
            }),
            vec![],
        ),
    );
    // One member arrives, one stays, with proven refusal: partial hold.
    let partial = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![fact("win-a", "out-b", 800), fact("win-b", "out-a", 100)],
            "",
            None,
            None,
            vec![AttemptFeedback {
                operation_id: "op-11".to_owned(),
                kind: AttemptKind::Transfer,
                outcome: AttemptOutcome::RefusedProven,
                hold: None,
            }],
        ),
    );
    let flight = partial
        .transfers
        .iter()
        .find(|t| matches!(t.kind, TransferKind::Migration))
        .expect("partial holds");
    assert!(flight.held, "partial refusal marks held: {flight:?}");
    // The feedback disappears on the next report: the flight must stay held
    // (no retry, identities preserved).
    let still = accepted(
        &mut engine,
        &report(
            4,
            4,
            vec![fact("win-a", "out-b", 800), fact("win-b", "out-a", 100)],
            "",
            None,
            None,
            vec![],
        ),
    );
    let flight = still
        .transfers
        .iter()
        .find(|t| matches!(t.kind, TransferKind::Migration))
        .expect("stays held after ack disappears");
    assert!(flight.held, "held persists: {flight:?}");
}

fn screen_no_workarea(output: &str, x: i32, primary: bool) -> ScreenReport {
    ScreenReport {
        output: OutputId(output.to_owned()),
        full_rect: Some(rect(x, 0, 800, 600)),
        work_area: Some(rect(x, 0, 800, 560)),
        gap: 0,
        outer_gap: 0,
        complete: false,
        primary,
    }
}

fn sticky_fact(window: &str, output: &str, x: i32) -> WindowFact {
    WindowFact {
        floating: true,
        sticky: true,
        ..fact(window, output, x)
    }
}

#[test]
fn source_only_migration_keeps_sticky_at_source_refill() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 100),
                sticky_fact("win-as", "out-a", 300),
            ],
            "",
            None,
            None,
            vec![],
        ),
    );
    let view_a = first
        .outputs
        .iter()
        .find(|row| row.output.0 == "out-a")
        .and_then(|row| row.logical_view.clone())
        .expect("view a");
    accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 100),
                sticky_fact("win-as", "out-a", 300),
            ],
            "",
            None,
            None,
            vec![],
        ),
    );
    let migrated = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 100),
                sticky_fact("win-as", "out-a", 300),
            ],
            "",
            Some(11),
            Some(WorldCommand::MigrateWorkspace {
                source_output: OutputId("out-a".to_owned()),
                target_output: OutputId("out-b".to_owned()),
                workspace: view_a.clone(),
            }),
            vec![],
        ),
    );
    // Core members ride the target; sticky never moves to the target.
    // 2b actuation audit: the immediate reply omits the refill sticky row
    // (pre-existing resweep scope only revisits newly-retiled domains, so
    // the refill domain first converges on the next sweep). That omission
    // is accepted behavior and unchanged by this unit; the invariants are
    // no target row, no frame write, and a single source row by the
    // unchanged report.
    for window in ["win-a", "win-b"] {
        let row = migrated
            .windows
            .iter()
            .find(|row| row.lifetime.0 == window)
            .unwrap_or_else(|| panic!("missing {window}"));
        assert_eq!(row.output.0, "out-b", "{window}: {row:?}");
        assert_eq!(row.workspace, view_a, "{window}: {row:?}");
    }
    let sticky_target: Vec<_> = migrated
        .windows
        .iter()
        .filter(|row| row.lifetime.0 == "win-as" && row.output.0 == "out-b")
        .collect();
    assert!(
        sticky_target.is_empty(),
        "sticky never at target: {sticky_target:?}"
    );
    // Unchanged report holds without flap; sticky still single source row.
    let held = accepted(
        &mut engine,
        &report(
            4,
            4,
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 100),
                sticky_fact("win-as", "out-a", 300),
            ],
            "",
            None,
            None,
            vec![],
        ),
    );
    let sticky: Vec<_> = held
        .windows
        .iter()
        .filter(|row| row.lifetime.0 == "win-as")
        .collect();
    assert_eq!(sticky.len(), 1, "unchanged sticky: {sticky:?}");
    assert_eq!(sticky[0].output.0, "out-a");
}

#[allow(clippy::too_many_arguments)]
fn report_with_screens(
    request_seq: u64,
    observation_seq: u64,
    screens: Vec<ScreenReport>,
    windows: Vec<WindowFact>,
    focused: &str,
    command_seq: Option<u64>,
    command: Option<WorldCommand>,
    feedback: Vec<AttemptFeedback>,
    inventory_complete: bool,
    closed: Vec<WindowId>,
) -> WorldReport {
    WorldReport {
        restored: None,
        contract_version: WORLD_CONTRACT_VERSION,
        owner: "owner-a".to_owned(),
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        observation_seq,
        request_seq,
        request_id: format!("req-{request_seq}"),
        command_seq,
        command,
        workspace_mode: tiler_core::workspace_scope::WorkspaceMode::PerOutputLocal,
        screens,
        windows,
        views: vec![],
        closed,
        host_owned_workspace_domains: false,
        equal_workspace_count: false,
        native: vec![],
        fixed_size_predicate: tiler_core::size_hints::FixedSizePredicate::BothAxes,
        migration_source_refill: tiler_core::output_selection::MigrationSourceRefill::LastRemaining,
        anchor_fallback: tiler_core::admission::AnchorFallback::default(),
        no_space_result: tiler_core::admission::NoSpaceResult::default(),
        default_tiled: true,
        focused_global: focused.to_owned(),
        inventory_complete,
        gesture_barrier: false,
        feedback,
    }
}

#[test]
fn gap_refusal_holds_then_proven_rollback_preserves_unit() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 100),
                fact("win-c", "out-a", 200),
            ],
            "win-b",
            None,
            None,
            vec![],
        ),
    );
    let view_a = first
        .outputs
        .iter()
        .find(|row| row.output.0 == "out-a")
        .and_then(|row| row.logical_view.clone())
        .expect("view a");
    accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 100),
                fact("win-c", "out-a", 200),
            ],
            "win-b",
            None,
            None,
            vec![],
        ),
    );
    let src_key = tiler_core::session::DomainKey {
        output: OutputId("out-a".to_owned()),
        workspace: tiler_core::directional::WorkspaceId(view_a.clone()),
    };
    let pre_tree = engine
        .engine()
        .session(&src_key)
        .unwrap()
        .tree_for(&src_key)
        .cloned();
    let pre_focus = engine.engine().session(&src_key).unwrap().focus();
    let pre_ring: Vec<String> = first
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    accepted(
        &mut engine,
        &report(
            4,
            4,
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 100),
                fact("win-c", "out-a", 200),
            ],
            "",
            Some(11),
            Some(WorldCommand::MigrateWorkspace {
                source_output: OutputId("out-a".to_owned()),
                target_output: OutputId("out-b".to_owned()),
                workspace: view_a.clone(),
            }),
            vec![],
        ),
    );
    // Gap + refusal: empty windows, incomplete inventory. Must hold, never
    // fabricate wholly-unarrived rollback from absence.
    let gap = accepted(
        &mut engine,
        &report_with_screens(
            5,
            5,
            vec![screen("out-a", 0, true), screen("out-b", 800, false)],
            vec![],
            "",
            None,
            None,
            vec![AttemptFeedback {
                operation_id: "op-11".to_owned(),
                kind: AttemptKind::Transfer,
                outcome: AttemptOutcome::RefusedProven,
                hold: None,
            }],
            false,
            vec![],
        ),
    );
    let flight = gap
        .transfers
        .iter()
        .find(|t| matches!(t.kind, TransferKind::Migration))
        .expect("gap+refusal holds");
    assert!(flight.held, "gap hold persistent: {flight:?}");
    let row = gap
        .workspaces
        .iter()
        .find(|row| row.id == view_a)
        .expect("mapping during gap");
    assert_eq!(row.output.0, "out-b", "no rollback from absence: {row:?}");
    // Partial arrival with complete inventory but no refusal ack: holds
    // narrowly (covered by `partial_refusal_holds_persistently_without_retry`
    // for the two-member shape; the three-member gap sequence goes straight
    // to proven rollback so trailing lifecycle never rekeys the ring).
    // Wholly proven source refusal with valid source: coherent rollback,
    // no new operation or replay.
    let settled = accepted(
        &mut engine,
        &report(
            7,
            7,
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 100),
                fact("win-c", "out-a", 200),
            ],
            "",
            None,
            None,
            vec![AttemptFeedback {
                operation_id: "op-11".to_owned(),
                kind: AttemptKind::Transfer,
                outcome: AttemptOutcome::RefusedProven,
                hold: None,
            }],
        ),
    );
    assert!(
        settled.transfers.is_empty(),
        "proven refusal settles: {:?}",
        settled.transfers
    );
    assert!(
        settled.windows.iter().all(|w| w.frame_operation.is_none()),
        "no replayed operations: {:?}",
        settled.windows
    );
    let row = settled
        .workspaces
        .iter()
        .find(|row| row.id == view_a)
        .expect("mapping restored");
    assert_eq!(row.output.0, "out-a");
    let post_ring: Vec<String> = settled
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    assert_eq!(pre_ring, post_ring, "ring position restored");
    let restored = engine.engine().session(&src_key).expect("rolled back");
    assert_eq!(
        restored.tree_for(&src_key),
        pre_tree.as_ref(),
        "tree/shares preserved"
    );
    assert_eq!(restored.focus().1, pre_focus.1, "focus leaf preserved");
}

#[test]
fn refusal_close_settles_across_gap() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            "",
            None,
            None,
            vec![],
        ),
    );
    let view_a = first
        .outputs
        .iter()
        .find(|row| row.output.0 == "out-a")
        .and_then(|row| row.logical_view.clone())
        .expect("view a");
    accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            "",
            Some(11),
            Some(WorldCommand::MigrateWorkspace {
                source_output: OutputId("out-a".to_owned()),
                target_output: OutputId("out-b".to_owned()),
                workspace: view_a,
            }),
            vec![],
        ),
    );
    // Explicit closes honored even across gaps, with refusal feedback.
    let settled = accepted(
        &mut engine,
        &report_with_screens(
            3,
            3,
            vec![screen("out-a", 0, true), screen("out-b", 800, false)],
            vec![],
            "",
            None,
            None,
            vec![AttemptFeedback {
                operation_id: "op-11".to_owned(),
                kind: AttemptKind::Transfer,
                outcome: AttemptOutcome::RefusedProven,
                hold: None,
            }],
            false,
            vec![WindowId("win-a".to_owned()), WindowId("win-b".to_owned())],
        ),
    );
    assert!(
        settled
            .transfers
            .iter()
            .all(|t| !matches!(t.kind, TransferKind::Migration)),
        "closed migration settles across gap: {:?}",
        settled.transfers
    );
}

#[test]
fn refusal_without_hostable_source_holds_without_destruction() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 100),
                fact("win-c", "out-a", 200),
            ],
            "win-b",
            None,
            None,
            vec![],
        ),
    );
    let view_a = first
        .outputs
        .iter()
        .find(|row| row.output.0 == "out-a")
        .and_then(|row| row.logical_view.clone())
        .expect("view a");
    accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 100),
                fact("win-c", "out-a", 200),
            ],
            "win-b",
            None,
            None,
            vec![],
        ),
    );
    let src_key = tiler_core::session::DomainKey {
        output: OutputId("out-a".to_owned()),
        workspace: tiler_core::directional::WorkspaceId(view_a.clone()),
    };
    let pre_tree = engine
        .engine()
        .session(&src_key)
        .unwrap()
        .tree_for(&src_key)
        .cloned();
    let pre_focus = engine.engine().session(&src_key).unwrap().focus();
    accepted(
        &mut engine,
        &report(
            4,
            4,
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 100),
                fact("win-c", "out-a", 200),
            ],
            "",
            Some(11),
            Some(WorldCommand::MigrateWorkspace {
                source_output: OutputId("out-a".to_owned()),
                target_output: OutputId("out-b".to_owned()),
                workspace: view_a.clone(),
            }),
            vec![],
        ),
    );
    // Complete report, members proven at source, refusal, but the source
    // cannot host the session (no work area): hold without destruction.
    let held = accepted(
        &mut engine,
        &report_with_screens(
            5,
            5,
            vec![
                screen_no_workarea("out-a", 0, true),
                screen("out-b", 800, false),
            ],
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 100),
                fact("win-c", "out-a", 200),
            ],
            "",
            None,
            None,
            vec![AttemptFeedback {
                operation_id: "op-11".to_owned(),
                kind: AttemptKind::Transfer,
                outcome: AttemptOutcome::RefusedProven,
                hold: None,
            }],
            true,
            vec![],
        ),
    );
    let flight = held
        .transfers
        .iter()
        .find(|t| matches!(t.kind, TransferKind::Migration))
        .expect("unhostable refusal holds");
    assert!(flight.held, "held through gap: {flight:?}");
    let row = held
        .workspaces
        .iter()
        .find(|row| row.id == view_a)
        .expect("mapping during gap");
    assert_eq!(row.output.0, "out-b", "no destructive rollback: {row:?}");
    // Recovery with valid geometry and refusal: coherent rollback.
    let settled = accepted(
        &mut engine,
        &report(
            6,
            6,
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 100),
                fact("win-c", "out-a", 200),
            ],
            "",
            None,
            None,
            vec![AttemptFeedback {
                operation_id: "op-11".to_owned(),
                kind: AttemptKind::Transfer,
                outcome: AttemptOutcome::RefusedProven,
                hold: None,
            }],
        ),
    );
    assert!(
        settled.transfers.is_empty(),
        "recovery settles: {:?}",
        settled.transfers
    );
    let row = settled
        .workspaces
        .iter()
        .find(|row| row.id == view_a)
        .expect("mapping restored");
    assert_eq!(row.output.0, "out-a");
    let restored = engine.engine().session(&src_key).expect("rolled back");
    assert_eq!(
        restored.tree_for(&src_key),
        pre_tree.as_ref(),
        "tree preserved through gap"
    );
    assert_eq!(restored.focus().1, pre_focus.1, "focus preserved");
}

#[test]
fn acknowledged_refusal_recovers_without_feedback_replay() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 100),
                fact("win-c", "out-a", 200),
            ],
            "win-b",
            None,
            None,
            vec![],
        ),
    );
    let view_a = first
        .outputs
        .iter()
        .find(|row| row.output.0 == "out-a")
        .and_then(|row| row.logical_view.clone())
        .expect("view a");
    accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 100),
                fact("win-c", "out-a", 200),
            ],
            "win-b",
            None,
            None,
            vec![],
        ),
    );
    let src_key = tiler_core::session::DomainKey {
        output: OutputId("out-a".to_owned()),
        workspace: tiler_core::directional::WorkspaceId(view_a.clone()),
    };
    let pre_tree = engine
        .engine()
        .session(&src_key)
        .unwrap()
        .tree_for(&src_key)
        .cloned();
    let pre_focus = engine.engine().session(&src_key).unwrap().focus();
    let pre_ring: Vec<String> = first
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    accepted(
        &mut engine,
        &report(
            4,
            4,
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 100),
                fact("win-c", "out-a", 200),
            ],
            "",
            Some(11),
            Some(WorldCommand::MigrateWorkspace {
                source_output: OutputId("out-a".to_owned()),
                target_output: OutputId("out-b".to_owned()),
                workspace: view_a.clone(),
            }),
            vec![],
        ),
    );
    // Terminal refusal arrives inside incomplete empty inventory: hold at
    // the target, never rollback from absence.
    let gap = accepted(
        &mut engine,
        &report_with_screens(
            5,
            5,
            vec![screen("out-a", 0, true), screen("out-b", 800, false)],
            vec![],
            "",
            None,
            None,
            vec![AttemptFeedback {
                operation_id: "op-11".to_owned(),
                kind: AttemptKind::Transfer,
                outcome: AttemptOutcome::RefusedProven,
                hold: None,
            }],
            false,
            vec![],
        ),
    );
    let flight = gap
        .transfers
        .iter()
        .find(|t| matches!(t.kind, TransferKind::Migration))
        .expect("gap+refusal holds");
    assert!(flight.held, "gap hold persistent: {flight:?}");
    // Acknowledgement: feedback disappears. Complete readable evidence
    // proves every member still at the original source. The retained
    // terminal outcome must reconcile to a wholly-source rollback with no
    // replay and no fabricated arrival.
    let settled = accepted(
        &mut engine,
        &report(
            7,
            7,
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 100),
                fact("win-c", "out-a", 200),
            ],
            "",
            None,
            None,
            vec![],
        ),
    );
    assert!(
        settled.transfers.is_empty(),
        "acknowledged refusal settles: {:?}",
        settled.transfers
    );
    assert!(
        settled.windows.iter().all(|w| w.frame_operation.is_none()),
        "no replayed operations: {:?}",
        settled.windows
    );
    assert!(
        settled.effects.is_empty(),
        "no effects on rollback reply after ack: {:?}",
        settled.effects
    );
    let source_scope = format!("out-a/{view_a}");
    let target_scope = format!("out-b/{view_a}");
    let source_status = settled
        .scopes
        .iter()
        .find(|s| s.scope == source_scope)
        .expect("source scope on rollback reply after ack");
    assert_eq!(
        source_status.authority,
        ScopeAuthority::Controlled,
        "source Controlled after ack: {source_status:?}"
    );
    assert!(
        settled.scopes.iter().all(|s| s.scope != target_scope),
        "no stale target scope after ack: {:?}",
        settled.scopes
    );
    for window in ["win-a", "win-b", "win-c"] {
        let row = settled
            .windows
            .iter()
            .find(|row| row.lifetime.0 == window)
            .unwrap_or_else(|| panic!("missing row {window} after ack"));
        assert_eq!(
            (row.output.0.as_str(), row.workspace.as_str()),
            ("out-a", view_a.as_str()),
            "{window} homed at source after ack: {row:?}"
        );
        assert_eq!(
            row.membership_authority,
            ScopeAuthority::Controlled,
            "{window} membership Controlled after ack: {row:?}"
        );
    }
    let row = settled
        .workspaces
        .iter()
        .find(|row| row.id == view_a)
        .expect("mapping restored");
    assert_eq!(row.output.0, "out-a", "rollback after ack: {row:?}");
    let post_ring: Vec<String> = settled
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    assert_eq!(pre_ring, post_ring, "ring position restored");
    assert_eq!(
        settled
            .outputs
            .iter()
            .find(|row| row.output.0 == "out-a")
            .and_then(|row| row.logical_view.clone())
            .as_deref(),
        Some(view_a.as_str()),
        "source view restored"
    );
    let restored = engine.engine().session(&src_key).expect("rolled back");
    assert_eq!(
        restored.tree_for(&src_key),
        pre_tree.as_ref(),
        "tree/shares preserved"
    );
    assert_eq!(restored.focus().1, pre_focus.1, "focus leaf preserved");
}

#[test]
fn acknowledged_supersession_recovers_without_feedback_replay() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 100),
                fact("win-c", "out-a", 200),
            ],
            "win-b",
            None,
            None,
            vec![],
        ),
    );
    let view_a = first
        .outputs
        .iter()
        .find(|row| row.output.0 == "out-a")
        .and_then(|row| row.logical_view.clone())
        .expect("view a");
    accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 100),
                fact("win-c", "out-a", 200),
            ],
            "win-b",
            None,
            None,
            vec![],
        ),
    );
    accepted(
        &mut engine,
        &report(
            4,
            4,
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 100),
                fact("win-c", "out-a", 200),
            ],
            "",
            Some(11),
            Some(WorldCommand::MigrateWorkspace {
                source_output: OutputId("out-a".to_owned()),
                target_output: OutputId("out-b".to_owned()),
                workspace: view_a.clone(),
            }),
            vec![],
        ),
    );
    // Terminal supersession inside incomplete empty inventory: hold at the
    // target, never rollback from absence.
    let gap = accepted(
        &mut engine,
        &report_with_screens(
            5,
            5,
            vec![screen("out-a", 0, true), screen("out-b", 800, false)],
            vec![],
            "",
            None,
            None,
            vec![AttemptFeedback {
                operation_id: "op-11".to_owned(),
                kind: AttemptKind::Transfer,
                outcome: AttemptOutcome::SupersededExternal,
                hold: None,
            }],
            false,
            vec![],
        ),
    );
    let flight = gap
        .transfers
        .iter()
        .find(|t| matches!(t.kind, TransferKind::Migration))
        .expect("gap+supersession holds");
    assert!(flight.held, "gap hold persistent: {flight:?}");
    // Acknowledgement: complete readable evidence proves every member still
    // at the original source. The latched terminal outcome reconciles to a
    // wholly-source rollback with no replay and no fabricated arrival.
    let settled = accepted(
        &mut engine,
        &report(
            7,
            7,
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 100),
                fact("win-c", "out-a", 200),
            ],
            "",
            None,
            None,
            vec![],
        ),
    );
    assert!(
        settled.transfers.is_empty(),
        "acknowledged supersession settles: {:?}",
        settled.transfers
    );
    assert!(
        settled.windows.iter().all(|w| w.frame_operation.is_none()),
        "no replayed operations: {:?}",
        settled.windows
    );
    let row = settled
        .workspaces
        .iter()
        .find(|row| row.id == view_a)
        .expect("mapping restored");
    assert_eq!(row.output.0, "out-a", "rollback after ack: {row:?}");
}

#[test]
fn partial_supersession_holds_across_ack_without_replay() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            "",
            None,
            None,
            vec![],
        ),
    );
    let view_a = first
        .outputs
        .iter()
        .find(|row| row.output.0 == "out-a")
        .and_then(|row| row.logical_view.clone())
        .expect("view a");
    accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            "",
            Some(11),
            Some(WorldCommand::MigrateWorkspace {
                source_output: OutputId("out-a".to_owned()),
                target_output: OutputId("out-b".to_owned()),
                workspace: view_a,
            }),
            vec![],
        ),
    );
    // One member arrives, one stays, with proven supersession: partial hold.
    let partial = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![fact("win-a", "out-b", 800), fact("win-b", "out-a", 100)],
            "",
            None,
            None,
            vec![AttemptFeedback {
                operation_id: "op-11".to_owned(),
                kind: AttemptKind::Transfer,
                outcome: AttemptOutcome::SupersededExternal,
                hold: None,
            }],
        ),
    );
    let flight = partial
        .transfers
        .iter()
        .find(|t| matches!(t.kind, TransferKind::Migration))
        .expect("partial holds");
    assert!(flight.held, "partial supersession holds: {flight:?}");
    // The feedback disappears: the flight stays held with identities
    // preserved and no retry.
    let still = accepted(
        &mut engine,
        &report(
            4,
            4,
            vec![fact("win-a", "out-b", 800), fact("win-b", "out-a", 100)],
            "",
            None,
            None,
            vec![],
        ),
    );
    let flight = still
        .transfers
        .iter()
        .find(|t| matches!(t.kind, TransferKind::Migration))
        .expect("stays held after ack disappears");
    assert!(flight.held, "held persists: {flight:?}");
    assert!(
        still.windows.iter().all(|w| w.frame_operation.is_none()),
        "no replayed operations: {:?}",
        still.windows
    );
}

#[test]
fn terminal_then_close_settles_without_feedback_replay() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            "",
            None,
            None,
            vec![],
        ),
    );
    let view_a = first
        .outputs
        .iter()
        .find(|row| row.output.0 == "out-a")
        .and_then(|row| row.logical_view.clone())
        .expect("view a");
    accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            "",
            Some(11),
            Some(WorldCommand::MigrateWorkspace {
                source_output: OutputId("out-a".to_owned()),
                target_output: OutputId("out-b".to_owned()),
                workspace: view_a,
            }),
            vec![],
        ),
    );
    // Terminal refusal inside a gap holds narrowly.
    let gap = accepted(
        &mut engine,
        &report_with_screens(
            3,
            3,
            vec![screen("out-a", 0, true), screen("out-b", 800, false)],
            vec![],
            "",
            None,
            None,
            vec![AttemptFeedback {
                operation_id: "op-11".to_owned(),
                kind: AttemptKind::Transfer,
                outcome: AttemptOutcome::RefusedProven,
                hold: None,
            }],
            false,
            vec![],
        ),
    );
    assert!(
        gap.transfers
            .iter()
            .any(|t| matches!(t.kind, TransferKind::Migration)),
        "gap holds: {:?}",
        gap.transfers
    );
    // Explicit closes arrive after the feedback row is acknowledged: the
    // fully-closed migration settles without any feedback replay.
    let settled = accepted(
        &mut engine,
        &report_with_screens(
            4,
            4,
            vec![screen("out-a", 0, true), screen("out-b", 800, false)],
            vec![],
            "",
            None,
            None,
            vec![],
            false,
            vec![WindowId("win-a".to_owned()), WindowId("win-b".to_owned())],
        ),
    );
    assert!(
        settled
            .transfers
            .iter()
            .all(|t| !matches!(t.kind, TransferKind::Migration)),
        "closed migration settles after ack: {:?}",
        settled.transfers
    );
}

#[test]
fn possibly_executed_never_rolls_back() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 100),
                fact("win-c", "out-a", 200),
            ],
            "",
            None,
            None,
            vec![],
        ),
    );
    let view_a = first
        .outputs
        .iter()
        .find(|row| row.output.0 == "out-a")
        .and_then(|row| row.logical_view.clone())
        .expect("view a");
    accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 100),
                fact("win-c", "out-a", 200),
            ],
            "",
            Some(11),
            Some(WorldCommand::MigrateWorkspace {
                source_output: OutputId("out-a".to_owned()),
                target_output: OutputId("out-b".to_owned()),
                workspace: view_a.clone(),
            }),
            vec![],
        ),
    );
    let ambiguous = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 100),
                fact("win-c", "out-a", 200),
            ],
            "",
            None,
            None,
            vec![AttemptFeedback {
                operation_id: "op-11".to_owned(),
                kind: AttemptKind::Transfer,
                outcome: AttemptOutcome::PossiblyExecuted,
                hold: None,
            }],
        ),
    );
    let flight = ambiguous
        .transfers
        .iter()
        .find(|t| matches!(t.kind, TransferKind::Migration))
        .expect("ambiguous holds");
    assert!(flight.held, "possibly-executed holds: {flight:?}");
    let row = ambiguous
        .workspaces
        .iter()
        .find(|row| row.id == view_a)
        .expect("mapping held");
    assert_eq!(row.output.0, "out-b", "no rollback from ambiguity: {row:?}");
    let still = accepted(
        &mut engine,
        &report(
            4,
            4,
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 100),
                fact("win-c", "out-a", 200),
            ],
            "",
            None,
            None,
            vec![],
        ),
    );
    let flight = still
        .transfers
        .iter()
        .find(|t| matches!(t.kind, TransferKind::Migration))
        .expect("stays held after ack");
    assert!(flight.held, "held persists: {flight:?}");
    let row = still
        .workspaces
        .iter()
        .find(|row| row.id == view_a)
        .expect("mapping held");
    assert_eq!(row.output.0, "out-b", "no rollback after ack: {row:?}");
}

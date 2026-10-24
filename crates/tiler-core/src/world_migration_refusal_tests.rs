//! Migration refusal rollback: wholly-unarrived refusal restores the
//! retained logical unit (ring position, views, history, membership) and
//! relocates the Engine session back so tree/shares/focus survive modulo
//! homing. Partial refusal holds persistently with identities preserved.

use crate::directional::{OutputId, WindowId};
use crate::geometry::Rect;
use crate::size_hints::WindowSizeHints;
use crate::world_engine::{WorldEngine, WorldOutcome};
use crate::world_report::{
    FrameSetter, MemberAttempt, ScopeAuthority, ScreenReport, TransferKind, ViewAttempt,
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
        native_workspace: None,
    }
}

/// Raw membership refusal for one migration member: explicit native refusal
/// with an unchanged readable preimage at the source (never the target).
fn member_refused(op: &str, window: &str, target_ws: &str, attempt_ms: u64) -> MemberAttempt {
    MemberAttempt {
        operation_id: op.to_owned(),
        window: WindowId(window.to_owned()),
        target_output: OutputId("out-b".to_owned()),
        target_workspace: target_ws.to_owned(),
        setter: FrameSetter::Refused,
        pre: Some((OutputId("out-a".to_owned()), target_ws.to_owned())),
        post: Some((OutputId("out-a".to_owned()), target_ws.to_owned())),
        attempt_ms,
    }
}

/// Raw ambiguous membership attempt (throwing setter, unchanged homing):
/// holds without re-actuation, never proves refusal or rollback.
fn member_thrown(op: &str, window: &str, target_ws: &str, attempt_ms: u64) -> MemberAttempt {
    MemberAttempt {
        operation_id: op.to_owned(),
        window: WindowId(window.to_owned()),
        target_output: OutputId("out-b".to_owned()),
        target_workspace: target_ws.to_owned(),
        setter: FrameSetter::Threw,
        pre: Some((OutputId("out-a".to_owned()), target_ws.to_owned())),
        post: Some((OutputId("out-a".to_owned()), target_ws.to_owned())),
        attempt_ms,
    }
}

/// Raw membership arrival for one migration member: accepted setter with
/// a source preimage and a target postimage.
fn member_arrived(op: &str, window: &str, target_ws: &str, attempt_ms: u64) -> MemberAttempt {
    MemberAttempt {
        operation_id: op.to_owned(),
        window: WindowId(window.to_owned()),
        target_output: OutputId("out-b".to_owned()),
        target_workspace: target_ws.to_owned(),
        setter: FrameSetter::Accepted,
        pre: Some((OutputId("out-a".to_owned()), target_ws.to_owned())),
        post: Some((OutputId("out-b".to_owned()), target_ws.to_owned())),
        attempt_ms,
    }
}

#[test]
fn settlement_shared_fixture_partial_holds_then_completes() {
    // Shared `test-fixtures/frame-convergence.json` settlement section: one
    // member arrives while the other refuses, so the migration holds its
    // operation; the refused member's late arrival then completes the
    // migration to the source refill. Raw attempts only, no verdicts.
    let raw = include_str!("../../../test-fixtures/frame-convergence.json");
    let fixture: serde_json::Value = serde_json::from_str(raw).expect("valid fixture");
    let cases = fixture["settlement_cases"]
        .as_array()
        .expect("settlement_cases array");
    assert_eq!(cases.len(), 1);
    for case in cases {
        let name = case["name"].as_str().expect("name");
        let members: Vec<String> = case["members"]
            .as_array()
            .expect("members")
            .iter()
            .map(|v| v.as_str().expect("member").to_owned())
            .collect();
        assert_eq!(members.len(), 2, "{name}: shared shape is two members");
        let refuse_member = case["refuse_member"].as_str().expect("refuse_member");
        let arrive_member = members
            .iter()
            .find(|m| m.as_str() != refuse_member)
            .expect("arrive member");
        let mut engine = WorldEngine::new("gen-1").expect("generation");
        let first = accepted(
            &mut engine,
            &report(
                1,
                1,
                vec![
                    fact(&members[0], "out-a", 0),
                    fact(&members[1], "out-a", 100),
                ],
                "",
                None,
                None,
                vec![],
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
                    fact(&members[0], "out-a", 0),
                    fact(&members[1], "out-a", 100),
                ],
                "",
                Some(11),
                Some(WorldCommand::MigrateWorkspace {
                    source_output: OutputId("out-a".to_owned()),
                    target_output: OutputId("out-b".to_owned()),
                    workspace: view_a.clone(),
                }),
                vec![],
                vec![],
            ),
        );
        // Partial arrival: one member proves at the target while the other
        // reports a raw refusal at the source. The operation stays retained.
        let partial = accepted(
            &mut engine,
            &report(
                3,
                3,
                vec![
                    fact(arrive_member, "out-b", 0),
                    fact(refuse_member, "out-a", 100),
                ],
                "",
                None,
                None,
                vec![
                    member_arrived("op-11", arrive_member, &view_a, 3),
                    member_refused("op-11", refuse_member, &view_a, 3),
                ],
                vec![],
            ),
        );
        assert_eq!(
            case["expect_retained"].as_bool().expect("expect_retained"),
            partial
                .transfers
                .iter()
                .any(|t| t.operation_id == "op-11" && matches!(t.kind, TransferKind::Migration)),
            "{name}: partial arrival retains the migration"
        );
        let arrived_row = partial
            .windows
            .iter()
            .find(|row| row.lifetime.0 == *arrive_member)
            .expect("arrived row");
        assert_eq!(
            (
                arrived_row.output.0.as_str(),
                arrived_row.workspace.as_str()
            ),
            ("out-b", view_a.as_str()),
            "{name}: arrived member homed at target"
        );
        let expect_membership = match case["expect_authority"].as_str().expect("expect_authority") {
            "controlled" => ScopeAuthority::Controlled,
            other => panic!("{name}: unknown expect_authority {other}"),
        };
        assert_eq!(
            arrived_row.membership_authority, expect_membership,
            "{name}: arrived member actionable as tabled"
        );
        // Late external arrival of the refused member completes the
        // migration to the source refill: no new actuation rides (the held
        // transfer never re-actuates), target observation wins over the
        // staged refusal. The tabled settlement selects which completion
        // shape to verify against the real transfer ids.
        let settled = accepted(
            &mut engine,
            &report(
                4,
                4,
                vec![
                    fact(arrive_member, "out-b", 0),
                    fact(refuse_member, "out-b", 100),
                ],
                "",
                None,
                None,
                vec![],
                vec![],
            ),
        );
        match case["expect_settled"].as_str().expect("expect_settled") {
            "refill-only" => {
                assert!(
                    settled
                        .transfers
                        .iter()
                        .any(|t| t.operation_id == "op-11"
                            && matches!(t.kind, TransferKind::Migration)),
                    "{name}: full arrival keeps the migration only for the refill: {:?}",
                    settled.transfers
                );
                assert!(
                    settled
                        .transfers
                        .iter()
                        .any(|t| t.operation_id == "op-11-refill"),
                    "{name}: the remaining work is the source refill: {:?}",
                    settled.transfers
                );
                for member in &members {
                    let row = settled
                        .windows
                        .iter()
                        .find(|row| row.lifetime.0 == *member)
                        .expect("settled row");
                    assert_eq!(
                        row.output.0.as_str(),
                        "out-b",
                        "{name}: {member} homed at target"
                    );
                }
            }
            other => panic!("{name}: unknown expect_settled {other}"),
        }
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
    member_attempts: Vec<MemberAttempt>,
    view_attempts: Vec<ViewAttempt>,
) -> WorldReport {
    WorldReport {
        contract_version: WORLD_CONTRACT_VERSION,
        owner: "owner-a".to_owned(),
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        observation_seq,
        request_seq,
        request_id: format!("req-{request_seq}"),
        command_seq,
        command,
        workspace_mode: crate::workspace_scope::WorkspaceMode::PerOutputLocal,
        screens: vec![screen("out-a", 0, true), screen("out-b", 800, false)],
        windows,
        views: vec![],
        closed: vec![],
        host_owned_workspace_domains: false,
        equal_workspace_count: false,
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
        default_tiled: true,
        focused_global: focused.to_owned(),
        action_foreground: None,
        action_cached: None,
        // Known registry matches the report screens: the action-context
        // tests below resolve through it.
        action_outputs: vec![OutputId("out-a".to_owned()), OutputId("out-b".to_owned())],
        inventory_complete: true,
        gesture_barrier: false,
        gesture: None,
        monotonic_ms: 0,
        frames: Vec::new(),
        member_attempts,
        view_attempts,
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
            vec![],
        ),
    );
    let src_key = crate::session::DomainKey {
        output: OutputId("out-a".to_owned()),
        workspace: crate::directional::WorkspaceId(view_a.clone()),
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
            vec![
                member_refused("op-11", "win-a", &view_a, 5),
                member_refused("op-11", "win-b", &view_a, 5),
                member_refused("op-11", "win-c", &view_a, 5),
            ],
            vec![],
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
            vec![],
        ),
    );
    // One member arrives, one stays, with a refused member attempt:
    // partial hold.
    let partial = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![fact("win-a", "out-b", 800), fact("win-b", "out-a", 100)],
            "",
            None,
            None,
            vec![member_refused("op-11", "win-b", &view_a, 3)],
            vec![],
        ),
    );
    let flight = partial
        .transfers
        .iter()
        .find(|t| matches!(t.kind, TransferKind::Migration))
        .expect("partial holds");
    assert!(flight.held, "partial refusal marks held: {flight:?}");
    // The raw attempt row is acknowledged on the next report: the flight must stay held
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
    member_attempts: Vec<MemberAttempt>,
    view_attempts: Vec<ViewAttempt>,
    inventory_complete: bool,
    closed: Vec<WindowId>,
) -> WorldReport {
    WorldReport {
        contract_version: WORLD_CONTRACT_VERSION,
        owner: "owner-a".to_owned(),
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        observation_seq,
        request_seq,
        request_id: format!("req-{request_seq}"),
        command_seq,
        command,
        workspace_mode: crate::workspace_scope::WorkspaceMode::PerOutputLocal,
        screens,
        windows,
        views: vec![],
        closed,
        host_owned_workspace_domains: false,
        equal_workspace_count: false,
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
        default_tiled: true,
        focused_global: focused.to_owned(),
        action_foreground: None,
        action_cached: None,
        // Known registry matches the report screens: the action-context
        // tests below resolve through it.
        action_outputs: vec![OutputId("out-a".to_owned()), OutputId("out-b".to_owned())],
        inventory_complete,
        gesture_barrier: false,
        gesture: None,
        monotonic_ms: 0,
        frames: Vec::new(),
        member_attempts,
        view_attempts,
        clear_attempts: Vec::new(),
        creation_attempts: Vec::new(),
        presentation: crate::world_report::PresentationIngress::disabled(),
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
            vec![],
        ),
    );
    let src_key = crate::session::DomainKey {
        output: OutputId("out-a".to_owned()),
        workspace: crate::directional::WorkspaceId(view_a.clone()),
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
            vec![
                member_refused("op-11", "win-a", &view_a, 5),
                member_refused("op-11", "win-b", &view_a, 5),
                member_refused("op-11", "win-c", &view_a, 5),
            ],
            vec![],
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
            vec![
                member_refused("op-11", "win-a", &view_a, 7),
                member_refused("op-11", "win-b", &view_a, 7),
                member_refused("op-11", "win-c", &view_a, 7),
            ],
            vec![],
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
            vec![],
        ),
    );
    // Explicit closes honored even across gaps, with refusal evidence.
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
            vec![
                member_refused("op-11", "win-a", &view_a, 3),
                member_refused("op-11", "win-b", &view_a, 3),
            ],
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
            vec![],
        ),
    );
    let src_key = crate::session::DomainKey {
        output: OutputId("out-a".to_owned()),
        workspace: crate::directional::WorkspaceId(view_a.clone()),
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
            vec![
                member_refused("op-11", "win-a", &view_a, 5),
                member_refused("op-11", "win-b", &view_a, 5),
                member_refused("op-11", "win-c", &view_a, 5),
            ],
            vec![],
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
            vec![
                member_refused("op-11", "win-a", &view_a, 6),
                member_refused("op-11", "win-b", &view_a, 6),
                member_refused("op-11", "win-c", &view_a, 6),
            ],
            vec![],
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
fn acknowledged_refusal_recovers_without_attempt_replay() {
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
            vec![],
        ),
    );
    let src_key = crate::session::DomainKey {
        output: OutputId("out-a".to_owned()),
        workspace: crate::directional::WorkspaceId(view_a.clone()),
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
            vec![
                member_refused("op-11", "win-a", &view_a, 5),
                member_refused("op-11", "win-b", &view_a, 5),
                member_refused("op-11", "win-c", &view_a, 5),
            ],
            vec![],
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
    // Acknowledgement: the attempt row drains. Complete readable evidence
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
fn acknowledged_supersession_recovers_without_attempt_replay() {
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
            vec![],
        ),
    );
    // Terminal supersession inside incomplete empty inventory: hold at the
    // target, never rollback from absence. The raw target-view attempt
    // proves newer external input (readable post naming a known logical
    // id, neither the target nor the KNOWN baseline).
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
            vec![],
            vec![ViewAttempt {
                operation_id: "op-11".to_owned(),
                target_output: OutputId("out-b".to_owned()),
                target_workspace: view_a.clone(),
                setter: FrameSetter::Threw,
                pre: None,
                post: Some(view_b),
                baseline: Some(view_a.clone()),
                attempt_ms: 5,
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
            vec![],
        ),
    );
    // One member arrives, one stays, with an ambiguous member attempt:
    // partial hold.
    let partial = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![fact("win-a", "out-b", 800), fact("win-b", "out-a", 100)],
            "",
            None,
            None,
            vec![member_thrown("op-11", "win-b", &view_a, 3)],
            vec![],
        ),
    );
    let flight = partial
        .transfers
        .iter()
        .find(|t| matches!(t.kind, TransferKind::Migration))
        .expect("partial holds");
    assert!(flight.held, "partial supersession holds: {flight:?}");
    // The attempt row drains: the flight stays held with identities
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
fn terminal_then_close_settles_without_attempt_replay() {
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
            vec![
                member_refused("op-11", "win-a", &view_a, 3),
                member_refused("op-11", "win-b", &view_a, 3),
            ],
            vec![],
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
    // Explicit closes arrive after the attempt row is acknowledged: the
    // fully-closed migration settles without any attempt replay.
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
            vec![
                member_thrown("op-11", "win-a", &view_a, 3),
                member_thrown("op-11", "win-b", &view_a, 3),
                member_thrown("op-11", "win-c", &view_a, 3),
            ],
            vec![],
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

#[test]
fn directional_fences_carry_distinct_fixed_reasons() {
    use crate::world_engine::{
        MSG_MIGRATION_INVENTORY, MSG_MIGRATION_SOURCE, MSG_MIGRATION_SOURCE_UNREADABLE,
        MSG_MIGRATION_TARGET_INCOMPLETE, MSG_MIGRATION_TOPOLOGY, MSG_MIGRATION_TOPOLOGY_UNREADABLE,
    };
    use crate::world_report::{ScreenReport, WindowFact};

    fn screens_two() -> Vec<ScreenReport> {
        vec![screen("out-a", 0, true), screen("out-b", 800, false)]
    }

    fn dir_cmd() -> WorldCommand {
        WorldCommand::MigrateDirectional {
            direction: "right".to_owned(),
        }
    }

    // Seed once for view ids.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![fact("win-a", "out-a", 0)],
            "win-a",
            None,
            None,
            vec![],
            vec![],
        ),
    );
    let view_a = first
        .outputs
        .iter()
        .find(|row| row.output.0 == "out-a")
        .and_then(|row| row.logical_view.clone())
        .expect("view a");

    // No adjacent target: eastmost output heading right. The action
    // context stages the out-b foreground so the core resolves the
    // eastmost source with its current view, then refuses topology.
    let east = WorldReport {
        screens: screens_two(),
        windows: vec![fact("win-a", "out-b", 800)],
        focused_global: "win-a".to_owned(),
        action_foreground: Some(OutputId("out-b".to_owned())),
        action_cached: None,
        action_outputs: vec![OutputId("out-a".to_owned()), OutputId("out-b".to_owned())],
        request_seq: 2,
        observation_seq: 2,
        request_id: "req-2".to_owned(),
        command_seq: Some(11),
        command: Some(WorldCommand::MigrateDirectional {
            direction: "right".to_owned(),
        }),
        inventory_complete: true,
        ..report(2, 2, vec![], "", None, None, vec![], vec![])
    };
    match engine.handle_report(&east) {
        WorldOutcome::Rejected { kind, message } => {
            assert_eq!(kind, "migration-refused");
            assert_eq!(message, MSG_MIGRATION_TOPOLOGY);
        }
        other => panic!("eastmost right must refuse no-target, got {other:?}"),
    }

    // Incomplete inventory.
    let mut inv = report(
        3,
        3,
        vec![fact("win-a", "out-a", 0)],
        "win-a",
        Some(12),
        Some(dir_cmd()),
        vec![],
        vec![],
    );
    inv.inventory_complete = false;
    match engine.handle_report(&inv) {
        WorldOutcome::Rejected { kind, message } => {
            assert_eq!(kind, "migration-refused");
            assert_eq!(message, MSG_MIGRATION_INVENTORY);
        }
        other => panic!("incomplete inventory must refuse, got {other:?}"),
    }

    // Stale registered cache: a mapped-but-removed source still selects
    // from the retained registry and refuses downstream instead of
    // silently retargeting. The explicit ghost-workspace route is gone:
    // direction-only commands always name the source's current view.
    let mut ghost = report(
        4,
        4,
        vec![fact("win-a", "out-a", 0)],
        "win-a",
        Some(13),
        Some(dir_cmd()),
        vec![],
        vec![],
    );
    ghost.action_outputs.push(OutputId("out-z".to_owned()));
    ghost.action_cached = Some(OutputId("out-z".to_owned()));
    match engine.handle_report(&ghost) {
        WorldOutcome::Rejected { kind, message } => {
            assert_eq!(kind, "migration-refused");
            assert_eq!(message, MSG_MIGRATION_SOURCE);
        }
        other => panic!("ghost source must refuse, got {other:?}"),
    }

    // Source unreadable: homed window with no rect.
    let unread = WindowFact {
        rect: None,
        ..fact("win-a", "out-a", 0)
    };
    let bad_src = report(
        5,
        5,
        vec![unread],
        "",
        Some(14),
        Some(dir_cmd()),
        vec![],
        vec![],
    );
    match engine.handle_report(&bad_src) {
        WorldOutcome::Rejected { kind, message } => {
            assert_eq!(kind, "migration-refused");
            assert_eq!(message, MSG_MIGRATION_SOURCE_UNREADABLE);
        }
        other => panic!("unreadable source must refuse, got {other:?}"),
    }

    // Target incomplete via the named path (directional topology runs
    // first, so an incomplete screen there reads as unreadable topology).
    let mut inc_screens = screens_two();
    inc_screens[1].complete = false;
    let mut tgt = report(
        6,
        6,
        vec![fact("win-a", "out-a", 0)],
        "win-a",
        Some(15),
        Some(WorldCommand::MigrateWorkspace {
            source_output: OutputId("out-a".to_owned()),
            target_output: OutputId("out-b".to_owned()),
            workspace: view_a.clone(),
        }),
        vec![],
        vec![],
    );
    tgt.screens = inc_screens.clone();
    match engine.handle_report(&tgt) {
        WorldOutcome::Rejected { kind, message } => {
            assert_eq!(kind, "migration-refused");
            assert_eq!(message, MSG_MIGRATION_TARGET_INCOMPLETE);
        }
        other => panic!("incomplete target must refuse, got {other:?}"),
    }

    // Directional topology unreadable: incomplete screens clear the
    // retained FULL topology, so the directional layer cannot rank.
    let mut topo = report(
        7,
        7,
        vec![fact("win-a", "out-a", 0)],
        "win-a",
        Some(16),
        Some(dir_cmd()),
        vec![],
        vec![],
    );
    topo.screens = inc_screens;
    match engine.handle_report(&topo) {
        WorldOutcome::Rejected { kind, message } => {
            assert_eq!(kind, "migration-refused");
            assert_eq!(message, MSG_MIGRATION_TOPOLOGY_UNREADABLE);
        }
        other => panic!("unreadable topology must refuse, got {other:?}"),
    }

    // Second migrate after a commit resolves the source's current view
    // (the refill workspace), not the moved-away one: the direction-only
    // command cannot name the stale workspace the old explicit legs
    // pinned, so the homed fence never fires here. Flight stays as a
    // defensive fence for retained-but-still-homed states (covered
    // above by the explicit retry).
    let mut flight_engine = WorldEngine::new("gen-1").expect("generation");
    accepted(
        &mut flight_engine,
        &report(
            1,
            1,
            vec![fact("win-a", "out-a", 0)],
            "win-a",
            None,
            None,
            vec![],
            vec![],
        ),
    );
    accepted(
        &mut flight_engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", 0)],
            "win-a",
            Some(21),
            Some(dir_cmd()),
            vec![],
            vec![],
        ),
    );
    let again = report(
        3,
        3,
        vec![fact("win-a", "out-a", 0)],
        "win-a",
        Some(22),
        Some(dir_cmd()),
        vec![],
        vec![],
    );
    match flight_engine.handle_report(&again) {
        WorldOutcome::Accepted(reply) => {
            let source = reply
                .command_result
                .clone()
                .and_then(|result| match result {
                    crate::world_report::CommandResult::Migration { source, .. } => Some(source),
                    _ => None,
                })
                .expect("migration provenance");
            assert_eq!(source.output.0, "out-a", "resolves the live source");
        }
        other => panic!("second migrate after commit must apply current view, got {other:?}"),
    }
}

#[test]
fn offset_two_output_migration_applies() {
    // Generic y-offset shape (never local specifics): primary plus a
    // narrower x-touching secondary offset in y. Proves offset geometry
    // alone never fences directional migration.
    use crate::directional::OutputId;
    use crate::geometry::Rect;
    use crate::size_hints::WindowSizeHints;
    use crate::world_report::{ScreenReport, WindowFact};

    fn r(x: i32, y: i32, w: i32, h: i32) -> Rect {
        Rect { x, y, w, h }
    }
    fn scr(output: &str, full: Rect, work: Rect, primary: bool) -> ScreenReport {
        ScreenReport {
            output: OutputId(output.to_owned()),
            full_rect: Some(full),
            work_area: Some(work),
            gap: 0,
            outer_gap: 0,
            complete: true,
            primary,
        }
    }
    fn wf(window: &str, output: &str, rect: Rect) -> WindowFact {
        WindowFact {
            window: crate::directional::WindowId(window.to_owned()),
            output: OutputId(output.to_owned()),
            rect: Some(rect),
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
    let screens = vec![
        scr("out-a", r(0, 0, 800, 600), r(0, 0, 800, 560), true),
        scr("out-b", r(800, 100, 640, 440), r(800, 100, 640, 420), false),
    ];
    let wins = || {
        vec![
            wf("m1", "out-a", r(10, 10, 200, 200)),
            wf("m2", "out-a", r(300, 10, 200, 200)),
            wf("m3", "out-a", r(300, 300, 200, 200)),
            wf("n1", "out-b", r(810, 110, 200, 200)),
            wf("n2", "out-b", r(1100, 110, 200, 200)),
        ]
    };
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    accepted(
        &mut engine,
        &WorldReport {
            screens: screens.clone(),
            windows: wins(),
            focused_global: "m1".to_owned(),
            action_foreground: None,
            action_cached: None,
            action_outputs: Vec::new(),
            request_seq: 1,
            observation_seq: 1,
            request_id: "req-1".to_owned(),
            ..report(1, 1, vec![], "", None, None, vec![], vec![])
        },
    );
    // Offset outputs still resolve the registry-order source with its
    // current view; the strict largest-edge target is unchanged.
    let reply = accepted(
        &mut engine,
        &WorldReport {
            screens: screens.clone(),
            windows: wins(),
            focused_global: "m1".to_owned(),
            action_foreground: None,
            action_cached: None,
            action_outputs: vec![OutputId("out-a".to_owned()), OutputId("out-b".to_owned())],
            request_seq: 2,
            observation_seq: 2,
            request_id: "req-2".to_owned(),
            command_seq: Some(11),
            command: Some(WorldCommand::MigrateDirectional {
                direction: "right".to_owned(),
            }),
            ..report(2, 2, vec![], "", None, None, vec![], vec![])
        },
    );
    assert!(
        reply
            .transfers
            .iter()
            .any(|t| matches!(t.kind, crate::world_report::TransferKind::Migration)),
        "offset 3+2 must retain migration transfer"
    );
}

#[test]
fn unknown_baseline_never_proves_third_location() {
    // Baseline None (unknown) must not prove external supersession even
    // when the post names a known logical id distinct from the target:
    // the flight holds on outstanding members instead of settling.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![fact("win-a", "out-a", 0)],
            "",
            None,
            None,
            vec![],
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
    accepted(
        &mut engine,
        &report(
            4,
            4,
            vec![fact("win-a", "out-a", 0)],
            "",
            Some(11),
            Some(WorldCommand::MigrateWorkspace {
                source_output: OutputId("out-a".to_owned()),
                target_output: OutputId("out-b".to_owned()),
                workspace: view_a.clone(),
            }),
            vec![],
            vec![],
        ),
    );
    let held = accepted(
        &mut engine,
        &report_with_screens(
            5,
            5,
            vec![screen("out-a", 0, true), screen("out-b", 800, false)],
            vec![fact("win-a", "out-a", 0)],
            "",
            None,
            None,
            vec![],
            vec![ViewAttempt {
                operation_id: "op-11".to_owned(),
                target_output: OutputId("out-b".to_owned()),
                target_workspace: view_a.clone(),
                setter: FrameSetter::Threw,
                pre: None,
                post: Some(view_b),
                baseline: None,
                attempt_ms: 5,
            }],
            true,
            vec![],
        ),
    );
    assert!(
        held.transfers
            .iter()
            .any(|t| matches!(t.kind, TransferKind::Migration) && t.held),
        "unknown baseline holds, never supersedes: {:?}",
        held.transfers
    );
}

#[test]
fn unchanged_preimage_never_proves_third_location() {
    // pre == post (unchanged) is never external proof even when the
    // baseline differs: the write left the view where it was.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![fact("win-a", "out-a", 0)],
            "",
            None,
            None,
            vec![],
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
    accepted(
        &mut engine,
        &report(
            4,
            4,
            vec![fact("win-a", "out-a", 0)],
            "",
            Some(11),
            Some(WorldCommand::MigrateWorkspace {
                source_output: OutputId("out-a".to_owned()),
                target_output: OutputId("out-b".to_owned()),
                workspace: view_a.clone(),
            }),
            vec![],
            vec![],
        ),
    );
    let held = accepted(
        &mut engine,
        &report_with_screens(
            5,
            5,
            vec![screen("out-a", 0, true), screen("out-b", 800, false)],
            vec![fact("win-a", "out-a", 0)],
            "",
            None,
            None,
            vec![],
            vec![ViewAttempt {
                operation_id: "op-11".to_owned(),
                target_output: OutputId("out-b".to_owned()),
                target_workspace: view_a.clone(),
                setter: FrameSetter::Threw,
                pre: Some(view_b.clone()),
                post: Some(view_b),
                baseline: Some(view_a.clone()),
                attempt_ms: 5,
            }],
            true,
            vec![],
        ),
    );
    assert!(
        held.transfers
            .iter()
            .any(|t| matches!(t.kind, TransferKind::Migration) && t.held),
        "unchanged pre holds, never supersedes: {:?}",
        held.transfers
    );
}

#[test]
fn refused_refill_keeps_child_and_holds_parent_across_ack() {
    // A refused refill never retires its child and never lets the parent
    // settle as externally moved: the child stays held with its raw
    // staged (spent fence blocks repeats), the parent holds persistently,
    // and mere child absence can never itself prove external. Multi-report:
    // refusal, ack/gap with no new attempt, then late exact arrival.
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
            vec![],
        ),
    );
    let view_a = first
        .outputs
        .iter()
        .find(|row| row.output.0 == "out-a")
        .and_then(|row| row.logical_view.clone())
        .expect("view a");
    let reply = accepted(
        &mut engine,
        &report(
            4,
            4,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            "",
            Some(11),
            Some(WorldCommand::MigrateWorkspace {
                source_output: OutputId("out-a".to_owned()),
                target_output: OutputId("out-b".to_owned()),
                workspace: view_a.clone(),
            }),
            vec![],
            vec![],
        ),
    );
    let refill_op = "op-11-refill".to_owned();
    let refill_ws = reply
        .transfers
        .iter()
        .find(|t| t.operation_id == refill_op)
        .map(|t| t.target_workspace.clone())
        .expect("refill child retained");
    let refill_out = reply
        .transfers
        .iter()
        .find(|t| t.operation_id == refill_op)
        .map(|t| t.target_output.clone())
        .expect("refill out");
    // Members arrive at the target but the source refill is refused: the
    // child stays, the parent holds.
    let refused = accepted(
        &mut engine,
        &report_with_screens(
            5,
            5,
            vec![screen("out-a", 0, true), screen("out-b", 800, false)],
            vec![fact("win-a", "out-b", 800), fact("win-b", "out-b", 900)],
            "",
            None,
            None,
            vec![],
            vec![ViewAttempt {
                operation_id: refill_op.clone(),
                target_output: refill_out.clone(),
                target_workspace: refill_ws.clone(),
                setter: FrameSetter::Refused,
                pre: Some(view_a.clone()),
                post: Some(view_a.clone()),
                baseline: Some(refill_ws.clone()),
                attempt_ms: 5,
            }],
            true,
            vec![],
        ),
    );
    let parent = refused
        .transfers
        .iter()
        .find(|t| matches!(t.kind, TransferKind::Migration))
        .expect("parent holds after refill refusal");
    assert!(
        refused
            .transfers
            .iter()
            .any(|t| matches!(t.kind, TransferKind::Migration)),
        "parent held: {parent:?}"
    );
    assert!(
        refused
            .transfers
            .iter()
            .any(|t| t.operation_id == refill_op),
        "refused child kept, never retired: {:?}",
        refused.transfers
    );
    // Ack/gap with no new attempt: still held, never settled as external.
    let acked = accepted(
        &mut engine,
        &report(
            6,
            6,
            vec![fact("win-a", "out-b", 800), fact("win-b", "out-b", 900)],
            "",
            None,
            None,
            vec![],
            vec![],
        ),
    );
    assert!(
        acked
            .transfers
            .iter()
            .any(|t| matches!(t.kind, TransferKind::Migration)),
        "parent holds across ack: {:?}",
        acked.transfers
    );
    assert!(
        acked.transfers.iter().any(|t| t.operation_id == refill_op),
        "child survives ack: {:?}",
        acked.transfers
    );
}

#[test]
fn refused_refill_external_source_echo_settles_parent() {
    // A refused refill holds, but a later genuine external source move
    // (a known current echo naming a third workspace, neither the refill
    // nor the refused preimage) settles the parent instead of holding
    // forever. Unknown echoes and gaps still hold.
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
            vec![],
        ),
    );
    let view_a = first
        .outputs
        .iter()
        .find(|row| row.output.0 == "out-a")
        .and_then(|row| row.logical_view.clone())
        .expect("view a");
    let reply = accepted(
        &mut engine,
        &report(
            4,
            4,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            "",
            Some(11),
            Some(WorldCommand::MigrateWorkspace {
                source_output: OutputId("out-a".to_owned()),
                target_output: OutputId("out-b".to_owned()),
                workspace: view_a.clone(),
            }),
            vec![],
            vec![],
        ),
    );
    let refill_op = "op-11-refill".to_owned();
    let refill_ws = reply
        .transfers
        .iter()
        .find(|t| t.operation_id == refill_op)
        .map(|t| t.target_workspace.clone())
        .expect("refill child retained");
    let refill_out = reply
        .transfers
        .iter()
        .find(|t| t.operation_id == refill_op)
        .map(|t| t.target_output.clone())
        .expect("refill out");
    let migration_op = reply
        .transfers
        .iter()
        .find(|t| matches!(t.kind, TransferKind::Migration))
        .expect("migration retained")
        .operation_id
        .clone();
    let target_ws = reply
        .transfers
        .iter()
        .find(|t| t.operation_id == migration_op)
        .map(|t| t.target_workspace.clone())
        .expect("target ws");
    let _ = target_ws;
    let refused = accepted(
        &mut engine,
        &report_with_screens(
            5,
            5,
            vec![screen("out-a", 0, true), screen("out-b", 800, false)],
            vec![fact("win-a", "out-b", 800), fact("win-b", "out-b", 900)],
            "",
            None,
            None,
            vec![],
            vec![ViewAttempt {
                operation_id: refill_op.clone(),
                target_output: refill_out.clone(),
                target_workspace: refill_ws.clone(),
                setter: FrameSetter::Refused,
                pre: Some(view_a.clone()),
                post: Some(view_a.clone()),
                baseline: Some(refill_ws.clone()),
                attempt_ms: 5,
            }],
            true,
            vec![],
        ),
    );
    assert!(
        refused
            .transfers
            .iter()
            .any(|t| matches!(t.kind, TransferKind::Migration)),
        "parent holds after refusal"
    );
    // Ack with no new attempt: still held.
    let acked = accepted(
        &mut engine,
        &report(
            6,
            6,
            vec![fact("win-a", "out-b", 800), fact("win-b", "out-b", 900)],
            "",
            None,
            None,
            vec![],
            vec![],
        ),
    );
    assert!(
        acked.transfers.iter().any(|t| t.operation_id == refill_op),
        "child survives ack: {:?}",
        acked.transfers
    );
    // Third known workspace on the source output, neither the refill nor
    // the refused preimage.
    let ring_a = engine.logical_ring("out-a");
    let third = ring_a
        .iter()
        .find(|id| *id != &refill_ws && *id != &view_a)
        .cloned()
        .expect("third known source workspace");
    // Current echo moves externally to the third workspace with no new raw
    // attempt (post-ack): the parent must settle, dropping the child.
    let mut external = report_with_screens(
        7,
        7,
        vec![screen("out-a", 0, true), screen("out-b", 800, false)],
        vec![fact("win-a", "out-b", 800), fact("win-b", "out-b", 900)],
        "",
        None,
        None,
        vec![],
        vec![],
        true,
        vec![],
    );
    external.views = vec![
        crate::world_report::ScreenView {
            output: OutputId("out-a".to_owned()),
            view: third.clone(),
        },
        crate::world_report::ScreenView {
            output: OutputId("out-b".to_owned()),
            view: engine
                .logical_ring("out-b")
                .into_iter()
                .find(|id| id == &view_a)
                .unwrap_or_else(|| {
                    engine
                        .logical_ring("out-b")
                        .first()
                        .cloned()
                        .unwrap_or_default()
                }),
        },
    ];
    let settled = accepted(&mut engine, &external);
    assert!(
        settled
            .transfers
            .iter()
            .all(|t| !matches!(t.kind, TransferKind::Migration)),
        "external source echo settles parent: {:?} (third={third:?} refill={refill_ws:?} pre={view_a:?})",
        settled.transfers
    );
    assert!(
        !settled
            .transfers
            .iter()
            .any(|t| t.operation_id == refill_op),
        "refused child drops on external source move: {:?}",
        settled.transfers
    );
}

#[test]
fn migrate_directional_resolves_action_context_without_adapter_preselect() {
    use crate::world_engine::MSG_MIGRATION_SOURCE;
    // Seed one window per output; views name the live workspace per wire.
    fn seeded() -> (WorldEngine, String, String) {
        let mut engine = WorldEngine::new("gen-1").expect("generation");
        let first = accepted(
            &mut engine,
            &report(
                1,
                1,
                vec![fact("win-a", "out-a", 0), fact("win-b", "out-b", 800)],
                "win-a",
                None,
                None,
                vec![],
                vec![],
            ),
        );
        let view = |output: &str| {
            first
                .outputs
                .iter()
                .find(|row| row.output.0 == output)
                .and_then(|row| row.logical_view.clone())
                .unwrap_or_else(|| panic!("view {output}"))
        };
        (engine, view("out-a"), view("out-b"))
    }
    fn registry(outputs: &[&str]) -> Vec<OutputId> {
        outputs
            .iter()
            .map(|out| OutputId((*out).to_owned()))
            .collect()
    }
    #[allow(clippy::too_many_arguments)]
    fn context_cmd(
        seq: u64,
        outputs: Vec<OutputId>,
        foreground: Option<&str>,
        cursor: Option<(i32, i32)>,
        cached: Option<&str>,
        direction: &str,
    ) -> WorldReport {
        WorldReport {
            cursor,
            action_foreground: foreground.map(|out| OutputId(out.to_owned())),
            action_cached: cached.map(|out| OutputId(out.to_owned())),
            action_outputs: outputs,
            request_seq: seq,
            observation_seq: seq,
            request_id: format!("req-{seq}"),
            command_seq: Some(10 + seq),
            command: Some(WorldCommand::MigrateDirectional {
                direction: direction.to_owned(),
            }),
            ..report(
                seq,
                seq,
                vec![fact("win-a", "out-a", 0), fact("win-b", "out-b", 800)],
                "win-a",
                None,
                None,
                vec![],
                vec![],
            )
        }
    }
    fn migrated(reply: &crate::world_report::WorldReply) -> (String, String) {
        let transfer = reply
            .transfers
            .iter()
            .find(|t| matches!(t.kind, TransferKind::Migration))
            .expect("migration retained");
        (
            transfer.target_output.0.clone(),
            transfer.target_workspace.clone(),
        )
    }
    let live = || registry(&["out-a", "out-b"]);
    // Pointer leg beats the cache: cursor on out-b resolves the out-b
    // source even while the cache still names live out-a.
    let (mut engine, _, view_b) = seeded();
    let reply = accepted(
        &mut engine,
        &context_cmd(2, live(), None, Some((810, 10)), Some("out-a"), "left"),
    );
    assert_eq!(migrated(&reply), ("out-a".to_owned(), view_b.clone()));
    // Foreground beats a disagreeing pointer: the out-a source migrates
    // right even with the cursor on out-b.
    let (mut engine, view_a, _) = seeded();
    let reply = accepted(
        &mut engine,
        &context_cmd(
            2,
            live(),
            Some("out-a"),
            Some((810, 10)),
            Some("out-b"),
            "right",
        ),
    );
    assert_eq!(migrated(&reply), ("out-b".to_owned(), view_a.clone()));
    // Cache leg carries an absent pointer: no foreground, no cursor, the
    // cached out-b source migrates left.
    let (mut engine, _, view_b) = seeded();
    let reply = accepted(
        &mut engine,
        &context_cmd(2, live(), None, None, Some("out-b"), "left"),
    );
    assert_eq!(migrated(&reply), ("out-a".to_owned(), view_b.clone()));
    // Registry order is the last leg: no facts at all resolves the first
    // registry entry out-a, which migrates right.
    let (mut engine, view_a, _) = seeded();
    let reply = accepted(
        &mut engine,
        &context_cmd(2, live(), None, None, None, "right"),
    );
    assert_eq!(migrated(&reply), ("out-b".to_owned(), view_a.clone()));
    // Unknown foreground and an outside pointer never invent: they fall
    // through to the cache, then to registry order.
    let (mut engine, view_a, _) = seeded();
    let reply = accepted(
        &mut engine,
        &context_cmd(2, live(), Some("gone"), Some((5000, 5000)), None, "right"),
    );
    assert_eq!(migrated(&reply), ("out-b".to_owned(), view_a.clone()));
    // A cached output outside the known registry never selects: it falls
    // through to registry order (the adapter sends `None` for unmapped
    // caches, so only registry filtering can observe this).
    let (mut engine, view_a, _) = seeded();
    let reply = accepted(
        &mut engine,
        &context_cmd(2, live(), None, None, Some("gone"), "right"),
    );
    assert_eq!(migrated(&reply), ("out-b".to_owned(), view_a.clone()));
    // A stale-but-registered cache still selects and refuses downstream
    // instead of silently retargeting live state.
    let (mut engine, _, _) = seeded();
    match engine.handle_report(&context_cmd(
        2,
        registry(&["out-a", "out-b", "out-z"]),
        None,
        None,
        Some("out-z"),
        "right",
    )) {
        WorldOutcome::Rejected { kind, message } => {
            assert_eq!(kind, "migration-refused");
            assert_eq!(message, MSG_MIGRATION_SOURCE);
        }
        other => panic!("stale cache must refuse the source, got {other:?}"),
    }
    // No known registry at all refuses without inventing focus. The
    // struct-update base carries a registry, so clear it explicitly:
    // empty screens plus an empty registry resolve nothing.
    let (mut engine, _, _) = seeded();
    let empty = WorldReport {
        screens: vec![],
        windows: vec![],
        focused_global: String::new(),
        action_foreground: None,
        action_cached: None,
        action_outputs: vec![],
        request_seq: 2,
        observation_seq: 2,
        request_id: "req-2".to_owned(),
        command_seq: Some(12),
        command: Some(WorldCommand::MigrateDirectional {
            direction: "right".to_owned(),
        }),
        ..report(2, 2, vec![], "", None, None, vec![], vec![])
    };
    match engine.handle_report(&empty) {
        WorldOutcome::Rejected { kind, message } => {
            assert_eq!(kind, "migration-refused");
            assert_eq!(message, MSG_MIGRATION_SOURCE);
        }
        other => panic!("empty registry must refuse the source, got {other:?}"),
    }
}

#[test]
fn migrate_directional_freezes_source_view_pre_observation() {
    use crate::world_engine::MSG_MIGRATION_SOURCE;
    // The direction-only source (output plus its current view) freezes at
    // run_accepted entry, before staging can converge views or settle
    // rehoming: first-tick unbootstrapped migration refuses like the old
    // pre-tick logical_view lookup, while the same command after a prior
    // observation applies.
    // Fresh engine, no prior observation: no retained view exists, so the
    // frozen source is unknown and the command refuses without mutation.
    let mut fresh = WorldEngine::new("gen-1").expect("valid generation");
    let unbootstrapped = WorldReport {
        request_seq: 1,
        observation_seq: 1,
        request_id: "req-1".to_owned(),
        command_seq: Some(11),
        command: Some(WorldCommand::MigrateDirectional {
            direction: "right".to_owned(),
        }),
        ..report(1, 1, vec![], "", None, None, vec![], vec![])
    };
    match fresh.handle_report(&unbootstrapped) {
        WorldOutcome::Rejected { kind, message } => {
            assert_eq!(kind, "migration-refused");
            assert_eq!(message, MSG_MIGRATION_SOURCE);
        }
        other => panic!("unbootstrapped migration must refuse, got {other:?}"),
    }
    // The same command after a prior observation applies: the retained
    // view resolves the frozen source.
    let mut engine = WorldEngine::new("gen-1").expect("valid generation");
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![fact("win-a", "out-a", 0)],
            "win-a",
            None,
            None,
            vec![],
            vec![],
        ),
    );
    let reply = accepted(
        &mut engine,
        &WorldReport {
            request_seq: 2,
            observation_seq: 2,
            request_id: "req-2".to_owned(),
            command_seq: Some(12),
            command: Some(WorldCommand::MigrateDirectional {
                direction: "right".to_owned(),
            }),
            ..report(
                2,
                2,
                vec![fact("win-a", "out-a", 0)],
                "win-a",
                None,
                None,
                vec![],
                vec![],
            )
        },
    );
    assert!(
        reply
            .transfers
            .iter()
            .any(|t| matches!(t.kind, TransferKind::Migration)),
        "retained view applies: {:?}",
        reply.transfers
    );
}

#[test]
fn migrate_directional_ignores_same_report_view_creation() {
    use crate::world_engine::MSG_MIGRATION_SOURCE;
    // A newly appearing screen must not supply the migration source view:
    // the frozen source predates staging, so the unknown pre-observation
    // view refuses instead of migrating the newly converged workspace.
    // A follow-up report with the now-retained view applies.
    let mut engine = WorldEngine::new("gen-1").expect("valid generation");
    accepted(
        &mut engine,
        &report_with_screens(
            1,
            1,
            vec![screen("out-a", 0, true)],
            vec![fact("win-a", "out-a", 0)],
            "",
            None,
            None,
            vec![],
            vec![],
            true,
            vec![],
        ),
    );
    let hotplug = WorldReport {
        screens: vec![screen("out-a", 0, true), screen("out-b", 800, false)],
        cursor: Some((900, 10)),
        action_outputs: vec![OutputId("out-a".to_owned()), OutputId("out-b".to_owned())],
        request_seq: 2,
        observation_seq: 2,
        request_id: "req-2".to_owned(),
        command_seq: Some(12),
        command: Some(WorldCommand::MigrateDirectional {
            direction: "left".to_owned(),
        }),
        ..report(
            2,
            2,
            vec![fact("win-a", "out-a", 0)],
            "",
            None,
            None,
            vec![],
            vec![],
        )
    };
    match engine.handle_report(&hotplug) {
        WorldOutcome::Rejected { kind, message } => {
            assert_eq!(kind, "migration-refused");
            assert_eq!(message, MSG_MIGRATION_SOURCE);
        }
        other => panic!("hotplug source must refuse without a pre-observation view, got {other:?}"),
    }
    // The observation-only catch-up retains the new screen's view, so the
    // same context on the next report applies instead of refusing.
    accepted(
        &mut engine,
        &report_with_screens(
            3,
            3,
            vec![screen("out-a", 0, true), screen("out-b", 800, false)],
            vec![fact("win-a", "out-a", 0)],
            "",
            None,
            None,
            vec![],
            vec![],
            true,
            vec![],
        ),
    );
    let reply = accepted(
        &mut engine,
        &WorldReport {
            screens: vec![screen("out-a", 0, true), screen("out-b", 800, false)],
            cursor: Some((900, 10)),
            action_outputs: vec![OutputId("out-a".to_owned()), OutputId("out-b".to_owned())],
            request_seq: 4,
            observation_seq: 4,
            request_id: "req-4".to_owned(),
            command_seq: Some(14),
            command: Some(WorldCommand::MigrateDirectional {
                direction: "left".to_owned(),
            }),
            ..report(
                4,
                4,
                vec![fact("win-a", "out-a", 0)],
                "",
                None,
                None,
                vec![],
                vec![],
            )
        },
    );
    let transfer = reply
        .transfers
        .iter()
        .find(|t| matches!(t.kind, TransferKind::Migration))
        .expect("migration applies once the view is retained");
    assert_eq!(transfer.target_output.0, "out-a");
}

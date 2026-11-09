//! Migration refusal rollback: wholly-unarrived refusal restores the
//! retained logical unit (ring position, views, history, membership) and
//! relocates the Engine session back so tree/shares/focus survive modulo
//! homing. Partial refusal holds persistently with identities preserved.

use crate::directional::{OutputId, WindowId, WorkspaceId};
use crate::geometry::Rect;
use crate::size_hints::WindowSizeHints;
use crate::world_engine::{WorldEngine, WorldOutcome};
use crate::world_report::{
    CommandRejectKind, CommandResult, FrameSetter, MemberAttempt, PresentationIngressV4,
    PresentationRowV4, ScopeAuthority, ScreenReport, TransferKind, ViewAttempt,
    WORLD_CONTRACT_VERSION_V4, WindowFact, WorkspaceRef, WorldCommand, WorldReportV4,
};

fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

fn logical_ref(output: &str, workspace: &str) -> WorkspaceRef {
    WorkspaceRef::Logical {
        output: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
    }
}

fn native_ref(output: &str, workspace: &str) -> WorkspaceRef {
    WorkspaceRef::Native {
        output: OutputId(output.to_owned()),
        workspace: crate::ids::NativeWorkspaceId::parse(workspace).expect("native workspace id"),
    }
}

fn logical_view_attempt(
    output: &str,
    target: &str,
    pre: Option<WorkspaceRef>,
    post: Option<WorkspaceRef>,
    baseline: Option<WorkspaceRef>,
) -> ViewAttempt {
    ViewAttempt {
        operation_id: "op-11".to_owned(),
        target_output: OutputId(output.to_owned()),
        target_workspace: target.to_owned(),
        setter: FrameSetter::Threw,
        pre,
        post,
        baseline,
        attempt_ms: 5,
    }
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

/// Raw membership refusal for one migration member: explicit native refusal
/// with an unchanged readable preimage at the source (never the target).
fn member_refused(op: &str, window: &str, target_ws: &str, attempt_ms: u64) -> MemberAttempt {
    MemberAttempt {
        operation_id: op.to_owned(),
        window: WindowId(window.to_owned()),
        target_output: OutputId("out-b".to_owned()),
        target_workspace: target_ws.to_owned(),
        setter: FrameSetter::Refused,
        pre: Some(logical_ref("out-a", target_ws)),
        post: Some(logical_ref("out-a", target_ws)),
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
        pre: Some(logical_ref("out-a", target_ws)),
        post: Some(logical_ref("out-a", target_ws)),
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
        pre: Some(logical_ref("out-a", target_ws)),
        post: Some(logical_ref("out-b", target_ws)),
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
        let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
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
            // The shared fixture still labels this shape "refill-only"
            // (kept byte-for-byte: other seams consume it). Under the
            // native-free binding the full arrival retires the parent
            // with no echo proof and no refill child: member settlement
            // is the terminal.
            "refill-only" => {
                assert!(
                    settled.transfers.is_empty(),
                    "{name}: full arrival retires the migration with no refill wait: {:?}",
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
) -> WorldReportV4 {
    WorldReportV4 {
        contract_version: WORLD_CONTRACT_VERSION_V4,
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        observation_seq,
        request_seq,
        request_id: format!("req-{request_seq}"),
        command_seq,
        command,
        screens: vec![screen("out-a", 0), screen("out-b", 800)],
        held_screens: Vec::new(),
        windows,
        held_windows: Vec::new(),
        closed: vec![],
        native: vec![],
        held_native: Vec::new(),
        cursor: None,
        cursor_hit: None,
        focused_global: focused.to_owned(),
        action_foreground: None,
        action_cached: None,
        // Known registry matches the report screens: the action-context
        // tests below resolve through it.
        action_outputs: vec![OutputId("out-a".to_owned()), OutputId("out-b".to_owned())],
        primary_output: None,
        inventory_complete: true,
        gesture_barrier: false,
        gesture: None,
        monotonic_ms: 0,
        frames: Vec::new(),
        member_attempts,
        view_attempts,
        clear_attempts: Vec::new(),
        creation_attempts: Vec::new(),
        held_attempts: Vec::new(),
        state_attempts: Vec::new(),
        focus_feedback: Vec::new(),
        presentation: PresentationRowV4::Valid(PresentationIngressV4::default()),
        intent_records: Vec::new(),
        renderer_input: None,
    }
}

fn accepted(engine: &mut WorldEngine, report: &WorldReportV4) -> crate::world_report::WorldReplyV4 {
    match engine.handle_report(report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    }
}

#[test]
fn wholly_unarrived_refusal_restores_unit_and_focus() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
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
    let source_status = settled
        .scopes
        .iter()
        .find(|s| s.scope.covers_workspace(&OutputId("out-a".into()), &view_a))
        .expect("source scope on rollback reply");
    assert_eq!(
        source_status.authority,
        ScopeAuthority::Controlled,
        "source Controlled on rollback reply: {source_status:?}"
    );
    assert!(
        settled.scopes.iter().all(|s| {
            !matches!(&s.scope, crate::world_report::AuthorityScope::Workspace { output, workspace }
                if output.0 == "out-b" && workspace.0 == view_a)
        }),
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
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
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

fn screen_no_workarea(output: &str, x: i32) -> ScreenReport {
    ScreenReport {
        output: OutputId(output.to_owned()),
        full_rect: Some(rect(x, 0, 800, 600)),
        work_area: Some(rect(x, 0, 800, 560)),
        complete: false,
        scale: 1.0,
        identity: crate::world_report::OutputIdentity::unknown(),
    }
}

fn sticky_fact(window: &str, output: &str, x: i32) -> WindowFact {
    WindowFact {
        sticky: Some(true),
        transient: Some(true),
        ..fact(window, output, x)
    }
}

#[test]
fn source_only_migration_keeps_sticky_at_source_refill() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
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
) -> WorldReportV4 {
    WorldReportV4 {
        contract_version: WORLD_CONTRACT_VERSION_V4,
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        observation_seq,
        request_seq,
        request_id: format!("req-{request_seq}"),
        command_seq,
        command,
        screens,
        held_screens: Vec::new(),
        windows,
        held_windows: Vec::new(),
        closed,
        native: vec![],
        held_native: Vec::new(),
        cursor: None,
        cursor_hit: None,
        focused_global: focused.to_owned(),
        action_foreground: None,
        action_cached: None,
        // Known registry matches the report screens: the action-context
        // tests below resolve through it.
        action_outputs: vec![OutputId("out-a".to_owned()), OutputId("out-b".to_owned())],
        primary_output: None,
        inventory_complete,
        gesture_barrier: false,
        gesture: None,
        monotonic_ms: 0,
        frames: Vec::new(),
        member_attempts,
        view_attempts,
        clear_attempts: Vec::new(),
        state_attempts: Vec::new(),
        focus_feedback: Vec::new(),
        creation_attempts: Vec::new(),
        held_attempts: Vec::new(),
        presentation: PresentationRowV4::Valid(PresentationIngressV4::default()),
        intent_records: Vec::new(),
        renderer_input: None,
    }
}

#[test]
fn gap_refusal_holds_then_proven_rollback_preserves_unit() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
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
            vec![screen("out-a", 0), screen("out-b", 800)],
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
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
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
            vec![screen("out-a", 0), screen("out-b", 800)],
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
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
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
            vec![screen_no_workarea("out-a", 0), screen("out-b", 800)],
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
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
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
            vec![screen("out-a", 0), screen("out-b", 800)],
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
    let source_status = settled
        .scopes
        .iter()
        .find(|s| s.scope.covers_workspace(&OutputId("out-a".into()), &view_a))
        .expect("source scope on rollback reply after ack");
    assert_eq!(
        source_status.authority,
        ScopeAuthority::Controlled,
        "source Controlled after ack: {source_status:?}"
    );
    assert!(
        settled.scopes.iter().all(|s| {
            !matches!(&s.scope, crate::world_report::AuthorityScope::Workspace { output, workspace }
                if output.0 == "out-b" && workspace.0 == view_a)
        }),
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
fn rejected_logical_supersession_does_not_replay_migration() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
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
    // V4 rejects the historical logical-selector supersession proof. The
    // incomplete inventory still cannot erase migration state.
    match engine.handle_report(&report_with_screens(
        5,
        5,
        vec![screen("out-a", 0), screen("out-b", 800)],
        vec![],
        "",
        None,
        None,
        vec![],
        vec![logical_view_attempt(
            "out-b",
            &view_a,
            None,
            Some(native_ref("out-b", &view_b)),
            Some(native_ref("out-b", &view_a)),
        )],
        false,
        vec![],
    )) {
        WorldOutcome::Rejected { message, .. } => {
            assert_eq!(message, "native-without-capability")
        }
        other => panic!("logical ViewAttempt must reject, got {other:?}"),
    }
    let gap = accepted(
        &mut engine,
        &report_with_screens(
            5,
            5,
            vec![screen("out-a", 0), screen("out-b", 800)],
            vec![],
            "",
            None,
            None,
            vec![],
            vec![],
            false,
            vec![],
        ),
    );
    let flight = gap
        .transfers
        .iter()
        .find(|t| matches!(t.kind, TransferKind::Migration))
        .expect("gap+supersession holds");
    assert_eq!(
        flight.target_workspace, view_a,
        "migration remains live: {flight:?}"
    );
    // Complete membership observations still identify the source, but no
    // logical selector receipt exists to supersede the native operation.
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
        settled
            .transfers
            .iter()
            .any(|transfer| { matches!(transfer.kind, TransferKind::Migration) }),
        "migration remains unresolved without selector proof: {:?}",
        settled.transfers
    );
    assert!(
        settled.windows.iter().all(|w| w.frame_operation.is_none()),
        "no replayed operations: {:?}",
        settled.windows
    );
    assert_eq!(settled.windows.len(), 3, "all members remain represented");
    assert!(settled.workspaces.iter().any(|row| row.id == view_a));
}

#[test]
fn partial_supersession_holds_across_ack_without_replay() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
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
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
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
            vec![screen("out-a", 0), screen("out-b", 800)],
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
            vec![screen("out-a", 0), screen("out-b", 800)],
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
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
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
    use crate::world_report::{ScreenReport, WindowFact};

    fn screens_two() -> Vec<ScreenReport> {
        vec![screen("out-a", 0), screen("out-b", 800)]
    }

    fn dir_cmd() -> WorldCommand {
        WorldCommand::MigrateDirectional {
            direction: "right".to_owned(),
        }
    }

    // Seed once for view ids.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
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
    let east = WorldReportV4 {
        screens: screens_two(),
        windows: vec![fact("win-a", "out-b", 800)],
        focused_global: "win-a".to_owned(),
        action_foreground: Some(OutputId("out-b".to_owned())),
        action_cached: None,
        action_outputs: vec![OutputId("out-a".to_owned()), OutputId("out-b".to_owned())],
        primary_output: None,
        held_screens: Vec::new(),
        held_windows: Vec::new(),
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
        WorldOutcome::Accepted(reply) => assert_eq!(
            reply.command_result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::MigrationRefused
            }),
            "eastmost right must refuse no-target"
        ),
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
        WorldOutcome::Accepted(reply) => assert_eq!(
            reply.command_result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::MigrationRefused
            }),
            "incomplete inventory must refuse"
        ),
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
        WorldOutcome::Accepted(reply) => assert_eq!(
            reply.command_result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::MigrationRefused
            }),
            "ghost source must refuse"
        ),
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
        WorldOutcome::Accepted(reply) => assert_eq!(
            reply.command_result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::MigrationRefused
            }),
            "unreadable source must refuse"
        ),
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
        WorldOutcome::Accepted(reply) => assert_eq!(
            reply.command_result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::MigrationRefused
            }),
            "incomplete target must refuse"
        ),
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
        WorldOutcome::Accepted(reply) => assert_eq!(
            reply.command_result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::MigrationRefused
            }),
            "unreadable topology must refuse"
        ),
        other => panic!("unreadable topology must refuse, got {other:?}"),
    }

    // Second migrate after a commit resolves the source's current view
    // (the refill workspace), not the moved-away one: the direction-only
    // command cannot name the stale workspace the old explicit legs
    // pinned, so the homed fence never fires here. Flight stays as a
    // defensive fence for retained-but-still-homed states (covered
    // above by the explicit retry).
    let mut flight_engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
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
    fn scr(output: &str, full: Rect, work: Rect) -> ScreenReport {
        ScreenReport {
            output: OutputId(output.to_owned()),
            full_rect: Some(full),
            work_area: Some(work),
            complete: true,
            scale: 1.0,
            identity: crate::world_report::OutputIdentity::unknown(),
        }
    }
    fn wf(window: &str, output: &str, rect: Rect) -> WindowFact {
        WindowFact {
            window: crate::directional::WindowId(window.to_owned()),
            output: Some(OutputId(output.to_owned())),
            rect: Some(rect),
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
    let screens = vec![
        scr("out-a", r(0, 0, 800, 600), r(0, 0, 800, 560)),
        scr("out-b", r(800, 100, 640, 440), r(800, 100, 640, 420)),
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
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    accepted(
        &mut engine,
        &WorldReportV4 {
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
        &WorldReportV4 {
            screens: screens.clone(),
            windows: wins(),
            focused_global: "m1".to_owned(),
            action_foreground: None,
            action_cached: None,
            action_outputs: vec![OutputId("out-a".to_owned()), OutputId("out-b".to_owned())],
            primary_output: None,
            held_screens: Vec::new(),
            held_windows: Vec::new(),
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
fn logical_view_attempt_is_rejected_without_losing_migration_membership() {
    // V4 rejects logical selector evidence. The independently observed
    // migration member remains at the source and the migration stays live.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
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
    let screens = vec![screen("out-a", 0), screen("out-b", 800)];
    let windows = vec![fact("win-a", "out-a", 0)];
    match engine.handle_report(&report_with_screens(
        5,
        5,
        screens.clone(),
        windows.clone(),
        "",
        None,
        None,
        vec![],
        vec![logical_view_attempt(
            "out-b",
            &view_a,
            None,
            Some(native_ref("out-b", &view_a)),
            None,
        )],
        true,
        vec![],
    )) {
        WorldOutcome::Rejected { message, .. } => {
            assert_eq!(message, "native-without-capability")
        }
        other => panic!("logical ViewAttempt must be rejected, got {other:?}"),
    }
    let held = accepted(
        &mut engine,
        &report_with_screens(
            5,
            5,
            screens,
            windows,
            "",
            None,
            None,
            vec![],
            vec![],
            true,
            vec![],
        ),
    );
    assert!(
        held.transfers
            .iter()
            .any(|t| matches!(t.kind, TransferKind::Migration)),
        "rejected selector evidence cannot settle migration: {:?}",
        held.transfers
    );
    assert!(
        held.windows
            .iter()
            .any(|window| window.output.0 == "out-b" && window.workspace == view_a),
        "member remains migration-homed at target: {:?}",
        held.windows
    );
}

#[test]
fn logical_selector_attempt_rejects_while_migration_remains_in_flight() {
    // Logical selector evidence is rejected; independently observed
    // migration membership remains at the target while the flight persists.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
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
    let screens = vec![screen("out-a", 0), screen("out-b", 800)];
    let windows = vec![fact("win-a", "out-a", 0)];
    match engine.handle_report(&report_with_screens(
        5,
        5,
        screens.clone(),
        windows.clone(),
        "",
        None,
        None,
        vec![],
        vec![logical_view_attempt(
            "out-b",
            &view_a,
            Some(native_ref("out-b", &view_b)),
            Some(native_ref("out-b", &view_b)),
            Some(native_ref("out-b", &view_a)),
        )],
        true,
        vec![],
    )) {
        WorldOutcome::Rejected { message, .. } => {
            assert_eq!(message, "native-without-capability")
        }
        other => panic!("logical ViewAttempt must be rejected, got {other:?}"),
    }
    let held = accepted(
        &mut engine,
        &report_with_screens(
            5,
            5,
            screens,
            windows,
            "",
            None,
            None,
            vec![],
            vec![],
            true,
            vec![],
        ),
    );
    assert!(
        held.transfers
            .iter()
            .any(|t| matches!(t.kind, TransferKind::Migration)),
        "rejected selector evidence cannot settle migration: {:?}",
        held.transfers
    );
    assert!(
        held.windows
            .iter()
            .any(|window| { window.output.0 == "out-b" && window.workspace == view_a })
    );
}

#[test]
fn arrived_members_retire_parent_without_refill_child_across_ack() {
    // A refused refill is legacy: no refill child exists on the
    // native-free route, so members arriving at the target retires the
    // parent with no echo proof, and the ack stays retired. The spent
    // view-phase fence is unneeded: there is no refill flight to defend.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
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
    // Members arrive at the target but the source refill is refused: the
    // child stays, the parent holds.
    let refused = accepted(
        &mut engine,
        &report_with_screens(
            5,
            5,
            vec![screen("out-a", 0), screen("out-b", 800)],
            vec![fact("win-a", "out-b", 800), fact("win-b", "out-b", 900)],
            "",
            None,
            None,
            vec![],
            vec![],
            true,
            vec![],
        ),
    );
    let parent_gone = refused
        .transfers
        .iter()
        .all(|t| !matches!(t.kind, TransferKind::Migration));
    assert!(
        parent_gone,
        "arrival retires the parent with no refill leg: {:?}",
        refused.transfers
    );
    assert!(
        refused
            .transfers
            .iter()
            .all(|t| t.operation_id != refill_op),
        "no refill child ever exists: {:?}",
        refused.transfers
    );
    // Ack/gap with no new attempt: still retired, never resurrected.
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
        acked.transfers.is_empty(),
        "retired parent stays retired across ack: {:?}",
        acked.transfers
    );
}

#[test]
fn logical_refill_selector_attempt_is_rejected_and_migration_already_retired() {
    // Logical selector receipts cannot settle a host-free v4 migration,
    // and there is no refill leg to defend: independent member arrival
    // already retired the parent, so the invalid proof changes nothing.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
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
    // No refill child exists on the native-free route; the source view
    // rides the commit reply instead.
    assert!(
        reply.transfers.iter().all(|t| t.operation_id != refill_op),
        "no refill child: {:?}",
        reply.transfers
    );
    let refill_ws = reply
        .outputs
        .iter()
        .find(|row| row.output.0 == "out-a")
        .and_then(|row| row.logical_view.clone())
        .expect("source view");
    // Members arrive: the parent retires with no echo proof.
    let refused = accepted(
        &mut engine,
        &report_with_screens(
            5,
            5,
            vec![screen("out-a", 0), screen("out-b", 800)],
            vec![fact("win-a", "out-b", 800), fact("win-b", "out-b", 900)],
            "",
            None,
            None,
            vec![],
            vec![],
            true,
            vec![],
        ),
    );
    // Members arrive: the parent retires with no echo proof, and the
    // ack stays retired.
    assert!(
        refused.transfers.is_empty(),
        "arrival retires the parent: {:?}",
        refused.transfers
    );
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
        acked.transfers.is_empty(),
        "retired parent stays retired: {:?}",
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
    // The former logical-selector refusal proof is invalid in v4.
    // v4 carries no view-echo lane; the adapter echoes here were inert
    // (native-free engines never stage or consult the views lane), so
    // the v4 port observes the same facts with an empty lane.
    let external = report_with_screens(
        7,
        7,
        vec![screen("out-a", 0), screen("out-b", 800)],
        vec![fact("win-a", "out-b", 800), fact("win-b", "out-b", 900)],
        "",
        None,
        None,
        vec![],
        vec![],
        true,
        vec![],
    );
    let mut invalid = external.clone();
    invalid.view_attempts.push(ViewAttempt {
        operation_id: refill_op.clone(),
        target_output: OutputId("out-a".to_owned()),
        target_workspace: refill_ws.clone(),
        setter: FrameSetter::Refused,
        pre: Some(native_ref("out-a", &view_a)),
        post: Some(native_ref("out-a", &view_a)),
        baseline: Some(native_ref("out-a", &refill_ws)),
        attempt_ms: 7,
    });
    match engine.handle_report(&invalid) {
        WorldOutcome::Rejected { message, .. } => {
            assert_eq!(message, "native-without-capability")
        }
        other => panic!("logical ViewAttempt must reject, got {other:?}"),
    }
    let settled = accepted(&mut engine, &external);
    assert!(
        settled.transfers.is_empty(),
        "nothing retained after the external move: {:?} (third={third:?} refill={refill_ws:?} pre={view_a:?})",
        settled.transfers
    );
    assert_eq!(
        settled.windows.len(),
        2,
        "both migrated members remain represented"
    );
}

#[test]
fn migrate_directional_resolves_action_context_without_adapter_preselect() {
    // Seed one window per output; views name the live workspace per wire.
    fn seeded() -> (WorldEngine, String, String) {
        let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
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
    ) -> WorldReportV4 {
        WorldReportV4 {
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
    fn migrated(reply: &crate::world_report::WorldReplyV4) -> (String, String) {
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
        WorldOutcome::Accepted(reply) => assert_eq!(
            reply.command_result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::MigrationRefused
            }),
            "stale cache must refuse the source"
        ),
        other => panic!("stale cache must refuse the source, got {other:?}"),
    }
    // No known registry at all refuses without inventing focus. The
    // struct-update base carries a registry, so clear it explicitly:
    // empty screens plus an empty registry resolve nothing.
    let (mut engine, _, _) = seeded();
    let empty = WorldReportV4 {
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
        WorldOutcome::Accepted(reply) => assert_eq!(
            reply.command_result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::MigrationRefused
            }),
            "empty registry must refuse the source"
        ),
        other => panic!("empty registry must refuse the source, got {other:?}"),
    }
}

#[test]
fn migrate_directional_freezes_source_view_pre_observation() {
    // The direction-only source (output plus its current view) freezes at
    // run_accepted entry, before staging can converge views or settle
    // rehoming: first-tick unbootstrapped migration refuses like the old
    // pre-tick logical_view lookup, while the same command after a prior
    // observation applies.
    // Fresh engine, no prior observation: no retained view exists, so the
    // frozen source is unknown and the command refuses without mutation.
    let mut fresh = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
    let unbootstrapped = WorldReportV4 {
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
        WorldOutcome::Accepted(reply) => assert_eq!(
            reply.command_result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::MigrationRefused
            }),
            "unbootstrapped migration must refuse"
        ),
        other => panic!("unbootstrapped migration must refuse, got {other:?}"),
    }
    // The same command after a prior observation applies: the retained
    // view resolves the frozen source.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
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
        &WorldReportV4 {
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
    // A newly appearing screen must not supply the migration source view:
    // the frozen source predates staging, so the unknown pre-observation
    // view refuses instead of migrating the newly converged workspace.
    // A follow-up report with the now-retained view applies.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
    accepted(
        &mut engine,
        &report_with_screens(
            1,
            1,
            vec![screen("out-a", 0)],
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
    let hotplug = WorldReportV4 {
        screens: vec![screen("out-a", 0), screen("out-b", 800)],
        cursor: Some((900, 10)),
        action_outputs: vec![OutputId("out-a".to_owned()), OutputId("out-b".to_owned())],
        primary_output: None,
        held_screens: Vec::new(),
        held_windows: Vec::new(),
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
        WorldOutcome::Accepted(reply) => assert_eq!(
            reply.command_result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::MigrationRefused
            }),
            "hotplug source must refuse without a pre-observation view"
        ),
        other => panic!("hotplug source must refuse without a pre-observation view, got {other:?}"),
    }
    // The observation-only catch-up retains the new screen's view, so the
    // same context on the next report applies instead of refusing.
    accepted(
        &mut engine,
        &report_with_screens(
            3,
            3,
            vec![screen("out-a", 0), screen("out-b", 800)],
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
        &WorldReportV4 {
            screens: vec![screen("out-a", 0), screen("out-b", 800)],
            cursor: Some((900, 10)),
            action_outputs: vec![OutputId("out-a".to_owned()), OutputId("out-b".to_owned())],
            primary_output: None,
            held_screens: Vec::new(),
            held_windows: Vec::new(),
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
    // The empty directional move commits (Migration provenance in the
    // command result) but retains no flight: zero members means zero
    // membership to await, so the issuing reply carries no Migration
    // transfer.
    let target = match &reply.command_result {
        Some(CommandResult::Migration { target, .. }) => target.clone(),
        other => panic!("migration applies once the view is retained: {:?}", other),
    };
    assert_eq!(target.output.0, "out-a");
    assert!(
        reply
            .transfers
            .iter()
            .all(|t| !matches!(t.kind, TransferKind::Migration)),
        "empty directional retains no flight: {:?}",
        reply.transfers
    );
}

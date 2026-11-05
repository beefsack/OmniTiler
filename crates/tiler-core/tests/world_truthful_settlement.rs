//! Truthful settlement regression (stage 2a): contradictory logical receipts
//! never prove arrival, host-owned unknown membership never proves arrival,
//! missing/empty/stale/external views hold narrowly, PossiblyExecuted stays
//! held across ack disappearance, and explicit closes settle across gaps.

use tiler_core::directional::{OutputId, WindowId, WorkspaceId};
use tiler_core::geometry::Rect;
use tiler_core::size_hints::WindowSizeHints;
use tiler_core::world_engine::{WorldEngine, WorldOutcome};
use tiler_core::world_report::{
    FrameSetter, MemberAttempt, ScopeAuthority, ScreenReport, ScreenView, SwitchFocus,
    SwitchOutcome, TransferKind, WORLD_CONTRACT_VERSION, WindowFact, WorkspaceRef, WorldCommand,
    WorldReport,
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
        executable: Some("synthetic.exe".to_owned()),
        process_id: Some("foreign-pid".to_owned()),
        class: Some("Synthetic".to_owned()),
        title: None,
        window_type: Some(tiler_core::world_report::WindowType::Normal),
        activation_policy: Some(tiler_core::world_report::ActivationPolicy::Regular),
        layer: Some(0),
        fullscreen_owner: None,
        fullscreen_exitable: None,
        suppression_available: None,
        visibility: tiler_core::world_report::VisibilityFact {
            visible: Some(true),
            suppression_owned: None,
        },
        last_write: None,
    }
}

#[allow(clippy::too_many_arguments)]
fn report(
    request_seq: u64,
    observation_seq: u64,
    windows: Vec<WindowFact>,
    views: Vec<(&str, &str)>,
    command_seq: Option<u64>,
    command: Option<WorldCommand>,
    member_attempts: Vec<MemberAttempt>,
    inventory_complete: bool,
) -> WorldReport {
    WorldReport {
        contract_version: WORLD_CONTRACT_VERSION,
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        observation_seq,
        request_seq,
        request_id: format!("req-{request_seq}"),
        command_seq,
        command,
        screens: vec![screen("out-a", 0), screen("out-b", 800)],
        windows: windows.clone(),
        views: views
            .into_iter()
            .map(|(output, view)| ScreenView {
                output: OutputId(output.to_owned()),
                view: view.to_owned(),
            })
            .collect(),
        closed: vec![],
        native: vec![],
        focused_global: windows
            .first()
            .map(|f| f.window.0.clone())
            .unwrap_or_default(),
        action_foreground: None,
        action_cached: None,
        action_outputs: Vec::new(),
        primary_output: None,
        held_screens: Vec::new(),
        held_windows: Vec::new(),
        inventory_complete,
        gesture_barrier: false,
        gesture: None,
        cursor: None,
        cursor_hit: None,
        monotonic_ms: 0,
        frames: Vec::new(),
        member_attempts,
        view_attempts: Vec::new(),
        clear_attempts: Vec::new(),
        creation_attempts: Vec::new(),
        state_attempts: Vec::new(),
        focus_feedback: Vec::new(),
        presentation: tiler_core::world_report::PresentationIngress::disabled(),

        intent_records: Vec::new(),
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

fn view_of(reply: &tiler_core::world_report::WorldReply, output: &str) -> Option<String> {
    reply
        .outputs
        .iter()
        .find(|row| row.output.0 == output)
        .and_then(|row| row.logical_view.clone())
}

#[test]
fn contradictory_receipt_never_proves_send_arrival() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![fact("win-a", "out-a", 0)],
            vec![],
            None,
            None,
            vec![],
            true,
        ),
    );
    let ring: Vec<String> = first
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    let target = ring[1].clone();
    let other = ring[0].clone();
    accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", 0)],
            vec![],
            Some(11),
            Some(WorldCommand::Send {
                window: WindowId("win-a".to_owned()),
                target_output: OutputId("out-a".to_owned()),
                target_workspace: target.clone(),
                follow: false,
            }),
            vec![],
            true,
        ),
    );
    let wrong = fact("win-a", "out-a", 0);
    let held = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![wrong],
            vec![],
            None,
            None,
            vec![MemberAttempt {
                operation_id: "op-11".to_owned(),
                window: WindowId("win-a".to_owned()),
                target_output: OutputId("out-a".to_owned()),
                target_workspace: target.clone(),
                setter: FrameSetter::Accepted,
                pre: None,
                post: Some(WorkspaceRef::Logical {
                    output: OutputId("out-a".to_owned()),
                    workspace: WorkspaceId(other.clone()),
                }),
                attempt_ms: 11,
            }],
            true,
        ),
    );
    assert_eq!(
        held.transfers.len(),
        1,
        "contradictory holds: {:?}",
        held.transfers
    );
}

#[test]
fn migration_contradictory_receipt_does_not_settle() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![fact("win-a", "out-a", 0)],
            vec![],
            None,
            None,
            vec![],
            true,
        ),
    );
    let view_a = view_of(&first, "out-a").expect("view a");
    let issued = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", 0)],
            vec![],
            Some(11),
            Some(WorldCommand::MigrateWorkspace {
                source_output: OutputId("out-a".to_owned()),
                target_output: OutputId("out-b".to_owned()),
                workspace: view_a.clone(),
            }),
            vec![],
            true,
        ),
    );
    // Genuine one-member control-eligible issuing shape, never vacuous:
    // exactly one Migration flight for op-11 with a Controlled member row.
    assert_eq!(
        issued.transfers.len(),
        1,
        "one issuing flight: {:?}",
        issued.transfers
    );
    assert!(
        issued
            .transfers
            .iter()
            .any(|t| t.operation_id == "op-11" && matches!(t.kind, TransferKind::Migration)),
        "migration flight issued: {:?}",
        issued.transfers
    );
    let issued_row = issued
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-a")
        .expect("issuing member row");
    assert_eq!(
        issued_row.membership_authority,
        ScopeAuthority::Controlled,
        "issuing member control-eligible: {:?}",
        issued_row
    );
    // Member observed on the target screen but naming a different workspace
    // must not settle the migration.
    let ring_b: Vec<String> = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![fact("win-a", "out-a", 0)],
            vec![],
            None,
            None,
            vec![],
            true,
        ),
    )
    .workspaces
    .iter()
    .filter(|row| row.output.0 == "out-b")
    .map(|row| row.id.clone())
    .collect();
    let wrong_ws = ring_b
        .into_iter()
        .find(|id| id != &view_a)
        .expect("other target workspace");
    // Contradictory raw while the fact remains SOURCE-readable must hold:
    // the member is still observed at out-a, and the Accepted post names a
    // different workspace than the migration target, so nothing proves.
    let held = accepted(
        &mut engine,
        &report(
            4,
            4,
            vec![fact("win-a", "out-a", 0)],
            vec![],
            None,
            None,
            vec![MemberAttempt {
                operation_id: "op-11".to_owned(),
                window: WindowId("win-a".to_owned()),
                target_output: OutputId("out-b".to_owned()),
                target_workspace: view_a.clone(),
                setter: FrameSetter::Accepted,
                pre: None,
                post: Some(WorkspaceRef::Logical {
                    output: OutputId("out-b".to_owned()),
                    workspace: WorkspaceId(wrong_ws.clone()),
                }),
                attempt_ms: 11,
            }],
            true,
        ),
    );
    assert!(
        held.transfers
            .iter()
            .any(|t| matches!(t.kind, TransferKind::Migration)),
        "contradictory migration holds: {:?}",
        held.transfers
    );
    // Independent physical arrival at the actual target retires the parent
    // with no view echoes, despite the old ambiguous raw above.
    let arrived = accepted(
        &mut engine,
        &report(
            5,
            5,
            vec![fact("win-a", "out-b", 800)],
            vec![],
            None,
            None,
            vec![],
            true,
        ),
    );
    assert!(
        arrived
            .transfers
            .iter()
            .all(|t| !matches!(t.kind, TransferKind::Migration)),
        "real arrival retires despite stale raw: {:?}",
        arrived.transfers
    );
}

#[test]
fn host_owned_unknown_holds_exact_settles() {
    use tiler_core::world_report::NativeObservation;
    // Host-owned mode is native-only: every step carries ordered-desktop
    // observations plus per-window native membership. Unknown native
    // membership holds; the mapped native receipt plus the exact
    // follow-view echo settles.
    const NA1: &str = "n-a1";
    const NA2: &str = "n-a2";
    const NB1: &str = "n-b1";
    fn native_rows() -> Vec<NativeObservation> {
        vec![
            NativeObservation {
                output: OutputId("out-a".to_owned()),
                native_order: vec![NA1.to_owned(), NA2.to_owned()],
                current_native: Some(NA1.to_owned()),
                occupancy: vec![
                    tiler_core::world_report::NativeOccupancy {
                        native: NA1.to_owned(),
                        count: None,
                    },
                    tiler_core::world_report::NativeOccupancy {
                        native: NA2.to_owned(),
                        count: None,
                    },
                ],
            },
            NativeObservation {
                output: OutputId("out-b".to_owned()),
                native_order: vec![NB1.to_owned()],
                current_native: Some(NB1.to_owned()),
                occupancy: vec![tiler_core::world_report::NativeOccupancy {
                    native: NB1.to_owned(),
                    count: None,
                }],
            },
        ]
    }
    fn hfact(window: &str, output: &str, x: i32, native: Option<&str>) -> WindowFact {
        let mut f = fact(window, output, x);
        f.native_workspace = native
            .map(|s| tiler_core::world_report::NativeWorkspaceFact::Value(s.to_owned()))
            .unwrap_or(tiler_core::world_report::NativeWorkspaceFact::Omitted);
        f
    }
    fn with_native(mut r: WorldReport) -> WorldReport {
        r.native = native_rows();
        r
    }

    // Window send, cross-output, host-owned on.
    let mut engine = WorldEngine::new_test_host_owned("gen-1", "adapter-1").expect("generation");
    let first = accepted(
        &mut engine,
        &with_native(report(
            1,
            1,
            vec![hfact("win-a", "out-a", 0, Some(NA1))],
            vec![],
            None,
            None,
            vec![],
            true,
        )),
    );
    let view_b = view_of(&first, "out-b").expect("view b");
    accepted(
        &mut engine,
        &with_native(report(
            2,
            2,
            vec![hfact("win-a", "out-a", 0, Some(NA1))],
            vec![],
            Some(11),
            Some(WorldCommand::SendDirectional {
                window: WindowId("win-a".to_owned()),
                direction: "right".to_owned(),
                follow: true,
            }),
            vec![],
            true,
        )),
    );
    // Raw screen arrival with unknown membership must not settle while
    // host-owned (no translated logical receipt yet).
    let held = accepted(
        &mut engine,
        &with_native(report(
            3,
            3,
            vec![hfact("win-a", "out-b", 800, None)],
            vec![],
            None,
            None,
            vec![],
            true,
        )),
    );
    assert_eq!(
        held.transfers.len(),
        1,
        "host-owned raw holds: {:?}",
        held.transfers
    );
    // Exact native receipt plus the exact follow-view echo settles: a
    // cross-output follow parent never settles on membership alone while
    // the target view is still stale. The native receipt proves.
    let exact = hfact("win-a", "out-b", 800, Some(NB1));
    let settled = accepted(
        &mut engine,
        &with_native(report(
            4,
            4,
            vec![exact],
            vec![("out-b", view_b.as_str())],
            None,
            None,
            vec![],
            true,
        )),
    );
    assert!(
        settled.transfers.is_empty(),
        "exact settles: {:?}",
        settled.transfers
    );

    // Migration, host-owned on: unknown at target holds.
    let mut engine = WorldEngine::new_test_host_owned("gen-1", "adapter-1").expect("generation");
    let first = accepted(
        &mut engine,
        &with_native(report(
            1,
            1,
            vec![hfact("win-a", "out-a", 0, Some(NA1))],
            vec![],
            None,
            None,
            vec![],
            true,
        )),
    );
    let view_a = view_of(&first, "out-a").expect("view a");
    accepted(
        &mut engine,
        &with_native(report(
            2,
            2,
            vec![hfact("win-a", "out-a", 0, Some(NA1))],
            vec![],
            Some(11),
            Some(WorldCommand::MigrateWorkspace {
                source_output: OutputId("out-a".to_owned()),
                target_output: OutputId("out-b".to_owned()),
                workspace: view_a.clone(),
            }),
            vec![],
            true,
        )),
    );
    let held = accepted(
        &mut engine,
        &with_native(report(
            3,
            3,
            vec![hfact("win-a", "out-b", 800, None)],
            vec![],
            None,
            None,
            vec![],
            true,
        )),
    );
    assert!(
        held.transfers
            .iter()
            .any(|t| matches!(t.kind, TransferKind::Migration)),
        "host-owned migration raw holds: {:?}",
        held.transfers
    );
}

#[test]
fn empty_focusless_issues_terminal_without_echo_resurrection() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    accepted(
        &mut engine,
        &report(1, 1, vec![], vec![], None, None, vec![], true),
    );
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![],
            vec![],
            Some(11),
            Some(WorldCommand::SelectNumbered {
                output: Some(OutputId("out-a".to_owned())),
                index: 2,
            }),
            vec![],
            true,
        ),
    );
    // Empty native-free focusless selection completes in the issuing
    // reply: one terminal on the existing op, no focus effect, no view
    // flight (the actual view parent retires at once).
    assert!(
        reply.transfers.is_empty(),
        "empty focusless issues no flight: {:?}",
        reply.transfers
    );
    assert!(
        reply.effects.is_empty(),
        "empty focusless invents no focus: {:?}",
        reply.effects
    );
    assert_eq!(
        reply.switch_settled.len(),
        1,
        "zero obligations complete at issuing: {:?}",
        reply.switch_settled
    );
    let event = &reply.switch_settled[0];
    assert_eq!(event.operation_id, "op-11");
    assert_eq!(event.outcome, SwitchOutcome::Complete);
    assert!(event.failed.is_empty());
    assert_eq!(event.focus, SwitchFocus::Unproven);
    assert!(!event.fallback);
    let target = view_of(&reply, "out-a").expect("selected logical view");
    // Missing observation: settled parent stays retired, no duplicate.
    let held = accepted(
        &mut engine,
        &report(3, 3, vec![], vec![], None, None, vec![], true),
    );
    assert!(
        held.transfers.is_empty(),
        "missing stays retired: {:?}",
        held.transfers
    );
    assert!(
        held.switch_settled.is_empty(),
        "no duplicate terminal: {:?}",
        held.switch_settled
    );
    // Empty observation: settled parent stays retired, no duplicate.
    let held = accepted(
        &mut engine,
        &report(4, 4, vec![], vec![("out-a", "")], None, None, vec![], true),
    );
    assert!(
        held.transfers.is_empty(),
        "empty stays retired: {:?}",
        held.transfers
    );
    assert!(
        held.switch_settled.is_empty(),
        "no duplicate terminal: {:?}",
        held.switch_settled
    );
    // Stale observation: settled parent stays retired, no duplicate.
    let held = accepted(
        &mut engine,
        &report(5, 5, vec![], vec![("out-a", "")], None, None, vec![], true),
    );
    assert!(
        held.transfers.is_empty(),
        "stale stays retired: {:?}",
        held.transfers
    );
    assert!(
        held.switch_settled.is_empty(),
        "no duplicate terminal: {:?}",
        held.switch_settled
    );
    // External unknown observation: settled parent stays retired, no duplicate.
    let held = accepted(
        &mut engine,
        &report(
            6,
            6,
            vec![],
            vec![("out-a", "ws-999")],
            None,
            None,
            vec![],
            true,
        ),
    );
    assert!(
        held.transfers.is_empty(),
        "unknown stays retired: {:?}",
        held.transfers
    );
    assert!(
        held.switch_settled.is_empty(),
        "no duplicate terminal: {:?}",
        held.switch_settled
    );
    // Exact observation: settled parent stays retired, no duplicate.
    let settled = accepted(
        &mut engine,
        &report(
            7,
            7,
            vec![],
            vec![("out-a", target.as_str())],
            None,
            None,
            vec![],
            true,
        ),
    );
    assert!(
        settled.transfers.is_empty(),
        "exact stays retired: {:?}",
        settled.transfers
    );
    assert!(
        settled.switch_settled.is_empty(),
        "no duplicate terminal: {:?}",
        settled.switch_settled
    );
}

#[test]
fn possibly_executed_persists_held_across_disappearance() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![fact("win-a", "out-a", 0)],
            vec![],
            None,
            None,
            vec![],
            true,
        ),
    );
    let ring: Vec<String> = first
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    let target = ring[1].clone();
    let source = first
        .outputs
        .iter()
        .find(|row| row.output.0 == "out-a")
        .and_then(|row| row.logical_view.clone())
        .expect("source view");
    accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", 0)],
            vec![],
            Some(11),
            Some(WorldCommand::Send {
                window: WindowId("win-a".to_owned()),
                target_output: OutputId("out-a".to_owned()),
                target_workspace: target.clone(),
                follow: false,
            }),
            vec![],
            true,
        ),
    );
    // Ambiguous actuation holds.
    let held = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![fact("win-a", "out-a", 0)],
            vec![],
            None,
            None,
            vec![MemberAttempt {
                operation_id: "op-11".to_owned(),
                window: WindowId("win-a".to_owned()),
                target_output: OutputId("out-a".to_owned()),
                target_workspace: target.clone(),
                setter: FrameSetter::Threw,
                pre: Some(WorkspaceRef::Logical {
                    output: OutputId("out-a".to_owned()),
                    workspace: WorkspaceId(source.clone()),
                }),
                post: Some(WorkspaceRef::Logical {
                    output: OutputId("out-a".to_owned()),
                    workspace: WorkspaceId(source),
                }),
                attempt_ms: 3,
            }],
            true,
        ),
    );
    assert_eq!(held.transfers.len(), 1);
    assert!(held.transfers[0].held, "possibly-executed holds");
    // Ack disappearance (no attempt row next report) must not unhold or
    // re-actuate: the flight stays held.
    let still = accepted(
        &mut engine,
        &report(
            4,
            4,
            vec![fact("win-a", "out-a", 0)],
            vec![],
            None,
            None,
            vec![],
            true,
        ),
    );
    assert_eq!(
        still.transfers.len(),
        1,
        "disappearance holds: {:?}",
        still.transfers
    );
    assert!(
        still.transfers[0].held,
        "stays held, never unheld: {:?}",
        still.transfers
    );
}

#[test]
fn explicit_close_settles_across_gaps() {
    // Window close during incomplete inventory settles the flight.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![fact("win-a", "out-a", 0)],
            vec![],
            None,
            None,
            vec![],
            true,
        ),
    );
    let first = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", 0)],
            vec![],
            None,
            None,
            vec![],
            true,
        ),
    );
    let ring: Vec<String> = first
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![fact("win-a", "out-a", 0)],
            vec![],
            Some(11),
            Some(WorldCommand::Send {
                window: WindowId("win-a".to_owned()),
                target_output: OutputId("out-a".to_owned()),
                target_workspace: ring[1].clone(),
                follow: false,
            }),
            vec![],
            true,
        ),
    );
    let mut incomplete = report(4, 4, vec![], vec![], None, None, vec![], false);
    incomplete.closed = vec![WindowId("win-a".to_owned())];
    let settled = accepted(&mut engine, &incomplete);
    assert!(
        settled.transfers.is_empty(),
        "close settles across gap: {:?}",
        settled.transfers
    );

    // Migration fully closed during incomplete inventory settles.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            vec![fact("win-a", "out-a", 0)],
            vec![],
            None,
            None,
            vec![],
            true,
        ),
    );
    let view_a = view_of(&first, "out-a").expect("view a");
    accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![fact("win-a", "out-a", 0)],
            vec![],
            Some(11),
            Some(WorldCommand::MigrateWorkspace {
                source_output: OutputId("out-a".to_owned()),
                target_output: OutputId("out-b".to_owned()),
                workspace: view_a,
            }),
            vec![],
            true,
        ),
    );
    let mut closed = report(3, 3, vec![], vec![], None, None, vec![], false);
    closed.closed = vec![WindowId("win-a".to_owned())];
    let settled = accepted(&mut engine, &closed);
    assert!(
        settled
            .transfers
            .iter()
            .all(|t| !matches!(t.kind, TransferKind::Migration)),
        "closed migration settles across gap: {:?}",
        settled.transfers
    );
}

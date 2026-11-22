//! Logical-first world contract tests (stage 2a): native-free bootstrap
//! and lifecycle, sends and selection, and the source-only-backing
//! disjoint migration discriminator with independent arrival/view/refill
//! settlement. No native workspace concept anywhere: screens plus window
//! facts are the only observations, and every workspace id is core-minted
//! (`ws-<n>`).

use crate::directional::{OutputId, WindowId, WorkspaceId};
use crate::geometry::Rect;
use crate::size_hints::WindowSizeHints;
use crate::world_engine::{WorldEngine, WorldOutcome};
use crate::world_report::{
    CommandRejectKind, CommandResult, FrameSetter, HoldReason, LastWritePath, MemberAttempt,
    PresentationIngressV4, PresentationRowV4, ScopeAuthority, ScreenReport, SendSource,
    TransferKind, VisibilityAttempt, VisibilityFact, VisibilityReason, VisibilitySample,
    VisibilitySetter, WORLD_CONTRACT_VERSION_V4, WindowFact, WorkspaceRef, WorldCommand,
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
        // Honest synthetic known identity per live handle: these return
        // tests exercise the same-panel same-name premise, which physical
        // reconnect matching proves via the tuple rather than the name.
        identity: crate::world_report::OutputIdentity {
            manufacturer: Some("SynthMfg".to_owned()),
            model: Some(format!("SynthModel-{output}")),
            serial: Some(format!("serial-{output}")),
            connector: None,
        },
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
    }
}

fn minimized_fact(window: &str, output: &str) -> WindowFact {
    WindowFact {
        minimized: Some(true),
        rect: None,
        ..fact(window, output, 0)
    }
}

#[allow(clippy::too_many_arguments)]
fn report(
    request_seq: u64,
    observation_seq: u64,
    screens: Vec<ScreenReport>,
    windows: Vec<WindowFact>,
    views: Vec<(&str, &str)>,
    closed: Vec<&str>,
    focused: &str,
    command_seq: Option<u64>,
    command: Option<WorldCommand>,
    inventory_complete: bool,
    gesture_barrier: bool,
) -> WorldReportV4 {
    report_mode(
        request_seq,
        observation_seq,
        screens,
        windows,
        views,
        closed,
        focused,
        command_seq,
        command,
        inventory_complete,
        gesture_barrier,
        crate::workspace_scope::WorkspaceMode::PerOutputLocal,
    )
}

#[allow(clippy::too_many_arguments)]
fn report_mode(
    request_seq: u64,
    observation_seq: u64,
    screens: Vec<ScreenReport>,
    windows: Vec<WindowFact>,
    views: Vec<(&str, &str)>,
    closed: Vec<&str>,
    focused: &str,
    command_seq: Option<u64>,
    command: Option<WorldCommand>,
    inventory_complete: bool,
    gesture_barrier: bool,
    workspace_mode: crate::workspace_scope::WorkspaceMode,
) -> WorldReportV4 {
    report_attempts(
        request_seq,
        observation_seq,
        screens,
        windows,
        views,
        closed,
        focused,
        command_seq,
        command,
        inventory_complete,
        gesture_barrier,
        workspace_mode,
        Vec::new(),
        Vec::new(),
    )
}

#[allow(clippy::too_many_arguments)]
fn report_attempts(
    request_seq: u64,
    observation_seq: u64,
    screens: Vec<ScreenReport>,
    windows: Vec<WindowFact>,
    views: Vec<(&str, &str)>,
    closed: Vec<&str>,
    focused: &str,
    command_seq: Option<u64>,
    command: Option<WorldCommand>,
    inventory_complete: bool,
    gesture_barrier: bool,
    _workspace_mode: crate::workspace_scope::WorkspaceMode,
    member_attempts: Vec<crate::world_report::MemberAttempt>,
    view_attempts: Vec<crate::world_report::ViewAttempt>,
) -> WorldReportV4 {
    assert!(
        views.is_empty(),
        "v4 has no views lane: {} echo(s) cannot convert",
        views.len()
    );
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
        // v4 carries no view-echo lane; the adapter echoes here were
        // inert (the staged echo derives solely from the native
        // current map, never from this lane), so the v4 port keeps the
        // parameter and asserts it stays empty.
        closed: closed
            .into_iter()
            .map(|id| WindowId(id.to_owned()))
            .collect(),
        native: Vec::new(),
        held_native: Vec::new(),
        cursor: None,
        cursor_hit: None,
        focused_global: focused.to_owned(),
        foreground_known: !focused.is_empty(),
        action_foreground: None,
        action_cached: None,
        action_outputs: Vec::new(),
        primary_output: None,
        inventory_complete,
        gesture_barrier,
        gesture: None,
        monotonic_ms: 0,
        frames: Vec::new(),
        member_attempts,
        view_attempts,
        creation_attempts: Vec::new(),
        held_attempts: Vec::new(),
        state_attempts: Vec::new(),
        visibility_attempts: Vec::new(),
        focus_changes: None,
        focus_feedback: Vec::new(),
        presentation: PresentationRowV4::Valid(PresentationIngressV4::default()),
        intent_records: Vec::new(),
        renderer_input: None,
    }
}

fn two_screens() -> Vec<ScreenReport> {
    vec![screen("out-a", 0), screen("out-b", 800)]
}

fn accepted(engine: &mut WorldEngine, report: &WorldReportV4) -> crate::world_report::WorldReplyV4 {
    match engine.handle_report(report, &[]).0 {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    }
}

fn engine_with_mode(mode: crate::workspace_scope::WorkspaceMode) -> WorldEngine {
    let token = match mode {
        crate::workspace_scope::WorkspaceMode::PerOutputLocal => "per-output-local",
        crate::workspace_scope::WorkspaceMode::GlobalUnique => "global-unique",
        crate::workspace_scope::WorkspaceMode::Shared => "shared",
    };
    WorldEngine::new_test_bound_with_initial_settings(
        "gen-1",
        "adapter-1",
        crate::connection::HostCapabilities {
            // Fixtures carry native sticky and maximize facts; the lane
            // under test is the native route.
            native_sticky: true,
            maximize: true,
            ..Default::default()
        },
        vec![(
            "mode".to_owned(),
            crate::connection::RawSettingValue::String(token.to_owned()),
        )],
    )
    .expect("valid generation")
}

fn with_native_context(
    mut report: WorldReportV4,
    current_a: &str,
    current_b: &str,
) -> WorldReportV4 {
    use crate::world_report::{NativeObservation, NativeOccupancy};
    report.native = [
        ("out-a", ["native-a-0", "native-a-1"], current_a),
        ("out-b", ["native-b-0", "native-b-1"], current_b),
    ]
    .into_iter()
    .map(|(output, order, current)| NativeObservation {
        output: OutputId(output.to_owned()),
        native_order: order.into_iter().map(str::to_owned).collect(),
        current_native: Some(current.to_owned()),
        occupancy: order
            .into_iter()
            .map(|native| NativeOccupancy {
                native: native.to_owned(),
                count: None,
            })
            .collect(),
    })
    .collect();
    report
}

fn workspace_ids(reply: &crate::world_report::WorldReplyV4, output: &str) -> Vec<String> {
    let mut ids: Vec<String> = reply
        .workspaces
        .iter()
        .filter(|row| row.output.0 == output)
        .map(|row| row.id.clone())
        .collect();
    ids.sort();
    ids
}

fn scope_of(
    reply: &crate::world_report::WorldReplyV4,
    output: &str,
    workspace: &str,
) -> ScopeAuthority {
    reply
        .scopes
        .iter()
        .find(|status| {
            status
                .scope
                .covers_workspace(&OutputId(output.to_owned()), workspace)
        })
        .unwrap_or_else(|| panic!("missing scope {output}/{workspace}"))
        .authority
}

fn window_row<'a>(
    reply: &'a crate::world_report::WorldReplyV4,
    window: &str,
) -> &'a crate::world_report::WindowDesire {
    reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == window)
        .unwrap_or_else(|| panic!("missing window row {window}"))
}

fn output_view(reply: &crate::world_report::WorldReplyV4, output: &str) -> Option<String> {
    reply
        .outputs
        .iter()
        .find(|row| row.output.0 == output)
        .and_then(|row| row.logical_view.clone())
}

#[test]
fn native_free_bootstrap_creates_per_screen_rings_views_and_trailing() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
    let reply = accepted(
        &mut engine,
        &report(
            1,
            1,
            two_screens(),
            vec![],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            false,
        ),
    );
    assert!(reply.ready);
    // One trailing spare per screen (single-spare invariant), stable ids.
    let ring_a = workspace_ids(&reply, "out-a");
    let ring_b = workspace_ids(&reply, "out-b");
    assert_eq!(ring_a.len(), 1, "ring: {ring_a:?}");
    assert_eq!(ring_b.len(), 1, "ring: {ring_b:?}");
    assert_ne!(ring_a[0], ring_b[0]);
    // Views show the heads; outputs carry the same logical views.
    let view_a = output_view(&reply, "out-a").expect("view a");
    let view_b = output_view(&reply, "out-b").expect("view b");
    assert_eq!(view_a, ring_a[0]);
    assert_eq!(view_b, ring_b[0]);
    // All scopes controlled with no facts at all.
    for scope in &reply.scopes {
        assert_eq!(
            scope.authority,
            ScopeAuthority::Controlled,
            "{:?}",
            scope.scope
        );
    }
    // Untouched workspaces authorize nothing yet: no windows, no transfers.
    assert!(reply.windows.is_empty());
    assert!(reply.transfers.is_empty());
}

#[test]
fn admission_homes_facts_to_views_and_lifecycle_grows_trailing() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            two_screens(),
            vec![],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            false,
        ),
    );
    let view_a = output_view(&first, "out-a").expect("view a");
    // Two windows arrive on out-a: both home to the current view.
    let second = accepted(
        &mut engine,
        &report(
            2,
            2,
            two_screens(),
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            false,
        ),
    );
    for win in ["win-a", "win-b"] {
        let row = window_row(&second, win);
        assert_eq!(row.output.0, "out-a");
        assert_eq!(row.workspace, view_a, "{row:?}");
        assert_eq!(row.membership_authority, ScopeAuthority::Controlled);
        assert_eq!(row.frame_authority, ScopeAuthority::Controlled);
        assert!(row.allocation.is_some());
    }
    // The trailing empty survives alongside occupancy.
    let ring_a = workspace_ids(&second, "out-a");
    assert_eq!(ring_a.len(), 2, "ring: {ring_a:?}");
    // Closing both windows returns the screen to empties without churn.
    let third = accepted(
        &mut engine,
        &report(
            3,
            3,
            two_screens(),
            vec![],
            vec![],
            vec!["win-a", "win-b"],
            "",
            None,
            None,
            true,
            false,
        ),
    );
    assert!(third.windows.iter().all(|row| row.lifetime.0 != "win-a"));
    let ring_a = workspace_ids(&third, "out-a");
    assert_eq!(ring_a.len(), 2, "ring: {ring_a:?}");
}

#[test]
fn fresh_admission_focus_is_at_most_once() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
    accepted(
        &mut engine,
        &report(
            1,
            1,
            two_screens(),
            vec![],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            false,
        ),
    );
    // A genuinely new lifetime mints one admission intent.
    let second = accepted(
        &mut engine,
        &report(
            2,
            2,
            two_screens(),
            vec![fact("win-a", "out-a", 0)],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            false,
        ),
    );
    let request = second.focus_request.as_ref().expect("admission request");
    assert_eq!(request.target.0, "win-a");
    assert_eq!(request.id, format!("focus-{}", second.state_revision));
    assert_eq!(request.issued_revision, second.state_revision);
    assert_eq!(
        request.cause,
        crate::world_report::FocusRequestCause::AutomaticAdmission
    );
    assert_eq!(request.cmd_seq, None);
    assert_eq!(request.after_shown, None);
    // Re-observation never replays it, even on a fresh sequence.
    let third = accepted(
        &mut engine,
        &report(
            3,
            3,
            two_screens(),
            vec![fact("win-a", "out-a", 0)],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            false,
        ),
    );
    assert!(third.focus_request.is_some());
}

#[test]
fn send_follow_and_stay_commit_window_transfers() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            two_screens(),
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            false,
        ),
    );
    let view_b = output_view(&first, "out-b").expect("view b");
    // Follow send: mover transfers, effect focuses the mover.
    let mut follow = report(
        2,
        2,
        two_screens(),
        vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
        vec![],
        vec![],
        "win-a",
        Some(11),
        Some(WorldCommand::SendDirectional {
            window: crate::directional::WindowSubject::Window(WindowId("win-a".to_owned())),
            direction: crate::directional::Direction::Right,
            follow: true,
        }),
        true,
        false,
    );
    let reply = accepted(&mut engine, &follow);
    let request = reply
        .focus_request
        .as_ref()
        .expect("explicit focus request");
    assert_eq!(request.target.0, "win-a");
    assert_eq!(request.id, format!("focus-{}", reply.state_revision));
    assert_eq!(request.issued_revision, reply.state_revision);
    assert_eq!(
        request.cause,
        crate::world_report::FocusRequestCause::ExplicitCommand
    );
    assert_eq!(request.cmd_seq, Some(11));
    assert_eq!(request.after_shown, None);
    assert_eq!(reply.transfers.len(), 1);
    assert_eq!(reply.transfers[0].operation_id, "op-11");
    // Before arrival the mover publishes Controlled membership desire at
    // the target with its actionable target slot (core-owned geometry
    // actuated only as part of the membership move, never a native remap
    // guess), and the source does not flap.
    let row = window_row(&reply, "win-a");
    assert_eq!(
        (row.output.0.as_str(), row.workspace.as_str()),
        ("out-b", view_b.as_str())
    );
    assert_eq!(row.membership_authority, ScopeAuthority::Controlled);
    assert_eq!(row.frame_authority, ScopeAuthority::Controlled);
    assert!(row.effective_frame.is_some(), "{row:?}");
    // Arrival proves through the staged homing: same fact on out-b.
    // Follow proves through the staged echo (native current map on
    // host-owned engines; retained view on native-free ones): the v4
    // port observes the same arrival with an empty views lane, which
    // is inert on both intakes.
    follow.request_seq = 3;
    follow.request_id = "req-3".to_string();
    follow.observation_seq = 3;
    follow.command_seq = None;
    follow.command = None;
    follow.windows = vec![fact("win-a", "out-b", 800), fact("win-b", "out-a", 100)];
    let settled = accepted(&mut engine, &follow);
    assert!(settled.transfers.is_empty());
    let row = window_row(&settled, "win-a");
    assert_eq!(row.frame_authority, ScopeAuthority::Controlled);
    assert!(row.allocation.is_some());
    // Stay send leaves a survivor and names exact source focus.
    // (The legacy setup echoed current views here; echoes are inert,
    // so the v4 report carries an empty lane.)
    let stay = report(
        4,
        4,
        two_screens(),
        vec![fact("win-a", "out-b", 800), fact("win-b", "out-a", 100)],
        vec![],
        vec![],
        "win-b",
        Some(12),
        Some(WorldCommand::SendDirectional {
            window: crate::directional::WindowSubject::Window(WindowId("win-b".to_owned())),
            direction: crate::directional::Direction::Right,
            follow: false,
        }),
        true,
        false,
    );
    let reply = accepted(&mut engine, &stay);
    assert_eq!(reply.transfers.len(), 1);
}

#[test]
fn stay_send_with_survivor_names_exact_source_focus() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
    accepted(
        &mut engine,
        &report(
            1,
            1,
            two_screens(),
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
            true,
            false,
        ),
    );
    let _view_b = {
        let reply = accepted(
            &mut engine,
            &report(
                2,
                2,
                two_screens(),
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
                true,
                false,
            ),
        );
        output_view(&reply, "out-b").expect("view b")
    };
    let reply = accepted(
        &mut engine,
        &report(
            3,
            3,
            two_screens(),
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 100),
                fact("win-c", "out-a", 200),
            ],
            vec![],
            vec![],
            "win-a",
            Some(11),
            Some(WorldCommand::SendDirectional {
                window: crate::directional::WindowSubject::Window(WindowId("win-a".to_owned())),
                direction: crate::directional::Direction::Right,
                follow: false,
            }),
            true,
            false,
        ),
    );
    assert_eq!(
        reply.transfers.len(),
        1,
        "transfer retained: {:?}",
        reply.transfers
    );
}

#[test]
fn explicit_send_is_same_output_only_and_directional_send_crosses() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            two_screens(),
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            false,
        ),
    );
    let ring_a = workspace_ids(&first, "out-a");
    assert_eq!(ring_a.len(), 2);
    // Explicit same-output send commits through the workspace path.
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            two_screens(),
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "win-a",
            Some(11),
            Some(WorldCommand::Send {
                window: crate::directional::WindowSubject::Window(WindowId("win-a".to_owned())),
                target_output: OutputId("out-a".to_owned()),
                target_workspace: crate::directional::WorkspaceId(ring_a[1].clone()),
                follow: true,
            }),
            true,
            false,
        ),
    );
    // Same-output follow retains the single window flight under the stable
    // command identity: no `{op}-follow` child, no view flight, no frozen
    // binding (these facts carry no suppression support, so the mover is
    // selection-ineligible and keeps the legacy direct focus). The logical
    // view still moves.
    assert_eq!(reply.transfers.len(), 1);
    assert!(matches!(&reply.transfers[0].kind, TransferKind::Window(_)));
    assert_eq!(reply.transfers[0].target_workspace, ring_a[1]);
    assert_eq!(reply.transfers[0].operation_id, "op-11");
    assert_eq!(
        output_view(&reply, "out-a").as_deref(),
        Some(ring_a[1].as_str()),
        "ineligible follow still moves the logical view"
    );
    // Explicit cross-output send refuses at the engine boundary (parked
    // explicit-vs-directional mismatch): directional sends own the
    // crossing, even when the logical target is known. The prior
    // same-output window flight settles on the member-attempt proof carried
    // in the observation-only report below (a rejected command would
    // discard the settlement with it); the echoed view proves nothing
    // further on this ineligible route.
    let view_b = output_view(&first, "out-b").expect("view b");
    let arrived = fact("win-a", "out-a", 0);
    // The window flight settles on the member-attempt proof; the echoed
    // view is inert here (the follow already moved the logical view at
    // command time with no binding), so the report carries both proofs
    // with no further effect.
    let mut settle = report(
        3,
        3,
        two_screens(),
        vec![arrived, fact("win-b", "out-a", 100)],
        vec![],
        vec![],
        "win-a",
        None,
        None,
        true,
        false,
    );
    settle.member_attempts.push(MemberAttempt {
        operation_id: "op-11".to_owned(),
        window: WindowId("win-a".to_owned()),
        target_output: OutputId("out-a".to_owned()),
        target_workspace: ring_a[1].clone(),
        setter: FrameSetter::Accepted.into(),
        pre: None,
        post: Some(WorkspaceRef::Logical {
            output: OutputId("out-a".to_owned()),
            workspace: WorkspaceId(ring_a[1].clone()),
        }),
        attempt_ms: 11,
    });
    let settled = accepted(&mut engine, &settle);
    assert!(
        settled.transfers.is_empty(),
        "membership proof settles: {:?}",
        settled.transfers
    );
    match engine.handle_report(
        &report(
            4,
            4,
            two_screens(),
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "win-a",
            Some(12),
            Some(WorldCommand::Send {
                window: crate::directional::WindowSubject::Window(WindowId("win-a".to_owned())),
                target_output: OutputId("out-b".to_owned()),
                target_workspace: crate::directional::WorkspaceId(view_b),
                follow: true,
            }),
            true,
            false,
        ),
        &[],
    ) {
        (WorldOutcome::Accepted(_), settlements) => {
            assert_eq!(settlements.len(), 1);
            assert_eq!(
                settlements[0].result,
                Some(CommandResult::Rejected {
                    kind: CommandRejectKind::OperationRefused
                }),
                "expected cross-domain-mismatch"
            );
        }
        (other, _) => panic!("expected cross-domain-mismatch, got {other:?}"),
    }
    // Directional move resolves the strict neighbor locally.
    let reply = accepted(
        &mut engine,
        &report(
            5,
            5,
            two_screens(),
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "win-a",
            Some(13),
            Some(WorldCommand::MoveDirectional {
                window: crate::directional::WindowSubject::Window(WindowId("win-a".to_owned())),
                direction: crate::directional::Direction::Right,
                same_axis: crate::directional::SameAxisMove::GroupWithNeighbor,
            }),
            true,
            false,
        ),
    );
    assert!(
        reply
            .transfers
            .iter()
            .any(|t| matches!(t.kind, TransferKind::Window(_)))
    );
}

fn queued_cross_output_move_dir(window: &str, direction: &str) -> WorldCommand {
    WorldCommand::MoveDirectional {
        window: crate::directional::WindowSubject::Window(WindowId(window.to_owned())),
        direction: crate::directional::Direction::parse_wire(direction).expect("direction"),
        same_axis: crate::directional::SameAxisMove::GroupWithNeighbor,
    }
}

fn queued_cross_output_bootstrap() -> WorldEngine {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
    accepted(
        &mut engine,
        &report(
            1,
            1,
            two_screens(),
            vec![fact("win-a", "out-a", 0)],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            false,
        ),
    );
    engine
}

fn queued_cross_output_lagging_capture(request_seq: u64) -> WorldReportV4 {
    report(
        request_seq,
        request_seq,
        two_screens(),
        vec![fact("win-a", "out-a", 0)],
        vec![],
        vec![],
        "win-a",
        None,
        None,
        true,
        false,
    )
}

fn assert_queued_cross_output_final_out_a(reply: &crate::world_report::WorldReplyV4) {
    assert_eq!(
        window_row(reply, "win-a").output.0,
        "out-a",
        "right then left returns to out-a: {reply:?}"
    );
    for transfer in reply
        .transfers
        .iter()
        .filter(|t| matches!(&t.kind, TransferKind::Window(_)))
    {
        assert_eq!(
            transfer.target_output.0, "out-a",
            "any final transfer targets latest logical source: {:?}",
            reply.transfers
        );
    }
}

#[test]
fn queued_cross_output_moves_drain_in_order_without_transfer_in_flight() {
    // Ordered-rapid-input regression (`docs/decisions/commands.md`): a
    // queued move after a retained cross-output flight must evaluate the
    // latest logical world without a native ack, never `transfer-in-flight`.
    // Same-batch arm: one lagging native capture carries queued right then left.
    let mut engine = queued_cross_output_bootstrap();
    let capture = queued_cross_output_lagging_capture(2);
    let (outcome, settlements) = engine.handle_report(
        &capture,
        &[
            (11, queued_cross_output_move_dir("win-a", "right")),
            (12, queued_cross_output_move_dir("win-a", "left")),
        ],
    );
    let reply = match outcome {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("same-batch capture must accept, got {other:?}"),
    };
    assert_eq!(
        settlements.len(),
        2,
        "both queued moves settle: {settlements:?}"
    );
    assert_eq!(settlements[0].seq, 11);
    assert_eq!(settlements[1].seq, 12);
    assert_eq!(
        settlements[0].result, None,
        "first cross-output move must settle successful: {:?}",
        settlements[0].result
    );
    assert_eq!(
        settlements[1].result, None,
        "second queued move must evaluate latest logical world and settle successful, not transfer-in-flight; got {:?}",
        settlements[1].result
    );
    assert_queued_cross_output_final_out_a(&reply);
}

#[test]
fn queued_cross_output_moves_split_captures_drain_in_order_without_transfer_in_flight() {
    // Split-capture arm: a later capture still reports the old native source.
    let mut engine = queued_cross_output_bootstrap();
    let first = queued_cross_output_lagging_capture(2);
    let (outcome, settlements) = engine.handle_report(
        &first,
        &[(11, queued_cross_output_move_dir("win-a", "right"))],
    );
    let right_reply = match outcome {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("first crossing must accept, got {other:?}"),
    };
    assert_eq!(settlements.len(), 1);
    assert_eq!(settlements[0].seq, 11);
    assert_eq!(settlements[0].result, None);
    assert_eq!(window_row(&right_reply, "win-a").output.0, "out-b");
    let second = queued_cross_output_lagging_capture(3);
    let (outcome, settlements) = engine.handle_report(
        &second,
        &[(12, queued_cross_output_move_dir("win-a", "left"))],
    );
    let reply = match outcome {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("split-capture capture must accept, got {other:?}"),
    };
    assert_eq!(
        settlements.len(),
        1,
        "queued left settles once: {settlements:?}"
    );
    assert_eq!(settlements[0].seq, 12);
    assert_eq!(
        settlements[0].result, None,
        "split-capture left must succeed against latest logical out-b, not refuse transfer-in-flight; got {:?}",
        settlements[0].result
    );
    assert_queued_cross_output_final_out_a(&reply);
}

#[test]
fn queued_cross_output_moves_alternating_eight_drain_in_order_without_transfer_in_flight() {
    // Eight alternating same-batch moves: every command evaluates the
    // preceding latest logical world, so the batch composes net-zero and
    // returns to out-a with every settlement successful.
    let mut engine = queued_cross_output_bootstrap();
    let capture = queued_cross_output_lagging_capture(2);
    let queued: Vec<(u64, WorldCommand)> = (0..8)
        .map(|i| {
            let direction = if i % 2 == 0 { "right" } else { "left" };
            (11 + i, queued_cross_output_move_dir("win-a", direction))
        })
        .collect();
    let (outcome, settlements) = engine.handle_report(&capture, &queued);
    let reply = match outcome {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("alternating capture must accept, got {other:?}"),
    };
    assert_eq!(
        settlements.len(),
        8,
        "all eight queued moves settle: {settlements:?}"
    );
    for (i, settlement) in settlements.iter().enumerate() {
        assert_eq!(settlement.seq, 11 + i as u64);
        assert_eq!(
            settlement.result, None,
            "alternating move {} must settle successful, not transfer-in-flight; got {:?}",
            settlement.seq, settlement.result
        );
    }
    assert_queued_cross_output_final_out_a(&reply);
}

#[test]
fn queued_cross_output_alternating_eight_separate_captures_drain_and_consume() {
    // Split-capture arm of the eight alternating R/L regression: commands
    // 11..18 each land in their own lagging capture (request_seq 2..9), the
    // native raw fact stays at out-a, and every command therefore evaluates
    // exactly the latest logical world without a native ack. The batch
    // composes net-zero back to out-a; all seven published predecessors stay
    // correlated until the latest is consumed; a spent replay cannot re-mint.
    use crate::world_report::MemberDisposition;

    let mut engine = queued_cross_output_bootstrap();
    let mut latest: Option<crate::world_report::TransferTargetV4> = None;
    let mut last_reply: Option<crate::world_report::WorldReplyV4> = None;
    for i in 0..8u64 {
        let seq = 11 + i;
        let capture_seq = 2 + i;
        let direction = if i % 2 == 0 { "right" } else { "left" };
        let expected_output = if i % 2 == 0 { "out-b" } else { "out-a" };
        let capture = queued_cross_output_lagging_capture(capture_seq);
        let (outcome, settlements) = engine.handle_report(
            &capture,
            &[(seq, queued_cross_output_move_dir("win-a", direction))],
        );
        let reply = match outcome {
            WorldOutcome::Accepted(reply) => reply,
            other => panic!("separate capture {capture_seq} must accept, got {other:?}"),
        };
        assert_eq!(
            settlements.len(),
            1,
            "separate capture {capture_seq} settles once: {settlements:?}"
        );
        assert_eq!(settlements[0].seq, seq);
        assert_eq!(
            settlements[0].result, None,
            "move {seq} must settle against the latest logical world, not transfer-in-flight; got {:?}",
            settlements[0].result
        );
        assert_eq!(
            reply.transfers.len(),
            1,
            "separate capture {capture_seq} carries exactly the latest transfer: {:?}",
            reply.transfers
        );
        assert_eq!(reply.transfers[0].operation_id, format!("op-{seq}"));
        assert_eq!(reply.transfers[0].target_output.0, expected_output);
        assert!(!reply.transfers[0].held);
        assert_eq!(window_row(&reply, "win-a").output.0, expected_output);
        latest = Some(reply.transfers[0].clone());
        last_reply = Some(reply);
    }
    let latest = latest.expect("latest transfer");
    assert_queued_cross_output_final_out_a(&last_reply.expect("last reply"));

    // Eight moves end at logical out-a: latest op18 targets A unheld, with
    // all seven earlier published operations retained as predecessors. No
    // cap is invented, so the full published history must be present.
    assert_eq!(latest.operation_id, "op-18");
    assert_eq!(latest.target_output.0, "out-a");
    assert!(!latest.held);
    assert_eq!(
        engine.world.settlement.retained.len(),
        1,
        "one latest retained entry"
    );
    let predecessor_ops: Vec<String> = engine.world.settlement.retained[0]
        .member_predecessors
        .iter()
        .map(|(target, _)| target.operation_id.clone())
        .collect();
    assert_eq!(
        predecessor_ops,
        (11..18).map(|s| format!("op-{s}")).collect::<Vec<_>>(),
        "all seven published earlier moves retained, no cap invented"
    );

    // Commandless capture10: the raw fact at out-a now matches the latest
    // target, but seven outstanding predecessors block a bare fact from
    // retiring the latest. It stays retained unheld.
    let commandless = accepted(&mut engine, &queued_cross_output_lagging_capture(10));
    assert_eq!(commandless.transfers.len(), 1, "{commandless:?}");
    assert_eq!(commandless.transfers[0].operation_id, latest.operation_id);
    assert_eq!(commandless.transfers[0].target_output.0, "out-a");
    assert!(!commandless.transfers[0].held);

    // Exact latest AlreadyAtTarget proof with readable logical-A pre/post
    // consumes op18 and clears both the transfer and its predecessor history.
    let home_a = WorkspaceRef::Logical {
        output: latest.target_output.clone(),
        workspace: WorkspaceId(latest.target_workspace.clone()),
    };
    let mut consumed = queued_cross_output_lagging_capture(11);
    consumed.monotonic_ms = 60;
    consumed.member_attempts = vec![MemberAttempt {
        operation_id: latest.operation_id.clone(),
        window: WindowId("win-a".to_owned()),
        target_output: latest.target_output.clone(),
        target_workspace: latest.target_workspace.clone(),
        setter: MemberDisposition::AlreadyAtTarget,
        pre: Some(home_a.clone()),
        post: Some(home_a),
        attempt_ms: 55,
    }];
    let consumed_reply = accepted(&mut engine, &consumed);
    assert_queued_cross_output_final_out_a(&consumed_reply);
    assert!(consumed_reply.transfers.is_empty(), "{consumed_reply:?}");
    assert!(
        engine.retained_transfers().is_empty(),
        "consumption clears transfer and all predecessor history: {:?}",
        engine.retained_transfers()
    );

    // Fresh replay of the spent evidence cannot re-mint a flight.
    consumed.request_seq = 12;
    consumed.observation_seq = 12;
    consumed.request_id = "req-12".to_owned();
    let replay = accepted(&mut engine, &consumed);
    assert!(
        replay.transfers.is_empty(),
        "spent latest evidence cannot re-mint on a fresh capture: {replay:?}"
    );
}

fn queued_cross_output_published_roundtrip() -> (
    WorldEngine,
    crate::world_report::TransferTargetV4,
    crate::world_report::TransferTargetV4,
) {
    let mut engine = queued_cross_output_bootstrap();
    let (right, first) = engine.handle_report(
        &queued_cross_output_lagging_capture(2),
        &[(11, queued_cross_output_move_dir("win-a", "right"))],
    );
    assert_eq!(first[0].result, None);
    let WorldOutcome::Accepted(right) = right else {
        panic!("right capture");
    };
    let prior = right.transfers[0].clone();
    let (left, second) = engine.handle_report(
        &queued_cross_output_lagging_capture(3),
        &[(12, queued_cross_output_move_dir("win-a", "left"))],
    );
    assert_eq!(second[0].result, None);
    let WorldOutcome::Accepted(left) = left else {
        panic!("left capture");
    };
    assert_queued_cross_output_final_out_a(&left);
    assert_eq!(left.transfers.len(), 1);
    (engine, prior, left.transfers[0].clone())
}

#[test]
fn queued_cross_output_latest_requires_consumption_and_survives_prior_own_arrival() {
    use crate::world_report::MemberDisposition;

    let (mut engine, prior, latest) = queued_cross_output_published_roundtrip();
    let unchanged = accepted(&mut engine, &queued_cross_output_lagging_capture(4));
    assert_eq!(unchanged.transfers.len(), 1);
    assert_eq!(unchanged.transfers[0].operation_id, latest.operation_id);
    let home = WorkspaceRef::Logical {
        output: latest.target_output.clone(),
        workspace: WorkspaceId(latest.target_workspace.clone()),
    };
    let mut older_arrival = queued_cross_output_lagging_capture(5);
    older_arrival.windows = vec![fact("win-a", "out-b", 800)];
    older_arrival.monotonic_ms = 50;
    older_arrival.member_attempts = vec![MemberAttempt {
        operation_id: prior.operation_id,
        window: WindowId("win-a".to_owned()),
        target_output: prior.target_output,
        target_workspace: prior.target_workspace,
        setter: MemberDisposition::Accepted,
        pre: Some(home.clone()),
        post: Some(home.clone()),
        attempt_ms: 40,
    }];
    let delayed = accepted(&mut engine, &older_arrival);
    assert_queued_cross_output_final_out_a(&delayed);
    assert_eq!(delayed.transfers.len(), 1);
    assert_eq!(delayed.transfers[0].operation_id, latest.operation_id);
    assert!(!delayed.transfers[0].held);

    let mut consumed = queued_cross_output_lagging_capture(6);
    consumed.monotonic_ms = 60;
    consumed.member_attempts = vec![MemberAttempt {
        operation_id: latest.operation_id,
        window: WindowId("win-a".to_owned()),
        target_output: latest.target_output,
        target_workspace: latest.target_workspace,
        setter: MemberDisposition::AlreadyAtTarget,
        pre: Some(home.clone()),
        post: Some(home),
        attempt_ms: 55,
    }];
    let final_reply = accepted(&mut engine, &consumed);
    assert_queued_cross_output_final_out_a(&final_reply);
    assert!(final_reply.transfers.is_empty());
    consumed.request_seq = 7;
    consumed.observation_seq = 7;
    consumed.request_id = "req-7".to_owned();
    assert!(accepted(&mut engine, &consumed).transfers.is_empty());
}

#[test]
fn queued_cross_output_latest_call_proof_survives_lagging_prior_arrival() {
    use crate::world_report::MemberDisposition;

    let mut control = queued_cross_output_bootstrap();
    let source = control
        .tx(|tx| tx.staged_homing(&WindowId("win-a".to_owned())))
        .unwrap();
    let (outcome, _) = control.handle_report(
        &queued_cross_output_lagging_capture(2),
        &[(11, queued_cross_output_move_dir("win-a", "right"))],
    );
    let WorldOutcome::Accepted(right) = outcome else {
        panic!("control right capture must accept");
    };
    assert_eq!(right.transfers.len(), 1, "{:?}", right.transfers);
    let transfer = right.transfers[0].clone();
    let mut capture3 = queued_cross_output_lagging_capture(3);
    capture3.monotonic_ms = 40;
    capture3.member_attempts = vec![MemberAttempt {
        operation_id: transfer.operation_id.clone(),
        window: WindowId("win-a".to_owned()),
        target_output: transfer.target_output.clone(),
        target_workspace: transfer.target_workspace.clone(),
        setter: MemberDisposition::Accepted,
        pre: Some(WorkspaceRef::Logical {
            output: source.output.clone(),
            workspace: source.workspace.clone(),
        }),
        post: Some(WorkspaceRef::Logical {
            output: transfer.target_output.clone(),
            workspace: WorkspaceId(transfer.target_workspace.clone()),
        }),
        attempt_ms: 30,
    }];
    let settled = accepted(&mut control, &capture3);
    assert!(settled.transfers.is_empty(), "{:?}", settled.transfers);
    assert_eq!(
        window_row(&settled, "win-a").output.0,
        "out-a",
        "attempt proof settles the transfer without rewriting the captured source: {settled:?}"
    );

    let (mut engine, prior, latest) = queued_cross_output_published_roundtrip();
    let home_a = WorkspaceRef::Logical {
        output: latest.target_output.clone(),
        workspace: WorkspaceId(latest.target_workspace.clone()),
    };
    let home_b = WorkspaceRef::Logical {
        output: prior.target_output.clone(),
        workspace: WorkspaceId(prior.target_workspace.clone()),
    };
    let mut capture4 = queued_cross_output_lagging_capture(4);
    capture4.windows = vec![fact("win-a", "out-b", 800)];
    capture4.monotonic_ms = 50;
    capture4.member_attempts = vec![
        MemberAttempt {
            operation_id: prior.operation_id.clone(),
            window: WindowId("win-a".to_owned()),
            target_output: prior.target_output.clone(),
            target_workspace: prior.target_workspace.clone(),
            setter: MemberDisposition::Accepted,
            pre: Some(home_a.clone()),
            post: Some(home_a.clone()),
            attempt_ms: 30,
        },
        MemberAttempt {
            operation_id: latest.operation_id.clone(),
            window: WindowId("win-a".to_owned()),
            target_output: latest.target_output.clone(),
            target_workspace: latest.target_workspace.clone(),
            setter: MemberDisposition::Accepted,
            pre: Some(home_b),
            post: Some(home_a),
            attempt_ms: 40,
        },
    ];
    let reply = accepted(&mut engine, &capture4);
    assert_eq!(
        window_row(&reply, "win-a").output.0,
        "out-b",
        "attempt proof settles without rewriting the captured source"
    );
    assert!(reply.transfers.is_empty(), "{:?}", reply.transfers);

    capture4.request_seq = 5;
    capture4.observation_seq = 5;
    capture4.request_id = "req-5".to_owned();
    capture4.windows = vec![fact("win-a", "out-a", 0)];
    let observed = accepted(&mut engine, &capture4);
    assert_queued_cross_output_final_out_a(&observed);
    assert!(observed.transfers.is_empty());
    capture4.request_seq = 6;
    capture4.observation_seq = 6;
    capture4.request_id = "req-6".to_owned();
    let replay = accepted(&mut engine, &capture4);
    assert!(replay.transfers.is_empty(), "{:?}", replay.transfers);
}

#[test]
fn queued_cross_output_no_call_needs_exact_identity_and_matching_readbacks() {
    use crate::world_report::MemberDisposition;

    for invalid in [
        "operation",
        "window",
        "target",
        "pre",
        "post",
        "different-pre",
    ] {
        let (mut engine, _, latest) = queued_cross_output_published_roundtrip();
        let home = WorkspaceRef::Logical {
            output: latest.target_output.clone(),
            workspace: WorkspaceId(latest.target_workspace.clone()),
        };
        let mut row = MemberAttempt {
            operation_id: latest.operation_id.clone(),
            window: WindowId("win-a".to_owned()),
            target_output: latest.target_output.clone(),
            target_workspace: latest.target_workspace.clone(),
            setter: MemberDisposition::AlreadyAtTarget,
            pre: Some(home.clone()),
            post: Some(home),
            attempt_ms: 30,
        };
        match invalid {
            "operation" => row.operation_id = "unrelated-op".to_owned(),
            "window" => row.window = WindowId("other-lifetime".to_owned()),
            "target" => row.target_workspace = "other-target".to_owned(),
            "pre" => row.pre = None,
            "post" => row.post = None,
            "different-pre" => {
                row.pre = Some(WorkspaceRef::Logical {
                    output: OutputId("out-b".to_owned()),
                    workspace: WorkspaceId(latest.target_workspace.clone()),
                });
            }
            _ => unreachable!(),
        }
        let mut capture = queued_cross_output_lagging_capture(4);
        capture.monotonic_ms = 40;
        capture.member_attempts = vec![row];
        let reply = accepted(&mut engine, &capture);
        assert_eq!(reply.transfers.len(), 1, "{invalid}");
        assert_eq!(
            reply.transfers[0].operation_id, latest.operation_id,
            "{invalid}"
        );
    }
}

#[test]
fn queued_cross_output_spent_latest_evidence_latches_held() {
    use crate::world_report::MemberDisposition;

    // Public report loop Right11 then Left12, native lagging at out-a: the
    // latest transfer op12 targets the logical home at out-a (latestA) while
    // its retained source is also read from that lagging out-a capture.
    let (mut engine, prior, latest) = queued_cross_output_published_roundtrip();
    let home_a = WorkspaceRef::Logical {
        output: latest.target_output.clone(),
        workspace: WorkspaceId(latest.target_workspace.clone()),
    };
    let home_b = WorkspaceRef::Logical {
        output: prior.target_output.clone(),
        workspace: WorkspaceId(prior.target_workspace.clone()),
    };
    // Next raw fact arrives at out-b (the predecessor's target): the staged
    // homing now matches op11's own-write target, not op12's lagging source.
    let mut arrival = queued_cross_output_lagging_capture(4);
    arrival.windows = vec![fact("win-a", "out-b", 800)];
    arrival.monotonic_ms = 50;
    arrival.member_attempts = vec![
        // Exact prior op11 accepted evidence: the earlier own write to out-b.
        MemberAttempt {
            operation_id: prior.operation_id.clone(),
            window: WindowId("win-a".to_owned()),
            target_output: prior.target_output.clone(),
            target_workspace: prior.target_workspace.clone(),
            setter: MemberDisposition::Accepted,
            pre: Some(home_a.clone()),
            post: Some(home_b),
            attempt_ms: 40,
        },
        // Exact latest op12 AlreadyAtTarget, but pre=None makes this no-call
        // consumption invalid; it cannot reauthorize actuation.
        MemberAttempt {
            operation_id: latest.operation_id.clone(),
            window: WindowId("win-a".to_owned()),
            target_output: latest.target_output.clone(),
            target_workspace: latest.target_workspace.clone(),
            setter: MemberDisposition::AlreadyAtTarget,
            pre: None,
            post: Some(home_a),
            attempt_ms: 45,
        },
    ];
    let reply = accepted(&mut engine, &arrival);
    // The latest original transfer and its logical out-a target survive, but
    // the spent/ambiguous latest raw evidence cannot reauthorize actuation:
    // the flight must latch held, not be retained unheld.
    assert_eq!(reply.transfers.len(), 1, "{reply:?}");
    assert_eq!(reply.transfers[0].operation_id, latest.operation_id);
    assert_eq!(reply.transfers[0].target_output.0, "out-a");
    assert_eq!(reply.transfers[0].target_workspace, latest.target_workspace);
    assert!(
        reply.transfers[0].held,
        "spent/ambiguous latest raw evidence must latch held, not retain unheld: {reply:?}"
    );
    assert_queued_cross_output_final_out_a(&reply);

    // Replay with a fresh sequence and the same spent evidence must not renew
    // actuation permission: the flight stays held.
    arrival.request_seq = 5;
    arrival.observation_seq = 5;
    arrival.request_id = "req-5".to_owned();
    let replay = accepted(&mut engine, &arrival);
    assert_eq!(replay.transfers.len(), 1, "{replay:?}");
    assert_eq!(replay.transfers[0].operation_id, latest.operation_id);
    assert!(
        replay.transfers[0].held,
        "replay must not renew actuation permission: {replay:?}"
    );
}

#[test]
fn queued_cross_output_latest_threw_without_source_keeps_own_arrival_held() {
    use crate::world_report::MemberDisposition;

    // Public Right11/Left12 loop: prior op11 -> out-b (B), latest op12 -> out-a
    // (targetA), with the lagging source also read from out-a. A later raw fact
    // at B carries exact prior op11 Accepted preA/postB plus latest op12 Threw
    // without a readback; only the latest pre differs between the two arms.
    for readable_source in [true, false] {
        let (mut engine, prior, latest) = queued_cross_output_published_roundtrip();
        // Preconditions: the helper is the exact Right11/Left12 loop.
        assert_eq!(prior.operation_id, "op-11");
        assert_eq!(latest.operation_id, "op-12");
        assert_eq!(prior.target_output.0, "out-b");
        assert_eq!(latest.target_output.0, "out-a");
        let home_a = WorkspaceRef::Logical {
            output: latest.target_output.clone(),
            workspace: WorkspaceId(latest.target_workspace.clone()),
        };
        let home_b = WorkspaceRef::Logical {
            output: prior.target_output.clone(),
            workspace: WorkspaceId(prior.target_workspace.clone()),
        };
        let mut arrival = queued_cross_output_lagging_capture(4);
        arrival.windows = vec![fact("win-a", "out-b", 800)];
        arrival.monotonic_ms = 50;
        arrival.member_attempts = vec![
            // Exact prior op11 accepted evidence: the earlier own write to B.
            MemberAttempt {
                operation_id: prior.operation_id.clone(),
                window: WindowId("win-a".to_owned()),
                target_output: prior.target_output.clone(),
                target_workspace: prior.target_workspace.clone(),
                setter: MemberDisposition::Accepted,
                pre: Some(home_a.clone()),
                post: Some(home_b),
                attempt_ms: 40,
            },
            // Latest op12 Threw, post=None: possibly-executed with no proof.
            MemberAttempt {
                operation_id: latest.operation_id.clone(),
                window: WindowId("win-a".to_owned()),
                target_output: latest.target_output.clone(),
                target_workspace: latest.target_workspace.clone(),
                setter: MemberDisposition::Threw,
                pre: if readable_source {
                    Some(home_a.clone())
                } else {
                    None
                },
                post: None,
                attempt_ms: 45,
            },
        ];
        let reply = accepted(&mut engine, &arrival);
        if readable_source {
            // Control: a readable current sourceA is legitimate clearance to
            // erase correlation, so B is outside source/target and the
            // existing external adoption/drop path is unchanged.
            assert!(
                reply.transfers.is_empty(),
                "readable latest pre keeps external adoption: {reply:?}"
            );
            assert_eq!(window_row(&reply, "win-a").output.0, "out-b");
        } else {
            // Unreadable source and no target proof cannot erase own-write
            // correlation: op12/targetA survives held.
            assert_eq!(reply.transfers.len(), 1, "{reply:?}");
            assert_eq!(reply.transfers[0].operation_id, latest.operation_id);
            assert_eq!(reply.transfers[0].target_output.0, "out-a");
            assert_eq!(reply.transfers[0].target_workspace, latest.target_workspace);
            assert!(
                reply.transfers[0].held,
                "possibly-executed latest without source must latch held: {reply:?}"
            );
            assert_queued_cross_output_final_out_a(&reply);

            // Fresh replay with the same unreadable evidence must not renew
            // actuation permission or mint a new transfer id.
            arrival.request_seq = 5;
            arrival.observation_seq = 5;
            arrival.request_id = "req-5".to_owned();
            let replay = accepted(&mut engine, &arrival);
            assert_eq!(replay.transfers.len(), 1, "{replay:?}");
            assert_eq!(replay.transfers[0].operation_id, latest.operation_id);
            assert!(
                replay.transfers[0].held,
                "replay must not renew actuation permission: {replay:?}"
            );
        }
    }
}

#[test]
fn queued_cross_output_unreadable_refused_predecessor_preserves_latest() {
    use crate::world_report::MemberDisposition;

    // Public report loop Right11 then Left12, native lagging at out-a:
    // latest op12 targets the logical home at out-a (targetA), while the
    // predecessor op11 targeted out-b (B). A later raw fact arrives at out-b,
    // paired with exact prior op11 Refused evidence holding preA. In the
    // unreadable arm post=None, so the refusal cannot be proven unchanged:
    // the prior call may have written, making out-b our own possible arrival
    // rather than proven external supersession. The original latest
    // op12/targetA/unheld transfer must survive. The proven-refusal control
    // (postA == preA) confirms the latest still drops and adopts B through
    // the existing external path, so this is not blanket protection.
    for unreadable in [true, false] {
        let (mut engine, prior, latest) = queued_cross_output_published_roundtrip();
        let home_a = WorkspaceRef::Logical {
            output: latest.target_output.clone(),
            workspace: WorkspaceId(latest.target_workspace.clone()),
        };
        let mut arrival = queued_cross_output_lagging_capture(4);
        arrival.windows = vec![fact("win-a", "out-b", 800)];
        arrival.monotonic_ms = 50;
        // Exact prior op11 Refused evidence only, with preA; no latest op12
        // attempt. post=None is unreadable, postA is a proven unchanged
        // refusal.
        arrival.member_attempts = vec![MemberAttempt {
            operation_id: prior.operation_id.clone(),
            window: WindowId("win-a".to_owned()),
            target_output: prior.target_output.clone(),
            target_workspace: prior.target_workspace.clone(),
            setter: MemberDisposition::Refused,
            pre: Some(home_a.clone()),
            post: if unreadable { None } else { Some(home_a) },
            attempt_ms: 40,
        }];
        let reply = accepted(&mut engine, &arrival);
        if unreadable {
            assert_eq!(reply.transfers.len(), 1, "unreadable: {reply:?}");
            assert_eq!(reply.transfers[0].operation_id, latest.operation_id);
            assert_eq!(reply.transfers[0].target_output.0, "out-a");
            assert_eq!(reply.transfers[0].target_workspace, latest.target_workspace);
            assert!(
                !reply.transfers[0].held,
                "unreadable prior refusal cannot prove an unchanged refusal: latest preserved unheld: {reply:?}"
            );
        } else {
            assert!(
                reply.transfers.is_empty(),
                "proven unchanged refusal: latest drops and external fact adopts B: {reply:?}"
            );
            assert_eq!(window_row(&reply, "win-a").output.0, "out-b");
        }
    }
}

#[test]
fn queued_cross_output_nonimmediate_predecessor_needs_raw_own_evidence() {
    use crate::world_report::MemberDisposition;

    for with_proof in [false, true] {
        let screens = vec![
            screen("out-a", 0),
            screen("out-b", 800),
            screen("out-c", 1600),
        ];
        let capture = |seq| {
            let mut r = queued_cross_output_lagging_capture(seq);
            r.screens = screens.clone();
            r
        };
        let mut engine =
            WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
        accepted(&mut engine, &capture(1));
        let mut prior: Option<crate::world_report::TransferTargetV4> = None;
        let mut latest: Option<crate::world_report::TransferTargetV4> = None;
        for (seq, direction, expected) in [
            (11, "right", "out-b"),
            (12, "right", "out-c"),
            (13, "left", "out-b"),
            (14, "left", "out-a"),
        ] {
            let (outcome, settlements) = engine.handle_report(
                &capture(seq - 9),
                &[(seq, queued_cross_output_move_dir("win-a", direction))],
            );
            assert_eq!(settlements[0].result, None);
            let reply = match outcome {
                WorldOutcome::Accepted(reply) => reply,
                other => panic!("three-output capture must accept, got {other:?}"),
            };
            assert_eq!(window_row(&reply, "win-a").output.0, expected);
            assert_eq!(reply.transfers.len(), 1);
            assert_eq!(reply.transfers[0].target_output.0, expected);
            if seq == 12 {
                prior = Some(reply.transfers[0].clone());
            }
            latest = Some(reply.transfers[0].clone());
        }
        let prior = prior.expect("prior transfer");
        let latest = latest.expect("latest transfer");
        let mut arrival = capture(6);
        arrival.windows = vec![fact("win-a", "out-c", 1600)];
        arrival.monotonic_ms = 60;
        if with_proof {
            arrival.member_attempts = vec![MemberAttempt {
                operation_id: prior.operation_id.clone(),
                window: WindowId("win-a".to_owned()),
                target_output: prior.target_output.clone(),
                target_workspace: prior.target_workspace.clone(),
                setter: MemberDisposition::Accepted,
                pre: Some(WorkspaceRef::Logical {
                    output: latest.target_output.clone(),
                    workspace: WorkspaceId(latest.target_workspace.clone()),
                }),
                post: Some(WorkspaceRef::Logical {
                    output: latest.target_output.clone(),
                    workspace: WorkspaceId(latest.target_workspace.clone()),
                }),
                attempt_ms: 40,
            }];
        }
        let reply = accepted(&mut engine, &arrival);
        if with_proof {
            let retained: Vec<_> = reply
                .transfers
                .iter()
                .filter(|t| t.operation_id == latest.operation_id)
                .collect();
            assert_eq!(
                retained.len(),
                1,
                "latest retained once: {:?}",
                reply.transfers
            );
            assert!(!retained[0].held);
            assert_eq!(window_row(&reply, "win-a").output.0, "out-a");
        } else {
            assert!(
                reply.transfers.is_empty(),
                "raw fact settles: {:?}",
                reply.transfers
            );
            assert_eq!(window_row(&reply, "win-a").output.0, "out-c");
        }
    }
}

#[test]
fn directional_focus_crosses_outputs_with_explicit_effect() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
    accepted(
        &mut engine,
        &report(
            1,
            1,
            two_screens(),
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-b", 800)],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            false,
        ),
    );
    // Left/right cross-output focus runs the strict pair; the explicit
    // effect names the resolved target window. The target needs a
    // remembered focus leaf, so win-b focuses first. Single-window
    // domains keep the cross leg free of local competitors.
    accepted(
        &mut engine,
        &report(
            2,
            2,
            two_screens(),
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-b", 800)],
            vec![],
            vec![],
            "win-b",
            None,
            None,
            true,
            false,
        ),
    );
    let reply = accepted(
        &mut engine,
        &report(
            3,
            3,
            two_screens(),
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-b", 800)],
            vec![],
            vec![],
            "win-a",
            Some(11),
            Some(WorldCommand::FocusDirectional {
                window: crate::world_report::FocusDirectionalSubject::Window(WindowId(
                    "win-a".to_owned(),
                )),
                direction: crate::directional::Direction::Right,
            }),
            true,
            false,
        ),
    );
    let request = reply
        .focus_request
        .as_ref()
        .expect("explicit focus request");
    assert_eq!(request.target.0, "win-b");
    assert_eq!(request.id, format!("focus-{}", reply.state_revision));
    assert_eq!(request.issued_revision, reply.state_revision);
    assert_eq!(
        request.cause,
        crate::world_report::FocusRequestCause::ExplicitCommand
    );
    assert_eq!(request.cmd_seq, Some(11));
    assert_eq!(request.after_shown, None);
}

#[test]
fn directional_focus_up_never_crosses_outputs() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
    accepted(
        &mut engine,
        &report(
            1,
            1,
            two_screens(),
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-b", 800),
                fact("win-c", "out-a", 100),
            ],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            false,
        ),
    );
    // Up never crosses: the outcome stays local (an effect on the home
    // output) or refuses as unchanged, never an out-b target.
    match engine
        .handle_report(
            &report(
                2,
                2,
                two_screens(),
                vec![
                    fact("win-a", "out-a", 0),
                    fact("win-b", "out-b", 800),
                    fact("win-c", "out-a", 100),
                ],
                vec![],
                vec![],
                "win-a",
                Some(11),
                Some(WorldCommand::FocusDirectional {
                    window: crate::world_report::FocusDirectionalSubject::Window(WindowId(
                        "win-a".to_owned(),
                    )),
                    direction: crate::directional::Direction::Up,
                }),
                true,
                false,
            ),
            &[],
        )
        .0
    {
        WorldOutcome::Accepted(reply) | WorldOutcome::CommandConsumed(reply) => {
            assert!(reply.focus_request.is_none());
        }
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "unchanged"),
        other => panic!("unexpected outcome: {other:?}"),
    }
}

#[test]
fn selection_commands_resolve_over_logical_rings() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            two_screens(),
            vec![minimized_fact("win-a", "out-a")],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            false,
        ),
    );
    let view_a = output_view(&first, "out-a").expect("view a");
    let ring_a = {
        let mut ring: Vec<String> = first
            .workspaces
            .iter()
            .filter(|row| row.output.0 == "out-a")
            .map(|row| row.id.clone())
            .collect();
        ring.sort_by_key(|id| {
            first
                .workspaces
                .iter()
                .find(|row| row.id == *id)
                .map(|row| row.order)
        });
        ring
    };
    assert_eq!(ring_a.len(), 2);
    // Numbered selection moves the view with history.
    let second = accepted(
        &mut engine,
        &report(
            2,
            2,
            two_screens(),
            vec![minimized_fact("win-a", "out-a")],
            vec![],
            vec![],
            "",
            Some(11),
            Some(WorldCommand::SelectNumbered {
                output: Some(OutputId("out-a".to_owned())),
                index: 2,
            }),
            true,
            false,
        ),
    );
    assert_eq!(
        output_view(&second, "out-a").as_deref(),
        Some(ring_a[1].as_str())
    );
    // An empty numbered select completes at issuing: the terminal carries
    // the switch outcome and the view parent retires with it.
    assert_eq!(second.switch_settled.len(), 1);
    assert!(second.transfers.is_empty());
    // Relative selection wraps around the ring.
    let third = accepted(
        &mut engine,
        &report(
            3,
            3,
            two_screens(),
            vec![minimized_fact("win-a", "out-a")],
            vec![],
            vec![],
            "",
            Some(12),
            Some(WorldCommand::SelectRelative {
                output: Some(OutputId("out-a".to_owned())),
                delta: 1,
            }),
            true,
            false,
        ),
    );
    assert_eq!(
        output_view(&third, "out-a").as_deref(),
        Some(view_a.as_str())
    );
    // Unknown targets refuse without mutation.
    let seq = 4;
    match engine.handle_report(
        &report(
            4,
            4,
            two_screens(),
            vec![minimized_fact("win-a", "out-a")],
            vec![],
            vec![],
            "",
            Some(13),
            Some(WorldCommand::SelectWorkspace {
                output: OutputId("out-a".to_owned()),
                workspace: crate::directional::WorkspaceId("ws-999".to_owned()),
            }),
            true,
            false,
        ),
        &[],
    ) {
        (WorldOutcome::Accepted(_), settlements) => {
            assert_eq!(settlements.len(), 1);
            assert_eq!(
                settlements[0].result,
                Some(CommandResult::Rejected {
                    kind: CommandRejectKind::ViewTargetUnknown
                }),
                "expected view-target-unknown"
            );
        }
        (other, _) => panic!("expected view-target-unknown, got {other:?}"),
    }
    let _ = seq;
    // Previous toggles back through recorded history.
    let fourth = accepted(
        &mut engine,
        &report(
            5,
            5,
            two_screens(),
            vec![minimized_fact("win-a", "out-a")],
            vec![],
            vec![],
            "",
            Some(14),
            Some(WorldCommand::SelectPrevious {
                output: Some(OutputId("out-a".to_owned())),
            }),
            true,
            false,
        ),
    );
    assert_eq!(
        output_view(&fourth, "out-a").as_deref(),
        Some(ring_a[1].as_str())
    );
}

#[test]
fn numbered_send_trailing_path_creates_and_commits_immediately() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
    accepted(
        &mut engine,
        &report(
            1,
            1,
            two_screens(),
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            false,
        ),
    );
    // With another source member, index 0 sends into the live trailing empty.
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            two_screens(),
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "win-a",
            Some(11),
            Some(WorldCommand::SendNumbered {
                window: crate::directional::WindowSubject::Window(WindowId("win-a".to_owned())),
                source: SendSource::Explicit(OutputId("out-a".to_owned())),
                index: 0,
                follow: true,
            }),
            true,
            false,
        ),
    );
    let request = reply
        .focus_request
        .as_ref()
        .expect("explicit focus request");
    assert_eq!(request.target.0, "win-a");
    assert_eq!(request.id, format!("focus-{}", reply.state_revision));
    assert_eq!(request.issued_revision, reply.state_revision);
    assert_eq!(
        request.cause,
        crate::world_report::FocusRequestCause::ExplicitCommand
    );
    assert_eq!(request.cmd_seq, Some(11));
    assert_eq!(request.after_shown, None);
    // Ineligible follow (no suppression support in these facts): the
    // window flight under the stable command identity with no view
    // flight and no frozen binding, legacy direct focus only. The
    // logical view still moves to the reused trailing empty.
    let window = reply
        .transfers
        .iter()
        .find(|t| matches!(&t.kind, TransferKind::Window(w) if w.0 == "win-a"))
        .expect("window flight");
    assert_eq!(reply.transfers.len(), 1, "window flight only");
    assert_eq!(window.operation_id, "op-11");
    assert_eq!(
        reply
            .focus_request
            .as_ref()
            .expect("explicit focus request")
            .after_shown,
        None,
        "ineligible follow never awaits"
    );
    let trailing = window.target_workspace.clone();
    assert_eq!(
        output_view(&reply, "out-a").as_deref(),
        Some(trailing.as_str()),
        "ineligible follow still moves the logical view"
    );
    // Filling the trailing empty grows the ring by lifecycle, never pends.
    let ringed = accepted(
        &mut engine,
        &report(
            3,
            3,
            two_screens(),
            vec![fact("win-a", "out-a", 0)],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            false,
        ),
    );
    assert!(workspace_ids(&ringed, "out-a").len() >= 2);
}

#[test]
fn numbered_send_mismatched_output_refuses_without_mutation() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
    // Establish a complete usable world before exercising foreign-output
    // refusal. Subsequent incomplete reports freeze trailing lifecycle.
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            two_screens(),
            vec![],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            false,
        ),
    );
    assert!(!workspace_ids(&first, "out-a").is_empty());
    assert!(!workspace_ids(&first, "out-b").is_empty());
    let ring_a = workspace_ids(&first, "out-a");
    let ring_b = workspace_ids(&first, "out-b");
    let view_a = output_view(&first, "out-a").expect("view a");
    let view_b = output_view(&first, "out-b").expect("view b");
    // Numbered sends are same-output workspace routing: addressing the
    // mover through a foreign output refuses exactly as the Engine
    // refuses cross-output workspace sends, before any ring resolution,
    // mint, staging, or view move. Both the trailing path (index 0, which
    // would mint into the foreign mapping) and the positional path refuse
    // alike.
    for (i, (seq, index)) in [(2u64, 0u8), (3u64, 1u8)].iter().enumerate() {
        let command_seq = 11 + i as u64;
        match engine.handle_report(
            &report(
                *seq,
                *seq,
                two_screens(),
                vec![fact("win-a", "out-a", 0), fact("win-b", "out-b", 800)],
                vec![],
                vec![],
                "win-a",
                Some(command_seq),
                Some(WorldCommand::SendNumbered {
                    window: crate::directional::WindowSubject::Window(WindowId("win-a".to_owned())),
                    source: SendSource::Explicit(OutputId("out-b".to_owned())),
                    index: *index,
                    follow: true,
                }),
                false,
                false,
            ),
            &[],
        ) {
            (WorldOutcome::Accepted(_), settlements) => {
                assert_eq!(settlements.len(), 1);
                assert_eq!(
                    settlements[0].result,
                    Some(CommandResult::Rejected {
                        kind: CommandRejectKind::OperationRefused
                    }),
                    "expected cross-domain-mismatch"
                );
            }
            (other, _) => panic!("expected cross-domain-mismatch, got {other:?}"),
        }
    }
    // The refused commands minted nothing: rings, views, and creation
    // state read exactly as at bootstrap plus the ordinary admissions.
    let calm = accepted(
        &mut engine,
        &report(
            4,
            4,
            two_screens(),
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-b", 800)],
            vec![],
            vec![],
            "",
            None,
            None,
            false,
            false,
        ),
    );
    assert_eq!(workspace_ids(&calm, "out-a"), ring_a);
    assert_eq!(workspace_ids(&calm, "out-b"), ring_b);
    assert_eq!(
        output_view(&calm, "out-a").as_deref(),
        Some(view_a.as_str())
    );
    assert_eq!(
        output_view(&calm, "out-b").as_deref(),
        Some(view_b.as_str())
    );
}

#[test]
fn implicit_send_source_freezes_pre_observation_home() {
    use crate::world_report::SendSource;
    // Implicit send sources resolve the mover's retained home frozen
    // before staging, then run the staged equality check exactly like the
    // retired adapter contract (homed mover output, else the per-route
    // fallback). Direct core tests below construct the implicit variants
    // and drive full handle_report ticks: homed wins over a disagreeing
    // pointer, rehomed movers and unhomed newcomers keep the old
    // mismatch refusal, and the keyboard (cached-live) and CLI (action
    // context) fallbacks disagree exactly where the old routes did.
    fn staged(
        seq: u64,
        windows: Vec<WindowFact>,
        focused: &str,
        cursor: Option<(i32, i32)>,
        cached: Option<&str>,
        registry: Vec<&str>,
        command: WorldCommand,
    ) -> WorldReportV4 {
        WorldReportV4 {
            cursor,
            action_foreground: None,
            action_cached: cached.map(|out| OutputId(out.to_owned())),
            action_outputs: registry
                .into_iter()
                .map(|out| OutputId(out.to_owned()))
                .collect(),
            ..report(
                seq,
                seq,
                two_screens(),
                windows,
                vec![],
                vec![],
                focused,
                Some(10 + seq),
                Some(command),
                true,
                false,
            )
        }
    }
    fn sent_to(reply: &crate::world_report::WorldReplyV4, window: &str) -> Option<String> {
        reply.transfers.iter().find_map(|t| match &t.kind {
            TransferKind::Window(w) if w.0 == window => Some(t.target_output.0.clone()),
            _ => None,
        })
    }
    // Homed mover beats a disagreeing pointer: the frozen out-a home
    // matches the staged source and the send commits there.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
    accepted(
        &mut engine,
        &report(
            1,
            1,
            two_screens(),
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            false,
        ),
    );
    let reply = accepted(
        &mut engine,
        &staged(
            2,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            "win-a",
            Some((810, 10)),
            Some("out-b"),
            vec!["out-a", "out-b"],
            WorldCommand::SendNumbered {
                window: crate::directional::WindowSubject::Window(WindowId("win-a".to_owned())),
                source: SendSource::HomeThenAction,
                index: 0,
                follow: false,
            },
        ),
    );
    assert_eq!(sent_to(&reply, "win-a").as_deref(), Some("out-a"));
    // Rehomed mover keeps the old mismatch refusal: frozen out-a against
    // the staged out-b source refuses instead of following the move.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
    accepted(
        &mut engine,
        &report(
            1,
            1,
            two_screens(),
            vec![fact("win-a", "out-a", 0)],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            false,
        ),
    );
    match engine.handle_report(
        &staged(
            2,
            vec![fact("win-a", "out-b", 800)],
            "win-a",
            None,
            None,
            vec!["out-a", "out-b"],
            WorldCommand::SendNumbered {
                window: crate::directional::WindowSubject::Window(WindowId("win-a".to_owned())),
                source: SendSource::HomeThenAction,
                index: 1,
                follow: false,
            },
        ),
        &[],
    ) {
        (WorldOutcome::Accepted(_), settlements) => {
            assert_eq!(settlements.len(), 1);
            assert_eq!(
                settlements[0].result,
                Some(CommandResult::Rejected {
                    kind: CommandRejectKind::OperationRefused
                }),
                "rehomed mover must refuse mismatch"
            );
        }
        (other, _) => panic!("rehomed mover must refuse mismatch, got {other:?}"),
    }
    // Unhomed newcomer with a live cached output the staged affinity
    // disagrees with: the cached leg still refuses, never retargets.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
    accepted(
        &mut engine,
        &report(
            1,
            1,
            two_screens(),
            vec![],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            false,
        ),
    );
    match engine.handle_report(
        &staged(
            2,
            vec![fact("win-n", "out-a", 10)],
            "win-n",
            None,
            Some("out-b"),
            vec!["out-a", "out-b"],
            WorldCommand::SendNumbered {
                window: crate::directional::WindowSubject::Window(WindowId("win-n".to_owned())),
                source: SendSource::HomeThenAction,
                index: 0,
                follow: false,
            },
        ),
        &[],
    ) {
        (WorldOutcome::Accepted(_), settlements) => {
            assert_eq!(settlements.len(), 1);
            assert_eq!(
                settlements[0].result,
                Some(CommandResult::Rejected {
                    kind: CommandRejectKind::OperationRefused
                }),
                "newcomer cache disagreement must refuse"
            );
        }
        (other, _) => panic!("newcomer cache disagreement must refuse, got {other:?}"),
    }
    // Live cached output resolves for the keyboard variant (and the
    // same-setup action variant would refuse): the cached leg filters to
    // live screens.
    let mut live_cached =
        WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
    accepted(
        &mut live_cached,
        &report(
            1,
            1,
            two_screens(),
            vec![],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            false,
        ),
    );
    match live_cached.handle_report(
        &staged(
            2,
            vec![fact("win-n", "out-a", 10)],
            "win-n",
            None,
            Some("out-b"),
            vec!["out-a", "out-b"],
            WorldCommand::SendNumbered {
                window: crate::directional::WindowSubject::Window(WindowId("win-n".to_owned())),
                source: SendSource::HomeThenAction,
                index: 0,
                follow: false,
            },
        ),
        &[],
    ) {
        (WorldOutcome::Accepted(_), settlements) => {
            assert_eq!(settlements.len(), 1);
            assert_eq!(
                settlements[0].result,
                Some(CommandResult::Rejected {
                    kind: CommandRejectKind::OperationRefused
                }),
                "live cached mismatch must refuse"
            );
        }
        (other, _) => panic!("live cached mismatch must refuse, got {other:?}"),
    }
    // Keyboard versus CLI fallbacks disagree exactly where the old
    // routes did: a stale-but-registered cache pins the CLI variant
    // into a mismatch refusal while the keyboard variant falls back to
    // the first live screen and commits there.
    let mut cli = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
    accepted(
        &mut cli,
        &report(
            1,
            1,
            two_screens(),
            vec![],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            false,
        ),
    );
    match cli.handle_report(
        &staged(
            2,
            vec![fact("win-n", "out-a", 10)],
            "win-n",
            None,
            Some("out-z"),
            vec!["out-a", "out-b", "out-z"],
            WorldCommand::SendNumbered {
                window: crate::directional::WindowSubject::Window(WindowId("win-n".to_owned())),
                source: SendSource::HomeThenAction,
                index: 0,
                follow: false,
            },
        ),
        &[],
    ) {
        (WorldOutcome::Accepted(_), settlements) => {
            assert_eq!(settlements.len(), 1);
            assert_eq!(
                settlements[0].result,
                Some(CommandResult::Rejected {
                    kind: CommandRejectKind::OperationRefused
                }),
                "stale cache must refuse the CLI send"
            );
        }
        (other, _) => panic!("stale cache must refuse the CLI send, got {other:?}"),
    }
    let mut keyboard = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
    accepted(
        &mut keyboard,
        &report(
            1,
            1,
            two_screens(),
            vec![],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            false,
        ),
    );
    match keyboard.handle_report(
        &staged(
            2,
            vec![fact("win-n", "out-a", 10), fact("win-m", "out-a", 100)],
            "win-n",
            None,
            Some("out-z"),
            vec!["out-a", "out-b", "out-z"],
            WorldCommand::SendRelative {
                window: crate::directional::WindowSubject::Window(WindowId("win-n".to_owned())),
                source: SendSource::HomeThenAction,
                delta: 1,
                follow: false,
            },
        ),
        &[],
    ) {
        (WorldOutcome::Accepted(_), settlements) => {
            assert_eq!(settlements.len(), 1);
            assert_eq!(
                settlements[0].result,
                Some(CommandResult::Rejected {
                    kind: CommandRejectKind::SendTargetUnknown
                }),
                "unavailable action-context target must refuse"
            );
        }
        (other, _) => panic!("unavailable action-context target must refuse, got {other:?}"),
    }
}

#[test]
fn numbered_send_trailing_create_stages_fresh_target_and_commits() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
    accepted(
        &mut engine,
        &report(
            1,
            1,
            two_screens(),
            vec![],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            false,
        ),
    );
    let second = accepted(
        &mut engine,
        &report(
            2,
            2,
            two_screens(),
            vec![fact("win-a", "out-a", 0)],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            false,
        ),
    );
    let view_a = output_view(&second, "out-a").expect("view a");
    // Occupy the trailing empty: select it, then place win-b there. The
    // third report moves the view; the fourth homes win-b on it.
    let third = accepted(
        &mut engine,
        &report(
            3,
            3,
            two_screens(),
            vec![fact("win-a", "out-a", 0)],
            vec![],
            vec![],
            "",
            Some(11),
            Some(WorldCommand::SelectTrailing {
                output: Some(OutputId("out-a".to_owned())),
            }),
            true,
            false,
        ),
    );
    let view_b = output_view(&third, "out-a").expect("trailing view");
    assert_ne!(view_a, view_b);
    // Incomplete inventory skips trailing lifecycle, so the ring stays fully
    // occupied and index 0 takes the Create path (mint + stage + commit).
    // Echo the trailing view so the earlier select flight confirms instead
    // of shadowing publication with its stale target.
    accepted(
        &mut engine,
        &report(
            4,
            4,
            two_screens(),
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "",
            None,
            None,
            false,
            false,
        ),
    );
    let reply = accepted(
        &mut engine,
        &report(
            5,
            5,
            two_screens(),
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "win-a",
            Some(12),
            Some(WorldCommand::SendNumbered {
                window: crate::directional::WindowSubject::Window(WindowId("win-a".to_owned())),
                source: SendSource::Explicit(OutputId("out-a".to_owned())),
                index: 0,
                follow: true,
            }),
            false,
            false,
        ),
    );
    let request = reply
        .focus_request
        .as_ref()
        .expect("explicit focus request");
    assert_eq!(request.target.0, "win-a");
    assert_eq!(request.id, format!("focus-{}", reply.state_revision));
    assert_eq!(request.issued_revision, reply.state_revision);
    assert_eq!(
        request.cause,
        crate::world_report::FocusRequestCause::ExplicitCommand
    );
    assert_eq!(request.cmd_seq, Some(12));
    assert_eq!(request.after_shown, None);
    // The Create-path window transfer commits alongside the still-outstanding
    // select view flight: filter to the mover's window transfer.
    let sends: Vec<_> = reply
        .transfers
        .iter()
        .filter(|t| {
            matches!(
                &t.kind,
                crate::world_report::TransferKind::Window(w) if w.0 == "win-a"
            )
        })
        .collect();
    assert_eq!(sends.len(), 1);
    // The fresh target is newly minted: distinct from both live views.
    let target = sends[0].target_workspace.clone();
    assert_ne!(target, view_a);
    assert_ne!(target, view_b);
    // Follow publishes the fresh target view immediately: the logical
    // authority moves with the commit, exactly like selection.
    assert_eq!(
        output_view(&reply, "out-a").as_deref(),
        Some(target.as_str())
    );
}

#[test]
fn numbered_follow_selects_target_view_and_stay_keeps_source() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
    accepted(
        &mut engine,
        &report(
            1,
            1,
            two_screens(),
            vec![],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            false,
        ),
    );
    let second = accepted(
        &mut engine,
        &report(
            2,
            2,
            two_screens(),
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
            true,
            false,
        ),
    );
    let view_a = output_view(&second, "out-a").expect("view a");
    let ring: Vec<String> = second
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    let view_b = ring
        .iter()
        .find(|id| *id != &view_a)
        .expect("trailing target")
        .clone();
    let index = (ring.iter().position(|id| id == &view_b).expect("target") + 1) as u8;
    // Follow with a source sibling: the target view publishes immediately
    // with the commit and the mover focuses there.
    let reply = accepted(
        &mut engine,
        &report(
            3,
            3,
            two_screens(),
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 100),
                fact("win-c", "out-a", 200),
            ],
            vec![],
            vec![],
            "win-a",
            Some(11),
            Some(WorldCommand::SendNumbered {
                window: crate::directional::WindowSubject::Window(WindowId("win-a".to_owned())),
                source: SendSource::Explicit(OutputId("out-a".to_owned())),
                index,
                follow: true,
            }),
            true,
            false,
        ),
    );
    assert_eq!(
        output_view(&reply, "out-a").as_deref(),
        Some(view_b.as_str())
    );
    let request = reply
        .focus_request
        .as_ref()
        .expect("explicit focus request");
    assert_eq!(request.target.0, "win-a");
    assert_eq!(request.id, format!("focus-{}", reply.state_revision));
    assert_eq!(request.issued_revision, reply.state_revision);
    assert_eq!(
        request.cause,
        crate::world_report::FocusRequestCause::ExplicitCommand
    );
    assert_eq!(request.cmd_seq, Some(11));
    assert_eq!(request.after_shown, None);
    let member = window_row(&reply, "win-a");
    assert_eq!(
        (member.output.0.as_str(), member.workspace.as_str()),
        ("out-a", view_b.as_str())
    );
    // Stay never moves any view: the source keeps showing while the mover
    // leaves, and stay focuses a source survivor instead of the mover.
    // (win-c pins the source workspace so lifecycle keeps it ringed.)
    let stayed = accepted(
        &mut engine,
        &report(
            4,
            4,
            two_screens(),
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 100),
                fact("win-c", "out-a", 200),
            ],
            vec![],
            vec![],
            "win-b",
            Some(12),
            Some(WorldCommand::SendNumbered {
                window: crate::directional::WindowSubject::Window(WindowId("win-b".to_owned())),
                source: SendSource::Explicit(OutputId("out-a".to_owned())),
                index,
                follow: false,
            }),
            true,
            false,
        ),
    );
    assert_eq!(
        output_view(&stayed, "out-a").as_deref(),
        Some(view_b.as_str())
    );
    let request = stayed
        .focus_request
        .as_ref()
        .expect("explicit focus request");
    assert_eq!(request.id, format!("focus-{}", stayed.state_revision));
    assert_eq!(request.issued_revision, stayed.state_revision);
    assert_eq!(
        request.cause,
        crate::world_report::FocusRequestCause::ExplicitCommand
    );
    assert_eq!(request.cmd_seq, Some(12));
    assert_eq!(request.after_shown, None);
    assert_ne!(request.target.0, "win-b");
    let member = window_row(&stayed, "win-b");
    assert_eq!(
        (member.output.0.as_str(), member.workspace.as_str()),
        ("out-a", view_b.as_str())
    );
    // The follow recorded history: previous-toggle returns to the source.
    let back = accepted(
        &mut engine,
        &report(
            5,
            5,
            two_screens(),
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 100),
                fact("win-c", "out-a", 200),
            ],
            vec![],
            vec![],
            "",
            Some(13),
            Some(WorldCommand::SelectPrevious {
                output: Some(OutputId("out-a".to_owned())),
            }),
            true,
            false,
        ),
    );
    assert_eq!(
        output_view(&back, "out-a").as_deref(),
        Some(view_a.as_str())
    );
}

#[test]
fn numbered_follow_fans_out_views_in_shared_mode() {
    let mode = crate::workspace_scope::WorkspaceMode::Shared;
    let mut engine = engine_with_mode(mode);
    accepted(
        &mut engine,
        &report_mode(
            1,
            1,
            two_screens(),
            vec![],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            false,
            mode,
        ),
    );
    let second = accepted(
        &mut engine,
        &report_mode(
            2,
            2,
            two_screens(),
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            false,
            mode,
        ),
    );
    let view_a = output_view(&second, "out-a").expect("view a");
    assert_eq!(
        output_view(&second, "out-b").as_deref(),
        Some(view_a.as_str())
    );
    let ring: Vec<String> = second
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    let view_b = ring
        .iter()
        .find(|id| *id != &view_a)
        .expect("trailing target")
        .clone();
    let index = (ring.iter().position(|id| id == &view_b).expect("target") + 1) as u8;
    // Leave a member on the source so the adjacent-spare no-op does not
    // apply. A successful follow moves the shared view on every live output.
    let reply = accepted(
        &mut engine,
        &report_mode(
            3,
            3,
            two_screens(),
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "win-a",
            Some(11),
            Some(WorldCommand::SendNumbered {
                window: crate::directional::WindowSubject::Window(WindowId("win-a".to_owned())),
                source: SendSource::Explicit(OutputId("out-a".to_owned())),
                index,
                follow: true,
            }),
            true,
            false,
            mode,
        ),
    );
    assert_eq!(
        output_view(&reply, "out-a").as_deref(),
        Some(view_b.as_str())
    );
    assert_eq!(
        output_view(&reply, "out-b").as_deref(),
        Some(view_b.as_str())
    );
    let request = reply
        .focus_request
        .as_ref()
        .expect("explicit focus request");
    assert_eq!(request.target.0, "win-a");
    assert_eq!(request.id, format!("focus-{}", reply.state_revision));
    assert_eq!(request.issued_revision, reply.state_revision);
    assert_eq!(
        request.cause,
        crate::world_report::FocusRequestCause::ExplicitCommand
    );
    assert_eq!(request.cmd_seq, Some(11));
    assert_eq!(request.after_shown, None);
}

#[test]
fn numbered_follow_moves_only_its_output_in_global_mode() {
    let mode = crate::workspace_scope::WorkspaceMode::GlobalUnique;
    let mut engine = engine_with_mode(mode);
    accepted(
        &mut engine,
        &report_mode(
            1,
            1,
            two_screens(),
            vec![],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            false,
            mode,
        ),
    );
    let second = accepted(
        &mut engine,
        &report_mode(
            2,
            2,
            two_screens(),
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            false,
            mode,
        ),
    );
    let view_a = output_view(&second, "out-a").expect("view a");
    let view_other = output_view(&second, "out-b").expect("other view");
    let ring: Vec<String> = second
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    let view_b = ring
        .iter()
        .find(|id| *id != &view_a)
        .expect("trailing target")
        .clone();
    assert_ne!(view_b, view_other);
    let index = (ring.iter().position(|id| id == &view_b).expect("target") + 1) as u8;
    // Same primitive as direct view retention: the sending output moves,
    // the other output keeps its view (no select-style swap for sends).
    let reply = accepted(
        &mut engine,
        &report_mode(
            3,
            3,
            two_screens(),
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "win-a",
            Some(11),
            Some(WorldCommand::SendNumbered {
                window: crate::directional::WindowSubject::Window(WindowId("win-a".to_owned())),
                source: SendSource::Explicit(OutputId("out-a".to_owned())),
                index,
                follow: true,
            }),
            true,
            false,
            mode,
        ),
    );
    assert_eq!(
        output_view(&reply, "out-a").as_deref(),
        Some(view_b.as_str())
    );
    assert_eq!(
        output_view(&reply, "out-b").as_deref(),
        Some(view_other.as_str())
    );
}

fn migrated_selection_binding(
    engine: &WorldEngine,
    parent_op: &str,
) -> crate::world_selection::RetainedSelection {
    engine
        .world
        .settlement
        .retained_selection
        .values()
        .find(|binding| binding.parent_op == parent_op)
        .unwrap_or_else(|| panic!("missing selection binding {parent_op}"))
        .clone()
}

#[test]
fn valid_migration_holds_target_without_suppression_then_retires_on_arrival() {
    // Native-free migration without suppression support binds one frozen
    // selection covering both outputs (target plus source refill) with no
    // refill child and no logical echo gates: per-window writes hold
    // without suppression, member arrival retires the Migration parent
    // while the Switch pends on its unprovable visibility obligations.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            two_screens(),
            vec![fact("win-a", "out-a", 0)],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            false,
        ),
    );
    let view_a = output_view(&first, "out-a").expect("view a");
    // Sanity: the member lives on out-a under the source workspace.
    assert_eq!(window_row(&first, "win-a").workspace, view_a);
    // Commit the whole-workspace migration of the source workspace.
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            two_screens(),
            vec![fact("win-a", "out-a", 0)],
            vec![],
            vec![],
            "",
            Some(11),
            Some(WorldCommand::MigrateWorkspace {
                source_output: OutputId("out-a".to_owned()),
                target_output: OutputId("out-b".to_owned()),
                workspace: crate::directional::WorkspaceId(view_a.clone()),
            }),
            true,
            false,
        ),
    );
    // BEFORE any arrival: the committed move publishes the target scope
    // Controlled with Controlled membership and frames, and only the
    // Migration flight is retained: no refill child rides the
    // native-free route (per-window visibility holds without
    // suppression, owned by the frozen binding below instead).
    assert_eq!(
        scope_of(&reply, "out-b", &view_a),
        ScopeAuthority::Controlled
    );
    let member = window_row(&reply, "win-a");
    assert_eq!(
        (member.output.0.as_str(), member.workspace.as_str()),
        ("out-b", view_a.as_str())
    );
    assert_eq!(member.membership_authority, ScopeAuthority::Controlled);
    assert_eq!(member.frame_authority, ScopeAuthority::Controlled);
    assert!(member.effective_frame.is_some(), "{member:?}");
    assert!(
        reply
            .transfers
            .iter()
            .any(|t| matches!(t.kind, TransferKind::Migration) && t.operation_id == "op-11")
    );
    assert!(
        reply
            .transfers
            .iter()
            .all(|t| t.parent.is_none() && !matches!(t.kind, TransferKind::View)),
        "no refill child on the native-free route: {:?}",
        reply.transfers
    );
    // One frozen binding covers both outputs with distinct selected
    // workspaces (migrated target plus source refill), focusless here
    // (no foreground member), granting the arrived member at the target.
    let binding = migrated_selection_binding(&engine, "op-11");
    assert!(binding.effect.is_none(), "no invented focus");
    {
        let mut affected = binding.affected.clone();
        affected.sort();
        assert_eq!(
            affected,
            vec![OutputId("out-a".to_owned()), OutputId("out-b".to_owned())]
        );
    }
    let source_selected = binding
        .selected
        .iter()
        .find(|home| home.output.0 == "out-a")
        .expect("source selected home");
    assert_ne!(source_selected.workspace.0, view_a);
    assert!(
        binding
            .selected
            .iter()
            .any(|home| home.output.0 == "out-b" && home.workspace.0 == view_a)
    );
    assert!(
        binding
            .switch_members
            .iter()
            .any(|member| member.window.0 == "win-a"
                && member.home.output.0 == "out-b"
                && member.home.workspace.0 == view_a
                && member.shown)
    );
    // Mapping authority moved the logical id to the target screen.
    let row = reply
        .workspaces
        .iter()
        .find(|row| row.id == view_a)
        .expect("migrated row");
    assert_eq!(row.output.0, "out-b");
    // The retained target session preserves the relocated tree.
    let target_key = crate::session::DomainKey {
        output: OutputId("out-b".to_owned()),
        workspace: crate::directional::WorkspaceId(view_a.clone()),
    };
    assert!(
        engine
            .world
            .layout()
            .is_tiled_window(&target_key, &WindowId("win-a".to_owned()))
    );
    // An unchanged report does not flap the source: same held scope, same
    // single flight, same mapping, same frozen binding.
    let stuck = accepted(
        &mut engine,
        &report(
            3,
            3,
            two_screens(),
            vec![fact("win-a", "out-a", 0)],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            false,
        ),
    );
    assert_eq!(
        scope_of(&stuck, "out-b", &view_a),
        ScopeAuthority::Controlled
    );
    let member = window_row(&stuck, "win-a");
    assert_eq!(
        (member.output.0.as_str(), member.workspace.as_str()),
        ("out-b", view_a.as_str())
    );
    assert_eq!(member.membership_authority, ScopeAuthority::Controlled);
    assert!(
        stuck
            .transfers
            .iter()
            .any(|t| matches!(t.kind, TransferKind::Migration) && t.operation_id == "op-11")
    );
    assert!(
        stuck
            .transfers
            .iter()
            .all(|t| t.parent.is_none() && !matches!(t.kind, TransferKind::View)),
        "unchanged report mints no refill child: {:?}",
        stuck.transfers
    );
    assert!(
        engine
            .world
            .settlement
            .retained_selection
            .values()
            .any(|binding| binding.parent_op == "op-11"),
        "frozen binding survives the unchanged report"
    );
    // Member arrival retires the Migration parent with no echo proof: the
    // Switch pends on its own (here unprovable) visibility obligations.
    let arrived = accepted(
        &mut engine,
        &report(
            4,
            4,
            two_screens(),
            vec![fact("win-a", "out-b", 800)],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            false,
        ),
    );
    assert!(
        arrived.transfers.iter().all(|t| t.operation_id != "op-11"),
        "arrival retires the parent with no echo proof: {:?}",
        arrived.transfers
    );
    let member = window_row(&arrived, "win-a");
    assert_eq!(
        (member.output.0.as_str(), member.workspace.as_str()),
        ("out-b", view_a.as_str())
    );
    assert_eq!(member.frame_authority, ScopeAuthority::Controlled);
    // Target view proof changes nothing: the parent already retired on
    // arrival, and the Switch still pends its visibility obligations.
    let viewed = accepted(
        &mut engine,
        &report(
            5,
            5,
            two_screens(),
            vec![fact("win-a", "out-b", 800)],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            false,
        ),
    );
    assert!(
        viewed.transfers.is_empty(),
        "retired parent stays retired: {:?}",
        viewed.transfers
    );
    // Source refill proof settles everything: flights drop, refill row
    // gone, views adopted.
    let refill = output_view(&viewed, "out-a").expect("refill view");
    assert_ne!(refill, view_a);
    let settled = accepted(
        &mut engine,
        &report(
            6,
            6,
            two_screens(),
            vec![fact("win-a", "out-b", 800)],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            false,
        ),
    );
    assert!(
        settled.transfers.is_empty(),
        "flights: {:?}",
        settled.transfers
    );
    assert_eq!(
        output_view(&settled, "out-b").as_deref(),
        Some(view_a.as_str())
    );
    assert_eq!(
        output_view(&settled, "out-a").as_deref(),
        Some(refill.as_str())
    );
    let member = window_row(&settled, "win-a");
    assert_eq!(member.membership_authority, ScopeAuthority::Controlled);
    assert_eq!(member.frame_authority, ScopeAuthority::Controlled);
    // Post-settlement reports stay settled and keep mapping authority.
    let calm = accepted(
        &mut engine,
        &report(
            7,
            7,
            two_screens(),
            vec![fact("win-a", "out-b", 800)],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            false,
        ),
    );
    assert!(calm.transfers.is_empty());
    let row = calm
        .workspaces
        .iter()
        .find(|row| row.id == view_a)
        .expect("migrated row");
    assert_eq!(row.output.0, "out-b");
}

#[test]
fn migration_holds_narrowly_on_incomplete_unreadable_and_gesture_evidence() {
    for (name, mutate) in [
        ("incomplete-inventory", true),
        ("unreadable-screen", false),
        ("gesture", false),
    ] as [(&str, bool); 3]
    {
        let mut engine =
            WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
        let first = accepted(
            &mut engine,
            &report(
                1,
                1,
                two_screens(),
                vec![fact("win-a", "out-a", 0)],
                vec![],
                vec![],
                "",
                None,
                None,
                true,
                false,
            ),
        );
        let view_a = output_view(&first, "out-a").expect("view a");
        let commit = accepted(
            &mut engine,
            &report(
                2,
                2,
                two_screens(),
                vec![fact("win-a", "out-a", 0)],
                vec![],
                vec![],
                "",
                Some(11),
                Some(WorldCommand::MigrateWorkspace {
                    source_output: OutputId("out-a".to_owned()),
                    target_output: OutputId("out-b".to_owned()),
                    workspace: crate::directional::WorkspaceId(view_a.clone()),
                }),
                true,
                false,
            ),
        );
        assert!(
            commit
                .transfers
                .iter()
                .any(|t| matches!(t.kind, TransferKind::Migration)),
            "{name}"
        );
        // Gap evidence holds the flight without settling or flapping.
        let mut screens = two_screens();
        if name == "unreadable-screen" {
            screens[1].complete = false;
            screens[1].full_rect = None;
            screens[1].work_area = None;
        }
        let held = accepted(
            &mut engine,
            &report(
                3,
                3,
                screens,
                vec![fact("win-a", "out-a", 0)],
                vec![],
                vec![],
                "",
                None,
                None,
                !mutate,
                name == "gesture",
            ),
        );
        assert!(
            held.transfers
                .iter()
                .any(|t| matches!(t.kind, TransferKind::Migration) && t.operation_id == "op-11"),
            "{name}: flight held, got {:?}",
            held.transfers
        );
        let member = window_row(&held, "win-a");
        assert_eq!(
            (member.output.0.as_str(), member.workspace.as_str()),
            ("out-b", view_a.as_str()),
            "{name}"
        );
    }
}

#[test]
fn migration_refused_or_superseded_settles_without_fighting() {
    use crate::world_report::{FrameSetter, MemberAttempt, ViewAttempt};
    for name in ["refused", "superseded"] {
        let mut engine =
            WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
        let first = accepted(
            &mut engine,
            &report(
                1,
                1,
                two_screens(),
                vec![fact("win-a", "out-a", 0)],
                vec![],
                vec![],
                "",
                None,
                None,
                true,
                false,
            ),
        );
        let view_a = output_view(&first, "out-a").expect("view a");
        accepted(
            &mut engine,
            &report(
                2,
                2,
                two_screens(),
                vec![fact("win-a", "out-a", 0)],
                vec![],
                vec![],
                "",
                Some(11),
                Some(WorldCommand::MigrateWorkspace {
                    source_output: OutputId("out-a".to_owned()),
                    target_output: OutputId("out-b".to_owned()),
                    workspace: crate::directional::WorkspaceId(view_a.clone()),
                }),
                true,
                false,
            ),
        );
        // Refused membership remains meaningful native-free evidence. A
        // logical selector attempt is rejected under v4 rather than being
        // used as proof of external supersession.
        let member_attempts = if name == "refused" {
            vec![MemberAttempt {
                operation_id: "op-11".to_owned(),
                window: WindowId("win-a".to_owned()),
                target_output: OutputId("out-b".to_owned()),
                target_workspace: view_a.clone(),
                setter: FrameSetter::Refused.into(),
                pre: Some(crate::world_report::WorkspaceRef::Logical {
                    output: OutputId("out-a".to_owned()),
                    workspace: crate::directional::WorkspaceId(view_a.clone()),
                }),
                post: Some(crate::world_report::WorkspaceRef::Logical {
                    output: OutputId("out-a".to_owned()),
                    workspace: crate::directional::WorkspaceId(view_a.clone()),
                }),
                attempt_ms: 3,
            }]
        } else {
            Vec::new()
        };
        let make_report = |view_attempts| {
            report_attempts(
                3,
                3,
                two_screens(),
                vec![fact("win-a", "out-a", 0)],
                vec![],
                vec![],
                "",
                None,
                None,
                true,
                false,
                crate::workspace_scope::WorkspaceMode::PerOutputLocal,
                member_attempts.clone(),
                view_attempts,
            )
        };
        let settled = if name == "superseded" {
            let logical_attempt = ViewAttempt {
                operation_id: "op-11".to_owned(),
                target_output: OutputId("out-b".to_owned()),
                target_workspace: view_a.clone(),
                setter: FrameSetter::Threw,
                pre: None,
                post: Some(crate::world_report::WorkspaceRef::Logical {
                    output: OutputId("out-b".to_owned()),
                    workspace: crate::directional::WorkspaceId(view_a.clone()),
                }),
                baseline: Some(crate::world_report::WorkspaceRef::Logical {
                    output: OutputId("out-b".to_owned()),
                    workspace: crate::directional::WorkspaceId(view_a.clone()),
                }),
                attempt_ms: 3,
            };
            match engine
                .handle_report(&make_report(vec![logical_attempt]), &[])
                .0
            {
                WorldOutcome::Rejected { message, .. } => {
                    assert_eq!(message, "native-without-capability")
                }
                other => panic!("logical ViewAttempt must reject: {other:?}"),
            }
            accepted(&mut engine, &make_report(Vec::new()))
        } else {
            accepted(&mut engine, &make_report(Vec::new()))
        };
        if name == "refused" {
            assert!(
                settled.transfers.is_empty(),
                "{name}: {:?}",
                settled.transfers
            );
        } else {
            assert!(
                settled
                    .transfers
                    .iter()
                    .any(|transfer| { matches!(transfer.kind, TransferKind::Migration) }),
                "rejected selector cannot settle migration: {:?}",
                settled.transfers
            );
            assert!(
                settled
                    .windows
                    .iter()
                    .any(|member| member.workspace == view_a)
            );
        }
        let row = settled
            .workspaces
            .iter()
            .find(|row| row.id == view_a)
            .expect("mapping restored");
        if name == "refused" {
            assert_eq!(row.output.0, "out-a", "{name}");
            assert_eq!(
                output_view(&settled, "out-a").as_deref(),
                Some(view_a.as_str()),
                "{name}"
            );
        }
        // Externally moved members settle without a fight.
        let mut engine =
            WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
        let first = accepted(
            &mut engine,
            &report(
                1,
                1,
                two_screens(),
                vec![fact("win-a", "out-a", 0)],
                vec![],
                vec![],
                "",
                None,
                None,
                true,
                false,
            ),
        );
        let view_a = output_view(&first, "out-a").expect("view a");
        accepted(
            &mut engine,
            &report(
                2,
                2,
                two_screens(),
                vec![fact("win-a", "out-a", 0)],
                vec![],
                vec![],
                "",
                Some(11),
                Some(WorldCommand::MigrateWorkspace {
                    source_output: OutputId("out-a".to_owned()),
                    target_output: OutputId("out-b".to_owned()),
                    workspace: crate::directional::WorkspaceId(view_a.clone()),
                }),
                true,
                false,
            ),
        );
        let moved = accepted(
            &mut engine,
            &report(
                3,
                3,
                two_screens(),
                vec![fact("win-a", "out-b", 100)],
                vec![],
                vec![],
                "",
                None,
                None,
                true,
                false,
            ),
        );
        // Arrival on the target screen retires the native-free parent with
        // no echo proof: membership arrival is the settlement, and the
        // Switch pends its own obligations invisibly.
        assert!(
            moved
                .transfers
                .iter()
                .all(|t| !matches!(t.kind, TransferKind::Migration)),
            "{name}"
        );
    }
}

#[test]
fn migration_explicit_close_drops_members_without_a_fight() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            two_screens(),
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            false,
        ),
    );
    let view_a = output_view(&first, "out-a").expect("view a");
    accepted(
        &mut engine,
        &report(
            2,
            2,
            two_screens(),
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "",
            Some(11),
            Some(WorldCommand::MigrateWorkspace {
                source_output: OutputId("out-a".to_owned()),
                target_output: OutputId("out-b".to_owned()),
                workspace: crate::directional::WorkspaceId(view_a.clone()),
            }),
            true,
            false,
        ),
    );
    // Closing one member mid-flight keeps the other outstanding.
    let partial = accepted(
        &mut engine,
        &report(
            3,
            3,
            two_screens(),
            vec![fact("win-b", "out-a", 100)],
            vec![],
            vec!["win-a"],
            "",
            None,
            None,
            true,
            false,
        ),
    );
    assert!(
        partial
            .transfers
            .iter()
            .any(|t| matches!(t.kind, TransferKind::Migration)),
        "one member left: {:?}",
        partial.transfers
    );
    // Closing the last member settles to observed (all superseded/closed).
    let settled = accepted(
        &mut engine,
        &report(
            4,
            4,
            two_screens(),
            vec![],
            vec![],
            vec!["win-a", "win-b"],
            "",
            None,
            None,
            true,
            false,
        ),
    );
    assert!(
        settled
            .transfers
            .iter()
            .all(|t| !matches!(t.kind, TransferKind::Migration)),
        "settled: {:?}",
        settled.transfers
    );
}

#[test]
fn multi_member_migration_preserves_tree_and_focus() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            two_screens(),
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            false,
        ),
    );
    let view_a = output_view(&first, "out-a").expect("view a");
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            two_screens(),
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "",
            Some(11),
            Some(WorldCommand::MigrateWorkspace {
                source_output: OutputId("out-a".to_owned()),
                target_output: OutputId("out-b".to_owned()),
                workspace: crate::directional::WorkspaceId(view_a.clone()),
            }),
            true,
            false,
        ),
    );
    // Both members ride the target homing as one unit.
    for win in ["win-a", "win-b"] {
        let member = window_row(&reply, win);
        assert_eq!(
            (member.output.0.as_str(), member.workspace.as_str()),
            ("out-b", view_a.as_str()),
            "{win}"
        );
        assert_eq!(
            member.membership_authority,
            ScopeAuthority::Controlled,
            "{win}"
        );
    }
    // The target session preserves both links (tree/order/shares kept).
    let target_key = crate::session::DomainKey {
        output: OutputId("out-b".to_owned()),
        workspace: crate::directional::WorkspaceId(view_a.clone()),
    };
    let mut homed: Vec<String> = engine
        .world
        .layout()
        .tiled_windows(&target_key)
        .map(|window| window.0.clone())
        .collect();
    homed.sort();
    assert_eq!(homed, vec!["win-a".to_owned(), "win-b".to_owned()]);
    // Both arrivals plus both views settle together.
    let settled = accepted(
        &mut engine,
        &report(
            3,
            3,
            two_screens(),
            vec![fact("win-a", "out-b", 800), fact("win-b", "out-b", 900)],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            false,
        ),
    );
    let _refill = output_view(&settled, "out-a").expect("refill");
    let settled = accepted(
        &mut engine,
        &report(
            4,
            4,
            two_screens(),
            vec![fact("win-a", "out-b", 800), fact("win-b", "out-b", 900)],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            false,
        ),
    );
    assert!(
        settled.transfers.is_empty(),
        "settled: {:?}",
        settled.transfers
    );
    for win in ["win-a", "win-b"] {
        let member = window_row(&settled, win);
        assert_eq!(member.frame_authority, ScopeAuthority::Controlled, "{win}");
    }
}

#[test]
fn empty_migration_binds_focusless_and_retires_without_echoes() {
    // An empty native-free migration binds a focusless zero-obligation
    // selection (no members, no feedback gate, no native obligation),
    // which completes into the issuing reply, and retains no Migration
    // parent at all: zero members means zero membership to await. No
    // refill child, no future wake, no new logical echoes.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            two_screens(),
            vec![fact("win-a", "out-a", 0)],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            false,
        ),
    );
    let ring_a: Vec<String> = first
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    assert_eq!(ring_a.len(), 2);
    // Migrate the trailing empty: no members, no proofs needed.
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            two_screens(),
            vec![],
            vec![],
            vec![],
            "",
            Some(11),
            Some(WorldCommand::MigrateWorkspace {
                source_output: OutputId("out-a".to_owned()),
                target_output: OutputId("out-b".to_owned()),
                workspace: crate::directional::WorkspaceId(ring_a[1].clone()),
            }),
            true,
            false,
        ),
    );
    assert!(
        reply
            .transfers
            .iter()
            .all(|t| !matches!(t.kind, TransferKind::Migration)),
        "zero-member native-free migration retains no parent: {:?}",
        reply.transfers
    );
    // The zero-obligation binding already completed at issuing: no
    // retained selection, no refill child, and no Migration parent.
    assert!(
        engine.world.settlement.retained_selection.is_empty(),
        "zero-member binding completes at issuing"
    );
    assert!(
        reply
            .transfers
            .iter()
            .all(|t| t.parent.is_none() && !matches!(t.kind, TransferKind::View)),
        "no refill child on the native-free route: {:?}",
        reply.transfers
    );
    // The emptied source shows a fresh trailing workspace; with no
    // parent retained there is nothing to settle with echo proof.
    let target = ring_a[1].clone();
    let settled = accepted(
        &mut engine,
        &report(
            3,
            3,
            two_screens(),
            vec![],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            false,
        ),
    );
    assert!(
        settled.transfers.is_empty(),
        "settled: {:?}",
        settled.transfers
    );
    let source_view = output_view(&settled, "out-a").expect("source view");
    assert_ne!(source_view, target);
}

#[test]
fn migration_refill_prefers_mru_history() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            two_screens(),
            vec![fact("win-a", "out-a", 0)],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            false,
        ),
    );
    let ring_a = workspace_ids(&first, "out-a");
    assert_eq!(ring_a.len(), 2);
    let (head, spare) = (ring_a[0].clone(), ring_a[1].clone());
    // Build MRU: view the spare, then return to the head.
    let mut r = report(
        2,
        2,
        two_screens(),
        vec![fact("win-a", "out-a", 0)],
        vec![],
        vec![],
        "",
        Some(11),
        Some(WorldCommand::SelectWorkspace {
            output: OutputId("out-a".to_owned()),
            workspace: crate::directional::WorkspaceId(spare.clone()),
        }),
        true,
        false,
    );
    accepted(&mut engine, &r);
    r = report(
        3,
        3,
        two_screens(),
        vec![fact("win-a", "out-a", 0)],
        vec![],
        vec![],
        "",
        Some(12),
        Some(WorldCommand::SelectWorkspace {
            output: OutputId("out-a".to_owned()),
            workspace: crate::directional::WorkspaceId(head.clone()),
        }),
        true,
        false,
    );
    accepted(&mut engine, &r);
    // Migrate the head with MRU refill: the source refill is the spare.
    let migrate = report(
        4,
        4,
        two_screens(),
        vec![fact("win-a", "out-a", 0)],
        vec![],
        vec![],
        "",
        Some(13),
        Some(WorldCommand::MigrateWorkspace {
            source_output: OutputId("out-a".to_owned()),
            target_output: OutputId("out-b".to_owned()),
            workspace: crate::directional::WorkspaceId(head.clone()),
        }),
        true,
        false,
    );
    engine
        .apply_test_settings(
            1,
            vec![(
                "refill".to_owned(),
                crate::connection::RawSettingValue::String("most-recently-used".to_owned()),
            )],
        )
        .expect("settings accepted");
    let reply = accepted(&mut engine, &migrate);
    // The MRU refill rides the frozen binding's source selected home: no
    // refill child exists on the native-free route.
    assert!(
        reply
            .transfers
            .iter()
            .all(|t| t.parent.is_none() && !matches!(t.kind, TransferKind::View)),
        "no refill child on the native-free route: {:?}",
        reply.transfers
    );
    let binding = migrated_selection_binding(&engine, "op-13");
    let refill = binding
        .selected
        .iter()
        .find(|home| home.output.0 == "out-a")
        .map(|home| home.workspace.0.clone())
        .expect("mru refill");
    assert_eq!(refill, spare);
}

#[test]
fn tiled_to_float_output_crossing_leaves_source_state_unchanged() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            two_screens(),
            vec![fact("win-a", "out-a", 0)],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            false,
        ),
    );
    let ring_b = workspace_ids(&first, "out-b");
    // Float the target workspace, then send across outputs into it.
    let floated = accepted(
        &mut engine,
        &report(
            2,
            2,
            two_screens(),
            vec![fact("win-a", "out-a", 0)],
            vec![],
            vec![],
            "",
            Some(11),
            Some(WorldCommand::SetWorkspaceTiling {
                output: OutputId("out-b".to_owned()),
                workspace: crate::directional::WorkspaceId(ring_b[0].clone()),
                tiled: false,
            }),
            true,
            false,
        ),
    );
    assert!(
        floated
            .scopes
            .iter()
            .any(|status| status.authority == ScopeAuthority::Untouched)
    );
    let reply = accepted(
        &mut engine,
        &report(
            3,
            3,
            two_screens(),
            vec![fact("win-a", "out-a", 0)],
            vec![],
            vec![],
            "win-a",
            Some(12),
            Some(WorldCommand::SendDirectional {
                window: crate::directional::WindowSubject::Window(WindowId("win-a".to_owned())),
                direction: crate::directional::Direction::Right,
                follow: true,
            }),
            true,
            false,
        ),
    );
    // Membership-only boundary transfer: no two-domain Engine plan ran,
    // so the committed source session still homes the window.
    assert_eq!(reply.transfers.len(), 1);
    let source_key = crate::session::DomainKey {
        output: OutputId("out-a".to_owned()),
        workspace: crate::directional::WorkspaceId(output_view(&first, "out-a").expect("view a")),
    };
    assert!(
        engine
            .world
            .layout()
            .is_tiled_window(&source_key, &WindowId("win-a".to_owned()))
    );
    // The mover rides membership at the target with no frame authority
    // and no diagnostic float origin on the boundary row.
    let row = window_row(&reply, "win-a");
    assert_eq!(row.membership_authority, ScopeAuthority::Controlled);
    assert_ne!(row.frame_authority, ScopeAuthority::Controlled);
    assert!(row.float_origin.is_none());
}

#[test]
fn minimized_fullscreen_and_hidden_facts_stay_truthful() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
    accepted(
        &mut engine,
        &report(
            1,
            1,
            two_screens(),
            vec![fact("win-a", "out-a", 0)],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            false,
        ),
    );
    let mut hidden = fact("win-full", "out-a", 100);
    hidden.fullscreen = Some(true);
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            two_screens(),
            vec![
                fact("win-a", "out-a", 0),
                minimized_fact("win-min", "out-a"),
                hidden,
            ],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            false,
        ),
    );
    // Minimized: explicit minimized-hold frame row with live membership.
    let row = window_row(&reply, "win-min");
    assert_eq!(
        row.frame_authority,
        ScopeAuthority::Hold(HoldReason::Minimized)
    );
    assert_eq!(row.membership_authority, ScopeAuthority::Controlled);
    assert!(row.allocation.is_none() && row.effective_frame.is_none());
    // Fullscreen tiled: the overlay is adapter-owned, so the row carries
    // no frame authority and no membership authority in a controlled
    // scope (explicit untouched row, never a frame write).
    let row = window_row(&reply, "win-full");
    assert!(row.allocation.is_none());
    assert!(row.effective_frame.is_none());
    assert_eq!(row.frame_authority, ScopeAuthority::Untouched);
    assert_eq!(row.membership_authority, ScopeAuthority::Untouched);
}

#[test]
fn unexplained_fit_excluded_newcomers_hold_their_scope() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            two_screens(),
            vec![],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            false,
        ),
    );
    let view_a = output_view(&first, "out-a").expect("view a");
    // A safety-excluded window (privileged game/overlay) rides with no
    // membership row and never takes automatic activation itself: any
    // admission intent names only ordinary newcomers. The scope stays
    // controlled around the excluded overlay.
    let mut overlay = fact("win-game", "out-a", 100);
    overlay.privileged = Some(true);
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            two_screens(),
            vec![fact("win-a", "out-a", 0), overlay],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            false,
        ),
    );
    assert_eq!(
        scope_of(&reply, "out-a", &view_a),
        ScopeAuthority::Controlled
    );
    // The game carries no membership row (safety exclusion homes
    // nothing) with no automatic activation of its own.
    assert!(
        reply.windows.iter().all(|row| row.lifetime.0 != "win-game"),
        "excluded overlay has no row: {:?}",
        reply.windows
    );
}

#[test]
fn gesture_barrier_holds_frames_without_repair() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
    accepted(
        &mut engine,
        &report(
            1,
            1,
            two_screens(),
            vec![fact("win-a", "out-a", 0)],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            false,
        ),
    );
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            two_screens(),
            vec![fact("win-a", "out-a", 0)],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            true,
        ),
    );
    let row = window_row(&reply, "win-a");
    assert_eq!(
        row.frame_authority,
        ScopeAuthority::Hold(HoldReason::GestureBarrier)
    );
    assert_eq!(row.membership_authority, ScopeAuthority::Controlled);
    // A newcomer arriving mid-gesture spends its admission without emitting.
    let reply = accepted(
        &mut engine,
        &report(
            3,
            3,
            two_screens(),
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            true,
        ),
    );
    assert!(reply.focus_request.is_none());
    let replay = accepted(
        &mut engine,
        &report(
            4,
            4,
            two_screens(),
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            false,
        ),
    );
    assert!(replay.focus_request.is_none());
}

#[test]
fn generations_watermarks_and_command_consumption_hold() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
    accepted(
        &mut engine,
        &report(
            1,
            1,
            two_screens(),
            vec![],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            false,
        ),
    );
    // Duplicate latest report returns the cached reply with no revision.
    let first = accepted(
        &mut engine,
        &report(
            2,
            2,
            two_screens(),
            vec![],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            false,
        ),
    );
    match engine
        .handle_report(
            &report(
                2,
                2,
                two_screens(),
                vec![],
                vec![],
                vec![],
                "",
                None,
                None,
                true,
                false,
            ),
            &[],
        )
        .0
    {
        WorldOutcome::Duplicate(cached) => {
            assert_eq!(cached.state_revision, first.state_revision)
        }
        other => panic!("expected duplicate, got {other:?}"),
    }
    // Reused sequence with different input is invalid, never executed.
    match engine
        .handle_report(
            &report(
                2,
                2,
                two_screens(),
                vec![fact("win-a", "out-a", 0)],
                vec![],
                vec![],
                "",
                None,
                None,
                true,
                false,
            ),
            &[],
        )
        .0
    {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "request-reused"),
        other => panic!("expected request-reused, got {other:?}"),
    }
    // Older sequences are superseded.
    match engine
        .handle_report(
            &report(
                1,
                1,
                two_screens(),
                vec![],
                vec![],
                vec![],
                "",
                None,
                None,
                true,
                false,
            ),
            &[],
        )
        .0
    {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "request-superseded"),
        other => panic!("expected request-superseded, got {other:?}"),
    }
    // Foreign generations never reset ordering.
    match engine
        .handle_report(
            &report(
                3,
                3,
                two_screens(),
                vec![fact("win-a", "out-a", 0)],
                vec![],
                vec![],
                "",
                None,
                None,
                true,
                false,
            ),
            &[],
        )
        .0
    {
        WorldOutcome::Accepted(_) => {}
        other => panic!("expected accepted, got {other:?}"),
    }
    // A consumed command reconciles observations without re-executing.
    let view_reply = accepted(
        &mut engine,
        &report(
            4,
            4,
            two_screens(),
            vec![],
            vec![],
            vec![],
            "",
            Some(11),
            Some(WorldCommand::SelectTrailing {
                output: Some(OutputId("out-a".to_owned())),
            }),
            true,
            false,
        ),
    );
    // An empty trailing select completes at issuing: the terminal carries
    // the switch outcome and the view parent retires with it.
    assert_eq!(view_reply.switch_settled.len(), 1);
    let transfers = view_reply.transfers.len();
    assert_eq!(transfers, 0);
    match engine
        .handle_report(
            &report(
                5,
                5,
                two_screens(),
                vec![],
                vec![],
                vec![],
                "",
                Some(11),
                Some(WorldCommand::SelectTrailing {
                    output: Some(OutputId("out-a".to_owned())),
                }),
                true,
                false,
            ),
            &[],
        )
        .0
    {
        WorldOutcome::CommandConsumed(reply) => assert_eq!(reply.transfers.len(), transfers),
        other => panic!("expected command-consumed, got {other:?}"),
    }
    // A wrong generation is rejected outright.
    let mut bad = report(
        6,
        6,
        two_screens(),
        vec![],
        vec![],
        vec![],
        "",
        None,
        None,
        true,
        false,
    );
    bad.core_generation = "gen-2".to_owned();
    match engine.handle_report(&bad, &[]).0 {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "generation-mismatch"),
        other => panic!("expected generation-mismatch, got {other:?}"),
    }
}

#[test]
fn unknown_outputs_hold_fail_closed() {
    // Retired: the legacy `views`/`ScreenView` unknown-echo fail-closed
    // hold (world-contract.md:349-352 removes `views`; G1 native-only
    // logical echo cannot prove completion; the v4 wire refuses views
    // outright) has no v4 lane, so the echo arm is dropped. What stays:
    // facts on unknown screens still route on the v4 lane.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
    accepted(
        &mut engine,
        &report(
            1,
            1,
            two_screens(),
            vec![fact("win-a", "out-a", 0)],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            false,
        ),
    );
    // Facts on unknown screens route: the new screen joins the scopes.
    let mut screens = two_screens();
    screens.push(screen("out-z", 1600));
    let ok = accepted(
        &mut engine,
        &report(
            2,
            2,
            screens,
            vec![fact("win-a", "out-a", 0), fact("win-x", "out-z", 0)],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            false,
        ),
    );
    assert!(ok.scopes.iter().any(|status| {
        status
            .scope
            .covers_workspace(&OutputId("out-z".into()), "ws-1")
    }));
}

#[test]
fn host_owned_capability_gates_native_observations() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
    // Native observations without the capability refuse.
    let mut bad = report(
        1,
        1,
        two_screens(),
        vec![],
        vec![],
        vec![],
        "",
        None,
        None,
        true,
        false,
    );
    bad.native.push(crate::world_report::NativeObservation {
        output: OutputId("out-a".to_owned()),
        native_order: vec!["ws".to_owned()],
        current_native: Some("ws".to_owned()),
        occupancy: vec![crate::world_report::NativeOccupancy {
            native: "ws".to_owned(),
            count: None,
        }],
    });
    match engine.handle_report(&bad, &[]).0 {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "snapshot-invalid"),
        other => panic!("expected snapshot-invalid, got {other:?}"),
    }
    // With the capability native observations adopt: out-a builds one
    // logical per native id in native order and publishes the binding;
    // the viewed single empty keeps no spare.
    let mut capable_engine =
        WorldEngine::new_test_host_owned("gen-1", "adapter-1").expect("valid generation");
    let mut good = report(
        1,
        1,
        two_screens(),
        vec![],
        vec![],
        vec![],
        "",
        None,
        None,
        true,
        false,
    );
    good.native.push(crate::world_report::NativeObservation {
        output: OutputId("out-a".to_owned()),
        native_order: vec!["ws".to_owned()],
        current_native: Some("ws".to_owned()),
        occupancy: vec![crate::world_report::NativeOccupancy {
            native: "ws".to_owned(),
            count: None,
        }],
    });
    let reply = accepted(&mut capable_engine, &good);
    assert!(reply.ready);
    let bound = reply
        .workspaces
        .iter()
        .find(|row| row.output.0 == "out-a" && row.native_binding.as_deref() == Some("ws"))
        .expect("bound row publishes its native id");
    assert_eq!(bound.order, 0);
    let ring_a: Vec<String> = reply
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    assert_eq!(
        ring_a,
        vec![bound.id.clone()],
        "viewed single empty keeps no spare: {ring_a:?}"
    );
    // Native-free output stays unbound.
    assert!(
        reply
            .workspaces
            .iter()
            .filter(|row| row.output.0 == "out-b")
            .all(|row| row.native_binding.is_none())
    );
}

#[test]
fn hotplug_displacement_preserves_logical_units_on_the_survivor() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
    accepted(
        &mut engine,
        &report(
            1,
            1,
            two_screens(),
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-b", 800)],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            false,
        ),
    );
    // out-b disconnects: its logical units displace onto the survivor.
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![screen("out-a", 0)],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            false,
        ),
    );
    let ids = workspace_ids(&reply, "out-a");
    assert!(ids.len() >= 3, "displaced units hosted: {ids:?}");
    // The displaced member stays homed to its logical id on the survivor.
    let survivor: Vec<&str> = reply
        .windows
        .iter()
        .filter(|row| row.lifetime.0 == "win-b")
        .map(|row| row.workspace.as_str())
        .collect();
    assert_eq!(survivor.len(), 1);
    assert!(ids.contains(&survivor[0].to_owned()));
    // Return reclaims the units back onto the origin.
    let back = accepted(
        &mut engine,
        &report(
            3,
            3,
            two_screens(),
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-b", 800)],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            false,
        ),
    );
    assert!(
        !engine.world.workspace.displaced.contains_key("out-b"),
        "same-panel return reclaims by physical identity: {:?}",
        engine.world.workspace.displaced
    );
    assert!(!workspace_ids(&back, "out-b").is_empty());
}

#[test]
fn cross_output_follow_ineligible_moves_view_without_binding() {
    // Ineligible cross-output follow (no suppression support in these
    // facts): the logical view still moves at command time under the
    // stable window identity with no frozen binding and legacy direct
    // focus. Membership retires the window flight on homing; the echoed
    // view alone settles nothing further (no binding, no terminal).
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            two_screens(),
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            false,
        ),
    );
    let view_b = output_view(&first, "out-b").expect("view b");
    let followed = accepted(
        &mut engine,
        &report(
            2,
            2,
            two_screens(),
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "win-a",
            Some(11),
            Some(WorldCommand::SendDirectional {
                window: crate::directional::WindowSubject::Window(WindowId("win-a".to_owned())),
                direction: crate::directional::Direction::Right,
                follow: true,
            }),
            true,
            false,
        ),
    );
    assert_eq!(followed.transfers.len(), 1, "window flight only");
    assert!(matches!(
        &followed.transfers[0].kind,
        TransferKind::Window(_)
    ));
    assert_eq!(followed.transfers[0].operation_id, "op-11");
    assert_eq!(followed.transfers[0].target_workspace, view_b);
    assert!(
        !engine
            .world
            .settlement
            .retained_selection
            .contains_key(&OutputId("out-b".to_owned())),
        "ineligible mover binds nothing"
    );
    let request = followed
        .focus_request
        .as_ref()
        .expect("explicit focus request");
    assert_eq!(request.target.0, "win-a");
    assert_eq!(request.id, format!("focus-{}", followed.state_revision));
    assert_eq!(request.issued_revision, followed.state_revision);
    assert_eq!(
        request.cause,
        crate::world_report::FocusRequestCause::ExplicitCommand
    );
    assert_eq!(request.cmd_seq, Some(11));
    assert_eq!(request.after_shown, None);
    // Membership arrival retires the window flight with no binding and no
    // terminal.
    let arrived = accepted(
        &mut engine,
        &report(
            3,
            3,
            two_screens(),
            vec![fact("win-a", "out-b", 800), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            false,
        ),
    );
    assert!(
        arrived.transfers.iter().all(|t| t.operation_id != "op-11"),
        "window retires on homing: {:?}",
        arrived.transfers
    );
    assert!(
        !engine
            .world
            .settlement
            .retained_selection
            .contains_key(&OutputId("out-b".to_owned())),
        "no binding pends"
    );
    assert!(
        arrived.switch_settled.is_empty(),
        "no terminal without a binding: {:?}",
        arrived.switch_settled
    );
    // Ack with no new attempt: still nothing (no binding to persist).
    let acked = accepted(
        &mut engine,
        &report(
            4,
            4,
            two_screens(),
            vec![fact("win-a", "out-b", 800), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            false,
        ),
    );
    assert!(
        acked.transfers.iter().all(|t| t.operation_id != "op-11"),
        "nothing persists across ack: {:?}",
        acked.transfers
    );
    assert!(acked.switch_settled.is_empty());
    // Gap also holds nothing.
    let gapped = accepted(
        &mut engine,
        &report(
            5,
            5,
            two_screens(),
            vec![fact("win-a", "out-b", 800), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "",
            None,
            None,
            false,
            false,
        ),
    );
    assert!(
        gapped.transfers.iter().all(|t| t.operation_id != "op-11"),
        "nothing persists across gap: {:?}",
        gapped.transfers
    );
    assert!(gapped.switch_settled.is_empty());
    // Late exact view echoes with no further effect.
    let settled = accepted(
        &mut engine,
        &report(
            6,
            6,
            two_screens(),
            vec![fact("win-a", "out-b", 800), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            false,
        ),
    );
    assert!(
        settled.transfers.is_empty(),
        "echo proves nothing further: {:?}",
        settled.transfers
    );
    assert!(
        settled.switch_settled.is_empty(),
        "echo alone settles nothing: {:?}",
        settled.switch_settled
    );
}

#[test]
fn window_refusal_needs_readable_source_proof_and_holds_across_gap() {
    // Raw refusal alone across missing facts never settles: incomplete
    // inventory holds with the raw retained (as floats do). Target arrival
    // wins over stale refusal elsewhere; here the gap hold is the
    // regression.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            two_screens(),
            vec![fact("win-a", "out-a", 0)],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            false,
        ),
    );
    let view_a = output_view(&first, "out-a").expect("view a");
    let view_b = output_view(&first, "out-b").expect("view b");
    accepted(
        &mut engine,
        &report(
            2,
            2,
            two_screens(),
            vec![fact("win-a", "out-a", 0)],
            vec![],
            vec![],
            "win-a",
            Some(11),
            Some(WorldCommand::SendDirectional {
                window: crate::directional::WindowSubject::Window(WindowId("win-a".to_owned())),
                direction: crate::directional::Direction::Right,
                follow: false,
            }),
            true,
            false,
        ),
    );
    // Refusal with incomplete inventory: hold, never settle from absence.
    let gapped = accepted(
        &mut engine,
        &report_attempts(
            3,
            3,
            two_screens(),
            vec![],
            vec![],
            vec![],
            "",
            None,
            None,
            false,
            false,
            crate::workspace_scope::WorkspaceMode::PerOutputLocal,
            vec![crate::world_report::MemberAttempt {
                operation_id: "op-11".to_owned(),
                window: WindowId("win-a".to_owned()),
                target_output: OutputId("out-b".to_owned()),
                target_workspace: view_b.clone(),
                setter: crate::world_report::FrameSetter::Refused.into(),
                pre: Some(crate::world_report::WorkspaceRef::Logical {
                    output: OutputId("out-a".to_owned()),
                    workspace: crate::directional::WorkspaceId(view_a.clone()),
                }),
                post: Some(crate::world_report::WorkspaceRef::Logical {
                    output: OutputId("out-a".to_owned()),
                    workspace: crate::directional::WorkspaceId(view_a.clone()),
                }),
                attempt_ms: 3,
            }],
            Vec::new(),
        ),
    );
    assert!(
        gapped
            .transfers
            .iter()
            .any(|t| t.operation_id == "op-11" && t.held),
        "refusal across gap holds: {:?}",
        gapped.transfers
    );
}

#[test]
fn standalone_view_hold_completes_through_member_verdict_not_echo() {
    // Missing/gapped echo holds the flight for its frozen selection
    // binding; a later producer verdict for the frozen hide member
    // completes the common terminal and retires the flight. No echo lane
    // is consulted native-free: a views row naming the preimage proves
    // nothing on its own.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            two_screens(),
            vec![fact("win-a", "out-a", 0)],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            false,
        ),
    );
    let ring_a = workspace_ids(&first, "out-a");
    assert!(ring_a.len() >= 2);
    let target = ring_a[1].clone();
    let _current = output_view(&first, "out-a").expect("current view");
    accepted(
        &mut engine,
        &report(
            2,
            2,
            two_screens(),
            vec![fact("win-a", "out-a", 0)],
            vec![],
            vec![],
            "",
            Some(11),
            Some(WorldCommand::SelectWorkspace {
                output: OutputId("out-a".to_owned()),
                workspace: crate::directional::WorkspaceId(target.clone()),
            }),
            true,
            false,
        ),
    );
    // Gapped echo (missing) with refusal proof: hold.
    let gapped = accepted(
        &mut engine,
        &report_attempts(
            3,
            3,
            two_screens(),
            vec![fact("win-a", "out-a", 0)],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            false,
            crate::workspace_scope::WorkspaceMode::PerOutputLocal,
            Vec::new(),
            Vec::new(),
        ),
    );
    assert!(
        !gapped.transfers.is_empty(),
        "gapped view refusal holds: {:?}",
        gapped.transfers
    );
    // Later producer verdict for the frozen hide member completes the
    // common terminal and retires the flight: the views row below names
    // the preimage yet proves nothing on its own.
    let binding_rev = engine
        .world
        .settlement
        .retained_selection
        .values()
        .find(|binding| binding.parent_op == "op-11")
        .expect("frozen selection binding")
        .rev;
    let hidden = fact("win-a", "out-a", 0);
    let mut r4 = report(
        4,
        4,
        two_screens(),
        vec![hidden],
        vec![],
        vec![],
        "",
        None,
        None,
        true,
        false,
    );
    r4.visibility_attempts = vec![VisibilityAttempt {
        window: WindowId("win-a".to_owned()),
        target: false,
        revision: binding_rev,
        path: LastWritePath::Staged,
        setter: VisibilitySetter::Accepted,
        reason: VisibilityReason::Empty,
        pre: None,
        post: Some(VisibilitySample {
            obs: 4,
            lifetime_matched: Some(true),
            visibility: VisibilityFact {
                visible: Some(false),
                suppression_owned: Some(false),
            },
            minimized: Some(false),
            host_hidden: Some(false),
        }),
    }];
    let settled = accepted(&mut engine, &r4);
    assert!(
        settled.transfers.is_empty(),
        "verdict retires the flight: {:?}",
        settled.transfers
    );
    assert_eq!(
        settled.switch_settled.len(),
        1,
        "one terminal: {:?}",
        settled.switch_settled
    );
    assert_eq!(settled.switch_settled[0].operation_id, "op-11");
}

#[test]
fn standalone_view_refusal_holds_on_current_incomplete_screen() {
    // The refusal bar reads the CURRENT report screens: an incomplete
    // current screen holds even after a complete prior and even with a
    // readable echo naming the refused preimage. A later complete screen
    // with the same echo settles to observed.
    let mut engine =
        WorldEngine::new_test_host_owned("gen-1", "adapter-1").expect("valid generation");
    let first = accepted(
        &mut engine,
        &with_native_context(
            report(
                1,
                1,
                two_screens(),
                vec![fact("win-a", "out-a", 0)],
                vec![],
                vec![],
                "",
                None,
                None,
                true,
                false,
            ),
            "native-a-0",
            "native-b-0",
        ),
    );
    let ring_a = workspace_ids(&first, "out-a");
    assert!(ring_a.len() >= 2);
    let target = ring_a[1].clone();
    let _current = output_view(&first, "out-a").expect("current view");
    accepted(
        &mut engine,
        &with_native_context(
            report(
                2,
                2,
                two_screens(),
                vec![fact("win-a", "out-a", 0)],
                vec![],
                vec![],
                "",
                Some(11),
                Some(WorldCommand::SelectWorkspace {
                    output: OutputId("out-a".to_owned()),
                    workspace: crate::directional::WorkspaceId(target.clone()),
                }),
                true,
                false,
            ),
            "native-a-0",
            "native-b-0",
        ),
    );
    // Prior screens were complete; the current report marks out-a
    // incomplete while the echo still names the refused preimage.
    let mut incomplete_a = screen("out-a", 0);
    incomplete_a.complete = false;
    let incomplete = vec![incomplete_a, screen("out-b", 800)];
    let held = accepted(
        &mut engine,
        &with_native_context(
            report_attempts(
                3,
                3,
                incomplete,
                vec![fact("win-a", "out-a", 0)],
                vec![],
                vec![],
                "",
                None,
                None,
                true,
                false,
                crate::workspace_scope::WorkspaceMode::PerOutputLocal,
                Vec::new(),
                vec![crate::world_report::ViewAttempt {
                    operation_id: "op-11".to_owned(),
                    target_output: OutputId("out-a".to_owned()),
                    target_workspace: target.clone(),
                    setter: crate::world_report::FrameSetter::Refused,
                    pre: Some(crate::world_report::WorkspaceRef::Native {
                        output: OutputId("out-a".to_owned()),
                        workspace: crate::ids::NativeWorkspaceId::parse("native-a-0").unwrap(),
                    }),
                    post: Some(crate::world_report::WorkspaceRef::Native {
                        output: OutputId("out-a".to_owned()),
                        workspace: crate::ids::NativeWorkspaceId::parse("native-a-0").unwrap(),
                    }),
                    baseline: Some(crate::world_report::WorkspaceRef::Native {
                        output: OutputId("out-a".to_owned()),
                        workspace: crate::ids::NativeWorkspaceId::parse("native-a-1").unwrap(),
                    }),
                    attempt_ms: 3,
                }],
            ),
            "native-a-0",
            "native-b-0",
        ),
    );
    assert!(
        !held.transfers.is_empty(),
        "current incomplete screen holds: {:?}",
        held.transfers
    );
    // Same echo on a complete screen settles to observed.
    let settled = accepted(
        &mut engine,
        &with_native_context(
            report(
                4,
                4,
                two_screens(),
                vec![fact("win-a", "out-a", 0)],
                vec![],
                vec![],
                "",
                None,
                None,
                true,
                false,
            ),
            "native-a-0",
            "native-b-0",
        ),
    );
    assert!(
        settled.transfers.is_empty(),
        "complete screen with pre echo settles: {:?}",
        settled.transfers
    );
}

#[test]
fn floating_select_workspace_completes_empty_at_issuing() {
    // Standalone selection into an empty floating workspace completes at
    // issuing with an unproven terminal: the logical view moves
    // immediately, the target scope publishes Untouched, and the issuing
    // terminal retires the view parent. Later echoes preserve the logical
    // view with no duplicate terminal and no resurrected flight.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            two_screens(),
            // Occupy the head so the single-spare invariant supplies the
            // second row the selection below needs.
            vec![fact("win-a", "out-a", 0)],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            false,
        ),
    );
    let ring_a = workspace_ids(&first, "out-a");
    assert_eq!(ring_a.len(), 2);
    let other = ring_a[1].clone();
    accepted(
        &mut engine,
        &report(
            2,
            2,
            two_screens(),
            vec![fact("win-a", "out-a", 0)],
            vec![],
            vec![],
            "",
            Some(11),
            Some(WorldCommand::SetWorkspaceTiling {
                output: OutputId("out-a".to_owned()),
                workspace: crate::directional::WorkspaceId(other.clone()),
                tiled: false,
            }),
            true,
            false,
        ),
    );
    // Select away into the floating workspace.
    let selected = accepted(
        &mut engine,
        &report(
            3,
            3,
            two_screens(),
            vec![],
            vec![],
            vec![],
            "",
            Some(12),
            Some(WorldCommand::SelectWorkspace {
                output: OutputId("out-a".to_owned()),
                workspace: crate::directional::WorkspaceId(other.clone()),
            }),
            true,
            false,
        ),
    );
    assert_eq!(
        output_view(&selected, "out-a").as_deref(),
        Some(other.as_str())
    );
    assert_eq!(
        scope_of(&selected, "out-a", &other),
        ScopeAuthority::Untouched
    );
    assert_eq!(
        selected.switch_settled.len(),
        1,
        "empty select completes at issuing"
    );
    assert_eq!(
        selected.transfers.len(),
        0,
        "issuing terminal retires the view parent"
    );
    // A stale report with no echo keeps the logical view intact with the
    // retained issuing terminal and no resurrected flight.
    let stale = accepted(
        &mut engine,
        &report(
            4,
            4,
            two_screens(),
            vec![],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            false,
        ),
    );
    assert!(stale.transfers.is_empty(), "no resurrected flight");
    assert_eq!(
        stale.switch_settled, selected.switch_settled,
        "retains issuing terminal without re-settling"
    );
    assert_eq!(
        output_view(&stale, "out-a").as_deref(),
        Some(other.as_str())
    );
    // An incomplete inventory without the echo holds as well.
    let gapped = accepted(
        &mut engine,
        &report(
            5,
            5,
            two_screens(),
            vec![],
            vec![],
            vec![],
            "",
            None,
            None,
            false,
            false,
        ),
    );
    assert!(gapped.transfers.is_empty(), "no resurrected flight");
    // The exact echo confirms: the logical view is preserved (never
    // reverted, never re-emitted), with no duplicate terminal.
    let settled = accepted(
        &mut engine,
        &report(
            6,
            6,
            two_screens(),
            vec![],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            false,
        ),
    );
    assert!(
        settled.transfers.is_empty(),
        "echo settles: {:?}",
        settled.transfers
    );
    assert_eq!(
        output_view(&settled, "out-a").as_deref(),
        Some(other.as_str())
    );
}

#[test]
fn floating_select_previous_returns_to_floating_target() {
    // Previous-toggle through recorded history resolves a floating
    // workspace exactly like a tiled one: same issuing terminal and
    // Untouched scope, with the logical view preserved.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            two_screens(),
            vec![minimized_fact("win-a", "out-a")],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            false,
        ),
    );
    let ring_a = workspace_ids(&first, "out-a");
    assert_eq!(ring_a.len(), 2);
    let (head, other) = (ring_a[0].clone(), ring_a[1].clone());
    accepted(
        &mut engine,
        &report(
            2,
            2,
            two_screens(),
            vec![minimized_fact("win-a", "out-a")],
            vec![],
            vec![],
            "",
            Some(11),
            Some(WorldCommand::SetWorkspaceTiling {
                output: OutputId("out-a".to_owned()),
                workspace: crate::directional::WorkspaceId(other.clone()),
                tiled: false,
            }),
            true,
            false,
        ),
    );
    accepted(
        &mut engine,
        &report(
            3,
            3,
            two_screens(),
            vec![minimized_fact("win-a", "out-a")],
            vec![],
            vec![],
            "",
            Some(12),
            Some(WorldCommand::SelectWorkspace {
                output: OutputId("out-a".to_owned()),
                workspace: crate::directional::WorkspaceId(other.clone()),
            }),
            true,
            false,
        ),
    );
    accepted(
        &mut engine,
        &report(
            4,
            4,
            two_screens(),
            vec![minimized_fact("win-a", "out-a")],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            false,
        ),
    );
    accepted(
        &mut engine,
        &report(
            5,
            5,
            two_screens(),
            vec![minimized_fact("win-a", "out-a")],
            vec![],
            vec![],
            "",
            Some(13),
            Some(WorldCommand::SelectWorkspace {
                output: OutputId("out-a".to_owned()),
                workspace: crate::directional::WorkspaceId(head.clone()),
            }),
            true,
            false,
        ),
    );
    accepted(
        &mut engine,
        &report(
            6,
            6,
            two_screens(),
            vec![minimized_fact("win-a", "out-a")],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            false,
        ),
    );
    // Previous toggles back to the floating workspace.
    let back = accepted(
        &mut engine,
        &report(
            7,
            7,
            two_screens(),
            vec![minimized_fact("win-a", "out-a")],
            vec![],
            vec![],
            "",
            Some(14),
            Some(WorldCommand::SelectPrevious {
                output: Some(OutputId("out-a".to_owned())),
            }),
            true,
            false,
        ),
    );
    assert_eq!(output_view(&back, "out-a").as_deref(), Some(other.as_str()));
    assert_eq!(scope_of(&back, "out-a", &other), ScopeAuthority::Untouched);
    assert_eq!(
        back.switch_settled.len(),
        1,
        "empty previous-toggle completes at issuing"
    );
    assert!(
        back.transfers.is_empty(),
        "issuing terminal retires the view parent"
    );
    let settled = accepted(
        &mut engine,
        &report(
            8,
            8,
            two_screens(),
            vec![minimized_fact("win-a", "out-a")],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            false,
        ),
    );
    assert!(
        settled.transfers.is_empty(),
        "echo settles: {:?}",
        settled.transfers
    );
    assert_eq!(
        output_view(&settled, "out-a").as_deref(),
        Some(other.as_str())
    );
}

#[test]
fn floating_select_trailing_reuses_floating_empty() {
    // Trailing selection reuses the live trailing empty when it is
    // floating: same issuing terminal and Untouched scope, with the
    // logical view preserved across later echoes.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            two_screens(),
            vec![minimized_fact("win-a", "out-a")],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            false,
        ),
    );
    let ring_a = workspace_ids(&first, "out-a");
    assert_eq!(ring_a.len(), 2);
    let head = output_view(&first, "out-a").expect("view a");
    let trailing = ring_a
        .iter()
        .find(|id| *id != &head)
        .expect("trailing empty")
        .clone();
    // Occupy the trailing empty through selection, then float it.
    accepted(
        &mut engine,
        &report(
            2,
            2,
            two_screens(),
            vec![minimized_fact("win-a", "out-a")],
            vec![],
            vec![],
            "",
            Some(11),
            Some(WorldCommand::SelectTrailing {
                output: Some(OutputId("out-a".to_owned())),
            }),
            true,
            false,
        ),
    );
    accepted(
        &mut engine,
        &report(
            3,
            3,
            two_screens(),
            vec![minimized_fact("win-a", "out-a")],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            false,
        ),
    );
    accepted(
        &mut engine,
        &report(
            4,
            4,
            two_screens(),
            vec![minimized_fact("win-a", "out-a")],
            vec![],
            vec![],
            "",
            Some(12),
            Some(WorldCommand::SetWorkspaceTiling {
                output: OutputId("out-a".to_owned()),
                workspace: crate::directional::WorkspaceId(trailing.clone()),
                tiled: false,
            }),
            true,
            false,
        ),
    );
    // Select away to the tiled head, then trailing reuses the floating
    // empty.
    accepted(
        &mut engine,
        &report(
            5,
            5,
            two_screens(),
            vec![minimized_fact("win-a", "out-a")],
            vec![],
            vec![],
            "",
            Some(13),
            Some(WorldCommand::SelectWorkspace {
                output: OutputId("out-a".to_owned()),
                workspace: crate::directional::WorkspaceId(head.clone()),
            }),
            true,
            false,
        ),
    );
    accepted(
        &mut engine,
        &report(
            6,
            6,
            two_screens(),
            vec![minimized_fact("win-a", "out-a")],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            false,
        ),
    );
    let reused = accepted(
        &mut engine,
        &report(
            7,
            7,
            two_screens(),
            vec![minimized_fact("win-a", "out-a")],
            vec![],
            vec![],
            "",
            Some(14),
            Some(WorldCommand::SelectTrailing {
                output: Some(OutputId("out-a".to_owned())),
            }),
            true,
            false,
        ),
    );
    assert_eq!(
        output_view(&reused, "out-a").as_deref(),
        Some(trailing.as_str())
    );
    assert_eq!(
        scope_of(&reused, "out-a", &trailing),
        ScopeAuthority::Untouched
    );
    assert_eq!(
        reused.switch_settled.len(),
        1,
        "empty trailing reuses complete at issuing"
    );
    assert!(
        reused.transfers.is_empty(),
        "issuing terminal retires the view parent"
    );
    let settled = accepted(
        &mut engine,
        &report(
            8,
            8,
            two_screens(),
            vec![minimized_fact("win-a", "out-a")],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            false,
        ),
    );
    assert!(
        settled.transfers.is_empty(),
        "echo settles: {:?}",
        settled.transfers
    );
    assert_eq!(
        output_view(&settled, "out-a").as_deref(),
        Some(trailing.as_str())
    );
}

#[test]
fn floating_select_refusal_settles_to_observed_preserving_logical() {
    // A proven refusal of a floating-target selection settles the flight
    // to observed without reverting the logical selection: the retained
    // view stays at the floating target while native keeps showing the
    // refused preimage. Gapped echoes hold first, exactly like tiled
    // targets.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            two_screens(),
            vec![fact("win-a", "out-a", 0)],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            false,
        ),
    );
    let ring_a = workspace_ids(&first, "out-a");
    assert_eq!(ring_a.len(), 2);
    let current = output_view(&first, "out-a").expect("current view");
    let target = ring_a
        .iter()
        .find(|id| *id != &current)
        .expect("second workspace")
        .clone();
    accepted(
        &mut engine,
        &report(
            2,
            2,
            two_screens(),
            vec![],
            vec![],
            vec![],
            "",
            Some(11),
            Some(WorldCommand::SetWorkspaceTiling {
                output: OutputId("out-a".to_owned()),
                workspace: crate::directional::WorkspaceId(target.clone()),
                tiled: false,
            }),
            true,
            false,
        ),
    );
    let _selected = accepted(
        &mut engine,
        &report(
            3,
            3,
            two_screens(),
            vec![],
            vec![],
            vec![],
            "",
            Some(12),
            Some(WorldCommand::SelectWorkspace {
                output: OutputId("out-a".to_owned()),
                workspace: crate::directional::WorkspaceId(target.clone()),
            }),
            true,
            false,
        ),
    );
    // Logical selector attempts are rejected in v4 rather than acting as
    // synthetic proof. The floating workspace membership remains intact.
    match engine
        .handle_report(
            &report_attempts(
                4,
                4,
                two_screens(),
                vec![],
                vec![],
                vec![],
                "",
                None,
                None,
                true,
                false,
                crate::workspace_scope::WorkspaceMode::PerOutputLocal,
                Vec::new(),
                vec![crate::world_report::ViewAttempt {
                    operation_id: "op-12".to_owned(),
                    target_output: OutputId("out-a".to_owned()),
                    target_workspace: target.clone(),
                    setter: crate::world_report::FrameSetter::Refused,
                    pre: Some(crate::world_report::WorkspaceRef::Logical {
                        output: OutputId("out-a".to_owned()),
                        workspace: crate::directional::WorkspaceId(current.clone()),
                    }),
                    post: Some(crate::world_report::WorkspaceRef::Logical {
                        output: OutputId("out-a".to_owned()),
                        workspace: crate::directional::WorkspaceId(current.clone()),
                    }),
                    baseline: Some(crate::world_report::WorkspaceRef::Logical {
                        output: OutputId("out-a".to_owned()),
                        workspace: crate::directional::WorkspaceId(target.clone()),
                    }),
                    attempt_ms: 4,
                }],
            ),
            &[],
        )
        .0
    {
        WorldOutcome::Rejected { message, .. } => {
            assert_eq!(message, "native-without-capability")
        }
        other => panic!("native-free logical selector evidence must reject, got {other:?}"),
    }
    // A later observation settles independently; the selected workspace
    // remains a floating member even though logical selector proof was rejected.
    let settled = accepted(
        &mut engine,
        &report(
            5,
            5,
            two_screens(),
            vec![],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            false,
        ),
    );
    assert!(
        settled.transfers.is_empty(),
        "pre echo settles: {:?}",
        settled.transfers
    );
    let floating = settled
        .workspaces
        .iter()
        .find(|row| row.id == target)
        .expect("floating workspace retained");
    assert_eq!(
        floating.tiling_mode,
        crate::world_report::TilingMode::Floating,
        "floating membership retained: {floating:?}"
    );
}

#[test]
fn floating_migration_rekeys_floating_scope_and_migrates_back_sessionless() {
    // Migrating a floating workspace must carry its floating scope to the
    // target output on the commit reply itself: the tiling mode follows
    // the id through `carry_tiling`, and the scope authority must follow
    // with it instead of publishing the floating target as a tiled scope
    // until the next report recomputes it. The inverse migration takes
    // the same sessionless path (no Engine session is ever built for the
    // floating id), preserving floating support rather than refusing it.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            two_screens(),
            vec![fact("win-a", "out-a", 0)],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            false,
        ),
    );
    let view_a = output_view(&first, "out-a").expect("view a");
    let _other = workspace_ids(&first, "out-a")
        .into_iter()
        .find(|id| id != &view_a)
        .expect("second workspace");
    accepted(
        &mut engine,
        &report(
            2,
            2,
            two_screens(),
            vec![fact("win-a", "out-a", 0)],
            vec![],
            vec![],
            "",
            Some(11),
            Some(WorldCommand::SetWorkspaceTiling {
                output: OutputId("out-a".to_owned()),
                workspace: crate::directional::WorkspaceId(view_a.clone()),
                tiled: false,
            }),
            true,
            false,
        ),
    );
    let moved = accepted(
        &mut engine,
        &report(
            3,
            3,
            two_screens(),
            vec![fact("win-a", "out-a", 0)],
            vec![],
            vec![],
            "",
            Some(12),
            Some(WorldCommand::MigrateWorkspace {
                source_output: OutputId("out-a".to_owned()),
                target_output: OutputId("out-b".to_owned()),
                workspace: crate::directional::WorkspaceId(view_a.clone()),
            }),
            true,
            false,
        ),
    );
    // The commit reply already publishes the floating target Untouched:
    // the tiling mode followed the id, so the scope authority follows it.
    assert_eq!(
        scope_of(&moved, "out-b", &view_a),
        ScopeAuthority::Untouched
    );
    let row = moved
        .workspaces
        .iter()
        .find(|row| row.id == view_a)
        .expect("migrated row");
    assert_eq!(row.output.0, "out-b");
    assert_eq!(
        row.tiling_mode,
        crate::world_report::TilingMode::Floating,
        "tiling mode follows the id: {row:?}"
    );
    assert!(
        moved
            .transfers
            .iter()
            .any(|t| matches!(t.kind, TransferKind::Migration) && t.operation_id == "op-12")
    );
    // The floating sessionless carry binds the same frozen selection: no
    // refill child, both outputs covered with distinct selected homes.
    assert!(
        moved
            .transfers
            .iter()
            .all(|t| t.parent.is_none() && !matches!(t.kind, TransferKind::View)),
        "no refill child on the native-free route: {:?}",
        moved.transfers
    );
    let binding = migrated_selection_binding(&engine, "op-12");
    {
        let mut affected = binding.affected.clone();
        affected.sort();
        assert_eq!(
            affected,
            vec![OutputId("out-a".to_owned()), OutputId("out-b".to_owned())]
        );
    }
    let member = window_row(&moved, "win-a");
    assert_eq!(
        (member.output.0.as_str(), member.workspace.as_str()),
        ("out-b", view_a.as_str())
    );
    assert_eq!(member.membership_authority, ScopeAuthority::Controlled);
    // Member arrival retires the whole migration with no echo wait: the
    // flight drops instead of stalling on the floating target.
    let settled = accepted(
        &mut engine,
        &report(
            4,
            4,
            two_screens(),
            vec![fact("win-a", "out-b", 800)],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            false,
        ),
    );
    assert!(
        settled.transfers.is_empty(),
        "arrival plus views settle: {:?}",
        settled.transfers
    );
    // The inverse migration takes the sessionless floating path again:
    // no Engine session is built for the floating id on either side.
    let back = accepted(
        &mut engine,
        &report(
            5,
            5,
            two_screens(),
            vec![fact("win-a", "out-b", 800)],
            vec![],
            vec![],
            "",
            Some(13),
            Some(WorldCommand::MigrateWorkspace {
                source_output: OutputId("out-b".to_owned()),
                target_output: OutputId("out-a".to_owned()),
                workspace: crate::directional::WorkspaceId(view_a.clone()),
            }),
            true,
            false,
        ),
    );
    assert_eq!(scope_of(&back, "out-a", &view_a), ScopeAuthority::Untouched);
    assert!(
        back.transfers
            .iter()
            .any(|t| matches!(t.kind, TransferKind::Migration) && t.operation_id == "op-13")
    );
    let home = crate::session::DomainKey {
        output: OutputId("out-a".to_owned()),
        workspace: crate::directional::WorkspaceId(view_a.clone()),
    };
    assert!(
        !engine.world.layout().contains_domain(&home),
        "floating inverse stays sessionless"
    );
}

#[test]
fn queued_implicit_selection_resolves_at_execution_cursor() {
    // Output-targeting race: the intent shapes (output None) are queued
    // while the cursor sits over out-a; each execution resolves the
    // report-time cursor output over the retained views/history, never the
    // intent-time cursor.
    let numbered = WorldCommand::SelectNumbered {
        output: None,
        index: 2,
    };
    let previous = WorldCommand::SelectPrevious { output: None };
    let relative = WorldCommand::SelectRelative {
        output: None,
        delta: 1,
    };
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
    // Initial observation: cursor over out-a, focus on win-a.
    let mut first_report = report(
        1,
        1,
        two_screens(),
        vec![fact("win-a", "out-a", 0), fact("win-b", "out-b", 800)],
        vec![],
        vec![],
        "win-a",
        None,
        None,
        true,
        false,
    );
    first_report.cursor = Some((100, 300));
    let first = accepted(&mut engine, &first_report);
    let view_a = output_view(&first, "out-a").expect("view a");
    let view_b = output_view(&first, "out-b").expect("view b");
    // New observation before execution: the cursor moves to out-b while the
    // queued None shape is unchanged, so it steers out-b, not out-a.
    let mut over_b = report(
        2,
        2,
        two_screens(),
        vec![fact("win-a", "out-a", 0), fact("win-b", "out-b", 800)],
        vec![],
        vec![],
        "win-a",
        Some(11),
        Some(numbered),
        true,
        false,
    );
    over_b.cursor = Some((900, 300));
    let second = accepted(&mut engine, &over_b);
    let moved_b = output_view(&second, "out-b").expect("moved b");
    assert_ne!(moved_b, view_b, "execution cursor steers to out-b");
    assert_eq!(
        output_view(&second, "out-a").as_deref(),
        Some(view_a.as_str())
    );
    // Same cursor: previous replays the just-recorded retained history.
    let mut back = report(
        3,
        3,
        two_screens(),
        vec![fact("win-a", "out-a", 0), fact("win-b", "out-b", 800)],
        vec![],
        vec![],
        "win-a",
        Some(12),
        Some(previous),
        true,
        false,
    );
    back.cursor = Some((900, 300));
    let third = accepted(&mut engine, &back);
    assert_eq!(
        output_view(&third, "out-b").as_deref(),
        Some(view_b.as_str()),
        "previous replays execution-time history"
    );
    assert_eq!(
        output_view(&third, "out-a").as_deref(),
        Some(view_a.as_str())
    );
    // Cursor returns to out-a before the next queued intent executes: the
    // same None shape now steers out-a, leaving out-b alone.
    let mut over_a = report(
        4,
        4,
        two_screens(),
        vec![fact("win-a", "out-a", 0), fact("win-b", "out-b", 800)],
        vec![],
        vec![],
        "win-a",
        Some(13),
        Some(relative),
        true,
        false,
    );
    over_a.cursor = Some((100, 300));
    let fourth = accepted(&mut engine, &over_a);
    assert_ne!(
        output_view(&fourth, "out-a").as_deref(),
        Some(view_a.as_str()),
        "returned cursor steers the later execution to out-a"
    );
    assert_eq!(
        output_view(&fourth, "out-b").as_deref(),
        Some(view_b.as_str())
    );
}

#[test]
fn focused_mode_and_explicit_selection_survive_disagreeing_cursor() {
    // Output-targeting race across a focus change: both intent values are
    // queued while focus sits on win-a; focused-mode execution resolves the
    // changed focus output and the explicit output passes through, each
    // ignoring a disagreeing cursor.
    let implicit = WorldCommand::SelectNumbered {
        output: None,
        index: 2,
    };
    let explicit = WorldCommand::SelectNumbered {
        output: Some(OutputId("out-a".to_owned())),
        index: 2,
    };
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
    // Initial observation: cursor over out-a, focus on win-a.
    let mut first_report = report(
        1,
        1,
        two_screens(),
        vec![fact("win-a", "out-a", 0)],
        vec![],
        vec![],
        "win-a",
        None,
        None,
        true,
        false,
    );
    first_report.cursor = Some((100, 300));
    let first = accepted(&mut engine, &first_report);
    let view_a = output_view(&first, "out-a").expect("view a");
    // New observation: win-b arrives on out-b under the moved cursor while
    // focus stays on win-a.
    let mut arrive = report(
        2,
        2,
        two_screens(),
        vec![fact("win-a", "out-a", 0), fact("win-b", "out-b", 800)],
        vec![],
        vec![],
        "win-a",
        None,
        None,
        true,
        false,
    );
    arrive.cursor = Some((900, 300));
    let arrived = accepted(&mut engine, &arrive);
    let view_b = output_view(&arrived, "out-b").expect("view b");
    // Focus changes to win-b before any queued intent executes.
    let mut refocus = report(
        3,
        3,
        two_screens(),
        vec![fact("win-a", "out-a", 0), fact("win-b", "out-b", 800)],
        vec![],
        vec![],
        "win-b",
        None,
        None,
        true,
        false,
    );
    refocus.cursor = Some((100, 300));
    accepted(&mut engine, &refocus);
    // Focused mode: the cursor names out-a but the pre-queued None shape
    // resolves the changed focus output out-b, leaving out-a alone.
    let mut focused_leg = report(
        4,
        4,
        two_screens(),
        vec![fact("win-a", "out-a", 0), fact("win-b", "out-b", 800)],
        vec![],
        vec![],
        "win-b",
        Some(11),
        Some(implicit),
        true,
        false,
    );
    engine
        .apply_test_settings(
            1,
            vec![(
                "switchTarget".to_owned(),
                crate::connection::RawSettingValue::String("focused-output".to_owned()),
            )],
        )
        .expect("settings accepted");
    focused_leg.cursor = Some((100, 300));
    let fourth = accepted(&mut engine, &focused_leg);
    let moved_b = output_view(&fourth, "out-b").expect("moved b");
    assert_ne!(
        moved_b, view_b,
        "changed focus wins over the disagreeing cursor"
    );
    assert_eq!(
        output_view(&fourth, "out-a").as_deref(),
        Some(view_a.as_str())
    );
    // Explicit output survives every disagreement: cursor and focus both
    // name out-b, yet the explicit out-a select still moves out-a only.
    let mut explicit_leg = report(
        5,
        5,
        two_screens(),
        vec![fact("win-a", "out-a", 0), fact("win-b", "out-b", 800)],
        vec![],
        vec![],
        "win-b",
        Some(12),
        Some(explicit),
        true,
        false,
    );
    explicit_leg.cursor = Some((900, 300));
    let fifth = accepted(&mut engine, &explicit_leg);
    assert_ne!(
        output_view(&fifth, "out-a").as_deref(),
        Some(view_a.as_str()),
        "explicit output honored despite cursor/focus"
    );
    assert_eq!(
        output_view(&fifth, "out-b").as_deref(),
        Some(moved_b.as_str())
    );
}

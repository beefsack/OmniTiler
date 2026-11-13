//! Core-owned positive-close refocus: retained request happy paths.
//!
//! The frozen pre-close MRU tiled survivor of the focused explicit-positive
//! close cohort is recorded before host-fallback ingestion, re-emitted with a
//! stable revision-derived id until target equality satisfies it, and stays
//! truthful to later observations.

use crate::boundary::CoreCommand;
use crate::directional::{OutputId, WindowId, WorkspaceId};
use crate::geometry::Rect;
use crate::size_hints::WindowSizeHints;
use crate::world_engine::{WorldEngine, WorldOutcome};
use crate::world_report::{
    BatchSettlement, CommandResult, FocusFeedback, FocusRequestCause, PresentationIngressV4,
    PresentationRowV4, ScreenReport, VisibilityReason, VisibilitySetter, WORLD_CONTRACT_VERSION_V4,
    WindowFact, WorldCommand, WorldReportV4,
};
use crate::world_run_v4::V4RunOutcome;

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

#[allow(clippy::too_many_arguments)]
fn report(
    request_seq: u64,
    windows: Vec<WindowFact>,
    closed: Vec<&str>,
    focused: &str,
    foreground_known: bool,
    command_seq: Option<u64>,
    command: Option<WorldCommand>,
) -> WorldReportV4 {
    WorldReportV4 {
        contract_version: WORLD_CONTRACT_VERSION_V4,
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        observation_seq: request_seq,
        request_seq,
        request_id: format!("req-{request_seq}"),
        command_seq,
        command,
        screens: vec![screen("out-a", 0), screen("out-b", 800)],
        held_windows: Vec::new(),
        held_screens: Vec::new(),
        windows,
        held_native: Vec::new(),
        native: Vec::new(),
        closed: closed
            .into_iter()
            .map(|id| WindowId(id.to_owned()))
            .collect(),
        cursor: None,
        cursor_hit: None,
        focused_global: focused.to_owned(),
        foreground_known,
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
        presentation: PresentationRowV4::Valid(PresentationIngressV4::default()),
        intent_records: Vec::new(),
        renderer_input: None,
    }
}

fn accepted(engine: &mut WorldEngine, report: &WorldReportV4) -> crate::world_report::WorldReplyV4 {
    match engine.handle_report(report, &[]).0 {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    }
}

fn view_of(reply: &crate::world_report::WorldReplyV4, output: &str) -> String {
    reply
        .outputs
        .iter()
        .find(|row| row.output.0 == output)
        .and_then(|row| row.logical_view.clone())
        .expect("logical view")
}

fn committed_focus(engine: &WorldEngine, output: &str, workspace: &str) -> Option<String> {
    let key = crate::session::DomainKey {
        output: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
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
fn close_cohort_freezes_mru_despite_host_fallback() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
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
            "win-a",
            true,
            None,
            None,
        ),
    );
    accepted(
        &mut engine,
        &report(
            2,
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 100),
                fact("win-c", "out-a", 200),
            ],
            vec![],
            "win-c",
            true,
            None,
            None,
        ),
    );
    let view_a = view_of(
        &accepted(
            &mut engine,
            &report(
                3,
                vec![
                    fact("win-a", "out-a", 0),
                    fact("win-b", "out-a", 100),
                    fact("win-c", "out-a", 200),
                ],
                vec![],
                "win-b",
                true,
                None,
                None,
            ),
        ),
        "out-a",
    );
    // Host fallback already picked win-a in the close report; the frozen MRU
    // survivor excluding the closed win-b is still win-c.
    let closed = accepted(
        &mut engine,
        &report(
            4,
            vec![fact("win-a", "out-a", 0), fact("win-c", "out-a", 200)],
            vec!["win-b"],
            "win-a",
            true,
            None,
            None,
        ),
    );
    let request = closed.focus_request.as_ref().expect("retained request");
    assert_eq!(request.id, "focus-4");
    assert_eq!(request.target.0, "win-c");
    assert_eq!(request.issued_revision, 4);
    assert_eq!(request.cause, FocusRequestCause::PositiveClose);
    assert_eq!(request.output.0, "out-a");
    assert_eq!(request.workspace, view_a);
    // Truthful ingestion keeps the observed fallback committed; the frozen
    // target rides the retained lane, never the committed slot.
    assert_eq!(
        committed_focus(&engine, "out-a", &view_a).as_deref(),
        Some("win-a")
    );
}

#[test]
fn close_without_observed_focus_mints_known_request() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
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
            "win-a",
            true,
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
                "win-b",
                true,
                None,
                None,
            ),
        ),
        "out-a",
    );
    // Known no-client close report: the MRU survivor excluding win-b is
    // win-a, minted as the retained request.
    let closed = accepted(
        &mut engine,
        &report(
            3,
            vec![fact("win-a", "out-a", 0), fact("win-c", "out-a", 200)],
            vec!["win-b"],
            "",
            true,
            None,
            None,
        ),
    );
    let request = closed.focus_request.as_ref().expect("retained request");
    assert_eq!(request.id, "focus-3");
    assert_eq!(request.target.0, "win-a");
    assert_eq!(request.issued_revision, 3);
    assert_eq!(request.workspace, view_a);
    assert_eq!(
        committed_focus(&engine, "out-a", &view_a).as_deref(),
        Some("win-a")
    );
}

#[test]
fn burst_reemits_stable_id_until_target_equality() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
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
            "win-a",
            true,
            None,
            None,
        ),
    );
    accepted(
        &mut engine,
        &report(
            2,
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 100),
                fact("win-c", "out-a", 200),
            ],
            vec![],
            "win-b",
            true,
            None,
            None,
        ),
    );
    let closed = accepted(
        &mut engine,
        &report(
            3,
            vec![fact("win-a", "out-a", 0), fact("win-c", "out-a", 200)],
            vec!["win-b"],
            "",
            true,
            None,
            None,
        ),
    );
    let issued = closed.focus_request.clone().expect("retained request");
    assert_eq!(issued.id, "focus-3");
    assert_eq!(issued.target.0, "win-a");
    // Unchanged known foreground across the burst: the identical row is
    // re-emitted, never reminted, with no publication dependency.
    let burst = accepted(
        &mut engine,
        &report(
            4,
            vec![fact("win-a", "out-a", 0), fact("win-c", "out-a", 200)],
            vec![],
            "",
            true,
            None,
            None,
        ),
    );
    assert_eq!(burst.focus_request, Some(issued));
    // Observed focus equals the target: satisfied, the lane retires.
    let satisfied = accepted(
        &mut engine,
        &report(
            5,
            vec![fact("win-a", "out-a", 0), fact("win-c", "out-a", 200)],
            vec![],
            "win-a",
            true,
            None,
            None,
        ),
    );
    assert_eq!(satisfied.focus_request, None);
}

fn float_fact(window: &str, output: &str, x: i32) -> WindowFact {
    WindowFact {
        transient: Some(true),
        ..fact(window, output, x)
    }
}

struct FeedbackRow<'a> {
    id: &'a str,
    target: &'a str,
    revision: u64,
    setter: VisibilitySetter,
    reason: VisibilityReason,
    pre: Option<&'a str>,
    post: Option<&'a str>,
    proven: bool,
}

fn feedback_row(args: FeedbackRow<'_>) -> FocusFeedback {
    FocusFeedback {
        id: args.id.to_owned(),
        target: WindowId(args.target.to_owned()),
        revision: args.revision,
        setter: args.setter,
        reason: args.reason,
        pre: args.pre.map(|id| WindowId(id.to_owned())),
        post: args.post.map(|id| WindowId(id.to_owned())),
        foreground_proven: args.proven,
        arrival: Vec::new(),
        attempt_ms: 7,
    }
}

fn with_feedback(mut base: WorldReportV4, rows: Vec<FocusFeedback>) -> WorldReportV4 {
    base.focus_feedback = rows;
    base
}

/// Focus `order` in sequence on out-a over `windows`, then return the reply
/// that focused the last entry plus its logical view.
fn focus_sequence(
    engine: &mut WorldEngine,
    seq_start: u64,
    windows: &[WindowFact],
    order: &[&str],
) -> crate::world_report::WorldReplyV4 {
    let mut reply = None;
    for (index, focused) in order.iter().enumerate() {
        let seq = seq_start + index as u64;
        reply = Some(accepted(
            engine,
            &report(seq, windows.to_vec(), vec![], focused, true, None, None),
        ));
    }
    reply.expect("nonempty focus order")
}

#[test]
fn host_matching_frozen_winner_mints_no_request() {
    // Focus order A,C,B makes the frozen survivor win-c; the host fallback
    // already chose win-c, so the close satisfies with no request.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let windows = vec![
        fact("win-a", "out-a", 0),
        fact("win-b", "out-a", 100),
        fact("win-c", "out-a", 200),
    ];
    focus_sequence(&mut engine, 1, &windows, &["win-a", "win-c", "win-b"]);
    let closed = accepted(
        &mut engine,
        &report(
            4,
            vec![fact("win-a", "out-a", 0), fact("win-c", "out-a", 200)],
            vec!["win-b"],
            "win-c",
            true,
            None,
            None,
        ),
    );
    assert_eq!(closed.focus_request, None);
}

#[test]
fn close_on_second_output_ignores_first_output_local_focus() {
    // out-a sorts first and keeps its own local focus on win-a, but the
    // prior globally observed focus is win-o on out-b. The focused close
    // of win-o must mint the out-b MRU survivor win-n despite the
    // other-output fallback candidate winning local order.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let windows = vec![
        fact("win-a", "out-a", 0),
        fact("win-m", "out-b", 800),
        fact("win-n", "out-b", 900),
        fact("win-o", "out-b", 1000),
    ];
    focus_sequence(
        &mut engine,
        1,
        &windows,
        &["win-a", "win-m", "win-n", "win-o"],
    );
    let closed = accepted(
        &mut engine,
        &report(
            5,
            vec![
                fact("win-a", "out-a", 0),
                fact("win-m", "out-b", 800),
                fact("win-n", "out-b", 900),
            ],
            vec!["win-o"],
            "win-m",
            true,
            None,
            None,
        ),
    );
    let request = closed.focus_request.as_ref().expect("retained request");
    assert_eq!(request.id, "focus-5");
    assert_eq!(request.target.0, "win-n");
    assert_eq!(request.output.0, "out-b");
}

#[test]
fn later_focus_change_supersedes_without_resurrection() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let windows = vec![
        fact("win-a", "out-a", 0),
        fact("win-b", "out-a", 100),
        fact("win-d", "out-a", 300),
    ];
    focus_sequence(&mut engine, 1, &windows, &["win-a", "win-b"]);
    let survivors = vec![fact("win-a", "out-a", 0), fact("win-d", "out-a", 300)];
    let minted = accepted(
        &mut engine,
        &report(3, survivors.clone(), vec!["win-b"], "", true, None, None),
    );
    let issued = minted.focus_request.clone().expect("retained request");
    assert_eq!(issued.target.0, "win-a");
    // The first publication is skipped adapter-side; the identical row still
    // re-emits on the next unchanged ingress report.
    let skipped = accepted(
        &mut engine,
        &report(4, survivors.clone(), vec![], "", true, None, None),
    );
    assert_eq!(skipped.focus_request, Some(issued));
    // A later observed change to win-d supersedes before any native apply.
    let superseded = accepted(
        &mut engine,
        &report(5, survivors.clone(), vec![], "win-d", true, None, None),
    );
    assert_eq!(superseded.focus_request, None);
    // Later equality with the retired target cannot resurrect the request:
    // the prior global is win-d, and no new close cohort exists.
    let no_resurrection = accepted(
        &mut engine,
        &report(6, survivors, vec![], "win-a", true, None, None),
    );
    assert_eq!(no_resurrection.focus_request, None);
}

#[test]
fn unknown_close_mints_nothing_and_unknown_retires() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let windows = vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)];
    focus_sequence(&mut engine, 1, &windows, &["win-a", "win-b"]);
    // Unknown foreground in the close report never originates a request.
    let unknown_close = accepted(
        &mut engine,
        &report(
            3,
            vec![fact("win-a", "out-a", 0)],
            vec!["win-b"],
            "",
            false,
            None,
            None,
        ),
    );
    assert_eq!(unknown_close.focus_request, None);
}

#[test]
fn unknown_later_report_retires_known_request() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let windows = vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)];
    focus_sequence(&mut engine, 1, &windows, &["win-a", "win-b"]);
    let survivors = vec![fact("win-a", "out-a", 0)];
    let minted = accepted(
        &mut engine,
        &report(3, survivors.clone(), vec!["win-b"], "", true, None, None),
    );
    assert!(minted.focus_request.is_some());
    let retired = accepted(
        &mut engine,
        &report(4, survivors, vec![], "", false, None, None),
    );
    assert_eq!(retired.focus_request, None);
}

#[test]
fn inline_command_supersedes_pending_request() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let windows = vec![
        fact("win-a", "out-a", 0),
        fact("win-b", "out-a", 100),
        fact("win-c", "out-a", 200),
    ];
    focus_sequence(&mut engine, 1, &windows, &["win-a", "win-b"]);
    let survivors = vec![fact("win-a", "out-a", 0), fact("win-c", "out-a", 200)];
    let minted = accepted(
        &mut engine,
        &report(3, survivors.clone(), vec!["win-b"], "", true, None, None),
    );
    assert!(minted.focus_request.is_some());
    // An explicit focus command is a newer focus decision of another cause:
    // it supersedes the close row with a new explicit-command request.
    let superseded = accepted(
        &mut engine,
        &report(
            4,
            survivors,
            vec![],
            "",
            true,
            Some(4),
            Some(WorldCommand::Focus {
                window: WindowId("win-c".to_owned()),
                direction: "left".to_owned(),
            }),
        ),
    );
    let request = superseded.focus_request.as_ref().expect("new request");
    assert_eq!(request.id, "focus-4");
    assert_eq!(request.target.0, "win-c");
    assert_eq!(request.cause, FocusRequestCause::ExplicitCommand);
    assert_eq!(request.cmd_seq, Some(4));
}

#[test]
fn consumed_sequence_report_supersedes_pending_request() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let windows = vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)];
    // Consume sequence 1 up front so a later repeat arrives consumed.
    accepted(
        &mut engine,
        &report(
            1,
            windows.clone(),
            vec![],
            "win-a",
            true,
            Some(1),
            Some(WorldCommand::Focus {
                window: WindowId("win-a".to_owned()),
                direction: "left".to_owned(),
            }),
        ),
    );
    focus_sequence(&mut engine, 2, &windows, &["win-a", "win-b"]);
    let survivors = vec![fact("win-a", "out-a", 0)];
    let minted = accepted(
        &mut engine,
        &report(4, survivors.clone(), vec!["win-b"], "", true, None, None),
    );
    assert!(minted.focus_request.is_some());
    // Repeating the consumed sequence reconciles only: no new focus
    // decision executes, so the retained request survives unchanged (Q1).
    // A consumed repeat publishes through the CommandConsumed outcome with
    // the same reply shape.
    let issued = minted.focus_request.clone().expect("retained request");
    let superseded = match engine
        .handle_report(
            &report(
                5,
                survivors,
                vec![],
                "",
                true,
                Some(1),
                Some(WorldCommand::Focus {
                    window: WindowId("win-a".to_owned()),
                    direction: "left".to_owned(),
                }),
            ),
            &[],
        )
        .0
    {
        WorldOutcome::Accepted(reply) | WorldOutcome::CommandConsumed(reply) => reply,
        other => panic!("expected accepted or consumed, got {other:?}"),
    };
    assert_eq!(superseded.focus_request, Some(issued));
}

#[test]
fn adopted_view_change_supersedes_pending_request() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let windows = vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)];
    focus_sequence(&mut engine, 1, &windows, &["win-a", "win-b"]);
    let survivors = vec![fact("win-a", "out-a", 0)];
    let minted = accepted(
        &mut engine,
        &report(3, survivors.clone(), vec!["win-b"], "", true, None, None),
    );
    assert!(minted.focus_request.is_some());
    // A newly adopted core view (as a view transfer would confirm) retires
    // the frozen source-domain request even with unchanged focus. The new
    // view joins the mapping ring so lifecycle view sanity keeps it.
    let mut ring = engine
        .mapping
        .get("out-a")
        .cloned()
        .unwrap_or_else(|| vec![view_of(&minted, "out-a")]);
    if !ring.contains(&"ws-moved".to_owned()) {
        ring.push("ws-moved".to_owned());
    }
    engine.mapping.insert("out-a".to_owned(), ring);
    engine
        .views
        .insert("out-a".to_owned(), "ws-moved".to_owned());
    let superseded = accepted(
        &mut engine,
        &report(4, survivors, vec![], "", true, None, None),
    );
    assert_eq!(superseded.focus_request, None);
}

#[test]
fn close_on_noncurrent_workspace_mints_no_request() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let windows = vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)];
    focus_sequence(&mut engine, 1, &windows, &["win-a", "win-b"]);
    // The source workspace is no longer current: background reflow only.
    engine
        .views
        .insert("out-a".to_owned(), "ws-elsewhere".to_owned());
    let closed = accepted(
        &mut engine,
        &report(
            3,
            vec![fact("win-a", "out-a", 0)],
            vec!["win-b"],
            "",
            true,
            None,
            None,
        ),
    );
    assert_eq!(closed.focus_request, None);
}

#[test]
fn minimized_target_retires_pending_request() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let windows = vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)];
    let view_a = view_of(
        &focus_sequence(&mut engine, 1, &windows, &["win-a", "win-b"]),
        "out-a",
    );
    let survivors = vec![fact("win-a", "out-a", 0)];
    let minted = accepted(
        &mut engine,
        &report(3, survivors, vec!["win-b"], "", true, None, None),
    );
    assert!(minted.focus_request.is_some());
    let mut minimized = fact("win-a", "out-a", 0);
    minimized.minimized = Some(true);
    minimized.rect = None;
    let retired = accepted(
        &mut engine,
        &report(4, vec![minimized], vec![], "", true, None, None),
    );
    assert_eq!(retired.focus_request, None);
    assert_eq!(view_of(&retired, "out-a"), view_a);
}

#[test]
fn hidden_target_retires_pending_request() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let windows = vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)];
    focus_sequence(&mut engine, 1, &windows, &["win-a", "win-b"]);
    let survivors = vec![fact("win-a", "out-a", 0)];
    let minted = accepted(
        &mut engine,
        &report(3, survivors, vec!["win-b"], "", true, None, None),
    );
    assert!(minted.focus_request.is_some());
    let mut hidden = fact("win-a", "out-a", 0);
    hidden.host_hidden = Some(true);
    let retired = accepted(
        &mut engine,
        &report(4, vec![hidden], vec![], "", true, None, None),
    );
    assert_eq!(retired.focus_request, None);
}

#[test]
fn unreadable_source_scope_retires_pending_request() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let windows = vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)];
    focus_sequence(&mut engine, 1, &windows, &["win-a", "win-b"]);
    let survivors = vec![fact("win-a", "out-a", 0)];
    let minted = accepted(
        &mut engine,
        &report(3, survivors.clone(), vec!["win-b"], "", true, None, None),
    );
    assert!(minted.focus_request.is_some());
    // An incomplete source screen holds the scope: the frozen winner is
    // fenced and the request retires without any alternate search.
    let mut held_report = report(4, survivors, vec![], "", true, None, None);
    held_report.screens = vec![
        ScreenReport {
            complete: false,
            ..screen("out-a", 0)
        },
        screen("out-b", 800),
    ];
    let retired = accepted(&mut engine, &held_report);
    assert_eq!(retired.focus_request, None);
}

#[test]
fn fullscreen_sibling_vetoes_pending_request_without_alternate() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let windows = vec![
        fact("win-a", "out-a", 0),
        fact("win-b", "out-a", 100),
        fact("win-c", "out-a", 200),
    ];
    focus_sequence(&mut engine, 1, &windows, &["win-a", "win-b"]);
    // The frozen MRU survivor is win-c; the host fallback picks win-a.
    let mut guarded = fact("win-c", "out-a", 200);
    guarded.fullscreen = Some(true);
    let closed = accepted(
        &mut engine,
        &report(
            3,
            vec![fact("win-a", "out-a", 0), guarded],
            vec!["win-b"],
            "win-a",
            true,
            None,
            None,
        ),
    );
    // The unsafe fullscreen winner authorizes no request and no alternate
    // search: win-a is eligible but never substituted.
    assert_eq!(closed.focus_request, None);
}

#[test]
fn focused_fullscreen_game_change_retires_pending_request() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let windows = vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)];
    focus_sequence(&mut engine, 1, &windows, &["win-a", "win-b"]);
    let survivors = vec![fact("win-a", "out-a", 0)];
    let minted = accepted(
        &mut engine,
        &report(3, survivors.clone(), vec!["win-b"], "", true, None, None),
    );
    assert!(minted.focus_request.is_some());
    // Focus moves to a fullscreen game: the later change supersedes under
    // the game pause, and the request retires.
    let mut game = fact("win-g", "out-a", 400);
    game.fullscreen = Some(true);
    game.privileged = Some(true);
    let mut gamed = survivors.clone();
    gamed.push(game);
    let retired = accepted(
        &mut engine,
        &report(4, gamed, vec![], "win-g", true, None, None),
    );
    assert_eq!(retired.focus_request, None);
}

#[test]
fn incomplete_inventory_close_mints_no_request() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let windows = vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)];
    focus_sequence(&mut engine, 1, &windows, &["win-a", "win-b"]);
    let mut gapped = report(
        3,
        vec![fact("win-a", "out-a", 0)],
        vec!["win-b"],
        "",
        true,
        None,
        None,
    );
    gapped.inventory_complete = false;
    assert_eq!(accepted(&mut engine, &gapped).focus_request, None);
}

#[test]
fn bare_absence_and_unfocused_close_mint_no_request() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let windows = vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)];
    focus_sequence(&mut engine, 1, &windows, &["win-a", "win-b"]);
    // Bare absence without an explicit close cohort never qualifies.
    let absent = accepted(
        &mut engine,
        &report(
            3,
            vec![fact("win-a", "out-a", 0)],
            vec![],
            "win-a",
            true,
            None,
            None,
        ),
    );
    assert_eq!(absent.focus_request, None);
    // Closing an unfocused lifetime while the global focus rests elsewhere
    // never qualifies either.
    let unfocused = accepted(
        &mut engine,
        &report(
            4,
            vec![fact("win-a", "out-a", 0)],
            vec!["win-b"],
            "win-a",
            true,
            None,
            None,
        ),
    );
    assert_eq!(unfocused.focus_request, None);
}

#[test]
fn duplicate_close_never_remints() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let windows = vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)];
    focus_sequence(&mut engine, 1, &windows, &["win-a", "win-b"]);
    let survivors = vec![fact("win-a", "out-a", 0)];
    let minted = accepted(
        &mut engine,
        &report(3, survivors.clone(), vec!["win-b"], "", true, None, None),
    );
    let issued = minted.focus_request.clone().expect("retained request");
    // The duplicate close report carries no new evidence: the identical
    // row persists with no id renewal.
    let dup = accepted(
        &mut engine,
        &report(4, survivors, vec!["win-b"], "", true, None, None),
    );
    assert_eq!(dup.focus_request, Some(issued));
}

#[test]
fn focused_ordinary_float_close_selects_tiled_mru_unchanged() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let tiles = vec![fact("win-a", "out-a", 0), fact("win-c", "out-a", 200)];
    let mut admitted = tiles.clone();
    admitted.push(float_fact("win-f", "out-a", 300));
    let before = focus_sequence(&mut engine, 1, &admitted, &["win-a", "win-f"]);
    let frame_before = |reply: &crate::world_report::WorldReplyV4, win: &str| {
        reply
            .windows
            .iter()
            .find(|row| row.lifetime.0 == win)
            .unwrap_or_else(|| panic!("missing row {win}"))
            .allocation
    };
    let closed = accepted(
        &mut engine,
        &report(3, tiles, vec!["win-f"], "", true, None, None),
    );
    // The ordinary float is not tiled-linked, yet its retained source home
    // still yields the tiled MRU survivor; no float candidate expansion.
    let request = closed.focus_request.as_ref().expect("retained request");
    assert_eq!(request.target.0, "win-a");
    assert_eq!(request.output.0, "out-a");
    for win in ["win-a", "win-c"] {
        assert_eq!(
            frame_before(&closed, win),
            frame_before(&before, win),
            "{win} tile frame unchanged"
        );
    }
}

#[test]
fn ineligible_same_domain_float_fallback_mints_no_request() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let mut admitted = vec![fact("win-a", "out-a", 0), fact("win-c", "out-a", 200)];
    admitted.push(float_fact("win-f", "out-a", 300));
    focus_sequence(&mut engine, 1, &admitted, &["win-a", "win-c"]);
    // The host fallback lands on the same-domain float: ineligible, so no
    // bounded correction is authorized.
    let closed = accepted(
        &mut engine,
        &report(
            3,
            vec![fact("win-a", "out-a", 0), float_fact("win-f", "out-a", 300)],
            vec!["win-c"],
            "win-f",
            true,
            None,
            None,
        ),
    );
    assert_eq!(closed.focus_request, None);
}

#[test]
fn other_output_fallback_mints_no_request() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let windows = vec![
        fact("win-a", "out-a", 0),
        fact("win-c", "out-a", 200),
        fact("win-x", "out-b", 800),
    ];
    focus_sequence(&mut engine, 1, &windows, &["win-a", "win-c"]);
    // The fallback names another output's window: foreign to the source
    // domain, so no correction is authorized.
    let closed = accepted(
        &mut engine,
        &report(
            3,
            vec![fact("win-a", "out-a", 0), fact("win-x", "out-b", 800)],
            vec!["win-c"],
            "win-x",
            true,
            None,
            None,
        ),
    );
    assert_eq!(closed.focus_request, None);
}

#[test]
fn sole_close_keeps_empty_workspace_with_no_request() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let view_a = view_of(
        &accepted(
            &mut engine,
            &report(
                1,
                vec![fact("win-a", "out-a", 0)],
                vec![],
                "win-a",
                true,
                None,
                None,
            ),
        ),
        "out-a",
    );
    // Sole close on a single shown workspace: no member request, the shown
    // empty workspace and its focused-output intent stay put.
    let closed = accepted(
        &mut engine,
        &report(2, Vec::new(), vec!["win-a"], "", true, None, None),
    );
    assert_eq!(closed.focus_request, None);
    assert_eq!(view_of(&closed, "out-a"), view_a);
    assert!(
        closed.windows.iter().all(|row| row.output.0 != "out-a"),
        "no out-a member rows survive the sole close"
    );
    assert_eq!(committed_focus(&engine, "out-a", &view_a), None);
}

#[test]
fn multi_output_sole_close_preserves_output() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let view_a = view_of(
        &accepted(
            &mut engine,
            &report(
                1,
                vec![fact("win-a", "out-a", 0), fact("win-x", "out-b", 800)],
                vec![],
                "win-a",
                true,
                None,
                None,
            ),
        ),
        "out-a",
    );
    // Sole close on out-a while out-b keeps its window: no member request,
    // no cross-output focus, and out-a stays shown and empty.
    let closed = accepted(
        &mut engine,
        &report(
            2,
            vec![fact("win-x", "out-b", 800)],
            vec!["win-a"],
            "",
            true,
            None,
            None,
        ),
    );
    assert_eq!(closed.focus_request, None);
    assert_eq!(view_of(&closed, "out-a"), view_a);
    assert!(
        closed.windows.iter().all(|row| row.output.0 != "out-a"),
        "no out-a member rows survive the sole close"
    );
    assert_eq!(committed_focus(&engine, "out-a", &view_a), None);
}

#[test]
fn mismatched_feedback_tuples_are_ignored() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let windows = vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)];
    focus_sequence(&mut engine, 1, &windows, &["win-a", "win-b"]);
    let survivors = vec![fact("win-a", "out-a", 0)];
    let minted = accepted(
        &mut engine,
        &report(3, survivors.clone(), vec!["win-b"], "", true, None, None),
    );
    let issued = minted.focus_request.clone().expect("retained request");
    let refused_shape = |id: &str, target: &str, revision: u64| {
        feedback_row(FeedbackRow {
            id,
            target,
            revision,
            setter: VisibilitySetter::Refused,
            reason: VisibilityReason::NativeRefused,
            pre: Some("win-x"),
            post: Some("win-x"),
            proven: false,
        })
    };
    // Wrong id, wrong target, and wrong revision rows all miss the exact
    // opaque tuple and change nothing.
    for (seq, id, target, revision) in [
        (4u64, "focus-999", "win-a", 3),
        (5u64, issued.id.as_str(), "win-b", 3),
        (6u64, issued.id.as_str(), "win-a", 2),
    ] {
        let kept = accepted(
            &mut engine,
            &with_feedback(
                report(seq, survivors.clone(), vec![], "", true, None, None),
                vec![refused_shape(id, target, revision)],
            ),
        );
        assert_eq!(
            kept.focus_request,
            Some(issued.clone()),
            "{id}/{target}/{revision}"
        );
    }
}

#[test]
fn correlated_refusal_retires_pending_request() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let windows = vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)];
    focus_sequence(&mut engine, 1, &windows, &["win-a", "win-b"]);
    let survivors = vec![fact("win-a", "out-a", 0)];
    let minted = accepted(
        &mut engine,
        &report(3, survivors.clone(), vec!["win-b"], "", true, None, None),
    );
    let issued = minted.focus_request.clone().expect("retained request");
    // Setter refused with the same unchanged non-target pre/post and no
    // proof: terminal refusal under the ordinary-focus rule.
    let retired = accepted(
        &mut engine,
        &with_feedback(
            report(4, survivors, vec![], "", true, None, None),
            vec![feedback_row(FeedbackRow {
                id: &issued.id,
                target: "win-a",
                revision: issued.issued_revision,
                setter: VisibilitySetter::Refused,
                reason: VisibilityReason::NativeRefused,
                pre: Some("win-x"),
                post: Some("win-x"),
                proven: false,
            })],
        ),
    );
    assert_eq!(retired.focus_request, None);
}

#[test]
fn unproven_first_row_latches_against_later_refusal() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let windows = vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)];
    focus_sequence(&mut engine, 1, &windows, &["win-a", "win-b"]);
    let survivors = vec![fact("win-a", "out-a", 0)];
    let minted = accepted(
        &mut engine,
        &report(3, survivors.clone(), vec!["win-b"], "", true, None, None),
    );
    let issued = minted.focus_request.clone().expect("retained request");
    // Accepted without equality is unproven: the row latches and the
    // request retains with its original stamp.
    let unproven = feedback_row(FeedbackRow {
        id: &issued.id,
        target: "win-a",
        revision: issued.issued_revision,
        setter: VisibilitySetter::Accepted,
        reason: VisibilityReason::Empty,
        pre: Some("win-x"),
        post: Some("win-x"),
        proven: false,
    });
    let mut unproven_late = unproven.clone();
    unproven_late.attempt_ms = 99;
    let kept = accepted(
        &mut engine,
        &with_feedback(
            report(4, survivors.clone(), vec![], "", true, None, None),
            vec![unproven],
        ),
    );
    assert_eq!(kept.focus_request, Some(issued.clone()));
    // A later refusal for the same tuple cannot replace the latched first
    // row, so the request still retains.
    let refused = feedback_row(FeedbackRow {
        id: &issued.id,
        target: "win-a",
        revision: issued.issued_revision,
        setter: VisibilitySetter::Refused,
        reason: VisibilityReason::NativeRefused,
        pre: Some("win-x"),
        post: Some("win-x"),
        proven: false,
    });
    let still_kept = accepted(
        &mut engine,
        &with_feedback(
            report(5, survivors, vec![], "", true, None, None),
            vec![refused, unproven_late],
        ),
    );
    assert_eq!(still_kept.focus_request, Some(issued));
}

#[test]
fn safety_no_call_retires_pending_request() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let windows = vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)];
    focus_sequence(&mut engine, 1, &windows, &["win-a", "win-b"]);
    let survivors = vec![fact("win-a", "out-a", 0)];
    let minted = accepted(
        &mut engine,
        &report(3, survivors.clone(), vec!["win-b"], "", true, None, None),
    );
    let issued = minted.focus_request.clone().expect("retained request");
    // A terminal safety no-call (native fullscreen veto, spent id) retires;
    // it never becomes a retry.
    let retired = accepted(
        &mut engine,
        &with_feedback(
            report(4, survivors, vec![], "", true, None, None),
            vec![feedback_row(FeedbackRow {
                id: &issued.id,
                target: "win-a",
                revision: issued.issued_revision,
                setter: VisibilitySetter::NotCalled,
                reason: VisibilityReason::Fullscreen,
                pre: None,
                post: None,
                proven: false,
            })],
        ),
    );
    assert_eq!(retired.focus_request, None);
}

#[test]
fn same_report_admission_beats_close() {
    // Q2: a positive close choosing tiled MRU win-c and a fresh ordinary
    // newcomer win-n in the same report retain one winner only: N wins
    // under explicit-command > automatic-admission > positive-close >
    // focus-follows-cursor. Both producers are genuine: the close cohort
    // proves the MRU survivor while the newcomer diff proves admission.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let windows = vec![
        fact("win-a", "out-a", 0),
        fact("win-b", "out-a", 100),
        fact("win-c", "out-a", 200),
    ];
    focus_sequence(&mut engine, 1, &windows, &["win-a", "win-c", "win-b"]);
    let reply = accepted(
        &mut engine,
        &report(
            4,
            vec![
                fact("win-a", "out-a", 0),
                fact("win-c", "out-a", 200),
                fact("win-n", "out-a", 300),
            ],
            vec!["win-b"],
            "win-a",
            true,
            None,
            None,
        ),
    );
    assert!(
        reply.effects.is_empty(),
        "focus never publishes as wire effects: {reply:?}"
    );
    let request = reply.focus_request.as_ref().expect("admission winner");
    assert_eq!(request.target.0, "win-n", "newcomer beats close: {reply:?}");
    assert_eq!(request.cause, FocusRequestCause::AutomaticAdmission);
    assert_eq!(request.id, "focus-4");
}

#[test]
fn same_report_explicit_beats_admission_and_close() {
    // Q2: an explicit command focus for win-a shares its report with a
    // positive close (survivor win-c) and a fresh newcomer win-n. The
    // explicit winner supersedes both automatic candidates with one
    // retained row.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let windows = vec![
        fact("win-a", "out-a", 0),
        fact("win-b", "out-a", 100),
        fact("win-c", "out-a", 200),
    ];
    focus_sequence(&mut engine, 1, &windows, &["win-a", "win-c", "win-b"]);
    let reply = accepted(
        &mut engine,
        &report(
            4,
            vec![
                fact("win-a", "out-a", 0),
                fact("win-c", "out-a", 200),
                fact("win-n", "out-a", 300),
            ],
            vec!["win-b"],
            "win-a",
            true,
            Some(4),
            Some(WorldCommand::Focus {
                window: WindowId("win-a".to_owned()),
                direction: "left".to_owned(),
            }),
        ),
    );
    assert!(
        reply.effects.is_empty(),
        "focus never publishes as wire effects: {reply:?}"
    );
    let request = reply.focus_request.as_ref().expect("explicit winner");
    assert_eq!(request.target.0, "win-a", "explicit beats all: {reply:?}");
    assert_eq!(request.cause, FocusRequestCause::ExplicitCommand);
    assert_eq!(request.cmd_seq, Some(4));
}

fn hover_engine() -> WorldEngine {
    WorldEngine::new_test_bound_with_initial_settings(
        "gen-1",
        "adapter-1",
        crate::connection::HostCapabilities {
            native_sticky: true,
            maximize: true,
            ..Default::default()
        },
        vec![(
            "focusFollowsCursor".to_owned(),
            crate::connection::RawSettingValue::Bool(true),
        )],
    )
    .expect("valid generation")
}

fn hover_report(
    seq: u64,
    windows: Vec<WindowFact>,
    focused: &str,
    cursor: Option<(i32, i32)>,
    cursor_hit: Option<&str>,
) -> WorldReportV4 {
    let mut base = report(seq, windows, vec![], focused, true, None, None);
    base.cursor = cursor;
    base.cursor_hit = cursor_hit.map(|hit| WindowId(hit.to_owned()));
    base
}

#[test]
fn hover_movement_over_pending_keeps_id() {
    // Q3: pointer movement over the already-pending hover target keeps the
    // id and never retries. Genuine hover producer: cursor moves but the
    // hit stays win-b.
    let mut engine = hover_engine();
    let windows = vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)];
    accepted(
        &mut engine,
        &hover_report(1, windows.clone(), "win-a", Some((10, 10)), None),
    );
    let first = accepted(
        &mut engine,
        &hover_report(2, windows.clone(), "win-a", Some((110, 10)), Some("win-b")),
    );
    let issued = first.focus_request.clone().expect("hover request");
    assert_eq!(issued.target.0, "win-b");
    assert_eq!(issued.cause, FocusRequestCause::FocusFollowsCursor);
    // Cursor moves again but stays over pending win-b: no new id, no retry.
    let second = accepted(
        &mut engine,
        &hover_report(3, windows.clone(), "win-a", Some((120, 12)), Some("win-b")),
    );
    assert!(
        second.effects.is_empty(),
        "hover never publishes as wire effects: {second:?}"
    );
    assert_eq!(
        second.focus_request,
        Some(issued.clone()),
        "movement over pending keeps the identical row"
    );
    // An unchanged report keeps the id as well: identical cursor state on
    // a fresh sequence re-emits the identical row.
    let fourth = accepted(
        &mut engine,
        &hover_report(4, windows, "win-a", Some((120, 12)), Some("win-b")),
    );
    assert_eq!(
        fourth.focus_request,
        Some(issued),
        "unchanged hover keeps the identical row"
    );
}

#[test]
fn hover_leave_and_return_mints_new_id() {
    // Q3: each fresh qualifying hover entry after leaving the live target
    // and returning mints a new id. The stale tuple for the first entry
    // can never settle the fresh grant.
    let mut engine = hover_engine();
    let windows = vec![
        fact("win-a", "out-a", 0),
        fact("win-b", "out-a", 100),
        fact("win-c", "out-a", 200),
    ];
    accepted(
        &mut engine,
        &hover_report(1, windows.clone(), "win-a", Some((10, 10)), None),
    );
    let first = accepted(
        &mut engine,
        &hover_report(2, windows.clone(), "win-a", Some((110, 10)), Some("win-b")),
    );
    let old = first.focus_request.clone().expect("hover win-b");
    assert_eq!(old.target.0, "win-b");
    let middle = accepted(
        &mut engine,
        &hover_report(3, windows.clone(), "win-a", Some((210, 10)), Some("win-c")),
    );
    let between = middle.focus_request.clone().expect("hover win-c");
    assert_eq!(between.target.0, "win-c");
    assert_ne!(between.id, old.id, "leave mints a new id");
    let back = accepted(
        &mut engine,
        &hover_report(4, windows.clone(), "win-a", Some((110, 10)), Some("win-b")),
    );
    let fresh = back.focus_request.clone().expect("hover win-b again");
    assert_eq!(fresh.target.0, "win-b");
    assert_ne!(fresh.id, old.id, "reentry mints a new id");
    assert_ne!(fresh.id, between.id);
    // Stale exact-tuple feedback for the first entry cannot latch the
    // fresh grant: the opaque id differs.
    let stale = FocusFeedback {
        id: old.id.clone(),
        target: WindowId("win-b".to_owned()),
        revision: old.issued_revision,
        setter: VisibilitySetter::Accepted,
        reason: VisibilityReason::Empty,
        pre: Some(WindowId("win-a".to_owned())),
        post: Some(WindowId("win-b".to_owned())),
        foreground_proven: true,
        arrival: Vec::new(),
        attempt_ms: 9,
    };
    let settled = accepted(
        &mut engine,
        &with_feedback(
            hover_report(5, windows, "win-a", Some((110, 10)), Some("win-b")),
            vec![stale],
        ),
    );
    assert_eq!(
        settled.focus_request,
        Some(fresh),
        "stale tuple never settles the fresh hover"
    );
}

#[test]
fn hover_leave_to_empty_and_return_mints_new_id() {
    // Q3 correction: leaving pending win-b onto an empty/ineligible pointer
    // position and returning is a fresh qualifying entry even though no
    // intervening request minted while away. Motion within pending win-b
    // (covered above) keeps the id; departure clears it.
    let mut engine = hover_engine();
    let windows = vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)];
    accepted(
        &mut engine,
        &hover_report(1, windows.clone(), "win-a", Some((10, 10)), None),
    );
    let first = accepted(
        &mut engine,
        &hover_report(2, windows.clone(), "win-a", Some((110, 10)), Some("win-b")),
    );
    let old = first.focus_request.clone().expect("hover win-b");
    assert_eq!(old.target.0, "win-b");
    // Pointer leaves to empty: no candidate, no new request, pending
    // survives unchanged.
    let away = accepted(
        &mut engine,
        &hover_report(3, windows.clone(), "win-a", Some((400, 400)), None),
    );
    assert_eq!(
        away.focus_request,
        Some(old.clone()),
        "pending survives the empty departure"
    );
    // Return to win-b with genuine movement: fresh entry, new id.
    let back = accepted(
        &mut engine,
        &hover_report(4, windows.clone(), "win-a", Some((110, 10)), Some("win-b")),
    );
    let fresh = back.focus_request.clone().expect("hover win-b again");
    assert_eq!(fresh.target.0, "win-b");
    assert_eq!(fresh.cause, FocusRequestCause::FocusFollowsCursor);
    assert_ne!(fresh.id, old.id, "empty departure plus return remints");
    assert_ne!(fresh.issued_revision, old.issued_revision);
}

#[test]
fn hover_fullscreen_hit_mints_nothing() {
    // Normative automatic restrictions: hover never targets a fullscreen
    // victim. Genuine producer with a fullscreen hit mints no request.
    let mut engine = hover_engine();
    let windows = vec![
        fact("win-a", "out-a", 0),
        WindowFact {
            fullscreen: Some(true),
            ..fact("win-b", "out-a", 100)
        },
    ];
    accepted(
        &mut engine,
        &hover_report(1, windows.clone(), "win-a", Some((10, 10)), None),
    );
    let reply = accepted(
        &mut engine,
        &hover_report(2, windows, "win-a", Some((110, 10)), Some("win-b")),
    );
    assert_eq!(
        reply.focus_request, None,
        "fullscreen hover mints nothing: {reply:?}"
    );
}

#[test]
fn same_report_close_beats_hover_cursor_follows_winner() {
    // Close/hover execution-cursor discriminator: closing focused win-b
    // selects tiled MRU survivor win-c while the pointer genuinely enters
    // hover target win-d in the same report. Q2 keeps the close winner C
    // over hover D. The issuing reply is skipped adapter-side; a trailing
    // implicit non-focus command must resolve its subject from the
    // published winner C, never the hover loser D.
    let mut engine = hover_engine();
    let windows = vec![
        fact("win-a", "out-a", 0),
        fact("win-b", "out-a", 100),
        fact("win-c", "out-a", 200),
        fact("win-d", "out-a", 300),
    ];
    focus_sequence(
        &mut engine,
        1,
        &windows,
        &["win-a", "win-d", "win-c", "win-b"],
    );
    let survivors = vec![
        fact("win-a", "out-a", 0),
        fact("win-c", "out-a", 200),
        fact("win-d", "out-a", 300),
    ];
    let mut close = report(
        5,
        survivors.clone(),
        vec!["win-b"],
        "win-a",
        true,
        None,
        None,
    );
    close.cursor = Some((310, 10));
    close.cursor_hit = Some(WindowId("win-d".to_owned()));
    let closed = accepted(&mut engine, &close);
    assert!(
        closed.effects.is_empty(),
        "focus never publishes as wire effects: {closed:?}"
    );
    let request = closed.focus_request.clone().expect("close winner retained");
    assert_eq!(request.target.0, "win-c", "close survivor wins: {closed:?}");
    assert_eq!(request.cause, FocusRequestCause::PositiveClose);
    assert_eq!(request.id, "focus-5");
    assert_eq!(
        engine.execution_subject(),
        Some(WindowId("win-c".to_owned())),
        "cursor follows published winner C, not hover D"
    );
    // The issuing reply is skipped adapter-side: the next unchanged-baseline
    // report carries no feedback. A trailing implicit non-focus command
    // resolves its subject from the cursor (C), never the hover loser.
    let mut follow = report(6, survivors, vec![], "win-a", true, None, None);
    follow.cursor = Some((310, 10));
    follow.cursor_hit = Some(WindowId("win-d".to_owned()));
    let toggle = WorldCommand::Implicit(Box::new(WorldCommand::Operation(
        CoreCommand::ToggleFloat {
            window: String::new(),
            float_rect: None,
        },
    )));
    let (reply, settlements) = match engine.handle_report(&follow, &[(5, toggle)]) {
        (V4RunOutcome::Accepted(reply), settlements) => (reply, settlements),
        (other, _) => panic!("expected accepted batch, got {other:?}"),
    };
    assert_eq!(settlements.len(), 1);
    assert!(
        !matches!(
            settlements[0].result,
            Some(crate::world_report::CommandResult::Rejected { .. })
        ),
        "implicit non-focus must not reject; got {:?}",
        settlements[0].result
    );
    let still = reply.focus_request.clone().expect("pending C survives");
    assert_eq!(still.target.0, "win-c");
    assert_eq!(still.id, request.id, "re-emission never changes the id");
    assert_eq!(
        engine.execution_subject(),
        Some(WindowId("win-c".to_owned())),
        "implicit subject stays on winner C, not hover D"
    );
    assert!(
        reply
            .intent_desired
            .iter()
            .any(|write| write.win.0 == "win-c"),
        "implicit non-focus resolves to winner C: {:?}",
        reply.intent_desired
    );
    assert!(
        reply
            .intent_desired
            .iter()
            .all(|write| write.win.0 != "win-d"),
        "hover loser D takes no implicit subject: {:?}",
        reply.intent_desired
    );
}

#[test]
fn ineligible_same_domain_sticky_fallback_mints_no_request() {
    // Sticky lifetimes carry no members-map homing, so a same-report
    // fallback onto one is never an eligible tiled survivor: the freeze
    // mints nothing, and the shared retained validator never sees a
    // close winner over a sticky foreground.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let admitted = vec![
        fact("win-a", "out-a", 0),
        WindowFact {
            sticky: Some(true),
            ..fact("win-s", "out-a", 100)
        },
        fact("win-c", "out-a", 200),
    ];
    focus_sequence(&mut engine, 1, &admitted, &["win-a", "win-c"]);
    let closed = accepted(
        &mut engine,
        &report(
            3,
            vec![
                fact("win-a", "out-a", 0),
                WindowFact {
                    sticky: Some(true),
                    ..fact("win-s", "out-a", 100)
                },
            ],
            vec!["win-c"],
            "win-s",
            true,
            None,
            None,
        ),
    );
    assert_eq!(closed.focus_request, None);
}

fn run_batched(
    engine: &mut WorldEngine,
    report: &WorldReportV4,
    queued: &[(u64, WorldCommand)],
) -> (crate::world_report::WorldReplyV4, Vec<BatchSettlement>) {
    match engine.handle_report(report, queued) {
        (V4RunOutcome::Accepted(reply), settlements) => (reply, settlements),
        (other, _) => panic!("expected accepted batch, got {other:?}"),
    }
}

fn four_windows() -> Vec<WindowFact> {
    vec![
        fact("win-a", "out-a", 0),
        fact("win-b", "out-a", 100),
        fact("win-c", "out-a", 200),
        fact("win-d", "out-a", 300),
    ]
}

fn implicit_focus_dir(direction: &str) -> WorldCommand {
    WorldCommand::Implicit(Box::new(WorldCommand::FocusDirectional {
        window: WindowId(String::new()),
        direction: direction.to_owned(),
    }))
}

fn implicit_move_dir(direction: &str) -> WorldCommand {
    WorldCommand::Implicit(Box::new(WorldCommand::MoveDirectional {
        window: WindowId(String::new()),
        direction: direction.to_owned(),
        same_axis: crate::directional::SameAxisMove::GroupWithNeighbor,
    }))
}

fn mint_close_a(engine: &mut WorldEngine) -> crate::world_report::FocusRequest {
    let windows = four_windows();
    focus_sequence(engine, 1, &windows, &["win-a", "win-b"]);
    // Survivors in C/A/D order with out-b adjacent right; the observed
    // leftward probe from A reaches D.
    let survivors = vec![
        fact("win-c", "out-a", 0),
        fact("win-a", "out-a", 100),
        fact("win-d", "out-a", 200),
    ];
    let closed = accepted(
        engine,
        &report(3, survivors, vec!["win-b"], "win-c", true, None, None),
    );
    closed.focus_request.clone().expect("close retains A")
}

/// Positive close with a live known non-target fallback: pre-close MRU A,
/// closing focus B, host fallback C. The freeze keeps A over the live C
/// baseline; stable C (silent, then accepted-unchanged feedback) retains
/// the identical row; a directional focus probe then acts from the
/// preserved cursor A and supersedes to D, while a move probe on a fresh
/// engine keeps the subject on A.
#[test]
fn close_live_non_target_baseline_freezes_a_and_probes_act_from_a() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let issued = mint_close_a(&mut engine);
    assert_eq!(issued.target.0, "win-a");
    assert_eq!(issued.cause, FocusRequestCause::PositiveClose);
    assert_eq!(issued.id, "focus-3");
    assert_eq!(
        engine
            .pending_focus
            .as_ref()
            .expect("pending")
            .baseline_focused,
        "win-c",
        "freeze anchors on the live host fallback C"
    );
    assert_eq!(
        engine.execution_subject(),
        Some(WindowId("win-a".to_owned())),
        "preserved cursor owns the subject over live C"
    );
    let survivors = vec![
        fact("win-c", "out-a", 0),
        fact("win-a", "out-a", 100),
        fact("win-d", "out-a", 200),
    ];
    let kept = accepted(
        &mut engine,
        &report(4, survivors.clone(), vec![], "win-c", true, None, None),
    );
    assert_eq!(kept.focus_request, Some(issued.clone()));
    let inert = FocusFeedback {
        id: issued.id.clone(),
        target: WindowId("win-a".to_owned()),
        revision: issued.issued_revision,
        setter: VisibilitySetter::Accepted,
        reason: VisibilityReason::Empty,
        pre: Some(WindowId("win-c".to_owned())),
        post: Some(WindowId("win-c".to_owned())),
        foreground_proven: false,
        arrival: Vec::new(),
        attempt_ms: 7,
    };
    let mut with_inert = report(5, survivors.clone(), vec![], "win-c", true, None, None);
    with_inert.focus_feedback = vec![inert];
    let kept2 = accepted(&mut engine, &with_inert);
    assert_eq!(
        kept2.focus_request,
        Some(issued.clone()),
        "accepted-unchanged over live C is inert"
    );
    let before: Vec<_> = kept2
        .windows
        .iter()
        .map(|row| (row.lifetime.0.clone(), row.allocation))
        .collect();
    // Focus probe first on the pristine layout: the observed leftward
    // probe from A reaches D.
    let (after_focus, focus_settlements) = run_batched(
        &mut engine,
        &report(6, survivors.clone(), vec![], "win-c", true, None, None),
        &[(9, implicit_focus_dir("left"))],
    );
    assert_eq!(focus_settlements.len(), 1);
    assert!(
        !matches!(
            focus_settlements[0].result,
            Some(CommandResult::Rejected { .. })
        ),
        "focus probe must not reject; got {:?}",
        focus_settlements[0].result
    );
    let next = after_focus.focus_request.clone().expect("new decision");
    assert_eq!(next.target.0, "win-d", "left of A among C/A/D is D");
    assert_eq!(next.cause, FocusRequestCause::ExplicitCommand);
    assert_eq!(next.cmd_seq, Some(9));
    assert_eq!(next.id, "focus-6");
    // Move probe on a fresh engine so the focus supersession above cannot
    // disturb the layout: a leftward move from A changes the tiling and
    // keeps the subject on the mover A.
    let mut moved_engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let moved_issued = mint_close_a(&mut moved_engine);
    assert_eq!(moved_issued.target.0, "win-a");
    let (after_move, move_settlements) = run_batched(
        &mut moved_engine,
        &report(4, survivors.clone(), vec![], "win-c", true, None, None),
        &[(9, implicit_move_dir("left"))],
    );
    assert_eq!(move_settlements.len(), 1);
    assert!(
        move_settlements[0].result.is_none(),
        "tiled move success carries no result; got {:?}",
        move_settlements[0].result
    );
    let after: Vec<_> = after_move
        .windows
        .iter()
        .map(|row| (row.lifetime.0.clone(), row.allocation))
        .collect();
    assert_ne!(before, after, "move must change the tiling allocation");
    assert_eq!(
        after_move
            .focus_request
            .as_ref()
            .map(|row| row.target.0.clone()),
        Some("win-a".to_owned()),
        "move keeps the subject on A; got {:?}",
        after_move.focus_request
    );
}

/// Close retirement branches: target equality, changed user focus, and
/// target loss. Refusal and view-drift retirement reuse existing
/// coverage (`correlated_refusal_retires_pending_request`,
/// `adopted_view_change_supersedes_pending_request`).
#[test]
fn close_retirement_equality_changed_focus_target_loss() {
    let mut equality = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    mint_close_a(&mut equality);
    let survivors = vec![
        fact("win-c", "out-a", 0),
        fact("win-a", "out-a", 100),
        fact("win-d", "out-a", 200),
    ];
    let satisfied = accepted(
        &mut equality,
        &report(4, survivors.clone(), vec![], "win-a", true, None, None),
    );
    assert_eq!(satisfied.focus_request, None);
    let mut changed = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    mint_close_a(&mut changed);
    let superseded = accepted(
        &mut changed,
        &report(4, survivors.clone(), vec![], "win-d", true, None, None),
    );
    assert_eq!(superseded.focus_request, None);
    assert_eq!(
        changed.execution_subject(),
        Some(WindowId("win-d".to_owned())),
        "changed user focus owns the subject after retirement"
    );
    let mut lost = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    mint_close_a(&mut lost);
    let retired = accepted(
        &mut lost,
        &report(
            4,
            vec![fact("win-c", "out-a", 0), fact("win-d", "out-a", 200)],
            vec!["win-a"],
            "win-c",
            true,
            None,
            None,
        ),
    );
    assert_eq!(retired.focus_request, None);
}

#[test]
fn trace_emits_pre_mint_close_equality_without_fabricating_request() {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let windows = vec![
        fact("win-a", "out-a", 0),
        fact("win-b", "out-a", 100),
        fact("win-c", "out-a", 200),
    ];
    focus_sequence(&mut engine, 1, &windows, &["win-a", "win-c", "win-b"]);
    let lines = crate::world_engine::with_focus_trace_capture(|| {
        // Close win-b when host fallback already chose the MRU survivor win-c:
        // no retained request is minted, and trace logs id=- / issuingRev=- / request=not-created.
        let closed = accepted(
            &mut engine,
            &report(
                4,
                vec![fact("win-a", "out-a", 0), fact("win-c", "out-a", 200)],
                vec!["win-b"],
                "win-c",
                true,
                None,
                None,
            ),
        );
        assert_eq!(closed.focus_request, None);
    });
    assert_eq!(
        lines.len(),
        1,
        "expected one pre-mint close equality trace: {lines:?}"
    );
    assert_eq!(
        lines[0],
        "omnitiler:core:trace ev=focus-eval gen=gen-1 req=req-4 obs=4 rev=4 id=- target=win-c issuingRev=- cause=positive-close cmdSeq=- decision=equality reason=already-target request=not-created"
    );
}

#[test]
fn trace_emits_retained_equality_and_terminal_retirements_for_admission_and_close() {
    // 1. AutomaticAdmission retained equality retirement
    let mut admission_engine =
        WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    accepted(
        &mut admission_engine,
        &report(
            1,
            vec![fact("win-a", "out-a", 0)],
            vec![],
            "win-a",
            true,
            None,
            None,
        ),
    );
    let admitted = accepted(
        &mut admission_engine,
        &report(
            2,
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            "win-a",
            true,
            None,
            None,
        ),
    );
    let req = admitted.focus_request.expect("admission focus request");
    assert_eq!(req.cause, FocusRequestCause::AutomaticAdmission);
    assert_eq!(req.target.0, "win-b");

    let admission_lines = crate::world_engine::with_focus_trace_capture(|| {
        let satisfied = accepted(
            &mut admission_engine,
            &report(
                3,
                vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
                vec![],
                "win-b",
                true,
                None,
                None,
            ),
        );
        assert_eq!(satisfied.focus_request, None);
    });
    assert_eq!(
        admission_lines.len(),
        1,
        "admission equality: {admission_lines:?}"
    );
    assert_eq!(
        admission_lines[0],
        format!(
            "omnitiler:core:trace ev=focus-eval gen=gen-1 req=req-3 obs=3 rev=3 id={} target=win-b issuingRev={} cause=automatic-admission cmdSeq=- decision=equality reason=already-target",
            req.id, req.issued_revision
        )
    );

    let survivors = vec![
        fact("win-c", "out-a", 0),
        fact("win-a", "out-a", 100),
        fact("win-d", "out-a", 200),
    ];

    // 2. PositiveClose retained equality retirement
    let mut close_eq_engine =
        WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let close_eq_req = mint_close_a(&mut close_eq_engine);
    let close_eq_lines = crate::world_engine::with_focus_trace_capture(|| {
        let satisfied = accepted(
            &mut close_eq_engine,
            &report(4, survivors.clone(), vec![], "win-a", true, None, None),
        );
        assert_eq!(satisfied.focus_request, None);
    });
    assert_eq!(close_eq_lines.len(), 1);
    assert_eq!(
        close_eq_lines[0],
        format!(
            "omnitiler:core:trace ev=focus-eval gen=gen-1 req=req-4 obs=4 rev=4 id={} target=win-a issuingRev={} cause=positive-close cmdSeq=- decision=equality reason=already-target",
            close_eq_req.id, close_eq_req.issued_revision
        )
    );

    // 3. NotCalled feedback retirement -> decision=no-call
    let mut not_called_engine =
        WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let close_req = mint_close_a(&mut not_called_engine);
    let close_lines = crate::world_engine::with_focus_trace_capture(|| {
        let mut with_feedback = report(4, survivors.clone(), vec![], "win-c", true, None, None);
        with_feedback.focus_feedback.push(FocusFeedback {
            id: close_req.id.clone(),
            target: close_req.target.clone(),
            revision: close_req.issued_revision,
            setter: VisibilitySetter::NotCalled,
            reason: VisibilityReason::IdentityStale,
            pre: Some(WindowId("win-c".to_owned())),
            post: Some(WindowId("win-c".to_owned())),
            foreground_proven: false,
            arrival: Vec::new(),
            attempt_ms: 0,
        });
        let retired = accepted(&mut not_called_engine, &with_feedback);
        assert_eq!(retired.focus_request, None);
    });
    assert_eq!(
        close_lines.len(),
        1,
        "close feedback retirement: {close_lines:?}"
    );
    assert_eq!(
        close_lines[0],
        format!(
            "omnitiler:core:trace ev=focus-eval gen=gen-1 req=req-4 obs=4 rev=4 id={} target=win-a issuingRev={} cause=positive-close cmdSeq=- decision=no-call reason=identity-stale",
            close_req.id, close_req.issued_revision
        )
    );

    // 4. Refused feedback retirement -> decision=called reason=native-refused
    let mut refused_engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let refused_req = mint_close_a(&mut refused_engine);
    let refused_lines = crate::world_engine::with_focus_trace_capture(|| {
        let mut with_feedback = report(4, survivors.clone(), vec![], "win-c", true, None, None);
        with_feedback.focus_feedback.push(FocusFeedback {
            id: refused_req.id.clone(),
            target: refused_req.target.clone(),
            revision: refused_req.issued_revision,
            setter: VisibilitySetter::Refused,
            reason: VisibilityReason::NativeRefused,
            pre: Some(WindowId("win-c".to_owned())),
            post: Some(WindowId("win-c".to_owned())),
            foreground_proven: true,
            arrival: Vec::new(),
            attempt_ms: 0,
        });
        let retired = accepted(&mut refused_engine, &with_feedback);
        assert_eq!(retired.focus_request, None);
    });
    assert_eq!(refused_lines.len(), 1);
    assert_eq!(
        refused_lines[0],
        format!(
            "omnitiler:core:trace ev=focus-eval gen=gen-1 req=req-4 obs=4 rev=4 id={} target=win-a issuingRev={} cause=positive-close cmdSeq=- decision=called reason=native-refused",
            refused_req.id, refused_req.issued_revision
        )
    );

    // 5. Threw feedback retirement -> decision=called reason=unsupported
    let mut threw_engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let threw_req = mint_close_a(&mut threw_engine);
    let threw_lines = crate::world_engine::with_focus_trace_capture(|| {
        let mut with_feedback = report(4, survivors.clone(), vec![], "win-c", true, None, None);
        with_feedback.focus_feedback.push(FocusFeedback {
            id: threw_req.id.clone(),
            target: threw_req.target.clone(),
            revision: threw_req.issued_revision,
            setter: VisibilitySetter::Threw,
            reason: VisibilityReason::Unsupported,
            pre: Some(WindowId("win-c".to_owned())),
            post: Some(WindowId("win-c".to_owned())),
            foreground_proven: false,
            arrival: Vec::new(),
            attempt_ms: 0,
        });
        let retired = accepted(&mut threw_engine, &with_feedback);
        assert_eq!(retired.focus_request, None);
    });
    assert_eq!(threw_lines.len(), 1);
    assert_eq!(
        threw_lines[0],
        format!(
            "omnitiler:core:trace ev=focus-eval gen=gen-1 req=req-4 obs=4 rev=4 id={} target=win-a issuingRev={} cause=positive-close cmdSeq=- decision=called reason=unsupported",
            threw_req.id, threw_req.issued_revision
        )
    );

    // 6. target-closed retirement
    let mut closed_engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let closed_req = mint_close_a(&mut closed_engine);
    let closed_lines = crate::world_engine::with_focus_trace_capture(|| {
        let retired = accepted(
            &mut closed_engine,
            &report(
                4,
                vec![fact("win-c", "out-a", 0), fact("win-d", "out-a", 200)],
                vec!["win-a"],
                "win-c",
                true,
                None,
                None,
            ),
        );
        assert_eq!(retired.focus_request, None);
    });
    assert_eq!(closed_lines.len(), 1);
    assert_eq!(
        closed_lines[0],
        format!(
            "omnitiler:core:trace ev=focus-eval gen=gen-1 req=req-4 obs=4 rev=4 id={} target=win-a issuingRev={} cause=positive-close cmdSeq=- decision=no-call reason=target-closed",
            closed_req.id, closed_req.issued_revision
        )
    );

    // 7. focus-unproven retirement (PositiveClose with empty + unknown foreground)
    let mut unproven_engine =
        WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let unproven_req = mint_close_a(&mut unproven_engine);
    let unproven_lines = crate::world_engine::with_focus_trace_capture(|| {
        let retired = accepted(
            &mut unproven_engine,
            &report(4, survivors.clone(), vec![], "", false, None, None),
        );
        assert_eq!(retired.focus_request, None);
    });
    assert_eq!(unproven_lines.len(), 1);
    assert_eq!(
        unproven_lines[0],
        format!(
            "omnitiler:core:trace ev=focus-eval gen=gen-1 req=req-4 obs=4 rev=4 id={} target=win-a issuingRev={} cause=positive-close cmdSeq=- decision=no-call reason=focus-unproven",
            unproven_req.id, unproven_req.issued_revision
        )
    );

    // 8. baseline-changed retirement
    let mut baseline_engine =
        WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let baseline_req = mint_close_a(&mut baseline_engine);
    let baseline_lines = crate::world_engine::with_focus_trace_capture(|| {
        let retired = accepted(
            &mut baseline_engine,
            &report(4, survivors.clone(), vec![], "win-d", true, None, None),
        );
        assert_eq!(retired.focus_request, None);
    });
    assert_eq!(baseline_lines.len(), 1);
    assert_eq!(
        baseline_lines[0],
        format!(
            "omnitiler:core:trace ev=focus-eval gen=gen-1 req=req-4 obs=4 rev=4 id={} target=win-a issuingRev={} cause=positive-close cmdSeq=- decision=no-call reason=baseline-changed",
            baseline_req.id, baseline_req.issued_revision
        )
    );

    // 9. view-drift retirement
    let mut view_engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    let windows = vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)];
    focus_sequence(&mut view_engine, 1, &windows, &["win-a", "win-b"]);
    let view_survivors = vec![fact("win-a", "out-a", 0)];
    let minted = accepted(
        &mut view_engine,
        &report(
            3,
            view_survivors.clone(),
            vec!["win-b"],
            "",
            true,
            None,
            None,
        ),
    );
    let view_req = minted.focus_request.clone().expect("minted close");
    let mut ring = view_engine
        .mapping
        .get("out-a")
        .cloned()
        .unwrap_or_else(|| vec![view_of(&minted, "out-a")]);
    if !ring.contains(&"ws-moved".to_owned()) {
        ring.push("ws-moved".to_owned());
    }
    view_engine.mapping.insert("out-a".to_owned(), ring);
    view_engine
        .views
        .insert("out-a".to_owned(), "ws-moved".to_owned());
    let view_lines = crate::world_engine::with_focus_trace_capture(|| {
        let retired = accepted(
            &mut view_engine,
            &report(4, view_survivors, vec![], "", true, None, None),
        );
        assert_eq!(retired.focus_request, None);
    });
    assert_eq!(view_lines.len(), 1);
    assert_eq!(
        view_lines[0],
        format!(
            "omnitiler:core:trace ev=focus-eval gen=gen-1 req=req-4 obs=4 rev=4 id={} target=win-a issuingRev={} cause=positive-close cmdSeq=- decision=no-call reason=view-drift",
            view_req.id, view_req.issued_revision
        )
    );

    // 10. Deterministic trace-off check (independent of external OMNITILER_TRACE env).
    let mut disabled_engine =
        WorldEngine::new_test_bound("gen-1", "adapter-1").expect("generation");
    mint_close_a(&mut disabled_engine);
    let captured_while_disabled = crate::world_engine::with_focus_trace_capture(|| {
        crate::world_engine::with_focus_trace_disabled(|| {
            assert!(!crate::world_engine::core_trace_enabled());
            let satisfied = accepted(
                &mut disabled_engine,
                &report(4, survivors.clone(), vec![], "win-a", true, None, None),
            );
            assert_eq!(satisfied.focus_request, None);
        });
    });
    assert!(
        captured_while_disabled.is_empty(),
        "disabled trace must emit zero lines: {captured_while_disabled:?}"
    );
}

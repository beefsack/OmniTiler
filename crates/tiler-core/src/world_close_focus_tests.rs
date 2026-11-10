//! Core-owned positive-close refocus: retained request happy paths.
//!
//! The frozen pre-close MRU tiled survivor of the focused explicit-positive
//! close cohort is recorded before host-fallback ingestion, re-emitted with a
//! stable revision-derived id until target equality satisfies it, and stays
//! truthful to later observations.

use crate::directional::{OutputId, WindowId, WorkspaceId};
use crate::geometry::Rect;
use crate::size_hints::WindowSizeHints;
use crate::world_engine::{WorldEngine, WorldOutcome};
use crate::world_report::{
    CloseFocusCause, FocusFeedback, PresentationIngressV4, PresentationRowV4, ScreenReport,
    VisibilityReason, VisibilitySetter, WORLD_CONTRACT_VERSION_V4, WindowFact, WorldCommand,
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
    match engine.handle_report(report) {
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
    let request = closed.close_focus.as_ref().expect("retained request");
    assert_eq!(request.id, "close-focus-4");
    assert_eq!(request.target.0, "win-c");
    assert_eq!(request.issued_revision, 4);
    assert_eq!(request.cause, CloseFocusCause::PositiveClose);
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
    let request = closed.close_focus.as_ref().expect("retained request");
    assert_eq!(request.id, "close-focus-3");
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
    let issued = closed.close_focus.clone().expect("retained request");
    assert_eq!(issued.id, "close-focus-3");
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
    assert_eq!(burst.close_focus, Some(issued));
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
    assert_eq!(satisfied.close_focus, None);
}

fn float_fact(window: &str, output: &str, x: i32) -> WindowFact {
    WindowFact {
        transient: Some(true),
        ..fact(window, output, x)
    }
}

fn feedback_row(
    id: &str,
    target: &str,
    revision: u64,
    setter: VisibilitySetter,
    reason: VisibilityReason,
    pre: Option<&str>,
    post: Option<&str>,
    proven: bool,
) -> FocusFeedback {
    FocusFeedback {
        id: id.to_owned(),
        target: WindowId(target.to_owned()),
        revision,
        setter,
        reason,
        pre: pre.map(|id| WindowId(id.to_owned())),
        post: post.map(|id| WindowId(id.to_owned())),
        foreground_proven: proven,
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
    assert_eq!(closed.close_focus, None);
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
    let request = closed.close_focus.as_ref().expect("retained request");
    assert_eq!(request.id, "close-focus-5");
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
    let issued = minted.close_focus.clone().expect("retained request");
    assert_eq!(issued.target.0, "win-a");
    // The first publication is skipped adapter-side; the identical row still
    // re-emits on the next unchanged ingress report.
    let skipped = accepted(
        &mut engine,
        &report(4, survivors.clone(), vec![], "", true, None, None),
    );
    assert_eq!(skipped.close_focus, Some(issued));
    // A later observed change to win-d supersedes before any native apply.
    let superseded = accepted(
        &mut engine,
        &report(5, survivors.clone(), vec![], "win-d", true, None, None),
    );
    assert_eq!(superseded.close_focus, None);
    // Later equality with the retired target cannot resurrect the request:
    // the prior global is win-d, and no new close cohort exists.
    let no_resurrection = accepted(
        &mut engine,
        &report(6, survivors, vec![], "win-a", true, None, None),
    );
    assert_eq!(no_resurrection.close_focus, None);
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
    assert_eq!(unknown_close.close_focus, None);
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
    assert!(minted.close_focus.is_some());
    let retired = accepted(
        &mut engine,
        &report(4, survivors, vec![], "", false, None, None),
    );
    assert_eq!(retired.close_focus, None);
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
    assert!(minted.close_focus.is_some());
    // A command riding a later report supersedes even with unchanged focus.
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
    assert_eq!(superseded.close_focus, None);
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
    assert!(minted.close_focus.is_some());
    // Repeating the consumed sequence reconciles only, and still retires.
    // A consumed repeat publishes through the CommandConsumed outcome with
    // the same reply shape.
    let superseded = match engine.handle_report(&report(
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
    )) {
        WorldOutcome::Accepted(reply) | WorldOutcome::CommandConsumed(reply) => reply,
        other => panic!("expected accepted or consumed, got {other:?}"),
    };
    assert_eq!(superseded.close_focus, None);
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
    assert!(minted.close_focus.is_some());
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
    assert_eq!(superseded.close_focus, None);
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
    assert_eq!(closed.close_focus, None);
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
    assert!(minted.close_focus.is_some());
    let mut minimized = fact("win-a", "out-a", 0);
    minimized.minimized = Some(true);
    minimized.rect = None;
    let retired = accepted(
        &mut engine,
        &report(4, vec![minimized], vec![], "", true, None, None),
    );
    assert_eq!(retired.close_focus, None);
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
    assert!(minted.close_focus.is_some());
    let mut hidden = fact("win-a", "out-a", 0);
    hidden.host_hidden = Some(true);
    let retired = accepted(
        &mut engine,
        &report(4, vec![hidden], vec![], "", true, None, None),
    );
    assert_eq!(retired.close_focus, None);
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
    assert!(minted.close_focus.is_some());
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
    assert_eq!(retired.close_focus, None);
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
    assert_eq!(closed.close_focus, None);
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
    assert!(minted.close_focus.is_some());
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
    assert_eq!(retired.close_focus, None);
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
    assert_eq!(accepted(&mut engine, &gapped).close_focus, None);
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
    assert_eq!(absent.close_focus, None);
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
    assert_eq!(unfocused.close_focus, None);
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
    let issued = minted.close_focus.clone().expect("retained request");
    // The duplicate close report carries no new evidence: the identical
    // row persists with no id renewal.
    let dup = accepted(
        &mut engine,
        &report(4, survivors, vec!["win-b"], "", true, None, None),
    );
    assert_eq!(dup.close_focus, Some(issued));
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
    let request = closed.close_focus.as_ref().expect("retained request");
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
    assert_eq!(closed.close_focus, None);
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
    assert_eq!(closed.close_focus, None);
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
    assert_eq!(closed.close_focus, None);
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
    assert_eq!(closed.close_focus, None);
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
    let issued = minted.close_focus.clone().expect("retained request");
    let refused_shape = |id: &str, target: &str, revision: u64| {
        feedback_row(
            id,
            target,
            revision,
            VisibilitySetter::Refused,
            VisibilityReason::NativeRefused,
            Some("win-x"),
            Some("win-x"),
            false,
        )
    };
    // Wrong id, wrong target, and wrong revision rows all miss the exact
    // opaque tuple and change nothing.
    for (seq, id, target, revision) in [
        (4u64, "close-focus-999", "win-a", 3),
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
            kept.close_focus,
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
    let issued = minted.close_focus.clone().expect("retained request");
    // Setter refused with the same unchanged non-target pre/post and no
    // proof: terminal refusal under the ordinary-focus rule.
    let retired = accepted(
        &mut engine,
        &with_feedback(
            report(4, survivors, vec![], "", true, None, None),
            vec![feedback_row(
                &issued.id,
                "win-a",
                issued.issued_revision,
                VisibilitySetter::Refused,
                VisibilityReason::NativeRefused,
                Some("win-x"),
                Some("win-x"),
                false,
            )],
        ),
    );
    assert_eq!(retired.close_focus, None);
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
    let issued = minted.close_focus.clone().expect("retained request");
    // Accepted without equality is unproven: the row latches and the
    // request retains with its original stamp.
    let unproven = feedback_row(
        &issued.id,
        "win-a",
        issued.issued_revision,
        VisibilitySetter::Accepted,
        VisibilityReason::Empty,
        Some("win-x"),
        Some("win-x"),
        false,
    );
    let mut unproven_late = unproven.clone();
    unproven_late.attempt_ms = 99;
    let kept = accepted(
        &mut engine,
        &with_feedback(
            report(4, survivors.clone(), vec![], "", true, None, None),
            vec![unproven],
        ),
    );
    assert_eq!(kept.close_focus, Some(issued.clone()));
    // A later refusal for the same tuple cannot replace the latched first
    // row, so the request still retains.
    let refused = feedback_row(
        &issued.id,
        "win-a",
        issued.issued_revision,
        VisibilitySetter::Refused,
        VisibilityReason::NativeRefused,
        Some("win-x"),
        Some("win-x"),
        false,
    );
    let still_kept = accepted(
        &mut engine,
        &with_feedback(
            report(5, survivors, vec![], "", true, None, None),
            vec![refused, unproven_late],
        ),
    );
    assert_eq!(still_kept.close_focus, Some(issued));
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
    let issued = minted.close_focus.clone().expect("retained request");
    // A terminal safety no-call (native fullscreen veto, spent id) retires;
    // it never becomes a retry.
    let retired = accepted(
        &mut engine,
        &with_feedback(
            report(4, survivors, vec![], "", true, None, None),
            vec![feedback_row(
                &issued.id,
                "win-a",
                issued.issued_revision,
                VisibilitySetter::NotCalled,
                VisibilityReason::Fullscreen,
                None,
                None,
                false,
            )],
        ),
    );
    assert_eq!(retired.close_focus, None);
}

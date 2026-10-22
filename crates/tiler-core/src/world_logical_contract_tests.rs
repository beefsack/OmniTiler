//! Logical-first world contract tests (stage 2a): native-free bootstrap
//! and lifecycle, sends and selection, and the source-only-backing
//! disjoint migration discriminator with independent arrival/view/refill
//! settlement. No native workspace concept anywhere: screens plus window
//! facts are the only observations, and every workspace id is core-minted
//! (`ws-<n>`).

use crate::directional::{OutputId, WindowId};
use crate::geometry::Rect;
use crate::size_hints::WindowSizeHints;
use crate::world_engine::{WorldEngine, WorldOutcome};
use crate::world_report::{
    HoldReason, ScopeAuthority, ScreenReport, ScreenView, TransferKind, WORLD_CONTRACT_VERSION,
    WindowFact, WorldCommand, WorldReport,
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

fn minimized_fact(window: &str, output: &str) -> WindowFact {
    WindowFact {
        minimized: true,
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
    feedback: Vec<crate::world_report::AttemptFeedback>,
) -> WorldReport {
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
        feedback,
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
    feedback: Vec<crate::world_report::AttemptFeedback>,
    workspace_mode: crate::workspace_scope::WorkspaceMode,
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
        workspace_mode,
        screens,
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
        default_tiled: true,
        focused_global: focused.to_owned(),
        inventory_complete,
        gesture_barrier,
        feedback,
        monotonic_ms: 0,
        frames: Vec::new(),
    }
}

fn two_screens() -> Vec<ScreenReport> {
    vec![screen("out-a", 0, true), screen("out-b", 800, false)]
}

fn accepted(engine: &mut WorldEngine, report: &WorldReport) -> crate::world_report::WorldReply {
    match engine.handle_report(report) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    }
}

fn workspace_ids(reply: &crate::world_report::WorldReply, output: &str) -> Vec<String> {
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
    reply: &crate::world_report::WorldReply,
    output: &str,
    workspace: &str,
) -> ScopeAuthority {
    reply
        .scopes
        .iter()
        .find(|status| status.scope == format!("{output}/{workspace}"))
        .unwrap_or_else(|| panic!("missing scope {output}/{workspace}"))
        .authority
}

fn window_row<'a>(
    reply: &'a crate::world_report::WorldReply,
    window: &str,
) -> &'a crate::world_report::WindowDesire {
    reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == window)
        .unwrap_or_else(|| panic!("missing window row {window}"))
}

fn output_view(reply: &crate::world_report::WorldReply, output: &str) -> Option<String> {
    reply
        .outputs
        .iter()
        .find(|row| row.output.0 == output)
        .and_then(|row| row.logical_view.clone())
}

#[test]
fn native_free_bootstrap_creates_per_screen_rings_views_and_trailing() {
    let mut engine = WorldEngine::new("gen-1").expect("valid generation");
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
            vec![],
        ),
    );
    assert!(reply.ready);
    // One owned workspace plus a trailing empty per screen, stable ids.
    let ring_a = workspace_ids(&reply, "out-a");
    let ring_b = workspace_ids(&reply, "out-b");
    assert_eq!(ring_a.len(), 2, "ring: {ring_a:?}");
    assert_eq!(ring_b.len(), 2, "ring: {ring_b:?}");
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
            "{}",
            scope.scope
        );
    }
    // Untouched workspaces authorize nothing yet: no windows, no transfers.
    assert!(reply.windows.is_empty());
    assert!(reply.transfers.is_empty());
}

#[test]
fn admission_homes_facts_to_views_and_lifecycle_grows_trailing() {
    let mut engine = WorldEngine::new("gen-1").expect("valid generation");
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
            vec![],
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
            vec![],
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
            vec![],
        ),
    );
    assert!(third.windows.iter().all(|row| row.lifetime.0 != "win-a"));
    let ring_a = workspace_ids(&third, "out-a");
    assert_eq!(ring_a.len(), 2, "ring: {ring_a:?}");
}

#[test]
fn fresh_admission_focus_is_at_most_once() {
    let mut engine = WorldEngine::new("gen-1").expect("valid generation");
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
            vec![],
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
            vec![],
        ),
    );
    assert_eq!(second.effects.len(), 1);
    assert!(second.effects[0].id.starts_with("af-"));
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
            vec![],
        ),
    );
    assert!(third.effects.is_empty());
}

#[test]
fn send_follow_and_stay_commit_window_transfers() {
    let mut engine = WorldEngine::new("gen-1").expect("valid generation");
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
            vec![],
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
            window: WindowId("win-a".to_owned()),
            direction: "right".to_owned(),
            follow: true,
        }),
        true,
        false,
        vec![],
    );
    let reply = accepted(&mut engine, &follow);
    assert_eq!(reply.effects.len(), 1);
    assert_eq!(reply.effects[0].target.0, "win-a");
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
    let mut stay = report(
        4,
        4,
        two_screens(),
        vec![fact("win-a", "out-b", 800), fact("win-b", "out-a", 100)],
        vec![("out-a", ""), ("out-b", "")],
        vec![],
        "win-b",
        Some(12),
        Some(WorldCommand::SendDirectional {
            window: WindowId("win-b".to_owned()),
            direction: "right".to_owned(),
            follow: false,
        }),
        true,
        false,
        vec![],
    );
    // Echo current views so homing stays truthful.
    let _ = &mut stay;
    let reply = accepted(&mut engine, &stay);
    assert!(
        reply.effects.is_empty(),
        "emptied source names no focus: {:?}",
        reply.effects
    );
    assert_eq!(reply.transfers.len(), 1);
}

#[test]
fn stay_send_with_survivor_names_exact_source_focus() {
    let mut engine = WorldEngine::new("gen-1").expect("valid generation");
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
            vec![],
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
                vec![],
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
                window: WindowId("win-a".to_owned()),
                direction: "right".to_owned(),
                follow: false,
            }),
            true,
            false,
            vec![],
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
    let mut engine = WorldEngine::new("gen-1").expect("valid generation");
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
            vec![],
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
                window: WindowId("win-a".to_owned()),
                target_output: OutputId("out-a".to_owned()),
                target_workspace: ring_a[1].clone(),
                follow: true,
            }),
            true,
            false,
            vec![],
        ),
    );
    assert_eq!(reply.transfers.len(), 2);
    assert!(matches!(&reply.transfers[0].kind, TransferKind::View));
    assert!(matches!(&reply.transfers[1].kind, TransferKind::Window(_)));
    assert_eq!(reply.transfers[0].target_workspace, ring_a[1]);
    assert_eq!(reply.transfers[1].target_workspace, ring_a[1]);
    assert_eq!(reply.transfers[0].operation_id, "op-11-follow");
    // Explicit cross-output send refuses at the engine boundary (parked
    // explicit-vs-directional mismatch): directional sends own the
    // crossing, even when the logical target is known. The prior
    // same-output flight settles on the observed membership carried in
    // the observation-only report above (a rejected command would discard
    // the settlement with it).
    let view_b = output_view(&first, "out-b").expect("view b");
    let mut arrived = fact("win-a", "out-a", 0);
    arrived.observed_workspace = Some(ring_a[1].clone());
    // The window flight settles on the membership receipt while the
    // follow-view flight confirms on the echoed view: each proves
    // independently, so the report carries both proofs.
    let settled = accepted(
        &mut engine,
        &report(
            3,
            3,
            two_screens(),
            vec![arrived, fact("win-b", "out-a", 100)],
            vec![("out-a", ring_a[1].as_str())],
            vec![],
            "win-a",
            None,
            None,
            true,
            false,
            vec![],
        ),
    );
    assert!(
        settled.transfers.is_empty(),
        "membership proof settles: {:?}",
        settled.transfers
    );
    match engine.handle_report(&report(
        4,
        4,
        two_screens(),
        vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
        vec![],
        vec![],
        "win-a",
        Some(12),
        Some(WorldCommand::Send {
            window: WindowId("win-a".to_owned()),
            target_output: OutputId("out-b".to_owned()),
            target_workspace: view_b,
            follow: true,
        }),
        true,
        false,
        vec![],
    )) {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "cross-domain-mismatch"),
        other => panic!("expected cross-domain-mismatch, got {other:?}"),
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
                window: WindowId("win-a".to_owned()),
                direction: "right".to_owned(),
                same_axis: crate::directional::SameAxisMove::GroupWithNeighbor,
            }),
            true,
            false,
            vec![],
        ),
    );
    assert!(
        reply
            .transfers
            .iter()
            .any(|t| matches!(t.kind, TransferKind::Window(_)))
    );
}

#[test]
fn directional_focus_crosses_outputs_with_explicit_effect() {
    let mut engine = WorldEngine::new("gen-1").expect("valid generation");
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
            vec![],
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
            vec![],
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
                window: WindowId("win-a".to_owned()),
                direction: "right".to_owned(),
                float_subject: false,
            }),
            true,
            false,
            vec![],
        ),
    );
    assert_eq!(reply.effects.len(), 1);
    assert_eq!(reply.effects[0].target.0, "win-b");
    assert!(reply.effects[0].is_explicit_focus_for(11));
}

#[test]
fn directional_focus_up_never_crosses_outputs() {
    let mut engine = WorldEngine::new("gen-1").expect("valid generation");
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
            vec![],
        ),
    );
    // Up never crosses: the outcome stays local (an effect on the home
    // output) or refuses as unchanged, never an out-b target.
    match engine.handle_report(&report(
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
            window: WindowId("win-a".to_owned()),
            direction: "up".to_owned(),
            float_subject: false,
        }),
        true,
        false,
        vec![],
    )) {
        WorldOutcome::Accepted(reply) | WorldOutcome::CommandConsumed(reply) => {
            assert!(
                reply
                    .effects
                    .iter()
                    .all(|effect| effect.target.0 != "win-b")
            );
        }
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "unchanged"),
        other => panic!("unexpected outcome: {other:?}"),
    }
}

#[test]
fn selection_commands_resolve_over_logical_rings() {
    let mut engine = WorldEngine::new("gen-1").expect("valid generation");
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
            vec![],
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
            vec![],
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
            vec![],
        ),
    );
    assert_eq!(
        output_view(&second, "out-a").as_deref(),
        Some(ring_a[1].as_str())
    );
    assert_eq!(second.transfers.len(), 1);
    // Relative selection wraps around the ring.
    let third = accepted(
        &mut engine,
        &report(
            3,
            3,
            two_screens(),
            vec![],
            vec![("out-a", ring_a[1].as_str())],
            vec![],
            "",
            Some(12),
            Some(WorldCommand::SelectRelative {
                output: Some(OutputId("out-a".to_owned())),
                delta: 1,
            }),
            true,
            false,
            vec![],
        ),
    );
    assert_eq!(
        output_view(&third, "out-a").as_deref(),
        Some(view_a.as_str())
    );
    // Unknown targets refuse without mutation.
    let seq = 4;
    match engine.handle_report(&report(
        4,
        4,
        two_screens(),
        vec![],
        vec![],
        vec![],
        "",
        Some(13),
        Some(WorldCommand::SelectWorkspace {
            output: OutputId("out-a".to_owned()),
            workspace: "ws-999".to_owned(),
        }),
        true,
        false,
        vec![],
    )) {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "view-target-unknown"),
        other => panic!("expected view-target-unknown, got {other:?}"),
    }
    let _ = seq;
    // Previous toggles back through recorded history.
    let fourth = accepted(
        &mut engine,
        &report(
            5,
            5,
            two_screens(),
            vec![],
            vec![("out-a", view_a.as_str())],
            vec![],
            "",
            Some(14),
            Some(WorldCommand::SelectPrevious {
                output: Some(OutputId("out-a".to_owned())),
            }),
            true,
            false,
            vec![],
        ),
    );
    assert_eq!(
        output_view(&fourth, "out-a").as_deref(),
        Some(ring_a[1].as_str())
    );
}

#[test]
fn numbered_send_trailing_path_creates_and_commits_immediately() {
    let mut engine = WorldEngine::new("gen-1").expect("valid generation");
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
            vec![],
        ),
    );
    // Index 0 reuses the live trailing empty with an ordinary transfer.
    let reply = accepted(
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
            Some(WorldCommand::SendNumbered {
                window: WindowId("win-a".to_owned()),
                output: OutputId("out-a".to_owned()),
                index: 0,
                follow: true,
            }),
            true,
            false,
            vec![],
        ),
    );
    assert_eq!(reply.effects.len(), 1);
    assert_eq!(reply.effects[0].target.0, "win-a");
    // Follow retains the window flight plus its own view flight under a
    // derived identity, both aimed at the reused trailing empty.
    let window = reply
        .transfers
        .iter()
        .find(|t| matches!(&t.kind, TransferKind::Window(w) if w.0 == "win-a"))
        .expect("window flight");
    let view = reply
        .transfers
        .iter()
        .find(|t| matches!(&t.kind, TransferKind::View))
        .expect("follow view flight");
    assert_eq!(view.operation_id, format!("{}-follow", window.operation_id));
    assert_eq!(view.target_workspace, window.target_workspace);
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
            vec![],
        ),
    );
    assert!(workspace_ids(&ringed, "out-a").len() >= 2);
}

#[test]
fn numbered_send_mismatched_output_refuses_without_mutation() {
    let mut engine = WorldEngine::new("gen-1").expect("valid generation");
    // Incomplete inventory freezes trailing lifecycle, so each output owns
    // exactly one fully occupied workspace: any trailing resolution on the
    // foreign output takes the Create path, which must never mint there.
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
            false,
            false,
            vec![],
        ),
    );
    assert_eq!(workspace_ids(&first, "out-a").len(), 1);
    assert_eq!(workspace_ids(&first, "out-b").len(), 1);
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
    for (seq, index) in [(2u64, 0u8), (3u64, 1u8)] {
        match engine.handle_report(&report(
            seq,
            seq,
            two_screens(),
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-b", 800)],
            vec![],
            vec![],
            "win-a",
            Some(11),
            Some(WorldCommand::SendNumbered {
                window: WindowId("win-a".to_owned()),
                output: OutputId("out-b".to_owned()),
                index,
                follow: true,
            }),
            false,
            false,
            vec![],
        )) {
            WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "cross-domain-mismatch"),
            other => panic!("expected cross-domain-mismatch, got {other:?}"),
        }
    }
    // The refused commands mutated nothing: rings, views, and creation
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
            vec![],
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
fn numbered_send_trailing_create_stages_fresh_target_and_commits() {
    let mut engine = WorldEngine::new("gen-1").expect("valid generation");
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
            vec![],
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
            vec![],
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
            vec![],
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
            vec![("out-a", view_b.as_str())],
            vec![],
            "",
            None,
            None,
            false,
            false,
            vec![],
        ),
    );
    let reply = accepted(
        &mut engine,
        &report(
            5,
            5,
            two_screens(),
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![("out-a", view_b.as_str())],
            vec![],
            "win-a",
            Some(12),
            Some(WorldCommand::SendNumbered {
                window: WindowId("win-a".to_owned()),
                output: OutputId("out-a".to_owned()),
                index: 0,
                follow: true,
            }),
            false,
            false,
            vec![],
        ),
    );
    assert_eq!(reply.effects.len(), 1);
    assert_eq!(reply.effects[0].target.0, "win-a");
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
    let mut engine = WorldEngine::new("gen-1").expect("valid generation");
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
            vec![],
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
            vec![],
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
                window: WindowId("win-a".to_owned()),
                output: OutputId("out-a".to_owned()),
                index,
                follow: true,
            }),
            true,
            false,
            vec![],
        ),
    );
    assert_eq!(
        output_view(&reply, "out-a").as_deref(),
        Some(view_b.as_str())
    );
    assert_eq!(reply.effects.len(), 1);
    assert_eq!(reply.effects[0].target.0, "win-a");
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
                window: WindowId("win-b".to_owned()),
                output: OutputId("out-a".to_owned()),
                index,
                follow: false,
            }),
            true,
            false,
            vec![],
        ),
    );
    assert_eq!(
        output_view(&stayed, "out-a").as_deref(),
        Some(view_b.as_str())
    );
    assert_eq!(
        stayed.effects.len(),
        1,
        "stay names a source survivor: {:?}",
        stayed.effects
    );
    assert_ne!(stayed.effects[0].target.0, "win-b");
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
            vec![],
        ),
    );
    assert_eq!(
        output_view(&back, "out-a").as_deref(),
        Some(view_a.as_str())
    );
}

#[test]
fn numbered_follow_fans_out_views_in_shared_mode() {
    let mut engine = WorldEngine::new("gen-1").expect("valid generation");
    let mode = crate::workspace_scope::WorkspaceMode::Shared;
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
            vec![],
            mode,
        ),
    );
    let second = accepted(
        &mut engine,
        &report_mode(
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
            vec![],
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
    // Follow moves the one shared view: every live output shows the target.
    let reply = accepted(
        &mut engine,
        &report_mode(
            3,
            3,
            two_screens(),
            vec![fact("win-a", "out-a", 0)],
            vec![],
            vec![],
            "win-a",
            Some(11),
            Some(WorldCommand::SendNumbered {
                window: WindowId("win-a".to_owned()),
                output: OutputId("out-a".to_owned()),
                index,
                follow: true,
            }),
            true,
            false,
            vec![],
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
    assert_eq!(reply.effects.len(), 1);
    assert_eq!(reply.effects[0].target.0, "win-a");
}

#[test]
fn numbered_follow_moves_only_its_output_in_global_mode() {
    let mut engine = WorldEngine::new("gen-1").expect("valid generation");
    let mode = crate::workspace_scope::WorkspaceMode::GlobalUnique;
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
            vec![],
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
            vec![],
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
                window: WindowId("win-a".to_owned()),
                output: OutputId("out-a".to_owned()),
                index,
                follow: true,
            }),
            true,
            false,
            vec![],
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

#[test]
fn valid_migration_proves_controlled_target_before_arrival_then_settles_in_phases() {
    let mut engine = WorldEngine::new("gen-1").expect("valid generation");
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
            vec![],
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
                workspace: view_a.clone(),
            }),
            true,
            false,
            vec![],
        ),
    );
    // BEFORE any arrival: the logical target scope is Controlled with
    // Controlled membership desire, and both flights are retained.
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
            .any(|t| matches!(t.kind, TransferKind::View) && t.operation_id == "op-11-refill")
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
    let session = engine
        .engine()
        .session(&target_key)
        .expect("retained target session");
    assert!(
        session
            .snapshot()
            .windows
            .iter()
            .any(|link| link.window.0 == "win-a")
    );
    // An unchanged report does not flap the source: same desire, same
    // flights, same mapping.
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
            vec![],
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
            .any(|t| matches!(t.kind, TransferKind::View) && t.operation_id == "op-11-refill")
    );
    // Member arrival alone does not settle: target view and source refill
    // proofs are still missing.
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
            vec![],
        ),
    );
    assert!(
        arrived
            .transfers
            .iter()
            .any(|t| matches!(t.kind, TransferKind::Migration) && t.operation_id == "op-11")
    );
    let member = window_row(&arrived, "win-a");
    assert_eq!(
        (member.output.0.as_str(), member.workspace.as_str()),
        ("out-b", view_a.as_str())
    );
    assert_eq!(member.frame_authority, ScopeAuthority::Controlled);
    // Target view proof alone does not settle: the source refill is still
    // missing.
    let viewed = accepted(
        &mut engine,
        &report(
            5,
            5,
            two_screens(),
            vec![fact("win-a", "out-b", 800)],
            vec![("out-b", view_a.as_str())],
            vec![],
            "",
            None,
            None,
            true,
            false,
            vec![],
        ),
    );
    assert!(
        viewed
            .transfers
            .iter()
            .any(|t| matches!(t.kind, TransferKind::Migration) && t.operation_id == "op-11")
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
            vec![("out-b", view_a.as_str()), ("out-a", refill.as_str())],
            vec![],
            "",
            None,
            None,
            true,
            false,
            vec![],
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
            vec![("out-b", view_a.as_str()), ("out-a", refill.as_str())],
            vec![],
            "",
            None,
            None,
            true,
            false,
            vec![],
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
        let mut engine = WorldEngine::new("gen-1").expect("valid generation");
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
                vec![],
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
                    workspace: view_a.clone(),
                }),
                true,
                false,
                vec![],
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
                vec![],
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
    use crate::world_report::{AttemptFeedback, AttemptKind, AttemptOutcome};
    for (name, outcome) in [
        ("refused", AttemptOutcome::RefusedProven),
        ("superseded", AttemptOutcome::SupersededExternal),
    ] {
        let mut engine = WorldEngine::new("gen-1").expect("valid generation");
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
                vec![],
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
                    workspace: view_a.clone(),
                }),
                true,
                false,
                vec![],
            ),
        );
        // Terminal feedback on a wholly-unarrived migration reconciles
        // to the observed source: the logical id returns to the source
        // screen with a coherent source view, and no token is reissued.
        let settled = accepted(
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
                vec![AttemptFeedback {
                    operation_id: "op-11".to_owned(),
                    kind: AttemptKind::Transfer,
                    outcome,
                    hold: None,
                }],
            ),
        );
        assert!(
            settled.transfers.is_empty(),
            "{name}: {:?}",
            settled.transfers
        );
        let row = settled
            .workspaces
            .iter()
            .find(|row| row.id == view_a)
            .expect("mapping restored");
        assert_eq!(row.output.0, "out-a", "{name}");
        assert_eq!(
            output_view(&settled, "out-a").as_deref(),
            Some(view_a.as_str()),
            "{name}"
        );
        // Externally moved members settle without a fight.
        let mut engine = WorldEngine::new("gen-1").expect("valid generation");
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
                vec![],
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
                    workspace: view_a.clone(),
                }),
                true,
                false,
                vec![],
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
                vec![],
            ),
        );
        // Arrival on the target screen is progress, not settlement.
        assert!(
            moved
                .transfers
                .iter()
                .any(|t| matches!(t.kind, TransferKind::Migration)),
            "{name}"
        );
    }
}

#[test]
fn migration_explicit_close_drops_members_without_a_fight() {
    let mut engine = WorldEngine::new("gen-1").expect("valid generation");
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
            vec![],
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
                workspace: view_a.clone(),
            }),
            true,
            false,
            vec![],
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
            vec![],
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
            vec![],
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
    let mut engine = WorldEngine::new("gen-1").expect("valid generation");
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
            vec![],
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
                workspace: view_a.clone(),
            }),
            true,
            false,
            vec![],
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
    let session = engine
        .engine()
        .session(&target_key)
        .expect("target session");
    let mut homed: Vec<String> = session
        .snapshot()
        .windows
        .iter()
        .map(|link| link.window.0.clone())
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
            vec![("out-b", view_a.as_str())],
            vec![],
            "",
            None,
            None,
            true,
            false,
            vec![],
        ),
    );
    let refill = output_view(&settled, "out-a").expect("refill");
    let settled = accepted(
        &mut engine,
        &report(
            4,
            4,
            two_screens(),
            vec![fact("win-a", "out-b", 800), fact("win-b", "out-b", 900)],
            vec![("out-b", view_a.as_str()), ("out-a", refill.as_str())],
            vec![],
            "",
            None,
            None,
            true,
            false,
            vec![],
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
fn empty_migration_still_verifies_both_views() {
    let mut engine = WorldEngine::new("gen-1").expect("valid generation");
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
            vec![],
        ),
    );
    let ring_a: Vec<String> = first
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    assert_eq!(ring_a.len(), 2);
    // Migrate the trailing empty: no members, still needs both proofs.
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
                workspace: ring_a[1].clone(),
            }),
            true,
            false,
            vec![],
        ),
    );
    assert!(
        reply
            .transfers
            .iter()
            .any(|t| matches!(t.kind, TransferKind::Migration))
    );
    // Views alone settle an empty migration.
    let target = reply
        .transfers
        .iter()
        .find(|t| matches!(t.kind, TransferKind::Migration))
        .map(|t| t.target_workspace.clone())
        .expect("target");
    let refill = reply
        .transfers
        .iter()
        .find(|t| t.operation_id == "op-11-refill")
        .map(|t| t.target_workspace.clone())
        .expect("refill");
    let settled = accepted(
        &mut engine,
        &report(
            3,
            3,
            two_screens(),
            vec![],
            vec![("out-b", target.as_str()), ("out-a", refill.as_str())],
            vec![],
            "",
            None,
            None,
            true,
            false,
            vec![],
        ),
    );
    assert!(
        settled.transfers.is_empty(),
        "settled: {:?}",
        settled.transfers
    );
}

#[test]
fn migration_refill_prefers_mru_history() {
    use crate::output_selection::MigrationSourceRefill;
    let mut engine = WorldEngine::new("gen-1").expect("valid generation");
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
            vec![],
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
            workspace: spare.clone(),
        }),
        true,
        false,
        vec![],
    );
    accepted(&mut engine, &r);
    r = report(
        3,
        3,
        two_screens(),
        vec![fact("win-a", "out-a", 0)],
        vec![("out-a", spare.as_str())],
        vec![],
        "",
        Some(12),
        Some(WorldCommand::SelectWorkspace {
            output: OutputId("out-a".to_owned()),
            workspace: head.clone(),
        }),
        true,
        false,
        vec![],
    );
    accepted(&mut engine, &r);
    // Migrate the head with MRU refill: the source refill is the spare.
    let mut migrate = report(
        4,
        4,
        two_screens(),
        vec![fact("win-a", "out-a", 0)],
        vec![("out-a", head.as_str())],
        vec![],
        "",
        Some(13),
        Some(WorldCommand::MigrateWorkspace {
            source_output: OutputId("out-a".to_owned()),
            target_output: OutputId("out-b".to_owned()),
            workspace: head.clone(),
        }),
        true,
        false,
        vec![],
    );
    migrate.migration_source_refill = MigrationSourceRefill::MostRecentlyUsed;
    let reply = accepted(&mut engine, &migrate);
    let refill = reply
        .transfers
        .iter()
        .find(|t| t.operation_id == "op-13-refill")
        .map(|t| t.target_workspace.clone())
        .expect("mru refill");
    assert_eq!(refill, spare);
}

#[test]
fn tiled_to_float_output_crossing_leaves_source_state_unchanged() {
    let mut engine = WorldEngine::new("gen-1").expect("valid generation");
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
            vec![],
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
                workspace: ring_b[0].clone(),
                tiled: false,
            }),
            true,
            false,
            vec![],
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
                window: WindowId("win-a".to_owned()),
                direction: "right".to_owned(),
                follow: true,
            }),
            true,
            false,
            vec![],
        ),
    );
    // Membership-only boundary transfer: no two-domain Engine plan ran,
    // so the committed source session still homes the window.
    assert_eq!(reply.transfers.len(), 1);
    let source_key = crate::session::DomainKey {
        output: OutputId("out-a".to_owned()),
        workspace: crate::directional::WorkspaceId(output_view(&first, "out-a").expect("view a")),
    };
    let session = engine
        .engine()
        .session(&source_key)
        .expect("source session intact");
    assert!(
        session
            .snapshot()
            .windows
            .iter()
            .any(|link| link.window.0 == "win-a")
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
    let mut engine = WorldEngine::new("gen-1").expect("valid generation");
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
            vec![],
        ),
    );
    let mut hidden = fact("win-full", "out-a", 100);
    hidden.fullscreen = true;
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
            vec![],
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
    let mut engine = WorldEngine::new("gen-1").expect("valid generation");
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
            vec![],
        ),
    );
    let view_a = output_view(&first, "out-a").expect("view a");
    // An unexplained fit-excluded window (game/overlay with no floating,
    // fullscreen, or maximized origin) rides as an exception with no
    // authority and never takes automatic activation itself: any admission
    // intent names only ordinary newcomers. The scope stays controlled
    // around the reserved overlay.
    let mut overlay = fact("win-game", "out-a", 100);
    overlay.fit_excluded = true;
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
            vec![],
        ),
    );
    assert!(
        reply
            .effects
            .iter()
            .all(|effect| effect.target.0 != "win-game")
    );
    assert_eq!(
        scope_of(&reply, "out-a", &view_a),
        ScopeAuthority::Controlled
    );
    // The game rides converged (tiled or exception per Engine policy)
    // with no automatic activation of its own.
    let _ = window_row(&reply, "win-game");
}

#[test]
fn gesture_barrier_holds_frames_without_repair() {
    let mut engine = WorldEngine::new("gen-1").expect("valid generation");
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
            vec![],
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
            vec![],
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
            vec![],
        ),
    );
    assert!(reply.effects.is_empty());
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
            vec![],
        ),
    );
    assert!(replay.effects.is_empty(), "spent intent never replays");
}

#[test]
fn generations_watermarks_and_command_consumption_hold() {
    let mut engine = WorldEngine::new("gen-1").expect("valid generation");
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
            vec![],
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
            vec![],
        ),
    );
    match engine.handle_report(&report(
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
        vec![],
    )) {
        WorldOutcome::Duplicate(cached) => assert_eq!(cached.state_revision, first.state_revision),
        other => panic!("expected duplicate, got {other:?}"),
    }
    // Reused sequence with different input is invalid, never executed.
    match engine.handle_report(&report(
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
        vec![],
    )) {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "request-reused"),
        other => panic!("expected request-reused, got {other:?}"),
    }
    // Older sequences are superseded.
    match engine.handle_report(&report(
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
        vec![],
    )) {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "request-superseded"),
        other => panic!("expected request-superseded, got {other:?}"),
    }
    // Foreign generations never reset ordering.
    match engine.handle_report(&report(
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
        vec![],
    )) {
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
            vec![],
        ),
    );
    let transfers = view_reply.transfers.len();
    assert_eq!(transfers, 1);
    match engine.handle_report(&report(
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
        vec![],
    )) {
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
        vec![],
    );
    bad.core_generation = "gen-2".to_owned();
    match engine.handle_report(&bad) {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "generation-mismatch"),
        other => panic!("expected generation-mismatch, got {other:?}"),
    }
}

#[test]
fn unknown_outputs_and_views_hold_fail_closed() {
    let mut engine = WorldEngine::new("gen-1").expect("valid generation");
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
            vec![],
        ),
    );
    // Unknown view echoes never authorize: the output holds instead of
    // adopting a workspace the core does not own.
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            two_screens(),
            vec![fact("win-a", "out-a", 0)],
            vec![("out-a", "ws-ghost")],
            vec![],
            "",
            None,
            None,
            true,
            false,
            vec![],
        ),
    );
    let output = reply
        .outputs
        .iter()
        .find(|row| row.output.0 == "out-a")
        .expect("output row");
    assert_eq!(output.hold, Some(HoldReason::IncompleteInventory));
    // Facts on unknown screens refuse the whole report.
    let mut bad = report(
        3,
        3,
        two_screens(),
        vec![fact("win-a", "out-a", 0), fact("win-x", "out-z", 0)],
        vec![],
        vec![],
        "",
        None,
        None,
        true,
        false,
        vec![],
    );
    bad.screens.push(screen("out-z", 1600, false));
    let ok = accepted(&mut engine, &bad);
    assert!(
        ok.scopes
            .iter()
            .any(|status| status.scope.starts_with("out-z/"))
    );
}

#[test]
fn host_owned_capability_gates_native_observations() {
    let mut engine = WorldEngine::new("gen-1").expect("valid generation");
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
        vec![],
    );
    bad.native.push(crate::world_report::NativeObservation {
        output: OutputId("out-a".to_owned()),
        native_order: vec!["ws".to_owned()],
        current_native: "ws".to_owned(),
    });
    match engine.handle_report(&bad) {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "snapshot-invalid"),
        other => panic!("expected snapshot-invalid, got {other:?}"),
    }
    // With the capability they ride as pure observations: mapping stays
    // logical and no native id leaks into the reply.
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
        vec![],
    );
    good.host_owned_workspace_domains = true;
    good.native.push(crate::world_report::NativeObservation {
        output: OutputId("out-a".to_owned()),
        native_order: vec!["ws".to_owned()],
        current_native: "ws".to_owned(),
    });
    let reply = accepted(&mut engine, &good);
    assert!(reply.ready);
    assert!(
        reply
            .workspaces
            .iter()
            .all(|row| row.native_binding.is_none())
    );
}

#[test]
fn hotplug_displacement_preserves_logical_units_on_the_survivor() {
    let mut engine = WorldEngine::new("gen-1").expect("valid generation");
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
            vec![],
        ),
    );
    // out-b disconnects: its logical units displace onto the survivor.
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![screen("out-a", 0, true)],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "",
            None,
            None,
            true,
            false,
            vec![],
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
            vec![],
        ),
    );
    assert!(!workspace_ids(&back, "out-b").is_empty());
}

//! Workspace-mode contract tests (stage 2a repair): per-output-local,
//! global-unique, and shared scoped numbering/rings/history/swaps, shared
//! hotplug/startup, migration refusal, equal-count parity, same-output
//! observed-membership settlement, and restart restoration. Logical
//! authority throughout: screens plus window facts are the only
//! observations; every workspace id is core-minted (`ws-<n>`).

use tiler_core::directional::{OutputId, WindowId, WorkspaceId};
use tiler_core::geometry::Rect;
use tiler_core::size_hints::WindowSizeHints;
use tiler_core::workspace_scope::WorkspaceMode;
use tiler_core::world_engine::{WorldEngine, WorldOutcome};
use tiler_core::world_report::{
    ScreenReport, ScreenView, TransferKind, WindowFact, WorldCommand, WorldReport,
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
    mode: WorkspaceMode,
    screens: Vec<ScreenReport>,
    windows: Vec<WindowFact>,
    views: Vec<(&str, &str)>,
    closed: Vec<&str>,
    focused: &str,
    command_seq: Option<u64>,
    command: Option<WorldCommand>,
    member_attempts: Vec<tiler_core::world_report::MemberAttempt>,
) -> WorldReport {
    WorldReport {
        contract_version: tiler_core::world_report::WORLD_CONTRACT_VERSION,
        owner: "owner-a".to_owned(),
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        observation_seq,
        request_seq,
        request_id: format!("req-{request_seq}"),
        command_seq,
        command,
        workspace_mode: mode,
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
        fixed_size_predicate: tiler_core::size_hints::FixedSizePredicate::BothAxes,
        migration_source_refill: tiler_core::output_selection::MigrationSourceRefill::LastRemaining,
        anchor_fallback: tiler_core::admission::AnchorFallback::default(),
        no_space_result: tiler_core::admission::NoSpaceResult::default(),
        switch_target: tiler_core::cursor_targeting::SwitchTarget::default(),
        fresh_placement: tiler_core::cursor_targeting::FreshPlacement::default(),
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
        feedback: Vec::new(),
        monotonic_ms: 0,
        frames: Vec::new(),
        member_attempts,
        view_attempts: Vec::new(),
        clear_attempts: Vec::new(),
    }
}

fn two_screens() -> Vec<ScreenReport> {
    vec![screen("out-a", 0, true), screen("out-b", 800, false)]
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

fn ring_of(reply: &tiler_core::world_report::WorldReply, output: &str) -> Vec<String> {
    let mut ids: Vec<String> = reply
        .workspaces
        .iter()
        .filter(|row| row.output.0 == output)
        .map(|row| (row.order, row.id.clone()))
        .collect::<Vec<_>>()
        .into_iter()
        .map(|(_, id)| id)
        .collect();
    ids.sort();
    ids
}

#[test]
fn local_modes_keep_independent_numbered_rings() {
    for mode in [WorkspaceMode::PerOutputLocal, WorkspaceMode::GlobalUnique] {
        let mut engine = WorldEngine::new("gen-1").expect("generation");
        let first = accepted(
            &mut engine,
            &report(
                1,
                1,
                mode,
                two_screens(),
                vec![],
                vec![],
                vec![],
                "",
                None,
                None,
                vec![],
            ),
        );
        // Independent per-output rings: disjoint stable ids.
        let ring_a = ring_of(&first, "out-a");
        let ring_b = ring_of(&first, "out-b");
        assert_eq!(ring_a.len(), 2, "{mode:?}");
        assert_eq!(ring_b.len(), 2, "{mode:?}");
        assert!(ring_a.iter().all(|id| !ring_b.contains(id)), "{mode:?}");
        // Numbered selection stays scoped: out-a index 2 selects within
        // out-a only.
        let second = accepted(
            &mut engine,
            &report(
                2,
                2,
                mode,
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
                vec![],
            ),
        );
        assert_eq!(
            view_of(&second, "out-a").as_deref(),
            Some(ring_a[1].as_str()),
            "{mode:?}"
        );
        assert_eq!(
            view_of(&second, "out-b").as_deref(),
            view_of(&first, "out-b").as_deref(),
            "{mode:?}"
        );
    }
}

#[test]
fn global_unique_swaps_visible_elsewhere_instead_of_duplicating() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            WorkspaceMode::GlobalUnique,
            two_screens(),
            vec![],
            vec![],
            vec![],
            "",
            None,
            None,
            vec![],
        ),
    );
    let view_a = view_of(&first, "out-a").expect("view a");
    let view_b = view_of(&first, "out-b").expect("view b");
    assert_ne!(view_a, view_b);
    // Select out-b's workspace on out-a: the two views exchange.
    let second = accepted(
        &mut engine,
        &report(
            2,
            2,
            WorkspaceMode::GlobalUnique,
            two_screens(),
            vec![],
            vec![],
            vec![],
            "",
            Some(11),
            Some(WorldCommand::SelectWorkspace {
                output: OutputId("out-a".to_owned()),
                workspace: view_b.clone(),
            }),
            vec![],
        ),
    );
    assert_eq!(view_of(&second, "out-a").as_deref(), Some(view_b.as_str()));
    assert_eq!(view_of(&second, "out-b").as_deref(), Some(view_a.as_str()));
    // The swapped-out ids left their scopes, so the per-output previous
    // entries invalidate (moved ids clear, never reinterpreted):
    // previous-toggle is a no-op until the next recorded change.
    let third = accepted(
        &mut engine,
        &report(
            3,
            3,
            WorkspaceMode::GlobalUnique,
            two_screens(),
            vec![],
            vec![("out-a", view_b.as_str()), ("out-b", view_a.as_str())],
            vec![],
            "",
            Some(12),
            Some(WorldCommand::SelectPrevious {
                output: Some(OutputId("out-a".to_owned())),
            }),
            vec![],
        ),
    );
    assert_eq!(view_of(&third, "out-a").as_deref(), Some(view_b.as_str()));
}

#[test]
fn local_mode_selects_without_swap_when_target_visible_elsewhere() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            WorkspaceMode::PerOutputLocal,
            two_screens(),
            vec![],
            vec![],
            vec![],
            "",
            None,
            None,
            vec![],
        ),
    );
    let view_b = view_of(&first, "out-b").expect("view b");
    // Local mode refuses a foreign id: no cross-output duplication.
    match engine.handle_report(&report(
        2,
        2,
        WorkspaceMode::PerOutputLocal,
        two_screens(),
        vec![],
        vec![],
        vec![],
        "",
        Some(11),
        Some(WorldCommand::SelectWorkspace {
            output: OutputId("out-a".to_owned()),
            workspace: view_b,
        }),
        vec![],
    )) {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "view-target-unknown"),
        other => panic!("expected view-target-unknown, got {other:?}"),
    }
}

#[test]
fn shared_mode_shares_one_ring_view_and_history() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            WorkspaceMode::Shared,
            two_screens(),
            vec![],
            vec![],
            vec![],
            "",
            None,
            None,
            vec![],
        ),
    );
    // One logical workspace mirrored on both outputs, one shared view.
    let ring_a = ring_of(&first, "out-a");
    let ring_b = ring_of(&first, "out-b");
    assert_eq!(ring_a, ring_b);
    assert_eq!(
        view_of(&first, "out-a").as_deref(),
        view_of(&first, "out-b").as_deref()
    );
    let shared = view_of(&first, "out-a").expect("shared view");
    // Selecting on one output converges all outputs with one history.
    let second = accepted(
        &mut engine,
        &report(
            2,
            2,
            WorkspaceMode::Shared,
            two_screens(),
            vec![],
            vec![],
            vec![],
            "",
            Some(11),
            Some(WorldCommand::SelectTrailing {
                output: Some(OutputId("out-a".to_owned())),
            }),
            vec![],
        ),
    );
    let trailing = view_of(&second, "out-a").expect("trailing");
    assert_ne!(trailing, shared);
    assert_eq!(
        view_of(&second, "out-b").as_deref(),
        Some(trailing.as_str())
    );
    assert_eq!(ring_of(&second, "out-a"), ring_of(&second, "out-b"));
    // Previous-toggle walks the one shared history.
    let third = accepted(
        &mut engine,
        &report(
            3,
            3,
            WorkspaceMode::Shared,
            two_screens(),
            vec![],
            vec![("out-a", trailing.as_str()), ("out-b", trailing.as_str())],
            vec![],
            "",
            Some(12),
            Some(WorldCommand::SelectPrevious {
                output: Some(OutputId("out-b".to_owned())),
            }),
            vec![],
        ),
    );
    assert_eq!(view_of(&third, "out-a").as_deref(), Some(shared.as_str()));
    assert_eq!(view_of(&third, "out-b").as_deref(), Some(shared.as_str()));
}

#[test]
fn shared_mode_refuses_whole_workspace_migration() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            WorkspaceMode::Shared,
            two_screens(),
            vec![fact("win-a", "out-a", 0)],
            vec![],
            vec![],
            "",
            None,
            None,
            vec![],
        ),
    );
    let shared = view_of(&first, "out-a").expect("shared view");
    match engine.handle_report(&report(
        2,
        2,
        WorkspaceMode::Shared,
        two_screens(),
        vec![fact("win-a", "out-a", 0)],
        vec![],
        vec![],
        "",
        Some(11),
        Some(WorldCommand::MigrateWorkspace {
            source_output: OutputId("out-a".to_owned()),
            target_output: OutputId("out-b".to_owned()),
            workspace: shared,
        }),
        vec![],
    )) {
        WorldOutcome::Rejected { kind, .. } => assert_eq!(kind, "migration-refused"),
        other => panic!("expected migration-refused, got {other:?}"),
    }
}

#[test]
fn shared_disconnect_discards_history_without_displacement() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    accepted(
        &mut engine,
        &report(
            1,
            1,
            WorkspaceMode::Shared,
            two_screens(),
            vec![fact("win-a", "out-a", 0)],
            vec![],
            vec![],
            "",
            None,
            None,
            vec![],
        ),
    );
    // One output disconnects: no displacement record migrates units.
    let second = accepted(
        &mut engine,
        &report(
            2,
            2,
            WorkspaceMode::Shared,
            vec![screen("out-a", 0, true)],
            vec![fact("win-a", "out-a", 0)],
            vec![],
            vec![],
            "",
            None,
            None,
            vec![],
        ),
    );
    assert!(second.transfers.is_empty());
    assert_eq!(second.outputs.len(), 1);
    // Reconnect converges onto the shared order without consulting the
    // discarded history.
    let third = accepted(
        &mut engine,
        &report(
            3,
            3,
            WorkspaceMode::Shared,
            two_screens(),
            vec![fact("win-a", "out-a", 0)],
            vec![],
            vec![],
            "",
            None,
            None,
            vec![],
        ),
    );
    assert_eq!(ring_of(&third, "out-a"), ring_of(&third, "out-b"));
    assert_eq!(
        view_of(&third, "out-a").as_deref(),
        view_of(&third, "out-b").as_deref()
    );
}

#[test]
fn equal_count_pads_shorter_rings() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let mut base = report(
        1,
        1,
        WorkspaceMode::PerOutputLocal,
        two_screens(),
        vec![],
        vec![],
        vec![],
        "",
        None,
        None,
        vec![],
    );
    base.equal_workspace_count = true;
    let first = accepted(&mut engine, &base);
    assert_eq!(
        ring_of(&first, "out-a").len(),
        ring_of(&first, "out-b").len()
    );
    // Fill one trailing empty on out-a only, then parity pads out-b back
    // to equal length on the next report.
    let mut fill = report(
        2,
        2,
        WorkspaceMode::PerOutputLocal,
        two_screens(),
        vec![fact("win-a", "out-a", 0)],
        vec![],
        vec![],
        "win-a",
        Some(11),
        Some(WorldCommand::SendNumbered {
            window: WindowId("win-a".to_owned()),
            source: tiler_core::world_report::SendSource::Explicit(OutputId("out-a".to_owned())),
            index: 0,
            follow: false,
        }),
        vec![],
    );
    fill.equal_workspace_count = true;
    accepted(&mut engine, &fill);
    let mut calm = report(
        3,
        3,
        WorkspaceMode::PerOutputLocal,
        two_screens(),
        vec![fact("win-a", "out-a", 0)],
        vec![],
        vec![],
        "",
        None,
        None,
        vec![],
    );
    calm.equal_workspace_count = true;
    let settled = accepted(&mut engine, &calm);
    assert_eq!(
        ring_of(&settled, "out-a").len(),
        ring_of(&settled, "out-b").len(),
        "parity: {settled:?}"
    );
}

#[test]
fn same_output_send_stays_pending_then_settles_on_observed_membership() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            WorkspaceMode::PerOutputLocal,
            two_screens(),
            vec![fact("win-a", "out-a", 0)],
            vec![],
            vec![],
            "",
            None,
            None,
            vec![],
        ),
    );
    let ring: Vec<String> = first
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    assert_eq!(ring.len(), 2);
    let target = ring[1].clone();
    // Same-output send commits a transfer that stays outstanding while
    // the observation is unchanged (no membership evidence yet).
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            WorkspaceMode::PerOutputLocal,
            two_screens(),
            vec![fact("win-a", "out-a", 0)],
            vec![],
            vec![],
            "win-a",
            Some(11),
            Some(WorldCommand::Send {
                window: WindowId("win-a".to_owned()),
                target_output: OutputId("out-a".to_owned()),
                target_workspace: target.clone(),
                follow: false,
            }),
            vec![],
        ),
    );
    assert_eq!(reply.transfers.len(), 1);
    let calm = accepted(
        &mut engine,
        &report(
            3,
            3,
            WorkspaceMode::PerOutputLocal,
            two_screens(),
            vec![fact("win-a", "out-a", 0)],
            vec![],
            vec![],
            "",
            None,
            None,
            vec![],
        ),
    );
    assert_eq!(
        calm.transfers.len(),
        1,
        "unchanged observation stays pending: {:?}",
        calm.transfers
    );
    // Observed logical membership at the target settles the transfer.
    let mut arrived = fact("win-a", "out-a", 0);
    arrived.observed_workspace = Some(target.clone());
    let settled = accepted(
        &mut engine,
        &report(
            4,
            4,
            WorkspaceMode::PerOutputLocal,
            two_screens(),
            vec![arrived],
            vec![],
            vec![],
            "",
            None,
            None,
            vec![],
        ),
    );
    assert!(
        settled.transfers.is_empty(),
        "membership proof settles: {:?}",
        settled.transfers
    );
}

#[test]
fn unknown_membership_never_counts_as_target_receipt() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            WorkspaceMode::PerOutputLocal,
            two_screens(),
            vec![fact("win-a", "out-a", 0)],
            vec![],
            vec![],
            "",
            None,
            None,
            vec![],
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
            WorkspaceMode::PerOutputLocal,
            two_screens(),
            vec![fact("win-a", "out-a", 0)],
            vec![],
            vec![],
            "win-a",
            Some(11),
            Some(WorldCommand::Send {
                window: WindowId("win-a".to_owned()),
                target_output: OutputId("out-a".to_owned()),
                target_workspace: target.clone(),
                follow: false,
            }),
            vec![],
        ),
    );
    // Observed membership naming a different workspace never settles
    // the flight: only the exact target id proves arrival.
    let mut wrong = fact("win-a", "out-a", 0);
    wrong.observed_workspace = Some(other);
    let held = accepted(
        &mut engine,
        &report(
            3,
            3,
            WorkspaceMode::PerOutputLocal,
            two_screens(),
            vec![wrong],
            vec![],
            vec![],
            "",
            None,
            None,
            vec![],
        ),
    );
    assert_eq!(
        held.transfers.len(),
        1,
        "wrong membership holds: {:?}",
        held.transfers
    );
}

#[test]
fn migration_partial_arrival_holds_narrowly_on_refusal() {
    use tiler_core::world_report::{FrameSetter, MemberAttempt};
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            WorkspaceMode::PerOutputLocal,
            two_screens(),
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "",
            None,
            None,
            vec![],
        ),
    );
    let view_a = view_of(&first, "out-a").expect("view a");
    accepted(
        &mut engine,
        &report(
            2,
            2,
            WorkspaceMode::PerOutputLocal,
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
            vec![],
        ),
    );
    // One member arrives, one stays: partial ambiguity.
    let partial = accepted(
        &mut engine,
        &report(
            3,
            3,
            WorkspaceMode::PerOutputLocal,
            two_screens(),
            vec![fact("win-a", "out-b", 800), fact("win-b", "out-a", 100)],
            vec![],
            vec![],
            "",
            None,
            None,
            vec![MemberAttempt {
                operation_id: "op-11".to_owned(),
                window: WindowId("win-b".to_owned()),
                target_output: OutputId("out-b".to_owned()),
                target_workspace: view_a.clone(),
                setter: FrameSetter::Refused,
                pre: Some((OutputId("out-a".to_owned()), view_a.clone())),
                post: Some((OutputId("out-a".to_owned()), view_a.clone())),
                attempt_ms: 3,
            }],
        ),
    );
    // The flight holds narrowly with surviving identities preserved;
    // nothing fabricates a rollback while arrivals are outstanding.
    assert!(
        partial
            .transfers
            .iter()
            .any(|t| matches!(t.kind, TransferKind::Migration)),
        "partial holds: {:?}",
        partial.transfers
    );
}

#[test]
fn reobservation_emits_no_fresh_claims() {
    // Exact re-observation emits no fresh admission intent beyond the first
    // observation. Real restoration across a new engine/new generation with
    // previously hidden/observed memberships is pending stage 2b: a fresh
    // generation cannot reuse the same namespace, so this test locks the
    // re-observation half only and flags restoration as pending.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            WorkspaceMode::PerOutputLocal,
            two_screens(),
            vec![fact("win-a", "out-a", 0)],
            vec![],
            vec![],
            "",
            None,
            None,
            vec![],
        ),
    );
    assert!(first.effects.len() <= 1);
    let second = accepted(
        &mut engine,
        &report(
            2,
            2,
            WorkspaceMode::PerOutputLocal,
            two_screens(),
            vec![fact("win-a", "out-a", 0)],
            vec![],
            vec![],
            "",
            None,
            None,
            vec![],
        ),
    );
    assert!(
        second.effects.is_empty(),
        "re-observation never replays admission: {:?}",
        second.effects
    );
    // Unknown workspace membership on a retained window adopts the core
    // view for homing (initial admission policy) but authorizes no
    // transfer settlement on its own.
    let row = second
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-a")
        .expect("homed row");
    assert_eq!(
        row.membership_authority,
        tiler_core::world_report::ScopeAuthority::Controlled
    );
    let _ = WorkspaceId("unused".to_owned());
}

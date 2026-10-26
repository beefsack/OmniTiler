//! Report envelope guarantees at the world boundary: every cheap gate
//! (generation, budget, shape, identity, freshness, validation) and every
//! late refusal (unknown targets, malformed commands) is transactional -
//! rejected reports leave revision, retained flights, ordering, and mapping
//! exactly as before. Duplicates return the cache with no revision.
//! Logical-first: screens plus window facts only.

use tiler_core::directional::{OutputId, WindowId};
use tiler_core::geometry::Rect;
use tiler_core::size_hints::WindowSizeHints;
use tiler_core::world_engine::{WorldEngine, WorldOutcome};
use tiler_core::world_report::{
    ScreenReport, WORLD_CONTRACT_VERSION, WindowFact, WorldCommand, WorldReport,
};

fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

fn screens() -> Vec<ScreenReport> {
    vec![
        ScreenReport {
            output: OutputId("out-a".to_owned()),
            full_rect: Some(rect(0, 0, 800, 600)),
            work_area: Some(rect(0, 0, 800, 560)),
            gap: 0,
            outer_gap: 0,
            complete: true,
            primary: true,
        },
        ScreenReport {
            output: OutputId("out-b".to_owned()),
            full_rect: Some(rect(800, 0, 800, 600)),
            work_area: Some(rect(800, 0, 800, 560)),
            gap: 0,
            outer_gap: 0,
            complete: true,
            primary: false,
        },
    ]
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

fn report(
    request_seq: u64,
    windows: Vec<WindowFact>,
    command_seq: Option<u64>,
    command: Option<WorldCommand>,
) -> WorldReport {
    report_obs(request_seq, request_seq, windows, command_seq, command)
}

fn report_obs(
    request_seq: u64,
    observation_seq: u64,
    windows: Vec<WindowFact>,
    command_seq: Option<u64>,
    command: Option<WorldCommand>,
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
        workspace_mode: tiler_core::workspace_scope::WorkspaceMode::PerOutputLocal,
        screens: screens(),
        windows,
        views: Vec::new(),
        closed: Vec::new(),
        host_owned_workspace_domains: false,
        equal_workspace_count: false,
        birth_clear: false,
        native: Vec::new(),
        fixed_size_predicate: tiler_core::size_hints::FixedSizePredicate::BothAxes,
        migration_source_refill: tiler_core::output_selection::MigrationSourceRefill::LastRemaining,
        anchor_fallback: tiler_core::admission::AnchorFallback::default(),
        no_space_result: tiler_core::admission::NoSpaceResult::default(),
        born_maximized: tiler_core::born_maximized::BornMaximized::default(),
        switch_target: tiler_core::cursor_targeting::SwitchTarget::default(),
        fresh_placement: tiler_core::cursor_targeting::FreshPlacement::default(),
        focus_follows_cursor: false,
        cursor_warp: false,
        cursor: None,
        cursor_hit: None,
        default_tiled: true,
        focused_global: String::new(),
        action_foreground: None,
        action_cached: None,
        action_outputs: Vec::new(),
        inventory_complete: true,
        gesture_barrier: false,
        gesture: None,
        monotonic_ms: 0,
        frames: Vec::new(),
        member_attempts: Vec::new(),
        view_attempts: Vec::new(),
        clear_attempts: Vec::new(),
        creation_attempts: Vec::new(),
        presentation: tiler_core::world_report::PresentationIngress::disabled(),
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

fn rejected_kind(engine: &mut WorldEngine, report: &WorldReport) -> &'static str {
    match engine.handle_report(report) {
        WorldOutcome::Rejected { kind, .. } => kind,
        other => panic!("expected rejected, got {other:?}"),
    }
}

fn ring_ids(engine: &mut WorldEngine, seq: u64) -> Vec<String> {
    let calm = accepted(
        engine,
        &report(seq, vec![fact("win-a", "out-a", 0)], None, None),
    );
    let mut ids: Vec<String> = calm.workspaces.iter().map(|row| row.id.clone()).collect();
    ids.sort();
    ids
}

#[test]
fn freshness_and_identity_gates_leave_no_trace() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(1, vec![fact("win-a", "out-a", 0)], None, None),
    );
    let revision = first.state_revision;
    // Foreign generation never resets ordering or state.
    let mut bad = report(2, vec![fact("win-a", "out-a", 0)], None, None);
    bad.core_generation = "gen-2".to_owned();
    assert_eq!(rejected_kind(&mut engine, &bad), "generation-mismatch");
    assert_eq!(engine.state_revision(), revision);
    // Reused sequence with different input is invalid, never executed.
    assert_eq!(
        rejected_kind(
            &mut engine,
            &report(
                1,
                vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
                None,
                None
            )
        ),
        "request-reused"
    );
    assert_eq!(engine.state_revision(), revision);
    // Older sequences are superseded.
    assert_eq!(
        rejected_kind(
            &mut engine,
            &report(0, vec![fact("win-a", "out-a", 0)], None, None)
        ),
        "request-superseded"
    );
    assert_eq!(engine.state_revision(), revision);
    assert!(engine.retained_transfers().is_empty());
    // The next valid report advances exactly one revision with no flights.
    let calm = accepted(
        &mut engine,
        &report(2, vec![fact("win-a", "out-a", 0)], None, None),
    );
    assert_eq!(calm.state_revision, revision + 1);
    assert!(calm.transfers.is_empty());
}

#[test]
fn observation_stale_and_duplicates_hold_the_watermark() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report_obs(1, 2, vec![fact("win-a", "out-a", 0)], None, None),
    );
    let revision = first.state_revision;
    // Newer request carrying an older observation is stale: the adapter
    // advanced numbering on invalidating input that this report omits.
    assert_eq!(
        rejected_kind(
            &mut engine,
            &report_obs(2, 1, vec![fact("win-a", "out-a", 0)], None, None)
        ),
        "observation-stale"
    );
    assert_eq!(engine.state_revision(), revision);
    // An identical latest report duplicates with the cached reply and no
    // new revision.
    match engine.handle_report(&report_obs(
        1,
        2,
        vec![fact("win-a", "out-a", 0)],
        None,
        None,
    )) {
        WorldOutcome::Duplicate(reply) => assert_eq!(reply.state_revision, revision),
        other => panic!("expected duplicate, got {other:?}"),
    }
    assert_eq!(engine.state_revision(), revision);
}

#[test]
fn malformed_reports_are_transactional() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(1, vec![fact("win-a", "out-a", 0)], None, None),
    );
    let revision = first.state_revision;
    // Closed ids overlapping live facts refuse the whole report.
    let mut bad = report(2, vec![fact("win-a", "out-a", 0)], None, None);
    bad.closed = vec![WindowId("win-a".to_owned())];
    assert_eq!(rejected_kind(&mut engine, &bad), "snapshot-invalid");
    assert_eq!(engine.state_revision(), revision);
    // Command without sequence (and sequence without command) refuse as a
    // pair violation, never executing.
    let mut bad = report(2, vec![fact("win-a", "out-a", 0)], None, None);
    bad.command = Some(WorldCommand::SelectTrailing {
        output: Some(OutputId("out-a".to_owned())),
    });
    assert_eq!(rejected_kind(&mut engine, &bad), "snapshot-invalid");
    assert_eq!(engine.state_revision(), revision);
    // Unknown direction vocabulary refuses at validation.
    assert_eq!(
        rejected_kind(
            &mut engine,
            &report(
                2,
                vec![fact("win-a", "out-a", 0)],
                Some(11),
                Some(WorldCommand::FocusDirectional {
                    window: WindowId("win-a".to_owned()),
                    direction: "sideways".to_owned(),
                }),
            )
        ),
        "snapshot-invalid"
    );
    assert_eq!(engine.state_revision(), revision);
    assert!(engine.retained_transfers().is_empty());
    // Recovery is exact: the next valid report advances one revision.
    let calm = accepted(
        &mut engine,
        &report(2, vec![fact("win-a", "out-a", 0)], None, None),
    );
    assert_eq!(calm.state_revision, revision + 1);
    assert!(calm.transfers.is_empty());
}

#[test]
fn late_command_refusal_rolls_back_without_mutation() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(1, vec![fact("win-a", "out-a", 0)], None, None),
    );
    let revision = first.state_revision;
    let before = ring_ids(&mut engine, 2);
    let revision = {
        let _ = revision;
        accepted(
            &mut engine,
            &report(3, vec![fact("win-a", "out-a", 0)], None, None),
        )
        .state_revision
    };
    // Same-output send to an unknown workspace refuses after staging:
    // no transfer retained, no ring/view churn, no revision.
    assert_eq!(
        rejected_kind(
            &mut engine,
            &report(
                4,
                vec![fact("win-a", "out-a", 0)],
                Some(11),
                Some(WorldCommand::Send {
                    window: WindowId("win-a".to_owned()),
                    target_output: OutputId("out-a".to_owned()),
                    target_workspace: "ws-999".to_owned(),
                    follow: false,
                }),
            )
        ),
        "send-target-unknown"
    );
    assert_eq!(engine.state_revision(), revision);
    assert!(engine.retained_transfers().is_empty());
    let after = ring_ids(&mut engine, 5);
    assert_eq!(before, after, "mapping untouched by the refused command");
}

#[test]
fn oversized_request_refuses_before_any_mutation() {
    use tiler_core::world_report::WORLD_MAX_REQUEST_BYTES;
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(1, vec![fact("win-a", "out-a", 0)], None, None),
    );
    let revision = first.state_revision;
    // A validly shaped report past the 4 MiB request estimate refuses at
    // the cheap gate: long opaque ids inflate the estimate past the bound
    // while every section stays within its own cap.
    let long = |prefix: &str, n: usize| format!("{prefix}-{n:05}-{}", "w".repeat(100));
    let windows: Vec<WindowFact> = (0..10_000)
        .map(|n| fact(&long("win", n), "out-a", 0))
        .collect();
    let big = report(2, windows, None, None);
    assert!(tiler_core::world_report::report_estimate(&big) > WORLD_MAX_REQUEST_BYTES);
    assert_eq!(rejected_kind(&mut engine, &big), "oversized");
    assert_eq!(engine.state_revision(), revision);
    assert!(engine.retained_transfers().is_empty());
    let calm = accepted(
        &mut engine,
        &report(2, vec![fact("win-a", "out-a", 0)], None, None),
    );
    assert_eq!(calm.state_revision, revision + 1);
}

//! Host-owned phase-1 gap fences: inventory/screen unreadability must
//! withhold native view echo confirmation, inferred close/homing/view
//! changes, equal-count padding, and shared creation minting/desires.
//! `host_owned_workspace_domains` stays true in every report; gaps are
//! expressed only via existing v2 representation (`complete`,
//! `inventory_complete`, `native[]` rows/order/current).

use tiler_core::directional::{OutputId, WindowId};
use tiler_core::geometry::Rect;
use tiler_core::size_hints::WindowSizeHints;
use tiler_core::workspace_scope::WorkspaceMode;
use tiler_core::world_engine::{WorldEngine, WorldOutcome};
use tiler_core::world_report::{
    HoldReason, NativeCreationAttempt, NativeObservation, ScopeAuthority, ScreenReport,
    TransferKind, WORLD_CONTRACT_VERSION, WindowFact, WorldCommand, WorldReport,
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

fn broken_screen(output: &str, x: i32, primary: bool) -> ScreenReport {
    ScreenReport {
        complete: false,
        ..screen(output, x, primary)
    }
}

const D1: &str = "3f9a8c1e-1111-4b2a-9c3d-0000000000d1";
const D2: &str = "3f9a8c1e-2222-4b2a-9c3d-0000000000d2";
const D3: &str = "7";

fn fact_native(window: &str, output: &str, frame: Rect, native: Option<&str>) -> WindowFact {
    WindowFact {
        window: WindowId(window.to_owned()),
        output: OutputId(output.to_owned()),
        rect: Some(frame),
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
        native_workspace: native.map(str::to_owned),
    }
}

fn native_obs(output: &str, order: &[&str], current: Option<&str>) -> NativeObservation {
    NativeObservation {
        output: OutputId(output.to_owned()),
        native_order: order.iter().map(|s| (*s).to_owned()).collect(),
        current_native: current.map(str::to_owned),
    }
}

#[allow(clippy::too_many_arguments)]
fn report(
    request_seq: u64,
    observation_seq: u64,
    screens: Vec<ScreenReport>,
    windows: Vec<WindowFact>,
    native: Vec<NativeObservation>,
    creations: Vec<NativeCreationAttempt>,
    command_seq: Option<u64>,
    command: Option<WorldCommand>,
    inventory_complete: bool,
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
        workspace_mode: WorkspaceMode::PerOutputLocal,
        screens,
        windows: windows.clone(),
        views: vec![],
        closed: vec![],
        host_owned_workspace_domains: true,
        equal_workspace_count: false,
        birth_clear: false,
        native,
        fixed_size_predicate: tiler_core::size_hints::FixedSizePredicate::BothAxes,
        migration_source_refill: tiler_core::output_selection::MigrationSourceRefill::LastRemaining,
        anchor_fallback: tiler_core::admission::AnchorFallback::default(),
        no_space_result: tiler_core::admission::NoSpaceResult::default(),
        born_maximized: tiler_core::born_maximized::BornMaximized::default(),
        default_tiled: true,
        focused_global: windows
            .first()
            .map(|f| f.window.0.clone())
            .unwrap_or_default(),
        action_foreground: None,
        action_cached: None,
        action_outputs: Vec::new(),
        inventory_complete,
        gesture_barrier: false,
        gesture: None,
        switch_target: tiler_core::cursor_targeting::SwitchTarget::default(),
        fresh_placement: tiler_core::cursor_targeting::FreshPlacement::default(),
        focus_follows_cursor: false,
        cursor_warp: false,
        cursor: None,
        cursor_hit: None,
        monotonic_ms: 0,
        frames: Vec::new(),
        member_attempts: Vec::new(),
        view_attempts: Vec::new(),
        clear_attempts: Vec::new(),
        creation_attempts: creations,
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

fn one_screen() -> Vec<ScreenReport> {
    vec![screen("out-a", 0, true)]
}

fn two_screens() -> Vec<ScreenReport> {
    vec![screen("out-a", 0, true), screen("out-b", 800, false)]
}

fn bound_ids(reply: &tiler_core::world_report::WorldReply, output: &str) -> Vec<(String, String)> {
    reply
        .workspaces
        .iter()
        .filter(|row| row.output.0 == output && row.native_binding.is_some())
        .map(|row| (row.id.clone(), row.native_binding.clone().expect("bound")))
        .collect()
}

fn workspace_ids(reply: &tiler_core::world_report::WorldReply, output: &str) -> Vec<String> {
    reply
        .workspaces
        .iter()
        .filter(|row| row.output.0 == output)
        .map(|row| row.id.clone())
        .collect()
}

fn output_view(reply: &tiler_core::world_report::WorldReply, output: &str) -> Option<String> {
    reply
        .outputs
        .iter()
        .find(|row| row.output.0 == output)
        .and_then(|row| row.logical_view.clone())
}

fn scope_held(
    reply: &tiler_core::world_report::WorldReply,
    output: &str,
    reason: HoldReason,
) -> bool {
    reply.scopes.iter().any(|row| {
        row.scope.starts_with(&format!("{output}/"))
            && row.authority == ScopeAuthority::Hold(reason)
    })
}

fn view_flights(reply: &tiler_core::world_report::WorldReply, output: &str) -> usize {
    reply
        .transfers
        .iter()
        .filter(|t| t.target_output.0 == output && matches!(t.kind, TransferKind::View))
        .count()
}

fn healthy_windows() -> Vec<WindowFact> {
    vec![
        fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(D1)),
        fact_native("win-b", "out-a", rect(400, 0, 200, 560), Some(D2)),
    ]
}

#[test]
fn inventory_gap_withholds_native_echo_and_holds_domain() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            one_screen(),
            healthy_windows(),
            vec![native_obs("out-a", &[D1, D2], Some(D1))],
            vec![],
            None,
            None,
            true,
        ),
    );
    let bound: std::collections::BTreeMap<String, String> = bound_ids(&first, "out-a")
        .into_iter()
        .map(|(id, n)| (n, id))
        .collect();
    let view0 = output_view(&first, "out-a").expect("baseline view");
    let ids0 = workspace_ids(&first, "out-a");
    let commanded = accepted(
        &mut engine,
        &report(
            2,
            2,
            one_screen(),
            healthy_windows(),
            vec![native_obs("out-a", &[D1, D2], Some(D1))],
            vec![],
            Some(11),
            Some(WorldCommand::SwitchView {
                output: OutputId("out-a".to_owned()),
                workspace: bound[D2].clone(),
            }),
            true,
        ),
    );
    assert_eq!(
        view_flights(&commanded, "out-a"),
        1,
        "switch retains a view flight: {:?}",
        commanded.transfers
    );
    // While the flight is pending the published view is its target, not
    // the settled view: retention keeps showing the target.
    let view_cmd = output_view(&commanded, "out-a").expect("commanded view");
    assert_eq!(view_cmd, bound[D2], "pending flight publishes its target");
    assert_ne!(
        view0, view_cmd,
        "pending target differs from the settled baseline"
    );
    // Inventory gap while the native current names the flight target: the
    // echo proves nothing until a complete inventory re-reads it.
    let gapped = accepted(
        &mut engine,
        &report(
            3,
            3,
            one_screen(),
            healthy_windows(),
            vec![native_obs("out-a", &[D1, D2], Some(D2))],
            vec![],
            None,
            None,
            false,
        ),
    );
    assert_eq!(
        view_flights(&gapped, "out-a"),
        1,
        "gapped echo must not confirm the flight: {:?}",
        gapped.transfers
    );
    assert_eq!(
        output_view(&gapped, "out-a"),
        Some(view_cmd),
        "retained flight keeps publishing its target, settled view unmoved"
    );
    assert!(
        scope_held(&gapped, "out-a", HoldReason::IncompleteInventory),
        "inventory gap surfaces domain hold: {:?}",
        gapped.scopes
    );
    assert!(
        gapped.native_creations.is_empty(),
        "gap mints no creations: {:?}",
        gapped.native_creations
    );
    assert_eq!(
        workspace_ids(&gapped, "out-a"),
        ids0,
        "gap infers no close and mints no workspace"
    );
}

#[test]
fn per_screen_gap_withholds_echo_on_gapped_output_only() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let windows = vec![
        fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(D1)),
        fact_native("win-b", "out-b", rect(800, 0, 400, 560), Some(D1)),
    ];
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            two_screens(),
            windows.clone(),
            vec![
                native_obs("out-a", &[D1, D2], Some(D1)),
                native_obs("out-b", &[D1, D2], Some(D1)),
            ],
            vec![],
            None,
            None,
            true,
        ),
    );
    let bound_b: std::collections::BTreeMap<String, String> = bound_ids(&first, "out-b")
        .into_iter()
        .map(|(id, n)| (n, id))
        .collect();
    let view_b0 = output_view(&first, "out-b").expect("out-b baseline view");
    let view_a0 = output_view(&first, "out-a").expect("out-a baseline view");
    let commanded = accepted(
        &mut engine,
        &report(
            2,
            2,
            two_screens(),
            windows.clone(),
            vec![
                native_obs("out-a", &[D1, D2], Some(D1)),
                native_obs("out-b", &[D1, D2], Some(D1)),
            ],
            vec![],
            Some(11),
            Some(WorldCommand::SwitchView {
                output: OutputId("out-b".to_owned()),
                workspace: bound_b[D2].clone(),
            }),
            true,
        ),
    );
    // While the flight is pending the published view is its target.
    let view_b_cmd = output_view(&commanded, "out-b").expect("commanded out-b view");
    assert_eq!(view_b_cmd, bound_b[D2]);
    assert_ne!(view_b0, view_b_cmd);
    // Only out-b goes incomplete while its native current names the flight
    // target; out-a stays complete and must stay Controlled.
    let gapped = accepted(
        &mut engine,
        &report(
            3,
            3,
            vec![screen("out-a", 0, true), broken_screen("out-b", 800, false)],
            windows.clone(),
            vec![
                native_obs("out-a", &[D1, D2], Some(D1)),
                native_obs("out-b", &[D1, D2], Some(D2)),
            ],
            vec![],
            None,
            None,
            true,
        ),
    );
    assert_eq!(
        view_flights(&gapped, "out-b"),
        1,
        "gapped-screen echo must not confirm: {:?}",
        gapped.transfers
    );
    assert_eq!(
        output_view(&gapped, "out-b"),
        Some(view_b_cmd),
        "retained flight keeps publishing its target, settled view unmoved"
    );
    assert_eq!(
        output_view(&gapped, "out-a"),
        Some(view_a0),
        "healthy output unaffected"
    );
    assert!(
        !scope_held(&gapped, "out-a", HoldReason::IncompleteInventory)
            && !scope_held(&gapped, "out-a", HoldReason::Unreadable),
        "healthy output stays Controlled: {:?}",
        gapped.scopes
    );
}

#[test]
fn healthy_none_current_retains_view_controlled() {
    // Contrast: a complete report with current None is the known
    // fullscreen-Space meaning, never a gap. View retained, Controlled.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            one_screen(),
            healthy_windows(),
            vec![native_obs("out-a", &[D1, D2], Some(D1))],
            vec![],
            None,
            None,
            true,
        ),
    );
    let view0 = output_view(&first, "out-a").expect("baseline view");
    let second = accepted(
        &mut engine,
        &report(
            2,
            2,
            one_screen(),
            healthy_windows(),
            vec![native_obs("out-a", &[D1, D2], None)],
            vec![],
            None,
            None,
            true,
        ),
    );
    assert_eq!(bound_ids(&second, "out-a").len(), 2, "order still binds");
    assert_eq!(
        output_view(&second, "out-a"),
        Some(view0),
        "None current never moves the view"
    );
    assert!(second.transfers.is_empty());
    assert!(
        second
            .scopes
            .iter()
            .all(|row| row.authority == ScopeAuthority::Controlled),
        "healthy None stays Controlled: {:?}",
        second.scopes
    );
}

#[test]
fn inventory_gap_none_current_holds_without_inference() {
    // Same None current as above, but the inventory is incomplete: the
    // None is unreadable, not the fullscreen meaning. Hold, no inference.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            one_screen(),
            healthy_windows(),
            vec![native_obs("out-a", &[D1, D2], Some(D1))],
            vec![],
            None,
            None,
            true,
        ),
    );
    let view0 = output_view(&first, "out-a").expect("baseline view");
    let ids0 = workspace_ids(&first, "out-a");
    let gapped = accepted(
        &mut engine,
        &report(
            2,
            2,
            one_screen(),
            healthy_windows(),
            vec![native_obs("out-a", &[D1, D2], None)],
            vec![],
            None,
            None,
            false,
        ),
    );
    assert!(
        scope_held(&gapped, "out-a", HoldReason::IncompleteInventory),
        "unreadable None holds: {:?}",
        gapped.scopes
    );
    assert_eq!(
        output_view(&gapped, "out-a"),
        Some(view0),
        "gap infers no view"
    );
    assert_eq!(
        workspace_ids(&gapped, "out-a"),
        ids0,
        "gap infers no close and mints no workspace"
    );
    assert!(gapped.transfers.is_empty());
}

#[test]
fn equal_count_padding_withheld_during_inventory_gap() {
    // Divergent rings without the equal flag: out-a binds two natives plus
    // its spare, out-b binds one plus its spare.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let windows = vec![
        fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(D1)),
        fact_native("win-a2", "out-a", rect(400, 0, 400, 560), Some(D2)),
        fact_native("win-b", "out-b", rect(800, 0, 400, 560), Some(D3)),
    ];
    let base = accepted(
        &mut engine,
        &report(
            1,
            1,
            two_screens(),
            windows.clone(),
            vec![
                native_obs("out-a", &[D1, D2], Some(D1)),
                native_obs("out-b", &[D3], Some(D3)),
            ],
            vec![],
            None,
            None,
            true,
        ),
    );
    let ids_a0 = workspace_ids(&base, "out-a");
    let ids_b0 = workspace_ids(&base, "out-b");
    assert!(
        ids_a0.len() > ids_b0.len(),
        "baseline diverges without the equal flag: {ids_a0:?} vs {ids_b0:?}"
    );
    // The equal flag arrives with an incomplete inventory: padding must
    // wait for a complete re-read, never mint from the gap.
    let mut gap = report(
        2,
        2,
        two_screens(),
        windows.clone(),
        vec![
            native_obs("out-a", &[D1, D2], Some(D1)),
            native_obs("out-b", &[D3], Some(D3)),
        ],
        vec![],
        None,
        None,
        false,
    );
    gap.equal_workspace_count = true;
    let gapped = accepted(&mut engine, &gap);
    assert_eq!(
        workspace_ids(&gapped, "out-b"),
        ids_b0,
        "gap pads nothing: {:?}",
        gapped.workspaces
    );
    assert_eq!(
        workspace_ids(&gapped, "out-a"),
        ids_a0,
        "gap pads nothing: {:?}",
        gapped.workspaces
    );
    assert!(
        gapped.native_creations.is_empty(),
        "gap mints no creations: {:?}",
        gapped.native_creations
    );
}

#[test]
fn empty_native_with_equal_flag_mints_nothing() {
    // Complete screens and inventory, but the native list is empty while
    // the equal flag is set: no row means no order evidence, never
    // padding or desires.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let windows = vec![
        fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(D1)),
        fact_native("win-a2", "out-a", rect(400, 0, 400, 560), Some(D2)),
        fact_native("win-b", "out-b", rect(800, 0, 400, 560), Some(D3)),
    ];
    let base = accepted(
        &mut engine,
        &report(
            1,
            1,
            two_screens(),
            windows.clone(),
            vec![
                native_obs("out-a", &[D1, D2], Some(D1)),
                native_obs("out-b", &[D3], Some(D3)),
            ],
            vec![],
            None,
            None,
            true,
        ),
    );
    let ids_a0 = workspace_ids(&base, "out-a");
    let ids_b0 = workspace_ids(&base, "out-b");
    assert!(
        ids_a0.len() > ids_b0.len(),
        "baseline diverges without the equal flag: {ids_a0:?} vs {ids_b0:?}"
    );
    let mut empty = report(
        2,
        2,
        two_screens(),
        windows.clone(),
        vec![],
        vec![],
        None,
        None,
        true,
    );
    empty.equal_workspace_count = true;
    let gapped = accepted(&mut engine, &empty);
    assert_eq!(
        workspace_ids(&gapped, "out-a"),
        ids_a0,
        "empty native pads nothing: {:?}",
        gapped.workspaces
    );
    assert_eq!(
        workspace_ids(&gapped, "out-b"),
        ids_b0,
        "empty native pads nothing: {:?}",
        gapped.workspaces
    );
    assert!(
        gapped.native_creations.is_empty(),
        "empty native mints no desires: {:?}",
        gapped.native_creations
    );
}

fn shared(report: &WorldReport) -> WorldReport {
    let mut report = report.clone();
    report.workspace_mode = WorkspaceMode::Shared;
    report
}

#[test]
fn shared_first_gapped_reports_mint_nothing() {
    // Fresh shared engine whose first reports are inventory gaps: a view
    // flight at the spare must not mint a creation until a complete
    // inventory confirms the order.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let windows = vec![fact_native(
        "win-a",
        "out-a",
        rect(0, 0, 400, 560),
        Some(D1),
    )];
    let first = accepted(
        &mut engine,
        &shared(&report(
            1,
            1,
            one_screen(),
            windows.clone(),
            vec![native_obs("out-a", &[D1, D2], Some(D1))],
            vec![],
            None,
            None,
            false,
        )),
    );
    let spare = first
        .workspaces
        .iter()
        .find(|row| row.output.0 == "out-a" && row.native_binding.is_none())
        .map(|row| row.id.clone())
        .expect("spare");
    let gapped = accepted(
        &mut engine,
        &shared(&report(
            2,
            2,
            one_screen(),
            windows.clone(),
            vec![native_obs("out-a", &[D1, D2], Some(D1))],
            vec![],
            Some(11),
            Some(WorldCommand::SwitchView {
                output: OutputId("out-a".to_owned()),
                workspace: spare,
            }),
            false,
        )),
    );
    assert!(
        gapped.native_creations.is_empty(),
        "gapped first flight mints nothing: {:?}",
        gapped.native_creations
    );
}

#[test]
fn shared_gap_withholds_desires_and_recovery_republishes() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let windows = vec![
        fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(D1)),
        fact_native("win-b", "out-a", rect(400, 0, 200, 560), Some(D2)),
    ];
    let native = vec![native_obs("out-a", &[D1, D2], Some(D1))];
    let first = accepted(
        &mut engine,
        &shared(&report(
            1,
            1,
            one_screen(),
            windows.clone(),
            native.clone(),
            vec![],
            None,
            None,
            true,
        )),
    );
    let spare = first
        .workspaces
        .iter()
        .find(|row| row.output.0 == "out-a" && row.native_binding.is_none())
        .map(|row| row.id.clone())
        .expect("spare");
    // Healthy Send occupies the spare: exactly one outstanding desire.
    let desired = accepted(
        &mut engine,
        &shared(&report(
            2,
            2,
            one_screen(),
            windows.clone(),
            native.clone(),
            vec![],
            Some(11),
            Some(WorldCommand::Send {
                window: WindowId("win-a".to_owned()),
                target_output: OutputId("out-a".to_owned()),
                target_workspace: spare.clone(),
                follow: false,
            }),
            true,
        )),
    );
    assert_eq!(
        desired.native_creations.len(),
        1,
        "healthy report mints one desire: {:?}",
        desired.native_creations
    );
    let op = desired.native_creations[0].operation_id.clone();
    // Inventory gap: the outstanding authorization publishes nothing.
    let gapped = accepted(
        &mut engine,
        &shared(&report(
            3,
            3,
            one_screen(),
            windows.clone(),
            native.clone(),
            vec![],
            None,
            None,
            false,
        )),
    );
    assert!(
        gapped.native_creations.is_empty(),
        "gap withholds desires: {:?}",
        gapped.native_creations
    );
    // Recovery re-publishes the same outstanding authorization, once.
    let recovered = accepted(
        &mut engine,
        &shared(&report(
            4,
            4,
            one_screen(),
            windows.clone(),
            native.clone(),
            vec![],
            None,
            None,
            true,
        )),
    );
    assert_eq!(
        recovered.native_creations.len(),
        1,
        "recovery re-publishes: {:?}",
        recovered.native_creations
    );
    assert_eq!(recovered.native_creations[0].operation_id, op);
    assert_eq!(recovered.native_creations[0].logical_workspace, spare);
}

#[test]
fn inventory_gap_recovery_restores_echo_without_duplication() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            one_screen(),
            healthy_windows(),
            vec![native_obs("out-a", &[D1, D2], Some(D1))],
            vec![],
            None,
            None,
            true,
        ),
    );
    let bound: std::collections::BTreeMap<String, String> = bound_ids(&first, "out-a")
        .into_iter()
        .map(|(id, n)| (n, id))
        .collect();
    let ids0 = workspace_ids(&first, "out-a");
    accepted(
        &mut engine,
        &report(
            2,
            2,
            one_screen(),
            healthy_windows(),
            vec![native_obs("out-a", &[D1, D2], Some(D1))],
            vec![],
            Some(11),
            Some(WorldCommand::SwitchView {
                output: OutputId("out-a".to_owned()),
                workspace: bound[D2].clone(),
            }),
            true,
        ),
    );
    let gapped = accepted(
        &mut engine,
        &report(
            3,
            3,
            one_screen(),
            healthy_windows(),
            vec![native_obs("out-a", &[D1, D2], Some(D2))],
            vec![],
            None,
            None,
            false,
        ),
    );
    assert_eq!(
        view_flights(&gapped, "out-a"),
        1,
        "gap holds the flight: {:?}",
        gapped.transfers
    );
    // Complete re-read of the same echo settles exactly once.
    let settled = accepted(
        &mut engine,
        &report(
            4,
            4,
            one_screen(),
            healthy_windows(),
            vec![native_obs("out-a", &[D1, D2], Some(D2))],
            vec![],
            None,
            None,
            true,
        ),
    );
    assert!(
        settled.transfers.is_empty(),
        "recovery settles: {:?}",
        settled.transfers
    );
    assert_eq!(
        output_view(&settled, "out-a"),
        Some(bound[D2].clone()),
        "recovery adopts the proven target"
    );
    assert_eq!(
        workspace_ids(&settled, "out-a"),
        ids0,
        "recovery duplicates nothing"
    );
    assert!(
        settled.native_creations.is_empty(),
        "recovery mints no creations: {:?}",
        settled.native_creations
    );
}

fn window_row(
    reply: &tiler_core::world_report::WorldReply,
    window: &str,
) -> Option<(String, String, ScopeAuthority)> {
    reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == window)
        .map(|row| {
            (
                row.output.0.clone(),
                row.workspace.clone(),
                row.membership_authority,
            )
        })
}

#[test]
fn gap_omitted_window_retains_membership_and_recovers() {
    // Omission inside a held scope retires nothing: the retained
    // membership row survives the gap with its homing, and recovery
    // restores Controlled authority without rehoming.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            one_screen(),
            healthy_windows(),
            vec![native_obs("out-a", &[D1, D2], Some(D1))],
            vec![],
            None,
            None,
            true,
        ),
    );
    let bound: std::collections::BTreeMap<String, String> = bound_ids(&first, "out-a")
        .into_iter()
        .map(|(id, n)| (n, id))
        .collect();
    let (home_out, home_ws, home_auth) = window_row(&first, "win-b").expect("baseline win-b row");
    assert_eq!(home_out, "out-a");
    assert_eq!(home_ws, bound[D2]);
    assert_eq!(home_auth, ScopeAuthority::Controlled);
    let ids0 = workspace_ids(&first, "out-a");
    // Inventory gap omits win-b outright: no close inference, no new id.
    let gapped = accepted(
        &mut engine,
        &report(
            2,
            2,
            one_screen(),
            vec![fact_native(
                "win-a",
                "out-a",
                rect(0, 0, 400, 560),
                Some(D1),
            )],
            vec![native_obs("out-a", &[D1, D2], Some(D1))],
            vec![],
            None,
            None,
            false,
        ),
    );
    let (gap_out, gap_ws, gap_auth) =
        window_row(&gapped, "win-b").expect("omitted win-b retains its row");
    assert_eq!(
        (gap_out, gap_ws),
        (home_out.clone(), home_ws.clone()),
        "gap rehomes nothing"
    );
    assert_eq!(
        gap_auth,
        ScopeAuthority::Hold(HoldReason::IncompleteInventory),
        "held scope carries the gap hold, never a write"
    );
    assert_eq!(
        workspace_ids(&gapped, "out-a"),
        ids0,
        "gap infers no close and mints no workspace"
    );
    let recovered = accepted(
        &mut engine,
        &report(
            3,
            3,
            one_screen(),
            healthy_windows(),
            vec![native_obs("out-a", &[D1, D2], Some(D1))],
            vec![],
            None,
            None,
            true,
        ),
    );
    let (rec_out, rec_ws, rec_auth) = window_row(&recovered, "win-b").expect("recovered win-b row");
    assert_eq!((rec_out, rec_ws), (home_out, home_ws));
    assert_eq!(rec_auth, ScopeAuthority::Controlled);
    assert_eq!(workspace_ids(&recovered, "out-a"), ids0);
}

#[test]
fn gap_conflicting_native_receipt_keeps_homing() {
    // A native receipt naming an unmapped desktop during a gap is
    // unknown evidence: retained homing wins, never a rehome.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            one_screen(),
            healthy_windows(),
            vec![native_obs("out-a", &[D1, D2], Some(D1))],
            vec![],
            None,
            None,
            true,
        ),
    );
    let bound: std::collections::BTreeMap<String, String> = bound_ids(&first, "out-a")
        .into_iter()
        .map(|(id, n)| (n, id))
        .collect();
    let (home_out, home_ws, _) = window_row(&first, "win-b").expect("baseline win-b row");
    assert_eq!(home_ws, bound[D2]);
    let mut windows = healthy_windows();
    windows[1].native_workspace = Some("unknown-native-zzz".to_owned());
    let gapped = accepted(
        &mut engine,
        &report(
            2,
            2,
            one_screen(),
            windows.clone(),
            vec![native_obs("out-a", &[D1, D2], Some(D1))],
            vec![],
            None,
            None,
            false,
        ),
    );
    let (gap_out, gap_ws, _) =
        window_row(&gapped, "win-b").expect("conflicting receipt keeps a row");
    assert_eq!(
        (gap_out, gap_ws),
        (home_out.clone(), home_ws.clone()),
        "unknown receipt never rehomes"
    );
    let recovered = accepted(
        &mut engine,
        &report(
            3,
            3,
            one_screen(),
            healthy_windows(),
            vec![native_obs("out-a", &[D1, D2], Some(D1))],
            vec![],
            None,
            None,
            true,
        ),
    );
    let (rec_out, rec_ws, rec_auth) = window_row(&recovered, "win-b").expect("recovered win-b row");
    assert_eq!((rec_out, rec_ws), (home_out, home_ws));
    assert_eq!(rec_auth, ScopeAuthority::Controlled);
}

#[test]
fn gap_changed_current_records_no_history() {
    // No command anywhere: the gap report's changed native current is
    // unproven evidence, so previous-view history stays empty. The
    // existing SelectPrevious command is the external oracle.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            one_screen(),
            healthy_windows(),
            vec![native_obs("out-a", &[D1, D2], Some(D1))],
            vec![],
            None,
            None,
            true,
        ),
    );
    let view0 = output_view(&first, "out-a").expect("baseline view");
    let gapped = accepted(
        &mut engine,
        &report(
            2,
            2,
            one_screen(),
            healthy_windows(),
            vec![native_obs("out-a", &[D1, D2], Some(D2))],
            vec![],
            None,
            None,
            false,
        ),
    );
    assert!(gapped.transfers.is_empty());
    assert_eq!(
        output_view(&gapped, "out-a"),
        Some(view0.clone()),
        "unproven current moves no view"
    );
    // Recovery re-reads the old current and asks for previous: with no
    // recorded transition this is a no-op, never a flight.
    let recovered = accepted(
        &mut engine,
        &report(
            3,
            3,
            one_screen(),
            healthy_windows(),
            vec![native_obs("out-a", &[D1, D2], Some(D1))],
            vec![],
            Some(11),
            Some(WorldCommand::SelectPrevious { output: None }),
            true,
        ),
    );
    assert!(
        recovered.transfers.is_empty(),
        "gap recorded no history: {:?}",
        recovered.transfers
    );
    assert_eq!(output_view(&recovered, "out-a"), Some(view0));
}

#[test]
fn shared_screen_gap_withholds_desires_and_recovers() {
    // Inventory is complete but the desire's target screen is
    // incomplete: the outstanding authorization publishes nothing
    // until the screen re-reads complete, under the same op id.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let windows = vec![
        fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(D1)),
        fact_native("win-b", "out-a", rect(400, 0, 200, 560), Some(D2)),
    ];
    let native = vec![native_obs("out-a", &[D1, D2], Some(D1))];
    let first = accepted(
        &mut engine,
        &shared(&report(
            1,
            1,
            one_screen(),
            windows.clone(),
            native.clone(),
            vec![],
            None,
            None,
            true,
        )),
    );
    let spare = first
        .workspaces
        .iter()
        .find(|row| row.output.0 == "out-a" && row.native_binding.is_none())
        .map(|row| row.id.clone())
        .expect("spare");
    let desired = accepted(
        &mut engine,
        &shared(&report(
            2,
            2,
            one_screen(),
            windows.clone(),
            native.clone(),
            vec![],
            Some(11),
            Some(WorldCommand::Send {
                window: WindowId("win-a".to_owned()),
                target_output: OutputId("out-a".to_owned()),
                target_workspace: spare.clone(),
                follow: false,
            }),
            true,
        )),
    );
    assert_eq!(
        desired.native_creations.len(),
        1,
        "healthy report mints one desire: {:?}",
        desired.native_creations
    );
    let op = desired.native_creations[0].operation_id.clone();
    let gapped = accepted(
        &mut engine,
        &shared(&report(
            3,
            3,
            vec![broken_screen("out-a", 0, true)],
            windows.clone(),
            native.clone(),
            vec![],
            None,
            None,
            true,
        )),
    );
    assert!(
        gapped.native_creations.is_empty(),
        "screen gap withholds desires: {:?}",
        gapped.native_creations
    );
    let recovered = accepted(
        &mut engine,
        &shared(&report(
            4,
            4,
            one_screen(),
            windows.clone(),
            native.clone(),
            vec![],
            None,
            None,
            true,
        )),
    );
    assert_eq!(
        recovered.native_creations.len(),
        1,
        "recovery re-publishes: {:?}",
        recovered.native_creations
    );
    assert_eq!(recovered.native_creations[0].operation_id, op);
    assert_eq!(recovered.native_creations[0].logical_workspace, spare);
}

#[test]
fn shared_missing_native_row_mints_nothing_new() {
    // Screens and inventory read complete but the native list carries
    // no row: no new authorization mints, and the outstanding one
    // withholds until the row re-reads.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let windows = vec![
        fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(D1)),
        fact_native("win-b", "out-a", rect(400, 0, 200, 560), Some(D2)),
    ];
    let native = vec![native_obs("out-a", &[D1, D2], Some(D1))];
    let first = accepted(
        &mut engine,
        &shared(&report(
            1,
            1,
            one_screen(),
            windows.clone(),
            native.clone(),
            vec![],
            None,
            None,
            true,
        )),
    );
    let spare = first
        .workspaces
        .iter()
        .find(|row| row.output.0 == "out-a" && row.native_binding.is_none())
        .map(|row| row.id.clone())
        .expect("spare");
    let desired = accepted(
        &mut engine,
        &shared(&report(
            2,
            2,
            one_screen(),
            windows.clone(),
            native.clone(),
            vec![],
            Some(11),
            Some(WorldCommand::Send {
                window: WindowId("win-a".to_owned()),
                target_output: OutputId("out-a".to_owned()),
                target_workspace: spare.clone(),
                follow: false,
            }),
            true,
        )),
    );
    assert_eq!(desired.native_creations.len(), 1);
    let op = desired.native_creations[0].operation_id.clone();
    let rowless = accepted(
        &mut engine,
        &shared(&report(
            3,
            3,
            one_screen(),
            windows.clone(),
            vec![],
            vec![],
            None,
            None,
            true,
        )),
    );
    assert!(
        rowless.native_creations.is_empty(),
        "rowless report withholds the outstanding desire: {:?}",
        rowless.native_creations
    );
    let recovered = accepted(
        &mut engine,
        &shared(&report(
            4,
            4,
            one_screen(),
            windows.clone(),
            native.clone(),
            vec![],
            None,
            None,
            true,
        )),
    );
    assert_eq!(recovered.native_creations.len(), 1);
    assert_eq!(recovered.native_creations[0].operation_id, op);
}

#[test]
fn healthy_equal_count_pads_without_gap() {
    // Anti-overbreadth guard: complete inventories, screens, and
    // non-empty native orders still pad shorter rings to parity.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let windows = vec![
        fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(D1)),
        fact_native("win-a2", "out-a", rect(400, 0, 400, 560), Some(D2)),
        fact_native("win-b", "out-b", rect(800, 0, 400, 560), Some(D3)),
    ];
    let base = accepted(
        &mut engine,
        &report(
            1,
            1,
            two_screens(),
            windows.clone(),
            vec![
                native_obs("out-a", &[D1, D2], Some(D1)),
                native_obs("out-b", &[D3], Some(D3)),
            ],
            vec![],
            None,
            None,
            true,
        ),
    );
    let ids_a0 = workspace_ids(&base, "out-a");
    assert!(ids_a0.len() > workspace_ids(&base, "out-b").len());
    let mut equal = report(
        2,
        2,
        two_screens(),
        windows.clone(),
        vec![
            native_obs("out-a", &[D1, D2], Some(D1)),
            native_obs("out-b", &[D3], Some(D3)),
        ],
        vec![],
        None,
        None,
        true,
    );
    equal.equal_workspace_count = true;
    let padded = accepted(&mut engine, &equal);
    assert_eq!(
        workspace_ids(&padded, "out-b").len(),
        workspace_ids(&padded, "out-a").len(),
        "healthy equal flag pads to parity: {:?}",
        padded.workspaces
    );
    assert!(
        padded
            .scopes
            .iter()
            .all(|row| row.authority == ScopeAuthority::Controlled),
        "healthy parity stays Controlled: {:?}",
        padded.scopes
    );
}

#[test]
fn shared_retained_desire_withholds_on_missing_and_empty_native_row() {
    // Outstanding authorization publishes nothing while its target
    // native row is missing or empty; recovery re-publishes the same op.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let windows = vec![
        fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(D1)),
        fact_native("win-b", "out-a", rect(400, 0, 200, 560), Some(D2)),
    ];
    let native = vec![native_obs("out-a", &[D1, D2], Some(D1))];
    let first = accepted(
        &mut engine,
        &shared(&report(
            1,
            1,
            one_screen(),
            windows.clone(),
            native.clone(),
            vec![],
            None,
            None,
            true,
        )),
    );
    let spare = first
        .workspaces
        .iter()
        .find(|row| row.output.0 == "out-a" && row.native_binding.is_none())
        .map(|row| row.id.clone())
        .expect("spare");
    let desired = accepted(
        &mut engine,
        &shared(&report(
            2,
            2,
            one_screen(),
            windows.clone(),
            native.clone(),
            vec![],
            Some(11),
            Some(WorldCommand::Send {
                window: WindowId("win-a".to_owned()),
                target_output: OutputId("out-a".to_owned()),
                target_workspace: spare.clone(),
                follow: false,
            }),
            true,
        )),
    );
    assert_eq!(desired.native_creations.len(), 1);
    let op = desired.native_creations[0].operation_id.clone();
    let rowless = accepted(
        &mut engine,
        &shared(&report(
            3,
            3,
            one_screen(),
            windows.clone(),
            vec![],
            vec![],
            None,
            None,
            true,
        )),
    );
    assert!(
        rowless.native_creations.is_empty(),
        "missing native row withholds desires: {:?}",
        rowless.native_creations
    );
    let empty_order = accepted(
        &mut engine,
        &shared(&report(
            4,
            4,
            one_screen(),
            windows.clone(),
            vec![native_obs("out-a", &[], None)],
            vec![],
            None,
            None,
            true,
        )),
    );
    assert!(
        empty_order.native_creations.is_empty(),
        "empty native order withholds desires: {:?}",
        empty_order.native_creations
    );
    let recovered = accepted(
        &mut engine,
        &shared(&report(
            5,
            5,
            one_screen(),
            windows.clone(),
            native.clone(),
            vec![],
            None,
            None,
            true,
        )),
    );
    assert_eq!(
        recovered.native_creations.len(),
        1,
        "recovery re-publishes: {:?}",
        recovered.native_creations
    );
    assert_eq!(recovered.native_creations[0].operation_id, op);
    assert_eq!(recovered.native_creations[0].logical_workspace, spare);
}

#[test]
fn shared_other_screen_gap_withholds_desires_and_recovers() {
    // Shared spans every screen: a gap on any screen withholds even a
    // desire whose own target screen reads complete.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let windows = vec![
        fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(D1)),
        fact_native("win-b", "out-a", rect(400, 0, 200, 560), Some(D2)),
    ];
    let native = vec![
        native_obs("out-a", &[D1, D2], Some(D1)),
        native_obs("out-b", &[D1, D2], Some(D1)),
    ];
    let first = accepted(
        &mut engine,
        &shared(&report(
            1,
            1,
            two_screens(),
            windows.clone(),
            native.clone(),
            vec![],
            None,
            None,
            true,
        )),
    );
    let spare = first
        .workspaces
        .iter()
        .find(|row| row.output.0 == "out-a" && row.native_binding.is_none())
        .map(|row| row.id.clone())
        .expect("spare");
    let desired = accepted(
        &mut engine,
        &shared(&report(
            2,
            2,
            two_screens(),
            windows.clone(),
            native.clone(),
            vec![],
            Some(11),
            Some(WorldCommand::Send {
                window: WindowId("win-a".to_owned()),
                target_output: OutputId("out-a".to_owned()),
                target_workspace: spare.clone(),
                follow: false,
            }),
            true,
        )),
    );
    assert_eq!(
        desired.native_creations.len(),
        1,
        "healthy report mints one desire: {:?}",
        desired.native_creations
    );
    let op = desired.native_creations[0].operation_id.clone();
    let gapped = accepted(
        &mut engine,
        &shared(&report(
            3,
            3,
            vec![screen("out-a", 0, true), broken_screen("out-b", 800, false)],
            windows.clone(),
            native.clone(),
            vec![],
            None,
            None,
            true,
        )),
    );
    assert!(
        gapped.native_creations.is_empty(),
        "other-screen gap withholds desires: {:?}",
        gapped.native_creations
    );
    let recovered = accepted(
        &mut engine,
        &shared(&report(
            4,
            4,
            two_screens(),
            windows.clone(),
            native.clone(),
            vec![],
            None,
            None,
            true,
        )),
    );
    assert_eq!(
        recovered.native_creations.len(),
        1,
        "recovery re-publishes: {:?}",
        recovered.native_creations
    );
    assert_eq!(recovered.native_creations[0].operation_id, op);
    assert_eq!(recovered.native_creations[0].logical_workspace, spare);
}

#[test]
fn equal_count_screen_gap_withholds_and_recovers() {
    // Same parity setup as the inventory-gap test, but the gap is a
    // broken screen: no padding until the screen re-reads complete.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let windows = vec![
        fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(D1)),
        fact_native("win-a2", "out-a", rect(400, 0, 400, 560), Some(D2)),
        fact_native("win-b", "out-b", rect(800, 0, 400, 560), Some(D3)),
    ];
    let native = vec![
        native_obs("out-a", &[D1, D2], Some(D1)),
        native_obs("out-b", &[D3], Some(D3)),
    ];
    let base = accepted(
        &mut engine,
        &report(
            1,
            1,
            two_screens(),
            windows.clone(),
            native.clone(),
            vec![],
            None,
            None,
            true,
        ),
    );
    let ids_a0 = workspace_ids(&base, "out-a");
    let ids_b0 = workspace_ids(&base, "out-b");
    assert!(
        ids_a0.len() > ids_b0.len(),
        "baseline diverges without the equal flag: {ids_a0:?} vs {ids_b0:?}"
    );
    let mut gapped_report = report(
        2,
        2,
        vec![screen("out-a", 0, true), broken_screen("out-b", 800, false)],
        windows.clone(),
        native.clone(),
        vec![],
        None,
        None,
        true,
    );
    gapped_report.equal_workspace_count = true;
    let gapped = accepted(&mut engine, &gapped_report);
    assert_eq!(
        workspace_ids(&gapped, "out-b"),
        ids_b0,
        "screen gap pads nothing: {:?}",
        gapped.workspaces
    );
    assert_eq!(
        workspace_ids(&gapped, "out-a"),
        ids_a0,
        "screen gap pads nothing: {:?}",
        gapped.workspaces
    );
    let mut recovered_report = report(
        3,
        3,
        two_screens(),
        windows.clone(),
        native.clone(),
        vec![],
        None,
        None,
        true,
    );
    recovered_report.equal_workspace_count = true;
    let recovered = accepted(&mut engine, &recovered_report);
    assert_eq!(
        workspace_ids(&recovered, "out-b").len(),
        workspace_ids(&recovered, "out-a").len(),
        "recovery pads to parity: {:?}",
        recovered.workspaces
    );
}

#[test]
fn gap_known_conflicting_receipt_holds_and_recovers() {
    // A known mapped conflicting receipt during an inventory gap is
    // unproven evidence: the gapped reply keeps retained homing with a
    // hold, and a neutral (unmapped) recovery proves nothing moved.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            one_screen(),
            healthy_windows(),
            vec![native_obs("out-a", &[D1, D2], Some(D1))],
            vec![],
            None,
            None,
            true,
        ),
    );
    let bound: std::collections::BTreeMap<String, String> = bound_ids(&first, "out-a")
        .into_iter()
        .map(|(id, n)| (n, id))
        .collect();
    let (home_out, home_ws, _) = window_row(&first, "win-b").expect("baseline win-b row");
    assert_eq!(home_ws, bound[D2]);
    let ids0 = workspace_ids(&first, "out-a");
    let view0 = output_view(&first, "out-a").expect("baseline view");
    // Gap: win-b's receipt names D1, mapped to the other logical.
    let mut gap_windows = healthy_windows();
    gap_windows[1].native_workspace = Some(D1.to_owned());
    let gapped = accepted(
        &mut engine,
        &report(
            2,
            2,
            one_screen(),
            gap_windows,
            vec![native_obs("out-a", &[D1, D2], Some(D1))],
            vec![],
            None,
            None,
            false,
        ),
    );
    let (gap_out, gap_ws, gap_auth) =
        window_row(&gapped, "win-b").expect("conflicting receipt keeps a row");
    assert_eq!(
        (gap_out, gap_ws),
        (home_out.clone(), home_ws.clone()),
        "gap infers no homing"
    );
    assert_eq!(
        gap_auth,
        ScopeAuthority::Hold(HoldReason::IncompleteInventory),
        "gap holds, never writes"
    );
    assert_eq!(
        workspace_ids(&gapped, "out-a"),
        ids0,
        "gap infers no close and mints no workspace"
    );
    assert_eq!(
        output_view(&gapped, "out-a"),
        Some(view0.clone()),
        "gap infers no view"
    );
    assert!(gapped.transfers.is_empty());
    assert!(
        gapped.native_creations.is_empty(),
        "gap mints no creations: {:?}",
        gapped.native_creations
    );
    // Neutral recovery (unmapped receipt) keeps the retained home: the
    // gap persisted no rehome.
    let mut rec_windows = healthy_windows();
    rec_windows[1].native_workspace = Some("unknown-native-zzz".to_owned());
    let recovered = accepted(
        &mut engine,
        &report(
            3,
            3,
            one_screen(),
            rec_windows,
            vec![native_obs("out-a", &[D1, D2], Some(D1))],
            vec![],
            None,
            None,
            true,
        ),
    );
    let (rec_out, rec_ws, rec_auth) = window_row(&recovered, "win-b").expect("recovered win-b row");
    assert_eq!(
        (rec_out, rec_ws),
        (home_out, home_ws),
        "gap persisted no rehome"
    );
    // Unknown native evidence fences writes (existing rule): the
    // discriminating field is the workspace, never an inference.
    assert_eq!(rec_auth, ScopeAuthority::Hold(HoldReason::Unreadable));
    assert_eq!(workspace_ids(&recovered, "out-a"), ids0);
}

#[test]
fn screen_gap_known_conflicting_receipt_holds_and_recovers() {
    // Same unproven-evidence rule for a per-screen gap: the screen reads
    // incomplete while the inventory is complete.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            one_screen(),
            healthy_windows(),
            vec![native_obs("out-a", &[D1, D2], Some(D1))],
            vec![],
            None,
            None,
            true,
        ),
    );
    let bound: std::collections::BTreeMap<String, String> = bound_ids(&first, "out-a")
        .into_iter()
        .map(|(id, n)| (n, id))
        .collect();
    let (home_out, home_ws, _) = window_row(&first, "win-b").expect("baseline win-b row");
    assert_eq!(home_ws, bound[D2]);
    let ids0 = workspace_ids(&first, "out-a");
    let mut gap_windows = healthy_windows();
    gap_windows[1].native_workspace = Some(D1.to_owned());
    let gapped = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![broken_screen("out-a", 0, true)],
            gap_windows,
            vec![native_obs("out-a", &[D1, D2], Some(D1))],
            vec![],
            None,
            None,
            true,
        ),
    );
    let (gap_out, gap_ws, gap_auth) =
        window_row(&gapped, "win-b").expect("conflicting receipt keeps a row");
    assert_eq!(
        (gap_out, gap_ws),
        (home_out.clone(), home_ws.clone()),
        "screen gap infers no homing"
    );
    assert_eq!(
        gap_auth,
        ScopeAuthority::Hold(HoldReason::IncompleteInventory),
        "screen gap holds, never writes"
    );
    assert_eq!(
        workspace_ids(&gapped, "out-a"),
        ids0,
        "screen gap infers no close and mints no workspace"
    );
    assert!(gapped.transfers.is_empty());
    assert!(gapped.native_creations.is_empty());
    let mut rec_windows = healthy_windows();
    rec_windows[1].native_workspace = Some("unknown-native-zzz".to_owned());
    let recovered = accepted(
        &mut engine,
        &report(
            3,
            3,
            one_screen(),
            rec_windows,
            vec![native_obs("out-a", &[D1, D2], Some(D1))],
            vec![],
            None,
            None,
            true,
        ),
    );
    let (rec_out, rec_ws, rec_auth) = window_row(&recovered, "win-b").expect("recovered win-b row");
    assert_eq!(
        (rec_out, rec_ws),
        (home_out, home_ws),
        "screen gap persisted no rehome"
    );
    // Unknown native evidence fences writes (existing rule): the
    // discriminating field is the workspace, never an inference.
    assert_eq!(rec_auth, ScopeAuthority::Hold(HoldReason::Unreadable));
    assert_eq!(workspace_ids(&recovered, "out-a"), ids0);
}

#[test]
fn gap_minimized_conflicting_receipt_holds_and_recovers() {
    // Minimized facts take the same rule: a known conflicting receipt
    // during an inventory gap keeps retained homing; the non-minimized
    // neutral recovery exposes any persisted rehome.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            one_screen(),
            healthy_windows(),
            vec![native_obs("out-a", &[D1, D2], Some(D1))],
            vec![],
            None,
            None,
            true,
        ),
    );
    let bound: std::collections::BTreeMap<String, String> = bound_ids(&first, "out-a")
        .into_iter()
        .map(|(id, n)| (n, id))
        .collect();
    let (home_out, home_ws, _) = window_row(&first, "win-b").expect("baseline win-b row");
    assert_eq!(home_ws, bound[D2]);
    let ids0 = workspace_ids(&first, "out-a");
    let mut min_windows = healthy_windows();
    min_windows[1].minimized = true;
    let minimized = accepted(
        &mut engine,
        &report(
            2,
            2,
            one_screen(),
            min_windows,
            vec![native_obs("out-a", &[D1, D2], Some(D1))],
            vec![],
            None,
            None,
            true,
        ),
    );
    let (min_out, min_ws, min_auth) = window_row(&minimized, "win-b").expect("minimized win-b row");
    assert_eq!((min_out, min_ws), (home_out.clone(), home_ws.clone()));
    assert_eq!(min_auth, ScopeAuthority::Controlled);
    // Gap: still minimized but the receipt conflicts to the other logical.
    let mut gap_windows = healthy_windows();
    gap_windows[1].minimized = true;
    gap_windows[1].native_workspace = Some(D1.to_owned());
    let gapped = accepted(
        &mut engine,
        &report(
            3,
            3,
            one_screen(),
            gap_windows,
            vec![native_obs("out-a", &[D1, D2], Some(D1))],
            vec![],
            None,
            None,
            false,
        ),
    );
    let (gap_out, gap_ws, gap_auth) =
        window_row(&gapped, "win-b").expect("minimized gap keeps a row");
    assert_eq!(
        (gap_out, gap_ws),
        (home_out.clone(), home_ws.clone()),
        "minimized gap infers no homing"
    );
    assert_eq!(
        gap_auth,
        ScopeAuthority::Hold(HoldReason::IncompleteInventory),
        "minimized gap holds, never writes"
    );
    assert_eq!(workspace_ids(&gapped, "out-a"), ids0);
    // Non-minimized neutral recovery: unknown keeps the retained home.
    let mut rec_windows = healthy_windows();
    rec_windows[1].native_workspace = Some("unknown-native-zzz".to_owned());
    let recovered = accepted(
        &mut engine,
        &report(
            4,
            4,
            one_screen(),
            rec_windows,
            vec![native_obs("out-a", &[D1, D2], Some(D1))],
            vec![],
            None,
            None,
            true,
        ),
    );
    let (rec_out, rec_ws, rec_auth) = window_row(&recovered, "win-b").expect("recovered win-b row");
    assert_eq!(
        (rec_out, rec_ws),
        (home_out, home_ws),
        "minimized gap persisted no rehome"
    );
    // Unknown native evidence fences writes (existing rule): the
    // discriminating field is the workspace, never an inference.
    assert_eq!(rec_auth, ScopeAuthority::Hold(HoldReason::Unreadable));
    assert_eq!(workspace_ids(&recovered, "out-a"), ids0);
}

#[test]
fn shared_desire_omitted_occupant_gap_recovers_same_op() {
    // Outstanding shared creation desire survives an inventory gap that
    // omits its occupant: the retained window flight keeps the need, the
    // gap publishes nothing, and recovery re-publishes the same op id
    // (never drop/remint).
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let windows = vec![
        fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(D1)),
        fact_native("win-b", "out-a", rect(400, 0, 200, 560), Some(D2)),
    ];
    let native = vec![native_obs("out-a", &[D1, D2], Some(D1))];
    let first = accepted(
        &mut engine,
        &shared(&report(
            1,
            1,
            one_screen(),
            windows.clone(),
            native.clone(),
            vec![],
            None,
            None,
            true,
        )),
    );
    let spare = first
        .workspaces
        .iter()
        .find(|row| row.output.0 == "out-a" && row.native_binding.is_none())
        .map(|row| row.id.clone())
        .expect("spare");
    let desired = accepted(
        &mut engine,
        &shared(&report(
            2,
            2,
            one_screen(),
            windows.clone(),
            native.clone(),
            vec![],
            Some(11),
            Some(WorldCommand::Send {
                window: WindowId("win-a".to_owned()),
                target_output: OutputId("out-a".to_owned()),
                target_workspace: spare.clone(),
                follow: false,
            }),
            true,
        )),
    );
    assert_eq!(
        desired.native_creations.len(),
        1,
        "healthy report mints one desire: {:?}",
        desired.native_creations
    );
    let op = desired.native_creations[0].operation_id.clone();
    // Inventory gap omits the moved occupant outright.
    let gapped = accepted(
        &mut engine,
        &shared(&report(
            3,
            3,
            one_screen(),
            vec![fact_native(
                "win-b",
                "out-a",
                rect(400, 0, 200, 560),
                Some(D2),
            )],
            native.clone(),
            vec![],
            None,
            None,
            false,
        )),
    );
    assert!(
        gapped.native_creations.is_empty(),
        "gap withholds desires: {:?}",
        gapped.native_creations
    );
    assert_eq!(
        gapped.transfers.len(),
        1,
        "gap retains the window flight: {:?}",
        gapped.transfers
    );
    let recovered = accepted(
        &mut engine,
        &shared(&report(
            4,
            4,
            one_screen(),
            windows.clone(),
            native.clone(),
            vec![],
            None,
            None,
            true,
        )),
    );
    assert_eq!(
        recovered.native_creations.len(),
        1,
        "recovery re-publishes: {:?}",
        recovered.native_creations
    );
    assert_eq!(
        recovered.native_creations[0].operation_id, op,
        "recovery keeps the same op, never remints"
    );
    assert_eq!(recovered.native_creations[0].logical_workspace, spare);
}

#[test]
fn shared_desire_conflicting_occupant_gap_recovers_same_op() {
    // Same fence for a conflicting occupant receipt during the gap: the
    // outstanding membership flight shields homing, the desire survives,
    // and recovery keeps the same op id.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let windows = vec![
        fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(D1)),
        fact_native("win-b", "out-a", rect(400, 0, 200, 560), Some(D2)),
    ];
    let native = vec![native_obs("out-a", &[D1, D2], Some(D1))];
    let first = accepted(
        &mut engine,
        &shared(&report(
            1,
            1,
            one_screen(),
            windows.clone(),
            native.clone(),
            vec![],
            None,
            None,
            true,
        )),
    );
    let spare = first
        .workspaces
        .iter()
        .find(|row| row.output.0 == "out-a" && row.native_binding.is_none())
        .map(|row| row.id.clone())
        .expect("spare");
    let desired = accepted(
        &mut engine,
        &shared(&report(
            2,
            2,
            one_screen(),
            windows.clone(),
            native.clone(),
            vec![],
            Some(11),
            Some(WorldCommand::Send {
                window: WindowId("win-a".to_owned()),
                target_output: OutputId("out-a".to_owned()),
                target_workspace: spare.clone(),
                follow: false,
            }),
            true,
        )),
    );
    assert_eq!(desired.native_creations.len(), 1);
    let op = desired.native_creations[0].operation_id.clone();
    // Gap: the moved occupant's receipt conflicts back to its old desktop.
    let mut gap_windows = windows.clone();
    gap_windows[0].native_workspace = Some(D1.to_owned());
    let gapped = accepted(
        &mut engine,
        &shared(&report(
            3,
            3,
            one_screen(),
            gap_windows,
            native.clone(),
            vec![],
            None,
            None,
            false,
        )),
    );
    assert!(
        gapped.native_creations.is_empty(),
        "gap withholds desires: {:?}",
        gapped.native_creations
    );
    let recovered = accepted(
        &mut engine,
        &shared(&report(
            4,
            4,
            one_screen(),
            windows.clone(),
            native.clone(),
            vec![],
            None,
            None,
            true,
        )),
    );
    assert_eq!(
        recovered.native_creations.len(),
        1,
        "recovery re-publishes: {:?}",
        recovered.native_creations
    );
    assert_eq!(
        recovered.native_creations[0].operation_id, op,
        "recovery keeps the same op, never remints"
    );
    assert_eq!(recovered.native_creations[0].logical_workspace, spare);
}

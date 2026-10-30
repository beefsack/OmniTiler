//! Host-owned native attempt settlement (arrival-gated source release).
//!
//! Core translates the adapter's raw native attempt readbacks through its
//! own per-output bindings under a single evidence lane (host-owned ignores
//! logical attempt fields; tiler-owned uses only the logical fields), then
//! applies the existing settlement rule: a change matching the outstanding
//! operation settles it, otherwise outside input is adopted. No adapter
//! binding table: logical attempt fields stay empty on host-owned reports.
//!
//! - Release rule: once every surviving migration member's staged homing
//!   names the target (positive per-member native arrival proof — never
//!   a roster-absence predicate; a missing, unreadable, or contradictory
//!   receipt for any member holds the flight instead), the migrated
//!   workspace's source-output native pairs release — operation-scoped to
//!   the migrated id, never a global sweep — and every released desktop
//!   present in the complete native order adopts at once as a fresh empty
//!   row at native-order position through the ordinary outside rule,
//!   whether selected or not. No vacancy inference from window facts and
//!   no attempt-proof gate for release (settlement keeps its own proof
//!   gates). A partial enumeration never mutates bindings: with an
//!   incomplete list the core cannot tell a vacated desktop from an
//!   unreported one, so partial reports hold without releasing. Pending
//!   creations defer (late desktops belong to them). The flight holds one
//!   more round on the unknown staged echo, then settles with the switch
//!   winning and the refill retired forever. Refusals never reach release
//!   (outstanding
//!   members, or the latched terminal, return first), so rollback keeps
//!   its backing.
//! - Known-surviving switch: both native views land, then the user switches both
//!   outputs to surviving known desktops before the proof capture (the
//!   target side restores its last-seen current; the source side moves to
//!   another surviving desktop). The translated posts prove both writes
//!   landed, so the rewound currents are newer external input even where
//!   the echo still names the last-seen view: the migration stays (members
//!   remain migrated, no rollback), the user's switch wins (both echoes
//!   adopted as views), and the paired source refill child retires forever
//!   (dropped, never re-actuated). Without the native attempt evidence
//!   the same observed state holds both flights.
//! - Exact-old-source vacated rewind (original B2 discriminator, option 1):
//!   after members arrive with views old, the user restores BOTH old
//!   currents, including the migration-vacated source desktop. Arrival
//!   already released the dormant pair, so the outside rule adopts the
//!   vacated desktop as a fresh empty logical row at its native-order
//!   position (no reverse-migration provenance, no eligible spare reuse).
//!   The translated posts prove both writes landed, so the restored
//!   currents are newer external input: the migration stays through the
//!   rewind round, then settles with the user's switch winning on the
//!   source and the refill child retired forever (never re-actuated).
//!   Occupied dormant desktops never release before arrival (a refused
//!   migration still rolls back onto its backing). Both the listed-order
//!   shape (order still names the vacated desktop) and the pruned-order
//!   shape (order lags the selected current) settle this way; unknown
//!   echoes with no adoption still hold.
//! - Ask 5: a late tiler send observed with intermediate readings (the
//!   staged fact still shows the source while the attempt's translated
//!   post already proves arrival at the target). The transfer settles on
//!   the proven write instead of holding for (or adopting) the stale
//!   reading as outside input. Without the native post the same staged
//!   state holds the flight for later proof. Report facts ride captured
//!   after the attempts per the reporting guide, so the intermediate
//!   capture is a modeled stale read, not a causal-ordering claim; the
//!   same-output shape below does not broaden to cross-output paths.

use tiler_core::directional::{OutputId, WindowId};
use tiler_core::geometry::Rect;
use tiler_core::size_hints::WindowSizeHints;
use tiler_core::workspace_scope::WorkspaceMode;
use tiler_core::world_engine::{WorldEngine, WorldOutcome};
use tiler_core::world_report::{
    FrameSetter, MemberAttempt, NativeCreationAttempt, NativeObservation, NativeOccupancy,
    ScreenReport, ViewAttempt, WORLD_CONTRACT_VERSION, WorldCommand, WorldReport,
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

// Opaque native ids: UUID-like desktop ids on out-a, numeric Space ids on
// out-b, plus one fresh Space id created mid-flight.
const UA1: &str = "3f9a8c1e-1111-4b2a-9c3d-0000000000a1";
const UA2: &str = "3f9a8c1e-2222-4b2a-9c3d-0000000000a2";
const UA3: &str = "3f9a8c1e-3333-4b2a-9c3d-0000000000a3";
const SB1: &str = "space-11";
const SB2: &str = "space-12";
const SB3: &str = "space-13";
const SB_NEW: &str = "space-19";

fn native_obs(output: &str, order: &[&str], current: &str) -> NativeObservation {
    NativeObservation {
        output: OutputId(output.to_owned()),
        native_order: order.iter().map(|s| (*s).to_owned()).collect(),
        current_native: Some(current.to_owned()),
        occupancy: order
            .iter()
            .map(|s| NativeOccupancy {
                native: (*s).to_owned(),
                count: None,
            })
            .collect(),
    }
}

fn fact(window: &str, output: &str, x: i32, native: &str) -> tiler_core::world_report::WindowFact {
    tiler_core::world_report::WindowFact {
        window: WindowId(window.to_owned()),
        output: OutputId(output.to_owned()),
        rect: Some(rect(x, 0, 200, 200)),
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
        native_workspace: Some(native.to_owned()),
    }
}

#[allow(clippy::too_many_arguments)]
fn report(
    request_seq: u64,
    screens: Vec<ScreenReport>,
    windows: Vec<tiler_core::world_report::WindowFact>,
    native: Vec<NativeObservation>,
    command_seq: Option<u64>,
    command: Option<WorldCommand>,
    member_attempts: Vec<MemberAttempt>,
    view_attempts: Vec<ViewAttempt>,
    creation_attempts: Vec<NativeCreationAttempt>,
) -> WorldReport {
    WorldReport {
        contract_version: WORLD_CONTRACT_VERSION,
        owner: "owner-a".to_owned(),
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        observation_seq: request_seq,
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
        action_outputs: vec![],
        inventory_complete: true,
        gesture_barrier: false,
        gesture: None,
        switch_target: tiler_core::cursor_targeting::SwitchTarget::default(),
        fresh_placement: tiler_core::cursor_targeting::FreshPlacement::default(),
        focus_follows_cursor: false,
        cursor_warp: false,
        cursor: None,
        cursor_hit: None,
        monotonic_ms: 0,
        frames: vec![],
        member_attempts,
        view_attempts,
        clear_attempts: vec![],
        creation_attempts,
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

fn two_screens() -> Vec<ScreenReport> {
    vec![screen("out-a", 0, true), screen("out-b", 800, false)]
}

fn output_view(reply: &tiler_core::world_report::WorldReply, output: &str) -> Option<String> {
    reply
        .outputs
        .iter()
        .find(|row| row.output.0 == output)
        .and_then(|row| row.logical_view.clone())
}

/// Bootstrap two adopted outputs and migrate the shown out-a workspace to
/// out-b, returning the engine plus the migrated logical id, the refill
/// logical id and its bound native, and the retained operation ids.
fn migrate_setup() -> (WorldEngine, String, String, String, String, String, String) {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            two_screens(),
            vec![
                fact("win-a", "out-a", 0, UA1),
                fact("win-b", "out-b", 800, SB1),
            ],
            vec![
                native_obs("out-a", &[UA1, UA2, UA3], UA1),
                native_obs("out-b", &[SB1, SB2, SB3], SB1),
            ],
            None,
            None,
            vec![],
            vec![],
            vec![],
        ),
    );
    let migrated = output_view(&first, "out-a").expect("out-a shows a workspace");
    // Migrate the shown out-a workspace right.
    let drained = accepted(
        &mut engine,
        &report(
            2,
            two_screens(),
            vec![
                fact("win-a", "out-a", 0, UA1),
                fact("win-b", "out-b", 800, SB1),
            ],
            vec![
                native_obs("out-a", &[UA1, UA2, UA3], UA1),
                native_obs("out-b", &[SB1, SB2, SB3], SB1),
            ],
            Some(11),
            Some(WorldCommand::MigrateWorkspace {
                source_output: OutputId("out-a".to_owned()),
                target_output: OutputId("out-b".to_owned()),
                workspace: migrated.clone(),
            }),
            vec![],
            vec![],
            vec![],
        ),
    );
    let migration_op = drained
        .transfers
        .iter()
        .find(|t| matches!(t.kind, tiler_core::world_report::TransferKind::Migration))
        .expect("migration retained")
        .operation_id
        .clone();
    let refill_op = format!("{migration_op}-refill");
    let refill_ws = drained
        .transfers
        .iter()
        .find(|t| t.operation_id == refill_op)
        .expect("refill child retained")
        .target_workspace
        .clone();
    let refill_native = drained
        .workspaces
        .iter()
        .find(|row| row.output.0 == "out-a" && row.id == refill_ws)
        .and_then(|row| row.native_binding.clone())
        .expect("refill stays bound");
    let target_out = drained
        .transfers
        .iter()
        .find(|t| t.operation_id == migration_op)
        .expect("migration target")
        .target_output
        .clone();
    assert_eq!(target_out.0, "out-b");
    // Fresh backing is desired for the migrated id on out-b (existing
    // desktops are never stolen).
    assert!(
        drained.native_creations.iter().any(|desire| desire
            .logical_workspace
            == migrated
            && desire.target_output.0 == "out-b"),
        "fresh backing desired: {:?}",
        drained.native_creations
    );
    (
        engine,
        migrated,
        refill_ws,
        refill_native,
        migration_op,
        refill_op,
        "op-11".to_owned(),
    )
}

/// Bind the fresh out-b backing and land the members with both views still
/// old: the mid-flight hold with arrived members and unproven views.
/// Returns the reply so callers can pin the arrival-round state (e.g. the
/// released source already rebound fresh while the flight is pending).
fn land_members_views_old(
    engine: &mut WorldEngine,
    migrated: &str,
    seq: u64,
    current_a: &str,
    current_b: &str,
) -> tiler_core::world_report::WorldReply {
    let settled = accepted(
        engine,
        &report(
            seq,
            two_screens(),
            vec![
                fact("win-a", "out-b", 800, SB_NEW),
                fact("win-b", "out-b", 800, SB1),
            ],
            vec![
                native_obs("out-a", &[UA1, UA2, UA3], current_a),
                native_obs("out-b", &[SB1, SB2, SB3, SB_NEW], current_b),
            ],
            None,
            None,
            vec![],
            vec![],
            vec![NativeCreationAttempt {
                operation_id: "op-native-1".to_owned(),
                logical_workspace: migrated.to_owned(),
                target_output: OutputId("out-b".to_owned()),
                setter: FrameSetter::Accepted,
                order_before: vec![SB1.to_owned(), SB2.to_owned(), SB3.to_owned()],
                order_after: vec![
                    SB1.to_owned(),
                    SB2.to_owned(),
                    SB3.to_owned(),
                    SB_NEW.to_owned(),
                ],
                created_native: Some(SB_NEW.to_owned()),
                attempt_ms: seq,
            }],
        ),
    );
    assert!(
        settled
            .transfers
            .iter()
            .any(|t| matches!(t.kind, tiler_core::world_report::TransferKind::Migration)),
        "members arrived but views old retains the migration: {:?}",
        settled.transfers
    );
    assert!(
        settled
            .transfers
            .iter()
            .any(|t| t.kind == tiler_core::world_report::TransferKind::View),
        "members arrived but views old retains the refill: {:?}",
        settled.transfers
    );
    settled
}

#[test]
fn known_surviving_source_switch_settles_with_switch_winning() {
    // Known-surviving switch (NOT the original B2 exact-old-source
    // discriminator): both native writes land (members arrive, both view
    // attempts prove their targets through translated native posts), then
    // the user switches both outputs to surviving known desktops before
    // the proof capture. The migration stays (members remain migrated, no
    // rollback), the user's switch wins (both rewound currents adopted as
    // views), and the paired source refill child retires forever (no
    // retained flight, so nothing re-actuates it).
    let (mut engine, migrated, refill_ws, refill_native, migration_op, refill_op, _) =
        migrate_setup();
    land_members_views_old(&mut engine, &migrated, 3, UA1, SB1);
    let converged = accepted(
        &mut engine,
        &report(
            4,
            two_screens(),
            vec![
                fact("win-a", "out-b", 800, SB_NEW),
                fact("win-b", "out-b", 800, SB1),
            ],
            vec![
                native_obs("out-a", &[UA1, UA2, UA3], UA2),
                native_obs("out-b", &[SB1, SB2, SB3, SB_NEW], SB1),
            ],
            None,
            None,
            vec![],
            vec![
                ViewAttempt {
                    operation_id: migration_op.clone(),
                    target_output: OutputId("out-b".to_owned()),
                    target_workspace: migrated.clone(),
                    setter: FrameSetter::Accepted,
                    pre: None,
                    post: None,
                    baseline: None,
                    native_pre: Some(SB1.to_owned()),
                    native_post: Some(SB_NEW.to_owned()),
                    native_baseline: Some(SB1.to_owned()),
                    attempt_ms: 4,
                },
                ViewAttempt {
                    operation_id: refill_op.clone(),
                    target_output: OutputId("out-a".to_owned()),
                    target_workspace: refill_ws.clone(),
                    setter: FrameSetter::Accepted,
                    pre: None,
                    post: None,
                    baseline: None,
                    native_pre: Some(UA1.to_owned()),
                    native_post: Some(refill_native.clone()),
                    native_baseline: Some(UA1.to_owned()),
                    attempt_ms: 4,
                },
            ],
            vec![],
        ),
    );
    assert!(
        converged.transfers.is_empty(),
        "proven-then-rewound views settle everything: {:?}",
        converged.transfers
    );
    let member = converged
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-a")
        .expect("mover published");
    assert_eq!(member.output.0, "out-b", "migration stays: {member:?}");
    assert_eq!(member.workspace, migrated, "migration stays: {member:?}");
    // The user's rewound currents win on both outputs ...
    let out_a_view = output_view(&converged, "out-a").expect("out-a view");
    let out_b_view = output_view(&converged, "out-b").expect("out-b view");
    assert_ne!(out_a_view, refill_ws, "refill abandoned: {out_a_view}");
    assert_ne!(out_b_view, migrated, "target abandoned: {out_b_view}");
    // ... and they name the surviving known desktops the user switched to.
    let ring_a = engine.logical_ring("out-a");
    let ring_b = engine.logical_ring("out-b");
    assert!(ring_a.contains(&out_a_view), "adopted known: {ring_a:?}");
    assert!(ring_b.contains(&out_b_view), "adopted known: {ring_b:?}");
    // A follow-up observation stays settled with no re-actuation: the
    // refill child retired forever, so no flight can revive it.
    let stable = accepted(
        &mut engine,
        &report(
            5,
            two_screens(),
            vec![
                fact("win-a", "out-b", 800, SB_NEW),
                fact("win-b", "out-b", 800, SB1),
            ],
            vec![
                native_obs("out-a", &[UA1, UA2, UA3], UA2),
                native_obs("out-b", &[SB1, SB2, SB3, SB_NEW], SB1),
            ],
            None,
            None,
            vec![],
            vec![],
            vec![],
        ),
    );
    assert!(
        stable.transfers.is_empty(),
        "no flight revives the retired refill: {:?}",
        stable.transfers
    );
    assert_eq!(
        output_view(&stable, "out-a").as_deref(),
        Some(out_a_view.as_str())
    );
    assert_eq!(
        output_view(&stable, "out-b").as_deref(),
        Some(out_b_view.as_str())
    );
}

#[test]
fn known_surviving_switch_without_native_attempt_evidence_holds_both_flights() {
    // Control for the known-surviving switch: the same rewound observation
    // WITHOUT native attempt evidence cannot prove the writes landed, so
    // both flights hold (stale echoes never settle, unknown natives never
    // prove).
    let (mut engine, migrated, _, _, migration_op, refill_op, _) = migrate_setup();
    land_members_views_old(&mut engine, &migrated, 3, UA1, SB1);
    let held = accepted(
        &mut engine,
        &report(
            4,
            two_screens(),
            vec![
                fact("win-a", "out-b", 800, SB_NEW),
                fact("win-b", "out-b", 800, SB1),
            ],
            vec![
                native_obs("out-a", &[UA1, UA2, UA3], UA2),
                native_obs("out-b", &[SB1, SB2, SB3, SB_NEW], SB1),
            ],
            None,
            None,
            vec![],
            vec![],
            vec![],
        ),
    );
    assert!(
        held.transfers
            .iter()
            .any(|t| t.operation_id == migration_op),
        "no attempt proof retains the migration: {:?}",
        held.transfers
    );
    assert!(
        held.transfers.iter().any(|t| t.operation_id == refill_op),
        "no attempt proof retains the refill: {:?}",
        held.transfers
    );
}

#[test]
fn ask5_late_send_with_intermediate_readings_settles_transfer() {
    // macOS ask 5: the tiler's send is observed late — the staged fact
    // still shows the source (intermediate capture) while the attempt's
    // translated native post already proves arrival at the target. The
    // transfer settles on the proven write instead of holding for (or
    // adopting) the stale reading as outside input. Without the native
    // post the same staged state holds the flight for later proof.
    // (Same-output: cross-output sends reconcile through a different
    // session path; the stale-reading settlement rule is output-agnostic.)
    for with_proof in [false, true] {
        let mut engine = WorldEngine::new("gen-1").expect("generation");
        let first = accepted(
            &mut engine,
            &report(
                1,
                two_screens(),
                vec![
                    fact("win-a", "out-a", 0, UA1),
                    fact("win-b", "out-b", 800, SB1),
                ],
                vec![
                    native_obs("out-a", &[UA1, UA2], UA1),
                    native_obs("out-b", &[SB1, SB2], SB1),
                ],
                None,
                None,
                vec![],
                vec![],
                vec![],
            ),
        );
        let ring_a = engine.logical_ring("out-a");
        let source_ws = output_view(&first, "out-a").expect("out-a shows a workspace");
        let target_ws = ring_a
            .iter()
            .find(|id| *id != &source_ws)
            .cloned()
            .expect("out-a has a second workspace");
        let target_native = first
            .workspaces
            .iter()
            .find(|row| row.output.0 == "out-a" && row.id == target_ws)
            .and_then(|row| row.native_binding.clone())
            .expect("target stays bound");
        accepted(
            &mut engine,
            &report(
                2,
                two_screens(),
                vec![
                    fact("win-a", "out-a", 0, UA1),
                    fact("win-b", "out-b", 800, SB1),
                ],
                vec![
                    native_obs("out-a", &[UA1, UA2], UA1),
                    native_obs("out-b", &[SB1, SB2], SB1),
                ],
                Some(11),
                Some(WorldCommand::Send {
                    window: WindowId("win-a".to_owned()),
                    target_output: OutputId("out-a".to_owned()),
                    target_workspace: target_ws.clone(),
                    follow: false,
                }),
                vec![],
                vec![],
                vec![],
            ),
        );
        // Intermediate capture: the fact still reads the source desktop
        // while the drain already proved arrival at the target native.
        let member_attempts = if with_proof {
            vec![MemberAttempt {
                operation_id: "op-11".to_owned(),
                window: WindowId("win-a".to_owned()),
                target_output: OutputId("out-a".to_owned()),
                target_workspace: target_ws.clone(),
                setter: FrameSetter::Accepted,
                pre: None,
                post: None,
                native_pre: Some(UA1.to_owned()),
                native_post: Some(target_native.clone()),
                attempt_ms: 3,
            }]
        } else {
            vec![]
        };
        let observed = accepted(
            &mut engine,
            &report(
                3,
                two_screens(),
                vec![
                    fact("win-a", "out-a", 0, UA1),
                    fact("win-b", "out-b", 800, SB1),
                ],
                vec![
                    native_obs("out-a", &[UA1, UA2], UA1),
                    native_obs("out-b", &[SB1, SB2], SB1),
                ],
                None,
                None,
                member_attempts,
                vec![],
                vec![],
            ),
        );
        if with_proof {
            assert!(
                observed.transfers.is_empty(),
                "proven post settles despite the stale fact: {:?}",
                observed.transfers
            );
            // The mover stays homed where the staged fact reads it (the
            // flight is settled; homing follows the next fresh capture).
            let member = observed
                .windows
                .iter()
                .find(|row| row.lifetime.0 == "win-a")
                .expect("mover published");
            assert_eq!(member.workspace, source_ws, "stale homing kept: {member:?}");
        } else {
            assert!(
                observed.transfers.iter().any(|t| t.operation_id == "op-11"),
                "no proof holds the flight for later arrival: {:?}",
                observed.transfers
            );
        }
        // A follow-up fresh capture converges the homing to the target.
        let fresh = accepted(
            &mut engine,
            &report(
                4,
                two_screens(),
                vec![
                    fact("win-a", "out-a", 0, &target_native),
                    fact("win-b", "out-b", 800, SB1),
                ],
                vec![
                    native_obs("out-a", &[UA1, UA2], UA1),
                    native_obs("out-b", &[SB1, SB2], SB1),
                ],
                None,
                None,
                vec![],
                vec![],
                vec![],
            ),
        );
        assert!(
            fresh.transfers.is_empty(),
            "settled flight stays settled: {:?}",
            fresh.transfers
        );
        let member = fresh
            .windows
            .iter()
            .find(|row| row.lifetime.0 == "win-a")
            .expect("mover published");
        assert_eq!(member.workspace, target_ws, "homing converges: {member:?}");
    }
}

#[test]
fn host_owned_ignores_conflicting_logical_and_proves_via_native() {
    // Capability lane: host-owned ignores logical attempt fields and
    // translates only natives. A conflicting logical pre/post (wrong
    // homing) must not veto a native proof that names the target.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            two_screens(),
            vec![
                fact("win-a", "out-a", 0, UA1),
                fact("win-b", "out-b", 800, SB1),
            ],
            vec![
                native_obs("out-a", &[UA1, UA2], UA1),
                native_obs("out-b", &[SB1, SB2], SB1),
            ],
            None,
            None,
            vec![],
            vec![],
            vec![],
        ),
    );
    let ring_a = engine.logical_ring("out-a");
    let source_ws = output_view(&first, "out-a").expect("out-a shows a workspace");
    let target_ws = ring_a
        .iter()
        .find(|id| *id != &source_ws)
        .cloned()
        .expect("out-a has a second workspace");
    let target_native = first
        .workspaces
        .iter()
        .find(|row| row.output.0 == "out-a" && row.id == target_ws)
        .and_then(|row| row.native_binding.clone())
        .expect("target stays bound");
    accepted(
        &mut engine,
        &report(
            2,
            two_screens(),
            vec![
                fact("win-a", "out-a", 0, UA1),
                fact("win-b", "out-b", 800, SB1),
            ],
            vec![
                native_obs("out-a", &[UA1, UA2], UA1),
                native_obs("out-b", &[SB1, SB2], SB1),
            ],
            Some(11),
            Some(WorldCommand::Send {
                window: WindowId("win-a".to_owned()),
                target_output: OutputId("out-a".to_owned()),
                target_workspace: target_ws.clone(),
                follow: false,
            }),
            vec![],
            vec![],
            vec![],
        ),
    );
    // Conflicting logical pre/post naming a wrong workspace plus the
    // correct native proof: the logical lane is ignored, the native post
    // settles the transfer.
    let observed = accepted(
        &mut engine,
        &report(
            3,
            two_screens(),
            vec![
                fact("win-a", "out-a", 0, UA1),
                fact("win-b", "out-b", 800, SB1),
            ],
            vec![
                native_obs("out-a", &[UA1, UA2], UA1),
                native_obs("out-b", &[SB1, SB2], SB1),
            ],
            None,
            None,
            vec![MemberAttempt {
                operation_id: "op-11".to_owned(),
                window: WindowId("win-a".to_owned()),
                target_output: OutputId("out-a".to_owned()),
                target_workspace: target_ws.clone(),
                setter: FrameSetter::Accepted,
                pre: Some((OutputId("out-a".to_owned()), "ws-wrong".to_owned())),
                post: Some((OutputId("out-a".to_owned()), "ws-wrong".to_owned())),
                native_pre: Some(UA1.to_owned()),
                native_post: Some(target_native.clone()),
                attempt_ms: 3,
            }],
            vec![],
            vec![],
        ),
    );
    assert!(
        observed.transfers.is_empty(),
        "conflicting logical ignored, native post settles: {:?}",
        observed.transfers
    );
}

#[test]
fn host_owned_unknown_native_holds_despite_conflicting_logical() {
    // Unknown native (unmapped id) never proves: with no readable native
    // pre/post the flight holds for later proof, and a conflicting logical
    // row must not settle it either (single native-only lane).
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            two_screens(),
            vec![
                fact("win-a", "out-a", 0, UA1),
                fact("win-b", "out-b", 800, SB1),
            ],
            vec![
                native_obs("out-a", &[UA1, UA2], UA1),
                native_obs("out-b", &[SB1, SB2], SB1),
            ],
            None,
            None,
            vec![],
            vec![],
            vec![],
        ),
    );
    let ring_a = engine.logical_ring("out-a");
    let source_ws = output_view(&first, "out-a").expect("out-a shows a workspace");
    let target_ws = ring_a
        .iter()
        .find(|id| *id != &source_ws)
        .cloned()
        .expect("out-a has a second workspace");
    accepted(
        &mut engine,
        &report(
            2,
            two_screens(),
            vec![
                fact("win-a", "out-a", 0, UA1),
                fact("win-b", "out-b", 800, SB1),
            ],
            vec![
                native_obs("out-a", &[UA1, UA2], UA1),
                native_obs("out-b", &[SB1, SB2], SB1),
            ],
            Some(11),
            Some(WorldCommand::Send {
                window: WindowId("win-a".to_owned()),
                target_output: OutputId("out-a".to_owned()),
                target_workspace: target_ws.clone(),
                follow: false,
            }),
            vec![],
            vec![],
            vec![],
        ),
    );
    let observed = accepted(
        &mut engine,
        &report(
            3,
            two_screens(),
            vec![
                fact("win-a", "out-a", 0, UA1),
                fact("win-b", "out-b", 800, SB1),
            ],
            vec![
                native_obs("out-a", &[UA1, UA2], UA1),
                native_obs("out-b", &[SB1, SB2], SB1),
            ],
            None,
            None,
            vec![MemberAttempt {
                operation_id: "op-11".to_owned(),
                window: WindowId("win-a".to_owned()),
                target_output: OutputId("out-a".to_owned()),
                target_workspace: target_ws.clone(),
                setter: FrameSetter::Accepted,
                pre: Some((OutputId("out-a".to_owned()), target_ws.clone())),
                post: Some((OutputId("out-a".to_owned()), target_ws.clone())),
                native_pre: None,
                native_post: Some("space-99".to_owned()),
                attempt_ms: 3,
            }],
            vec![],
            vec![],
        ),
    );
    assert!(
        observed.transfers.iter().any(|t| t.operation_id == "op-11"),
        "unknown native holds even with a logical-looking row: {:?}",
        observed.transfers
    );
}

#[test]
fn cross_output_refused_native_proves_only_with_source_homing() {
    // Cross-output refusal translates the native post on the retained
    // SOURCE output: an unchanged source readback (pre == post, not the
    // target) reconciles against staged source homing and settles to
    // observed. Output movement itself is not proven by the native latch
    // alone (limitation): the same latch with staged homing at the target
    // does not settle as refused.
    let (mut engine, migrated, _, _, migration_op, _, _) = migrate_setup();
    // Refused cross-output member write, window still on the source: the
    // unchanged source native readback proves refusal and settles to
    // observed (no re-actuation, no rollback beyond observed). With the
    // old target-side post translation this same row could never prove
    // refusal (post unmapped on the target).
    let refused = accepted(
        &mut engine,
        &report(
            3,
            two_screens(),
            vec![
                fact("win-a", "out-a", 0, UA1),
                fact("win-b", "out-b", 800, SB1),
            ],
            vec![
                native_obs("out-a", &[UA1, UA2, UA3], UA1),
                native_obs("out-b", &[SB1, SB2, SB3], SB1),
            ],
            None,
            None,
            vec![MemberAttempt {
                operation_id: migration_op.clone(),
                window: WindowId("win-a".to_owned()),
                target_output: OutputId("out-b".to_owned()),
                target_workspace: migrated.clone(),
                setter: FrameSetter::Refused,
                pre: None,
                post: None,
                native_pre: Some(UA1.to_owned()),
                native_post: Some(UA1.to_owned()),
                attempt_ms: 3,
            }],
            vec![],
            vec![],
        ),
    );
    assert!(
        !refused
            .transfers
            .iter()
            .any(|t| t.operation_id == migration_op),
        "refused source readback settles the migration to observed: {:?}",
        refused.transfers
    );
}

#[test]
fn accepted_target_native_post_with_different_known_staged_homing_holds() {
    // An accepted native post proving the target does not adopt a
    // different known staged homing as outside input on the same output:
    // the stale-proof shape holds the flight (ambiguity latch) rather than
    // fighting the user or fabricating external settlement.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            two_screens(),
            vec![
                fact("win-a", "out-a", 0, UA1),
                fact("win-b", "out-b", 800, SB1),
            ],
            vec![
                native_obs("out-a", &[UA1, UA2, UA3], UA1),
                native_obs("out-b", &[SB1, SB2], SB1),
            ],
            None,
            None,
            vec![],
            vec![],
            vec![],
        ),
    );
    let ring_a = engine.logical_ring("out-a");
    let source_ws = output_view(&first, "out-a").expect("out-a shows a workspace");
    let target_ws = ring_a
        .iter()
        .find(|id| *id != &source_ws)
        .cloned()
        .expect("out-a has a second workspace");
    let target_native = first
        .workspaces
        .iter()
        .find(|row| row.output.0 == "out-a" && row.id == target_ws)
        .and_then(|row| row.native_binding.clone())
        .expect("target stays bound");
    accepted(
        &mut engine,
        &report(
            2,
            two_screens(),
            vec![
                fact("win-a", "out-a", 0, UA1),
                fact("win-b", "out-b", 800, SB1),
            ],
            vec![
                native_obs("out-a", &[UA1, UA2, UA3], UA1),
                native_obs("out-b", &[SB1, SB2], SB1),
            ],
            Some(11),
            Some(WorldCommand::Send {
                window: WindowId("win-a".to_owned()),
                target_output: OutputId("out-a".to_owned()),
                target_workspace: target_ws.clone(),
                follow: false,
            }),
            vec![],
            vec![],
            vec![],
        ),
    );
    // Stage the mover on a third known workspace (neither source nor
    // target) while the attempt still proves the target. The flight must
    // not settle as outside input from the attempt alone.
    let ring_live = engine.logical_ring("out-a");
    let other_ws = ring_live
        .iter()
        .find(|id| *id != &source_ws && *id != &target_ws)
        .cloned()
        .expect("third workspace exists");
    let other_native = first
        .workspaces
        .iter()
        .find(|row| row.output.0 == "out-a" && row.id == other_ws)
        .and_then(|row| row.native_binding.clone())
        .expect("third stays bound");
    let observed = accepted(
        &mut engine,
        &report(
            3,
            two_screens(),
            vec![
                fact("win-a", "out-a", 0, &other_native),
                fact("win-b", "out-b", 800, SB1),
            ],
            vec![
                native_obs("out-a", &[UA1, UA2, UA3], UA1),
                native_obs("out-b", &[SB1, SB2], SB1),
            ],
            None,
            None,
            vec![MemberAttempt {
                operation_id: "op-11".to_owned(),
                window: WindowId("win-a".to_owned()),
                target_output: OutputId("out-a".to_owned()),
                target_workspace: target_ws.clone(),
                setter: FrameSetter::Accepted,
                pre: None,
                post: None,
                native_pre: Some(UA1.to_owned()),
                native_post: Some(target_native.clone()),
                attempt_ms: 3,
            }],
            vec![],
            vec![],
        ),
    );
    assert!(
        observed.transfers.iter().any(|t| t.operation_id == "op-11"),
        "different known staged homing holds, never adopts from the attempt: {:?}",
        observed.transfers
    );
}

#[test]
fn exact_old_source_listed_rewind_settles_with_fresh_adoption() {
    // Original B2 shape with the vacated desktop still listed (option 1):
    // after members arrive with views old, both currents rewind to the OLD
    // natives while the orders still list every desktop. Arrival released
    // the vacated source pair, so the outside rule adopts UA1 as a fresh
    // empty logical row at native-order position 0. The translated attempt
    // posts prove both writes landed, so the restored currents are newer
    // external input: the migration stays on the destination, the user's
    // switch wins on the source, and the refill child retires forever
    // (never re-actuated).
    let (mut engine, migrated, refill_ws, refill_native, migration_op, refill_op, _) =
        migrate_setup();
    // Arrival round: members homed at the target (positive native
    // arrival proof per member), views old. The release fires here —
    // the (out-a, UA1) pair naming the migrated id is removed and UA1
    // re-adopts fresh — while both flights stay pending. Later attempt
    // rows translating UA1 therefore resolve through the NEW binding
    // while the operation is still live.
    let landed = land_members_views_old(&mut engine, &migrated, 3, UA1, SB1);
    let rebound = landed
        .workspaces
        .iter()
        .find(|row| row.output.0 == "out-a" && row.native_binding.as_deref() == Some(UA1))
        .expect("arrival round rebinds the vacated desktop fresh while pending");
    assert_ne!(
        rebound.id, migrated,
        "rebound row is never the migrated id: {rebound:?}"
    );
    assert_ne!(
        rebound.id, refill_ws,
        "rebound row is never the refill: {rebound:?}"
    );
    let converged = accepted(
        &mut engine,
        &report(
            4,
            two_screens(),
            vec![
                fact("win-a", "out-b", 800, SB_NEW),
                fact("win-b", "out-b", 800, SB1),
            ],
            vec![
                native_obs("out-a", &[UA1, UA2, UA3], UA1),
                native_obs("out-b", &[SB1, SB2, SB3, SB_NEW], SB1),
            ],
            None,
            None,
            vec![],
            vec![
                ViewAttempt {
                    operation_id: migration_op.clone(),
                    target_output: OutputId("out-b".to_owned()),
                    target_workspace: migrated.clone(),
                    setter: FrameSetter::Accepted,
                    pre: None,
                    post: None,
                    baseline: None,
                    native_pre: Some(SB1.to_owned()),
                    native_post: Some(SB_NEW.to_owned()),
                    native_baseline: Some(SB1.to_owned()),
                    attempt_ms: 4,
                },
                ViewAttempt {
                    operation_id: refill_op.clone(),
                    target_output: OutputId("out-a".to_owned()),
                    target_workspace: refill_ws.clone(),
                    setter: FrameSetter::Accepted,
                    pre: None,
                    post: None,
                    baseline: None,
                    native_pre: Some(UA1.to_owned()),
                    native_post: Some(refill_native.clone()),
                    native_baseline: Some(UA1.to_owned()),
                    attempt_ms: 4,
                },
            ],
            vec![],
        ),
    );
    assert!(
        converged.transfers.is_empty(),
        "vacated rewind settles everything: {:?}",
        converged.transfers
    );
    // The migrated unit stays on the destination on fresh backing.
    let member = converged
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-a")
        .expect("mover published");
    assert_eq!(member.output.0, "out-b", "migration stays: {member:?}");
    assert_eq!(member.workspace, migrated, "migration stays: {member:?}");
    // The vacated source desktop adopts as a FRESH logical row (never the
    // migrated id, never the refill), bound to UA1 at native-order head.
    let out_a_view = output_view(&converged, "out-a").expect("out-a view");
    assert_ne!(out_a_view, refill_ws, "refill abandoned: {out_a_view}");
    assert_ne!(out_a_view, migrated, "migrated id stays on the target");
    let fresh_row = converged
        .workspaces
        .iter()
        .find(|row| row.output.0 == "out-a" && row.id == out_a_view)
        .expect("fresh source row published");
    assert_eq!(
        fresh_row.native_binding.as_deref(),
        Some(UA1),
        "fresh row binds the vacated desktop: {fresh_row:?}"
    );
    assert_eq!(
        engine.logical_ring("out-a").first().map(String::as_str),
        Some(out_a_view.as_str()),
        "fresh row sits at its native-order position: {:?}",
        engine.logical_ring("out-a")
    );
    // The target-side rewind wins there too (restored old current).
    let out_b_view = output_view(&converged, "out-b").expect("out-b view");
    assert_ne!(out_b_view, migrated, "target abandoned: {out_b_view}");
    // A follow-up observation stays settled with no re-actuation: the
    // refill child retired forever, so no flight can revive it.
    let stable = accepted(
        &mut engine,
        &report(
            5,
            two_screens(),
            vec![
                fact("win-a", "out-b", 800, SB_NEW),
                fact("win-b", "out-b", 800, SB1),
            ],
            vec![
                native_obs("out-a", &[UA1, UA2, UA3], UA1),
                native_obs("out-b", &[SB1, SB2, SB3, SB_NEW], SB1),
            ],
            None,
            None,
            vec![],
            vec![],
            vec![],
        ),
    );
    assert!(
        stable.transfers.is_empty(),
        "no flight revives the retired refill: {:?}",
        stable.transfers
    );
    assert_eq!(
        output_view(&stable, "out-a").as_deref(),
        Some(out_a_view.as_str())
    );
    assert_eq!(
        output_view(&stable, "out-b").as_deref(),
        Some(out_b_view.as_str())
    );
}

#[test]
fn exact_old_source_pruned_rewind_holds_then_settles_with_fresh_adoption() {
    // Pruned-order variant: the source order lags the restored current
    // (out-a lists [UA2,UA3] with current UA1; out-b stays listed). The
    // arrival round carries both proving view attempts yet still holds
    // both flights: a selected-but-unlisted current is contradictory
    // evidence, so nothing mints for it and the unknown echo stays a gap
    // hold. When the order catches up, the ordinary rule adopts UA1 fresh
    // at position 0 and the flight settles with the switch winning and
    // the refill retired forever.
    let (mut engine, migrated, refill_ws, refill_native, migration_op, _, _) = migrate_setup();
    let refill_op = format!("{migration_op}-refill");
    let view_attempts = || {
        vec![
            ViewAttempt {
                operation_id: migration_op.clone(),
                target_output: OutputId("out-b".to_owned()),
                target_workspace: migrated.clone(),
                setter: FrameSetter::Accepted,
                pre: None,
                post: None,
                baseline: None,
                native_pre: Some(SB1.to_owned()),
                native_post: Some(SB_NEW.to_owned()),
                native_baseline: Some(SB1.to_owned()),
                attempt_ms: 3,
            },
            ViewAttempt {
                operation_id: refill_op.clone(),
                target_output: OutputId("out-a".to_owned()),
                target_workspace: refill_ws.clone(),
                setter: FrameSetter::Accepted,
                pre: None,
                post: None,
                baseline: None,
                native_pre: Some(UA1.to_owned()),
                native_post: Some(refill_native.clone()),
                native_baseline: Some(UA1.to_owned()),
                attempt_ms: 3,
            },
        ]
    };
    let arrived_facts = vec![
        fact("win-a", "out-b", 800, SB_NEW),
        fact("win-b", "out-b", 800, SB1),
    ];
    let creation = vec![NativeCreationAttempt {
        operation_id: "op-native-1".to_owned(),
        logical_workspace: migrated.to_owned(),
        target_output: OutputId("out-b".to_owned()),
        setter: FrameSetter::Accepted,
        order_before: vec![SB1.to_owned(), SB2.to_owned(), SB3.to_owned()],
        order_after: vec![
            SB1.to_owned(),
            SB2.to_owned(),
            SB3.to_owned(),
            SB_NEW.to_owned(),
        ],
        created_native: Some(SB_NEW.to_owned()),
        attempt_ms: 3,
    }];
    // Arrival round with the source order lagging: holds both flights and
    // mints nothing for the unlisted current.
    let held = accepted(
        &mut engine,
        &report(
            3,
            two_screens(),
            arrived_facts.clone(),
            vec![
                native_obs("out-a", &[UA2, UA3], UA1),
                native_obs("out-b", &[SB1, SB2, SB3, SB_NEW], SB1),
            ],
            None,
            None,
            vec![],
            view_attempts(),
            creation,
        ),
    );
    assert!(
        held.transfers
            .iter()
            .any(|t| t.operation_id == migration_op),
        "pruned rewind holds the migration: {:?}",
        held.transfers
    );
    assert!(
        held.transfers.iter().any(|t| t.operation_id == refill_op),
        "pruned rewind holds the refill: {:?}",
        held.transfers
    );
    assert!(
        !held
            .workspaces
            .iter()
            .any(|row| { row.output.0 == "out-a" && row.native_binding.as_deref() == Some(UA1) }),
        "no row mints for an unlisted current: {:?}",
        held.workspaces
    );
    // The order catches up: UA1 adopts fresh at position 0, the refill
    // retires, and the migration settles with both switches winning.
    // The proving view rows ride again verbatim: redelivery through the
    // removed and re-adopted binding still retires the flight while it
    // is pending (no resurrection, no mis-settlement).
    let converged = accepted(
        &mut engine,
        &report(
            4,
            two_screens(),
            arrived_facts.clone(),
            vec![
                native_obs("out-a", &[UA1, UA2, UA3], UA1),
                native_obs("out-b", &[SB1, SB2, SB3, SB_NEW], SB1),
            ],
            None,
            None,
            vec![],
            view_attempts(),
            vec![],
        ),
    );
    assert!(
        converged.transfers.is_empty(),
        "caught-up order settles everything: {:?}",
        converged.transfers
    );
    let member = converged
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-a")
        .expect("mover published");
    assert_eq!(member.output.0, "out-b", "migration stays: {member:?}");
    assert_eq!(member.workspace, migrated, "migration stays: {member:?}");
    let out_a_view = output_view(&converged, "out-a").expect("out-a view");
    assert_ne!(out_a_view, migrated, "migrated id stays on the target");
    assert_ne!(out_a_view, refill_ws, "refill abandoned: {out_a_view}");
    let fresh_a = converged
        .workspaces
        .iter()
        .find(|row| row.output.0 == "out-a" && row.id == out_a_view)
        .expect("fresh source row published");
    assert_eq!(
        fresh_a.native_binding.as_deref(),
        Some(UA1),
        "fresh source row binds the selected desktop: {fresh_a:?}"
    );
    assert_eq!(
        engine.logical_ring("out-a").first().map(String::as_str),
        Some(out_a_view.as_str()),
        "fresh row sits at its native-order position: {:?}",
        engine.logical_ring("out-a")
    );
    let out_b_view = output_view(&converged, "out-b").expect("out-b view");
    assert_ne!(out_b_view, migrated, "target abandoned: {out_b_view}");
    // Follow-up stability: nothing revives, nothing re-actuates.
    let stable = accepted(
        &mut engine,
        &report(
            5,
            two_screens(),
            arrived_facts,
            vec![
                native_obs("out-a", &[UA1, UA2, UA3], UA1),
                native_obs("out-b", &[SB1, SB2, SB3, SB_NEW], SB1),
            ],
            None,
            None,
            vec![],
            vec![],
            vec![],
        ),
    );
    assert!(
        stable.transfers.is_empty(),
        "settled flight stays settled: {:?}",
        stable.transfers
    );
    assert_eq!(
        output_view(&stable, "out-a").as_deref(),
        Some(out_a_view.as_str())
    );
    assert_eq!(
        output_view(&stable, "out-b").as_deref(),
        Some(out_b_view.as_str())
    );
}

#[test]
fn stale_occupant_fact_does_not_block_arrival_gated_release() {
    // Negative-vacancy discriminator: the arrival round observes members
    // homed at the target with the vacated source still selected, plus a
    // stale fact still naming the vacated desktop (a gapped/unreported
    // occupant, e.g. an unacknowledged close). Release consults no window
    // facts — arrival is the positive proof — so the fresh row still
    // adopts at order position and the flight holds for fresh-echo proof.
    let (mut engine, migrated, _, _, migration_op, refill_op, _) = migrate_setup();
    let held = accepted(
        &mut engine,
        &report(
            3,
            two_screens(),
            vec![
                fact("win-a", "out-b", 800, SB_NEW),
                fact("win-b", "out-b", 800, SB1),
                fact("win-stale", "out-a", 0, UA1),
            ],
            vec![
                native_obs("out-a", &[UA1, UA2, UA3], UA1),
                native_obs("out-b", &[SB1, SB2, SB3, SB_NEW], SB1),
            ],
            None,
            None,
            vec![],
            vec![],
            vec![NativeCreationAttempt {
                operation_id: "op-native-1".to_owned(),
                logical_workspace: migrated.to_owned(),
                target_output: OutputId("out-b".to_owned()),
                setter: FrameSetter::Accepted,
                order_before: vec![SB1.to_owned(), SB2.to_owned(), SB3.to_owned()],
                order_after: vec![
                    SB1.to_owned(),
                    SB2.to_owned(),
                    SB3.to_owned(),
                    SB_NEW.to_owned(),
                ],
                created_native: Some(SB_NEW.to_owned()),
                attempt_ms: 3,
            }],
        ),
    );
    assert!(
        held.transfers
            .iter()
            .any(|t| t.operation_id == migration_op),
        "stale fact holds the migration: {:?}",
        held.transfers
    );
    assert!(
        held.transfers.iter().any(|t| t.operation_id == refill_op),
        "stale fact holds the refill: {:?}",
        held.transfers
    );
    let fresh = engine.logical_view("out-a").expect("out-a view");
    assert_ne!(fresh, migrated, "fresh row is never the migrated id");
    let fresh_row = held
        .workspaces
        .iter()
        .find(|row| row.output.0 == "out-a" && row.id == fresh)
        .expect("fresh source row published despite the stale fact");
    assert_eq!(
        fresh_row.native_binding.as_deref(),
        Some(UA1),
        "fresh row binds the vacated desktop: {fresh_row:?}"
    );
    assert_eq!(
        engine.logical_ring("out-a").first().map(String::as_str),
        Some(fresh.as_str()),
        "fresh row sits at its native-order position: {:?}",
        engine.logical_ring("out-a")
    );
}

#[test]
fn fresh_row_persists_noncurrent_and_live_selection_never_adopts() {
    // The adopted backing persists at its order position after the user
    // switches away (non-current backing is retained, still bound), and
    // selecting a live desktop never mints anything: the rewind round
    // proves both writes, retires the refill and settles with the live
    // selection winning while the fresh row stays bound.
    let (mut engine, migrated, refill_ws, refill_native, migration_op, refill_op, _) =
        migrate_setup();
    land_members_views_old(&mut engine, &migrated, 3, UA1, SB1);
    let ring_before = engine.logical_ring("out-a").len();
    let observed = accepted(
        &mut engine,
        &report(
            4,
            two_screens(),
            vec![
                fact("win-a", "out-b", 800, SB_NEW),
                fact("win-b", "out-b", 800, SB1),
            ],
            vec![
                native_obs("out-a", &[UA1, UA2, UA3], UA2),
                native_obs("out-b", &[SB1, SB2, SB3, SB_NEW], SB1),
            ],
            None,
            None,
            vec![],
            vec![
                ViewAttempt {
                    operation_id: migration_op.clone(),
                    target_output: OutputId("out-b".to_owned()),
                    target_workspace: migrated.clone(),
                    setter: FrameSetter::Accepted,
                    pre: None,
                    post: None,
                    baseline: None,
                    native_pre: Some(SB1.to_owned()),
                    native_post: Some(SB_NEW.to_owned()),
                    native_baseline: Some(SB1.to_owned()),
                    attempt_ms: 4,
                },
                ViewAttempt {
                    operation_id: refill_op.clone(),
                    target_output: OutputId("out-a".to_owned()),
                    target_workspace: refill_ws.clone(),
                    setter: FrameSetter::Accepted,
                    pre: None,
                    post: None,
                    baseline: None,
                    native_pre: Some(UA1.to_owned()),
                    native_post: Some(refill_native.clone()),
                    native_baseline: Some(UA1.to_owned()),
                    attempt_ms: 4,
                },
            ],
            vec![],
        ),
    );
    assert!(
        observed.transfers.is_empty(),
        "proven abandonment settles everything: {:?}",
        observed.transfers
    );
    assert_eq!(
        engine.logical_ring("out-a").len(),
        ring_before,
        "live selection mints no rows: {:?}",
        engine.logical_ring("out-a")
    );
    let ua1_row = observed
        .workspaces
        .iter()
        .find(|row| row.output.0 == "out-a" && row.native_binding.as_deref() == Some(UA1))
        .expect("non-current backing stays bound");
    assert_eq!(
        engine.logical_ring("out-a").first().map(String::as_str),
        Some(ua1_row.id.as_str()),
        "backing keeps its order position while non-current: {:?}",
        engine.logical_ring("out-a")
    );
    let ua2_row = observed
        .workspaces
        .iter()
        .find(|row| row.output.0 == "out-a" && row.native_binding.as_deref() == Some(UA2))
        .expect("live selection stays bound");
    assert_eq!(
        output_view(&observed, "out-a").as_deref(),
        Some(ua2_row.id.as_str()),
        "ordinary observation follows the live selection"
    );
}

#[test]
fn redelivered_attempts_after_settle_are_ignored() {
    // Delayed/replayed-attempt discriminator: redelivering the rewind
    // attempt rows verbatim after the flight retired — with the vacated
    // desktop now rebound to the fresh row — resurrects nothing and
    // re-actuates nothing. Unmatched operation ids are never evidence.
    let (mut engine, migrated, refill_ws, refill_native, migration_op, refill_op, _) =
        migrate_setup();
    land_members_views_old(&mut engine, &migrated, 3, UA1, SB1);
    let settled_view_a = output_view(
        &accepted(
            &mut engine,
            &report(
                4,
                two_screens(),
                vec![
                    fact("win-a", "out-b", 800, SB_NEW),
                    fact("win-b", "out-b", 800, SB1),
                ],
                vec![
                    native_obs("out-a", &[UA1, UA2, UA3], UA1),
                    native_obs("out-b", &[SB1, SB2, SB3, SB_NEW], SB1),
                ],
                None,
                None,
                vec![],
                vec![
                    ViewAttempt {
                        operation_id: migration_op.clone(),
                        target_output: OutputId("out-b".to_owned()),
                        target_workspace: migrated.clone(),
                        setter: FrameSetter::Accepted,
                        pre: None,
                        post: None,
                        baseline: None,
                        native_pre: Some(SB1.to_owned()),
                        native_post: Some(SB_NEW.to_owned()),
                        native_baseline: Some(SB1.to_owned()),
                        attempt_ms: 4,
                    },
                    ViewAttempt {
                        operation_id: refill_op.clone(),
                        target_output: OutputId("out-a".to_owned()),
                        target_workspace: refill_ws.clone(),
                        setter: FrameSetter::Accepted,
                        pre: None,
                        post: None,
                        baseline: None,
                        native_pre: Some(UA1.to_owned()),
                        native_post: Some(refill_native.clone()),
                        native_baseline: Some(UA1.to_owned()),
                        attempt_ms: 4,
                    },
                ],
                vec![],
            ),
        ),
        "out-a",
    )
    .expect("out-a view");
    let _ = settled_view_a;
    // Redeliver both rewind view rows plus a member row for the retired
    // migration, all translated now through the changed bindings.
    let replayed = accepted(
        &mut engine,
        &report(
            5,
            two_screens(),
            vec![
                fact("win-a", "out-b", 800, SB_NEW),
                fact("win-b", "out-b", 800, SB1),
            ],
            vec![
                native_obs("out-a", &[UA1, UA2, UA3], UA1),
                native_obs("out-b", &[SB1, SB2, SB3, SB_NEW], SB1),
            ],
            None,
            None,
            vec![MemberAttempt {
                operation_id: migration_op.clone(),
                window: WindowId("win-a".to_owned()),
                target_output: OutputId("out-b".to_owned()),
                target_workspace: migrated.clone(),
                setter: FrameSetter::Accepted,
                pre: None,
                post: None,
                native_pre: Some(UA1.to_owned()),
                native_post: Some(SB_NEW.to_owned()),
                attempt_ms: 6,
            }],
            vec![
                ViewAttempt {
                    operation_id: migration_op.clone(),
                    target_output: OutputId("out-b".to_owned()),
                    target_workspace: migrated.clone(),
                    setter: FrameSetter::Accepted,
                    pre: None,
                    post: None,
                    baseline: None,
                    native_pre: Some(SB1.to_owned()),
                    native_post: Some(SB_NEW.to_owned()),
                    native_baseline: Some(SB1.to_owned()),
                    attempt_ms: 6,
                },
                ViewAttempt {
                    operation_id: refill_op.clone(),
                    target_output: OutputId("out-a".to_owned()),
                    target_workspace: "ws-stale".to_owned(),
                    setter: FrameSetter::Accepted,
                    pre: None,
                    post: None,
                    baseline: None,
                    native_pre: Some(UA1.to_owned()),
                    native_post: Some(refill_native.clone()),
                    native_baseline: Some(UA1.to_owned()),
                    attempt_ms: 6,
                },
            ],
            vec![],
        ),
    );
    assert!(
        replayed.transfers.is_empty(),
        "replayed attempts resurrect nothing: {:?}",
        replayed.transfers
    );
    let member = replayed
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-a")
        .expect("mover published");
    assert_eq!(member.output.0, "out-b", "homing undisturbed: {member:?}");
    assert_eq!(member.workspace, migrated, "homing undisturbed: {member:?}");
}

#[test]
fn known_source_echo_never_releases_and_refusal_keeps_backing() {
    // A source echo naming a live bound row never releases (the refill
    // path settles normally with no fresh row), and a refused migration
    // rolls back onto its intact backing: release needs arrival, so a
    // pre-arrival refusal keeps the dormant pair for rollback.
    let (mut engine, migrated, refill_ws, _, _, _, _) = migrate_setup();
    let refill_native = engine
        .logical_ring("out-a")
        .into_iter()
        .find(|id| id == &refill_ws)
        .and_then(|_| {
            accepted(
                &mut engine,
                &report(
                    3,
                    two_screens(),
                    vec![
                        fact("win-a", "out-b", 800, SB_NEW),
                        fact("win-b", "out-b", 800, SB1),
                    ],
                    vec![
                        native_obs("out-a", &[UA1, UA2, UA3], UA1),
                        native_obs("out-b", &[SB1, SB2, SB3, SB_NEW], SB1),
                    ],
                    None,
                    None,
                    vec![],
                    vec![],
                    vec![NativeCreationAttempt {
                        operation_id: "op-native-1".to_owned(),
                        logical_workspace: migrated.to_owned(),
                        target_output: OutputId("out-b".to_owned()),
                        setter: FrameSetter::Accepted,
                        order_before: vec![SB1.to_owned(), SB2.to_owned(), SB3.to_owned()],
                        order_after: vec![
                            SB1.to_owned(),
                            SB2.to_owned(),
                            SB3.to_owned(),
                            SB_NEW.to_owned(),
                        ],
                        created_native: Some(SB_NEW.to_owned()),
                        attempt_ms: 3,
                    }],
                ),
            )
            .workspaces
            .iter()
            .find(|row| row.output.0 == "out-a" && row.id == refill_ws)
            .and_then(|row| row.native_binding.clone())
        })
        .expect("refill stays bound");
    // Members arrived with the source already showing the refill: the
    // ordinary path settles with no fresh row for the dormant desktop.
    let ring_len = engine.logical_ring("out-a").len() + engine.logical_ring("out-b").len();
    let settled = accepted(
        &mut engine,
        &report(
            4,
            two_screens(),
            vec![
                fact("win-a", "out-b", 800, SB_NEW),
                fact("win-b", "out-b", 800, SB1),
            ],
            vec![
                native_obs("out-a", &[UA1, UA2, UA3], &refill_native),
                native_obs("out-b", &[SB1, SB2, SB3, SB_NEW], SB_NEW),
            ],
            None,
            None,
            vec![],
            vec![],
            vec![],
        ),
    );
    assert!(
        settled.transfers.is_empty(),
        "refill echo settles normally: {:?}",
        settled.transfers
    );
    assert_eq!(
        engine.logical_ring("out-a").len() + engine.logical_ring("out-b").len(),
        ring_len,
        "no spurious fresh rows: {:?} {:?}",
        engine.logical_ring("out-a"),
        engine.logical_ring("out-b")
    );
    // A refused migration (members never left) rolls back onto the intact
    // backing with no fresh row minted.
    let (mut engine, migrated, _, _, migration_op, _, _) = migrate_setup();
    let refused = accepted(
        &mut engine,
        &report(
            3,
            two_screens(),
            vec![
                fact("win-a", "out-a", 0, UA1),
                fact("win-b", "out-b", 800, SB1),
            ],
            vec![
                native_obs("out-a", &[UA1, UA2, UA3], UA1),
                native_obs("out-b", &[SB1, SB2, SB3], SB1),
            ],
            None,
            None,
            vec![MemberAttempt {
                operation_id: migration_op.clone(),
                window: WindowId("win-a".to_owned()),
                target_output: OutputId("out-b".to_owned()),
                target_workspace: migrated.clone(),
                setter: FrameSetter::Refused,
                pre: None,
                post: None,
                native_pre: Some(UA1.to_owned()),
                native_post: Some(UA1.to_owned()),
                attempt_ms: 3,
            }],
            vec![],
            vec![],
        ),
    );
    assert!(
        !refused
            .transfers
            .iter()
            .any(|t| t.operation_id == migration_op),
        "refusal settles the migration: {:?}",
        refused.transfers
    );
    let rolled = refused
        .workspaces
        .iter()
        .find(|row| row.output.0 == "out-a" && row.id == migrated)
        .expect("rolled-back logical published on the source");
    assert_eq!(
        rolled.native_binding.as_deref(),
        Some(UA1),
        "rollback keeps the original backing: {rolled:?}"
    );
}
#[test]
fn empty_migration_target_survives_lifecycle_without_backing() {
    // Later-backing-correlation invariant (KDE producer leg3 is the
    // discriminating pin: proven red without flight-target lifecycle
    // retention, green with it): an empty migration's unbound arrival
    // must keep its row and its creation desire across backing-less
    // rounds until the backing correlates. This unit test guards the
    // survival shape at core level; the theft race needs the adapter's
    // multi-round split correlation to reproduce.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            two_screens(),
            vec![
                fact("win-a", "out-a", 0, UA1),
                fact("win-b", "out-b", 800, SB1),
            ],
            vec![
                native_obs("out-a", &[UA1, UA2, UA3], UA1),
                native_obs("out-b", &[SB1, SB2, SB3], SB1),
            ],
            None,
            None,
            vec![],
            vec![],
            vec![],
        ),
    );
    let shown_a = output_view(&first, "out-a").expect("out-a shows a workspace");
    // Close the shown workspace's only member, then migrate the shown
    // (now empty) row: the vacated source desktop stays selected, so the
    // arrival-gated release drops its dormant pair on the first naked
    // round.
    let mut closed_report = report(
        2,
        two_screens(),
        vec![fact("win-b", "out-b", 800, SB1)],
        vec![
            native_obs("out-a", &[UA1, UA2, UA3], UA1),
            native_obs("out-b", &[SB1, SB2, SB3], SB1),
        ],
        None,
        None,
        vec![],
        vec![],
        vec![],
    );
    closed_report.closed = vec![WindowId("win-a".to_owned())];
    accepted(&mut engine, &closed_report);
    let drained = accepted(
        &mut engine,
        &report(
            3,
            two_screens(),
            vec![fact("win-b", "out-b", 800, SB1)],
            vec![
                native_obs("out-a", &[UA1, UA2, UA3], UA1),
                native_obs("out-b", &[SB1, SB2, SB3], SB1),
            ],
            Some(11),
            Some(WorldCommand::MigrateWorkspace {
                source_output: OutputId("out-a".to_owned()),
                target_output: OutputId("out-b".to_owned()),
                workspace: shown_a.clone(),
            }),
            vec![],
            vec![],
            vec![],
        ),
    );
    assert!(
        drained.transfers.iter().any(|t| t.operation_id == "op-11"),
        "empty migration retained: {:?}",
        drained.transfers
    );
    assert!(
        drained
            .native_creations
            .iter()
            .any(|desire| desire.logical_workspace == shown_a && desire.target_output.0 == "out-b"),
        "fresh backing desired: {:?}",
        drained.native_creations
    );
    // Two backing-less rounds with the vacated source still selected and
    // the target echo naming the old current: the arrival row and its
    // desire survive.
    for seq in [4, 5] {
        let reply = accepted(
            &mut engine,
            &report(
                seq,
                two_screens(),
                vec![fact("win-b", "out-b", 800, SB1)],
                vec![
                    native_obs("out-a", &[UA1, UA2, UA3], UA1),
                    native_obs("out-b", &[SB1, SB2, SB3], SB1),
                ],
                None,
                None,
                vec![],
                vec![],
                vec![],
            ),
        );
        assert!(
            engine.logical_ring("out-b").contains(&shown_a),
            "seq {seq}: arrival row survives: {:?}",
            engine.logical_ring("out-b")
        );
        assert!(
            reply
                .native_creations
                .iter()
                .any(|desire| desire.logical_workspace == shown_a
                    && desire.target_output.0 == "out-b"),
            "seq {seq}: backing desire survives: {:?}",
            reply.native_creations
        );
    }
}

#[test]
fn never_selected_vacated_source_adopts_at_order() {
    // Never-selected release discriminator: members arrive while the
    // source already shows the refill (the vacated desktop is never
    // selected after the move), yet the listed vacated desktop still
    // releases and adopts fresh at native-order position with no dormant
    // pair left behind. Adoption is by complete-order presence, never by
    // selection and never by vacancy inference.
    let (mut engine, migrated, refill_ws, _, _, _, _) = migrate_setup();
    // Bind the fresh backing first (creation correlation), with members
    // homed at the target and the source still on the old current.
    let landed = accepted(
        &mut engine,
        &report(
            3,
            two_screens(),
            vec![
                fact("win-a", "out-b", 800, SB_NEW),
                fact("win-b", "out-b", 800, SB1),
            ],
            vec![
                native_obs("out-a", &[UA1, UA2, UA3], UA1),
                native_obs("out-b", &[SB1, SB2, SB3, SB_NEW], SB1),
            ],
            None,
            None,
            vec![],
            vec![],
            vec![NativeCreationAttempt {
                operation_id: "op-native-1".to_owned(),
                logical_workspace: migrated.to_owned(),
                target_output: OutputId("out-b".to_owned()),
                setter: FrameSetter::Accepted,
                order_before: vec![SB1.to_owned(), SB2.to_owned(), SB3.to_owned()],
                order_after: vec![
                    SB1.to_owned(),
                    SB2.to_owned(),
                    SB3.to_owned(),
                    SB_NEW.to_owned(),
                ],
                created_native: Some(SB_NEW.to_owned()),
                attempt_ms: 3,
            }],
        ),
    );
    assert!(
        landed
            .transfers
            .iter()
            .any(|t| matches!(t.kind, tiler_core::world_report::TransferKind::Migration)),
        "members arrived but views old retains the migration: {:?}",
        landed.transfers
    );
    // The refill's native binding, resolved from the landed rows.
    let refill_native = landed
        .workspaces
        .iter()
        .find(|row| row.output.0 == "out-a" && row.id == refill_ws)
        .and_then(|row| row.native_binding.clone())
        .expect("refill stays bound");
    // Source switches to the refill with the vacated desktop never
    // selected again: the flight settles normally, and the listed
    // vacated desktop adopts fresh at order position on a later round.
    let settled = accepted(
        &mut engine,
        &report(
            4,
            two_screens(),
            vec![
                fact("win-a", "out-b", 800, SB_NEW),
                fact("win-b", "out-b", 800, SB1),
            ],
            vec![
                native_obs("out-a", &[UA1, UA2, UA3], &refill_native),
                native_obs("out-b", &[SB1, SB2, SB3, SB_NEW], SB_NEW),
            ],
            None,
            None,
            vec![],
            vec![],
            vec![],
        ),
    );
    assert!(
        settled.transfers.is_empty(),
        "refill echo settles normally: {:?}",
        settled.transfers
    );
    let member = settled
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-a")
        .expect("mover published");
    assert_eq!(member.output.0, "out-b", "migration stays: {member:?}");
    assert_eq!(member.workspace, migrated, "migration stays: {member:?}");
    // A later complete round adopts the never-selected vacated desktop
    // fresh at native-order head with no dormant pair remaining: UA1
    // binds a fresh id (adoption is impossible while mapped).
    let adopted = accepted(
        &mut engine,
        &report(
            5,
            two_screens(),
            vec![
                fact("win-a", "out-b", 800, SB_NEW),
                fact("win-b", "out-b", 800, SB1),
            ],
            vec![
                native_obs("out-a", &[UA1, UA2, UA3], &refill_native),
                native_obs("out-b", &[SB1, SB2, SB3, SB_NEW], SB_NEW),
            ],
            None,
            None,
            vec![],
            vec![],
            vec![],
        ),
    );
    assert!(
        adopted.transfers.is_empty(),
        "settled flight stays settled: {:?}",
        adopted.transfers
    );
    let fresh_row = adopted
        .workspaces
        .iter()
        .find(|row| row.output.0 == "out-a" && row.native_binding.as_deref() == Some(UA1))
        .expect("never-selected vacated desktop adopts fresh");
    assert_ne!(fresh_row.id, migrated, "fresh row is never the migrated id");
    assert_ne!(fresh_row.id, refill_ws, "fresh row is never the refill");
    assert_eq!(
        engine.logical_ring("out-a").first().map(String::as_str),
        Some(fresh_row.id.as_str()),
        "fresh row sits at its native-order position: {:?}",
        engine.logical_ring("out-a")
    );
}

#[test]
fn incomplete_roster_never_releases() {
    // Partial-enumeration discriminator: an arrival round with an
    // incomplete inventory (or partial screen list) holds without
    // releasing or adopting, even with the vacated desktop selected.
    // Release needs a complete native order because a partial
    // enumeration cannot tell a vacated desktop from an unreported one —
    // never from roster absence. The next complete round still releases
    // and adopts: the guard defers, never denies.
    let (mut engine, migrated, _, _, migration_op, refill_op, _) = migrate_setup();
    let mut gapped = report(
        3,
        two_screens(),
        vec![
            fact("win-a", "out-b", 800, SB_NEW),
            fact("win-b", "out-b", 800, SB1),
        ],
        vec![
            native_obs("out-a", &[UA1, UA2, UA3], UA1),
            native_obs("out-b", &[SB1, SB2, SB3, SB_NEW], SB1),
        ],
        None,
        None,
        vec![],
        vec![],
        vec![NativeCreationAttempt {
            operation_id: "op-native-1".to_owned(),
            logical_workspace: migrated.to_owned(),
            target_output: OutputId("out-b".to_owned()),
            setter: FrameSetter::Accepted,
            order_before: vec![SB1.to_owned(), SB2.to_owned(), SB3.to_owned()],
            order_after: vec![
                SB1.to_owned(),
                SB2.to_owned(),
                SB3.to_owned(),
                SB_NEW.to_owned(),
            ],
            created_native: Some(SB_NEW.to_owned()),
            attempt_ms: 3,
        }],
    );
    gapped.inventory_complete = false;
    let ring_before = engine.logical_ring("out-a").len() + engine.logical_ring("out-b").len();
    let held = accepted(&mut engine, &gapped);
    assert!(
        held.transfers
            .iter()
            .any(|t| t.operation_id == migration_op),
        "gapped arrival holds the migration: {:?}",
        held.transfers
    );
    assert!(
        held.transfers.iter().any(|t| t.operation_id == refill_op),
        "gapped arrival holds the refill: {:?}",
        held.transfers
    );
    assert_eq!(
        engine.logical_ring("out-a").len() + engine.logical_ring("out-b").len(),
        ring_before,
        "gapped roster mints no rows: {:?} {:?}",
        engine.logical_ring("out-a"),
        engine.logical_ring("out-b")
    );
    assert!(
        !held.workspaces.iter().any(|row| row.output.0 == "out-a"
            && row.native_binding.as_deref() == Some(UA1)
            && row.id != migrated),
        "gapped roster adopts no fresh row for the vacated desktop: {:?}",
        held.workspaces
    );
    // The next complete round releases and adopts: the guard defers on
    // freshness, never denies on absence. (The creation receipt rides
    // again: receipts repeat until incorporated.)
    let complete = accepted(
        &mut engine,
        &report(
            4,
            two_screens(),
            vec![
                fact("win-a", "out-b", 800, SB_NEW),
                fact("win-b", "out-b", 800, SB1),
            ],
            vec![
                native_obs("out-a", &[UA1, UA2, UA3], UA1),
                native_obs("out-b", &[SB1, SB2, SB3, SB_NEW], SB1),
            ],
            None,
            None,
            vec![],
            vec![],
            vec![NativeCreationAttempt {
                operation_id: "op-native-1".to_owned(),
                logical_workspace: migrated.to_owned(),
                target_output: OutputId("out-b".to_owned()),
                setter: FrameSetter::Accepted,
                order_before: vec![SB1.to_owned(), SB2.to_owned(), SB3.to_owned()],
                order_after: vec![
                    SB1.to_owned(),
                    SB2.to_owned(),
                    SB3.to_owned(),
                    SB_NEW.to_owned(),
                ],
                created_native: Some(SB_NEW.to_owned()),
                attempt_ms: 4,
            }],
        ),
    );
    let fresh_row = complete
        .workspaces
        .iter()
        .find(|row| row.output.0 == "out-a" && row.native_binding.as_deref() == Some(UA1))
        .expect("complete roster adopts the vacated desktop fresh");
    assert_ne!(fresh_row.id, migrated, "fresh row is never the migrated id");
}

#[test]
fn all_closed_migration_releases_source() {
    // All-closed exit: every member explicitly closed settles the
    // migration without waiting for views — and must still release the
    // source backing, since closed lifetimes never roll back. No dormant
    // pair may remain with the retired flight.
    let (mut engine, migrated, refill_ws, _, _, _, _) = migrate_setup();
    let mut closed_report = report(
        3,
        two_screens(),
        vec![fact("win-b", "out-b", 800, SB1)],
        vec![
            native_obs("out-a", &[UA1, UA2, UA3], UA1),
            native_obs("out-b", &[SB1, SB2, SB3], SB1),
        ],
        None,
        None,
        vec![],
        vec![],
        vec![],
    );
    closed_report.closed = vec![WindowId("win-a".to_owned())];
    let settled = accepted(&mut engine, &closed_report);
    assert!(
        settled.transfers.is_empty(),
        "closed migration settles: {:?}",
        settled.transfers
    );
    let fresh_row = settled
        .workspaces
        .iter()
        .find(|row| row.output.0 == "out-a" && row.native_binding.as_deref() == Some(UA1))
        .expect("closed migration still releases its source fresh");
    assert_ne!(fresh_row.id, migrated, "fresh row is never the migrated id");
    assert_ne!(fresh_row.id, refill_ws, "fresh row is never the refill");
    assert_eq!(
        engine.logical_ring("out-a").first().map(String::as_str),
        Some(fresh_row.id.as_str()),
        "fresh row sits at its native-order position: {:?}",
        engine.logical_ring("out-a")
    );
}

const SC1: &str = "space-21";
const SC2: &str = "space-22";

fn three_screens() -> Vec<ScreenReport> {
    vec![
        screen("out-a", 0, true),
        screen("out-b", 800, false),
        screen("out-c", 1600, false),
    ]
}

#[test]
fn off_output_move_releases_source() {
    // No-survivors exit: the lone member moves off both transfer screens
    // (positive newer input on a third output, not roster absence), so
    // the migration settles to observed — and must still release the
    // source backing, since nothing remains to roll back.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let before = accepted(
        &mut engine,
        &report(
            1,
            three_screens(),
            vec![
                fact("win-a", "out-a", 0, UA1),
                fact("win-b", "out-b", 800, SB1),
                fact("win-d", "out-c", 1600, SC1),
            ],
            vec![
                native_obs("out-a", &[UA1, UA2, UA3], UA1),
                native_obs("out-b", &[SB1, SB2, SB3], SB1),
                native_obs("out-c", &[SC1, SC2], SC1),
            ],
            None,
            None,
            vec![],
            vec![],
            vec![],
        ),
    );
    let shown_a = output_view(&before, "out-a").expect("out-a shows a workspace");
    let drained = accepted(
        &mut engine,
        &report(
            2,
            three_screens(),
            vec![
                fact("win-a", "out-a", 0, UA1),
                fact("win-b", "out-b", 800, SB1),
                fact("win-d", "out-c", 1600, SC1),
            ],
            vec![
                native_obs("out-a", &[UA1, UA2, UA3], UA1),
                native_obs("out-b", &[SB1, SB2, SB3], SB1),
                native_obs("out-c", &[SC1, SC2], SC1),
            ],
            Some(11),
            Some(WorldCommand::MigrateWorkspace {
                source_output: OutputId("out-a".to_owned()),
                target_output: OutputId("out-b".to_owned()),
                workspace: shown_a.clone(),
            }),
            vec![],
            vec![],
            vec![],
        ),
    );
    assert!(
        drained.transfers.iter().any(|t| t.operation_id == "op-11"),
        "migration retained: {:?}",
        drained.transfers
    );
    // The member moves off both transfer screens with complete orders:
    // positive newer input settles the flight and releases the source.
    let settled = accepted(
        &mut engine,
        &report(
            3,
            three_screens(),
            vec![
                fact("win-a", "out-c", 1600, SC1),
                fact("win-b", "out-b", 800, SB1),
                fact("win-d", "out-c", 1600, SC1),
            ],
            vec![
                native_obs("out-a", &[UA1, UA2, UA3], UA1),
                native_obs("out-b", &[SB1, SB2, SB3], SB1),
                native_obs("out-c", &[SC1, SC2], SC1),
            ],
            None,
            None,
            vec![],
            vec![],
            vec![],
        ),
    );
    assert!(
        settled.transfers.is_empty(),
        "off-output move settles: {:?}",
        settled.transfers
    );
    let fresh_row = settled
        .workspaces
        .iter()
        .find(|row| row.output.0 == "out-a" && row.native_binding.as_deref() == Some(UA1))
        .expect("off-output settlement still releases its source fresh");
    assert_ne!(fresh_row.id, shown_a, "fresh row is never the migrated id");
}

const UA_NEW: &str = "3f9a8c1e-4444-4b2a-9c3d-0000000000a4";

#[test]
fn deferred_release_completes_after_creation_resolves() {
    // Deferred-release pin: arrival with a pending creation on the
    // source output defers release (late desktops belong to the pending
    // creation), even when the flight settles through known echoes in
    // the meantime. The deferred work is retained and completes exactly
    // once when the creation resolves — settling never orphans a
    // dormant binding. Two crossing migrations stage this naturally:
    // op-12's backing desire pends on op-11's source output.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            two_screens(),
            vec![
                fact("win-a", "out-a", 0, UA1),
                fact("win-m", "out-b", 800, SB1),
            ],
            vec![
                native_obs("out-a", &[UA1, UA2, UA3], UA1),
                native_obs("out-b", &[SB1, SB2, SB3], SB1),
            ],
            None,
            None,
            vec![],
            vec![],
            vec![],
        ),
    );
    let shown_a = output_view(&first, "out-a").expect("out-a shows a workspace");
    let shown_b = output_view(&first, "out-b").expect("out-b shows a workspace");
    // Drain both directions: op-11 carries win-a A->B, op-12 carries
    // win-m B->A. Each arrival is unbound, so each mints a backing
    // desire on its own target — op-12's desire pends on op-11's source.
    let drain1 = accepted(
        &mut engine,
        &report(
            2,
            two_screens(),
            vec![
                fact("win-a", "out-a", 0, UA1),
                fact("win-m", "out-b", 800, SB1),
            ],
            vec![
                native_obs("out-a", &[UA1, UA2, UA3], UA1),
                native_obs("out-b", &[SB1, SB2, SB3], SB1),
            ],
            Some(11),
            Some(WorldCommand::MigrateWorkspace {
                source_output: OutputId("out-a".to_owned()),
                target_output: OutputId("out-b".to_owned()),
                workspace: shown_a.clone(),
            }),
            vec![],
            vec![],
            vec![],
        ),
    );
    let refill11_ws = drain1
        .transfers
        .iter()
        .find(|t| t.operation_id == "op-11-refill")
        .expect("refill retained")
        .target_workspace
        .clone();
    let drain2 = accepted(
        &mut engine,
        &report(
            3,
            two_screens(),
            vec![
                fact("win-a", "out-a", 0, UA1),
                fact("win-m", "out-b", 800, SB1),
            ],
            vec![
                native_obs("out-a", &[UA1, UA2, UA3], UA1),
                native_obs("out-b", &[SB1, SB2, SB3], SB1),
            ],
            Some(12),
            Some(WorldCommand::MigrateWorkspace {
                source_output: OutputId("out-b".to_owned()),
                target_output: OutputId("out-a".to_owned()),
                workspace: shown_b.clone(),
            }),
            vec![],
            vec![],
            vec![],
        ),
    );
    assert!(
        drain2.transfers.iter().any(|t| t.operation_id == "op-12"),
        "return migration retained: {:?}",
        drain2.transfers
    );
    let op_l = drain2
        .native_creations
        .iter()
        .find(|d| d.logical_workspace == shown_a && d.target_output.0 == "out-b")
        .expect("arrival backing desired")
        .operation_id
        .clone();
    let op_m = drain2
        .native_creations
        .iter()
        .find(|d| d.logical_workspace == shown_b && d.target_output.0 == "out-a")
        .expect("return backing desired")
        .operation_id
        .clone();
    let refill11_native = drain2
        .workspaces
        .iter()
        .find(|row| row.output.0 == "out-a" && row.id == refill11_ws)
        .and_then(|row| row.native_binding.clone())
        .expect("refill stays bound");
    let ring_a_len = engine.logical_ring("out-a").len();
    // Arrival with the return backing still pending on the source:
    // op-11's release defers (no fresh row yet) while both flights hold.
    let deferred = accepted(
        &mut engine,
        &report(
            4,
            two_screens(),
            vec![
                fact("win-a", "out-b", 800, SB_NEW),
                fact("win-m", "out-b", 800, SB1),
            ],
            vec![
                native_obs("out-a", &[UA1, UA2, UA3], &refill11_native),
                native_obs("out-b", &[SB1, SB2, SB3, SB_NEW], SB1),
            ],
            None,
            None,
            vec![],
            vec![],
            vec![NativeCreationAttempt {
                operation_id: op_l.clone(),
                logical_workspace: shown_a.clone(),
                target_output: OutputId("out-b".to_owned()),
                setter: FrameSetter::Accepted,
                order_before: vec![SB1.to_owned(), SB2.to_owned(), SB3.to_owned()],
                order_after: vec![
                    SB1.to_owned(),
                    SB2.to_owned(),
                    SB3.to_owned(),
                    SB_NEW.to_owned(),
                ],
                created_native: Some(SB_NEW.to_owned()),
                attempt_ms: 4,
            }],
        ),
    );
    assert!(
        deferred.transfers.iter().any(|t| t.operation_id == "op-11"),
        "deferred arrival holds op-11: {:?}",
        deferred.transfers
    );
    assert!(
        deferred.transfers.iter().any(|t| t.operation_id == "op-12"),
        "outstanding return holds op-12: {:?}",
        deferred.transfers
    );
    assert_eq!(
        engine.logical_ring("out-a").len(),
        ring_a_len,
        "deferred release mints no rows: {:?}",
        engine.logical_ring("out-a")
    );
    assert!(
        !deferred
            .workspaces
            .iter()
            .any(|row| row.output.0 == "out-a" && row.native_binding.as_deref() == Some(UA1)),
        "deferred release adopts nothing yet: {:?}",
        deferred.workspaces
    );
    // The return backing correlates: the pending creation resolves, so
    // the retained release completes exactly once on the next intake.
    let resolved = accepted(
        &mut engine,
        &report(
            5,
            two_screens(),
            vec![
                fact("win-a", "out-b", 800, SB_NEW),
                fact("win-m", "out-b", 800, SB1),
            ],
            vec![
                native_obs("out-a", &[UA1, UA2, UA3, UA_NEW], &refill11_native),
                native_obs("out-b", &[SB1, SB2, SB3, SB_NEW], SB1),
            ],
            None,
            None,
            vec![],
            vec![],
            vec![NativeCreationAttempt {
                operation_id: op_m.clone(),
                logical_workspace: shown_b.clone(),
                target_output: OutputId("out-a".to_owned()),
                setter: FrameSetter::Accepted,
                order_before: vec![UA1.to_owned(), UA2.to_owned(), UA3.to_owned()],
                order_after: vec![
                    UA1.to_owned(),
                    UA2.to_owned(),
                    UA3.to_owned(),
                    UA_NEW.to_owned(),
                ],
                created_native: Some(UA_NEW.to_owned()),
                attempt_ms: 5,
            }],
        ),
    );
    let fresh_row = resolved
        .workspaces
        .iter()
        .find(|row| row.output.0 == "out-a" && row.native_binding.as_deref() == Some(UA1))
        .expect("resolved creation completes the retained release fresh");
    assert_ne!(fresh_row.id, shown_a, "fresh row is never the migrated id");
    assert_ne!(fresh_row.id, refill11_ws, "fresh row is never the refill");
    assert_eq!(
        engine.logical_ring("out-a").first().map(String::as_str),
        Some(fresh_row.id.as_str()),
        "fresh row sits at its native-order position: {:?}",
        engine.logical_ring("out-a")
    );
    assert!(
        resolved.transfers.iter().any(|t| t.operation_id == "op-11"),
        "op-11 still held on target lag: {:?}",
        resolved.transfers
    );
    assert!(
        resolved.transfers.iter().any(|t| t.operation_id == "op-12"),
        "op-12 still held while its member is outstanding: {:?}",
        resolved.transfers
    );
    // A further round mints nothing more: exactly-once release, no
    // orphan binding, no second row.
    let stable = accepted(
        &mut engine,
        &report(
            6,
            two_screens(),
            vec![
                fact("win-a", "out-b", 800, SB_NEW),
                fact("win-m", "out-b", 800, SB1),
            ],
            vec![
                native_obs("out-a", &[UA1, UA2, UA3, UA_NEW], &refill11_native),
                native_obs("out-b", &[SB1, SB2, SB3, SB_NEW], SB1),
            ],
            None,
            None,
            vec![],
            vec![],
            vec![],
        ),
    );
    assert_eq!(
        engine.logical_ring("out-a").len(),
        ring_a_len + 1,
        "exactly one fresh row total: {:?}",
        engine.logical_ring("out-a")
    );
    assert_eq!(
        stable
            .workspaces
            .iter()
            .filter(|row| row.output.0 == "out-a" && row.native_binding.as_deref() == Some(UA1))
            .count(),
        1,
        "exactly one binding for the vacated desktop: {:?}",
        stable.workspaces
    );
    assert_eq!(
        output_view(&stable, "out-a").as_deref(),
        Some(shown_b.as_str()),
        "non-current adoption leaves the shown view alone"
    );
}

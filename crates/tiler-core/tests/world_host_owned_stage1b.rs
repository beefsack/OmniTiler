//! Host-owned native adoption Stage 1b tests: native-only receipts,
//! strict `current_native` nulls, and core-owned creation desires with raw
//! creation attempts. Opaque native ids (UUID-like and numeric Space ids);
//! fullscreen Spaces are never ordered desktops.

use tiler_core::directional::{OutputId, WindowId};
use tiler_core::geometry::Rect;
use tiler_core::size_hints::WindowSizeHints;
use tiler_core::workspace_scope::WorkspaceMode;
use tiler_core::world_engine::{WorldEngine, WorldOutcome};
use tiler_core::world_report::{
    FrameSetter, HoldReason, NativeCreationAttempt, NativeObservation, ScopeAuthority,
    ScreenReport, ScreenView, TransferKind, WORLD_CONTRACT_VERSION, WindowFact, WorldCommand,
    WorldReport,
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

const D1: &str = "3f9a8c1e-1111-4b2a-9c3d-0000000000d1";
const D2: &str = "3f9a8c1e-2222-4b2a-9c3d-0000000000d2";
const D3: &str = "7";
const D4: &str = "12";

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

fn fullscreen_native(window: &str, output: &str, frame: Rect, native: Option<&str>) -> WindowFact {
    WindowFact {
        fullscreen: true,
        fit_excluded: false,
        ..fact_native(window, output, frame, native)
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
fn creation_attempt(
    op: &str,
    ws: &str,
    out: &str,
    setter: FrameSetter,
    before: &[&str],
    after: &[&str],
    created: Option<&str>,
    ms: u64,
) -> NativeCreationAttempt {
    NativeCreationAttempt {
        operation_id: op.to_owned(),
        logical_workspace: ws.to_owned(),
        target_output: OutputId(out.to_owned()),
        setter,
        order_before: before.iter().map(|s| (*s).to_owned()).collect(),
        order_after: after.iter().map(|s| (*s).to_owned()).collect(),
        created_native: created.map(str::to_owned),
        attempt_ms: ms,
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
        native,
        fixed_size_predicate: tiler_core::size_hints::FixedSizePredicate::BothAxes,
        migration_source_refill: tiler_core::output_selection::MigrationSourceRefill::LastRemaining,
        anchor_fallback: tiler_core::admission::AnchorFallback::default(),
        no_space_result: tiler_core::admission::NoSpaceResult::default(),
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

fn bound_ids(reply: &tiler_core::world_report::WorldReply, output: &str) -> Vec<(String, String)> {
    reply
        .workspaces
        .iter()
        .filter(|row| row.output.0 == output && row.native_binding.is_some())
        .map(|row| (row.id.clone(), row.native_binding.clone().expect("bound")))
        .collect()
}

fn workspace_hold(
    reply: &tiler_core::world_report::WorldReply,
    output: &str,
    id: &str,
) -> Option<HoldReason> {
    reply
        .workspaces
        .iter()
        .find(|row| row.output.0 == output && row.id == id)
        .and_then(|row| row.hold)
}

#[test]
fn kde_four_desktops_preserve_spare_and_mint_no_creation() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let reply = accepted(
        &mut engine,
        &report(
            1,
            1,
            one_screen(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 200, 560), Some(D1)),
                fact_native("win-b", "out-a", rect(200, 0, 200, 560), Some(D2)),
                fact_native("win-c", "out-a", rect(400, 0, 200, 560), Some(D3)),
                fact_native("win-d", "out-a", rect(600, 0, 200, 560), Some(D4)),
            ],
            vec![native_obs("out-a", &[D1, D2, D3, D4], Some(D1))],
            vec![],
            None,
            None,
            true,
        ),
    );
    let bound = bound_ids(&reply, "out-a");
    assert_eq!(
        bound.iter().map(|(_, n)| n.as_str()).collect::<Vec<_>>(),
        vec![D1, D2, D3, D4],
        "native order preserved"
    );
    let total: Vec<_> = reply
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .collect();
    assert_eq!(total.len(), 5, "four bound plus the ordinary spare");
    assert!(
        total.last().is_some_and(|row| row.native_binding.is_none()),
        "spare stays unbound"
    );
    assert!(
        reply.native_creations.is_empty(),
        "no backing missing, no creation desired"
    );
    assert!(reply.transfers.is_empty());
}

#[test]
fn fullscreen_space_never_adopted_and_home_retained_without_writes() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            one_screen(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(D1)),
                fact_native("win-b", "out-a", rect(400, 0, 200, 560), Some(D2)),
            ],
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
    // A fullscreen window on its own (unlisted) Space: unknown membership,
    // never a new workspace, never a move.
    let second = accepted(
        &mut engine,
        &report(
            2,
            2,
            one_screen(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(D1)),
                fact_native("win-b", "out-a", rect(400, 0, 200, 560), Some(D2)),
                fullscreen_native("win-g", "out-a", rect(0, 0, 800, 560), None),
            ],
            vec![native_obs("out-a", &[D1, D2], Some(D1))],
            vec![],
            None,
            None,
            true,
        ),
    );
    assert_eq!(
        bound_ids(&second, "out-a").len(),
        2,
        "fullscreen Space adopts nothing"
    );
    assert!(
        second.windows.iter().all(|row| row.lifetime.0 != "win-g"),
        "unknown membership publishes no row, never a guess"
    );
    assert!(second.transfers.is_empty());
    // The game exits fullscreen onto D2: homed where observed, no writes
    // blessed for the overlay itself.
    let third = accepted(
        &mut engine,
        &report(
            3,
            3,
            one_screen(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(D1)),
                fact_native("win-b", "out-a", rect(400, 0, 200, 560), Some(D2)),
                fullscreen_native("win-g", "out-a", rect(0, 0, 800, 560), Some(D2)),
            ],
            vec![native_obs("out-a", &[D1, D2], Some(D1))],
            vec![],
            None,
            None,
            true,
        ),
    );
    let row_g = third
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-g")
        .expect("homed where observed");
    assert_eq!(row_g.workspace, bound[D2]);
    assert!(
        row_g.effective_frame.is_none(),
        "overlay owns pixels, no managed frame"
    );
    assert!(third.transfers.is_empty());
}

#[test]
fn fullscreen_current_none_retains_view_and_uses_order() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
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
            true,
        ),
    );
    let view = first
        .outputs
        .iter()
        .find(|row| row.output.0 == "out-a")
        .and_then(|row| row.logical_view.clone())
        .expect("view");
    // The current surface is a fullscreen Space (not an ordered desktop):
    // the order still applies, the retained logical view stays.
    let second = accepted(
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
            vec![native_obs("out-a", &[D1, D2], None)],
            vec![],
            None,
            None,
            true,
        ),
    );
    assert_eq!(bound_ids(&second, "out-a").len(), 2, "order still binds");
    let held_view = second
        .outputs
        .iter()
        .find(|row| row.output.0 == "out-a")
        .and_then(|row| row.logical_view.clone())
        .expect("retained view");
    assert_eq!(held_view, view, "None current never moves the view");
    assert!(second.transfers.is_empty());
    assert!(second.native_creations.is_empty());
}

#[test]
fn send_to_new_workspace_binds_late_desktop_exactly_once() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            one_screen(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(D1)),
                fact_native("win-b", "out-a", rect(400, 0, 200, 560), Some(D2)),
            ],
            vec![native_obs("out-a", &[D1, D2], Some(D1))],
            vec![],
            None,
            None,
            true,
        ),
    );
    let spare_id = first
        .workspaces
        .iter()
        .find(|row| row.output.0 == "out-a" && row.native_binding.is_none())
        .map(|row| row.id.clone())
        .expect("spare");
    // Occupy the spare: the committed logical has no native backing yet,
    // so the core desires exactly one creation (stable op, explicit only).
    let desired = accepted(
        &mut engine,
        &report(
            2,
            2,
            one_screen(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(D1)),
                fact_native("win-b", "out-a", rect(400, 0, 200, 560), Some(D2)),
            ],
            vec![native_obs("out-a", &[D1, D2], Some(D1))],
            vec![],
            Some(11),
            Some(WorldCommand::Send {
                window: WindowId("win-a".to_owned()),
                target_output: OutputId("out-a".to_owned()),
                target_workspace: spare_id.clone(),
                follow: false,
            }),
            true,
        ),
    );
    assert_eq!(desired.native_creations.len(), 1, "one explicit desire");
    let op = desired.native_creations[0].operation_id.clone();
    assert!(
        op.starts_with("op-native-"),
        "core serial, not a command seq"
    );
    assert_eq!(desired.native_creations[0].logical_workspace, spare_id);
    assert_eq!(workspace_hold(&desired, "out-a", &spare_id), None);
    // The created desktop appears late with its raw attempt: bound to the
    // pending logical at native-order position, never adopted as outside.
    const DN: &str = "9f1c-new-space";
    let settled = accepted(
        &mut engine,
        &report(
            3,
            3,
            one_screen(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(D1)),
                fact_native("win-b", "out-a", rect(400, 0, 200, 560), Some(D2)),
            ],
            vec![native_obs("out-a", &[D1, D2, DN], Some(D1))],
            vec![creation_attempt(
                &op,
                &spare_id,
                "out-a",
                FrameSetter::Accepted,
                &[D1, D2],
                &[D1, D2, DN],
                Some(DN),
                7,
            )],
            None,
            None,
            true,
        ),
    );
    let bound: std::collections::BTreeMap<String, String> = bound_ids(&settled, "out-a")
        .into_iter()
        .map(|(id, n)| (n, id))
        .collect();
    assert_eq!(
        bound[DN], spare_id,
        "late desktop belongs to the pending creation"
    );
    assert_eq!(bound.len(), 3, "exactly one new workspace, no duplicate");
    assert!(
        settled.native_creations.is_empty(),
        "bound creations retire"
    );
    // Replay idempotent: the same evidence on a later report binds nothing
    // new and mints no replacement operation.
    let replay = accepted(
        &mut engine,
        &report(
            4,
            4,
            one_screen(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(D1)),
                fact_native("win-b", "out-a", rect(400, 0, 200, 560), Some(D2)),
            ],
            vec![native_obs("out-a", &[D1, D2, DN], Some(D1))],
            vec![creation_attempt(
                &op,
                &spare_id,
                "out-a",
                FrameSetter::Accepted,
                &[D1, D2],
                &[D1, D2, DN],
                Some(DN),
                7,
            )],
            None,
            None,
            true,
        ),
    );
    let again: std::collections::BTreeMap<String, String> = bound_ids(&replay, "out-a")
        .into_iter()
        .map(|(id, n)| (n, id))
        .collect();
    assert_eq!(again, bound, "replay binds nothing new");
    assert!(replay.native_creations.is_empty());
}

#[test]
fn ambiguous_creation_holds_and_never_blind_recreates() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            one_screen(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(D1)),
                fact_native("win-b", "out-a", rect(400, 0, 200, 560), Some(D2)),
            ],
            vec![native_obs("out-a", &[D1, D2], Some(D1))],
            vec![],
            None,
            None,
            true,
        ),
    );
    let spare_id = first
        .workspaces
        .iter()
        .find(|row| row.output.0 == "out-a" && row.native_binding.is_none())
        .map(|row| row.id.clone())
        .expect("spare");
    let desired = accepted(
        &mut engine,
        &report(
            2,
            2,
            one_screen(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(D1)),
                fact_native("win-b", "out-a", rect(400, 0, 200, 560), Some(D2)),
            ],
            vec![native_obs("out-a", &[D1, D2], Some(D1))],
            vec![],
            Some(11),
            Some(WorldCommand::Send {
                window: WindowId("win-a".to_owned()),
                target_output: OutputId("out-a".to_owned()),
                target_workspace: spare_id.clone(),
                follow: false,
            }),
            true,
        ),
    );
    let op = desired.native_creations[0].operation_id.clone();
    // No Space observed: the desire stays under the same op, held without
    // proof, and no extra workspace appears.
    let waiting = accepted(
        &mut engine,
        &report(
            3,
            3,
            one_screen(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(D1)),
                fact_native("win-b", "out-a", rect(400, 0, 200, 560), Some(D2)),
            ],
            vec![native_obs("out-a", &[D1, D2], Some(D1))],
            vec![],
            None,
            None,
            true,
        ),
    );
    assert_eq!(
        waiting
            .native_creations
            .iter()
            .map(|d| &d.operation_id)
            .collect::<Vec<_>>(),
        vec![&op],
        "same authorization, no blind recreate"
    );
    assert_eq!(bound_ids(&waiting, "out-a").len(), 2);
    // The host refuses the creation: ambiguity latches on the row, and the
    // op is never replaced by a fresh one.
    let refused = accepted(
        &mut engine,
        &report(
            4,
            4,
            one_screen(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(D1)),
                fact_native("win-b", "out-a", rect(400, 0, 200, 560), Some(D2)),
            ],
            vec![native_obs("out-a", &[D1, D2], Some(D1))],
            vec![creation_attempt(
                &op,
                &spare_id,
                "out-a",
                FrameSetter::Refused,
                &[D1, D2],
                &[D1, D2],
                None,
                9,
            )],
            None,
            None,
            true,
        ),
    );
    assert_eq!(
        workspace_hold(&refused, "out-a", &spare_id),
        Some(HoldReason::AmbiguousCreation),
        "AmbiguousCreation has a producer"
    );
    assert!(
        refused.native_creations.is_empty(),
        "consumed attempt republishes no authorization"
    );
    // A later report without the attempt row keeps the same op and hold.
    let again = accepted(
        &mut engine,
        &report(
            5,
            5,
            one_screen(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(D1)),
                fact_native("win-b", "out-a", rect(400, 0, 200, 560), Some(D2)),
            ],
            vec![native_obs("out-a", &[D1, D2], Some(D1))],
            vec![],
            None,
            None,
            true,
        ),
    );
    assert!(
        again.native_creations.is_empty(),
        "consumed attempt republishes no authorization"
    );
    assert_eq!(
        workspace_hold(&again, "out-a", &spare_id),
        Some(HoldReason::AmbiguousCreation)
    );
    assert_eq!(bound_ids(&again, "out-a").len(), 2, "no extra workspace");
}

#[test]
fn creation_collision_holds_without_guessing() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            one_screen(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(D1)),
                fact_native("win-b", "out-a", rect(400, 0, 200, 560), Some(D2)),
            ],
            vec![native_obs("out-a", &[D1, D2], Some(D1))],
            vec![],
            None,
            None,
            true,
        ),
    );
    let spare_id = first
        .workspaces
        .iter()
        .find(|row| row.output.0 == "out-a" && row.native_binding.is_none())
        .map(|row| row.id.clone())
        .expect("spare");
    let desired = accepted(
        &mut engine,
        &report(
            2,
            2,
            one_screen(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(D1)),
                fact_native("win-b", "out-a", rect(400, 0, 200, 560), Some(D2)),
            ],
            vec![native_obs("out-a", &[D1, D2], Some(D1))],
            vec![],
            Some(11),
            Some(WorldCommand::Send {
                window: WindowId("win-a".to_owned()),
                target_output: OutputId("out-a".to_owned()),
                target_workspace: spare_id.clone(),
                follow: false,
            }),
            true,
        ),
    );
    let op = desired.native_creations[0].operation_id.clone();
    // Two outside candidates against one outstanding creation: hold, bind
    // nothing, adopt nothing.
    const CX: &str = "aa-outside-x";
    const CY: &str = "bb-outside-y";
    let held = accepted(
        &mut engine,
        &report(
            3,
            3,
            one_screen(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(D1)),
                fact_native("win-b", "out-a", rect(400, 0, 200, 560), Some(D2)),
            ],
            vec![native_obs("out-a", &[D1, D2, CX, CY], Some(D1))],
            vec![creation_attempt(
                &op,
                &spare_id,
                "out-a",
                FrameSetter::Accepted,
                &[D1, D2],
                &[D1, D2, CX, CY],
                None,
                11,
            )],
            None,
            None,
            true,
        ),
    );
    assert_eq!(
        bound_ids(&held, "out-a").len(),
        2,
        "collision binds nothing and adopts nothing"
    );
    assert_eq!(
        workspace_hold(&held, "out-a", &spare_id),
        Some(HoldReason::AmbiguousCreation),
        "collision holds"
    );
    assert!(
        held.native_creations.is_empty(),
        "consumed attempt republishes no authorization"
    );
}

#[test]
fn startup_unknown_membership_holds_then_homes_without_rerun() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    // Startup with partial membership: win-x unknown, win-a known. No
    // move at launch, no row for the unknown window.
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            one_screen(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(D1)),
                fact_native("win-x", "out-a", rect(100, 100, 200, 150), None),
            ],
            vec![native_obs("out-a", &[D1, D2], Some(D1))],
            vec![],
            None,
            None,
            true,
        ),
    );
    assert!(
        first.windows.iter().all(|row| row.lifetime.0 != "win-x"),
        "unknown membership waits, never defaults"
    );
    assert!(first.transfers.is_empty());
    let alloc_a = first
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-a")
        .and_then(|row| row.allocation)
        .expect("win-a placed");
    // Membership completes: win-x homes where observed, win-a untouched.
    let second = accepted(
        &mut engine,
        &report(
            2,
            2,
            one_screen(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(D1)),
                fact_native("win-x", "out-a", rect(100, 100, 200, 150), Some(D2)),
            ],
            vec![native_obs("out-a", &[D1, D2], Some(D1))],
            vec![],
            None,
            None,
            true,
        ),
    );
    let bound: std::collections::BTreeMap<String, String> = bound_ids(&second, "out-a")
        .into_iter()
        .map(|(id, n)| (n, id))
        .collect();
    let row_x = second
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-x")
        .expect("homed once observed");
    assert_eq!(row_x.workspace, bound[D2]);
    assert_eq!(row_x.membership_authority, ScopeAuthority::Controlled);
    let again_a = second
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-a")
        .and_then(|row| row.allocation)
        .expect("win-a kept");
    assert_eq!(again_a, alloc_a, "no destructive launch rerun");
    assert!(second.transfers.is_empty());
}

#[test]
fn delayed_send_settles_while_unmatched_outside_evidence_adopts() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            one_screen(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(D1)),
                fact_native("win-b", "out-a", rect(400, 0, 200, 560), Some(D1)),
            ],
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
    accepted(
        &mut engine,
        &report(
            2,
            2,
            one_screen(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(D1)),
                fact_native("win-b", "out-a", rect(400, 0, 200, 560), Some(D1)),
            ],
            vec![native_obs("out-a", &[D1, D2], Some(D1))],
            vec![],
            Some(11),
            Some(WorldCommand::Send {
                window: WindowId("win-a".to_owned()),
                target_output: OutputId("out-a".to_owned()),
                target_workspace: bound[D2].clone(),
                follow: false,
            }),
            true,
        ),
    );
    // Intermediate reading with unknown membership: the flight holds, and
    // an unrelated newcomer with exact proof is adopted on its own
    // evidence (unmatched outside input, never the flight's).
    let mid = accepted(
        &mut engine,
        &report(
            3,
            3,
            one_screen(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), None),
                fact_native("win-b", "out-a", rect(400, 0, 200, 560), Some(D1)),
                fact_native("win-c", "out-a", rect(0, 0, 800, 560), Some(D2)),
            ],
            vec![native_obs("out-a", &[D1, D2], Some(D1))],
            vec![],
            None,
            None,
            true,
        ),
    );
    assert_eq!(mid.transfers.len(), 1, "intermediate holds the flight");
    let row_c = mid
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-c")
        .expect("outside evidence adopts");
    assert_eq!(row_c.workspace, bound[D2]);
    // Exact native receipt settles the flight instead of adopting an
    // outside move.
    let settled = accepted(
        &mut engine,
        &report(
            4,
            4,
            one_screen(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 800, 560), Some(D2)),
                fact_native("win-b", "out-a", rect(0, 0, 800, 560), Some(D1)),
                fact_native("win-c", "out-a", rect(0, 0, 800, 560), Some(D2)),
            ],
            vec![native_obs("out-a", &[D1, D2], Some(D1))],
            vec![],
            None,
            None,
            true,
        ),
    );
    assert!(settled.transfers.is_empty(), "exact receipt settles");
}

#[test]
fn spanning_identical_order_shares_one_ring_without_duplicates() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let screens = vec![screen("out-a", 0, true), screen("out-b", 800, false)];
    let mut r = report(
        1,
        1,
        screens,
        vec![
            fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(D1)),
            fact_native("win-b", "out-b", rect(800, 0, 400, 560), Some(D2)),
        ],
        vec![
            native_obs("out-a", &[D1, D2], Some(D1)),
            native_obs("out-b", &[D1, D2], Some(D1)),
        ],
        vec![],
        None,
        None,
        true,
    );
    r.workspace_mode = WorkspaceMode::Shared;
    let reply = accepted(&mut engine, &r);
    let ring_a: Vec<String> = reply
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| row.id.clone())
        .collect();
    let ring_b: Vec<String> = reply
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-b")
        .map(|row| row.id.clone())
        .collect();
    assert_eq!(ring_a, ring_b, "one spanning domain");
    let mut sorted = ring_a.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), ring_a.len(), "no per-output duplicates");
    assert_eq!(bound_ids(&reply, "out-a").len(), 2);
    assert_eq!(bound_ids(&reply, "out-b").len(), 2);
    assert!(reply.native_creations.is_empty());
}

#[test]
fn outside_desktop_binds_at_native_position_not_appended() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    accepted(
        &mut engine,
        &report(
            1,
            1,
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
            true,
        ),
    );
    // D3 appears between D1 and D2 in observed order with a member.
    const MID: &str = "5";
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            one_screen(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(D1)),
                fact_native("win-n", "out-a", rect(0, 0, 800, 560), Some(MID)),
            ],
            vec![native_obs("out-a", &[D1, MID, D2], Some(D1))],
            vec![],
            None,
            None,
            true,
        ),
    );
    let order: Vec<(String, String)> = reply
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| {
            (
                row.id.clone(),
                row.native_binding.clone().unwrap_or_default(),
            )
        })
        .collect();
    let positions: Vec<&str> = order.iter().map(|(_, n)| n.as_str()).collect();
    // All three bound in observed order; no spare is appended while the
    // bound tail stays empty (retained rows hold, never removed).
    assert_eq!(
        positions,
        vec![D1, MID, D2],
        "logical order follows observed native order: {order:?}"
    );
    assert!(reply.native_creations.is_empty());
}

#[test]
fn reset_readopts_and_vanished_complete_removal_without_rename() {
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let windows = vec![
        fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(D1)),
        fact_native("win-b", "out-a", rect(400, 0, 200, 560), Some(D2)),
    ];
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            one_screen(),
            windows.clone(),
            vec![native_obs("out-a", &[D1, D2], Some(D1))],
            vec![],
            None,
            None,
            true,
        ),
    );
    let order: Vec<String> = bound_ids(&first, "out-a")
        .into_iter()
        .map(|(_, n)| n)
        .collect();
    assert_eq!(order, vec![D1, D2]);
    // A fresh core generation re-adopts the same observed order.
    let mut reset = WorldEngine::new("gen-1").expect("generation");
    let readopted = accepted(
        &mut reset,
        &report(
            1,
            1,
            one_screen(),
            windows.clone(),
            vec![native_obs("out-a", &[D1, D2], Some(D1))],
            vec![],
            None,
            None,
            true,
        ),
    );
    let again: Vec<String> = bound_ids(&readopted, "out-a")
        .into_iter()
        .map(|(_, n)| n)
        .collect();
    assert_eq!(again, vec![D1, D2], "reset re-adopts in observed order");
    // Partial snapshot: vanished id stays (no removal from a gap).
    let partial = accepted(
        &mut reset,
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
            vec![native_obs("out-a", &[D1], Some(D1))],
            vec![],
            None,
            None,
            false,
        ),
    );
    assert_eq!(
        bound_ids(&partial, "out-a").len(),
        2,
        "partial never removes"
    );
    // Absent native row: never a deletion either.
    let mut absent = report(
        3,
        3,
        one_screen(),
        vec![fact_native(
            "win-a",
            "out-a",
            rect(0, 0, 400, 560),
            Some(D1),
        )],
        vec![],
        vec![],
        None,
        None,
        true,
    );
    let held = accepted(&mut reset, &absent);
    assert_eq!(
        bound_ids(&held, "out-a").len(),
        2,
        "absent rows never delete"
    );
    let _ = &mut absent;
    // Complete snapshot with an explicit close: the vanished id is a
    // removal, never a rename of the survivor.
    let mut gone = report(
        4,
        4,
        one_screen(),
        vec![fact_native(
            "win-a",
            "out-a",
            rect(0, 0, 400, 560),
            Some(D1),
        )],
        vec![native_obs("out-a", &[D1], Some(D1))],
        vec![],
        None,
        None,
        true,
    );
    gone.closed = vec![WindowId("win-b".to_owned())];
    let removed = accepted(&mut reset, &gone);
    let remaining: Vec<(String, String)> = removed
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| {
            (
                row.id.clone(),
                row.native_binding.clone().unwrap_or_default(),
            )
        })
        .collect();
    assert!(
        remaining.iter().all(|(_, n)| n != D2),
        "vanished id removed: {remaining:?}"
    );
    assert!(
        remaining.iter().any(|(_, n)| n == D1),
        "survivor keeps its binding, never renamed: {remaining:?}"
    );
    assert!(
        removed.native_creations.is_empty(),
        "removal mints no creation"
    );
}

#[test]
fn replaced_desktop_is_deletion_plus_outside_creation_never_rename() {
    // A vanished id within the generation is removal, never rename: the
    // host retires desktop D2 and the same complete snapshot already shows
    // X. X binds a fresh logical (never the retired id), the window homes
    // from its native X proof, and D2 stays unbound everywhere.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            one_screen(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(D1)),
                fact_native("win-b", "out-a", rect(400, 0, 200, 560), Some(D2)),
            ],
            vec![native_obs("out-a", &[D1, D2], Some(D2))],
            vec![],
            None,
            None,
            true,
        ),
    );
    let retired: String = bound_ids(&first, "out-a")
        .into_iter()
        .find(|(_, n)| n == D2)
        .map(|(id, _)| id)
        .expect("retired logical");
    const X: &str = "replacement-x";
    let rebound = accepted(
        &mut engine,
        &report(
            2,
            2,
            one_screen(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(D1)),
                fact_native("win-b", "out-a", rect(400, 0, 200, 560), Some(X)),
            ],
            vec![native_obs("out-a", &[D1, X], Some(D1))],
            vec![],
            None,
            None,
            true,
        ),
    );
    let bound: std::collections::BTreeMap<String, String> = bound_ids(&rebound, "out-a")
        .into_iter()
        .map(|(id, n)| (n, id))
        .collect();
    assert!(
        !bound.values().any(|id| id == &retired),
        "retired id binds nothing: {bound:?}"
    );
    let fresh = bound.get(X).expect("X binds a logical");
    assert_ne!(
        fresh, &retired,
        "outside creation mints fresh, never renames the vanished id"
    );
    assert!(
        rebound.native_creations.is_empty(),
        "outside adoption runs no creation route"
    );
    let row_b = rebound
        .windows
        .iter()
        .find(|row| row.lifetime.0 == "win-b")
        .expect("member kept");
    assert_eq!(row_b.workspace, *fresh, "window homes from native X proof");
    assert!(rebound.transfers.is_empty());
}

#[test]
fn migrated_workspace_desires_creation_then_binds_late_desktop() {
    // Otherwise-create case: no unmapped desktop on the target, so the
    // core desires exactly one creation for the migrated logical; the late
    // desktop then binds it.
    let screens = vec![screen("out-a", 0, true), screen("out-b", 800, false)];
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            screens.clone(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(D1)),
                fact_native("win-c", "out-b", rect(800, 0, 400, 560), Some(D3)),
            ],
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
    let view_a = first
        .outputs
        .iter()
        .find(|row| row.output.0 == "out-a")
        .and_then(|row| row.logical_view.clone())
        .expect("view a");
    let migrated = accepted(
        &mut engine,
        &report(
            2,
            2,
            screens.clone(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(D1)),
                fact_native("win-c", "out-b", rect(800, 0, 400, 560), Some(D3)),
            ],
            vec![
                native_obs("out-a", &[D1, D2], Some(D1)),
                native_obs("out-b", &[D3], Some(D3)),
            ],
            vec![],
            Some(11),
            Some(WorldCommand::MigrateWorkspace {
                source_output: OutputId("out-a".to_owned()),
                target_output: OutputId("out-b".to_owned()),
                workspace: view_a.clone(),
            }),
            true,
        ),
    );
    let desire = migrated
        .native_creations
        .iter()
        .find(|d| d.logical_workspace == view_a && d.target_output.0 == "out-b")
        .expect("migrated backing desired");
    assert!(desire.operation_id.starts_with("op-native-"));
    const LATE: &str = "late-native-4";
    // The stored source refill rides the reply: both views must prove for
    // the migration to settle (target shows the migrated logical now that
    // the created desktop is current).
    let refill_a = migrated
        .outputs
        .iter()
        .find(|row| row.output.0 == "out-a")
        .and_then(|row| row.logical_view.clone())
        .expect("source refill");
    let mut step3 = report(
        3,
        3,
        screens.clone(),
        vec![
            fact_native("win-a", "out-b", rect(800, 0, 400, 560), Some(LATE)),
            fact_native("win-c", "out-b", rect(800, 0, 400, 560), Some(D3)),
        ],
        vec![
            native_obs("out-a", &[D2], Some(D2)),
            native_obs("out-b", &[D3, LATE], Some(LATE)),
        ],
        vec![creation_attempt(
            &desire.operation_id,
            &view_a,
            "out-b",
            FrameSetter::Accepted,
            &[D3],
            &[D3, LATE],
            Some(LATE),
            5,
        )],
        None,
        None,
        true,
    );
    step3.views = vec![
        ScreenView {
            output: OutputId("out-a".to_owned()),
            view: refill_a,
        },
        ScreenView {
            output: OutputId("out-b".to_owned()),
            view: view_a.clone(),
        },
    ];
    let settled = accepted(&mut engine, &step3);
    let rebound: Vec<(String, Option<String>)> = settled
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-b")
        .map(|row| (row.id.clone(), row.native_binding.clone()))
        .collect();
    assert!(
        rebound
            .iter()
            .any(|(id, bind)| id == &view_a && bind.as_deref() == Some(LATE)),
        "late desktop backs the migrated logical: {rebound:?}"
    );
    assert!(settled.native_creations.is_empty());
    assert!(
        !settled
            .transfers
            .iter()
            .any(|t| matches!(t.kind, TransferKind::Migration)),
        "migration members arrived with proof: {:?}",
        settled.transfers
    );
}

/// Occupy the spare and return its logical id plus the creation op.
/// Helper for the review-hardening sequences below.
fn occupy_spare(engine: &mut WorldEngine, spare_seq: u64) -> (String, String) {
    let first = accepted(
        engine,
        &report(
            1,
            1,
            one_screen(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(D1)),
                fact_native("win-b", "out-a", rect(400, 0, 200, 560), Some(D2)),
            ],
            vec![native_obs("out-a", &[D1, D2], Some(D1))],
            vec![],
            None,
            None,
            true,
        ),
    );
    let spare_id = first
        .workspaces
        .iter()
        .find(|row| row.output.0 == "out-a" && row.native_binding.is_none())
        .map(|row| row.id.clone())
        .expect("spare");
    let desired = accepted(
        engine,
        &report(
            2,
            2,
            one_screen(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(D1)),
                fact_native("win-b", "out-a", rect(400, 0, 200, 560), Some(D2)),
            ],
            vec![native_obs("out-a", &[D1, D2], Some(D1))],
            vec![],
            Some(spare_seq),
            Some(WorldCommand::Send {
                window: WindowId("win-a".to_owned()),
                target_output: OutputId("out-a".to_owned()),
                target_workspace: spare_id.clone(),
                follow: false,
            }),
            true,
        ),
    );
    let op = desired.native_creations[0].operation_id.clone();
    (spare_id, op)
}

#[test]
fn no_attempt_never_binds_and_collision_without_attempt_holds() {
    // A fresh desktop with NO raw attempt must never be attributed to the
    // outstanding creation (it could be a concurrent outside desktop):
    // hold instead of fabricating the binding, and never adopt it outside.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let (spare_id, op) = occupy_spare(&mut engine, 11);
    assert!(op.starts_with("op-native-"));
    const X: &str = "concurrent-x";
    let held = accepted(
        &mut engine,
        &report(
            3,
            3,
            one_screen(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(D1)),
                fact_native("win-b", "out-a", rect(400, 0, 200, 560), Some(D2)),
            ],
            vec![native_obs("out-a", &[D1, D2, X], Some(D1))],
            vec![],
            None,
            None,
            true,
        ),
    );
    assert_eq!(
        bound_ids(&held, "out-a").len(),
        2,
        "no bind without an exact Accepted attempt"
    );
    assert_eq!(
        workspace_hold(&held, "out-a", &spare_id),
        Some(HoldReason::AmbiguousCreation),
        "unattributed candidate holds, never adopts"
    );
    assert_eq!(
        held.native_creations
            .iter()
            .map(|d| &d.operation_id)
            .collect::<Vec<_>>(),
        vec![&op],
        "authorization stays under the same op while uncorrelated"
    );
}

#[test]
fn future_and_stale_attempt_stamps_hold() {
    // Monotonic/idempotency rules: an attempt stamped after the report's
    // own clock, or before the creation was minted, proves nothing.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let (spare_id, op) = occupy_spare(&mut engine, 11);
    // Mint happened at monotonic 10 (set below on the desire report is
    // already past; re-mint deterministically here is unnecessary: the
    // retained baseline order is [D1, D2] either way).
    const C: &str = "late-c";
    // Future stamp: attempt_ms 99 past the report clock 12.
    let mut fut = report(
        3,
        3,
        one_screen(),
        vec![
            fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(D1)),
            fact_native("win-b", "out-a", rect(400, 0, 200, 560), Some(D2)),
        ],
        vec![native_obs("out-a", &[D1, D2, C], Some(D1))],
        vec![creation_attempt(
            &op,
            &spare_id,
            "out-a",
            FrameSetter::Accepted,
            &[D1, D2],
            &[D1, D2, C],
            None,
            99,
        )],
        None,
        None,
        true,
    );
    fut.monotonic_ms = 12;
    let held = accepted(&mut engine, &fut);
    assert_eq!(
        bound_ids(&held, "out-a").len(),
        2,
        "future stamp never binds"
    );
    assert_eq!(
        workspace_hold(&held, "out-a", &spare_id),
        Some(HoldReason::AmbiguousCreation)
    );
    assert!(
        held.native_creations.is_empty(),
        "consumed attempt republishes no authorization"
    );
}

#[test]
fn delayed_no_delta_attempt_settles_on_late_readback() {
    // Hard delayed-readback sequence: the attempt reports before==after
    // (no delta yet) and is NEVER updated after the fact; the later
    // appended order alone settles the single outstanding creation.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let (spare_id, op) = occupy_spare(&mut engine, 11);
    // No delta yet: Accepted, before == after == baseline.
    let waiting = accepted(
        &mut engine,
        &report(
            3,
            3,
            one_screen(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(D1)),
                fact_native("win-b", "out-a", rect(400, 0, 200, 560), Some(D2)),
            ],
            vec![native_obs("out-a", &[D1, D2], Some(D1))],
            vec![creation_attempt(
                &op,
                &spare_id,
                "out-a",
                FrameSetter::Accepted,
                &[D1, D2],
                &[D1, D2],
                None,
                7,
            )],
            None,
            None,
            true,
        ),
    );
    assert_eq!(bound_ids(&waiting, "out-a").len(), 2);
    assert_eq!(workspace_hold(&waiting, "out-a", &spare_id), None);
    assert!(
        waiting.native_creations.is_empty(),
        "consumed attempt republishes no authorization"
    );
    // The desktop appears late with NO new attempt row: the retained
    // no-delta Accepted attempt plus the unique readback delta settles.
    const C: &str = "late-c";
    let settled = accepted(
        &mut engine,
        &report(
            4,
            4,
            one_screen(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(D1)),
                fact_native("win-b", "out-a", rect(400, 0, 200, 560), Some(D2)),
            ],
            vec![native_obs("out-a", &[D1, D2, C], Some(D1))],
            vec![],
            None,
            None,
            true,
        ),
    );
    let bound: std::collections::BTreeMap<String, String> = bound_ids(&settled, "out-a")
        .into_iter()
        .map(|(id, n)| (n, id))
        .collect();
    assert_eq!(
        bound[C], spare_id,
        "late unique delta settles the pending creation"
    );
    assert!(settled.native_creations.is_empty());
}

#[test]
fn changed_replay_marks_ambiguous() {
    // The first exact attempt is consumed once: a changed replay for the
    // same op (different after-image) marks ambiguity instead of rebinding.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let (spare_id, op) = occupy_spare(&mut engine, 11);
    accepted(
        &mut engine,
        &report(
            3,
            3,
            one_screen(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(D1)),
                fact_native("win-b", "out-a", rect(400, 0, 200, 560), Some(D2)),
            ],
            vec![native_obs("out-a", &[D1, D2], Some(D1))],
            vec![creation_attempt(
                &op,
                &spare_id,
                "out-a",
                FrameSetter::Accepted,
                &[D1, D2],
                &[D1, D2],
                None,
                7,
            )],
            None,
            None,
            true,
        ),
    );
    // Replay claims a delta the fresh readback does not show.
    const C: &str = "ghost-c";
    let held = accepted(
        &mut engine,
        &report(
            4,
            4,
            one_screen(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(D1)),
                fact_native("win-b", "out-a", rect(400, 0, 200, 560), Some(D2)),
            ],
            vec![native_obs("out-a", &[D1, D2], Some(D1))],
            vec![creation_attempt(
                &op,
                &spare_id,
                "out-a",
                FrameSetter::Accepted,
                &[D1, D2],
                &[D1, D2, C],
                None,
                8,
            )],
            None,
            None,
            true,
        ),
    );
    assert_eq!(
        workspace_hold(&held, "out-a", &spare_id),
        Some(HoldReason::AmbiguousCreation),
        "changed replay never rebinds"
    );
    assert_eq!(bound_ids(&held, "out-a").len(), 2);
    assert!(held.native_creations.is_empty());
}

#[test]
fn creation_triple_mismatch_is_invalid() {
    // One op naming two different triples is contradictory evidence, never
    // a silent absent-fallback for the wrong row.
    let report = WorldReport {
        contract_version: WORLD_CONTRACT_VERSION,
        owner: "owner-a".to_owned(),
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        observation_seq: 1,
        request_seq: 1,
        request_id: "req-1".to_owned(),
        command_seq: None,
        command: None,
        workspace_mode: WorkspaceMode::PerOutputLocal,
        screens: one_screen(),
        windows: vec![],
        views: vec![],
        closed: vec![],
        host_owned_workspace_domains: true,
        equal_workspace_count: false,
        native: vec![native_obs("out-a", &[D1], Some(D1))],
        fixed_size_predicate: tiler_core::size_hints::FixedSizePredicate::BothAxes,
        migration_source_refill: tiler_core::output_selection::MigrationSourceRefill::LastRemaining,
        anchor_fallback: tiler_core::admission::AnchorFallback::default(),
        no_space_result: tiler_core::admission::NoSpaceResult::default(),
        default_tiled: true,
        focused_global: String::new(),
        action_foreground: None,
        action_cached: None,
        action_outputs: Vec::new(),
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
        frames: Vec::new(),
        member_attempts: Vec::new(),
        view_attempts: Vec::new(),
        clear_attempts: Vec::new(),
        creation_attempts: vec![
            creation_attempt(
                "op-native-1",
                "ws-1",
                "out-a",
                FrameSetter::Accepted,
                &[D1],
                &[D1],
                None,
                1,
            ),
            creation_attempt(
                "op-native-1",
                "ws-9",
                "out-a",
                FrameSetter::Accepted,
                &[D1],
                &[D1],
                None,
                1,
            ),
        ],
        presentation: tiler_core::world_report::PresentationIngress::disabled(),
    };
    assert_eq!(
        tiler_core::world_report::validate_report_reason(&report),
        Some("creation-triple-mismatch")
    );
}

#[test]
fn native_reorder_reorders_bound_rows() {
    // Same ids, new host order: bound logical rows follow the observed
    // order instead of keeping the stale insertion order.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            one_screen(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(D1)),
                fact_native("win-b", "out-a", rect(400, 0, 200, 560), Some(D2)),
            ],
            vec![native_obs("out-a", &[D1, D2], Some(D1))],
            vec![],
            None,
            None,
            true,
        ),
    );
    let reordered = accepted(
        &mut engine,
        &report(
            2,
            2,
            one_screen(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(D1)),
                fact_native("win-b", "out-a", rect(400, 0, 200, 560), Some(D2)),
            ],
            vec![native_obs("out-a", &[D2, D1], Some(D2))],
            vec![],
            None,
            None,
            true,
        ),
    );
    let order: Vec<String> = bound_ids(&reordered, "out-a")
        .into_iter()
        .map(|(_, n)| n)
        .collect();
    assert_eq!(order, vec![D2, D1], "bound rows follow observed order");
    let view = reordered
        .outputs
        .iter()
        .find(|row| row.output.0 == "out-a")
        .and_then(|row| row.logical_view.clone())
        .expect("view");
    let view_first = first
        .outputs
        .iter()
        .find(|row| row.output.0 == "out-a")
        .and_then(|row| row.logical_view.clone())
        .expect("first view");
    // Order follows the host; the retained core view does not move on a
    // native reorder alone (views move through transfers and commands).
    assert_eq!(view, view_first, "reorder never moves the retained view");
}

#[test]
fn vanished_selected_and_absent_window_remove_without_recreation() {
    // The vanished desktop was selected: removal must rehome the view and
    // never recreate from the stale retained view or member.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    accepted(
        &mut engine,
        &report(
            1,
            1,
            one_screen(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(D1)),
                fact_native("win-b", "out-a", rect(400, 0, 200, 560), Some(D2)),
            ],
            vec![native_obs("out-a", &[D1, D2], Some(D2))],
            vec![],
            None,
            None,
            true,
        ),
    );
    let mut gone = report(
        2,
        2,
        one_screen(),
        vec![fact_native(
            "win-a",
            "out-a",
            rect(0, 0, 400, 560),
            Some(D1),
        )],
        vec![native_obs("out-a", &[D1], Some(D1))],
        vec![],
        None,
        None,
        true,
    );
    gone.closed = vec![WindowId("win-b".to_owned())];
    let removed = accepted(&mut engine, &gone);
    let remaining: Vec<(String, String)> = removed
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| {
            (
                row.id.clone(),
                row.native_binding.clone().unwrap_or_default(),
            )
        })
        .collect();
    assert!(
        remaining.iter().all(|(_, n)| n != D2),
        "vanished selected id removed: {remaining:?}"
    );
    assert!(
        remaining.iter().any(|(_, n)| n == D1),
        "survivor keeps its binding: {remaining:?}"
    );
    assert!(
        removed.native_creations.is_empty(),
        "removal mints no creation"
    );
    // Absent window with no explicit close on a complete snapshot: the
    // emptied logical is removed, never backed by a fresh creation.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    accepted(
        &mut engine,
        &report(
            1,
            1,
            one_screen(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(D1)),
                fact_native("win-b", "out-a", rect(400, 0, 200, 560), Some(D2)),
            ],
            vec![native_obs("out-a", &[D1, D2], Some(D1))],
            vec![],
            None,
            None,
            true,
        ),
    );
    let removed = accepted(
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
            vec![native_obs("out-a", &[D1], Some(D1))],
            vec![],
            None,
            None,
            true,
        ),
    );
    let remaining: Vec<(String, String)> = removed
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .map(|row| {
            (
                row.id.clone(),
                row.native_binding.clone().unwrap_or_default(),
            )
        })
        .collect();
    assert!(
        remaining.iter().all(|(_, n)| n != D2),
        "absent window empties the logical: {remaining:?}"
    );
    assert!(
        removed.native_creations.is_empty(),
        "no recreation from stale member"
    );
    assert!(removed.transfers.is_empty());
}

#[test]
fn stale_attempt_before_mint_holds() {
    // An attempt stamped before the creation was minted cannot prove it:
    // pre-mint evidence holds instead of binding.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let mut bootstrap = report(
        1,
        1,
        one_screen(),
        vec![
            fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(D1)),
            fact_native("win-b", "out-a", rect(400, 0, 200, 560), Some(D2)),
        ],
        vec![native_obs("out-a", &[D1, D2], Some(D1))],
        vec![],
        None,
        None,
        true,
    );
    bootstrap.monotonic_ms = 10;
    let first = accepted(&mut engine, &bootstrap);
    let spare_id = first
        .workspaces
        .iter()
        .find(|row| row.output.0 == "out-a" && row.native_binding.is_none())
        .map(|row| row.id.clone())
        .expect("spare");
    let mut send = report(
        2,
        2,
        one_screen(),
        vec![
            fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(D1)),
            fact_native("win-b", "out-a", rect(400, 0, 200, 560), Some(D2)),
        ],
        vec![native_obs("out-a", &[D1, D2], Some(D1))],
        vec![],
        Some(11),
        Some(WorldCommand::Send {
            window: WindowId("win-a".to_owned()),
            target_output: OutputId("out-a".to_owned()),
            target_workspace: spare_id.clone(),
            follow: false,
        }),
        true,
    );
    send.monotonic_ms = 10;
    let desired = accepted(&mut engine, &send);
    let op = desired.native_creations[0].operation_id.clone();
    // Attempt stamped 5 predates the mint at 10: stale, never proof, even
    // though the readback shows the single expected addition.
    const C: &str = "late-c";
    let mut stale = report(
        3,
        3,
        one_screen(),
        vec![
            fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(D1)),
            fact_native("win-b", "out-a", rect(400, 0, 200, 560), Some(D2)),
        ],
        vec![native_obs("out-a", &[D1, D2, C], Some(D1))],
        vec![creation_attempt(
            &op,
            &spare_id,
            "out-a",
            FrameSetter::Accepted,
            &[D1, D2],
            &[D1, D2, C],
            None,
            5,
        )],
        None,
        None,
        true,
    );
    stale.monotonic_ms = 12;
    let held = accepted(&mut engine, &stale);
    assert_eq!(
        bound_ids(&held, "out-a").len(),
        2,
        "stale stamp never binds"
    );
    assert_eq!(
        workspace_hold(&held, "out-a", &spare_id),
        Some(HoldReason::AmbiguousCreation)
    );
    assert!(held.native_creations.is_empty());
}

#[test]
fn accepted_no_delta_late_singleton_binds_uncorrelated_risk() {
    // Hybrid-C contract: without a host-correlated created id, exactly one
    // outstanding creation plus a single fresh addition binds by unique
    // delta, even asynchronously. This accepts the residual outside-race
    // risk (X may be a concurrent outside desktop while the tiler's own T
    // is still in flight); hosts that can prove causality send `created`.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let (spare_id, op) = occupy_spare(&mut engine, 11);
    // Adapter reports Accepted with no delta yet (async create accepted,
    // desktop not visible). Mirrors delayed_no_delta_attempt_settles_on_late_readback.
    accepted(
        &mut engine,
        &report(
            3,
            3,
            one_screen(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(D1)),
                fact_native("win-b", "out-a", rect(400, 0, 200, 560), Some(D2)),
            ],
            vec![native_obs("out-a", &[D1, D2], Some(D1))],
            vec![creation_attempt(
                &op,
                &spare_id,
                "out-a",
                FrameSetter::Accepted,
                &[D1, D2],
                &[D1, D2],
                None,
                7,
            )],
            None,
            None,
            true,
        ),
    );
    // Meanwhile the user creates X before the tiler's T appears. Complete
    // order is [D1, D2, X] with NO new attempt row: accepted uncorrelated
    // inference settles the one outstanding creation on the singleton.
    const X: &str = "late-singleton-x";
    let settled = accepted(
        &mut engine,
        &report(
            4,
            4,
            one_screen(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(D1)),
                fact_native("win-b", "out-a", rect(400, 0, 200, 560), Some(D2)),
            ],
            vec![native_obs("out-a", &[D1, D2, X], Some(D1))],
            vec![],
            None,
            None,
            true,
        ),
    );
    let bound: std::collections::BTreeMap<String, String> = bound_ids(&settled, "out-a")
        .into_iter()
        .map(|(id, n)| (n, id))
        .collect();
    assert_eq!(
        bound[X], spare_id,
        "uncorrelated singleton settles the pending creation: {bound:?}"
    );
    assert_eq!(bound.len(), 3, "exactly one new workspace: {bound:?}");
    assert_eq!(workspace_hold(&settled, "out-a", &spare_id), None);
    assert!(
        settled.native_creations.is_empty(),
        "bound creations retire"
    );
    // A further desktop is then an outside creation under a fresh logical:
    // nothing outstanding remains to confuse it with.
    const T: &str = "later-outside-t";
    let later = accepted(
        &mut engine,
        &report(
            5,
            5,
            one_screen(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(D1)),
                fact_native("win-b", "out-a", rect(400, 0, 200, 560), Some(D2)),
            ],
            vec![native_obs("out-a", &[D1, D2, X, T], Some(D1))],
            vec![],
            None,
            None,
            true,
        ),
    );
    let bound_later: std::collections::BTreeMap<String, String> = bound_ids(&later, "out-a")
        .into_iter()
        .map(|(id, n)| (n, id))
        .collect();
    assert_eq!(
        bound_later[X], spare_id,
        "settled binding is stable: {bound_later:?}"
    );
    let fresh = bound_later.get(T).expect("later desktop adopts fresh");
    assert_ne!(
        fresh, &spare_id,
        "later outside creation mints fresh, never renames: {bound_later:?}"
    );
    assert_eq!(bound_later.len(), 4);
    assert!(later.native_creations.is_empty());
}

#[test]
fn correlated_created_binds_exactly_despite_race() {
    // A clean host-correlated receipt binds exactly its named desktop at
    // most once, even with a racing outside addition in the same readback.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let (spare_id, op) = occupy_spare(&mut engine, 11);
    const T: &str = "tiler-created-t";
    const X: &str = "user-outside-x";
    // Async attempt: accepted with no delta yet, but the host already
    // proves which desktop its create call produced.
    accepted(
        &mut engine,
        &report(
            3,
            3,
            one_screen(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(D1)),
                fact_native("win-b", "out-a", rect(400, 0, 200, 560), Some(D2)),
            ],
            vec![native_obs("out-a", &[D1, D2], Some(D1))],
            vec![creation_attempt(
                &op,
                &spare_id,
                "out-a",
                FrameSetter::Accepted,
                &[D1, D2],
                &[D1, D2],
                Some(T),
                7,
            )],
            None,
            None,
            true,
        ),
    );
    // Both the created desktop and a racing outside desktop appear with no
    // new attempt row: the retained receipt still proves T causally.
    let raced = accepted(
        &mut engine,
        &report(
            4,
            4,
            one_screen(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(D1)),
                fact_native("win-b", "out-a", rect(400, 0, 200, 560), Some(D2)),
            ],
            vec![native_obs("out-a", &[D1, D2, X, T], Some(D1))],
            vec![],
            None,
            None,
            true,
        ),
    );
    let bound: std::collections::BTreeMap<String, String> = bound_ids(&raced, "out-a")
        .into_iter()
        .map(|(id, n)| (n, id))
        .collect();
    assert_eq!(
        bound[T], spare_id,
        "correlated receipt binds exactly its desktop: {bound:?}"
    );
    let fresh_x = bound.get(X).expect("racing outside desktop adopts fresh");
    assert_ne!(
        fresh_x, &spare_id,
        "the race never steals the pending logical: {bound:?}"
    );
    assert_eq!(bound.len(), 4, "no duplicate workspace: {bound:?}");
    assert_eq!(workspace_hold(&raced, "out-a", &spare_id), None);
    assert!(raced.native_creations.is_empty());
    // Replay idempotent: the same receipt on a later report binds nothing
    // new and mints no replacement operation.
    let replay = accepted(
        &mut engine,
        &report(
            5,
            5,
            one_screen(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(D1)),
                fact_native("win-b", "out-a", rect(400, 0, 200, 560), Some(D2)),
            ],
            vec![native_obs("out-a", &[D1, D2, X, T], Some(D1))],
            vec![creation_attempt(
                &op,
                &spare_id,
                "out-a",
                FrameSetter::Accepted,
                &[D1, D2],
                &[D1, D2],
                Some(T),
                7,
            )],
            None,
            None,
            true,
        ),
    );
    let again: std::collections::BTreeMap<String, String> = bound_ids(&replay, "out-a")
        .into_iter()
        .map(|(id, n)| (n, id))
        .collect();
    assert_eq!(again, bound, "replay binds nothing new");
    assert!(replay.native_creations.is_empty());
}

#[test]
fn correlated_created_wait_holds_then_binds_late() {
    // The named desktop is not visible yet while an outside candidate is:
    // hold transiently without guessing, then bind when T arrives.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let (spare_id, op) = occupy_spare(&mut engine, 11);
    const T: &str = "tiler-created-t";
    const X: &str = "user-outside-x";
    accepted(
        &mut engine,
        &report(
            3,
            3,
            one_screen(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(D1)),
                fact_native("win-b", "out-a", rect(400, 0, 200, 560), Some(D2)),
            ],
            vec![native_obs("out-a", &[D1, D2], Some(D1))],
            vec![creation_attempt(
                &op,
                &spare_id,
                "out-a",
                FrameSetter::Accepted,
                &[D1, D2],
                &[D1, D2],
                Some(T),
                7,
            )],
            None,
            None,
            true,
        ),
    );
    // X appears but T does not: bind nothing, adopt nothing, latch
    // nothing. The correlated pending never resolves by unique delta.
    let waiting = accepted(
        &mut engine,
        &report(
            4,
            4,
            one_screen(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(D1)),
                fact_native("win-b", "out-a", rect(400, 0, 200, 560), Some(D2)),
            ],
            vec![native_obs("out-a", &[D1, D2, X], Some(D1))],
            vec![],
            None,
            None,
            true,
        ),
    );
    let held: std::collections::BTreeMap<String, String> = bound_ids(&waiting, "out-a")
        .into_iter()
        .map(|(id, n)| (n, id))
        .collect();
    assert_eq!(
        held.len(),
        2,
        "unnamed singleton never binds correlated: {held:?}"
    );
    assert!(!held.contains_key(X), "X stays unadopted: {held:?}");
    assert_eq!(workspace_hold(&waiting, "out-a", &spare_id), None);
    assert!(
        waiting.native_creations.is_empty(),
        "consumed receipt republishes no authorization"
    );
    // T arrives late alongside X: the receipt proves T; X adopts outside.
    let settled = accepted(
        &mut engine,
        &report(
            5,
            5,
            one_screen(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(D1)),
                fact_native("win-b", "out-a", rect(400, 0, 200, 560), Some(D2)),
            ],
            vec![native_obs("out-a", &[D1, D2, X, T], Some(D1))],
            vec![],
            None,
            None,
            true,
        ),
    );
    let bound: std::collections::BTreeMap<String, String> = bound_ids(&settled, "out-a")
        .into_iter()
        .map(|(id, n)| (n, id))
        .collect();
    assert_eq!(bound[T], spare_id, "late named desktop binds: {bound:?}");
    assert!(
        bound.contains_key(X),
        "outside race adopts fresh: {bound:?}"
    );
    assert_eq!(bound.len(), 4);
    assert!(settled.native_creations.is_empty());
}

#[test]
fn correlated_created_changed_replay_holds() {
    // The first exact receipt is consumed once: a changed replay for the
    // same op (different correlated id) latches ambiguity instead of
    // rebinding.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let (spare_id, op) = occupy_spare(&mut engine, 11);
    const T: &str = "tiler-created-t";
    const U: &str = "ghost-u";
    accepted(
        &mut engine,
        &report(
            3,
            3,
            one_screen(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(D1)),
                fact_native("win-b", "out-a", rect(400, 0, 200, 560), Some(D2)),
            ],
            vec![native_obs("out-a", &[D1, D2], Some(D1))],
            vec![creation_attempt(
                &op,
                &spare_id,
                "out-a",
                FrameSetter::Accepted,
                &[D1, D2],
                &[D1, D2],
                Some(T),
                7,
            )],
            None,
            None,
            true,
        ),
    );
    let held = accepted(
        &mut engine,
        &report(
            4,
            4,
            one_screen(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(D1)),
                fact_native("win-b", "out-a", rect(400, 0, 200, 560), Some(D2)),
            ],
            vec![native_obs("out-a", &[D1, D2], Some(D1))],
            vec![creation_attempt(
                &op,
                &spare_id,
                "out-a",
                FrameSetter::Accepted,
                &[D1, D2],
                &[D1, D2],
                Some(U),
                8,
            )],
            None,
            None,
            true,
        ),
    );
    assert_eq!(
        workspace_hold(&held, "out-a", &spare_id),
        Some(HoldReason::AmbiguousCreation),
        "changed receipt never rebinds"
    );
    assert_eq!(bound_ids(&held, "out-a").len(), 2);
    assert!(held.native_creations.is_empty());
}

#[test]
fn correlated_created_reused_id_holds() {
    // A receipt naming an already-existing desktop (in `before`) proves
    // nothing new: ambiguity instead of stealing the bound desktop.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let (spare_id, op) = occupy_spare(&mut engine, 11);
    let held = accepted(
        &mut engine,
        &report(
            3,
            3,
            one_screen(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(D1)),
                fact_native("win-b", "out-a", rect(400, 0, 200, 560), Some(D2)),
            ],
            vec![native_obs("out-a", &[D1, D2], Some(D1))],
            vec![creation_attempt(
                &op,
                &spare_id,
                "out-a",
                FrameSetter::Accepted,
                &[D1, D2],
                &[D1, D2],
                Some(D1),
                7,
            )],
            None,
            None,
            true,
        ),
    );
    assert_eq!(
        workspace_hold(&held, "out-a", &spare_id),
        Some(HoldReason::AmbiguousCreation),
        "reused receipt never steals a bound desktop"
    );
    let bound: std::collections::BTreeMap<String, String> = bound_ids(&held, "out-a")
        .into_iter()
        .map(|(id, n)| (n, id))
        .collect();
    assert_eq!(bound.len(), 2, "nothing binds: {bound:?}");
    assert!(held.native_creations.is_empty());
}

#[test]
fn creation_receipt_validation_rejects_reuse_and_bad_ids() {
    // A non-opaque correlated id refuses with fixed vocabulary.
    let bad = report(
        1,
        1,
        one_screen(),
        vec![fact_native(
            "win-a",
            "out-a",
            rect(0, 0, 400, 560),
            Some(D1),
        )],
        vec![native_obs("out-a", &[D1], Some(D1))],
        vec![creation_attempt(
            "op-native-1",
            "ws-1",
            "out-a",
            FrameSetter::Accepted,
            &[D1],
            &[D1],
            Some("not an id!"),
            1,
        )],
        None,
        None,
        true,
    );
    assert_eq!(
        tiler_core::world_report::validate_report_reason(&bad),
        Some("creation-id-invalid")
    );
    // One correlated id claimed by two operations is contradictory
    // evidence and refuses outright, like a triple mismatch.
    let reused = report(
        1,
        1,
        one_screen(),
        vec![fact_native(
            "win-a",
            "out-a",
            rect(0, 0, 400, 560),
            Some(D1),
        )],
        vec![native_obs("out-a", &[D1], Some(D1))],
        vec![
            creation_attempt(
                "op-native-1",
                "ws-1",
                "out-a",
                FrameSetter::Accepted,
                &[D1],
                &[D1],
                Some("shared-new"),
                1,
            ),
            creation_attempt(
                "op-native-2",
                "ws-9",
                "out-a",
                FrameSetter::Accepted,
                &[D1],
                &[D1],
                Some("shared-new"),
                2,
            ),
        ],
        None,
        None,
        true,
    );
    assert_eq!(
        tiler_core::world_report::validate_report_reason(&reused),
        Some("creation-receipt-reused")
    );
}

#[test]
fn multiple_outstanding_creations_hold_without_blind_recreate() {
    // Two outstanding uncorrelated creations on one output plus one fresh
    // candidate: bind nothing, latch both, mint nothing new. The pair is
    // a migrated logical awaiting backing plus an occupied spare (one
    // screen cannot host two unbound needed logicals from sends alone:
    // the trailing spare regenerates only on arrival proof).
    let screens = vec![screen("out-a", 0, true), screen("out-b", 800, false)];
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            screens.clone(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(D1)),
                fact_native("win-c", "out-b", rect(800, 0, 400, 560), Some(D3)),
            ],
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
    let view_a = first
        .outputs
        .iter()
        .find(|row| row.output.0 == "out-a")
        .and_then(|row| row.logical_view.clone())
        .expect("view a");
    // Migrate the viewed logical to out-b: one creation desire for the
    // migrated backing.
    let migrated = accepted(
        &mut engine,
        &report(
            2,
            2,
            screens.clone(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(D1)),
                fact_native("win-c", "out-b", rect(800, 0, 400, 560), Some(D3)),
            ],
            vec![
                native_obs("out-a", &[D1, D2], Some(D1)),
                native_obs("out-b", &[D3], Some(D3)),
            ],
            vec![],
            Some(11),
            Some(WorldCommand::MigrateWorkspace {
                source_output: OutputId("out-a".to_owned()),
                target_output: OutputId("out-b".to_owned()),
                workspace: view_a.clone(),
            }),
            true,
        ),
    );
    assert!(
        migrated
            .native_creations
            .iter()
            .any(|d| d.logical_workspace == view_a && d.target_output.0 == "out-b"),
        "migrated backing desired"
    );
    let spare_b = migrated
        .workspaces
        .iter()
        .find(|row| row.output.0 == "out-b" && row.native_binding.is_none() && row.id != view_a)
        .map(|row| row.id.clone())
        .expect("out-b spare");
    assert_ne!(spare_b, view_a);
    // Occupy the out-b spare as well: two outstanding creations there.
    let desired = accepted(
        &mut engine,
        &report(
            3,
            3,
            screens.clone(),
            vec![
                fact_native("win-c", "out-b", rect(800, 0, 400, 560), Some(D3)),
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(D1)),
            ],
            vec![
                native_obs("out-a", &[D1, D2], Some(D1)),
                native_obs("out-b", &[D3], Some(D3)),
            ],
            vec![],
            Some(12),
            Some(WorldCommand::Send {
                window: WindowId("win-c".to_owned()),
                target_output: OutputId("out-b".to_owned()),
                target_workspace: spare_b.clone(),
                follow: false,
            }),
            true,
        ),
    );
    assert_eq!(
        desired
            .native_creations
            .iter()
            .filter(|d| d.target_output.0 == "out-b")
            .count(),
        2,
        "two outstanding creations on out-b: {:?}",
        desired.native_creations
    );
    // A single fresh desktop against two outstanding creations proves
    // neither: hold both rows, bind nothing, mint nothing.
    const C: &str = "late-c";
    let held = accepted(
        &mut engine,
        &report(
            4,
            4,
            screens.clone(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(D1)),
                fact_native("win-c", "out-b", rect(800, 0, 400, 560), Some(D3)),
            ],
            vec![
                native_obs("out-a", &[D1, D2], Some(D1)),
                native_obs("out-b", &[D3, C], Some(D3)),
            ],
            vec![],
            None,
            None,
            true,
        ),
    );
    assert_eq!(
        bound_ids(&held, "out-b").len(),
        1,
        "multiple outstanding never bind by guessing"
    );
    assert_eq!(
        workspace_hold(&held, "out-b", &view_a),
        Some(HoldReason::AmbiguousCreation)
    );
    assert_eq!(
        workspace_hold(&held, "out-b", &spare_b),
        Some(HoldReason::AmbiguousCreation)
    );
    // No attempt evidence arrived, so the hold stays transient (not
    // latched): the same two authorizations republish under their stable
    // ops for a later correlated receipt to settle. Never a blind
    // replacement operation.
    let held_ops: Vec<&str> = held
        .native_creations
        .iter()
        .map(|d| d.operation_id.as_str())
        .collect();
    let mut want: Vec<&str> = desired
        .native_creations
        .iter()
        .filter(|d| d.target_output.0 == "out-b")
        .map(|d| d.operation_id.as_str())
        .collect();
    let mut got = held_ops.clone();
    want.sort();
    got.sort();
    assert_eq!(got, want, "same ops republish, no blind recreate");
}

#[test]
fn correlated_created_raced_before_still_binds_exact_id() {
    // The host's truthful `before` may include an outside desktop that
    // arrived after the creation was minted: that never refuses a
    // correlated receipt. Only the named desktop binds.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let (spare_id, op) = occupy_spare(&mut engine, 11);
    const X: &str = "user-outside-x";
    const T: &str = "tiler-created-t";
    // Outside X appears while the tiler creation is still in flight: bind
    // nothing, adopt nothing.
    let waiting = accepted(
        &mut engine,
        &report(
            3,
            3,
            one_screen(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(D1)),
                fact_native("win-b", "out-a", rect(400, 0, 200, 560), Some(D2)),
            ],
            vec![native_obs("out-a", &[D1, D2, X], Some(D1))],
            vec![],
            None,
            None,
            true,
        ),
    );
    assert_eq!(bound_ids(&waiting, "out-a").len(), 2);
    assert_eq!(
        workspace_hold(&waiting, "out-a", &spare_id),
        Some(HoldReason::AmbiguousCreation)
    );
    // The accepted write ran after X arrived, so the truthful `before`
    // includes it. The receipt still proves exactly T.
    let settled = accepted(
        &mut engine,
        &report(
            4,
            4,
            one_screen(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(D1)),
                fact_native("win-b", "out-a", rect(400, 0, 200, 560), Some(D2)),
            ],
            vec![native_obs("out-a", &[D1, D2, X, T], Some(D1))],
            vec![creation_attempt(
                &op,
                &spare_id,
                "out-a",
                FrameSetter::Accepted,
                &[D1, D2, X],
                &[D1, D2, X, T],
                Some(T),
                7,
            )],
            None,
            None,
            true,
        ),
    );
    let bound: std::collections::BTreeMap<String, String> = bound_ids(&settled, "out-a")
        .into_iter()
        .map(|(id, n)| (n, id))
        .collect();
    assert_eq!(
        bound[T], spare_id,
        "raced before never refuses the named desktop: {bound:?}"
    );
    let fresh_x = bound.get(X).expect("racing outside desktop adopts fresh");
    assert_ne!(
        fresh_x, &spare_id,
        "the race never steals the pending logical: {bound:?}"
    );
    assert_eq!(bound.len(), 4);
    assert_eq!(workspace_hold(&settled, "out-a", &spare_id), None);
    assert!(settled.native_creations.is_empty());
}

#[test]
fn two_correlated_creations_settle_sequentially() {
    // Two outstanding correlated creations on one output settle one at a
    // time. The second receipt's truthful `before` includes the first
    // creation's desktop (it arrived after op2 was minted): that never
    // refuses op2.
    let screens = vec![screen("out-a", 0, true), screen("out-b", 800, false)];
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            screens.clone(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(D1)),
                fact_native("win-c", "out-b", rect(800, 0, 400, 560), Some(D3)),
            ],
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
    let view_a = first
        .outputs
        .iter()
        .find(|row| row.output.0 == "out-a")
        .and_then(|row| row.logical_view.clone())
        .expect("view a");
    let migrated = accepted(
        &mut engine,
        &report(
            2,
            2,
            screens.clone(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(D1)),
                fact_native("win-c", "out-b", rect(800, 0, 400, 560), Some(D3)),
            ],
            vec![
                native_obs("out-a", &[D1, D2], Some(D1)),
                native_obs("out-b", &[D3], Some(D3)),
            ],
            vec![],
            Some(11),
            Some(WorldCommand::MigrateWorkspace {
                source_output: OutputId("out-a".to_owned()),
                target_output: OutputId("out-b".to_owned()),
                workspace: view_a.clone(),
            }),
            true,
        ),
    );
    let op1 = migrated
        .native_creations
        .iter()
        .find(|d| d.logical_workspace == view_a && d.target_output.0 == "out-b")
        .map(|d| d.operation_id.clone())
        .expect("migrated backing desired");
    let spare_b = migrated
        .workspaces
        .iter()
        .find(|row| row.output.0 == "out-b" && row.native_binding.is_none() && row.id != view_a)
        .map(|row| row.id.clone())
        .expect("out-b spare");
    let desired = accepted(
        &mut engine,
        &report(
            3,
            3,
            screens.clone(),
            vec![
                fact_native("win-c", "out-b", rect(800, 0, 400, 560), Some(D3)),
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(D1)),
            ],
            vec![
                native_obs("out-a", &[D1, D2], Some(D1)),
                native_obs("out-b", &[D3], Some(D3)),
            ],
            vec![],
            Some(12),
            Some(WorldCommand::Send {
                window: WindowId("win-c".to_owned()),
                target_output: OutputId("out-b".to_owned()),
                target_workspace: spare_b.clone(),
                follow: false,
            }),
            true,
        ),
    );
    let op2 = desired
        .native_creations
        .iter()
        .find(|d| d.logical_workspace == spare_b && d.target_output.0 == "out-b")
        .map(|d| d.operation_id.clone())
        .expect("spare backing desired");
    assert_ne!(op1, op2);
    // First receipt settles op1; op2 republishes while T2 is invisible.
    const T1: &str = "tiler-created-t1";
    const T2: &str = "tiler-created-t2";
    let after_first = accepted(
        &mut engine,
        &report(
            4,
            4,
            screens.clone(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(D1)),
                fact_native("win-c", "out-b", rect(800, 0, 400, 560), Some(D3)),
            ],
            vec![
                native_obs("out-a", &[D1, D2], Some(D1)),
                native_obs("out-b", &[D3, T1], Some(D3)),
            ],
            vec![creation_attempt(
                &op1,
                &view_a,
                "out-b",
                FrameSetter::Accepted,
                &[D3],
                &[D3, T1],
                Some(T1),
                7,
            )],
            None,
            None,
            true,
        ),
    );
    let bound_first: std::collections::BTreeMap<String, String> = bound_ids(&after_first, "out-b")
        .into_iter()
        .map(|(id, n)| (n, id))
        .collect();
    assert_eq!(
        bound_first[T1], view_a,
        "op1 binds exactly: {bound_first:?}"
    );
    assert_eq!(
        after_first
            .native_creations
            .iter()
            .map(|d| &d.operation_id)
            .collect::<Vec<_>>(),
        vec![&op2],
        "op2 republishes while its desktop is invisible"
    );
    assert_eq!(workspace_hold(&after_first, "out-b", &view_a), None);
    // Second receipt names T2 with op1's desktop truthfully in `before`.
    let after_second = accepted(
        &mut engine,
        &report(
            5,
            5,
            screens.clone(),
            vec![
                fact_native("win-a", "out-a", rect(0, 0, 400, 560), Some(D1)),
                fact_native("win-c", "out-b", rect(800, 0, 400, 560), Some(D3)),
            ],
            vec![
                native_obs("out-a", &[D1, D2], Some(D1)),
                native_obs("out-b", &[D3, T1, T2], Some(D3)),
            ],
            vec![creation_attempt(
                &op2,
                &spare_b,
                "out-b",
                FrameSetter::Accepted,
                &[D3, T1],
                &[D3, T1, T2],
                Some(T2),
                8,
            )],
            None,
            None,
            true,
        ),
    );
    let bound: std::collections::BTreeMap<String, String> = bound_ids(&after_second, "out-b")
        .into_iter()
        .map(|(id, n)| (n, id))
        .collect();
    assert_eq!(bound[T1], view_a, "first binding stable: {bound:?}");
    assert_eq!(
        bound[T2], spare_b,
        "second receipt settles despite op1 desktop in before: {bound:?}"
    );
    assert_eq!(bound.len(), 3, "exactly D3 plus both creations: {bound:?}");
    assert_eq!(workspace_hold(&after_second, "out-b", &spare_b), None);
    assert!(after_second.native_creations.is_empty());
}

#[test]
fn none_current_conflicting_view_holds_flight_until_native_proof() {
    // Native-only view receipts: with an unknown current (`None`, e.g. a
    // fullscreen Space) a conflicting explicit logical echo proves
    // nothing. The retained SwitchView flight holds (never settles on the
    // logical echo) and the later native current settles it with no
    // logical evidence at all.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
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
            true,
        ),
    );
    let bound: std::collections::BTreeMap<String, String> = bound_ids(&first, "out-a")
        .into_iter()
        .map(|(id, n)| (n, id))
        .collect();
    accepted(
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
            Some(11),
            Some(WorldCommand::SwitchView {
                output: OutputId("out-a".to_owned()),
                workspace: bound[D2].clone(),
            }),
            true,
        ),
    );
    // Unknown current echoing the flight target logically: proves nothing.
    let mut bogus = report(
        3,
        3,
        one_screen(),
        vec![fact_native(
            "win-a",
            "out-a",
            rect(0, 0, 400, 560),
            Some(D1),
        )],
        vec![native_obs("out-a", &[D1, D2], None)],
        vec![],
        None,
        None,
        true,
    );
    bogus.views = vec![ScreenView {
        output: OutputId("out-a".to_owned()),
        view: bound[D2].clone(),
    }];
    let held = accepted(&mut engine, &bogus);
    assert_eq!(
        held.transfers.len(),
        1,
        "unknown current holds the flight despite the logical echo: {:?}",
        held.transfers
    );
    // The native current later proves the target with no logical echo.
    let settled = accepted(
        &mut engine,
        &report(
            4,
            4,
            one_screen(),
            vec![fact_native(
                "win-a",
                "out-a",
                rect(0, 0, 400, 560),
                Some(D1),
            )],
            vec![native_obs("out-a", &[D1, D2], Some(D2))],
            vec![],
            None,
            None,
            true,
        ),
    );
    assert!(
        settled.transfers.is_empty(),
        "native proof settles: {:?}",
        settled.transfers
    );
}

#[test]
fn actual_native_current_settles_despite_bogus_view() {
    // Native-only view receipts: the actual native current proves the
    // retained flight even when the explicit logical echo names a stale
    // workspace. No conflict hold, no logical fallback.
    let mut engine = WorldEngine::new("gen-1").expect("generation");
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
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
            true,
        ),
    );
    let bound: std::collections::BTreeMap<String, String> = bound_ids(&first, "out-a")
        .into_iter()
        .map(|(id, n)| (n, id))
        .collect();
    accepted(
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
            Some(11),
            Some(WorldCommand::SwitchView {
                output: OutputId("out-a".to_owned()),
                workspace: bound[D2].clone(),
            }),
            true,
        ),
    );
    // The host shows the target natively while the logical echo is stale.
    let mut stale = report(
        3,
        3,
        one_screen(),
        vec![fact_native(
            "win-a",
            "out-a",
            rect(0, 0, 400, 560),
            Some(D1),
        )],
        vec![native_obs("out-a", &[D1, D2], Some(D2))],
        vec![],
        None,
        None,
        true,
    );
    stale.views = vec![ScreenView {
        output: OutputId("out-a".to_owned()),
        view: bound[D1].clone(),
    }];
    let settled = accepted(&mut engine, &stale);
    assert!(
        settled.transfers.is_empty(),
        "actual native current wins despite the bogus echo: {:?}",
        settled.transfers
    );
}

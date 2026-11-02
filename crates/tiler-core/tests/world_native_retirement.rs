//! Native retirement eligibility (Stage 4): a bound logical row whose
//! native desktop reads globally empty (`Some(0)` on every projection)
//! publishes `existence:false`, while the row and its native binding stay
//! until the desktop vanishes from a complete readback. Unknown, partial,
//! occupied, viewed, trailing, homed, retained-flight/creation, displaced,
//! or minimum-floor evidence retains `existence:true`.

use tiler_core::connection::{HostCapabilities, RawSettingValue};
use tiler_core::directional::{OutputId, WindowId};
use tiler_core::geometry::Rect;
use tiler_core::size_hints::WindowSizeHints;
use tiler_core::workspace_scope::WorkspaceMode;
use tiler_core::world_engine::{WorldEngine, WorldOutcome};
use tiler_core::world_report::{
    NativeObservation, NativeOccupancy, ScreenReport, WindowFact, WorldCommand, WorldReport,
};

fn engine_host_owned(mode: WorkspaceMode, equal_count: bool) -> WorldEngine {
    let token = match mode {
        WorkspaceMode::PerOutputLocal => "per-output-local",
        WorkspaceMode::GlobalUnique => "global-unique",
        WorkspaceMode::Shared => "shared",
    };
    WorldEngine::new_test_bound_with_initial_settings(
        "gen-1",
        "adapter-1",
        HostCapabilities {
            host_owned: true,
            equal_count,
            ..Default::default()
        },
        vec![("mode".to_owned(), RawSettingValue::String(token.to_owned()))],
    )
    .expect("generation")
}

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

fn incomplete_screen(output: &str, x: i32) -> ScreenReport {
    ScreenReport {
        complete: false,
        ..screen(output, x)
    }
}

fn fact_native(window: &str, output: &str, native: Option<&str>) -> WindowFact {
    WindowFact {
        window: WindowId(window.to_owned()),
        output: Some(OutputId(output.to_owned())),
        rect: Some(rect(0, 0, 40, 90)),
        fullscreen: Some(false),
        maximized: Some(false),
        sticky: Some(false),
        hints: Some(WindowSizeHints::none()),
        minimized: Some(false),
        native_workspace: native
            .map(|s| tiler_core::world_report::NativeWorkspaceFact::Value(s.to_owned()))
            .unwrap_or(tiler_core::world_report::NativeWorkspaceFact::Omitted),
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
    screens: Vec<ScreenReport>,
    windows: Vec<WindowFact>,
    native: Vec<NativeObservation>,
    command_seq: Option<u64>,
    command: Option<WorldCommand>,
    inventory_complete: bool,
) -> WorldReport {
    WorldReport {
        contract_version: tiler_core::world_report::WORLD_CONTRACT_VERSION,
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        observation_seq,
        request_seq,
        request_id: format!("req-{request_seq}"),
        command_seq,
        command,
        screens,
        windows: windows.clone(),
        views: vec![],
        closed: vec![],
        native,
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
        member_attempts: Vec::new(),
        view_attempts: Vec::new(),
        clear_attempts: Vec::new(),
        creation_attempts: Vec::new(),
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

fn native_obs_occ(
    output: &str,
    order: &[&str],
    current: Option<&str>,
    counts: &[Option<u32>],
) -> NativeObservation {
    assert_eq!(order.len(), counts.len(), "order/counts keyed coverage");
    NativeObservation {
        output: OutputId(output.to_owned()),
        native_order: order.iter().map(|s| (*s).to_owned()).collect(),
        current_native: current.map(str::to_owned),
        occupancy: order
            .iter()
            .zip(counts.iter())
            .map(|(s, count)| NativeOccupancy {
                native: (*s).to_owned(),
                count: *count,
            })
            .collect(),
    }
}

fn empty_obs(output: &str, order: &[&str], current: &str) -> NativeObservation {
    native_obs_occ(output, order, Some(current), &vec![Some(0); order.len()])
}

fn bound_ids(reply: &tiler_core::world_report::WorldReply, output: &str) -> Vec<(String, String)> {
    reply
        .workspaces
        .iter()
        .filter(|row| row.output.0 == output && row.native_binding.is_some())
        .map(|row| (row.id.clone(), row.native_binding.clone().expect("bound")))
        .collect()
}

fn ring_ids(reply: &tiler_core::world_report::WorldReply, output: &str) -> Vec<String> {
    reply
        .workspaces
        .iter()
        .filter(|row| row.output.0 == output)
        .map(|row| row.id.clone())
        .collect()
}

fn existence(
    reply: &tiler_core::world_report::WorldReply,
    output: &str,
    logical: &str,
) -> Option<bool> {
    reply
        .workspaces
        .iter()
        .find(|row| row.output.0 == output && row.id == logical)
        .map(|row| row.existence)
}

fn logical_for(
    reply: &tiler_core::world_report::WorldReply,
    output: &str,
    native: &str,
) -> Option<String> {
    bound_ids(reply, output)
        .into_iter()
        .find(|(_, n)| n == native)
        .map(|(id, _)| id)
}

fn single_screen() -> Vec<ScreenReport> {
    vec![screen("out-a", 0)]
}

fn setup_four(engine: &mut WorldEngine) -> tiler_core::world_report::WorldReply {
    accepted(
        engine,
        &report(
            1,
            1,
            single_screen(),
            vec![],
            vec![empty_obs("out-a", &["d1", "d2", "d3", "d4"], "d1")],
            None,
            None,
            true,
        ),
    )
}

#[test]
fn eligible_empty_desktop_publishes_removal_while_row_and_binding_persist() {
    let mut engine = engine_host_owned(WorkspaceMode::PerOutputLocal, false);
    let first = setup_four(&mut engine);
    assert_eq!(bound_ids(&first, "out-a").len(), 4, "four bound: {first:?}");
    assert_eq!(
        ring_ids(&first, "out-a").len(),
        4,
        "no spare needed: {first:?}"
    );
    // d1 is viewed (current); d4 is the bound trailing spare. d2 and d3
    // read empty and retire jointly.
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            single_screen(),
            vec![],
            vec![empty_obs("out-a", &["d1", "d2", "d3", "d4"], "d1")],
            None,
            None,
            true,
        ),
    );
    let d1 = logical_for(&reply, "out-a", "d1").expect("d1 still bound");
    let d2 = logical_for(&reply, "out-a", "d2").expect("d2 still bound");
    let d3 = logical_for(&reply, "out-a", "d3").expect("d3 still bound");
    let d4 = logical_for(&reply, "out-a", "d4").expect("d4 still bound");
    assert_eq!(
        existence(&reply, "out-a", &d1),
        Some(true),
        "view vetoes: {reply:?}"
    );
    assert_eq!(
        existence(&reply, "out-a", &d2),
        Some(false),
        "empty retires: {reply:?}"
    );
    assert_eq!(
        existence(&reply, "out-a", &d3),
        Some(false),
        "empty retires: {reply:?}"
    );
    assert_eq!(
        existence(&reply, "out-a", &d4),
        Some(true),
        "trailing vetoes: {reply:?}"
    );
    // The retiring rows are still published with bindings and order.
    let ring = ring_ids(&reply, "out-a");
    assert_eq!(
        ring.len(),
        4,
        "row persists until readback absence: {ring:?}"
    );
    assert_eq!(
        bound_ids(&reply, "out-a").len(),
        4,
        "bindings persist: {reply:?}"
    );
}

#[test]
fn retirement_repeat_is_stable_with_same_identities() {
    let mut engine = engine_host_owned(WorkspaceMode::PerOutputLocal, false);
    setup_four(&mut engine);
    let mk = |seq| {
        report(
            seq,
            seq,
            single_screen(),
            vec![],
            vec![empty_obs("out-a", &["d1", "d2", "d3", "d4"], "d1")],
            None,
            None,
            true,
        )
    };
    let second = accepted(&mut engine, &mk(2));
    let third = accepted(&mut engine, &mk(3));
    assert_eq!(
        ring_ids(&second, "out-a"),
        ring_ids(&third, "out-a"),
        "stable surviving identities"
    );
    assert_eq!(
        bound_ids(&second, "out-a"),
        bound_ids(&third, "out-a"),
        "stable bindings"
    );
    for row in &third.workspaces {
        let prev = existence(&second, &row.output.0, &row.id);
        assert_eq!(Some(row.existence), prev, "stable eligibility: {row:?}");
    }
}

#[test]
fn partial_inventory_holds_every_row() {
    let mut engine = engine_host_owned(WorkspaceMode::PerOutputLocal, false);
    setup_four(&mut engine);
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            single_screen(),
            vec![],
            vec![empty_obs("out-a", &["d1", "d2", "d3", "d4"], "d1")],
            None,
            None,
            false,
        ),
    );
    assert!(
        reply.workspaces.iter().all(|row| row.existence),
        "partial inventory retains all: {reply:?}"
    );
}

#[test]
fn incomplete_screen_holds_every_row() {
    let mut engine = engine_host_owned(WorkspaceMode::PerOutputLocal, false);
    setup_four(&mut engine);
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![incomplete_screen("out-a", 0)],
            vec![],
            vec![empty_obs("out-a", &["d1", "d2", "d3", "d4"], "d1")],
            None,
            None,
            true,
        ),
    );
    assert!(
        reply.workspaces.iter().all(|row| row.existence),
        "incomplete screen retains all: {reply:?}"
    );
}

#[test]
fn unknown_occupancy_blocks_candidate() {
    let mut engine = engine_host_owned(WorkspaceMode::PerOutputLocal, false);
    let first = setup_four(&mut engine);
    let d3 = logical_for(&first, "out-a", "d3").expect("d3 bound");
    let d2 = logical_for(&first, "out-a", "d2").expect("d2 bound");
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            single_screen(),
            vec![],
            vec![native_obs_occ(
                "out-a",
                &["d1", "d2", "d3", "d4"],
                Some("d1"),
                &[Some(0), Some(0), None, Some(0)],
            )],
            None,
            None,
            true,
        ),
    );
    assert_eq!(
        existence(&reply, "out-a", &d3),
        Some(true),
        "unknown occupancy never proves empty: {reply:?}"
    );
    assert_eq!(
        existence(&reply, "out-a", &d2),
        Some(false),
        "selective: proven-empty sibling still retires: {reply:?}"
    );
}

#[test]
fn occupied_desktop_blocks_candidate() {
    let mut engine = engine_host_owned(WorkspaceMode::PerOutputLocal, false);
    let first = setup_four(&mut engine);
    let d3 = logical_for(&first, "out-a", "d3").expect("d3 bound");
    let d2 = logical_for(&first, "out-a", "d2").expect("d2 bound");
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            single_screen(),
            vec![],
            vec![native_obs_occ(
                "out-a",
                &["d1", "d2", "d3", "d4"],
                Some("d1"),
                &[Some(0), Some(0), Some(2), Some(0)],
            )],
            None,
            None,
            true,
        ),
    );
    assert_eq!(
        existence(&reply, "out-a", &d3),
        Some(true),
        "occupant vetoes retirement: {reply:?}"
    );
    assert_eq!(
        existence(&reply, "out-a", &d2),
        Some(false),
        "selective: proven-empty sibling still retires: {reply:?}"
    );
}

#[test]
fn viewed_desktop_never_retires() {
    let mut engine = engine_host_owned(WorkspaceMode::PerOutputLocal, false);
    let first = setup_four(&mut engine);
    let d1 = logical_for(&first, "out-a", "d1").expect("d1 bound");
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            single_screen(),
            vec![],
            vec![empty_obs("out-a", &["d1", "d2", "d3", "d4"], "d1")],
            None,
            None,
            true,
        ),
    );
    assert_eq!(
        existence(&reply, "out-a", &d1),
        Some(true),
        "current view vetoes: {reply:?}"
    );
}

#[test]
fn unknown_current_falls_back_to_retained_view() {
    let mut engine = engine_host_owned(WorkspaceMode::PerOutputLocal, false);
    let first = setup_four(&mut engine);
    let d1 = logical_for(&first, "out-a", "d1").expect("d1 bound");
    let d2 = logical_for(&first, "out-a", "d2").expect("d2 bound");
    // `current_native: None` names no ordered desktop: the retained view
    // (d1) still vetoes while other empty desktops retire normally.
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            single_screen(),
            vec![],
            vec![native_obs_occ(
                "out-a",
                &["d1", "d2", "d3", "d4"],
                None,
                &[Some(0), Some(0), Some(0), Some(0)],
            )],
            None,
            None,
            true,
        ),
    );
    assert_eq!(
        existence(&reply, "out-a", &d1),
        Some(true),
        "retained view vetoes without current: {reply:?}"
    );
    assert_eq!(
        existence(&reply, "out-a", &d2),
        Some(false),
        "other empties still retire: {reply:?}"
    );
}

#[test]
fn known_raw_current_vetoes_despite_command_moved_view() {
    let mut engine = engine_host_owned(WorkspaceMode::PerOutputLocal, false);
    let first = setup_four(&mut engine);
    let d2 = logical_for(&first, "out-a", "d2").expect("d2 bound");
    let d3 = logical_for(&first, "out-a", "d3").expect("d3 bound");
    // The command moves the desired view to d3, but the raw native
    // current still names d2: adapter truth wins, d2 stays.
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            single_screen(),
            vec![],
            vec![empty_obs("out-a", &["d1", "d2", "d3", "d4"], "d2")],
            Some(1),
            Some(WorldCommand::SwitchView {
                output: OutputId("out-a".to_owned()),
                workspace: d3.clone(),
            }),
            true,
        ),
    );
    assert_eq!(
        existence(&reply, "out-a", &d2),
        Some(true),
        "raw current vetoes despite command: {reply:?}"
    );
    assert_eq!(
        reply
            .outputs
            .iter()
            .find(|row| row.output.0 == "out-a")
            .and_then(|row| row.logical_view.clone()),
        Some(d3),
        "command moved the desired view: {reply:?}"
    );
}

#[test]
fn gesture_barrier_vetoes_every_retirement() {
    let mut engine = engine_host_owned(WorkspaceMode::PerOutputLocal, false);
    setup_four(&mut engine);
    let mut barrier = report(
        2,
        2,
        single_screen(),
        vec![],
        vec![empty_obs("out-a", &["d1", "d2", "d3", "d4"], "d1")],
        None,
        None,
        true,
    );
    barrier.gesture_barrier = true;
    let reply = accepted(&mut engine, &barrier);
    assert!(
        reply.workspaces.iter().all(|row| row.existence),
        "gesture barrier retains all: {reply:?}"
    );
}

#[test]
fn shared_mirror_flight_vetoes_observing_projection() {
    let mut engine = engine_host_owned(WorkspaceMode::Shared, false);
    // Shared mode, `n` observed only on out-a; its logical mirrors on
    // out-b without a binding there.
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            two_screens(),
            vec![],
            vec![
                empty_obs("out-a", &["v", "n", "t"], "v"),
                empty_obs("out-b", &["v", "t"], "v"),
            ],
            None,
            None,
            true,
        ),
    );
    let ln = logical_for(&first, "out-a", "n").expect("n bound on a");
    assert!(
        logical_for(&first, "out-b", "n").is_none(),
        "n unobserved on b: {first:?}"
    );
    assert!(
        ring_ids(&first, "out-b").contains(&ln),
        "logical mirrors on b: {first:?}"
    );
    // Control: without protection the one-sided native retires (no equal
    // count flag).
    let control = accepted(
        &mut engine,
        &report(
            2,
            2,
            two_screens(),
            vec![],
            vec![
                empty_obs("out-a", &["v", "n", "t"], "v"),
                empty_obs("out-b", &["v", "t"], "v"),
            ],
            None,
            None,
            true,
        ),
    );
    assert_eq!(
        existence(&control, "out-a", &ln),
        Some(false),
        "unprotected one-sided retires: {control:?}"
    );
    // A retained view flight targeting the mirrored row on out-b protects
    // the shared logical everywhere, including the observing projection.
    let reply = accepted(
        &mut engine,
        &report(
            3,
            3,
            two_screens(),
            vec![],
            vec![
                empty_obs("out-a", &["v", "n", "t"], "v"),
                empty_obs("out-b", &["v", "t"], "v"),
            ],
            Some(1),
            Some(WorldCommand::SwitchView {
                output: OutputId("out-b".to_owned()),
                workspace: ln.clone(),
            }),
            true,
        ),
    );
    assert_eq!(
        existence(&reply, "out-a", &ln),
        Some(true),
        "mirror flight vetoes observer: {reply:?}"
    );
}

#[test]
fn native_minimum_floor_blocks_last_two() {
    let mut engine = engine_host_owned(WorkspaceMode::PerOutputLocal, false);
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            single_screen(),
            vec![],
            vec![empty_obs("out-a", &["d1", "d2"], "d1")],
            None,
            None,
            true,
        ),
    );
    let d2 = logical_for(&first, "out-a", "d2").expect("d2 bound");
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            single_screen(),
            vec![],
            vec![empty_obs("out-a", &["d1", "d2"], "d1")],
            None,
            None,
            true,
        ),
    );
    assert_eq!(
        existence(&reply, "out-a", &d2),
        Some(true),
        "native minimum two holds: {reply:?}"
    );
}

#[test]
fn trailing_bound_row_never_eligible() {
    let mut engine = engine_host_owned(WorkspaceMode::PerOutputLocal, false);
    let first = setup_four(&mut engine);
    // The last bound row is the trailing spare even while bound.
    let d4 = logical_for(&first, "out-a", "d4").expect("d4 bound");
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            single_screen(),
            vec![],
            vec![empty_obs("out-a", &["d1", "d2", "d3", "d4"], "d1")],
            None,
            None,
            true,
        ),
    );
    assert_eq!(
        existence(&reply, "out-a", &d4),
        Some(true),
        "trailing spare stays: {reply:?}"
    );
}

#[test]
fn unbound_spare_never_eligible() {
    let mut engine = engine_host_owned(WorkspaceMode::PerOutputLocal, false);
    // Occupying the last desktop appends an ordinary unbound spare.
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            single_screen(),
            vec![fact_native("win-a", "out-a", Some("d4"))],
            vec![native_obs_occ(
                "out-a",
                &["d1", "d2", "d3", "d4"],
                Some("d1"),
                &[Some(0), Some(0), Some(0), Some(1)],
            )],
            None,
            None,
            true,
        ),
    );
    assert_eq!(
        ring_ids(&first, "out-a").len(),
        5,
        "spare appended: {first:?}"
    );
    let spare_row = first
        .workspaces
        .iter()
        .find(|row| row.output.0 == "out-a" && row.native_binding.is_none())
        .expect("unbound spare row published");
    assert!(spare_row.trailing, "spare is trailing: {spare_row:?}");
    assert!(
        first.native_creations.is_empty(),
        "no creation for ordinary spare: {first:?}"
    );
    let spare = spare_row.id.clone();
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            single_screen(),
            vec![fact_native("win-a", "out-a", Some("d4"))],
            vec![native_obs_occ(
                "out-a",
                &["d1", "d2", "d3", "d4"],
                Some("d1"),
                &[Some(0), Some(0), Some(0), Some(1)],
            )],
            None,
            None,
            true,
        ),
    );
    assert_eq!(
        existence(&reply, "out-a", &spare),
        Some(true),
        "logical spare stays: {reply:?}"
    );
}

#[test]
fn vanished_desktop_recycles_logical_as_trailing_spare_after_complete_absence() {
    let mut engine = engine_host_owned(WorkspaceMode::PerOutputLocal, false);
    let first = setup_four(&mut engine);
    let d2 = logical_for(&first, "out-a", "d2").expect("d2 bound");
    let ring_before = ring_ids(&first, "out-a");
    assert!(ring_before.contains(&d2), "d2 ringed before absence");
    // Complete readback without d2: binding prunes; with no other spare
    // the emptied logical is recycled as the unbound trailing spare
    // (lifecycle's normal trailing path, never a recreation). Survivors
    // keep bindings with compact ordinals.
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            single_screen(),
            vec![],
            vec![empty_obs("out-a", &["d1", "d3", "d4"], "d1")],
            None,
            None,
            true,
        ),
    );
    assert!(
        bound_ids(&reply, "out-a").iter().all(|(id, _)| id != &d2),
        "binding gone after absence: {reply:?}"
    );
    let recycled = existence(&reply, "out-a", &d2);
    assert_eq!(
        recycled,
        Some(true),
        "emptied logical retained as spare: {reply:?}"
    );
    let recycled_row = reply
        .workspaces
        .iter()
        .find(|row| row.output.0 == "out-a" && row.id == d2)
        .expect("recycled row published");
    assert!(
        recycled_row.native_binding.is_none(),
        "unbound spare: {recycled_row:?}"
    );
    assert!(
        recycled_row.trailing,
        "trailing spare role: {recycled_row:?}"
    );
    for (index, row) in reply
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .enumerate()
    {
        assert_eq!(row.order, index, "compact ordinals: {row:?}");
    }
}

#[test]
fn vanished_middle_desktop_deletes_row_when_spare_already_exists() {
    let mut engine = engine_host_owned(WorkspaceMode::PerOutputLocal, false);
    // Occupying the last desktop appends an ordinary unbound spare, so a
    // later-proven absence has a lifecycle spare without recycling.
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            single_screen(),
            vec![fact_native("win-a", "out-a", Some("d4"))],
            vec![native_obs_occ(
                "out-a",
                &["d1", "d2", "d3", "d4"],
                Some("d1"),
                &[Some(0), Some(0), Some(0), Some(1)],
            )],
            None,
            None,
            true,
        ),
    );
    let spare = first
        .workspaces
        .iter()
        .find(|row| row.output.0 == "out-a" && row.native_binding.is_none())
        .expect("spare exists")
        .id
        .clone();
    let bound_before = bound_ids(&first, "out-a");
    let d2 = logical_for(&first, "out-a", "d2").expect("d2 bound");
    // Complete readback without d2: the retired row is deleted while the
    // pre-existing spare survives untouched.
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            single_screen(),
            vec![fact_native("win-a", "out-a", Some("d4"))],
            vec![native_obs_occ(
                "out-a",
                &["d1", "d3", "d4"],
                Some("d1"),
                &[Some(0), Some(0), Some(1)],
            )],
            None,
            None,
            true,
        ),
    );
    let ring = ring_ids(&reply, "out-a");
    assert!(!ring.contains(&d2), "retired row deleted: {ring:?}");
    assert!(
        ring.contains(&spare),
        "pre-existing spare survives: {ring:?}"
    );
    let bound_after = bound_ids(&reply, "out-a");
    let expected: Vec<(String, String)> = bound_before
        .into_iter()
        .filter(|(_, native)| native != "d2")
        .collect();
    assert_eq!(
        bound_after, expected,
        "surviving bound identities unchanged: {reply:?}"
    );
    for (index, row) in reply
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a")
        .enumerate()
    {
        assert_eq!(row.order, index, "compact ordinals: {row:?}");
    }
    assert!(
        reply.native_creations.is_empty(),
        "no recreation after proven absence: {reply:?}"
    );
}

#[test]
fn missing_native_row_refuses_report() {
    let mut engine = engine_host_owned(WorkspaceMode::PerOutputLocal, false);
    setup_four(&mut engine);
    // No observation row for the live output: nothing may retire.
    if let WorldOutcome::Accepted(reply) = engine.handle_report(&report(
        2,
        2,
        single_screen(),
        vec![],
        vec![],
        None,
        None,
        true,
    )) {
        assert!(
            reply.workspaces.iter().all(|row| row.existence),
            "missing row retains all: {reply:?}"
        );
    }
}

#[test]
fn occupancy_coverage_gap_refuses_report() {
    let mut engine = engine_host_owned(WorkspaceMode::PerOutputLocal, false);
    setup_four(&mut engine);
    let mut obs = empty_obs("out-a", &["d1", "d2", "d3", "d4"], "d1");
    obs.occupancy.pop();
    let outcome = engine.handle_report(&report(
        2,
        2,
        single_screen(),
        vec![],
        vec![obs],
        None,
        None,
        true,
    ));
    assert!(
        !matches!(outcome, WorldOutcome::Accepted(_)),
        "coverage gap must not accept: {outcome:?}"
    );
}

#[test]
fn homed_window_blocks_candidate() {
    let mut engine = engine_host_owned(WorkspaceMode::PerOutputLocal, false);
    let first = setup_four(&mut engine);
    let d3 = logical_for(&first, "out-a", "d3").expect("d3 bound");
    let d2 = logical_for(&first, "out-a", "d2").expect("d2 bound");
    // A sticky window homed to d3 reads managed homing there even though
    // the neutral native occupancy still counts zero.
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            single_screen(),
            vec![{
                let mut fact = fact_native("win-s", "out-a", Some("d4"));
                fact.sticky = Some(true);
                fact.native_workspace =
                    tiler_core::world_report::NativeWorkspaceFact::Value("d3".to_owned());
                fact
            }],
            vec![empty_obs("out-a", &["d1", "d2", "d3", "d4"], "d1")],
            None,
            None,
            true,
        ),
    );
    assert_eq!(
        existence(&reply, "out-a", &d3),
        Some(true),
        "managed homing vetoes: {reply:?}"
    );
    assert_eq!(
        existence(&reply, "out-a", &d2),
        Some(false),
        "selective: unhomed sibling still retires: {reply:?}"
    );
}

#[test]
fn retained_send_blocks_candidate() {
    let mut engine = engine_host_owned(WorkspaceMode::PerOutputLocal, false);
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            single_screen(),
            vec![fact_native("win-a", "out-a", Some("d1"))],
            vec![native_obs_occ(
                "out-a",
                &["d1", "d2", "d3", "d4"],
                Some("d1"),
                &[Some(1), Some(0), Some(0), Some(0)],
            )],
            None,
            None,
            true,
        ),
    );
    let d3 = logical_for(&first, "out-a", "d3").expect("d3 bound");
    let d2 = logical_for(&first, "out-a", "d2").expect("d2 bound");
    // A retained send flight targeting d3 protects it on the next read.
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            single_screen(),
            vec![fact_native("win-a", "out-a", Some("d1"))],
            vec![native_obs_occ(
                "out-a",
                &["d1", "d2", "d3", "d4"],
                Some("d1"),
                &[Some(1), Some(0), Some(0), Some(0)],
            )],
            Some(1),
            Some(WorldCommand::Send {
                window: WindowId("win-a".to_owned()),
                target_output: OutputId("out-a".to_owned()),
                target_workspace: d3.clone(),
                follow: false,
            }),
            true,
        ),
    );
    assert_eq!(
        existence(&reply, "out-a", &d3),
        Some(true),
        "retained flight vetoes: {reply:?}"
    );
    assert_eq!(
        existence(&reply, "out-a", &d2),
        Some(false),
        "selective: untargeted sibling still retires: {reply:?}"
    );
}

fn two_screens() -> Vec<ScreenReport> {
    vec![screen("out-a", 0), screen("out-b", 800)]
}

fn global_modes() -> Vec<WorkspaceMode> {
    vec![
        WorkspaceMode::PerOutputLocal,
        WorkspaceMode::GlobalUnique,
        WorkspaceMode::Shared,
    ]
}

#[test]
fn global_same_id_joins_all_modes() {
    for mode in global_modes() {
        let mut engine = engine_host_owned(mode, false);
        let first = accepted(
            &mut engine,
            &report(
                1,
                1,
                two_screens(),
                vec![],
                vec![
                    empty_obs("out-a", &["g1", "g2", "g3"], "g1"),
                    empty_obs("out-b", &["g1", "g2", "g3"], "g1"),
                ],
                None,
                None,
                true,
            ),
        );
        let (_mode_token, la2) = (mode, logical_for(&first, "out-a", "g2"));
        let lb2 = logical_for(&first, "out-b", "g2");
        // Shared mode unifies the logical id; separate modes bind per
        // output. Either way every projection must read Some(0).
        let reply = accepted(
            &mut engine,
            &report(
                2,
                2,
                two_screens(),
                vec![],
                vec![
                    empty_obs("out-a", &["g1", "g2", "g3"], "g1"),
                    empty_obs("out-b", &["g1", "g2", "g3"], "g1"),
                ],
                None,
                None,
                true,
            ),
        );
        match (la2, lb2) {
            (Some(la2), Some(lb2)) => {
                // g1 viewed on both; g3 trailing on both; g2 retires
                // jointly on every projection.
                assert_eq!(
                    existence(&reply, "out-a", &la2),
                    Some(false),
                    "joint retire {mode:?}: {reply:?}"
                );
                assert_eq!(
                    existence(&reply, "out-b", &lb2),
                    Some(false),
                    "joint retire {mode:?}: {reply:?}"
                );
            }
            _ => panic!("g2 must bind on both projections in {mode:?}: {first:?}"),
        }
    }
}

#[test]
fn occupant_on_any_output_blocks_every_projection() {
    for mode in global_modes() {
        let mut engine = engine_host_owned(mode, false);
        let first = accepted(
            &mut engine,
            &report(
                1,
                1,
                two_screens(),
                vec![],
                vec![
                    empty_obs("out-a", &["g1", "g2", "g3"], "g1"),
                    empty_obs("out-b", &["g1", "g2", "g3"], "g1"),
                ],
                None,
                None,
                true,
            ),
        );
        let la2 = logical_for(&first, "out-a", "g2").expect("g2 bound a");
        let lb2 = logical_for(&first, "out-b", "g2").expect("g2 bound b");
        let reply = accepted(
            &mut engine,
            &report(
                2,
                2,
                two_screens(),
                vec![],
                vec![
                    empty_obs("out-a", &["g1", "g2", "g3"], "g1"),
                    native_obs_occ(
                        "out-b",
                        &["g1", "g2", "g3"],
                        Some("g1"),
                        &[Some(0), Some(1), Some(0)],
                    ),
                ],
                None,
                None,
                true,
            ),
        );
        assert_eq!(
            existence(&reply, "out-a", &la2),
            Some(true),
            "any-output occupant vetoes {mode:?}: {reply:?}"
        );
        assert_eq!(
            existence(&reply, "out-b", &lb2),
            Some(true),
            "any-output occupant vetoes {mode:?}: {reply:?}"
        );
    }
}

#[test]
fn view_on_any_output_blocks_every_projection() {
    for mode in global_modes() {
        let mut engine = engine_host_owned(mode, false);
        let first = accepted(
            &mut engine,
            &report(
                1,
                1,
                two_screens(),
                vec![],
                vec![
                    empty_obs("out-a", &["g1", "g2", "g3"], "g1"),
                    empty_obs("out-b", &["g1", "g2", "g3"], "g1"),
                ],
                None,
                None,
                true,
            ),
        );
        let la2 = logical_for(&first, "out-a", "g2").expect("g2 bound a");
        let lb2 = logical_for(&first, "out-b", "g2").expect("g2 bound b");
        // Same global id viewed from out-b while out-a still shows g1.
        let reply = accepted(
            &mut engine,
            &report(
                2,
                2,
                two_screens(),
                vec![],
                vec![
                    empty_obs("out-a", &["g1", "g2", "g3"], "g1"),
                    empty_obs("out-b", &["g1", "g2", "g3"], "g2"),
                ],
                None,
                None,
                true,
            ),
        );
        assert_eq!(
            existence(&reply, "out-a", &la2),
            Some(true),
            "any-output view vetoes {mode:?}: {reply:?}"
        );
        assert_eq!(
            existence(&reply, "out-b", &lb2),
            Some(true),
            "any-output view vetoes {mode:?}: {reply:?}"
        );
    }
}

#[test]
fn one_sided_native_retires_without_equal_count() {
    for mode in global_modes() {
        let mut engine = engine_host_owned(mode, false);
        let first = accepted(
            &mut engine,
            &report(
                1,
                1,
                two_screens(),
                vec![],
                vec![
                    empty_obs("out-a", &["s1", "g1", "g2"], "g1"),
                    empty_obs("out-b", &["g1", "g2"], "g1"),
                ],
                None,
                None,
                true,
            ),
        );
        // s1 exists only on out-a: without the equal-count flag the
        // independent native list retires on its own minimum-two floor.
        let s1 = logical_for(&first, "out-a", "s1")
            .unwrap_or_else(|| panic!("s1 must bind on out-a in {mode:?}: {first:?}"));
        let reply = accepted(
            &mut engine,
            &report(
                2,
                2,
                two_screens(),
                vec![],
                vec![
                    empty_obs("out-a", &["s1", "g1", "g2"], "g1"),
                    empty_obs("out-b", &["g1", "g2"], "g1"),
                ],
                None,
                None,
                true,
            ),
        );
        assert_eq!(
            existence(&reply, "out-a", &s1),
            Some(false),
            "one-sided retires without equal count {mode:?}: {reply:?}"
        );
    }
}

#[test]
fn one_sided_native_presence_retains_padding_row() {
    for mode in global_modes() {
        let mut engine = engine_host_owned(mode, true);
        let first = accepted(
            &mut engine,
            &report(
                1,
                1,
                two_screens(),
                vec![],
                vec![
                    empty_obs("out-a", &["s1", "g1", "g2"], "g1"),
                    empty_obs("out-b", &["g1", "g2"], "g1"),
                ],
                None,
                None,
                true,
            ),
        );
        // Shared removal shrinks the same mirrored logical ring everywhere;
        // other modes need this row to preserve equal-count padding.
        let s1 = logical_for(&first, "out-a", "s1")
            .unwrap_or_else(|| panic!("s1 must bind on out-a in {mode:?}: {first:?}"));
        let reply = accepted(
            &mut engine,
            &report(
                2,
                2,
                two_screens(),
                vec![],
                vec![
                    empty_obs("out-a", &["s1", "g1", "g2"], "g1"),
                    empty_obs("out-b", &["g1", "g2"], "g1"),
                ],
                None,
                None,
                true,
            ),
        );
        assert_eq!(
            existence(&reply, "out-a", &s1),
            Some(mode != WorkspaceMode::Shared),
            "one-sided padding retained {mode:?}: {reply:?}"
        );
    }
}

#[test]
fn disjoint_balanced_pair_retires_jointly_under_equal_count() {
    let mut engine = engine_host_owned(WorkspaceMode::PerOutputLocal, true);
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            two_screens(),
            vec![],
            vec![
                empty_obs("out-a", &["v", "a", "c", "t"], "v"),
                empty_obs("out-b", &["v", "b", "c", "t"], "v"),
            ],
            None,
            None,
            true,
        ),
    );
    let la = logical_for(&first, "out-a", "a").expect("a bound");
    let lc_a = logical_for(&first, "out-a", "c").expect("c bound a");
    let lb = logical_for(&first, "out-b", "b").expect("b bound");
    let lc_b = logical_for(&first, "out-b", "c").expect("c bound b");
    // Disjoint one-sided extras plus one common native: post-removal
    // counts stay equal, so all three retire jointly.
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            two_screens(),
            vec![],
            vec![
                empty_obs("out-a", &["v", "a", "c", "t"], "v"),
                empty_obs("out-b", &["v", "b", "c", "t"], "v"),
            ],
            None,
            None,
            true,
        ),
    );
    assert_eq!(
        existence(&reply, "out-a", &la),
        Some(false),
        "disjoint extra retires: {reply:?}"
    );
    assert_eq!(
        existence(&reply, "out-b", &lb),
        Some(false),
        "disjoint extra retires: {reply:?}"
    );
    assert_eq!(
        existence(&reply, "out-a", &lc_a),
        Some(false),
        "common retires: {reply:?}"
    );
    assert_eq!(
        existence(&reply, "out-b", &lc_b),
        Some(false),
        "common retires: {reply:?}"
    );
    assert_eq!(
        ring_ids(&reply, "out-a").len(),
        ring_ids(&reply, "out-b").len(),
        "equal count parity survives: {reply:?}"
    );
}

#[test]
fn common_plus_extra_spares_only_extra_under_equal_count() {
    let mut engine = engine_host_owned(WorkspaceMode::PerOutputLocal, true);
    // out-b's occupied last desktop appends a padding spare, so both
    // outputs hold four logical rows while only out-a observes `x`.
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            two_screens(),
            vec![fact_native("win-b", "out-b", Some("t"))],
            vec![
                empty_obs("out-a", &["v", "x", "c", "t"], "v"),
                native_obs_occ(
                    "out-b",
                    &["v", "c", "t"],
                    Some("v"),
                    &[Some(0), Some(0), Some(1)],
                ),
            ],
            None,
            None,
            true,
        ),
    );
    assert_eq!(ring_ids(&first, "out-a").len(), 4, "four rows a: {first:?}");
    assert_eq!(ring_ids(&first, "out-b").len(), 4, "four rows b: {first:?}");
    let lx = logical_for(&first, "out-a", "x").expect("x bound");
    let lc_a = logical_for(&first, "out-a", "c").expect("c bound a");
    let lc_b = logical_for(&first, "out-b", "c").expect("c bound b");
    // Retiring both `x` and `c` would leave 2 vs 3 rows; only the extra
    // is spared so the common native still retires jointly.
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            two_screens(),
            vec![fact_native("win-b", "out-b", Some("t"))],
            vec![
                empty_obs("out-a", &["v", "x", "c", "t"], "v"),
                native_obs_occ(
                    "out-b",
                    &["v", "c", "t"],
                    Some("v"),
                    &[Some(0), Some(0), Some(1)],
                ),
            ],
            None,
            None,
            true,
        ),
    );
    assert_eq!(
        existence(&reply, "out-a", &lx),
        Some(true),
        "only extra spared: {reply:?}"
    );
    assert_eq!(
        existence(&reply, "out-a", &lc_a),
        Some(false),
        "common retires: {reply:?}"
    );
    assert_eq!(
        existence(&reply, "out-b", &lc_b),
        Some(false),
        "common retires: {reply:?}"
    );
}

#[test]
fn joint_reduction_preserves_equal_count() {
    let mut engine = engine_host_owned(WorkspaceMode::PerOutputLocal, true);
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            two_screens(),
            vec![],
            vec![
                empty_obs("out-a", &["g1", "g2", "g3"], "g1"),
                empty_obs("out-b", &["g1", "g2", "g3"], "g1"),
            ],
            None,
            None,
            true,
        ),
    );
    let la2 = logical_for(&first, "out-a", "g2").expect("g2 bound a");
    let lb2 = logical_for(&first, "out-b", "g2").expect("g2 bound b");
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            two_screens(),
            vec![],
            vec![
                empty_obs("out-a", &["g1", "g2", "g3"], "g1"),
                empty_obs("out-b", &["g1", "g2", "g3"], "g1"),
            ],
            None,
            None,
            true,
        ),
    );
    assert_eq!(
        existence(&reply, "out-a", &la2),
        Some(false),
        "joint retire: {reply:?}"
    );
    assert_eq!(
        existence(&reply, "out-b", &lb2),
        Some(false),
        "joint retire: {reply:?}"
    );
    assert_eq!(
        ring_ids(&reply, "out-a").len(),
        ring_ids(&reply, "out-b").len(),
        "equal count parity survives: {reply:?}"
    );
}

#[test]
fn staggered_joint_absence_retains_padding_identity_until_balanced_readback() {
    let mut engine = engine_host_owned(WorkspaceMode::PerOutputLocal, true);
    let windows = vec![
        fact_native("win-a", "out-a", Some("t")),
        fact_native("win-b", "out-b", Some("t")),
    ];
    let initial = report(
        1,
        1,
        two_screens(),
        windows,
        vec![
            native_obs_occ(
                "out-a",
                &["v", "a", "t"],
                Some("v"),
                &[Some(0), Some(0), Some(1)],
            ),
            native_obs_occ(
                "out-b",
                &["v", "b", "t"],
                Some("v"),
                &[Some(0), Some(0), Some(1)],
            ),
        ],
        None,
        None,
        true,
    );
    let first = accepted(&mut engine, &initial);
    let la = logical_for(&first, "out-a", "a").expect("a bound");
    let lb = logical_for(&first, "out-b", "b").expect("b bound");
    let mut desired = initial.clone();
    desired.request_seq = 2;
    desired.observation_seq = 2;
    desired.request_id = "req-2".into();
    let second = accepted(&mut engine, &desired);
    assert_eq!(existence(&second, "out-a", &la), Some(false));
    assert_eq!(existence(&second, "out-b", &lb), Some(false));
    let mut partial = desired.clone();
    partial.request_seq = 3;
    partial.observation_seq = 3;
    partial.request_id = "req-3".into();
    partial.native[0] = native_obs_occ("out-a", &["v", "t"], Some("v"), &[Some(0), Some(1)]);
    let third = accepted(&mut engine, &partial);
    assert!(
        logical_for(&third, "out-a", "a").is_none(),
        "absence clears only native binding"
    );
    assert_eq!(
        ring_ids(&third, "out-a")
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>(),
        ring_ids(&second, "out-a").into_iter().collect(),
        "padding keeps old identities after native-order compaction"
    );
    assert_eq!(
        existence(&third, "out-a", &la),
        Some(true),
        "unbound padding survives"
    );
    assert_eq!(bound_ids(&third, "out-b"), bound_ids(&second, "out-b"));
    let mut complete = partial.clone();
    complete.request_seq = 4;
    complete.observation_seq = 4;
    complete.request_id = "req-4".into();
    complete.native[1] = native_obs_occ("out-b", &["v", "t"], Some("v"), &[Some(0), Some(1)]);
    let fourth = accepted(&mut engine, &complete);
    assert_eq!(ring_ids(&fourth, "out-a").len(), 3);
    assert_eq!(ring_ids(&fourth, "out-b").len(), 3);
    assert_eq!(existence(&fourth, "out-a", &la), None);
    assert_eq!(existence(&fourth, "out-b", &lb), None);
    for output in ["out-a", "out-b"] {
        for (index, row) in fourth
            .workspaces
            .iter()
            .filter(|row| row.output.0 == output)
            .enumerate()
        {
            assert_eq!(row.order, index);
        }
    }
}

//! Native retirement eligibility (Stage 4): a bound logical row whose
//! native desktop reads globally empty (`Some(0)` on every projection)
//! publishes `existence:false`, while the row and its native binding stay
//! until the desktop vanishes from a complete readback. Unknown, partial,
//! occupied, viewed, trailing, homed, retained-flight/creation, displaced,
//! or minimum-floor evidence retains `existence:true`.

use tiler_core::connection::{HostCapabilities, NativeSelectorScope, RawSettingValue};
use tiler_core::directional::{OutputId, WindowId};
use tiler_core::geometry::Rect;
use tiler_core::size_hints::WindowSizeHints;
use tiler_core::workspace_scope::WorkspaceMode;
use tiler_core::world_engine::WorldEngine;
use tiler_core::world_report::{
    FrameSetter, NativeObservation, NativeOccupancy, ScreenReport, WindowFact, WorldCommand,
    WorldReportV4,
};

fn engine_host_owned(mode: WorkspaceMode, equal_count: bool) -> WorldEngine {
    engine_host_owned_with_selector(mode, equal_count, false, NativeSelectorScope::PerOutput)
}

fn engine_host_owned_with_selector(
    mode: WorkspaceMode,
    equal_count: bool,
    shared_native_roster: bool,
    native_selector_scope: NativeSelectorScope,
) -> WorldEngine {
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
            shared_native_roster,
            native_selector_scope,
            // Fixtures carry native sticky and maximize facts (including a
            // sticky homing probe); the lane under test is the native route.
            native_sticky: true,
            maximize: true,
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
) -> WorldReportV4 {
    WorldReportV4 {
        contract_version: tiler_core::world_report::WORLD_CONTRACT_VERSION_V4,
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        observation_seq,
        request_seq,
        request_id: format!("req-{request_seq}"),
        command_seq,
        command,
        screens,
        windows: windows.clone(),
        closed: vec![],
        native,
        held_native: Vec::new(),
        focused_global: windows
            .first()
            .map(|f| f.window.0.clone())
            .unwrap_or_default(),
        foreground_known: windows.first().is_some_and(|f| !f.window.0.is_empty()),
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
        creation_attempts: Vec::new(),
        held_attempts: Vec::new(),
        state_attempts: Vec::new(),
        visibility_attempts: Vec::new(),
        focus_changes: None,
        focus_feedback: Vec::new(),
        presentation: tiler_core::world_report::PresentationRowV4::Valid(
            tiler_core::world_report::PresentationIngressV4::default(),
        ),

        intent_records: Vec::new(),
        renderer_input: None,
    }
}

#[path = "support/allow_gate.rs"]
mod allow_gate;
use allow_gate::TestGate;

fn eval_report(
    engine: &mut WorldEngine,
    report: &WorldReportV4,
) -> (
    tiler_core::world_engine::WorldOutcome,
    Vec<tiler_core::world_report::BatchSettlement>,
) {
    let run = engine
        .handle_report_with_gate_and_prepare(report, &[], &TestGate, |_| {
            Ok::<(), std::convert::Infallible>(())
        })
        .unwrap_or_else(|e| match e {});
    let settlements = run
        .dispositions
        .into_iter()
        .filter_map(|d| match d.disposition {
            tiler_core::world_report::CommandDisposition::Executed(result) => {
                Some(tiler_core::world_report::BatchSettlement { seq: d.seq, result })
            }
            tiler_core::world_report::CommandDisposition::Refused(result) => {
                Some(tiler_core::world_report::BatchSettlement {
                    seq: d.seq,
                    result: Some(tiler_core::world_report::CommandResult::Rejected {
                        kind: result,
                    }),
                })
            }
            tiler_core::world_report::CommandDisposition::AlreadyConsumed
            | tiler_core::world_report::CommandDisposition::NotRun => None,
        })
        .collect();
    (run.outcome, settlements)
}

fn accepted(
    engine: &mut WorldEngine,
    report: &WorldReportV4,
) -> tiler_core::world_report::WorldReplyV4 {
    match eval_report(engine, report).0 {
        tiler_core::world_engine::WorldOutcome::Accepted(reply) => reply,
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

fn bound_ids(
    reply: &tiler_core::world_report::WorldReplyV4,
    output: &str,
) -> Vec<(String, String)> {
    // Live bindings only: tombstone absence intents (`existence:false`)
    // reuse the removed logical id plus its native binding as correlation
    // metadata and are never live bindings outside the ring.
    reply
        .workspaces
        .iter()
        .filter(|row| row.output.0 == output && row.existence && row.native_binding.is_some())
        .map(|row| (row.id.clone(), row.native_binding.clone().expect("bound")))
        .collect()
}

fn ring_ids(reply: &tiler_core::world_report::WorldReplyV4, output: &str) -> Vec<String> {
    // Existing rows only: tombstone absence intents are not ring rows.
    reply
        .workspaces
        .iter()
        .filter(|row| row.output.0 == output && row.existence)
        .map(|row| row.id.clone())
        .collect()
}

fn tombstone_id(
    reply: &tiler_core::world_report::WorldReplyV4,
    output: &str,
    native: &str,
) -> Option<String> {
    reply
        .workspaces
        .iter()
        .find(|row| {
            row.output.0 == output
                && row.native_binding.as_deref() == Some(native)
                && !row.existence
        })
        .map(|row| row.id.clone())
}

fn tombstone_row<'a>(
    reply: &'a tiler_core::world_report::WorldReplyV4,
    output: &str,
    native: &str,
) -> Option<&'a tiler_core::world_report::WorkspaceDesire> {
    reply.workspaces.iter().find(|row| {
        row.output.0 == output && row.native_binding.as_deref() == Some(native) && !row.existence
    })
}

fn existence(
    reply: &tiler_core::world_report::WorldReplyV4,
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
    reply: &tiler_core::world_report::WorldReplyV4,
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

fn setup_four(engine: &mut WorldEngine) -> tiler_core::world_report::WorldReplyV4 {
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

/// Bootstrap that retains every bound identity: unknown occupancy for the
/// middle pair (`None`) fails closed, so no local prune happens and the
/// second report can inject a guard/protection before pruning.
fn setup_four_unpruned(engine: &mut WorldEngine) -> tiler_core::world_report::WorldReplyV4 {
    accepted(
        engine,
        &report(
            1,
            1,
            single_screen(),
            vec![],
            vec![native_obs_occ(
                "out-a",
                &["d1", "d2", "d3", "d4"],
                Some("d1"),
                &[Some(0), None, None, Some(0)],
            )],
            None,
            None,
            true,
        ),
    )
}

/// Generic fake host: applies explicit absence intents only, vetoing
/// actuation on global occupancy, any-output visibility, window presence,
/// holds, scope holds, gesture, incomplete inventory, unreadable rows, and
/// the host minimum floor. Returns true when the host would delete the
/// native desktop. Counts per step: creates = `native_creations.len()`,
/// deletes = veto-passed intents, membership writes = `transfers.len()`.
fn host_would_delete(
    reply: &tiler_core::world_report::WorldReplyV4,
    native_rows: &[NativeObservation],
    windows: &[WindowFact],
    inventory_complete: bool,
    gesture_barrier: bool,
    output: &str,
    native: &str,
) -> bool {
    let Some(row) = tombstone_row(reply, output, native) else {
        return false;
    };
    if row.hold.is_some() {
        return false;
    }
    if reply.scopes.iter().any(|scope| {
        scope.scope == tiler_core::world_report::AuthorityScope::Output(OutputId(output.to_owned()))
            && !matches!(
                scope.authority,
                tiler_core::world_report::ScopeAuthority::Controlled
            )
    }) {
        return false;
    }
    if !inventory_complete || gesture_barrier {
        return false;
    }
    for entry in native_rows {
        if entry.occupancy.len() != entry.native_order.len() {
            return false;
        }
        if !entry.native_order.iter().any(|id| id == native) {
            continue;
        }
        match entry.occupancy.iter().find(|row| row.native == native) {
            Some(row) if row.count.is_some_and(|count| count > 0) => return false,
            Some(row) if row.count.is_some() => {}
            _ => return false,
        }
        if entry.current_native.as_deref() == Some(native) {
            return false;
        }
    }
    // Any-output visibility veto: the same native id viewed anywhere.
    if native_rows
        .iter()
        .any(|entry| entry.current_native.as_deref() == Some(native))
    {
        return false;
    }
    if windows
        .iter()
        .any(|fact| fact.native_workspace.as_deref() == Some(native))
    {
        return false;
    }
    // Host minimum floor: a single live bound row never deletes.
    let live_bound = reply
        .workspaces
        .iter()
        .filter(|row| row.output.0 == output && row.existence && row.native_binding.is_some())
        .count();
    if live_bound <= 1 {
        return false;
    }
    true
}

#[test]
fn eligible_empty_desktop_publishes_removal_while_row_and_binding_persist() {
    let mut engine = engine_host_owned(WorkspaceMode::PerOutputLocal, false);
    let first = setup_four(&mut engine);
    // Approved split: the first complete empty readback already prunes the
    // middle pair locally. Live bindings are d1 (viewed) and d4 (trailing
    // spare); d2/d3 publish as `existence:false` tombstones (correlation
    // metadata only, never live bindings).
    assert_eq!(bound_ids(&first, "out-a").len(), 2, "live bound: {first:?}");
    assert_eq!(
        ring_ids(&first, "out-a").len(),
        2,
        "live ring excludes tombstones: {first:?}"
    );
    assert!(tombstone_id(&first, "out-a", "d2").is_some());
    assert!(tombstone_id(&first, "out-a", "d3").is_some());
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
    let d4 = logical_for(&reply, "out-a", "d4").expect("d4 still bound");
    let d2t = tombstone_id(&reply, "out-a", "d2").expect("d2 tombstone");
    let d3t = tombstone_id(&reply, "out-a", "d3").expect("d3 tombstone");
    assert_eq!(
        existence(&reply, "out-a", &d1),
        Some(true),
        "view vetoes: {reply:?}"
    );
    assert_eq!(
        existence(&reply, "out-a", &d2t),
        Some(false),
        "empty retires: {reply:?}"
    );
    assert_eq!(
        existence(&reply, "out-a", &d3t),
        Some(false),
        "empty retires: {reply:?}"
    );
    assert_eq!(
        existence(&reply, "out-a", &d4),
        Some(true),
        "trailing vetoes: {reply:?}"
    );
    // Tombstones publish after the ring and never as live bindings.
    let ring = ring_ids(&reply, "out-a");
    assert_eq!(
        ring.len(),
        2,
        "live ring persists without tombstones: {ring:?}"
    );
    assert_eq!(
        bound_ids(&reply, "out-a").len(),
        2,
        "live bindings persist: {reply:?}"
    );
    // Fake host: globally empty, shown nowhere, readable, above the host
    // minimum floor, so actuation proceeds (explicit intents only).
    let native = vec![empty_obs("out-a", &["d1", "d2", "d3", "d4"], "d1")];
    assert!(host_would_delete(
        &reply,
        &native,
        &[],
        true,
        false,
        "out-a",
        "d2"
    ));
    assert!(host_would_delete(
        &reply,
        &native,
        &[],
        true,
        false,
        "out-a",
        "d3"
    ));
    assert_eq!(reply.transfers.len(), 0, "pruning writes no memberships");
    assert!(
        reply.native_creations.is_empty(),
        "no spare creation needed: single spare already present"
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
fn partial_inventory_fences_retirement_actuation() {
    let mut engine = engine_host_owned(WorkspaceMode::PerOutputLocal, false);
    setup_four(&mut engine);
    let native = vec![empty_obs("out-a", &["d1", "d2", "d3", "d4"], "d1")];
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            single_screen(),
            vec![],
            native.clone(),
            None,
            None,
            false,
        ),
    );
    // Already-retired tombstones persist `existence:false` (never flipped
    // back merely to satisfy old tests); the partial inventory fences
    // actuation via the tombstone hold and the output scope hold.
    for native_id in ["d2", "d3"] {
        let tomb = tombstone_id(&reply, "out-a", native_id).expect("tombstone persists");
        assert_eq!(existence(&reply, "out-a", &tomb), Some(false));
        assert!(
            tombstone_row(&reply, "out-a", native_id).is_some_and(|row| row.hold.is_some()),
            "partial inventory holds the intent: {reply:?}"
        );
        assert!(
            !host_would_delete(&reply, &native, &[], false, false, "out-a", native_id),
            "fake host vetoes deletion under partial inventory"
        );
    }
    assert!(
        reply.scopes.iter().any(|scope| !matches!(
            scope.authority,
            tiler_core::world_report::ScopeAuthority::Controlled
        )),
        "scope fenced: {reply:?}"
    );
    assert_eq!(reply.transfers.len(), 0, "no membership writes");
}

#[test]
fn incomplete_screen_holds_retirement_intents() {
    let mut engine = engine_host_owned(WorkspaceMode::PerOutputLocal, false);
    setup_four(&mut engine);
    let native = vec![empty_obs("out-a", &["d1", "d2", "d3", "d4"], "d1")];
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![incomplete_screen("out-a", 0)],
            vec![],
            native.clone(),
            None,
            None,
            true,
        ),
    );
    for native_id in ["d2", "d3"] {
        let tomb = tombstone_id(&reply, "out-a", native_id).expect("tombstone persists");
        assert_eq!(existence(&reply, "out-a", &tomb), Some(false));
        assert!(
            tombstone_row(&reply, "out-a", native_id).is_some_and(|row| row.hold.is_some()),
            "incomplete screen holds the intent: {reply:?}"
        );
        assert!(
            !host_would_delete(&reply, &native, &[], true, false, "out-a", native_id),
            "fake host vetoes deletion on incomplete screen"
        );
    }
}

#[test]
fn unknown_occupancy_blocks_candidate() {
    let mut engine = engine_host_owned(WorkspaceMode::PerOutputLocal, false);
    // Bootstrap with unknown occupancy so d2/d3 stay live bound; the guard
    // is injected before pruning on report 2.
    let first = setup_four_unpruned(&mut engine);
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
fn occupied_desktop_blocks_actuation() {
    let mut engine = engine_host_owned(WorkspaceMode::PerOutputLocal, false);
    let first = setup_four(&mut engine);
    // d2/d3 already retired on report 1; the occupant count does not flip
    // tombstones back to live. The global occupant vetoes actuation: the
    // fake host preserves the native desktop while the intent stays held
    // only by the explicit absence row.
    let d3t = tombstone_id(&first, "out-a", "d3").expect("d3 tombstone");
    let d2t = tombstone_id(&first, "out-a", "d2").expect("d2 tombstone");
    let native = vec![native_obs_occ(
        "out-a",
        &["d1", "d2", "d3", "d4"],
        Some("d1"),
        &[Some(0), Some(0), Some(2), Some(0)],
    )];
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            single_screen(),
            vec![],
            native.clone(),
            None,
            None,
            true,
        ),
    );
    assert_eq!(
        existence(&reply, "out-a", &d3t),
        Some(false),
        "tombstone persists, never flipped live: {reply:?}"
    );
    assert_eq!(
        existence(&reply, "out-a", &d2t),
        Some(false),
        "proven-empty sibling tombstone persists: {reply:?}"
    );
    assert!(
        !host_would_delete(&reply, &native, &[], true, false, "out-a", "d3"),
        "global occupant vetoes actuation: {reply:?}"
    );
    assert_eq!(reply.transfers.len(), 0, "no membership writes");
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
    let d2t = tombstone_id(&first, "out-a", "d2").expect("d2 tombstone");
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
        existence(&reply, "out-a", &d2t),
        Some(false),
        "other empties still retire: {reply:?}"
    );
}

#[test]
fn known_raw_current_vetoes_despite_command_moved_view() {
    let mut engine = engine_host_owned(WorkspaceMode::PerOutputLocal, false);
    // Unpruned bootstrap keeps d2/d3 live so the SwitchView targets a live
    // row and the raw current veto can be observed before pruning.
    let first = setup_four_unpruned(&mut engine);
    let d2 = logical_for(&first, "out-a", "d2").expect("d2 bound");
    let d3 = logical_for(&first, "out-a", "d3").expect("d3 bound");
    // The command moves the desired view to d3, but the raw native
    // current still names d2: adapter truth wins, d2 stays.
    let (outcome, settlements) = eval_report(
        &mut engine,
        &report(
            2,
            2,
            single_screen(),
            vec![],
            vec![empty_obs("out-a", &["d1", "d2", "d3", "d4"], "d2")],
            Some(1),
            Some(WorldCommand::SelectWorkspace {
                output: OutputId("out-a".to_owned()),
                workspace: tiler_core::directional::WorkspaceId(d3.clone()),
            }),
            true,
        ),
    );
    let reply = match outcome {
        tiler_core::world_engine::WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted reply, got {other:?}"),
    };
    assert_eq!(
        existence(&reply, "out-a", &d2),
        Some(true),
        "raw current vetoes despite command: {reply:?}"
    );
    // The same-report SwitchView targets a row retiring on this readback,
    // so the core rejects it (`ViewTargetUnknown`) and the raw native
    // current wins: the desired view stays on d2.
    assert!(
        matches!(
            settlements.first().and_then(|s| s.result.as_ref()),
            Some(tiler_core::world_report::CommandResult::Rejected { .. })
        ),
        "retiring target rejected: {settlements:?}"
    );
    assert_eq!(
        reply
            .outputs
            .iter()
            .find(|row| row.output.0 == "out-a")
            .and_then(|row| row.logical_view.clone()),
        Some(d2),
        "raw current wins the desired view: {reply:?}"
    );
}

#[test]
fn gesture_barrier_fences_retirement_actuation() {
    let mut engine = engine_host_owned(WorkspaceMode::PerOutputLocal, false);
    setup_four(&mut engine);
    let native = vec![empty_obs("out-a", &["d1", "d2", "d3", "d4"], "d1")];
    let mut barrier = report(
        2,
        2,
        single_screen(),
        vec![],
        native.clone(),
        None,
        None,
        true,
    );
    barrier.gesture_barrier = true;
    let reply = accepted(&mut engine, &barrier);
    // Tombstones persist with an explicit gesture fence; the fake host
    // vetoes deletion while the barrier holds.
    for native_id in ["d2", "d3"] {
        let tomb = tombstone_id(&reply, "out-a", native_id).expect("tombstone persists");
        assert_eq!(existence(&reply, "out-a", &tomb), Some(false));
        assert!(
            tombstone_row(&reply, "out-a", native_id).is_some_and(|row| row.hold.is_some()),
            "gesture fences the intent: {reply:?}"
        );
        assert!(
            !host_would_delete(&reply, &native, &[], true, true, "out-a", native_id),
            "fake host vetoes deletion under gesture"
        );
    }
    assert_eq!(reply.transfers.len(), 0, "no membership writes");
}

#[test]
fn shared_mirror_flight_vetoes_observing_projection() {
    // Shared mode, `n` observed only on out-a; its logical mirrors on
    // out-b without a binding there. Bootstrap with unknown occupancy so
    // `n` stays live and mirrored before any prune.
    let boot_native = vec![
        native_obs_occ(
            "out-a",
            &["v", "n", "t"],
            Some("v"),
            &[Some(0), None, Some(0)],
        ),
        empty_obs("out-b", &["v", "t"], "v"),
    ];
    let mut engine = engine_host_owned(WorkspaceMode::Shared, false);
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            two_screens(),
            vec![],
            boot_native.clone(),
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
    // Control (separate engine): without protection the one-sided native
    // retires into a tombstone (no equal-count flag).
    let mut control_engine = engine_host_owned(WorkspaceMode::Shared, false);
    let _ = accepted(
        &mut control_engine,
        &report(1, 1, two_screens(), vec![], boot_native, None, None, true),
    );
    let control = accepted(
        &mut control_engine,
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
    let tomb = tombstone_id(&control, "out-a", "n").expect("one-sided tombstone");
    assert_eq!(
        existence(&control, "out-a", &tomb),
        Some(false),
        "unprotected one-sided retires: {control:?}"
    );
    // Establish the mirrored view flight while `n` is unknown (no
    // same-report prune), then verify the veto on the proven-empty
    // readback. The pre-existing flight is the guard before pruning.
    let flight = accepted(
        &mut engine,
        &report(
            2,
            2,
            two_screens(),
            vec![],
            vec![
                native_obs_occ(
                    "out-a",
                    &["v", "n", "t"],
                    Some("v"),
                    &[Some(0), None, Some(0)],
                ),
                empty_obs("out-b", &["v", "t"], "v"),
            ],
            Some(1),
            Some(WorldCommand::SelectWorkspace {
                output: OutputId("out-b".to_owned()),
                workspace: tiler_core::directional::WorkspaceId(ln.clone()),
            }),
            true,
        ),
    );
    assert_eq!(
        existence(&flight, "out-a", &ln),
        Some(true),
        "flight established while unknown: {flight:?}"
    );
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
            None,
            None,
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
                &[Some(0), None, None, Some(1)],
            )],
            None,
            None,
            true,
        ),
    );
    // Live ring: d1 (viewed), d4 (occupied), plus the ordinary unbound
    // spare. d2/d3 stay live bound via unknown occupancy (unpruned).
    assert_eq!(
        ring_ids(&first, "out-a").len(),
        5,
        "live ring with spare, unpruned: {first:?}"
    );
    let spare_row = first
        .workspaces
        .iter()
        .find(|row| row.output.0 == "out-a" && row.native_binding.is_none())
        .expect("unbound spare row published");
    assert!(spare_row.trailing, "spare is trailing: {spare_row:?}");
    // Causal spare-only creation: the trailing spare needs native backing.
    assert_eq!(
        first.native_creations.len(),
        1,
        "single spare creation: {first:?}"
    );
    assert_eq!(
        first.native_creations[0].logical_workspace, spare_row.id,
        "creation targets the spare: {first:?}"
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
    let first = setup_four_unpruned(&mut engine);
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
        .filter(|row| row.output.0 == "out-a" && row.existence)
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
    // Unknown middle occupancy keeps d2/d3 live before the absence.
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
                &[Some(0), None, None, Some(1)],
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
    // d2 vanished (binding gone, no tombstone); d3 retired on the same
    // proven-empty readback (tombstone, never live). Survivors keep their
    // bound identities.
    let expected: Vec<(String, String)> = bound_before
        .into_iter()
        .filter(|(_, native)| native != "d2" && native != "d3")
        .collect();
    assert_eq!(
        bound_after, expected,
        "surviving bound identities unchanged: {reply:?}"
    );
    assert!(
        tombstone_id(&reply, "out-a", "d3").is_some(),
        "same-report empty retires: {reply:?}"
    );
    assert!(
        tombstone_id(&reply, "out-a", "d2").is_none(),
        "vanished native leaves no tombstone: {reply:?}"
    );
    for (index, row) in reply
        .workspaces
        .iter()
        .filter(|row| row.output.0 == "out-a" && row.existence)
        .enumerate()
    {
        assert_eq!(row.order, index, "compact ordinals: {row:?}");
    }
    // Causal spare-only creation: the pre-existing unbound spare still
    // needs native backing; absence never recreates the vanished binding.
    assert_eq!(
        reply.native_creations.len(),
        1,
        "single spare creation, no recreation: {reply:?}"
    );
    assert_eq!(
        reply.native_creations[0].logical_workspace, spare,
        "creation targets the spare: {reply:?}"
    );
}

#[test]
fn missing_native_row_fences_retirement_intents() {
    let mut engine = engine_host_owned(WorkspaceMode::PerOutputLocal, false);
    setup_four(&mut engine);
    // No observation row for the live output: tombstones persist fenced;
    // nothing new authorizes deletion.
    if let tiler_core::world_engine::WorldOutcome::Accepted(reply) = eval_report(
        &mut engine,
        &report(2, 2, single_screen(), vec![], vec![], None, None, true),
    )
    .0
    {
        for native_id in ["d2", "d3"] {
            let tomb = tombstone_id(&reply, "out-a", native_id).expect("tombstone persists");
            assert_eq!(existence(&reply, "out-a", &tomb), Some(false));
            assert!(
                tombstone_row(&reply, "out-a", native_id).is_some_and(|row| row.hold.is_some()),
                "missing row holds the intent: {reply:?}"
            );
        }
        assert_eq!(reply.transfers.len(), 0, "no membership writes");
    }
}

#[test]
fn occupancy_coverage_gap_refuses_report() {
    let mut engine = engine_host_owned(WorkspaceMode::PerOutputLocal, false);
    setup_four(&mut engine);
    let mut obs = empty_obs("out-a", &["d1", "d2", "d3", "d4"], "d1");
    obs.occupancy.pop();
    let outcome = eval_report(
        &mut engine,
        &report(2, 2, single_screen(), vec![], vec![obs], None, None, true),
    )
    .0;
    assert!(
        !matches!(outcome, tiler_core::world_engine::WorldOutcome::Accepted(_)),
        "coverage gap must not accept: {outcome:?}"
    );
}

#[test]
fn homed_window_blocks_candidate() {
    let mut engine = engine_host_owned(WorkspaceMode::PerOutputLocal, false);
    let first = setup_four_unpruned(&mut engine);
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
    // Unpruned bootstrap keeps d2/d3 live (unknown middle occupancy fails
    // closed); the retained send is injected before pruning.
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
                &[Some(1), None, None, Some(0)],
            )],
            None,
            None,
            true,
        ),
    );
    let d3 = logical_for(&first, "out-a", "d3").expect("d3 bound");
    let d2 = logical_for(&first, "out-a", "d2").expect("d2 bound");
    // Establish the retained send on report 2 with d3 unknown (no
    // same-report prune), then verify the veto on report 3 with d3 proven
    // empty. The pre-existing flight is the guard injected before pruning.
    let flight = accepted(
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
                &[Some(1), Some(0), None, Some(0)],
            )],
            Some(1),
            Some(WorldCommand::Send {
                window: tiler_core::directional::WindowSubject::Window(WindowId(
                    "win-a".to_owned(),
                )),
                target_output: OutputId("out-a".to_owned()),
                target_workspace: tiler_core::directional::WorkspaceId(d3.clone()),
                follow: false,
            }),
            true,
        ),
    );
    assert_eq!(
        existence(&flight, "out-a", &d3),
        Some(true),
        "flight established while d3 unknown: {flight:?}"
    );
    let reply = accepted(
        &mut engine,
        &report(
            3,
            3,
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
        let la2t = tombstone_id(&first, "out-a", "g2").expect("g2 tombstone a");
        let lb2t = tombstone_id(&first, "out-b", "g2").expect("g2 tombstone b");
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
        // g1 viewed on both; g3 trailing on both; g2 retires jointly.
        assert_eq!(
            existence(&reply, "out-a", &la2t),
            Some(false),
            "joint retire {mode:?}: {reply:?}"
        );
        assert_eq!(
            existence(&reply, "out-b", &lb2t),
            Some(false),
            "joint retire {mode:?}: {reply:?}"
        );
    }
}

#[test]
fn occupant_on_any_output_vetoes_actuation() {
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
        // g2 already retired on report 1; tombstone IDs persist as
        // correlation metadata, never live bindings.
        let la2t = tombstone_id(&first, "out-a", "g2").expect("g2 tombstone a");
        let lb2t = tombstone_id(&first, "out-b", "g2").expect("g2 tombstone b");
        let native = vec![
            empty_obs("out-a", &["g1", "g2", "g3"], "g1"),
            native_obs_occ(
                "out-b",
                &["g1", "g2", "g3"],
                Some("g1"),
                &[Some(0), Some(1), Some(0)],
            ),
        ];
        let reply = accepted(
            &mut engine,
            &report(
                2,
                2,
                two_screens(),
                vec![],
                native.clone(),
                None,
                None,
                true,
            ),
        );
        for (output, tomb) in [("out-a", &la2t), ("out-b", &lb2t)] {
            assert_eq!(
                existence(&reply, output, tomb),
                Some(false),
                "tombstone persists {mode:?}: {reply:?}"
            );
            assert!(
                !host_would_delete(&reply, &native, &[], true, false, output, "g2"),
                "any-output occupant vetoes actuation {mode:?}: {reply:?}"
            );
        }
        assert_eq!(reply.transfers.len(), 0, "no membership writes");
    }
}

#[test]
fn view_on_any_output_blocks_every_projection() {
    for mode in global_modes() {
        let mut engine = engine_host_owned(mode, false);
        // Unpruned bootstrap keeps g2 live on both projections so the
        // any-output view guard can be observed before pruning.
        let first = accepted(
            &mut engine,
            &report(
                1,
                1,
                two_screens(),
                vec![],
                vec![
                    native_obs_occ(
                        "out-a",
                        &["g1", "g2", "g3"],
                        Some("g1"),
                        &[Some(0), None, Some(0)],
                    ),
                    native_obs_occ(
                        "out-b",
                        &["g1", "g2", "g3"],
                        Some("g1"),
                        &[Some(0), None, Some(0)],
                    ),
                ],
                None,
                None,
                true,
            ),
        );
        let la2 = logical_for(&first, "out-a", "g2").expect("g2 bound a");
        let _lb2 = logical_for(&first, "out-b", "g2").expect("g2 bound b");
        // Same global id viewed from out-b while out-a still shows g1.
        // Local prune still publishes tombstones; the any-output view
        // vetoes host actuation (fake host preserves the native desktop).
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
        let native = vec![
            empty_obs("out-a", &["g1", "g2", "g3"], "g1"),
            empty_obs("out-b", &["g1", "g2", "g3"], "g2"),
        ];
        assert_eq!(
            existence(&reply, "out-a", &la2),
            Some(false),
            "local tombstone persists {mode:?}: {reply:?}"
        );
        assert!(
            !host_would_delete(&reply, &native, &[], true, false, "out-a", "g2"),
            "any-output view vetoes actuation {mode:?}: {reply:?}"
        );
        assert!(
            !host_would_delete(&reply, &native, &[], true, false, "out-b", "g2"),
            "any-output view vetoes actuation {mode:?}: {reply:?}"
        );
        assert_eq!(reply.transfers.len(), 0, "no membership writes");
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
        // independent native list retires locally into a tombstone.
        let s1t = tombstone_id(&first, "out-a", "s1")
            .unwrap_or_else(|| panic!("s1 tombstone on out-a in {mode:?}: {first:?}"));
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
            existence(&reply, "out-a", &s1t),
            Some(false),
            "one-sided retires without equal count {mode:?}: {reply:?}"
        );
    }
}

#[test]
fn one_sided_native_presence_retires_locally_empty_binding() {
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
        // One-sided locally-empty native retires locally into a tombstone
        // even with equal-count padding enabled: native-only retirement,
        // not host-preserved padding.
        let s1t = tombstone_id(&first, "out-a", "s1").unwrap_or_else(|| {
            // Unpruned first readback may still hold s1 live when the
            // equal-count spare logic applies; fall back to the live id.
            logical_for(&first, "out-a", "s1")
                .unwrap_or_else(|| panic!("s1 must bind on out-a in {mode:?}: {first:?}"))
        });
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
            existence(&reply, "out-a", &s1t),
            Some(false),
            "one-sided publishes tombstone {mode:?}: {reply:?}"
        );
        assert_eq!(reply.transfers.len(), 0, "no membership writes");
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
    let la = tombstone_id(&first, "out-a", "a").expect("a tombstone");
    let lc_a = tombstone_id(&first, "out-a", "c").expect("c tombstone a");
    let lb = tombstone_id(&first, "out-b", "b").expect("b tombstone");
    let lc_b = tombstone_id(&first, "out-b", "c").expect("c tombstone b");
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

/// Equal-count exception: equal ring lengths spare the one-sided extra `x`
/// from ordinary-empty pruning while the common `c` retires jointly; the
/// spared extra prunes once the longest ring shrinks.
#[test]
fn common_plus_extra_spares_only_extra_under_equal_count() {
    let mut engine = engine_host_owned(WorkspaceMode::PerOutputLocal, true);
    // out-b's occupied last desktop forces padding, so both outputs hold
    // three logical rows while only out-a observes `x`.
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
    assert_eq!(ring_ids(&first, "out-a").len(), 3, "live rows a: {first:?}");
    assert_eq!(ring_ids(&first, "out-b").len(), 3, "live rows b: {first:?}");
    let lx = logical_for(&first, "out-a", "x").expect("x bound");
    let lc_a = tombstone_id(&first, "out-a", "c").expect("c tombstone a");
    let lc_b = tombstone_id(&first, "out-b", "c").expect("c tombstone b");
    // Equal-ring exception: retiring both `x` and `c` would leave 2 vs 3
    // rows, so the equal-count removal clamp spares the earliest extra
    // (`x`) while the common native still retires jointly.
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
    assert_eq!(
        ring_ids(&reply, "out-a").len(),
        ring_ids(&reply, "out-b").len(),
        "equal-count parity survives the exception: {reply:?}"
    );
    // Shrinking the longest ring releases the exception: the out-b occupant
    // closes, out-b drops its occupied-final padding, and the spared extra
    // prunes while parity survives.
    let mut third = report(
        3,
        3,
        two_screens(),
        vec![],
        vec![
            empty_obs("out-a", &["v", "x", "c", "t"], "v"),
            empty_obs("out-b", &["v", "c", "t"], "v"),
        ],
        None,
        None,
        true,
    );
    third.closed = vec![WindowId("win-b".to_owned())];
    let shrunk = accepted(&mut engine, &third);
    let lxt = tombstone_id(&shrunk, "out-a", "x").expect("x tombstone after shrink");
    assert_eq!(
        existence(&shrunk, "out-a", &lxt),
        Some(false),
        "spared extra prunes once longest ring shrinks: {shrunk:?}"
    );
    assert!(
        !ring_ids(&shrunk, "out-a").contains(&lx),
        "pruned extra leaves the live ring: {shrunk:?}"
    );
    assert_eq!(
        ring_ids(&shrunk, "out-a").len(),
        ring_ids(&shrunk, "out-b").len(),
        "equal-count parity survives after shrink: {shrunk:?}"
    );
    assert!(
        ring_ids(&shrunk, "out-b").len() < ring_ids(&reply, "out-b").len(),
        "longest ring shrank: {shrunk:?} vs {reply:?}"
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
    let la2t = tombstone_id(&first, "out-a", "g2").expect("g2 tombstone a");
    let lb2t = tombstone_id(&first, "out-b", "g2").expect("g2 tombstone b");
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
        existence(&reply, "out-a", &la2t),
        Some(false),
        "joint retire: {reply:?}"
    );
    assert_eq!(
        existence(&reply, "out-b", &lb2t),
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
    // `a`/`b` retire on the first complete empty readback; tombstone IDs
    // persist as correlation metadata, never live bindings.
    let la = tombstone_id(&first, "out-a", "a")
        .or_else(|| logical_for(&first, "out-a", "a"))
        .expect("a retired or bound");
    let lb = tombstone_id(&first, "out-b", "b")
        .or_else(|| logical_for(&first, "out-b", "b"))
        .expect("b retired or bound");
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
    // Native deletion clears only the vanished marker: the out-a tombstone
    // stops while the out-b tombstone persists.
    assert!(
        tombstone_id(&third, "out-a", "a").is_none(),
        "deletion clears the out-a marker"
    );
    assert_eq!(
        existence(&third, "out-b", &lb),
        Some(false),
        "out-b tombstone persists"
    );
    assert_eq!(
        ring_ids(&third, "out-a"),
        ring_ids(&second, "out-a"),
        "live ring stable after native-order compaction"
    );
    assert_eq!(bound_ids(&third, "out-b"), bound_ids(&second, "out-b"));
    assert_eq!(third.transfers.len(), 0, "no membership writes");
    let mut complete = partial.clone();
    complete.request_seq = 4;
    complete.observation_seq = 4;
    complete.request_id = "req-4".into();
    complete.native[1] = native_obs_occ("out-b", &["v", "t"], Some("v"), &[Some(0), Some(1)]);
    let fourth = accepted(&mut engine, &complete);
    assert_eq!(
        ring_ids(&fourth, "out-a").len(),
        ring_ids(&fourth, "out-b").len()
    );
    assert_eq!(existence(&fourth, "out-a", &la), None);
    assert_eq!(existence(&fourth, "out-b", &lb), None);
    assert!(
        tombstone_id(&fourth, "out-a", "a").is_none()
            && tombstone_id(&fourth, "out-b", "b").is_none(),
        "balanced deletion clears both markers"
    );
    for output in ["out-a", "out-b"] {
        for (index, row) in fourth
            .workspaces
            .iter()
            .filter(|row| row.output.0 == output && row.existence)
            .enumerate()
        {
            assert_eq!(row.order, index);
        }
    }
}

/// Independent rosters keep the legacy adopt-everything disconnect path:
/// the removed output's memberless rows ride onto the survivor untouched.
#[test]
fn kde_disconnect_independent_roster_adopts_everything_unchanged() {
    let mut engine = engine_host_owned(WorkspaceMode::PerOutputLocal, true);
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            two_screens(),
            vec![
                fact_native("win-a", "out-a", Some("d2")),
                fact_native("win-b", "out-b", Some("d2")),
            ],
            vec![
                native_obs_occ("out-a", &["d1", "d2"], Some("d1"), &[Some(0), Some(1)]),
                native_obs_occ("out-b", &["d1", "d2"], Some("d1"), &[Some(0), Some(1)]),
            ],
            None,
            None,
            true,
        ),
    );
    let stale_b1 = logical_for(&first, "out-b", "d1").expect("B1 bound d1");
    let second = accepted(
        &mut engine,
        &report(
            2,
            2,
            vec![screen("out-a", 0)],
            vec![
                fact_native("win-a", "out-a", Some("d2")),
                fact_native("win-b", "out-a", Some("d2")),
            ],
            vec![native_obs_occ(
                "out-a",
                &["d1", "d2"],
                Some("d1"),
                &[Some(0), Some(2)],
            )],
            None,
            None,
            true,
        ),
    );
    assert!(
        ring_ids(&second, "out-a").iter().any(|id| id == &stale_b1),
        "independent path still adopts everything: {second:?}"
    );
}

fn published_view(reply: &tiler_core::world_report::WorldReplyV4, output: &str) -> Option<String> {
    reply
        .outputs
        .iter()
        .find(|row| row.output.0 == output)
        .and_then(|row| row.logical_view.clone())
}

fn view_transfer_pending(
    reply: &tiler_core::world_report::WorldReplyV4,
    output: &str,
    logical: &str,
) -> bool {
    reply.transfers.iter().any(|flight| {
        flight.target_output.0 == output
            && flight.target_workspace == logical
            && matches!(flight.kind, tiler_core::world_report::TransferKind::View)
    })
}

fn native_ref(native: &str) -> tiler_core::world_report::WorkspaceRef {
    tiler_core::world_report::WorkspaceRef::Native {
        output: OutputId("out-a".to_owned()),
        workspace: tiler_core::ids::NativeWorkspaceId::parse(native).expect("valid native"),
    }
}

/// Idle adoption publishes the complete known echo with no transfer:
/// the external move converges into the retained view and the shown
/// row stays live without any core reassertion.
#[test]
fn idle_complete_echo_adopts_into_retained_view_without_transfer() {
    let mut engine = engine_host_owned(WorkspaceMode::PerOutputLocal, false);
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            single_screen(),
            vec![
                fact_native("win-a", "out-a", Some("d1")),
                fact_native("win-b", "out-a", Some("d2")),
            ],
            vec![native_obs_occ(
                "out-a",
                &["d1", "d2", "d3", "d4"],
                Some("d1"),
                &[Some(1), Some(1), Some(0), Some(0)],
            )],
            None,
            None,
            true,
        ),
    );
    let d2 = logical_for(&first, "out-a", "d2").expect("d2 bound");
    let mut reply = first;
    for seq in 2..=3 {
        let mut moved = report(
            seq,
            seq,
            single_screen(),
            vec![
                fact_native("win-a", "out-a", Some("d1")),
                fact_native("win-b", "out-a", Some("d2")),
            ],
            vec![native_obs_occ(
                "out-a",
                &["d1", "d2", "d3", "d4"],
                Some("d2"),
                &[Some(1), Some(1), Some(0), Some(0)],
            )],
            None,
            None,
            true,
        );
        moved.focused_global = "win-b".to_owned();
        reply = accepted(&mut engine, &moved);
    }
    assert_eq!(
        published_view(&reply, "out-a"),
        Some(d2.clone()),
        "idle echo owns publication: {reply:?}"
    );
    assert_eq!(reply.transfers.len(), 0, "adoption actuates nothing");
    assert_eq!(
        existence(&reply, "out-a", &d2),
        Some(true),
        "shown row stays live: {reply:?}"
    );
}

/// Shown-but-empty native stays until the user leaves it: closing the
/// window on the adopted desktop keeps the row live with no tombstone.
#[test]
fn idle_adopted_shown_empty_stays_until_leaving() {
    let mut engine = engine_host_owned(WorkspaceMode::PerOutputLocal, false);
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            single_screen(),
            vec![
                fact_native("win-a", "out-a", Some("d1")),
                fact_native("win-b", "out-a", Some("d2")),
            ],
            vec![native_obs_occ(
                "out-a",
                &["d1", "d2", "d3", "d4"],
                Some("d1"),
                &[Some(1), Some(1), Some(0), Some(0)],
            )],
            None,
            None,
            true,
        ),
    );
    let d2 = logical_for(&first, "out-a", "d2").expect("d2 bound");
    let mut moved = report(
        2,
        2,
        single_screen(),
        vec![
            fact_native("win-a", "out-a", Some("d1")),
            fact_native("win-b", "out-a", Some("d2")),
        ],
        vec![native_obs_occ(
            "out-a",
            &["d1", "d2", "d3", "d4"],
            Some("d2"),
            &[Some(1), Some(1), Some(0), Some(0)],
        )],
        None,
        None,
        true,
    );
    moved.focused_global = "win-b".to_owned();
    let adopted = accepted(&mut engine, &moved);
    assert_eq!(published_view(&adopted, "out-a"), Some(d2.clone()));
    let mut closed = report(
        3,
        3,
        single_screen(),
        vec![fact_native("win-a", "out-a", Some("d1"))],
        vec![native_obs_occ(
            "out-a",
            &["d1", "d2", "d3", "d4"],
            Some("d2"),
            &[Some(1), Some(0), Some(0), Some(0)],
        )],
        None,
        None,
        true,
    );
    closed.closed = vec![WindowId("win-b".to_owned())];
    let reply = accepted(&mut engine, &closed);
    assert_eq!(
        published_view(&reply, "out-a"),
        Some(d2.clone()),
        "shown empty keeps the view: {reply:?}"
    );
    assert_eq!(
        existence(&reply, "out-a", &d2),
        Some(true),
        "shown empty stays live: {reply:?}"
    );
    assert!(
        tombstone_id(&reply, "out-a", "d2").is_none(),
        "no deletion intent for the shown native: {reply:?}"
    );
    assert_eq!(reply.transfers.len(), 0, "no membership writes");
}

/// Leaving the shown empty prunes the row and authorizes native
/// deletion: the tombstone publishes and the fake host may delete.
#[test]
fn idle_leave_prunes_row_and_authorizes_native_deletion() {
    let mut engine = engine_host_owned(WorkspaceMode::PerOutputLocal, false);
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            single_screen(),
            vec![
                fact_native("win-a", "out-a", Some("d1")),
                fact_native("win-b", "out-a", Some("d2")),
            ],
            vec![native_obs_occ(
                "out-a",
                &["d1", "d2", "d3", "d4"],
                Some("d1"),
                &[Some(1), Some(1), Some(0), Some(0)],
            )],
            None,
            None,
            true,
        ),
    );
    let d2 = logical_for(&first, "out-a", "d2").expect("d2 bound");
    let mut moved = report(
        2,
        2,
        single_screen(),
        vec![
            fact_native("win-a", "out-a", Some("d1")),
            fact_native("win-b", "out-a", Some("d2")),
        ],
        vec![native_obs_occ(
            "out-a",
            &["d1", "d2", "d3", "d4"],
            Some("d2"),
            &[Some(1), Some(1), Some(0), Some(0)],
        )],
        None,
        None,
        true,
    );
    moved.focused_global = "win-b".to_owned();
    let _ = accepted(&mut engine, &moved);
    let mut closed = report(
        3,
        3,
        single_screen(),
        vec![fact_native("win-a", "out-a", Some("d1"))],
        vec![native_obs_occ(
            "out-a",
            &["d1", "d2", "d3", "d4"],
            Some("d2"),
            &[Some(1), Some(0), Some(0), Some(0)],
        )],
        None,
        None,
        true,
    );
    closed.closed = vec![WindowId("win-b".to_owned())];
    let _ = accepted(&mut engine, &closed);
    let reply = accepted(
        &mut engine,
        &report(
            4,
            4,
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
    let tomb = tombstone_id(&reply, "out-a", "d2").expect("left empty retires");
    assert_eq!(existence(&reply, "out-a", &tomb), Some(false));
    assert!(
        !ring_ids(&reply, "out-a").contains(&d2),
        "left row leaves the ring: {reply:?}"
    );
    let native = vec![native_obs_occ(
        "out-a",
        &["d1", "d2", "d3", "d4"],
        Some("d1"),
        &[Some(1), Some(0), Some(0), Some(0)],
    )];
    assert!(
        host_would_delete(&reply, &native, &[], true, false, "out-a", "d2"),
        "globally empty unshown native deletes: {reply:?}"
    );
    assert_eq!(reply.transfers.len(), 0, "no membership writes");
}

/// Unknown occupancy still adopts the complete known current into the
/// view (adoption needs only the complete list) while retirement stays
/// fail-closed: the shown row is never a tombstone.
#[test]
fn idle_unknown_occupancy_adopts_view_without_retiring() {
    let mut engine = engine_host_owned(WorkspaceMode::PerOutputLocal, false);
    let first = setup_four_unpruned(&mut engine);
    let d2 = logical_for(&first, "out-a", "d2").expect("d2 bound");
    let moved = report(
        2,
        2,
        single_screen(),
        vec![],
        vec![native_obs_occ(
            "out-a",
            &["d1", "d2", "d3", "d4"],
            Some("d2"),
            &[Some(0), None, None, Some(0)],
        )],
        None,
        None,
        true,
    );
    let reply = accepted(&mut engine, &moved);
    assert_eq!(
        published_view(&reply, "out-a"),
        Some(d2.clone()),
        "complete current adopts despite unknown occupancy: {reply:?}"
    );
    assert_eq!(
        existence(&reply, "out-a", &d2),
        Some(true),
        "unknown occupancy never retires: {reply:?}"
    );
    assert!(
        tombstone_id(&reply, "out-a", "d2").is_none(),
        "no tombstone under unknown occupancy: {reply:?}"
    );
}

/// Incomplete evidence never adopts: an incomplete screen or a partial
/// inventory keeps the retained view.
#[test]
fn idle_incomplete_evidence_keeps_retained_view() {
    for incomplete in [true, false] {
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
        let d1 = logical_for(&first, "out-a", "d1").expect("d1 bound");
        let screens = if incomplete {
            vec![incomplete_screen("out-a", 0)]
        } else {
            single_screen()
        };
        let reply = accepted(
            &mut engine,
            &report(
                2,
                2,
                screens,
                vec![fact_native("win-a", "out-a", Some("d1"))],
                vec![native_obs_occ(
                    "out-a",
                    &["d1", "d2", "d3", "d4"],
                    Some("d2"),
                    &[Some(1), Some(0), Some(0), Some(0)],
                )],
                None,
                None,
                incomplete,
            ),
        );
        assert_eq!(
            published_view(&reply, "out-a"),
            Some(d1.clone()),
            "incomplete evidence never adopts (screen-incomplete={incomplete}): {reply:?}"
        );
    }
}

/// A pending view transfer is never overridden by a stale echo: the
/// flight stays pending and publication names its target until fresh
/// evidence reconciles it.
#[test]
fn idle_stale_echo_never_overrides_pending_view_transfer() {
    let mut engine = engine_host_owned(WorkspaceMode::PerOutputLocal, false);
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            single_screen(),
            vec![
                fact_native("win-a", "out-a", Some("d1")),
                fact_native("win-b", "out-a", Some("d2")),
            ],
            vec![native_obs_occ(
                "out-a",
                &["d1", "d2", "d3", "d4"],
                Some("d1"),
                &[Some(1), Some(1), Some(0), Some(0)],
            )],
            None,
            None,
            true,
        ),
    );
    let d2 = logical_for(&first, "out-a", "d2").expect("d2 bound");
    let flight = accepted(
        &mut engine,
        &report(
            2,
            2,
            single_screen(),
            vec![
                fact_native("win-a", "out-a", Some("d1")),
                fact_native("win-b", "out-a", Some("d2")),
            ],
            vec![native_obs_occ(
                "out-a",
                &["d1", "d2", "d3", "d4"],
                Some("d1"),
                &[Some(1), Some(1), Some(0), Some(0)],
            )],
            Some(1),
            Some(WorldCommand::SelectWorkspace {
                output: OutputId("out-a".to_owned()),
                workspace: tiler_core::directional::WorkspaceId(d2.clone()),
            }),
            true,
        ),
    );
    assert!(
        view_transfer_pending(&flight, "out-a", &d2),
        "flight retained: {flight:?}"
    );
    // Stale repeat: the host still reports the old current while the
    // flight is outstanding. The flight holds; nothing reverts.
    let reply = accepted(
        &mut engine,
        &report(
            3,
            3,
            single_screen(),
            vec![
                fact_native("win-a", "out-a", Some("d1")),
                fact_native("win-b", "out-a", Some("d2")),
            ],
            vec![native_obs_occ(
                "out-a",
                &["d1", "d2", "d3", "d4"],
                Some("d1"),
                &[Some(1), Some(1), Some(0), Some(0)],
            )],
            None,
            None,
            true,
        ),
    );
    assert!(
        view_transfer_pending(&reply, "out-a", &d2),
        "stale echo keeps the flight: {reply:?}"
    );
    assert_eq!(
        published_view(&reply, "out-a"),
        Some(d2.clone()),
        "stale echo never overrides the pending target: {reply:?}"
    );
}

/// Adoption is observation-only and mints no actuation, so it stays
/// ungated on the gesture barrier: existing holds gate writes and
/// deletion, not the observed idle view.
#[test]
fn idle_adoption_ignores_gesture_barrier_without_actuation() {
    let mut engine = engine_host_owned(WorkspaceMode::PerOutputLocal, false);
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            single_screen(),
            vec![
                fact_native("win-a", "out-a", Some("d1")),
                fact_native("win-b", "out-a", Some("d2")),
            ],
            vec![native_obs_occ(
                "out-a",
                &["d1", "d2", "d3", "d4"],
                Some("d1"),
                &[Some(1), Some(1), Some(0), Some(0)],
            )],
            None,
            None,
            true,
        ),
    );
    let d2 = logical_for(&first, "out-a", "d2").expect("d2 bound");
    let mut moved = report(
        2,
        2,
        single_screen(),
        vec![
            fact_native("win-a", "out-a", Some("d1")),
            fact_native("win-b", "out-a", Some("d2")),
        ],
        vec![native_obs_occ(
            "out-a",
            &["d1", "d2", "d3", "d4"],
            Some("d2"),
            &[Some(1), Some(1), Some(0), Some(0)],
        )],
        None,
        None,
        true,
    );
    moved.gesture_barrier = true;
    moved.focused_global = "win-b".to_owned();
    let reply = accepted(&mut engine, &moved);
    assert_eq!(
        published_view(&reply, "out-a"),
        Some(d2.clone()),
        "gesture never strands the view: {reply:?}"
    );
    assert_eq!(reply.transfers.len(), 0, "adoption actuates nothing");
}

/// D2 prune fallback: a third-location drop retires the flight without
/// adopting, then ordinary pruning removes the abandoned target row.
/// The view-sanity fallback must publish the complete known echo the
/// host shows (d4), never the ring head (d1).
#[test]
fn idle_prune_fallback_prefers_known_echo_over_ring_head() {
    let mut engine = engine_host_owned(WorkspaceMode::PerOutputLocal, false);
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            single_screen(),
            vec![
                fact_native("win-a", "out-a", Some("d1")),
                fact_native("win-b", "out-a", Some("d2")),
            ],
            vec![native_obs_occ(
                "out-a",
                &["d1", "d2", "d3", "d4"],
                Some("d1"),
                &[Some(1), Some(1), None, Some(0)],
            )],
            None,
            None,
            true,
        ),
    );
    let d2 = logical_for(&first, "out-a", "d2").expect("d2 bound");
    let d4 = logical_for(&first, "out-a", "d4").expect("d4 bound");
    let flight = accepted(
        &mut engine,
        &report(
            2,
            2,
            single_screen(),
            vec![
                fact_native("win-a", "out-a", Some("d1")),
                fact_native("win-b", "out-a", Some("d2")),
            ],
            vec![native_obs_occ(
                "out-a",
                &["d1", "d2", "d3", "d4"],
                Some("d1"),
                &[Some(1), Some(1), Some(0), Some(0)],
            )],
            Some(1),
            Some(WorldCommand::SelectWorkspace {
                output: OutputId("out-a".to_owned()),
                workspace: tiler_core::directional::WorkspaceId(d2.clone()),
            }),
            true,
        ),
    );
    assert!(
        view_transfer_pending(&flight, "out-a", &d2),
        "flight retained: {flight:?}"
    );
    // The user moved to d4 after the proven write path diverged: raw
    // post names d4 (ringed, neither target d2 nor baseline d1), so the
    // flight drops as third-location external input without adopting.
    // win-b closes alongside, so ordinary pruning removes the abandoned
    // d2 row in the same pass.
    let mut external = report(
        3,
        3,
        single_screen(),
        vec![fact_native("win-a", "out-a", Some("d1"))],
        vec![native_obs_occ(
            "out-a",
            &["d1", "d2", "d3", "d4"],
            Some("d4"),
            &[Some(1), Some(0), Some(0), Some(0)],
        )],
        None,
        None,
        true,
    );
    external.closed = vec![WindowId("win-b".to_owned())];
    external.view_attempts = vec![tiler_core::world_report::ViewAttempt {
        operation_id: "op-1".to_owned(),
        target_output: OutputId("out-a".to_owned()),
        target_workspace: d2.clone(),
        disposition: FrameSetter::Accepted,
        pre: Some(native_ref("d1")),
        post: Some(native_ref("d4")),
        baseline: Some(native_ref("d1")),
        attempt_ms: 0,
    }];
    let reply = accepted(&mut engine, &external);
    assert!(
        !view_transfer_pending(&reply, "out-a", &d2),
        "third-location external input drops the flight: {reply:?}"
    );
    assert_eq!(
        published_view(&reply, "out-a"),
        Some(d4.clone()),
        "fallback publishes the known echo, never the ring head: {reply:?}"
    );
    assert!(
        tombstone_id(&reply, "out-a", "d2").is_some(),
        "abandoned row prunes: {reply:?}"
    );
    assert_eq!(reply.transfers.len(), 0, "no membership writes");
}

/// D2 idle-divergence invariant (policy-neutral): a complete, known native
/// current that moves with no outstanding command or retained flight must
/// not leave a settled published/native view divergence. Either the
/// published view's native binding matches the known current (adoption) or
/// an explicit view transfer owns convergence to the published view
/// (bounded re-assertion). The actually-shown native stays live either way:
/// the raw-current veto is preserved, never weakened, and no deletion of
/// the shown native is required.
#[test]
fn idle_external_current_move_leaves_no_unexplained_view_divergence() {
    let mut engine = engine_host_owned(WorkspaceMode::PerOutputLocal, false);
    // Both occupied rows stay live; unrelated empty rows may retire.
    // The only view change below is the external current move.
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            single_screen(),
            vec![
                fact_native("win-a", "out-a", Some("d1")),
                fact_native("win-b", "out-a", Some("d2")),
            ],
            vec![native_obs_occ(
                "out-a",
                &["d1", "d2", "d3", "d4"],
                Some("d1"),
                &[Some(1), Some(1), Some(0), Some(0)],
            )],
            None,
            None,
            true,
        ),
    );
    let d2 = logical_for(&first, "out-a", "d2").expect("d2 bound");
    // Idle external move: compositor current d1 -> d2 with no core command
    // and no membership change. Repeated complete reports settle the state,
    // so any divergence is idle steady state rather than one-frame lag.
    let mut reply = first;
    for seq in 2..=4 {
        let mut moved = report(
            seq,
            seq,
            single_screen(),
            vec![
                fact_native("win-a", "out-a", Some("d1")),
                fact_native("win-b", "out-a", Some("d2")),
            ],
            vec![native_obs_occ(
                "out-a",
                &["d1", "d2", "d3", "d4"],
                Some("d2"),
                &[Some(1), Some(1), Some(0), Some(0)],
            )],
            None,
            None,
            true,
        );
        // The real trigger activates win-b on d2: focus follows the move.
        moved.focused_global = "win-b".to_owned();
        reply = accepted(&mut engine, &moved);
    }
    // Shown-native protection holds: the echo-named row stays live.
    assert_eq!(
        existence(&reply, "out-a", &d2),
        Some(true),
        "actually-shown native stays live: {reply:?}"
    );
    // Policy-neutral convergence: either the published view is bound to the
    // known idle current, or a view transfer explicitly owns convergence to
    // the published view.
    let published = reply
        .outputs
        .iter()
        .find(|row| row.output.0 == "out-a")
        .and_then(|row| row.logical_view.clone())
        .expect("published view");
    let published_native = bound_ids(&reply, "out-a")
        .into_iter()
        .find(|(id, _)| id == &published)
        .map(|(_, native)| native);
    let view_owned = reply.transfers.iter().any(|flight| {
        flight.target_output.0 == "out-a"
            && flight.target_workspace == published
            && matches!(flight.kind, tiler_core::world_report::TransferKind::View)
    });
    assert!(
        published_native.as_deref() == Some("d2") || view_owned,
        "settled idle current d2 must match the published view binding ({published_native:?}) or own it with a view transfer: {reply:?}"
    );
}

/// Unknown current (`None`) and a missing native row hold the retained
/// view; the next complete current adopts with no transfer.
#[test]
fn idle_unknown_current_none_then_missing_row_hold_then_adopt() {
    let mut engine = engine_host_owned(WorkspaceMode::PerOutputLocal, false);
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            single_screen(),
            vec![
                fact_native("win-a", "out-a", Some("d1")),
                fact_native("win-b", "out-a", Some("d2")),
            ],
            vec![native_obs_occ(
                "out-a",
                &["d1", "d2", "d3", "d4"],
                Some("d1"),
                &[Some(1), Some(1), Some(0), Some(0)],
            )],
            None,
            None,
            true,
        ),
    );
    let d1 = logical_for(&first, "out-a", "d1").expect("d1 bound");
    let d2 = logical_for(&first, "out-a", "d2").expect("d2 bound");
    // `None` current is unknown: the retained view holds.
    let none_reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            single_screen(),
            vec![
                fact_native("win-a", "out-a", Some("d1")),
                fact_native("win-b", "out-a", Some("d2")),
            ],
            vec![native_obs_occ(
                "out-a",
                &["d1", "d2", "d3", "d4"],
                None,
                &[Some(1), Some(1), Some(0), Some(0)],
            )],
            None,
            None,
            true,
        ),
    );
    assert_eq!(
        published_view(&none_reply, "out-a"),
        Some(d1.clone()),
        "None current holds: {none_reply:?}"
    );
    // A missing native row is also unknown: the retained view holds.
    let missing_reply = accepted(
        &mut engine,
        &report(
            3,
            3,
            single_screen(),
            vec![
                fact_native("win-a", "out-a", Some("d1")),
                fact_native("win-b", "out-a", Some("d2")),
            ],
            vec![],
            None,
            None,
            true,
        ),
    );
    assert_eq!(
        published_view(&missing_reply, "out-a"),
        Some(d1.clone()),
        "missing native row holds: {missing_reply:?}"
    );
    // The next complete current adopts with no transfer.
    let mut moved = report(
        4,
        4,
        single_screen(),
        vec![
            fact_native("win-a", "out-a", Some("d1")),
            fact_native("win-b", "out-a", Some("d2")),
        ],
        vec![native_obs_occ(
            "out-a",
            &["d1", "d2", "d3", "d4"],
            Some("d2"),
            &[Some(1), Some(1), Some(0), Some(0)],
        )],
        None,
        None,
        true,
    );
    moved.focused_global = "win-b".to_owned();
    let reply = accepted(&mut engine, &moved);
    assert_eq!(
        published_view(&reply, "out-a"),
        Some(d2.clone()),
        "complete current adopts after unknown legs: {reply:?}"
    );
    assert_eq!(reply.transfers.len(), 0, "adoption actuates nothing");
}

/// D2 live-trace shape (per-output-local, two outputs): X on A/ws2, Y+Z
/// on B/ws2 while B shows ws3; external activation moves B to ws2;
/// send-stay moves Z to ws3; X closes and A is left; Y closes leaving B
/// showing an empty ws2 with no mismatch; leaving B retires the surplus
/// with host deletion allowed. Real send command; closes via `closed`.
#[test]
fn idle_d2_two_output_activation_send_close_leave_journey() {
    let mut engine = engine_host_owned(WorkspaceMode::PerOutputLocal, false);
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            two_screens(),
            vec![
                fact_native("win-w", "out-a", Some("a1")),
                fact_native("win-x", "out-a", Some("a2")),
                fact_native("win-y", "out-b", Some("b2")),
                fact_native("win-z", "out-b", Some("b2")),
                fact_native("win-q", "out-b", Some("b4")),
            ],
            vec![
                native_obs_occ(
                    "out-a",
                    &["a1", "a2", "a3"],
                    Some("a2"),
                    &[Some(1), Some(1), Some(0)],
                ),
                native_obs_occ(
                    "out-b",
                    &["b2", "b4", "b3"],
                    Some("b3"),
                    &[Some(2), Some(1), Some(0)],
                ),
            ],
            None,
            None,
            true,
        ),
    );
    let a2 = logical_for(&first, "out-a", "a2").expect("a2 bound");
    let b2 = logical_for(&first, "out-b", "b2").expect("b2 bound");
    let b3 = logical_for(&first, "out-b", "b3").expect("b3 bound");
    assert_eq!(published_view(&first, "out-a"), Some(a2.clone()));
    assert_eq!(
        published_view(&first, "out-b"),
        logical_for(&first, "out-b", "b3"),
        "B shows ws3 while Y/Z sit on ws2: {first:?}"
    );
    // External activation: B current ws3 -> ws2 with no core command.
    // Send-stay Z to ws3 rides the same report as a real command.
    let mut send_report = report(
        2,
        2,
        two_screens(),
        vec![
            fact_native("win-w", "out-a", Some("a1")),
            fact_native("win-x", "out-a", Some("a2")),
            fact_native("win-y", "out-b", Some("b2")),
            fact_native("win-z", "out-b", Some("b2")),
            fact_native("win-q", "out-b", Some("b4")),
        ],
        vec![
            native_obs_occ(
                "out-a",
                &["a1", "a2", "a3"],
                Some("a2"),
                &[Some(1), Some(1), Some(0)],
            ),
            native_obs_occ(
                "out-b",
                &["b2", "b4", "b3"],
                Some("b2"),
                &[Some(2), Some(1), Some(0)],
            ),
        ],
        Some(1),
        Some(WorldCommand::Send {
            window: tiler_core::directional::WindowSubject::Window(WindowId("win-z".to_owned())),
            target_output: OutputId("out-b".to_owned()),
            target_workspace: tiler_core::directional::WorkspaceId(b3.clone()),
            follow: false,
        }),
        true,
    );
    send_report.focused_global = "win-z".to_owned();
    let activated = accepted(&mut engine, &send_report);
    assert_eq!(
        published_view(&activated, "out-b"),
        Some(b2.clone()),
        "external activation adopts ws2: {activated:?}"
    );
    assert!(
        activated
            .transfers
            .iter()
            .any(|flight| { flight.target_output.0 == "out-b" && flight.target_workspace == b3 }),
        "send-stay retained: {activated:?}"
    );
    // Arrival: Z lands on ws3, follow=false keeps the B view on ws2.
    let arrived = accepted(
        &mut engine,
        &report(
            3,
            3,
            two_screens(),
            vec![
                fact_native("win-w", "out-a", Some("a1")),
                fact_native("win-x", "out-a", Some("a2")),
                fact_native("win-y", "out-b", Some("b2")),
                fact_native("win-z", "out-b", Some("b3")),
                fact_native("win-q", "out-b", Some("b4")),
            ],
            vec![
                native_obs_occ(
                    "out-a",
                    &["a1", "a2", "a3"],
                    Some("a2"),
                    &[Some(1), Some(1), Some(0)],
                ),
                native_obs_occ(
                    "out-b",
                    &["b2", "b4", "b3"],
                    Some("b2"),
                    &[Some(1), Some(1), Some(1)],
                ),
            ],
            None,
            None,
            true,
        ),
    );
    assert_eq!(
        published_view(&arrived, "out-b"),
        Some(b2.clone()),
        "send-stay keeps the source view: {arrived:?}"
    );
    // Close X and leave A: A current ws2 -> ws1, the emptied ws2 retires.
    let mut leave_a = report(
        4,
        4,
        two_screens(),
        vec![
            fact_native("win-w", "out-a", Some("a1")),
            fact_native("win-y", "out-b", Some("b2")),
            fact_native("win-z", "out-b", Some("b3")),
            fact_native("win-q", "out-b", Some("b4")),
        ],
        vec![
            native_obs_occ(
                "out-a",
                &["a1", "a2", "a3"],
                Some("a1"),
                &[Some(1), Some(0), Some(0)],
            ),
            native_obs_occ(
                "out-b",
                &["b2", "b4", "b3"],
                Some("b2"),
                &[Some(1), Some(1), Some(1)],
            ),
        ],
        None,
        None,
        true,
    );
    leave_a.closed = vec![WindowId("win-x".to_owned())];
    let left_a = accepted(&mut engine, &leave_a);
    assert!(
        tombstone_id(&left_a, "out-a", "a2").is_some(),
        "left ws2 retires on A: {left_a:?}"
    );
    assert_eq!(
        existence(&left_a, "out-a", &a2),
        Some(false),
        "left row is removal intent: {left_a:?}"
    );
    // Close Y: B shows an empty ws2 that stays adopted with no mismatch
    // and no deletion intent for the shown native.
    let mut close_y = report(
        5,
        5,
        two_screens(),
        vec![
            fact_native("win-w", "out-a", Some("a1")),
            fact_native("win-z", "out-b", Some("b3")),
            fact_native("win-q", "out-b", Some("b4")),
        ],
        vec![
            native_obs_occ(
                "out-a",
                &["a1", "a2", "a3"],
                Some("a1"),
                &[Some(1), Some(0), Some(0)],
            ),
            native_obs_occ(
                "out-b",
                &["b2", "b4", "b3"],
                Some("b2"),
                &[Some(0), Some(1), Some(1)],
            ),
        ],
        None,
        None,
        true,
    );
    close_y.closed = vec![WindowId("win-y".to_owned())];
    let shown_empty = accepted(&mut engine, &close_y);
    assert_eq!(
        published_view(&shown_empty, "out-b"),
        Some(b2.clone()),
        "B shown-empty ws2 stays adopted: {shown_empty:?}"
    );
    assert_eq!(
        existence(&shown_empty, "out-b", &b2),
        Some(true),
        "shown empty stays live: {shown_empty:?}"
    );
    assert!(
        tombstone_id(&shown_empty, "out-b", "b2").is_none(),
        "no deletion intent for the shown native: {shown_empty:?}"
    );
    let shown_native = bound_ids(&shown_empty, "out-b")
        .into_iter()
        .find(|(id, _)| id == &published_view(&shown_empty, "out-b").expect("view"))
        .map(|(_, native)| native);
    assert_eq!(
        shown_native.as_deref(),
        Some("b2"),
        "no unowned publish mismatch: {shown_empty:?}"
    );
    assert_eq!(shown_empty.transfers.len(), 0, "no membership writes");
    // Close Z and leave B: the unshown, globally empty ws2 retires and
    // the fake host may delete it.
    let mut leave_b = report(
        6,
        6,
        two_screens(),
        vec![
            fact_native("win-w", "out-a", Some("a1")),
            fact_native("win-q", "out-b", Some("b4")),
        ],
        vec![
            native_obs_occ(
                "out-a",
                &["a1", "a2", "a3"],
                Some("a1"),
                &[Some(1), Some(0), Some(0)],
            ),
            native_obs_occ(
                "out-b",
                &["b2", "b4", "b3"],
                Some("b3"),
                &[Some(0), Some(1), Some(0)],
            ),
        ],
        None,
        None,
        true,
    );
    leave_b.closed = vec![WindowId("win-z".to_owned())];
    let left_b = accepted(&mut engine, &leave_b);
    let tomb = tombstone_id(&left_b, "out-b", "b2").expect("left ws2 retires");
    assert_eq!(existence(&left_b, "out-b", &tomb), Some(false));
    assert!(
        !ring_ids(&left_b, "out-b").contains(&b2),
        "left row leaves the ring: {left_b:?}"
    );
    let native_rows = vec![
        native_obs_occ(
            "out-a",
            &["a1", "a2", "a3"],
            Some("a1"),
            &[Some(1), Some(0), Some(0)],
        ),
        native_obs_occ(
            "out-b",
            &["b2", "b4", "b3"],
            Some("b3"),
            &[Some(0), Some(1), Some(0)],
        ),
    ];
    assert!(
        host_would_delete(
            &left_b,
            &native_rows,
            &[
                fact_native("win-w", "out-a", Some("a1")),
                fact_native("win-q", "out-b", Some("b4"))
            ],
            true,
            false,
            "out-b",
            "b2"
        ),
        "globally empty unshown ws2 deletes: {left_b:?}"
    );
}

/// Pending migration target and source refill survive a stale echo: both
/// flights stay pending with unchanged publication, and the refill child
/// exists throughout the interval.
#[test]
fn idle_migration_target_and_refill_hold_through_stale_echo() {
    let mut engine = engine_host_owned(WorkspaceMode::PerOutputLocal, false);
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            two_screens(),
            vec![
                fact_native("win-a", "out-a", Some("a1")),
                fact_native("win-b", "out-b", Some("b1")),
            ],
            vec![
                native_obs_occ("out-a", &["a1", "a2"], Some("a1"), &[Some(1), Some(0)]),
                native_obs_occ("out-b", &["b1", "b2"], Some("b1"), &[Some(1), Some(0)]),
            ],
            None,
            None,
            true,
        ),
    );
    let migrated = logical_for(&first, "out-a", "a1").expect("a1 bound");
    let drained = accepted(
        &mut engine,
        &report(
            2,
            2,
            two_screens(),
            vec![
                fact_native("win-a", "out-a", Some("a1")),
                fact_native("win-b", "out-b", Some("b1")),
            ],
            vec![
                native_obs_occ("out-a", &["a1", "a2"], Some("a1"), &[Some(1), Some(0)]),
                native_obs_occ("out-b", &["b1", "b2"], Some("b1"), &[Some(1), Some(0)]),
            ],
            Some(1),
            Some(WorldCommand::MigrateWorkspace {
                source_output: OutputId("out-a".to_owned()),
                target_output: OutputId("out-b".to_owned()),
                workspace: tiler_core::directional::WorkspaceId(migrated.clone()),
            }),
            true,
        ),
    );
    let migration_op = drained
        .transfers
        .iter()
        .find(|flight| {
            matches!(
                flight.kind,
                tiler_core::world_report::TransferKind::Migration
            )
        })
        .expect("migration retained")
        .operation_id
        .clone();
    let refill = drained
        .transfers
        .iter()
        .find(|flight| flight.parent.as_deref() == Some(migration_op.as_str()))
        .expect("refill child retained");
    let refill_op = refill.operation_id.clone();
    let refill_ws = refill.target_workspace.clone();
    let view_before_a = published_view(&drained, "out-a");
    let view_before_b = published_view(&drained, "out-b");
    // Stale repeat: both currents still name the pre-migration views while
    // the flights are outstanding. Neither flight settles and the refill
    // child still exists; publication does not move.
    let reply = accepted(
        &mut engine,
        &report(
            3,
            3,
            two_screens(),
            vec![
                fact_native("win-a", "out-a", Some("a1")),
                fact_native("win-b", "out-b", Some("b1")),
            ],
            vec![
                native_obs_occ("out-a", &["a1", "a2"], Some("a1"), &[Some(1), Some(0)]),
                native_obs_occ("out-b", &["b1", "b2"], Some("b1"), &[Some(1), Some(0)]),
            ],
            None,
            None,
            true,
        ),
    );
    assert!(
        reply
            .transfers
            .iter()
            .any(|flight| flight.operation_id == migration_op),
        "stale echo keeps the migration target: {reply:?}"
    );
    assert!(
        reply
            .transfers
            .iter()
            .any(|flight| flight.operation_id == refill_op),
        "stale echo keeps the source refill child: {reply:?}"
    );
    assert_eq!(
        published_view(&reply, "out-a"),
        view_before_a,
        "stale echo never overrides the refill: {reply:?}"
    );
    assert_eq!(
        published_view(&reply, "out-b"),
        view_before_b,
        "stale echo never overrides the migration target: {reply:?}"
    );
    let _ = refill_ws;
}

/// GlobalUnique keeps per-output native bindings disjoint: the same native
/// name on two outputs maps to different logicals, so idle adoption on
/// both outputs cannot publish a duplicate logical view.
#[test]
fn idle_global_unique_adopts_distinct_logicals_per_output() {
    let mut engine = engine_host_owned(WorkspaceMode::GlobalUnique, false);
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            two_screens(),
            vec![
                fact_native("win-a", "out-a", Some("g2")),
                fact_native("win-b", "out-b", Some("g2")),
            ],
            vec![
                native_obs_occ("out-a", &["g1", "g2"], Some("g1"), &[Some(0), Some(1)]),
                native_obs_occ("out-b", &["g1", "g2"], Some("g1"), &[Some(0), Some(1)]),
            ],
            None,
            None,
            true,
        ),
    );
    let la2 = logical_for(&first, "out-a", "g2").expect("g2 bound a");
    let lb2 = logical_for(&first, "out-b", "g2").expect("g2 bound b");
    assert_ne!(la2, lb2, "scoped bindings are disjoint: {first:?}");
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            two_screens(),
            vec![
                fact_native("win-a", "out-a", Some("g2")),
                fact_native("win-b", "out-b", Some("g2")),
            ],
            vec![
                native_obs_occ("out-a", &["g1", "g2"], Some("g2"), &[Some(0), Some(1)]),
                native_obs_occ("out-b", &["g1", "g2"], Some("g2"), &[Some(0), Some(1)]),
            ],
            None,
            None,
            true,
        ),
    );
    assert_eq!(
        published_view(&reply, "out-a"),
        Some(la2.clone()),
        "out-a adopts its own g2 logical: {reply:?}"
    );
    assert_eq!(
        published_view(&reply, "out-b"),
        Some(lb2.clone()),
        "out-b adopts its own g2 logical: {reply:?}"
    );
    assert_ne!(
        published_view(&reply, "out-a"),
        published_view(&reply, "out-b"),
        "no duplicate logical view across outputs: {reply:?}"
    );
    assert_eq!(reply.transfers.len(), 0, "adoption actuates nothing");
}

/// Shared mode keeps one mirrored view: an idle external current change
/// stays mirrored on both outputs with no swap policy.
#[test]
fn idle_shared_external_change_stays_mirrored() {
    let mut engine = engine_host_owned(WorkspaceMode::Shared, false);
    let first = accepted(
        &mut engine,
        &report(
            1,
            1,
            two_screens(),
            vec![fact_native("win-a", "out-a", Some("g2"))],
            vec![
                native_obs_occ("out-a", &["g1", "g2"], Some("g1"), &[Some(0), Some(1)]),
                native_obs_occ("out-b", &["g1", "g2"], Some("g1"), &[Some(0), Some(1)]),
            ],
            None,
            None,
            true,
        ),
    );
    let g2 = logical_for(&first, "out-a", "g2").expect("g2 bound");
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            two_screens(),
            vec![fact_native("win-a", "out-a", Some("g2"))],
            vec![
                native_obs_occ("out-a", &["g1", "g2"], Some("g2"), &[Some(0), Some(1)]),
                native_obs_occ("out-b", &["g1", "g2"], Some("g2"), &[Some(0), Some(1)]),
            ],
            None,
            None,
            true,
        ),
    );
    assert_eq!(
        published_view(&reply, "out-a"),
        Some(g2.clone()),
        "shared mirrors the external change on out-a: {reply:?}"
    );
    assert_eq!(
        published_view(&reply, "out-b"),
        Some(g2.clone()),
        "shared mirrors the external change on out-b: {reply:?}"
    );
    assert_eq!(reply.transfers.len(), 0, "no swap or actuation");
}

#[path = "support/shared_roster_slots.rs"]
mod shared_roster_slots;

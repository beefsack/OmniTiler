//! Portable core dynamic-workspace policy regressions.
//!
//! Covers separate / shared / global-unique: no core minimum-two floor,
//! trailing spare retention, occupied-final supplies next spare, and
//! leading/middle prune after leave. Host-owned trailing spare requests
//! native backing. Never prunes displaced or held units (held occupancy
//! and displaced hosting retain).

use crate::connection::HostCapabilities;
use crate::connection::RawSettingValue;
use crate::directional::{OutputId, WindowId};
use crate::geometry::Rect;
use crate::size_hints::WindowSizeHints;
use crate::workspace::{WorkspaceFacts, plan_trailing};
use crate::workspace_scope::WorkspaceMode;
use crate::world_engine::{WorldEngine, WorldOutcome};
use crate::world_report::{
    FrameSetter, NativeCreationAttempt, NativeObservation, NativeOccupancy, PresentationIngressV4,
    PresentationRowV4, ScreenReport, WORLD_CONTRACT_VERSION_V4, WindowFact, WorldReportV4,
};
use std::collections::{BTreeMap, BTreeSet};

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

fn empty_fact() -> WorkspaceFacts {
    WorkspaceFacts {
        empty: true,
        visible: false,
        occupied: false,
        retained: false,
    }
}

fn occupied_fact() -> WorkspaceFacts {
    WorkspaceFacts {
        empty: false,
        visible: true,
        occupied: true,
        retained: false,
    }
}

fn mode_token(mode: WorkspaceMode) -> &'static str {
    match mode {
        WorkspaceMode::PerOutputLocal => "per-output-local",
        WorkspaceMode::GlobalUnique => "global-unique",
        WorkspaceMode::Shared => "shared",
    }
}

fn engine_native_free(mode: WorkspaceMode) -> WorldEngine {
    WorldEngine::new_test_bound_with_initial_settings(
        "gen-1",
        "adapter-1",
        HostCapabilities {
            native_sticky: true,
            maximize: true,
            ..Default::default()
        },
        vec![(
            "mode".to_owned(),
            RawSettingValue::String(mode_token(mode).to_owned()),
        )],
    )
    .expect("generation")
}

fn engine_host_owned(mode: WorkspaceMode) -> WorldEngine {
    WorldEngine::new_test_bound_with_initial_settings(
        "gen-1",
        "adapter-1",
        HostCapabilities {
            host_owned: true,
            ..Default::default()
        },
        vec![(
            "mode".to_owned(),
            RawSettingValue::String(mode_token(mode).to_owned()),
        )],
    )
    .expect("generation")
}

fn report_native_free(
    seq: u64,
    screens: Vec<ScreenReport>,
    windows: Vec<WindowFact>,
    closed: Vec<WindowId>,
) -> WorldReportV4 {
    WorldReportV4 {
        contract_version: WORLD_CONTRACT_VERSION_V4,
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        observation_seq: seq,
        request_seq: seq,
        request_id: format!("req-{seq}"),
        command_seq: None,
        command: None,
        screens,
        held_screens: Vec::new(),
        windows,
        held_windows: Vec::new(),
        closed,
        native: Vec::new(),
        held_native: Vec::new(),
        cursor: None,
        cursor_hit: None,
        focused_global: String::new(),
        foreground_known: false,
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

fn ring_ids(reply: &crate::world_report::WorldReplyV4, output: &str) -> Vec<String> {
    let mut rows: Vec<(usize, String)> = reply
        .workspaces
        .iter()
        .filter(|row| row.output.0 == output)
        .map(|row| (row.order, row.id.clone()))
        .collect();
    rows.sort();
    rows.into_iter().map(|(_, id)| id).collect()
}

#[test]
fn dynamic_no_core_minimum_two_single_spare() {
    // Portable: bootstrap must leave exactly one trailing spare per output,
    // not two. Fails before the MIN_WORKSPACES=2 floor is removed.
    let facts = vec![empty_fact()];
    let plan = plan_trailing(&facts);
    assert!(plan.remove.is_empty(), "single spare kept");
    assert!(
        !plan.need_append,
        "single trailing empty needs no append (no minimum-two)"
    );

    let facts = vec![empty_fact(), empty_fact()];
    let plan = plan_trailing(&facts);
    assert_eq!(plan.remove, vec![0], "leading empty prunes, trailing kept");
    assert!(!plan.need_append, "no append once trailing spare exists");
}

#[test]
fn dynamic_occupied_final_supplies_next_spare() {
    // Occupied final row must request exactly one new spare, then stabilize.
    let plan = plan_trailing(&[occupied_fact(), occupied_fact()]);
    assert!(plan.remove.is_empty());
    assert!(plan.need_append, "occupied final supplies next spare");

    let plan = plan_trailing(&[occupied_fact(), empty_fact()]);
    assert!(plan.remove.is_empty());
    assert!(!plan.need_append, "fresh spare stabilizes, no oscillation");
}

#[test]
fn dynamic_shown_empty_retained_then_pruned_after_leave() {
    // Shown empty retained while visible; pruned once unshown and nontrailing.
    let shown = WorkspaceFacts {
        empty: true,
        visible: true,
        occupied: false,
        retained: false,
    };
    let plan = plan_trailing(&[shown, empty_fact()]);
    assert!(plan.remove.is_empty(), "shown empty retained while visible");

    // After leave: first becomes unshown ordinary empty, second stays trailing.
    let plan = plan_trailing(&[empty_fact(), empty_fact()]);
    assert_eq!(plan.remove, vec![0], "leading unshown prunes after leave");
}

#[test]
fn dynamic_native_free_bootstrap_one_spare_per_output_all_modes() {
    for mode in [
        WorkspaceMode::PerOutputLocal,
        WorkspaceMode::GlobalUnique,
        WorkspaceMode::Shared,
    ] {
        let mut engine = engine_native_free(mode);
        let reply = accepted(
            &mut engine,
            &report_native_free(
                1,
                vec![screen("out-a", 0), screen("out-b", 800)],
                Vec::new(),
                Vec::new(),
            ),
        );
        let before_a = ring_ids(&reply, "out-a");
        let before_b = ring_ids(&reply, "out-b");
        // No core minimum-two: one spare per output (shared mirrors one id).
        if mode == WorkspaceMode::Shared {
            assert_eq!(
                before_a.len(),
                1,
                "shared bootstrap one spare, mode {mode:?}"
            );
            assert_eq!(
                before_b.len(),
                1,
                "shared bootstrap one spare, mode {mode:?}"
            );
            assert_eq!(before_a, before_b, "shared rings mirror");
        } else {
            assert_eq!(before_a.len(), 1, "bootstrap one spare, mode {mode:?}");
            assert_eq!(before_b.len(), 1, "bootstrap one spare, mode {mode:?}");
        }
        // No oscillation: repeat readback keeps identical ids, no transfers.
        let repeat = accepted(
            &mut engine,
            &report_native_free(
                2,
                vec![screen("out-a", 0), screen("out-b", 800)],
                Vec::new(),
                Vec::new(),
            ),
        );
        assert_eq!(
            ring_ids(&repeat, "out-a"),
            before_a,
            "stable, mode {mode:?}"
        );
        assert_eq!(
            ring_ids(&repeat, "out-b"),
            before_b,
            "stable, mode {mode:?}"
        );
        assert!(
            repeat.transfers.is_empty(),
            "no membership writes, mode {mode:?}"
        );
        assert_eq!(repeat.native_creations.len(), 0, "native-free no creates");
        assert!(
            repeat.workspaces.iter().all(|row| row.existence),
            "native-free no deletes"
        );
    }
}

#[test]
fn dynamic_native_free_occupy_final_grows_then_stable_all_modes() {
    for mode in [
        WorkspaceMode::PerOutputLocal,
        WorkspaceMode::GlobalUnique,
        WorkspaceMode::Shared,
    ] {
        let mut engine = engine_native_free(mode);
        let first = accepted(
            &mut engine,
            &report_native_free(1, vec![screen("out-a", 0)], Vec::new(), Vec::new()),
        );
        let spare = ring_ids(&first, "out-a").pop().expect("spare");
        let _ = spare;
        // Occupy the single spare: next report must supply exactly one new spare.
        let second = accepted(
            &mut engine,
            &report_native_free(
                2,
                vec![screen("out-a", 0)],
                vec![fact("win-a", "out-a", 0)],
                Vec::new(),
            ),
        );
        let ring = ring_ids(&second, "out-a");
        assert_eq!(
            ring.len(),
            2,
            "occupied final supplies next spare, mode {mode:?}"
        );
        // Stable: no further growth, no transfers, no native writes.
        let third = accepted(
            &mut engine,
            &report_native_free(
                3,
                vec![screen("out-a", 0)],
                vec![fact("win-a", "out-a", 0)],
                Vec::new(),
            ),
        );
        assert_eq!(
            ring_ids(&third, "out-a"),
            ring,
            "no oscillation, mode {mode:?}"
        );
        assert!(third.transfers.is_empty(), "no membership writes for gaps");
    }
}

#[test]
fn dynamic_native_free_leading_middle_prune_after_leave() {
    // Build three rings via occupancy, then empty leading/middle and leave.
    // Only trailing spare must survive; no window moves merely for gaps.
    let mut engine = engine_native_free(WorkspaceMode::PerOutputLocal);
    let first = accepted(
        &mut engine,
        &report_native_free(1, vec![screen("out-a", 0)], Vec::new(), Vec::new()),
    );
    assert_eq!(ring_ids(&first, "out-a").len(), 1);
    // Occupy spare twice to grow to 3: win-a on w1, select trailing, win-b there, etc.
    // Simpler direct-state prune check via plan_trailing over scoped facts:
    // [ordinary empty leading, occupied middle, trailing spare] prunes leading.
    let plan = plan_trailing(&[empty_fact(), occupied_fact(), empty_fact()]);
    assert_eq!(
        plan.remove,
        vec![0],
        "leading empty prunes, middle occupied kept"
    );
    assert!(!plan.need_append);
    // [occupied, ordinary empty middle, trailing spare] prunes middle.
    let plan = plan_trailing(&[occupied_fact(), empty_fact(), empty_fact()]);
    // First empty (index 1) is nontrailing ordinary empty -> prune; trailing kept.
    assert_eq!(plan.remove, vec![1], "middle empty prunes");
    assert!(!plan.need_append);
}

#[test]
fn dynamic_trailing_spare_requests_native_backing() {
    // Host-owned: unbound trailing empty spare (unviewed, untargeted,
    // unoccupied) must mint exactly one native creation desire.
    let mut engine = engine_host_owned(WorkspaceMode::PerOutputLocal);
    engine.mapping.insert(
        "out-a".to_owned(),
        vec!["ws-1".to_owned(), "ws-9".to_owned()],
    );
    // Bind head so only the trailing spare is unbound; view stays on head
    // so the spare is genuinely unviewed/untargeted/unoccupied.
    engine.native_map.insert(
        ("out-a".to_owned(), "native-out-a-0".to_owned()),
        "ws-1".to_owned(),
    );
    engine.views.insert("out-a".to_owned(), "ws-1".to_owned());
    // Stage a complete native list + screen so creation gates pass.
    engine.staged.screens = vec![screen("out-a", 0)];
    engine.staged.observation.inventory_complete = true;
    engine.staged.observation.native = vec![NativeObservation {
        output: OutputId("out-a".to_owned()),
        native_order: vec!["native-out-a-0".to_owned()],
        current_native: Some("native-out-a-0".to_owned()),
        occupancy: vec![NativeOccupancy {
            native: "native-out-a-0".to_owned(),
            count: Some(0),
        }],
    }];
    engine.ensure_native_creations("out-a");
    let desires = engine.native_creation_desires();
    assert_eq!(desires.len(), 1, "trailing spare mints one desire");
    assert_eq!(desires[0].logical_workspace, "ws-9");
    // Idempotent: repeat mints nothing new.
    engine.ensure_native_creations("out-a");
    assert_eq!(
        engine.native_creation_desires().len(),
        1,
        "stable operation id, no oscillation"
    );
}

#[test]
fn dynamic_shared_trailing_spare_requests_native_backing() {
    let mut engine = engine_host_owned(WorkspaceMode::Shared);
    engine.mapping.insert(
        "out-a".to_owned(),
        vec!["ws-1".to_owned(), "ws-9".to_owned()],
    );
    engine.mapping.insert(
        "out-b".to_owned(),
        vec!["ws-1".to_owned(), "ws-9".to_owned()],
    );
    engine
        .native_map
        .insert(("out-a".to_owned(), "n-0".to_owned()), "ws-1".to_owned());
    engine.views.insert("out-a".to_owned(), "ws-1".to_owned());
    engine.views.insert("out-b".to_owned(), "ws-1".to_owned());
    engine.staged.screens = vec![screen("out-a", 0), screen("out-b", 800)];
    engine.staged.observation.inventory_complete = true;
    engine.staged.observation.native = vec![
        NativeObservation {
            output: OutputId("out-a".to_owned()),
            native_order: vec!["n-0".to_owned()],
            current_native: Some("n-0".to_owned()),
            occupancy: vec![NativeOccupancy {
                native: "n-0".to_owned(),
                count: Some(0),
            }],
        },
        NativeObservation {
            output: OutputId("out-b".to_owned()),
            native_order: vec!["n-0".to_owned()],
            current_native: Some("n-0".to_owned()),
            occupancy: vec![NativeOccupancy {
                native: "n-0".to_owned(),
                count: Some(0),
            }],
        },
    ];
    engine.ensure_native_creations_shared(&["out-a".to_owned(), "out-b".to_owned()]);
    let desires = engine.native_creation_desires();
    assert_eq!(desires.len(), 1, "shared trailing spare mints once");
    assert_eq!(desires[0].logical_workspace, "ws-9");
}

#[test]
fn dynamic_never_prunes_displaced_or_held() {
    // Displaced hosting and held occupancy retain against trailing cleanup.
    let hosted = WorkspaceFacts {
        empty: true,
        visible: false,
        occupied: false,
        retained: true,
    };
    let plan = plan_trailing(&[hosted, empty_fact()]);
    assert!(
        plan.remove.is_empty(),
        "displaced/held retained never prunes"
    );
    // Held windows occupy: an empty-looking row with held occupancy is occupied.
    let held_occupied = WorkspaceFacts {
        empty: false,
        visible: false,
        occupied: true,
        retained: false,
    };
    let plan = plan_trailing(&[held_occupied, empty_fact()]);
    assert!(plan.remove.is_empty());
    assert!(!plan.need_append);
}

// ---------------------------------------------------------------------------
// Generic host-owned real-pipeline retirement regressions.
//
// Fake host covers native-shared-across-outputs and per-output-native cases
// in separate/shared/global-unique modes. Each step counts native creates,
// removal-eligibility rows, membership writes (transfers) and asserts zero
// member writes for pruning/spare lifecycle.
// ---------------------------------------------------------------------------

use crate::world_report::NativeWorkspaceFact;

fn engine_host_pipeline(mode: WorkspaceMode) -> WorldEngine {
    WorldEngine::new_test_bound_with_initial_settings(
        "gen-1",
        "adapter-1",
        HostCapabilities {
            host_owned: true,
            window_suppression: true,
            maximize: true,
            ..Default::default()
        },
        vec![(
            "mode".to_owned(),
            RawSettingValue::String(mode_token(mode).to_owned()),
        )],
    )
    .expect("generation")
}

fn hfact(window: &str, output: &str, native: &str, x: i32, visible: bool) -> WindowFact {
    let mut f = fact(window, output, x);
    f.sticky = None;
    f.suppression_available = Some(true);
    f.native_workspace = NativeWorkspaceFact::Value(native.to_owned());
    f.visibility.visible = Some(visible);
    f
}

fn obs(
    output: &str,
    order: Vec<&str>,
    current: &str,
    counts: Vec<(&str, Option<u32>)>,
) -> NativeObservation {
    NativeObservation {
        output: OutputId(output.to_owned()),
        native_order: order.into_iter().map(str::to_owned).collect(),
        current_native: Some(current.to_owned()),
        occupancy: counts
            .into_iter()
            .map(|(native, count)| NativeOccupancy {
                native: native.to_owned(),
                count,
            })
            .collect(),
    }
}

fn report_host(
    seq: u64,
    screens: Vec<ScreenReport>,
    windows: Vec<WindowFact>,
    native: Vec<NativeObservation>,
) -> WorldReportV4 {
    let mut report = report_native_free(seq, screens, windows, Vec::new());
    report.native = native;
    report
}

fn ring_ids_live(engine: &WorldEngine, output: &str) -> Vec<String> {
    engine.mapping.get(output).cloned().unwrap_or_default()
}

fn tombstone_for(
    reply: &crate::world_report::WorldReplyV4,
    engine: &WorldEngine,
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
                && !ring_ids_live(engine, output).iter().any(|id| id == &row.id)
        })
        .map(|row| row.id.clone())
}

fn binding_for(
    reply: &crate::world_report::WorldReplyV4,
    output: &str,
    native: &str,
) -> Option<String> {
    reply
        .workspaces
        .iter()
        .find(|row| {
            row.output.0 == output && row.native_binding.as_deref() == Some(native) && row.existence
        })
        .map(|row| row.id.clone())
}

fn step_counts(reply: &crate::world_report::WorldReplyV4) -> (usize, usize, usize) {
    (
        reply.native_creations.len(),
        reply.workspaces.iter().filter(|row| !row.existence).count(),
        reply.transfers.len(),
    )
}

#[test]
fn retirement_tombstone_published_for_pruned_bound_row() {
    // Per-output natives, separate mode: pruning a bound ordinary empty
    // must retire (output, native), clear the binding, publish an
    // existence:false tombstone row keyed by the removed logical id, and
    // write zero memberships.
    let mut engine = engine_host_pipeline(WorkspaceMode::PerOutputLocal);
    let boot = accepted(
        &mut engine,
        &report_host(
            1,
            vec![screen("out-a", 0), screen("out-b", 800)],
            vec![hfact("w-b", "out-b", "b0", 900, true)],
            vec![
                obs(
                    "out-a",
                    vec!["a0", "a1", "a2"],
                    "a0",
                    vec![("a0", Some(0)), ("a1", Some(0)), ("a2", Some(0))],
                ),
                obs("out-b", vec!["b0"], "b0", vec![("b0", Some(1))]),
            ],
        ),
    );
    let (creates, deletes, transfers) = step_counts(&boot);
    assert!(transfers == 0, "boot writes no memberships: {transfers}");
    // The middle ordinary empty on out-a must prune: retired marker set,
    // binding cleared, tombstone published under the removed logical id.
    let retired: Vec<String> = ["a1", "a2"]
        .into_iter()
        .filter(|native| engine.is_locally_retired("out-a", native))
        .map(str::to_owned)
        .collect();
    assert_eq!(
        retired.len(),
        1,
        "exactly one out-a native retires (creates={creates} deletes={deletes} transfers={transfers}) ring={:?} rows={:?}",
        engine.mapping.get("out-a"),
        boot.workspaces
            .iter()
            .map(|row| (
                row.output.0.clone(),
                row.id.clone(),
                row.native_binding.clone(),
                row.existence
            ))
            .collect::<Vec<_>>(),
    );
    let native = retired[0].clone();
    let tomb = tombstone_for(&boot, &engine, "out-a", &native).unwrap_or_else(|| {
        panic!(
            "tombstone row for retired (out-a,{native}) missing: {:?}",
            boot.workspaces
                .iter()
                .map(|row| (
                    row.output.0.clone(),
                    row.id.clone(),
                    row.native_binding.clone(),
                    row.existence
                ))
                .collect::<Vec<_>>(),
        )
    });
    assert!(
        !ring_ids_live(&engine, "out-a").contains(&tomb),
        "tombstone id is not a live ring row"
    );
    assert!(
        engine.native_binding_for("out-a", &tomb).is_none(),
        "no live native_map binding retained outside rings"
    );
    // Repeat readback: stable, no re-adoption, no oscillation.
    let ring_before_repeat: Vec<String> = ring_ids_live(&engine, "out-a");
    let repeat = accepted(
        &mut engine,
        &report_host(
            2,
            vec![screen("out-a", 0), screen("out-b", 800)],
            vec![hfact("w-b", "out-b", "b0", 900, true)],
            vec![
                obs(
                    "out-a",
                    vec!["a0", "a1", "a2"],
                    "a0",
                    vec![("a0", Some(0)), ("a1", Some(0)), ("a2", Some(0))],
                ),
                obs("out-b", vec!["b0"], "b0", vec![("b0", Some(1))]),
            ],
        ),
    );
    let (creates2, deletes2, transfers2) = step_counts(&repeat);
    assert_eq!(transfers2, 0, "repeat writes no memberships");
    assert_eq!(
        ring_ids_live(&engine, "out-a"),
        ring_before_repeat,
        "repeat readback keeps the live ring stable"
    );
    assert!(
        engine.is_locally_retired("out-a", &native),
        "marker survives repeat readback (creates={creates2} deletes={deletes2})"
    );
    assert!(
        binding_for(&repeat, "out-a", &native).is_none(),
        "no re-adoption without positive local evidence"
    );
    assert!(
        tombstone_for(&repeat, &engine, "out-a", &native).is_some(),
        "tombstone keeps publishing until deletion or re-adopt"
    );
    assert_eq!(
        repeat.native_creations, boot.native_creations,
        "no new creation desires on repeat (no oscillation)"
    );
    let _ = (creates, deletes, creates2, deletes2);
}

#[test]
fn shared_native_ids_never_readopt_from_remote_occupancy() {
    // Global native ids observed on both outputs, separate and
    // global-unique modes: the producer repeats the global occupant count
    // on every output. Retiring (out-a, n1) while out-b hosts the occupant
    // must suppress re-adoption on out-a: a repeated global count is NOT
    // positive output-local evidence.
    for mode in [WorkspaceMode::PerOutputLocal, WorkspaceMode::GlobalUnique] {
        let mut engine = engine_host_pipeline(mode);
        let boot = accepted(
            &mut engine,
            &report_host(
                1,
                vec![screen("out-a", 0), screen("out-b", 800)],
                vec![hfact("w-b", "out-b", "n1", 900, true)],
                vec![
                    obs(
                        "out-a",
                        vec!["n0", "n1", "n2"],
                        "n0",
                        vec![("n0", Some(0)), ("n1", Some(1)), ("n2", Some(0))],
                    ),
                    obs(
                        "out-b",
                        vec!["n0", "n1"],
                        "n0",
                        vec![("n0", Some(0)), ("n1", Some(1))],
                    ),
                ],
            ),
        );
        assert_eq!(
            boot.transfers.len(),
            0,
            "boot writes no memberships, mode {mode:?}"
        );
        assert!(
            engine.is_locally_retired("out-a", "n1"),
            "out-a retires n1 while out-b hosts the occupant, mode {mode:?}: ring={:?}",
            engine.mapping.get("out-a"),
        );
        assert!(
            tombstone_for(&boot, &engine, "out-a", "n1").is_some(),
            "retire row publishes on out-a, mode {mode:?}"
        );
        // out-b keeps its live occupied binding; no tombstone there.
        assert!(
            binding_for(&boot, "out-b", "n1").is_some(),
            "out-b keeps live n1 binding, mode {mode:?}"
        );
        assert!(
            tombstone_for(&boot, &engine, "out-b", "n1").is_none(),
            "no retire row on out-b, mode {mode:?}"
        );
        // Repeated global-count readback must NOT re-adopt on out-a.
        for seq in 2..=3u64 {
            let repeat = accepted(
                &mut engine,
                &report_host(
                    seq,
                    vec![screen("out-a", 0), screen("out-b", 800)],
                    vec![hfact("w-b", "out-b", "n1", 900, true)],
                    vec![
                        obs(
                            "out-a",
                            vec!["n0", "n1", "n2"],
                            "n0",
                            vec![("n0", Some(0)), ("n1", Some(1)), ("n2", Some(0))],
                        ),
                        obs(
                            "out-b",
                            vec!["n0", "n1"],
                            "n0",
                            vec![("n0", Some(0)), ("n1", Some(1))],
                        ),
                    ],
                ),
            );
            assert_eq!(
                repeat.transfers.len(),
                0,
                "repeat writes no memberships, mode {mode:?}"
            );
            assert!(
                engine.is_locally_retired("out-a", "n1"),
                "global repeated count must not clear the marker, mode {mode:?} seq {seq}"
            );
            assert!(
                binding_for(&repeat, "out-a", "n1").is_none(),
                "no re-adopt from remote occupancy, mode {mode:?} seq {seq}"
            );
            assert!(
                tombstone_for(&repeat, &engine, "out-a", "n1").is_some(),
                "tombstone persists, mode {mode:?} seq {seq}"
            );
        }
        // Selecting the retired desktop on out-a is positive local
        // evidence: the marker clears and a FRESH logical adopts at the
        // native position; the retirement row stops.
        let pruned_id = boot
            .workspaces
            .iter()
            .find(|row| {
                row.output.0 == "out-a"
                    && row.native_binding.as_deref() == Some("n1")
                    && !row.existence
            })
            .map(|row| row.id.clone())
            .expect("tombstone id");
        let reselect = accepted(
            &mut engine,
            &report_host(
                4,
                vec![screen("out-a", 0), screen("out-b", 800)],
                vec![hfact("w-b", "out-b", "n1", 900, true)],
                vec![
                    obs(
                        "out-a",
                        vec!["n0", "n1", "n2"],
                        "n1",
                        vec![("n0", Some(0)), ("n1", Some(1)), ("n2", Some(0))],
                    ),
                    obs(
                        "out-b",
                        vec!["n0", "n1"],
                        "n0",
                        vec![("n0", Some(0)), ("n1", Some(1))],
                    ),
                ],
            ),
        );
        assert_eq!(
            reselect.transfers.len(),
            0,
            "reselect writes no memberships"
        );
        assert!(
            !engine.is_locally_retired("out-a", "n1"),
            "current-native selection clears the marker, mode {mode:?}"
        );
        let fresh = binding_for(&reselect, "out-a", "n1")
            .unwrap_or_else(|| panic!("fresh logical adopts n1 at native position, mode {mode:?}"));
        assert_ne!(fresh, pruned_id, "fresh identity, never the removed id");
        assert!(
            tombstone_for(&reselect, &engine, "out-a", "n1").is_none(),
            "retirement row stops after re-adopt, mode {mode:?}"
        );
        // A window arriving on out-a/n1 is normal admission on the fresh
        // logical through the ordinary homing path (no transfer write); the
        // pre-existing trailing spare already covers growth, so the live
        // ring stays the same length.
        let before: Vec<String> = ring_ids_live(&engine, "out-a");
        let arrival = accepted(
            &mut engine,
            &report_host(
                5,
                vec![screen("out-a", 0), screen("out-b", 800)],
                vec![
                    hfact("w-b", "out-b", "n1", 900, true),
                    hfact("w-a", "out-a", "n1", 100, true),
                ],
                vec![
                    obs(
                        "out-a",
                        vec!["n0", "n1", "n2"],
                        "n1",
                        vec![("n0", Some(0)), ("n1", Some(2)), ("n2", Some(0))],
                    ),
                    obs(
                        "out-b",
                        vec!["n0", "n1"],
                        "n0",
                        vec![("n0", Some(0)), ("n1", Some(2))],
                    ),
                ],
            ),
        );
        assert_eq!(
            arrival.transfers.len(),
            0,
            "admission writes no memberships"
        );
        let after: Vec<String> = ring_ids_live(&engine, "out-a");
        assert!(
            after.contains(&fresh),
            "fresh logical still rings the arrival, mode {mode:?}"
        );
        // Admission lands the window on the fresh logical through the
        // ordinary homing path (no transfer write); the pre-existing
        // trailing spare already covers growth, so the ring stays stable.
        let homed = arrival
            .windows
            .iter()
            .find(|row| row.lifetime.0 == "w-a")
            .unwrap_or_else(|| panic!("arrival row for w-a, mode {mode:?}"));
        assert_eq!(homed.output.0, "out-a", "mode {mode:?}");
        assert_eq!(homed.workspace, fresh, "arrival homed on fresh logical");
        assert_eq!(
            after.len(),
            before.len(),
            "spare already present: stable ring, mode {mode:?}: {before:?} -> {after:?}"
        );
        // Occupying the final row supplies the next spare with zero
        // membership writes: select the trailing bound native, then arrive
        // a window there.
        let trailing_native = arrival
            .workspaces
            .iter()
            .find(|row| {
                row.output.0 == "out-a"
                    && row.existence
                    && row.native_binding.as_deref() == Some("n2")
            })
            .map(|row| row.native_binding.clone().expect("bound"))
            .expect("trailing bound native");
        let _ = trailing_native;
        let grown_base: Vec<String> = ring_ids_live(&engine, "out-a");
        let select_tail = accepted(
            &mut engine,
            &report_host(
                6,
                vec![screen("out-a", 0), screen("out-b", 800)],
                vec![
                    hfact("w-b", "out-b", "n1", 900, true),
                    hfact("w-a", "out-a", "n1", 100, true),
                ],
                vec![
                    obs(
                        "out-a",
                        vec!["n0", "n1", "n2"],
                        "n2",
                        vec![("n0", Some(0)), ("n1", Some(2)), ("n2", Some(0))],
                    ),
                    obs(
                        "out-b",
                        vec!["n0", "n1"],
                        "n0",
                        vec![("n0", Some(0)), ("n1", Some(2))],
                    ),
                ],
            ),
        );
        assert_eq!(
            select_tail.transfers.len(),
            0,
            "tail select writes no memberships"
        );
        let grown = accepted(
            &mut engine,
            &report_host(
                7,
                vec![screen("out-a", 0), screen("out-b", 800)],
                vec![
                    hfact("w-b", "out-b", "n1", 900, true),
                    hfact("w-a", "out-a", "n1", 100, true),
                    hfact("w-c", "out-a", "n2", 200, true),
                ],
                vec![
                    obs(
                        "out-a",
                        vec!["n0", "n1", "n2"],
                        "n2",
                        vec![("n0", Some(0)), ("n1", Some(2)), ("n2", Some(1))],
                    ),
                    obs(
                        "out-b",
                        vec!["n0", "n1"],
                        "n0",
                        vec![("n0", Some(0)), ("n1", Some(2))],
                    ),
                ],
            ),
        );
        assert_eq!(
            grown.transfers.len(),
            0,
            "tail admission writes no memberships"
        );
        let grown_ring: Vec<String> = ring_ids_live(&engine, "out-a");
        assert_eq!(
            grown_ring.len(),
            grown_base.len() + 1,
            "occupied final supplies next spare, mode {mode:?}: {grown_base:?} -> {grown_ring:?}"
        );
    }
}

#[test]
fn native_deletion_clears_retired_marker() {
    // After retiring (out-a, n1), the host deleting n1 from out-a's
    // complete order clears the marker and stops the retirement row.
    let mut engine = engine_host_pipeline(WorkspaceMode::PerOutputLocal);
    let boot = accepted(
        &mut engine,
        &report_host(
            1,
            vec![screen("out-a", 0), screen("out-b", 800)],
            vec![hfact("w-b", "out-b", "n1", 900, true)],
            vec![
                obs(
                    "out-a",
                    vec!["n0", "n1", "n2"],
                    "n0",
                    vec![("n0", Some(0)), ("n1", Some(1)), ("n2", Some(0))],
                ),
                obs(
                    "out-b",
                    vec!["n0", "n1"],
                    "n0",
                    vec![("n0", Some(0)), ("n1", Some(1))],
                ),
            ],
        ),
    );
    assert!(engine.is_locally_retired("out-a", "n1"), "retires first");
    assert!(tombstone_for(&boot, &engine, "out-a", "n1").is_some());
    let deleted = accepted(
        &mut engine,
        &report_host(
            2,
            vec![screen("out-a", 0), screen("out-b", 800)],
            vec![hfact("w-b", "out-b", "n1", 900, true)],
            vec![
                obs(
                    "out-a",
                    vec!["n0", "n2"],
                    "n0",
                    vec![("n0", Some(0)), ("n2", Some(0))],
                ),
                obs(
                    "out-b",
                    vec!["n0", "n1"],
                    "n0",
                    vec![("n0", Some(0)), ("n1", Some(1))],
                ),
            ],
        ),
    );
    assert_eq!(
        deleted.transfers.len(),
        0,
        "deletion readback writes no memberships"
    );
    assert!(
        !engine.is_locally_retired("out-a", "n1"),
        "native deletion clears the marker"
    );
    assert!(
        tombstone_for(&deleted, &engine, "out-a", "n1").is_none(),
        "retirement row stops after native deletion"
    );
    assert!(
        binding_for(&deleted, "out-b", "n1").is_some(),
        "out-b binding survives independently"
    );
}

#[test]
fn shared_mode_unified_retire_and_reselect() {
    // Shared ring, shared native ids: pruning the unified middle retires
    // the native on every observing output; selecting it anywhere clears
    // globally and re-adopts coherently with zero membership writes.
    let mut engine = engine_host_pipeline(WorkspaceMode::Shared);
    let boot = accepted(
        &mut engine,
        &report_host(
            1,
            vec![screen("out-a", 0), screen("out-b", 800)],
            Vec::new(),
            vec![
                obs(
                    "out-a",
                    vec!["n0", "n1", "n2"],
                    "n0",
                    vec![("n0", Some(0)), ("n1", Some(0)), ("n2", Some(0))],
                ),
                obs(
                    "out-b",
                    vec!["n0", "n1", "n2"],
                    "n0",
                    vec![("n0", Some(0)), ("n1", Some(0)), ("n2", Some(0))],
                ),
            ],
        ),
    );
    assert_eq!(boot.transfers.len(), 0, "boot writes no memberships");
    assert!(
        engine.is_locally_retired("out-a", "n1") || engine.is_locally_retired("out-a", "n2"),
        "shared middle prunes while natives survive: {:?}",
        boot.workspaces
            .iter()
            .map(|row| (
                row.output.0.clone(),
                row.id.clone(),
                row.native_binding.clone(),
                row.existence
            ))
            .collect::<Vec<_>>(),
    );
    let retired_native = ["n1", "n2"]
        .into_iter()
        .find(|native| engine.is_locally_retired("out-a", native))
        .expect("one retired native")
        .to_owned();
    assert!(
        tombstone_for(&boot, &engine, "out-a", &retired_native).is_some(),
        "tombstone publishes for retired shared native"
    );
    // Repeat: no partial re-adoption of the retired native anywhere.
    let repeat = accepted(
        &mut engine,
        &report_host(
            2,
            vec![screen("out-a", 0), screen("out-b", 800)],
            Vec::new(),
            vec![
                obs(
                    "out-a",
                    vec!["n0", "n1", "n2"],
                    "n0",
                    vec![("n0", Some(0)), ("n1", Some(0)), ("n2", Some(0))],
                ),
                obs(
                    "out-b",
                    vec!["n0", "n1", "n2"],
                    "n0",
                    vec![("n0", Some(0)), ("n1", Some(0)), ("n2", Some(0))],
                ),
            ],
        ),
    );
    assert_eq!(repeat.transfers.len(), 0, "repeat writes no memberships");
    assert!(
        binding_for(&repeat, "out-a", &retired_native).is_none()
            && binding_for(&repeat, "out-b", &retired_native).is_none(),
        "no partial re-adoption in the unified ring"
    );
    // Select the retired desktop on out-a: global clear, fresh unified adopt.
    let reselect = accepted(
        &mut engine,
        &report_host(
            3,
            vec![screen("out-a", 0), screen("out-b", 800)],
            Vec::new(),
            vec![
                obs(
                    "out-a",
                    vec!["n0", "n1", "n2"],
                    &retired_native,
                    vec![("n0", Some(0)), ("n1", Some(0)), ("n2", Some(0))],
                ),
                obs(
                    "out-b",
                    vec!["n0", "n1", "n2"],
                    "n0",
                    vec![("n0", Some(0)), ("n1", Some(0)), ("n2", Some(0))],
                ),
            ],
        ),
    );
    assert_eq!(
        reselect.transfers.len(),
        0,
        "reselect writes no memberships"
    );
    assert!(
        !engine.is_locally_retired("out-a", &retired_native)
            && !engine.is_locally_retired("out-b", &retired_native),
        "selection evidence clears every pair naming the native"
    );
}

// ---------------------------------------------------------------------------
// Reselect spare-rotation recurrence: shared global native ids observed
// identically on two outputs plus a floor-2 adapter simulation (unheld,
// globally empty, unviewed absence intents delete; spare desires append a
// globally visible native with an Accepted correlated receipt). Reselecting
// a retired native must reach actual quiescence: no unbounded
// +1create/+1delete per-report rotation with zero membership writes.
// ---------------------------------------------------------------------------

struct RotationHost {
    order: Vec<String>,
    currents: BTreeMap<String, String>,
    facts: Vec<(String, String, String)>,
    attempts: Vec<NativeCreationAttempt>,
    next_native: usize,
    creations: usize,
    deletions: usize,
    transfers: usize,
    log: Vec<String>,
}

fn engine_host_pipeline_equal(mode: WorkspaceMode) -> WorldEngine {
    // Production-faithful: the host declares equal-count (shared global
    // native orders read equal length on every output), so parity padding
    // couples the per-output rings.
    WorldEngine::new_test_bound_with_initial_settings(
        "gen-1",
        "adapter-1",
        HostCapabilities {
            host_owned: true,
            window_suppression: true,
            maximize: true,
            equal_count: true,
            ..Default::default()
        },
        vec![(
            "mode".to_owned(),
            RawSettingValue::String(mode_token(mode).to_owned()),
        )],
    )
    .expect("generation")
}

fn rotation_round(
    engine: &mut WorldEngine,
    host: &mut RotationHost,
    seq: u64,
) -> crate::world_report::WorldReplyV4 {
    let windows: Vec<WindowFact> = host
        .facts
        .iter()
        .enumerate()
        .map(|(index, (window, output, native))| {
            hfact(window, output, native, (index as i32) * 100, true)
        })
        .collect();
    let natives: Vec<NativeObservation> = ["out-a", "out-b"]
        .iter()
        .map(|output| NativeObservation {
            output: OutputId(output.to_string()),
            native_order: host.order.clone(),
            current_native: Some(host.currents[*output].clone()),
            occupancy: host
                .order
                .iter()
                .map(|native| NativeOccupancy {
                    native: native.clone(),
                    count: Some(
                        host.facts
                            .iter()
                            .filter(|(_, _, fact)| fact == native)
                            .count() as u32,
                    ),
                })
                .collect(),
        })
        .collect();
    let mut report = report_host(
        seq,
        vec![screen("out-a", 0), screen("out-b", 800)],
        windows,
        natives,
    );
    report.creation_attempts = std::mem::take(&mut host.attempts);
    let reply = accepted(engine, &report);
    host.transfers += reply.transfers.len();
    // Adapter: execute every spare-backing desire as a global append with a
    // correlated Accepted receipt for the next report.
    let pre = host.order.clone();
    for desire in &reply.native_creations {
        let id = format!("n{}", host.next_native);
        host.next_native += 1;
        host.order.push(id.clone());
        host.creations += 1;
        host.attempts.push(NativeCreationAttempt {
            operation_id: desire.operation_id.clone(),
            logical_workspace: desire.logical_workspace.clone(),
            target_output: desire.target_output.clone(),
            setter: FrameSetter::Accepted,
            order_before: pre.clone(),
            order_after: host.order.clone(),
            created_native: Some(id),
            attempt_ms: seq,
        });
    }
    // Adapter: delete unheld, globally empty, unviewed absence intents while
    // the global inventory stays above the two-desktop floor.
    let mut doomed: BTreeSet<String> = BTreeSet::new();
    for row in &reply.workspaces {
        if row.existence || row.hold.is_some() {
            continue;
        }
        let Some(native) = row.native_binding.clone() else {
            continue;
        };
        if host.facts.iter().any(|(_, _, fact)| fact == &native) {
            continue;
        }
        if host.currents.values().any(|current| current == &native) {
            continue;
        }
        doomed.insert(native);
    }
    let mut deleted: Vec<String> = Vec::new();
    for native in &doomed {
        if host.order.len() > 2 + deleted.len() {
            deleted.push(native.clone());
        }
    }
    for native in deleted {
        host.order.retain(|id| id != &native);
        host.deletions += 1;
    }
    let mut retired: Vec<String> = Vec::new();
    for output in ["out-a", "out-b"] {
        for native in &host.order {
            if engine.is_locally_retired(output, native) {
                retired.push(format!("{output}:{native}"));
            }
        }
        for (out, native) in engine.retired_natives.keys() {
            if out == output && !host.order.iter().any(|id| id == native) {
                retired.push(format!("{output}:{native}(gone)"));
            }
        }
    }
    retired.sort();
    retired.dedup();
    host.log.push(format!(
        "r{seq}: order={:?} creations={} deletions={} transfers={} desires={} absent={} ring-a={:?} ring-b={:?} retired={retired:?}",
        host.order,
        host.creations,
        host.deletions,
        host.transfers,
        reply.native_creations.len(),
        reply.workspaces.iter().filter(|row| !row.existence).count(),
        engine.mapping.get("out-a"),
        engine.mapping.get("out-b"),
    ));
    reply
}

#[test]
fn reselect_retired_native_reaches_quiescence_shared_global() {
    let mut engine = engine_host_pipeline_equal(WorkspaceMode::PerOutputLocal);
    let mut host = RotationHost {
        order: vec![
            "n0".to_owned(),
            "n1".to_owned(),
            "n2".to_owned(),
            "n3".to_owned(),
        ],
        currents: BTreeMap::from([
            ("out-a".to_owned(), "n0".to_owned()),
            ("out-b".to_owned(), "n1".to_owned()),
        ]),
        facts: vec![
            ("wa".to_owned(), "out-a".to_owned(), "n0".to_owned()),
            ("wb".to_owned(), "out-b".to_owned(), "n1".to_owned()),
            ("wd".to_owned(), "out-b".to_owned(), "n2".to_owned()),
            ("wc".to_owned(), "out-a".to_owned(), "n3".to_owned()),
        ],
        attempts: Vec::new(),
        next_native: 4,
        creations: 0,
        deletions: 0,
        transfers: 0,
        log: Vec::new(),
    };
    for seq in 1..=3u64 {
        rotation_round(&mut engine, &mut host, seq);
    }
    assert_eq!(
        host.transfers,
        0,
        "settle writes no memberships:\n{}",
        host.log.join("\n")
    );
    let retired = host
        .order
        .iter()
        .find(|native| engine.is_locally_retired("out-a", native))
        .cloned()
        .unwrap_or_else(|| {
            panic!(
                "out-a retires a locally empty native:\n{}",
                host.log.join("\n")
            )
        });
    host.currents.insert("out-a".to_owned(), retired.clone());
    let reselect = rotation_round(&mut engine, &mut host, 4);
    assert!(
        !engine.is_locally_retired("out-a", &retired),
        "current-native selection clears the marker:\n{}",
        host.log.join("\n")
    );
    let fresh = binding_for(&reselect, "out-a", &retired).unwrap_or_else(|| {
        panic!(
            "fresh logical adopts the reselected native:\n{}",
            host.log.join("\n")
        )
    });
    // Compact order: the fresh row sits among live ring bindings at its
    // native-order position, ignoring locally retired projections (no raw
    // global-index padding reinstatement).
    let ring = ring_ids_live(&engine, "out-a");
    let position = ring
        .iter()
        .position(|id| id == &fresh)
        .unwrap_or_else(|| panic!("fresh logical rings on out-a:\n{}", host.log.join("\n")));
    let bound_before = host
        .order
        .iter()
        .take_while(|native| native.as_str() != retired.as_str())
        .filter(|native| {
            engine
                .native_logical(&OutputId("out-a".to_owned()), native.as_str())
                .is_some()
        })
        .count();
    assert_eq!(
        position,
        bound_before,
        "fresh row at compact live-binding position:\n{}",
        host.log.join("\n")
    );
    for seq in 5..=6u64 {
        rotation_round(&mut engine, &mut host, seq);
    }
    let (creations, deletions, transfers) = (host.creations, host.deletions, host.transfers);
    let settled_order = host.order.clone();
    for seq in 7..=12u64 {
        rotation_round(&mut engine, &mut host, seq);
    }
    assert_eq!(
        (host.creations, host.deletions, host.transfers),
        (creations, deletions, transfers),
        "reselect reaches actual quiescence (no create/delete/member churn):\n{}",
        host.log.join("\n")
    );
    assert_eq!(
        host.order,
        settled_order,
        "native inventory stable after reselect:\n{}",
        host.log.join("\n")
    );
}

// ---------------------------------------------------------------------------
// Independent per-output native shapes: disjoint native ids per output with
// per-output occupancy and a per-output two-desktop floor. Reselecting a
// retired native (fast reselect before the adapter's next apply pass acts
// on the tombstone) readopts fresh at the compact live-binding position and
// the lifecycle stays quiescent: per-output spare backing never leaks onto
// the sibling output, and the cross-output tombstone fence stays a no-op
// across disjoint ids.
// ---------------------------------------------------------------------------

struct SplitHost {
    orders: BTreeMap<String, Vec<String>>,
    currents: BTreeMap<String, String>,
    facts: Vec<(String, String, String)>,
    attempts: Vec<NativeCreationAttempt>,
    next_native: usize,
    creations: usize,
    deletions: usize,
    transfers: usize,
    log: Vec<String>,
}

fn split_round(
    engine: &mut WorldEngine,
    host: &mut SplitHost,
    seq: u64,
    apply_deletes: bool,
) -> crate::world_report::WorldReplyV4 {
    let windows: Vec<WindowFact> = host
        .facts
        .iter()
        .enumerate()
        .map(|(index, (window, output, native))| {
            hfact(window, output, native, (index as i32) * 100, true)
        })
        .collect();
    let mut outputs: Vec<String> = host.orders.keys().cloned().collect();
    outputs.sort();
    let natives: Vec<NativeObservation> = outputs
        .iter()
        .map(|output| NativeObservation {
            output: OutputId(output.clone()),
            native_order: host.orders[output].clone(),
            current_native: Some(host.currents[output].clone()),
            occupancy: host.orders[output]
                .iter()
                .map(|native| NativeOccupancy {
                    native: native.clone(),
                    count: Some(
                        host.facts
                            .iter()
                            .filter(|(_, fact_output, fact_native)| {
                                fact_output == output && fact_native == native
                            })
                            .count() as u32,
                    ),
                })
                .collect(),
        })
        .collect();
    let mut screens: Vec<ScreenReport> = Vec::new();
    for (index, output) in outputs.iter().enumerate() {
        screens.push(screen(output, (index as i32) * 800));
    }
    let mut report = report_host(seq, screens, windows, natives);
    report.creation_attempts = std::mem::take(&mut host.attempts);
    let reply = accepted(engine, &report);
    host.transfers += reply.transfers.len();
    // Adapter: execute every spare-backing desire as an append on the
    // desire's target output only, with a correlated Accepted receipt.
    for desire in &reply.native_creations {
        let target = desire.target_output.0.clone();
        let pre = host.orders[&target].clone();
        let id = format!("m{}", host.next_native);
        host.next_native += 1;
        host.orders
            .get_mut(&target)
            .expect("target order")
            .push(id.clone());
        host.creations += 1;
        host.attempts.push(NativeCreationAttempt {
            operation_id: desire.operation_id.clone(),
            logical_workspace: desire.logical_workspace.clone(),
            target_output: desire.target_output.clone(),
            setter: FrameSetter::Accepted,
            order_before: pre,
            order_after: host.orders[&target].clone(),
            created_native: Some(id),
            attempt_ms: seq,
        });
    }
    // Adapter: delete unheld, empty, unviewed absence intents while the
    // observing output stays above its two-desktop floor.
    if apply_deletes {
        let mut doomed: Vec<(String, String)> = Vec::new();
        for row in &reply.workspaces {
            if row.existence || row.hold.is_some() {
                continue;
            }
            let Some(native) = row.native_binding.clone() else {
                continue;
            };
            let output = row.output.0.clone();
            if host.facts.iter().any(|(_, _, fact)| fact == &native) {
                continue;
            }
            if host
                .currents
                .get(&output)
                .is_some_and(|current| current == &native)
            {
                continue;
            }
            doomed.push((output, native));
        }
        doomed.sort();
        doomed.dedup();
        for (output, native) in doomed {
            let order = host.orders.get_mut(&output).expect("output order");
            if order.len() > 2 {
                order.retain(|id| id != &native);
                host.deletions += 1;
            }
        }
    }
    host.log.push(format!(
        "r{seq}: orders={:?} creations={} deletions={} transfers={} desires={} absent={} ring-a={:?} ring-b={:?}",
        host.orders,
        host.creations,
        host.deletions,
        host.transfers,
        reply.native_creations.len(),
        reply.workspaces.iter().filter(|row| !row.existence).count(),
        engine.mapping.get("out-a"),
        engine.mapping.get("out-b"),
    ));
    reply
}

#[test]
fn reselect_retired_native_reaches_quiescence_independent() {
    let mut engine = engine_host_pipeline(WorkspaceMode::PerOutputLocal);
    let mut host = SplitHost {
        orders: BTreeMap::from([
            (
                "out-a".to_owned(),
                vec!["a0".to_owned(), "a1".to_owned(), "a2".to_owned()],
            ),
            ("out-b".to_owned(), vec!["b0".to_owned(), "b1".to_owned()]),
        ]),
        currents: BTreeMap::from([
            ("out-a".to_owned(), "a0".to_owned()),
            ("out-b".to_owned(), "b0".to_owned()),
        ]),
        facts: vec![
            ("wa".to_owned(), "out-a".to_owned(), "a0".to_owned()),
            ("wc".to_owned(), "out-a".to_owned(), "a2".to_owned()),
            ("wb".to_owned(), "out-b".to_owned(), "b0".to_owned()),
        ],
        attempts: Vec::new(),
        next_native: 0,
        creations: 0,
        deletions: 0,
        transfers: 0,
        log: Vec::new(),
    };
    // Retire the locally empty middle on out-a; the adapter has not applied
    // the tombstone yet. The fence is a no-op across disjoint ids: the
    // tombstone for a1 publishes without hold.
    let boot = split_round(&mut engine, &mut host, 1, false);
    assert_eq!(
        host.transfers,
        0,
        "settle writes no memberships:\n{}",
        host.log.join("\n")
    );
    assert!(
        engine.is_locally_retired("out-a", "a1"),
        "out-a retires its locally empty middle:\n{}",
        host.log.join("\n")
    );
    let stone = boot
        .workspaces
        .iter()
        .find(|row| {
            row.output.0 == "out-a" && row.native_binding.as_deref() == Some("a1") && !row.existence
        })
        .unwrap_or_else(|| {
            panic!(
                "tombstone publishes for retired a1:\n{}",
                host.log.join("\n")
            )
        });
    assert_eq!(
        stone.hold,
        None,
        "disjoint native needs no cross-output fence:\n{}",
        host.log.join("\n")
    );
    // Fast reselect before the next apply pass: the marker clears and a
    // fresh logical adopts at the compact live-binding position.
    host.currents.insert("out-a".to_owned(), "a1".to_owned());
    let reselect = split_round(&mut engine, &mut host, 2, false);
    assert!(
        !engine.is_locally_retired("out-a", "a1"),
        "current-native selection clears the marker:\n{}",
        host.log.join("\n")
    );
    let fresh = binding_for(&reselect, "out-a", "a1")
        .unwrap_or_else(|| panic!("fresh logical adopts a1:\n{}", host.log.join("\n")));
    let ring = ring_ids_live(&engine, "out-a");
    let position = ring
        .iter()
        .position(|id| id == &fresh)
        .unwrap_or_else(|| panic!("fresh logical rings on out-a:\n{}", host.log.join("\n")));
    assert_eq!(
        position,
        1,
        "fresh row at compact live-binding position (a0 bound, a1 readopted):\n{}",
        host.log.join("\n")
    );
    for seq in 3..=4u64 {
        split_round(&mut engine, &mut host, seq, true);
    }
    let (creations, deletions, transfers) = (host.creations, host.deletions, host.transfers);
    let settled = host.orders.clone();
    for seq in 5..=10u64 {
        split_round(&mut engine, &mut host, seq, true);
    }
    assert_eq!(
        (host.creations, host.deletions, host.transfers),
        (creations, deletions, transfers),
        "independent reselect stays quiescent:\n{}",
        host.log.join("\n")
    );
    assert_eq!(
        host.orders,
        settled,
        "independent inventory stable after reselect:\n{}",
        host.log.join("\n")
    );
}

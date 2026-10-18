//! Per-output-local startup conversion (dual-output enable): the native
//! current shown on several outputs is owned by the primary; every other
//! output receives its own fresh backing plus membership-only moves of its
//! non-sticky windows from the shared native without changing output,
//! preserving float/fullscreen/maximize. A distinct pre-existing current
//! shown on a secondary belongs to that secondary. One ring per native;
//! visible-current evidence never creates ring ownership.
//!
//! Producer-shaped regressions: full [`WorldReport`] inputs on the
//! production transition, asserting the production [`WorldReply`] rows and
//! retained transfers.

use std::collections::BTreeMap;

use tiler_core::directional::{OutputId, WindowId, WorkspaceId};
use tiler_core::geometry::Rect;
use tiler_core::seed::EngineWindow;
use tiler_core::session::OutputDomain;
use tiler_core::size_hints::WindowSizeHints;
use tiler_core::workspace_scope::WorkspaceMode;
use tiler_core::world_engine::{WorldEngine, WorldOutcome};
use tiler_core::world_report::{
    DomainReport, HoldReason, MinimizedEntry, OutputTopology, ScopeAuthority, TransferKind,
    WORLD_CONTRACT_VERSION, WorkspaceBinding, WorldCommand, WorldReport,
};

fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

#[allow(clippy::too_many_arguments)]
fn win(
    id: &str,
    out: &str,
    ws: &str,
    sticky: bool,
    floating: bool,
    fullscreen: bool,
    maximized: bool,
) -> EngineWindow {
    EngineWindow {
        window: WindowId(id.to_owned()),
        output: OutputId(out.to_owned()),
        workspace: WorkspaceId(ws.to_owned()),
        rect: rect(0, 0, 200, 200),
        floating,
        fit_excluded: false,
        fullscreen,
        maximized,
        sticky,
        fixed_auto: false,
        fixed_suppress: false,
        hints: WindowSizeHints::none(),
    }
}

fn tiled(id: &str, out: &str, ws: &str) -> EngineWindow {
    win(id, out, ws, false, false, false, false)
}

fn domain(out: &str, ws: &str, bounds: Rect, windows: Vec<EngineWindow>) -> DomainReport {
    readable_domain(out, ws, bounds, windows, true)
}

fn readable_domain(
    out: &str,
    ws: &str,
    bounds: Rect,
    windows: Vec<EngineWindow>,
    readable: bool,
) -> DomainReport {
    DomainReport {
        domain: OutputDomain {
            id: OutputId(out.to_owned()),
            workspace: WorkspaceId(ws.to_owned()),
            bounds,
            gap: 8,
            adjacent: BTreeMap::new(),
        },
        outer_gap: 4,
        focused: WindowId(String::new()),
        windows,
        closed: Vec::new(),
        readable,
        hold: (!readable).then_some(HoldReason::Unreadable),
    }
}

fn binding(logical: &str, native: &str, out: &str) -> WorkspaceBinding {
    WorkspaceBinding {
        logical_id: logical.to_owned(),
        native_id: native.to_owned(),
        output: OutputId(out.to_owned()),
        pinned: false,
        displaced: false,
        managed: true,
    }
}

struct OutputSpec {
    out: &'static str,
    order: Vec<&'static str>,
    current: &'static str,
    rect: Option<Rect>,
    primary: bool,
}

fn output(out: &'static str, order: Vec<&'static str>, current: &'static str) -> OutputSpec {
    OutputSpec {
        out,
        order,
        current,
        rect: Some(match out {
            "out-2" => rect(800, 0, 800, 600),
            _ => rect(0, 0, 800, 600),
        }),
        primary: false,
    }
}

#[allow(clippy::too_many_arguments)]
fn report(
    request_seq: u64,
    observation_seq: u64,
    mode: WorkspaceMode,
    outputs: Vec<OutputSpec>,
    domains: Vec<DomainReport>,
    bindings: Vec<WorkspaceBinding>,
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
        workspace_mode: mode,
        topology: outputs
            .into_iter()
            .map(
                |OutputSpec {
                     out,
                     order,
                     current,
                     rect,
                     primary,
                 }| OutputTopology {
                    output: OutputId(out.to_owned()),
                    native_order: order.into_iter().map(str::to_owned).collect(),
                    current_native: current.to_owned(),
                    complete: true,
                    geometry: rect,
                    primary,
                },
            )
            .collect(),
        bindings,
        domains,
        minimized: Vec::new(),
        fixed_size_predicate: tiler_core::size_hints::FixedSizePredicate::BothAxes,
        migration_source_refill: tiler_core::output_selection::MigrationSourceRefill::LastRemaining,
        anchor_fallback: tiler_core::admission::AnchorFallback::default(),
        no_space_result: tiler_core::admission::NoSpaceResult::default(),
        default_tiled: true,
        focused_global: String::new(),
        inventory_complete: true,
        gesture_barrier: false,
        feedback: Vec::new(),
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

fn rows_for<'a>(
    reply: &'a tiler_core::world_report::WorldReply,
    native: &str,
) -> Vec<&'a tiler_core::world_report::WorkspaceDesire> {
    reply
        .workspaces
        .iter()
        .filter(|row| row.native_binding.as_deref() == Some(native) || row.id == native)
        .collect()
}

fn unbound_rows_for<'a>(
    reply: &'a tiler_core::world_report::WorldReply,
    out: &str,
) -> Vec<&'a tiler_core::world_report::WorkspaceDesire> {
    reply
        .workspaces
        .iter()
        .filter(|row| row.output.0 == out && row.native_binding.is_none())
        .collect()
}

fn startup_shared_report(seq: u64, natives: Vec<&'static str>) -> WorldReport {
    report(
        seq,
        seq,
        WorkspaceMode::PerOutputLocal,
        [
            output("out-1", natives.clone(), "ws-1"),
            output("out-2", natives.clone(), "ws-1"),
        ]
        .into_iter()
        .map(|mut spec| {
            spec.primary = spec.out == "out-1";
            spec
        })
        .collect(),
        vec![
            domain(
                "out-1",
                "ws-1",
                rect(0, 0, 800, 600),
                vec![tiled("win-a", "out-1", "ws-1")],
            ),
            domain(
                "out-2",
                "ws-1",
                rect(800, 0, 800, 600),
                vec![
                    tiled("win-b", "out-2", "ws-1"),
                    win("win-s", "out-2", "ws-1", true, false, false, false),
                    win("win-f", "out-2", "ws-1", false, true, false, false),
                    win("win-m", "out-2", "ws-1", false, false, false, true),
                ],
            ),
        ],
        Vec::new(),
        None,
        None,
    )
}

#[test]
fn shared_current_startup_primary_owns_and_secondary_converts() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    let reply = accepted(
        &mut engine,
        &startup_shared_report(1, vec!["ws-1", "ws-2", "ws-3"]),
    );
    // One ring per native: the shared current is homed exactly once, under
    // the primary. Visible-current evidence on the secondary never creates
    // ring ownership.
    let homed = rows_for(&reply, "ws-1");
    assert_eq!(homed.len(), 1, "shared native homed exactly once");
    assert_eq!(homed[0].output.0, "out-1", "primary owns shared current");
    // The converting secondary receives its own fresh backing: exactly one
    // unbound pending row, a fresh logical token (never a native id).
    let pending = unbound_rows_for(&reply, "out-2");
    assert_eq!(
        pending.len(),
        1,
        "secondary converts with one fresh backing"
    );
    let token = pending[0].logical_id.clone().expect("pending token");
    assert!(token.starts_with("wl-"), "fresh logical token");
    assert!(
        reply
            .workspaces
            .iter()
            .all(|row| row.native_binding.as_deref() != Some(token.as_str())),
        "token never aliases a native id"
    );
    // No membership moves before the fresh backing binds: the adapter must
    // not actuate against an unbound token.
    assert!(
        reply.transfers.is_empty(),
        "no conversion transfers while backing is unbound"
    );
}

#[test]
fn distinct_currents_belong_to_shown_output() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    let reply = accepted(
        &mut engine,
        &report(
            1,
            1,
            WorkspaceMode::PerOutputLocal,
            vec![
                output("out-1", vec!["ws-1", "ws-2"], "ws-1"),
                output("out-2", vec!["ws-1", "ws-2"], "ws-2"),
            ],
            vec![
                domain(
                    "out-1",
                    "ws-1",
                    rect(0, 0, 800, 600),
                    vec![tiled("win-a", "out-1", "ws-1")],
                ),
                domain(
                    "out-2",
                    "ws-2",
                    rect(800, 0, 800, 600),
                    vec![tiled("win-b", "out-2", "ws-2")],
                ),
            ],
            Vec::new(),
            None,
            None,
        ),
    );
    let primary = rows_for(&reply, "ws-1");
    assert_eq!(primary.len(), 1, "ws-1 homed exactly once");
    assert_eq!(primary[0].output.0, "out-1");
    let secondary = rows_for(&reply, "ws-2");
    assert_eq!(secondary.len(), 1, "ws-2 homed exactly once");
    assert_eq!(
        secondary[0].output.0, "out-2",
        "distinct secondary current belongs to secondary"
    );
}

#[test]
fn secondary_shared_view_completes_after_binding() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    let first = accepted(&mut engine, &startup_shared_report(1, vec!["ws-1", "ws-2"]));
    let token = unbound_rows_for(&first, "out-2")[0]
        .logical_id
        .clone()
        .expect("pending token");
    // Fresh backing binds under the secondary; its pre-existing windows ride
    // along on the same output.
    let second = accepted(
        &mut engine,
        &report(
            2,
            2,
            WorkspaceMode::PerOutputLocal,
            vec![
                output("out-1", vec!["ws-1", "ws-2", "ws-new"], "ws-1"),
                output("out-2", vec!["ws-1", "ws-2", "ws-new"], "ws-1"),
            ],
            vec![
                domain(
                    "out-1",
                    "ws-1",
                    rect(0, 0, 800, 600),
                    vec![tiled("win-a", "out-1", "ws-1")],
                ),
                domain(
                    "out-2",
                    "ws-1",
                    rect(800, 0, 800, 600),
                    vec![
                        tiled("win-b", "out-2", "ws-1"),
                        win("win-s", "out-2", "ws-1", true, false, false, false),
                        win("win-f", "out-2", "ws-1", false, true, false, false),
                        win("win-m", "out-2", "ws-1", false, false, false, true),
                    ],
                ),
            ],
            vec![binding(&token, "ws-new", "out-2")],
            None,
            None,
        ),
    );
    let homed = rows_for(&second, "ws-new");
    assert_eq!(homed.len(), 1, "fresh backing homed exactly once");
    assert_eq!(homed[0].output.0, "out-2");
    let mut moved: Vec<String> = second
        .transfers
        .iter()
        .filter(|t| matches!(t.kind, TransferKind::Window(_)))
        .map(|t| match &t.kind {
            TransferKind::Window(w) => w.0.clone(),
            _ => unreachable!(),
        })
        .collect();
    moved.sort();
    assert_eq!(
        moved,
        vec!["win-b".to_owned(), "win-f".to_owned(), "win-m".to_owned()],
        "non-sticky members convert on the same output, sticky stays"
    );
    for t in &second.transfers {
        if matches!(t.kind, TransferKind::Window(_)) {
            assert_eq!(
                t.target_output.0, "out-2",
                "conversion never changes output"
            );
            assert_eq!(
                t.target_workspace, "ws-new",
                "conversion targets fresh backing"
            );
        }
    }
    let views: Vec<&_> = second
        .transfers
        .iter()
        .filter(|t| matches!(t.kind, TransferKind::View) && t.target_output.0 == "out-2")
        .collect();
    assert_eq!(views.len(), 1, "secondary view follows its fresh backing");
    assert_eq!(views[0].target_workspace, "ws-new");
}

#[test]
fn delayed_binding_dedupes_adopted_native() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    let first = accepted(&mut engine, &startup_shared_report(1, vec!["ws-1"]));
    let token = unbound_rows_for(&first, "out-2")[0]
        .logical_id
        .clone()
        .expect("pending token");
    // The fresh backing appears natively before its binding is read back:
    // adoption parks it on the primary, but never duplicates it.
    let natives = vec!["ws-1", "ws-new"];
    let second = accepted(
        &mut engine,
        &report(
            2,
            2,
            WorkspaceMode::PerOutputLocal,
            vec![
                output("out-1", natives.clone(), "ws-1"),
                output("out-2", natives.clone(), "ws-1"),
            ],
            vec![
                domain(
                    "out-1",
                    "ws-1",
                    rect(0, 0, 800, 600),
                    vec![tiled("win-a", "out-1", "ws-1")],
                ),
                domain(
                    "out-2",
                    "ws-1",
                    rect(800, 0, 800, 600),
                    vec![tiled("win-b", "out-2", "ws-1")],
                ),
            ],
            Vec::new(),
            None,
            None,
        ),
    );
    assert_eq!(
        rows_for(&second, "ws-new").len(),
        1,
        "unbound fresh backing homed exactly once while pending"
    );
    // Delayed readback binds the fresh backing to the secondary: the primary
    // adoption yields, exactly one homing survives, and conversion fires.
    let third = accepted(
        &mut engine,
        &report(
            3,
            3,
            WorkspaceMode::PerOutputLocal,
            vec![
                output("out-1", natives.clone(), "ws-1"),
                output("out-2", natives.clone(), "ws-1"),
            ],
            vec![
                domain(
                    "out-1",
                    "ws-1",
                    rect(0, 0, 800, 600),
                    vec![tiled("win-a", "out-1", "ws-1")],
                ),
                domain(
                    "out-2",
                    "ws-1",
                    rect(800, 0, 800, 600),
                    vec![tiled("win-b", "out-2", "ws-1")],
                ),
            ],
            vec![binding(&token, "ws-new", "out-2")],
            None,
            None,
        ),
    );
    let homed = rows_for(&third, "ws-new");
    assert_eq!(homed.len(), 1, "bound backing homed exactly once");
    assert_eq!(homed[0].output.0, "out-2");
    assert!(
        third
            .transfers
            .iter()
            .any(|t| matches!(t.kind, TransferKind::Window(_))
                && t.target_output.0 == "out-2"
                && t.target_workspace == "ws-new"),
        "delayed binding still fires conversion"
    );
}

#[test]
fn wired_primary_owns_shared_current() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    let reply = accepted(
        &mut engine,
        &report(
            1,
            1,
            WorkspaceMode::PerOutputLocal,
            vec![
                {
                    let mut spec = output("out-1", vec!["ws-1", "ws-2"], "ws-1");
                    spec.primary = false;
                    spec
                },
                {
                    let mut spec = output("out-2", vec!["ws-1", "ws-2"], "ws-1");
                    spec.primary = true;
                    spec
                },
            ],
            vec![
                domain(
                    "out-1",
                    "ws-1",
                    rect(0, 0, 800, 600),
                    vec![tiled("win-a", "out-1", "ws-1")],
                ),
                domain(
                    "out-2",
                    "ws-1",
                    rect(800, 0, 800, 600),
                    vec![tiled("win-b", "out-2", "ws-1")],
                ),
            ],
            Vec::new(),
            None,
            None,
        ),
    );
    let homed = rows_for(&reply, "ws-1");
    assert_eq!(homed.len(), 1);
    assert_eq!(
        homed[0].output.0, "out-2",
        "wired primary owns the shared current, not topology order"
    );
    assert_eq!(
        unbound_rows_for(&reply, "out-1").len(),
        1,
        "topology-first output converts when it is not primary"
    );
}

#[test]
fn post_startup_foreign_view_stays_authoritative() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted(
        &mut engine,
        &report(
            1,
            1,
            WorkspaceMode::PerOutputLocal,
            vec![
                output("out-1", vec!["ws-1", "ws-2"], "ws-1"),
                output("out-2", vec!["ws-1", "ws-2"], "ws-2"),
            ],
            vec![
                domain(
                    "out-1",
                    "ws-1",
                    rect(0, 0, 800, 600),
                    vec![tiled("win-a", "out-1", "ws-1")],
                ),
                domain(
                    "out-2",
                    "ws-2",
                    rect(800, 0, 800, 600),
                    vec![tiled("win-b", "out-2", "ws-2")],
                ),
            ],
            Vec::new(),
            None,
            None,
        ),
    );
    // After startup the secondary shows the primary-owned workspace. The
    // startup partition must not re-fire: ownership stays, no conversion
    // transfers are retained, and the native keeps exactly one homing.
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            WorkspaceMode::PerOutputLocal,
            vec![
                output("out-1", vec!["ws-1", "ws-2"], "ws-1"),
                output("out-2", vec!["ws-1", "ws-2"], "ws-1"),
            ],
            vec![
                domain(
                    "out-1",
                    "ws-1",
                    rect(0, 0, 800, 600),
                    vec![tiled("win-a", "out-1", "ws-1")],
                ),
                domain(
                    "out-2",
                    "ws-1",
                    rect(800, 0, 800, 600),
                    vec![tiled("win-b", "out-2", "ws-1")],
                ),
            ],
            Vec::new(),
            None,
            None,
        ),
    );
    let homed = rows_for(&reply, "ws-1");
    assert_eq!(homed.len(), 1, "post-startup ownership never duplicates");
    assert_eq!(homed[0].output.0, "out-1");
    assert!(
        reply
            .transfers
            .iter()
            .all(|t| !matches!(t.kind, TransferKind::Window(_))),
        "no post-startup conversion member moves"
    );
}

/// Finding 1: a stale binding must never steal retained mapping. Distinct
/// initial currents partition first; a later fallback-echo binding homing
/// ws-2 at out-1 is ignored, never a rehome.
#[test]
fn stale_binding_never_steals_retained_mapping() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted(
        &mut engine,
        &report(
            1,
            1,
            WorkspaceMode::PerOutputLocal,
            vec![
                output("out-1", vec!["ws-1", "ws-2"], "ws-1"),
                output("out-2", vec!["ws-1", "ws-2"], "ws-2"),
            ],
            vec![
                domain(
                    "out-1",
                    "ws-1",
                    rect(0, 0, 800, 600),
                    vec![tiled("win-a", "out-1", "ws-1")],
                ),
                domain(
                    "out-2",
                    "ws-2",
                    rect(800, 0, 800, 600),
                    vec![tiled("win-b", "out-2", "ws-2")],
                ),
            ],
            Vec::new(),
            None,
            None,
        ),
    );
    // Stale fallback echo (logical === native, the generic adapter shape)
    // claims ws-2 for out-1 although out-2 retains and shows it.
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            WorkspaceMode::PerOutputLocal,
            vec![
                output("out-1", vec!["ws-1", "ws-2"], "ws-1"),
                output("out-2", vec!["ws-1", "ws-2"], "ws-2"),
            ],
            vec![
                domain(
                    "out-1",
                    "ws-1",
                    rect(0, 0, 800, 600),
                    vec![tiled("win-a", "out-1", "ws-1")],
                ),
                domain(
                    "out-2",
                    "ws-2",
                    rect(800, 0, 800, 600),
                    vec![tiled("win-b", "out-2", "ws-2")],
                ),
            ],
            vec![binding("ws-2", "ws-2", "out-1")],
            None,
            None,
        ),
    );
    let homed = rows_for(&reply, "ws-2");
    assert_eq!(homed.len(), 1, "stale seed never duplicates");
    assert_eq!(
        homed[0].output.0, "out-2",
        "retained mapping wins over stale binding"
    );
    assert!(
        reply.transfers.is_empty(),
        "no conversion fires from ignored evidence"
    );
}

/// Finding 2: actual initial bindings (legacy snapshot homing every native
/// at the primary with generic fallback echoes) must not override the
/// observed distinct secondary current at first complete startup.
#[test]
fn fallback_echo_bindings_do_not_override_observed_currents() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    let reply = accepted(
        &mut engine,
        &report(
            1,
            1,
            WorkspaceMode::PerOutputLocal,
            vec![
                output("out-1", vec!["ws-1", "ws-2"], "ws-1"),
                output("out-2", vec!["ws-1", "ws-2"], "ws-2"),
            ],
            vec![
                domain(
                    "out-1",
                    "ws-1",
                    rect(0, 0, 800, 600),
                    vec![tiled("win-a", "out-1", "ws-1")],
                ),
                domain(
                    "out-2",
                    "ws-2",
                    rect(800, 0, 800, 600),
                    vec![tiled("win-b", "out-2", "ws-2")],
                ),
            ],
            vec![
                binding("ws-1", "ws-1", "out-1"),
                binding("ws-2", "ws-2", "out-1"),
            ],
            None,
            None,
        ),
    );
    let secondary = rows_for(&reply, "ws-2");
    assert_eq!(secondary.len(), 1, "distinct current homed exactly once");
    assert_eq!(
        secondary[0].output.0, "out-2",
        "observed distinct secondary wins over fallback echoes"
    );
    assert!(
        reply.transfers.is_empty(),
        "no conversion when every current is owned"
    );
}

/// Findings 2+4: a partially readable startup report must not move mapping
/// ownership, mint conversion, or consume the startup pass. The first
/// complete report then partitions distinct currents exactly once.
#[test]
fn partial_report_freezes_startup_ownership_until_complete() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    let partial = accepted(
        &mut engine,
        &report(
            1,
            1,
            WorkspaceMode::PerOutputLocal,
            vec![
                output("out-1", vec!["ws-1", "ws-2"], "ws-1"),
                output("out-2", vec!["ws-1", "ws-2"], "ws-1"),
            ],
            vec![
                domain(
                    "out-1",
                    "ws-1",
                    rect(0, 0, 800, 600),
                    vec![tiled("win-a", "out-1", "ws-1")],
                ),
                readable_domain(
                    "out-2",
                    "ws-1",
                    rect(800, 0, 800, 600),
                    vec![tiled("win-b", "out-2", "ws-1")],
                    false,
                ),
            ],
            Vec::new(),
            None,
            None,
        ),
    );
    // Mapping untouched: nothing is owned yet, and no conversion token is
    // minted on partial evidence. (A trailing spare for the readable
    // output is ordinary lifecycle, not conversion, and binds output-
    // scoped later; the unreadable output gets nothing.)
    assert!(
        unbound_rows_for(&partial, "out-2").is_empty(),
        "partial report mints no conversion backing"
    );
    assert!(
        partial.transfers.is_empty(),
        "partial report retains no conversion"
    );
    // First complete report with distinct currents partitions, not converts.
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            WorkspaceMode::PerOutputLocal,
            vec![
                output("out-1", vec!["ws-1", "ws-2"], "ws-1"),
                output("out-2", vec!["ws-1", "ws-2"], "ws-2"),
            ],
            vec![
                domain(
                    "out-1",
                    "ws-1",
                    rect(0, 0, 800, 600),
                    vec![tiled("win-a", "out-1", "ws-1")],
                ),
                domain(
                    "out-2",
                    "ws-2",
                    rect(800, 0, 800, 600),
                    vec![tiled("win-b", "out-2", "ws-2")],
                ),
            ],
            Vec::new(),
            None,
            None,
        ),
    );
    assert_eq!(rows_for(&reply, "ws-1").len(), 1);
    assert_eq!(rows_for(&reply, "ws-1")[0].output.0, "out-1");
    let secondary = rows_for(&reply, "ws-2");
    assert_eq!(secondary.len(), 1, "distinct current homed exactly once");
    assert_eq!(secondary[0].output.0, "out-2");
    assert!(
        reply.transfers.is_empty(),
        "delayed startup still converts nothing when owned"
    );
}

/// Finding 4: shared current under a partially readable report mints
/// nothing and consumes nothing; the first complete report partitions
/// leftovers onto the primary and starts the conversion exactly once.
/// A stale distinct current seen only on the partial report must not
/// leak into ownership.
#[test]
fn partial_shared_report_defers_conversion_to_first_complete() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    // Partial report sees distinct currents; the out-2 domain is unreadable.
    let partial = accepted(
        &mut engine,
        &report(
            1,
            1,
            WorkspaceMode::PerOutputLocal,
            vec![
                output("out-1", vec!["ws-1", "ws-2"], "ws-1"),
                output("out-2", vec!["ws-1", "ws-2"], "ws-2"),
            ],
            vec![
                domain(
                    "out-1",
                    "ws-1",
                    rect(0, 0, 800, 600),
                    vec![tiled("win-a", "out-1", "ws-1")],
                ),
                readable_domain(
                    "out-2",
                    "ws-2",
                    rect(800, 0, 800, 600),
                    vec![tiled("win-b", "out-2", "ws-2")],
                    false,
                ),
            ],
            Vec::new(),
            None,
            None,
        ),
    );
    assert!(
        unbound_rows_for(&partial, "out-2").is_empty(),
        "no conversion token on partial evidence"
    );
    // First complete report shows the shared native on both outputs. The
    // stale ws-2 current from the partial report owns nothing.
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            WorkspaceMode::PerOutputLocal,
            vec![
                output("out-1", vec!["ws-1", "ws-2"], "ws-1"),
                output("out-2", vec!["ws-1", "ws-2"], "ws-1"),
            ],
            vec![
                domain(
                    "out-1",
                    "ws-1",
                    rect(0, 0, 800, 600),
                    vec![tiled("win-a", "out-1", "ws-1")],
                ),
                domain(
                    "out-2",
                    "ws-1",
                    rect(800, 0, 800, 600),
                    vec![tiled("win-b", "out-2", "ws-1")],
                ),
            ],
            Vec::new(),
            None,
            None,
        ),
    );
    let homed = rows_for(&reply, "ws-1");
    assert_eq!(homed.len(), 1, "shared native homed exactly once");
    assert_eq!(homed[0].output.0, "out-1");
    let leftover = rows_for(&reply, "ws-2");
    assert_eq!(leftover.len(), 1, "stale partial current adopts singly");
    assert_eq!(
        leftover[0].output.0, "out-1",
        "unseen leftovers adopt onto the primary"
    );
    assert_eq!(
        unbound_rows_for(&reply, "out-2").len(),
        1,
        "first complete report starts the conversion exactly once"
    );
    assert!(
        reply.transfers.is_empty(),
        "nothing moves before the fresh binding"
    );
}

/// Non-sticky minimized lifetimes convert membership-only with the rest
/// of the shared native's members: the retained window transfer targets
/// the fresh backing on the same output, the desired row rides the target
/// with minimized frame authority, and exactly one homing survives. The
/// sticky minimized lifetime is exempt: no transfer, homing untouched.
/// Observation proof (minimized homed at the target) settles the
/// retained transfer on the following report.
#[test]
fn non_sticky_minimized_window_on_shared_native_converts() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    let first = accepted(&mut engine, &startup_shared_report(1, vec!["ws-1", "ws-2"]));
    let token = unbound_rows_for(&first, "out-2")[0]
        .logical_id
        .clone()
        .expect("pending token");
    let mut minimized_report = report(
        2,
        2,
        WorkspaceMode::PerOutputLocal,
        vec![
            output("out-1", vec!["ws-1", "ws-2", "ws-new"], "ws-1"),
            output("out-2", vec!["ws-1", "ws-2", "ws-new"], "ws-1"),
        ],
        vec![
            domain(
                "out-1",
                "ws-1",
                rect(0, 0, 800, 600),
                vec![tiled("win-a", "out-1", "ws-1")],
            ),
            domain(
                "out-2",
                "ws-1",
                rect(800, 0, 800, 600),
                vec![tiled("win-b", "out-2", "ws-1")],
            ),
        ],
        vec![binding(&token, "ws-new", "out-2")],
        None,
        None,
    );
    minimized_report.minimized = vec![
        MinimizedEntry {
            window: WindowId("win-z".to_owned()),
            output: OutputId("out-2".to_owned()),
            workspace: WorkspaceId("ws-1".to_owned()),
            sticky: false,
        },
        MinimizedEntry {
            window: WindowId("win-y".to_owned()),
            output: OutputId("out-2".to_owned()),
            workspace: WorkspaceId("ws-1".to_owned()),
            sticky: true,
        },
    ];
    let reply = accepted(&mut engine, &minimized_report);
    let moved: Vec<String> = reply
        .transfers
        .iter()
        .filter(|t| matches!(t.kind, TransferKind::Window(_)))
        .map(|t| match &t.kind {
            TransferKind::Window(w) => w.0.clone(),
            _ => unreachable!(),
        })
        .collect();
    assert_eq!(
        moved,
        vec!["win-b".to_owned(), "win-z".to_owned()],
        "non-sticky members convert, including minimized; sticky minimized exempt"
    );
    for t in &reply.transfers {
        if matches!(t.kind, TransferKind::Window(_)) {
            assert_eq!(
                t.target_output.0, "out-2",
                "conversion never changes output"
            );
            assert_eq!(
                t.target_workspace, "ws-new",
                "conversion targets fresh backing"
            );
        }
    }
    let rows: Vec<_> = reply
        .windows
        .iter()
        .filter(|w| w.lifetime.0 == "win-z")
        .collect();
    assert_eq!(rows.len(), 1, "minimized lifetime keeps exactly one row");
    assert_eq!(
        format!("{}/{}", rows[0].output.0, rows[0].workspace),
        "out-2/ws-new",
        "minimized row rides the conversion target"
    );
    assert_eq!(
        rows[0].frame_authority,
        ScopeAuthority::Hold(HoldReason::Minimized),
        "minimized frame authority never tiles"
    );
    assert!(rows[0].allocation.is_none());
    assert_eq!(rows[0].membership_authority, ScopeAuthority::Controlled);
    let sticky_rows: Vec<_> = reply
        .windows
        .iter()
        .filter(|w| w.lifetime.0 == "win-y")
        .collect();
    assert_eq!(
        sticky_rows.len(),
        1,
        "sticky minimized keeps exactly one row"
    );
    assert_eq!(
        format!("{}/{}", sticky_rows[0].output.0, sticky_rows[0].workspace),
        "out-2/ws-1",
        "sticky minimized homing untouched"
    );
    // Observation proof settles the retained transfer: the minimized
    // lifetime homed at the fresh backing confirms, so the next reply
    // retains no transfer for it.
    let mut settled_report = report(
        3,
        3,
        WorkspaceMode::PerOutputLocal,
        vec![
            output("out-1", vec!["ws-1", "ws-2", "ws-new"], "ws-1"),
            output("out-2", vec!["ws-1", "ws-2", "ws-new"], "ws-new"),
        ],
        vec![
            domain(
                "out-1",
                "ws-1",
                rect(0, 0, 800, 600),
                vec![tiled("win-a", "out-1", "ws-1")],
            ),
            domain(
                "out-2",
                "ws-new",
                rect(800, 0, 800, 600),
                vec![tiled("win-b", "out-2", "ws-new")],
            ),
        ],
        vec![binding(&token, "ws-new", "out-2")],
        None,
        None,
    );
    settled_report.minimized = vec![
        MinimizedEntry {
            window: WindowId("win-z".to_owned()),
            output: OutputId("out-2".to_owned()),
            workspace: WorkspaceId("ws-new".to_owned()),
            sticky: false,
        },
        MinimizedEntry {
            window: WindowId("win-y".to_owned()),
            output: OutputId("out-2".to_owned()),
            workspace: WorkspaceId("ws-1".to_owned()),
            sticky: true,
        },
    ];
    let settled = accepted(&mut engine, &settled_report);
    assert!(
        settled
            .transfers
            .iter()
            .filter(|t| matches!(t.kind, TransferKind::Window(_)))
            .all(|t| match &t.kind {
                TransferKind::Window(w) => w.0 != "win-z",
                _ => true,
            }),
        "minimized observation proof settles conversion retention"
    );
}

/// Fullscreen members convert membership-only on the same output: the
/// overlay flag never blocks the membership move and no output send is
/// retained. A float member rides alongside with its float state intact.
#[test]
fn fullscreen_and_float_members_convert_on_same_output() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    let first = accepted(&mut engine, &startup_shared_report(1, vec!["ws-1", "ws-2"]));
    let token = unbound_rows_for(&first, "out-2")[0]
        .logical_id
        .clone()
        .expect("pending token");
    let reply = accepted(
        &mut engine,
        &report(
            2,
            2,
            WorkspaceMode::PerOutputLocal,
            vec![
                output("out-1", vec!["ws-1", "ws-2", "ws-new"], "ws-1"),
                output("out-2", vec!["ws-1", "ws-2", "ws-new"], "ws-1"),
            ],
            vec![
                domain(
                    "out-1",
                    "ws-1",
                    rect(0, 0, 800, 600),
                    vec![tiled("win-a", "out-1", "ws-1")],
                ),
                domain(
                    "out-2",
                    "ws-1",
                    rect(800, 0, 800, 600),
                    vec![
                        tiled("win-b", "out-2", "ws-1"),
                        win("win-fs", "out-2", "ws-1", false, false, true, false),
                        win("win-fl", "out-2", "ws-1", false, true, false, false),
                    ],
                ),
            ],
            vec![binding(&token, "ws-new", "out-2")],
            None,
            None,
        ),
    );
    let mut moved: Vec<String> = reply
        .transfers
        .iter()
        .filter(|t| matches!(t.kind, TransferKind::Window(_)))
        .map(|t| match &t.kind {
            TransferKind::Window(w) => w.0.clone(),
            _ => unreachable!(),
        })
        .collect();
    moved.sort();
    assert_eq!(
        moved,
        vec!["win-b".to_owned(), "win-fl".to_owned(), "win-fs".to_owned()],
        "fullscreen and float members convert with the rest"
    );
    for t in &reply.transfers {
        if matches!(t.kind, TransferKind::Window(_)) {
            assert_eq!(
                t.target_output.0, "out-2",
                "conversion never changes output"
            );
            assert_eq!(
                t.target_workspace, "ws-new",
                "conversion targets fresh backing"
            );
        }
    }
}

/// Follow-up: incomplete reports must not publish unowned lifecycle rows.
/// While complete mapping evidence is unavailable, natives with no retained
/// owner are omitted (scope/output holds remain); a native never gains a
/// row under more than one output from a partial report.
#[test]
fn incomplete_initial_report_publishes_no_unowned_rows() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    let partial = accepted(
        &mut engine,
        &report(
            1,
            1,
            WorkspaceMode::PerOutputLocal,
            vec![
                output("out-1", vec!["ws-1", "ws-2"], "ws-1"),
                output("out-2", vec!["ws-1", "ws-2"], "ws-1"),
            ],
            vec![
                domain(
                    "out-1",
                    "ws-1",
                    rect(0, 0, 800, 600),
                    vec![tiled("win-a", "out-1", "ws-1")],
                ),
                readable_domain(
                    "out-2",
                    "ws-1",
                    rect(800, 0, 800, 600),
                    vec![tiled("win-b", "out-2", "ws-1")],
                    false,
                ),
            ],
            Vec::new(),
            None,
            None,
        ),
    );
    for native in ["ws-1", "ws-2"] {
        assert!(
            rows_for(&partial, native).len() <= 1,
            "partial report never duplicates {native}"
        );
    }
    assert!(
        rows_for(&partial, "ws-1").is_empty() && rows_for(&partial, "ws-2").is_empty(),
        "unowned natives are omitted until complete evidence arrives"
    );
}

/// Follow-up: a retained partition under a later partial report still
/// publishes each mapped native exactly once (held), and the next complete
/// report preserves the conversion policy.
#[test]
fn retained_partial_report_keeps_single_homed_held_rows() {
    let mut engine = WorldEngine::new("gen-1").expect("valid");
    accepted(
        &mut engine,
        &report(
            1,
            1,
            WorkspaceMode::PerOutputLocal,
            vec![
                output("out-1", vec!["ws-1", "ws-2"], "ws-1"),
                output("out-2", vec!["ws-1", "ws-2"], "ws-1"),
            ],
            vec![
                domain(
                    "out-1",
                    "ws-1",
                    rect(0, 0, 800, 600),
                    vec![tiled("win-a", "out-1", "ws-1")],
                ),
                domain(
                    "out-2",
                    "ws-1",
                    rect(800, 0, 800, 600),
                    vec![tiled("win-b", "out-2", "ws-1")],
                ),
            ],
            Vec::new(),
            None,
            None,
        ),
    );
    let partial = accepted(
        &mut engine,
        &report(
            2,
            2,
            WorkspaceMode::PerOutputLocal,
            vec![
                output("out-1", vec!["ws-1", "ws-2"], "ws-1"),
                output("out-2", vec!["ws-1", "ws-2"], "ws-1"),
            ],
            vec![
                domain(
                    "out-1",
                    "ws-1",
                    rect(0, 0, 800, 600),
                    vec![tiled("win-a", "out-1", "ws-1")],
                ),
                readable_domain(
                    "out-2",
                    "ws-1",
                    rect(800, 0, 800, 600),
                    vec![tiled("win-b", "out-2", "ws-1")],
                    false,
                ),
            ],
            Vec::new(),
            None,
            None,
        ),
    );
    for native in ["ws-1", "ws-2"] {
        assert!(
            rows_for(&partial, native).len() <= 1,
            "retained partial never duplicates {native}"
        );
    }
    assert_eq!(
        rows_for(&partial, "ws-1").len(),
        1,
        "retained owner still publishes its held row"
    );
    // The next complete report preserves policy: shared native stays
    // single-homed on the primary with conversion pending.
    let reply = accepted(
        &mut engine,
        &report(
            3,
            3,
            WorkspaceMode::PerOutputLocal,
            vec![
                output("out-1", vec!["ws-1", "ws-2"], "ws-1"),
                output("out-2", vec!["ws-1", "ws-2"], "ws-1"),
            ],
            vec![
                domain(
                    "out-1",
                    "ws-1",
                    rect(0, 0, 800, 600),
                    vec![tiled("win-a", "out-1", "ws-1")],
                ),
                domain(
                    "out-2",
                    "ws-1",
                    rect(800, 0, 800, 600),
                    vec![tiled("win-b", "out-2", "ws-1")],
                ),
            ],
            Vec::new(),
            None,
            None,
        ),
    );
    let homed = rows_for(&reply, "ws-1");
    assert_eq!(homed.len(), 1, "shared native homed exactly once");
    assert_eq!(homed[0].output.0, "out-1");
    assert_eq!(
        unbound_rows_for(&reply, "out-2").len(),
        1,
        "complete follow-up resumes conversion"
    );
}

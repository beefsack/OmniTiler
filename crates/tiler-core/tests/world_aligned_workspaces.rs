#![forbid(unsafe_code)]
//! Aligned per-output-local workspaces (platform opt-in, off by default):
//! full native-position rings every output, per-output trailing empties,
//! global removal eligibility with a floor of two, per-(output, native)
//! logical identity, startup adoption without conversion, and holds on
//! incomplete reports. Whole-workspace migration into the destination
//! trailing empty lives in `world_aligned_migration.rs`; hotplug
//! displacement is a follow-up step and explicitly out of scope here.

use std::collections::{BTreeMap, BTreeSet};

use tiler_core::directional::{OutputId, WindowId, WorkspaceId};
use tiler_core::geometry::Rect;
use tiler_core::seed::EngineWindow;
use tiler_core::session::OutputDomain;
use tiler_core::size_hints::WindowSizeHints;
use tiler_core::workspace_scope::{
    TrailingSelection, WorkspaceMode, aligned_removal_set, aligned_ring_ids,
    aligned_trailing_index, resolve_aligned_trailing,
};
use tiler_core::world_engine::{WorldEngine, WorldEngineConfig, WorldOutcome};
use tiler_core::world_report::{
    DomainReport, HoldReason, MinimizedEntry, OutputTopology, WORLD_CONTRACT_VERSION,
    WorkspaceBinding, WorldCommand, WorldReport,
};

fn aligned_config() -> WorldEngineConfig {
    WorldEngineConfig {
        fixed_send_admission: false,
        maximized_focus_fence: true,
        aligned_per_output_workspaces: true,
    }
}

fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

fn win(id: &str, out: &str, ws: &str) -> EngineWindow {
    EngineWindow {
        window: WindowId(id.to_owned()),
        output: OutputId(out.to_owned()),
        workspace: WorkspaceId(ws.to_owned()),
        rect: rect(0, 0, 200, 200),
        floating: false,
        fit_excluded: false,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: WindowSizeHints::none(),
    }
}

fn readable_domain(out: &str, ws: &str, windows: Vec<EngineWindow>) -> DomainReport {
    DomainReport {
        domain: OutputDomain {
            id: OutputId(out.to_owned()),
            workspace: WorkspaceId(ws.to_owned()),
            bounds: rect(0, 0, 800, 600),
            gap: 8,
            adjacent: BTreeMap::new(),
        },
        outer_gap: 4,
        focused: WindowId(String::new()),
        windows,
        closed: Vec::new(),
        readable: true,
        hold: None,
    }
}

fn topo(out: &str, order: &[&str], current: &str, primary: bool) -> OutputTopology {
    OutputTopology {
        output: OutputId(out.to_owned()),
        native_order: order.iter().map(|s| s.to_string()).collect(),
        current_native: current.to_owned(),
        complete: true,
        geometry: Some(if out == "out-2" {
            rect(800, 0, 800, 600)
        } else {
            rect(0, 0, 800, 600)
        }),
        primary,
    }
}

#[allow(clippy::too_many_arguments)]
fn report(
    seq: u64,
    outputs: Vec<OutputTopology>,
    domains: Vec<DomainReport>,
    bindings: Vec<WorkspaceBinding>,
) -> WorldReport {
    report_with_minimized(seq, outputs, domains, bindings, Vec::new())
}

fn report_with_minimized(
    seq: u64,
    outputs: Vec<OutputTopology>,
    domains: Vec<DomainReport>,
    bindings: Vec<WorkspaceBinding>,
    minimized: Vec<MinimizedEntry>,
) -> WorldReport {
    WorldReport {
        contract_version: WORLD_CONTRACT_VERSION,
        owner: "owner-a".to_owned(),
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        observation_seq: seq,
        request_seq: seq,
        request_id: format!("req-{seq}"),
        command_seq: None,
        command: None,
        workspace_mode: WorkspaceMode::PerOutputLocal,
        topology: outputs,
        bindings,
        domains,
        minimized,
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

fn commanded(
    engine: &mut WorldEngine,
    report: &WorldReport,
    seq: u64,
    command: WorldCommand,
) -> tiler_core::world_report::WorldReply {
    let mut commanded = report.clone();
    commanded.observation_seq = seq;
    commanded.request_seq = seq;
    commanded.request_id = format!("req-{seq}");
    commanded.command_seq = Some(50 + seq);
    commanded.command = Some(command);
    match engine.handle_report(&commanded) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    }
}

fn minimized_entry(id: &str, out: &str, ws: &str, sticky: bool) -> MinimizedEntry {
    MinimizedEntry {
        window: WindowId(id.to_owned()),
        output: OutputId(out.to_owned()),
        workspace: WorkspaceId(ws.to_owned()),
        sticky,
    }
}

fn natives_for(reply: &tiler_core::world_report::WorldReply, out: &str) -> Vec<String> {
    let mut rows: Vec<_> = reply
        .workspaces
        .iter()
        .filter(|row| row.output.0 == out && row.native_binding.is_some())
        .collect();
    rows.sort_by_key(|row| row.order);
    rows.into_iter()
        .map(|row| row.native_binding.clone().expect("bound"))
        .collect()
}

#[test]
fn opt_in_off_by_default_and_leaves_disjoint_behavior() {
    // Off by default: `Default` matches the historical constructor exactly
    // (see `default_config_matches_new_exactly`), and the disjoint startup
    // conversion stays live for the default model (pinned by
    // `world_startup_conversion::shared_current_startup_primary_owns_and_secondary_converts`).
    // This unit therefore keeps `startup_conversion`; removal is deferred
    // until the KDE carrier enables aligned mode and the disjoint path is
    // provably unreachable.
    assert!(!WorldEngineConfig::default().aligned_per_output_workspaces);
}

#[test]
fn pure_aligned_ring_trailing_and_removal() {
    let ring = aligned_ring_ids(&["a".to_owned(), "b".to_owned(), "c".to_owned()]);
    assert_eq!(ring, vec!["a".to_owned(), "b".to_owned(), "c".to_owned()]);
    // Last occupied b -> trailing c.
    let occupied: BTreeSet<String> = ["b".to_owned()].into_iter().collect();
    assert_eq!(aligned_trailing_index(&ring, &occupied), Some(2));
    assert!(matches!(
        resolve_aligned_trailing(&ring, &occupied),
        TrailingSelection::Reuse(id) if id == "c"
    ));
    // All occupied -> creation.
    let full: BTreeSet<String> = ["a".to_owned(), "b".to_owned(), "c".to_owned()]
        .into_iter()
        .collect();
    assert_eq!(aligned_trailing_index(&ring, &full), None);
    assert!(matches!(
        resolve_aligned_trailing(&ring, &full),
        TrailingSelection::Create
    ));
    // Removal: hole on one output stays while another occupies it.
    let occupied_any: BTreeSet<String> = ["a".to_owned()].into_iter().collect();
    let trailing_any: BTreeSet<String> = ["c".to_owned()].into_iter().collect();
    let removed = aligned_removal_set(
        &ring,
        &occupied_any,
        &trailing_any,
        &BTreeSet::from(["a".to_owned()]),
        &BTreeSet::new(),
    );
    assert_eq!(removed, BTreeSet::from(["b".to_owned()]));
    // Floor of two including trailing: five live with four eligible keeps two.
    let live: Vec<String> = ["a", "b", "c", "d", "e"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    let removed = aligned_removal_set(
        &live,
        &BTreeSet::new(),
        &BTreeSet::from(["e".to_owned()]),
        &BTreeSet::new(),
        &BTreeSet::new(),
    );
    assert_eq!(live.len() - removed.len(), 2);
}

#[test]
fn aligned_rings_replicate_every_output_with_per_output_trailing() {
    let mut engine = WorldEngine::new_with_config("gen-1", aligned_config()).expect("valid");
    // Left occupies ws-1 only; right occupies ws-2 only; four natives live.
    let natives = ["ws-1", "ws-2", "ws-3", "ws-4"];
    let reply = accepted(
        &mut engine,
        &report(
            1,
            vec![
                topo("out-1", &natives, "ws-1", true),
                topo("out-2", &natives, "ws-2", false),
            ],
            vec![
                readable_domain("out-1", "ws-1", vec![win("win-a", "out-1", "ws-1")]),
                readable_domain("out-2", "ws-2", vec![win("win-b", "out-2", "ws-2")]),
            ],
            Vec::new(),
        ),
    );
    // Full native-position rings on both outputs.
    assert_eq!(
        natives_for(&reply, "out-1"),
        natives.iter().map(|s| s.to_string()).collect::<Vec<_>>()
    );
    assert_eq!(
        natives_for(&reply, "out-2"),
        natives.iter().map(|s| s.to_string()).collect::<Vec<_>>()
    );
    // Per-output trailing: out-1 trails ws-2 (first empty after ws-1),
    // out-2 trails ws-3 (first empty after ws-2). Both select without
    // creating.
    for (out, index, target) in [("out-1", 2u8, "ws-2"), ("out-2", 3u8, "ws-3")] {
        let mut numbered = report(
            10 + u64::from(index),
            vec![
                topo("out-1", &natives, "ws-1", true),
                topo("out-2", &natives, "ws-2", false),
            ],
            vec![
                readable_domain("out-1", "ws-1", vec![win("win-a", "out-1", "ws-1")]),
                readable_domain("out-2", "ws-2", vec![win("win-b", "out-2", "ws-2")]),
            ],
            Vec::new(),
        );
        numbered.command_seq = Some(100 + u64::from(index));
        numbered.command = Some(WorldCommand::SelectNumbered {
            output: OutputId(out.to_owned()),
            index,
        });
        let outcome = engine.handle_report(&numbered);
        let reply = match outcome {
            WorldOutcome::Accepted(reply) => reply,
            other => panic!("expected accepted, got {other:?}"),
        };
        let view = reply
            .transfers
            .iter()
            .find(|t| t.target_output.0 == out)
            .expect("view retained");
        assert_eq!(view.target_workspace, target, "output {out}");
    }
}

#[test]
fn aligned_hole_stays_while_other_output_occupies_and_removal_needs_all_empty() {
    let mut engine = WorldEngine::new_with_config("gen-1", aligned_config()).expect("valid");
    // ws-2 empty on out-1 but occupied on out-2: a hole, not removal.
    let natives = ["ws-1", "ws-2", "ws-3", "ws-4"];
    let reply = accepted(
        &mut engine,
        &report(
            1,
            vec![
                topo("out-1", &natives, "ws-1", true),
                topo("out-2", &natives, "ws-2", false),
            ],
            vec![
                readable_domain("out-1", "ws-1", vec![win("win-a", "out-1", "ws-1")]),
                readable_domain("out-2", "ws-2", vec![win("win-b", "out-2", "ws-2")]),
            ],
            Vec::new(),
        ),
    );
    let row = reply
        .workspaces
        .iter()
        .find(|row| row.output.0 == "out-1" && row.native_binding.as_deref() == Some("ws-2"))
        .expect("hole row");
    assert!(
        row.existence,
        "hole stays while another output occupies ws-2"
    );
    // ws-4 empty everywhere, not any trailing (trailings are ws-2/ws-3),
    // not shown: eligible for removal on both outputs.
    for out in ["out-1", "out-2"] {
        let row = reply
            .workspaces
            .iter()
            .find(|row| row.output.0 == out && row.native_binding.as_deref() == Some("ws-4"))
            .expect("ws-4 row");
        assert!(!row.existence, "ws-4 eligible on {out}");
    }
}

#[test]
fn aligned_startup_adopts_shared_current_without_conversion() {
    let mut engine = WorldEngine::new_with_config("gen-1", aligned_config()).expect("valid");
    // Both outputs show ws-1 with a hidden member on out-2's ws-1 cell.
    let natives = ["ws-1", "ws-2"];
    let reply = accepted(
        &mut engine,
        &report(
            1,
            vec![
                topo("out-1", &natives, "ws-1", true),
                topo("out-2", &natives, "ws-1", false),
            ],
            vec![
                readable_domain("out-1", "ws-1", vec![win("win-a", "out-1", "ws-1")]),
                readable_domain("out-2", "ws-1", vec![win("win-b", "out-2", "ws-1")]),
            ],
            Vec::new(),
        ),
    );
    // No conversion token rows: every row is bound, nothing pending.
    assert!(
        reply
            .workspaces
            .iter()
            .all(|row| row.native_binding.is_some()),
        "startup adopts without minting conversion backings"
    );
    assert!(
        reply.transfers.is_empty(),
        "no conversion member/view moves"
    );
    // Both outputs keep full rings including the shared native.
    assert_eq!(natives_for(&reply, "out-1"), vec!["ws-1", "ws-2"]);
    assert_eq!(natives_for(&reply, "out-2"), vec!["ws-1", "ws-2"]);
}

#[test]
fn aligned_logical_identity_is_per_output_native_and_stable() {
    let mut engine = WorldEngine::new_with_config("gen-1", aligned_config()).expect("valid");
    let natives = ["ws-1", "ws-2"];
    // Replicated bindings with a generic collapse would alias these; core
    // must ignore them for identity.
    let bindings = vec![
        WorkspaceBinding {
            logical_id: "ws-1".to_owned(),
            native_id: "ws-1".to_owned(),
            output: OutputId("out-1".to_owned()),
            pinned: false,
            displaced: false,
            managed: true,
        },
        WorkspaceBinding {
            logical_id: "ws-1".to_owned(),
            native_id: "ws-1".to_owned(),
            output: OutputId("out-2".to_owned()),
            pinned: false,
            displaced: false,
            managed: true,
        },
    ];
    let first = accepted(
        &mut engine,
        &report(
            1,
            vec![
                topo("out-1", &natives, "ws-1", true),
                topo("out-2", &natives, "ws-2", false),
            ],
            vec![
                readable_domain("out-1", "ws-1", vec![win("win-a", "out-1", "ws-1")]),
                readable_domain("out-2", "ws-2", vec![win("win-b", "out-2", "ws-2")]),
            ],
            bindings.clone(),
        ),
    );
    let logical = |reply: &tiler_core::world_report::WorldReply, out: &str, native: &str| {
        reply
            .workspaces
            .iter()
            .find(|row| row.output.0 == out && row.native_binding.as_deref() == Some(native))
            .expect("cell row")
            .logical_id
            .clone()
            .expect("logical")
    };
    let a11 = logical(&first, "out-1", "ws-1");
    let a21 = logical(&first, "out-2", "ws-1");
    assert_ne!(a11, a21, "same native has distinct per-output logicals");
    assert_ne!(a11, "ws-1", "no generic native->logical collapse");
    // Second identical report (new seq): identity is stable across rebinding.
    let second = accepted(
        &mut engine,
        &report(
            2,
            vec![
                topo("out-1", &natives, "ws-2", true),
                topo("out-2", &natives, "ws-1", false),
            ],
            vec![
                readable_domain("out-1", "ws-1", vec![win("win-a", "out-1", "ws-1")]),
                readable_domain("out-2", "ws-1", vec![win("win-b", "out-2", "ws-1")]),
            ],
            bindings,
        ),
    );
    assert_eq!(logical(&second, "out-1", "ws-1"), a11);
    assert_eq!(logical(&second, "out-2", "ws-1"), a21);
    // Output views project the per-output logical.
    let views: BTreeMap<&str, Option<String>> = second
        .outputs
        .iter()
        .map(|row| (row.output.0.as_str(), row.logical_view.clone()))
        .collect();
    assert_eq!(views["out-1"], Some(logical(&second, "out-1", "ws-2")));
    assert_eq!(views["out-2"], Some(logical(&second, "out-2", "ws-1")));
}

#[test]
fn aligned_incomplete_report_holds_without_removal_or_creation() {
    let mut engine = WorldEngine::new_with_config("gen-1", aligned_config()).expect("valid");
    let natives = ["ws-1", "ws-2", "ws-3"];
    let mut incomplete = report(
        1,
        vec![
            topo("out-1", &natives, "ws-1", true),
            topo("out-2", &natives, "ws-2", false),
        ],
        vec![
            readable_domain("out-1", "ws-1", vec![win("win-a", "out-1", "ws-1")]),
            readable_domain("out-2", "ws-2", vec![win("win-b", "out-2", "ws-2")]),
        ],
        Vec::new(),
    );
    incomplete.inventory_complete = false;
    let reply = accepted(&mut engine, &incomplete);
    assert!(
        reply
            .workspaces
            .iter()
            .all(|row| row.existence && row.hold == Some(HoldReason::IncompleteInventory))
    );
    assert!(
        reply
            .workspaces
            .iter()
            .all(|row| row.native_binding.is_some()),
        "no creation rows while held"
    );
}

#[test]
fn aligned_select_trailing_reuses_per_output_empty() {
    let mut engine = WorldEngine::new_with_config("gen-1", aligned_config()).expect("valid");
    // out-1 occupies ws-1 only (trailing ws-2); out-2 occupies ws-2 only
    // (trailing ws-3). Trailing selection reuses each output's own first
    // empty cell after its last occupied one and creates nothing.
    let natives = ["ws-1", "ws-2", "ws-3", "ws-4"];
    let base = || {
        report(
            1,
            vec![
                topo("out-1", &natives, "ws-1", true),
                topo("out-2", &natives, "ws-2", false),
            ],
            vec![
                readable_domain("out-1", "ws-1", vec![win("win-a", "out-1", "ws-1")]),
                readable_domain("out-2", "ws-2", vec![win("win-b", "out-2", "ws-2")]),
            ],
            Vec::new(),
        )
    };
    let first = accepted(&mut engine, &base());
    assert!(
        first
            .workspaces
            .iter()
            .all(|row| row.native_binding.is_some()),
        "trailing reuse creates no token rows"
    );
    for (out, target, seq, cmd) in [("out-1", "ws-2", 10u64, 60u64), ("out-2", "ws-3", 11, 61)] {
        let mut trailing = base();
        trailing.observation_seq = seq;
        trailing.request_seq = seq;
        trailing.request_id = format!("req-{seq}");
        trailing.command_seq = Some(cmd);
        trailing.command = Some(WorldCommand::SelectTrailing {
            output: OutputId(out.to_owned()),
        });
        let reply = match engine.handle_report(&trailing) {
            WorldOutcome::Accepted(reply) => reply,
            other => panic!("expected accepted, got {other:?}"),
        };
        let view = reply
            .transfers
            .iter()
            .find(|t| t.target_output.0 == out)
            .expect("view retained");
        assert_eq!(view.target_workspace, target, "output {out}");
        assert!(
            !view.target_workspace.starts_with("wl-"),
            "reuse names the live native, never a token"
        );
        assert!(
            reply
                .workspaces
                .iter()
                .all(|row| row.native_binding.is_some()),
            "still no token rows on {out}"
        );
    }
}

#[test]
fn aligned_select_trailing_creates_stable_token_when_full() {
    let mut engine = WorldEngine::new_with_config("gen-1", aligned_config()).expect("valid");
    // out-1 occupies every cell through the end: no trailing empty, so the
    // trailing path mints one stable pending token shared with lifecycle.
    let natives = ["ws-1", "ws-2"];
    let base = || {
        report(
            1,
            vec![
                topo("out-1", &natives, "ws-1", true),
                topo("out-2", &natives, "ws-2", false),
            ],
            vec![
                readable_domain("out-1", "ws-1", vec![win("win-a", "out-1", "ws-1")]),
                readable_domain("out-1", "ws-2", vec![win("win-c", "out-1", "ws-2")]),
                readable_domain("out-2", "ws-2", vec![win("win-b", "out-2", "ws-2")]),
            ],
            Vec::new(),
        )
    };
    accepted(&mut engine, &base());
    let mut tokens = Vec::new();
    for (seq, cmd) in [(10u64, 60u64), (11, 61)] {
        let mut trailing = base();
        trailing.observation_seq = seq;
        trailing.request_seq = seq;
        trailing.request_id = format!("req-{seq}");
        trailing.command_seq = Some(cmd);
        trailing.command = Some(WorldCommand::SelectTrailing {
            output: OutputId("out-1".to_owned()),
        });
        let reply = match engine.handle_report(&trailing) {
            WorldOutcome::Accepted(reply) => reply,
            other => panic!("expected accepted, got {other:?}"),
        };
        let view = reply
            .transfers
            .iter()
            .find(|t| t.target_output.0 == "out-1")
            .expect("token view retained");
        assert!(
            view.target_workspace.starts_with("wl-"),
            "full ring mints a pending token"
        );
        tokens.push(view.target_workspace.clone());
        assert!(
            reply.workspaces.iter().any(|row| row.output.0 == "out-1"
                && row.native_binding.is_none()
                && row.logical_id.as_deref() == Some(view.target_workspace.as_str())),
            "one unbound token row rides out-1"
        );
    }
    assert_eq!(tokens[0], tokens[1], "token stable across reports");
}

#[test]
fn aligned_send_numbered_zero_uses_per_output_trailing() {
    use tiler_core::world_report::TransferKind;
    let mut engine = WorldEngine::new_with_config("gen-1", aligned_config()).expect("valid");
    // win-a lives on out-1/ws-1. out-1's trailing is ws-2 (reported as an
    // empty readable domain), so numbered send 0 is an ordinary same-output
    // transfer there, never a token and never a view switch.
    let natives = ["ws-1", "ws-2", "ws-3"];
    let base = || {
        let mut domains = vec![
            readable_domain("out-1", "ws-1", vec![win("win-a", "out-1", "ws-1")]),
            readable_domain("out-2", "ws-2", vec![win("win-b", "out-2", "ws-2")]),
        ];
        domains.push(readable_domain("out-1", "ws-2", Vec::new()));
        report(
            1,
            vec![
                topo("out-1", &natives, "ws-1", true),
                topo("out-2", &natives, "ws-2", false),
            ],
            domains,
            Vec::new(),
        )
    };
    // Empty domains carry no focus.
    let mut first = base();
    first.domains[2].focused = WindowId(String::new());
    accepted(&mut engine, &first);
    let mut send = base();
    send.domains[2].focused = WindowId(String::new());
    // Numbered sends move the focused window of the source domain.
    send.domains[0].focused = WindowId("win-a".to_owned());
    send.observation_seq = 2;
    send.request_seq = 2;
    send.request_id = "req-2".to_owned();
    send.command_seq = Some(70);
    send.command = Some(WorldCommand::SendNumbered {
        window: WindowId("win-a".to_owned()),
        output: OutputId("out-1".to_owned()),
        index: 0,
        follow: false,
    });
    let reply = match engine.handle_report(&send) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    assert!(
        reply
            .transfers
            .iter()
            .any(|t| matches!(t.kind, TransferKind::Window(_))
                && t.target_output.0 == "out-1"
                && t.target_workspace == "ws-2"),
        "send 0 transfers win-a to out-1 trailing ws-2: {:?}",
        reply.transfers
    );
}

#[test]
fn aligned_protections_hold_pinned_displaced_unmanaged_and_flight() {
    let mut engine = WorldEngine::new_with_config("gen-1", aligned_config()).expect("valid");
    // Eight natives so the floor cannot spare anything: out-1 shows ws-1
    // (occupied), out-2 shows ws-2 (occupied). Trailings are ws-2 (out-1)
    // and ws-3 (out-2); ws-1 is occupied and shown. ws-4 is empty
    // everywhere, untrailing, unshown, unprotected -> the only removal.
    let natives = [
        "ws-1", "ws-2", "ws-3", "ws-4", "ws-5", "ws-6", "ws-7", "ws-8",
    ];
    let base = || {
        report(
            1,
            vec![
                topo("out-1", &natives, "ws-1", true),
                topo("out-2", &natives, "ws-2", false),
            ],
            vec![
                readable_domain("out-1", "ws-1", vec![win("win-a", "out-1", "ws-1")]),
                readable_domain("out-2", "ws-2", vec![win("win-b", "out-2", "ws-2")]),
            ],
            Vec::new(),
        )
    };
    accepted(&mut engine, &base());
    // Hold an in-flight view target on ws-8: a newer select still pending
    // arrival protects its native exactly like a binding flag.
    let mut select = base();
    select.observation_seq = 2;
    select.request_seq = 2;
    select.request_id = "req-2".to_owned();
    select.command_seq = Some(50);
    select.command = Some(WorldCommand::SelectWorkspace {
        output: OutputId("out-2".to_owned()),
        workspace: "ws-8".to_owned(),
    });
    let selected = match engine.handle_report(&select) {
        WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    };
    assert!(
        selected.transfers.iter().any(|t| {
            matches!(t.kind, tiler_core::world_report::TransferKind::View)
                && t.target_workspace == "ws-8"
        }),
        "select retains its ws-8 view"
    );
    // Pinned, displaced, and unmanaged bindings each protect their native
    // on every output, alongside the ws-8 flight above.
    let bindings = vec![
        WorkspaceBinding {
            logical_id: "keep-5".to_owned(),
            native_id: "ws-5".to_owned(),
            output: OutputId("out-1".to_owned()),
            pinned: true,
            displaced: false,
            managed: true,
        },
        WorkspaceBinding {
            logical_id: "keep-6".to_owned(),
            native_id: "ws-6".to_owned(),
            output: OutputId("out-1".to_owned()),
            pinned: false,
            displaced: true,
            managed: true,
        },
        WorkspaceBinding {
            logical_id: "keep-7".to_owned(),
            native_id: "ws-7".to_owned(),
            output: OutputId("out-1".to_owned()),
            pinned: false,
            displaced: false,
            managed: false,
        },
    ];
    let mut guarded = base();
    guarded.observation_seq = 3;
    guarded.request_seq = 3;
    guarded.request_id = "req-3".to_owned();
    guarded.bindings = bindings;
    let reply = accepted(&mut engine, &guarded);
    for (native, why) in [
        ("ws-5", "pinned"),
        ("ws-6", "displaced"),
        ("ws-7", "unmanaged"),
        ("ws-8", "flight"),
    ] {
        for out in ["out-1", "out-2"] {
            let row = reply
                .workspaces
                .iter()
                .find(|row| row.output.0 == out && row.native_binding.as_deref() == Some(native))
                .expect("protected row");
            assert!(row.existence, "{why} {native} keeps on {out}");
        }
    }
    for out in ["out-1", "out-2"] {
        let row = reply
            .workspaces
            .iter()
            .find(|row| row.output.0 == out && row.native_binding.as_deref() == Some("ws-4"))
            .expect("ws-4 row");
        assert!(!row.existence, "unprotected ws-4 eligible on {out}");
    }
}

#[test]
fn aligned_minimum_grows_with_one_global_creation() {
    let mut engine = WorldEngine::new_with_config("gen-1", aligned_config()).expect("valid");
    // One live native, empty everywhere: the trailing empty exists (ws-1
    // itself) but the global floor of two still wants one creation.
    // Creation is global (one desktop is empty on every output at once),
    // so exactly one output rides the token row, never one per output.
    let single = |seq: u64| {
        report(
            seq,
            vec![
                topo("out-1", &["ws-1"], "ws-1", true),
                topo("out-2", &["ws-1"], "ws-1", false),
            ],
            vec![
                readable_domain("out-1", "ws-1", Vec::new()),
                readable_domain("out-2", "ws-1", Vec::new()),
            ],
            Vec::new(),
        )
    };
    let first = accepted(&mut engine, &single(1));
    let unbound: Vec<_> = first
        .workspaces
        .iter()
        .filter(|row| row.native_binding.is_none())
        .collect();
    assert_eq!(
        unbound.len(),
        1,
        "one global creation, not one per output: {:?}",
        first.workspaces
    );
    assert_eq!(unbound[0].output.0, "out-1");
    let token = unbound[0].logical_id.clone().expect("token logical");
    assert!(token.starts_with("wl-"), "stable pending token");
    // Stable across reports while unbound.
    let second = accepted(&mut engine, &single(2));
    let again: Vec<_> = second
        .workspaces
        .iter()
        .filter(|row| row.native_binding.is_none())
        .collect();
    assert_eq!(again.len(), 1);
    assert_eq!(again[0].logical_id.as_deref(), Some(token.as_str()));
    assert_eq!(again[0].output.0, "out-1");
    // The adapter binds the second native: the floor is met, both
    // outputs own the full ring, and no token row remains.
    let natives = ["ws-1", "ws-2"];
    let bound = report(
        3,
        vec![
            topo("out-1", &natives, "ws-1", true),
            topo("out-2", &natives, "ws-1", false),
        ],
        vec![
            readable_domain("out-1", "ws-1", Vec::new()),
            readable_domain("out-2", "ws-1", Vec::new()),
        ],
        vec![WorkspaceBinding {
            logical_id: token,
            native_id: "ws-2".to_owned(),
            output: OutputId("out-1".to_owned()),
            pinned: false,
            displaced: false,
            managed: true,
        }],
    );
    let grown = accepted(&mut engine, &bound);
    assert!(
        grown
            .workspaces
            .iter()
            .all(|row| row.native_binding.is_some()),
        "floor met, no creation rows: {:?}",
        grown.workspaces
    );
    assert_eq!(natives_for(&grown, "out-1"), vec!["ws-1", "ws-2"]);
    assert_eq!(natives_for(&grown, "out-2"), vec!["ws-1", "ws-2"]);
}

fn sticky_win(id: &str, out: &str, ws: &str) -> EngineWindow {
    EngineWindow {
        sticky: true,
        floating: true,
        fit_excluded: true,
        ..win(id, out, ws)
    }
}

#[test]
fn aligned_showing_empty_end_creates_no_spare_and_next_wraps() {
    let mut engine = WorldEngine::new_with_config("gen-1", aligned_config()).expect("valid");
    let natives = ["ws-1", "ws-2", "ws-3", "ws-4"];
    // out-1 occupies ws-1 only; ws-4 is shown but holds only a sticky
    // anchor, so it stays empty for spare purposes. out-2 occupies ws-2.
    // The sticky anchor stays present for the whole journey (T03 shape):
    // retention keeps its homed cell instead of looping on removal.
    let shown = |seq: u64, left: &str, right: &str| {
        report(
            seq,
            vec![
                topo("out-1", &natives, left, true),
                topo("out-2", &natives, right, false),
            ],
            vec![
                readable_domain("out-1", "ws-1", vec![win("win-a", "out-1", "ws-1")]),
                readable_domain(
                    "out-1",
                    "ws-4",
                    vec![sticky_win("win-sticky", "out-1", "ws-4")],
                ),
                readable_domain("out-2", "ws-2", vec![win("win-b", "out-2", "ws-2")]),
            ],
            Vec::new(),
        )
    };
    let reply = accepted(&mut engine, &shown(1, "ws-4", "ws-2"));
    assert!(
        reply
            .workspaces
            .iter()
            .all(|row| row.native_binding.is_some()),
        "showing empty end creates no spare: {:?}",
        reply.workspaces
    );
    // Forward wrap from the shown empty end on out-1 with the anchor
    // present: the retained end wraps instead of stepping onto a spare.
    let reply = commanded(
        &mut engine,
        &shown(2, "ws-4", "ws-2"),
        2,
        WorldCommand::SelectRelative {
            output: OutputId("out-1".to_owned()),
            delta: 1,
        },
    );
    let view = reply
        .transfers
        .iter()
        .find(|t| t.target_output.0 == "out-1")
        .expect("view retained");
    assert_eq!(view.target_workspace, "ws-1", "forward wrap on out-1");
    // After the departure the sticky-homed end is invisible yet retained
    // (never removal-eligible while the anchor persists), so the ring and
    // the spare state converge with no token and no removal.
    let reply = accepted(&mut engine, &shown(3, "ws-1", "ws-2"));
    assert!(
        reply
            .workspaces
            .iter()
            .all(|row| row.native_binding.is_some()),
        "departure with anchor present mints nothing: {:?}",
        reply.workspaces
    );
    let held = reply
        .workspaces
        .iter()
        .find(|row| row.output.0 == "out-1" && row.native_binding.as_deref() == Some("ws-4"))
        .expect("sticky-homed end keeps its row");
    assert!(
        held.existence,
        "sticky-homed end retained while shown empty"
    );
    // The right output shows its own empty end (genuinely empty there: the
    // anchor is homed on out-1) and wraps forward the same way with the
    // left view unchanged.
    let reply = accepted(&mut engine, &shown(4, "ws-1", "ws-4"));
    assert!(
        reply
            .workspaces
            .iter()
            .all(|row| row.native_binding.is_some()),
        "genuinely empty shown end creates no spare: {:?}",
        reply.workspaces
    );
    let reply = commanded(
        &mut engine,
        &shown(5, "ws-1", "ws-4"),
        5,
        WorldCommand::SelectRelative {
            output: OutputId("out-2".to_owned()),
            delta: 1,
        },
    );
    let view = reply
        .transfers
        .iter()
        .find(|t| t.target_output.0 == "out-2")
        .expect("view retained");
    assert_eq!(view.target_workspace, "ws-1", "forward wrap on out-2");
    // Numbered departure from the empty end works with the anchor present.
    let reply = commanded(
        &mut engine,
        &shown(6, "ws-4", "ws-2"),
        6,
        WorldCommand::SelectNumbered {
            output: OutputId("out-1".to_owned()),
            index: 1,
        },
    );
    let view = reply
        .transfers
        .iter()
        .find(|t| t.target_output.0 == "out-1")
        .expect("view retained");
    assert_eq!(view.target_workspace, "ws-1");
    // Occupancy arrival still extends the spare on each output: a real
    // window on an output's end leaves no empty after its last occupied
    // cell, minting exactly one spare per fill.
    let mut fill_left = shown(7, "ws-1", "ws-2");
    fill_left.domains.push(readable_domain(
        "out-1",
        "ws-4",
        vec![win("win-c", "out-1", "ws-4")],
    ));
    // The mixed sticky-plus-ordinary domain counts as occupied.
    let reply = accepted(&mut engine, &fill_left);
    let tokens: Vec<_> = reply
        .workspaces
        .iter()
        .filter(|row| row.native_binding.is_none())
        .collect();
    assert_eq!(
        tokens.len(),
        1,
        "left fill mints exactly one spare: {:?}",
        reply.workspaces
    );
    assert_eq!(tokens[0].output.0, "out-1");
    let token = tokens[0].logical_id.clone().expect("token logical");
    // Bind the minted spare, then fill the right end the same way.
    let bound_natives = ["ws-1", "ws-2", "ws-3", "ws-4", "ws-5"];
    let mut bound = report(
        8,
        vec![
            topo("out-1", &bound_natives, "ws-1", true),
            topo("out-2", &bound_natives, "ws-2", false),
        ],
        vec![
            readable_domain("out-1", "ws-1", vec![win("win-a", "out-1", "ws-1")]),
            readable_domain(
                "out-1",
                "ws-4",
                vec![
                    sticky_win("win-sticky", "out-1", "ws-4"),
                    win("win-c", "out-1", "ws-4"),
                ],
            ),
            readable_domain("out-2", "ws-2", vec![win("win-b", "out-2", "ws-2")]),
        ],
        vec![WorkspaceBinding {
            logical_id: token,
            native_id: "ws-5".to_owned(),
            output: OutputId("out-1".to_owned()),
            pinned: false,
            displaced: false,
            managed: true,
        }],
    );
    let reply = accepted(&mut engine, &bound);
    assert!(
        reply
            .workspaces
            .iter()
            .all(|row| row.native_binding.is_some()),
        "bound spare settles with no dangling token: {:?}",
        reply.workspaces
    );
    bound.observation_seq = 9;
    bound.request_seq = 9;
    bound.request_id = "req-9".to_owned();
    bound.domains.push(readable_domain(
        "out-2",
        "ws-5",
        vec![win("win-d", "out-2", "ws-5")],
    ));
    let reply = accepted(&mut engine, &bound);
    let tokens: Vec<_> = reply
        .workspaces
        .iter()
        .filter(|row| row.native_binding.is_none())
        .collect();
    assert_eq!(
        tokens.len(),
        1,
        "right fill mints exactly one spare: {:?}",
        reply.workspaces
    );
    assert_eq!(tokens[0].output.0, "out-2");
}

#[test]
fn aligned_ordinary_minimized_occupies_but_sticky_minimized_does_not() {
    let mut engine = WorldEngine::new_with_config("gen-1", aligned_config()).expect("valid");
    let natives = ["ws-1", "ws-2", "ws-3"];
    let base = |seq: u64, minimized: Vec<MinimizedEntry>| {
        report_with_minimized(
            seq,
            vec![
                topo("out-1", &natives, "ws-1", true),
                topo("out-2", &natives, "ws-2", false),
            ],
            vec![
                readable_domain("out-1", "ws-1", vec![win("win-a", "out-1", "ws-1")]),
                readable_domain("out-2", "ws-2", vec![win("win-b", "out-2", "ws-2")]),
            ],
            Vec::new(),
            minimized,
        )
    };
    // An ordinary minimized lifetime homed on the end occupies it: no
    // empty remains after the last occupied cell, so one spare is minted.
    let reply = accepted(
        &mut engine,
        &base(1, vec![minimized_entry("win-m", "out-1", "ws-3", false)]),
    );
    assert!(
        reply
            .workspaces
            .iter()
            .any(|row| row.native_binding.is_none()),
        "ordinary minimized end still mints the spare: {:?}",
        reply.workspaces
    );
    // A sticky minimized lifetime is an all-desktops exception like a
    // sticky row: it never occupies, so the trailing empty is reused.
    let mut fresh = WorldEngine::new_with_config("gen-1", aligned_config()).expect("valid");
    let reply = accepted(
        &mut fresh,
        &base(1, vec![minimized_entry("win-m", "out-1", "ws-3", true)]),
    );
    assert!(
        reply
            .workspaces
            .iter()
            .all(|row| row.native_binding.is_some()),
        "sticky minimized end creates no spare: {:?}",
        reply.workspaces
    );
}

#[test]
fn aligned_sticky_homed_invisible_cell_retained_then_released() {
    let mut engine = WorldEngine::new_with_config("gen-1", aligned_config()).expect("valid");
    let natives = ["ws-1", "ws-2", "ws-3", "ws-4"];
    // ws-2 is empty and invisible on out-1 (shown ws-1) with only a sticky
    // anchor: it is neither trailing (trailing is ws-4) nor visible, yet it
    // must be retained while the anchor persists instead of looping on a
    // removal the adapter will hold.
    let anchored = report(
        1,
        vec![
            topo("out-1", &natives, "ws-1", true),
            topo("out-2", &natives, "ws-4", false),
        ],
        vec![
            readable_domain("out-1", "ws-1", vec![win("win-a", "out-1", "ws-1")]),
            readable_domain(
                "out-1",
                "ws-2",
                vec![sticky_win("win-sticky", "out-1", "ws-2")],
            ),
            readable_domain("out-1", "ws-3", vec![win("win-c", "out-1", "ws-3")]),
            readable_domain("out-2", "ws-4", vec![win("win-b", "out-2", "ws-4")]),
        ],
        Vec::new(),
    );
    let reply = accepted(&mut engine, &anchored);
    let held = reply
        .workspaces
        .iter()
        .find(|row| row.output.0 == "out-1" && row.native_binding.as_deref() == Some("ws-2"))
        .expect("sticky-homed row");
    assert!(
        held.existence,
        "sticky-homed invisible non-trailing cell retained: {:?}",
        reply.workspaces
    );
    // After the anchor closes the same cell is genuinely empty and
    // unretained: normal lifecycle marks it removal-eligible.
    let mut released = anchored.clone();
    released.observation_seq = 2;
    released.request_seq = 2;
    released.request_id = "req-2".to_owned();
    released
        .domains
        .retain(|domain| !(domain.domain.id.0 == "out-1" && domain.domain.workspace.0 == "ws-2"));
    let reply = accepted(&mut engine, &released);
    let gone = reply
        .workspaces
        .iter()
        .find(|row| row.output.0 == "out-1" && row.native_binding.as_deref() == Some("ws-2"))
        .expect("released row");
    assert!(
        !gone.existence,
        "released cell becomes removal-eligible: {:?}",
        reply.workspaces
    );
}

#[test]
fn disjoint_sticky_trailing_reuses_and_homed_cell_kept() {
    // Default (disjoint) model: a sticky-only trailing cell is empty for
    // spare purposes (per the committed scoped-trailing-facts contract),
    // while a sticky-homed invisible non-final cell keeps its row exactly
    // as before the occupancy exclusion.
    let mut engine =
        WorldEngine::new_with_config("gen-1", WorldEngineConfig::default()).expect("valid");
    let natives = ["w1", "w2", "w3"];
    let reply = accepted(
        &mut engine,
        &report(
            1,
            vec![topo("out", &natives, "w3", true)],
            vec![
                readable_domain("out", "w1", vec![win("win-a", "out", "w1")]),
                readable_domain("out", "w2", vec![sticky_win("win-s", "out", "w2")]),
            ],
            Vec::new(),
        ),
    );
    assert!(
        reply
            .workspaces
            .iter()
            .all(|row| row.native_binding.is_some()),
        "disjoint sticky trailing reuses, no creation: {:?}",
        reply.workspaces
    );
    let kept = reply
        .workspaces
        .iter()
        .find(|row| row.native_binding.as_deref() == Some("w2"))
        .expect("sticky-homed row");
    assert!(kept.existence, "disjoint sticky-homed cell kept");
}

#[test]
fn shared_sticky_trailing_reuses() {
    let mut engine =
        WorldEngine::new_with_config("gen-1", WorldEngineConfig::default()).expect("valid");
    let natives = ["w1", "w2"];
    let mut shared = report(
        1,
        vec![topo("out", &natives, "w2", true)],
        vec![
            readable_domain("out", "w1", vec![win("win-a", "out", "w1")]),
            readable_domain("out", "w2", vec![sticky_win("win-s", "out", "w2")]),
        ],
        Vec::new(),
    );
    shared.workspace_mode = WorkspaceMode::Shared;
    let reply = accepted(&mut engine, &shared);
    assert!(
        reply
            .workspaces
            .iter()
            .all(|row| row.native_binding.is_some()),
        "shared sticky trailing reuses, no creation: {:?}",
        reply.workspaces
    );
}

//! Same-output workspace send and cross-output send tests (portable, headless)
//! over `WorldLayout::run_pair_send` and public `WorldEngine::handle_report`
//! for world-owned unknown-target refusals.
//!
//! Ported from `tests/session_send_to_workspace.rs` (15 tests):
//! - Preserved: follow/stay workspace and output sends, occupied-target
//!   remembered-leaf split, focus-history fallback when remembered is stale,
//!   empty-destination lone-root follow/stay, stay-of-sole-window, follow-vs-stay
//!   parity, post-stay native focus authority, R1-R4 rule/capability parity,
//!   output-vs-workspace scope separation, sticky refusal vs intentional float
//!   deferred carry, and refusal gates (`FocusMismatch`, `Unchanged`,
//!   `UnknownDomain`/`send-target-unknown`, `CrossDomainMismatch`,
//!   `PartialObservation`, `MalformedInput`).
//! - Retired (deleted `Session`/`Reconciler`/`Ack`/`Pending`/`Divergence`
//!   handshake legs inside `refusals_fail_closed_then_lifecycle_diverges`):
//!   `CapabilityRefused`, `PendingExists`, `NotAcknowledged`, and
//!   `PostconditionMismatch`.

use std::collections::{BTreeMap, BTreeSet};

use crate::directional::{Axis, Capability, Node, NodeId, OutputId, Rule, WindowId, WorkspaceId};
use crate::geometry::Rect;
use crate::seed::EngineWindow;
use crate::session::{
    DomainKey, ExceptionBehavior, ExceptionFlags, ObservedWindow, OutputDomain, RefusalKind,
};
use crate::size_hints::WindowSizeHints;
use crate::world_engine::WorldEngine;
use crate::world_report::{
    BatchSettlement, CommandRejectKind, CommandResult, PresentationIngressV4, PresentationRowV4,
    WORLD_CONTRACT_VERSION_V4, WorldCommand, WorldReportV4,
};
use crate::world_run_v4::V4RunOutcome;
use crate::world_v4_support_tests::{fact, screen};

use super::outcome::{LayoutFailure, PairOperationFailure, PairSendKind, PairSendOutcome};
use super::pair_operation::PairOperationContext;
use super::state::{DomainLayout, WorldLayout};

fn bounds() -> Rect {
    Rect {
        x: 0,
        y: 0,
        w: 120,
        h: 80,
    }
}

fn dom(o: &str, w: &str) -> OutputDomain {
    OutputDomain {
        id: OutputId(o.to_owned()),
        workspace: WorkspaceId(w.to_owned()),
        bounds: bounds(),
        gap: 0,
        adjacent: BTreeMap::new(),
    }
}

fn dk(o: &str, ws: &str) -> DomainKey {
    DomainKey {
        output: OutputId(o.to_owned()),
        workspace: WorkspaceId(ws.to_owned()),
    }
}

fn win(id: &str) -> WindowId {
    WindowId(id.to_owned())
}

fn tiled(w: &str, o: &str, ws: &str) -> ObservedWindow {
    ObservedWindow {
        window: win(w),
        output: OutputId(o.to_owned()),
        workspace: WorkspaceId(ws.to_owned()),
        floating: false,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixed_auto: false,
        fixed_suppress: false,
        hints: WindowSizeHints::none(),
    }
}

fn eng_from_observed(obs: &ObservedWindow) -> EngineWindow {
    EngineWindow {
        window: obs.window.clone(),
        output: obs.output.clone(),
        workspace: obs.workspace.clone(),
        rect: bounds(),
        floating: obs.floating,
        fit_excluded: false,
        fullscreen: obs.fullscreen,
        maximized: obs.maximized,
        sticky: obs.sticky,
        fixed_auto: obs.fixed_auto,
        fixed_suppress: obs.fixed_suppress,
        hints: obs.hints,
    }
}

fn domain_observed(layout: &WorldLayout, domain: &DomainKey) -> Vec<ObservedWindow> {
    let mut windows: Vec<ObservedWindow> = layout
        .tiled_windows(domain)
        .map(|w| tiled(&w.0, &domain.output.0, &domain.workspace.0))
        .collect();
    for (w, flags) in layout.exception_windows_with_flags(domain) {
        let mut row = tiled(&w.0, &domain.output.0, &domain.workspace.0);
        row.floating = flags.floating;
        row.fullscreen = flags.fullscreen;
        row.maximized = flags.maximized;
        row.sticky = flags.sticky;
        windows.push(row);
    }
    windows
}

fn pair_observed(layout: &WorldLayout, a: &DomainKey, b: &DomainKey) -> Vec<EngineWindow> {
    domain_observed(layout, a)
        .into_iter()
        .chain(domain_observed(layout, b))
        .map(|o| eng_from_observed(&o))
        .collect()
}

fn admit(layout: &mut WorldLayout, w: &str, o: &str, ws: &str, pw: i32, ph: i32) {
    let k = dk(o, ws);
    if !layout.contains_domain(&k) {
        layout
            .domains
            .insert(k.clone(), DomainLayout::new(bounds(), 0, 0));
    }
    let mut obs = domain_observed(layout, &k);
    obs.push(tiled(w, o, ws));
    layout
        .admit_window(
            &k,
            &win(w),
            ExceptionFlags::none(),
            None,
            Rect {
                x: 0,
                y: 0,
                w: pw,
                h: ph,
            },
            false,
            false,
            &obs,
        )
        .expect("admit");
}

fn defer_exception(layout: &mut WorldLayout, w: &str, o: &str, ws: &str, flags: ExceptionFlags) {
    let k = dk(o, ws);
    if !layout.contains_domain(&k) {
        layout
            .domains
            .insert(k.clone(), DomainLayout::new(bounds(), 0, 0));
    }
    let mut row = tiled(w, o, ws);
    row.floating = flags.floating;
    row.fullscreen = flags.fullscreen;
    row.maximized = flags.maximized;
    row.sticky = flags.sticky;
    let mut obs = domain_observed(layout, &k);
    obs.push(row);
    layout
        .admit_window(
            &k,
            &win(w),
            flags,
            Some(ExceptionBehavior::Defer),
            bounds(),
            false,
            false,
            &obs,
        )
        .expect("defer");
}

fn send(
    layout: &mut WorldLayout,
    source: &DomainKey,
    target: &DomainKey,
    subject: &str,
    observed_focus: &str,
    follow: bool,
    allow_cross_output: bool,
) -> Result<PairSendOutcome, PairOperationFailure> {
    let src_dom = dom(&source.output.0, &source.workspace.0);
    let dst_dom = dom(&target.output.0, &target.workspace.0);
    let observed = pair_observed(layout, source, target);
    let subj = win(subject);
    let focus = win(observed_focus);
    let ctx = PairOperationContext {
        source_domain: &src_dom,
        target_domain: &dst_dom,
        source_outer_gap: 0,
        target_outer_gap: Some(0),
        observed_focus: &focus,
        observed: &observed,
        closed: &[],
    };
    layout.run_pair_send(&ctx, &subj, follow, allow_cross_output)
}

fn occupied_trio() -> WorldLayout {
    let mut layout = WorldLayout::new();
    admit(&mut layout, "win-t1", "out-1", "ws-b", 120, 80);
    admit(&mut layout, "win-t2", "out-1", "ws-b", 120, 80);
    admit(&mut layout, "win-1", "out-1", "ws-a", 120, 80);
    admit(&mut layout, "win-2", "out-1", "ws-a", 120, 80);
    admit(&mut layout, "win-3", "out-1", "ws-a", 120, 80);
    layout
}

fn flat(t: Option<&Node>) -> Vec<String> {
    fn go(n: &Node, out: &mut Vec<String>) {
        match n {
            Node::Leaf { id } => out.push(id.0.clone()),
            Node::Group { children, .. } => children.iter().for_each(|c| go(c, out)),
        }
    }
    let mut v = vec![];
    if let Some(t) = t {
        go(t, &mut v);
    }
    v
}

fn leaves_of(layout: &WorldLayout, o: &str, ws: &str) -> Vec<String> {
    flat(layout.tree(&dk(o, ws)))
}

fn parent_of(tree: &Node, leaf: &str) -> Option<NodeId> {
    match tree {
        Node::Leaf { .. } => None,
        Node::Group { id, children, .. } => {
            if children.iter().any(|c| c.id().0 == leaf) {
                return Some(id.clone());
            }
            children.iter().find_map(|c| parent_of(c, leaf))
        }
    }
}

fn report_v4(seq: u64, windows: Vec<crate::world_report::WindowFact>) -> WorldReportV4 {
    WorldReportV4 {
        contract_version: WORLD_CONTRACT_VERSION_V4,
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        observation_seq: seq,
        request_seq: seq,
        request_id: format!("req-{seq}"),
        command_seq: None,
        command: None,
        screens: vec![screen("out-1", 0)],
        held_screens: Vec::new(),
        windows,
        held_windows: Vec::new(),
        closed: Vec::new(),
        native: Vec::new(),
        held_native: Vec::new(),
        cursor: None,
        cursor_hit: None,
        focused_global: "win-1".to_owned(),
        foreground_known: true,
        primary_output: Some(OutputId("out-1".to_owned())),
        action_foreground: Some(OutputId("out-1".to_owned())),
        action_cached: None,
        action_outputs: vec![OutputId("out-1".to_owned())],
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

fn engine_with_two_windows() -> WorldEngine {
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("engine");
    let report = report_v4(
        1,
        vec![fact("win-1", "out-1", 0), fact("win-2", "out-1", 400)],
    );
    let (outcome, _) = engine.handle_report(&report, &[]);
    assert!(matches!(outcome, V4RunOutcome::Accepted { .. }));
    engine
}

#[test]
fn send_follows_moved_window_into_target() {
    let mut layout = WorldLayout::new();
    admit(&mut layout, "win-1", "out-1", "ws-a", 120, 80);
    admit(&mut layout, "win-2", "out-1", "ws-a", 120, 80);
    admit(&mut layout, "win-3", "out-1", "ws-a", 80, 120);
    let outcome = send(
        &mut layout,
        &dk("out-1", "ws-a"),
        &dk("out-1", "ws-b"),
        "win-3",
        "win-3",
        true,
        false,
    )
    .expect("move plans");
    assert_eq!(outcome.kind, PairSendKind::Tiled);
    assert_eq!(outcome.window, win("win-3"));
    assert_eq!(outcome.source_workspace.0, "ws-a");
    assert_eq!(outcome.target_workspace.0, "ws-b");
    assert_eq!(
        leaves_of(&layout, "out-1", "ws-a"),
        vec!["leaf-win-1", "leaf-win-2"]
    );
    assert_eq!(leaves_of(&layout, "out-1", "ws-b"), vec!["leaf-win-3"]);
    assert_eq!(
        layout.focus_leaf(&dk("out-1", "ws-b")),
        Some(&NodeId("leaf-win-3".to_owned()))
    );
    assert_eq!(outcome.geometry.len(), 3);

    // Sending the followed window back follows it again even though its
    // now-empty source has no remaining leaf.
    let back = send(
        &mut layout,
        &dk("out-1", "ws-b"),
        &dk("out-1", "ws-a"),
        "win-3",
        "win-3",
        true,
        false,
    )
    .expect("return plans");
    assert_eq!(
        leaves_of(&layout, "out-1", "ws-a"),
        vec!["leaf-win-1", "leaf-win-2", "leaf-win-3"]
    );
    assert!(leaves_of(&layout, "out-1", "ws-b").is_empty());
    assert_eq!(
        layout.focus_leaf(&dk("out-1", "ws-a")),
        Some(&NodeId("leaf-win-3".to_owned()))
    );
    assert_eq!(back.geometry.len(), 3);
}

#[test]
fn occupied_target_splits_remembered_leaf() {
    let mut layout = WorldLayout::new();
    admit(&mut layout, "win-t1", "out-1", "ws-b", 120, 80);
    admit(&mut layout, "win-t2", "out-1", "ws-b", 120, 80);
    admit(&mut layout, "win-1", "out-1", "ws-a", 120, 80);
    admit(&mut layout, "win-2", "out-1", "ws-a", 120, 80);

    let outcome = send(
        &mut layout,
        &dk("out-1", "ws-a"),
        &dk("out-1", "ws-b"),
        "win-2",
        "win-2",
        true,
        false,
    )
    .expect("move plans");
    assert_eq!(
        leaves_of(&layout, "out-1", "ws-b"),
        vec!["leaf-win-t1", "leaf-win-t2", "leaf-win-2"]
    );
    match layout.tree(&dk("out-1", "ws-b")) {
        Some(Node::Group {
            axis,
            children,
            shares,
            ..
        }) => {
            assert_eq!(*axis, Axis::Horizontal);
            assert_eq!(*shares, vec![1, 1]);
            assert!(matches!(&children[0], Node::Leaf { id } if id.0 == "leaf-win-t1"));
            match &children[1] {
                Node::Group {
                    axis,
                    children: inner,
                    shares,
                    ..
                } => {
                    assert_eq!(*axis, Axis::Vertical);
                    assert_eq!(*shares, vec![1, 1]);
                    assert!(matches!(&inner[0], Node::Leaf { id } if id.0 == "leaf-win-t2"));
                    assert!(matches!(&inner[1], Node::Leaf { id } if id.0 == "leaf-win-2"));
                }
                other => panic!("expected nested split, got {other:?}"),
            }
        }
        other => panic!("expected root group, got {other:?}"),
    }
    assert_eq!(leaves_of(&layout, "out-1", "ws-a"), vec!["leaf-win-1"]);
    assert_eq!(
        layout.focus_leaf(&dk("out-1", "ws-b")),
        Some(&NodeId("leaf-win-2".to_owned()))
    );
    assert_eq!(outcome.geometry.len(), 4);
}

#[test]
fn refusals_fail_closed() {
    let mut layout = WorldLayout::new();
    admit(&mut layout, "win-1", "out-1", "ws-a", 120, 80);
    admit(&mut layout, "win-2", "out-1", "ws-b", 120, 80);

    // Focus mismatch: mover does not match observed focus.
    assert_eq!(
        send(
            &mut layout,
            &dk("out-1", "ws-a"),
            &dk("out-1", "ws-b"),
            "win-1",
            "win-2",
            true,
            false
        ),
        Err(PairOperationFailure::Layout(LayoutFailure::Semantics(
            RefusalKind::FocusMismatch
        )))
    );

    // Same-domain, unknown-window, and unknown-target refusals via public WorldEngine.
    let mut engine = engine_with_two_windows();
    let probe_same = report_v4(
        2,
        vec![fact("win-1", "out-1", 0), fact("win-2", "out-1", 400)],
    );
    let (_, settlements_same) = engine.handle_report(
        &probe_same,
        &[(
            1,
            WorldCommand::Send {
                window: win("win-1"),
                target_output: OutputId("out-1".to_owned()),
                target_workspace: "ws-1".to_owned(),
                follow: true,
            },
        )],
    );
    assert!(matches!(
        settlements_same.as_slice(),
        [BatchSettlement {
            result: Some(CommandResult::Rejected {
                kind: CommandRejectKind::OperationRefused
            }),
            ..
        }]
    ));
    let probe_win = report_v4(
        3,
        vec![fact("win-1", "out-1", 0), fact("win-2", "out-1", 400)],
    );
    let (_, settlements_win) = engine.handle_report(
        &probe_win,
        &[(
            2,
            WorldCommand::Send {
                window: win("win-zzz"),
                target_output: OutputId("out-1".to_owned()),
                target_workspace: "ws-1".to_owned(),
                follow: true,
            },
        )],
    );
    assert!(matches!(
        settlements_win.as_slice(),
        [BatchSettlement {
            result: Some(CommandResult::Rejected {
                kind: CommandRejectKind::SendWindowUnknown
            }),
            ..
        }]
    ));

    // Cross-output on workspace send -> CrossDomainMismatch.
    assert_eq!(
        send(
            &mut layout,
            &dk("out-1", "ws-b"),
            &dk("out-2", "ws-a"),
            "win-2",
            "win-2",
            true,
            false
        ),
        Err(PairOperationFailure::Layout(LayoutFailure::Semantics(
            RefusalKind::CrossDomainMismatch
        )))
    );

    // Partial observation: omit known win-1 from observed at the non-converging layout gate.
    assert_eq!(
        layout.toggle_orientation(&dk("out-1", "ws-a"), &win("win-1"), &win("win-1"), &[]),
        Err(LayoutFailure::Semantics(RefusalKind::PartialObservation))
    );

    // Malformed: empty subject.
    assert_eq!(
        send(
            &mut layout,
            &dk("out-1", "ws-b"),
            &dk("out-1", "ws-a"),
            "",
            "win-2",
            true,
            false
        ),
        Err(PairOperationFailure::Layout(LayoutFailure::Semantics(
            RefusalKind::MalformedInput
        )))
    );

    // World-owned unknown-target refusal via public WorldEngine.
    let mut engine = engine_with_two_windows();
    let probe = report_v4(
        2,
        vec![fact("win-1", "out-1", 0), fact("win-2", "out-1", 400)],
    );
    let (_, settlements) = engine.handle_report(
        &probe,
        &[(
            1,
            WorldCommand::Send {
                window: win("win-1"),
                target_output: OutputId("out-1".to_owned()),
                target_workspace: "ws-zzz".to_owned(),
                follow: true,
            },
        )],
    );
    assert!(matches!(
        settlements.as_slice(),
        [BatchSettlement {
            result: Some(CommandResult::Rejected {
                kind: CommandRejectKind::SendTargetUnknown
            }),
            ..
        }]
    ));

    // Ordinary intentional float rides deferred transfer.
    let mut f_flags = ExceptionFlags::none();
    f_flags.floating = true;
    defer_exception(&mut layout, "win-9", "out-1", "ws-a", f_flags);
    let deferred = send(
        &mut layout,
        &dk("out-1", "ws-a"),
        &dk("out-1", "ws-b"),
        "win-9",
        "win-9",
        true,
        false,
    )
    .expect("ordinary float send must plan");
    assert_eq!(deferred.kind, PairSendKind::Deferred);
}

#[test]
fn stay_keeps_source_selected_with_mru_focus() {
    let mut layout = WorldLayout::new();
    admit(&mut layout, "win-1", "out-1", "ws-a", 120, 80);
    admit(&mut layout, "win-2", "out-1", "ws-a", 120, 80);
    admit(&mut layout, "win-3", "out-1", "ws-a", 120, 80);

    let outcome = send(
        &mut layout,
        &dk("out-1", "ws-a"),
        &dk("out-1", "ws-b"),
        "win-3",
        "win-3",
        false,
        false,
    )
    .expect("stay plans");
    assert_eq!(outcome.kind, PairSendKind::Tiled);
    assert!(!outcome.follow);
    assert_eq!(leaves_of(&layout, "out-1", "ws-b"), vec!["leaf-win-3"]);
    assert_eq!(
        leaves_of(&layout, "out-1", "ws-a"),
        vec!["leaf-win-1", "leaf-win-2"]
    );
    assert_eq!(outcome.stay_focus_window, Some(win("win-2")));
    assert_eq!(
        layout.focus_leaf(&dk("out-1", "ws-a")),
        Some(&NodeId("leaf-win-2".to_owned()))
    );
    assert_eq!(outcome.geometry.len(), 3);
}

#[test]
fn stay_of_sole_source_window_leaves_no_focus() {
    let mut layout = WorldLayout::new();
    admit(&mut layout, "win-solo", "out-1", "ws-a", 120, 80);
    let outcome = send(
        &mut layout,
        &dk("out-1", "ws-a"),
        &dk("out-1", "ws-b"),
        "win-solo",
        "win-solo",
        false,
        false,
    )
    .expect("stay plans");
    assert_eq!(leaves_of(&layout, "out-1", "ws-b"), vec!["leaf-win-solo"]);
    assert!(leaves_of(&layout, "out-1", "ws-a").is_empty());
    assert_eq!(outcome.stay_focus_window, None);
    assert_eq!(outcome.focus_leaf, None);
    assert_eq!(layout.focus_leaf(&dk("out-1", "ws-a")), None);
    assert_eq!(outcome.geometry.len(), 1);
}

#[test]
fn follow_and_stay_share_transfer_with_focus_only_difference() {
    let mut l_follow = occupied_trio();
    let mut l_stay = occupied_trio();
    let mut l_remove = occupied_trio();

    let follow = send(
        &mut l_follow,
        &dk("out-1", "ws-a"),
        &dk("out-1", "ws-b"),
        "win-3",
        "win-3",
        true,
        false,
    )
    .expect("follow");
    let stay = send(
        &mut l_stay,
        &dk("out-1", "ws-a"),
        &dk("out-1", "ws-b"),
        "win-3",
        "win-3",
        false,
        false,
    )
    .expect("stay");

    let remove_obs = vec![
        tiled("win-1", "out-1", "ws-a"),
        tiled("win-2", "out-1", "ws-a"),
    ];
    l_remove
        .converge_domain(&dk("out-1", "ws-a"), &remove_obs, None, &BTreeSet::new())
        .expect("remove");

    assert_eq!(
        l_follow.tree(&dk("out-1", "ws-a")),
        l_stay.tree(&dk("out-1", "ws-a"))
    );
    assert_eq!(
        l_follow.tree(&dk("out-1", "ws-b")),
        l_stay.tree(&dk("out-1", "ws-b"))
    );
    assert_eq!(follow.geometry, stay.geometry);
    assert!(follow.follow);
    assert!(!stay.follow);
    assert_eq!(
        l_follow.focus_leaf(&dk("out-1", "ws-b")),
        Some(&NodeId("leaf-win-3".to_owned()))
    );
    assert_eq!(
        l_stay.focus_leaf(&dk("out-1", "ws-a")),
        l_remove.focus_leaf(&dk("out-1", "ws-a"))
    );
    assert_eq!(
        l_stay.focus_leaf(&dk("out-1", "ws-a")),
        Some(&NodeId("leaf-win-2".to_owned()))
    );
}

#[test]
fn post_stay_observation_keeps_native_focus_over_mru() {
    let mut layout = WorldLayout::new();
    admit(&mut layout, "win-1", "out-1", "ws-a", 120, 80);
    admit(&mut layout, "win-2", "out-1", "ws-a", 120, 80);
    admit(&mut layout, "win-3", "out-1", "ws-a", 120, 80);
    send(
        &mut layout,
        &dk("out-1", "ws-a"),
        &dk("out-1", "ws-b"),
        "win-3",
        "win-3",
        false,
        false,
    )
    .expect("stay");
    assert_eq!(
        layout.focus_leaf(&dk("out-1", "ws-a")),
        Some(&NodeId("leaf-win-2".to_owned()))
    );

    let obs = domain_observed(&layout, &dk("out-1", "ws-a"));
    let counts = layout
        .converge_domain(
            &dk("out-1", "ws-a"),
            &obs,
            Some(&win("win-1")),
            &BTreeSet::new(),
        )
        .expect("converges");
    assert_eq!(counts.removed, 0);
    assert_eq!(counts.admitted, 0);
    assert_eq!(counts.flags_adopted, 0);
    assert_eq!(
        layout.focus_leaf(&dk("out-1", "ws-a")),
        Some(&NodeId("leaf-win-1".to_owned()))
    );
}

#[test]
fn r1_r4_untouched() {
    const RULES: [Rule; 6] = [
        Rule::R1,
        Rule::R2a,
        Rule::R2b,
        Rule::R2c,
        Rule::R3,
        Rule::R4,
    ];
    assert_eq!(RULES.len(), 6);
    assert_ne!(Capability::SwapNeighbor, Capability::CrossOutputTransfer);
}

#[test]
fn output_send_follow_moves_to_destination_current_workspace() {
    let mut layout = WorldLayout::new();
    admit(&mut layout, "win-1", "out-1", "ws-a", 120, 80);
    admit(&mut layout, "win-2", "out-1", "ws-a", 120, 80);
    admit(&mut layout, "win-t", "out-2", "ws-a", 120, 80);

    let outcome = send(
        &mut layout,
        &dk("out-1", "ws-a"),
        &dk("out-2", "ws-a"),
        "win-2",
        "win-2",
        true,
        true,
    )
    .expect("output send plans");
    assert_eq!(outcome.kind, PairSendKind::Tiled);
    assert_eq!(outcome.source_output.0, "out-1");
    assert_eq!(outcome.target_output.0, "out-2");
    assert_eq!(
        leaves_of(&layout, "out-1", "ws-a"),
        vec!["leaf-win-1".to_string()]
    );
    assert!(leaves_of(&layout, "out-2", "ws-a").contains(&"leaf-win-2".to_string()));
    assert_eq!(
        layout.focus_leaf(&dk("out-2", "ws-a")),
        Some(&NodeId("leaf-win-2".to_owned()))
    );
}

#[test]
fn output_send_stay_keeps_source_mru_without_selecting_target() {
    fn trio() -> WorldLayout {
        let mut layout = WorldLayout::new();
        admit(&mut layout, "win-1", "out-1", "ws-a", 120, 80);
        admit(&mut layout, "win-2", "out-1", "ws-a", 120, 80);
        admit(&mut layout, "win-3", "out-1", "ws-a", 120, 80);
        admit(&mut layout, "win-t", "out-2", "ws-a", 120, 80);
        layout
    }
    let mut l_follow = trio();
    let follow_outcome = send(
        &mut l_follow,
        &dk("out-1", "ws-a"),
        &dk("out-2", "ws-a"),
        "win-3",
        "win-3",
        true,
        true,
    )
    .expect("follow");
    let mut l_stay = trio();
    let stay_outcome = send(
        &mut l_stay,
        &dk("out-1", "ws-a"),
        &dk("out-2", "ws-a"),
        "win-3",
        "win-3",
        false,
        true,
    )
    .expect("stay");

    assert_eq!(
        leaves_of(&l_stay, "out-2", "ws-a"),
        leaves_of(&l_follow, "out-2", "ws-a")
    );
    assert_eq!(stay_outcome.geometry, follow_outcome.geometry);
    assert!(!stay_outcome.follow);
    assert_eq!(stay_outcome.stay_focus_window, Some(win("win-2")));
    assert_eq!(
        l_stay.focus_leaf(&dk("out-1", "ws-a")),
        Some(&NodeId("leaf-win-2".to_owned()))
    );
    assert!(leaves_of(&l_stay, "out-2", "ws-a").contains(&"leaf-win-3".to_string()));
}

#[test]
fn output_send_admits_at_remembered_leaf() {
    let mut layout = WorldLayout::new();
    admit(&mut layout, "win-t1", "out-2", "ws-a", 120, 80);
    admit(&mut layout, "win-t2", "out-2", "ws-a", 120, 80);
    admit(&mut layout, "win-1", "out-1", "ws-a", 120, 80);
    admit(&mut layout, "win-2", "out-1", "ws-a", 120, 80);

    send(
        &mut layout,
        &dk("out-1", "ws-a"),
        &dk("out-2", "ws-a"),
        "win-2",
        "win-2",
        true,
        true,
    )
    .expect("output send plans");
    let target = layout.tree(&dk("out-2", "ws-a")).expect("target");
    assert_eq!(flat(Some(target)).len(), 3);
    let mover_parent = parent_of(target, "leaf-win-2").expect("mover parent");
    assert_eq!(
        parent_of(target, "leaf-win-t2"),
        Some(mover_parent),
        "mover splits beside the remembered leaf"
    );
}

#[test]
fn output_send_scope_refusals_keep_ops_distinct() {
    let mut layout = WorldLayout::new();
    admit(&mut layout, "win-1", "out-1", "ws-a", 120, 80);
    admit(&mut layout, "win-2", "out-1", "ws-a", 120, 80);

    // Same-output distinct workspace belongs to workspace send.
    assert_eq!(
        send(
            &mut layout,
            &dk("out-1", "ws-a"),
            &dk("out-1", "ws-b"),
            "win-2",
            "win-2",
            true,
            true
        ),
        Err(PairOperationFailure::Layout(LayoutFailure::Semantics(
            RefusalKind::CrossDomainMismatch
        )))
    );
    // Same-domain workspace send vs directional output send keep ops distinct on WorldEngine.
    let mut engine = engine_with_two_windows();
    let probe_same = report_v4(
        2,
        vec![fact("win-1", "out-1", 0), fact("win-2", "out-1", 400)],
    );
    let (_, settlements_same) = engine.handle_report(
        &probe_same,
        &[(
            1,
            WorldCommand::Send {
                window: win("win-1"),
                target_output: OutputId("out-1".to_owned()),
                target_workspace: "ws-1".to_owned(),
                follow: true,
            },
        )],
    );
    assert!(matches!(
        settlements_same.as_slice(),
        [BatchSettlement {
            result: Some(CommandResult::Rejected {
                kind: CommandRejectKind::OperationRefused
            }),
            ..
        }]
    ));
    let probe_dir = report_v4(
        3,
        vec![fact("win-1", "out-1", 0), fact("win-2", "out-1", 400)],
    );
    let (_, settlements_dir) = engine.handle_report(
        &probe_dir,
        &[(
            2,
            WorldCommand::SendDirectional {
                window: win("win-1"),
                direction: crate::directional::Direction::Right,
                follow: true,
            },
        )],
    );
    assert!(matches!(
        settlements_dir.as_slice(),
        [BatchSettlement {
            result: Some(CommandResult::Rejected {
                kind: CommandRejectKind::SendTargetUnknown
            }),
            ..
        }]
    ));
    // Workspace send refuses cross-output targets.
    assert_eq!(
        send(
            &mut layout,
            &dk("out-1", "ws-a"),
            &dk("out-2", "ws-a"),
            "win-2",
            "win-2",
            true,
            false
        ),
        Err(PairOperationFailure::Layout(LayoutFailure::Semantics(
            RefusalKind::CrossDomainMismatch
        )))
    );

    // Unknown output target refuses through public WorldEngine.
    let probe = report_v4(
        4,
        vec![fact("win-1", "out-1", 0), fact("win-2", "out-1", 400)],
    );
    let (_, settlements) = engine.handle_report(
        &probe,
        &[(
            3,
            WorldCommand::Send {
                window: win("win-1"),
                target_output: OutputId("out-9".to_owned()),
                target_workspace: "ws-z".to_owned(),
                follow: true,
            },
        )],
    );
    assert!(matches!(
        settlements.as_slice(),
        [BatchSettlement {
            result: Some(CommandResult::Rejected {
                kind: CommandRejectKind::SendTargetUnknown
            }),
            ..
        }]
    ));
}

#[test]
fn output_send_refuses_sticky_but_carries_intentional_floats() {
    // Sticky mover refuses: floating+sticky exception stays out of leaves during
    // converge_pair and fails deferred_send_eligible -> FocusMismatch.
    let mut s_layout = WorldLayout::new();
    admit(&mut s_layout, "win-1", "out-1", "ws-a", 120, 80);
    let mut s_flags = ExceptionFlags::none();
    s_flags.floating = true;
    s_flags.sticky = true;
    defer_exception(&mut s_layout, "win-f", "out-1", "ws-a", s_flags);
    assert_eq!(
        send(
            &mut s_layout,
            &dk("out-1", "ws-a"),
            &dk("out-2", "ws-a"),
            "win-f",
            "win-f",
            true,
            true
        ),
        Err(PairOperationFailure::Layout(LayoutFailure::Semantics(
            RefusalKind::FocusMismatch
        ))),
        "sticky movers stay ineligible"
    );

    // Sticky window also refuses directly at the public WorldEngine boundary.
    let mut engine = engine_with_two_windows();
    let mut sticky_fact = fact("win-s", "out-1", 100);
    sticky_fact.sticky = Some(true);
    let probe_sticky = report_v4(
        2,
        vec![
            fact("win-1", "out-1", 0),
            fact("win-2", "out-1", 400),
            sticky_fact,
        ],
    );
    let (_, settlements_sticky) = engine.handle_report(
        &probe_sticky,
        &[(
            1,
            WorldCommand::Send {
                window: win("win-s"),
                target_output: OutputId("out-1".to_owned()),
                target_workspace: "ws-2".to_owned(),
                follow: true,
            },
        )],
    );
    assert!(matches!(
        settlements_sticky.as_slice(),
        [BatchSettlement {
            result: Some(CommandResult::Rejected {
                kind: CommandRejectKind::SendRefused
            }),
            ..
        }]
    ));

    // Ordinary intentional float rides deferred transfer.
    let mut f_layout = WorldLayout::new();
    admit(&mut f_layout, "win-1", "out-1", "ws-a", 120, 80);
    let mut f_flags = ExceptionFlags::none();
    f_flags.floating = true;
    defer_exception(&mut f_layout, "win-f", "out-1", "ws-a", f_flags);
    let outcome = send(
        &mut f_layout,
        &dk("out-1", "ws-a"),
        &dk("out-2", "ws-a"),
        "win-f",
        "win-f",
        true,
        true,
    )
    .expect("ordinary float output send plans");
    assert_eq!(outcome.kind, PairSendKind::Deferred);
    assert!(f_layout.is_exception(&dk("out-2", "ws-a"), &win("win-f")));
    assert!(
        f_layout
            .exception_flags(&dk("out-2", "ws-a"), &win("win-f"))
            .expect("flags")
            .floating
    );
}

#[test]
fn output_send_uses_focus_history_when_remembered_stale() {
    let mut layout = WorldLayout::new();
    admit(&mut layout, "win-a1", "out-2", "ws-a", 120, 80);
    admit(&mut layout, "win-a2", "out-2", "ws-a", 120, 80);
    admit(&mut layout, "win-a3", "out-2", "ws-a", 120, 80);
    admit(&mut layout, "win-b1", "out-1", "ws-b", 120, 80);
    send(
        &mut layout,
        &dk("out-2", "ws-a"),
        &dk("out-1", "ws-b"),
        "win-a3",
        "win-a3",
        true,
        true,
    )
    .expect("first send");
    assert_eq!(leaves_of(&layout, "out-2", "ws-a").len(), 2);

    admit(&mut layout, "win-c1", "out-1", "ws-a", 120, 80);
    admit(&mut layout, "win-c2", "out-1", "ws-a", 120, 80);
    send(
        &mut layout,
        &dk("out-1", "ws-a"),
        &dk("out-2", "ws-a"),
        "win-c2",
        "win-c2",
        true,
        true,
    )
    .expect("second send");

    let target = layout.tree(&dk("out-2", "ws-a")).expect("target");
    assert_eq!(flat(Some(target)).len(), 3);
    let mover_parent = parent_of(target, "leaf-win-c2").expect("mover parent");
    assert_eq!(
        parent_of(target, "leaf-win-a2"),
        Some(mover_parent),
        "mover splits beside the focus-history leaf, not a root wrap"
    );
    assert_eq!(
        layout.focus_leaf(&dk("out-2", "ws-a")),
        Some(&NodeId("leaf-win-c2".to_owned()))
    );
    assert_eq!(
        leaves_of(&layout, "out-1", "ws-a"),
        vec!["leaf-win-c1".to_string()]
    );
    let mut ws_b = leaves_of(&layout, "out-1", "ws-b");
    ws_b.sort();
    assert_eq!(
        ws_b,
        vec!["leaf-win-a3".to_string(), "leaf-win-b1".to_string()]
    );
}

#[test]
fn output_send_empty_destination_admits_lone_root_follow_and_stay() {
    let mut l_follow = WorldLayout::new();
    admit(&mut l_follow, "win-1", "out-1", "ws-a", 120, 80);
    admit(&mut l_follow, "win-2", "out-1", "ws-a", 120, 80);
    send(
        &mut l_follow,
        &dk("out-1", "ws-a"),
        &dk("out-2", "ws-a"),
        "win-2",
        "win-2",
        true,
        true,
    )
    .expect("follow");
    assert_eq!(
        leaves_of(&l_follow, "out-2", "ws-a"),
        vec!["leaf-win-2".to_string()]
    );
    assert_eq!(
        l_follow.focus_leaf(&dk("out-2", "ws-a")),
        Some(&NodeId("leaf-win-2".to_owned()))
    );
    assert_eq!(
        leaves_of(&l_follow, "out-1", "ws-a"),
        vec!["leaf-win-1".to_string()]
    );
    assert!(leaves_of(&l_follow, "out-1", "ws-b").is_empty());

    let mut l_stay = WorldLayout::new();
    admit(&mut l_stay, "solo", "out-1", "ws-a", 120, 80);
    let stay_outcome = send(
        &mut l_stay,
        &dk("out-1", "ws-a"),
        &dk("out-2", "ws-a"),
        "solo",
        "solo",
        false,
        true,
    )
    .expect("stay");
    assert_eq!(
        leaves_of(&l_stay, "out-2", "ws-a"),
        vec!["leaf-solo".to_string()]
    );
    assert_eq!(stay_outcome.focus_leaf, None);
    assert_eq!(stay_outcome.stay_focus_window, None);
    assert!(leaves_of(&l_stay, "out-1", "ws-a").is_empty());
    assert_eq!(l_stay.focus_leaf(&dk("out-1", "ws-a")), None);
}

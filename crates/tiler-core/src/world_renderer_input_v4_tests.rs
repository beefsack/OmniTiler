//! Core-internal v4 renderer-input validator regressions.
//!
//! [`WorldEngine::validate_renderer_input_v4`] checks a staged v4 renderer
//! target against current core identity/home state only: workspace iff the
//! id currently belongs on the output, window iff the exact lifetime is
//! homed there, group iff the exact group id exists in the live session
//! tree. Revision newer than the latest published presentation refuses;
//! older/equal proceeds to the row check. No ordinal, trailing, binding,
//! label, occupancy, or member comparison, and no rendered-row history.
//!
//! Revision evidence is validator-level only (no v4 runtime claim): every
//! `latest` below is the actual latest published presentation revision
//! read off the engine's `last_reply.presentation.rev`, and every older
//! rev is an actually published earlier revision from the same seed.
//! Same-id legs assert the actual before/after row state (pager
//! ordinal/label/occupied/trailing flags, retained native binding,
//! exact domain member sets), never just unrelated payload. Direct
//! retained-state setups (ring reorder, native binding) are labeled as
//! such; everything else runs the production accepted-report path.

use std::collections::BTreeSet;

use crate::directional::{Node, NodeId, OutputId, WindowId, WorkspaceId};
use crate::geometry::Rect;
use crate::session::DomainKey;
use crate::size_hints::WindowSizeHints;
use crate::world_engine::{LegacyWorldOutcome, WorldEngine};
use crate::world_report::{
    CommandRejectKind, RenderTarget, RendererInput, ScreenReport, WORLD_CONTRACT_VERSION_LEGACY,
    WindowFact, WorldCommand, WorldReport,
};

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

#[allow(clippy::too_many_arguments)]
fn report(
    generation: &str,
    seq: u64,
    screens: Vec<ScreenReport>,
    windows: Vec<WindowFact>,
    closed: Vec<WindowId>,
    focused: &str,
    command_seq: Option<u64>,
    command: Option<WorldCommand>,
) -> WorldReport {
    WorldReport {
        contract_version: WORLD_CONTRACT_VERSION_LEGACY,
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: generation.to_owned(),
        observation_seq: seq,
        request_seq: seq,
        request_id: format!("req-{seq}"),
        command_seq,
        command,
        screens,
        windows,
        views: Vec::new(),
        closed,
        native: Vec::new(),
        cursor: None,
        cursor_hit: None,
        focused_global: focused.to_owned(),
        action_foreground: None,
        action_cached: None,
        action_outputs: Vec::new(),
        primary_output: None,
        held_screens: Vec::new(),
        held_windows: Vec::new(),
        inventory_complete: true,
        gesture_barrier: false,
        gesture: None,
        monotonic_ms: 0,
        frames: Vec::new(),
        member_attempts: Vec::new(),
        view_attempts: Vec::new(),
        clear_attempts: Vec::new(),
        creation_attempts: Vec::new(),
        state_attempts: Vec::new(),
        focus_feedback: Vec::new(),
        presentation: crate::world_report::PresentationIngress::disabled(),
        intent_records: Vec::new(),
    }
}

fn accepted(engine: &mut WorldEngine, report: &WorldReport) -> crate::world_report::WorldReply {
    match engine.handle_report_legacy(report) {
        LegacyWorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted, got {other:?}"),
    }
}

/// Actual latest published presentation revision on the engine.
fn latest_published(engine: &WorldEngine) -> u64 {
    engine
        .last_reply
        .as_ref()
        .expect("seed published a reply")
        .presentation
        .rev
}

fn screens_pair() -> Vec<ScreenReport> {
    vec![screen("out-a", 0), screen("out-b", 800)]
}

fn facts_pair() -> Vec<WindowFact> {
    vec![
        fact("win-a", "out-a", 0),
        fact("win-b", "out-a", 100),
        fact("win-c", "out-b", 800),
    ]
}

/// Seed one engine on `generation` through the accepted legacy path and
/// return it with the live logical views plus the two actually published
/// presentation revisions (`rev_old` from the first reply, `rev_new` from
/// the second; `rev_old < rev_new`). Both out-a windows home to `view_a`,
/// so out-a carries a live Engine group.
fn seed_pair(generation: &str) -> (WorldEngine, String, String, u64, u64) {
    let mut engine =
        WorldEngine::new_test_bound(generation, "adapter-1").expect("valid generation");
    let first = accepted(
        &mut engine,
        &report(
            generation,
            1,
            screens_pair(),
            facts_pair(),
            Vec::new(),
            "win-a",
            None,
            None,
        ),
    );
    let second = accepted(
        &mut engine,
        &report(
            generation,
            2,
            screens_pair(),
            facts_pair(),
            Vec::new(),
            "win-a",
            None,
            None,
        ),
    );
    let rev_old = first.presentation.rev;
    let rev_new = second.presentation.rev;
    assert!(
        rev_old < rev_new,
        "seed publishes advancing revisions: {rev_old} then {rev_new}"
    );
    let view_a = first
        .outputs
        .iter()
        .find(|row| row.output.0 == "out-a")
        .and_then(|row| row.logical_view.clone())
        .expect("view a");
    let view_b = first
        .outputs
        .iter()
        .find(|row| row.output.0 == "out-b")
        .and_then(|row| row.logical_view.clone())
        .expect("view b");
    assert_eq!(
        engine.core_homing(&WindowId("win-a".to_owned())),
        Some(DomainKey {
            output: OutputId("out-a".to_owned()),
            workspace: WorkspaceId(view_a.clone()),
        }),
        "seed homes win-a on out-a/view_a"
    );
    (engine, view_a, view_b, rev_old, rev_new)
}

/// Live group ids in one domain's committed Engine session tree.
fn group_ids(engine: &WorldEngine, output: &str, workspace: &str) -> Vec<NodeId> {
    let key = domain_key(output, workspace);
    let Some(session) = engine.engine.session(&key) else {
        return Vec::new();
    };
    let Some(tree) = session.tree_for(&key) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    collect_groups(tree, &mut out);
    out
}

fn domain_key(output: &str, workspace: &str) -> DomainKey {
    DomainKey {
        output: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
    }
}

fn collect_groups(tree: &Node, out: &mut Vec<NodeId>) {
    if let Node::Group { id, children, .. } = tree {
        out.push(id.clone());
        for child in children {
            collect_groups(child, out);
        }
    }
}

/// Exact member lifetime set committed in one domain's Engine session.
fn domain_members(engine: &WorldEngine, output: &str, workspace: &str) -> BTreeSet<WindowId> {
    let key = domain_key(output, workspace);
    let Some(session) = engine.engine.session(&key) else {
        return BTreeSet::new();
    };
    session
        .snapshot()
        .windows
        .into_iter()
        .filter(|link| link.output == key.output && link.workspace == key.workspace)
        .map(|link| link.window)
        .collect()
}

/// Pager workspace row for one id on one output, from an accepted reply.
fn pager_row<'a>(
    reply: &'a crate::world_report::WorldReply,
    output: &str,
    id: &str,
) -> &'a crate::world_presentation::PagerWorkspace {
    reply
        .presentation
        .pager
        .iter()
        .find(|screen| screen.output == output)
        .unwrap_or_else(|| panic!("pager screen {output}"))
        .workspaces
        .iter()
        .find(|ws| ws.id == id)
        .unwrap_or_else(|| panic!("pager row {id}"))
}

fn ws_input(rev: u64, output: &str, workspace: &str) -> RendererInput {
    RendererInput {
        rev,
        target: RenderTarget::Workspace {
            out: OutputId(output.to_owned()),
            ws: WorkspaceId(workspace.to_owned()),
        },
    }
}

fn win_input(rev: u64, output: &str, workspace: &str, window: &str) -> RendererInput {
    RendererInput {
        rev,
        target: RenderTarget::Window {
            out: OutputId(output.to_owned()),
            ws: WorkspaceId(workspace.to_owned()),
            win: WindowId(window.to_owned()),
        },
    }
}

fn grp_input(rev: u64, output: &str, workspace: &str, group: &NodeId) -> RendererInput {
    RendererInput {
        rev,
        target: RenderTarget::Group {
            out: OutputId(output.to_owned()),
            ws: WorkspaceId(workspace.to_owned()),
            group: group.clone(),
        },
    }
}

fn check(
    engine: &WorldEngine,
    input: Option<&RendererInput>,
    latest: u64,
) -> Option<CommandRejectKind> {
    engine.validate_renderer_input_v4(input, latest)
}

#[test]
fn none_input_is_valid_for_ordinary_command() {
    let (engine, _, _, _, rev_new) = seed_pair("gen-1");
    assert_eq!(latest_published(&engine), rev_new);
    assert_eq!(check(&engine, None, rev_new), None);
}

#[test]
fn older_and_equal_published_revision_accepted() {
    let (engine, view_a, _, rev_old, rev_new) = seed_pair("gen-1");
    assert_eq!(latest_published(&engine), rev_new);
    let groups = group_ids(&engine, "out-a", &view_a);
    assert!(!groups.is_empty(), "seeded domain carries a group");
    // The older revision is genuinely published (first seed reply), not
    // synthetic; the equal revision is the actual latest published.
    for rev in [rev_old, rev_new] {
        assert_eq!(
            check(&engine, Some(&ws_input(rev, "out-a", &view_a)), rev_new),
            None
        );
        assert_eq!(
            check(
                &engine,
                Some(&win_input(rev, "out-a", &view_a, "win-a")),
                rev_new
            ),
            None
        );
        assert_eq!(
            check(
                &engine,
                Some(&grp_input(rev, "out-a", &view_a, &groups[0])),
                rev_new
            ),
            None
        );
    }
}

#[test]
fn future_revision_refuses_even_valid_target() {
    let (engine, view_a, _, _, rev_new) = seed_pair("gen-1");
    let future = rev_new + 1;
    let groups = group_ids(&engine, "out-a", &view_a);
    assert!(!groups.is_empty(), "seeded domain carries a group");
    assert_eq!(
        check(&engine, Some(&ws_input(future, "out-a", &view_a)), rev_new),
        Some(CommandRejectKind::StaleRenderTarget)
    );
    assert_eq!(
        check(
            &engine,
            Some(&win_input(future, "out-a", &view_a, "win-a")),
            rev_new
        ),
        Some(CommandRejectKind::StaleRenderTarget)
    );
    assert_eq!(
        check(
            &engine,
            Some(&grp_input(future, "out-a", &view_a, &groups[0])),
            rev_new
        ),
        Some(CommandRejectKind::StaleRenderTarget)
    );
}

#[test]
fn removed_window_refused() {
    let (mut engine, view_a, _, rev_old, _) = seed_pair("gen-1");
    accepted(
        &mut engine,
        &report(
            "gen-1",
            3,
            screens_pair(),
            vec![fact("win-b", "out-a", 100), fact("win-c", "out-b", 800)],
            vec![WindowId("win-a".to_owned())],
            "win-b",
            None,
            None,
        ),
    );
    let latest = latest_published(&engine);
    assert_eq!(
        check(
            &engine,
            Some(&win_input(rev_old, "out-a", &view_a, "win-a")),
            latest
        ),
        Some(CommandRejectKind::StaleRenderTarget)
    );
    assert_eq!(
        check(
            &engine,
            Some(&win_input(rev_old, "out-a", &view_a, "win-b")),
            latest
        ),
        None
    );
    assert_eq!(
        check(&engine, Some(&ws_input(rev_old, "out-a", &view_a)), latest),
        None
    );
}

#[test]
fn window_moved_workspace_refused() {
    let (mut engine, view_a, _, rev_old, _) = seed_pair("gen-1");
    let ring = engine.mapping.get("out-a").cloned().expect("ring a");
    assert!(ring.iter().any(|id| id == &view_a));
    let target_ws = ring
        .iter()
        .find(|id| *id != &view_a)
        .expect("second ws")
        .clone();
    accepted(
        &mut engine,
        &report(
            "gen-1",
            3,
            screens_pair(),
            facts_pair(),
            Vec::new(),
            "win-a",
            Some(11),
            Some(WorldCommand::Send {
                window: WindowId("win-a".to_owned()),
                target_output: OutputId("out-a".to_owned()),
                target_workspace: target_ws.clone(),
                follow: false,
            }),
        ),
    );
    let latest = latest_published(&engine);
    assert_eq!(
        check(
            &engine,
            Some(&win_input(rev_old, "out-a", &view_a, "win-a")),
            latest
        ),
        Some(CommandRejectKind::StaleRenderTarget)
    );
    assert_eq!(
        check(
            &engine,
            Some(&win_input(rev_old, "out-a", &target_ws, "win-a")),
            latest
        ),
        None
    );
}

#[test]
fn window_moved_output_refused() {
    let (mut engine, view_a, view_b, rev_old, _) = seed_pair("gen-1");
    let mut moved = facts_pair();
    moved[0] = fact("win-a", "out-b", 800);
    accepted(
        &mut engine,
        &report(
            "gen-1",
            3,
            screens_pair(),
            moved,
            Vec::new(),
            "win-a",
            None,
            None,
        ),
    );
    assert_eq!(
        engine.core_homing(&WindowId("win-a".to_owned())),
        Some(domain_key("out-b", &view_b)),
        "observed cross-output move rehomes to the out-b view"
    );
    let latest = latest_published(&engine);
    // Stale row, stale workspace at the new output, and stale output at the
    // new workspace each refuse; only the exact new row accepts.
    assert_eq!(
        check(
            &engine,
            Some(&win_input(rev_old, "out-a", &view_a, "win-a")),
            latest
        ),
        Some(CommandRejectKind::StaleRenderTarget)
    );
    assert_eq!(
        check(
            &engine,
            Some(&win_input(rev_old, "out-b", &view_a, "win-a")),
            latest
        ),
        Some(CommandRejectKind::StaleRenderTarget)
    );
    assert_eq!(
        check(
            &engine,
            Some(&win_input(rev_old, "out-a", &view_b, "win-a")),
            latest
        ),
        Some(CommandRejectKind::StaleRenderTarget)
    );
    assert_eq!(
        check(
            &engine,
            Some(&win_input(rev_old, "out-b", &view_b, "win-a")),
            latest
        ),
        None
    );
}

#[test]
fn workspace_moved_output_refused() {
    let (mut engine, view_a, _, rev_old, _) = seed_pair("gen-1");
    accepted(
        &mut engine,
        &report(
            "gen-1",
            3,
            screens_pair(),
            facts_pair(),
            Vec::new(),
            "win-a",
            Some(11),
            Some(WorldCommand::SetWorkspaceTiling {
                output: OutputId("out-a".to_owned()),
                workspace: view_a.clone(),
                tiled: false,
            }),
        ),
    );
    accepted(
        &mut engine,
        &report(
            "gen-1",
            4,
            screens_pair(),
            facts_pair(),
            Vec::new(),
            "win-a",
            Some(12),
            Some(WorldCommand::MigrateWorkspace {
                source_output: OutputId("out-a".to_owned()),
                target_output: OutputId("out-b".to_owned()),
                workspace: view_a.clone(),
            }),
        ),
    );
    assert!(
        !engine
            .mapping
            .get("out-a")
            .is_some_and(|ring| ring.iter().any(|id| id == &view_a)),
        "migration drops the id from the source ring"
    );
    let latest = latest_published(&engine);
    assert_eq!(
        check(&engine, Some(&ws_input(rev_old, "out-a", &view_a)), latest),
        Some(CommandRejectKind::StaleRenderTarget)
    );
    // The id lands at a different ring position on out-b, yet the same-id
    // target accepts: no ordinal/label comparison.
    assert_eq!(
        check(&engine, Some(&ws_input(rev_old, "out-b", &view_a)), latest),
        None
    );
    assert_eq!(
        check(
            &engine,
            Some(&win_input(rev_old, "out-a", &view_a, "win-a")),
            latest
        ),
        Some(CommandRejectKind::StaleRenderTarget)
    );
    assert_eq!(
        check(
            &engine,
            Some(&win_input(rev_old, "out-b", &view_a, "win-a")),
            latest
        ),
        None
    );
}

#[test]
fn removed_group_refused() {
    let (mut engine, view_a, _, rev_old, _) = seed_pair("gen-1");
    let groups = group_ids(&engine, "out-a", &view_a);
    assert!(!groups.is_empty(), "seeded domain carries a group");
    accepted(
        &mut engine,
        &report(
            "gen-1",
            3,
            screens_pair(),
            vec![fact("win-a", "out-a", 0), fact("win-c", "out-b", 800)],
            vec![WindowId("win-b".to_owned())],
            "win-a",
            None,
            None,
        ),
    );
    assert!(
        group_ids(&engine, "out-a", &view_a).is_empty(),
        "collapsed tree carries no group"
    );
    let latest = latest_published(&engine);
    assert_eq!(
        check(
            &engine,
            Some(&grp_input(rev_old, "out-a", &view_a, &groups[0])),
            latest
        ),
        Some(CommandRejectKind::StaleRenderTarget)
    );
    assert_eq!(
        check(
            &engine,
            Some(&win_input(rev_old, "out-a", &view_a, "win-a")),
            latest
        ),
        None
    );
    assert_eq!(
        check(&engine, Some(&ws_input(rev_old, "out-a", &view_a)), latest),
        None
    );
}

#[test]
fn stale_group_refuses_despite_later_group() {
    let (mut engine, view_a, _, rev_old, _) = seed_pair("gen-1");
    let old_groups = group_ids(&engine, "out-a", &view_a);
    assert!(!old_groups.is_empty(), "seeded domain carries a group");
    accepted(
        &mut engine,
        &report(
            "gen-1",
            3,
            screens_pair(),
            vec![fact("win-a", "out-a", 0), fact("win-c", "out-b", 800)],
            vec![WindowId("win-b".to_owned())],
            "win-a",
            None,
            None,
        ),
    );
    // Actual production mint path: two fresh lifetimes admitted to the same
    // workspace mint the next monotonic serial, never the removed id.
    accepted(
        &mut engine,
        &report(
            "gen-1",
            4,
            screens_pair(),
            vec![
                fact("win-a", "out-a", 0),
                fact("win-d", "out-a", 100),
                fact("win-e", "out-a", 200),
                fact("win-c", "out-b", 800),
            ],
            Vec::new(),
            "win-a",
            None,
            None,
        ),
    );
    let new_groups = group_ids(&engine, "out-a", &view_a);
    assert!(!new_groups.is_empty(), "re-formed domain carries a group");
    assert!(
        !new_groups.contains(&old_groups[0]),
        "removed id never reissued: {:?} vs {:?}",
        old_groups,
        new_groups
    );
    let latest = latest_published(&engine);
    assert_eq!(
        check(
            &engine,
            Some(&grp_input(rev_old, "out-a", &view_a, &old_groups[0])),
            latest
        ),
        Some(CommandRejectKind::StaleRenderTarget)
    );
    assert_eq!(
        check(
            &engine,
            Some(&grp_input(rev_old, "out-a", &view_a, &new_groups[0])),
            latest
        ),
        None
    );
}

#[test]
fn ring_reorder_changes_ordinal_and_label_but_accepts_same_workspace() {
    let (mut engine, view_a, _, rev_old, _) = seed_pair("gen-1");
    let ring = engine.mapping.get("out-a").cloned().expect("ring a");
    assert_eq!(ring.first(), Some(&view_a), "seed view heads the ring");
    // Occupy the spare through the production send path so the reorder
    // below swaps two live workspaces (no lifecycle spare involved).
    let spare = ring
        .iter()
        .find(|id| *id != &view_a)
        .expect("spare ws")
        .clone();
    let grown = accepted(
        &mut engine,
        &report(
            "gen-1",
            3,
            screens_pair(),
            facts_pair(),
            Vec::new(),
            "win-a",
            Some(11),
            Some(WorldCommand::Send {
                window: WindowId("win-b".to_owned()),
                target_output: OutputId("out-a".to_owned()),
                target_workspace: spare.clone(),
                follow: false,
            }),
        ),
    );
    let before = pager_row(&grown, "out-a", &view_a);
    let (ordinal_before, label_before) = (before.ordinal, before.label.clone());
    assert_eq!(
        (ordinal_before, label_before.as_str()),
        (1, "Workspace 1"),
        "seed view paints first"
    );
    // Direct retained-ring reorder (labeled setup, not a production op):
    // swap the two live heads; any lifecycle spare stays last.
    let live = engine.mapping.get("out-a").cloned().expect("ring a");
    assert!(live.len() >= 2, "ring holds two live rows: {live:?}");
    let mut reordered = live.clone();
    reordered.swap(0, 1);
    engine.mapping.insert("out-a".to_owned(), reordered);
    let republished = accepted(
        &mut engine,
        &report(
            "gen-1",
            4,
            screens_pair(),
            facts_pair(),
            Vec::new(),
            "win-a",
            None,
            None,
        ),
    );
    let ring_after = engine.mapping.get("out-a").cloned().expect("ring a");
    assert_eq!(
        (&ring_after[0], &ring_after[1]),
        (&live[1], &live[0]),
        "converge preserves the swapped live heads"
    );
    let after = pager_row(&republished, "out-a", &view_a);
    assert_ne!(after.ordinal, ordinal_before, "ordinal actually changed");
    assert_eq!(
        after.label,
        crate::world_presentation::pager_workspace_label(after.ordinal),
        "label derives from the new ordinal"
    );
    assert_ne!(after.label, label_before, "label actually changed");
    let latest = latest_published(&engine);
    assert_eq!(
        check(&engine, Some(&ws_input(rev_old, "out-a", &view_a)), latest),
        None,
        "same workspace id accepts despite ordinal/label change"
    );
}

#[test]
fn trailing_and_occupied_change_accepts_same_workspace() {
    let (mut engine, view_a, _, rev_old, _) = seed_pair("gen-1");
    let ring = engine.mapping.get("out-a").cloned().expect("ring a");
    let spare = ring
        .iter()
        .find(|id| *id != &view_a)
        .expect("spare ws")
        .clone();
    // Verified before state: the spare is the marked trailing row,
    // unoccupied in both the pager and the membership owner.
    let baseline = accepted(
        &mut engine,
        &report(
            "gen-1",
            3,
            screens_pair(),
            facts_pair(),
            Vec::new(),
            "win-a",
            None,
            None,
        ),
    );
    assert_eq!(
        engine.trailing_marked_id("out-a"),
        Some(spare.clone()),
        "spare is the marked trailing id"
    );
    let spare_row = pager_row(&baseline, "out-a", &spare);
    assert!(spare_row.trailing, "pager marks the spare trailing");
    assert!(!spare_row.occupied, "pager shows the spare unoccupied");
    assert!(
        engine.logical_members("out-a", &spare).is_empty(),
        "membership owner holds no spare members"
    );
    assert_eq!(
        check(&engine, Some(&ws_input(rev_old, "out-a", &spare)), rev_old),
        None
    );
    // Direct staged-homing setup (labeled setup, not a production op):
    // the explicit same-output Send of win-b into the spare refuses here
    // (`OperationRefused`), and transfer settlement needs adapter arrival
    // proofs beyond this validator's scope. Stage the arrival directly in
    // retained staging: win-b's staged fact already exists and is
    // non-sticky, so pointing its staged homing at the spare flips exactly
    // the occupancy/trailing accounting the pager derives from. Retained
    // homing, mapping, and membership are untouched.
    let ring_before = engine.mapping.get("out-a").cloned().expect("ring a");
    let home_before = engine.core_homing(&WindowId("win-b".to_owned()));
    engine.staged.homing.insert(
        WindowId("win-b".to_owned()),
        DomainKey {
            output: OutputId("out-a".to_owned()),
            workspace: WorkspaceId(spare.clone()),
        },
    );
    // Verified after state: same id, no longer trailing, occupied in the
    // staging the pager derives from. (The last published pager row
    // predates this staging-only setup; `trailing_marked_id` and
    // `ring_occupied` below are the live fields that row derives from.)
    assert_ne!(
        engine.trailing_marked_id("out-a"),
        Some(spare.clone()),
        "spare is no longer the marked trailing id"
    );
    assert!(
        engine
            .ring_occupied(&OutputId("out-a".to_owned()))
            .contains(&spare),
        "staging shows the spare occupied"
    );
    assert_eq!(
        engine.core_homing(&WindowId("win-b".to_owned())),
        home_before,
        "retained homing untouched by the staging setup"
    );
    assert_eq!(
        engine.mapping.get("out-a"),
        Some(&ring_before),
        "ring untouched by the staging setup"
    );
    let latest = latest_published(&engine);
    assert_eq!(
        check(&engine, Some(&ws_input(rev_old, "out-a", &spare)), latest),
        None,
        "same workspace id accepts despite trailing/occupied change"
    );
    assert_eq!(
        check(
            &engine,
            Some(&win_input(rev_old, "out-a", &view_a, "win-b")),
            latest
        ),
        None,
        "window homing untouched by the staging setup still accepts"
    );
}

#[test]
fn native_binding_change_accepts_same_workspace() {
    let (mut engine, view_a, _, rev_old, _) = seed_pair("gen-1");
    // Verified before state: native-free seed carries no binding.
    assert_eq!(
        engine.native_binding_for("out-a", &view_a),
        None,
        "no retained binding before"
    );
    assert_eq!(
        check(&engine, Some(&ws_input(rev_old, "out-a", &view_a)), rev_old),
        None
    );
    // Direct retained native_map setup (labeled setup, not a production
    // op): bind one native id to the same logical workspace. The
    // capability stays off, so mapping and membership are untouched.
    let ring_before = engine.mapping.get("out-a").cloned().expect("ring a");
    let home_before = engine.core_homing(&WindowId("win-a".to_owned()));
    engine
        .native_map
        .insert(("out-a".to_owned(), "ndk-1".to_owned()), view_a.clone());
    // Verified after state: the binding reads back, identity untouched.
    assert_eq!(
        engine.native_binding_for("out-a", &view_a),
        Some("ndk-1".to_owned()),
        "retained binding reads back"
    );
    assert_eq!(
        engine.mapping.get("out-a"),
        Some(&ring_before),
        "ring untouched by the binding"
    );
    assert_eq!(
        engine.core_homing(&WindowId("win-a".to_owned())),
        home_before,
        "homing untouched by the binding"
    );
    let latest = latest_published(&engine);
    assert_eq!(
        check(&engine, Some(&ws_input(rev_old, "out-a", &view_a)), latest),
        None,
        "same workspace id accepts despite binding change"
    );
}

#[test]
fn group_member_set_change_accepts_surviving_group() {
    let (mut engine, view_a, _, rev_old, _) = seed_pair("gen-1");
    let groups_before = group_ids(&engine, "out-a", &view_a);
    assert!(!groups_before.is_empty(), "seeded domain carries a group");
    let members_before = domain_members(&engine, "out-a", &view_a);
    assert_eq!(
        members_before,
        BTreeSet::from([WindowId("win-a".to_owned()), WindowId("win-b".to_owned())]),
        "seeded domain holds exactly win-a/win-b"
    );
    // Production path: a newcomer observed on the same output adopts the
    // live view through the accepted report.
    accepted(
        &mut engine,
        &report(
            "gen-1",
            3,
            screens_pair(),
            vec![
                fact("win-a", "out-a", 0),
                fact("win-b", "out-a", 100),
                fact("win-d", "out-a", 200),
                fact("win-c", "out-b", 800),
            ],
            Vec::new(),
            "win-a",
            None,
            None,
        ),
    );
    assert_eq!(
        engine.core_homing(&WindowId("win-d".to_owned())),
        Some(domain_key("out-a", &view_a)),
        "newcomer adopts the live view"
    );
    // The exact pre-existing group id survives as the domain root while
    // the exact member lifetime set changed (not just frame payload).
    let key = domain_key("out-a", &view_a);
    let root_id = engine
        .engine
        .session(&key)
        .and_then(|session| session.tree_for(&key))
        .map(|tree| tree.id().clone())
        .expect("domain tree");
    assert_eq!(
        root_id, groups_before[0],
        "pre-existing group id survives as the domain root"
    );
    let members_after = domain_members(&engine, "out-a", &view_a);
    assert_eq!(
        members_after,
        BTreeSet::from([
            WindowId("win-a".to_owned()),
            WindowId("win-b".to_owned()),
            WindowId("win-d".to_owned())
        ]),
        "exact member lifetime set changed"
    );
    assert_ne!(members_after, members_before);
    let latest = latest_published(&engine);
    assert_eq!(
        check(
            &engine,
            Some(&grp_input(rev_old, "out-a", &view_a, &groups_before[0])),
            latest
        ),
        None,
        "surviving group id accepts despite member-set change"
    );
    assert_eq!(
        check(
            &engine,
            Some(&win_input(rev_old, "out-a", &view_a, "win-d")),
            latest
        ),
        None
    );
}

#[test]
fn generation_reset_leftover_rev_refuses() {
    // Fresh generation restarts low: a leftover higher input rev from the
    // old generation refuses against the new actual latest published rev
    // even for the identical row identity. Validator-level only: no v4
    // envelope gate is wired, so no handle_report-v4 claim is made here.
    let (engine, view_a, _, _, rev_new) = seed_pair("gen-2");
    assert_eq!(latest_published(&engine), rev_new);
    let leftover = rev_new + 8;
    assert_eq!(
        check(
            &engine,
            Some(&ws_input(leftover, "out-a", &view_a)),
            rev_new
        ),
        Some(CommandRejectKind::StaleRenderTarget)
    );
    assert_eq!(
        check(&engine, Some(&ws_input(rev_new, "out-a", &view_a)), rev_new),
        None
    );
}

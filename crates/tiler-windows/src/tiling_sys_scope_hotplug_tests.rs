//! Mode-aware disconnect/reconnect topology tests (item 12 stage C).
//!
//! Offline simulated topology: drive the real production
//! [`super::tiling_sys_scope_hotplug::sync_monitor_outputs`] entry with
//! narrowed [`super::MonitorArea`] sets (no live windows, hooks, overlays,
//! or settings changes). Covers each mode for active-on-removed versus
//! active-on-survivor, sends/new windows while absent, history
//! discard/re-prime with reconnect selection independent of history,
//! displaced return (local/global), shared no-displacement (KDE parity),
//! and sibling Engine-domain retention.
//!
//! The global foreground branch runs through the real production path via
//! an explicit identified member location (`sync_fg`); only the native API
//! read plus lifetime gate stay live-only and unproven here.

use super::{MonitorArea, TileLoop, rmax03_adapter_tests, workspace_domain_for};
use crate::workspace::WindowKey;
use crate::workspace_scope_owner::WorkspaceScopeOwner;
use tiler_core::geometry::Rect;
use tiler_core::workspace_scope::WorkspaceMode::{GlobalUnique, PerOutputLocal, Shared};

const BOUNDS: Rect = Rect {
    x: 0,
    y: 0,
    w: 800,
    h: 600,
};

fn key(hwnd: u64, pid: u32) -> WindowKey {
    WindowKey {
        hwnd,
        pid,
        creation: format!("c{hwnd:016x}"),
    }
}

fn areas() -> Vec<MonitorArea> {
    vec![
        MonitorArea {
            device: "mon-a".to_owned(),
            work: BOUNDS,
            full: BOUNDS,
        },
        MonitorArea {
            device: "mon-b".to_owned(),
            work: BOUNDS,
            full: BOUNDS,
        },
    ]
}

fn only_a() -> Vec<MonitorArea> {
    vec![areas()[0].clone()]
}

fn only_b() -> Vec<MonitorArea> {
    vec![areas()[1].clone()]
}

fn two_output_state(mode: tiler_core::workspace_scope::WorkspaceMode) -> TileLoop {
    let mut state = rmax03_adapter_tests::test_state();
    state.workspaces.ensure_output("mon-a");
    state.workspaces.ensure_output("mon-b");
    state.scope_owner = WorkspaceScopeOwner::adopt_startup(mode);
    state.known_outputs = vec!["mon-a".to_owned(), "mon-b".to_owned()];
    state.last_areas = areas();
    state.active_output = "mon-a".to_owned();
    state
}

fn stage_member(state: &mut TileLoop, output: &str, ws: &str, member: &WindowKey) {
    assert!(
        state.workspaces.assign(member.clone(), output, ws, false),
        "stage assigns"
    );
    // Owner token table (the foreground branch resolves members through it).
    state
        .member_tokens
        .insert(member.clone(), format!("t{}", member.hwnd));
}

/// Seed one non-empty Engine layout via the real reconcile route (a
/// committed layout with topology, not idle history).
fn seed_layout_session(state: &mut TileLoop, output: &str, ws: &str, token: &str) {
    let full = areas();
    let (domain, key) = workspace_domain_for(output, ws, &full, state.inner_gap, state.outer_gap)
        .expect("seed domain");
    let event = crate::tiling::build_reconcile_event_for(
        &state.owner.clone(),
        &state.generation.clone(),
        &tiler_core::ids::CorrelationId::parse("hotplug-seed").expect("correlation"),
        0,
        1,
        &domain,
        &key,
        state.outer_gap,
        &[(
            tiler_core::directional::WindowId(token.to_owned()),
            BOUNDS,
            tiler_core::size_hints::WindowSizeHints::none(),
        )],
        Some(tiler_core::directional::WindowId(token.to_owned())).as_ref(),
    );
    let reply = state.engine.handle(&event);
    assert!(
        matches!(
            reply,
            tiler_core::boundary::CoreReply::Projection(_)
                | tiler_core::boundary::CoreReply::Tiled(_)
        ),
        "seed layout ({output},{ws}) converges"
    );
}

/// Retain one idle empty Engine domain (stale-session pruning input).
fn seed_empty_session(state: &mut TileLoop, output: &str, ws: &str) {
    let full = areas();
    let (domain, key) = workspace_domain_for(output, ws, &full, state.inner_gap, state.outer_gap)
        .expect("seed domain");
    let session = tiler_core::session::Session::new(
        state.owner.clone(),
        state.generation.clone(),
        0,
        0,
        vec![domain],
    )
    .expect("seed session");
    state.engine.insert_raw(key, session, state.outer_gap);
}

fn scope_domain_key(output: &str, ws: &str) -> tiler_core::session::DomainKey {
    tiler_core::session::DomainKey {
        output: tiler_core::directional::OutputId(output.to_owned()),
        workspace: tiler_core::directional::WorkspaceId(ws.to_owned()),
    }
}

fn session_nonempty(state: &TileLoop, output: &str, ws: &str) -> bool {
    state
        .engine
        .session(&scope_domain_key(output, ws))
        .is_some_and(|s| !tiler_core::engine::committed_session_is_empty(s))
}

fn sync(state: &mut TileLoop, narrow: &[MonitorArea]) {
    super::tiling_sys_scope_hotplug::sync_monitor_outputs(state, narrow);
}

/// Global sync with an explicit foreground location: drives the real
/// production foreground-on-removed branch with an identified member
/// location (the native read plus lifetime gate stay live-only).
fn sync_fg(state: &mut TileLoop, narrow: &[MonitorArea], fg: Option<(String, String)>) {
    super::tiling_sys_scope_hotplug::sync_global_with_foreground(state, narrow, fg);
}

fn record(state: &mut TileLoop, output: &str, current: &str) -> bool {
    let (owner, spaces) = (&mut state.scope_owner, &mut state.workspaces);
    owner.record_completed(spaces, output, current)
}

#[test]
fn local_disconnect_survivor_keeps_view_and_discards_origin() {
    let mut state = two_output_state(PerOutputLocal);
    let m1 = key(0x101, 11);
    let m2 = key(0x102, 12);
    stage_member(&mut state, "mon-a", "ws-1", &m1);
    stage_member(&mut state, "mon-b", "ws-3", &m2);
    // Local previous on the survivor: ws-1 precedes ws-2.
    assert!(state.workspaces.activate("mon-a", "ws-2"));
    assert!(state.workspaces.observe_workspace_change("mon-a", "ws-2"));
    assert_eq!(
        state.workspaces.history_previous("mon-a"),
        Some("ws-1".to_owned())
    );
    sync(&mut state, &only_a());
    // Survivor view untouched (no foreground in tests), displaced unit moved
    // whole onto the survivor, origin history discarded with the output.
    assert_eq!(state.workspaces.active_id("mon-a"), Some("ws-2".to_owned()));
    assert!(
        state
            .workspaces
            .workspace_ids("mon-a")
            .contains(&"ws-3".to_owned())
    );
    assert_eq!(
        state.workspaces.member_loc(&m2).map(|l| l.output.clone()),
        Some("mon-a".to_owned())
    );
    assert_eq!(
        state
            .workspaces
            .displaced_snapshot()
            .get("mon-b")
            .map(|r| r.dest_key.clone()),
        Some("mon-a".to_owned())
    );
    assert_eq!(state.workspaces.history_previous("mon-b"), None);
    assert_eq!(
        state.workspaces.history_previous("mon-a"),
        Some("ws-1".to_owned())
    );
    assert_eq!(state.active_output, "mon-a");
}

#[test]
fn local_disconnect_active_origin_repoints_without_stealing() {
    let mut state = two_output_state(PerOutputLocal);
    state.active_output = "mon-b".to_owned();
    sync(&mut state, &only_a());
    assert_eq!(state.active_output, "mon-a");
    assert_eq!(state.workspaces.active_id("mon-a"), Some("ws-1".to_owned()));
}

#[test]
fn local_reconnect_returns_then_current_contents() {
    let mut state = two_output_state(PerOutputLocal);
    let m2 = key(0x102, 12);
    stage_member(&mut state, "mon-b", "ws-3", &m2);
    sync(&mut state, &only_a());
    // New contents while absent join the displaced unit on the survivor.
    let fresh = key(0x103, 13);
    stage_member(&mut state, "mon-a", "ws-3", &fresh);
    sync(&mut state, &areas());
    // Exact-origin return with then-current contents, never a snapshot.
    assert_eq!(
        state.workspaces.workspace_ids("mon-b"),
        vec!["ws-3".to_owned(), "ws-4".to_owned()]
    );
    assert_eq!(state.workspaces.active_id("mon-b"), Some("ws-3".to_owned()));
    assert!(state.workspaces.displaced_snapshot().is_empty());
    for member in [&m2, &fresh] {
        assert_eq!(
            state
                .workspaces
                .member_loc(member)
                .map(|l| { (l.output.clone(), l.workspace.clone()) }),
            Some(("mon-b".to_owned(), "ws-3".to_owned()))
        );
    }
    // Survivor keeps its own scope; reconnect never restores history, so the
    // toggle is a no-op until the next recorded change.
    assert_eq!(
        state.workspaces.workspace_ids("mon-a"),
        vec!["ws-1".to_owned(), "ws-2".to_owned()]
    );
    assert_eq!(state.workspaces.history_previous("mon-b"), None);
    assert_eq!(state.workspaces.resolve_previous("mon-b"), None);
    assert_eq!(state.active_output, "mon-a");
}

#[test]
fn local_no_survivor_defers_with_state_kept() {
    let mut state = two_output_state(PerOutputLocal);
    let m1 = key(0x101, 11);
    stage_member(&mut state, "mon-a", "ws-1", &m1);
    sync(&mut state, &[]);
    assert!(
        state
            .workspaces
            .workspace_ids("mon-a")
            .contains(&"ws-1".to_owned())
    );
    assert!(
        state
            .workspaces
            .workspace_ids("mon-b")
            .contains(&"ws-3".to_owned())
    );
    assert!(state.workspaces.displaced_snapshot().is_empty());
    assert_eq!(
        state.known_outputs,
        vec!["mon-a".to_owned(), "mon-b".to_owned()]
    );
}

#[test]
fn global_disconnect_discards_origin_and_records_survivor() {
    let mut state = two_output_state(GlobalUnique);
    let m1 = key(0x101, 11);
    let m2 = key(0x102, 12);
    stage_member(&mut state, "mon-a", "ws-1", &m1);
    stage_member(&mut state, "mon-a", "ws-2", &m2);
    // Scoped previous ws-1 with current ws-2 on the survivor.
    assert!(!record(&mut state, "mon-a", "ws-1"));
    assert!(state.workspaces.activate("mon-a", "ws-2"));
    assert!(record(&mut state, "mon-a", "ws-2"));
    assert!(!record(&mut state, "mon-b", "ws-3"));
    sync(&mut state, &only_a());
    assert_eq!(state.workspaces.active_id("mon-a"), Some("ws-2".to_owned()));
    assert!(
        state
            .workspaces
            .workspace_ids("mon-a")
            .contains(&"ws-3".to_owned())
    );
    // Scoped origin history discarded with the output; survivor entry keeps
    // its recorded previous (same-view observation is a validating no-op).
    assert_eq!(state.scope_owner.history().previous("mon-b"), None);
    assert_eq!(state.scope_owner.history().baseline("mon-b"), None);
    assert_eq!(
        state.scope_owner.history().previous("mon-a"),
        Some("ws-1".to_owned())
    );
    assert_eq!(
        state
            .workspaces
            .displaced_snapshot()
            .get("mon-b")
            .map(|r| r.dest_key.clone()),
        Some("mon-a".to_owned())
    );
}

#[test]
fn global_rws17_toggle_then_return_clears_and_noops() {
    let mut state = two_output_state(GlobalUnique);
    let m1 = key(0x201, 21);
    let m2 = key(0x202, 22);
    let md = key(0x203, 23);
    stage_member(&mut state, "mon-a", "ws-1", &m1);
    stage_member(&mut state, "mon-a", "ws-2", &m2);
    stage_member(&mut state, "mon-b", "ws-3", &md);
    seed_layout_session(&mut state, "mon-a", "ws-2", "sib-a");
    seed_layout_session(&mut state, "mon-b", "ws-3", "sib-b");
    assert!(!record(&mut state, "mon-a", "ws-1"));
    assert!(state.workspaces.activate("mon-a", "ws-2"));
    assert!(record(&mut state, "mon-a", "ws-2"));
    assert!(!record(&mut state, "mon-b", "ws-3"));
    // Disconnect through the real foreground-on-removed branch: D (ws-3)
    // held focus, so the survivor shows D with its focus preserved.
    sync_fg(
        &mut state,
        &only_a(),
        Some(("mon-b".to_owned(), "ws-3".to_owned())),
    );
    assert_eq!(state.workspaces.active_id("mon-a"), Some("ws-3".to_owned()));
    assert_eq!(
        state.workspaces.remembered_focus("mon-a", "ws-3"),
        Some(md.clone())
    );
    assert_eq!(
        state.scope_owner.history().previous("mon-a"),
        Some("ws-2".to_owned())
    );
    assert_eq!(
        state.scope_owner.history().baseline("mon-a"),
        Some("ws-3".to_owned())
    );
    // First toggle selects ws-2 with previous D.
    let (owner, spaces) = (&mut state.scope_owner, &mut state.workspaces);
    assert_eq!(
        owner.resolve_previous(spaces, "mon-a"),
        Some("ws-2".to_owned())
    );
    assert!(state.workspaces.activate("mon-a", "ws-2"));
    assert!(record(&mut state, "mon-a", "ws-2"));
    assert_eq!(
        state.scope_owner.history().previous("mon-a"),
        Some("ws-3".to_owned())
    );
    // New contents before return join the displaced unit.
    let fresh = key(0x204, 24);
    stage_member(&mut state, "mon-a", "ws-3", &fresh);
    // Reconnect: D returns with then-current contents; the survivor keeps
    // ws-2 with no stealing.
    sync(&mut state, &areas());
    assert_eq!(
        state.workspaces.workspace_ids("mon-b"),
        vec!["ws-3".to_owned(), "ws-4".to_owned()]
    );
    assert_eq!(state.workspaces.active_id("mon-b"), Some("ws-3".to_owned()));
    for member in [&md, &fresh] {
        assert_eq!(
            state
                .workspaces
                .member_loc(member)
                .map(|l| { (l.output.clone(), l.workspace.clone()) }),
            Some(("mon-b".to_owned(), "ws-3".to_owned()))
        );
    }
    assert_eq!(state.workspaces.active_id("mon-a"), Some("ws-2".to_owned()));
    // Reconnect selection never consulted history: the origin baseline
    // re-primed at the returning view with no previous entry.
    assert_eq!(
        state.scope_owner.history().baseline("mon-b"),
        Some("ws-3".to_owned())
    );
    assert_eq!(state.scope_owner.history().previous("mon-b"), None);
    // The returned previous clears like removal: second toggle is a no-op.
    let (owner, spaces) = (&mut state.scope_owner, &mut state.workspaces);
    assert_eq!(owner.resolve_previous(spaces, "mon-a"), None);
    // Engine layouts ride the unit both ways; the surviving sibling keeps
    // its own domain.
    assert!(session_nonempty(&state, "mon-b", "ws-3"));
    assert!(session_nonempty(&state, "mon-a", "ws-2"));
}

#[test]
fn global_while_absent_sends_new_windows_and_stale_prune() {
    let mut state = two_output_state(GlobalUnique);
    let m1 = key(0x301, 31);
    let md = key(0x302, 32);
    stage_member(&mut state, "mon-a", "ws-1", &m1);
    assert!(state.workspaces.activate("mon-a", "ws-1"));
    assert!(!record(&mut state, "mon-a", "ws-1"));
    stage_member(&mut state, "mon-b", "ws-3", &md);
    // Ordinary empty middle plus its idle empty Engine domain: prunable.
    seed_empty_session(&mut state, "mon-a", "ws-2");
    sync(&mut state, &only_a());
    // Sends resolve to the displaced D on the survivor over the scoped ring
    // (ws-2 already pruned below, so the ring steps over it).
    let ring = state.scope_owner.scoped_ring(&state.workspaces, "mon-a");
    let pos = ring.iter().position(|id| id == "ws-3").expect("D in ring") + 1;
    assert_eq!(
        state
            .scope_owner
            .resolve_numbered(&state.workspaces, "mon-a", pos as u8),
        Some("ws-3".to_owned())
    );
    assert_eq!(
        state
            .scope_owner
            .resolve_relative(&state.workspaces, "mon-a", 1),
        Some("ws-3".to_owned())
    );
    // Ordinary stale empty pruned with its idle domain; displaced empties
    // stay pinned with their sessions.
    assert!(
        !state
            .workspaces
            .workspace_ids("mon-a")
            .contains(&"ws-2".to_owned())
    );
    assert!(
        state
            .engine
            .session(&scope_domain_key("mon-a", "ws-2"))
            .is_none()
    );
    assert!(
        state
            .workspaces
            .workspace_ids("mon-a")
            .contains(&"ws-3".to_owned())
    );
    assert!(
        state
            .workspaces
            .workspace_ids("mon-a")
            .contains(&"ws-4".to_owned())
    );
    // New windows while absent land on the survivor and stay there.
    let fresh = key(0x303, 33);
    stage_member(&mut state, "mon-a", "ws-1", &fresh);
    sync(&mut state, &areas());
    assert_eq!(
        state
            .workspaces
            .member_loc(&fresh)
            .map(|l| { (l.output.clone(), l.workspace.clone()) }),
        Some(("mon-a".to_owned(), "ws-1".to_owned()))
    );
    assert_eq!(
        state
            .workspaces
            .member_loc(&md)
            .map(|l| { (l.output.clone(), l.workspace.clone()) }),
        Some(("mon-b".to_owned(), "ws-3".to_owned()))
    );
}

#[test]
fn global_sibling_layouts_return_home() {
    let mut state = two_output_state(GlobalUnique);
    seed_layout_session(&mut state, "mon-a", "ws-1", "home-a");
    seed_layout_session(&mut state, "mon-b", "ws-3", "home-b");
    sync(&mut state, &only_a());
    // Relocation is best-effort: the layout is preserved on exactly one side.
    assert!(session_nonempty(&state, "mon-a", "ws-3") || session_nonempty(&state, "mon-b", "ws-3"));
    assert!(session_nonempty(&state, "mon-a", "ws-1"));
    sync(&mut state, &areas());
    assert!(session_nonempty(&state, "mon-b", "ws-3"));
    assert!(session_nonempty(&state, "mon-a", "ws-1"));
}

fn converged_shared_state() -> TileLoop {
    let mut state = two_output_state(Shared);
    state.scope_owner.sync_shared_order(&state.workspaces);
    // Converge every live id onto both output-bound domains (same logical
    // ids, independent Engine domains).
    let order = state.scope_owner.shared_order().clone();
    for output in ["mon-a", "mon-b"] {
        for id in &order {
            state.workspaces.ensure_workspace_id(output, id);
        }
    }
    state.scope_owner.sync_shared_order(&state.workspaces);
    state.workspaces.activate("mon-a", "ws-1");
    state.workspaces.activate("mon-b", "ws-1");
    let (owner, spaces) = (&mut state.scope_owner, &mut state.workspaces);
    owner.prime_shared_view(spaces, "mon-a");
    state
}

#[test]
fn shared_disconnect_skips_displacement_and_merges() {
    let mut state = converged_shared_state();
    let ma = key(0x401, 41);
    let mb = key(0x402, 42);
    let mc = key(0x403, 44);
    stage_member(&mut state, "mon-a", "ws-1", &ma);
    stage_member(&mut state, "mon-b", "ws-1", &mb);
    // Occupy ws-3 so scoped trailing keeps the full ring (ordinary empty
    // middles prune by design; stability is pinned for occupied ids).
    stage_member(&mut state, "mon-a", "ws-3", &mc);
    // One shared change first: ws-1 -> ws-2 on the survivor.
    assert!(state.workspaces.activate("mon-a", "ws-2"));
    assert!(state.workspaces.activate("mon-b", "ws-2"));
    assert!(record(&mut state, "mon-a", "ws-2"));
    assert_eq!(
        state.scope_owner.history().shared_previous(),
        Some("ws-1".to_owned())
    );
    let order_before = state.scope_owner.shared_order().clone();
    seed_layout_session(&mut state, "mon-a", "ws-1", "shared-a");
    seed_layout_session(&mut state, "mon-b", "ws-1", "shared-b");
    sync(&mut state, &only_a());
    // KDE parity: no displaced record, survivor view untouched, shared order
    // stable, same-id members re-homed, shared history just observes.
    assert!(state.workspaces.displaced_snapshot().is_empty());
    assert!(!state.workspaces.output_keys().contains(&"mon-b".to_owned()));
    assert_eq!(state.workspaces.active_id("mon-a"), Some("ws-2".to_owned()));
    assert_eq!(*state.scope_owner.shared_order(), order_before);
    for member in [&ma, &mb] {
        assert_eq!(
            state
                .workspaces
                .member_loc(member)
                .map(|l| { (l.output.clone(), l.workspace.clone()) }),
            Some(("mon-a".to_owned(), "ws-1".to_owned()))
        );
    }
    assert_eq!(
        state.scope_owner.history().shared_previous(),
        Some("ws-1".to_owned())
    );
    // Sibling layouts retained independently: the survivor session was never
    // clobbered and the removed sibling session persists unmapped.
    assert!(session_nonempty(&state, "mon-a", "ws-1"));
    assert!(session_nonempty(&state, "mon-b", "ws-1"));
}

#[test]
fn shared_active_origin_repoints_without_view_switch() {
    let mut state = converged_shared_state();
    // Occupy the middle ids so scoped trailing keeps the full ring while the
    // first-sorted output drops (members merge onto the survivor).
    let m2 = key(0x411, 45);
    let m3 = key(0x412, 46);
    stage_member(&mut state, "mon-b", "ws-2", &m2);
    stage_member(&mut state, "mon-b", "ws-3", &m3);
    // Drop the first-sorted output: the stable order sync must not reorder.
    let order_before = state.scope_owner.shared_order().clone();
    state.active_output = "mon-a".to_owned();
    sync(&mut state, &only_b());
    assert_eq!(state.active_output, "mon-b");
    assert!(state.workspaces.displaced_snapshot().is_empty());
    assert_eq!(*state.scope_owner.shared_order(), order_before);
    assert_eq!(state.workspaces.active_id("mon-b"), Some("ws-1".to_owned()));
    // Same-id members merged onto the survivor with the logical id intact.
    assert_eq!(
        state
            .workspaces
            .member_loc(&m2)
            .map(|l| { (l.output.clone(), l.workspace.clone()) }),
        Some(("mon-b".to_owned(), "ws-2".to_owned()))
    );
    assert_eq!(
        state
            .workspaces
            .member_loc(&m3)
            .map(|l| { (l.output.clone(), l.workspace.clone()) }),
        Some(("mon-b".to_owned(), "ws-3".to_owned()))
    );
}

#[test]
fn shared_reconnect_converges_without_consulting_history() {
    let mut state = converged_shared_state();
    // Occupy the ends of the ring so trailing keeps every id (previous ws-1
    // must stay a valid toggle target throughout).
    let keep1 = key(0x421, 47);
    let keep3 = key(0x422, 48);
    stage_member(&mut state, "mon-a", "ws-1", &keep1);
    stage_member(&mut state, "mon-a", "ws-3", &keep3);
    assert!(state.workspaces.activate("mon-a", "ws-2"));
    assert!(state.workspaces.activate("mon-b", "ws-2"));
    assert!(record(&mut state, "mon-a", "ws-2"));
    sync(&mut state, &only_a());
    // Sends resolve over the live-filtered shared ring while absent.
    let ring = state.scope_owner.shared_ring(&state.workspaces);
    assert_eq!(ring, *state.scope_owner.shared_order());
    assert_eq!(
        state
            .scope_owner
            .resolve_numbered(&state.workspaces, "mon-a", 2),
        Some(ring[1].clone())
    );
    // New windows while absent land on the survivor under the shared id.
    let fresh = key(0x403, 43);
    stage_member(&mut state, "mon-a", "ws-3", &fresh);
    sync(&mut state, &areas());
    // Returning origin converges onto the shared order at the survivor view;
    // the shared previous is untouched, so reconnect provably ignored it.
    assert!(state.workspaces.displaced_snapshot().is_empty());
    assert_eq!(
        state.workspaces.workspace_ids("mon-b"),
        state.workspaces.workspace_ids("mon-a")
    );
    assert_eq!(state.workspaces.active_id("mon-b"), Some("ws-2".to_owned()));
    assert_eq!(
        state.scope_owner.history().shared_previous(),
        Some("ws-1".to_owned())
    );
    let (owner, spaces) = (&mut state.scope_owner, &mut state.workspaces);
    assert_eq!(
        owner.resolve_previous(spaces, "mon-a"),
        Some("ws-1".to_owned())
    );
    assert_eq!(
        state
            .workspaces
            .member_loc(&fresh)
            .map(|l| { (l.output.clone(), l.workspace.clone()) }),
        Some(("mon-a".to_owned(), "ws-3".to_owned()))
    );
    assert_eq!(state.active_output, "mon-a");
}

/// Real Engine flight staged through public session ops (no inbox mocks):
/// a directional-proposal pending plan plus an open drag capture.
fn live_observation(
    state: &TileLoop,
    output: &str,
    ws: &str,
) -> tiler_core::session::SessionObservation {
    use tiler_core::session::ObservedWindow;
    let key = scope_domain_key(output, ws);
    let session = state.engine.session(&key).expect("seeded session");
    let mut wins: Vec<ObservedWindow> = session
        .snapshot()
        .windows
        .iter()
        .map(|l| ObservedWindow {
            window: l.window.clone(),
            output: l.output.clone(),
            workspace: l.workspace.clone(),
            floating: false,
            fullscreen: false,
            maximized: false,
            sticky: false,
            fixed_auto: false,
            fixed_suppress: false,
            hints: tiler_core::size_hints::WindowSizeHints::none(),
        })
        .collect();
    wins.extend(session.exception_observed());
    tiler_core::session::SessionObservation {
        observation: tiler_core::contract::Observation::new(
            state.owner.clone(),
            state.generation.clone(),
            session.accepted_revision(),
            100,
        ),
        windows: wins,
    }
}

/// Stage a real pending plan: propose removing the seeded window and
/// withhold acknowledgement, so the reconciler pending slot stays set.
fn stage_pending_remove(state: &mut TileLoop, output: &str, ws: &str, token: &str) {
    use tiler_core::directional::WindowId;
    let key = scope_domain_key(output, ws);
    let obs = live_observation(state, output, ws);
    let session = state.engine.session_mut(&key).expect("seeded session");
    let plan = session
        .propose(
            &tiler_core::session::SessionCommand::Remove {
                window: WindowId(token.to_owned()),
            },
            &obs,
            &tiler_core::ids::CorrelationId::parse("hotplug-flight").expect("correlation"),
            &tiler_core::contract::LifecycleCapabilities::full(),
        )
        .expect("remove proposes");
    let _ = plan;
    assert!(session.has_pending(), "remove stages a pending plan");
}

/// Stage a real drag capture on the seeded (focused) window.
fn stage_drag(state: &mut TileLoop, output: &str, ws: &str, token: &str) {
    use tiler_core::directional::WindowId;
    let key = scope_domain_key(output, ws);
    let obs = live_observation(state, output, ws);
    let session = state.engine.session_mut(&key).expect("seeded session");
    session
        .begin_drag(&WindowId(token.to_owned()), &obs)
        .expect("drag begins");
    assert!(session.has_drag(), "drag capture is open");
}

#[test]
fn shared_cold_startup_matches_stage_b_oracle() {
    // Truly empty state: no prepopulated outputs (this is the regression the
    // steady-state tests mask). Primary-first areas order disagrees with
    // stable-key order to pin the anchor rule.
    let mut state = rmax03_adapter_tests::test_state();
    state.scope_owner = WorkspaceScopeOwner::adopt_startup(Shared);
    let mut rev = areas();
    rev.reverse();
    assert_eq!(rev[0].device, "mon-b");
    sync(&mut state, &rev);
    // Legacy initial creation exactly like the local path, in enumeration
    // (areas) order: mon-b came first, so it owns the first minimum pair.
    // No shared order, no history, no displacement.
    assert_eq!(
        state.workspaces.workspace_ids("mon-b"),
        vec!["ws-1".to_owned(), "ws-2".to_owned()]
    );
    assert_eq!(
        state.workspaces.workspace_ids("mon-a"),
        vec!["ws-3".to_owned(), "ws-4".to_owned()]
    );
    assert!(state.scope_owner.shared_order().is_empty());
    assert_eq!(state.scope_owner.history().shared_previous(), None);
    assert!(state.workspaces.displaced_snapshot().is_empty());
    assert_eq!(
        state.known_outputs,
        vec!["mon-b".to_owned(), "mon-a".to_owned()]
    );
    // Actual startup sequence: shared prime converges the stable-key first
    // output's view (mon-a ws-3), not the primary (mon-b ws-1), primes one
    // history without recording, and converges the trailing floor.
    state.last_areas = rev.clone();
    super::tiling_sys_scope_trailing::prime_after_sync(&mut state);
    assert_eq!(state.workspaces.active_id("mon-a"), Some("ws-3".to_owned()));
    assert_eq!(state.workspaces.active_id("mon-b"), Some("ws-3".to_owned()));
    assert_eq!(state.scope_owner.history().shared_previous(), None);
    assert!(!state.scope_owner.shared_order().is_empty());
    // Stage B ring floor (ring-global, per the shared trailing contract):
    // at least the minimum with a globally unoccupied tail.
    let ring = state.scope_owner.shared_ring(&state.workspaces);
    assert!(ring.len() >= tiler_core::workspace::MIN_WORKSPACES);
    let tail = ring.last().cloned().expect("ring tail");
    assert!(
        !state
            .workspaces
            .output_keys()
            .iter()
            .any(|o| !state.workspaces.workspace_members(o, &tail).is_empty()),
        "trailing spare is globally unoccupied"
    );
}

#[test]
fn global_foreground_on_survivor_keeps_view() {
    let mut state = two_output_state(GlobalUnique);
    let m1 = key(0x501, 51);
    let m2 = key(0x502, 52);
    stage_member(&mut state, "mon-a", "ws-1", &m1);
    stage_member(&mut state, "mon-a", "ws-2", &m2);
    state.workspaces.note_foreground(&m2);
    assert!(!record(&mut state, "mon-a", "ws-1"));
    assert!(state.workspaces.activate("mon-a", "ws-2"));
    assert!(record(&mut state, "mon-a", "ws-2"));
    // Foreground on the survivor: the real branch takes no activation, keeps
    // the view, and leaves history plus remembered focus untouched.
    sync_fg(
        &mut state,
        &only_a(),
        Some(("mon-a".to_owned(), "ws-2".to_owned())),
    );
    assert_eq!(state.workspaces.active_id("mon-a"), Some("ws-2".to_owned()));
    assert_eq!(
        state.scope_owner.history().previous("mon-a"),
        Some("ws-1".to_owned())
    );
    assert_eq!(
        state.workspaces.remembered_focus("mon-a", "ws-2"),
        Some(m2.clone())
    );
    assert!(
        state
            .workspaces
            .workspace_ids("mon-a")
            .contains(&"ws-3".to_owned())
    );
}

#[test]
fn global_new_output_primes_scoped_baseline() {
    // Fresh global run: outputs appear through sync with no baselines yet.
    let mut state = rmax03_adapter_tests::test_state();
    state.scope_owner = WorkspaceScopeOwner::adopt_startup(GlobalUnique);
    sync(&mut state, &areas());
    for output in ["mon-a", "mon-b"] {
        let active = state.workspaces.active_id(output).expect("active");
        assert_eq!(state.scope_owner.history().baseline(output), Some(active));
        assert_eq!(state.scope_owner.history().previous(output), None);
    }
    // A genuinely new output mid-run primes the same way (reconnect parity).
    state.known_outputs = vec!["mon-a".to_owned(), "mon-b".to_owned()];
    state.last_areas = areas();
    let mut three = areas();
    three.push(MonitorArea {
        device: "mon-c".to_owned(),
        work: BOUNDS,
        full: BOUNDS,
    });
    sync(&mut state, &three);
    let active_c = state.workspaces.active_id("mon-c").expect("new active");
    assert_eq!(
        state.scope_owner.history().baseline("mon-c"),
        Some(active_c)
    );
    assert_eq!(state.scope_owner.history().previous("mon-c"), None);
}

#[test]
fn global_pending_displaced_session_survives_vs_idle_prune() {
    let mut state = two_output_state(GlobalUnique);
    let m1 = key(0x601, 61);
    stage_member(&mut state, "mon-a", "ws-1", &m1);
    assert!(state.workspaces.activate("mon-a", "ws-1"));
    assert!(!record(&mut state, "mon-a", "ws-1"));
    // Real pending flight on the removed output's layout.
    seed_layout_session(&mut state, "mon-b", "ws-3", "flight-w");
    stage_pending_remove(&mut state, "mon-b", "ws-3", "flight-w");
    // Ordinary idle-empty middle plus its idle domain: prunable control.
    seed_empty_session(&mut state, "mon-a", "ws-2");
    sync(&mut state, &only_a());
    // Relocation refuses pending sessions, so the flight stays output-bound
    // and unmapped-yet-pending: pinned through scoped cleanup, still pending.
    assert!(
        state
            .engine
            .session(&scope_domain_key("mon-b", "ws-3"))
            .is_some_and(|s| s.has_pending())
    );
    // Ordinary stale empty pruned with its idle domain dropped.
    assert!(
        !state
            .workspaces
            .workspace_ids("mon-a")
            .contains(&"ws-2".to_owned())
    );
    assert!(
        state
            .engine
            .session(&scope_domain_key("mon-a", "ws-2"))
            .is_none()
    );
    // Reconnect keeps the still-pending session (relocation still refuses).
    sync(&mut state, &areas());
    assert!(
        state
            .engine
            .session(&scope_domain_key("mon-b", "ws-3"))
            .is_some_and(|s| s.has_pending())
    );
}

#[test]
fn shared_orphan_prune_keeps_pending_and_nonempty() {
    let mut state = converged_shared_state();
    let keep = key(0x611, 62);
    stage_member(&mut state, "mon-a", "ws-1", &keep);
    stage_member(&mut state, "mon-b", "ws-1", &keep);
    // Removed-output sessions: idle-empty orphan, real drag flight, and a
    // plain nonempty layout, all under still-live logical ids.
    seed_empty_session(&mut state, "mon-b", "ws-1");
    seed_layout_session(&mut state, "mon-b", "ws-2", "drag-w");
    stage_drag(&mut state, "mon-b", "ws-2", "drag-w");
    seed_layout_session(&mut state, "mon-b", "ws-3", "kept-w");
    // Survivor sibling domains (never touched by the prune below).
    seed_layout_session(&mut state, "mon-a", "ws-1", "sib-a1");
    seed_layout_session(&mut state, "mon-a", "ws-2", "sib-a2");
    sync(&mut state, &only_a());
    assert!(state.workspaces.displaced_snapshot().is_empty());
    // Idle-empty orphan domain for the removed output drops even though
    // ws-1 stays live on the survivor.
    assert!(
        state
            .engine
            .session(&scope_domain_key("mon-b", "ws-1"))
            .is_none()
    );
    // In-flight drag and plain nonempty layouts persist unmapped.
    assert!(
        state
            .engine
            .session(&scope_domain_key("mon-b", "ws-2"))
            .is_some_and(|s| s.has_drag())
    );
    assert!(session_nonempty(&state, "mon-b", "ws-3"));
    // Survivor siblings retained with their own layouts.
    assert!(session_nonempty(&state, "mon-a", "ws-1"));
    assert!(session_nonempty(&state, "mon-a", "ws-2"));
}

#[test]
fn shared_divergent_source_id_merges_without_orphan() {
    let mut state = converged_shared_state();
    // Transiently divergent union: mon-b carries an occupied id the survivor
    // never converged (startup-order residue).
    assert!(state.workspaces.ensure_workspace_id("mon-b", "ws-9"));
    let mx = key(0x621, 63);
    stage_member(&mut state, "mon-b", "ws-9", &mx);
    state.scope_owner.sync_shared_order(&state.workspaces);
    assert!(
        state
            .scope_owner
            .shared_order()
            .contains(&"ws-9".to_owned())
    );
    sync(&mut state, &only_a());
    // The destination converges the logical id BEFORE the origin drops, so
    // the member re-homes instead of stranding on a removed output.
    assert!(
        state
            .workspaces
            .workspace_ids("mon-a")
            .contains(&"ws-9".to_owned())
    );
    assert_eq!(
        state
            .workspaces
            .member_loc(&mx)
            .map(|l| { (l.output.clone(), l.workspace.clone()) }),
        Some(("mon-a".to_owned(), "ws-9".to_owned()))
    );
    assert!(
        state
            .workspaces
            .workspace_members("mon-a", "ws-9")
            .contains(&mx)
    );
}

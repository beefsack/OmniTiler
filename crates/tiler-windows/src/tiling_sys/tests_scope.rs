#![forbid(unsafe_code)]
//! Scoped selection/history adapter tests (item 12 stage B).
//!
//! Path-included from `tiling_sys.rs` (`#[path]`), so `super` is the
//! `tiling_sys` module and private seams (`apply_live_settings`,
//! `rmax03_adapter_tests::test_state`) stay reachable. Holds the prior
//! startup-adoption tests plus the new scoped selection/history tests so
//! `tiling_sys.rs` does not grow with logic/tests.

use crate::settings::{LiveSettings, Settings};
use crate::workspace::ManagedWorkspaces;
use crate::workspace_scope_owner::{
    TrailingResolve, WorkspaceScopeOwner, live_union, mapping_from_workspaces,
};
use tiler_core::workspace_scope::WorkspaceMode::{GlobalUnique, PerOutputLocal, Shared};

fn aligned_state() -> super::TileLoop {
    use crate::settings::{build_disabled, build_remap, tile_options_from_settings};
    let mut state = super::rmax03_adapter_tests::test_state();
    state.log_path = std::path::PathBuf::from("NUL");
    let base = tile_options_from_settings(&Settings::default());
    state.border = base.border;
    state.underlay = base.underlay;
    state.keyboard = crate::snapkey::KeyboardConfig {
        takeover: true,
        allow_win_l: false,
    };
    state.last_remap = build_remap(&Settings::default());
    state.last_disabled = build_disabled(&Settings::default());
    state.settings_live = Some(LiveSettings::fresh(Settings::default(), None));
    state.scope_owner =
        crate::workspace_scope_owner::WorkspaceScopeOwner::adopt_startup(startup_adopted(&state));
    state
}

fn startup_adopted(state: &super::TileLoop) -> tiler_core::workspace_scope::WorkspaceMode {
    crate::workspace_scope_owner::startup_mode(state.settings_live.as_ref())
}

fn test_identity() -> crate::model::ProcessIdentity {
    crate::model::ProcessIdentity {
        pid: 4242,
        process_creation: "test-creation".to_owned(),
        user_sid: "S-1-5-test".to_owned(),
        session_id: 1,
        exe_path: "test.exe".to_owned(),
    }
}

#[test]
fn mode_only_edit_reports_restart_without_live_adoption() {
    let tmp = std::env::temp_dir().join(format!(
        "tiler-scope-drift-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let store = crate::storage::LedgerStore::open(&tmp).expect("ledger temp dir");
    let mut state = aligned_state();
    let mut snap_want = true;
    let mut incoming = Settings::default();
    incoming.core.workspace.mode = "global-unique".to_owned();
    incoming.revision = 8;
    let changed = super::apply_live_settings(
        &mut state,
        &tmp,
        &test_identity(),
        &store,
        &mut snap_want,
        incoming,
        None,
    );
    assert!(
        !changed,
        "mode-only drift must not wake the pump or rebuild"
    );
    assert_eq!(
        state.scope_owner.adopted(),
        tiler_core::workspace_scope::WorkspaceMode::PerOutputLocal,
        "enforced mode stays startup-adopted"
    );
    let live = state.settings_live.as_ref().expect("live");
    assert_eq!(
        live.settings.core.workspace.mode, "global-unique",
        "saved pending choice is retained in the document"
    );
    assert_eq!(live.status, "saved:8+scope-restart:global-unique");
    drop(store);
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn same_mode_edit_reports_no_drift_with_legacy_status() {
    let tmp = std::env::temp_dir().join(format!(
        "tiler-scope-nodrift-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let store = crate::storage::LedgerStore::open(&tmp).expect("ledger temp dir");
    let mut state = aligned_state();
    let mut snap_want = true;
    let incoming = Settings {
        revision: 8,
        ..Settings::default()
    };
    let changed = super::apply_live_settings(
        &mut state,
        &tmp,
        &test_identity(),
        &store,
        &mut snap_want,
        incoming,
        None,
    );
    assert!(!changed, "mode-identical edit changes nothing");
    assert_eq!(
        state.scope_owner.adopted(),
        tiler_core::workspace_scope::WorkspaceMode::PerOutputLocal
    );
    let live = state.settings_live.as_ref().expect("live");
    assert_eq!(live.status, "saved:8");
    drop(store);
    let _ = std::fs::remove_dir_all(&tmp);
}

fn two_output_fixture() -> ManagedWorkspaces {
    let mut spaces = ManagedWorkspaces::new();
    spaces.ensure_output("mon-a");
    spaces.ensure_output("mon-b");
    spaces
}

#[test]
fn local_scoped_resolvers_match_managed_equivalents() {
    let spaces = two_output_fixture();
    let owner = WorkspaceScopeOwner::adopt_startup(PerOutputLocal);
    for output in ["mon-a", "mon-b"] {
        for index in 1..=9u8 {
            assert_eq!(
                owner.resolve_numbered(&spaces, output, index),
                spaces.resolve_send(output, index),
                "numbered {index} on {output}"
            );
        }
        assert_eq!(owner.resolve_numbered(&spaces, output, 0), None);
        for delta in [-1, 1] {
            assert_eq!(
                owner.resolve_relative(&spaces, output, delta),
                spaces.resolve_relative(output, delta),
                "relative {delta} on {output}"
            );
        }
        assert_eq!(owner.resolve_relative(&spaces, output, 0), None);
        let empty_sticky = std::collections::BTreeSet::new();
        assert_eq!(owner.trailing_reuse_id(&spaces, output, &empty_sticky), {
            let ids = spaces.workspace_ids(output);
            let last = ids.last().cloned();
            last.filter(|id| spaces.workspace_members(output, id).is_empty())
        });
    }
    // Local trailing select preserves the mutating reuse path.
    let mut spaces = two_output_fixture();
    let mut owner = WorkspaceScopeOwner::adopt_startup(PerOutputLocal);
    let before = spaces.workspace_ids("mon-a");
    let empty_sticky = std::collections::BTreeSet::new();
    match owner.resolve_trailing_select(&mut spaces, "mon-a", &empty_sticky) {
        TrailingResolve::Reuse(id) => assert_eq!(id, *before.last().expect("trailing")),
        other => panic!("expected reuse, got {other:?}"),
    }
    assert_eq!(spaces.workspace_ids("mon-a"), before);
    // Local previous starts with no history.
    let mut spaces = two_output_fixture();
    let mut owner = WorkspaceScopeOwner::adopt_startup(PerOutputLocal);
    assert_eq!(owner.resolve_previous(&mut spaces, "mon-a"), None);
}

#[test]
fn local_completed_history_matches_workspace_observation() {
    let mut spaces = two_output_fixture();
    let mut owner = WorkspaceScopeOwner::adopt_startup(PerOutputLocal);
    let first = spaces.active_id("mon-a").expect("active");
    // Same-view never records.
    assert!(!owner.record_completed(&mut spaces, "mon-a", &first));
    assert_eq!(spaces.history_previous("mon-a"), None);
    // Unknown current never records.
    assert!(!owner.record_completed(&mut spaces, "mon-a", "ws-gone"));
    // Completed transitions record into workspaces, isolated per output.
    // `ensure_output` already primed the baseline to the active view, so the
    // first change records immediately.
    let second = spaces
        .workspace_ids("mon-a")
        .into_iter()
        .find(|id| id != &first)
        .expect("second");
    assert!(owner.record_completed(&mut spaces, "mon-a", &second));
    assert_eq!(
        spaces.history_previous("mon-a").as_deref(),
        Some(first.as_str())
    );
    assert!(owner.record_completed(&mut spaces, "mon-a", &first));
    assert_eq!(
        spaces.history_previous("mon-a").as_deref(),
        Some(second.as_str())
    );
    assert_eq!(spaces.history_previous("mon-b"), None);
    assert!(owner.history().previous("mon-a").is_none());
}

#[test]
fn global_ring_matches_local_but_history_is_scoped() {
    let mut spaces = two_output_fixture();
    let local = WorkspaceScopeOwner::adopt_startup(PerOutputLocal);
    let mut global = WorkspaceScopeOwner::adopt_startup(GlobalUnique);
    for output in ["mon-a", "mon-b"] {
        assert_eq!(
            global.scoped_ring(&spaces, output),
            local.scoped_ring(&spaces, output)
        );
        for index in 1..=9u8 {
            assert_eq!(
                global.resolve_numbered(&spaces, output, index),
                local.resolve_numbered(&spaces, output, index),
            );
        }
    }
    let first = spaces.active_id("mon-a").expect("active");
    let second = spaces
        .workspace_ids("mon-a")
        .into_iter()
        .find(|id| id != &first)
        .expect("second");
    // Prime the scoped baseline like the later ensure-output hook will, so the
    // first change records immediately (same as local).
    global.prime_output(&spaces, "mon-a");
    // Same-view and unknown never record.
    assert!(!global.record_completed(&mut spaces, "mon-a", &first));
    assert!(!global.record_completed(&mut spaces, "mon-a", "ws-gone"));
    assert_eq!(global.history().previous("mon-a"), None);
    // Completed transitions record scoped per-output history, never the
    // workspace-local history.
    assert!(global.record_completed(&mut spaces, "mon-a", &second));
    assert_eq!(
        global.history().previous("mon-a").as_deref(),
        Some(first.as_str())
    );
    assert!(global.record_completed(&mut spaces, "mon-a", &first));
    assert_eq!(
        global.history().previous("mon-a").as_deref(),
        Some(second.as_str())
    );
    assert_eq!(global.history().previous("mon-b"), None);
    assert_eq!(spaces.history_previous("mon-a"), None);
    // Previous resolves the scoped entry; the active id itself is a valid
    // already-there no-op target.
    assert_eq!(
        global.resolve_previous(&mut spaces, "mon-a").as_deref(),
        Some(second.as_str())
    );
}

#[test]
fn global_swap_plan_detects_visible_elsewhere_without_effects() {
    let spaces = two_output_fixture();
    let owner = WorkspaceScopeOwner::adopt_startup(GlobalUnique);
    let a = spaces.active_id("mon-a").expect("active a");
    let b = spaces.active_id("mon-b").expect("active b");
    assert_ne!(a, b);
    let plan = owner
        .swap_plan_for(&spaces, "mon-a", &b)
        .expect("visible elsewhere plans a swap");
    assert_eq!(plan.active_output, "mon-a");
    assert_eq!(plan.other_output, "mon-b");
    assert_eq!(plan.target, b);
    assert_eq!(plan.active_current, a);
    // Already there, not visible elsewhere, and unknown targets plan nothing.
    assert_eq!(owner.swap_plan_for(&spaces, "mon-a", &a), None);
    let missing = spaces
        .workspace_ids("mon-a")
        .into_iter()
        .nth(1)
        .expect("second a");
    assert_ne!(missing, b);
    assert_eq!(owner.swap_plan_for(&spaces, "mon-a", &missing), None);
    assert_eq!(owner.swap_plan_for(&spaces, "mon-a", "ws-gone"), None);
    assert_eq!(owner.swap_plan_for(&spaces, "mon-gone", &b), None);
    // Non-global modes never plan swaps.
    let local = WorkspaceScopeOwner::adopt_startup(PerOutputLocal);
    assert_eq!(local.swap_plan_for(&spaces, "mon-a", &b), None);
    let mut shared = WorkspaceScopeOwner::adopt_startup(Shared);
    shared.sync_shared_order(&spaces);
    assert_eq!(shared.swap_plan_for(&spaces, "mon-a", &b), None);
}

#[test]
fn shared_ring_history_and_trailing_interface() {
    let spaces = two_output_fixture();
    let mut owner = WorkspaceScopeOwner::adopt_startup(Shared);
    // Empty until explicitly synced: honest unknown-target, no pretending.
    assert!(owner.shared_ring(&spaces).is_empty());
    assert_eq!(owner.resolve_numbered(&spaces, "mon-a", 1), None);
    assert_eq!(owner.resolve_relative(&spaces, "mon-a", 1), None);
    assert_eq!(owner.resolve_previous(&mut spaces.clone(), "mon-a"), None);
    owner.sync_shared_order(&spaces);
    let mapping = mapping_from_workspaces(&spaces);
    let union = live_union(&mapping);
    assert_eq!(*owner.shared_order(), union);
    assert_eq!(owner.shared_ring(&spaces), union);
    assert_eq!(owner.scoped_ring(&spaces, "mon-a"), union);
    assert_eq!(owner.scoped_ring(&spaces, "mon-b"), union);
    // Numbered resolves positionally over the one logical ring.
    assert_eq!(
        owner.resolve_numbered(&spaces, "mon-a", 1).as_deref(),
        union.first().map(String::as_str)
    );
    assert_eq!(owner.resolve_numbered(&spaces, "mon-a", 0), None);
    // Relative resolves from the requesting output's bound current.
    let current_a = spaces.active_id("mon-a").expect("active a");
    let at = union
        .iter()
        .position(|id| id == &current_a)
        .expect("current in ring");
    let next = union[(at + 1) % union.len()].clone();
    assert_eq!(
        owner.resolve_relative(&spaces, "mon-a", 1).as_deref(),
        Some(next.as_str())
    );
    // Shared history: same-view/unknown never record; completed transitions
    // record one shared entry across output-bound domains. Prime first like
    // the later enable hook will, so the first change records immediately.
    owner.prime_shared_view(&spaces, "mon-a");
    assert!(!owner.record_completed(&mut spaces.clone(), "mon-a", &current_a));
    assert!(owner.history().shared_previous().is_none());
    assert!(!owner.record_completed(&mut spaces.clone(), "mon-a", "ws-gone"));
    let mut spaces = spaces;
    assert!(owner.record_completed(&mut spaces, "mon-a", &next));
    assert_eq!(
        owner.history().shared_previous().as_deref(),
        Some(current_a.as_str())
    );
    assert!(owner.record_completed(&mut spaces, "mon-a", &current_a));
    assert_eq!(
        owner.history().shared_previous().as_deref(),
        Some(next.as_str())
    );
    assert_eq!(
        owner.resolve_previous(&mut spaces, "mon-b").as_deref(),
        Some(next.as_str())
    );
    // Trailing reuse when the shared last id is unoccupied everywhere.
    let empty_sticky = std::collections::BTreeSet::new();
    match owner.resolve_trailing_select(&mut spaces, "mon-a", &empty_sticky) {
        TrailingResolve::Reuse(id) => assert_eq!(id, *union.last().expect("trailing")),
        other => panic!("expected shared reuse, got {other:?}"),
    }
    // Shared select plan covers every output-bound domain; unknown and
    // non-shared targets plan nothing.
    let plan = owner
        .plan_shared_select(&spaces, &next)
        .expect("shared plan");
    assert_eq!(plan.target, next);
    assert_eq!(plan.outputs, spaces.output_keys());
    assert_eq!(owner.plan_shared_select(&spaces, "ws-gone"), None);
    let local = WorkspaceScopeOwner::adopt_startup(PerOutputLocal);
    assert_eq!(local.plan_shared_select(&spaces, &next), None);
}

#[test]
fn focus_alone_never_records_history() {
    let mut spaces = two_output_fixture();
    let mut owner = WorkspaceScopeOwner::adopt_startup(GlobalUnique);
    let first = spaces.active_id("mon-a").expect("active");
    let members = spaces.workspace_members("mon-a", &first);
    // No members in the synthetic fixture, so there is nothing to foreground;
    // the invariant is structural: `note_foreground` has no history path,
    // and same-view completion never records.
    for key in members {
        spaces.note_foreground(&key);
    }
    assert_eq!(owner.history().previous("mon-a"), None);
    assert_eq!(owner.history().baseline("mon-a"), None);
    assert!(!owner.record_completed(&mut spaces, "mon-a", &first));
}

#[test]
fn mapping_live_union_and_shared_sync_are_deterministic() {
    let spaces = two_output_fixture();
    let mapping = mapping_from_workspaces(&spaces);
    assert_eq!(
        mapping.keys().cloned().collect::<Vec<_>>(),
        spaces.output_keys()
    );
    for output in spaces.output_keys() {
        assert_eq!(mapping.get(&output), Some(&spaces.workspace_ids(&output)));
    }
    let union = live_union(&mapping);
    let mut seen = std::collections::BTreeSet::new();
    for id in &union {
        assert!(seen.insert(id.clone()), "duplicate {id} in live union");
    }
    let mut owner = WorkspaceScopeOwner::adopt_startup(Shared);
    owner.sync_shared_order(&spaces);
    assert_eq!(*owner.shared_order(), union);
    owner.prime_shared_view(&spaces, "mon-a");
    owner.discard_scoped_output("mon-a");
    assert_eq!(owner.history().previous("mon-a"), None);
}

// ---- item 12 stage B scope-effects adapter tests ----
//
// Two-output rigs over the real Engine plus scripted recovery/observation
// seams (never live HWNDs): leaving sets stage pre-hidden flag-only so the
// hide phase is a no-op exactly like the in-tree select fixture, entering
// members stage claim-free so reveals unflag with no native calls, and
// third members stage real ledger claims to prove recovery ownership
// survives on both outputs. Geometry applies stay stubbed through the
// injected observation (no native moves; per-side writables recorded).

use tiler_core::directional::{OutputId, WindowId, WorkspaceId};

const SCOPE_BOUNDS: tiler_core::geometry::Rect = tiler_core::geometry::Rect {
    x: 0,
    y: 0,
    w: 800,
    h: 600,
};
const SCOPE_BOUNDS_B: tiler_core::geometry::Rect = tiler_core::geometry::Rect {
    x: 800,
    y: 0,
    w: 800,
    h: 600,
};

fn scope_key(hwnd: u64, pid: u32) -> crate::workspace::WindowKey {
    crate::workspace::WindowKey {
        hwnd,
        pid,
        creation: format!("c{hwnd:016x}"),
    }
}

fn scope_stage_member(
    state: &mut super::TileLoop,
    output: &str,
    workspace: &str,
    key: &crate::workspace::WindowKey,
    token: &str,
    tag: &str,
) {
    assert!(
        state
            .workspaces
            .assign(key.clone(), output, workspace, false),
        "assign {token} to ({output},{workspace})"
    );
    state.member_tokens.insert(key.clone(), token.to_owned());
    state.member_identity.insert(
        key.clone(),
        crate::model::ProcessIdentity {
            pid: key.pid,
            process_creation: key.creation.clone(),
            user_sid: "S-1-5-test".to_owned(),
            session_id: 1,
            exe_path: "test.exe".to_owned(),
        },
    );
    state.member_tags.insert(key.clone(), tag.to_owned());
    super::inject_pending_tag(key.hwnd, Some(tag.to_owned()));
}

fn scope_observed(
    key: &crate::workspace::WindowKey,
    token: &str,
    frame: tiler_core::geometry::Rect,
) -> super::ObservedWindow {
    super::ObservedWindow {
        hwnd: key.hwnd,
        token: token.to_owned(),
        outer: frame,
        visible: frame,
        insets: crate::tiling::FrameInsets::default(),
        identity: crate::tiling::ObservedTarget {
            hwnd: key.hwnd,
            pid: key.pid,
            process_creation: key.creation.clone(),
            exe_path: "test.exe".to_owned(),
            user_sid: "S-1-5-test".to_owned(),
            session_id: 1,
            tag: String::new(),
        },
        facts: crate::tiling::WindowFacts {
            visible: true,
            minimized: false,
            maximized: false,
            cloaked: false,
            elevated: false,
            shell: false,
            tool_window: false,
            owned: false,
            captionless_fullscreen: false,
            no_activate: false,
            dialog: false,
        },
    }
}

/// Stage one real ledger claim plus the hidden flag (exactly what
/// `workspace_hide_one` commits): the recovery-ownership fixture for
/// members that must survive a transition untouched.
fn scope_stage_claim(state: &mut super::TileLoop, key: &crate::workspace::WindowKey, tag: &str) {
    let claim = crate::model::WindowIdentity {
        hwnd: key.hwnd,
        process: crate::model::ProcessIdentity {
            pid: key.pid,
            process_creation: key.creation.clone(),
            user_sid: "S-1-5-test".to_owned(),
            session_id: 1,
            exe_path: "test.exe".to_owned(),
        },
        tag: tag.to_owned(),
        kind: crate::model::WindowClaimKind::Product,
        show: crate::model::WindowShowState {
            minimized: false,
            maximized: false,
        },
    };
    state.hidden_claims.insert(
        key.clone(),
        super::HiddenRecord {
            claim,
            iconic: false,
        },
    );
    state.workspaces.set_hidden(key, true);
}

fn scope_seed_session(
    state: &mut super::TileLoop,
    output: &str,
    workspace: &str,
    rows: &[(&str, tiler_core::geometry::Rect)],
    focused: Option<&str>,
) {
    let areas = vec![
        super::MonitorArea {
            device: "mon-a".to_owned(),
            work: SCOPE_BOUNDS,
            full: SCOPE_BOUNDS,
        },
        super::MonitorArea {
            device: "mon-b".to_owned(),
            work: SCOPE_BOUNDS_B,
            full: SCOPE_BOUNDS_B,
        },
    ];
    let (domain, key) =
        super::workspace_domain_for(output, workspace, &areas, state.inner_gap, state.outer_gap)
            .expect("seed domain");
    let event = crate::tiling::build_reconcile_event_for(
        &state.owner.clone(),
        &state.generation.clone(),
        &tiler_core::ids::CorrelationId::parse("scope-seed").expect("correlation"),
        0,
        1,
        &domain,
        &key,
        state.outer_gap,
        &rows
            .iter()
            .map(|(token, rect)| {
                (
                    WindowId((*token).to_owned()),
                    *rect,
                    tiler_core::size_hints::WindowSizeHints::none(),
                )
            })
            .collect::<Vec<_>>(),
        focused.map(|token| WindowId(token.to_owned())).as_ref(),
    );
    let reply = state.engine.handle(&event);
    assert!(
        matches!(
            reply,
            tiler_core::boundary::CoreReply::Projection(_)
                | tiler_core::boundary::CoreReply::Tiled(_)
        ),
        "seed ({output},{workspace}) converges, got {reply:?}"
    );
}

fn scope_ctx(state: &super::TileLoop) -> super::ActionCtx {
    super::ActionCtx {
        correlation: "act-scope-1".to_owned(),
        tick: state.tick,
        queued_at: std::time::Instant::now(),
        start: std::time::Instant::now(),
    }
}

fn scope_tail_seams(observed: Vec<super::ObservedWindow>) {
    super::inject_pending_observe(observed, Vec::new(), false);
    super::inject_pending_suspend(Some(false));
    super::inject_pending_elevated(Some(false));
    super::inject_pending_focus(Some("focus-ok"));
}

fn state_overlay(state: &mut super::TileLoop, token: &str) {
    state.overlay_now.insert(token.to_owned(), (false, false));
}

fn scope_tmp(name: &str) -> (crate::storage::LedgerStore, std::path::PathBuf) {
    let tmp = std::env::temp_dir().join(format!(
        "tiler-scope-fx-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let store = crate::storage::LedgerStore::open(&tmp).expect("ledger temp dir");
    (store, tmp)
}

struct ScopeRig {
    state: super::TileLoop,
    me: crate::model::ProcessIdentity,
    store: crate::storage::LedgerStore,
    dir: std::path::PathBuf,
    areas: Vec<super::MonitorArea>,
    fulls: Vec<tiler_core::geometry::Rect>,
    a: String,
    b: String,
    ka: crate::workspace::WindowKey,
    kb: crate::workspace::WindowKey,
}

/// Two-output scope rig in `mode`: mon-a shows `a` with `ka`, mon-b shows
/// `b` with `kb` (both pre-hidden flag-only), plus real Engine sessions for
/// both current views. Third members with real claims stage separately per
/// test through `scope_stage_claim`.
fn scope_two_output_rig(mode: tiler_core::workspace_scope::WorkspaceMode) -> ScopeRig {
    super::clear_pending_seams();
    let mut state = super::rmax03_adapter_tests::test_state();
    state.log_path = std::path::PathBuf::from("NUL");
    let me = test_identity();
    let (store, dir) = scope_tmp("two");
    state.workspaces.ensure_output("mon-a");
    state.workspaces.ensure_output("mon-b");
    let a = state.workspaces.workspace_ids("mon-a")[0].clone();
    let b = state.workspaces.workspace_ids("mon-b")[0].clone();
    assert_ne!(a, b, "globally unique ids across outputs");
    state.workspaces.activate("mon-a", &a);
    state.workspaces.activate("mon-b", &b);
    let ka = scope_key(0x601, 1601);
    let kb = scope_key(0x602, 1602);
    scope_stage_member(&mut state, "mon-a", &a, &ka, "wA1", "tag-a1");
    scope_stage_member(&mut state, "mon-b", &b, &kb, "wB1", "tag-b1");
    state.workspaces.set_hidden(&ka, true);
    state.workspaces.set_hidden(&kb, true);
    state.overlay_now.insert("wA1".to_owned(), (false, false));
    state.overlay_now.insert("wB1".to_owned(), (false, false));
    let areas = vec![
        super::MonitorArea {
            device: "mon-a".to_owned(),
            work: SCOPE_BOUNDS,
            full: SCOPE_BOUNDS,
        },
        super::MonitorArea {
            device: "mon-b".to_owned(),
            work: SCOPE_BOUNDS_B,
            full: SCOPE_BOUNDS_B,
        },
    ];
    scope_seed_session(
        &mut state,
        "mon-a",
        &a,
        &[("wA1", SCOPE_BOUNDS)],
        Some("wA1"),
    );
    scope_seed_session(
        &mut state,
        "mon-b",
        &b,
        &[("wB1", SCOPE_BOUNDS)],
        Some("wB1"),
    );
    state.scope_owner = WorkspaceScopeOwner::adopt_startup(mode);
    ScopeRig {
        state,
        me,
        store,
        dir,
        areas,
        fulls: vec![SCOPE_BOUNDS, SCOPE_BOUNDS_B],
        a,
        b,
        ka,
        kb,
    }
}

fn scope_domain_key(output: &str, workspace: &str) -> tiler_core::session::DomainKey {
    tiler_core::session::DomainKey {
        output: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
    }
}

#[test]
fn swap_entries_matches_core_assignment_contract() {
    let mut spaces = two_output_fixture();
    // Grow both rings to four ids: occupy the trailing empty so the
    // trailing resolver appends a fresh id, twice per output.
    for (output, base) in [("mon-a", 0x710u64), ("mon-b", 0x720u64)] {
        for n in 0..2u64 {
            let ids = spaces.workspace_ids(output);
            let last = ids.last().cloned().expect("trailing");
            assert!(spaces.assign(scope_key(base + n, 1710 + n as u32), output, &last, false));
            let (_, created) = spaces.resolve_send_trailing(output).expect("append");
            assert!(created, "fresh id appended on {output}");
        }
    }
    let ids_a = spaces.workspace_ids("mon-a");
    let ids_b = spaces.workspace_ids("mon-b");
    assert_eq!(ids_a.len(), 4, "four-id ring on mon-a");
    assert_eq!(ids_b.len(), 4, "four-id ring on mon-b");
    let (a, b) = (ids_a[0].clone(), ids_b[0].clone());
    spaces.activate("mon-a", &a);
    spaces.activate("mon-b", &b);
    let ka = scope_key(0x701, 1701);
    let kb = scope_key(0x702, 1702);
    let sticky = scope_key(0x703, 1703);
    assert!(spaces.assign(ka.clone(), "mon-a", &a, false));
    assert!(spaces.assign(kb.clone(), "mon-b", &b, false));
    assert!(spaces.assign(sticky.clone(), "mon-a", &a, false));
    spaces.note_foreground(&ka);
    spaces.note_foreground(&kb);
    let token_a = spaces.workspace_token("mon-a", &a);
    let token_b = spaces.workspace_token("mon-b", &b);
    let sticky_set: std::collections::BTreeSet<crate::workspace::WindowKey> =
        [sticky.clone()].into_iter().collect();
    // The core plan is the oracle: assignments after the native transfer
    // must equal `apply_swap_assignments` on the pre-swap mapping exactly.
    let plan = tiler_core::workspace_scope::SwapPlan {
        target: b.clone(),
        active_output: "mon-a".to_owned(),
        active_current: a.clone(),
        other_output: "mon-b".to_owned(),
    };
    let expected = tiler_core::workspace_scope::apply_swap_assignments(
        &mapping_from_workspaces(&spaces),
        &plan,
    );
    assert!(spaces.swap_entries("mon-a", &a, "mon-b", &b, &sticky_set));
    // Both current views reflect the plan.
    assert_eq!(spaces.active_id("mon-a").as_deref(), Some(b.as_str()));
    assert_eq!(spaces.active_id("mon-b").as_deref(), Some(a.as_str()));
    // Assignments equal the core mapping on every output: survivors keep
    // ring order, swapped ids append to the destination scope ends.
    for output in spaces.output_keys() {
        assert_eq!(
            spaces.workspace_ids(&output),
            *expected.get(&output).expect("planned output"),
            "scope {output} matches the core contract"
        );
    }
    assert_eq!(
        spaces.workspace_ids("mon-a"),
        vec![
            ids_a[1].clone(),
            ids_a[2].clone(),
            ids_a[3].clone(),
            b.clone()
        ]
    );
    assert_eq!(
        spaces.workspace_ids("mon-b"),
        vec![
            ids_b[1].clone(),
            ids_b[2].clone(),
            ids_b[3].clone(),
            a.clone()
        ]
    );
    // Entry tokens traveled with the entries.
    assert_eq!(spaces.workspace_token("mon-a", &b), token_b);
    assert_eq!(spaces.workspace_token("mon-b", &a), token_a);
    // Member output rekeyed by id; workspace membership and visibility kept.
    assert_eq!(
        spaces.member_loc(&ka),
        Some(&crate::workspace::MemberLoc {
            output: "mon-b".to_owned(),
            workspace: a.clone(),
            hidden: false,
        })
    );
    assert_eq!(
        spaces.member_loc(&kb),
        Some(&crate::workspace::MemberLoc {
            output: "mon-a".to_owned(),
            workspace: b.clone(),
            hidden: false,
        })
    );
    assert!(spaces.workspace_members("mon-b", &a).contains(&ka));
    assert!(spaces.workspace_members("mon-a", &b).contains(&kb));
    // Remembered focus traveled with the entries (non-sticky only).
    assert_eq!(spaces.remembered_focus("mon-b", &a).as_ref(), Some(&ka));
    assert_eq!(spaces.remembered_focus("mon-a", &b).as_ref(), Some(&kb));
    // Sticky stayed homed on its output under the incoming entry.
    assert_eq!(
        spaces.member_loc(&sticky),
        Some(&crate::workspace::MemberLoc {
            output: "mon-a".to_owned(),
            workspace: b.clone(),
            hidden: false,
        })
    );
    assert!(spaces.workspace_members("mon-a", &b).contains(&sticky));
    assert!(!spaces.workspace_members("mon-b", &a).contains(&sticky));
    // A sticky remembered-focus entry clears rather than carries: note the
    // sticky on its new home, swap back, and the restored views keep
    // non-sticky focus while the sticky entry is gone.
    spaces.note_foreground(&sticky);
    assert!(spaces.swap_entries("mon-a", &b, "mon-b", &a, &sticky_set));
    assert_eq!(spaces.active_id("mon-a").as_deref(), Some(a.as_str()));
    assert_eq!(spaces.active_id("mon-b").as_deref(), Some(b.as_str()));
    assert_eq!(spaces.remembered_focus("mon-a", &a).as_ref(), Some(&ka));
    // The sticky note overwrote kb and then cleared on strip: never carried,
    // never restored.
    assert_eq!(spaces.remembered_focus("mon-b", &b), None);
    assert_eq!(
        spaces.member_loc(&sticky),
        Some(&crate::workspace::MemberLoc {
            output: "mon-a".to_owned(),
            workspace: a.clone(),
            hidden: false,
        })
    );
    // Refusals change nothing.
    let before_a = spaces.workspace_ids("mon-a");
    let before_b = spaces.workspace_ids("mon-b");
    assert!(!spaces.swap_entries("mon-a", &a, "mon-b", "ws-gone", &sticky_set));
    assert!(!spaces.swap_entries("mon-a", &a, "mon-a", &b, &sticky_set));
    assert!(!spaces.swap_entries("mon-gone", &a, "mon-b", &b, &sticky_set));
    assert_eq!(spaces.workspace_ids("mon-a"), before_a);
    assert_eq!(spaces.workspace_ids("mon-b"), before_b);
}

#[test]
fn swap_entries_strips_requester_owned_target_per_plan() {
    // The plan derives from observed current views, never from assumed entry
    // homing: here the target is listed (owned) on the requesting output but
    // shown on the other output. The transfer must still strip both ids
    // everywhere and append per the plan instead of swapping presumed owners.
    let mut spaces = two_output_fixture();
    let a = spaces.workspace_ids("mon-a")[0].clone();
    let b = spaces.workspace_ids("mon-b")[0].clone();
    spaces.activate("mon-a", &a);
    spaces.activate("mon-b", &b);
    assert!(spaces.ensure_workspace_id("mon-a", &b));
    assert_eq!(
        spaces
            .workspace_ids("mon-a")
            .iter()
            .filter(|id| *id == &b)
            .count(),
        1,
        "requester owns the target id"
    );
    let kb = scope_key(0x704, 1704);
    assert!(spaces.assign(kb.clone(), "mon-b", &b, false));
    let empty: std::collections::BTreeSet<crate::workspace::WindowKey> =
        std::collections::BTreeSet::new();
    let plan = tiler_core::workspace_scope::SwapPlan {
        target: b.clone(),
        active_output: "mon-a".to_owned(),
        active_current: a.clone(),
        other_output: "mon-b".to_owned(),
    };
    let expected = tiler_core::workspace_scope::apply_swap_assignments(
        &mapping_from_workspaces(&spaces),
        &plan,
    );
    assert!(spaces.swap_entries("mon-a", &a, "mon-b", &b, &empty));
    assert_eq!(spaces.active_id("mon-a").as_deref(), Some(b.as_str()));
    assert_eq!(spaces.active_id("mon-b").as_deref(), Some(a.as_str()));
    for output in spaces.output_keys() {
        assert_eq!(
            spaces.workspace_ids(&output),
            *expected.get(&output).expect("planned output"),
            "scope {output} matches the core contract"
        );
    }
    // No duplication: each swapped id lives on exactly its plan destination.
    assert_eq!(
        spaces
            .workspace_ids("mon-a")
            .iter()
            .filter(|id| *id == &b)
            .count(),
        1
    );
    assert!(!spaces.workspace_ids("mon-b").contains(&b));
    assert!(!spaces.workspace_ids("mon-a").contains(&a));
    // The shown member rekeyed to the requesting output with the entry.
    assert_eq!(
        spaces
            .member_loc(&kb)
            .map(|loc| (loc.output.as_str(), loc.workspace.as_str())),
        Some(("mon-a", b.as_str()))
    );
    assert!(spaces.workspace_members("mon-a", &b).contains(&kb));
}

#[test]
fn ensure_workspace_id_converges_shared_entry() {
    let mut spaces = two_output_fixture();
    let a = spaces.workspace_ids("mon-a")[0].clone();
    let active_before = spaces.active_id("mon-a");
    // Unknown output never creates.
    assert!(!spaces.ensure_workspace_id("mon-gone", &a));
    // Already-present id never duplicates and never activates.
    assert!(!spaces.ensure_workspace_id("mon-a", &a));
    assert_eq!(spaces.active_id("mon-a"), active_before);
    let before = spaces.workspace_ids("mon-a").len();
    // A live id from the sibling output converges with a fresh unique token.
    let b = spaces.workspace_ids("mon-b")[0].clone();
    assert!(spaces.ensure_workspace_id("mon-a", &b));
    assert!(!spaces.ensure_workspace_id("mon-a", &b));
    let ids = spaces.workspace_ids("mon-a");
    assert_eq!(ids.len(), before + 1);
    assert_eq!(ids.last().map(String::as_str), Some(b.as_str()));
    assert_eq!(spaces.active_id("mon-a"), active_before);
    assert!(spaces.workspace_members("mon-a", &b).is_empty());
    assert_ne!(
        spaces.workspace_token("mon-a", &b),
        spaces.workspace_token("mon-b", &b),
        "converged entry mints its own token"
    );
}

#[test]
fn global_swap_transfers_views_members_engine_and_both_histories() {
    let mut rig = scope_two_output_rig(GlobalUnique);
    // Project remembered focus rides the entries; focus alone records no
    // history anywhere.
    rig.state.workspaces.note_foreground(&rig.ka);
    rig.state.workspaces.note_foreground(&rig.kb);
    assert_eq!(rig.state.scope_owner.history().previous("mon-a"), None);
    assert_eq!(rig.state.scope_owner.history().previous("mon-b"), None);
    assert_eq!(rig.state.workspaces.history_previous("mon-a"), None);
    // Third members with real recovery claims on the sibling workspaces.
    let a1 = rig.state.workspaces.workspace_ids("mon-a")[1].clone();
    let b1 = rig.state.workspaces.workspace_ids("mon-b")[1].clone();
    let kc = scope_key(0x603, 1603);
    let kd = scope_key(0x604, 1604);
    scope_stage_member(&mut rig.state, "mon-a", &a1, &kc, "wC", "tag-kc");
    scope_stage_member(&mut rig.state, "mon-b", &b1, &kd, "wD", "tag-kd");
    scope_stage_claim(&mut rig.state, &kc, "tag-kc");
    scope_stage_claim(&mut rig.state, &kd, "tag-kd");
    // Prime the scoped baselines like the ensure-output hook so both
    // completed transitions record immediately.
    rig.state
        .scope_owner
        .prime_output(&rig.state.workspaces, "mon-a");
    rig.state
        .scope_owner
        .prime_output(&rig.state.workspaces, "mon-b");
    let pre_b_windows: Vec<WindowId> = rig
        .state
        .engine
        .session(&scope_domain_key("mon-b", &rig.b))
        .expect("seeded (mon-b,b)")
        .domain_snapshot(&OutputId("mon-b".to_owned()), &WorkspaceId(rig.b.clone()))
        .expect("seeded snapshot")
        .windows
        .iter()
        .map(|link| link.window.clone())
        .collect();
    assert_eq!(pre_b_windows, vec![WindowId("wB1".to_owned())]);
    scope_tail_seams(vec![
        scope_observed(&rig.ka, "wA1", SCOPE_BOUNDS),
        scope_observed(&rig.kb, "wB1", SCOPE_BOUNDS),
    ]);
    // The core plan is the oracle: assignments after the native transfer
    // must equal `apply_swap_assignments` on the pre-swap mapping exactly.
    let target = rig.b.clone();
    let plan = rig
        .state
        .scope_owner
        .swap_plan_for(&rig.state.workspaces, "mon-a", &target)
        .expect("swap plan");
    let expected = tiler_core::workspace_scope::apply_swap_assignments(
        &mapping_from_workspaces(&rig.state.workspaces),
        &plan,
    );
    let ctx = scope_ctx(&rig.state);
    let mut observed: Vec<super::ObservedWindow> = Vec::new();
    let effect = super::workspace_do_select(
        &mut rig.state,
        &rig.me,
        &rig.store,
        &rig.dir,
        &rig.fulls,
        &rig.areas,
        &mut observed,
        "mon-a",
        &target,
        &ctx,
        None,
        false,
        false,
    );
    let counters = super::pending_counters();
    assert_eq!(effect.outcome, "ok", "visible-elsewhere swap completes");
    // Both views swapped per the plan; assignments equal the core mapping
    // on every output (survivors keep ring order, swapped ids append to the
    // destination scope ends per the KDE assignment order).
    assert_eq!(
        rig.state.workspaces.active_id("mon-a").as_deref(),
        Some(target.as_str())
    );
    assert_eq!(
        rig.state.workspaces.active_id("mon-b").as_deref(),
        Some(rig.a.as_str())
    );
    for output in rig.state.workspaces.output_keys() {
        let ids = rig.state.workspaces.workspace_ids(&output);
        let planned = expected.get(&output).expect("planned output");
        // The transfer matches the core contract exactly; the scoped tail
        // then converges trailing lifecycle past it (at most one spare
        // append, same as the local select path), never reordering.
        assert!(
            ids.starts_with(planned),
            "scope {output} starts with the core contract"
        );
        assert!(
            ids.len() <= planned.len() + 1,
            "scope {output} converges at most one trailing spare"
        );
    }
    let ids_a = rig.state.workspaces.workspace_ids("mon-a");
    assert_eq!(&ids_a[..2], [a1.clone(), target.clone()]);
    assert_eq!(
        rig.state.workspaces.workspace_ids("mon-b")[..2],
        [b1.clone(), rig.a.clone()]
    );
    assert_eq!(rig.state.active_output, "mon-a");
    // Visibility and member output transferred on both outputs.
    assert!(!rig.state.workspaces.is_hidden(&rig.ka));
    assert!(!rig.state.workspaces.is_hidden(&rig.kb));
    assert_eq!(
        rig.state
            .workspaces
            .member_loc(&rig.ka)
            .map(|loc| (loc.output.as_str(), loc.workspace.as_str())),
        Some(("mon-b", rig.a.as_str()))
    );
    assert_eq!(
        rig.state
            .workspaces
            .member_loc(&rig.kb)
            .map(|loc| (loc.output.as_str(), loc.workspace.as_str())),
        Some(("mon-a", target.as_str()))
    );
    // Remembered focus traveled with the entries.
    assert_eq!(
        rig.state
            .workspaces
            .remembered_focus("mon-b", &rig.a)
            .as_ref(),
        Some(&rig.ka)
    );
    assert_eq!(
        rig.state
            .workspaces
            .remembered_focus("mon-a", &target)
            .as_ref(),
        Some(&rig.kb)
    );
    // Recovery ownership preserved on both outputs: third claims intact with
    // homing untouched, project float/topmost sets untouched.
    assert_eq!(
        rig.state
            .hidden_claims
            .get(&kc)
            .map(|record| record.claim.tag.as_str()),
        Some("tag-kc")
    );
    assert_eq!(
        rig.state
            .hidden_claims
            .get(&kd)
            .map(|record| record.claim.tag.as_str()),
        Some("tag-kd")
    );
    assert_eq!(
        rig.state
            .workspaces
            .member_loc(&kc)
            .map(|loc| (loc.output.as_str(), loc.workspace.as_str())),
        Some(("mon-a", a1.as_str()))
    );
    assert_eq!(
        rig.state
            .workspaces
            .member_loc(&kd)
            .map(|loc| (loc.output.as_str(), loc.workspace.as_str())),
        Some(("mon-b", b1.as_str()))
    );
    // Output-bound Engine layout/domains relocated per workspace id: moved,
    // never reseeded (still exactly two sessions, old keys gone), with the
    // laid-out contents carried onto the new output.
    assert_eq!(rig.state.engine.len(), 2, "relocate moves, never copies");
    assert!(
        rig.state
            .engine
            .session(&scope_domain_key("mon-a", &rig.a))
            .is_none()
    );
    assert!(
        rig.state
            .engine
            .session(&scope_domain_key("mon-b", &target))
            .is_none()
    );
    let moved = rig
        .state
        .engine
        .session(&scope_domain_key("mon-a", &target))
        .expect("relocated (mon-a,b)");
    let moved_windows: Vec<WindowId> = moved
        .domain_snapshot(&OutputId("mon-a".to_owned()), &WorkspaceId(target.clone()))
        .expect("relocated snapshot")
        .windows
        .iter()
        .map(|link| link.window.clone())
        .collect();
    assert_eq!(
        moved_windows, pre_b_windows,
        "layout contents ride the relocation"
    );
    assert!(
        rig.state
            .engine
            .session(&scope_domain_key("mon-b", &rig.a))
            .is_some(),
        "relocated (mon-b,a)"
    );
    // Both completed transitions observed into scoped history; local
    // history stays empty under global scope. Baselines advance on both
    // outputs while previous stays empty on both: the pre-swap views left
    // their output's scope in the transfer, so the core eager-invalidation
    // rule clears them exactly like a removal. The next transition records
    // anew (a swap back then offers the toggle target).
    assert_eq!(
        rig.state.scope_owner.history().baseline("mon-a").as_deref(),
        Some(target.as_str())
    );
    assert_eq!(
        rig.state.scope_owner.history().baseline("mon-b").as_deref(),
        Some(rig.a.as_str())
    );
    assert_eq!(rig.state.scope_owner.history().previous("mon-a"), None);
    assert_eq!(rig.state.scope_owner.history().previous("mon-b"), None);
    assert_eq!(rig.state.workspaces.history_previous("mon-a"), None);
    assert_eq!(rig.state.workspaces.history_previous("mon-b"), None);
    // Focus ran once (requesting output, no stealing) and both sides applied
    // geometry through the stub with per-side writable sets.
    assert_eq!(
        counters.focus, 1,
        "one focus actuation on the requesting output"
    );
    assert_eq!(counters.apply, 2, "geometry converges on both new views");
    let mut writables = super::pending_apply_writables();
    writables.sort();
    assert_eq!(
        writables,
        vec![
            ["wA1".to_owned()].into_iter().collect(),
            ["wB1".to_owned()].into_iter().collect(),
        ],
        "each side writes exactly its own revealed members"
    );
    assert!(
        effect.geometry.is_some(),
        "requesting side carries geometry evidence"
    );
    drop(rig.store);
    let _ = std::fs::remove_dir_all(&rig.dir);
    super::clear_pending_seams();
}

#[test]
fn global_swap_reveal_failure_keeps_both_views_without_history() {
    let mut rig = scope_two_output_rig(GlobalUnique);
    rig.state
        .scope_owner
        .prime_output(&rig.state.workspaces, "mon-a");
    rig.state
        .scope_owner
        .prime_output(&rig.state.workspaces, "mon-b");
    // A real claim on the entering member plus a refusing proof gate fails
    // the native reveal deterministically with no native calls: the swap
    // must stop narrowly with no replay. (On a live run a dead claim would
    // retire instead; fake HWNDs always read dead, so the gate is the
    // honest failure seam here.)
    scope_stage_claim(&mut rig.state, &rig.kb, "tag-b1-real");
    rig.state.allowlist = Some(Vec::new());
    scope_tail_seams(vec![
        scope_observed(&rig.ka, "wA1", SCOPE_BOUNDS),
        scope_observed(&rig.kb, "wB1", SCOPE_BOUNDS),
    ]);
    let ctx = scope_ctx(&rig.state);
    let mut observed: Vec<super::ObservedWindow> = Vec::new();
    let target = rig.b.clone();
    let effect = super::workspace_do_select(
        &mut rig.state,
        &rig.me,
        &rig.store,
        &rig.dir,
        &rig.fulls,
        &rig.areas,
        &mut observed,
        "mon-a",
        &target,
        &ctx,
        None,
        false,
        false,
    );
    assert_eq!(effect.outcome, "partial", "failed reveal stays honest");
    // Both views kept, assignments unmoved, no history on either output.
    assert_eq!(
        rig.state.workspaces.active_id("mon-a").as_deref(),
        Some(rig.a.as_str())
    );
    assert_eq!(
        rig.state.workspaces.active_id("mon-b").as_deref(),
        Some(target.as_str())
    );
    assert_eq!(rig.state.workspaces.workspace_ids("mon-a")[0], rig.a);
    assert_eq!(rig.state.workspaces.workspace_ids("mon-b")[0], target);
    assert_eq!(rig.state.scope_owner.history().previous("mon-a"), None);
    assert_eq!(rig.state.scope_owner.history().previous("mon-b"), None);
    assert_eq!(rig.state.workspaces.history_previous("mon-a"), None);
    // Engine sessions untouched: no relocation, no fresh seeds.
    assert_eq!(rig.state.engine.len(), 2);
    assert!(
        rig.state
            .engine
            .session(&scope_domain_key("mon-a", &rig.a))
            .is_some()
    );
    assert!(
        rig.state
            .engine
            .session(&scope_domain_key("mon-b", &target))
            .is_some()
    );
    assert!(
        rig.state
            .engine
            .session(&scope_domain_key("mon-a", &target))
            .is_none()
    );
    drop(rig.store);
    let _ = std::fs::remove_dir_all(&rig.dir);
    super::clear_pending_seams();
}

#[test]
fn global_swap_relocates_both_layouts_to_destinations_with_counts() {
    let mut rig = scope_two_output_rig(GlobalUnique);
    rig.state
        .scope_owner
        .prime_output(&rig.state.workspaces, "mon-a");
    rig.state
        .scope_owner
        .prime_output(&rig.state.workspaces, "mon-b");
    // Real Engine sessions on both current views with laid-out contents.
    let pre_a_windows: Vec<WindowId> = rig
        .state
        .engine
        .session(&scope_domain_key("mon-a", &rig.a))
        .expect("seeded (mon-a,a)")
        .domain_snapshot(&OutputId("mon-a".to_owned()), &WorkspaceId(rig.a.clone()))
        .expect("seeded snapshot")
        .windows
        .iter()
        .map(|link| link.window.clone())
        .collect();
    let pre_b_windows: Vec<WindowId> = rig
        .state
        .engine
        .session(&scope_domain_key("mon-b", &rig.b))
        .expect("seeded (mon-b,b)")
        .domain_snapshot(&OutputId("mon-b".to_owned()), &WorkspaceId(rig.b.clone()))
        .expect("seeded snapshot")
        .windows
        .iter()
        .map(|link| link.window.clone())
        .collect();
    assert_eq!(pre_a_windows, vec![WindowId("wA1".to_owned())]);
    assert_eq!(pre_b_windows, vec![WindowId("wB1".to_owned())]);
    // Observe the explicit relocation counts directly: the production path
    // logs relocated/retained on the workspace-swap line. Final sessions
    // alone cannot discriminate (the tail reconcile would steal the layout
    // even when the explicit phase misses), so the count assertion owns the
    // destination-order regression.
    let log_path = rig.dir.join("swap-count.log");
    rig.state.log_path = log_path.clone();
    scope_tail_seams(vec![
        scope_observed(&rig.ka, "wA1", SCOPE_BOUNDS),
        scope_observed(&rig.kb, "wB1", SCOPE_BOUNDS),
    ]);
    let ctx = scope_ctx(&rig.state);
    let mut observed: Vec<super::ObservedWindow> = Vec::new();
    let target = rig.b.clone();
    let effect = super::workspace_do_select(
        &mut rig.state,
        &rig.me,
        &rig.store,
        &rig.dir,
        &rig.fulls,
        &rig.areas,
        &mut observed,
        "mon-a",
        &target,
        &ctx,
        None,
        false,
        false,
    );
    assert_eq!(effect.outcome, "ok", "visible-elsewhere swap completes");
    let log_text = std::fs::read_to_string(&log_path).expect("swap log written");
    let mut relocated: Option<u64> = None;
    let mut retained: Option<u64> = None;
    for line in log_text.lines() {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if value.get("event").and_then(|e| e.as_str()) == Some("workspace-swap") {
            relocated = value.get("relocated").and_then(|v| v.as_u64());
            retained = value.get("retained").and_then(|v| v.as_u64());
        }
    }
    assert_eq!(relocated, Some(2), "both layouts relocate explicitly");
    assert_eq!(retained, Some(0), "no retained fallback on the swap line");
    // Both layouts ride to the post-transfer destinations with contents
    // preserved; the pre-transfer keys are gone and nothing copies.
    assert_eq!(rig.state.engine.len(), 2, "relocate moves, never copies");
    assert!(
        rig.state
            .engine
            .session(&scope_domain_key("mon-a", &rig.a))
            .is_none()
    );
    assert!(
        rig.state
            .engine
            .session(&scope_domain_key("mon-b", &target))
            .is_none()
    );
    let moved_a = rig
        .state
        .engine
        .session(&scope_domain_key("mon-b", &rig.a))
        .expect("relocated (mon-b,a)");
    let moved_a_windows: Vec<WindowId> = moved_a
        .domain_snapshot(&OutputId("mon-b".to_owned()), &WorkspaceId(rig.a.clone()))
        .expect("relocated snapshot")
        .windows
        .iter()
        .map(|link| link.window.clone())
        .collect();
    assert_eq!(
        moved_a_windows, pre_a_windows,
        "layout contents ride (mon-b,a)"
    );
    let moved_b = rig
        .state
        .engine
        .session(&scope_domain_key("mon-a", &target))
        .expect("relocated (mon-a,b)");
    let moved_b_windows: Vec<WindowId> = moved_b
        .domain_snapshot(&OutputId("mon-a".to_owned()), &WorkspaceId(target.clone()))
        .expect("relocated snapshot")
        .windows
        .iter()
        .map(|link| link.window.clone())
        .collect();
    assert_eq!(
        moved_b_windows, pre_b_windows,
        "layout contents ride (mon-a,b)"
    );
    drop(rig.store);
    let _ = std::fs::remove_dir_all(&rig.dir);
    super::clear_pending_seams();
}

#[test]
fn shared_select_converges_all_outputs_with_one_history() {
    let mut rig = scope_two_output_rig(Shared);
    rig.state
        .scope_owner
        .sync_shared_order(&rig.state.workspaces);
    rig.state
        .scope_owner
        .prime_shared_view(&rig.state.workspaces, "mon-a");
    let target = rig.b.clone();
    assert!(
        rig.state
            .scope_owner
            .plan_shared_select(&rig.state.workspaces, &target)
            .is_some(),
        "target plans across all live outputs"
    );
    scope_tail_seams(vec![
        scope_observed(&rig.ka, "wA1", SCOPE_BOUNDS),
        scope_observed(&rig.kb, "wB1", SCOPE_BOUNDS),
    ]);
    let ctx = scope_ctx(&rig.state);
    let mut observed: Vec<super::ObservedWindow> = Vec::new();
    let effect = super::workspace_do_select(
        &mut rig.state,
        &rig.me,
        &rig.store,
        &rig.dir,
        &rig.fulls,
        &rig.areas,
        &mut observed,
        "mon-a",
        &target,
        &ctx,
        None,
        false,
        false,
    );
    assert_eq!(effect.outcome, "ok", "shared select converges");
    // The same logical id shows on every live output.
    assert_eq!(
        rig.state.workspaces.active_id("mon-a").as_deref(),
        Some(target.as_str())
    );
    assert_eq!(
        rig.state.workspaces.active_id("mon-b").as_deref(),
        Some(target.as_str())
    );
    assert_eq!(rig.state.active_output, "mon-a");
    // mon-a converged the missing entry; mon-b kept its own domain entry.
    assert!(
        rig.state
            .workspaces
            .workspace_ids("mon-a")
            .contains(&target)
    );
    // One shared history entry; per-output scoped entries stay empty.
    assert_eq!(
        rig.state.scope_owner.history().shared_previous().as_deref(),
        Some(rig.a.as_str())
    );
    assert_eq!(rig.state.scope_owner.history().previous("mon-a"), None);
    assert_eq!(rig.state.scope_owner.history().previous("mon-b"), None);
    // Per-output layout domains preserved: mon-b keeps its session with its
    // laid-out contents, and the memberless converged side seeds nothing
    // (no steal, no ghost session).
    let kept = rig
        .state
        .engine
        .session(&scope_domain_key("mon-b", &target))
        .expect("mon-b keeps its domain");
    let kept_windows: Vec<WindowId> = kept
        .domain_snapshot(&OutputId("mon-b".to_owned()), &WorkspaceId(target.clone()))
        .expect("kept snapshot")
        .windows
        .iter()
        .map(|link| link.window.clone())
        .collect();
    assert_eq!(kept_windows, vec![WindowId("wB1".to_owned())]);
    assert_eq!(rig.state.engine.len(), 2, "no steal, no ghost session");
    assert!(
        rig.state
            .engine
            .session(&scope_domain_key("mon-a", &target))
            .is_none()
    );
    drop(rig.store);
    let _ = std::fs::remove_dir_all(&rig.dir);
    super::clear_pending_seams();
}

#[test]
fn shared_select_memberful_side_without_session_seeds_without_stealing() {
    let mut rig = scope_two_output_rig(Shared);
    // A member homed on the converged entry before its output owns a layout
    // domain: the side must fresh-seed while the sibling session stands.
    let target = rig.b.clone();
    rig.state.workspaces.ensure_workspace_id("mon-a", &target);
    let km = scope_key(0x605, 1605);
    scope_stage_member(&mut rig.state, "mon-a", &target, &km, "wM", "tag-km");
    state_overlay(&mut rig.state, "wM");
    rig.state.workspaces.set_hidden(&km, true);
    rig.state
        .scope_owner
        .sync_shared_order(&rig.state.workspaces);
    rig.state
        .scope_owner
        .prime_shared_view(&rig.state.workspaces, "mon-a");
    // Sanity: only the sibling owns a session for the target.
    assert!(
        rig.state
            .engine
            .session(&scope_domain_key("mon-a", &target))
            .is_none()
    );
    assert!(
        rig.state
            .engine
            .session(&scope_domain_key("mon-b", &target))
            .is_some()
    );
    scope_tail_seams(vec![
        scope_observed(&rig.ka, "wA1", SCOPE_BOUNDS),
        scope_observed(&rig.kb, "wB1", SCOPE_BOUNDS),
        scope_observed(&km, "wM", SCOPE_BOUNDS),
    ]);
    let ctx = scope_ctx(&rig.state);
    let mut observed: Vec<super::ObservedWindow> = Vec::new();
    let effect = super::workspace_do_select(
        &mut rig.state,
        &rig.me,
        &rig.store,
        &rig.dir,
        &rig.fulls,
        &rig.areas,
        &mut observed,
        "mon-a",
        &target,
        &ctx,
        None,
        false,
        false,
    );
    assert_eq!(effect.outcome, "ok");
    assert_eq!(
        rig.state.workspaces.active_id("mon-a").as_deref(),
        Some(target.as_str())
    );
    assert_eq!(
        rig.state.workspaces.active_id("mon-b").as_deref(),
        Some(target.as_str())
    );
    // The sibling session was never stolen: it still lays out its own
    // members on its own output.
    let kept = rig
        .state
        .engine
        .session(&scope_domain_key("mon-b", &target))
        .expect("sibling session survives");
    let kept_windows: Vec<WindowId> = kept
        .domain_snapshot(&OutputId("mon-b".to_owned()), &WorkspaceId(target.clone()))
        .expect("kept snapshot")
        .windows
        .iter()
        .map(|link| link.window.clone())
        .collect();
    assert_eq!(kept_windows, vec![WindowId("wB1".to_owned())]);
    // The memberful side seeded fresh and shows its own member.
    let seeded = rig
        .state
        .engine
        .session(&scope_domain_key("mon-a", &target))
        .expect("memberful side seeded fresh");
    let seeded_windows: Vec<WindowId> = seeded
        .domain_snapshot(&OutputId("mon-a".to_owned()), &WorkspaceId(target.clone()))
        .expect("seeded snapshot")
        .windows
        .iter()
        .map(|link| link.window.clone())
        .collect();
    assert_eq!(seeded_windows, vec![WindowId("wM".to_owned())]);
    assert!(!rig.state.workspaces.is_hidden(&km));
    drop(rig.store);
    let _ = std::fs::remove_dir_all(&rig.dir);
    super::clear_pending_seams();
}

#[test]
fn shared_select_single_output_works() {
    super::clear_pending_seams();
    let mut state = super::rmax03_adapter_tests::test_state();
    state.log_path = std::path::PathBuf::from("NUL");
    let me = test_identity();
    let (store, dir) = scope_tmp("single");
    state.workspaces.ensure_output("mon-a");
    let ids = state.workspaces.workspace_ids("mon-a");
    let (current, target) = (ids[0].clone(), ids[1].clone());
    state.workspaces.activate("mon-a", &current);
    let k1 = scope_key(0x611, 1611);
    let k2 = scope_key(0x612, 1612);
    scope_stage_member(&mut state, "mon-a", &current, &k1, "w1", "tag-1");
    scope_stage_member(&mut state, "mon-a", &target, &k2, "w2", "tag-2");
    state.workspaces.set_hidden(&k1, true);
    state.overlay_now.insert("w1".to_owned(), (false, false));
    state.overlay_now.insert("w2".to_owned(), (false, false));
    let areas = vec![super::MonitorArea {
        device: "mon-a".to_owned(),
        work: SCOPE_BOUNDS,
        full: SCOPE_BOUNDS,
    }];
    // Seed through the shared-mode domain so the session predates selection.
    let (domain, key) =
        super::workspace_domain_for("mon-a", &current, &areas, state.inner_gap, state.outer_gap)
            .expect("seed domain");
    let event = crate::tiling::build_reconcile_event_for(
        &state.owner.clone(),
        &state.generation.clone(),
        &tiler_core::ids::CorrelationId::parse("scope-seed").expect("correlation"),
        0,
        1,
        &domain,
        &key,
        state.outer_gap,
        &[(
            WindowId("w1".to_owned()),
            SCOPE_BOUNDS,
            tiler_core::size_hints::WindowSizeHints::none(),
        )],
        Some(&WindowId("w1".to_owned())),
    );
    let reply = state.engine.handle(&event);
    assert!(matches!(
        reply,
        tiler_core::boundary::CoreReply::Projection(_) | tiler_core::boundary::CoreReply::Tiled(_)
    ));
    let mut owner = WorkspaceScopeOwner::adopt_startup(Shared);
    owner.sync_shared_order(&state.workspaces);
    owner.prime_shared_view(&state.workspaces, "mon-a");
    state.scope_owner = owner;
    scope_tail_seams(vec![
        scope_observed(&k1, "w1", SCOPE_BOUNDS),
        scope_observed(&k2, "w2", SCOPE_BOUNDS),
    ]);
    let ctx = scope_ctx(&state);
    let mut observed: Vec<super::ObservedWindow> = Vec::new();
    let effect = super::workspace_do_select(
        &mut state,
        &me,
        &store,
        &dir,
        &[SCOPE_BOUNDS],
        &areas,
        &mut observed,
        "mon-a",
        &target,
        &ctx,
        None,
        false,
        false,
    );
    assert_eq!(
        effect.outcome, "ok",
        "single-output shared selection completes"
    );
    assert_eq!(
        state.workspaces.active_id("mon-a").as_deref(),
        Some(target.as_str())
    );
    assert!(!state.workspaces.is_hidden(&k2));
    assert_eq!(
        state.scope_owner.history().shared_previous().as_deref(),
        Some(current.as_str()),
        "one shared entry records the completed transition"
    );
    drop(store);
    let _ = std::fs::remove_dir_all(&dir);
    super::clear_pending_seams();
}

#[test]
fn foreground_focus_alone_stays_unrecorded_under_global_scope() {
    let mut rig = scope_two_output_rig(GlobalUnique);
    // A foreground-driven poll with no matching hidden claim is a no-op that
    // records nothing anywhere.
    let polled = super::poll_foreground_workspace(
        &mut rig.state,
        &rig.me,
        &rig.store,
        &rig.dir,
        &rig.fulls,
        &rig.areas,
        &mut Vec::new(),
    );
    assert!(polled.is_none(), "no verified hidden foreground, no select");
    assert_eq!(rig.state.scope_owner.history().previous("mon-a"), None);
    // Foreground focus alone (remembered focus with no view change) never
    // records scoped history.
    rig.state.workspaces.note_foreground(&rig.ka);
    rig.state.workspaces.note_foreground(&rig.kb);
    assert_eq!(rig.state.scope_owner.history().previous("mon-a"), None);
    assert_eq!(rig.state.scope_owner.history().previous("mon-b"), None);
    assert_eq!(rig.state.scope_owner.history().baseline("mon-a"), None);
    // A completed ordinary transition through the same dispatcher records
    // the per-output scoped entry (second workspace on mon-a is unoccupied,
    // so the select converges with no native effects beyond the tail).
    rig.state
        .scope_owner
        .prime_output(&rig.state.workspaces, "mon-a");
    let second = rig.state.workspaces.workspace_ids("mon-a")[1].clone();
    scope_tail_seams(vec![
        scope_observed(&rig.ka, "wA1", SCOPE_BOUNDS),
        scope_observed(&rig.kb, "wB1", SCOPE_BOUNDS),
    ]);
    let ctx = scope_ctx(&rig.state);
    let mut observed: Vec<super::ObservedWindow> = Vec::new();
    let effect = super::workspace_do_select(
        &mut rig.state,
        &rig.me,
        &rig.store,
        &rig.dir,
        &rig.fulls,
        &rig.areas,
        &mut observed,
        "mon-a",
        &second,
        &ctx,
        None,
        false,
        false,
    );
    assert_eq!(effect.outcome, "ok");
    assert_eq!(
        rig.state.scope_owner.history().previous("mon-a").as_deref(),
        Some(rig.a.as_str())
    );
    assert_eq!(rig.state.scope_owner.history().previous("mon-b"), None);
    drop(rig.store);
    let _ = std::fs::remove_dir_all(&rig.dir);
    super::clear_pending_seams();
}

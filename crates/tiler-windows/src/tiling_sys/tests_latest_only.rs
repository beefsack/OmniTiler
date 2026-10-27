#![forbid(unsafe_code)]
//! Windows latest-only workspace-selection scheduling (offline, native-free).
//!
//! Smallest safe subset: pure `Select` bursts order every command through the
//! core once while only the latest reply plans/applies natively. `Send` is a
//! barrier (always the full path; flushes any pending select first).
//! Suspension/elevation fences stay intact: a gated batch dispatches nothing
//! and applies nothing.

use super::rmax03_adapter_tests::test_state;
use super::world_tick::{
    command_apply_count, observation_apply_count, reset_all_apply_counts, total_apply_count,
};
use super::{
    MonitorArea, clear_pending_seams, inject_pending_elevated, inject_pending_elevated_seq,
    inject_pending_foreground, inject_pending_observe, inject_pending_suspend,
    inject_pending_suspend_seq,
};
use crate::snapkey::{
    QueuedWorkspaceHistoryIntent, QueuedWorkspaceIntent, SnapEdge, WorkspaceHistoryOp, WorkspaceOp,
};
use tiler_core::geometry::Rect;
use tiler_core::world_report::TransferKind;

fn fake_me() -> crate::model::ProcessIdentity {
    crate::model::ProcessIdentity {
        pid: 4242,
        process_creation: "proc-4242".to_owned(),
        user_sid: "S-1-5-21-1000".to_owned(),
        session_id: 1,
        exe_path: "C:\\Apps\\app.exe".to_owned(),
    }
}

fn test_ledger(name: &str) -> (std::path::PathBuf, crate::storage::LedgerStore) {
    let dir = std::env::temp_dir().join(format!(
        "tiler-latest-only-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&dir).expect("tmp dir");
    let store = crate::storage::LedgerStore::open(&dir.join("ledger")).expect("ledger");
    (dir, store)
}

fn live_settings() -> crate::settings::LiveSettings {
    let mut settings = crate::settings::Settings::default();
    settings.core.switch_target = "focused-output".to_owned();
    settings.core.fresh_placement = "focused-output-last-focused".to_owned();
    crate::settings::LiveSettings::fresh(settings, None)
}

fn facts() -> crate::tiling::WindowFacts {
    crate::tiling::WindowFacts {
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
    }
}

fn observed_window(hwnd: u64, token: &str, pid: u32, creation: &str) -> super::ObservedWindow {
    let visible = Rect {
        x: 0,
        y: 0,
        w: 200,
        h: 200,
    };
    let outer = Rect {
        x: -8,
        y: -8,
        w: 216,
        h: 216,
    };
    super::ObservedWindow {
        hwnd,
        token: token.to_owned(),
        outer,
        visible,
        insets: crate::tiling::FrameInsets::measure(outer, visible),
        identity: super::ObservedTarget {
            hwnd,
            pid,
            process_creation: creation.to_owned(),
            exe_path: "C:\\Apps\\app.exe".to_owned(),
            user_sid: "S-1-5-21-1000".to_owned(),
            session_id: 1,
            tag: format!("tag-{token}"),
        },
        facts: facts(),
        member_tag: Some(format!("tag-{token}")),
    }
}

fn seed_member(state: &mut super::TileLoop, hwnd: u64, token: &str, pid: u32, creation: &str) {
    let key = crate::native_guards::WindowKey {
        hwnd,
        pid,
        creation: creation.to_owned(),
    };
    state.member_tokens.insert(key.clone(), token.to_owned());
    state
        .member_tags
        .insert(key.clone(), format!("tag-{token}"));
    state.member_identity.insert(
        key,
        crate::model::ProcessIdentity {
            pid,
            process_creation: creation.to_owned(),
            user_sid: "S-1-5-21-1000".to_owned(),
            session_id: 1,
            exe_path: "C:\\Apps\\app.exe".to_owned(),
        },
    );
}
fn area() -> MonitorArea {
    MonitorArea {
        device: "DEV1".to_owned(),
        work: Rect {
            x: 0,
            y: 0,
            w: 800,
            h: 560,
        },
        full: Rect {
            x: 0,
            y: 0,
            w: 800,
            h: 600,
        },
    }
}

fn select(index: u8) -> QueuedWorkspaceIntent {
    QueuedWorkspaceIntent {
        op: WorkspaceOp::Select,
        index,
        follow: false,
        edge: SnapEdge::Down,
        origin: None,
        consumed: true,
        announce: true,
        tick: std::time::Instant::now(),
    }
}

fn send_unmanaged(index: u8) -> QueuedWorkspaceIntent {
    QueuedWorkspaceIntent {
        op: WorkspaceOp::Send,
        index,
        follow: false,
        edge: SnapEdge::Down,
        origin: None,
        consumed: true,
        announce: true,
        tick: std::time::Instant::now(),
    }
}

fn send_linked(
    index: u8,
    hwnd: u64,
    token: &str,
    pid: u32,
    creation: &str,
) -> QueuedWorkspaceIntent {
    QueuedWorkspaceIntent {
        op: WorkspaceOp::Send,
        index,
        follow: false,
        edge: SnapEdge::Down,
        origin: Some(crate::snapkey::SnapOrigin {
            hwnd,
            token: token.to_owned(),
            pid,
            creation: creation.to_owned(),
        }),
        consumed: true,
        announce: true,
        tick: std::time::Instant::now(),
    }
}

fn fulls() -> Vec<Rect> {
    vec![Rect {
        x: 0,
        y: 0,
        w: 800,
        h: 600,
    }]
}

fn seed_two_windows(state: &mut super::TileLoop) -> Vec<super::ObservedWindow> {
    state.ensure_device("DEV1");
    state.settings_live = Some(live_settings());
    seed_member(state, 101, "win-a", 201, "proc-a");
    seed_member(state, 102, "win-b", 202, "proc-b");
    vec![
        observed_window(101, "win-a", 201, "proc-a"),
        observed_window(102, "win-b", 202, "proc-b"),
    ]
}

fn clean_batch_with(observed: Vec<super::ObservedWindow>) {
    inject_pending_observe(observed, Vec::new(), false);
    inject_pending_foreground(Some(0));
    inject_pending_suspend(Some(false));
    inject_pending_elevated(Some(false));
}

#[test]
fn queued_selects_order_every_command_but_apply_only_latest() {
    let mut state = test_state();
    state.keyboard.takeover = true;
    let (dir, store) = test_ledger("burst");
    let areas = vec![area()];
    let observed = seed_two_windows(&mut state);
    // Seed the world through one observation so selects resolve against
    // converged homing/views instead of bootstrap emptiness.
    clean_batch_with(observed.clone());
    let seeded = super::world_tick::run_observation_tick(
        &mut state,
        &fake_me(),
        &store,
        &dir,
        &observed,
        &[],
        &areas,
        0,
        &fulls(),
        Some((100, 100)),
        false,
    );
    assert!(seeded.is_some(), "seed observation must converge");
    // Alternating departures/arrivals: every select changes the view, so
    // every command incorporates fresh (no stale catch-up between them).
    reset_all_apply_counts();
    let cmd_before = state.world.next_command_seq();
    let rev_before = state.world.engine().state_revision();
    clean_batch_with(observed.clone());
    super::workspace_navigation::workspace_tick(
        &mut state,
        &fake_me(),
        &store,
        &dir,
        &fulls(),
        &areas,
        vec![select(2), select(1), select(2)],
        None,
    );
    clear_pending_seams();
    let ordered = state.world.next_command_seq() - cmd_before;
    assert_eq!(ordered, 3, "core must order every queued select once");
    assert_eq!(
        state.world.engine().state_revision() - rev_before,
        3,
        "every select must incorporate fresh"
    );
    assert_eq!(
        command_apply_count(),
        1,
        "only the latest select may plan/apply natively"
    );
    assert_eq!(
        observation_apply_count(),
        0,
        "no synchronous catch-up may hide extra native applies in this burst"
    );
    assert_eq!(
        total_apply_count(),
        1,
        "combined apply passes must equal the single latest native plan"
    );
    let reply = state
        .world
        .last_reply()
        .cloned()
        .expect("latest reply must be retained");
    let wire = state.output_token("DEV1");
    let reply_view = reply
        .outputs
        .iter()
        .find(|row| row.output.0 == wire)
        .and_then(|row| row.logical_view.clone())
        .expect("latest reply names the DEV1 view");
    assert_eq!(
        reply_view.as_str(),
        "ws-2",
        "retained reply must be the latest select (index 2), not a superseded one"
    );
}

#[test]
fn gated_select_batch_dispatches_and_applies_nothing() {
    let mut state = test_state();
    state.keyboard.takeover = true;
    let (dir, store) = test_ledger("gated");
    let areas = vec![area()];
    inject_pending_observe(Vec::new(), Vec::new(), false);
    inject_pending_foreground(Some(0));
    inject_pending_suspend(Some(true));
    inject_pending_elevated(Some(false));
    reset_all_apply_counts();
    let cmd_before = state.world.next_command_seq();
    super::workspace_navigation::workspace_tick(
        &mut state,
        &fake_me(),
        &store,
        &dir,
        &[],
        &areas,
        vec![select(1), select(2), select(3)],
        None,
    );
    clear_pending_seams();
    assert_eq!(
        state.world.next_command_seq() - cmd_before,
        0,
        "suspended batch must dispatch nothing"
    );
    assert_eq!(
        command_apply_count(),
        0,
        "suspended batch must apply nothing"
    );
    assert_eq!(
        observation_apply_count(),
        0,
        "suspended batch must run no catch-up applies"
    );
}

#[test]
fn observation_tick_increments_observation_oracle() {
    // The oracle covers observation-path applies too: a lone observation
    // tick must read as exactly one observation apply and zero command
    // applies, so a burst's `observation == 0` is proof of no hidden
    // catch-up rather than an uninstrumented path.
    let mut state = test_state();
    state.keyboard.takeover = true;
    let (dir, store) = test_ledger("oracle-obs");
    let areas = vec![area()];
    let observed = seed_two_windows(&mut state);
    clean_batch_with(observed.clone());
    let seeded = super::world_tick::run_observation_tick(
        &mut state,
        &fake_me(),
        &store,
        &dir,
        &observed,
        &[],
        &areas,
        0,
        &fulls(),
        Some((100, 100)),
        false,
    );
    assert!(seeded.is_some(), "seed observation must converge");
    reset_all_apply_counts();
    clean_batch_with(observed.clone());
    let ticked = super::world_tick::run_observation_tick(
        &mut state,
        &fake_me(),
        &store,
        &dir,
        &observed,
        &[],
        &areas,
        0,
        &fulls(),
        Some((100, 100)),
        false,
    );
    clear_pending_seams();
    assert!(ticked.is_some(), "observation tick must converge");
    assert_eq!(command_apply_count(), 0, "no command ran");
    assert_eq!(
        observation_apply_count(),
        1,
        "one observation pass must count once"
    );
}

#[test]
fn repeated_same_view_burst_orders_all_applies_latest_only() {
    // Same-view repeats still order every command once while only the
    // latest reply plans/applies natively; duplicates must not smuggle
    // extra observation catch-up applies.
    let mut state = test_state();
    state.keyboard.takeover = true;
    let (dir, store) = test_ledger("same-view");
    let areas = vec![area()];
    let observed = seed_two_windows(&mut state);
    clean_batch_with(observed.clone());
    let seeded = super::world_tick::run_observation_tick(
        &mut state,
        &fake_me(),
        &store,
        &dir,
        &observed,
        &[],
        &areas,
        0,
        &fulls(),
        Some((100, 100)),
        false,
    );
    assert!(seeded.is_some(), "seed observation must converge");
    reset_all_apply_counts();
    let cmd_before = state.world.next_command_seq();
    clean_batch_with(observed.clone());
    super::workspace_navigation::workspace_tick(
        &mut state,
        &fake_me(),
        &store,
        &dir,
        &fulls(),
        &areas,
        vec![select(2), select(2), select(2)],
        None,
    );
    clear_pending_seams();
    assert_eq!(
        state.world.next_command_seq() - cmd_before,
        3,
        "core must order every repeated select once"
    );
    assert_eq!(
        command_apply_count(),
        1,
        "only the latest repeat may plan/apply natively"
    );
    assert_eq!(
        observation_apply_count(),
        0,
        "repeats must not hide catch-up applies"
    );
    let reply = state
        .world
        .last_reply()
        .cloned()
        .expect("latest reply must be retained");
    let wire = state.output_token("DEV1");
    let reply_view = reply
        .outputs
        .iter()
        .find(|row| row.output.0 == wire)
        .and_then(|row| row.logical_view.clone())
        .expect("latest reply names the DEV1 view");
    assert_eq!(
        reply_view.as_str(),
        "ws-2",
        "retained reply must be the latest select (index 2)"
    );
}

#[test]
fn select_send_barrier_flushes_pending_select_first() {
    // `Send` is a barrier: the pending select applies fully first, then the
    // (here unmanaged, undispatched) send settles. The barrier flush still
    // runs exactly one native plan; the send itself dispatches nothing.
    let mut state = test_state();
    state.keyboard.takeover = true;
    let (dir, store) = test_ledger("barrier");
    let areas = vec![area()];
    let observed = seed_two_windows(&mut state);
    clean_batch_with(observed.clone());
    let seeded = super::world_tick::run_observation_tick(
        &mut state,
        &fake_me(),
        &store,
        &dir,
        &observed,
        &[],
        &areas,
        0,
        &fulls(),
        Some((100, 100)),
        false,
    );
    assert!(seeded.is_some(), "seed observation must converge");
    reset_all_apply_counts();
    let cmd_before = state.world.next_command_seq();
    clean_batch_with(observed.clone());
    super::workspace_navigation::workspace_tick(
        &mut state,
        &fake_me(),
        &store,
        &dir,
        &fulls(),
        &areas,
        vec![select(1), select(2), send_unmanaged(1)],
        None,
    );
    clear_pending_seams();
    assert_eq!(
        state.world.next_command_seq() - cmd_before,
        2,
        "both selects order; the unmanaged send dispatches nothing"
    );
    assert_eq!(
        command_apply_count(),
        1,
        "barrier flush applies the pending select exactly once"
    );
    assert_eq!(
        observation_apply_count(),
        0,
        "barrier burst must hide no catch-up applies"
    );
}

#[test]
fn flush_recheck_drops_pending_select_when_suspended() {
    // Adversarial gate flip between intake and flush: both selects arrive
    // unsuspended, then the game suspends before the flush. The superseded
    // select stays ordered core-only, but the pending latest must drop
    // without dispatching or applying.
    let mut state = test_state();
    state.keyboard.takeover = true;
    let (dir, store) = test_ledger("flip-suspend");
    let areas = vec![area()];
    let observed = seed_two_windows(&mut state);
    clean_batch_with(observed.clone());
    let seeded = super::world_tick::run_observation_tick(
        &mut state,
        &fake_me(),
        &store,
        &dir,
        &observed,
        &[],
        &areas,
        0,
        &fulls(),
        Some((100, 100)),
        false,
    );
    assert!(seeded.is_some(), "seed observation must converge");
    reset_all_apply_counts();
    let cmd_before = state.world.next_command_seq();
    inject_pending_observe(observed.clone(), Vec::new(), false);
    inject_pending_foreground(Some(0));
    // Two intake reads pass, the flush recheck suspends; the persistent
    // fallback stays pass so only the scripted flip blocks.
    inject_pending_suspend_seq(vec![false, false, true]);
    inject_pending_suspend(Some(false));
    inject_pending_elevated(Some(false));
    super::workspace_navigation::workspace_tick(
        &mut state,
        &fake_me(),
        &store,
        &dir,
        &fulls(),
        &areas,
        vec![select(1), select(2)],
        None,
    );
    clear_pending_seams();
    assert_eq!(
        state.world.next_command_seq() - cmd_before,
        1,
        "only the superseded select orders; the suspended flush dispatches nothing"
    );
    assert_eq!(
        command_apply_count(),
        0,
        "suspended flush must plan/apply nothing"
    );
    assert_eq!(
        observation_apply_count(),
        0,
        "suspended flush must run no catch-up applies"
    );
}

#[test]
fn flush_recheck_drops_pending_select_when_elevated() {
    // Same adversarial flip through the elevation gate: intakes pass, the
    // flush recheck sees an elevated foreground and drops the pending
    // latest without dispatching or applying.
    let mut state = test_state();
    state.keyboard.takeover = true;
    let (dir, store) = test_ledger("flip-elevated");
    let areas = vec![area()];
    let observed = seed_two_windows(&mut state);
    clean_batch_with(observed.clone());
    let seeded = super::world_tick::run_observation_tick(
        &mut state,
        &fake_me(),
        &store,
        &dir,
        &observed,
        &[],
        &areas,
        0,
        &fulls(),
        Some((100, 100)),
        false,
    );
    assert!(seeded.is_some(), "seed observation must converge");
    reset_all_apply_counts();
    let cmd_before = state.world.next_command_seq();
    inject_pending_observe(observed.clone(), Vec::new(), false);
    inject_pending_foreground(Some(0));
    inject_pending_suspend(Some(false));
    // Two intake reads pass, the flush recheck elevates; the persistent
    // fallback stays pass so only the scripted flip blocks.
    inject_pending_elevated_seq(vec![false, false, true]);
    inject_pending_elevated(Some(false));
    super::workspace_navigation::workspace_tick(
        &mut state,
        &fake_me(),
        &store,
        &dir,
        &fulls(),
        &areas,
        vec![select(1), select(2)],
        None,
    );
    clear_pending_seams();
    assert_eq!(
        state.world.next_command_seq() - cmd_before,
        1,
        "only the superseded select orders; the elevated flush dispatches nothing"
    );
    assert_eq!(
        command_apply_count(),
        0,
        "elevated flush must plan/apply nothing"
    );
    assert_eq!(
        observation_apply_count(),
        0,
        "elevated flush must run no catch-up applies"
    );
}

#[test]
fn forced_catchup_burst_orders_all_without_native_applies() {
    // Armed observation-only fence mid-burst: the first superseded select
    // inspects the fence before dispatching, services it once through a
    // native-free core-only observation, then dispatches exactly once. No
    // re-dispatch follows a refusal: a still-fenced command would drop and
    // the next pre-dispatch inspection would service again. Only the latest
    // select plans and applies natively; neither oracle counter sees the
    // fence service.
    let mut state = test_state();
    state.keyboard.takeover = true;
    let (dir, store) = test_ledger("forced-catchup");
    let areas = vec![area()];
    let observed = seed_two_windows(&mut state);
    clean_batch_with(observed.clone());
    let seeded = super::world_tick::run_observation_tick(
        &mut state,
        &fake_me(),
        &store,
        &dir,
        &observed,
        &[],
        &areas,
        0,
        &fulls(),
        Some((100, 100)),
        false,
    );
    assert!(seeded.is_some(), "seed observation must converge");
    reset_all_apply_counts();
    let cmd_before = state.world.next_command_seq();
    let rev_before = state.world.engine().state_revision();
    state.world.arm_catchup_for_test();
    clean_batch_with(observed.clone());
    super::workspace_navigation::workspace_tick(
        &mut state,
        &fake_me(),
        &store,
        &dir,
        &fulls(),
        &areas,
        vec![select(2), select(1), select(2)],
        None,
    );
    clear_pending_seams();
    assert_eq!(
        state.world.next_command_seq() - cmd_before,
        3,
        "every queued select must order exactly once despite the armed fence"
    );
    assert_eq!(
        state.world.engine().state_revision() - rev_before,
        4,
        "fence-service observation plus three commands each incorporate fresh"
    );
    assert_eq!(
        command_apply_count(),
        1,
        "only the latest select may plan/apply natively"
    );
    assert_eq!(
        observation_apply_count(),
        0,
        "fence service must not hide catch-up applies"
    );
    assert_eq!(
        total_apply_count(),
        1,
        "combined apply passes must equal the single latest native plan"
    );
    let reply = state
        .world
        .last_reply()
        .cloned()
        .expect("latest reply must be retained");
    let wire = state.output_token("DEV1");
    let reply_view = reply
        .outputs
        .iter()
        .find(|row| row.output.0 == wire)
        .and_then(|row| row.logical_view.clone())
        .expect("latest reply names the DEV1 view");
    assert_eq!(
        reply_view.as_str(),
        "ws-2",
        "retained reply must be the latest select (index 2), not a fence-service intermediate"
    );
}

#[test]
fn admission_preamble_burst_stays_native_free() {
    // A stranger window rides the burst inventory: admission runs on every
    // leg but binds nothing offline (fail-closed before the tag-install
    // write seam, which the install-attempt counter proves is never
    // reached), while ordering and the oracle stay exact. The member table
    // keeps exactly the two seeded members: no binding, no eviction.
    // Honesty bound: on a live host the same preamble may bind a genuinely
    // proven stranger exactly as a full-leg tick would (removing admission
    // from the superseded leg would weaken fail-closed binding, so the
    // native-free claim covers planning/application only).
    let mut state = test_state();
    state.keyboard.takeover = true;
    let (dir, store) = test_ledger("admission-burst");
    let areas = vec![area()];
    let mut observed = seed_two_windows(&mut state);
    observed.push(observed_window(103, "win-c", 203, "proc-c"));
    clean_batch_with(observed.clone());
    let seeded = super::world_tick::run_observation_tick(
        &mut state,
        &fake_me(),
        &store,
        &dir,
        &observed,
        &[],
        &areas,
        0,
        &fulls(),
        Some((100, 100)),
        false,
    );
    assert!(seeded.is_some(), "seed observation must converge");
    reset_all_apply_counts();
    crate::product_hide::sys::reset_install_tag_attempts();
    let cmd_before = state.world.next_command_seq();
    let rev_before = state.world.engine().state_revision();
    clean_batch_with(observed.clone());
    super::workspace_navigation::workspace_tick(
        &mut state,
        &fake_me(),
        &store,
        &dir,
        &fulls(),
        &areas,
        vec![select(2), select(1), select(2)],
        None,
    );
    clear_pending_seams();
    assert_eq!(
        state.world.next_command_seq() - cmd_before,
        3,
        "core must order every queued select once with a stranger present"
    );
    assert_eq!(
        state.world.engine().state_revision() - rev_before,
        3,
        "every select must incorporate fresh"
    );
    assert_eq!(
        command_apply_count(),
        1,
        "only the latest select may plan/apply natively"
    );
    assert_eq!(
        observation_apply_count(),
        0,
        "admission legs must hide no catch-up applies"
    );
    assert_eq!(
        total_apply_count(),
        1,
        "combined apply passes must equal the single latest native plan"
    );
    assert_eq!(
        crate::product_hide::sys::install_tag_attempts(),
        0,
        "admission must never reach the tag-install write seam in this burst"
    );
    assert_eq!(
        state.member_tokens.len(),
        2,
        "admission must bind nothing new and evict nothing seeded"
    );
    let mut hwnds: Vec<u64> = state.member_tokens.keys().map(|key| key.hwnd).collect();
    hwnds.sort_unstable();
    assert_eq!(
        hwnds,
        vec![101, 102],
        "member table must keep exactly the two seeded windows"
    );
    assert!(
        state.member_tokens.keys().all(|key| key.hwnd != 103),
        "stranger window must stay unbound (fail-closed admission)"
    );
}

fn hist(op: WorkspaceHistoryOp) -> QueuedWorkspaceHistoryIntent {
    QueuedWorkspaceHistoryIntent {
        op,
        edge: SnapEdge::Down,
        origin: None,
        consumed: true,
        announce: true,
        tick: std::time::Instant::now(),
    }
}

fn dev1_view(state: &super::TileLoop) -> String {
    let wire = state.output_token("DEV1");
    state
        .world
        .engine()
        .logical_view(&wire)
        .expect("DEV1 must name a logical view")
}

fn seed_converged(
    state: &mut super::TileLoop,
    dir: &std::path::Path,
    store: &crate::storage::LedgerStore,
    areas: &[MonitorArea],
    observed: &[super::ObservedWindow],
) {
    clean_batch_with(observed.to_vec());
    let seeded = super::world_tick::run_observation_tick(
        state,
        &fake_me(),
        store,
        dir,
        observed,
        &[],
        areas,
        0,
        &fulls(),
        Some((100, 100)),
        false,
    );
    assert!(seeded.is_some(), "seed observation must converge");
}

#[test]
fn history_burst_orders_all_applies_latest_only_with_exact_final_view() {
    // History latest-only: every relative/toggle intent orders through the
    // core exactly once while only the latest reply plans/applies natively.
    // The final view must equal the sequential full-path reference (three
    // singleton ticks), proving intermediate core-only legs preserve the
    // order-dependent history stack.
    let areas = vec![area()];
    let mk_burst = || {
        vec![
            hist(WorkspaceHistoryOp::Next),
            hist(WorkspaceHistoryOp::Next),
            hist(WorkspaceHistoryOp::Prev),
        ]
    };
    // Burst leg.
    let mut burst = test_state();
    burst.keyboard.takeover = true;
    let (dir_b, store_b) = test_ledger("history-burst");
    let observed_b = seed_two_windows(&mut burst);
    seed_converged(&mut burst, &dir_b, &store_b, &areas, &observed_b);
    reset_all_apply_counts();
    let cmd_before = burst.world.next_command_seq();
    clean_batch_with(observed_b.clone());
    super::history::workspace_history_tick(
        &mut burst,
        &fake_me(),
        &store_b,
        &dir_b,
        &fulls(),
        &areas,
        mk_burst(),
        None,
    );
    clear_pending_seams();
    assert_eq!(
        burst.world.next_command_seq() - cmd_before,
        3,
        "core must order every queued history intent once"
    );
    assert_eq!(
        command_apply_count(),
        1,
        "only the latest history intent may plan/apply natively"
    );
    assert_eq!(
        observation_apply_count(),
        0,
        "burst must hide no catch-up applies"
    );
    assert_eq!(
        total_apply_count(),
        1,
        "combined applies must equal the single latest plan"
    );
    let burst_view = dev1_view(&burst);
    // Sequential reference leg: same three intents, one tick each (each
    // singleton flushes fully, matching the pre-latest-only path).
    let mut seq = test_state();
    seq.keyboard.takeover = true;
    let (dir_s, store_s) = test_ledger("history-sequential");
    let observed_s = seed_two_windows(&mut seq);
    seed_converged(&mut seq, &dir_s, &store_s, &areas, &observed_s);
    for intent in mk_burst() {
        clean_batch_with(observed_s.clone());
        super::history::workspace_history_tick(
            &mut seq,
            &fake_me(),
            &store_s,
            &dir_s,
            &fulls(),
            &areas,
            vec![intent],
            None,
        );
        clear_pending_seams();
    }
    assert_eq!(
        burst_view,
        dev1_view(&seq),
        "burst final view must equal the sequential full-path final view"
    );
}

#[test]
fn history_gated_batch_dispatches_and_applies_nothing() {
    let mut state = test_state();
    state.keyboard.takeover = true;
    let (dir, store) = test_ledger("history-gated");
    let areas = vec![area()];
    inject_pending_observe(Vec::new(), Vec::new(), false);
    inject_pending_foreground(Some(0));
    inject_pending_suspend(Some(true));
    inject_pending_elevated(Some(false));
    reset_all_apply_counts();
    let cmd_before = state.world.next_command_seq();
    super::history::workspace_history_tick(
        &mut state,
        &fake_me(),
        &store,
        &dir,
        &[],
        &areas,
        vec![
            hist(WorkspaceHistoryOp::Next),
            hist(WorkspaceHistoryOp::Prev),
        ],
        None,
    );
    clear_pending_seams();
    assert_eq!(
        state.world.next_command_seq() - cmd_before,
        0,
        "suspended history batch must dispatch nothing"
    );
    assert_eq!(
        command_apply_count(),
        0,
        "suspended history batch must apply nothing"
    );
    assert_eq!(
        observation_apply_count(),
        0,
        "suspended history batch runs no catch-up"
    );
}

#[test]
fn history_flush_recheck_drops_pending_when_suspended() {
    let mut state = test_state();
    state.keyboard.takeover = true;
    let (dir, store) = test_ledger("history-flip-suspend");
    let areas = vec![area()];
    let observed = seed_two_windows(&mut state);
    seed_converged(&mut state, &dir, &store, &areas, &observed);
    reset_all_apply_counts();
    let cmd_before = state.world.next_command_seq();
    inject_pending_observe(observed.clone(), Vec::new(), false);
    inject_pending_foreground(Some(0));
    inject_pending_suspend_seq(vec![false, false, true]);
    inject_pending_suspend(Some(false));
    inject_pending_elevated(Some(false));
    super::history::workspace_history_tick(
        &mut state,
        &fake_me(),
        &store,
        &dir,
        &fulls(),
        &areas,
        vec![
            hist(WorkspaceHistoryOp::Next),
            hist(WorkspaceHistoryOp::Next),
        ],
        None,
    );
    clear_pending_seams();
    assert_eq!(
        state.world.next_command_seq() - cmd_before,
        1,
        "only the superseded history intent orders; the suspended flush dispatches nothing"
    );
    assert_eq!(
        command_apply_count(),
        0,
        "suspended history flush must plan/apply nothing"
    );
    assert_eq!(
        observation_apply_count(),
        0,
        "suspended history flush runs no catch-up"
    );
}

#[test]
fn history_forced_catchup_burst_orders_all_without_extra_applies() {
    let mut state = test_state();
    state.keyboard.takeover = true;
    let (dir, store) = test_ledger("history-forced-catchup");
    let areas = vec![area()];
    let observed = seed_two_windows(&mut state);
    seed_converged(&mut state, &dir, &store, &areas, &observed);
    reset_all_apply_counts();
    let cmd_before = state.world.next_command_seq();
    let rev_before = state.world.engine().state_revision();
    state.world.arm_catchup_for_test();
    clean_batch_with(observed.clone());
    super::history::workspace_history_tick(
        &mut state,
        &fake_me(),
        &store,
        &dir,
        &fulls(),
        &areas,
        vec![
            hist(WorkspaceHistoryOp::Next),
            hist(WorkspaceHistoryOp::Prev),
            hist(WorkspaceHistoryOp::Next),
        ],
        None,
    );
    clear_pending_seams();
    assert_eq!(
        state.world.next_command_seq() - cmd_before,
        3,
        "every queued history intent must order exactly once despite the armed fence"
    );
    assert_eq!(
        state.world.engine().state_revision() - rev_before,
        4,
        "fence-service observation plus three commands each incorporate fresh"
    );
    assert_eq!(
        command_apply_count(),
        1,
        "only the latest history intent may apply natively"
    );
    assert_eq!(
        observation_apply_count(),
        0,
        "fence service must not hide catch-up applies"
    );
    assert_eq!(
        total_apply_count(),
        1,
        "combined applies must equal the single latest plan"
    );
}

#[test]
fn send_barrier_applies_pending_select_then_linked_move() {
    // Meaningful barrier: the pending select applies fully first, then a
    // genuinely linked send (seam-bound origin token) dispatches its
    // membership move through the full path. Both commands order, both
    // apply natively, and no observation catch-up hides. The origin HWND and
    // the capture foreground both come from the injected test foreground
    // seam (101), so the barrier binds deterministically with no dependence
    // on whatever foreground the test host holds.
    let mut state = test_state();
    state.keyboard.takeover = true;
    let (dir, store) = test_ledger("linked-barrier");
    let areas = vec![area()];
    let observed = seed_two_windows(&mut state);
    clean_batch_with(observed.clone());
    let seeded = super::world_tick::run_observation_tick(
        &mut state,
        &fake_me(),
        &store,
        &dir,
        &observed,
        &[],
        &areas,
        0,
        &fulls(),
        Some((100, 100)),
        false,
    );
    assert!(seeded.is_some(), "seed observation must converge");
    reset_all_apply_counts();
    let cmd_before = state.world.next_command_seq();
    // Burst inventory with the linked window as the seam foreground (101):
    // the Send arm binds the chord origin against the same injected
    // foreground the capture path grounds focus from, so the linked send
    // carries a deterministic origin on every host.
    inject_pending_observe(observed.clone(), Vec::new(), false);
    inject_pending_foreground(Some(101));
    inject_pending_suspend(Some(false));
    inject_pending_elevated(Some(false));
    super::workspace_navigation::workspace_tick(
        &mut state,
        &fake_me(),
        &store,
        &dir,
        &fulls(),
        &areas,
        vec![
            select(2),
            select(1),
            send_linked(2, 101, "win-a", 201, "proc-a"),
        ],
        None,
    );
    clear_pending_seams();
    assert_eq!(
        state.world.next_command_seq() - cmd_before,
        3,
        "superseded select, barrier select, and linked send must each order"
    );
    assert_eq!(
        command_apply_count(),
        2,
        "barrier select plus linked send each apply natively"
    );
    assert_eq!(
        observation_apply_count(),
        0,
        "barrier burst must hide no catch-up applies"
    );
    assert_eq!(
        total_apply_count(),
        2,
        "combined apply passes must equal the two full native plans"
    );
    let reply = state
        .world
        .last_reply()
        .cloned()
        .expect("latest reply must be retained");
    assert!(
        reply.transfers.iter().any(|transfer| {
            matches!(
                &transfer.kind,
                TransferKind::Window(window) if window.0 == "win-a"
            ) && transfer.target_workspace == "ws-2"
        }),
        "linked send must carry the win-a membership move to ws-2"
    );
}

fn area2() -> MonitorArea {
    MonitorArea {
        device: "DEV2".to_owned(),
        work: Rect {
            x: 800,
            y: 0,
            w: 800,
            h: 560,
        },
        full: Rect {
            x: 800,
            y: 0,
            w: 800,
            h: 600,
        },
    }
}

fn view_on(state: &super::TileLoop, device: &str) -> String {
    let wire = state.output_token(device);
    state
        .world
        .engine()
        .logical_view(&wire)
        .expect("device must name a logical view")
}

fn seed_converged_two_output(
    state: &mut super::TileLoop,
    dir: &std::path::Path,
    store: &crate::storage::LedgerStore,
    areas: &[MonitorArea],
    observed: &[super::ObservedWindow],
) {
    state.ensure_device("DEV2");
    clean_batch_with(observed.to_vec());
    let seeded = super::world_tick::run_observation_tick(
        state,
        &fake_me(),
        store,
        dir,
        observed,
        &[],
        areas,
        0,
        &areas.iter().map(|row| row.full).collect::<Vec<_>>(),
        Some((100, 100)),
        false,
    );
    assert!(seeded.is_some(), "seed observation must converge");
}

#[test]
fn trailing_burst_orders_all_applies_latest_only_with_sequential_parity() {
    // Index-0 trailing coverage: trailing participates in latest-only both
    // as a superseded leg (first entry orders core-only) and as the latest
    // full-path flush (last entry plans/applies). Final view must equal
    // the sequential full-path reference (three singleton ticks).
    let areas = vec![area()];
    let mk_burst = || vec![select(0), select(1), select(0)];
    let mut burst = test_state();
    burst.keyboard.takeover = true;
    let (dir_b, store_b) = test_ledger("trailing-burst");
    let observed_b = seed_two_windows(&mut burst);
    seed_converged(&mut burst, &dir_b, &store_b, &areas, &observed_b);
    reset_all_apply_counts();
    let cmd_before = burst.world.next_command_seq();
    clean_batch_with(observed_b.clone());
    super::workspace_navigation::workspace_tick(
        &mut burst,
        &fake_me(),
        &store_b,
        &dir_b,
        &fulls(),
        &areas,
        mk_burst(),
        None,
    );
    clear_pending_seams();
    assert_eq!(
        burst.world.next_command_seq() - cmd_before,
        3,
        "core must order every trailing/numbered select once"
    );
    assert_eq!(
        command_apply_count(),
        1,
        "only the latest trailing select may plan/apply natively"
    );
    assert_eq!(
        observation_apply_count(),
        0,
        "burst must hide no catch-up applies"
    );
    assert_eq!(
        total_apply_count(),
        1,
        "combined applies must equal the single latest plan"
    );
    let burst_view = dev1_view(&burst);
    let mut seq = test_state();
    seq.keyboard.takeover = true;
    let (dir_s, store_s) = test_ledger("trailing-sequential");
    let observed_s = seed_two_windows(&mut seq);
    seed_converged(&mut seq, &dir_s, &store_s, &areas, &observed_s);
    for intent in mk_burst() {
        clean_batch_with(observed_s.clone());
        super::workspace_navigation::workspace_tick(
            &mut seq,
            &fake_me(),
            &store_s,
            &dir_s,
            &fulls(),
            &areas,
            vec![intent],
            None,
        );
        clear_pending_seams();
    }
    assert_eq!(
        burst_view,
        dev1_view(&seq),
        "trailing burst final view must equal the sequential full-path final view"
    );
}

#[test]
fn history_previous_mixed_burst_matches_sequential() {
    // Toggle coverage: `Previous` mixed with relative steps orders every
    // intent once with latest-only applies, and the final view equals the
    // sequential full-path reference (order-dependent history stack stays
    // intact through the core-only superseded legs).
    let areas = vec![area()];
    let mk_burst = || {
        vec![
            hist(WorkspaceHistoryOp::Previous),
            hist(WorkspaceHistoryOp::Next),
            hist(WorkspaceHistoryOp::Prev),
        ]
    };
    let mut burst = test_state();
    burst.keyboard.takeover = true;
    let (dir_b, store_b) = test_ledger("history-previous-burst");
    let observed_b = seed_two_windows(&mut burst);
    seed_converged(&mut burst, &dir_b, &store_b, &areas, &observed_b);
    reset_all_apply_counts();
    let cmd_before = burst.world.next_command_seq();
    clean_batch_with(observed_b.clone());
    super::history::workspace_history_tick(
        &mut burst,
        &fake_me(),
        &store_b,
        &dir_b,
        &fulls(),
        &areas,
        mk_burst(),
        None,
    );
    clear_pending_seams();
    assert_eq!(
        burst.world.next_command_seq() - cmd_before,
        3,
        "core must order every mixed history intent once"
    );
    assert_eq!(
        command_apply_count(),
        1,
        "only the latest history intent may plan/apply natively"
    );
    assert_eq!(
        observation_apply_count(),
        0,
        "burst must hide no catch-up applies"
    );
    assert_eq!(
        total_apply_count(),
        1,
        "combined applies must equal the single latest plan"
    );
    let burst_view = dev1_view(&burst);
    let mut seq = test_state();
    seq.keyboard.takeover = true;
    let (dir_s, store_s) = test_ledger("history-previous-sequential");
    let observed_s = seed_two_windows(&mut seq);
    seed_converged(&mut seq, &dir_s, &store_s, &areas, &observed_s);
    for intent in mk_burst() {
        clean_batch_with(observed_s.clone());
        super::history::workspace_history_tick(
            &mut seq,
            &fake_me(),
            &store_s,
            &dir_s,
            &fulls(),
            &areas,
            vec![intent],
            None,
        );
        clear_pending_seams();
    }
    assert_eq!(
        burst_view,
        dev1_view(&seq),
        "mixed Previous burst final view must equal the sequential full-path final view"
    );
}

#[test]
fn multi_output_select_burst_matches_sequential_on_both_outputs() {
    // Multi-output final-state consistency: the same select burst run as one
    // latest-only batch versus three singleton full-path ticks converges to
    // the same per-output views, with latest-only apply counts on the burst
    // leg. Both legs share the host cursor sample, so cursor-first switch
    // resolution cannot diverge between them; a moving-cursor leg would need
    // a cursor seam that does not exist offline and is not asserted here.
    let areas = vec![area(), area2()];
    let fulls: Vec<Rect> = areas.iter().map(|row| row.full).collect();
    let mk_burst = || vec![select(2), select(1), select(2)];
    let mut burst = test_state();
    burst.keyboard.takeover = true;
    let (dir_b, store_b) = test_ledger("multi-output-burst");
    let observed_b = seed_two_windows(&mut burst);
    seed_converged_two_output(&mut burst, &dir_b, &store_b, &areas, &observed_b);
    reset_all_apply_counts();
    let cmd_before = burst.world.next_command_seq();
    clean_batch_with(observed_b.clone());
    super::workspace_navigation::workspace_tick(
        &mut burst,
        &fake_me(),
        &store_b,
        &dir_b,
        &fulls,
        &areas,
        mk_burst(),
        None,
    );
    clear_pending_seams();
    assert_eq!(
        burst.world.next_command_seq() - cmd_before,
        3,
        "core must order every select once across two outputs"
    );
    assert_eq!(
        command_apply_count(),
        1,
        "only the latest select may plan/apply natively"
    );
    assert_eq!(
        observation_apply_count(),
        0,
        "burst must hide no catch-up applies"
    );
    assert_eq!(
        total_apply_count(),
        1,
        "combined applies must equal the single latest plan"
    );
    let burst_dev1 = view_on(&burst, "DEV1");
    let burst_dev2 = view_on(&burst, "DEV2");
    let mut seq = test_state();
    seq.keyboard.takeover = true;
    let (dir_s, store_s) = test_ledger("multi-output-sequential");
    let observed_s = seed_two_windows(&mut seq);
    seed_converged_two_output(&mut seq, &dir_s, &store_s, &areas, &observed_s);
    for intent in mk_burst() {
        clean_batch_with(observed_s.clone());
        super::workspace_navigation::workspace_tick(
            &mut seq,
            &fake_me(),
            &store_s,
            &dir_s,
            &fulls,
            &areas,
            vec![intent],
            None,
        );
        clear_pending_seams();
    }
    assert_eq!(
        burst_dev1,
        view_on(&seq, "DEV1"),
        "burst DEV1 view must equal the sequential final view"
    );
    assert_eq!(
        burst_dev2,
        view_on(&seq, "DEV2"),
        "burst DEV2 view must equal the sequential final view"
    );
}

#[test]
fn gesture_stash_single_slot_overwrites_in_id_order() {
    // Exact stash contract (per `stash_gesture_cancel`): single slot, Cancelled
    // phase, END-captured frame, and the ingress prefers the stashed terminal
    // over any live cursor. Stashing a newer id overwrites the older entry,
    // which is safe because ids order globally and every terminal consumes
    // forward. No tick runs here: this asserts the stash shape only, never a
    // core incorporation or clearing rule.
    use tiler_core::gesture::{GestureKind, GesturePhase};
    let mut state = test_state();
    let frame_first = Rect {
        x: 0,
        y: 0,
        w: 200,
        h: 200,
    };
    let returned = super::gesture_tick::stash_gesture_cancel(
        &mut state,
        101,
        7,
        "win-a",
        GestureKind::Move,
        true,
        frame_first,
    );
    assert_eq!(returned, 7, "stash must return the staged id");
    let stashed = state
        .gesture_override
        .clone()
        .expect("stash must set the slot");
    assert_eq!(stashed.id, 7);
    assert_eq!(stashed.window.0, "win-a");
    assert_eq!(stashed.kind, GestureKind::Move);
    assert_eq!(stashed.phase, GesturePhase::Cancelled);
    assert!(!stashed.esc);
    assert!(stashed.modal_move);
    assert_eq!(stashed.start_frame, frame_first);
    let ingress = state
        .gesture_ingress(Some((1, 1)))
        .expect("stashed terminal must ride the ingress");
    assert_eq!(ingress.id, 7, "ingress must prefer the stashed terminal");
    assert_eq!(ingress.phase, GesturePhase::Cancelled);
    let frame_second = Rect {
        x: 10,
        y: 10,
        w: 200,
        h: 200,
    };
    let returned = super::gesture_tick::stash_gesture_cancel(
        &mut state,
        101,
        9,
        "win-a",
        GestureKind::Move,
        true,
        frame_second,
    );
    assert_eq!(returned, 9, "restash must return the newer id");
    let stashed = state.gesture_override.clone().expect("slot must stay set");
    assert_eq!(stashed.id, 9, "single slot must hold the newest terminal");
    assert_eq!(stashed.start_frame, frame_second);
    let ingress = state
        .gesture_ingress(Some((1, 1)))
        .expect("newer terminal must ride the ingress");
    assert_eq!(ingress.id, 9, "ingress must prefer the newest terminal");
    assert_eq!(ingress.phase, GesturePhase::Cancelled);
}

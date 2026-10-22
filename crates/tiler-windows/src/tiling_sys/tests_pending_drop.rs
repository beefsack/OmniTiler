//! Pending-drop slot-machine regressions (offline, native-free).
//!
//! Restores independent accepted coverage for the LIVE pending-drop slot
//! machine (`queue_pending_drop` / `service_pending_drop`) over the current
//! world route. The stage-2b cutover deleted `tests_pending_drop.rs` with
//! the retired Engine/apply/counter seams (`PendingCounters`,
//! `INJECTED_APPLY`, scripted focus/stop/area injectors, cross preview
//! helpers); this module re-pins every still-applicable accepted behavior
//! through the production functions with the current seams only.
//!
//! Conventions: `state.tick` is the converge counter (`reconcile_tick` with
//! an injected observation only bumps `tick`, so +1 means exactly one
//! converge and +0 means none). The world command pre-bump runs before the
//! world route, so a proven same-output drop converges with +2 (dispatch
//! plus converge) while a proven cross drop that cannot prove arrival holds
//! at +1 (dispatch with no converge: the commit-last property). Receipts
//! (`applied_homing` / `applied_views`) merge only from live native proof,
//! so they stay empty on fake HWNDs: tests assert they are never fabricated.
//! A tiled arrival can never verify offline (`readback_frame` needs a live
//! window), so proven drops always end in the honest unverified converge.
//!
//! Accepted-behavior map (every deleted `tests_pending_drop.rs` test ->
//! coverage; old scripted-verdict mechanics are superseded but the accepted
//! effects below stay pinned by the named cases).
//!
//! Slot machine (this module, world-routed; `state.tick` is the converge
//! counter: +1 per converge, +0 for none):
//! - proven_hidden_executes_with_fresh_validation ->
//!   `proven_same_output_dispatches_and_converges_without_receipt`.
//! - unproven_ack_keeps_pending_with_no_actuation ->
//!   `unproven_ack_keeps_pending`.
//! - stale_ack_pair_keeps_pending_with_no_actuation ->
//!   `stale_ack_pair_keeps_pending`.
//! - timeout_refuses_with_no_actuation_or_reconcile -> `timeout_drops_slot`.
//! - start_identity_mismatch_refuses -> `start_identity_mismatch_refuses`.
//! - member_token_remap_refuses -> `member_token_remap_refuses`.
//! - occupied_newcomer_is_refused_narrowly ->
//!   `overlapping_end_refuses_busy`.
//! - cross_pending_proven_hide_settles_transfer_with_arrival ->
//!   `cross_proven_send_without_observed_focus_refuses_and_converges` (the
//!   injected foreground is 0, so the T2-staged `Send` dispatch refuses
//!   `absent-focus`; acceptance with observed focus is pinned by
//!   `world_route_tests::production_send_route_reports_honest_outcome_without_proof`).
//! - cross_pending_empty_native_flow_admits_then_reflows ->
//!   `cross_empty_destination_never_switches_view` (the core `Send`
//!   unifies the empty/occupied lanes).
//! - cross_pending_occupied_late_topology_retains_source_and_engine and
//!   cross_pending_empty_late_topology_retains_source_and_engine ->
//!   `cross_fresh_topology_drift_withholds`.
//! - cross_pending_stale_topology_refuses_without_actuation ->
//!   `cross_stale_pointer_topology_refuses`.
//! - cross_pending_stale_view_refuses_without_actuation ->
//!   `cross_stale_view_refuses`.
//! - cross_pending_stale_lifetime_refuses_without_actuation ->
//!   `cross_stale_lifetime_prunes`.
//! - cross_pending_suspend_refuses_without_actuation ->
//!   `cross_suspend_refuses`.
//! - cross_pending_timeout_refuses_without_actuation -> shared pre-check,
//!   covered by `timeout_drops_slot`.
//! - esc_invalidates_pending_with_converge ->
//!   `esc_invalidates_with_converge`.
//! - suspend_invalidates_pending_without_actuation ->
//!   `suspend_invalidates_without_converge`.
//! - new_gesture_supersedes_pending ->
//!   `superseding_gesture_invalidates`.
//! - stop_while_pending_drops_without_actuation ->
//!   `stop_invalidates_without_converge`.
//! - domain_drift_after_ack_refuses -> `domain_drift_refuses`.
//! - live_member_tag_prefers_injected_without_native ->
//!   `live_member_tag_prefers_injected`.
//! - hide_reason_transition_posts_once_without_forging_visibility ->
//!   `hide_reason_transition_posts_once`.
//!
//! Accepted effects (the old scripted-apply implementation is superseded;
//! each effect stays proven by the named retained case):
//! - cross_pending_native_moved_frame_commits_with_bounded_source_row:
//!   the native frame write lands exactly and the receipt merges ->
//!   `world_native::world_native_tests::composition::tiled_cross_output_send_writes_target_frame_both_ways`,
//!   `world_native::world_native_tests::receipt_updates_store_only_visibility_proven_arrival`.
//! - cross_pending_occupied_failed_readback_retains_source_and_engine and
//!   cross_pending_empty_dest_readback_retains_source_and_engine: an
//!   accepted setter with an unmoved frame reports `PossiblyExecuted`,
//!   never `Applied`, keeps the prior receipt, and the flight stays
//!   pending ->
//!   `world_native::world_native_tests::production_setter_lie_never_reports_applied`,
//!   `world_native::world_native_tests::composition::lying_setter_keeps_no_receipt_and_flight_pending`.
//! - cross_pending_occupied_mismatch_retains_source_and_engine: drifted
//!   identity/creation withholds the frame and the receipt ->
//!   `world_native::world_native_tests::production_identity_drift_holds_without_receipt`,
//!   `world_native::world_native_tests::same_pid_creation_drift_withholds_frame_and_foreground`,
//!   `world_native::world_native_tests::receipt_updates_never_echo_unproven_desire`.
//! - cross_pending_empty_source_reflow_fail_retains_source_and_engine:
//!   the first write lands, then invalidation stops the rest and the
//!   partial outcome is reported truthfully ->
//!   `world_diff_tests::invalidation_stops_after_first_write`,
//!   `world_diff_tests::write_plus_hold_reports_partial_truthfully`.
//! - cross_pending_empty_arrival_fail_retains_source_and_engine: an
//!   unproven arrival converges with no receipt ->
//!   `proven_same_output_dispatches_and_converges_without_receipt`,
//!   `world_native::world_native_tests::receipt_updates_never_echo_unproven_desire`.
//!
//! True live gaps (no offline equivalent; every offline leg fails closed):
//! - a real setter moving a real frame to the target with a matching
//!   readback, receipt merge, and follow-focus.
//! - a setter moving a real frame to the WRONG rect (only live windows
//!   distinguish a wrong frame from an unmoved one).
//! - real renderer `Hidden` acks driving the `applied_visible` /
//!   `applied_known` mirror (`product_hide::batch` tests cover the batch
//!   fake composition only, never production ack-driven visibility).
//! - real foreground focus admitting cross-`Send` follow and
//!   `actuate_focus` success.
//! - tests_window_diag emission suite (15 tests): the batching machinery is
//!   deleted with it; retained fns are pinned by `diag_identity_break_*`,
//!   `drop_diag_sample_prunes_member`, and `float_origin_never_proves_arrival`
//!   (`FloatOrigin` is diagnostic-only).
#![forbid(unsafe_code)]
use super::rmax03_adapter_tests::test_state;
use super::{
    CrossDragSettle, MonitorArea, ObservedTarget, ObservedWindow, PendingCrossDest, RenderBridge,
    RenderCarrier, clear_pending_seams, diag_identity_broke, diag_identity_rebound,
    drop_diag_sample, hide_render_carrier, inject_monitor_areas, inject_pending_elevated,
    inject_pending_foreground, inject_pending_observe, inject_pending_stop, inject_pending_suspend,
    inject_pending_tag, live_member_tag, queue_pending_drop, service_pending_drop,
    snapshot_cross_drag,
};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tiler_core::geometry::Rect;

const DEV_A: &str = "DEV1";
const DEV_B: &str = "DEV2";
const MOVER_HWND: u64 = 0x2101;
const MOVER_PID: u32 = 6101;
const MOVER_CREATION: &str = "creation-2101";
const MOVER_TOKEN: &str = "w-mover";
const MOVER_TAG: &str = "tag-2101";
const OCC_HWND: u64 = 0x2102;
const OCC_PID: u32 = 6102;
const OCC_CREATION: &str = "creation-2102";
const OCC_TOKEN: &str = "w-occ";
const OCC_TAG: &str = "tag-2102";
const SIB_HWND: u64 = 0x2103;
const SIB_PID: u32 = 6103;
const SIB_CREATION: &str = "creation-2103";
const SIB_TOKEN: &str = "w-sib";
const SIB_TAG: &str = "tag-2103";

fn pd_me() -> crate::model::ProcessIdentity {
    crate::model::ProcessIdentity {
        pid: 5001,
        process_creation: "creation-pending-me".to_owned(),
        user_sid: "S-1-pending".to_owned(),
        session_id: 7,
        exe_path: "C:\\test\\app.exe".to_owned(),
    }
}

fn pd_areas() -> Vec<MonitorArea> {
    vec![
        MonitorArea {
            device: DEV_A.to_owned(),
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
        },
        MonitorArea {
            device: DEV_B.to_owned(),
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
        },
    ]
}

fn pd_fulls(areas: &[MonitorArea]) -> Vec<Rect> {
    areas.iter().map(|area| area.full).collect()
}

fn pd_facts() -> crate::tiling::WindowFacts {
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

fn pd_observed(
    hwnd: u64,
    token: &str,
    pid: u32,
    creation: &str,
    me: &crate::model::ProcessIdentity,
    rect: Rect,
) -> ObservedWindow {
    ObservedWindow {
        hwnd,
        token: token.to_owned(),
        outer: rect,
        visible: rect,
        insets: crate::tiling::FrameInsets::measure(rect, rect),
        identity: ObservedTarget {
            hwnd,
            pid,
            process_creation: creation.to_owned(),
            exe_path: me.exe_path.clone(),
            user_sid: me.user_sid.clone(),
            session_id: me.session_id,
            tag: MOVER_TAG.to_owned(),
        },
        facts: pd_facts(),
        member_tag: Some(MOVER_TAG.to_owned()),
    }
}

fn pd_bridge_state() -> super::TileLoop {
    let mut state = test_state();
    let channels = Arc::new(crate::render_handoff::RenderChannels::new());
    state.render_bridge = Some(RenderBridge {
        channels,
        handle: None,
        gens: [0, 0, 0],
        hide_posted: [false, false, false],
        hide_reason: [None, None, None],
        hidden_ack: [None, None, None],
        posted: std::array::from_fn(|_| std::collections::VecDeque::new()),
        applied_visible: [false, false, false],
        applied_note: [None, None, None],
        applied_ack: [None, None, None],
        applied_known: [false, false, false],
        admission_open: true,
        pending: None,
    });
    state
}

fn pd_dirs(name: &str) -> (std::path::PathBuf, crate::storage::LedgerStore) {
    // Unique per call: tests run in parallel threads of one process and the
    // ledger dir lock is exclusive.
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!(
        "omnitiler-pending-drop-{name}-{seq}-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let store = crate::storage::LedgerStore::open(&dir).expect("ledger temp dir");
    (dir, store)
}

fn mover_key() -> crate::native_guards::WindowKey {
    crate::native_guards::WindowKey {
        hwnd: MOVER_HWND,
        pid: MOVER_PID,
        creation: MOVER_CREATION.to_owned(),
    }
}

fn occ_key() -> crate::native_guards::WindowKey {
    crate::native_guards::WindowKey {
        hwnd: OCC_HWND,
        pid: OCC_PID,
        creation: OCC_CREATION.to_owned(),
    }
}

fn sib_key() -> crate::native_guards::WindowKey {
    crate::native_guards::WindowKey {
        hwnd: SIB_HWND,
        pid: SIB_PID,
        creation: SIB_CREATION.to_owned(),
    }
}

fn mover_identity(me: &crate::model::ProcessIdentity) -> crate::model::ProcessIdentity {
    crate::model::ProcessIdentity {
        pid: MOVER_PID,
        process_creation: MOVER_CREATION.to_owned(),
        user_sid: me.user_sid.clone(),
        session_id: me.session_id,
        exe_path: me.exe_path.clone(),
    }
}

fn sib_identity(me: &crate::model::ProcessIdentity) -> crate::model::ProcessIdentity {
    crate::model::ProcessIdentity {
        pid: SIB_PID,
        process_creation: SIB_CREATION.to_owned(),
        user_sid: me.user_sid.clone(),
        session_id: me.session_id,
        exe_path: me.exe_path.clone(),
    }
}

fn occ_identity(me: &crate::model::ProcessIdentity) -> crate::model::ProcessIdentity {
    crate::model::ProcessIdentity {
        pid: OCC_PID,
        process_creation: OCC_CREATION.to_owned(),
        user_sid: me.user_sid.clone(),
        session_id: me.session_id,
        exe_path: me.exe_path.clone(),
    }
}

/// Seeded two-output fixture: the mover plus a sibling tile the `DEV1`
/// view, the occupant (unless `with_occupant` is false) tiles the `DEV2`
/// view, and one world observation tick plants genuine core homing plus
/// both current views. Queues nothing; callers queue same- or cross-output
/// finishes.
struct Fx {
    state: super::TileLoop,
    me: crate::model::ProcessIdentity,
    dir: std::path::PathBuf,
    store: crate::storage::LedgerStore,
    areas: Vec<MonitorArea>,
    fulls: Vec<Rect>,
    ws_a: String,
    ws_b: String,
    seed_req: String,
    seed: tiler_core::world_report::WorldReply,
}

fn pd_fixture(with_occupant: bool) -> Fx {
    clear_pending_seams();
    let me = pd_me();
    let mut state = pd_bridge_state();
    let areas = pd_areas();
    let fulls = pd_fulls(&areas);
    state.ensure_device(DEV_A);
    state.ensure_device(DEV_B);
    let mover = mover_key();
    state
        .member_tokens
        .insert(mover.clone(), MOVER_TOKEN.to_owned());
    state
        .member_identity
        .insert(mover.clone(), mover_identity(&me));
    state
        .member_tags
        .insert(mover.clone(), MOVER_TAG.to_owned());
    let sib = sib_key();
    state
        .member_tokens
        .insert(sib.clone(), SIB_TOKEN.to_owned());
    state.member_identity.insert(sib.clone(), sib_identity(&me));
    state.member_tags.insert(sib.clone(), SIB_TAG.to_owned());
    let mover_rect = Rect {
        x: 10,
        y: 10,
        w: 400,
        h: 300,
    };
    let mut observed = vec![
        pd_observed(
            MOVER_HWND,
            MOVER_TOKEN,
            MOVER_PID,
            MOVER_CREATION,
            &me,
            mover_rect,
        ),
        pd_observed(
            SIB_HWND,
            SIB_TOKEN,
            SIB_PID,
            SIB_CREATION,
            &me,
            Rect {
                x: 420,
                y: 10,
                w: 360,
                h: 300,
            },
        ),
    ];
    inject_pending_tag(SIB_HWND, Some(SIB_TAG.to_owned()));
    if with_occupant {
        let occ = occ_key();
        state
            .member_tokens
            .insert(occ.clone(), OCC_TOKEN.to_owned());
        state.member_identity.insert(occ.clone(), occ_identity(&me));
        state.member_tags.insert(occ.clone(), OCC_TAG.to_owned());
        observed.push(pd_observed(
            OCC_HWND,
            OCC_TOKEN,
            OCC_PID,
            OCC_CREATION,
            &me,
            Rect {
                x: 810,
                y: 10,
                w: 200,
                h: 200,
            },
        ));
        inject_pending_tag(OCC_HWND, Some(OCC_TAG.to_owned()));
    }
    inject_pending_observe(observed.clone(), Vec::new(), false);
    inject_pending_tag(MOVER_HWND, Some(MOVER_TAG.to_owned()));
    inject_pending_suspend(Some(false));
    inject_pending_elevated(Some(false));
    // No foreground observed: the settle command routes run with foreground
    // 0, so cross `Send` deterministically refuses `absent-focus` offline.
    inject_pending_foreground(Some(0));
    let (dir, store) = pd_dirs(if with_occupant { "seed" } else { "seed-empty" });
    let seed = super::world_tick::run_observation_tick(
        &mut state,
        &me,
        &store,
        &dir,
        &observed,
        &[],
        &areas,
        0,
        &fulls,
        None,
    )
    .expect("seed converges");
    let home = state.member_home(&mover).expect("mover homed");
    assert_eq!(home.output, DEV_A, "mover homes on the source output");
    let ws_b = state.active_view(DEV_B).expect("destination view");
    Fx {
        state,
        me,
        dir,
        store,
        areas,
        fulls,
        ws_a: home.workspace,
        ws_b,
        seed_req: seed.request_id.clone(),
        seed,
    }
}

/// Drop point on the sibling slot's far edge (away from the mover): slot
/// centers classify as center-stack (unsupported), while an edge third
/// reorders, so a proven same-output drop actually dispatches instead of
/// refusing.
fn pd_sib_edge_drop(fx: &Fx) -> (i32, i32) {
    let frame = |token: &str| {
        fx.seed
            .windows
            .iter()
            .find(|row| row.lifetime.0 == token)
            .and_then(|row| row.effective_frame)
            .expect("seed frames the pair")
    };
    let mover = frame(MOVER_TOKEN);
    let sib = frame(SIB_TOKEN);
    let x = if mover.x > sib.x {
        sib.x + 2
    } else {
        sib.x + sib.w - 2
    };
    (x, sib.y + sib.h / 2)
}

/// Re-stage the injected observation at the seed's effective frames: the
/// steady-state condition production ticks observe (the applied layout),
/// so the command report converges onto the seeded homing instead of
/// re-homing from stale pre-seed rects.
fn pd_restage_seed_frames(fx: &mut Fx) {
    let me = fx.me.clone();
    let mut restaged = Vec::new();
    for (hwnd, token, pid, creation) in [
        (MOVER_HWND, MOVER_TOKEN, MOVER_PID, MOVER_CREATION),
        (SIB_HWND, SIB_TOKEN, SIB_PID, SIB_CREATION),
        (OCC_HWND, OCC_TOKEN, OCC_PID, OCC_CREATION),
    ] {
        let Some(frame) = fx
            .seed
            .windows
            .iter()
            .find(|row| row.lifetime.0 == token)
            .and_then(|row| row.effective_frame)
        else {
            continue;
        };
        restaged.push(pd_observed(hwnd, token, pid, creation, &me, frame));
    }
    assert!(
        restaged.iter().any(|window| window.token == MOVER_TOKEN),
        "seed frames the mover"
    );
    inject_pending_observe(restaged, Vec::new(), false);
}

fn pd_finish_fx(fx: &mut Fx) {
    let _ = std::fs::remove_dir_all(&fx.dir);
    clear_pending_seams();
}

fn pd_slot(fx: &Fx) -> bool {
    fx.state
        .render_bridge
        .as_ref()
        .and_then(|bridge| bridge.pending.as_ref())
        .is_some()
}

fn pd_prove(fx: &mut Fx) {
    let (hide_gen, hide_safety) = fx
        .state
        .render_bridge
        .as_ref()
        .and_then(|bridge| bridge.pending.as_ref())
        .map(|pending| (pending.hide_gen, pending.hide_safety))
        .expect("pending queued");
    if let Some(bridge) = fx.state.render_bridge.as_mut() {
        bridge.hidden_ack[RenderCarrier::Preview.index()] = Some((hide_gen, hide_safety));
    }
}

fn pd_queue_same(fx: &mut Fx, x: i32, y: i32) {
    let mover = mover_key();
    let occupied = queue_pending_drop(
        &mut fx.state,
        &MOVER_HWND,
        "native",
        crate::tiling::GestureIntent::MoveDrop { x, y },
        MOVER_TOKEN,
        Some(mover.clone()),
        Some(Some(MOVER_TAG.to_owned())),
        mover,
        DEV_A,
        &fx.ws_a,
        None,
    );
    assert!(!occupied, "fixture queues into an empty slot");
    pd_prove(fx);
}

fn pd_queue_cross_full(fx: &mut Fx, x: i32, y: i32, snapshots: bool) {
    let mover = mover_key();
    let settle = CrossDragSettle {
        mover_token: MOVER_TOKEN.to_owned(),
        member_key: mover.clone(),
        source_output: DEV_A.to_owned(),
        source_workspace: fx.ws_a.clone(),
        dest_output: DEV_B.to_owned(),
        dest_workspace: fx.ws_b.clone(),
        x,
        y,
        drop_prior: None,
        producer: "native".to_owned(),
    };
    let fence = snapshot_cross_drag(&fx.state, &fx.areas, &settle, false, false)
        .expect("cross fence snapshots");
    // Without START snapshots the settle skips the START-identity gate and
    // exercises the fence lifetime leg instead (admitted-between-ticks
    // gestures queue with no START capture).
    let (start_key, start_tag) = if snapshots {
        (Some(mover.clone()), Some(Some(MOVER_TAG.to_owned())))
    } else {
        (None, None)
    };
    let occupied = queue_pending_drop(
        &mut fx.state,
        &MOVER_HWND,
        "native",
        crate::tiling::GestureIntent::MoveDrop { x, y },
        MOVER_TOKEN,
        start_key,
        start_tag,
        mover,
        DEV_A,
        &fx.ws_a,
        Some(PendingCrossDest {
            output: DEV_B.to_owned(),
            workspace: fx.ws_b.clone(),
            fence,
        }),
    );
    assert!(!occupied, "fixture queues into an empty slot");
    pd_prove(fx);
}

fn pd_queue_cross(fx: &mut Fx, x: i32, y: i32) {
    pd_queue_cross_full(fx, x, y, true);
}

fn pd_service(fx: &mut Fx) {
    let areas = fx.areas.clone();
    let fulls = fx.fulls.clone();
    service_pending_drop(&mut fx.state, &fx.me, &fx.store, &fx.dir, &fulls, &areas);
}

fn pd_last_req(fx: &Fx) -> Option<String> {
    fx.state
        .world
        .last_reply()
        .map(|reply| reply.request_id.clone())
}

#[test]
fn proven_same_output_dispatches_and_converges_without_receipt() {
    // Proven same-output drop onto the sibling slot: the reorder dispatches
    // the world command (a drop that changes nothing refuses as unchanged
    // instead), the tiled arrival cannot prove offline, and the tail
    // converges with no fabricated receipt.
    let mut fx = pd_fixture(true);
    pd_restage_seed_frames(&mut fx);
    let (x, y) = pd_sib_edge_drop(&fx);
    pd_queue_same(&mut fx, x, y);
    let tick0 = fx.state.tick;
    pd_service(&mut fx);
    assert!(!pd_slot(&fx), "proven pending is consumed");
    assert_eq!(
        fx.state.tick,
        tick0 + 2,
        "dispatch plus converge, got {}",
        fx.state.tick
    );
    assert_ne!(
        pd_last_req(&fx).as_deref(),
        Some(fx.seed_req.as_str()),
        "the world command dispatched"
    );
    let home = fx
        .state
        .member_home(&mover_key())
        .expect("mover stays homed");
    assert_eq!(home.output, DEV_A);
    assert_eq!(home.workspace, fx.ws_a);
    assert!(
        !fx.state.applied_homing.contains_key(MOVER_TOKEN),
        "no receipt without live proof: {:?}",
        fx.state.applied_homing
    );
    assert!(
        fx.state.applied_views.is_empty(),
        "no view receipt without live proof"
    );
    pd_finish_fx(&mut fx);
}

#[test]
fn unproven_ack_keeps_pending() {
    let mut fx = pd_fixture(true);
    pd_queue_same(&mut fx, 400, 280);
    if let Some(bridge) = fx.state.render_bridge.as_mut() {
        bridge.hidden_ack[RenderCarrier::Preview.index()] = None;
        if let Some(pending) = bridge.pending.as_mut() {
            pending.deadline = Instant::now() + Duration::from_secs(60);
        }
    }
    let tick0 = fx.state.tick;
    pd_service(&mut fx);
    assert!(pd_slot(&fx), "unproven pending waits");
    assert_eq!(fx.state.tick, tick0, "no actuation before hidden");
    pd_finish_fx(&mut fx);
}

#[test]
fn stale_ack_pair_keeps_pending() {
    let mut fx = pd_fixture(true);
    pd_queue_same(&mut fx, 400, 280);
    if let Some(bridge) = fx.state.render_bridge.as_mut() {
        let (hide_gen, hide_safety) = bridge
            .pending
            .as_ref()
            .map(|pending| (pending.hide_gen, pending.hide_safety))
            .expect("pending");
        bridge.hidden_ack[RenderCarrier::Preview.index()] =
            Some((hide_gen.saturating_sub(1), hide_safety.saturating_sub(1)));
        if let Some(pending) = bridge.pending.as_mut() {
            pending.deadline = Instant::now() + Duration::from_secs(60);
        }
    }
    let tick0 = fx.state.tick;
    pd_service(&mut fx);
    assert!(pd_slot(&fx), "stale ack keeps pending");
    assert_eq!(fx.state.tick, tick0, "no actuation on a stale pair");
    pd_finish_fx(&mut fx);
}

#[test]
fn timeout_drops_slot() {
    let mut fx = pd_fixture(true);
    pd_queue_same(&mut fx, 400, 280);
    if let Some(bridge) = fx.state.render_bridge.as_mut() {
        bridge.hidden_ack[RenderCarrier::Preview.index()] = None;
        if let Some(pending) = bridge.pending.as_mut() {
            pending.deadline = Instant::now() - Duration::from_secs(1);
        }
    }
    let tick0 = fx.state.tick;
    pd_service(&mut fx);
    assert!(!pd_slot(&fx), "timeout drops the slot");
    assert_eq!(fx.state.tick, tick0, "timeout never converges");
    assert!(
        !fx.state.gesture_start_key.contains_key(&MOVER_HWND),
        "timeout releases START snapshots"
    );
    assert!(
        !fx.state.gesture_start_tag.contains_key(&MOVER_HWND),
        "timeout releases START snapshots"
    );
    pd_finish_fx(&mut fx);
}

#[test]
fn start_identity_mismatch_refuses() {
    let mut fx = pd_fixture(true);
    pd_queue_same(&mut fx, 400, 280);
    // Same-process HWND reuse between END and the proven hide: the live
    // lifetime tag no longer matches the START snapshot.
    inject_pending_tag(MOVER_HWND, Some("tag-reused".to_owned()));
    let tick0 = fx.state.tick;
    pd_service(&mut fx);
    assert!(!pd_slot(&fx), "identity mismatch consumes the slot");
    assert_eq!(fx.state.tick, tick0 + 1, "identity mismatch converges once");
    let home = fx
        .state
        .member_home(&mover_key())
        .expect("mover stays homed");
    assert_eq!(home.output, DEV_A);
    pd_finish_fx(&mut fx);
}

#[test]
fn member_token_remap_refuses() {
    let mut fx = pd_fixture(true);
    pd_queue_same(&mut fx, 400, 280);
    // Token remap between END and the proven hide: the queued slot names a
    // stale member while the live lookup resolves the current one.
    if let Some(bridge) = fx.state.render_bridge.as_mut()
        && let Some(pending) = bridge.pending.as_mut()
    {
        pending.member_key = crate::native_guards::WindowKey {
            hwnd: 0x9999,
            pid: 9999,
            creation: "creation-stale".to_owned(),
        };
    }
    let tick0 = fx.state.tick;
    pd_service(&mut fx);
    assert!(!pd_slot(&fx), "remap consumes the slot");
    assert_eq!(fx.state.tick, tick0 + 1, "remap converges once");
    pd_finish_fx(&mut fx);
}

#[test]
fn overlapping_end_refuses_busy() {
    clear_pending_seams();
    let mut state = pd_bridge_state();
    let intent = || crate::tiling::GestureIntent::MoveDrop { x: 400, y: 280 };
    let first = queue_pending_drop(
        &mut state,
        &MOVER_HWND,
        "native",
        intent(),
        MOVER_TOKEN,
        Some(mover_key()),
        Some(Some(MOVER_TAG.to_owned())),
        mover_key(),
        DEV_A,
        "ws-a",
        None,
    );
    assert!(!first, "first finish owns the slot");
    let second = queue_pending_drop(
        &mut state,
        &OCC_HWND,
        "native",
        intent(),
        OCC_TOKEN,
        Some(occ_key()),
        Some(Some(OCC_TAG.to_owned())),
        occ_key(),
        DEV_A,
        "ws-a",
        None,
    );
    assert!(second, "overlapping END is refused busy");
    let pending = state
        .render_bridge
        .as_ref()
        .and_then(|bridge| bridge.pending.as_ref())
        .expect("first pending owns the settle");
    assert_eq!(pending.hwnd, MOVER_HWND);
    assert_eq!(pending.mover_token, MOVER_TOKEN);
    assert_eq!(state.tick, 0, "queueing never acts");
    clear_pending_seams();
}

#[test]
fn cross_proven_send_without_observed_focus_refuses_and_converges() {
    // Proven cross finish over a stable topology: the tail reaches the
    // T2-staged `Send` dispatch, but the core refuses without an observed
    // focused window (the fixture injects foreground 0), so the slot
    // converges with no transfer and no receipts. `Send` acceptance with
    // observed focus rides the controlled foreground in
    // `production_send_route_reports_honest_outcome_without_proof`; the T2
    // decline itself is pinned by
    // `t2_staged_decline_leaves_committed_world_unchanged`.
    let mut fx = pd_fixture(true);
    inject_monitor_areas(Some(fx.areas.clone()));
    pd_queue_cross(&mut fx, 1200, 280);
    let tick0 = fx.state.tick;
    pd_service(&mut fx);
    assert!(!pd_slot(&fx), "proven cross pending is consumed");
    assert_eq!(
        fx.state.tick,
        tick0 + 2,
        "dispatch attempt plus converge, got {}",
        fx.state.tick
    );
    assert_eq!(
        pd_last_req(&fx).as_deref(),
        Some(fx.seed_req.as_str()),
        "refused Send keeps the seed reply"
    );
    let home = fx
        .state
        .member_home(&mover_key())
        .expect("mover stays homed");
    assert_eq!(home.output, DEV_A, "mover stays home");
    assert_eq!(home.workspace, fx.ws_a);
    assert!(
        !fx.state.applied_homing.contains_key(MOVER_TOKEN),
        "no homing receipt without live proof: {:?}",
        fx.state.applied_homing
    );
    assert!(
        fx.state.applied_views.is_empty(),
        "no view receipt without live proof"
    );
    pd_finish_fx(&mut fx);
}

#[test]
fn cross_empty_destination_never_switches_view() {
    // Same proven cross path with an empty destination: the refused `Send`
    // still converges, and the destination view never switches.
    let mut fx = pd_fixture(false);
    inject_monitor_areas(Some(fx.areas.clone()));
    pd_queue_cross(&mut fx, 1200, 280);
    let tick0 = fx.state.tick;
    let view_before = fx.state.active_view(DEV_B).expect("dest view");
    pd_service(&mut fx);
    assert!(!pd_slot(&fx), "proven cross pending is consumed");
    assert_eq!(fx.state.tick, tick0 + 2, "dispatch attempt plus converge");
    assert_eq!(
        fx.state.active_view(DEV_B).as_deref(),
        Some(view_before.as_str()),
        "no destination view switch"
    );
    assert!(
        fx.state.applied_views.is_empty(),
        "no view receipt without live proof"
    );
    pd_finish_fx(&mut fx);
}

#[test]
fn cross_stale_pointer_topology_refuses() {
    // The pointer output moved under the pending finish (destination FULL
    // drifted between END and service): refuse before any fence read.
    let mut fx = pd_fixture(true);
    inject_monitor_areas(Some(fx.areas.clone()));
    pd_queue_cross(&mut fx, 1200, 280);
    let mut drifted = fx.areas.clone();
    if let Some(area) = drifted.iter_mut().find(|area| area.device == DEV_B) {
        area.full.x += 800;
    }
    fx.areas = drifted;
    let tick0 = fx.state.tick;
    pd_service(&mut fx);
    assert!(!pd_slot(&fx), "stale pointer consumes the slot");
    assert_eq!(fx.state.tick, tick0 + 1, "stale pointer converges once");
    let home = fx
        .state
        .member_home(&mover_key())
        .expect("mover stays homed");
    assert_eq!(home.output, DEV_A, "mover stays home");
    assert_eq!(
        pd_last_req(&fx).as_deref(),
        Some(fx.seed_req.as_str()),
        "no Send dispatched"
    );
    pd_finish_fx(&mut fx);
}

#[test]
fn cross_stale_view_refuses() {
    // The queued fence names a destination view the output no longer shows:
    // the view gate refuses before any dispatch.
    let mut fx = pd_fixture(true);
    inject_monitor_areas(Some(fx.areas.clone()));
    pd_queue_cross(&mut fx, 1200, 280);
    if let Some(bridge) = fx.state.render_bridge.as_mut()
        && let Some(pending) = bridge.pending.as_mut()
        && let Some(dest) = pending.dest.as_mut()
    {
        dest.fence.target_workspace = "ws-stale".to_owned();
    }
    let tick0 = fx.state.tick;
    pd_service(&mut fx);
    assert!(!pd_slot(&fx), "stale view consumes the slot");
    assert_eq!(fx.state.tick, tick0 + 1, "stale view converges once");
    assert_eq!(
        pd_last_req(&fx).as_deref(),
        Some(fx.seed_req.as_str()),
        "no Send dispatched"
    );
    pd_finish_fx(&mut fx);
}

#[test]
fn cross_stale_lifetime_prunes() {
    // Same-process HWND reuse between END and the proven hide on a finish
    // queued with no START snapshots: the START gate is skipped, so the
    // fence `identity-changed` leg prunes the stale member, then converges.
    let mut fx = pd_fixture(true);
    inject_monitor_areas(Some(fx.areas.clone()));
    pd_queue_cross_full(&mut fx, 1200, 280, false);
    inject_pending_tag(MOVER_HWND, Some("tag-reused".to_owned()));
    let tick0 = fx.state.tick;
    pd_service(&mut fx);
    assert!(!pd_slot(&fx), "stale lifetime consumes the slot");
    assert_eq!(fx.state.tick, tick0 + 1, "stale lifetime converges once");
    assert!(
        !fx.state.member_tokens.contains_key(&mover_key()),
        "identity break prunes the stale member"
    );
    pd_finish_fx(&mut fx);
}

#[test]
fn cross_fresh_topology_drift_withholds() {
    // Stable pointer resolution but drifted fresh enumeration at actuation
    // time: every write withholds and the tail converges on the fresh
    // topology instead.
    let mut fx = pd_fixture(true);
    pd_queue_cross(&mut fx, 1200, 280);
    let mut drifted = fx.areas.clone();
    if let Some(area) = drifted.iter_mut().find(|area| area.device == DEV_B) {
        area.full.x += 100;
    }
    inject_monitor_areas(Some(drifted));
    let tick0 = fx.state.tick;
    pd_service(&mut fx);
    assert!(!pd_slot(&fx), "drifted topology consumes the slot");
    assert_eq!(fx.state.tick, tick0 + 2, "dispatch plus fresh converge");
    let home = fx
        .state
        .member_home(&mover_key())
        .expect("mover stays homed");
    assert_eq!(home.output, DEV_A, "mover stays home");
    assert_eq!(
        pd_last_req(&fx).as_deref(),
        Some(fx.seed_req.as_str()),
        "no Send dispatched"
    );
    pd_finish_fx(&mut fx);
}

#[test]
fn cross_suspend_refuses() {
    let mut fx = pd_fixture(true);
    inject_monitor_areas(Some(fx.areas.clone()));
    pd_queue_cross(&mut fx, 1200, 280);
    inject_pending_suspend(Some(true));
    let tick0 = fx.state.tick;
    pd_service(&mut fx);
    assert!(!pd_slot(&fx), "suspended pending is consumed");
    assert_eq!(fx.state.tick, tick0, "suspend drops without converging");
    pd_finish_fx(&mut fx);
}

#[test]
fn esc_invalidates_with_converge() {
    let mut fx = pd_fixture(true);
    pd_queue_same(&mut fx, 400, 280);
    fx.state.esc_latched.insert(MOVER_HWND);
    let tick0 = fx.state.tick;
    pd_service(&mut fx);
    assert!(!pd_slot(&fx), "Esc drops the slot");
    assert_eq!(fx.state.tick, tick0 + 1, "Esc converges once");
    pd_finish_fx(&mut fx);
}

#[test]
fn suspend_invalidates_without_converge() {
    let mut fx = pd_fixture(true);
    pd_queue_same(&mut fx, 400, 280);
    inject_pending_suspend(Some(true));
    let tick0 = fx.state.tick;
    pd_service(&mut fx);
    assert!(!pd_slot(&fx), "suspend drops the slot");
    assert_eq!(fx.state.tick, tick0, "suspend drops without converging");
    pd_finish_fx(&mut fx);
}

#[test]
fn superseding_gesture_invalidates_without_converge() {
    let mut fx = pd_fixture(true);
    pd_queue_same(&mut fx, 400, 280);
    // A new gesture for the same HWND supersedes the queued finish; the
    // new gesture owns its own settle.
    fx.state.active.insert(MOVER_HWND);
    let tick0 = fx.state.tick;
    pd_service(&mut fx);
    assert!(!pd_slot(&fx), "superseded pending is consumed");
    assert_eq!(
        fx.state.tick, tick0,
        "supersede leaves settle to the new gesture"
    );
    pd_finish_fx(&mut fx);
}

#[test]
fn stop_invalidates_without_converge() {
    let mut fx = pd_fixture(true);
    pd_queue_same(&mut fx, 400, 280);
    inject_pending_stop(true);
    let tick0 = fx.state.tick;
    pd_service(&mut fx);
    assert!(!pd_slot(&fx), "stop drops the slot");
    assert_eq!(fx.state.tick, tick0, "stop never acts");
    pd_finish_fx(&mut fx);
}

#[test]
fn domain_drift_refuses() {
    let mut fx = pd_fixture(true);
    pd_queue_same(&mut fx, 400, 280);
    // The member moved workspaces between END and the proven hide.
    if let Some(bridge) = fx.state.render_bridge.as_mut()
        && let Some(pending) = bridge.pending.as_mut()
    {
        pending.workspace = "ws-stale".to_owned();
    }
    let tick0 = fx.state.tick;
    pd_service(&mut fx);
    assert!(!pd_slot(&fx), "domain drift consumes the slot");
    assert_eq!(fx.state.tick, tick0 + 1, "domain drift converges once");
    pd_finish_fx(&mut fx);
}

#[test]
fn sticky_refuses_without_converge() {
    let mut fx = pd_fixture(true);
    pd_queue_same(&mut fx, 400, 280);
    fx.state.sticky.insert(mover_key(), false);
    let tick0 = fx.state.tick;
    pd_service(&mut fx);
    assert!(!pd_slot(&fx), "sticky consumes the slot");
    assert_eq!(fx.state.tick, tick0, "sticky never acts");
    pd_finish_fx(&mut fx);
}

#[test]
fn live_member_tag_prefers_injected_without_native() {
    clear_pending_seams();
    inject_pending_tag(0x9999, Some("tag-injected".to_owned()));
    assert_eq!(live_member_tag(0x9999), Some("tag-injected".to_owned()));
    assert_eq!(
        live_member_tag(0x8888),
        None,
        "unscripted stays None native-free"
    );
    clear_pending_seams();
}

#[test]
fn hide_reason_transition_posts_once() {
    // Consecutive hidden reason changes post one new hide generation/epoch
    // each; a repeated reason stays idempotent, visibility is never forged,
    // and the queued finish keeps its original pair (only the renderer ack
    // proves hidden).
    clear_pending_seams();
    let mut state = pd_bridge_state();
    let bridge = state.render_bridge.as_ref().expect("bridge");
    let preview = RenderCarrier::Preview.index();
    let safety_before = bridge.channels.safety();
    let gen_before = bridge.gens[preview];
    hide_render_carrier(&mut state, RenderCarrier::Preview, "finish");
    let bridge = state.render_bridge.as_ref().expect("bridge");
    let gen_first = bridge.gens[preview];
    let safety_first = bridge.channels.safety();
    assert!(gen_first > gen_before, "first hide posts");
    assert!(safety_first > safety_before, "first hide bumps safety");
    // Same reason as the queued finish stays idempotent: no new epoch.
    hide_render_carrier(&mut state, RenderCarrier::Preview, "finish");
    let bridge = state.render_bridge.as_ref().expect("bridge");
    assert_eq!(bridge.channels.safety(), safety_first);
    assert_eq!(bridge.gens[preview], gen_first);
    // Changed reason posts exactly one new hide generation/epoch.
    hide_render_carrier(&mut state, RenderCarrier::Preview, "cancel");
    let bridge = state.render_bridge.as_ref().expect("bridge");
    assert_eq!(bridge.channels.safety(), safety_first + 1);
    assert_eq!(bridge.gens[preview], gen_first + 1);
    assert_eq!(
        bridge.hide_reason[preview].as_deref(),
        Some("cancel"),
        "ack logs the true transition reason"
    );
    // Repeating the new reason stays idempotent: no per-tick feedback.
    hide_render_carrier(&mut state, RenderCarrier::Preview, "cancel");
    let bridge = state.render_bridge.as_ref().expect("bridge");
    assert_eq!(bridge.channels.safety(), safety_first + 1);
    assert_eq!(bridge.gens[preview], gen_first + 1);
    assert!(!bridge.applied_visible[preview]);
    assert!(!bridge.applied_known[preview]);
    clear_pending_seams();
}

fn diag_sample() -> super::WindowDiagSample {
    super::WindowDiagSample {
        output: DEV_A.to_owned(),
        workspace: "ws-a".to_owned(),
        op: "drag-drop".to_owned(),
        correlation: "corr-1".to_owned(),
        hwnd: MOVER_HWND,
        exe: "app.exe".to_owned(),
        outcome: "applied",
        timeout_ms: 50,
        query_us: 7,
        carried: tiler_core::size_hints::WindowSizeHints::none(),
        fallback: false,
        fallback_age_ms: 0,
        fallback_reason: "fresh",
        observed_outer: None,
        insets: [0; 4],
        planned: None,
        effective: None,
        outer: None,
        pre: None,
        post: None,
        disposition: "applied",
        suppress: "none",
        hint_tick: 1,
        write_tick: 2,
    }
}

#[test]
fn diag_identity_break_marks_class_unknown_until_rebound() {
    // An identity break drops the published baseline (it describes a stale
    // window) and marks the class unknown so a recycled HWND never reports
    // the old window's class; a later trusted query re-binds.
    let mut state = pd_bridge_state();
    state
        .diag_emitted
        .insert(MOVER_TOKEN.to_owned(), diag_sample());
    state
        .diag_class
        .insert(MOVER_TOKEN.to_owned(), "Notepad".to_owned());
    diag_identity_broke(&mut state, MOVER_TOKEN);
    assert!(
        !state.diag_emitted.contains_key(MOVER_TOKEN),
        "stale baseline drops"
    );
    assert_eq!(
        state.diag_class.get(MOVER_TOKEN).map(String::as_str),
        Some(""),
        "class marked unknown"
    );
    diag_identity_rebound(&mut state, MOVER_TOKEN);
    assert!(
        !state.diag_class.contains_key(MOVER_TOKEN),
        "trusted query re-binds"
    );
    // A trusted cached class is never dropped by the rebound.
    state
        .diag_class
        .insert(MOVER_TOKEN.to_owned(), "Notepad".to_owned());
    diag_identity_rebound(&mut state, MOVER_TOKEN);
    assert_eq!(
        state.diag_class.get(MOVER_TOKEN).map(String::as_str),
        Some("Notepad"),
        "trusted class survives"
    );
    clear_pending_seams();
}

#[test]
fn drop_diag_sample_prunes_member() {
    let mut state = pd_bridge_state();
    state
        .diag_pending
        .insert(MOVER_TOKEN.to_owned(), diag_sample());
    state
        .diag_emitted
        .insert(MOVER_TOKEN.to_owned(), diag_sample());
    state
        .diag_class
        .insert(MOVER_TOKEN.to_owned(), "Notepad".to_owned());
    state.diag_emit_seq.insert(MOVER_TOKEN.to_owned(), 3);
    state.diag_coalesced.insert(MOVER_TOKEN.to_owned());
    drop_diag_sample(&mut state, MOVER_TOKEN);
    assert!(!state.diag_pending.contains_key(MOVER_TOKEN));
    assert!(!state.diag_emitted.contains_key(MOVER_TOKEN));
    assert!(!state.diag_class.contains_key(MOVER_TOKEN));
    assert!(!state.diag_emit_seq.contains_key(MOVER_TOKEN));
    assert!(!state.diag_coalesced.contains(MOVER_TOKEN));
    clear_pending_seams();
}

fn arrival_reply(
    out: &str,
    rows: Vec<tiler_core::world_report::WindowDesire>,
) -> tiler_core::world_report::WorldReply {
    use tiler_core::world_report::{ScopeAuthority as Auth, ScopeStatus};
    tiler_core::world_report::WorldReply {
        contract_version: tiler_core::world_report::WORLD_CONTRACT_VERSION,
        adapter_incarnation: "adapter-1".to_owned(),
        core_generation: "gen-1".to_owned(),
        request_id: "req-9".to_owned(),
        based_on_observation_seq: 9,
        state_revision: 9,
        ready: true,
        command_consumed: false,
        scopes: vec![ScopeStatus {
            scope: format!("{out}/ws1"),
            authority: Auth::Controlled,
        }],
        workspaces: Vec::new(),
        outputs: Vec::new(),
        windows: rows,
        transfers: Vec::new(),
        effects: Vec::new(),
        command_result: None,
        presentation_bytes: 0,
    }
}

fn arrival_row(
    out: &str,
    token: &str,
    origin: Option<tiler_core::world_report::FloatOrigin>,
) -> tiler_core::world_report::WindowDesire {
    use tiler_core::world_report::ScopeAuthority as Auth;
    tiler_core::world_report::WindowDesire {
        lifetime: tiler_core::directional::WindowId(token.to_owned()),
        output: tiler_core::directional::OutputId(out.to_owned()),
        workspace: "ws1".to_owned(),
        allocation: None,
        effective_frame: None,
        frame_authority: Auth::Untouched,
        membership_authority: Auth::Controlled,
        floating: true,
        float_origin: origin,
        frame_operation: None,
    }
}

#[test]
fn float_origin_never_proves_arrival() {
    // `FloatOrigin` is diagnostic-only: a frameless float arrival settles on
    // the proven applied-homing receipt alone, never on the origin mark.
    use tiler_core::world_report::FloatOrigin;
    let mut state = pd_bridge_state();
    state.ensure_device(DEV_A);
    let out = state.output_token(DEV_A);
    let me = pd_me();
    let areas = pd_areas();
    let fulls = pd_fulls(&areas);
    for origin in [None, Some(FloatOrigin::Capacity)] {
        let reply = arrival_reply(&out, vec![arrival_row(&out, "w1", origin)]);
        assert!(
            !super::drop_arrival_verified(
                &mut state, &me, &fulls, &areas, &reply, "w1", 9999, None
            ),
            "no receipt proves nothing (origin {origin:?})"
        );
    }
    state
        .applied_homing
        .insert("w1".to_owned(), (out.clone(), "ws1".to_owned()));
    for origin in [None, Some(FloatOrigin::Capacity)] {
        let reply = arrival_reply(&out, vec![arrival_row(&out, "w1", origin)]);
        assert!(
            super::drop_arrival_verified(&mut state, &me, &fulls, &areas, &reply, "w1", 9999, None),
            "matching receipt proves (origin {origin:?})"
        );
    }
    clear_pending_seams();
}

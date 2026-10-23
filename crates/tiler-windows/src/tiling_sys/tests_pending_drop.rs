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
//! converge and +0 means none). The service bumps once per proven release
//! (the release tick itself applies the accepted reply), so a proven
//! release lands at +1 with no extra converge step: the commit tick IS
//! the converge, and later ordinary ticks continue from it. Receipts
//! (`applied_homing` / `applied_views`) merge only from live native proof,
//! so they stay empty on fake HWNDs: tests assert they are never fabricated.
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
//! - start_identity_mismatch_refuses -> `start_identity_mismatch_refuses`
//!   (START snapshots) and `cross_stale_lifetime_refuses` (stored tag leg
//!   without snapshots).
//! - member_token_remap_refuses -> `member_token_remap_refuses`.
//! - occupied_newcomer_is_refused_narrowly ->
//!   `overlapping_end_refuses_busy`.
//! - cross_pending_proven_hide_settles_transfer_with_arrival ->
//!   `cross_proven_release_commits_transfer_without_receipt` (the core
//!   commits the gesture transfer from the stashed END facts; the tiled
//!   arrival cannot prove offline, so the tail holds without converging
//!   and without fabricating receipts).
//! - cross_pending_empty_native_flow_admits_then_reflows ->
//!   `cross_empty_destination_never_switches_view` (the core commits the
//!   transfer; the destination view never switches).
//! - stale pointer topology (destination FULL drifted between END and
//!   service) -> `cross_stale_pointer_topology_refuses` (the core
//!   resolves against the fresh staged observation; no adapter pointer
//!   check remains).
//! - cross_pending_suspend_refuses_without_actuation ->
//!   `cross_suspend_refuses`.
//! - cross_pending_timeout_refuses_without_actuation -> shared pre-check,
//!   covered by `timeout_drops_slot`.
//! - esc_invalidates_pending_with_converge ->
//!   `esc_invalidates_with_converge`.
//! - suspend_invalidates_pending_without_actuation ->
//!   `suspend_invalidates_without_converge`.
//! - new_gesture_supersedes_pending ->
//!   `superseding_gesture_invalidates_without_converge`.
//! - stop_while_pending_drops_without_actuation ->
//!   `stop_invalidates_without_converge`.
//! - sticky (core-owned hold, no adapter gate) ->
//!   `sticky_holds_without_commit`.
//! - normalized producer mapping and render consumption (stage 4a):
//!   `gesture_ingress_maps_single_open_gesture` (Started down-point, then
//!   Updated live),
//!   `gesture_ingress_ignores_multi_unmanaged_unknown_and_missing_snapshot`,
//!   `gesture_ingress_prefers_stashed_override`, and
//!   `observation_tick_publishes_drop_overlay_from_ingress` pin the
//!   adapter fact collection (START binding, kind, PRE frame, Esc) and
//!   the drop-overlay consumption path (observation tick publishes the
//!   core overlay; the release commits without a command).
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
//! - real foreground focus admitting cross-`Send` follow and the single
//!   spent directional follow-focus effect.
//! - tests_window_diag emission suite (15 tests): the batching machinery is
//!   deleted with it; retained fns are pinned by `diag_identity_break_*`,
//!   `drop_diag_sample_prunes_member`, and `float_origin_never_proves_arrival`
//!   (`FloatOrigin` is diagnostic-only).
#![forbid(unsafe_code)]
use super::rmax03_adapter_tests::test_state;
use super::{
    MonitorArea, ObservedTarget, ObservedWindow, RenderBridge, RenderCarrier, clear_pending_seams,
    diag_identity_broke, diag_identity_rebound, drop_diag_sample, hide_render_carrier,
    inject_pending_elevated, inject_pending_foreground, inject_pending_observe,
    inject_pending_stop, inject_pending_suspend, inject_pending_tag, live_member_tag,
    queue_pending_drop, service_pending_drop,
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
    Fx {
        state,
        me,
        dir,
        store,
        areas,
        fulls,
        ws_a: home.workspace,
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

/// Drop point inside win-a's left-top edge zone: a sibling-driven move
/// reorders there (mirrors what `pd_sib_edge_drop` proves for the mover:
/// an edge third, never the center stack, never the mover's own slot).
fn pd_sib_drop_for_win_a(fx: &Fx) -> (i32, i32) {
    let frame = fx
        .seed
        .windows
        .iter()
        .find(|row| row.lifetime.0 == MOVER_TOKEN)
        .and_then(|row| row.effective_frame)
        .expect("seed frames the mover");
    (frame.x + frame.w / 4, frame.y + frame.h / 12)
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

fn pd_restage_mover_at(fx: &mut Fx, rect: Rect) {
    // Stage the mover observed at the release point (the native frame
    // follows the pointer before END in production): the core derives the
    // release intent from START-vs-release frames, so an unmoved offline
    // frame would classify as no-change and snap back.
    let me = fx.me.clone();
    let mut restaged = Vec::new();
    for (hwnd, token, pid, creation) in [
        (OCC_HWND, OCC_TOKEN, OCC_PID, OCC_CREATION),
        (SIB_HWND, SIB_TOKEN, SIB_PID, SIB_CREATION),
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
    restaged.push(pd_observed(
        MOVER_HWND,
        MOVER_TOKEN,
        MOVER_PID,
        MOVER_CREATION,
        &me,
        rect,
    ));
    inject_pending_observe(restaged, Vec::new(), false);
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

/// Allocate the stable gesture id the way the native START drain does
/// (monotonic, never zero, retained across the gesture).
fn pd_next_id(state: &mut super::TileLoop) -> u64 {
    state.next_gesture_seq += 1;
    state.next_gesture_seq
}

fn pd_queue_same(fx: &mut Fx, x: i32, y: i32) {
    use tiler_core::gesture::GestureKind;
    let mover = mover_key();
    // START-captured PRE frame (the mover's seed frame): the core release
    // ground truth.
    let start_frame = fx
        .seed
        .windows
        .iter()
        .find(|row| row.lifetime.0 == MOVER_TOKEN)
        .and_then(|row| row.effective_frame)
        .expect("seed frames the mover");
    let qid = pd_next_id(&mut fx.state);
    let occupied = queue_pending_drop(
        &mut fx.state,
        &MOVER_HWND,
        qid,
        "native",
        "drag-drop",
        Some((x, y)),
        None,
        false,
        start_frame,
        GestureKind::Move,
        MOVER_TOKEN,
        Some(mover.clone()),
        Some(Some(MOVER_TAG.to_owned())),
        mover,
    );
    assert!(!occupied, "fixture queues into an empty slot");
    pd_prove(fx);
}

/// Queue one bridged finish over the pointer output (same helper as the
/// same-output path: the core resolves the destination from the staged
/// observation, so the adapter carries no destination or fence snapshot).
/// `snapshots` controls the START-identity capture: without it the settle
/// skips the START gate, exercising the member-lookup leg instead.
fn pd_queue_cross_full(fx: &mut Fx, x: i32, y: i32, snapshots: bool) {
    use tiler_core::gesture::GestureKind;
    let mover = mover_key();
    let start_frame = fx
        .seed
        .windows
        .iter()
        .find(|row| row.lifetime.0 == MOVER_TOKEN)
        .and_then(|row| row.effective_frame)
        .expect("seed frames the mover");
    // Without START snapshots the settle skips the START-identity gate and
    // exercises the member-lookup leg instead (admitted-between-ticks
    // gestures queue with no START capture).
    let (start_key, start_tag) = if snapshots {
        (Some(mover.clone()), Some(Some(MOVER_TAG.to_owned())))
    } else {
        (None, None)
    };
    let qid = pd_next_id(&mut fx.state);
    let occupied = queue_pending_drop(
        &mut fx.state,
        &MOVER_HWND,
        qid,
        "native",
        "drag-drop",
        Some((x, y)),
        None,
        false,
        start_frame,
        GestureKind::Move,
        MOVER_TOKEN,
        start_key,
        start_tag,
        mover,
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
    // Proven same-output drop onto the sibling slot: the release tick
    // dispatches through the core, which settles without committing (the
    // offline frame never moved, so the intent classifies as no-change),
    // then converges once with no fabricated receipt. The terminal record
    // is what the service logs - never the bare accepted reply.
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
        "service bump plus honest settle converge, got {}",
        fx.state.tick
    );
    assert_ne!(
        pd_last_req(&fx).as_deref(),
        Some(fx.seed_req.as_str()),
        "the release tick dispatched"
    );
    assert!(
        fx.state.gesture_override.is_none(),
        "the terminal incorporated: no stashed override survives"
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
    use tiler_core::geometry::Rect;
    use tiler_core::gesture::GestureKind;
    clear_pending_seams();
    let mut state = pd_bridge_state();
    let frame = Rect {
        x: 10,
        y: 10,
        w: 400,
        h: 300,
    };
    let qid_first = pd_next_id(&mut state);
    let first = queue_pending_drop(
        &mut state,
        &MOVER_HWND,
        qid_first,
        "native",
        "drag-drop",
        Some((400, 280)),
        None,
        false,
        frame,
        GestureKind::Move,
        MOVER_TOKEN,
        Some(mover_key()),
        Some(Some(MOVER_TAG.to_owned())),
        mover_key(),
    );
    assert!(!first, "first finish owns the slot");
    let qid_second = pd_next_id(&mut state);
    let second = queue_pending_drop(
        &mut state,
        &OCC_HWND,
        qid_second,
        "native",
        "drag-drop",
        Some((400, 280)),
        None,
        false,
        frame,
        GestureKind::Move,
        OCC_TOKEN,
        Some(occ_key()),
        Some(Some(OCC_TAG.to_owned())),
        occ_key(),
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
fn cross_proven_release_commits_transfer_without_receipt() {
    // Proven cross finish over a stable topology: the core commits the
    // gesture transfer (membership move plus follow) from the stashed END
    // facts. The tiled arrival cannot prove offline, so the tail holds
    // without converging and without fabricating receipts. (The retired
    // `Send` dispatch refused here for absent focus; the gesture commit
    // needs no observed focus.)
    let mut fx = pd_fixture(true);
    pd_queue_cross(&mut fx, 1200, 280);
    // Production drags hold the mover focused; the fixture seeds foreground
    // 0, so prove focus for the commit leg. Stage the mover observed at the
    // release point (the native frame follows the pointer before END).
    inject_pending_foreground(Some(MOVER_HWND));
    // Same-size frame at the release point: moves keep the PRE-gesture
    // size, so the core derives a move (a changed size would derive a
    // resize instead).
    pd_restage_mover_at(
        &mut fx,
        Rect {
            x: 900,
            y: 10,
            w: 400,
            h: 300,
        },
    );
    // START-ground the queued release on the observed-at-queue frame (the
    // queue helper seeds the desired frame; the START capture would have
    // frozen the observed one).
    if let Some(bridge) = fx.state.render_bridge.as_mut()
        && let Some(pending) = bridge.pending.as_mut()
    {
        pending.start_frame = Rect {
            x: 10,
            y: 10,
            w: 400,
            h: 300,
        };
    }
    let tick0 = fx.state.tick;
    pd_service(&mut fx);
    assert!(!pd_slot(&fx), "proven cross pending is consumed");
    assert_eq!(
        fx.state.tick,
        tick0 + 1,
        "service bump without converge, got {}",
        fx.state.tick
    );
    let reply = fx.state.world.last_reply().expect("reply ran");
    let transfer = reply
        .transfers
        .iter()
        .find(|t| matches!(&t.kind, tiler_core::world_report::TransferKind::Window(w) if w.0 == MOVER_TOKEN))
        .expect("cross release retains a window transfer");
    assert_eq!(
        transfer.target_output.0,
        fx.state.output_token(DEV_B),
        "cross release targets out-b"
    );
    assert!(transfer.follow, "cross drop follows");
    let home = fx
        .state
        .member_home(&mover_key())
        .expect("mover stays homed");
    assert_eq!(home.output, DEV_B, "committed mover homes away");
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
    // Same proven cross path with an empty destination: the core commits
    // the transfer, the tail holds for unverified arrival, and the
    // destination view never switches.
    let mut fx = pd_fixture(false);
    pd_queue_cross(&mut fx, 1200, 280);
    inject_pending_foreground(Some(MOVER_HWND));
    pd_restage_mover_at(
        &mut fx,
        Rect {
            x: 900,
            y: 10,
            w: 400,
            h: 300,
        },
    );
    if let Some(bridge) = fx.state.render_bridge.as_mut()
        && let Some(pending) = bridge.pending.as_mut()
    {
        pending.start_frame = Rect {
            x: 10,
            y: 10,
            w: 400,
            h: 300,
        };
    }
    let tick0 = fx.state.tick;
    let view_before = fx.state.active_view(DEV_B).expect("dest view");
    pd_service(&mut fx);
    assert!(!pd_slot(&fx), "proven cross pending is consumed");
    assert_eq!(
        fx.state.tick,
        tick0 + 1,
        "service bump without converge, got {}",
        fx.state.tick
    );
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
    // drifted between END and service): the core resolves against the
    // fresh staged observation, so the drop refuses with no transfer and
    // no fabricated receipts. No adapter pointer check remains.
    let mut fx = pd_fixture(true);
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
    let reply = fx.state.world.last_reply().expect("reply ran");
    assert!(
        reply.transfers.is_empty(),
        "drifted topology retains nothing"
    );
    assert!(
        !fx.state.applied_homing.contains_key(MOVER_TOKEN),
        "no receipt without live proof"
    );
    pd_finish_fx(&mut fx);
}

#[test]
fn cross_suspend_refuses() {
    let mut fx = pd_fixture(true);
    pd_queue_cross(&mut fx, 1200, 280);
    inject_pending_suspend(Some(true));
    let tick0 = fx.state.tick;
    pd_service(&mut fx);
    assert!(!pd_slot(&fx), "suspended pending is consumed");
    assert_eq!(fx.state.tick, tick0, "suspend drops without converging");
    pd_finish_fx(&mut fx);
}

#[test]
fn cross_stale_lifetime_refuses() {
    // Same-process HWND reuse between END and the proven hide on a finish
    // queued with no START snapshots: the START gate is skipped, so the
    // stored member-lifetime tag leg refuses (the live tag no longer
    // matches the stamped one), then converges with no commit.
    let mut fx = pd_fixture(true);
    pd_queue_cross_full(&mut fx, 1200, 280, false);
    inject_pending_tag(MOVER_HWND, Some("tag-reused".to_owned()));
    let tick0 = fx.state.tick;
    pd_service(&mut fx);
    assert!(!pd_slot(&fx), "stale lifetime consumes the slot");
    assert_eq!(fx.state.tick, tick0 + 1, "stale lifetime converges once");
    let reply = fx.state.world.last_reply().expect("reply ran");
    assert!(
        reply.transfers.is_empty(),
        "reused lifetime retains nothing"
    );
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
fn sticky_holds_without_commit() {
    // Sticky is a core-owned refusal: the service still runs the release
    // tick (the slot consumes), but the core settles without committing,
    // so the service logs an honest refusal and converges once, with no
    // fabricated receipts. No adapter float gate remains.
    let mut fx = pd_fixture(true);
    pd_queue_same(&mut fx, 400, 280);
    fx.state.sticky.insert(mover_key(), false);
    let tick0 = fx.state.tick;
    pd_service(&mut fx);
    assert!(!pd_slot(&fx), "sticky consumes the slot");
    assert_eq!(
        fx.state.tick,
        tick0 + 2,
        "sticky runs the tick and converges"
    );
    assert!(
        fx.state.gesture_override.is_none(),
        "the terminal incorporated: no stashed override survives"
    );
    let reply = fx.state.world.last_reply().expect("reply ran");
    assert!(reply.transfers.is_empty(), "sticky retains nothing");
    assert!(
        !fx.state.applied_homing.contains_key(MOVER_TOKEN),
        "no receipt without live proof"
    );
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
        gesture_settled: None,
        cursor_warp: None,
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
        clear_operation: None,
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

/// Stage an open gesture on the sibling window (the "next window"): full
/// START tables plus a fresh stable id, mirroring a native START drain.
fn pd_stage_sib_gesture(fx: &mut Fx, kind: crate::group_underlay::MoveSizeKind) {
    use super::PreviewStartBinding;
    let frame = fx
        .seed
        .windows
        .iter()
        .find(|row| row.lifetime.0 == SIB_TOKEN)
        .and_then(|row| row.effective_frame)
        .expect("seed frames the sibling");
    fx.state.managed.insert(SIB_HWND);
    fx.state.active.insert(SIB_HWND);
    fx.state.gesture_before.insert(SIB_HWND, frame);
    fx.state.gesture_preview_start.insert(
        SIB_HWND,
        PreviewStartBinding {
            token: SIB_TOKEN.to_owned(),
            output: DEV_A.to_owned(),
            workspace: fx.ws_a.clone(),
        },
    );
    fx.state.move_kind.insert(SIB_HWND, kind);
    fx.state.gesture_start_cursor.insert(SIB_HWND, (20, 20));
    fx.state.next_gesture_seq += 1;
    let seq = fx.state.next_gesture_seq;
    fx.state.gesture_seq.insert(SIB_HWND, seq);
}

/// Drive one observation tick carrying the loop's current ingress (stashed
/// override or open tables) with an explicit cursor sample, returning the
/// reply. The central terminal drain runs inside, so incorporation clears
/// a matching override.
fn pd_dispatch(
    fx: &mut Fx,
    observed: &[ObservedWindow],
    cursor: Option<(i32, i32)>,
    ms: u64,
) -> Option<tiler_core::world_report::WorldReply> {
    pd_dispatch_fg(fx, observed, cursor, ms, MOVER_HWND)
}

/// [`pd_dispatch`] with an explicit foreground window (proves which
/// window the adapter observed focused; the game lane never reaches here).
fn pd_dispatch_fg(
    fx: &mut Fx,
    observed: &[ObservedWindow],
    cursor: Option<(i32, i32)>,
    ms: u64,
    foreground: u64,
) -> Option<tiler_core::world_report::WorldReply> {
    let areas = fx.areas.clone();
    super::world_tick::dispatch_test_observation(
        &mut fx.state,
        observed,
        &[],
        &areas,
        foreground,
        cursor,
        ms,
    )
}

/// Restage the seeded effective frames as the injected observation and
/// return the staged list for direct dispatches.
fn pd_seed_observed(fx: &mut Fx) -> Vec<ObservedWindow> {
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
    inject_pending_observe(restaged.clone(), Vec::new(), false);
    restaged
}

/// Stage one open managed move gesture on the fixture mover: the START
/// tables a native START drain would fill (token binding, PRE frame,
/// modal kind, down-point cursor), plus loop membership. Returns the PRE
/// frame.
fn pd_stage_open_gesture(fx: &mut Fx, kind: crate::group_underlay::MoveSizeKind) -> Rect {
    use super::PreviewStartBinding;
    let frame = fx
        .seed
        .windows
        .iter()
        .find(|row| row.lifetime.0 == MOVER_TOKEN)
        .and_then(|row| row.effective_frame)
        .expect("seed frames the mover");
    fx.state.managed.insert(MOVER_HWND);
    fx.state.active.insert(MOVER_HWND);
    fx.state.gesture_before.insert(MOVER_HWND, frame);
    // Stable id, allocated the way the native START drain does.
    fx.state.next_gesture_seq += 1;
    let seq = fx.state.next_gesture_seq;
    fx.state.gesture_seq.insert(MOVER_HWND, seq);
    fx.state.gesture_preview_start.insert(
        MOVER_HWND,
        PreviewStartBinding {
            token: MOVER_TOKEN.to_owned(),
            output: DEV_A.to_owned(),
            workspace: fx.ws_a.clone(),
        },
    );
    fx.state.move_kind.insert(MOVER_HWND, kind);
    // Down-point cursor distinct from any live sample below, so the
    // Started/Updated phase split is observable.
    fx.state.gesture_start_cursor.insert(MOVER_HWND, (10, 10));
    frame
}

#[test]
fn gesture_ingress_maps_single_open_gesture() {
    // Normalized producer mapping: the first tick carrying the open
    // managed gesture reports `Started` with the down-point cursor; later
    // ticks report `Updated` with the live cursor. The core owns every
    // target/release decision from here.
    use crate::group_underlay::MoveSizeKind;
    let mut fx = pd_fixture(true);
    let frame = pd_stage_open_gesture(&mut fx, MoveSizeKind::Move);
    let ingress = fx
        .state
        .gesture_ingress(Some((400, 280)))
        .expect("open gesture maps");
    assert_eq!(ingress.window.0, MOVER_TOKEN);
    assert_eq!(ingress.id, 1, "stable id allocated at START");
    assert_eq!(ingress.kind, tiler_core::gesture::GestureKind::Move);
    assert_eq!(ingress.cursor, Some((10, 10)), "down point first");
    assert_eq!(ingress.start_cursor, Some((10, 10)));
    assert_eq!(ingress.phase, tiler_core::gesture::GesturePhase::Started);
    assert!(!ingress.esc);
    assert!(ingress.modal_move);
    assert_eq!(ingress.start_frame, frame);
    // The next tick carries the live cursor as an update with the same id
    // and the same START down point.
    let ingress = fx
        .state
        .gesture_ingress(Some((400, 280)))
        .expect("held gesture maps");
    assert_eq!(ingress.id, 1, "id is stable across samples");
    assert_eq!(ingress.cursor, Some((400, 280)), "live cursor next");
    assert_eq!(ingress.start_cursor, Some((10, 10)));
    assert_eq!(ingress.phase, tiler_core::gesture::GesturePhase::Updated);
    // Esc latch rides the ingress; the core clears on it.
    fx.state.esc_latched.insert(MOVER_HWND);
    let ingress = fx
        .state
        .gesture_ingress(Some((400, 280)))
        .expect("esc still maps");
    assert!(ingress.esc, "Esc fact rides the ingress");
    pd_finish_fx(&mut fx);
}

#[test]
fn gesture_ingress_ignores_multi_unmanaged_unknown_and_missing_snapshot() {
    // Fail-closed producer mapping: multi-gesture opens, unmanaged races,
    // unknown START kinds, and missing PRE snapshots emit nothing (the
    // barrier bool still holds scopes; the core never sees a gesture).
    use crate::group_underlay::MoveSizeKind;
    let mut fx = pd_fixture(true);
    // Multi-gesture: two HWNDs open at once.
    pd_stage_open_gesture(&mut fx, MoveSizeKind::Move);
    fx.state.managed.insert(SIB_HWND);
    fx.state.active.insert(SIB_HWND);
    fx.state.gesture_before.insert(
        SIB_HWND,
        Rect {
            x: 0,
            y: 0,
            w: 10,
            h: 10,
        },
    );
    assert!(
        fx.state.gesture_ingress(Some((400, 280))).is_none(),
        "multi-gesture emits nothing"
    );
    fx.state.active.remove(&SIB_HWND);
    fx.state.managed.remove(&SIB_HWND);
    fx.state.gesture_before.remove(&SIB_HWND);
    // Unknown START kind never triggers.
    fx.state.move_kind.insert(MOVER_HWND, MoveSizeKind::Unknown);
    assert!(
        fx.state.gesture_ingress(Some((400, 280))).is_none(),
        "unknown kind emits nothing"
    );
    fx.state.move_kind.insert(MOVER_HWND, MoveSizeKind::Resize);
    let ingress = fx
        .state
        .gesture_ingress(Some((400, 280)))
        .expect("resize maps");
    assert_eq!(ingress.kind, tiler_core::gesture::GestureKind::Resize);
    assert_eq!(
        ingress.phase,
        tiler_core::gesture::GesturePhase::Started,
        "first sighting starts"
    );
    assert!(!ingress.modal_move, "resize carries no modal truth");
    // Missing PRE snapshot fails closed.
    fx.state.gesture_before.remove(&MOVER_HWND);
    assert!(
        fx.state.gesture_ingress(Some((400, 280))).is_none(),
        "missing snapshot emits nothing"
    );
    pd_finish_fx(&mut fx);
}

#[test]
fn gesture_ingress_prefers_stashed_override() {
    // Terminal phases ride an explicit stashed override (END-captured
    // cursor, never re-read), taking precedence over the open tables.
    use tiler_core::gesture::{GestureIngress, GestureKind, GesturePhase};
    let mut fx = pd_fixture(true);
    pd_stage_open_gesture(&mut fx, crate::group_underlay::MoveSizeKind::Move);
    let frame = fx
        .seed
        .windows
        .iter()
        .find(|row| row.lifetime.0 == MOVER_TOKEN)
        .and_then(|row| row.effective_frame)
        .expect("seed frames the mover");
    fx.state.gesture_override = Some(GestureIngress {
        id: 1,
        window: tiler_core::directional::WindowId(MOVER_TOKEN.to_owned()),
        kind: GestureKind::Move,
        cursor: Some((1200, 280)),
        start_cursor: Some((10, 10)),
        phase: GesturePhase::Released,
        esc: false,
        modal_move: true,
        start_frame: frame,
    });
    let ingress = fx
        .state
        .gesture_ingress(Some((1, 1)))
        .expect("override maps");
    assert_eq!(ingress.phase, GesturePhase::Released);
    assert_eq!(ingress.cursor, Some((1200, 280)), "END cursor wins");
    pd_finish_fx(&mut fx);
}

#[test]
fn observation_tick_publishes_drop_overlay_from_ingress() {
    // Drop-overlay render consumption: an observation tick whose capture
    // carries the Updated ingress publishes the core drop-overlay shape;
    // the pump renders from that reply instead of dispatching its own
    // command. No command is consumed.
    use tiler_core::gesture::{GestureKind, GesturePhase};
    let mut fx = pd_fixture(true);
    pd_restage_seed_frames(&mut fx);
    pd_stage_open_gesture(&mut fx, crate::group_underlay::MoveSizeKind::Move);
    let (x, y) = pd_sib_edge_drop(&fx);
    let mut restaged = Vec::new();
    let me = fx.me.clone();
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
    inject_pending_observe(restaged.clone(), Vec::new(), false);
    // The first tick carries `Started`: bind only, no overlay. The second
    // tick carries the moved live cursor as `Updated` and publishes the
    // core drop-overlay shape.
    let bound = super::world_tick::dispatch_test_observation(
        &mut fx.state,
        &restaged,
        &[],
        &fx.areas,
        MOVER_HWND,
        Some((x, y)),
        1,
    )
    .expect("bind tick runs");
    assert!(bound.command_result.is_none(), "bind carries no overlay");
    let reply = super::world_tick::dispatch_test_observation(
        &mut fx.state,
        &restaged,
        &[],
        &fx.areas,
        MOVER_HWND,
        Some((x, y)),
        2,
    )
    .expect("observation tick runs");
    let Some(tiler_core::world_report::CommandResult::DragPreview {
        rect,
        domain,
        source,
        target,
        ..
    }) = reply.command_result.clone()
    else {
        panic!(
            "expected drop-overlay result, got {:?}",
            reply.command_result
        );
    };
    assert!(rect.w > 0 && rect.h > 0, "core-derived overlay rect");
    assert_ne!(target.0, MOVER_TOKEN, "neighbor target, never the mover");
    assert_eq!(source, domain, "same-output capture");
    assert!(reply.transfers.is_empty(), "preview retains nothing");
    assert!(
        !reply.command_consumed,
        "observation carries no command to consume"
    );
    // A release through the same ingress path commits without a command:
    // restage the mover shifted (same size, new origin) so the release
    // derives a move, then release at the same drop point.
    let start = fx
        .seed
        .windows
        .iter()
        .find(|row| row.lifetime.0 == MOVER_TOKEN)
        .and_then(|row| row.effective_frame)
        .expect("seed frames the mover");
    let shifted = Rect {
        x: start.x + 50,
        y: start.y,
        w: start.w,
        h: start.h,
    };
    let mut restaged2 = Vec::new();
    for window in &restaged {
        if window.token == MOVER_TOKEN {
            restaged2.push(pd_observed(
                MOVER_HWND,
                MOVER_TOKEN,
                MOVER_PID,
                MOVER_CREATION,
                &me,
                shifted,
            ));
        } else {
            restaged2.push(window.clone());
        }
    }
    inject_pending_observe(restaged2.clone(), Vec::new(), false);
    let before = reply
        .windows
        .iter()
        .find(|row| row.lifetime.0 == MOVER_TOKEN)
        .and_then(|row| row.effective_frame);
    let release = tiler_core::gesture::GestureIngress {
        id: 1,
        window: tiler_core::directional::WindowId(MOVER_TOKEN.to_owned()),
        kind: GestureKind::Move,
        cursor: Some((x, y)),
        start_cursor: Some((10, 10)),
        phase: GesturePhase::Released,
        esc: false,
        modal_move: true,
        start_frame: start,
    };
    fx.state.gesture_override = Some(release);
    let released = super::world_tick::dispatch_test_observation(
        &mut fx.state,
        &restaged2,
        &[],
        &fx.areas,
        MOVER_HWND,
        Some((x, y)),
        3,
    )
    .expect("release tick runs");
    assert!(
        fx.state.gesture_override.is_none(),
        "incorporated terminal clears the stashed override"
    );
    assert_eq!(
        released.gesture_settled,
        Some(tiler_core::gesture::GestureSettled {
            id: 1,
            committed: true
        }),
        "the reply honestly reports the commit"
    );
    assert!(
        released.command_result.is_none(),
        "release rides the snapshot"
    );
    let after = released
        .windows
        .iter()
        .find(|row| row.lifetime.0 == MOVER_TOKEN)
        .and_then(|row| row.effective_frame);
    assert_ne!(after, before, "release commits the drop without a command");
    assert!(released.transfers.is_empty(), "same-output retains nothing");
    pd_finish_fx(&mut fx);
}

#[test]
fn esc_end_delivers_terminal_then_next_window_binds() {
    // Producer START binds a core session, then Esc ends the gesture: the
    // native tables clear, but the terminal stashes instead of leaking the
    // session. Incorporation clears the stash, and the next window's
    // gesture binds instead of being ignored as a replacement.
    use crate::group_underlay::MoveSizeKind;
    use tiler_core::gesture::GesturePhase;
    let mut fx = pd_fixture(true);
    pd_stage_open_gesture(&mut fx, MoveSizeKind::Move);
    let restaged = pd_seed_observed(&mut fx);
    // Bind the session through one Started tick.
    let bound = pd_dispatch(&mut fx, &restaged, None, 1).expect("bind tick runs");
    assert!(bound.command_result.is_none(), "bind carries no overlay");
    // END arrives with Esc latched (native END drain already removed the
    // HWND from `active`, exactly like production).
    fx.state.esc_latched.insert(MOVER_HWND);
    fx.state.active.remove(&MOVER_HWND);
    super::gesture_tick::gesture_tick(
        &mut fx.state,
        &fx.me,
        &fx.store,
        &fx.dir,
        &fx.fulls,
        &fx.areas,
        &[MOVER_HWND],
    );
    let stashed = fx
        .state
        .gesture_override
        .as_ref()
        .expect("Esc stashes a terminal instead of leaking the session");
    assert_eq!(stashed.id, 1);
    assert_eq!(stashed.phase, GesturePhase::Cancelled);
    // The next eligible observation incorporates the terminal.
    let incorporated = pd_dispatch(&mut fx, &restaged, None, 2).expect("incorporation runs");
    assert!(
        fx.state.gesture_override.is_none(),
        "incorporation clears the stash"
    );
    assert_eq!(
        incorporated.gesture_settled,
        Some(tiler_core::gesture::GestureSettled {
            id: 1,
            committed: false
        }),
        "cancel records an uncommitted terminal"
    );
    // The next window's gesture binds and previews: nothing leaked. The
    // sibling is the observed foreground (a native gesture always runs on
    // the focused window) and the drop point sits in win-a's edge zone so
    // the resolver plans instead of refusing.
    pd_stage_sib_gesture(&mut fx, MoveSizeKind::Move);
    let bound = pd_dispatch_fg(&mut fx, &restaged, None, 3, SIB_HWND).expect("sibling bind runs");
    assert!(bound.command_result.is_none(), "bind carries no overlay");
    let (sx, sy) = pd_sib_drop_for_win_a(&fx);
    let preview = pd_dispatch_fg(&mut fx, &restaged, Some((sx, sy)), 4, SIB_HWND)
        .expect("sibling update runs");
    assert!(
        matches!(
            preview.command_result,
            Some(tiler_core::world_report::CommandResult::DragPreview { .. })
        ),
        "next window previews after the terminal cleared, got {:?}",
        preview.command_result
    );
    pd_finish_fx(&mut fx);
}

#[test]
fn nochange_end_delivers_terminal_then_next_window_binds() {
    // Zero-movement END: the observed frame never left the PRE frame, so
    // nothing could commit, but the terminal still stashes and
    // incorporates instead of leaking the preview-bound session.
    use crate::group_underlay::MoveSizeKind;
    use tiler_core::gesture::GesturePhase;
    let mut fx = pd_fixture(true);
    pd_stage_open_gesture(&mut fx, MoveSizeKind::Move);
    let restaged = pd_seed_observed(&mut fx);
    let bound = pd_dispatch(&mut fx, &restaged, None, 1).expect("bind tick runs");
    assert!(bound.command_result.is_none(), "bind carries no overlay");
    fx.state.active.remove(&MOVER_HWND);
    super::gesture_tick::gesture_tick(
        &mut fx.state,
        &fx.me,
        &fx.store,
        &fx.dir,
        &fx.fulls,
        &fx.areas,
        &[MOVER_HWND],
    );
    let stashed = fx
        .state
        .gesture_override
        .as_ref()
        .expect("no-change END stashes a terminal");
    assert_eq!(stashed.id, 1);
    assert_eq!(stashed.phase, GesturePhase::Cancelled);
    let _ = pd_dispatch(&mut fx, &restaged, None, 2);
    assert!(
        fx.state.gesture_override.is_none(),
        "incorporation clears the stash"
    );
    pd_stage_sib_gesture(&mut fx, MoveSizeKind::Move);
    let bound = pd_dispatch_fg(&mut fx, &restaged, None, 3, SIB_HWND).expect("sibling bind runs");
    assert!(bound.command_result.is_none(), "bind carries no overlay");
    let (sx, sy) = pd_sib_drop_for_win_a(&fx);
    let preview = pd_dispatch_fg(&mut fx, &restaged, Some((sx, sy)), 4, SIB_HWND)
        .expect("sibling update runs");
    assert!(
        matches!(
            preview.command_result,
            Some(tiler_core::world_report::CommandResult::DragPreview { .. })
        ),
        "next window previews after the terminal cleared, got {:?}",
        preview.command_result
    );
    pd_finish_fx(&mut fx);
}

#[test]
fn pending_timeout_stashes_terminal_for_next_observation() {
    // A proven hide that never acks drops the slot at the deadline, but
    // the terminal stashes for the next eligible observation instead of
    // leaking the core session. No actuation runs on the timeout itself.
    let mut fx = pd_fixture(true);
    pd_queue_same(&mut fx, 400, 280);
    let queued_id = fx
        .state
        .render_bridge
        .as_ref()
        .and_then(|bridge| bridge.pending.as_ref())
        .map(|pending| pending.id)
        .expect("pending queued");
    if let Some(bridge) = fx.state.render_bridge.as_mut() {
        bridge.hidden_ack[RenderCarrier::Preview.index()] = None;
        if let Some(pending) = bridge.pending.as_mut() {
            pending.deadline = Instant::now() - Duration::from_secs(1);
        }
    }
    let tick0 = fx.state.tick;
    pd_service(&mut fx);
    assert!(!pd_slot(&fx), "timeout drops the slot");
    assert_eq!(fx.state.tick, tick0, "timeout never acts");
    let stashed = fx
        .state
        .gesture_override
        .as_ref()
        .expect("timeout stashes the terminal");
    assert_eq!(stashed.id, queued_id);
    assert_eq!(stashed.phase, tiler_core::gesture::GesturePhase::Cancelled);
    let restaged = pd_seed_observed(&mut fx);
    let incorporated = pd_dispatch(&mut fx, &restaged, None, 1).expect("incorporation runs");
    assert!(
        fx.state.gesture_override.is_none(),
        "incorporation clears the stash"
    );
    assert_eq!(
        incorporated.gesture_settled,
        Some(tiler_core::gesture::GestureSettled {
            id: queued_id,
            committed: false
        }),
    );
    pd_finish_fx(&mut fx);
}

#[test]
fn pending_stop_invalidation_stashes_terminal_without_actuation() {
    // Stop while a finish waits: the slot drops with no actuation, but the
    // terminal stashes so the next eligible observation still incorporates
    // it instead of leaking the session.
    let mut fx = pd_fixture(true);
    pd_queue_same(&mut fx, 400, 280);
    let queued_id = fx
        .state
        .render_bridge
        .as_ref()
        .and_then(|bridge| bridge.pending.as_ref())
        .map(|pending| pending.id)
        .expect("pending queued");
    inject_pending_stop(true);
    let tick0 = fx.state.tick;
    pd_service(&mut fx);
    assert!(!pd_slot(&fx), "stop drops the slot");
    assert_eq!(fx.state.tick, tick0, "stop never acts");
    let stashed = fx
        .state
        .gesture_override
        .as_ref()
        .expect("stop stashes the terminal");
    assert_eq!(stashed.id, queued_id);
    inject_pending_stop(false);
    let restaged = pd_seed_observed(&mut fx);
    let incorporated = pd_dispatch(&mut fx, &restaged, None, 1).expect("incorporation runs");
    assert!(
        fx.state.gesture_override.is_none(),
        "incorporation clears the stash"
    );
    assert_eq!(
        incorporated.gesture_settled,
        Some(tiler_core::gesture::GestureSettled {
            id: queued_id,
            committed: false
        }),
    );
    pd_finish_fx(&mut fx);
}

#[test]
fn enum_failure_keeps_pending_slot() {
    // A transient enumeration failure while a proven finish waits loses
    // nothing: the slot (and its deadline) survives for the next service,
    // and no terminal stashes for a gesture that never ran.
    let mut fx = pd_fixture(true);
    pd_queue_same(&mut fx, 400, 280);
    inject_pending_observe(vec![], vec![], true);
    let tick0 = fx.state.tick;
    pd_service(&mut fx);
    assert!(pd_slot(&fx), "transient failure keeps the slot");
    assert_eq!(fx.state.tick, tick0, "failure never acts");
    assert!(
        fx.state.gesture_override.is_none(),
        "no terminal without a run"
    );
    // Recovery proceeds on the next service with fresh observations.
    pd_restage_seed_frames(&mut fx);
    pd_service(&mut fx);
    assert!(!pd_slot(&fx), "recovery consumes the slot");
    pd_finish_fx(&mut fx);
}

#[test]
fn clear_blocked_gesture_stashes_cancel_for_open_gesture() {
    // A blocked turn with an open gesture stashes its terminal before
    // clearing the tables, so suspension cannot leak the core session.
    use crate::group_underlay::MoveSizeKind;
    let mut fx = pd_fixture(true);
    pd_stage_open_gesture(&mut fx, MoveSizeKind::Move);
    super::run_loop_tick::clear_blocked_gesture(&mut fx.state);
    assert!(
        fx.state.gesture_before.is_empty(),
        "blocked turn clears tables"
    );
    let stashed = fx
        .state
        .gesture_override
        .as_ref()
        .expect("blocked turn stashes the terminal");
    assert_eq!(stashed.phase, tiler_core::gesture::GesturePhase::Cancelled);
    assert_eq!(stashed.id, 1);
    let restaged = pd_seed_observed(&mut fx);
    let incorporated = pd_dispatch(&mut fx, &restaged, None, 1).expect("incorporation runs");
    assert!(
        fx.state.gesture_override.is_none(),
        "incorporation clears the stash"
    );
    assert_eq!(
        incorporated.gesture_settled,
        Some(tiler_core::gesture::GestureSettled {
            id: 1,
            committed: false
        }),
    );
    pd_finish_fx(&mut fx);
}

#[test]
fn rejected_tick_never_claims_and_keeps_stashed_terminal() {
    // A report the core refuses (out-of-bounds cursor) runs nothing: the
    // tick returns None instead of a cached reply, the stashed terminal
    // survives for the next eligible observation, and no terminal is
    // recorded. The next valid tick then incorporates and honest-logs it.
    use tiler_core::gesture::{GestureIngress, GestureKind, GesturePhase};
    let mut fx = pd_fixture(true);
    pd_seed_observed(&mut fx);
    pd_stage_open_gesture(&mut fx, crate::group_underlay::MoveSizeKind::Move);
    let restaged = pd_seed_observed(&mut fx);
    let _ = pd_dispatch(&mut fx, &restaged, None, 1).expect("bind runs");
    // Stash a terminal release directly: the END facts arrived while the
    // world path is about to refuse the report.
    let start = fx
        .seed
        .windows
        .iter()
        .find(|row| row.lifetime.0 == MOVER_TOKEN)
        .and_then(|row| row.effective_frame)
        .expect("seed frames the mover");
    fx.state.gesture_override = Some(GestureIngress {
        id: 7,
        window: tiler_core::directional::WindowId(MOVER_TOKEN.to_owned()),
        kind: GestureKind::Move,
        cursor: Some((150, 50)),
        start_cursor: Some((10, 10)),
        phase: GesturePhase::Released,
        esc: false,
        modal_move: true,
        start_frame: start,
    });
    let refused = pd_dispatch(&mut fx, &restaged, Some((99999999, 0)), 2);
    assert!(refused.is_none(), "refused report claims nothing");
    assert!(
        fx.state.gesture_override.is_some(),
        "terminal survives refusal"
    );
    // The next eligible tick incorporates the terminal with no commit
    // (the observed frame never moved from the PRE frame).
    let incorporated =
        pd_dispatch(&mut fx, &restaged, Some((150, 50)), 3).expect("incorporation runs");
    assert!(
        fx.state.gesture_override.is_none(),
        "incorporation clears the stash"
    );
    assert_eq!(
        incorporated.gesture_settled,
        Some(tiler_core::gesture::GestureSettled {
            id: 7,
            committed: false
        }),
    );
    pd_finish_fx(&mut fx);
}

#[test]
fn stale_cancel_override_cleared_by_newer_start() {
    // A stashed terminal for an older id on the same window never rides
    // past a newer native START: allocation supersedes it immediately.
    let mut fx = pd_fixture(true);
    fx.state.gesture_override = Some(tiler_core::gesture::GestureIngress {
        id: 5,
        window: tiler_core::directional::WindowId(MOVER_TOKEN.to_owned()),
        kind: tiler_core::gesture::GestureKind::Move,
        cursor: None,
        start_cursor: None,
        phase: tiler_core::gesture::GesturePhase::Cancelled,
        esc: false,
        modal_move: true,
        start_frame: Rect {
            x: 0,
            y: 0,
            w: 10,
            h: 10,
        },
    });
    fx.state.next_gesture_seq = 5;
    let me = pd_me();
    super::native_drag::capture_gesture_start(&mut fx.state, &me, MOVER_HWND, 99);
    assert!(
        fx.state.gesture_override.is_none(),
        "newer START drops the stale cancel"
    );
    assert_eq!(
        fx.state.gesture_seq.get(&MOVER_HWND).copied(),
        Some(6),
        "new START allocates a fresh id"
    );
    pd_finish_fx(&mut fx);
}

#[test]
fn inline_release_preserves_down_point() {
    // Inline END regression: the START down point must be read before the
    // per-gesture tables clear, so the Released ingress carries it. An
    // out-of-bounds release cursor forces the tick to refuse
    // (gesture-cursor-invalid), leaving the stashed override in place for
    // inspection: it must still carry the down point.
    use crate::group_underlay::MoveSizeKind;
    use tiler_core::gesture::GesturePhase;
    let mut fx = pd_fixture(true);
    pd_stage_open_gesture(&mut fx, MoveSizeKind::Move);
    // Moved observed frame so the END is not a no-change short-circuit.
    let start = fx
        .seed
        .windows
        .iter()
        .find(|row| row.lifetime.0 == MOVER_TOKEN)
        .and_then(|row| row.effective_frame)
        .expect("seed frames the mover");
    let shifted = Rect {
        x: start.x + 50,
        y: start.y,
        w: start.w,
        h: start.h,
    };
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
        let frame = if token == MOVER_TOKEN { shifted } else { frame };
        restaged.push(pd_observed(hwnd, token, pid, creation, &me, frame));
    }
    inject_pending_observe(restaged.clone(), Vec::new(), false);
    // END arrived: the HWND left `active`, and the release cursor was
    // captured out of bounds (staged directly, bypassing the normalized
    // event drain, to force refusal after the override is built).
    // Same-thread inline path (no render bridge): the release runs
    // through the ordinary observation tick.
    fx.state.render_bridge = None;
    fx.state.active.remove(&MOVER_HWND);
    fx.state
        .gesture_end_cursor
        .insert(MOVER_HWND, (99999999, 0));
    super::gesture_tick::gesture_tick(
        &mut fx.state,
        &fx.me,
        &fx.store,
        &fx.dir,
        &fx.fulls,
        &fx.areas,
        &[MOVER_HWND],
    );
    let stashed = fx
        .state
        .gesture_override
        .as_ref()
        .expect("refused release stays stashed");
    assert_eq!(stashed.id, 1, "stable id allocated at START");
    assert_eq!(stashed.phase, GesturePhase::Released);
    assert_eq!(
        stashed.cursor,
        Some((99999999, 0)),
        "END-captured cursor rides as-is"
    );
    assert_eq!(
        stashed.start_cursor,
        Some((10, 10)),
        "down point read before table clear"
    );
    pd_finish_fx(&mut fx);
}

#[test]
fn stale_delivery_claims_nothing_but_clears_on_incorporation() {
    // Stale acknowledgment regression through the production dispatch,
    // deliver, and ack path: a racing invalidate between dispatch and
    // ack acknowledges Stale. The tick contract maps Stale to None (never
    // the cached last reply), while the incorporated terminal still
    // clears the matching stash exactly once: incorporation already
    // happened, only the claim is withheld. A replay of the same id on
    // the next tick records nothing.
    use tiler_core::gesture::{GestureIngress, GestureKind, GesturePhase};
    let mut fx = pd_fixture(true);
    let restaged = pd_seed_observed(&mut fx);
    let first = pd_dispatch(&mut fx, &restaged, None, 1).expect("seed tick runs");
    assert!(
        fx.state.world.last_reply().is_some(),
        "a cached reply exists to not return"
    );
    let _ = first;
    // Stash a terminal release for the mover (no-change settle: the
    // observed frame never moved from the PRE frame).
    let start = fx
        .seed
        .windows
        .iter()
        .find(|row| row.lifetime.0 == MOVER_TOKEN)
        .and_then(|row| row.effective_frame)
        .expect("seed frames the mover");
    fx.state.gesture_override = Some(GestureIngress {
        id: 7,
        window: tiler_core::directional::WindowId(MOVER_TOKEN.to_owned()),
        kind: GestureKind::Move,
        cursor: Some((150, 50)),
        start_cursor: Some((10, 10)),
        phase: GesturePhase::Released,
        esc: false,
        modal_move: true,
        start_frame: start,
    });
    // Mirror `dispatch_test_observation` (production seam): gesture from
    // the stashed override, disjoint borrows for the owner inputs.
    use super::TileLoop;
    let gesture = fx.state.gesture_ingress(Some((150, 50)));
    assert!(
        gesture.as_ref().is_some_and(|ingress| ingress.id == 7),
        "override rides the capture"
    );
    let TileLoop {
        ref mut world,
        ref member_tokens,
        ref member_identity,
        ref member_tags,
        ref member_rects,
        ref float_rects,
        ref hint_logged,
        ref sticky,
        ref floated,
        ref restart_reject_hold,
        ref born_fullscreen,
        ref born_first_exit,
        ref fixed_auto_seen,
        ref fixed_tile_overrides,
        ref fixed_origin_overrides,
        ref mut hidden_claims,
        ref minimized_omitted,
        ref mut pending_closed,
        ref scope,
        ref settings_live,
        ref output_tokens,
        ref mut applied_homing,
        ref mut applied_views,
        inner_gap,
        outer_gap,
        ref active,
        ..
    } = fx.state;
    world.invalidate();
    let inputs = TileLoop::owner_inputs(
        member_tokens,
        member_identity,
        member_tags,
        member_rects,
        float_rects,
        hint_logged,
        sticky,
        floated,
        restart_reject_hold,
        born_fullscreen,
        born_first_exit,
        fixed_auto_seen,
        fixed_tile_overrides,
        fixed_origin_overrides,
        hidden_claims,
        minimized_omitted,
        pending_closed,
        scope,
        settings_live,
        output_tokens,
        applied_homing,
        applied_views,
        inner_gap,
        outer_gap,
        active,
        Some((150, 50)),
        None,
        gesture.clone(),
    );
    let report = world
        .try_dispatch_observation(&inputs, Some(&restaged), &[], &fx.areas, MOVER_HWND, 2)
        .expect("dispatch runs");
    // Racing tick input lands between dispatch and ack (the production
    // stale race): the watermark advances past the flight.
    world.invalidate();
    let reply = match world.deliver_production(&report) {
        tiler_core::world_engine::WorldOutcome::Accepted(reply) => reply,
        other => panic!("expected accepted delivery, got {other:?}"),
    };
    let (ack, _) = world.ack(&reply, pending_closed);
    assert!(
        matches!(ack, super::world_sequencer::AckOutcome::Stale(_)),
        "racing invalidate acknowledges stale, got {ack:?}"
    );
    // Tick contract: stale never presents the cached reply as the outcome.
    let out = match ack {
        super::world_sequencer::AckOutcome::Applied(_) => {
            world.note_diff_applied(&reply);
            Some(reply.clone())
        }
        super::world_sequencer::AckOutcome::Stale(_) => None,
        super::world_sequencer::AckOutcome::Ignored { .. } => None,
    };
    assert!(out.is_none(), "stale claims nothing");
    assert!(
        fx.state.world.last_reply().is_some(),
        "cached reply still exists, just not returned"
    );
    // The incorporated terminal clears the matching stash exactly once
    // (never replayed): the fresh reply carries it, even though the tick
    // claims nothing.
    TileLoop::clear_gesture_override_on_incorporation(&mut fx.state.gesture_override, &reply);
    assert!(
        fx.state.gesture_override.is_none(),
        "incorporation clears the stash even on stale"
    );
    assert_eq!(
        reply.gesture_settled,
        Some(tiler_core::gesture::GestureSettled {
            id: 7,
            committed: false
        }),
        "no-change terminal records uncommitted"
    );
    pd_finish_fx(&mut fx);
}

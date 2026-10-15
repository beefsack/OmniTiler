#![forbid(unsafe_code)]

//! Owned `input-session` post legs (split from [`crate::input_session`];
//! no behavior change).
//!
//! Responsibility: one synthetic post per edge through the shared guarded
//! boundary plus settle readbacks, fault-injection legs, and the per-route
//! leg sequence. No tiling or frame writes anywhere on this path.

use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::ffi::{CarbonChordStatus, TapSourceReport};
use crate::focus::SpaceSnapshot;
use crate::input::cg_flag_bits;
use crate::input_session::{
    ABSENCE_WAIT, ArrowStatus, CtxMeta, DELIVERY_WAIT, InputSessionRequest, InputSessionRoute,
    KEY_H, OwnerHit, PostLeg, bucket_index, extra_flag_for, gated_post, latest_helper_counts,
    pump_main,
};
use crate::native_input::{TapTileFactory, tile_catalogue};

pub(crate) fn delta(before: u64, after: u64) -> u64 {
    after.saturating_sub(before)
}

/// Post one full chord (down, brief hold, up) with the oracle naming the
/// target before EACH edge, then settle: wait for the owner's indexed hit
/// (when `expect_owner_index` is set; the hit must carry that exact index
/// and the down edge) and read the helper counter deltas. Absence legs
/// (`expect_owner_index == None`) hold the full absence window with no hit
/// as their (bounded, reported) evidence. Suppression legs (`absence_hold`)
/// additionally hold the absence window AFTER the expected hit before the
/// helper readback, so helper absence is measured, not assumed.
#[allow(clippy::too_many_arguments)]
pub(crate) fn post_chord_leg(
    child: &mut std::process::Child,
    meta: &CtxMeta,
    lines: &mpsc::Receiver<String>,
    hits: &mpsc::Receiver<OwnerHit>,
    name: &str,
    key_code: u32,
    flags: u64,
    expect_owner_index: Option<usize>,
    absence_hold: bool,
) -> Result<PostLeg, String> {
    while hits.try_recv().is_ok() {}
    let before = poll_helper_counts(lines, Duration::from_millis(1200))
        .ok_or_else(|| format!("{name}: no helper key-count snapshot before post"))?;
    let t_down = Instant::now();
    let down = gated_post(child, meta, key_code as u16, flags, true)?;
    std::thread::sleep(Duration::from_millis(80));
    let up = gated_post(child, meta, key_code as u16, flags, false)?;
    let (owner_latency_ms, after) = match expect_owner_index {
        Some(index) => {
            let hit = await_owner_hit(hits, DELIVERY_WAIT)?;
            if hit.index != index || !hit.down {
                return Err(format!(
                    "{name}: owner hit mismatch (want index {index} down, have index {} down={})",
                    hit.index, hit.down
                ));
            }
            if absence_hold {
                // The paired up edge (same index, down=false) legitimately
                // arrives inside the hold; anything else (a second down, a
                // foreign index) fails it.
                hold_absence_allow_up(hits, name, index)?;
            }
            let after = poll_helper_counts(lines, Duration::from_millis(1200))
                .ok_or_else(|| format!("{name}: no helper key-count snapshot after post"))?;
            let latency = hit
                .at
                .saturating_duration_since(t_down)
                .as_millis()
                .min(u128::from(u64::MAX)) as u64;
            (Some(latency), after)
        }
        None => {
            hold_absence(hits, name)?;
            let after = poll_helper_counts(lines, Duration::from_millis(1200))
                .ok_or_else(|| format!("{name}: no helper key-count snapshot after post"))?;
            (None, after)
        }
    };
    let want_bucket = bucket_index(meta.want_flags);
    let extra_bucket = bucket_index(meta.want_flags | extra_flag_for(meta.modifier));
    let plain_bucket = bucket_index(0);
    Ok(PostLeg {
        name: name.to_owned(),
        key_code,
        flags,
        down_post_us: down.post_latency_us,
        up_post_us: up.post_latency_us,
        owner_latency_ms,
        helper_exact_delta: (
            delta(before.h_down[want_bucket], after.h_down[want_bucket]),
            delta(before.h_up[want_bucket], after.h_up[want_bucket]),
        ),
        helper_extra_delta: (
            delta(before.h_down[extra_bucket], after.h_down[extra_bucket]),
            delta(before.h_up[extra_bucket], after.h_up[extra_bucket]),
        ),
        helper_plain_delta: (
            delta(before.h_down[plain_bucket], after.h_down[plain_bucket]),
            delta(before.h_up[plain_bucket], after.h_up[plain_bucket]),
        ),
        direct_enabled_before: None,
        direct_enabled_after: None,
        verdict: "posted".to_owned(),
    })
}

/// Fault-injection exact leg (tap + `--stall-once-ms` only): posts the
/// exact down (consumed by the one-shot callback stall), then a bounded
/// burst of plain chords WHILE the callback sleeps so events back up
/// behind it — a lone slow callback under no load did not trip the OS
/// timeout (measured live: 1200ms stall, reenables=0), while a stalled tap
/// under stream load is the condition the timeout guards — then the exact
/// up. Expectations: the owner down hit arrives, the stalled down never
/// reaches the helper, and the tap saw at least the down (the disable
/// verdict itself comes from the re-enable counter in the stall stage, and
/// bypassed burst/up arrivals at the helper are recorded, never asserted).
#[allow(clippy::too_many_arguments)]
pub(crate) fn post_stall_leg(
    child: &mut std::process::Child,
    meta: &CtxMeta,
    lines: &mpsc::Receiver<String>,
    hits: &mpsc::Receiver<OwnerHit>,
    tap_factory: Option<&TapTileFactory>,
    h_index: usize,
    want: u64,
    burst_chords: u64,
) -> Result<PostLeg, String> {
    const NAME: &str = "exact";
    while hits.try_recv().is_ok() {}
    let tap_before = tap_factory
        .map(|f| f.latest_report())
        .unwrap_or(TapSourceReport {
            matched: 0,
            suppressed: 0,
            deliveries: 0,
            reenables_after_timeout: 0,
        });
    let before = poll_helper_counts(lines, Duration::from_millis(1200))
        .ok_or_else(|| format!("{NAME}: no helper key-count snapshot before post"))?;
    let t_down = Instant::now();
    let down = gated_post(child, meta, KEY_H as u16, want, true)?;
    // Burst load behind the sleeping callback (plain chords match nothing,
    // so they never consume or disturb the stall; each post stays
    // oracle-gated like every other post).
    for _ in 0..burst_chords {
        gated_post(child, meta, KEY_H as u16, 0, true)?;
        gated_post(child, meta, KEY_H as u16, 0, false)?;
    }
    std::thread::sleep(Duration::from_millis(80));
    let up = gated_post(child, meta, KEY_H as u16, want, false)?;
    let hit = await_owner_hit(hits, DELIVERY_WAIT)?;
    if hit.index != h_index || !hit.down {
        return Err(format!(
            "{NAME}: owner hit mismatch (want index {h_index} down, have index {} down={})",
            hit.index, hit.down
        ));
    }
    hold_absence_allow_up(hits, NAME, h_index)?;
    let after = poll_helper_counts(lines, Duration::from_millis(1200))
        .ok_or_else(|| format!("{NAME}: no helper key-count snapshot after post"))?;
    let tap_after = tap_factory
        .map(|f| f.latest_report())
        .unwrap_or(TapSourceReport {
            matched: 0,
            suppressed: 0,
            deliveries: 0,
            reenables_after_timeout: 0,
        });
    let want_bucket = bucket_index(meta.want_flags);
    let extra_bucket = bucket_index(meta.want_flags | extra_flag_for(meta.modifier));
    let plain_bucket = bucket_index(0);
    let mut leg = PostLeg {
        name: NAME.to_owned(),
        key_code: KEY_H,
        flags: want,
        down_post_us: down.post_latency_us,
        up_post_us: up.post_latency_us,
        owner_latency_ms: Some(
            hit.at
                .saturating_duration_since(t_down)
                .as_millis()
                .min(u128::from(u64::MAX)) as u64,
        ),
        helper_exact_delta: (
            delta(before.h_down[want_bucket], after.h_down[want_bucket]),
            delta(before.h_up[want_bucket], after.h_up[want_bucket]),
        ),
        helper_extra_delta: (
            delta(before.h_down[extra_bucket], after.h_down[extra_bucket]),
            delta(before.h_up[extra_bucket], after.h_up[extra_bucket]),
        ),
        helper_plain_delta: (
            delta(before.h_down[plain_bucket], after.h_down[plain_bucket]),
            delta(before.h_up[plain_bucket], after.h_up[plain_bucket]),
        ),
        direct_enabled_before: None,
        direct_enabled_after: None,
        verdict: "posted".to_owned(),
    };
    // The stalled down must never reach the helper; the burst/up may
    // bypass while the tap sits OS-disabled (recorded, decided later).
    if leg.helper_exact_delta.0 != 0 {
        return Err(format!(
            "stall down escaped suppression (helper exact {:?})",
            leg.helper_exact_delta
        ));
    }
    if leg.helper_extra_delta != (0, 0) {
        return Err(format!(
            "stall leg touched the extra bucket (extra {:?})",
            leg.helper_extra_delta
        ));
    }
    leg.verdict = format!(
        "stall-consumed down-suppressed up-bypassed={} plain-bypassed={:?} tap matched+{} suppressed+{} reenables={} latency_ms={}",
        leg.helper_exact_delta.1,
        leg.helper_plain_delta,
        tap_after.matched.saturating_sub(tap_before.matched),
        tap_after.suppressed.saturating_sub(tap_before.suppressed),
        tap_after.reenables_after_timeout,
        leg.owner_latency_ms.unwrap_or(u64::MAX)
    );
    Ok(leg)
}
/// Bounded absence window with main-thread pumping: any owner hit inside
/// fails the leg. Used for no-hit variants AND as the suppression hold
/// after an expected hit (helper absence is then measured, not assumed).
pub(crate) fn hold_absence(hits: &mpsc::Receiver<OwnerHit>, name: &str) -> Result<(), String> {
    let deadline = Instant::now() + ABSENCE_WAIT;
    while Instant::now() < deadline {
        pump_main();
        if let Ok(hit) = hits.try_recv() {
            return Err(format!(
                "{name}: unexpected owner hit during absence window (index {})",
                hit.index
            ));
        }
    }
    Ok(())
}

/// Suppression hold after an expected down hit: the paired up edge (same
/// index, down=false) is the only permitted arrival inside the window; a
/// second down or any foreign index fails the leg.
pub(crate) fn hold_absence_allow_up(
    hits: &mpsc::Receiver<OwnerHit>,
    name: &str,
    index: usize,
) -> Result<(), String> {
    let deadline = Instant::now() + ABSENCE_WAIT;
    while Instant::now() < deadline {
        pump_main();
        if let Ok(hit) = hits.try_recv()
            && (hit.down || hit.index != index)
        {
            return Err(format!(
                "{name}: unexpected owner hit during suppression hold (index {} down={})",
                hit.index, hit.down
            ));
        }
    }
    Ok(())
}

/// Bounded wait for the actual OS tap-state readback (`CGEventTapIsEnabled`
/// as published by the owner thread): returns the observed state once it
/// equals `want`, or a truthful timeout error. Unknown (`None` published)
/// never satisfies either want. Pumps the controller main while waiting.
pub(crate) fn await_direct_enabled(
    factory: &TapTileFactory,
    want: Option<bool>,
    name: &str,
) -> Result<bool, String> {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if let Some(state) = factory.latest_direct_enabled()
            && Some(state) == want
        {
            return Ok(state);
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "{name}: actual tap-state readback never reached {want:?} (have {:?})",
                factory.latest_direct_enabled()
            ));
        }
        pump_main();
    }
}

/// DIRECT disabled-post leg (tap + `--direct-disable-hold-ms` only): posts
/// the exact chord while the tap sits DIRECTLY OS-disabled, with the hold
/// covering ONLY the post edges (about the requested hold milliseconds):
/// helper snapshots and the absence proof run OUTSIDE the hold, so a long
/// direct-disable never invites the OS user-disable response (measured live:
/// a ~4s hold drew `TapDisabledByUserInput` and killed the owner). Steps:
/// snapshot before (tap enabled), hold, await the actual `==false`
/// readback, oracle-gated down/up edges, extend the hold to the requested
/// span, release, await the actual `==true` readback, gate owner liveness
/// (refusal unset, channel connected), then the absence window plus the
/// after snapshot prove the tap never saw the chord (no hit) while the
/// helper proves passthrough (exact +1/+1). Labeled DIRECT throughout.
#[allow(clippy::too_many_arguments)]
pub(crate) fn post_direct_disabled_leg(
    child: &mut std::process::Child,
    meta: &CtxMeta,
    lines: &mpsc::Receiver<String>,
    hits: &mpsc::Receiver<OwnerHit>,
    factory: &TapTileFactory,
    want: u64,
    hold_target_ms: u64,
) -> Result<PostLeg, String> {
    const NAME: &str = "direct-disabled-exact";
    while hits.try_recv().is_ok() {}
    let before = poll_helper_counts(lines, Duration::from_millis(1200))
        .ok_or_else(|| format!("{NAME}: no helper key-count snapshot before post"))?;
    let hold_start = Instant::now();
    factory.set_direct_hold(true);
    let disabled = await_direct_enabled(factory, Some(false), "direct-disable")
        .inspect_err(|_| factory.set_direct_hold(false))?;
    let t_down = Instant::now();
    let down = gated_post(child, meta, KEY_H as u16, want, true)
        .inspect_err(|_| factory.set_direct_hold(false))?;
    std::thread::sleep(Duration::from_millis(80));
    let up = gated_post(child, meta, KEY_H as u16, want, false)
        .inspect_err(|_| factory.set_direct_hold(false))?;
    // Extend the hold to the requested span (edges alone are ~80ms), then
    // release: the disabled window stays narrow by construction.
    let target = Duration::from_millis(hold_target_ms.max(1));
    let elapsed = hold_start.elapsed();
    if elapsed < target {
        std::thread::sleep(target - elapsed);
    }
    factory.set_direct_hold(false);
    let hold_ms = hold_start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
    let recovered = await_direct_enabled(factory, Some(true), "direct-recovery")
        .map_err(|cause| format!("{NAME}: {cause}"))?;
    // Owner-liveness gate BEFORE the absence proof: a refusal here means
    // the OS answered the DIRECT cycle with a user-disable (fail with the
    // refusal, never a bare disconnect later); a disconnected channel
    // means the owner is already gone.
    if let Some(refusal) = factory.refusal() {
        return Err(format!(
            "{NAME}: owner refused after DIRECT cycle ({refusal})"
        ));
    }
    match hits.try_recv() {
        Ok(hit) => {
            return Err(format!(
                "{NAME}: unexpected owner hit right after DIRECT cycle (index {} down={})",
                hit.index, hit.down
            ));
        }
        Err(mpsc::TryRecvError::Disconnected) => {
            return Err(format!(
                "{NAME}: owner gone after DIRECT cycle (refusal={:?})",
                factory.refusal()
            ));
        }
        Err(mpsc::TryRecvError::Empty) => {}
    }
    // Absence proof with liveness: no hit may arrive (the tap never saw the
    // chord); a disconnect mid-window fails with the refusal attached.
    let absence_deadline = Instant::now() + ABSENCE_WAIT;
    while Instant::now() < absence_deadline {
        pump_main();
        match hits.try_recv() {
            Ok(hit) => {
                return Err(format!(
                    "{NAME}: unexpected owner hit during absence window (index {})",
                    hit.index
                ));
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                return Err(format!(
                    "{NAME}: owner exited during absence window (refusal={:?})",
                    factory.refusal()
                ));
            }
            Err(mpsc::TryRecvError::Empty) => {}
        }
    }
    let after = poll_helper_counts(lines, Duration::from_millis(1200))
        .ok_or_else(|| format!("{NAME}: no helper key-count snapshot after post"))?;
    let want_bucket = bucket_index(meta.want_flags);
    let extra_bucket = bucket_index(meta.want_flags | extra_flag_for(meta.modifier));
    let plain_bucket = bucket_index(0);
    let mut leg = PostLeg {
        name: NAME.to_owned(),
        key_code: KEY_H,
        flags: want,
        down_post_us: down.post_latency_us,
        up_post_us: up.post_latency_us,
        owner_latency_ms: None,
        helper_exact_delta: (
            delta(before.h_down[want_bucket], after.h_down[want_bucket]),
            delta(before.h_up[want_bucket], after.h_up[want_bucket]),
        ),
        helper_extra_delta: (
            delta(before.h_down[extra_bucket], after.h_down[extra_bucket]),
            delta(before.h_up[extra_bucket], after.h_up[extra_bucket]),
        ),
        helper_plain_delta: (
            delta(before.h_down[plain_bucket], after.h_down[plain_bucket]),
            delta(before.h_up[plain_bucket], after.h_up[plain_bucket]),
        ),
        direct_enabled_before: Some(disabled),
        direct_enabled_after: Some(recovered),
        verdict: "posted".to_owned(),
    };
    if leg.helper_exact_delta != (1, 1) {
        return Err(format!(
            "{NAME}: DIRECT passthrough mismatch (want exact +1/+1, have {:?}; recovered direct=={recovered})",
            leg.helper_exact_delta
        ));
    }
    if leg.helper_extra_delta != (0, 0) {
        return Err(format!(
            "{NAME}: DIRECT disabled leg touched the extra bucket (extra {:?})",
            leg.helper_extra_delta
        ));
    }
    let _ = t_down;
    leg.verdict = format!(
        "DIRECT-disabled-passthrough helper +1/+1 direct==false->true hold_ms={hold_ms} target_ms={hold_target_ms}"
    );
    Ok(leg)
}

/// DIRECT disabled-post leg for the EXPLICIT re-arm path (tap +
/// `--direct-rearm` only): like `post_direct_disabled_leg` except the hold is
/// LEFT SET on success — the old guard stays directly OS-disabled so the next
/// step drops it WITHOUT a same-tap re-enable. Steps: snapshot before (tap
/// enabled), hold, await the actual `==false` readback, oracle-gated down/up
/// edges, extend the hold to the requested span, then the absence proof with
/// liveness plus the after snapshot prove the OLD tap never saw the chord (no
/// hit) while the helper proves passthrough (exact +1/+1). The caller owns
/// the EXPLICIT re-arm next; this function never clears the hold itself
/// (callers clear via a fresh source epoch, never via the old tap).
#[allow(clippy::too_many_arguments)]
pub(crate) fn post_disabled_passthrough_held(
    child: &mut std::process::Child,
    meta: &CtxMeta,
    lines: &mpsc::Receiver<String>,
    hits: &mpsc::Receiver<OwnerHit>,
    factory: &TapTileFactory,
    want: u64,
    hold_target_ms: u64,
) -> Result<PostLeg, String> {
    const NAME: &str = "direct-rearm-disabled-exact";
    while hits.try_recv().is_ok() {}
    let before = poll_helper_counts(lines, Duration::from_millis(1200))
        .ok_or_else(|| format!("{NAME}: no helper key-count snapshot before post"))?;
    let hold_start = Instant::now();
    factory.set_direct_hold(true);
    let disabled = await_direct_enabled(factory, Some(false), "direct-rearm-disable")
        .inspect_err(|_| factory.set_direct_hold(false))?;
    let down = gated_post(child, meta, KEY_H as u16, want, true)
        .inspect_err(|_| factory.set_direct_hold(false))?;
    std::thread::sleep(Duration::from_millis(80));
    let up = gated_post(child, meta, KEY_H as u16, want, false)
        .inspect_err(|_| factory.set_direct_hold(false))?;
    let target = Duration::from_millis(hold_target_ms.max(1));
    let elapsed = hold_start.elapsed();
    if elapsed < target {
        std::thread::sleep(target - elapsed);
    }
    let hold_ms = hold_start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
    // Owner-liveness gate BEFORE the absence proof: a refusal here means the
    // OS answered the DIRECT disable with a user-disable (fail with the
    // refusal, clear the hold, never a bare disconnect later); a disconnected
    // channel means the owner is already gone.
    if let Some(refusal) = factory.refusal() {
        factory.set_direct_hold(false);
        return Err(format!(
            "{NAME}: owner refused during DIRECT hold ({refusal})"
        ));
    }
    match hits.try_recv() {
        Ok(hit) => {
            factory.set_direct_hold(false);
            return Err(format!(
                "{NAME}: unexpected owner hit while DIRECTLY disabled (index {} down={})",
                hit.index, hit.down
            ));
        }
        Err(mpsc::TryRecvError::Disconnected) => {
            factory.set_direct_hold(false);
            return Err(format!(
                "{NAME}: owner gone while DIRECTLY disabled (refusal={:?})",
                factory.refusal()
            ));
        }
        Err(mpsc::TryRecvError::Empty) => {}
    }
    let absence_deadline = Instant::now() + ABSENCE_WAIT;
    while Instant::now() < absence_deadline {
        pump_main();
        match hits.try_recv() {
            Ok(hit) => {
                factory.set_direct_hold(false);
                return Err(format!(
                    "{NAME}: unexpected owner hit during absence window (index {})",
                    hit.index
                ));
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                factory.set_direct_hold(false);
                return Err(format!(
                    "{NAME}: owner exited during absence window (refusal={:?})",
                    factory.refusal()
                ));
            }
            Err(mpsc::TryRecvError::Empty) => {}
        }
    }
    let after = poll_helper_counts(lines, Duration::from_millis(1200))
        .ok_or_else(|| format!("{NAME}: no helper key-count snapshot after post"))?;
    let want_bucket = bucket_index(meta.want_flags);
    let extra_bucket = bucket_index(meta.want_flags | extra_flag_for(meta.modifier));
    let plain_bucket = bucket_index(0);
    let mut leg = PostLeg {
        name: NAME.to_owned(),
        key_code: KEY_H,
        flags: want,
        down_post_us: down.post_latency_us,
        up_post_us: up.post_latency_us,
        owner_latency_ms: None,
        helper_exact_delta: (
            delta(before.h_down[want_bucket], after.h_down[want_bucket]),
            delta(before.h_up[want_bucket], after.h_up[want_bucket]),
        ),
        helper_extra_delta: (
            delta(before.h_down[extra_bucket], after.h_down[extra_bucket]),
            delta(before.h_up[extra_bucket], after.h_up[extra_bucket]),
        ),
        helper_plain_delta: (
            delta(before.h_down[plain_bucket], after.h_down[plain_bucket]),
            delta(before.h_up[plain_bucket], after.h_up[plain_bucket]),
        ),
        direct_enabled_before: Some(disabled),
        direct_enabled_after: factory.latest_direct_enabled(),
        verdict: "posted".to_owned(),
    };
    if leg.helper_exact_delta != (1, 1) {
        factory.set_direct_hold(false);
        return Err(format!(
            "{NAME}: DIRECT passthrough mismatch (want exact +1/+1, have {:?}; direct=={:?}, hold still set until re-arm)",
            leg.helper_exact_delta, leg.direct_enabled_after
        ));
    }
    if leg.helper_extra_delta != (0, 0) {
        factory.set_direct_hold(false);
        return Err(format!(
            "{NAME}: DIRECT disabled leg touched the extra bucket (extra {:?})",
            leg.helper_extra_delta
        ));
    }
    // Hold stays SET: the EXPLICIT re-arm drops this disabled guard next.
    leg.verdict = format!(
        "DIRECT-REARM-disabled-passthrough-held helper +1/+1 direct==false hold_ms={hold_ms} target_ms={hold_target_ms}"
    );
    Ok(leg)
}

/// Bounded wait for the EXPLICIT re-arm epoch bump published by the owner
/// thread. Returns the new epoch once it exceeds `past`, or a truthful
/// timeout error. A recorded user-disable refusal fails closed here (never a
/// silent hold). Pumps the controller main while waiting.
pub(crate) fn await_rearm_epoch(
    factory: &TapTileFactory,
    past: u64,
    name: &str,
) -> Result<u64, String> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(refusal) = factory.refusal() {
            return Err(format!("{name}: owner refused during re-arm ({refusal})"));
        }
        let epoch = factory.rearm_epoch();
        if epoch > past {
            return Ok(epoch);
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "{name}: re-arm epoch never advanced past {past} (have {epoch}, refusal={:?})",
                factory.refusal()
            ));
        }
        pump_main();
    }
}

pub(crate) fn await_owner_hit(
    hits: &mpsc::Receiver<OwnerHit>,
    wait: Duration,
) -> Result<OwnerHit, String> {
    let deadline = Instant::now() + wait;
    loop {
        match hits.try_recv() {
            Ok(hit) => return Ok(hit),
            Err(mpsc::TryRecvError::Disconnected) => {
                return Err("owner channel disconnected (unexpected EOS)".to_owned());
            }
            Err(mpsc::TryRecvError::Empty) => {}
        }
        if Instant::now() >= deadline {
            return Err("timed out waiting for the owner hit (3s)".to_owned());
        }
        pump_main();
    }
}

pub(crate) fn poll_helper_counts(
    lines: &mpsc::Receiver<String>,
    wait: Duration,
) -> Option<crate::ffi::KeyCounts> {
    let deadline = Instant::now() + wait;
    let mut best = latest_helper_counts(lines);
    while Instant::now() < deadline {
        pump_main();
        if let Some(fresh) = latest_helper_counts(lines) {
            best = Some(fresh);
        }
    }
    best
}

/// The post legs per route (see module docs). Pushes one [`PostLeg`] per
/// chord and enforces each leg's delivery/absence expectation; any mismatch
/// fails with `(stage, cause)`. Arrow chords: carbon observes registration
/// statuses only (never posted); tap posts matched Left/Right only after
/// the `h` proof on a single-Space baseline with a Space readback (Up/Down
/// never posted).
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_post_legs(
    child: &mut std::process::Child,
    meta: &CtxMeta,
    lines: &mpsc::Receiver<String>,
    hits: &mpsc::Receiver<OwnerHit>,
    request: InputSessionRequest,
    space_before: &SpaceSnapshot,
    single_space_baseline: bool,
    legs: &mut Vec<PostLeg>,
    tap_factory: Option<&TapTileFactory>,
) -> Result<(), (String, String)> {
    let fail = |stage: &str, cause: String| -> Result<(), (String, String)> {
        Err((stage.to_owned(), cause))
    };
    let (want, _) = cg_flag_bits(request.modifier);
    let extra = want | extra_flag_for(request.modifier);
    let h_index = tile_catalogue()
        .iter()
        .position(|b| b.key_code == KEY_H && !b.shift)
        .ok_or_else(|| {
            (
                "catalogue".to_owned(),
                "focus Ctrl+h missing from tile catalogue".to_owned(),
            )
        })?;
    let tap_counts = || {
        tap_factory
            .map(|f| f.latest_report())
            .unwrap_or(TapSourceReport {
                matched: 0,
                suppressed: 0,
                deliveries: 0,
                reenables_after_timeout: 0,
            })
    };

    // Exact chord: carbon expects the owner hit plus helper delivery; tap
    // expects the owner match plus helper ABSENCE (suppression proof over
    // the bounded hold, never assumed); stall expects the stalled down
    // suppressed under burst load behind the sleeping callback (see
    // `post_stall_leg`).
    let tap_before = tap_counts();
    let mut exact = if request.stall_once_ms > 0 {
        post_stall_leg(child, meta, lines, hits, tap_factory, h_index, want, 6)
            .map_err(|e| ("post-exact".to_owned(), e))?
    } else {
        post_chord_leg(
            child,
            meta,
            lines,
            hits,
            "exact",
            KEY_H,
            want,
            Some(h_index),
            request.route == InputSessionRoute::Tap,
        )
        .map_err(|e| ("post-exact".to_owned(), e))?
    };
    if request.route == InputSessionRoute::Carbon {
        if exact.helper_exact_delta != (1, 1) {
            return fail(
                "post-exact",
                format!(
                    "carbon exact delivery mismatch (want exact +1/+1, have {:?})",
                    exact.helper_exact_delta
                ),
            );
        }
        exact.verdict = format!(
            "carbon-hit latency_ms={} helper +1/+1",
            exact.owner_latency_ms.unwrap_or(u64::MAX)
        );
    } else if request.stall_once_ms > 0 {
        // The stall leg carries its own verdict (down suppressed under
        // burst load, bypass arrivals recorded); nothing further to assert
        // here — the disable stage below decides from the re-enable
        // counter, and the recovery leg re-proves suppression after it.
    } else {
        if exact.helper_exact_delta != (0, 0) {
            return fail(
                "post-exact",
                format!(
                    "tap suppression FAILED (helper exact changed {:?}, want absence)",
                    exact.helper_exact_delta
                ),
            );
        }
        let after = tap_counts();
        if after.matched.saturating_sub(tap_before.matched) < 2
            || after.suppressed.saturating_sub(tap_before.suppressed) < 2
        {
            return fail(
                "post-exact",
                format!(
                    "tap counters too low after exact chord (matched +{}, suppressed +{})",
                    after.matched.saturating_sub(tap_before.matched),
                    after.suppressed.saturating_sub(tap_before.suppressed)
                ),
            );
        }
        exact.verdict = format!(
            "suppressed latency_ms={} tap matched+{} suppressed+{}",
            exact.owner_latency_ms.unwrap_or(u64::MAX),
            after.matched.saturating_sub(tap_before.matched),
            after.suppressed.saturating_sub(tap_before.suppressed)
        );
    }
    legs.push(exact);

    // Fault-injection leg (tap + stall only): the exact post above already
    // consumed the one-shot stall, so the OS MUST have disabled the tap by
    // timeout and the pump MUST have re-enabled (counted). A zero counter
    // fails truthfully: no manual callback ever stands in for the OS.
    if request.stall_once_ms > 0 {
        let report = tap_counts();
        if report.reenables_after_timeout < 1 {
            return fail(
                "stall-disable",
                format!(
                    "no REAL OS timeout-disable observed after {}ms callback stall (reenables={}); refusing to claim recovery",
                    request.stall_once_ms, report.reenables_after_timeout
                ),
            );
        }
        // Recovery follow-up: exact chord again — matched AND suppressed
        // (interception holds after the re-enable), helper still absent.
        let tap_before = tap_counts();
        let mut recovery = post_chord_leg(
            child,
            meta,
            lines,
            hits,
            "stall-recovery",
            KEY_H,
            want,
            Some(h_index),
            true,
        )
        .map_err(|e| ("stall-recovery".to_owned(), e))?;
        if recovery.helper_exact_delta != (0, 0) {
            return fail(
                "stall-recovery",
                format!(
                    "post-recovery suppression lost (helper exact {:?})",
                    recovery.helper_exact_delta
                ),
            );
        }
        let after = tap_counts();
        recovery.verdict = format!(
            "recovered reenables={} tap matched+{} suppressed+{} latency_ms={}",
            report.reenables_after_timeout,
            after.matched.saturating_sub(tap_before.matched),
            after.suppressed.saturating_sub(tap_before.suppressed),
            recovery.owner_latency_ms.unwrap_or(u64::MAX)
        );
        legs.push(recovery);
    }

    // DIRECT disable/recovery leg (tap + --direct-disable-hold-ms only,
    // never combined with the stall leg above): the owner parks the tap
    // DIRECTLY OS-disabled for ONLY the post edges (about the requested
    // hold milliseconds) while the main posts the exact chord; snapshots
    // and the absence proof run outside the hold so a long direct-disable
    // never invites the OS user-disable response (measured live: a ~4s
    // hold drew `TapDisabledByUserInput` and killed the owner). The
    // disable readback (`CGEventTapIsEnabled==false`), the helper
    // passthrough (+1/+1 in the exact bucket), the recovery readback
    // (`==true`), and the re-proved suppression after it are all gated
    // here — labeled DIRECT throughout, never the OS-timeout path.
    if request.direct_disable_hold_ms > 0 {
        let factory = tap_factory.ok_or_else(|| {
            (
                "direct-disable".to_owned(),
                "DIRECT hold needs the tap factory".to_owned(),
            )
        })?;
        // Baseline: the tap proved suppression above, so the OS state must
        // read enabled before the hold (bounded wait for the owner's first
        // published readback; unknown refuses rather than assuming).
        let baseline = await_direct_enabled(factory, Some(true), "direct-baseline")
            .map_err(|cause| ("direct-baseline".to_owned(), cause))?;
        // Narrow disabled post: hold covers the edges only (see
        // `post_direct_disabled_leg`); the leg carries its own verdict.
        let passthrough = post_direct_disabled_leg(
            child,
            meta,
            lines,
            hits,
            factory,
            want,
            request.direct_disable_hold_ms,
        )
        .map_err(|e| ("direct-disabled-exact".to_owned(), e))?;
        if passthrough.direct_enabled_before != Some(false) {
            return fail(
                "direct-disabled-exact",
                format!(
                    "DIRECT disable readback lost (before={:?})",
                    passthrough.direct_enabled_before
                ),
            );
        }
        let _ = baseline;
        legs.push(passthrough);
        // Post-recovery the exact chord must suppress again (owner match
        // plus helper absence), proving interception holds after the
        // DIRECT re-enable.
        let tap_before_recovery = tap_counts();
        let mut recovery = post_chord_leg(
            child,
            meta,
            lines,
            hits,
            "direct-recovery-exact",
            KEY_H,
            want,
            Some(h_index),
            true,
        )
        .map_err(|e| ("direct-recovery-exact".to_owned(), e))?;
        recovery.direct_enabled_before = Some(true);
        recovery.direct_enabled_after = factory.latest_direct_enabled();
        if recovery.helper_exact_delta != (0, 0) {
            return fail(
                "direct-recovery-exact",
                format!(
                    "DIRECT post-recovery suppression lost (helper exact {:?})",
                    recovery.helper_exact_delta
                ),
            );
        }
        let after = tap_counts();
        if after.matched.saturating_sub(tap_before_recovery.matched) < 2
            || after
                .suppressed
                .saturating_sub(tap_before_recovery.suppressed)
                < 2
        {
            return fail(
                "direct-recovery-exact",
                format!(
                    "DIRECT recovery tap counters too low (matched +{}, suppressed +{})",
                    after.matched.saturating_sub(tap_before_recovery.matched),
                    after
                        .suppressed
                        .saturating_sub(tap_before_recovery.suppressed)
                ),
            );
        }
        recovery.verdict = format!(
            "DIRECT-recovered-suppressed direct==true tap matched+{} suppressed+{} latency_ms={}",
            after.matched.saturating_sub(tap_before_recovery.matched),
            after
                .suppressed
                .saturating_sub(tap_before_recovery.suppressed),
            recovery.owner_latency_ms.unwrap_or(u64::MAX)
        );
        legs.push(recovery);
    }

    // EXPLICIT re-arm leg (tap + `--direct-rearm` only, never combined with
    // the stall or hold legs above): after the exact-suppression proof above,
    // the owner parks the OLD tap DIRECTLY OS-disabled for ONLY the post
    // edges (narrow 300ms hold) while the main posts the exact chord; the
    // held snapshots and absence proof run with the hold still set so the old
    // guard never sees a same-tap re-enable. The owner then drops the old
    // disabled guard and installs a FRESH `TapSource` over the same chords
    // and tap placement on the SAME owner thread (epoch bump plus fresh
    // `==true` readback), and the exact chord must suppress again on the new
    // generation (owner match plus helper absence plus fresh tap counters).
    // A genuine user-disable refusal fails closed at any step (never masked,
    // never auto re-enabled). Labeled DIRECT-REARM throughout.
    if request.direct_rearm {
        const REARM_HOLD_MS: u64 = 300;
        let factory = tap_factory.ok_or_else(|| {
            (
                "direct-rearm".to_owned(),
                "DIRECT-REARM needs the tap factory".to_owned(),
            )
        })?;
        let baseline = await_direct_enabled(factory, Some(true), "direct-rearm-baseline")
            .map_err(|cause| ("direct-rearm-baseline".to_owned(), cause))?;
        let _ = baseline;
        let epoch_before = factory.rearm_epoch();
        if epoch_before != 0 {
            return fail(
                "direct-rearm",
                format!("re-arm epoch not fresh at entry (have {epoch_before}, want 0)"),
            );
        }
        // Narrow disabled post on the OLD tap; the hold stays SET.
        let held =
            post_disabled_passthrough_held(child, meta, lines, hits, factory, want, REARM_HOLD_MS)
                .map_err(|e| ("direct-rearm-disabled-exact".to_owned(), e))?;
        if held.direct_enabled_before != Some(false) {
            factory.set_direct_hold(false);
            return fail(
                "direct-rearm-disabled-exact",
                format!(
                    "DIRECT-REARM disable readback lost (before={:?})",
                    held.direct_enabled_before
                ),
            );
        }
        legs.push(held);
        // EXPLICIT re-arm: drop the old disabled guard, install a FRESH
        // source on the SAME owner thread (same chords, same placement).
        // Exactly one semantic re-arm per run; the owner clears the hold as
        // part of the epoch bump, never via the old tap.
        factory.request_rearm();
        let epoch = await_rearm_epoch(factory, epoch_before, "direct-rearm-epoch")
            .map_err(|cause| ("direct-rearm-epoch".to_owned(), cause))?;
        if epoch != epoch_before + 1 {
            return fail(
                "direct-rearm-epoch",
                format!(
                    "re-arm epoch jump (have {epoch}, want {})",
                    epoch_before + 1
                ),
            );
        }
        let recovered = await_direct_enabled(factory, Some(true), "direct-rearm-recovery")
            .map_err(|cause| ("direct-rearm-recovery".to_owned(), cause))?;
        if !recovered {
            return fail(
                "direct-rearm-recovery",
                "fresh tap did not read enabled after EXPLICIT re-arm".to_owned(),
            );
        }
        if let Some(refusal) = factory.refusal() {
            return fail(
                "direct-rearm-recovery",
                format!("owner refused after EXPLICIT re-arm ({refusal})"),
            );
        }
        // New-generation suppression on the FRESH tap: owner match plus
        // helper absence plus fresh tap counters rising.
        let tap_before_recovery = tap_counts();
        let mut recovery = post_chord_leg(
            child,
            meta,
            lines,
            hits,
            "direct-rearm-recovered-exact",
            KEY_H,
            want,
            Some(h_index),
            true,
        )
        .map_err(|e| ("direct-rearm-recovered-exact".to_owned(), e))?;
        recovery.direct_enabled_before = Some(true);
        recovery.direct_enabled_after = factory.latest_direct_enabled();
        if recovery.helper_exact_delta != (0, 0) {
            return fail(
                "direct-rearm-recovered-exact",
                format!(
                    "DIRECT-REARM post-recovery suppression lost (helper exact {:?})",
                    recovery.helper_exact_delta
                ),
            );
        }
        let after = tap_counts();
        // Fresh-source counters restart at the new guard: the deltas below
        // are the new generation's own proof (never carried from the old
        // guard). A user-disable would have refused above instead.
        if after.matched.saturating_sub(tap_before_recovery.matched) < 2
            || after
                .suppressed
                .saturating_sub(tap_before_recovery.suppressed)
                < 2
        {
            return fail(
                "direct-rearm-recovered-exact",
                format!(
                    "DIRECT-REARM recovery tap counters too low (matched +{}, suppressed +{})",
                    after.matched.saturating_sub(tap_before_recovery.matched),
                    after
                        .suppressed
                        .saturating_sub(tap_before_recovery.suppressed)
                ),
            );
        }
        recovery.verdict = format!(
            "DIRECT-REARM-recovered-suppressed epoch={epoch} direct==true tap matched+{} suppressed+{} latency_ms={}",
            after.matched.saturating_sub(tap_before_recovery.matched),
            after
                .suppressed
                .saturating_sub(tap_before_recovery.suppressed),
            recovery.owner_latency_ms.unwrap_or(u64::MAX)
        );
        legs.push(recovery);
    }

    // Extra-modifier variant: no owner hit on either route, helper
    // passthrough (+1/+1 in the extra bucket, exact bucket untouched).
    let mut extra_leg = post_chord_leg(
        child,
        meta,
        lines,
        hits,
        "extra-modifier",
        KEY_H,
        extra,
        None,
        false,
    )
    .map_err(|e| ("post-extra".to_owned(), e))?;
    if extra_leg.helper_extra_delta != (1, 1) || extra_leg.helper_exact_delta != (0, 0) {
        return fail(
            "post-extra",
            format!(
                "extra-modifier passthrough mismatch (extra {:?}, exact {:?})",
                extra_leg.helper_extra_delta, extra_leg.helper_exact_delta
            ),
        );
    }
    extra_leg.verdict = "passed-through".to_owned();
    legs.push(extra_leg);

    // Plain variant: no owner hit, helper plain bucket +1/+1.
    let mut plain = post_chord_leg(child, meta, lines, hits, "plain", KEY_H, 0, None, false)
        .map_err(|e| ("post-plain".to_owned(), e))?;
    if plain.helper_plain_delta != (1, 1) {
        return fail(
            "post-plain",
            format!(
                "plain passthrough mismatch (plain {:?})",
                plain.helper_plain_delta
            ),
        );
    }
    plain.verdict = "passed-through".to_owned();
    legs.push(plain);

    // Arrow observations. Carbon: registration statuses only (Mission
    // Control chords are never posted and no settings are written). Tap:
    // matched Left/Right suppression only after the `h` proof above AND a
    // single-Space baseline plus a Space readback proving no Space moved
    // (a suppressed event never dispatches; Up/Down are never posted — no
    // user-owned overview restore path exists).
    if request.route == InputSessionRoute::Tap && single_space_baseline {
        for (name, code) in [("arrow-left", 0x7Bu32), ("arrow-right", 0x7Cu32)] {
            let index = tile_catalogue()
                .iter()
                .position(|b| b.key_code == code && !b.shift)
                .ok_or_else(|| {
                    (
                        "catalogue".to_owned(),
                        format!("{name} missing from tile catalogue"),
                    )
                })?;
            let tap_before = tap_counts();
            let mut leg = post_chord_leg(
                child,
                meta,
                lines,
                hits,
                name,
                code,
                want,
                Some(index),
                true,
            )
            .map_err(|e| ("post-arrow".to_owned(), e))?;
            let after = tap_counts();
            if after.matched.saturating_sub(tap_before.matched) < 2
                || after.suppressed.saturating_sub(tap_before.suppressed) < 2
            {
                return fail(
                    "post-arrow",
                    format!(
                        "{name} tap suppression unproven (matched/suppressed did not rise by 2)"
                    ),
                );
            }
            let space_now =
                crate::focus::capture_space().map_err(|e| ("post-arrow".to_owned(), e))?;
            if space_now != *space_before {
                return fail(
                    "post-arrow",
                    format!("{name} moved Spaces; refusing (baseline violated)"),
                );
            }
            leg.verdict = "suppressed".to_owned();
            legs.push(leg);
        }
    }
    Ok(())
}

/// Arrow observation rows: every catalogue arrow chord with its OS
/// registration status when known (carbon source or the transient tap-leg
/// probe) plus the tap suppression note from the legs above.
pub(crate) fn arrow_observations(
    request: InputSessionRequest,
    carbon_statuses: &[CarbonChordStatus],
    legs: &[PostLeg],
) -> Vec<ArrowStatus> {
    let mut out = Vec::new();
    for binding in tile_catalogue() {
        if !matches!(binding.key_code, 0x7B..=0x7E) {
            continue;
        }
        let carbon_os_status = carbon_statuses
            .iter()
            .find(|s| {
                s.key_code == binding.key_code
                    && s.carbon_modifiers
                        == crate::input::carbon_mod_bits(request.modifier)
                            + if binding.shift {
                                crate::native_input::CARBON_SHIFT_BIT
                            } else {
                                0
                            }
            })
            .map(|s| s.os_status);
        let tap_observed = legs
            .iter()
            .find(|leg| {
                leg.key_code == binding.key_code
                    && (leg.name == "arrow-left" || leg.name == "arrow-right")
            })
            .map(|leg| leg.verdict.clone());
        out.push(ArrowStatus {
            key_code: binding.key_code,
            shift: binding.shift,
            carbon_os_status,
            tap_observed,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{MAX_PROBE_SECONDS, ProbeModifier};
    use crate::input::cg_flag_bits;
    use crate::input_match::{TapKeyKind, TapTuning, decide_tap_event};
    use crate::input_session::{
        CG_SHIFT_BIT, DEFAULT_INPUT_SECONDS, InputSessionRequest, InputSessionRoute, KEY_H,
        MAX_DIRECT_HOLD_MS, MAX_STALL_ONCE_MS, MAX_WATCHDOG_ONLY_SECONDS, PostRoute, bucket_index,
        extra_flag_for, tuning_for,
    };
    use crate::native_input::tile_catalogue;

    fn request(route: InputSessionRoute) -> InputSessionRequest {
        InputSessionRequest {
            seconds: DEFAULT_INPUT_SECONDS,
            route,
            post_route: PostRoute::Session,
            stall_once_ms: 0,
            direct_disable_hold_ms: 0,
            direct_rearm: false,
            watchdog_only: false,
            modifier: ProbeModifier::Ctrl,
        }
    }

    #[test]
    fn route_parses_exactly() {
        assert_eq!(
            InputSessionRoute::parse("carbon"),
            Ok(InputSessionRoute::Carbon)
        );
        assert_eq!(InputSessionRoute::parse("tap"), Ok(InputSessionRoute::Tap));
        assert!(InputSessionRoute::parse("auto").is_err());
        assert!(InputSessionRoute::parse("").is_err());
    }

    #[test]
    fn post_route_parses_and_defaults_to_session() {
        assert_eq!(PostRoute::parse("session"), Ok(PostRoute::Session));
        assert_eq!(PostRoute::parse("hid"), Ok(PostRoute::Hid));
        assert!(PostRoute::parse("auto").is_err());
        assert_eq!(
            request(InputSessionRoute::Carbon).post_route,
            PostRoute::Session
        );
        // The HID producer is orthogonal to the input source: allowed on
        // both routes, so the Carbon HID leg and the tap stall
        // discriminator both validate.
        let mut r = request(InputSessionRoute::Carbon);
        r.post_route = PostRoute::Hid;
        assert!(r.validate().is_ok());
        let mut r = request(InputSessionRoute::Tap);
        r.post_route = PostRoute::Hid;
        r.stall_once_ms = 2000;
        assert!(r.validate().is_ok());
    }

    #[test]
    fn validation_bounds_lifetime_and_stall() {
        assert!(request(InputSessionRoute::Carbon).validate().is_ok());
        let mut r = request(InputSessionRoute::Carbon);
        r.seconds = 0;
        assert!(r.validate().is_err());
        r.seconds = MAX_PROBE_SECONDS + 1;
        assert!(r.validate().is_err());
        // Stall is tap-only and bounded.
        let mut r = request(InputSessionRoute::Carbon);
        r.stall_once_ms = 500;
        assert!(r.validate().is_err());
        let mut r = request(InputSessionRoute::Tap);
        r.stall_once_ms = MAX_STALL_ONCE_MS + 1;
        assert!(r.validate().is_err());
        let mut r = request(InputSessionRoute::Tap);
        r.stall_once_ms = 1200;
        assert!(r.validate().is_ok());
    }

    #[test]
    fn validation_bounds_direct_hold_and_watchdog_only() {
        // DIRECT hold is tap-only and bounded, never combined with stall.
        let mut r = request(InputSessionRoute::Carbon);
        r.direct_disable_hold_ms = 300;
        assert!(r.validate().is_err());
        let mut r = request(InputSessionRoute::Tap);
        r.direct_disable_hold_ms = MAX_DIRECT_HOLD_MS + 1;
        assert!(r.validate().is_err());
        let mut r = request(InputSessionRoute::Tap);
        r.direct_disable_hold_ms = 300;
        assert!(r.validate().is_ok());
        let mut r = request(InputSessionRoute::Tap);
        r.direct_disable_hold_ms = 300;
        r.stall_once_ms = 500;
        assert!(r.validate().is_err());
        // Watchdog-only is tap-only, 2..=5s, and takes no other fault flag.
        let mut r = request(InputSessionRoute::Carbon);
        r.watchdog_only = true;
        r.seconds = 4;
        assert!(r.validate().is_err());
        let mut r = request(InputSessionRoute::Tap);
        r.watchdog_only = true;
        r.seconds = 1;
        assert!(r.validate().is_err());
        let mut r = request(InputSessionRoute::Tap);
        r.watchdog_only = true;
        r.seconds = MAX_WATCHDOG_ONLY_SECONDS + 1;
        assert!(r.validate().is_err());
        let mut r = request(InputSessionRoute::Tap);
        r.watchdog_only = true;
        r.seconds = 4;
        assert!(r.validate().is_ok());
        let mut r = request(InputSessionRoute::Tap);
        r.watchdog_only = true;
        r.seconds = 4;
        r.stall_once_ms = 500;
        assert!(r.validate().is_err());
        let mut r = request(InputSessionRoute::Tap);
        r.watchdog_only = true;
        r.seconds = 4;
        r.direct_disable_hold_ms = 300;
        assert!(r.validate().is_err());
        let mut r = request(InputSessionRoute::Tap);
        r.watchdog_only = true;
        r.seconds = 4;
        r.direct_rearm = true;
        assert!(r.validate().is_err());
    }

    #[test]
    fn validation_bounds_direct_rearm() {
        // DIRECT-REARM is tap-only and never combined with the other fault
        // legs (stall, hold, watchdog-only).
        let mut r = request(InputSessionRoute::Carbon);
        r.direct_rearm = true;
        assert!(r.validate().is_err());
        let mut r = request(InputSessionRoute::Tap);
        r.direct_rearm = true;
        assert!(r.validate().is_ok());
        let mut r = request(InputSessionRoute::Tap);
        r.direct_rearm = true;
        r.stall_once_ms = 500;
        assert!(r.validate().is_err());
        let mut r = request(InputSessionRoute::Tap);
        r.direct_rearm = true;
        r.direct_disable_hold_ms = 300;
        assert!(r.validate().is_err());
    }

    #[test]
    fn bucket_index_folds_the_modifier_nibble() {
        // Exact Ctrl (0x40000) -> bucket 2; Ctrl+Shift -> bucket 3; plain
        // -> bucket 0. Caps/Fn sit outside the mask and never move buckets.
        assert_eq!(bucket_index(0x0004_0000), 2);
        assert_eq!(bucket_index(0x0006_0000), 3);
        assert_eq!(bucket_index(0), 0);
        assert_eq!(bucket_index(0x0004_0000 | 0x0001_0000), 2);
        assert_eq!(bucket_index(0x0004_0000 | 0x0080_0000), 2);
    }

    #[test]
    fn extra_flag_breaks_exact_but_stays_masked() {
        // The extra bit fails EVERY catalogue chord (focus and Shift move
        // arms alike) for every Ctrl role, and sits inside the mask (so it
        // is an *extra*, never ignored noise).
        for modifier in [
            ProbeModifier::Meta,
            ProbeModifier::Ctrl,
            ProbeModifier::Alt,
            ProbeModifier::Shift,
        ] {
            let (want, mask) = cg_flag_bits(modifier);
            let extra = extra_flag_for(modifier);
            assert_eq!(extra & mask, extra);
            assert_eq!(extra & want, 0);
            for binding in tile_catalogue() {
                let tuning = tuning_for(modifier, binding.key_code, binding.shift);
                let mut pending = false;
                assert!(
                    !decide_tap_event(
                        TapKeyKind::KeyDown,
                        want | extra,
                        i64::from(binding.key_code),
                        tuning,
                        true,
                        &mut pending,
                    )
                    .matched,
                    "{modifier:?} extra must fail chord {} shift={}",
                    binding.key_code,
                    binding.shift
                );
            }
            // ...while the bare chord still matches its own arm.
            let tuning = tuning_for(modifier, KEY_H, false);
            let mut pending = false;
            assert!(
                decide_tap_event(
                    TapKeyKind::KeyDown,
                    want,
                    i64::from(KEY_H),
                    tuning,
                    true,
                    &mut pending,
                )
                .matched
            );
        }
    }

    #[test]
    fn owner_tuning_mirror_matches_catalogue_exactly() {
        // The owner-side mirror keys the full (code, shift-arm) pair like
        // the production source: focus and move arms never alias.
        let focus = tuning_for(ProbeModifier::Ctrl, KEY_H, false);
        let moved = tuning_for(ProbeModifier::Ctrl, KEY_H, true);
        let (want, _) = cg_flag_bits(ProbeModifier::Ctrl);
        let matched = |flags: u64, tuning: TapTuning| {
            let mut pending = false;
            decide_tap_event(
                TapKeyKind::KeyDown,
                flags,
                i64::from(KEY_H),
                tuning,
                false,
                &mut pending,
            )
            .matched
        };
        assert!(matched(want, focus));
        assert!(!matched(want, moved));
        assert!(matched(want | CG_SHIFT_BIT, moved));
        assert!(!matched(want | CG_SHIFT_BIT, focus));
    }

    #[test]
    fn h_index_resolves_to_the_focus_arm() {
        let catalogue = tile_catalogue();
        let index = catalogue
            .iter()
            .position(|b| b.key_code == KEY_H && !b.shift)
            .expect("focus Ctrl+h in catalogue");
        assert!(matches!(
            catalogue[index].action,
            crate::runtime::InputSemanticAction::FocusStep { .. }
        ));
    }

    #[test]
    fn delta_saturates() {
        assert_eq!(delta(3, 5), 2);
        assert_eq!(delta(5, 5), 0);
        assert_eq!(delta(7, 5), 0);
    }

    #[test]
    fn arrows_cover_both_arms() {
        // Observation rows exist for all four arrows on both arms (4 arrows
        // x focus/move = 8 rows).
        let rows = arrow_observations(request(InputSessionRoute::Carbon), &[], &[]);
        assert_eq!(rows.len(), 8);
        for code in [0x7Bu32, 0x7Cu32, 0x7Du32, 0x7Eu32] {
            assert!(rows.iter().any(|r| r.key_code == code && !r.shift));
            assert!(rows.iter().any(|r| r.key_code == code && r.shift));
        }
    }
}

#![forbid(unsafe_code)]

//! Bounded cross-process group-underlay carrier fixture
//! (`group-underlay-session`).
//!
//! Controller (this process, through the signed LaunchServices bundle for
//! TCC) owns the [`crate::ffi::GroupUnderlayCarrier`] fill panel; THREE
//! separate owned disposable helper processes (existing `Child` + nonce
//! protocol) own the windows: two group members plus one nonmember receipt
//! window. Every leg joins the CURRENT live identity (spawn record + fresh
//! CG owner-PID/window join + AX identity) with per-action ownership
//! revalidation; only owned helpers are ever activated; no Space writes, no
//! taps, no synthetic keys. The session refuses on a locked (or unknown)
//! graphical session before touching UI, re-checks the lock before EACH leg,
//! and enforces its bounded lifetime as a real deadline: no new leg starts
//! past it, helpers self-exit at it, and overrun tears down instead of
//! continuing.
//!
//! What this fixture does NOT claim: `ignoresMouseEvents` is reported as an
//! API readback only — the REAL click proof is the nonmember receipt
//! helper's own AppKit `mouse_counts` delta around one guarded HID click at
//! a coordinate proven (by carrier-frame geometry) to lie in the fill margin
//! outside both members but inside the carrier, with the WindowServer hit
//! joins read at that point. Physical appearance stays parked (no
//! screenshot proof). Fullscreen-exit membership is read back but never
//! gated synchronously (the transition is asynchronous); the exit gates on
//! the AX fullscreen readback (`Some(false)`) after the bounded settle poll
//! through the held-Child authorization, plus carrier return and focus
//! agreement afterwards.

use serde::{Deserialize, Serialize};

/// Fixture outcome: measured readbacks, never desired-state claims. A failed
/// `pass` returns as `Err` carrying this JSON so every measured leg stays
/// observable.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupUnderlaySessionReport {
    /// Session observed unlocked at entry (and re-checked before each leg).
    pub session_unlocked: bool,
    /// AX trust observed at entry (`Some(true)` required to proceed).
    pub ax_trusted: Option<bool>,
    /// SLPS focus symbols available at entry.
    pub slps_available: bool,
    /// Member A helper PID (separate process from the carrier owner).
    pub member_a_pid: u32,
    /// Member A window ID (AX/CG joined, never guessed).
    pub member_a_wid: u32,
    /// Member B helper PID.
    pub member_b_pid: u32,
    /// Member B window ID.
    pub member_b_wid: u32,
    /// Nonmember receipt helper PID.
    pub nonmember_pid: u32,
    /// Nonmember receipt window ID.
    pub nonmember_wid: u32,
    /// Carrier window number (controller process).
    pub carrier_wid: i64,
    /// Exercised fill (shared `#aarrggbb`).
    pub fill_hex: String,
    /// `ignoresMouseEvents` readback (API evidence only, not click proof).
    pub carrier_ignores_mouse: bool,
    /// `canBecomeKeyWindow` readback (must be false).
    pub carrier_can_key: bool,
    /// `canBecomeMainWindow` readback (must be false).
    pub carrier_can_main: bool,
    /// Collection behavior holds `Managed` only.
    pub carrier_collection_managed_only: bool,
    /// Carrier window level (expected `Normal` = 0).
    pub carrier_level: i64,
    /// Fill `drawRect:` dispatches before teardown (fill-path proof).
    pub carrier_draws: u64,
    /// Global CG order: carrier below BOTH members (cross-PID, no owner
    /// filtering). `false` with a note is the API-constraint STOP, not a
    /// retryable failure.
    pub ordered_below_global: bool,
    /// Global CG index of the carrier (-1 when absent).
    pub cg_carrier_idx: isize,
    /// Global CG indices of members A and B (-1 when absent).
    pub cg_member_idxs: Vec<isize>,
    /// Global CG index of the nonmember (-1 when absent).
    pub cg_nonmember_idx: isize,
    /// Nonmember stacking class at the order leg: `above` (in front of both
    /// members), `interleaved` (between them), `below-carrier` (behind the
    /// carrier), `below-members` (behind members but ahead of the carrier),
    /// or `unknown` (any index missing).
    pub nonmember_position: String,
    /// AX focused (pid, wid) before the carrier showed.
    pub focused_before_carrier: Option<(i32, Option<u32>)>,
    /// AX focused (pid, wid) after carrier show/update legs.
    pub focused_after_carrier: Option<(i32, Option<u32>)>,
    /// Neither readback ever named the carrier or the controller.
    pub carrier_never_focused: bool,
    /// Reflow leg: member B moved, carrier re-tracked the recomputed union.
    pub reflow_ok: bool,
    /// Reflow leg set-to-readback micros (AX roundtrip + settle included;
    /// API sampling, not display lag).
    pub reflow_lag_us: u64,
    /// Target screen backing scale used for the explicit backing readback.
    pub backing_scale: f64,
    /// Click point in CG top-left points (proven in the fill margin).
    pub click_point_cg: Option<(f64, f64)>,
    /// SLS window-server hit at the click point (topmost window ID).
    pub click_sls_wid: Option<u32>,
    /// AX element-at-position hit owner at the click point.
    pub click_ax_hit: Option<(u32, u32)>,
    /// Receipt helper mouse-down counter before the click.
    pub click_mouse_before: u64,
    /// Receipt helper mouse-down counter after the click.
    pub click_mouse_after: u64,
    /// Receipt helper mouse-up counter before the click (supporting edge).
    pub click_mouse_up_before: u64,
    /// Receipt helper mouse-up counter after the click (supporting edge).
    pub click_mouse_up_after: u64,
    /// Click receipt proved (post-click counter exceeds pre-click).
    pub click_receipt_ok: bool,
    /// Pointer restored to the pre-click position via the eventless warp.
    pub pointer_restored: bool,
    /// Fence leg: carrier hides on `orderOut:` and reads back invisible.
    pub fence_hide_ok: bool,
    /// Fence leg: carrier re-shows below afterwards and reads back visible.
    pub fence_restore_ok: bool,
    /// Fullscreen leg: owned member entered fullscreen (AX readback).
    pub fullscreen_entered: bool,
    /// Fullscreen leg: carrier hid while the member was fullscreen.
    pub fullscreen_hide_ok: bool,
    /// Fullscreen exit readback (`None` = exit not attempted).
    pub fullscreen_exit_readback: Option<bool>,
    /// Fullscreen exit rode the held-Child authorization.
    pub fullscreen_exit_held: bool,
    /// Post-exit return: carrier re-shown below plus focus agreement.
    pub fullscreen_return_ok: bool,
    /// Member Space membership preimage (read-only, fullscreen leg).
    pub membership_before: Vec<u64>,
    /// Member Space membership after the leg (read-only; never gated).
    pub membership_after: Vec<u64>,
    /// Member A Child reaped by the exact held handle.
    pub member_a_reaped: bool,
    /// Member A process gone after stop.
    pub member_a_gone: bool,
    /// Member A spawn record removed after verified exit.
    pub member_a_record_removed: bool,
    /// Member B reaped/gone/record-removed.
    pub member_b_reaped: bool,
    pub member_b_gone: bool,
    pub member_b_record_removed: bool,
    /// Nonmember reaped/record removed/gone.
    pub nonmember_reaped: bool,
    pub nonmember_record_removed: bool,
    pub nonmember_gone: bool,
    /// Owned CG windows remaining for helpers + carrier (total).
    pub residue_total: usize,
    /// Owned CG windows still onscreen after teardown (must be 0).
    pub residue_onscreen: usize,
    /// Frontmost PID before the spawns (baseline; never restored by keys).
    pub frontmost_before: Option<i32>,
    /// Frontmost PID after teardown (readback only).
    pub frontmost_after: Option<i32>,
    /// Per-display Space topology identical before/after.
    pub space_restored: bool,
    /// Fixture wall time in seconds (bounded by `--seconds`).
    pub elapsed_secs: u64,
    /// Core acceptance: global below-member order, focus isolation, reflow,
    /// click receipt through the fill, fence hide/restore, fullscreen enter
    /// plus held-exit with return, and clean teardown of all helpers.
    pub pass: bool,
    /// Free-form leg notes (indices, lags, skips, constraint text).
    pub detail: String,
}

/// Minimum fixture lifetime: three spawns, joins, and legs need headroom.
pub const MIN_UNDERLAY_SECONDS: u64 = 45;
/// Maximum fixture lifetime (helpers cap at the 120s probe ceiling, so the
/// fixture keeps a 30s self-exit headroom inside it).
pub const MAX_UNDERLAY_SECONDS: u64 = 90;
/// Default fixture lifetime.
pub const DEFAULT_UNDERLAY_SECONDS: u64 = 75;

/// Pure lifetime gate for the fixture (also the enforced deadline budget).
pub fn validate_underlay_seconds(seconds: u64) -> Result<u64, String> {
    if (MIN_UNDERLAY_SECONDS..=MAX_UNDERLAY_SECONDS).contains(&seconds) {
        Ok(seconds)
    } else {
        Err(format!(
            "group-underlay-session needs --seconds {MIN_UNDERLAY_SECONDS}..={MAX_UNDERLAY_SECONDS} (helpers cap at the 120s probe ceiling)"
        ))
    }
}

/// Live-leg frame comparison (macOS legs and the pure unit test below use
/// this; the off-macOS stub never calls it).
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn frames_close(a: (f64, f64, f64, f64), b: (f64, f64, f64, f64)) -> bool {
    let close = |x: f64, y: f64| (x - y).abs() <= 0.5;
    close(a.0, b.0) && close(a.1, b.1) && close(a.2, b.2) && close(a.3, b.3)
}

/// Sample one fill-margin point from a carrier outer frame (AppKit
/// bottom-left points) given the member rects in the same space: the middle
/// of the top margin band. `None` when the band is degenerate, non-finite,
/// or the sample is not strictly outside every member. The caller proves
/// band membership; this sample alone proves nothing.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn fill_margin_sample_appkit(
    outer: (f64, f64, f64, f64),
    members: &[(f64, f64, f64, f64)],
) -> Option<(f64, f64)> {
    let (x, y, w, h) = outer;
    for v in [x, y, w, h] {
        if !v.is_finite() {
            return None;
        }
    }
    if w <= 0.0 || h <= 0.0 {
        return None;
    }
    let pad_top = outer.1 + outer.3
        - members
            .iter()
            .map(|m| m.1 + m.3)
            .fold(f64::NEG_INFINITY, f64::max);
    if !pad_top.is_finite() || pad_top <= 1.0 {
        return None;
    }
    let p = (x + w / 2.0, y + h - pad_top / 2.0);
    let strictly_inside = |r: (f64, f64, f64, f64), q: (f64, f64)| {
        q.0 > r.0 && q.0 < r.0 + r.2 && q.1 > r.1 && q.1 < r.1 + r.3
    };
    if members.iter().any(|m| strictly_inside(*m, p)) {
        return None;
    }
    Some(p)
}

/// Owned-window stack dump for diagnosis: per tracked WID its global index,
/// onscreen flag and layer, plus every CG window owned by the helper PIDs
/// (multi-window drift detection) and the list length. IDs, order and flags
/// only — never titles or content.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn owned_stack_dump(
    carrier_wid: i64,
    receipt_wid: u32,
    member_a_wid: u32,
    member_b_wid: u32,
    helper_pids: &[u32],
) -> String {
    match crate::ffi::cg_window_list_all() {
        Ok(list) => {
            let entry = |wid: u32| {
                list.iter().position(|w| w.wid == wid).map(|i| {
                    let w = &list[i];
                    format!("{wid}@{i}:onscreen={} L{}", w.onscreen, w.layer)
                })
            };
            let per_pid: Vec<String> = helper_pids
                .iter()
                .map(|pid| {
                    let owned: Vec<String> = list
                        .iter()
                        .filter(|w| w.pid == *pid)
                        .map(|w| format!("{}(onscreen={})", w.wid, w.onscreen))
                        .collect();
                    format!("pid{pid}=[{}]", owned.join(","))
                })
                .collect();
            format!(
                "len={} carrier={:?} receipt={:?} a={:?} b={:?} {}",
                list.len(),
                entry(carrier_wid as u32),
                entry(receipt_wid),
                entry(member_a_wid),
                entry(member_b_wid),
                per_pid.join(" ")
            )
        }
        Err(e) => format!("unreadable: {e}"),
    }
}

/// Front-to-back stack fingerprint for diagnosis: the first entries of the
/// global CG order as `wid:pid/Llayer` (IDs, layers, and order only).
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn stack_fingerprint() -> String {
    match crate::ffi::cg_window_list_all() {
        Ok(list) => list
            .iter()
            .take(16)
            .map(|w| format!("{}:{}/L{}", w.wid, w.pid, w.layer))
            .collect::<Vec<_>>()
            .join(" "),
        Err(e) => format!("unreadable: {e}"),
    }
}

/// Latest receipt-helper `mouse_counts` edges in the drained helper stdout
/// lines. Returns the newest parsed `(down, up)` pair; `None` means no
/// complete snapshot was observed (fail closed). The gate uses the down
/// delta; the up pair rides along as supporting evidence.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn latest_mouse_counts(lines: &std::sync::mpsc::Receiver<String>) -> Option<(u64, u64)> {
    let mut best = None;
    while let Ok(line) = lines.try_recv() {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&line)
            && let Some(counts) = value.get("mouse_counts")
            && let (Some(down), Some(up)) = (
                counts.get("left_down").and_then(|n| n.as_u64()),
                counts.get("left_up").and_then(|n| n.as_u64()),
            )
        {
            best = Some((down, up));
        }
    }
    best
}

fn fail(stage: &str, error: String) -> (i32, String) {
    (
        1,
        serde_json::json!({"stage": stage, "error": error}).to_string(),
    )
}

/// Run the bounded cross-process underlay fixture on the calling (main)
/// thread. Returns the JSON report on success; a failed `pass` (or a fixture
/// error) is an `Err` carrying the report JSON so every measured leg stays
/// observable.
pub fn run_group_underlay_session(seconds: u64) -> Result<String, (i32, String)> {
    validate_underlay_seconds(seconds).map_err(|e| (2, e))?;
    #[cfg(not(target_os = "macos"))]
    {
        let _ = seconds;
        Err(fail(
            "unsupported",
            "group-underlay-session is macOS-only".to_owned(),
        ))
    }
    #[cfg(target_os = "macos")]
    {
        run_group_underlay_session_macos(seconds)
    }
}

#[cfg(target_os = "macos")]
#[allow(clippy::too_many_lines)]
fn run_group_underlay_session_macos(seconds: u64) -> Result<String, (i32, String)> {
    use std::time::{Duration, Instant};

    let start = Instant::now();
    let deadline = start + Duration::from_secs(seconds);
    let mut detail: Vec<String> = Vec::new();
    let elapsed_secs = || start.elapsed().as_secs();

    if !crate::ffi::session_state().unlocked() {
        return Err(fail(
            "session-lock",
            "refusing on a locked or unknown graphical session".to_owned(),
        ));
    }
    let trust = crate::ffi::ax_is_process_trusted();
    if trust != Some(true) {
        return Err(fail(
            "ax-trust",
            format!("AX trust not observed true ({trust:?}); stop — user consent needed"),
        ));
    }
    if !crate::ffi::slps_available() {
        return Err(fail(
            "slps-unavailable",
            "SLPS focus symbols unavailable; stop — route absent".to_owned(),
        ));
    }
    let mtm = objc2::MainThreadMarker::new().ok_or_else(|| {
        fail(
            "main-thread",
            "group-underlay-session must run on the main thread".to_owned(),
        )
    })?;

    let gate = |stage: &str, detail: &mut Vec<String>| -> Result<(), (i32, String)> {
        if !crate::ffi::session_state().unlocked() {
            return Err(fail(stage, "session locked mid-run; stopping".to_owned()));
        }
        if Instant::now() >= deadline {
            detail.push(format!("{stage}: deadline exceeded; stopping"));
            return Err(fail(
                stage,
                "fixture deadline exceeded; stopping".to_owned(),
            ));
        }
        Ok(())
    };

    gate("baseline", &mut detail)?;
    let frontmost_before = crate::ffi::frontmost_app().map(|(pid, _)| pid);
    let space_before = crate::focus::capture_space().map_err(|e| fail("space-before", e))?;
    let displays =
        crate::ffi::ns_display_snapshot().map_err(|e| fail("displays", e.to_string()))?;
    let backing_scale = displays.first().map(|d| d.scale).unwrap_or(1.0);
    let primary_h = crate::topology::primary_height(&displays).ok_or_else(|| {
        fail(
            "displays",
            "primary display height unreadable; refusing mixed coordinates".to_owned(),
        )
    })?;
    let ax_to_appkit = |f: crate::ffi::AxFrame| tiler_core::visual::VisualRect {
        x: f.x,
        y: primary_h - (f.y + f.h),
        w: f.w,
        h: f.h,
    };
    detail.push(format!(
        "displays={} scales={:?} primary_h={primary_h}",
        displays.len(),
        displays.iter().map(|d| d.scale).collect::<Vec<_>>()
    ));

    // Spawn three owned helpers: the nonmember receipt FIRST so creation
    // order stacks it backmost, then members A and B. Regular policy on all
    // three (the click post needs a real bundle/oracle-capable helper).
    // Helper lifetimes outlast the fixture deadline with headroom: the
    // fixture never starts new legs past its own deadline and always stops
    // the exact held Children first, so self-exits are backstops only.
    gate("spawn", &mut detail)?;
    let helper_seconds = (seconds + 30).min(120);
    let mut raws: Vec<crate::probe_target::UnreadySpawn> = Vec::new();
    let mut nonces: Vec<String> = Vec::new();
    for (name, ox, oy) in [
        ("receipt", 200.0, 640.0),
        ("a", 200.0, 200.0),
        ("b", 760.0, 200.0),
    ] {
        let nonce = crate::probe_target::mint_nonce().map_err(|e| fail("spawn", e))?;
        let spec = crate::probe_target::ProbeTargetSpec::new(&nonce, 0, 0, helper_seconds)
            .map(|s| s.with_regular_policy(true).with_origin(ox, oy))
            .map_err(|e| fail("spawn", e))?;
        let mut raw = crate::probe_target::start_helper_process(&spec).map_err(|e| {
            for r in raws.iter_mut() {
                let _ = r.child.kill();
                let _ = r.child.wait();
            }
            fail("spawn", format!("{name}: {e}"))
        })?;
        if let Err(e) = crate::probe_target::await_handshake(&mut raw, &spec) {
            let _ = raw.child.kill();
            let _ = raw.child.wait();
            for r in raws.iter_mut() {
                let _ = r.child.kill();
                let _ = r.child.wait();
            }
            return Err(fail("spawn", format!("{name}: {e}")));
        }
        if let Err(e) = crate::probe_target::record_spawn(raw.pid, &spec, &raw.helper_path) {
            let _ = raw.child.kill();
            let _ = raw.child.wait();
            for r in raws.iter_mut() {
                let _ = r.child.kill();
                let _ = r.child.wait();
            }
            return Err(fail("spawn", format!("{name}: {e}")));
        }
        nonces.push(nonce);
        raws.push(raw);
    }
    // `raws` (including stdout receivers) stays alive for the whole run:
    // dropping a receiver ends the holder-side drain thread, after which
    // that helper's next periodic snapshot line raises EPIPE.
    let stop_one = |raw: &mut crate::probe_target::UnreadySpawn,
                    nonce: &str,
                    detail: &mut Vec<String>|
     -> (bool, bool, bool) {
        let pid = raw.pid;
        match crate::probe_target::load_verified_record(pid, nonce) {
            Ok(_) => {}
            Err(e) => {
                detail.push(format!("stop identity revalidation refused: {e}"));
                let live_exe = crate::ffi::process_executable_path(pid as i32);
                let child_live = matches!(raw.child.try_wait(), Ok(None));
                if child_live && live_exe.as_deref() == Some(raw.helper_path.as_str()) {
                    detail.push("stop fallback: exact held Child plus live-exe match".to_owned());
                } else {
                    detail.push(format!(
                        "stop fallback refused (child_live={child_live} exe={live_exe:?})"
                    ));
                    return (false, false, false);
                }
            }
        }
        let stop = crate::focus::stop_verified(&mut raw.child, pid);
        detail.push(format!(
            "stop reaped={} record_removed={} gone={} verified_exit={}",
            stop.reaped, stop.record_removed, stop.process_gone, stop.verified_exit
        ));
        (stop.reaped, stop.process_gone, stop.record_removed)
    };

    let app = objc2_app_kit::NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(objc2_app_kit::NSApplicationActivationPolicy::Accessory);
    app.finishLaunching();
    let pump = |ms: u64| {
        objc2::rc::autoreleasepool(|_| {
            crate::ffi::drain_app_events(&app);
            crate::ffi::pump_runloop_once(ms);
        });
    };

    // Join all three windows (bounded poll each; AX registration can lag).
    gate("join", &mut detail)?;
    let mut wids: Vec<u32> = Vec::new();
    for (i, raw) in raws.iter().enumerate() {
        let join_deadline = Instant::now() + Duration::from_secs(8);
        let wid = loop {
            match crate::focus::join_helper_wid(raw.pid) {
                Ok(wid) => break wid,
                Err(e) => {
                    if Instant::now() >= join_deadline {
                        for (j, r) in raws.iter_mut().enumerate() {
                            let _ = stop_one(r, &nonces[j], &mut detail);
                        }
                        return Err(fail("join", format!("helper {i}: {e}")));
                    }
                    pump(250);
                }
            }
        };
        wids.push(wid);
    }
    let (receipt_pid, member_a_pid, member_b_pid) = (raws[0].pid, raws[1].pid, raws[2].pid);
    let (receipt_wid, member_a_wid, member_b_wid) = (wids[0], wids[1], wids[2]);
    detail.push(format!(
        "receipt_pid={receipt_pid} receipt_wid={receipt_wid} \
         member_a_pid={member_a_pid} member_a_wid={member_a_wid} \
         member_b_pid={member_b_pid} member_b_wid={member_b_wid}"
    ));

    // Focus member A (owned SLPS dispatch only), then read the member AX
    // frames and build the union + carrier outer.
    gate("activate", &mut detail)?;
    let focused_before_carrier = crate::ffi::ax_focused_preimage_detailed().ok();
    let owned_a = crate::ax::authorize_owned_action(member_a_pid, &nonces[1]).map_err(|e| {
        for (j, r) in raws.iter_mut().enumerate() {
            let _ = stop_one(r, &nonces[j], &mut detail);
        }
        fail("activate", e.to_string())
    })?;
    if let Err(e) = crate::focus::dispatch_focus(member_a_pid, member_a_wid) {
        for (j, r) in raws.iter_mut().enumerate() {
            let _ = stop_one(r, &nonces[j], &mut detail);
        }
        return Err(fail("activate", e.to_string()));
    }
    pump(250);
    let read_a = || {
        crate::ax::read_window_owned(&owned_a, member_a_pid, member_a_wid)
            .map_err(|e| e.to_string())
    };
    let owned_b = crate::ax::authorize_owned_action(member_b_pid, &nonces[2]).map_err(|e| {
        for (j, r) in raws.iter_mut().enumerate() {
            let _ = stop_one(r, &nonces[j], &mut detail);
        }
        fail("activate", e.to_string())
    })?;
    let read_b = || {
        crate::ax::read_window_owned(&owned_b, member_b_pid, member_b_wid)
            .map_err(|e| e.to_string())
    };
    let frame_a_ax = read_a()
        .map_err(|e| {
            for (j, r) in raws.iter_mut().enumerate() {
                let _ = stop_one(r, &nonces[j], &mut detail);
            }
            fail("observe", e)
        })?
        .ax_frame
        .ok_or_else(|| {
            for (j, r) in raws.iter_mut().enumerate() {
                let _ = stop_one(r, &nonces[j], &mut detail);
            }
            fail("observe", "member A AX frame unreadable".to_owned())
        })?;
    let frame_b_ax = read_b()
        .map_err(|e| {
            for (j, r) in raws.iter_mut().enumerate() {
                let _ = stop_one(r, &nonces[j], &mut detail);
            }
            fail("observe", e)
        })?
        .ax_frame
        .ok_or_else(|| {
            for (j, r) in raws.iter_mut().enumerate() {
                let _ = stop_one(r, &nonces[j], &mut detail);
            }
            fail("observe", "member B AX frame unreadable".to_owned())
        })?;
    let frame_a = ax_to_appkit(frame_a_ax);
    let frame_b = ax_to_appkit(frame_b_ax);
    let union = crate::group_underlay::union_rect(&[frame_a, frame_b]).ok_or_else(|| {
        for (j, r) in raws.iter_mut().enumerate() {
            let _ = stop_one(r, &nonces[j], &mut detail);
        }
        fail("observe", "member union refused".to_owned())
    })?;
    let outer = crate::group_underlay::underlay_outer_rect(
        union,
        crate::group_underlay::DEFAULT_GAP,
        crate::group_underlay::DEFAULT_BORDER_WIDTH,
        crate::group_underlay::DEFAULT_EXTENSION,
    )
    .ok_or_else(|| {
        for (j, r) in raws.iter_mut().enumerate() {
            let _ = stop_one(r, &nonces[j], &mut detail);
        }
        fail("observe", "underlay outer refused".to_owned())
    })?;
    detail.push(format!(
        "union=({:.1},{:.1},{:.1},{:.1}) outer=({:.1},{:.1},{:.1},{:.1})",
        union.x, union.y, union.w, union.h, outer.x, outer.y, outer.w, outer.h
    ));

    // Carrier create + show below the LOWEST member. Cross-process relative
    // ordering is NOT assumed: the verdict comes only from the global CG
    // readback below.
    gate("carrier", &mut detail)?;
    let fill = crate::group_underlay::DEFAULT_FILL_RGBA;
    let carrier = crate::ffi::GroupUnderlayCarrier::create(
        mtm,
        outer,
        fill,
        &format!("OmniTiler group underlay {}", nonces[1]),
    )
    .map_err(|e| {
        for (j, r) in raws.iter_mut().enumerate() {
            let _ = stop_one(r, &nonces[j], &mut detail);
        }
        fail("carrier", e)
    })?;
    let flags = carrier.flags();
    let carrier_wid = carrier.window_number();
    // Lowest member = larger CG index (further back) right now; re-read
    // after every restack below.
    let lowest_member_wid = || -> u32 {
        let order: Vec<(u32, isize)> = crate::ffi::cg_window_list_all()
            .map(|list| {
                list.iter()
                    .enumerate()
                    .filter_map(|(i, w)| {
                        if w.wid == member_a_wid {
                            Some((member_a_wid, i as isize))
                        } else if w.wid == member_b_wid {
                            Some((member_b_wid, i as isize))
                        } else {
                            None
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();
        let mut best = member_b_wid;
        let mut best_idx = -2isize;
        for (wid, idx) in order {
            if idx > best_idx {
                best_idx = idx;
                best = wid;
            }
        }
        best
    };
    carrier.show_below(mtm, lowest_member_wid() as isize);
    pump(250);
    let visible_after_show = carrier.is_visible();
    let draws_after_show = carrier.draws();

    macro_rules! teardown_fail {
        ($e:expr) => {{
            carrier.destroy(mtm);
            for (j, r) in raws.iter_mut().enumerate() {
                let _ = stop_one(r, &nonces[j], &mut detail);
            }
            eprintln!("group-underlay-session detail: {}", detail.join("; "));
            return Err($e);
        }};
    }

    // ORDER leg (global, cross-PID, no owner filtering): the carrier must
    // follow BOTH members in list order. Settle-tolerant poll (stacking may
    // still be in flight after activation + show). A persistent miss is the
    // API-constraint STOP (pass=false) with the remaining legs still run.
    if gate("order", &mut detail).is_err() {
        carrier.destroy(mtm);
        for (j, r) in raws.iter_mut().enumerate() {
            let _ = stop_one(r, &nonces[j], &mut detail);
        }
        return Err(fail("order", "gate refused".to_owned()));
    }
    let order_deadline = Instant::now() + Duration::from_secs(3);
    let (ordered_below_global, cg_carrier_idx, cg_member_idxs) = loop {
        let cg_list = crate::ffi::cg_window_list_all().unwrap_or_default();
        let (below, ci, mi) = crate::ffi::cg_carrier_below_targets(
            &cg_list,
            carrier_wid as isize,
            &[member_a_wid, member_b_wid],
        );
        if below || Instant::now() >= order_deadline {
            break (below, ci, mi);
        }
        pump(250);
    };
    let cg_nonmember_idx = crate::ffi::cg_window_list_all()
        .map(|list| {
            list.iter()
                .position(|w| w.wid == receipt_wid)
                .map(|i| i as isize)
                .unwrap_or(-1)
        })
        .unwrap_or(-1);
    let nonmember_position = match (cg_member_idxs.as_slice(), cg_nonmember_idx) {
        ([a, b], n) if *a >= 0 && *b >= 0 && n >= 0 => {
            let front = (*a).min(*b);
            let back = (*a).max(*b);
            if n < front {
                "above"
            } else if n <= back {
                "interleaved"
            } else if cg_carrier_idx >= 0 && n > cg_carrier_idx {
                "below-carrier"
            } else {
                "below-members"
            }
        }
        _ => "unknown",
    };
    detail.push(format!(
        "order below={ordered_below_global} carrier={cg_carrier_idx} members={cg_member_idxs:?} \
         nonmember={cg_nonmember_idx} ({nonmember_position}) visible={visible_after_show} \
         draws={draws_after_show} stack=[{}]",
        stack_fingerprint()
    ));
    detail.push(format!(
        "owned stack: {}",
        owned_stack_dump(
            carrier_wid,
            receipt_wid,
            member_a_wid,
            member_b_wid,
            &[receipt_pid, member_a_pid, member_b_pid]
        )
    ));
    if !ordered_below_global {
        detail.push(
            "ORDER CONSTRAINT: carrier not below both foreign members in the global CG list; \
             public relative ordering is unreliable cross-process (no SkyLight attempted)"
                .to_owned(),
        );
    }

    // FOCUS legs: focus must still name member A; the carrier/controller
    // must never be focused. This proves no activation by our panel.
    if gate("focus", &mut detail).is_err() {
        teardown_fail!(fail("focus", "gate refused".to_owned()));
    }
    let focused_after_carrier = crate::ffi::ax_focused_preimage_detailed().ok();
    let me = std::process::id() as i32;
    let names_me =
        |f: &Option<(i32, Option<u32>)>| -> bool { matches!(f, Some((pid, _)) if *pid == me) };
    let focus_names_member_a =
        focused_after_carrier == Some((member_a_pid as i32, Some(member_a_wid)));
    let carrier_never_focused =
        !names_me(&focused_before_carrier) && !names_me(&focused_after_carrier);
    detail.push(format!(
        "focused_before={focused_before_carrier:?} focused_after={focused_after_carrier:?}"
    ));

    // REFLOW leg: move member B, recompute the union, re-track the carrier
    // to the OBSERVED frames (never the requested one), and read back the
    // actual panel frame.
    if gate("reflow", &mut detail).is_err() {
        teardown_fail!(fail("reflow", "gate refused".to_owned()));
    }
    let t0 = Instant::now();
    let want_b = crate::ffi::AxFrame {
        x: 840.0,
        y: 260.0,
        w: 480.0,
        h: 320.0,
    };
    let persistent =
        match crate::ax::set_frame_owned_persistent(&owned_b, member_b_pid, member_b_wid, want_b) {
            Ok(p) => p,
            Err(e) => teardown_fail!(fail("reflow", e.to_string())),
        };
    if !persistent.matched {
        teardown_fail!(fail(
            "reflow",
            format!(
                "member B frame not applied (preimage {:?}, attempted {:?}, applied {:?})",
                persistent.preimage, persistent.attempted, persistent.applied
            )
        ));
    }
    let poll_deadline = Instant::now() + Duration::from_secs(5);
    let mut seen_b: Option<crate::ffi::AxFrame> = None;
    while Instant::now() < poll_deadline {
        if let Ok(cur) = read_b()
            && cur.ax_frame == Some(want_b)
        {
            seen_b = cur.ax_frame;
            break;
        }
        pump(100);
    }
    let seen_b = match seen_b {
        Some(f) => f,
        None => teardown_fail!(fail(
            "reflow",
            "AX poll never showed the applied member B frame".to_owned()
        )),
    };
    let seen_a_ax = match read_a() {
        Ok(st) => st.ax_frame,
        Err(e) => teardown_fail!(fail("reflow", e)),
    };
    let seen_a_ax = match seen_a_ax {
        Some(f) => f,
        None => teardown_fail!(fail("reflow", "member A AX frame unreadable".to_owned())),
    };
    let reflow_union =
        match crate::group_underlay::union_rect(&[ax_to_appkit(seen_a_ax), ax_to_appkit(seen_b)]) {
            Some(u) => u,
            None => teardown_fail!(fail("reflow", "reflow union refused".to_owned())),
        };
    let reflow_outer = match crate::group_underlay::underlay_outer_rect(
        reflow_union,
        crate::group_underlay::DEFAULT_GAP,
        crate::group_underlay::DEFAULT_BORDER_WIDTH,
        crate::group_underlay::DEFAULT_EXTENSION,
    ) {
        Some(o) => o,
        None => teardown_fail!(fail("reflow", "reflow outer refused".to_owned())),
    };
    if let Err(e) = carrier.update(mtm, reflow_outer, fill) {
        teardown_fail!(fail("reflow", e));
    }
    carrier.show_below(mtm, lowest_member_wid() as isize);
    pump(50);
    let got = carrier.frame_points();
    let reflow_ok = frames_close(
        got,
        (
            reflow_outer.x,
            reflow_outer.y,
            reflow_outer.w,
            reflow_outer.h,
        ),
    );
    let reflow_lag_us = t0.elapsed().as_micros() as u64;
    let backing = carrier.backing_rect(backing_scale);
    detail.push(format!(
        "reflow lag_us={reflow_lag_us} carrier_frame={got:?} backing={backing:?}"
    ));

    // CLICK leg: one synthetic click at a FILL-MARGIN pixel (outside both
    // members, inside the carrier) with receipt by the nonmember window
    // parked underneath. The nonmember is re-parked centered on the sampled
    // margin point; the SLS input hit-test and the AX hit must both name the
    // nonmember while the unskipped global CG order proves the carrier sits
    // between the members and the nonmember. Together with the margin proof
    // and the counter delta, that is the traversal proof.
    if gate("click", &mut detail).is_err() {
        teardown_fail!(fail("click", "gate refused".to_owned()));
    }
    let cursor_before = match crate::synth::cursor_position() {
        Some(p) => p,
        None => teardown_fail!(fail(
            "click",
            "live cursor position unreadable; refusing rather than leaving the pointer unrestored"
                .to_owned()
        )),
    };
    let carrier_now = carrier.frame_points();
    let member_now = |wid_frame: tiler_core::visual::VisualRect| {
        (wid_frame.x, wid_frame.y, wid_frame.w, wid_frame.h)
    };
    let margin_appkit = match fill_margin_sample_appkit(
        carrier_now,
        &[
            member_now(ax_to_appkit(seen_a_ax)),
            member_now(ax_to_appkit(seen_b)),
        ],
    ) {
        Some(p) => p,
        None => teardown_fail!(fail(
            "click",
            "fill margin sample refused (degenerate band or inside a member)".to_owned()
        )),
    };
    let click_cg = (margin_appkit.0, primary_h - margin_appkit.1);
    if let Err(e) = crate::synth::validate_click_point(click_cg) {
        teardown_fail!(fail("click", format!("margin CG point refused ({e})")));
    }
    let owned_r = match crate::ax::authorize_owned_action(receipt_pid, &nonces[0]) {
        Ok(o) => o,
        Err(e) => teardown_fail!(fail("click", format!("receipt authorize: {e}"))),
    };
    let read_r = || {
        crate::ax::read_window_owned(&owned_r, receipt_pid, receipt_wid).map_err(|e| e.to_string())
    };
    let want_receipt = crate::ffi::AxFrame {
        x: click_cg.0 - 40.0,
        y: click_cg.1 - 40.0,
        w: 80.0,
        h: 80.0,
    };
    let placed = match crate::ax::set_frame_owned_persistent(
        &owned_r,
        receipt_pid,
        receipt_wid,
        want_receipt,
    ) {
        Ok(p) => p,
        Err(e) => teardown_fail!(fail("click-place", e.to_string())),
    };
    let applied = match placed.applied {
        Some(f) => f,
        None => teardown_fail!(fail(
            "click-place",
            "receipt frame not applied; click unprovable without covering foreign windows"
                .to_owned()
        )),
    };
    let poll_deadline = Instant::now() + Duration::from_secs(5);
    let mut receipt_placed = false;
    while Instant::now() < poll_deadline {
        if let Ok(cur) = read_r()
            && cur.ax_frame == Some(applied)
        {
            receipt_placed = true;
            break;
        }
        pump(100);
    }
    if !receipt_placed {
        teardown_fail!(fail(
            "click-place",
            "receipt never settled at the applied frame; skipping".to_owned()
        ));
    }
    let margin = 5.0;
    if !(click_cg.0 >= applied.x + margin
        && click_cg.0 <= applied.x + applied.w - margin
        && click_cg.1 >= applied.y + margin
        && click_cg.1 <= applied.y + applied.h - margin)
    {
        teardown_fail!(fail(
            "click",
            format!(
                "margin pixel ({:.1},{:.1}) outside the applied receipt frame; refusing",
                click_cg.0, click_cg.1
            )
        ));
    }
    // Re-establish below-member order after the receipt move, then read the
    // hit joins at the click point. The hit-tests skip click-through
    // windows (they name the receipt, never the carrier); acceptance needs
    // them to name the receipt while the CG order proves the carrier is
    // stacked between the members and the receipt.
    carrier.show_below(mtm, lowest_member_wid() as isize);
    pump(200);
    let sls_hit = crate::ffi::sls_window_at_point(click_cg.0, click_cg.1).ok();
    let ax_hit = crate::ffi::ax_hit_test_owner(click_cg.0, click_cg.1).ok();
    let (pre_carrier_idx, pre_receipt_idx, pre_member_idxs) = match crate::ffi::cg_window_list_all()
    {
        Ok(list) => {
            let mut ci: isize = -1;
            let mut ri: isize = -1;
            let mut mi = vec![-1isize; 2];
            for (i, w) in list.iter().enumerate() {
                if w.wid as i64 == carrier_wid && ci < 0 {
                    ci = i as isize;
                }
                if w.wid == receipt_wid && ri < 0 {
                    ri = i as isize;
                }
                if w.wid == member_a_wid && mi[0] < 0 {
                    mi[0] = i as isize;
                }
                if w.wid == member_b_wid && mi[1] < 0 {
                    mi[1] = i as isize;
                }
            }
            (ci, ri, mi)
        }
        Err(_) => (-1, -1, vec![-1, -1]),
    };
    let sls_names_receipt = sls_hit == Some(receipt_wid);
    let ax_names_receipt = matches!(ax_hit, Some((p, w)) if p == receipt_pid && w == receipt_wid);
    let carrier_between =
        pre_carrier_idx >= 0 && pre_receipt_idx >= 0 && pre_carrier_idx < pre_receipt_idx;
    let carrier_shown = carrier.is_visible();
    detail.push(format!(
        "click margin cg=({:.1},{:.1}) sls={sls_hit:?} ax={ax_hit:?} \
         cg_carrier={pre_carrier_idx} cg_receipt={pre_receipt_idx} cg_members={pre_member_idxs:?} \
         shown={carrier_shown}",
        click_cg.0, click_cg.1
    ));
    if !(sls_names_receipt && ax_names_receipt && carrier_between && carrier_shown) {
        teardown_fail!(fail(
            "click",
            format!(
                "click pixel not proven traversable (sls={sls_hit:?} ax={ax_hit:?} \
                 cg_carrier={pre_carrier_idx} cg_receipt={pre_receipt_idx} shown={carrier_shown} \
                 stack=[{}]); refusing a click that cannot traverse the fill",
                stack_fingerprint()
            )
        ));
    }
    if let Err(e) = crate::focus::dispatch_focus(receipt_pid, receipt_wid) {
        teardown_fail!(fail("click-focus", e.to_string()));
    }
    pump(250);
    if let Err(e) = crate::focus::read_agreement(receipt_pid, receipt_wid) {
        teardown_fail!(fail("click-focus", format!("receipt agreement: {e}")));
    }
    // Activation may restack the receipt above the carrier: re-prove the
    // between-order immediately before the post (our own panel only, never
    // a foreign window), up to three times. The final readback decides.
    let mut order_before_post: Option<(isize, isize)> = None;
    for _ in 0..3 {
        carrier.show_below(mtm, lowest_member_wid() as isize);
        pump(150);
        order_before_post = match crate::ffi::cg_window_list_all() {
            Ok(list) => {
                let mut ci: isize = -1;
                let mut ri: isize = -1;
                for (i, w) in list.iter().enumerate() {
                    if w.wid as i64 == carrier_wid && ci < 0 {
                        ci = i as isize;
                    }
                    if w.wid == receipt_wid && ri < 0 {
                        ri = i as isize;
                    }
                }
                Some((ci, ri))
            }
            Err(_) => None,
        };
        if matches!(order_before_post, Some((ci, ri)) if ci >= 0 && ri >= 0 && ci < ri) {
            break;
        }
    }
    match order_before_post {
        Some((ci, ri)) if ci >= 0 && ri >= 0 && ci < ri => {
            detail.push(format!("click order before post carrier={ci} receipt={ri}"));
        }
        other => {
            teardown_fail!(fail(
                "click",
                format!(
                    "carrier no longer above receipt before post (order={other:?} stack=[{}]); refusing",
                    stack_fingerprint()
                )
            ));
        }
    }
    let (mouse_before, mouse_up_before) = {
        pump(600);
        match latest_mouse_counts(&raws[0].lines) {
            Some(n) => n,
            None => teardown_fail!(fail(
                "click",
                "no receipt mouse snapshot observed before the click".to_owned()
            )),
        }
    };
    let posted = {
        let receipt_helper_path = raws[0].helper_path.clone();
        let post_target = match crate::synth::OwnedPostTarget::from_held_child(
            &mut raws[0].child,
            receipt_pid,
            receipt_wid,
            &receipt_helper_path,
        ) {
            Ok(t) => t,
            Err(e) => teardown_fail!(fail("click", format!("click target refused: {e}"))),
        };
        match crate::synth::post_mouse_click_guarded(&post_target, click_cg) {
            Ok(p) => p,
            Err(e) => teardown_fail!(fail("click", format!("click post refused: {e}"))),
        }
    };
    pump(600);
    pump(600);
    let (mouse_after, mouse_up_after) = match latest_mouse_counts(&raws[0].lines) {
        Some(n) => n,
        None => teardown_fail!(fail(
            "click",
            "no receipt mouse snapshot observed after the click".to_owned()
        )),
    };
    let click_receipt_ok = mouse_after > mouse_before;
    detail.push(format!(
        "click down_us={} up_us={} mouse_before={mouse_before} mouse_after={mouse_after} \
         mouse_up_before={mouse_up_before} mouse_up_after={mouse_up_after}",
        posted.down_latency_us, posted.up_latency_us
    ));
    if !click_receipt_ok {
        teardown_fail!(fail(
            "click",
            format!(
                "no receipt delta (before={mouse_before} after={mouse_after}); \
                 posted click not delivered (cg=({:.1},{:.1}) sls_hit={sls_hit:?} \
                 ax_hit={ax_hit:?} carrier_wid={carrier_wid} receipt_wid={receipt_wid})",
                click_cg.0, click_cg.1
            )
        ));
    }
    if let Err(e) = crate::ffi::warp_cursor_position(cursor_before.0, cursor_before.1) {
        teardown_fail!(fail("click", format!("pointer restore refused: {e}")));
    }
    let pointer_restored = match crate::synth::cursor_position() {
        Some(p) if (p.0 - cursor_before.0).abs() <= 2.0 && (p.1 - cursor_before.1).abs() <= 2.0 => {
            true
        }
        other => {
            detail.push(format!("pointer restore readback off: {other:?}"));
            false
        }
    };
    if !pointer_restored {
        teardown_fail!(fail(
            "click",
            "pointer restore readback missed the pre-click position".to_owned()
        ));
    }
    if let Err(e) = crate::focus::dispatch_focus(member_a_pid, member_a_wid) {
        teardown_fail!(fail("click", format!("member refocus: {e}")));
    }
    pump(250);
    if let Err(e) = crate::focus::read_agreement(member_a_pid, member_a_wid) {
        teardown_fail!(fail("click", format!("member agreement after click: {e}")));
    }
    carrier.show_below(mtm, lowest_member_wid() as isize);
    pump(200);
    if !carrier.is_visible() {
        teardown_fail!(fail(
            "click",
            "carrier not visible after click focus restore".to_owned()
        ));
    }

    // FENCE leg (no Space mutation): hide reads back invisible, re-show
    // below reads back visible and below both members.
    if gate("fence", &mut detail).is_err() {
        teardown_fail!(fail("fence", "gate refused".to_owned()));
    }
    carrier.hide(mtm);
    pump(200);
    let fence_hide_ok = !carrier.is_visible();
    carrier.show_below(mtm, lowest_member_wid() as isize);
    pump(200);
    let fence_restore_ok = carrier.is_visible();

    // FULLSCREEN leg on owned member A only: enter via the ownership-joined
    // AX control, verify the carrier hides DURING fullscreen, then exit via
    // the HELD-Child authorization and prove the return. Membership is read
    // back read-only — never gated.
    if gate("fullscreen", &mut detail).is_err() {
        teardown_fail!(fail("fullscreen", "gate refused".to_owned()));
    }
    let membership_before = crate::ffi::sls_spaces_for_window(member_a_wid).unwrap_or_default();
    let enter = match crate::ax::set_fullscreen_owned(&owned_a, member_a_pid, member_a_wid, true) {
        Ok(e) => e,
        Err(e) => teardown_fail!(fail("fullscreen", e.to_string())),
    };
    let fullscreen_entered = enter.applied && enter.fullscreen == Some(true);
    detail.push(format!(
        "fs_enter applied={} readback={:?} membership_before={membership_before:?}",
        enter.applied, enter.fullscreen
    ));
    let mut fullscreen_hide_ok = false;
    let mut fullscreen_exit_readback: Option<bool> = None;
    let mut fullscreen_exit_held = false;
    let mut fullscreen_return_ok = false;
    if fullscreen_entered {
        carrier.hide(mtm);
        pump(250);
        fullscreen_hide_ok = !carrier.is_visible();
        let member_a_helper_path = raws[1].helper_path.clone();
        match crate::ax::set_fullscreen_owned_held(
            &owned_a,
            &mut raws[1].child,
            &member_a_helper_path,
            member_a_pid,
            member_a_wid,
            false,
        ) {
            Ok(exit) => {
                fullscreen_exit_readback = Some(exit.fullscreen.unwrap_or(true));
                fullscreen_exit_held = true;
                detail.push(format!(
                    "fs_exit held applied={} readback={:?}",
                    exit.applied, exit.fullscreen
                ));
                carrier.show_below(mtm, lowest_member_wid() as isize);
                pump(250);
                let back_visible = carrier.is_visible();
                let back_focused = crate::ffi::ax_focused_preimage_detailed().ok()
                    == Some((member_a_pid as i32, Some(member_a_wid)));
                fullscreen_return_ok = back_visible && back_focused;
                detail.push(format!(
                    "fs_return visible={back_visible} focused_member_a={back_focused}"
                ));
            }
            Err(e) => {
                detail.push(format!("fs_exit held refused ({e})"));
            }
        }
    } else {
        detail.push("fullscreen leg skipped: AX enter not applied; no exit attempted".to_owned());
    }
    let mut membership_after = crate::ffi::sls_spaces_for_window(member_a_wid).unwrap_or_default();
    if membership_after != membership_before {
        pump(500);
        membership_after = crate::ffi::sls_spaces_for_window(member_a_wid).unwrap_or_default();
        detail.push(format!("membership_after resettled={membership_after:?}"));
    }

    // TEARDOWN: destroy the carrier, stop the EXACT held helpers (identity
    // revalidated before each signal), then prove the residue: helpers gone,
    // records removed, no owned CG windows onscreen, Space topology
    // identical to the preimage. Focus is READ BACK only.
    if gate("teardown", &mut detail).is_err() {
        teardown_fail!(fail("teardown", "gate refused".to_owned()));
    }
    let carrier_draws = carrier.draws();
    carrier.destroy(mtm);
    pump(200);
    let (nonmember_reaped, nonmember_gone, nonmember_record_removed) =
        stop_one(&mut raws[0], &nonces[0], &mut detail);
    let (member_a_reaped, member_a_gone, member_a_record_removed) =
        stop_one(&mut raws[1], &nonces[1], &mut detail);
    let (member_b_reaped, member_b_gone, member_b_record_removed) =
        stop_one(&mut raws[2], &nonces[2], &mut detail);
    pump(400);
    let residue: Vec<_> = crate::ffi::cg_window_list_all()
        .map(|list| {
            list.into_iter()
                .filter(|w| {
                    w.pid == receipt_pid
                        || w.pid == member_a_pid
                        || w.pid == member_b_pid
                        || (w.pid == std::process::id() && w.wid as i64 == carrier_wid)
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let residue_total = residue.len();
    let residue_onscreen = residue.iter().filter(|w| w.onscreen).count();
    let space_after = match crate::focus::capture_space() {
        Ok(s) => s,
        Err(e) => {
            for (j, r) in raws.iter_mut().enumerate() {
                let _ = stop_one(r, &nonces[j], &mut detail);
            }
            return Err(fail("space-after", e));
        }
    };
    let space_restored = space_after == space_before;
    let frontmost_after = crate::ffi::frontmost_app().map(|(pid, _)| pid);
    let focused_final = crate::ffi::ax_focused_preimage_detailed().ok();
    detail.push(format!(
        "space_restored={space_restored} frontmost_before={frontmost_before:?} \
         frontmost_after={frontmost_after:?} focused_final={focused_final:?}"
    ));

    let pass = ordered_below_global
        && focus_names_member_a
        && carrier_never_focused
        && reflow_ok
        && click_receipt_ok
        && pointer_restored
        && fence_hide_ok
        && fence_restore_ok
        && fullscreen_entered
        && fullscreen_hide_ok
        && fullscreen_exit_readback == Some(false)
        && fullscreen_exit_held
        && fullscreen_return_ok
        && member_a_reaped
        && member_a_gone
        && member_a_record_removed
        && member_b_reaped
        && member_b_gone
        && member_b_record_removed
        && nonmember_reaped
        && nonmember_gone
        && nonmember_record_removed
        && residue_onscreen == 0
        && space_restored;
    let report = GroupUnderlaySessionReport {
        session_unlocked: true,
        ax_trusted: trust,
        slps_available: true,
        member_a_pid,
        member_a_wid,
        member_b_pid,
        member_b_wid,
        nonmember_pid: receipt_pid,
        nonmember_wid: receipt_wid,
        carrier_wid,
        fill_hex: crate::group_underlay::render_fill(fill),
        carrier_ignores_mouse: flags.ignores_mouse,
        carrier_can_key: flags.can_key,
        carrier_can_main: flags.can_main,
        carrier_collection_managed_only: flags.collection_managed_only,
        carrier_level: flags.level,
        carrier_draws,
        ordered_below_global,
        cg_carrier_idx,
        cg_member_idxs,
        cg_nonmember_idx,
        nonmember_position: nonmember_position.to_owned(),
        focused_before_carrier,
        focused_after_carrier,
        carrier_never_focused,
        reflow_ok,
        reflow_lag_us,
        backing_scale,
        click_point_cg: Some(click_cg),
        click_sls_wid: sls_hit,
        click_ax_hit: ax_hit,
        click_mouse_before: mouse_before,
        click_mouse_after: mouse_after,
        click_mouse_up_before: mouse_up_before,
        click_mouse_up_after: mouse_up_after,
        click_receipt_ok,
        pointer_restored,
        fence_hide_ok,
        fence_restore_ok,
        fullscreen_entered,
        fullscreen_hide_ok,
        fullscreen_exit_readback,
        fullscreen_exit_held,
        fullscreen_return_ok,
        membership_before,
        membership_after,
        member_a_reaped,
        member_a_gone,
        member_a_record_removed,
        member_b_reaped,
        member_b_gone,
        member_b_record_removed,
        nonmember_reaped,
        nonmember_record_removed,
        nonmember_gone,
        residue_total,
        residue_onscreen,
        frontmost_before,
        frontmost_after,
        space_restored,
        elapsed_secs: elapsed_secs(),
        pass,
        detail: detail.join("; "),
    };
    let body = serde_json::to_string(&report).map_err(|e| fail("encode", e.to_string()))?;
    if !pass {
        return Err((1, body));
    }
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn underlay_seconds_gate_is_tight_and_real() {
        assert!(validate_underlay_seconds(MIN_UNDERLAY_SECONDS).is_ok());
        assert!(validate_underlay_seconds(DEFAULT_UNDERLAY_SECONDS).is_ok());
        assert!(validate_underlay_seconds(MAX_UNDERLAY_SECONDS).is_ok());
        assert!(validate_underlay_seconds(MIN_UNDERLAY_SECONDS - 1).is_err());
        assert!(validate_underlay_seconds(MAX_UNDERLAY_SECONDS + 1).is_err());
        assert!(validate_underlay_seconds(0).is_err());
    }

    #[test]
    fn margin_sample_needs_a_real_band_outside_members() {
        // Outer (0,0,200,100), one member inset by 10 on every side: the top
        // band middle is (100, 95) — inside outer, outside the member.
        let outer = (0.0, 0.0, 200.0, 100.0);
        let members = vec![(10.0, 10.0, 180.0, 80.0)];
        assert_eq!(
            fill_margin_sample_appkit(outer, &members),
            Some((100.0, 95.0))
        );
        // A member covering the band middle refuses.
        let covering = vec![(0.0, 0.0, 200.0, 100.0)];
        assert!(fill_margin_sample_appkit(outer, &covering).is_none());
        // Degenerate outer refuses.
        assert!(fill_margin_sample_appkit((0.0, 0.0, 0.0, 10.0), &members).is_none());
    }

    #[test]
    fn frames_close_tolerates_half_point_rounding() {
        assert!(frames_close((1.0, 2.0, 3.0, 4.0), (1.4, 2.4, 3.4, 4.4)));
        assert!(!frames_close((1.0, 2.0, 3.0, 4.0), (1.6, 2.0, 3.0, 4.0)));
    }
}

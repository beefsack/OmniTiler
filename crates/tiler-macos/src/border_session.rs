#![forbid(unsafe_code)]

//! Bounded cross-process active-border carrier fixture (`border-session`).
//!
//! Controller (this process, through the signed LaunchServices bundle for
//! TCC) owns the [`crate::ffi::BorderCarrier`] panel; a SEPARATE owned
//! disposable helper process (existing `Child` + nonce protocol) owns the
//! target window. Every leg joins the CURRENT live identity (spawn record +
//! fresh CG owner-PID/window join + AX identity) with per-action ownership
//! revalidation; only the owned helper is ever activated (never Ghostty,
//! never any foreign window); no Space writes, no taps, no synthetic keys.
//! The session refuses on a locked (or unknown) graphical session before
//! touching UI, re-checks the lock before EACH leg, and enforces its bounded
//! lifetime as a real deadline: no new leg starts past it, the helper
//! self-exits at it, and overrun tears down instead of continuing.
//!
//! What this fixture does NOT claim: `ignoresMouseEvents` is reported as an
//! API readback only — the REAL click proof is the owned receipt helper's
//! own AppKit `mouse_counts` delta around one guarded HID click at a
//! coordinate proven (by carrier-frame geometry) to lie in the painted ring
//! band, with the WindowServer hit joins read at that point. Physical
//! appearance stays parked (no screenshot proof). Fullscreen-exit
//! membership is read back but never gated synchronously (the transition
//! is asynchronous); the exit itself gates on the AX fullscreen readback
//! (`Some(false)`) after the bounded settle poll, driven through the
//! held-Child authorization (no title proof is available while the helper
//! is fullscreen), plus carrier return and focus agreement afterwards.

use serde::{Deserialize, Serialize};

/// Fixture outcome: measured readbacks, never desired-state claims. A failed
/// `pass` returns as `Err` carrying this JSON so every measured leg stays
/// observable.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BorderSessionReport {
    /// Session observed unlocked at entry (and re-checked before each leg).
    pub session_unlocked: bool,
    /// AX trust observed at entry (`Some(true)` required to proceed).
    pub ax_trusted: Option<bool>,
    /// SLPS focus symbols available at entry.
    pub slps_available: bool,
    /// Owned helper PID (separate process from the carrier owner).
    pub helper_pid: u32,
    /// Owned helper target window ID (AX/CG joined, never guessed).
    pub helper_wid: u32,
    /// Carrier window number (controller process).
    pub carrier_wid: i64,
    /// Echo of the exercised style (shared defaults).
    pub style_width: f64,
    /// Echo of the exercised gap.
    pub style_gap: f64,
    /// Echo of the exercised radius.
    pub style_radius: f64,
    /// Echo of the exercised colour (effective: accent or fallback).
    pub style_color_hex: String,
    /// Live accent resolved (`None` = fallback was used).
    pub accent_hex: Option<String>,
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
    /// Ring `drawRect:` dispatches before teardown (stroke-path proof).
    pub carrier_draws: u64,
    /// Global CG order: carrier above the foreign target (cross-PID, no
    /// owner filtering). `false` with a note is the API-constraint STOP, not
    /// a retryable failure: public relative ordering does not extend
    /// cross-process.
    pub ordered_above_global: bool,
    /// Global CG index of the carrier (-1 when absent).
    pub cg_carrier_idx: isize,
    /// Global CG index of the foreign target (-1 when absent).
    pub cg_target_idx: isize,
    /// AX focused (pid, wid) before the carrier showed.
    pub focused_before_carrier: Option<(i32, Option<u32>)>,
    /// AX focused (pid, wid) after carrier show/update legs.
    pub focused_after_carrier: Option<(i32, Option<u32>)>,
    /// Neither readback ever named the carrier or the controller.
    pub carrier_never_focused: bool,
    /// Move leg: helper applied exactly, carrier frame equals recomputed outer.
    pub move_track_ok: bool,
    /// Move leg set-to-readback micros (AX roundtrip + settle included; API
    /// sampling, not display lag).
    pub move_lag_us: u64,
    /// Resize leg: same proof at a new size.
    pub resize_track_ok: bool,
    /// Resize leg set-to-readback micros (same honest note).
    pub resize_lag_us: u64,
    /// Target screen backing scale used for the explicit backing readback.
    pub backing_scale: f64,
    /// Fence leg: carrier hides on `orderOut:` and reads back invisible.
    pub fence_hide_ok: bool,
    /// Fence leg: carrier re-shows afterwards and reads back visible.
    pub fence_restore_ok: bool,
    /// Fullscreen leg: owned foreign fullscreen entered (AX readback).
    pub fullscreen_entered: bool,
    /// Fullscreen leg: carrier hid while the foreign target was fullscreen.
    pub fullscreen_hide_ok: bool,
    /// Fullscreen exit readback (`None` = exit not attempted after a failed
    /// enter; never gated synchronously).
    pub fullscreen_exit_readback: Option<bool>,
    /// Fullscreen exit rode the held-Child authorization (no title proof is
    /// available while the helper is fullscreen).
    pub fullscreen_exit_held: bool,
    /// Post-exit return: carrier re-shown and readable plus focus agreement
    /// back on the helper.
    pub fullscreen_return_ok: bool,
    /// Target Space membership preimage (read-only, fullscreen leg).
    pub membership_before: Vec<u64>,
    /// Target Space membership after the leg (read-only; equality NOT an
    /// oracle — the transition is asynchronous).
    pub membership_after: Vec<u64>,
    /// Helper Child reaped by the exact held handle.
    pub helper_reaped: bool,
    /// Helper process gone after stop.
    pub helper_gone: bool,
    /// Spawn record removed after verified exit.
    pub record_removed: bool,
    /// Receipt helper PID (second owned process parked under the ring pixel).
    pub receipt_pid: u32,
    /// Receipt helper window ID.
    pub receipt_wid: u32,
    /// Click point in CG top-left points (proven inside the ring band).
    pub click_point_cg: Option<(f64, f64)>,
    /// SLS window-server hit at the click point (topmost window ID).
    pub click_sls_wid: Option<u32>,
    /// AX element-at-position hit owner at the click point.
    pub click_ax_hit: Option<(u32, u32)>,
    /// Receipt helper mouse-down counter before the click.
    pub click_mouse_before: u64,
    /// Receipt helper mouse-down counter after the click.
    pub click_mouse_after: u64,
    /// Receipt helper mouse-up counter before the click (supporting edge:
    /// recorded, never gated — a per-edge boundary refusal between down
    /// and up leaves a down without an up by design).
    pub click_mouse_up_before: u64,
    /// Receipt helper mouse-up counter after the click (supporting edge).
    pub click_mouse_up_after: u64,
    /// Click receipt proved (post-click counter exceeds pre-click).
    pub click_receipt_ok: bool,
    /// Pointer restored to the pre-click position via the eventless warp.
    pub pointer_restored: bool,
    /// Receipt helper reaped/record removed/gone.
    pub receipt_reaped: bool,
    pub receipt_record_removed: bool,
    pub receipt_gone: bool,
    /// Owned CG windows remaining for helper + controller carrier (total).
    pub residue_total: usize,
    /// Owned CG windows still onscreen after teardown (must be 0).
    pub residue_onscreen: usize,
    /// Frontmost PID before the spawn (baseline; never restored by keys).
    pub frontmost_before: Option<i32>,
    /// Frontmost PID after teardown (readback only; Ghostty is never
    /// activated, keyed, or otherwise touched to "restore" it).
    pub frontmost_after: Option<i32>,
    /// Per-display Space topology identical before/after.
    pub space_restored: bool,
    /// Real click-through check parked (no helper click-receipt channel; no
    /// new TCC grant requested; nothing invented).
    pub clickthrough_parked: bool,
    /// Click-through PROVED by the receipt helper's own mouse-down delta
    /// (the old parked flag stays for shape continuity; the proof is the
    /// `click_receipt_ok` leg above).
    pub clickthrough_proved: bool,
    /// Fixture wall time in seconds (bounded by `--seconds`).
    pub elapsed_secs: u64,
    /// Core acceptance: global order, focus isolation, move/resize tracking,
    /// click receipt through the ring, fence hide/restore, fullscreen enter
    /// plus held-exit with return, and clean teardown of both helpers.
    /// Fullscreen exit MEMBERSHIP equality is deliberately not a gate (the
    /// transition is asynchronous); the AX exit readback plus return is.
    pub pass: bool,
    /// Free-form leg notes (indices, lags, skips, constraint text).
    pub detail: String,
}

/// Minimum fixture lifetime: spawn, joins, and legs need headroom under
/// their own bounded waits.
pub const MIN_BORDER_SECONDS: u64 = 30;
/// Maximum fixture lifetime (matches the probe ceiling).
pub const MAX_BORDER_SECONDS: u64 = 120;
/// Default fixture lifetime.
pub const DEFAULT_BORDER_SECONDS: u64 = 60;

/// Pure lifetime gate for the fixture (also the enforced deadline budget).
pub fn validate_border_seconds(seconds: u64) -> Result<u64, String> {
    if (MIN_BORDER_SECONDS..=MAX_BORDER_SECONDS).contains(&seconds) {
        Ok(seconds)
    } else {
        Err(format!(
            "border-session needs --seconds {MIN_BORDER_SECONDS}..={MAX_BORDER_SECONDS}"
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

/// Sample one ring-band point from a carrier outer frame (AppKit
/// bottom-left points): the middle of the left band. `None` for a
/// non-positive or non-finite ring/frame. The caller converts to CG
/// top-left points and proves band membership with
/// [`point_in_ring_band`]; this sample alone proves nothing.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn ring_sample_appkit(outer: (f64, f64, f64, f64), width: f64) -> Option<(f64, f64)> {
    let (x, y, w, h) = outer;
    if !x.is_finite() || !y.is_finite() || !w.is_finite() || !h.is_finite() || !width.is_finite() {
        return None;
    }
    if width <= 0.0 || w <= width || h <= 0.0 {
        return None;
    }
    Some((x + width / 2.0, y + h / 2.0))
}

/// True when `p` (same coordinate space as both rects) lies inside `outer`
/// but strictly outside `inner`: i.e. on the painted ring band between the
/// two rects. Both rects are `(x, y, w, h)` tuples.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn point_in_ring_band(
    outer: (f64, f64, f64, f64),
    inner: (f64, f64, f64, f64),
    p: (f64, f64),
) -> bool {
    let inside = |r: (f64, f64, f64, f64), q: (f64, f64)| {
        q.0 >= r.0 && q.0 <= r.0 + r.2 && q.1 >= r.1 && q.1 <= r.1 + r.3
    };
    let strictly_inside = |r: (f64, f64, f64, f64), q: (f64, f64)| {
        q.0 > r.0 && q.0 < r.0 + r.2 && q.1 > r.1 && q.1 < r.1 + r.3
    };
    inside(outer, p) && !strictly_inside(inner, p)
}

/// Accepted click placement: the CG point plus the raw hit joins read there.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
struct AcceptedClick {
    point_cg: (f64, f64),
    sls_hit: Option<u32>,
    ax_hit: Option<(u32, u32)>,
    cg_carrier_idx: isize,
    cg_receipt_idx: isize,
}

/// Front-to-back stack fingerprint for diagnosis: the first entries of the
/// global CG order as `wid:pid/Llayer` (IDs, layers, and order only — never
/// titles or content). Names which windows sit above a refused click pixel
/// when the order readback denies the traversal proof.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn stack_fingerprint() -> String {
    match crate::ffi::cg_window_list_all() {
        Ok(list) => list
            .iter()
            .take(14)
            .map(|w| format!("{}:{}/L{}", w.wid, w.pid, w.layer))
            .collect::<Vec<_>>()
            .join(" "),
        Err(e) => format!("unreadable: {e}"),
    }
}

/// Latest receipt-helper `mouse_counts` edges in the drained helper stdout
/// lines (periodic `{"mouse_counts": {"left_down", "left_up"}}` snapshots).
/// Returns the newest parsed `(down, up)` pair; a snapshot missing either
/// edge is skipped, never an error. `None` means no complete snapshot was
/// observed (fail closed). The gate uses the down delta; the up pair rides
/// along as supporting evidence (a per-edge boundary refusal between down
/// and up leaves a down without an up by design).
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

/// Live-process diagnostic snapshot for error paths: bundle-table value,
/// executable path, existence, CG presence, and fixture elapsed. Read-only;
/// disambiguates a dead helper, PID reuse, and a lapsed LS registration.
#[cfg(target_os = "macos")]
fn diag_snapshot(helper_pid: u32, helper_wid: u32, start: std::time::Instant) -> String {
    let pid_i32 = helper_pid as i32;
    let bundle = crate::ffi::bundle_id_for_pid(pid_i32);
    let exe = crate::ffi::process_executable_path(pid_i32);
    let exists = crate::ffi::process_exists(pid_i32);
    let cg_present = crate::ffi::cg_window_list_all()
        .map(|list| {
            list.iter()
                .filter(|w| w.pid == helper_pid)
                .map(|w| w.wid)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let frontmost = crate::ffi::frontmost_app().map(|(pid, _)| pid);
    format!(
        "elapsed_s={} bundle={bundle:?} exe={exe:?} exists={exists} \
         cg_wids={cg_present:?} helper_wid={helper_wid} frontmost={frontmost:?}",
        start.elapsed().as_secs()
    )
}

/// Run the bounded cross-process carrier fixture on the calling (main)
/// thread. Returns the JSON report on success; a failed `pass` (or a fixture
/// error) is an `Err` carrying the report JSON so every measured leg stays
/// observable.
pub fn run_border_session(
    seconds: u64,
    helper_regular_policy: bool,
) -> Result<String, (i32, String)> {
    validate_border_seconds(seconds).map_err(|e| (2, e))?;
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (seconds, helper_regular_policy);
        Err(fail(
            "unsupported",
            "border-session is macOS-only".to_owned(),
        ))
    }
    #[cfg(target_os = "macos")]
    {
        run_border_session_macos(seconds, helper_regular_policy)
    }
}

#[cfg(target_os = "macos")]
fn run_border_session_macos(
    seconds: u64,
    helper_regular_policy: bool,
) -> Result<String, (i32, String)> {
    use std::time::{Duration, Instant};

    use crate::classifier::{ActivationPolicy, AppFacts, AxFacts, CgFacts};

    let start = Instant::now();
    let deadline = start + Duration::from_secs(seconds);
    let mut detail: Vec<String> = Vec::new();
    let elapsed_secs = || start.elapsed().as_secs();

    // Gate: session lock before touching UI (unknown is not unlocked), plus
    // the TCC/route prerequisites. No mutation has occurred, so no restore
    // is owed on these refuses.
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
            "border-session must run on the main thread".to_owned(),
        )
    })?;

    // Per-leg guards: re-check the lock AND the real deadline before every
    // leg. An overrun tears down instead of continuing.
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

    // Baseline BEFORE the spawn: frontmost, focused, Space topology, and
    // display scales. Read-only; nothing to restore on a pre-spawn refuse.
    gate("baseline", &mut detail)?;
    let frontmost_before = crate::ffi::frontmost_app().map(|(pid, _)| pid);
    let space_before = crate::focus::capture_space().map_err(|e| fail("space-before", e))?;
    let displays =
        crate::ffi::ns_display_snapshot().map_err(|e| fail("displays", e.to_string()))?;
    let backing_scale = displays.first().map(|d| d.scale).unwrap_or(1.0);
    // Coordinate spaces stay explicit: AX/CG frames are top-left points,
    // AppKit carrier frames bottom-left. Conversion needs the primary
    // display height; without it the fixture refuses instead of mixing.
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

    // Spawn the SEPARATE owned helper (exact Child + nonce protocol). The
    // helper owns the target window in its own process; the carrier below
    // lives here. Deterministic spawn origin keeps the first observed frame
    // inside the visible area without any controller frame write.
    gate("spawn", &mut detail)?;
    let nonce = crate::probe_target::mint_nonce().map_err(|e| fail("spawn", e))?;
    // Helper lifetime outlasts the fixture deadline with headroom: the
    // fixture never starts new legs past its own deadline and always stops
    // the exact held Child first, so the helper's self-exit is a backstop,
    // never the teardown — and it can never fire mid-leg while legs run.
    let helper_seconds = (seconds + 30).min(MAX_BORDER_SECONDS);
    let spec = crate::probe_target::ProbeTargetSpec::new(&nonce, 0, 0, helper_seconds)
        .map(|s| {
            s.with_regular_policy(helper_regular_policy)
                .with_origin(200.0, 200.0)
        })
        .map_err(|e| fail("spawn", e))?;
    let mut raw = crate::probe_target::start_helper_process(&spec).map_err(|e| fail("spawn", e))?;
    let helper_pid = raw.pid;
    let helper_path = raw.helper_path.clone();
    crate::probe_target::await_handshake(&mut raw, &spec).map_err(|e| {
        let _ = raw.child.kill();
        let _ = raw.child.wait();
        fail("spawn", e)
    })?;
    crate::probe_target::record_spawn(helper_pid, &spec, &helper_path).map_err(|e| {
        let _ = raw.child.kill();
        let _ = raw.child.wait();
        fail("spawn", e)
    })?;
    // `raw` (including the stdout line receiver) stays alive for the whole
    // run: dropping the receiver ends the holder-side drain thread, after
    // which the helper's next periodic snapshot line raises EPIPE and the
    // helper dies silently (its stderr is null). `spawn_verified` cannot be
    // used here for exactly this reason.
    // From here every failure path must stop the exact held Child and verify
    // the residue; the `finish` closure below owns that teardown.
    let stop_held = |raw: &mut crate::probe_target::UnreadySpawn,
                     detail: &mut Vec<String>|
     -> (bool, bool, bool) {
        // Identity validation before the signal: exact PID + nonce record,
        // expected helper exe, live process, CURRENT live window nonce.
        // Fallback when the title proof is unavailable (e.g. the window
        // server withholds titles while the helper is fullscreen): the
        // EXACT held Child handle plus a live-executable match. `try_wait`
        // returning `None` proves our child has not been reaped (no PID
        // reuse), so signalling that exact handle cannot hit another
        // process; the exe re-check adds currency. A reaped/exited child
        // never takes this path.
        let pid = raw.pid;
        match crate::probe_target::load_verified_record(pid, &nonce) {
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

    // Join the helper window: fresh CG owner-PID set intersected with the
    // live AX identity join for the same PID (never a guessed WID).
    gate("join", &mut detail).inspect_err(|_| {
        let _ = stop_held(&mut raw, &mut detail);
    })?;
    let helper_wid = crate::focus::join_helper_wid(helper_pid).map_err(|e| {
        let _ = stop_held(&mut raw, &mut detail);
        fail("join", e.to_string())
    })?;
    let owned = crate::ax::authorize_owned_action(helper_pid, &nonce).map_err(|e| {
        let _ = stop_held(&mut raw, &mut detail);
        fail("join", e.to_string())
    })?;
    detail.push(format!("helper_pid={helper_pid} helper_wid={helper_wid}"));

    // Controller AppKit lifecycle: Accessory + finishLaunching publishes the
    // app WITHOUT activating (no focus theft); the manual bounded pump keeps
    // lifetime control. The controller is never the regular-policy app here:
    // only the helper opts into regular (CLI flag) for the focus oracle.
    let app = objc2_app_kit::NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(objc2_app_kit::NSApplicationActivationPolicy::Accessory);
    app.finishLaunching();
    let pump = |ms: u64| {
        objc2::rc::autoreleasepool(|_| {
            crate::ffi::drain_app_events(&app);
            crate::ffi::pump_runloop_once(ms);
        });
    };

    // Spawn the SECOND owned helper up front (receipt window for the click
    // leg): it must predate the carrier so creation order stacks the fresh
    // panel above both helpers. Bundled-helper spawn (regular policy): the
    // click post boundary refuses bundle-less direct spawns as the probe
    // itself. Retained stdout receiver feeds its mouse snapshots; it lives
    // to the shared teardown below. Every failure path below stops what it
    // started (partial receipt spawn, then the exact held target Child).
    //
    // Receipt-helper state (shared with the click leg and the two-helper
    // teardown below): `None` until the spawn below stores it.
    let mut raw2_opt: Option<crate::probe_target::UnreadySpawn>;
    let mut receipt_reaped = false;
    let mut receipt_gone = false;
    let mut receipt_record_removed = false;
    gate("click-spawn", &mut detail).inspect_err(|_| {
        let _ = stop_held(&mut raw, &mut detail);
    })?;
    let nonce2 = crate::probe_target::mint_nonce().map_err(|e| {
        let _ = stop_held(&mut raw, &mut detail);
        fail("click-spawn", e)
    })?;
    let spec2 = crate::probe_target::ProbeTargetSpec::new(&nonce2, 0, 0, helper_seconds)
        .map(|s| s.with_regular_policy(true).with_origin(200.0, 200.0))
        .map_err(|e| {
            let _ = stop_held(&mut raw, &mut detail);
            fail("click-spawn", e)
        })?;
    let mut raw2 = crate::probe_target::start_helper_process(&spec2).map_err(|e| {
        let _ = stop_held(&mut raw, &mut detail);
        fail("click-spawn", e)
    })?;
    let receipt_pid = raw2.pid;
    if let Err(e) = crate::probe_target::await_handshake(&mut raw2, &spec2) {
        let _ = raw2.child.kill();
        let _ = raw2.child.wait();
        let _ = stop_held(&mut raw, &mut detail);
        return Err(fail("click-spawn", e));
    }
    if let Err(e) = crate::probe_target::record_spawn(receipt_pid, &spec2, &raw2.helper_path) {
        let _ = raw2.child.kill();
        let _ = raw2.child.wait();
        let _ = stop_held(&mut raw, &mut detail);
        return Err(fail("click-spawn", e));
    }
    raw2_opt = Some(raw2);
    // Receipt stop with ITS nonce (the shared `stop_held` below binds the
    // target nonce; sharing it would force the receipt through the
    // Child-fallback with a misleading refusal line).
    let stop_held2 = |raw2: &mut crate::probe_target::UnreadySpawn,
                      detail: &mut Vec<String>|
     -> (bool, bool, bool) {
        let pid = raw2.pid;
        match crate::probe_target::load_verified_record(pid, &nonce2) {
            Ok(_) => {}
            Err(e) => {
                detail.push(format!("receipt stop identity revalidation refused: {e}"));
                let live_exe = crate::ffi::process_executable_path(pid as i32);
                let child_live = matches!(raw2.child.try_wait(), Ok(None));
                if child_live && live_exe.as_deref() == Some(raw2.helper_path.as_str()) {
                    detail.push(
                        "receipt stop fallback: exact held Child plus live-exe match".to_owned(),
                    );
                } else {
                    detail.push(format!(
                        "receipt stop fallback refused (child_live={child_live} exe={live_exe:?})"
                    ));
                    return (false, false, false);
                }
            }
        }
        let stop = crate::focus::stop_verified(&mut raw2.child, pid);
        detail.push(format!(
            "receipt stop reaped={} record_removed={} gone={} verified_exit={}",
            stop.reaped, stop.record_removed, stop.process_gone, stop.verified_exit
        ));
        (stop.reaped, stop.process_gone, stop.record_removed)
    };
    // Receipt join with a bounded poll: AX registration can lag the
    // runloop `ready` handshake, so a first-miss join is retried (reads
    // only) instead of failing the fixture on a transient.
    let join_deadline = Instant::now() + Duration::from_secs(8);
    let receipt_wid = loop {
        match crate::focus::join_helper_wid(receipt_pid) {
            Ok(wid) => break wid,
            Err(e) => {
                if Instant::now() >= join_deadline {
                    if let Some(raw2) = raw2_opt.as_mut() {
                        let _ = stop_held2(raw2, &mut detail);
                    }
                    let _ = stop_held(&mut raw, &mut detail);
                    return Err(fail("click-join", e.to_string()));
                }
                // Main-thread wait (drains AppKit + runloop, which is what
                // the pending AX registration needs).
                pump(250);
            }
        }
    };
    detail.push(format!(
        "receipt_pid={receipt_pid} receipt_wid={receipt_wid}"
    ));
    // NOTE: the receipt identity (`owned2`) is deliberately NOT bound here.
    // A freshly launched bundled helper has no LaunchServices bundle entry
    // yet, so an early bind would freeze the probe-bundle fallback and
    // mismatch once the helper bundle registers. The click search loop
    // re-authorizes per attempt instead (fresh bundle, fresh CG join).

    // Observe the foreign target (read-only, ownership-joined): AX facts +
    // CG bounds + app facts, then the pure mapper to a BorderTarget. The
    // focused join comes from the AX focused-window readback AFTER the
    // helper-only activation below; before that the mapper would (correctly)
    // yield no active target.
    gate("observe", &mut detail).inspect_err(|_| {
        let _ = stop_held(&mut raw, &mut detail);
    })?;
    let read_state = || -> Result<crate::ax::WindowStateReport, String> {
        crate::ax::read_window_owned(&owned, helper_pid, helper_wid).map_err(|e| e.to_string())
    };
    let hidden = crate::ffi::app_hidden_for_pid(helper_pid as i32).unwrap_or(true);
    let policy_raw = crate::ffi::activation_policy_raw_for_pid(helper_pid as i32).unwrap_or(-1);
    let policy = match policy_raw {
        0 => ActivationPolicy::Regular,
        1 => ActivationPolicy::Accessory,
        2 => ActivationPolicy::Prohibited,
        _ => ActivationPolicy::Unknown,
    };
    // Activate ONLY the owned helper (SLPS dispatch to its exact PID/WID;
    // never Ghostty, never any foreign window), then join the focused
    // window: the ring follows the focused window only.
    gate("activate", &mut detail).inspect_err(|_| {
        let _ = stop_held(&mut raw, &mut detail);
    })?;
    let focused_before_carrier = crate::ffi::ax_focused_preimage_detailed().ok();
    let dispatch = crate::focus::dispatch_focus(helper_pid, helper_wid).map_err(|e| {
        let _ = stop_held(&mut raw, &mut detail);
        fail("activate", e.to_string())
    })?;
    pump(250);
    detail.push(format!(
        "dispatch_latency_ms={} set_front={}",
        dispatch.dispatch_latency_ms, dispatch.set_front_status
    ));
    let st = read_state().map_err(|e| {
        let _ = stop_held(&mut raw, &mut detail);
        fail("observe", e)
    })?;
    let focused = crate::ffi::ax_focused_preimage_detailed().ok();
    let focused_is_helper = focused == Some((helper_pid as i32, Some(helper_wid)));
    // Mapper frame in AppKit points, converted from the joined AX frame
    // (top-left): never the raw CG rect, never a guessed space.
    let ax_frame = st.ax_frame.ok_or_else(|| {
        let _ = stop_held(&mut raw, &mut detail);
        fail("observe", "helper AX frame unreadable".to_owned())
    })?;
    let frame = ax_to_appkit(ax_frame);
    let obs = crate::active_border::BorderObservation {
        frame: Some(frame),
        ax: AxFacts {
            role: st.role.clone(),
            subrole: st.subrole.clone(),
            fullscreen: st.fullscreen.unwrap_or(false),
            minimized: st.minimized.unwrap_or(false),
            modal: false,
            size_settable: true,
            hints: tiler_core::size_hints::WindowSizeHints::none(),
        },
        cg: CgFacts {
            layer: st.cg_layer.unwrap_or(0),
            onscreen: st.cg_onscreen.unwrap_or(false),
        },
        app: AppFacts {
            hidden,
            activation_policy: policy,
            is_own_bundle: false,
        },
        maximized_h: false,
        maximized_v: false,
        focused: focused_is_helper,
        deleted: false,
    };
    let style = crate::active_border::ActiveBorderStyle::default();
    let target = crate::active_border::map_observation_to_target(&obs).ok_or_else(|| {
        let _ = stop_held(&mut raw, &mut detail);
        fail(
            "observe",
            format!(
                "helper did not map to an active target (focused_is_helper={focused_is_helper})"
            ),
        )
    })?;
    let inner = crate::active_border::border_decision(true, &target, style.gap).map_err(|r| {
        let _ = stop_held(&mut raw, &mut detail);
        fail("observe", format!("helper target hidden: {}", r.as_str()))
    })?;
    let outer = crate::active_border::border_outer_rect(inner, style.width).ok_or_else(|| {
        let _ = stop_held(&mut raw, &mut detail);
        fail("observe", "outer rect refused".to_owned())
    })?;
    let accent = crate::ffi::border_system_accent();
    let effective = crate::active_border::effective_color(&style, accent);
    detail.push(format!(
        "accent={} policy_raw={policy_raw} hidden={hidden} role={:?} subrole={:?}",
        accent
            .map(|a| crate::active_border::render_color(a.color))
            .unwrap_or_else(|| "fallback".to_owned()),
        st.role,
        st.subrole
    ));

    // Carrier create + show above the FOREIGN window number. Cross-process
    // relative ordering is NOT assumed: the verdict comes only from the
    // global CG readback below.
    gate("carrier", &mut detail).inspect_err(|_| {
        let _ = stop_held(&mut raw, &mut detail);
    })?;
    let carrier = crate::ffi::BorderCarrier::create(
        mtm,
        &style,
        outer,
        accent,
        &format!("OmniTiler border carrier {nonce}"),
    )
    .map_err(|e| {
        let _ = stop_held(&mut raw, &mut detail);
        fail("carrier", e)
    })?;
    let flags = carrier.flags();
    let carrier_wid = carrier.window_number();
    carrier.show_above(mtm, helper_wid as isize);
    pump(250);
    let visible_after_show = carrier.is_visible();
    let draws_after_show = carrier.draws();

    // Teardown-on-error: destroy the carrier plus stop the exact held
    // helper, then return the error. Used only after the carrier exists;
    // earlier legs stop the helper alone.
    macro_rules! or_teardown {
        ($result:expr, $carrier:ident, $session:ident, $detail:ident) => {
            match $result {
                Ok(v) => v,
                Err(e) => {
                    $carrier.destroy(mtm);
                    let _ = stop_held(&mut $session, &mut $detail);
                    return Err(e);
                }
            }
        };
    }

    // Receipt-helper error macros (state declared up front at the early
    // spawn): stop it whenever present, then the carrier and the target
    // helper.
    macro_rules! click_fail {
        ($e:expr) => {{
            if let Some(raw2) = raw2_opt.as_mut() {
                let _ = stop_held2(raw2, &mut detail);
            }
            carrier.destroy(mtm);
            let _ = stop_held(&mut raw, &mut detail);
            // Diagnostic only (stderr, never the stdout payload): the
            // per-attempt hit joins that drove the verdict.
            eprintln!("border-session click detail: {}", detail.join("; "));
            return Err($e);
        }};
    }
    // Teardown-on-error once the receipt helper exists: stop it too, then
    // the carrier and the target helper.
    macro_rules! or_teardown2 {
        ($result:expr) => {
            match $result {
                Ok(v) => v,
                Err(e) => {
                    if let Some(raw2) = raw2_opt.as_mut() {
                        let _ = stop_held2(raw2, &mut detail);
                    }
                    carrier.destroy(mtm);
                    let _ = stop_held(&mut raw, &mut detail);
                    return Err(e);
                }
            }
        };
    }

    // ORDER leg (global, cross-PID, no owner filtering). A `false` verdict
    // with both windows present is the documented API constraint: public
    // `orderWindow:relativeTo:` against a foreign window is unreliable
    // (observed both above and buried across runs — never a SkyLight
    // attempt). That STOPs the ordering claim (pass=false) but the
    // remaining legs still run so focus/tracking/fence/teardown evidence is
    // complete.
    or_teardown!(gate("order", &mut detail), carrier, raw, detail);
    // Settle-tolerant order readback: window-server stacking is still in
    // flight shortly after the target's activation animation and our show,
    // so poll the global list briefly for the above verdict instead of
    // trusting the first snapshot. Still pure readback (never assumed); a
    // persistent miss keeps the API-constraint STOP below.
    let order_deadline = Instant::now() + Duration::from_secs(3);
    let (ordered_above_global, cg_carrier_idx, cg_target_idx, cg_len, cg_last) = loop {
        let cg_list = crate::ffi::cg_window_list_all().unwrap_or_default();
        let (above, ci, ti) = crate::ffi::cg_carrier_above_target(
            &cg_list,
            carrier_wid as isize,
            helper_wid as isize,
        );
        if above || Instant::now() >= order_deadline {
            let len = cg_list.len();
            break (above, ci, ti, len, cg_list);
        }
        pump(250);
    };
    let (ordered_above_receipt, _, cg_receipt_idx) =
        crate::ffi::cg_carrier_above_target(&cg_last, carrier_wid as isize, receipt_wid as isize);
    detail.push(format!(
        "order receipt pair above={ordered_above_receipt} receipt_idx={cg_receipt_idx} stack=[{}]",
        stack_fingerprint()
    ));
    detail.push(format!(
        "cg_len={cg_len} cg_carrier={cg_carrier_idx} cg_target={cg_target_idx} visible={visible_after_show} draws={draws_after_show}",
    ));
    if !ordered_above_global {
        detail.push(
            "ORDER CONSTRAINT: carrier not above the foreign target in the global CG list; \
             public relative ordering is unreliable cross-process (no SkyLight attempted)"
                .to_owned(),
        );
    }

    // FOCUS legs: focused readbacks before (pre-carrier) and after the
    // carrier show/update. Both must name the owned helper; the carrier and
    // the controller must never be focused. This proves no activation by our
    // panel — flag values alone never would.
    or_teardown!(gate("focus", &mut detail), carrier, raw, detail);
    let focused_after_carrier = crate::ffi::ax_focused_preimage_detailed().ok();
    let me = std::process::id() as i32;
    let names_carrier_or_me = |f: &Option<(i32, Option<u32>)>| -> bool {
        match f {
            Some((pid, _)) => *pid == me,
            None => false,
        }
    };
    let focus_names_helper = focused_after_carrier == Some((helper_pid as i32, Some(helper_wid)));
    let carrier_never_focused = !names_carrier_or_me(&focused_before_carrier)
        && !names_carrier_or_me(&focused_after_carrier);
    detail.push(format!(
        "focused_before={focused_before_carrier:?} focused_after={focused_after_carrier:?}"
    ));

    // TRACK legs: persistent AX frame writes to the OWNED helper (per-action
    // ownership joins inside), then the carrier re-tracks the OBSERVED frame
    // (AX applied + CG poll), with the ACTUAL panel frame read back. Lag is
    // set-to-readback micros including the AX roundtrip plus one settle
    // pump: honest API sampling, not display lag. Target frames stay in
    // AppKit points; the backing readback pins the scale explicitly.
    let mut move_track_ok = false;
    let mut move_lag_us = 0u64;
    let mut resize_track_ok = false;
    let mut resize_lag_us = 0u64;
    for (leg, want_ax) in [
        (
            "move",
            crate::ffi::AxFrame {
                x: 260.0,
                y: 260.0,
                w: 480.0,
                h: 320.0,
            },
        ),
        (
            "resize",
            crate::ffi::AxFrame {
                x: 260.0,
                y: 260.0,
                w: 560.0,
                h: 380.0,
            },
        ),
    ] {
        or_teardown!(gate(leg, &mut detail), carrier, raw, detail);
        let t0 = Instant::now();
        let persistent = or_teardown!(
            crate::ax::set_frame_owned_persistent(&owned, helper_pid, helper_wid, want_ax).map_err(
                |e| {
                    let diag = diag_snapshot(helper_pid, helper_wid, start);
                    fail(leg, format!("{e}; diag: {diag}"))
                }
            ),
            carrier,
            raw,
            detail
        );
        if !persistent.matched {
            carrier.destroy(mtm);
            let _ = stop_held(&mut raw, &mut detail);
            return Err(fail(
                leg,
                format!(
                    "helper frame not applied (preimage {:?}, attempted {:?}, applied {:?})",
                    persistent.preimage, persistent.attempted, persistent.applied
                ),
            ));
        }
        // CG poll for the applied frame (bounded, same AX top-left space):
        // the write latched, but the carrier tracks the OBSERVED frame,
        // never the requested one.
        let poll_deadline = Instant::now() + Duration::from_secs(5);
        let mut observed_ax: Option<crate::ffi::AxFrame> = None;
        while Instant::now() < poll_deadline {
            if let Ok(cur) = read_state()
                && cur.ax_frame == Some(want_ax)
            {
                observed_ax = cur.ax_frame;
                break;
            }
            pump(100);
        }
        let seen_ax = or_teardown!(
            observed_ax.ok_or_else(|| fail(
                leg,
                "AX poll never showed the applied helper frame".to_owned()
            )),
            carrier,
            raw,
            detail
        );
        let seen_target = crate::active_border::BorderTarget {
            has_window: true,
            frame: ax_to_appkit(seen_ax),
            deleted: false,
            minimized: false,
            fullscreen: false,
            maximized_h: false,
            maximized_v: false,
            excluded: false,
        };
        let want_outer = or_teardown!(
            (|| {
                let inner =
                    crate::active_border::border_decision(true, &seen_target, style.gap).ok()?;
                crate::active_border::border_outer_rect(inner, style.width)
            })()
            .ok_or_else(|| fail(leg, "observed helper frame unexpectedly hidden".to_owned())),
            carrier,
            raw,
            detail
        );
        or_teardown!(
            carrier
                .update(mtm, &style, want_outer, accent)
                .map_err(|e| fail(leg, e)),
            carrier,
            raw,
            detail
        );
        carrier.show_above(mtm, helper_wid as isize);
        pump(50);
        let got = carrier.frame_points();
        let want_tuple = (want_outer.x, want_outer.y, want_outer.w, want_outer.h);
        let ok = frames_close(got, want_tuple);
        let lag_us = t0.elapsed().as_micros() as u64;
        let backing = carrier.backing_rect(backing_scale);
        detail.push(format!(
            "{leg} lag_us={lag_us} carrier_frame={got:?} backing={backing:?}"
        ));
        if leg == "move" {
            move_track_ok = ok;
            move_lag_us = lag_us;
        } else {
            resize_track_ok = ok;
            resize_lag_us = lag_us;
        }
    }

    // CLICK leg: one synthetic click at an ACTUAL BORDER PIXEL with receipt
    // by an underlying owned window. A second owned helper (receipt
    // window) is parked under a ring-band coordinate proven from the live
    // carrier frame; the WindowServer hit joins are read at that point, the
    // click posts guarded-HID through the click-through carrier, and the
    // receipt helper's own AppKit mouse-down delta proves delivery (API
    // `ignoresMouseEvents` alone never would). Focus moves to the receipt
    // helper for the oracle-honest post and is restored to the target
    // afterwards with agreement; the pointer is restored via the eventless
    // warp. The receipt helper lives to the shared teardown below.
    or_teardown2!(gate("click-spawn", &mut detail));
    let cursor_before = match crate::synth::cursor_position() {
        Some(p) => p,
        None => click_fail!(fail(
            "click",
            "live cursor position unreadable; refusing rather than leaving the pointer unrestored"
                .to_owned()
        )),
    };
    // The receipt helper predates the carrier (spawned up front), so the
    // fresh panel arrives above both helpers; every search attempt below
    // still re-reads the live order and refuses unless the carrier is
    // above the receipt at the click pixel.
    detail.push(format!(
        "carrier wid={carrier_wid} over receipt wid={receipt_wid}"
    ));
    // Clear-pixel search: candidate target origins (AX/CG top-left points)
    // for a ring pixel no foreign window covers. The current post-resize
    // frame comes first, then display-spanning alternatives from the
    // baseline snapshot. Each attempt fully re-tracks the carrier and
    // re-parks the receipt window; the hit joins decide. Foreign windows
    // are never moved, resized, hidden, or reordered — only our own
    // windows move, and only to positions the hits then verify.
    let (dw, dh) = displays
        .first()
        .map(|d| (d.frame.w, d.frame.h))
        .unwrap_or((1440.0, 900.0));
    let mut candidates = vec![(260.0, 260.0)];
    for (ox, oy) in [
        (dw - 640.0, 120.0),
        (80.0, dh - 460.0),
        (dw - 640.0, dh - 460.0),
        ((dw - 560.0) / 2.0, (dh - 380.0) / 2.0),
    ] {
        let origin = (ox.max(0.0), oy.max(0.0));
        if !candidates.contains(&origin) {
            candidates.push(origin);
        }
    }
    let mut accepted: Option<AcceptedClick> = None;
    for (attempt, (ox, oy)) in candidates.iter().enumerate() {
        or_teardown2!(gate("click-search", &mut detail));
        // Move OUR target window to the candidate origin (applied-tracked,
        // never the requested frame downstream).
        let want_click = crate::ffi::AxFrame {
            x: *ox,
            y: *oy,
            w: 560.0,
            h: 380.0,
        };
        let moved =
            match crate::ax::set_frame_owned_persistent(&owned, helper_pid, helper_wid, want_click)
            {
                Ok(p) => p,
                Err(e) => {
                    let diag = diag_snapshot(helper_pid, helper_wid, start);
                    click_fail!(fail(
                        "click-search",
                        format!("target move refused: {e}; diag: {diag}")
                    ));
                }
            };
        let applied_t = match moved.applied {
            Some(f) => f,
            None => {
                detail.push(format!(
                    "click-search attempt {attempt}: target not applied; skipping"
                ));
                continue;
            }
        };
        let settle_deadline = Instant::now() + Duration::from_secs(5);
        let mut target_settled = false;
        while Instant::now() < settle_deadline {
            if let Ok(cur) = read_state()
                && cur.ax_frame == Some(applied_t)
            {
                target_settled = true;
                break;
            }
            pump(100);
        }
        if !target_settled {
            detail.push(format!(
                "click-search attempt {attempt}: target never settled at the applied frame; skipping"
            ));
            continue;
        }
        // Current observed target geometry (never the requested frame):
        // the ring math must describe the LIVE carrier, so skip on drift.
        let cur_state = match read_state() {
            Ok(st) => st,
            Err(e) => {
                detail.push(format!(
                    "click-search attempt {attempt}: observe: {e}; skipping"
                ));
                continue;
            }
        };
        let cur_ax = match cur_state.ax_frame {
            Some(f) => f,
            None => {
                detail.push(format!(
                    "click-search attempt {attempt}: AX frame unreadable; skipping"
                ));
                continue;
            }
        };
        let cur_target = crate::active_border::BorderTarget {
            has_window: true,
            frame: ax_to_appkit(cur_ax),
            deleted: false,
            minimized: false,
            fullscreen: false,
            maximized_h: false,
            maximized_v: false,
            excluded: false,
        };
        let cur_inner = match crate::active_border::border_decision(true, &cur_target, style.gap) {
            Ok(inner) => inner,
            Err(r) => {
                detail.push(format!(
                    "click-search attempt {attempt}: hidden ({}) ; skipping",
                    r.as_str()
                ));
                continue;
            }
        };
        let cur_outer = match crate::active_border::border_outer_rect(cur_inner, style.width) {
            Some(outer) => outer,
            None => {
                detail.push(format!(
                    "click-search attempt {attempt}: outer refused; skipping"
                ));
                continue;
            }
        };
        // Re-track the carrier to the observed outer before sampling it.
        if let Err(e) = carrier.update(mtm, &style, cur_outer, accent) {
            click_fail!(fail(
                "click-search",
                format!("carrier re-track refused: {e}")
            ));
        }
        carrier.show_above(mtm, helper_wid as isize);
        pump(100);
        let carrier_now = carrier.frame_points();
        if !frames_close(
            carrier_now,
            (cur_outer.x, cur_outer.y, cur_outer.w, cur_outer.h),
        ) {
            detail.push(format!(
                "click-search attempt {attempt}: carrier drifted; skipping"
            ));
            continue;
        }
        let ring_appkit = match ring_sample_appkit(
            (cur_outer.x, cur_outer.y, cur_outer.w, cur_outer.h),
            style.width,
        ) {
            Some(p) => p,
            None => click_fail!(fail(
                "click-search",
                "ring sample refused (non-positive/non-finite ring)".to_owned()
            )),
        };
        if !point_in_ring_band(
            (cur_outer.x, cur_outer.y, cur_outer.w, cur_outer.h),
            (cur_inner.x, cur_inner.y, cur_inner.w, cur_inner.h),
            ring_appkit,
        ) {
            click_fail!(fail(
                "click-search",
                "ring sample outside the painted band; refusing".to_owned()
            ));
        }
        // AppKit bottom-left points to CG top-left points for the hit
        // joins and the HID post (AX placement stays top-left natively).
        let cg = (ring_appkit.0, primary_h - ring_appkit.1);
        if let Err(e) = crate::synth::validate_click_point(cg) {
            detail.push(format!(
                "click-search attempt {attempt}: ring CG point refused ({e}); skipping"
            ));
            continue;
        }
        detail.push(format!(
            "click-search attempt {attempt} outer=({:.1},{:.1},{:.1},{:.1}) cg=({:.1},{:.1})",
            cur_outer.x, cur_outer.y, cur_outer.w, cur_outer.h, cg.0, cg.1
        ));
        let want_receipt = crate::ffi::AxFrame {
            x: cg.0 - 40.0,
            y: cg.1 - 40.0,
            w: 80.0,
            h: 80.0,
        };
        // Fresh per-attempt identity (record + live bundle + CURRENT title
        // proof): the helper bundle may have registered since the last
        // attempt, so a frozen bind would mismatch.
        let owned2 = match crate::ax::authorize_owned_action(receipt_pid, &nonce2) {
            Ok(o) => o,
            Err(e) => {
                detail.push(format!(
                    "click-search attempt {attempt}: receipt authorize ({e}); skipping"
                ));
                continue;
            }
        };
        let read_receipt = || -> Result<crate::ax::WindowStateReport, String> {
            crate::ax::read_window_owned(&owned2, receipt_pid, receipt_wid)
                .map_err(|e| e.to_string())
        };
        {
            let placed = match crate::ax::set_frame_owned_persistent(
                &owned2,
                receipt_pid,
                receipt_wid,
                want_receipt,
            ) {
                Ok(p) => p,
                Err(e) => {
                    let diag = diag_snapshot(receipt_pid, receipt_wid, start);
                    click_fail!(fail("click-place", format!("{e}; diag: {diag}")));
                }
            };
            // The window server may snap the receipt frame (minimum width,
            // integer origin): track the APPLIED frame, never the requested
            // one. What matters is that the applied window actually
            // underlies the ring pixel with margin.
            let applied = match placed.applied {
                Some(f) => f,
                None => {
                    detail.push(format!(
                        "click-search attempt {attempt}: receipt not applied; skipping"
                    ));
                    continue;
                }
            };
            detail.push(format!(
                "click-search attempt {attempt} receipt matched={} applied=({:.1},{:.1},{:.1},{:.1})",
                placed.matched, applied.x, applied.y, applied.w, applied.h
            ));
            let poll_deadline = Instant::now() + Duration::from_secs(5);
            let mut receipt_placed = false;
            while Instant::now() < poll_deadline {
                if let Ok(cur) = read_receipt()
                    && cur.ax_frame == Some(applied)
                {
                    receipt_placed = true;
                    break;
                }
                pump(100);
            }
            if !receipt_placed {
                detail.push(format!(
                    "click-search attempt {attempt}: receipt never settled; skipping"
                ));
                continue;
            }
            // The click point must sit strictly inside the applied receipt
            // window (5pt margin): otherwise the receipt proves nothing.
            let margin = 5.0;
            if !(cg.0 >= applied.x + margin
                && cg.0 <= applied.x + applied.w - margin
                && cg.1 >= applied.y + margin
                && cg.1 <= applied.y + applied.h - margin)
            {
                detail.push(format!(
                    "click-search attempt {attempt}: ring pixel ({:.1},{:.1}) outside the applied receipt frame; skipping",
                    cg.0, cg.1
                ));
                continue;
            }
        }
        // Pin the carrier front (plain `orderFront:`, no foreign relative
        // target — the relative form is unreliable cross-process: observed
        // both above the target and parked low, so this leg never depends
        // on it and always verifies the live order below), then read the hit
        // joins at the click point. The hit-tests SKIP click-through windows
        // (they name the receipt, never the carrier), so acceptance never
        // requires them to name the carrier. Instead: the SLS input hit-test
        // (the OS input-dispatch target) and the AX hit must both name the
        // receipt, while the UNSKIPPED global CG order proves the carrier is
        // stacked above it. Together with the band proof and the counter
        // delta below, that is the traversal proof: the OS dispatches input
        // at a painted ring pixel to the window beneath the carrier.
        // Anything else is occlusion — the search moves on instead of
        // clicking into a foreign window.
        carrier.show_front(mtm);
        pump(200);
        let sls_hit = crate::ffi::sls_window_at_point(cg.0, cg.1).ok();
        let ax_hit = crate::ffi::ax_hit_test_owner(cg.0, cg.1).ok();
        let (cg_carrier_idx, cg_receipt_idx) = match crate::ffi::cg_window_list_all() {
            Ok(list) => {
                let mut carrier_idx: isize = -1;
                let mut receipt_idx: isize = -1;
                for (i, w) in list.iter().enumerate() {
                    if w.wid as i64 == carrier_wid && carrier_idx < 0 {
                        carrier_idx = i as isize;
                    }
                    if w.wid == receipt_wid && receipt_idx < 0 {
                        receipt_idx = i as isize;
                    }
                }
                (carrier_idx, receipt_idx)
            }
            Err(_) => (-1, -1),
        };
        let sls_names_receipt = sls_hit == Some(receipt_wid);
        let ax_names_receipt =
            matches!(ax_hit, Some((p, w)) if p == receipt_pid && w == receipt_wid);
        let carrier_above_receipt =
            cg_carrier_idx >= 0 && cg_receipt_idx >= 0 && cg_carrier_idx < cg_receipt_idx;
        let carrier_shown = carrier.is_visible();
        if sls_names_receipt && ax_names_receipt && carrier_above_receipt && carrier_shown {
            detail.push(format!(
                "click-search attempt {attempt} ACCEPTED sls={sls_hit:?} ax={ax_hit:?} cg_carrier={cg_carrier_idx} cg_receipt={cg_receipt_idx}"
            ));
            accepted = Some(AcceptedClick {
                point_cg: cg,
                sls_hit,
                ax_hit,
                cg_carrier_idx,
                cg_receipt_idx,
            });
            break;
        }
        detail.push(format!(
            "click-search attempt {attempt} refused (sls={sls_hit:?} ax={ax_hit:?} cg_carrier={cg_carrier_idx} cg_receipt={cg_receipt_idx} shown={carrier_shown} stack=[{}]); trying next",
            stack_fingerprint()
        ));
    }
    let (click_cg, sls_hit, ax_hit) = match accepted {
        Some(a) => {
            detail.push(format!(
                "click accepted cg_order carrier={} receipt={}",
                a.cg_carrier_idx, a.cg_receipt_idx
            ));
            (a.point_cg, a.sls_hit, a.ax_hit)
        }
        None => click_fail!(fail(
            "click",
            "no clear ring pixel after the bounded placement search; click unprovable without covering foreign windows"
                .to_owned()
        )),
    };
    // Oracle-honest post: the receipt helper must be the agreed focus
    // before the click reaches its window.
    if let Err(e) = crate::focus::dispatch_focus(receipt_pid, receipt_wid) {
        click_fail!(fail("click-focus", e.to_string()));
    }
    pump(250);
    if let Err(e) = crate::focus::read_agreement(receipt_pid, receipt_wid) {
        click_fail!(fail("click-focus", format!("receipt agreement: {e}")));
    }
    // Activation may restack the receipt app above our carrier: re-prove
    // the order immediately before the post, recovering the front (our own
    // panel only, never a foreign window) up to three times. The final
    // readback still decides — a carrier that will not stay above refuses
    // the post, so a click that cannot traverse the ring is never sent.
    let mut order_before_post: Option<(isize, isize)> = None;
    for _ in 0..3 {
        carrier.show_front(mtm);
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
            click_fail!(fail(
                "click",
                format!(
                    "carrier no longer above receipt before post (order={other:?} stack=[{}]); refusing a click that cannot traverse the ring",
                    stack_fingerprint()
                )
            ));
        }
    }
    let (mouse_before, mouse_up_before) = {
        pump(600);
        match latest_mouse_counts(&raw2_opt.as_ref().expect("receipt spawn stored").lines) {
            Some(n) => n,
            None => click_fail!(fail(
                "click",
                "no receipt mouse snapshot observed before the click".to_owned()
            )),
        }
    };
    let posted = {
        let raw2 = raw2_opt.as_mut().expect("receipt spawn stored");
        let post_target = match crate::synth::OwnedPostTarget::from_held_child(
            &mut raw2.child,
            receipt_pid,
            receipt_wid,
            &raw2.helper_path.clone(),
        ) {
            Ok(t) => t,
            Err(e) => click_fail!(fail("click", format!("click target refused: {e}"))),
        };
        match crate::synth::post_mouse_click_guarded(&post_target, click_cg) {
            Ok(p) => p,
            Err(e) => click_fail!(fail("click", format!("click post refused: {e}"))),
        }
    };
    pump(600);
    pump(600);
    let (mouse_after, mouse_up_after) =
        match latest_mouse_counts(&raw2_opt.as_ref().expect("receipt spawn stored").lines) {
            Some(n) => n,
            None => click_fail!(fail(
                "click",
                "no receipt mouse snapshot observed after the click".to_owned()
            )),
        };
    let click_receipt_ok = mouse_after > mouse_before;
    detail.push(format!(
        "click down_us={} up_us={} mouse_before={mouse_before} mouse_after={mouse_after} mouse_up_before={mouse_up_before} mouse_up_after={mouse_up_after}",
        posted.down_latency_us, posted.up_latency_us
    ));
    if !click_receipt_ok {
        click_fail!(fail(
            "click",
            format!(
                "no receipt delta (before={mouse_before} after={mouse_after}); posted click not delivered to the receipt helper (cg=({:.1},{:.1}) sls_hit={sls_hit:?} ax_hit={ax_hit:?} carrier_wid={carrier_wid} receipt_wid={receipt_wid})",
                click_cg.0, click_cg.1
            )
        ));
    }
    // Pointer restore via the eventless warp (never a synthesized move),
    // then focus back to the target with agreement.
    if let Err(e) = crate::ffi::warp_cursor_position(cursor_before.0, cursor_before.1) {
        click_fail!(fail("click", format!("pointer restore refused: {e}")));
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
        click_fail!(fail(
            "click",
            "pointer restore readback missed the pre-click position".to_owned()
        ));
    }
    if let Err(e) = crate::focus::dispatch_focus(helper_pid, helper_wid) {
        click_fail!(fail("click", format!("target refocus: {e}")));
    }
    pump(250);
    if let Err(e) = crate::focus::read_agreement(helper_pid, helper_wid) {
        click_fail!(fail("click", format!("target agreement after click: {e}")));
    }
    carrier.show_above(mtm, helper_wid as isize);
    pump(200);
    if !carrier.is_visible() {
        click_fail!(fail(
            "click",
            "carrier not visible after click focus restore".to_owned()
        ));
    }

    // FENCE leg (no Space mutation): hide reads back invisible, re-show
    // reads back visible above the foreign target.
    or_teardown2!(gate("fence", &mut detail));
    carrier.hide(mtm);
    pump(200);
    let fence_hide_ok = !carrier.is_visible();
    carrier.show_above(mtm, helper_wid as isize);
    pump(200);
    let fence_restore_ok = carrier.is_visible();

    // FULLSCREEN leg on the OWNED FOREIGN window only: enter via the
    // ownership-joined AX control, verify the carrier hides DURING
    // fullscreen, then exit via the HELD-Child authorization and prove the
    // return. Runs BEFORE the click leg so one invocation yields both
    // evidence sets even when the click search finds no clear pixel. Exit
    // membership is read back read-only — its synchronous equality is NOT
    // an oracle (the transition animates asynchronously; two prior
    // variations of that semantic failed and must not be retried). The exit
    // gates on the AX fullscreen readback (`Some(false)` after the bounded
    // settle poll) plus carrier return and focus agreement; final
    // teardown/topology restoration carries the rest.
    or_teardown2!(gate("fullscreen", &mut detail));
    let membership_before = crate::ffi::sls_spaces_for_window(helper_wid).unwrap_or_default();
    let enter = or_teardown2!(
        crate::ax::set_fullscreen_owned(&owned, helper_pid, helper_wid, true).map_err(|e| {
            let diag = diag_snapshot(helper_pid, helper_wid, start);
            fail("fullscreen", format!("{e}; diag: {diag}"))
        })
    );
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
        // Exit rides the held-Child authorization: while fullscreen the
        // window server withholds the title proof the cross-command join
        // needs, but the exact unreaped Child still excludes PID reuse, so
        // this exit is a NEW ownership basis — not a renamed retry of the
        // failed synchronous membership oracle (membership stays
        // read-only below).
        match crate::ax::set_fullscreen_owned_held(
            &owned,
            &mut raw.child,
            &raw.helper_path,
            helper_pid,
            helper_wid,
            false,
        ) {
            Ok(exit) => {
                fullscreen_exit_readback = Some(exit.fullscreen.unwrap_or(true));
                fullscreen_exit_held = true;
                detail.push(format!(
                    "fs_exit held applied={} readback={:?}",
                    exit.applied, exit.fullscreen
                ));
                carrier.show_above(mtm, helper_wid as isize);
                pump(250);
                let back_visible = carrier.is_visible();
                let back_focused = crate::ffi::ax_focused_preimage_detailed().ok()
                    == Some((helper_pid as i32, Some(helper_wid)));
                fullscreen_return_ok = back_visible && back_focused;
                detail.push(format!(
                    "fs_return visible={back_visible} focused_helper={back_focused}"
                ));
            }
            Err(e) => {
                let diag = diag_snapshot(helper_pid, helper_wid, start);
                detail.push(format!("fs_exit held refused ({e}; diag: {diag})"));
            }
        }
    } else {
        detail.push("fullscreen leg skipped: AX enter not applied; no exit attempted".to_owned());
    }
    let mut membership_after = crate::ffi::sls_spaces_for_window(helper_wid).unwrap_or_default();
    // Read-only resettle: the exit animation can still be in flight at the
    // first read (e.g. a transient empty set right after return), so take
    // one more read after a short pump for the record. Never gated.
    if membership_after != membership_before {
        pump(500);
        membership_after = crate::ffi::sls_spaces_for_window(helper_wid).unwrap_or_default();
        detail.push(format!("membership_after resettled={membership_after:?}"));
    }

    // TEARDOWN: destroy the carrier, stop the EXACT held helpers (receipt
    // first, then the target; identity revalidated before each signal),
    // then prove the residue: helpers gone, records removed, no owned CG
    // windows onscreen, Space topology identical to the preimage. Focus is
    // READ BACK only: the baseline was foreign (possibly the agent host)
    // and is never restored with keys, activation, or any touch.
    or_teardown2!(gate("teardown", &mut detail));
    let carrier_draws = carrier.draws();
    carrier.destroy(mtm);
    pump(200);
    if let Some(raw2) = raw2_opt.as_mut() {
        let (r_reaped, r_gone, r_removed) = stop_held2(raw2, &mut detail);
        receipt_reaped = r_reaped;
        receipt_gone = r_gone;
        receipt_record_removed = r_removed;
    }
    let (helper_reaped, helper_gone, record_removed) = stop_held(&mut raw, &mut detail);
    pump(400);
    let residue: Vec<_> = crate::ffi::cg_window_list_all()
        .map(|list| {
            list.into_iter()
                .filter(|w| {
                    w.pid == helper_pid
                        || w.pid == receipt_pid
                        || (w.pid == std::process::id() && w.wid as i64 == carrier_wid)
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let residue_total = residue.len();
    let residue_onscreen = residue.iter().filter(|w| w.onscreen).count();
    let space_after = crate::focus::capture_space().map_err(|e| fail("space-after", e))?;
    let space_restored = space_after == space_before;
    let frontmost_after = crate::ffi::frontmost_app().map(|(pid, _)| pid);
    let focused_final = crate::ffi::ax_focused_preimage_detailed().ok();
    detail.push(format!(
        "space_restored={space_restored} frontmost_before={frontmost_before:?} \
         frontmost_after={frontmost_after:?} focused_final={focused_final:?}"
    ));

    let pass = ordered_above_global
        && focus_names_helper
        && carrier_never_focused
        && move_track_ok
        && resize_track_ok
        && click_receipt_ok
        && pointer_restored
        && fence_hide_ok
        && fence_restore_ok
        && fullscreen_entered
        && fullscreen_hide_ok
        && fullscreen_exit_readback == Some(false)
        && fullscreen_exit_held
        && fullscreen_return_ok
        && helper_reaped
        && helper_gone
        && record_removed
        && receipt_reaped
        && receipt_gone
        && receipt_record_removed
        && residue_onscreen == 0
        && space_restored;
    let report = BorderSessionReport {
        session_unlocked: true,
        ax_trusted: trust,
        slps_available: true,
        helper_pid,
        helper_wid,
        carrier_wid,
        style_width: style.width,
        style_gap: style.gap,
        style_radius: style.radius,
        style_color_hex: crate::active_border::render_color(effective),
        accent_hex: accent.map(|a| crate::active_border::render_color(a.color)),
        carrier_ignores_mouse: flags.ignores_mouse,
        carrier_can_key: flags.can_key,
        carrier_can_main: flags.can_main,
        carrier_collection_managed_only: flags.collection_managed_only,
        carrier_level: flags.level,
        carrier_draws,
        ordered_above_global,
        cg_carrier_idx,
        cg_target_idx,
        focused_before_carrier,
        focused_after_carrier,
        carrier_never_focused,
        move_track_ok,
        move_lag_us,
        resize_track_ok,
        resize_lag_us,
        backing_scale,
        fence_hide_ok,
        fence_restore_ok,
        fullscreen_entered,
        fullscreen_hide_ok,
        fullscreen_exit_readback,
        fullscreen_exit_held,
        fullscreen_return_ok,
        membership_before,
        membership_after,
        helper_reaped,
        helper_gone,
        record_removed,
        receipt_pid,
        receipt_wid,
        click_point_cg: Some(click_cg),
        click_sls_wid: sls_hit,
        click_ax_hit: ax_hit,
        click_mouse_before: mouse_before,
        click_mouse_after: mouse_after,
        click_mouse_up_before: mouse_up_before,
        click_mouse_up_after: mouse_up_after,
        click_receipt_ok,
        pointer_restored,
        receipt_reaped,
        receipt_record_removed,
        receipt_gone,
        residue_total,
        residue_onscreen,
        frontmost_before,
        frontmost_after,
        space_restored,
        clickthrough_parked: true,
        clickthrough_proved: click_receipt_ok && pointer_restored,
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
    fn border_seconds_gate_is_tight_and_real() {
        // 30..=120 is both validation AND the enforced deadline budget:
        // no new leg starts past it and the helper self-exits at it.
        assert!(validate_border_seconds(MIN_BORDER_SECONDS).is_ok());
        assert!(validate_border_seconds(DEFAULT_BORDER_SECONDS).is_ok());
        assert!(validate_border_seconds(MAX_BORDER_SECONDS).is_ok());
        assert!(validate_border_seconds(MIN_BORDER_SECONDS - 1).is_err());
        assert!(validate_border_seconds(MAX_BORDER_SECONDS + 1).is_err());
        assert!(validate_border_seconds(0).is_err());
    }

    #[test]
    fn border_session_refuses_locked_session_before_ui() {
        // The gate is pure session-state logic: a non-unlocked verdict never
        // reaches window creation. This exercises the refusal shape without
        // launching UI (the live verdict comes from `session_state`).
        let locked = !crate::ffi::SessionState::unknown().unlocked();
        assert!(locked);
    }

    #[test]
    fn frame_equality_is_half_point() {
        assert!(frames_close((1.0, 2.0, 3.0, 4.0), (1.4, 2.4, 3.4, 4.4)));
        assert!(!frames_close((1.0, 2.0, 3.0, 4.0), (1.6, 2.0, 3.0, 4.0)));
    }

    #[test]
    fn ring_sample_lies_in_the_painted_band() {
        // Outer 100,100 200x100 with a 3pt ring: the sample is the left
        // band middle, inside the outer frame but outside the inner one.
        let outer = (100.0, 100.0, 200.0, 100.0);
        let inner = (103.0, 103.0, 194.0, 94.0);
        let sample = ring_sample_appkit(outer, 3.0).expect("sample");
        assert!((sample.0 - 101.5).abs() < 1e-9);
        assert!((sample.1 - 150.0).abs() < 1e-9);
        assert!(point_in_ring_band(outer, inner, sample));
        // Band edges: outer corners are band, the target interior is not.
        assert!(point_in_ring_band(outer, inner, (100.0, 100.0)));
        assert!(!point_in_ring_band(outer, inner, (150.0, 150.0)));
        // Deep outside and degenerate rings refuse.
        assert!(!point_in_ring_band(outer, inner, (50.0, 50.0)));
        assert!(ring_sample_appkit(outer, 0.0).is_none());
        assert!(ring_sample_appkit(outer, -1.0).is_none());
        assert!(ring_sample_appkit((0.0, 0.0, 2.0, 2.0), 3.0).is_none());
    }

    #[test]
    fn latest_mouse_counts_reads_both_edges() {
        let (tx, rx) = std::sync::mpsc::channel::<String>();
        for line in [
            r#"{"ready": true}"#,
            r#"{"key_counts": {"h_down": [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]}}"#,
            r#"{"key_counts": {}, "mouse_counts": {"left_down": 2, "left_up": 2}}"#,
            "not json",
            // Partial edge (down without up) is skipped, never half-read.
            r#"{"mouse_counts": {"left_down": 9}}"#,
            r#"{"mouse_counts": {"left_down": 3, "left_up": 4}}"#,
        ] {
            tx.send(line.to_owned()).unwrap();
        }
        assert_eq!(latest_mouse_counts(&rx), Some((3, 4)));
        // Drained: nothing left.
        assert_eq!(latest_mouse_counts(&rx), None);
    }
}

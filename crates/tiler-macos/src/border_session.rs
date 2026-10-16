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
//! API readback only — physical click-through stays parked until a helper
//! click-receipt channel exists (no new TCC grant is requested for it).
//! Fullscreen-exit membership is read back but never gated synchronously
//! (the transition is asynchronous); the fullscreen leg gates on hide
//! DURING fullscreen plus final teardown/topology restoration instead.

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
    /// Fixture wall time in seconds (bounded by `--seconds`).
    pub elapsed_secs: u64,
    /// Core acceptance: global order, tracking, focus isolation, fence,
    /// fullscreen hide, and clean teardown. Fullscreen EXIT equality is
    /// deliberately not a gate.
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

    // ORDER leg (global, cross-PID, no owner filtering). A `false` verdict
    // with both windows present is the documented API constraint: public
    // `orderWindow:relativeTo:` does not pin a panel above a foreign
    // window. That STOPs the ordering claim (pass=false) but the remaining
    // legs still run so focus/tracking/fence/teardown evidence is complete.
    or_teardown!(gate("order", &mut detail), carrier, raw, detail);
    let cg_list = crate::ffi::cg_window_list_all().unwrap_or_default();
    let (ordered_above_global, cg_carrier_idx, cg_target_idx) =
        crate::ffi::cg_carrier_above_target(&cg_list, carrier_wid as isize, helper_wid as isize);
    detail.push(format!(
        "cg_len={} cg_carrier={cg_carrier_idx} cg_target={cg_target_idx} visible={visible_after_show} draws={draws_after_show}",
        cg_list.len(),
    ));
    if !ordered_above_global {
        detail.push(
            "ORDER CONSTRAINT: carrier not above the foreign target in the global CG list; \
             public relative ordering does not extend cross-process (no SkyLight attempted)"
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

    // FENCE leg (no Space mutation): hide reads back invisible, re-show
    // reads back visible above the foreign target.
    or_teardown!(gate("fence", &mut detail), carrier, raw, detail);
    carrier.hide(mtm);
    pump(200);
    let fence_hide_ok = !carrier.is_visible();
    carrier.show_above(mtm, helper_wid as isize);
    pump(200);
    let fence_restore_ok = carrier.is_visible();

    // FULLSCREEN leg on the OWNED FOREIGN window only: enter via the
    // ownership-joined AX control, verify the carrier hides DURING
    // fullscreen, then exit. Exit membership is read back read-only — its
    // synchronous equality is NOT an oracle (the transition animates
    // asynchronously; two prior variations of that semantic failed). The leg
    // gates on hide-during-fullscreen; final teardown/topology restoration
    // below carries the exit proof instead.
    or_teardown!(gate("fullscreen", &mut detail), carrier, raw, detail);
    let membership_before = crate::ffi::sls_spaces_for_window(helper_wid).unwrap_or_default();
    let enter = or_teardown!(
        crate::ax::set_fullscreen_owned(&owned, helper_pid, helper_wid, true).map_err(|e| {
            let diag = diag_snapshot(helper_pid, helper_wid, start);
            fail("fullscreen", format!("{e}; diag: {diag}"))
        }),
        carrier,
        raw,
        detail
    );
    let fullscreen_entered = enter.applied && enter.fullscreen == Some(true);
    detail.push(format!(
        "fs_enter applied={} readback={:?} membership_before={membership_before:?}",
        enter.applied, enter.fullscreen
    ));
    let mut fullscreen_hide_ok = false;
    let mut fullscreen_exit_readback: Option<bool> = None;
    if fullscreen_entered {
        carrier.hide(mtm);
        pump(250);
        fullscreen_hide_ok = !carrier.is_visible();
        // Exit is attempted once through the joined path, but its outcome
        // never gates: while fullscreen the window server may withhold the
        // title proof the join needs, and the transition itself is
        // asynchronous. A failed exit attempt records and continues to
        // teardown (the exact-Child fallback stop owns the fullscreen
        // helper either way); final topology/restoration carries the proof.
        match crate::ax::set_fullscreen_owned(&owned, helper_pid, helper_wid, false) {
            Ok(exit) => {
                fullscreen_exit_readback = Some(exit.fullscreen.unwrap_or(true));
                detail.push(format!(
                    "fs_exit applied={} readback={:?}",
                    exit.applied, exit.fullscreen
                ));
                carrier.show_above(mtm, helper_wid as isize);
                pump(250);
            }
            Err(e) => {
                let diag = diag_snapshot(helper_pid, helper_wid, start);
                detail.push(format!("fs_exit skipped ({e}; diag: {diag})"));
            }
        }
    } else {
        detail.push("fullscreen leg skipped: AX enter not applied; no exit attempted".to_owned());
    }
    let membership_after = crate::ffi::sls_spaces_for_window(helper_wid).unwrap_or_default();

    // TEARDOWN: destroy the carrier, stop the EXACT held helper Child
    // (identity revalidated before the signal), then prove the residue:
    // helper gone, record removed, no owned CG windows onscreen, Space
    // topology identical to the preimage. Focus is READ BACK only: the
    // baseline was foreign (possibly the agent host) and is never restored
    // with keys, activation, or any touch.
    or_teardown!(gate("teardown", &mut detail), carrier, raw, detail);
    let carrier_draws = carrier.draws();
    carrier.destroy(mtm);
    pump(200);
    let (helper_reaped, helper_gone, record_removed) = stop_held(&mut raw, &mut detail);
    pump(400);
    let residue: Vec<_> = crate::ffi::cg_window_list_all()
        .map(|list| {
            list.into_iter()
                .filter(|w| {
                    w.pid == helper_pid
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
        && fence_hide_ok
        && fence_restore_ok
        && fullscreen_entered
        && fullscreen_hide_ok
        && helper_reaped
        && helper_gone
        && record_removed
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
        membership_before,
        membership_after,
        helper_reaped,
        helper_gone,
        record_removed,
        residue_total,
        residue_onscreen,
        frontmost_before,
        frontmost_after,
        space_restored,
        clickthrough_parked: true,
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
}

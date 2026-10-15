#![forbid(unsafe_code)]

//! Reusable owned-window focus primitive over the yabai SLPS route.
//!
//! Actuation is exactly yabai's `window_manager_focus_window_with_raise`
//! shape minus `AXRaise`: `_SLPSSetFrontProcessWithOptions(psn, wid,
//! kCPSUserGenerated)` then the two `SLPSPostEventRecordTo` key-window posts
//! built by [`make_key_event_bytes`] (0xf8 bytes, `[0x04]=0xf8`,
//! `[0x3a]=0x10`, wid at `0x3c`, `0xff` fill at `0x20..0x30`,
//! `[0x08]=0x01` then `0x02`). Source pins (local checkouts, no web fetch):
//! `yabai/src/window_manager.c` (`window_manager_make_key_window`,
//! `window_manager_focus_window_with_raise`), `yabai/src/window_manager.h`
//! (`kCPSUserGenerated`), `yabai/src/misc/extern.h` (both SLPS symbols).
//!
//! This module never touches `NSRunningApplication::activate`,
//! `SetFrontProcess`, helper self-activation, taps, or synthetic keystrokes
//! (prior failed routes). All `unsafe` lives in [`crate::ffi`]; this module
//! uses only its safe API. Dispatch (`Ok`) means sent — focus arrival is
//! async and MUST be verified by the selected oracle here (SLPS
//! front-process agreeing with the NSWorkspace frontmost before AND after,
//! plus the direct per-app focused window — `AXFocusedWindow`, else
//! `AXMainWindow` — with a CG owner join; agreement required for target AND
//! restore, disagreement fails closed). The system-wide `AXFocusedApplication`
//! read is diagnostic-only and `AXFrontmost` never gates. Restoration
//! uses the SAME route to the recorded preimage. The oracle fixture is the
//! owned helper with the opt-in regular activation policy (see
//! [`crate::probe_target`]); no separate process-identity machinery.

use serde::Serialize;
use std::time::{Duration, Instant};

/// Length of the yabai key-window event record.
pub const EVENT_LEN: usize = crate::ffi::SLPS_EVENT_LEN;
/// `kCPSUserGenerated` (yabai `window_manager.h`).
pub const CPS_USER_GENERATED: u32 = crate::ffi::SLPS_CPS_USER_GENERATED;
/// Bundle ID of the macOS loginwindow process (lock screen / login
/// context). Focus work never baselines on it and never dispatches while it
/// is frontmost: the lock screen is outside the live safety contract, and no
/// lockscreen recovery is attempted.
pub const LOGINWINDOW_BUNDLE: &str = "com.apple.loginwindow";

/// Pure predicate: whether a bundle ID is the loginwindow context that
/// focus work must refuse. Unit-tested; no desktop effect.
pub fn is_loginwindow_bundle(bundle: &str) -> bool {
    bundle == LOGINWINDOW_BUNDLE
}

/// Build one key-window event record for `wid` with phase byte `phase`
/// (`0x01` then `0x02` per yabai `window_manager_make_key_window`).
/// Pure; unit-tested below. `wid` is stored with native byte order to mirror
/// yabai's `memcpy` of the `uint32_t` (little-endian on Apple Silicon).
pub fn make_key_event_bytes(wid: u32, phase: u8) -> [u8; EVENT_LEN] {
    let mut bytes = [0u8; EVENT_LEN];
    bytes[0x04] = 0xf8;
    bytes[0x3a] = 0x10;
    bytes[0x3c..0x3c + 4].copy_from_slice(&wid.to_ne_bytes());
    for b in bytes.iter_mut().skip(0x20).take(0x10) {
        *b = 0xff;
    }
    bytes[0x08] = phase;
    bytes
}

/// Meaningful focus errors (no bare codes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FocusError {
    Unavailable(String),
    Invalid(String),
    Ownership(String),
    Disagreement(String),
    System(String),
}

impl std::fmt::Display for FocusError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unavailable(r) => write!(f, "focus unavailable: {r}"),
            Self::Invalid(r) => write!(f, "focus invalid: {r}"),
            Self::Ownership(r) => write!(f, "focus ownership refused: {r}"),
            Self::Disagreement(r) => write!(f, "focus disagreement: {r}"),
            Self::System(r) => write!(f, "focus system error: {r}"),
        }
    }
}

impl std::error::Error for FocusError {}

/// One SLPS dispatch (actuation only, never effect evidence).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FocusDispatch {
    pub pid: u32,
    pub wid: u32,
    pub set_front_status: i32,
    pub post_statuses: Vec<i32>,
    pub dispatch_latency_ms: u64,
}

/// Verified focus state: SLPS front-process + workspace frontmost + direct
/// per-app focused window agree, with CG ownership. Required for target AND
/// restore alike. `evidence_route` always records the selected oracle route
/// (`slps-front-direct-focused` or `slps-front-direct-main`): the raw
/// system-wide leg is diagnostic-only and never feeds production agreement,
/// and `AXFrontmost` never gates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FocusAgreement {
    pub workspace_pid: i32,
    pub workspace_bundle: String,
    pub ax_pid: i32,
    pub ax_wid: Option<u32>,
    pub cg_wid_owned: bool,
    pub evidence_route: String,
}

/// Original preimage (foreign app to restore). `wid` is required: a missing
/// focused window fails closed (no wid => no same-route restore). `exe` is
/// the live executable currency: the restore revalidates it (plus bundle
/// and CG join) before dispatching, so a stale PID/WID reuse is refused
/// instead of focused. `evidence_route` always records the selected oracle
/// route; the raw system-wide read never feeds production.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OriginalPreimage {
    pub pid: i32,
    pub bundle: String,
    pub wid: u32,
    pub exe: String,
    pub evidence_route: String,
}

/// Space baseline/readback: per-display membership in display order. Each
/// entry keeps its display's ordered Space ID list plus that display's
/// current Space, so cross-display topology survives the round trip (no
/// union/sort that would collapse two displays into one claim). IDs only;
/// display UUIDs never leave the snapshot call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DisplaySpaces {
    pub spaces: Vec<u64>,
    pub current: Option<u64>,
}

/// Space baseline/readback: per-display membership in display order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SpaceSnapshot {
    pub displays: Vec<DisplaySpaces>,
}

pub(crate) fn latency_ms(start: Instant) -> u64 {
    start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64
}

/// Dispatch the SLPS focus route to `pid`/`wid` (owned helper or recorded
/// preimage — caller owns the policy). Returns the OS statuses plus the
/// dispatch wall time. Pure validation (reserved PID, wid 0, out-of-range
/// PID) runs before any FFI or symbol probe, so invalid inputs never touch
/// the desktop on any platform.
///
/// Dispatch is NOT atomic: `_SLPSSetFrontProcessWithOptions` or the first
/// `SLPSPostEventRecordTo` can take effect before a later post fails, so an
/// `Err` never means "nothing happened". Every caller must attempt the
/// SAME-route restore with a readback after any dispatch error; no automatic
/// retry is attempted here. Preserved nonzero statuses plus the readback are
/// the honest failure evidence.
pub fn dispatch_focus(pid: u32, wid: u32) -> Result<FocusDispatch, FocusError> {
    if pid <= 1 {
        return Err(FocusError::Invalid(format!("refusing reserved pid {pid}")));
    }
    if wid == 0 {
        return Err(FocusError::Invalid("refusing window id 0".to_owned()));
    }
    let pid_i32: i32 = pid
        .try_into()
        .map_err(|_| FocusError::Invalid(format!("pid {pid} out of range")))?;
    if !crate::ffi::slps_available() {
        return Err(FocusError::Unavailable(
            "SLPS focus symbols unavailable (GetProcessForPID/_SLPSSetFrontProcessWithOptions/SLPSPostEventRecordTo); stop — needs no SA, no policy change, route simply absent"
                .to_owned(),
        ));
    }
    // Lockscreen firewall, read immediately before any SLPS effect (covers
    // every dispatch path including restore): a missing/unknown frontmost
    // or a loginwindow frontmost refuses without touching the desktop. No
    // lockscreen recovery is attempted.
    match crate::ffi::frontmost_app() {
        Some((_, bundle)) if is_loginwindow_bundle(&bundle) => {
            return Err(FocusError::Disagreement(
                "frontmost is com.apple.loginwindow (lock screen/login context); refusing dispatch — no lockscreen recovery attempted"
                    .to_owned(),
            ));
        }
        Some(_) => {}
        None => {
            return Err(FocusError::System(
                "frontmost unreadable immediately before dispatch; refusing".to_owned(),
            ));
        }
    }
    let start = Instant::now();
    let psn = crate::ffi::slps_psn_for_pid(pid_i32).map_err(|e| {
        if e.contains("unavailable") {
            FocusError::Unavailable(e)
        } else {
            FocusError::System(e)
        }
    })?;
    let set_status = match crate::ffi::slps_set_front(&psn, wid, CPS_USER_GENERATED) {
        Ok(()) => 0,
        Err(code) => code,
    };
    if set_status != 0 {
        return Err(FocusError::System(format!(
            "_SLPSSetFrontProcessWithOptions pid {pid} wid {wid}: CGError {set_status} (dispatch refused)"
        )));
    }
    let mut post_statuses = Vec::with_capacity(2);
    for phase in [0x01u8, 0x02u8] {
        let bytes = make_key_event_bytes(wid, phase);
        match crate::ffi::slps_post_event(&psn, &bytes) {
            Ok(()) => post_statuses.push(0),
            Err(code) => {
                return Err(FocusError::System(format!(
                    "SLPSPostEventRecordTo pid {pid} wid {wid} phase {phase:#04x}: CGError {code}"
                )));
            }
        }
    }
    Ok(FocusDispatch {
        pid,
        wid,
        set_front_status: set_status,
        post_statuses,
        dispatch_latency_ms: latency_ms(start),
    })
}

/// Capture the original frontmost preimage with the selected oracle (SLPS
/// front-process agreeing with the workspace frontmost, plus the direct
/// per-app focused window with CG ownership), plus executable currency for
/// the restore. Fails closed on any disagreement (SLPS pid != workspace
/// pid, AX identity mismatch, selected wid missing, CG join mismatch,
/// unreadable exe/bundle). `exclude_pid` (the owned helper once spawned)
/// must not already be frontmost — otherwise there is no foreign preimage
/// to restore. Callers capture BEFORE spawning the helper so spawn-time
/// activation can never invalidate the preimage.
pub fn capture_original(exclude_pid: Option<u32>) -> Result<OriginalPreimage, FocusError> {
    if crate::ffi::ax_is_process_trusted() != Some(true) {
        return Err(FocusError::Unavailable(
            "AX trust not observed true; preimage unreadable — stop (user consent needed, no bypass)"
                .to_owned(),
        ));
    }
    let (ws_pid, ws_bundle) = crate::ffi::frontmost_app()
        .ok_or_else(|| FocusError::System("frontmost identity unreadable".to_owned()))?;
    if ws_pid <= 1 {
        return Err(FocusError::System(format!(
            "frontmost pid {ws_pid} is reserved; refusing"
        )));
    }
    // Exact comparison without a lossy `u32 as i32` cast: an out-of-range
    // `exclude_pid` cannot equal an `i32` frontmost PID, so it simply does
    // not exclude.
    let excluded = exclude_pid
        .and_then(|p| i32::try_from(p).ok())
        .is_some_and(|excluded| ws_pid == excluded);
    if excluded {
        return Err(FocusError::Disagreement(
            "frontmost is already the owned helper; no foreign preimage to restore".to_owned(),
        ));
    }
    // Lockscreen firewall for the baseline: a loginwindow frontmost (lock
    // screen / login context) is refused BEFORE any spawn, so a later run on
    // a locked desktop can never spawn the helper or move focus. Checked
    // here rather than in the poll loop so the refusal cause is exact.
    if is_loginwindow_bundle(&ws_bundle) {
        return Err(FocusError::Disagreement(
            "frontmost is com.apple.loginwindow (lock screen/login context); refusing baseline — no spawn, no focus effects"
                .to_owned(),
        ));
    }
    // Selected oracle read: SLPS front == workspace frontmost (before+after)
    // plus the direct per-app focused window (focused, else main fallback)
    // with CG ownership. The route is recorded honestly from this same read
    // — never a second live call that could race it.
    let selected = match crate::ffi::ax_selected_focus_evidence() {
        Ok(evidence) => evidence,
        Err(selected_cause) => {
            // Real cause (named SLPS/AX/CG leg), never a wedged-OS guess:
            // a windowless frontmost yields a named missing-window leg while
            // foreground attribution stays readable. PIDs/bundle IDs only —
            // never titles or content.
            let ws_side = crate::ffi::frontmost_app()
                .map(|(p, b)| format!("workspace frontmost pid {p} bundle '{b}'"))
                .unwrap_or_else(|| "workspace frontmost unreadable".to_owned());
            return Err(FocusError::System(format!(
                "selected focus preimage unreadable ({selected_cause}); {ws_side}; refusing"
            )));
        }
    };
    let (ax_pid, ax_wid) = (selected.pid, selected.wid);
    if ax_pid != ws_pid {
        return Err(FocusError::Disagreement(format!(
            "workspace pid {ws_pid} != selected AX pid {ax_pid}; refusing without agreement"
        )));
    }
    let wid = ax_wid.ok_or_else(|| {
        FocusError::System(
            "selected focused window none (frontmost has no focused/main window); refusing without a restorable wid"
                .to_owned(),
        )
    })?;
    if wid == 0 {
        return Err(FocusError::Invalid(
            "selected focused wid is 0; refusing".to_owned(),
        ));
    }
    crate::cg::verify_wid_owned_by_pid(ws_pid as u32, wid).map_err(|e| {
        FocusError::Disagreement(format!("CG join disagrees with selected oracle: {e}"))
    })?;
    if ws_bundle.is_empty() {
        return Err(FocusError::System(
            "frontmost bundle unreadable; refusing without bundle currency".to_owned(),
        ));
    }
    let exe = crate::ffi::process_executable_path(ws_pid).ok_or_else(|| {
        FocusError::System(format!(
            "frontmost pid {ws_pid}: executable path unreadable; refusing without exe currency"
        ))
    })?;
    Ok(OriginalPreimage {
        pid: ws_pid,
        bundle: ws_bundle,
        wid,
        exe,
        evidence_route: selected.route.as_str().to_owned(),
    })
}

/// Revalidate the recorded original before the SAME-route restore dispatch:
/// live bundle must equal the recorded bundle, live executable must equal
/// the recorded exe, and the WID must still join the live PID. A stale
/// PID/WID reuse is refused here — never dispatched.
pub fn verify_original_currency(original: &OriginalPreimage) -> Result<(), FocusError> {
    if original.pid <= 1 {
        return Err(FocusError::Invalid(format!(
            "refusing reserved pid {}",
            original.pid
        )));
    }
    if original.wid == 0 {
        return Err(FocusError::Invalid("refusing window id 0".to_owned()));
    }
    match crate::ffi::bundle_id_for_pid(original.pid).filter(|b| !b.is_empty()) {
        Some(live) if live == original.bundle => {}
        Some(live) => {
            return Err(FocusError::Disagreement(format!(
                "original bundle changed (was '{}', live '{live}'); refusing stale restore",
                original.bundle
            )));
        }
        None => {
            return Err(FocusError::Disagreement(format!(
                "original pid {} has no live bundle; refusing stale restore",
                original.pid
            )));
        }
    }
    match crate::ffi::process_executable_path(original.pid) {
        Some(live) if live == original.exe => {}
        _ => {
            return Err(FocusError::Disagreement(format!(
                "original pid {} executable changed; refusing stale restore",
                original.pid
            )));
        }
    }
    crate::cg::verify_wid_owned_by_pid(original.pid as u32, original.wid)
        .map_err(|e| FocusError::Disagreement(format!("original CG join lost: {e}")))?;
    Ok(())
}

/// Read the current selected-oracle agreement once (no wait). `Ok` only
/// when the SLPS front-process PID, the workspace frontmost PID, and the
/// direct per-app selected window ALL agree on `want_pid`/`want_wid` with
/// CG ownership; otherwise `Err` naming the mismatch. The system-wide AX
/// read never participates; `AXFrontmost` never gates.
pub fn read_agreement(want_pid: u32, want_wid: u32) -> Result<FocusAgreement, FocusError> {
    if want_pid <= 1 {
        return Err(FocusError::Invalid(format!(
            "refusing reserved pid {want_pid}"
        )));
    }
    if want_wid == 0 {
        return Err(FocusError::Invalid("refusing window id 0".to_owned()));
    }
    // Exact comparison without a lossy `u32 as i32` cast.
    let want_pid_i32 = i32::try_from(want_pid)
        .map_err(|_| FocusError::Invalid(format!("pid {want_pid} out of range")))?;
    let (ws_pid, ws_bundle) = crate::ffi::frontmost_app()
        .ok_or_else(|| FocusError::System("frontmost unreadable".to_owned()))?;
    // Selected production read: SLPS foreground plus the direct per-app
    // window legs. The experimental diagnostic fallback (behind
    // `focus-diagnose --fallback`) and the raw system-wide read never feed
    // agreement.
    let slps_pid = crate::ffi::slps_front_pid().map_err(FocusError::System)?;
    let selected = crate::ffi::ax_selected_focus_evidence()
        .map_err(|cause| FocusError::System(format!("selected focus unreadable ({cause})")))?;
    let (ax_pid, ax_wid) = (selected.pid, selected.wid);
    let evidence_route = selected.route.as_str().to_owned();
    let cg_owned = crate::cg::verify_wid_owned_by_pid(want_pid, want_wid).is_ok()
        && crate::ffi::cg_window_list_all()
            .map(|list| list.iter().any(|w| w.wid == want_wid && w.pid == want_pid))
            .unwrap_or(false);
    let agreement = FocusAgreement {
        workspace_pid: ws_pid,
        workspace_bundle: ws_bundle,
        ax_pid,
        ax_wid,
        cg_wid_owned: cg_owned,
        evidence_route: evidence_route.clone(),
    };
    let agreed = ws_pid == want_pid_i32
        && slps_pid == want_pid_i32
        && crate::ffi::selected_focus_agrees(
            want_pid,
            want_wid,
            Some(slps_pid),
            Some(ws_pid),
            ax_wid,
            cg_owned,
        )
        && ax_pid == want_pid_i32;
    if agreed {
        Ok(agreement)
    } else {
        Err(FocusError::Disagreement(format!(
            "want pid {want_pid} wid {want_wid}, have slps {slps_pid} workspace {ws_pid} AX ({ax_pid},{ax_wid:?}) cg_owned={cg_owned} route={evidence_route}"
        )))
    }
}

/// Poll until workspace + AX + CG agree on `want_pid`/`want_wid` or the
/// timeout. Returns the agreement plus the poll wall time; timeout is a
/// disagreement (never a pass).
///
/// Inter-read wait keeps a main-thread caller serviced: the controller polls
/// on the main thread, and an uns serviced `sleep` there starves the very
/// workspace foreground delivery the oracle reads (the owned helper already
/// pumps; a fresh spawn plus a blocked poll left the controller's
/// `NSWorkspace` read stale while SLPS/AX/CG had moved). Off the main
/// thread the wait sleeps and never touches main UI; the decision is the
/// live `MainThreadMarker`, never an init flag. Bounded by the deadline.
pub fn await_agreement(
    want_pid: u32,
    want_wid: u32,
    timeout: Duration,
) -> Result<(FocusAgreement, u64), FocusError> {
    let start = Instant::now();
    let deadline = start + timeout;
    let mut last_err: String;
    loop {
        match read_agreement(want_pid, want_wid) {
            Ok(agreement) => return Ok((agreement, latency_ms(start))),
            Err(e) => last_err = e.to_string(),
        }
        if Instant::now() >= deadline {
            return Err(FocusError::Disagreement(format!(
                "focus agreement timeout ({timeout:?}): {last_err}"
            )));
        }
        poll_wait(deadline);
    }
}

/// Pure inter-read step: never longer than 100ms and never past the
/// deadline. Unit-tested; no desktop effect.
pub(crate) fn poll_step_ms(remaining_ms: u64) -> u64 {
    remaining_ms.min(100)
}

/// Bounded inter-read wait for [`await_agreement`]. On the main thread the
/// wait drains pending AppKit events then pumps the runloop (bounded
/// slices, autorelease-pooled) instead of sleeping uns serviced; off the
/// main thread it sleeps. Safe APIs only; no `unsafe` outside
/// [`crate::ffi`].
fn poll_wait(deadline: Instant) {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return;
    }
    let step_ms = poll_step_ms(remaining.as_millis().min(u128::from(u64::MAX)) as u64);
    #[cfg(target_os = "macos")]
    {
        if let Some(mtm) = objc2::MainThreadMarker::new() {
            objc2::rc::autoreleasepool(|_| {
                // `sharedApplication` here is a benign singleton handle
                // (no activation, no windows, no `finishLaunching`): in the
                // `raw` init path it only exists to let the drain service
                // queued application events during the wait. In the
                // `appkit-accessory` path it returns the already-finished
                // singleton. Either way nothing activates or steals focus.
                let app = objc2_app_kit::NSApplication::sharedApplication(mtm);
                crate::ffi::drain_app_events(&app);
                crate::ffi::pump_runloop_once(step_ms);
            });
            return;
        }
    }
    std::thread::sleep(Duration::from_millis(step_ms));
}

/// Join the owned helper's window: intersect the fresh CG owner-PID set
/// with the live AX window-ID set for the same PID. A regular-policy AppKit
/// app may own several CG windows (app chrome beyond the probe window), so
/// a bare "exactly 1 CG window" gate is wrong here; the intersection with
/// the AX identity join (`_AXUIElementGetWindow`) picks the verified probe
/// window without titles and without guessing. Exactly one intersection is
/// required — zero or several fails closed with the counts as evidence.
/// Every `AxElement` is constructed, used, and dropped on a dedicated
/// worker; the caller waits bounded.
pub(crate) fn join_helper_wid(pid: u32) -> Result<u32, FocusError> {
    if pid <= 1 {
        return Err(FocusError::Invalid(format!("refusing reserved pid {pid}")));
    }
    let list = crate::ffi::cg_window_list_all()
        .map_err(|e| FocusError::System(format!("CG snapshot: {e}")))?;
    let cg_wids: Vec<u32> = list
        .iter()
        .filter(|w| w.pid == pid)
        .map(|w| w.wid)
        .collect();
    if cg_wids.is_empty() {
        return Err(FocusError::Ownership(format!(
            "no live CG window for owned pid {pid}; refusing"
        )));
    }
    let pid_i32: i32 = pid
        .try_into()
        .map_err(|_| FocusError::Invalid(format!("pid {pid} out of range")))?;
    let (done_tx, done_rx) = std::sync::mpsc::channel::<Result<Vec<u32>, String>>();
    if std::thread::Builder::new()
        .name(format!("focus-join-{pid}"))
        .spawn(move || {
            // Every AX handle lives and dies on this worker. A single
            // failed `_AXUIElementGetWindow` join fails the whole join:
            // silently dropping it could intersect down to a wrong window.
            let wids: Result<Vec<u32>, String> = (|| {
                let app = crate::ffi::AxElement::application(pid_i32)
                    .ok_or_else(|| format!("no AX element for pid {pid}"))?;
                let _ = crate::ffi::ax_set_messaging_timeout(&app, 2.0);
                let windows = app.windows().map_err(crate::ffi::ax_error_name)?;
                let mut ids = Vec::with_capacity(windows.len());
                for window in &windows {
                    ids.push(
                        crate::ffi::ax_window_id_detailed(window)
                            .map_err(|e| format!("owned AX window join: {e}"))?,
                    );
                }
                Ok(ids)
            })();
            let _ = done_tx.send(wids);
        })
        .is_err()
    {
        return Err(FocusError::System("spawn AX join worker".to_owned()));
    }
    let ax_wids = match done_rx.recv_timeout(Duration::from_secs(15)) {
        Ok(Ok(wids)) => wids,
        Ok(Err(e)) => {
            return Err(FocusError::System(format!("owned AX window join: {e}")));
        }
        Err(_) => {
            return Err(FocusError::System(
                "owned AX window join timed out (bounded 15s)".to_owned(),
            ));
        }
    };
    let mut matched: Vec<u32> = cg_wids
        .into_iter()
        .filter(|w| ax_wids.contains(w))
        .collect();
    matched.sort_unstable();
    matched.dedup();
    match matched.len() {
        1 => Ok(matched[0]),
        n => Err(FocusError::Disagreement(format!(
            "owned pid {pid}: {n} CG∩AX windows (need exactly 1); refusing without exact identity"
        ))),
    }
}

/// Read-only Space snapshot: per-display membership in display order (see
/// [`SpaceSnapshot`]).
pub fn capture_space() -> Result<SpaceSnapshot, String> {
    let d = crate::spaces::diagnose().map_err(|e| e.to_string())?;
    Ok(SpaceSnapshot {
        displays: d
            .displays
            .into_iter()
            .map(|x| DisplaySpaces {
                spaces: x.spaces,
                current: x.current,
            })
            .collect(),
    })
}

// -- Bounded holder session (exact Child, live proof, same-route restore) --

const FOCUS_POLL: Duration = Duration::from_secs(5);
const STOP_POLL_MS: u64 = 5_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HolderStop {
    pub pid: u32,
    pub reaped: bool,
    pub record_removed: bool,
    pub process_gone: bool,
    pub cg_residue: Option<bool>,
    /// True only when the exit was verified (observed status or
    /// poll-observed exit). An unverified exit retains the record.
    pub verified_exit: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PreFocusRestore {
    pub dispatch: FocusDispatch,
    pub agreement: FocusAgreement,
    /// Unfocus dispatch start through original agreement (not poll-only).
    pub latency_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FocusSessionReport {
    pub bundle: String,
    /// Initialization path for this run: `raw` (default, no AppKit) or
    /// `appkit-accessory` (explicit diagnostic `--appkit`: Accessory +
    /// `finishLaunching` plus a bounded pump, equal to the tile
    /// lifecycle; no windows, no activation, no focus effects).
    pub init_path: String,
    pub trust_observed: Option<bool>,
    pub slps_available: bool,
    pub helper_pid: u32,
    pub helper_wid: u32,
    pub regular_policy: bool,
    /// Whether the spawn itself held frontmost before the measured dispatch
    /// (regular policy may activate at launch; `None` when unreadable).
    /// Observed evidence only — the proof below is dispatch-through-agreement.
    pub helper_frontmost_at_spawn: Option<bool>,
    /// Present only when the helper already held full agreement at spawn: the
    /// SAME-route unfocus back to the original (dispatch + agreement) that
    /// ran BEFORE the measured focus dispatch, so the measured leg starts
    /// from a proven-unfocused state and is never vacuous. `None` when the
    /// helper was not focused at spawn (measured leg already non-vacuous).
    pub pre_focus_restore: Option<PreFocusRestore>,
    pub original: OriginalPreimage,
    pub space_before: SpaceSnapshot,
    pub focus_dispatch: FocusDispatch,
    pub focus_agreement: FocusAgreement,
    /// Dispatch start through agreement (not poll-only).
    pub focus_latency_ms: u64,
    pub restore_dispatch: FocusDispatch,
    pub restore_agreement: FocusAgreement,
    /// Restore dispatch start through restore agreement.
    pub restore_latency_ms: u64,
    pub space_after: SpaceSnapshot,
    pub space_unchanged: bool,
    /// Original agreement read AFTER the helper stop (`None` explicitly
    /// marks unrestored). The success path fails when this is `None`.
    pub post_stop_readback: Option<FocusAgreement>,
    pub stop: HolderStop,
}

/// Three-way interpretation of one `try_wait` observation. `Unknown` (a wait
/// error) is never an exit and must not cause a raw-PID signal: resolution
/// uses the exact handle only.
#[derive(Debug, Clone, PartialEq, Eq)]
enum WaitState {
    Live,
    Exited,
    Unknown(String),
}

fn classify_wait(result: &std::io::Result<Option<std::process::ExitStatus>>) -> WaitState {
    match result {
        Ok(Some(_)) => WaitState::Exited,
        Ok(None) => WaitState::Live,
        Err(e) => WaitState::Unknown(format!("owned child wait failed: {e}")),
    }
}

fn poll_exit(child: &mut std::process::Child, timeout_ms: u64) -> bool {
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return true,
            Ok(None) => {
                if Instant::now() >= deadline {
                    return matches!(child.try_wait(), Ok(Some(_)));
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            // Unknown is never an exit by itself.
            Err(_) => return false,
        }
    }
}

/// Verified exact-Child stop. Signals go only to a liveness-checked held
/// handle: an already-exited handle is never signalled, and an unknown wait
/// observation never causes a raw-PID signal (exact-handle kill + poll
/// only). Reap runs only after an observed exit; the spawn record is removed
/// only on a verified exit (observed status or poll-observed exit) and is
/// retained otherwise for inspection.
pub(crate) fn stop_verified(child: &mut std::process::Child, pid: u32) -> HolderStop {
    let mut out = HolderStop {
        pid,
        reaped: false,
        record_removed: false,
        process_gone: false,
        cg_residue: None,
        verified_exit: false,
    };
    let exited: bool = match classify_wait(&child.try_wait()) {
        WaitState::Exited => true,
        WaitState::Live => {
            // Exact held handle observed live: PID signal is bound to this
            // handle's liveness (never a guess, never Ghostty). A failed
            // delivery still polls: the child may have exited on its own.
            let _ = crate::ffi::terminate_process(pid as i32);
            if poll_exit(child, STOP_POLL_MS) {
                true
            } else {
                let _ = child.kill();
                poll_exit(child, STOP_POLL_MS)
            }
        }
        WaitState::Unknown(_) => {
            // No raw-PID signal on unknown state: exact-handle kill + poll
            // only; the error itself is never evidence of exit.
            let _ = child.kill();
            poll_exit(child, STOP_POLL_MS)
        }
    };
    out.verified_exit = exited;
    out.reaped = match child.try_wait() {
        Ok(Some(_)) => child.wait().is_ok(),
        _ => false,
    };
    if out.verified_exit {
        let path = crate::probe_target::session_record_path(pid);
        let _ = std::fs::remove_file(&path);
        out.record_removed = !path.exists();
    }
    out.process_gone = !crate::ffi::process_exists(pid as i32);
    out.cg_residue = crate::ffi::cg_window_list_all()
        .ok()
        .map(|list| list.iter().any(|w| w.pid == pid));
    out
}

/// Recheck the held Child plus executable + CG join immediately before the
/// target mutation. The `try_wait` Unknown state fails closed (no signal,
/// no mutation); an exited Child, a changed executable, or a lost CG join
/// refuses the dispatch.
pub(crate) fn recheck_holder_before_mutation(
    child: &mut std::process::Child,
    pid: u32,
    exe: &str,
    wid: u32,
) -> Result<(), FocusError> {
    match classify_wait(&child.try_wait()) {
        WaitState::Live => {}
        WaitState::Exited => {
            return Err(FocusError::Ownership(format!(
                "owned helper pid {pid} exited before mutation; refusing"
            )));
        }
        WaitState::Unknown(e) => {
            return Err(FocusError::System(format!(
                "owned holder wait state unknown ({e}); refusing without verified liveness"
            )));
        }
    }
    crate::ownership::verify_live_process(pid, Some(exe))
        .map(|_| ())
        .map_err(FocusError::Ownership)?;
    crate::cg::verify_wid_owned_by_pid(pid, wid)
        .map(|_| ())
        .map_err(|e| FocusError::Ownership(format!("holder CG join refused: {e}")))?;
    Ok(())
}

/// SAME-route restore attempt for post-spawn failure paths: revalidate the
/// recorded original currency, dispatch the SLPS route to it, and poll for
/// agreement. This is an ATTEMPT subject to original identity currency, not
/// a guaranteed restoration. Every leg records its own outcome with the
/// cause: exactly one of `dispatch` / `dispatch_error` / `skipped_reason`
/// is set, and after a dispatch exactly one of `agreement` /
/// `agreement_error` is set. `None` fields mean that leg was not reached —
/// never agreement by default. Callers report the whole struct alongside
/// cleanup evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RestoreAttempt {
    /// The restore dispatch on success; `None` otherwise.
    pub dispatch: Option<FocusDispatch>,
    /// The dispatch failure cause when a dispatch was attempted and failed;
    /// `None` otherwise.
    pub dispatch_error: Option<String>,
    /// Why no dispatch was attempted (stale original currency refused before
    /// any desktop effect); `None` when a dispatch was attempted.
    pub skipped_reason: Option<String>,
    /// The restore agreement polled after a successful dispatch; `None`
    /// otherwise.
    pub agreement: Option<FocusAgreement>,
    /// The agreement failure cause when agreement was awaited and failed;
    /// `None` otherwise.
    pub agreement_error: Option<String>,
}

/// SAME-route restore attempt for post-spawn failure paths (see
/// [`RestoreAttempt`]).
pub(crate) fn attempt_restore(original: &OriginalPreimage) -> RestoreAttempt {
    if let Err(e) = verify_original_currency(original) {
        return RestoreAttempt {
            dispatch: None,
            dispatch_error: None,
            skipped_reason: Some(format!(
                "original currency refused ({e}); no dispatch attempted"
            )),
            agreement: None,
            agreement_error: None,
        };
    }
    let dispatch = match dispatch_focus(original.pid as u32, original.wid) {
        Ok(d) => d,
        Err(e) => {
            return RestoreAttempt {
                dispatch: None,
                dispatch_error: Some(e.to_string()),
                skipped_reason: None,
                agreement: None,
                agreement_error: None,
            };
        }
    };
    match await_agreement(original.pid as u32, original.wid, FOCUS_POLL) {
        Ok((agreement, _)) => RestoreAttempt {
            dispatch: Some(dispatch),
            dispatch_error: None,
            skipped_reason: None,
            agreement: Some(agreement),
            agreement_error: None,
        },
        Err(e) => RestoreAttempt {
            dispatch: Some(dispatch),
            dispatch_error: None,
            skipped_reason: None,
            agreement: None,
            agreement_error: Some(e.to_string()),
        },
    }
}

/// Current agreement read for the recorded original, or `None` when the
/// recorded PID no longer converts or agreement fails. `None` marks
/// unrestored — never agreement.
fn original_readback(original: &OriginalPreimage) -> Option<FocusAgreement> {
    let pid = u32::try_from(original.pid).ok()?;
    read_agreement(pid, original.wid).ok()
}

/// Stop the helper, then read back the original agreement AFTER the stop.
/// Pre-stop readbacks cannot observe focus moves caused by the helper's own
/// exit, so every failure path reports this post-stop readback alongside
/// the stop evidence; `None` explicitly marks unrestored.
pub(crate) fn stop_and_readback(
    raw: &mut crate::probe_target::UnreadySpawn,
    original: &OriginalPreimage,
) -> (SpaceSnapshot, HolderStop, Option<FocusAgreement>) {
    let space_after = capture_space().unwrap_or(SpaceSnapshot { displays: vec![] });
    let stop = stop_verified(&mut raw.child, raw.pid);
    (space_after, stop, original_readback(original))
}

/// Explicit diagnostic AppKit lifecycle equal to the tile controller:
/// Accessory policy + `finishLaunching` plus a bounded pump (drain + 50ms
/// slices, no `run`), no windows, no activation, no focus effects, no
/// settings writes. Used only behind the `--appkit` opt-in so a future
/// live run can discriminate initialization; the default path never calls
/// this. Main thread only.
pub(crate) fn init_appkit_accessory_for_diagnose() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        use objc2::MainThreadMarker;
        let mtm = MainThreadMarker::new()
            .ok_or_else(|| "appkit diagnostic requires the main thread".to_owned())?;
        let app = objc2_app_kit::NSApplication::sharedApplication(mtm);
        app.setActivationPolicy(objc2_app_kit::NSApplicationActivationPolicy::Accessory);
        app.finishLaunching();
        for _ in 0..4 {
            objc2::rc::autoreleasepool(|_| {
                crate::ffi::drain_app_events(&app);
                crate::ffi::pump_runloop_once(50);
            });
        }
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err("appkit diagnostic is macOS-only".to_owned())
    }
}

/// Bounded owned focus session over the regular-policy fixture: throwaway
/// spawn, original preimage with agreement, pre-mutation holder recheck,
/// SLPS focus to the owned window with workspace+AX agreement, SAME-route
/// restore with workspace+AX agreement, per-display Space unchanged,
/// verified exact-Child cleanup. Every post-dispatch failure preserves the
/// restoration readback and cleanup evidence. No taps, no synthetic keys, no
/// SA, no activation API anywhere on this path.
///
/// `appkit` selects the initialization path, labelled in the report as
/// `init_path`: `false` (default) runs raw with no AppKit initialization;
/// `true` runs the explicit diagnostic AppKit lifecycle equal to the tile
/// controller (Accessory + `finishLaunching` plus a bounded pump, no
/// windows, no activation, no focus effects) so a future live run can
/// discriminate initialization.
pub fn run_focus_session(seconds: u64, appkit: bool) -> Result<String, (i32, String)> {
    crate::config::ProbeConfig::validate_probe_seconds(seconds).map_err(|e| (2, e))?;
    let init_path = if appkit {
        init_appkit_accessory_for_diagnose().map_err(|e| (2, e))?;
        "appkit-accessory".to_owned()
    } else {
        "raw".to_owned()
    };
    let err_json = |stage: &str, error: String| -> (i32, String) {
        (
            1,
            serde_json::json!({"stage": stage, "error": error}).to_string(),
        )
    };
    if !crate::ffi::slps_available() {
        return Err(err_json(
            "slps-unavailable",
            "SLPS focus symbols unavailable; stop — no SA, no policy change".to_owned(),
        ));
    }
    let trust = crate::ffi::ax_is_process_trusted();
    if trust != Some(true) {
        return Err(err_json(
            "ax-trust",
            format!("AX trust not observed true ({trust:?}); stop — user consent needed"),
        ));
    }
    // Original preimage + Space baseline come BEFORE the spawn: the
    // regular-policy fixture may take frontmost at launch, which must never
    // invalidate the foreign preimage it will be measured against. The
    // preimage polls briefly (bounded): at probe launch the window server
    // may transiently report no focused application/window while frontmost
    // settles back to the foreign app. A persistently unreadable preimage
    // (no consent, no window) fails closed after the bound — never guessed.
    // No mutation has occurred at this point, so no restore is owed.
    let original = {
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            match capture_original(None) {
                Ok(o) => break o,
                Err(e) => {
                    if Instant::now() >= deadline {
                        return Err(err_json("preimage", e.to_string()));
                    }
                    std::thread::sleep(Duration::from_millis(200));
                }
            }
        }
    };
    let space_before = match capture_space() {
        Ok(s) => s,
        Err(e) => return Err(err_json("space-before", e)),
    };
    let nonce = crate::probe_target::mint_nonce().map_err(|e| err_json("spawn", e))?;
    let spec = crate::probe_target::ProbeTargetSpec::new(&nonce, 0, 0, seconds)
        .map(|s| s.with_regular_policy(true))
        .map_err(|e| err_json("spawn", e))?;
    let mut raw =
        crate::probe_target::start_helper_process(&spec).map_err(|e| err_json("spawn", e))?;
    let pid = raw.pid;
    let helper_path = raw.helper_path.clone();
    if let Err(e) = crate::probe_target::await_handshake(&mut raw, &spec) {
        let restore_attempt = attempt_restore(&original);
        let (space_after, stop, post_stop_readback) = stop_and_readback(&mut raw, &original);
        return Err((
            1,
            serde_json::json!({"stage": "handshake", "error": e, "restore_attempt": restore_attempt, "space_before": space_before, "space_after": space_after, "stop": stop, "post_stop_readback": post_stop_readback}).to_string(),
        ));
    }
    if let Err(e) = crate::probe_target::record_spawn(pid, &spec, &helper_path) {
        let restore_attempt = attempt_restore(&original);
        let (space_after, stop, post_stop_readback) = stop_and_readback(&mut raw, &original);
        return Err((
            1,
            serde_json::json!({"stage": "record", "error": e, "restore_attempt": restore_attempt, "space_before": space_before, "space_after": space_after, "stop": stop, "post_stop_readback": post_stop_readback}).to_string(),
        ));
    }
    // Owned window join: CG owner-PID set intersected with the live AX
    // identity join for the same PID (never a guessed WID; a regular-policy
    // app owns app-chrome CG windows beyond the probe window). An ownership
    // failure here refuses the target mutation below — no dispatch follows.
    let wid: u32 = match join_helper_wid(pid) {
        Ok(w) => w,
        Err(e) => {
            let restore_attempt = attempt_restore(&original);
            let (space_after, stop, post_stop_readback) = stop_and_readback(&mut raw, &original);
            return Err((
                1,
                serde_json::json!({"stage": "cg-join", "error": e.to_string(), "restore_attempt": restore_attempt, "space_before": space_before, "space_after": space_after, "stop": stop, "post_stop_readback": post_stop_readback})
                    .to_string(),
            ));
        }
    };
    if let Err(e) = crate::cg::verify_wid_owned_by_pid(pid, wid).map(|_| ()) {
        let restore_attempt = attempt_restore(&original);
        let (space_after, stop, post_stop_readback) = stop_and_readback(&mut raw, &original);
        return Err((
            1,
            serde_json::json!({"stage": "cg-join", "error": e.to_string(), "restore_attempt": restore_attempt, "space_before": space_before, "space_after": space_after, "stop": stop, "post_stop_readback": post_stop_readback})
                .to_string(),
        ));
    }
    // Space/display baseline re-read after the join (spawn must not move
    // Spaces; the pre-spawn read above stays the restoration reference).
    let space_joined = match capture_space() {
        Ok(s) => s,
        Err(e) => {
            let restore_attempt = attempt_restore(&original);
            let (space_after, stop, post_stop_readback) = stop_and_readback(&mut raw, &original);
            return Err((
                1,
                serde_json::json!({"stage": "space-joined", "error": e, "restore_attempt": restore_attempt, "space_before": space_before, "space_after": space_after, "stop": stop, "post_stop_readback": post_stop_readback}).to_string(),
            ));
        }
    };
    if space_joined != space_before {
        let restore_attempt = attempt_restore(&original);
        let (space_after, stop, post_stop_readback) = stop_and_readback(&mut raw, &original);
        return Err((
            1,
            serde_json::json!({"stage": "space-joined", "error": "spawn moved Spaces; refusing", "space_before": space_before, "space_joined": space_joined, "restore_attempt": restore_attempt, "space_after": space_after, "stop": stop, "post_stop_readback": post_stop_readback}).to_string(),
        ));
    }
    // Whether spawn itself took frontmost (regular policy may activate at
    // launch): observed evidence only — the measured proof below is still
    // dispatch-through-agreement on the SAME route.
    let helper_frontmost_at_spawn: Option<bool> =
        crate::ffi::frontmost_app().map(|(p, _)| i64::from(p) == i64::from(pid));
    // Holder + executable + CG recheck immediately before the mutation. An
    // ownership failure refuses the dispatch below (no target mutation); the
    // SAME-route restore is still attempted subject to original currency
    // before cleanup.
    if let Err(e) = recheck_holder_before_mutation(&mut raw.child, pid, &helper_path, wid) {
        let restore_attempt = attempt_restore(&original);
        let (space_after, stop, post_stop_readback) = stop_and_readback(&mut raw, &original);
        return Err((
            1,
            serde_json::json!({"stage": "pre-mutation", "error": e.to_string(), "restore_attempt": restore_attempt, "space_before": space_before, "space_after": space_after, "stop": stop, "post_stop_readback": post_stop_readback}).to_string(),
        ));
    }
    // Non-vacuous proof: the regular-policy fixture may already hold FULL
    // agreement from spawn-time activation. A dispatch to an already-focused
    // target proves nothing, so when the helper already agrees, restore the
    // original via the SAME route with agreement FIRST (unfocus with proof);
    // the measured dispatch below then starts from a proven-unfocused state.
    // When the helper is not focused at spawn this leg is skipped (`None`)
    // and the measured leg below is already non-vacuous.
    let pre_focus_restore: Option<PreFocusRestore> = if read_agreement(pid, wid).is_ok() {
        let t_pre = Instant::now();
        if let Err(e) = verify_original_currency(&original) {
            let restore_readback = original_readback(&original);
            let (space_after, stop, post_stop_readback) = stop_and_readback(&mut raw, &original);
            return Err((
                1,
                serde_json::json!({
                    "stage": "pre-restore-currency",
                    "error": format!("{e} (FOCUS NOT RESTORED — residue: helper pid {pid} may hold focus)"),
                    "restore_readback": restore_readback,
                    "space_before": space_before,
                    "space_after": space_after,
                    "stop": stop, "post_stop_readback": post_stop_readback,
                })
                .to_string(),
            ));
        }
        let pre_dispatch = match dispatch_focus(original.pid as u32, original.wid) {
            Ok(d) => d,
            Err(e) => {
                let restore_readback = original_readback(&original);
                let (space_after, stop, post_stop_readback) =
                    stop_and_readback(&mut raw, &original);
                return Err((
                    1,
                    serde_json::json!({
                        "stage": "pre-restore-dispatch",
                        "error": format!("{e} (FOCUS NOT RESTORED — residue: helper pid {pid} may hold focus)"),
                        "restore_readback": restore_readback,
                        "space_before": space_before,
                        "space_after": space_after,
                        "stop": stop, "post_stop_readback": post_stop_readback,
                    })
                    .to_string(),
                ));
            }
        };
        let pre_agreement = match await_agreement(original.pid as u32, original.wid, FOCUS_POLL) {
            Ok((agreement, _)) => agreement,
            Err(e) => {
                let restore_readback = original_readback(&original);
                let (space_after, stop, post_stop_readback) =
                    stop_and_readback(&mut raw, &original);
                return Err((
                    1,
                    serde_json::json!({
                        "stage": "pre-restore-agreement",
                        "error": format!("PRE-RESTORE FAILED: {e} (helper pid {pid} may hold focus) — STOP"),
                        "pre_dispatch": pre_dispatch,
                        "restore_readback": restore_readback,
                        "space_before": space_before,
                        "space_after": space_after,
                        "stop": stop, "post_stop_readback": post_stop_readback,
                    })
                    .to_string(),
                ));
            }
        };
        // The unfocus waited: re-verify the holder is still ours before the
        // measured mutation (PID-reuse guard across the wait). Ownership
        // failure refuses the dispatch; the original already agrees above.
        if let Err(e) = recheck_holder_before_mutation(&mut raw.child, pid, &helper_path, wid) {
            let (space_after, stop, post_stop_readback) = stop_and_readback(&mut raw, &original);
            return Err((
                1,
                serde_json::json!({"stage": "pre-mutation-recheck", "error": e.to_string(), "pre_focus_restore": PreFocusRestore { dispatch: pre_dispatch, agreement: pre_agreement, latency_ms: latency_ms(t_pre) }, "space_before": space_before, "space_after": space_after, "stop": stop, "post_stop_readback": post_stop_readback}).to_string(),
            ));
        }
        Some(PreFocusRestore {
            dispatch: pre_dispatch,
            agreement: pre_agreement,
            latency_ms: latency_ms(t_pre),
        })
    } else {
        None
    };
    // Focus the owned window via the SLPS route (no activation API).
    // Latency runs dispatch start through agreement.
    let t_focus = Instant::now();
    let focus_dispatch = match dispatch_focus(pid, wid) {
        Ok(d) => d,
        Err(e) => {
            // Set-front may have acted before a post failure: attempt the
            // SAME-route restore with its readback, then verified cleanup.
            let restore_attempt = attempt_restore(&original);
            let (space_after, stop, post_stop_readback) = stop_and_readback(&mut raw, &original);
            return Err((
                1,
                serde_json::json!({
                    "stage": "focus-dispatch",
                    "error": e.to_string(),
                    "focus_dispatch": serde_json::Value::Null,
                    "pre_focus_restore": pre_focus_restore,
                    "restore_attempt": restore_attempt,
                    "space_before": space_before,
                    "space_after": space_after,
                    "stop": stop, "post_stop_readback": post_stop_readback,
                })
                .to_string(),
            ));
        }
    };
    let focus_agreement = match await_agreement(pid, wid, FOCUS_POLL) {
        Ok((agreement, _)) => agreement,
        Err(e) => {
            let restore_attempt = attempt_restore(&original);
            let (space_after, stop, post_stop_readback) = stop_and_readback(&mut raw, &original);
            return Err((
                1,
                serde_json::json!({
                    "stage": "focus-agreement",
                    "error": e.to_string(),
                    "focus_dispatch": focus_dispatch,
                    "pre_focus_restore": pre_focus_restore,
                    "restore_attempt": restore_attempt,
                    "space_before": space_before,
                    "space_after": space_after,
                    "stop": stop, "post_stop_readback": post_stop_readback,
                })
                .to_string(),
            ));
        }
    };
    let focus_latency = latency_ms(t_focus);
    // Restore currency first: stale PID/WID never dispatches.
    if let Err(e) = verify_original_currency(&original) {
        let restore_readback = original_readback(&original);
        let (space_after, stop, post_stop_readback) = stop_and_readback(&mut raw, &original);
        return Err((
            1,
            serde_json::json!({
                    "stage": "restore-currency",
                    "error": format!("{e} (FOCUS NOT RESTORED — residue: helper pid {pid} may hold focus)"),
                    "focus_dispatch": focus_dispatch,
                    "focus_agreement": focus_agreement,
                    "pre_focus_restore": pre_focus_restore,
                    "restore_readback": restore_readback,
                "space_before": space_before,
                "space_after": space_after,
                "stop": stop, "post_stop_readback": post_stop_readback,
            })
            .to_string(),
        ));
    }
    // Restore the original via the SAME route (never an activation API).
    let t_restore = Instant::now();
    let restore_dispatch = match dispatch_focus(original.pid as u32, original.wid) {
        Ok(d) => d,
        Err(e) => {
            let restore_readback = original_readback(&original);
            let (space_after, stop, post_stop_readback) = stop_and_readback(&mut raw, &original);
            return Err((
                1,
                serde_json::json!({
                    "stage": "restore-dispatch",
                    "error": format!("{e} (FOCUS NOT RESTORED — residue: helper pid {pid} may hold focus)"),
                    "focus_dispatch": focus_dispatch,
                    "focus_agreement": focus_agreement,
                    "pre_focus_restore": pre_focus_restore,
                    "restore_readback": restore_readback,
                    "space_before": space_before,
                    "space_after": space_after,
                    "stop": stop, "post_stop_readback": post_stop_readback,
                })
                .to_string(),
            ));
        }
    };
    let restore_agreement = match await_agreement(original.pid as u32, original.wid, FOCUS_POLL) {
        Ok((agreement, _)) => agreement,
        Err(e) => {
            let restore_readback = original_readback(&original);
            let (space_after, stop, post_stop_readback) = stop_and_readback(&mut raw, &original);
            return Err((
                1,
                serde_json::json!({
                    "stage": "restore-agreement",
                    "error": format!("RESTORE FAILED: {e} (helper pid {pid} may hold focus) — STOP"),
                    "focus_dispatch": focus_dispatch,
                    "focus_agreement": focus_agreement,
                    "pre_focus_restore": pre_focus_restore,
                    "restore_dispatch": restore_dispatch,
                    "restore_readback": restore_readback,
                    "space_before": space_before,
                    "space_after": space_after,
                    "stop": stop, "post_stop_readback": post_stop_readback,
                })
                .to_string(),
            ));
        }
    };
    let restore_latency = latency_ms(t_restore);
    // Space unchanged readback (per-display, order-preserving) + verified
    // cleanup + post-stop agreement: killing the helper must not move focus
    // elsewhere, so the original must still agree AFTER the stop.
    let space_after = capture_space().unwrap_or(SpaceSnapshot { displays: vec![] });
    let space_unchanged = space_after == space_before;
    let mut child = raw.child;
    let stop = stop_verified(&mut child, pid);
    let post_stop_readback = original_readback(&original);
    let post_stop_ok = post_stop_readback.is_some();
    let report = FocusSessionReport {
        bundle: crate::BUNDLE_ID.to_owned(),
        init_path: init_path.clone(),
        trust_observed: trust,
        slps_available: true,
        helper_pid: pid,
        helper_wid: wid,
        regular_policy: true,
        helper_frontmost_at_spawn,
        pre_focus_restore,
        original,
        space_before,
        focus_dispatch,
        focus_agreement,
        focus_latency_ms: focus_latency,
        restore_dispatch,
        restore_agreement,
        restore_latency_ms: restore_latency,
        space_after,
        space_unchanged,
        post_stop_readback,
        stop: stop.clone(),
    };
    if !space_unchanged {
        return Err((
            1,
            serde_json::json!({"stage": "space-changed", "report": report}).to_string(),
        ));
    }
    if !(stop.verified_exit
        && stop.process_gone
        && stop.record_removed
        && stop.cg_residue == Some(false))
    {
        return Err((
            1,
            serde_json::json!({"stage": "cleanup-residue", "report": report}).to_string(),
        ));
    }
    if !post_stop_ok {
        return Err((
            1,
            serde_json::json!({"stage": "post-stop-disagreement", "error": "original no longer agrees after helper stop; focus may have moved — STOP", "report": report}).to_string(),
        ));
    }
    serde_json::to_string(&report).map_err(|e| (1, e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_bytes_match_yabai_make_key_window() {
        // yabai window_manager_make_key_window: 0xf8 zeroed, [0x04]=0xf8,
        // [0x3a]=0x10, wid memcpy at 0x3c, 0xff fill at 0x20..0x30,
        // [0x08]=phase per post.
        for (wid, phase) in [(0x1234_5678u32, 0x01u8), (7u32, 0x02u8)] {
            let b = make_key_event_bytes(wid, phase);
            assert_eq!(b.len(), 0xf8);
            assert_eq!(b[0x04], 0xf8);
            assert_eq!(b[0x3a], 0x10);
            assert_eq!(b[0x08], phase);
            assert_eq!(&b[0x3c..0x3c + 4], &wid.to_ne_bytes());
            assert!(b[0x20..0x30].iter().all(|x| *x == 0xff));
            // Zeroed elsewhere (spot-check, excluding written fields).
            assert_eq!(b[0x00], 0);
            assert_eq!(b[0x3a - 1], 0);
            assert_eq!(b[0xf7], 0);
        }
        // Phases differ only in [0x08].
        let a = make_key_event_bytes(9, 0x01);
        let b = make_key_event_bytes(9, 0x02);
        assert_eq!(a[0x08], 0x01);
        assert_eq!(b[0x08], 0x02);
        let mut am = a;
        am[0x08] = 0x02;
        assert_eq!(am, b);
    }

    #[test]
    fn dispatch_refuses_reserved_and_zero_before_ffi() {
        assert!(matches!(
            dispatch_focus(1, 7).unwrap_err(),
            FocusError::Invalid(_)
        ));
        assert!(matches!(
            dispatch_focus(4242, 0).unwrap_err(),
            FocusError::Invalid(_)
        ));
    }

    #[test]
    fn dispatch_rejects_out_of_range_pid_before_any_ffi() {
        // Pure range validation: out-of-range PIDs never reach the symbol
        // probe, PSN lookup, or dispatch on any platform — no desktop effect.
        let err = dispatch_focus(u32::MAX, 7).unwrap_err();
        assert!(matches!(err, FocusError::Invalid(_)));
        assert!(err.to_string().contains("out of range"));
    }

    #[test]
    fn read_agreement_validates_ids_before_any_ffi() {
        // Pure validation: reserved PIDs and zero WIDs never reach the
        // window server on any platform — no desktop effect.
        assert!(matches!(
            read_agreement(0, 7).unwrap_err(),
            FocusError::Invalid(_)
        ));
        assert!(matches!(
            read_agreement(1, 7).unwrap_err(),
            FocusError::Invalid(_)
        ));
        assert!(matches!(
            read_agreement(4242, 0).unwrap_err(),
            FocusError::Invalid(_)
        ));
    }

    #[test]
    fn loginwindow_predicate_is_exact_bundle_match() {
        // Pure string predicate: no FFI, no desktop effect.
        assert!(is_loginwindow_bundle("com.apple.loginwindow"));
        assert!(is_loginwindow_bundle(LOGINWINDOW_BUNDLE));
        assert!(!is_loginwindow_bundle("com.apple.loginwindow "));
        assert!(!is_loginwindow_bundle(" com.apple.loginwindow"));
        assert!(!is_loginwindow_bundle("com.apple.finder"));
        assert!(!is_loginwindow_bundle(""));
    }

    #[test]
    fn focus_errors_are_meaningful() {
        // Display strings carry the kind + cause, never a bare code.
        let e = FocusError::Disagreement("workspace 1 != AX 2".to_owned());
        assert!(e.to_string().contains("disagreement"));
        assert!(e.to_string().contains("workspace"));
        let e = FocusError::Unavailable("symbols missing".to_owned());
        assert!(e.to_string().contains("unavailable"));
    }

    #[test]
    fn poll_step_never_exceeds_100ms_or_remaining() {
        // Pure step cap: no FFI, no desktop effect.
        assert_eq!(poll_step_ms(0), 0);
        assert_eq!(poll_step_ms(1), 1);
        assert_eq!(poll_step_ms(99), 99);
        assert_eq!(poll_step_ms(100), 100);
        assert_eq!(poll_step_ms(101), 100);
        assert_eq!(poll_step_ms(5_000), 100);
        assert_eq!(poll_step_ms(u64::MAX), 100);
    }

    #[test]
    fn await_agreement_invalid_ids_stay_bounded_without_live_effects() {
        // Reserved PIDs / zero WIDs fail validation before any FFI, so this
        // exercises the bounded poll wait with no desktop effect on any
        // platform. Tests run off the main thread, so the wait sleeps.
        let start = Instant::now();
        let err = await_agreement(0, 7, Duration::from_millis(200)).unwrap_err();
        let elapsed = start.elapsed();
        assert!(err.to_string().contains("timeout"));
        assert!(elapsed >= Duration::from_millis(150));
        assert!(elapsed < Duration::from_secs(2));
    }
}

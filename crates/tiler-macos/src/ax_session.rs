#![forbid(unsafe_code)]

//! Single bounded owned-AX session (exact-Child holder).
//!
//! Binds identity to the unreaped exact `Child` plus its pipe-proven nonce,
//! the live executable binding, and fresh CG owner-PID/WID joins — never the
//! cross-command AX-title fallback (which cannot serve an unresponsive target
//! and costs a worker per revalidation). Every mutating leg revalidates that
//! holder-grade binding at its write boundaries; a stale or reused identity
//! fails closed with the evidence retained.
//!
//! Focus legs use normal AppKit application activation of the owned helper
//! only (no synthetic keyboard/mouse, no window actions on anything). The
//! recorded pre-session frontmost application is restored the same way, and
//! a failed restoration stops the session with the residue reported.
//!
//! Session order: AX-consent gate, then the focus-restoration proof on a
//! throwaway spawn (gates everything else), then the responsive phase
//! (pre-window observer, driven move+resize, self-activation, raise with
//! readback, frontmost restore, exact-Child stop, merged notification
//! counts), then the fixed-size phase (controlled position-write / refused
//! size-write partial restoration). No leftover helper, record, tap, focus,
//! or geometry on any path. No hotkeys, taps, or Space writes anywhere here.

use serde::Serialize;
use std::time::{Duration, Instant};

#[cfg(target_os = "macos")]
use crate::ax::FrameTxOps;
use crate::ax::{AxError, FrameChangeReport, FrameTxFailure, RaiseReport};
#[cfg(target_os = "macos")]
use crate::ownership::OwnedIdentity;
use crate::probe_target::{
    ProbeTargetSpec, UnreadySpawn, await_handshake, mint_nonce, record_spawn, start_helper_process,
};

/// Per-observer pump window: covers attach retries, the handshake, driven
/// move/resize, self-activation, and the exact-Child stop with slack. Finite
/// and documented; the observer thread always terminates (finite runloop),
/// so joining it afterwards is bounded by construction.
const OBSERVE_WINDOW_SECS: u64 = 12;
/// Attach-retry budget for a pre-window observer: the app registers with AX
/// shortly after spawn, so a fast registration refusal retries briefly
/// instead of failing the session. Bounded; success pumps the full window.
const ATTACH_RETRY_TIMEOUT: Duration = Duration::from_secs(4);
/// Focus/activation poll bounds (async arrival, verified by readback).
const FOCUS_POLL_TIMEOUT: Duration = Duration::from_secs(5);
const FRONTMOST_POLL_TIMEOUT: Duration = Duration::from_secs(5);
/// Graceful-stop poll per stage (SIGTERM, then kill fallback).
const STOP_POLL_SECS: u64 = 5;
/// Owned self-close delay past the helper's `ready` marker. The drive (CG
/// join, one frame set, activation, raise, frontmost restore) completes in
/// ~1s on a live machine (all legs measured in the tens of milliseconds),
/// and the close-wait below starts right after the restore (~3.5s after
/// spawn with the 2s creation defer) polling up to 8s — so 6s lands the close
/// ~2.5s into the wait with margin on both sides, while both 12s observer
/// windows still pump. If the drive ever blew its own timeouts first, those
/// legs fail their verdicts first — a close arriving early then can only
/// remove an already-failed leg's window, never break a passing one
/// (frame/raise need the window; their timeouts bound them far under this
/// delay on any passing run).
const CLOSE_WINDOW_AFTER_READY_MS: u64 = 6000;
/// Total close-wait bound (marker drain plus CG-absence poll in one loop).
/// The drive completes ~3.5s after spawn and the close fires 6s past `ready`
/// (~8.5s after spawn), so the marker plus the absence land ~5s into the
/// wait; the bound covers slow scheduling with room while the joined
/// observer still pumps.
const CLOSE_WAIT_SECS: u64 = 10;

/// Frontmost application identity (PID plus bundle ID). No titles, no
/// content. The controlling terminal's identity is a restore target only —
/// its windows are never acted on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FrontmostIdentity {
    pub pid: i32,
    pub bundle: String,
}

/// Focus-restoration proof on a throwaway spawn. `restored` is a measured
/// frontmost readback equal to the recorded preimage, never the activation
/// request status alone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FocusProof {
    pub helper_pid: u32,
    pub original: FrontmostIdentity,
    pub activation_sent: bool,
    pub focused_owned: bool,
    pub focused_latency_ms: u64,
    pub restored: bool,
    pub restore_latency_ms: u64,
}

/// Exact-Child shutdown evidence. `record_removed` is true only after a
/// verified exit; `cg_residue` is `None` when the snapshot itself is
/// unavailable (unknown — never claimed clean).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HolderStop {
    pub pid: u32,
    pub graceful: bool,
    pub kill_fallback_used: bool,
    pub reaped: bool,
    pub record_removed: bool,
    pub process_gone: bool,
    pub cg_residue: Option<bool>,
}

/// Owned self-close readback: the helper closed its own window on its timer
/// while the joined observer pumped. `marker_seen` is the helper's own
/// execution report (`closed:true` plus its post-close own-PID window
/// count); `cg_absence` is the holder's independent readback that no
/// ON-SCREEN window of the helper PID remains; `lingering` records any
/// remaining off-screen entries for that PID explicitly (a closed window's
/// record can linger with `onscreen:false` while its live app keeps a
/// tombstone; process death clears it, proven by the stop block's residue
/// readback); `process_alive` proves the app outlived its window (exact held
/// handle un-reaped plus live exe binding). The destroyed-notification count
/// itself lives in the merged observer leg. Either report alone proves
/// nothing — the verdict requires all channels together.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LingeringWindow {
    pub wid: u32,
    pub onscreen: bool,
    pub layer: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CloseLeg {
    pub marker_seen: bool,
    pub marker_windows: Option<usize>,
    pub marker_visible: Option<bool>,
    pub cg_absence: bool,
    pub lingering: Vec<LingeringWindow>,
    pub absence_latency_ms: u64,
    pub process_alive: bool,
}

/// On-screen entries owned by exactly `pid` from a fresh snapshot. Pure;
/// unit-tested. A closed window's record may linger off-screen — that is
/// reported separately, never counted as presence.
fn onscreen_entries(windows: &[crate::ffi::CgWindow], pid: u32) -> Vec<LingeringWindow> {
    windows
        .iter()
        .filter(|w| w.pid == pid && w.onscreen)
        .map(|w| LingeringWindow {
            wid: w.wid,
            onscreen: w.onscreen,
            layer: w.layer,
        })
        .collect()
}

/// All entries owned by exactly `pid` (for the explicit lingering record).
fn pid_entries(windows: &[crate::ffi::CgWindow], pid: u32) -> Vec<LingeringWindow> {
    windows
        .iter()
        .filter(|w| w.pid == pid)
        .map(|w| LingeringWindow {
            wid: w.wid,
            onscreen: w.onscreen,
            layer: w.layer,
        })
        .collect()
}

/// Parse one helper report line into the post-close readbacks: the own-PID
/// window count plus the window's own visibility flag. `Some` only for the
/// exact `{"closed":true,...}` shape; handshake lines, garbage, or missing
/// fields are `None`. Pure; unit-tested.
fn parse_closed_marker(line: &str) -> Option<(usize, Option<bool>)> {
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    if value.get("closed").and_then(|c| c.as_bool()) != Some(true) {
        return None;
    }
    let windows = value
        .get("windows")
        .and_then(|w| w.as_u64())
        .map(|n| n.min(usize::MAX as u64) as usize)?;
    let visible = value.get("visible").and_then(|v| v.as_bool());
    Some((windows, visible))
}

/// Merged observer evidence from the pre-window and window-joined observers.
/// Per-name delivery counts; names absent from `received` delivered zero.
/// App-level names were registered on both observers, so their counts are
/// delivery totals across the two watches, not de-duplicated unique events.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ObserverLeg {
    pub added: Vec<String>,
    pub received: Vec<(String, u32)>,
    pub window_secs: u64,
}

/// Frame leg with its in-process latency (holder-side wall around the worker,
/// excluding any launch plumbing).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FrameLeg {
    pub report: Option<FrameChangeReport>,
    pub error: Option<String>,
    pub latency_ms: u64,
}

/// Raise leg with its in-process latency.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RaiseLeg {
    pub report: Option<RaiseReport>,
    pub error: Option<String>,
    pub latency_ms: u64,
}

/// Controlled partial-write leg: the full structured transaction failure
/// (preimage, attempted, observed partial, restore proof) plus latency.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PartialLeg {
    pub failure: Option<FrameTxFailure>,
    pub error: Option<String>,
    pub latency_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ResponsivePhase {
    pub pid: u32,
    pub wid: u32,
    pub cg_join_latency_ms: u64,
    pub observer: ObserverLeg,
    pub frame: FrameLeg,
    pub activation_sent: bool,
    pub activation_latency_ms: u64,
    pub raise: RaiseLeg,
    pub frontmost_restored: bool,
    pub frontmost_restore_latency_ms: u64,
    pub close: CloseLeg,
    pub stop: HolderStop,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FixedPhase {
    pub pid: u32,
    pub wid: u32,
    pub partial: PartialLeg,
    pub stop: HolderStop,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AxSessionReport {
    pub bundle: String,
    pub trust_observed: Option<bool>,
    pub focus_proof: FocusProof,
    pub responsive: ResponsivePhase,
    pub fixed: FixedPhase,
    pub engine_ax_events: u64,
}

fn latency_ms(start: Instant) -> u64 {
    start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64
}

/// Holder-grade revalidation: live executable binding plus a fresh CG WID
/// join. No titles anywhere on this path by design (the title fallback cannot
/// serve an unresponsive target). The exact-Child currency itself lives on
/// the holder thread (unreaped handle); this is the per-boundary live check.
fn holder_revalidate(pid: u32, wid: u32, exe: &str) -> Result<crate::ffi::CgWindow, String> {
    if pid <= 1 {
        return Err(format!("refusing reserved pid {pid}"));
    }
    crate::ownership::verify_live_process(pid, Some(exe))
        .map_err(|e| format!("holder liveness refused: {e}"))?;
    crate::cg::verify_wid_owned_by_pid(pid, wid).map_err(|e| format!("holder CG join refused: {e}"))
}

/// Poll the AX focused application until `want` (exact PID) or the timeout.
/// `None` on timeout — the caller re-reads for residue evidence when needed.
fn await_focused_pid(want: i32, timeout: Duration) -> Option<(i32, Option<u32>)> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some((pid, wid)) = crate::ffi::ax_focused_preimage()
            && pid == want
        {
            return Some((pid, wid));
        }
        if Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Poll the frontmost application until `want` (exact PID) or the timeout.
fn await_frontmost_pid(want: i32, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some((pid, _)) = crate::ffi::frontmost_app()
            && pid == want
        {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Bounded poll for the exact Child to exit. Never blocks past `timeout_ms`.
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
            Err(_) => return false,
        }
    }
}

/// Exact-Child graceful stop. SIGTERM goes only to a liveness-checked held
/// handle (an already-exited handle is never signalled); an exact-handle
/// kill fallback follows when graceful fails. Reap runs only after an
/// observed exit (never an unbounded wait on a live child); the spawn record
/// is removed only on a verified exit.
fn stop_graceful_exact(child: &mut std::process::Child, pid: u32) -> HolderStop {
    let mut out = HolderStop {
        pid,
        graceful: false,
        kill_fallback_used: false,
        reaped: false,
        record_removed: false,
        process_gone: false,
        cg_residue: None,
    };
    let mut exited = matches!(child.try_wait(), Ok(Some(_)));
    let mut signaled = false;
    if !exited {
        if crate::ffi::terminate_process(pid as i32) {
            signaled = true;
        }
        exited = poll_exit(child, STOP_POLL_SECS * 1000);
    }
    if !exited {
        out.kill_fallback_used = true;
        let _ = child.kill();
        exited = poll_exit(child, STOP_POLL_SECS * 1000);
    }
    out.graceful = signaled && exited && !out.kill_fallback_used;
    out.reaped = match child.try_wait() {
        Ok(Some(_)) => child.wait().is_ok(),
        _ => false,
    };
    if exited {
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

/// Focus-restoration proof on a bounded throwaway spawn. Records the live
/// frontmost identity, activates the owned helper with a normal AppKit
/// activation request (no synthetic input, no window actions), verifies the
/// AX focus arrival, then restores the recorded frontmost application the
/// same way and verifies the return. ANY failure — including a failed
/// restoration — stops the caller: the error names the residue instead of
/// proceeding to the driven phases.
fn prove_focus_restore(helper_seconds: u64) -> Result<FocusProof, String> {
    let err_json = |stage: &str, error: String, pid: Option<u32>| -> String {
        serde_json::json!({
            "stage": stage,
            "error": error,
            "helper_pid": pid,
        })
        .to_string()
    };
    let nonce = mint_nonce().map_err(|e| err_json("focus-proof-spawn", e, None))?;
    let spec = ProbeTargetSpec::new(&nonce, 0, 0, helper_seconds)
        .map_err(|e| err_json("focus-proof-spawn", e, None))?;
    let mut raw =
        start_helper_process(&spec).map_err(|e| err_json("focus-proof-spawn", e, None))?;
    let pid = raw.pid;
    let cleanup = |raw: &mut UnreadySpawn| stop_graceful_exact(&mut raw.child, raw.pid);
    if let Err(e) = await_handshake(&mut raw, &spec) {
        let stop = cleanup(&mut raw);
        return Err(err_json(
            "focus-proof-handshake",
            format!(
                "{e}; exact child cleaned (reaped={} record_removed={})",
                stop.reaped, stop.record_removed
            ),
            Some(pid),
        ));
    }
    if let Err(e) = record_spawn(pid, &spec, &raw.helper_path) {
        let stop = cleanup(&mut raw);
        return Err(err_json(
            "focus-proof-record",
            format!("{e}; exact child cleaned (reaped={})", stop.reaped),
            Some(pid),
        ));
    }
    // Baseline: the live frontmost identity must exist and must not already
    // be the helper — otherwise there is no foreign preimage to restore.
    let (orig_pid, orig_bundle) = match crate::ffi::frontmost_app() {
        Some(identity) => identity,
        None => {
            let stop = cleanup(&mut raw);
            return Err(err_json(
                "focus-proof-preimage",
                format!(
                    "frontmost identity unreadable; refusing focus change without a restorable preimage (child reaped={})",
                    stop.reaped
                ),
                Some(pid),
            ));
        }
    };
    if orig_pid == pid as i32 {
        let stop = cleanup(&mut raw);
        return Err(err_json(
            "focus-proof-preimage",
            format!(
                "frontmost is already the owned helper; refusing without a foreign preimage (child reaped={})",
                stop.reaped
            ),
            Some(pid),
        ));
    }
    // Owned self-activation: a normal activation request, verified by
    // readback — the request status alone proves nothing.
    let t_focus = Instant::now();
    let sent = crate::ffi::activate_app(pid as i32);
    let focused = if sent {
        await_focused_pid(pid as i32, FOCUS_POLL_TIMEOUT)
    } else {
        None
    };
    let focused_latency = latency_ms(t_focus);
    // Restore FIRST, report after: the recorded application is re-activated
    // (application activation only — never a window action on it) and the
    // return is verified by frontmost readback.
    let t_restore = Instant::now();
    let restore_sent = crate::ffi::activate_app(orig_pid);
    let restored = restore_sent && await_frontmost_pid(orig_pid, FRONTMOST_POLL_TIMEOUT);
    let restore_latency = latency_ms(t_restore);
    let stop = cleanup(&mut raw);
    let proof = FocusProof {
        helper_pid: pid,
        original: FrontmostIdentity {
            pid: orig_pid,
            bundle: orig_bundle,
        },
        activation_sent: sent,
        focused_owned: focused.is_some(),
        focused_latency_ms: focused_latency,
        restored,
        restore_latency_ms: restore_latency,
    };
    if focused.is_none() {
        return Err(err_json(
            "focus-proof-activate",
            format!(
                "helper activation did not land focus (sent={sent}); restore attempted={restored} evidence={}",
                serde_json::to_string(&proof).unwrap_or_default()
            ),
            Some(pid),
        ));
    }
    if !restored {
        return Err(err_json(
            "focus-proof-restore",
            format!(
                "FRONTMOST RESTORE FAILED (frontmost not returned to pid {orig_pid}); residue evidence={} stop={{\"reaped\":{},\"record_removed\":{},\"process_gone\":{}}} — STOP",
                serde_json::to_string(&proof).unwrap_or_default(),
                stop.reaped,
                stop.record_removed,
                stop.process_gone,
            ),
            Some(pid),
        ));
    }
    Ok(proof)
}

/// Holder-mode frame ops: one AX window element used on exactly one worker
/// thread, with holder-grade (Child + CG, title-free) revalidation at every
/// write boundary.
#[cfg(target_os = "macos")]
struct HolderFrameOps<'w> {
    window: &'w crate::ffi::AxElement,
    pid: u32,
    wid: u32,
    helper_exe: String,
}

#[cfg(target_os = "macos")]
impl FrameTxOps for HolderFrameOps<'_> {
    fn set_frame(&mut self, frame: &crate::ffi::AxFrame) -> Result<(), i32> {
        crate::ffi::ax_set_frame(self.window, frame)
    }

    fn get_frame(&mut self) -> Result<crate::ffi::AxFrame, i32> {
        crate::ffi::ax_get_frame(self.window)
    }

    fn revalidate(&mut self) -> Result<(), String> {
        holder_revalidate(self.pid, self.wid, &self.helper_exe).map(|_| ())
    }
}

/// Worker body for a holder frame attempt. Runs ONLY on a dedicated AX
/// worker: every `AxElement` is constructed, used, and dropped here. Binding
/// is the held Child's PID/nonce (pure derived identity) plus the live
/// bundle plus holder-grade revalidation — never cross-command title proof.
#[cfg(target_os = "macos")]
fn frame_attempt_on_worker(
    pid: u32,
    wid: u32,
    exe: &str,
    nonce: &str,
    frame: crate::ffi::AxFrame,
) -> Result<FrameChangeReport, AxError> {
    use crate::ffi::{AxElement, ax_get_frame, ax_window_id};
    let owned = OwnedIdentity::new(pid, crate::BUNDLE_ID, nonce).map_err(AxError::Ownership)?;
    let live_bundle = crate::ax::live_owned_bundle(pid)?;
    crate::ax::gate_owned_frame(&owned, pid, &live_bundle)?;
    holder_revalidate(pid, wid, exe).map_err(AxError::Ownership)?;
    let app = AxElement::application(pid as i32)
        .ok_or_else(|| AxError::System(format!("no AX element for pid {pid}")))?;
    let _ = crate::ffi::ax_set_messaging_timeout(&app, 2.0);
    let windows = app
        .windows()
        .map_err(|code| AxError::System(format!("windows: {}", crate::ffi::ax_error_name(code))))?;
    let window = windows
        .iter()
        .find(|w| ax_window_id(w) == Some(wid))
        .ok_or_else(|| {
            AxError::Ownership(format!(
                "wid {wid} not joined to owned pid {pid} (identity join failed)"
            ))
        })?;
    let _ = crate::ffi::ax_set_messaging_timeout(window, 2.0);
    let preimage = ax_get_frame(window).map_err(|code| {
        AxError::System(format!("preimage: {}", crate::ffi::ax_error_name(code)))
    })?;
    holder_revalidate(pid, wid, exe).map_err(AxError::Ownership)?;
    let mut ops = HolderFrameOps {
        window,
        pid,
        wid,
        helper_exe: exe.to_owned(),
    };
    crate::ax::run_frame_transaction(&mut ops, pid, wid, preimage, frame)
}

/// Worker body for a holder raise attempt. Fail-closed focus policy, same as
/// the cross-command path: the live focus preimage must already be the owned
/// session (established by owned self-activation) before anything changes.
/// Success requires the post-raise focus readback to equal the target; a
/// miss re-raises only the owned preimage window, never a foreign one.
#[cfg(target_os = "macos")]
fn raise_attempt_on_worker(
    pid: u32,
    wid: u32,
    exe: &str,
    nonce: &str,
) -> Result<RaiseReport, AxError> {
    use crate::ffi::{AxElement, ax_raise, ax_window_id};
    let owned = OwnedIdentity::new(pid, crate::BUNDLE_ID, nonce).map_err(AxError::Ownership)?;
    let live_bundle = crate::ax::live_owned_bundle(pid)?;
    crate::ax::gate_owned_frame(&owned, pid, &live_bundle)?;
    holder_revalidate(pid, wid, exe).map_err(AxError::Ownership)?;
    let app = AxElement::application(pid as i32)
        .ok_or_else(|| AxError::System(format!("no AX element for pid {pid}")))?;
    let _ = crate::ffi::ax_set_messaging_timeout(&app, 2.0);
    let windows = app
        .windows()
        .map_err(|code| AxError::System(format!("windows: {}", crate::ffi::ax_error_name(code))))?;
    let window = windows
        .iter()
        .find(|w| ax_window_id(w) == Some(wid))
        .ok_or_else(|| {
            AxError::Ownership(format!(
                "wid {wid} not joined to owned pid {pid} (identity join failed)"
            ))
        })?;
    let (pre_pid, pre_wid) = crate::ffi::ax_focused_preimage().ok_or_else(|| {
        AxError::System(
            "focus preimage unreadable (AX consent or no focused window); refusing to change focus"
                .to_owned(),
        )
    })?;
    if pre_pid != pid as i32 {
        return Err(AxError::Ownership(format!(
            "focus preimage is foreign (pid {pre_pid} != owned {pid}); refusing to steal focus"
        )));
    }
    let pre_wid = pre_wid.ok_or_else(|| {
        AxError::System(
            "focus preimage window unknown; refusing to change focus without a restorable preimage"
                .to_owned(),
        )
    })?;
    holder_revalidate(pid, wid, exe).map_err(AxError::Ownership)?;
    ax_raise(window)
        .map_err(|code| AxError::System(format!("raise: {}", crate::ffi::ax_error_name(code))))?;
    let post = crate::ffi::ax_focused_preimage();
    let (focused_pid, focused_wid) = match post {
        Some((p, w)) => (u32::try_from(p).ok(), w),
        None => (None, None),
    };
    let focus_verified = focused_pid == Some(pid) && focused_wid == Some(wid);
    if focus_verified {
        return Ok(RaiseReport {
            pid,
            wid,
            preimage_pid: pid,
            preimage_wid: pre_wid,
            focused_pid,
            focused_wid,
            focus_verified: true,
            focus_restored: true,
        });
    }
    let mut focus_restored = false;
    if holder_revalidate(pid, wid, exe).is_ok()
        && let Some(preimage_window) = windows.iter().find(|w| ax_window_id(w) == Some(pre_wid))
        && ax_raise(preimage_window).is_ok()
        && let Some((restored_pid, restored_wid)) = crate::ffi::ax_focused_preimage()
    {
        focus_restored = restored_pid == pid as i32 && restored_wid == Some(pre_wid);
    }
    Err(AxError::System(format!(
        "raise accepted but focus readback mismatch (focused pid={focused_pid:?} wid={focused_wid:?}, want pid={pid} wid={wid}); preimage restore attempted: {focus_restored}"
    )))
}

/// Run one holder worker with the documented worst-case bound. The worker is
/// never cancelled: a timeout return means "outcome unknown, a restore may
/// still be in flight" — never clean, never complete.
fn run_frame_worker(
    pid: u32,
    wid: u32,
    exe: &str,
    nonce: &str,
    frame: crate::ffi::AxFrame,
) -> (Result<FrameChangeReport, AxError>, u64) {
    let start = Instant::now();
    #[cfg(target_os = "macos")]
    {
        let exe = exe.to_owned();
        let nonce = nonce.to_owned();
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        if std::thread::Builder::new()
            .name(format!("ax-session-frame-{pid}"))
            .spawn(move || {
                let _ = done_tx.send(frame_attempt_on_worker(pid, wid, &exe, &nonce, frame));
            })
            .is_err()
        {
            return (
                Err(AxError::System("spawn AX worker thread".to_owned())),
                latency_ms(start),
            );
        }
        match done_rx.recv_timeout(Duration::from_secs(crate::ax::SET_FRAME_OUTER_SECS)) {
            Ok(report) => (report, latency_ms(start)),
            Err(_) => (
                Err(AxError::Timeout {
                    pid,
                    timeout_ms: crate::ax::SET_FRAME_OUTER_SECS * 1000,
                }),
                latency_ms(start),
            ),
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (pid, wid, exe, nonce, frame);
        (
            Err(AxError::Unsupported(
                "AX frame set is macOS-only".to_owned(),
            )),
            latency_ms(start),
        )
    }
}

/// Run one holder raise worker with a bounded wait.
fn run_raise_worker(
    pid: u32,
    wid: u32,
    exe: &str,
    nonce: &str,
) -> (Result<RaiseReport, AxError>, u64) {
    let start = Instant::now();
    #[cfg(target_os = "macos")]
    {
        let exe = exe.to_owned();
        let nonce = nonce.to_owned();
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        if std::thread::Builder::new()
            .name(format!("ax-session-raise-{pid}"))
            .spawn(move || {
                let _ = done_tx.send(raise_attempt_on_worker(pid, wid, &exe, &nonce));
            })
            .is_err()
        {
            return (
                Err(AxError::System("spawn AX worker thread".to_owned())),
                latency_ms(start),
            );
        }
        match done_rx.recv_timeout(Duration::from_secs(15)) {
            Ok(report) => (report, latency_ms(start)),
            Err(_) => (
                Err(AxError::Timeout {
                    pid,
                    timeout_ms: 15_000,
                }),
                latency_ms(start),
            ),
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (pid, wid, exe, nonce);
        (
            Err(AxError::Unsupported("AX raise is macOS-only".to_owned())),
            latency_ms(start),
        )
    }
}

/// Run one session observer (app-level set, plus the window-level set when
/// `wid` joins). Returns the outcome and the in-process wall time.
fn run_session_observer(
    pid: u32,
    wid: Option<u32>,
    window_secs: u64,
    tx: &std::sync::mpsc::SyncSender<crate::engine::EngineMessage>,
) -> (Result<crate::ffi::AxSessionOutcome, String>, u64) {
    let start = Instant::now();
    if pid <= 1 {
        return (
            Err(format!("refusing reserved pid {pid}")),
            latency_ms(start),
        );
    }
    #[cfg(target_os = "macos")]
    {
        let txc = tx.clone();
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        if std::thread::Builder::new()
            .name(format!("ax-session-observe-{pid}"))
            .spawn(move || {
                let _ = done_tx.send(crate::ffi::ax_observe_session(
                    pid as i32,
                    wid,
                    Duration::from_secs(window_secs),
                    2.0,
                    &txc,
                ));
            })
            .is_err()
        {
            return (Err("spawn observer thread".to_owned()), latency_ms(start));
        }
        // The observer thread always terminates (finite runloop) with a grace
        // slice of slack before a timeout is reported.
        match done_rx.recv_timeout(Duration::from_secs(window_secs.saturating_add(3))) {
            Ok(outcome) => (outcome.map_err(|e| e.to_string()), latency_ms(start)),
            Err(_) => (
                Err(format!(
                    "observer thread exceeded its window ({window_secs}s)"
                )),
                latency_ms(start),
            ),
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (wid, window_secs, tx);
        (
            Err("AX observation is macOS-only".to_owned()),
            latency_ms(start),
        )
    }
}

/// Start a session observer, retrying fast registration refusals while the
/// freshly spawned app registers with AX. A successful start pumps the full
/// window; only fast failures retry, bounded by [`ATTACH_RETRY_TIMEOUT`].
/// Returns the outcome plus the total in-process wall time.
fn observe_with_attach_retry(
    pid: u32,
    wid: Option<u32>,
    window_secs: u64,
    tx: &std::sync::mpsc::SyncSender<crate::engine::EngineMessage>,
) -> (Result<crate::ffi::AxSessionOutcome, String>, u64) {
    let start = Instant::now();
    loop {
        let (outcome, _) = run_session_observer(pid, wid, window_secs, tx);
        match outcome {
            Ok(_) => return (outcome, latency_ms(start)),
            Err(e) => {
                let retryable = e.contains("no AX notifications registered");
                if retryable && Instant::now() - start < ATTACH_RETRY_TIMEOUT {
                    std::thread::sleep(Duration::from_millis(200));
                    continue;
                }
                return (Err(e), latency_ms(start));
            }
        }
    }
}

/// Join a detached observer thread, ignoring its outcome. The thread always
/// terminates (finite observation window), so this is bounded. Takes the
/// handle so later error paths cannot double-join.
fn join_observer(
    handle: &mut Option<
        std::thread::JoinHandle<(Result<crate::ffi::AxSessionOutcome, String>, u64)>,
    >,
) {
    if let Some(joined) = handle.take() {
        let _ = joined.join();
    }
}
fn merge_observer(
    a: crate::ffi::AxSessionOutcome,
    b: crate::ffi::AxSessionOutcome,
    window_secs: u64,
) -> ObserverLeg {
    use std::collections::BTreeMap;
    let mut added: Vec<String> = a.notifications_added;
    for name in b.notifications_added {
        if !added.contains(&name) {
            added.push(name);
        }
    }
    added.sort_unstable();
    let mut counts: BTreeMap<String, u32> = BTreeMap::new();
    for (name, count) in a
        .notifications_received
        .into_iter()
        .chain(b.notifications_received)
    {
        *counts.entry(name).or_insert(0) += count;
    }
    ObserverLeg {
        added,
        received: counts.into_iter().collect(),
        window_secs,
    }
}

/// Count deliveries for one notification name in a merged leg.
fn deliveries_for(leg: &ObserverLeg, name: &str) -> u32 {
    leg.received
        .iter()
        .find(|(n, _)| n == name)
        .map(|(_, c)| *c)
        .unwrap_or(0)
}

/// Responsive phase: pre-window observer, driven move+resize, owned
/// self-activation, raise with readback, frontmost restore, exact-Child
/// stop. Every leg runs against the held Child with fresh CG joins; no title
/// proof anywhere. A failed leg still restores frontmost and stops the exact
/// child before the phase reports the failure as JSON evidence.
fn responsive_phase(
    engine_tx: &std::sync::mpsc::SyncSender<crate::engine::EngineMessage>,
    original: &FrontmostIdentity,
    seconds: u64,
) -> Result<ResponsivePhase, String> {
    let err_json = |stage: &str, error: String, extra: serde_json::Value| -> String {
        serde_json::json!({
            "stage": stage,
            "error": error,
            "evidence": extra,
        })
        .to_string()
    };
    let nonce = mint_nonce().map_err(|e| err_json("spawn", e, serde_json::Value::Null))?;
    // Deferred window creation (2s): the app registers with AX during the
    // defer while the pre-window observer below is already attached, so the
    // creation deterministically lands inside the observation window.
    // Self-close (close timer): the owned teardown stimulus for the
    // destroyed leg, firing after the driven legs complete (see
    // CLOSE_WINDOW_AFTER_READY_MS).
    let spec = ProbeTargetSpec::new(&nonce, 0, 0, seconds)
        .map(|s| {
            s.with_defer_window_ms(2000)
                .with_close_window_ms(CLOSE_WINDOW_AFTER_READY_MS)
        })
        .map_err(|e| err_json("spawn", e, serde_json::Value::Null))?;
    let mut raw =
        start_helper_process(&spec).map_err(|e| err_json("spawn", e, serde_json::Value::Null))?;
    let pid = raw.pid;
    let helper_path = raw.helper_path.clone();
    // Observer A starts BEFORE the handshake: the window does not exist yet,
    // so its creation lands inside the observation window. Detached thread;
    // joined after the stop below (finite window bounds the join).
    let tx_a = engine_tx.clone();
    let mut obs_a = match std::thread::Builder::new()
        .name(format!("ax-session-early-{pid}"))
        .spawn(move || observe_with_attach_retry(pid, None, OBSERVE_WINDOW_SECS, &tx_a))
    {
        Ok(handle) => Some(handle),
        Err(e) => {
            // No observer is running and no record exists yet: kill the
            // exact held handle (bounded) so no orphaned helper remains.
            let _ = raw.child.kill();
            let _ = raw.child.wait();
            return Err(err_json(
                "observer-spawn",
                format!("early observer thread: {e} (exact child killed, no record existed)"),
                serde_json::Value::Null,
            ));
        }
    };
    if let Err(e) = await_handshake(&mut raw, &spec) {
        let stop = stop_graceful_exact(&mut raw.child, pid);
        join_observer(&mut obs_a);
        return Err(err_json(
            "handshake",
            format!("{e} (child reaped={})", stop.reaped),
            serde_json::Value::Null,
        ));
    }
    if let Err(e) = record_spawn(pid, &spec, &helper_path) {
        let stop = stop_graceful_exact(&mut raw.child, pid);
        join_observer(&mut obs_a);
        return Err(err_json(
            "record",
            format!("{e} (child reaped={})", stop.reaped),
            serde_json::Value::Null,
        ));
    }
    // Fresh CG join: the helper owns exactly one window; anything else fails
    // closed (never a guessed WID).
    let t_cg = Instant::now();
    let owned_windows: Vec<crate::ffi::CgWindow> = crate::ffi::cg_window_list_all()
        .map_err(|e| {
            let stop = stop_graceful_exact(&mut raw.child, pid);
            join_observer(&mut obs_a);
            err_json(
                "cg-join",
                format!("CG snapshot: {e} (child reaped={})", stop.reaped),
                serde_json::Value::Null,
            )
        })?
        .into_iter()
        .filter(|w| w.pid == pid)
        .collect();
    if owned_windows.len() != 1 {
        let stop = stop_graceful_exact(&mut raw.child, pid);
        join_observer(&mut obs_a);
        return Err(err_json(
            "cg-join",
            format!(
                "expected exactly 1 CG window for owned pid {pid}, found {} (child reaped={})",
                owned_windows.len(),
                stop.reaped
            ),
            serde_json::Value::Null,
        ));
    }
    let wid = owned_windows[0].wid;
    if let Err(e) = holder_revalidate(pid, wid, &helper_path) {
        let stop = stop_graceful_exact(&mut raw.child, pid);
        join_observer(&mut obs_a);
        return Err(err_json(
            "cg-join",
            format!("{e} (child reaped={})", stop.reaped),
            serde_json::Value::Null,
        ));
    }
    let cg_ms = latency_ms(t_cg);
    // Observer B joins the live window: app-level plus window-level sets.
    let tx_b = engine_tx.clone();
    let obs_b = std::thread::Builder::new()
        .name(format!("ax-session-joined-{pid}"))
        .spawn(move || observe_with_attach_retry(pid, Some(wid), OBSERVE_WINDOW_SECS, &tx_b))
        .map_err(|e| {
            let stop = stop_graceful_exact(&mut raw.child, pid);
            join_observer(&mut obs_a);
            err_json(
                "observer-spawn",
                format!("joined observer thread: {e} (child reaped={})", stop.reaped),
                serde_json::Value::Null,
            )
        })?;
    // Drive move+resize: one frame set changing both position and size, so
    // both the app-level (AXWindowMoved/AXWindowResized) and window-level
    // (AXMoved/AXResized) notifications fire on the observed threads.
    let attempted = crate::ffi::AxFrame {
        x: 240.0,
        y: 240.0,
        w: 480.0,
        h: 320.0,
    };
    let (frame_res, frame_ms) = run_frame_worker(pid, wid, &helper_path, &nonce, attempted);
    let frame_leg = FrameLeg {
        report: frame_res.as_ref().ok().cloned(),
        error: frame_res.as_ref().err().map(|e| e.to_string()),
        latency_ms: frame_ms,
    };
    // Owned self-activation: normal activation request, verified by AX
    // focus readback — the request status alone proves nothing.
    let t_act = Instant::now();
    let sent = crate::ffi::activate_app(pid as i32);
    let focused = if sent {
        await_focused_pid(pid as i32, FOCUS_POLL_TIMEOUT)
    } else {
        None
    };
    let act_ms = latency_ms(t_act);
    // Raise with the owned preimage now live.
    let (raise_res, raise_ms) = run_raise_worker(pid, wid, &helper_path, &nonce);
    let raise_leg = RaiseLeg {
        report: raise_res.as_ref().ok().cloned(),
        error: raise_res.as_ref().err().map(|e| e.to_string()),
        latency_ms: raise_ms,
    };
    // Frontmost restore ALWAYS runs after the driven legs — even when a leg
    // above failed. Application activation only; never a window action.
    let t_restore = Instant::now();
    let restore_sent = crate::ffi::activate_app(original.pid);
    let frontmost_restored =
        restore_sent && await_frontmost_pid(original.pid, FRONTMOST_POLL_TIMEOUT);
    let restore_ms = latency_ms(t_restore);
    // Owned self-close readback: the helper closes its own window on its
    // timer (after the driven legs) while the joined observer still pumps.
    // One loop drains the helper's own execution marker AND polls the
    // independent CG absence readback (bounded); afterwards the process must
    // still be alive via the exact held handle plus the live exe binding. No
    // AX touch from here on except the already-running observers.
    let t_close = Instant::now();
    let mut marker_seen = false;
    let mut marker_windows: Option<usize> = None;
    let mut marker_visible: Option<bool> = None;
    let mut cg_absence = false;
    let mut lingering: Vec<LingeringWindow> = Vec::new();
    let close_deadline = Instant::now() + Duration::from_secs(CLOSE_WAIT_SECS);
    loop {
        while let Ok(line) = raw.lines.try_recv() {
            if marker_seen {
                continue;
            }
            if let Some((windows, visible)) = parse_closed_marker(&line) {
                marker_seen = true;
                marker_windows = Some(windows);
                marker_visible = visible;
            }
        }
        // Absence means no ON-SCREEN entry for the PID; any remaining
        // entries are recorded explicitly as lingering (never hidden).
        if let Ok(list) = crate::ffi::cg_window_list_all() {
            lingering = pid_entries(&list, pid);
            if onscreen_entries(&list, pid).is_empty() {
                cg_absence = true;
            }
        }
        if (marker_seen && cg_absence) || Instant::now() >= close_deadline {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let close_ms = latency_ms(t_close);
    let process_alive = matches!(raw.child.try_wait(), Ok(None))
        && crate::ownership::verify_live_process(pid, Some(&helper_path)).is_ok();
    let close_leg = CloseLeg {
        marker_seen,
        marker_windows,
        marker_visible,
        cg_absence,
        lingering,
        absence_latency_ms: close_ms,
        process_alive,
    };
    // Exact-Child stop (owned teardown lands inside both windows when the
    // schedule holds; delivery itself is OS behavior and counted as-is).
    let stop = stop_graceful_exact(&mut raw.child, pid);
    // Join both observers (finite windows bound both joins; a panicked
    // thread fails the leg with its evidence).
    let early = obs_a.take().map(|handle| handle.join());
    let joined = obs_b.join();
    let ((obs_a_res, _), (obs_b_res, _)) = match (early, joined) {
        (Some(Ok(a)), Ok(b)) => (a, b),
        (a, b) => {
            return Err(err_json(
                "observer-join",
                format!(
                    "observer thread missing or panicked (early={} joined={})",
                    a.is_none_or(|r| r.is_err()),
                    b.is_err()
                ),
                serde_json::json!({"stop": stop}),
            ));
        }
    };
    let obs_a_res = obs_a_res.map_err(|e| {
        err_json(
            "observer-early",
            format!(
                "{e} stop={}",
                serde_json::to_string(&stop).unwrap_or_default()
            ),
            serde_json::json!({"stop": stop}),
        )
    })?;
    let obs_b_res = obs_b_res.map_err(|e| {
        err_json(
            "observer-joined",
            format!(
                "{e} stop={}",
                serde_json::to_string(&stop).unwrap_or_default()
            ),
            serde_json::json!({"stop": stop}),
        )
    })?;
    let observer = merge_observer(obs_a_res, obs_b_res, OBSERVE_WINDOW_SECS);
    let phase = ResponsivePhase {
        pid,
        wid,
        cg_join_latency_ms: cg_ms,
        observer,
        frame: frame_leg,
        activation_sent: sent,
        activation_latency_ms: act_ms,
        raise: raise_leg,
        frontmost_restored,
        frontmost_restore_latency_ms: restore_ms,
        close: close_leg,
        stop,
    };
    // Verdicts, each naming its evidence. Frontmost restoration is checked
    // first: a failed restore stops everything with the residue.
    if !frontmost_restored {
        return Err(err_json(
            "frontmost-restore",
            format!(
                "FRONTMOST RESTORE FAILED (frontmost not returned to pid {}); residue evidence={} — STOP",
                original.pid,
                serde_json::to_string(&phase).unwrap_or_default()
            ),
            serde_json::to_value(&phase).unwrap_or(serde_json::Value::Null),
        ));
    }
    if focused.is_none() {
        return Err(err_json(
            "focus",
            format!(
                "helper activation did not land focus (sent={sent}); evidence={}",
                serde_json::to_string(&phase).unwrap_or_default()
            ),
            serde_json::to_value(&phase).unwrap_or(serde_json::Value::Null),
        ));
    }
    match &phase.frame.report {
        Some(report) if report.restored => {}
        _ => {
            return Err(err_json(
                "frame",
                format!(
                    "frame leg did not restore the exact preimage; evidence={}",
                    serde_json::to_string(&phase).unwrap_or_default()
                ),
                serde_json::to_value(&phase).unwrap_or(serde_json::Value::Null),
            ));
        }
    }
    match &phase.raise.report {
        Some(report) if report.focus_verified => {}
        _ => {
            return Err(err_json(
                "raise",
                format!(
                    "raise leg lacks a verified focus readback; evidence={}",
                    serde_json::to_string(&phase).unwrap_or_default()
                ),
                serde_json::to_value(&phase).unwrap_or(serde_json::Value::Null),
            ));
        }
    }
    // Notification thresholds: created, moved, resized, and focused must
    // each have delivered at least once across the merged observers.
    // Destroyed is verified by the close leg below (self-close stimulus with
    // CG-absence plus process-alive readbacks, then the delivery count).
    for name in [
        "AXWindowCreated",
        "AXWindowMoved",
        "AXWindowResized",
        "AXFocusedWindowChanged",
    ] {
        if deliveries_for(&phase.observer, name) == 0 {
            return Err(err_json(
                "observer",
                format!(
                    "no {name} delivery across the merged observers; evidence={}",
                    serde_json::to_string(&phase).unwrap_or_default()
                ),
                serde_json::to_value(&phase).unwrap_or(serde_json::Value::Null),
            ));
        }
    }
    // Destroyed verdict: the owned window self-closed on its timer while the
    // joined observer pumped. Requires the helper's own execution marker,
    // the independent CG absence, a still-alive process, and at least one
    // AXUIElementDestroyed delivery. Anything less is an honest failed route
    // with the full evidence — never a wider retry.
    if !phase.close.marker_seen || !phase.close.cg_absence || !phase.close.process_alive {
        return Err(err_json(
            "close",
            format!(
                "self-close readback incomplete (marker_seen={} marker_windows={:?} marker_visible={:?} cg_absence={} lingering={:?} process_alive={}); evidence={}",
                phase.close.marker_seen,
                phase.close.marker_windows,
                phase.close.marker_visible,
                phase.close.cg_absence,
                phase.close.lingering,
                phase.close.process_alive,
                serde_json::to_string(&phase).unwrap_or_default()
            ),
            serde_json::to_value(&phase).unwrap_or(serde_json::Value::Null),
        ));
    }
    if deliveries_for(&phase.observer, "AXUIElementDestroyed") == 0 {
        return Err(err_json(
            "destroyed",
            format!(
                "no AXUIElementDestroyed delivery despite verified self-close (CG absence read back, process alive); evidence={}",
                serde_json::to_string(&phase).unwrap_or_default()
            ),
            serde_json::to_value(&phase).unwrap_or(serde_json::Value::Null),
        ));
    }
    Ok(phase)
}

/// Fixed-size phase: controlled partial-write restoration. The non-resizable
/// owned window accepts the position leg but refuses the size leg, so the
/// transaction must restore the exact preimage. Success requires the
/// `set`-stage failure WITH a verified restore; any other outcome (both legs
/// accepted, restore failed, wrong stage) is reported with the full evidence
/// — including what the floor actually did.
fn fixed_phase(seconds: u64) -> Result<FixedPhase, String> {
    let err_json = |stage: &str, error: String, extra: serde_json::Value| -> String {
        serde_json::json!({
            "stage": stage,
            "error": error,
            "evidence": extra,
        })
        .to_string()
    };
    let nonce = mint_nonce().map_err(|e| err_json("spawn", e, serde_json::Value::Null))?;
    let spec = ProbeTargetSpec::new(&nonce, 0, 0, seconds)
        .map(|s| s.with_fixed_size(true))
        .map_err(|e| err_json("spawn", e, serde_json::Value::Null))?;
    let mut raw =
        start_helper_process(&spec).map_err(|e| err_json("spawn", e, serde_json::Value::Null))?;
    let pid = raw.pid;
    let helper_path = raw.helper_path.clone();
    if let Err(e) = await_handshake(&mut raw, &spec) {
        let stop = stop_graceful_exact(&mut raw.child, pid);
        return Err(err_json(
            "handshake",
            format!("{e} (child reaped={})", stop.reaped),
            serde_json::Value::Null,
        ));
    }
    if let Err(e) = record_spawn(pid, &spec, &helper_path) {
        let stop = stop_graceful_exact(&mut raw.child, pid);
        return Err(err_json(
            "record",
            format!("{e} (child reaped={})", stop.reaped),
            serde_json::Value::Null,
        ));
    }
    let owned_windows: Vec<crate::ffi::CgWindow> = crate::ffi::cg_window_list_all()
        .map_err(|e| {
            let stop = stop_graceful_exact(&mut raw.child, pid);
            err_json(
                "cg-join",
                format!("CG snapshot: {e} (child reaped={})", stop.reaped),
                serde_json::Value::Null,
            )
        })?
        .into_iter()
        .filter(|w| w.pid == pid)
        .collect();
    if owned_windows.len() != 1 {
        let stop = stop_graceful_exact(&mut raw.child, pid);
        return Err(err_json(
            "cg-join",
            format!(
                "expected exactly 1 CG window for owned pid {pid}, found {} (child reaped={})",
                owned_windows.len(),
                stop.reaped
            ),
            serde_json::Value::Null,
        ));
    }
    let wid = owned_windows[0].wid;
    if let Err(e) = holder_revalidate(pid, wid, &helper_path) {
        let stop = stop_graceful_exact(&mut raw.child, pid);
        return Err(err_json(
            "cg-join",
            format!("{e} (child reaped={})", stop.reaped),
            serde_json::Value::Null,
        ));
    }
    // The fault leg: position must apply, size must refuse, the exact
    // preimage must be restored and verified.
    let attempted = crate::ffi::AxFrame {
        x: 280.0,
        y: 280.0,
        w: 520.0,
        h: 360.0,
    };
    let (attempt, attempt_ms) = run_frame_worker(pid, wid, &helper_path, &nonce, attempted);
    let partial = match attempt {
        Ok(report) => PartialLeg {
            failure: None,
            error: Some(format!(
                "fault control ineffective on this floor: full set accepted and restored (applied={:?}); no partial occurred",
                report.applied
            )),
            latency_ms: attempt_ms,
        },
        Err(AxError::FrameTx(failure)) => PartialLeg {
            failure: Some(*failure),
            error: None,
            latency_ms: attempt_ms,
        },
        Err(other) => PartialLeg {
            failure: None,
            error: Some(other.to_string()),
            latency_ms: attempt_ms,
        },
    };
    let stop = stop_graceful_exact(&mut raw.child, pid);
    let phase = FixedPhase {
        pid,
        wid,
        partial,
        stop,
    };
    // Verdict: `set`-stage failure with a verified exact-preimage restore.
    match &phase.partial.failure {
        Some(failure) if failure.stage == "set" && failure.restored => {}
        _ => {
            return Err(err_json(
                "partial",
                format!(
                    "no controlled partial-write restoration; evidence={}",
                    serde_json::to_string(&phase).unwrap_or_default()
                ),
                serde_json::to_value(&phase).unwrap_or(serde_json::Value::Null),
            ));
        }
    }
    Ok(phase)
}

/// Single bounded owned-AX session. Owns every exact `Child` end to end;
/// reads, drives, and cleans up only the spawned helpers. No hotkeys, taps,
/// or Space writes anywhere on this path.
pub fn run_ax_session(seconds: u64) -> Result<String, (i32, String)> {
    crate::config::ProbeConfig::validate_probe_seconds(seconds).map_err(|e| (2, e))?;
    // AX-consent gate first: every leg below needs observed trust. Anything
    // else is a consent shortfall with the grant path — never a bypass.
    let trust = crate::ffi::ax_is_process_trusted();
    if trust != Some(true) {
        return Err((
            2,
            format!(
                "AX consent shortfall (trust observed {trust:?}); run through the signed bundle with the grant in place, then retry"
            ),
        ));
    }
    // Engine covers the whole session plus slack; a short idle must never
    // drop events mid-observation.
    let idle = Duration::from_secs(seconds.saturating_add(180));
    let (engine, handle) = crate::engine::Engine::spawn_thread(idle);
    let outcome: Result<AxSessionReport, String> = (|| {
        // Focus restoration FIRST on a bounded throwaway spawn: it gates
        // every focus-changing leg that follows.
        let focus_proof = prove_focus_restore(seconds.min(30))?;
        let original = focus_proof.original.clone();
        let responsive = responsive_phase(&engine.tx, &original, seconds)?;
        let fixed = fixed_phase(seconds)?;
        Ok(AxSessionReport {
            bundle: crate::BUNDLE_ID.to_owned(),
            trust_observed: trust,
            focus_proof,
            responsive,
            fixed,
            engine_ax_events: 0,
        })
    })();
    let _ = engine.try_send(crate::engine::EngineMessage::Shutdown);
    let state = handle
        .join()
        .map_err(|_| (1, "engine thread failed".to_owned()))?;
    match outcome {
        Ok(mut report) => {
            report.engine_ax_events = state.ax_events;
            serde_json::to_string(&report).map_err(|e| (1, e.to_string()))
        }
        Err(e) => Err((1, e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_seconds_rejected_before_spawn() {
        assert!(run_ax_session(0).is_err());
        assert!(run_ax_session(crate::config::MAX_PROBE_SECONDS + 1).is_err());
    }

    #[test]
    fn holder_revalidate_refuses_reserved_pid_before_any_touch() {
        assert!(holder_revalidate(1, 7, "/nonexistent").is_err());
        assert!(holder_revalidate(0, 0, "/nonexistent").is_err());
    }

    #[test]
    fn frontmost_wait_is_bounded_for_an_impossible_pid() {
        // Reserved PIDs never become frontmost: the poll must return false
        // fast instead of waiting out the full timeout.
        let start = Instant::now();
        assert!(!await_frontmost_pid(1, Duration::from_millis(150)));
        assert!(start.elapsed() < Duration::from_secs(10));
    }

    #[test]
    fn closed_marker_parses_only_the_exact_shape() {
        assert_eq!(
            parse_closed_marker(r#"{"closed":true,"windows":0,"visible":false}"#),
            Some((0, Some(false)))
        );
        assert_eq!(
            parse_closed_marker(r#"{"closed":true,"windows":3}"#),
            Some((3, None))
        );
        // Handshake lines, wrong shapes, and garbage never parse.
        assert_eq!(
            parse_closed_marker(
                r#"{"bundle":"io.github.omnitiler.macos.probe","pid":1,"nonce":"ab"}"#
            ),
            None
        );
        assert_eq!(parse_closed_marker(r#"{"ready":true}"#), None);
        assert_eq!(parse_closed_marker(r#"{"closed":false,"windows":0}"#), None);
        assert_eq!(parse_closed_marker(r#"{"closed":true}"#), None);
        assert_eq!(parse_closed_marker("not json"), None);
        assert_eq!(parse_closed_marker(""), None);
    }

    fn cg_window(pid: u32, wid: u32, onscreen: bool) -> crate::ffi::CgWindow {
        crate::ffi::CgWindow {
            wid,
            pid,
            layer: 0,
            onscreen,
            bounds: crate::ffi::CgBounds {
                x: 0.0,
                y: 0.0,
                w: 1.0,
                h: 1.0,
            },
            title: None,
        }
    }

    #[test]
    fn onscreen_presence_ignores_offscreen_tombstones() {
        // A closed window's record may linger off-screen: presence means an
        // on-screen entry, while lingering records every PID entry
        // explicitly. Foreign PIDs never count.
        let live = vec![
            cg_window(4242, 7, true),
            cg_window(4242, 8, false),
            cg_window(9999, 9, true),
        ];
        let present = onscreen_entries(&live, 4242);
        assert_eq!(present.len(), 1);
        assert_eq!(present[0].wid, 7);
        assert!(onscreen_entries(&live, 1).is_empty());
        let lingering = pid_entries(&live, 4242);
        assert_eq!(lingering.len(), 2);
        assert!(pid_entries(&live, 4243).is_empty());
        // Fully closed: no entries at all.
        assert!(onscreen_entries(&[], 4242).is_empty());
        assert!(pid_entries(&[], 4242).is_empty());
    }

    #[test]
    fn merged_observer_unions_names_and_sums_counts() {
        let a = crate::ffi::AxSessionOutcome {
            notifications_added: vec!["AXWindowCreated".to_owned()],
            notifications_received: vec![
                ("AXWindowCreated".to_owned(), 1),
                ("AXWindowMoved".to_owned(), 2),
            ],
            messaging_timeout_set: true,
        };
        let b = crate::ffi::AxSessionOutcome {
            notifications_added: vec!["AXMoved".to_owned(), "AXWindowCreated".to_owned()],
            notifications_received: vec![("AXWindowCreated".to_owned(), 3)],
            messaging_timeout_set: true,
        };
        let merged = merge_observer(a, b, 12);
        assert_eq!(merged.added.len(), 2);
        assert_eq!(deliveries_for(&merged, "AXWindowCreated"), 4);
        assert_eq!(deliveries_for(&merged, "AXWindowMoved"), 2);
        assert_eq!(deliveries_for(&merged, "AXNope"), 0);
    }

    #[test]
    fn session_notification_sets_cover_the_acceptance_legs() {
        // The registered sets must include the app-level created/moved/
        // resized/focused/main notifications plus the window-level
        // moved/resized/destroyed/minimized/deminimized companions —
        // exactly the acceptance legs (minimize/close reflow needs the
        // window-level minimize/deminimize/destroyed watches).
        for name in [
            "AXWindowCreated",
            "AXFocusedWindowChanged",
            "AXMainWindowChanged",
            "AXWindowMoved",
            "AXWindowResized",
        ] {
            assert!(
                crate::ffi::AX_SESSION_APP_NOTIFICATIONS.contains(&name),
                "{name} must be app-observed"
            );
        }
        for name in [
            "AXMoved",
            "AXResized",
            "AXUIElementDestroyed",
            "AXWindowMiniaturized",
            "AXWindowDeminiaturized",
        ] {
            assert!(
                crate::ffi::AX_SESSION_WINDOW_NOTIFICATIONS.contains(&name),
                "{name} must be window-observed"
            );
        }
    }
}

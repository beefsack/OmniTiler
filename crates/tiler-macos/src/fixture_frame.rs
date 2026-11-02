//! Development-only standalone AX frame-write diagnostic for the signed live fixture.
//!
//! Exactly one ordinary bounded cross-process `AXPosition`/`AXSize` write to
//! one exact fixture window, with fresh signed-fixture proof before the write
//! and immediate before/after AX-frame facts. Standalone experiment only: no
//! leg admission, no controller lifecycle, no product protocol or state.
//!
//! Proof shape (refuses before any write, first failure wins): session gates
//! (SLPS symbols, AX trust, Secure Input observed inactive - unknown refuses,
//! unlocked session), the serve announcement file (the only nonce source),
//! live bundle/executable/launch reads plus `codesign` `--verify` and the
//! designated-requirement read against the explicit `--signer`, a nonce-gated
//! read-only `get` over the announced Unix socket (the announced socket path
//! is used and shape-checked; the exchange proves the nonce live), and the CG
//! owner join plus the sentinel-title caller/run-tag join cross-checked
//! against the live server snapshot's run tag, judged through the shared
//! [`crate::fixture_zoom::check_zoom_authorization`] choke point (same proof
//! as the zoom/menu diagnostics, never weakened).
//!
//! No menu traversal, no `AXPress`, no fullscreen/green-button vocabulary, no
//! input synthesis, no focus acquisition: the write targets the exact joined
//! AX element directly, so no focused-window/frontmost agreement is required
//! (menus need the key window; a direct frame write does not). The commit
//! boundary repeats the cheap proof (fresh identity plus fresh CG through the
//! same pure choke point, no socket, no subprocess) on the AX worker
//! immediately before the single write. One write per invocation, no retry on
//! uncertain outcome. A main-thread timeout return means write state unknown
//! (a late write remains possible): never retry blindly, inspect the
//! before/after facts first. Outputs are metadata-only JSON (IDs, stages,
//! frames, latencies - never titles, content, paths, nonces, executables, or
//! requirement text).
//!
//! Partial-write hazard: the frame write is two ordered legs (position, then
//! size). A size refusal after an accepted position write leaves the window
//! partially mutated; both leg statuses are reported and the invocation
//! returns unknown with no replay and no automatic restore. Restoration is an
//! explicit separate step (socket geometry or a second invocation), never an
//! implicit second write, so the arbitration evidence stays intact.

#![forbid(unsafe_code)]

use serde::Serialize;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

use crate::fixture_post::{
    SocketSnapshot, bundle_verdict, caller_id_valid, fixture_designated_tail,
    fixture_process_identity, parse_fixture_announcement, secure_input_verdict,
    socket_snapshot_for,
};

/// Bound for the single AX worker (join plus commit plus one write plus
/// after-facts). Each AX read carries its own 2 s messaging timeout; the
/// worker bound caps the whole pass.
const AX_WORKER_BOUND: Duration = Duration::from_secs(15);
/// Hard lifetime for the whole diagnostic (proof plus worker plus report).
const HARD_LIMIT: Duration = Duration::from_secs(60);

/// CLI entry parameters for one standalone frame write: the serve
/// announcement file (socket/pid/nonce source), the selected fixture caller
/// id, the exact target window id, the requested ordinary frame in screen
/// points, and the expected designated requirement (from `fixture-verify`
/// output at preflight; machine-specific, never hardcoded here).
pub struct FixtureFrameWriteArgs {
    pub announcement_file: String,
    pub caller: String,
    pub wid: u32,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub signer: String,
}

/// Requested frame in screen points. Plain data; validated by
/// [`check_frame_bounds`] before any proof runs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RequestedFrame {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// Pure ordinary-frame bounds over plain data (no OS touch): finite
/// coordinates, strictly positive size, and a generous absolute cap so an
/// absurd value refuses before any proof or write. This bounds the numbers,
/// never the OS effect: the operator must still choose an ordinary windowed
/// frame (not a full-display frame), and the before/after readback judges the
/// effect. Pure; unit-tested.
pub fn check_frame_bounds(frame: &RequestedFrame) -> Result<(), String> {
    for (name, value) in [
        ("x", frame.x),
        ("y", frame.y),
        ("w", frame.w),
        ("h", frame.h),
    ] {
        if !value.is_finite() {
            return Err(format!(
                "refusing fixture frame write: {name} is not finite; refusing"
            ));
        }
    }
    if frame.w <= 0.0 || frame.h <= 0.0 {
        return Err(
            "refusing fixture frame write: width and height must be positive; refusing".to_owned(),
        );
    }
    const ABS: f64 = 16384.0;
    if frame.x.abs() > ABS || frame.y.abs() > ABS || frame.w > ABS || frame.h > ABS {
        return Err(format!(
            "refusing fixture frame write: frame exceeds the ordinary {ABS}pt bound; refusing"
        ));
    }
    Ok(())
}

/// Pure write-readiness over the before-facts: the before AX frame must have
/// read successfully. An unreadable preimage refuses before any commit proof
/// or write: without it neither arbitration nor restoration can be judged.
/// Pure; unit-tested.
#[cfg(target_os = "macos")]
pub(crate) fn frame_preimage_readable(before: &FrameFact) -> Result<(), String> {
    if before.frame.is_some() {
        Ok(())
    } else {
        Err(format!(
            "refusing fixture frame write: before AX frame unreadable ({}); refusing without a preimage",
            before.status
        ))
    }
}

/// Pure commit-to-write decision in write order: frame bounds, then the
/// preimage readability, then the commit re-proof verdict, then the
/// latched-interrupt check, then the main-bound cancellation fence. Any Err
/// must prevent the single write; only Ok authorizes it. Pure; unit-tested.
#[cfg(target_os = "macos")]
pub(crate) fn authorize_frame_write(
    frame: &RequestedFrame,
    before: &FrameFact,
    commit: &Result<(), String>,
    interrupted: bool,
    cancelled: bool,
) -> Result<(), String> {
    check_frame_bounds(frame)?;
    frame_preimage_readable(before)?;
    if let Err(error) = commit {
        return Err(format!("frame worker commit: {error}"));
    }
    if interrupted {
        return Err("frame worker: SIGINT/SIGTERM latched; refusing write".to_owned());
    }
    if cancelled {
        return Err(
            "frame worker: main bound lapsed before the write; refusing (write state stays provable only via facts)"
                .to_owned(),
        );
    }
    Ok(())
}

/// One attempted pair of AX legs: per-leg statuses plus an optional
/// post-actuation error. `error: Some` means at least one AX write was
/// attempted, so the caller must still sample after-facts and report outcome
/// unknown with exit 1. `size_status` is [`SIZE_NOT_ATTEMPTED`] when the
/// size leg never ran. Pure data; unit-tested.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AttemptedWrite {
    pub position_status: String,
    pub size_status: String,
    pub error: Option<String>,
}

/// Size-leg status recorded when the size leg never ran (the position leg or
/// the between-legs gate stopped the sequence first).
pub(crate) const SIZE_NOT_ATTEMPTED: &str = "not-attempted";

/// Run the two ordered AX legs with a fresh session/cancellation gate
/// between the blocking calls. Each leg reports `Ok(())` or its exact
/// status string; the between-gate reports refusal text. The first failure
/// stops the sequence with the known statuses retained and the error
/// recorded; there is no retry and no restore. Pure orchestration over
/// injected legs; unit-tested with fakes, driven live with the real AX
/// setters plus the real between-legs gate.
pub(crate) fn run_write_legs(
    write_position: impl FnOnce() -> Result<(), String>,
    between_gates: impl FnOnce() -> Result<(), String>,
    write_size: impl FnOnce() -> Result<(), String>,
) -> AttemptedWrite {
    let position_status = match write_position() {
        Ok(()) => "ok".to_owned(),
        Err(status) => {
            return AttemptedWrite {
                position_status: status.clone(),
                size_status: SIZE_NOT_ATTEMPTED.to_owned(),
                error: Some(format!(
                    "frame worker: AXPosition write failed ({status}) with no retry; write state unknown"
                )),
            };
        }
    };
    if let Err(refusal) = between_gates() {
        return AttemptedWrite {
            position_status,
            size_status: SIZE_NOT_ATTEMPTED.to_owned(),
            error: Some(format!(
                "frame worker: between-legs gate refused after accepted AXPosition ({refusal}) with no retry and no automatic restore; window may be partially mutated, inspect the after-facts"
            )),
        };
    }
    match write_size() {
        Ok(()) => AttemptedWrite {
            position_status,
            size_status: "ok".to_owned(),
            error: None,
        },
        Err(status) => AttemptedWrite {
            position_status,
            size_status: status.clone(),
            error: Some(format!(
                "frame worker: AXSize write failed after accepted AXPosition ({status}) with no retry and no automatic restore; window may be partially mutated, inspect the after-facts"
            )),
        },
    }
}

/// Drive exactly one write effect: the pure decision first, then a single
/// effect call with no retry. Pre-effect refusals propagate as `Err` and the
/// effect never runs. A completed effect returns its [`AttemptedWrite`],
/// including post-actuation failures with the known leg statuses retained:
/// the caller must still sample after-facts and report outcome unknown.
/// Unit-tested with fake effects standing in for the single AX call site.
#[cfg(target_os = "macos")]
pub(crate) fn drive_single_frame_write(
    frame: &RequestedFrame,
    before: &FrameFact,
    commit: Result<(), String>,
    interrupted: bool,
    cancelled: bool,
    effect: impl FnOnce() -> Result<(AttemptedWrite, u64), String>,
) -> Result<(AttemptedWrite, u64), String> {
    authorize_frame_write(frame, before, &commit, interrupted, cancelled)?;
    effect()
}

/// Shared live session gates before proof and inside the commit re-proof (no
/// writes): SLPS symbols, AX trust, Secure Input (strict: unknown refuses),
/// unlocked session. No CG post-access gate: this route performs no CG post.
/// Same shape as the zoom/menu diagnostics; duplicated (not shared) so this
/// module judges through identical gates without touching those files.
#[cfg(target_os = "macos")]
fn frame_session_gates() -> Result<(), String> {
    if !crate::ffi::slps_available() {
        return Err("slps-unavailable: SLPS focus symbols unavailable; stop".to_owned());
    }
    if crate::ffi::ax_is_process_trusted() != Some(true) {
        return Err("ax-trust: AX trust not observed true; stop".to_owned());
    }
    secure_input_verdict(crate::ffi::secure_input_known())?;
    match crate::ffi::session_state().lock {
        crate::ffi::SessionLock::Unlocked => Ok(()),
        crate::ffi::SessionLock::Locked => {
            Err("session: session locked; refusing frame write".to_owned())
        }
        crate::ffi::SessionLock::Unknown => {
            Err("session: session unknown; refusing frame write".to_owned())
        }
    }
}

/// Join the exact `(pid, wid)` AX window on the calling thread: application
/// element, 2 s messaging timeout, scan with a per-window 2 s timeout set
/// BEFORE the id read (an unsettable window is skipped, never read
/// unbounded), then a 2 s timeout on the retained target (refuses when
/// unsettable: later reads on it must stay bounded).
#[cfg(target_os = "macos")]
fn join_ax_window(pid: u32, wid: u32) -> Result<crate::ffi::AxElement, String> {
    use crate::ffi::{AxElement, ax_error_name, ax_set_messaging_timeout, ax_window_id};
    let pid_i32 = i32::try_from(pid).map_err(|_| format!("frame join: pid {pid} out of range"))?;
    let app = AxElement::application(pid_i32)
        .ok_or_else(|| format!("frame join: no AX element for pid {pid}"))?;
    if ax_set_messaging_timeout(&app, 2.0) != 0 {
        return Err("frame join: messaging timeout unset before window join".to_owned());
    }
    let windows = app.windows().map_err(|code| {
        format!(
            "frame join: AX window list unreadable ({})",
            ax_error_name(code)
        )
    })?;
    let mut target: Option<AxElement> = None;
    for window in windows {
        if ax_set_messaging_timeout(&window, 2.0) != 0 {
            continue;
        }
        if ax_window_id(&window) == Some(wid) {
            target = Some(window);
            break;
        }
    }
    let Some(window) = target else {
        return Err(format!("frame join: window wid {wid} rejoin missed"));
    };
    if ax_set_messaging_timeout(&window, 2.0) != 0 {
        return Err("frame join: messaging timeout unset on retained target; refusing".to_owned());
    }
    Ok(window)
}

/// One fresh title-leg CG entry for `wid`: `(owner pid, title)`. `None`
/// entry (absent wid) is returned for the pure check to judge, never
/// guessed.
#[cfg(target_os = "macos")]
fn cg_entry_for(wid: u32) -> Result<Option<(u32, Option<String>)>, String> {
    crate::ffi::cg_window_list_all()
        .map(|list| {
            list.into_iter()
                .find(|window| window.wid == wid)
                .map(|window| (window.pid, window.title))
        })
        .map_err(|e| format!("CG snapshot refused ({e})"))
}

/// One AX frame fact: the observed frame in screen points plus the exact
/// native or local status. `frame` is `Some` only on a successful read;
/// otherwise `status` carries the exact AX error name or the local refusal.
#[derive(Debug, Clone, Serialize)]
pub struct FrameFact {
    pub frame: Option<FramePoint>,
    pub status: String,
}

/// Serializable frame in screen points (no conversion claims).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct FramePoint {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[cfg(target_os = "macos")]
impl From<crate::ffi::AxFrame> for FramePoint {
    fn from(frame: crate::ffi::AxFrame) -> Self {
        FramePoint {
            x: frame.x,
            y: frame.y,
            w: frame.w,
            h: frame.h,
        }
    }
}

/// Read one AX frame fact off the retained joined window. Every unknown
/// carries its exact native status; nothing is defaulted.
#[cfg(target_os = "macos")]
fn read_frame_fact(window: &crate::ffi::AxElement) -> FrameFact {
    use crate::ffi::{ax_error_name, ax_get_frame, ax_set_messaging_timeout};
    let _ = ax_set_messaging_timeout(window, 2.0);
    match ax_get_frame(window) {
        Ok(frame) => FrameFact {
            frame: Some(frame.into()),
            status: "ok".to_owned(),
        },
        Err(code) => FrameFact {
            frame: None,
            status: ax_error_name(code),
        },
    }
}

#[derive(Debug, Clone, Serialize)]
struct FixtureFrameReport {
    outcome: String,
    error: Option<String>,
    pid: u32,
    wid: u32,
    caller: String,
    secure_input: Option<bool>,
    ax_trusted: Option<bool>,
    proof_us: u64,
    write_latency_us: u64,
    /// Shared host monotonic stamps (`crate::ffi::host_now_ns`, `None`
    /// when the timebase is unreadable): the position-leg dispatch edge,
    /// the size-leg dispatch edge, and the return after both legs. These
    /// join with the concurrent sampler's stamps on one origin; the
    /// `write_latency_us` worker-relative cost above is unchanged.
    position_dispatch_ns: Option<u64>,
    size_dispatch_ns: Option<u64>,
    return_ns: Option<u64>,
    requested: FramePoint,
    position_status: String,
    size_status: String,
    frame_before: FrameFact,
    frame_after: FrameFact,
    zoom_before: crate::scope_inspect::ZoomFacts,
    zoom_after: crate::scope_inspect::ZoomFacts,
    cg_owner_before: Option<u32>,
    cg_owner_after: Option<u32>,
    run_tag_agreement: bool,
    synthetic: bool,
    note: String,
}

/// Standalone frame-write diagnostic: fresh proof, then exactly one native
/// cross-process AX frame write on an AX worker with a commit-boundary
/// re-proof, then before/after facts. A post-actuation leg failure still
/// emits the full after-facts report with outcome unknown and exit 1.
/// Exit codes: 2 usage/validation, 1 gate/proof/write failure with evidence.
#[cfg(target_os = "macos")]
pub fn run_fixture_frame_write(args: &FixtureFrameWriteArgs) -> Result<String, (i32, String)> {
    use crate::fixture_zoom::{ZoomAuthInputs, check_zoom_authorization};
    let err_json = |stage: &str, error: String| -> (i32, String) {
        (
            1,
            serde_json::json!({"stage": stage, "error": error}).to_string(),
        )
    };
    let refuse = |stage: &str, error: String| -> (i32, String) {
        eprintln!("fixture-frame-write refused at {stage}: {error}");
        err_json(stage, error)
    };
    if args.announcement_file.is_empty() {
        return Err((
            2,
            "fixture-frame-write needs a nonempty --announcement-file".to_owned(),
        ));
    }
    if args.signer.trim().is_empty() {
        return Err((
            2,
            "fixture-frame-write needs a nonempty --signer".to_owned(),
        ));
    }
    if !caller_id_valid(&args.caller) {
        return Err((
            2,
            "refusing fixture frame write: caller id malformed".to_owned(),
        ));
    }
    if args.wid == 0 {
        return Err((2, "refusing fixture frame write to window id 0".to_owned()));
    }
    let requested = RequestedFrame {
        x: args.x,
        y: args.y,
        w: args.w,
        h: args.h,
    };
    check_frame_bounds(&requested).map_err(|e| (2, e))?;
    let mut _watchdog = crate::watchdog::Watchdog::arm(HARD_LIMIT).map_err(|e| (1, e))?;
    frame_session_gates().map_err(|e| refuse("session", e))?;
    let _interrupt =
        crate::space_drag_diag_live::InterruptArmed::arm().map_err(|e| refuse("interrupt", e))?;
    let proof_start = Instant::now();
    let raw = std::fs::read_to_string(&args.announcement_file).map_err(|e| {
        (
            2,
            format!("fixture-frame-write announcement file unreadable ({e})"),
        )
    })?;
    let announced = parse_fixture_announcement(raw.trim())
        .map_err(|e| (2, format!("announcement refused: {e}")))?;
    let pid = announced.pid;
    if pid <= 1 {
        return Err((
            2,
            format!("refusing fixture frame write to reserved pid {pid}"),
        ));
    }
    if pid == std::process::id() {
        return Err((
            2,
            "refusing fixture frame write to the controller itself".to_owned(),
        ));
    }
    if i32::try_from(pid).is_err() {
        return Err((
            2,
            format!("refusing fixture frame write: pid {pid} out of range"),
        ));
    }
    let identity = fixture_process_identity(pid).map_err(|e| refuse("identity", e))?;
    bundle_verdict(identity.bundle.as_deref())
        .map_err(|e| refuse("identity", format!("identity: {e}")))?;
    let exe = identity.exe.filter(|exe| !exe.is_empty()).ok_or_else(|| {
        refuse(
            "identity",
            "identity: executable path unreadable; refusing".to_owned(),
        )
    })?;
    let launch = identity
        .launch
        .filter(|launch| !launch.is_empty())
        .ok_or_else(|| {
            refuse(
                "identity",
                "identity: launch date unreadable (no LaunchServices lifetime); refusing"
                    .to_owned(),
            )
        })?;
    let tail = fixture_designated_tail(&exe).map_err(|e| refuse("signature", e))?;
    if tail != args.signer.trim() {
        return Err(refuse(
            "signature",
            "signature: designated requirement mismatch (not the expected fixture signer); refusing"
                .to_owned(),
        ));
    }
    let snapshot =
        socket_snapshot_for(&announced.socket, &announced.nonce, &args.caller).map_err(|e| {
            refuse(
                "socket",
                format!("socket: {e} (nonce proves the live run; stale or foreign runs refuse)"),
            )
        })?;
    if snapshot.pid != pid {
        return Err(refuse(
            "socket",
            format!(
                "refusing fixture frame write: live server pid {} != announced pid {pid}; refusing",
                snapshot.pid
            ),
        ));
    }
    let cg_entry = cg_entry_for(args.wid).map_err(|e| refuse("proof", e))?;
    let cg_owner_before = cg_entry.as_ref().map(|(owner, _)| *owner);
    // When CG withholds the title (this reader lacks Screen Recording) but
    // the owner join already names the fixture pid, take one bounded AXTitle
    // read off the exact joined fixture window as the title source. A
    // present CG title stays authoritative and an absent CG entry still
    // refuses: the fallback fills a withheld title, never an absent window,
    // and never a foreign pid's window.
    let ax_title: Option<String> = match &cg_entry {
        Some((owner, None)) if *owner == pid => join_ax_window(pid, args.wid)
            .ok()
            .and_then(|window| {
                crate::ffi::ax_string_attribute(&window, "AXTitle")
                    .ok()
                    .flatten()
            })
            .filter(|title| !title.is_empty()),
        _ => None,
    };
    let bound = ZoomAuthInputs {
        caller: args.caller.clone(),
        wid: args.wid,
        announced_pid: pid,
        snapshot: snapshot.clone(),
        bundle: identity.bundle.clone(),
        sig_tail: Some(tail.clone()),
        expected_signer: args.signer.clone(),
        live_exe: Some(exe.clone()),
        bound_exe: exe.clone(),
        live_launch: Some(launch.clone()),
        bound_launch: launch.clone(),
        cg_entry,
        ax_title,
        secure_input: crate::ffi::secure_input_known(),
    };
    check_zoom_authorization(&bound).map_err(|e| {
        refuse(
            "proof",
            e.replace("refusing fixture zoom", "refusing fixture frame write"),
        )
    })?;
    if crate::space_drag_diag_live::diag_interrupted() {
        return Err(refuse(
            "interrupted",
            "interrupted: SIGINT/SIGTERM latched; refusing write".to_owned(),
        ));
    }
    let proof_us = proof_start.elapsed().as_micros().min(u128::from(u64::MAX)) as u64;
    // The single AX worker owns every `AxElement` (construct, use, drop);
    // the main thread only waits bounded. The worker repeats the cheap proof
    // through the same pure choke point at commit (fresh identity plus fresh
    // CG, no socket, no subprocess). Inside the effect, the retained handle
    // is re-proven live immediately before the single write, and the late
    // cancellation fence is rechecked there too.
    let worker_caller = args.caller.clone();
    let worker_wid = args.wid;
    let worker_signer = args.signer.clone();
    // Best-effort cancellation fence: set when the main bound lapses so a
    // still-running worker refuses instead of writing late. It narrows but
    // cannot close the post-timeout write window of a synchronous AX set
    // (flag set after the worker's final check still races), so a timeout
    // return always means write state UNKNOWN, never no-write, and never a
    // replay without inspecting the before/after facts.
    let cancelled = Arc::new(AtomicBool::new(false));
    let worker_cancelled = Arc::clone(&cancelled);
    let (done_tx, done_rx) = std::sync::mpsc::channel::<Result<WorkerOutcome, FrameWriteFailure>>();
    std::thread::Builder::new()
        .name(format!("fixture-frame-write-{pid}"))
        .spawn(move || {
            let outcome = frame_write_on_worker(
                pid,
                &worker_caller,
                worker_wid,
                requested,
                &worker_signer,
                &snapshot,
                &exe,
                &launch,
                &worker_cancelled,
            );
            let _ = done_tx.send(outcome);
        })
        .map_err(|e| refuse("ax-worker", format!("AX worker spawn refused ({e})")))?;
    // Bounded wait: a hung worker costs this bound only. A timeout or a dead
    // worker sets the fence and reports write state unknown (a late write
    // remains possible past the timeout); the invocation never replays. A
    // post-actuation leg failure still carries the full after-facts report
    // with outcome unknown and exit 1.
    let (label, error, outcome) = match done_rx.recv_timeout(AX_WORKER_BOUND) {
        Ok(Ok(outcome)) => ("written", None, outcome),
        Ok(Err(FrameWriteFailure::Unknown { outcome, error })) => {
            eprintln!("fixture-frame-write unknown after write: {error}");
            ("unknown", Some(error), *outcome)
        }
        Ok(Err(FrameWriteFailure::Refused(e))) => return Err(refuse("write", e)),
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
            cancelled.store(true, Ordering::SeqCst);
            return Err(refuse(
                "ax-worker",
                "AX worker timed out (bounded 15s); write state unknown (a late write remains possible), no replay; inspect the before/after facts before any retry"
                    .to_owned(),
            ));
        }
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
            cancelled.store(true, Ordering::SeqCst);
            return Err(refuse(
                "ax-worker",
                "AX worker died before reporting; write state unknown, no replay".to_owned(),
            ));
        }
    };
    let report = FixtureFrameReport {
        outcome: label.to_owned(),
        error,
        pid,
        wid: args.wid,
        caller: args.caller.clone(),
        secure_input: crate::ffi::secure_input_known(),
        ax_trusted: crate::ffi::ax_is_process_trusted(),
        proof_us,
        write_latency_us: outcome.write_latency_us,
        position_dispatch_ns: outcome.position_dispatch_ns,
        size_dispatch_ns: outcome.size_dispatch_ns,
        return_ns: outcome.return_ns,
        requested: FramePoint {
            x: requested.x,
            y: requested.y,
            w: requested.w,
            h: requested.h,
        },
        position_status: outcome.position_status,
        size_status: outcome.size_status,
        frame_before: outcome.before,
        frame_after: outcome.after,
        zoom_before: outcome.zoom_before,
        zoom_after: outcome.zoom_after,
        cg_owner_before,
        cg_owner_after: outcome.cg_owner_after,
        run_tag_agreement: true,
        synthetic: false,
        note: "development-only standalone frame diagnostic: fresh signed-fixture proof plus exactly one cross-process AXPosition/AXSize write with before/after facts; partial writes report per-leg statuses with no replay and no automatic restore; no menu traversal, no button press, no input synthesis"
            .to_owned(),
    };
    let json = serde_json::to_string(&report).map_err(|e| (1, e.to_string()))?;
    _watchdog.disarm();
    if label == "unknown" {
        return Err((1, json));
    }
    Ok(json)
}

/// Non-macOS fallback: the fixture transport does not exist here.
#[cfg(not(target_os = "macos"))]
pub fn run_fixture_frame_write(_args: &FixtureFrameWriteArgs) -> Result<String, (i32, String)> {
    Err((1, "fixture-frame-write is macOS-only".to_owned()))
}

/// Outcome of the single AX worker pass: before/after AX frames plus zoom
/// facts, per-leg write statuses, the write cost and the after CG owner.
/// `Ok` means the single write effect ran between the fact reads. `Err`
/// means write state is UNKNOWN, never no-write: commit/interrupt/cancel
/// refusals precede any write, but an AX failure or a post-write read failure
/// can follow an actual actuation. Never infer no-write from `Err` and never
/// replay without inspecting the before/after facts.
#[cfg(target_os = "macos")]
pub(crate) struct WorkerOutcome {
    before: FrameFact,
    after: FrameFact,
    zoom_before: crate::scope_inspect::ZoomFacts,
    zoom_after: crate::scope_inspect::ZoomFacts,
    position_status: String,
    size_status: String,
    write_latency_us: u64,
    position_dispatch_ns: Option<u64>,
    size_dispatch_ns: Option<u64>,
    return_ns: Option<u64>,
    cg_owner_after: Option<u32>,
}

/// Failure of the single AX worker pass: either a pre-actuation refusal (no
/// write was attempted, so no after-facts exist) or a post-actuation
/// unknown (at least one AX leg ran, so the caller still samples after-facts
/// and reports outcome unknown with exit 1). Neither path replays or
/// restores implicitly.
#[cfg(target_os = "macos")]
pub(crate) enum FrameWriteFailure {
    Refused(String),
    Unknown {
        outcome: Box<WorkerOutcome>,
        error: String,
    },
}

/// The whole AX pass on the owning worker thread: join the exact window,
/// read before-facts, repeat the cheap proof (fresh identity plus fresh CG
/// through the shared pure choke point), write exactly once as two ordered
/// legs with per-leg statuses, then read after-facts. Every `AxElement` is
/// constructed, used, and dropped here; nothing crosses to the waiting main
/// thread except plain facts.
#[cfg(target_os = "macos")]
#[allow(clippy::too_many_arguments)]
fn frame_write_on_worker(
    pid: u32,
    caller: &str,
    wid: u32,
    requested: RequestedFrame,
    signer: &str,
    snapshot: &SocketSnapshot,
    bound_exe: &str,
    bound_launch: &str,
    cancelled: &AtomicBool,
) -> Result<WorkerOutcome, FrameWriteFailure> {
    use crate::ffi::{
        AxFrame, ax_error_name, ax_get_frame, ax_set_messaging_timeout, ax_set_position,
        ax_set_size, ax_string_attribute,
    };
    use crate::fixture_menu::refuse_if_late_cancelled;
    use crate::fixture_zoom::{ZoomAuthInputs, check_zoom_authorization};
    // One shared bounded join: the retained `window` below is the exact
    // joined fixture window every later read uses.
    let window = join_ax_window(pid, wid).map_err(FrameWriteFailure::Refused)?;
    let pid_i32 = i32::try_from(pid)
        .map_err(|_| format!("frame worker: pid {pid} out of range"))
        .map_err(FrameWriteFailure::Refused)?;
    let before = read_frame_fact(&window);
    let zoom_before = crate::scope_inspect::read_zoom_facts(pid, wid);
    // Commit boundary: fresh identity plus fresh CG through the same pure
    // choke point as the pre-write proof (no socket, no subprocess). A
    // re-created window, a relaunched process, or an activated Secure Input
    // refuses here, before the window server is touched. The signature was
    // proven at bind and is never re-forked: the retained tail is compared
    // again here so the commit judges through the identical choke point.
    // When CG withholds the title but the fresh owner join already names the
    // fixture pid, one bounded AXTitle read off the retained joined window
    // fills the title source; a present CG title stays authoritative and an
    // absent CG entry still refuses.
    let fresh_cg = cg_entry_for(wid).map_err(FrameWriteFailure::Refused)?;
    let ax_title: Option<String> = match &fresh_cg {
        Some((owner, None)) if *owner == pid => ax_string_attribute(&window, "AXTitle")
            .ok()
            .flatten()
            .filter(|title| !title.is_empty()),
        _ => None,
    };
    let commit: Result<(), String> = (|| {
        frame_session_gates()?;
        let fresh = crate::ffi::ns_app_identity_for_pid(pid_i32);
        check_zoom_authorization(&ZoomAuthInputs {
            caller: caller.to_owned(),
            wid,
            announced_pid: pid,
            snapshot: snapshot.clone(),
            bundle: fresh.bundle,
            sig_tail: Some(signer.to_owned()),
            expected_signer: signer.to_owned(),
            live_exe: fresh.exe,
            bound_exe: bound_exe.to_owned(),
            live_launch: fresh.launch_date,
            bound_launch: bound_launch.to_owned(),
            cg_entry: fresh_cg,
            ax_title,
            secure_input: crate::ffi::secure_input_known(),
        })
        .map_err(|e| e.replace("refusing fixture zoom", "refusing fixture frame write"))
    })();
    let interrupted = crate::space_drag_diag_live::diag_interrupted();
    let fenced = cancelled.load(Ordering::SeqCst);
    let target = AxFrame {
        x: requested.x,
        y: requested.y,
        w: requested.w,
        h: requested.h,
    };
    // The single write of this invocation: two ordered legs (position, then
    // size) with per-leg statuses. The retained handle is re-proven live
    // inside the effect immediately before actuation, so a handle that went
    // stale after the commit refuses instead of writing; the effect runs
    // exactly once with no retry and no automatic restore. A fresh
    // session/cancellation gate runs between the blocking legs, so a
    // lock/Secure Input flip or a main-bound lapse after an accepted
    // position write still blocks the size leg and reports unknown with the
    // known statuses retained. Post-actuation leg failures are carried in
    // the returned [`AttemptedWrite`], never dropped: the caller still
    // samples after-facts and reports outcome unknown.
    // Each leg stamps the shared host monotonic clock at its dispatch edge
    // (the sampler joins these stamps on the same origin); the stamps are
    // captured through `&mut` out-vars so the single-effect signature and
    // its fake-effect tests stay unchanged.
    let mut position_dispatch_ns: Option<u64> = None;
    let mut size_dispatch_ns: Option<u64> = None;
    let mut return_ns: Option<u64> = None;
    let attempted = drive_single_frame_write(
        &requested,
        &before,
        commit,
        interrupted,
        fenced,
        || {
            let _ = ax_set_messaging_timeout(&window, 2.0);
            match ax_get_frame(&window) {
                Ok(_) => {}
                Err(code) => {
                    return Err(format!(
                        "frame worker: retained target liveness unreadable ({}); refusing stale target",
                        ax_error_name(code)
                    ));
                }
            }
            // Last pre-actuation gates before any write: fresh session gates
            // plus the late fence recheck, so a lock/Secure Input flip or a
            // main-bound lapse during the blocking reads refuses here with
            // no write attempted.
            frame_session_gates().map_err(|e| {
                format!("frame worker: session gate lost immediately before write; refusing ({e})")
            })?;
            refuse_if_late_cancelled(
                crate::space_drag_diag_live::diag_interrupted(),
                cancelled.load(Ordering::SeqCst),
            )
            .map_err(|e| e.replace("menu worker", "frame worker"))?;
            let write_start = Instant::now();
            let attempted = run_write_legs(
                || {
                    position_dispatch_ns = crate::ffi::host_now_ns();
                    ax_set_position(&window, &target).map_err(ax_error_name)
                },
                || {
                    frame_session_gates().map_err(|e| {
                        format!(
                            "frame worker: session gate lost between legs; refusing ({e})"
                        )
                    })?;
                    refuse_if_late_cancelled(
                        crate::space_drag_diag_live::diag_interrupted(),
                        cancelled.load(Ordering::SeqCst),
                    )
                    .map_err(|e| e.replace("menu worker", "frame worker"))
                },
                || {
                    size_dispatch_ns = crate::ffi::host_now_ns();
                    ax_set_size(&window, &target).map_err(ax_error_name)
                },
            );
            let return_ns_here = crate::ffi::host_now_ns();
            let latency = write_start
                .elapsed()
                .as_micros()
                .min(u128::from(u64::MAX)) as u64;
            return_ns = return_ns_here;
            Ok((attempted, latency))
        },
    )
    .map_err(FrameWriteFailure::Refused)?;
    let (attempted, write_latency_us) = attempted;
    let after = read_frame_fact(&window);
    let zoom_after = crate::scope_inspect::read_zoom_facts(pid, wid);
    let cg_owner_after = cg_entry_for(wid).ok().flatten().map(|(owner, _)| owner);
    let outcome = WorkerOutcome {
        before,
        after,
        zoom_before,
        zoom_after,
        position_status: attempted.position_status,
        size_status: attempted.size_status,
        write_latency_us,
        position_dispatch_ns,
        size_dispatch_ns,
        return_ns,
        cg_owner_after,
    };
    match attempted.error {
        None => Ok(outcome),
        Some(error) => Err(FrameWriteFailure::Unknown {
            outcome: Box::new(outcome),
            error,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn framed(x: f64, y: f64, w: f64, h: f64) -> RequestedFrame {
        RequestedFrame { x, y, w, h }
    }

    fn readable_frame() -> FrameFact {
        FrameFact {
            frame: Some(FramePoint {
                x: 100.0,
                y: 100.0,
                w: 800.0,
                h: 600.0,
            }),
            status: "ok".to_owned(),
        }
    }

    fn unreadable_frame() -> FrameFact {
        FrameFact {
            frame: None,
            status: "kAXErrorNoValue".to_owned(),
        }
    }

    #[test]
    fn ordinary_frames_pass_and_absurd_frames_refuse() {
        assert!(check_frame_bounds(&framed(100.0, 100.0, 800.0, 600.0)).is_ok());
        assert!(check_frame_bounds(&framed(-1500.0, 40.0, 320.0, 240.0)).is_ok());
        for (name, frame) in [
            ("nan-x", framed(f64::NAN, 0.0, 100.0, 100.0)),
            ("inf-w", framed(0.0, 0.0, f64::INFINITY, 100.0)),
            ("zero-w", framed(0.0, 0.0, 0.0, 100.0)),
            ("neg-h", framed(0.0, 0.0, 100.0, -5.0)),
            ("huge-w", framed(0.0, 0.0, 99999.0, 100.0)),
            ("far-x", framed(99999.0, 0.0, 100.0, 100.0)),
        ] {
            assert!(check_frame_bounds(&frame).is_err(), "{name} must refuse");
        }
    }

    #[test]
    fn authorize_frame_write_orders_gates_before_any_write() {
        let frame = framed(100.0, 100.0, 800.0, 600.0);
        let before = readable_frame();
        let ok: Result<(), String> = Ok(());
        assert!(authorize_frame_write(&frame, &before, &ok, false, false).is_ok());
        // Ordering is pinned: bounds beat preimage, preimage beats commit,
        // commit beats interrupt, interrupt beats cancel.
        let bad_frame = framed(0.0, 0.0, 0.0, 100.0);
        let err = authorize_frame_write(
            &bad_frame,
            &unreadable_frame(),
            &Err("commit bad".to_owned()),
            true,
            true,
        )
        .unwrap_err();
        assert!(err.contains("positive"), "bounds must win: {err}");
        let err = authorize_frame_write(
            &frame,
            &unreadable_frame(),
            &Err("commit bad".to_owned()),
            true,
            true,
        )
        .unwrap_err();
        assert!(err.contains("preimage"), "preimage must win: {err}");
        let err = authorize_frame_write(&frame, &before, &Err("commit bad".to_owned()), true, true)
            .unwrap_err();
        assert!(
            err.contains("frame worker commit"),
            "commit must win: {err}"
        );
        let err = authorize_frame_write(&frame, &before, &ok, true, true).unwrap_err();
        assert!(err.contains("latched"), "interrupt must win: {err}");
        let err = authorize_frame_write(&frame, &before, &ok, false, true).unwrap_err();
        assert!(err.contains("bound lapsed"), "cancel must refuse: {err}");
    }

    #[test]
    fn drive_single_frame_write_writes_once_on_ok_and_never_on_refusal() {
        use std::cell::Cell;
        let frame = framed(100.0, 100.0, 800.0, 600.0);
        let before = readable_frame();
        let ok: Result<(), String> = Ok(());
        // Authorized: the fake effect runs exactly once and its value passes
        // through (it stands in for the single AX frame-write call site).
        let calls = Cell::new(0u32);
        let (attempted, latency) =
            drive_single_frame_write(&frame, &before, ok, false, false, || {
                calls.set(calls.get() + 1);
                Ok((
                    AttemptedWrite {
                        position_status: "ok".to_owned(),
                        size_status: "ok".to_owned(),
                        error: None,
                    },
                    77u64,
                ))
            })
            .expect("authorized write drives");
        assert_eq!(attempted.position_status, "ok");
        assert_eq!(attempted.size_status, "ok");
        assert!(attempted.error.is_none());
        assert_eq!(latency, 77);
        assert_eq!(calls.get(), 1);
        // Every refusal drives zero effects.
        type RefusalVector = (
            &'static str,
            RequestedFrame,
            FrameFact,
            Result<(), String>,
            bool,
            bool,
        );
        let bad_frame = framed(0.0, 0.0, 0.0, 100.0);
        let cases: Vec<RefusalVector> = vec![
            ("bounds", bad_frame, readable_frame(), Ok(()), false, false),
            (
                "preimage",
                framed(100.0, 100.0, 800.0, 600.0),
                unreadable_frame(),
                Ok(()),
                false,
                false,
            ),
            (
                "commit",
                framed(100.0, 100.0, 800.0, 600.0),
                readable_frame(),
                Err("commit bad".to_owned()),
                false,
                false,
            ),
            (
                "interrupt",
                framed(100.0, 100.0, 800.0, 600.0),
                readable_frame(),
                Ok(()),
                true,
                false,
            ),
            (
                "cancel",
                framed(100.0, 100.0, 800.0, 600.0),
                readable_frame(),
                Ok(()),
                false,
                true,
            ),
        ];
        for (name, frame, facts, commit, interrupted, cancelled) in cases {
            let calls = Cell::new(0u32);
            let outcome =
                drive_single_frame_write(&frame, &facts, commit, interrupted, cancelled, || {
                    calls.set(calls.get() + 1);
                    Ok((
                        AttemptedWrite {
                            position_status: "ok".to_owned(),
                            size_status: "ok".to_owned(),
                            error: None,
                        },
                        0u64,
                    ))
                });
            assert!(outcome.is_err(), "{name} must refuse");
            assert_eq!(calls.get(), 0, "{name} must drive no effect");
        }
    }

    #[test]
    fn write_legs_retain_known_statuses_on_partial_failure() {
        // Size refusal after an accepted position write: the position status
        // is retained, the size status is recorded, and the error is kept.
        let attempted = run_write_legs(|| Ok(()), || Ok(()), || Err("kAXErrorFailure".to_owned()));
        assert_eq!(attempted.position_status, "ok");
        assert_eq!(attempted.size_status, "kAXErrorFailure");
        let error = attempted.error.expect("size failure must record an error");
        assert!(error.contains("AXSize"), "unexpected error: {error}");
        // Position refusal: the position status is retained, the size leg
        // never runs, and the error is kept.
        let attempted = run_write_legs(|| Err("kAXErrorNoValue".to_owned()), || Ok(()), || Ok(()));
        assert_eq!(attempted.position_status, "kAXErrorNoValue");
        assert_eq!(attempted.size_status, SIZE_NOT_ATTEMPTED);
        assert!(attempted.error.is_some());
        // All legs ok: no error.
        let attempted = run_write_legs(|| Ok(()), || Ok(()), || Ok(()));
        assert_eq!(attempted.position_status, "ok");
        assert_eq!(attempted.size_status, "ok");
        assert!(attempted.error.is_none());
    }

    #[test]
    fn between_legs_gate_flip_blocks_size_leg() {
        use std::cell::Cell;
        let size_calls = Cell::new(0u32);
        // The real late-cancellation fence trips between the legs: the size
        // leg must never run and the report keeps position ok with the size
        // leg unattempted.
        let attempted = run_write_legs(
            || Ok(()),
            || crate::fixture_menu::refuse_if_late_cancelled(true, false),
            || {
                size_calls.set(size_calls.get() + 1);
                Ok(())
            },
        );
        assert_eq!(attempted.position_status, "ok");
        assert_eq!(attempted.size_status, SIZE_NOT_ATTEMPTED);
        assert!(attempted.error.is_some());
        assert_eq!(size_calls.get(), 0, "fence flip must block the size leg");
    }

    #[test]
    fn report_carries_no_secret_or_content_keys() {
        let zoom = crate::scope_inspect::ZoomFacts {
            attr_names_status: "ok".to_owned(),
            attr_names: Some(vec!["AXZoomed".to_owned()]),
            has_zoomed_attr: Some(false),
            zoomed: None,
            zoomed_status: "kAXErrorAttributeUnsupported".to_owned(),
            zoom_button: crate::scope_inspect::ZoomButtonFact {
                found: true,
                via: "direct-attr".to_owned(),
                lookup_status: "ok".to_owned(),
                has_enabled_attr: Some(true),
                has_value_attr: Some(false),
                enabled: Some(true),
                enabled_status: "ok".to_owned(),
                value_bool: None,
                value_status: "kAXErrorAttributeUnsupported".to_owned(),
            },
            fullscreen_button: crate::scope_inspect::ZoomButtonFact {
                found: false,
                via: "direct-attr".to_owned(),
                lookup_status: "kAXErrorNoValue".to_owned(),
                has_enabled_attr: None,
                has_value_attr: None,
                enabled: None,
                enabled_status: "not-read".to_owned(),
                value_bool: None,
                value_status: "not-read".to_owned(),
            },
        };
        let report = FixtureFrameReport {
            outcome: "written".to_owned(),
            error: None,
            pid: 4242,
            wid: 7,
            caller: "frame-leg-1".to_owned(),
            secure_input: Some(false),
            ax_trusted: Some(true),
            proof_us: 11,
            write_latency_us: 13,
            position_dispatch_ns: Some(1_700_000_000_000_000_000),
            size_dispatch_ns: Some(1_700_000_000_001_000_000),
            return_ns: Some(1_700_000_000_002_000_000),
            requested: FramePoint {
                x: 100.0,
                y: 100.0,
                w: 800.0,
                h: 600.0,
            },
            position_status: "ok".to_owned(),
            size_status: "ok".to_owned(),
            frame_before: readable_frame(),
            frame_after: readable_frame(),
            zoom_before: zoom.clone(),
            zoom_after: zoom,
            cg_owner_before: Some(4242),
            cg_owner_after: Some(4242),
            run_tag_agreement: true,
            synthetic: false,
            note: "test".to_owned(),
        };
        let json = serde_json::to_string(&report).unwrap();
        for key in [
            "\"title\"",
            "\"content\"",
            "\"path\"",
            "\"exe\"",
            "\"executable\"",
            "\"nonce\"",
            "\"socket\"",
            "\"signer\"",
            "\"bundle\"",
        ] {
            assert!(!json.contains(key), "frame report must not carry {key}");
        }
        assert!(json.contains("\"outcome\":\"written\""));
        assert!(json.contains("\"synthetic\":false"));
        assert!(json.contains("\"position_status\":\"ok\""));
    }

    #[test]
    fn unknown_report_keeps_leg_statuses_and_no_secrets() {
        let report = FixtureFrameReport {
            outcome: "unknown".to_owned(),
            error: Some(
                "frame worker: AXSize write failed after accepted AXPosition (kAXErrorFailure) with no retry and no automatic restore; window may be partially mutated, inspect the after-facts"
                    .to_owned(),
            ),
            pid: 4242,
            wid: 7,
            caller: "frame-leg-1".to_owned(),
            secure_input: Some(false),
            ax_trusted: Some(true),
            proof_us: 11,
            write_latency_us: 13,
            position_dispatch_ns: Some(1_700_000_000_000_000_000),
            size_dispatch_ns: Some(1_700_000_000_001_000_000),
            return_ns: Some(1_700_000_000_002_000_000),
            requested: FramePoint {
                x: 100.0,
                y: 100.0,
                w: 800.0,
                h: 600.0,
            },
            position_status: "ok".to_owned(),
            size_status: "kAXErrorFailure".to_owned(),
            frame_before: readable_frame(),
            frame_after: readable_frame(),
            zoom_before: crate::scope_inspect::ZoomFacts {
                attr_names_status: "ok".to_owned(),
                attr_names: Some(vec!["AXZoomed".to_owned()]),
                has_zoomed_attr: Some(false),
                zoomed: None,
                zoomed_status: "kAXErrorAttributeUnsupported".to_owned(),
                zoom_button: crate::scope_inspect::ZoomButtonFact {
                    found: true,
                    via: "direct-attr".to_owned(),
                    lookup_status: "ok".to_owned(),
                    has_enabled_attr: Some(true),
                    has_value_attr: Some(false),
                    enabled: Some(true),
                    enabled_status: "ok".to_owned(),
                    value_bool: None,
                    value_status: "kAXErrorAttributeUnsupported".to_owned(),
                },
                fullscreen_button: crate::scope_inspect::ZoomButtonFact {
                    found: false,
                    via: "direct-attr".to_owned(),
                    lookup_status: "kAXErrorNoValue".to_owned(),
                    has_enabled_attr: None,
                    has_value_attr: None,
                    enabled: None,
                    enabled_status: "not-read".to_owned(),
                    value_bool: None,
                    value_status: "not-read".to_owned(),
                },
            },
            zoom_after: crate::scope_inspect::ZoomFacts {
                attr_names_status: "ok".to_owned(),
                attr_names: Some(vec!["AXZoomed".to_owned()]),
                has_zoomed_attr: Some(false),
                zoomed: None,
                zoomed_status: "kAXErrorAttributeUnsupported".to_owned(),
                zoom_button: crate::scope_inspect::ZoomButtonFact {
                    found: true,
                    via: "direct-attr".to_owned(),
                    lookup_status: "ok".to_owned(),
                    has_enabled_attr: Some(true),
                    has_value_attr: Some(false),
                    enabled: Some(true),
                    enabled_status: "ok".to_owned(),
                    value_bool: None,
                    value_status: "kAXErrorAttributeUnsupported".to_owned(),
                },
                fullscreen_button: crate::scope_inspect::ZoomButtonFact {
                    found: false,
                    via: "direct-attr".to_owned(),
                    lookup_status: "kAXErrorNoValue".to_owned(),
                    has_enabled_attr: None,
                    has_value_attr: None,
                    enabled: None,
                    enabled_status: "not-read".to_owned(),
                    value_bool: None,
                    value_status: "not-read".to_owned(),
                },
            },
            cg_owner_before: Some(4242),
            cg_owner_after: Some(4242),
            run_tag_agreement: true,
            synthetic: false,
            note: "test".to_owned(),
        };
        let json = serde_json::to_string(&report).unwrap();
        for key in [
            "\"title\"",
            "\"content\"",
            "\"path\"",
            "\"exe\"",
            "\"executable\"",
            "\"nonce\"",
            "\"socket\"",
            "\"signer\"",
            "\"bundle\"",
        ] {
            assert!(!json.contains(key), "frame report must not carry {key}");
        }
        assert!(json.contains("\"outcome\":\"unknown\""));
        assert!(json.contains("\"position_status\":\"ok\""));
        assert!(json.contains("\"size_status\":\"kAXErrorFailure\""));
    }
}

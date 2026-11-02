#![forbid(unsafe_code)]

//! Development-only standalone zoom diagnostic for the signed live fixture.
//!
//! Exactly one native `AXZoomButton` `AXPress` on one exact fixture window,
//! with fresh signed-fixture proof before the press and immediate
//! before/after AX/CG/button facts. Standalone experiment only: no leg
//! admission, no controller lifecycle, no product protocol or state.
//!
//! Proof shape (refuses before any press, first failure wins):
//! session gates (SLPS symbols, AX trust, Secure Input observed inactive -
//! unknown refuses, unlocked session), the serve announcement file (the only
//! nonce source), live bundle/executable/launch reads plus `codesign`
//! `--verify` and the designated-requirement read against the explicit
//! `--signer`, a nonce-gated read-only `get` over the announced Unix socket
//! (the announced socket path is used and shape-checked; the exchange proves
//! the nonce live), and the CG owner join plus the sentinel-title caller
//! join cross-checked against the live server snapshot's run tag.
//!
//! The commit boundary repeats the cheap proof (fresh identity plus fresh CG
//! owner/title/run-tag join through the same pure choke point, no socket, no
//! subprocess) on the AX worker immediately before the single press. One
//! press per invocation, no retry on uncertain outcome. A main-thread timeout
//! return means press state unknown (a late press remains possible): never
//! retry blindly, inspect the before/after facts first. Outputs are
//! metadata-only JSON (IDs, stages, latencies - never titles, content,
//! paths, nonces, executables, or requirement text).
//!
//! No input synthesis, no `AXFullScreenButton` presses, no foreign scopes,
//! menu traversal, or arbitrary AX action vocabulary. The M4-3 fixture leg
//! observed this `AXZoomButton` press entering native fullscreen and creating
//! a Space: the accessor name does not guarantee ordinary zoom. This is not
//! a maximize executor; native effect readback is required separately.

use serde::Serialize;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

use crate::fixture_post::{
    SocketSnapshot, bundle_verdict, caller_id_valid, fixture_designated_tail,
    fixture_process_identity, lifetime_verdict, parse_fixture_announcement, parse_fixture_title,
    run_tag_verdict, secure_input_verdict, socket_snapshot_for,
};

/// Bound for the single AX worker (join plus commit plus one press plus
/// after-facts). Each AX read carries its own 2 s messaging timeout; the
/// worker bound caps the whole pass.
const AX_WORKER_BOUND: Duration = Duration::from_secs(15);
/// Hard lifetime for the whole diagnostic (proof plus worker plus report).
const HARD_LIMIT: Duration = Duration::from_secs(60);

/// CLI entry parameters for one standalone zoom press: the serve
/// announcement file (socket/pid/nonce source), the selected fixture caller
/// id, the exact target window id, and the expected designated requirement
/// (from `fixture-verify` output at preflight; machine-specific, never
/// hardcoded here). Bundled so the entry keeps a narrow call shape.
pub struct FixtureZoomPressArgs {
    pub announcement_file: String,
    pub caller: String,
    pub wid: u32,
    pub signer: String,
}

/// Plain-data authorization inputs (no OS touch): the single test choke
/// point. Both the pre-press proof and the commit-boundary re-proof judge
/// through [`check_zoom_authorization`]; the commit pass carries fresh live
/// reads with the retained bind values (`snapshot`, `expected_signer`,
/// `bound_exe`, `bound_launch`) and no new socket exchange. `ax_title` is
/// the bounded AXTitle read off the exact joined fixture window, used only
/// when the CG entry withholds the title (`None`); a present CG title stays
/// authoritative and is never bypassed.
#[derive(Debug, Clone)]
pub(crate) struct ZoomAuthInputs {
    pub caller: String,
    pub wid: u32,
    pub announced_pid: u32,
    pub snapshot: SocketSnapshot,
    pub bundle: Option<String>,
    pub sig_tail: Option<String>,
    pub expected_signer: String,
    pub live_exe: Option<String>,
    pub bound_exe: String,
    pub live_launch: Option<String>,
    pub bound_launch: String,
    pub cg_entry: Option<(u32, Option<String>)>,
    pub ax_title: Option<String>,
    pub secure_input: Option<bool>,
}

/// Pure zoom authorization over gathered reads (no OS touch): caller shape,
/// exact wid, strict Secure Input (only observed inactive proceeds),
/// socket pid/caller join against the announcement, exact fixture bundle,
/// exact designated requirement, process-lifetime continuity, then the CG
/// owner plus the sentinel-title caller/run-tag join. The title resolves
/// from the CG entry when present (a present-but-wrong CG title refuses,
/// never falls through); only a withheld CG title may use the bounded AX
/// fallback, judged through the identical verdicts. First failure wins, so
/// gate ordering is pinned by test.
pub(crate) fn check_zoom_authorization(inputs: &ZoomAuthInputs) -> Result<(), String> {
    if !caller_id_valid(&inputs.caller) {
        return Err("refusing fixture zoom: caller id malformed".to_owned());
    }
    if inputs.wid == 0 {
        return Err("refusing fixture zoom to window id 0".to_owned());
    }
    secure_input_verdict(inputs.secure_input).map_err(|e| format!("refusing fixture zoom: {e}"))?;
    if inputs.snapshot.pid != inputs.announced_pid {
        return Err(format!(
            "refusing fixture zoom: live server pid {} != announced pid {}; refusing",
            inputs.snapshot.pid, inputs.announced_pid
        ));
    }
    if inputs.snapshot.caller != inputs.caller {
        return Err(
            "refusing fixture zoom: socket snapshot caller mismatch (stale or foreign window)"
                .to_owned(),
        );
    }
    bundle_verdict(inputs.bundle.as_deref()).map_err(|e| format!("refusing fixture zoom: {e}"))?;
    match inputs.sig_tail.as_deref() {
        Some(tail) if tail == inputs.expected_signer.trim() => {}
        Some(_) => {
            return Err(
                "refusing fixture zoom: designated requirement mismatch (not the expected fixture signer); refusing"
                    .to_owned(),
            );
        }
        None => {
            return Err(
                "refusing fixture zoom: designated requirement unreadable; refusing".to_owned(),
            );
        }
    }
    lifetime_verdict(
        inputs.live_exe.as_deref(),
        &inputs.bound_exe,
        inputs.live_launch.as_deref(),
        &inputs.bound_launch,
    )
    .map_err(|e| format!("refusing fixture zoom: {e}"))?;
    let (owner_pid, cg_title) = inputs.cg_entry.as_ref().ok_or_else(|| {
        format!(
            "refusing fixture zoom: wid {} not present in CGWindowList; refusing without exact PID association",
            inputs.wid
        )
    })?;
    if *owner_pid != inputs.announced_pid {
        return Err(format!(
            "refusing fixture zoom: wid {} belongs to pid {owner_pid} (not fixture pid {}); refusing",
            inputs.wid, inputs.announced_pid
        ));
    }
    // Title source: a present CG title is authoritative (a present-but-wrong
    // one refuses below, never falls through to AX). Only a withheld CG
    // title uses the bounded AX fallback off the exact joined fixture
    // window; with neither, the target is stale. Both sources judge through
    // the identical sentinel/caller/run-tag verdicts below: the fallback
    // changes the channel, never the proof.
    let resolved = match (cg_title.as_deref(), inputs.ax_title.as_deref()) {
        (Some(title), _) => title,
        (None, Some(title)) => title,
        (None, None) => {
            return Err(
                "refusing fixture zoom: target title unreadable (CG withheld it and no AX fallback was read); refusing stale target"
                    .to_owned(),
            );
        }
    };
    let (native_caller, native_tag) =
        parse_fixture_title(resolved).map_err(|e| format!("refusing fixture zoom: {e}"))?;
    if native_caller != inputs.caller {
        return Err(
            "refusing fixture zoom: native title caller mismatch (stale or foreign window)"
                .to_owned(),
        );
    }
    run_tag_verdict(&native_tag, &inputs.snapshot.run_tag)
        .map_err(|e| format!("refusing fixture zoom: {e}"))?;
    Ok(())
}

/// Pure button pressability over the before-facts: the zoom button must be
/// joined AND observed enabled. A missing, disabled, or unreadable-enabled
/// button refuses before any commit proof or press. Pure; unit-tested.
#[cfg(target_os = "macos")]
pub(crate) fn zoom_button_pressable(
    before: &crate::scope_inspect::ZoomFacts,
) -> Result<(), String> {
    if !before.zoom_button.found {
        return Err(format!(
            "refusing fixture zoom: AXZoomButton join missed ({})",
            before.zoom_button.lookup_status
        ));
    }
    match before.zoom_button.enabled {
        Some(true) => Ok(()),
        Some(false) => {
            Err("refusing fixture zoom: AXZoomButton observed disabled; refusing".to_owned())
        }
        None => Err(format!(
            "refusing fixture zoom: AXZoomButton enabled unreadable ({}); refusing",
            before.zoom_button.enabled_status
        )),
    }
}

/// Pure commit-to-press decision in press order: button pressable, then the
/// commit re-proof verdict, then the latched-interrupt check, then the
/// main-bound cancellation fence. Any Err must prevent the single press;
/// only Ok authorizes it. Pure; unit-tested.
#[cfg(target_os = "macos")]
pub(crate) fn authorize_press(
    before: &crate::scope_inspect::ZoomFacts,
    commit: &Result<(), String>,
    interrupted: bool,
    cancelled: bool,
) -> Result<(), String> {
    zoom_button_pressable(before)?;
    if let Err(error) = commit {
        return Err(format!("zoom worker commit: {error}"));
    }
    if interrupted {
        return Err("zoom worker: SIGINT/SIGTERM latched; refusing press".to_owned());
    }
    if cancelled {
        return Err(
            "zoom worker: main bound lapsed before the press; refusing (press state stays provable only via facts)"
                .to_owned(),
        );
    }
    Ok(())
}

/// Drive exactly one press effect: the pure decision first, then a single
/// effect call with no retry. Uncertain outcomes propagate as Err and the
/// caller must not replay. Unit-tested with a fake effect counter standing
/// in for the single AXPress call site.
#[cfg(target_os = "macos")]
pub(crate) fn drive_single_press(
    before: &crate::scope_inspect::ZoomFacts,
    commit: Result<(), String>,
    interrupted: bool,
    cancelled: bool,
    effect: impl FnOnce() -> Result<u64, String>,
) -> Result<u64, String> {
    authorize_press(before, &commit, interrupted, cancelled)?;
    effect()
}

/// Shared live session gates before proof and inside the commit re-proof (no
/// presses): SLPS symbols, AX trust, Secure Input (strict: unknown refuses),
/// unlocked session. No CG post-access gate: this route performs no CG post.
#[cfg(target_os = "macos")]
fn zoom_session_gates() -> Result<(), String> {
    if !crate::ffi::slps_available() {
        return Err("slps-unavailable: SLPS focus symbols unavailable; stop".to_owned());
    }
    if crate::ffi::ax_is_process_trusted() != Some(true) {
        return Err("ax-trust: AX trust not observed true; stop".to_owned());
    }
    secure_input_verdict(crate::ffi::secure_input_known())?;
    match crate::ffi::session_state().lock {
        crate::ffi::SessionLock::Unlocked => Ok(()),
        crate::ffi::SessionLock::Locked => Err("session: session locked; refusing zoom".to_owned()),
        crate::ffi::SessionLock::Unknown => {
            Err("session: session unknown; refusing zoom".to_owned())
        }
    }
}

/// Join the exact `(pid, wid)` AX window on the calling thread: application
/// element, 2 s messaging timeout, scan with a per-window 2 s timeout set
/// BEFORE the id read (an unsettable window is skipped, never read
/// unbounded), then a 2 s timeout on the retained target (refuses when
/// unsettable: later reads on it must stay bounded). Shared by the worker
/// and the main-thread title fallback so both joins carry identical bounds.
#[cfg(target_os = "macos")]
fn join_ax_window(pid: u32, wid: u32) -> Result<crate::ffi::AxElement, String> {
    use crate::ffi::{AxElement, ax_error_name, ax_set_messaging_timeout, ax_window_id};
    let pid_i32 = i32::try_from(pid).map_err(|_| format!("zoom join: pid {pid} out of range"))?;
    let app = AxElement::application(pid_i32)
        .ok_or_else(|| format!("zoom join: no AX element for pid {pid}"))?;
    if ax_set_messaging_timeout(&app, 2.0) != 0 {
        return Err("zoom join: messaging timeout unset before window join".to_owned());
    }
    let windows = app.windows().map_err(|code| {
        format!(
            "zoom join: AX window list unreadable ({})",
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
        return Err(format!("zoom join: window wid {wid} rejoin missed"));
    };
    if ax_set_messaging_timeout(&window, 2.0) != 0 {
        return Err("zoom join: messaging timeout unset on retained target; refusing".to_owned());
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

#[derive(Debug, Clone, Serialize)]
struct FixtureZoomReport {
    outcome: String,
    pid: u32,
    wid: u32,
    caller: String,
    secure_input: Option<bool>,
    ax_trusted: Option<bool>,
    proof_us: u64,
    press_latency_us: u64,
    cg_owner_before: Option<u32>,
    cg_owner_after: Option<u32>,
    run_tag_agreement: bool,
    zoom_before: crate::scope_inspect::ZoomFacts,
    zoom_after: crate::scope_inspect::ZoomFacts,
    synthetic: bool,
    note: String,
}

/// Standalone zoom diagnostic: fresh proof, then exactly one native
/// `AXZoomButton` `AXPress` on an AX worker with a commit-boundary re-proof,
/// then before/after facts. Exit codes: 2 usage/validation, 1 gate/proof/
/// press failure with evidence.
#[cfg(target_os = "macos")]
pub fn run_fixture_zoom_press(args: &FixtureZoomPressArgs) -> Result<String, (i32, String)> {
    let err_json = |stage: &str, error: String| -> (i32, String) {
        (
            1,
            serde_json::json!({"stage": stage, "error": error}).to_string(),
        )
    };
    let refuse = |stage: &str, error: String| -> (i32, String) {
        eprintln!("fixture-zoom-press refused at {stage}: {error}");
        err_json(stage, error)
    };
    if args.announcement_file.is_empty() {
        return Err((
            2,
            "fixture-zoom-press needs a nonempty --announcement-file".to_owned(),
        ));
    }
    if args.signer.trim().is_empty() {
        return Err((2, "fixture-zoom-press needs a nonempty --signer".to_owned()));
    }
    if !caller_id_valid(&args.caller) {
        return Err((2, "refusing fixture zoom: caller id malformed".to_owned()));
    }
    if args.wid == 0 {
        return Err((2, "refusing fixture zoom to window id 0".to_owned()));
    }
    let mut _watchdog = crate::watchdog::Watchdog::arm(HARD_LIMIT).map_err(|e| (1, e))?;
    zoom_session_gates().map_err(|e| refuse("session", e))?;
    let _interrupt =
        crate::space_drag_diag_live::InterruptArmed::arm().map_err(|e| refuse("interrupt", e))?;
    let proof_start = Instant::now();
    let raw = std::fs::read_to_string(&args.announcement_file).map_err(|e| {
        (
            2,
            format!("fixture-zoom-press announcement file unreadable ({e})"),
        )
    })?;
    let announced = parse_fixture_announcement(raw.trim())
        .map_err(|e| (2, format!("announcement refused: {e}")))?;
    let pid = announced.pid;
    if pid <= 1 {
        return Err((2, format!("refusing fixture zoom to reserved pid {pid}")));
    }
    if pid == std::process::id() {
        return Err((
            2,
            "refusing fixture zoom to the controller itself".to_owned(),
        ));
    }
    if i32::try_from(pid).is_err() {
        return Err((2, format!("refusing fixture zoom: pid {pid} out of range")));
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
                "refusing fixture zoom: live server pid {} != announced pid {pid}; refusing",
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
    check_zoom_authorization(&bound).map_err(|e| refuse("proof", e))?;
    if crate::space_drag_diag_live::diag_interrupted() {
        return Err(refuse(
            "interrupted",
            "interrupted: SIGINT/SIGTERM latched; refusing press".to_owned(),
        ));
    }
    let proof_us = proof_start.elapsed().as_micros().min(u128::from(u64::MAX)) as u64;
    // The single AX worker owns every `AxElement` (construct, use, drop);
    // the main thread only waits bounded. The worker repeats the cheap
    // proof through the same pure choke point immediately before the press
    // (fresh identity plus fresh CG, no socket, no subprocess), so a window
    // the initial proof outlasts refuses at the commit boundary.
    let worker_caller = args.caller.clone();
    let worker_wid = args.wid;
    let worker_signer = args.signer.clone();
    // Best-effort cancellation fence: set when the main bound lapses so a
    // still-running worker refuses instead of pressing late. It narrows but
    // cannot close the post-timeout press window of a synchronous AXPress
    // (flag set after the worker's final check still races), so a timeout
    // return always means press state UNKNOWN, never no-press, and never a
    // replay without inspecting the before/after facts.
    let cancelled = Arc::new(AtomicBool::new(false));
    let worker_cancelled = Arc::clone(&cancelled);
    let (done_tx, done_rx) = std::sync::mpsc::channel::<Result<WorkerOutcome, String>>();
    std::thread::Builder::new()
        .name(format!("fixture-zoom-press-{pid}"))
        .spawn(move || {
            let outcome = zoom_press_on_worker(
                pid,
                &worker_caller,
                worker_wid,
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
    // worker sets the fence and reports press state unknown (a late press
    // remains possible past the timeout); the invocation never replays.
    let outcome = match done_rx.recv_timeout(AX_WORKER_BOUND) {
        Ok(outcome) => outcome.map_err(|e| refuse("press", e))?,
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
            cancelled.store(true, Ordering::SeqCst);
            return Err(refuse(
                "ax-worker",
                "AX worker timed out (bounded 15s); press state unknown (a late press remains possible), no replay; inspect the before/after facts before any retry"
                    .to_owned(),
            ));
        }
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
            cancelled.store(true, Ordering::SeqCst);
            return Err(refuse(
                "ax-worker",
                "AX worker died before reporting; press state unknown, no replay".to_owned(),
            ));
        }
    };
    let report = FixtureZoomReport {
        outcome: "pressed".to_owned(),
        pid,
        wid: args.wid,
        caller: args.caller.clone(),
        secure_input: crate::ffi::secure_input_known(),
        ax_trusted: crate::ffi::ax_is_process_trusted(),
        proof_us,
        press_latency_us: outcome.press_latency_us,
        cg_owner_before,
        cg_owner_after: outcome.cg_owner_after,
        run_tag_agreement: true,
        zoom_before: outcome.before,
        zoom_after: outcome.after,
        synthetic: false,
        note: "development-only standalone button diagnostic: fresh signed-fixture proof plus exactly one AXZoomButton AXPress; effect unclassified, native readback required separately; may enter native fullscreen; no input synthesis or menu traversal"
            .to_owned(),
    };
    let out = serde_json::to_string(&report).map_err(|e| (1, e.to_string()));
    _watchdog.disarm();
    out
}

/// Non-macOS fallback: the fixture transport does not exist here.
#[cfg(not(target_os = "macos"))]
pub fn run_fixture_zoom_press(_args: &FixtureZoomPressArgs) -> Result<String, (i32, String)> {
    Err((1, "fixture-zoom-press is macOS-only".to_owned()))
}

/// Outcome of the single AX worker pass: before/after zoom facts plus the
/// press cost and the after CG owner. `Ok` means exactly one press happened
/// between the two fact reads. `Err` means press state is UNKNOWN, never
/// no-press: commit/interrupt/cancel refusals precede any press, but an AX
/// failure or a post-press read failure can follow an actual actuation.
/// Never infer no-press from `Err` and never replay without inspecting the
/// before/after facts.
#[cfg(target_os = "macos")]
struct WorkerOutcome {
    before: crate::scope_inspect::ZoomFacts,
    after: crate::scope_inspect::ZoomFacts,
    press_latency_us: u64,
    cg_owner_after: Option<u32>,
}

/// The whole AX pass on the owning worker thread: join the exact window,
/// read before-facts, join the zoom button through its direct accessor,
/// repeat the cheap proof (fresh identity plus fresh CG through the shared
/// pure choke point), press exactly once, then read after-facts. Every
/// `AxElement` is constructed, used, and dropped here; nothing crosses to
/// the waiting main thread except plain facts.
#[cfg(target_os = "macos")]
#[allow(clippy::too_many_arguments)]
fn zoom_press_on_worker(
    pid: u32,
    caller: &str,
    wid: u32,
    signer: &str,
    snapshot: &SocketSnapshot,
    bound_exe: &str,
    bound_launch: &str,
    cancelled: &AtomicBool,
) -> Result<WorkerOutcome, String> {
    use crate::ffi::{
        ax_bool_attribute, ax_element_attribute, ax_error_name, ax_perform_action,
        ax_set_messaging_timeout, ax_string_attribute,
    };
    use crate::mc_probe::MC_ACTION_PRESS;
    // One shared bounded join (2 s app timeout, per-window 2 s set BEFORE
    // the id read, 2 s on the retained target): the retained `window` below
    // is the exact joined fixture window every later read uses.
    let window = join_ax_window(pid, wid)?;
    let pid_i32 = i32::try_from(pid).map_err(|_| format!("zoom worker: pid {pid} out of range"))?;
    let before = crate::scope_inspect::read_zoom_facts(pid, wid);
    let button = ax_element_attribute(&window, "AXZoomButton")
        .map_err(|status| format!("zoom worker: AXZoomButton handle join missed ({status})"))?;
    if ax_set_messaging_timeout(&button, 2.0) != 0 {
        return Err("zoom worker: messaging timeout unset on retained button; refusing".to_owned());
    }
    // Commit boundary: fresh identity plus fresh CG through the same pure
    // choke point as the pre-press proof (no socket, no subprocess). A
    // re-created window, a relaunched process, or an activated Secure Input
    // refuses here, before the window server is touched. The signature was
    // proven at bind and is never re-forked: the retained tail is compared
    // again here so the commit judges through the identical choke point.
    // When CG withholds the title but the fresh owner join already names the
    // fixture pid, one bounded AXTitle read off the retained joined window
    // fills the title source; a present CG title stays authoritative and an
    // absent CG entry still refuses.
    let fresh_cg = cg_entry_for(wid)?;
    let ax_title: Option<String> = match &fresh_cg {
        Some((owner, None)) if *owner == pid => ax_string_attribute(&window, "AXTitle")
            .ok()
            .flatten()
            .filter(|title| !title.is_empty()),
        _ => None,
    };
    let commit: Result<(), String> = (|| {
        zoom_session_gates()?;
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
    })();
    let interrupted = crate::space_drag_diag_live::diag_interrupted();
    let fenced = cancelled.load(Ordering::SeqCst);
    // The single press of this invocation: zoom-only vocabulary (`AXPress`
    // on the joined `AXZoomButton`, never fullscreen). The retained handle
    // is re-proven enabled inside the effect immediately before actuation,
    // so a handle that went stale after the commit refuses instead of
    // pressing; the effect runs exactly once with no retry.
    let press_latency_us = drive_single_press(&before, commit, interrupted, fenced, || {
        match ax_bool_attribute(&button, "AXEnabled") {
            Ok(true) => {}
            Ok(false) => {
                return Err(
                    "zoom worker: retained AXZoomButton reads disabled; refusing stale target"
                        .to_owned(),
                );
            }
            Err(code) => {
                return Err(format!(
                    "zoom worker: retained AXZoomButton liveness unreadable ({}); refusing",
                    ax_error_name(code)
                ));
            }
        }
        let press_start = Instant::now();
        ax_perform_action(&button, MC_ACTION_PRESS).map_err(|code| {
            format!(
                "zoom worker: AXPress failed ({}) with no retry",
                ax_error_name(code)
            )
        })?;
        Ok(press_start.elapsed().as_micros().min(u128::from(u64::MAX)) as u64)
    })?;
    let after = crate::scope_inspect::read_zoom_facts(pid, wid);
    let cg_owner_after = cg_entry_for(wid).ok().flatten().map(|(owner, _)| owner);
    Ok(WorkerOutcome {
        before,
        after,
        press_latency_us,
        cg_owner_after,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture_post::SocketSnapshot;

    fn passing_inputs() -> ZoomAuthInputs {
        ZoomAuthInputs {
            caller: "zoom-leg-1".to_owned(),
            wid: 7,
            announced_pid: 4242,
            snapshot: SocketSnapshot {
                pid: 4242,
                caller: "zoom-leg-1".to_owned(),
                run_tag: "run-tag-9".to_owned(),
                native_handle: "handle-opaque".to_owned(),
            },
            bundle: Some("io.github.omnitiler.live-fixture".to_owned()),
            sig_tail: Some("expected-requirement".to_owned()),
            expected_signer: "expected-requirement".to_owned(),
            live_exe: Some("bound-exe".to_owned()),
            bound_exe: "bound-exe".to_owned(),
            live_launch: Some("bound-launch".to_owned()),
            bound_launch: "bound-launch".to_owned(),
            cg_entry: Some((
                4242,
                Some("OmniTilerTestWindow:zoom-leg-1:run-tag-9".to_owned()),
            )),
            ax_title: None,
            secure_input: Some(false),
        }
    }

    fn pressing_zoom() -> crate::scope_inspect::ZoomFacts {
        crate::scope_inspect::ZoomFacts {
            attr_names_status: "ok".to_owned(),
            attr_names: Some(vec!["AXZoomed".to_owned()]),
            has_zoomed_attr: Some(true),
            zoomed: Some(false),
            zoomed_status: "ok".to_owned(),
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
        }
    }

    #[test]
    fn passing_inputs_authorize() {
        assert!(check_zoom_authorization(&passing_inputs()).is_ok());
    }

    #[test]
    fn each_refusal_vector_fails_closed_with_its_stage() {
        type Mutator = Box<dyn Fn(&mut ZoomAuthInputs)>;
        type Case<'a> = (&'a str, Mutator, &'a str);
        let cases: Vec<Case<'_>> = vec![
            (
                "caller",
                Box::new(|i| i.caller = "bad caller!".to_owned()),
                "caller id",
            ),
            ("wid", Box::new(|i| i.wid = 0), "window id 0"),
            (
                "secure-unknown",
                Box::new(|i| i.secure_input = None),
                "Secure Input",
            ),
            (
                "secure-active",
                Box::new(|i| i.secure_input = Some(true)),
                "Secure Input",
            ),
            (
                "socket-pid",
                Box::new(|i| i.snapshot.pid = 9999),
                "live server pid",
            ),
            (
                "socket-caller",
                Box::new(|i| i.snapshot.caller = "other-caller".to_owned()),
                "snapshot caller",
            ),
            (
                "bundle-foreign",
                Box::new(|i| i.bundle = Some("com.example.other".to_owned())),
                "not the signed fixture scope",
            ),
            (
                "bundle-unknown",
                Box::new(|i| i.bundle = None),
                "no bundle identity",
            ),
            (
                "signer-mismatch",
                Box::new(|i| i.sig_tail = Some("foreign-requirement".to_owned())),
                "designated requirement mismatch",
            ),
            (
                "signer-unreadable",
                Box::new(|i| i.sig_tail = None),
                "designated requirement unreadable",
            ),
            (
                "exe-drift",
                Box::new(|i| i.live_exe = Some("other-exe".to_owned())),
                "executable drift",
            ),
            (
                "exe-unreadable",
                Box::new(|i| i.live_exe = None),
                "executable path unreadable",
            ),
            (
                "launch-drift",
                Box::new(|i| i.live_launch = Some("other-launch".to_owned())),
                "launch date drift",
            ),
            (
                "launch-unreadable",
                Box::new(|i| i.live_launch = None),
                "launch date unreadable",
            ),
            (
                "cg-missing",
                Box::new(|i| i.cg_entry = None),
                "not present in CGWindowList",
            ),
            (
                "cg-owner",
                Box::new(|i| {
                    i.cg_entry = Some((
                        31337,
                        Some("OmniTilerTestWindow:zoom-leg-1:run-tag-9".to_owned()),
                    ));
                }),
                "belongs to pid",
            ),
            (
                "cg-title-missing",
                Box::new(|i| i.cg_entry = Some((4242, None))),
                "title unreadable",
            ),
            (
                "native-caller",
                Box::new(|i| {
                    i.cg_entry = Some((
                        4242,
                        Some("OmniTilerTestWindow:other-caller:run-tag-9".to_owned()),
                    ));
                }),
                "native title caller mismatch",
            ),
            (
                "run-tag",
                Box::new(|i| {
                    i.cg_entry = Some((
                        4242,
                        Some("OmniTilerTestWindow:zoom-leg-1:stale-tag".to_owned()),
                    ));
                }),
                "run tag",
            ),
        ];
        for (name, mutate, needle) in cases {
            let mut inputs = passing_inputs();
            mutate(&mut inputs);
            let err = match check_zoom_authorization(&inputs) {
                Ok(()) => panic!("{name} must refuse"),
                Err(err) => err,
            };
            assert!(
                err.contains(needle),
                "{name}: error {err:?} must name its stage ({needle})"
            );
        }
    }

    #[test]
    fn ax_title_fills_a_withheld_cg_title_but_never_bypasses_one() {
        // CG withheld + AX fallback with the live caller/tag authorizes.
        let mut fallback = passing_inputs();
        fallback.cg_entry = Some((4242, None));
        fallback.ax_title = Some("OmniTilerTestWindow:zoom-leg-1:run-tag-9".to_owned());
        assert!(check_zoom_authorization(&fallback).is_ok());
        // Neither source: still refuses as stale.
        let mut neither = passing_inputs();
        neither.cg_entry = Some((4242, None));
        neither.ax_title = None;
        let err = check_zoom_authorization(&neither).unwrap_err();
        assert!(err.contains("title unreadable"), "unexpected: {err}");
        // A present-but-wrong CG title refuses even with a good AX fallback.
        for (name, cg, ax) in [
            (
                "cg-foreign-caller",
                Some("OmniTilerTestWindow:other-caller:run-tag-9".to_owned()),
                Some("OmniTilerTestWindow:zoom-leg-1:run-tag-9".to_owned()),
            ),
            (
                "cg-stale-tag",
                Some("OmniTilerTestWindow:zoom-leg-1:stale-tag".to_owned()),
                Some("OmniTilerTestWindow:zoom-leg-1:run-tag-9".to_owned()),
            ),
            (
                "cg-malformed",
                Some("NotAFixtureWindow".to_owned()),
                Some("OmniTilerTestWindow:zoom-leg-1:run-tag-9".to_owned()),
            ),
        ] {
            let mut inputs = passing_inputs();
            inputs.cg_entry = Some((4242, cg));
            inputs.ax_title = ax;
            assert!(
                check_zoom_authorization(&inputs).is_err(),
                "{name} must refuse without AX bypass"
            );
        }
        // A wrong AX fallback refuses exactly like a wrong CG title.
        for (name, ax, needle) in [
            (
                "ax-foreign-caller",
                "OmniTilerTestWindow:other-caller:run-tag-9",
                "native title caller mismatch",
            ),
            (
                "ax-stale-tag",
                "OmniTilerTestWindow:zoom-leg-1:stale-tag",
                "run tag",
            ),
            ("ax-malformed", "NotAFixtureWindow", "sentinel"),
        ] {
            let mut inputs = passing_inputs();
            inputs.cg_entry = Some((4242, None));
            inputs.ax_title = Some(ax.to_owned());
            let err = check_zoom_authorization(&inputs).unwrap_err();
            assert!(err.contains(needle), "{name}: unexpected: {err}");
        }
    }

    #[test]
    fn button_pressable_requires_joined_and_enabled() {
        assert!(zoom_button_pressable(&pressing_zoom()).is_ok());
        let mut disabled = pressing_zoom();
        disabled.zoom_button.enabled = Some(false);
        let err = zoom_button_pressable(&disabled).unwrap_err();
        assert!(err.contains("disabled"), "unexpected: {err}");
        let mut unknown = pressing_zoom();
        unknown.zoom_button.enabled = None;
        let err = zoom_button_pressable(&unknown).unwrap_err();
        assert!(err.contains("unreadable"), "unexpected: {err}");
        let mut missing = pressing_zoom();
        missing.zoom_button.found = false;
        missing.zoom_button.lookup_status = "kAXErrorNoValue".to_owned();
        missing.zoom_button.enabled = None;
        let err = zoom_button_pressable(&missing).unwrap_err();
        assert!(err.contains("join missed"), "unexpected: {err}");
    }

    #[test]
    fn authorize_press_orders_gates_before_any_press() {
        let before = pressing_zoom();
        let ok: Result<(), String> = Ok(());
        assert!(authorize_press(&before, &ok, false, false).is_ok());
        // Each refusal vector blocks, and ordering is pinned: button state
        // beats commit, commit beats interrupt, interrupt beats cancel.
        let mut disabled = pressing_zoom();
        disabled.zoom_button.enabled = Some(false);
        let err =
            authorize_press(&disabled, &Err("commit bad".to_owned()), true, true).unwrap_err();
        assert!(err.contains("disabled"), "button must win: {err}");
        let err = authorize_press(&before, &Err("commit bad".to_owned()), true, true).unwrap_err();
        assert!(err.contains("zoom worker commit"), "commit must win: {err}");
        let err = authorize_press(&before, &ok, true, true).unwrap_err();
        assert!(err.contains("latched"), "interrupt must win: {err}");
        let err = authorize_press(&before, &ok, false, true).unwrap_err();
        assert!(err.contains("bound lapsed"), "cancel must refuse: {err}");
    }

    #[test]
    fn drive_single_press_presses_once_on_ok_and_never_on_refusal() {
        use std::cell::Cell;
        type RefusalVector<'a> = (
            &'a str,
            crate::scope_inspect::ZoomFacts,
            Result<(), String>,
            bool,
            bool,
        );
        let before = pressing_zoom();
        let ok: Result<(), String> = Ok(());
        // Authorized: the fake effect runs exactly once and its value passes
        // through (it stands in for the single AXPress call site).
        let calls = Cell::new(0u32);
        let latency = drive_single_press(&before, ok, false, false, || {
            calls.set(calls.get() + 1);
            Ok(77u64)
        })
        .expect("authorized press drives");
        assert_eq!(latency, 77);
        assert_eq!(calls.get(), 1);
        // Every refusal drives zero effects.
        let mut disabled = pressing_zoom();
        disabled.zoom_button.enabled = Some(false);
        let cases: Vec<RefusalVector<'_>> = vec![
            ("button", disabled, Ok(()), false, false),
            (
                "commit",
                pressing_zoom(),
                Err("commit bad".to_owned()),
                false,
                false,
            ),
            ("interrupt", pressing_zoom(), Ok(()), true, false),
            ("cancel", pressing_zoom(), Ok(()), false, true),
        ];
        for (name, facts, commit, interrupted, cancelled) in cases {
            let calls = Cell::new(0u32);
            let outcome = drive_single_press(&facts, commit, interrupted, cancelled, || {
                calls.set(calls.get() + 1);
                Ok(0u64)
            });
            assert!(outcome.is_err(), "{name} must refuse");
            assert_eq!(calls.get(), 0, "{name} must drive no effect");
        }
    }

    #[test]
    fn report_carries_no_secret_or_content_keys() {
        let zoom = crate::scope_inspect::ZoomFacts {
            attr_names_status: "ok".to_owned(),
            attr_names: Some(vec!["AXZoomed".to_owned()]),
            has_zoomed_attr: Some(true),
            zoomed: Some(false),
            zoomed_status: "ok".to_owned(),
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
        let report = FixtureZoomReport {
            outcome: "pressed".to_owned(),
            pid: 4242,
            wid: 7,
            caller: "zoom-leg-1".to_owned(),
            secure_input: Some(false),
            ax_trusted: Some(true),
            proof_us: 11,
            press_latency_us: 13,
            cg_owner_before: Some(4242),
            cg_owner_after: Some(4242),
            run_tag_agreement: true,
            zoom_before: zoom.clone(),
            zoom_after: zoom,
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
        ] {
            assert!(!json.contains(key), "zoom report must not carry {key}");
        }
        assert!(json.contains("\"outcome\":\"pressed\""));
        assert!(json.contains("\"synthetic\":false"));
    }
}

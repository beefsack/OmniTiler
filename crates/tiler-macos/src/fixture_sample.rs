//! Development-only read-only AX-frame sampler for the signed live fixture.
//!
//! Bounded timestamped frame samples off one exact fixture window, stamped
//! on the shared host monotonic clock ([`crate::ffi::host_now_ns`]) so a
//! concurrent action invocation's dispatch/return stamps join offline on one
//! origin. Standalone observation only: no leg admission, no controller
//! lifecycle, no product protocol or state.
//!
//! Intended use (offline join, no combined orchestrator): start this sampler
//! in one process, run exactly one `fixture-frame-write` or
//! `fixture-menu-press` invocation in another process inside the sample
//! window, then join that action report's dispatch/return stamps with these
//! per-sample brackets: first observed change, stable-frame criterion point
//! ([`settle_index`]), bounceback/reassertion. (`fixture-zoom-press` carries
//! no clock stamps and is out of scope for this join.)
//!
//! Proof shape (refuses before any sample, first failure wins): identical to
//! the frame diagnostic's pre-actuation proof (session gates, serve
//! announcement file as the only nonce source, live bundle/executable/launch
//! reads plus `codesign` `--verify` and the designated-requirement read
//! against the explicit `--signer`, nonce-gated read-only `get` over the
//! announced Unix socket, CG owner join plus sentinel-title caller/run-tag
//! join), judged through the shared
//! [`crate::fixture_zoom::check_zoom_authorization`] choke point (same proof
//! as the zoom/menu/frame diagnostics, never weakened).
//!
//! Sampling fences: one retained exact joined AX window handle (every sample
//! reads that handle, never a re-walk); a title-free wid/pid CG-owner fence
//! plus a launch-token lifetime fence BEFORE each AX read (a fence trip
//! stops the run before any further read, so a same-PID recycled owner is
//! excluded, not admitted); the CG-owner fence again AFTER each read (a
//! foreign sample is never recorded); every blocking two-attribute frame
//! read bracketed by dispatch/completion stamps on the shared host clock,
//! each miss kept null and counted; an unreadable frame stops the run with
//! the collected evidence kept; per-sample interrupt/session recheck (stops
//! with the collected samples kept, never a silent continuation). No writes,
//! no `AXPress`, no focus acquisition or changes, no menu traversal, no input
//! synthesis, no replay, no restore. Outputs are metadata-only JSON (IDs,
//! stages, frames, stamps - never titles, content, paths, nonces,
//! executables, or requirement text).

#![forbid(unsafe_code)]

use serde::Serialize;
use std::time::Duration;

use crate::fixture_frame::FramePoint;
use crate::fixture_post::{
    bundle_verdict, caller_id_valid, fixture_designated_tail, fixture_process_identity,
    parse_fixture_announcement, secure_input_verdict, socket_snapshot_for,
};

/// Hard lifetime for the whole sampler (proof plus bounded sample loop plus
/// report). The loop itself is capped far below this by [`SampleBounds`].
const SAMPLE_HARD_LIMIT: Duration = Duration::from_secs(30);

/// Default sample window: 5 s at 20 ms cadence, at most 256 records. Small
/// enough to hold in one report, long enough to cover one bounded action
/// plus its settle tail.
pub const SAMPLE_DEFAULT_DURATION_MS: u64 = 5000;
/// Default cadence between samples.
pub const SAMPLE_DEFAULT_INTERVAL_MS: u64 = 20;
/// Default record cap.
pub const SAMPLE_DEFAULT_MAX: u64 = 256;
/// Upper bounds enforced by [`normalize_sample_bounds`]: the window never
/// exceeds 5 s, the cadence never exceeds 1 s, the record count never
/// exceeds 256.
pub const SAMPLE_CAP_DURATION_MS: u64 = 5000;
/// Maximum pacing interval (a full second between reads).
pub const SAMPLE_CAP_INTERVAL_MS: u64 = 1000;
/// Maximum retained records per invocation.
pub const SAMPLE_CAP_COUNT: u64 = 256;
/// Settle criterion applied to the returned samples: consecutive frames
/// agreeing within half a point in every component count as stable.
pub const SETTLE_EPSILON_PTS: f64 = 0.5;
/// Minimum stable span for [`settle_index`]: the trailing run's
/// read-completion stamps must cover at least 200 ms, so a lone sample or
/// a brief coincidence never reports settled.
pub const SETTLE_MIN_SPAN_NS: u64 = 200_000_000;
/// CLI entry parameters for one read-only sample run: the serve announcement
/// file (socket/pid/nonce source), the selected fixture caller id, the exact
/// target window id, the sample bounds, and the expected designated
/// requirement (from `fixture-verify` output at preflight; machine-specific,
/// never hardcoded here).
pub struct FixtureFrameSampleArgs {
    pub announcement_file: String,
    pub caller: String,
    pub wid: u32,
    pub duration_ms: u64,
    pub interval_ms: u64,
    pub max_samples: u64,
    pub signer: String,
}

/// Normalized sample bounds over plain data. Pure; unit-tested.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SampleBounds {
    pub duration_ms: u64,
    pub interval_ms: u64,
    pub max_samples: usize,
}

/// Pure bounds check over plain data (no OS touch): every bound must be
/// nonzero and within its cap, so an absurd value refuses before any proof
/// or sample. Pure; unit-tested.
pub fn normalize_sample_bounds(
    duration_ms: u64,
    interval_ms: u64,
    max_samples: u64,
) -> Result<SampleBounds, String> {
    if duration_ms == 0 || duration_ms > SAMPLE_CAP_DURATION_MS {
        return Err(format!(
            "refusing fixture frame sample: --duration-ms {duration_ms} outside 1..={}ms; refusing",
            SAMPLE_CAP_DURATION_MS
        ));
    }
    if interval_ms == 0 || interval_ms > SAMPLE_CAP_INTERVAL_MS {
        return Err(format!(
            "refusing fixture frame sample: --interval-ms {interval_ms} outside 1..={}ms; refusing",
            SAMPLE_CAP_INTERVAL_MS
        ));
    }
    if max_samples == 0 || max_samples > SAMPLE_CAP_COUNT {
        return Err(format!(
            "refusing fixture frame sample: --max-samples {max_samples} outside 1..={}; refusing",
            SAMPLE_CAP_COUNT
        ));
    }
    Ok(SampleBounds {
        duration_ms,
        interval_ms,
        max_samples: max_samples as usize,
    })
}

/// One timestamped frame sample: the shared host monotonic bracket around
/// the blocking two-attribute AX frame read (position plus size are two
/// separate reads, never one atomic snapshot) plus the observed frame in
/// screen points plus the exact native or local status. `t_ns` stamps the
/// read dispatch, `t_end_ns` stamps read completion; each is `None`
/// exactly when that host-clock read missed (each miss counts in
/// `clock_misses`, never substituted). `cg_owner` is the per-sample CG
/// owner fence that stopped-owner-change runs rely on.
#[derive(Debug, Clone, Serialize)]
pub struct FrameSample {
    pub t_ns: Option<u64>,
    pub t_end_ns: Option<u64>,
    pub frame: Option<FramePoint>,
    pub status: String,
    pub cg_owner: Option<u32>,
}

/// Pure frame agreement over plain data (no OS touch): every component
/// within `epsilon_pts`. A non-finite or negative epsilon never agrees.
/// Pure; unit-tested.
pub fn frames_agree(a: &FramePoint, b: &FramePoint, epsilon_pts: f64) -> bool {
    if !(epsilon_pts.is_finite() && epsilon_pts >= 0.0) {
        return false;
    }
    (a.x - b.x).abs() <= epsilon_pts
        && (a.y - b.y).abs() <= epsilon_pts
        && (a.w - b.w).abs() <= epsilon_pts
        && (a.h - b.h).abs() <= epsilon_pts
}

/// Pure settle index over plain data (no OS touch): the index of the first
/// sample of the trailing run that is stable against the FINAL sample's
/// frame (every run member agrees with the anchor within `epsilon_pts`)
/// and whose read-completion stamps (`t_end_ns`) span at least
/// [`SETTLE_MIN_SPAN_NS`]. A failed sample (`frame: None`) or a clockless
/// sample (`t_end_ns: None`) breaks the run; a clockless or failed tail, a
/// lone sample, or a run shorter than the minimum span yields `None`, as
/// does a bad epsilon. No per-sample policy engine: this is the one
/// offline stability criterion. Pure; unit-tested.
pub fn settle_index(samples: &[FrameSample], epsilon_pts: f64) -> Option<usize> {
    if !(epsilon_pts.is_finite() && epsilon_pts >= 0.0) {
        return None;
    }
    let tail = samples.last()?;
    let anchor = tail.frame?;
    let anchor_end = tail.t_end_ns?;
    let mut start = samples.len() - 1;
    for (idx, sample) in samples.iter().enumerate().rev().skip(1) {
        match (sample.frame, sample.t_end_ns) {
            (Some(frame), Some(_)) if frames_agree(&frame, &anchor, epsilon_pts) => {
                start = idx;
            }
            _ => break,
        }
    }
    let start_end = samples[start].t_end_ns?;
    if anchor_end.saturating_sub(start_end) >= SETTLE_MIN_SPAN_NS {
        Some(start)
    } else {
        None
    }
}

/// Pure per-sample CG-owner fence verdict over plain data (no OS touch):
/// the loop calls this with a title-free wid/pid lookup BEFORE each AX
/// read (and again after it). `Ok` continues; `Err` stops the run with the
/// collected evidence kept, before any further read on a fence trip.
/// Pure; unit-tested.
pub fn check_loop_owner(read: Result<Option<u32>, String>, pid: u32) -> Result<(), String> {
    match read {
        Err(error) => Err(format!(
            "sample loop: CG owner unreadable; stopping with collected samples ({error})"
        )),
        Ok(None) => {
            Err("sample loop: wid absent from CG list; stopping with collected samples".to_owned())
        }
        Ok(Some(owner)) if owner == pid => Ok(()),
        Ok(Some(owner)) => Err(format!(
            "sample loop: owner changed mid-sample (now {owner}); stopping without recording the foreign sample"
        )),
    }
}

/// Pure process-lifetime fence over plain data (no OS touch): the live
/// launch-date token must still equal the proof-time token, excluding a
/// same-PID recycled owner that a bare PID comparison would admit. An
/// unreadable live token is uncertainty, not continuity: it stops the run.
/// Pure; unit-tested.
pub fn check_loop_lifetime(live_launch: Option<&str>, bound_launch: &str) -> Result<(), String> {
    match live_launch {
        Some(live) if live == bound_launch => Ok(()),
        Some(_) => Err(
            "sample loop: process launch token changed (possible PID reuse); stopping with collected samples"
                .to_owned(),
        ),
        None => Err(
            "sample loop: process launch token unreadable; stopping with collected samples"
                .to_owned(),
        ),
    }
}

#[derive(Debug, Clone, Serialize)]
struct FixtureSampleReport {
    outcome: String,
    stop_reason: Option<String>,
    pid: u32,
    wid: u32,
    caller: String,
    secure_input: Option<bool>,
    ax_trusted: Option<bool>,
    duration_ms: u64,
    interval_ms: u64,
    max_samples: usize,
    t_start_ns: Option<u64>,
    t_end_ns: Option<u64>,
    clock_misses: usize,
    settle_index_eps05: Option<usize>,
    samples: Vec<FrameSample>,
    run_tag_agreement: bool,
    synthetic: bool,
    note: String,
}

/// Read-only frame sampler: fresh proof, then bounded timestamped AX-frame
/// samples off the retained joined window with a per-sample CG-owner fence.
/// A mid-run fence trip stops the run and reports what was collected with
/// outcome `"stopped"` (exit 0: a stopped run is honest evidence, not a
/// tool failure). Exit codes: 2 usage/validation, 1 gate/proof failure with
/// evidence.
#[cfg(target_os = "macos")]
pub fn run_fixture_frame_sample(args: &FixtureFrameSampleArgs) -> Result<String, (i32, String)> {
    use crate::fixture_zoom::{ZoomAuthInputs, check_zoom_authorization};
    use std::time::Instant;
    let err_json = |stage: &str, error: String| -> (i32, String) {
        (
            1,
            serde_json::json!({"stage": stage, "error": error}).to_string(),
        )
    };
    let refuse = |stage: &str, error: String| -> (i32, String) {
        eprintln!("fixture-frame-sample refused at {stage}: {error}");
        err_json(stage, error)
    };
    if args.announcement_file.is_empty() {
        return Err((
            2,
            "fixture-frame-sample needs a nonempty --announcement-file".to_owned(),
        ));
    }
    if args.signer.trim().is_empty() {
        return Err((
            2,
            "fixture-frame-sample needs a nonempty --signer".to_owned(),
        ));
    }
    if !caller_id_valid(&args.caller) {
        return Err((
            2,
            "refusing fixture frame sample: caller id malformed".to_owned(),
        ));
    }
    if args.wid == 0 {
        return Err((2, "refusing fixture frame sample of window id 0".to_owned()));
    }
    let bounds = normalize_sample_bounds(args.duration_ms, args.interval_ms, args.max_samples)
        .map_err(|e| (2, e))?;
    let mut _watchdog = crate::watchdog::Watchdog::arm(SAMPLE_HARD_LIMIT).map_err(|e| (1, e))?;
    sample_session_gates().map_err(|e| refuse("session", e))?;
    let _interrupt =
        crate::space_drag_diag_live::InterruptArmed::arm().map_err(|e| refuse("interrupt", e))?;
    let raw = std::fs::read_to_string(&args.announcement_file).map_err(|e| {
        (
            2,
            format!("fixture-frame-sample announcement file unreadable ({e})"),
        )
    })?;
    let announced = parse_fixture_announcement(raw.trim())
        .map_err(|e| (2, format!("announcement refused: {e}")))?;
    let pid = announced.pid;
    if pid <= 1 {
        return Err((
            2,
            format!("refusing fixture frame sample of reserved pid {pid}"),
        ));
    }
    if pid == std::process::id() {
        return Err((
            2,
            "refusing fixture frame sample of the controller itself".to_owned(),
        ));
    }
    if i32::try_from(pid).is_err() {
        return Err((
            2,
            format!("refusing fixture frame sample: pid {pid} out of range"),
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
                "refusing fixture frame sample: live server pid {} != announced pid {pid}; refusing",
                snapshot.pid
            ),
        ));
    }
    let cg_entry = cg_entry_for(args.wid).map_err(|e| refuse("proof", e))?;
    // Same withheld-title fallback as the frame diagnostic: one bounded
    // AXTitle read off the exact joined fixture window when CG withholds
    // the title but the owner join already names the fixture pid. A present
    // CG title stays authoritative and an absent CG entry still refuses.
    let ax_title: Option<String> = match &cg_entry {
        Some((owner, None)) if *owner == pid => join_sample_window(pid, args.wid)
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
            e.replace("refusing fixture zoom", "refusing fixture frame sample"),
        )
    })?;
    if crate::space_drag_diag_live::diag_interrupted() {
        return Err(refuse(
            "interrupted",
            "interrupted: SIGINT/SIGTERM latched; refusing sample".to_owned(),
        ));
    }
    // One shared bounded join: the retained `window` below is the exact
    // joined fixture window every sample reads. Nothing crosses threads:
    // the calling thread owns every `AxElement` (construct, use, drop).
    let window = join_sample_window(pid, args.wid).map_err(|e| refuse("join", e))?;
    // PID reuse is excluded per sample by the proof-time launch token: a
    // bare PID comparison alone would admit a recycled owner.
    let pid_i32 = i32::try_from(pid)
        .map_err(|_| refuse("identity", format!("sample loop: pid {pid} out of range")))?;
    let window_duration = Duration::from_millis(bounds.duration_ms);
    let window_interval = Duration::from_millis(bounds.interval_ms);
    let loop_start = Instant::now();
    let mut samples: Vec<FrameSample> = Vec::new();
    let mut clock_misses: usize = 0;
    let mut stop_reason: Option<String> = None;
    let stamp = |misses: &mut usize| {
        let stamped = crate::ffi::host_now_ns();
        if stamped.is_none() {
            *misses += 1;
        }
        stamped
    };
    let t_start_ns = stamp(&mut clock_misses);
    loop {
        if samples.len() >= bounds.max_samples {
            break;
        }
        if loop_start.elapsed() >= window_duration {
            break;
        }
        if crate::space_drag_diag_live::diag_interrupted() {
            stop_reason = Some(
                "interrupted: SIGINT/SIGTERM latched; stopping with collected samples".to_owned(),
            );
            break;
        }
        if let Err(error) = sample_session_gates() {
            stop_reason = Some(format!(
                "session gate lost mid-sample; stopping with collected samples ({error})"
            ));
            break;
        }
        // Pre-read fences: the title-free wid/pid owner fence plus the
        // launch-token lifetime fence run BEFORE the blocking AX read, so
        // a fence trip prevents the read instead of following it.
        if let Err(reason) = check_loop_owner(cg_owner_basic(args.wid), pid) {
            stop_reason = Some(reason);
            break;
        }
        if let Err(reason) =
            check_loop_lifetime(crate::ffi::launch_date_for_pid(pid_i32).as_deref(), &launch)
        {
            stop_reason = Some(reason);
            break;
        }
        // The AX frame read is two blocking attribute reads (position plus
        // size), never one atomic snapshot: bracket dispatch and completion
        // on the shared host clock so the offline join sees the honest
        // read window.
        let t_ns = stamp(&mut clock_misses);
        let (frame, status) = read_sample_frame(&window);
        let t_end_ns = stamp(&mut clock_misses);
        // Post-read fence retained: a foreign sample is never recorded.
        if let Err(reason) = check_loop_owner(cg_owner_basic(args.wid), pid) {
            stop_reason = Some(reason);
            break;
        }
        let cg_owner = Some(pid);
        let unreadable = frame.is_none();
        samples.push(FrameSample {
            t_ns,
            t_end_ns,
            frame,
            status,
            cg_owner,
        });
        if unreadable {
            let status = samples
                .last()
                .map(|sample| sample.status.clone())
                .unwrap_or_else(|| "unknown".to_owned());
            stop_reason = Some(format!(
                "frame unreadable mid-sample ({status}); stopping with collected samples"
            ));
            break;
        }
        if samples.len() >= bounds.max_samples {
            break;
        }
        let elapsed = loop_start.elapsed();
        if elapsed >= window_duration {
            break;
        }
        std::thread::sleep(window_interval.min(window_duration.saturating_sub(elapsed)));
    }
    let t_end_ns = stamp(&mut clock_misses);
    let report = FixtureSampleReport {
        outcome: if stop_reason.is_none() {
            "sampled".to_owned()
        } else {
            "stopped".to_owned()
        },
        stop_reason,
        pid,
        wid: args.wid,
        caller: args.caller.clone(),
        secure_input: crate::ffi::secure_input_known(),
        ax_trusted: crate::ffi::ax_is_process_trusted(),
        duration_ms: bounds.duration_ms,
        interval_ms: bounds.interval_ms,
        max_samples: bounds.max_samples,
        t_start_ns,
        t_end_ns,
        clock_misses,
        settle_index_eps05: settle_index(&samples, SETTLE_EPSILON_PTS),
        samples,
        run_tag_agreement: true,
        synthetic: false,
        note: "development-only read-only frame sampler: fresh signed-fixture proof plus bounded timestamped AX-frame samples off the retained joined window with a per-sample CG-owner fence; no writes, no press, no focus changes, no input synthesis; run concurrently with exactly one action invocation and join stamps on the shared host monotonic origin offline"
            .to_owned(),
    };
    let json = serde_json::to_string(&report).map_err(|e| (1, e.to_string()))?;
    _watchdog.disarm();
    Ok(json)
}

/// Non-macOS fallback: the fixture transport does not exist here.
#[cfg(not(target_os = "macos"))]
pub fn run_fixture_frame_sample(_args: &FixtureFrameSampleArgs) -> Result<String, (i32, String)> {
    Err((1, "fixture-frame-sample is macOS-only".to_owned()))
}

/// Shared live session gates before proof and inside the sample loop (no
/// writes): SLPS symbols, AX trust, Secure Input (strict: unknown refuses),
/// unlocked session. No CG post-access gate: this route performs no CG post.
/// Same shape as the frame/menu diagnostics; duplicated (not shared) so this
/// module judges through identical gates without touching those files.
#[cfg(target_os = "macos")]
fn sample_session_gates() -> Result<(), String> {
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
            Err("session: session locked; refusing frame sample".to_owned())
        }
        crate::ffi::SessionLock::Unknown => {
            Err("session: session unknown; refusing frame sample".to_owned())
        }
    }
}

/// Join the exact `(pid, wid)` AX window on the calling thread: application
/// element, 2 s messaging timeout, scan with a per-window 2 s timeout set
/// BEFORE the id read (an unsettable window is skipped, never read
/// unbounded), then a 2 s timeout on the retained target (refuses when
/// unsettable: later reads on it must stay bounded).
#[cfg(target_os = "macos")]
fn join_sample_window(pid: u32, wid: u32) -> Result<crate::ffi::AxElement, String> {
    use crate::ffi::{AxElement, ax_set_messaging_timeout, ax_window_id};
    let pid_i32 = i32::try_from(pid).map_err(|_| format!("sample join: pid {pid} out of range"))?;
    let app = AxElement::application(pid_i32)
        .ok_or_else(|| format!("sample join: no AX element for pid {pid}"))?;
    if ax_set_messaging_timeout(&app, 2.0) != 0 {
        return Err("sample join: messaging timeout unset before window join".to_owned());
    }
    let windows = app.windows().map_err(|code| {
        format!(
            "sample join: AX window list unreadable ({})",
            crate::ffi::ax_error_name(code)
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
        return Err(format!("sample join: window wid {wid} rejoin missed"));
    };
    if ax_set_messaging_timeout(&window, 2.0) != 0 {
        return Err("sample join: messaging timeout unset on retained target; refusing".to_owned());
    }
    Ok(window)
}

/// Title-free per-sample owner fence for one wid: the owner pid from the
/// basic (wid/pid, no title leg) CG list. The proof-time full/sentinel join
/// stays at proof; the loop uses this narrower list for its pre- and
/// post-read fences. No overhead comparison is claimed here: the full-list
/// cost is unmeasured, and this choice is about fence narrowness (no title
/// acquisition, not even transiently), not speed.
#[cfg(target_os = "macos")]
fn cg_owner_basic(wid: u32) -> Result<Option<u32>, String> {
    crate::ffi::cg_window_list_basic()
        .map(|list| {
            list.into_iter()
                .find(|window| window.wid == wid)
                .map(|window| window.pid)
        })
        .map_err(|e| format!("CG snapshot refused ({e})"))
}

/// CG owner entry for one wid: `(owner pid, title)` for the pure choke
/// point to judge, or `None` when the wid is absent from the snapshot. An
/// absent wid refuses through the choke point; a withheld title is filled
/// by the bounded AXTitle fallback at the call site, never here.
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

/// Read one timestamped AX frame fact off the retained joined window. Every
/// unknown carries its exact native status; nothing is defaulted.
#[cfg(target_os = "macos")]
fn read_sample_frame(window: &crate::ffi::AxElement) -> (Option<FramePoint>, String) {
    use crate::ffi::{ax_error_name, ax_get_frame, ax_set_messaging_timeout};
    let _ = ax_set_messaging_timeout(window, 2.0);
    match ax_get_frame(window) {
        Ok(frame) => (Some(frame.into()), "ok".to_owned()),
        Err(code) => (None, ax_error_name(code)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sampled(t_end: u64, x: f64, owner: u32) -> FrameSample {
        FrameSample {
            t_ns: Some(t_end.saturating_sub(1_000_000)),
            t_end_ns: Some(t_end),
            frame: Some(FramePoint {
                x,
                y: 100.0,
                w: 800.0,
                h: 600.0,
            }),
            status: "ok".to_owned(),
            cg_owner: Some(owner),
        }
    }

    fn failed(t_end: u64, owner: u32) -> FrameSample {
        FrameSample {
            t_ns: Some(t_end.saturating_sub(1_000_000)),
            t_end_ns: Some(t_end),
            frame: None,
            status: "kAXErrorNoValue".to_owned(),
            cg_owner: Some(owner),
        }
    }

    fn clockless(x: f64, owner: u32) -> FrameSample {
        FrameSample {
            t_ns: None,
            t_end_ns: None,
            frame: Some(FramePoint {
                x,
                y: 100.0,
                w: 800.0,
                h: 600.0,
            }),
            status: "ok".to_owned(),
            cg_owner: Some(owner),
        }
    }

    #[test]
    fn sample_bounds_accept_defaults_and_refuse_absurd() {
        let bounds = normalize_sample_bounds(
            SAMPLE_DEFAULT_DURATION_MS,
            SAMPLE_DEFAULT_INTERVAL_MS,
            SAMPLE_DEFAULT_MAX,
        )
        .expect("defaults must normalize");
        assert_eq!(bounds.duration_ms, 5000);
        assert_eq!(bounds.interval_ms, 20);
        assert_eq!(bounds.max_samples, 256);
        for (name, args) in [
            ("zero-duration", (0, 20, 256)),
            ("over-duration", (5001, 20, 256)),
            ("zero-interval", (5000, 0, 256)),
            ("over-interval", (5000, 1001, 256)),
            ("zero-count", (5000, 20, 0)),
            ("over-count", (5000, 20, 257)),
        ] {
            assert!(
                normalize_sample_bounds(args.0, args.1, args.2).is_err(),
                "{name} must refuse"
            );
        }
    }

    #[test]
    fn settle_index_anchors_to_final_and_requires_span() {
        assert_eq!(settle_index(&[], 0.5), None);
        // Lone sample: zero span, never settled.
        assert_eq!(settle_index(&[sampled(1_000, 100.0, 7)], 0.5), None);
        // Ramp at 0.4/step agrees pairwise but only the anchor-agreeing
        // tail counts, and 50 ms of completion span is insufficient.
        let ramp: Vec<FrameSample> = (0..6)
            .map(|i| sampled(1_000 + i * 50_000_000, 100.0 + i as f64 * 0.4, 7))
            .collect();
        assert_eq!(settle_index(&ramp, 0.5), None);
        // Flat run spanning 300 ms settles at its first sample.
        let flat: Vec<FrameSample> = (0..4)
            .map(|i| sampled(1_000 + i * 100_000_000, 100.0, 7))
            .collect();
        assert_eq!(settle_index(&flat, 0.5), Some(0));
        // Same flat run at 50 ms spacing spans 150 ms: insufficient.
        let short: Vec<FrameSample> = (0..4)
            .map(|i| sampled(1_000 + i * 50_000_000, 100.0, 7))
            .collect();
        assert_eq!(settle_index(&short, 0.5), None);
        // Exactly 200 ms of span settles.
        let exact: Vec<FrameSample> = (0..3)
            .map(|i| sampled(1_000 + i * 100_000_000, 100.0, 7))
            .collect();
        assert_eq!(settle_index(&exact, 0.5), Some(0));
        // Failed tail breaks the run: no trailing stable sample.
        let run = vec![sampled(1_000, 100.0, 7), failed(2_000, 7)];
        assert_eq!(settle_index(&run, 0.5), None);
        // Clockless tail cannot prove completion span.
        let run = vec![sampled(1_000, 100.0, 7), clockless(100.0, 7)];
        assert_eq!(settle_index(&run, 0.5), None);
        // Clockless mid-run sample breaks the run even when the surviving
        // tail would otherwise span enough.
        let run = vec![
            sampled(1_000, 100.0, 7),
            clockless(100.0, 7),
            sampled(1_000_000_000, 100.0, 7),
            sampled(1_300_000_000, 100.0, 7),
        ];
        assert_eq!(settle_index(&run, 0.5), Some(2));
        // A bad epsilon never settles.
        assert_eq!(settle_index(&[sampled(1_000, 100.0, 7)], f64::NAN), None);
        assert_eq!(settle_index(&[sampled(1_000, 100.0, 7)], -1.0), None);
    }

    #[test]
    fn loop_owner_fence_admits_only_the_bound_pid() {
        assert!(check_loop_owner(Ok(Some(4242)), 4242).is_ok());
        // Owner change, absent wid, and unreadable list all stop.
        for (name, read) in [
            ("changed", Ok(Some(99))),
            ("absent", Ok(None)),
            (
                "unreadable",
                Err("CG snapshot refused (boom)".to_owned()) as Result<Option<u32>, String>,
            ),
        ] {
            assert!(check_loop_owner(read, 4242).is_err(), "{name} must stop");
        }
    }

    #[test]
    fn loop_lifetime_fence_excludes_recycled_pids() {
        assert!(check_loop_lifetime(Some("token-a"), "token-a").is_ok());
        assert!(check_loop_lifetime(Some("token-b"), "token-a").is_err());
        assert!(check_loop_lifetime(None, "token-a").is_err());
    }

    #[test]
    fn frames_agree_is_a_per_component_epsilon() {
        let base = FramePoint {
            x: 100.0,
            y: 100.0,
            w: 800.0,
            h: 600.0,
        };
        let near = FramePoint { x: 100.5, ..base };
        let far = FramePoint { x: 100.6, ..base };
        assert!(frames_agree(&base, &near, 0.5));
        assert!(!frames_agree(&base, &far, 0.5));
        assert!(!frames_agree(&base, &base, f64::NAN));
    }

    #[test]
    fn sample_report_carries_no_secrets() {
        let report = FixtureSampleReport {
            outcome: "sampled".to_owned(),
            stop_reason: None,
            pid: 4242,
            wid: 7,
            caller: "sample-leg-1".to_owned(),
            secure_input: Some(false),
            ax_trusted: Some(true),
            duration_ms: 5000,
            interval_ms: 20,
            max_samples: 256,
            t_start_ns: Some(1_700_000_000_000_000_000),
            t_end_ns: Some(1_700_000_005_000_000_000),
            clock_misses: 0,
            settle_index_eps05: None,
            samples: vec![sampled(1_700_000_000_000_000_000, 100.0, 4242)],
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
            assert!(!json.contains(key), "sample report must not carry {key}");
        }
        assert!(json.contains("\"outcome\":\"sampled\""));
        assert!(json.contains("\"synthetic\":false"));
        assert!(json.contains("\"t_end_ns\":1700000000000000000"));
    }
}

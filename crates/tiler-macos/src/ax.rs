#![forbid(unsafe_code)]

//! Accessibility probes over the typed [`crate::ffi`] surface.
//!
//! Threading: main/caller performs AppKit discovery (`running_apps` pid
//! snapshot) plus ownership gates only. Every `AxElement` is constructed,
//! used, and dropped on a dedicated per-app AX worker; workers return only
//! owned plain data over channels and the caller aggregates under one global
//! finite deadline with fair polling, so hung apps share one slice and never
//! stall the remaining apps.
//!
//! - `enumerate_counts`: real per-application `AXWindows` queries on per-app
//!   workers; counts only, never titles or content. Per-app AX failures are
//!   counted and reported with the process-trust (consent) state — never
//!   fabricated as zero, never hidden.
//! - `observe_app`: real `AXObserver` creation + notification registration on
//!   a per-app thread with a finite messaging timeout and bounded runloop.
//!   A hung app stalls at most one slice past its timeout; bad PIDs and
//!   missing consent fail fast.
//! - Frame set / raise: ONLY the owned probe process on one dedicated worker:
//!   caller claim, then live bundle lookup, then exact window join, then
//!   preimage capture, set, readback, and restoration. Anything else fails
//!   closed.

use serde::{Deserialize, Serialize};

use crate::ownership::OwnedIdentity;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AxCounts {
    pub apps: usize,
    pub windows: usize,
    pub ax_failures: u64,
    /// `None` off-macOS (unstubbed platform reports no trust state).
    pub process_trusted: Option<bool>,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AxError {
    Unsupported(String),
    Ownership(String),
    Timeout { pid: u32, timeout_ms: u64 },
    System(String),
    FrameTx(Box<FrameTxFailure>),
}

impl std::fmt::Display for AxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsupported(r) => write!(f, "unsupported: {r}"),
            Self::Ownership(r) => write!(f, "ownership refused: {r}"),
            Self::Timeout { pid, timeout_ms } => {
                write!(f, "AX timeout: pid {pid} exceeded {timeout_ms}ms")
            }
            Self::System(r) => write!(f, "AX system error: {r}"),
            Self::FrameTx(failure) => write!(
                f,
                "AX frame transaction failed at {}: {} (restore: {}) | evidence {}",
                failure.stage,
                failure.error,
                failure
                    .restore_refused
                    .as_deref()
                    .or(failure.restore_error.as_deref())
                    .unwrap_or(if failure.restored {
                        "verified"
                    } else {
                        "unverified"
                    }),
                serde_json::to_string(&**failure).unwrap_or_default(),
            ),
        }
    }
}

impl std::error::Error for AxError {}

/// Validate an owned frame mutation before any AX call.
pub fn gate_owned_frame(
    owned: &OwnedIdentity,
    target_pid: u32,
    target_bundle: &str,
) -> Result<(), AxError> {
    owned
        .check_action(target_pid, target_bundle)
        .map_err(AxError::Ownership)
}

/// Authenticate an owned session for `target_pid` + caller nonce against
/// the persisted spawn record (pid/nonce, expected helper, live
/// bundle-or-exe plus the CURRENT live window nonce). The bound identity
/// carries the LIVE bundle (probe or dedicated helper — see
/// [`crate::ownership::is_owned_bundle`]), never a normalized one: a
/// bundled-helper session bound to the probe bundle would fail every
/// downstream effect gate. Every mutating or
/// observing AX path enters here; arbitrary/foreign PIDs fail closed with no
/// AX call made.
pub fn authorize_owned_action(target_pid: u32, nonce: &str) -> Result<OwnedIdentity, AxError> {
    let record =
        crate::probe_target::load_verified_record(target_pid, nonce).map_err(AxError::Ownership)?;
    // Fresh live bundle (plus exe revalidation) AFTER the record proof: the
    // record only proves pid/nonce/exe/title currency, while the identity
    // must name the exact live bundle for the effect gates below.
    let live_bundle = crate::ownership::verify_live_process(target_pid, Some(&record.helper_path))
        .map_err(AxError::Ownership)?;
    OwnedIdentity::new(record.pid, &live_bundle, &record.nonce).map_err(AxError::Ownership)
}

/// Resolve the live bundle ID for a PID and require the owned one. Caller
/// claims are not proof; the running-application table is. Directly spawned
/// helpers have no LaunchServices bundle entry: when no bundle is live the
/// spawn record's executable-path plus liveness check (done in
/// `authorize_owned_action` before this call) plus the fresh CG owner-PID
/// join at use carry ownership, so a missing bundle with a live PID defers
/// to the record instead of failing.
///
/// `pub(crate)` for holder-grade sessions (`ax_session`), which bind the
/// same live bundle to the exact held `Child` plus fresh CG joins instead of
/// the title-proof record path used here.
pub(crate) fn live_owned_bundle(target_pid: u32) -> Result<String, AxError> {
    let pid_i32: i32 = target_pid
        .try_into()
        .map_err(|_| AxError::Ownership(format!("pid {target_pid} out of range")))?;
    match crate::ffi::bundle_id_for_pid(pid_i32).filter(|b| !b.is_empty()) {
        Some(bundle) => Ok(bundle),
        None if crate::ffi::process_exists(pid_i32) => Ok(crate::BUNDLE_ID.to_owned()),
        None => Err(AxError::Ownership(format!(
            "no live bundle for pid {target_pid} (not running or no bundle)"
        ))),
    }
}

/// Read-only counts. No titles, no content, no PII.
pub fn enumerate_counts(max_windows: usize) -> Result<AxCounts, AxError> {
    if max_windows == 0 || max_windows > crate::config::MAX_WINDOWS {
        return Err(AxError::Unsupported(format!(
            "max_windows out of bound 1..={}",
            crate::config::MAX_WINDOWS
        )));
    }
    #[cfg(target_os = "macos")]
    {
        macos_counts(max_windows)
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err(AxError::Unsupported(
            "AX enumeration is macOS-only".to_owned(),
        ))
    }
}

/// One per-app result collected from an AX worker. Plain data only; every
/// AX handle lives and dies on the worker. Shared by the live collector and
/// the scripted regression below so the bound is tested without live apps.
#[derive(Debug)]
#[cfg(any(test, target_os = "macos"))]
struct EnumCount {
    windows: usize,
    failed: bool,
}

/// Discovery/worker cap: one thread per app is unbounded against large app
/// tables, so only the first `MAX_ENUM_WORKERS` PIDs (sorted for
/// determinism) get a worker; the remainder is reported via `truncated`.
/// No threadpool framework: plain `std::thread` spawns only.
#[cfg(target_os = "macos")]
const MAX_ENUM_WORKERS: usize = 64;

/// Single shared collection slice for the whole aggregate (not per app).
#[cfg(target_os = "macos")]
const ENUM_AGGREGATE_SECS: u64 = 3;

/// Aggregate per-app worker outcomes under ONE global finite deadline.
/// Ready workers are collected fairly regardless of order, so one hung
/// worker cannot starve the workers behind it; whatever is still pending at
/// the deadline is reported as failures plus `truncated`. Workers run in
/// parallel and self-terminate on their own finite messaging timeout; the
/// receiver ends are simply dropped here. Pure over channels: scripted
/// receivers exercise this with no apps, no AX, and no live state.
#[cfg(any(test, target_os = "macos"))]
fn aggregate_counts(
    receivers: Vec<std::sync::mpsc::Receiver<EnumCount>>,
    max_windows: usize,
    deadline: std::time::Instant,
) -> (usize, u64, bool) {
    let mut pending = receivers;
    let mut windows = 0usize;
    let mut failures: u64 = 0;
    let mut truncated = false;
    while !pending.is_empty() {
        if std::time::Instant::now() >= deadline {
            failures += pending.len() as u64;
            truncated = true;
            break;
        }
        let mut progressed = false;
        let mut i = 0;
        while i < pending.len() {
            match pending[i].try_recv() {
                Ok(r) => {
                    if r.failed {
                        failures += 1;
                    } else if windows >= max_windows {
                        truncated = true;
                    } else {
                        let room = max_windows - windows;
                        windows += r.windows.min(room);
                        if r.windows > room {
                            truncated = true;
                        }
                    }
                    pending.remove(i);
                    progressed = true;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => i += 1,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    failures += 1;
                    pending.remove(i);
                    progressed = true;
                }
            }
        }
        if !progressed {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
    (windows, failures, truncated)
}

#[cfg(target_os = "macos")]
fn macos_counts(max_windows: usize) -> Result<AxCounts, AxError> {
    // Main-thread AppKit discovery first: snapshot the pid list, then hand
    // each pid to a dedicated per-app AX worker. No `AxElement` is
    // constructed, used, or dropped on this (caller/main) thread.
    let trusted = crate::ffi::ax_is_process_trusted();
    let mut pids: Vec<i32> = Vec::new();
    for (pid, _) in crate::ffi::running_apps() {
        if pid <= 1 {
            continue;
        }
        pids.push(pid);
    }
    pids.sort_unstable();
    let apps = pids.len();
    let truncated_discovery = apps > MAX_ENUM_WORKERS;

    let mut receivers: Vec<std::sync::mpsc::Receiver<EnumCount>> =
        Vec::with_capacity(apps.min(MAX_ENUM_WORKERS));
    let mut spawn_failures: u64 = 0;
    for pid in pids.into_iter().take(MAX_ENUM_WORKERS) {
        let (tx, rx) = std::sync::mpsc::channel::<EnumCount>();
        let spawned = std::thread::Builder::new()
            .name(format!("ax-enum-{pid}"))
            .spawn(move || {
                // Every AX handle below lives and dies on this app worker.
                let outcome: Option<usize> = (|| {
                    let element = crate::ffi::AxElement::application(pid)?;
                    // Finite messaging timeout so one hung app cannot stall
                    // its own worker indefinitely.
                    let _ = crate::ffi::ax_set_messaging_timeout(&element, 1.0);
                    match element.windows() {
                        Ok(list) => Some(list.len()),
                        Err(_) => None,
                    }
                })();
                let result = match outcome {
                    Some(n) => EnumCount {
                        windows: n,
                        failed: false,
                    },
                    None => EnumCount {
                        windows: 0,
                        failed: true,
                    },
                };
                let _ = tx.send(result);
            });
        match spawned {
            Ok(_) => receivers.push(rx),
            Err(_) => spawn_failures += 1,
        }
    }

    // Bounded aggregate: all workers share one global slice. A hung app
    // costs the shared budget only and never stalls the remaining apps.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(ENUM_AGGREGATE_SECS);
    let (windows, mut failures, truncated_collect) =
        aggregate_counts(receivers, max_windows, deadline);
    failures += spawn_failures;
    Ok(AxCounts {
        apps,
        windows,
        ax_failures: failures,
        process_trusted: trusted,
        truncated: truncated_discovery || truncated_collect,
    })
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AxObserveReport {
    pub pid: u32,
    pub timeout_ms: u64,
    pub notifications_added: u32,
    pub notifications_received: u32,
    pub messaging_timeout_set: bool,
}

/// Bounded per-app observation of the OWNED session only. The caller nonce
/// must authenticate against the persisted spawn record; arbitrary,
/// protected, or foreign PIDs fail closed before any AX call.
pub fn observe_app(
    owned: &OwnedIdentity,
    pid: u32,
    timeout_ms: u64,
    tx: &std::sync::mpsc::SyncSender<crate::engine::EngineMessage>,
) -> Result<AxObserveReport, AxError> {
    if pid <= 1 {
        return Err(AxError::Ownership(format!("refusing reserved pid {pid}")));
    }
    // Owned-session gate first: the observed PID must be the authorized one
    // and its live bundle must be the owned probe (protected bundles and
    // foreign processes fail here, before any observer is created).
    let live_bundle = live_owned_bundle(pid)?;
    gate_owned_frame(owned, pid, &live_bundle)?;
    #[cfg(target_os = "macos")]
    let pid_i32: i32 = pid
        .try_into()
        .map_err(|_| AxError::Ownership(format!("pid {pid} out of range")))?;
    if timeout_ms == 0 || timeout_ms > crate::config::MAX_AX_TIMEOUT_MS {
        return Err(AxError::Unsupported(format!(
            "timeout_ms out of bound 1..={}",
            crate::config::MAX_AX_TIMEOUT_MS
        )));
    }
    // Entry record snapshot for the worker's registration-boundary
    // revalidation (same spawn exe/nonce plus live window nonce as command entry).
    let entry =
        crate::probe_target::load_verified_record(pid, &owned.nonce).map_err(AxError::Ownership)?;
    #[cfg(target_os = "macos")]
    {
        let tx = tx.clone();
        let (done_tx, done_rx) = std::sync::mpsc::channel::<Result<AxObserveReport, AxError>>();
        std::thread::Builder::new()
            .name(format!("ax-observe-{pid}"))
            .spawn(move || {
                // Revalidate before creating the observer and registering
                // notifications (no bare entry claims reach AX).
                if let Err(e) = revalidate_entry(&entry) {
                    let _ = done_tx.send(Err(e));
                    return;
                }
                let outcome = crate::ffi::ax_observe_bounded(
                    pid_i32,
                    std::time::Duration::from_millis(timeout_ms),
                    (timeout_ms as f32 / 1000.0).clamp(0.1, 5.0),
                    &tx,
                );
                let report = outcome.map(|o| AxObserveReport {
                    pid,
                    timeout_ms,
                    notifications_added: o.notifications_added,
                    notifications_received: o.notifications_received,
                    messaging_timeout_set: o.messaging_timeout_set,
                });
                let _ = done_tx.send(report.map_err(|e| match e {
                    crate::ffi::AxObserveError::InvalidPid(r) => AxError::Ownership(r),
                    crate::ffi::AxObserveError::Unsupported(r) => AxError::Unsupported(r),
                    crate::ffi::AxObserveError::System(r) => AxError::System(r),
                }));
            })
            .map_err(|e| AxError::System(format!("spawn observer thread: {e}")))?;
        // Bounded wait: observer thread always terminates (finite runloop),
        // with one grace slice of slack before reporting a timeout.
        match done_rx.recv_timeout(std::time::Duration::from_millis(
            timeout_ms.saturating_add(2000),
        )) {
            Ok(report) => report,
            Err(_) => Err(AxError::Timeout { pid, timeout_ms }),
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (&entry, tx);
        Err(AxError::Unsupported(
            "AX observation is macOS-only".to_owned(),
        ))
    }
}

/// Frame change with preimage, observed applied frame, restore readback proof.
///
/// `applied` is the readback observed after the set (position+size legs);
/// `restore_readback` is the readback observed after the restoration write.
/// Both are `Option` because a failed read is evidence too — never guessed.
/// `restored` is true ONLY on exact `restore_readback == Some(preimage)`;
/// an accepted OS status alone is never treated as restored.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FrameChangeReport {
    pub pid: u32,
    pub wid: u32,
    pub preimage: crate::ffi::AxFrame,
    pub attempted: crate::ffi::AxFrame,
    pub applied: Option<crate::ffi::AxFrame>,
    pub restore_readback: Option<crate::ffi::AxFrame>,
    pub restored: bool,
}

/// Structured evidence for a failed frame transaction. Every failure path
/// after the first possible write carries the exact preimage, the attempted
/// frame, whatever was observed (`applied`, `restore_readback`), and the
/// failure/restore reasons — so error outputs never lose the preimage.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FrameTxFailure {
    pub pid: u32,
    pub wid: u32,
    pub preimage: crate::ffi::AxFrame,
    pub attempted: crate::ffi::AxFrame,
    pub applied: Option<crate::ffi::AxFrame>,
    pub restore_readback: Option<crate::ffi::AxFrame>,
    pub restored: bool,
    /// `set` | `readback` | `restore-write` | `restore-verify` | `ownership`.
    pub stage: String,
    pub error: String,
    pub restore_error: Option<String>,
    pub restore_refused: Option<String>,
}

/// Minimal injectable surface for the frame transaction: one window's
/// get/set plus the write-boundary ownership revalidation. The live
/// implementation wraps an `AxElement` on the dedicated AX worker; tests
/// inject scripted fakes. No native handles cross threads either way.
pub trait FrameTxOps {
    fn set_frame(&mut self, frame: &crate::ffi::AxFrame) -> Result<(), i32>;
    fn get_frame(&mut self) -> Result<crate::ffi::AxFrame, i32>;
    fn revalidate(&mut self) -> Result<(), String>;
}

/// Pure frame-mutation transaction over [`FrameTxOps`].
///
/// Contract: every path after the first possible write attempts restoring
/// the exact preimage when freshly verified ownership permits it, then
/// captures the restoration readback. Ownership loss refuses all further
/// writes but retains the structured preimage/attempted evidence. Success
/// (`Ok`) requires the set accepted AND the restore write accepted AND the
/// restore readback exactly equal to the preimage; anything else is
/// `Err(AxError::FrameTx)` with the full evidence. Pre-write ownership
/// refusal needs no restoration (nothing was written) and reports
/// `stage: ownership`.
pub fn run_frame_transaction<O: FrameTxOps>(
    ops: &mut O,
    pid: u32,
    wid: u32,
    preimage: crate::ffi::AxFrame,
    attempted: crate::ffi::AxFrame,
) -> Result<FrameChangeReport, AxError> {
    use crate::ffi::ax_error_name;
    // Pre-write gate: nothing has been written yet, so refusal needs no
    // restoration — but the evidence (preimage + attempted) is retained.
    if let Err(e) = ops.revalidate() {
        return Err(AxError::FrameTx(Box::new(FrameTxFailure {
            pid,
            wid,
            preimage,
            attempted,
            applied: None,
            restore_readback: None,
            restored: false,
            stage: "ownership".to_owned(),
            error: format!("pre-write ownership refused: {e}"),
            restore_error: None,
            restore_refused: Some(format!("restore not attempted (nothing written): {e}")),
        })));
    }
    // The write itself. `ax_set_frame` is two legs (position, then size): an
    // `Err` here may still leave the window partially mutated, so every
    // `Err` path below restores rather than returning directly.
    let set_error: Option<String> = match ops.set_frame(&attempted) {
        Ok(()) => None,
        Err(code) => Some(format!("set: {}", ax_error_name(code))),
    };
    // Post-write gate: a stale record refuses the restore write (and any
    // further touch of a possibly-reused window), with the set outcome
    // retained as the primary error.
    if let Err(e) = ops.revalidate() {
        return Err(AxError::FrameTx(Box::new(FrameTxFailure {
            pid,
            wid,
            preimage,
            attempted,
            applied: None,
            restore_readback: None,
            restored: false,
            stage: "ownership".to_owned(),
            error: format!(
                "{}(restore REFUSED on stale record: {e})",
                set_error.unwrap_or_else(|| "set dispatched".to_owned()),
            ),
            restore_error: None,
            restore_refused: Some(format!("stale record: {e}")),
        })));
    }
    // Observed applied frame (read-only evidence, never assumed).
    let mut readback_error: Option<String> = None;
    let applied: Option<crate::ffi::AxFrame> = match ops.get_frame() {
        Ok(frame) => Some(frame),
        Err(code) => {
            readback_error = Some(format!("readback: {}", ax_error_name(code)));
            None
        }
    };
    // Pre-restore gate: revalidate immediately before the cleanup write.
    if let Err(e) = ops.revalidate() {
        return Err(AxError::FrameTx(Box::new(FrameTxFailure {
            pid,
            wid,
            preimage,
            attempted,
            applied,
            restore_readback: None,
            restored: false,
            stage: "ownership".to_owned(),
            error: format!(
                "{}(restore REFUSED on stale record: {e})",
                set_error
                    .or(readback_error)
                    .unwrap_or_else(|| "applied observed".to_owned()),
            ),
            restore_error: None,
            restore_refused: Some(format!("stale record: {e}")),
        })));
    }
    let restore_error: Option<String> = match ops.set_frame(&preimage) {
        Ok(()) => None,
        // Neutral write status only: the write returned an error. Whether
        // the window is actually left modified is decided ONLY by the restore
        // readback below (`restored` plus `restore_readback`) — never by this
        // string. (A refused leg can coincide with an already-correct end
        // state, e.g. restoring size on a non-resizable window whose size
        // never moved.)
        Err(code) => Some(format!("restore write failed: {}", ax_error_name(code))),
    };
    let restore_readback: Option<crate::ffi::AxFrame> = match ops.get_frame() {
        Ok(frame) => Some(frame),
        Err(code) => {
            let verify = format!("restore verify: {}", ax_error_name(code));
            let error = set_error
                .or(readback_error)
                .or(restore_error.clone())
                .unwrap_or_else(|| "restore attempted".to_owned());
            return Err(AxError::FrameTx(Box::new(FrameTxFailure {
                pid,
                wid,
                preimage,
                attempted,
                applied,
                restore_readback: None,
                restored: false,
                stage: "restore-verify".to_owned(),
                error: format!("{error} ({verify})"),
                restore_error,
                restore_refused: None,
            })));
        }
    };
    let restored = restore_readback == Some(preimage);
    if let Some(error) = set_error {
        return Err(AxError::FrameTx(Box::new(FrameTxFailure {
            pid,
            wid,
            preimage,
            attempted,
            applied,
            restore_readback,
            restored,
            stage: "set".to_owned(),
            error,
            restore_error,
            restore_refused: None,
        })));
    }
    if let Some(error) = readback_error {
        return Err(AxError::FrameTx(Box::new(FrameTxFailure {
            pid,
            wid,
            preimage,
            attempted,
            applied,
            restore_readback,
            restored,
            stage: "readback".to_owned(),
            error,
            restore_error,
            restore_refused: None,
        })));
    }
    if let Some(error) = restore_error {
        return Err(AxError::FrameTx(Box::new(FrameTxFailure {
            pid,
            wid,
            preimage,
            attempted,
            applied,
            restore_readback,
            restored,
            stage: "restore-write".to_owned(),
            error: error.clone(),
            restore_error: Some(error),
            restore_refused: None,
        })));
    }
    if !restored {
        return Err(AxError::FrameTx(Box::new(FrameTxFailure {
            pid,
            wid,
            preimage,
            attempted,
            applied,
            restore_readback,
            restored: false,
            stage: "restore-verify".to_owned(),
            error: "RESTORE MISMATCH (window left modified): restore readback != preimage"
                .to_owned(),
            restore_error: None,
            restore_refused: None,
        })));
    }
    Ok(FrameChangeReport {
        pid,
        wid,
        preimage,
        attempted,
        applied,
        restore_readback,
        restored: true,
    })
}

/// Revalidate the entry spawn-record snapshot inside the AX worker:
/// same spawn exe/nonce plus live window nonce immediately before AX mutations and before
/// cleanup/restore writes. Metadata checks only (record file plus live
/// process identity); every `AxElement` is still constructed, used and
/// dropped on this worker thread (thread affinity kept). Narrows the
/// check-to-use window; does not claim races impossible.
#[cfg(target_os = "macos")]
fn revalidate_entry(entry: &crate::probe_target::SpawnRecord) -> Result<(), AxError> {
    crate::probe_target::verify_record_unchanged(entry, entry.pid, &entry.nonce)
        .map(|_| ())
        .map_err(AxError::Ownership)
}

/// Revalidate the entry snapshot inside the AX worker WITHOUT the live
/// window-nonce title proof: record-file equality plus live executable
/// binding plus a fresh CG owner-PID/window join. Used ONLY by the
/// held-Child fullscreen-exit path, where the window server withholds
/// titles while the helper is fullscreen. The caller-thread held-Child
/// proof (unreaped exact handle for the same PID, checked before the
/// worker spawns and again after it joins) is what excludes PID reuse
/// across the write; this worker-side check excludes restarts and window
/// reuse at each boundary. Thread affinity as above.
#[cfg(target_os = "macos")]
fn revalidate_entry_held(
    entry: &crate::probe_target::SpawnRecord,
    pid: u32,
    wid: u32,
) -> Result<(), AxError> {
    crate::probe_target::verify_record_file_unchanged(entry, entry.pid, &entry.nonce)
        .map(|_| ())
        .map_err(AxError::Ownership)?;
    crate::ownership::verify_live_process(pid, Some(&entry.helper_path))
        .map(|_| ())
        .map_err(AxError::Ownership)?;
    crate::cg::verify_wid_owned_by_pid(pid, wid)
        .map(|_| ())
        .map_err(|e| AxError::Ownership(e.to_string()))
}

/// Live [`FrameTxOps`] over one AX window element. Constructed, used, and
/// dropped on the dedicated per-app AX worker only; the entry snapshot is
/// owned plain data revalidated at every write boundary. The AX element
/// handle is preserved, but every [`FrameTxOps::revalidate`] additionally
/// re-joins the WID to the owned PID through a fresh CGWindowList snapshot:
/// the handle alone cannot prove the WID still belongs to this PID, and a
/// WID reuse inside the still-running helper must never authorize a write to
/// a foreign window.
#[cfg(target_os = "macos")]
struct LiveFrameOps<'w> {
    window: &'w crate::ffi::AxElement,
    entry: crate::probe_target::SpawnRecord,
    pid: u32,
    wid: u32,
}

#[cfg(target_os = "macos")]
impl FrameTxOps for LiveFrameOps<'_> {
    fn set_frame(&mut self, frame: &crate::ffi::AxFrame) -> Result<(), i32> {
        crate::ffi::ax_set_frame(self.window, frame)
    }

    fn get_frame(&mut self) -> Result<crate::ffi::AxFrame, i32> {
        crate::ffi::ax_get_frame(self.window)
    }

    fn revalidate(&mut self) -> Result<(), String> {
        revalidate_entry(&self.entry)
            .map(|_| ())
            .map_err(|e| e.to_string())?;
        crate::cg::verify_wid_owned_by_pid(self.pid, self.wid)
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}

/// App-thread body for [`set_frame_owned`]. Runs ONLY on the dedicated
/// per-app AX worker: constructs, uses, and drops every `AxElement` here.
/// Never called on the caller/main thread.
///
/// Pre-write failures (unknown window, unreadable preimage) return before
/// any write with no restoration to fake. Every path after the first
/// possible write runs through [`run_frame_transaction`], which restores the
/// exact preimage whenever freshly verified ownership permits and captures
/// the restoration readback; ownership loss refuses writes with the
/// structured preimage retained.
#[cfg(target_os = "macos")]
fn set_frame_on_worker(
    owned: &OwnedIdentity,
    entry: &crate::probe_target::SpawnRecord,
    target_pid: u32,
    wid: u32,
    frame: crate::ffi::AxFrame,
) -> Result<FrameChangeReport, AxError> {
    use crate::ffi::{AxElement, ax_get_frame, ax_window_id};
    // Re-check the live bundle inside the worker from owned plain data
    // (no handles cross threads); the caller already gated before spawn.
    let live_bundle = live_owned_bundle(target_pid)?;
    gate_owned_frame(owned, target_pid, &live_bundle)?;
    // Same spawn exe/nonce plus live window nonce as command entry, before any mutation.
    revalidate_entry(entry)?;
    let pid_i32 = target_pid as i32;
    let app = AxElement::application(pid_i32)
        .ok_or_else(|| AxError::System(format!("no AX element for pid {target_pid}")))?;
    let _ = crate::ffi::ax_set_messaging_timeout(&app, 2.0);
    let windows = app
        .windows()
        .map_err(|code| AxError::System(format!("windows: {}", crate::ffi::ax_error_name(code))))?;
    let window = windows
        .iter()
        .find(|w| ax_window_id(w) == Some(wid))
        .ok_or_else(|| {
            AxError::Ownership(format!(
                "wid {wid} not joined to owned pid {target_pid} (identity join failed)"
            ))
        })?;
    // Finite messaging timeout on the window element itself, not only the
    // app: every frame leg below (preimage get, attempted set, applied get,
    // restore set, restore readback) is issued against this window, so its
    // own timeout bounds each call.
    let _ = crate::ffi::ax_set_messaging_timeout(window, 2.0);
    let preimage = ax_get_frame(window).map_err(|code| {
        AxError::System(format!("preimage: {}", crate::ffi::ax_error_name(code)))
    })?;
    revalidate_entry(entry)?;
    let mut ops = LiveFrameOps {
        window,
        entry: entry.clone(),
        pid: target_pid,
        wid,
    };
    run_frame_transaction(&mut ops, target_pid, wid, preimage, frame)
}

/// Set an owned window's frame with preimage capture, readback, and
/// restoration. Every step is verified; any failure aborts with the exact
/// step named and no success claimed. Threading: gates on the caller, then
/// one dedicated AX worker owns every handle with a bounded wait.
///
/// Caller bound ([`SET_FRAME_OUTER_SECS`]): the wait always covers the
/// worker's documented worst case, so it can never fire while a mutation or
/// restoration is still within its finite budget. The worker is never
/// cancelled — it always runs the transaction to restore completion — and a
/// timeout return means "outcome unknown, a restore may still be in flight":
/// never clean, never complete.
pub fn set_frame_owned(
    owned: &OwnedIdentity,
    target_pid: u32,
    wid: u32,
    frame: crate::ffi::AxFrame,
) -> Result<FrameChangeReport, AxError> {
    if !frame.valid_for_set() {
        return Err(AxError::Unsupported("degenerate frame".to_owned()));
    }
    let live_bundle = live_owned_bundle(target_pid)?;
    gate_owned_frame(owned, target_pid, &live_bundle)?;
    // Entry record snapshot for the worker's write-boundary revalidation.
    let entry = crate::probe_target::load_verified_record(target_pid, &owned.nonce)
        .map_err(AxError::Ownership)?;
    #[cfg(target_os = "macos")]
    {
        // Dedicated app thread: every `AxElement` is constructed, used, and
        // dropped inside the worker. The caller sends only owned plain data
        // (pid/wid/frame plus the entry record snapshot) and waits bounded;
        // a hung app costs the timeout.
        let owned_send = owned.clone();
        let frame_send = frame;
        let (done_tx, done_rx) = std::sync::mpsc::channel::<Result<FrameChangeReport, AxError>>();
        std::thread::Builder::new()
            .name(format!("ax-set-frame-{target_pid}"))
            .spawn(move || {
                let outcome = set_frame_on_worker(&owned_send, &entry, target_pid, wid, frame_send);
                let _ = done_tx.send(outcome);
            })
            .map_err(|e| AxError::System(format!("spawn AX worker thread: {e}")))?;
        match done_rx.recv_timeout(std::time::Duration::from_secs(SET_FRAME_OUTER_SECS)) {
            Ok(report) => report,
            Err(_) => Err(AxError::Timeout {
                pid: target_pid,
                timeout_ms: SET_FRAME_OUTER_SECS * 1000,
            }),
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (&entry, wid, frame);
        Err(AxError::Unsupported(
            "AX frame set is macOS-only".to_owned(),
        ))
    }
}

/// Persistent frame-write outcome for one owned window: the preimage, the
/// attempted frame, the observed applied readback, and whether it matched
/// exactly. Unlike [`set_frame_owned`] there is deliberately NO restore:
/// the caller owns cleanup (the bounded border fixture destroys the
/// disposable helper at teardown and proves zero CG residue). Every path
/// before the first write refuses with no mutation; every path after it
/// retains the preimage evidence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PersistentFrameReport {
    pub pid: u32,
    pub wid: u32,
    pub preimage: crate::ffi::AxFrame,
    pub attempted: crate::ffi::AxFrame,
    pub applied: Option<crate::ffi::AxFrame>,
    pub matched: bool,
}

/// App-thread body for [`set_frame_owned_persistent`]. Same gates and
/// handle discipline as [`set_frame_owned`] (live-bundle gate, entry
/// revalidation, fresh CG WID join, per-leg messaging timeouts) but the
/// applied frame is LEFT in place: no restore write follows the applied
/// readback. Runs ONLY on the dedicated per-app AX worker.
#[cfg(target_os = "macos")]
/// `live_bundle` is resolved by the CALLER and threaded in as plain data:
/// `NSWorkspace` table reads off the main runloop thread are unreliable
/// (stale/empty snapshots misfire the direct-spawn fallback), so workers
/// never resolve the bundle themselves.
fn set_frame_persistent_on_worker(
    owned: &OwnedIdentity,
    entry: &crate::probe_target::SpawnRecord,
    live_bundle: &str,
    target_pid: u32,
    wid: u32,
    frame: crate::ffi::AxFrame,
) -> Result<PersistentFrameReport, AxError> {
    use crate::ffi::{AxElement, ax_get_frame, ax_window_id};
    gate_owned_frame(owned, target_pid, live_bundle)?;
    revalidate_entry(entry)?;
    let pid_i32 = target_pid as i32;
    let app = AxElement::application(pid_i32)
        .ok_or_else(|| AxError::System(format!("no AX element for pid {target_pid}")))?;
    let _ = crate::ffi::ax_set_messaging_timeout(&app, 2.0);
    let windows = app
        .windows()
        .map_err(|code| AxError::System(format!("windows: {}", crate::ffi::ax_error_name(code))))?;
    let window = windows
        .iter()
        .find(|w| ax_window_id(w) == Some(wid))
        .ok_or_else(|| {
            AxError::Ownership(format!(
                "wid {wid} not joined to owned pid {target_pid} (identity join failed)"
            ))
        })?;
    let _ = crate::ffi::ax_set_messaging_timeout(window, 2.0);
    let preimage = ax_get_frame(window).map_err(|code| {
        AxError::System(format!("preimage: {}", crate::ffi::ax_error_name(code)))
    })?;
    revalidate_entry(entry)?;
    let mut ops = LiveFrameOps {
        window,
        entry: entry.clone(),
        pid: target_pid,
        wid,
    };
    if let Err(e) = ops.revalidate() {
        return Err(AxError::Ownership(format!(
            "pre-write ownership refused: {e}"
        )));
    }
    if let Err(code) = ops.set_frame(&frame) {
        return Err(AxError::System(format!(
            "persistent set failed (preimage {preimage:?}, attempted {frame:?}): {}",
            crate::ffi::ax_error_name(code)
        )));
    }
    // Bounded applied poll: the window server applies asynchronously, so the
    // first readback may still show the preimage. Revalidate before the
    // readback series; ownership loss after the write refuses further reads
    // with the preimage retained (the window keeps whatever was applied).
    if let Err(e) = ops.revalidate() {
        return Err(AxError::Ownership(format!(
            "post-write ownership refused (preimage {preimage:?}, attempted {frame:?}): {e}"
        )));
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
    let mut applied: Option<crate::ffi::AxFrame> = None;
    while std::time::Instant::now() < deadline {
        match ops.get_frame() {
            Ok(current) => {
                applied = Some(current);
                if current == frame {
                    break;
                }
            }
            Err(code) => {
                return Err(AxError::System(format!(
                    "applied readback failed (preimage {preimage:?}, attempted {frame:?}): {}",
                    crate::ffi::ax_error_name(code)
                )));
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    let matched = applied == Some(frame);
    Ok(PersistentFrameReport {
        pid: target_pid,
        wid,
        preimage,
        attempted: frame,
        applied,
        matched,
    })
}

/// Move/resize one owned window and LEAVE the applied frame in place, with
/// preimage capture and an exact-match applied readback. Fixture-only
/// (disposable helper control for the bounded border tracking legs); never
/// a shipped product move, never foreign windows. The caller owns cleanup:
/// the border fixture destroys the helper at teardown and proves zero CG
/// residue instead of restoring. Threading matches [`set_frame_owned`]:
/// gates on the caller, then one dedicated AX worker with a bounded wait.
pub fn set_frame_owned_persistent(
    owned: &OwnedIdentity,
    target_pid: u32,
    wid: u32,
    frame: crate::ffi::AxFrame,
) -> Result<PersistentFrameReport, AxError> {
    if !frame.valid_for_set() {
        return Err(AxError::Unsupported("degenerate frame".to_owned()));
    }
    let live_bundle = live_owned_bundle(target_pid)?;
    gate_owned_frame(owned, target_pid, &live_bundle)?;
    let entry = crate::probe_target::load_verified_record(target_pid, &owned.nonce)
        .map_err(AxError::Ownership)?;
    #[cfg(target_os = "macos")]
    {
        let owned_send = owned.clone();
        let frame_send = frame;
        let bundle_send = live_bundle.clone();
        let (done_tx, done_rx) =
            std::sync::mpsc::channel::<Result<PersistentFrameReport, AxError>>();
        std::thread::Builder::new()
            .name(format!("ax-set-frame-persistent-{target_pid}"))
            .spawn(move || {
                let outcome = set_frame_persistent_on_worker(
                    &owned_send,
                    &entry,
                    &bundle_send,
                    target_pid,
                    wid,
                    frame_send,
                );
                let _ = done_tx.send(outcome);
            })
            .map_err(|e| AxError::System(format!("spawn AX worker thread: {e}")))?;
        match done_rx.recv_timeout(std::time::Duration::from_secs(SET_FRAME_OUTER_SECS)) {
            Ok(report) => report,
            Err(_) => Err(AxError::Timeout {
                pid: target_pid,
                timeout_ms: SET_FRAME_OUTER_SECS * 1000,
            }),
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (&entry, wid, frame);
        Err(AxError::Unsupported(
            "AX frame set is macOS-only".to_owned(),
        ))
    }
}

/// Owned window state with AX + CG frames (read-only: no writes on any
/// path). Every leg is `Option`: an unreadable attribute is unknown
/// evidence, never a guessed state. No titles, no content, no paths.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WindowStateReport {
    pub pid: u32,
    pub wid: u32,
    pub ax_frame: Option<crate::ffi::AxFrame>,
    pub cg_bounds: Option<crate::ffi::CgBounds>,
    pub cg_onscreen: Option<bool>,
    pub cg_layer: Option<i32>,
    pub minimized: Option<bool>,
    pub fullscreen: Option<bool>,
    pub role: Option<String>,
    pub subrole: Option<String>,
}

/// App-thread body for [`read_window_owned`]. Runs ONLY on the dedicated
/// per-app AX worker: constructs, uses, and drops every `AxElement` here.
/// Never called on the caller/main thread. Read-only throughout: entry +
/// CG revalidation gates the reads, nothing is written.
#[cfg(target_os = "macos")]
/// `live_bundle` is caller-resolved plain data (see
/// `set_frame_persistent_on_worker`): workers never read the table.
fn read_window_on_worker(
    owned: &OwnedIdentity,
    entry: &crate::probe_target::SpawnRecord,
    live_bundle: &str,
    target_pid: u32,
    wid: u32,
) -> Result<WindowStateReport, AxError> {
    use crate::ffi::{AxElement, ax_window_id};
    gate_owned_frame(owned, target_pid, live_bundle)?;
    revalidate_entry(entry)?;
    let app = AxElement::application(target_pid as i32)
        .ok_or_else(|| AxError::System(format!("no AX element for pid {target_pid}")))?;
    let _ = crate::ffi::ax_set_messaging_timeout(&app, 2.0);
    let windows = app
        .windows()
        .map_err(|code| AxError::System(format!("windows: {}", crate::ffi::ax_error_name(code))))?;
    let window = windows
        .iter()
        .find(|w| ax_window_id(w) == Some(wid))
        .ok_or_else(|| {
            AxError::Ownership(format!(
                "wid {wid} not joined to owned pid {target_pid} (identity join failed)"
            ))
        })?;
    let _ = crate::ffi::ax_set_messaging_timeout(window, 2.0);
    revalidate_entry(entry)?;
    // Fresh CG join for the same (pid, wid): the AX handle alone cannot
    // prove the WID still belongs to this PID.
    let cg = crate::cg::verify_wid_owned_by_pid(target_pid, wid).ok();
    Ok(WindowStateReport {
        pid: target_pid,
        wid,
        ax_frame: crate::ffi::ax_get_frame(window).ok(),
        cg_bounds: cg.as_ref().map(|w| w.bounds),
        cg_onscreen: cg.as_ref().map(|w| w.onscreen),
        cg_layer: cg.as_ref().map(|w| w.layer),
        minimized: crate::ffi::ax_bool_attribute(window, "AXMinimized").ok(),
        fullscreen: crate::ffi::ax_fullscreen_state(window).ok(),
        role: crate::ffi::ax_string_attribute(window, "AXRole")
            .ok()
            .flatten(),
        subrole: crate::ffi::ax_string_attribute(window, "AXSubrole")
            .ok()
            .flatten(),
    })
}

/// Read one owned window's AX + CG state with entry/CG revalidation.
/// Read-only: no writes on any path. Threading: gates on the caller, then
/// a dedicated per-app AX worker performs every `AxElement` call. The
/// caller waits bounded and reports a timeout rather than stalling.
pub fn read_window_owned(
    owned: &OwnedIdentity,
    target_pid: u32,
    wid: u32,
) -> Result<WindowStateReport, AxError> {
    if wid == 0 {
        return Err(AxError::Ownership("refusing window id 0".to_owned()));
    }
    let live_bundle = live_owned_bundle(target_pid)?;
    gate_owned_frame(owned, target_pid, &live_bundle)?;
    let entry = crate::probe_target::load_verified_record(target_pid, &owned.nonce)
        .map_err(AxError::Ownership)?;
    #[cfg(target_os = "macos")]
    {
        let owned_send = owned.clone();
        let bundle_send = live_bundle.clone();
        let (done_tx, done_rx) = std::sync::mpsc::channel::<Result<WindowStateReport, AxError>>();
        std::thread::Builder::new()
            .name(format!("ax-read-window-{target_pid}"))
            .spawn(move || {
                let outcome =
                    read_window_on_worker(&owned_send, &entry, &bundle_send, target_pid, wid);
                let _ = done_tx.send(outcome);
            })
            .map_err(|e| AxError::System(format!("spawn AX worker thread: {e}")))?;
        match done_rx.recv_timeout(std::time::Duration::from_secs(15)) {
            Ok(report) => report,
            Err(_) => Err(AxError::Timeout {
                pid: target_pid,
                timeout_ms: 15_000,
            }),
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = entry;
        Err(AxError::Unsupported(
            "AX window read is macOS-only".to_owned(),
        ))
    }
}

/// Owned minimize/restore outcome: the requested state, the preimage, the
/// readback, and whether the readback proves the request landed. An
/// accepted OS status alone never proves it — only `applied`
/// (`minimized == Some(want)`) does. No geometry writes on any path, so no
/// frame restoration is owed; a failed leg leaves the window as-is with
/// the readback as evidence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MinimizeReport {
    pub pid: u32,
    pub wid: u32,
    pub want_minimized: bool,
    pub was_minimized: Option<bool>,
    pub minimized: Option<bool>,
    pub applied: bool,
    pub already: bool,
    pub ax_frame: Option<crate::ffi::AxFrame>,
    pub cg_onscreen: Option<bool>,
}

/// App-thread body for [`set_minimized_owned`]. Runs ONLY on the dedicated
/// per-app AX worker. Entry + CG revalidation before the write and before
/// the readback; the boolean setter is the single write.
#[cfg(target_os = "macos")]
fn set_minimized_on_worker(
    owned: &OwnedIdentity,
    entry: &crate::probe_target::SpawnRecord,
    target_pid: u32,
    wid: u32,
    want: bool,
) -> Result<MinimizeReport, AxError> {
    use crate::ffi::{AxElement, ax_window_id};
    let live_bundle = live_owned_bundle(target_pid)?;
    gate_owned_frame(owned, target_pid, &live_bundle)?;
    revalidate_entry(entry)?;
    let app = AxElement::application(target_pid as i32)
        .ok_or_else(|| AxError::System(format!("no AX element for pid {target_pid}")))?;
    let _ = crate::ffi::ax_set_messaging_timeout(&app, 2.0);
    let windows = app
        .windows()
        .map_err(|code| AxError::System(format!("windows: {}", crate::ffi::ax_error_name(code))))?;
    let window = windows
        .iter()
        .find(|w| ax_window_id(w) == Some(wid))
        .ok_or_else(|| {
            AxError::Ownership(format!(
                "wid {wid} not joined to owned pid {target_pid} (identity join failed)"
            ))
        })?;
    let _ = crate::ffi::ax_set_messaging_timeout(window, 2.0);
    let was = crate::ffi::ax_bool_attribute(window, "AXMinimized").ok();
    revalidate_entry(entry)?;
    crate::cg::verify_wid_owned_by_pid(target_pid, wid)
        .map_err(|e| AxError::Ownership(format!("CG rejoin before minimize write refused: {e}")))?;
    crate::ffi::ax_set_bool_attribute(window, "AXMinimized", want).map_err(|code| {
        AxError::System(format!("minimize: {}", crate::ffi::ax_error_name(code)))
    })?;
    // Settle + re-read: the compositor applies minimization asynchronously
    // (observed live: an accepted write still reads back the old state
    // immediately after). Poll boundedly; the verdict below still requires
    // the read state to equal the request — an accepted status alone never
    // proves it. Reads only from here on; no further writes whatever the
    // outcome.
    let settle_deadline = std::time::Instant::now() + std::time::Duration::from_millis(2000);
    let minimized = loop {
        let state = crate::ffi::ax_bool_attribute(window, "AXMinimized").ok();
        if state == Some(want) || std::time::Instant::now() >= settle_deadline {
            break state;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    };
    let applied = minimized == Some(want);
    if !applied {
        return Err(AxError::System(format!(
            "minimize write accepted but readback mismatch (want {want}, have {minimized:?}); window left as-is, no geometry written"
        )));
    }
    Ok(MinimizeReport {
        pid: target_pid,
        wid,
        want_minimized: want,
        was_minimized: was,
        minimized,
        applied: true,
        already: was == Some(want),
        ax_frame: crate::ffi::ax_get_frame(window).ok(),
        cg_onscreen: crate::cg::verify_wid_owned_by_pid(target_pid, wid)
            .map(|w| w.onscreen)
            .ok(),
    })
}

/// Minimize or restore one owned window with preimage + readback proof.
/// Explicit owned minimize/restore only (disposable helper fixture
/// control); never hides foreign windows. Threading: gates on the caller,
/// then a dedicated per-app AX worker with a bounded wait.
pub fn set_minimized_owned(
    owned: &OwnedIdentity,
    target_pid: u32,
    wid: u32,
    want: bool,
) -> Result<MinimizeReport, AxError> {
    if wid == 0 {
        return Err(AxError::Ownership("refusing window id 0".to_owned()));
    }
    let live_bundle = live_owned_bundle(target_pid)?;
    gate_owned_frame(owned, target_pid, &live_bundle)?;
    let entry = crate::probe_target::load_verified_record(target_pid, &owned.nonce)
        .map_err(AxError::Ownership)?;
    #[cfg(target_os = "macos")]
    {
        let owned_send = owned.clone();
        let (done_tx, done_rx) = std::sync::mpsc::channel::<Result<MinimizeReport, AxError>>();
        std::thread::Builder::new()
            .name(format!("ax-minimize-{target_pid}"))
            .spawn(move || {
                let outcome = set_minimized_on_worker(&owned_send, &entry, target_pid, wid, want);
                let _ = done_tx.send(outcome);
            })
            .map_err(|e| AxError::System(format!("spawn AX worker thread: {e}")))?;
        match done_rx.recv_timeout(std::time::Duration::from_secs(15)) {
            Ok(report) => report,
            Err(_) => Err(AxError::Timeout {
                pid: target_pid,
                timeout_ms: 15_000,
            }),
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (entry, want);
        Err(AxError::Unsupported("AX minimize is macOS-only".to_owned()))
    }
}

/// Owned fullscreen enter/exit outcome: the requested state, the preimage,
/// the readback, and whether the readback proves the request landed. An
/// accepted OS status alone never proves it — only `applied`
/// (`fullscreen == Some(want)`) does. Fixture-only control for the bounded
/// owned helper (never a shipped product fullscreen feature, never foreign
/// windows). No geometry writes on any path; a failed leg leaves the window
/// as-is with the readback as evidence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FullscreenReport {
    pub pid: u32,
    pub wid: u32,
    pub want_fullscreen: bool,
    pub was_fullscreen: Option<bool>,
    pub fullscreen: Option<bool>,
    pub applied: bool,
    pub already: bool,
    pub ax_frame: Option<crate::ffi::AxFrame>,
    pub cg_onscreen: Option<bool>,
}

/// App-thread body for [`set_fullscreen_owned`]. Runs ONLY on the dedicated
/// per-app AX worker. Entry + CG revalidation before the write and before
/// the readback; the boolean setter is the single write. Spelling follows
/// the reader: `AXFullScreen` (yabai) first, `AXFullscreen` (rift) fallback
/// on service failures only.
///
/// `held` selects the fullscreen-exit revalidation: `false` is the standard
/// title-proof path; `true` uses [`revalidate_entry_held`] (record file
/// plus live executable plus fresh CG join, no title proof) for the exit
/// back out of native fullscreen, where the window server withholds the
/// title proof. The caller proves held-Child ownership around the worker.
#[cfg(target_os = "macos")]
/// `live_bundle` is caller-resolved plain data (see
/// `set_frame_persistent_on_worker`): workers never read the table.
fn set_fullscreen_on_worker(
    owned: &OwnedIdentity,
    entry: &crate::probe_target::SpawnRecord,
    live_bundle: &str,
    target_pid: u32,
    wid: u32,
    want: bool,
    held: bool,
) -> Result<FullscreenReport, AxError> {
    use crate::ffi::{AxElement, ax_window_id};
    gate_owned_frame(owned, target_pid, live_bundle)?;
    if held {
        revalidate_entry_held(entry, target_pid, wid)?;
    } else {
        revalidate_entry(entry)?;
    }
    let app = AxElement::application(target_pid as i32)
        .ok_or_else(|| AxError::System(format!("no AX element for pid {target_pid}")))?;
    let _ = crate::ffi::ax_set_messaging_timeout(&app, 2.0);
    // Join the exact window, polling boundedly: a fullscreen transition
    // animates asynchronously, so the hierarchy can momentarily omit or
    // unjoin the window right after entry. Reads only until the join
    // holds; the refusal below names the final shape (counts/statuses
    // only, never titles) instead of the first miss.
    let join_deadline = std::time::Instant::now() + std::time::Duration::from_millis(5000);
    let windows = loop {
        let windows = app.windows().map_err(|code| {
            AxError::System(format!("windows: {}", crate::ffi::ax_error_name(code)))
        })?;
        if windows.iter().any(|w| ax_window_id(w) == Some(wid)) {
            break windows;
        }
        if std::time::Instant::now() >= join_deadline {
            let mut joined = 0usize;
            let mut unjoined = 0usize;
            let mut fullscreen_true = 0usize;
            for w in &windows {
                if ax_window_id(w).is_some() {
                    joined += 1;
                } else {
                    unjoined += 1;
                }
                if crate::ffi::ax_fullscreen_state(w).ok() == Some(true) {
                    fullscreen_true += 1;
                }
            }
            return Err(AxError::Ownership(format!(
                "wid {wid} not joined to owned pid {target_pid} after the bounded join poll (ax_windows={} joined={joined} unjoined={unjoined} fullscreen_true={fullscreen_true})",
                windows.len()
            )));
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    };
    let window = windows
        .iter()
        .find(|w| ax_window_id(w) == Some(wid))
        .expect("bounded join poll held the window join");
    let _ = crate::ffi::ax_set_messaging_timeout(window, 2.0);
    let was = crate::ffi::ax_fullscreen_state(window).ok();
    if held {
        revalidate_entry_held(entry, target_pid, wid)?;
    } else {
        revalidate_entry(entry)?;
    }
    crate::cg::verify_wid_owned_by_pid(target_pid, wid).map_err(|e| {
        AxError::Ownership(format!("CG rejoin before fullscreen write refused: {e}"))
    })?;
    let set_result = crate::ffi::ax_set_bool_attribute(window, "AXFullScreen", want);
    if let Err(code) = set_result {
        const FALLBACK_CODES: [i32; 3] = [
            crate::ffi::AX_ATTRIBUTE_UNSUPPORTED,
            crate::ffi::AX_NO_VALUE,
            crate::ffi::AX_NOT_IMPLEMENTED,
        ];
        if FALLBACK_CODES.contains(&code) {
            crate::ffi::ax_set_bool_attribute(window, "AXFullscreen", want).map_err(
                |fallback| {
                    AxError::System(format!(
                        "fullscreen: {}",
                        crate::ffi::ax_error_name(fallback)
                    ))
                },
            )?;
        } else {
            return Err(AxError::System(format!(
                "fullscreen: {}",
                crate::ffi::ax_error_name(code)
            )));
        }
    }
    // Settle + re-read: native Space animation applies asynchronously
    // (an accepted write still reads back the old state immediately after).
    // Poll boundedly; the verdict below still requires the read state to
    // equal the request. Reads only from here on; no further writes.
    let settle_deadline = std::time::Instant::now() + std::time::Duration::from_millis(5000);
    let fullscreen = loop {
        let state = crate::ffi::ax_fullscreen_state(window).ok();
        if state == Some(want) || std::time::Instant::now() >= settle_deadline {
            break state;
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    };
    let applied = fullscreen == Some(want);
    if !applied {
        return Err(AxError::System(format!(
            "fullscreen write accepted but readback mismatch (want {want}, have {fullscreen:?}); window left as-is, no geometry written"
        )));
    }
    Ok(FullscreenReport {
        pid: target_pid,
        wid,
        want_fullscreen: want,
        was_fullscreen: was,
        fullscreen,
        applied: true,
        already: was == Some(want),
        ax_frame: crate::ffi::ax_get_frame(window).ok(),
        cg_onscreen: crate::cg::verify_wid_owned_by_pid(target_pid, wid)
            .map(|w| w.onscreen)
            .ok(),
    })
}

/// Enter or exit native fullscreen on one owned window with preimage +
/// readback proof. Fixture-only (disposable helper control for the bounded
/// fullscreen enter/exit preflight); never a shipped feature, never foreign
/// windows. Threading: gates on the caller, then a dedicated per-app AX
/// worker with a bounded wait.
pub fn set_fullscreen_owned(
    owned: &OwnedIdentity,
    target_pid: u32,
    wid: u32,
    want: bool,
) -> Result<FullscreenReport, AxError> {
    if wid == 0 {
        return Err(AxError::Ownership("refusing window id 0".to_owned()));
    }
    let live_bundle = live_owned_bundle(target_pid)?;
    gate_owned_frame(owned, target_pid, &live_bundle)?;
    let entry = crate::probe_target::load_verified_record(target_pid, &owned.nonce)
        .map_err(AxError::Ownership)?;
    #[cfg(target_os = "macos")]
    {
        let owned_send = owned.clone();
        let bundle_send = live_bundle.clone();
        let (done_tx, done_rx) = std::sync::mpsc::channel::<Result<FullscreenReport, AxError>>();
        std::thread::Builder::new()
            .name(format!("ax-fullscreen-{target_pid}"))
            .spawn(move || {
                let outcome = set_fullscreen_on_worker(
                    &owned_send,
                    &entry,
                    &bundle_send,
                    target_pid,
                    wid,
                    want,
                    false,
                );
                let _ = done_tx.send(outcome);
            })
            .map_err(|e| AxError::System(format!("spawn AX worker thread: {e}")))?;
        match done_rx.recv_timeout(std::time::Duration::from_secs(20)) {
            Ok(report) => report,
            Err(_) => Err(AxError::Timeout {
                pid: target_pid,
                timeout_ms: 20_000,
            }),
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (entry, want);
        Err(AxError::Unsupported(
            "AX fullscreen is macOS-only".to_owned(),
        ))
    }
}

/// Exit (or enter) native fullscreen on one HELD owned window without the
/// live window-nonce title proof.
///
/// New-hypothesis control for the exact failure the title-proof path cannot
/// pass: while the helper is in native fullscreen the window server
/// withholds `kCGWindowName`, so [`load_verified_record`] (and its AX
/// title fallback) refuse the exit write even though the same holder proved
/// entry moments earlier. This path replaces the title proof with the exact
/// held `Child` handle: [`load_held_record`] requires the handle to name
/// exactly `target_pid` and still be unreaped, which excludes PID reuse
/// across the write (an unreaped child reserves its PID); the worker still
/// revalidates the record file, the live executable, and a fresh CG
/// owner-PID/window join at each boundary. Entry into fullscreen keeps the
/// title-proof [`set_fullscreen_owned`]; cross-command uses without a
/// `Child` are unchanged.
///
/// Threading mirrors [`set_fullscreen_owned`]: gates on the caller, then a
/// dedicated per-app AX worker with a bounded wait, plus a post-join
/// held-Child liveness check proving the same process survived the write.
pub fn set_fullscreen_owned_held(
    owned: &OwnedIdentity,
    child: &mut std::process::Child,
    helper_path: &str,
    target_pid: u32,
    wid: u32,
    want: bool,
) -> Result<FullscreenReport, AxError> {
    if wid == 0 {
        return Err(AxError::Ownership("refusing window id 0".to_owned()));
    }
    let live_bundle = live_owned_bundle(target_pid)?;
    gate_owned_frame(owned, target_pid, &live_bundle)?;
    let entry = crate::probe_target::load_held_record(target_pid, &owned.nonce, child, helper_path)
        .map_err(AxError::Ownership)?;
    crate::cg::verify_wid_owned_by_pid(target_pid, wid).map_err(|e| {
        AxError::Ownership(format!("CG join before held fullscreen write refused: {e}"))
    })?;
    #[cfg(target_os = "macos")]
    {
        let owned_send = owned.clone();
        let bundle_send = live_bundle.clone();
        let (done_tx, done_rx) = std::sync::mpsc::channel::<Result<FullscreenReport, AxError>>();
        std::thread::Builder::new()
            .name(format!("ax-fullscreen-held-{target_pid}"))
            .spawn(move || {
                let outcome = set_fullscreen_on_worker(
                    &owned_send,
                    &entry,
                    &bundle_send,
                    target_pid,
                    wid,
                    want,
                    true,
                );
                let _ = done_tx.send(outcome);
            })
            .map_err(|e| AxError::System(format!("spawn AX worker thread: {e}")))?;
        let report = match done_rx.recv_timeout(std::time::Duration::from_secs(20)) {
            Ok(report) => report,
            Err(_) => {
                return Err(AxError::Timeout {
                    pid: target_pid,
                    timeout_ms: 20_000,
                });
            }
        };
        // Post-write held proof: the exact child is still unreaped, so the
        // PID above could not have been reused across the worker write —
        // the write provably reached our helper.
        match child.try_wait() {
            Ok(None) => report,
            Ok(Some(status)) => Err(AxError::Ownership(format!(
                "held child for pid {target_pid} exited during the fullscreen write ({status}); effect unprovable"
            ))),
            Err(e) => Err(AxError::Ownership(format!(
                "held child liveness unreadable after the fullscreen write ({e}); effect unprovable"
            ))),
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (entry, child, helper_path, want);
        Err(AxError::Unsupported(
            "AX fullscreen is macOS-only".to_owned(),
        ))
    }
}

/// Raise outcome with proven focus state. `focus_verified` is a measured
/// readback (focused app/window equals the raised target), never the OS
/// accept status alone; `focus_restored` proves the preimage was recovered
/// when the raise did not land.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RaiseReport {
    pub pid: u32,
    pub wid: u32,
    pub preimage_pid: u32,
    pub preimage_wid: u32,
    pub focused_pid: Option<u32>,
    pub focused_wid: Option<u32>,
    pub focus_verified: bool,
    pub focus_restored: bool,
}

/// App-thread body for [`raise_owned`]. Runs ONLY on the dedicated
/// per-app AX worker: constructs, uses, and drops every `AxElement` here,
/// including the focus-preimage reads. Never called on caller/main.
#[cfg(target_os = "macos")]
fn raise_on_worker(
    owned: &OwnedIdentity,
    entry: &crate::probe_target::SpawnRecord,
    target_pid: u32,
    wid: u32,
) -> Result<RaiseReport, AxError> {
    use crate::ffi::{AxElement, ax_raise, ax_window_id};
    let live_bundle = live_owned_bundle(target_pid)?;
    gate_owned_frame(owned, target_pid, &live_bundle)?;
    // Same spawn exe/nonce plus live window nonce as command entry, before any mutation.
    revalidate_entry(entry)?;
    let app = AxElement::application(target_pid as i32)
        .ok_or_else(|| AxError::System(format!("no AX element for pid {target_pid}")))?;
    let _ = crate::ffi::ax_set_messaging_timeout(&app, 2.0);
    let windows = app
        .windows()
        .map_err(|code| AxError::System(format!("windows: {}", crate::ffi::ax_error_name(code))))?;
    let window = windows
        .iter()
        .find(|w| ax_window_id(w) == Some(wid))
        .ok_or_else(|| {
            AxError::Ownership(format!(
                "wid {wid} not joined to owned pid {target_pid} (identity join failed)"
            ))
        })?;
    // Exact focus preimage BEFORE any change. Unreadable (no consent)
    // or foreign (terminal, another app) fails closed here.
    let (pre_pid, pre_wid) = crate::ffi::ax_focused_preimage().ok_or_else(|| {
        AxError::System(
            "focus preimage unreadable (AX consent or no focused window); refusing to change focus"
                .to_owned(),
        )
    })?;
    if pre_pid != target_pid as i32 {
        return Err(AxError::Ownership(format!(
            "focus preimage is foreign (pid {pre_pid} != owned {target_pid}); refusing to steal focus — focus the owned probe window first (user action), then retry"
        )));
    }
    let pre_wid = pre_wid.ok_or_else(|| {
        AxError::System(
            "focus preimage window unknown; refusing to change focus without a restorable preimage"
                .to_owned(),
        )
    })?;
    // Revalidate immediately before changing focus.
    revalidate_entry(entry)?;
    ax_raise(window)
        .map_err(|code| AxError::System(format!("raise: {}", crate::ffi::ax_error_name(code))))?;
    // Readback: focused app/window must equal the raised target.
    let post = crate::ffi::ax_focused_preimage();
    let (focused_pid, focused_wid) = match post {
        Some((p, w)) => (u32::try_from(p).ok(), w),
        None => (None, None),
    };
    let focus_verified = focused_pid == Some(target_pid) && focused_wid == Some(wid);
    if focus_verified {
        return Ok(RaiseReport {
            pid: target_pid,
            wid,
            preimage_pid: target_pid,
            preimage_wid: pre_wid,
            focused_pid,
            focused_wid,
            focus_verified: true,
            focus_restored: true,
        });
    }
    // Raise did not land on the target: restore the owned preimage
    // window (same owned app, safe) and re-read. Revalidate before the
    // cleanup write, like every restore path.
    let mut focus_restored = false;
    if revalidate_entry(entry).is_ok()
        && let Some(preimage_window) = windows.iter().find(|w| ax_window_id(w) == Some(pre_wid))
        && ax_raise(preimage_window).is_ok()
        && let Some((restored_pid, restored_wid)) = crate::ffi::ax_focused_preimage()
    {
        focus_restored = restored_pid == target_pid as i32 && restored_wid == Some(pre_wid);
    }
    Err(AxError::System(format!(
        "raise accepted but focus readback mismatch (focused pid={focused_pid:?} wid={focused_wid:?}, want pid={target_pid} wid={wid}); preimage restore attempted: {focus_restored}"
    )))
}

/// Raise an owned window with an exact focus preimage and readback.
///
/// Fail-closed focus policy: the current focus preimage must already be the
/// owned session (same PID, known window) BEFORE anything changes —
/// otherwise this refuses without touching focus, so a raise can never
/// steal focus from the controlling terminal or any foreign app. The user
/// focuses the owned probe window first (ordinary user action), then
/// retries. Restoration re-raises only the owned preimage window, never a
/// foreign one. Success requires the post-raise focus readback to equal the
/// target; the OS accept status alone is not evidence.
///
/// Threading: AppKit discovery + ownership gates run on the caller, then a
/// dedicated per-app AX worker performs every `AxElement`/focus call. The
/// caller waits bounded and reports a timeout rather than stalling.
pub fn raise_owned(
    owned: &OwnedIdentity,
    target_pid: u32,
    wid: u32,
) -> Result<RaiseReport, AxError> {
    let live_bundle = live_owned_bundle(target_pid)?;
    gate_owned_frame(owned, target_pid, &live_bundle)?;
    // Entry record snapshot for the worker's write-boundary revalidation.
    let entry = crate::probe_target::load_verified_record(target_pid, &owned.nonce)
        .map_err(AxError::Ownership)?;
    #[cfg(target_os = "macos")]
    {
        let owned_send = owned.clone();
        let (done_tx, done_rx) = std::sync::mpsc::channel::<Result<RaiseReport, AxError>>();
        std::thread::Builder::new()
            .name(format!("ax-raise-{target_pid}"))
            .spawn(move || {
                let outcome = raise_on_worker(&owned_send, &entry, target_pid, wid);
                let _ = done_tx.send(outcome);
            })
            .map_err(|e| AxError::System(format!("spawn AX worker thread: {e}")))?;
        match done_rx.recv_timeout(std::time::Duration::from_secs(15)) {
            Ok(report) => report,
            Err(_) => Err(AxError::Timeout {
                pid: target_pid,
                timeout_ms: 15_000,
            }),
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (&entry, wid);
        Err(AxError::Unsupported("AX raise is macOS-only".to_owned()))
    }
}

/// Write-boundary revalidations on the `set_frame_owned` worker path: 2 in
/// `set_frame_on_worker` (before any mutation, before the transaction) plus
/// 3 inside `run_frame_transaction` (pre-write, post-set, pre-restore).
#[cfg(any(test, target_os = "macos"))]
const FRAME_TX_REVALIDATIONS: u64 = 5;
/// Each revalidation can run the AX-title fallback worker (bounded 8 s) when
/// CG titles are withheld.
#[cfg(any(test, target_os = "macos"))]
const REVALIDATION_FALLBACK_SECS: u64 = 8;
/// Every frame AX leg runs under the 2 s element messaging timeout (set on
/// both the app and the owned window); a leg is at most 2 calls
/// (position+size writes, or position/size reads).
#[cfg(any(test, target_os = "macos"))]
const FRAME_LEG_TIMEOUT_SECS: u64 = 2;
#[cfg(any(test, target_os = "macos"))]
const FRAME_LEG_CALLS: u64 = 2;
/// Frame legs on the path: preimage get, attempted set, applied get,
/// restore set, restore readback.
#[cfg(any(test, target_os = "macos"))]
const FRAME_LEGS: u64 = 5;
/// Slack for snapshots, thread spawn, and channel handoff.
#[cfg(any(test, target_os = "macos"))]
const FRAME_OVERHEAD_SECS: u64 = 15;

/// Conservative caller bound for `set_frame_owned`: 5×8 s revalidations +
/// 5×(2×2 s) frame legs + 15 s overhead = 75 s. A caller timeout therefore
/// means "outcome unknown, a restore may still be in flight" — never clean,
/// never complete.
///
/// `pub(crate)` so holder-grade sessions (`ax_session`) wait out the same
/// documented worst case on their own worker threads.
#[cfg(any(test, target_os = "macos"))]
pub(crate) const SET_FRAME_OUTER_SECS: u64 = FRAME_TX_REVALIDATIONS * REVALIDATION_FALLBACK_SECS
    + FRAME_LEGS * FRAME_LEG_CALLS * FRAME_LEG_TIMEOUT_SECS
    + FRAME_OVERHEAD_SECS;

#[cfg(test)]
mod tests {
    use super::*;

    fn owned() -> OwnedIdentity {
        OwnedIdentity::new(4242, crate::BUNDLE_ID, "0123456789abcdef").unwrap()
    }

    #[test]
    fn enumerate_bound_enforced() {
        assert!(enumerate_counts(0).is_err());
        assert!(enumerate_counts(crate::config::MAX_WINDOWS + 1).is_err());
    }

    #[test]
    fn set_frame_outer_bound_covers_documented_worst_case() {
        // Exact worst-call budget, kept in sync with the worker path by
        // construction: 5 write-boundary revalidations (2 pre-transaction +
        // 3 inside) × 8 s AX-title fallback each, plus 5 frame legs
        // (preimage/attempted/applied/restore/verify) × 2 calls × 2 s
        // element messaging timeout, plus 15 s snapshot/spawn slack.
        assert_eq!(FRAME_TX_REVALIDATIONS, 5);
        assert_eq!(
            SET_FRAME_OUTER_SECS,
            FRAME_TX_REVALIDATIONS * REVALIDATION_FALLBACK_SECS
                + FRAME_LEGS * FRAME_LEG_CALLS * FRAME_LEG_TIMEOUT_SECS
                + FRAME_OVERHEAD_SECS
        );
        assert_eq!(SET_FRAME_OUTER_SECS, 75);
    }

    #[test]
    fn aggregate_uses_one_global_deadline_for_many_receivers() {
        // Scripted receivers: no apps, no AX, no worker threads. Half answer
        // immediately, half hang forever (sender kept alive, never sends).
        // The old sequential `recv_timeout(3s)` shape would cost N*3s here;
        // the global deadline must bound the whole aggregate instead, while
        // still collecting every ready outcome regardless of order.
        let mut receivers = Vec::new();
        for i in 0..6 {
            let (tx, rx) = std::sync::mpsc::channel::<EnumCount>();
            if i % 2 == 0 {
                tx.send(EnumCount {
                    windows: 3,
                    failed: false,
                })
                .unwrap();
            } else {
                std::mem::forget(tx);
            }
            receivers.push(rx);
        }
        let start = std::time::Instant::now();
        let (windows, failures, truncated) = aggregate_counts(
            receivers,
            256,
            start + std::time::Duration::from_millis(200),
        );
        assert!(
            start.elapsed() < std::time::Duration::from_secs(5),
            "aggregate must respect the global deadline, took {:?}",
            start.elapsed()
        );
        // Ready outcomes are collected despite hung receivers ahead of them.
        assert_eq!(windows, 9);
        assert_eq!(failures, 3);
        assert!(truncated, "uncollected remainder must be flagged");
    }

    #[test]
    fn aggregate_reports_window_cap_truncation() {
        let (tx, rx) = std::sync::mpsc::channel::<EnumCount>();
        tx.send(EnumCount {
            windows: 10,
            failed: false,
        })
        .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let (windows, failures, truncated) = aggregate_counts(vec![rx], 4, deadline);
        assert_eq!(windows, 4);
        assert_eq!(failures, 0);
        assert!(truncated);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn enumerate_reports_real_counts_with_consent_state() {
        // Real query: apps seen must be nonzero on any live Mac; trust state
        // must be explicit (never defaulted).
        let counts = enumerate_counts(16).unwrap();
        assert!(counts.apps > 0);
        assert!(counts.process_trusted.is_some());
        assert!(counts.windows <= 16);
    }

    #[test]
    fn frame_gate_refuses_foreign() {
        let id = owned();
        assert!(gate_owned_frame(&id, 4242, crate::BUNDLE_ID).is_ok());
        assert!(gate_owned_frame(&id, 4243, crate::BUNDLE_ID).is_err());
        assert!(gate_owned_frame(&id, 4242, "com.mitchellh.ghostty").is_err());
    }

    #[test]
    fn degenerate_frame_rejected_before_any_ax() {
        use crate::ffi::AxFrame;
        let id = owned();
        let bad = AxFrame {
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
        };
        assert!(set_frame_owned(&id, 4242, 3, bad).is_err());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn observer_rejects_dead_pid_fast() {
        use std::time::Instant;
        let (engine, _rx) = crate::engine::Engine::spawn();
        let id = owned();
        let start = Instant::now();
        // i32::MAX cannot be a live owned session: authorization must fail
        // fast, never hang to the timeout.
        let err = observe_app(&id, i32::MAX as u32, 4000, &engine.tx).unwrap_err();
        assert!(start.elapsed() < std::time::Duration::from_secs(3));
        assert!(matches!(err, AxError::Ownership(_) | AxError::System(_)));
    }

    #[test]
    fn observe_refuses_foreign_pid_before_any_ax() {
        let (engine, _rx) = crate::engine::Engine::spawn();
        let id = owned();
        // Owned identity is pid 4242; any other PID is refused by the gate
        // even before session lookup.
        let err = observe_app(&id, 4243, 500, &engine.tx).unwrap_err();
        assert!(matches!(err, AxError::Ownership(_)));
    }

    #[test]
    fn owned_paths_require_spawn_record() {
        // No spawn record exists for this PID: every owned path refuses.
        assert!(authorize_owned_action(4242, "0123456789abcdef").is_err());
        assert!(
            set_frame_owned(
                &owned(),
                4242,
                3,
                crate::ffi::AxFrame {
                    x: 240.0,
                    y: 240.0,
                    w: 480.0,
                    h: 320.0,
                },
            )
            .is_err()
        );
        assert!(raise_owned(&owned(), 4242, 3).is_err());
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn observer_is_refused_off_macos() {
        let (engine, _rx) = crate::engine::Engine::spawn();
        // Off-macOS there is no live owned session, so the owned gate
        // refuses before any platform path is reached.
        assert!(observe_app(&owned(), 4242, 500, &engine.tx).is_err());
    }

    // -- Frame-transaction regression tests (pure, all platforms) --
    //
    // Scripted fake over `FrameTxOps`: no AX, no threads, no live state.
    // Each test pins one failure leg and asserts the transaction restores
    // the exact preimage (or explicitly refuses) with structured evidence.

    fn preimage() -> crate::ffi::AxFrame {
        crate::ffi::AxFrame {
            x: 200.0,
            y: 200.0,
            w: 480.0,
            h: 320.0,
        }
    }

    fn attempted() -> crate::ffi::AxFrame {
        crate::ffi::AxFrame {
            x: 240.0,
            y: 240.0,
            w: 480.0,
            h: 320.0,
        }
    }

    // Rebuild the fake with an explicit preimage shadow (the struct above
    // needs the shadow field; define the full struct here instead).
    struct FakeTx {
        current: crate::ffi::AxFrame,
        pre: crate::ffi::AxFrame,
        set_fail_next: Option<i32>,
        set_partial: bool,
        get_fail_next: Option<i32>,
        restore_ignored: bool,
        revalidate_fail_from: Option<usize>,
        set_calls: usize,
        get_calls: usize,
        revalidations: usize,
        restore_writes: usize,
    }

    impl FakeTx {
        fn new(pre: crate::ffi::AxFrame) -> Self {
            Self {
                current: pre,
                pre,
                set_fail_next: None,
                set_partial: false,
                get_fail_next: None,
                restore_ignored: false,
                revalidate_fail_from: None,
                set_calls: 0,
                get_calls: 0,
                revalidations: 0,
                restore_writes: 0,
            }
        }
    }

    impl FrameTxOps for FakeTx {
        fn set_frame(&mut self, frame: &crate::ffi::AxFrame) -> Result<(), i32> {
            self.set_calls += 1;
            if *frame == self.pre {
                self.restore_writes += 1;
                if self.restore_ignored {
                    return Ok(());
                }
            }
            if let Some(code) = self.set_fail_next.take() {
                if self.set_partial {
                    self.current.x = frame.x;
                    self.current.y = frame.y;
                }
                return Err(code);
            }
            self.current = *frame;
            Ok(())
        }

        fn get_frame(&mut self) -> Result<crate::ffi::AxFrame, i32> {
            self.get_calls += 1;
            if let Some(code) = self.get_fail_next.take() {
                return Err(code);
            }
            Ok(self.current)
        }

        fn revalidate(&mut self) -> Result<(), String> {
            self.revalidations += 1;
            if let Some(from) = self.revalidate_fail_from
                && self.revalidations >= from
            {
                return Err("spawn record changed since command entry".to_owned());
            }
            Ok(())
        }
    }

    fn tx_failure(err: AxError) -> FrameTxFailure {
        match err {
            AxError::FrameTx(f) => *f,
            other => panic!("expected FrameTx failure, got {other:?}"),
        }
    }

    #[test]
    fn tx_success_restores_exact_preimage_with_proof() {
        let pre = preimage();
        let att = attempted();
        let mut ops = FakeTx::new(pre);
        let report = run_frame_transaction(&mut ops, 4242, 7, pre, att).unwrap();
        assert_eq!(report.preimage, pre);
        assert_eq!(report.attempted, att);
        assert_eq!(report.applied, Some(att));
        assert_eq!(report.restore_readback, Some(pre));
        assert!(report.restored);
    }

    #[test]
    fn tx_partial_setter_mutation_still_restores() {
        // Position write accepted, size write refused: the window is left
        // partially mutated (x/y moved, w/h stale). The transaction must
        // still restore the exact preimage and report the set failure with
        // the restore proof — never leave the partial frame behind.
        let pre = preimage();
        let att = attempted();
        let mut ops = FakeTx::new(pre);
        ops.set_fail_next = Some(-25204);
        ops.set_partial = true;
        let err = run_frame_transaction(&mut ops, 4242, 7, pre, att).unwrap_err();
        let f = tx_failure(err);
        assert_eq!(f.stage, "set");
        assert_eq!(f.preimage, pre);
        assert_eq!(f.attempted, att);
        assert!(f.error.contains("set:"));
        assert_eq!(ops.restore_writes, 1, "partial write must trigger restore");
        assert_eq!(ops.current, pre, "window must end on the exact preimage");
        assert_eq!(f.restore_readback, Some(pre));
        assert!(f.restore_error.is_none());
    }

    #[test]
    fn tx_original_write_fail_restores() {
        let pre = preimage();
        let att = attempted();
        let mut ops = FakeTx::new(pre);
        ops.set_fail_next = Some(-25200);
        let err = run_frame_transaction(&mut ops, 4242, 7, pre, att).unwrap_err();
        let f = tx_failure(err);
        assert_eq!(f.stage, "set");
        assert_eq!(f.preimage, pre);
        assert_eq!(f.attempted, att);
        assert_eq!(ops.restore_writes, 1);
        assert_eq!(f.restore_readback, Some(pre));
    }

    #[test]
    fn tx_readback_fail_restores_with_proof() {
        let pre = preimage();
        let att = attempted();
        let mut ops = FakeTx::new(pre);
        // First get (applied readback) fails; the later restore readback
        // succeeds, proving the restore.
        ops.get_fail_next = Some(-25204);
        let err = run_frame_transaction(&mut ops, 4242, 7, pre, att).unwrap_err();
        let f = tx_failure(err);
        assert_eq!(f.stage, "readback");
        assert_eq!(f.applied, None);
        assert_eq!(ops.restore_writes, 1);
        assert_eq!(f.restore_readback, Some(pre));
        assert_eq!(ops.current, pre);
    }

    #[test]
    fn tx_restore_write_fail_is_strict_failure_with_evidence() {
        let pre = preimage();
        let att = attempted();
        // Fail the restore write specifically: first set (attempted) must
        // succeed, so a wrapper fails only the preimage (restore) write.
        let mut ops = FailRestoreTx {
            inner: FakeTx::new(pre),
            failed: false,
        };
        let err = run_frame_transaction(&mut ops, 4242, 7, pre, att).unwrap_err();
        let f = tx_failure(err);
        assert_eq!(f.stage, "restore-write");
        assert_eq!(f.preimage, pre);
        assert_eq!(f.attempted, att);
        assert_eq!(f.applied, Some(att));
        assert!(f.error.contains("restore write failed"));
        // The primary error text and the structured restore field must agree:
        // the restore failure is retained explicitly, never nulled.
        assert_eq!(f.restore_error, Some(f.error.clone()));
        assert!(
            f.restore_error
                .as_deref()
                .is_some_and(|e| e.contains("restore write failed"))
        );
        assert!(f.restore_refused.is_none());
        // CLI Display status surfaces the same restore failure.
        let text = AxError::FrameTx(Box::new(f)).to_string();
        assert!(text.contains("restore-write"));
        assert!(text.contains("restore write failed"));
    }

    struct FailRestoreTx {
        inner: FakeTx,
        failed: bool,
    }

    impl FrameTxOps for FailRestoreTx {
        fn set_frame(&mut self, frame: &crate::ffi::AxFrame) -> Result<(), i32> {
            if *frame == self.inner.pre && !self.failed {
                self.failed = true;
                self.inner.set_calls += 1;
                self.inner.restore_writes += 1;
                return Err(-25204);
            }
            self.inner.set_frame(frame)
        }

        fn get_frame(&mut self) -> Result<crate::ffi::AxFrame, i32> {
            self.inner.get_frame()
        }

        fn revalidate(&mut self) -> Result<(), String> {
            self.inner.revalidate()
        }
    }

    #[test]
    fn tx_restore_mismatch_is_strict_failure_not_accepted() {
        // The API reports success for the restore write but the window never
        // moves back (lying API / clamped geometry). Acceptance alone must
        // NOT count as restored: exact-equality readback mismatch is a
        // strict failure with the evidence retained.
        let pre = preimage();
        let att = attempted();
        let mut ops = FakeTx::new(pre);
        ops.restore_ignored = true;
        let err = run_frame_transaction(&mut ops, 4242, 7, pre, att).unwrap_err();
        let f = tx_failure(err);
        assert_eq!(f.stage, "restore-verify");
        assert_eq!(f.applied, Some(att));
        assert_eq!(f.restore_readback, Some(att));
        assert!(!f.restored);
        assert!(f.error.contains("MISMATCH"));
    }

    #[test]
    fn tx_pre_write_ownership_loss_blocks_set_without_restore() {
        let pre = preimage();
        let att = attempted();
        let mut ops = FakeTx::new(pre);
        ops.revalidate_fail_from = Some(1);
        let err = run_frame_transaction(&mut ops, 4242, 7, pre, att).unwrap_err();
        let f = tx_failure(err);
        assert_eq!(f.stage, "ownership");
        assert_eq!(f.preimage, pre);
        assert_eq!(f.attempted, att);
        assert_eq!(ops.set_calls, 0, "no write may precede ownership proof");
        assert_eq!(ops.restore_writes, 0, "nothing written: no restore to fake");
        assert!(f.restore_refused.is_some());
    }

    #[test]
    fn tx_post_write_ownership_loss_refuses_restore_write() {
        let pre = preimage();
        let att = attempted();
        let mut ops = FakeTx::new(pre);
        // Revalidations: 1 = pre-write, 2 = post-set, 3 = pre-restore.
        // Fail from the 3rd: the set already happened, the restore must be
        // refused (no unsafe write to a possibly-reused window).
        ops.revalidate_fail_from = Some(3);
        let err = run_frame_transaction(&mut ops, 4242, 7, pre, att).unwrap_err();
        let f = tx_failure(err);
        assert_eq!(f.stage, "ownership");
        assert_eq!(f.preimage, pre);
        assert_eq!(f.attempted, att);
        assert_eq!(ops.set_calls, 1, "the attempted write happened");
        assert_eq!(ops.restore_writes, 0, "stale ownership must refuse restore");
        assert!(f.restore_refused.is_some());
        // The error text retains the structured evidence as JSON.
        let text = AxError::FrameTx(Box::new(f)).to_string();
        assert!(text.contains("preimage"));
    }
}

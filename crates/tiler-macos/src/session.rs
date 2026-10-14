#![forbid(unsafe_code)]

//! Single bounded CLI owned-session Space probe (Child-holder).
//!
//! This module holds the exact `std::process::Child` returned by
//! [`crate::probe_target::spawn_verified`] for the whole session. Identity
//! here is the live Child handle (no PID-reuse window while un-reaped) plus
//! the pipe-proven nonce plus the live executable binding plus fresh CG
//! owner-PID joins — never cross-command title proof for holder operations.
//! Every spawned Child is owned by [`ChildGuard`]: signals go only to a
//! liveness-checked held handle (never a raw PID after reap), `wait()` runs
//! only after an observed exit (never blocks), and the spawn record is
//! removed only after a verified exit. `Drop` is a silent best-effort
//! backstop so no error path leaks a zombie, orphan, or blindly deleted
//! record.
//!
//! This command performs reads and lifecycle proofs only. No Space move,
//! switch, focus change, or synthetic event is dispatched here: independent
//! restore is not yet proven, so `--allow-untested` does not authorize
//! mutation. Mission Control setup and gesture fallback prerequisites are
//! observed through [`AutomationGate`]; neither route is attempted here,
//! and no SIP conclusion is drawn from a skip.
//!
//! Session order: exact-Child graceful proof, then exact-Child kill proof
//! (each on its own throwaway spawn), then the Spaces session spawn with CG
//! identity, Space-membership readback, neutral cross-command gate
//! observation, permission-gate observation, topology note, layout check,
//! and verified Child cleanup. No leftover helper, tap, or Space on any path.

use serde::Serialize;

use crate::probe_target::{ProbeTargetSpec, SpawnSession, mint_nonce, spawn_verified};

/// Lifecycle proof for one throwaway spawn (exact Child handle only).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LifecycleProof {
    /// `graceful-sigterm` or `kill`.
    pub how: String,
    pub exited: bool,
    pub reaped: bool,
    pub record_removed: bool,
    pub process_gone: bool,
}

/// Owned CG window summary. Titles deliberately excluded: only IDs, layer,
/// on-screen flag, and geometry are evidence.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CgSummary {
    pub wid: u32,
    pub pid: u32,
    pub layer: i32,
    pub onscreen: bool,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// Sanitized Space layout: IDs and current Space only, no display UUIDs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SpaceLayout {
    pub connection: Option<i32>,
    pub space_ids: Vec<u64>,
    pub current: Vec<u64>,
    pub space_count: usize,
}

/// Observed permission gate for UI automation (Mission
/// Control temporary-Space setup) and native focus/switch gestures. All
/// reads are non-prompting and non-mutating: `AXIsProcessTrusted` (never the
/// `WithOptions` prompting variant) plus `CGPreflightPostEventAccess`
/// (preflight only, never request/post) plus Secure Input observation.
/// `post_event_access` is the window server's own answer for synthetic
/// posting: `Some(true/false)` on macOS via the maintained
/// `objc2-core-graphics` binding, `None` off-macOS (stub reports no state).
/// Posted session events from a process the window server refuses are
/// dropped, so event posting counts as unavailable only when the preflight
/// itself says so; AX trust alone never conclusively proves all automation
/// routes impossible. This probe posts zero synthetic events on every path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AutomationGate {
    pub ax_trusted: Option<bool>,
    pub post_event_access: Option<bool>,
    pub secure_input: String,
    pub mission_control_setup: String,
    pub focus_gesture: String,
    pub synthetic_events_posted: bool,
}

/// How the session Child was stopped. `cg_residue` is `None` when the CG
/// snapshot itself is unavailable (unknown — a clean status is never claimed
/// on unknown diagnostics).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SessionStop {
    pub pid: u32,
    pub graceful: bool,
    pub kill_fallback_used: bool,
    pub reaped: bool,
    pub record_removed: bool,
    pub process_gone: bool,
    pub cg_residue: Option<bool>,
}

/// Structured error payload for every spawned-child failure path: the stage
/// and cause plus the exact-Child cleanup outcome. `pid`/`stop` are present
/// exactly when a child was held and cleaned; a failure with no spawned
/// child carries `None` (absent, never a fabricated PID). `layout_after` is
/// a final read-only readback when available; `membership_after` is always
/// `None` after cleanup — membership is no longer readable then, and absence
/// is reported rather than fabricated.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SessionErrorReport {
    pub stage: String,
    pub error: String,
    pub pid: Option<u32>,
    pub stop: Option<SessionStop>,
    pub layout_after: Option<SpaceLayout>,
    pub membership_after: Option<Vec<u64>>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SpacesSessionReport {
    pub bundle: String,
    pub graceful_proof: LifecycleProof,
    pub kill_proof: LifecycleProof,
    pub session_pid: u32,
    pub cg_windows: Vec<CgSummary>,
    pub layout_before: SpaceLayout,
    pub membership_preimage: Vec<u64>,
    /// Neutral cross-command gate observation while the Child was alive:
    /// `authenticated: ...` where a live title exists, `refused: ...`
    /// where titles are withheld. Holder operations never depend on it.
    pub cross_command_gate: String,
    pub automation_gate: AutomationGate,
    /// Move-symbol presence diagnostics only. Nothing is dispatched through
    /// these symbols in this command.
    pub move_symbols: crate::ffi::SlsMoveSymbols,
    pub topology_note: String,
    /// Always set: mutation needs proven independent restore.
    pub mutation_skipped: String,
    pub layout_after: SpaceLayout,
    pub membership_after: Vec<u64>,
    pub layout_restored: bool,
    pub stop: SessionStop,
}

fn sanitize_layout(d: &crate::spaces::SpacesDiagnosis) -> SpaceLayout {
    let mut space_ids: Vec<u64> = d
        .displays
        .iter()
        .flat_map(|x| x.spaces.iter().copied())
        .collect();
    space_ids.sort_unstable();
    space_ids.dedup();
    let mut current: Vec<u64> = d.displays.iter().filter_map(|x| x.current).collect();
    current.sort_unstable();
    current.dedup();
    SpaceLayout {
        connection: d.connection,
        space_count: d.space_count(),
        space_ids,
        current,
    }
}

fn summarize(w: &crate::ffi::CgWindow) -> CgSummary {
    CgSummary {
        wid: w.wid,
        pid: w.pid,
        layer: w.layer,
        onscreen: w.onscreen,
        x: w.bounds.x,
        y: w.bounds.y,
        w: w.bounds.w,
        h: w.bounds.h,
    }
}

/// Bounded poll for the exact Child to exit. Returns true only on an
/// observed exit. Never blocks past `timeout_ms`.
fn poll_exit(child: &mut std::process::Child, timeout_ms: u64) -> bool {
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return true,
            Ok(None) => {
                if std::time::Instant::now() >= deadline {
                    return matches!(child.try_wait(), Ok(Some(_)));
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            Err(_) => return false,
        }
    }
}

/// Three-way interpretation of one `try_wait` observation. `Exited` is the
/// only verified exit; `Unknown` (a wait error) is never an exit by itself
/// and needs a bounded poll to observe one — it must neither delete the
/// record nor report success.
#[derive(Debug, Clone, PartialEq, Eq)]
enum WaitState {
    Live,
    Exited,
    Unknown(String),
}

/// Classify one `try_wait` result without side effects. Pure: scripted
/// `Err` values exercise the unknown path with no OS failure needed.
fn classify_wait(result: &std::io::Result<Option<std::process::ExitStatus>>) -> WaitState {
    match result {
        Ok(Some(_)) => WaitState::Exited,
        Ok(None) => WaitState::Live,
        Err(e) => WaitState::Unknown(format!("owned child wait failed: {e}")),
    }
}

/// Exit is verified ONLY by an observed `Ok(Some)` status or a bounded poll
/// that saw the exit. An unknown wait state counts only with a poll-observed
/// exit; the error itself is never evidence of exit.
fn verified_exit(state: &WaitState, poll_observed: bool) -> bool {
    match state {
        WaitState::Exited => true,
        WaitState::Live | WaitState::Unknown(_) => poll_observed,
    }
}

/// Reap only after an observed exit. Returns true when the Child was
/// reaped. Never blocks: a live Child returns false immediately instead of
/// entering the unbounded `wait()`.
fn reap_if_exited(child: &mut std::process::Child) -> bool {
    match child.try_wait() {
        Ok(Some(_)) => child.wait().is_ok(),
        _ => false,
    }
}

/// Remove the spawn record only after a verified exit. An unverified exit
/// retains the record for inspection instead of blindly deleting evidence.
fn remove_record_if_exited(pid: u32, exited: bool) -> bool {
    if !exited {
        return false;
    }
    let _ = std::fs::remove_file(crate::probe_target::session_record_path(pid));
    !crate::probe_target::session_record_path(pid).exists()
}

/// Outcome of one exact-Child shutdown. `record_removed` is true only when
/// the exit was verified first. `wait_error` retains an unknown wait
/// observation (a `try_wait` error): such a shutdown never reports `exited`
/// unless a bounded poll observed the exit afterwards.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Shutdown {
    signaled_graceful: bool,
    exited: bool,
    reaped: bool,
    kill_fallback_used: bool,
    record_removed: bool,
    process_gone: bool,
    wait_error: Option<String>,
}

/// Exact-Child owner. Signals go only to the held handle after a liveness
/// check (never a raw PID to an exited/reaped handle), `wait()` runs only
/// after an observed exit, and the spawn record is removed only after a
/// verified exit. `Drop` repeats the best-effort resolution silently so a
/// forgotten path still cannot leak a zombie or orphan; a retained record
/// on unverified exit stays visible for inspection.
struct ChildGuard {
    session: Option<SpawnSession>,
    finished: bool,
}

impl ChildGuard {
    fn launch(seconds: u64) -> Result<Self, String> {
        let nonce = mint_nonce()?;
        let spec = ProbeTargetSpec::new(&nonce, 0, 0, seconds)?;
        Ok(Self {
            session: Some(spawn_verified(&spec)?),
            finished: false,
        })
    }

    fn pid(&self) -> u32 {
        self.session.as_ref().map(|s| s.pid).unwrap_or(0)
    }

    /// Liveness plus live executable binding. No title involved: the held
    /// handle — not a typed PID — is the currency.
    fn require_live(&mut self) -> Result<(), String> {
        let session = self
            .session
            .as_mut()
            .ok_or_else(|| "child handle already resolved".to_owned())?;
        match session
            .child
            .try_wait()
            .map_err(|e| format!("owned child wait failed: {e}"))?
        {
            None => crate::ownership::verify_live_process(session.pid, Some(&session.helper_path))
                .map(|_| ())
                .map_err(|e| format!("session ownership: {e}")),
            Some(status) => Err(format!("owned child exited early: {status}")),
        }
    }

    /// Core shutdown. When `graceful_first`, SIGTERM goes only to a
    /// liveness-checked held handle; an already-exited Child skips the
    /// signal entirely and goes straight to reap. Kill fallback uses the
    /// exact handle. Bounded throughout; `wait()` only after observed exit.
    /// A wait error (`Unknown`) is never treated as an exit: the error is
    /// retained, resolution falls back to bounded exact-handle kill plus
    /// poll, and the record is removed only if that poll observes the exit.
    fn shutdown(&mut self, graceful_first: bool) -> Shutdown {
        let mut out = Shutdown {
            signaled_graceful: false,
            exited: false,
            reaped: false,
            kill_fallback_used: false,
            record_removed: false,
            process_gone: false,
            wait_error: None,
        };
        let Some(session) = self.session.as_mut() else {
            self.finished = true;
            return out;
        };
        let pid = session.pid;
        // Three-way state check before any signal: a verified exit skips
        // every signal and goes straight to reap; only a live handle is
        // signalled (never a raw PID to an exited/reaped handle); an
        // unknown wait observation resolves via bounded exact-handle kill
        // plus poll below, never by assuming the error means exit.
        let state = classify_wait(&session.child.try_wait());
        match &state {
            WaitState::Exited => {
                out.exited = true;
            }
            WaitState::Live => {
                if graceful_first {
                    if crate::ffi::terminate_process(pid as i32) {
                        out.signaled_graceful = true;
                    }
                    out.exited = poll_exit(&mut session.child, 5_000);
                } else {
                    let _ = session.child.kill();
                    out.kill_fallback_used = true;
                    out.exited = poll_exit(&mut session.child, 5_000);
                }
            }
            WaitState::Unknown(err) => {
                out.wait_error = Some(err.clone());
                let _ = session.child.kill();
                out.kill_fallback_used = true;
                out.exited = verified_exit(&state, poll_exit(&mut session.child, 5_000));
            }
        }
        if !out.exited {
            // Graceful path failed: exact-handle kill fallback, still bounded.
            out.kill_fallback_used = true;
            let _ = session.child.kill();
            out.exited = poll_exit(&mut session.child, 5_000);
        }
        out.reaped = reap_if_exited(&mut session.child);
        out.record_removed = remove_record_if_exited(pid, out.exited);
        out.process_gone = !crate::ffi::process_exists(pid as i32);
        self.finished = true;
        out
    }

    /// Error-path resolution: no graceful attempt, exact-handle kill plus
    /// bounded reap, record removed only on verified exit.
    fn resolve_after_error(&mut self) -> Shutdown {
        self.shutdown(false)
    }

    fn shutdown_graceful(&mut self) -> Shutdown {
        self.shutdown(true)
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        // Silent backstop: never a zombie/orphan from a forgotten path.
        // Record stays when the exit is unverified (visible, not deleted).
        let _ = self.shutdown(false);
    }
}

fn spawn_ready_guard(seconds: u64) -> Result<ChildGuard, String> {
    ChildGuard::launch(seconds)
}

/// Retained wait-state evidence for structured errors: an unknown wait
/// observation rides along with the cause instead of being dropped.
fn wait_suffix(shutdown: &Shutdown) -> String {
    shutdown
        .wait_error
        .as_deref()
        .map(|e| format!("; wait state: {e}"))
        .unwrap_or_default()
}

/// Outcome of stopping one exact held Child handle. Public so holder paths
/// outside this module (e.g. `probe-spawn`) reuse the same bounded
/// exact-Child cleanup instead of hand-rolled kill/wait sequences.
/// `wait_error` retains an unknown wait observation: such a stop never
/// reports `exited` unless a bounded poll observed the exit afterwards.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChildStopOutcome {
    pub exited: bool,
    pub kill_attempted: bool,
    pub reaped: bool,
    pub record_removed: bool,
    pub wait_error: Option<String>,
}

/// Stop one exact held Child handle: check Child state first (an
/// already-exited handle is never signalled, and control never falls back
/// to a PID guess), kill the exact handle when live, poll exit bounded,
/// reap only after an observed exit (never an unbounded wait on a live
/// child), and remove the spawn record only on a verified exit (an
/// unverified exit retains the record for inspection). A wait error is
/// retained as evidence and never counts as an exit by itself.
pub fn stop_exact_child(child: &mut std::process::Child, pid: u32) -> ChildStopOutcome {
    let mut out = ChildStopOutcome {
        exited: false,
        kill_attempted: false,
        reaped: false,
        record_removed: false,
        wait_error: None,
    };
    let state = classify_wait(&child.try_wait());
    match &state {
        WaitState::Exited => out.exited = true,
        WaitState::Live => {
            out.kill_attempted = child.kill().is_ok();
            out.exited = poll_exit(child, 5_000);
        }
        WaitState::Unknown(err) => {
            out.wait_error = Some(err.clone());
            out.kill_attempted = child.kill().is_ok();
            out.exited = verified_exit(&state, poll_exit(child, 5_000));
        }
    }
    // `reap_if_exited` never blocks: a live Child reports false immediately.
    out.reaped = reap_if_exited(child);
    out.record_removed = remove_record_if_exited(pid, out.exited);
    out
}

/// Map a CG snapshot result to residue evidence: `None` when the snapshot
/// itself is unavailable (unknown — never claimed clean), `Some(present)`
/// otherwise.
fn cg_residue_of(list: Result<Vec<crate::ffi::CgWindow>, String>, pid: u32) -> Option<bool> {
    list.ok().map(|l| l.iter().any(|w| w.pid == pid))
}

/// Summarize one exact-Child shutdown for reports: the held PID plus the
/// verified cleanup fields plus fresh CG residue evidence (`None` when the
/// snapshot is unavailable — unknown, never claimed clean).
fn stop_summary(pid: u32, shutdown: &Shutdown) -> SessionStop {
    SessionStop {
        pid,
        graceful: shutdown.signaled_graceful && shutdown.exited && !shutdown.kill_fallback_used,
        kill_fallback_used: shutdown.kill_fallback_used,
        reaped: shutdown.reaped,
        record_removed: shutdown.record_removed,
        process_gone: shutdown.process_gone,
        cg_residue: cg_residue_of(crate::ffi::cg_window_list_all(), pid),
    }
}

/// Final read-only layout readback for error payloads. `None` when the
/// readback itself is unavailable — reported absent, never fabricated.
fn final_layout() -> Option<SpaceLayout> {
    crate::spaces::diagnose().ok().map(|d| sanitize_layout(&d))
}

/// Encode one structured session error. Every spawned-child failure path
/// returns this JSON (never a bare string), so cleanup evidence always
/// accompanies the cause.
fn session_error_json(report: SessionErrorReport) -> (i32, String) {
    let body = serde_json::to_string(&report).unwrap_or_else(|e| {
        format!(
            "{{\"stage\":\"{}\",\"error\":\"encode: {e}\"}}",
            report.stage
        )
    });
    (1, body)
}

/// Error-path resolution for a held guard: exact-handle kill plus bounded
/// reap (record removed only on verified exit), then the structured payload
/// with the final layout readback when available and no fabricated
/// membership.
fn cleanup_error(guard: &mut ChildGuard, stage: &str, msg: String) -> (i32, String) {
    let pid = guard.pid();
    let shutdown = guard.resolve_after_error();
    if pid == 0 {
        return session_error_json(SessionErrorReport {
            stage: stage.to_owned(),
            error: msg,
            pid: None,
            stop: None,
            layout_after: final_layout(),
            membership_after: None,
        });
    }
    session_error_json(SessionErrorReport {
        stage: stage.to_owned(),
        error: msg,
        pid: Some(pid),
        stop: Some(stop_summary(pid, &shutdown)),
        layout_after: final_layout(),
        membership_after: None,
    })
}

fn prove_lifecycle(how: &str, seconds: u64) -> Result<LifecycleProof, String> {
    let stage = format!("lifecycle-proof-{how}");
    let spawn_error = |error: String| -> String {
        // No child was spawned/held: structured error with no PID and no
        // cleanup to report (absent, never invented).
        serde_json::to_string(&SessionErrorReport {
            stage: stage.clone(),
            error,
            pid: None,
            stop: None,
            layout_after: final_layout(),
            membership_after: None,
        })
        .unwrap_or_else(|_| "session spawn failed".to_owned())
    };
    let mut guard = match spawn_ready_guard(seconds) {
        Ok(g) => g,
        Err(e) => return Err(spawn_error(format!("session spawn: {e}"))),
    };
    if let Err(e) = guard.require_live() {
        let shutdown = guard.resolve_after_error();
        let pid = guard.pid();
        let report = SessionErrorReport {
            stage: stage.clone(),
            error: format!("pre-stop liveness failed ({e}){}", wait_suffix(&shutdown)),
            pid: if pid == 0 { None } else { Some(pid) },
            stop: if pid == 0 {
                None
            } else {
                Some(stop_summary(pid, &shutdown))
            },
            layout_after: final_layout(),
            membership_after: None,
        };
        return Err(serde_json::to_string(&report)
            .unwrap_or_else(|_| "pre-stop liveness failed".to_owned()));
    }
    let s = if how == "graceful-sigterm" {
        guard.shutdown_graceful()
    } else {
        guard.resolve_after_error()
    };
    if !s.exited {
        let pid = guard.pid();
        let report = SessionErrorReport {
            stage,
            error: format!(
                "exact child ({how}) did not exit bounded; record retained for inspection{}",
                wait_suffix(&s)
            ),
            pid: if pid == 0 { None } else { Some(pid) },
            stop: if pid == 0 {
                None
            } else {
                Some(stop_summary(pid, &s))
            },
            layout_after: final_layout(),
            membership_after: None,
        };
        return Err(serde_json::to_string(&report)
            .unwrap_or_else(|_| "exact child did not exit bounded".to_owned()));
    }
    Ok(LifecycleProof {
        how: how.to_owned(),
        exited: s.exited,
        reaped: s.reaped,
        record_removed: s.record_removed,
        process_gone: s.process_gone,
    })
}

/// Permission-gate observation for Mission Control setup and focus/switch
/// gestures. Reads only; posts nothing. This read-only command needs no
/// temporary Space and reports the prerequisites without changing topology.
fn observe_automation_gate() -> AutomationGate {
    let ax_trusted = crate::ffi::ax_is_process_trusted();
    let post_event_access = crate::ffi::cg_post_event_access_known();
    let secure_input = format!("{:?}", crate::input::secure_input());
    // Synthetic posting is unavailable only when the window server's own
    // preflight says so; an unknown preflight (missing symbol) is reported
    // as unknown, never as proof that posting is impossible.
    let posting = match post_event_access {
        Some(true) => "preflight true",
        Some(false) => "preflight false (window server would drop posted events)",
        None => {
            "preflight unknown (off-macOS stub reports no state; posting availability unproven)"
        }
    };
    let (mission_control_setup, focus_gesture) = match ax_trusted {
        Some(true) => (
            format!(
                "skipped: AX trust observed true, but this command dispatches no mutation (independent restore unproven); synthetic posting {posting}; no UI touched, no event posted"
            ),
            format!(
                "skipped: AX trust observed true, but native recovery is unproven and this command changes no focus; synthetic posting {posting}; no event posted"
            ),
        ),
        _ => (
            format!(
                "skipped: temporary-Space creation needs Mission Control UI automation or synthetic input; AX trust observed {ax_trusted:?}, synthetic posting {posting}, Secure Input {secure_input}; no synthetic event posted, nothing attempted, no SIP conclusion"
            ),
            format!(
                "skipped: native focus/switch gesture needs synthetic Dock-swipe with cursor warp/click; AX trust observed {ax_trusted:?}, synthetic posting {posting}, Secure Input {secure_input}; focus unchanged, no event posted, no SIP conclusion"
            ),
        ),
    };
    AutomationGate {
        ax_trusted,
        post_event_access,
        secure_input,
        mission_control_setup,
        focus_gesture,
        synthetic_events_posted: false,
    }
}

/// Single bounded owned-session Space probe. Owns the exact Child end to
/// end; reads and lifecycle proofs only — no Space mutation in this command.
pub fn run_spaces_session(seconds: u64, allow_untested: bool) -> Result<String, (i32, String)> {
    crate::config::ProbeConfig::validate_probe_seconds(seconds).map_err(|e| (2, e))?;

    // Lifecycle proofs first, each on its own throwaway spawn. Failures are
    // already structured JSON (stage + exact-Child cleanup); pass through.
    let graceful_proof = prove_lifecycle("graceful-sigterm", 10).map_err(|e| (1, e))?;
    let kill_proof = prove_lifecycle("kill", 10).map_err(|e| (1, e))?;

    // Spaces session spawn: the Child stays held until verified stop. A
    // spawn failure holds no child: structured error with no PID (absent,
    // never invented) and no cleanup to report.
    let mut guard = match spawn_ready_guard(seconds) {
        Ok(g) => g,
        Err(e) => {
            return Err(session_error_json(SessionErrorReport {
                stage: "session-spawn".to_owned(),
                error: format!("session spawn: {e}"),
                pid: None,
                stop: None,
                layout_after: final_layout(),
                membership_after: None,
            }));
        }
    };
    let cleanup_err = |guard: &mut ChildGuard, stage: &str, msg: String| -> (i32, String) {
        cleanup_error(guard, stage, msg)
    };

    // Child-held CG identity: liveness + exe binding + fresh owner-PID join.
    // No title check anywhere on this path by design.
    if let Err(e) = guard.require_live() {
        return Err(cleanup_err(&mut guard, "session-liveness", e));
    }
    let pid = guard.pid();
    let live: Vec<crate::ffi::CgWindow> = {
        let session = guard.session.as_ref().expect("guard holds session");
        match crate::ffi::cg_window_list_all() {
            Ok(list) => list.into_iter().filter(|w| w.pid == session.pid).collect(),
            Err(e) => {
                return Err(cleanup_err(
                    &mut guard,
                    "cg-snapshot",
                    format!("CG snapshot: {e}"),
                ));
            }
        }
    };
    if live.is_empty() {
        return Err(cleanup_err(
            &mut guard,
            "cg-identity",
            format!("no live CG window for pid {pid} (unprovable holder identity); refusing"),
        ));
    }
    let wid = live[0].wid;
    if let Err(e) = crate::cg::verify_wid_owned_by_pid(pid, wid)
        .map(|_| ())
        .map_err(|e| e.to_string())
    {
        return Err(cleanup_err(&mut guard, "cg-join", format!("CG join: {e}")));
    }
    let cg_windows: Vec<CgSummary> = live.iter().map(summarize).collect();

    let layout_before = sanitize_layout(
        &crate::spaces::diagnose()
            .map_err(|e| cleanup_err(&mut guard, "layout-before", e.to_string()))?,
    );
    let membership_preimage = crate::spaces::window_spaces(wid)
        .map_err(|e| cleanup_err(&mut guard, "membership-preimage", e.to_string()))?;
    if membership_preimage.len() != 1 {
        return Err(cleanup_err(
            &mut guard,
            "membership-preimage",
            format!(
                "session window membership is not a single restorable Space ({} reported)",
                membership_preimage.len()
            ),
        ));
    }

    // Neutral cross-command gate observation while the Child is held. A pass
    // (live title available on other sessions) is valid and reported
    // neutrally; a refusal (titles withheld here) is reported neutrally too.
    // Holder operations never depend on this result.
    let cross_command_gate = {
        let session = guard.session.as_ref().expect("guard holds session");
        match crate::ax::authorize_owned_action(session.pid, &session.nonce) {
            Ok(_) => "authenticated: live title available on this session".to_owned(),
            Err(e) => {
                let mut s = e.to_string();
                s.truncate(160);
                format!("refused: {s}")
            }
        }
    };

    let automation_gate = observe_automation_gate();
    let move_symbols = crate::ffi::sls_move_symbols();
    let topology_note = format!(
        "{} Space(s) known ({}); current {:?}; direct symbol present={}; bridged-async present={}",
        layout_before.space_ids.len(),
        layout_before
            .space_ids
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
            .join(","),
        layout_before.current,
        move_symbols.direct,
        move_symbols.bridged_async,
    );
    // No mutation regardless of opt-in or topology: same-holder
    // revalidation is not independent recovery (it shares the process and
    // thread and cannot survive the primary's exit), and no independently
    // demonstrated restore exists yet. `--allow-untested` (given: {allow_untested})
    // does not bypass the live-guide recovery gate.
    let mutation_skipped = format!(
        "move/restore not attempted: independent restore not yet proven (same-holder revalidation is not independent recovery); --allow-untested={allow_untested} does not bypass the recovery gate"
    );

    let layout_after = sanitize_layout(
        &crate::spaces::diagnose()
            .map_err(|e| cleanup_err(&mut guard, "layout-after", e.to_string()))?,
    );
    let membership_after = crate::spaces::window_spaces(wid)
        .map_err(|e| cleanup_err(&mut guard, "membership-after", e.to_string()))?;
    let layout_restored = layout_after.space_ids == layout_before.space_ids
        && layout_after.current == layout_before.current
        && membership_after == membership_preimage;
    let session_pid = guard.pid();
    let shutdown = guard.shutdown_graceful();
    let stop = SessionStop {
        pid: session_pid,
        graceful: shutdown.signaled_graceful && shutdown.exited && !shutdown.kill_fallback_used,
        kill_fallback_used: shutdown.kill_fallback_used,
        reaped: shutdown.reaped,
        record_removed: shutdown.record_removed,
        process_gone: shutdown.process_gone,
        cg_residue: cg_residue_of(crate::ffi::cg_window_list_all(), session_pid),
    };

    let report = SpacesSessionReport {
        bundle: crate::BUNDLE_ID.to_owned(),
        graceful_proof,
        kill_proof,
        session_pid,
        cg_windows,
        layout_before,
        membership_preimage,
        cross_command_gate,
        automation_gate,
        move_symbols,
        topology_note,
        mutation_skipped,
        layout_after,
        membership_after,
        layout_restored,
        stop,
    };
    serde_json::to_string(&report).map_err(|e| (1, e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_sanitizer_drops_uuids_and_sorts() {
        let d = crate::spaces::SpacesDiagnosis {
            connection: Some(7),
            displays: vec![crate::ffi::SlsDisplay {
                uuid: Some("some-uuid".to_owned()),
                spaces: vec![3, 1],
                current: Some(3),
            }],
        };
        let layout = sanitize_layout(&d);
        assert_eq!(layout.connection, Some(7));
        assert_eq!(layout.space_ids, vec![1, 3]);
        assert_eq!(layout.current, vec![3]);
        assert_eq!(layout.space_count, 2);
        let json = serde_json::to_string(&layout).unwrap();
        assert!(!json.contains("some-uuid"), "UUIDs must not leak");
    }

    #[test]
    fn summary_excludes_titles() {
        let w = crate::ffi::CgWindow {
            wid: 9,
            pid: 4242,
            layer: 0,
            onscreen: true,
            bounds: crate::ffi::CgBounds {
                x: 1.0,
                y: 2.0,
                w: 3.0,
                h: 4.0,
            },
            title: Some("secret title".to_owned()),
        };
        let s = summarize(&w);
        assert_eq!(s.wid, 9);
        let json = serde_json::to_string(&s).unwrap();
        assert!(!json.contains("secret"), "titles must not leak");
    }

    #[test]
    fn invalid_seconds_rejected_before_spawn() {
        assert!(run_spaces_session(0, false).is_err());
        assert!(run_spaces_session(crate::config::MAX_PROBE_SECONDS + 1, false).is_err());
    }

    /// Bounded poll returns fast for a live child instead of waiting it out.
    #[test]
    fn bounded_poll_returns_fast_for_live_child() {
        let mut child = std::process::Command::new("sleep")
            .arg("30")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("sleep helper for poll test");
        let start = std::time::Instant::now();
        assert!(!poll_exit(&mut child, 200));
        assert!(
            start.elapsed() < std::time::Duration::from_secs(10),
            "poll must stay bounded, took {:?}",
            start.elapsed()
        );
        child.kill().expect("kill test sleeper");
        assert!(poll_exit(&mut child, 5_000));
        assert!(reap_if_exited(&mut child));
    }

    /// Reap never blocks for a live child: it reports false immediately
    /// instead of entering the unbounded `wait()`.
    #[test]
    fn reap_never_blocks_for_live_child() {
        let mut child = std::process::Command::new("sleep")
            .arg("30")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("sleep helper for reap test");
        let start = std::time::Instant::now();
        assert!(!reap_if_exited(&mut child));
        assert!(
            start.elapsed() < std::time::Duration::from_secs(10),
            "reap must not block, took {:?}",
            start.elapsed()
        );
        child.kill().expect("kill test sleeper");
        assert!(poll_exit(&mut child, 5_000));
        assert!(reap_if_exited(&mut child));
    }

    /// Guard shutdown on an already-exited child skips every signal, still
    /// reaps, and only then clears the record.
    #[test]
    fn shutdown_skips_signal_for_dead_child() {
        let mut child = std::process::Command::new("true")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("true helper for dead-child test");
        assert!(poll_exit(&mut child, 5_000));
        let pid = child.id();
        let mut guard = ChildGuard {
            session: Some(SpawnSession {
                child,
                pid,
                nonce: "0123456789abcdef0123456789abcdef".to_owned(),
                helper_path: "/nonexistent-helper".to_owned(),
            }),
            finished: false,
        };
        let s = guard.shutdown_graceful();
        assert!(s.exited);
        assert!(s.reaped);
        assert!(!s.signaled_graceful, "no signal to an exited handle");
        assert!(!s.kill_fallback_used);
        assert!(s.record_removed, "record cleared after verified exit");
    }

    /// Record removal is gated on verified exit: an unverified exit retains
    /// the record for inspection instead of blindly deleting it.
    #[test]
    fn record_removal_gated_on_verified_exit() {
        let pid = u32::MAX - 9999;
        let path = crate::probe_target::session_record_path(pid);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("session dir for test");
        }
        std::fs::write(&path, "{\"test\":true}").expect("stale record for test");
        assert!(!remove_record_if_exited(pid, false));
        assert!(path.exists(), "unverified exit must retain the record");
        assert!(remove_record_if_exited(pid, true));
        assert!(!path.exists());
    }

    /// Forced owned-child early failure without AppKit: a pure `true` child
    /// exits immediately, so liveness fails exactly like an early-exiting
    /// helper. The error path must clean the exact Child (bounded kill +
    /// reap, record removed only on the verified exit) and emit structured
    /// error JSON — stage/error plus the held PID and the full SessionStop
    /// (pid, reaped, record_removed, process_gone, cg_residue) plus the
    /// final layout readback when available. Membership after cleanup is no
    /// longer readable and must be absent (null), never fabricated.
    #[test]
    fn early_child_failure_emits_structured_cleanup_json() {
        let mut child = std::process::Command::new("true")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("true helper for early-failure test");
        assert!(poll_exit(&mut child, 5_000));
        let pid = child.id();
        assert_ne!(pid, 0, "test seam must hold a real child PID");
        let mut guard = ChildGuard {
            session: Some(SpawnSession {
                child,
                pid,
                nonce: "0123456789abcdef0123456789abcdef".to_owned(),
                helper_path: "/nonexistent-helper".to_owned(),
            }),
            finished: false,
        };
        let liveness = guard
            .require_live()
            .expect_err("exited child must fail liveness");
        let (code, body) = cleanup_error(&mut guard, "lifecycle-proof-kill", liveness);
        assert_eq!(code, 1);
        let v: serde_json::Value = serde_json::from_str(&body).expect("error path must emit JSON");
        assert_eq!(v["stage"], "lifecycle-proof-kill");
        assert!(
            v["error"]
                .as_str()
                .is_some_and(|e| e.contains("exited early")),
            "unexpected error: {body}"
        );
        assert_eq!(v["pid"], pid);
        let stop = &v["stop"];
        assert_eq!(stop["pid"], pid);
        assert_eq!(stop["reaped"], true);
        assert_eq!(stop["record_removed"], true);
        assert_eq!(stop["process_gone"], true);
        assert!(
            stop.get("cg_residue").is_some(),
            "cg_residue must be reported"
        );
        assert!(
            v.get("membership_after").is_none() || v["membership_after"].is_null(),
            "membership after cleanup must be absent, never fabricated"
        );
        assert!(
            v.get("layout_after").is_some(),
            "layout readback slot must be present (value or null)"
        );
        assert!(
            !body.contains("0123456789abcdef0123456789abcdef"),
            "nonces must not leak into error output"
        );
    }

    /// No spawned child, no invented PID: a spawn failure carries an absent
    /// PID and no cleanup block.
    #[test]
    fn spawn_failure_reports_absent_pid_without_cleanup() {
        let (code, body) = session_error_json(SessionErrorReport {
            stage: "session-spawn".to_owned(),
            error: "session spawn: probe helper missing".to_owned(),
            pid: None,
            stop: None,
            layout_after: None,
            membership_after: None,
        });
        assert_eq!(code, 1);
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(v["stage"], "session-spawn");
        assert!(v["pid"].is_null());
        assert!(v["stop"].is_null());
        assert!(v["membership_after"].is_null());
    }

    /// Shared exact-Child stop: a live sleeper is killed via the exact
    /// handle (never a PID guess), reaped bounded, and its record cleared
    /// only on the verified exit.
    #[test]
    fn stop_exact_child_kills_and_reaps_live_child_bounded() {
        let mut child = std::process::Command::new("sleep")
            .arg("30")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("sleep helper for exact-stop test");
        let pid = child.id();
        let start = std::time::Instant::now();
        let out = stop_exact_child(&mut child, pid);
        assert!(
            start.elapsed() < std::time::Duration::from_secs(10),
            "exact stop must stay bounded, took {:?}",
            start.elapsed()
        );
        assert!(out.kill_attempted);
        assert!(out.exited);
        assert!(out.reaped);
        assert!(out.record_removed, "verified exit clears the record");
        // A reaped handle is stable: stopping again signals nothing and
        // reports the verified exit without blocking.
        let again = stop_exact_child(&mut child, pid);
        assert!(!again.kill_attempted, "no signal to an exited handle");
        assert!(again.exited);
    }

    /// Unknown wait state is never a verified exit: a scripted `try_wait`
    /// error (no OS failure needed) classifies `Unknown`, and only a
    /// poll-observed exit afterwards counts — the error itself never deletes
    /// the record nor reports success.
    #[test]
    fn unknown_wait_state_is_never_a_verified_exit() {
        let live: std::io::Result<Option<std::process::ExitStatus>> = Ok(None);
        assert_eq!(classify_wait(&live), WaitState::Live);
        let unknown: std::io::Result<Option<std::process::ExitStatus>> =
            Err(std::io::Error::other("injected wait failure"));
        let state = classify_wait(&unknown);
        assert!(matches!(state, WaitState::Unknown(_)));
        assert!(
            !verified_exit(&state, false),
            "a wait error without a poll-observed exit must not count as exit"
        );
        assert!(verified_exit(&state, true));
        assert!(!verified_exit(&WaitState::Live, false));
        // Record gating honors the same rule: unverified exits retain.
        let pid = u32::MAX - 9998;
        assert!(!remove_record_if_exited(pid, verified_exit(&state, false)));
    }

    /// Unavailable CG diagnostics are unknown, not clean: an `Err` snapshot
    /// maps to `None` (never `Some(false)`), while a readable snapshot maps
    /// presence honestly.
    #[test]
    fn unavailable_cg_snapshot_is_unknown_not_clean() {
        assert_eq!(cg_residue_of(Err("withheld".to_owned()), 4242), None);
        let present = crate::ffi::CgWindow {
            wid: 7,
            pid: 4242,
            layer: 0,
            onscreen: true,
            bounds: crate::ffi::CgBounds {
                x: 0.0,
                y: 0.0,
                w: 1.0,
                h: 1.0,
            },
            title: None,
        };
        assert_eq!(cg_residue_of(Ok(vec![present]), 4242), Some(true));
        assert_eq!(cg_residue_of(Ok(vec![]), 4242), Some(false));
    }
}

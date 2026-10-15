#![forbid(unsafe_code)]

//! Read-only focus-oracle diagnosis: workspace frontmost before/after plus
//! the actual session/secure/trust legs and, for exactly the observed
//! frontmost PID, the system-wide `AXFocusedApplication` leg beside the
//! independent direct per-application leg (`AXWindows` count plus
//! focused/main window joins and the `AXFrontmost` pair read BEFORE and
//! AFTER all window legs) with a title-free CG owner join. Both AX legs
//! here are diagnostic-only: production agreement uses the selected oracle
//! (SLPS front-process agreeing with the workspace frontmost before+after,
//! plus the direct per-app selection with CG ownership — no system-wide
//! leg, no `AXFrontmost` gating; see
//! [`crate::ffi::ax_selected_focus_evidence`]). The experimental
//! corroborated preimage evidence outcome
//! ([`crate::ffi::ax_focused_preimage_evidence`]) runs ONLY behind the
//! explicit `--fallback` opt-in (unproven, lacks acceptance; default legs
//! use no additional SLPS symbols implicitly) and the Accessory AppKit
//! lifecycle runs ONLY behind the explicit `--appkit` opt-in (default is
//! raw/no-AppKit). Every AX handle lives and dies on its leg's thread; the
//! worker leg runs the same two reads on a dedicated thread for the
//! main-vs-worker comparison (plus a run-loop query-presence bit on each
//! side — `CFRunLoop::current` is created-on-demand, so query-presence
//! only, never pumped/live acceptance). IDs, counts, status names, and
//! latencies only: never titles, content, paths, or owner names, and never
//! unrelated window dumps (only the observed frontmost PID's WIDs). No focus
//! dispatch, no activation, no taps, no synthetic input, no Space moves, no
//! attribute writes, no settings writes: messaging timeouts (2s, as in the
//! preimage) are the only AX setting touched, exactly as the existing read
//! paths do. Bounded: the direct leg inspects focused/main/count only (no
//! every-window joins), so the worker bound never pays a per-window worst
//! case; a shared hard-lifetime watchdog bounds the whole run.

use serde::Serialize;
use std::time::{Duration, Instant};

/// Hard lifetime for the entire focus-diagnose run (main + worker + CG +
/// optional fallback): the worker query is bounded 25s and every other leg
/// is a finite AX/CG read, so 60s covers the whole run plus grace. The
/// shared watchdog forces a clear failure exit on expiry; disarm joins it
/// on return.
const DIAGNOSE_HARD_LIMIT: Duration = Duration::from_secs(60);
/// Bounded worker query: the direct leg carries no every-window joins, so
/// this covers system + app (count/focused/main/frontmost pair) + CG with
/// margin.
const WORKER_BOUND: Duration = Duration::from_secs(25);

/// One thread's diagnostic legs with its wall time and run-loop observation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FocusDiagnoseSide {
    pub latency_ms: u64,
    /// Run-loop query-presence on this thread (`CFRunLoop::current` is
    /// created-on-demand: presence of the query only, never pumped/live
    /// acceptance).
    pub runloop_query_present: bool,
    pub system: crate::ffi::AxSystemFocusLeg,
    pub app: Option<crate::ffi::AxAppFocusLeg>,
}

/// Experimental fallback evidence outcome on the calling thread (only
/// populated behind `--fallback`): the evidence (route + detail) or the
/// refusal cause, with its own latency. The `route` inside `evidence`
/// always distinguishes fallback corroboration from systemwide success.
/// Unproven: lacking acceptance, never production evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FallbackEvidenceView {
    pub latency_ms: u64,
    pub evidence: Option<crate::ffi::PreimageEvidence>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FocusDiagnoseReport {
    pub bundle: String,
    /// Initialization path: `raw` (default, no AppKit) or
    /// `appkit-accessory` (explicit `--appkit` diagnostic).
    pub init_path: String,
    pub session_before: crate::ffi::SessionState,
    pub session_after: crate::ffi::SessionState,
    pub secure_input: Option<bool>,
    pub ax_trusted: Option<bool>,
    pub frontmost_before: Option<FrontmostId>,
    pub frontmost_after: Option<FrontmostId>,
    pub main: FocusDiagnoseSide,
    pub worker: FocusDiagnoseSide,
    pub worker_spawn: String,
    pub fallback: FallbackEvidenceView,
    pub cg_status: String,
    pub cg_total_windows: Option<usize>,
    pub cg_frontmost_count: Option<usize>,
    pub cg_frontmost_wids: Vec<u32>,
    pub cg_frontmost_onscreen: Option<usize>,
}

/// Minimal frontmost identity for the diagnose report: PID plus bundle ID
/// only (no titles, paths, or content).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FrontmostId {
    pub pid: i32,
    pub bundle: String,
}

fn frontmost_id() -> Option<FrontmostId> {
    crate::ffi::frontmost_app().map(|(pid, bundle)| FrontmostId { pid, bundle })
}

/// Pure session gate for the diagnose legs: `None` only on the ordinary
/// unlocked desktop; anything else is a stop reason naming the verdict.
/// Unit-tested; no desktop effect.
fn locked_stop_reason(session: &crate::ffi::SessionState) -> Option<String> {
    if session.unlocked() {
        None
    } else {
        Some(format!(
            "session not unlocked ({:?}); refusing focus-diagnose legs",
            session.lock
        ))
    }
}

fn diagnose_side(frontmost_pid: Option<i32>) -> FocusDiagnoseSide {
    let start = Instant::now();
    let runloop_query_present = crate::ffi::runloop_present();
    let system = crate::ffi::ax_system_focus_leg();
    let app = match frontmost_pid {
        Some(pid) if pid > 1 => Some(crate::ffi::ax_app_focus_leg(pid)),
        _ => None,
    };
    FocusDiagnoseSide {
        latency_ms: crate::focus::latency_ms(start),
        runloop_query_present,
        system,
        app,
    }
}

/// Nonmutating focus diagnostic (see [`FocusDiagnoseReport`]). Fails closed
/// when the window-server session is not the ordinary unlocked desktop;
/// otherwise records every leg's real status — AX failures are data, never
/// errors here.
///
/// `fallback` opts into the experimental unproven corroborated evidence
/// (default legs never touch its SLPS symbols implicitly); `appkit` opts
/// into the Accessory AppKit lifecycle equal to tile (default is
/// raw/no-AppKit). Both are labelled in the report; neither adds focus
/// effects or settings writes.
pub fn diagnose_focus(fallback: bool, appkit: bool) -> Result<String, (i32, String)> {
    let err_json = |stage: &str, error: String| -> (i32, String) {
        (
            1,
            serde_json::json!({"stage": stage, "error": error}).to_string(),
        )
    };
    // Hard lifetime for the entire run (main + worker + CG + optional
    // fallback), shared implementation with tile. `Drop` disarms on every
    // return path; expiry forces a clear failure exit.
    let mut _watchdog = crate::watchdog::Watchdog::arm(DIAGNOSE_HARD_LIMIT).map_err(|e| (1, e))?;
    let session_before = crate::ffi::session_state();
    if let Some(reason) = locked_stop_reason(&session_before) {
        return Err(err_json("session-locked", reason));
    }
    let init_path = if appkit {
        crate::focus::init_appkit_accessory_for_diagnose().map_err(|e| (2, e))?;
        "appkit-accessory".to_owned()
    } else {
        "raw".to_owned()
    };
    if let Some(reason) = locked_stop_reason(&crate::ffi::session_state()) {
        return Err(err_json("session-locked", reason));
    }
    let trust = crate::ffi::ax_is_process_trusted();
    let secure = crate::ffi::secure_input_known();
    let before = frontmost_id();
    let frontmost_pid = before.as_ref().map(|f| f.pid);
    let main = diagnose_side(frontmost_pid);
    // Session re-check before the worker leg: a lock appearing mid-run
    // stops the run rather than reading on under a changed session.
    if let Some(reason) = locked_stop_reason(&crate::ffi::session_state()) {
        return Err(err_json("session-locked-worker", reason));
    }
    let worker = {
        let (done_tx, done_rx) = std::sync::mpsc::channel::<FocusDiagnoseSide>();
        let spawned = std::thread::Builder::new()
            .name("focus-diagnose-worker".to_owned())
            .spawn(move || {
                // Every AX handle below lives and dies on this worker.
                let side = diagnose_side(frontmost_pid);
                let _ = done_tx.send(side);
            })
            .is_ok();
        if !spawned {
            FocusDiagnoseSide {
                latency_ms: 0,
                runloop_query_present: false,
                system: crate::ffi::AxSystemFocusLeg {
                    system_element: "worker-spawn-failed".to_owned(),
                    system_timeout_status: None,
                    system_timeout_name: None,
                    focused_app_status: "worker-spawn-failed".to_owned(),
                    focused_pid: None,
                    app_timeout_status: None,
                    app_timeout_name: None,
                    app_pid_status: "unreached".to_owned(),
                    focused_window_status: "unreached".to_owned(),
                    focused_wid: None,
                },
                app: None,
            }
        } else {
            match done_rx.recv_timeout(WORKER_BOUND) {
                Ok(side) => side,
                Err(_) => FocusDiagnoseSide {
                    latency_ms: WORKER_BOUND.as_millis().min(u128::from(u64::MAX)) as u64,
                    runloop_query_present: false,
                    system: crate::ffi::AxSystemFocusLeg {
                        system_element: "worker-timeout".to_owned(),
                        system_timeout_status: None,
                        system_timeout_name: None,
                        focused_app_status: "worker-timeout (bounded 25s)".to_owned(),
                        focused_pid: None,
                        app_timeout_status: None,
                        app_timeout_name: None,
                        app_pid_status: "unreached".to_owned(),
                        focused_window_status: "unreached".to_owned(),
                        focused_wid: None,
                    },
                    app: None,
                },
            }
        }
    };
    let worker_spawn = if worker.system.system_element == "worker-spawn-failed" {
        "spawn-failed".to_owned()
    } else if worker.system.system_element == "worker-timeout" {
        "timeout".to_owned()
    } else {
        "ok".to_owned()
    };
    // Session re-check before the fallback-evidence leg: same stop rule.
    if let Some(reason) = locked_stop_reason(&crate::ffi::session_state()) {
        return Err(err_json("session-locked-evidence", reason));
    }
    let fallback = if fallback {
        let start = Instant::now();
        match crate::ffi::ax_focused_preimage_evidence() {
            Ok(evidence) => FallbackEvidenceView {
                latency_ms: crate::focus::latency_ms(start),
                evidence: Some(evidence),
                error: None,
            },
            Err(error) => FallbackEvidenceView {
                latency_ms: crate::focus::latency_ms(start),
                evidence: None,
                error: Some(error),
            },
        }
    } else {
        FallbackEvidenceView {
            latency_ms: 0,
            evidence: None,
            error: Some(
                "fallback not requested (pass --fallback for experimental unproven evidence)"
                    .to_owned(),
            ),
        }
    };
    // Session re-check before the CG join: same stop rule, no read under a
    // changed session.
    if let Some(reason) = locked_stop_reason(&crate::ffi::session_state()) {
        return Err(err_json("session-locked-cg", reason));
    }
    let (cg_status, cg_total_windows, cg_frontmost_count, cg_frontmost_wids, cg_frontmost_onscreen) =
        match crate::ffi::cg_window_list_basic() {
            Ok(list) => {
                let total = list.len();
                match frontmost_pid {
                    Some(pid) if pid > 1 => {
                        if let Ok(pid_u) = u32::try_from(pid) {
                            let owned: Vec<&crate::ffi::CgWindowBasic> =
                                list.iter().filter(|w| w.pid == pid_u).collect();
                            let wids: Vec<u32> = owned
                                .iter()
                                .take(crate::config::MAX_WINDOWS)
                                .map(|w| w.wid)
                                .collect();
                            let onscreen = owned.iter().filter(|w| w.onscreen).count();
                            (
                                "kAXErrorSuccess".to_owned(),
                                Some(total),
                                Some(owned.len()),
                                wids,
                                Some(onscreen),
                            )
                        } else {
                            (
                                "frontmost-pid-out-of-range".to_owned(),
                                Some(total),
                                None,
                                Vec::new(),
                                None,
                            )
                        }
                    }
                    _ => (
                        "no-frontmost-pid".to_owned(),
                        Some(total),
                        None,
                        Vec::new(),
                        None,
                    ),
                }
            }
            Err(e) => (e, None, None, Vec::new(), None),
        };
    let session_after = crate::ffi::session_state();
    let after = frontmost_id();
    let report = FocusDiagnoseReport {
        bundle: crate::BUNDLE_ID.to_owned(),
        init_path,
        session_before,
        session_after,
        secure_input: secure,
        ax_trusted: trust,
        frontmost_before: before,
        frontmost_after: after,
        main,
        worker,
        worker_spawn,
        fallback,
        cg_status,
        cg_total_windows,
        cg_frontmost_count,
        cg_frontmost_wids,
        cg_frontmost_onscreen,
    };
    let out = serde_json::to_string(&report).map_err(|e| (1, e.to_string()));
    _watchdog.disarm();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnose_session_gate_only_passes_unlocked() {
        // Pure gate: the ordinary unlocked desktop proceeds; locked or
        // unknown sessions stop before any AX/CG leg. No FFI, no effect.
        use crate::ffi::{ScreenLockRead, SessionLock, SessionState};
        let unlocked = SessionState {
            login_done: Some(true),
            on_console: Some(true),
            screen_lock: ScreenLockRead::Absent,
            lock: SessionLock::Unlocked,
        };
        assert!(locked_stop_reason(&unlocked).is_none());
        let locked = SessionState {
            login_done: Some(true),
            on_console: Some(true),
            screen_lock: ScreenLockRead::Locked,
            lock: SessionLock::Locked,
        };
        assert!(locked_stop_reason(&locked).is_some());
        let unknown = SessionState::unknown();
        assert!(locked_stop_reason(&unknown).is_some());
    }

    #[test]
    fn diagnose_report_carries_no_content_keys() {
        // Privacy shape: the report type has no title/content/path/exe
        // field to leak — IDs, counts, status names, and the evidence
        // route/detail only.
        use crate::ffi::{AxAppFocusLeg, AxSystemFocusLeg, PreimageEvidence, PreimageRoute};
        let system = AxSystemFocusLeg {
            system_element: "ok".to_owned(),
            system_timeout_status: Some(0),
            system_timeout_name: Some("kAXErrorSuccess".to_owned()),
            focused_app_status: "kAXErrorCannotComplete".to_owned(),
            focused_pid: None,
            app_timeout_status: None,
            app_timeout_name: None,
            app_pid_status: "unreached".to_owned(),
            focused_window_status: "unreached".to_owned(),
            focused_wid: None,
        };
        let app = AxAppFocusLeg {
            pid: 4242,
            element: "ok".to_owned(),
            timeout_status: Some(0),
            timeout_name: Some("kAXErrorSuccess".to_owned()),
            windows_status: "kAXErrorSuccess".to_owned(),
            windows_count: Some(1),
            window_join_failures: 0,
            joined_wids: Vec::new(),
            focused_status: "kAXErrorSuccess".to_owned(),
            focused_wid: Some(7),
            main_status: "kAXErrorNoValue".to_owned(),
            main_wid: None,
            frontmost_before_status: "kAXErrorSuccess-true".to_owned(),
            frontmost_before: Some(true),
            frontmost_after_status: "kAXErrorSuccess-true".to_owned(),
            frontmost_after: Some(true),
        };
        let side = FocusDiagnoseSide {
            latency_ms: 0,
            runloop_query_present: false,
            system,
            app: Some(app),
        };
        let report = FocusDiagnoseReport {
            bundle: crate::BUNDLE_ID.to_owned(),
            init_path: "raw".to_owned(),
            session_before: crate::ffi::SessionState::unknown(),
            session_after: crate::ffi::SessionState::unknown(),
            secure_input: None,
            ax_trusted: None,
            frontmost_before: None,
            frontmost_after: None,
            main: side.clone(),
            worker: side,
            worker_spawn: "ok".to_owned(),
            fallback: FallbackEvidenceView {
                latency_ms: 0,
                evidence: Some(PreimageEvidence {
                    pid: 4242,
                    wid: Some(7),
                    route: PreimageRoute::SlpsFrontCorroborated,
                    detail: "corroborated".to_owned(),
                }),
                error: None,
            },
            cg_status: "kAXErrorSuccess".to_owned(),
            cg_total_windows: Some(5),
            cg_frontmost_count: Some(1),
            cg_frontmost_wids: vec![7],
            cg_frontmost_onscreen: Some(1),
        };
        let json = serde_json::to_string(&report).unwrap();
        for key in ["\"title\"", "\"content\"", "\"path\"", "\"exe\""] {
            assert!(!json.contains(key), "diagnose report must not carry {key}");
        }
        assert!(json.contains("slps-front-corroborated"));
        assert!(json.contains("\"init_path\":\"raw\""));
    }
}

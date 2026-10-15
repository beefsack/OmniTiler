#![forbid(unsafe_code)]

//! Read-only scoped metadata inspection for named everyday apps.
//!
//! `scope-inspect --scope BUNDLE` reports bounded native facts for exactly the
//! allowlisted bundles: PID set from `running_apps`, per-app AX facts via
//! [`crate::native_facts`] (role/subrole/frame/fullscreen/minimized/modal,
//! size-settable, min hints, CG layer/on-screen join), classifier disposition,
//! and per-window Space membership plus current-Space metadata. No writes on
//! any path: no AX setters, no focus dispatch, no taps, no Space moves, no
//! settings writes. Messaging timeouts (finite, per-element, thread-local)
//! are the only AX setting touched, exactly as existing read paths do.
//!
//! Privacy: IDs, frames, roles, counts, and status names only. No AX titles,
//! no content, no paths, no executable names. Bundle IDs ride only as the
//! requested scope echo plus the matched flag (never a discovered foreign
//! string beyond the allowlist check).
//!
//! Threading: the caller (main) snapshots the PID list plus Space diagnosis,
//! then one dedicated per-app AX thread per matched PID performs every
//! `AxElement` construct/use/drop plus its CG/SLS joins. The whole query runs
//! under the shared hard-lifetime watchdog; the main thread pumps AppKit
//! while waiting (bounded, never blocking indefinitely). A hung app costs its
//! worker bound only.

use serde::Serialize;
use std::time::{Duration, Instant};

/// Hard lifetime for the whole inspection (discovery + workers + CG/SLS).
const INSPECT_HARD_LIMIT: Duration = Duration::from_secs(60);
/// Per-app worker bound: AX window join plus per-window SLS reads plus margin.
const WORKER_BOUND: Duration = Duration::from_secs(20);
/// Max inspected apps: explicit scopes stay small; beyond this report
/// truncated rather than spawning unbounded threads.
const MAX_INSPECT_APPS: usize = 16;

/// One window's read-only facts plus classification. No titles, no content,
/// no paths, no exe names. Frames are points (AX) and CG points as read.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct InspectedWindow {
    pub wid: u32,
    pub role: Option<String>,
    pub subrole: Option<String>,
    pub ax_frame: Option<AxRect>,
    pub cg_bounds: Option<AxRect>,
    pub cg_layer: Option<i32>,
    pub cg_onscreen: Option<bool>,
    pub fullscreen: Option<bool>,
    pub minimized: Option<bool>,
    pub modal: Option<bool>,
    pub size_settable: Option<bool>,
    pub min_size: Option<MinSize>,
    pub hints_min_w: Option<i32>,
    pub hints_min_h: Option<i32>,
    pub disposition: String,
    pub floating: Option<bool>,
    pub spaces: Vec<u64>,
    pub spaces_status: String,
}

/// Integer/float rect in points as read (no conversion claims).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct AxRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// Minimum size as reported (points), where available.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct MinSize {
    pub w: f64,
    pub h: f64,
}

/// One inspected app: PID plus matched flag plus facts. `bundle_echo` is the
/// requested scope string that matched (never a foreign discovered string).
/// `launch_date_present` is a presence bit only (never a value, never a
/// path): launch-date currency for the harness's fresh-identity story. No
/// executable presence/value is recorded (privacy: no titles, content,
/// paths, or exe values).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct InspectedApp {
    pub pid: u32,
    pub bundle_echo: String,
    pub launch_date_present: bool,
    pub hidden: Option<bool>,
    pub activation_policy: Option<String>,
    pub is_own_bundle: Option<bool>,
    pub requested: usize,
    pub observed: usize,
    pub failed: usize,
    pub first_cause: String,
    pub windows: Vec<InspectedWindow>,
    pub error: Option<String>,
}

/// Read-only scoped inspection report. IDs/frames/roles/counts only.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ScopeInspectReport {
    pub bundle: String,
    pub scopes: Vec<String>,
    pub session_before: crate::ffi::SessionState,
    pub session_after: crate::ffi::SessionState,
    pub secure_input: Option<bool>,
    pub ax_trusted: Option<bool>,
    pub frontmost_pid: Option<i32>,
    pub spaces: Option<SpacesView>,
    pub apps: Vec<InspectedApp>,
    pub truncated: bool,
    pub latency_ms: u64,
}

/// Current-Space metadata: displays plus current per display plus totals.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SpacesView {
    pub space_count: usize,
    pub displays: Vec<crate::ffi::SlsDisplay>,
}

fn locked_stop_reason(session: &crate::ffi::SessionState) -> Option<String> {
    if session.unlocked() {
        None
    } else {
        Some(format!(
            "session not unlocked ({:?}); refusing scope-inspect legs",
            session.lock
        ))
    }
}

#[cfg(target_os = "macos")]
fn inspect_one_app(pid: u32, bundle_echo: String) -> InspectedApp {
    use crate::classifier::{WindowFrame, classify_window};
    use crate::native_facts::{read_app_facts, read_app_window_facts};

    let base = |error: Option<String>| InspectedApp {
        pid,
        bundle_echo: bundle_echo.clone(),
        launch_date_present: false,
        hidden: None,
        activation_policy: None,
        is_own_bundle: None,
        requested: 0,
        observed: 0,
        failed: 0,
        first_cause: String::new(),
        windows: Vec::new(),
        error,
    };
    if pid <= 1 {
        return base(Some(format!("refusing reserved pid {pid}")));
    }
    // Presence bit only (never a value, never a path): launch-date currency
    // for the harness's fresh-identity story. No executable presence/value
    // is recorded.
    let identity = crate::ffi::ns_app_identity_for_pid(pid as i32);
    let launch_date_present = identity
        .launch_date
        .as_deref()
        .is_some_and(|s| !s.is_empty());
    let app_facts = match read_app_facts(pid) {
        Ok(f) => f,
        Err(e) => {
            let mut app = base(Some(e));
            app.launch_date_present = launch_date_present;
            return app;
        }
    };
    let outcome = match read_app_window_facts(pid) {
        Ok(o) => o,
        Err(e) => {
            let mut app = base(Some(e));
            app.launch_date_present = launch_date_present;
            app.hidden = Some(app_facts.hidden);
            app.activation_policy = Some(format!("{:?}", app_facts.activation_policy));
            app.is_own_bundle = Some(app_facts.is_own_bundle);
            return app;
        }
    };
    let app_model = app_facts.to_app_facts();
    let mut windows = Vec::with_capacity(outcome.windows.len());
    for facts in &outcome.windows {
        let ax_frame = facts.frame.map(|f| AxRect {
            x: f.x,
            y: f.y,
            w: f.w,
            h: f.h,
        });
        // CG bounds: re-join the title-free list for geometry (the facts
        // carry layer/on-screen only; geometry rides here read-only).
        let cg_bounds = crate::ffi::cg_window_list_basic().ok().and_then(|list| {
            list.iter()
                .find(|w| w.wid == facts.wid && w.pid == pid)
                .map(|w| AxRect {
                    x: w.bounds.x,
                    y: w.bounds.y,
                    w: w.bounds.w,
                    h: w.bounds.h,
                })
        });
        let spaces_status;
        let spaces = match crate::ffi::sls_spaces_for_window(facts.wid) {
            Ok(s) => {
                spaces_status = "ok".to_owned();
                s
            }
            Err(e) => {
                spaces_status = e;
                Vec::new()
            }
        };
        // Classifier disposition over the same pure mapping as the tile
        // backend (modal fallback false: unknown modal maps plain). Gaps
        // report as incomplete rather than invented tiles.
        let (disposition, floating) = match (facts.to_ax_facts(false), facts.to_cg_facts()) {
            (Some(ax), Some(cg)) => {
                let frame = facts.frame.map(|f| WindowFrame {
                    wid: facts.wid,
                    x: f.x as i32,
                    y: f.y as i32,
                    w: f.w as i32,
                    h: f.h as i32,
                });
                match frame {
                    Some(frame) => {
                        let classified = classify_window(frame, &ax, &cg, &app_model);
                        let floating = classified.to_observed().map(|o| o.floating);
                        (format!("{:?}", classified.disposition), floating)
                    }
                    None => ("incomplete-frame".to_owned(), None),
                }
            }
            _ => ("incomplete-facts".to_owned(), None),
        };
        let hints = facts.size_hints();
        windows.push(InspectedWindow {
            wid: facts.wid,
            role: facts.role.clone(),
            subrole: facts.subrole.clone(),
            ax_frame,
            cg_bounds,
            cg_layer: facts.cg_layer,
            cg_onscreen: facts.cg_onscreen,
            fullscreen: facts.fullscreen,
            minimized: facts.minimized,
            modal: facts.modal,
            size_settable: facts.size_settable,
            min_size: facts.min_size.map(|(w, h)| MinSize { w, h }),
            hints_min_w: hints.min_w,
            hints_min_h: hints.min_h,
            disposition,
            floating,
            spaces,
            spaces_status,
        });
    }
    // Deterministic order for evidence diffing.
    windows.sort_by_key(|w| w.wid);
    InspectedApp {
        pid,
        bundle_echo,
        launch_date_present,
        hidden: Some(app_facts.hidden),
        activation_policy: Some(format!("{:?}", app_facts.activation_policy)),
        is_own_bundle: Some(app_facts.is_own_bundle),
        requested: outcome.requested,
        observed: outcome.observed,
        failed: outcome.failed,
        first_cause: outcome.first_cause.clone(),
        windows,
        error: None,
    }
}

#[cfg(not(target_os = "macos"))]
fn inspect_one_app(pid: u32, bundle_echo: String) -> InspectedApp {
    InspectedApp {
        pid,
        bundle_echo,
        launch_date_present: false,
        hidden: None,
        activation_policy: None,
        is_own_bundle: None,
        requested: 0,
        observed: 0,
        failed: 0,
        first_cause: String::new(),
        windows: Vec::new(),
        error: Some("scope-inspect is macOS-only".to_owned()),
    }
}

/// Read-only scoped inspection over explicit bundle scopes. Validates the
/// allowlist with [`crate::scope::AppScope`] (Ghostty/own/empty never inspect),
/// snapshots matching PIDs, then reads each on its own bounded worker.
/// `may_mutate` is always false for this command.
pub fn run_scope_inspect(scopes: Vec<String>) -> Result<String, (i32, String)> {
    let err_json = |stage: &str, error: String| -> (i32, String) {
        (
            1,
            serde_json::json!({"stage": stage, "error": error}).to_string(),
        )
    };
    let start = Instant::now();
    let scope = crate::scope::AppScope {
        allowed: scopes.clone(),
    };
    if let Err(e) = scope.validate() {
        return Err((2, e));
    }
    if scopes.is_empty() {
        return Err((
            2,
            "scope-inspect requires at least one --scope BUNDLE".to_owned(),
        ));
    }
    let mut _watchdog = crate::watchdog::Watchdog::arm(INSPECT_HARD_LIMIT).map_err(|e| (1, e))?;
    let session_before = crate::ffi::session_state();
    if let Some(reason) = locked_stop_reason(&session_before) {
        return Err(err_json("session-locked", reason));
    }
    // Main-thread discovery snapshot: PID list only (no AX handles here).
    // `allows` (not `allows_effect`) is the read gate: Ghostty/own/unknown
    // never inspect even when listed (validate already refused them), and
    // out-of-scope bundles never match. Reserved PIDs and the inspector
    // itself never match.
    let self_pid = std::process::id();
    let mut matched: Vec<(u32, String)> = Vec::new();
    #[cfg(target_os = "macos")]
    {
        for (pid_i32, bundle) in crate::ffi::running_apps() {
            if pid_i32 <= 1 {
                continue;
            }
            let pid = pid_i32 as u32;
            if pid == self_pid {
                continue;
            }
            if bundle.is_empty() {
                continue;
            }
            if !scope.allows(Some(&bundle)) {
                continue;
            }
            // Echo the allowlist entry (never a foreign discovered string
            // beyond the match itself).
            if let Some(echo) = scopes.iter().find(|s| s.as_str() == bundle.as_str()) {
                matched.push((pid, echo.clone()));
            }
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (&scope, self_pid, &mut matched);
    }
    matched.sort();
    matched.dedup();
    let truncated = matched.len() > MAX_INSPECT_APPS;
    matched.truncate(MAX_INSPECT_APPS);
    // Space metadata on the caller (main) thread: displays plus current.
    let spaces = match crate::spaces::diagnose() {
        Ok(d) => Some(SpacesView {
            space_count: d.space_count(),
            displays: d.displays,
        }),
        Err(_) => None,
    };
    // One bounded worker per matched PID; the main thread pumps AppKit while
    // waiting (never blocks indefinitely on a hung app).
    let mut delivered: Vec<InspectedApp> = Vec::new();
    let mut rx_list: Vec<(u32, String, std::sync::mpsc::Receiver<InspectedApp>)> = Vec::new();
    for (pid, echo) in matched {
        let echo_for_worker = echo.clone();
        let (tx, rx) = std::sync::mpsc::channel::<InspectedApp>();
        let spawned = std::thread::Builder::new()
            .name(format!("scope-inspect-{pid}"))
            .spawn(move || {
                let app = inspect_one_app(pid, echo_for_worker);
                let _ = tx.send(app);
            })
            .is_ok();
        if !spawned {
            // Spawn failure is per-app evidence, never a silent omission:
            // the PID plus echo ride with the per-app failure count.
            delivered.push(InspectedApp {
                pid,
                bundle_echo: echo,
                launch_date_present: false,
                hidden: None,
                activation_policy: None,
                is_own_bundle: None,
                requested: 0,
                observed: 0,
                failed: 1,
                first_cause: "worker spawn failed".to_owned(),
                windows: Vec::new(),
                error: Some("worker spawn failed".to_owned()),
            });
        } else {
            rx_list.push((pid, echo, rx));
        }
    }
    let deadline = Instant::now() + WORKER_BOUND;
    // Self Accessory AppKit init for the READONLY main pump below (runloop
    // service while waiting on workers): no user windows, no activation, no
    // writes — exactly the existing read-path lifecycle, not a window write.
    #[cfg(target_os = "macos")]
    let appkit_pump = {
        use objc2::MainThreadMarker;
        MainThreadMarker::new().map(|mtm| {
            use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};
            let app = NSApplication::sharedApplication(mtm);
            app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
            app.finishLaunching();
            app
        })
    };
    // Spawn failures: no receiver at all — nothing to wait on; they are
    // already bounded (no thread) and already reported above with PID plus
    // per-app failure count.
    while !rx_list.is_empty() {
        #[cfg(target_os = "macos")]
        {
            if let Some(app) = appkit_pump.as_ref() {
                objc2::rc::autoreleasepool(|_| {
                    crate::ffi::drain_app_events(app);
                    crate::ffi::pump_runloop_once(50);
                });
            }
        }
        if Instant::now() >= deadline {
            for (pid, echo, _) in rx_list.drain(..) {
                delivered.push(InspectedApp {
                    pid,
                    bundle_echo: echo,
                    launch_date_present: false,
                    hidden: None,
                    activation_policy: None,
                    is_own_bundle: None,
                    requested: 0,
                    observed: 0,
                    failed: 1,
                    first_cause: "worker timeout (bounded 20s)".to_owned(),
                    windows: Vec::new(),
                    error: Some("worker timeout (bounded 20s)".to_owned()),
                });
            }
            break;
        }
        let mut i = 0;
        let mut progressed = false;
        while i < rx_list.len() {
            match rx_list[i].2.try_recv() {
                Ok(app) => {
                    delivered.push(app);
                    rx_list.remove(i);
                    progressed = true;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => i += 1,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    let (pid, echo, _) = rx_list.remove(i);
                    delivered.push(InspectedApp {
                        pid,
                        bundle_echo: echo,
                        launch_date_present: false,
                        hidden: None,
                        activation_policy: None,
                        is_own_bundle: None,
                        requested: 0,
                        observed: 0,
                        failed: 1,
                        first_cause: "worker disconnected".to_owned(),
                        windows: Vec::new(),
                        error: Some("worker disconnected".to_owned()),
                    });
                    progressed = true;
                }
            }
        }
        if !progressed {
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    delivered.sort_by_key(|a| a.pid);
    let session_after = crate::ffi::session_state();
    let report = ScopeInspectReport {
        bundle: crate::BUNDLE_ID.to_owned(),
        scopes: scopes.clone(),
        session_before,
        session_after,
        secure_input: crate::ffi::secure_input_known(),
        ax_trusted: crate::ffi::ax_is_process_trusted(),
        frontmost_pid: crate::ffi::frontmost_app().map(|(pid, _)| pid),
        spaces,
        apps: delivered,
        truncated,
        latency_ms: start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
    };
    let out = serde_json::to_string(&report).map_err(|e| (1, e.to_string()));
    _watchdog.disarm();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_empty_scope_before_any_native_touch() {
        let err = run_scope_inspect(Vec::new()).unwrap_err();
        assert_eq!(err.0, 2);
    }

    #[test]
    fn rejects_ghostty_and_own_bundle_scopes() {
        for scope in [
            "com.mitchellh.ghostty".to_owned(),
            "COM.MITCHELLH.GHOSTTY".to_owned(),
            crate::BUNDLE_ID.to_owned(),
            String::new(),
        ] {
            let err = run_scope_inspect(vec![scope.clone()]).unwrap_err();
            assert_ne!(err.1, String::new(), "scope {scope}");
        }
    }

    #[test]
    fn report_carries_no_title_content_path_or_exe_keys() {
        let report = ScopeInspectReport {
            bundle: crate::BUNDLE_ID.to_owned(),
            scopes: vec!["com.apple.TextEdit".to_owned()],
            session_before: crate::ffi::SessionState::unknown(),
            session_after: crate::ffi::SessionState::unknown(),
            secure_input: None,
            ax_trusted: None,
            frontmost_pid: None,
            spaces: None,
            apps: vec![InspectedApp {
                pid: 4242,
                bundle_echo: "com.apple.TextEdit".to_owned(),
                launch_date_present: true,
                hidden: Some(false),
                activation_policy: Some("Regular".to_owned()),
                is_own_bundle: Some(false),
                requested: 1,
                observed: 1,
                failed: 0,
                first_cause: String::new(),
                windows: vec![InspectedWindow {
                    wid: 7,
                    role: Some("AXWindow".to_owned()),
                    subrole: Some("AXStandardWindow".to_owned()),
                    ax_frame: Some(AxRect {
                        x: 0.0,
                        y: 0.0,
                        w: 800.0,
                        h: 600.0,
                    }),
                    cg_bounds: Some(AxRect {
                        x: 0.0,
                        y: 0.0,
                        w: 800.0,
                        h: 600.0,
                    }),
                    cg_layer: Some(0),
                    cg_onscreen: Some(true),
                    fullscreen: Some(false),
                    minimized: Some(false),
                    modal: Some(false),
                    size_settable: Some(true),
                    min_size: None,
                    hints_min_w: None,
                    hints_min_h: None,
                    disposition: "Tile".to_owned(),
                    floating: Some(false),
                    spaces: vec![1],
                    spaces_status: "ok".to_owned(),
                }],
                error: None,
            }],
            truncated: false,
            latency_ms: 0,
        };
        let json = serde_json::to_string(&report).unwrap();
        for key in [
            "\"title\"",
            "\"content\"",
            "\"path\"",
            "\"exe\"",
            "\"executable\"",
        ] {
            assert!(!json.contains(key), "inspect report must not carry {key}");
        }
        assert!(json.contains("AXWindow"));
        assert!(json.contains("\"disposition\":\"Tile\""));
    }

    #[test]
    fn window_order_is_deterministic_by_wid() {
        fn blank(wid: u32) -> InspectedWindow {
            InspectedWindow {
                wid,
                role: None,
                subrole: None,
                ax_frame: None,
                cg_bounds: None,
                cg_layer: None,
                cg_onscreen: None,
                fullscreen: None,
                minimized: None,
                modal: None,
                size_settable: None,
                min_size: None,
                hints_min_w: None,
                hints_min_h: None,
                disposition: "Tile".to_owned(),
                floating: None,
                spaces: vec![],
                spaces_status: "ok".to_owned(),
            }
        }
        let mut windows = [blank(9), blank(7)];
        windows.sort_by_key(|w| w.wid);
        assert_eq!((windows[0].wid, windows[1].wid), (7, 9));
    }
}

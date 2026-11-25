//! Windows-only dedicated DXGI fullscreen request probe (development only).
//!
//! Own winit window plus own D3D11 device and swapchain; it never touches a
//! fixture HWND (the fixture uses winit + softbuffer and must not share a
//! swapchain). Current-mode-only: `SetFullscreenState(TRUE)` with a NULL
//! target and no `ResizeTarget`/mode change.
//!
//! Focused/pumped leg: the window runs on a pumped winit event loop, requests
//! foreground via the safe `focus_window` API (no synthetic input), verifies
//! the grant natively, presents one windowed frame, then issues the bounded
//! fullscreen request. The request always ends with `SetFullscreenState(FALSE)`
//! plus a FALSE readback, best-effort prior-foreground restoration, window
//! destruction with a native absence check, and a before/after comparison of
//! display mode, topology and foreground.
//!
//! A `GetFullscreenState == true` readback is an API grant only: it is NOT
//! proof of legacy exclusive ownership, FSO, or independent-flip promotion.
//!
//! Non-Windows builds compile to a stub; all logic below is `cfg(windows)`.

#[cfg(not(windows))]
fn main() {
    eprintln!("win-dxgi-probe: Windows-only; stub on this platform.");
    std::process::exit(2);
}

#[cfg(windows)]
fn main() {
    let code = probe::run(std::env::args().skip(1).collect());
    std::process::exit(code);
}

#[cfg(windows)]
mod probe {
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    // Reviewed DXGI wrappers live in the fixture's approved Windows FFI root,
    // shared with `win-probe` instead of per-example `allow(unsafe_code)`.
    use tiler_live_fixture::pipe_win::dxgi as nat;
    use winit::application::ApplicationHandler;
    use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
    use winit::window::{Window, WindowId};

    const DEFAULT_HOLD_MS: u64 = 2_000;
    const MAX_HOLD_MS: u64 = 10_000;
    /// Bound for the foreground grant wait before proceeding regardless.
    const FOCUS_WAIT_MS: u64 = 1_500;

    pub fn run(args: Vec<String>) -> i32 {
        let out = flag(&args, "--out")
            .map(PathBuf::from)
            .unwrap_or_else(default_out);
        let hold_ms = flag(&args, "--hold-ms")
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(DEFAULT_HOLD_MS)
            .min(MAX_HOLD_MS);

        // Baseline foreground BEFORE any window exists.
        let pre_create_foreground = nat::foreground();
        let mut app = ProbeApp::new(hold_ms, pre_create_foreground);
        // `EventLoop::new` also installs the process PMv2 DPI awareness that
        // keeps the native geometry below in physical pixels.
        let event_loop = match EventLoop::new() {
            Ok(loop_) => loop_,
            Err(err) => {
                return finish(
                    &out,
                    serde_json::json!({
                        "leg": "dxgi-probe-focused",
                        "status": "window-create-failed",
                        "error": format!("{err:?}"),
                    }),
                );
            }
        };
        if let Err(err) = event_loop.run_app(&mut app) {
            app.report["run_app_error"] = serde_json::json!(format!("{err:?}"));
            app.report["status"] = serde_json::json!("event-loop-failed");
        }
        finish(&out, app.report)
    }

    enum Phase {
        Init,
        Failed,
        Settling {
            since: Instant,
        },
        Holding {
            swap: nat::SwapPair,
            guard: Option<nat::FullscreenGuard>,
            deadline: Instant,
        },
        Done,
    }

    struct ProbeApp {
        hold_ms: u64,
        pid: u32,
        pre_create_foreground: isize,
        window: Option<Window>,
        hwnd: Option<isize>,
        focus_requested: bool,
        focus_granted: Option<bool>,
        baseline: serde_json::Value,
        report: serde_json::Value,
        phase: Phase,
    }

    impl ProbeApp {
        fn new(hold_ms: u64, pre_create_foreground: isize) -> Self {
            let pid = std::process::id();
            Self {
                hold_ms,
                pid,
                pre_create_foreground,
                window: None,
                hwnd: None,
                focus_requested: false,
                focus_granted: None,
                baseline: serde_json::Value::Null,
                report: serde_json::json!({
                    "leg": "dxgi-probe-focused",
                    "pid": pid,
                    "hold_ms": hold_ms,
                    "pre_create_foreground": pre_create_foreground,
                }),
                phase: Phase::Init,
            }
        }

        fn fail(&mut self, el: &ActiveEventLoop, status: &str, detail: serde_json::Value) {
            self.report["status"] = serde_json::json!(status);
            self.report["error"] = detail;
            self.window = None;
            self.phase = Phase::Failed;
            el.exit();
        }

        /// Baseline, device/swapchain, one windowed present, fullscreen TRUE.
        fn issue_request(&mut self, el: &ActiveEventLoop) {
            let hwnd = match self.hwnd {
                Some(hwnd) => hwnd,
                None => {
                    self.fail(
                        el,
                        "window-create-failed",
                        serde_json::json!("missing hwnd"),
                    );
                    return;
                }
            };
            let window = match self.window.as_ref() {
                Some(window) => window,
                None => {
                    self.fail(
                        el,
                        "window-create-failed",
                        serde_json::json!("missing window"),
                    );
                    return;
                }
            };
            let client = window.inner_size();
            let window_rect = nat::window_rect(hwnd);
            let monitor = nat::monitor_info(hwnd);
            let mode = monitor
                .as_ref()
                .and_then(|m| nat::display_mode(Some(&m.device)));
            self.baseline = serde_json::json!({
                "window_rect": window_rect,
                "monitor": monitor,
                "mode": mode,
                "foreground": nat::foreground(),
                "dpi": nat::dpi(hwnd),
                "scale_factor": window.scale_factor(),
                "client_px": [client.width, client.height],
            });
            self.report["baseline"] = self.baseline.clone();

            // Default adapter, HARDWARE driver, current client size, windowed
            // swapchain. No WARP fallback, no ResizeTarget, no mode change.
            let (create_hr, swap) =
                nat::create_device_and_swapchain(hwnd, client.width, client.height);
            self.report["create_hr"] = serde_json::json!(create_hr);
            let Some(swap) = swap else {
                self.report["status"] = serde_json::json!("device-create-refused");
                self.window = None;
                self.phase = Phase::Failed;
                el.exit();
                return;
            };
            // One minimal windowed present: a cheap real oracle for the
            // composed present path. The backbuffer is uninitialized; this
            // proves presentation mechanics only, never content.
            let present_hr = nat::present(&swap.chain);
            self.report["present_hr"] = serde_json::json!(present_hr);

            // Panic guard: leaving fullscreen TRUE is the one state this
            // probe must never leave behind. It is disarmed only after the
            // FALSE readback succeeds below.
            let guard = nat::FullscreenGuard::armed(swap.chain.clone());
            let set_true_hr = nat::set_fullscreen(&swap.chain, true);
            self.report["set_true_hr"] = serde_json::json!(set_true_hr);
            self.report["request"] = serde_json::json!(if set_true_hr == 0 {
                "grant-requested"
            } else {
                "refused"
            });
            let deadline = Instant::now() + Duration::from_millis(self.hold_ms);
            self.phase = Phase::Holding {
                swap,
                guard: Some(guard),
                deadline,
            };
        }

        /// Readbacks, SetFullscreenState(FALSE) + verified readback, prior
        /// foreground best-effort restore, destroy, native absence check.
        fn settle(&mut self, el: &ActiveEventLoop) {
            let hwnd = self.hwnd.unwrap_or(0);
            let (swap, mut guard) = match std::mem::replace(&mut self.phase, Phase::Done) {
                Phase::Holding { swap, guard, .. } => (swap, guard),
                _ => {
                    self.fail(
                        el,
                        "event-loop-failed",
                        serde_json::json!("bad settle phase"),
                    );
                    return;
                }
            };
            let (state_hr, fullscreen, has_output) = nat::get_fullscreen(&swap.chain);
            self.report["after_true"] = serde_json::json!({
                "get_state_hr": state_hr,
                "fullscreen": fullscreen,
                "has_output": has_output,
                "output": nat::containing_output(&swap.chain),
                "window_rect": nat::window_rect(hwnd),
                "monitor": nat::monitor_info(hwnd),
                "foreground": nat::foreground(),
            });
            if let Some(m) = nat::monitor_info(hwnd) {
                self.report["after_true"]["mode"] =
                    serde_json::json!(nat::display_mode(Some(&m.device)));
            }

            let set_false_hr = nat::set_fullscreen(&swap.chain, false);
            self.report["set_false_hr"] = serde_json::json!(set_false_hr);
            let (final_hr, final_fullscreen, _) = nat::get_fullscreen(&swap.chain);
            // Disarm only on a verified FALSE readback; otherwise the guard
            // stays armed and retries FALSE when dropped below.
            if final_fullscreen == Some(false)
                && let Some(g) = guard.take()
            {
                g.disarm();
            }
            let final_window = nat::window_rect(hwnd);
            let final_monitor = nat::monitor_info(hwnd);
            let final_mode = final_monitor
                .as_ref()
                .and_then(|m| nat::display_mode(Some(&m.device)));
            // Best-effort prior-foreground restore without synthetic input:
            // a direct request that the OS may deny; the readback decides.
            let restore_ok = nat::set_foreground(self.pre_create_foreground);
            self.report["final"] = serde_json::json!({
                "get_state_hr": final_hr,
                "fullscreen": final_fullscreen,
                "window_rect": final_window,
                "monitor": final_monitor,
                "mode": final_mode,
                "prior_foreground_restore_requested": restore_ok,
            });

            drop(swap);
            drop(guard);
            // Destroy the window, then check natively before claiming absence.
            // A recycled HWND could false-positive, so this is point-in-time.
            self.window = None;
            let absent_native = !nat::is_window(hwnd);
            self.report["window_absent_native"] = serde_json::json!(absent_native);
            self.report["foreground_after_destroy"] = serde_json::json!(nat::foreground());

            let mode_match = serde_json::json!(final_mode.clone()) == self.baseline["mode"];
            let monitor_match =
                serde_json::json!(final_monitor.clone()) == self.baseline["monitor"];
            self.report["restored"] =
                serde_json::json!(final_fullscreen == Some(false) && mode_match && monitor_match);
            self.report["status"] = serde_json::json!(if final_fullscreen == Some(false)
                && mode_match
                && monitor_match
            {
                "completed"
            } else {
                "completed-restore-mismatch"
            });
            self.report["limits"] = serde_json::json!([
                "GetFullscreenState true is a SetFullscreenState API grant only, not legacy exclusive/FSO/independent-flip proof",
                "one windowed present of an uninitialized backbuffer proves presentation mechanics only, never content",
                "foreground grant verified by native readback; denial leaves a limit, not a retry",
                "window absence is a point-in-time native IsWindow check after drop, not proof against HWND reuse",
                "single default-adapter HARDWARE attempt: no WARP fallback, no ResizeTarget, no mode change",
            ]);
            self.phase = Phase::Done;
            el.exit();
        }
    }

    impl ApplicationHandler for ProbeApp {
        fn resumed(&mut self, el: &ActiveEventLoop) {
            if self.window.is_some() || matches!(self.phase, Phase::Failed | Phase::Done) {
                return;
            }
            let attrs = Window::default_attributes()
                .with_title(format!("OmniTilerDxgiProbe:{}", self.pid))
                .with_inner_size(winit::dpi::LogicalSize::new(640.0, 480.0));
            match el.create_window(attrs) {
                Ok(window) => {
                    // Safe activation request only; the OS decides. No
                    // synthetic input is ever injected.
                    window.focus_window();
                    self.focus_requested = true;
                    self.hwnd = window_hwnd(&window);
                    self.window = Some(window);
                    self.phase = Phase::Settling {
                        since: Instant::now(),
                    };
                }
                Err(err) => {
                    self.fail(
                        el,
                        "window-create-failed",
                        serde_json::json!(format!("{err:?}")),
                    );
                }
            }
            el.set_control_flow(ControlFlow::WaitUntil(
                Instant::now() + Duration::from_millis(50),
            ));
        }

        fn window_event(
            &mut self,
            el: &ActiveEventLoop,
            _id: WindowId,
            event: winit::event::WindowEvent,
        ) {
            use winit::event::WindowEvent;
            if matches!(event, WindowEvent::CloseRequested | WindowEvent::Destroyed) {
                self.window = None;
                self.phase = Phase::Done;
                el.exit();
            }
        }

        fn about_to_wait(&mut self, el: &ActiveEventLoop) {
            match &self.phase {
                Phase::Settling { since } => {
                    let granted = self.hwnd == Some(nat::foreground()) && self.hwnd != Some(0);
                    if granted {
                        self.focus_granted = Some(true);
                        self.report["focus"] = serde_json::json!({
                            "requested": self.focus_requested,
                            "granted": true,
                            "foreground_at_grant": nat::foreground(),
                        });
                        self.issue_request(el);
                    } else if since.elapsed() >= Duration::from_millis(FOCUS_WAIT_MS) {
                        self.focus_granted = Some(false);
                        self.report["focus"] = serde_json::json!({
                            "requested": self.focus_requested,
                            "granted": false,
                            "foreground_at_timeout": nat::foreground(),
                        });
                        self.issue_request(el);
                    } else {
                        el.set_control_flow(ControlFlow::WaitUntil(
                            Instant::now() + Duration::from_millis(50),
                        ));
                    }
                }
                Phase::Holding { deadline, .. } => {
                    if Instant::now() >= *deadline {
                        self.settle(el);
                    } else {
                        el.set_control_flow(ControlFlow::WaitUntil(*deadline));
                    }
                }
                Phase::Init | Phase::Failed | Phase::Done => {}
            }
        }
    }

    fn window_hwnd(window: &Window) -> Option<isize> {
        use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
        match window.window_handle().ok()?.as_raw() {
            RawWindowHandle::Win32(handle) => Some(handle.hwnd.get()),
            _ => None,
        }
    }

    fn flag(args: &[String], name: &str) -> Option<String> {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1).cloned())
    }

    fn default_out() -> PathBuf {
        std::env::temp_dir().join(format!("omnitiler-dxgi-probe-{}.json", std::process::id()))
    }

    fn finish(out: &PathBuf, report: serde_json::Value) -> i32 {
        let text = serde_json::to_string_pretty(&report).unwrap_or_else(|_| "{}".to_string());
        println!("{text}");
        let code = match report.get("status").and_then(|s| s.as_str()) {
            Some("completed") => 0,
            _ => 1,
        };
        if let Some(parent) = out.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if std::fs::write(out, &text).is_err() {
            eprintln!("win-dxgi-probe: cannot write {}", out.display());
            return 1;
        }
        code
    }
}

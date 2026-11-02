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

    // Sole unsafe owner in this example (mirrors win-probe.rs `ffi_ext`):
    // minimal reviewed wrappers over the locked windows 0.61 bindings. All
    // wrappers are safe signatures returning plain data plus raw HRESULTs.
    #[allow(unsafe_code)]
    mod nat {
        use windows::Win32::Foundation::{FALSE, HMODULE, HWND, RECT, TRUE};
        use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE_HARDWARE, D3D_FEATURE_LEVEL};
        use windows::Win32::Graphics::Direct3D11::{
            D3D11_CREATE_DEVICE_FLAG, D3D11_SDK_VERSION, D3D11CreateDeviceAndSwapChain,
            ID3D11Device, ID3D11DeviceContext,
        };
        use windows::Win32::Graphics::Dxgi::Common::{
            DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_MODE_DESC, DXGI_MODE_SCALING_UNSPECIFIED,
            DXGI_MODE_SCANLINE_ORDER_UNSPECIFIED, DXGI_RATIONAL, DXGI_SAMPLE_DESC,
        };
        use windows::Win32::Graphics::Dxgi::{
            DXGI_PRESENT, DXGI_SWAP_CHAIN_DESC, DXGI_SWAP_EFFECT_DISCARD,
            DXGI_USAGE_RENDER_TARGET_OUTPUT, IDXGIAdapter, IDXGIOutput, IDXGISwapChain,
        };
        use windows::Win32::Graphics::Gdi::{
            DEVMODEW, ENUM_CURRENT_SETTINGS, EnumDisplaySettingsW, GetMonitorInfoW, HMONITOR,
            MONITOR_DEFAULTTONEAREST, MONITORINFO, MONITORINFOEXW, MonitorFromWindow,
        };
        use windows::Win32::UI::HiDpi::GetDpiForWindow;
        use windows::Win32::UI::WindowsAndMessaging::{
            GetForegroundWindow, GetWindowRect, IsWindow, SetForegroundWindow,
        };
        use windows::core::{BOOL, PCWSTR};

        pub struct SwapPair {
            pub chain: IDXGISwapChain,
            #[allow(dead_code)]
            pub device: Option<ID3D11Device>,
            #[allow(dead_code)]
            pub context: Option<ID3D11DeviceContext>,
        }

        /// Best-effort Drop guard so a panic after SetFullscreenState(TRUE)
        /// still leaves fullscreen FALSE. Disarm only after the FALSE
        /// readback succeeds; otherwise drop retries FALSE.
        pub struct FullscreenGuard {
            chain: Option<IDXGISwapChain>,
        }

        impl FullscreenGuard {
            pub fn armed(chain: IDXGISwapChain) -> Self {
                Self { chain: Some(chain) }
            }

            pub fn disarm(mut self) {
                self.chain = None;
            }
        }

        impl Drop for FullscreenGuard {
            fn drop(&mut self) {
                if let Some(chain) = self.chain.take() {
                    // Best effort only; the explicit restore records HRESULTs.
                    unsafe {
                        let _ = chain.SetFullscreenState(false, None::<&IDXGIOutput>);
                    }
                }
            }
        }

        fn hwnd(raw: isize) -> HWND {
            HWND(raw as *mut core::ffi::c_void)
        }

        fn rect4(rect: &RECT) -> [i32; 4] {
            [rect.left, rect.top, rect.right, rect.bottom]
        }

        fn wide_str(raw: &[u16]) -> String {
            let len = raw.iter().position(|c| *c == 0).unwrap_or(raw.len());
            String::from_utf16_lossy(&raw[..len])
        }

        pub fn window_rect(raw: isize) -> Option<[i32; 4]> {
            let mut rect = RECT::default();
            // `GetWindowRect` here is the Result-wrapped projection; with the
            // process PMv2-aware the values are physical pixels.
            unsafe {
                GetWindowRect(hwnd(raw), &mut rect)
                    .ok()
                    .map(|()| rect4(&rect))
            }
        }

        pub fn foreground() -> isize {
            unsafe { GetForegroundWindow().0 as isize }
        }

        /// Best-effort foreground restore without synthetic input; returns
        /// whether the OS granted the request. Never targets a null handle.
        pub fn set_foreground(raw: isize) -> bool {
            if raw == 0 {
                return false;
            }
            unsafe { SetForegroundWindow(hwnd(raw)) != FALSE }
        }

        /// Point-in-time native existence check; call only after drop.
        pub fn is_window(raw: isize) -> bool {
            if raw == 0 {
                return false;
            }
            unsafe { IsWindow(Some(hwnd(raw))) != FALSE }
        }

        pub fn dpi(raw: isize) -> u32 {
            unsafe { GetDpiForWindow(hwnd(raw)) }
        }

        #[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
        pub struct MonitorInfo {
            pub monitor: isize,
            pub flags: u32,
            pub monitor_rect: [i32; 4],
            pub work_rect: [i32; 4],
            pub device: String,
        }

        pub fn monitor_info(raw: isize) -> Option<MonitorInfo> {
            unsafe {
                let monitor: HMONITOR = MonitorFromWindow(hwnd(raw), MONITOR_DEFAULTTONEAREST);
                let mut ex = MONITORINFOEXW::default();
                ex.monitorInfo.cbSize = core::mem::size_of::<MONITORINFOEXW>() as u32;
                let ok =
                    GetMonitorInfoW(monitor, &mut ex as *mut MONITORINFOEXW as *mut MONITORINFO);
                if ok == FALSE {
                    return None;
                }
                Some(MonitorInfo {
                    monitor: monitor.0 as isize,
                    flags: ex.monitorInfo.dwFlags,
                    monitor_rect: rect4(&ex.monitorInfo.rcMonitor),
                    work_rect: rect4(&ex.monitorInfo.rcWork),
                    device: wide_str(&ex.szDevice),
                })
            }
        }

        #[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
        pub struct ModeInfo {
            pub width: u32,
            pub height: u32,
            pub frequency: u32,
            pub bits_per_pel: u32,
        }

        /// Read-only current display mode. `None` reads the primary display.
        pub fn display_mode(device: Option<&str>) -> Option<ModeInfo> {
            unsafe {
                let mut mode: DEVMODEW = core::mem::zeroed();
                mode.dmSize = core::mem::size_of::<DEVMODEW>() as u16;
                let wide: Vec<u16>;
                let name = match device {
                    Some(name) => {
                        wide = name.encode_utf16().chain([0]).collect();
                        PCWSTR(wide.as_ptr())
                    }
                    None => PCWSTR::null(),
                };
                let ok = EnumDisplaySettingsW(name, ENUM_CURRENT_SETTINGS, &mut mode);
                if ok == FALSE {
                    return None;
                }
                Some(ModeInfo {
                    width: mode.dmPelsWidth,
                    height: mode.dmPelsHeight,
                    frequency: mode.dmDisplayFrequency,
                    bits_per_pel: mode.dmBitsPerPel,
                })
            }
        }

        pub fn create_device_and_swapchain(
            raw: isize,
            width: u32,
            height: u32,
        ) -> (i32, Option<SwapPair>) {
            let desc = DXGI_SWAP_CHAIN_DESC {
                BufferDesc: DXGI_MODE_DESC {
                    Width: width,
                    Height: height,
                    RefreshRate: DXGI_RATIONAL {
                        Numerator: 0,
                        Denominator: 0,
                    },
                    Format: DXGI_FORMAT_R8G8B8A8_UNORM,
                    ScanlineOrdering: DXGI_MODE_SCANLINE_ORDER_UNSPECIFIED,
                    Scaling: DXGI_MODE_SCALING_UNSPECIFIED,
                },
                SampleDesc: DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                },
                BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
                BufferCount: 1,
                OutputWindow: hwnd(raw),
                Windowed: TRUE,
                SwapEffect: DXGI_SWAP_EFFECT_DISCARD,
                Flags: 0,
            };
            let mut chain: Option<IDXGISwapChain> = None;
            let mut device: Option<ID3D11Device> = None;
            let mut level = D3D_FEATURE_LEVEL::default();
            let mut context: Option<ID3D11DeviceContext> = None;
            let hr = unsafe {
                D3D11CreateDeviceAndSwapChain(
                    None::<&IDXGIAdapter>,
                    D3D_DRIVER_TYPE_HARDWARE,
                    HMODULE(core::ptr::null_mut()),
                    D3D11_CREATE_DEVICE_FLAG(0),
                    None,
                    D3D11_SDK_VERSION,
                    Some(&desc as *const DXGI_SWAP_CHAIN_DESC),
                    Some(&mut chain as *mut Option<IDXGISwapChain>),
                    Some(&mut device as *mut Option<ID3D11Device>),
                    Some(&mut level as *mut D3D_FEATURE_LEVEL),
                    Some(&mut context as *mut Option<ID3D11DeviceContext>),
                )
            };
            match hr {
                Ok(()) => (
                    0,
                    chain.map(|chain| SwapPair {
                        chain,
                        device,
                        context,
                    }),
                ),
                Err(err) => (err.code().0, None),
            }
        }

        /// One minimal windowed present of the uninitialized backbuffer:
        /// proves presentation mechanics only, never content. Raw HRESULT.
        pub fn present(chain: &IDXGISwapChain) -> i32 {
            unsafe { chain.Present(1, DXGI_PRESENT(0)).0 }
        }

        pub fn set_fullscreen(chain: &IDXGISwapChain, fullscreen: bool) -> i32 {
            // `None` selects the current output; no ResizeTarget or mode
            // change is ever issued.
            match unsafe { chain.SetFullscreenState(fullscreen, None::<&IDXGIOutput>) } {
                Ok(()) => 0,
                Err(err) => err.code().0,
            }
        }

        pub fn get_fullscreen(chain: &IDXGISwapChain) -> (i32, Option<bool>, Option<bool>) {
            let mut state = BOOL::default();
            let mut target: Option<IDXGIOutput> = None;
            match unsafe {
                chain.GetFullscreenState(
                    Some(&mut state as *mut BOOL),
                    Some(&mut target as *mut Option<IDXGIOutput>),
                )
            } {
                Ok(()) => (0, Some(state.as_bool()), Some(target.is_some())),
                Err(err) => (err.code().0, None, None),
            }
        }

        #[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
        pub struct OutputDesc {
            pub device: String,
            pub desktop_rect: [i32; 4],
            pub attached: bool,
            pub monitor: isize,
        }

        pub fn containing_output(chain: &IDXGISwapChain) -> Option<OutputDesc> {
            unsafe {
                let output = chain.GetContainingOutput().ok()?;
                let desc = output.GetDesc().ok()?;
                Some(OutputDesc {
                    device: wide_str(&desc.DeviceName),
                    desktop_rect: rect4(&desc.DesktopCoordinates),
                    attached: desc.AttachedToDesktop.as_bool(),
                    monitor: desc.Monitor.0 as isize,
                })
            }
        }
    }
}

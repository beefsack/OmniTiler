#![allow(unsafe_code)]
//! Windows-only native boundary for `tiler-live-fixture`.
//!
//! Sole `unsafe` owner in this crate: the approved Windows FFI root shared
//! with the `win-probe` and `win-dxgi-probe` examples. Every Win32 call,
//! raw-pointer use, and handle/SID/descriptor lifetime lives here; the
//! transport layer (`transport.rs`), the examples, and everything else use
//! this module through safe APIs that return `io::Result` and owned values,
//! never raw pointers or validity obligations. Buffers are allocated here
//! and freed here; fallible reads fail closed with the OS error, never
//! guessed values.
//!
//! Why this module exists instead of a safe dependency: the preferred
//! maintained pipe crate (`interprocess`) exposes a safe owner-restriction
//! API but pulls `0BSD`-only transitive crates (`doctest-file`, `recvmsg`)
//! that the workspace license gate rejects, and the policy stays unchanged.
//! Tokio (MIT, maintained) supplies the safe async pipe API but deliberately
//! leaves the security descriptor to the caller
//! (`create_with_security_attributes_raw`), so this module builds exactly
//! that one descriptor plus the two small Win32 helpers the transport needs
//! (current-user SID, pipe wait).
//!
//! Fixture-only live probes add reviewed wrappers in two child modules
//! (`ffi_ext` for `win-probe`, `dxgi` for `win-dxgi-probe`). They are safe
//! signatures over plain data and raw HRESULTs, live under this file's
//! reviewed `allow(unsafe_code)` (child modules inherit it), and are never
//! linked into product code.
//!
//! Owner semantics: the DACL names the literal current-user SID plus
//! `SYSTEM`, never the `OW` (Owner Rights, `S-1-3-4`) pseudo-SID. `OW`
//! resolves to the object's owner at access-check time: for an elevated
//! server the owner may be Administrators rather than the invoking user,
//! and Owner Rights can be curtailed by policy. A literal user SID in a
//! protected DACL, with the descriptor owner field set to the same SID, is
//! the precise owner-only boundary. The per-run nonce still guards every
//! request on top.

use std::ffi::OsStr;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::ptr::null_mut;

use tokio::net::windows::named_pipe::{NamedPipeServer, PipeMode, ServerOptions};
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, LocalFree};
use windows_sys::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows_sys::Win32::Security::{
    GetTokenInformation, PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER,
    TokenUser,
};
use windows_sys::Win32::System::Pipes::WaitNamedPipeW;
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

/// Encode an `OsStr` as NUL-terminated UTF-16 for Win32 `PCWSTR` parameters.
fn wide_nul(s: &OsStr) -> Vec<u16> {
    s.encode_wide().chain([0]).collect()
}

/// Current process user SID as a string (`S-1-5-21-...`).
///
/// Reads the process token (`OpenProcessToken` + `GetTokenInformation`
/// with `TokenUser`) and renders the SID. Every fallible step returns the
/// OS error; the token handle and SID string are released on all paths.
pub fn current_user_sid() -> io::Result<String> {
    // SAFETY: all pointers are valid for their call duration (stack slots,
    // caller-owned buffers sized by the length probe); handles and heap
    // strings are released below on every path, exactly once.
    unsafe {
        let mut token: HANDLE = null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return Err(io::Error::last_os_error());
        }
        // Length probe: expected to fail; `len` receives the size needed.
        let mut len: u32 = 0;
        GetTokenInformation(token, TokenUser, null_mut(), 0, &mut len);
        if len == 0 {
            CloseHandle(token);
            return Err(io::Error::other("GetTokenInformation gave no token size"));
        }
        // Aligned backing store: `TOKEN_USER` is pointer-sized fields only,
        // so `align_of::<TOKEN_USER>() <= align_of::<usize>()` (const-proven
        // below) and a `Vec<usize>` rounded up to `len` bytes satisfies the
        // alignment for the `cast::<TOKEN_USER>` read. A `Vec<u8>` would
        // only guarantee alignment 1: dereferencing the header through it
        // would be undefined behavior. No generic allocation helper is
        // introduced: this is the single site that reads a Win32 struct
        // out of a length-probed buffer.
        const _: () = assert!(std::mem::align_of::<TOKEN_USER>() <= std::mem::align_of::<usize>());
        let words = (len as usize).div_ceil(std::mem::size_of::<usize>());
        let mut buf = vec![0usize; words];
        let cap = (words * std::mem::size_of::<usize>()) as u32;
        if GetTokenInformation(
            token,
            TokenUser,
            buf.as_mut_ptr() as *mut core::ffi::c_void,
            cap,
            &mut len,
        ) == 0
        {
            let err = io::Error::last_os_error();
            CloseHandle(token);
            return Err(err);
        }
        let sid = (*buf.as_ptr().cast::<TOKEN_USER>()).User.Sid;
        let mut text: *mut u16 = null_mut();
        if ConvertSidToStringSidW(sid, &mut text) == 0 {
            let err = io::Error::last_os_error();
            CloseHandle(token);
            return Err(err);
        }
        let mut out = Vec::new();
        let mut i: usize = 0;
        loop {
            let w = *text.add(i);
            if w == 0 {
                break;
            }
            out.push(w);
            i += 1;
        }
        LocalFree(text as *mut core::ffi::c_void);
        CloseHandle(token);
        String::from_utf16(&out)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "user SID is not UTF-16"))
    }
}

/// Owned absolute-format security descriptor: owner-only DACL.
pub struct PipeSecurity {
    sd: PSECURITY_DESCRIPTOR,
}

// SAFETY: the pointer owns a heap blob from
// `ConvertStringSecurityDescriptorToSecurityDescriptorW`, freed once in
// `Drop`. It is never aliased mutably and never dereferenced outside the
// creating Win32 call, so sharing it across threads is sound.
unsafe impl Send for PipeSecurity {}
unsafe impl Sync for PipeSecurity {}

impl Drop for PipeSecurity {
    fn drop(&mut self) {
        // SAFETY: `sd` came from the converter and is freed exactly once here.
        unsafe {
            LocalFree(self.sd);
        }
    }
}

impl PipeSecurity {
    /// Descriptor with owner field = current user and protected DACL
    /// granting generic-all to exactly the current-user SID and `SYSTEM`.
    /// Everyone else is denied by omission; nothing is inherited.
    pub fn owner_only() -> io::Result<Self> {
        let sid = current_user_sid()?;
        let sddl = format!("O:{sid}D:P(A;;GA;;;{sid})(A;;GA;;;SY)");
        let wide = wide_nul(OsStr::new(&sddl));
        // SAFETY: `wide` is NUL-terminated and lives for the call; `sd`/`len`
        // are valid out-slots. On success `sd` owns a heap blob transferred
        // to `Self` (freed in `Drop`); on failure nothing is owned.
        unsafe {
            let mut sd: PSECURITY_DESCRIPTOR = null_mut();
            let mut len: u32 = 0;
            if ConvertStringSecurityDescriptorToSecurityDescriptorW(
                wide.as_ptr(),
                SDDL_REVISION_1,
                &mut sd,
                &mut len,
            ) == 0
            {
                return Err(io::Error::last_os_error());
            }
            if sd.is_null() {
                return Err(io::Error::other(
                    "security descriptor converter returned null",
                ));
            }
            Ok(Self { sd })
        }
    }

    /// `SECURITY_ATTRIBUTES` referencing the owned descriptor. The returned
    /// value borrows `self`; it must not outlive the creating call (callers
    /// pass it straight into the pipe create). Filling the struct copies a
    /// pointer but dereferences nothing, so this is safe.
    pub fn attributes(&self) -> SECURITY_ATTRIBUTES {
        SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: self.sd,
            bInheritHandle: 0,
        }
    }
}

/// Create one pipe server instance with the owner-only descriptor.
///
/// `first` selects `FILE_FLAG_FIRST_PIPE_INSTANCE`: the very first create
/// fails when another server already holds the name (exclusive bind);
/// later instances for concurrent clients omit it. Requires a Tokio
/// runtime with I/O enabled on the calling thread (provided by the
/// transport's `block_on`); a missing runtime panics inside Tokio, which is
/// a programming error, not memory unsafety.
pub fn create_server(
    pipe_name: &OsStr,
    first: bool,
    sec: &PipeSecurity,
) -> io::Result<NamedPipeServer> {
    let mut opts = ServerOptions::new();
    opts.first_pipe_instance(first)
        .reject_remote_clients(true)
        .pipe_mode(PipeMode::Byte);
    let mut attrs = sec.attributes();
    // SAFETY: `attrs` points at a valid `SECURITY_ATTRIBUTES` whose
    // descriptor (`sec`) outlives this call; Tokio copies the descriptor
    // during creation. The raw-pointer handoff is the only unsafe act.
    unsafe {
        opts.create_with_security_attributes_raw(
            pipe_name,
            (&mut attrs as *mut SECURITY_ATTRIBUTES) as *mut core::ffi::c_void,
        )
    }
}

/// Block (kernel wait, no polling) until a pipe instance is available or
/// `timeout` passes. Used to bound client connects.
pub fn wait_for_pipe(pipe_name: &OsStr, timeout_ms: u32) -> io::Result<()> {
    let wide = wide_nul(pipe_name);
    // SAFETY: `wide` is NUL-terminated and lives for the call.
    unsafe {
        if WaitNamedPipeW(wide.as_ptr(), timeout_ms) == 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

/// Fixture-only Win32 gap helpers for `win-probe` (checked exstyle read,
/// title text, client rect, virtual desktop, synthetic mouse, DWM cloak).
pub mod ffi_ext {
    use windows_sys::Win32::Foundation::{GetLastError, SetLastError};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GetWindowLongW, GetWindowTextLengthW, GetWindowTextW,
    };

    /// Checked exstyle read mirroring `window_style_checked`: `None`
    /// only when the word reads zero with a live error.
    pub fn window_exstyle_checked(hwnd: u64) -> Option<u32> {
        // SAFETY: `hwnd` is an opaque handle supplied by the caller; a
        // stale/invalid value makes the API fail with a live error, which
        // is reported as `None` rather than guessed. No pointers escape.
        unsafe {
            SetLastError(0);
            let value = GetWindowLongW(hwnd as isize as _, GWL_EXSTYLE);
            let err = GetLastError();
            if value == 0 && err != 0 {
                None
            } else {
                Some(value as u32)
            }
        }
    }

    /// Best-effort window title text; `None` when unreadable.
    /// Never used as a write vector: join evidence only.
    pub fn window_text(hwnd: u64) -> Option<String> {
        // SAFETY: the length probe only reads window metadata for a caller
        // handle; a dead handle yields 0/error and is handled below.
        let len = unsafe { GetWindowTextLengthW(hwnd as isize as _) };
        if len < 0 {
            return None;
        }
        // Length excludes the NUL; empty titles read as Some("").
        let mut buf = vec![0u16; (len as usize) + 1];
        // SAFETY: `buf` has room for the reported length plus the NUL, and
        // the call writes at most `buf.len()` UTF-16 code units.
        let copied =
            unsafe { GetWindowTextW(hwnd as isize as _, buf.as_mut_ptr(), buf.len() as i32) };
        if copied == 0 {
            // SAFETY: thread-local last-error read; no pointer arguments.
            let err = unsafe { GetLastError() };
            if err != 0 {
                return None;
            }
            return Some(String::new());
        }
        Some(String::from_utf16_lossy(&buf[..copied as usize]))
    }

    /// Screen-space client rect via GetClientRect + ClientToScreen
    /// mapping of both corners. `None` when the window is gone or the
    /// mapping fails. Read-only geometry evidence.
    pub fn client_rect_screen(hwnd: u64) -> Option<(i32, i32, i32, i32)> {
        use windows_sys::Win32::Foundation::POINT;
        use windows_sys::Win32::Graphics::Gdi::ClientToScreen;
        use windows_sys::Win32::UI::WindowsAndMessaging::GetClientRect;
        // SAFETY: `rc`/`top_left`/`bottom_right` are valid stack out-slots
        // for their calls; the handle is caller-supplied and failure is
        // mapped to `None`.
        unsafe {
            let mut rc: windows_sys::Win32::Foundation::RECT = std::mem::zeroed();
            if GetClientRect(hwnd as isize as _, &mut rc) == 0 {
                return None;
            }
            let mut top_left = POINT {
                x: rc.left,
                y: rc.top,
            };
            let mut bottom_right = POINT {
                x: rc.right,
                y: rc.bottom,
            };
            if ClientToScreen(hwnd as isize as _, &mut top_left) == 0 {
                return None;
            }
            if ClientToScreen(hwnd as isize as _, &mut bottom_right) == 0 {
                return None;
            }
            Some((top_left.x, top_left.y, bottom_right.x, bottom_right.y))
        }
    }

    /// Virtual-desktop rect `(x, y, w, h)` via `GetSystemMetrics`.
    /// Read-only; used to normalize absolute `SendInput` coordinates.
    pub fn virtual_desktop_px() -> Option<(i32, i32, i32, i32)> {
        use windows_sys::Win32::UI::WindowsAndMessaging::GetSystemMetrics;
        const SM_XVIRTUALSCREEN: i32 = 76;
        const SM_YVIRTUALSCREEN: i32 = 77;
        const SM_CXVIRTUALSCREEN: i32 = 78;
        const SM_CYVIRTUALSCREEN: i32 = 79;
        // SAFETY: plain metric reads with scalar arguments and results.
        unsafe {
            let x = GetSystemMetrics(SM_XVIRTUALSCREEN);
            let y = GetSystemMetrics(SM_YVIRTUALSCREEN);
            let w = GetSystemMetrics(SM_CXVIRTUALSCREEN);
            let h = GetSystemMetrics(SM_CYVIRTUALSCREEN);
            if w <= 0 || h <= 0 {
                return None;
            }
            Some((x, y, w, h))
        }
    }

    /// One `SendInput` mouse record. Returns the accepted-event count
    /// (0 when the API refused it). Sole synthetic-input write vector in
    /// this fixture; every caller re-verifies guards immediately before
    /// each batch and releases the button in a RAII guard.
    pub fn send_mouse_input(dx: i32, dy: i32, mouse_data: u32, flags: u32) -> u32 {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
            INPUT, INPUT_MOUSE, MOUSEINPUT, SendInput,
        };
        // SAFETY: one zeroed `INPUT` is initialized for the mouse union
        // before the call, and `SendInput` reads exactly one record of the
        // declared size. The record is caller-owned stack memory.
        unsafe {
            let mut input: INPUT = std::mem::zeroed();
            input.r#type = INPUT_MOUSE;
            input.Anonymous.mi = MOUSEINPUT {
                dx,
                dy,
                mouseData: mouse_data,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            };
            let size = std::mem::size_of::<INPUT>() as i32;
            SendInput(1, &input, size)
        }
    }

    /// Show-command constants for the hide/reveal leg (own fixture only).
    pub const SW_HIDE: i32 = 0;
    pub const SW_SHOWNA: i32 = 8;

    /// Public `DwmSetWindowAttribute(DWMWA_CLOAK)` write probe. Returns
    /// the raw HRESULT (0 == S_OK). The caller records the HRESULT plus
    /// the native `cloaked()` readback before/after and restores 0.
    pub fn dwm_set_cloak(hwnd: u64, cloak: bool) -> i32 {
        use windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute;
        const DWMWA_CLOAK: u32 = 14;
        let value: i32 = if cloak { 1 } else { 0 };
        // SAFETY: `value` is a valid `i32` for the duration of the call and
        // the size matches the attribute's expected payload.
        unsafe {
            DwmSetWindowAttribute(
                hwnd as isize as _,
                DWMWA_CLOAK,
                (&value as *const i32).cast(),
                std::mem::size_of::<i32>() as u32,
            )
        }
    }
}

/// Fixture-only DXGI fullscreen-request helpers for `win-dxgi-probe`
/// (own device/swapchain; never attached to a fixture HWND).
pub mod dxgi {
    use windows::Win32::Foundation::{FALSE, HMODULE, HWND, RECT, TRUE};
    use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE_HARDWARE, D3D_FEATURE_LEVEL};
    use windows::Win32::Graphics::Direct3D11::{
        D3D11_CREATE_DEVICE_FLAG, D3D11_SDK_VERSION, D3D11CreateDeviceAndSwapChain, ID3D11Device,
        ID3D11DeviceContext,
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
                // SAFETY: COM method on a live interface; the best-effort
                // result is intentionally ignored, and the explicit restore
                // records HRESULTs.
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
        // SAFETY: `rect` is a valid stack out-slot for the call; a missing
        // window is reported as `None`.
        unsafe {
            GetWindowRect(hwnd(raw), &mut rect)
                .ok()
                .map(|()| rect4(&rect))
        }
    }

    pub fn foreground() -> isize {
        // SAFETY: scalar handle read with no arguments.
        unsafe { GetForegroundWindow().0 as isize }
    }

    /// Best-effort foreground restore without synthetic input; returns
    /// whether the OS granted the request. Never targets a null handle.
    pub fn set_foreground(raw: isize) -> bool {
        if raw == 0 {
            return false;
        }
        // SAFETY: non-null caller handle; a dead handle is a no-op failure.
        unsafe { SetForegroundWindow(hwnd(raw)) != FALSE }
    }

    /// Point-in-time native existence check; call only after drop.
    pub fn is_window(raw: isize) -> bool {
        if raw == 0 {
            return false;
        }
        // SAFETY: non-null caller handle; stale handles return false.
        unsafe { IsWindow(Some(hwnd(raw))) != FALSE }
    }

    pub fn dpi(raw: isize) -> u32 {
        // SAFETY: scalar read for a caller handle; dead handles return 0.
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
        // SAFETY: `ex` is a zeroed `MONITORINFOEXW` with its `cbSize` set
        // before the read; the monitor handle is derived from the caller
        // window. Failure is mapped to `None`; the device string is copied
        // out of the fixed buffer.
        unsafe {
            let monitor: HMONITOR = MonitorFromWindow(hwnd(raw), MONITOR_DEFAULTTONEAREST);
            let mut ex = MONITORINFOEXW::default();
            ex.monitorInfo.cbSize = core::mem::size_of::<MONITORINFOEXW>() as u32;
            let ok = GetMonitorInfoW(monitor, &mut ex as *mut MONITORINFOEXW as *mut MONITORINFO);
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
        // SAFETY: `mode` is a zeroed `DEVMODEW` with `dmSize` set; the
        // optional device name is NUL-terminated and outlives the call.
        // Failure is mapped to `None`.
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
        // SAFETY: `desc` and every out-slot are valid stack locations for
        // the call; the out-params are initialized to `None` so a failed
        // create cannot surface a half-built pair. HRESULT is returned raw.
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
        // SAFETY: COM method on a live interface supplied by the caller.
        unsafe { chain.Present(1, DXGI_PRESENT(0)).0 }
    }

    pub fn set_fullscreen(chain: &IDXGISwapChain, fullscreen: bool) -> i32 {
        // `None` selects the current output; no ResizeTarget or mode
        // change is ever issued.
        // SAFETY: COM method on a live interface supplied by the caller.
        match unsafe { chain.SetFullscreenState(fullscreen, None::<&IDXGIOutput>) } {
            Ok(()) => 0,
            Err(err) => err.code().0,
        }
    }

    pub fn get_fullscreen(chain: &IDXGISwapChain) -> (i32, Option<bool>, Option<bool>) {
        let mut state = BOOL::default();
        let mut target: Option<IDXGIOutput> = None;
        // SAFETY: both out-slots are valid stack locations; the interface
        // is live because the caller owns it. Failure returns `None`s.
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
        // SAFETY: COM getter on a live interface; the returned descriptor
        // is copied into owned fields before the temporary output is
        // dropped. Failure is mapped to `None`.
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

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::Security::Authorization::{GetSecurityInfo, SE_KERNEL_OBJECT};
    use windows_sys::Win32::Security::{
        ACCESS_ALLOWED_ACE, ACL, ACL_SIZE_INFORMATION, AclSizeInformation,
        DACL_SECURITY_INFORMATION, GetAce, GetAclInformation, OWNER_SECURITY_INFORMATION, PSID,
    };

    #[test]
    fn owner_sid_is_a_literal_account_sid() {
        let sid = current_user_sid().expect("process token must yield a user SID");
        // A concrete account SID (`S-1-5-21-...`), never the Owner Rights
        // pseudo-SID (`S-1-3-4`) the old SDDL relied on, and never SYSTEM.
        assert!(sid.starts_with("S-1-5-21-"), "unexpected owner SID: {sid}");
        assert_ne!(sid, "S-1-3-4");
        assert_ne!(sid, "S-1-5-18");
        assert!(sid.split('-').count() > 4);
    }

    /// `ACCESS_ALLOWED_ACE_TYPE` (`Win32::System::SystemServices`) without
    /// pulling that feature: the Win32 value is 0.
    const ACCESS_ALLOW_TYPE: u8 = 0;

    #[test]
    fn pipe_security_is_owner_only_with_literal_sids() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .enable_time()
            .build()
            .expect("test runtime");
        let user = current_user_sid().expect("user SID");
        let name = format!(
            "\\\\.\\pipe\\tiler-live-fixture-sdtest-{}",
            std::process::id()
        );
        let server = rt.block_on(async {
            let sec = PipeSecurity::owner_only().expect("owner-only SD");
            create_server(OsStr::new(&name), true, &sec).expect("first instance")
        });
        // SAFETY: test-only Win32 reads below; all pointers are valid for
        // their call, heap outputs are freed, the pipe handle stays owned
        // by `server` (never closed here). `GetSecurityInfo` reports
        // success with `WIN32_ERROR` 0 (`ERROR_SUCCESS`), unlike the
        // `BOOL`-returning calls elsewhere in this file.
        unsafe {
            use std::os::windows::io::AsRawHandle;
            let handle: HANDLE = server.as_raw_handle();
            let mut owner: PSID = null_mut();
            let mut dacl: *mut ACL = null_mut();
            let mut sd: PSECURITY_DESCRIPTOR = null_mut();
            assert_eq!(
                GetSecurityInfo(
                    handle,
                    SE_KERNEL_OBJECT,
                    OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
                    &mut owner,
                    null_mut(),
                    &mut dacl,
                    null_mut(),
                    &mut sd,
                ),
                0
            );
            assert!(!owner.is_null() && !dacl.is_null() && !sd.is_null());

            // Owner field is literally the current user.
            let mut text: *mut u16 = null_mut();
            assert_ne!(ConvertSidToStringSidW(owner, &mut text), 0);
            let mut got = Vec::new();
            let mut i = 0;
            loop {
                let w = *text.add(i);
                if w == 0 {
                    break;
                }
                got.push(w);
                i += 1;
            }
            LocalFree(text as *mut core::ffi::c_void);
            assert_eq!(String::from_utf16(&got).unwrap(), user);

            // DACL holds exactly two allow ACEs: user + SYSTEM, nothing else.
            // A typed `ACL_SIZE_INFORMATION` out-buffer: no hand parsing of
            // the header layout.
            let mut info = ACL_SIZE_INFORMATION {
                AceCount: 0,
                AclBytesInUse: 0,
                AclBytesFree: 0,
            };
            assert_ne!(
                GetAclInformation(
                    dacl.cast_const(),
                    &mut info as *mut ACL_SIZE_INFORMATION as *mut core::ffi::c_void,
                    std::mem::size_of::<ACL_SIZE_INFORMATION>() as u32,
                    AclSizeInformation,
                ),
                0
            );
            let ace_count = info.AceCount;
            assert_eq!(ace_count, 2, "owner-only DACL must hold user + SYSTEM");
            let mut sids = Vec::new();
            for index in 0..ace_count {
                let mut ace: *mut core::ffi::c_void = null_mut();
                assert_ne!(GetAce(dacl.cast_const(), index, &mut ace), 0);
                let ace = &*(ace as *const ACCESS_ALLOWED_ACE);
                assert_eq!(ace.Header.AceType, ACCESS_ALLOW_TYPE);
                let sid_ptr = std::ptr::addr_of!(ace.SidStart) as PSID;
                let mut text: *mut u16 = null_mut();
                assert_ne!(ConvertSidToStringSidW(sid_ptr, &mut text), 0);
                let mut s = Vec::new();
                let mut i = 0;
                loop {
                    let w = *text.add(i);
                    if w == 0 {
                        break;
                    }
                    s.push(w);
                    i += 1;
                }
                LocalFree(text as *mut core::ffi::c_void);
                sids.push(String::from_utf16(&s).unwrap());
            }
            sids.sort();
            let mut expected = vec![user, "S-1-5-18".to_string()];
            expected.sort();
            assert_eq!(sids, expected);
            LocalFree(sd);
        }
        drop(server);
    }

    #[test]
    fn flagged_first_instance_is_exclusive() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .enable_time()
            .build()
            .expect("test runtime");
        let name = format!(
            "\\\\.\\pipe\\tiler-live-fixture-first-{}-{}",
            std::process::id(),
            "exclusive"
        );
        rt.block_on(async {
            let sec = PipeSecurity::owner_only().expect("owner-only SD");
            let _first = create_server(OsStr::new(&name), true, &sec).expect("first instance");
            let err = create_server(OsStr::new(&name), true, &sec).unwrap_err();
            assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);
        });
    }
}

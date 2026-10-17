//! Message-only and owned overlapped test windows (test support only).
//!
//! Class registration (with internal procedures) and the window handles are
//! owned here; safe test code receives opaque values plus numeric ids. The
//! overlapped class is fixed, never caller-supplied.

use std::rc::Rc;

use windows_sys::Win32::Foundation::{
    ERROR_CLASS_ALREADY_EXISTS, GetLastError, HWND, LPARAM, LRESULT, WPARAM,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, HWND_MESSAGE, PostQuitMessage, RegisterClassW,
    WM_CLOSE, WM_DESTROY, WM_TIMER, WNDCLASSW,
};

use crate::test_window::TEST_WINDOW_CLASS;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain([0]).collect()
}

unsafe extern "system" fn test_wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
}

/// Owned message-only window; drop destroys it. Confined to the creating
/// thread: neither [`Send`] nor [`Sync`].
///
/// ```compile_fail
/// fn assert_send<T: Send>() {}
/// assert_send::<tiler_windows::ffi::TestWindow>();
/// ```
///
/// ```compile_fail
/// fn assert_sync<T: Sync>() {}
/// assert_sync::<tiler_windows::ffi::TestWindow>();
/// ```
pub struct TestWindow {
    hwnd: u64,
    _confined: std::marker::PhantomData<Rc<()>>,
}

impl TestWindow {
    /// Register `class_name` (tolerating a previous registration) and
    /// create one invisible message-only window; `None` when creation
    /// fails. No other window is touched.
    pub fn create_message_only(class_name: &str, title: &str) -> Option<Self> {
        let class_w = wide(class_name);
        let title_w = wide(title);
        let hinst = unsafe { GetModuleHandleW(std::ptr::null()) };
        let mut cls: WNDCLASSW = unsafe { std::mem::zeroed() };
        cls.lpfnWndProc = Some(test_wndproc);
        cls.hInstance = hinst;
        cls.lpszClassName = class_w.as_ptr();
        let atom = unsafe { RegisterClassW(&cls) };
        if atom == 0 && unsafe { GetLastError() } != ERROR_CLASS_ALREADY_EXISTS {
            return None;
        }
        let hwnd = unsafe {
            CreateWindowExW(
                0,
                class_w.as_ptr(),
                title_w.as_ptr(),
                0,
                0,
                0,
                0,
                0,
                HWND_MESSAGE,
                std::ptr::null_mut(),
                hinst,
                std::ptr::null_mut(),
            )
        };
        if hwnd.is_null() {
            return None;
        }
        Some(Self {
            hwnd: hwnd as usize as u64,
            _confined: std::marker::PhantomData,
        })
    }

    pub fn hwnd(&self) -> u64 {
        self.hwnd
    }
}

impl Drop for TestWindow {
    fn drop(&mut self) {
        unsafe {
            DestroyWindow(self.hwnd as usize as HWND);
        }
    }
}

unsafe extern "system" fn owned_wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_CLOSE | WM_TIMER => {
            unsafe { DestroyWindow(hwnd) };
            0
        }
        WM_DESTROY => {
            unsafe { PostQuitMessage(0) };
            0
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    }
}

/// Pump failure with the previous vocabulary: timer setup or the message
/// read itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PumpError {
    Timer,
    Pump,
}

impl PumpError {
    pub fn message(self) -> &'static str {
        match self {
            Self::Timer => "SetTimer failed",
            Self::Pump => "GetMessage failed",
        }
    }
}

/// Owned-window setup failure with the previous vocabulary: class
/// registration or window creation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OwnedCreateError {
    Register,
    Window,
}

impl OwnedCreateError {
    pub fn message(self) -> &'static str {
        match self {
            Self::Register => "RegisterClass failed",
            Self::Window => "CreateWindow failed",
        }
    }
}
/// Owned overlapped test window (test support only); drop destroys it.
/// The procedure closes on `WM_CLOSE`/timer and quits the pump on destroy;
/// paint, activation, and focus behavior are the caller's (it shows with an
/// explicit non-activating command and verifies with a readback). Confined
/// to the creating thread like all owned windows here: neither [`Send`]
/// nor [`Sync`].
///
/// ```compile_fail
/// fn assert_send<T: Send>() {}
/// assert_send::<tiler_windows::ffi::FfiOwnedTestWindow>();
/// ```
///
/// ```compile_fail
/// fn assert_sync<T: Sync>() {}
/// assert_sync::<tiler_windows::ffi::FfiOwnedTestWindow>();
/// ```
pub struct OwnedWindow {
    hwnd: u64,
    _confined: std::marker::PhantomData<Rc<()>>,
}

impl OwnedWindow {
    /// Register the owned test class (tolerating a previous registration)
    /// and create one overlapped window at the given bounds, visible only
    /// when asked. The class is fixed (never caller-supplied) so creation
    /// can never pair an unrelated existing window procedure with this
    /// carrier. Errors distinguish registration from creation with the
    /// previous vocabulary; no other window is touched.
    pub fn create(
        title: &str,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        visible: bool,
    ) -> Result<Self, OwnedCreateError> {
        Self::create_inner(title, x, y, w, h, visible, false)
    }

    /// Invisible overlapped window with the `TOPMOST` band set (test
    /// support for band-gate classification); otherwise identical to
    /// [`create`](Self::create). The fixed class keeps the same window
    /// procedure.
    pub fn create_topmost(
        title: &str,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
    ) -> Result<Self, OwnedCreateError> {
        Self::create_inner(title, x, y, w, h, false, true)
    }

    fn create_inner(
        title: &str,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        visible: bool,
        topmost: bool,
    ) -> Result<Self, OwnedCreateError> {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            CreateWindowExW, RegisterClassW, WNDCLASSW, WS_EX_TOPMOST, WS_OVERLAPPEDWINDOW,
            WS_VISIBLE,
        };
        let class_w = wide(TEST_WINDOW_CLASS);
        let title_w = wide(title);
        let hinst = unsafe { GetModuleHandleW(std::ptr::null()) };
        let mut cls: WNDCLASSW = unsafe { std::mem::zeroed() };
        cls.lpfnWndProc = Some(owned_wndproc);
        cls.hInstance = hinst;
        cls.lpszClassName = class_w.as_ptr();
        let atom = unsafe { RegisterClassW(&cls) };
        if atom == 0 && unsafe { GetLastError() } != ERROR_CLASS_ALREADY_EXISTS {
            return Err(OwnedCreateError::Register);
        }
        let style = if visible {
            WS_OVERLAPPEDWINDOW | WS_VISIBLE
        } else {
            WS_OVERLAPPEDWINDOW
        };
        let exstyle = if topmost { WS_EX_TOPMOST } else { 0 };
        let hwnd = unsafe {
            CreateWindowExW(
                exstyle,
                class_w.as_ptr(),
                title_w.as_ptr(),
                style,
                x,
                y,
                w,
                h,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                hinst,
                std::ptr::null_mut(),
            )
        };
        if hwnd.is_null() {
            return Err(OwnedCreateError::Window);
        }
        Ok(Self {
            hwnd: hwnd as usize as u64,
            _confined: std::marker::PhantomData,
        })
    }

    pub fn hwnd(&self) -> u64 {
        self.hwnd
    }

    /// Arm a one-shot `timer_ms` lifetime timer and pump messages until the
    /// window quits itself (`WM_CLOSE`, timer expiry, or an owner close).
    pub fn run_message_loop(&self, timer_ms: u32) -> Result<(), PumpError> {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            DispatchMessageW, GetMessageW, MSG, SetTimer, TranslateMessage,
        };
        let hwnd = self.hwnd as usize as HWND;
        let timer = unsafe { SetTimer(hwnd, 1, timer_ms, None) };
        if timer == 0 {
            return Err(PumpError::Timer);
        }
        let mut msg: MSG = unsafe { std::mem::zeroed() };
        loop {
            let r = unsafe { GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) };
            if r == 0 {
                break;
            }
            if r == -1 {
                return Err(PumpError::Pump);
            }
            unsafe {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        Ok(())
    }
}

impl Drop for OwnedWindow {
    fn drop(&mut self) {
        unsafe {
            DestroyWindow(self.hwnd as usize as HWND);
        }
    }
}

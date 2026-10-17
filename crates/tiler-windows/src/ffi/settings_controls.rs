//! Settings child-control Win32 primitives (no policy).
//!
//! Every text/check/enable/listbox/creation/font call lives here. Buffers
//! are owned here; callers pass opaque window handles and plain values only.
//! The programmatic-push suppression flag lives here so `set_text` pushes
//! never mark the draft dirty: the owner procedure checks it first.
//!
//! The handles crossing these wrappers are owned by the settings windows in
//! this boundary (never caller-supplied arbitrary pointers), so the raw
//! handle parameters stay safe wrappers rather than `unsafe fn`.

// Allow `clippy::not_unsafe_ptr_arg_deref` for the reason above.
#![allow(clippy::not_unsafe_ptr_arg_deref)]

use std::sync::atomic::{AtomicBool, Ordering};

use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::Graphics::Gdi::HFONT;

static SUPPRESS_NOTIFY: AtomicBool = AtomicBool::new(false);

pub(crate) fn notifications_suppressed() -> bool {
    SUPPRESS_NOTIFY.load(Ordering::SeqCst)
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain([0]).collect()
}

pub fn set_text(hwnd: HWND, text: &str) {
    use windows_sys::Win32::UI::WindowsAndMessaging::SetWindowTextW;
    SUPPRESS_NOTIFY.store(true, Ordering::SeqCst);
    let text_w = wide(text);
    unsafe {
        SetWindowTextW(hwnd, text_w.as_ptr());
    }
    SUPPRESS_NOTIFY.store(false, Ordering::SeqCst);
}

pub fn get_text(hwnd: HWND) -> String {
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindowTextLengthW, GetWindowTextW};
    let len = unsafe { GetWindowTextLengthW(hwnd) };
    if len <= 0 {
        return String::new();
    }
    let capped = len.min(512) as usize;
    let mut buf = vec![0u16; capped + 1];
    let read = unsafe { GetWindowTextW(hwnd, buf.as_mut_ptr(), capped as i32 + 1) };
    if read <= 0 {
        return String::new();
    }
    String::from_utf16_lossy(&buf[..read as usize])
}

pub fn checked(hwnd: HWND) -> bool {
    use windows_sys::Win32::UI::Controls::BST_CHECKED;
    use windows_sys::Win32::UI::WindowsAndMessaging::{BM_GETCHECK, SendMessageW};
    unsafe { SendMessageW(hwnd, BM_GETCHECK, 0, 0) == isize::try_from(BST_CHECKED).unwrap_or(1) }
}

pub fn set_checked(hwnd: HWND, value: bool) {
    use windows_sys::Win32::UI::Controls::{BST_CHECKED, BST_UNCHECKED};
    use windows_sys::Win32::UI::WindowsAndMessaging::{BM_SETCHECK, SendMessageW};
    let flag = if value { BST_CHECKED } else { BST_UNCHECKED };
    unsafe {
        SendMessageW(hwnd, BM_SETCHECK, flag as usize, 0);
    }
}

pub fn enable(hwnd: HWND, value: bool) {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::EnableWindow;
    unsafe {
        EnableWindow(hwnd, if value { 1 } else { 0 });
    }
}

pub fn list_reset(hwnd: HWND) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{LB_RESETCONTENT, SendMessageW};
    unsafe {
        SendMessageW(hwnd, LB_RESETCONTENT, 0, 0);
    }
}

pub fn list_add(hwnd: HWND, text: &str) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{LB_ADDSTRING, SendMessageW};
    let text_w = wide(text);
    unsafe {
        SendMessageW(hwnd, LB_ADDSTRING, 0, text_w.as_ptr() as isize);
    }
}

pub fn list_set_sel(hwnd: HWND, index: usize) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{LB_SETCURSEL, SendMessageW};
    unsafe {
        SendMessageW(hwnd, LB_SETCURSEL, index, 0);
    }
}

pub fn list_cur_sel(hwnd: HWND) -> isize {
    use windows_sys::Win32::UI::WindowsAndMessaging::{LB_GETCURSEL, SendMessageW};
    unsafe { SendMessageW(hwnd, LB_GETCURSEL, 0, 0) }
}

pub fn send_list_sel_changed(hwnd: HWND, index: usize) {
    use windows_sys::Win32::UI::WindowsAndMessaging::SendMessageW;
    unsafe {
        SendMessageW(
            hwnd,
            windows_sys::Win32::UI::WindowsAndMessaging::LB_SETCURSEL,
            index,
            0,
        );
    }
}

pub fn set_edit_limit(hwnd: HWND, limit: usize) {
    use windows_sys::Win32::UI::Controls::EM_SETLIMITTEXT;
    use windows_sys::Win32::UI::WindowsAndMessaging::SendMessageW;
    unsafe {
        SendMessageW(hwnd, EM_SETLIMITTEXT, limit, 0);
    }
}

/// Create one child control with the DPI-scaled message font. Returns the
/// new handle, or null on failure (caller aborts like before).
#[allow(clippy::too_many_arguments)]
pub fn create_child(
    parent: HWND,
    hinst: windows_sys::Win32::Foundation::HINSTANCE,
    class: &str,
    text: &str,
    id: u32,
    style: u32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    font: HFONT,
) -> HWND {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, WM_SETFONT, WS_CHILD, WS_CLIPSIBLINGS, WS_VISIBLE,
    };
    let class_w = wide(class);
    let text_w = wide(text);
    let hwnd = unsafe {
        CreateWindowExW(
            0,
            class_w.as_ptr(),
            text_w.as_ptr(),
            WS_CHILD | WS_VISIBLE | WS_CLIPSIBLINGS | style,
            x,
            y,
            w,
            h,
            parent,
            id as isize as windows_sys::Win32::UI::WindowsAndMessaging::HMENU,
            hinst,
            std::ptr::null(),
        )
    };
    if hwnd.is_null() {
        return hwnd;
    }
    if !font.is_null() {
        use windows_sys::Win32::UI::WindowsAndMessaging::SendMessageW;
        unsafe {
            SendMessageW(hwnd, WM_SETFONT, font as usize, 1);
        }
    }
    hwnd
}

pub fn message_font(dpi: u32) -> HFONT {
    use windows_sys::Win32::Graphics::Gdi::{CreateFontW, DEFAULT_GUI_FONT, GetStockObject};
    let height = -((9 * dpi as i32 + 36) / 72);
    let face = wide("Segoe UI");
    let font = unsafe { CreateFontW(height, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, face.as_ptr()) };
    if font.is_null() {
        unsafe { GetStockObject(DEFAULT_GUI_FONT) }
    } else {
        font
    }
}

pub fn show_window(hwnd: HWND, show: bool) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{SW_HIDE, SW_SHOW, ShowWindow};
    unsafe {
        ShowWindow(hwnd, if show { SW_SHOW } else { SW_HIDE });
    }
}

pub fn set_tab_sel(strip: HWND, index: usize) {
    use windows_sys::Win32::UI::Controls::TCM_SETCURSEL;
    use windows_sys::Win32::UI::WindowsAndMessaging::SendMessageW;
    unsafe {
        SendMessageW(strip, TCM_SETCURSEL, index, 0);
    }
}

pub fn tab_cur_sel(strip: HWND) -> isize {
    use windows_sys::Win32::UI::Controls::TCM_GETCURSEL;
    use windows_sys::Win32::UI::WindowsAndMessaging::SendMessageW;
    unsafe { SendMessageW(strip, TCM_GETCURSEL, 0, 0) }
}

pub fn get_focus() -> HWND {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetFocus;
    unsafe { GetFocus() }
}

pub fn set_focus(hwnd: HWND) {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus;
    unsafe {
        SetFocus(hwnd);
    }
}

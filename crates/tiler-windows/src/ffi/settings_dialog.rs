//! Owned modal dialogs for the Settings window (Win32 only).
//!
//! The window procedure, class, loop, and owned `Box<RefCell<DialogBox>>`
//! live here. Outcomes are owned snapshots; safe callers pass title/body/
//! buttons and receive a verdict. Provenance is validated against live
//! registration; unique borrowing is enforced by `RefCell`, so a reentrant
//! callback while a guard is live fails closed instead of aliasing. The
//! outcome channel is cloned under a shared guard and set with no guard
//! held; the nested pump never runs under a live borrow of dialog state.

use std::cell::RefCell;

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{
    Arc, Mutex, OnceLock,
    atomic::{AtomicBool, Ordering},
};

use windows_sys::Win32::Foundation::HWND;

static DIALOG_OPEN: AtomicBool = AtomicBool::new(false);
static CLOSE_DEFERRED: AtomicBool = AtomicBool::new(false);

pub(crate) fn dialog_open() -> bool {
    DIALOG_OPEN.load(Ordering::SeqCst)
}

pub(crate) fn defer_close() {
    CLOSE_DEFERRED.store(true, Ordering::SeqCst);
}

pub(crate) fn take_close_deferred() -> bool {
    CLOSE_DEFERRED.swap(false, Ordering::SeqCst)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewEnd {
    Apply,
    Cancel,
    OwnerGone,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscardEnd {
    Discard,
    Cancel,
    OwnerGone,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ModalExit {
    Choice(u32),
    Dismissed,
    OwnerGone,
}

const DLG_APPLY: u32 = 1;
const DLG_DISCARD: u32 = 1;
const DLG_OK: u32 = 1;
const DLG_CANCEL: u32 = 2;
const STOP_POLL_MS: u32 = 50;
const ESCAPE_VK: usize = 0x1B;

struct DialogBox {
    font: windows_sys::Win32::Graphics::Gdi::HFONT,
    outcome: Arc<Mutex<Option<u32>>>,
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain([0]).collect()
}

fn dialog_font(dpi: u32) -> windows_sys::Win32::Graphics::Gdi::HFONT {
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

fn slots() -> &'static Mutex<HashMap<u64, usize>> {
    static SLOTS: OnceLock<Mutex<HashMap<u64, usize>>> = OnceLock::new();
    SLOTS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn hwnd_key(hwnd: HWND) -> u64 {
    hwnd as usize as u64
}

fn validated(hwnd: HWND) -> Option<*mut RefCell<DialogBox>> {
    let user = unsafe {
        windows_sys::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW(
            hwnd,
            windows_sys::Win32::UI::WindowsAndMessaging::GWLP_USERDATA,
        )
    } as usize;
    if user == 0 {
        return None;
    }
    let registered = slots()
        .lock()
        .ok()
        .and_then(|g| g.get(&hwnd_key(hwnd)).copied())?;
    if registered != user {
        return None;
    }
    Some(registered as *mut RefCell<DialogBox>)
}

unsafe extern "system" fn dialog_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: windows_sys::Win32::Foundation::WPARAM,
    lparam: windows_sys::Win32::Foundation::LPARAM,
) -> windows_sys::Win32::Foundation::LRESULT {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CREATESTRUCTW, DefWindowProcW, DestroyWindow, GWLP_USERDATA, SetWindowLongPtrW, WM_CLOSE,
        WM_COMMAND, WM_CREATE, WM_DESTROY,
    };
    match msg {
        WM_CREATE => {
            let create = unsafe { &*(lparam as *const CREATESTRUCTW) };
            unsafe {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
            }
            0
        }
        WM_COMMAND => {
            let id = (wparam & 0xffff) as u32;
            let notify = ((wparam >> 16) & 0xffff) as u32;
            if notify == 0 && (id == DLG_OK || id == DLG_CANCEL) {
                if let Some(ptr) = validated(hwnd) {
                    let channel = unsafe { &*ptr }
                        .try_borrow()
                        .ok()
                        .map(|boxed| Arc::clone(&boxed.outcome));
                    if let Some(channel) = channel {
                        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            *channel.lock().unwrap_or_else(|poison| poison.into_inner()) = Some(id);
                        }));
                    }
                }
                unsafe {
                    DestroyWindow(hwnd);
                }
            }
            0
        }
        WM_CLOSE => {
            unsafe {
                DestroyWindow(hwnd);
            }
            0
        }
        WM_DESTROY => 0,
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn register_class(hinst: windows_sys::Win32::Foundation::HINSTANCE) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{IDC_ARROW, LoadCursorW};
    use windows_sys::Win32::UI::WindowsAndMessaging::{RegisterClassW, WNDCLASSW};
    let class_w = wide("OmniTilerSettingsDialog");
    let mut cls: WNDCLASSW = unsafe { std::mem::zeroed() };
    cls.lpfnWndProc = Some(dialog_wnd_proc);
    cls.hInstance = hinst;
    cls.hCursor = unsafe { LoadCursorW(std::ptr::null_mut(), IDC_ARROW) };
    cls.hbrBackground = unsafe {
        windows_sys::Win32::Graphics::Gdi::GetSysColorBrush(
            windows_sys::Win32::Graphics::Gdi::COLOR_BTNFACE,
        )
    };
    cls.lpszClassName = class_w.as_ptr();
    let atom = unsafe { RegisterClassW(&cls) };
    atom != 0 || unsafe { windows_sys::Win32::Foundation::GetLastError() } == 1410
}

struct OwnedDialog {
    hwnd: HWND,
    state: Box<RefCell<DialogBox>>,
    _confined: std::marker::PhantomData<Rc<()>>,
}

impl Drop for OwnedDialog {
    fn drop(&mut self) {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            DestroyWindow, GWLP_USERDATA, IsWindow, SetWindowLongPtrW,
        };
        if let Ok(mut guard) = slots().lock() {
            guard.remove(&hwnd_key(self.hwnd));
        }
        let font = self
            .state
            .try_borrow()
            .map(|boxed| boxed.font)
            .unwrap_or(std::ptr::null_mut());
        unsafe {
            SetWindowLongPtrW(self.hwnd, GWLP_USERDATA, 0);
            if IsWindow(self.hwnd) != 0 {
                DestroyWindow(self.hwnd);
            }
            if !font.is_null() {
                windows_sys::Win32::Graphics::Gdi::DeleteObject(font);
            }
        }
    }
}

fn owned_modal(
    owner: HWND,
    title: &str,
    body: &str,
    buttons: &[(&str, u32, bool)],
    client_w: i32,
    client_h: i32,
) -> ModalExit {
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::HiDpi::GetDpiForSystem;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::EnableWindow;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        AdjustWindowRect, BS_DEFPUSHBUTTON, BS_PUSHBUTTON, CreateWindowExW, DestroyWindow,
        DispatchMessageW, ES_AUTOVSCROLL, ES_MULTILINE, ES_READONLY, GetSystemMetrics,
        GetWindowRect, IsChild, IsDialogMessageW, IsWindow, MsgWaitForMultipleObjectsEx, PM_REMOVE,
        PeekMessageW, PostQuitMessage, QS_ALLINPUT, SM_CXSCREEN, SM_CYSCREEN, SW_SHOWNORMAL,
        SetForegroundWindow, ShowWindow, TranslateMessage, WM_KEYDOWN, WS_BORDER, WS_CAPTION,
        WS_CHILD, WS_CLIPCHILDREN, WS_EX_DLGMODALFRAME, WS_SYSMENU, WS_TABSTOP, WS_VISIBLE,
        WS_VSCROLL,
    };
    if owner.is_null() || unsafe { IsWindow(owner) } == 0 {
        return ModalExit::OwnerGone;
    }
    let hinst = unsafe { GetModuleHandleW(std::ptr::null()) };
    if !register_class(hinst) {
        return ModalExit::Dismissed;
    }
    let dpi = unsafe { GetDpiForSystem() };
    let dpi = if dpi == 0 { 96 } else { dpi };
    let px = |value: i32| ((i64::from(value) * i64::from(dpi) + 48) / 96) as i32;
    let (client_w, client_h) = (px(client_w), px(client_h));
    let style = WS_CAPTION | WS_SYSMENU | WS_CLIPCHILDREN;
    let mut rect = windows_sys::Win32::Foundation::RECT {
        left: 0,
        top: 0,
        right: client_w,
        bottom: client_h,
    };
    unsafe {
        AdjustWindowRect(&mut rect, style, 0);
    }
    let win_w = rect.right - rect.left;
    let win_h = rect.bottom - rect.top;
    let (mut pos_x, mut pos_y) = unsafe {
        let (sw, sh) = (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN));
        (((sw - win_w) / 2).max(0), ((sh - win_h) / 2).max(0))
    };
    unsafe {
        let mut outer: windows_sys::Win32::Foundation::RECT = std::mem::zeroed();
        if GetWindowRect(owner, &mut outer) != 0 {
            let ow = outer.right - outer.left;
            let oh = outer.bottom - outer.top;
            pos_x = (outer.left + (ow - win_w) / 2).max(0);
            pos_y = (outer.top + (oh - win_h) / 2).max(0);
        }
    }
    let font = dialog_font(dpi);
    let outcome = Arc::new(Mutex::new(None));
    let mut boxed = Box::new(RefCell::new(DialogBox {
        font,
        outcome: Arc::clone(&outcome),
    }));
    let raw: *mut RefCell<DialogBox> = &mut *boxed;
    let title_w = wide(title);
    let dlg = unsafe {
        CreateWindowExW(
            WS_EX_DLGMODALFRAME,
            wide("OmniTilerSettingsDialog").as_ptr(),
            title_w.as_ptr(),
            style,
            pos_x,
            pos_y,
            win_w,
            win_h,
            owner,
            std::ptr::null_mut(),
            hinst,
            raw.cast(),
        )
    };
    if dlg.is_null() {
        if let Ok(boxed) = boxed.try_borrow() {
            unsafe {
                windows_sys::Win32::Graphics::Gdi::DeleteObject(boxed.font);
            }
        }
        return ModalExit::Dismissed;
    }
    if let Ok(mut guard) = slots().lock() {
        guard.insert(hwnd_key(dlg), raw as usize);
    }
    let dialog = OwnedDialog {
        hwnd: dlg,
        state: boxed,
        _confined: std::marker::PhantomData,
    };
    let margin = px(10);
    let button_w = px(110);
    let button_h = px(28);
    let gap = px(8);
    let button_y = client_h - margin - button_h;
    let text_h = button_y - margin - gap;
    let make_child = |class: &str,
                      text: &str,
                      id: u32,
                      def: bool,
                      x: i32,
                      y: i32,
                      w: i32,
                      h: i32,
                      extra: u32| unsafe {
        CreateWindowExW(
            0,
            wide(class).as_ptr(),
            wide(text).as_ptr(),
            WS_CHILD
                | WS_VISIBLE
                | WS_TABSTOP
                | if def {
                    BS_DEFPUSHBUTTON as u32
                } else {
                    BS_PUSHBUTTON as u32
                }
                | extra,
            x,
            y,
            w,
            h,
            dlg,
            id as isize as windows_sys::Win32::UI::WindowsAndMessaging::HMENU,
            hinst,
            std::ptr::null(),
        )
    };
    let text_style =
        ES_MULTILINE as u32 | ES_READONLY as u32 | ES_AUTOVSCROLL as u32 | WS_VSCROLL | WS_BORDER;
    let text_hwnd = unsafe {
        CreateWindowExW(
            0,
            wide("EDIT").as_ptr(),
            wide(body).as_ptr(),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | text_style,
            margin,
            margin,
            client_w - 2 * margin,
            text_h.max(px(40)),
            dlg,
            std::ptr::null_mut(),
            hinst,
            std::ptr::null(),
        )
    };
    let total = buttons.len() as i32;
    let mut button_hwnds = Vec::new();
    let mut children_ok = !text_hwnd.is_null();
    for (index, (label, id, is_default)) in buttons.iter().enumerate() {
        let x = client_w - margin - total * button_w - (total - 1) * gap
            + index as i32 * (button_w + gap);
        let hwnd = make_child(
            "BUTTON",
            label,
            *id,
            *is_default,
            x,
            button_y,
            button_w,
            button_h,
            0,
        );
        if hwnd.is_null() {
            children_ok = false;
        } else {
            button_hwnds.push(hwnd);
        }
    }
    if !children_ok {
        unsafe {
            DestroyWindow(dlg);
        }
        unsafe {
            let mut msg = std::mem::zeroed();
            while PeekMessageW(&mut msg, dlg, 0, 0, PM_REMOVE) != 0 {
                DispatchMessageW(&msg);
                if IsWindow(dlg) == 0 {
                    break;
                }
            }
        }
        drop(dialog);
        return ModalExit::Dismissed;
    }
    let set_font = |child: HWND| unsafe {
        windows_sys::Win32::UI::WindowsAndMessaging::SendMessageW(
            child,
            windows_sys::Win32::UI::WindowsAndMessaging::WM_SETFONT,
            font as usize,
            1,
        );
    };
    set_font(text_hwnd);
    for child in button_hwnds {
        set_font(child);
    }
    unsafe {
        EnableWindow(owner, 0);
        ShowWindow(dlg, SW_SHOWNORMAL);
        SetForegroundWindow(dlg);
    }
    DIALOG_OPEN.store(true, Ordering::SeqCst);
    loop {
        let mut msg: windows_sys::Win32::UI::WindowsAndMessaging::MSG =
            unsafe { std::mem::zeroed() };
        let mut progressed = false;
        while unsafe { PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) } != 0 {
            progressed = true;
            if msg.message == windows_sys::Win32::UI::WindowsAndMessaging::WM_QUIT {
                unsafe {
                    if IsWindow(dlg) != 0 {
                        DestroyWindow(dlg);
                    }
                    PostQuitMessage(msg.wParam as i32);
                }
                break;
            }
            if unsafe { IsWindow(dlg) } == 0 {
                break;
            }
            let to_dialog =
                msg.hwnd.is_null() || msg.hwnd == dlg || unsafe { IsChild(dlg, msg.hwnd) } != 0;
            if !to_dialog {
                unsafe {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
                continue;
            }
            if msg.message == WM_KEYDOWN && msg.wParam == ESCAPE_VK {
                unsafe {
                    DestroyWindow(dlg);
                }
                continue;
            }
            let eaten = unsafe { IsDialogMessageW(dlg, std::ptr::addr_of!(msg)) };
            if eaten == 0 {
                unsafe {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
        }
        if unsafe { IsWindow(dlg) } == 0 {
            break;
        }
        if unsafe { IsWindow(owner) } == 0 {
            unsafe {
                DestroyWindow(dlg);
            }
            DIALOG_OPEN.store(false, Ordering::SeqCst);
            drop(dialog);
            return ModalExit::OwnerGone;
        }
        if !progressed {
            unsafe {
                MsgWaitForMultipleObjectsEx(0, std::ptr::null(), STOP_POLL_MS, QS_ALLINPUT, 0);
            }
        }
    }
    DIALOG_OPEN.store(false, Ordering::SeqCst);
    let hwnd_alive = unsafe { IsWindow(owner) } != 0;
    if hwnd_alive {
        unsafe {
            EnableWindow(owner, 1);
            SetForegroundWindow(owner);
        }
    }
    let choice = *outcome.lock().unwrap_or_else(|poison| poison.into_inner());
    drop(dialog);
    if !hwnd_alive {
        return ModalExit::OwnerGone;
    }
    match choice {
        Some(id) => ModalExit::Choice(id),
        None => ModalExit::Dismissed,
    }
}

pub fn ask_preview(owner: HWND, body: &str, title: &str) -> PreviewEnd {
    match owned_modal(
        owner,
        title,
        body,
        &[("Apply", DLG_APPLY, false), ("Cancel", DLG_CANCEL, true)],
        520,
        340,
    ) {
        ModalExit::Choice(id) if id == DLG_APPLY => PreviewEnd::Apply,
        ModalExit::OwnerGone => PreviewEnd::OwnerGone,
        ModalExit::Choice(_) | ModalExit::Dismissed => PreviewEnd::Cancel,
    }
}

pub fn ask_discard(owner: HWND, body: &str, title: &str) -> DiscardEnd {
    match owned_modal(
        owner,
        title,
        body,
        &[
            ("Discard", DLG_DISCARD, false),
            ("Cancel", DLG_CANCEL, true),
        ],
        440,
        180,
    ) {
        ModalExit::Choice(id) if id == DLG_DISCARD => DiscardEnd::Discard,
        ModalExit::OwnerGone => DiscardEnd::OwnerGone,
        ModalExit::Choice(_) | ModalExit::Dismissed => DiscardEnd::Cancel,
    }
}

pub fn show_info(owner: HWND, title: &str, body: &str) -> bool {
    !matches!(
        owned_modal(owner, title, body, &[("OK", DLG_OK, true)], 440, 180),
        ModalExit::OwnerGone
    )
}

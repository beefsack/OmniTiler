//! Settings window host: class, procedure, creation, and message loop.
//!
//! The `extern` procedure, class registration, owned `Box<RefCell<App>>`
//! lifetime, singleton mutex, tab-strip items, tooltip attach, z-order, and
//! the modal message loop live here. Policy (draft, validation, file IO,
//! menu model, badge text) stays in `settings_ui`.
//!
//! Borrow discipline: the registry proves provenance only. Unique borrowing
//! is enforced at runtime by `RefCell`: every procedure/loop entry resolves
//! the validated cell and uses `try_borrow`/`try_borrow_mut`. A failed
//! borrow means a reentrant callback arrived while a guard is live, and the
//! nested message fails closed (returns 0, no state touched) instead of
//! aliasing. Guards are held across the safe policy callouts they serve so
//! nesting fails closed; Win32 pumps conditional on no live guard use
//! snapshots cloned before the pump. The registry lock guards bookkeeping
//! only and is never held across callbacks or pumps. Arbitrary classes never
//! invoke this procedure with foreign state.

use std::cell::{RefCell, RefMut};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Mutex, OnceLock};

use windows_sys::Win32::Foundation::{HINSTANCE, HWND};

use crate::settings_ui::App;

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain([0]).collect()
}

fn slots() -> &'static Mutex<HashMap<u64, usize>> {
    static SLOTS: OnceLock<Mutex<HashMap<u64, usize>>> = OnceLock::new();
    SLOTS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn hwnd_key(hwnd: HWND) -> u64 {
    hwnd as usize as u64
}

fn validated_cell(hwnd: HWND) -> Option<*mut RefCell<App>> {
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
    Some(registered as *mut RefCell<App>)
}

/// Run `f` under a checked exclusive borrow. `None` on unregistered state
/// or a live guard (reentrant nesting fails closed, never aliases).
fn with_app_mut<R>(hwnd: HWND, f: impl FnOnce(&mut App) -> R) -> Option<R> {
    let ptr = validated_cell(hwnd)?;
    let cell = unsafe { &*ptr };
    let mut guard: RefMut<'_, App> = cell.try_borrow_mut().ok()?;
    Some(f(&mut guard))
}

/// One held per-user settings mutex for the life of the open window.
pub(crate) struct SingletonGuard {
    handle: windows_sys::Win32::Foundation::HANDLE,
    _confined: std::marker::PhantomData<Rc<()>>,
}

impl Drop for SingletonGuard {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            unsafe {
                windows_sys::Win32::Foundation::CloseHandle(self.handle);
            }
        }
    }
}

fn focus_existing_settings(class: &str) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        FindWindowW, SW_RESTORE, SetForegroundWindow, ShowWindow,
    };
    let class_w = wide(class);
    let hwnd = unsafe { FindWindowW(class_w.as_ptr(), std::ptr::null()) };
    if hwnd.is_null() {
        return;
    }
    unsafe {
        ShowWindow(hwnd, SW_RESTORE);
        SetForegroundWindow(hwnd);
    }
}

/// Acquire the per-user settings singleton. `Ok(None)` means another window
/// holds it (existing window focused, caller returns without writing).
pub(crate) fn acquire_singleton(
    class: &str,
    mutex_name: &str,
) -> Result<Option<SingletonGuard>, String> {
    use windows_sys::Win32::Foundation::GetLastError;
    use windows_sys::Win32::System::Threading::CreateMutexW;
    let name_w = wide(mutex_name);
    let handle = unsafe { CreateMutexW(std::ptr::null(), 0, name_w.as_ptr()) };
    if handle.is_null() {
        return Err("error: settings single-instance lock failed".to_owned());
    }
    if unsafe { GetLastError() } == 183 {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(handle);
        }
        focus_existing_settings(class);
        return Ok(None);
    }
    Ok(Some(SingletonGuard {
        handle,
        _confined: std::marker::PhantomData,
    }))
}

pub(crate) fn ensure_tab_classes() -> Result<(), String> {
    use windows_sys::Win32::UI::Controls::{
        ICC_TAB_CLASSES, INITCOMMONCONTROLSEX, InitCommonControlsEx,
    };
    let icc = INITCOMMONCONTROLSEX {
        dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
        dwICC: ICC_TAB_CLASSES,
    };
    if unsafe { InitCommonControlsEx(&icc) } == 0 {
        return Err("error: settings tab classes failed".to_owned());
    }
    Ok(())
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: windows_sys::Win32::Foundation::WPARAM,
    lparam: windows_sys::Win32::Foundation::LPARAM,
) -> windows_sys::Win32::Foundation::LRESULT {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CREATESTRUCTW, DefWindowProcW, DestroyWindow, GWLP_USERDATA, SetWindowLongPtrW, WM_CLOSE,
        WM_COMMAND, WM_CREATE, WM_DESTROY, WM_GETMINMAXINFO, WM_NOTIFY, WM_TIMER,
    };
    if msg == WM_CREATE {
        let create = unsafe { &*(lparam as *const CREATESTRUCTW) };
        unsafe {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
        }
        return 0;
    }
    if (msg == WM_COMMAND || msg == WM_NOTIFY || msg == WM_TIMER)
        && crate::ffi::settings_dialog::dialog_open()
    {
        return 0;
    }
    if msg == WM_GETMINMAXINFO {
        let user = unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW(hwnd, GWLP_USERDATA)
        };
        let ready = crate::settings_ui::fixed_tracks_ready(user, lparam);
        if !ready {
            return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
        }
        let outer = (|| {
            use windows_sys::Win32::UI::WindowsAndMessaging::GetWindowRect;
            let mut outer: windows_sys::Win32::Foundation::RECT = unsafe { std::mem::zeroed() };
            if unsafe { GetWindowRect(hwnd, &mut outer) } == 0 {
                return None;
            }
            let w: Option<i32> = (i64::from(outer.right) - i64::from(outer.left))
                .try_into()
                .ok();
            let h: Option<i32> = (i64::from(outer.bottom) - i64::from(outer.top))
                .try_into()
                .ok();
            w.and_then(|w| h.and_then(|h| crate::settings_ui::fixed_minmax_tracks(w, h)))
        })();
        match outer {
            Some((w, h)) => {
                let info = unsafe {
                    &mut *(lparam as *mut windows_sys::Win32::UI::WindowsAndMessaging::MINMAXINFO)
                };
                info.ptMinTrackSize.x = w;
                info.ptMinTrackSize.y = h;
                info.ptMaxTrackSize.x = w;
                info.ptMaxTrackSize.y = h;
                0
            }
            None => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
        }
    } else if msg == WM_COMMAND {
        if crate::ffi::settings_controls::notifications_suppressed() {
            return 0;
        }
        let id = (wparam & 0xffff) as u32;
        if id == crate::settings_ui::ID_CLOSE {
            unsafe {
                DestroyWindow(hwnd);
            }
            return 0;
        }
        let notify = ((wparam >> 16) & 0xffff) as u32;
        let alive = with_app_mut(hwnd, |app| {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                crate::settings_ui::handle_command(app, id, notify)
            }))
            .unwrap_or(false)
        });
        if alive == Some(true) {
            let _ = with_app_mut(hwnd, |app| {
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    crate::settings_ui::refresh_badge_for(app);
                }));
            });
            if crate::ffi::settings_dialog::take_close_deferred() {
                use windows_sys::Win32::UI::WindowsAndMessaging::IsWindow;
                if unsafe { IsWindow(hwnd) } != 0 {
                    unsafe {
                        DestroyWindow(hwnd);
                    }
                }
            }
        }
        0
    } else if msg == WM_CLOSE {
        if crate::ffi::settings_dialog::dialog_open() {
            crate::ffi::settings_dialog::defer_close();
            return 0;
        }
        unsafe {
            DestroyWindow(hwnd);
        }
        0
    } else if msg == WM_NOTIFY {
        use windows_sys::Win32::UI::Controls::{NMHDR, TCM_GETCURSEL, TCN_SELCHANGE};
        use windows_sys::Win32::UI::WindowsAndMessaging::SendMessageW;
        // Snapshot the notification header before borrowing state: the
        // `SendMessageW` query below must not run under a live guard.
        let pick = if lparam == 0 {
            None
        } else {
            let (id_from, code, from) = unsafe {
                let hdr = &*(lparam as *const NMHDR);
                (hdr.idFrom, hdr.code, hdr.hwndFrom)
            };
            if id_from as u32 == crate::settings_ui::ID_TABS && code == TCN_SELCHANGE {
                let picked = unsafe { SendMessageW(from, TCM_GETCURSEL, 0, 0) };
                if picked >= 0 {
                    Some(picked as usize)
                } else {
                    None
                }
            } else {
                None
            }
        };
        if let Some(picked) = pick {
            let _ = with_app_mut(hwnd, |app| {
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    crate::settings_ui::show_tab_for(app, picked);
                }));
            });
        }
        0
    } else if msg == WM_TIMER {
        let _ = with_app_mut(hwnd, |app| {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                crate::settings_ui::handle_timer(app);
            }));
        });
        0
    } else if msg == WM_DESTROY {
        use windows_sys::Win32::UI::WindowsAndMessaging::{KillTimer, PostQuitMessage};
        unsafe {
            KillTimer(hwnd, crate::settings_ui::GAMING_TIMER_ID);
            PostQuitMessage(0);
        }
        0
    } else {
        unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
    }
}

pub(crate) struct SettingsWindow {
    hwnd: HWND,
    state: Box<RefCell<App>>,
    _singleton: Option<SingletonGuard>,
    _confined: std::marker::PhantomData<Rc<()>>,
}

impl Drop for SettingsWindow {
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
            .map(|app| app.font)
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

impl SettingsWindow {
    pub(crate) fn hwnd(&self) -> HWND {
        self.hwnd
    }

    /// Checked shared access. `None` when a guard is live (reentrant
    /// nesting fails closed, never aliases).
    pub(crate) fn with_app<R>(&self, f: impl FnOnce(&App) -> R) -> Option<R> {
        let guard = self.state.try_borrow().ok()?;
        Some(f(&guard))
    }

    /// Checked exclusive access scoped to `f`. `None` when a guard is live.
    /// Callers must not pump or call back into window functions inside `f`;
    /// Win32 and safe policy callouts run outside the guard.
    pub(crate) fn with_app_mut<R>(&self, f: impl FnOnce(&mut App) -> R) -> Option<R> {
        let mut guard = self.state.try_borrow_mut().ok()?;
        Some(f(&mut guard))
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn create(
        class: &str,
        title: &str,
        style: u32,
        ex_style: u32,
        origin_x: i32,
        origin_y: i32,
        client_w: i32,
        client_h: i32,
        mut app: Box<RefCell<App>>,
        singleton: Option<SingletonGuard>,
    ) -> Result<Self, (Box<RefCell<App>>, String)> {
        use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            AdjustWindowRect, CreateWindowExW, IDC_ARROW, LoadCursorW, RegisterClassW, WNDCLASSW,
        };
        let hinst: HINSTANCE = unsafe { GetModuleHandleW(std::ptr::null()) };
        let class_w = wide(class);
        let mut cls: WNDCLASSW = unsafe { std::mem::zeroed() };
        cls.lpfnWndProc = Some(wnd_proc);
        cls.hInstance = hinst;
        cls.hCursor = unsafe { LoadCursorW(std::ptr::null_mut(), IDC_ARROW) };
        cls.hbrBackground = unsafe {
            windows_sys::Win32::Graphics::Gdi::GetSysColorBrush(
                windows_sys::Win32::Graphics::Gdi::COLOR_BTNFACE,
            )
        };
        cls.lpszClassName = class_w.as_ptr();
        let atom = unsafe { RegisterClassW(&cls) };
        if atom == 0 {
            let code = unsafe { windows_sys::Win32::Foundation::GetLastError() };
            if code != 1410 {
                return Err((app, "error: settings RegisterClass failed".to_owned()));
            }
        }
        let raw: *mut RefCell<App> = &mut *app;
        let title_w = wide(title);
        let mut rect = windows_sys::Win32::Foundation::RECT {
            left: 0,
            top: 0,
            right: client_w,
            bottom: client_h,
        };
        unsafe {
            AdjustWindowRect(&mut rect, style, 0);
        }
        let hwnd = unsafe {
            CreateWindowExW(
                ex_style,
                class_w.as_ptr(),
                title_w.as_ptr(),
                style,
                origin_x,
                origin_y,
                rect.right - rect.left,
                rect.bottom - rect.top,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                hinst,
                raw.cast(),
            )
        };
        if hwnd.is_null() {
            return Err((app, "error: settings CreateWindow failed".to_owned()));
        }
        {
            let mut guard = app.borrow_mut();
            guard.window = hwnd;
        }
        if let Ok(mut guard) = slots().lock() {
            guard.insert(hwnd_key(hwnd), raw as usize);
        }
        Ok(Self {
            hwnd,
            state: app,
            _singleton: singleton,
            _confined: std::marker::PhantomData,
        })
    }

    pub(crate) fn hinstance() -> HINSTANCE {
        unsafe { windows_sys::Win32::System::LibraryLoader::GetModuleHandleW(std::ptr::null()) }
    }

    pub(crate) fn insert_tab_items(&self, titles: &[&str]) -> Result<(), String> {
        use windows_sys::Win32::UI::Controls::{TCIF_TEXT, TCITEMW, TCM_INSERTITEMW};
        use windows_sys::Win32::UI::WindowsAndMessaging::SendMessageW;
        let strip = self
            .state
            .try_borrow()
            .map(|app| app.ctl(crate::settings_ui::ID_TABS))
            .unwrap_or(std::ptr::null_mut());
        if strip.is_null() {
            return Err("error: settings tab strip missing".to_owned());
        }
        ensure_tab_classes()?;
        for (index, title) in titles.iter().enumerate() {
            let text_w = wide(title);
            let item = TCITEMW {
                mask: TCIF_TEXT,
                dwState: 0,
                dwStateMask: 0,
                pszText: text_w.as_ptr() as *mut u16,
                cchTextMax: 0,
                iImage: 0,
                lParam: 0,
            };
            let placed = unsafe {
                SendMessageW(
                    strip,
                    TCM_INSERTITEMW,
                    index,
                    &item as *const TCITEMW as isize,
                )
            };
            if placed < 0 {
                return Err("error: settings tab item failed".to_owned());
            }
        }
        Ok(())
    }

    pub(crate) fn attach_tips(&self) {
        use windows_sys::Win32::UI::Controls::{
            ICC_BAR_CLASSES, INITCOMMONCONTROLSEX, InitCommonControlsEx, TTF_IDISHWND,
            TTF_SUBCLASS, TTM_ADDTOOLW, TTS_ALWAYSTIP, TTTOOLINFOW,
        };
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            CreateWindowExW, SendMessageW, WS_POPUP,
        };
        // Snapshot handles and tooltip text addresses under one shared guard,
        // then release before any Win32 pump. Buffers are fixed at App
        // construction and never mutated, so addresses stay valid for the
        // window lifetime owned here.
        struct TipSnap {
            control: HWND,
            text: *const u16,
        }
        let snaps: Option<Vec<TipSnap>> = self.state.try_borrow().ok().map(|app| {
            [
                crate::settings_ui::ID_SAME_AXIS_GROUP,
                crate::settings_ui::ID_SAME_AXIS_SWAP,
                crate::settings_ui::ID_BORN_MAX_TILE,
                crate::settings_ui::ID_BORN_MAX_OVERLAY,
                crate::settings_ui::ID_FIXED_SIZE_BOTH,
                crate::settings_ui::ID_FIXED_SIZE_EITHER,
                crate::settings_ui::ID_MIGRATION_REFILL_LAST,
                crate::settings_ui::ID_MIGRATION_REFILL_MRU,
                crate::settings_ui::ID_ANCHOR_LARGEST,
                crate::settings_ui::ID_ANCHOR_CLOSEST,
                crate::settings_ui::ID_ANCHOR_NO_SEARCH,
                crate::settings_ui::ID_NO_SPACE_FLOAT,
                crate::settings_ui::ID_NO_SPACE_OVERLAP,
            ]
            .iter()
            .enumerate()
            .map(|(i, id)| TipSnap {
                control: app.ctl(*id),
                text: app.tip_text[i].as_ptr(),
            })
            .collect()
        });
        let Some(snaps) = snaps else {
            return;
        };
        let icc = INITCOMMONCONTROLSEX {
            dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_BAR_CLASSES,
        };
        unsafe {
            InitCommonControlsEx(&icc);
        }
        let hinst = Self::hinstance();
        let hwnd = self.hwnd;
        let class_w = wide("tooltips_class32");
        let tip = unsafe {
            CreateWindowExW(
                0,
                class_w.as_ptr(),
                std::ptr::null(),
                WS_POPUP | TTS_ALWAYSTIP,
                0,
                0,
                0,
                0,
                hwnd,
                std::ptr::null_mut(),
                hinst,
                std::ptr::null(),
            )
        };
        if tip.is_null() {
            return;
        }
        for snap in &snaps {
            if snap.control.is_null() {
                continue;
            }
            let info = TTTOOLINFOW {
                cbSize: std::mem::size_of::<TTTOOLINFOW>() as u32,
                uFlags: TTF_IDISHWND | TTF_SUBCLASS,
                hwnd,
                uId: snap.control as usize,
                rect: unsafe { std::mem::zeroed() },
                hinst: std::ptr::null_mut(),
                lpszText: snap.text as *mut u16,
                lParam: 0,
                lpReserved: std::ptr::null_mut(),
            };
            unsafe {
                SendMessageW(tip, TTM_ADDTOOLW, 0, &info as *const TTTOOLINFOW as isize);
            }
        }
    }

    pub(crate) fn apply_zorder(
        &self,
        order: &[crate::settings_ui::tabs::ZOrderOp],
    ) -> Result<(), String> {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            HWND_BOTTOM, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SetWindowPos,
        };
        let order_flags = SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE;
        let (strip, created) = self
            .state
            .try_borrow()
            .map(|app| (app.ctl(crate::settings_ui::ID_TABS), app.groups.clone()))
            .unwrap_or((std::ptr::null_mut(), Vec::new()));
        if strip.is_null() {
            return Err("error: settings tab strip missing".to_owned());
        }
        for op in order {
            let target = match op {
                crate::settings_ui::tabs::ZOrderOp::StripToBottom => strip,
                crate::settings_ui::tabs::ZOrderOp::GroupToBottom(index) => created
                    .get(*index)
                    .map(|(_, handle)| *handle)
                    .unwrap_or(std::ptr::null_mut()),
            };
            if target.is_null() {
                return Err("error: settings groupbox order failed".to_owned());
            }
            let placed = unsafe { SetWindowPos(target, HWND_BOTTOM, 0, 0, 0, 0, order_flags) };
            if placed == 0 {
                return Err("error: settings groupbox order failed".to_owned());
            }
        }
        Ok(())
    }

    pub(crate) fn show(&self) {
        use windows_sys::Win32::UI::WindowsAndMessaging::{SW_SHOWNORMAL, ShowWindow};
        unsafe {
            ShowWindow(self.hwnd, SW_SHOWNORMAL);
        }
    }

    pub(crate) fn start_gaming_timer(&self) {
        use windows_sys::Win32::UI::WindowsAndMessaging::SetTimer;
        unsafe {
            SetTimer(
                self.hwnd,
                crate::settings_ui::GAMING_TIMER_ID,
                crate::settings_ui::GAMING_TIMER_MS,
                None,
            );
        }
    }

    /// Owner-thread modal loop: pumps the queue until `WM_QUIT`. Ctrl+Tab
    /// cycles tabs under a checked exclusive guard (nested callbacks fail
    /// closed instead of aliasing); Escape destroys the owned window.
    pub(crate) fn run_loop(&self) -> Result<(), String> {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetKeyState;
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            DestroyWindow, DispatchMessageW, GetMessageW, IsDialogMessageW, TranslateMessage,
            WM_KEYDOWN,
        };
        let hwnd = self.hwnd;
        let mut msg: windows_sys::Win32::UI::WindowsAndMessaging::MSG =
            unsafe { std::mem::zeroed() };
        loop {
            let turn = unsafe { GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) };
            if turn == 0 {
                break;
            }
            if turn == -1 {
                return Err("error: settings message loop failed".to_owned());
            }
            if msg.message == WM_KEYDOWN && msg.wParam == crate::settings_ui::ESCAPE_VK {
                unsafe {
                    DestroyWindow(hwnd);
                }
                continue;
            }
            if msg.message == WM_KEYDOWN && msg.wParam == 0x09 {
                let ctrl = unsafe { GetKeyState(0x11) } < 0;
                if ctrl {
                    let shift = unsafe { GetKeyState(0x10) } < 0;
                    let _ = with_app_mut(hwnd, |app| {
                        let next = crate::settings_ui::tabs::next_tab(app.current_tab, !shift);
                        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            crate::settings_ui::show_tab_for(app, next);
                        }));
                    });
                    continue;
                }
            }
            let eaten = unsafe { IsDialogMessageW(hwnd, std::ptr::addr_of!(msg)) };
            if eaten == 0 {
                unsafe {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::wnd_proc;

    #[test]
    fn destroy_posts_quit_so_modal_loop_exits() {
        // Offline: dedicated thread owns its message queue, so the
        // `PostQuitMessage` from `WM_DESTROY` lands there. No window,
        // hook, or `GetMessage` pump is created; `KillTimer` on a null
        // handle with no thread timers is a no-op. Bounded `PeekMessageW`
        // only, never a blocking `GetMessageW`.
        let posted = std::thread::spawn(|| {
            use windows_sys::Win32::UI::WindowsAndMessaging::{
                MSG, PM_NOREMOVE, PM_REMOVE, PeekMessageW, WM_DESTROY, WM_QUIT,
            };
            unsafe {
                let mut warm: MSG = std::mem::zeroed();
                PeekMessageW(&mut warm, std::ptr::null_mut(), 0, 0, PM_NOREMOVE);
            }
            let turn = unsafe { wnd_proc(std::ptr::null_mut(), WM_DESTROY, 0, 0) };
            assert_eq!(turn, 0);
            for _ in 0..100 {
                let mut msg: MSG = unsafe { std::mem::zeroed() };
                let got = unsafe { PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) };
                if got != 0 && msg.message == WM_QUIT {
                    return true;
                }
            }
            false
        })
        .join()
        .expect("destroy probe thread");
        assert!(
            posted,
            "WM_DESTROY must PostQuitMessage so run_loop GetMessage returns 0"
        );
    }
}

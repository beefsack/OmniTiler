//! Owned modal dialogs for the Settings window (Apply/Revert).
//!
//! Pure policy (portable, tested) plus the Win32 owned-modal implementation
//! (`cfg(windows)` only). The native dialogs are exact owned popups of the
//! Settings window: no `MessageBoxW`, no title search. The owner is disabled
//! while a dialog runs and re-enabled on every exit; the modal loop pumps
//! the thread queue, answers `Escape`/close as cancel, preserves a pending
//! `WM_QUIT` for the outer Settings loop, and exits promptly when the owner
//! goes away. Keyboard traversal uses `IsDialogMessageW` on the dialog.
//!
//! Conflict semantics reuse the tray warnings exactly
//! (`tray_warning_conflicts` over `effective_bindings`): the preview lists
//! the draft's current warning set in catalog order, never a separate
//! documented-owner inventory.

use crate::gaming::GamingSnapshot;
use crate::settings::Settings;
use crate::tray::UnresolvedConflict;

/// Title for every Apply dialog (preview and result).
pub const APPLY_TITLE: &str = "Apply Settings";
/// Title for every Revert dialog (discard confirm and result).
pub const REVERT_TITLE: &str = "Revert Settings";
/// Short Apply success body shown in the result dialog.
pub const APPLY_SUCCESS: &str = "Settings saved.";
/// Short Revert success body shown in the result dialog.
pub const REVERT_SUCCESS: &str = "Reverted to saved settings.";
/// Discard-confirm body: shown only when unsaved state would be lost.
pub const DISCARD_PROMPT: &str = "Discard unsaved edits?";
/// Preview intro line above the readonly conflict list.
pub const PREVIEW_INTRO: &str =
    "These shortcuts conflict with Windows. Apply anyway? (Cancel keeps your draft.)";
/// Single-line cap for concise dialog error reasons.
pub const MAX_REASON_CHARS: usize = 240;

/// Collapse to one line and cap at `cap` characters (by char, not byte).
/// Newlines, carriage returns and tabs become single spaces; surrounding
/// whitespace is trimmed; longer input truncates on a char boundary.
#[must_use]
pub fn single_line_capped(text: &str, cap: usize) -> String {
    let flat = text.replace("\r\n", "\n").replace(['\r', '\n', '\t'], " ");
    let collapsed: String = flat.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() > cap {
        collapsed.chars().take(cap).collect()
    } else {
        collapsed
    }
}

/// Concise single-line dialog reason, capped at [`MAX_REASON_CHARS`].
#[must_use]
pub fn short_reason(reason: &str) -> String {
    single_line_capped(reason, MAX_REASON_CHARS)
}

/// Concise error dialog body; the full diagnostic stays on the status line.
#[must_use]
pub fn error_body(reason: &str) -> String {
    format!("Error: {}", short_reason(reason))
}

/// True when any unsaved channel holds edits: HWND-dirty flag, staged
/// `work != base` drift, or a still-typed pending chord.
#[must_use]
pub const fn has_unsaved_edits(dirty: bool, work_drift: bool, pending: bool) -> bool {
    dirty || work_drift || pending
}

/// Apply routing: a nonempty draft warning set gates on one preview.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyGate {
    Direct,
    Preview,
}

#[must_use]
pub const fn apply_gate(conflict_count: usize) -> ApplyGate {
    if conflict_count == 0 {
        ApplyGate::Direct
    } else {
        ApplyGate::Preview
    }
}

/// Revert routing: unsaved state needs one discard confirm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RevertGate {
    Proceed,
    ConfirmDiscard,
}

#[must_use]
pub const fn revert_gate(has_unsaved: bool) -> RevertGate {
    if has_unsaved {
        RevertGate::ConfirmDiscard
    } else {
        RevertGate::Proceed
    }
}

/// The draft's current tray-warning set: `effective_bindings` rows filtered
/// through the existing `tray_warning_conflicts` semantics (takeover-gated,
/// cleared-row aware, snapshot-aware). Catalog order is preserved.
#[must_use]
pub fn preview_conflicts(
    settings: &Settings,
    takeover: bool,
    allow_win_l: bool,
    snapshot: &GamingSnapshot,
) -> Vec<UnresolvedConflict> {
    crate::gaming::tray_warning_conflicts(settings, takeover, allow_win_l, snapshot)
}

/// Readonly preview body: intro plus one stable `id [chord] -- note` line
/// per conflict in the given (catalog) order. Empty input yields the intro
/// only; callers gate on [`apply_gate`] so the preview never opens empty.
#[must_use]
pub fn preview_body(conflicts: &[UnresolvedConflict]) -> String {
    let mut out = PREVIEW_INTRO.to_owned();
    for conflict in conflicts {
        out.push('\n');
        out.push_str(&format!(
            "{} [{}] -- {}",
            conflict.id, conflict.chord, conflict.note
        ));
    }
    out
}

#[cfg(windows)]
mod native {
    use super::{APPLY_TITLE, REVERT_TITLE};
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, Ordering};
    use windows_sys::Win32::Foundation::HWND;

    /// True while an owned Settings dialog runs its nested modal pump. The
    /// Settings `wnd_proc` uses this to ignore `WM_COMMAND`/`WM_NOTIFY`/
    /// `WM_TIMER` (which would re-borrow `App`) while the outer `&mut App`
    /// borrow is still live. `WM_CLOSE` is deferred separately (see
    /// `CLOSE_DEFERRED`): swallowing it would drop a user close, and
    /// destroying through it would free the borrowed `App` mid-modal.
    static DIALOG_OPEN: AtomicBool = AtomicBool::new(false);

    pub fn dialog_open() -> bool {
        DIALOG_OPEN.load(Ordering::SeqCst)
    }

    /// Owner `WM_CLOSE` received while `DIALOG_OPEN` holds. The owner is
    /// disabled precisely so it cannot be torn down mid-modal; the close is
    /// honored right after the outer `&mut App` borrow ends (drained once
    /// per `WM_COMMAND` in the owner `wnd_proc`), never dropped and never
    /// run while borrowed. No `App` state is touched to record it.
    static CLOSE_DEFERRED: AtomicBool = AtomicBool::new(false);

    pub fn defer_close() {
        CLOSE_DEFERRED.store(true, Ordering::SeqCst);
    }

    pub fn take_close_deferred() -> bool {
        CLOSE_DEFERRED.swap(false, Ordering::SeqCst)
    }

    /// How an owned modal ended. `Dismissed` covers every user/bounded
    /// dismissal with the owner still alive (button Cancel/OK, Escape,
    /// dialog close, child-creation failure, reposted outer quit): the
    /// caller may keep using its `App` borrow. `OwnerGone` means the owner
    /// window died mid-modal (backstop only; the `WM_CLOSE` deferral makes
    /// this unreachable through the close path): the `App` box was freed
    /// with it, so the caller must abandon its borrow immediately and
    /// touch nothing.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum ModalExit {
        Choice(u32),
        Dismissed,
        OwnerGone,
    }

    /// Preview verdict: Apply proceeds (with a post-preview disk recheck),
    /// Cancel preserves draft and file with no second dialog, OwnerGone
    /// abandons the `App` borrow.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum PreviewEnd {
        Apply,
        Cancel,
        OwnerGone,
    }

    /// Discard verdict: same contract as [`PreviewEnd`].
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum DiscardEnd {
        Discard,
        Cancel,
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
        outcome: std::sync::Arc<Mutex<Option<u32>>>,
    }

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain([0]).collect()
    }

    fn dialog_font(dpi: u32) -> windows_sys::Win32::Graphics::Gdi::HFONT {
        use windows_sys::Win32::Graphics::Gdi::{CreateFontW, DEFAULT_GUI_FONT, GetStockObject};
        let height = -((9 * dpi as i32 + 36) / 72);
        let face = wide("Segoe UI");
        let font =
            unsafe { CreateFontW(height, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, face.as_ptr()) };
        if font.is_null() {
            unsafe { GetStockObject(DEFAULT_GUI_FONT) }
        } else {
            font
        }
    }

    unsafe extern "system" fn dialog_wnd_proc(
        hwnd: HWND,
        msg: u32,
        wparam: windows_sys::Win32::Foundation::WPARAM,
        lparam: windows_sys::Win32::Foundation::LPARAM,
    ) -> windows_sys::Win32::Foundation::LRESULT {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            CREATESTRUCTW, DefWindowProcW, DestroyWindow, GWLP_USERDATA, GetWindowLongPtrW,
            SetWindowLongPtrW, WM_CLOSE, WM_COMMAND, WM_CREATE, WM_DESTROY,
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
                // Push-button clicks only (0 == BN_CLICKED).
                if notify == 0 && (id == DLG_OK || id == DLG_CANCEL) {
                    let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) };
                    if ptr != 0 {
                        let boxed = unsafe { &*(ptr as *mut DialogBox) };
                        *boxed
                            .outcome
                            .lock()
                            .unwrap_or_else(|poison| poison.into_inner()) = Some(id);
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
            WM_DESTROY => {
                let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) };
                if ptr != 0 {
                    unsafe {
                        SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                        let boxed = Box::from_raw(ptr as *mut DialogBox);
                        windows_sys::Win32::Graphics::Gdi::DeleteObject(boxed.font);
                        drop(boxed);
                    }
                }
                // Never PostQuit here: this is a nested modal inside the
                // Settings message loop. The modal loop exits on
                // `IsWindow == 0`; a quit would break the outer loop.
                0
            }
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

    /// Owned modal core: one owned popup with a readonly scrollable body and
    /// one or two buttons. `OwnerGone` reports owner death only (entry-dead
    /// or the mid-modal backstop); every other path keeps the owner alive.
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
            GetWindowRect, IsChild, IsDialogMessageW, IsWindow, MsgWaitForMultipleObjectsEx,
            PM_REMOVE, PeekMessageW, PostQuitMessage, QS_ALLINPUT, SM_CXSCREEN, SM_CYSCREEN,
            SW_SHOWNORMAL, SetForegroundWindow, ShowWindow, TranslateMessage, WM_KEYDOWN,
            WS_BORDER, WS_CAPTION, WS_CHILD, WS_CLIPCHILDREN, WS_EX_DLGMODALFRAME, WS_SYSMENU,
            WS_TABSTOP, WS_VISIBLE, WS_VSCROLL,
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
        // Center over the owner when its rect reads; else screen center.
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
        let outcome = std::sync::Arc::new(Mutex::new(None));
        let boxed = Box::new(DialogBox {
            font,
            outcome: std::sync::Arc::clone(&outcome),
        });
        let raw = Box::into_raw(boxed);
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
            unsafe {
                let boxed = Box::from_raw(raw);
                windows_sys::Win32::Graphics::Gdi::DeleteObject(boxed.font);
                drop(boxed);
            }
            return ModalExit::Dismissed;
        }
        // Layout (logical units, scaled above): body text on top, buttons in
        // the bottom-right. Body is a readonly multiline edit with its own
        // vertical scrollbar and tab stop so long conflict lists stay
        // readable and keyboard reachable.
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
        let text_style = ES_MULTILINE as u32
            | ES_READONLY as u32
            | ES_AUTOVSCROLL as u32
            | WS_VSCROLL
            | WS_BORDER;
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
        // Buttons right-aligned: single OK, or Apply/Cancel and
        // Discard/Cancel pairs with Cancel as the default (safe) button.
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
            // `WM_DESTROY` reclaimed the box; the dismissal stands.
            // Pump once so the destroy completes before returning.
            unsafe {
                let mut msg = std::mem::zeroed();
                while PeekMessageW(&mut msg, dlg, 0, 0, PM_REMOVE) != 0 {
                    DispatchMessageW(&msg);
                    if IsWindow(dlg) == 0 {
                        break;
                    }
                }
            }
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
        // Nested modal pump: every thread message is pumped so child
        // keyboard focus and dialog navigation keep working; the Settings
        // `wnd_proc` ignores App-touching messages while `DIALOG_OPEN`
        // holds (see `dialog_open`), so the outer `&mut App` borrow across
        // this call never aliases.
        loop {
            let mut msg: windows_sys::Win32::UI::WindowsAndMessaging::MSG =
                unsafe { std::mem::zeroed() };
            let mut progressed = false;
            while unsafe { PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) } != 0 {
                progressed = true;
                if msg.message == windows_sys::Win32::UI::WindowsAndMessaging::WM_QUIT {
                    // Preserve the quit for the outer Settings loop.
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
                    // Not ours: dispatch normally (owner paint/timer paths
                    // run through the guarded `wnd_proc`, which ignores
                    // App-touching messages while this flag holds).
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
                // Owner went away: owned windows die with it; exit without
                // touching the owner handle again. The caller must abandon
                // its `App` borrow: `WM_DESTROY` freed the box.
                unsafe {
                    DestroyWindow(dlg);
                }
                DIALOG_OPEN.store(false, Ordering::SeqCst);
                return ModalExit::OwnerGone;
            }
            if !progressed {
                unsafe {
                    MsgWaitForMultipleObjectsEx(0, std::ptr::null(), STOP_POLL_MS, QS_ALLINPUT, 0);
                }
            }
        }
        DIALOG_OPEN.store(false, Ordering::SeqCst);
        if unsafe { IsWindow(owner) } != 0 {
            unsafe {
                EnableWindow(owner, 1);
                SetForegroundWindow(owner);
            }
        } else {
            // Destroyed between the last poll and the dialog's own teardown:
            // same contract as the backstop above.
            return ModalExit::OwnerGone;
        }
        // Drain any stray quit the destroy may have left? No: the dialog
        // never posts quit, and a consumed outer quit was reposted above.
        // Remove only our own leftover messages is unnecessary: the loop
        // exited because the window is gone.
        let guard = outcome.lock().unwrap_or_else(|poison| poison.into_inner());
        match *guard {
            Some(id) => ModalExit::Choice(id),
            None => ModalExit::Dismissed,
        }
    }

    /// Preview gate: readonly scrollable list with Apply/Cancel. Cancel is
    /// the default (safe) button. Apply proceeds; Cancel preserves draft and
    /// file with no second dialog; OwnerGone abandons the `App` borrow.
    pub fn ask_preview(owner: HWND, body: &str) -> PreviewEnd {
        match owned_modal(
            owner,
            APPLY_TITLE,
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

    /// Discard confirm for Revert: Discard/Cancel, Cancel default.
    /// Same contract as [`ask_preview`].
    pub fn ask_discard(owner: HWND, body: &str) -> DiscardEnd {
        match owned_modal(
            owner,
            REVERT_TITLE,
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

    /// Short result/error notice with a single OK (default). Returns false
    /// only on OwnerGone (caller must abandon its `App` borrow).
    pub fn show_info(owner: HWND, title: &str, body: &str) -> bool {
        !matches!(
            owned_modal(owner, title, body, &[("OK", DLG_OK, true)], 440, 180),
            ModalExit::OwnerGone
        )
    }
}

#[cfg(windows)]
pub use native::{
    DiscardEnd, PreviewEnd, ask_discard, ask_preview, defer_close, dialog_open, show_info,
    take_close_deferred,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gaming::{GameBarOutcome, GameBarPackage, GamingSnapshot, XboxConfig, XboxOutcome};
    use crate::settings::{
        BindingSetting, BindingState, Settings, apply_preset, validate_settings,
    };

    fn unknown_snapshot() -> GamingSnapshot {
        GamingSnapshot {
            xbox: XboxConfig::Unknown,
            xbox_outcome: XboxOutcome::Absent,
            gamebar: GameBarPackage::Unknown,
            gamebar_outcome: GameBarOutcome::QueryFailed,
            generation: 0,
        }
    }

    fn known_snapshot() -> GamingSnapshot {
        GamingSnapshot {
            xbox: XboxConfig::Unknown,
            xbox_outcome: XboxOutcome::Absent,
            gamebar: GameBarPackage::Registered,
            gamebar_outcome: GameBarOutcome::Counted,
            generation: 1,
        }
    }

    fn proven_snapshot() -> GamingSnapshot {
        GamingSnapshot {
            xbox: XboxConfig::Enabled,
            xbox_outcome: XboxOutcome::Matched,
            gamebar: GameBarPackage::Registered,
            gamebar_outcome: GameBarOutcome::Counted,
            generation: 1,
        }
    }

    #[test]
    fn authentic_preview_lists_two_rows_in_catalog_order() {
        let settings = Settings::default();
        // Part B: only positive proof lists rows; unknown warns nowhere.
        assert!(preview_conflicts(&settings, true, false, &unknown_snapshot()).is_empty());
        let conflicts = preview_conflicts(&settings, true, false, &proven_snapshot());
        let ids: Vec<&str> = conflicts.iter().map(|c| c.id).collect();
        assert_eq!(ids, vec!["toggle-float", "toggle-fullscreen"]);
        assert_eq!(apply_gate(conflicts.len()), ApplyGate::Preview);
        let body = preview_body(&conflicts);
        assert!(body.starts_with(PREVIEW_INTRO));
        let float_at = body.find("toggle-float").expect("float row");
        let full_at = body.find("toggle-fullscreen").expect("fullscreen row");
        assert!(float_at < full_at);
        for id in &ids {
            assert_eq!(body.matches(id).count(), 1, "{id} exactly once");
        }
        assert!(body.contains("Win+G"));
        assert!(body.contains("Win+F11"));
    }

    #[test]
    fn compatible_needs_no_preview() {
        let mut settings = Settings::default();
        apply_preset(&mut settings, crate::settings::Preset::Compatible);
        assert!(validate_settings(&settings).is_ok());
        let conflicts = preview_conflicts(&settings, true, false, &known_snapshot());
        assert!(conflicts.is_empty());
        assert_eq!(apply_gate(conflicts.len()), ApplyGate::Direct);
        assert_eq!(preview_body(&conflicts), PREVIEW_INTRO);
    }

    #[test]
    fn rebound_and_disabled_rows_leave_the_set() {
        let mut rebound = Settings::default();
        rebound.bindings.insert(
            "toggle-float".to_owned(),
            BindingSetting {
                state: BindingState::Rebind,
                chord: Some("Win+Q".to_owned()),
            },
        );
        assert!(validate_settings(&rebound).is_ok());
        let ids: Vec<&str> = preview_conflicts(&rebound, true, false, &proven_snapshot())
            .iter()
            .map(|c| c.id)
            .collect();
        assert_eq!(ids, vec!["toggle-fullscreen"]);
        let mut disabled = Settings::default();
        disabled.bindings.insert(
            "toggle-float".to_owned(),
            BindingSetting {
                state: BindingState::Disabled,
                chord: None,
            },
        );
        assert!(validate_settings(&disabled).is_ok());
        let ids: Vec<&str> = preview_conflicts(&disabled, true, false, &proven_snapshot())
            .iter()
            .map(|c| c.id)
            .collect();
        assert_eq!(ids, vec!["toggle-fullscreen"]);
        // Unknown warns nowhere for either shape.
        assert!(preview_conflicts(&rebound, true, false, &unknown_snapshot()).is_empty());
        assert!(preview_conflicts(&disabled, true, false, &unknown_snapshot()).is_empty());
    }

    #[test]
    fn takeover_off_silences_preview_and_win_l_stays_a_warning() {
        let settings = Settings::default();
        assert!(preview_conflicts(&settings, false, false, &unknown_snapshot()).is_empty());
        assert!(preview_conflicts(&settings, false, true, &unknown_snapshot()).is_empty());
        assert!(preview_conflicts(&settings, false, false, &proven_snapshot()).is_empty());
        assert!(preview_conflicts(&settings, false, true, &proven_snapshot()).is_empty());
        // Unknown warns nowhere: Win+L opt-in alone lists exactly one row.
        let unknown_lock = preview_conflicts(&settings, true, true, &unknown_snapshot());
        let ids: Vec<&str> = unknown_lock.iter().map(|c| c.id).collect();
        assert_eq!(ids, vec!["focus-right"]);
        // Win+L opt-in adds exactly one warning row after the two proven
        // conflicts.
        let with_lock = preview_conflicts(&settings, true, true, &proven_snapshot());
        let ids: Vec<&str> = with_lock.iter().map(|c| c.id).collect();
        assert_eq!(
            ids,
            vec!["focus-right", "toggle-float", "toggle-fullscreen"]
        );
        let body = preview_body(&with_lock);
        assert!(body.contains("Win+L"));
        assert_eq!(revert_gate(true), RevertGate::ConfirmDiscard);
        assert_eq!(revert_gate(false), RevertGate::Proceed);
    }

    #[test]
    fn unsaved_detection_covers_full_draft_and_pending_edits() {
        assert!(!has_unsaved_edits(false, false, false));
        assert!(has_unsaved_edits(true, false, false));
        assert!(has_unsaved_edits(false, true, false));
        assert!(has_unsaved_edits(false, false, true));
        assert!(has_unsaved_edits(true, true, true));
        assert_eq!(
            revert_gate(has_unsaved_edits(false, true, false)),
            RevertGate::ConfirmDiscard
        );
        assert_eq!(
            revert_gate(has_unsaved_edits(false, false, false)),
            RevertGate::Proceed
        );
    }

    #[test]
    fn error_reason_is_single_line_capped() {
        let reason = "line one\nline two\r\nline three\twith tab plus trailing spaces   ";
        assert_eq!(
            short_reason(reason),
            "line one line two line three with tab plus trailing spaces"
        );
        assert!(!short_reason("a\nb").contains('\n'));
        let long = "x".repeat(400);
        assert_eq!(short_reason(&long).chars().count(), MAX_REASON_CHARS);
        assert_eq!(short_reason(&long), "x".repeat(MAX_REASON_CHARS));
        let body = error_body(&long);
        assert!(body.starts_with("Error: "));
        assert!(!body.contains('\n'));
        // Success strings carry no runtime-delivery claim.
        assert_eq!(APPLY_SUCCESS, "Settings saved.");
        assert_eq!(REVERT_SUCCESS, "Reverted to saved settings.");
        assert!(!APPLY_SUCCESS.to_lowercase().contains("applied"));
        assert!(!REVERT_SUCCESS.to_lowercase().contains("default"));
    }

    #[test]
    fn stale_and_save_reasons_sanitize_to_one_capped_line() {
        let stale = "Settings changed on disk (another window saved); reloaded. Re-apply your edits.\nSecond line.";
        let short = short_reason(stale);
        assert!(!short.contains('\n'));
        assert!(short.chars().count() <= MAX_REASON_CHARS);
        assert!(error_body("Settings file turned invalid (boom)").contains("boom"));
    }

    #[test]
    fn dead_owner_headless_reports_owner_gone_not_cancel() {
        // Null HWND never opens a window: the wrappers must report OwnerGone
        // (abandon the `App` borrow), never a Cancel that would keep using it.
        // This pins the HIGH disambiguation headless: no window is created.
        use std::ptr::null_mut;
        assert_eq!(ask_preview(null_mut(), "body"), PreviewEnd::OwnerGone);
        assert_eq!(ask_discard(null_mut(), "body"), DiscardEnd::OwnerGone);
        assert!(!show_info(null_mut(), "title", "body"));
        assert!(!dialog_open());
    }

    #[test]
    fn deferred_close_drains_exactly_once() {
        // Dispatch policy behind the HIGH fix: `WM_CLOSE` during a modal is
        // recorded without touching `App` and honored once after the outer
        // borrow ends. The flag starts clear, drains exactly once, and is
        // left clear so a later window never inherits a stale teardown.
        assert!(!take_close_deferred());
        defer_close();
        assert!(take_close_deferred());
        assert!(!take_close_deferred());
    }
}

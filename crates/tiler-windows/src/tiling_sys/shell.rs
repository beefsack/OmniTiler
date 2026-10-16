use super::*;

// Extended-frame-bounds and cloaked attribute ids (stable DWM ABI).
pub(super) const DWMWA_EXTENDED_FRAME_BOUNDS: u32 = 9;
pub(super) const DWMWA_CLOAKED: u32 = 14;

/// Shell classes that are never tile targets.
pub(super) fn is_shell_class(class: &str) -> bool {
    matches!(
        class,
        "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd" | "DV2ControlHost"
    )
}

/// Border-only shell-popup exclusion: the KDE `isAppletPopup` analogue.
/// Targeted, never blanket: ordinary dialogs (`#32770`), tool windows,
/// owned windows, and `explorer.exe` file browsers stay eligible. Only exact
/// shell classes plus XAML-island shell hosts gated on the shell process
/// suppress the ring. Task View suspension also hides the owned surface.
pub(super) fn is_border_shell_popup(class: &str, exe_file: &str) -> bool {
    if crate::product_hide::shell_class_excluded(class) {
        return true;
    }
    if matches!(
        class,
        "MultitaskingViewFrame" | "TaskSwitcherWnd" | "NotifyIconOverflowWindow" | "Shell_Flyout"
    ) {
        return true;
    }
    // XAML-island shell surfaces (Start, Search, Widgets): class alone is
    // generic, so require the shell experience process. Never matches
    // explorer.exe file browsers or ApplicationFrameHost store apps.
    if class == "Windows.UI.Core.CoreWindow" || class == "XamlExplorerHostIslandWindow" {
        return matches!(
            exe_file,
            "startmenuexperiencehost.exe"
                | "shellexperiencehost.exe"
                | "searchhost.exe"
                | "searchui.exe"
                | "widgets.exe"
                | "textinputhost.exe"
        );
    }
    false
}

pub(super) fn exe_file_name(exe_path: &str) -> &str {
    exe_path.rsplit(['/', '\\']).next().unwrap_or(exe_path)
}

pub(super) fn rect_from_win(rect: RECT) -> Option<Rect> {
    let w = (rect.right as i64 - rect.left as i64).try_into().ok()?;
    let h = (rect.bottom as i64 - rect.top as i64).try_into().ok()?;
    if w <= 0 || h <= 0 {
        return None;
    }
    Some(Rect {
        x: rect.left,
        y: rect.top,
        w,
        h,
    })
}

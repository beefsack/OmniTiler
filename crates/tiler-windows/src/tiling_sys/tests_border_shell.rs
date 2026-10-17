#![forbid(unsafe_code)]
use super::*;

#[test]
fn applet_analogues_excluded_without_blanket_dialog_tool_owned() {
    for class in [
        "MultitaskingViewFrame",
        "TaskSwitcherWnd",
        "NotifyIconOverflowWindow",
        "Shell_Flyout",
        "Progman",
        "MSCTFIME UI",
        "SystemTray_Main",
    ] {
        assert!(is_border_shell_popup(class, "explorer.exe"), "{class}");
    }
    assert!(is_border_shell_popup(
        "Windows.UI.Core.CoreWindow",
        "startmenuexperiencehost.exe"
    ));
    assert!(is_border_shell_popup(
        "XamlExplorerHostIslandWindow",
        "searchhost.exe"
    ));
    assert!(!is_border_shell_popup(
        "Windows.UI.Core.CoreWindow",
        "explorer.exe"
    ));
    assert!(!is_border_shell_popup(
        "Windows.UI.Core.CoreWindow",
        "applicationframehost.exe"
    ));
    assert!(!is_border_shell_popup("Notepad", "notepad.exe"));
    assert!(!is_border_shell_popup("#32770", "notepad.exe"));
    assert!(!is_border_shell_popup("MSPaintApp", "mspaint.exe"));
    assert!(!is_border_shell_popup("CabinetWClass", "explorer.exe"));
}

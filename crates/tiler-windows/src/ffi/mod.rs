#![allow(unsafe_code)]
//! Windows-only native boundary for `tiler-windows`.
//!
//! Sole `unsafe` owner in this crate: every Win32 call, raw-pointer use,
//! buffer lifetime, and `extern "system"` callback lives in this directory.
//! Safe callers elsewhere pass numeric window ids (`u64`, as the existing
//! model does), plain values, boring borrows, and shared safe data types
//! (never re-buffered across the boundary) - never arbitrary buffer/state
//! pointers, and never validity obligations. Buffers are allocated here,
//! callback contexts are owned here, and fallible reads fail closed
//! (`None`/`false`/zero) instead of guessing. Owned windows are opaque
//! thread-confined values (`!Send`/`!Sync` structurally, destroyed by
//! `Drop`); window-procedure state access is validated against live
//! registration, never trusted from caller input.
//!
//! Responsibility split (each file owns its protocol only, no policy):
//!
//! - [`window`]: window predicates, foreground, styles, geometry, props,
//!   class names, DWM reads, hit-testing, minmax queries.
//! - [`enumeration`]: top-level/child window and monitor enumeration with
//!   callback contexts owned here.
//! - [`hooks`]: WinEvent hook install/uninstall plus the single `extern`
//!   trampoline forwarding to a safe caller-supplied sink.
//! - [`llhook`]: low-level keyboard hook ownership: the two
//!   `extern` trampolines copy OS payloads into safe event structs, then
//!   invoke the installing thread's policy callback (consume or pass).
//! - [`sysparam`]: session-only system-parameter reads/writes.
//! - [`pump`]: thread message drain, the alertable outer wait, and the
//!   timer-driven message-loop step.
//! - [`input`]: cursor position, async key state, tick count, key-pair
//!   sends, foreground-rights prime.
//! - [`dpi`]: DPI awareness and system metrics.
//! - [`process`]: process/token/query primitives backing `native.rs`.
//! - [`shell`]: shell launch verbs.
//! - [`sync`]: auto-reset event creation/signalling/probing.
//! - [`fs`]: directory-change notification watches plus atomic file moves.
//! - [`test_window`]: message-only and owned overlapped test windows (test
//!   support only).
//! - [`cloak`]: experimental owned-helper private COM cloak and taskbar calls.
//! - [`overlay`]: owned layered-overlay carrier (active border, group
//!   underlay, drop preview): class registration, the accent-version window
//!   procedure, DIB presentation with caller painters, and placement.
//! - [`pager`]: owned per-output taskbar pager overlays: class, window
//!   procedure, GDI paint, per-window frames, click delivery, taskbar
//!   probes, and calibration flags.
//! - [`icons`]: icon-handle ownership plus worker-rendered pixel buffers.

pub mod accent;
pub mod cloak;
pub mod dpi;
pub mod enumeration;
pub mod fs;
pub mod gaming;
pub mod hooks;
pub mod icons;
pub mod input;
pub mod llhook;
pub mod overlay;
pub mod pager;
pub mod process;
pub mod pump;
pub mod settings_controls;
pub mod settings_dialog;
pub mod settings_window;
pub mod shell;
pub mod sync;
pub mod sysparam;
pub mod test_window;
pub mod tray;
pub mod window;

pub use accent::{AccentStage, OwnerAccentSource, accent_version};
pub use cloak::{
    EXPECTED_BUILD as CLOAK_EXPECTED_BUILD, check_experimental_gate, cloak_hwnd, cloak_mask_of,
    taskbar_add_tab, taskbar_delete_tab, uncloak_hwnd,
};
pub use dpi::{set_process_pm_v2, system_metric, system_metric_for_dpi, thread_pm_v2_active};
pub use enumeration::{
    MonitorInfo, enumerate_child_windows, enumerate_monitors, enumerate_windows,
};
pub use fs::{
    ChangeNotification, move_file_no_replace, move_file_replace, rearm_watch, watch_directory,
    watch_raw_handle, watch_signaled,
};
pub use hooks::{
    WinEvent, WinEventHook, WinEventSink, install_taskbar_hook, install_win_event_hook,
    install_win_event_hook_for_pid, set_win_event_sink, take_taskbar_events,
};
pub use icons::{AppIcon, IconPixels, class_icon, exe_icon, message_icon, render_icon};
pub use input::{
    KeyPairSend, async_key_down, cursor_pos, pack_mouse_lparam, prime_foreground_key,
    send_key_pair, tick_count,
};
#[cfg(test)]
pub use llhook::drive_snapkey;
pub use llhook::{
    HookVerdict, KeyEvent, SnapKeyHook, WinArrowHook, clear_snapkey_sink, clear_winarrow_sink,
    install_snapkey_hook, install_winarrow_hook, set_snapkey_sink, set_winarrow_sink,
    uninstall_snapkey_hook, uninstall_winarrow_hook,
};
pub use overlay::{
    OverlayClass, create_overlay_window, destroy_overlay, hide_overlay, overlay_alive, place_above,
    place_below, present_layered, present_preview,
};
pub use pager::{
    IconBlit, MiniSlot, PAGER_WINDOW_CLASS, PagerCalibrationSink, PagerClick, PagerClickSink,
    PagerFrame, PagerWindow, TaskbarProbe, autohide_from_state as pager_autohide_from_state,
    discover_secondary_taskbars, ensure_pager_class,
    note_taskbar_rediscover as note_pager_taskbar_rediscover, primary_taskbar_pid,
    query_appbar_state, read_primary_taskbar as read_primary_taskbar_probe,
    register_taskbar_message, take_rediscover as take_pager_rediscover,
    take_stale_hide as take_pager_stale_hide,
};
pub use process::{
    ProcError, ProcessHandle, WaitOutcome, creation_key_of, current_image_path, current_process_id,
    image_path_of, integrity_rid_current, integrity_rid_of, local_app_data_dir, open_process_with,
    session_of_pid, terminate_process, user_sid_of, wait_for_object,
};
pub use pump::{MsgWaitOutcome, TimerMessage, drain_messages, msg_wait, msg_wait_raw};
pub use pump::{kill_thread_timer, prime_thread_queue, set_thread_timer, wait_timer_message};
pub use settings_dialog::{DiscardEnd, PreviewEnd, ask_discard, ask_preview, show_info};
pub use shell::open_url;
pub use sync::{create_auto_reset_event, event_signaled, signal_event};
pub use sysparam::{get_win_arranging, set_win_arranging_session};
pub use test_window::{
    OwnedCreateError as FfiOwnedCreateError, OwnedWindow as FfiOwnedTestWindow,
    PumpError as FfiPumpError, TestWindow,
};
pub use tray::{
    FIRST_RUN_WINDOW_CLASS, FirstRunOutcome, TrayWindow, first_run_dialog as ffi_first_run_dialog,
    remove_stale_icon as ffi_remove_stale_icon,
};
pub use window::{
    MinmaxQuery, PositionAfter, attach_thread_input, class_name, cloaked, current_thread_id,
    desktop_window, dpi_for_window, extended_frame_bounds, foreground_window, get_prop,
    hit_test_at, is_iconic, is_visible, is_window, is_zoomed, owner_of, parent_of, post_close,
    query_minmax, reapply_frame, remove_prop, seed_minmax_info, set_foreground_window, set_prop,
    set_topmost, set_window_placement, set_window_pos, set_window_pos_after,
    set_window_pos_detailed, set_window_style, set_window_text, shell_window, show_window,
    show_window_async, show_window_prev_visible, take_prop, try_class_name, window_exstyle,
    window_pid, window_placement, window_rect, window_style, window_style_checked,
    window_thread_id,
};

fn hwnd_of(hwnd: u64) -> windows_sys::Win32::Foundation::HWND {
    hwnd as isize as windows_sys::Win32::Foundation::HWND
}

#[cfg(test)]
mod tests {
    use super::pack_mouse_lparam;

    #[test]
    fn mouse_lparam_packs_low_x_high_y_with_truncation() {
        // MAKELPARAM packing: low word x, high word y; narrowing is the
        // packing, so values outside `u16` truncate deterministically.
        assert_eq!(
            pack_mouse_lparam(0x1122_3344, 0x5566_7788) as u32,
            0x7788_3344
        );
        assert_eq!(pack_mouse_lparam(-1, -2) as u32, 0xFFFE_FFFF);
        assert_eq!(pack_mouse_lparam(100, 200) as u32, (200 << 16) | 100);
    }
}

//! Raw-input pointer-activity witness: mouse Raw Input.
//!
//! One `RegisterRawInputDevices` registration (usage page `0x01`, usage
//! `0x02`, `RIDEV_INPUTSINK`) targets the owner tray window. Its window
//! procedure forwards `WM_INPUT` here, where a successful mouse
//! `GetRawInputData` read increments one cumulative generation. The
//! generation is the only Q2 witness: the adapter's own `SetCursorPos`
//! generates no raw input, while injected input (`SendInput`) may, so
//! injected motion is conservatively treated as user activity. Windows
//! Raw Input carries no physical-versus-injected origin bit.
//!
//! Fail-closed: before successful registration (and after an explicit
//! unregister) [`pointer_activity`] reports `None`, never a fabricated
//! `Some(0)`. Registration failure leaves tiling running without the
//! witness. Writer and reader live on the owner loop thread; the atomics
//! only make the cross-module read cheap and torn-free.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use windows_sys::Win32::Foundation::{HWND, LPARAM};
use windows_sys::Win32::UI::Input::{
    GetRawInputData, RAWINPUTDEVICE, RAWINPUTHEADER, RID_INPUT, RIDEV_INPUTSINK, RIDEV_REMOVE,
    RIM_TYPEMOUSE, RegisterRawInputDevices,
};

/// Mouse usage page / id per the HID usage tables.
const MOUSE_USAGE_PAGE: u16 = 0x01;
const MOUSE_USAGE: u16 = 0x02;

/// Upper bound for one raw input record. `RAWINPUT` for a mouse is well
/// under this; anything larger is rejected rather than truncated.
const RAW_BUFFER_BYTES: usize = 128;

static INSTALLED: AtomicBool = AtomicBool::new(false);
static GENERATION: AtomicU64 = AtomicU64::new(0);

fn hwnd_of(hwnd: u64) -> HWND {
    hwnd as usize as HWND
}

/// Register mouse Raw Input against `hwnd` and make the witness readable.
/// Returns false on registration failure, leaving the witness unavailable;
/// no cursor state is touched. The cumulative generation is never reset:
/// a tray/window recreation continues the same monotonic sequence, so a
/// core `pointerBaseline` taken before re-installation cannot alias a
/// reused value.
pub fn install(hwnd: u64) -> bool {
    let devices = [RAWINPUTDEVICE {
        usUsagePage: MOUSE_USAGE_PAGE,
        usUsage: MOUSE_USAGE,
        dwFlags: RIDEV_INPUTSINK,
        hwndTarget: hwnd_of(hwnd),
    }];
    let ok = unsafe {
        RegisterRawInputDevices(
            devices.as_ptr(),
            devices.len() as u32,
            std::mem::size_of::<RAWINPUTDEVICE>() as u32,
        )
    } != 0;
    if ok {
        INSTALLED.store(true, Ordering::Release);
    } else {
        // Never keep a stale readable witness across a failed re-install.
        INSTALLED.store(false, Ordering::Release);
    }
    ok
}

/// Remove the registration and make [`pointer_activity`] unavailable.
/// Called from the tray teardown; also safe when never installed.
pub fn uninstall() {
    INSTALLED.store(false, Ordering::Release);
    let devices = [RAWINPUTDEVICE {
        usUsagePage: MOUSE_USAGE_PAGE,
        usUsage: MOUSE_USAGE,
        dwFlags: RIDEV_REMOVE,
        hwndTarget: std::ptr::null_mut(),
    }];
    unsafe {
        RegisterRawInputDevices(
            devices.as_ptr(),
            devices.len() as u32,
            std::mem::size_of::<RAWINPUTDEVICE>() as u32,
        );
    }
}

/// Cumulative raw-input pointer generation (device or injected), `None`
/// until a registration succeeded. Exactly one successful mouse read
/// advances it.
pub fn pointer_activity() -> Option<u64> {
    if INSTALLED.load(Ordering::Acquire) {
        Some(GENERATION.load(Ordering::Acquire))
    } else {
        None
    }
}

/// Consume one `WM_INPUT` delivered to the registered window. Returns true
/// only when a successful mouse record was read; the generation advances
/// exactly then. Keyboard/other records are consumed without counting.
pub fn handle_wm_input(lparam: LPARAM) -> bool {
    let hraw = lparam as usize as *mut std::ffi::c_void;
    let header_size = std::mem::size_of::<RAWINPUTHEADER>() as u32;
    let mut size: u32 = 0;
    let probe = unsafe {
        GetRawInputData(
            hraw,
            RID_INPUT,
            std::ptr::null_mut(),
            &mut size,
            header_size,
        )
    };
    if probe == u32::MAX
        || (size as usize) < header_size as usize
        || size as usize > RAW_BUFFER_BYTES
    {
        return false;
    }
    let mut buffer = [0u8; RAW_BUFFER_BYTES];
    let read = unsafe {
        GetRawInputData(
            hraw,
            RID_INPUT,
            buffer.as_mut_ptr().cast(),
            &mut size,
            header_size,
        )
    };
    if read == u32::MAX || read == 0 || (read as usize) < header_size as usize {
        return false;
    }
    // SAFETY: the successful `GetRawInputData` above fully wrote at least
    // `header_size` bytes at `buffer`'s start. The byte buffer has alignment
    // 1, so the header is read unaligned rather than borrowed as a reference;
    // `RAWINPUTHEADER` is plain old data.
    let header = unsafe { std::ptr::read_unaligned(buffer.as_ptr().cast::<RAWINPUTHEADER>()) };
    if header.dwType != RIM_TYPEMOUSE {
        return false;
    }
    GENERATION.fetch_add(1, Ordering::Release);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn witness_unavailable_until_registered() {
        // No live registration ever runs in the unit lane: the honest
        // value is absence, never a synthesized zero.
        uninstall();
        assert_eq!(pointer_activity(), None);
    }

    #[test]
    fn generation_advances_only_on_a_mouse_read() {
        // Direct handler-path check without the OS: a bogus handle cannot
        // produce a mouse record, so neither the return nor the generation
        // may claim one. This never touches real raw-input state.
        uninstall();
        assert!(!handle_wm_input(0));
        assert_eq!(pointer_activity(), None);
    }

    // ------------------------------------------------------------------
    // Live probe (ignored by default): proves `SetCursorPos` does not
    // advance the Raw Input generation while this window's message pump
    // runs. It moves the real cursor, so it obeys the live-input safety
    // boundary: the foreground window must resolve to this test process's
    // own probe window immediately before every move, the cursor preimage
    // is restored under the same gate, and any failed proof aborts before
    // input instead of retrying elsewhere.
    // ------------------------------------------------------------------

    use std::time::{Duration, Instant};

    /// One owned top-level probe window whose procedure consumes `WM_INPUT`
    /// through [`handle_wm_input`].
    struct ProbeWindow {
        hwnd: u64,
    }

    impl ProbeWindow {
        fn create() -> Option<Self> {
            use windows_sys::Win32::Foundation::{ERROR_CLASS_ALREADY_EXISTS, GetLastError};
            use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
            use windows_sys::Win32::UI::WindowsAndMessaging::{
                CreateWindowExW, RegisterClassW, WNDCLASSW, WS_EX_TOPMOST, WS_OVERLAPPEDWINDOW,
                WS_VISIBLE,
            };
            let class: Vec<u16> = "OmniTilerRawInputProbe\0".encode_utf16().collect();
            let title: Vec<u16> = "OmniTiler raw-input probe\0".encode_utf16().collect();
            let hinst = unsafe { GetModuleHandleW(std::ptr::null()) };
            let mut cls: WNDCLASSW = unsafe { std::mem::zeroed() };
            cls.lpfnWndProc = Some(probe_wndproc);
            cls.hInstance = hinst;
            cls.lpszClassName = class.as_ptr();
            let atom = unsafe { RegisterClassW(&cls) };
            if atom == 0 && unsafe { GetLastError() } != ERROR_CLASS_ALREADY_EXISTS {
                return None;
            }
            let hwnd = unsafe {
                CreateWindowExW(
                    WS_EX_TOPMOST,
                    class.as_ptr(),
                    title.as_ptr(),
                    WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                    120,
                    120,
                    420,
                    260,
                    std::ptr::null_mut(),
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
            })
        }
    }

    impl Drop for ProbeWindow {
        fn drop(&mut self) {
            unsafe {
                windows_sys::Win32::UI::WindowsAndMessaging::DestroyWindow(
                    self.hwnd as usize as windows_sys::Win32::Foundation::HWND,
                );
            }
        }
    }

    unsafe extern "system" fn probe_wndproc(
        hwnd: windows_sys::Win32::Foundation::HWND,
        msg: u32,
        wp: windows_sys::Win32::Foundation::WPARAM,
        lp: windows_sys::Win32::Foundation::LPARAM,
    ) -> windows_sys::Win32::Foundation::LRESULT {
        if msg == windows_sys::Win32::UI::WindowsAndMessaging::WM_INPUT {
            handle_wm_input(lp);
        }
        unsafe { windows_sys::Win32::UI::WindowsAndMessaging::DefWindowProcW(hwnd, msg, wp, lp) }
    }

    /// Unregister the witness even when an assertion unwinds.
    struct WitnessGuard;

    impl Drop for WitnessGuard {
        fn drop(&mut self) {
            uninstall();
        }
    }

    /// Restore the cursor preimage only while the probe window still holds
    /// the foreground in this process (the input-ownership invariant).
    struct CursorRestore {
        pre: (i32, i32),
        hwnd: u64,
        armed: bool,
    }

    impl Drop for CursorRestore {
        fn drop(&mut self) {
            if self.armed && owned_foreground(self.hwnd) {
                crate::ffi::input::set_cursor_pos(self.pre.0, self.pre.1);
            }
        }
    }

    fn current_pid() -> u32 {
        crate::ffi::current_process_id()
    }

    fn owned_foreground(hwnd: u64) -> bool {
        crate::ffi::foreground_window() == hwnd && crate::ffi::window_pid(hwnd) == current_pid()
    }

    fn root_window(mut hwnd: u64) -> u64 {
        loop {
            let parent = crate::ffi::parent_of(hwnd);
            let next = if parent != 0 {
                parent
            } else {
                crate::ffi::owner_of(hwnd)
            };
            if next == 0 || next == hwnd {
                return hwnd;
            }
            hwnd = next;
        }
    }

    fn pump_for(ms: u64) {
        let start = Instant::now();
        while start.elapsed() < Duration::from_millis(ms) {
            crate::ffi::drain_messages();
            std::thread::sleep(Duration::from_millis(5));
        }
        crate::ffi::drain_messages();
    }

    #[test]
    #[ignore = "live probe; moves the cursor only under a proven test-owned foreground"]
    fn live_set_cursor_pos_does_not_advance_raw_input_generation() {
        uninstall();
        let window = ProbeWindow::create().expect("probe window creation");
        let hwnd = window.hwnd;
        if !crate::ffi::set_foreground_window(hwnd) {
            panic!("refused: SetForegroundWindow failed; no input sent");
        }
        pump_for(150);
        if !owned_foreground(hwnd) {
            panic!(
                "refused: foreground {} is not the test-owned probe window; no input sent",
                crate::ffi::foreground_window()
            );
        }
        let pre = crate::ffi::cursor_pos().expect("live cursor readable");
        let mut restore = CursorRestore {
            pre,
            hwnd,
            armed: false,
        };
        // Two points inside the probe window, verified through
        // WindowFromPoint before any position write.
        let targets = [(250, 220), (330, 250)];
        for target in targets {
            let hit = crate::ffi::window_at_point(target.0, target.1);
            if hit == 0 || root_window(hit) != hwnd {
                panic!("refused: warp target point is not under the probe window; no input sent");
            }
        }
        if !install(hwnd) {
            panic!("refused: RegisterRawInputDevices failed; no input sent");
        }
        let _witness = WitnessGuard;
        pump_for(100);
        let baseline = pointer_activity().expect("witness installed");
        restore.armed = true;
        // Leg 1: a real position setter must not look like hardware input.
        for (index, target) in targets.into_iter().enumerate() {
            if !owned_foreground(hwnd) {
                panic!("refused: foreground changed before SetCursorPos leg; no input sent");
            }
            let hit = crate::ffi::window_at_point(target.0, target.1);
            if hit == 0 || root_window(hit) != hwnd {
                panic!("refused: SetCursorPos target is not under the probe window");
            }
            assert!(
                crate::ffi::set_cursor_pos(target.0, target.1),
                "SetCursorPos leg {index} failed"
            );
            pump_for(200);
        }
        let after_sets = pointer_activity().expect("witness installed");
        assert_eq!(
            after_sets, baseline,
            "SetCursorPos advanced the Raw Input generation (baseline {baseline}, after {after_sets})"
        );
        // Leg 2: witness liveness. Raw Input does not distinguish injected
        // from physical input, so this leg never claims hardware origin; it
        // proves the same handler that must see user motion actually sees a
        // delivered mouse record. A negative/absent witness fails the probe
        // and therefore cannot support a capability flip.
        assert!(
            owned_foreground(hwnd),
            "refused: foreground changed before synthetic leg"
        );
        let inserted = crate::ffi::input::send_mouse_move_relative(1, 0);
        assert!(
            inserted > 0,
            "synthetic move was not inserted; Raw Input witness liveness unproven"
        );
        pump_for(200);
        let after_inject = pointer_activity().expect("witness installed");
        println!(
            "raw-input witness: SetCursorPos gen {baseline} -> {after_sets} (unchanged); \
             synthetic move inserted={inserted}, gen {after_sets} -> {after_inject}"
        );
        assert!(
            after_inject > after_sets,
            "Raw Input witness did not observe injected motion \
             (gen {after_sets} -> {after_inject}); witness liveness unproven"
        );
        assert!(
            owned_foreground(hwnd),
            "probe ownership changed before teardown"
        );
    }
}

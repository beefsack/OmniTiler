//! Low-level hook ownership (`WH_KEYBOARD_LL`, `WH_MOUSE_LL`).
//!
//! The `extern "system"` trampolines live here and nowhere else.
//! Each trampoline copies the OS payload into a safe event struct, then
//! invokes the installing thread's policy callback with plain values only
//! (never a raw pointer, never a validity obligation on the safe caller).
//! The callback verdict selects consume (`1`) or pass-through via
//! `CallNextHookEx` with the original parameters, exactly like the previous
//! inline callbacks. A panicking policy fails open to pass-through instead
//! of unwinding across the `extern` edge. Hook handles are thread-confined
//! owners (`!Send`/`!Sync` structurally, no custom impls); uninstall is
//! explicit and reports honestly, mirroring the previous retry semantics.
//! Test drives synthesize scalar inputs through the real trampolines (no
//! arbitrary pointer injection surface).
//!
//! The mouse tap is observation-only motion reporting for
//! focus-follows-cursor: it never consumes (always passes through), never
//! synthesizes input, and ignores injected-flagged motion. It installs
//! only while focus-follows-cursor is armed and no game suspend holds; a
//! failed release retains the handle and retries with backoff. Cost while
//! installed: one extra hook-chain link per system mouse message on the
//! installing thread, plus one coalesced owner turn per motion burst. Game
//! input between game start and the next tick still traverses the tap
//! (detection interval, unmeasured).

use std::cell::RefCell;
use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(test)]
use windows_sys::Win32::Foundation::POINT;
use windows_sys::Win32::Foundation::{HMODULE, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, HC_ACTION, HHOOK, HOOKPROC, KBDLLHOOKSTRUCT, MSLLHOOKSTRUCT, SetWindowsHookExW,
    UnhookWindowsHookEx, WH_KEYBOARD_LL, WH_MOUSE_LL, WM_MOUSEMOVE,
};

/// One copied low-level keyboard event: plain values only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyEvent {
    pub vk: u32,
    pub scan: u32,
    pub flags: u32,
    pub time: u32,
    pub extra: u64,
}

/// Policy verdict for one hook event: swallow it (`Consume`, returns `1`
/// with no further delivery) or pass it down the hook chain (`Pass`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookVerdict {
    Consume,
    Pass,
}

/// Thread-local policy for one keyboard hook: installed by the owner before
/// the hook, cleared with it. Thread-local (not process-global) so each
/// hook thread keeps its own classifier and test threads stay isolated.
type KeyPolicy = fn(KeyEvent) -> HookVerdict;

thread_local! {
    static SNAPKEY_SINK: RefCell<Option<KeyPolicy>> = const { RefCell::new(None) };
    static WINARROW_SINK: RefCell<Option<KeyPolicy>> = const { RefCell::new(None) };
}

/// Install the snapkey classifier as this thread's keyboard policy. Called
/// by the owner before installing the hook (no native effects).
pub fn set_snapkey_sink(sink: KeyPolicy) {
    SNAPKEY_SINK.with(|s| *s.borrow_mut() = Some(sink));
}

/// Clear this thread's snapkey policy (after uninstall or failed install).
pub fn clear_snapkey_sink() {
    SNAPKEY_SINK.with(|s| *s.borrow_mut() = None);
}

/// Install the winarrow classifier as this thread's keyboard policy.
pub fn set_winarrow_sink(sink: KeyPolicy) {
    WINARROW_SINK.with(|s| *s.borrow_mut() = Some(sink));
}

/// Clear this thread's winarrow policy.
pub fn clear_winarrow_sink() {
    WINARROW_SINK.with(|s| *s.borrow_mut() = None);
}

fn call_next(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) }
}

fn drive_key(sink: Option<KeyPolicy>, event: KeyEvent) -> Option<HookVerdict> {
    let sink = sink?;
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| sink(event))).ok()
}

unsafe extern "system" fn snapkey_trampoline(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code as u32 != HC_ACTION {
        return call_next(code, wparam, lparam);
    }
    // Single-copy of the stable `KBDLLHOOKSTRUCT` ABI: the release/down
    // point rides the event itself, so no later re-read can tear it.
    let kb = unsafe { &*(lparam as *const KBDLLHOOKSTRUCT) };
    let event = KeyEvent {
        vk: kb.vkCode,
        scan: kb.scanCode,
        flags: kb.flags,
        time: kb.time,
        extra: kb.dwExtraInfo as u64,
    };
    // The borrow ends before the policy runs, so a reentrant delivery
    // (the mask `SendInput` pair below the policy) never re-borrows.
    let sink = SNAPKEY_SINK.with(|s| *s.borrow());
    match drive_key(sink, event) {
        Some(HookVerdict::Consume) => 1,
        _ => call_next(code, wparam, lparam),
    }
}

unsafe extern "system" fn winarrow_trampoline(
    code: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if code as u32 != HC_ACTION {
        return call_next(code, wparam, lparam);
    }
    let kb = unsafe { &*(lparam as *const KBDLLHOOKSTRUCT) };
    let event = KeyEvent {
        vk: kb.vkCode,
        scan: kb.scanCode,
        flags: kb.flags,
        time: kb.time,
        extra: kb.dwExtraInfo as u64,
    };
    let sink = WINARROW_SINK.with(|s| *s.borrow());
    match drive_key(sink, event) {
        Some(HookVerdict::Consume) => 1,
        _ => call_next(code, wparam, lparam),
    }
}

fn install_ll(id: i32, proc: HOOKPROC) -> Option<HHOOK> {
    let hmod: HMODULE = unsafe { GetModuleHandleW(std::ptr::null()) };
    let hook = unsafe { SetWindowsHookExW(id, proc, hmod, 0) };
    if hook.is_null() { None } else { Some(hook) }
}

fn uninstall_ll(raw: HHOOK) -> bool {
    unsafe { UnhookWindowsHookEx(raw) != 0 }
}

/// Installed snapkey keyboard hook; confined to the installing thread.
/// Explicit [`uninstall_snapkey_hook`] releases it (process exit releases
/// it in any case, but success is never fabricated).
pub struct SnapKeyHook {
    raw: HHOOK,
    _confine: PhantomData<Rc<()>>,
}

/// Installed winarrow keyboard hook; same confinement contract.
pub struct WinArrowHook {
    raw: HHOOK,
    _confine: PhantomData<Rc<()>>,
}

/// Install the snapkey low-level keyboard hook on the calling thread; the
/// caller must pump messages so the callback runs. `None` when the API
/// refuses it (the caller degrades, as before).
pub fn install_snapkey_hook() -> Option<SnapKeyHook> {
    install_ll(WH_KEYBOARD_LL, Some(snapkey_trampoline)).map(|raw| SnapKeyHook {
        raw,
        _confine: PhantomData,
    })
}

/// Install the winarrow low-level keyboard hook on the calling thread.
/// `None` when the API refuses it.
pub fn install_winarrow_hook() -> Option<WinArrowHook> {
    install_ll(WH_KEYBOARD_LL, Some(winarrow_trampoline)).map(|raw| WinArrowHook {
        raw,
        _confine: PhantomData,
    })
}

/// Release the snapkey hook. State mutates only on success: a failed
/// unhook returns `false` with the handle retained for a creator-side
/// retry.
pub fn uninstall_snapkey_hook(hook: &mut SnapKeyHook) -> bool {
    if !uninstall_ll(hook.raw) {
        return false;
    }
    hook.raw = std::ptr::null_mut();
    true
}

/// Release the winarrow hook; same retain-on-failure contract.
pub fn uninstall_winarrow_hook(hook: &mut WinArrowHook) -> bool {
    if !uninstall_ll(hook.raw) {
        return false;
    }
    hook.raw = std::ptr::null_mut();
    true
}

/// Test-only keyboard drive (no hooks, no native input calls): feeds one
/// fabricated low-level key event through the real trampoline, so the
/// single-copy plus verdict mapping runs exactly as in production. Safe
/// inputs only (see the policy wrappers for the `SendInput` restriction);
/// the fabricated struct lives on the stack for the synchronous call.
#[cfg(test)]
pub fn drive_snapkey(vk: u32, scan: u32, flags: u32, extra: u64) -> LRESULT {
    let kb = KBDLLHOOKSTRUCT {
        vkCode: vk,
        scanCode: scan,
        flags,
        time: 0,
        dwExtraInfo: extra as usize,
    };
    unsafe { snapkey_trampoline(HC_ACTION as i32, 0, &kb as *const KBDLLHOOKSTRUCT as isize) }
}

/// Injected-motion flags (`MSLLHOOKSTRUCT.flags`): our own warp and any
/// other synthetic provider must never wake the loop (feedback loop).
const LLMHF_INJECTED: u32 = 1;
const LLMHF_LOWER_IL_INJECTED: u32 = 2;

/// Coalesced genuine-motion sequence: bumped once per non-injected
/// `WM_MOUSEMOVE` through the tap. The owner consumes it per turn, so a
/// burst collapses to one wake.
static MOUSE_MOVE_SEQ: AtomicU64 = AtomicU64::new(0);

/// Test-only serialization for the global motion sequence: parallel test
/// threads share [`MOUSE_MOVE_SEQ`], so motion drive plus wake-take
/// sequences hold this across the whole assertion burst.
#[cfg(test)]
pub static MOUSE_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Current genuine-motion sequence (observation only, never consumed here).
pub fn mouse_move_seq() -> u64 {
    MOUSE_MOVE_SEQ.load(Ordering::SeqCst)
}

unsafe extern "system" fn mouse_trampoline(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code as u32 != HC_ACTION {
        return call_next(code, wparam, lparam);
    }
    if wparam as u32 != WM_MOUSEMOVE {
        return call_next(code, wparam, lparam);
    }
    // Single-copy of the `MSLLHOOKSTRUCT` ABI, then filter only:
    // injected-flagged motion never wakes (synthetic providers that mark
    // their input), and the tap never consumes (always passes through the
    // chain). A raw position setter like our own warp may still surface
    // here as motion: that self-wake is bounded (one coalesced wake; the
    // warped cursor sits on the focused target, so FFC mints nothing new)
    // and never consumes input.
    let stamp = unsafe { &*(lparam as *const MSLLHOOKSTRUCT) };
    if stamp.flags & (LLMHF_INJECTED | LLMHF_LOWER_IL_INJECTED) == 0 {
        MOUSE_MOVE_SEQ.fetch_add(1, Ordering::SeqCst);
    }
    call_next(code, wparam, lparam)
}

/// Installed observation-only mouse-motion tap; confined to the installing
/// thread. Never consumes: every event passes through the hook chain.
/// Explicit [`uninstall_mouse_hook`] releases it (process exit releases it
/// in any case, but success is never fabricated).
pub struct MouseHook {
    raw: HHOOK,
    _confine: PhantomData<Rc<()>>,
}

/// Install the mouse-motion tap on the calling thread; the caller must pump
/// messages so the callback runs. `None` when the API refuses it (the
/// caller degrades to owner-wake sampling, as before).
pub fn install_mouse_hook() -> Option<MouseHook> {
    install_ll(WH_MOUSE_LL, Some(mouse_trampoline)).map(|raw| MouseHook {
        raw,
        _confine: PhantomData,
    })
}

/// Release the mouse tap. State mutates only on success: a failed unhook
/// returns `false` with the handle retained for a creator-side retry.
pub fn uninstall_mouse_hook(hook: &mut MouseHook) -> bool {
    if !uninstall_ll(hook.raw) {
        return false;
    }
    hook.raw = std::ptr::null_mut();
    true
}

/// Test-only tap handle: a lifecycle stand-in with no OS registration,
/// so unit tests exercise the real `Some`-handle paths (install success,
/// retained failed release, drop release) through the injected seam.
#[cfg(test)]
pub fn test_hook() -> MouseHook {
    MouseHook {
        raw: std::ptr::null_mut(),
        _confine: PhantomData,
    }
}

/// Test-only mouse drive (no hooks, no native input calls): feeds one
/// fabricated low-level mouse message through the real trampoline, so the
/// filter plus sequence bump runs exactly as in production. The fabricated
/// struct lives on the stack for the synchronous call.
#[cfg(test)]
pub fn drive_mouse_move(wparam: u32, x: i32, y: i32, flags: u32) -> LRESULT {
    let stamp = MSLLHOOKSTRUCT {
        pt: POINT { x, y },
        mouseData: 0,
        flags,
        time: 0,
        dwExtraInfo: 0,
    };
    unsafe {
        mouse_trampoline(
            HC_ACTION as i32,
            wparam as usize,
            &stamp as *const MSLLHOOKSTRUCT as isize,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn consume_key(_: KeyEvent) -> HookVerdict {
        HookVerdict::Consume
    }

    fn pass_key(_: KeyEvent) -> HookVerdict {
        HookVerdict::Pass
    }

    #[test]
    fn keyboard_verdict_maps_consume_to_one_and_pass_to_chain() {
        // No hooks installed: consume returns `1` with no chain call, and
        // pass-through reaches the (empty) chain instead of consuming.
        set_snapkey_sink(consume_key);
        assert_eq!(drive_snapkey(0x41, 0, 0, 0), 1);
        set_snapkey_sink(pass_key);
        assert_ne!(drive_snapkey(0x41, 0, 0, 0), 1);
        clear_snapkey_sink();
        // No sink: fail-open pass-through, never consume.
        assert_ne!(drive_snapkey(0x41, 0, 0, 0), 1);
    }

    #[test]
    fn non_action_codes_pass_without_policy() {
        // A consuming policy never runs for non-`HC_ACTION` codes: the
        // trampoline passes through before copying any payload.
        set_snapkey_sink(consume_key);
        let kb = KBDLLHOOKSTRUCT {
            vkCode: 0x41,
            scanCode: 0,
            flags: 0,
            time: 0,
            dwExtraInfo: 0,
        };
        let verdict = unsafe { snapkey_trampoline(1, 0, &kb as *const KBDLLHOOKSTRUCT as isize) };
        assert_ne!(verdict, 1);
        clear_snapkey_sink();
    }

    #[test]
    fn mouse_tap_reports_motion_and_ignores_the_rest() {
        let _guard = MOUSE_TEST_LOCK.lock().expect("test lock");
        // Genuine motion bumps the sequence and always passes through
        // (never consumes); injected motion (our own warp) and non-move
        // messages never bump.
        let before = mouse_move_seq();
        assert_ne!(drive_mouse_move(WM_MOUSEMOVE, 10, 10, 0), 1);
        assert_eq!(mouse_move_seq(), before + 1);
        assert_ne!(drive_mouse_move(WM_MOUSEMOVE, 11, 11, LLMHF_INJECTED), 1);
        assert_ne!(
            drive_mouse_move(WM_MOUSEMOVE, 12, 12, LLMHF_LOWER_IL_INJECTED),
            1
        );
        assert_eq!(mouse_move_seq(), before + 1);
        // Non-move messages (buttons, wheel) never wake the loop.
        assert_ne!(drive_mouse_move(0x0201, 13, 13, 0), 1);
        assert_eq!(mouse_move_seq(), before + 1);
        // Non-action codes pass without touching the sequence.
        let stamp = MSLLHOOKSTRUCT {
            pt: POINT { x: 14, y: 14 },
            mouseData: 0,
            flags: 0,
            time: 0,
            dwExtraInfo: 0,
        };
        assert_ne!(
            unsafe {
                mouse_trampoline(
                    1,
                    WM_MOUSEMOVE as usize,
                    &stamp as *const MSLLHOOKSTRUCT as isize,
                )
            },
            1
        );
        assert_eq!(mouse_move_seq(), before + 1);
    }
}

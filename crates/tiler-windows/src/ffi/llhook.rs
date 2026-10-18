//! Low-level hook ownership (`WH_KEYBOARD_LL`).
//!
//! The two `extern "system"` trampolines live here and nowhere else.
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

use std::cell::RefCell;
use std::marker::PhantomData;
use std::rc::Rc;

use windows_sys::Win32::Foundation::{HMODULE, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, HC_ACTION, HHOOK, HOOKPROC, KBDLLHOOKSTRUCT, SetWindowsHookExW,
    UnhookWindowsHookEx, WH_KEYBOARD_LL,
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
}

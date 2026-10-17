//! Thread message pump and the alertable outer wait.
//!
//! The `MSG` buffer and every handle pointer stay inside these functions.
//! Wait handles cross as opaque values only; an invalid handle reports
//! `Failed`, never undefined behavior.

use std::os::windows::io::RawHandle;

use windows_sys::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, KillTimer, MSG, MsgWaitForMultipleObjectsEx, PM_NOREMOVE,
    PM_REMOVE, PeekMessageW, QS_ALLINPUT, SetTimer, TranslateMessage, WM_TIMER,
};

/// Drain pending loop-thread messages (delivering hook callbacks) without
/// waiting. Returns true when anything flowed.
pub fn drain_messages() -> bool {
    let mut msg: MSG = unsafe { std::mem::zeroed() };
    let mut flowed = false;
    loop {
        let has = unsafe { PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) };
        if has == 0 {
            break;
        }
        flowed = true;
        unsafe {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    flowed
}

/// Outcome of one alertable wait over `handles` plus the thread message
/// queue. `Signaled(i)` names the wait handle index; message-queue
/// readiness reports `Message` distinctly from signaled handles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MsgWaitOutcome {
    Signaled(usize),
    Message,
    Timeout,
    Failed,
}

const MAX_WAIT: usize = 8;

/// Raw `MsgWaitForMultipleObjectsEx` return code over `handles` plus the
/// thread message queue (the `WAIT_OBJECT_0 + n` contract: handles first,
/// then the queue). The caller maps it with its own pure classifier, so
/// the code (a plain value, never a pointer) crosses safely.
pub fn msg_wait_raw(handles: &[RawHandle], timeout_ms: u32) -> u32 {
    let mut native = [std::ptr::null_mut(); MAX_WAIT];
    let mut count: u32 = 0;
    for raw in handles {
        if (count as usize) >= MAX_WAIT {
            break;
        }
        native[count as usize] = *raw;
        count += 1;
    }
    unsafe {
        if count == 0 {
            MsgWaitForMultipleObjectsEx(0, std::ptr::null(), timeout_ms, QS_ALLINPUT, 0)
        } else {
            MsgWaitForMultipleObjectsEx(count, native.as_ptr(), timeout_ms, QS_ALLINPUT, 0)
        }
    }
}

/// Wait for any of `handles` (opaque waitable values borrowed from their
/// owners, e.g. `OwnedHandle`s and watch handles) or a thread message,
/// up to `timeout_ms`. `u32::MAX` blocks at healthy idle.
pub fn msg_wait(handles: &[RawHandle], timeout_ms: u32) -> MsgWaitOutcome {
    const WAIT_TIMEOUT: u32 = 258;
    let result = msg_wait_raw(handles, timeout_ms);
    if result == WAIT_TIMEOUT {
        MsgWaitOutcome::Timeout
    } else if (result as usize) < handles.len().min(MAX_WAIT) {
        MsgWaitOutcome::Signaled(result as usize)
    } else if result as usize == handles.len().min(MAX_WAIT) {
        MsgWaitOutcome::Message
    } else {
        MsgWaitOutcome::Failed
    }
}

/// Prime the calling thread's message queue so a message loop can run
/// (delivers nothing, waits for nothing).
pub fn prime_thread_queue() {
    let mut msg: MSG = unsafe { std::mem::zeroed() };
    unsafe { PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_NOREMOVE) };
}

/// Arm a periodic thread timer firing `WM_TIMER` every `interval_ms`; `0`
/// when the API refuses it (the caller fails closed, as before).
pub fn set_thread_timer(interval_ms: u32) -> usize {
    unsafe { SetTimer(std::ptr::null_mut(), 0, interval_ms, None) }
}

/// Disarm a thread timer armed by [`set_thread_timer`].
pub fn kill_thread_timer(timer_id: usize) {
    unsafe { KillTimer(std::ptr::null_mut(), timer_id) };
}

/// One blocking wait step of a timer-driven message loop: non-timer
/// messages are dispatched internally and skipped, so the caller only
/// sees timer ticks, ordered shutdown (`Quit`), or a broken pump.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerMessage {
    Tick,
    Quit,
    Failed,
}

pub fn wait_timer_message() -> TimerMessage {
    loop {
        let mut msg: MSG = unsafe { std::mem::zeroed() };
        let r = unsafe { GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) };
        if r == 0 {
            return TimerMessage::Quit;
        }
        if r == -1 {
            return TimerMessage::Failed;
        }
        if msg.message == WM_TIMER {
            return TimerMessage::Tick;
        }
    }
}

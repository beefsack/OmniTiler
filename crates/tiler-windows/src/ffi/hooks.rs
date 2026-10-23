//! WinEvent hook ownership.
//!
//! `SetWinEventHook` takes no caller context, so the single `extern`
//! trampoline lives here and forwards to a process-global safe sink
//! installed by the owner loop. Hook handles are RAII: drop unhooks.
//! Event filtering (which edges become wakes) stays with the caller.

use std::sync::{Arc, Mutex, OnceLock};

use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent};

/// One delivered WinEvent, plain values only.
#[derive(Debug, Clone, Copy)]
pub struct WinEvent {
    pub event: u32,
    pub hwnd: u64,
    pub id_object: i32,
    pub thread: u32,
    pub time: u32,
}

/// Safe receiver for hook deliveries. Runs on the installing (loop)
/// thread while it pumps messages; implementations must follow the same
/// lock discipline as before (never hold owner locks across a pump).
pub trait WinEventSink: Send + Sync {
    fn on_win_event(&self, event: WinEvent);
}

fn global_sink() -> &'static Mutex<Option<Arc<dyn WinEventSink>>> {
    static SINK: OnceLock<Mutex<Option<Arc<dyn WinEventSink>>>> = OnceLock::new();
    SINK.get_or_init(|| Mutex::new(None))
}

/// Install the process-global hook-event sink (the owner loop calls this
/// once before installing its hooks; later installs replace it).
pub fn set_win_event_sink(sink: Arc<dyn WinEventSink>) {
    if let Ok(mut guard) = global_sink().lock() {
        *guard = Some(sink);
    }
}

unsafe extern "system" fn win_event_trampoline(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    id_object: i32,
    _id_child: i32,
    thread: u32,
    time: u32,
) {
    let sink = global_sink().lock().ok().and_then(|guard| guard.clone());
    if let Some(sink) = sink {
        // The sink is safe caller code: a panic must never unwind across
        // the `extern` edge (fail closed, drop the event).
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            sink.on_win_event(WinEvent {
                event,
                hwnd: hwnd as usize as u64,
                id_object,
                thread,
                time,
            });
        }));
    }
}

/// One installed WinEvent hook; drop unhooks. Confined to the installing
/// thread like before (the handle type is not `Send`/`Sync`).
pub struct WinEventHook {
    raw: HWINEVENTHOOK,
}

impl Drop for WinEventHook {
    fn drop(&mut self) {
        if !self.raw.is_null() {
            unsafe {
                UnhookWinEvent(self.raw);
            }
        }
    }
}

/// Install one out-of-context hook for `[event_min, event_max]`;
/// `None` when the API refuses it (the caller degrades, as before).
pub fn install_win_event_hook(event_min: u32, event_max: u32) -> Option<WinEventHook> {
    install_win_event_hook_for(event_min, event_max, 0)
}

/// PID-scoped install: only events from `pid` are delivered to the sink.
/// Thread scope stays all threads of that process (`idthread = 0`). The
/// existing all-process wrapper above is unchanged.
pub fn install_win_event_hook_for_pid(
    event_min: u32,
    event_max: u32,
    pid: u32,
) -> Option<WinEventHook> {
    install_win_event_hook_for(event_min, event_max, pid)
}

fn install_win_event_hook_for(event_min: u32, event_max: u32, pid: u32) -> Option<WinEventHook> {
    let raw = unsafe {
        SetWinEventHook(
            event_min,
            event_max,
            std::ptr::null_mut(),
            Some(win_event_trampoline),
            pid,
            0,
            windows_sys::Win32::UI::WindowsAndMessaging::WINEVENT_OUTOFCONTEXT,
        )
    };
    if raw.is_null() {
        return None;
    }
    Some(WinEventHook { raw })
}

/// Taskbar-scoped WinEvent queue: only the pid-scoped taskbar hooks below
/// push here (Explorer-owned create/destroy/show/hide/location traffic),
/// so game and app windows never appear. Plain values, same lock
/// discipline as the main queue (never held across a message pump).
/// Storage itself is bounded ([`TASKBAR_DRAIN_CAP`]): the push path drops
/// excess and records coalesced overflow, so no backlog can accumulate
/// between drains.
#[cfg(not(test))]
fn taskbar_queue() -> &'static Mutex<TaskbarQueue> {
    static QUEUE: OnceLock<Mutex<TaskbarQueue>> = OnceLock::new();
    QUEUE.get_or_init(|| Mutex::new(TaskbarQueue::new()))
}

#[cfg(test)]
thread_local! {
    static TEST_TASKBAR_QUEUE: Arc<Mutex<TaskbarQueue>> = Arc::new(Mutex::new(TaskbarQueue::new()));
}

/// Test storage for [`taskbar_queue`]: each test thread owns its queue, so
/// parallel drain tests never observe each other's edges. Push/take/drain
/// code is unchanged and runs the production path.
#[cfg(test)]
fn taskbar_queue() -> Arc<Mutex<TaskbarQueue>> {
    TEST_TASKBAR_QUEUE.with(Arc::clone)
}

/// Max retained taskbar events: overflow coalesces to the overflow flag
/// (sheer volume is itself change) at enqueue time, so the queue never
/// grows without bound and validation stays bounded.
pub const TASKBAR_DRAIN_CAP: usize = 64;

/// Bounded taskbar-event store: at most [`TASKBAR_DRAIN_CAP`] retained
/// events plus one coalesced-overflow bit. Pure push/take logic so the cap
/// and clean-take contract unit-test without globals.
struct TaskbarQueue {
    events: Vec<WinEvent>,
    overflowed: bool,
}

impl TaskbarQueue {
    fn new() -> Self {
        Self {
            events: Vec::new(),
            overflowed: false,
        }
    }

    fn push(&mut self, event: WinEvent) {
        if self.events.len() < TASKBAR_DRAIN_CAP {
            self.events.push(event);
        } else {
            self.overflowed = true;
        }
    }

    fn take(&mut self) -> (Vec<WinEvent>, bool) {
        (
            std::mem::take(&mut self.events),
            std::mem::replace(&mut self.overflowed, false),
        )
    }
}

/// Taskbar-scope trampoline: routes pid-filtered Explorer events to the
/// taskbar queue only. Shape filtering (window-object ids) lives once with
/// the product drain; this stays a plain bounded push with the same
/// panic containment as the main trampoline, and it runs solely for
/// Explorer-delivered events, never for game or app traffic.
unsafe extern "system" fn taskbar_trampoline(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    id_object: i32,
    _id_child: i32,
    thread: u32,
    time: u32,
) {
    // No sink callback runs here, so nothing unwinds across the `extern`
    // edge; lock poison drops the event (fail closed).
    if let Ok(mut guard) = taskbar_queue().lock() {
        guard.push(WinEvent {
            event,
            hwnd: hwnd as usize as u64,
            id_object,
            thread,
            time,
        });
    }
}

/// Install one out-of-context pid-scoped hook routing to the taskbar queue;
/// `None` when the API refuses it (the caller degrades to broadcast-only
/// invalidation, as before). Scope is fixed at install: the caller
/// reinstalls on Explorer identity change.
pub fn install_taskbar_hook(event_min: u32, event_max: u32, pid: u32) -> Option<WinEventHook> {
    let raw = unsafe {
        SetWinEventHook(
            event_min,
            event_max,
            std::ptr::null_mut(),
            Some(taskbar_trampoline),
            pid,
            0,
            windows_sys::Win32::UI::WindowsAndMessaging::WINEVENT_OUTOFCONTEXT,
        )
    };
    if raw.is_null() {
        return None;
    }
    Some(WinEventHook { raw })
}

/// Take queued taskbar-scope events (consumed once by the loop drain).
/// Returns the retained batch plus whether enqueue-time overflow coalesced:
/// the caller treats overflow as change. The take always empties the store,
/// so no stale backlog survives a drain.
pub fn take_taskbar_events() -> (Vec<WinEvent>, bool) {
    taskbar_queue()
        .lock()
        .map(|mut queue| queue.take())
        .unwrap_or_default()
}

#[cfg(test)]
pub(crate) fn drive_taskbar_event(event: u32, hwnd: u64, id_object: i32, thread: u32, time: u32) {
    // Offline drive through the taskbar trampoline: no hook is installed,
    // the fabricated handle is only converted to an id, never dereferenced.
    // Populates the taskbar queue without touching the global sink, so
    // product-drain tests stay hermetic under parallel execution.
    unsafe {
        taskbar_trampoline(
            std::ptr::null_mut(),
            event,
            hwnd as usize as HWND,
            id_object,
            0,
            thread,
            time,
        );
    }
}

#[cfg(test)]
fn drive_win_event(event: u32, hwnd: u64, id_object: i32, thread: u32, time: u32) {
    // Offline drive through the real trampoline: no hook is installed, the
    // fabricated handle is only converted to an id, never dereferenced.
    unsafe {
        win_event_trampoline(
            std::ptr::null_mut(),
            event,
            hwnd as usize as HWND,
            id_object,
            0,
            thread,
            time,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct Record {
        events: Mutex<Vec<WinEvent>>,
    }

    impl WinEventSink for Record {
        fn on_win_event(&self, event: WinEvent) {
            self.events.lock().expect("record lock").push(event);
        }
    }

    struct Panic;

    impl WinEventSink for Panic {
        fn on_win_event(&self, _event: WinEvent) {
            panic!("sink boom");
        }
    }

    #[test]
    fn trampoline_forwards_values_and_contains_sink_panic() {
        let record = Arc::new(Record {
            events: Mutex::new(Vec::new()),
        });
        set_win_event_sink(record.clone());
        drive_win_event(0x8005, 0x1234, 7, 42, 99);
        let events = record.events.lock().expect("record lock");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event, 0x8005);
        assert_eq!(events[0].hwnd, 0x1234);
        assert_eq!(events[0].id_object, 7);
        assert_eq!(events[0].thread, 42);
        assert_eq!(events[0].time, 99);
        drop(events);
        // A panicking sink must not unwind across the `extern` edge: the
        // drive returns normally and the event is dropped.
        set_win_event_sink(Arc::new(Panic));
        drive_win_event(0x8005, 0x1234, 7, 42, 99);
        set_win_event_sink(record);
    }

    fn taskbar_event(hwnd: u64) -> WinEvent {
        WinEvent {
            event: 0x8000,
            hwnd,
            id_object: 0,
            thread: 11,
            time: 1,
        }
    }

    #[test]
    fn taskbar_queue_bounds_storage_and_coalesces_overflow() {
        // Local pure fixture: no globals, so parallel tests cannot race.
        // Producer-shaped burst: cap plus excess pushes.
        let mut queue = TaskbarQueue::new();
        for hwnd in 0..(TASKBAR_DRAIN_CAP as u64 + 10) {
            queue.push(taskbar_event(1000 + hwnd));
        }
        // First take returns exactly the cap plus the overflow bit.
        let (batch, overflowed) = queue.take();
        assert_eq!(batch.len(), TASKBAR_DRAIN_CAP);
        assert!(overflowed);
        assert_eq!(batch[0].hwnd, 1000);
        assert_eq!(
            batch[TASKBAR_DRAIN_CAP - 1].hwnd,
            1000 + TASKBAR_DRAIN_CAP as u64 - 1
        );
        // Second take is clean: no stale backlog survives a drain.
        let (again, overflowed) = queue.take();
        assert!(again.is_empty());
        assert!(!overflowed);
        // The store stays usable after overflow.
        queue.push(taskbar_event(2000));
        let (batch, overflowed) = queue.take();
        assert_eq!(batch.len(), 1);
        assert_eq!(batch[0].hwnd, 2000);
        assert!(!overflowed);
    }

    #[test]
    fn taskbar_queue_without_overflow_takes_clean() {
        let mut queue = TaskbarQueue::new();
        queue.push(taskbar_event(3000));
        queue.push(taskbar_event(3001));
        let (batch, overflowed) = queue.take();
        assert_eq!(batch.len(), 2);
        assert!(!overflowed);
        let (again, overflowed) = queue.take();
        assert!(again.is_empty());
        assert!(!overflowed);
    }

    #[test]
    fn taskbar_concurrent_producers_stay_isolated() {
        // Intended test contract: each test thread's injected taskbar traffic
        // is visible only to that thread's drain, so parallel pager tests
        // never steal or pollute each other's edges. All producers stage
        // before any consumer drains (barrier), which is deterministic: on a
        // single shared queue exactly one consumer would take the whole
        // burst and every assertion below would fail.
        use std::sync::{Arc, Barrier};
        const THREADS: usize = 8;
        const PER_THREAD: u64 = 5;
        let _ = super::take_taskbar_events();
        let barrier = Arc::new(Barrier::new(THREADS));
        let handles: Vec<_> = (0..THREADS)
            .map(|t| {
                let barrier = barrier.clone();
                std::thread::spawn(move || -> Result<(), String> {
                    let base = 50_000 + (t as u64) * 1_000;
                    for i in 0..PER_THREAD {
                        super::drive_taskbar_event(0x8000, base + i, 0, 11, 1);
                    }
                    barrier.wait();
                    let (batch, overflowed) = super::take_taskbar_events();
                    if overflowed {
                        return Err(format!("thread {t}: unexpected overflow"));
                    }
                    if batch.len() as u64 != PER_THREAD {
                        return Err(format!(
                            "thread {t}: took {} of {PER_THREAD} staged events",
                            batch.len()
                        ));
                    }
                    for (i, event) in batch.iter().enumerate() {
                        let want = super::WinEvent {
                            event: 0x8000,
                            hwnd: base + i as u64,
                            id_object: 0,
                            thread: 11,
                            time: 1,
                        };
                        if event.event != want.event
                            || event.hwnd != want.hwnd
                            || event.id_object != want.id_object
                            || event.thread != want.thread
                            || event.time != want.time
                        {
                            return Err(format!("thread {t}: position {i} carries {event:?}"));
                        }
                    }
                    Ok(())
                })
            })
            .collect();
        for (t, handle) in handles.into_iter().enumerate() {
            let outcome = handle.join().expect("worker thread must join");
            assert!(outcome.is_ok(), "worker {t}: {}", outcome.unwrap_err());
        }
    }

    #[test]
    fn taskbar_callback_route_preserves_fields_and_cap() {
        // The isolated queue still runs the production callback route
        // (trampoline push, bounded store, take oracle): every field
        // survives, overflow coalesces at the cap, and the take stays clean.
        let _ = super::take_taskbar_events();
        for i in 0..(super::TASKBAR_DRAIN_CAP as u64 + 10) {
            super::drive_taskbar_event(0x8005, 7000 + i, 0, 42, 7);
        }
        let (batch, overflowed) = super::take_taskbar_events();
        assert_eq!(batch.len(), super::TASKBAR_DRAIN_CAP);
        assert!(overflowed);
        assert_eq!(batch[0].hwnd, 7000);
        assert_eq!(
            batch[super::TASKBAR_DRAIN_CAP - 1].hwnd,
            7000 + super::TASKBAR_DRAIN_CAP as u64 - 1
        );
        for event in &batch {
            assert_eq!(event.event, 0x8005);
            assert_eq!(event.id_object, 0);
            assert_eq!(event.thread, 42);
            assert_eq!(event.time, 7);
        }
        let (again, overflowed) = super::take_taskbar_events();
        assert!(again.is_empty());
        assert!(!overflowed);
    }
}

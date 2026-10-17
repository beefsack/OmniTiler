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
    let raw = unsafe {
        SetWinEventHook(
            event_min,
            event_max,
            std::ptr::null_mut(),
            Some(win_event_trampoline),
            0,
            0,
            windows_sys::Win32::UI::WindowsAndMessaging::WINEVENT_OUTOFCONTEXT,
        )
    };
    if raw.is_null() {
        return None;
    }
    Some(WinEventHook { raw })
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
}

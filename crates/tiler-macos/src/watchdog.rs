#![forbid(unsafe_code)]

//! Cancellable hard-lifetime watchdog shared by bounded native runs.
//!
//! Armed for a finite `limit`, forces a dirty process exit if the guarded
//! work has not disarmed by then. The waiter sleeps in short slices so
//! disarm lands promptly; disarm joins bounded by one slice. Fires
//! independently of the guarded thread (a stalled main still exits).
//! `Drop` disarms as a backstop so early errors never leave a delayed
//! forced exit behind. Factored out of the tile controller so
//! read-only diagnostics reuse the same implementation instead of
//! duplicating it. No AX, no UI, no settings writes.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

pub struct Watchdog {
    cancel: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Drop for Watchdog {
    fn drop(&mut self) {
        self.disarm();
    }
}

impl Watchdog {
    pub fn arm(limit: Duration) -> Result<Self, String> {
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&cancel);
        let thread = std::thread::Builder::new()
            .name("hard-lifetime-watchdog".to_owned())
            .spawn(move || {
                let deadline = Instant::now() + limit;
                while Instant::now() < deadline {
                    if flag.load(Ordering::SeqCst) {
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
                if !flag.load(Ordering::SeqCst) {
                    eprintln!("hard lifetime exceeded; forcing dirty exit (windows left in place)");
                    std::process::exit(1);
                }
            })
            .map_err(|e| format!("watchdog spawn: {e}"))?;
        Ok(Self {
            cancel,
            thread: Some(thread),
        })
    }

    pub fn disarm(&mut self) {
        self.cancel.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disarm_cancels_before_the_limit() {
        let mut watchdog = Watchdog::arm(Duration::from_secs(60)).expect("arm");
        watchdog.disarm();
    }

    #[test]
    fn drop_disarms_without_explicit_call() {
        {
            let _watchdog = Watchdog::arm(Duration::from_secs(60)).expect("arm");
        }
    }
}

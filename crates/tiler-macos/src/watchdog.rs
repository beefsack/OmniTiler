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

use std::path::PathBuf;
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
        Self::start(limit, None, dirty_exit)
    }

    /// Independently observe a stop request, then bound controller shutdown.
    /// Unlike an overall lifetime, this leaves an idle long-running owner alone.
    /// The observing thread does not depend on the main loop servicing stop.
    pub fn arm_on_stop(stop_file: PathBuf, limit: Duration) -> Result<Self, String> {
        Self::start(limit, Some(stop_file), dirty_exit)
    }

    fn start(
        limit: Duration,
        stop_file: Option<PathBuf>,
        on_expiry: impl FnOnce() + Send + 'static,
    ) -> Result<Self, String> {
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&cancel);
        let thread = std::thread::Builder::new()
            .name("hard-lifetime-watchdog".to_owned())
            .spawn(move || {
                let mut deadline = stop_file.is_none().then(|| Instant::now() + limit);
                loop {
                    if flag.load(Ordering::SeqCst) {
                        return;
                    }
                    if deadline.is_none() && stop_file.as_ref().is_some_and(|file| file.exists()) {
                        deadline = Some(Instant::now() + limit);
                    }
                    if deadline.is_some_and(|end| Instant::now() >= end) {
                        if !flag.load(Ordering::SeqCst) {
                            on_expiry();
                        }
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(100));
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

fn dirty_exit() {
    eprintln!(
        "hard lifetime or stop deadline exceeded; forcing dirty exit (windows left in place)"
    );
    std::process::exit(1);
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

    #[test]
    fn stop_watcher_expires_without_controller_pumping() {
        // Test-only temporary identity: portable uniqueness from time, PID,
        // and an atomic counter. This is not a live process nonce, so it
        // must not use the product CSPRNG (`mint_nonce` fails closed where
        // `/dev/urandom` is absent, e.g. Windows).
        static TMP_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let unique = format!(
            "{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0),
            TMP_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
        );
        let dir = std::env::temp_dir().join(format!("omnitiler-watchdog-{unique}"));
        std::fs::create_dir(&dir).expect("temporary directory");
        let stop = dir.join("stop-requested");
        let (tx, rx) = std::sync::mpsc::channel();
        let mut watcher =
            Watchdog::start(Duration::from_millis(20), Some(stop.clone()), move || {
                let _ = tx.send(());
            })
            .expect("watcher");
        assert!(rx.recv_timeout(Duration::from_millis(150)).is_err());
        std::fs::write(&stop, []).expect("stop request");
        // No controller turn drives this: the independent owner observes stop.
        rx.recv_timeout(Duration::from_secs(2))
            .expect("stop deadline");
        watcher.disarm();
        std::fs::remove_file(stop).expect("remove stop");
        std::fs::remove_dir(dir).expect("remove directory");
    }
}

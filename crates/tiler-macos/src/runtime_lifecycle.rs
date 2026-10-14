#![forbid(unsafe_code)]

//! Phase 1 thread lifecycle accounting (offline, portable).
//!
//! Exit types plus join helpers shared by the runtime supervisor and its
//! owners. The rules live here so both `runtime` (supervision) and
//! `runtime_thread` (owners) stay small:
//!
//! - [`WorkerExit::Unclean`] is returned teardown evidence (e.g. backend
//!   `shutdown()` refused), distinct from a panic (`join` error). Both are
//!   dirty, never clean, and both persist in diagnostics so a later reaped
//!   worker still poisons the terminal shutdown.
//! - [`join_exited`] joins only threads proven exited and never blocks: a
//!   still-running handle is retained for [`RuntimeHandle::reap`].
//! - [`classify_shutdown`] folds panic, timeout-retained, unclean-teardown,
//!   and unexpected-EOS evidence into one honest outcome.

/// Per-app worker exit. `Unclean` carries a static reason only — never
/// backend strings or user content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WorkerExit {
    Clean,
    Unclean { reason: &'static str },
}

/// Engine owner exit.
pub(crate) enum EngineExit {
    Done,
}

/// Input owner exit.
pub(crate) enum InputExit {
    Done,
}

/// Logger owner exit.
pub(crate) enum LoggerExit {
    Done,
}

pub(crate) enum LiveHandle {
    Engine(std::thread::JoinHandle<EngineExit>),
    Input(std::thread::JoinHandle<InputExit>),
    Logger(std::thread::JoinHandle<LoggerExit>),
    Worker {
        pid: u32,
        generation: u64,
        handle: std::thread::JoinHandle<WorkerExit>,
    },
}

impl LiveHandle {
    pub(crate) fn is_finished(&self) -> bool {
        match self {
            Self::Engine(h) => h.is_finished(),
            Self::Input(h) => h.is_finished(),
            Self::Logger(h) => h.is_finished(),
            Self::Worker { handle, .. } => handle.is_finished(),
        }
    }

    /// Join after proving exit. Returns `(panicked, unclean)`: a returned
    /// `Unclean` is counted separately from a panic.
    fn join_finished(self) -> (bool, bool) {
        match self {
            Self::Engine(h) => (h.join().is_err(), false),
            Self::Input(h) => (h.join().is_err(), false),
            Self::Logger(h) => (h.join().is_err(), false),
            Self::Worker { handle, .. } => match handle.join() {
                Err(_) => (true, false),
                Ok(WorkerExit::Clean) => (false, false),
                Ok(WorkerExit::Unclean { .. }) => (false, true),
            },
        }
    }
}

/// Join only proven-exited handles. Call WITHOUT holding the shared lock
/// (owners take that lock for diagnostics): joining under it risks deadlock
/// against a thread blocked on the same lock. Returns
/// `(joined, panics, unclean, retained)`.
pub(crate) fn join_exited(handles: Vec<LiveHandle>) -> (usize, usize, usize, Vec<LiveHandle>) {
    let mut joined = 0usize;
    let mut panics = 0usize;
    let mut unclean = 0usize;
    let mut retained = Vec::new();
    for handle in handles {
        if handle.is_finished() {
            let (panicked, was_unclean) = handle.join_finished();
            if panicked {
                panics += 1;
            }
            if was_unclean {
                unclean += 1;
            }
            joined += 1;
        } else {
            retained.push(handle);
        }
    }
    (joined, panics, unclean, retained)
}

/// Shutdown classification. A panic, a missed deadline (retained handles),
/// a returned unclean teardown, or an unexpected EOS is never clean.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShutdownOutcome {
    Clean,
    DirtyTimeout,
    Unclean,
    Panic,
    AlreadyStopped,
}

/// Shutdown report under the single global deadline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShutdownReport {
    pub outcome: ShutdownOutcome,
    pub elapsed_ms: u64,
    pub joined: usize,
    pub retained: usize,
    pub panics: usize,
    pub unclean: usize,
}

/// Later reap report for handles retained after a dirty shutdown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReapReport {
    pub reaped: usize,
    pub panics: usize,
    pub unclean: usize,
    pub remaining: usize,
}

/// Fold persistent plus just-joined evidence into one honest outcome.
/// Priority: panic first, then missed deadline, then unclean/EOS.
#[must_use]
pub const fn classify_shutdown(
    panics_observed: u64,
    retained: usize,
    unclean_observed: u64,
    unexpected_eos: u64,
) -> ShutdownOutcome {
    if panics_observed > 0 {
        ShutdownOutcome::Panic
    } else if retained > 0 {
        ShutdownOutcome::DirtyTimeout
    } else if unclean_observed > 0 || unexpected_eos > 0 {
        ShutdownOutcome::Unclean
    } else {
        ShutdownOutcome::Clean
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_prioritises_panic_then_timeout_then_unclean() {
        assert_eq!(
            classify_shutdown(1, 1, 1, 1),
            ShutdownOutcome::Panic,
            "panic dominates"
        );
        assert_eq!(
            classify_shutdown(0, 2, 1, 1),
            ShutdownOutcome::DirtyTimeout,
            "retained handles are a missed deadline"
        );
        assert_eq!(
            classify_shutdown(0, 0, 1, 0),
            ShutdownOutcome::Unclean,
            "teardown evidence is unclean"
        );
        assert_eq!(
            classify_shutdown(0, 0, 0, 3),
            ShutdownOutcome::Unclean,
            "unexpected EOS is unclean, never clean"
        );
        assert_eq!(classify_shutdown(0, 0, 0, 0), ShutdownOutcome::Clean);
    }

    #[test]
    fn join_counts_unclean_distinct_from_panic() {
        let clean = std::thread::Builder::new()
            .spawn(|| WorkerExit::Clean)
            .expect("spawn");
        let unclean = std::thread::Builder::new()
            .spawn(|| WorkerExit::Unclean {
                reason: "teardown-failed",
            })
            .expect("spawn");
        // Proven exits: join immediately.
        while !clean.is_finished() || !unclean.is_finished() {
            std::thread::yield_now();
        }
        let (joined, panics, unclean_count, retained) = join_exited(vec![
            LiveHandle::Worker {
                pid: 1,
                generation: 1,
                handle: clean,
            },
            LiveHandle::Worker {
                pid: 2,
                generation: 1,
                handle: unclean,
            },
        ]);
        assert_eq!((joined, panics, unclean_count), (2, 0, 1));
        assert!(retained.is_empty());
    }
}

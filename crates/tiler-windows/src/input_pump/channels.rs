#![forbid(unsafe_code)]
use super::*;

/// Shared input channels owned by the owner handle and the pump thread.
///
/// Esc clocks and edges are process-global singletons in
/// [`crate::input_handoff`] (callbacks bump them, WinEvent START/END and the
/// owner settle sample them), so handles can never mismatch clocks across
/// restarts. Generations pair exactly one owner handle with its pump thread.
#[derive(Debug)]
pub struct InputChannels {
    /// Owner -> input FIFO.
    pub inbox: Arc<Inbox>,
    /// Input -> owner snap batches, tagged with the applied policy
    /// generation at drain time.
    pub snap_outbox: Arc<Outbox<TaggedBatch<QueuedSnapEvent>>>,
    /// Input -> owner trace callback summaries (closed vocabulary only).
    pub cb_outbox: Arc<Outbox<Vec<crate::snapkey::CallbackDiag>>>,
    /// Bounded diagnostics published by the pump.
    pub diag: Arc<DiagSlot>,
    /// Latest policy generation the pump applied, in FIFO order. The owner
    /// drops batches tagged older than its published policy as stale.
    pub applied_policy_gen: Arc<std::sync::atomic::AtomicU64>,
    /// Stale queued records dropped by the latest policy-changing apply
    /// (real classifier queue; outbox/pending records are dropped
    /// owner-side by generation). Read with [`Self::applied_policy_gen`].
    pub acked_stale: Arc<std::sync::atomic::AtomicU32>,
    /// Stop requested by the owner; pump unhooks on its own thread and acks.
    pub stop_flag: Arc<AtomicBool>,
    /// Owner-intent wake: auto-reset kernel event signaled by the pump after
    /// each accepted keyboard batch (fresh or pending-retry). Stays
    /// pending between offer and the owner outer wait; coalesces bursts.
    /// `None` only when creation failed (spawn refuses cleanly).
    #[cfg(windows)]
    pub intent_wake: Option<OwnedHandle>,
    /// Owner-to-input work wake: auto-reset kernel event signaled by the
    /// owner after every accepted inbox publish/control, retry flush with
    /// payload, outbox drain, and stop request. Breaks the pump wait early
    /// so no fixed idle quantum is needed; coalesces bursts. `None` only
    /// when creation failed (spawn refuses cleanly).
    #[cfg(windows)]
    pub work_wake: Option<OwnedHandle>,
    /// Stalled-retry flag (owner sets, pump consumes): true while the owner
    /// holds a refused snapshot retry or outstanding sender payloads. The
    /// pump signals the intent wake exactly once per successful inbox drain
    /// while set, so the retry re-offers promptly instead of waiting for an
    /// unrelated owner wake. No feedback loop: the flag clears on signal
    /// and re-arms only if the retry turn still finds no room.
    pub inbox_stalled: Arc<AtomicBool>,
}

/// Create the owner-intent wake: auto-reset, initially non-signaled.
#[cfg(windows)]
fn create_intent_event() -> Result<OwnedHandle, String> {
    crate::ffi::create_auto_reset_event()
        .ok_or_else(|| "error: intent wake creation failed".to_owned())
        .map_err(|message| {
            // Preserve the previous `win32-{code}` vocabulary via the
            // thread's last error, read here at creation-failure time.
            let code = crate::ffi::process::last_error();
            format!("{message}: win32-{code}")
        })
}

impl InputChannels {
    /// New channels with fresh clocks and empty queues.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inbox: Arc::new(Inbox::new()),
            snap_outbox: Arc::new(Outbox::new()),
            cb_outbox: Arc::new(Outbox::new()),
            diag: Arc::new(DiagSlot::new()),
            applied_policy_gen: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            acked_stale: Arc::new(std::sync::atomic::AtomicU32::new(0)),
            stop_flag: Arc::new(AtomicBool::new(false)),
            inbox_stalled: Arc::new(AtomicBool::new(false)),
            #[cfg(windows)]
            intent_wake: create_intent_event().ok(),
            #[cfg(windows)]
            work_wake: create_intent_event().ok(),
        }
    }
}

#[cfg(windows)]
impl InputChannels {
    /// Borrow the owner-intent wake, if creation succeeded.
    pub fn intent_wake(&self) -> Option<&OwnedHandle> {
        self.intent_wake.as_ref()
    }

    /// Signal the owner-intent wake (best-effort, never waits, never logs).
    /// No-op when creation failed; the owner keeps its periodic fallback.
    pub fn notify_intent(&self) {
        if let Some(handle) = self.intent_wake.as_ref() {
            crate::ffi::signal_event(handle);
        }
    }

    /// Borrow the owner-to-input work wake, if creation succeeded.
    pub fn work_wake(&self) -> Option<&OwnedHandle> {
        self.work_wake.as_ref()
    }

    /// Signal the owner-to-input work wake (best-effort, never waits, never
    /// logs). No-op when creation failed; the pump falls back to its retry
    /// deadlines and message-queue wake.
    pub fn notify_work(&self) {
        if let Some(handle) = self.work_wake.as_ref() {
            crate::ffi::signal_event(handle);
        }
    }
}

impl Default for InputChannels {
    fn default() -> Self {
        Self::new()
    }
}

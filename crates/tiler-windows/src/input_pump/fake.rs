use super::*;

/// Portable scripted backend for deterministic install/retry/stop tests. The
/// production pump step drives this through the `NativeHooks` seam under
/// `cfg(windows)`; the pure backoff/queue/ordering vocabulary stays portable
/// here. No threads, no native calls.
#[derive(Debug, Default)]
pub struct FakeBackend {
    /// Scripted keyboard install results (`true` = success), consumed in order.
    pub keyboard_script: VecDeque<bool>,
    /// Scripted mouse install results, consumed in order.
    pub mouse_script: VecDeque<bool>,
    /// Scripted unhook results, consumed in order (defaults to success).
    pub unhook_script: VecDeque<bool>,
    /// Keyboard install attempts so far.
    pub keyboard_attempts: u32,
    /// Mouse install attempts so far.
    pub mouse_attempts: u32,
    /// Message drains requested (two per completed pump step).
    pub pump_turns: u32,
    /// Message-pump timeout per turn in milliseconds (deadline evidence:
    /// install-retry deadline while hooks are down, `u32::MAX` while idle).
    pub pump_timeouts: Vec<u32>,
    /// Scripted classifier items enqueued during each message drain,
    /// mimicking LL callbacks that ran on the pump thread (test-only: no
    /// native effects). Drained by the pre-wait transfer on the same turn.
    pub snap_on_drain: Vec<QueuedSnapEvent>,
    /// Scripted windrag edges enqueued during each message drain, like
    /// [`Self::snap_on_drain`].
    pub windrag_on_drain: Vec<WinDragEdge>,
    /// When true (test-only), the wait asserts no classifier work remains
    /// queued: pre-wait callback work must transfer
    /// before blocking.
    pub assert_transferred_before_wait: bool,
    /// Keyboard config seen at each install attempt (stale-init evidence).
    pub keyboard_configs: Vec<KeyboardConfig>,
    /// Retry backoffs recorded (evidence).
    pub retry_secs: Vec<u64>,
    /// Scripted native wait reasons, consumed in order (defaults to timeout).
    pub wait_script: VecDeque<InputWaitReason>,
}

impl FakeBackend {
    /// New backend with scripted install outcomes (unhook succeeds).
    #[must_use]
    pub fn new(keyboard_script: Vec<bool>, mouse_script: Vec<bool>, unhook_ok: bool) -> Self {
        let mut unhook_script = VecDeque::new();
        if !unhook_ok {
            unhook_script.push_back(false);
        }
        Self {
            keyboard_script: keyboard_script.into(),
            mouse_script: mouse_script.into(),
            unhook_script,
            keyboard_attempts: 0,
            mouse_attempts: 0,
            pump_turns: 0,
            pump_timeouts: Vec::new(),
            snap_on_drain: Vec::new(),
            windrag_on_drain: Vec::new(),
            assert_transferred_before_wait: false,
            keyboard_configs: Vec::new(),
            retry_secs: Vec::new(),
            wait_script: VecDeque::new(),
        }
    }

    /// Record a backoff wait. Returns the waited seconds.
    pub fn note_retry(&mut self, failures: u32) -> u64 {
        let secs = install_backoff_secs(failures);
        self.retry_secs.push(secs);
        secs
    }
}

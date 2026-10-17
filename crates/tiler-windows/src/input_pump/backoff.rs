#![forbid(unsafe_code)]
use super::*;

/// Bounded install-retry backoff in seconds: 5s doubling, 60s cap. Mirrors
/// the existing owner keyboard retry so recovery timing does not change.
#[must_use]
pub fn install_backoff_secs(failures: u32) -> u64 {
    (5u64 << failures.min(4)).min(60)
}

/// How long the owner waits for input readiness / stop acknowledgement
/// before reporting failure without claiming success.
pub const READY_WAIT: Duration = Duration::from_secs(2);
/// Bounded stop acknowledgement wait: a missing ack returns failure and
/// retains `Stopping` (never claims clean, never starts an overlapping
/// worker).
pub const STOP_WAIT: Duration = Duration::from_secs(5);
/// Bounded transient-contention retry: one-shot wait while a try-lock
/// exchange reported WouldBlock (contended, possibly free). Lock holds are
/// sub-millisecond by construction (try-locks only, never held across
/// waits), so this fires at most once per contention sighting; steady state
/// returns to deadline/indefinite waits. Never a fixed idle poll.
pub const CONTENTION_RETRY: Duration = Duration::from_millis(5);

/// Native pump wait return reason: exactly one per wait. Counts native
/// `MsgWaitForMultipleObjectsEx` returns only, never thread context switches.
/// `Timeout` is the genuine deadline expiry (`WAIT_TIMEOUT`, 258) only;
/// `Failed` is `WAIT_FAILED` or any unrecognized code, counted separately and
/// never merged into timeout. Classification is telemetry only: the loop keeps
/// its existing fallthrough to periodic work either way, with no new recovery
/// and no timing claim (a failed wait returns immediately; not proven delayed).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputWaitReason {
    Work,
    Message,
    Timeout,
    Failed,
}

/// Classify one native input-wait return code. Pure (no native calls) so
/// tests prove the work/message/timeout/failed split without a desktop.
#[cfg(windows)]
#[must_use]
pub(crate) fn classify_input_wait(result: u32, has_work: bool) -> InputWaitReason {
    const WAIT_OBJECT_0: u32 = 0;
    const WAIT_TIMEOUT: u32 = 258;
    if result == WAIT_TIMEOUT {
        return InputWaitReason::Timeout;
    }
    if has_work {
        if result == WAIT_OBJECT_0 {
            return InputWaitReason::Work;
        }
        if result == WAIT_OBJECT_0 + 1 {
            return InputWaitReason::Message;
        }
    } else if result == WAIT_OBJECT_0 {
        return InputWaitReason::Message;
    }
    InputWaitReason::Failed
}

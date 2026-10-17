#![forbid(unsafe_code)]
use super::pump_wait_with_intent;

fn wake_signaled(handle: &std::os::windows::io::OwnedHandle) -> bool {
    crate::ffi::event_signaled(handle)
}

#[test]
fn wait_consumes_pending_wake_and_keeps_fallback() {
    // Race-safe: a signal offered before the wait stays pending and the
    // wait consumes it; an unsignaled wait and the degraded `None` path
    // both return via the retained periodic fallback (zero timeout here
    // keeps the test deterministic with no desktop effects).
    let channels = crate::input_pump::InputChannels::new();
    let wake = channels.intent_wake().expect("wake created");
    channels.notify_intent();
    let woke = pump_wait_with_intent(0, Some(wake), None, &[]);
    assert!(woke.input && woke.full_turn());
    assert!(!wake_signaled(wake), "pending wake is consumed by the wait");
    let woke = pump_wait_with_intent(0, Some(wake), None, &[]);
    // No signal and no pump traffic: genuine timeout (which is a full
    // turn by design, preserving the timed cadence), an ambient message,
    // or the pre-existing failed-wait fallback (also a full turn).
    assert!(woke.timeout || woke.message || woke.failed);
    assert!(!wake_signaled(wake));
    let _ = pump_wait_with_intent(0, None, None, &[]);
}

#[test]
fn wait_reports_ack_only_without_input_or_message() {
    // The feedback-loop guard: an ack-only wake reports exactly that,
    // so the loop drains acks and services the pending finish without
    // reposting, reconciling, or settings/input work. A bare wait with
    // nothing signaled normally reports timeout (the periodic full turn).
    use super::WaitWake;
    use std::os::windows::io::OwnedHandle;
    fn event() -> OwnedHandle {
        crate::ffi::create_auto_reset_event().expect("event created")
    }
    fn signal(handle: &OwnedHandle) {
        crate::ffi::signal_event(handle);
    }
    let intent = event();
    let ack = event();
    signal(&ack);
    let woke = pump_wait_with_intent(0, Some(&intent), Some(&ack), &[]);
    assert!(!wake_signaled(&ack), "outer wait consumes the ack");
    assert!(!wake_signaled(&intent), "peer wake stays unsignaled");
    // Zero timeout with no signal and no pump traffic reports timeout:
    // either a pure timeout or a timeout plus ambient messages (a busy
    // desktop may deliver broadcasts through the pump drains).
    let idle = pump_wait_with_intent(0, Some(&intent), Some(&ack), &[]);
    assert!(idle.timeout || idle.message || idle.failed);
    assert!(!idle.input && !idle.ack);
    // Reason bits compose honestly: ack-only is never a full turn, while
    // the pre-existing failed-wait fallback still takes the full path.
    let ack_only = WaitWake {
        input: false,
        ack: true,
        message: false,
        notify: false,
        timeout: false,
        failed: false,
    };
    assert!(!ack_only.full_turn());
    assert!(woke.ack);
    assert!(
        WaitWake {
            input: true,
            ack: false,
            message: false,
            notify: false,
            timeout: false,
            failed: false,
        }
        .full_turn()
    );
    assert!(
        WaitWake {
            input: false,
            ack: false,
            message: false,
            notify: false,
            timeout: false,
            failed: true,
        }
        .full_turn(),
        "failed keeps the existing fallback path; only the counter differs"
    );
    // An extra notification handle alone is a full turn (its file or
    // result still needs the poll/drain), and it never claims to be
    // input, an ack, or a message.
    assert!(
        WaitWake {
            input: false,
            ack: false,
            message: false,
            notify: true,
            timeout: false,
            failed: false,
        }
        .full_turn()
    );
}

#[test]
fn wait_wakes_on_either_intent_or_ack_without_consuming_peer() {
    // Two wakes share the outer wait: each signal returns immediately,
    // and waiting observes only the signaled one (auto-reset events
    // stay pending until a wait actually observes them).
    use std::os::windows::io::OwnedHandle;
    fn event() -> OwnedHandle {
        crate::ffi::create_auto_reset_event().expect("event created")
    }
    fn signal(handle: &OwnedHandle) {
        crate::ffi::signal_event(handle);
    }
    let intent = event();
    let ack = event();
    signal(&ack);
    pump_wait_with_intent(0, Some(&intent), Some(&ack), &[]);
    assert!(!wake_signaled(&ack), "outer wait consumes the ack");
    assert!(!wake_signaled(&intent), "peer wake stays unsignaled");
    signal(&intent);
    pump_wait_with_intent(0, Some(&intent), Some(&ack), &[]);
    assert!(!wake_signaled(&intent), "outer wait consumes the intent");
    assert!(!wake_signaled(&ack), "peer wake stays unsignaled");
    pump_wait_with_intent(0, Some(&intent), Some(&ack), &[]);
}

#[test]
fn work_wake_created_nonsignaled_and_signals() {
    // Owner-to-input direction: fresh channels carry a non-signaled
    // auto-reset work wake; one signal pends exactly once (a publish
    // before the pump wait is never lost, bursts coalesce).
    let channels = crate::input_pump::InputChannels::new();
    let wake = channels.work_wake().expect("work wake created");
    assert!(!wake_signaled(wake), "no spurious pump wake");
    channels.notify_work();
    assert!(wake_signaled(wake), "owner publish wakes the pump");
    assert!(!wake_signaled(wake), "auto-reset consumes the wake");
}

#[test]
fn outer_wait_counts_split_reasons_with_ack_only_subset() {
    // Trace discriminator shape: combined ack-plus-message reasons each
    // count (hits can exceed waits), genuine timeout stays separate from
    // failed, while the ack-only early return is a counted subset that
    // never claims a full turn.
    use super::{OwnerWaitCounts, WaitWake};
    let mut counts = OwnerWaitCounts::default();
    counts.note_wait(WaitWake {
        input: false,
        ack: true,
        message: true,
        notify: true,
        timeout: false,
        failed: false,
    });
    counts.note_ack_only();
    counts.note_wait(WaitWake {
        input: false,
        ack: false,
        message: false,
        notify: false,
        timeout: true,
        failed: false,
    });
    counts.note_wait(WaitWake {
        input: false,
        ack: false,
        message: false,
        notify: false,
        timeout: false,
        failed: true,
    });
    assert_eq!(counts.waits, 3);
    assert_eq!(counts.ack_only, 1);
    assert_eq!((counts.ack, counts.msg, counts.timeout), (1, 1, 1));
    assert_eq!(counts.notify, 1);
    assert_eq!(counts.failed, 1);
    assert_eq!(counts.input, 0);
}

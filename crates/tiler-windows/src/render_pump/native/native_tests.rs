#![forbid(unsafe_code)]
use super::*;

fn signaled(handle: &OwnedHandle) -> bool {
    crate::ffi::event_signaled(handle)
}

fn bare_handle() -> RenderHandle {
    let (fail_tx, fail_rx) = sync_channel(8);
    let mut lifecycle = RenderLifecycle::new();
    lifecycle.start().expect("fresh handle starts");
    RenderHandle {
        channels: Arc::new(RenderChannels::new()),
        signals: Arc::new(RenderSignals::new(fail_tx)),
        failures: fail_rx,
        lifecycle,
        thread: None,
        stop_report: None,
        ack_wake: create_event().expect("ack wake"),
        work_wake: create_event().expect("work wake"),
    }
}

#[test]
fn request_stop_signals_work_wake_without_owner_notify() {
    // Bare stop event: `request_stop` alone (no explicit owner
    // `notify_work`) leaves the work wake pending, so a fully idle
    // renderer wait observes the stop promptly.
    let mut handle = bare_handle();
    assert!(!signaled(&handle.work_wake), "no spurious renderer wake");
    handle.request_stop().expect("stop requested");
    assert!(
        signaled(&handle.work_wake),
        "bare request_stop wakes the renderer"
    );
    assert!(!signaled(&handle.work_wake), "auto-reset consumes the wake");
}

#[test]
fn wait_reason_splits_work_message_and_retry_timeout() {
    // Native return-code split (no desktop): owner work, queued
    // broadcast messages, and the genuine hide-retry deadline (258)
    // map distinctly; `WAIT_FAILED` and any unrecognized code count
    // as `Failed`, never merged into timeout.
    assert_eq!(classify_render_wait(0), RenderWait::Work);
    assert_eq!(classify_render_wait(1), RenderWait::Message);
    assert_eq!(classify_render_wait(258), RenderWait::Timeout);
    assert_eq!(classify_render_wait(0xFFFF_FFFF), RenderWait::Failed);
    assert_eq!(classify_render_wait(7), RenderWait::Failed);
}

#[test]
fn wait_counts_sum_with_acks_tracked_separately() {
    // Wake-evidence shape: wait returns sum by reason while ack posts
    // stay on `ack_seq`, so a trace diff distinguishes a blocking
    // wait from ack traffic (the Stage3 idle question). Failures stay
    // separate from genuine timeouts.
    use crate::render_handoff::{CarrierAck, RenderCarrier, RenderOutcome};
    let channels = RenderChannels::new();
    assert_eq!(channels.wait_counts(), (0, 0, 0, 0, 0));
    assert_eq!(channels.ack_seq(), 0);
    channels.note_wait_work();
    channels.note_wait_msg();
    channels.note_wait_timeout();
    channels.note_wait_timeout();
    channels.note_wait_failed();
    assert_eq!(channels.wait_counts(), (5, 1, 1, 2, 1));
    channels.post_ack(
        RenderCarrier::Border,
        CarrierAck {
            generation: 1,
            safety: 1,
            outcome: RenderOutcome::Hidden,
            checksum: 0,
            span_us: 0,
            queue_age_us: 0,
            last_error: None,
            hidden_proven: true,
            identity_proven: true,
            applied: None,
        },
    );
    assert_eq!(channels.ack_seq(), 1);
    // An ack never moves the wait counters.
    assert_eq!(channels.wait_counts(), (5, 1, 1, 2, 1));
}

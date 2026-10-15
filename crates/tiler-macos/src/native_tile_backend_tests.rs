#![cfg(test)]
//! Mechanical split of `native_tile_backend.rs` cfg tests (no behavior change).
use crate::native_tile_backend::*;

const INTERVAL: u64 = 1_000;

#[test]
fn backstop_covers_missed_notifications_after_one_interval() {
    // Silence is the minimize/close defect: zero notifications after
    // the last read must still reread once the interval elapses.
    assert!(!scoped_backstop_due(
        false,
        false,
        0,
        Some(0),
        None,
        INTERVAL
    ));
    assert!(!scoped_backstop_due(
        false,
        false,
        999,
        Some(0),
        None,
        INTERVAL
    ));
    assert!(scoped_backstop_due(
        false,
        false,
        1_000,
        Some(0),
        None,
        INTERVAL
    ));
    assert!(scoped_backstop_due(
        false,
        false,
        60_000,
        Some(0),
        None,
        INTERVAL
    ));
    // Never read at all: due immediately (a read is always safe).
    assert!(scoped_backstop_due(false, false, 0, None, None, INTERVAL));
}

#[test]
fn backstop_notification_rereads_immediately_and_stop_withholds() {
    // A delivered notification rereads at once, even right after a read.
    assert!(scoped_backstop_due(
        false,
        true,
        10,
        Some(10),
        None,
        INTERVAL
    ));
    // Stop withholds everything, even a due backstop or notification.
    assert!(!scoped_backstop_due(
        true,
        false,
        60_000,
        Some(0),
        None,
        INTERVAL
    ));
    assert!(!scoped_backstop_due(
        true,
        true,
        60_000,
        Some(0),
        None,
        INTERVAL
    ));
    assert!(!scoped_backstop_due(true, false, 0, None, None, INTERVAL));
}

#[test]
fn backstop_failure_backs_off_at_the_same_cadence() {
    // A failed reread at t=1000 never retries fast: silence inside the
    // next interval stays quiet, then the backstop refires.
    assert!(!scoped_backstop_due(
        false,
        false,
        1_500,
        Some(0),
        Some(1_000),
        INTERVAL
    ));
    assert!(!scoped_backstop_due(
        false,
        false,
        1_999,
        Some(0),
        Some(1_000),
        INTERVAL
    ));
    assert!(scoped_backstop_due(
        false,
        false,
        2_000,
        Some(0),
        Some(1_000),
        INTERVAL
    ));
    // A notification still rereads immediately despite backoff (fresh
    // truth may have arrived; the failure was about the last read).
    assert!(scoped_backstop_due(
        false,
        true,
        1_500,
        Some(0),
        Some(1_000),
        INTERVAL
    ));
}

#[test]
fn window_subscription_set_carries_the_minimize_pair() {
    // The tile-path consumer of the window-level set: minimize and
    // deminimize must ride alongside move/resize/destroyed, or the
    // minimize-reflow leg can never observe. Membership is
    // order-independent; emptiness or a missing pair fails.
    for name in [
        "AXMoved",
        "AXResized",
        "AXUIElementDestroyed",
        "AXWindowMiniaturized",
        "AXWindowDeminiaturized",
    ] {
        assert!(
            crate::ffi::AX_SESSION_WINDOW_NOTIFICATIONS.contains(&name),
            "{name} must be window-observed for the tile backstop to see it"
        );
    }
    assert!(
        crate::ffi::AX_SESSION_APP_NOTIFICATIONS.contains(&"AXMainWindowChanged"),
        "AXMainWindowChanged must be app-observed"
    );
}

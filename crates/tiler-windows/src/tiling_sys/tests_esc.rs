#![forbid(unsafe_code)]
use super::esc_edge_cancels;

#[test]
fn end_bound_sequence_orders_cancel() {
    // Latest physical Esc AFTER the END (before the owner drain) must
    // not cancel the completed drop: the END snapshot still equals
    // START even though the live counter moved on.
    assert!(!esc_edge_cancels(5, 5));
    // Esc on/before the END cancels: the END snapshot is newer.
    assert!(esc_edge_cancels(5, 6));
    // No edge at all never cancels.
    assert!(!esc_edge_cancels(0, 0));
    assert!(!esc_edge_cancels(u64::MAX, u64::MAX));
    // Multiple edges still cancel.
    assert!(esc_edge_cancels(7, 9));
}

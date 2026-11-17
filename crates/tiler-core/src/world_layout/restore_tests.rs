use super::*;

#[test]
fn comparable_inserted_shares_keeps_skewed_fraction_on_small_current() {
    // Old B at 1/10 against normalized-small survivors: the current
    // weights rescale up (entrant 2, survivors 9 and 9) instead of
    // rounding the entrant to 0 and clamping to 1/3.
    assert_eq!(
        comparable_inserted_shares(1, 10, &[1, 1], 0),
        Some(vec![2, 9, 9])
    );
    assert_eq!(
        comparable_inserted_shares(1, 10, &[1, 1], 1),
        Some(vec![9, 2, 9])
    );
    assert_eq!(
        comparable_inserted_shares(1, 10, &[1, 1], 2),
        Some(vec![9, 9, 2])
    );
}

#[test]
fn comparable_inserted_shares_preserves_fraction_and_survivor_ratios() {
    // Old 9/20 against resized survivors: exact fraction (2421/5380)
    // with exact survivor ratio (1254:1705 = 228:310).
    assert_eq!(
        comparable_inserted_shares(9, 20, &[228, 310], 0),
        Some(vec![2421, 1254, 1705])
    );
    // Even old shares rescale exactly (GCD-reduced).
    assert_eq!(
        comparable_inserted_shares(1, 3, &[1, 1], 1),
        Some(vec![1, 1, 1])
    );
}

#[test]
fn comparable_inserted_shares_fails_closed() {
    assert_eq!(comparable_inserted_shares(0, 10, &[1, 1], 0), None);
    assert_eq!(comparable_inserted_shares(1, 1, &[1, 1], 0), None);
    assert_eq!(comparable_inserted_shares(2, 1, &[1, 1], 0), None);
    assert_eq!(comparable_inserted_shares(1, 10, &[], 0), None);
    assert_eq!(comparable_inserted_shares(1, 10, &[1, 0], 0), None);
    assert_eq!(comparable_inserted_shares(1, 10, &[1, 1], 3), None);
    // Unrepresentable post-reduction values fall back to fresh admission.
    assert_eq!(comparable_inserted_shares(1, 2, &[u64::MAX, 1], 0), None);
}

#[test]
fn comparable_inserted_shares_rejects_nonrepresentable_total() {
    // Reduced entries each fit `u64`, but their combined total does
    // not: group totals must not overflow, so best effort falls back
    // instead of producing an invalid tree. Here the reduced vector would
    // be `[u64::MAX - 1, 1, 1]` totalling `u64::MAX + 1`.
    assert_eq!(
        comparable_inserted_shares((1u64 << 63) - 1, 1u128 << 63, &[1, 1], 0),
        None
    );
    // One below the bound still rejoins exactly: reduced
    // `[2^63 - 2, 1, 1]` totals exactly `2^63`.
    assert_eq!(
        comparable_inserted_shares((1u64 << 63) - 2, 1u128 << 63, &[1, 1], 0),
        Some(vec![(1u64 << 63) - 2, 1, 1])
    );
}

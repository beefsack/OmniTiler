#![forbid(unsafe_code)]
// Windows-only last-good declared-hint reuse on native query Timeout.
// Regression coverage for the `fresh_member_hint` decision table in
// `rows.rs`: a fresh usable hint stores immediately (replacing any previous
// entry); a successful-but-empty query clears so a later Timeout cannot
// revive it; unperformed (`Invalid`), failed, and budget outcomes never
// touch the entry; a Timeout reuses the bound entry with no time bound
// after the live lifetime-tag fence passes; a token/tag move or a lifetime
// mismatch drops and never reuses. All HWNDs are synthetic and the only
// native read on the Timeout path (`live_member_tag`) is injected, so
// every leg runs offline with no renderer, hooks, or windows.
use super::minmax::QueryCost;
use super::rmax03_adapter_tests::test_state;
use super::rows::fresh_member_hint;
use super::*;
use std::time::{Duration, Instant};
use tiler_core::size_hints::WindowSizeHints;

fn hb_hints() -> WindowSizeHints {
    WindowSizeHints {
        min_w: Some(400),
        min_h: Some(300),
        max_w: None,
        max_h: None,
    }
}

fn hb_new_hints() -> WindowSizeHints {
    WindowSizeHints {
        min_w: Some(640),
        min_h: Some(480),
        max_w: None,
        max_h: None,
    }
}

fn hb_fresh(hints: WindowSizeHints, outcome: HintOutcome) -> FreshHint {
    FreshHint {
        hints,
        outcome,
        cost: QueryCost::default(),
    }
}

/// One synthetic member bound to `token`/`tag`; the live lifetime read is
/// injected to agree unless a leg overrides it afterwards.
fn hb_member(
    state: &mut TileLoop,
    token: &str,
    hwnd: u64,
    tag: &str,
) -> crate::native_guards::WindowKey {
    let key = crate::native_guards::WindowKey {
        hwnd,
        pid: 7000 + hwnd as u32,
        creation: format!("chb{hwnd:016x}"),
    };
    state.member_tokens.insert(key.clone(), token.to_owned());
    state.member_tags.insert(key.clone(), tag.to_owned());
    inject_pending_tag(key.hwnd, Some(tag.to_owned()));
    key
}

fn hb_store(state: &mut TileLoop, key: &crate::native_guards::WindowKey, token: &str) {
    let (carried, outcome) = fresh_member_hint(
        state,
        key,
        token,
        hb_fresh(hb_hints(), HintOutcome::Hint),
        Instant::now(),
    );
    assert_eq!(carried, hb_hints());
    assert_eq!(outcome, HintOutcome::Hint);
}

fn hb_use(state: &TileLoop, token: &str) -> HintFallbackUse {
    *state
        .hint_fallback_uses
        .get(token)
        .expect("fallback use recorded")
}

#[test]
fn timeout_reuses_last_good_with_no_time_bound() {
    clear_pending_seams();
    let mut state = test_state();
    let key = hb_member(&mut state, "w1", 401_001, "hb-tag-w1");
    // Store long ago: the timestamp feeds diagnostic age only and never
    // expires the entry.
    let old = Instant::now() - Duration::from_secs(3600);
    let (carried, outcome) = fresh_member_hint(
        &mut state,
        &key,
        "w1",
        hb_fresh(hb_hints(), HintOutcome::Hint),
        old,
    );
    assert_eq!(carried, hb_hints());
    assert_eq!(outcome, HintOutcome::Hint);
    let (carried, outcome) = fresh_member_hint(
        &mut state,
        &key,
        "w1",
        FreshHint::unperformed(HintOutcome::Timeout),
        Instant::now(),
    );
    assert_eq!(carried, hb_hints(), "timeout rides the stored hints");
    assert_eq!(
        outcome,
        HintOutcome::Timeout,
        "reported outcome stays Timeout"
    );
    let use_ = hb_use(&state, "w1");
    assert!(use_.reused);
    assert_eq!(use_.reason, "reused");
    assert!(
        use_.age_ms >= 3_600_000,
        "age {:?} pins the old store",
        use_
    );
    clear_pending_seams();
}

#[test]
fn fresh_valid_change_replaces_stored_entry() {
    clear_pending_seams();
    let mut state = test_state();
    let key = hb_member(&mut state, "w1", 402_001, "hb-tag-w1");
    hb_store(&mut state, &key, "w1");
    let (carried, _) = fresh_member_hint(
        &mut state,
        &key,
        "w1",
        hb_fresh(hb_new_hints(), HintOutcome::Hint),
        Instant::now(),
    );
    assert_eq!(carried, hb_new_hints());
    let (carried, _) = fresh_member_hint(
        &mut state,
        &key,
        "w1",
        FreshHint::unperformed(HintOutcome::Timeout),
        Instant::now(),
    );
    assert_eq!(carried, hb_new_hints(), "timeout rides the replacement");
    assert!(hb_use(&state, "w1").reused);
    clear_pending_seams();
}

#[test]
fn successful_empty_clears_so_timeout_cannot_revive() {
    clear_pending_seams();
    let mut state = test_state();
    let key = hb_member(&mut state, "w1", 403_001, "hb-tag-w1");
    hb_store(&mut state, &key, "w1");
    let (carried, outcome) = fresh_member_hint(
        &mut state,
        &key,
        "w1",
        hb_fresh(WindowSizeHints::none(), HintOutcome::NoHint),
        Instant::now(),
    );
    assert!(carried.is_empty());
    assert_eq!(outcome, HintOutcome::NoHint);
    let (carried, outcome) = fresh_member_hint(
        &mut state,
        &key,
        "w1",
        FreshHint::unperformed(HintOutcome::Timeout),
        Instant::now(),
    );
    assert!(carried.is_empty(), "cleared entry never revives");
    assert_eq!(outcome, HintOutcome::Timeout);
    let use_ = hb_use(&state, "w1");
    assert!(!use_.reused);
    assert_eq!(use_.reason, "no-entry");
    clear_pending_seams();
}

#[test]
fn unperformed_invalid_preserves_entry_for_later_timeout() {
    clear_pending_seams();
    let mut state = test_state();
    let key = hb_member(&mut state, "w1", 404_001, "hb-tag-w1");
    hb_store(&mut state, &key, "w1");
    let (carried, outcome) = fresh_member_hint(
        &mut state,
        &key,
        "w1",
        FreshHint::unperformed(HintOutcome::Invalid),
        Instant::now(),
    );
    assert!(carried.is_empty());
    assert_eq!(outcome, HintOutcome::Invalid);
    assert_eq!(hb_use(&state, "w1").reason, "invalid");
    let (carried, _) = fresh_member_hint(
        &mut state,
        &key,
        "w1",
        FreshHint::unperformed(HintOutcome::Timeout),
        Instant::now(),
    );
    assert_eq!(carried, hb_hints(), "invalid never touched the entry");
    assert!(hb_use(&state, "w1").reused);
    clear_pending_seams();
}

#[test]
fn failed_and_budget_report_themselves_without_consuming_entry() {
    clear_pending_seams();
    let mut state = test_state();
    let key = hb_member(&mut state, "w1", 405_001, "hb-tag-w1");
    hb_store(&mut state, &key, "w1");
    for outcome in [HintOutcome::Failed, HintOutcome::Budget] {
        let (carried, reported) = fresh_member_hint(
            &mut state,
            &key,
            "w1",
            FreshHint::unperformed(outcome),
            Instant::now(),
        );
        assert!(carried.is_empty());
        assert_eq!(reported, outcome);
    }
    let (carried, _) = fresh_member_hint(
        &mut state,
        &key,
        "w1",
        FreshHint::unperformed(HintOutcome::Timeout),
        Instant::now(),
    );
    assert_eq!(carried, hb_hints(), "failures never consumed the entry");
    assert!(hb_use(&state, "w1").reused);
    clear_pending_seams();
}

#[test]
fn token_move_drops_entry_without_native_read() {
    clear_pending_seams();
    let mut state = test_state();
    let key = hb_member(&mut state, "w1", 406_001, "hb-tag-w1");
    hb_store(&mut state, &key, "w1");
    // The member moved on (new token, same HWND identity): the Timeout must
    // drop the stale entry and report `no-entry` without reusing.
    let (carried, outcome) = fresh_member_hint(
        &mut state,
        &key,
        "w1-moved",
        FreshHint::unperformed(HintOutcome::Timeout),
        Instant::now(),
    );
    assert!(carried.is_empty());
    assert_eq!(outcome, HintOutcome::Timeout);
    let use_ = hb_use(&state, "w1-moved");
    assert!(!use_.reused);
    assert_eq!(use_.reason, "no-entry");
    assert!(
        !state.hint_last_good.contains_key(&key),
        "moved entry drops on lookup"
    );
    clear_pending_seams();
}

#[test]
fn live_lifetime_mismatch_breaks_identity_without_reuse() {
    clear_pending_seams();
    let mut state = test_state();
    let key = hb_member(&mut state, "w1", 407_001, "hb-tag-w1");
    hb_store(&mut state, &key, "w1");
    // Same-process HWND reuse: token/tag still bind, but the live lifetime
    // property disagrees, so reuse is refused and the entry is dropped.
    inject_pending_tag(key.hwnd, Some("hb-tag-stale".to_owned()));
    let (carried, outcome) = fresh_member_hint(
        &mut state,
        &key,
        "w1",
        FreshHint::unperformed(HintOutcome::Timeout),
        Instant::now(),
    );
    assert!(carried.is_empty());
    assert_eq!(outcome, HintOutcome::Timeout);
    let use_ = hb_use(&state, "w1");
    assert!(!use_.reused);
    assert_eq!(use_.reason, "identity");
    assert!(
        !state.hint_last_good.contains_key(&key),
        "broken entry drops"
    );
    clear_pending_seams();
}

#[test]
fn absent_live_tag_never_reuses() {
    clear_pending_seams();
    let mut state = test_state();
    let key = hb_member(&mut state, "w1", 408_001, "hb-tag-w1");
    hb_store(&mut state, &key, "w1");
    // Untracked window (no live lifetime property): an empty stored tag
    // never matches, so the Timeout reports `identity` without reuse.
    inject_pending_tag(key.hwnd, None);
    let (carried, outcome) = fresh_member_hint(
        &mut state,
        &key,
        "w1",
        FreshHint::unperformed(HintOutcome::Timeout),
        Instant::now(),
    );
    assert!(carried.is_empty());
    assert_eq!(outcome, HintOutcome::Timeout);
    let use_ = hb_use(&state, "w1");
    assert!(!use_.reused);
    assert_eq!(use_.reason, "identity");
    clear_pending_seams();
}

#![forbid(unsafe_code)]
use super::{KeyboardConfig, MaskTrigger, SnapClassify, VK_LWIN, VK_RWIN};

fn enabled() -> KeyboardConfig {
    KeyboardConfig {
        takeover: true,
        allow_win_l: true,
    }
}

#[test]
fn windrag_arms_mask_once_and_both_win_terminal_clears() {
    let mut m = SnapClassify::new(enabled());
    SnapClassify::push(&mut m, VK_LWIN, false, true, false);
    m.note_windrag_consumed();
    m.note_windrag_consumed();
    let first = m.win_up_needs_mask(VK_LWIN, true);
    assert_eq!(first, Some(MaskTrigger::WinDrag));
    // A fresh hold arms and fires exactly once; a stale extra Win-up
    // after the both-Wins terminal cleared the hold never re-fires.
    SnapClassify::push(&mut m, VK_RWIN, false, true, false);
    m.note_windrag_consumed();
    let _ = m.win_up_needs_mask(VK_RWIN, true);
    let second = m.win_up_needs_mask(VK_LWIN, true);
    assert_eq!(second, None);
}

#[test]
fn windrag_mask_state_only_path_leaves_no_stale_flag() {
    // State-only path (installed=false, e.g. synthetic Win traffic the
    // callback filtered) updates bookkeeping without arming a send, and
    // the both-Wins-up terminal clears any pending flag.
    let mut m = SnapClassify::new(enabled());
    SnapClassify::push(&mut m, VK_LWIN, false, true, false);
    m.note_windrag_consumed();
    let _ = m.win_up_needs_mask(VK_LWIN, false);
    let again = m.win_up_needs_mask(VK_LWIN, false);
    assert_eq!(again, None);
}

#[test]
fn injected_win_never_arms_mask() {
    // Injected input returns None before any bookkeeping: a synthetic
    // Win hold leaves no mask obligation, so a later physical Win-up
    // sends no product mask for it.
    let mut m = SnapClassify::new(enabled());
    let out = SnapClassify::push(&mut m, VK_LWIN, false, true, true);
    assert_eq!(out, None);
    let fire = m.win_up_needs_mask(VK_LWIN, true);
    assert_eq!(fire, None);
}

#[test]
fn synthetic_note_cannot_corrupt_next_bare_physical_win() {
    // A synthetic mouse gesture notes while no physical Win is tracked:
    // the note must NOOP, so the next bare physical Win tap opens Start
    // normally with no product mask.
    let mut m = SnapClassify::new(enabled());
    m.note_windrag_consumed();
    SnapClassify::push(&mut m, VK_LWIN, false, true, false);
    let fire = m.win_up_needs_mask(VK_LWIN, true);
    assert_eq!(fire, None);
}

#[test]
fn windrag_mask_waits_for_both_tracked_wins_up() {
    // Genuinely simultaneous both-Win hold: the first Win-up must not
    // fire while the other side is still tracked held; the terminal
    // release fires exactly once, then the hold is clean.
    let mut m = SnapClassify::new(enabled());
    SnapClassify::push(&mut m, VK_LWIN, false, true, false);
    SnapClassify::push(&mut m, VK_RWIN, false, true, false);
    m.note_windrag_consumed();
    assert_eq!(m.win_up_needs_mask(VK_LWIN, true), None);
    assert_eq!(
        m.win_up_needs_mask(VK_RWIN, true),
        Some(MaskTrigger::WinDrag)
    );
    assert_eq!(m.win_up_needs_mask(VK_LWIN, true), None);
    assert_eq!(m.win_up_needs_mask(VK_RWIN, true), None);
}

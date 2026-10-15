use super::{KeyboardConfig, SnapOp, VK_LWIN};

fn enabled() -> KeyboardConfig {
    KeyboardConfig {
        takeover: true,
        allow_win_l: true,
    }
}

#[test]
fn preheld_shift_selects_move_on_fresh_down() {
    // Preheld Shift (held before install, tracked state clear) binds from
    // the async sample so the fresh down resolves move, not focus.
    let mut m = super::SnapClassify::new(enabled());
    super::SnapClassify::push(&mut m, VK_LWIN, false, true, false);
    assert!(!m.tracked_modifiers().2);
    m.sync_shift_from_async(true);
    let down = super::SnapClassify::push(&mut m, super::VK_H, false, true, false)
        .and_then(|ev| match ev {
            super::Classified::Snap(intent) => Some(intent),
            _ => None,
        })
        .expect("fresh down");
    assert_eq!(down.op, SnapOp::Move);
    assert!(down.consumed);
}

#[test]
fn missed_shift_up_resolves_focus_on_fresh_down() {
    // Missed Shift-up (tracked held, physical released) syncs back so a
    // fresh down resolves focus, not a stale move.
    let mut m = super::SnapClassify::new(enabled());
    super::SnapClassify::push(&mut m, VK_LWIN, false, true, false);
    super::SnapClassify::push(&mut m, super::VK_SHIFT, false, true, false);
    assert!(m.tracked_modifiers().2);
    m.sync_shift_from_async(false);
    let down = super::SnapClassify::push(&mut m, super::VK_H, false, true, false)
        .and_then(|ev| match ev {
            super::Classified::Snap(intent) => Some(intent),
            _ => None,
        })
        .expect("fresh down");
    assert_eq!(down.op, SnapOp::Focus);
}

#[test]
fn armed_hold_keeps_down_time_op_across_sync() {
    // Syncing Shift mid-hold never flips the armed op: repeats and the
    // paired up ride the pinned down-time op.
    let mut m = super::SnapClassify::new(enabled());
    super::SnapClassify::push(&mut m, VK_LWIN, false, true, false);
    let down = super::SnapClassify::push(&mut m, super::VK_H, false, true, false)
        .and_then(|ev| match ev {
            super::Classified::Snap(intent) => Some(intent),
            _ => None,
        })
        .expect("fresh down");
    assert_eq!(down.op, SnapOp::Focus);
    m.sync_shift_from_async(true);
    let repeat = super::SnapClassify::push(&mut m, super::VK_H, false, true, false)
        .and_then(|ev| match ev {
            super::Classified::Snap(intent) => Some(intent),
            _ => None,
        })
        .expect("repeat");
    assert_eq!(repeat.op, SnapOp::Focus);
    assert!(repeat.consumed);
    assert!(
        !repeat.announce,
        "shift-flipped repeat swallows without dispatch"
    );
    let up = super::SnapClassify::push(&mut m, super::VK_H, true, true, false)
        .and_then(|ev| match ev {
            super::Classified::Snap(intent) => Some(intent),
            _ => None,
        })
        .expect("paired up");
    assert_eq!(up.op, SnapOp::Focus);
    assert!(up.consumed);
}

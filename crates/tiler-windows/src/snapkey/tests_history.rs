use super::{
    ChordAction, ChordDisable, ChordRemap, Classified, KeyboardConfig, MaskTrigger, SnapClassify,
    SnapEdge, VK_CONTROL, VK_DOWN, VK_H, VK_J, VK_K, VK_L, VK_LEFT, VK_LWIN, VK_MENU, VK_RIGHT,
    VK_SHIFT, VK_TAB, VK_UP, WorkspaceHistoryOp,
};
use tiler_core::directional::Direction;

fn enabled() -> KeyboardConfig {
    KeyboardConfig {
        takeover: true,
        allow_win_l: false,
    }
}

fn ctrl_down(m: &mut SnapClassify) {
    SnapClassify::push(m, VK_CONTROL, false, true, false);
}

fn ctrl_up(m: &mut SnapClassify) {
    SnapClassify::push(m, VK_CONTROL, true, true, false);
}

fn history_op(m: &mut SnapClassify, vk: u32) -> WorkspaceHistoryOp {
    match SnapClassify::push(m, vk, false, true, false).expect("history down") {
        Classified::WorkspaceHistory(intent) => {
            assert!(intent.consumed && intent.announce);
            intent.op
        }
        other => panic!("expected history, got {other:?}"),
    }
}

#[test]
fn exact_ctrl_tab_routing_with_op_per_key() {
    // Win+Ctrl+Tab toggles, H/K/Left/Up step prev, J/L/Down/Right next.
    // Bare Tab/Chords without Win or Ctrl pass through untracked.
    for (vk, op) in [
        (VK_TAB, WorkspaceHistoryOp::Previous),
        (VK_H, WorkspaceHistoryOp::Prev),
        (VK_K, WorkspaceHistoryOp::Prev),
        (VK_LEFT, WorkspaceHistoryOp::Prev),
        (VK_UP, WorkspaceHistoryOp::Prev),
        (VK_J, WorkspaceHistoryOp::Next),
        (VK_L, WorkspaceHistoryOp::Next),
        (VK_DOWN, WorkspaceHistoryOp::Next),
        (VK_RIGHT, WorkspaceHistoryOp::Next),
    ] {
        let mut m = SnapClassify::new(enabled());
        SnapClassify::push(&mut m, VK_LWIN, false, true, false);
        ctrl_down(&mut m);
        assert_eq!(history_op(&mut m, vk), op);
        match SnapClassify::push(&mut m, vk, true, true, false).expect("history up") {
            Classified::WorkspaceHistory(intent) => {
                assert_eq!(intent.op, op);
                assert!(intent.consumed && !intent.announce);
            }
            other => panic!("expected history up, got {other:?}"),
        }
        ctrl_up(&mut m);
        assert!(!m.key_is_down(vk));
    }
    // Bare (no Win) and non-catalog Alt-modified chords refuse with no
    // pin, so their paired ups also pass. Shift+Ctrl no longer refuses:
    // item 2 routes Win+Ctrl+Shift+H to the relative-send follow arm
    // (history itself still demands unshifted Win+Ctrl), and item 5
    // routes Win+Ctrl+Alt+H to the output follow arm.
    let mut m = SnapClassify::new(enabled());
    assert_eq!(SnapClassify::push(&mut m, VK_TAB, false, true, false), None);
    assert_eq!(SnapClassify::push(&mut m, VK_TAB, true, true, false), None);
    let mut m = SnapClassify::new(enabled());
    SnapClassify::push(&mut m, VK_LWIN, false, true, false);
    SnapClassify::push(&mut m, VK_SHIFT, false, true, false);
    ctrl_down(&mut m);
    match SnapClassify::push(&mut m, VK_H, false, true, false).expect("relative send") {
        Classified::WorkspaceSend(intent) => {
            assert_eq!((intent.delta, intent.follow), (-1, true));
            assert!(intent.consumed && intent.announce);
        }
        other => panic!("expected relative send, got {other:?}"),
    }
    let mut m = SnapClassify::new(enabled());
    SnapClassify::push(&mut m, VK_LWIN, false, true, false);
    SnapClassify::push(&mut m, VK_MENU, false, true, false);
    ctrl_down(&mut m);
    match SnapClassify::push(&mut m, VK_H, false, true, false).expect("output send") {
        Classified::WorkspaceOutput(intent) => {
            assert_eq!(intent.direction, Direction::Left);
            assert!(intent.follow && intent.consumed && intent.announce);
        }
        other => panic!("expected output send, got {other:?}"),
    }
    // A non-catalog key under the same modifiers still refuses with no
    // pin, so its paired up passes too.
    let mut m = SnapClassify::new(enabled());
    SnapClassify::push(&mut m, VK_LWIN, false, true, false);
    SnapClassify::push(&mut m, VK_MENU, false, true, false);
    ctrl_down(&mut m);
    assert_eq!(SnapClassify::push(&mut m, VK_TAB, false, true, false), None);
    assert_eq!(SnapClassify::push(&mut m, VK_TAB, true, true, false), None);
}

#[test]
fn history_repeat_dispatches_while_ctrl_live() {
    // Held-key autorepeat: repeats dispatch while Win+Ctrl holds, stay
    // swallowed (no dispatch) once Ctrl leaves, pair closes consumed.
    let mut m = SnapClassify::new(enabled());
    SnapClassify::push(&mut m, VK_LWIN, false, true, false);
    ctrl_down(&mut m);
    assert_eq!(history_op(&mut m, VK_J), WorkspaceHistoryOp::Next);
    match SnapClassify::push(&mut m, VK_J, false, true, false).expect("repeat") {
        Classified::WorkspaceHistory(intent) => {
            assert_eq!(
                (intent.op, intent.edge),
                (WorkspaceHistoryOp::Next, SnapEdge::Repeat)
            );
            assert!(intent.consumed && intent.announce);
        }
        other => panic!("expected repeat, got {other:?}"),
    }
    ctrl_up(&mut m);
    match SnapClassify::push(&mut m, VK_J, false, true, false).expect("repeat") {
        Classified::WorkspaceHistory(intent) => assert!(intent.consumed && !intent.announce),
        other => panic!("expected swallowed repeat, got {other:?}"),
    }
    match SnapClassify::push(&mut m, VK_J, true, true, false).expect("up") {
        Classified::WorkspaceHistory(intent) => assert!(intent.consumed && !intent.announce),
        other => panic!("expected up, got {other:?}"),
    }
}

#[test]
fn previous_hold_repeat_swallows_without_pingpong() {
    // Win+Ctrl+Tab down dispatches once; held repeats stay consumed and
    // masked but never re-dispatch (maximize parity: otherwise the hold
    // would pingpong between the two views). The paired release closes
    // consumed with no orphan hold behind it.
    let mut m = SnapClassify::new(enabled());
    SnapClassify::push(&mut m, VK_LWIN, false, true, false);
    ctrl_down(&mut m);
    assert_eq!(history_op(&mut m, VK_TAB), WorkspaceHistoryOp::Previous);
    for _ in 0..2 {
        match SnapClassify::push(&mut m, VK_TAB, false, true, false).expect("repeat") {
            Classified::WorkspaceHistory(intent) => {
                assert_eq!(
                    (intent.op, intent.edge),
                    (WorkspaceHistoryOp::Previous, SnapEdge::Repeat)
                );
                assert!(intent.consumed && !intent.announce);
            }
            other => panic!("expected swallowed repeat, got {other:?}"),
        }
    }
    match SnapClassify::push(&mut m, VK_TAB, true, true, false).expect("up") {
        Classified::WorkspaceHistory(intent) => {
            assert_eq!(intent.op, WorkspaceHistoryOp::Previous);
            assert!(intent.consumed && !intent.announce);
        }
        other => panic!("expected up, got {other:?}"),
    }
    assert!(!m.key_is_down(VK_TAB));
    assert_eq!(SnapClassify::push(&mut m, VK_TAB, true, true, false), None);
    // Relative steps still re-dispatch while live (control case).
    let mut m = SnapClassify::new(enabled());
    SnapClassify::push(&mut m, VK_LWIN, false, true, false);
    ctrl_down(&mut m);
    assert_eq!(history_op(&mut m, VK_J), WorkspaceHistoryOp::Next);
    match SnapClassify::push(&mut m, VK_J, false, true, false).expect("repeat") {
        Classified::WorkspaceHistory(intent) => assert!(intent.consumed && intent.announce),
        other => panic!("expected dispatching repeat, got {other:?}"),
    }
}

#[test]
fn history_held_rebind_disable_gate_repeat_but_keep_pair() {
    // Rebound history (Win+Ctrl+U -> Prev/H) dispatches; removing the
    // mapping mid-hold swallows repeats without dispatching while the
    // pair still closes consumed; disabling the canonical mid-hold does
    // the same; fresh presses afterwards pass through.
    let remap = || {
        vec![ChordRemap {
            from_vk: 0x55,
            from_shift: false,
            from_ctrl: true,
            from_alt: false,
            action: ChordAction::WorkspacePrev,
            to_vk: VK_H,
        }]
    };
    let mut m = SnapClassify::new(enabled());
    m.set_remap(remap());
    SnapClassify::push(&mut m, VK_LWIN, false, true, false);
    ctrl_down(&mut m);
    assert_eq!(history_op(&mut m, 0x55), WorkspaceHistoryOp::Prev);
    m.set_remap(Vec::new());
    match SnapClassify::push(&mut m, 0x55, false, true, false).expect("repeat") {
        Classified::WorkspaceHistory(intent) => assert!(intent.consumed && !intent.announce),
        other => panic!("expected swallowed repeat, got {other:?}"),
    }
    match SnapClassify::push(&mut m, 0x55, true, true, false).expect("up") {
        Classified::WorkspaceHistory(intent) => assert!(intent.consumed),
        other => panic!("expected up, got {other:?}"),
    }
    assert_eq!(SnapClassify::push(&mut m, 0x55, false, true, false), None);
    // Canonical disable mid-hold gates dispatch but keeps the pair.
    let mut m = SnapClassify::new(enabled());
    SnapClassify::push(&mut m, VK_LWIN, false, true, false);
    ctrl_down(&mut m);
    assert_eq!(history_op(&mut m, VK_H), WorkspaceHistoryOp::Prev);
    m.set_disabled(vec![ChordDisable {
        vk: VK_H,
        shift: false,
        ctrl: true,
        alt: false,
    }]);
    match SnapClassify::push(&mut m, VK_H, false, true, false).expect("repeat") {
        Classified::WorkspaceHistory(intent) => assert!(intent.consumed && !intent.announce),
        other => panic!("expected swallowed repeat, got {other:?}"),
    }
    match SnapClassify::push(&mut m, VK_H, true, true, false).expect("up") {
        Classified::WorkspaceHistory(intent) => assert!(intent.consumed),
        other => panic!("expected up, got {other:?}"),
    }
}

#[test]
fn history_arms_mask_and_survives_saturation() {
    // A consumed history chord arms the Start-menu mask; saturation drops
    // only queued evidence while the verdict still swallows.
    use super::{SnapQueue, classify_and_queue, win_up_mask_reserve};
    let mut m = SnapClassify::new(enabled());
    let mut q = SnapQueue::new();
    SnapClassify::push(&mut m, VK_LWIN, false, true, false);
    ctrl_down(&mut m);
    let down = SnapClassify::push(&mut m, VK_TAB, false, true, false).expect("down");
    assert!(down.consumed() && down.announce());
    assert!(win_up_mask_reserve(
        &mut m,
        &mut q,
        VK_LWIN,
        true,
        std::time::Instant::now()
    ));
    match q.pop_front().expect("mask") {
        super::QueuedSnapEvent::Mask(mask) => {
            assert_eq!(
                mask.trigger,
                MaskTrigger::WorkspaceHistory {
                    op: WorkspaceHistoryOp::Previous
                }
            )
        }
        other => panic!("expected mask, got {other:?}"),
    }
    // Queue/mask routing: history classifies and queues with announce.
    let mut m = SnapClassify::new(enabled());
    let mut q = SnapQueue::new();
    let tick = std::time::Instant::now();
    assert_eq!(
        classify_and_queue(&mut m, &mut q, VK_LWIN, false, None, false, tick, true),
        None
    );
    assert_eq!(
        classify_and_queue(&mut m, &mut q, VK_CONTROL, false, None, false, tick, true),
        None
    );
    assert_eq!(
        classify_and_queue(&mut m, &mut q, VK_J, false, None, false, tick, true),
        Some(true)
    );
    match q.pop_front().expect("history") {
        super::QueuedSnapEvent::WorkspaceHistory(queued) => {
            assert_eq!(queued.op, WorkspaceHistoryOp::Next);
            assert!(queued.consumed && queued.announce);
        }
        other => panic!("expected history, got {other:?}"),
    }
}

#[test]
fn history_cross_arm_holds_never_collide() {
    // Win+H (focus) and Win+Ctrl+H (history-prev) share VK_H across
    // separate slots: both hold independently, release in any order.
    let mut m = SnapClassify::new(enabled());
    SnapClassify::push(&mut m, VK_LWIN, false, true, false);
    match SnapClassify::push(&mut m, VK_H, false, true, false).expect("focus") {
        Classified::Snap(intent) => assert!(intent.consumed),
        other => panic!("expected focus, got {other:?}"),
    }
    ctrl_down(&mut m);
    // Same physical H is already pinned to focus: this is the owner's
    // repeat (swallowed, no dispatch flip into history).
    match SnapClassify::push(&mut m, VK_H, false, true, false).expect("repeat") {
        Classified::Snap(intent) => assert!(intent.consumed && !intent.announce),
        other => panic!("expected pinned focus repeat, got {other:?}"),
    }
    ctrl_up(&mut m);
    match SnapClassify::push(&mut m, VK_H, true, true, false).expect("up") {
        Classified::Snap(intent) => assert!(intent.consumed),
        other => panic!("expected focus up, got {other:?}"),
    }
    // Fresh history after the focus hold closed.
    ctrl_down(&mut m);
    assert_eq!(history_op(&mut m, VK_H), WorkspaceHistoryOp::Prev);
}

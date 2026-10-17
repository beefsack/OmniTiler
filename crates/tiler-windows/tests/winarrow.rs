#![forbid(unsafe_code)]
use tiler_windows::winarrow::{
    Arrow, EVENT_QUEUE_CAP, Edge, EventQueue, HookClassify, QueuedChord, QueuedEvent, QueuedMask,
    VK_CONTROL, VK_DOWN, VK_LCONTROL, VK_LEFT, VK_LMENU, VK_LSHIFT, VK_LWIN, VK_MASK, VK_MENU,
    VK_RWIN, VK_SHIFT, events_path_for, is_win_vk, new_hook_method, parse_spike_run_args,
    stamp_mask_result, win_up_mask_reserve,
};

fn strings(args: &[&str]) -> Vec<String> {
    args.iter().map(|s| (*s).to_owned()).collect()
}

fn consumed_down(m: &mut HookClassify, win: u32, vk: u32) {
    m.push(win, false, true, false);
    let ev = m.push(vk, false, true, false).expect("consumed down");
    assert!(ev.consumed);
    assert!(ev.announce);
    assert_eq!(ev.edge, Edge::Down);
}

#[test]
fn arrow_codes_roundtrip() {
    for arrow in Arrow::all() {
        assert_eq!(Arrow::from_vk(arrow.vk()), Some(arrow));
    }
    assert_eq!(Arrow::from_vk(0), None);
    assert_eq!(Arrow::from_vk(65), None);
    assert!(is_win_vk(91));
    assert!(is_win_vk(92));
    assert!(!is_win_vk(37));
}

#[test]
fn arrow_offsets_show_direction() {
    let (dx, _) = Arrow::Left.offset();
    assert!(dx < 0);
    let (dx, _) = Arrow::Right.offset();
    assert!(dx > 0);
    let (_, dy) = Arrow::Up.offset();
    assert!(dy < 0);
    let (_, dy) = Arrow::Down.offset();
    assert!(dy > 0);
}

#[test]
fn chord_down_up_pair_consumes_with_action_flag() {
    let mut m = HookClassify::new(true);
    assert_eq!(m.push(VK_LWIN, false, true, false), None);
    let down = m.push(VK_LEFT, false, true, false).expect("down");
    assert_eq!(
        (down.arrow, down.edge, down.consumed, down.announce),
        (Arrow::Left, Edge::Down, true, true)
    );
    let up = m.push(VK_LEFT, true, true, false).expect("up");
    assert_eq!(
        (up.arrow, up.edge, up.consumed, up.announce),
        (Arrow::Left, Edge::Up, true, false)
    );
    let c = m.chords[0];
    assert_eq!(
        (c.down, c.up, c.repeat, c.consumed, c.passed),
        (1, 1, 0, 2, 0)
    );
    m.note_acted(Arrow::Left);
    assert_eq!(m.chords[0].acted, 1);
}

#[test]
fn chord_repeat_consumes_each_time() {
    let mut m = HookClassify::new(true);
    m.push(VK_RWIN, false, true, false);
    let first = m.push(VK_LEFT, false, true, false).expect("first");
    assert_eq!(first.edge, Edge::Down);
    let second = m.push(VK_LEFT, false, true, false).expect("repeat");
    assert_eq!(
        (second.edge, second.consumed, second.announce),
        (Edge::Repeat, true, true)
    );
    let c = m.chords[0];
    assert_eq!((c.down, c.repeat, c.consumed), (1, 1, 2));
}

#[test]
fn win_release_before_arrow_up_still_pairs() {
    let mut m = HookClassify::new(true);
    m.push(VK_LWIN, false, true, false);
    m.push(VK_LEFT, false, true, false);
    m.push(VK_LWIN, true, true, false);
    let up = m.push(VK_LEFT, true, true, false).expect("paired up");
    assert!(up.consumed);
    assert_eq!(m.chords[0].up, 1);
}

#[test]
fn extra_modifier_chords_pass_untracked() {
    for mod_vk in [
        VK_CONTROL,
        VK_LCONTROL,
        VK_MENU,
        VK_LMENU,
        VK_SHIFT,
        VK_LSHIFT,
    ] {
        let mut m = HookClassify::new(true);
        m.push(VK_LWIN, false, true, false);
        m.push(mod_vk, false, true, false);
        assert_eq!(m.push(VK_LEFT, false, true, false), None);
        assert_eq!(m.push(VK_LEFT, true, true, false), None);
        let c = m.chords[0];
        assert_eq!((c.down, c.up, c.consumed, c.passed), (0, 0, 0, 0));
        m.push(mod_vk, true, true, false);
        m.push(VK_LWIN, true, true, false);
        assert_eq!(
            m.push(VK_LEFT, false, true, false),
            None,
            "plain arrow without Win still passes"
        );
    }
}

#[test]
fn paired_up_after_extra_modifier_stays_consistent() {
    let mut m = HookClassify::new(true);
    m.push(VK_LWIN, false, true, false);
    assert!(m.push(VK_LEFT, false, true, false).is_some());
    m.push(VK_CONTROL, false, true, false);
    let up = m.push(VK_LEFT, true, true, false).expect("paired up");
    assert!(up.consumed);
    let c = m.chords[0];
    assert_eq!((c.down, c.up, c.consumed), (1, 1, 2));
}

#[test]
fn extra_modifier_down_never_steals_unrelated_up() {
    let mut m = HookClassify::new(true);
    m.push(VK_LWIN, false, true, false);
    m.push(VK_CONTROL, false, true, false);
    assert_eq!(m.push(VK_LEFT, false, true, false), None);
    m.push(VK_CONTROL, true, true, false);
    m.push(VK_LWIN, true, true, false);
    assert_eq!(m.push(VK_LEFT, true, true, false), None);
    assert_eq!(m.chords[0].up, 0);
}

#[test]
fn background_down_up_logged_passed() {
    // Every approved chord edge is evidence, even background-initiated.
    let mut m = HookClassify::new(true);
    m.push(VK_LWIN, false, false, false);
    let down = m.push(VK_LEFT, false, false, false).expect("logged");
    assert_eq!(
        (down.edge, down.consumed, down.announce, down.foreground),
        (Edge::Down, false, false, false)
    );
    let up = m.push(VK_LEFT, true, false, false).expect("logged");
    assert_eq!(
        (up.edge, up.consumed, up.announce),
        (Edge::Up, false, false)
    );
    let c = m.chords[0];
    assert_eq!((c.down, c.up, c.consumed, c.passed), (1, 1, 0, 2));
}

#[test]
fn background_origin_never_consumes_midhold() {
    // A background-origin hold stays passed when it turns foreground, and its
    // key-up is never stolen: a later unrelated up stays untracked.
    let mut m = HookClassify::new(true);
    m.push(VK_LWIN, false, false, false);
    m.push(VK_LEFT, false, false, false);
    let rep = m.push(VK_LEFT, false, true, false).expect("logged");
    assert_eq!((rep.edge, rep.consumed), (Edge::Repeat, false));
    let up = m.push(VK_LEFT, true, true, false).expect("logged");
    assert!(!up.consumed, "background-origin up must pass, never stolen");
    assert_eq!(
        m.push(VK_LEFT, true, true, false),
        None,
        "sequence cleared; unrelated up untracked"
    );
    let c = m.chords[0];
    assert_eq!(
        (c.down, c.repeat, c.up, c.consumed, c.passed),
        (1, 1, 1, 0, 3)
    );
}

#[test]
fn repeat_edge_identity_per_origin() {
    let mut m = HookClassify::new(true);
    m.push(VK_LWIN, false, true, false);
    m.push(VK_LEFT, false, true, false);
    let fg_rep = m.push(VK_LEFT, false, true, false).expect("repeat");
    assert_eq!(
        (fg_rep.edge, fg_rep.consumed, fg_rep.announce),
        (Edge::Repeat, true, true)
    );
    m.push(VK_LEFT, true, true, false);
    m.push(VK_LWIN, true, true, false);
    // Fresh background-origin hold: repeat keeps the Repeat edge, passed.
    m.push(VK_RWIN, false, false, false);
    m.push(VK_LEFT, false, false, false);
    let bg_rep = m.push(VK_LEFT, false, false, false).expect("repeat");
    assert_eq!(
        (bg_rep.edge, bg_rep.consumed, bg_rep.announce),
        (Edge::Repeat, false, false)
    );
}

#[test]
fn background_repeat_and_up_pass_but_log() {
    // A hold that starts in the foreground keeps logging while background.
    let mut m = HookClassify::new(true);
    m.push(VK_LWIN, false, true, false);
    m.push(VK_LEFT, false, true, false);
    let rep = m.push(VK_LEFT, false, false, false).expect("logged");
    assert_eq!(
        (rep.edge, rep.consumed, rep.announce, rep.foreground),
        (Edge::Repeat, false, false, false)
    );
    let up = m.push(VK_LEFT, true, false, false).expect("logged");
    assert!(!up.consumed);
    let c = m.chords[0];
    assert_eq!(
        (c.down, c.repeat, c.up, c.consumed, c.passed),
        (1, 1, 1, 1, 2)
    );
}

#[test]
fn background_paired_up_passes_but_counts() {
    let mut m = HookClassify::new(true);
    m.push(VK_LWIN, false, true, false);
    m.push(VK_LEFT, false, true, false);
    let up = m.push(VK_LEFT, true, false, false).expect("logged");
    assert!(!up.consumed);
    let c = m.chords[0];
    assert_eq!((c.up, c.consumed, c.passed), (1, 1, 1));
}

#[test]
fn foreground_transition_passes_without_uninstall() {
    // Gating: focus loss passes chords through but never disables the machine.
    let mut m = HookClassify::new(true);
    m.push(VK_LWIN, false, true, false);
    let first = m.push(VK_LEFT, false, true, false).expect("fg down");
    assert!(first.consumed && first.announce);
    let rep = m.push(VK_LEFT, false, false, false).expect("bg repeat");
    assert!(!rep.consumed);
    m.push(VK_LEFT, true, false, false);
    let fg = m.push(VK_LEFT, false, true, false).expect("fg again");
    assert!(fg.consumed && fg.announce);
    m.push(VK_LEFT, true, true, false);
    assert!(m.enabled, "focus loss must not disable classification");
    let c = m.chords[0];
    assert_eq!(
        (c.down, c.repeat, c.up, c.consumed, c.passed),
        (2, 1, 2, 3, 2)
    );
}

#[test]
fn disabled_input_counts_but_passes() {
    let mut m = HookClassify::new(false);
    m.push(VK_LWIN, false, true, false);
    let down = m.push(VK_LEFT, false, true, false).expect("logged");
    assert!(!down.consumed && !down.announce);
    let up = m.push(VK_LEFT, true, true, false).expect("logged");
    assert!(!up.consumed);
    let c = m.chords[0];
    assert_eq!((c.down, c.up, c.consumed, c.passed), (1, 1, 0, 2));
    m.set_enabled(true);
    let back = m.push(VK_DOWN, false, true, false).expect("consumed");
    assert!(back.consumed);
}

#[test]
fn plain_arrows_and_injected_pass_untracked() {
    let mut m = HookClassify::new(true);
    assert_eq!(m.push(VK_LEFT, false, true, false), None);
    assert_eq!(m.push(VK_LEFT, true, true, false), None);
    m.push(VK_LWIN, false, true, false);
    assert_eq!(m.push(VK_LEFT, false, true, true), None);
    assert_eq!(m.push(65, false, true, false), None);
    for c in &m.chords {
        assert_eq!(c.down, 0);
        assert_eq!(c.consumed, 0);
    }
}

#[test]
fn queue_is_bounded_and_fail_closed() {
    let mut q = EventQueue::new();
    assert!(q.is_empty());
    for i in 0..EVENT_QUEUE_CAP {
        let ok = q.push(QueuedEvent::Chord(QueuedChord {
            arrow: Arrow::Left,
            edge: Edge::Down,
            foreground: true,
            consumed: true,
            announce: true,
            tick: std::time::Instant::now(),
        }));
        assert!(ok, "push {i} must fit");
    }
    assert!(q.is_full());
    assert_eq!(q.len(), EVENT_QUEUE_CAP);
    assert!(!q.push(QueuedEvent::Chord(QueuedChord {
        arrow: Arrow::Left,
        edge: Edge::Down,
        foreground: true,
        consumed: true,
        announce: true,
        tick: std::time::Instant::now(),
    })));
    assert_eq!(q.dropped, 1);
    // FIFO order preserved for the drain.
    assert_eq!(q.len(), EVENT_QUEUE_CAP);
    assert!(q.pop_front().is_some());
    assert_eq!(q.len(), EVENT_QUEUE_CAP - 1);
    while q.pop_front().is_some() {}
    assert!(q.is_empty());
    assert_eq!(q.pop_front(), None);
}

fn gated(
    m: &mut HookClassify,
    q: &mut EventQueue,
    vk: u32,
    is_up: bool,
    foreground: bool,
    injected: bool,
) -> Option<bool> {
    tiler_windows::winarrow::classify_and_queue(
        m,
        q,
        vk,
        is_up,
        foreground,
        injected,
        std::time::Instant::now(),
    )
}

#[test]
fn saturated_gate_passes_without_consuming() {
    // Same path the callback uses: a full queue keeps bookkeeping but never
    // consumes, and counts the loss for an approved chord only.
    let mut m = HookClassify::new(true);
    let mut q = EventQueue::new();
    m.push(VK_LWIN, false, true, false);
    assert_eq!(
        gated(&mut m, &mut q, VK_LEFT, false, true, false),
        Some(true)
    );
    assert_eq!(q.len(), 1);
    while q.len() < EVENT_QUEUE_CAP {
        let ok = q.push(QueuedEvent::Chord(QueuedChord {
            arrow: Arrow::Right,
            edge: Edge::Down,
            foreground: true,
            consumed: true,
            announce: true,
            tick: std::time::Instant::now(),
        }));
        assert!(ok);
    }
    assert!(q.is_full());
    // Approved chord while saturated: passed through, loss counted once.
    assert_eq!(
        gated(&mut m, &mut q, VK_LEFT, true, true, false),
        Some(false)
    );
    assert_eq!(q.dropped, 1);
    assert_eq!(m.chords[0].passed, 1);
    // Injected and unrelated inputs: no queue interaction, no loss counted.
    assert_eq!(gated(&mut m, &mut q, VK_LEFT, false, true, true), None);
    assert_eq!(gated(&mut m, &mut q, 65, false, true, false), None);
    assert_eq!(q.dropped, 1);
}

#[test]
fn saturated_hold_keeps_pairing_after_drain() {
    // A down that arrives saturated stays passed after the drain, and its up
    // clears the sequence without stealing a later unrelated up.
    let mut m = HookClassify::new(true);
    let mut q = EventQueue::new();
    m.push(VK_LWIN, false, true, false);
    while q.len() < EVENT_QUEUE_CAP {
        let ok = q.push(QueuedEvent::Chord(QueuedChord {
            arrow: Arrow::Right,
            edge: Edge::Down,
            foreground: true,
            consumed: true,
            announce: true,
            tick: std::time::Instant::now(),
        }));
        assert!(ok);
    }
    assert_eq!(
        gated(&mut m, &mut q, VK_LEFT, false, true, false),
        Some(false)
    );
    assert!(m.enabled, "saturation must not disable the machine");
    while q.pop_front().is_some() {}
    let up = m.push(VK_LEFT, true, true, false).expect("paired up");
    assert!(!up.consumed, "saturated-origin hold stays passed");
    assert_eq!(m.push(VK_LEFT, true, true, false), None);
    let c = m.chords[0];
    assert_eq!((c.down, c.up, c.consumed, c.passed), (1, 1, 0, 2));
    assert_eq!(q.dropped, 1);
}

#[test]
fn mask_key_is_unassigned_e8() {
    assert_eq!(VK_MASK, 0xE8);
}

fn reserve(m: &mut HookClassify, q: &mut EventQueue, vk: u32, installed: bool) -> bool {
    win_up_mask_reserve(m, q, vk, installed, std::time::Instant::now())
}

fn pop_mask(q: &mut EventQueue) -> QueuedMask {
    match q.pop_front().expect("mask record") {
        QueuedEvent::Mask(m) => m,
        QueuedEvent::Chord(_) => panic!("expected mask record"),
    }
}

#[test]
fn plain_win_tap_never_masks() {
    let mut m = HookClassify::new(true);
    let mut q = EventQueue::new();
    m.push(VK_LWIN, false, true, false);
    assert!(!reserve(&mut m, &mut q, VK_LWIN, true));
    assert!(q.is_empty());
    assert_eq!(
        (m.mask_attempted, m.mask_ok, m.mask_failed, m.mask_skipped),
        (0, 0, 0, 0)
    );
}

#[test]
fn consumed_down_arms_without_immediate_injection() {
    // Arming queues only the chord; no mask record exists until Win-up.
    let mut m = HookClassify::new(true);
    let mut q = EventQueue::new();
    m.push(VK_LWIN, false, true, false);
    assert_eq!(
        gated(&mut m, &mut q, VK_LEFT, false, true, false),
        Some(true)
    );
    assert_eq!(q.len(), 1);
    assert!(matches!(q.pop_front(), Some(QueuedEvent::Chord(_))));
}

#[test]
fn consumed_hold_fires_mask_once_at_win_up() {
    let mut m = HookClassify::new(true);
    let mut q = EventQueue::new();
    m.push(VK_LWIN, false, true, false);
    assert_eq!(
        gated(&mut m, &mut q, VK_LEFT, false, true, false),
        Some(true)
    );
    assert_eq!(
        gated(&mut m, &mut q, VK_LEFT, true, true, false),
        Some(true)
    );
    assert!(reserve(&mut m, &mut q, VK_LWIN, true));
    assert_eq!(m.mask_attempted, 1);
    stamp_mask_result(&mut m, &mut q, 2, false);
    assert_eq!((m.mask_ok, m.mask_failed), (1, 0));
    // Chord down, chord up, then the mask record carrying the trigger arrow.
    assert!(matches!(q.pop_front(), Some(QueuedEvent::Chord(_))));
    assert!(matches!(q.pop_front(), Some(QueuedEvent::Chord(_))));
    let mask = pop_mask(&mut q);
    assert_eq!(mask.trigger, Arrow::Left);
    assert_eq!((mask.inserted, mask.release_sent), (2, false));
    assert!(q.is_empty());
}

#[test]
fn win_repeat_preserves_pending_mask() {
    let mut m = HookClassify::new(true);
    let mut q = EventQueue::new();
    m.push(VK_LWIN, false, true, false);
    assert_eq!(
        gated(&mut m, &mut q, VK_LEFT, false, true, false),
        Some(true)
    );
    m.push(VK_LWIN, false, true, false);
    assert!(reserve(&mut m, &mut q, VK_LWIN, true));
}

#[test]
fn lr_pair_masks_once_without_repeat() {
    let mut m = HookClassify::new(true);
    let mut q = EventQueue::new();
    m.push(VK_LWIN, false, true, false);
    m.push(VK_RWIN, false, true, false);
    assert_eq!(
        gated(&mut m, &mut q, VK_LEFT, false, true, false),
        Some(true)
    );
    assert!(reserve(&mut m, &mut q, VK_LWIN, true));
    assert!(!reserve(&mut m, &mut q, VK_RWIN, true));
    assert_eq!((m.mask_attempted, m.mask_skipped), (1, 0));
}

#[test]
fn naked_repeat_after_mask_rearms() {
    let mut m = HookClassify::new(true);
    let mut q = EventQueue::new();
    m.push(VK_LWIN, false, true, false);
    m.push(VK_RWIN, false, true, false);
    assert_eq!(
        gated(&mut m, &mut q, VK_LEFT, false, true, false),
        Some(true)
    );
    assert!(reserve(&mut m, &mut q, VK_LWIN, true));
    m.push(VK_RWIN, false, true, false);
    assert!(reserve(&mut m, &mut q, VK_RWIN, true));
    assert_eq!(m.mask_attempted, 2);
}

#[test]
fn uninstalled_win_up_never_fires() {
    let mut m = HookClassify::new(true);
    let mut q = EventQueue::new();
    m.push(VK_LWIN, false, true, false);
    assert_eq!(
        gated(&mut m, &mut q, VK_LEFT, false, true, false),
        Some(true)
    );
    assert!(!reserve(&mut m, &mut q, VK_LWIN, false));
    assert_eq!(q.len(), 1, "only the chord is queued");
    assert_eq!((m.mask_attempted, m.mask_skipped), (0, 0));
}

#[test]
fn background_extra_mods_injected_never_arm() {
    // Background chord passes: no arm.
    let mut m = HookClassify::new(true);
    let mut q = EventQueue::new();
    m.push(VK_LWIN, false, false, false);
    assert_eq!(
        gated(&mut m, &mut q, VK_LEFT, false, false, false),
        Some(false)
    );
    assert!(!reserve(&mut m, &mut q, VK_LWIN, true));
    // Extra modifier chord is untracked: no arm.
    let mut m = HookClassify::new(true);
    let mut q = EventQueue::new();
    m.push(VK_LWIN, false, true, false);
    m.push(VK_CONTROL, false, true, false);
    assert_eq!(m.push(VK_LEFT, false, true, false), None);
    m.push(VK_CONTROL, true, true, false);
    assert!(!reserve(&mut m, &mut q, VK_LWIN, true));
    // Injected chord is ignored: no arm.
    let mut m = HookClassify::new(true);
    let mut q = EventQueue::new();
    m.push(VK_LWIN, false, true, false);
    assert_eq!(m.push(VK_LEFT, false, true, true), None);
    assert!(!reserve(&mut m, &mut q, VK_LWIN, true));
    assert!(q.is_empty());
}

#[test]
fn paired_arrow_up_keeps_pending_armed() {
    let mut m = HookClassify::new(true);
    let mut q = EventQueue::new();
    m.push(VK_LWIN, false, true, false);
    assert_eq!(
        gated(&mut m, &mut q, VK_LEFT, false, true, false),
        Some(true)
    );
    let up = m.push(VK_LEFT, true, true, false).expect("paired up");
    assert!(up.consumed);
    assert!(reserve(&mut m, &mut q, VK_LWIN, true));
}

#[test]
fn ordinary_and_passed_input_disarms_pending() {
    // An ordinary key reaches the OS and disguises Win by itself.
    let mut m = HookClassify::new(true);
    let mut q = EventQueue::new();
    m.push(VK_LWIN, false, true, false);
    assert_eq!(
        gated(&mut m, &mut q, VK_LEFT, false, true, false),
        Some(true)
    );
    assert_eq!(m.push(65, false, true, false), None);
    assert!(!reserve(&mut m, &mut q, VK_LWIN, true));
    // A passed (background) up also reaches the OS.
    let mut m = HookClassify::new(true);
    let mut q = EventQueue::new();
    m.push(VK_LWIN, false, true, false);
    assert_eq!(
        gated(&mut m, &mut q, VK_LEFT, false, true, false),
        Some(true)
    );
    let up = m.push(VK_LEFT, true, false, false).expect("logged");
    assert!(!up.consumed);
    assert!(!reserve(&mut m, &mut q, VK_LWIN, true));
}

#[test]
fn full_queue_injects_nothing_and_counts_skip() {
    let mut m = HookClassify::new(true);
    let mut q = EventQueue::new();
    m.push(VK_LWIN, false, true, false);
    m.push(VK_RWIN, false, true, false);
    m.push(VK_LEFT, false, true, false);
    while q.len() < EVENT_QUEUE_CAP {
        assert!(q.push(QueuedEvent::Chord(QueuedChord {
            arrow: Arrow::Right,
            edge: Edge::Down,
            foreground: true,
            consumed: true,
            announce: true,
            tick: std::time::Instant::now(),
        })));
    }
    assert!(!reserve(&mut m, &mut q, VK_LWIN, true));
    assert_eq!((m.mask_attempted, m.mask_skipped), (0, 1));
    assert_eq!(q.dropped, 1);
    // The hold is intact, so a later Win-up retries once room drains.
    assert!(q.pop_front().is_some());
    assert!(reserve(&mut m, &mut q, VK_RWIN, true));
    assert_eq!(m.mask_attempted, 1);
}

#[test]
fn short_send_never_claims_success() {
    let mut m = HookClassify::new(true);
    let mut q = EventQueue::new();
    m.push(VK_LWIN, false, true, false);
    assert_eq!(
        gated(&mut m, &mut q, VK_LEFT, false, true, false),
        Some(true)
    );
    assert!(reserve(&mut m, &mut q, VK_LWIN, true));
    stamp_mask_result(&mut m, &mut q, 0, false);
    assert_eq!((m.mask_ok, m.mask_failed), (0, 1));
    assert!(matches!(q.pop_front(), Some(QueuedEvent::Chord(_))));
    let mask = pop_mask(&mut q);
    assert_eq!((mask.inserted, mask.release_sent), (0, false));
    // Partial insert plus the single bounded E8-up release still counts failed.
    let mut m = HookClassify::new(true);
    let mut q = EventQueue::new();
    m.push(VK_LWIN, false, true, false);
    assert_eq!(
        gated(&mut m, &mut q, VK_LEFT, false, true, false),
        Some(true)
    );
    assert!(reserve(&mut m, &mut q, VK_LWIN, true));
    stamp_mask_result(&mut m, &mut q, 1, true);
    assert_eq!((m.mask_ok, m.mask_failed), (0, 1));
    assert!(matches!(q.pop_front(), Some(QueuedEvent::Chord(_))));
    let mask = pop_mask(&mut q);
    assert_eq!((mask.inserted, mask.release_sent), (1, true));
}

#[test]
fn hook_report_shape_has_plain_counts() {
    let hook = new_hook_method();
    assert_eq!(hook.method, "hook");
    assert_eq!(hook.queue_dropped, 0);
    assert_eq!(hook.events_logged, 0);
    assert!(hook.events_path.is_none());
    let json = serde_json::to_value(&hook).expect("json");
    assert!(json.get("raw_unavailable").is_none());
    assert!(json.get("post_failures").is_none());
    let chord = &json["chords"][0];
    assert!(chord.get("setup_ok").is_none());
    assert_eq!(chord["down"], 0);
    assert_eq!(chord["delivered"], 0);
    assert_eq!(chord["skipped"], 0);
    assert!(json["hook_installed"].is_null());
    assert_eq!(hook.mask_attempted, 0);
    assert_eq!(hook.mask_ok, 0);
    assert_eq!(hook.mask_failed, 0);
    assert_eq!(hook.mask_skipped, 0);
    assert_eq!(json["mask_attempted"], 0);
    assert_eq!(json["mask_ok"], 0);
    assert_eq!(json["mask_failed"], 0);
    assert_eq!(json["mask_skipped"], 0);
}

#[test]
fn method_state_wire_names() {
    let mut hook = new_hook_method();
    let json = serde_json::to_value(&hook).expect("json");
    assert_eq!(json["state"], "enabled");
    assert_eq!(json["method"], "hook");
    hook.state = tiler_windows::winarrow::MethodState::ReleaseCheck;
    let json = serde_json::to_value(&hook).expect("json");
    assert_eq!(json["state"], "release-check");
    hook.state = tiler_windows::winarrow::MethodState::Done;
    let json = serde_json::to_value(&hook).expect("json");
    assert_eq!(json["state"], "done");
}

#[test]
fn consumed_down_helper_covers_all_chords() {
    for arrow in Arrow::all() {
        let mut m = HookClassify::new(true);
        consumed_down(&mut m, VK_LWIN, arrow.vk());
        let idx = arrow as usize;
        assert_eq!(m.chords[idx].down, 1);
    }
}

#[test]
fn events_path_is_sibling_jsonl() {
    let report = std::path::Path::new("run/hook.json");
    assert_eq!(
        events_path_for(report),
        Some(std::path::PathBuf::from("run/events.jsonl"))
    );
    assert_eq!(
        events_path_for(std::path::Path::new("hook.json")),
        Some(std::path::PathBuf::from("events.jsonl"))
    );
}

#[test]
fn spike_run_arg_bounds() {
    let opts = parse_spike_run_args(&strings(&["--helper-hwnd", "123", "--report", "r.json"]))
        .expect("parsed");
    assert_eq!(opts.helper_hwnd, 123);
    assert!(parse_spike_run_args(&strings(&[])).is_err());
    assert!(parse_spike_run_args(&strings(&["--helper-hwnd", "0", "--report", "r.json"])).is_err());
    assert!(parse_spike_run_args(&strings(&["--helper-hwnd", "x", "--report", "r.json"])).is_err());
    assert!(parse_spike_run_args(&strings(&["--helper-hwnd", "123"])).is_err());
    assert!(parse_spike_run_args(&strings(&["--bogus"])).is_err());
    assert!(
        parse_spike_run_args(&strings(&[
            "--method",
            "hook",
            "--helper-hwnd",
            "123",
            "--report",
            "r.json"
        ]))
        .is_err(),
        "method flag is removed"
    );
    assert!(
        parse_spike_run_args(&strings(&["--report", "r.json"])).is_err(),
        "hwnd is required"
    );
}

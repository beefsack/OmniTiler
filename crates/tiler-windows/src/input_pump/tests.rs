#![forbid(unsafe_code)]
use super::{
    FakeBackend, InboxSender, SAFETY_INVALIDATES_CAP, install_backoff_secs, snap_batch_age_us,
};
use crate::input_handoff::{
    EscClock, Inbox, InboxItem, InputSnapshot, OUTBOX_BATCH_CAP, Outbox, PumpLifecycle,
    TaggedBatch, snapshot_content_eq, snapshot_policy_eq,
};
use crate::snapkey::{
    INTENT_QUEUE_CAP, KeyboardConfig, QueuedIntent, QueuedSnapEvent, SnapClassify, SnapEdge,
    SnapOp, SnapQueue, VK_H, VK_LWIN, classify_and_queue, win_up_mask_reserve,
};
use crate::win_mouse::{
    UpOutcome, WINDRAG_QUEUE_CAP, WinDragEdge, WinDragKind, WinDragQueue, WinHookArm,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tiler_core::directional::Direction;

#[test]
fn install_backoff_doubles_to_sixty() {
    assert_eq!(install_backoff_secs(0), 5);
    assert_eq!(install_backoff_secs(1), 10);
    assert_eq!(install_backoff_secs(2), 20);
    assert_eq!(install_backoff_secs(3), 40);
    assert_eq!(install_backoff_secs(4), 60);
    assert_eq!(install_backoff_secs(9), 60);
}

#[test]
fn fake_backend_scripts_install_outcomes() {
    // Scripted outcomes plus the shared backoff vocabulary; the
    // production step drives these through `NativeHooks` (covered in
    // `core_tests`).
    let mut backend = FakeBackend::new(vec![false, false, true], vec![true], true);
    assert_eq!(backend.keyboard_script.len(), 3);
    assert_eq!(backend.mouse_script.len(), 1);
    assert!(backend.unhook_script.is_empty());
    assert_eq!(backend.note_retry(1), 10);
    assert_eq!(backend.note_retry(2), 20);
    assert_eq!(backend.retry_secs, vec![10, 20]);
}

#[test]
fn sender_fault_closes_admission_and_retries_in_order() {
    let inbox = Arc::new(Inbox::new());
    let mut sender = InboxSender::new(Arc::clone(&inbox));
    for _ in 0..crate::input_handoff::INBOX_CAP {
        sender.offer(InboxItem::ClearEsc).expect("fits");
    }
    sender.flush();
    // Inbox full now: the next offer faults closed but is preserved.
    sender
        .offer(InboxItem::InvalidateWindrag(7))
        .expect("buffered");
    assert!(sender.admission_closed);
    assert_eq!(sender.pending_len(), 1);
    // Safety obligations still merge while closed (lossless stall);
    // snapshots refuse for rebuild-and-retry later.
    sender.offer(InboxItem::ClearEsc).expect("slotted");
    assert_eq!(sender.safety_len(), 1);
    let rebuild = InputSnapshot {
        generation: 0,
        keyboard: KeyboardConfig {
            takeover: true,
            allow_win_l: false,
        },
        remap: Vec::new(),
        disabled: Vec::new(),
        snap_origins: HashMap::new(),
        windrag_origins: HashMap::new(),
        active: true,
        trace: false,
    };
    assert!(
        sender
            .offer(InboxItem::Snapshot(Arc::new(rebuild)))
            .is_err()
    );
    // Owner drains one slot, flush retries the preserved control first,
    // then the slotted suspend-clear behind it.
    assert_eq!(inbox.drain().len(), crate::input_handoff::INBOX_CAP);
    assert_eq!(sender.flush(), 0);
    assert!(!sender.admission_closed);
    assert_eq!(
        inbox.drain(),
        vec![InboxItem::InvalidateWindrag(7), InboxItem::ClearEsc]
    );
}

#[test]
fn sender_collapses_exact_duplicate_controls() {
    let inbox = Arc::new(Inbox::new());
    let mut sender = InboxSender::new(Arc::clone(&inbox));
    for _ in 0..crate::input_handoff::INBOX_CAP {
        sender.offer(InboxItem::ClearEsc).expect("fits");
    }
    sender
        .offer(InboxItem::InvalidateWindrag(7))
        .expect("buffered");
    // Same control again collapses (preserved once, counted) even while
    // admission is closed.
    sender
        .offer(InboxItem::InvalidateWindrag(7))
        .expect("deduped");
    assert_eq!(sender.pending_len(), 1);
    assert_eq!(sender.deduped, 1);
    // A different control while closed merges into the safety slots
    // (retained, never refused).
    sender
        .offer(InboxItem::InvalidateWindrag(9))
        .expect("slotted");
    assert_eq!(sender.pending_len(), 1);
    assert_eq!(sender.safety_len(), 1);
    // After the pump drains, the held control retries first and the
    // slotted one expands behind it in order.
    assert_eq!(inbox.drain().len(), crate::input_handoff::INBOX_CAP);
    assert_eq!(sender.flush(), 0);
    assert!(!sender.admission_closed);
    assert_eq!(
        inbox.drain(),
        vec![
            InboxItem::InvalidateWindrag(7),
            InboxItem::InvalidateWindrag(9)
        ]
    );
}

#[test]
fn sender_retains_distinct_safety_controls_under_stall() {
    // Stalled full transport keeps every distinct invalidation plus the
    // suspend-clear obligations: inbox full, pending held, slots merged,
    // all delivered in order once the pump drains.
    let inbox = Arc::new(Inbox::new());
    let mut sender = InboxSender::new(Arc::clone(&inbox));
    for hwnd in 100..108u64 {
        sender
            .offer(InboxItem::InvalidateWindrag(hwnd))
            .expect("fits inbox");
    }
    sender
        .offer(InboxItem::InvalidateWindrag(200))
        .expect("buffered pending");
    assert!(sender.admission_closed);
    sender
        .offer(InboxItem::InvalidateWindrag(7))
        .expect("slotted");
    sender
        .offer(InboxItem::InvalidateWindrag(9))
        .expect("slotted");
    sender.offer(InboxItem::ClearEsc).expect("slotted");
    // Slot duplicates collapse without losing the obligation.
    sender
        .offer(InboxItem::InvalidateWindrag(7))
        .expect("deduped");
    assert_eq!(sender.deduped, 1);
    assert_eq!(sender.safety_len(), 3);
    assert_eq!(inbox.drain().len(), crate::input_handoff::INBOX_CAP);
    assert_eq!(sender.flush(), 0);
    assert!(!sender.admission_closed);
    assert_eq!(
        inbox.drain(),
        vec![
            InboxItem::InvalidateWindrag(200),
            InboxItem::InvalidateWindrag(7),
            InboxItem::InvalidateWindrag(9),
            InboxItem::ClearEsc,
        ]
    );
}

#[test]
fn sender_escalates_to_clear_beyond_fixed_cap() {
    // Far more distinct invalidations than the fixed cap while stalled:
    // every offer absorbs (`Ok`), slot state stays bounded, and the
    // expansion carries the conservative clear (a proven superset of
    // every invalidation) instead of an unbounded HWND list.
    let inbox = Arc::new(Inbox::new());
    let mut sender = InboxSender::new(Arc::clone(&inbox));
    for hwnd in 100..108u64 {
        sender
            .offer(InboxItem::InvalidateWindrag(hwnd))
            .expect("fits inbox");
    }
    sender
        .offer(InboxItem::InvalidateWindrag(200))
        .expect("buffered pending");
    assert!(sender.admission_closed);
    for hwnd in 1000..1040u64 {
        sender
            .offer(InboxItem::InvalidateWindrag(hwnd))
            .expect("absorbed");
        assert!(sender.safety_len() <= SAFETY_INVALIDATES_CAP + 2);
    }
    assert!(sender.invalidates.len() <= SAFETY_INVALIDATES_CAP);
    assert_eq!(inbox.drain().len(), crate::input_handoff::INBOX_CAP);
    assert_eq!(sender.flush(), 0);
    assert!(!sender.admission_closed);
    // Pending first, then the escalated clear, then at most a cap of
    // trailing invalidates queued after the last escalation. Exact
    // shape: 40 offers fill 8, escalate, fill 8, escalate, fill 8,
    // escalate, fill 8, escalate, then 4 remain.
    assert_eq!(
        inbox.drain(),
        vec![
            InboxItem::InvalidateWindrag(200),
            InboxItem::ClearWindrag,
            InboxItem::InvalidateWindrag(1036),
            InboxItem::InvalidateWindrag(1037),
            InboxItem::InvalidateWindrag(1038),
            InboxItem::InvalidateWindrag(1039),
        ]
    );
}

#[test]
fn sender_clear_windrag_subsumes_older_slot_invalidates() {
    // A stalled `ClearWindrag` subsumes older slotted invalidates (every
    // gesture already disarmed with every origin dropped); a newer
    // invalidate behind the clear is still retained.
    let inbox = Arc::new(Inbox::new());
    let mut sender = InboxSender::new(Arc::clone(&inbox));
    for hwnd in 100..108u64 {
        sender
            .offer(InboxItem::InvalidateWindrag(hwnd))
            .expect("fits inbox");
    }
    sender
        .offer(InboxItem::InvalidateWindrag(200))
        .expect("buffered pending");
    sender
        .offer(InboxItem::InvalidateWindrag(7))
        .expect("slotted");
    sender
        .offer(InboxItem::InvalidateWindrag(9))
        .expect("slotted");
    sender.offer(InboxItem::ClearWindrag).expect("slotted");
    sender
        .offer(InboxItem::InvalidateWindrag(11))
        .expect("slotted");
    // Duplicate clears collapse.
    sender.offer(InboxItem::ClearWindrag).expect("deduped");
    assert_eq!(inbox.drain().len(), crate::input_handoff::INBOX_CAP);
    assert_eq!(sender.flush(), 0);
    assert_eq!(
        inbox.drain(),
        vec![
            InboxItem::InvalidateWindrag(200),
            InboxItem::ClearWindrag,
            InboxItem::InvalidateWindrag(11),
        ]
    );
}

#[test]
fn snapshot_content_and_policy_compare_without_generation() {
    let base = InputSnapshot {
        generation: 1,
        keyboard: KeyboardConfig {
            takeover: true,
            allow_win_l: false,
        },
        remap: Vec::new(),
        disabled: Vec::new(),
        snap_origins: HashMap::new(),
        windrag_origins: HashMap::new(),
        active: true,
        trace: false,
    };
    // Generation alone never counts as a change.
    let mut renumbered = base.clone();
    renumbered.generation = 2;
    assert!(snapshot_content_eq(&base, &renumbered));
    assert!(snapshot_policy_eq(&base, &renumbered));
    // Map-only drift: content differs, policy identical (no queue flush).
    let mut maps = base.clone();
    maps.windrag_origins.insert(
        7,
        crate::input_handoff::WindragOrigin {
            origin: crate::snapkey::SnapOrigin {
                hwnd: 7,
                token: "t".to_owned(),
                pid: 1,
                creation: "c".to_owned(),
            },
            tag: None,
        },
    );
    assert!(!snapshot_content_eq(&base, &maps));
    assert!(snapshot_policy_eq(&base, &maps));
    // Policy drift: takeover off changes both (stale drain required).
    let mut policy = base.clone();
    policy.keyboard.takeover = false;
    policy.active = false;
    assert!(!snapshot_content_eq(&base, &policy));
    assert!(!snapshot_policy_eq(&base, &policy));
}

#[test]
fn sender_snapshot_invalidate_snapshot_stays_ordered() {
    let inbox = Arc::new(Inbox::new());
    let mut sender = InboxSender::new(Arc::clone(&inbox));
    let first = Arc::new(InputSnapshot {
        generation: 0,
        keyboard: KeyboardConfig {
            takeover: true,
            allow_win_l: false,
        },
        remap: Vec::new(),
        disabled: Vec::new(),
        snap_origins: HashMap::new(),
        windrag_origins: HashMap::new(),
        active: true,
        trace: false,
    });
    let second = Arc::new(InputSnapshot {
        generation: 0,
        keyboard: KeyboardConfig::disabled(),
        remap: Vec::new(),
        disabled: Vec::new(),
        snap_origins: HashMap::new(),
        windrag_origins: HashMap::new(),
        active: false,
        trace: false,
    });
    assert!(
        sender
            .offer(InboxItem::Snapshot(Arc::clone(&first)))
            .is_ok()
    );
    assert!(sender.offer(InboxItem::InvalidateWindrag(7)).is_ok());
    assert!(
        sender
            .offer(InboxItem::Snapshot(Arc::clone(&second)))
            .is_ok()
    );
    sender.flush();
    let drained = inbox.drain();
    assert_eq!(drained.len(), 3);
    assert!(matches!(drained[0], InboxItem::Snapshot(_)));
    assert_eq!(drained[1], InboxItem::InvalidateWindrag(7));
    assert!(matches!(drained[2], InboxItem::Snapshot(_)));
}

#[test]
fn failed_stop_never_frees_the_worker_slot() {
    let mut life = PumpLifecycle::new();
    life.start().expect("start");
    life.note_installed().expect("ready");
    life.request_stop().expect("stop");
    let report = life.note_unhooked(false).expect("ack");
    assert!(!report.clean);
    assert!(life.start().is_err());
}

#[test]
fn mask_reentry_fires_after_config_change_without_rebind() {
    // Both hooks stay together: a consumed windrag Down arms the shared
    // keyboard mask; a config change between Down and Win-up must not
    // drop the mask obligation or rebind the held pair.
    let mut machine = SnapClassify::new(KeyboardConfig {
        takeover: true,
        allow_win_l: false,
    });
    machine.set_gate_active(true);
    let mut queue = SnapQueue::new();
    assert_eq!(
        SnapClassify::push(&mut machine, VK_LWIN, false, true, false),
        None
    );
    machine.note_windrag_consumed();
    assert_eq!(
        classify_and_queue(
            &mut machine,
            &mut queue,
            VK_H,
            false,
            None,
            false,
            Instant::now(),
            true
        ),
        Some(true)
    );
    // Ordered config apply between Down and Win-up (input pump FIFO):
    // stale actions drain, held verdicts survive.
    machine.set_enabled(true);
    machine.set_remap(Vec::new());
    machine.set_disabled(Vec::new());
    queue.drain_stale();
    assert!(win_up_mask_reserve(
        &mut machine,
        &mut queue,
        VK_LWIN,
        true,
        Instant::now()
    ));
    assert_eq!(
        classify_and_queue(
            &mut machine,
            &mut queue,
            VK_H,
            true,
            None,
            false,
            Instant::now(),
            true
        ),
        Some(true)
    );
}

#[test]
fn saturated_down_passes_and_terminal_up_survives_outbox_stall() {
    // Saturated new Down passes with nothing armed; the terminal Up of a
    // consumed gesture keeps guaranteed delivery end to end even while
    // the outbox is occupied.
    let queue = &mut WinDragQueue::new();
    for n in 0..WINDRAG_QUEUE_CAP {
        assert!(queue.push(WinDragEdge {
            hwnd: n as u64,
            kind: WinDragKind::Down,
            x: 0,
            y: 0,
            esc_seq: 0,
            snapshot: None,
        }));
    }
    // Saturated Down: pass-through, no arm, counted loss.
    assert!(!queue.push(WinDragEdge {
        hwnd: 999,
        kind: WinDragKind::Down,
        x: 0,
        y: 0,
        esc_seq: 0,
        snapshot: None,
    }));
    queue.note_drop();
    assert!(!WinHookArm::default().is_armed());
    // Consumed gesture elsewhere still closes with guaranteed delivery.
    let mut arm2 = WinHookArm::default();
    assert!(arm2.on_down(7, 1, 1, true, true));
    queue.push_terminal(WinDragEdge {
        hwnd: 7,
        kind: WinDragKind::Up,
        x: 2,
        y: 3,
        esc_seq: 1,
        snapshot: None,
    });
    let outbox: Outbox<TaggedBatch<WinDragEdge>> = Outbox::new();
    let mut first = Vec::new();
    while first.len() < OUTBOX_BATCH_CAP {
        first.push(queue.pop_front().expect("queued"));
    }
    outbox.put(TaggedBatch::new(2, first)).expect("offer");
    // Occupied: newer drains wait in the real queue; the terminal Up is
    // still queued behind, never stranded.
    let mut rest = Vec::new();
    while let Some(edge) = queue.pop_front() {
        rest.push(edge);
    }
    assert_eq!(
        rest.last(),
        Some(&WinDragEdge {
            hwnd: 7,
            kind: WinDragKind::Up,
            x: 2,
            y: 3,
            esc_seq: 1,
            snapshot: None,
        })
    );
    assert_eq!(
        arm2.on_up(2, 3),
        UpOutcome::Gesture(crate::win_mouse::WinBound {
            hwnd: 7,
            start_x: 1,
            start_y: 1,
            cur_x: 2,
            cur_y: 3,
        })
    );
}

#[test]
fn esc_clock_orders_start_end_without_batch_delay() {
    let clock = EscClock::new();
    let start = clock.load();
    assert!(!crate::input_handoff::esc_edge_cancels(start, clock.load()));
    clock.bump();
    assert!(crate::input_handoff::esc_edge_cancels(start, clock.load()));
    let end_sample = clock.load();
    clock.bump();
    assert!(!crate::input_handoff::esc_edge_cancels(start, start));
    assert!(crate::input_handoff::esc_edge_cancels(start, end_sample));
}

#[test]
fn snap_batch_age_uses_oldest_tick() {
    let now = Instant::now();
    let old = now - Duration::from_millis(120);
    let mk = |tick| {
        QueuedSnapEvent::Intent(QueuedIntent {
            op: SnapOp::Focus,
            direction: Direction::Left,
            edge: SnapEdge::Down,
            origin: None,
            consumed: true,
            announce: true,
            tick,
        })
    };
    let batch = vec![mk(now), mk(old)];
    let age = snap_batch_age_us(&batch, now);
    assert!((100_000..=500_000).contains(&age));
    assert_eq!(snap_batch_age_us(&[], now), 0);
}

#[test]
fn snap_saturation_keeps_mask_end_to_end() {
    let mut machine = SnapClassify::new(KeyboardConfig {
        takeover: true,
        allow_win_l: false,
    });
    machine.set_gate_active(true);
    let mut queue = SnapQueue::new();
    assert_eq!(
        SnapClassify::push(&mut machine, VK_LWIN, false, true, false),
        None
    );
    machine.note_windrag_consumed();
    for _ in 0..INTENT_QUEUE_CAP {
        assert!(queue.push(QueuedSnapEvent::Intent(QueuedIntent {
            op: SnapOp::Focus,
            direction: Direction::Left,
            edge: SnapEdge::Down,
            origin: None,
            consumed: true,
            announce: true,
            tick: Instant::now(),
        })));
    }
    assert!(win_up_mask_reserve(
        &mut machine,
        &mut queue,
        VK_LWIN,
        true,
        Instant::now()
    ));
    assert_eq!(queue.len(), INTENT_QUEUE_CAP);
    let outbox: Outbox<TaggedBatch<QueuedSnapEvent>> = Outbox::new();
    let mut batch = Vec::new();
    while batch.len() < OUTBOX_BATCH_CAP {
        batch.push(queue.pop_front().expect("queued"));
    }
    outbox.put(TaggedBatch::new(5, batch)).expect("offer");
    let taken = outbox.take().expect("take");
    assert_eq!(taken.generation, 5);
    assert!(!taken.is_stale(5));
    assert!(taken.is_stale(6));
}

#[test]
fn sender_counts_accepted_clears_for_owner_republish() {
    // Every accepted clear (direct, buffered, slotted, escalated) ticks
    // the owner republish trigger; only duplicate collapses never tick
    // it. Direct re-accepts are idempotent clears that force again
    // (bounded, harmless); the owner clears its force on snapshot
    // accept.
    let inbox = Arc::new(Inbox::new());
    let mut sender = InboxSender::new(Arc::clone(&inbox));
    assert_eq!(sender.windrag_cleared(), 0);
    sender.offer(InboxItem::ClearWindrag).expect("direct");
    assert_eq!(sender.windrag_cleared(), 1);
    sender.offer(InboxItem::ClearWindrag).expect("re-accepted");
    assert_eq!(sender.windrag_cleared(), 2);
    // Stall, then slot a clear twice: the second collapses (counted,
    // never ticking) while the first ticks.
    for _ in 0..crate::input_handoff::INBOX_CAP - 2 {
        sender.offer(InboxItem::ClearEsc).expect("fits");
    }
    sender
        .offer(InboxItem::InvalidateWindrag(200))
        .expect("buffered");
    assert!(sender.admission_closed);
    let before = sender.windrag_cleared();
    sender.offer(InboxItem::ClearWindrag).expect("slotted");
    assert_eq!(sender.windrag_cleared(), before + 1);
    sender.offer(InboxItem::ClearWindrag).expect("deduped");
    assert_eq!(sender.windrag_cleared(), before + 1);
    assert_eq!(sender.deduped, 1);
    // Overflow past the fixed cap: escalation ticks again.
    for hwnd in 1000..1040u64 {
        sender
            .offer(InboxItem::InvalidateWindrag(hwnd))
            .expect("absorbed");
    }
    assert!(sender.windrag_cleared() > before + 1);
    assert!(sender.invalidates.len() <= SAFETY_INVALIDATES_CAP);
}

#[test]
fn sender_preserves_newest_control_across_queued_snapshot() {
    // Pending [Inv7, Snap rearm7] plus a newer Inv7: the newer control
    // must be preserved (never collapsed), or the rearmed origin would
    // stay bound after its invalidation.
    let inbox = Arc::new(Inbox::new());
    let mut sender = InboxSender::new(Arc::clone(&inbox));
    for _ in 0..crate::input_handoff::INBOX_CAP {
        inbox.push(InboxItem::ClearEsc).expect("stall");
    }
    sender
        .offer(InboxItem::InvalidateWindrag(7))
        .expect("buffered");
    // Admission is closed with pending held; reopen it in-test to stage
    // the intervening snapshot behind the held control (the ordering
    // the pump applies), then offer the newer invalidation.
    sender.admission_closed = false;
    let snap = Arc::new(InputSnapshot {
        generation: 0,
        keyboard: KeyboardConfig {
            takeover: true,
            allow_win_l: false,
        },
        remap: Vec::new(),
        disabled: Vec::new(),
        snap_origins: HashMap::new(),
        windrag_origins: HashMap::new(),
        active: true,
        trace: false,
    });
    sender
        .offer(InboxItem::Snapshot(Arc::clone(&snap)))
        .expect("buffered behind");
    assert_eq!(sender.pending_len(), 2);
    sender
        .offer(InboxItem::InvalidateWindrag(7))
        .expect("newest preserved");
    assert_eq!(sender.deduped, 0);
    assert_eq!(sender.pending_len(), 3);
    assert_eq!(inbox.drain().len(), crate::input_handoff::INBOX_CAP);
    sender.flush();
    assert_eq!(
        inbox.drain(),
        vec![
            InboxItem::InvalidateWindrag(7),
            InboxItem::Snapshot(snap),
            InboxItem::InvalidateWindrag(7),
        ]
    );
}

#[test]
fn sender_preserves_slotted_control_across_inbox_snapshot() {
    // Inbox [Snap rearm7] ahead plus slotted Inv7 behind must not
    // collapse a newer Inv7: the inbox snapshot rebinds between them.
    let inbox = Arc::new(Inbox::new());
    let mut sender = InboxSender::new(Arc::clone(&inbox));
    let snap = Arc::new(InputSnapshot {
        generation: 0,
        keyboard: KeyboardConfig {
            takeover: true,
            allow_win_l: false,
        },
        remap: Vec::new(),
        disabled: Vec::new(),
        snap_origins: HashMap::new(),
        windrag_origins: HashMap::new(),
        active: true,
        trace: false,
    });
    sender
        .offer(InboxItem::Snapshot(Arc::clone(&snap)))
        .expect("queued");
    for _ in 0..crate::input_handoff::INBOX_CAP - 1 {
        sender.offer(InboxItem::ClearEsc).expect("fits");
    }
    sender
        .offer(InboxItem::InvalidateWindrag(7))
        .expect("buffered");
    assert!(sender.admission_closed);
    sender
        .offer(InboxItem::InvalidateWindrag(9))
        .expect("slotted");
    // Newer Inv7 across the queued snapshot: preserved, not deduped.
    sender
        .offer(InboxItem::InvalidateWindrag(7))
        .expect("newest preserved");
    assert_eq!(sender.deduped, 0);
    assert_eq!(inbox.drain().len(), crate::input_handoff::INBOX_CAP);
    assert_eq!(sender.flush(), 0);
    assert_eq!(
        inbox.drain(),
        vec![
            InboxItem::InvalidateWindrag(7),
            InboxItem::InvalidateWindrag(9),
            InboxItem::InvalidateWindrag(7),
        ]
    );
}

#[test]
fn esc_interleave_callback_end_settle_orders_by_sequence() {
    // F8 interleaving fixture, portable: an Esc callback edge followed
    // by a WinEvent END before any pump forwarding must still cancel the
    // ended gesture (END sample newer than START), while a post-END Esc
    // must not rewrite the settled comparison.
    use crate::input_handoff::esc_edge_cancels;
    use std::sync::atomic::Ordering;
    // Serial guard: the global edge store/swap races a concurrent pump
    // `ClearEsc` apply (which stores false).
    let _guard = crate::input_handoff::fence_test_serial();
    let clock = EscClock::new();
    let start = clock.load();
    // Callback Esc: bump then END samples the new sequence.
    clock.bump();
    let end_sample = clock.load();
    assert!(esc_edge_cancels(start, end_sample));
    // Settle consumes the edge flag; a later Esc bumps again but the
    // ended comparison still uses the END sample, never the live clock.
    crate::input_handoff::global_esc_edge().store(true, Ordering::SeqCst);
    assert!(crate::input_handoff::global_esc_edge().swap(false, Ordering::SeqCst));
    clock.bump();
    assert!(!esc_edge_cancels(start, start));
    assert!(esc_edge_cancels(start, end_sample));
    assert!(!esc_edge_cancels(end_sample, end_sample));
}

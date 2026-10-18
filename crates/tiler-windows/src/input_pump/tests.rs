#![forbid(unsafe_code)]
use super::{FakeBackend, InboxSender, install_backoff_secs, snap_batch_age_us};
use crate::input_handoff::{
    EscClock, Inbox, InboxItem, InputSnapshot, OUTBOX_BATCH_CAP, Outbox, PumpLifecycle,
    TaggedBatch, snapshot_content_eq, snapshot_policy_eq,
};
use crate::snapkey::{
    INTENT_QUEUE_CAP, KeyboardConfig, QueuedIntent, QueuedSnapEvent, SnapClassify, SnapEdge,
    SnapOp, SnapQueue, VK_H, VK_LWIN, classify_and_queue, win_up_mask_reserve,
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
    let mut backend = FakeBackend::new(vec![false, false, true], true);
    assert_eq!(backend.keyboard_script.len(), 3);
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
    // Inbox full now: the next snapshot buffers and closes admission.
    let first = InputSnapshot {
        generation: 0,
        keyboard: KeyboardConfig {
            takeover: true,
            allow_win_l: false,
        },
        remap: Vec::new(),
        disabled: Vec::new(),
        snap_origins: HashMap::new(),
        active: true,
        trace: false,
    };
    sender
        .offer(InboxItem::Snapshot(Arc::new(first)))
        .expect("buffered");
    assert!(sender.admission_closed);
    assert_eq!(sender.pending_len(), 1);
    // The suspend-clear obligation still merges while closed (lossless
    // stall); snapshots refuse for rebuild-and-retry later.
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
        active: true,
        trace: false,
    };
    assert!(
        sender
            .offer(InboxItem::Snapshot(Arc::new(rebuild)))
            .is_err()
    );
    // Owner drains one slot, flush retries the preserved snapshot first,
    // then the slotted suspend-clear behind it.
    assert_eq!(inbox.drain().len(), crate::input_handoff::INBOX_CAP);
    assert_eq!(sender.flush(), 0);
    assert!(!sender.admission_closed);
    let drained = inbox.drain();
    assert_eq!(drained.len(), 2);
    assert!(matches!(drained[0], InboxItem::Snapshot(_)));
    assert_eq!(drained[1], InboxItem::ClearEsc);
}

#[test]
fn sender_collapses_exact_duplicate_controls() {
    let inbox = Arc::new(Inbox::new());
    let mut sender = InboxSender::new(Arc::clone(&inbox));
    for _ in 0..crate::input_handoff::INBOX_CAP {
        sender.offer(InboxItem::ClearEsc).expect("fits");
    }
    sender.offer(InboxItem::ClearEsc).expect("buffered");
    // Same control again collapses (preserved once, counted) even while
    // admission is closed.
    sender.offer(InboxItem::ClearEsc).expect("deduped");
    assert_eq!(sender.pending_len(), 1);
    assert_eq!(sender.deduped, 1);
    // A snapshot while closed refuses for rebuild-and-retry later.
    let rebuild = InputSnapshot {
        generation: 0,
        keyboard: KeyboardConfig {
            takeover: true,
            allow_win_l: false,
        },
        remap: Vec::new(),
        disabled: Vec::new(),
        snap_origins: HashMap::new(),
        active: true,
        trace: false,
    };
    assert!(
        sender
            .offer(InboxItem::Snapshot(Arc::new(rebuild)))
            .is_err()
    );
    // After the pump drains, the held control retries and nothing
    // duplicates behind it.
    assert_eq!(inbox.drain().len(), crate::input_handoff::INBOX_CAP);
    assert_eq!(sender.flush(), 0);
    assert!(!sender.admission_closed);
    assert_eq!(inbox.drain(), vec![InboxItem::ClearEsc]);
}

#[test]
fn sender_retains_slotted_esc_clear_under_stall() {
    // Stalled full transport keeps the suspend-clear obligation:
    // inbox full, pending held, slot merged, all delivered in order
    // once the pump drains.
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
        active: true,
        trace: false,
    });
    sender
        .offer(InboxItem::Snapshot(Arc::clone(&snap)))
        .expect("fits inbox");
    for _ in 0..crate::input_handoff::INBOX_CAP - 1 {
        sender.offer(InboxItem::ClearEsc).expect("fits inbox");
    }
    sender.offer(InboxItem::ClearEsc).expect("buffered pending");
    assert!(sender.admission_closed);
    // A second clear while stalled merges into the safety slot
    // (retained, never refused): the queued snapshot between the two
    // clears means the newer one is a fresh obligation, never a
    // duplicate collapse.
    sender.offer(InboxItem::ClearEsc).expect("slotted");
    assert_eq!(sender.deduped, 0);
    assert_eq!(sender.safety_len(), 1);
    assert_eq!(inbox.drain().len(), crate::input_handoff::INBOX_CAP);
    assert_eq!(sender.flush(), 0);
    assert!(!sender.admission_closed);
    assert_eq!(
        inbox.drain(),
        vec![InboxItem::ClearEsc, InboxItem::ClearEsc]
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
    maps.snap_origins.insert(
        7,
        crate::snapkey::SnapOrigin {
            hwnd: 7,
            token: "t".to_owned(),
            pid: 1,
            creation: "c".to_owned(),
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
fn sender_snapshot_snapshot_stays_ordered() {
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
        active: true,
        trace: false,
    });
    let second = Arc::new(InputSnapshot {
        generation: 0,
        keyboard: KeyboardConfig::disabled(),
        remap: Vec::new(),
        disabled: Vec::new(),
        snap_origins: HashMap::new(),
        active: false,
        trace: false,
    });
    assert!(
        sender
            .offer(InboxItem::Snapshot(Arc::clone(&first)))
            .is_ok()
    );
    assert!(sender.offer(InboxItem::ClearEsc).is_ok());
    assert!(
        sender
            .offer(InboxItem::Snapshot(Arc::clone(&second)))
            .is_ok()
    );
    sender.flush();
    let drained = inbox.drain();
    assert_eq!(drained.len(), 3);
    assert!(matches!(drained[0], InboxItem::Snapshot(_)));
    assert_eq!(drained[1], InboxItem::ClearEsc);
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
    // A consumed chord arms the Win-up mask; a config change between
    // chord Down and Win-up must not drop the mask obligation or
    // rebind the held pair.
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
    // Drain the chord event: only the mask obligation must survive.
    while queue.pop_front().is_some() {}
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
    // Consumed chord arms the Win-up mask obligation for this hold.
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
    // Drain the chord event: only the mask obligation must survive.
    while queue.pop_front().is_some() {}
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
fn sender_preserves_newest_control_across_queued_snapshot() {
    // Pending [ClearEsc, Snap] plus a newer ClearEsc: the newer control
    // must be preserved (never collapsed), or the rearmed state would
    // stay cleared after its suspend edge.
    let inbox = Arc::new(Inbox::new());
    let mut sender = InboxSender::new(Arc::clone(&inbox));
    for _ in 0..crate::input_handoff::INBOX_CAP {
        inbox.push(InboxItem::ClearEsc).expect("stall");
    }
    sender.offer(InboxItem::ClearEsc).expect("buffered");
    // Admission is closed with pending held; reopen it in-test to stage
    // the intervening snapshot behind the held control (the ordering
    // the pump applies), then offer the newer control.
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
        active: true,
        trace: false,
    });
    sender
        .offer(InboxItem::Snapshot(Arc::clone(&snap)))
        .expect("buffered behind");
    assert_eq!(sender.pending_len(), 2);
    sender.offer(InboxItem::ClearEsc).expect("newest preserved");
    assert_eq!(sender.deduped, 0);
    assert_eq!(sender.pending_len(), 3);
    assert_eq!(inbox.drain().len(), crate::input_handoff::INBOX_CAP);
    sender.flush();
    assert_eq!(
        inbox.drain(),
        vec![
            InboxItem::ClearEsc,
            InboxItem::Snapshot(snap),
            InboxItem::ClearEsc,
        ]
    );
}

#[test]
fn sender_preserves_slotted_control_across_inbox_snapshot() {
    // Inbox [Snap] ahead plus slotted ClearEsc behind must not
    // collapse a newer ClearEsc: the inbox snapshot rebinds between them.
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
        active: true,
        trace: false,
    });
    sender
        .offer(InboxItem::Snapshot(Arc::clone(&snap)))
        .expect("queued");
    for _ in 0..crate::input_handoff::INBOX_CAP - 1 {
        sender.offer(InboxItem::ClearEsc).expect("fits");
    }
    sender.offer(InboxItem::ClearEsc).expect("buffered");
    assert!(sender.admission_closed);
    // Newer ClearEsc across the queued snapshot: preserved, not deduped.
    sender.offer(InboxItem::ClearEsc).expect("newest preserved");
    assert_eq!(sender.deduped, 0);
    assert_eq!(inbox.drain().len(), crate::input_handoff::INBOX_CAP);
    assert_eq!(sender.flush(), 0);
    assert_eq!(
        inbox.drain(),
        vec![InboxItem::ClearEsc, InboxItem::ClearEsc,]
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

#![forbid(unsafe_code)]
use super::*;
use crate::input_handoff::{
    InboxItem, InputSnapshot, SharedSnapshot, TaggedBatch, WindragOrigin, global_esc_clock,
    global_windrag_esc_clock,
};
use crate::snapkey::{KeyboardConfig, SnapClassify, SnapOrigin};
use std::collections::HashMap;

fn test_channels() -> (Arc<InputChannels>, Arc<PumpSignals>) {
    let channels = Arc::new(InputChannels::new());
    let (tx, _rx) = sync_channel(16);
    (Arc::clone(&channels), Arc::new(PumpSignals::new(tx)))
}

fn snapshot_with(policy_generation: u64, takeover: bool) -> SharedSnapshot {
    Arc::new(InputSnapshot {
        generation: policy_generation,
        keyboard: KeyboardConfig {
            takeover,
            allow_win_l: false,
        },
        remap: Vec::new(),
        disabled: Vec::new(),
        snap_origins: HashMap::new(),
        windrag_origins: HashMap::new(),
        active: takeover,
        trace: false,
    })
}

#[test]
fn install_retry_reports_then_readies_on_both_hooks() {
    // Keyboard fails twice, mouse succeeds: no readiness until both
    // install; every failure is reported with its backoff.
    let (channels, signals) = test_channels();
    let backend = super::super::FakeBackend::new(vec![false, false, true], vec![true], true);
    let mut core = PumpCore::new(
        backend,
        Arc::clone(&channels),
        Arc::clone(&signals),
        KeyboardConfig {
            takeover: true,
            allow_win_l: false,
        },
    );
    // First turn: both install attempts run (keyboard fails).
    assert!(!core.step());
    assert!(!signals.is_ready());
    assert_eq!(core.keyboard_failures, 1);
    assert!(core.mouse_hook.is_some());
    // Backoff (5s) blocks immediate retry: force due.
    core.keyboard_retry_at = Some(Instant::now());
    assert!(!core.step());
    assert_eq!(core.keyboard_failures, 2);
    core.keyboard_retry_at = Some(Instant::now());
    assert!(!core.step());
    assert!(core.keyboard_hook.is_some());
    assert!(signals.is_ready());
}

#[test]
fn retry_uses_latest_snapshot_policy_not_spawn_config() {
    // A policy snapshot arriving while install keeps failing must be
    // the config of the eventual install, never the stale spawn init.
    // Serial guard: the step applies generations to the
    // process-global fence, which races guarded fence tests.
    let _guard = crate::input_handoff::fence_test_serial();
    let (channels, signals) = test_channels();
    let backend = super::super::FakeBackend::new(vec![false, true], vec![true], true);
    let mut core = PumpCore::new(
        backend,
        Arc::clone(&channels),
        Arc::clone(&signals),
        KeyboardConfig::disabled(),
    );
    assert!(!core.step());
    assert!(core.keyboard_hook.is_none());
    channels
        .inbox
        .push(InboxItem::Snapshot(snapshot_with(1, true)))
        .expect("fits");
    core.keyboard_retry_at = Some(Instant::now());
    assert!(!core.step());
    assert!(core.keyboard_hook.is_some());
    assert!(core.backend.keyboard_configs.contains(&KeyboardConfig {
        takeover: true,
        allow_win_l: false,
    }));
}

#[test]
fn inbox_controls_apply_in_fifo_order() {
    // Snapshot, invalidate, snapshot through the real step with
    // hook-free TLS fixtures (no hooks, no native calls): the middle
    // control must apply between the two policies. Serial guard:
    // the step applies generations to the process-global fence.
    let _guard = crate::input_handoff::fence_test_serial();
    crate::snapkey::sys::test_ensure_state(KeyboardConfig {
        takeover: true,
        allow_win_l: false,
    });
    crate::win_mouse::sys::test_ensure_state();
    let (channels, signals) = test_channels();
    let backend = super::super::FakeBackend::new(vec![true], vec![true], true);
    let mut core = PumpCore::new(
        backend,
        Arc::clone(&channels),
        Arc::clone(&signals),
        KeyboardConfig {
            takeover: true,
            allow_win_l: false,
        },
    );
    assert!(!core.step());
    let mut first = snapshot_with(1, true);
    Arc::get_mut(&mut first)
        .expect("unique")
        .snap_origins
        .insert(
            7,
            crate::snapkey::SnapOrigin {
                hwnd: 7,
                token: "t".to_owned(),
                pid: 1,
                creation: "c".to_owned(),
            },
        );
    channels
        .inbox
        .push(InboxItem::Snapshot(first))
        .expect("fits");
    channels
        .inbox
        .push(InboxItem::InvalidateWindrag(7))
        .expect("fits");
    channels
        .inbox
        .push(InboxItem::Snapshot(snapshot_with(2, false)))
        .expect("fits");
    assert!(!core.step());
    // Applied keyboard origins lost hwnd 7 while the second
    // snapshot's changed policy (takeover off) applied; the mouse
    // snapshot dropped it too.
    assert!(!crate::snapkey::sys::test_keyboard_origins().contains_key(&7));
    assert!(
        !crate::win_mouse::sys::test_snapshot()
            .windrag_origins
            .contains_key(&7)
    );
    // Policy generations advanced in order; the invalidated origin
    // never rebound through the later snapshot.
    assert_eq!(
        channels
            .applied_policy_gen
            .load(std::sync::atomic::Ordering::SeqCst),
        2
    );
    crate::snapkey::sys::test_clear_state();
    crate::win_mouse::sys::test_clear_state();
}

#[test]
fn retained_snapshot_reapplies_after_initial_install() {
    // Snapshot applied while TLS is absent (install pending), then
    // both hooks install through the real step: the retained snapshot
    // must restore origins/gates onto the fresh empty hook state.
    // Without the reapply the installs would leave empty state with
    // change-gated publication never republishing it. Serial guard:
    // the step applies generations to the process-global fence.
    let _guard = crate::input_handoff::fence_test_serial();
    crate::snapkey::sys::test_clear_state();
    crate::win_mouse::sys::test_clear_state();
    let (channels, signals) = test_channels();
    let backend = super::super::FakeBackend::new(vec![true], vec![true], true);
    let mut core = PumpCore::new(
        backend,
        Arc::clone(&channels),
        Arc::clone(&signals),
        KeyboardConfig::disabled(),
    );
    let mut snap = snapshot_with(11, true);
    Arc::get_mut(&mut snap)
        .expect("unique")
        .snap_origins
        .insert(
            7,
            SnapOrigin {
                hwnd: 7,
                token: "t".to_owned(),
                pid: 1,
                creation: "c".to_owned(),
            },
        );
    Arc::get_mut(&mut snap)
        .expect("unique")
        .windrag_origins
        .insert(
            7,
            WindragOrigin {
                origin: SnapOrigin {
                    hwnd: 7,
                    token: "t".to_owned(),
                    pid: 1,
                    creation: "c".to_owned(),
                },
                tag: Some("tag-7".to_owned()),
            },
        );
    channels
        .inbox
        .push(InboxItem::Snapshot(snap))
        .expect("fits");
    assert!(!core.step());
    assert!(core.keyboard_hook.is_some());
    assert!(core.mouse_hook.is_some());
    // The install used the retained policy, not the stale spawn init.
    assert!(core.backend.keyboard_configs.contains(&KeyboardConfig {
        takeover: true,
        allow_win_l: false,
    }));
    assert!(crate::snapkey::sys::test_keyboard_origins().contains_key(&7));
    let mouse = crate::win_mouse::sys::test_snapshot();
    assert!(mouse.windrag_origins.contains_key(&7));
    assert!(mouse.active);
    assert_eq!(
        channels
            .applied_policy_gen
            .load(std::sync::atomic::Ordering::SeqCst),
        11
    );
    crate::snapkey::sys::test_clear_state();
    crate::win_mouse::sys::test_clear_state();
}

#[test]
fn retained_snapshot_reapplies_after_partial_install_failure() {
    // Keyboard installs while mouse keeps failing; the later mouse
    // recovery through the real step must still restore the retained
    // mouse snapshot (origins plus gate). Serial guard: the step
    // applies generations to the process-global fence.
    let _guard = crate::input_handoff::fence_test_serial();
    crate::snapkey::sys::test_clear_state();
    crate::win_mouse::sys::test_clear_state();
    let (channels, signals) = test_channels();
    let backend = super::super::FakeBackend::new(vec![true], vec![false, true], true);
    let mut core = PumpCore::new(
        backend,
        Arc::clone(&channels),
        Arc::clone(&signals),
        KeyboardConfig::disabled(),
    );
    let mut snap = snapshot_with(21, true);
    Arc::get_mut(&mut snap)
        .expect("unique")
        .windrag_origins
        .insert(
            7,
            WindragOrigin {
                origin: SnapOrigin {
                    hwnd: 7,
                    token: "t".to_owned(),
                    pid: 1,
                    creation: "c".to_owned(),
                },
                tag: Some("tag-7".to_owned()),
            },
        );
    channels
        .inbox
        .push(InboxItem::Snapshot(snap))
        .expect("fits");
    assert!(!core.step());
    assert!(core.keyboard_hook.is_some());
    assert!(core.mouse_hook.is_none());
    assert_eq!(core.mouse_failures, 1);
    // Mouse retry due now: recovery installs and reapplies.
    core.mouse_retry_at = Some(Instant::now());
    assert!(!core.step());
    assert!(core.mouse_hook.is_some());
    assert!(signals.is_ready());
    let mouse = crate::win_mouse::sys::test_snapshot();
    assert!(mouse.windrag_origins.contains_key(&7));
    assert!(mouse.active);
    crate::snapkey::sys::test_clear_state();
    crate::win_mouse::sys::test_clear_state();
}

#[test]
fn clear_forgets_retained_then_forced_same_snapshot_restores() {
    // Clear drops both hook origin maps and the retention; the
    // owner's forced republish of the UNCHANGED desired snapshot
    // (same content, same generation: no policy change, no bump)
    // restores origins through the real step. Without the force the
    // cleared origins would stay absent forever under change-gated
    // publication. Serial guard: the step applies generations to the
    // process-global fence.
    let _guard = crate::input_handoff::fence_test_serial();
    crate::snapkey::sys::test_clear_state();
    crate::win_mouse::sys::test_clear_state();
    let (channels, signals) = test_channels();
    let backend = super::super::FakeBackend::new(vec![true], vec![true], true);
    let mut core = PumpCore::new(
        backend,
        Arc::clone(&channels),
        Arc::clone(&signals),
        KeyboardConfig {
            takeover: true,
            allow_win_l: false,
        },
    );
    assert!(!core.step());
    let mut snap = snapshot_with(5, true);
    let origin = SnapOrigin {
        hwnd: 7,
        token: "t".to_owned(),
        pid: 1,
        creation: "c".to_owned(),
    };
    Arc::get_mut(&mut snap)
        .expect("unique")
        .snap_origins
        .insert(7, origin.clone());
    Arc::get_mut(&mut snap)
        .expect("unique")
        .windrag_origins
        .insert(
            7,
            WindragOrigin {
                origin,
                tag: Some("tag-7".to_owned()),
            },
        );
    channels
        .inbox
        .push(InboxItem::Snapshot(Arc::clone(&snap)))
        .expect("fits");
    assert!(!core.step());
    assert!(crate::snapkey::sys::test_keyboard_origins().contains_key(&7));
    assert!(
        crate::win_mouse::sys::test_snapshot()
            .windrag_origins
            .contains_key(&7)
    );
    channels.inbox.push(InboxItem::ClearWindrag).expect("fits");
    assert!(!core.step());
    assert!(crate::snapkey::sys::test_keyboard_origins().is_empty());
    assert!(
        crate::win_mouse::sys::test_snapshot()
            .windrag_origins
            .is_empty()
    );
    // Forced republish of the unchanged snapshot restores both maps.
    channels
        .inbox
        .push(InboxItem::Snapshot(snap))
        .expect("fits");
    assert!(!core.step());
    assert!(crate::snapkey::sys::test_keyboard_origins().contains_key(&7));
    assert!(
        crate::win_mouse::sys::test_snapshot()
            .windrag_origins
            .contains_key(&7)
    );
    assert_eq!(
        channels
            .applied_policy_gen
            .load(std::sync::atomic::Ordering::SeqCst),
        5
    );
    crate::snapkey::sys::test_clear_state();
    crate::win_mouse::sys::test_clear_state();
}

#[test]
fn install_with_queued_invalidate_does_not_rearm() {
    // Snapshot and invalidation queued before the hooks exist: the
    // step applies to absent TLS, installs empty hook state, then
    // reapplies the retention. The retention must have forgotten the
    // invalidated origin, or the install would rearm it. Serial
    // guard: the step applies generations to the process-global
    // fence.
    let _guard = crate::input_handoff::fence_test_serial();
    crate::snapkey::sys::test_clear_state();
    crate::win_mouse::sys::test_clear_state();
    let (channels, signals) = test_channels();
    let backend = super::super::FakeBackend::new(vec![true], vec![true], true);
    let mut core = PumpCore::new(
        backend,
        Arc::clone(&channels),
        Arc::clone(&signals),
        KeyboardConfig::disabled(),
    );
    let mut snap = snapshot_with(40, true);
    let origin = SnapOrigin {
        hwnd: 7,
        token: "t".to_owned(),
        pid: 1,
        creation: "c".to_owned(),
    };
    Arc::get_mut(&mut snap)
        .expect("unique")
        .snap_origins
        .insert(7, origin.clone());
    Arc::get_mut(&mut snap)
        .expect("unique")
        .windrag_origins
        .insert(
            7,
            WindragOrigin {
                origin,
                tag: Some("tag-7".to_owned()),
            },
        );
    channels
        .inbox
        .push(InboxItem::Snapshot(snap))
        .expect("fits");
    channels
        .inbox
        .push(InboxItem::InvalidateWindrag(7))
        .expect("fits");
    assert!(!core.step());
    assert!(core.keyboard_hook.is_some());
    assert!(core.mouse_hook.is_some());
    assert!(!crate::snapkey::sys::test_keyboard_origins().contains_key(&7));
    assert!(
        !crate::win_mouse::sys::test_snapshot()
            .windrag_origins
            .contains_key(&7)
    );
    crate::snapkey::sys::test_clear_state();
    crate::win_mouse::sys::test_clear_state();
}

#[test]
fn keyboard_recovers_without_rearming_invalidated_origin() {
    // Keyboard fails while the mouse installs (gate closed, no mask
    // source), an invalidation lands, then the keyboard recovers:
    // the mouse gate must reinstall in the recovered direction
    // (closed -> open) from the retention without rearming the
    // invalidated origin on either hook. Serial guard: the step
    // applies generations to the process-global fence.
    let _guard = crate::input_handoff::fence_test_serial();
    crate::snapkey::sys::test_clear_state();
    crate::win_mouse::sys::test_clear_state();
    let (channels, signals) = test_channels();
    let backend = super::super::FakeBackend::new(vec![false, true], vec![true], true);
    let mut core = PumpCore::new(
        backend,
        Arc::clone(&channels),
        Arc::clone(&signals),
        KeyboardConfig::disabled(),
    );
    let mut snap = snapshot_with(9, true);
    let origin = SnapOrigin {
        hwnd: 7,
        token: "t".to_owned(),
        pid: 1,
        creation: "c".to_owned(),
    };
    Arc::get_mut(&mut snap)
        .expect("unique")
        .snap_origins
        .insert(7, origin.clone());
    Arc::get_mut(&mut snap)
        .expect("unique")
        .windrag_origins
        .insert(
            7,
            WindragOrigin {
                origin,
                tag: Some("tag-7".to_owned()),
            },
        );
    channels
        .inbox
        .push(InboxItem::Snapshot(snap))
        .expect("fits");
    assert!(!core.step());
    assert!(core.keyboard_hook.is_none());
    assert!(core.mouse_hook.is_some());
    // Mouse installed without the keyboard: origins present but the
    // fresh-Down gate closed.
    assert!(
        crate::win_mouse::sys::test_snapshot()
            .windrag_origins
            .contains_key(&7)
    );
    assert!(!crate::win_mouse::sys::test_keyboard_installed());
    channels
        .inbox
        .push(InboxItem::InvalidateWindrag(7))
        .expect("fits");
    core.keyboard_retry_at = Some(Instant::now());
    assert!(!core.step());
    assert!(core.keyboard_hook.is_some());
    // Recovery installed the retained policy, not the stale spawn
    // init, and neither hook rearmed the invalidated origin.
    assert!(core.backend.keyboard_configs.contains(&KeyboardConfig {
        takeover: true,
        allow_win_l: false,
    }));
    assert!(!crate::snapkey::sys::test_keyboard_origins().contains_key(&7));
    assert!(
        !crate::win_mouse::sys::test_snapshot()
            .windrag_origins
            .contains_key(&7)
    );
    // The mouse gate recovered with the keyboard (closed -> open).
    assert!(crate::win_mouse::sys::test_keyboard_installed());
    assert!(crate::win_mouse::sys::test_snapshot().active);
    crate::snapkey::sys::test_clear_state();
    crate::win_mouse::sys::test_clear_state();
}

#[test]
fn revert_snapshot_advances_ack_with_equal_content() {
    // Owner revert after a refused policy change reuses the armed
    // generation with content equal to the applied policy: the ack
    // must still advance, or the fence would stay pending forever
    // while the classifier is already back on the desired policy.
    // Serial guard: the step applies generations to the
    // process-global fence.
    let _guard = crate::input_handoff::fence_test_serial();
    crate::snapkey::sys::test_ensure_state(KeyboardConfig {
        takeover: true,
        allow_win_l: false,
    });
    crate::win_mouse::sys::test_ensure_state();
    let (channels, signals) = test_channels();
    let backend = super::super::FakeBackend::new(vec![true], vec![true], true);
    let mut core = PumpCore::new(
        backend,
        Arc::clone(&channels),
        Arc::clone(&signals),
        KeyboardConfig {
            takeover: true,
            allow_win_l: false,
        },
    );
    assert!(!core.step());
    channels
        .inbox
        .push(InboxItem::Snapshot(snapshot_with(5, true)))
        .expect("fits");
    assert!(!core.step());
    assert_eq!(
        channels
            .applied_policy_gen
            .load(std::sync::atomic::Ordering::SeqCst),
        5
    );
    // Revert: same policy content as applied, reused armed gen 6.
    channels
        .inbox
        .push(InboxItem::Snapshot(snapshot_with(6, true)))
        .expect("fits");
    assert!(!core.step());
    assert_eq!(
        channels
            .applied_policy_gen
            .load(std::sync::atomic::Ordering::SeqCst),
        6
    );
    crate::snapkey::sys::test_clear_state();
    crate::win_mouse::sys::test_clear_state();
}

#[test]
fn stalled_overflow_escalates_and_resumes_through_pump_seam() {
    // Production sender plus production pump step: a stalled
    // transport absorbing far more distinct invalidations than the
    // fixed cap escalates to a clear the pump applies (origins
    // dropped on both hooks, latched suspend clears applied), then a
    // resume snapshot restores; the cycle repeats. Proves bounded
    // lossless delivery end to end: no rearm of obsolete origins, no
    // lost suspend clears, fixed slot state throughout. Serial
    // guard: the step applies generations (and `ClearEsc` global
    // edges) process-globally, racing fence/edge tests.
    let _guard = crate::input_handoff::fence_test_serial();
    use crate::input_handoff::INBOX_CAP;
    use crate::snapkey::VK_ESCAPE;
    use crate::snapkey::sys::{take_esc_edge, take_windrag_esc_edge, test_drive_llproc};
    crate::snapkey::sys::test_clear_state();
    crate::win_mouse::sys::test_clear_state();
    let (channels, signals) = test_channels();
    let backend = super::super::FakeBackend::new(vec![true], vec![true], true);
    let mut core = PumpCore::new(
        backend,
        Arc::clone(&channels),
        Arc::clone(&signals),
        KeyboardConfig {
            takeover: true,
            allow_win_l: false,
        },
    );
    assert!(!core.step());
    assert!(signals.is_ready());
    let mut sender = super::super::InboxSender::new(Arc::clone(&channels.inbox));
    for cycle in 0..2u64 {
        // Publish origins and apply through the real step (the resume
        // path: in production the owner's change-gated flow rebuilds
        // after managed-set churn, which is also what produced the
        // invalidations, so fresher snapshots rearm only live
        // origins and never obsolete ones).
        let mut snap = snapshot_with(100 + cycle, true);
        for hwnd in [7u64, 8] {
            let origin = SnapOrigin {
                hwnd,
                token: "t".to_owned(),
                pid: 1,
                creation: "c".to_owned(),
            };
            Arc::get_mut(&mut snap)
                .expect("unique")
                .snap_origins
                .insert(hwnd, origin.clone());
            Arc::get_mut(&mut snap)
                .expect("unique")
                .windrag_origins
                .insert(
                    hwnd,
                    WindragOrigin {
                        origin,
                        tag: Some("tag".to_owned()),
                    },
                );
        }
        channels
            .inbox
            .push(InboxItem::Snapshot(snap))
            .expect("fits");
        assert!(!core.step());
        assert!(crate::snapkey::sys::test_keyboard_origins().contains_key(&7));
        assert!(
            crate::win_mouse::sys::test_snapshot()
                .windrag_origins
                .contains_key(&8)
        );
        // Latch suspend-clear edges through the real callback, prove
        // latched, re-latch for the cycle.
        test_drive_llproc(VK_ESCAPE, 0, 0, 0);
        assert!(take_esc_edge());
        assert!(take_windrag_esc_edge());
        test_drive_llproc(VK_ESCAPE, 0, 0, 0);
        // Stall with harmless idempotent filler (unknown HWNDs touch
        // nothing), then churn far past the fixed cap plus the
        // suspend-clear obligation.
        for n in 0..INBOX_CAP {
            channels
                .inbox
                .push(InboxItem::InvalidateWindrag(900 + n as u64))
                .expect("fits");
        }
        sender
            .offer(InboxItem::InvalidateWindrag(500 + cycle))
            .expect("buffered");
        for hwnd in 0..40u64 {
            sender
                .offer(InboxItem::InvalidateWindrag(2000 + cycle * 100 + hwnd))
                .expect("absorbed");
        }
        sender.offer(InboxItem::ClearEsc).expect("absorbed");
        assert!(sender.safety_len() <= super::super::SAFETY_INVALIDATES_CAP + 2);
        // Drain through real pump steps interleaved with sender
        // retries; ends with a step so the inbox is fully applied.
        for _ in 0..16 {
            sender.flush();
            assert!(!core.step());
        }
        assert_eq!(sender.pending_len(), 0);
        assert_eq!(sender.safety_len(), 0);
        assert!(!sender.admission_closed);
        assert!(channels.inbox.is_empty());
        // Escalated clear applied: origins dropped on both hooks...
        assert!(crate::snapkey::sys::test_keyboard_origins().is_empty());
        assert!(
            crate::win_mouse::sys::test_snapshot()
                .windrag_origins
                .is_empty()
        );
        // ...and the latched suspend clears applied, not lost.
        assert!(!take_esc_edge());
        assert!(!take_windrag_esc_edge());
    }
    crate::snapkey::sys::test_clear_state();
    crate::win_mouse::sys::test_clear_state();
}

#[test]
fn policy_change_drains_stale_once_with_ack() {
    // A real policy change through the step drains the classifier
    // queue exactly once and publishes the ack generation + count.
    // Serial guard: the step applies generations to the
    // process-global fence.
    let _guard = crate::input_handoff::fence_test_serial();
    crate::snapkey::sys::test_ensure_state(KeyboardConfig {
        takeover: true,
        allow_win_l: false,
    });
    crate::win_mouse::sys::test_ensure_state();
    let (channels, signals) = test_channels();
    let backend = super::super::FakeBackend::new(vec![true], vec![true], true);
    let mut core = PumpCore::new(
        backend,
        Arc::clone(&channels),
        Arc::clone(&signals),
        KeyboardConfig {
            takeover: true,
            allow_win_l: false,
        },
    );
    assert!(!core.step());
    channels
        .inbox
        .push(InboxItem::Snapshot(snapshot_with(1, true)))
        .expect("fits");
    assert!(!core.step());
    assert_eq!(
        channels
            .applied_policy_gen
            .load(std::sync::atomic::Ordering::SeqCst),
        1
    );
    // Map-only republish (same policy): no drain, generation held.
    channels
        .inbox
        .push(InboxItem::Snapshot(snapshot_with(1, true)))
        .expect("fits");
    assert!(!core.step());
    assert_eq!(
        channels
            .applied_policy_gen
            .load(std::sync::atomic::Ordering::SeqCst),
        1
    );
    crate::snapkey::sys::test_clear_state();
    crate::win_mouse::sys::test_clear_state();
}

#[test]
fn pending_stall_preserves_tagged_terminal_order() {
    // Terminal Up queued behind an occupied outbox waits in the real
    // queue and arrives tagged after release; a parked batch keeps
    // its original generation when it finally offers. Serial guard:
    // the step applies generations to the process-global fence.
    let _guard = crate::input_handoff::fence_test_serial();
    crate::snapkey::sys::test_ensure_state(KeyboardConfig {
        takeover: true,
        allow_win_l: false,
    });
    crate::win_mouse::sys::test_ensure_state();
    let (channels, signals) = test_channels();
    let backend = super::super::FakeBackend::new(vec![true], vec![true], true);
    let mut core = PumpCore::new(
        backend,
        Arc::clone(&channels),
        Arc::clone(&signals),
        KeyboardConfig {
            takeover: true,
            allow_win_l: false,
        },
    );
    assert!(!core.step());
    channels
        .inbox
        .push(InboxItem::Snapshot(snapshot_with(1, true)))
        .expect("fits");
    assert!(!core.step());
    channels
        .windrag_outbox
        .put(TaggedBatch::new(
            1,
            vec![crate::win_mouse::WinDragEdge {
                hwnd: 1,
                kind: crate::win_mouse::WinDragKind::Down,
                x: 0,
                y: 0,
                esc_seq: 0,
                snapshot: None,
            }],
        ))
        .expect("occupy");
    crate::win_mouse::sys::test_push_edge(crate::win_mouse::WinDragEdge {
        hwnd: 7,
        kind: crate::win_mouse::WinDragKind::Up,
        x: 2,
        y: 3,
        esc_seq: 1,
        snapshot: None,
    });
    // Occupied: the terminal Up waits in the real queue.
    assert!(!core.step());
    assert!(core.pending_windrag.is_none());
    channels.windrag_outbox.take().expect("release");
    assert!(!core.step());
    let batch = channels.windrag_outbox.take().expect("offered");
    assert_eq!(batch.generation, 1);
    assert_eq!(batch.items.len(), 1);
    assert_eq!(batch.items[0].kind, crate::win_mouse::WinDragKind::Up);
    // Parked batches keep their drain-time generation on retry.
    core.pending_snap = Some(TaggedBatch::new(1, Vec::new()));
    channels
        .snap_outbox
        .put(TaggedBatch::new(1, Vec::new()))
        .expect("occupy");
    assert!(!core.step());
    assert!(core.pending_snap.is_some());
    channels.snap_outbox.take().expect("release");
    assert!(!core.step());
    assert!(core.pending_snap.is_none());
    let batch = channels.snap_outbox.take().expect("offered");
    assert_eq!(batch.generation, 1);
    crate::snapkey::sys::test_clear_state();
    crate::win_mouse::sys::test_clear_state();
}

#[test]
fn stop_unhook_failure_retries_bounded_then_reports_unclean() {
    // Permanent unhook failure: bounded creator-side retry, then an
    // unclean terminal report (never claims clean, never hangs).
    let (channels, signals) = test_channels();
    let backend = super::super::FakeBackend::new(vec![true], vec![true], true);
    let mut core = PumpCore::new(
        backend,
        Arc::clone(&channels),
        Arc::clone(&signals),
        KeyboardConfig {
            takeover: true,
            allow_win_l: false,
        },
    );
    assert!(!core.step());
    channels
        .stop_flag
        .store(true, std::sync::atomic::Ordering::SeqCst);
    core.backend.unhook_script.clear();
    for _ in 0..4 {
        core.backend.unhook_script.push_back(false);
    }
    let report = core.shutdown();
    assert!(!report.clean && !report.unhook_ok);
}

/// Test-only owner handle over a fake pump thread: the thread runs
/// no hooks, installs nothing, and only returns a [`StopReport`]
/// (or blocks/panics as scripted), so stop-seam behavior is
/// exercised with zero native effects.
fn test_handle(thread: Option<std::thread::JoinHandle<StopReport>>) -> InputHandle {
    let (fail_tx, fail_rx) = sync_channel(16);
    let mut handle = InputHandle {
        channels: Arc::new(InputChannels::new()),
        signals: Arc::new(PumpSignals::new(fail_tx)),
        failures: fail_rx,
        lifecycle: PumpLifecycle::new(),
        thread,
        stop_report: None,
        keyboard_config: KeyboardConfig {
            takeover: true,
            allow_win_l: false,
        },
    };
    handle.lifecycle.start().expect("fresh handle starts");
    handle.request_stop().expect("stop requested");
    handle
}

#[test]
fn second_wait_returns_cached_report_without_panic() {
    // Terminal waits are idempotent: a second bounded wait returns
    // the stored report instead of joining a consumed handle.
    let _serial = crate::input_handoff::pump_slot_test_serial();
    let thread = std::thread::Builder::new()
        .name("fake-input-stop".to_owned())
        .spawn(|| StopReport {
            unhook_ok: true,
            clean: true,
        })
        .expect("fake thread spawns");
    let mut handle = test_handle(Some(thread));
    handle.signals.note_stopped(true);
    let first = handle.wait_stopped(Duration::from_secs(10));
    assert!(first.clean && first.unhook_ok);
    let second = handle.wait_stopped(Duration::from_secs(10));
    assert_eq!(second, first);
    crate::input_handoff::pump_slot_release();
}

#[test]
fn ack_with_stalled_thread_stays_bounded_then_completes() {
    // An ack published before the thread finishes must not block
    // the owner past the bound: the handle is retained and the
    // call reports unclean; a later wait still joins cleanly.
    let _serial = crate::input_handoff::pump_slot_test_serial();
    let (gate_tx, gate_rx) = std::sync::mpsc::channel::<()>();
    let thread = std::thread::Builder::new()
        .name("fake-input-stall".to_owned())
        .spawn(move || {
            let _ = gate_rx.recv();
            StopReport {
                unhook_ok: true,
                clean: true,
            }
        })
        .expect("fake thread spawns");
    let mut handle = test_handle(Some(thread));
    handle.signals.note_stopped(true);
    let started = Instant::now();
    let first = handle.wait_stopped(Duration::from_millis(100));
    let elapsed = started.elapsed();
    assert!(!first.clean, "stalled thread must not report clean");
    assert!(
        elapsed < Duration::from_secs(10),
        "stop wait stayed bounded, took {elapsed:?}"
    );
    assert!(
        handle.thread.is_some(),
        "stalled handle retained for a later wait"
    );
    drop(gate_tx);
    let second = handle.wait_stopped(Duration::from_secs(10));
    assert!(second.clean && second.unhook_ok);
    crate::input_handoff::pump_slot_release();
}

#[test]
fn join_panic_reports_unclean_not_clean() {
    // A thread that acks then panics must not report clean from
    // the ack alone: the failed join folds into the report.
    let _serial = crate::input_handoff::pump_slot_test_serial();
    let (fail_tx, fail_rx) = sync_channel(16);
    let signals = Arc::new(PumpSignals::new(fail_tx));
    let thread_signals = Arc::clone(&signals);
    let thread = std::thread::Builder::new()
        .name("fake-input-panic".to_owned())
        .spawn(move || -> StopReport {
            thread_signals.note_stopped(true);
            panic!("simulated pump panic");
        })
        .expect("fake thread spawns");
    let mut handle = InputHandle {
        channels: Arc::new(InputChannels::new()),
        signals,
        failures: fail_rx,
        lifecycle: PumpLifecycle::new(),
        thread: Some(thread),
        stop_report: None,
        keyboard_config: KeyboardConfig {
            takeover: true,
            allow_win_l: false,
        },
    };
    handle.lifecycle.start().expect("fresh handle starts");
    handle.request_stop().expect("stop requested");
    let report = handle.wait_stopped(Duration::from_secs(10));
    assert!(
        !report.clean,
        "panicked thread must not report clean, got {report:?}"
    );
    assert!(
        handle.lifecycle.state() == crate::input_handoff::PumpState::Stopping,
        "unclean stop retains Stopping, never frees the slot"
    );
    crate::input_handoff::pump_slot_release();
}

#[test]
fn failed_ack_retains_slot_and_refuses_overlap() {
    // A failed unhook ack reports unclean and keeps the process
    // slot, so no overlapping worker can start; a clean stop
    // releases it.
    let _serial = crate::input_handoff::pump_slot_test_serial();
    assert!(!crate::input_handoff::pump_slot_reserved());
    assert!(crate::input_handoff::pump_slot_try_reserve());
    let thread = std::thread::Builder::new()
        .name("fake-input-fail".to_owned())
        .spawn(|| StopReport {
            unhook_ok: false,
            clean: false,
        })
        .expect("fake thread spawns");
    let mut handle = test_handle(Some(thread));
    handle.signals.note_stopped(false);
    let report = handle.wait_stopped(Duration::from_secs(10));
    assert!(!report.clean && !report.unhook_ok);
    assert!(
        crate::input_handoff::pump_slot_reserved(),
        "unclean stop retains the slot"
    );
    assert!(
        !crate::input_handoff::pump_slot_try_reserve(),
        "no overlapping worker while retained"
    );
    crate::input_handoff::pump_slot_release();
    assert!(crate::input_handoff::pump_slot_try_reserve());
    let thread = std::thread::Builder::new()
        .name("fake-input-clean".to_owned())
        .spawn(|| StopReport {
            unhook_ok: true,
            clean: true,
        })
        .expect("fake thread spawns");
    let mut handle = test_handle(Some(thread));
    handle.signals.note_stopped(true);
    let report = handle.wait_stopped(Duration::from_secs(10));
    assert!(report.clean && report.unhook_ok);
    assert!(
        !crate::input_handoff::pump_slot_reserved(),
        "clean stop releases the slot"
    );
}

#[test]
fn missing_ack_with_failed_thread_reports_unclean() {
    // No ack, already-finished thread: the bounded wait joins and
    // folds the thread's own unhook failure, never claiming clean.
    let _serial = crate::input_handoff::pump_slot_test_serial();
    let thread = std::thread::Builder::new()
        .name("fake-input-noack".to_owned())
        .spawn(|| StopReport {
            unhook_ok: false,
            clean: false,
        })
        .expect("fake thread spawns");
    let mut handle = test_handle(Some(thread));
    let report = handle.wait_stopped(Duration::from_secs(10));
    assert!(!report.clean && !report.unhook_ok);
    crate::input_handoff::pump_slot_release();
}

#[test]
fn esc_clocks_are_process_singletons() {
    // No per-handle mismatch possible: callbacks and the owner share
    // these exact atomics for the process lifetime (monotonic, so
    // parallel tests can only advance them further).
    let before = global_esc_clock().load();
    global_esc_clock().bump();
    global_esc_clock().bump();
    assert!(global_esc_clock().load() >= before.wrapping_add(2));
    let _ = SnapClassify::new(KeyboardConfig::disabled());
    let _ = global_windrag_esc_clock().load();
}

fn wake_signaled(channels: &InputChannels) -> bool {
    let Some(handle) = channels.intent_wake() else {
        panic!("intent wake missing");
    };
    crate::ffi::event_signaled(handle)
}

fn dummy_snap() -> crate::snapkey::QueuedSnapEvent {
    use crate::snapkey::{QueuedIntent, QueuedSnapEvent, SnapEdge, SnapOp};
    use tiler_core::directional::Direction;
    QueuedSnapEvent::Intent(QueuedIntent {
        op: SnapOp::Focus,
        direction: Direction::Left,
        edge: SnapEdge::Down,
        origin: None,
        consumed: true,
        announce: true,
        tick: Instant::now(),
    })
}

fn dummy_windrag() -> crate::win_mouse::WinDragEdge {
    crate::win_mouse::WinDragEdge {
        hwnd: 7,
        kind: crate::win_mouse::WinDragKind::Down,
        x: 1,
        y: 2,
        esc_seq: 0,
        snapshot: None,
    }
}

fn installed_core(
    channels: &Arc<InputChannels>,
    signals: &Arc<PumpSignals>,
) -> PumpCore<super::super::FakeBackend> {
    let backend = super::super::FakeBackend::new(vec![true], vec![true], true);
    let mut core = PumpCore::new(
        backend,
        Arc::clone(channels),
        Arc::clone(signals),
        KeyboardConfig {
            takeover: true,
            allow_win_l: false,
        },
    );
    assert!(!core.step());
    assert!(core.keyboard_hook.is_some());
    assert!(core.mouse_hook.is_some());
    assert!(!wake_signaled(channels));
    core
}

#[test]
fn intent_wake_created_nonsignaled() {
    // Lifecycle happy path: fresh channels carry a non-signaled
    // auto-reset wake (no spurious owner wake).
    let (channels, _signals) = test_channels();
    assert!(channels.intent_wake().is_some());
    assert!(!wake_signaled(&channels));
}

#[test]
fn snap_offer_signals_single_publication() {
    // Publication: one accepted snap batch leaves one pending wake
    // for the owner wait; the first probe consumes it (auto-reset)
    // and the second finds nothing.
    crate::snapkey::sys::test_ensure_state(KeyboardConfig {
        takeover: true,
        allow_win_l: false,
    });
    crate::win_mouse::sys::test_ensure_state();
    let (channels, signals) = test_channels();
    let mut core = installed_core(&channels, &signals);
    assert!(crate::snapkey::sys::test_push_queued(dummy_snap()));
    assert!(!core.step());
    assert!(channels.snap_outbox.occupied());
    assert!(wake_signaled(&channels));
    assert!(!wake_signaled(&channels));
    channels.snap_outbox.take().expect("batch");
    crate::snapkey::sys::test_clear_state();
    crate::win_mouse::sys::test_clear_state();
}

#[test]
fn snap_and_windrag_offers_share_one_pending_wake() {
    // Coalescing: one turn offering both a snap and a windrag batch
    // occupies both outboxes but leaves a single pending wake.
    crate::snapkey::sys::test_ensure_state(KeyboardConfig {
        takeover: true,
        allow_win_l: false,
    });
    crate::win_mouse::sys::test_ensure_state();
    let (channels, signals) = test_channels();
    let mut core = installed_core(&channels, &signals);
    assert!(crate::snapkey::sys::test_push_queued(dummy_snap()));
    crate::win_mouse::sys::test_push_edge(dummy_windrag());
    assert!(!core.step());
    assert!(channels.snap_outbox.occupied());
    assert!(channels.windrag_outbox.occupied());
    assert!(wake_signaled(&channels));
    assert!(!wake_signaled(&channels));
    assert_eq!(channels.snap_outbox.take().expect("batch").items.len(), 1);
    assert_eq!(
        channels.windrag_outbox.take().expect("batch").items.len(),
        1
    );
    crate::snapkey::sys::test_clear_state();
    crate::win_mouse::sys::test_clear_state();
}

#[test]
fn take_republish_interleaving_delivers_next_wake() {
    // Interleaving: offer/wake, consume wake, take batch, publish
    // another batch, and the next wake plus data arrive. A notify
    // landing after the take (owner already drained) is a harmless
    // extra wake: the following drain finds no data and the signal
    // does not stick.
    crate::snapkey::sys::test_ensure_state(KeyboardConfig {
        takeover: true,
        allow_win_l: false,
    });
    crate::win_mouse::sys::test_ensure_state();
    let (channels, signals) = test_channels();
    let mut core = installed_core(&channels, &signals);
    assert!(crate::snapkey::sys::test_push_queued(dummy_snap()));
    assert!(!core.step());
    assert!(wake_signaled(&channels));
    assert_eq!(channels.snap_outbox.take().expect("batch").items.len(), 1);
    assert!(crate::snapkey::sys::test_push_queued(dummy_snap()));
    assert!(!core.step());
    assert!(wake_signaled(&channels));
    assert_eq!(channels.snap_outbox.take().expect("batch").items.len(), 1);
    // Take-before-notify: extra wake with empty outboxes drains to
    // nothing and leaves no stuck signal.
    channels.notify_intent();
    assert!(wake_signaled(&channels));
    assert!(channels.snap_outbox.take().is_none());
    assert!(!wake_signaled(&channels));
    crate::snapkey::sys::test_clear_state();
    crate::win_mouse::sys::test_clear_state();
}

#[test]
fn occupied_snap_waits_without_wake_and_retry_signals() {
    // Occupied outboxes skip draining (items wait in the real queue,
    // no wake); the drain after release wakes, as does a parked
    // pending retry once space frees.
    crate::snapkey::sys::test_ensure_state(KeyboardConfig {
        takeover: true,
        allow_win_l: false,
    });
    crate::win_mouse::sys::test_ensure_state();
    let (channels, signals) = test_channels();
    let mut core = installed_core(&channels, &signals);
    channels
        .snap_outbox
        .put(TaggedBatch::new(0, vec![dummy_snap()]))
        .expect("occupy");
    assert!(crate::snapkey::sys::test_push_queued(dummy_snap()));
    assert!(!core.step());
    assert!(!wake_signaled(&channels));
    let filler = channels.snap_outbox.take().expect("release");
    assert_eq!(filler.items.len(), 1);
    assert!(!core.step());
    assert!(channels.snap_outbox.occupied());
    assert!(wake_signaled(&channels));
    let batch = channels.snap_outbox.take().expect("batch");
    assert_eq!(batch.items.len(), 1);
    // Thread-local parked batch: refused retry stays silent, the
    // retry after release wakes.
    core.pending_snap = Some(TaggedBatch::new(1, vec![dummy_snap()]));
    channels
        .snap_outbox
        .put(TaggedBatch::new(1, vec![dummy_snap()]))
        .expect("occupy");
    assert!(!core.step());
    assert!(core.pending_snap.is_some());
    assert!(!wake_signaled(&channels));
    channels.snap_outbox.take().expect("release");
    assert!(!core.step());
    assert!(core.pending_snap.is_none());
    assert!(wake_signaled(&channels));
    channels.snap_outbox.take().expect("batch");
    crate::snapkey::sys::test_clear_state();
    crate::win_mouse::sys::test_clear_state();
}

#[test]
fn chord_transition_signals_with_empty_batches_and_steady_stays_quiet() {
    // Modifier-only traffic queues no batch, yet a bare Win+Shift
    // transition must still wake the owner; steady levels must not. Drives
    // the real callback (no hooks, no `SendInput`: Win-up is never driven,
    // Shift carries no mask obligation) and asserts the wake arrives with
    // both outboxes empty, so it is the transition-only path, not a batch.
    use windows_sys::Win32::UI::WindowsAndMessaging::LLKHF_UP;
    crate::snapkey::sys::test_ensure_state(KeyboardConfig {
        takeover: true,
        allow_win_l: false,
    });
    crate::win_mouse::sys::test_ensure_state();
    let (channels, signals) = test_channels();
    let mut core = installed_core(&channels, &signals);
    assert_eq!(crate::snapkey::sys::chord_levels(), (false, false));
    crate::snapkey::sys::test_drive_llproc(crate::snapkey::VK_LWIN, 0, 0, 0);
    assert_eq!(crate::snapkey::sys::chord_levels(), (true, false));
    assert!(!core.step());
    // Win alone leaves the chord quiet: no batch, no transition wake.
    assert!(!wake_signaled(&channels));
    assert!(!channels.snap_outbox.occupied());
    crate::snapkey::sys::test_drive_llproc(crate::snapkey::VK_SHIFT, 0, 0, 0);
    assert_eq!(crate::snapkey::sys::chord_levels(), (true, true));
    assert!(!core.step());
    // Transition with empty outboxes: the wake is transition-only.
    assert!(wake_signaled(&channels));
    assert!(!wake_signaled(&channels));
    assert!(!channels.snap_outbox.occupied());
    assert!(!channels.windrag_outbox.occupied());
    // Steady hold: no repeated wake.
    assert!(!core.step());
    assert!(!wake_signaled(&channels));
    crate::snapkey::sys::test_drive_llproc(crate::snapkey::VK_SHIFT, 0, LLKHF_UP, 0);
    assert_eq!(crate::snapkey::sys::chord_levels(), (true, false));
    assert!(!core.step());
    assert!(wake_signaled(&channels));
    assert!(!wake_signaled(&channels));
    assert!(!channels.snap_outbox.occupied());
    crate::snapkey::sys::test_clear_state();
    crate::win_mouse::sys::test_clear_state();
}

#[test]
fn esc_edge_signals_once_without_stealing_the_owner_latch() {
    // An Esc down queues no batch, yet it must wake the owner once so an
    // open gesture latches promptly; the pump consumes only its own
    // thread-local edge while the process-global latch stays set for the
    // owner drain, so a second step stays quiet. Serial guard: the global
    // edge assertions race concurrent pump `ClearEsc` applies and sibling
    // Esc tests.
    use std::sync::atomic::Ordering;
    let _guard = crate::input_handoff::fence_test_serial();
    crate::snapkey::sys::test_ensure_state(KeyboardConfig {
        takeover: true,
        allow_win_l: false,
    });
    crate::win_mouse::sys::test_ensure_state();
    let (channels, signals) = test_channels();
    let mut core = installed_core(&channels, &signals);
    crate::snapkey::sys::test_drive_llproc(crate::snapkey::VK_ESCAPE, 1, 0, 0);
    assert!(!core.step());
    assert!(wake_signaled(&channels), "cancel edge wakes the owner");
    assert!(channels.snap_outbox.take().is_none());
    assert!(
        crate::input_handoff::global_esc_edge().load(Ordering::SeqCst),
        "owner latch survives the pump drain"
    );
    // Edge consumed pump-side: no repeated wake while the owner latch
    // pends its own drain.
    assert!(!core.step());
    assert!(!wake_signaled(&channels));
    assert!(
        crate::input_handoff::global_esc_edge().load(Ordering::SeqCst),
        "still latched for the owner"
    );
    // Owner-side consumption (as the tick drain does) quiets everything.
    crate::input_handoff::global_esc_edge().store(false, Ordering::SeqCst);
    crate::input_handoff::global_windrag_esc_edge().store(false, Ordering::SeqCst);
    crate::snapkey::sys::clear_esc_edge();
    crate::snapkey::sys::clear_windrag_esc_edge();
    assert!(!core.step());
    assert!(!wake_signaled(&channels));
    crate::snapkey::sys::test_clear_state();
    crate::win_mouse::sys::test_clear_state();
}

#[test]
fn windrag_offer_signals_while_diag_only_step_does_not() {
    // Mouse batches wake; a diagnostics-only turn never does.
    crate::snapkey::sys::test_ensure_state(KeyboardConfig {
        takeover: true,
        allow_win_l: false,
    });
    crate::win_mouse::sys::test_ensure_state();
    let (channels, signals) = test_channels();
    let mut core = installed_core(&channels, &signals);
    crate::win_mouse::sys::test_push_edge(dummy_windrag());
    assert!(!core.step());
    assert!(channels.windrag_outbox.occupied());
    assert!(wake_signaled(&channels));
    channels.windrag_outbox.take().expect("batch");
    // Diagnostics publish every turn; with empty queues nothing wakes.
    assert!(!core.step());
    assert!(!wake_signaled(&channels));
    crate::snapkey::sys::test_clear_state();
    crate::win_mouse::sys::test_clear_state();
}

#[test]
fn stalled_retry_wakes_once_on_capacity_freed() {
    // Refusal then drain: a full inbox refuses the owner's snapshot
    // (stalled retry, no owner wake pending), the pump's next successful
    // drain signals exactly one wake, and the freed capacity accepts the
    // retry with no unrelated event and no repeat signal. Serial
    // guard: the inbox holds `ClearEsc` (process-global edges) plus a
    // snapshot generation applied process-globally, racing fence/edge
    // tests.
    use std::sync::atomic::Ordering;
    let _guard = crate::input_handoff::fence_test_serial();
    crate::snapkey::sys::test_ensure_state(KeyboardConfig {
        takeover: true,
        allow_win_l: false,
    });
    crate::win_mouse::sys::test_ensure_state();
    let (channels, signals) = test_channels();
    let mut core = installed_core(&channels, &signals);
    // Fill the inbox to its cap through the real bounded push.
    for _ in 0..crate::input_handoff::INBOX_CAP {
        let mut slot = None;
        assert!(
            channels
                .inbox
                .try_push(crate::input_handoff::InboxItem::ClearEsc, &mut slot)
        );
    }
    // Owner-side sender over the same inbox: the first snapshot buffers
    // and closes admission, the second refuses (mirrors the production
    // `offer_input_snapshot` refusal that arms `retry_gen`).
    let mut sender = crate::input_pump::InboxSender::new(Arc::clone(&channels.inbox));
    let snapshot = || crate::input_handoff::InboxItem::Snapshot(snapshot_with(1, true));
    assert!(sender.offer(snapshot()).is_ok(), "first buffers");
    assert!(
        sender.offer(snapshot()).is_err(),
        "second refuses while full"
    );
    assert!(sender.flush() > 0, "flush still outstanding");
    // Owner arms the stalled flag as `offer_input_snapshot` does.
    channels.inbox_stalled.store(true, Ordering::SeqCst);
    // Pump drains the full inbox: exactly one wake, no unrelated event.
    assert!(!core.step());
    assert!(wake_signaled(&channels), "capacity-freed wakes the retry");
    assert!(!wake_signaled(&channels), "bursts coalesce to one wake");
    // Freed capacity accepts the retry; the flag clears so the next
    // drain stays quiet.
    assert_eq!(sender.flush(), 0, "drain freed the inbox");
    assert!(sender.offer(snapshot()).is_ok(), "retry accepted");
    channels.inbox_stalled.store(false, Ordering::SeqCst);
    assert!(!core.step());
    assert!(!wake_signaled(&channels), "no repeat signal once clear");
    crate::snapkey::sys::test_clear_state();
    crate::win_mouse::sys::test_clear_state();
}

#[test]
fn stalled_retry_survives_drain_between_refusal_and_flag_store() {
    // Exact lost-wake interleaving: the owner's offer refuses (inbox
    // full), then a pump drain lands BEFORE the owner stores the stalled
    // flag (room freed with no signal — the old loss). The owner then
    // settles the flag and nudges the pump in production order
    // (`offer_input_snapshot`: store, then `notify_work` on refusal), and
    // the nudged step must still wake the retry with no unrelated event.
    // Serial guard: the inbox holds `ClearEsc` plus a snapshot
    // generation applied process-globally (and the retry step applies
    // it), racing fence/edge tests.
    use std::sync::atomic::Ordering;
    let _guard = crate::input_handoff::fence_test_serial();
    crate::snapkey::sys::test_ensure_state(KeyboardConfig {
        takeover: true,
        allow_win_l: false,
    });
    crate::win_mouse::sys::test_ensure_state();
    let (channels, signals) = test_channels();
    let mut core = installed_core(&channels, &signals);
    for _ in 0..crate::input_handoff::INBOX_CAP {
        let mut slot = None;
        assert!(
            channels
                .inbox
                .try_push(crate::input_handoff::InboxItem::ClearEsc, &mut slot)
        );
    }
    let mut sender = crate::input_pump::InboxSender::new(Arc::clone(&channels.inbox));
    let snapshot = || crate::input_handoff::InboxItem::Snapshot(snapshot_with(1, true));
    assert!(sender.offer(snapshot()).is_ok(), "first buffers");
    assert!(
        sender.offer(snapshot()).is_err(),
        "second refuses while full"
    );
    // Adversarial drain between refusal and flag establishment: empties
    // the inbox with no signal, since the flag is not yet set.
    let drained = channels.inbox.try_drain().expect("drain succeeds");
    assert!(!drained.is_empty(), "drain actually freed room");
    assert!(!wake_signaled(&channels), "pre-flag drain signals nothing");
    // Owner resumes in production order: settle the flag, then nudge.
    channels.inbox_stalled.store(true, Ordering::SeqCst);
    channels.notify_work();
    // Nudged pump step: the drain succeeds (empty but successful) and
    // observes the armed flag, so exactly one wake follows.
    assert!(!core.step());
    assert!(wake_signaled(&channels), "nudged drain recovers the retry");
    assert!(!wake_signaled(&channels), "bursts coalesce to one wake");
    // Retry accepted on the freed capacity; quiet after.
    assert_eq!(sender.flush(), 0, "retry flushes into freed room");
    assert!(sender.offer(snapshot()).is_ok(), "retry accepted");
    channels.inbox_stalled.store(false, Ordering::SeqCst);
    assert!(!core.step());
    assert!(!wake_signaled(&channels), "no repeat signal once clear");
    crate::snapkey::sys::test_clear_state();
    crate::win_mouse::sys::test_clear_state();
}

fn work_signaled(channels: &InputChannels) -> bool {
    let Some(handle) = channels.work_wake() else {
        panic!("work wake missing");
    };
    crate::ffi::event_signaled(handle)
}

#[test]
fn work_wake_created_nonsignaled() {
    // Owner-to-input direction: fresh channels carry a non-signaled
    // auto-reset work wake (no spurious pump wake).
    let (channels, _signals) = test_channels();
    assert!(channels.work_wake().is_some());
    assert!(!work_signaled(&channels));
}

#[test]
fn wait_reason_splits_work_message_and_retry_timeout() {
    // Native return-code split (no desktop): work wake, queued
    // messages, and the genuine deadline (`WAIT_TIMEOUT` 258) map
    // distinctly; `WAIT_FAILED` and any unrecognized code count as
    // `Failed`, never merged into timeout.
    use super::super::{InputWaitReason, classify_input_wait};
    assert_eq!(classify_input_wait(0, true), InputWaitReason::Work);
    assert_eq!(classify_input_wait(1, true), InputWaitReason::Message);
    assert_eq!(classify_input_wait(258, true), InputWaitReason::Timeout);
    assert_eq!(classify_input_wait(0, false), InputWaitReason::Message);
    assert_eq!(classify_input_wait(258, false), InputWaitReason::Timeout);
    assert_eq!(
        classify_input_wait(0xFFFF_FFFF, true),
        InputWaitReason::Failed
    );
    assert_eq!(
        classify_input_wait(0xFFFF_FFFF, false),
        InputWaitReason::Failed
    );
    assert_eq!(classify_input_wait(7, true), InputWaitReason::Failed);
}

#[test]
fn step_counts_native_waits_by_reason_with_turns_equal() {
    // Wait-return evidence through the production step: scripted
    // work/message/timeout/failed returns accumulate by reason and the
    // completed-turn count stays equal (no stop short-circuit here).
    // Timeout holds the genuine deadline only; failures stay separate.
    use super::super::InputWaitReason;
    let (channels, signals) = test_channels();
    let mut core = installed_core(&channels, &signals);
    let base_turns = channels.diag.snapshot().turns;
    core.backend.wait_script.push_back(InputWaitReason::Work);
    core.backend.wait_script.push_back(InputWaitReason::Message);
    core.backend.wait_script.push_back(InputWaitReason::Timeout);
    core.backend.wait_script.push_back(InputWaitReason::Failed);
    assert!(!core.step());
    assert!(!core.step());
    assert!(!core.step());
    assert!(!core.step());
    let diag = channels.diag.snapshot();
    assert_eq!(diag.turns, base_turns + 4);
    assert_eq!(diag.waits, base_turns + 4);
    assert_eq!(diag.wait_work, 1);
    assert_eq!(diag.wait_msg, 1);
    // installed_core's setup turn plus the four scripted turns: the
    // setup wait defaults to timeout.
    assert_eq!(diag.wait_timeout, base_turns + 1);
    assert_eq!(diag.wait_failed, 1);
}

#[test]
fn next_timer_idles_without_deadline_and_retries_on_deadline() {
    // Deadline evidence through the production step: fully installed
    // idles with `u32::MAX` (INFINITE); a failed install schedules
    // its backoff as the wait deadline; recovery idles again.
    let (channels, signals) = test_channels();
    let backend = super::super::FakeBackend::new(vec![false], vec![true], true);
    let mut core = PumpCore::new(
        backend,
        Arc::clone(&channels),
        Arc::clone(&signals),
        KeyboardConfig {
            takeover: true,
            allow_win_l: false,
        },
    );
    assert!(!core.step());
    assert!(core.keyboard_hook.is_none());
    assert!(core.mouse_hook.is_some());
    let due = core.next_timer().expect("keyboard retry pending");
    let secs = due.saturating_duration_since(Instant::now()).as_secs();
    assert!(
        (9..=10).contains(&secs),
        "keyboard backoff is 10s after the first failure, got {secs}s"
    );
    let waited = core
        .backend
        .pump_timeouts
        .last()
        .copied()
        .expect("wait recorded");
    assert!(
        (9900..=10000).contains(&waited),
        "wait matches the retry deadline, got {waited}ms"
    );
    // Recovery installs and the pump idles with no deadline.
    core.keyboard_retry_at = Some(Instant::now());
    core.backend.keyboard_script.push_back(true);
    assert!(!core.step());
    assert!(core.keyboard_hook.is_some());
    assert_eq!(core.next_timer(), None);
    assert_eq!(
        core.backend.pump_timeouts.last().copied(),
        Some(u32::MAX),
        "fully idle waits indefinitely"
    );
    crate::snapkey::sys::test_clear_state();
    crate::win_mouse::sys::test_clear_state();
}

#[test]
fn publish_after_wait_race_applies_without_loss() {
    // Publish-after-wait race: a work signal offered before the pump
    // wait stays pending (auto-reset), so the inbox control the owner
    // just published is never lost behind an idle wait.
    let _guard = crate::input_handoff::fence_test_serial();
    crate::snapkey::sys::test_ensure_state(KeyboardConfig {
        takeover: true,
        allow_win_l: false,
    });
    crate::win_mouse::sys::test_ensure_state();
    let (channels, signals) = test_channels();
    let mut core = installed_core(&channels, &signals);
    channels.inbox.push(InboxItem::ClearEsc).expect("fits");
    channels.notify_work();
    assert!(work_signaled(&channels), "publish wakes the idle pump");
    assert!(!core.step());
    assert!(channels.inbox.is_empty(), "published control applied");
    crate::snapkey::sys::test_clear_state();
    crate::win_mouse::sys::test_clear_state();
}

#[test]
fn drain_wakes_parked_retry_after_backpressure() {
    // Saturation/backpressure: an occupied outbox parks the retry
    // silently; the owner drain wakes the pump so the retained event
    // does not strand behind an idle wait.
    crate::snapkey::sys::test_ensure_state(KeyboardConfig {
        takeover: true,
        allow_win_l: false,
    });
    crate::win_mouse::sys::test_ensure_state();
    let (channels, signals) = test_channels();
    let mut core = installed_core(&channels, &signals);
    channels
        .snap_outbox
        .put(TaggedBatch::new(0, vec![dummy_snap()]))
        .expect("occupy");
    assert!(crate::snapkey::sys::test_push_queued(dummy_snap()));
    assert!(!core.step());
    assert!(!work_signaled(&channels), "no spurious wake while stalled");
    channels.snap_outbox.take().expect("owner drain");
    channels.notify_work();
    assert!(work_signaled(&channels), "drain wakes the pump");
    assert!(!core.step());
    assert!(channels.snap_outbox.occupied());
    channels.snap_outbox.take().expect("batch");
    crate::snapkey::sys::test_clear_state();
    crate::win_mouse::sys::test_clear_state();
}

#[test]
fn stop_wakes_idle_pump() {
    // Stop path: the flag plus the work signal (what `request_stop`
    // publishes) makes the next turn report stop promptly.
    let (channels, signals) = test_channels();
    let mut core = installed_core(&channels, &signals);
    assert_eq!(core.next_timer(), None, "installed pump idles");
    channels
        .stop_flag
        .store(true, std::sync::atomic::Ordering::SeqCst);
    channels.notify_work();
    assert!(work_signaled(&channels), "stop wakes the idle pump");
    assert!(core.step(), "stop reports promptly");
}

#[test]
fn pre_wait_callback_work_transfers_before_blocking() {
    // S1: callbacks delivered during the pre-wait drain queue
    // classifier work; the pre-wait transfer must offer it before
    // the wait blocks, or the final callback strands behind an idle
    // wait with nothing left to wake the pump. The fake asserts the
    // queues are empty at wait time, so the old drain-before-INFINITE
    // ordering fails loudly here with no external wake.
    crate::snapkey::sys::test_ensure_state(KeyboardConfig {
        takeover: true,
        allow_win_l: false,
    });
    crate::win_mouse::sys::test_ensure_state();
    let (channels, signals) = test_channels();
    let mut core = installed_core(&channels, &signals);
    core.backend.assert_transferred_before_wait = true;
    core.backend.snap_on_drain.push(dummy_snap());
    core.backend.windrag_on_drain.push(dummy_windrag());
    assert!(!core.step());
    assert!(channels.snap_outbox.occupied());
    assert!(channels.windrag_outbox.occupied());
    assert_eq!(
        core.backend.pump_timeouts.last().copied(),
        Some(u32::MAX),
        "installed pump still idles after transferring"
    );
    channels.snap_outbox.take().expect("batch");
    channels.windrag_outbox.take().expect("batch");
    crate::snapkey::sys::test_clear_state();
    crate::win_mouse::sys::test_clear_state();
}

#[test]
fn inbox_contention_arms_bounded_retry_without_loss() {
    // S2: inbox `try_drain` WouldBlock (owner mid-offer) arms one
    // short retry instead of stranding; the control applies on the
    // next turn once released. Bounded queues preserved throughout.
    let _guard = crate::input_handoff::fence_test_serial();
    crate::snapkey::sys::test_ensure_state(KeyboardConfig {
        takeover: true,
        allow_win_l: false,
    });
    crate::win_mouse::sys::test_ensure_state();
    let (channels, signals) = test_channels();
    let mut core = installed_core(&channels, &signals);
    channels.inbox.push(InboxItem::ClearEsc).expect("fits");
    {
        let _held = channels.inbox.test_lock_held();
        assert!(channels.inbox.try_drain().is_none());
        assert!(!core.step());
        let due = core.next_timer().expect("contention retry armed");
        assert!(
            due.saturating_duration_since(Instant::now())
                <= super::super::CONTENTION_RETRY + Duration::from_millis(50),
            "retry is bounded and short, got {due:?}"
        );
        let waited = core
            .backend
            .pump_timeouts
            .last()
            .copied()
            .expect("wait recorded");
        assert!(
            waited <= super::super::CONTENTION_RETRY.as_millis() as u32,
            "wait matches the contention retry, got {waited}ms"
        );
    }
    assert!(!core.step());
    assert!(channels.inbox.is_empty(), "control applied, never lost");
    assert_eq!(core.next_timer(), None, "clean turn clears the retry");
    crate::snapkey::sys::test_clear_state();
    crate::win_mouse::sys::test_clear_state();
}

#[test]
fn outbox_contention_parks_and_retries_without_loss() {
    // S2: outbox `try_occupied` WouldBlock on a free outbox (owner
    // mid-take) skips the drain but arms the retry; queued records
    // wait in the real queue under their own bound and offer next
    // turn. A parked pending batch under contention re-parks and
    // then offers; terminals never strand.
    crate::snapkey::sys::test_ensure_state(KeyboardConfig {
        takeover: true,
        allow_win_l: false,
    });
    crate::win_mouse::sys::test_ensure_state();
    let (channels, signals) = test_channels();
    let mut core = installed_core(&channels, &signals);
    assert!(crate::snapkey::sys::test_push_queued(dummy_snap()));
    channels.snap_outbox.test_with_lock_held(|| {
        assert!(!core.step());
    });
    assert!(
        core.pending_snap.is_none(),
        "nothing drained, nothing parked"
    );
    assert!(
        core.next_timer().is_some(),
        "unknown occupancy arms the retry"
    );
    assert!(!core.step());
    assert!(channels.snap_outbox.occupied(), "retained event offered");
    // Parked pending under contention: re-parks, then offers.
    core.pending_snap = Some(TaggedBatch::new(1, vec![dummy_snap()]));
    channels.snap_outbox.test_with_lock_held(|| {
        assert!(!core.step());
    });
    assert!(core.pending_snap.is_some(), "contended retry re-parks");
    assert!(core.next_timer().is_some());
    channels.snap_outbox.take().expect("owner drain");
    assert!(!core.step());
    assert!(core.pending_snap.is_none(), "parked batch offered");
    assert_eq!(core.next_timer(), None, "clean turn clears the retry");
    channels.snap_outbox.take().expect("batch");
    crate::snapkey::sys::test_clear_state();
    crate::win_mouse::sys::test_clear_state();
}

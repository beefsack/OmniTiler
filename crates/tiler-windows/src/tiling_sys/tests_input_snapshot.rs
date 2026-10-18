#![forbid(unsafe_code)]
use super::{SnapshotOffer, owner_snap_dropped, plan_snapshot_offer};
use crate::input_handoff::{
    Inbox, InboxItem, InputSnapshot, SharedSnapshot, fence_base_generation, fence_test_allocate,
    fence_test_serial, keyboard_fresh_gate, note_policy_applied, note_policy_published,
    policy_fence_pending,
};
use crate::input_pump::InboxSender;
use crate::snapkey::{KeyboardConfig, SnapOrigin};
use std::collections::HashMap;
use std::sync::Arc;

fn snapshot_with(takeover: bool, hwnds: &[u64]) -> InputSnapshot {
    let mut snap = InputSnapshot {
        generation: 0,
        keyboard: KeyboardConfig {
            takeover,
            allow_win_l: false,
        },
        remap: Vec::new(),
        disabled: Vec::new(),
        snap_origins: HashMap::new(),
        active: takeover,
        trace: false,
    };
    for hwnd in hwnds {
        let origin = SnapOrigin {
            hwnd: *hwnd,
            token: format!("token-{hwnd}"),
            pid: 1,
            creation: "c".to_owned(),
        };
        snap.snap_origins.insert(*hwnd, origin.clone());
    }
    snap
}

#[test]
fn plan_skips_unchanged_and_retries_armed() {
    let last: SharedSnapshot = Arc::new(snapshot_with(true, &[7]));
    let same = snapshot_with(true, &[7]);
    assert_eq!(
        plan_snapshot_offer(Some(&last), &same, None),
        SnapshotOffer::Skip
    );
    // A refused policy offer resends with its armed generation even for
    // a revert to `last` (never Skip) and never inflates a new one.
    assert_eq!(
        plan_snapshot_offer(Some(&last), &same, Some(41)),
        SnapshotOffer::Retry(41)
    );
    let changed = snapshot_with(false, &[]);
    assert_eq!(
        plan_snapshot_offer(Some(&last), &changed, None),
        SnapshotOffer::Fresh
    );
    assert_eq!(
        plan_snapshot_offer(None, &changed, None),
        SnapshotOffer::Fresh
    );
}

#[test]
fn refused_policy_reverts_with_reused_generation_and_fence_pairs() {
    // Production plan + production sender + production fence: A->B
    // refuses after arming, the revert to A still resends with the armed
    // generation (fresh callbacks pass while pending), and the pump ack
    // (even for content equal to the applied policy) resumes
    // consumption. Mirrors `offer_input_snapshot` field bookkeeping.
    let _guard = fence_test_serial();
    let base = fence_test_allocate();
    note_policy_applied(base);
    let mut published_gen = fence_base_generation();
    assert_eq!(published_gen, base);
    let inbox = Arc::new(Inbox::new());
    let mut sender = InboxSender::new(Arc::clone(&inbox));
    let mut last: Option<SharedSnapshot> = Some(Arc::new(snapshot_with(true, &[7])));
    let mut retry_gen: Option<u64> = None;
    // Stall the transport, then refuse the policy change B.
    for _ in 0..crate::input_handoff::INBOX_CAP {
        sender.offer(InboxItem::ClearEsc).expect("fits");
    }
    sender.offer(InboxItem::ClearEsc).expect("buffered");
    let mut candidate_b = snapshot_with(false, &[]);
    assert_eq!(
        plan_snapshot_offer(last.as_ref(), &candidate_b, retry_gen),
        SnapshotOffer::Fresh
    );
    published_gen += 1;
    let armed = published_gen;
    note_policy_published(armed);
    assert!(policy_fence_pending());
    assert!(!keyboard_fresh_gate(true));
    candidate_b.generation = armed;
    assert!(
        sender
            .offer(InboxItem::Snapshot(Arc::new(candidate_b)))
            .is_err(),
        "policy change refuses while stalled"
    );
    retry_gen = Some(armed);
    // Desired reverts to A: the plan must resend (never Skip) with the
    // armed generation, not a fresh bump.
    let revert = snapshot_with(true, &[7]);
    assert_eq!(
        plan_snapshot_offer(last.as_ref(), &revert, retry_gen),
        SnapshotOffer::Retry(armed)
    );
    // Pump drains; the retry lands behind the retained safety control.
    assert_eq!(inbox.drain().len(), crate::input_handoff::INBOX_CAP);
    assert_eq!(sender.flush(), 0);
    let mut retry = snapshot_with(true, &[7]);
    retry.generation = retry_gen.expect("retry armed");
    assert_eq!(retry.generation, armed);
    sender
        .offer(InboxItem::Snapshot(Arc::new(retry.clone())))
        .expect("retry accepted");
    last = Some(Arc::new(retry));
    retry_gen = None;
    assert!(last.as_ref().is_some_and(|s| s.generation == armed));
    assert_eq!(retry_gen, None);
    // Ordered behind the retained control, then the pump ack for
    // the revert (content equals the applied policy) clears the fence
    // and fresh consumption resumes.
    assert_eq!(
        inbox.drain(),
        vec![
            InboxItem::ClearEsc,
            InboxItem::Snapshot(last.expect("accepted")),
        ]
    );
    assert!(policy_fence_pending());
    assert!(!keyboard_fresh_gate(true));
    note_policy_applied(armed);
    assert!(!policy_fence_pending());
    assert!(keyboard_fresh_gate(true));
}

#[test]
fn owner_snap_dropped_reads_diag_when_bridged() {
    // Regression: on the product path the owner thread must not read its
    // own (empty) hook TLS. Bridged reads come from pump diagnostics.
    crate::snapkey::sys::test_ensure_state(KeyboardConfig {
        takeover: true,
        allow_win_l: false,
    });
    assert_eq!(crate::snapkey::sys::queue_dropped(), 0);
    assert_eq!(owner_snap_dropped(None), 0);
    assert_eq!(owner_snap_dropped(Some(7)), 7);
    crate::snapkey::sys::test_clear_state();
}

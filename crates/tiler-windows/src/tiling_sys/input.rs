#![forbid(unsafe_code)]
use super::*;

use super::trace::log_json_at;

/// Owner-side dedicated-input bridge (product path only). The pump owns both
/// hooks and their classifiers; the owner publishes versioned snapshots and
/// ordered controls through the bounded inbox and dispatches tagged batches.
/// Generations pair batches with policy: a batch tagged older than the latest
/// published policy is stale-dropped (mask events still resolve), never
/// dispatched under a replaced policy.
pub(super) struct InputBridge {
    /// Pump channels (outboxes, diagnostics, apply-ack atomics).
    pub(super) channels: std::sync::Arc<crate::input_pump::InputChannels>,
    /// Bounded FIFO sender with ordered pending retry.
    pub(super) sender: InboxSender,
    /// Pump thread handle (lifecycle + bounded join).
    pub(super) handle: InputHandle,
    /// Last snapshot offered (change detection; content without generation).
    pub(super) last_snapshot: Option<SharedSnapshot>,
    /// Armed-but-refused policy generation awaiting retry. Set when a
    /// policy-changing snapshot refuses (inbox fault) after the fence armed;
    /// the next offer reuses this generation for whatever the current
    /// desired content is (even a revert to `last_snapshot`) instead of
    /// inflating a new one, so the fence and the ack pair up exactly once.
    /// Cleared when a snapshot is accepted. Map-only refusals need none:
    /// unchanged `last_snapshot` re-offers them naturally with no fence.
    pub(super) retry_gen: Option<u64>,
    /// Latest published policy generation.
    pub(super) published_gen: u64,
    /// Last observed applied policy generation.
    pub(super) applied_gen: u64,
    /// Last observed sender fault count (transition-only fault logs).
    pub(super) last_faults: u64,
    /// Last observed sender dedupe count (transition-only evidence).
    pub(super) last_deduped: u64,
    /// Policy generation awaiting apply ack (`None` once acknowledged).
    pub(super) pending_ack: Option<u64>,
    /// Last acknowledged config-apply log (dedupes the ack event).
    pub(super) last_ack_logged: u64,
}

/// Log sender fault/dedupe transitions with pending counts (once per
/// transition, never per-tick noise).
pub(super) fn log_input_faults(bridge: &mut InputBridge, log_path: &std::path::Path) {
    if bridge.sender.faults != bridge.last_faults {
        bridge.last_faults = bridge.sender.faults;
        log_json_at(
            log_path,
            serde_json::json!({
                "event": "input-inbox-full",
                "faults": bridge.sender.faults,
                "pending": bridge.sender.pending_len(),
                "note": "fresh admission closed; ordered payload preserved for retry",
            }),
        );
    }
    if bridge.sender.deduped != bridge.last_deduped {
        bridge.last_deduped = bridge.sender.deduped;
        log_json_at(
            log_path,
            serde_json::json!({
                "event": "input-inbox-dedupe",
                "deduped": bridge.sender.deduped,
            }),
        );
    }
}

/// Offer one control item through the bridge. Controls are infallibly
/// absorbed by the sender (queued, buffered in FIFO order, collapsed as exact
/// duplicates, or merged into the fixed-bound safety slots with clear
/// escalation), so this always returns true on the product path: nothing
/// safety-relevant is ever refused or dropped here. Snapshots never route
/// through this function (they rebuild from live state every tick until
/// accepted).
pub(super) fn offer_input_control(state: &mut TileLoop, item: InboxItem) -> bool {
    let log_path = state.log_path.clone();
    let Some(bridge) = state.input_bridge.as_mut() else {
        // Proof/legacy paths: same-thread hooks, direct sys call.
        match item {
            InboxItem::ClearEsc => crate::snapkey::sys::clear_esc_edge(),
            InboxItem::Snapshot(_) => {}
        }
        return true;
    };
    bridge.sender.flush();
    match bridge.sender.offer(item) {
        Ok(()) => {
            log_input_faults(bridge, &log_path);
            // Accepted control: wake the pump so deadline-idle waits apply
            // it promptly (auto-reset, coalesces bursts).
            bridge.channels.notify_work();
            true
        }
        Err(refused) => {
            // Unreachable for controls: only snapshots refuse (rebuildable),
            // and they route through `offer_input_snapshot`.
            debug_assert!(
                matches!(refused, InboxItem::Snapshot(_)),
                "controls are infallibly absorbed by the sender"
            );
            log_input_faults(bridge, &log_path);
            false
        }
    }
}

/// Offer the per-tick snapshot when content changed (or when a retry
/// is outstanding). Assigns policy generations (bumped
/// only on real policy change, reused on retry), stages the config-change
/// ack, and skips redundant offers so unchanged ticks send nothing and never
/// flush classifier queues.
pub(super) fn offer_input_snapshot(
    state: &mut TileLoop,
    mut candidate: crate::input_handoff::InputSnapshot,
) {
    let log_path = state.log_path.clone();
    let Some(bridge) = state.input_bridge.as_mut() else {
        return;
    };
    // Refusal this turn (set in the `Err` arms below): after the flag
    // settle, nudge the pump so a stalled inbox drains promptly. Ordered
    // after the store so the prompted drain always observes the armed flag
    // (a drain slipping between the failed offer and the store can no
    // longer lose the wake); the pump's bounded contention retry covers a
    // contended prompted drain.
    let mut refused = false;
    match plan_snapshot_offer(bridge.last_snapshot.as_ref(), &candidate, bridge.retry_gen) {
        SnapshotOffer::Skip => {
            // No offer attempted, but sender payloads may still be stuck
            // from an earlier failed flush: arm and nudge together so the
            // retry does not wait for an unrelated wake.
            if bridge.sender.pending_len() + bridge.sender.safety_len() > 0 {
                bridge
                    .channels
                    .inbox_stalled
                    .store(true, std::sync::atomic::Ordering::SeqCst);
                bridge.channels.notify_work();
            }
            return;
        }
        SnapshotOffer::Retry(generation) => {
            // Reuse the armed generation for the current desired content
            // (the fence is already armed and `pending_ack` already pairs
            // it; re-arming is a no-op `fetch_max`). Accepted even when the
            // content equals `last_snapshot` (revert): the pump advances its
            // ack on apply even for equal content, clearing the fence.
            candidate.generation = generation;
            // Retry preserved sender payloads before the fresh snapshot (FIFO).
            let had = bridge.sender.pending_len() + bridge.sender.safety_len();
            bridge.sender.flush();
            if had > 0 {
                bridge.channels.notify_work();
            }
            let shared: SharedSnapshot = std::sync::Arc::new(candidate);
            match bridge
                .sender
                .offer(InboxItem::Snapshot(Arc::clone(&shared)))
            {
                Ok(()) => {
                    bridge.last_snapshot = Some(shared);
                    bridge.retry_gen = None;
                    bridge.channels.notify_work();
                }
                Err(_) => {
                    // Still stalled: the retry stays armed for the next tick.
                    // Fault counted by the sender. The post-settle nudge
                    // below prompts the pump drain that observes the flag.
                    refused = true;
                }
            }
        }
        SnapshotOffer::Fresh => {
            let policy_changed = bridge
                .last_snapshot
                .as_ref()
                .is_none_or(|last| !snapshot_policy_eq(last, &candidate));
            if policy_changed {
                bridge.published_gen += 1;
                bridge.pending_ack = Some(bridge.published_gen);
                // Arm the callback-visible fence before the inbox offer so fresh
                // callbacks refuse fail-closed even while the snapshot is queued.
                note_policy_published(bridge.published_gen);
            }
            candidate.generation = bridge.published_gen;
            // Retry preserved sender payloads before the fresh snapshot (FIFO).
            let had = bridge.sender.pending_len() + bridge.sender.safety_len();
            bridge.sender.flush();
            if had > 0 {
                bridge.channels.notify_work();
            }
            let shared: SharedSnapshot = std::sync::Arc::new(candidate);
            match bridge
                .sender
                .offer(InboxItem::Snapshot(Arc::clone(&shared)))
            {
                Ok(()) => {
                    bridge.last_snapshot = Some(shared);
                    bridge.channels.notify_work();
                }
                Err(_) => {
                    // Regenerable: map-only refusals rebuild next tick
                    // (last_snapshot unchanged, so change detection
                    // re-offers). A refused policy change arms the retry so
                    // the fence still pairs exactly once, even across a
                    // revert. Fault counted by the sender. The post-settle
                    // nudge below prompts the pump drain that observes the
                    // flag.
                    if policy_changed {
                        bridge.retry_gen = Some(bridge.published_gen);
                    }
                    refused = true;
                }
            }
        }
    }
    // Stalled-retry flag for the pump's capacity-freed wake: true while a
    // refused retry or sender payloads are outstanding, false once clear.
    // Self-maintaining every offer (no separate clear path to forget).
    bridge.channels.inbox_stalled.store(
        bridge.retry_gen.is_some() || bridge.sender.pending_len() + bridge.sender.safety_len() > 0,
        std::sync::atomic::Ordering::SeqCst,
    );
    if refused {
        bridge.channels.notify_work();
    }
    log_input_faults(bridge, &log_path);
}

/// Poll the pump's applied policy generation and log the config-change ack
/// with the true stale-drained count once per change.
pub(super) fn poll_input_ack(state: &mut TileLoop) {
    use std::sync::atomic::Ordering;
    let log_path = state.log_path.clone();
    let Some(bridge) = state.input_bridge.as_mut() else {
        return;
    };
    let applied = bridge.channels.applied_policy_gen.load(Ordering::SeqCst);
    if applied > bridge.applied_gen {
        bridge.applied_gen = applied;
    }
    if let Some(pending) = bridge.pending_ack
        && applied >= pending
        && bridge.last_ack_logged != pending
    {
        let drained = bridge.channels.acked_stale.load(Ordering::Relaxed);
        bridge.pending_ack = None;
        bridge.last_ack_logged = pending;
        log_json_at(
            &log_path,
            serde_json::json!({
                "event": "settings-keyboard-ack",
                "generation": pending,
                "applied": applied,
                "drained": drained,
            }),
        );
    }
}

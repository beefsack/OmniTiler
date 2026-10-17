#![forbid(unsafe_code)]
use super::*;

/// Fixed cap on distinct stalled-transport invalidated HWNDs held in the
/// safety slots. Overflow escalates conservatively to a full windrag clear
/// (see [`InboxSender`]): total outstanding safety state stays fixed at this
/// cap plus two clear flags, regardless of owner window churn.
pub const SAFETY_INVALIDATES_CAP: usize = 8;

/// Bounded owner -> input sender. Full is an explicit fault: snapshots are
/// refused for rebuild-and-retry later (never queued here), while every
/// invalidation/suspend-clear obligation merges into fixed-bound
/// safety-state slots (never refused, never dropped). Fresh admission closes
/// on the first fault and reopens once everything drains; no unbounded owner
/// backlog accumulates.
///
/// Slot bounds: at most [`SAFETY_INVALIDATES_CAP`] distinct invalidated HWNDs
/// plus one windrag-clear flag plus one Esc-clear flag, all fixed. Overflow
/// escalates to `ClearWindrag`, which is a proven superset of every
/// invalidation: it drops all origins on both hooks and disarms any armed
/// gesture with a Cancel while retaining the swallow-until-up obligation
/// (the suspend path already applies exactly this pair). Escalation only
/// over-disarms: fresh gestures fail closed until the next snapshot
/// republishes, and no stranded pair or lost suspend-clear is possible.
/// Exact duplicates collapse (counted). Slot order on expansion is
/// `ClearWindrag`, invalidates in arrival order, then `ClearEsc`: a reordered
/// invalidate-after-clear is a harmless redundant disarm (invalidations never
/// rearm; snapshots ahead in the FIFO applied first in order), never a lost
/// obligation.
#[derive(Debug)]
pub struct InboxSender {
    inbox: Arc<Inbox>,
    pending: VecDeque<InboxItem>,
    /// Stalled-transport invalidated HWNDs, arrival order, deduplicated.
    pub(super) invalidates: Vec<u64>,
    /// Stalled-transport suspend/teardown windrag clear.
    clear_windrag: bool,
    /// Stalled-transport suspend/teardown Esc clear.
    clear_esc: bool,
    /// Fresh snapshots/controls must not be admitted while true (inbox fault
    /// outstanding). Ordered accepted controls/config already in the inbox or
    /// pending are preserved and retried.
    pub admission_closed: bool,
    /// Explicit inbox-full faults observed (trace evidence).
    pub faults: u64,
    /// Exact-duplicate offers collapsed while pending (trace evidence).
    pub deduped: u64,
    /// Accepted `ClearWindrag` obligations observed (monotonic). The owner
    /// watches this to force a full snapshot republish after clears: a clear
    /// drops both hook origin maps while the owner's `last_snapshot` still
    /// carries the old origins, so without a forced republish the correct
    /// origins stay absent indefinitely under change-gated publication.
    /// Incremented once per non-duplicate accepted clear (direct or slotted,
    /// including overflow escalation); duplicate collapses never increment.
    windrag_cleared: u64,
}

impl InboxSender {
    /// New sender over a shared inbox.
    #[must_use]
    pub fn new(inbox: Arc<Inbox>) -> Self {
        Self {
            inbox,
            pending: VecDeque::with_capacity(INBOX_CAP),
            invalidates: Vec::new(),
            clear_windrag: false,
            clear_esc: false,
            admission_closed: false,
            faults: 0,
            deduped: 0,
            windrag_cleared: 0,
        }
    }

    /// Accepted `ClearWindrag` obligations observed so far (monotonic owner
    /// republish trigger; see the field docs).
    #[must_use]
    pub fn windrag_cleared(&self) -> u64 {
        self.windrag_cleared
    }

    /// Slotted safety obligations not yet expanded into the FIFO.
    #[must_use]
    pub fn safety_len(&self) -> usize {
        self.invalidates.len() + usize::from(self.clear_windrag) + usize::from(self.clear_esc)
    }

    /// Pop one slotted obligation in expansion order.
    fn next_slot_item(&mut self) -> Option<InboxItem> {
        if self.clear_windrag {
            self.clear_windrag = false;
            return Some(InboxItem::ClearWindrag);
        }
        if !self.invalidates.is_empty() {
            return Some(InboxItem::InvalidateWindrag(self.invalidates.remove(0)));
        }
        if self.clear_esc {
            self.clear_esc = false;
            return Some(InboxItem::ClearEsc);
        }
        None
    }

    /// Retain one safety obligation while stalled. Snapshots rebuild later
    /// (`Err`); every invalidation/suspend-clear merges into the fixed-bound
    /// slots (`Ok`, never dropped, never refused).
    fn offer_slotted(&mut self, item: InboxItem) -> Result<(), InboxItem> {
        match item {
            InboxItem::Snapshot(_) => Err(item),
            InboxItem::ClearWindrag => {
                self.clear_windrag = true;
                self.windrag_cleared = self.windrag_cleared.saturating_add(1);
                // Subsumed: older slotted invalidates add nothing to a clear
                // that disarms every gesture and drops every origin.
                self.invalidates.clear();
                Ok(())
            }
            InboxItem::ClearEsc => {
                self.clear_esc = true;
                Ok(())
            }
            InboxItem::InvalidateWindrag(hwnd) => {
                // Slot duplicates collapsed before admission/capacity checks,
                // so reaching here means a new distinct obligation.
                if self.invalidates.len() >= SAFETY_INVALIDATES_CAP {
                    // Fixed-bound escalation: a full windrag clear is a proven
                    // superset of every invalidation (all origins dropped,
                    // any armed gesture disarmed with Cancel, swallow
                    // retained). Conservative over-disarm only; the bound
                    // holds regardless of window churn.
                    self.clear_windrag = true;
                    self.windrag_cleared = self.windrag_cleared.saturating_add(1);
                    self.invalidates.clear();
                } else {
                    self.invalidates.push(hwnd);
                }
                Ok(())
            }
        }
    }

    /// True for whole-payload republishes (never subsume safety controls).
    fn is_snapshot(item: &InboxItem) -> bool {
        matches!(item, InboxItem::Snapshot(_))
    }

    /// True for safety controls (invalidations and suspend/teardown clears).
    fn is_safety(item: &InboxItem) -> bool {
        matches!(
            item,
            InboxItem::InvalidateWindrag(_) | InboxItem::ClearWindrag | InboxItem::ClearEsc
        )
    }

    /// Offer one item. `Ok` for everything except snapshots refused while
    /// stalled (rebuildable): controls are infallibly absorbed (queued,
    /// buffered in order, collapsed as exact duplicates, or merged into the
    /// fixed-bound safety slots with clear escalation). `Err` returns a
    /// refused snapshot intact. Accepted order is always FIFO: pending drains
    /// before new items, slots expand after pending. Nothing
    /// safety-relevant is ever dropped inside.
    pub fn offer(&mut self, item: InboxItem) -> Result<(), InboxItem> {
        // Duplicate collapse is lossless only with no intervening
        // opposite-kind payload between the preserved duplicate and this
        // offer. A snapshot between two equal invalidates/clears rebinds
        // (repullishes) the origin the first one dropped, so the newer
        // control is a fresh obligation that must be preserved; symmetrically
        // a control between two equal snapshots means the newer snapshot
        // rebinds what the control dropped. Pending is sender-visible;
        // queued inbox payloads are probed without waiting (contention fails
        // closed to "present", preserving the newest work).
        let snapshot_between =
            self.pending.iter().any(Self::is_snapshot) || self.inbox.has_snapshot();
        let safety_between = self.pending.iter().any(Self::is_safety) || self.inbox.has_safety();
        let dedupe_safe = if Self::is_snapshot(&item) {
            !safety_between
        } else {
            !snapshot_between
        };
        // Exact duplicates of preserved work collapse first (counted): the
        // work is already ordered ahead, so collapsing is lossless even
        // while admission is closed.
        if dedupe_safe && self.pending.iter().any(|staged| *staged == item) {
            self.deduped += 1;
            return Ok(());
        }
        // Slot duplicates collapse the same way (counted).
        if dedupe_safe {
            match &item {
                InboxItem::ClearEsc if self.clear_esc => {
                    self.deduped += 1;
                    return Ok(());
                }
                InboxItem::ClearWindrag if self.clear_windrag => {
                    self.deduped += 1;
                    return Ok(());
                }
                InboxItem::InvalidateWindrag(hwnd) if self.invalidates.contains(hwnd) => {
                    self.deduped += 1;
                    return Ok(());
                }
                _ => {}
            }
        }
        // Every non-duplicate accepted clear is a fresh origin-drop
        // obligation the owner must republish after (see `windrag_cleared`).
        let accepted_clear = matches!(item, InboxItem::ClearWindrag);
        if self.admission_closed {
            let outcome = self.offer_slotted(item);
            // `offer_slotted` already counted slotted/escalated clears; a
            // direct clear can never reach here refused, so nothing further.
            return outcome;
        }
        if !self.pending.is_empty() {
            if self.pending.len() >= INBOX_CAP {
                self.faults += 1;
                self.admission_closed = true;
                return self.offer_slotted(item);
            }
            self.pending.push_back(item);
            if accepted_clear {
                self.windrag_cleared = self.windrag_cleared.saturating_add(1);
            }
            return Ok(());
        }
        let mut slot = None;
        if self.inbox.try_push(item, &mut slot) {
            if accepted_clear {
                self.windrag_cleared = self.windrag_cleared.saturating_add(1);
            }
            return Ok(());
        }
        let item = slot.expect("refused item preserved");
        if self.pending.len() >= INBOX_CAP {
            self.faults += 1;
            self.admission_closed = true;
            return self.offer_slotted(item);
        }
        let buffered_clear = matches!(item, InboxItem::ClearWindrag);
        self.pending.push_back(item);
        if buffered_clear {
            self.windrag_cleared = self.windrag_cleared.saturating_add(1);
        }
        self.faults += 1;
        self.admission_closed = true;
        Ok(())
    }

    /// Retry pending items in order, then expand slotted safety obligations
    /// behind them. Returns items still outstanding (pending plus slots).
    /// Admission reopens once fully drained.
    pub fn flush(&mut self) -> usize {
        loop {
            if self.pending.is_empty() {
                let Some(next) = self.next_slot_item() else {
                    break;
                };
                self.pending.push_back(next);
            }
            let item = self.pending.pop_front().expect("pending item");
            let mut slot = None;
            if self.inbox.try_push(item, &mut slot) {
                continue;
            }
            self.pending
                .push_front(slot.expect("refused item preserved"));
            self.faults += 1;
            return self.pending.len() + self.safety_len();
        }
        self.admission_closed = false;
        0
    }

    /// Pending item count.
    #[must_use]
    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }
}

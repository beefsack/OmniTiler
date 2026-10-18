#![forbid(unsafe_code)]
use super::*;

/// Bounded owner -> input sender. Full is an explicit fault: snapshots are
/// refused for rebuild-and-retry later (never queued here), while the
/// suspend/teardown Esc-clear obligation merges into a fixed-bound
/// safety-state slot (never refused, never dropped). Fresh admission closes
/// on the first fault and reopens once everything drains; no unbounded owner
/// backlog accumulates.
///
/// Slot bounds: one Esc-clear flag, fixed. Exact duplicates collapse
/// (counted).
#[derive(Debug)]
pub struct InboxSender {
    inbox: Arc<Inbox>,
    pending: VecDeque<InboxItem>,
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
}

impl InboxSender {
    /// New sender over a shared inbox.
    #[must_use]
    pub fn new(inbox: Arc<Inbox>) -> Self {
        Self {
            inbox,
            pending: VecDeque::with_capacity(INBOX_CAP),
            clear_esc: false,
            admission_closed: false,
            faults: 0,
            deduped: 0,
        }
    }

    /// Slotted safety obligations not yet expanded into the FIFO.
    #[must_use]
    pub fn safety_len(&self) -> usize {
        usize::from(self.clear_esc)
    }

    /// Pop one slotted obligation in expansion order.
    fn next_slot_item(&mut self) -> Option<InboxItem> {
        if self.clear_esc {
            self.clear_esc = false;
            return Some(InboxItem::ClearEsc);
        }
        None
    }

    /// Retain one safety obligation while stalled. Snapshots rebuild later
    /// (`Err`); the suspend/teardown Esc clear merges into the fixed-bound
    /// slot (`Ok`, never dropped, never refused).
    fn offer_slotted(&mut self, item: InboxItem) -> Result<(), InboxItem> {
        match item {
            InboxItem::Snapshot(_) => Err(item),
            InboxItem::ClearEsc => {
                self.clear_esc = true;
                Ok(())
            }
        }
    }

    /// True for whole-payload republishes (never subsume safety controls).
    fn is_snapshot(item: &InboxItem) -> bool {
        matches!(item, InboxItem::Snapshot(_))
    }

    /// True for safety controls (suspend/teardown clears).
    fn is_safety(item: &InboxItem) -> bool {
        matches!(item, InboxItem::ClearEsc)
    }

    /// Offer one item. `Ok` for everything except snapshots refused while
    /// stalled (rebuildable): controls are infallibly absorbed (queued,
    /// buffered in order, or collapsed as exact duplicates). `Err` returns a
    /// refused snapshot intact. Accepted order is always FIFO: pending drains
    /// before new items, slots expand after pending. Nothing
    /// safety-relevant is ever dropped inside.
    pub fn offer(&mut self, item: InboxItem) -> Result<(), InboxItem> {
        // Duplicate collapse is lossless only with no intervening
        // opposite-kind payload between the preserved duplicate and this
        // offer. A snapshot between two equal clears rebinds (republishes)
        // the state the first one cleared, so the newer control is a fresh
        // obligation that must be preserved; symmetrically
        // a control between two equal snapshots means the newer snapshot
        // rebinds what the control cleared. Pending is sender-visible;
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
                _ => {}
            }
        }
        if self.admission_closed {
            let outcome = self.offer_slotted(item);
            // `offer_slotted` already counted slotted clears; a
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
            return Ok(());
        }
        let mut slot = None;
        if self.inbox.try_push(item, &mut slot) {
            return Ok(());
        }
        let item = slot.expect("refused item preserved");
        if self.pending.len() >= INBOX_CAP {
            self.faults += 1;
            self.admission_closed = true;
            return self.offer_slotted(item);
        }
        self.pending.push_back(item);
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

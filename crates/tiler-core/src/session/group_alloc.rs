//! Generation-shared monotonic group allocator.
//!
//! Every production group mint in the session draws from one
//! generation-owned counter. The counter is shared through
//! `Arc<AtomicU64>` clones so discarded whole-session clones, failed
//! fits/trials, rolled-back placements, and paired commands all burn
//! serials: a burned serial is never reissued, and a removed group id is
//! therefore never minted again within the generation. Only a fresh
//! generation (new allocator) may restart the sequence.
//!
//! Minted ids carry the serial verbatim (`grp-{serial}`). Against
//! imported static/test trees holding opaque ids, minting skips colliding
//! serials (burning them, never reissuing) so a fresh mint never duplicates
//! a live id. Exhaustion (`u64::MAX` reached) fails closed (`None`); the
//! counter never wraps.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::directional::NodeId;

/// Generation-shared monotonic group-id allocator.
///
/// `Clone` shares the underlying counter (burns survive discarded clones).
/// `Default`/`new` starts a fresh generation sequence at zero.
#[derive(Debug, Clone, Default)]
pub(crate) struct GroupAllocator {
    next: Arc<AtomicU64>,
}

impl GroupAllocator {
    /// Fresh generation allocator starting at serial zero.
    #[must_use]
    pub(crate) fn new() -> Self {
        Self {
            next: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Draw the next serial, burning it even when the caller discards the
    /// placement. Fails closed (`None`) once the sequence is exhausted;
    /// never wraps.
    fn next_serial(&self) -> Option<u64> {
        loop {
            let current = self.next.load(Ordering::SeqCst);
            if current == u64::MAX {
                return None;
            }
            match self.next.compare_exchange(
                current,
                current.saturating_add(1),
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => return Some(current),
                Err(_) => continue,
            }
        }
    }

    /// Mint one fresh group id that avoids every id in `existing`.
    /// Colliding serials burn (never reissued); exhaustion fails closed.
    #[must_use]
    pub(crate) fn mint(&self, existing: &BTreeSet<NodeId>) -> Option<NodeId> {
        loop {
            let serial = self.next_serial()?;
            let candidate = NodeId(format!("grp-{serial}"));
            if !existing.contains(&candidate) {
                return Some(candidate);
            }
        }
    }

    /// Current high-water mark (tests only): the next serial that would
    /// be drawn.
    #[cfg(test)]
    #[must_use]
    pub(crate) fn next_value(&self) -> u64 {
        self.next.load(Ordering::SeqCst)
    }

    /// Test-only allocator starting at an arbitrary serial (exhaustion
    /// probes). Production allocators always start fresh at zero.
    #[cfg(test)]
    pub(crate) fn test_with_next(next: u64) -> Self {
        Self {
            next: Arc::new(AtomicU64::new(next)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mint_advances_and_never_reissues() {
        let alloc = GroupAllocator::new();
        let mut existing = BTreeSet::new();
        let first = alloc.mint(&existing).expect("mints");
        existing.insert(first.clone());
        let second = alloc.mint(&existing).expect("mints");
        assert_ne!(first, second);
        // A removed id still collides: the mint below must skip it.
        existing.remove(&first);
        let mut live = BTreeSet::new();
        live.insert(first.clone());
        let third = alloc.mint(&live).expect("mints");
        assert_ne!(third, first);
        assert_ne!(third, second);
    }

    #[test]
    fn clones_share_one_counter() {
        let alloc = GroupAllocator::new();
        let fork = alloc.clone();
        let mut existing = BTreeSet::new();
        let first = fork.mint(&existing).expect("mints");
        existing.insert(first);
        // The discarded fork burned serial zero; the owner continues past it.
        let second = alloc.mint(&existing).expect("mints");
        assert_eq!(second, NodeId("grp-1".to_owned()));
    }

    #[test]
    fn colliding_opaque_ids_burn_without_reissue() {
        let alloc = GroupAllocator::new();
        let mut existing = BTreeSet::from([
            NodeId("grp-0".to_owned()),
            NodeId("grp-1".to_owned()),
            NodeId("grp-2".to_owned()),
        ]);
        let minted = alloc.mint(&existing).expect("mints");
        assert_eq!(minted, NodeId("grp-3".to_owned()));
        existing.insert(minted);
        assert_eq!(alloc.next_value(), 4);
    }
}

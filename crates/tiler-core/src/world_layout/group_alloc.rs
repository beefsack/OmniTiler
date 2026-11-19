//! Generation-shared monotonic group allocator.
//!
//! Every production group mint in the layout draws from one
//! generation-owned counter. The counter is shared through
//! `Arc<AtomicU64>` clones so discarded working clones, failed
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

#[derive(Debug, Default)]
struct AllocatorCounters {
    group_next: AtomicU64,
    leaf_next: AtomicU64,
}

/// Generation-shared monotonic group-id allocator.
///
/// `Clone` shares the underlying counters (burns survive discarded clones).
/// `Default`/`new` starts a fresh generation sequence at zero.
#[derive(Debug, Clone, Default)]
pub(crate) struct GroupAllocator {
    counters: Arc<AllocatorCounters>,
}

impl GroupAllocator {
    /// Fresh generation allocator starting at serial zero.
    #[must_use]
    pub(crate) fn new() -> Self {
        Self {
            counters: Arc::new(AllocatorCounters::default()),
        }
    }

    /// Draw the next serial from `counter`, burning it even when the caller
    /// discards the placement. Fails closed (`None`) once the sequence is
    /// exhausted; never wraps.
    fn next_serial_from(counter: &AtomicU64) -> Option<u64> {
        loop {
            let current = counter.load(Ordering::SeqCst);
            if current == u64::MAX {
                return None;
            }
            match counter.compare_exchange(
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

    /// Mint one fresh id with `prefix` that avoids every id in `existing`.
    /// Colliding serials burn (never reissued); exhaustion fails closed.
    fn mint_prefixed(
        counter: &AtomicU64,
        prefix: &str,
        existing: &BTreeSet<NodeId>,
    ) -> Option<NodeId> {
        loop {
            let serial = Self::next_serial_from(counter)?;
            let candidate = NodeId(format!("{prefix}{serial}"));
            if !existing.contains(&candidate) {
                return Some(candidate);
            }
        }
    }

    /// Mint one fresh group id that avoids every id in `existing`.
    /// Colliding serials burn (never reissued); exhaustion fails closed.
    #[must_use]
    pub(crate) fn mint(&self, existing: &BTreeSet<NodeId>) -> Option<NodeId> {
        Self::mint_prefixed(&self.counters.group_next, "grp-", existing)
    }

    /// Mint one fresh leaf id independently of group serials, avoiding every
    /// id in `existing`. Colliding serials burn; exhaustion fails closed.
    #[must_use]
    pub(crate) fn mint_leaf(&self, existing: &BTreeSet<NodeId>) -> Option<NodeId> {
        Self::mint_prefixed(&self.counters.leaf_next, "leaf-", existing)
    }

    /// Current high-water mark (tests only): the next serial that would
    /// be drawn.
    #[cfg(test)]
    #[must_use]
    pub(crate) fn next_value(&self) -> u64 {
        self.counters.group_next.load(Ordering::SeqCst)
    }

    /// Current leaf high-water mark (tests only): the next leaf serial that
    /// would be drawn.
    #[cfg(test)]
    #[must_use]
    pub(crate) fn leaf_next_value(&self) -> u64 {
        self.counters.leaf_next.load(Ordering::SeqCst)
    }

    /// Test-only allocator starting at an arbitrary serial (exhaustion
    /// probes). Production allocators always start fresh at zero.
    #[cfg(test)]
    pub(crate) fn test_with_next(next: u64) -> Self {
        Self {
            counters: Arc::new(AllocatorCounters {
                group_next: AtomicU64::new(next),
                leaf_next: AtomicU64::new(0),
            }),
        }
    }

    /// Test-only allocator starting at an arbitrary leaf serial (exhaustion
    /// probes).
    #[cfg(test)]
    pub(crate) fn test_with_leaf_next(leaf_next: u64) -> Self {
        Self {
            counters: Arc::new(AllocatorCounters {
                group_next: AtomicU64::new(0),
                leaf_next: AtomicU64::new(leaf_next),
            }),
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

    #[test]
    fn mint_leaf_is_independent_of_group_serials() {
        let alloc = GroupAllocator::new();
        let mut existing = BTreeSet::new();
        let leaf0 = alloc.mint_leaf(&existing).expect("mints leaf-0");
        assert_eq!(leaf0, NodeId("leaf-0".to_owned()));
        existing.insert(leaf0);
        let leaf1 = alloc.mint_leaf(&existing).expect("mints leaf-1");
        assert_eq!(leaf1, NodeId("leaf-1".to_owned()));
        assert_eq!(
            alloc.next_value(),
            0,
            "leaf minting must not spend group serials"
        );
        assert_eq!(alloc.leaf_next_value(), 2);

        let grp0 = alloc.mint(&existing).expect("mints grp-0");
        assert_eq!(grp0, NodeId("grp-0".to_owned()));
        assert_eq!(
            alloc.leaf_next_value(),
            2,
            "group minting must not spend leaf serials"
        );
    }

    #[test]
    fn mint_leaf_exhaustion_fails_closed() {
        let alloc = GroupAllocator::test_with_leaf_next(u64::MAX);
        let existing = BTreeSet::new();
        assert_eq!(alloc.mint_leaf(&existing), None);
        assert_eq!(alloc.leaf_next_value(), u64::MAX);
    }
}

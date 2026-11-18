use std::collections::BTreeSet;

use crate::directional::{NodeId, WindowId};
use crate::session::DomainKey;

use super::state::{DomainLayout, WorldLayout};
use super::topology::{self, tree_has_leaf};

/// Leaf and reciprocal-link membership for one domain's focus-pruning batch,
/// built once instead of resolving every stack/history entry independently
/// (the former path cloned and walked a whole tree and scanned every link per
/// entry, quadratic in report windows).
struct FocusPruneIndex {
    valid_leaves: BTreeSet<NodeId>,
    linked_leaves: BTreeSet<NodeId>,
}

impl FocusPruneIndex {
    fn new(domain: &DomainLayout) -> Self {
        let valid_leaves = domain
            .tree
            .as_ref()
            .map(|tree| topology::collect_leaves(tree).into_iter().collect())
            .unwrap_or_default();
        let linked_leaves = domain.leaves.values().cloned().collect();
        Self {
            valid_leaves,
            linked_leaves,
        }
    }

    fn resolves(&self, leaf: &NodeId) -> bool {
        self.valid_leaves.contains(leaf) && self.linked_leaves.contains(leaf)
    }
}

/// Leaf-only resolution scoped to a single retained domain. A leaf resolves
/// only when it is a real tree leaf and has a reciprocal window binding.
pub(super) fn domain_focus_resolves(domain: &DomainLayout, leaf: &NodeId) -> bool {
    let Some(tree) = &domain.tree else {
        return false;
    };
    tree_has_leaf(tree, leaf) && domain.leaves.values().any(|bound| bound == leaf)
}

/// Drop leaves that no longer resolve from a recency vector, then append a
/// focus leaf. The stack appends unconditionally (it is the removal-fallback
/// MRU); accepted history appends only when the leaf resolves. Native
/// convergence prunes history without appending its observed focus.
fn prune_then_append(
    leaves: &mut Vec<NodeId>,
    focus: Option<&NodeId>,
    index: &FocusPruneIndex,
    guarded: bool,
) {
    leaves.retain(|leaf| index.resolves(leaf));
    if let Some(leaf) = focus {
        leaves.retain(|known| known != leaf);
        if !guarded || index.resolves(leaf) {
            leaves.push(leaf.clone());
        }
    }
}

/// Domain-level focus sync used by both ordinary activation and the local
/// orientation candidate. Resolves `window` to its bound leaf in this domain,
/// then sets focused leaf and demote/re-appends it on the MRU stack. Accepted
/// focus history is not touched: observed focus never reorders it. Callers
/// validate topology first. Returns the resolved leaf, or `None` when the
/// window is not bound here (leaving the domain untouched).
pub(super) fn sync_domain_focus_from_window(
    domain: &mut DomainLayout,
    window: &WindowId,
) -> Option<NodeId> {
    let leaf = domain.leaves.get(window)?.clone();
    let index = FocusPruneIndex::new(domain);
    let mut stack = domain.focus_stack.clone();
    prune_then_append(&mut stack, Some(&leaf), &index, false);
    domain.focused_leaf = Some(leaf.clone());
    domain.focus_stack = stack;
    Some(leaf)
}

/// Exact focus settle tail for one domain, callable on a local candidate.
/// Prunes stale leaves from `stack_base`, then unconditionally demotes and
/// re-appends `focused_leaf` on the MRU stack, prunes accepted history without
/// appending observed focus, and advances last-active only for a resolving
/// leaf (otherwise `current_last_active` is kept). No other domain is read.
pub(super) fn settle_domain_focus_tail(
    domain: &mut DomainLayout,
    stack_base: &[NodeId],
    focused_leaf: Option<NodeId>,
    current_last_active: Option<&NodeId>,
) {
    let index = FocusPruneIndex::new(domain);
    let mut stack = stack_base.to_vec();
    prune_then_append(&mut stack, focused_leaf.as_ref(), &index, false);
    domain.focus_history.retain(|leaf| index.resolves(leaf));
    let last_active = if let Some(leaf) = focused_leaf.as_ref()
        && index.resolves(leaf)
    {
        Some(leaf.clone())
    } else {
        current_last_active.cloned()
    };
    domain.focused_leaf = focused_leaf;
    domain.focus_stack = stack;
    domain.last_active = last_active;
}

/// Exact accepted-focus commit tail for one domain: prunes stale leaves from
/// `stack_base` and unconditionally demotes/re-appends `focused_leaf` on the
/// MRU stack, prunes accepted history while demoting/re-appending a resolving
/// `focused_leaf`, and advances last-active when `focused_leaf` resolves.
pub(super) fn settle_domain_accepted_focus_tail(
    domain: &mut DomainLayout,
    stack_base: &[NodeId],
    focused_leaf: Option<NodeId>,
    current_last_active: Option<&NodeId>,
) {
    let index = FocusPruneIndex::new(domain);
    let mut stack = stack_base.to_vec();
    prune_then_append(&mut stack, focused_leaf.as_ref(), &index, false);
    let mut history = domain.focus_history.clone();
    prune_then_append(&mut history, focused_leaf.as_ref(), &index, true);
    let last_active = if let Some(leaf) = focused_leaf.as_ref()
        && index.resolves(leaf)
    {
        Some(leaf.clone())
    } else {
        current_last_active.cloned()
    };
    domain.focused_leaf = focused_leaf;
    domain.focus_stack = stack;
    domain.focus_history = history;
    domain.last_active = last_active;
}

impl WorldLayout {
    /// Whether a leaf resolves to a linked leaf in its domain. Leaf-only
    /// traversal keeps a malformed group id that repeats a leaf id from
    /// resolving as a leaf.
    #[must_use]
    pub(crate) fn focus_resolves(&self, domain: &DomainKey, leaf: &NodeId) -> bool {
        self.domain(domain)
            .is_some_and(|layout| domain_focus_resolves(layout, leaf))
    }

    /// Last-active leaf while it still resolves in its domain.
    #[must_use]
    pub(crate) fn remembered_leaf(&self, target: &DomainKey) -> Option<NodeId> {
        let leaf = self.domain(target)?.last_active.clone()?;
        self.focus_resolves(target, &leaf).then_some(leaf)
    }

    /// Newest-first focus history: last-active leaf first, then the domain
    /// MRU stack newest-first, deduplicated. A stale head simply misses the
    /// candidate set and falls through to recency/overlap/DFS order.
    #[must_use]
    pub(crate) fn cross_output_history_newest_first(&self, target: &DomainKey) -> Vec<NodeId> {
        let mut out: Vec<NodeId> = Vec::new();
        if let Some(domain) = self.domain(target) {
            if let Some(leaf) = &domain.last_active {
                out.push(leaf.clone());
            }
            for leaf in domain.focus_stack.iter().rev() {
                if !out.contains(leaf) {
                    out.push(leaf.clone());
                }
            }
        }
        out
    }

    /// View-return focus candidates: remembered last-active leaf, then the
    /// domain MRU stack in recency order, then every linked leaf in layout
    /// (child) order. Only linked leaves are named.
    #[must_use]
    pub(crate) fn view_return_candidates(&self, target: &DomainKey) -> Vec<NodeId> {
        let mut out: Vec<NodeId> = Vec::new();
        if let Some(leaf) = self.remembered_leaf(target)
            && !out.contains(&leaf)
        {
            out.push(leaf);
        }
        let Some(domain) = self.domain(target) else {
            return out;
        };
        let index = FocusPruneIndex::new(domain);
        for leaf in domain.focus_stack.iter().rev() {
            if !out.contains(leaf) && index.resolves(leaf) {
                out.push(leaf.clone());
            }
        }
        if let Some(tree) = &domain.tree {
            for leaf in topology::collect_leaves(tree) {
                if !out.contains(&leaf) && index.resolves(&leaf) {
                    out.push(leaf);
                }
            }
        }
        out
    }

    /// Focus-stack recency after pruning stale/domain-mismatched leaves and
    /// appending an observed focus leaf. The append is unconditional here so
    /// an unresolved committed focus still seeds the fallback stack.
    #[cfg(test)]
    #[must_use]
    pub(super) fn updated_focus_stack_on(
        &self,
        key: &DomainKey,
        base: &[NodeId],
        focus: Option<&NodeId>,
    ) -> Vec<NodeId> {
        let Some(domain) = self.domain(key) else {
            return focus.cloned().into_iter().collect();
        };
        let index = FocusPruneIndex::new(domain);
        let mut next = base.to_vec();
        prune_then_append(&mut next, focus, &index, false);
        next
    }

    /// Accepted focus-history recency after pruning. Only a resolving accepted
    /// focus is appended; native convergence must prune without appending.
    #[cfg(test)]
    #[must_use]
    pub(super) fn updated_focus_history_on(
        &self,
        key: &DomainKey,
        base: &[NodeId],
        focus: Option<&NodeId>,
    ) -> Vec<NodeId> {
        let Some(domain) = self.domain(key) else {
            return Vec::new();
        };
        let index = FocusPruneIndex::new(domain);
        let mut next = base.to_vec();
        prune_then_append(&mut next, focus, &index, true);
        next
    }

    /// Last-active slot after an optional resolving focus leaf: the leaf when
    /// it resolves, otherwise the supplied `current`. Uses borrowed leaf-only
    /// matching, no batch index.
    #[must_use]
    pub(super) fn updated_last_active_on(
        &self,
        key: &DomainKey,
        focus: Option<&NodeId>,
        current: Option<&NodeId>,
    ) -> Option<NodeId> {
        if let Some(leaf) = focus
            && self.focus_resolves(key, leaf)
        {
            Some(leaf.clone())
        } else {
            current.cloned()
        }
    }

    /// Synchronize retained focus from an ordinary activation of a known
    /// tiled window in an existing domain. Only the focused leaf and its MRU
    /// stack move; topology, membership, revision and accepted focus history
    /// are untouched. Native observed focus MUST NOT reorder accepted
    /// history. Fails closed on unknown domains, malformed topology or an
    /// unbound/cross-domain window.
    pub(crate) fn sync_focus_from_window(&mut self, domain: &DomainKey, window: &WindowId) -> bool {
        let Some(layout) = self.domain(domain) else {
            return false;
        };
        if !topology::validate_domain_topology(domain, layout) {
            return false;
        }
        let layout = self.domains.get_mut(domain).expect("domain checked above");
        sync_domain_focus_from_window(layout, window).is_some()
    }

    /// Remember `window` as the last-active focus of `domain`, recording
    /// accepted focus history and advancing the last-active leaf. Same
    /// binding guards as [`Self::sync_focus_from_window`].
    pub(crate) fn remember_focus(&mut self, domain: &DomainKey, window: &WindowId) -> bool {
        if !self.sync_focus_from_window(domain, window) {
            return false;
        }
        let _ = self.record_focus_history(domain, window);
        let last_active = self.updated_last_active_on(
            domain,
            self.domain(domain).and_then(|d| d.focused_leaf.as_ref()),
            self.domain(domain).and_then(|d| d.last_active.as_ref()),
        );
        if let Some(layout) = self.domains.get_mut(domain) {
            layout.last_active = last_active;
        }
        true
    }

    /// Fail-closed gate for world-owned canonical focus memory: a non-empty
    /// known lifetime (tiled link or exception) in the validated target
    /// domain. Unrelated domains never gate this target.
    #[must_use]
    pub(crate) fn validates_memory_target(&self, key: &DomainKey, window: &WindowId) -> bool {
        if window.0.is_empty() {
            return false;
        }
        let Some(domain) = self.domain(key) else {
            return false;
        };
        if !topology::validate_domain_topology(key, domain) {
            return false;
        }
        domain.leaves.contains_key(window) || domain.exceptions.contains_key(window)
    }

    /// Record `window` as the most recently accepted logical tiled focus in
    /// `domain`, without moving the focused leaf, MRU stack or last-active
    /// slot. Only resolving linked leaves are appended.
    pub(crate) fn record_focus_history(&mut self, domain: &DomainKey, window: &WindowId) -> bool {
        let Some(layout) = self.domain(domain) else {
            return false;
        };
        let Some(leaf) = layout.leaves.get(window).cloned() else {
            return false;
        };
        if !self.focus_resolves(domain, &leaf) {
            return false;
        }
        let index = FocusPruneIndex::new(layout);
        let mut history = layout.focus_history.clone();
        prune_then_append(&mut history, Some(&leaf), &index, true);
        if let Some(layout) = self.domains.get_mut(domain) {
            layout.focus_history = history;
        }
        true
    }

    /// Domain-level accepted-history prune without appending observed focus.
    pub(super) fn prune_domain_focus_history(domain: &mut DomainLayout) {
        let index = FocusPruneIndex::new(domain);
        domain.focus_history.retain(|leaf| index.resolves(leaf));
    }

    /// Apply the domain-scoped convergence focus tail: set the focused leaf,
    /// prune then unconditionally demote/re-append the selected leaf on the
    /// MRU stack, prune accepted history without appending observed focus,
    /// and advance last-active only for a resolving leaf. `stack_base` is the
    /// captured pre-convergence stack. No other domain is mutated.
    pub(super) fn settle_focus_bookkeeping(
        &mut self,
        key: &DomainKey,
        stack_base: &[NodeId],
        focused_leaf: Option<NodeId>,
    ) {
        let Some(domain) = self.domains.get_mut(key) else {
            return;
        };
        let current_last_active = domain.last_active.clone();
        settle_domain_focus_tail(
            domain,
            stack_base,
            focused_leaf,
            current_last_active.as_ref(),
        );
    }

    /// Drop automatic-fixed and explicit-tile overrides in this domain for
    /// windows no longer linked, excepted or restore-slotted. A reused id
    /// classifies again on next admission.
    /// Domain-level fixed-size provenance prune for a local candidate.
    pub(super) fn prune_domain_fixed_size_state(domain: &mut DomainLayout) {
        domain.automatic_fixed.retain(|id| {
            domain.leaves.contains_key(id)
                || domain.exceptions.contains_key(id)
                || domain.minimized_slots.contains_key(id)
        });
        domain.fixed_tile_override.retain(|id| {
            domain.leaves.contains_key(id)
                || domain.exceptions.contains_key(id)
                || domain.minimized_slots.contains_key(id)
        });
    }

    /// Drop capacity-float provenance for windows no longer linked or
    /// excepted (minimize slots do not carry capacity provenance), and
    /// admission order for windows no longer linked, excepted or
    /// restore-slotted. Domain-scoped, like the local operation that calls it.
    /// Domain-level capacity provenance prune for a local candidate.
    pub(super) fn prune_domain_capacity_state(domain: &mut DomainLayout) {
        domain
            .capacity_floats
            .retain(|id| domain.leaves.contains_key(id) || domain.exceptions.contains_key(id));
        domain.admission_order.retain(|id, _| {
            domain.leaves.contains_key(id)
                || domain.exceptions.contains_key(id)
                || domain.minimized_slots.contains_key(id)
        });
    }
}

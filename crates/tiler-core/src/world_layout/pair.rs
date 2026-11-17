//! Private canonical two-domain pair assembly, split, and atomic install.
//!
//! `PairCandidate` owns exactly two affected [`DomainLayout`] candidates plus
//! minimal ephemeral pair metadata (descriptors, shared revision/focus, target
//! inverse-id restore map, and trial admission high-water). Policy, allocator,
//! and config remain owned solely by [`WorldLayout`].

use std::collections::{BTreeMap, BTreeSet};

use crate::directional::{Direction, Node, NodeId, OutputId, WindowId};
use crate::geometry::Rect;
use crate::session::{DomainKey, OutputDomain};

use super::adoption::domain_is_empty;
use super::focus::domain_focus_resolves;
use super::state::{DomainException, DomainLayout, DomainRestoreSlot, WorldLayout};
use super::topology::{
    collect_node_ids, validate_domain_topology, validate_domain_topology_with_focus,
};

/// Typed pre-state failure for canonical pair assembly, split, and install.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PairAssemblyFailure {
    SourceUnavailable,
    TargetUnusable,
    DomainMismatch,
    DuplicateState,
}

impl PairAssemblyFailure {
    /// Stable diagnostic token matching the legacy engine seam.
    #[must_use]
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::SourceUnavailable => "canonical-source-unavailable",
            Self::TargetUnusable => "canonical-pair-unusable",
            Self::DomainMismatch => "canonical-pair-domain-mismatch",
            Self::DuplicateState => "canonical-pair-duplicate-state",
        }
    }
}

/// Ephemeral two-domain pair candidate assembled from retained domains.
#[derive(Debug, Clone)]
pub(super) struct PairCandidate {
    pub(super) source_domain: OutputDomain,
    pub(super) target_domain: OutputDomain,
    pub(super) source: DomainLayout,
    pub(super) target: DomainLayout,
    pub(super) pair_revision: u64,
    pub(super) focused_domain: Option<DomainKey>,
    pub(super) focused_leaf: Option<NodeId>,
    pub(super) target_restore: BTreeMap<NodeId, NodeId>,
    pub(super) admission_seq_next: u64,
}

impl PairCandidate {
    #[must_use]
    pub(super) fn source_key(&self) -> DomainKey {
        self.source_domain.key()
    }

    #[must_use]
    pub(super) fn target_key(&self) -> DomainKey {
        self.target_domain.key()
    }

    /// Forget any minimized restore slot for `window` on either domain in the
    /// pair without advancing `pair_revision`.
    pub(super) fn forget_minimized_slot(&mut self, window: &WindowId) {
        self.source.minimized_slots.remove(window);
        self.target.minimized_slots.remove(window);
    }

    /// Source-equivalent pair topology guard (`session.rs::validate_topology`):
    /// validates each domain's tree/shares/bindings against pair-global focus
    /// (never enforcing stale embedded per-domain focus), enforces globally
    /// unique linked leaf IDs across both domains while allowing repeated group
    /// IDs across domains, and checks cross-domain state disjointness.
    #[must_use]
    pub(super) fn validate_pair_topology(&self) -> bool {
        let source_key = self.source_key();
        let target_key = self.target_key();
        if source_key == target_key
            || !self.source_domain.validate()
            || !self.target_domain.validate()
        {
            return false;
        }
        let (source_focus, target_focus) = match (&self.focused_domain, &self.focused_leaf) {
            (None, None) => (None, None),
            (Some(domain), Some(leaf)) if domain == &source_key => (Some(leaf), None),
            (Some(domain), Some(leaf)) if domain == &target_key => (None, Some(leaf)),
            _ => return false,
        };
        if !validate_domain_topology_with_focus(&source_key, &self.source, source_focus)
            || !validate_domain_topology_with_focus(&target_key, &self.target, target_focus)
        {
            return false;
        }
        if !domains_disjoint(&self.source, &self.target) {
            return false;
        }
        let target_leaves: BTreeSet<&NodeId> = self.target.leaves.values().collect();
        !self
            .source
            .leaves
            .values()
            .any(|leaf| target_leaves.contains(leaf))
    }

    /// Split the pair back into independent domain-scoped [`DomainLayout`]
    /// candidates, preserving the existing outer gaps on each domain.
    pub(super) fn split(
        &self,
    ) -> Result<(DomainLayout, Option<DomainLayout>), PairAssemblyFailure> {
        self.split_with_outer_gaps(self.source.outer_gap, self.target.outer_gap)
    }

    /// Split the pair back into independent domain-scoped [`DomainLayout`]
    /// candidates with explicit output outer gaps applied before validation.
    /// Borrows `&self` so a converged working view can be installed without
    /// consuming or recanonicalizing its temporary pair ids.
    pub(super) fn split_with_outer_gaps(
        &self,
        source_outer_gap: i32,
        target_outer_gap: i32,
    ) -> Result<(DomainLayout, Option<DomainLayout>), PairAssemblyFailure> {
        let source_key = self.source_key();
        let target_key = self.target_key();

        if !self.validate_pair_topology() {
            return Err(PairAssemblyFailure::TargetUnusable);
        }

        let (source_focus, target_focus) = self.split_pair_focus(&source_key, &target_key);

        let restored_target_ids: BTreeSet<NodeId> = self.target_restore.values().cloned().collect();
        let mut moved_target_remap = BTreeMap::new();
        let mut occupied = domain_all_node_ids(&self.source);
        occupied.extend(domain_all_node_ids(&self.target));
        occupied.extend(restored_target_ids.iter().cloned());

        let mut target_leaves = self.target.leaves.clone();
        for leaf in target_leaves.values_mut() {
            if !self.target_restore.contains_key(leaf) && restored_target_ids.contains(leaf) {
                let original = leaf.clone();
                let mut index = 0u32;
                loop {
                    let candidate = NodeId(format!("pair-moved-{index}"));
                    index += 1;
                    if occupied.insert(candidate.clone()) {
                        moved_target_remap.insert(original, candidate.clone());
                        *leaf = candidate;
                        break;
                    }
                }
            }
        }
        for leaf in target_leaves.values_mut() {
            remap_node_id(leaf, &moved_target_remap);
            remap_node_id(leaf, &self.target_restore);
        }

        let mut target_minimized = self.target.minimized_slots.clone();
        for slot in target_minimized.values_mut() {
            remap_node_id(&mut slot.leaf, &moved_target_remap);
            remap_node_id(&mut slot.leaf, &self.target_restore);
            remap_tree_node_ids(&mut slot.tree, &moved_target_remap);
            remap_tree_node_ids(&mut slot.tree, &self.target_restore);
        }

        let mut target_tree = self.target.tree.clone();
        remap_optional_tree(&mut target_tree, &moved_target_remap);
        remap_optional_tree(&mut target_tree, &self.target_restore);

        let mut target_focus_stack = self.target.focus_stack.clone();
        remap_node_ids(&mut target_focus_stack, &moved_target_remap);
        remap_node_ids(&mut target_focus_stack, &self.target_restore);

        let mut target_focus_history = self.target.focus_history.clone();
        remap_node_ids(&mut target_focus_history, &moved_target_remap);
        remap_node_ids(&mut target_focus_history, &self.target_restore);

        let mut target_last_active = self.target.last_active.clone();
        if let Some(leaf) = target_last_active.as_mut() {
            remap_node_id(leaf, &moved_target_remap);
            remap_node_id(leaf, &self.target_restore);
        }

        let target_focused_leaf = target_focus.map(|mut leaf| {
            remap_node_id(&mut leaf, &moved_target_remap);
            remap_node_id(&mut leaf, &self.target_restore);
            leaf
        });

        // Repartition retained float geometry and provenance by current
        // membership: source priority, target next, all orphans to source.
        let mut source = DomainLayout::new(
            self.source_domain.bounds,
            self.source_domain.gap,
            source_outer_gap,
        );
        source.tree = self.source.tree.clone();
        source.leaves = self.source.leaves.clone();
        source.focused_leaf = source_focus;
        source.focus_stack = self.source.focus_stack.clone();
        source.focus_history = self.source.focus_history.clone();
        source.last_active = self.source.last_active.clone();
        source.exceptions = self.source.exceptions.clone();
        source.minimized_slots = self.source.minimized_slots.clone();
        source.accepted_revision = self.pair_revision;

        let mut target = DomainLayout::new(
            self.target_domain.bounds,
            self.target_domain.gap,
            target_outer_gap,
        );
        target.tree = target_tree;
        target.leaves = target_leaves;
        target.focused_leaf = target_focused_leaf;
        target.focus_stack = target_focus_stack;
        target.focus_history = target_focus_history;
        target.last_active = target_last_active;
        target.exceptions = self.target.exceptions.clone();
        target.minimized_slots = target_minimized;
        target.accepted_revision = self.pair_revision;

        repartition_retained_geometry(
            &self.source.retained_float_geometry,
            &self.target.retained_float_geometry,
            &mut source,
            &mut target,
        );
        repartition_window_set(
            &self.source.automatic_fixed,
            &self.target.automatic_fixed,
            &source.leaves,
            &source.exceptions,
            &target.leaves,
            &target.exceptions,
            &mut source.automatic_fixed,
            &mut target.automatic_fixed,
        );
        repartition_window_set(
            &self.source.fixed_tile_override,
            &self.target.fixed_tile_override,
            &source.leaves,
            &source.exceptions,
            &target.leaves,
            &target.exceptions,
            &mut source.fixed_tile_override,
            &mut target.fixed_tile_override,
        );
        repartition_window_set(
            &self.source.capacity_floats,
            &self.target.capacity_floats,
            &source.leaves,
            &source.exceptions,
            &target.leaves,
            &target.exceptions,
            &mut source.capacity_floats,
            &mut target.capacity_floats,
        );
        repartition_admission_order(
            &self.source.admission_order,
            &self.target.admission_order,
            &mut source,
            &mut target,
        );

        if !validate_domain_topology(&source_key, &source)
            || !validate_domain_topology(&target_key, &target)
        {
            return Err(PairAssemblyFailure::DomainMismatch);
        }

        WorldLayout::prune_domain_focus_history(&mut source);
        WorldLayout::prune_domain_focus_history(&mut target);

        if domain_is_empty_for_retire(&target) {
            Ok((source, None))
        } else {
            Ok((source, Some(target)))
        }
    }

    fn split_pair_focus(
        &self,
        source_key: &DomainKey,
        target_key: &DomainKey,
    ) -> (Option<NodeId>, Option<NodeId>) {
        let source_focus = if self.focused_domain.as_ref() == Some(source_key) {
            self.focused_leaf.clone()
        } else {
            self.source
                .focus_stack
                .last()
                .cloned()
                .filter(|leaf| domain_focus_resolves(&self.source, leaf))
        };
        let target_focus = if self.focused_domain.as_ref() == Some(target_key) {
            self.focused_leaf.clone()
        } else {
            self.target
                .focus_stack
                .last()
                .cloned()
                .filter(|leaf| domain_focus_resolves(&self.target, leaf))
        };
        (source_focus, target_focus)
    }
}

impl WorldLayout {
    /// Assemble a directional pair requiring reciprocal cross-output adjacency.
    pub(super) fn assemble_directional_pair(
        &self,
        source_domain: &OutputDomain,
        target_domain: &OutputDomain,
    ) -> Result<PairCandidate, PairAssemblyFailure> {
        self.assemble_pair(source_domain, target_domain, true)
    }

    /// Assemble an explicit-send pair skipping cross-output reciprocal adjacency
    /// while still validating descriptor and adjacency structure.
    pub(super) fn assemble_send_pair(
        &self,
        source_domain: &OutputDomain,
        target_domain: &OutputDomain,
    ) -> Result<PairCandidate, PairAssemblyFailure> {
        self.assemble_pair(source_domain, target_domain, false)
    }

    pub(super) fn assemble_pair(
        &self,
        source_domain: &OutputDomain,
        target_domain: &OutputDomain,
        require_adjacency: bool,
    ) -> Result<PairCandidate, PairAssemblyFailure> {
        let source_key = source_domain.key();
        let target_key = target_domain.key();

        let source_orig = self
            .domain(&source_key)
            .filter(|domain| {
                validate_domain_topology(&source_key, domain)
                    && !domain_is_empty(domain)
                    && domain.bounds == source_domain.bounds
                    && domain.gap == source_domain.gap
            })
            .ok_or(PairAssemblyFailure::SourceUnavailable)?;

        let target_orig = match self.domain(&target_key) {
            None => None,
            Some(domain)
                if validate_domain_topology(&target_key, domain)
                    && !domain_is_empty(domain)
                    && domain.bounds == target_domain.bounds
                    && domain.gap == target_domain.gap =>
            {
                Some(domain)
            }
            Some(_) => return Err(PairAssemblyFailure::TargetUnusable),
        };

        if !source_domain.validate()
            || !target_domain.validate()
            || source_key == target_key
            || !validate_adjacency(&[source_domain.clone(), target_domain.clone()])
        {
            return Err(PairAssemblyFailure::DomainMismatch);
        }

        if require_adjacency
            && source_domain.id != target_domain.id
            && (!source_domain
                .adjacent
                .values()
                .any(|v| v == &target_domain.id)
                || !target_domain
                    .adjacent
                    .values()
                    .any(|v| v == &source_domain.id))
        {
            return Err(PairAssemblyFailure::DomainMismatch);
        }

        if let Some(target_orig) = target_orig {
            if !domains_disjoint(source_orig, target_orig) {
                return Err(PairAssemblyFailure::DuplicateState);
            }
        }

        let pair_revision = source_orig
            .accepted_revision
            .max(target_orig.map(|t| t.accepted_revision).unwrap_or(0));

        let mut source = source_orig.clone();
        let mut target = target_orig.cloned().unwrap_or_else(|| {
            DomainLayout::new(
                target_domain.bounds,
                target_domain.gap,
                source_orig.outer_gap,
            )
        });

        let mut focused_domain = source.focused_leaf.as_ref().map(|_| source_key.clone());
        let mut focused_leaf = source.focused_leaf.clone();
        let mut target_restore = BTreeMap::new();

        if target_orig.is_some() {
            let target_remap = pair_target_node_remap(&source, &target);
            target_restore = target_remap
                .iter()
                .map(|(from, to)| (to.clone(), from.clone()))
                .collect();

            for leaf in target.leaves.values_mut() {
                remap_node_id(leaf, &target_remap);
            }
            for slot in target.minimized_slots.values_mut() {
                remap_node_id(&mut slot.leaf, &target_remap);
                remap_tree_node_ids(&mut slot.tree, &target_remap);
            }
            remap_node_ids(&mut target.focus_stack, &target_remap);
            remap_node_ids(&mut target.focus_history, &target_remap);
            if let Some(leaf) = target.last_active.as_mut() {
                remap_node_id(leaf, &target_remap);
            }
            if let Some(leaf) = target.focused_leaf.as_mut() {
                remap_node_id(leaf, &target_remap);
            }
            remap_optional_tree(&mut target.tree, &target_remap);

            if focused_domain.is_none() && target.focused_leaf.is_some() {
                focused_domain = Some(target_key.clone());
                focused_leaf = target.focused_leaf.clone();
            }

            // Source admission_order wins duplicate keys when merged.
            target
                .admission_order
                .retain(|id, _| !source.admission_order.contains_key(id));
        }

        // Drop minimized slots for any window present tiled or excepted anywhere in the pair.
        let remove_stale_slot = |id: &WindowId, _: &mut DomainRestoreSlot| {
            !source.leaves.contains_key(id)
                && !target.leaves.contains_key(id)
                && !source.exceptions.contains_key(id)
                && !target.exceptions.contains_key(id)
        };
        source.minimized_slots.retain(remove_stale_slot);
        target.minimized_slots.retain(remove_stale_slot);

        let pair = PairCandidate {
            source_domain: source_domain.clone(),
            target_domain: target_domain.clone(),
            source,
            target,
            pair_revision,
            focused_domain,
            focused_leaf,
            target_restore,
            admission_seq_next: self.admission_seq_next,
        };
        if !pair.validate_pair_topology() {
            return Err(PairAssemblyFailure::DomainMismatch);
        }
        Ok(pair)
    }

    /// Atomically validate/split both candidates and install them into retained
    /// storage. Refuses before any retained map or high-water write. Borrows
    /// `&PairCandidate` so callers can install convergence while keeping the
    /// same canonical pair working view for a follow-up command.
    pub(super) fn install_pair(
        &mut self,
        pair: &PairCandidate,
        source_outer_gap: i32,
        target_outer_gap_override: Option<i32>,
    ) -> Result<(), PairAssemblyFailure> {
        let source_key = pair.source_key();
        let target_key = pair.target_key();
        let target_outer_gap = target_outer_gap_override.unwrap_or_else(|| {
            self.domain(&target_key)
                .map(|domain| domain.outer_gap)
                .unwrap_or(source_outer_gap)
        });
        let pair_admission_seq_next = pair.admission_seq_next;

        let (source, target) = pair.split_with_outer_gaps(source_outer_gap, target_outer_gap)?;

        if domain_is_empty_for_retire(&source) {
            self.domains.remove(&source_key);
        } else {
            self.domains.insert(source_key, source);
        }

        if let Some(target) = target {
            self.domains.insert(target_key, target);
        } else {
            self.domains.remove(&target_key);
        }

        self.admission_seq_next = self.admission_seq_next.max(pair_admission_seq_next);
        Ok(())
    }
}

fn domain_is_empty_for_retire(domain: &DomainLayout) -> bool {
    domain.tree.is_none() && domain_is_empty(domain)
}

fn domains_disjoint(source: &DomainLayout, target: &DomainLayout) -> bool {
    let source_present =
        |id: &WindowId| source.leaves.contains_key(id) || source.exceptions.contains_key(id);
    if target
        .leaves
        .keys()
        .chain(target.exceptions.keys())
        .any(source_present)
    {
        return false;
    }
    if source
        .retained_float_geometry
        .keys()
        .any(|id| target.retained_float_geometry.contains_key(id))
    {
        return false;
    }
    if source
        .minimized_slots
        .keys()
        .any(|id| target.minimized_slots.contains_key(id))
    {
        return false;
    }
    true
}

fn domain_all_node_ids(domain: &DomainLayout) -> BTreeSet<NodeId> {
    let mut ids = BTreeSet::new();
    if let Some(tree) = &domain.tree {
        collect_node_ids(tree, &mut ids);
    }
    ids
}

fn pair_target_node_remap(
    source: &DomainLayout,
    target: &DomainLayout,
) -> BTreeMap<NodeId, NodeId> {
    let mut occupied = domain_all_node_ids(source);
    let mut mapping = BTreeMap::new();
    for id in domain_all_node_ids(target) {
        let mut index = 0u32;
        loop {
            let candidate = NodeId(format!("pair-target-{index}"));
            index += 1;
            if occupied.insert(candidate.clone()) {
                mapping.insert(id, candidate);
                break;
            }
        }
    }
    mapping
}

fn remap_node_id(id: &mut NodeId, mapping: &BTreeMap<NodeId, NodeId>) {
    if let Some(mapped) = mapping.get(id) {
        *id = mapped.clone();
    }
}

fn remap_node_ids(ids: &mut [NodeId], mapping: &BTreeMap<NodeId, NodeId>) {
    for id in ids {
        remap_node_id(id, mapping);
    }
}

fn remap_optional_tree(tree: &mut Option<Node>, mapping: &BTreeMap<NodeId, NodeId>) {
    if let Some(tree) = tree {
        remap_tree_node_ids(tree, mapping);
    }
}

fn remap_tree_node_ids(node: &mut Node, mapping: &BTreeMap<NodeId, NodeId>) {
    match node {
        Node::Leaf { id } => remap_node_id(id, mapping),
        Node::Group { id, children, .. } => {
            remap_node_id(id, mapping);
            for child in children {
                remap_tree_node_ids(child, mapping);
            }
        }
    }
}

pub(super) fn repartition_retained_geometry(
    source_retained: &BTreeMap<WindowId, Rect>,
    target_retained: &BTreeMap<WindowId, Rect>,
    source: &mut DomainLayout,
    target: &mut DomainLayout,
) {
    let mut combined: BTreeMap<WindowId, Rect> = source_retained.clone();
    for (id, rect) in target_retained {
        combined.entry(id.clone()).or_insert(*rect);
    }
    for (id, rect) in combined {
        if source.leaves.contains_key(&id) || source.exceptions.contains_key(&id) {
            source.retained_float_geometry.insert(id, rect);
        } else if target.leaves.contains_key(&id) || target.exceptions.contains_key(&id) {
            target.retained_float_geometry.insert(id, rect);
        } else {
            source.retained_float_geometry.insert(id, rect);
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn repartition_window_set(
    source_set: &BTreeSet<WindowId>,
    target_set: &BTreeSet<WindowId>,
    source_leaves: &BTreeMap<WindowId, NodeId>,
    source_exceptions: &BTreeMap<WindowId, DomainException>,
    target_leaves: &BTreeMap<WindowId, NodeId>,
    target_exceptions: &BTreeMap<WindowId, DomainException>,
    out_source: &mut BTreeSet<WindowId>,
    out_target: &mut BTreeSet<WindowId>,
) {
    let mut combined: BTreeSet<WindowId> = source_set.clone();
    combined.extend(target_set.iter().cloned());
    for id in combined {
        if source_leaves.contains_key(&id) || source_exceptions.contains_key(&id) {
            out_source.insert(id);
        } else if target_leaves.contains_key(&id) || target_exceptions.contains_key(&id) {
            out_target.insert(id);
        } else {
            out_source.insert(id);
        }
    }
}

pub(super) fn repartition_admission_order(
    source_order: &BTreeMap<WindowId, u64>,
    target_order: &BTreeMap<WindowId, u64>,
    source: &mut DomainLayout,
    target: &mut DomainLayout,
) {
    let mut combined: BTreeMap<WindowId, u64> = source_order.clone();
    for (id, seq) in target_order {
        combined.entry(id.clone()).or_insert(*seq);
    }
    for (id, seq) in combined {
        if source.leaves.contains_key(&id)
            || source.exceptions.contains_key(&id)
            || source.minimized_slots.contains_key(&id)
        {
            source.admission_order.insert(id, seq);
        } else if target.leaves.contains_key(&id)
            || target.exceptions.contains_key(&id)
            || target.minimized_slots.contains_key(&id)
        {
            target.admission_order.insert(id, seq);
        } else {
            source.admission_order.insert(id, seq);
        }
    }
}

fn opposite_direction(direction: Direction) -> Direction {
    match direction {
        Direction::Left => Direction::Right,
        Direction::Right => Direction::Left,
        Direction::Up => Direction::Down,
        Direction::Down => Direction::Up,
    }
}

fn validate_adjacency(domains: &[OutputDomain]) -> bool {
    let mut by_output: BTreeMap<OutputId, Vec<&OutputDomain>> = BTreeMap::new();
    for domain in domains {
        by_output.entry(domain.id.clone()).or_default().push(domain);
    }
    for domain in domains {
        for (direction, target) in &domain.adjacent {
            if target.0.is_empty() || target == &domain.id {
                return false;
            }
            let Some(candidates) = by_output.get(target) else {
                return false;
            };
            if candidates.len() != 1 {
                return false;
            }
            let target_domain = candidates[0];
            let opposite = opposite_direction(*direction);
            match target_domain.adjacent.get(&opposite) {
                Some(back) if back == &domain.id => {}
                _ => return false,
            }
        }
    }
    true
}

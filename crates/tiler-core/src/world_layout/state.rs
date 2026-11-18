use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use crate::cursor_targeting::FreshPlacement;
use crate::directional::{Node, NodeId, WindowId};
use crate::geometry::Rect;
use crate::policy::{LayoutPolicy, default_policy};
#[cfg(test)]
use crate::session::MinimizedSlot;
use crate::session::{DomainKey, ExceptionFlags, ExceptionRecord};

use super::group_alloc::GroupAllocator;
use crate::size_hints::FixedSizePredicate;

/// One generation's retained layout storage.
///
/// Every field here is private to `world_layout`. There is no mutable-map
/// API: callers supply validated keys/windows and receive read-only views or
/// typed outcomes. Outer clones share the group allocator counter, so burned
/// serials survive discarded trials and discarded clones.
#[derive(Debug, Clone)]
pub(crate) struct WorldLayout {
    /// Per-domain layout data keyed by the logical domain. The key is the
    /// sole home of its exceptions and restore slots: no stored row carries
    /// an independent output/workspace home.
    pub(super) domains: BTreeMap<DomainKey, DomainLayout>,
    /// The single layout policy. Stateless transactionally; carried once and
    /// never copied into domains.
    pub(super) policy: Arc<dyn LayoutPolicy>,
    /// Generation-shared monotonic group allocator.
    pub(super) group_alloc: GroupAllocator,
    /// Layout-wide first-successful-tiled-admission sequence value (D7 victim
    /// order input). One high-water, not one per domain.
    pub(super) admission_seq_next: u64,
    /// Layout configuration held once, never propagated into domains.
    pub(super) config: LayoutConfig,
}

/// Layout configuration, owned once by [`WorldLayout`].
#[derive(Debug, Clone, Copy)]
pub(super) struct LayoutConfig {
    pub(super) fixed_size_admission: bool,
    pub(super) fixed_size_predicate: FixedSizePredicate,
    pub(super) fresh_placement: FreshPlacement,
    pub(super) fresh_cursor: Option<(i32, i32)>,
}

/// Retained layout data for one logical domain.
///
/// No owner, generation, fingerprint, reconciler, pending plan, drag state
/// or native-settlement machine lives here. An accepted layout revision is
/// per-domain and distinct from the world publication revision.
#[derive(Debug, Clone)]
pub(super) struct DomainLayout {
    pub(super) tree: Option<Node>,
    /// Leaf association only, never a second homing authority: membership in
    /// `WorldEngine` remains the sole owner of window lifetimes.
    pub(super) leaves: BTreeMap<WindowId, NodeId>,
    pub(super) bounds: Rect,
    pub(super) gap: i32,
    pub(super) outer_gap: i32,
    pub(super) focused_leaf: Option<NodeId>,
    /// Logical focus recency (oldest-to-newest) for removal fallback.
    pub(super) focus_stack: Vec<NodeId>,
    /// Focus-only per-domain history for directional descent.
    pub(super) focus_history: Vec<NodeId>,
    pub(super) last_active: Option<NodeId>,
    pub(super) exceptions: BTreeMap<WindowId, DomainException>,
    pub(super) retained_float_geometry: BTreeMap<WindowId, Rect>,
    /// Classifier-created automatic fixed floats.
    pub(super) automatic_fixed: BTreeSet<WindowId>,
    /// Explicit user tile wins.
    pub(super) fixed_tile_override: BTreeSet<WindowId>,
    /// Capacity-float provenance.
    pub(super) capacity_floats: BTreeSet<WindowId>,
    pub(super) admission_order: BTreeMap<WindowId, u64>,
    pub(super) minimized_slots: BTreeMap<WindowId, DomainRestoreSlot>,
    pub(super) accepted_revision: u64,
}

/// Stored exception with no independent home. Output/workspace are derived
/// from the owning [`DomainKey`] when a public snapshot is produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DomainException {
    pub(super) flags: ExceptionFlags,
    pub(super) floating_geometry: Option<Rect>,
}

/// Stored minimize restore slot with no independent home.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DomainRestoreSlot {
    pub(super) leaf: NodeId,
    pub(super) tree: Node,
}

impl DomainLayout {
    /// Initialize an empty retained domain with the given geometry. No tree,
    /// bindings, focus history or provenance marks are present yet.
    #[must_use]
    pub(super) fn new(bounds: Rect, gap: i32, outer_gap: i32) -> Self {
        Self {
            tree: None,
            leaves: BTreeMap::new(),
            bounds,
            gap,
            outer_gap,
            focused_leaf: None,
            focus_stack: Vec::new(),
            focus_history: Vec::new(),
            last_active: None,
            exceptions: BTreeMap::new(),
            retained_float_geometry: BTreeMap::new(),
            automatic_fixed: BTreeSet::new(),
            fixed_tile_override: BTreeSet::new(),
            capacity_floats: BTreeSet::new(),
            admission_order: BTreeMap::new(),
            minimized_slots: BTreeMap::new(),
            accepted_revision: 0,
        }
    }
}

impl WorldLayout {
    /// Empty layout storage with default configuration. A fresh generation
    /// constructs a fresh `WorldLayout`, which also restarts its allocator.
    #[must_use]
    pub(crate) fn new() -> Self {
        Self {
            domains: BTreeMap::new(),
            policy: default_policy(),
            group_alloc: GroupAllocator::new(),
            admission_seq_next: 0,
            config: LayoutConfig {
                fixed_size_admission: false,
                fixed_size_predicate: FixedSizePredicate::default(),
                fresh_placement: FreshPlacement::default(),
                fresh_cursor: None,
            },
        }
    }

    /// Select fixed-size float admission for subsequent admissions only.
    pub(crate) fn set_fixed_size_admission(&mut self, enabled: bool) {
        self.config.fixed_size_admission = enabled;
    }

    /// Select the fixed-size admission predicate for subsequent admissions.
    pub(crate) fn set_fixed_size_predicate(&mut self, predicate: FixedSizePredicate) {
        self.config.fixed_size_predicate = predicate;
    }

    /// Select cursor-first fresh placement for subsequent admissions.
    pub(crate) fn set_fresh_placement(&mut self, placement: FreshPlacement) {
        self.config.fresh_placement = placement;
    }

    /// Retain the latest accepted report's normalized cursor.
    pub(crate) fn set_fresh_cursor(&mut self, cursor: Option<(i32, i32)>) {
        self.config.fresh_cursor = cursor;
    }

    pub(super) fn domain(&self, key: &DomainKey) -> Option<&DomainLayout> {
        self.domains.get(key)
    }

    /// Keys of all domains with retained layout data, in stable order.
    #[must_use]
    pub(crate) fn domain_keys(&self) -> Vec<DomainKey> {
        self.domains.keys().cloned().collect()
    }

    /// Logical focus leaf retained for a domain.
    #[must_use]
    pub(crate) fn focus_leaf(&self, key: &DomainKey) -> Option<&NodeId> {
        self.domain(key)
            .and_then(|domain| domain.focused_leaf.as_ref())
    }

    /// Retained tree for a domain (`None` for an empty retained domain).
    #[must_use]
    pub(crate) fn tree(&self, key: &DomainKey) -> Option<&Node> {
        self.domain(key).and_then(|domain| domain.tree.as_ref())
    }

    /// Public exception snapshot. Output and workspace come from the owning
    /// map key, never from an independently mutable stored home.
    #[must_use]
    pub(crate) fn exception_record(
        &self,
        key: &DomainKey,
        window: &WindowId,
    ) -> Option<ExceptionRecord> {
        self.domain(key)
            .and_then(|domain| domain.exceptions.get(window))
            .map(|exception| ExceptionRecord {
                window: window.clone(),
                output: key.output.clone(),
                workspace: key.workspace.clone(),
                flags: exception.flags,
                floating_geometry: exception.floating_geometry,
            })
    }

    /// Public minimize restore-slot snapshot, homed by the owning map key.
    #[cfg(test)]
    #[must_use]
    pub(crate) fn restore_slot_snapshot(
        &self,
        key: &DomainKey,
        window: &WindowId,
    ) -> Option<MinimizedSlot> {
        self.domain(key)
            .and_then(|domain| domain.minimized_slots.get(window))
            .map(|slot| MinimizedSlot {
                window: window.clone(),
                output: key.output.clone(),
                workspace: key.workspace.clone(),
                leaf: slot.leaf.clone(),
                tree: slot.tree.clone(),
            })
    }
}

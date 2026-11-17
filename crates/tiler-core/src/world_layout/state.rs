use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use crate::cursor_targeting::FreshPlacement;
use crate::directional::{Node, NodeId, WindowId};
use crate::geometry::Rect;
use crate::policy::{LayoutPolicy, default_policy};
use crate::session::{DomainKey, ExceptionFlags, ExceptionRecord, GroupAllocator, MinimizedSlot};
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
    domains: BTreeMap<DomainKey, DomainLayout>,
    /// The single layout policy. Stateless transactionally; carried once and
    /// never copied into domains.
    policy: Arc<dyn LayoutPolicy>,
    /// Generation-shared monotonic group allocator.
    group_alloc: GroupAllocator,
    /// Layout-wide first-successful-tiled-admission sequence value (D7 victim
    /// order input). One high-water, not one per domain.
    admission_seq_next: u64,
    /// Layout configuration held once, never propagated into domains.
    config: LayoutConfig,
}

/// Layout configuration, owned once by [`WorldLayout`].
#[derive(Debug, Clone, Copy)]
struct LayoutConfig {
    fixed_size_admission: bool,
    fixed_size_predicate: FixedSizePredicate,
    fresh_placement: FreshPlacement,
    fresh_cursor: Option<(i32, i32)>,
}

/// Retained layout data for one logical domain.
///
/// No owner, generation, fingerprint, reconciler, pending plan, drag state
/// or native-settlement machine lives here. An accepted layout revision is
/// per-domain and distinct from the world publication revision.
#[derive(Debug, Clone)]
struct DomainLayout {
    tree: Option<Node>,
    /// Leaf association only, never a second homing authority: membership in
    /// `WorldEngine` remains the sole owner of window lifetimes.
    leaves: BTreeMap<WindowId, NodeId>,
    bounds: Rect,
    gap: i32,
    outer_gap: i32,
    focused_leaf: Option<NodeId>,
    /// Logical focus recency (oldest-to-newest) for removal fallback.
    focus_stack: Vec<NodeId>,
    /// Focus-only per-domain history for directional descent.
    focus_history: Vec<NodeId>,
    last_active: Option<NodeId>,
    exceptions: BTreeMap<WindowId, DomainException>,
    retained_float_geometry: BTreeMap<WindowId, Rect>,
    /// Classifier-created automatic fixed floats.
    automatic_fixed: BTreeSet<WindowId>,
    /// Explicit user tile wins.
    fixed_tile_override: BTreeSet<WindowId>,
    /// Capacity-float provenance.
    capacity_floats: BTreeSet<WindowId>,
    admission_order: BTreeMap<WindowId, u64>,
    minimized_slots: BTreeMap<WindowId, DomainRestoreSlot>,
    accepted_revision: u64,
}

/// Stored exception with no independent home. Output/workspace are derived
/// from the owning [`DomainKey`] when a public snapshot is produced.
#[derive(Debug, Clone, PartialEq, Eq)]
struct DomainException {
    flags: ExceptionFlags,
    floating_geometry: Option<Rect>,
}

/// Stored minimize restore slot with no independent home.
#[derive(Debug, Clone, PartialEq, Eq)]
struct DomainRestoreSlot {
    leaf: NodeId,
    tree: Node,
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

    fn domain(&self, key: &DomainKey) -> Option<&DomainLayout> {
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

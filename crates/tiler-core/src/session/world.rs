//! Session world state: durable domain, window-membership, focus-MRU, and
//! exception types plus world-oriented [`super::Session`] accessors.
//!
//! Responsibility: own the authoritative world (logical domains, tiled
//! membership links, domain-scoped focus with MRU stacks/last-active,
//! deferred exceptions and retained float geometry) and read-only world
//! queries/validation. Operation families (lifecycle, move, focus, resize,
//! drag, float, workspace) and near-layout fit stay outside this module and
//! call back through these accessors.

use std::collections::{BTreeMap, BTreeSet};

use crate::contract::{DivergenceKind, Observation};
use crate::directional::{
    Direction, Node, NodeId, OutputId, Snapshot, WindowId, WindowLink, WorkspaceId,
};
use crate::geometry::Rect;

use super::{ProposeError, RefusalKind};

/// Logical output domain: separate output/workspace scope with explicit
/// portable bounds and gap for the deterministic projector, plus configured
/// logical output adjacency for R4 planning. Adjacency maps a
/// portable [`Direction`] to a neighboring [`OutputId`]; the target domain is
/// the adjacent output's currently selected logical workspace (its domain
/// workspace, which may differ from the source workspace). Targets resolve as
/// `(target_output, target_workspace)` domain keys and are validated strictly
/// reciprocal/known/non-self at [`super::Session::new`]. No platform enums or native
/// data appear here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputDomain {
    pub id: OutputId,
    pub workspace: WorkspaceId,
    pub bounds: Rect,
    pub gap: i32,
    pub adjacent: BTreeMap<Direction, OutputId>,
}

impl OutputDomain {
    /// Validity without echoing input (shape only; reciprocal/known/same
    /// workspace membership is validated across all domains at construction).
    #[must_use]
    pub fn validate(&self) -> bool {
        !self.id.0.is_empty()
            && !self.workspace.0.is_empty()
            && self.bounds.w > 0
            && self.bounds.h > 0
            && self.bounds.x.checked_add(self.bounds.w).is_some()
            && self.bounds.y.checked_add(self.bounds.h).is_some()
            && self.gap >= 0
            && self.adjacent.len() <= 4
            && self
                .adjacent
                .iter()
                .all(|(_, target)| !target.0.is_empty() && target != &self.id)
    }

    /// Domain key for this logical domain.
    #[must_use]
    pub fn key(&self) -> DomainKey {
        DomainKey {
            output: self.id.clone(),
            workspace: self.workspace.clone(),
        }
    }
}

/// Logical domain key: distinct `(OutputId, WorkspaceId)` pair with exact
/// opaque ids preserved. Logical outputs and workspaces remain separate
/// domains, so one output id may appear with several workspace ids.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DomainKey {
    pub output: OutputId,
    pub workspace: WorkspaceId,
}

/// Portable view of one logical domain: exact opaque ids plus its ordered
/// N-ary tree (`None` for an empty retained domain).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionDomainView {
    pub output: OutputId,
    pub workspace: WorkspaceId,
    pub tree: Option<Node>,
}

/// Portable session snapshot: ordered domain views in session construction
/// order plus window links sorted by window id. Unlike
/// [`crate::directional::Snapshot`], an `OutputId` may repeat across distinct
/// workspace domains.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionSnapshot {
    pub domains: Vec<SessionDomainView>,
    pub windows: Vec<WindowLink>,
}

/// Explicit observed exception flags for one window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExceptionFlags {
    pub floating: bool,
    pub fullscreen: bool,
    pub maximized: bool,
    pub sticky: bool,
}

impl ExceptionFlags {
    /// No exception flags set.
    #[must_use]
    pub const fn none() -> Self {
        Self {
            floating: false,
            fullscreen: false,
            maximized: false,
            sticky: false,
        }
    }

    /// Whether any exception flag is set.
    #[must_use]
    pub const fn any(self) -> bool {
        self.floating || self.fullscreen || self.maximized || self.sticky
    }
}

/// Explicit caller-selected behavior for an observed exception window. Fail
/// closed: admission with any set flag and `None` behavior is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExceptionBehavior {
    Defer,
}

/// One adapter-observed window with explicit exception flags plus the
/// ephemeral client size hints (AR12). Hints are advisory per-observation
/// inputs: they shape projection and clamp acceptance but never participate
/// in identity, membership, or pre/post-image matching. The two fixed-size
/// signals are explicit adapter assertions, never inferred: `fixed_auto`
/// carries classifier-created automatic origin, `fixed_suppress` carries an
/// explicit user tile win. Both default false so existing carriers behave
/// exactly as before.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedWindow {
    pub window: WindowId,
    pub output: OutputId,
    pub workspace: WorkspaceId,
    pub floating: bool,
    pub fullscreen: bool,
    pub maximized: bool,
    pub sticky: bool,
    pub fixed_auto: bool,
    pub fixed_suppress: bool,
    pub hints: crate::size_hints::WindowSizeHints,
}

impl ObservedWindow {
    /// Exception flags for this observation.
    #[must_use]
    pub const fn flags(&self) -> ExceptionFlags {
        ExceptionFlags {
            floating: self.floating,
            fullscreen: self.fullscreen,
            maximized: self.maximized,
            sticky: self.sticky,
        }
    }
}

/// Session observation: identity-bound revision plus the complete normalized
/// observed window list. Completeness is enforced at proposal: the observed
/// set must equal the known tiled-plus-exception set (plus the admitted window
/// for admissions).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionObservation {
    pub observation: Observation,
    pub windows: Vec<ObservedWindow>,
}

/// Stored exception record for a deferred window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExceptionRecord {
    pub window: WindowId,
    pub output: OutputId,
    pub workspace: WorkspaceId,
    pub flags: ExceptionFlags,
    /// Session-local floating placement. Never participates in tree geometry
    /// or fresh admission.
    pub floating_geometry: Option<Rect>,
}

/// D23 stored old slot for one minimized tiled window (REQ-MNZ-01/02/03).
/// Session-runtime only, never persisted: the homing domain, the removed
/// leaf id, and the full pre-minimize domain tree. Restore takes an exact
/// fast path (removing the leaf from the stored tree yields exactly the
/// current tree) when nothing intervened, else a best-effort rejoin in the
/// surviving old group at the old index, or regrouped with a live former
/// neighbor on the stored axis and side, with an entrant share preserving
/// the stored old fraction; only no live former neighbor, a gone domain, a
/// window sent elsewhere, or an explicit close (via
/// [`super::Session::forget_minimized_slot`]) falls back to fresh ordinary
/// admission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MinimizedSlot {
    pub window: WindowId,
    pub output: OutputId,
    pub workspace: WorkspaceId,
    pub leaf: NodeId,
    pub tree: Node,
}

/// Carried sticky-float state for swap relocation: the exception record
/// plus every per-window mark the session otherwise keys by lifetime.
/// `Session::insert_exception_floats` restores all of them, so a later
/// retile or fixed-size predicate sees exactly the pre-swap marks
/// (retained geometry, automatic-fixed classification, explicit tile
/// wins, capacity origin, and admission order).
#[derive(Debug, Clone)]
pub(crate) struct StickyFloatCarry {
    pub(crate) record: ExceptionRecord,
    pub(crate) geometry: Option<Rect>,
    pub(crate) automatic_fixed: bool,
    pub(crate) fixed_tile_override: bool,
    pub(crate) capacity_float: bool,
    pub(crate) admission_order: Option<u64>,
}

impl super::Session {
    /// Logical domains in deterministic construction order.
    #[must_use]
    pub fn domains(&self) -> &[OutputDomain] {
        &self.domains
    }
    /// Update one retained domain's projected work area without changing its
    /// topology, shares, membership, focus, or accepted revision.
    pub fn reproject_domain(&mut self, key: &DomainKey, bounds: Rect) {
        if let Some(domain) = self.domains.iter_mut().find(|domain| &domain.key() == key) {
            domain.bounds = bounds;
        }
    }

    /// Deliberate gap-update reprojection for one retained domain: adopt new
    /// projected work-area bounds plus a new inner gap while preserving
    /// topology, shares, membership, focus, exceptions, and accepted revision.
    /// Atomic: any validation failure leaves state exactly as before (`false`,
    /// no mutation). Refuses an out-of-range gap, invalid bounds,
    /// divergence, pending/drag residue, or an unknown domain. Outer-gap
    /// state lives in the Planner map, not here.
    pub fn update_domain_gaps(&mut self, key: &DomainKey, bounds: Rect, gap: i32) -> bool {
        if !(0..=64).contains(&gap) {
            return false;
        }
        if bounds.w <= 0 || bounds.h <= 0 {
            return false;
        }
        if self.reconciler.divergence().is_some() {
            return false;
        }
        if self.has_pending() || self.has_pending_desired() {
            return false;
        }
        if self.drag.is_some() {
            return false;
        }
        let index = match self.domains.iter().position(|d| &d.key() == key) {
            Some(index) => index,
            None => return false,
        };
        let backup = self.clone();
        self.domains[index].bounds = bounds;
        self.domains[index].gap = gap;
        if self.validate_current_topology() {
            return true;
        }
        *self = backup;
        false
    }

    /// Portable output relocation for one retained domain: move the domain
    /// key from `source` to `target` (different output, same or different
    /// workspace) preserving topology, shares, focus, exceptions, float
    /// geometry, and accepted revision. Updates domain identity/bounds/gap
    /// plus every window link and exception homed on the source. Atomic:
    /// any validation failure leaves revision, gap, tree/shares, windows,
    /// exceptions, and focus exactly as before (`false`, no mutation).
    /// Refuses on same output, unknown source, existing target,
    /// pending/drag/divergence residue, or invalid bounds/gap. Exception
    /// classes (`flags`, `floating_geometry`) are never altered, only the
    /// homing output/workspace.
    pub fn relocate_domain(
        &mut self,
        source: &DomainKey,
        target: &DomainKey,
        bounds: Rect,
        gap: i32,
    ) -> bool {
        if source == target {
            return false;
        }
        if source.output == target.output {
            return false;
        }
        if bounds.w <= 0 || bounds.h <= 0 {
            return false;
        }
        if !(0..=64).contains(&gap) {
            return false;
        }
        if self.reconciler.divergence().is_some() {
            return false;
        }
        if self.has_pending() || self.has_pending_desired() {
            return false;
        }
        if self.drag.is_some() {
            return false;
        }
        if self.trees.contains_key(target) {
            return false;
        }
        if self.domains.iter().find(|d| &d.key() == target).is_some() {
            return false;
        }
        let source_index = match self.domains.iter().position(|d| &d.key() == source) {
            Some(index) => index,
            None => return false,
        };
        if !self.validate_current_topology() {
            return false;
        }
        if !self.trees.contains_key(source) {
            return false;
        }
        // Atomic mutation with rollback: snapshot every mutated field so the
        // trailing topology validation (or any unexpected failure) restores
        // the exact prior revision, gap, tree/shares, windows, exceptions,
        // and focus instead of leaving partial state.
        let backup = self.clone();
        let tree = match self.trees.remove(source) {
            Some(tree) => tree,
            None => return false,
        };
        // Update domain identity/bounds/gap in place, preserving order.
        self.domains[source_index].id = target.output.clone();
        self.domains[source_index].workspace = target.workspace.clone();
        self.domains[source_index].bounds = bounds;
        self.domains[source_index].gap = gap;
        self.trees.insert(target.clone(), tree);
        for link in self.windows.values_mut() {
            if link.output == source.output && link.workspace == source.workspace {
                link.output = target.output.clone();
                link.workspace = target.workspace.clone();
            }
        }
        for record in self.exceptions.values_mut() {
            if record.output == source.output && record.workspace == source.workspace {
                record.output = target.output.clone();
                record.workspace = target.workspace.clone();
            }
        }
        for slot in self.minimized_slots.values_mut() {
            if slot.output == source.output && slot.workspace == source.workspace {
                slot.output = target.output.clone();
                slot.workspace = target.workspace.clone();
            }
        }
        if self.focused_domain.as_ref() == Some(source) {
            self.focused_domain = Some(target.clone());
        }
        if let Some(stack) = self.focus_stack.remove(source) {
            self.focus_stack.insert(target.clone(), stack);
        }
        if let Some(active) = self.last_active.remove(source) {
            self.last_active.insert(target.clone(), active);
        }
        // Pending desired mirrors committed state when present; relocation
        // already refused pending, so no pendingDesired update is needed.
        if self.validate_current_topology() {
            return true;
        }
        *self = backup;
        false
    }

    /// Extract the listed exception windows, returning each record with its
    /// full per-window state. Tiled links are untouched; callers identify
    /// sticky floats via staged facts (session records never carry the
    /// sticky flag). Missing ids are skipped.
    pub(crate) fn extract_exception_floats(
        &mut self,
        ids: &BTreeSet<WindowId>,
    ) -> Vec<StickyFloatCarry> {
        let mut out = Vec::new();
        for id in ids {
            let Some(record) = self.exceptions.remove(id) else {
                continue;
            };
            out.push(StickyFloatCarry {
                record,
                geometry: self.retained_float_geometry.remove(id),
                automatic_fixed: self.automatic_fixed.remove(id),
                fixed_tile_override: self.fixed_tile_override.remove(id),
                capacity_float: self.capacity_floats.remove(id),
                admission_order: self.admission_order.remove(id),
            });
        }
        out
    }

    /// Insert previously extracted sticky floats, rehomed onto `target`.
    /// Restores flags, floating geometry, and every per-window mark;
    /// skips ids already present. Transient/diagnostic session state
    /// (pending, drag, preview) is untouched.
    pub(crate) fn insert_exception_floats(
        &mut self,
        carried: Vec<StickyFloatCarry>,
        target: &DomainKey,
    ) {
        for mut carry in carried {
            if self.exceptions.contains_key(&carry.record.window)
                || self.windows.contains_key(&carry.record.window)
            {
                continue;
            }
            carry.record.output = target.output.clone();
            carry.record.workspace = target.workspace.clone();
            if let Some(geometry) = carry.geometry {
                self.retained_float_geometry
                    .insert(carry.record.window.clone(), geometry);
            }
            if carry.automatic_fixed {
                self.automatic_fixed.insert(carry.record.window.clone());
            }
            if carry.fixed_tile_override {
                self.fixed_tile_override.insert(carry.record.window.clone());
            }
            if carry.capacity_float {
                self.capacity_floats.insert(carry.record.window.clone());
            }
            if let Some(order) = carry.admission_order {
                self.admission_order
                    .insert(carry.record.window.clone(), order);
            }
            self.exceptions
                .insert(carry.record.window.clone(), carry.record);
        }
        debug_assert!(self.validate_current_topology());
    }

    /// Derive a sticky-only stay session rehomed same-output onto `target`
    /// from actual source state (never a fresh session, never tree
    /// surgery: callers fence tiled/minimized sticky before calling, so
    /// the source holds no tiled links for these lifetimes). Drops every
    /// non-sticky lifetime — tiled links, minimized slots, and unrelated
    /// exceptions ride the cross-output moved part instead — prunes the
    /// per-window maps to the sticky set, and rekeys the single domain
    /// (identity/bounds/gap; adjacency preserved like `relocate_domain`)
    /// with the exceptions. Focus clears (no tiled leaf survives).
    /// Returns `None` when the set is empty, any id is not an exception
    /// here, homing disagrees, the session is not single-domain, or
    /// validation fails.
    pub(crate) fn split_sticky_stay(
        &self,
        sticky_ids: &BTreeSet<WindowId>,
        target: &DomainKey,
        bounds: Rect,
        gap: i32,
    ) -> Option<Self> {
        if sticky_ids.is_empty() {
            return None;
        }
        if !sticky_ids.iter().all(|id| self.exceptions.contains_key(id)) {
            return None;
        }
        if self.domains.len() != 1 {
            return None;
        }
        let source_key = self.domains[0].key();
        if source_key.output != target.output {
            return None;
        }
        if bounds.w <= 0 || bounds.h <= 0 || !(0..=64).contains(&gap) {
            return None;
        }
        if self.reconciler.divergence().is_some()
            || self.has_pending()
            || self.has_pending_desired()
            || self.drag.is_some()
        {
            return None;
        }
        if !self.validate_current_topology() {
            return None;
        }
        let mut stay = self.clone();
        stay.windows.clear();
        stay.trees.clear();
        stay.minimized_slots.clear();
        stay.focused_domain = None;
        stay.focused_leaf = None;
        stay.focus_stack.clear();
        stay.last_active.clear();
        stay.exceptions.retain(|id, _| sticky_ids.contains(id));
        stay.retained_float_geometry
            .retain(|id, _| sticky_ids.contains(id));
        stay.automatic_fixed.retain(|id| sticky_ids.contains(id));
        stay.fixed_tile_override
            .retain(|id| sticky_ids.contains(id));
        stay.capacity_floats.retain(|id| sticky_ids.contains(id));
        stay.admission_order.retain(|id, _| sticky_ids.contains(id));
        stay.overlap_accepted.remove(&source_key);
        stay.domains[0].id = target.output.clone();
        stay.domains[0].workspace = target.workspace.clone();
        stay.domains[0].bounds = bounds;
        stay.domains[0].gap = gap;
        stay.trees.insert(target.clone(), None);
        for record in stay.exceptions.values_mut() {
            record.output = target.output.clone();
            record.workspace = target.workspace.clone();
        }
        if stay.validate_current_topology() {
            Some(stay)
        } else {
            None
        }
    }

    /// Current authoritative portable snapshot (domain order; windows sorted
    /// by window id). May repeat an `OutputId` across workspace domains.
    #[must_use]
    pub fn snapshot(&self) -> SessionSnapshot {
        self.snapshot_for(&self.trees, &self.windows)
    }

    /// Single-domain directional snapshot for movement reuse. Returns `None`
    /// for unknown domains. Scoped to exactly one logical domain so the
    /// frozen directional unique-output invariant is never abused.
    #[must_use]
    pub fn domain_snapshot(&self, output: &OutputId, workspace: &WorkspaceId) -> Option<Snapshot> {
        use crate::directional::Output;
        let domain = self.domain_for(output, workspace)?;
        let key = domain.key();
        let tree = self.trees.get(&key).cloned().flatten();
        let windows: Vec<WindowLink> = self
            .windows
            .values()
            .filter(|l| &l.output == output && &l.workspace == workspace)
            .cloned()
            .collect();
        Some(Snapshot {
            outputs: vec![Output {
                id: output.clone(),
                workspace: workspace.clone(),
                tree,
                adjacent: BTreeMap::new(),
            }],
            windows,
        })
    }

    /// Current domain-scoped focus: the focused `(output, workspace)` domain
    /// plus the focused leaf.
    #[must_use]
    pub fn focus(&self) -> (Option<DomainKey>, Option<NodeId>) {
        (self.focused_domain.clone(), self.focused_leaf.clone())
    }

    /// Committed tree for one domain, if any. Trace boundary only.
    #[must_use]
    pub fn tree_for(&self, key: &DomainKey) -> Option<&Node> {
        self.trees.get(key).and_then(|tree| tree.as_ref())
    }

    /// Synchronize retained focus from an ordinary activation of a known
    /// tiled window in an existing domain.
    ///
    /// Only updates the focused domain/leaf; topology, membership, revision,
    /// and pending state are untouched. Fails closed (`false`, no mutation)
    /// on divergence, pending/drag residue, unknown domains, malformed
    /// topology, or unknown/exception/cross-domain windows. Callers must
    /// still run the full directional proposal so ownership, observation
    /// completeness, and capability validation apply unchanged.
    pub fn sync_focus_from_window(&mut self, domain: &DomainKey, window: &WindowId) -> bool {
        if self.reconciler.divergence().is_some() {
            return false;
        }
        if self.has_pending() || self.drag.is_some() {
            return false;
        }
        if self.domains.iter().find(|d| &d.key() == domain).is_none() {
            return false;
        }
        if !self.validate_current_topology() {
            return false;
        }
        let Some(link) = self.windows.get(window) else {
            return false;
        };
        if link.output != domain.output || link.workspace != domain.workspace {
            return false;
        }
        let Some(tree) = self.trees.get(domain).cloned().flatten() else {
            return false;
        };
        if !super::collect_leaves(&tree).contains(&link.leaf) {
            return false;
        }
        self.focused_domain = Some(domain.clone());
        self.focused_leaf = Some(link.leaf.clone());
        self.focus_stack = self.updated_focus_stack(
            &self.focused_domain,
            &self.focused_leaf,
            &self.trees,
            &self.windows,
        );
        true
    }

    /// Remember `window` as the last-active focus of `domain` without
    /// touching topology, membership, revision, pending, or divergence
    /// state. Same binding gates as [`Session::sync_focus_from_window`]
    /// (unknown domains, malformed topology, unknown/exception/cross-domain
    /// windows fail closed with no mutation), plus the `last_active` advance
    /// the sync deliberately leaves to convergence: cross-output focus
    /// planning reads the remembered leaf, so an adapter-observed native
    /// focus move must land here before the focus request or the plan keeps
    /// aiming at the stale remembered target. Returns `false` (no mutation)
    /// on divergence, pending/drag residue, or any binding failure.
    pub fn remember_focus(&mut self, domain: &DomainKey, window: &WindowId) -> bool {
        // A staged plan owns its focus: never rebind underneath it.
        if self.has_pending_desired() {
            return false;
        }
        if !self.sync_focus_from_window(domain, window) {
            return false;
        }
        self.last_active = self.updated_last_active(
            &self.focused_domain,
            &self.focused_leaf,
            &self.trees,
            &self.windows,
        );
        true
    }

    /// Whether an exception member is eligible for the slotless deferred
    /// send transfer (D28 born-fullscreen carry, option (a); D16 ordinary
    /// intentional-float carry, option (a), no setting; D16 approved
    /// floating+fullscreen / floating+maximized combo carry, 2026-10-13;
    /// Windows automatic fixed-size carry with the `fixed_send_admission`
    /// opt-in).
    ///
    /// The retained record is always the floating-only shape; three
    /// observed shapes carry: the D28 held mover (never floating, still
    /// fullscreen), the ordinary float (still floating, no overlays), and
    /// the approved combos (still floating with exactly one overlay).
    /// Sticky, both overlays at once, explicit tile wins
    /// (`fixed_suppress`), and exited movers refuse. Automatic-origin
    /// signals (`fixed_auto` observation flag, retained `automatic_fixed`
    /// marker) refuse unless the `fixed_send_admission` opt-in is set, in
    /// which case automatic floats carry exactly like intentional floats.
    /// Hints never gate; size travels in the retained
    /// record. Overlays ride adapter-native with no geometry write.
    pub fn deferred_send_eligible(
        &self,
        window: &WindowId,
        source: &DomainKey,
        observed: &[ObservedWindow],
    ) -> bool {
        let Some(record) = self.exceptions.get(window) else {
            return false;
        };
        if record.output != source.output || record.workspace != source.workspace {
            return false;
        }
        if !(record.flags.floating
            && !record.flags.fullscreen
            && !record.flags.maximized
            && !record.flags.sticky)
        {
            return false;
        }
        let Some(entry) = observed.iter().find(|w| &w.window == window) else {
            return false;
        };
        if entry.output != source.output || entry.workspace != source.workspace {
            return false;
        }
        // Automatic origin lifts only under the Windows send opt-in; the
        // explicit tile win (`fixed_suppress`) refuses either way. With
        // the opt-in off every arm keeps its exact prior refusal.
        // Held born-fullscreen arm (D28): the native send-route shape.
        if !entry.floating && entry.fullscreen && !entry.maximized && !entry.sticky {
            if self.fixed_send_admission {
                return !entry.fixed_suppress;
            }
            return !entry.fixed_auto && !entry.fixed_suppress;
        }
        // Ordinary intentional-float arm (D16): still floating, no
        // overlays. Size travels in the retained record, never in the
        // observation, so no geometry gates here.
        if entry.floating && !entry.fullscreen && !entry.maximized && !entry.sticky {
            if self.fixed_send_admission {
                return !entry.fixed_suppress;
            }
            return !entry.fixed_auto
                && !entry.fixed_suppress
                && !self.automatic_fixed.contains(window);
        }
        // Approved combo arm (D16, 2026-10-13): still floating with
        // exactly one overlay, never sticky.
        if entry.floating && !entry.sticky && (entry.fullscreen ^ entry.maximized) {
            if self.fixed_send_admission {
                return !entry.fixed_suppress;
            }
            return !entry.fixed_auto
                && !entry.fixed_suppress
                && !self.automatic_fixed.contains(window);
        }
        false
    }

    /// Number of tracked exception windows.
    #[must_use]
    pub fn exception_count(&self) -> usize {
        self.exceptions.len()
    }

    /// Whether a window is tracked as an exception.
    #[must_use]
    pub fn is_exception(&self, window: &WindowId) -> bool {
        self.exceptions.contains_key(window)
    }

    /// Stored D23 minimize restore slot for a window, if any.
    #[must_use]
    pub fn minimized_slot(&self, window: &WindowId) -> Option<&MinimizedSlot> {
        self.minimized_slots.get(window)
    }

    /// Homing of every stored D23 minimize restore slot, sorted by window
    /// id. Read-only projection for the world reply so minimized lifetimes
    /// are explicit rows (never silent absence, never inferred closed).
    #[must_use]
    pub fn minimized_entries(&self) -> Vec<(WindowId, OutputId, WorkspaceId)> {
        let mut out: Vec<(WindowId, OutputId, WorkspaceId)> = self
            .minimized_slots
            .values()
            .map(|slot| {
                (
                    slot.window.clone(),
                    slot.output.clone(),
                    slot.workspace.clone(),
                )
            })
            .collect();
        out.sort_by(|a, b| a.0.0.cmp(&b.0.0));
        out
    }

    /// D23 close cleanup: explicitly forget the stored minimize restore
    /// slot for a window (for example when a minimized window closes; a
    /// bare absence from observation is a minimize, never a close). Without
    /// the report a closed minimized window's slot lingers until restart.
    /// Returns true when a slot was forgotten. Revision-neutral bookkeeping
    /// like focus sync: topology, membership, focus, and revision are
    /// untouched.
    pub fn forget_minimized_slot(&mut self, window: &WindowId) -> bool {
        self.minimized_slots.remove(window).is_some()
    }

    /// Number of stored D23 minimize restore slots.
    #[must_use]
    pub fn minimized_count(&self) -> usize {
        self.minimized_slots.len()
    }

    /// Whether any stored D23 minimize slot is still homed on a retained
    /// domain. Slots homed on removed workspaces are dropped at convergence,
    /// so only live-homed slots retain the session against empty retirement.
    #[must_use]
    pub fn has_live_minimized_slot(&self) -> bool {
        self.minimized_slots.values().any(|slot| {
            self.domains
                .iter()
                .any(|d| d.id == slot.output && d.workspace == slot.workspace)
        })
    }

    /// Whether opt-in Q2 fixed-size float admission is enabled (D1-D8).
    /// Off by default; the Linux planner route enables it while Windows
    /// keeps exact current behavior.
    #[must_use]
    pub fn fixed_size_admission(&self) -> bool {
        self.fixed_admission
    }

    /// Enable or disable opt-in Q2 fixed-size float admission. Never
    /// touches topology, shares, membership, focus, or revision.
    pub fn set_fixed_size_admission(&mut self, enabled: bool) {
        self.fixed_admission = enabled;
    }

    /// Whether the opt-in G-06 maximized directional focus fence is
    /// enabled (REQ-MAX-08). Off by default; the Linux planner route
    /// enables it while Windows keeps exact current behavior.
    #[must_use]
    pub fn maximized_focus_fence(&self) -> bool {
        self.maximized_focus_fence
    }

    /// Enable or disable the opt-in G-06 maximized directional focus
    /// fence. Never touches topology, shares, membership, focus, or
    /// revision.
    pub fn set_maximized_focus_fence(&mut self, enabled: bool) {
        self.maximized_focus_fence = enabled;
    }

    /// Whether the opt-in deferred send for automatic fixed-size floats
    /// is enabled (Windows workspace sends). Off by default; KDE/macOS
    /// carriers keep exact current behavior while the Windows route
    /// enables it.
    #[must_use]
    pub fn fixed_send_admission(&self) -> bool {
        self.fixed_send_admission
    }

    /// Enable or disable deferred sends for automatic fixed-size floats.
    /// Never touches topology, shares, membership, focus, or revision.
    pub fn set_fixed_send_admission(&mut self, enabled: bool) {
        self.fixed_send_admission = enabled;
    }

    /// Current R-SPC-04 D1 fixed-size admission predicate. Both-axes
    /// default (current delivered behavior).
    #[must_use]
    pub fn fixed_size_predicate(&self) -> crate::size_hints::FixedSizePredicate {
        self.fixed_predicate
    }

    /// Select the fixed-size admission predicate for subsequent admissions
    /// only. Never touches topology, shares, membership, focus, revision,
    /// or existing automatic/override marks (D2: no reclassification).
    pub fn set_fixed_size_predicate(&mut self, predicate: crate::size_hints::FixedSizePredicate) {
        self.fixed_predicate = predicate;
    }

    /// Whether a window floats by automatic fixed-size classification
    /// (membership only, D8), as opposed to an intentional float (D6).
    #[must_use]
    pub fn is_automatic_fixed_float(&self, window: &WindowId) -> bool {
        self.automatic_fixed.contains(window)
    }

    /// Number of classifier-created automatic fixed floats (bounded
    /// diagnostic count only, never identifiers).
    #[must_use]
    pub fn automatic_fixed_count(&self) -> usize {
        self.automatic_fixed.len()
    }

    /// Whether an explicit user tile wins for this live client (D3).
    #[must_use]
    pub fn has_fixed_tile_override(&self, window: &WindowId) -> bool {
        self.fixed_tile_override.contains(window)
    }

    /// Drop fixed-size bookkeeping for windows no longer in membership.
    /// A removed/replaced native reference with a reused id classifies
    /// again on next admission (D3); overrides never cross restart
    /// because rebinding clears the world map (D7).
    pub(super) fn prune_fixed_size_state(&mut self) {
        self.automatic_fixed
            .retain(|id| self.windows.contains_key(id) || self.exceptions.contains_key(id));
        self.fixed_tile_override
            .retain(|id| self.windows.contains_key(id) || self.exceptions.contains_key(id));
    }

    /// Retained intentional-float geometry, if this session owns that state.
    #[must_use]
    pub fn floating_geometry(&self, window: &WindowId) -> Option<Rect> {
        self.exceptions
            .get(window)
            .and_then(|record| record.floating_geometry)
    }

    /// Durable retained intentional-float geometry that survives unfloat. A
    /// window keeps its last floating placement here while tiled, so the next
    /// float can select it instead of recomputing the centered fallback.
    #[must_use]
    pub fn retained_float_geometry(&self, window: &WindowId) -> Option<Rect> {
        self.retained_float_geometry.get(window).copied()
    }

    /// The intentional-float rectangle staged by the pending plan, if any.
    /// Non-empty only for a tiled-to-float transition; unfloat clears the
    /// exception so this returns `None`.
    #[must_use]
    pub fn pending_float_geometry(&self, window: &WindowId) -> Option<Rect> {
        self.pending_desired
            .as_ref()
            .and_then(|desired| desired.exceptions.get(window))
            .and_then(|record| record.floating_geometry)
    }

    /// Exception entries as observed windows (sorted by window id).
    #[must_use]
    pub fn exception_observed(&self) -> Vec<ObservedWindow> {
        self.exceptions
            .values()
            .map(|record| ObservedWindow {
                window: record.window.clone(),
                output: record.output.clone(),
                workspace: record.workspace.clone(),
                floating: record.flags.floating,
                fullscreen: record.flags.fullscreen,
                maximized: record.flags.maximized,
                sticky: record.flags.sticky,
                // Exceptions carry no fixed-size origin: the classifier
                // recreates it from live signals, never from retained echo.
                fixed_auto: false,
                fixed_suppress: false,
                // Exceptions are never projected: hints stay empty.
                hints: crate::size_hints::WindowSizeHints::none(),
            })
            .collect()
    }

    pub(super) fn snapshot_for(
        &self,
        trees: &BTreeMap<DomainKey, Option<Node>>,
        windows: &BTreeMap<WindowId, WindowLink>,
    ) -> SessionSnapshot {
        let mut domains = Vec::with_capacity(self.domains.len());
        for domain in &self.domains {
            domains.push(SessionDomainView {
                output: domain.id.clone(),
                workspace: domain.workspace.clone(),
                tree: trees.get(&domain.key()).cloned().flatten(),
            });
        }
        let windows = windows.values().cloned().collect();
        SessionSnapshot { domains, windows }
    }

    pub(super) fn domain_for(
        &self,
        output: &OutputId,
        workspace: &WorkspaceId,
    ) -> Option<&OutputDomain> {
        self.domains
            .iter()
            .find(|d| &d.id == output && &d.workspace == workspace)
    }

    pub(super) fn all_node_ids(&self) -> BTreeSet<NodeId> {
        let mut ids = BTreeSet::new();
        for tree in self.trees.values().flatten() {
            super::collect_node_ids(tree, &mut ids);
        }
        ids
    }

    pub(super) fn validate_current_topology(&self) -> bool {
        super::validate_topology(
            &self.domains,
            &self.trees,
            &self.windows,
            &self.exceptions,
            &self.focused_domain,
            &self.focused_leaf,
        )
    }

    pub(super) fn eligible_focus_in(&self, key: &DomainKey) -> Option<NodeId> {
        if self.focused_domain.as_ref() != Some(key) {
            return None;
        }
        let focused = self.focused_leaf.clone()?;
        let tree = self.trees.get(key).cloned().flatten()?;
        if super::collect_leaves(&tree).contains(&focused) {
            let link_holds = self.windows.values().any(|l| {
                l.leaf == focused && l.output == key.output && l.workspace == key.workspace
            });
            if link_holds {
                return Some(focused);
            }
        }
        None
    }

    pub(super) fn focus_resolves(
        &self,
        domain: &Option<DomainKey>,
        leaf: &Option<NodeId>,
        trees: &BTreeMap<DomainKey, Option<Node>>,
        windows: &BTreeMap<WindowId, WindowLink>,
    ) -> bool {
        let (Some(domain), Some(leaf)) = (domain, leaf) else {
            return domain.is_none() && leaf.is_none();
        };
        let Some(tree) = trees.get(domain).cloned().flatten() else {
            return false;
        };
        if !super::collect_leaves(&tree).contains(leaf) {
            return false;
        }
        windows.values().any(|l| {
            &l.leaf == leaf && l.output == domain.output && l.workspace == domain.workspace
        })
    }

    pub(super) fn focused_window_for(&self, leaf: &NodeId, key: &DomainKey) -> Option<WindowId> {
        self.windows
            .values()
            .find(|l| &l.leaf == leaf && l.output == key.output && l.workspace == key.workspace)
            .map(|l| l.window.clone())
    }

    pub(super) fn remembered_leaf(&self, target: &DomainKey) -> Option<NodeId> {
        let leaf = self.last_active.get(target)?.clone();
        let tree = self.trees.get(target).cloned().flatten()?;
        if !super::collect_leaves(&tree).contains(&leaf) {
            return None;
        }
        self.windows
            .values()
            .any(|l| l.leaf == leaf && l.output == target.output && l.workspace == target.workspace)
            .then_some(leaf)
    }

    pub(super) fn focus_stack_fallback(
        &self,
        domain: &DomainKey,
        trees: &BTreeMap<DomainKey, Option<Node>>,
        windows: &BTreeMap<WindowId, WindowLink>,
    ) -> Option<NodeId> {
        self.focus_stack_fallback_on(&self.focus_stack, domain, trees, windows)
    }

    /// Cursor-first fresh-admission anchor for a newcomer homed in `key`
    /// when the primary anchor does not apply (evolving admission focus
    /// for other placements, observed focus leaf for Hyprland). Under the
    /// Hyprland placement with a known cursor this returns the
    /// cursor-closest retained leaf (`DwindleAlgorithm::getClosestNode` at
    /// pin `19fb395d`); otherwise the target domain's last-focused leaf,
    /// then its MRU stack fallback. Hyprland's final fallback is the first
    /// layout leaf, never a root wrap; other placements root-wrap when the
    /// domain carries no resolvable anchor.
    pub(super) fn fresh_admit_anchor(
        &self,
        key: &DomainKey,
        current: Option<&Node>,
        trees: &BTreeMap<DomainKey, Option<Node>>,
        windows: &BTreeMap<WindowId, WindowLink>,
        domain: &OutputDomain,
    ) -> Option<NodeId> {
        let hypr = self.fresh_placement
            == crate::cursor_targeting::FreshPlacement::CursorOutputCursorClosest;
        // Project once for the cursor-closest and first-leaf fallbacks;
        // child-order visitation matches the pinned first-wins ties.
        let projected: Vec<(NodeId, Rect)> = current
            .and_then(|tree| crate::geometry::project(tree, domain.bounds, domain.gap).ok())
            .map(|leaves| {
                leaves
                    .into_iter()
                    .map(|entry| (entry.leaf, entry.rect))
                    .collect()
            })
            .unwrap_or_default();
        if hypr
            && let Some(cursor) = self.fresh_cursor
            && let Some(closest) = crate::cursor_targeting::cursor_closest_leaf(&projected, cursor)
        {
            return Some(closest);
        }
        if let Some(leaf) = self
            .remembered_leaf(key)
            .or_else(|| self.focus_stack_fallback(key, trees, windows))
        {
            return Some(leaf);
        }
        if hypr {
            return projected.into_iter().next().map(|(leaf, _)| leaf);
        }
        None
    }

    pub(super) fn focus_stack_fallback_on(
        &self,
        base: &BTreeMap<DomainKey, Vec<NodeId>>,
        domain: &DomainKey,
        trees: &BTreeMap<DomainKey, Option<Node>>,
        windows: &BTreeMap<WindowId, WindowLink>,
    ) -> Option<NodeId> {
        base.get(domain)?.iter().rev().find_map(|leaf| {
            self.focus_resolves(&Some(domain.clone()), &Some(leaf.clone()), trees, windows)
                .then(|| leaf.clone())
        })
    }

    pub(super) fn updated_focus_stack(
        &self,
        focus_domain: &Option<DomainKey>,
        focus_leaf: &Option<NodeId>,
        trees: &BTreeMap<DomainKey, Option<Node>>,
        windows: &BTreeMap<WindowId, WindowLink>,
    ) -> BTreeMap<DomainKey, Vec<NodeId>> {
        self.updated_focus_stack_on(&self.focus_stack, focus_domain, focus_leaf, trees, windows)
    }

    pub(super) fn updated_focus_stack_on(
        &self,
        base: &BTreeMap<DomainKey, Vec<NodeId>>,
        focus_domain: &Option<DomainKey>,
        focus_leaf: &Option<NodeId>,
        trees: &BTreeMap<DomainKey, Option<Node>>,
        windows: &BTreeMap<WindowId, WindowLink>,
    ) -> BTreeMap<DomainKey, Vec<NodeId>> {
        let mut next = base.clone();
        next.retain(|domain, leaves| {
            leaves.retain(|leaf| {
                self.focus_resolves(&Some(domain.clone()), &Some(leaf.clone()), trees, windows)
            });
            !leaves.is_empty()
        });
        if let (Some(domain), Some(leaf)) = (focus_domain, focus_leaf) {
            let leaves = next.entry(domain.clone()).or_default();
            leaves.retain(|known| known != leaf);
            leaves.push(leaf.clone());
        }
        next
    }

    pub(super) fn updated_last_active(
        &self,
        focus_domain: &Option<DomainKey>,
        focus_leaf: &Option<NodeId>,
        trees: &BTreeMap<DomainKey, Option<Node>>,
        windows: &BTreeMap<WindowId, WindowLink>,
    ) -> BTreeMap<DomainKey, NodeId> {
        let mut next = self.last_active.clone();
        if let (Some(d), Some(l)) = (focus_domain, focus_leaf)
            && let Some(tree) = trees.get(d).cloned().flatten()
            && super::collect_leaves(&tree).contains(l)
            && windows
                .values()
                .any(|w| &w.leaf == l && w.output == d.output && w.workspace == d.workspace)
        {
            next.insert(d.clone(), l.clone());
        }
        next
    }

    /// Converge retained membership and floating state to one complete
    /// current portable observation atomically.
    ///
    /// Bounded production CORE primitive for the observation-convergence
    /// design (`docs/changes/archive/observation-convergence.md`): missing known
    /// windows are removed via the existing tree-collapse helper (survivor
    /// order/shares preserved), brand-new normal windows are admitted via the
    /// existing normal-placement helper, and tiled<->floating transitions are
    /// adopted. No staged or fabricated lifecycle observations and no new
    /// authority tokens are used. Cross-domain homing is refused (the Engine
    /// converges each per-domain session on its own complete observation).
    ///
    /// Flag scope: only portable `floating` selects the floating exception.
    /// `fullscreen`/`maximized` stay tiled (native overlays are not
    /// represented in the portable observation), `sticky` relies on the
    /// adapter's existing floating mapping and never forces an exception here,
    /// and advisory `fit_excluded` (which is not even carried by
    /// [`ObservedWindow`]) never forces an exception. Float geometry is
    /// retained where already held and never fabricated: a tiled-to-floating
    /// transition keeps the existing retained rectangle (or `None`), and an
    /// unfloat preserves it for the next float.
    ///
    /// Focus resolves to the observed tiled focus when it names a converged
    /// tiled window, else the existing focus when it still resolves, else the
    /// existing MRU stack fallback, else `None`. Topology is validated before
    /// commit; any failure leaves state exactly untouched.
    ///
    /// Fences: recorded divergence, pending desired/drag residue, and
    /// owner/generation/revision binding mismatches fail closed (binding
    /// mismatches diverge like the propose path). A change advances the
    /// verified revision/fingerprint by exactly one via the reconciler without
    /// using the pending slot; an exact membership/flags match advances
    /// nothing (focus still syncs without a revision bump, as with
    /// [`Session::sync_focus_from_window`]).
    ///
    /// Returns bounded counts only, never native identifiers.
    #[allow(clippy::too_many_lines)]
    pub fn converge_observation(
        &mut self,
        observation: &SessionObservation,
        observed_focus: Option<&WindowId>,
    ) -> Result<ObservationConvergence, ProposeError> {
        if let Some(reason) = self.reconciler.divergence() {
            return Err(ProposeError::Diverged(reason));
        }
        if self.has_pending() || self.has_pending_desired() || self.has_drag() {
            return Err(ProposeError::PendingExists);
        }
        if !self.validate_current_topology() {
            return Err(ProposeError::Refused(RefusalKind::MalformedTopology));
        }
        let diverged = |session: &mut Self, reason: DivergenceKind| {
            let kind = session.reconciler.diverge_convergence(reason);
            session.pending_desired = None;
            session.drag = None;
            ProposeError::Diverged(kind)
        };
        if !observation.observation.validate() || observation.observation.owner != self.owner {
            let reason = if observation.observation.owner != self.owner {
                DivergenceKind::OwnerMismatch
            } else if !crate::contract::is_generation_id(
                observation.observation.generation.as_str(),
            ) {
                DivergenceKind::GenerationMismatch
            } else {
                DivergenceKind::StaleRevision
            };
            return Err(diverged(self, reason));
        }
        if observation.observation.generation != self.generation {
            return Err(diverged(self, DivergenceKind::GenerationMismatch));
        }
        if observation.observation.revision != self.accepted_revision() {
            return Err(diverged(self, DivergenceKind::StaleRevision));
        }
        if !super::valid_observed_shapes(&observation.windows) {
            return Err(ProposeError::Refused(RefusalKind::MalformedInput));
        }
        let mut observed: BTreeMap<&WindowId, &ObservedWindow> = BTreeMap::new();
        for entry in &observation.windows {
            if self.domain_for(&entry.output, &entry.workspace).is_none() {
                return Err(ProposeError::Refused(RefusalKind::CrossDomainMismatch));
            }
            observed.insert(&entry.window, entry);
        }
        // Single-domain convergence only: a retained window observed under
        // different homing is a cross-session move (remove-from-source plus
        // admit-into-destination on their own complete observations), refused
        // here so the Engine handles it across its per-domain sessions.
        for (id, entry) in &observed {
            if let Some(link) = self.windows.get(*id) {
                if entry.output != link.output || entry.workspace != link.workspace {
                    return Err(ProposeError::Refused(RefusalKind::CrossDomainMismatch));
                }
            } else if let Some(record) = self.exceptions.get(*id)
                && (entry.output != record.output || entry.workspace != record.workspace)
            {
                return Err(ProposeError::Refused(RefusalKind::CrossDomainMismatch));
            }
        }
        let mut new_trees = self.trees.clone();
        let mut new_windows = self.windows.clone();
        let mut new_exceptions = self.exceptions.clone();
        let mut new_retained = self.retained_float_geometry.clone();
        let mut new_automatic = self.automatic_fixed.clone();
        let mut new_override = self.fixed_tile_override.clone();
        let mut new_minimized = self.minimized_slots.clone();
        let mut removed: usize = 0;
        let mut admitted: usize = 0;
        let mut flags_adopted: usize = 0;
        // Q2 fixed-size admission helper: brand-new fixed windows float
        // (membership only, D8) unless an explicit user tile wins (D3) or
        // the window is born fullscreen (D5 bypass). Admission-only in
        // both directions (D2): hint changes on known windows never
        // reclassify here.
        let fixed_floats_at_admit = |entry: &ObservedWindow| -> bool {
            self.fixed_admission
                && !entry.floating
                && !entry.fullscreen
                && !entry.sticky
                && !entry.fixed_suppress
                && !self.fixed_tile_override.contains(&entry.window)
                && crate::size_hints::is_fixed_size_with(entry.hints, self.fixed_predicate)
        };
        // Missing known windows are removed first so survivor collapse
        // preserves order/shares before any admission. BTreeMap iteration is
        // already window-id ordered, so no extra sorting is needed.
        for id in self.windows.keys().chain(self.exceptions.keys()) {
            if observed.contains_key(id) {
                continue;
            }
            if let Some(link) = new_windows.remove(id) {
                let key = DomainKey {
                    output: link.output.clone(),
                    workspace: link.workspace.clone(),
                };
                let current = new_trees.get(&key).cloned().flatten();
                // D23 MNZ-01/03: a missing tiled window releases its
                // allocation through the existing collapse below and stores
                // its old slot (pre-minimize tree plus leaf) for restore.
                // A sole-window minimize empties the tree while the domain
                // slot is retained; focus resolves to none below.
                let stored = current.clone();
                let next = super::remove_leaf_from_tree(self.policy(), current, &link.leaf);
                new_trees.insert(key, next);
                if let Some(tree) = stored {
                    new_minimized.insert(
                        (*id).clone(),
                        MinimizedSlot {
                            window: (*id).clone(),
                            output: link.output.clone(),
                            workspace: link.workspace.clone(),
                            leaf: link.leaf.clone(),
                            tree,
                        },
                    );
                }
                removed += 1;
            } else if new_exceptions.remove(id).is_some() {
                removed += 1;
            } else {
                return Err(ProposeError::Refused(RefusalKind::MalformedTopology));
            }
        }
        // Tiled observed floating: remove the leaf and retain a floating
        // exception. Geometry is retained where already held, never
        // fabricated.
        for (id, entry) in &observed {
            if !entry.floating || !new_windows.contains_key(*id) {
                continue;
            }
            let Some(link) = new_windows.remove(*id) else {
                return Err(ProposeError::Refused(RefusalKind::MalformedTopology));
            };
            let key = DomainKey {
                output: link.output.clone(),
                workspace: link.workspace.clone(),
            };
            let current = new_trees.get(&key).cloned().flatten();
            let next = super::remove_leaf_from_tree(self.policy(), current, &link.leaf);
            new_trees.insert(key, next);
            new_exceptions.insert(
                (*id).clone(),
                floating_record(
                    id,
                    &entry.output,
                    &entry.workspace,
                    new_retained.get(*id).copied(),
                ),
            );
            flags_adopted += 1;
        }
        // Known floating observed tiled. D5 first exits and D6 enable
        // rechecks keep automatic floats without explicit wins; only an
        // observed suppress signal or a retained override records a
        // tile win (D3). Hint changes never reclassify (D2): an
        // automatic float keeps floating across hint loss until a
        // genuine explicit win. The still-fullscreen hold below is
        // opt-in only: legacy retile
        // keeps its exact behavior with the opt-in off. Diagnostic
        // capacity origin grants no leniency (D4): a capacity float
        // observed floating:false re-admits exactly like an ordinary
        // float (tile when room, capacity-float again when still
        // exhausted). A normalized floating:true echo persists with no
        // auto-retile; only an explicit ToggleFloat command guarantees a
        // tile retry.
        let mut to_tile: Vec<WindowId> = Vec::new();
        for id in self.exceptions.keys() {
            if observed.get(id).is_some_and(|entry| {
                !entry.floating && (!self.fixed_admission || !entry.fullscreen)
            }) && new_exceptions.contains_key(id)
            {
                to_tile.push(id.clone());
            }
        }
        for id in &to_tile {
            let entry = observed.get(id).expect("to_tile observed");
            let suppressed = entry.fixed_suppress;
            let overridden = self.fixed_tile_override.contains(id);
            let was_automatic = new_automatic.contains(id);
            let fixed_now =
                crate::size_hints::is_fixed_size_with(entry.hints, self.fixed_predicate);
            if self.fixed_admission && was_automatic && !suppressed && !overridden {
                flags_adopted += 1;
                continue;
            }
            if self.fixed_admission
                && !was_automatic
                && !suppressed
                && !overridden
                && !entry.sticky
                && fixed_now
            {
                new_automatic.insert(id.clone());
                flags_adopted += 1;
                continue;
            }
            new_exceptions.remove(id);
            new_automatic.remove(id);
            if self.fixed_admission && suppressed {
                new_override.insert(id.clone());
            }
            flags_adopted += 1;
        }
        // Sticky observations adopt automatics as intentional.
        for (id, entry) in &observed {
            if entry.sticky && new_automatic.contains(*id) {
                new_automatic.remove(*id);
            }
        }
        // Adapter-asserted origin on a known floating exception adopts
        // the marker unless a tile win competes (D3).
        for (id, entry) in &observed {
            if !entry.floating || !entry.fixed_auto || entry.fixed_suppress || entry.sticky {
                continue;
            }
            if !new_exceptions.contains_key(*id) || new_automatic.contains(*id) {
                continue;
            }
            if self.fixed_tile_override.contains(*id) || new_override.contains(*id) {
                continue;
            }
            if !self.fixed_admission {
                continue;
            }
            new_automatic.insert((*id).clone());
            flags_adopted += 1;
        }
        // Brand-new floating windows become exceptions directly. Known
        // floating intent stays intentional: the automatic marker is set
        // only from the adapter's explicit fixed_auto origin signal, never
        // inferred from fixed hints alone; born fullscreen bypasses the
        // marker and stays a plain synthetic exception (D5).
        for (id, entry) in &observed {
            if entry.floating
                && !self.windows.contains_key(*id)
                && !self.exceptions.contains_key(*id)
            {
                new_exceptions.insert(
                    (*id).clone(),
                    floating_record(id, &entry.output, &entry.workspace, None),
                );
                if self.fixed_admission
                    && entry.fixed_auto
                    && !entry.fullscreen
                    && !entry.sticky
                    && !entry.fixed_suppress
                    && !self.fixed_tile_override.contains(*id)
                {
                    new_automatic.insert((*id).clone());
                }
                flags_adopted += 1;
            }
        }
        // Brand-new fixed windows float with an automatic marker (D1,
        // D8). Brand-new fullscreen holds slotless without a marker so
        // the first exit classifies fresh (D5); born maximized floats
        // under its overlay with no reserved tile (D4).
        // Direct fullscreen arrivals (floating=false) hold here; the
        // synthetic adapter hold (floating+fullscreen) held above.
        for (id, entry) in &observed {
            if !self.fixed_admission || entry.sticky {
                continue;
            }
            if self.windows.contains_key(*id)
                || self.exceptions.contains_key(*id)
                || new_exceptions.contains_key(*id)
            {
                continue;
            }
            if !entry.fullscreen || entry.floating {
                continue;
            }
            new_exceptions.insert(
                (*id).clone(),
                floating_record(id, &entry.output, &entry.workspace, None),
            );
            flags_adopted += 1;
        }
        for (id, entry) in &observed {
            if self.windows.contains_key(*id)
                || self.exceptions.contains_key(*id)
                || new_exceptions.contains_key(*id)
            {
                continue;
            }
            if !fixed_floats_at_admit(entry) {
                continue;
            }
            new_exceptions.insert(
                (*id).clone(),
                floating_record(id, &entry.output, &entry.workspace, None),
            );
            new_automatic.insert((*id).clone());
            flags_adopted += 1;
        }
        // D23: slots for windows that are present again (known tiled or
        // exception) are stale fresh-admission history, never restore
        // sources; drop them before the restore pass below.
        new_minimized
            .retain(|id, _| !new_windows.contains_key(id) && !new_exceptions.contains_key(id));
        // Normal admissions: brand-new tiled windows plus unfloats, in
        // window-id order through the existing normal-placement helper.
        // Successive admissions split the previous leaf; without eligible
        // focus, insertion wraps the root.
        let base_revision = observation.observation.revision;
        let mut existing_ids: BTreeSet<NodeId> = BTreeSet::new();
        for tree in new_trees.values().flatten() {
            super::collect_node_ids(tree, &mut existing_ids);
        }
        // D23 MNZ-02: native restore returns a minimized window to its
        // stored old neighborhood. An exact fast path restores the whole
        // stored tree when removing the leaf from it yields exactly the
        // current tree (nothing intervened); otherwise a best-effort
        // rejoin in the surviving old group at the old index, or regrouped
        // with a live former neighbor on the stored axis and side, with an
        // entrant share preserving the stored old fraction. Resizes,
        // admissions, sibling closes with a survivor, swaps, other
        // minimize/restore roundtrips, and gaps/resolution changes therefore
        // do not alone invalidate. Fresh ordinary admission below applies
        // only with no live former neighbor, a gone domain, a window sent
        // elsewhere, or a forgotten (closed) slot. Restore takes precedence
        // over the fixed-size classifier: a tiled minimize always comes
        // back tiled.
        let mut restored: BTreeSet<WindowId> = BTreeSet::new();
        // Serial admission order per domain for fresh-seed MRU history
        // (see the focus seeding below).
        let mut seeded_order: BTreeMap<DomainKey, Vec<NodeId>> = BTreeMap::new();
        for (id, entry) in &observed {
            if entry.floating
                || entry.fullscreen
                || entry.maximized
                || entry.sticky
                || new_windows.contains_key(*id)
                || new_exceptions.contains_key(*id)
            {
                continue;
            }
            let Some(slot) = new_minimized.get(*id).cloned() else {
                continue;
            };
            let key = DomainKey {
                output: slot.output.clone(),
                workspace: slot.workspace.clone(),
            };
            // The slot is consumed either way: a rejoin uses it, anything
            // else must not linger for a later observation.
            new_minimized.remove(*id);
            let homed = entry.output == slot.output
                && entry.workspace == slot.workspace
                && self
                    .domains
                    .iter()
                    .any(|d| d.id == slot.output && d.workspace == slot.workspace)
                && !existing_ids.contains(&slot.leaf);
            if !homed {
                continue;
            }
            let current = new_trees.get(&key).cloned().flatten();
            let expected =
                super::remove_leaf_from_tree(self.policy(), Some(slot.tree.clone()), &slot.leaf);
            let rejoined = if current == expected {
                Some(slot.tree.clone())
            } else {
                super::best_effort_rejoin(
                    &slot,
                    &current,
                    &new_windows,
                    &mut existing_ids,
                    id,
                    base_revision,
                )
            };
            let Some(tree) = rejoined else {
                continue;
            };
            new_trees.insert(key.clone(), Some(tree));
            new_windows.insert(
                (*id).clone(),
                WindowLink {
                    window: (*id).clone(),
                    leaf: slot.leaf.clone(),
                    output: slot.output.clone(),
                    workspace: slot.workspace.clone(),
                },
            );
            existing_ids.insert(slot.leaf.clone());
            restored.insert((*id).clone());
            seeded_order
                .entry(key.clone())
                .or_default()
                .push(slot.leaf.clone());
            admitted += 1;
        }
        let (mut adomain, mut aleaf) = (self.focused_domain.clone(), self.focused_leaf.clone());
        // Carried order drives serial admission (D8: the Engine pre-orders
        // spatial focus-last); already-known windows skip, so duplicates
        // converge to first-wins instead of map-overwrite.
        // Best-effort legs (D2/D3): ordinary anchor first, fallback search,
        // then capacity float or overlap-admit. Minimum flags alone never
        // disqualify a leg (D1, B6 kept).
        let mut hints_all: BTreeMap<WindowId, crate::size_hints::WindowSizeHints> = BTreeMap::new();
        for entry in &observation.windows {
            hints_all.insert(entry.window.clone(), entry.hints);
        }
        let mut new_capacity = self.capacity_floats.clone();
        let mut new_order = self.admission_order.clone();
        let mut new_counter = self.admission_counter;
        let mut new_overlap = self.overlap_accepted.clone();
        for entry in &observation.windows {
            let id = &entry.window;
            if entry.floating || new_windows.contains_key(id) || new_exceptions.contains_key(id) {
                continue;
            }
            let key = DomainKey {
                output: entry.output.clone(),
                workspace: entry.workspace.clone(),
            };
            let Some(domain) = self.domains.iter().find(|d| d.key() == key) else {
                return Err(ProposeError::Refused(RefusalKind::CrossDomainMismatch));
            };
            // Anchor order. Non-Hypr placements lead with the evolving
            // admission focus when homed here and still resolved, else the
            // fresh anchor below. Hyprland instead leads with the observed
            // focus leaf when it names a converged tiled link here (the
            // pinned `addTarget` active-window branch at 19fb395d covers
            // the global active window on its own workspace only), else
            // the fresh anchor (cursor-closest, MRU, first leaf). An
            // off-target global focus therefore never anchors under Hypr.
            let current = new_trees.get(&key).cloned().flatten();
            let hypr = self.fresh_placement
                == crate::cursor_targeting::FreshPlacement::CursorOutputCursorClosest;
            let observed_leaf = observed_focus.and_then(|window| {
                new_windows
                    .get(window)
                    .filter(|link| link.output == key.output && link.workspace == key.workspace)
                    .map(|link| link.leaf.clone())
            });
            let evolving = match (&adomain, &aleaf) {
                (Some(d), Some(l)) if d == &key => self
                    .focus_resolves(&adomain, &aleaf, &new_trees, &new_windows)
                    .then(|| l.clone()),
                _ => None,
            };
            let eligible = if hypr {
                observed_leaf.or_else(|| {
                    self.fresh_admit_anchor(
                        &key,
                        current.as_ref(),
                        &new_trees,
                        &new_windows,
                        domain,
                    )
                })
            } else {
                evolving.or_else(|| {
                    self.fresh_admit_anchor(
                        &key,
                        current.as_ref(),
                        &new_trees,
                        &new_windows,
                        domain,
                    )
                })
            };
            let placement =
                super::admission_placement_for(domain, current.as_ref(), eligible.as_ref());
            let orientation = self.policy().admission_axis_for_rect(&placement);
            let leaf_links: BTreeMap<crate::directional::NodeId, WindowId> = new_windows
                .values()
                .filter(|l| l.output == key.output && l.workspace == key.workspace)
                .map(|l| (l.leaf.clone(), l.window.clone()))
                .collect();
            let leg = super::capacity::plan_admission_leg(
                self.policy(),
                self.anchor_fallback,
                current,
                domain.bounds,
                domain.gap,
                eligible,
                orientation,
                id,
                base_revision,
                &mut existing_ids,
                &hints_all,
                &leaf_links,
            );
            let leaf_id = leg.leaf_id;
            let ordinary = leg.ordinary_tree;
            let admitted_tree: Option<Node> = match leg.selected {
                Ok(target) => Some(target.tree),
                Err(()) => None,
            };
            if let Some(next) = admitted_tree {
                new_trees.insert(key.clone(), Some(next));
                new_windows.insert(
                    id.clone(),
                    WindowLink {
                        window: id.clone(),
                        leaf: leaf_id.clone(),
                        output: entry.output.clone(),
                        workspace: entry.workspace.clone(),
                    },
                );
                // D7 first-successful-tiled-admission order: record the
                // sequence exactly on this successful leg insert, in
                // carried admission order (never in window-id order).
                // Lifetimes already tracked keep their first number;
                // floating legs take no order.
                if !new_order.contains_key(id) {
                    new_order.insert(id.clone(), new_counter);
                    new_counter = new_counter.saturating_add(1);
                }
                adomain = Some(key.clone());
                aleaf = Some(leaf_id.clone());
                seeded_order
                    .entry(key.clone())
                    .or_default()
                    .push(leaf_id.clone());
                // An observed suppress signal mirrors into the retained
                // override set so the win survives cross-domain transfer
                // (the paired transplant carries it); removals still prune.
                if self.fixed_admission && entry.fixed_suppress {
                    new_automatic.remove(id);
                    new_override.insert(id.clone());
                }
                new_capacity.remove(id);
            } else if self.no_space == crate::admission::NoSpaceResult::AllowOverlap
                && let Some(candidate) = ordinary
                && let Some(domain) = self.domains.iter().find(|d| d.key() == key)
            {
                // Overlap-admit at the ordinary anchor (D9): tiled
                // membership with positive overflowing geometry.
                let mut overlap_windows = new_windows.clone();
                overlap_windows.insert(
                    id.clone(),
                    WindowLink {
                        window: id.clone(),
                        leaf: leaf_id.clone(),
                        output: entry.output.clone(),
                        workspace: entry.workspace.clone(),
                    },
                );
                let Some(_) = super::capacity::project_domain_overlap(
                    domain,
                    &candidate,
                    &overlap_windows,
                    &key,
                    &hints_all,
                ) else {
                    return Err(ProposeError::Refused(RefusalKind::MalformedTopology));
                };
                new_trees.insert(key.clone(), Some(candidate));
                new_windows = overlap_windows;
                new_overlap.insert(key.clone());
                // Same first-successful-tiled-admission sequence as the
                // strict leg above (overlap-admit is a tiled success).
                if !new_order.contains_key(id) {
                    new_order.insert(id.clone(), new_counter);
                    new_counter = new_counter.saturating_add(1);
                }
                adomain = Some(key.clone());
                aleaf = Some(leaf_id.clone());
                seeded_order
                    .entry(key.clone())
                    .or_default()
                    .push(leaf_id.clone());
                if self.fixed_admission && entry.fixed_suppress {
                    new_automatic.remove(id);
                    new_override.insert(id.clone());
                }
                new_capacity.remove(id);
            } else {
                // Capacity float (D4): slotless membership with
                // diagnostic-only origin at the untouched native frame.
                // No false fixed-tile override: only genuinely
                // fixed-size windows keep or earn one.
                new_exceptions.insert(
                    id.clone(),
                    floating_record(
                        id,
                        &entry.output,
                        &entry.workspace,
                        new_retained.get(id).copied(),
                    ),
                );
                new_capacity.insert(id.clone());
                let fixed_now =
                    crate::size_hints::is_fixed_size_with(entry.hints, self.fixed_predicate);
                if !new_override.contains(id) {
                    if entry.fixed_suppress && fixed_now {
                        new_automatic.remove(id);
                        new_override.insert(id.clone());
                    } else {
                        new_override.remove(id);
                    }
                }
            }
            // Unfloat re-admits were already counted as flag adoptions.
            if !to_tile.contains(id) {
                admitted += 1;
            }
        }
        // Floating legs take no admission order; same-lifetime
        // retile/move retention and close clearing happen through the
        // existing clone/prune paths below.
        new_capacity.retain(|id| new_windows.contains_key(id) || new_exceptions.contains_key(id));
        // D7 repair gate: every retained tree must project strictly, be an
        // accepted overlap allocation, or repair by floating victims.
        // Protected holds stay narrow and truthful (MalformedTopology).
        {
            let mut protected: BTreeSet<WindowId> = BTreeSet::new();
            for (id, entry) in &observed {
                if (entry.fullscreen || entry.maximized || entry.sticky)
                    && new_windows.contains_key(*id)
                {
                    protected.insert((*id).clone());
                }
            }
            let empty_frames: BTreeMap<WindowId, crate::geometry::Rect> = BTreeMap::new();
            let mut repair = super::capacity::DomainRepair {
                policy: self.policy.clone(),
                no_space: self.no_space,
                domains: self.domains.clone(),
                trees: new_trees.clone(),
                windows: new_windows.clone(),
                exceptions: new_exceptions.clone(),
                retained: new_retained.clone(),
                capacity: new_capacity.clone(),
                overlap: new_overlap.clone(),
                fixed_override: new_override.clone(),
                admission_order: new_order.clone(),
            };
            for domain in &self.domains {
                let key = domain.key();
                if repair.trees.get(&key).cloned().flatten().is_none() {
                    repair.overlap.remove(&key);
                    continue;
                }
                let focus_stack = self.focus_stack.get(&key).cloned().unwrap_or_default();
                let focused = (self.focused_domain.clone(), self.focused_leaf.clone());
                if repair
                    .repair(
                        &key,
                        &hints_all,
                        &protected,
                        &empty_frames,
                        // Convergence never changes bounds or gaps: an
                        // accepted overlap mark shelters here, and real
                        // capacity decisions happen per leg above.
                        false,
                        &focus_stack,
                        &focused,
                    )
                    .is_none()
                {
                    return Err(ProposeError::Refused(RefusalKind::MalformedTopology));
                }
            }
            new_trees = repair.trees;
            new_windows = repair.windows;
            new_exceptions = repair.exceptions;
            new_retained = repair.retained;
            new_capacity = repair.capacity;
            new_overlap = repair.overlap;
        }
        // Fresh-seed MRU history (D8): per-leg commits used to push
        // each admitted leaf, so focused-removal fallback (sends,
        // unfocused closes) keeps its MRU instead of dropping to none.
        // Newly admitted leaves append in serial order; retained history
        // is never rewritten or reordered. The merged base stays local
        // until commit: refusals below leave state exactly untouched.
        let mut stack_base = self.focus_stack.clone();
        for (key, leaves) in &seeded_order {
            let stack = stack_base.entry(key.clone()).or_default();
            for leaf in leaves {
                if !stack.contains(leaf) {
                    stack.push(leaf.clone());
                }
            }
        }
        // Focus: observed tiled focus when valid, else retained focus when it
        // still resolves, else the existing MRU stack fallback, else None.
        // D23 MNZ-02: a just-restored minimized window never takes focus
        // from the observation; the host owns any restore focus, so the
        // pre-restore MRU focus (or none) stays valid.
        let (mut next_focus_domain, mut next_focus_leaf): (Option<DomainKey>, Option<NodeId>) =
            (None, None);
        let observed_is_restored = observed_focus.is_some_and(|focused| restored.contains(focused));
        if !observed_is_restored
            && let Some(focused) = observed_focus
            && let Some(link) = new_windows.get(focused)
        {
            next_focus_domain = Some(DomainKey {
                output: link.output.clone(),
                workspace: link.workspace.clone(),
            });
            next_focus_leaf = Some(link.leaf.clone());
        } else if self.focus_resolves(
            &self.focused_domain,
            &self.focused_leaf,
            &new_trees,
            &new_windows,
        ) {
            next_focus_domain = self.focused_domain.clone();
            next_focus_leaf = self.focused_leaf.clone();
        } else if let Some(domain) = self.focused_domain.clone()
            && let Some(leaf) =
                self.focus_stack_fallback_on(&stack_base, &domain, &new_trees, &new_windows)
        {
            next_focus_domain = Some(domain);
            next_focus_leaf = Some(leaf);
        }
        if !super::validate_topology(
            &self.domains,
            &new_trees,
            &new_windows,
            &new_exceptions,
            &next_focus_domain,
            &next_focus_leaf,
        ) {
            return Err(ProposeError::Refused(RefusalKind::MalformedTopology));
        }
        // Removed windows drop fixed-size bookkeeping so a reused id
        // classifies again (D3); overrides never cross restart (D7).
        // Minimize slots homed on removed workspaces are dropped; slots
        // for present windows were already pruned above.
        new_automatic.retain(|id| new_windows.contains_key(id) || new_exceptions.contains_key(id));
        new_override.retain(|id| new_windows.contains_key(id) || new_exceptions.contains_key(id));
        new_capacity.retain(|id| new_windows.contains_key(id) || new_exceptions.contains_key(id));
        new_order.retain(|id, _| {
            new_windows.contains_key(id)
                || new_exceptions.contains_key(id)
                || new_minimized.contains_key(id)
        });
        new_overlap.retain(|key| new_trees.contains_key(key));
        new_minimized.retain(|_, slot| {
            self.domains
                .iter()
                .any(|d| d.id == slot.output && d.workspace == slot.workspace)
        });
        let changed = new_trees != self.trees
            || new_windows != self.windows
            || new_exceptions != self.exceptions
            || new_retained != self.retained_float_geometry
            || new_automatic != self.automatic_fixed
            || new_override != self.fixed_tile_override
            || new_minimized != self.minimized_slots
            || new_capacity != self.capacity_floats
            || new_order != self.admission_order
            || new_counter != self.admission_counter
            || new_overlap != self.overlap_accepted;
        let fingerprint = observation.observation.fingerprint;
        if changed {
            // Exhaustion guard before the advance: refuse without recording a
            // terminal divergence so a transient observation at the bound
            // never permanently bricks the session.
            if self.accepted_revision() >= crate::contract::MAX_REVISION {
                return Err(ProposeError::Diverged(DivergenceKind::RevisionExhausted));
            }
            match self.reconciler.commit_observation_convergence(fingerprint) {
                Ok(_) => {}
                Err(reason) => {
                    if self.reconciler.divergence().is_some() {
                        self.pending_desired = None;
                        self.drag = None;
                    }
                    return Err(ProposeError::Diverged(reason));
                }
            }
            self.trees = new_trees;
            self.windows = new_windows;
            self.exceptions = new_exceptions;
            self.retained_float_geometry = new_retained;
            self.automatic_fixed = new_automatic;
            self.fixed_tile_override = new_override;
            self.minimized_slots = new_minimized;
            self.capacity_floats = new_capacity;
            self.admission_order = new_order;
            self.admission_counter = new_counter;
            self.overlap_accepted = new_overlap;
            self.accepted_fingerprint = fingerprint;
        }
        self.focused_domain = next_focus_domain;
        self.focused_leaf = next_focus_leaf;
        self.focus_stack = self.updated_focus_stack_on(
            &stack_base,
            &self.focused_domain,
            &self.focused_leaf,
            &self.trees,
            &self.windows,
        );
        self.last_active = self.updated_last_active(
            &self.focused_domain,
            &self.focused_leaf,
            &self.trees,
            &self.windows,
        );
        debug_assert!(self.validate_current_topology());
        if changed {
            Ok(ObservationConvergence {
                removed,
                admitted,
                flags_adopted,
            })
        } else {
            Ok(ObservationConvergence {
                removed: 0,
                admitted: 0,
                flags_adopted: 0,
            })
        }
    }
}

/// One floating exception record: portable `floating` set, every other flag
/// clear (fullscreen/maximized overlays stay tiled, sticky rides the
/// adapter's floating mapping). Geometry is retained where held, never
/// fabricated.
fn floating_record(
    window: &WindowId,
    output: &OutputId,
    workspace: &WorkspaceId,
    geometry: Option<Rect>,
) -> ExceptionRecord {
    ExceptionRecord {
        window: window.clone(),
        output: output.clone(),
        workspace: workspace.clone(),
        flags: ExceptionFlags {
            floating: true,
            fullscreen: false,
            maximized: false,
            sticky: false,
        },
        floating_geometry: geometry,
    }
}

/// Bounded result of [`Session::converge_observation`]: correlated
/// removed/admitted/flag-adopted counts with no identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObservationConvergence {
    /// Missing known windows removed.
    pub removed: usize,
    /// Brand-new normal windows admitted.
    pub admitted: usize,
    /// Floating adoptions: tiled<->floating transitions and brand-new
    /// floating exceptions.
    pub flags_adopted: usize,
}

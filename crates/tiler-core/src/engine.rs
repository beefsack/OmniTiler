//! Portable world-level engine: owns per-domain sessions without merging them.
//!
//! Adapter-normalized integer geometry only; no transport, JSON, platform, or
//! process imports. Each logical `(OutputId, WorkspaceId)` domain keeps its
//! own independent [`Session`] (independent revisions, fingerprints,
//! divergence isolation, node identity, and outer-gap handling).
//! The engine keeps domains independent while owning seeding, relocation,
//! and typed request outcomes. Protocol keeps
//! envelope validation, ordered ingress fences,
//! and wire serialization.

use std::collections::{BTreeMap, BTreeSet};

use crate::boundary::{
    ActiveGroupResolution, CoreCommand, CoreEvent, CoreReply, NoGroupReason, ProjectionKind,
    ProjectionPlan, project_retained_tiled_geometry, resolve_active_group,
};
use crate::bounds::{is_gap, is_opaque_id};
use crate::contract::{
    AckOutcome, AdapterAck, DivergenceKind, DragCapabilities, DragPostObservation,
    FocusCapabilities, FocusPostObservation, LIFECYCLE_POLICY_VERSION, LifecycleCapabilities,
    LifecyclePostObservation, LifecyclePrecondition, Observation, PostObservation,
    ResizeCapabilities, ResizeMode, ResizePostObservation,
};
use crate::directional::{
    Axis, Capabilities, Direction, MoveOperation, Node, NodeId, OutputId, WindowId, WorkspaceId,
};
use crate::geometry::Rect;
use crate::ids::{CorrelationId, GenerationId, OwnerId};
use crate::policy::{LayoutPolicy, default_policy};
use crate::seed::EngineWindow;
use crate::session::{
    CanonicalPairError, DomainKey, DragHoverPrior, ExceptionFlags, OutputDomain, ProposeError,
    RefusalKind, Session, SessionCommand, SessionDragPlan, SessionObservation,
};

/// Wire `kind`/`message` for the Session internal pending fence.
///
/// Mirrors the protocol `pending-exists` rejection exactly; single source for
/// the Session-owned conflict outcome so serialization stays byte identical.
const PENDING_EXISTS_KIND: &str = "pending-exists";
const PENDING_EXISTS_MESSAGE: &str = "complete the pending plan before proposing";
/// Wire `kind`/`message` for ambiguous seed order (mirrors `MSG_AMBIGUOUS`).
const AMBIGUOUS_KIND: &str = "ambiguous-placement";
const AMBIGUOUS_MESSAGE: &str = "window placement is ambiguous";
/// Wire `message` for snapshot failures (mirrors `MSG_OBSERVATION`).
const OBSERVATION_MESSAGE: &str = "observation does not cover the known window set";
/// Wire `message` for opaque id failures (mirrors `MSG_OPAQUE_ID`).
const OPAQUE_ID_MESSAGE: &str = "opaque id is invalid";
/// Wire `kind`/`message` for direction failures (mirrors `MSG_DIRECTION`).
const DIRECTION_KIND: &str = "direction-invalid";
const DIRECTION_MESSAGE: &str = "direction is invalid";

/// Portable world engine: per-domain sessions plus binding state.
#[derive(Debug, Clone)]
pub struct Engine {
    /// Selected layout policy carried into every retained session. Stateless
    /// transactionally: cloning, backups, relocation, and canonical
    /// pair/split work carry it without touching revision, divergence,
    /// or gap state. COSMIC v1 is the only implementation.
    policy: std::sync::Arc<dyn LayoutPolicy>,
    sessions: BTreeMap<DomainKey, Session>,
    outer_gaps: BTreeMap<DomainKey, i32>,
    owner: Option<OwnerId>,
    generation: Option<GenerationId>,
    /// Last single-domain observation-convergence report for protocol logging.
    ///
    /// Set only when [`Engine::handle`] converged with nonzero counts; cleared
    /// at the start of every [`Engine::handle`] so callers never read a stale
    /// op. Bounded counts only, never native identifiers; the correlation
    /// binds the existing planner summary boundary (`PLAN_SUMMARY_PREFIX`)
    /// without any [`CoreReply`] change (core has no logging sink). Exact
    /// (zero-count) convergence records nothing so only nonzero counts log.
    last_convergence: Option<EngineConvergenceReport>,
    /// Last fresh adoption-fit report for protocol logging.
    ///
    /// Set exactly once per actual fresh adoption attempt in
    /// [`Engine::fresh_admit_shared`] (no retained slot at entry); cleared
    /// at the start of every [`Engine::handle`]. Bounded counts plus
    /// correlation/outcome/reason only, never native identifiers.
    last_adoption_fit: Option<EngineAdoptionFitReport>,
    /// Trace-only startup fit placement diagnostic for the current op.
    ///
    /// Set exactly once per actual fresh adoption attempt in
    /// [`Engine::fresh_admit_shared`], alongside `last_adoption_fit`.
    /// Bounded domain bounds plus capped input rectangles, outcome/reason,
    /// and the resulting tree summary (counts plus capped root shares).
    /// Cleared at the start of every [`Engine::handle`].
    last_startup_fit_trace: Option<EngineStartupFitTrace>,
    /// Trace-only send placement diagnostic for the current op.
    ///
    /// Set when [`Engine::transfer_request`] successfully proposes a
    /// `MoveToWorkspace` plan, copying the session's anchor/axis/projected
    /// selection. Cleared at the start of every [`Engine::handle`].
    last_send_placement: Option<EngineSendPlacementTrace>,
    /// Whether the current [`Engine::handle`] converged (changed or exact).
    ///
    /// Internal reseed guard only, never logged: once converged, partial or
    /// diverged follow-ups fail closed without reset/reseed.
    converged_this_op: bool,
    /// Opt-in Q2 fixed-size float admission (D1-D8). Off by default so
    /// Windows carriers (`Engine::new`) keep exact current behavior; the
    /// Linux planner route enables it.
    fixed_size_admission: bool,
    /// Opt-in G-06 maximized directional focus fence (REQ-MAX-08). Off by
    /// default so Windows carriers keep exact current behavior; the Linux
    /// planner route enables it.
    maximized_focus_fence: bool,
    /// Opt-in deferred send for automatic fixed-size floats (Windows
    /// workspace sends). Off by default so KDE/macOS carriers keep exact
    /// current behavior; the Windows route enables it.
    fixed_send_admission: bool,
    /// Best-effort admission policy (D2/D3, selected 2026-10-17): anchor
    /// fallback and no-space result for subsequent capacity decisions
    /// only. Largest-splittable-window and float defaults. Changing
    /// either never reflows, retries, or reclassifies retained state.
    anchor_fallback: crate::admission::AnchorFallback,
    no_space: crate::admission::NoSpaceResult,
    /// R-SPC-04 D1 fixed-size admission predicate. Both-axes default
    /// (current delivered behavior). Changing it never reclassifies
    /// retained windows, only subsequent admissions.
    fixed_size_predicate: crate::size_hints::FixedSizePredicate,
    /// Engine-wide next first-successful-tiled-admission sequence value
    /// (D7 victim order input). Independent per-domain sessions each start
    /// their counter at zero, so admissions in different chronological
    /// requests would collide or invert once their windows share a domain
    /// through a send or pair assembly. Every working session is
    /// synchronized to at least this floor before admission work, and
    /// every store records the session counter back here, keeping one
    /// monotonic Engine-wide sequence with no wallclock. Reset with the
    /// world map on owner/generation change; never persisted across
    /// restart. Saturating arithmetic throughout: the bound never wraps.
    admission_seq_next: u64,
    /// Last fixed-size admission report for protocol logging.
    ///
    /// Set when [`Engine::handle`] admitted fixed-size windows with the
    /// opt-in enabled; cleared at the start of every [`Engine::handle`].
    /// Bounded counts plus correlation/op/reason only, never native
    /// identifiers.
    last_fixed_admission: Option<EngineFixedAdmissionReport>,
    /// Last whole-workspace migration report for protocol logging.
    ///
    /// Set exactly once per successful explicit migration in
    /// [`Engine::migrate_workspace_request`]; cleared at the start of every
    /// [`Engine::handle`]. Bounded counts plus correlation/direction only,
    /// never native identifiers.
    last_migration: Option<EngineMigrationReport>,
    /// Last read-only drag-preview diagnostic stage for protocol logging.
    ///
    /// Set only when a `drag-preview` request refuses with an otherwise
    /// ambiguous kind (`cross-domain-mismatch` or `malformed-topology`);
    /// cleared at the start of every [`Engine::handle`]. Closed stage
    /// vocabulary plus correlation only, never native identifiers; the
    /// [`CoreReply`] is unchanged (core has no logging sink).
    last_drag_preview_stage: Option<EngineDragPreviewStage>,
}

/// Bounded correlated observation-convergence report for the protocol logging
/// boundary. Counts only, no window/domain identifiers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineConvergenceReport {
    /// Validated correlation for this op (cross-service lookup key).
    pub correlation: CorrelationId,
    /// Single-domain op token (`reconcile`, `admit`, `remove`, ...).
    pub op: &'static str,
    /// Missing known windows removed by convergence.
    pub removed: usize,
    /// Brand-new normal windows admitted by convergence.
    pub admitted: usize,
    /// Floating adoptions by convergence.
    pub flags_adopted: usize,
}

/// Bounded correlated fixed-size admission report for the protocol logging
/// boundary (D1-D8). Counts plus the fixed decision token only, never
/// native identifiers, geometry, or content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineFixedAdmissionReport {
    /// Validated correlation for this op (cross-service lookup key).
    pub correlation: CorrelationId,
    /// Single-domain op token (`reconcile`, `admit`, ...).
    pub op: &'static str,
    /// Carried windows evaluated for fixed-size admission.
    pub evaluated: usize,
    /// Windows admitted as automatic fixed floats by this op.
    pub admitted: usize,
    /// Fixed decision token (`fixed-equal` when admitted, else the
    /// first applicable `not-fixed-*` reason).
    pub reason: &'static str,
}

/// Bounded correlated whole-workspace migration report for the protocol
/// logging boundary (R-WS-12). Records the retained rekey only, never native
/// completion. Counts plus direction/empty tokens only, never native
/// identifiers, geometry, domains, owner, or payloads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineMigrationReport {
    /// Validated correlation for this op (cross-service lookup key).
    pub correlation: CorrelationId,
    /// Requested direction token (`left`/`right`/`up`/`down`).
    pub direction: &'static str,
    /// Carried source members relocated (tiled plus floating).
    pub members: usize,
    /// Retained floating exceptions carried with the domain.
    pub floats: usize,
    /// True only for the empty-domain path (no retained session moved).
    pub empty: bool,
}

/// Bounded correlated read-only drag-preview stage report for the protocol
/// logging boundary. Closed stage vocabulary plus correlation only, never
/// native identifiers, geometry, domains, owner, or payloads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineDragPreviewStage {
    /// Validated correlation for this op (cross-service lookup key).
    pub correlation: CorrelationId,
    /// Failing stage token (`converge`, `begin`, `topology`,
    /// `stale-capture`, `scope`, `resolve-outside`, `resolve-topology`,
    /// `projection`, each optionally `pair-`-prefixed on the occupied
    /// cross-output pair path).
    pub stage: &'static str,
}

/// Bounded correlated fresh adoption-fit report for the protocol logging
/// boundary. Counts only, no window/domain identifiers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineAdoptionFitReport {
    /// Validated correlation for this op (cross-service lookup key).
    pub correlation: CorrelationId,
    /// `fitted` when the fit committed, else `fallback`.
    pub outcome: &'static str,
    /// Complete carried window count.
    pub windows: usize,
    /// `ok` for fitted, else the decline token (`single_window`,
    /// `fit_excluded`, `invalid_geometry`, `no_cut`, `projection_invalid`,
    /// or `commit_failed` when fit geometry succeeded but commit did not).
    pub reason: &'static str,
    /// Centre splits in the committed fit (0 for clean fits and fallbacks).
    pub centre_splits: usize,
}

/// Maximum carried inputs kept in a startup trace.
pub const STARTUP_TRACE_MAX_RECTS: usize = 8;
/// Maximum characters kept in a startup tree description.
pub const STARTUP_TRACE_MAX_TOPOLOGY: usize = 256;

/// Trace-only bounded startup placement diagnostic for the protocol
/// trace boundary. Domain bounds plus capped carried inputs (opaque
/// window token plus rectangle each), fit outcome/reason, and the
/// resulting ordered tree description with axes and nested shares.
/// Integer geometry and opaque tokens only; never native identifiers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartupInput {
    pub window: WindowId,
    pub rect: Rect,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineStartupFitTrace {
    /// Validated correlation for this op (cross-service lookup key).
    pub correlation: CorrelationId,
    /// Complete carried window count.
    pub windows: usize,
    /// Adopting domain bounds.
    pub domain_bounds: Rect,
    /// Carried inputs in observation order, capped at
    /// [`STARTUP_TRACE_MAX_RECTS`].
    pub inputs: Vec<StartupInput>,
    /// `fitted` when the fit committed, else `fallback`.
    pub outcome: &'static str,
    /// `ok` for fitted, else the decline token (mirrors the adoption-fit
    /// reason vocabulary plus `commit_failed`).
    pub reason: &'static str,
    /// Centre splits in the committed fit (0 for clean fits and fallbacks).
    pub centre_splits: usize,
    /// Leaf count in the resulting tree (0 when no tree resulted).
    pub leaves: usize,
    /// Ordered tree description: `L` per leaf, `H[s,..]`/`V[s,..]`
    /// per group with nested shares in child order, capped at
    /// [`STARTUP_TRACE_MAX_TOPOLOGY`] characters (`-` when empty).
    pub topology: String,
}

/// Trace-only bounded send placement diagnostic for the protocol trace
/// boundary. Selected `map_to_tree` anchor branch plus its opaque leaf,
/// the admission axis derived from its projected rectangle, that
/// rectangle, and the target leaf count. Opaque tokens only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineSendPlacementTrace {
    /// Validated correlation for this op (cross-service lookup key).
    pub correlation: CorrelationId,
    /// Selected anchor branch (`remembered`/`mru`/`fallback`/`empty`).
    pub anchor_kind: &'static str,
    /// Selected anchor leaf (`None` for `fallback`/`empty`).
    pub anchor: Option<NodeId>,
    /// Admission axis derived from the projected rectangle.
    pub axis: Axis,
    /// Rectangle the axis was derived from (anchor leaf rect or target
    /// domain bounds; `empty` targets always carry the domain bounds).
    pub projected: Rect,
    /// Leaf count in the target tree before insertion (0 for empty).
    pub target_leaves: usize,
}

/// Single-domain convergence routing: absent sessions run the existing seed
/// route, converged sessions run the ordinary operation, and primitive
/// errors return a typed rejection with no operation and no reseed.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ConvergeOutcome {
    /// No retained slot: run the existing fit/seed/relocation route.
    NoSession,
    /// Primitive ran (changed or exact): run the ordinary operation; reseed
    /// is forbidden from here on for this op.
    Converged,
    /// Scoped pending fence or primitive error: return this reply directly;
    /// do not run the operation and do not reseed. Boxed: cold-path only.
    Rejected(Box<CoreReply>),
}

pub fn session_domain_matches(session: &Session, domain: &OutputDomain) -> bool {
    session
        .domains()
        .iter()
        .find(|d| d.id == domain.id && d.workspace == domain.workspace)
        .is_some_and(|d| {
            d.bounds == domain.bounds && d.gap == domain.gap && d.adjacent == domain.adjacent
        })
}

pub fn session_usable(session: &Session) -> bool {
    session.divergence().is_none() && !session.has_pending()
}

/// A committed session with no tiled members, no deferred exceptions, and no
/// live-homed D23 minimize slots holds no topology and must not consume a
/// domain slot. A session holding only stored minimize slots is retained
/// while a slot home domain still exists so a later native restore can
/// return to the old slot; orphan-homed slots never retain.
pub fn committed_session_is_empty(session: &Session) -> bool {
    session.snapshot().windows.is_empty()
        && session.exception_count() == 0
        && !session.has_live_minimized_slot()
}

/// Trace-only ordered tree description with axes and nested shares:
/// leaf count plus `L` per leaf and `H[s,..]`/`V[s,..]` per group in
/// child order, capped at [`STARTUP_TRACE_MAX_TOPOLOGY`] characters.
fn describe_topology(tree: &Node) -> (usize, String) {
    fn walk(node: &Node, out: &mut String, leaves: &mut usize) {
        match node {
            Node::Leaf { .. } => {
                *leaves += 1;
                out.push('L');
            }
            Node::Group {
                axis,
                children,
                shares,
                ..
            } => {
                out.push(match axis {
                    Axis::Horizontal => 'H',
                    Axis::Vertical => 'V',
                });
                out.push('[');
                for (i, share) in shares.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    out.push_str(&share.to_string());
                }
                out.push_str("](");
                for (i, child) in children.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    walk(child, out, leaves);
                }
                out.push(')');
            }
        }
    }
    let mut out = String::new();
    let mut leaves = 0usize;
    walk(tree, &mut out, &mut leaves);
    if out.len() > STARTUP_TRACE_MAX_TOPOLOGY {
        out.truncate(STARTUP_TRACE_MAX_TOPOLOGY);
    }
    (leaves, out)
}

/// Refresh a fallback startup trace with the seeded resulting tree, if the
/// domain now retains one. Fitted traces already carry the fit tree and
/// are untouched.
fn refresh_startup_seed_tree(engine: &mut Engine, event: &CoreEvent) {
    if engine
        .last_startup_fit_trace
        .as_ref()
        .is_none_or(|trace| trace.outcome != "fallback")
    {
        return;
    }
    let seeded = engine
        .session(&event.domain_key)
        .and_then(|session| session.tree_for(&event.domain_key))
        .map(describe_topology);
    if let (Some(trace), Some((leaves, topology))) =
        (engine.last_startup_fit_trace.as_mut(), seeded)
    {
        trace.leaves = leaves;
        trace.topology = topology;
    }
}

impl Default for Engine {
    fn default() -> Self {
        Self {
            policy: default_policy(),
            sessions: BTreeMap::new(),
            outer_gaps: BTreeMap::new(),
            owner: None,
            generation: None,
            last_convergence: None,
            last_adoption_fit: None,
            last_startup_fit_trace: None,
            last_send_placement: None,
            converged_this_op: false,
            fixed_size_admission: false,
            fixed_size_predicate: crate::size_hints::FixedSizePredicate::BothAxes,
            maximized_focus_fence: false,
            fixed_send_admission: false,
            anchor_fallback: crate::admission::AnchorFallback::default(),
            no_space: crate::admission::NoSpaceResult::default(),
            last_fixed_admission: None,
            last_migration: None,
            last_drag_preview_stage: None,
            admission_seq_next: 0,
        }
    }
}

impl Engine {
    /// Empty retained engine.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Retained engine carrying an explicit layout policy.
    #[must_use]
    pub fn with_policy(policy: std::sync::Arc<dyn LayoutPolicy>) -> Self {
        Self {
            policy,
            ..Self::default()
        }
    }

    /// Selected layout policy carried into retained sessions.
    #[must_use]
    pub fn policy(&self) -> &std::sync::Arc<dyn LayoutPolicy> {
        &self.policy
    }

    /// Number of retained domains.
    #[must_use]
    pub fn retained_domains(&self) -> usize {
        self.sessions.len()
    }

    /// Retained owner binding, if any.
    #[must_use]
    pub fn owner(&self) -> Option<&OwnerId> {
        self.owner.as_ref()
    }

    /// Retained generation binding, if any.
    #[must_use]
    pub fn generation(&self) -> Option<&GenerationId> {
        self.generation.as_ref()
    }

    /// Binding sync: on owner/generation change (adapter restart) discard the
    /// world map and gap map, then rebind.
    pub fn sync_binding(&mut self, owner: &OwnerId, generation: &GenerationId) {
        let owner_changed = self
            .owner
            .as_ref()
            .is_none_or(|o| o.as_str() != owner.as_str());
        let generation_changed = self
            .generation
            .as_ref()
            .is_none_or(|g| g.as_str() != generation.as_str());
        if owner_changed || generation_changed {
            self.sessions.clear();
            self.outer_gaps.clear();
            self.admission_seq_next = 0;
            self.owner = Some(owner.clone());
            self.generation = Some(generation.clone());
        }
    }

    /// Borrow a retained session.
    #[must_use]
    pub fn session(&self, key: &DomainKey) -> Option<&Session> {
        self.sessions.get(key)
    }

    /// Mutably borrow a retained session.
    #[must_use]
    pub fn session_mut(&mut self, key: &DomainKey) -> Option<&mut Session> {
        self.sessions.get_mut(key)
    }

    /// D23 close cleanup: forget the stored minimize restore slot for a
    /// window on one retained domain (see [`Session::forget_minimized_slot`]).
    /// Returns false for unknown domains or absent slots.
    pub fn forget_minimized_slot(&mut self, key: &DomainKey, window: &WindowId) -> bool {
        self.sessions
            .get_mut(key)
            .is_some_and(|session| session.forget_minimized_slot(window))
    }

    /// Whether a domain slot exists (including empty/stale slots owned by the
    /// normal seeding path).
    #[must_use]
    pub fn contains(&self, key: &DomainKey) -> bool {
        self.sessions.contains_key(key)
    }

    /// Borrowed ordered domain keys for relocation scans.
    pub fn keys(&self) -> impl Iterator<Item = &DomainKey> {
        self.sessions.keys()
    }

    /// Number of slots including stale ones (relocation capacity check).
    #[must_use]
    pub fn len(&self) -> usize {
        self.sessions.len()
    }

    /// Whether no domain slot is retained.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.sessions.is_empty()
    }

    /// Ordered retained domain keys.
    #[must_use]
    pub fn domain_keys(&self) -> Vec<DomainKey> {
        self.sessions.keys().cloned().collect()
    }

    /// Retained outer gap for a domain, if any.
    #[must_use]
    pub fn outer_gap(&self, key: &DomainKey) -> Option<i32> {
        self.outer_gaps.get(key).copied()
    }

    /// Borrowed outer-gap entry for protocol fence comparisons.
    #[must_use]
    pub fn outer_gap_ref(&self, key: &DomainKey) -> Option<&i32> {
        self.outer_gaps.get(key)
    }

    /// Last single-domain convergence report for protocol logging, if the
    /// current [`Engine::handle`] converged before its ordinary operation.
    /// Bounded counts plus correlation/op only, never native identifiers.
    #[must_use]
    pub fn last_convergence(&self) -> Option<&EngineConvergenceReport> {
        self.last_convergence.as_ref()
    }

    /// Last fresh adoption-fit report for protocol logging, if the current
    /// [`Engine::handle`] attempted a fresh adoption. `None` for retained
    /// reconciliations so they stay silent.
    #[must_use]
    pub fn last_adoption_fit(&self) -> Option<&EngineAdoptionFitReport> {
        self.last_adoption_fit.as_ref()
    }

    /// Trace-only startup fit placement diagnostic for the current op, if a
    /// fresh adoption was attempted. Trace boundary only; `None` for
    /// retained reconciliations.
    #[must_use]
    pub fn last_startup_fit_trace(&self) -> Option<&EngineStartupFitTrace> {
        self.last_startup_fit_trace.as_ref()
    }

    /// Trace-only send placement diagnostic for the current op, if a
    /// `MoveToWorkspace` plan was proposed. Trace boundary only.
    #[must_use]
    pub fn last_send_placement(&self) -> Option<&EngineSendPlacementTrace> {
        self.last_send_placement.as_ref()
    }

    /// Whether opt-in Q2 fixed-size float admission is enabled.
    #[must_use]
    pub fn fixed_size_admission(&self) -> bool {
        self.fixed_size_admission
    }

    /// Enable or disable opt-in Q2 fixed-size float admission (D1-D8).
    /// Propagates to every retained session on store; fresh sessions
    /// adopt it at creation. Never touches topology or revision.
    pub fn set_fixed_size_admission(&mut self, enabled: bool) {
        self.fixed_size_admission = enabled;
        for session in self.sessions.values_mut() {
            session.set_fixed_size_admission(enabled);
        }
    }

    /// Whether the opt-in G-06 maximized directional focus fence is enabled.
    #[must_use]
    pub fn maximized_focus_fence(&self) -> bool {
        self.maximized_focus_fence
    }

    /// Enable or disable the opt-in G-06 maximized directional focus fence
    /// (REQ-MAX-08). Propagates to every retained session on store; fresh
    /// sessions adopt it at creation. Never touches topology or revision.
    pub fn set_maximized_focus_fence(&mut self, enabled: bool) {
        self.maximized_focus_fence = enabled;
        for session in self.sessions.values_mut() {
            session.set_maximized_focus_fence(enabled);
        }
    }

    /// Whether deferred sends for automatic fixed-size floats are enabled.
    #[must_use]
    pub fn fixed_send_admission(&self) -> bool {
        self.fixed_send_admission
    }

    /// Enable or disable deferred sends for automatic fixed-size floats
    /// (Windows workspace sends). Propagates to every retained session on
    /// store; fresh sessions adopt it at creation. Never touches topology
    /// or revision.
    pub fn set_fixed_send_admission(&mut self, enabled: bool) {
        self.fixed_send_admission = enabled;
        for session in self.sessions.values_mut() {
            session.set_fixed_send_admission(enabled);
        }
    }

    /// Current R-SPC-04 D1 fixed-size admission predicate. Both-axes
    /// default (current delivered behavior).
    #[must_use]
    pub fn fixed_size_predicate(&self) -> crate::size_hints::FixedSizePredicate {
        self.fixed_size_predicate
    }

    /// Select the fixed-size admission predicate for subsequent admissions
    /// only. Propagates to every retained session on store; fresh sessions
    /// adopt it at creation. Never touches topology, revision, or existing
    /// automatic/override marks (D2: no reclassification).
    pub fn set_fixed_size_predicate(&mut self, predicate: crate::size_hints::FixedSizePredicate) {
        self.fixed_size_predicate = predicate;
        for session in self.sessions.values_mut() {
            session.set_fixed_size_predicate(predicate);
        }
    }

    /// Current D2 anchor fallback. Largest-splittable default.
    #[must_use]
    pub fn anchor_fallback(&self) -> crate::admission::AnchorFallback {
        self.anchor_fallback
    }

    /// Select the D2 anchor fallback for subsequent capacity decisions
    /// only. Propagates to every retained session; fresh sessions adopt
    /// it at creation. Never touches topology, revision, or existing
    /// classifications: no reflow or retry on Apply (D10).
    pub fn set_anchor_fallback(&mut self, fallback: crate::admission::AnchorFallback) {
        self.anchor_fallback = fallback;
        for session in self.sessions.values_mut() {
            session.set_anchor_fallback(fallback);
        }
    }

    /// Current D3 no-space result. Float default.
    #[must_use]
    pub fn no_space_result(&self) -> crate::admission::NoSpaceResult {
        self.no_space
    }

    /// Select the D3 no-space result for subsequent admission or actual
    /// capacity-repair decisions only. Propagates to every retained
    /// session; fresh sessions adopt it at creation. Never touches
    /// topology, revision, or existing classifications: retained overlap
    /// allocations stay valid until a real capacity decision (D10).
    pub fn set_no_space_result(&mut self, result: crate::admission::NoSpaceResult) {
        self.no_space = result;
        for session in self.sessions.values_mut() {
            session.set_no_space_result(result);
        }
    }

    /// Last fixed-size admission report for protocol logging, if the
    /// current [`Engine::handle`] admitted automatic fixed floats.
    /// Bounded counts plus correlation/op/reason only.
    #[must_use]
    pub fn last_fixed_admission(&self) -> Option<&EngineFixedAdmissionReport> {
        self.last_fixed_admission.as_ref()
    }

    /// Last whole-workspace migration report for protocol logging, if the
    /// current [`Engine::handle`] completed an explicit migration.
    /// Bounded counts plus correlation/direction only.
    #[must_use]
    pub fn last_migration(&self) -> Option<&EngineMigrationReport> {
        self.last_migration.as_ref()
    }

    /// Last read-only drag-preview diagnostic stage for protocol logging,
    /// if the current [`Engine::handle`] refused a preview with an
    /// ambiguous kind. `None` otherwise so successes and distinctly-kinded
    /// refusals stay silent.
    #[must_use]
    pub fn last_drag_preview_stage(&self) -> Option<&EngineDragPreviewStage> {
        self.last_drag_preview_stage.as_ref()
    }

    /// Record the read-only drag-preview diagnostic stage for the current
    /// op. Log-only: never touches sessions, replies, or commits.
    fn note_drag_preview_stage(
        &mut self,
        correlation: &CorrelationId,
        base: &'static str,
        paired: bool,
    ) {
        self.last_drag_preview_stage = Some(EngineDragPreviewStage {
            correlation: correlation.clone(),
            stage: drag_preview_stage_token(base, paired),
        });
    }

    /// Carry the Engine admission settings into a session without
    /// touching owner, generation, revision, divergence, pending, or drag
    /// state. Fixed-size opt-ins plus the D2/D3 capacity policy ride
    /// together; topology and classifications are never reflowed.
    fn adopt_session_policy(&self, session: &mut Session) {
        session.set_fixed_size_admission(self.fixed_size_admission);
        session.set_fixed_size_predicate(self.fixed_size_predicate);
        session.set_maximized_focus_fence(self.maximized_focus_fence);
        session.set_fixed_send_admission(self.fixed_send_admission);
        session.set_anchor_fallback(self.anchor_fallback);
        session.set_no_space_result(self.no_space);
    }

    /// Current Engine-wide admission sequence floor: the recorded
    /// high-water mark over every live retained counter. The stored mark
    /// survives retired (emptied) sessions whose counters vanish with
    /// them; live sessions contribute directly so an unrecorded store
    /// still floors correctly.
    fn admission_seq_floor(&self) -> u64 {
        let mut floor = self.admission_seq_next;
        for session in self.sessions.values() {
            floor = floor.max(session.admission_counter_value());
        }
        floor
    }

    /// Advance a working session's next admission sequence to the
    /// Engine-wide floor before admission work, preserving every tracked
    /// lifetime sequence. Idempotent; never moves backwards.
    fn sync_working_counter(&self, session: &mut Session) {
        session.sync_admission_counter(self.admission_seq_floor());
    }

    /// Record a stored session's counter into the Engine-wide high-water
    /// mark. Called on every store so the floor survives retired
    /// sessions; a plain max that never wraps.
    fn note_working_counter(&mut self, session: &Session) {
        self.admission_seq_next = self
            .admission_seq_next
            .max(session.admission_counter_value());
    }

    /// Predicate-selected candidate check for the retained Engine setting.
    /// Fixed hints under the predicate, not born fullscreen (D5 bypass),
    /// not already floating, and no adapter-asserted tile win (D3 suppress
    /// signal). Sticky stays intentional (D6).
    fn is_fixed_candidate_with(
        window: &crate::seed::EngineWindow,
        predicate: crate::size_hints::FixedSizePredicate,
    ) -> bool {
        !window.floating
            && !window.fullscreen
            && !window.sticky
            && !window.fixed_suppress
            && crate::size_hints::is_fixed_size_with(window.hints, predicate)
    }

    /// Record the bounded fixed-size admission diagnostic for this op
    /// when the opt-in admitted automatic floats. Correlation plus
    /// counts and the fixed decision token only, never identifiers.
    /// Committed outcomes only: callers invoke this after the store
    /// commits, so exact-match (nothing admitted) and failed stores stay
    /// silent instead of logging success. A later ordinary-op refusal
    /// does not retract the report because the convergence itself
    /// committed and persists by design.
    fn note_fixed_admission(
        &mut self,
        correlation: &CorrelationId,
        op: &'static str,
        evaluated: usize,
        admitted_before: usize,
        session: &Session,
    ) {
        if !self.fixed_size_admission {
            return;
        }
        let admitted = session
            .automatic_fixed_count()
            .saturating_sub(admitted_before);
        if admitted == 0 {
            return;
        }
        self.last_fixed_admission = Some(EngineFixedAdmissionReport {
            correlation: correlation.clone(),
            op,
            evaluated,
            admitted,
            reason: "fixed-equal",
        });
    }

    /// Converge one retained single-domain session to the complete current
    /// observation before its ordinary operation.
    ///
    /// Uses the existing [`Session::converge_observation`] primitive with the
    /// complete carried window set (`floating` plus advisory `fit_excluded`
    /// as carried, with native `fullscreen`/`sticky` overlays for the
    /// fixed-size classifier) and the carried focus. Preserves
    /// survivor topology, exact-match revision semantics (no bump on zeros),
    /// and first-time fit/seed ([`ConvergeOutcome::NoSession`] runs the
    /// existing seed route). Owner/generation mismatches return terminal
    /// divergence without mutating (never diverging the retained session).
    /// Any other primitive error returns its typed rejection and the caller
    /// must not run the operation or reseed.
    fn converge_for_single_domain(
        &mut self,
        event: &CoreEvent,
        op: &'static str,
    ) -> ConvergeOutcome {
        let Some(session) = self.sessions.get(&event.domain_key) else {
            return ConvergeOutcome::NoSession;
        };
        if session.owner() != &event.owner {
            return ConvergeOutcome::Rejected(Box::new(CoreReply::Diverged(
                DivergenceKind::OwnerMismatch,
            )));
        }
        if session.generation() != &event.generation {
            return ConvergeOutcome::Rejected(Box::new(CoreReply::Diverged(
                DivergenceKind::GenerationMismatch,
            )));
        }
        let base = session.accepted_revision();
        let observation = crate::seed::session_observation_for(
            &event.owner,
            &event.generation,
            base,
            event.fingerprint,
            &event.windows,
        );
        let focus = if event.focused_window.0.is_empty() {
            None
        } else {
            Some(&event.focused_window)
        };
        let admitted_before = self
            .sessions
            .get(&event.domain_key)
            .map(|session| session.automatic_fixed_count())
            .unwrap_or(0);
        let predicate = self.fixed_size_predicate;
        let evaluated = event
            .windows
            .iter()
            .filter(|window| Self::is_fixed_candidate_with(window, predicate))
            .count();
        // Retained counters predate other domains' admissions; advance
        // before convergence assigns serial-leg sequences.
        let floor = self.admission_seq_floor();
        let Some(session_mut) = self.sessions.get_mut(&event.domain_key) else {
            return ConvergeOutcome::NoSession;
        };
        session_mut.sync_admission_counter(floor);
        match session_mut.converge_observation(&observation, focus) {
            Ok(counts) => {
                self.converged_this_op = true;
                // Record the converged counter so the high-water mark
                // survives a later retirement of this slot.
                let counter = session_mut.admission_counter_value();
                self.admission_seq_next = self.admission_seq_next.max(counter);
                // D23 close cleanup: a valid converged observation forgets
                // explicitly closed minimize slots AFTER convergence (an
                // ordinary closed leaf would otherwise mint a slot above)
                // and BEFORE retirement (a slot-only session must retire).
                // Revision-neutral bookkeeping; fenced by the converge
                // itself (rejected/diverged observations never reach here,
                // so the adapter must retry them with the close retained).
                for window in &event.closed_windows {
                    session_mut.forget_minimized_slot(window);
                }
                if counts.removed + counts.admitted + counts.flags_adopted > 0 {
                    self.last_convergence = Some(EngineConvergenceReport {
                        correlation: event.correlation.clone(),
                        op,
                        removed: counts.removed,
                        admitted: counts.admitted,
                        flags_adopted: counts.flags_adopted,
                    });
                }
                let admitted_after = self
                    .sessions
                    .get(&event.domain_key)
                    .map(|session| session.automatic_fixed_count())
                    .unwrap_or(admitted_before);
                // Inlined fixed-admission note (borrows the count, not the
                // session) so the mutable report write needs no alias.
                if self.fixed_size_admission && admitted_after.saturating_sub(admitted_before) > 0 {
                    self.last_fixed_admission = Some(EngineFixedAdmissionReport {
                        correlation: event.correlation.clone(),
                        op,
                        evaluated,
                        admitted: admitted_after.saturating_sub(admitted_before),
                        reason: "fixed-equal",
                    });
                }
                ConvergeOutcome::Converged
            }
            Err(ProposeError::PendingExists) => {
                ConvergeOutcome::Rejected(Box::new(CoreReply::Rejected {
                    kind: PENDING_EXISTS_KIND,
                    message: PENDING_EXISTS_MESSAGE,
                }))
            }
            Err(ProposeError::Diverged(reason)) => {
                ConvergeOutcome::Rejected(Box::new(CoreReply::Diverged(reason)))
            }
            Err(error) => ConvergeOutcome::Rejected(Box::new(CoreReply::Rejected {
                kind: error.kind(),
                message: error.message(),
            })),
        }
    }

    /// Converge one assembled paired Session once on the complete carried
    /// observation, then split/store it atomically so valid convergence
    /// persists even if the ordinary command refuses.
    ///
    /// Same [`Session::converge_observation`] primitive as the single-domain
    /// path, called exactly once on the assembled two-domain world (never two
    /// per-domain primitives): the pair already carries BOTH domains, so one
    /// call converges source and target membership together. Focus rides from
    /// the source observation, the observation revision rides from the
    /// assembled Session, and nothing is fabricated. On success the converged
    /// pair splits/stores through the existing canonical machinery
    /// (revisions and exception state preserved, no new machinery) before the
    /// caller rebuilds its command observation at the converged revision.
    /// Primitive errors return their typed reply with no store and no command.
    fn converge_assembled_pair(
        &mut self,
        session: &mut Session,
        event: &CoreEvent,
        op: &'static str,
        source_key: &DomainKey,
        target_key: &DomainKey,
    ) -> Result<(u64, SessionObservation), Box<CoreReply>> {
        let base = session.accepted_revision();
        let observation = crate::seed::session_observation_for(
            &event.owner,
            &event.generation,
            base,
            event.fingerprint,
            &event.windows,
        );
        let focus = if event.focused_window.0.is_empty() {
            None
        } else {
            Some(&event.focused_window)
        };
        let admitted_before = session.automatic_fixed_count();
        let predicate = self.fixed_size_predicate;
        let evaluated = event
            .windows
            .iter()
            .filter(|window| Self::is_fixed_candidate_with(window, predicate))
            .count();
        // Pair assembly keeps only the component max; advance to the
        // Engine-wide floor before convergence assigns serial-leg
        // sequences. The canonical store below records the result.
        self.sync_working_counter(session);
        match session.converge_observation(&observation, focus) {
            Ok(counts) => {
                self.converged_this_op = true;
                // D23 close cleanup on the assembled pair, same fencing as
                // the single-domain path: AFTER convergence, BEFORE the
                // canonical store below (which retires slot-only sessions).
                for window in &event.closed_windows {
                    session.forget_minimized_slot(window);
                }
                if counts.removed + counts.admitted + counts.flags_adopted > 0 {
                    self.last_convergence = Some(EngineConvergenceReport {
                        correlation: event.correlation.clone(),
                        op,
                        removed: counts.removed,
                        admitted: counts.admitted,
                        flags_adopted: counts.flags_adopted,
                    });
                }
                if !self.store_canonical_pair(
                    source_key.clone(),
                    target_key.clone(),
                    session.clone(),
                    event.outer_gap,
                ) {
                    return Err(Box::new(CoreReply::SnapshotInvalid {
                        message: OBSERVATION_MESSAGE,
                        detail: "commit-rejected",
                    }));
                }
                // Reported only after the store commits: a failed store
                // must not log a successful admission.
                self.note_fixed_admission(
                    &event.correlation,
                    op,
                    evaluated,
                    admitted_before,
                    session,
                );
                let base = session.accepted_revision();
                let observation = crate::seed::session_observation_for(
                    &event.owner,
                    &event.generation,
                    base,
                    event.fingerprint,
                    &event.windows,
                );
                Ok((base, observation))
            }
            Err(ProposeError::PendingExists) => Err(Box::new(CoreReply::Rejected {
                kind: PENDING_EXISTS_KIND,
                message: PENDING_EXISTS_MESSAGE,
            })),
            Err(ProposeError::Diverged(reason)) => Err(Box::new(CoreReply::Diverged(reason))),
            Err(error) => Err(Box::new(CoreReply::Rejected {
                kind: error.kind(),
                message: error.message(),
            })),
        }
    }

    /// Remove a domain slot and its outer gap.
    pub fn remove(&mut self, key: &DomainKey) {
        self.sessions.remove(key);
        self.outer_gaps.remove(key);
    }

    /// Remember `window` as the last-active focus of the retained canonical
    /// session for `key`, in process, without touching topology, membership,
    /// revision, pending, gaps, or divergence state. The adapter calls this
    /// with its freshly observed project MRU before a cross-output focus
    /// request so the Engine plans toward the live target instead of a stale
    /// remembered one; the ordinary focus fences still apply inside the
    /// request. Returns `false` (no mutation) for unknown sessions or any
    /// [`Session::remember_focus`] binding failure.
    pub fn remember_domain_focus(&mut self, key: &DomainKey, window: &WindowId) -> bool {
        let Some(session) = self.sessions.get_mut(key) else {
            return false;
        };
        session.remember_focus(key, window)
    }

    /// Direct insert used only by pair-relocation paths that already validated
    /// on a clone and manage capacity explicitly. Normal commits must use
    /// [`Engine::store_committed`].
    pub fn insert_raw(&mut self, key: DomainKey, mut session: Session, outer_gap: i32) {
        // Retaining carries the selected policy and the fixed-size opt-in;
        // no revision, divergence, pending, gap, or topology state is touched.
        session.set_policy(self.policy.clone());
        self.adopt_session_policy(&mut session);
        self.note_working_counter(&session);
        self.outer_gaps.insert(key.clone(), outer_gap);
        self.sessions.insert(key, session);
    }

    /// Restore a session without an outer-gap entry (exact legacy restore
    /// when the source had no gap recorded).
    pub fn insert_session_only(&mut self, key: DomainKey, mut session: Session) {
        session.set_policy(self.policy.clone());
        self.adopt_session_policy(&mut session);
        self.note_working_counter(&session);
        self.sessions.insert(key, session);
    }

    /// Remove only the outer-gap entry (pair-relocation source teardown).
    pub fn remove_outer_gap(&mut self, key: &DomainKey) {
        self.outer_gaps.remove(key);
    }

    /// Record an outer-gap change without touching the session (update-gaps
    /// commit path after `Session::update_domain_gaps` succeeds).
    pub fn set_outer_gap(&mut self, key: DomainKey, outer_gap: i32) {
        self.outer_gaps.insert(key, outer_gap);
    }

    /// Take a usable session clone for the propose/commit path: the slot must
    /// exist, be divergence/pending free, match the observed domain exactly,
    /// and hold topology. Unusable or empty slots are retired lazily without
    /// proposing so a later admission can reuse the slot.
    pub fn take_usable_session(
        &mut self,
        domain_key: &DomainKey,
        domain: &OutputDomain,
    ) -> Option<Session> {
        let session = self.sessions.get(domain_key)?;
        if !session_usable(session) || !session_domain_matches(session, domain) {
            self.sessions.remove(domain_key);
            self.outer_gaps.remove(domain_key);
            return None;
        }
        if committed_session_is_empty(session) {
            self.sessions.remove(domain_key);
            self.outer_gaps.remove(domain_key);
            return None;
        }
        let mut working = session.clone();
        // The retained counter may predate other domains' admissions;
        // advance before any propose/commit work assigns sequences.
        working.sync_admission_counter(self.admission_seq_floor());
        Some(working)
    }

    /// Store a committed session: empty results retire the slot, otherwise
    /// the slot and outer gap are recorded.
    pub fn store_committed(&mut self, domain_key: DomainKey, mut session: Session, outer_gap: i32) {
        if committed_session_is_empty(&session) {
            self.sessions.remove(&domain_key);
            self.outer_gaps.remove(&domain_key);
            return;
        }
        session.set_policy(self.policy.clone());
        self.adopt_session_policy(&mut session);
        self.note_working_counter(&session);
        self.outer_gaps.insert(domain_key.clone(), outer_gap);
        self.sessions.insert(domain_key, session);
    }

    /// Canonical component view: retained domain identity/bounds/gap with
    /// adjacency stripped for pair assembly.
    #[must_use]
    pub fn canonical_component_domain(domain: &OutputDomain) -> OutputDomain {
        OutputDomain {
            id: domain.id.clone(),
            workspace: domain.workspace.clone(),
            bounds: domain.bounds,
            gap: domain.gap,
            adjacent: std::collections::BTreeMap::new(),
        }
    }

    /// Unique relocation source for a target key: the single retained domain
    /// sharing the workspace id on a different output. `None` when missing or
    /// ambiguous (fail closed, no mutation).
    #[must_use]
    pub fn find_unique_source_for_target(&self, target_key: &DomainKey) -> Option<DomainKey> {
        let mut source_key: Option<DomainKey> = None;
        for key in self.sessions.keys() {
            if key.workspace == target_key.workspace && key.output != target_key.output {
                if source_key.is_some() {
                    return None;
                }
                source_key = Some(key.clone());
            }
        }
        source_key
    }

    /// Whether the retained outer gap for `key` equals the carried value.
    #[must_use]
    pub fn outer_gap_matches(&self, key: &DomainKey, outer_gap: i32) -> bool {
        self.outer_gaps.get(key).copied() == Some(outer_gap)
    }

    /// Assemble a temporary directional view solely from canonical per-domain
    /// sessions. Never infers a tree from current geometry: selected
    /// cross-output paths require retained authoritative state.
    pub fn assemble_directional_pair(
        &self,
        source_domain: &OutputDomain,
        source_key: &DomainKey,
        target_domain: &OutputDomain,
        target_key: &DomainKey,
    ) -> Result<Session, &'static str> {
        let source_component = Self::canonical_component_domain(source_domain);
        let target_component = Self::canonical_component_domain(target_domain);
        let source = self
            .sessions
            .get(source_key)
            .filter(|session| {
                session_usable(session)
                    && !committed_session_is_empty(session)
                    && session_domain_matches(session, &source_component)
            })
            .cloned()
            .ok_or("canonical-source-unavailable")?;
        let target = match self.sessions.get(target_key) {
            None => None,
            Some(session)
                if session_usable(session)
                    && !committed_session_is_empty(session)
                    && session_domain_matches(session, &target_component) =>
            {
                Some(session.clone())
            }
            Some(_) => return Err("canonical-pair-unusable"),
        };
        Session::paired_from_canonical(
            &source,
            target.as_ref(),
            vec![source_domain.clone(), target_domain.clone()],
        )
        .map(|mut pair| {
            // Pair assembly keeps only the component max; advance to the
            // Engine-wide floor before convergence or planning assigns.
            pair.sync_admission_counter(self.admission_seq_floor());
            pair
        })
        .map_err(|error| match error {
            CanonicalPairError::MismatchedIdentity => "canonical-pair-identity-mismatch",
            CanonicalPairError::UnusableInput => "canonical-pair-unusable",
            CanonicalPairError::DomainMismatch => "canonical-pair-domain-mismatch",
            CanonicalPairError::DuplicateState => "canonical-pair-duplicate-state",
        })
    }

    /// Return a terminal two-domain transaction to the sole canonical state
    /// authority. The pair is never retained after this boundary.
    pub fn store_canonical_pair(
        &mut self,
        source_key: DomainKey,
        target_key: DomainKey,
        pair: Session,
        source_outer_gap: i32,
    ) -> bool {
        let Ok((source, target)) = pair.split_canonical_pair() else {
            return false;
        };
        // New destinations inherit the carried outer gap; existing
        // destinations keep their retained gap.
        let target_outer_gap = self
            .outer_gaps
            .get(&target_key)
            .copied()
            .unwrap_or(source_outer_gap);
        self.store_committed(source_key, source, source_outer_gap);
        if let Some(target) = target {
            self.store_committed(target_key, target, target_outer_gap);
        } else {
            self.sessions.remove(&target_key);
            self.outer_gaps.remove(&target_key);
        }
        true
    }

    /// Typed world-level entry point for the workspace-send, reconcile,
    /// update-gaps, and active-group request phases. Direct send/R4 request
    /// commits immediately with native assignment plus geometry.
    pub fn handle(&mut self, event: &CoreEvent) -> CoreReply {
        // Single-domain observation convergence before the ordinary operation:
        // one primitive with the complete current observation and focus keeps
        // survivor topology and preserves exact-match/fit/seed paths.
        // Primitive errors return a typed rejection with no operation and no
        // reseed; absent sessions run the existing seed route. Pair
        // (two-domain) moves/focuses converge once on the assembled
        // BOTH-domain world inside their request arms (same primitive, never
        // two per-domain calls). `run_retained` never converges again, so no
        // double converge.
        self.last_convergence = None;
        self.last_adoption_fit = None;
        self.last_startup_fit_trace = None;
        self.last_send_placement = None;
        self.last_fixed_admission = None;
        self.last_migration = None;
        self.last_drag_preview_stage = None;
        self.converged_this_op = false;
        match &event.command {
            CoreCommand::Reconcile => {
                // All-new IDs need fresh spatial adoption, not arbitrary ID-order convergence.
                // Known minimized restore slots are not all-new: a native restore
                // converges back to its stored old slot on the retained session.
                let disjoint = !event.windows.is_empty()
                    && self.sessions.get(&event.domain_key).is_some_and(|session| {
                        if session.owner() != &event.owner
                            || session.generation() != &event.generation
                        {
                            return false;
                        }
                        let snapshot = session.snapshot();
                        !event.windows.iter().any(|w| {
                            snapshot.windows.iter().any(|link| link.window == w.window)
                                || session.is_exception(&w.window)
                                || session.minimized_slot(&w.window).is_some()
                        })
                    });
                if disjoint {
                    self.remove(&event.domain_key);
                }
                match self.converge_for_single_domain(event, "reconcile") {
                    ConvergeOutcome::Rejected(reply) => *reply,
                    _ => self.reconcile_request(event),
                }
            }
            CoreCommand::UpdateGaps => {
                match self.converge_for_single_domain(event, "update-gaps") {
                    ConvergeOutcome::Rejected(reply) => *reply,
                    _ => self.update_gaps_request(event),
                }
            }
            CoreCommand::SendToWorkspace { .. } => self.transfer_request(event, false),
            CoreCommand::SendToOutput { .. } => self.transfer_request(event, true),
            CoreCommand::MigrateWorkspace { .. } => self.migrate_workspace_request(event),
            CoreCommand::ActiveGroup => self.active_group_request(event),
            CoreCommand::ReleaseDomain => self.release_request(event),
            CoreCommand::ToggleFloat { .. } => {
                match self.converge_for_single_domain(event, "toggle-float") {
                    ConvergeOutcome::Rejected(reply) => *reply,
                    _ => self.toggle_float_request(event),
                }
            }
            CoreCommand::ToggleOrientation { .. } => {
                match self.converge_for_single_domain(event, "toggle-orientation") {
                    ConvergeOutcome::Rejected(reply) => *reply,
                    _ => self.toggle_orientation_request(event),
                }
            }
            CoreCommand::Move { .. } => {
                if event
                    .directional
                    .as_ref()
                    .is_none_or(|pair| pair.len() != 2)
                    && let ConvergeOutcome::Rejected(reply) =
                        self.converge_for_single_domain(event, "move")
                {
                    return *reply;
                }
                self.directional_move_request(event)
            }
            CoreCommand::Focus { .. } => {
                if event
                    .directional
                    .as_ref()
                    .is_none_or(|pair| pair.len() != 2)
                    && let ConvergeOutcome::Rejected(reply) =
                        self.converge_for_single_domain(event, "focus")
                {
                    return *reply;
                }
                self.directional_focus_request(event)
            }
            CoreCommand::Resize { .. } => match self.converge_for_single_domain(event, "resize") {
                ConvergeOutcome::Rejected(reply) => *reply,
                _ => self.resize_request(event),
            },
            CoreCommand::PointerResize { .. } => {
                match self.converge_for_single_domain(event, "pointer-resize") {
                    ConvergeOutcome::Rejected(reply) => *reply,
                    _ => self.pointer_resize_request(event),
                }
            }
            CoreCommand::DragDrop { source, .. } => {
                // Occupied cross-output drops converge once on the assembled
                // BOTH-domain world inside the pair request below; the legacy
                // single-domain pre-convergence would refuse the combined
                // observation as cross-homed before the pair assembles.
                let cross_pair = source.as_ref().is_some_and(|key| *key != event.domain_key)
                    && event
                        .directional
                        .as_ref()
                        .is_some_and(|pair| pair.len() == 2);
                if cross_pair {
                    self.drag_drop_request(event)
                } else {
                    match self.converge_for_single_domain(event, "drag-drop") {
                        ConvergeOutcome::Rejected(reply) => *reply,
                        _ => self.drag_drop_request(event),
                    }
                }
            }
            CoreCommand::DragPreview { .. } => self.drag_preview_request(event),
        }
    }

    /// Fresh-domain seed anchor for public reconcile: the focused tiled
    /// member, else the first tiled member in observation order. Mirrors the
    /// KWin fresh-admit naming so fit eligibility (`window == focused`) and
    /// seed order match the admit route exactly. `None` when no tiled
    /// member exists (all-floating or truly empty observation).
    fn fresh_reconcile_seed_anchor(event: &CoreEvent) -> Option<WindowId> {
        if !event.focused_window.0.is_empty()
            && event
                .windows
                .iter()
                .any(|entry| entry.window == event.focused_window && !entry.floating)
        {
            return Some(event.focused_window.clone());
        }
        event
            .windows
            .iter()
            .find(|entry| !entry.floating)
            .map(|entry| entry.window.clone())
    }

    /// Ordinary-float observed focus (D4): the observed active window is a
    /// known floating exception homed in this domain. Tiled MRU stays
    /// authoritative internally and the native active never wedges
    /// projection: callers project with the retained tiled focus (or none)
    /// instead of refusing `focus-mismatch`. Diagnostic origin is never
    /// consulted: capacity and user floats converge alike.
    fn observed_float_focus(session: &Session, key: &DomainKey, focused: &WindowId) -> bool {
        session.exception_observed().iter().any(|observed| {
            &observed.window == focused
                && observed.floating
                && observed.output == key.output
                && observed.workspace == key.workspace
        })
    }

    /// Read-only adoption gate: true when the hinted projection of the clean
    /// fitted topology flags any leaf overconstrained. Projection failure is
    /// not infeasibility (plain validation already passed), so false.
    fn fitted_topology_is_min_infeasible(
        domain: &crate::session::OutputDomain,
        tree: &Node,
        links: &[crate::directional::WindowLink],
        windows: &[EngineWindow],
    ) -> bool {
        let by_window: BTreeMap<&WindowId, crate::size_hints::WindowSizeHints> =
            windows.iter().map(|w| (&w.window, w.hints)).collect();
        let by_leaf: BTreeMap<&NodeId, &WindowId> =
            links.iter().map(|l| (&l.leaf, &l.window)).collect();
        let resolve = |leaf: &NodeId| {
            by_leaf
                .get(leaf)
                .and_then(|window| by_window.get(*window))
                .copied()
                .unwrap_or_else(crate::size_hints::WindowSizeHints::none)
        };
        matches!(
            crate::size_hints::project_with_hints(tree, domain.bounds, domain.gap, &resolve),
            Ok(hinted) if !hinted.overconstrained.is_empty()
        )
    }

    /// Fresh floating-aware convergence build shared by fresh admit, fresh
    /// reconcile (all-floating tail), and toggle-float.
    ///
    /// Builds one empty session and runs the single convergence primitive
    /// over the complete carried observation, reporting nonzero counts under
    /// `report_op` and storing the converged session. Per-op gating (fresh
    /// slot, floating presence, unique source), empty-session creation
    /// failure, and every reply/follow-on stay at the call sites: `Err(None)`
    /// means `Session::new` failed, `Err(Some(reply))` is the mapped
    /// primitive error to return, and `Ok(base)` is the converged revision
    /// already stored (callers reload/project from the store so empty-retire
    /// stays exact). The reply is boxed per the existing
    /// [`ConvergeOutcome::Rejected`] precedent (`CoreReply` is large).
    fn converge_fresh_floating(
        &mut self,
        event: &CoreEvent,
        report_op: &'static str,
    ) -> Result<u64, Option<Box<CoreReply>>> {
        let Ok(mut fresh) = Session::new(
            event.owner.clone(),
            event.generation.clone(),
            0,
            event.fingerprint,
            vec![event.domain.clone()],
        ) else {
            return Err(None);
        };
        fresh.set_policy(self.policy.clone());
        self.adopt_session_policy(&mut fresh);
        // Fresh domains start their counter at zero; advance to the
        // Engine-wide floor so serial legs order after retained domains.
        self.sync_working_counter(&mut fresh);
        let observation = crate::seed::session_observation_for(
            &event.owner,
            &event.generation,
            fresh.accepted_revision(),
            event.fingerprint,
            &event.windows,
        );
        let focus = if event.focused_window.0.is_empty() {
            None
        } else {
            Some(&event.focused_window)
        };
        match fresh.converge_observation(&observation, focus) {
            Err(ProposeError::PendingExists) => Err(Some(Box::new(CoreReply::Rejected {
                kind: PENDING_EXISTS_KIND,
                message: PENDING_EXISTS_MESSAGE,
            }))),
            Err(ProposeError::Diverged(reason)) => Err(Some(Box::new(CoreReply::Diverged(reason)))),
            Err(error) => Err(Some(Box::new(CoreReply::Rejected {
                kind: error.kind(),
                message: error.message(),
            }))),
            Ok(counts) => {
                self.converged_this_op = true;
                if counts.removed + counts.admitted + counts.flags_adopted > 0 {
                    self.last_convergence = Some(EngineConvergenceReport {
                        correlation: event.correlation.clone(),
                        op: report_op,
                        removed: counts.removed,
                        admitted: counts.admitted,
                        flags_adopted: counts.flags_adopted,
                    });
                }
                let predicate = self.fixed_size_predicate;
                let evaluated = event
                    .windows
                    .iter()
                    .filter(|window| Self::is_fixed_candidate_with(window, predicate))
                    .count();
                self.note_fixed_admission(&event.correlation, report_op, evaluated, 0, &fresh);
                let base = fresh.accepted_revision();
                self.store_committed(event.domain_key.clone(), fresh, event.outer_gap);
                Ok(base)
            }
        }
    }

    /// Shared fresh-domain admission route: recursive-cut fit fast path,
    /// floating-aware convergence build, deterministic seed order, seeding,
    /// relocation, propose/commit, and store.
    ///
    /// Invoked by the fresh public reconcile path (`report_op = "reconcile"`)
    /// with the same anchor/placement inputs, so startup fit, seed fallback,
    /// focus-last placement, mixed float+tiled handling, and revision shape
    /// stay byte-identical without fabricating a synthetic admit command.
    ///
    /// Owns the fresh adoption-fit decision for protocol logging: exactly one
    /// [`EngineAdoptionFitReport`] per actual fresh attempt (no retained slot
    /// at entry, no explicit placement, anchor is focus). `fitted` only when
    /// the fit commits; any decline or commit failure records `fallback`.
    fn fresh_admit_shared(
        &mut self,
        event: &CoreEvent,
        window: &WindowId,
        output: &OutputId,
        workspace: &WorkspaceId,
        placement_bounds: Option<Rect>,
        report_op: &'static str,
    ) -> CoreReply {
        use crate::boundary::{TiledKind, TiledPlan};
        // Engine-owned fit decision: compute once, commit once. The report
        // records the actual commit result, never fitted on commit failure.
        let fresh_attempt = placement_bounds.is_none()
            && window.0 == event.focused_window.0
            && self.session(&event.domain_key).is_none();
        let fixed_present = self.fixed_size_admission
            && event
                .windows
                .iter()
                .any(|window| Self::is_fixed_candidate_with(window, self.fixed_size_predicate));
        if fresh_attempt && fixed_present {
            // Fixed-size members never join the fitted topology: they
            // float at admission (D1), so the fit declines to the
            // floating-aware convergence build below with the existing
            // fit-excluded token.
            self.last_adoption_fit = Some(EngineAdoptionFitReport {
                correlation: event.correlation.clone(),
                outcome: "fallback",
                windows: event.windows.len(),
                reason: crate::seed::FitDeclineReason::FitExcluded.as_str(),
                centre_splits: 0,
            });
            self.last_startup_fit_trace = Some(EngineStartupFitTrace {
                correlation: event.correlation.clone(),
                windows: event.windows.len(),
                domain_bounds: event.domain.bounds,
                inputs: event
                    .windows
                    .iter()
                    .take(STARTUP_TRACE_MAX_RECTS)
                    .map(|w| StartupInput {
                        window: w.window.clone(),
                        rect: w.rect,
                    })
                    .collect(),
                outcome: "fallback",
                reason: crate::seed::FitDeclineReason::FitExcluded.as_str(),
                centre_splits: 0,
                leaves: 0,
                topology: "-".to_owned(),
            });
        }
        if fresh_attempt && !fixed_present {
            match crate::seed::try_recursive_cut_fit_with_centre_count(
                &event.domain,
                &event.windows,
            ) {
                Ok((tree, links, centre_splits)) => {
                    let trace_inputs: Vec<StartupInput> = event
                        .windows
                        .iter()
                        .take(STARTUP_TRACE_MAX_RECTS)
                        .map(|w| StartupInput {
                            window: w.window.clone(),
                            rect: w.rect,
                        })
                        .collect();
                    // Fits needing a centre split (overlap/cascade never-tiled)
                    // decline to the sequential long-edge seed.
                    if centre_splits > 0 {
                        self.last_adoption_fit = Some(EngineAdoptionFitReport {
                            correlation: event.correlation.clone(),
                            outcome: "fallback",
                            windows: event.windows.len(),
                            reason: crate::seed::FitDeclineReason::CentreSplit.as_str(),
                            centre_splits,
                        });
                        self.last_startup_fit_trace = Some(EngineStartupFitTrace {
                            correlation: event.correlation.clone(),
                            windows: event.windows.len(),
                            domain_bounds: event.domain.bounds,
                            inputs: trace_inputs,
                            outcome: "fallback",
                            reason: crate::seed::FitDeclineReason::CentreSplit.as_str(),
                            centre_splits,
                            leaves: 0,
                            topology: "-".to_owned(),
                        });
                    } else if Self::fitted_topology_is_min_infeasible(
                        &event.domain,
                        &tree,
                        &links,
                        &event.windows,
                    ) {
                        // Clean but minimum-infeasible fitted topology takes the
                        // same seed instead of committing overconstrained tiles.
                        self.last_adoption_fit = Some(EngineAdoptionFitReport {
                            correlation: event.correlation.clone(),
                            outcome: "fallback",
                            windows: event.windows.len(),
                            reason: crate::seed::FitDeclineReason::MinInfeasible.as_str(),
                            centre_splits: 0,
                        });
                        self.last_startup_fit_trace = Some(EngineStartupFitTrace {
                            correlation: event.correlation.clone(),
                            windows: event.windows.len(),
                            domain_bounds: event.domain.bounds,
                            inputs: trace_inputs,
                            outcome: "fallback",
                            reason: crate::seed::FitDeclineReason::MinInfeasible.as_str(),
                            centre_splits: 0,
                            leaves: 0,
                            topology: "-".to_owned(),
                        });
                    } else {
                        let (trace_leaves, trace_topology) = describe_topology(&tree);
                        let focus_leaf = links
                            .iter()
                            .find(|l| l.window.0 == window.0)
                            .map(|l| l.leaf.clone());
                        let mut committed: Option<CoreReply> = None;
                        if let (Some(focus_leaf), Ok(mut fitted)) = (
                            focus_leaf,
                            Session::new(
                                event.owner.clone(),
                                event.generation.clone(),
                                0,
                                event.fingerprint,
                                vec![event.domain.clone()],
                            ),
                        ) {
                            fitted.set_policy(self.policy.clone());
                            self.adopt_session_policy(&mut fitted);
                            // The clean-fit roster shares one sequence
                            // number; take it from the Engine-wide floor so
                            // independent domains never tie except by fit.
                            self.sync_working_counter(&mut fitted);
                            let base = fitted.accepted_revision();
                            let observation = crate::seed::session_observation_for(
                                &event.owner,
                                &event.generation,
                                base,
                                event.fingerprint,
                                &event.windows,
                            );
                            if let Ok(plan) = fitted.propose_fitted_admit(
                                tree,
                                links,
                                focus_leaf,
                                window,
                                output,
                                workspace,
                                &observation,
                                &event.correlation,
                                &LifecycleCapabilities::full(),
                            ) {
                                let typed = CoreReply::Tiled(TiledPlan::from_lifecycle(
                                    TiledKind::Admit,
                                    &plan,
                                ));
                                if Self::commit_lifecycle(&mut fitted, &plan, event, base) {
                                    self.store_committed(
                                        event.domain_key.clone(),
                                        fitted,
                                        event.outer_gap,
                                    );
                                    committed = Some(typed);
                                }
                            }
                        }
                        if let Some(typed) = committed {
                            self.last_adoption_fit = Some(EngineAdoptionFitReport {
                                correlation: event.correlation.clone(),
                                outcome: "fitted",
                                windows: event.windows.len(),
                                reason: "ok",
                                centre_splits,
                            });
                            self.last_startup_fit_trace = Some(EngineStartupFitTrace {
                                correlation: event.correlation.clone(),
                                windows: event.windows.len(),
                                domain_bounds: event.domain.bounds,
                                inputs: trace_inputs,
                                outcome: "fitted",
                                reason: "ok",
                                centre_splits,
                                leaves: trace_leaves,
                                topology: trace_topology,
                            });
                            return typed;
                        }
                        self.last_adoption_fit = Some(EngineAdoptionFitReport {
                            correlation: event.correlation.clone(),
                            outcome: "fallback",
                            windows: event.windows.len(),
                            reason: "commit_failed",
                            centre_splits: 0,
                        });
                        self.last_startup_fit_trace = Some(EngineStartupFitTrace {
                            correlation: event.correlation.clone(),
                            windows: event.windows.len(),
                            domain_bounds: event.domain.bounds,
                            inputs: trace_inputs,
                            outcome: "fallback",
                            reason: "commit_failed",
                            centre_splits: 0,
                            leaves: 0,
                            topology: "-".to_owned(),
                        });
                    }
                }
                Err(reason) => {
                    self.last_adoption_fit = Some(EngineAdoptionFitReport {
                        correlation: event.correlation.clone(),
                        outcome: "fallback",
                        windows: event.windows.len(),
                        reason: reason.as_str(),
                        centre_splits: 0,
                    });
                    self.last_startup_fit_trace = Some(EngineStartupFitTrace {
                        correlation: event.correlation.clone(),
                        windows: event.windows.len(),
                        domain_bounds: event.domain.bounds,
                        inputs: event
                            .windows
                            .iter()
                            .take(STARTUP_TRACE_MAX_RECTS)
                            .map(|w| StartupInput {
                                window: w.window.clone(),
                                rect: w.rect,
                            })
                            .collect(),
                        outcome: "fallback",
                        reason: reason.as_str(),
                        centre_splits: 0,
                        leaves: 0,
                        topology: "-".to_owned(),
                    });
                }
            }
        }
        // D8 serial seed: the clean fit declined. The legacy
        // floating-aware build runs first where it applies; the legacy
        // seed owns relocation candidates and excluded members; the
        // atomic serial seed below owns genuinely fresh domains. Every
        // serial leg runs the ordinary anchor, the D2 fallback, then the
        // D3 no-space result; capacity floats plus fixed-size and
        // born-fullscreen exceptions join the same commit. No automatic
        // re-tile: floats persist until an explicit tile retry (D5).
        // Fresh floating-aware build: no retained slot, no relocation source,
        // and floating/fullscreen members present, so the tiled-only seed cannot run. One
        // empty session plus the same single convergence primitive admits
        // normal members and retains floating exceptions atomically; no staged
        // or fabricated observations. Born-fullscreen arrivals (D5) ride this
        // same build: direct fullscreen observations hold slotless without a
        // marker so the first exit classifies fresh. Fixed-size candidates (D1) ride this
        // same build: they converge to automatic floating exceptions instead
        // of joining the fitted/seed topology. Relocation candidates always
        // keep the legacy route byte-for-byte, as do all-normal fresh
        // observations.
        if self.session(&event.domain_key).is_none()
            && (event.windows.iter().any(|w| w.floating || w.fullscreen) || fixed_present)
            && self
                .find_unique_source_for_target(&event.domain_key)
                .is_none()
        {
            // Shared floating-aware convergence: `Session::new` failure
            // (`Err(None)`) falls through to the legacy route below with no
            // flags set; the projection below then misses the store and falls
            // through identically.
            if let Err(Some(reply)) = self.converge_fresh_floating(event, report_op) {
                return *reply;
            }
            // Fresh mixed float+tiled projection: convergence above already
            // admitted normals and retained floating exceptions; project the
            // newly converged session directly at its committed revision.
            if let Some(session) = self.session(&event.domain_key).cloned()
                && session
                    .snapshot()
                    .windows
                    .iter()
                    .any(|l| &l.window == window)
            {
                let mut focused = session;
                let _ = focused.sync_focus_from_window(&event.domain_key, window);
                if committed_session_is_empty(&focused) {
                    return CoreReply::Tiled(TiledPlan {
                        base_revision: focused.accepted_revision(),
                        policy_version: LIFECYCLE_POLICY_VERSION,
                        kind: TiledKind::Admit,
                        geometry: Vec::new(),
                        focus_domain: None,
                        focus_leaf: None,
                        float_window: None,
                        float_rect: None,
                    });
                }
                let (focus_domain, focus_leaf) = focused.focus();
                if let (Some(focus_domain), Some(focus_leaf)) = (focus_domain, focus_leaf) {
                    let hints = event
                        .windows
                        .iter()
                        .map(|entry| (entry.window.clone(), entry.hints))
                        .collect::<BTreeMap<_, _>>();
                    if let Some(plan) = project_retained_tiled_geometry(
                        &focused,
                        &event.domain_key,
                        event.domain.bounds,
                        event.domain.gap,
                        Some((focus_domain, focus_leaf)),
                        ProjectionKind::Reconcile,
                        &hints,
                        &event.windows,
                    ) {
                        if let Some(stored) = self.session_mut(&event.domain_key) {
                            *stored = focused;
                        }
                        refresh_startup_seed_tree(self, event);
                        return CoreReply::Tiled(TiledPlan {
                            base_revision: plan.base_revision,
                            policy_version: LIFECYCLE_POLICY_VERSION,
                            kind: TiledKind::Admit,
                            geometry: plan.geometry,
                            focus_domain: plan.focus_domain,
                            focus_leaf: plan.focus_leaf,
                            float_window: None,
                            float_rect: None,
                        });
                    }
                }
            }
            // Fixed-size anchor admitted as an automatic float (D1): no
            // tiled slot exists for it, so project the converged tiled
            // remainder (or nothing) instead of falling into the seed
            // below, which would refuse as a duplicate.
            if let Some(session) = self.session(&event.domain_key).cloned()
                && session.is_exception(window)
            {
                let focused = session;
                if committed_session_is_empty(&focused) {
                    return CoreReply::Tiled(TiledPlan {
                        base_revision: focused.accepted_revision(),
                        policy_version: LIFECYCLE_POLICY_VERSION,
                        kind: TiledKind::Admit,
                        geometry: Vec::new(),
                        focus_domain: None,
                        focus_leaf: None,
                        float_window: None,
                        float_rect: None,
                    });
                }
                let (focus_domain, focus_leaf) = focused.focus();
                if let (Some(focus_domain), Some(focus_leaf)) = (focus_domain, focus_leaf) {
                    let hints = event
                        .windows
                        .iter()
                        .map(|entry| (entry.window.clone(), entry.hints))
                        .collect::<BTreeMap<_, _>>();
                    if let Some(plan) = project_retained_tiled_geometry(
                        &focused,
                        &event.domain_key,
                        event.domain.bounds,
                        event.domain.gap,
                        Some((focus_domain, focus_leaf)),
                        ProjectionKind::Reconcile,
                        &hints,
                        &event.windows,
                    ) {
                        if let Some(stored) = self.session_mut(&event.domain_key) {
                            *stored = focused;
                        }
                        refresh_startup_seed_tree(self, event);
                        return CoreReply::Tiled(TiledPlan {
                            base_revision: plan.base_revision,
                            policy_version: LIFECYCLE_POLICY_VERSION,
                            kind: TiledKind::Admit,
                            geometry: plan.geometry,
                            focus_domain: plan.focus_domain,
                            focus_leaf: plan.focus_leaf,
                            float_window: None,
                            float_rect: None,
                        });
                    }
                }
            }
        }
        // Relocation candidate or excluded members: the full legacy
        // fresh route owns this domain (floating-aware convergence
        // build, then the deterministic seed). It preserves the
        // ambiguous-source fresh-seed rule and the gap-mismatch
        // atomicity contract exactly, and it never reclassifies
        // adapter-excluded (`fit_excluded`) members through
        // best-effort: exclusion stays a refusal/hold, not a float or
        // a tile. The atomic serial seed below only runs for genuinely
        // fresh domains with no relocation candidate and no excluded
        // members.
        if self
            .find_unique_source_for_target(&event.domain_key)
            .is_some()
            || event.windows.iter().any(|w| w.fit_excluded)
        {
            let seed_order = crate::seed::order_spatial_with_focus_last(
                event
                    .windows
                    .iter()
                    .filter(|w| w.window != *window)
                    .cloned()
                    .collect(),
                &event.focused_window,
                true,
            );
            let window = window.clone();
            let output = output.clone();
            let workspace = workspace.clone();
            let domain = event.domain.clone();
            let placement_explicit = placement_bounds;
            let reply = self.run_retained(
                event,
                seed_order,
                false,
                |session, observation| {
                    let placement = placement_explicit
                        .unwrap_or_else(|| crate::seed::seed_target_bounds(session, &domain));
                    session.propose(
                        &SessionCommand::Admit {
                            window: window.clone(),
                            output: output.clone(),
                            workspace: workspace.clone(),
                            exceptions: ExceptionFlags::none(),
                            exception_behavior: None,
                            placement_bounds: placement,
                            suppress_fixed_float: false,
                        },
                        observation,
                        &event.correlation,
                        &LifecycleCapabilities::full(),
                    )
                },
                |plan| CoreReply::Tiled(TiledPlan::from_lifecycle(TiledKind::Admit, plan)),
                Self::commit_lifecycle,
            );
            refresh_startup_seed_tree(self, event);
            return reply;
        }
        // Atomic serial seed for genuinely fresh domains: spatial
        // `(y, x, h, w)` with the anchor last (legacy seed parity:
        // survivors admit in spatial order, then the anchor wraps the
        // root); stable adapter order on ties.
        let ordered =
            crate::seed::order_spatial_with_focus_last(event.windows.clone(), window, true)
                .unwrap_or_else(|| event.windows.clone());
        let mut seeded = match Session::new(
            event.owner.clone(),
            event.generation.clone(),
            0,
            event.fingerprint,
            vec![event.domain.clone()],
        ) {
            Ok(session) => session,
            Err(_) => {
                return CoreReply::SnapshotInvalid {
                    message: OBSERVATION_MESSAGE,
                    detail: "seed-failed",
                };
            }
        };
        seeded.set_policy(self.policy.clone());
        self.adopt_session_policy(&mut seeded);
        // Serial seed legs take successive sequences; start them at the
        // Engine-wide floor so successful legs order after retained
        // domains instead of replaying their counters.
        self.sync_working_counter(&mut seeded);
        let seed_observation = crate::seed::session_observation_for(
            &event.owner,
            &event.generation,
            seeded.accepted_revision(),
            event.fingerprint,
            &ordered,
        );
        let seed_focus = if event.focused_window.0.is_empty() {
            None
        } else {
            Some(&event.focused_window)
        };
        let predicate = self.fixed_size_predicate;
        let evaluated = event
            .windows
            .iter()
            .filter(|entry| Self::is_fixed_candidate_with(entry, predicate))
            .count();
        match seeded.converge_observation(&seed_observation, seed_focus) {
            Ok(counts) => {
                self.converged_this_op = true;
                if counts.removed + counts.admitted + counts.flags_adopted > 0 {
                    self.last_convergence = Some(EngineConvergenceReport {
                        correlation: event.correlation.clone(),
                        op: report_op,
                        removed: counts.removed,
                        admitted: counts.admitted,
                        flags_adopted: counts.flags_adopted,
                    });
                }
                self.note_fixed_admission(&event.correlation, report_op, evaluated, 0, &seeded);
                self.store_committed(event.domain_key.clone(), seeded, event.outer_gap);
            }
            Err(ProposeError::PendingExists) => {
                return CoreReply::Rejected {
                    kind: PENDING_EXISTS_KIND,
                    message: PENDING_EXISTS_MESSAGE,
                };
            }
            Err(ProposeError::Diverged(reason)) => {
                return CoreReply::Diverged(reason);
            }
            Err(_) => {
                return CoreReply::SnapshotInvalid {
                    message: OBSERVATION_MESSAGE,
                    detail: "seed-failed",
                };
            }
        }
        // Serial-seed reply: a tiled anchor projects with anchor focus;
        // a capacity-floated anchor projects the remainder with the
        // newcomer as the native float target (core tiled focus stays on
        // the surviving MRU; the adapter owns native activation in the
        // route integration). Fixed-size and born-fullscreen anchors
        // keep their existing float-free remainder shapes.
        let Some(session) = self.session(&event.domain_key).cloned() else {
            return CoreReply::SnapshotInvalid {
                message: OBSERVATION_MESSAGE,
                detail: "seed-failed",
            };
        };
        let is_capacity = session.is_capacity_float(window);
        if session
            .snapshot()
            .windows
            .iter()
            .any(|l| &l.window == window)
        {
            let mut focused = session;
            let _ = focused.sync_focus_from_window(&event.domain_key, window);
            if committed_session_is_empty(&focused) {
                return CoreReply::Tiled(TiledPlan {
                    base_revision: focused.accepted_revision(),
                    policy_version: LIFECYCLE_POLICY_VERSION,
                    kind: TiledKind::Admit,
                    geometry: Vec::new(),
                    focus_domain: None,
                    focus_leaf: None,
                    float_window: None,
                    float_rect: None,
                });
            }
            let (focus_domain, focus_leaf) = focused.focus();
            if let (Some(focus_domain), Some(focus_leaf)) = (focus_domain, focus_leaf) {
                let hints = event
                    .windows
                    .iter()
                    .map(|entry| (entry.window.clone(), entry.hints))
                    .collect::<BTreeMap<_, _>>();
                if let Some(plan) = project_retained_tiled_geometry(
                    &focused,
                    &event.domain_key,
                    event.domain.bounds,
                    event.domain.gap,
                    Some((focus_domain, focus_leaf)),
                    ProjectionKind::Reconcile,
                    &hints,
                    &event.windows,
                ) {
                    if let Some(stored) = self.session_mut(&event.domain_key) {
                        *stored = focused;
                    }
                    refresh_startup_seed_tree(self, event);
                    return CoreReply::Tiled(TiledPlan {
                        base_revision: plan.base_revision,
                        policy_version: LIFECYCLE_POLICY_VERSION,
                        kind: TiledKind::Admit,
                        geometry: plan.geometry,
                        focus_domain: plan.focus_domain,
                        focus_leaf: plan.focus_leaf,
                        float_window: None,
                        float_rect: None,
                    });
                }
            }
            return CoreReply::SnapshotInvalid {
                message: OBSERVATION_MESSAGE,
                detail: "seed-failed",
            };
        }
        if session.is_exception(window) {
            let focused = session;
            if committed_session_is_empty(&focused) {
                return CoreReply::Tiled(TiledPlan {
                    base_revision: focused.accepted_revision(),
                    policy_version: LIFECYCLE_POLICY_VERSION,
                    kind: TiledKind::Admit,
                    geometry: Vec::new(),
                    focus_domain: None,
                    focus_leaf: None,
                    float_window: None,
                    float_rect: None,
                });
            }
            let (focus_domain, focus_leaf) = focused.focus();
            let focus = match (focus_domain, focus_leaf) {
                (Some(domain), Some(leaf)) => Some((domain, leaf)),
                _ => None,
            };
            let hints = event
                .windows
                .iter()
                .map(|entry| (entry.window.clone(), entry.hints))
                .collect::<BTreeMap<_, _>>();
            if let Some(plan) = project_retained_tiled_geometry(
                &focused,
                &event.domain_key,
                event.domain.bounds,
                event.domain.gap,
                focus,
                ProjectionKind::Reconcile,
                &hints,
                &event.windows,
            ) {
                if let Some(stored) = self.session_mut(&event.domain_key) {
                    *stored = focused;
                }
                refresh_startup_seed_tree(self, event);
                // A capacity-floated newcomer keeps its native opening
                // frame (no float rect authority) and is the native
                // float target; fixed-size and born-fullscreen
                // anchors keep the float-free shape. The tiled
                // remainder projects even with focus none (a floated
                // anchor never takes tiled focus).
                let float_window = is_capacity.then(|| window.clone());
                return CoreReply::Tiled(TiledPlan {
                    base_revision: plan.base_revision,
                    policy_version: LIFECYCLE_POLICY_VERSION,
                    kind: TiledKind::Admit,
                    geometry: plan.geometry,
                    focus_domain: plan.focus_domain,
                    focus_leaf: plan.focus_leaf,
                    float_window,
                    float_rect: None,
                });
            }
            // An all-floated roster projects no tiled geometry; the
            // committed exceptions still publish as float rows.
            refresh_startup_seed_tree(self, event);
            return CoreReply::Tiled(TiledPlan {
                base_revision: focused.accepted_revision(),
                policy_version: LIFECYCLE_POLICY_VERSION,
                kind: TiledKind::Admit,
                geometry: Vec::new(),
                focus_domain: None,
                focus_leaf: None,
                float_window: is_capacity.then(|| window.clone()),
                float_rect: None,
            });
        }
        CoreReply::SnapshotInvalid {
            message: OBSERVATION_MESSAGE,
            detail: "seed-failed",
        }
    }

    /// Fresh-domain reconcile on an absent domain: seed through the SAME
    /// existing fresh admission machinery (recursive-cut fit fast path,
    /// floating-aware convergence build, deterministic seed order) without
    /// requiring KWin to derive an admit.
    ///
    /// Tiled observations route internally via [`Engine::fresh_admit_shared`]
    /// with the anchor window, event domain/output/workspace, and no
    /// placement bounds; the reply is the complete admission projection at
    /// the committed revision.
    /// All-floating observations build an empty session and run the same
    /// single convergence primitive, then project no geometry. Truly empty
    /// observations retain nothing and project no geometry at revision 0.
    /// No lifecycle observation is fabricated: every path consumes the
    /// validated carried window set.
    ///
    /// Ambiguous same-workspace sources do NOT refuse: the fresh domain
    /// seeds without relocating (like a fresh admit), leaving every
    /// candidate source untouched. Only a genuinely absent domain seeds
    /// here; a unique safe source relocates in
    /// [`Engine::reconcile_request`] before this arm runs.
    fn fresh_reconcile_request(&mut self, event: &CoreEvent) -> CoreReply {
        if event.windows.is_empty() {
            return CoreReply::Projection(ProjectionPlan {
                base_revision: 0,
                kind: ProjectionKind::Reconcile,
                geometry: Vec::new(),
                focus_domain: None,
                focus_leaf: None,
            });
        }
        if let Some(anchor) = Self::fresh_reconcile_seed_anchor(event) {
            return self.fresh_admit_shared(
                event,
                &anchor,
                &event.domain.id,
                &event.domain.workspace,
                None,
                "reconcile",
            );
        }
        // All-floating tail: no anchor exists, so converge the complete
        // observation through the shared fresh floating-aware build. Only a
        // genuinely absent domain seeds here; `Session::new` failure keeps
        // the existing `seed-failed` shape.
        match self.converge_fresh_floating(event, "reconcile") {
            Ok(base) => CoreReply::Projection(ProjectionPlan {
                base_revision: base,
                kind: ProjectionKind::Reconcile,
                geometry: Vec::new(),
                focus_domain: None,
                focus_leaf: None,
            }),
            Err(Some(reply)) => *reply,
            Err(None) => CoreReply::SnapshotInvalid {
                message: OBSERVATION_MESSAGE,
                detail: "seed-failed",
            },
        }
    }

    /// Explicit domain release: drop the exact domain slot and its outer gap
    /// with no native geometry writes. Idempotent when absent; every other
    /// domain and the owner/generation binding are untouched. No convergence,
    /// seeding, relocation, or observation is fabricated: a later ordinary
    /// fresh reconcile re-adopts current geometry through the existing
    /// fit/seed route.
    fn release_request(&mut self, event: &CoreEvent) -> CoreReply {
        self.remove(&event.domain_key);
        CoreReply::Released
    }

    /// Reconcile request phase: retained-tree projection with displaced
    /// workspace relocation and work-area reprojection.
    ///
    /// Fence order matches the legacy protocol handler exactly: unknown
    /// session (fresh domains seed instead of refusing), divergence
    /// (as rejection, never terminal), pending/drag (`pending-exists`),
    /// retained domain binding (`unknown-domain`), inner-gap and outer-gap
    /// binding (`domain-mismatch` with the exact message), membership on the
    /// relocated target-miss path only (`partial-observation` with atomic
    /// rollback; retained targets ride the pre-request convergence),
    /// empty-domain projection (retire a fully empty slot, else reproject on
    /// bounds change, else `malformed-topology`), focus binding
    /// (`focus-mismatch`, except a focus-none convergence whose observed
    /// focus names a converged tile: the D23 no-focus-write restore, which
    /// projects with focus none), pure projection (`malformed-topology` on shape
    /// failure), then work-area reprojection on bounds change. The projection
    /// honors carried client size hints (AR12: satisfiable minimums take
    /// sibling slack, unsatisfiable windows flag `overconstrained`) and
    /// assesses observed rectangles as client clamps (`client_clamped` flags
    /// for accepted clamps, never adopted, shares untouched). Relocation is
    /// atomic: the outer gap is pre-validated against the unique source
    /// before mutating, and any later rejection restores the pre-request
    /// world exactly.
    fn reconcile_request(&mut self, event: &CoreEvent) -> CoreReply {
        let backup = self.clone();
        let mut relocated_here = false;
        if !self.contains(&event.domain_key) {
            let outer_ok = match self.find_unique_source_for_target(&event.domain_key) {
                Some(source) => {
                    if !self.outer_gap_matches(&source, event.outer_gap) {
                        false
                    } else if let Some(session) = self.sessions.get(&source) {
                        let snapshot = session.snapshot();
                        event.windows.iter().any(|entry| {
                            snapshot
                                .windows
                                .iter()
                                .any(|link| link.window == entry.window)
                                || session.is_exception(&entry.window)
                        })
                    } else {
                        false
                    }
                }
                None => false,
            };
            if outer_ok {
                relocated_here =
                    self.try_relocate_for_target(&event.domain_key, &event.domain, event.outer_gap);
            }
        }
        let restore = |engine: &mut Self, backup: &Self, relocated: bool| {
            if relocated {
                *engine = backup.clone();
            }
        };
        let Some(session) = self.session(&event.domain_key).cloned() else {
            restore(self, &backup, relocated_here);
            // Absent domain with no relocation source: seed through the same
            // fresh admission route instead of refusing `unknown-domain`, so
            // KWin never derives an admit. Relocation already ran above; this
            // arm only sees genuinely fresh domains.
            return self.fresh_reconcile_request(event);
        };
        if let Some(reason) = session.divergence() {
            restore(self, &backup, relocated_here);
            return CoreReply::Rejected {
                kind: reason.as_str(),
                message: reason.message(),
            };
        }
        if session.has_pending() || session.has_pending_desired() || session.has_drag() {
            restore(self, &backup, relocated_here);
            return CoreReply::Rejected {
                kind: PENDING_EXISTS_KIND,
                message: PENDING_EXISTS_MESSAGE,
            };
        }
        let Some(retained_domain) = session
            .domains()
            .iter()
            .find(|d| d.key() == event.domain_key)
            .cloned()
        else {
            restore(self, &backup, relocated_here);
            return CoreReply::Rejected {
                kind: RefusalKind::UnknownDomain.as_str(),
                message: RefusalKind::UnknownDomain.message(),
            };
        };
        if retained_domain.gap != event.domain.gap {
            restore(self, &backup, relocated_here);
            return CoreReply::Rejected {
                kind: "domain-mismatch",
                message: "domain gap does not match retained state",
            };
        }
        if self.outer_gap_ref(&event.domain_key).copied() != Some(event.outer_gap) {
            restore(self, &backup, relocated_here);
            return CoreReply::Rejected {
                kind: "domain-mismatch",
                message: "domain outer gap does not match retained state",
            };
        }
        let bounds_changed = retained_domain.bounds != event.domain.bounds;
        let snapshot = session.snapshot();
        // Membership rides the pre-request convergence for retained targets.
        // A relocated target missed convergence (`NoSession` before the move),
        // so only that path keeps its exact-set fence with atomic rollback.
        if relocated_here {
            let mut known: std::collections::BTreeSet<String> = snapshot
                .windows
                .iter()
                .map(|l| l.window.0.clone())
                .collect();
            for entry in session.exception_observed() {
                known.insert(entry.window.0.clone());
            }
            let observed: std::collections::BTreeSet<String> =
                event.windows.iter().map(|w| w.window.0.clone()).collect();
            if observed != known {
                restore(self, &backup, relocated_here);
                return CoreReply::Rejected {
                    kind: RefusalKind::PartialObservation.as_str(),
                    message: RefusalKind::PartialObservation.message(),
                };
            }
        }
        let domain_view = snapshot.domains.into_iter().find(|d| {
            d.output.0 == event.domain_key.output.0 && d.workspace.0 == event.domain_key.workspace.0
        });
        if domain_view.and_then(|d| d.tree).is_none() {
            // An empty domain or one containing only floating exceptions has
            // no tiled geometry to project. A missing tree with tiled links
            // is still malformed.
            if !snapshot.windows.iter().any(|link| {
                link.output == event.domain_key.output
                    && link.workspace == event.domain_key.workspace
            }) {
                if committed_session_is_empty(&session) {
                    // Post-convergence empty: retire the slot at this applied
                    // boundary and report the converged revision. Bounds
                    // changes are moot once retired.
                    let base = session.accepted_revision();
                    self.remove(&event.domain_key);
                    return CoreReply::Projection(ProjectionPlan {
                        base_revision: base,
                        kind: ProjectionKind::Reconcile,
                        geometry: Vec::new(),
                        focus_domain: None,
                        focus_leaf: None,
                    });
                }
                if bounds_changed {
                    self.reproject_retained(&event.domain_key, event.domain.bounds);
                }
                return CoreReply::Projection(ProjectionPlan {
                    base_revision: session.accepted_revision(),
                    kind: ProjectionKind::Reconcile,
                    geometry: Vec::new(),
                    focus_domain: None,
                    focus_leaf: None,
                });
            }
            restore(self, &backup, relocated_here);
            return CoreReply::Rejected {
                kind: RefusalKind::MalformedTopology.as_str(),
                message: RefusalKind::MalformedTopology.message(),
            };
        }
        let (focus_domain, focus_leaf) = session.focus();
        let focus = match (focus_domain, focus_leaf) {
            (Some(focus_domain), Some(focus_leaf)) if focus_domain == event.domain_key => {
                Some((focus_domain, focus_leaf))
            }
            (None, None)
                if !event.focused_window.0.is_empty()
                    && snapshot.windows.iter().any(|link| {
                        link.window == event.focused_window
                            && link.output == event.domain_key.output
                            && link.workspace == event.domain_key.workspace
                    }) =>
            {
                // D23 MNZ-02 no-focus-write restore: a just-restored
                // minimized window never takes focus, so a sole-minimize
                // restore converges to focus none with the old-slot tree
                // back. The observed focus names that restored tile, which
                // is the correct converged state rather than a mismatch,
                // so project it with focus none. Any other focus-none
                // state still refuses below, as does a cross-domain focus.
                None
            }
            (None, None)
                if !event.focused_window.0.is_empty()
                    && Self::observed_float_focus(
                        &session,
                        &event.domain_key,
                        &event.focused_window,
                    ) =>
            {
                // Ordinary-float observed focus (D4): the native active
                // window is a known float homed here (user or capacity
                // alike), so the tiled remainder projects with focus none
                // instead of wedging the domain on `focus-mismatch`. The
                // retained tiled MRU is untouched.
                None
            }
            _ => {
                restore(self, &backup, relocated_here);
                return CoreReply::Rejected {
                    kind: RefusalKind::FocusMismatch.as_str(),
                    message: RefusalKind::FocusMismatch.message(),
                };
            }
        };
        let hints = event
            .windows
            .iter()
            .map(|entry| (entry.window.clone(), entry.hints))
            .collect::<BTreeMap<_, _>>();
        let Some(plan) = project_retained_tiled_geometry(
            &session,
            &event.domain_key,
            event.domain.bounds,
            retained_domain.gap,
            focus,
            ProjectionKind::Reconcile,
            &hints,
            &event.windows,
        ) else {
            // D7 geometric repair on a real capacity decision (bounds
            // changed under a retained tree, or convergence left an
            // infeasible survivor set). Protected or impossible
            // remainders keep the narrow truthful hold.
            if !bounds_changed {
                restore(self, &backup, relocated_here);
                return CoreReply::Rejected {
                    kind: RefusalKind::MalformedTopology.as_str(),
                    message: RefusalKind::MalformedTopology.message(),
                };
            }
            let Some(repaired) = Self::try_repair_domain(
                &session,
                &event.domain_key,
                event.domain.bounds,
                retained_domain.gap,
                event,
            ) else {
                restore(self, &backup, relocated_here);
                return CoreReply::Rejected {
                    kind: RefusalKind::MalformedTopology.as_str(),
                    message: RefusalKind::MalformedTopology.message(),
                };
            };
            let (focus_domain, focus_leaf) = repaired.focus();
            let focus = match (focus_domain, focus_leaf) {
                (Some(focus_domain), Some(focus_leaf)) if focus_domain == event.domain_key => {
                    Some((focus_domain, focus_leaf))
                }
                (None, None) => None,
                _ => {
                    restore(self, &backup, relocated_here);
                    return CoreReply::Rejected {
                        kind: RefusalKind::FocusMismatch.as_str(),
                        message: RefusalKind::FocusMismatch.message(),
                    };
                }
            };
            let Some(plan) = project_retained_tiled_geometry(
                &repaired,
                &event.domain_key,
                event.domain.bounds,
                retained_domain.gap,
                focus,
                ProjectionKind::Reconcile,
                &hints,
                &event.windows,
            ) else {
                restore(self, &backup, relocated_here);
                return CoreReply::Rejected {
                    kind: RefusalKind::MalformedTopology.as_str(),
                    message: RefusalKind::MalformedTopology.message(),
                };
            };
            self.store_committed(event.domain_key.clone(), repaired, event.outer_gap);
            return CoreReply::Projection(plan);
        };
        if bounds_changed {
            self.reproject_retained(&event.domain_key, event.domain.bounds);
        }
        CoreReply::Projection(plan)
    }

    /// Update-gaps request phase: deliberate gap-update reprojection.
    ///
    /// Same fence order as [`Engine::reconcile_request`] minus relocation:
    /// unknown session, divergence (as rejection), pending/drag, domain
    /// binding, empty/exception-only gap adoption (a missing tree with no
    /// tiled links and no tiled observation adopts the gaps and projects
    /// empty; a missing tree alongside tiled links or a tiled observation
    /// stays `malformed-topology`), focus binding, pure projection with the
    /// new inner gap into the new bounds before mutating, then gap adoption
    /// plus store. Membership rides the pre-request convergence, like the
    /// non-relocated reconcile path. A fully empty slot retires instead of
    /// retaining. Never seeds, relocates, resets, or reseeds: an unknown
    /// domain refuses so the fresh reconcile path seeds it.
    fn update_gaps_request(&mut self, event: &CoreEvent) -> CoreReply {
        let Some(session) = self.session(&event.domain_key).cloned() else {
            return CoreReply::Rejected {
                kind: RefusalKind::UnknownDomain.as_str(),
                message: RefusalKind::UnknownDomain.message(),
            };
        };
        if let Some(reason) = session.divergence() {
            return CoreReply::Rejected {
                kind: reason.as_str(),
                message: reason.message(),
            };
        }
        if session.has_pending() || session.has_pending_desired() || session.has_drag() {
            return CoreReply::Rejected {
                kind: PENDING_EXISTS_KIND,
                message: PENDING_EXISTS_MESSAGE,
            };
        }
        if session
            .domains()
            .iter()
            .find(|d| d.key() == event.domain_key)
            .is_none()
        {
            return CoreReply::Rejected {
                kind: RefusalKind::UnknownDomain.as_str(),
                message: RefusalKind::UnknownDomain.message(),
            };
        }
        let snapshot = session.snapshot();
        let domain_view = snapshot.domains.into_iter().find(|d| {
            d.output.0 == event.domain_key.output.0 && d.workspace.0 == event.domain_key.workspace.0
        });
        if domain_view.and_then(|d| d.tree).is_none() {
            // No tiled tree: adopt gaps and project empty when neither the
            // converged session nor the complete observation holds a tiled
            // member for this domain (truly empty or floating exceptions
            // only). A missing tree alongside tiled links or a tiled
            // observation stays malformed.
            let has_tiled_links = snapshot.windows.iter().any(|link| {
                link.output == event.domain_key.output
                    && link.workspace == event.domain_key.workspace
            });
            let has_tiled_observed = event.windows.iter().any(|entry| {
                !entry.floating
                    && entry.output == event.domain.id
                    && entry.workspace == event.domain.workspace
            });
            if !has_tiled_links && !has_tiled_observed {
                if event.windows.is_empty() && committed_session_is_empty(&session) {
                    // Post-convergence fully empty: retire the slot and
                    // report the converged revision; gap adoption is moot.
                    let base = session.accepted_revision();
                    self.remove(&event.domain_key);
                    return CoreReply::Projection(ProjectionPlan {
                        base_revision: base,
                        kind: ProjectionKind::UpdateGaps,
                        geometry: Vec::new(),
                        focus_domain: None,
                        focus_leaf: None,
                    });
                }
                let base = session.accepted_revision();
                if let Some(stored) = self.session_mut(&event.domain_key)
                    && !stored.update_domain_gaps(
                        &event.domain_key,
                        event.domain.bounds,
                        event.domain.gap,
                    )
                {
                    return CoreReply::Rejected {
                        kind: RefusalKind::MalformedTopology.as_str(),
                        message: RefusalKind::MalformedTopology.message(),
                    };
                }
                self.set_outer_gap(event.domain_key.clone(), event.outer_gap);
                return CoreReply::Projection(ProjectionPlan {
                    base_revision: base,
                    kind: ProjectionKind::UpdateGaps,
                    geometry: Vec::new(),
                    focus_domain: None,
                    focus_leaf: None,
                });
            }
            return CoreReply::Rejected {
                kind: RefusalKind::MalformedTopology.as_str(),
                message: RefusalKind::MalformedTopology.message(),
            };
        }
        let (focus_domain, focus_leaf) = session.focus();
        let focus = match (focus_domain, focus_leaf) {
            (Some(focus_domain), Some(focus_leaf)) if focus_domain == event.domain_key => {
                Some((focus_domain, focus_leaf))
            }
            (None, None)
                if !event.focused_window.0.is_empty()
                    && Self::observed_float_focus(
                        &session,
                        &event.domain_key,
                        &event.focused_window,
                    ) =>
            {
                // Ordinary-float observed focus (D4): the native active
                // window is a known float homed here (user or capacity
                // alike), so the update projects with focus none instead
                // of wedging on `focus-mismatch`. Tiled MRU untouched.
                None
            }
            _ => {
                return CoreReply::Rejected {
                    kind: RefusalKind::FocusMismatch.as_str(),
                    message: RefusalKind::FocusMismatch.message(),
                };
            }
        };
        let hints = event
            .windows
            .iter()
            .map(|entry| (entry.window.clone(), entry.hints))
            .collect::<BTreeMap<_, _>>();
        let Some(mut plan) = project_retained_tiled_geometry(
            &session,
            &event.domain_key,
            event.domain.bounds,
            event.domain.gap,
            focus,
            ProjectionKind::UpdateGaps,
            &hints,
            &event.windows,
        ) else {
            // D7 geometric repair (float newest-first victims, or D9
            // overlap retention): a real capacity decision on new
            // bounds/gap, never on settings alone.
            let Some(repaired) = Self::try_repair_domain(
                &session,
                &event.domain_key,
                event.domain.bounds,
                event.domain.gap,
                event,
            ) else {
                return CoreReply::Rejected {
                    kind: RefusalKind::MalformedTopology.as_str(),
                    message: RefusalKind::MalformedTopology.message(),
                };
            };
            let (focus_domain, focus_leaf) = repaired.focus();
            let (Some(focus_domain), Some(focus_leaf)) = (focus_domain, focus_leaf) else {
                // Repair emptied the tiled set without emptying
                // membership (floats remain): project empty, keep slot.
                self.store_committed(event.domain_key.clone(), repaired.clone(), event.outer_gap);
                return CoreReply::Projection(ProjectionPlan {
                    base_revision: repaired.accepted_revision(),
                    kind: ProjectionKind::UpdateGaps,
                    geometry: Vec::new(),
                    focus_domain: None,
                    focus_leaf: None,
                });
            };
            if focus_domain != event.domain_key {
                return CoreReply::Rejected {
                    kind: RefusalKind::FocusMismatch.as_str(),
                    message: RefusalKind::FocusMismatch.message(),
                };
            }
            let Some(plan) = project_retained_tiled_geometry(
                &repaired,
                &event.domain_key,
                event.domain.bounds,
                event.domain.gap,
                Some((focus_domain, focus_leaf)),
                ProjectionKind::UpdateGaps,
                &hints,
                &event.windows,
            ) else {
                return CoreReply::Rejected {
                    kind: RefusalKind::MalformedTopology.as_str(),
                    message: RefusalKind::MalformedTopology.message(),
                };
            };
            self.store_committed(event.domain_key.clone(), repaired, event.outer_gap);
            return CoreReply::Projection(plan);
        };
        let mut session = session;
        if !session.update_domain_gaps(&event.domain_key, event.domain.bounds, event.domain.gap) {
            return CoreReply::Rejected {
                kind: RefusalKind::MalformedTopology.as_str(),
                message: RefusalKind::MalformedTopology.message(),
            };
        }
        plan.base_revision = session.accepted_revision();
        self.store_committed(event.domain_key.clone(), session, event.outer_gap);
        CoreReply::Projection(plan)
    }

    /// Active-group request phase: focus-only retained sync plus read-only
    /// resolution.
    ///
    /// Aligns retained focus from the valid observed snapshot before resolving
    /// the immediate parent group. Focus-only sync: no topology or geometry
    /// mutation. Fails closed on divergence, pending/drag residue,
    /// unknown/exception/cross-domain windows, or domain bounds/gap mismatch
    /// (then the resolver still replies fail-closed `no-group` without
    /// persisting). The carried revision is never a staleness gate: this is a
    /// current-state snapshot whose authoritative `base_revision` is returned
    /// for downstream ordering. Protocol keeps envelope validation, tagged
    /// command decoding, and wire serialization; by the time an event reaches
    /// here the command decoded and the observation validated.
    fn active_group_request(&mut self, event: &CoreEvent) -> CoreReply {
        let Some(mut session) = self.session(&event.domain_key).cloned() else {
            return CoreReply::NoGroup {
                base_revision: None,
                reason: NoGroupReason::NoSession,
            };
        };
        if !event.focused_window.0.is_empty()
            && let Some(retained_domain) = session
                .domains()
                .iter()
                .find(|domain| domain.key() == event.domain_key)
            && retained_domain.bounds == event.domain.bounds
            && retained_domain.gap == event.domain.gap
        {
            let before = session.focus();
            if session.sync_focus_from_window(&event.domain_key, &event.focused_window)
                && session.focus() != before
                && let Some(stored) = self.session_mut(&event.domain_key)
            {
                *stored = session.clone();
            }
        }
        match resolve_active_group(Some(&session), event) {
            ActiveGroupResolution::Found(found) => CoreReply::ActiveGroup(found),
            ActiveGroupResolution::NoGroup {
                base_revision,
                reason,
            } => CoreReply::NoGroup {
                base_revision,
                reason,
            },
        }
    }

    /// Workspace-send request phase: canonical-pair immediate commit.
    ///
    /// Assembles the temporary canonical source/target pair from the retained
    /// per-domain sessions (same-output distinct-workspace accepted by
    /// `Session::paired_from_canonical`), preserving both survivor trees. A
    /// previously absent source seeds once from its complete observation; a
    /// retained source is never reseeded. An empty target uses `None`.
    /// Converges once over BOTH domains with the combined
    /// `workspace_observation_for` (never `event.windows` alone), split/stored
    /// before the command so a refused proposal keeps the converged
    /// observation. Then proposes `MoveToWorkspace` and synchronously commits
    /// its planned topology via `commit_lifecycle`, split/storing both
    /// canonical sessions (source `event.outer_gap`, retained-or-zero target
    /// gap) and returning `SendWorkspace`. No pair survives the call. A
    /// failed commit stores nothing further, so no half pair persists.
    ///
    /// Fence order: target presence/shape, focus binding, canonical assembly
    /// (`canonical-*`), fresh-source seed (`seed-failed`), one combined
    /// convergence, focus sync (`focus-mismatch`), propose (mapped to
    /// `Rejected`), and the `MoveTiled` shape gate (`move-op-invalid`).
    ///
    /// Shared by workspace send (`output_send=false`: `SendToWorkspace` /
    /// `MoveToWorkspace`, same-output distinct-workspace only) and explicit
    /// output send (`output_send=true`: `SendToOutput` / `MoveToOutput`,
    /// the destination output's current workspace, cross-output only). The
    /// canonical two-domain transfer, ordinary admission, and follow/stay
    /// are identical; only the command/intent scope and the reply kind
    /// differ.
    fn transfer_request(&mut self, event: &CoreEvent, output_send: bool) -> CoreReply {
        let (window, target_output, target_workspace, follow) = match &event.command {
            CoreCommand::SendToWorkspace {
                window,
                target_output,
                target_workspace,
                follow,
            } if !output_send => (window, target_output, target_workspace, follow),
            CoreCommand::SendToOutput {
                window,
                target_output,
                target_workspace,
                follow,
            } if output_send => (window, target_output, target_workspace, follow),
            _ => {
                return CoreReply::Rejected {
                    kind: "unknown-value",
                    message: "request contains an unknown value",
                };
            }
        };
        let Some((target_domain, target_key)) = event.target_domain.as_ref() else {
            return CoreReply::Rejected {
                kind: "workspace-target-invalid",
                message: "target workspace domain is missing",
            };
        };
        if target_output != &target_key.output.0 || target_workspace != &target_key.workspace.0 {
            return CoreReply::Rejected {
                kind: "target-mismatch",
                message: "command target does not match the target domain",
            };
        }
        if event.focused_window.0.is_empty() {
            return CoreReply::Rejected {
                kind: "absent-focus",
                message: "no focused window is observed",
            };
        }
        if window != &event.focused_window.0 {
            return CoreReply::Rejected {
                kind: "focus-mismatch",
                message: "the moved window is not the focused window",
            };
        }
        let source_key = event.domain_key.clone();
        let target_key = target_key.clone();
        // First-time source seed from the complete observation when absent.
        // Never reseeds a retained source: presence (even unusable/empty)
        // falls through to canonical assembly below, which fails closed.
        if !self.contains(&source_key) {
            let canonical_source = Self::canonical_component_domain(&event.domain);
            let mut seeded = match Session::new(
                event.owner.clone(),
                event.generation.clone(),
                0,
                event.fingerprint,
                vec![canonical_source],
            ) {
                Ok(session) => session,
                Err(_) => {
                    return CoreReply::SnapshotInvalid {
                        message: OBSERVATION_MESSAGE,
                        detail: "seed-failed",
                    };
                }
            };
            seeded.set_policy(self.policy.clone());
            // First-time seeds join the Engine-wide sequence at the floor.
            self.sync_working_counter(&mut seeded);
            let seed_observation = crate::seed::session_observation_for(
                &event.owner,
                &event.generation,
                seeded.accepted_revision(),
                event.fingerprint,
                &event.windows,
            );
            let seed_focus = if event.focused_window.0.is_empty() {
                None
            } else {
                Some(&event.focused_window)
            };
            match seeded.converge_observation(&seed_observation, seed_focus) {
                Ok(_) => {
                    self.store_committed(source_key.clone(), seeded, event.outer_gap);
                }
                Err(ProposeError::PendingExists) => {
                    return CoreReply::Rejected {
                        kind: PENDING_EXISTS_KIND,
                        message: PENDING_EXISTS_MESSAGE,
                    };
                }
                Err(ProposeError::Diverged(reason)) => {
                    return CoreReply::Diverged(reason);
                }
                Err(_) => {
                    return CoreReply::SnapshotInvalid {
                        message: OBSERVATION_MESSAGE,
                        detail: "seed-failed",
                    };
                }
            }
        }
        // Canonical pair assembly preserving both survivor trees. Empty
        // target observation uses `None`; a retained unusable target fails
        // closed rather than inventing.
        let source_component = Self::canonical_component_domain(&event.domain);
        let target_component = Self::canonical_component_domain(target_domain);
        let source = match self.sessions.get(&source_key).filter(|session| {
            session_usable(session)
                && !committed_session_is_empty(session)
                && session_domain_matches(session, &source_component)
        }) {
            Some(session) => session.clone(),
            None => {
                return CoreReply::SnapshotInvalid {
                    message: OBSERVATION_MESSAGE,
                    detail: if self.contains(&source_key) {
                        "canonical-pair-unusable"
                    } else {
                        "canonical-source-unavailable"
                    },
                };
            }
        };
        let target = if event.target_windows.is_empty() {
            None
        } else {
            match self.sessions.get(&target_key) {
                None => None,
                Some(session)
                    if session_usable(session)
                        && !committed_session_is_empty(session)
                        && session_domain_matches(session, &target_component) =>
                {
                    Some(session.clone())
                }
                Some(_) => {
                    return CoreReply::SnapshotInvalid {
                        message: OBSERVATION_MESSAGE,
                        detail: "canonical-pair-unusable",
                    };
                }
            }
        };
        let mut session = match Session::paired_from_canonical_for_send(
            &source,
            target.as_ref(),
            vec![event.domain.clone(), target_domain.clone()],
        ) {
            Ok(session) => session,
            Err(error) => {
                return CoreReply::SnapshotInvalid {
                    message: OBSERVATION_MESSAGE,
                    detail: match error {
                        CanonicalPairError::MismatchedIdentity => {
                            "canonical-pair-identity-mismatch"
                        }
                        CanonicalPairError::UnusableInput => "canonical-pair-unusable",
                        CanonicalPairError::DomainMismatch => "canonical-pair-domain-mismatch",
                        CanonicalPairError::DuplicateState => "canonical-pair-duplicate-state",
                    },
                };
            }
        };
        // One convergence over BOTH domains with the combined observation.
        // Pair assembly keeps only the component max; advance to the
        // Engine-wide floor before convergence assigns serial-leg
        // sequences. The canonical store below records the result.
        self.sync_working_counter(&mut session);
        let base = session.accepted_revision();
        let combined = crate::seed::workspace_observation_for(
            &event.owner,
            &event.generation,
            base,
            event.fingerprint,
            &event.windows,
            &event.target_windows,
        );
        let focus = if event.focused_window.0.is_empty() {
            None
        } else {
            Some(&event.focused_window)
        };
        let (base, observation) = match session.converge_observation(&combined, focus) {
            Ok(counts) => {
                self.converged_this_op = true;
                if counts.removed + counts.admitted + counts.flags_adopted > 0 {
                    self.last_convergence = Some(EngineConvergenceReport {
                        correlation: event.correlation.clone(),
                        op: if output_send {
                            "send-to-output"
                        } else {
                            "send-to-workspace"
                        },
                        removed: counts.removed,
                        admitted: counts.admitted,
                        flags_adopted: counts.flags_adopted,
                    });
                }
                if !self.store_canonical_pair(
                    source_key.clone(),
                    target_key.clone(),
                    session.clone(),
                    event.outer_gap,
                ) {
                    return CoreReply::SnapshotInvalid {
                        message: OBSERVATION_MESSAGE,
                        detail: "commit-rejected",
                    };
                }
                let base = session.accepted_revision();
                let observation = crate::seed::workspace_observation_for(
                    &event.owner,
                    &event.generation,
                    base,
                    event.fingerprint,
                    &event.windows,
                    &event.target_windows,
                );
                (base, observation)
            }
            Err(ProposeError::PendingExists) => {
                return CoreReply::Rejected {
                    kind: PENDING_EXISTS_KIND,
                    message: PENDING_EXISTS_MESSAGE,
                };
            }
            Err(ProposeError::Diverged(reason)) => return CoreReply::Diverged(reason),
            Err(error) => {
                return CoreReply::Rejected {
                    kind: error.kind(),
                    message: error.message(),
                };
            }
        };
        if !session.sync_focus_from_window(&source_key, &event.focused_window) {
            // D28/D16 slotless carry: an explicit focused mover that is a
            // floating exception transfers without a tiled link. Bookkeeping focus falls
            // back to a tiled MRU (never the mover); native follow/stay
            // runs adapter-side while the exception rehomes through the
            // deferred transfer below. Anything else still refuses as
            // `FocusMismatch`.
            let mover = WindowId(window.clone());
            if window != &event.focused_window.0
                || !session.deferred_send_eligible(&mover, &source_key, &observation.windows)
            {
                let kind = RefusalKind::FocusMismatch;
                return CoreReply::Rejected {
                    kind: kind.as_str(),
                    message: kind.message(),
                };
            }
        }
        let session_command = if output_send {
            SessionCommand::MoveToOutput {
                window: crate::directional::WindowId(window.clone()),
                target_output: target_key.output.clone(),
                target_workspace: target_key.workspace.clone(),
                follow: *follow,
            }
        } else {
            SessionCommand::MoveToWorkspace {
                window: crate::directional::WindowId(window.clone()),
                target_output: target_key.output.clone(),
                target_workspace: target_key.workspace.clone(),
                follow: *follow,
            }
        };
        // Trustworthy native live frame for same-output sends only:
        // the carried mover rectangle is the exact current native frame
        // (allocation can differ under B6/clamped hints). Cross-output
        // sends carry none: source coordinates are never staged as a
        // target claim; the established native send remap plus its
        // arrival proof owns placement there.
        let mover_live_frame = if output_send {
            None
        } else {
            event
                .windows
                .iter()
                .find(|entry| entry.window.0 == *window)
                .map(|entry| entry.rect)
        };
        match session.propose_send_with_live_frame(
            &session_command,
            &observation,
            &event.correlation,
            &LifecycleCapabilities::full(),
            mover_live_frame,
        ) {
            Ok(plan) => {
                if let Some(trace) = session.last_send_placement().cloned() {
                    self.last_send_placement = Some(EngineSendPlacementTrace {
                        correlation: event.correlation.clone(),
                        anchor_kind: trace.anchor_kind,
                        anchor: trace.anchor,
                        axis: trace.axis,
                        projected: trace.projected,
                        target_leaves: trace.target_leaves,
                    });
                }
                let Some(typed) = crate::boundary::SendWorkspacePlan::from_session(&plan) else {
                    return CoreReply::SnapshotInvalid {
                        message: OBSERVATION_MESSAGE,
                        detail: "move-op-invalid",
                    };
                };
                if Self::commit_lifecycle(&mut session, &plan, event, base)
                    && self.store_canonical_pair(source_key, target_key, session, event.outer_gap)
                {
                    if output_send {
                        return CoreReply::SendOutput(typed);
                    }
                    return CoreReply::SendWorkspace(typed);
                }
                CoreReply::SnapshotInvalid {
                    message: OBSERVATION_MESSAGE,
                    detail: "commit-rejected",
                }
            }
            Err(error) => CoreReply::Rejected {
                kind: error.kind(),
                message: error.message(),
            },
        }
    }

    /// Explicit whole-workspace migration (R-WS-12) through
    /// [`Session::relocate_domain`]: same workspace id, different output.
    /// Sticky members (carried `sticky`) stay homed on the source in a
    /// residual session; workspace floats ride with the domain. The active
    /// client may be tiled, float, sticky, or absent: reply focus names a
    /// migrating tiled leaf only, while `active_window` names a migrated
    /// member only (null for absent or stayed-sticky actives, follow-only).
    /// Currency runs on clones via the standard
    /// convergence primitive, so refusals never mutate live state; commit
    /// stores the relocated domain (plus the sticky residual when nonempty).
    /// Reply `planned` means retained rekey only; native transfer stays
    /// adapter-gated on `adapter-must-verify-postconditions`.
    fn migrate_workspace_request(&mut self, event: &CoreEvent) -> CoreReply {
        let CoreCommand::MigrateWorkspace { direction } = &event.command else {
            return CoreReply::Rejected {
                kind: "unknown-value",
                message: "request contains an unknown value",
            };
        };
        let Some(direction_parsed) = parse_engine_direction(direction) else {
            return CoreReply::Rejected {
                kind: DIRECTION_KIND,
                message: DIRECTION_MESSAGE,
            };
        };
        let Some((target_domain, target_key)) = event.target_domain.as_ref() else {
            return CoreReply::Rejected {
                kind: "workspace-target-invalid",
                message: "target workspace domain is missing",
            };
        };
        let source_key = event.domain_key.clone();
        let target_key = target_key.clone();
        if target_key.workspace != source_key.workspace || target_key.output == source_key.output {
            return CoreReply::Rejected {
                kind: "cross-domain-mismatch",
                message: "command target does not match the target domain",
            };
        }
        // D8: fullscreen/maximized members carry via native output remap
        // (native re-fits overlays; no extra geometry/focus writes while
        // fullscreen). Only an unexplained fit-excluded flag without
        // floating/sticky/fullscreen/maximized origin still refuses.
        // Floats ride; sticky stays natively.
        for entry in event.windows.iter().chain(event.target_windows.iter()) {
            if entry.fit_excluded
                && !entry.floating
                && !entry.sticky
                && !entry.fullscreen
                && !entry.maximized
            {
                return CoreReply::Rejected {
                    kind: "overlay-present",
                    message: "workspace contains an unexplained fit-excluded member",
                };
            }
        }
        // The migration target carries no observation: any carried target
        // window, or any retained target session, is SAME-workspace residue.
        // A destination showing a DIFFERENT workspace stays KDE-native.
        if !event.target_windows.is_empty() || self.sessions.contains_key(&target_key) {
            return CoreReply::Rejected {
                kind: RefusalKind::Unchanged.as_str(),
                message: RefusalKind::Unchanged.message(),
            };
        }
        if !is_gap(event.outer_gap) {
            return CoreReply::Rejected {
                kind: "domain-mismatch",
                message: "domain outer gap does not match retained state",
            };
        }
        let empty_plan = |base: u64, active: Option<WindowId>| {
            CoreReply::MigrateWorkspace(crate::boundary::MigrateWorkspacePlan {
                base_revision: base,
                direction: direction_parsed,
                source: source_key.clone(),
                target: target_key.clone(),
                geometry: Vec::new(),
                focus_domain: None,
                focus_leaf: None,
                active_window: active,
                members: 0,
                floats: 0,
                preconditions: vec![
                    LifecyclePrecondition::WindowObserved,
                    LifecyclePrecondition::DesiredTopologyValid,
                    LifecyclePrecondition::AdapterMustVerifyPostconditions,
                ],
            })
        };
        // Empty migration moves no slot. Absent sources need empty focus;
        // a retained empty session migrates the same way.
        let Some(session) = self.sessions.get(&source_key).cloned() else {
            if event.windows.is_empty() && event.focused_window.0.is_empty() {
                self.last_migration = Some(EngineMigrationReport {
                    correlation: event.correlation.clone(),
                    direction: direction_token(direction_parsed),
                    members: 0,
                    floats: 0,
                    empty: true,
                });
                return empty_plan(event.revision, None);
            }
            return CoreReply::Rejected {
                kind: RefusalKind::UnknownDomain.as_str(),
                message: RefusalKind::UnknownDomain.message(),
            };
        };
        if session.owner() != &event.owner {
            return CoreReply::Diverged(DivergenceKind::OwnerMismatch);
        }
        if session.generation() != &event.generation {
            return CoreReply::Diverged(DivergenceKind::GenerationMismatch);
        }
        if let Some(reason) = session.divergence() {
            return CoreReply::Diverged(reason);
        }
        if !session_usable(&session) || session.has_pending_desired() || session.has_drag() {
            return CoreReply::Rejected {
                kind: PENDING_EXISTS_KIND,
                message: PENDING_EXISTS_MESSAGE,
            };
        }
        if !session_domain_matches(&session, &event.domain)
            || self.outer_gap_ref(&source_key).copied() != Some(event.outer_gap)
        {
            return CoreReply::Rejected {
                kind: "domain-mismatch",
                message: "domain outer gap does not match retained state",
            };
        }
        // The wire revision is vestigial across the whole protocol
        // (adapters hardcode 0; every op derives observation bases from the
        // accepted revision internally, as the clone currency below does).
        // Staleness is fenced by membership, not by the revision token.
        // Frozen identity: every retained member must be carried, so currency
        // below can only adopt flags/admit newcomers, never erase unseen
        // (sticky) origin.
        for entry in &event.windows {
            if entry.output != source_key.output || entry.workspace != source_key.workspace {
                return CoreReply::Rejected {
                    kind: RefusalKind::CrossDomainMismatch.as_str(),
                    message: RefusalKind::CrossDomainMismatch.message(),
                };
            }
        }
        let mut retained_ids: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
        let retained_snapshot = session.snapshot();
        for link in retained_snapshot.windows.iter() {
            retained_ids.insert(link.window.0.as_str());
        }
        let retained_exceptions = session.exception_observed();
        for observed in retained_exceptions.iter() {
            retained_ids.insert(observed.window.0.as_str());
        }
        if !retained_ids.iter().all(|id| {
            event
                .windows
                .iter()
                .any(|entry| &entry.window.0.as_str() == id)
        }) {
            return CoreReply::Rejected {
                kind: RefusalKind::PartialObservation.as_str(),
                message: RefusalKind::PartialObservation.message(),
            };
        }
        // Dangling active client: a named focus must be carried. Empty focus
        // migrates without fabricated focus.
        if !event.focused_window.0.is_empty()
            && !event
                .windows
                .iter()
                .any(|entry| entry.window == event.focused_window)
        {
            return CoreReply::Rejected {
                kind: RefusalKind::FocusMismatch.as_str(),
                message: RefusalKind::FocusMismatch.message(),
            };
        }
        // Retained-empty sources migrate empty only when nothing is carried;
        // carried newcomers fall through to the normal adoption flow below.
        if committed_session_is_empty(&session) && event.windows.is_empty() {
            let base = session.accepted_revision();
            self.sessions.remove(&source_key);
            self.outer_gaps.remove(&source_key);
            self.last_migration = Some(EngineMigrationReport {
                correlation: event.correlation.clone(),
                direction: direction_token(direction_parsed),
                members: 0,
                floats: 0,
                empty: true,
            });
            return empty_plan(base, None);
        }
        // Partition on clones: migrating members move, carried-sticky stays.
        // Subset convergence removes the other side through the standard
        // primitive; any failure refuses with live state untouched.
        let sticky_ids: std::collections::BTreeSet<&str> = event
            .windows
            .iter()
            .filter(|entry| entry.sticky)
            .map(|entry| entry.window.0.as_str())
            .collect();
        let active = (!event.focused_window.0.is_empty()).then(|| event.focused_window.clone());
        let mut moved = session.clone();
        let migrating: std::collections::BTreeSet<&str> = event
            .windows
            .iter()
            .filter(|entry| !entry.sticky)
            .map(|entry| entry.window.0.as_str())
            .collect();
        let moved_focus = match &active {
            Some(window) if migrating.contains(window.0.as_str()) => Some(window),
            _ => None,
        };
        let moved_obs = SessionObservation {
            observation: crate::contract::Observation::new(
                event.owner.clone(),
                event.generation.clone(),
                moved.accepted_revision(),
                event.fingerprint,
            ),
            windows: event
                .windows
                .iter()
                .filter(|entry| migrating.contains(entry.window.0.as_str()))
                .map(crate::seed::observed_window_from_engine)
                .collect(),
        };
        // Partitioned clones converge carried newcomers; advance both to
        // the Engine-wide floor first so adopted sequences order after
        // every retained domain instead of replaying its counter.
        self.sync_working_counter(&mut moved);
        if let Err(error) = moved.converge_observation(&moved_obs, moved_focus) {
            return match error {
                ProposeError::Diverged(reason) => CoreReply::Diverged(reason),
                ProposeError::PendingExists => CoreReply::Rejected {
                    kind: PENDING_EXISTS_KIND,
                    message: PENDING_EXISTS_MESSAGE,
                },
                ProposeError::Refused(kind) => CoreReply::Rejected {
                    kind: kind.as_str(),
                    message: kind.message(),
                },
            };
        }
        let stay = if sticky_ids.is_empty() {
            None
        } else {
            let mut stay = session.clone();
            let stay_focus = match &active {
                Some(window) if sticky_ids.contains(window.0.as_str()) => Some(window),
                _ => None,
            };
            let stay_obs = SessionObservation {
                observation: crate::contract::Observation::new(
                    event.owner.clone(),
                    event.generation.clone(),
                    stay.accepted_revision(),
                    event.fingerprint,
                ),
                windows: event
                    .windows
                    .iter()
                    .filter(|entry| sticky_ids.contains(entry.window.0.as_str()))
                    .map(crate::seed::observed_window_from_engine)
                    .collect(),
            };
            self.sync_working_counter(&mut stay);
            if let Err(error) = stay.converge_observation(&stay_obs, stay_focus) {
                return match error {
                    ProposeError::Diverged(reason) => CoreReply::Diverged(reason),
                    ProposeError::PendingExists => CoreReply::Rejected {
                        kind: PENDING_EXISTS_KIND,
                        message: PENDING_EXISTS_MESSAGE,
                    },
                    ProposeError::Refused(kind) => CoreReply::Rejected {
                        kind: kind.as_str(),
                        message: kind.message(),
                    },
                };
            }
            Some(stay)
        };
        if !moved.relocate_domain(
            &source_key,
            &target_key,
            target_domain.bounds,
            target_domain.gap,
        ) {
            return CoreReply::Rejected {
                kind: RefusalKind::MalformedTopology.as_str(),
                message: RefusalKind::MalformedTopology.message(),
            };
        }
        // Follow-only echo: reply focus names a migrating tiled leaf only,
        // and `active_window` names a migrated member only. A stayed sticky
        // active echoes null like an absent client: KDE still switches to the
        // migrated workspace after verified arrival and never activates a
        // source window on the target. Retained bookkeeping is untouched.
        let moved_snapshot = moved.snapshot();
        let active_leaf = match &active {
            Some(window) => moved_snapshot
                .windows
                .iter()
                .find(|link| &link.window == window)
                .map(|link| link.leaf.clone()),
            None => None,
        };
        let (focus_domain, focus_leaf) = match active_leaf {
            Some(leaf) => (Some(target_key.clone()), Some(leaf)),
            None => (None, None),
        };
        let mut hints: std::collections::BTreeMap<WindowId, crate::size_hints::WindowSizeHints> =
            std::collections::BTreeMap::new();
        for entry in &event.windows {
            hints.insert(entry.window.clone(), entry.hints);
        }
        let geometry = if moved_snapshot.windows.is_empty() {
            Vec::new()
        } else {
            let Some(plan) = crate::boundary::project_retained_tiled_geometry(
                &moved,
                &target_key,
                target_domain.bounds,
                target_domain.gap,
                match (&focus_domain, &focus_leaf) {
                    (Some(domain), Some(leaf)) => Some((domain.clone(), leaf.clone())),
                    _ => None,
                },
                crate::boundary::ProjectionKind::Reconcile,
                &hints,
                &event.windows,
            ) else {
                return CoreReply::Rejected {
                    kind: RefusalKind::MalformedTopology.as_str(),
                    message: RefusalKind::MalformedTopology.message(),
                };
            };
            plan.geometry
        };
        let members = moved_snapshot.windows.len() + moved.exception_count();
        let floats = moved.exception_count();
        let revision = moved.accepted_revision();
        let reply_active = match &active {
            Some(window)
                if moved_snapshot
                    .windows
                    .iter()
                    .any(|link| &link.window == window)
                    || moved.is_exception(window) =>
            {
                Some(window.clone())
            }
            _ => None,
        };
        // Commit: relocated domain takes the target slot; the sticky residual
        // keeps the source slot (and its outer gap) only while nonempty.
        // Both clones converged above, so record their counters first.
        self.note_working_counter(&moved);
        if let Some(stay) = stay.as_ref() {
            self.note_working_counter(stay);
        }
        let stay = stay.filter(|stay| !committed_session_is_empty(stay));
        let moved_empty = committed_session_is_empty(&moved);
        self.sessions.remove(&source_key);
        if !moved_empty {
            self.outer_gaps.insert(target_key.clone(), event.outer_gap);
            self.sessions.insert(target_key.clone(), moved);
        }
        if let Some(stay) = stay {
            self.sessions.insert(source_key.clone(), stay);
        } else {
            self.outer_gaps.remove(&source_key);
        }
        self.last_migration = Some(EngineMigrationReport {
            correlation: event.correlation.clone(),
            direction: direction_token(direction_parsed),
            members,
            floats,
            empty: moved_empty,
        });
        if moved_empty {
            return empty_plan(revision, reply_active);
        }
        CoreReply::MigrateWorkspace(crate::boundary::MigrateWorkspacePlan {
            base_revision: revision,
            direction: direction_parsed,
            source: source_key,
            target: target_key,
            geometry,
            focus_domain,
            focus_leaf,
            active_window: reply_active,
            members,
            floats,
            preconditions: vec![
                LifecyclePrecondition::WindowObserved,
                LifecyclePrecondition::DesiredTopologyValid,
                LifecyclePrecondition::AdapterMustVerifyPostconditions,
            ],
        })
    }

    /// Shared retained propose/commit: try the usable retained session, then
    /// rebuild once from `seed_order`. `ambiguous_as_snapshot` selects the
    /// fail-closed kind when no safe order exists (toggle-float uses
    /// `snapshot-invalid`). Mirrors the legacy protocol `run_retained` over the
    /// typed [`CoreEvent`]: target presence gates relocation, usable sessions
    /// propose directly, rebuilds fall back to relocation then seeding, and
    /// every outcome maps to the identical typed [`CoreReply`].
    ///
    /// Once [`Engine::handle`] converged the domain for this op
    /// (`converged_this_op`, changed or exact), partial/diverged mismatches
    /// fail closed without reset/reseed: no discard, no relocation retry, no
    /// seeding. First-time (no convergence) keeps the exact legacy rebuild.
    fn run_retained<R>(
        &mut self,
        event: &CoreEvent,
        seed_order: Option<Vec<EngineWindow>>,
        ambiguous_as_snapshot: bool,
        propose: impl Fn(&mut Session, &SessionObservation) -> Result<R, ProposeError>,
        reply: impl Fn(&R) -> CoreReply,
        commit: impl Fn(&mut Session, &R, &CoreEvent, u64) -> bool,
    ) -> CoreReply {
        let converged = self.converged_this_op;
        let target_existed = self.contains(&event.domain_key);
        // Do not let the mutating take discard a just-converged mismatched
        // domain; the existing converged refusal below keeps its topology.
        let candidate = if converged
            && target_existed
            && self.sessions.get(&event.domain_key).is_some_and(|session| {
                !session_usable(session)
                    || !session_domain_matches(session, &event.domain)
                    || committed_session_is_empty(session)
            }) {
            None
        } else {
            self.take_usable_session(&event.domain_key, &event.domain)
        };
        if let Some(mut session) = candidate {
            let base = session.accepted_revision();
            let observation = crate::seed::session_observation_for(
                &event.owner,
                &event.generation,
                base,
                event.fingerprint,
                &event.windows,
            );
            match propose(&mut session, &observation) {
                Ok(plan) => {
                    let typed = reply(&plan);
                    if commit(&mut session, &plan, event, base) {
                        self.store_committed(event.domain_key.clone(), session, event.outer_gap);
                        return typed;
                    }
                    self.remove(&event.domain_key);
                    return CoreReply::SnapshotInvalid {
                        message: OBSERVATION_MESSAGE,
                        detail: "commit-rejected",
                    };
                }
                Err(error) if engine_needs_rebuild(&error) => {
                    if converged {
                        return CoreReply::Rejected {
                            kind: error.kind(),
                            message: error.message(),
                        };
                    }
                    self.remove(&event.domain_key);
                }
                Err(error) => {
                    return CoreReply::Rejected {
                        kind: error.kind(),
                        message: error.message(),
                    };
                }
            }
        } else if !target_existed {
            let backup = self.clone();
            if self.try_relocate_for_target(&event.domain_key, &event.domain, event.outer_gap) {
                if let Some(mut session) =
                    self.take_usable_session(&event.domain_key, &event.domain)
                {
                    let base = session.accepted_revision();
                    let observation = crate::seed::session_observation_for(
                        &event.owner,
                        &event.generation,
                        base,
                        event.fingerprint,
                        &event.windows,
                    );
                    match propose(&mut session, &observation) {
                        Ok(plan) => {
                            let typed = reply(&plan);
                            if commit(&mut session, &plan, event, base) {
                                self.store_committed(
                                    event.domain_key.clone(),
                                    session,
                                    event.outer_gap,
                                );
                                return typed;
                            }
                            *self = backup;
                            return CoreReply::SnapshotInvalid {
                                message: OBSERVATION_MESSAGE,
                                detail: "commit-rejected",
                            };
                        }
                        Err(error) if engine_needs_rebuild(&error) => {
                            *self = backup;
                        }
                        Err(error) => {
                            *self = backup;
                            return CoreReply::Rejected {
                                kind: error.kind(),
                                message: error.message(),
                            };
                        }
                    }
                } else {
                    *self = backup;
                }
            }
        }
        // Once converged, never reseed: fail closed without reset. Ambiguous
        // seed order keeps its exact legacy shape (no seeding either way);
        // a concrete order refuses instead of rebuilding survivors.
        if converged {
            let Some(_order) = seed_order else {
                if ambiguous_as_snapshot {
                    return CoreReply::SnapshotInvalid {
                        message: OBSERVATION_MESSAGE,
                        detail: "missing-seed-order",
                    };
                }
                return CoreReply::Rejected {
                    kind: AMBIGUOUS_KIND,
                    message: AMBIGUOUS_MESSAGE,
                };
            };
            return CoreReply::Rejected {
                kind: RefusalKind::PartialObservation.as_str(),
                message: RefusalKind::PartialObservation.message(),
            };
        }
        let Some(order) = seed_order else {
            if ambiguous_as_snapshot {
                return CoreReply::SnapshotInvalid {
                    message: OBSERVATION_MESSAGE,
                    detail: "missing-seed-order",
                };
            }
            return CoreReply::Rejected {
                kind: AMBIGUOUS_KIND,
                message: AMBIGUOUS_MESSAGE,
            };
        };
        let floor = self.admission_seq_floor();
        let Some(mut session) = crate::seed::seed_session_from_counter(
            &event.owner,
            &event.generation,
            event.fingerprint,
            &event.domain,
            &order,
            floor,
        ) else {
            return CoreReply::SnapshotInvalid {
                message: OBSERVATION_MESSAGE,
                detail: "seed-failed",
            };
        };
        session.set_policy(self.policy.clone());
        let base = session.accepted_revision();
        let observation = crate::seed::session_observation_for(
            &event.owner,
            &event.generation,
            base,
            event.fingerprint,
            &event.windows,
        );
        match propose(&mut session, &observation) {
            Ok(plan) => {
                let typed = reply(&plan);
                if commit(&mut session, &plan, event, base) {
                    self.store_committed(event.domain_key.clone(), session, event.outer_gap);
                    return typed;
                }
                CoreReply::SnapshotInvalid {
                    message: OBSERVATION_MESSAGE,
                    detail: "commit-rejected",
                }
            }
            Err(error) => CoreReply::Rejected {
                kind: error.kind(),
                message: error.message(),
            },
        }
    }

    /// Synchronous acknowledge plus `verify_lifecycle` commit for one
    /// retained lifecycle plan. Mirrors the legacy protocol commit closure
    /// exactly.
    fn commit_lifecycle(
        session: &mut Session,
        plan: &crate::session::SessionPlan,
        event: &CoreEvent,
        base: u64,
    ) -> bool {
        if !engine_acknowledge(session, event, base) {
            return false;
        }
        let post = LifecyclePostObservation::new(
            Observation::new(
                event.owner.clone(),
                event.generation.clone(),
                base,
                event.fingerprint,
            ),
            event.correlation.clone(),
            true,
            plan.dispatch.preconditions.clone(),
            plan.dispatch.operation.clone(),
        );
        session.verify_lifecycle(&post).is_ok()
    }

    /// Toggle-float request phase through the shared retained lifecycle.
    ///
    /// Protocol keeps the raw-window floating `not-tiled` probe, tagged
    /// decoding, opaque window checks, carried-bounds validation, and
    /// partial-observation at their exact positions; this owns seed ordering,
    /// seeding, relocation, propose/commit, and store, including the
    /// pending-float effective rectangle.
    ///
    /// Fresh domains whose complete observation contains floating members
    /// cannot seed (the seed rebuild is tiled-only): like the fresh reconcile
    /// path,
    /// an empty session plus the same single convergence primitive retains
    /// the floating exceptions first, then the ordinary toggle below unfloats
    /// into the current domain. No staged or fabricated observations.
    /// Relocation candidates and all-tiled fresh observations keep the legacy
    /// route byte-for-byte.
    fn toggle_float_request(&mut self, event: &CoreEvent) -> CoreReply {
        use crate::boundary::TiledPlan;
        let CoreCommand::ToggleFloat { window, float_rect } = &event.command else {
            return CoreReply::Rejected {
                kind: "unknown-value",
                message: "request contains an unknown value",
            };
        };
        if self.session(&event.domain_key).is_none()
            && event.windows.iter().any(|w| w.floating)
            && self
                .find_unique_source_for_target(&event.domain_key)
                .is_none()
        {
            // Shared floating-aware convergence; `Session::new` failure
            // (`Err(None)`) falls through to the legacy route with no flags
            // set, exactly like the previous `let Ok(...)` gate.
            if let Err(Some(reply)) = self.converge_fresh_floating(event, "toggle-float") {
                return *reply;
            }
            if self.session(&event.domain_key).is_none() {
                // Converged empty retires the slot: behave as if never
                // converged so the legacy route keeps its exact shape.
                self.converged_this_op = false;
                self.last_convergence = None;
            }
        }
        let seed_order = crate::seed::order_spatial_with_focus_last(
            event.windows.clone(),
            &event.focused_window,
            false,
        );
        let window = WindowId(window.clone());
        let float_rect = *float_rect;
        self.run_retained(
            event,
            seed_order,
            true,
            |session, observation| {
                session
                    .propose(
                        &SessionCommand::ToggleFloat {
                            window: window.clone(),
                            float_geometry: float_rect,
                        },
                        observation,
                        &event.correlation,
                        &LifecycleCapabilities::full(),
                    )
                    .map(|plan| {
                        let effective = session.pending_float_geometry(&window);
                        // Explicit unfloat retries once: a capacity
                        // re-float keeps the window floated with no false
                        // tile override. The newcomer names the native
                        // float target (no frame authority); the adapter
                        // owns native focus in the route integration.
                        let no_space = session.pending_stages_capacity_float(&window);
                        (plan, effective, no_space)
                    })
            },
            |result| {
                let mut tiled = CoreReply::Tiled(TiledPlan::for_toggle_float(&result.0, result.1));
                if result.2
                    && let CoreReply::Tiled(plan) = &mut tiled
                {
                    plan.float_window = Some(window.clone());
                }
                tiled
            },
            |session, result, e, base| Self::commit_lifecycle(session, &result.0, e, base),
        )
    }

    /// Toggle-orientation request phase through the shared retained lifecycle.
    ///
    /// Same ownership contract as [`Engine::local_move_request`]: protocol
    /// keeps tagged command decoding and window authorization; this owns seed
    /// ordering, focus sync, propose (focused leaf's immediate parent axis
    /// flip), commit, and store. Ordinary single-domain handling only: no
    /// directional pair route. The typed [`TiledKind::ToggleOrientation`]
    /// reply funnels through the exact planned wire shape.
    fn toggle_orientation_request(&mut self, event: &CoreEvent) -> CoreReply {
        use crate::boundary::{TiledKind, TiledPlan};
        let CoreCommand::ToggleOrientation { window } = &event.command else {
            return CoreReply::Rejected {
                kind: "unknown-value",
                message: "request contains an unknown value",
            };
        };
        if !is_opaque_id(window) {
            return CoreReply::SnapshotInvalid {
                message: OPAQUE_ID_MESSAGE,
                detail: "toggle-orient-window-invalid",
            };
        }
        let seed_order = crate::seed::order_spatial_with_focus_last(
            event.windows.clone(),
            &event.focused_window,
            false,
        );
        let window = WindowId(window.clone());
        self.run_retained(
            event,
            seed_order,
            true,
            |session, observation| {
                let _ = session.sync_focus_from_window(&event.domain_key, &event.focused_window);
                session.propose(
                    &SessionCommand::ToggleOrientation {
                        window: window.clone(),
                    },
                    observation,
                    &event.correlation,
                    &LifecycleCapabilities::full(),
                )
            },
            |plan| {
                CoreReply::Tiled(TiledPlan::from_lifecycle(
                    TiledKind::ToggleOrientation,
                    plan,
                ))
            },
            Self::commit_lifecycle,
        )
    }

    /// Directional move request phase: two-domain pair planning with R4
    /// staging, plus the legacy single-domain local path. Protocol keeps
    /// envelope validation, tagged command decoding, pair scope shape
    /// validation, mover binding, parsed-direction/op validation, and wire
    /// serialization at their exact positions; by the time an event reaches
    /// here the command decoded and the pair shape-checked. This owns the
    /// planning outcome and the one-shot transition: canonical pair assembly,
    /// focus sync, capability-gated propose, R4 pending staging, and the
    /// R1-R3 synchronous acknowledge/verify/split/store. Single-domain
    /// observations (no two-entry pair) run the local retained
    /// propose/commit through [`Engine::local_move_request`], preserving the
    /// legacy `run_retained` order exactly.
    ///
    /// Fence order matches the legacy protocol handler exactly: window opaque
    /// (`move-window-invalid`), parsed direction (`direction-invalid`), pair
    /// presence (`domain-invalid`), scoped pending pre-fence across either
    /// pair domain, canonical assembly (`canonical-*`), one convergence
    /// primitive on the assembled world (split/stored before the command),
    /// propose (mapped to `Rejected` with the exact kind/message, including
    /// divergences as rejections like the legacy `propose_failure`), then
    /// the R4 immediate commit or the R1-R3 sync commit (`commit-rejected`
    /// on failure).
    fn directional_move_request(&mut self, event: &CoreEvent) -> CoreReply {
        use crate::boundary::MovePlanReply;
        let CoreCommand::Move {
            window,
            direction,
            cross_output_transfer,
            same_axis_move,
        } = &event.command
        else {
            return CoreReply::Rejected {
                kind: "unknown-value",
                message: "request contains an unknown value",
            };
        };
        if event
            .directional
            .as_ref()
            .is_none_or(|pair| pair.len() != 2)
        {
            return self.local_move_request(event, window, direction, *same_axis_move);
        }
        if !is_opaque_id(window) {
            return CoreReply::SnapshotInvalid {
                message: OPAQUE_ID_MESSAGE,
                detail: "move-window-invalid",
            };
        }
        let Some(direction) = parse_engine_direction(direction) else {
            return CoreReply::Rejected {
                kind: DIRECTION_KIND,
                message: DIRECTION_MESSAGE,
            };
        };
        let Some(pair) = event.directional.as_ref().filter(|pair| pair.len() == 2) else {
            return CoreReply::SnapshotInvalid {
                message: OBSERVATION_MESSAGE,
                detail: "domain-invalid",
            };
        };
        let (source_domain, source_key) = &pair[0];
        let (target_domain, target_key) = &pair[1];
        let window = WindowId(window.clone());
        let mut session = match self.assemble_directional_pair(
            source_domain,
            source_key,
            target_domain,
            target_key,
        ) {
            Ok(session) => session,
            Err(detail) => {
                return CoreReply::SnapshotInvalid {
                    message: OBSERVATION_MESSAGE,
                    detail,
                };
            }
        };
        // One primitive on the assembled BOTH-domain world (never two
        // per-domain calls); the converged split persists below even if the
        // ordinary move refuses.
        let (base, observation) =
            match self.converge_assembled_pair(&mut session, event, "move", source_key, target_key)
            {
                Ok(next) => next,
                Err(reply) => return *reply,
            };
        let _ = session.sync_focus_from_window(source_key, &event.focused_window.clone());
        let mut capabilities = Capabilities::full();
        capabilities.cross_output_transfer = *cross_output_transfer;
        match session.propose_move_with_same_axis(
            source_key,
            &window,
            direction,
            &observation,
            &event.correlation,
            &capabilities,
            *same_axis_move,
        ) {
            Ok(crate::session::SessionMoveOutcome::Moved(plan)) => {
                if matches!(plan.dispatch.operation, MoveOperation::CrossOutput { .. }) {
                    let target_outer_gap = event
                        .directional_target_outer_gap
                        .filter(|gap| is_gap(*gap))
                        .unwrap_or(0);
                    let Some(typed) =
                        MovePlanReply::from_cross(&plan, &source_key.output, &source_key.workspace)
                    else {
                        return CoreReply::SnapshotInvalid {
                            message: OBSERVATION_MESSAGE,
                            detail: "domain-invalid",
                        };
                    };
                    if Self::commit_directional_move(&mut session, event, &plan, base) {
                        let Ok((source, target)) = session.split_canonical_pair() else {
                            return CoreReply::SnapshotInvalid {
                                message: OBSERVATION_MESSAGE,
                                detail: "commit-rejected",
                            };
                        };
                        self.store_committed(source_key.clone(), source, event.outer_gap);
                        if let Some(target) = target {
                            self.store_committed(target_key.clone(), target, target_outer_gap);
                        } else {
                            self.sessions.remove(target_key);
                            self.outer_gaps.remove(target_key);
                        }
                        return CoreReply::MoveDirectional(typed);
                    }
                    CoreReply::SnapshotInvalid {
                        message: OBSERVATION_MESSAGE,
                        detail: "commit-rejected",
                    }
                } else {
                    let typed = MovePlanReply::from_local(direction, &plan);
                    if Self::commit_directional_move(&mut session, event, &plan, base) {
                        let source_key = source_key.clone();
                        let target_key = target_key.clone();
                        let source_outer_gap = event.outer_gap;
                        if self.store_canonical_pair(
                            source_key,
                            target_key,
                            session,
                            source_outer_gap,
                        ) {
                            return CoreReply::MoveDirectional(typed);
                        }
                    }
                    CoreReply::SnapshotInvalid {
                        message: OBSERVATION_MESSAGE,
                        detail: "commit-rejected",
                    }
                }
            }
            Ok(crate::session::SessionMoveOutcome::CapacityFloat(plan)) => {
                // R4 capacity fallback: the exact wrap proved geometric
                // exhaustion, so the established send result carries the
                // slotless float (same commit/split/store as the R4 move,
                // through the lifecycle verifier). The adapter actuates one
                // transfer plus the native cross-output remap with no mover
                // geometry write.
                let target_outer_gap = event
                    .directional_target_outer_gap
                    .filter(|gap| is_gap(*gap))
                    .unwrap_or(0);
                let Some(typed) = crate::boundary::SendWorkspacePlan::from_session(&plan) else {
                    return CoreReply::SnapshotInvalid {
                        message: OBSERVATION_MESSAGE,
                        detail: "move-op-invalid",
                    };
                };
                if Self::commit_lifecycle(&mut session, &plan, event, base) {
                    let Ok((source, target)) = session.split_canonical_pair() else {
                        return CoreReply::SnapshotInvalid {
                            message: OBSERVATION_MESSAGE,
                            detail: "commit-rejected",
                        };
                    };
                    self.store_committed(source_key.clone(), source, event.outer_gap);
                    if let Some(target) = target {
                        self.store_committed(target_key.clone(), target, target_outer_gap);
                    } else {
                        self.sessions.remove(target_key);
                        self.outer_gaps.remove(target_key);
                    }
                    return CoreReply::SendOutput(typed);
                }
                CoreReply::SnapshotInvalid {
                    message: OBSERVATION_MESSAGE,
                    detail: "commit-rejected",
                }
            }
            Err(error) => CoreReply::Rejected {
                kind: error.kind(),
                message: error.message(),
            },
        }
    }

    /// Synchronous acknowledge/verify/split for an R1-R3 directional move
    /// plan. Mirrors the legacy protocol commit helper exactly.
    fn commit_directional_move(
        session: &mut Session,
        event: &CoreEvent,
        plan: &crate::session::SessionMovePlan,
        base: u64,
    ) -> bool {
        let ack = AdapterAck::new(
            event.correlation.clone(),
            event.owner.clone(),
            event.generation.clone(),
            base,
            AckOutcome::Accepted,
        );
        if session.acknowledge(&ack).is_err() {
            return false;
        }
        let post = PostObservation::new(
            Observation::new(
                event.owner.clone(),
                event.generation.clone(),
                base,
                event.fingerprint,
            ),
            event.correlation.clone(),
            true,
            plan.dispatch.preconditions.clone(),
            plan.dispatch.operation.clone(),
        );
        session.verify_move(&post).is_ok()
    }

    /// Directional focus request phase: two-domain pair planning with local
    /// first then exhausted Left/Right cross, plus the legacy single-domain
    /// local path. Same ownership contract as
    /// [`Engine::directional_move_request`]: protocol keeps envelope,
    /// tagged decoding, pair scope shape, parsed-direction/op validation, and
    /// serialization; this owns assembly, focus sync, propose, and the
    /// synchronous acknowledge/verify/split/store. Single-domain observations
    /// (no two-entry pair) run the local retained propose/commit through
    /// [`Engine::local_focus_request`], preserving the legacy `run_retained`
    /// order exactly.
    ///
    /// Fence order matches the legacy protocol handler exactly: window opaque
    /// (`focus-window-invalid`), parsed direction (`direction-invalid`), pair
    /// presence (`domain-invalid`), canonical assembly (`canonical-*`), one
    /// convergence primitive on the assembled world (split/stored before the
    /// command), propose (mapped like `propose_failure`), then the sync
    /// commit (`commit-rejected` on failure).
    fn directional_focus_request(&mut self, event: &CoreEvent) -> CoreReply {
        use crate::boundary::FocusPlanReply;
        let CoreCommand::Focus {
            window,
            direction,
            float_subject,
            ..
        } = &event.command
        else {
            return CoreReply::Rejected {
                kind: "unknown-value",
                message: "request contains an unknown value",
            };
        };
        if event
            .directional
            .as_ref()
            .is_none_or(|pair| pair.len() != 2)
        {
            return self.local_focus_request(event, window, direction);
        }
        if !is_opaque_id(window) {
            return CoreReply::SnapshotInvalid {
                message: OPAQUE_ID_MESSAGE,
                detail: "focus-window-invalid",
            };
        }
        let Some(direction) = parse_engine_direction(direction) else {
            return CoreReply::Rejected {
                kind: DIRECTION_KIND,
                message: DIRECTION_MESSAGE,
            };
        };
        let Some(pair) = event.directional.as_ref().filter(|pair| pair.len() == 2) else {
            return CoreReply::SnapshotInvalid {
                message: OBSERVATION_MESSAGE,
                detail: "domain-invalid",
            };
        };
        let (source_domain, source_key) = &pair[0];
        let (target_domain, target_key) = &pair[1];
        let window = WindowId(window.clone());
        let mut session = match self.assemble_directional_pair(
            source_domain,
            source_key,
            target_domain,
            target_key,
        ) {
            Ok(session) => session,
            Err(detail) => {
                return CoreReply::SnapshotInvalid {
                    message: OBSERVATION_MESSAGE,
                    detail,
                };
            }
        };
        // One primitive on the assembled BOTH-domain world (never two
        // per-domain calls); the converged split persists below even if the
        // ordinary focus refuses.
        let (base, observation) = match self.converge_assembled_pair(
            &mut session,
            event,
            "focus",
            source_key,
            target_key,
        ) {
            Ok(next) => next,
            Err(reply) => return *reply,
        };
        let _ = session.sync_focus_from_window(source_key, &event.focused_window.clone());
        // Float-origin cross-output focus (adapter local float search missed
        // Left/Right): the subject is a floating/sticky exception, so the
        // tiled local proposal (which refuses exceptions before any edge
        // check) is skipped and the shared reciprocal-adjacency remembered
        // target fallback applies directly. The subject stays bound to the
        // active window; anything else refuses fail-closed.
        let (plan, crossed) = if *float_subject {
            if !matches!(direction, Direction::Left | Direction::Right) {
                return CoreReply::Rejected {
                    kind: RefusalKind::Unchanged.as_str(),
                    message: RefusalKind::Unchanged.message(),
                };
            }
            if window.0 != event.focused_window.0 {
                return CoreReply::Rejected {
                    kind: RefusalKind::FocusMismatch.as_str(),
                    message: RefusalKind::FocusMismatch.message(),
                };
            }
            // Home-bind the flagged subject: the active window must be
            // observed floating on the source domain (sticky rides the
            // adapter floating flag). Anything else refuses fail-closed.
            if !event.windows.iter().any(|entry| {
                entry.window == window
                    && entry.output == source_key.output
                    && entry.workspace == source_key.workspace
                    && entry.floating
            }) {
                return CoreReply::Rejected {
                    kind: RefusalKind::FocusMismatch.as_str(),
                    message: RefusalKind::FocusMismatch.message(),
                };
            }
            match session.propose_float_cross_output_focus(
                source_key,
                &window,
                direction,
                &observation,
                &event.correlation,
                &FocusCapabilities::full(),
            ) {
                Ok(plan) => (plan, true),
                Err(error) => {
                    return CoreReply::Rejected {
                        kind: error.kind(),
                        message: error.message(),
                    };
                }
            }
        } else {
            match session.propose_focus(
                source_key,
                &window,
                direction,
                &observation,
                &event.correlation,
                &FocusCapabilities::full(),
            ) {
                Ok(plan) => (plan, false),
                Err(ProposeError::Refused(RefusalKind::Unchanged))
                    if matches!(direction, Direction::Left | Direction::Right) =>
                {
                    match session.propose_cross_output_focus(
                        source_key,
                        &window,
                        direction,
                        &observation,
                        &event.correlation,
                        &FocusCapabilities::full(),
                    ) {
                        Ok(plan) => (plan, true),
                        Err(error) => {
                            return CoreReply::Rejected {
                                kind: error.kind(),
                                message: error.message(),
                            };
                        }
                    }
                }
                Err(error) => {
                    return CoreReply::Rejected {
                        kind: error.kind(),
                        message: error.message(),
                    };
                }
            }
        };
        let typed = if crossed {
            FocusPlanReply::from_cross(direction, &plan)
        } else {
            FocusPlanReply::from_local(direction, &plan)
        };
        if Self::commit_directional_focus(&mut session, event, &plan, base) {
            let source_key = source_key.clone();
            let target_key = target_key.clone();
            let source_outer_gap = event.outer_gap;
            if self.store_canonical_pair(source_key, target_key, session, source_outer_gap) {
                return CoreReply::FocusDirectional(typed);
            }
        }
        CoreReply::SnapshotInvalid {
            message: OBSERVATION_MESSAGE,
            detail: "commit-rejected",
        }
    }

    /// Synchronous acknowledge/verify/split for a directional focus plan.
    /// Mirrors the legacy protocol commit helper exactly.
    fn commit_directional_focus(
        session: &mut Session,
        event: &CoreEvent,
        plan: &crate::session::SessionFocusPlan,
        base: u64,
    ) -> bool {
        let ack = AdapterAck::new(
            event.correlation.clone(),
            event.owner.clone(),
            event.generation.clone(),
            base,
            AckOutcome::Accepted,
        );
        if session.acknowledge(&ack).is_err() {
            return false;
        }
        let post = FocusPostObservation::new(
            Observation::new(
                event.owner.clone(),
                event.generation.clone(),
                base,
                event.fingerprint,
            ),
            event.correlation.clone(),
            true,
            plan.dispatch.preconditions.clone(),
            plan.dispatch.operation.clone(),
        );
        session.verify_focus(&post).is_ok()
    }

    /// Legacy single-domain move through the shared retained lifecycle.
    ///
    /// Protocol keeps tagged command decoding, the directional-pair branch,
    /// window/direction authorization, and wire serialization at their exact
    /// positions; by the time an event reaches here the command decoded. This
    /// owns seed ordering, focus sync, capability-gated propose, relocation,
    /// commit, and store, preserving the legacy `run_retained` order exactly
    /// (ambiguous seed as `snapshot-invalid`/`missing-seed-order`, full
    /// capabilities so the carried transfer flag stays transport-only).
    fn local_move_request(
        &mut self,
        event: &CoreEvent,
        window: &str,
        direction: &str,
        same_axis_move: crate::directional::SameAxisMove,
    ) -> CoreReply {
        use crate::boundary::MovePlanReply;
        if !is_opaque_id(window) {
            return CoreReply::SnapshotInvalid {
                message: OPAQUE_ID_MESSAGE,
                detail: "move-window-invalid",
            };
        }
        let Some(direction) = parse_engine_direction(direction) else {
            return CoreReply::Rejected {
                kind: DIRECTION_KIND,
                message: DIRECTION_MESSAGE,
            };
        };
        let seed_order = crate::seed::order_spatial_with_focus_last(
            event.windows.clone(),
            &event.focused_window,
            false,
        );
        let window = WindowId(window.to_owned());
        self.run_retained(
            event,
            seed_order,
            true,
            |session, observation| {
                let _ = session.sync_focus_from_window(&event.domain_key, &event.focused_window);
                session
                    .propose_move_with_same_axis(
                        &event.domain_key,
                        &window,
                        direction,
                        observation,
                        &event.correlation,
                        &Capabilities::full(),
                        same_axis_move,
                    )
                    .and_then(|outcome| match outcome {
                        // Single-domain retained sessions cannot plan a
                        // cross-output wrap, so the R4 capacity fallback is
                        // unreachable here; the refusal below only preserves
                        // the no-mutation invariant if ever reached.
                        crate::session::SessionMoveOutcome::Moved(plan) => Ok(plan),
                        crate::session::SessionMoveOutcome::CapacityFloat(_) => {
                            Err(ProposeError::Refused(RefusalKind::MalformedTopology))
                        }
                    })
            },
            |plan| CoreReply::MoveDirectional(MovePlanReply::from_local(direction, plan)),
            |session, plan, event, base| Self::commit_directional_move(session, event, plan, base),
        )
    }

    /// Legacy single-domain focus through the shared retained lifecycle.
    ///
    /// Same ownership contract as [`Engine::local_move_request`]: protocol
    /// keeps tagged decoding, the directional-pair branch, and
    /// window/direction authorization; this owns seed ordering, focus sync,
    /// propose (local only, no cross fallback), commit, and store.
    fn local_focus_request(
        &mut self,
        event: &CoreEvent,
        window: &str,
        direction: &str,
    ) -> CoreReply {
        use crate::boundary::FocusPlanReply;
        if !is_opaque_id(window) {
            return CoreReply::SnapshotInvalid {
                message: OPAQUE_ID_MESSAGE,
                detail: "focus-window-invalid",
            };
        }
        let Some(direction) = parse_engine_direction(direction) else {
            return CoreReply::Rejected {
                kind: DIRECTION_KIND,
                message: DIRECTION_MESSAGE,
            };
        };
        let seed_order = crate::seed::order_spatial_with_focus_last(
            event.windows.clone(),
            &event.focused_window,
            false,
        );
        let window = WindowId(window.to_owned());
        self.run_retained(
            event,
            seed_order,
            true,
            |session, observation| {
                let _ = session.sync_focus_from_window(&event.domain_key, &event.focused_window);
                session.propose_focus(
                    &event.domain_key,
                    &window,
                    direction,
                    observation,
                    &event.correlation,
                    &FocusCapabilities::full(),
                )
            },
            |plan| CoreReply::FocusDirectional(FocusPlanReply::from_local(direction, plan)),
            |session, plan, event, base| Self::commit_directional_focus(session, event, plan, base),
        )
    }

    /// Legacy keyboard resize through the shared retained lifecycle.
    ///
    /// Protocol keeps tagged command decoding, window/direction/mode
    /// authorization, and wire serialization at their exact positions; this
    /// owns seed ordering, focus sync, keyboard-gated propose, commit, and
    /// store. Mode failures map to `direction-invalid` exactly like the
    /// legacy handler, and resize shares flow through the typed
    /// [`crate::boundary::ResizePlanReply`] unchanged.
    fn resize_request(&mut self, event: &CoreEvent) -> CoreReply {
        use crate::boundary::ResizePlanReply;
        let CoreCommand::Resize {
            window,
            direction,
            mode,
            press_index,
        } = &event.command
        else {
            return CoreReply::Rejected {
                kind: "unknown-value",
                message: "request contains an unknown value",
            };
        };
        if !is_opaque_id(window) {
            return CoreReply::SnapshotInvalid {
                message: OPAQUE_ID_MESSAGE,
                detail: "resize-window-invalid",
            };
        }
        let Some(direction) = parse_engine_direction(direction) else {
            return CoreReply::Rejected {
                kind: DIRECTION_KIND,
                message: DIRECTION_MESSAGE,
            };
        };
        let Some(mode) = parse_engine_mode(mode) else {
            return CoreReply::Rejected {
                kind: DIRECTION_KIND,
                message: DIRECTION_MESSAGE,
            };
        };
        let seed_order = crate::seed::order_spatial_with_focus_last(
            event.windows.clone(),
            &event.focused_window,
            false,
        );
        let window = WindowId(window.to_owned());
        let press_index = *press_index;
        self.run_retained(
            event,
            seed_order,
            true,
            |session, observation| {
                let _ = session.sync_focus_from_window(&event.domain_key, &event.focused_window);
                session.propose_resize(
                    &event.domain_key,
                    &window,
                    direction,
                    mode,
                    press_index,
                    observation,
                    &event.correlation,
                    &ResizeCapabilities {
                        keyboard_resize: true,
                        pointer_resize: false,
                    },
                )
            },
            |plan| CoreReply::Resize(ResizePlanReply::from_keyboard(direction, mode, plan)),
            |session, plan, event, base| Self::commit_resize(session, event, plan, base),
        )
    }

    /// Legacy pointer resize through the shared retained lifecycle.
    ///
    /// Same ownership contract as [`Engine::resize_request`] with the pointer
    /// capability gate and carried boundary; the boundary crosses opaquely
    /// with no authorization beyond the tagged decode, exactly like the
    /// legacy handler.
    fn pointer_resize_request(&mut self, event: &CoreEvent) -> CoreReply {
        use crate::boundary::ResizePlanReply;
        use crate::directional::Axis;
        let CoreCommand::PointerResize {
            window,
            direction,
            boundary,
            direction2,
            boundary2,
        } = &event.command
        else {
            return CoreReply::Rejected {
                kind: "unknown-value",
                message: "request contains an unknown value",
            };
        };
        if !is_opaque_id(window) {
            return CoreReply::SnapshotInvalid {
                message: OPAQUE_ID_MESSAGE,
                detail: "pointer-resize-window-invalid",
            };
        }
        let Some(direction) = parse_engine_direction(direction) else {
            return CoreReply::Rejected {
                kind: DIRECTION_KIND,
                message: DIRECTION_MESSAGE,
            };
        };
        // Corner second axis: both-or-neither (the protocol layer refuses a
        // half-present pair before this boundary); a present pair must parse
        // and must span perpendicular axes.
        let corner = match (direction2, boundary2) {
            (None, None) => None,
            (Some(raw), Some(second)) => {
                let Some(parsed) = parse_engine_direction(raw) else {
                    return CoreReply::Rejected {
                        kind: DIRECTION_KIND,
                        message: DIRECTION_MESSAGE,
                    };
                };
                if Axis::for_direction(direction) == Axis::for_direction(parsed) {
                    return CoreReply::Rejected {
                        kind: DIRECTION_KIND,
                        message: DIRECTION_MESSAGE,
                    };
                }
                Some((parsed, *second))
            }
            _ => {
                return CoreReply::Rejected {
                    kind: "unknown-value",
                    message: "request contains an unknown value",
                };
            }
        };
        let seed_order = crate::seed::order_spatial_with_focus_last(
            event.windows.clone(),
            &event.focused_window,
            false,
        );
        let window = WindowId(window.to_owned());
        let boundary = *boundary;
        match corner {
            None => self.run_retained(
                event,
                seed_order,
                true,
                |session, observation| {
                    let _ =
                        session.sync_focus_from_window(&event.domain_key, &event.focused_window);
                    session.propose_pointer_resize(
                        &event.domain_key,
                        &window,
                        direction,
                        boundary,
                        observation,
                        &event.correlation,
                        &ResizeCapabilities {
                            keyboard_resize: false,
                            pointer_resize: true,
                        },
                    )
                },
                |plan| CoreReply::Resize(ResizePlanReply::from_pointer(direction, boundary, plan)),
                |session, plan, event, base| Self::commit_resize(session, event, plan, base),
            ),
            Some((second_direction, second_boundary)) => {
                // Axis-normalized corner: horizontal primary, vertical
                // secondary, regardless of wire order.
                let (direction_h, boundary_h, direction_v, boundary_v) =
                    if Axis::for_direction(direction) == Axis::Horizontal {
                        (direction, boundary, second_direction, second_boundary)
                    } else {
                        (second_direction, second_boundary, direction, boundary)
                    };
                self.run_retained(
                    event,
                    seed_order,
                    true,
                    |session, observation| {
                        let _ = session
                            .sync_focus_from_window(&event.domain_key, &event.focused_window);
                        session.propose_pointer_resize_corner(
                            &event.domain_key,
                            &window,
                            direction_h,
                            boundary_h,
                            direction_v,
                            boundary_v,
                            observation,
                            &event.correlation,
                            &ResizeCapabilities {
                                keyboard_resize: false,
                                pointer_resize: true,
                            },
                        )
                    },
                    |plan| {
                        CoreReply::Resize(ResizePlanReply::from_pointer_corner(
                            direction_h,
                            boundary_h,
                            direction_v,
                            boundary_v,
                            plan,
                        ))
                    },
                    |session, plan, event, base| {
                        Self::commit_resize_corner(session, event, plan, base)
                    },
                )
            }
        }
    }

    /// Synchronous acknowledge/verify for a retained resize plan. Mirrors the
    /// legacy protocol commit closure exactly.
    fn commit_resize(
        session: &mut Session,
        event: &CoreEvent,
        plan: &crate::session::SessionResizePlan,
        base: u64,
    ) -> bool {
        if !engine_acknowledge(session, event, base) {
            return false;
        }
        let post = ResizePostObservation::new(
            Observation::new(
                event.owner.clone(),
                event.generation.clone(),
                base,
                event.fingerprint,
            ),
            event.correlation.clone(),
            true,
            plan.dispatch.preconditions.clone(),
            plan.dispatch.operation.clone(),
        );
        session.verify_resize(&post).is_ok()
    }

    /// Synchronous acknowledge/verify for a retained atomic corner resize
    /// plan. Mirrors [`Engine::commit_resize`] with both operation echoes
    /// bound, so the single pending corner transaction commits exactly once.
    fn commit_resize_corner(
        session: &mut Session,
        event: &CoreEvent,
        plan: &crate::session::SessionResizePlan,
        base: u64,
    ) -> bool {
        if !engine_acknowledge(session, event, base) {
            return false;
        }
        let post = ResizePostObservation::new_with_secondary(
            Observation::new(
                event.owner.clone(),
                event.generation.clone(),
                base,
                event.fingerprint,
            ),
            event.correlation.clone(),
            true,
            plan.dispatch.preconditions.clone(),
            plan.dispatch.operation.clone(),
            plan.dispatch.secondary_operation.clone(),
        );
        session.verify_resize(&post).is_ok()
    }

    /// Shared drag resolver for preview and drop: same complete observation,
    /// convergence, source binding, prior, resolve, and ordinary projection.
    ///
    /// Retained drops reuse the pre-request convergence in [`Engine::handle`];
    /// retained previews converge the discarded clone on the same complete
    /// observation so a new arrival previews where the drop resolves. Absent
    /// destinations seed fresh from the complete observation and converge once
    /// (never relocating the source). Preview never commits or stores; drop
    /// commits once. Empty/singleton cross-output falls back to the ordinary
    /// retained projection; same-output singletons snap back.
    fn drag_drop_request(&mut self, event: &CoreEvent) -> CoreReply {
        self.drag_resolve(event, false)
    }

    fn drag_preview_request(&mut self, event: &CoreEvent) -> CoreReply {
        self.drag_resolve(event, true)
    }

    /// Build the working session plus freshness flag, or the fence reply.
    ///
    /// Fresh seeds from the complete destination observation and converges
    /// once; retained applies the fences and clones. Never stores.
    fn drag_working(&self, event: &CoreEvent) -> Result<(Session, bool), Box<CoreReply>> {
        if !self.contains(&event.domain_key) {
            if !is_gap(event.outer_gap) || !is_gap(event.domain.gap) {
                return Err(Box::new(CoreReply::Rejected {
                    kind: "domain-mismatch",
                    message: "domain outer gap does not match retained state",
                }));
            }
            let mut fresh = Session::new(
                event.owner.clone(),
                event.generation.clone(),
                0,
                event.fingerprint,
                vec![event.domain.clone()],
            )
            .map_err(|_| {
                Box::new(CoreReply::SnapshotInvalid {
                    message: OBSERVATION_MESSAGE,
                    detail: "seed-failed",
                })
            })?;
            fresh.set_policy(self.policy.clone());
            let observation = drag_observation_for(event, fresh.accepted_revision());
            // Fresh drag domains join the Engine-wide sequence at the floor.
            self.sync_working_counter(&mut fresh);
            fresh
                .converge_observation(&observation, drag_focus_for(event))
                .map_err(|error| Box::new(drag_propose_reply(error)))?;
            if committed_session_is_empty(&fresh) {
                return Err(Box::new(CoreReply::Rejected {
                    kind: RefusalKind::UnknownDomain.as_str(),
                    message: RefusalKind::UnknownDomain.message(),
                }));
            }
            return Ok((fresh, true));
        }
        let Some(mut session) = self.session(&event.domain_key).cloned() else {
            return Err(Box::new(CoreReply::Rejected {
                kind: RefusalKind::UnknownDomain.as_str(),
                message: RefusalKind::UnknownDomain.message(),
            }));
        };
        if let Some(reason) = session.divergence() {
            return Err(Box::new(CoreReply::Rejected {
                kind: reason.as_str(),
                message: reason.message(),
            }));
        }
        if session.has_pending() || session.has_pending_desired() || session.has_drag() {
            return Err(Box::new(CoreReply::Rejected {
                kind: PENDING_EXISTS_KIND,
                message: PENDING_EXISTS_MESSAGE,
            }));
        }
        let retained_matches = session
            .domains()
            .iter()
            .find(|d| d.key() == event.domain_key)
            .is_some_and(|d| d.bounds == event.domain.bounds && d.gap == event.domain.gap);
        if !retained_matches {
            return Err(Box::new(CoreReply::Rejected {
                kind: RefusalKind::UnknownDomain.as_str(),
                message: RefusalKind::UnknownDomain.message(),
            }));
        }
        if self.outer_gap_ref(&event.domain_key).copied() != Some(event.outer_gap) {
            return Err(Box::new(CoreReply::Rejected {
                kind: "domain-mismatch",
                message: "domain outer gap does not match retained state",
            }));
        }
        // The retained counter may predate other domains; advance before
        // the resolve converges newcomers onto this working copy.
        self.sync_working_counter(&mut session);
        Ok((session, false))
    }

    /// Ordinary singleton projection for a cross-output single mover; `None`
    /// for same-output or multi-window sessions.
    fn drag_singleton_project(
        working: &Session,
        event: &CoreEvent,
        mover: &WindowId,
        source: &Option<DomainKey>,
    ) -> Option<(
        crate::boundary::ProjectionPlan,
        crate::directional::NodeId,
        Rect,
    )> {
        let binding = source.as_ref()?;
        if !is_opaque_id(binding.output.0.as_str())
            || !is_opaque_id(binding.workspace.0.as_str())
            || binding.output == event.domain_key.output
        {
            return None;
        }
        let snapshot = working.snapshot();
        if snapshot.windows.len() != 1 {
            return None;
        }
        let link = snapshot.windows.into_iter().next()?;
        if link.window != *mover {
            return None;
        }
        let hints = event
            .windows
            .iter()
            .map(|entry| (entry.window.clone(), entry.hints))
            .collect::<BTreeMap<_, _>>();
        let plan = project_retained_tiled_geometry(
            working,
            &event.domain_key,
            event.domain.bounds,
            event.domain.gap,
            Some((event.domain_key.clone(), link.leaf.clone())),
            ProjectionKind::Reconcile,
            &hints,
            &event.windows,
        )?;
        let rect = plan
            .geometry
            .iter()
            .find(|g| g.window == *mover)
            .map(|g| g.rect)?;
        let leaf = plan.focus_leaf.clone().unwrap_or(link.leaf.clone());
        Some((plan, leaf, rect))
    }

    #[allow(clippy::too_many_lines)]
    fn drag_resolve(&mut self, event: &CoreEvent, is_preview: bool) -> CoreReply {
        use crate::boundary::{DragPreviewPlan, TiledPlan};
        let (window, x, y, hover_prior, source) = match &event.command {
            CoreCommand::DragDrop {
                window,
                x,
                y,
                hover_prior,
                source,
            }
            | CoreCommand::DragPreview {
                window,
                x,
                y,
                hover_prior,
                source,
            } => (window, *x, *y, hover_prior, source),
            _ => {
                return CoreReply::Rejected {
                    kind: "unknown-value",
                    message: "request contains an unknown value",
                };
            }
        };
        if !is_opaque_id(window) {
            return CoreReply::SnapshotInvalid {
                message: OPAQUE_ID_MESSAGE,
                detail: "drag-drop-window-invalid",
            };
        }
        let mover = WindowId(window.to_owned());
        // Cross-output drag preview/drop through the occupied pair path: the
        // mover leaves the `source` domain for the pointer output's current
        // domain (`event.domain`), with the source-first pair in
        // `directional` and combined rows in `windows`. Cross-source events
        // without a pair keep the legacy singleton admission route below
        // (empty destinations); same-domain events likewise stay legacy.
        let cross_pair = source.as_ref().is_some_and(|key| *key != event.domain_key)
            && event
                .directional
                .as_ref()
                .is_some_and(|pair| pair.len() == 2);
        if cross_pair {
            return self.drag_cross_output(event, mover, x, y, hover_prior, source, is_preview);
        }
        let (mut working, is_fresh) = match self.drag_working(event) {
            Ok(pair) => pair,
            Err(reply) => return *reply,
        };
        if is_preview && !is_fresh {
            let observation = drag_observation_for(event, working.accepted_revision());
            if let Err(error) = working.converge_observation(&observation, drag_focus_for(event)) {
                if drag_preview_stage_ambiguous(&error) {
                    self.note_drag_preview_stage(
                        &event.correlation,
                        crate::session::DRAG_PREVIEW_STAGE_CONVERGE,
                        false,
                    );
                }
                return drag_propose_reply(error);
            }
        } else if !is_preview && is_fresh {
            self.converged_this_op = true;
        }
        if committed_session_is_empty(&working) {
            return drag_propose_reply(ProposeError::Refused(RefusalKind::UnknownDomain));
        }
        let base = working.accepted_revision();
        let observation = drag_observation_for(event, base);
        if is_fresh {
            let _ = working.sync_focus_from_window(&event.domain_key, &mover);
        } else {
            let _ = working.sync_focus_from_window(&event.domain_key, &event.focused_window);
        }
        if let Err(error) = working.begin_drag(&mover, &observation) {
            if is_preview && drag_preview_stage_ambiguous(&error) {
                self.note_drag_preview_stage(
                    &event.correlation,
                    crate::session::DRAG_PREVIEW_STAGE_BEGIN,
                    false,
                );
            }
            return drag_propose_reply(error);
        }
        if let Some(carried) = hover_prior {
            let _ = working.carry_drag_prior(carried);
        }
        if is_preview {
            match working.preview_drag(x, y, &observation.windows) {
                Ok(preview) => CoreReply::DragPreview(DragPreviewPlan {
                    base_revision: base,
                    preview,
                }),
                Err(ProposeError::Refused(RefusalKind::Unchanged)) => {
                    let Some((_, leaf, rect)) =
                        Self::drag_singleton_project(&working, event, &mover, source)
                    else {
                        return drag_propose_reply(ProposeError::Refused(RefusalKind::Unchanged));
                    };
                    CoreReply::DragPreview(DragPreviewPlan {
                        base_revision: base,
                        preview: crate::session::DragPreview {
                            domain: event.domain_key.clone(),
                            source_domain: source.clone().unwrap_or(event.domain_key.clone()),
                            source_leaf: leaf.clone(),
                            source_window: mover.clone(),
                            revision: base,
                            source_rect: rect,
                            target_leaf: leaf.clone(),
                            target_window: mover.clone(),
                            target_rect: rect,
                            proposed_rect: rect,
                            side: crate::contract::DragSide::Left,
                            axis: crate::directional::Axis::Horizontal,
                            before: true,
                            wrap: false,
                            target_group: leaf,
                            insertion_index: 0,
                            prior: None,
                        },
                    })
                }
                Err(error) => {
                    if let Some(stage) = working.preview_stage() {
                        self.note_drag_preview_stage(&event.correlation, stage, false);
                    }
                    drag_propose_reply(error)
                }
            }
        } else {
            match working.drop_drag(
                x,
                y,
                &observation,
                &event.correlation,
                &DragCapabilities::full(),
            ) {
                Ok(crate::session::DragRelease::Planned(plan)) => {
                    let typed = CoreReply::Tiled(TiledPlan::from_drag(&plan));
                    if Self::commit_drag(&mut working, event, &plan, base) {
                        self.store_committed(event.domain_key.clone(), working, event.outer_gap);
                        return typed;
                    }
                    if !is_fresh {
                        self.remove(&event.domain_key);
                    }
                    CoreReply::SnapshotInvalid {
                        message: OBSERVATION_MESSAGE,
                        detail: "commit-rejected",
                    }
                }
                Ok(crate::session::DragRelease::SnapBack(_)) => {
                    let Some((plan, leaf, _)) =
                        Self::drag_singleton_project(&working, event, &mover, source)
                    else {
                        return drag_propose_reply(ProposeError::Refused(RefusalKind::Unchanged));
                    };
                    let reply = CoreReply::Tiled(TiledPlan {
                        base_revision: plan.base_revision,
                        policy_version: LIFECYCLE_POLICY_VERSION,
                        kind: crate::boundary::TiledKind::DragDrop,
                        geometry: plan.geometry,
                        focus_domain: Some(event.domain_key.clone()),
                        focus_leaf: Some(leaf),
                        float_window: None,
                        float_rect: None,
                    });
                    if is_fresh {
                        self.store_committed(event.domain_key.clone(), working, event.outer_gap);
                    }
                    reply
                }
                Err(error) => drag_propose_reply(error),
            }
        }
    }

    /// Cross-output drag preview/drop through a two-domain pair transaction.
    ///
    /// The pointer output's current domain (`event.domain`) is the drop
    /// target; `source` names the capture domain the mover leaves. The event
    /// carries the source-first pair in `directional` (pointer-resolved, so
    /// the send-style assembly without an adjacency fence applies) and the
    /// combined both-domain window rows in `windows`. Empty destinations
    /// never reach here: the adapter routes those through singleton
    /// admission plus source reconcile.
    ///
    /// One convergence runs on the assembled BOTH-domain world (persisted
    /// even when the drop refuses, exactly like the directional pair), the
    /// capture binds on the source domain, resolution runs against the
    /// destination tree, and the drop commits source removal plus
    /// destination insertion atomically before the canonical split/store.
    /// Preview resolves identically without committing.
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::too_many_lines)]
    fn drag_cross_output(
        &mut self,
        event: &CoreEvent,
        mover: WindowId,
        x: i32,
        y: i32,
        hover_prior: &Option<DragHoverPrior>,
        source: &Option<DomainKey>,
        is_preview: bool,
    ) -> CoreReply {
        use crate::boundary::{DragPreviewPlan, TiledPlan};
        let Some(source_key) = source else {
            return CoreReply::SnapshotInvalid {
                message: OBSERVATION_MESSAGE,
                detail: "domain-invalid",
            };
        };
        let Some(pair) = event.directional.as_ref().filter(|pair| pair.len() == 2) else {
            return CoreReply::SnapshotInvalid {
                message: OBSERVATION_MESSAGE,
                detail: "domain-invalid",
            };
        };
        let (source_domain, pair_source_key) = &pair[0];
        let (target_domain, pair_target_key) = &pair[1];
        if pair_source_key != source_key || pair_target_key != &event.domain_key {
            return CoreReply::SnapshotInvalid {
                message: OBSERVATION_MESSAGE,
                detail: "domain-invalid",
            };
        }
        let source_component = Self::canonical_component_domain(source_domain);
        let target_component = Self::canonical_component_domain(target_domain);
        let source_session = match self.sessions.get(source_key).filter(|session| {
            session_usable(session)
                && !committed_session_is_empty(session)
                && session_domain_matches(session, &source_component)
        }) {
            Some(session) => session.clone(),
            None => {
                return CoreReply::SnapshotInvalid {
                    message: OBSERVATION_MESSAGE,
                    detail: if self.contains(source_key) {
                        "canonical-pair-unusable"
                    } else {
                        "canonical-source-unavailable"
                    },
                };
            }
        };
        let target_session = match self.sessions.get(&event.domain_key) {
            Some(session)
                if session_usable(session)
                    && !committed_session_is_empty(session)
                    && session_domain_matches(session, &target_component) =>
            {
                session.clone()
            }
            Some(_) => {
                return CoreReply::SnapshotInvalid {
                    message: OBSERVATION_MESSAGE,
                    detail: "canonical-pair-unusable",
                };
            }
            // No retained destination session: the adapter misrouted an
            // empty destination into the occupied pair path.
            None => {
                return CoreReply::SnapshotInvalid {
                    message: OBSERVATION_MESSAGE,
                    detail: "canonical-target-unavailable",
                };
            }
        };
        let mut session = match Session::paired_from_canonical_for_send(
            &source_session,
            Some(&target_session),
            vec![source_domain.clone(), target_domain.clone()],
        ) {
            Ok(session) => session,
            Err(error) => {
                return CoreReply::SnapshotInvalid {
                    message: OBSERVATION_MESSAGE,
                    detail: match error {
                        CanonicalPairError::MismatchedIdentity => {
                            "canonical-pair-identity-mismatch"
                        }
                        CanonicalPairError::UnusableInput => "canonical-pair-unusable",
                        CanonicalPairError::DomainMismatch => "canonical-pair-domain-mismatch",
                        CanonicalPairError::DuplicateState => "canonical-pair-duplicate-state",
                    },
                };
            }
        };
        let op = if is_preview {
            "drag-preview"
        } else {
            "drag-drop"
        };
        let (base, observation) = match self.converge_assembled_pair(
            &mut session,
            event,
            op,
            source_key,
            &event.domain_key,
        ) {
            Ok(next) => next,
            Err(reply) => {
                if is_preview && let CoreReply::Rejected { kind, .. } = reply.as_ref() {
                    let kind: &str = kind;
                    if kind == RefusalKind::CrossDomainMismatch.as_str()
                        || kind == RefusalKind::MalformedTopology.as_str()
                    {
                        self.note_drag_preview_stage(
                            &event.correlation,
                            crate::session::DRAG_PREVIEW_STAGE_CONVERGE,
                            true,
                        );
                    }
                }
                return *reply;
            }
        };
        // The capture binds on the source domain: the mover must be the
        // focused tiled window there (begin enforces this exactly).
        let _ = session.sync_focus_from_window(source_key, &mover);
        if let Err(error) = session.begin_drag(&mover, &observation) {
            if is_preview && drag_preview_stage_ambiguous(&error) {
                self.note_drag_preview_stage(
                    &event.correlation,
                    crate::session::DRAG_PREVIEW_STAGE_BEGIN,
                    true,
                );
            }
            return drag_propose_reply(error);
        }
        if let Some(carried) = hover_prior {
            let _ = session.carry_drag_prior(carried);
        }
        if is_preview {
            match session.preview_drag_cross_output(&event.domain_key, x, y, &observation) {
                Ok(preview) => CoreReply::DragPreview(DragPreviewPlan {
                    base_revision: base,
                    preview,
                }),
                Err(error) => {
                    if let Some(stage) = session.preview_stage() {
                        self.note_drag_preview_stage(&event.correlation, stage, true);
                    }
                    drag_propose_reply(error)
                }
            }
        } else {
            match session.drop_drag_cross_output(
                &event.domain_key,
                x,
                y,
                &observation,
                &event.correlation,
                &DragCapabilities::full(),
            ) {
                Ok(crate::session::DragRelease::Planned(plan)) => {
                    let typed = CoreReply::Tiled(TiledPlan::from_drag(&plan));
                    if Self::commit_drag(&mut session, event, &plan, base) {
                        let Ok((source, target)) = session.split_canonical_pair() else {
                            return CoreReply::SnapshotInvalid {
                                message: OBSERVATION_MESSAGE,
                                detail: "commit-rejected",
                            };
                        };
                        self.store_committed(source_key.clone(), source, event.outer_gap);
                        let target_outer_gap = event
                            .directional_target_outer_gap
                            .filter(|gap| is_gap(*gap))
                            .unwrap_or(0);
                        if let Some(target) = target {
                            self.store_committed(
                                event.domain_key.clone(),
                                target,
                                target_outer_gap,
                            );
                        } else {
                            self.sessions.remove(&event.domain_key);
                            self.outer_gaps.remove(&event.domain_key);
                        }
                        return typed;
                    }
                    CoreReply::SnapshotInvalid {
                        message: OBSERVATION_MESSAGE,
                        detail: "commit-rejected",
                    }
                }
                Ok(crate::session::DragRelease::SnapBack(_)) => {
                    drag_propose_reply(ProposeError::Refused(RefusalKind::Unchanged))
                }
                Err(error) => drag_propose_reply(error),
            }
        }
    }

    /// Synchronous acknowledge plus `verify_drag` commit for one retained
    /// drag plan. Mirrors the other synchronous commit helpers exactly.
    fn commit_drag(
        session: &mut Session,
        event: &CoreEvent,
        plan: &SessionDragPlan,
        base: u64,
    ) -> bool {
        if !engine_acknowledge(session, event, base) {
            return false;
        }
        let post = DragPostObservation::new(
            Observation::new(
                event.owner.clone(),
                event.generation.clone(),
                base,
                event.fingerprint,
            ),
            event.correlation.clone(),
            true,
            plan.dispatch.preconditions.clone(),
            plan.dispatch.operation.clone(),
        );
        session.verify_drag(&post).is_ok()
    }

    /// Portable output relocation: when no usable session exists for the
    /// target key, move a usable retained session with the same workspace id
    /// from a different output to the target, preserving topology, shares,
    /// focus, exceptions, and revision. Target collision, unique source,
    /// capacity rollback, and empty-session rules match the legacy behavior
    /// exactly.
    pub fn try_relocate_for_target(
        &mut self,
        target_key: &DomainKey,
        target_domain: &OutputDomain,
        request_outer_gap: i32,
    ) -> bool {
        if !is_gap(request_outer_gap) {
            return false;
        }
        if self.sessions.contains_key(target_key) {
            return false;
        }
        let mut source_key: Option<DomainKey> = None;
        for key in self.sessions.keys() {
            if key.workspace == target_key.workspace && key.output != target_key.output {
                if source_key.is_some() {
                    return false;
                }
                source_key = Some(key.clone());
            }
        }
        let Some(source) = source_key else {
            return false;
        };
        let Some(session) = self.sessions.get(&source).cloned() else {
            return false;
        };
        if !session_usable(&session) {
            return false;
        }
        if session.has_pending_desired() || session.has_drag() {
            return false;
        }
        if committed_session_is_empty(&session) {
            return false;
        }
        let mut moved = session.clone();
        if !moved.relocate_domain(&source, target_key, target_domain.bounds, target_domain.gap) {
            return false;
        }
        if committed_session_is_empty(&moved) {
            return false;
        }
        self.sessions.remove(&source);
        self.outer_gaps.remove(&source);
        self.outer_gaps
            .insert(target_key.clone(), request_outer_gap);
        self.note_working_counter(&moved);
        self.sessions.insert(target_key.clone(), moved);
        true
    }

    /// Hotplug reprojection: update retained bounds for `key` without touching
    /// topology, shares, membership, focus, or revision. `false` when unknown.
    pub fn reproject_retained(&mut self, key: &DomainKey, bounds: Rect) -> bool {
        let Some(session) = self.sessions.get_mut(key) else {
            return false;
        };
        session.reproject_domain(key, bounds);
        true
    }

    /// D7 repair attempt for a retained domain whose tree no longer
    /// projects at `bounds`/`gap`: adopt the geometry structurally, then
    /// float newest-first victims (or retain overlap) with the
    /// observation's hints, overlay protections, and live frames.
    /// Revision-neutral like gap adoption; the caller projects and
    /// stores on success. `None` is a narrow truthful hold. An
    /// actually-changed geometry re-evaluates under the current setting;
    /// identical geometry keeps an accepted overlap mark sheltered.
    fn try_repair_domain(
        session: &Session,
        key: &DomainKey,
        bounds: Rect,
        gap: i32,
        event: &CoreEvent,
    ) -> Option<Session> {
        let geometry_changed = session
            .domains()
            .iter()
            .find(|d| d.key() == *key)
            .is_none_or(|d| d.bounds != bounds || d.gap != gap);
        let mut repaired = session.clone();
        if !repaired.update_domain_gaps(key, bounds, gap) {
            return None;
        }
        let observed: BTreeMap<&WindowId, &crate::seed::EngineWindow> = event
            .windows
            .iter()
            .map(|entry| (&entry.window, entry))
            .collect();
        let hints: BTreeMap<WindowId, crate::size_hints::WindowSizeHints> = event
            .windows
            .iter()
            .map(|entry| (entry.window.clone(), entry.hints))
            .collect();
        let mut protected: BTreeSet<WindowId> = BTreeSet::new();
        let mut live_frames: BTreeMap<WindowId, Rect> = BTreeMap::new();
        for link in repaired.snapshot().windows.iter() {
            let Some(entry) = observed.get(&link.window) else {
                continue;
            };
            if !entry.floating {
                live_frames.insert(link.window.clone(), entry.rect);
            }
            if entry.fullscreen || entry.maximized || entry.sticky || entry.fit_excluded {
                protected.insert(link.window.clone());
            }
        }
        if repaired.repair_infeasible_domain(
            key,
            &hints,
            &protected,
            &live_frames,
            geometry_changed,
        ) {
            Some(repaired)
        } else {
            None
        }
    }
}

/// Shared drag observation at `base` from the complete carried window set.
fn drag_observation_for(event: &CoreEvent, base: u64) -> SessionObservation {
    crate::seed::session_observation_for(
        &event.owner,
        &event.generation,
        base,
        event.fingerprint,
        &event.windows,
    )
}

/// Shared carried focus (`None` when the event carries no focused window).
fn drag_focus_for(event: &CoreEvent) -> Option<&WindowId> {
    if event.focused_window.0.is_empty() {
        None
    } else {
        Some(&event.focused_window)
    }
}

/// Shared drag [`ProposeError`] mapping: divergences stay terminal, everything
/// else is a rejection with the exact kind/message.
fn drag_propose_reply(error: ProposeError) -> CoreReply {
    match error {
        ProposeError::Diverged(reason) => CoreReply::Diverged(reason),
        _ => CoreReply::Rejected {
            kind: error.kind(),
            message: error.message(),
        },
    }
}

/// Whether a drag [`ProposeError`] carries an ambiguous preview kind.
fn drag_preview_stage_ambiguous(error: &ProposeError) -> bool {
    matches!(
        error,
        ProposeError::Refused(RefusalKind::CrossDomainMismatch | RefusalKind::MalformedTopology)
    )
}

/// Closed single/pair stage token: the cross-output pair path prefixes
/// `pair-`, the single-domain path keeps the session token verbatim.
fn drag_preview_stage_token(base: &'static str, paired: bool) -> &'static str {
    const PAIRED: [(&str, &str); 8] = [
        (crate::session::DRAG_PREVIEW_STAGE_CONVERGE, "pair-converge"),
        (crate::session::DRAG_PREVIEW_STAGE_BEGIN, "pair-begin"),
        (crate::session::DRAG_PREVIEW_STAGE_TOPOLOGY, "pair-topology"),
        (
            crate::session::DRAG_PREVIEW_STAGE_STALE_CAPTURE,
            "pair-stale-capture",
        ),
        (crate::session::DRAG_PREVIEW_STAGE_SCOPE, "pair-scope"),
        (
            crate::session::DRAG_PREVIEW_STAGE_RESOLVE_OUTSIDE,
            "pair-resolve-outside",
        ),
        (
            crate::session::DRAG_PREVIEW_STAGE_RESOLVE_TOPOLOGY,
            "pair-resolve-topology",
        ),
        (
            crate::session::DRAG_PREVIEW_STAGE_PROJECTION,
            "pair-projection",
        ),
    ];
    if !paired {
        return base;
    }
    PAIRED
        .iter()
        .find_map(|(single, pair)| (*single == base).then_some(*pair))
        .unwrap_or(base)
}

/// Retained rebuild gate mirroring the protocol `needs_rebuild`: diverged or
/// partial-observation proposals discard and rebuild once, everything else
/// fails closed.
fn engine_needs_rebuild(error: &ProposeError) -> bool {
    match error {
        ProposeError::Diverged(_) => true,
        ProposeError::Refused(RefusalKind::PartialObservation) => true,
        ProposeError::PendingExists => false,
        ProposeError::Refused(_) => false,
    }
}

/// Synchronous acknowledge helper mirroring the protocol commit closure.
fn engine_acknowledge(session: &mut Session, event: &CoreEvent, base: u64) -> bool {
    let ack = AdapterAck::new(
        event.correlation.clone(),
        event.owner.clone(),
        event.generation.clone(),
        base,
        AckOutcome::Accepted,
    );
    session.acknowledge(&ack).is_ok()
}

/// Parsed-direction gate mirroring the protocol vocabulary.
fn parse_engine_direction(value: &str) -> Option<Direction> {
    match value {
        "left" => Some(Direction::Left),
        "right" => Some(Direction::Right),
        "up" => Some(Direction::Up),
        "down" => Some(Direction::Down),
        _ => None,
    }
}

/// Stable direction token shared by the migration report and its summary.
fn direction_token(direction: Direction) -> &'static str {
    match direction {
        Direction::Left => "left",
        Direction::Right => "right",
        Direction::Up => "up",
        Direction::Down => "down",
    }
}

/// Parsed-mode gate mirroring the protocol vocabulary.
fn parse_engine_mode(value: &str) -> Option<ResizeMode> {
    match value {
        "inwards" => Some(ResizeMode::Inwards),
        "outwards" => Some(ResizeMode::Outwards),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::directional::{OutputId, WorkspaceId};
    use crate::geometry::Rect;
    use std::collections::BTreeMap;

    fn domain(output: &str, workspace: &str) -> OutputDomain {
        OutputDomain {
            id: OutputId(output.to_owned()),
            workspace: WorkspaceId(workspace.to_owned()),
            bounds: Rect {
                x: 0,
                y: 0,
                w: 800,
                h: 600,
            },
            gap: 0,
            adjacent: BTreeMap::new(),
        }
    }

    fn new_session(owner: &OwnerId, gen_id: &GenerationId, domain: OutputDomain) -> Session {
        Session::new(
            OwnerId::parse(owner.as_str()).expect("valid"),
            GenerationId::parse(gen_id.as_str()).expect("valid"),
            0,
            7,
            vec![domain],
        )
        .expect("session")
    }

    #[test]
    fn binding_change_clears_world() {
        let mut engine = Engine::new();
        let owner_a = OwnerId::parse("owner-a").expect("valid");
        let owner_b = OwnerId::parse("owner-b").expect("valid");
        let gen_id = GenerationId::parse("gen-1").expect("valid");
        engine.sync_binding(&owner_a, &gen_id);
        assert_eq!(engine.owner().expect("bound").as_str(), "owner-a");
        let d = domain("out", "ws");
        let key = d.key();
        let session = new_session(&owner_a, &gen_id, d);
        engine.insert_raw(key.clone(), session, 0);
        assert_eq!(engine.retained_domains(), 1);
        engine.sync_binding(&owner_a, &gen_id);
        assert_eq!(engine.retained_domains(), 1);
        engine.sync_binding(&owner_b, &gen_id);
        assert_eq!(engine.retained_domains(), 0);
        assert_eq!(engine.outer_gap(&key), None);
    }

    /// Admit one window into a fresh single-domain session through the full
    /// propose/acknowledge/verify cycle, leaving committed (non-empty) state.
    fn admit_one(
        session: &mut Session,
        owner: &OwnerId,
        gen_id: &GenerationId,
        output: &str,
        workspace: &str,
        window: &str,
    ) {
        use crate::contract::{
            AckOutcome, AdapterAck, LifecycleCapabilities, LifecyclePostObservation, Observation,
        };
        use crate::directional::WindowId;
        use crate::ids::CorrelationId;
        use crate::session::{ExceptionFlags, ObservedWindow, SessionCommand, SessionObservation};
        let rev = session.accepted_revision();
        let window_id = WindowId(window.to_owned());
        let observation = SessionObservation {
            observation: Observation::new(owner.clone(), gen_id.clone(), rev, 100 + rev),
            windows: vec![ObservedWindow {
                window: window_id.clone(),
                output: OutputId(output.to_owned()),
                workspace: WorkspaceId(workspace.to_owned()),
                floating: false,
                fullscreen: false,
                maximized: false,
                sticky: false,
                fixed_auto: false,
                fixed_suppress: false,
                hints: crate::size_hints::WindowSizeHints::none(),
            }],
        };
        let correlation = CorrelationId::parse(&format!("corr-{window}")).expect("valid");
        let command = SessionCommand::Admit {
            window: window_id,
            output: OutputId(output.to_owned()),
            workspace: WorkspaceId(workspace.to_owned()),
            exceptions: ExceptionFlags::none(),
            exception_behavior: None,
            placement_bounds: Rect {
                x: 0,
                y: 0,
                w: 100,
                h: 50,
            },
            suppress_fixed_float: false,
        };
        let plan = session
            .propose(
                &command,
                &observation,
                &correlation,
                &LifecycleCapabilities::full(),
            )
            .expect("admit propose");
        session
            .acknowledge(&AdapterAck::new(
                correlation.clone(),
                owner.clone(),
                gen_id.clone(),
                rev,
                AckOutcome::Accepted,
            ))
            .expect("admit ack");
        session
            .verify_lifecycle(&LifecyclePostObservation::new(
                Observation::new(owner.clone(), gen_id.clone(), rev, 200 + rev),
                correlation,
                true,
                plan.dispatch.preconditions.clone(),
                plan.dispatch.operation.clone(),
            ))
            .expect("admit verify");
    }

    #[test]
    fn store_committed_retains_beyond_old_domain_cap_and_retires_empty() {
        let mut engine = Engine::new();
        let owner = OwnerId::parse("owner-a").expect("valid");
        let gen_id = GenerationId::parse("gen-1").expect("valid");
        engine.sync_binding(&owner, &gen_id);
        // Empty results still retire through the production commit path.
        let d = domain("out", "ws");
        let key = d.key();
        let session = new_session(&owner, &gen_id, d);
        engine.store_committed(key.clone(), session.clone(), 4);
        assert_eq!(engine.retained_domains(), 0);
        assert_eq!(engine.outer_gap(&key), None);
        // No retained-domain count cap on the production path: 20 committed
        // single-window sessions (well beyond the old 16-domain bound) are
        // all retained via `store_committed`, never refused or evicted.
        for i in 0..20 {
            let output = format!("out-{i}");
            let workspace = format!("ws-{i}");
            let window = format!("win-{i}");
            let d = domain(&output, &workspace);
            let key = d.key();
            let mut session = new_session(&owner, &gen_id, d);
            admit_one(&mut session, &owner, &gen_id, &output, &workspace, &window);
            engine.store_committed(key.clone(), session, 0);
            assert!(engine.contains(&key), "domain {i} retained");
            assert_eq!(engine.retained_domains(), i + 1);
        }
    }

    #[test]
    fn take_usable_retires_mismatch_without_proposing() {
        let mut engine = Engine::new();
        let owner = OwnerId::parse("owner-a").expect("valid");
        let gen_id = GenerationId::parse("gen-1").expect("valid");
        engine.sync_binding(&owner, &gen_id);
        let d = domain("out", "ws");
        let key = d.key();
        let session = new_session(&owner, &gen_id, d.clone());
        engine.insert_raw(key.clone(), session, 0);
        assert!(engine.take_usable_session(&key, &d).is_none());
        assert!(!engine.contains(&key));
    }

    #[test]
    fn converged_move_with_changed_bounds_retains_session() {
        use crate::boundary::{CoreCommand, CoreEvent, CoreReply};
        use crate::ids::CorrelationId;
        use crate::seed::EngineWindow;
        let mut engine = Engine::new();
        let owner = OwnerId::parse("owner-a").expect("valid");
        let gen_id = GenerationId::parse("gen-1").expect("valid");
        engine.sync_binding(&owner, &gen_id);
        let d = domain("out", "ws");
        let key = d.key();
        let seed_order = ["win-1", "win-2"]
            .iter()
            .map(|window| EngineWindow {
                window: WindowId(window.to_string()),
                output: OutputId("out".to_owned()),
                workspace: WorkspaceId("ws".to_owned()),
                rect: Rect {
                    x: 0,
                    y: 0,
                    w: 10,
                    h: 10,
                },
                floating: false,
                fit_excluded: false,
                fullscreen: false,
                maximized: false,
                sticky: false,
                fixed_auto: false,
                fixed_suppress: false,
                hints: crate::size_hints::WindowSizeHints::none(),
            })
            .collect::<Vec<_>>();
        let seeded =
            crate::seed::seed_session(&owner, &gen_id, 7, &d, &seed_order).expect("seeds two");
        let pre_revision = seeded.accepted_revision();
        engine.store_committed(key.clone(), seeded, 0);
        let mut changed = d.clone();
        changed.bounds = Rect {
            x: 0,
            y: 0,
            w: 1024,
            h: 768,
        };
        let windows = ["win-1", "win-2", "win-3"]
            .iter()
            .enumerate()
            .map(|(i, window)| EngineWindow {
                window: WindowId(window.to_string()),
                output: OutputId("out".to_owned()),
                workspace: WorkspaceId("ws".to_owned()),
                rect: Rect {
                    x: (i as i32) * 100,
                    y: 0,
                    w: 10,
                    h: 10,
                },
                floating: false,
                fit_excluded: false,
                fullscreen: false,
                maximized: false,
                sticky: false,
                fixed_auto: false,
                fixed_suppress: false,
                hints: crate::size_hints::WindowSizeHints::none(),
            })
            .collect::<Vec<_>>();
        let event = CoreEvent {
            owner: owner.clone(),
            generation: gen_id.clone(),
            correlation: CorrelationId::parse("corr-conv-bounds-retain").expect("valid"),
            revision: 0,
            fingerprint: 7,
            domain: changed,
            domain_key: key.clone(),
            outer_gap: 0,
            focused_window: WindowId("win-1".to_owned()),
            windows: windows.clone(),
            directional: None,
            directional_target_outer_gap: None,
            target_domain: None,
            target_windows: vec![],
            closed_windows: vec![],
            command: CoreCommand::Move {
                window: "win-1".to_owned(),
                direction: "left".to_owned(),
                cross_output_transfer: false,
                same_axis_move: crate::directional::SameAxisMove::GroupWithNeighbor,
            },
        };
        match engine.handle(&event) {
            CoreReply::Rejected { kind, message } => {
                assert_eq!(kind, "partial-observation");
                assert_eq!(message, "observation does not cover the known window set");
            }
            other => panic!("changed-bounds move must reject partial-observation, got {other:?}"),
        }
        assert!(engine.contains(&key), "refused op must retain slot");
        assert_eq!(engine.outer_gap(&key), Some(0), "outer gap preserved");
        let retained = engine.session(&key).expect("session retained");
        assert!(
            retained.accepted_revision() >= pre_revision,
            "converged revision preserved"
        );
        let mut members: Vec<String> = retained
            .snapshot()
            .windows
            .iter()
            .map(|l| l.window.0.clone())
            .collect();
        members.sort();
        assert_eq!(
            members,
            vec![
                "win-1".to_string(),
                "win-2".to_string(),
                "win-3".to_string()
            ],
            "converged membership preserved"
        );
        let reconcile = CoreEvent {
            owner: owner.clone(),
            generation: gen_id.clone(),
            correlation: CorrelationId::parse("corr-conv-bounds-reconcile").expect("valid"),
            revision: 0,
            fingerprint: 7,
            domain: d.clone(),
            domain_key: key.clone(),
            outer_gap: 0,
            focused_window: WindowId("win-1".to_owned()),
            windows,
            directional: None,
            directional_target_outer_gap: None,
            target_domain: None,
            target_windows: vec![],
            closed_windows: vec![],
            command: CoreCommand::Reconcile,
        };
        match engine.handle(&reconcile) {
            CoreReply::Projection(plan) => assert_eq!(
                plan.geometry.len(),
                3,
                "correct-domain reconcile projects survivors"
            ),
            other => panic!("reconcile must project survivors, got {other:?}"),
        }
    }

    #[test]
    fn relocate_refuses_pending_gap_collision_and_ambiguity() {
        let mut engine = Engine::new();
        let owner = OwnerId::parse("owner-a").expect("valid");
        let gen_id = GenerationId::parse("gen-1").expect("valid");
        engine.sync_binding(&owner, &gen_id);
        let d = domain("out", "ws");
        let key = d.key();
        let session = new_session(&owner, &gen_id, d.clone());
        engine.insert_raw(key.clone(), session, 5);
        let target = domain("out-2", "ws");
        let target_key = target.key();
        // Empty source refuses (no topology to preserve).
        assert!(!engine.try_relocate_for_target(&target_key, &target, 0));
        assert!(!engine.contains(&target_key));
        assert!(engine.contains(&key));
        // Out-of-range outer gap refuses.
        assert!(!engine.try_relocate_for_target(&target_key, &target, 65));
        assert!(!engine.try_relocate_for_target(&target_key, &target, -1));
        // Target collision refuses even though the target slot is empty.
        engine.insert_raw(
            target_key.clone(),
            new_session(&owner, &gen_id, target.clone()),
            0,
        );
        assert!(!engine.try_relocate_for_target(&target_key, &target, 0));
        engine.remove(&target_key);
        // Ambiguous sources refuse.
        engine.insert_raw(
            domain("out-3", "ws").key(),
            new_session(&owner, &gen_id, domain("out-3", "ws")),
            0,
        );
        assert_eq!(engine.find_unique_source_for_target(&target_key), None);
        assert!(!engine.try_relocate_for_target(&target_key, &target, 0));
    }

    #[test]
    fn relocate_moves_seeded_topology_with_outer_gap() {
        use crate::directional::{OutputId, WorkspaceId};
        use crate::seed::{EngineWindow, seed_session};
        let mut engine = Engine::new();
        let owner = OwnerId::parse("owner-a").expect("valid");
        let gen_id = GenerationId::parse("gen-1").expect("valid");
        engine.sync_binding(&owner, &gen_id);
        let source_domain = domain("out", "ws");
        let source_key = source_domain.key();
        let order = vec![EngineWindow {
            window: crate::directional::WindowId("win-1".to_owned()),
            output: OutputId("out".to_owned()),
            workspace: WorkspaceId("ws".to_owned()),
            rect: Rect {
                x: 0,
                y: 0,
                w: 10,
                h: 10,
            },
            floating: false,
            fit_excluded: false,
            fullscreen: false,
            maximized: false,
            sticky: false,
            fixed_auto: false,
            fixed_suppress: false,
            hints: crate::size_hints::WindowSizeHints::none(),
        }];
        let seeded = seed_session(&owner, &gen_id, 7, &source_domain, &order).expect("seeds");
        let revision = seeded.accepted_revision();
        engine.insert_raw(source_key.clone(), seeded, 3);
        let target_domain = OutputDomain {
            id: OutputId("out-2".to_owned()),
            workspace: WorkspaceId("ws".to_owned()),
            bounds: Rect {
                x: 0,
                y: 0,
                w: 800,
                h: 600,
            },
            gap: 0,
            adjacent: BTreeMap::new(),
        };
        let target_key = target_domain.key();
        assert_eq!(
            engine.find_unique_source_for_target(&target_key),
            Some(source_key.clone())
        );
        assert!(engine.outer_gap_matches(&source_key, 3));
        assert!(!engine.outer_gap_matches(&source_key, 0));
        assert!(engine.try_relocate_for_target(&target_key, &target_domain, 9));
        assert!(!engine.contains(&source_key));
        assert!(engine.contains(&target_key));
        assert_eq!(engine.outer_gap(&target_key), Some(9));
        let moved = engine.session(&target_key).expect("moved");
        assert_eq!(moved.accepted_revision(), revision);
        assert_eq!(moved.snapshot().windows.len(), 1);
    }

    #[test]
    fn relocated_target_skew_rejects_partial_observation_with_rollback() {
        use crate::boundary::CoreCommand;
        use crate::directional::WindowId;
        use crate::ids::CorrelationId;
        use crate::seed::EngineWindow;
        let mut engine = Engine::new();
        let owner = OwnerId::parse("owner-a").expect("valid");
        let gen_id = GenerationId::parse("gen-1").expect("valid");
        engine.sync_binding(&owner, &gen_id);
        // Source with two committed members; the target slot is absent so the
        // reconcile relocates (same outer gap) without converging first.
        let source_domain = domain("out", "ws");
        let source_key = source_domain.key();
        let seed_order = ["win-1", "win-2"]
            .iter()
            .map(|window| EngineWindow {
                window: WindowId(window.to_string()),
                output: OutputId("out".to_owned()),
                workspace: WorkspaceId("ws".to_owned()),
                rect: Rect {
                    x: 0,
                    y: 0,
                    w: 10,
                    h: 10,
                },
                floating: false,
                fit_excluded: false,
                fullscreen: false,
                maximized: false,
                sticky: false,
                fixed_auto: false,
                fixed_suppress: false,
                hints: crate::size_hints::WindowSizeHints::none(),
            })
            .collect::<Vec<_>>();
        let source = crate::seed::seed_session(&owner, &gen_id, 7, &source_domain, &seed_order)
            .expect("seeds two");
        let source_revision = source.accepted_revision();
        engine.store_committed(source_key.clone(), source, 3);
        let target_domain = domain("out-2", "ws");
        let target_key = target_domain.key();
        let event = CoreEvent {
            owner: owner.clone(),
            generation: gen_id.clone(),
            correlation: CorrelationId::parse("corr-reloc-skew").expect("valid"),
            revision: 0,
            fingerprint: 7,
            domain: target_domain.clone(),
            domain_key: target_key.clone(),
            outer_gap: 3,
            focused_window: WindowId("win-1".to_owned()),
            windows: vec![EngineWindow {
                window: WindowId("win-1".to_owned()),
                output: OutputId("out-2".to_owned()),
                workspace: WorkspaceId("ws".to_owned()),
                rect: Rect {
                    x: 0,
                    y: 0,
                    w: 10,
                    h: 10,
                },
                floating: false,
                fit_excluded: false,
                fullscreen: false,
                maximized: false,
                sticky: false,
                fixed_auto: false,
                fixed_suppress: false,
                hints: crate::size_hints::WindowSizeHints::none(),
            }],
            directional: None,
            directional_target_outer_gap: None,
            target_domain: None,
            target_windows: vec![],
            closed_windows: vec![],
            command: CoreCommand::Reconcile,
        };
        match engine.handle(&event) {
            CoreReply::Rejected { kind, .. } => assert_eq!(
                kind, "partial-observation",
                "relocated skew must reject without projecting"
            ),
            other => panic!("relocated skew must reject partial-observation, got {other:?}"),
        }
        // Atomic rollback: the exact source state returns, the target stays
        // absent, and no revision or outer gap moved.
        assert!(!engine.contains(&target_key), "target stays absent");
        assert!(engine.contains(&source_key), "source restored");
        let restored = engine.session(&source_key).expect("source restored");
        assert_eq!(restored.accepted_revision(), source_revision);
        let mut members: Vec<String> = restored
            .snapshot()
            .windows
            .iter()
            .map(|l| l.window.0.clone())
            .collect();
        members.sort();
        assert_eq!(members, vec!["win-1".to_string(), "win-2".to_string()]);
        assert_eq!(engine.outer_gap(&source_key), Some(3));
        assert_eq!(engine.outer_gap(&target_key), None);
    }

    #[test]
    fn fresh_reconcile_mixed_float_names_first_tiled() {
        use crate::boundary::{CoreCommand, CoreEvent, CoreReply};
        use crate::ids::CorrelationId;
        // Focused window is floating, so the anchor is the first tiled
        // member; the complete observation still converges the exception.
        let d = domain("out", "ws");
        let key = d.key();
        let windows = vec![
            EngineWindow {
                window: WindowId("win-float".to_owned()),
                output: OutputId("out".to_owned()),
                workspace: WorkspaceId("ws".to_owned()),
                rect: Rect {
                    x: 0,
                    y: 0,
                    w: 100,
                    h: 100,
                },
                floating: true,
                fit_excluded: true,
                fullscreen: false,
                maximized: false,
                sticky: false,
                fixed_auto: false,
                fixed_suppress: false,
                hints: crate::size_hints::WindowSizeHints::none(),
            },
            EngineWindow {
                window: WindowId("win-1".to_owned()),
                output: OutputId("out".to_owned()),
                workspace: WorkspaceId("ws".to_owned()),
                rect: Rect {
                    x: 0,
                    y: 0,
                    w: 400,
                    h: 600,
                },
                floating: false,
                fit_excluded: false,
                fullscreen: false,
                maximized: false,
                sticky: false,
                fixed_auto: false,
                fixed_suppress: false,
                hints: crate::size_hints::WindowSizeHints::none(),
            },
            EngineWindow {
                window: WindowId("win-2".to_owned()),
                output: OutputId("out".to_owned()),
                workspace: WorkspaceId("ws".to_owned()),
                rect: Rect {
                    x: 400,
                    y: 0,
                    w: 400,
                    h: 600,
                },
                floating: false,
                fit_excluded: false,
                fullscreen: false,
                maximized: false,
                sticky: false,
                fixed_auto: false,
                fixed_suppress: false,
                hints: crate::size_hints::WindowSizeHints::none(),
            },
        ];
        let owner = OwnerId::parse("owner-a").expect("valid");
        let gen_id = GenerationId::parse("gen-1").expect("valid");
        let mut engine = Engine::new();
        engine.sync_binding(&owner, &gen_id);
        let event = CoreEvent {
            owner: owner.clone(),
            generation: gen_id.clone(),
            correlation: CorrelationId::parse("corr-rec-mixed").expect("valid"),
            revision: 0,
            fingerprint: 7,
            domain: d,
            domain_key: key.clone(),
            outer_gap: 0,
            focused_window: WindowId("win-float".to_owned()),
            windows,
            directional: None,
            directional_target_outer_gap: None,
            target_domain: None,
            target_windows: vec![],
            closed_windows: vec![],
            command: CoreCommand::Reconcile,
        };
        match engine.handle(&event) {
            CoreReply::Tiled(plan) => {
                let mut members: Vec<String> = plan
                    .geometry
                    .iter()
                    .map(|entry| entry.window.0.clone())
                    .collect();
                members.sort();
                assert_eq!(members, vec!["win-1".to_string(), "win-2".to_string()]);
            }
            other => panic!("fresh mixed reconcile must plan tiled, got {other:?}"),
        }
        let session = engine.session(&key).expect("fresh domain retained");
        assert!(
            session.is_exception(&WindowId("win-float".to_owned())),
            "floating member converges as an exception"
        );
    }

    /// Restart-hydration candidate shape from the recorded 2026-10-17
    /// `duplicate-window` rejection (run `01dd5da7`, tick 1): three staged
    /// floats (two sticky, one ordinary) sharing one centered frame plus
    /// two tiled controls carrying fresh minimums. Only the float frame
    /// `[517,281,1526,818]` and the control minimums (`[1157,789]` and
    /// `[401,246]`) come from the failure record; the original logs were
    /// removed at cleanup, so domain bounds, tiled frames, focus, and
    /// whether either minimum pins its maximum are reconstruction, marked
    /// below. Staged rows ride hintless with `fit_excluded: false`, exactly
    /// as `build_reconcile_event_for_floating` carries them.
    fn restart_hydration_windows(
        control_hints: crate::size_hints::WindowSizeHints,
        other_hints: crate::size_hints::WindowSizeHints,
    ) -> Vec<crate::seed::EngineWindow> {
        use crate::directional::WindowId;
        let float_rect = Rect {
            x: 517,
            y: 281,
            w: 1526,
            h: 818,
        };
        let mut windows = Vec::new();
        for id in ["f1", "f2", "f3"] {
            windows.push(crate::seed::EngineWindow {
                window: WindowId(id.to_owned()),
                output: OutputId("out".to_owned()),
                workspace: WorkspaceId("ws".to_owned()),
                rect: float_rect,
                floating: true,
                fit_excluded: false,
                fullscreen: false,
                maximized: false,
                sticky: false,
                fixed_auto: false,
                fixed_suppress: false,
                hints: crate::size_hints::WindowSizeHints::none(),
            });
        }
        // Reconstruction: exact tiled frames are unrecoverable.
        windows.push(crate::seed::EngineWindow {
            window: WindowId("c1".to_owned()),
            output: OutputId("out".to_owned()),
            workspace: WorkspaceId("ws".to_owned()),
            rect: Rect {
                x: 48,
                y: 20,
                w: 1157,
                h: 789,
            },
            floating: false,
            fit_excluded: false,
            fullscreen: false,
            maximized: false,
            sticky: false,
            fixed_auto: false,
            fixed_suppress: false,
            hints: control_hints,
        });
        windows.push(crate::seed::EngineWindow {
            window: WindowId("c2".to_owned()),
            output: OutputId("out".to_owned()),
            workspace: WorkspaceId("ws".to_owned()),
            rect: Rect {
                x: 8,
                y: 8,
                w: 2544,
                h: 1364,
            },
            floating: false,
            fit_excluded: false,
            fullscreen: false,
            maximized: false,
            sticky: false,
            fixed_auto: false,
            fixed_suppress: false,
            hints: other_hints,
        });
        windows
    }

    /// Fresh reconcile over the recorded hydration shape. Reconstruction
    /// domain mirrors the throwaway probe bounds; gap matches production.
    fn restart_hydration_event(
        owner: &OwnerId,
        gen_id: &GenerationId,
        domain: OutputDomain,
        windows: Vec<crate::seed::EngineWindow>,
    ) -> crate::boundary::CoreEvent {
        use crate::boundary::{CoreCommand, CoreEvent};
        use crate::directional::WindowId;
        use crate::ids::CorrelationId;
        let key = domain.key();
        CoreEvent {
            owner: owner.clone(),
            generation: gen_id.clone(),
            correlation: CorrelationId::parse("corr-restart-hydration").expect("valid"),
            revision: 0,
            fingerprint: 7,
            domain,
            domain_key: key,
            outer_gap: 8,
            focused_window: WindowId("c1".to_owned()),
            windows,
            directional: None,
            directional_target_outer_gap: None,
            target_domain: None,
            target_windows: vec![],
            closed_windows: vec![],
            command: CoreCommand::Reconcile,
        }
    }

    fn restart_hydration_domain() -> OutputDomain {
        OutputDomain {
            id: OutputId("out".to_owned()),
            workspace: WorkspaceId("ws".to_owned()),
            bounds: Rect {
                x: 8,
                y: 8,
                w: 2544,
                h: 1364,
            },
            gap: 8,
            adjacent: BTreeMap::new(),
        }
    }

    #[test]
    fn restart_hydration_min_only_controls_adopt_cleanly() {
        use crate::boundary::CoreReply;
        use crate::directional::WindowId;
        // Recorded minimums carried as advisory minimums only (maximums
        // unset, so neither control is fixed-size): this shape adopts
        // cleanly, matching the recorded reconstructed-input probe. It
        // pins the non-reproducing baseline: the rejection trigger needs
        // a fact this shape lacks (exact tiled frames, focus, or
        // maximum-pinned hints).
        let control = crate::size_hints::WindowSizeHints {
            min_w: Some(1157),
            min_h: Some(789),
            ..crate::size_hints::WindowSizeHints::none()
        };
        let other = crate::size_hints::WindowSizeHints {
            min_w: Some(401),
            min_h: Some(246),
            ..crate::size_hints::WindowSizeHints::none()
        };
        let owner = OwnerId::parse("owner-a").expect("valid");
        let gen_id = GenerationId::parse("gen-1").expect("valid");
        let mut engine = Engine::new();
        engine.sync_binding(&owner, &gen_id);
        engine.set_fixed_size_admission(true);
        let domain = restart_hydration_domain();
        let key = domain.key();
        let event = restart_hydration_event(
            &owner,
            &gen_id,
            domain,
            restart_hydration_windows(control, other),
        );
        match engine.handle(&event) {
            CoreReply::Tiled(plan) => {
                let mut members: Vec<String> = plan
                    .geometry
                    .iter()
                    .map(|entry| entry.window.0.clone())
                    .collect();
                members.sort();
                assert_eq!(members, vec!["c1".to_string(), "c2".to_string()]);
            }
            other => panic!("min-only hydration shape must adopt, got {other:?}"),
        }
        let session = engine.session(&key).expect("hydrated domain retained");
        for id in ["f1", "f2", "f3"] {
            assert!(
                session.is_exception(&WindowId(id.to_owned())),
                "staged float {id} converges as an exception"
            );
        }
        for id in ["c1", "c2"] {
            assert!(
                !session.is_exception(&WindowId(id.to_owned())),
                "control {id} stays tiled"
            );
        }
        let convergence = engine.last_convergence().expect("converged hydration");
        assert_eq!(convergence.op, "reconcile");
        assert_eq!(convergence.admitted, 2);
        assert_eq!(convergence.flags_adopted, 3);
        let fit = engine.last_adoption_fit().expect("fit decision recorded");
        assert_eq!(fit.outcome, "fallback");
    }

    #[test]
    fn restart_hydration_fixed_anchor_reports_fallthrough_evidence() {
        use crate::boundary::CoreReply;
        use crate::directional::WindowId;
        // Same recorded shape, but the focused control pins
        // minimum == maximum (`[1157,789]` read as a fixed-size hint), so
        // convergence retains it as an automatic float exception. The
        // suspected path is the converge-then-seed fall-through: the
        // floating-aware build stores the session, the tiled projection
        // misses the exception anchor, and the legacy seed re-admits the
        // already-converged anchor as `duplicate-window`.
        let control = crate::size_hints::WindowSizeHints {
            min_w: Some(1157),
            min_h: Some(789),
            max_w: Some(1157),
            max_h: Some(789),
        };
        let other = crate::size_hints::WindowSizeHints {
            min_w: Some(401),
            min_h: Some(246),
            ..crate::size_hints::WindowSizeHints::none()
        };
        let owner = OwnerId::parse("owner-a").expect("valid");
        let gen_id = GenerationId::parse("gen-1").expect("valid");
        let mut engine = Engine::new();
        engine.sync_binding(&owner, &gen_id);
        engine.set_fixed_size_admission(true);
        let domain = restart_hydration_domain();
        let key = domain.key();
        let event = restart_hydration_event(
            &owner,
            &gen_id,
            domain,
            restart_hydration_windows(control, other),
        );
        match engine.handle(&event) {
            CoreReply::Tiled(plan) => {
                // The focused fixed anchor converges as an automatic
                // float; the tiled remainder (c2) still projects even
                // though the converged session holds no tiled focus (a
                // floated anchor never takes it). Fixed anchors keep
                // the float-free remainder shape (no capacity float
                // target).
                assert_eq!(plan.geometry.len(), 1);
                assert_eq!(plan.geometry[0].window.0, "c2");
                assert_eq!(plan.focus_domain, None);
                assert_eq!(plan.focus_leaf, None);
                assert_eq!(plan.float_window, None);
                assert_eq!(plan.float_rect, None);
            }
            other => panic!("fixed-anchor hydration shape must admit mixed roster, got {other:?}"),
        }
        // Atomic serial-seed evidence, pinned to the exact branch: the
        // fit declines for the fixed anchor (`fit_excluded`), then one
        // convergence commits a tiled control plus four exceptions (the
        // focused anchor becomes an automatic float, so the converged
        // session holds no tiled focus). No legacy seed fall-through
        // re-admits the converged anchor as `duplicate-window`. The
        // converged session stays stored, so the next observation
        // re-adopts from it (tick 2).
        let convergence = engine.last_convergence().expect("converged before seed");
        assert_eq!(convergence.op, "reconcile");
        assert_eq!(
            (
                convergence.removed,
                convergence.admitted,
                convergence.flags_adopted
            ),
            (0, 1, 4)
        );
        let fit = engine.last_adoption_fit().expect("fit decision recorded");
        assert_eq!(
            (fit.outcome, fit.windows, fit.reason),
            ("fallback", 5, "fit_excluded")
        );
        let fixed = engine
            .last_fixed_admission()
            .expect("fixed decision recorded");
        assert_eq!(
            (fixed.op, fixed.evaluated, fixed.admitted, fixed.reason),
            ("reconcile", 1, 1, "fixed-equal")
        );
        let session = engine.session(&key).expect("converged session retained");
        assert_eq!(session.accepted_revision(), 1);
        let tiled: Vec<String> = session
            .snapshot()
            .windows
            .iter()
            .map(|link| link.window.0.clone())
            .collect();
        assert_eq!(tiled, vec!["c2".to_string()]);
        for id in ["f1", "f2", "f3", "c1"] {
            assert!(
                session.is_exception(&WindowId(id.to_owned())),
                "{id} converges as an exception"
            );
        }
        assert!(
            !session.is_exception(&WindowId("c2".to_owned())),
            "c2 stays tiled"
        );
        assert_eq!(session.focus(), (None, None));
    }

    #[test]
    fn fresh_reconcile_prefers_relocation_over_seeding() {
        use crate::boundary::{CoreCommand, CoreEvent, CoreReply};
        use crate::ids::CorrelationId;
        // A unique same-workspace source relocates exactly like the retained
        // path instead of seeding a second session.
        let owner = OwnerId::parse("owner-a").expect("valid");
        let gen_id = GenerationId::parse("gen-1").expect("valid");
        let mut engine = Engine::new();
        engine.sync_binding(&owner, &gen_id);
        let source_domain = domain("out", "ws");
        let source_key = source_domain.key();
        let order = vec![EngineWindow {
            window: WindowId("win-1".to_owned()),
            output: OutputId("out".to_owned()),
            workspace: WorkspaceId("ws".to_owned()),
            rect: Rect {
                x: 0,
                y: 0,
                w: 10,
                h: 10,
            },
            floating: false,
            fit_excluded: false,
            fullscreen: false,
            maximized: false,
            sticky: false,
            fixed_auto: false,
            fixed_suppress: false,
            hints: crate::size_hints::WindowSizeHints::none(),
        }];
        let seeded =
            crate::seed::seed_session(&owner, &gen_id, 7, &source_domain, &order).expect("seeds");
        engine.store_committed(source_key.clone(), seeded, 3);
        let target_domain = domain("out-2", "ws");
        let target_key = target_domain.key();
        let event = CoreEvent {
            owner: owner.clone(),
            generation: gen_id.clone(),
            correlation: CorrelationId::parse("corr-rec-reloc").expect("valid"),
            revision: 0,
            fingerprint: 7,
            domain: target_domain,
            domain_key: target_key.clone(),
            outer_gap: 3,
            focused_window: WindowId("win-1".to_owned()),
            windows: vec![EngineWindow {
                window: WindowId("win-1".to_owned()),
                output: OutputId("out-2".to_owned()),
                workspace: WorkspaceId("ws".to_owned()),
                rect: Rect {
                    x: 0,
                    y: 0,
                    w: 10,
                    h: 10,
                },
                floating: false,
                fit_excluded: false,
                fullscreen: false,
                maximized: false,
                sticky: false,
                fixed_auto: false,
                fixed_suppress: false,
                hints: crate::size_hints::WindowSizeHints::none(),
            }],
            directional: None,
            directional_target_outer_gap: None,
            target_domain: None,
            target_windows: vec![],
            closed_windows: vec![],
            command: CoreCommand::Reconcile,
        };
        match engine.handle(&event) {
            CoreReply::Projection(plan) => assert_eq!(plan.geometry.len(), 1),
            other => panic!("relocated fresh reconcile must project, got {other:?}"),
        }
        assert!(!engine.contains(&source_key), "source moved");
        assert!(engine.contains(&target_key), "target retained");
    }

    #[test]
    fn fresh_reconcile_disjoint_unique_source_seeds_separate_domain() {
        use crate::boundary::{CoreCommand, CoreEvent, CoreReply};
        use crate::ids::CorrelationId;
        // Unique same-workspace source with disjoint observed windows must
        // not relocate: the fresh domain seeds separately and the source
        // stays untouched. Overlap relocation exact-set/rollback stays
        // covered by `fresh_reconcile_prefers_relocation_over_seeding` and
        // `relocated_target_skew_rejects_partial_observation_with_rollback`.
        let owner = OwnerId::parse("owner-a").expect("valid");
        let gen_id = GenerationId::parse("gen-1").expect("valid");
        let mut engine = Engine::new();
        engine.sync_binding(&owner, &gen_id);
        let source_domain = domain("out", "ws");
        let source_key = source_domain.key();
        let order = vec![EngineWindow {
            window: WindowId("win-1".to_owned()),
            output: OutputId("out".to_owned()),
            workspace: WorkspaceId("ws".to_owned()),
            rect: Rect {
                x: 0,
                y: 0,
                w: 10,
                h: 10,
            },
            floating: false,
            fit_excluded: false,
            fullscreen: false,
            maximized: false,
            sticky: false,
            fixed_auto: false,
            fixed_suppress: false,
            hints: crate::size_hints::WindowSizeHints::none(),
        }];
        let seeded =
            crate::seed::seed_session(&owner, &gen_id, 7, &source_domain, &order).expect("seeds");
        let source_revision = seeded.accepted_revision();
        engine.store_committed(source_key.clone(), seeded, 3);
        let target_domain = domain("out-2", "ws");
        let target_key = target_domain.key();
        let event = CoreEvent {
            owner: owner.clone(),
            generation: gen_id.clone(),
            correlation: CorrelationId::parse("corr-rec-disjoint").expect("valid"),
            revision: 0,
            fingerprint: 7,
            domain: target_domain,
            domain_key: target_key.clone(),
            outer_gap: 3,
            focused_window: WindowId("win-9".to_owned()),
            windows: vec![EngineWindow {
                window: WindowId("win-9".to_owned()),
                output: OutputId("out-2".to_owned()),
                workspace: WorkspaceId("ws".to_owned()),
                rect: Rect {
                    x: 0,
                    y: 0,
                    w: 10,
                    h: 10,
                },
                floating: false,
                fit_excluded: false,
                fullscreen: false,
                maximized: false,
                sticky: false,
                fixed_auto: false,
                fixed_suppress: false,
                hints: crate::size_hints::WindowSizeHints::none(),
            }],
            directional: None,
            directional_target_outer_gap: None,
            target_domain: None,
            target_windows: vec![],
            closed_windows: vec![],
            command: CoreCommand::Reconcile,
        };
        match engine.handle(&event) {
            CoreReply::Tiled(plan) => assert_eq!(plan.geometry.len(), 1),
            other => panic!("disjoint fresh reconcile must seed, got {other:?}"),
        }
        assert!(engine.contains(&target_key), "fresh target seeded");
        assert!(engine.contains(&source_key), "disjoint source preserved");
        let restored = engine.session(&source_key).expect("source kept");
        assert_eq!(restored.accepted_revision(), source_revision);
        assert_eq!(engine.retained_domains(), 2);
    }

    #[test]
    fn retained_reconcile_empty_retires_slot() {
        use crate::boundary::{CoreCommand, CoreEvent, CoreReply};
        use crate::ids::CorrelationId;
        let owner = OwnerId::parse("owner-a").expect("valid");
        let gen_id = GenerationId::parse("gen-1").expect("valid");
        let mut engine = Engine::new();
        engine.sync_binding(&owner, &gen_id);
        let d = domain("out", "ws");
        let key = d.key();
        // Genuinely empty slot-free session: omitting tiled windows now
        // stores D23 minimize slots and retains the session (covered by
        // the MNZ-03 engine regression), so the retire path is set up
        // with a session that never held members.
        let session = new_session(&owner, &gen_id, d.clone());
        let pre = session.accepted_revision();
        engine.insert_raw(key.clone(), session, 0);
        let event = CoreEvent {
            owner: owner.clone(),
            generation: gen_id.clone(),
            correlation: CorrelationId::parse("corr-rec-retire").expect("valid"),
            revision: 0,
            fingerprint: 7,
            domain: d,
            domain_key: key.clone(),
            outer_gap: 0,
            focused_window: WindowId(String::new()),
            windows: vec![],
            directional: None,
            directional_target_outer_gap: None,
            target_domain: None,
            target_windows: vec![],
            closed_windows: vec![],
            command: CoreCommand::Reconcile,
        };
        match engine.handle(&event) {
            CoreReply::Projection(plan) => {
                assert!(plan.geometry.is_empty());
                assert_eq!(
                    plan.base_revision, pre,
                    "retire reports the converged revision"
                );
            }
            other => panic!("retained empty reconcile must project empty, got {other:?}"),
        }
        assert!(!engine.contains(&key), "empty slot retires");
        assert_eq!(engine.outer_gap(&key), None);
    }

    #[test]
    fn retained_reconcile_empty_keeps_gap_fence_before_retire() {
        use crate::boundary::{CoreCommand, CoreEvent, CoreReply};
        use crate::ids::CorrelationId;
        // Gap mismatch still refuses before any retire: the slot survives.
        let owner = OwnerId::parse("owner-a").expect("valid");
        let gen_id = GenerationId::parse("gen-1").expect("valid");
        let mut engine = Engine::new();
        engine.sync_binding(&owner, &gen_id);
        let d = domain("out", "ws");
        let key = d.key();
        let order = vec![EngineWindow {
            window: WindowId("win-1".to_owned()),
            output: OutputId("out".to_owned()),
            workspace: WorkspaceId("ws".to_owned()),
            rect: Rect {
                x: 0,
                y: 0,
                w: 10,
                h: 10,
            },
            floating: false,
            fit_excluded: false,
            fullscreen: false,
            maximized: false,
            sticky: false,
            fixed_auto: false,
            fixed_suppress: false,
            hints: crate::size_hints::WindowSizeHints::none(),
        }];
        let seeded = crate::seed::seed_session(&owner, &gen_id, 7, &d, &order).expect("seeds");
        engine.store_committed(key.clone(), seeded, 0);
        let mut gapped = d.clone();
        gapped.gap = 8;
        let event = CoreEvent {
            owner: owner.clone(),
            generation: gen_id.clone(),
            correlation: CorrelationId::parse("corr-rec-gapfence").expect("valid"),
            revision: 0,
            fingerprint: 7,
            domain: gapped,
            domain_key: key.clone(),
            outer_gap: 0,
            focused_window: WindowId(String::new()),
            windows: vec![],
            directional: None,
            directional_target_outer_gap: None,
            target_domain: None,
            target_windows: vec![],
            closed_windows: vec![],
            command: CoreCommand::Reconcile,
        };
        match engine.handle(&event) {
            CoreReply::Rejected { kind, .. } => assert_eq!(kind, "domain-mismatch"),
            other => panic!("gap-skewed empty reconcile must reject, got {other:?}"),
        }
        assert!(engine.contains(&key), "fenced slot survives");
    }

    #[test]
    fn retained_reconcile_empty_keeps_owner_fence_before_retire() {
        use crate::boundary::{CoreCommand, CoreEvent, CoreReply};
        use crate::ids::CorrelationId;
        let owner = OwnerId::parse("owner-a").expect("valid");
        let gen_id = GenerationId::parse("gen-1").expect("valid");
        let mut engine = Engine::new();
        engine.sync_binding(&owner, &gen_id);
        let d = domain("out", "ws");
        let key = d.key();
        let order = vec![EngineWindow {
            window: WindowId("win-1".to_owned()),
            output: OutputId("out".to_owned()),
            workspace: WorkspaceId("ws".to_owned()),
            rect: Rect {
                x: 0,
                y: 0,
                w: 10,
                h: 10,
            },
            floating: false,
            fit_excluded: false,
            fullscreen: false,
            maximized: false,
            sticky: false,
            fixed_auto: false,
            fixed_suppress: false,
            hints: crate::size_hints::WindowSizeHints::none(),
        }];
        let seeded = crate::seed::seed_session(&owner, &gen_id, 7, &d, &order).expect("seeds");
        engine.store_committed(key.clone(), seeded, 0);
        let event = CoreEvent {
            owner: OwnerId::parse("owner-b").expect("valid"),
            generation: gen_id.clone(),
            correlation: CorrelationId::parse("corr-rec-owner").expect("valid"),
            revision: 0,
            fingerprint: 7,
            domain: d,
            domain_key: key.clone(),
            outer_gap: 0,
            focused_window: WindowId(String::new()),
            windows: vec![],
            directional: None,
            directional_target_outer_gap: None,
            target_domain: None,
            target_windows: vec![],
            closed_windows: vec![],
            command: CoreCommand::Reconcile,
        };
        match engine.handle(&event) {
            CoreReply::Diverged(_) => {}
            other => panic!("foreign-owner empty reconcile must diverge, got {other:?}"),
        }
        assert!(engine.contains(&key), "diverged slot survives");
    }

    #[test]
    fn update_gaps_malformed_tiled_links_still_reject() {
        use crate::boundary::{CoreCommand, CoreEvent, CoreReply};
        use crate::ids::CorrelationId;
        // A tiled observation homed to the domain while the converged tree
        // is missing cannot happen through convergence, so this pins the
        // fail-closed shape indirectly: a tiled member observed under an
        // unknown domain refuses instead of seeding through update-gaps.
        let owner = OwnerId::parse("owner-a").expect("valid");
        let gen_id = GenerationId::parse("gen-1").expect("valid");
        let mut engine = Engine::new();
        engine.sync_binding(&owner, &gen_id);
        let d = domain("out", "ws");
        let key = d.key();
        let event = CoreEvent {
            owner: owner.clone(),
            generation: gen_id.clone(),
            correlation: CorrelationId::parse("corr-gap-unknown").expect("valid"),
            revision: 0,
            fingerprint: 7,
            domain: d,
            domain_key: key,
            outer_gap: 0,
            focused_window: WindowId("win-1".to_owned()),
            windows: vec![EngineWindow {
                window: WindowId("win-1".to_owned()),
                output: OutputId("out".to_owned()),
                workspace: WorkspaceId("ws".to_owned()),
                rect: Rect {
                    x: 0,
                    y: 0,
                    w: 10,
                    h: 10,
                },
                floating: false,
                fit_excluded: false,
                fullscreen: false,
                maximized: false,
                sticky: false,
                fixed_auto: false,
                fixed_suppress: false,
                hints: crate::size_hints::WindowSizeHints::none(),
            }],
            directional: None,
            directional_target_outer_gap: None,
            target_domain: None,
            target_windows: vec![],
            closed_windows: vec![],
            command: CoreCommand::UpdateGaps,
        };
        match engine.handle(&event) {
            CoreReply::Rejected { kind, .. } => assert_eq!(kind, "unknown-domain"),
            other => panic!("fresh tiled update-gaps must refuse, got {other:?}"),
        }
        assert_eq!(engine.retained_domains(), 0, "refused update never seeds");
    }

    #[test]
    fn fresh_reconcile_seeds_despite_ambiguous_candidates() {
        use crate::boundary::{CoreCommand, CoreEvent, CoreReply};
        use crate::ids::CorrelationId;
        // Multiple same-workspace other-output sessions make relocation
        // ambiguous, so no source moves; with no pending, the absent domain
        // still seeds fresh (same as the admit path) while both candidates
        // stay untouched.
        let owner = OwnerId::parse("owner-a").expect("valid");
        let gen_id = GenerationId::parse("gen-1").expect("valid");
        let mut engine = Engine::new();
        engine.sync_binding(&owner, &gen_id);
        for (output, window) in [("out-a", "win-a"), ("out-b", "win-b")] {
            let d = domain(output, "ws-x");
            let key = d.key();
            let order = vec![EngineWindow {
                window: WindowId(window.to_owned()),
                output: OutputId(output.to_owned()),
                workspace: WorkspaceId("ws-x".to_owned()),
                rect: Rect {
                    x: 0,
                    y: 0,
                    w: 10,
                    h: 10,
                },
                floating: false,
                fit_excluded: false,
                fullscreen: false,
                maximized: false,
                sticky: false,
                fixed_auto: false,
                fixed_suppress: false,
                hints: crate::size_hints::WindowSizeHints::none(),
            }];
            let seeded = crate::seed::seed_session(&owner, &gen_id, 7, &d, &order).expect("seeds");
            engine.store_committed(key, seeded, 0);
        }
        assert_eq!(engine.retained_domains(), 2);
        let target = domain("out-c", "ws-x");
        let target_key = target.key();
        let event = CoreEvent {
            owner: owner.clone(),
            generation: gen_id.clone(),
            correlation: CorrelationId::parse("corr-rec-amb").expect("valid"),
            revision: 0,
            fingerprint: 7,
            domain: target,
            domain_key: target_key.clone(),
            outer_gap: 0,
            focused_window: WindowId("win-a".to_owned()),
            windows: vec![EngineWindow {
                window: WindowId("win-a".to_owned()),
                output: OutputId("out-c".to_owned()),
                workspace: WorkspaceId("ws-x".to_owned()),
                rect: Rect {
                    x: 0,
                    y: 0,
                    w: 10,
                    h: 10,
                },
                floating: false,
                fit_excluded: false,
                fullscreen: false,
                maximized: false,
                sticky: false,
                fixed_auto: false,
                fixed_suppress: false,
                hints: crate::size_hints::WindowSizeHints::none(),
            }],
            directional: None,
            directional_target_outer_gap: None,
            target_domain: None,
            target_windows: vec![],
            closed_windows: vec![],
            command: CoreCommand::Reconcile,
        };
        match engine.handle(&event) {
            CoreReply::Tiled(plan) => assert_eq!(plan.geometry.len(), 1),
            other => panic!("ambiguous fresh reconcile must seed, got {other:?}"),
        }
        assert!(engine.contains(&target_key), "fresh target seeded");
        assert_eq!(engine.retained_domains(), 3, "candidates untouched");
    }

    #[test]
    fn canonical_pair_helpers_stay_typed() {
        let mut anchored = domain("out", "ws");
        anchored.adjacent = BTreeMap::from([(
            crate::directional::Direction::Right,
            crate::directional::OutputId("out-2".to_owned()),
        )]);
        let stripped = Engine::canonical_component_domain(&anchored);
        assert!(stripped.adjacent.is_empty());
        assert_eq!(stripped.bounds, anchored.bounds);
        let engine = Engine::new();
        let target = domain("out-2", "ws");
        assert_eq!(
            engine
                .assemble_directional_pair(&anchored, &anchored.key(), &target, &target.key())
                .unwrap_err(),
            "canonical-source-unavailable"
        );
    }

    #[test]
    fn reproject_updates_bounds_only() {
        let mut engine = Engine::new();
        let owner = OwnerId::parse("owner-a").expect("valid");
        let gen_id = GenerationId::parse("gen-1").expect("valid");
        engine.sync_binding(&owner, &gen_id);
        let d = domain("out", "ws");
        let key = d.key();
        engine.insert_raw(key.clone(), new_session(&owner, &gen_id, d), 0);
        let next = Rect {
            x: 0,
            y: 0,
            w: 1024,
            h: 768,
        };
        assert!(engine.reproject_retained(&key, next));
        assert_eq!(
            engine.session(&key).expect("kept").domains()[0].bounds,
            next
        );
        assert!(!engine.reproject_retained(&domain("missing", "ws").key(), next));
    }

    fn engine_window(window: &str, output: &str, workspace: &str) -> crate::seed::EngineWindow {
        crate::seed::EngineWindow {
            window: WindowId(window.to_owned()),
            output: OutputId(output.to_owned()),
            workspace: WorkspaceId(workspace.to_owned()),
            rect: Rect {
                x: 0,
                y: 0,
                w: 10,
                h: 10,
            },
            floating: false,
            fit_excluded: false,
            fullscreen: false,
            maximized: false,
            sticky: false,
            fixed_auto: false,
            fixed_suppress: false,
            hints: crate::size_hints::WindowSizeHints::none(),
        }
    }

    fn drag_event(
        owner: &OwnerId,
        gen_id: &GenerationId,
        domain: &OutputDomain,
        focused: &str,
        windows: Vec<crate::seed::EngineWindow>,
        command: crate::boundary::CoreCommand,
        correlation: &str,
    ) -> crate::boundary::CoreEvent {
        crate::boundary::CoreEvent {
            owner: owner.clone(),
            generation: gen_id.clone(),
            correlation: crate::ids::CorrelationId::parse(correlation).expect("valid"),
            revision: 0,
            fingerprint: 7,
            domain: domain.clone(),
            domain_key: domain.key(),
            outer_gap: 0,
            focused_window: WindowId(focused.to_owned()),
            windows,
            directional: None,
            directional_target_outer_gap: None,
            target_domain: None,
            target_windows: vec![],
            closed_windows: vec![],
            command,
        }
    }

    fn cross_source(output: &str, workspace: &str) -> DomainKey {
        DomainKey {
            output: OutputId(output.to_owned()),
            workspace: WorkspaceId(workspace.to_owned()),
        }
    }

    fn hinted_window(
        window: &str,
        output: &str,
        workspace: &str,
        min_w: Option<i32>,
    ) -> crate::seed::EngineWindow {
        let mut entry = engine_window(window, output, workspace);
        entry.hints = crate::size_hints::WindowSizeHints {
            min_w,
            min_h: None,
            max_w: None,
            max_h: None,
        };
        entry
    }

    #[test]
    fn drag_empty_singleton_cross_output_admits_without_relocating_source() {
        use crate::boundary::{CoreCommand, CoreEvent, CoreReply};
        use crate::ids::CorrelationId;
        let mut engine = Engine::new();
        let owner = OwnerId::parse("owner-a").expect("valid");
        let gen_id = GenerationId::parse("gen-1").expect("valid");
        engine.sync_binding(&owner, &gen_id);
        let source_domain = domain("out-1", "ws-1");
        let source_key = source_domain.key();
        let source = crate::seed::seed_session(
            &owner,
            &gen_id,
            7,
            &source_domain,
            &[
                engine_window("win-a", "out-1", "ws-1"),
                engine_window("win-b", "out-1", "ws-1"),
            ],
        )
        .expect("seeds source");
        engine.store_committed(source_key.clone(), source, 0);
        let dest_domain = domain("out-2", "ws-1");
        let dest_key = dest_domain.key();
        let event = |command, correlation: &str| CoreEvent {
            owner: owner.clone(),
            generation: gen_id.clone(),
            correlation: CorrelationId::parse(correlation).expect("valid"),
            revision: 0,
            fingerprint: 7,
            domain: dest_domain.clone(),
            domain_key: dest_key.clone(),
            outer_gap: 0,
            focused_window: WindowId("win-a".to_owned()),
            windows: vec![engine_window("win-a", "out-2", "ws-1")],
            directional: None,
            directional_target_outer_gap: None,
            target_domain: None,
            target_windows: vec![],
            closed_windows: vec![],
            command,
        };
        let same_drop = event(
            CoreCommand::DragDrop {
                window: "win-a".to_owned(),
                x: 400,
                y: 300,
                hover_prior: None,
                source: None,
            },
            "corr-drag-empty-same",
        );
        match engine.handle(&same_drop) {
            CoreReply::Rejected { kind, .. } => assert_eq!(kind, "unchanged"),
            other => panic!("same-output singleton must snap back, got {other:?}"),
        }
        assert!(!engine.contains(&dest_key));
        let same_preview = event(
            CoreCommand::DragPreview {
                window: "win-a".to_owned(),
                x: 400,
                y: 300,
                hover_prior: None,
                source: None,
            },
            "corr-drag-empty-pv-same",
        );
        match engine.handle(&same_preview) {
            CoreReply::Rejected { kind, .. } => assert_eq!(kind, "unchanged"),
            other => panic!("same-output preview must snap back, got {other:?}"),
        }
        let preview = event(
            CoreCommand::DragPreview {
                window: "win-a".to_owned(),
                x: 400,
                y: 300,
                hover_prior: None,
                source: Some(cross_source("out-1", "ws-1")),
            },
            "corr-drag-empty-pv",
        );
        let rect = match engine.handle(&preview) {
            CoreReply::DragPreview(plan) => plan.preview.proposed_rect,
            other => panic!("cross-output preview must project, got {other:?}"),
        };
        assert!(!engine.contains(&dest_key), "preview must not store");
        let drop = event(
            CoreCommand::DragDrop {
                window: "win-a".to_owned(),
                x: 400,
                y: 300,
                hover_prior: None,
                source: Some(cross_source("out-1", "ws-1")),
            },
            "corr-drag-empty",
        );
        match engine.handle(&drop) {
            CoreReply::Tiled(plan) => {
                assert_eq!(plan.kind, crate::boundary::TiledKind::DragDrop);
                assert_eq!(plan.geometry.len(), 1);
                assert_eq!(plan.geometry[0].rect, rect);
                assert_eq!(plan.geometry[0].rect, dest_domain.bounds);
            }
            other => panic!("empty drop must admit singleton, got {other:?}"),
        }
        assert!(engine.contains(&dest_key));
        let kept = engine.session(&source_key).expect("source kept");
        assert!(
            kept.snapshot()
                .windows
                .iter()
                .any(|l| l.window.0 == "win-a")
        );
        assert!(
            kept.snapshot()
                .windows
                .iter()
                .any(|l| l.window.0 == "win-b")
        );
    }

    /// Two-output drag world with deliberately different arrangements: the
    /// source is a small positive-coordinate monitor, the destination a
    /// larger negative-x monitor (no adjacency derivation: the pointer output
    /// decides, so sideways, stacked, and negative layouts all behave alike).
    fn cross_drag_domain(output: &str, workspace: &str, bounds: Rect) -> OutputDomain {
        OutputDomain {
            id: OutputId(output.to_owned()),
            workspace: WorkspaceId(workspace.to_owned()),
            bounds,
            gap: 0,
            adjacent: BTreeMap::new(),
        }
    }

    fn cross_drag_world(
        source_windows: &[&str],
        dest_windows: &[&str],
    ) -> (Engine, OwnerId, GenerationId, OutputDomain, OutputDomain) {
        let mut engine = Engine::new();
        let owner = OwnerId::parse("owner-a").expect("valid");
        let gen_id = GenerationId::parse("gen-1").expect("valid");
        engine.sync_binding(&owner, &gen_id);
        let source_domain = cross_drag_domain(
            "out-1",
            "ws-1",
            Rect {
                x: 0,
                y: 0,
                w: 800,
                h: 600,
            },
        );
        let dest_domain = cross_drag_domain(
            "out-2",
            "ws-1",
            Rect {
                x: -1920,
                y: -200,
                w: 1920,
                h: 1080,
            },
        );
        let source_seed: Vec<crate::seed::EngineWindow> = source_windows
            .iter()
            .map(|name| engine_window(name, "out-1", "ws-1"))
            .collect();
        let source = crate::seed::seed_session(&owner, &gen_id, 7, &source_domain, &source_seed)
            .expect("seeds source");
        engine.store_committed(source_domain.key(), source, 0);
        let dest_seed: Vec<crate::seed::EngineWindow> = dest_windows
            .iter()
            .map(|name| engine_window(name, "out-2", "ws-1"))
            .collect();
        let dest = crate::seed::seed_session(&owner, &gen_id, 7, &dest_domain, &dest_seed)
            .expect("seeds dest");
        engine.store_committed(dest_domain.key(), dest, 0);
        (engine, owner, gen_id, source_domain, dest_domain)
    }

    /// Cross-output pair event: the pointer output's current domain carries
    /// the request, the combined both-domain rows ride `windows`, and the
    /// source-first pair (no adjacency entries: pointer-resolved, so the
    /// send-style assembly applies) scopes the transaction.
    #[allow(clippy::too_many_arguments)]
    fn cross_drag_event(
        owner: &OwnerId,
        gen_id: &GenerationId,
        source_domain: &OutputDomain,
        dest_domain: &OutputDomain,
        focused: &str,
        windows: Vec<crate::seed::EngineWindow>,
        command: crate::boundary::CoreCommand,
        correlation: &str,
    ) -> crate::boundary::CoreEvent {
        crate::boundary::CoreEvent {
            owner: owner.clone(),
            generation: gen_id.clone(),
            correlation: crate::ids::CorrelationId::parse(correlation).expect("valid"),
            revision: 0,
            fingerprint: 7,
            domain: dest_domain.clone(),
            domain_key: dest_domain.key(),
            outer_gap: 0,
            focused_window: WindowId(focused.to_owned()),
            windows,
            directional: Some(vec![
                (source_domain.clone(), source_domain.key()),
                (dest_domain.clone(), dest_domain.key()),
            ]),
            directional_target_outer_gap: Some(0),
            target_domain: None,
            target_windows: vec![],
            closed_windows: vec![],
            command,
        }
    }

    fn cross_drag_rows() -> Vec<crate::seed::EngineWindow> {
        vec![
            engine_window("win-a", "out-1", "ws-1"),
            engine_window("win-b", "out-1", "ws-1"),
            engine_window("win-c", "out-2", "ws-1"),
        ]
    }

    #[test]
    fn drag_cross_output_occupied_drop_relocates_source_to_pointer_edge() {
        use crate::boundary::{CoreCommand, CoreReply};
        let (mut engine, owner, gen_id, source_domain, dest_domain) =
            cross_drag_world(&["win-a", "win-b"], &["win-c"]);
        let source_key = source_domain.key();
        let dest_key = dest_domain.key();
        // Destination holds sole win-c over (-1920,-200,1920,1080): x=-1900
        // sits in the left edge zone, so win-b inserts before win-c.
        let preview = cross_drag_event(
            &owner,
            &gen_id,
            &source_domain,
            &dest_domain,
            "win-b",
            cross_drag_rows(),
            CoreCommand::DragPreview {
                window: "win-b".to_owned(),
                x: -1900,
                y: 400,
                hover_prior: None,
                source: Some(cross_source("out-1", "ws-1")),
            },
            "corr-drag-x-pv",
        );
        let previewed = match engine.handle(&preview) {
            CoreReply::DragPreview(plan) => plan.preview,
            other => panic!("cross-output preview must project, got {other:?}"),
        };
        assert_eq!(previewed.domain, dest_key);
        assert_eq!(previewed.source_window.0, "win-b");
        assert_eq!(previewed.target_window.0, "win-c");
        // Preview commits nothing on either domain.
        assert_eq!(
            engine
                .session(&source_key)
                .expect("source kept")
                .snapshot()
                .windows
                .len(),
            2
        );
        let drop = cross_drag_event(
            &owner,
            &gen_id,
            &source_domain,
            &dest_domain,
            "win-b",
            cross_drag_rows(),
            CoreCommand::DragDrop {
                window: "win-b".to_owned(),
                x: -1900,
                y: 400,
                hover_prior: None,
                source: Some(cross_source("out-1", "ws-1")),
            },
            "corr-drag-x",
        );
        let mover_rect = match engine.handle(&drop) {
            CoreReply::Tiled(plan) => {
                assert_eq!(plan.kind, crate::boundary::TiledKind::DragDrop);
                assert_eq!(plan.geometry.len(), 3);
                assert_eq!(plan.focus_domain, Some(dest_key.clone()));
                // Focus stays on the moved window in the destination domain.
                let mover_leaf = engine
                    .session(&dest_key)
                    .expect("dest kept")
                    .snapshot()
                    .windows
                    .iter()
                    .find(|l| l.window.0 == "win-b")
                    .expect("mover link")
                    .leaf
                    .clone();
                assert_eq!(plan.focus_leaf, Some(mover_leaf));
                plan.geometry
                    .iter()
                    .find(|g| g.window.0 == "win-b")
                    .expect("mover geometry")
                    .rect
            }
            other => panic!("cross-output drop must plan, got {other:?}"),
        };
        assert_eq!(mover_rect, previewed.proposed_rect);
        // Left half of the negative-x destination, in physical coordinates.
        assert_eq!(
            mover_rect,
            Rect {
                x: -1920,
                y: -200,
                w: 960,
                h: 1080
            }
        );
        // Source membership removed (sole win-a reflows full source);
        // destination holds the arrival beside win-c.
        let source_windows: Vec<String> = engine
            .session(&source_key)
            .expect("source kept")
            .snapshot()
            .windows
            .iter()
            .map(|l| l.window.0.clone())
            .collect();
        assert_eq!(source_windows, vec!["win-a".to_owned()]);
        let mut dest_windows: Vec<String> = engine
            .session(&dest_key)
            .expect("dest kept")
            .snapshot()
            .windows
            .iter()
            .map(|l| l.window.0.clone())
            .collect();
        dest_windows.sort();
        assert_eq!(dest_windows, vec!["win-b".to_owned(), "win-c".to_owned()]);
    }

    #[test]
    fn drag_cross_output_wrap_and_sole_source() {
        use crate::boundary::{CoreCommand, CoreReply};
        // Sole source mover plus a sole destination tile; a top-edge drop
        // wraps the destination root with the mover first.
        let (mut engine, owner, gen_id, source_domain, dest_domain) =
            cross_drag_world(&["win-b"], &["win-c"]);
        let source_key = source_domain.key();
        let dest_key = dest_domain.key();
        let rows = vec![
            engine_window("win-b", "out-1", "ws-1"),
            engine_window("win-c", "out-2", "ws-1"),
        ];
        let drop = cross_drag_event(
            &owner,
            &gen_id,
            &source_domain,
            &dest_domain,
            "win-b",
            rows,
            CoreCommand::DragDrop {
                window: "win-b".to_owned(),
                x: -960,
                y: -190,
                hover_prior: None,
                source: Some(cross_source("out-1", "ws-1")),
            },
            "corr-drag-wrap",
        );
        let mover_rect = match engine.handle(&drop) {
            CoreReply::Tiled(plan) => {
                assert_eq!(plan.geometry.len(), 2);
                plan.geometry
                    .iter()
                    .find(|g| g.window.0 == "win-b")
                    .expect("mover geometry")
                    .rect
            }
            other => panic!("cross-output wrap drop must plan, got {other:?}"),
        };
        // Top half of the destination: the mover wrapped the root first.
        assert_eq!(
            mover_rect,
            Rect {
                x: -1920,
                y: -200,
                w: 1920,
                h: 540
            }
        );
        // The emptied source retires its session: no membership remains.
        assert!(engine.session(&source_key).is_none());
        let mut dest_windows: Vec<String> = engine
            .session(&dest_key)
            .expect("dest kept")
            .snapshot()
            .windows
            .iter()
            .map(|l| l.window.0.clone())
            .collect();
        dest_windows.sort();
        assert_eq!(dest_windows, vec!["win-b".to_owned(), "win-c".to_owned()]);
    }

    #[test]
    fn drag_cross_output_stale_and_misrouted_refusals_commit_no_drop() {
        use crate::boundary::{CoreCommand, CoreReply};
        let (mut engine, owner, gen_id, source_domain, dest_domain) =
            cross_drag_world(&["win-a", "win-b"], &["win-c"]);
        let source_key = source_domain.key();
        let dest_key = dest_domain.key();
        // Empty destination misrouted into the occupied pair path: no
        // retained session exists for a fabricated key, so the pair cannot
        // assemble. (The adapter routes genuine empty destinations through
        // singleton admission plus source reconcile instead.)
        let missing_domain = cross_drag_domain(
            "out-9",
            "ws-9",
            Rect {
                x: 0,
                y: 0,
                w: 800,
                h: 600,
            },
        );
        let misrouted = cross_drag_event(
            &owner,
            &gen_id,
            &source_domain,
            &missing_domain,
            "win-b",
            cross_drag_rows(),
            CoreCommand::DragDrop {
                window: "win-b".to_owned(),
                x: 100,
                y: 100,
                hover_prior: None,
                source: Some(cross_source("out-1", "ws-1")),
            },
            "corr-drag-missing",
        );
        match engine.handle(&misrouted) {
            CoreReply::SnapshotInvalid { detail, .. } => {
                assert_eq!(detail, "canonical-target-unavailable")
            }
            other => panic!("missing destination must refuse, got {other:?}"),
        }
        // Swapped pair keys disagree with the request addressing: refused
        // before any Engine mutation.
        let mut swapped = cross_drag_event(
            &owner,
            &gen_id,
            &source_domain,
            &dest_domain,
            "win-b",
            cross_drag_rows(),
            CoreCommand::DragDrop {
                window: "win-b".to_owned(),
                x: -1900,
                y: 400,
                hover_prior: None,
                source: Some(cross_source("out-1", "ws-1")),
            },
            "corr-drag-swapped",
        );
        swapped.directional = Some(vec![
            (dest_domain.clone(), dest_domain.key()),
            (source_domain.clone(), source_domain.key()),
        ]);
        match engine.handle(&swapped) {
            CoreReply::SnapshotInvalid { detail, .. } => assert_eq!(detail, "domain-invalid"),
            other => panic!("swapped pair must refuse, got {other:?}"),
        }
        // Mover absent from the combined observation: the source converges
        // the departure, then the capture binds nothing and the drop refuses
        // without a plan.
        let mut stale_rows = cross_drag_rows();
        stale_rows.retain(|w| w.window.0 != "win-b");
        let stale = cross_drag_event(
            &owner,
            &gen_id,
            &source_domain,
            &dest_domain,
            "win-b",
            stale_rows,
            CoreCommand::DragDrop {
                window: "win-b".to_owned(),
                x: -1900,
                y: 400,
                hover_prior: None,
                source: Some(cross_source("out-1", "ws-1")),
            },
            "corr-drag-stale",
        );
        match engine.handle(&stale) {
            CoreReply::Rejected { kind, .. } => assert_eq!(kind, "unknown-window"),
            other => panic!("mover-less drop must refuse, got {other:?}"),
        }
        // No drop committed anywhere above: the destination still holds only
        // win-c and no drag plan reached either session.
        let dest_windows: Vec<String> = engine
            .session(&dest_key)
            .expect("dest kept")
            .snapshot()
            .windows
            .iter()
            .map(|l| l.window.0.clone())
            .collect();
        assert_eq!(dest_windows, vec!["win-c".to_owned()]);
        assert!(!engine.session(&source_key).expect("source kept").has_drag());
    }

    #[test]
    fn drag_retained_empty_arrival_preview_matches_drop() {
        use crate::boundary::{CoreCommand, CoreReply};
        let mut engine = Engine::new();
        let owner = OwnerId::parse("owner-a").expect("valid");
        let generation = GenerationId::parse("gen-1").expect("valid");
        engine.sync_binding(&owner, &generation);
        let dest = domain("out-2", "ws-1");
        let empty = Session::new(owner.clone(), generation.clone(), 0, 7, vec![dest.clone()])
            .expect("valid empty domain");
        engine.insert_raw(dest.key(), empty, 0);
        let event = |command| {
            drag_event(
                &owner,
                &generation,
                &dest,
                "win-a",
                vec![engine_window("win-a", "out-2", "ws-1")],
                command,
                "corr-empty",
            )
        };
        let preview = event(CoreCommand::DragPreview {
            window: "win-a".to_owned(),
            x: 400,
            y: 300,
            hover_prior: None,
            source: Some(cross_source("out-1", "ws-1")),
        });
        let rect = match engine.handle(&preview) {
            CoreReply::DragPreview(plan) => plan.preview.proposed_rect,
            other => panic!("retained empty preview must project arrival: {other:?}"),
        };
        assert!(
            engine
                .session(&dest.key())
                .expect("kept")
                .snapshot()
                .windows
                .is_empty()
        );
        let drop = event(CoreCommand::DragDrop {
            window: "win-a".to_owned(),
            x: 400,
            y: 300,
            hover_prior: None,
            source: Some(cross_source("out-1", "ws-1")),
        });
        match engine.handle(&drop) {
            CoreReply::Tiled(plan) => assert_eq!(plan.geometry[0].rect, rect),
            other => panic!("retained empty drop must match preview: {other:?}"),
        }
    }

    #[test]
    fn drag_preview_drop_parity_with_hints_and_prior() {
        use crate::boundary::{CoreCommand, CoreReply};
        let mut engine = Engine::new();
        let owner = OwnerId::parse("owner-a").expect("valid");
        let gen_id = GenerationId::parse("gen-1").expect("valid");
        engine.sync_binding(&owner, &gen_id);
        let dest_domain = domain("out-2", "ws-1");
        let dest_key = dest_domain.key();
        let seeded = crate::seed::seed_session(
            &owner,
            &gen_id,
            7,
            &dest_domain,
            std::slice::from_ref(&engine_window("win-a", "out-2", "ws-1")),
        )
        .expect("seeds retained destination");
        engine.store_committed(dest_key.clone(), seeded, 0);
        let windows = vec![
            hinted_window("win-a", "out-2", "ws-1", Some(200)),
            hinted_window("win-m", "out-2", "ws-1", Some(200)),
        ];
        let preview_event = drag_event(
            &owner,
            &gen_id,
            &dest_domain,
            "win-m",
            windows.clone(),
            CoreCommand::DragPreview {
                window: "win-m".to_owned(),
                x: 5,
                y: 300,
                hover_prior: None,
                source: Some(cross_source("out-1", "ws-1")),
            },
            "corr-drag-hint-pv",
        );
        let (proposed, prior) = match engine.handle(&preview_event) {
            CoreReply::DragPreview(plan) => {
                (plan.preview.proposed_rect, plan.preview.hover_prior())
            }
            other => panic!("hinted preview must project, got {other:?}"),
        };
        assert!(
            engine
                .session(&dest_key)
                .expect("retained")
                .snapshot()
                .windows
                .iter()
                .all(|l| l.window.0 != "win-m")
        );
        let preview_prior = drag_event(
            &owner,
            &gen_id,
            &dest_domain,
            "win-m",
            windows.clone(),
            CoreCommand::DragPreview {
                window: "win-m".to_owned(),
                x: 5,
                y: 300,
                hover_prior: Some(prior.clone()),
                source: Some(cross_source("out-1", "ws-1")),
            },
            "corr-drag-hint-pv-prior",
        );
        match engine.handle(&preview_prior) {
            CoreReply::DragPreview(_) => {}
            other => panic!("carried prior preview must project, got {other:?}"),
        }
        let drop_event = drag_event(
            &owner,
            &gen_id,
            &dest_domain,
            "win-m",
            windows,
            CoreCommand::DragDrop {
                window: "win-m".to_owned(),
                x: 5,
                y: 300,
                hover_prior: Some(prior),
                source: Some(cross_source("out-1", "ws-1")),
            },
            "corr-drag-hint-drop",
        );
        match engine.handle(&drop_event) {
            CoreReply::Tiled(plan) => {
                let moved = plan
                    .geometry
                    .iter()
                    .find(|g| g.window.0 == "win-m")
                    .expect("mover geometry");
                assert_eq!(moved.rect, proposed, "hinted drop must match preview");
            }
            other => panic!("hinted drop must plan, got {other:?}"),
        }
        assert!(
            engine
                .session(&dest_key)
                .expect("stored")
                .snapshot()
                .windows
                .iter()
                .any(|l| l.window.0 == "win-m")
        );
    }

    /// Single-domain preview stages: unambiguous replies record nothing;
    /// an out-of-area pointer and an off-homed row refuse with ambiguous
    /// kinds and distinct stages.
    #[test]
    fn drag_preview_stage_discriminates_single_domain() {
        use crate::boundary::{CoreCommand, CoreReply};
        fn stage_of(engine: &Engine) -> Option<&'static str> {
            engine.last_drag_preview_stage().map(|report| report.stage)
        }
        let mut engine = Engine::new();
        let owner = OwnerId::parse("owner-a").expect("valid");
        let gen_id = GenerationId::parse("gen-1").expect("valid");
        engine.sync_binding(&owner, &gen_id);
        let source_domain = domain("out-1", "ws-1");
        let source_key = source_domain.key();
        let source = crate::seed::seed_session(
            &owner,
            &gen_id,
            7,
            &source_domain,
            &[
                engine_window("win-a", "out-1", "ws-1"),
                engine_window("win-b", "out-1", "ws-1"),
            ],
        )
        .expect("seeds source");
        engine.store_committed(source_key, source, 0);
        let preview = |engine: &mut Engine,
                       windows: Vec<crate::seed::EngineWindow>,
                       x: i32,
                       y: i32,
                       correlation: &str| {
            let event = drag_event(
                &owner,
                &gen_id,
                &source_domain,
                "win-b",
                windows,
                CoreCommand::DragPreview {
                    window: "win-b".to_owned(),
                    x,
                    y,
                    hover_prior: None,
                    source: Some(cross_source("out-1", "ws-1")),
                },
                correlation,
            );
            engine.handle(&event)
        };
        let rows = || {
            vec![
                engine_window("win-a", "out-1", "ws-1"),
                engine_window("win-b", "out-1", "ws-1"),
            ]
        };
        match preview(&mut engine, rows(), 790, 300, "corr-sd-1") {
            CoreReply::Rejected { kind, .. } => assert_eq!(kind, "unchanged"),
            other => panic!("no-op point must refuse unchanged, got {other:?}"),
        }
        assert_eq!(stage_of(&engine), None);
        match preview(&mut engine, rows(), 7999, 300, "corr-sd-2") {
            CoreReply::Rejected { kind, .. } => assert_eq!(kind, "cross-domain-mismatch"),
            other => panic!("outside pointer must refuse, got {other:?}"),
        }
        assert_eq!(stage_of(&engine), Some("resolve-outside"));
        match preview(&mut engine, rows(), 10, 300, "corr-sd-3") {
            CoreReply::DragPreview(_) => {}
            other => panic!("live point must preview, got {other:?}"),
        }
        assert_eq!(stage_of(&engine), None);
        let off_homed = vec![
            engine_window("win-a", "out-1", "ws-1"),
            engine_window("win-b", "out-9", "ws-9"),
        ];
        match preview(&mut engine, off_homed, 10, 300, "corr-sd-4") {
            CoreReply::Rejected { kind, .. } => assert_eq!(kind, "cross-domain-mismatch"),
            other => panic!("off-homed row must refuse, got {other:?}"),
        }
        assert_eq!(stage_of(&engine), Some("converge"));
    }

    /// Occupied pair preview stages: green previews stay silent, while a
    /// flip-homed mover row and an out-of-area pointer refuse with distinct
    /// pair stages.
    #[test]
    fn drag_preview_pair_stages_cover_homing_and_resolution() {
        use crate::boundary::{CoreCommand, CoreReply};
        fn stage_of(engine: &Engine) -> Option<&'static str> {
            engine.last_drag_preview_stage().map(|report| report.stage)
        }
        // All rows homed on their retained domains: green preview, silent.
        let (mut engine, owner, gen_id, source_domain, dest_domain) =
            cross_drag_world(&["win-a", "win-b"], &["win-c"]);
        let green = cross_drag_event(
            &owner,
            &gen_id,
            &source_domain,
            &dest_domain,
            "win-b",
            cross_drag_rows(),
            CoreCommand::DragPreview {
                window: "win-b".to_owned(),
                x: -1900,
                y: 400,
                hover_prior: None,
                source: Some(cross_source("out-1", "ws-1")),
            },
            "corr-pair-green",
        );
        match engine.handle(&green) {
            CoreReply::DragPreview(_) => {}
            other => panic!("all-homed pair preview stays green, got {other:?}"),
        }
        assert_eq!(stage_of(&engine), None);
        // Native flip observed on the destination while the mover is still
        // retained on the source: single-domain convergence refuses the
        // cross-homed row before any capture binds.
        let (mut engine, owner, gen_id, source_domain, dest_domain) =
            cross_drag_world(&["win-a", "win-b"], &["win-c"]);
        let flipped = cross_drag_event(
            &owner,
            &gen_id,
            &source_domain,
            &dest_domain,
            "win-b",
            vec![
                engine_window("win-a", "out-1", "ws-1"),
                engine_window("win-b", "out-2", "ws-1"),
                engine_window("win-c", "out-2", "ws-1"),
            ],
            CoreCommand::DragPreview {
                window: "win-b".to_owned(),
                x: -1900,
                y: 400,
                hover_prior: None,
                source: Some(cross_source("out-1", "ws-1")),
            },
            "corr-pair-flip",
        );
        match engine.handle(&flipped) {
            CoreReply::Rejected { kind, .. } => assert_eq!(kind, "cross-domain-mismatch"),
            other => panic!("flip-homed pair preview must refuse, got {other:?}"),
        }
        assert_eq!(stage_of(&engine), Some("pair-converge"));
        // Pointer outside the retained destination work area refuses at
        // destination resolution with its own stage.
        let (mut engine, owner, gen_id, source_domain, dest_domain) =
            cross_drag_world(&["win-a", "win-b"], &["win-c"]);
        let outside = cross_drag_event(
            &owner,
            &gen_id,
            &source_domain,
            &dest_domain,
            "win-b",
            cross_drag_rows(),
            CoreCommand::DragPreview {
                window: "win-b".to_owned(),
                x: 5000,
                y: 5000,
                hover_prior: None,
                source: Some(cross_source("out-1", "ws-1")),
            },
            "corr-pair-outside",
        );
        match engine.handle(&outside) {
            CoreReply::Rejected { kind, .. } => assert_eq!(kind, "cross-domain-mismatch"),
            other => panic!("out-of-area pair preview must refuse, got {other:?}"),
        }
        assert_eq!(stage_of(&engine), Some("pair-resolve-outside"));
    }

    #[test]
    fn migrate_workspace_preserves_topology_shares_focus_and_floats() {
        use crate::boundary::{CoreCommand, CoreEvent, CoreReply};
        use crate::contract::{LifecycleCapabilities, Observation};
        use crate::directional::WindowId;
        use crate::ids::CorrelationId;
        use crate::seed::EngineWindow;
        use crate::session::{ObservedWindow, SessionObservation};
        let mut engine = Engine::new();
        let owner = OwnerId::parse("owner-a").expect("valid");
        let gen_id = GenerationId::parse("gen-1").expect("valid");
        engine.sync_binding(&owner, &gen_id);
        let source_domain = domain("out-1", "ws-2");
        let source_key = source_domain.key();
        // Three tiled members seed a nested topology; the third floats below.
        let seed_order = ["win-a", "win-b", "win-c"]
            .iter()
            .enumerate()
            .map(|(i, window)| EngineWindow {
                window: WindowId(window.to_string()),
                output: OutputId("out-1".to_owned()),
                workspace: WorkspaceId("ws-2".to_owned()),
                rect: Rect {
                    x: (i as i32) * 100,
                    y: 0,
                    w: 100,
                    h: 100,
                },
                floating: false,
                fit_excluded: false,
                fullscreen: false,
                maximized: false,
                sticky: false,
                fixed_auto: false,
                fixed_suppress: false,
                hints: crate::size_hints::WindowSizeHints::none(),
            })
            .collect::<Vec<_>>();
        let mut seeded = crate::seed::seed_session(&owner, &gen_id, 7, &source_domain, &seed_order)
            .expect("seeds three");
        // Intentional float of win-c through the convergence primitive (keeps
        // the existing retained rectangle, here `None`).
        let base = seeded.accepted_revision();
        let obs_windows: Vec<ObservedWindow> = seeded
            .snapshot()
            .windows
            .iter()
            .map(|link| {
                let floating = link.window.0 == "win-c";
                ObservedWindow {
                    window: link.window.clone(),
                    output: link.output.clone(),
                    workspace: link.workspace.clone(),
                    floating,
                    fullscreen: false,
                    maximized: false,
                    sticky: false,
                    fixed_auto: false,
                    fixed_suppress: false,
                    hints: crate::size_hints::WindowSizeHints::none(),
                }
            })
            .collect();
        seeded
            .converge_observation(
                &SessionObservation {
                    observation: Observation::new(owner.clone(), gen_id.clone(), base, 7),
                    windows: obs_windows,
                },
                Some(&WindowId("win-b".to_owned())),
            )
            .expect("float converges");
        assert!(seeded.is_exception(&WindowId("win-c".to_owned())));
        let pre_tree = seeded.tree_for(&source_key).cloned();
        let (pre_focus_domain, pre_focus_leaf) = seeded.focus();
        assert_eq!(pre_focus_domain, Some(source_key.clone()));
        let pre_revision = seeded.accepted_revision();
        let pre_float_geometry = seeded.floating_geometry(&WindowId("win-c".to_owned()));
        engine.store_committed(source_key.clone(), seeded, 3);
        // Unrelated domain stays byte-identical through the migration.
        let other_domain = domain("out-9", "ws-9");
        let other_key = other_domain.key();
        let other_order = vec![EngineWindow {
            window: WindowId("win-9".to_owned()),
            output: OutputId("out-9".to_owned()),
            workspace: WorkspaceId("ws-9".to_owned()),
            rect: Rect {
                x: 0,
                y: 0,
                w: 10,
                h: 10,
            },
            floating: false,
            fit_excluded: false,
            fullscreen: false,
            maximized: false,
            sticky: false,
            fixed_auto: false,
            fixed_suppress: false,
            hints: crate::size_hints::WindowSizeHints::none(),
        }];
        let other = crate::seed::seed_session(&owner, &gen_id, 7, &other_domain, &other_order)
            .expect("seeds other");
        engine.store_committed(other_key.clone(), other, 5);
        let other_before = engine
            .session(&other_key)
            .expect("other retained")
            .snapshot();
        // Carried source observation mirrors the retained set: tiled members
        // plain, the float exception with floating plus fit-excluded.
        let carried = ["win-a", "win-b"]
            .iter()
            .map(|window| EngineWindow {
                window: WindowId(window.to_string()),
                output: OutputId("out-1".to_owned()),
                workspace: WorkspaceId("ws-2".to_owned()),
                rect: Rect {
                    x: 0,
                    y: 0,
                    w: 10,
                    h: 10,
                },
                floating: false,
                fit_excluded: false,
                fullscreen: false,
                maximized: false,
                sticky: false,
                fixed_auto: false,
                fixed_suppress: false,
                hints: crate::size_hints::WindowSizeHints::none(),
            })
            .chain(std::iter::once(EngineWindow {
                window: WindowId("win-c".to_owned()),
                output: OutputId("out-1".to_owned()),
                workspace: WorkspaceId("ws-2".to_owned()),
                rect: Rect {
                    x: 240,
                    y: 160,
                    w: 100,
                    h: 100,
                },
                floating: true,
                fit_excluded: true,
                fullscreen: false,
                maximized: false,
                sticky: false,
                fixed_auto: false,
                fixed_suppress: false,
                hints: crate::size_hints::WindowSizeHints::none(),
            }))
            .collect::<Vec<_>>();
        let target_domain = OutputDomain {
            id: OutputId("out-2".to_owned()),
            workspace: WorkspaceId("ws-2".to_owned()),
            bounds: Rect {
                x: 0,
                y: 0,
                w: 800,
                h: 600,
            },
            gap: 0,
            adjacent: BTreeMap::new(),
        };
        let target_key = target_domain.key();
        let event = CoreEvent {
            owner: owner.clone(),
            generation: gen_id.clone(),
            correlation: CorrelationId::parse("corr-migrate-preserve").expect("valid"),
            revision: pre_revision,
            fingerprint: 7,
            domain: source_domain.clone(),
            domain_key: source_key.clone(),
            outer_gap: 3,
            focused_window: WindowId("win-b".to_owned()),
            windows: carried,
            directional: None,
            directional_target_outer_gap: None,
            target_domain: Some((target_domain.clone(), target_key.clone())),
            target_windows: vec![],
            closed_windows: vec![],
            command: CoreCommand::MigrateWorkspace {
                direction: "right".to_owned(),
            },
        };
        let plan = match engine.handle(&event) {
            CoreReply::MigrateWorkspace(plan) => plan,
            other => panic!("migration must plan, got {other:?}"),
        };
        assert_eq!(plan.base_revision, pre_revision, "revision preserved");
        assert_eq!(plan.source, source_key);
        assert_eq!(plan.target, target_key);
        assert_eq!(plan.geometry.len(), 2, "tiled members project");
        assert_eq!(plan.focus_domain, Some(target_key.clone()));
        assert_eq!(plan.focus_leaf, pre_focus_leaf);
        assert_eq!(
            plan.active_window,
            Some(WindowId("win-b".to_owned())),
            "active client named"
        );
        assert_eq!(plan.members, 3);
        assert_eq!(plan.floats, 1);
        assert!(
            plan.preconditions
                .contains(&crate::contract::LifecyclePrecondition::AdapterMustVerifyPostconditions)
        );
        assert!(!engine.contains(&source_key), "source rekeyed away");
        assert!(engine.contains(&target_key), "target retained");
        assert_eq!(engine.outer_gap(&target_key), Some(3));
        let moved = engine.session(&target_key).expect("moved");
        assert_eq!(moved.tree_for(&target_key), pre_tree.as_ref());
        assert_eq!(moved.accepted_revision(), pre_revision);
        let mut members: Vec<String> = moved
            .snapshot()
            .windows
            .iter()
            .map(|l| {
                assert_eq!(l.output.0, "out-2");
                assert_eq!(l.workspace.0, "ws-2");
                l.window.0.clone()
            })
            .collect();
        members.sort();
        assert_eq!(members, vec!["win-a".to_string(), "win-b".to_string()]);
        assert!(moved.is_exception(&WindowId("win-c".to_owned())));
        assert_eq!(
            moved.floating_geometry(&WindowId("win-c".to_owned())),
            pre_float_geometry,
            "float geometry preserved"
        );
        assert_eq!(moved.focus(), (Some(target_key.clone()), pre_focus_leaf));
        assert_eq!(
            engine.session(&other_key).expect("other kept").snapshot(),
            other_before,
            "unrelated domain unchanged"
        );
        let report = engine.last_migration().expect("migration logged");
        assert_eq!(report.members, 3);
        assert_eq!(report.floats, 1);
        assert!(!report.empty);
        let _ = LifecycleCapabilities::full();
    }

    #[test]
    fn migrate_workspace_allows_empty_without_fabricated_focus() {
        use crate::boundary::{CoreCommand, CoreEvent, CoreReply};
        use crate::directional::WindowId;
        use crate::ids::CorrelationId;
        let mut engine = Engine::new();
        let owner = OwnerId::parse("owner-a").expect("valid");
        let gen_id = GenerationId::parse("gen-1").expect("valid");
        engine.sync_binding(&owner, &gen_id);
        let other_domain = domain("out-9", "ws-9");
        let other_key = other_domain.key();
        let other_order = vec![crate::seed::EngineWindow {
            window: WindowId("win-9".to_owned()),
            output: OutputId("out-9".to_owned()),
            workspace: WorkspaceId("ws-9".to_owned()),
            rect: Rect {
                x: 0,
                y: 0,
                w: 10,
                h: 10,
            },
            floating: false,
            fit_excluded: false,
            fullscreen: false,
            maximized: false,
            sticky: false,
            fixed_auto: false,
            fixed_suppress: false,
            hints: crate::size_hints::WindowSizeHints::none(),
        }];
        let other = crate::seed::seed_session(&owner, &gen_id, 7, &other_domain, &other_order)
            .expect("seeds other");
        engine.store_committed(other_key.clone(), other, 5);
        let other_before = engine
            .session(&other_key)
            .expect("other retained")
            .snapshot();
        let source_domain = domain("out-1", "ws-2");
        let source_key = source_domain.key();
        let target_domain = domain("out-2", "ws-2");
        let target_key = target_domain.key();
        let event = CoreEvent {
            owner: owner.clone(),
            generation: gen_id.clone(),
            correlation: CorrelationId::parse("corr-migrate-empty").expect("valid"),
            revision: 0,
            fingerprint: 7,
            domain: source_domain,
            domain_key: source_key.clone(),
            outer_gap: 0,
            focused_window: WindowId(String::new()),
            windows: vec![],
            directional: None,
            directional_target_outer_gap: None,
            target_domain: Some((target_domain, target_key.clone())),
            target_windows: vec![],
            closed_windows: vec![],
            command: CoreCommand::MigrateWorkspace {
                direction: "right".to_owned(),
            },
        };
        match engine.handle(&event) {
            CoreReply::MigrateWorkspace(plan) => {
                assert!(plan.geometry.is_empty());
                assert_eq!(plan.focus_domain, None);
                assert_eq!(plan.focus_leaf, None);
            }
            other => panic!("empty migration must plan, got {other:?}"),
        }
        assert!(!engine.contains(&source_key));
        assert!(!engine.contains(&target_key), "empty moves no slot");
        assert_eq!(
            engine.session(&other_key).expect("other kept").snapshot(),
            other_before,
            "unrelated domain unchanged"
        );
        let report = engine.last_migration().expect("migration logged");
        assert!(report.empty);
        assert_eq!(report.members, 0);
    }

    #[test]
    fn migrate_workspace_refuses_overlay_invalid_target_and_residue() {
        use crate::boundary::{CoreCommand, CoreEvent, CoreReply};
        use crate::directional::WindowId;
        use crate::ids::CorrelationId;
        use crate::seed::EngineWindow;
        fn carried(
            output: &str,
            workspace: &str,
            window: &str,
            fullscreen: bool,
            fit_excluded: bool,
        ) -> EngineWindow {
            EngineWindow {
                window: WindowId(window.to_owned()),
                output: OutputId(output.to_owned()),
                workspace: WorkspaceId(workspace.to_owned()),
                rect: Rect {
                    x: 0,
                    y: 0,
                    w: 10,
                    h: 10,
                },
                floating: false,
                fit_excluded,
                fullscreen,
                maximized: false,
                sticky: false,
                fixed_auto: false,
                fixed_suppress: false,
                hints: crate::size_hints::WindowSizeHints::none(),
            }
        }
        fn migrate_event(
            owner: &OwnerId,
            gen_id: &GenerationId,
            source_domain: &OutputDomain,
            windows: Vec<EngineWindow>,
            target: Option<(OutputDomain, DomainKey)>,
            target_windows: Vec<EngineWindow>,
            correlation: &str,
        ) -> CoreEvent {
            CoreEvent {
                owner: owner.clone(),
                generation: gen_id.clone(),
                correlation: CorrelationId::parse(correlation).expect("valid"),
                revision: 0,
                fingerprint: 7,
                domain: source_domain.clone(),
                domain_key: source_domain.key(),
                outer_gap: 0,
                focused_window: WindowId("win-1".to_owned()),
                windows,
                directional: None,
                directional_target_outer_gap: None,
                target_domain: target,
                target_windows,
                closed_windows: vec![],
                command: CoreCommand::MigrateWorkspace {
                    direction: "right".to_owned(),
                },
            }
        }
        let mut engine = Engine::new();
        let owner = OwnerId::parse("owner-a").expect("valid");
        let gen_id = GenerationId::parse("gen-1").expect("valid");
        engine.sync_binding(&owner, &gen_id);
        let source_domain = domain("out-1", "ws-2");
        let source_key = source_domain.key();
        let seed_order = ["win-1", "win-2"]
            .iter()
            .map(|window| carried("out-1", "ws-2", window, false, false))
            .collect::<Vec<_>>();
        let seeded = crate::seed::seed_session(&owner, &gen_id, 7, &source_domain, &seed_order)
            .expect("seeds two");
        let pre_revision = seeded.accepted_revision();
        engine.store_committed(source_key.clone(), seeded, 0);
        let target_domain = domain("out-2", "ws-2");
        let target_key = target_domain.key();
        // D8: fullscreen members carry with their tiled slot preserved.
        // Verified on an isolated engine so the refusal checks below keep a
        // pristine source.
        {
            let mut carry = Engine::new();
            carry.sync_binding(&owner, &gen_id);
            let seeded = crate::seed::seed_session(&owner, &gen_id, 7, &source_domain, &seed_order)
                .expect("seeds two");
            let pre = seeded.accepted_revision();
            carry.store_committed(source_key.clone(), seeded, 0);
            let overlay = migrate_event(
                &owner,
                &gen_id,
                &source_domain,
                vec![
                    carried("out-1", "ws-2", "win-1", true, true),
                    carried("out-1", "ws-2", "win-2", false, false),
                ],
                Some((target_domain.clone(), target_key.clone())),
                vec![],
                "corr-migrate-overlay",
            );
            match carry.handle(&overlay) {
                CoreReply::MigrateWorkspace(plan) => {
                    assert_eq!(plan.members, 2);
                    assert_eq!(plan.active_window, Some(WindowId("win-1".to_owned())));
                    assert_eq!(plan.base_revision, pre);
                    let moved = carry.session(&target_key).expect("moved");
                    assert!(
                        moved
                            .snapshot()
                            .windows
                            .iter()
                            .any(|link| link.window.0 == "win-1")
                    );
                }
                other => panic!("fullscreen must carry, got {other:?}"),
            }
        }
        // Maximized-shaped fit-excluded (no floating/sticky/fullscreen/maximized) refuses.
        let maximized = migrate_event(
            &owner,
            &gen_id,
            &source_domain,
            vec![
                carried("out-1", "ws-2", "win-1", false, true),
                carried("out-1", "ws-2", "win-2", false, false),
            ],
            Some((target_domain.clone(), target_key.clone())),
            vec![],
            "corr-migrate-maximized",
        );
        match engine.handle(&maximized) {
            CoreReply::Rejected { kind, .. } => assert_eq!(kind, "overlay-present"),
            other => panic!("maximized must refuse, got {other:?}"),
        }
        // Same-output target refuses.
        let same_output = migrate_event(
            &owner,
            &gen_id,
            &source_domain,
            seed_order.clone(),
            Some((source_domain.clone(), source_key.clone())),
            vec![],
            "corr-migrate-same-output",
        );
        match engine.handle(&same_output) {
            CoreReply::Rejected { kind, .. } => assert_eq!(kind, "cross-domain-mismatch"),
            other => panic!("same-output must refuse, got {other:?}"),
        }
        // Different-workspace target refuses.
        let other_ws = domain("out-2", "ws-9");
        let other_ws_key = other_ws.key();
        let cross_ws = migrate_event(
            &owner,
            &gen_id,
            &source_domain,
            seed_order.clone(),
            Some((other_ws, other_ws_key)),
            vec![],
            "corr-migrate-cross-ws",
        );
        match engine.handle(&cross_ws) {
            CoreReply::Rejected { kind, .. } => assert_eq!(kind, "cross-domain-mismatch"),
            other => panic!("different-workspace must refuse, got {other:?}"),
        }
        // Carried target windows (SAME-workspace residue) refuse.
        let residue_carried = migrate_event(
            &owner,
            &gen_id,
            &source_domain,
            seed_order.clone(),
            Some((target_domain.clone(), target_key.clone())),
            vec![carried("out-2", "ws-2", "win-x", false, false)],
            "corr-migrate-residue-carried",
        );
        match engine.handle(&residue_carried) {
            CoreReply::Rejected { kind, .. } => assert_eq!(kind, "unchanged"),
            other => panic!("carried residue must refuse, got {other:?}"),
        }
        // Retained target residue refuses.
        let target_seed_order = ["win-1", "win-2"]
            .iter()
            .map(|window| carried("out-2", "ws-2", window, false, false))
            .collect::<Vec<_>>();
        engine.insert_raw(
            target_key.clone(),
            crate::seed::seed_session(&owner, &gen_id, 7, &target_domain, &target_seed_order)
                .map(|mut session| {
                    session.set_policy(engine.policy().clone());
                    session
                })
                .expect("seeds target residue"),
            0,
        );
        let retained_residue = migrate_event(
            &owner,
            &gen_id,
            &source_domain,
            seed_order.clone(),
            Some((target_domain.clone(), target_key.clone())),
            vec![],
            "corr-migrate-residue-retained",
        );
        match engine.handle(&retained_residue) {
            CoreReply::Rejected { kind, .. } => assert_eq!(kind, "unchanged"),
            other => panic!("retained residue must refuse, got {other:?}"),
        }
        engine.remove(&target_key);
        // Unknown source with members refuses (never autorekeys).
        let unknown_domain = domain("out-7", "ws-7");
        let unknown_target = domain("out-8", "ws-7");
        let unknown_key = unknown_target.key();
        let unknown = migrate_event(
            &owner,
            &gen_id,
            &unknown_domain,
            vec![carried("out-7", "ws-7", "win-z", false, false)],
            Some((unknown_target, unknown_key)),
            vec![],
            "corr-migrate-unknown",
        );
        match engine.handle(&unknown) {
            CoreReply::Rejected { kind, .. } => assert_eq!(kind, "unknown-domain"),
            other => panic!("unknown source must refuse, got {other:?}"),
        }
        // Pending residue refuses.
        {
            use crate::contract::{AckOutcome, AdapterAck, LifecycleCapabilities, Observation};
            use crate::session::{ExceptionFlags, SessionCommand, SessionObservation};
            let session = engine.session_mut(&source_key).expect("source");
            let base = session.accepted_revision();
            let observed = SessionObservation {
                observation: Observation::new(owner.clone(), gen_id.clone(), base, 7),
                windows: crate::seed::session_observation_for(
                    &owner,
                    &gen_id,
                    base,
                    7,
                    &seed_order,
                )
                .windows
                .into_iter()
                .chain(std::iter::once(crate::session::ObservedWindow {
                    window: WindowId("win-new".to_owned()),
                    output: OutputId("out-1".to_owned()),
                    workspace: WorkspaceId("ws-2".to_owned()),
                    floating: false,
                    fullscreen: false,
                    maximized: false,
                    sticky: false,
                    fixed_auto: false,
                    fixed_suppress: false,
                    hints: crate::size_hints::WindowSizeHints::none(),
                }))
                .collect(),
            };
            let correlation = CorrelationId::parse("corr-migrate-stage-pending").expect("valid");
            session
                .propose(
                    &SessionCommand::Admit {
                        window: WindowId("win-new".to_owned()),
                        output: OutputId("out-1".to_owned()),
                        workspace: WorkspaceId("ws-2".to_owned()),
                        exceptions: ExceptionFlags::none(),
                        exception_behavior: None,
                        placement_bounds: Rect {
                            x: 0,
                            y: 0,
                            w: 800,
                            h: 600,
                        },
                        suppress_fixed_float: false,
                    },
                    &observed,
                    &correlation,
                    &LifecycleCapabilities::full(),
                )
                .expect("stages pending");
            let _ = AdapterAck::new(
                correlation,
                owner.clone(),
                gen_id.clone(),
                base,
                AckOutcome::Accepted,
            );
        }
        let pending = migrate_event(
            &owner,
            &gen_id,
            &source_domain,
            seed_order.clone(),
            Some((target_domain.clone(), target_key.clone())),
            vec![],
            "corr-migrate-pending",
        );
        match engine.handle(&pending) {
            CoreReply::Rejected { kind, .. } => assert_eq!(kind, "pending-exists"),
            other => panic!("pending must refuse, got {other:?}"),
        }
        // Every refusal left the source exactly intact.
        assert!(engine.contains(&source_key));
        assert!(!engine.contains(&target_key));
        let retained = engine.session(&source_key).expect("source kept");
        assert_eq!(retained.accepted_revision(), pre_revision);
        let mut members: Vec<String> = retained
            .snapshot()
            .windows
            .iter()
            .map(|l| l.window.0.clone())
            .collect();
        members.sort();
        assert_eq!(members, vec!["win-1".to_string(), "win-2".to_string()]);
    }

    fn migrate_carried(member: (&str, bool, bool)) -> EngineWindow {
        // (window, floating, sticky) homed on the (out-1, ws-2) fixture
        // source with adapter-shaped flags.
        let (window, floating, sticky) = member;
        EngineWindow {
            window: WindowId(window.to_owned()),
            output: OutputId("out-1".to_owned()),
            workspace: WorkspaceId("ws-2".to_owned()),
            rect: Rect {
                x: 0,
                y: 0,
                w: 10,
                h: 10,
            },
            floating,
            fit_excluded: floating || sticky,
            fullscreen: false,
            maximized: false,
            sticky,
            fixed_auto: false,
            fixed_suppress: false,
            hints: crate::size_hints::WindowSizeHints::none(),
        }
    }

    fn migrate_float_adopted(
        session: &mut Session,
        owner: &OwnerId,
        gen_id: &GenerationId,
        floats: &[&str],
        focus: Option<&WindowId>,
    ) {
        use crate::contract::Observation;
        use crate::session::{ObservedWindow, SessionObservation};
        let base = session.accepted_revision();
        let windows: Vec<ObservedWindow> = session
            .snapshot()
            .windows
            .iter()
            .map(|link| ObservedWindow {
                window: link.window.clone(),
                output: link.output.clone(),
                workspace: link.workspace.clone(),
                floating: floats.contains(&link.window.0.as_str()),
                fullscreen: false,
                maximized: false,
                sticky: false,
                fixed_auto: false,
                fixed_suppress: false,
                hints: crate::size_hints::WindowSizeHints::none(),
            })
            .collect();
        session
            .converge_observation(
                &SessionObservation {
                    observation: Observation::new(owner.clone(), gen_id.clone(), base, 7),
                    windows,
                },
                focus,
            )
            .expect("float adoption converges");
    }

    #[allow(clippy::too_many_arguments)]
    fn migrate_event_for(
        owner: &OwnerId,
        gen_id: &GenerationId,
        source_domain: &OutputDomain,
        revision: u64,
        focused: &str,
        windows: Vec<EngineWindow>,
        target_domain: &OutputDomain,
        correlation: &str,
    ) -> CoreEvent {
        use crate::boundary::CoreCommand;
        use crate::ids::CorrelationId;
        CoreEvent {
            owner: owner.clone(),
            generation: gen_id.clone(),
            correlation: CorrelationId::parse(correlation).expect("valid"),
            revision,
            fingerprint: 7,
            domain: source_domain.clone(),
            domain_key: source_domain.key(),
            outer_gap: 0,
            focused_window: WindowId(focused.to_owned()),
            windows,
            directional: None,
            directional_target_outer_gap: None,
            target_domain: Some((target_domain.clone(), target_domain.key())),
            target_windows: vec![],
            closed_windows: vec![],
            command: CoreCommand::MigrateWorkspace {
                direction: "right".to_owned(),
            },
        }
    }

    #[test]
    fn migrate_workspace_active_float_names_client_without_tile_focus() {
        use crate::boundary::CoreReply;
        use crate::directional::WindowId;
        // Focused names the float exception: no tiled leaf to echo, so the
        // reply carries no focus while `active_window` names the client; the
        // stored tiled focus still rekeys with the domain.
        let mut engine = Engine::new();
        let owner = OwnerId::parse("owner-a").expect("valid");
        let gen_id = GenerationId::parse("gen-1").expect("valid");
        engine.sync_binding(&owner, &gen_id);
        let source_domain = domain("out-1", "ws-2");
        let source_key = source_domain.key();
        let order = ["win-a", "win-b", "win-c"]
            .iter()
            .map(|window| migrate_carried((window, false, false)))
            .collect::<Vec<_>>();
        let mut seeded =
            crate::seed::seed_session(&owner, &gen_id, 7, &source_domain, &order).expect("seeds");
        migrate_float_adopted(
            &mut seeded,
            &owner,
            &gen_id,
            &["win-c"],
            Some(&WindowId("win-b".to_owned())),
        );
        let (_, pre_leaf) = seeded.focus();
        let pre_leaf = pre_leaf.expect("tiled focus retained");
        let pre_revision = seeded.accepted_revision();
        engine.store_committed(source_key.clone(), seeded, 0);
        let target_domain = domain("out-2", "ws-2");
        let target_key = target_domain.key();
        let carried = ["win-a", "win-b"]
            .iter()
            .map(|window| migrate_carried((window, false, false)))
            .chain(std::iter::once(migrate_carried(("win-c", true, false))))
            .collect::<Vec<_>>();
        let event = migrate_event_for(
            &owner,
            &gen_id,
            &source_domain,
            pre_revision,
            "win-c",
            carried,
            &target_domain,
            "corr-migrate-active-float",
        );
        let plan = match engine.handle(&event) {
            CoreReply::MigrateWorkspace(plan) => plan,
            other => panic!("active-float migration must plan, got {other:?}"),
        };
        assert_eq!(plan.geometry.len(), 2);
        assert_eq!(plan.focus_domain, None, "no tile focus fabricated");
        assert_eq!(plan.focus_leaf, None);
        assert_eq!(plan.active_window, Some(WindowId("win-c".to_owned())));
        let moved = engine.session(&target_key).expect("moved");
        assert_eq!(moved.focus(), (Some(target_key), Some(pre_leaf)));
        assert!(moved.is_exception(&WindowId("win-c".to_owned())));
    }

    #[test]
    fn migrate_workspace_float_only_carries_exceptions() {
        use crate::boundary::CoreReply;
        use crate::directional::WindowId;
        let mut engine = Engine::new();
        let owner = OwnerId::parse("owner-a").expect("valid");
        let gen_id = GenerationId::parse("gen-1").expect("valid");
        engine.sync_binding(&owner, &gen_id);
        let source_domain = domain("out-1", "ws-2");
        let source_key = source_domain.key();
        let order = ["win-a", "win-b"]
            .iter()
            .map(|window| migrate_carried((window, false, false)))
            .collect::<Vec<_>>();
        let mut seeded =
            crate::seed::seed_session(&owner, &gen_id, 7, &source_domain, &order).expect("seeds");
        migrate_float_adopted(&mut seeded, &owner, &gen_id, &["win-a", "win-b"], None);
        assert!(seeded.tree_for(&source_key).is_none(), "no tiled tree left");
        let pre_revision = seeded.accepted_revision();
        engine.store_committed(source_key.clone(), seeded, 0);
        let target_domain = domain("out-2", "ws-2");
        let target_key = target_domain.key();
        let carried = ["win-a", "win-b"]
            .iter()
            .map(|window| migrate_carried((window, true, false)))
            .collect::<Vec<_>>();
        let event = migrate_event_for(
            &owner,
            &gen_id,
            &source_domain,
            pre_revision,
            "win-a",
            carried,
            &target_domain,
            "corr-migrate-float-only",
        );
        let plan = match engine.handle(&event) {
            CoreReply::MigrateWorkspace(plan) => plan,
            other => panic!("float-only migration must plan, got {other:?}"),
        };
        assert!(plan.geometry.is_empty());
        assert_eq!(plan.focus_domain, None);
        assert_eq!(plan.active_window, Some(WindowId("win-a".to_owned())));
        assert_eq!(plan.members, 2);
        assert_eq!(plan.floats, 2);
        let moved = engine.session(&target_key).expect("moved");
        for window in ["win-a", "win-b"] {
            assert!(moved.is_exception(&WindowId(window.to_owned())));
        }
        assert!(!engine.contains(&source_key));
    }

    #[test]
    fn migrate_workspace_absent_active_client_keeps_members() {
        use crate::boundary::CoreReply;
        use crate::directional::WindowId;
        // No active client (empty focus with members): members still migrate
        // with no fabricated focus on either side.
        let mut engine = Engine::new();
        let owner = OwnerId::parse("owner-a").expect("valid");
        let gen_id = GenerationId::parse("gen-1").expect("valid");
        engine.sync_binding(&owner, &gen_id);
        let source_domain = domain("out-1", "ws-2");
        let source_key = source_domain.key();
        let order = ["win-a", "win-b"]
            .iter()
            .map(|window| migrate_carried((window, false, false)))
            .collect::<Vec<_>>();
        let seeded =
            crate::seed::seed_session(&owner, &gen_id, 7, &source_domain, &order).expect("seeds");
        let pre_revision = seeded.accepted_revision();
        engine.store_committed(source_key.clone(), seeded, 0);
        let target_domain = domain("out-2", "ws-2");
        let target_key = target_domain.key();
        let event = migrate_event_for(
            &owner,
            &gen_id,
            &source_domain,
            pre_revision,
            "",
            order,
            &target_domain,
            "corr-migrate-no-active",
        );
        let plan = match engine.handle(&event) {
            CoreReply::MigrateWorkspace(plan) => plan,
            other => panic!("no-active migration must plan, got {other:?}"),
        };
        assert_eq!(plan.geometry.len(), 2);
        assert_eq!(plan.focus_domain, None);
        assert_eq!(plan.active_window, None);
        assert!(engine.session(&target_key).is_some());
        assert!(!engine.contains(&source_key));
        let _ = WindowId("win-a".to_owned());
    }

    #[test]
    fn migrate_workspace_retained_empty_session_migrates_empty() {
        use crate::boundary::{CoreCommand, CoreEvent, CoreReply};
        use crate::directional::WindowId;
        use crate::ids::CorrelationId;
        // A retained (raw-inserted) empty session migrates like the absent
        // path: the slot is retired with no focus and no geometry.
        let mut engine = Engine::new();
        let owner = OwnerId::parse("owner-a").expect("valid");
        let gen_id = GenerationId::parse("gen-1").expect("valid");
        engine.sync_binding(&owner, &gen_id);
        let source_domain = domain("out-1", "ws-2");
        let source_key = source_domain.key();
        let empty = Session::new(
            owner.clone(),
            gen_id.clone(),
            0,
            7,
            vec![source_domain.clone()],
        )
        .expect("empty session");
        engine.insert_raw(source_key.clone(), empty, 0);
        let target_domain = domain("out-2", "ws-2");
        let target_key = target_domain.key();
        let event = CoreEvent {
            owner: owner.clone(),
            generation: gen_id.clone(),
            correlation: CorrelationId::parse("corr-migrate-empty-present").expect("valid"),
            revision: 0,
            fingerprint: 7,
            domain: source_domain,
            domain_key: source_key.clone(),
            outer_gap: 0,
            focused_window: WindowId(String::new()),
            windows: vec![],
            directional: None,
            directional_target_outer_gap: None,
            target_domain: Some((target_domain, target_key.clone())),
            target_windows: vec![],
            closed_windows: vec![],
            command: CoreCommand::MigrateWorkspace {
                direction: "right".to_owned(),
            },
        };
        match engine.handle(&event) {
            CoreReply::MigrateWorkspace(plan) => {
                assert!(plan.geometry.is_empty());
                assert_eq!(plan.focus_domain, None);
            }
            other => panic!("retained-empty migration must plan, got {other:?}"),
        }
        assert!(!engine.contains(&source_key));
        assert!(!engine.contains(&target_key));
    }

    #[test]
    fn migrate_workspace_sticky_stays_homed_on_source() {
        use crate::boundary::CoreReply;
        use crate::directional::WindowId;
        // Sticky rides the carried observation for identity but never the
        // target: tiled members plus the workspace float move, the sticky
        // link stays homed on the source with its leaf and class intact.
        let mut engine = Engine::new();
        let owner = OwnerId::parse("owner-a").expect("valid");
        let gen_id = GenerationId::parse("gen-1").expect("valid");
        engine.sync_binding(&owner, &gen_id);
        let source_domain = domain("out-1", "ws-2");
        let source_key = source_domain.key();
        let order = ["win-1", "win-2", "win-f"]
            .iter()
            .map(|window| migrate_carried((window, false, false)))
            .collect::<Vec<_>>();
        let mut seeded =
            crate::seed::seed_session(&owner, &gen_id, 7, &source_domain, &order).expect("seeds");
        migrate_float_adopted(
            &mut seeded,
            &owner,
            &gen_id,
            &["win-f"],
            Some(&WindowId("win-1".to_owned())),
        );
        // The sticky window converges as an ordinary tiled link (sticky is
        // adapter-owned); it joins retained state through the same primitive.
        {
            use crate::contract::Observation;
            use crate::session::{ObservedWindow, SessionObservation};
            let base = seeded.accepted_revision();
            let mut windows: Vec<ObservedWindow> = seeded
                .snapshot()
                .windows
                .iter()
                .map(|link| ObservedWindow {
                    window: link.window.clone(),
                    output: link.output.clone(),
                    workspace: link.workspace.clone(),
                    floating: false,
                    fullscreen: false,
                    maximized: false,
                    sticky: false,
                    fixed_auto: false,
                    fixed_suppress: false,
                    hints: crate::size_hints::WindowSizeHints::none(),
                })
                .collect();
            windows.extend(seeded.exception_observed());
            windows.push(ObservedWindow {
                window: WindowId("win-s".to_owned()),
                output: OutputId("out-1".to_owned()),
                workspace: WorkspaceId("ws-2".to_owned()),
                floating: false,
                fullscreen: false,
                maximized: false,
                sticky: true,
                fixed_auto: false,
                fixed_suppress: false,
                hints: crate::size_hints::WindowSizeHints::none(),
            });
            seeded
                .converge_observation(
                    &SessionObservation {
                        observation: Observation::new(owner.clone(), gen_id.clone(), base, 7),
                        windows,
                    },
                    Some(&WindowId("win-1".to_owned())),
                )
                .expect("sticky converges");
        }
        let sticky_leaf = seeded
            .snapshot()
            .windows
            .iter()
            .find(|link| link.window.0 == "win-s")
            .expect("sticky linked")
            .leaf
            .clone();
        let pre_revision = seeded.accepted_revision();
        engine.store_committed(source_key.clone(), seeded, 0);
        let target_domain = domain("out-2", "ws-2");
        let target_key = target_domain.key();
        let carried = ["win-1", "win-2"]
            .iter()
            .map(|window| migrate_carried((window, false, false)))
            .chain(std::iter::once(migrate_carried(("win-f", true, false))))
            .chain(std::iter::once(migrate_carried(("win-s", false, true))))
            .collect::<Vec<_>>();
        let event = migrate_event_for(
            &owner,
            &gen_id,
            &source_domain,
            pre_revision,
            "win-1",
            carried,
            &target_domain,
            "corr-migrate-sticky",
        );
        let plan = match engine.handle(&event) {
            CoreReply::MigrateWorkspace(plan) => plan,
            other => panic!("sticky migration must plan, got {other:?}"),
        };
        assert_eq!(plan.geometry.len(), 2, "sticky takes no tile");
        assert!(plan.geometry.iter().all(|entry| entry.window.0 != "win-s"));
        assert_eq!(plan.active_window, Some(WindowId("win-1".to_owned())));
        let moved = engine.session(&target_key).expect("moved");
        assert!(
            moved
                .snapshot()
                .windows
                .iter()
                .all(|link| link.window.0 != "win-s"),
            "sticky never rehomes to the target"
        );
        assert!(moved.is_exception(&WindowId("win-f".to_owned())));
        let stay = engine.session(&source_key).expect("sticky residual");
        let stay_snapshot = stay.snapshot();
        let stay_link = stay_snapshot
            .windows
            .iter()
            .find(|link| link.window.0 == "win-s")
            .expect("sticky kept");
        assert_eq!(stay_link.output.0, "out-1", "source home preserved");
        assert_eq!(stay_link.workspace.0, "ws-2");
        assert_eq!(stay_link.leaf, sticky_leaf, "leaf identity preserved");
        assert_eq!(engine.outer_gap(&source_key), Some(0));
        let report = engine.last_migration().expect("migration logged");
        assert_eq!(report.members, 3);
        assert_eq!(report.floats, 1);
    }

    #[test]
    fn migrate_workspace_ignores_wire_revision() {
        use crate::boundary::CoreReply;
        // The wire revision is vestigial (adapters hardcode 0; bases derive
        // internally like every other op): an arbitrary revision with exact
        // members still plans at the retained revision.
        let mut engine = Engine::new();
        let owner = OwnerId::parse("owner-a").expect("valid");
        let gen_id = GenerationId::parse("gen-1").expect("valid");
        engine.sync_binding(&owner, &gen_id);
        let source_domain = domain("out-1", "ws-2");
        let source_key = source_domain.key();
        let order = ["win-1", "win-2"]
            .iter()
            .map(|window| migrate_carried((window, false, false)))
            .collect::<Vec<_>>();
        let seeded =
            crate::seed::seed_session(&owner, &gen_id, 7, &source_domain, &order).expect("seeds");
        let pre_revision = seeded.accepted_revision();
        engine.store_committed(source_key.clone(), seeded, 0);
        let target_domain = domain("out-2", "ws-2");
        let event = migrate_event_for(
            &owner,
            &gen_id,
            &source_domain,
            u64::MAX,
            "win-1",
            order,
            &target_domain,
            "corr-migrate-wire-rev",
        );
        match engine.handle(&event) {
            CoreReply::MigrateWorkspace(plan) => assert_eq!(plan.base_revision, pre_revision),
            other => panic!("vestigial revision must still plan, got {other:?}"),
        }
        assert!(!engine.contains(&source_key));
    }

    #[test]
    fn migrate_workspace_refuses_partial_and_maximized_float() {
        use crate::boundary::CoreReply;
        use crate::directional::WindowId;
        let mut engine = Engine::new();
        let owner = OwnerId::parse("owner-a").expect("valid");
        let gen_id = GenerationId::parse("gen-1").expect("valid");
        engine.sync_binding(&owner, &gen_id);
        let source_domain = domain("out-1", "ws-2");
        let source_key = source_domain.key();
        let order = ["win-1", "win-2"]
            .iter()
            .map(|window| migrate_carried((window, false, false)))
            .collect::<Vec<_>>();
        let seeded =
            crate::seed::seed_session(&owner, &gen_id, 7, &source_domain, &order).expect("seeds");
        let pre_revision = seeded.accepted_revision();
        assert!(pre_revision > 0);
        engine.store_committed(source_key.clone(), seeded, 0);
        let target_domain = domain("out-2", "ws-2");
        // Partial membership (unseen retained member) refuses without commit.
        let partial = migrate_event_for(
            &owner,
            &gen_id,
            &source_domain,
            pre_revision,
            "win-1",
            vec![migrate_carried(("win-1", false, false))],
            &target_domain,
            "corr-migrate-partial",
        );
        match engine.handle(&partial) {
            CoreReply::Rejected { kind, message } => {
                assert_eq!(kind, "partial-observation");
                assert_eq!(message, "observation does not cover the known window set");
            }
            other => panic!("partial must refuse, got {other:?}"),
        }
        // D8: a maximized float carries with its exception slot preserved.
        // Verified on an isolated engine so the partial refusal above keeps
        // a pristine source for the intact-state checks below.
        {
            let mut carry = Engine::new();
            carry.sync_binding(&owner, &gen_id);
            let seeded = crate::seed::seed_session(&owner, &gen_id, 7, &source_domain, &order)
                .expect("seeds");
            let pre = seeded.accepted_revision();
            carry.store_committed(source_key.clone(), seeded, 0);
            let mut max_float = migrate_carried(("win-1", true, false));
            max_float.maximized = true;
            let overlay = migrate_event_for(
                &owner,
                &gen_id,
                &source_domain,
                pre,
                "win-1",
                vec![max_float, migrate_carried(("win-2", false, false))],
                &target_domain,
                "corr-migrate-max-float",
            );
            match carry.handle(&overlay) {
                CoreReply::MigrateWorkspace(plan) => {
                    assert_eq!(plan.members, 2);
                    assert_eq!(plan.floats, 1);
                    assert_eq!(plan.active_window, Some(WindowId("win-1".to_owned())));
                    let moved = carry.session(&target_domain.key()).expect("moved");
                    assert!(moved.is_exception(&WindowId("win-1".to_owned())));
                }
                other => panic!("maximized float must carry, got {other:?}"),
            }
        }
        assert!(engine.contains(&source_key), "refusals mutate nothing");
        assert!(!engine.contains(&target_domain.key()));
        let retained = engine.session(&source_key).expect("source kept");
        assert_eq!(retained.accepted_revision(), pre_revision);
        let _ = WindowId("win-1".to_owned());
    }

    #[test]
    fn migrate_workspace_carries_maximized_tiled_with_slot() {
        use crate::boundary::CoreReply;
        use crate::directional::WindowId;
        // D8: a tiled maximized member carries with its reserved tile slot
        // preserved on the target and no unmaximize.
        let mut engine = Engine::new();
        let owner = OwnerId::parse("owner-a").expect("valid");
        let gen_id = GenerationId::parse("gen-1").expect("valid");
        engine.sync_binding(&owner, &gen_id);
        let source_domain = domain("out-1", "ws-2");
        let source_key = source_domain.key();
        let order = ["win-1", "win-2"]
            .iter()
            .map(|window| migrate_carried((window, false, false)))
            .collect::<Vec<_>>();
        let seeded =
            crate::seed::seed_session(&owner, &gen_id, 7, &source_domain, &order).expect("seeds");
        let pre_revision = seeded.accepted_revision();
        engine.store_committed(source_key.clone(), seeded, 0);
        let target_domain = domain("out-2", "ws-2");
        let target_key = target_domain.key();
        let mut maximized = migrate_carried(("win-2", false, false));
        maximized.maximized = true;
        maximized.fit_excluded = true;
        let event = migrate_event_for(
            &owner,
            &gen_id,
            &source_domain,
            pre_revision,
            "win-1",
            vec![migrate_carried(("win-1", false, false)), maximized],
            &target_domain,
            "corr-migrate-max-tiled",
        );
        let plan = match engine.handle(&event) {
            CoreReply::MigrateWorkspace(plan) => plan,
            other => panic!("maximized tiled must carry, got {other:?}"),
        };
        assert_eq!(plan.members, 2);
        assert_eq!(plan.floats, 0);
        assert_eq!(plan.active_window, Some(WindowId("win-1".to_owned())));
        let moved = engine.session(&target_key).expect("moved");
        assert_eq!(moved.snapshot().windows.len(), 2, "reserved slot preserved");
        assert!(!engine.contains(&source_key));
    }

    #[test]
    fn migrate_workspace_retained_empty_adopts_carried_newcomers() {
        use crate::boundary::CoreReply;
        use crate::directional::WindowId;
        // Retained-empty sources migrate empty only when nothing is carried;
        // carried newcomers are adopted through the normal flow instead of
        // being discarded with the retired slot.
        let mut engine = Engine::new();
        let owner = OwnerId::parse("owner-a").expect("valid");
        let gen_id = GenerationId::parse("gen-1").expect("valid");
        engine.sync_binding(&owner, &gen_id);
        let source_domain = domain("out-1", "ws-2");
        let source_key = source_domain.key();
        let empty = Session::new(
            owner.clone(),
            gen_id.clone(),
            0,
            7,
            vec![source_domain.clone()],
        )
        .expect("empty session");
        engine.insert_raw(source_key.clone(), empty, 0);
        let target_domain = domain("out-2", "ws-2");
        let target_key = target_domain.key();
        let order = ["win-1", "win-2"]
            .iter()
            .map(|window| migrate_carried((window, false, false)))
            .collect::<Vec<_>>();
        let event = migrate_event_for(
            &owner,
            &gen_id,
            &source_domain,
            0,
            "win-1",
            order,
            &target_domain,
            "corr-migrate-empty-adopt",
        );
        let plan = match engine.handle(&event) {
            CoreReply::MigrateWorkspace(plan) => plan,
            other => panic!("newcomers must be adopted and migrated, got {other:?}"),
        };
        assert_eq!(plan.geometry.len(), 2);
        assert_eq!(plan.active_window, Some(WindowId("win-1".to_owned())));
        let moved = engine.session(&target_key).expect("moved");
        assert_eq!(moved.snapshot().windows.len(), 2);
        assert!(!engine.contains(&source_key));
    }

    #[test]
    fn migrate_workspace_refuses_off_source_homing_and_dangling_focus() {
        use crate::boundary::CoreReply;
        use crate::directional::WindowId;
        // Direct Engine callers get the same fences as the protocol route:
        // off-source homing and unfocused-but-named clients refuse with live
        // state exactly intact.
        let mut engine = Engine::new();
        let owner = OwnerId::parse("owner-a").expect("valid");
        let gen_id = GenerationId::parse("gen-1").expect("valid");
        engine.sync_binding(&owner, &gen_id);
        let source_domain = domain("out-1", "ws-2");
        let source_key = source_domain.key();
        let order = ["win-1", "win-2"]
            .iter()
            .map(|window| migrate_carried((window, false, false)))
            .collect::<Vec<_>>();
        let seeded =
            crate::seed::seed_session(&owner, &gen_id, 7, &source_domain, &order).expect("seeds");
        let pre_revision = seeded.accepted_revision();
        engine.store_committed(source_key.clone(), seeded, 0);
        let target_domain = domain("out-2", "ws-2");
        let mut drifted = migrate_carried(("win-2", false, false));
        drifted.output = OutputId("out-9".to_owned());
        let homing = migrate_event_for(
            &owner,
            &gen_id,
            &source_domain,
            pre_revision,
            "win-1",
            vec![migrate_carried(("win-1", false, false)), drifted],
            &target_domain,
            "corr-migrate-homing",
        );
        match engine.handle(&homing) {
            CoreReply::Rejected { kind, .. } => assert_eq!(kind, "cross-domain-mismatch"),
            other => panic!("off-source homing must refuse, got {other:?}"),
        }
        let dangling = migrate_event_for(
            &owner,
            &gen_id,
            &source_domain,
            pre_revision,
            "win-z",
            order,
            &target_domain,
            "corr-migrate-dangling",
        );
        match engine.handle(&dangling) {
            CoreReply::Rejected { kind, .. } => assert_eq!(kind, "focus-mismatch"),
            other => panic!("dangling focus must refuse, got {other:?}"),
        }
        assert!(engine.contains(&source_key));
        assert!(!engine.contains(&target_domain.key()));
        assert_eq!(
            engine
                .session(&source_key)
                .expect("kept")
                .accepted_revision(),
            pre_revision
        );
        let _ = WindowId("win-1".to_owned());
    }

    #[test]
    fn migrate_workspace_sticky_active_echoes_null() {
        use crate::boundary::CoreReply;
        use crate::directional::WindowId;
        // Follow-only: a stayed sticky active echoes null like an absent
        // client. KDE still shows the migrated workspace after verified
        // arrival and never activates the source sticky on the target.
        let mut engine = Engine::new();
        let owner = OwnerId::parse("owner-a").expect("valid");
        let gen_id = GenerationId::parse("gen-1").expect("valid");
        engine.sync_binding(&owner, &gen_id);
        let source_domain = domain("out-1", "ws-2");
        let source_key = source_domain.key();
        let order = ["win-1", "win-s"]
            .iter()
            .map(|window| migrate_carried((window, false, false)))
            .collect::<Vec<_>>();
        let mut seeded =
            crate::seed::seed_session(&owner, &gen_id, 7, &source_domain, &order).expect("seeds");
        migrate_float_adopted(&mut seeded, &owner, &gen_id, &[], None);
        {
            use crate::contract::Observation;
            use crate::session::{ObservedWindow, SessionObservation};
            let base = seeded.accepted_revision();
            let mut windows: Vec<ObservedWindow> = seeded
                .snapshot()
                .windows
                .iter()
                .map(|link| ObservedWindow {
                    window: link.window.clone(),
                    output: link.output.clone(),
                    workspace: link.workspace.clone(),
                    floating: false,
                    fullscreen: false,
                    maximized: false,
                    sticky: link.window.0 == "win-s",
                    fixed_auto: false,
                    fixed_suppress: false,
                    hints: crate::size_hints::WindowSizeHints::none(),
                })
                .collect();
            windows.extend(seeded.exception_observed());
            seeded
                .converge_observation(
                    &SessionObservation {
                        observation: Observation::new(owner.clone(), gen_id.clone(), base, 7),
                        windows,
                    },
                    Some(&WindowId("win-s".to_owned())),
                )
                .expect("sticky converges");
        }
        let pre_revision = seeded.accepted_revision();
        engine.store_committed(source_key.clone(), seeded, 0);
        let target_domain = domain("out-2", "ws-2");
        let target_key = target_domain.key();
        let carried = vec![
            migrate_carried(("win-1", false, false)),
            migrate_carried(("win-s", false, true)),
        ];
        let event = migrate_event_for(
            &owner,
            &gen_id,
            &source_domain,
            pre_revision,
            "win-s",
            carried,
            &target_domain,
            "corr-migrate-sticky-active",
        );
        let plan = match engine.handle(&event) {
            CoreReply::MigrateWorkspace(plan) => plan,
            other => panic!("sticky-active migration must plan, got {other:?}"),
        };
        assert_eq!(plan.geometry.len(), 1);
        assert_eq!(plan.focus_domain, None);
        assert_eq!(plan.active_window, None, "stayed sticky never echoes");
        let moved = engine.session(&target_key).expect("moved");
        assert_eq!(moved.snapshot().windows.len(), 1);
        let stay = engine.session(&source_key).expect("residual");
        assert!(
            stay.snapshot()
                .windows
                .iter()
                .any(|link| link.window.0 == "win-s"),
            "sticky stays homed on source"
        );
    }

    #[test]
    fn startup_topology_describes_axes_and_nested_shares() {
        use crate::directional::{Axis, NodeId};
        let leaf = |id: &str| Node::Leaf {
            id: NodeId::from(id),
        };
        assert_eq!(describe_topology(&leaf("a")), (1, "L".to_owned()));
        let tree = Node::Group {
            id: NodeId::from("root"),
            axis: Axis::Horizontal,
            children: vec![
                leaf("a"),
                Node::Group {
                    id: NodeId::from("g"),
                    axis: Axis::Vertical,
                    children: vec![leaf("b"), leaf("c")],
                    shares: vec![1, 1],
                },
            ],
            shares: vec![1, 1],
        };
        assert_eq!(
            describe_topology(&tree),
            (3, "H[1,1](L,V[1,1](L,L))".to_owned())
        );
        let wide = Node::Group {
            id: NodeId::from("root"),
            axis: Axis::Horizontal,
            children: (0..20).map(|i| leaf(&format!("l{i}"))).collect(),
            shares: (1..=20).collect(),
        };
        let (leaves, topology) = describe_topology(&wide);
        assert_eq!(leaves, 20);
        assert!(topology.len() <= STARTUP_TRACE_MAX_TOPOLOGY);
    }

    #[test]
    fn fresh_traces_start_empty() {
        let engine = Engine::new();
        assert!(engine.last_startup_fit_trace().is_none());
        assert!(engine.last_send_placement().is_none());
    }
}

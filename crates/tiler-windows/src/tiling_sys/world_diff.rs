#![forbid(unsafe_code)]
//! Offline host-diff/feedback facade (W2, corrected).
//!
//! Test-only actuator over typed [`WorldReply`](tiler_core::world_report::WorldReply)
//! facts against a fresh offline native facade. It never touches the running
//! [`TileLoop`](super::TileLoop) owner, the recovery ledger, or the desktop:
//! all setters and proof re-reads go through [`NativeOps`], implemented here
//! by the in-memory [`FakeNative`]. Geometry converges from the core reply,
//! never from a second policy: this module honors per-row authority, scope
//! authority, holds, transfers, creations, removals, views, migrations, and
//! one-shot effects exactly as the reply states them.
//!
//! Source-first inputs (M1): [`FreshHost::from_sources`] converts the real
//! per-tick sources over the accepted W1 capture boundary. Window identity
//! comes from [`WindowKey`](crate::workspace::WindowKey) (never HWND alone),
//! safety facts from the real
//! [`ObservedWindow`](super::observe::ObservedWindow)/[`RetainedRow`](super::retained::RetainedRow)
//! slices, classification (hidden, iconic, unreadable) from the same owner
//! tables capture used, and frames/homing from captured rows. Lifetimes with
//! no provable facts carry `facts: None` and hold; withheld or never
//! inventoried tokens carry no baseline and hold as unknown. Fixtures diverge
//! live facts per flag to prove each fence independently.
//!
//! Native rules reused rather than reinvented:
//! - [`capture_facts_rideable`](super::world_capture::capture_facts_rideable):
//!   eligible or overlay facts ride; every other `classify` refusal has no
//!   window authority lane.
//! - [`readback_outcome`](crate::tiling::readback_outcome) plus the
//!   [`RefusedTracker`](crate::tiling::RefusedTracker) clamp/transient lanes:
//!   a successful write whose readback still mismatches is an app-held size;
//!   a failed setter is a bounded-backoff transient; an exact match clears
//!   both lanes. Identical intent stays suppressed; a real target,
//!   constraint, or observation change re-arms. Ordinary geometry emits no
//!   feedback at all: refusal memory lives in the tracker and observations.
//! - Feedback is emitted ONLY with operation ids the core actually consumes:
//!   retained window/view/migration transfer ids (see
//!   `WorldEngine::settle_retained`) and retained explicit float-placement
//!   ids (see `WorldEngine::settle_floats`). Creation, removal, and effect
//!   outcomes are proven by read-back and counted in stats; inventing
//!   `prop-*`/`remove-*`/`create-*` feedback would ride reports pointlessly
//!   since the core matches feedback by operation id only and ignores the
//!   rest.
//!
//! Freshness and proof rules (shared core contract):
//! - The reply must be ready, bound to the current adapter/core generation,
//!   and based on the CURRENT watermark, never a dispatched copy. Desired
//!   acceptance is strictly newer than the last applied revision
//!   ([`DiffGate::note_applied`]); anything else authorizes no setter.
//! - Fresh frames, homing, views, and safety re-read through [`NativeOps`]
//!   immediately before every setter; a gesture barrier suspends geometry.
//!   External invalidation is polled before every setter and stops remaining
//!   unsafe writes, including after geometry/float/create/remove/view/
//!   migration/effect writes.
//! - Possibly-executed transfers, float placements, views, and effects arm
//!   BEFORE the first native setter and never repeat while ambiguous.
//!   Refused-proven settles only on fresh proof that nothing happened plus
//!   an explicit native refusal. Third-location proof settles
//!   superseded-external instead of fighting the user. Follow-view failure
//!   holds the window transfer as possibly-executed: a view outcome is never
//!   claimed on a window transfer id, and focus requires the exact live view.
//! - Desired absence is removal eligibility only: the facade requires a
//!   managed, non-displaced binding plus fresh occupant/visibility proof.
//!   Unknown occupancy keeps.
//! - Stable logical creation binds at most one flight per id and adopts the
//!   binding strictly from direct setter results or later reply rows. An
//!   ambiguous create never steals an unrelated external workspace: binding
//!   recovery comes from the next report, never from enumeration guessing.
//!   Dependent membership waits for the binding; the logical token is never
//!   used as a native id.
//! - The writable frame target is `effective_frame` only: `allocation` is the
//!   canonical slot, never a write target.
//! - Partial results are reported honestly; nothing here claims atomicity.

use super::world_capture::{CapturedWorld, capture_facts_rideable};
use super::{ObservedWindow, RetainedRow, TileLoop};
use crate::tiling::{ReadbackOutcome, RefusedTracker, WindowFacts, readback_outcome};
use crate::workspace::WindowKey;
use std::collections::{BTreeSet, HashMap, HashSet};
use tiler_core::geometry::Rect;
use tiler_core::world_report::{
    AttemptFeedback, AttemptKind, AttemptOutcome, HoldReason, ScopeAuthority, TransferKind,
    TransferTarget, WorkspaceBinding, WorldReply,
};

/// Fresh offline window facts for one managed lifetime: the observation-time
/// baseline plus the shape of every fresh re-read through [`NativeOps`].
///
/// Each flag has a distinct native source, never a guessed duplicate:
/// - `key`/`token`: stable member identity and Engine token.
/// - `frame`: last proven frame (`None` = unreadable, never fabricated).
/// - `facts`: the raw native read (`None` = unproven; holds). Distinct from
///   `iconic`/`hidden`: `facts.minimized` is the native minimized bit while
///   `iconic` is the adapter minimized-omission classification; `facts.visible`
///   is native visibility while `hidden` is the adapter hide-ledger state.
/// - `output`/`workspace`: homing at observation time.
/// - `iconic`: adapter minimized omission or explicit minimized entry.
/// - `hidden`: adapter hide-ledger membership.
/// - `sticky`/`floating`/`fit_excluded`: adapter intent lanes from the
///   accepted capture rows (float placement intent never authorizes tiled
///   writes; only a stable `frame_operation` token does).
/// - `live_tag`: live visible-membership tag; must equal the stored tag and
///   be nonempty, mirroring the production visible-lifetime gate.
#[derive(Debug, Clone)]
pub(super) struct FreshWindow {
    pub(super) key: WindowKey,
    pub(super) token: String,
    pub(super) frame: Option<Rect>,
    pub(super) facts: Option<WindowFacts>,
    pub(super) output: String,
    pub(super) workspace: String,
    pub(super) iconic: bool,
    pub(super) hidden: bool,
    pub(super) sticky: bool,
    pub(super) floating: bool,
    pub(super) fit_excluded: bool,
    pub(super) live_tag: Option<String>,
}

/// Observation-time snapshot behind one diff pass, plus the stored
/// visible-membership tags the lifetime gate rechecks against, the live
/// output views, and the adapter-observed workspace mapping (logical/native
/// plus pin/displacement/managed flags) that removal ownership rechecks
/// against. The mapping is report-carried knowledge; occupancy, visibility,
/// and enumeration stay live reads.
#[derive(Debug, Clone, Default)]
pub(super) struct FreshHost {
    pub(super) baseline: HashMap<String, FreshWindow>,
    pub(super) stored_tags: HashMap<String, String>,
    /// Live output views at observation time (`output -> native workspace`),
    /// seeding proof re-reads.
    pub(super) views: HashMap<String, String>,
    pub(super) bindings: Vec<WorkspaceBinding>,
}

impl FreshHost {
    /// Source-first conversion over the accepted W1 capture boundary plus
    /// the real per-tick sources it was built from. See the module docs for
    /// the per-field sources.
    pub(super) fn from_sources(
        state: &TileLoop,
        observed: &[ObservedWindow],
        retained: &[RetainedRow],
        captured: &CapturedWorld,
    ) -> Self {
        let mut facts_of: HashMap<&str, WindowFacts> = HashMap::new();
        for window in observed {
            facts_of.insert(window.token.as_str(), window.facts);
        }
        for row in retained {
            if let Some(facts) = row.facts {
                facts_of.entry(row.token.as_str()).or_insert(facts);
            }
        }
        let mut stored_tags: HashMap<String, String> = HashMap::new();
        for (key, token) in &state.member_tokens {
            if let Some(tag) = state.member_tags.get(key) {
                stored_tags.insert(token.clone(), tag.clone());
            }
        }
        let mut baseline: HashMap<String, FreshWindow> = HashMap::new();
        for domain in &captured.domains {
            for row in &domain.windows {
                let token = row.window.0.as_str();
                let Some(key) = state.member_tokens.iter().find_map(|(key, id)| {
                    if id.as_str() == token {
                        Some(key)
                    } else {
                        None
                    }
                }) else {
                    // No provable lifetime: unknown baselines hold.
                    continue;
                };
                let hidden = state
                    .workspaces
                    .member_loc(key)
                    .is_some_and(|loc| loc.hidden);
                let iconic = state.minimized_omitted.contains(key);
                baseline.insert(
                    token.to_owned(),
                    FreshWindow {
                        key: key.clone(),
                        token: token.to_owned(),
                        frame: Some(row.rect),
                        facts: facts_of.get(token).copied(),
                        output: row.output.0.clone(),
                        workspace: row.workspace.0.clone(),
                        iconic,
                        hidden,
                        sticky: row.sticky,
                        floating: row.floating,
                        fit_excluded: row.fit_excluded,
                        live_tag: state.member_tags.get(key).cloned(),
                    },
                );
            }
        }
        for entry in &captured.minimized {
            let token = entry.window.0.as_str();
            if baseline.contains_key(token) {
                continue;
            }
            let Some(key) = state.member_tokens.iter().find_map(|(key, id)| {
                if id.as_str() == token {
                    Some(key)
                } else {
                    None
                }
            }) else {
                continue;
            };
            let hidden = state
                .workspaces
                .member_loc(key)
                .is_some_and(|loc| loc.hidden);
            baseline.insert(
                token.to_owned(),
                FreshWindow {
                    key: key.clone(),
                    token: token.to_owned(),
                    frame: None,
                    facts: facts_of.get(token).copied(),
                    output: entry.output.0.clone(),
                    workspace: entry.workspace.0.clone(),
                    iconic: true,
                    hidden,
                    sticky: state.sticky.contains_key(key),
                    floating: false,
                    fit_excluded: false,
                    live_tag: state.member_tags.get(key).cloned(),
                },
            );
        }
        let mut views: HashMap<String, String> = HashMap::new();
        for topology in &captured.topology {
            views.insert(topology.output.0.clone(), topology.current_native.clone());
        }
        Self {
            baseline,
            stored_tags,
            views,
            bindings: captured.bindings.clone(),
        }
    }
}

/// Freshness gate for one diff pass. Desired acceptance ([`Self::note_applied`])
/// is the core revision the sequencer applied; host results (per-property
/// read-back proof) are owned by the facade stats and feedback. The two are
/// never conflated: a duplicate revision (same number, already applied)
/// authorizes no setter.
#[derive(Debug, Clone)]
pub(super) struct DiffGate {
    pub(super) adapter: String,
    pub(super) generation: String,
    pub(super) current_obs: u64,
    pub(super) last_rev: u64,
    pub(super) has_accepted: bool,
}

impl DiffGate {
    pub(super) fn fresh_reply(&self, reply: &WorldReply) -> bool {
        reply.ready
            && reply.adapter_incarnation == self.adapter
            && reply.core_generation == self.generation
            && reply.based_on_observation_seq == self.current_obs
            && (!self.has_accepted || reply.state_revision > self.last_rev)
    }

    /// Record one sequencer-applied core revision as the desired baseline.
    /// Called by the owner after [`AckOutcome::Applied`](super::world_sequencer::AckOutcome::Applied);
    /// host application results never move this watermark.
    pub(super) fn note_applied(&mut self, reply: &WorldReply) {
        self.last_rev = reply.state_revision;
        self.has_accepted = true;
    }

    /// Generation reset: new core namespace, no carried ordering. Caller
    /// clears the generation-scoped [`HostDiff`] tables alongside.
    pub(super) fn reset_generation(&mut self, generation: &str) {
        self.generation = generation.to_owned();
        self.current_obs = 0;
        self.last_rev = 0;
        self.has_accepted = false;
    }
}

/// Native proof/mutation boundary. Reads are fresh re-reads, never cached
/// observations; writes return `Ok(true)` accepted, `Ok(false)` explicit
/// refusal, `Err(())` throw/ambiguous (may have executed). Occupant,
/// visibility, and enumeration reads are always live: the facade keeps no
/// cache of them, so removal rechecks observe racing host changes.
pub(super) trait NativeOps {
    fn read_window(&self, token: &str) -> Option<FreshWindow>;
    fn read_frame(&self, token: &str) -> Option<Rect>;
    fn read_homing(&self, token: &str) -> Option<(String, String)>;
    fn read_view(&self, output: &str) -> Option<String>;
    fn is_foreground(&self, token: &str) -> bool;
    fn gesture_barrier(&self) -> bool;
    fn write_frame(&mut self, token: &str, rect: Rect) -> Result<bool, ()>;
    fn move_membership(&mut self, token: &str, out: &str, ws: &str) -> Result<bool, ()>;
    fn create_workspace(&mut self, logical: &str) -> Result<Option<String>, ()>;
    fn remove_workspace(&mut self, native: &str) -> Result<bool, ()>;
    fn switch_view(&mut self, output: &str, native: &str) -> Result<bool, ()>;
    fn focus_window(&mut self, token: &str) -> Result<bool, ()>;
    fn occupants(&self, native: &str) -> (usize, bool);
    fn is_visible(&self, native: &str) -> Option<bool>;
    fn native_ids(&self) -> BTreeSet<String>;
    fn workspace_count(&self) -> usize;
    /// One external invalidation consumed per call: racing native input that
    /// arrived mid-apply. The facade polls this before every setter.
    fn poll_external_invalidation(&mut self) -> bool {
        false
    }
}

/// In-memory offline native. Seeded from a [`FreshHost`] baseline (windows
/// plus views); writes mutate the live maps so proof re-reads observe them.
/// Enumeration inputs (desktops, occupants, visibility) are staged per
/// fixture and always read live.
#[derive(Debug, Default)]
pub(super) struct FakeNative {
    pub(super) windows: HashMap<String, FreshWindow>,
    pub(super) views: HashMap<String, String>,
    pub(super) desktops: BTreeSet<String>,
    pub(super) occupants: HashMap<String, (usize, bool)>,
    pub(super) visible: HashMap<String, Option<bool>>,
    pub(super) foreground: Option<String>,
    pub(super) gesture: bool,
    pub(super) fail_frame: HashSet<String>,
    pub(super) throw_frame: HashSet<String>,
    /// Setter accepts but the app holds its frame: the write reports
    /// success while the read-back still mismatches (native app-clamp
    /// lane input).
    pub(super) clamp_frame: HashSet<String>,
    pub(super) fail_membership: HashSet<String>,
    pub(super) throw_membership: HashSet<String>,
    pub(super) fail_view: HashSet<String>,
    pub(super) throw_view: HashSet<String>,
    pub(super) fail_focus: HashSet<String>,
    pub(super) throw_focus: HashSet<String>,
    pub(super) create_result: Option<Option<String>>,
    pub(super) throw_create: bool,
    pub(super) fail_remove: HashSet<String>,
    pub(super) pending_invalidation: bool,
    pub(super) fire_after_writes: Option<usize>,
    pub(super) writes_done: usize,
    pub(super) frame_writes: usize,
    pub(super) membership_writes: usize,
}

impl FakeNative {
    pub(super) fn seeded(host: &FreshHost) -> Self {
        let mut native = Self::default();
        for (token, window) in &host.baseline {
            native.windows.insert(token.clone(), window.clone());
        }
        native.views.clone_from(&host.views);
        native
    }

    /// Arm one racing-input invalidation after `n` further successful
    /// setter calls. The facade consumes it through
    /// [`NativeOps::poll_external_invalidation`] before its next setter.
    pub(super) fn arm_invalidation_after_writes(&mut self, n: usize) {
        self.fire_after_writes = Some(self.writes_done.saturating_add(n));
    }

    /// Fire one racing-input invalidation immediately (consumed by the next
    /// facade poll).
    pub(super) fn fire_pending_invalidation(&mut self) {
        self.pending_invalidation = true;
    }

    fn note_write(&mut self) {
        self.writes_done += 1;
        if self
            .fire_after_writes
            .is_some_and(|at| self.writes_done >= at)
        {
            self.fire_after_writes = None;
            self.pending_invalidation = true;
        }
    }
}

impl NativeOps for FakeNative {
    fn read_window(&self, token: &str) -> Option<FreshWindow> {
        self.windows.get(token).cloned()
    }
    fn read_frame(&self, token: &str) -> Option<Rect> {
        self.windows.get(token).and_then(|w| w.frame)
    }
    fn read_homing(&self, token: &str) -> Option<(String, String)> {
        self.windows
            .get(token)
            .map(|w| (w.output.clone(), w.workspace.clone()))
    }
    fn read_view(&self, output: &str) -> Option<String> {
        self.views.get(output).cloned()
    }
    fn is_foreground(&self, token: &str) -> bool {
        self.foreground.as_deref() == Some(token)
    }
    fn gesture_barrier(&self) -> bool {
        self.gesture
    }
    fn write_frame(&mut self, token: &str, rect: Rect) -> Result<bool, ()> {
        if self.throw_frame.contains(token) {
            return Err(());
        }
        if self.fail_frame.contains(token) {
            self.note_write();
            return Ok(false);
        }
        if !self.clamp_frame.contains(token)
            && let Some(window) = self.windows.get_mut(token)
        {
            window.frame = Some(rect);
        }
        self.frame_writes += 1;
        self.note_write();
        Ok(true)
    }
    fn move_membership(&mut self, token: &str, out: &str, ws: &str) -> Result<bool, ()> {
        if self.throw_membership.contains(token) {
            return Err(());
        }
        if self.fail_membership.contains(token) {
            self.note_write();
            return Ok(false);
        }
        if let Some(window) = self.windows.get_mut(token) {
            window.output = out.to_owned();
            window.workspace = ws.to_owned();
        }
        self.membership_writes += 1;
        self.note_write();
        Ok(true)
    }
    fn create_workspace(&mut self, logical: &str) -> Result<Option<String>, ()> {
        if self.throw_create {
            return Err(());
        }
        if let Some(result) = self.create_result.clone() {
            if let Some(native) = result.clone() {
                self.desktops.insert(native.clone());
                self.occupants.entry(native.clone()).or_insert((0, true));
                self.visible.entry(native.clone()).or_insert(Some(false));
            }
            let _ = logical;
            self.note_write();
            return Ok(result);
        }
        let native = format!("ws-created-{logical}");
        self.desktops.insert(native.clone());
        self.occupants.entry(native.clone()).or_insert((0, true));
        self.visible.entry(native.clone()).or_insert(Some(false));
        self.note_write();
        Ok(Some(native))
    }
    fn remove_workspace(&mut self, native: &str) -> Result<bool, ()> {
        if self.fail_remove.contains(native) {
            self.note_write();
            return Ok(false);
        }
        let removed = self.desktops.remove(native);
        self.note_write();
        Ok(removed)
    }
    fn switch_view(&mut self, output: &str, native: &str) -> Result<bool, ()> {
        let key = format!("{output}/{native}");
        if self.throw_view.contains(&key) {
            return Err(());
        }
        if self.fail_view.contains(&key) {
            self.note_write();
            return Ok(false);
        }
        self.views.insert(output.to_owned(), native.to_owned());
        self.note_write();
        Ok(true)
    }
    fn focus_window(&mut self, token: &str) -> Result<bool, ()> {
        if self.throw_focus.contains(token) {
            return Err(());
        }
        if self.fail_focus.contains(token) {
            self.note_write();
            return Ok(false);
        }
        self.foreground = Some(token.to_owned());
        self.note_write();
        Ok(true)
    }
    fn occupants(&self, native: &str) -> (usize, bool) {
        self.occupants.get(native).copied().unwrap_or((0, false))
    }
    fn is_visible(&self, native: &str) -> Option<bool> {
        self.visible.get(native).copied().unwrap_or(None)
    }
    fn native_ids(&self) -> BTreeSet<String> {
        self.desktops.clone()
    }
    fn workspace_count(&self) -> usize {
        self.desktops.len()
    }
    fn poll_external_invalidation(&mut self) -> bool {
        std::mem::take(&mut self.pending_invalidation)
    }
}

/// One-flight logical creation state. The binding is adopted strictly from a
/// direct setter result or a later reply row naming this logical id; the
/// facade never binds by enumeration guessing, so an unrelated external
/// workspace is never stolen.
#[derive(Debug, Clone, Default)]
struct CreationFlight {
    bound: Option<String>,
}

/// Stable logical creation table: at most one flight per logical id.
#[derive(Debug, Default)]
pub(super) struct CreationTable {
    flights: HashMap<String, CreationFlight>,
}

impl CreationTable {
    pub(super) fn is_bound(&self, logical: &str) -> bool {
        self.flights
            .get(logical)
            .is_some_and(|flight| flight.bound.is_some())
    }

    pub(super) fn is_inflight(&self, logical: &str) -> bool {
        self.flights.contains_key(logical)
    }

    pub(super) fn start(&mut self, logical: &str) {
        self.flights.entry(logical.to_owned()).or_default();
    }

    pub(super) fn bind(&mut self, logical: &str, native: String) {
        self.flights.entry(logical.to_owned()).or_default().bound = Some(native);
    }
}

/// At-most-once and ambiguity state for transfers, floats, views, effects.
/// Operation-scoped entries (ambiguous, spent, applied) live and die with the
/// core generation; [`HostDiff::reset_generation`] clears them.
#[derive(Debug, Default)]
pub(super) struct EffectTracker {
    pub(super) ambiguous: HashMap<String, String>,
    pub(super) spent_effects: HashSet<String>,
    pub(super) spent_floats: HashSet<String>,
    /// Distinct local view-phase memory per owning core id: the retained
    /// view id itself for standalone views, the window parent for follow,
    /// the migration parent for the target view, and the actual refill id
    /// for refill. Never cleared by membership arrival; only operation
    /// retirement/reset retires a phase. Every view phase is armed (marked)
    /// immediately before its setter; exact target proof may settle without
    /// a dispatch.
    pub(super) spent_views: HashSet<String>,
    pub(super) applied_window: HashMap<String, (String, String, String)>,
    pub(super) applied_view: HashMap<String, (String, String)>,
    pub(super) applied_float: HashMap<String, (String, Rect)>,
}

impl EffectTracker {
    pub(super) fn is_ambiguous(&self, op: &str) -> bool {
        self.ambiguous.contains_key(op)
    }
    pub(super) fn arm(&mut self, op: &str, context: String) {
        self.ambiguous.entry(op.to_owned()).or_insert(context);
    }
    fn settle_applied(&mut self, op: &str) {
        self.ambiguous.remove(op);
    }
    /// Mark one effect spent BEFORE dispatch. Returns false when already
    /// spent: duplicates never reapply.
    pub(super) fn mark_effect_spent(&mut self, id: &str) -> bool {
        self.spent_effects.insert(id.to_owned())
    }
    /// Mark one float placement spent BEFORE dispatch. Returns false when
    /// already spent for this operation.
    pub(super) fn mark_float_spent(&mut self, op: &str) -> bool {
        self.spent_floats.insert(op.to_owned())
    }
    /// View-phase fence: true once the owning core id attempted its view
    /// setter (follow parent, migration parent, refill id, or retained view
    /// id). Late membership arrival never clears this.
    pub(super) fn is_view_spent(&self, owning: &str) -> bool {
        self.spent_views.contains(owning)
    }
    /// Arm one view phase immediately before its setter. Returns false when
    /// already spent: duplicates never reapply.
    pub(super) fn mark_view_spent(&mut self, owning: &str) -> bool {
        self.spent_views.insert(owning.to_owned())
    }
    fn clear_generation(&mut self) {
        self.ambiguous.clear();
        self.spent_effects.clear();
        self.spent_floats.clear();
        self.spent_views.clear();
        self.applied_window.clear();
        self.applied_view.clear();
        self.applied_float.clear();
    }
}

/// One offline diff unit: freshness gate plus the refusal, creation, and
/// at-most-once tables behind [`apply_world_reply`]. All maps stay bounded:
/// [`Self::prune`] retains entries against the current reply and baseline
/// every pass, and [`Self::reset_generation`] clears generation-scoped state.
#[derive(Debug)]
pub(super) struct HostDiff {
    pub(super) gate: DiffGate,
    pub(super) refused: RefusedTracker,
    pub(super) creations: CreationTable,
    pub(super) effects: EffectTracker,
}

impl HostDiff {
    pub(super) fn new(adapter: &str, generation: &str) -> Self {
        Self {
            gate: DiffGate {
                adapter: adapter.to_owned(),
                generation: generation.to_owned(),
                current_obs: 0,
                last_rev: 0,
                has_accepted: false,
            },
            refused: RefusedTracker::default(),
            creations: CreationTable::default(),
            effects: EffectTracker::default(),
        }
    }

    /// Generation reset: new core namespace clears operation-scoped tables
    /// (reused operation ids must never inherit spent/ambiguous/applied
    /// state) and the refusal memory (no stale suppression under a fresh
    /// world). The caller re-derives the watermark from the new bootstrap.
    pub(super) fn reset_generation(&mut self, generation: &str) {
        self.gate.reset_generation(generation);
        self.refused = RefusedTracker::default();
        self.creations = CreationTable::default();
        self.effects.clear_generation();
    }

    /// Bound every table against the current reply and baseline: refusal
    /// entries survive only for live reply/baseline lifetimes, spent and
    /// ambiguous entries only for operation ids the reply still carries,
    /// applied proofs only for baselined lifetimes (view proofs additionally
    /// only for live outputs and live operation ids, since hotplug can
    /// retire an output while its proof entry would otherwise linger), and
    /// creation flights only for logical ids the reply still lists.
    fn prune(&mut self, reply: &WorldReply, fresh: &FreshHost) {
        let mut live_tokens: Vec<&str> = fresh.baseline.keys().map(String::as_str).collect();
        for row in &reply.windows {
            if !live_tokens.contains(&row.lifetime.0.as_str()) {
                live_tokens.push(row.lifetime.0.as_str());
            }
        }
        self.refused.retain(&live_tokens);
        let mut live_ops: HashSet<&str> = HashSet::new();
        for effect in &reply.effects {
            live_ops.insert(effect.id.as_str());
        }
        for transfer in &reply.transfers {
            live_ops.insert(transfer.operation_id.as_str());
        }
        for row in &reply.windows {
            if let Some(op) = row.frame_operation.as_deref()
                && !op.is_empty()
            {
                live_ops.insert(op);
            }
        }
        for row in &reply.workspaces {
            let logical = row
                .logical_id
                .as_deref()
                .filter(|id| !id.is_empty())
                .unwrap_or(row.id.as_str());
            live_ops.insert(logical);
        }
        self.effects
            .spent_effects
            .retain(|id| live_ops.contains(id.as_str()));
        self.effects
            .spent_floats
            .retain(|op| live_ops.contains(op.as_str()));
        self.effects
            .spent_views
            .retain(|op| live_ops.contains(op.as_str()));
        self.effects
            .ambiguous
            .retain(|op, _| live_ops.contains(op.as_str()));
        self.effects
            .applied_window
            .retain(|token, _| fresh.baseline.contains_key(token));
        self.effects
            .applied_float
            .retain(|token, _| fresh.baseline.contains_key(token));
        // View proofs are keyed by output (not by window token), so neither
        // baseline retention above covers them: keep only proofs whose
        // output is still observed and whose operation the reply still
        // carries. A retired hotplug output or a settled operation must not
        // linger and suppress a later same-output view outcome.
        let mut live_outputs: HashSet<&str> = HashSet::new();
        for output in fresh.views.keys() {
            live_outputs.insert(output.as_str());
        }
        self.effects.applied_view.retain(|output, (op, _)| {
            live_outputs.contains(output.as_str()) && live_ops.contains(op.as_str())
        });
        let mut live_logicals: HashSet<&str> = HashSet::new();
        for row in &reply.workspaces {
            let logical = row
                .logical_id
                .as_deref()
                .filter(|id| !id.is_empty())
                .unwrap_or(row.id.as_str());
            if !logical.is_empty() {
                live_logicals.insert(logical);
            }
        }
        self.creations
            .flights
            .retain(|logical, _| live_logicals.contains(logical.as_str()));
    }
}

/// Honest outcome of one diff pass. `partial` is true when at least one
/// write landed while at least one row held: never atomic.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct ApplyStats {
    pub(super) geometry_writes: usize,
    pub(super) membership_writes: usize,
    pub(super) view_writes: usize,
    pub(super) creations: usize,
    pub(super) removals: usize,
    pub(super) effects: usize,
    pub(super) held: usize,
    pub(super) stopped_mid_apply: bool,
    pub(super) partial: bool,
}

/// Scope authority for one `(output, workspace)` domain, matching the core
/// envelope: `ScopeStatus.scope` is `"{output}/{workspace}"`, and omission
/// inside a held or untouched scope authorizes no write or retirement.
fn scope_controlled(reply: &WorldReply, output: &str, workspace: &str) -> bool {
    let scope = format!("{output}/{workspace}");
    reply
        .scopes
        .iter()
        .any(|status| status.scope == scope && status.authority == ScopeAuthority::Controlled)
}

/// Freshness plus racing-input poll before a setter. Consumes at most one
/// pending external invalidation; on any staleness the caller stops before
/// remaining unsafe writes.
fn ensure_fresh(
    reply: &WorldReply,
    gate: &mut DiffGate,
    entry_obs: u64,
    ops: &mut dyn NativeOps,
) -> bool {
    if !gate.fresh_reply(reply) || gate.current_obs != entry_obs {
        return false;
    }
    if ops.poll_external_invalidation() {
        gate.current_obs = gate.current_obs.saturating_add(1);
        return false;
    }
    true
}

/// Setter gate verdict: pass, hold this row narrowly, or stop the pass.
/// Fencing and identity failures stop (global trust); row-local safety
/// drift holds (one raced row never halts the world).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Gate {
    Pass,
    Hold,
    Stop,
}

/// What one setter needs from the live re-read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SetterKind {
    /// Tiled frame write: any gesture suspends, overlays and float lanes
    /// never take automatic writes.
    Frame,
    /// Explicit float placement: no automatic authority, so only the stable
    /// token plus strict eligibility authorizes.
    Float,
    /// Membership and focus: overlays may move or focus, ineligible safety
    /// facts may not.
    Membership,
}

fn lifetime_ok(fresh: &FreshWindow, stored: &HashMap<String, String>) -> bool {
    if fresh.key.hwnd == 0 || fresh.key.pid == 0 || fresh.key.creation.is_empty() {
        return false;
    }
    let Some(live) = fresh.live_tag.as_deref() else {
        return false;
    };
    if live.is_empty() {
        return false;
    }
    stored.get(&fresh.token).is_some_and(|tag| tag == live)
}

fn overlay_guarded(window: &FreshWindow) -> bool {
    window
        .facts
        .is_some_and(|facts| facts.captionless_fullscreen || facts.maximized)
        || window.sticky
        || window.floating
        || window.fit_excluded
}

/// Full fresh safety/identity gate immediately before a setter, over the
/// LIVE re-read (never the observation preimage alone):
/// - current watermark; any gesture suspends frame writes;
/// - the live lifetime still names the same key with a matching stored tag
///   (a recycled HWND authorizes nothing): fencing failures stop;
/// - no fresh hidden/iconic/unreadable state and no fresh facts refusal: the
///   full [`classify`](crate::tiling::classify) runs on the live read, so
///   cloaked, elevated, shell, tool, owned, dialog, no-activate, hidden, and
///   minimized facts all fail closed, while fullscreen/maximized overlays
///   ride their reserved lane: row-local safety drift holds;
/// - [`SetterKind::Frame`] additionally requires a non-guarded live window
///   (no overlay, sticky, float, or fit-excluded frame writes);
///   [`SetterKind::Float`] requires strict eligibility (no overlay target);
///   [`SetterKind::Membership`] allows overlays to move and focus.
fn pre_setter_ok(
    gate: &DiffGate,
    entry_obs: u64,
    ops: &dyn NativeOps,
    base: &FreshWindow,
    stored: &HashMap<String, String>,
    kind: SetterKind,
) -> Gate {
    if gate.current_obs != entry_obs {
        return Gate::Stop;
    }
    if matches!(kind, SetterKind::Frame | SetterKind::Float) && ops.gesture_barrier() {
        return Gate::Stop;
    }
    let Some(live) = ops.read_window(&base.token) else {
        return Gate::Hold;
    };
    if live.key != base.key {
        return Gate::Stop;
    }
    if !lifetime_ok(&live, stored) {
        return Gate::Stop;
    }
    if live.hidden || live.iconic || live.frame.is_none() {
        return Gate::Hold;
    }
    let Some(facts) = live.facts else {
        return Gate::Hold;
    };
    match kind {
        SetterKind::Frame => {
            if !capture_facts_rideable(&facts) {
                return Gate::Hold;
            }
            if overlay_guarded(&live) {
                return Gate::Hold;
            }
        }
        SetterKind::Float => {
            // Strict eligibility: the full classify gate (cloaked,
            // elevated, shell, tool, owned, dialog, no-activate, hidden,
            // minimized, plus fullscreen/maximized overlay protection) and
            // the live adapter lanes. Ordinary floating intent alone is
            // allowed: only sticky and fit-excluded hold here.
            if crate::tiling::classify(&facts).is_err() {
                return Gate::Hold;
            }
            if live.sticky || live.fit_excluded {
                return Gate::Hold;
            }
        }
        SetterKind::Membership => {
            if !capture_facts_rideable(&facts) {
                return Gate::Hold;
            }
        }
    }
    Gate::Pass
}

/// Membership/view proof over a fresh re-read: the live lifetime still
/// matches (key plus stored tag) AND the live homing names the expected
/// target. The setter boolean is never proof.
fn homing_proven(
    ops: &dyn NativeOps,
    base: &FreshWindow,
    stored: &HashMap<String, String>,
    output: &str,
    workspace: &str,
) -> bool {
    let Some(live) = ops.read_window(&base.token) else {
        return false;
    };
    if live.key != base.key || !lifetime_ok(&live, stored) {
        return false;
    }
    ops.read_homing(&base.token)
        .is_some_and(|(out, ws)| out == output && ws == workspace)
}

fn feedback(
    operation_id: &str,
    kind: AttemptKind,
    outcome: AttemptOutcome,
    hold: Option<HoldReason>,
) -> AttemptFeedback {
    AttemptFeedback {
        operation_id: operation_id.to_owned(),
        kind,
        outcome,
        hold,
    }
}

fn window_transfer_for<'a>(reply: &'a WorldReply, window: &str) -> Option<&'a TransferTarget> {
    reply.transfers.iter().find(|t| {
        matches!(t.kind, TransferKind::Window(_)) && {
            match &t.kind {
                TransferKind::Window(id) => id.0 == window,
                _ => false,
            }
        }
    })
}

/// Follow-view outcome for one window transfer whose membership proof
/// already arrived. The transfer id carries no view outcome: only the
/// transfer's ambiguous/confirmed state is reported on it.
enum FollowOutcome {
    Shown,
    Switched,
    /// First failed attempt: the caller replaces Applied with
    /// PossiblyExecuted once and the phase is now spent.
    HeldEmit,
    /// Already spent: silent hold, no dispatch and no repeat feedback.
    HeldSilent,
}

/// Third-location proof: a readable live view that is neither the
/// observation baseline nor the requested target proves newer external
/// input, never a refusal of this target.
fn view_third_location(proof: &Option<String>, baseline: Option<&String>, target: &str) -> bool {
    match proof {
        Some(view) => view != target && Some(view) != baseline,
        None => false,
    }
}

/// Shared view truth table for standalone and refill view attempts over
/// the exact-output proof:
/// - proof == target: Applied even if the setter threw;
/// - explicit false with unchanged readable preimage: RefusedProven;
/// - unchanged proof (old after success, or unresolved throw without
///   external change): PossiblyExecuted;
/// - readable changed proof that is neither the baseline nor the target
///   (newer external input during the setter): SupersededExternal.
fn settle_view_attempt(
    pre: &Option<String>,
    proof: &Option<String>,
    baseline: Option<&String>,
    target: &str,
    result: &Result<bool, ()>,
) -> AttemptOutcome {
    if proof.as_deref() == Some(target) {
        return AttemptOutcome::Applied;
    }
    if matches!(result, Ok(false))
        && pre.is_some()
        && pre == proof
        && pre.as_deref() != Some(target)
    {
        return AttemptOutcome::RefusedProven;
    }
    if pre == proof {
        return AttemptOutcome::PossiblyExecuted;
    }
    if view_third_location(proof, baseline, target) {
        return AttemptOutcome::SupersededExternal;
    }
    AttemptOutcome::PossiblyExecuted
}

#[allow(clippy::too_many_arguments)]
fn actuate_follow_view(
    reply: &WorldReply,
    gate: &mut DiffGate,
    entry_obs: u64,
    ops: &mut dyn NativeOps,
    stats: &mut ApplyStats,
    transfer: &TransferTarget,
    effects: &mut EffectTracker,
    _fresh: &FreshHost,
) -> FollowOutcome {
    let out = transfer.target_output.0.clone();
    let ws = transfer.target_workspace.clone();
    let parent = transfer.operation_id.clone();
    if ops.read_view(&out).as_deref() == Some(ws.as_str()) {
        return FollowOutcome::Shown;
    }
    // Late membership arrival never rearms a spent follow-view phase:
    // exact target proof above may settle without dispatch, otherwise an
    // attempted phase holds silently without another switch.
    if effects.is_view_spent(&parent) {
        stats.held += 1;
        return FollowOutcome::HeldSilent;
    }
    if !ensure_fresh(reply, gate, entry_obs, ops) {
        stats.stopped_mid_apply = true;
        return FollowOutcome::HeldEmit;
    }
    // Arm the follow-view phase immediately before dispatch so a
    // possibly-executed switch never repeats.
    if !effects.mark_view_spent(&parent) {
        stats.held += 1;
        return FollowOutcome::HeldSilent;
    }
    match ops.switch_view(&out, &ws) {
        Ok(_) if ops.read_view(&out).as_deref() == Some(ws.as_str()) => {
            stats.view_writes += 1;
            FollowOutcome::Switched
        }
        _ => {
            // Membership is proven but the follow view is not: hold the
            // transfer ambiguous without claiming a view outcome on its id.
            // Never RefusedProven here: the membership already landed, so a
            // view miss cannot prove the window transfer refused.
            stats.held += 1;
            FollowOutcome::HeldEmit
        }
    }
}

fn finalize(stats: &mut ApplyStats) {
    let writes = stats.geometry_writes
        + stats.membership_writes
        + stats.view_writes
        + stats.creations
        + stats.removals
        + stats.effects;
    stats.partial = writes > 0 && (stats.held > 0 || stats.stopped_mid_apply);
}

/// Diff one ready reply against fresh proof and actuate through `ops`.
///
/// The caller owns generation/observation advancement: `state.gate.current_obs`
/// is the CURRENT watermark at entry. Racing input is consumed through
/// [`NativeOps::poll_external_invalidation`] before every setter; within a
/// pass the facade stops before remaining unsafe writes when the watermark
/// moves. State tables stay bounded through [`HostDiff::prune`]; only
/// operation ids the core consumes ever produce feedback.
#[allow(clippy::too_many_lines)]
pub(super) fn apply_world_reply(
    state: &mut HostDiff,
    reply: &WorldReply,
    fresh: &FreshHost,
    ops: &mut dyn NativeOps,
) -> (ApplyStats, Vec<AttemptFeedback>) {
    let mut stats = ApplyStats::default();
    let mut out_feedback: Vec<AttemptFeedback> = Vec::new();
    let entry_obs = state.gate.current_obs;

    if !state.gate.fresh_reply(reply) {
        stats.held += 1;
        stats.stopped_mid_apply = true;
        return (stats, out_feedback);
    }
    state.prune(reply, fresh);

    // Logical ids without a native binding: dependent membership, view, and
    // migration targets naming one hold until the binding arrives.
    let mut unbound_logicals: HashSet<&str> = HashSet::new();
    for row in &reply.workspaces {
        if row.native_binding.is_none()
            && let Some(logical) = row.logical_id.as_deref()
            && !logical.is_empty()
        {
            unbound_logicals.insert(logical);
        }
    }

    let HostDiff {
        gate,
        refused,
        creations,
        effects,
    } = state;

    // 1. Window rows: per-property authority against FRESH proof.
    for row in &reply.windows {
        if gate.current_obs != entry_obs {
            stats.stopped_mid_apply = true;
            break;
        }
        if !scope_controlled(reply, &row.output.0, &row.workspace) {
            stats.held += 1;
            continue;
        }
        let Some(base) = fresh.baseline.get(&row.lifetime.0) else {
            stats.held += 1;
            continue;
        };
        // Safety facts with no authority lane hold narrowly: hidden,
        // iconic, unreadable, and game/excluded facts never take automatic
        // writes. Overlays keep membership but take no tiled frame writes.
        if base.hidden || base.iconic || base.frame.is_none() {
            stats.held += 1;
            continue;
        }
        if base
            .facts
            .is_some_and(|facts| !capture_facts_rideable(&facts))
        {
            stats.held += 1;
            continue;
        }
        let frame_controlled = row.frame_authority == ScopeAuthority::Controlled;
        let member_controlled = row.membership_authority == ScopeAuthority::Controlled;
        let transfer = window_transfer_for(reply, &row.lifetime.0);
        let arrival_pending = transfer.is_some_and(|t| {
            ops.read_homing(&row.lifetime.0)
                .is_some_and(|(out, ws)| out != t.target_output.0 || ws != t.target_workspace)
        });
        // The writable frame target is `effective_frame` only: `allocation`
        // is the canonical slot, never a write target.
        if frame_controlled && row.effective_frame.is_some() && arrival_pending {
            // Deferred until membership proof: the target frame belongs to
            // the destination output, so writing it pre-move would place
            // foreign geometry on the source. Never hold-count a correctly
            // deferred frame.
        } else if frame_controlled && let Some(target) = row.effective_frame {
            if overlay_guarded(base) {
                // Reserved lanes (overlay, sticky, float, fit-excluded)
                // never take automatic tiled writes.
                stats.held += 1;
            } else {
                let live = ops.read_frame(&row.lifetime.0);
                match live {
                    None => stats.held += 1,
                    Some(live_rect) if live_rect == target => {
                        refused.note_match(&row.lifetime.0);
                    }
                    Some(live_rect) => {
                        let now = std::time::Instant::now();
                        if refused.should_skip(&row.lifetime.0, &target, &live_rect, now) {
                            stats.held += 1;
                        } else if !ensure_fresh(reply, gate, entry_obs, ops) {
                            stats.stopped_mid_apply = true;
                            break;
                        } else {
                            match pre_setter_ok(
                                gate,
                                entry_obs,
                                ops,
                                base,
                                &fresh.stored_tags,
                                SetterKind::Frame,
                            ) {
                                Gate::Hold => {
                                    stats.held += 1;
                                }
                                Gate::Stop => {
                                    stats.stopped_mid_apply = true;
                                    break;
                                }
                                Gate::Pass => {
                                    let wrote = ops.write_frame(&row.lifetime.0, target).ok();
                                    match (wrote, ops.read_frame(&row.lifetime.0)) {
                                        (Some(true), Some(proof)) => {
                                            match readback_outcome(true, &target, &proof) {
                                                ReadbackOutcome::Match => {
                                                    stats.geometry_writes += 1;
                                                    refused.note_match(&row.lifetime.0);
                                                }
                                                ReadbackOutcome::Clamp => {
                                                    stats.geometry_writes += 1;
                                                    refused.note_clamp(
                                                        &row.lifetime.0,
                                                        &target,
                                                        &proof,
                                                    );
                                                    stats.held += 1;
                                                }
                                                ReadbackOutcome::Pending => {
                                                    // Unreachable: a write
                                                    // sits behind this
                                                    // mismatch, so it is a
                                                    // clamp.
                                                    stats.geometry_writes += 1;
                                                    refused.note_clamp(
                                                        &row.lifetime.0,
                                                        &target,
                                                        &proof,
                                                    );
                                                    stats.held += 1;
                                                }
                                            }
                                        }
                                        _ => {
                                            // Explicit refusal or throw: the
                                            // bounded transient lane, exactly
                                            // like the native setter-failed
                                            // path. No feedback: ordinary
                                            // geometry results are proven by
                                            // read-back, not by invented
                                            // operation ids.
                                            refused.note_transient(
                                                &row.lifetime.0,
                                                &target,
                                                &live_rect,
                                                now,
                                            );
                                            stats.held += 1;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        // Membership rides only under a retained core window transfer with
        // its stable operation identity. A Controlled frame authority with
        // no effective frame contradicts the publish contract (the
        // effective frame is the only write target): hold fail-closed.
        if frame_controlled && row.effective_frame.is_none() {
            stats.held += 1;
        }
        if !member_controlled {
            continue;
        }
        let Some(t) = transfer else {
            continue;
        };
        let op = t.operation_id.clone();
        let target_out = t.target_output.0.clone();
        let target_ws = t.target_workspace.clone();
        if unbound_logicals.contains(target_ws.as_str()) {
            stats.held += 1;
            continue;
        }
        // Never actuate membership into a held or untouched scope.
        if !scope_controlled(reply, &target_out, &target_ws) {
            stats.held += 1;
            continue;
        }
        if homing_proven(ops, base, &fresh.stored_tags, &target_out, &target_ws) {
            effects.settle_applied(&op);
            effects.applied_window.insert(
                row.lifetime.0.clone(),
                (op.clone(), target_out.clone(), target_ws.clone()),
            );
            out_feedback.push(feedback(
                &op,
                AttemptKind::Transfer,
                AttemptOutcome::Applied,
                None,
            ));
            if t.follow {
                match actuate_follow_view(
                    reply, gate, entry_obs, ops, &mut stats, t, effects, fresh,
                ) {
                    FollowOutcome::Shown | FollowOutcome::Switched => {}
                    FollowOutcome::HeldEmit => {
                        // Membership is proven but the follow view is not:
                        // the transfer stays ambiguous without claiming a
                        // view outcome on its id.
                        out_feedback.pop();
                        out_feedback.push(feedback(
                            &op,
                            AttemptKind::Transfer,
                            AttemptOutcome::PossiblyExecuted,
                            None,
                        ));
                    }
                    FollowOutcome::HeldSilent => {
                        out_feedback.pop();
                        stats.held += 1;
                    }
                }
            }
            continue;
        }
        if t.held || effects.is_ambiguous(&op) {
            // Possibly-executed stays held without repeating: source homing
            // after a throw cannot prove non-execution (the attempt may have
            // landed and been moved back), so only completion, proven
            // refusal, or third-location proof settles.
            stats.held += 1;
            continue;
        }
        if let Some((applied_op, applied_out, applied_ws)) =
            effects.applied_window.get(&row.lifetime.0)
            && applied_op == &op
            && (base.output != *applied_out || base.workspace != *applied_ws)
        {
            // Same operation already applied elsewhere, but the current
            // observation shows the window left the proven target: newer
            // external input supersedes instead of fighting the user.
            out_feedback.push(feedback(
                &op,
                AttemptKind::Transfer,
                AttemptOutcome::SupersededExternal,
                None,
            ));
            effects.settle_applied(&op);
            continue;
        }
        let Some(home) = ops.read_homing(&row.lifetime.0) else {
            stats.held += 1;
            continue;
        };
        let base_home = (base.output.clone(), base.workspace.clone());
        let at_source = home == base_home;
        let partial = (home.0 == target_out && home.1 == base_home.1)
            || (home.0 == base_home.0 && home.1 == target_ws);
        if !at_source && !partial {
            out_feedback.push(feedback(
                &op,
                AttemptKind::Transfer,
                AttemptOutcome::SupersededExternal,
                None,
            ));
            effects.settle_applied(&op);
            continue;
        }
        if !ensure_fresh(reply, gate, entry_obs, ops) {
            stats.stopped_mid_apply = true;
            break;
        }
        match pre_setter_ok(
            gate,
            entry_obs,
            ops,
            base,
            &fresh.stored_tags,
            SetterKind::Membership,
        ) {
            Gate::Hold => {
                stats.held += 1;
                continue;
            }
            Gate::Stop => {
                stats.stopped_mid_apply = true;
                break;
            }
            Gate::Pass => {}
        }
        effects.arm(&op, format!("{target_out}/{target_ws}"));
        let before = ops.read_homing(&row.lifetime.0);
        let moved = ops.move_membership(&row.lifetime.0, &target_out, &target_ws);
        if gate.current_obs != entry_obs {
            stats.held += 1;
            stats.stopped_mid_apply = true;
            break;
        }
        // Re-read before settling: the setter boolean is never proof. A
        // throw that still landed (ambiguous call, proven target) settles
        // applied without counting a confirmed write.
        let proven = homing_proven(ops, base, &fresh.stored_tags, &target_out, &target_ws);
        match (moved, proven) {
            (Ok(_), true) => {
                stats.membership_writes += 1;
                effects.settle_applied(&op);
                effects.applied_window.insert(
                    row.lifetime.0.clone(),
                    (op.clone(), target_out.clone(), target_ws.clone()),
                );
                out_feedback.push(feedback(
                    &op,
                    AttemptKind::Transfer,
                    AttemptOutcome::Applied,
                    None,
                ));
                if t.follow {
                    match actuate_follow_view(
                        reply, gate, entry_obs, ops, &mut stats, t, effects, fresh,
                    ) {
                        FollowOutcome::Shown | FollowOutcome::Switched => {}
                        FollowOutcome::HeldEmit => {
                            out_feedback.pop();
                            out_feedback.push(feedback(
                                &op,
                                AttemptKind::Transfer,
                                AttemptOutcome::PossiblyExecuted,
                                None,
                            ));
                        }
                        FollowOutcome::HeldSilent => {
                            out_feedback.pop();
                            stats.held += 1;
                        }
                    }
                }
            }
            (Err(_), true) => {
                effects.settle_applied(&op);
                effects.applied_window.insert(
                    row.lifetime.0.clone(),
                    (op.clone(), target_out.clone(), target_ws.clone()),
                );
                out_feedback.push(feedback(
                    &op,
                    AttemptKind::Transfer,
                    AttemptOutcome::Applied,
                    None,
                ));
            }
            (Ok(false), false)
                if before.is_some_and(|(b_out, b_ws)| {
                    ops.read_homing(&row.lifetime.0)
                        .is_some_and(|(o, w)| o == b_out && w == b_ws)
                }) =>
            {
                out_feedback.push(feedback(
                    &op,
                    AttemptKind::Transfer,
                    AttemptOutcome::RefusedProven,
                    Some(HoldReason::RefusedTarget),
                ));
                stats.held += 1;
            }
            _ => {
                // Throw or partial write: stays ambiguous, never repeated
                // without fresh unambiguous proof.
                out_feedback.push(feedback(
                    &op,
                    AttemptKind::Transfer,
                    AttemptOutcome::PossiblyExecuted,
                    None,
                ));
                stats.held += 1;
            }
        }
        if gate.current_obs != entry_obs {
            stats.stopped_mid_apply = true;
            break;
        }
    }
    if stats.stopped_mid_apply {
        finalize(&mut stats);
        return (stats, out_feedback);
    }

    // 1b. Explicit float placements: the stable `frame_operation` token is
    // necessary but never sufficient. The row carries no frame authority
    // (core publishes `Untouched`), so the token plus the full fresh gates
    // below are the only write authority. Ordinary floats never ride here.
    for row in &reply.windows {
        let op = row.frame_operation.as_deref().unwrap_or("");
        if op.is_empty() {
            continue;
        }
        if gate.current_obs != entry_obs {
            stats.stopped_mid_apply = true;
            break;
        }
        if !scope_controlled(reply, &row.output.0, &row.workspace) {
            stats.held += 1;
            continue;
        }
        let Some(target) = row.effective_frame else {
            // No placement target: hold without consuming the placement.
            stats.held += 1;
            continue;
        };
        let Some(base) = fresh.baseline.get(&row.lifetime.0) else {
            // No provable lifetime: hold without consuming the placement.
            stats.held += 1;
            continue;
        };
        if base.hidden || base.iconic || base.frame.is_none() {
            stats.held += 1;
            continue;
        }
        if base
            .facts
            .is_some_and(|facts| !capture_facts_rideable(&facts))
        {
            stats.held += 1;
            continue;
        }
        if ops.gesture_barrier() {
            stats.held += 1;
            continue;
        }
        if effects.is_ambiguous(op) {
            // Possibly-executed stays held without repeating: even a frame
            // still at the baseline cannot prove the attempt never landed.
            stats.held += 1;
            continue;
        }
        let live = ops.read_frame(&row.lifetime.0);
        match live {
            Some(rect) if rect == target => {
                effects.mark_float_spent(op);
                effects.settle_applied(op);
                effects
                    .applied_float
                    .insert(row.lifetime.0.clone(), (op.to_owned(), target));
                refused.note_match(&row.lifetime.0);
                out_feedback.push(feedback(
                    op,
                    AttemptKind::Property,
                    AttemptOutcome::Applied,
                    None,
                ));
                continue;
            }
            None => {
                stats.held += 1;
                continue;
            }
            _ => {}
        }
        // Manual-change race: the live frame left the observation baseline.
        // Never fight the user; hold for the next stable observation.
        let live_rect = live.expect("matched above");
        if base.frame.is_some_and(|baseline| baseline != live_rect) {
            stats.held += 1;
            continue;
        }
        if let Some((applied_op, _)) = effects.applied_float.get(&row.lifetime.0)
            && applied_op == op
            && base.frame.is_some_and(|baseline| baseline != target)
        {
            effects.mark_float_spent(op);
            out_feedback.push(feedback(
                op,
                AttemptKind::Property,
                AttemptOutcome::SupersededExternal,
                None,
            ));
            continue;
        }
        if !ensure_fresh(reply, gate, entry_obs, ops) {
            stats.stopped_mid_apply = true;
            break;
        }
        match pre_setter_ok(
            gate,
            entry_obs,
            ops,
            base,
            &fresh.stored_tags,
            SetterKind::Float,
        ) {
            Gate::Hold => {
                stats.held += 1;
                continue;
            }
            Gate::Stop => {
                stats.stopped_mid_apply = true;
                break;
            }
            Gate::Pass => {}
        }
        // Arm the float phase immediately pre-dispatch, after every
        // safety/authority gate passed, so holds never consume placement.
        if !effects.mark_float_spent(op) {
            stats.held += 1;
            continue;
        }
        effects.arm(op, format!("{}/{}", row.output.0, row.workspace));
        let now = std::time::Instant::now();
        let wrote = ops.write_frame(&row.lifetime.0, target).ok();
        let proof = ops.read_frame(&row.lifetime.0);
        match (wrote, proof) {
            (Some(true), Some(rect)) => match readback_outcome(true, &target, &rect) {
                ReadbackOutcome::Match => {
                    stats.geometry_writes += 1;
                    refused.note_match(&row.lifetime.0);
                    effects.settle_applied(op);
                    effects
                        .applied_float
                        .insert(row.lifetime.0.clone(), (op.to_owned(), target));
                    out_feedback.push(feedback(
                        op,
                        AttemptKind::Property,
                        AttemptOutcome::Applied,
                        None,
                    ));
                }
                ReadbackOutcome::Clamp | ReadbackOutcome::Pending => {
                    stats.geometry_writes += 1;
                    refused.note_clamp(&row.lifetime.0, &target, &rect);
                    out_feedback.push(feedback(
                        op,
                        AttemptKind::Property,
                        AttemptOutcome::PossiblyExecuted,
                        None,
                    ));
                    stats.held += 1;
                }
            },
            (Some(false), Some(rect)) if rect == live_rect => {
                refused.note_transient(&row.lifetime.0, &target, &live_rect, now);
                out_feedback.push(feedback(
                    op,
                    AttemptKind::Property,
                    AttemptOutcome::RefusedProven,
                    Some(HoldReason::ConstraintClamp),
                ));
                stats.held += 1;
            }
            _ => {
                refused.note_transient(&row.lifetime.0, &target, &live_rect, now);
                out_feedback.push(feedback(
                    op,
                    AttemptKind::Property,
                    AttemptOutcome::PossiblyExecuted,
                    None,
                ));
                stats.held += 1;
            }
        }
        if gate.current_obs != entry_obs {
            stats.stopped_mid_apply = true;
            break;
        }
    }
    if stats.stopped_mid_apply {
        finalize(&mut stats);
        return (stats, out_feedback);
    }

    // Adopt bindings the reply carries for in-flight logical ids. Binding
    // recovery comes strictly from the report (which the W1 capture built
    // from native bindings), never from enumeration guessing.
    for row in &reply.workspaces {
        if let (Some(native), Some(logical)) =
            (row.native_binding.as_deref(), row.logical_id.as_deref())
            && !native.is_empty()
            && !logical.is_empty()
            && creations.is_inflight(logical)
            && !creations.is_bound(logical)
        {
            creations.bind(logical, native.to_owned());
            effects.settle_applied(&format!("create-{logical}"));
        }
    }

    // 2. Workspace creation: one flight per logical id. Rows with a binding
    // are adopted above; held and desired-absent rows never create; the
    // logical token is never used as a native id.
    for row in &reply.workspaces {
        if row.native_binding.is_some() {
            continue;
        }
        if row.hold.is_some() {
            stats.held += 1;
            continue;
        }
        if !row.existence {
            stats.held += 1;
            continue;
        }
        // A pending logical has no domain scope yet: creation needs a
        // controlled domain on the same output, so a fully held output
        // never actuates lifecycle.
        let output_live = reply.scopes.iter().any(|status| {
            status.authority == ScopeAuthority::Controlled
                && status
                    .scope
                    .strip_prefix(row.output.0.as_str())
                    .is_some_and(|rest| rest.starts_with('/'))
        });
        if !output_live {
            stats.held += 1;
            continue;
        }
        let logical = row
            .logical_id
            .as_deref()
            .filter(|id| !id.is_empty())
            .unwrap_or(row.id.as_str());
        if logical.is_empty() {
            continue;
        }
        if logical == row.id.as_str() && ops.native_ids().contains(logical) {
            // The token collides with a live native id: it is never a
            // native id, even on collision.
            stats.held += 1;
            continue;
        }
        if creations.is_bound(logical) {
            continue;
        }
        if creations.is_inflight(logical) {
            // Ambiguous result: hold without another attempt. An unrelated
            // external workspace appearing meanwhile is never stolen; the
            // binding arrives through a later reply row.
            stats.held += 1;
            continue;
        }
        let op = format!("create-{logical}");
        if effects.is_ambiguous(&op) {
            stats.held += 1;
            continue;
        }
        if !ensure_fresh(reply, gate, entry_obs, ops) {
            stats.stopped_mid_apply = true;
            break;
        }
        creations.start(logical);
        effects.arm(&op, row.output.0.clone());
        match ops.create_workspace(logical) {
            Ok(Some(native)) if !native.is_empty() && native != logical => {
                creations.bind(logical, native);
                effects.settle_applied(&op);
                stats.creations += 1;
            }
            Ok(_) => {
                // Null/empty without a throw may still have executed:
                // hold ambiguously for the reply-carried binding, never
                // claim a proven refusal that would green-light a duplicate.
                stats.held += 1;
            }
            Err(()) => {
                stats.held += 1;
            }
        }
        if gate.current_obs != entry_obs {
            stats.stopped_mid_apply = true;
            break;
        }
    }
    if stats.stopped_mid_apply {
        finalize(&mut stats);
        return (stats, out_feedback);
    }

    // 2b. Workspace removal: desired absence is eligibility only. The reply
    // mapping must own the workspace (managed, non-displaced binding);
    // occupancy (including non-tiled occupants), visibility, references, and
    // the minimum count re-read fresh; unknown occupancy keeps.
    for row in &reply.workspaces {
        if row.existence || row.hold.is_some() {
            continue;
        }
        let Some(native) = row.native_binding.as_deref().filter(|id| !id.is_empty()) else {
            continue;
        };
        if !scope_controlled(reply, &row.output.0, native) {
            stats.held += 1;
            continue;
        }
        let owned = fresh
            .bindings
            .iter()
            .find(|binding| binding.native_id == native && binding.output == row.output);
        let Some(binding) = owned else {
            stats.held += 1;
            continue;
        };
        if !binding.managed || binding.displaced || binding.pinned {
            stats.held += 1;
            continue;
        }
        let referenced = reply.transfers.iter().any(|t| {
            t.target_workspace == native
                || matches!(&t.kind, TransferKind::Window(id) if {
                    fresh.baseline.get(&id.0).is_some_and(|w| w.workspace == native)
                })
        }) || reply.windows.iter().any(|w| w.workspace == native);
        if referenced {
            stats.held += 1;
            continue;
        }
        if !ensure_fresh(reply, gate, entry_obs, ops) {
            stats.stopped_mid_apply = true;
            break;
        }
        let (total, readable) = ops.occupants(native);
        if !readable {
            stats.held += 1;
            continue;
        }
        if total > 0 {
            stats.held += 1;
            continue;
        }
        match ops.is_visible(native) {
            Some(false) => {}
            _ => {
                stats.held += 1;
                continue;
            }
        }
        if ops.workspace_count() <= 2 {
            stats.held += 1;
            continue;
        }
        if !ensure_fresh(reply, gate, entry_obs, ops) {
            stats.stopped_mid_apply = true;
            break;
        }
        match ops.remove_workspace(native) {
            Ok(true) => stats.removals += 1,
            _ => stats.held += 1,
        }
        if gate.current_obs != entry_obs {
            stats.stopped_mid_apply = true;
            break;
        }
    }
    if stats.stopped_mid_apply {
        finalize(&mut stats);
        return (stats, out_feedback);
    }

    // Paired migration source-refill views (`{migration-op}-refill`) defer
    // to the migration path below: switching the source view before member
    // arrival proof would hide unmoved members.
    let mut deferred_refills: HashSet<&str> = HashSet::new();
    for t in &reply.transfers {
        if !matches!(t.kind, TransferKind::View) {
            continue;
        }
        if let Some(prefix) = t.operation_id.strip_suffix("-refill")
            && reply
                .transfers
                .iter()
                .any(|m| matches!(m.kind, TransferKind::Migration) && m.operation_id == prefix)
        {
            deferred_refills.insert(t.operation_id.as_str());
        }
    }

    // 3. Output views: retained transfer targets only, proven on the exact
    // target output. Unresolvable targets hold with no attempt recorded, so
    // the retained core target retries on a later report.
    for t in &reply.transfers {
        if !matches!(t.kind, TransferKind::View) {
            continue;
        }
        if deferred_refills.contains(t.operation_id.as_str()) {
            continue;
        }
        if gate.current_obs != entry_obs {
            stats.stopped_mid_apply = true;
            break;
        }
        let out = t.target_output.0.clone();
        let ws = t.target_workspace.clone();
        let op = t.operation_id.clone();
        if unbound_logicals.contains(ws.as_str()) {
            stats.held += 1;
            continue;
        }
        if !scope_controlled(reply, &out, &ws) {
            stats.held += 1;
            continue;
        }
        if ops.read_view(&out).as_deref() == Some(ws.as_str()) {
            effects.settle_applied(&op);
            effects
                .applied_view
                .insert(out.clone(), (op.clone(), ws.clone()));
            out_feedback.push(feedback(
                &op,
                AttemptKind::View,
                AttemptOutcome::Applied,
                None,
            ));
            continue;
        }
        if t.held || effects.is_view_spent(&op) {
            // A live view that is neither the observation baseline nor the
            // target proves newer external input: supersede instead of
            // fighting the user. Anything else holds without repeating: a
            // failed switch cannot prove non-execution, so retrying could
            // fight a user who switched away after an executed attempt.
            // Late target proof above already settles without a dispatch;
            // this fence is never rearmed by other proofs.
            let baseline_view = fresh.views.get(&out);
            if ops
                .read_view(&out)
                .is_some_and(|live| Some(&live) != baseline_view && live != ws)
            {
                out_feedback.push(feedback(
                    &op,
                    AttemptKind::View,
                    AttemptOutcome::SupersededExternal,
                    None,
                ));
                effects.settle_applied(&op);
                continue;
            }
            stats.held += 1;
            continue;
        }
        if let Some((applied_op, applied_ws)) = effects.applied_view.get(&out)
            && applied_op == &op
            && ops.read_view(&out).as_deref() != Some(applied_ws.as_str())
        {
            out_feedback.push(feedback(
                &op,
                AttemptKind::View,
                AttemptOutcome::SupersededExternal,
                None,
            ));
            effects.settle_applied(&op);
            continue;
        }
        if !ensure_fresh(reply, gate, entry_obs, ops) {
            stats.stopped_mid_apply = true;
            break;
        }
        let pre = ops.read_view(&out);
        let baseline = fresh.views.get(&out);
        // Arm the view phase immediately before dispatch; late proofs
        // cannot rearm it, so a possibly-executed switch never repeats.
        if !effects.mark_view_spent(&op) {
            stats.held += 1;
            continue;
        }
        let result = ops.switch_view(&out, &ws);
        let proof = ops.read_view(&out);
        match settle_view_attempt(&pre, &proof, baseline, &ws, &result) {
            AttemptOutcome::Applied => {
                if result.is_ok() && proof.as_deref() == Some(ws.as_str()) {
                    stats.view_writes += 1;
                }
                effects.settle_applied(&op);
                effects
                    .applied_view
                    .insert(out.clone(), (op.clone(), ws.clone()));
                out_feedback.push(feedback(
                    &op,
                    AttemptKind::View,
                    AttemptOutcome::Applied,
                    None,
                ));
            }
            AttemptOutcome::SupersededExternal => {
                out_feedback.push(feedback(
                    &op,
                    AttemptKind::View,
                    AttemptOutcome::SupersededExternal,
                    None,
                ));
                effects.settle_applied(&op);
                stats.held += 1;
            }
            AttemptOutcome::RefusedProven => {
                out_feedback.push(feedback(
                    &op,
                    AttemptKind::View,
                    AttemptOutcome::RefusedProven,
                    Some(HoldReason::RefusedTarget),
                ));
                stats.held += 1;
            }
            _ => {
                out_feedback.push(feedback(
                    &op,
                    AttemptKind::View,
                    AttemptOutcome::PossiblyExecuted,
                    None,
                ));
                stats.held += 1;
            }
        }
        if gate.current_obs != entry_obs {
            stats.stopped_mid_apply = true;
            break;
        }
    }
    if stats.stopped_mid_apply {
        finalize(&mut stats);
        return (stats, out_feedback);
    }

    // 3b. Whole-workspace migration: membership-only moves under one stable
    // operation, then views. No view switches until ALL fresh member arrival
    // proof is in: a partial failure holds truthfully under the same id, and
    // the paired source refill actuates only after the target view proves.
    for t in &reply.transfers {
        if !matches!(t.kind, TransferKind::Migration) {
            continue;
        }
        if gate.current_obs != entry_obs {
            stats.stopped_mid_apply = true;
            break;
        }
        let op = t.operation_id.clone();
        let out = t.target_output.0.clone();
        let ws = t.target_workspace.clone();
        if unbound_logicals.contains(ws.as_str()) {
            stats.held += 1;
            continue;
        }
        if !scope_controlled(reply, &out, &ws) {
            stats.held += 1;
            continue;
        }
        let members: Vec<String> = reply
            .windows
            .iter()
            .filter(|w| w.output.0 == out && w.workspace == ws)
            .map(|w| w.lifetime.0.clone())
            .collect();
        if members.is_empty() || members.iter().any(|id| !fresh.baseline.contains_key(id)) {
            stats.held += 1;
            continue;
        }
        if t.held || effects.is_ambiguous(&op) {
            // Late full-membership proof settles an ambiguous parent without
            // member replay: every member homing proven at the target with a
            // valid lifetime is unambiguous proof of completion. Anything
            // less holds without repeating.
            let mut late_proof = true;
            for id in &members {
                let base = &fresh.baseline[id];
                if !homing_proven(ops, base, &fresh.stored_tags, &out, &ws) {
                    late_proof = false;
                    break;
                }
            }
            if !late_proof {
                stats.held += 1;
                continue;
            }
        }
        let mut superseded = false;
        for id in &members {
            let base = &fresh.baseline[id];
            let Some(home) = ops.read_homing(id) else {
                stats.held += 1;
                superseded = true;
                break;
            };
            if home.0 == out && home.1 == ws {
                continue;
            }
            let at_source = home.0 == base.output && home.1 == base.workspace;
            let partial_progress = (home.0 == out && home.1 == base.workspace)
                || (home.0 == base.output && home.1 == ws);
            if !at_source && !partial_progress {
                out_feedback.push(feedback(
                    &op,
                    AttemptKind::Transfer,
                    AttemptOutcome::SupersededExternal,
                    None,
                ));
                effects.settle_applied(&op);
                superseded = true;
                break;
            }
        }
        if superseded {
            continue;
        }
        let mut armed = false;
        let mut failed = false;
        for id in &members {
            let base = &fresh.baseline[id];
            if homing_proven(ops, base, &fresh.stored_tags, &out, &ws) {
                effects
                    .applied_window
                    .insert(id.clone(), (op.clone(), out.clone(), ws.clone()));
                continue;
            }
            if !ensure_fresh(reply, gate, entry_obs, ops) {
                stats.stopped_mid_apply = true;
                break;
            }
            match pre_setter_ok(
                gate,
                entry_obs,
                ops,
                base,
                &fresh.stored_tags,
                SetterKind::Membership,
            ) {
                Gate::Hold => {
                    // Row-local safety drift: skip this member but keep
                    // converging the rest; the all-homed check below holds
                    // the operation truthfully until every arrival proves.
                    stats.held += 1;
                    continue;
                }
                Gate::Stop => {
                    stats.stopped_mid_apply = true;
                    break;
                }
                Gate::Pass => {}
            }
            if !armed {
                armed = true;
                effects.arm(&op, format!("{out}/{ws}"));
            }
            let moved = ops.move_membership(id, &out, &ws);
            if gate.current_obs != entry_obs {
                stats.stopped_mid_apply = true;
                break;
            }
            if moved.is_ok() && homing_proven(ops, base, &fresh.stored_tags, &out, &ws) {
                stats.membership_writes += 1;
                effects
                    .applied_window
                    .insert(id.clone(), (op.clone(), out.clone(), ws.clone()));
            } else {
                failed = true;
                stats.held += 1;
                break;
            }
        }
        if stats.stopped_mid_apply {
            break;
        }
        if failed {
            out_feedback.push(feedback(
                &op,
                AttemptKind::Transfer,
                AttemptOutcome::PossiblyExecuted,
                None,
            ));
            continue;
        }
        let all_homed = members
            .iter()
            .all(|id| homing_proven(ops, &fresh.baseline[id], &fresh.stored_tags, &out, &ws));
        if !all_homed {
            out_feedback.push(feedback(
                &op,
                AttemptKind::Transfer,
                AttemptOutcome::PossiblyExecuted,
                None,
            ));
            stats.held += 1;
            continue;
        }
        // Target view only after every member arrival proof. The phase is
        // owned by the migration parent: late member proof never rearms an
        // attempted target view, and exact target proof settles without a
        // dispatch. Successful members/views are never replayed.
        if ops.read_view(&out).as_deref() != Some(ws.as_str()) {
            if effects.is_view_spent(&op) {
                // Late member proof never rearms an attempted target view:
                // silent hold without another switch or repeat feedback.
                stats.held += 1;
                continue;
            }
            if !ensure_fresh(reply, gate, entry_obs, ops) {
                stats.stopped_mid_apply = true;
                break;
            }
            if !effects.mark_view_spent(&op) {
                // Raced arming: another phase spent it first; silent hold.
                stats.held += 1;
                continue;
            }
            match ops.switch_view(&out, &ws) {
                Ok(_) if ops.read_view(&out).as_deref() == Some(ws.as_str()) => {
                    stats.view_writes += 1;
                }
                _ => {
                    out_feedback.push(feedback(
                        &op,
                        AttemptKind::Transfer,
                        AttemptOutcome::PossiblyExecuted,
                        None,
                    ));
                    stats.held += 1;
                    continue;
                }
            }
        }
        // Paired source refill only after the target view proves: never
        // ahead of arrival. The refill owns its own view phase under the
        // actual refill id, with its own held/scope/binding/spent gates.
        // Parent partial stays truthful without replaying successful phases.
        let refill_op = format!("{op}-refill");
        if let Some(refill) = reply
            .transfers
            .iter()
            .find(|v| matches!(v.kind, TransferKind::View) && v.operation_id == refill_op)
        {
            let refill_out = refill.target_output.0.clone();
            let refill_ws = refill.target_workspace.clone();
            if ops.read_view(&refill_out).as_deref() == Some(refill_ws.as_str()) {
                effects.settle_applied(&refill.operation_id);
                out_feedback.push(feedback(
                    &refill.operation_id,
                    AttemptKind::View,
                    AttemptOutcome::Applied,
                    None,
                ));
            } else if refill.held
                || !scope_controlled(reply, &refill_out, &refill_ws)
                || unbound_logicals.contains(refill_ws.as_str())
                || effects.is_view_spent(&refill.operation_id)
            {
                // Held, scope-held, unbound, or already attempted refill:
                // never switch a view that must remain untouched. A live
                // third-location view proves external input; anything else
                // holds truthfully with the parent unresolved.
                let baseline_refill = fresh.views.get(&refill_out);
                if view_third_location(&ops.read_view(&refill_out), baseline_refill, &refill_ws) {
                    out_feedback.push(feedback(
                        &refill.operation_id,
                        AttemptKind::View,
                        AttemptOutcome::SupersededExternal,
                        None,
                    ));
                    effects.settle_applied(&refill.operation_id);
                }
                out_feedback.push(feedback(
                    &op,
                    AttemptKind::Transfer,
                    AttemptOutcome::PossiblyExecuted,
                    None,
                ));
                stats.held += 1;
                continue;
            } else {
                if !ensure_fresh(reply, gate, entry_obs, ops) {
                    stats.stopped_mid_apply = true;
                    break;
                }
                let pre = ops.read_view(&refill_out);
                let baseline_refill = fresh.views.get(&refill_out);
                if !effects.mark_view_spent(&refill.operation_id) {
                    // Raced arming: silent hold without another switch.
                    stats.held += 1;
                    continue;
                }
                let result = ops.switch_view(&refill_out, &refill_ws);
                let proof = ops.read_view(&refill_out);
                match settle_view_attempt(&pre, &proof, baseline_refill, &refill_ws, &result) {
                    AttemptOutcome::Applied => {
                        if result.is_ok() && proof.as_deref() == Some(refill_ws.as_str()) {
                            stats.view_writes += 1;
                        }
                        effects.settle_applied(&refill.operation_id);
                        out_feedback.push(feedback(
                            &refill.operation_id,
                            AttemptKind::View,
                            AttemptOutcome::Applied,
                            None,
                        ));
                    }
                    AttemptOutcome::SupersededExternal => {
                        out_feedback.push(feedback(
                            &refill.operation_id,
                            AttemptKind::View,
                            AttemptOutcome::SupersededExternal,
                            None,
                        ));
                        effects.settle_applied(&refill.operation_id);
                        out_feedback.push(feedback(
                            &op,
                            AttemptKind::Transfer,
                            AttemptOutcome::PossiblyExecuted,
                            None,
                        ));
                        stats.held += 1;
                        continue;
                    }
                    AttemptOutcome::RefusedProven => {
                        out_feedback.push(feedback(
                            &refill.operation_id,
                            AttemptKind::View,
                            AttemptOutcome::RefusedProven,
                            Some(HoldReason::RefusedTarget),
                        ));
                        out_feedback.push(feedback(
                            &op,
                            AttemptKind::Transfer,
                            AttemptOutcome::PossiblyExecuted,
                            None,
                        ));
                        stats.held += 1;
                        continue;
                    }
                    _ => {
                        out_feedback.push(feedback(
                            &refill.operation_id,
                            AttemptKind::View,
                            AttemptOutcome::PossiblyExecuted,
                            None,
                        ));
                        out_feedback.push(feedback(
                            &op,
                            AttemptKind::Transfer,
                            AttemptOutcome::PossiblyExecuted,
                            None,
                        ));
                        stats.held += 1;
                        continue;
                    }
                }
            }
        }
        effects.settle_applied(&op);
        out_feedback.push(feedback(
            &op,
            AttemptKind::Transfer,
            AttemptOutcome::Applied,
            None,
        ));
        if gate.current_obs != entry_obs {
            stats.stopped_mid_apply = true;
            break;
        }
    }
    if stats.stopped_mid_apply {
        finalize(&mut stats);
        return (stats, out_feedback);
    }

    // 4. One-shot effects: spent BEFORE dispatch, proven by read-back, gated
    // on the exact live view (switch-before-focus). Stale effects drop
    // (at-most-once), never steal focus later. Effects produce no feedback:
    // the core consumes no effect outcomes; the spent proof plus the
    // foreground read-back is the whole contract.
    for effect in &reply.effects {
        if gate.current_obs != entry_obs {
            stats.stopped_mid_apply = true;
            break;
        }
        if !gate.fresh_reply(reply) {
            stats.stopped_mid_apply = true;
            break;
        }
        let Some(base) = fresh.baseline.get(&effect.target.0) else {
            stats.held += 1;
            continue;
        };
        if base.hidden || base.iconic || base.frame.is_none() {
            stats.held += 1;
            continue;
        }
        if base
            .facts
            .is_some_and(|facts| !capture_facts_rideable(&facts))
        {
            stats.held += 1;
            continue;
        }
        if !ensure_fresh(reply, gate, entry_obs, ops) {
            stats.stopped_mid_apply = true;
            break;
        }
        match pre_setter_ok(
            gate,
            entry_obs,
            ops,
            base,
            &fresh.stored_tags,
            SetterKind::Membership,
        ) {
            Gate::Hold => {
                stats.held += 1;
                continue;
            }
            Gate::Stop => {
                stats.stopped_mid_apply = true;
                break;
            }
            Gate::Pass => {}
        }
        // Switch-before-focus liveness: the CURRENT live homing (not the
        // observation preimage) must name a controlled live scope whose
        // exact-output view is shown. A moved lifetime focuses only where
        // it actually lives.
        let Some((live_out, live_ws)) = ops.read_homing(&effect.target.0) else {
            stats.held += 1;
            continue;
        };
        if !scope_controlled(reply, &live_out, &live_ws) {
            stats.held += 1;
            continue;
        }
        if ops.read_view(&live_out).as_deref() != Some(live_ws.as_str()) {
            // The live workspace is not shown on its output: focusing now
            // would steal focus from the wrong view. Hold for the view.
            stats.held += 1;
            continue;
        }
        if !effects.mark_effect_spent(&effect.id) {
            continue;
        }
        effects.arm(&effect.id, effect.target.0.clone());
        // Foreground read-back is authoritative even if the setter throws:
        // a throwing call that still landed proves focus.
        let _ = ops.focus_window(&effect.target.0);
        if ops.is_foreground(&effect.target.0) {
            stats.effects += 1;
            effects.settle_applied(&effect.id);
        } else {
            stats.held += 1;
        }
        if gate.current_obs != entry_obs {
            stats.stopped_mid_apply = true;
            break;
        }
    }

    finalize(&mut stats);
    (stats, out_feedback)
}

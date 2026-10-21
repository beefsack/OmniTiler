#![forbid(unsafe_code)]
//! Native host-diff/feedback facade for the single world owner.
//!
//! Actuator over typed [`WorldReply`](tiler_core::world_report::WorldReply)
//! facts against fresh native observations. All setters and proof re-reads
//! go through [`NativeOps`], implemented by the production `LiveNative` and
//! an in-memory fake in offline tests. Geometry converges from the core reply,
//! never from a second policy: this module honors per-row authority, scope
//! authority, holds, transfers, views, migrations, and one-shot effects
//! exactly as the reply states them. Workspace lifecycle needs no
//! actuation: the core owns logical workspaces outright.
//!
//! Source-first inputs (M1): [`FreshHost::from_sources`] converts the real
//! per-tick sources over the accepted W1 capture boundary. Window identity
//! comes from [`WindowKey`](crate::native_guards::WindowKey) (never HWND alone),
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
//! - The writable frame target is `effective_frame` only: `allocation` is the
//!   canonical slot, never a write target.
//! - Partial results are reported honestly; nothing here claims atomicity.

use super::world_capture::{CapturedWorld, OwnerInputs, capture_facts_rideable};
use super::{ObservedWindow, RetainedRow};
use crate::native_guards::WindowKey;
use crate::tiling::{ReadbackOutcome, RefusedTracker, WindowFacts, readback_outcome};
use std::collections::{HashMap, HashSet};
use tiler_core::geometry::Rect;
use tiler_core::world_report::{
    AttemptFeedback, AttemptKind, AttemptOutcome, HoldReason, ScopeAuthority, TransferKind,
    TransferTarget, WorldReply,
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
/// visible-membership tags the lifetime gate rechecks against and the live
/// output views. Logical workspace mapping is core-owned; the adapter
/// carries no binding table. Occupancy, visibility, and enumeration stay
/// live reads.
#[derive(Debug, Clone, Default)]
pub(super) struct FreshHost {
    pub(super) baseline: HashMap<String, FreshWindow>,
    pub(super) stored_tags: HashMap<String, String>,
    /// Live output views at observation time (`output -> logical workspace`
    /// echo), seeding proof re-reads. Empty when the adapter has no echoed
    /// view yet.
    pub(super) views: HashMap<String, String>,
}

impl FreshHost {
    /// Source-first conversion over the accepted W1 capture boundary plus
    /// the real per-tick sources it was built from. See the module docs for
    /// the per-field sources. Takes the field-input seam (`OwnerInputs`),
    /// never the whole loop, so the runtime can hold the owner mutably
    /// alongside this read.
    pub(super) fn from_sources(
        state: &OwnerInputs<'_>,
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
        for (key, token) in state.member_tokens {
            if let Some(tag) = state.member_tags.get(key) {
                stored_tags.insert(token.clone(), tag.clone());
            }
        }
        let mut baseline: HashMap<String, FreshWindow> = HashMap::new();
        // Logical homing is reply-side: the capture carries screen
        // affinity plus truthful observed receipts. Baselines home to
        // the last proven receipt where capture freshly proved it
        // (never desire); unproven lifetimes hold with no workspace
        // claim so third-location checks never mistake an empty
        // preimage for an external move. Membership and frame writes
        // still gate on core Controlled scope rows plus fresh proof.
        for fact in &captured.windows {
            let token = fact.window.0.as_str();
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
            let hidden = state.hidden_claims.contains_key(key);
            let iconic = fact.minimized || state.minimized_omitted.contains(key);
            // Observation-time homing: the retained proven receipt where
            // capture freshly revalidated it this tick, else the live
            // screen affinity with no workspace claim.
            let (output, workspace) = match &fact.observed_workspace {
                Some(proven_ws) => state
                    .applied_homing
                    .get(token)
                    .filter(|(_, ws)| ws == proven_ws)
                    .cloned()
                    .unwrap_or_else(|| (fact.output.0.clone(), proven_ws.clone())),
                None => (fact.output.0.clone(), String::new()),
            };
            baseline.insert(
                token.to_owned(),
                FreshWindow {
                    key: key.clone(),
                    token: token.to_owned(),
                    frame: fact.rect,
                    facts: facts_of.get(token).copied(),
                    output,
                    workspace,
                    iconic,
                    hidden,
                    sticky: fact.sticky,
                    floating: fact.floating,
                    fit_excluded: fact.fit_excluded,
                    live_tag: state.member_tags.get(key).cloned(),
                },
            );
        }
        // Observation-time views are the adapter's proven applied
        // receipts from capture, never a fallback of desired views:
        // empty means unknown and retains pending flights.
        let views: HashMap<String, String> = captured
            .observed_views
            .iter()
            .map(|view| (view.output.0.clone(), view.view.clone()))
            .collect();
        Self {
            baseline,
            stored_tags,
            views,
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
///
/// Reads take `&mut self`: production reads advance a side observation map
/// and consume one-shot invalidation state, so even proofs are mutating.
/// Callers already hold the actuator mutably.
pub(super) trait NativeOps {
    fn read_window(&mut self, token: &str) -> Option<FreshWindow>;
    fn read_frame(&mut self, token: &str) -> Option<Rect>;
    fn read_homing(&mut self, token: &str) -> Option<(String, String)>;
    fn read_view(&mut self, output: &str) -> Option<String>;
    fn is_foreground(&mut self, token: &str) -> bool;
    fn gesture_barrier(&mut self) -> bool;
    fn write_frame(&mut self, token: &str, rect: Rect) -> Result<bool, ()>;
    /// Membership actuation toward `(out, ws)` with the desired visible
    /// frame as an actuation TARGET only (`None` for membership-only
    /// rows such as redirect-lane movers, floats/sticky/overlays, which
    /// carry no published frame). `remap` allows a tiled cross-output
    /// arrival to translate the live frame onto the target output
    /// mechanically (position preserved, never resized or refit: the
    /// sweep re-tiles after arrival); floats pass `false` and preserve
    /// their physical frame. The setter boolean is never proof: the
    /// caller re-reads exact homing/frame/visibility before settling
    /// Applied.
    fn move_membership(
        &mut self,
        token: &str,
        out: &str,
        ws: &str,
        frame: Option<Rect>,
        remap: bool,
    ) -> Result<bool, ()>;
    fn switch_view(&mut self, output: &str, logical: &str) -> Result<bool, ()>;
    fn focus_window(&mut self, token: &str) -> Result<bool, ()>;
    /// One external invalidation consumed per call: racing native input that
    /// arrived mid-apply. The facade polls this before every setter.
    fn poll_external_invalidation(&mut self) -> bool {
        false
    }
}

/// In-memory offline native. Seeded from a [`FreshHost`] baseline (windows
/// plus views); writes mutate the live maps so proof re-reads observe them.
#[derive(Debug, Default)]
#[cfg(test)]
pub(super) struct FakeNative {
    pub(super) windows: HashMap<String, FreshWindow>,
    pub(super) views: HashMap<String, String>,
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
    pub(super) pending_invalidation: bool,
    pub(super) frame_writes: usize,
    pub(super) membership_writes: usize,
}

#[cfg(test)]
impl FakeNative {
    pub(super) fn seeded(host: &FreshHost) -> Self {
        let mut native = Self::default();
        for (token, window) in &host.baseline {
            native.windows.insert(token.clone(), window.clone());
        }
        native.views.clone_from(&host.views);
        native
    }

    fn note_write(&mut self) {}
}

#[cfg(test)]
impl NativeOps for FakeNative {
    fn read_window(&mut self, token: &str) -> Option<FreshWindow> {
        self.windows.get(token).cloned()
    }
    fn read_frame(&mut self, token: &str) -> Option<Rect> {
        self.windows.get(token).and_then(|w| w.frame)
    }
    fn read_homing(&mut self, token: &str) -> Option<(String, String)> {
        self.windows
            .get(token)
            .map(|w| (w.output.clone(), w.workspace.clone()))
    }
    fn read_view(&mut self, output: &str) -> Option<String> {
        self.views.get(output).cloned()
    }
    fn is_foreground(&mut self, token: &str) -> bool {
        self.foreground.as_deref() == Some(token)
    }
    fn gesture_barrier(&mut self) -> bool {
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
    fn move_membership(
        &mut self,
        token: &str,
        out: &str,
        ws: &str,
        frame: Option<Rect>,
        remap: bool,
    ) -> Result<bool, ()> {
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
            // Membership-only lane: the offline stand-in never moves
            // frames here (production `LiveNative` remaps tiled targets
            // and writes exact ones under the same gates; geometry still
            // converges through the deferred frame phase). Both actuation
            // targets ride only as inputs for the production seam.
            let _ = (frame, remap);
        }
        self.membership_writes += 1;
        self.note_write();
        Ok(true)
    }
    fn switch_view(&mut self, output: &str, logical: &str) -> Result<bool, ()> {
        let key = format!("{output}/{logical}");
        if self.throw_view.contains(&key) {
            return Err(());
        }
        if self.fail_view.contains(&key) {
            self.note_write();
            return Ok(false);
        }
        self.views.insert(output.to_owned(), logical.to_owned());
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
    fn poll_external_invalidation(&mut self) -> bool {
        std::mem::take(&mut self.pending_invalidation)
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
    // Reset the generation-scoped tables together.
    #[allow(dead_code)]
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

/// One offline diff unit: freshness gate plus the refusal and at-most-once
/// tables behind [`apply_world_reply`]. All maps stay bounded:
/// [`Self::prune`] retains entries against the current reply and baseline
/// every pass, and [`Self::reset_generation`] clears generation-scoped state.
/// (Native workspace lifecycle needs no table: the core owns logical
/// workspaces outright, so the adapter never creates or removes them.)
#[derive(Debug)]
pub(super) struct HostDiff {
    pub(super) gate: DiffGate,
    pub(super) refused: RefusedTracker,
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
            effects: EffectTracker::default(),
        }
    }

    /// Generation reset: new core namespace clears operation-scoped tables
    /// (reused operation ids must never inherit spent/ambiguous/applied
    /// state) and the refusal memory (no stale suppression under a fresh
    /// world). The caller re-derives the watermark from the new bootstrap.
    #[allow(dead_code)]
    pub(super) fn reset_generation(&mut self, generation: &str) {
        self.gate.reset_generation(generation);
        self.refused = RefusedTracker::default();
        self.effects.clear_generation();
    }

    /// Bound every table against the current reply and baseline: refusal
    /// entries survive only for live reply/baseline lifetimes, spent and
    /// ambiguous entries only for operation ids the reply still carries,
    /// and applied proofs only for baselined lifetimes (view proofs
    /// additionally only for live outputs and live operation ids, since
    /// hotplug can retire an output while its proof entry would otherwise
    /// linger).
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
    ops: &mut dyn NativeOps,
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
    ops: &mut dyn NativeOps,
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

/// Record one proven window-transfer arrival plus its follow view.
/// Pushes Applied feedback; when follow actuation cannot prove the
/// view, the transfer degrades to PossiblyExecuted (HeldEmit) or holds
/// silently (HeldSilent) without claiming a view outcome on its id.
/// `count_write` marks a confirmed native write (post-setter proof);
/// pre-proven arrivals (no setter ran) record no write.
#[allow(clippy::too_many_arguments)]
fn settle_transfer_applied(
    reply: &WorldReply,
    gate: &mut DiffGate,
    entry_obs: u64,
    ops: &mut dyn NativeOps,
    stats: &mut ApplyStats,
    out_feedback: &mut Vec<AttemptFeedback>,
    transfer: &TransferTarget,
    effects: &mut EffectTracker,
    fresh: &FreshHost,
    token: &str,
    count_write: bool,
) {
    if count_write {
        stats.membership_writes += 1;
    }
    effects.settle_applied(&transfer.operation_id);
    effects.applied_window.insert(
        token.to_owned(),
        (
            transfer.operation_id.clone(),
            transfer.target_output.0.clone(),
            transfer.target_workspace.clone(),
        ),
    );
    out_feedback.push(feedback(
        &transfer.operation_id,
        AttemptKind::Transfer,
        AttemptOutcome::Applied,
        None,
    ));
    if transfer.follow {
        match actuate_follow_view(reply, gate, entry_obs, ops, stats, transfer, effects, fresh) {
            FollowOutcome::Shown | FollowOutcome::Switched => {}
            FollowOutcome::HeldEmit => {
                // Membership is proven but the follow view is not:
                // the transfer stays ambiguous without claiming a
                // view outcome on its id.
                out_feedback.pop();
                out_feedback.push(feedback(
                    &transfer.operation_id,
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

/// Logical no-effect receipt for a floating membership-only move: the
/// live window still names the key with a matching stored tag and the
/// physical frame is preserved exactly. Never reinterprets the
/// physical output (no affinity check): the logical arrival is proven
/// by the explicit commanded application (identity plus preserved
/// frame plus visibility), not by screen position.
fn float_no_effect_proven(
    ops: &mut dyn NativeOps,
    base: &FreshWindow,
    stored: &HashMap<String, String>,
) -> bool {
    if base.frame.is_none() {
        return false;
    }
    let Some(live) = ops.read_window(&base.token) else {
        return false;
    };
    live.key == base.key
        && lifetime_ok(&live, stored)
        && live.frame == base.frame
        && live.frame.is_some()
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

    // Logical workspace ids need no native binding: the core owns them
    // outright, so membership, view, and migration targets never wait for
    // a binding that will never arrive.

    let HostDiff {
        gate,
        refused,
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
        // Never actuate membership into a held or untouched scope.
        if !scope_controlled(reply, &target_out, &target_ws) {
            stats.held += 1;
            continue;
        }
        if homing_proven(ops, base, &fresh.stored_tags, &target_out, &target_ws) {
            settle_transfer_applied(
                reply,
                gate,
                entry_obs,
                ops,
                &mut stats,
                &mut out_feedback,
                t,
                effects,
                fresh,
                &row.lifetime.0,
                false,
            );
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
            && !base.workspace.is_empty()
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
        if base.workspace.is_empty() {
            // Unknown preimage (missing receipts): never positive
            // third-location evidence. Hold for the next proven observation
            // instead of misclassifying unknown as external; the transfer
            // stays retained and retries.
            stats.held += 1;
            continue;
        }
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
        // Tiled rows carry their desired frame as the actuation target
        // (the production seam writes it as part of the move);
        // membership-only lanes (redirect movers, floats/sticky/overlays)
        // pass `None`. Tiled cross-output arrivals additionally allow a
        // mechanical remap onto the target output (position preserved,
        // never resized: the sweep re-tiles after arrival); floats pass
        // `false` and preserve their physical frame.
        let frame_target = if row.floating || base.floating {
            None
        } else {
            row.effective_frame
        };
        let remap = !row.floating && !base.floating;
        let moved = ops.move_membership(
            &row.lifetime.0,
            &target_out,
            &target_ws,
            frame_target,
            remap,
        );
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
                settle_transfer_applied(
                    reply,
                    gate,
                    entry_obs,
                    ops,
                    &mut stats,
                    &mut out_feedback,
                    t,
                    effects,
                    fresh,
                    &row.lifetime.0,
                    true,
                );
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
            (Ok(true), false)
                if base.floating && float_no_effect_proven(ops, base, &fresh.stored_tags) =>
            {
                // Existing-float crossing: membership-only, physical
                // rectangle preserved. The logical arrival settles on
                // the explicit commanded application (identity plus
                // preserved frame plus visibility), never on a
                // reinterpreted physical output.
                settle_transfer_applied(
                    reply,
                    gate,
                    entry_obs,
                    ops,
                    &mut stats,
                    &mut out_feedback,
                    t,
                    effects,
                    fresh,
                    &row.lifetime.0,
                    true,
                );
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
        // A memberless migration carries no membership phase: the target and
        // refill views below still actuate. Only unknown (unbaselined)
        // members hold; an empty set proceeds to the view gates.
        if members.iter().any(|id| !fresh.baseline.contains_key(id)) {
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
            if base.workspace.is_empty() {
                // Unknown preimage (missing receipts): never positive
                // third-location evidence. Hold without settling so the
                // migration retries on the next proven observation.
                stats.held += 1;
                superseded = true;
                break;
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
            // Migration members ride their desired frames as actuation
            // targets exactly like window transfers (redirect-lane
            // members carry none and remap mechanically instead);
            // floats preserve their frame.
            let (member_frame, member_remap) = reply
                .windows
                .iter()
                .find(|row| row.lifetime.0 == *id)
                .map(|row| {
                    let remap = !row.floating && !base.floating;
                    let frame = if remap { row.effective_frame } else { None };
                    (frame, remap)
                })
                .unwrap_or((None, false));
            let moved = ops.move_membership(id, &out, &ws, member_frame, member_remap);
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
                || effects.is_view_spent(&refill.operation_id)
            {
                // Held, scope-held, or already attempted refill:
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

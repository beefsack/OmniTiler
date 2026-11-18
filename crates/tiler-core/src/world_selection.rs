//! Selection visibility/readiness prerequisite plus terminal switch
//! settlement.
//!
//! Two narrow routes through
//! [`crate::world_tiling_views::WorldEngine::retain_view`]: a non-host-owned
//! engine with handshake `windowSuppression` support, where an actual
//! explicit `Select*`/`SwitchView` changes the view and resolves an eligible
//! focus target (focus-bearing: selected-home shows and departing-home hides
//! under the issuing revision plus one `awaitShown` focus grant), and a
//! focusless selection (no required focus: empty, minimized-only, or
//! native-owned without a focus candidate), which freezes the same
//! visibility scope with no focus effect. Window sends with `follow=true`
//! bind the same common frozen selection binding on the destination (same
//! frozen binding as an ordinary explicit selection, so one completion rule
//! covers every follow): `Window` membership settles independently with no
//! `{op}-follow` child, and the follow focus effect carries the arrival
//! grant until the destination is shown. Every other route (ordinary focus,
//! admission, hover, stay sends, ineligible follow movers) keeps
//! `awaitShown=false` with no visibility targets, exactly as before.
//!
//! This lane freezes every command-granted visibility target across all
//! affected outputs at publish, latches the first target-and-revision
//! correlated [`LastWrite`](crate::world_report::LastWrite) per frozen
//! member plus (focus-bearing only) the first exact-tuple correlated focus
//! feedback, and emits one terminal
//! [`SwitchEvent`](crate::world_report::SwitchEvent) on the existing parent
//! view operation when every frozen member has a verdict. Focusless bindings
//! report `focus:unproven` and complete once visibility obligations are
//! satisfied (zero members complete at the issuing reply); focus-bearing
//! bindings additionally require full frozen arrival readiness for a proven
//! focus. Completion releases the finite feedback binding and retires the
//! actual retained view parent flight (selection needs no logical attempt or
//! echo beyond the host-owned native readback gate); scopes and per-window
//! targets survive. Later admissions never join an old frozen arrival set;
//! they register their own revision at the next selection.
//!
//! Visibility authorization is per live lifetime (`shown`, issuing revision,
//! never fallback), stable across observation-only replies, disagreeing facts
//! and holds; only a new selection intent (or a genuinely new member under
//! one) grants a new revision. The frozen arrival snapshot is fixed at
//! publish from the final post-layout desire frames and never updated by
//! later layout changes or admissions. Feedback binds by the full opaque
//! `(id, target, issuing revision)` tuple with the first row latched: raw
//! evidence is preserved, later rows never upgrade the original attempt, and
//! the foreground verdict stays distinct from arrival readiness. No retries,
//! timers, fallback authority, or acceptance-memory rollback follow from
//! this lane.

use std::collections::{BTreeMap, BTreeSet};

use crate::directional::{OutputId, WindowId};
use crate::geometry::Rect;
use crate::session::DomainKey;
use crate::world_attempts::{
    PropertyAttempt, RetentionPolicy, VisibilityAttemptEvidence, retain_candidate,
    retain_matching_attempt, view_evidence_matches, visibility_evidence_matches,
};
use crate::world_engine::WorldState;
use crate::world_focus::{FocusVerdict, classify_focus_feedback};
use crate::world_report::{
    AfterShown, AfterShownMember, AfterShownScope, EffectKind, FocusFeedback, HeldScreenRow,
    HoldReason, OneShotEffect, ScopeAuthority, ScreenReport, SwitchEvent, SwitchFocus,
    SwitchOutcome, TransferKind, VisibilityTarget, WindowDesire, WorldCommand,
};
use crate::world_transaction::WorldTransaction;

/// Minimal carrier between the real selection command and publish only:
/// the dispatching command, the resolved focus target (if any) and its
/// selected home, the newly selected and departing homes per affected
/// output, and the issuing world revision and observation watermark.
/// `focus` is `None` for focusless selections (no required focus: empty,
/// minimized-only, ineligible focus, or native-owned without a focus
/// candidate); the target selected home is still carried in `focus_home`
/// for scope freezing and binding identity. `command_seq` is `None` for
/// commandless policy selections, which carry their existing `op-dpv-*`
/// parent in `parent_op` instead of inventing a sequence. Dropped
/// unadopted on envelope failure; restored away on command refusal via
/// the pre-command image.
#[derive(Debug, Clone)]
pub(crate) struct PendingSelection {
    pub command_seq: Option<u64>,
    pub parent_op: String,
    pub focus: Option<WindowId>,
    pub focus_home: DomainKey,
    pub selected: Vec<DomainKey>,
    pub departing: Vec<DomainKey>,
    pub affected_outputs: Vec<OutputId>,
    pub rev: u64,
    pub obs: u64,
    /// Follow sends move the focus target to the retained destination, so
    /// the focus fallback freezes the retained target instead of the
    /// target's actual staged home at grant (ordinary sticky selections
    /// keep the actual home).
    pub follow_mover: bool,
}

/// One retained per-window selection visibility authorization: the logical
/// home at grant, the desired shown state, and the issuing selection world
/// revision. Stable across observations, holds, and disagreeing facts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SelectionTarget {
    pub home: DomainKey,
    pub shown: bool,
    pub rev: u64,
}

/// One frozen switch member: every command-granted visibility target
/// across all affected outputs, frozen at publish from the retained
/// per-window map. Separate from [`FrozenArrivalMember`] (the focus-output
/// controlled shown set driving arrival readiness only): hides and
/// non-focus-output targets settle the switch through canonical visibility
/// attempt verdicts without proving arrival. Late admissions never join: only the
/// granting report's targets freeze, later revisions register against the
/// surviving scopes instead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SwitchMember {
    pub window: WindowId,
    pub home: DomainKey,
    pub shown: bool,
    pub rev: u64,
}
/// One frozen arrival member: a controlled desired-shown window on the
/// focus effect output with its retained logical home and final published
/// desire frame (`None` waives frame equality, never the need for a
/// readable live frame).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FrozenArrivalMember {
    pub window: WindowId,
    pub home: DomainKey,
    pub frame: Option<Rect>,
}

/// One retained active selection scope for one output: the selected and
/// departing homes registered by the latest focus-bearing selection on
/// that output, plus the issuing world revision. Scopes outlive the
/// finite arrival grant: a closed frozen member drops the grant, never
/// the surviving output policy, so later in-scope admissions still
/// register their own revision. Cleared only by a superseding view
/// selection, the central view reconcile, or true output loss.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SelectionScope {
    pub selected: Vec<DomainKey>,
    pub departing: Vec<DomainKey>,
    pub rev: u64,
}

/// One latched per-member switch verdict: the first target-and-revision
/// correlated canonical [`PropertyAttempt`] visibility evidence, the
/// receiving report observation (receipt watermark), and the confirmation
/// verdict computed once at receipt. The wire `LastWrite` DTO is normalized
/// once at the staged boundary and never reconstructed here. Later reports
/// never re-evaluate the historical post bound against a newer watermark: a
/// future-dated post stays failed, raw facts are preserved verbatim in the
/// canonical attempt, and later facts never upgrade the original.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LatchedSwitchEvidence {
    pub attempt: PropertyAttempt,
    pub receipt_obs: u64,
    pub confirmed: bool,
}

/// Retained selection binding: the focus carrier (`None`
/// for focusless selections, which mint no focus effect), sharing the
/// winning global request's frozen tuple where feedback correlates; the parent view
/// operation identity (`op-<cmdseq>`, for the follow-on switch dependency;
/// recorded from the dispatching command, never a new wire id), the focus
/// selected home, the issuing revision and observation, the frozen arrival
/// snapshot, the frozen switch target set, the first exact-tuple correlated
/// feedback row (if any), the first-correlated canonical visibility attempt
/// per frozen switch member, the two distinct readiness facts, and
/// the native view targets (host-owned focus-bearing and focusless alike:
/// per affected output
/// the selected home proving via the native selector readback). Bounded to
/// the latest selection per focus output; observation-only revisions keep
/// the binding. The minimal active selected/departing home scope lives in
/// [`SelectionScope`] per output instead (late admissions register against
/// scopes, never the frozen set).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RetainedSelection {
    pub effect: Option<OneShotEffect>,
    pub parent_op: String,
    pub home: DomainKey,
    pub rev: u64,
    pub obs: u64,
    pub members: Vec<FrozenArrivalMember>,
    pub switch_members: Vec<SwitchMember>,
    pub switch_evidence: BTreeMap<WindowId, LatchedSwitchEvidence>,
    pub feedback: Option<FocusFeedback>,
    pub foreground_proven: bool,
    pub arrival_ready: bool,
    pub view_targets: BTreeMap<OutputId, DomainKey>,
    pub view_raw: BTreeMap<OutputId, PropertyAttempt>,
    /// Frozen selected homes plus affected outputs from the granting
    /// [`PendingSelection`]. Ordinary selections carry one home per
    /// affected output with a shared workspace id; a native-free
    /// whole-workspace migration carries distinct selected workspaces on
    /// the target (migrated id) and the source (refill). Used only for
    /// internal scope coverage (departing-echo tolerance on the exact
    /// selected home, supersession of the whole covering binding from a
    /// non-focus output). Never published; no wire or observable reply
    /// effect beyond which binding survives.
    pub selected: Vec<DomainKey>,
    pub affected: Vec<OutputId>,
}

impl RetainedSelection {
    /// Exact `afterShown` projection of this frozen binding: the existing
    /// parent operation and issuing observation, the frozen arrival members
    /// with lifetime/home/frame, the frozen native view targets (host-owned
    /// only; native-free bindings carry empty `views`), and the frozen
    /// selected/departing scopes. No caps, no truncation, no row
    /// re-derivation: the frozen state is already exact. Departing scopes
    /// come from the surviving per-output scopes at the binding revision.
    pub(crate) fn after_shown_projection(
        &self,
        scopes: &BTreeMap<OutputId, SelectionScope>,
    ) -> AfterShown {
        let members: Vec<AfterShownMember> = self
            .members
            .iter()
            .map(|member| AfterShownMember {
                win: member.window.clone(),
                out: member.home.output.clone(),
                ws: member.home.workspace.0.clone(),
                frame: member.frame,
            })
            .collect();
        // Native-free bindings obligate no native view: empty views.
        // Host-owned bindings prove via the frozen native view targets.
        let views: Vec<AfterShownScope> = self
            .view_targets
            .values()
            .map(|home| AfterShownScope {
                out: home.output.clone(),
                ws: home.workspace.0.clone(),
            })
            .collect();
        let selected: Vec<AfterShownScope> = self
            .selected
            .iter()
            .map(|home| AfterShownScope {
                out: home.output.clone(),
                ws: home.workspace.0.clone(),
            })
            .collect();
        let mut departing: Vec<AfterShownScope> = Vec::new();
        for output in &self.affected {
            if let Some(scope) = scopes.get(output)
                && scope.rev == self.rev
            {
                for home in &scope.departing {
                    let row = AfterShownScope {
                        out: home.output.clone(),
                        ws: home.workspace.0.clone(),
                    };
                    if !departing
                        .iter()
                        .any(|kept| kept.out == row.out && kept.ws == row.ws)
                    {
                        departing.push(row);
                    }
                }
            }
        }
        AfterShown {
            op: self.parent_op.clone(),
            obs: self.obs,
            members,
            views,
            selected,
            departing,
        }
    }
}

/// Live outputs for selection retention: reported live screens plus held
/// (narrowly unreadable yet still known) screens. A held screen is never
/// true output loss: scopes, targets, and finite grants survive under it
/// with held authority until true loss or supersession. Recovery never
/// renews an unchanged target; legacy rehoming can change its home/polarity
/// and therefore require a fresh authorization.
fn selection_known_outputs(
    screens: &[ScreenReport],
    held_screens: &[HeldScreenRow],
) -> BTreeSet<OutputId> {
    screens
        .iter()
        .map(|screen| screen.output.clone())
        .chain(held_screens.iter().map(|held| held.output.clone()))
        .collect()
}

impl WorldTransaction<'_> {
    /// Whether one resolved selection focus target may take the
    /// native-free suppression-capable route: no host-owned domains,
    /// handshake suppression support, and a known-safe per-window preimage
    /// mirroring the composed sticky visibility gates (suppression
    /// available, known visibility pair, native ownership known, non-
    /// minimized, non-hidden, non-fullscreen, capability-gated maximize,
    /// no derived hold/hide/overlay, no exclusion lane). Composed-sticky
    /// members stay with the sticky producer. Raw project suppression
    /// ownership is not host hiding and never blocks eligibility, and
    /// native-owned windows never take this route.
    pub(crate) fn selection_route_eligible(
        &self,
        window: &WindowId,
        home: &DomainKey,
        held_scopes: &BTreeSet<DomainKey>,
    ) -> bool {
        if self.connection.host_owned_workspace_domains() {
            return false;
        }
        if !self
            .connection
            .capabilities()
            .is_some_and(|caps| caps.window_suppression)
        {
            return false;
        }
        if self.world.intent.restored_sticky.contains_key(window) {
            return false;
        }
        if held_scopes.contains(home) {
            return false;
        }
        if self.is_held_window(window) || self.is_excluded_window(window) {
            return false;
        }
        let Some(fact) = self.world.staged.facts.get(window) else {
            return false;
        };
        if fact.suppression_available != Some(true) {
            return false;
        }
        if fact.visibility.visible.is_none() || fact.visibility.suppression_owned.is_none() {
            return false;
        }
        let owned_supported = self.connection.capabilities().is_none_or(|caps| {
            !caps
                .unavailable_facts
                .contains(&crate::connection::FactName::Owned)
        });
        if owned_supported && fact.owned != Some(false) {
            return false;
        }
        if !owned_supported && fact.owned == Some(true) {
            return false;
        }
        if fact.minimized != Some(false)
            || fact.host_hidden != Some(false)
            || fact.fullscreen != Some(false)
        {
            return false;
        }
        let maximize_supported = self
            .connection
            .capabilities()
            .is_some_and(|caps| caps.maximize);
        if maximize_supported && fact.maximized != Some(false) {
            return false;
        }
        let derived = self.derived_for(window);
        if derived.hold
            || derived.hidden
            || derived.overlay_fullscreen
            || derived.overlay_maximized
            || derived.excluded
            || derived.own_excluded
        {
            return false;
        }
        true
    }

    /// Latest intent wins on a changed selection: drop retained active
    /// scopes, per-window targets, and awaitShown bindings for every
    /// affected output. The focus route re-grants below at publish; a
    /// non-focus route completes nothing and leaves the dropped scope
    /// without stale controlled targets.
    pub(crate) fn prune_selection_for_outputs(&mut self, affected: &[OutputId]) {
        self.world
            .settlement
            .selection_scopes
            .retain(|output, _| !affected.contains(output));
        self.world
            .settlement
            .selection_targets
            .retain(|_, entry| !affected.contains(&entry.home.output));
        self.world
            .settlement
            .retained_selection
            .retain(|output, _| !affected.contains(output));
        if self
            .world
            .settlement
            .pending_selection
            .as_ref()
            .is_some_and(|pending| {
                pending
                    .affected_outputs
                    .iter()
                    .any(|out| affected.contains(out))
            })
        {
            self.world.settlement.pending_selection = None;
        }
        // Latest intent wins over policy carriers too: a command on an
        // overlapping output supersedes the policy pending there (policy
        // never supersedes a command).
        self.world
            .settlement
            .pending_policy_selections
            .retain(|pending| {
                !pending
                    .affected_outputs
                    .iter()
                    .any(|out| affected.contains(out))
            });
    }

    /// Whether one resolved selection focus target may take the
    /// host-owned native route: host-owned domains only, with the same
    /// awaitShown carrier as the native-free route but native desktop
    /// obligations (no per-window targets, no suppression demand). The
    /// target must be a plausible arrival subject: live fact, homed,
    /// readable classification, non-minimized. Off-desktop
    /// `visibility.visible=false` is normal pre-arrival and never blocks;
    /// native ownership never blocks (every member on this lane is
    /// native-owned).
    pub(crate) fn host_selection_route_eligible(
        &self,
        window: &WindowId,
        home: &DomainKey,
        held_scopes: &BTreeSet<DomainKey>,
    ) -> bool {
        if !self.connection.host_owned_workspace_domains() {
            return false;
        }
        if held_scopes.contains(home) {
            return false;
        }
        if self.is_held_window(window) || self.is_excluded_window(window) {
            return false;
        }
        let Some(fact) = self.world.staged.facts.get(window) else {
            return false;
        };
        if fact.minimized == Some(true) {
            return false;
        }
        let derived = self.derived_for(window);
        if derived.excluded || derived.own_excluded || derived.hold || derived.hidden {
            return false;
        }
        true
    }

    /// Hard per-window skip shared by the grant-time map build and the
    /// late-admission pass: held, excluded, and composed-sticky windows
    /// never take selection targets, and native-owned or minimized or
    /// factless windows take none either (authority gates in
    /// [`Self::decorate_selection_visibility`] still decide Controlled
    /// versus Hold for every granted target).
    fn selection_member_blocked(&self, window: &WindowId) -> bool {
        if self.is_held_window(window) || self.is_excluded_window(window) {
            return true;
        }
        if self.world.intent.restored_sticky.contains_key(window) {
            return true;
        }
        let Some(fact) = self.world.staged.facts.get(window) else {
            return true;
        };
        if fact.owned == Some(true) {
            return true;
        }
        if fact.minimized == Some(true) {
            return true;
        }
        false
    }

    /// Central view reconcile for every active selection scope, before
    /// publication: any view change that bypassed the selection command
    /// path (follow sends, migrations, adoption, policy swaps) must not
    /// leave stale controlled targets behind. When the known current view
    /// of a scoped output differs from every registered selected home,
    /// latest intent wins: the scope, its per-window targets, and the old
    /// await binding on that output are superseded without granting new
    /// authority. An outstanding retained view flight targeting a selected
    /// home is the selection command path itself (actuation still proving),
    /// never a bypass, so those outputs wait for the flight. An own
    /// correlated refusal (proven refusal with an unchanged readable
    /// preimage that still echoes) is likewise never newer external
    /// input: the historical accounting pends with its latched raw,
    /// effect, and snapshot until the first exact-tuple feedback lands
    /// (or a genuinely newer intent, third-location move, output loss, or
    /// close supersedes through the existing paths). Unknown views (no
    /// ring), held selected scopes, and incomplete inventories never
    /// prove a change. A pending new select applies after this reconcile,
    /// so `retain_view`'s own prune still owns the superseding intent.
    pub(crate) fn reconcile_selection_scopes(&mut self, held_scopes: &BTreeSet<DomainKey>) {
        if self.world.settlement.selection_scopes.is_empty()
            || !self.world.staged.observation.inventory_complete
        {
            return;
        }
        let mut stale: Vec<OutputId> = Vec::new();
        for (output, scope) in &self.world.settlement.selection_scopes {
            if scope.selected.iter().any(|home| held_scopes.contains(home)) {
                continue;
            }
            let own_flight = self.world.settlement.retained.iter().any(|retained| {
                matches!(retained.target.kind, TransferKind::View)
                    && scope.selected.iter().any(|home| {
                        home.output == retained.target.target_output
                            && home.workspace.0 == retained.target.target_workspace
                    })
            });
            if own_flight {
                continue;
            }
            // An own refusal that retired the flight above still echoes
            // its preimage: not external supersession, so the binding
            // pends for its feedback instead of dropping silently.
            if self.selection_refusal_pends(output) {
                continue;
            }
            let Some(current) = self.effective_view(output) else {
                continue;
            };
            let matches = scope
                .selected
                .iter()
                .any(|home| home.output == *output && home.workspace.0 == current);
            if !matches {
                // Common departing-echo tolerance for focus-bearing
                // bindings: the adapter echo still names the scope's
                // departing home while the selection proves (a follow
                // keeps no view flight, so no own_flight guards the
                // initial echo; native-free bindings mint no native
                // obligation either). Revision-joined: an unrelated output
                // grant over shared workspace text never keeps the scope.
                // Exact selected-home coverage (not just the focus
                // workspace id) so a native-free migration with distinct
                // selected workspaces on each output tolerates lag on
                // either leg. A genuine third-location echo still drops
                // through the normal path below. Effect-gated so focusless
                // (no effect) keeps the existing removal; selected-covered
                // via the scope (not the native view_targets map) so both
                // lanes share one guard.
                let departing_echo =
                    self.world
                        .settlement
                        .retained_selection
                        .values()
                        .any(|binding| {
                            binding.effect.is_some()
                                && scope.rev == binding.rev
                                && scope
                                    .selected
                                    .iter()
                                    .any(|selected| binding.selected.contains(selected))
                                && scope.departing.iter().any(|home| {
                                    home.output == *output && home.workspace.0 == current
                                })
                        });
                if departing_echo {
                    // Explicit superseding View flight to the departing
                    // home restores the prior supersession drop: a
                    // redundant select back home mints a fresh View flight
                    // via retain_view_inner, while a follow keeps no view
                    // flight at all.
                    let superseding_view = self.world.settlement.retained.iter().any(|retained| {
                        matches!(retained.target.kind, TransferKind::View)
                            && retained.target.target_output == *output
                            && retained.target.target_workspace == current
                    });
                    if !superseding_view {
                        continue;
                    }
                }
                // Shared host-owned focus-bearing bindings are keyed by the
                // focus output but obligate every affected output. A
                // non-focus output still echoing its frozen departing home
                // is merely outstanding while the key output already proves
                // the selection: keep the scope so a later genuine
                // third-location move still drops the covering binding
                // through the normal path. Native-free migration bindings
                // leave view_targets empty but obligate both outputs via
                // the frozen selected/affected association, so the same
                // non-key cover applies there, focus-bearing and focusless
                // alike (effect-gated only for host-owned bindings, whose
                // focusless form keeps key-only removal). Key-proven-gated
                // so a superseding redundant select back home (key itself
                // stale) still drops.
                let outstanding =
                    self.world
                        .settlement
                        .retained_selection
                        .iter()
                        .any(|(key, binding)| {
                            if binding.effect.is_none() && !binding.view_targets.is_empty() {
                                return false;
                            }
                            let covers = binding.view_targets.contains_key(output)
                                || (binding.view_targets.is_empty()
                                    && *output != *key
                                    && binding.affected.contains(output));
                            if !covers {
                                return false;
                            }
                            if !scope
                                .departing
                                .iter()
                                .any(|home| home.output == *output && home.workspace.0 == current)
                            {
                                return false;
                            }
                            if let Some(home) = binding.view_targets.get(key) {
                                return self.effective_view(key).as_ref()
                                    == Some(&home.workspace.0);
                            }
                            // Native-free cover: the key output already shows its
                            // frozen selected home.
                            binding
                                .selected
                                .iter()
                                .find(|home| home.output == *key)
                                .is_some_and(|home| {
                                    self.effective_view(key).as_ref() == Some(&home.workspace.0)
                                })
                        });
                if outstanding {
                    continue;
                }
                stale.push(output.clone());
            }
        }
        // Shared host-owned focus-bearing bindings are keyed by the focus
        // output but obligate every affected output. A stale non-focus
        // output drops the whole covering binding. Native-free migration
        // bindings leave view_targets empty but cover both outputs via
        // the frozen affected association, so a stale non-key output
        // drops the whole covering binding there too (never leaving the
        // destination grant alive), focus-bearing and focusless alike;
        // only host-owned focusless and single-output native-free
        // bindings keep key-only removal. Departing echoes never reach
        // this filter: they stay out of stale above.
        let covered: Vec<OutputId> = self
            .world
            .settlement
            .retained_selection
            .iter()
            .filter(|(key, binding)| {
                if stale.contains(*key) {
                    return true;
                }
                if binding.effect.is_none() && !binding.view_targets.is_empty() {
                    return false;
                }
                stale.iter().any(|output| {
                    binding.view_targets.contains_key(output)
                        || (binding.view_targets.is_empty()
                            && *key != output
                            && binding.affected.contains(output))
                })
            })
            .map(|(key, _)| key.clone())
            .collect();
        for output in &stale {
            self.world.settlement.selection_scopes.remove(output);
            self.world
                .settlement
                .selection_targets
                .retain(|_, entry| entry.home.output != *output);
        }
        for output in covered {
            self.world.settlement.retained_selection.remove(&output);
        }
    }

    /// Whether one scoped output's retained selection binding pends on its
    /// own proven refusal: the latched exact-tuple raw refuses the frozen
    /// native obligation with an unchanged readable preimage that still
    /// echoes. Newer commands prune elsewhere; third-location echoes,
    /// output loss, and closes never satisfy this gate, so genuine
    /// supersession still drops through the existing paths.
    fn selection_refusal_pends(&self, output: &OutputId) -> bool {
        // Keyed binding first (original behavior); a shared host-owned
        // focus-bearing binding is keyed by its focus output, so a stale
        // non-focus output falls back to the covering binding whose
        // view_targets obligate it. Effect-gated: native-free bindings
        // leave view_targets empty and focusless bindings mint no effect,
        // so both keep key-only lookup.
        let binding = match self.world.settlement.retained_selection.get(output) {
            Some(binding) => binding,
            None => match self
                .world
                .settlement
                .retained_selection
                .values()
                .find(|binding| {
                    binding.effect.is_some() && binding.view_targets.contains_key(output)
                }) {
                Some(binding) => binding,
                None => return false,
            },
        };
        let Some(home) = binding.view_targets.get(output) else {
            return false;
        };
        let Some(attempt) = binding
            .view_raw
            .get(output)
            .and_then(PropertyAttempt::as_view)
        else {
            return false;
        };
        let host_owned = self.connection.host_owned_workspace_domains();
        // Production view_targets freeze host-owned only: native proof
        // only, never logical echoes.
        let echo = self.native_echo_for(output);
        self.view_refused(attempt, home.workspace.0.as_str(), host_owned)
            && self.effective_view_pre(attempt, host_owned).as_deref() == echo.as_deref()
    }

    /// Carry the pending selection into the retained per-window target map
    /// from post-command post-restage homing. Show entries cover members
    /// homed to a newly selected home, hide entries cover members homed to
    /// a departing home; legacy rows (minimized, held, excluded,
    /// native-owned, composed-sticky) keep their existing lanes. Entries
    /// keep the issuing revision; later facts never rewrite it.
    pub(crate) fn apply_pending_selection(&mut self) {
        let mut pendings: Vec<PendingSelection> = Vec::new();
        if let Some(pending) = self.world.settlement.pending_selection.clone() {
            pendings.push(pending);
        }
        pendings.extend(
            self.world
                .settlement
                .pending_policy_selections
                .iter()
                .cloned(),
        );
        for pending in &pendings {
            self.apply_one_pending_selection(pending);
        }
    }

    /// Carry one pending selection into the retained per-window target
    /// map, same grant rules for command and policy carriers.
    fn apply_one_pending_selection(&mut self, pending: &PendingSelection) {
        self.world
            .settlement
            .selection_targets
            .retain(|_, entry| !pending.affected_outputs.contains(&entry.home.output));
        // Host-owned (focus-bearing and focusless alike) freezes native
        // selector obligations only:
        // per-window visibility targets stay null (no LastWrite is ever
        // permitted for native-homed windows, and decorating publishes
        // nothing on this lane), so an empty target set here is the
        // obligation set, never a gap.
        if self.connection.host_owned_workspace_domains() {
            return;
        }
        let selected: BTreeSet<DomainKey> = pending.selected.iter().cloned().collect();
        let departing: BTreeSet<DomainKey> = pending.departing.iter().cloned().collect();
        let mut homed: Vec<(WindowId, DomainKey)> = self
            .world
            .staged
            .homing
            .iter()
            .map(|(window, home)| (window.clone(), home.clone()))
            .collect();
        homed.sort();
        for (window, home) in homed {
            let shown = if selected.contains(&home) {
                true
            } else if departing.contains(&home) {
                false
            } else {
                continue;
            };
            if self.selection_member_blocked(&window) {
                continue;
            }
            self.world.settlement.selection_targets.insert(
                window,
                SelectionTarget {
                    home,
                    shown,
                    rev: pending.rev,
                },
            );
        }
        // Native-free whole-workspace migration members ride the
        // committed target home (show) even though staged homing still
        // names the source cell until arrival: the core already moved the
        // mapping, and the frozen binding owns target visibility through
        // the existing completion. Membership still settles independently
        // through the Migration flight plus the frozen arrival brackets;
        // this grant never self-confirms it.
        if !self.connection.host_owned_workspace_domains() {
            let op = pending.parent_op.clone();
            let target = self.world.settlement.retained.iter().find_map(|retained| {
                if matches!(&retained.target.kind, TransferKind::Migration)
                    && retained.target.operation_id == op
                {
                    Some(DomainKey {
                        output: retained.target.target_output.clone(),
                        workspace: crate::directional::WorkspaceId(
                            retained.target.target_workspace.clone(),
                        ),
                    })
                } else {
                    None
                }
            });
            if let Some(target) = target {
                let movers: Vec<WindowId> = self
                    .world
                    .settlement
                    .retained
                    .iter()
                    .find(|retained| {
                        matches!(&retained.target.kind, TransferKind::Migration)
                            && retained.target.operation_id == op
                    })
                    .map(|retained| retained.members.clone())
                    .unwrap_or_default();
                for window in movers {
                    if self.selection_member_blocked(&window) {
                        continue;
                    }
                    self.world.settlement.selection_targets.insert(
                        window,
                        SelectionTarget {
                            home: target.clone(),
                            shown: true,
                            rev: pending.rev,
                        },
                    );
                }
            }
        }
    }

    /// Register late admissions against the active output scopes: any
    /// targetless window homed to a retained selected or departing home
    /// gets its own target stamped with the issuing reply revision
    /// (`state_revision + 1`), never the old authorization. Existing
    /// targets keep their revision, and the frozen arrival snapshot is
    /// untouched: late members never join old frozen arrivals. Scoped to
    /// live selection scopes, so a dropped finite grant never blocks a
    /// surviving output's policy and no new effect is resurrected.
    pub(crate) fn admit_selection_late_members(&mut self) {
        if self.connection.host_owned_workspace_domains() {
            return;
        }
        if self.world.settlement.selection_scopes.is_empty() {
            return;
        }
        let rev = self.world.state_revision + 1;
        let mut grants: Vec<(WindowId, DomainKey, bool)> = Vec::new();
        for scope in self.world.settlement.selection_scopes.values() {
            for home in scope.selected.iter().chain(scope.departing.iter()) {
                let shown = scope.selected.contains(home);
                for (window, homed) in &self.world.staged.homing {
                    if homed != home {
                        continue;
                    }
                    // A home-mismatched entry is stale (the finish veto
                    // drops it): the in-scope homing registers fresh
                    // below instead of inheriting the old revision.
                    // Same-home entries keep their revision untouched.
                    let stale = self
                        .world
                        .settlement
                        .selection_targets
                        .get(window)
                        .is_some_and(|entry| entry.home != *homed);
                    if !stale && self.world.settlement.selection_targets.contains_key(window) {
                        continue;
                    }
                    if self.selection_member_blocked(window) {
                        continue;
                    }
                    grants.push((window.clone(), home.clone(), shown));
                }
            }
        }
        grants.sort();
        for (window, home, shown) in grants {
            self.world
                .settlement
                .selection_targets
                .insert(window, SelectionTarget { home, shown, rev });
        }
    }

    /// Central native-free selection visibility decoration over the
    /// assembled publish rows. Composed-sticky rows keep precedence (they
    /// already carry a target); native-owned windows take no per-window
    /// writes. Safety, capability, per-window preimage, hold, and overlay
    /// gates mirror the composed visibility producer (unknown or
    /// native-owned or minimized or hidden or fullscreen facts withhold,
    /// maximize withholds only when the capability is supported, derived
    /// hold/hide/overlay/exclusion lanes withhold); the raw project
    /// suppression-owned flag is not host hiding and never blocks a show.
    pub(crate) fn decorate_selection_visibility(
        &self,
        held_scopes: &BTreeSet<DomainKey>,
        windows: &mut [WindowDesire],
    ) {
        if self.connection.host_owned_workspace_domains() {
            return;
        }
        let caps_ok = self
            .connection
            .capabilities()
            .is_some_and(|caps| caps.window_suppression);
        for desire in windows.iter_mut() {
            if desire.visibility.is_some() {
                continue;
            }
            let window = desire.lifetime.clone();
            let Some(entry) = self.world.settlement.selection_targets.get(&window) else {
                continue;
            };
            if desire.output != entry.home.output || desire.workspace != entry.home.workspace.0 {
                continue;
            }
            if self
                .world
                .staged
                .facts
                .get(&window)
                .is_some_and(|fact| fact.owned == Some(true))
            {
                continue;
            }
            let target = VisibilityTarget {
                shown: entry.shown,
                revision: entry.rev,
                fallback: false,
            };
            let derived = self.derived_for(&window);
            let excluded_lane =
                self.is_excluded_window(&window) || derived.excluded || derived.own_excluded;
            let domain_readable = self
                .world
                .staged
                .domains
                .iter()
                .any(|domain| domain.key == entry.home && domain.readable);
            let homing_current = self.staged_homing(&window).as_ref() == Some(&entry.home);
            // Committed whole-workspace migration members show at the
            // target under commit authority even though staged homing
            // still names the source cell until arrival (the mapping
            // already moved; the Migration flight plus arrival brackets
            // still prove membership independently, never this desire).
            // Every other gate below still applies.
            let committed_mover = !homing_current
                && !self.connection.host_owned_workspace_domains()
                && self.world.settlement.retained.iter().any(|retained| {
                    matches!(&retained.target.kind, TransferKind::Migration)
                        && retained.members.contains(&window)
                        && retained.target.target_output == entry.home.output
                        && retained.target.target_workspace == entry.home.workspace.0
                });
            // Polarity gating: a show authorizes only on the current view,
            // a hide only when the current view is known and elsewhere.
            // An unknown current view never authorizes Controlled in
            // either direction.
            let current_view = self.effective_view(&entry.home.output);
            let view_current = if entry.shown {
                current_view.as_ref() == Some(&entry.home.workspace.0)
            } else {
                current_view.is_some_and(|view| view != entry.home.workspace.0)
            };
            let mut controlled = caps_ok
                && self.world.staged.observation.inventory_complete
                && !self.world.staged.gesture_barrier
                && !held_scopes.contains(&entry.home)
                && !self.is_held_window(&window)
                && !excluded_lane
                && domain_readable
                && (homing_current || committed_mover)
                && view_current
                && (self.world.engine_knows_window_any(&window)
                    || self.world.workspace.members.contains_key(&window));
            match self.world.staged.facts.get(&window) {
                Some(fact) => {
                    if fact.suppression_available != Some(true) {
                        controlled = false;
                    }
                    if fact.visibility.visible.is_none()
                        || fact.visibility.suppression_owned.is_none()
                    {
                        controlled = false;
                    }
                    let owned_supported = self.connection.capabilities().is_none_or(|caps| {
                        !caps
                            .unavailable_facts
                            .contains(&crate::connection::FactName::Owned)
                    });
                    // Unknown ownership never defaults safe when the fact
                    // is supported; a known native-owned window never
                    // takes per-window writes on this lane.
                    if owned_supported && fact.owned.is_none() {
                        controlled = false;
                    }
                    if fact.owned == Some(true) {
                        controlled = false;
                    }
                    // Unknown never defaults false: each nullable safety
                    // input must read an explicit known-safe value.
                    // `host_hidden`/`minimized`/`fullscreen` carry no
                    // unavailable-fact name, so their gates stay
                    // unconditional. `maximized` is capability-gated:
                    // unsupported `null` is the contract-valid absent
                    // value and must not hold the show; `Some(true)`
                    // still withholds via the overlay gate below.
                    if fact.minimized != Some(false)
                        || fact.host_hidden != Some(false)
                        || fact.fullscreen != Some(false)
                    {
                        controlled = false;
                    }
                    let maximize_supported = self
                        .connection
                        .capabilities()
                        .is_some_and(|caps| caps.maximize);
                    if maximize_supported && fact.maximized != Some(false) {
                        controlled = false;
                    }
                }
                None => controlled = false,
            }
            if derived.hold
                || derived.hidden
                || derived.overlay_fullscreen
                || derived.overlay_maximized
            {
                controlled = false;
            }
            if controlled {
                desire.visibility = Some(target);
                desire.visibility_authority = ScopeAuthority::Controlled;
                continue;
            }
            desire.visibility = Some(target);
            if excluded_lane {
                desire.visibility_authority = ScopeAuthority::Untouched;
            } else if self.world.staged.gesture_barrier {
                desire.visibility_authority = ScopeAuthority::Hold(HoldReason::GestureBarrier);
            } else if !self.world.staged.observation.inventory_complete {
                desire.visibility_authority = ScopeAuthority::Hold(HoldReason::IncompleteInventory);
            } else if self
                .world
                .staged
                .facts
                .get(&window)
                .is_some_and(|fact| fact.minimized == Some(true))
            {
                desire.visibility_authority = ScopeAuthority::Hold(HoldReason::Minimized);
            } else if let Some(reason) = self.world.hold_reasons.get(&entry.home).copied() {
                desire.visibility_authority = ScopeAuthority::Hold(reason);
            } else {
                desire.visibility_authority = ScopeAuthority::Hold(HoldReason::Unreadable);
            }
        }
    }

    /// Host-owned focus-bearing arrival snapshot: every managed member
    /// homed to the selected focus home, frozen with its final published
    /// desire frame (`None` waives frame equality, never the need for a
    /// readable live frame), independent of suppression targets (this lane
    /// mints none). Gates, precisely: staged homing must equal the frozen
    /// selected home (departing homes never arrive); the published row must
    /// name the focus output and home; held, excluded (including
    /// own-process), held-classified, hidden, and minimized windows never
    /// join (they cannot prove a controlled arrival). Native ownership,
    /// off-desktop `visibility.visible=false`, unknown suppression facts,
    /// and overlay/sticky carriage never exclude: the Native-arm proof
    /// still demands visible, non-minimized, non-host-hidden reads with a
    /// matching translated home and frame. The minted focus target always
    /// freezes below with its ACTUAL staged home (a sticky focus stays
    /// homed away from the target, never fabricated onto it), except a
    /// follow mover, which freezes the retained target it relocates to;
    /// an exactly-homed focus with no published frame freezes frameless
    /// rather than dropping the required subject.
    fn host_arrival_snapshot(
        &self,
        pending: &PendingSelection,
        windows: &[WindowDesire],
    ) -> Vec<FrozenArrivalMember> {
        let mut members: Vec<FrozenArrivalMember> = Vec::new();
        for row in windows {
            if row.output != pending.focus_home.output
                || row.workspace != pending.focus_home.workspace.0
            {
                continue;
            }
            // No frame gate: every eligible required member freezes,
            // frameless rows included (null desired waives equality,
            // never the need for a readable live frame).
            if self.staged_homing(&row.lifetime).as_ref() != Some(&pending.focus_home) {
                continue;
            }
            if self.is_held_window(&row.lifetime) || self.is_excluded_window(&row.lifetime) {
                continue;
            }
            let derived = self.derived_for(&row.lifetime);
            if derived.excluded || derived.own_excluded || derived.hold || derived.hidden {
                continue;
            }
            if self
                .world
                .staged
                .facts
                .get(&row.lifetime)
                .is_none_or(|fact| fact.minimized == Some(true))
            {
                continue;
            }
            members.push(FrozenArrivalMember {
                window: row.lifetime.clone(),
                home: pending.focus_home.clone(),
                frame: row.effective_frame,
            });
        }
        // The minted effect target always freezes with its ACTUAL staged
        // home (a sticky focus stays homed away from the target, never
        // fabricated onto it). Follow movers relocate to the retained
        // target, so the fallback freezes the retained target instead.
        // Same fail-closed gates as the main loop; an unfrozen focus
        // simply holds, never completes.
        if let Some(focus) = pending.focus.clone()
            && !members.iter().any(|member| member.window == focus)
        {
            let home = if pending.follow_mover {
                Some(pending.focus_home.clone())
            } else {
                self.staged_homing(&focus)
            };
            if let Some(home) = home
                && !self.is_held_window(&focus)
                && !self.is_excluded_window(&focus)
            {
                let derived = self.derived_for(&focus);
                let blocked =
                    derived.excluded || derived.own_excluded || derived.hold || derived.hidden;
                let minimized = self
                    .world
                    .staged
                    .facts
                    .get(&focus)
                    .is_none_or(|fact| fact.minimized == Some(true));
                if !blocked && !minimized {
                    let frame = windows
                        .iter()
                        .find(|row| row.lifetime == focus)
                        .and_then(|row| row.effective_frame);
                    members.push(FrozenArrivalMember {
                        window: focus,
                        home,
                        frame,
                    });
                }
            }
        }
        members
    }

    /// Freeze the final arrival snapshot plus the active output scopes
    /// after the layout desire frames are assembled in publish: every
    /// controlled desired-shown member on the focus effect output (the
    /// composed sticky producer keeps precedence but its controlled shown
    /// rows still count: arrival needs every controlled shown member on
    /// the output) with its retained logical home and final published
    /// frame, and one active scope per affected output that late
    /// admissions register against. Later admissions never join the frozen
    /// set and later layout changes never update it. The parent view
    /// operation identity is recorded for the follow-on switch dependency;
    /// no new wire id is minted. Consumes the command pending plus every
    /// policy pending, each freezing its own binding under its own parent.
    /// Focusless selections (no focus target, no effect) freeze an
    /// empty arrival snapshot with the same scope/switch-member rules and no
    /// focus effect; zero-obligation bindings complete synchronously into
    /// the issuing reply via the common terminal predicate below.
    pub(crate) fn freeze_selection_arrival(
        &mut self,
        effects: &[OneShotEffect],
        windows: &[WindowDesire],
    ) {
        let mut pendings: Vec<PendingSelection> = Vec::new();
        if let Some(pending) = self.world.settlement.pending_selection.take() {
            pendings.push(pending);
        }
        pendings.extend(std::mem::take(
            &mut self.world.settlement.pending_policy_selections,
        ));
        for pending in pendings {
            self.freeze_one_selection(effects, windows, pending);
        }
    }

    /// Freeze one pending selection (command or policy) into its retained
    /// binding under its own parent identity.
    fn freeze_one_selection(
        &mut self,
        effects: &[OneShotEffect],
        windows: &[WindowDesire],
        pending: PendingSelection,
    ) {
        let mut effect: Option<OneShotEffect> = match pending.focus.clone() {
            Some(focus) => effects
                .iter()
                .find(|effect| {
                    effect.kind == EffectKind::Focus
                        && effect.await_shown
                        && effect.cause == crate::world_report::EffectCause::ExplicitCommand
                        && effect.command_seq == pending.command_seq
                        && effect.target == focus
                })
                .cloned(),
            None => None,
        };
        // Single-authority correlation: the retained carrier is only a
        // representation and shares the winning global request's frozen
        // tuple, so adapter feedback (keyed by the published request)
        // latches here. Each binding keeps its own frozen copy: later
        // supersession of the global slot never rewrites it, and
        // member/visibility obligations are untouched.
        if let (Some(slot), Some(repr)) = (self.world.focus.pending_focus.as_ref(), effect.as_mut())
            && slot.cause == crate::world_report::FocusRequestCause::ExplicitCommand
            && slot.cmd_seq == pending.command_seq
            && slot.target == repr.target
            && slot.issued_revision == repr.issued_revision
        {
            repr.id = slot.id.clone();
        }
        if pending.focus.is_some() && effect.is_none() {
            return;
        }
        for output in &pending.affected_outputs {
            let selected: Vec<DomainKey> = pending
                .selected
                .iter()
                .filter(|home| home.output == *output)
                .cloned()
                .collect();
            let departing: Vec<DomainKey> = pending
                .departing
                .iter()
                .filter(|home| home.output == *output)
                .cloned()
                .collect();
            if selected.is_empty() && departing.is_empty() {
                continue;
            }
            self.world.settlement.selection_scopes.insert(
                output.clone(),
                SelectionScope {
                    selected,
                    departing,
                    rev: pending.rev,
                },
            );
        }
        let focus_output = pending.focus_home.output.clone();
        let host_owned = self.connection.host_owned_workspace_domains();
        let mut members: Vec<FrozenArrivalMember> = Vec::new();
        if effect.is_some() {
            if host_owned {
                members = self.host_arrival_snapshot(&pending, windows);
            } else {
                // Committed whole-workspace migration members freeze at
                // the TARGET with their final published frame even while
                // the pre-arrival target domain still decorates Hold
                // (the commit grant names a DomainKey absent from the
                // pre-command staged domains, so domain_readable is
                // false): the Migration flight plus arrival brackets
                // still prove membership independently, never this
                // grant. Ordinary selectors keep the Controlled rule.
                let parent_op = pending.parent_op.clone();
                let committed_movers: BTreeSet<WindowId> = self
                    .world
                    .settlement
                    .retained
                    .iter()
                    .find(|retained| {
                        matches!(&retained.target.kind, TransferKind::Migration)
                            && retained.target.operation_id == parent_op
                    })
                    .map(|retained| {
                        retained
                            .members
                            .iter()
                            .cloned()
                            .collect::<BTreeSet<WindowId>>()
                    })
                    .unwrap_or_default();
                for row in windows {
                    if row.output != focus_output {
                        continue;
                    }
                    if row.visibility.is_none_or(|target| !target.shown) {
                        continue;
                    }
                    // Committed migration movers freeze the granted home
                    // (entry.home): staged homing still names the source
                    // cell until arrival while the binding already owns
                    // target visibility, and the pre-arrival target domain
                    // may decorate Hold (allowed only when the granted
                    // entry homes the focus target). Ordinary rows keep
                    // the Controlled rule with the staged_homing fallback:
                    // the composed sticky producer keeps precedence with a
                    // separate target ledger (no selection_targets entry),
                    // so requiring a granted entry here would wrongly drop
                    // sticky members from the frozen snapshot.
                    if committed_movers.contains(&row.lifetime) {
                        if row.visibility_authority != ScopeAuthority::Controlled
                            && !self
                                .world
                                .settlement
                                .selection_targets
                                .get(&row.lifetime)
                                .is_some_and(|entry| entry.home == pending.focus_home)
                        {
                            continue;
                        }
                        let Some(entry) =
                            self.world.settlement.selection_targets.get(&row.lifetime)
                        else {
                            continue;
                        };
                        members.push(FrozenArrivalMember {
                            window: row.lifetime.clone(),
                            home: entry.home.clone(),
                            frame: row.effective_frame,
                        });
                    } else {
                        if row.visibility_authority != ScopeAuthority::Controlled {
                            continue;
                        }
                        let Some(home) = self.staged_homing(&row.lifetime) else {
                            continue;
                        };
                        members.push(FrozenArrivalMember {
                            window: row.lifetime.clone(),
                            home,
                            frame: row.effective_frame,
                        });
                    }
                }
            }
            members.sort_by(|a, b| a.window.cmp(&b.window));
        }
        // Frozen switch target set: every command-granted visibility target
        // across all affected outputs (shows and hides alike, however
        // authorized), separate from the focus-output arrival snapshot
        // above. Frozen once at grant; later admissions register against
        // the surviving scopes with their own revision and never join.
        // Host-owned focusless freezes native obligations only, never
        // per-window members (no LastWrite is permitted there).
        let mut switch_members: Vec<SwitchMember> =
            if self.connection.host_owned_workspace_domains() {
                Vec::new()
            } else {
                self.world
                    .settlement
                    .selection_targets
                    .iter()
                    .filter(|(_, entry)| pending.affected_outputs.contains(&entry.home.output))
                    .map(|(window, entry)| SwitchMember {
                        window: window.clone(),
                        home: entry.home.clone(),
                        shown: entry.shown,
                        rev: entry.rev,
                    })
                    .collect()
            };
        switch_members.sort_by(|a, b| a.window.cmp(&b.window));
        // Host-owned bindings (focusless and focus-bearing alike): per
        // affected output the selected home proving via the native
        // selector readback. Native-free bindings leave this empty
        // (visibility obligations alone decide).
        let mut view_targets: BTreeMap<OutputId, DomainKey> = BTreeMap::new();
        if self.connection.host_owned_workspace_domains() {
            for output in &pending.affected_outputs {
                if let Some(home) = pending
                    .selected
                    .iter()
                    .find(|home| home.output == *output)
                    .cloned()
                {
                    view_targets.insert(output.clone(), home);
                }
            }
        }
        let parent_op = pending.parent_op.clone();
        let home = pending.focus_home.clone();
        let rev = pending.rev;
        let obs = pending.obs;
        let selected = pending.selected.clone();
        let affected = pending.affected_outputs.clone();
        let focus_key = pending.focus.clone();
        let focus_seq = pending.command_seq;
        self.world.settlement.retained_selection.insert(
            pending.focus_home.output.clone(),
            RetainedSelection {
                effect,
                parent_op,
                home,
                rev,
                obs,
                members,
                switch_members,
                switch_evidence: BTreeMap::new(),
                feedback: None,
                foreground_proven: false,
                arrival_ready: false,
                view_targets,
                view_raw: BTreeMap::new(),
                selected,
                affected,
            },
        );
        // Same-freeze `afterShown` copy: when the retained focus winner is
        // the selection-backed explicit request this binding was just
        // frozen for (same command sequence and focus target), project the
        // binding's exact frozen state into it. No row re-derivation, no
        // caps, no truncation; parent settlement later cannot erase the
        // copy. Direct/directional explicit winners have no binding and
        // keep `None`.
        if let (Some(focus), Some(focus_seq)) = (focus_key, focus_seq) {
            let matches = self.world.focus.pending_focus.as_ref().is_some_and(|slot| {
                slot.cause == crate::world_report::FocusRequestCause::ExplicitCommand
                    && slot.cmd_seq == Some(focus_seq)
                    && slot.target == focus
                    && slot.after_shown.is_none()
            });
            if matches
                && let Some(output) = self
                    .world
                    .focus
                    .pending_focus
                    .as_ref()
                    .map(|slot| slot.output.clone())
                && let Some(binding) = self
                    .world
                    .settlement
                    .retained_selection
                    .get(&output)
                    .cloned()
                && binding.effect.as_ref().is_some_and(|effect| {
                    effect.command_seq == Some(focus_seq) && effect.target == focus
                })
            {
                let projection =
                    binding.after_shown_projection(&self.world.settlement.selection_scopes);
                if let Some(slot) = self.world.focus.pending_focus.as_mut() {
                    slot.after_shown = Some(projection);
                }
            }
        }
        // Zero-obligation completion at the earliest normal publication: a
        // binding with no frozen switch members, no feedback gate, and no
        // native obligation is vacuously satisfied, so the issuing reply
        // carries its terminal through the common predicate below, command
        // and policy carriers alike (no timer, poll, or artificial future
        // wake; policy issuing terminals size pre-mutation via
        // `policy_terminal_count_v4`). Host-owned focusless never completes
        // here: its native obligation still reads Pending with no raw yet.
        // No evidence latches here: same-report raw facts would guess a
        // future issuing revision.
        if let Some(binding) = self
            .world
            .settlement
            .retained_selection
            .get(&focus_output)
            .cloned()
            && let Some(event) =
                selection_terminal(&focus_output, &binding, NativeSelectionProof::Pending)
        {
            self.stage_selection_terminal(&focus_output, event);
        }
    }

    /// Consume accepted-report focus feedback against the retained
    /// selection bindings, before the command-only rollback boundary and
    /// independent of sweep readability holds. First exact opaque
    /// `(id, target, issuing revision)` match wins per binding, within one
    /// report and across later ones; raw feedback is preserved and later
    /// contradictory rows never upgrade the original attempt. Fresh facts
    /// never upgrade the first row. Focusless bindings mint no effect and
    /// latch no feedback.
    pub(crate) fn settle_selection_feedback(&mut self) {
        let homing: BTreeMap<WindowId, DomainKey> = self
            .world
            .staged
            .homing
            .iter()
            .map(|(window, home)| (window.clone(), home.clone()))
            .collect();
        let report_obs = self.world.staged.observation.observation_seq;
        // Minimal native binding context for the arrival proof: the
        // host-owned flag selects the Native arm and the map translates
        // the observed native home through this engine's own bindings.
        // Cloned once per report ahead of the per-binding mutable borrow.
        let host_owned = self.connection.host_owned_workspace_domains();
        let native_map = self.world.settlement.native_map.clone();
        let attempts = &self.world.staged.observation.property_attempts;
        for binding in self.world.settlement.retained_selection.values_mut() {
            // First exact-correlated canonical view evidence per obligated
            // output latches here (same pre-rollback position as feedback and
            // LastWrite): the sequencer pending map forgets acknowledged
            // receipts, and a held Refused receipt consumed on one report
            // must still prove on a later recovery that omits it.
            // Wrong-tuple rows never correlate and never latch.
            for (output, home) in binding.view_targets.clone() {
                if binding.view_raw.contains_key(&output) {
                    continue;
                }
                let operation_id = binding.parent_op.clone();
                let target_workspace = home.workspace.0.clone();
                let mut slot: Option<PropertyAttempt> = None;
                retain_matching_attempt(
                    attempts,
                    |attempt| {
                        attempt.as_view().is_some_and(|evidence| {
                            view_evidence_matches(
                                evidence,
                                &operation_id,
                                &output,
                                &target_workspace,
                            )
                        })
                    },
                    &mut slot,
                    RetentionPolicy::FirstMatch,
                );
                if let Some(attempt) = slot {
                    binding.view_raw.insert(output, attempt);
                }
            }
            let Some(effect) = binding.effect.as_ref() else {
                continue;
            };
            if binding.feedback.is_some() {
                continue;
            }
            for row in &self.world.staged.observation.focus_feedback {
                if row.id == effect.id
                    && row.target == effect.target
                    && row.revision == effect.issued_revision
                {
                    binding.foreground_proven = classify_focus_feedback(&effect.target, row)
                        == crate::world_focus::FocusVerdict::Proven;
                    binding.arrival_ready = prove_selection_arrival(
                        &binding.members,
                        row,
                        &homing,
                        binding.obs,
                        report_obs,
                        host_owned,
                        &native_map,
                    );
                    // Host-owned invariant: the minted focus target always
                    // freezes with its actual staged home, so a binding that
                    // omits its own effect target never reads arrival-ready;
                    // unfrozen focus simply holds, never completes. The
                    // producer route always freezes the target, so this
                    // omission is not proven reachable (artificial invariant,
                    // not a product path).
                    if host_owned
                        && !binding
                            .members
                            .iter()
                            .any(|member| member.window == effect.target)
                    {
                        binding.arrival_ready = false;
                    }
                    binding.feedback = Some(row.clone());
                    break;
                }
            }
        }
    }

    /// Correlate canonical visibility evidence for all bindings in one pass.
    /// A temporary per-window reference index preserves the old keyed
    /// complexity: one traversal of the canonical attempt lane builds a
    /// `window -> first staged row` map (`or_insert`, so healthy-first and the
    /// first decodable held row keep precedence), and each member then does a
    /// keyed lookup plus exact revision/target check. This is a borrowed
    /// internal index, not a second evidence store; only matched candidates
    /// are cloned into the returned pending rows.
    fn index_visibility_switches(&self) -> BTreeMap<WindowId, &PropertyAttempt> {
        let mut index: BTreeMap<WindowId, &PropertyAttempt> = BTreeMap::new();
        for attempt in &self.world.staged.observation.property_attempts {
            if let Some(evidence) = attempt.as_visibility() {
                index.entry(evidence.window.clone()).or_insert(attempt);
            }
        }
        index
    }

    /// Exact (window, revision, target) correlation for one binding's
    /// unresolved members over the borrowed index, latching each candidate
    /// once through the shared `retain_candidate` FirstMatch primitive. A
    /// wrong-kind or wrong-tuple row reads as absent and never shadows a later
    /// exact row; the first retained receipt wins and later reports cannot
    /// upgrade it.
    fn correlate_visibility_switches(
        index: &BTreeMap<WindowId, &PropertyAttempt>,
        binding: &RetainedSelection,
    ) -> Vec<(WindowId, PropertyAttempt)> {
        let mut pending: Vec<(WindowId, PropertyAttempt)> = Vec::new();
        for member in &binding.switch_members {
            if binding.switch_evidence.contains_key(&member.window) {
                continue;
            }
            let Some(attempt) = index.get(&member.window) else {
                continue;
            };
            if !attempt.as_visibility().is_some_and(|evidence| {
                visibility_evidence_matches(evidence, &member.window, member.shown, member.rev)
            }) {
                continue;
            }
            let mut slot: Option<PropertyAttempt> = None;
            if retain_candidate(attempt, &mut slot, RetentionPolicy::FirstMatch)
                && let Some(attempt) = slot
            {
                pending.push((member.window.clone(), attempt));
            }
        }
        pending
    }

    /// Settle retained selection switches against first-correlated canonical
    /// visibility attempts plus (focus-bearing only) the latched focus
    /// feedback, once per accepted report before the command-only rollback
    /// boundary (so consumed and observation-only reports settle exactly
    /// like command reports, and a rejected command's pre-command image
    /// keeps the staged terminals). Per frozen switch member the first
    /// target-and-revision-correlated row latches: raw evidence is
    /// preserved, later contradictory rows and observation gaps never
    /// upgrade the original attempt, and held rows keep latched evidence.
    /// Absence or uncorrelated/mismatched rows are no verdict (never a
    /// timeout failure); every correlated member verdict completes the
    /// switch, confirmed or failed, without retry.
    ///
    /// Focusless bindings need no feedback and no arrival: every correlated
    /// member verdict (zero vacuously) completes with `focus:unproven`,
    /// gate is conditional on the minted effect: bindings with an effect
    /// need the first correlated feedback; focusless bindings (`None`)
    /// report `Unproven`. Completion needs every frozen member verdict
    /// confirmed-equivalent (zero vacuously complete) AND, for focus-bearing
    /// bindings, proven focus with full arrival readiness, AND, for
    /// native-obligated bindings, a proven native selector readback. A
    /// proven native refusal terminates through the same predicate with a
    /// failure outcome (never a second mint path).
    ///
    /// Completion mints one [`SwitchEvent`] on the existing parent
    /// operation into the take-once store, releases the finite feedback
    /// binding, and retires the actual retained view parent flight as well
    /// (selection completes with no logical view attempt or echo beyond the
    /// host-owned native readback gate: views advance at retain time and
    /// history converges later in `converge_logical_post`, and neither is
    /// adopted here). Scopes and per-window
    /// targets survive the terminal. An explicit close of the focus target
    /// (focus-bearing) or any frozen member drops the binding without an
    /// event, like the existing grant prune. Empty native-free focusless
    /// bindings already complete at the issuing reply; they never reach
    /// this settle.
    pub(crate) fn settle_selection_switch(&mut self) {
        if self.world.settlement.retained_selection.is_empty() {
            return;
        }
        let report_obs = self.world.staged.observation.observation_seq;
        let closed: BTreeSet<WindowId> = self.world.staged.closes.iter().cloned().collect();
        let outputs: Vec<OutputId> = self
            .world
            .settlement
            .retained_selection
            .keys()
            .cloned()
            .collect();
        // Keyed intake for the whole phase: build the borrowed window index
        // once, prepare each binding's pending rows, then drop the borrow before
        // any per-output mutable policy work. Per-output policy order below is
        // unchanged.
        let index = self.index_visibility_switches();
        let mut pending_by_output: BTreeMap<OutputId, Vec<(WindowId, PropertyAttempt)>> =
            BTreeMap::new();
        for output in &outputs {
            if let Some(binding) = self.world.settlement.retained_selection.get(output) {
                let pending = Self::correlate_visibility_switches(&index, binding);
                if !pending.is_empty() {
                    pending_by_output.insert(output.clone(), pending);
                }
            }
        }
        drop(index);
        for output in outputs {
            let issuing_obs = match self.world.settlement.retained_selection.get(&output) {
                Some(binding) => binding.obs,
                None => continue,
            };
            // Latch first-correlated canonical evidence before the terminal
            // below. Staging already ordered healthy facts before held rows
            // (malformed held raw skipped), so FirstMatch preserves the exact
            // graded/held precedence and latches once at this receipt.
            let pending = pending_by_output.remove(&output).unwrap_or_default();
            let native_proof = {
                let binding = match self.world.settlement.retained_selection.get(&output) {
                    Some(binding) => binding.clone(),
                    None => continue,
                };
                self.selection_native_proof(&binding)
            };
            let Some(binding) = self.world.settlement.retained_selection.get_mut(&output) else {
                continue;
            };
            let focus_closed = binding
                .effect
                .as_ref()
                .is_some_and(|effect| closed.contains(&effect.target));
            if focus_closed
                || binding
                    .switch_members
                    .iter()
                    .any(|member| closed.contains(&member.window))
            {
                continue;
            }
            for (window, attempt) in pending {
                // Verdict latches once at receipt: the historical post bound
                // is checked against this report only, never re-evaluated
                // against a later watermark. Property proof policy sits above
                // the common retention; it reads only canonical raw facts.
                let member = binding
                    .switch_members
                    .iter()
                    .find(|member| member.window == window)
                    .cloned();
                let Some(member) = member else {
                    continue;
                };
                let confirmed = attempt.as_visibility().is_some_and(|evidence| {
                    switch_confirms_target(&member, evidence, issuing_obs, report_obs)
                });
                binding.switch_evidence.insert(
                    window,
                    LatchedSwitchEvidence {
                        attempt,
                        receipt_obs: report_obs,
                        confirmed,
                    },
                );
            }
            // One common terminal for both optional-focus states: the
            // shared predicate gates feedback only when an effect was
            // minted, reports Unproven without one, and folds visibility,
            // focus, and native proof into one event construction. Missing
            // verdicts hold (never a failure); every verdict present
            // terminates, confirmed or failed, without retry.
            let proof = native_proof;
            let event = match self.world.settlement.retained_selection.get(&output) {
                Some(current) => selection_terminal(&output, current, proof),
                None => continue,
            };
            let Some(event) = event else {
                continue;
            };
            let output_key = output.clone();
            self.stage_selection_terminal(&output_key, event);
        }
    }

    /// Native selector proof for one binding: vacuous when the binding
    /// carries no native obligation, else exact fresh native echo per
    /// obligated output proves. The first exact-tuple correlated raw (the
    /// latched pre-rollback raw when present, else fresh from this report)
    /// reuses the existing selector machinery: an accepted post proving the
    /// target proves the write landed, and a proven refusal (unchanged
    /// readable preimage plus a readable echo naming the preimage, on a
    /// complete inventory with a complete screen and no gesture barrier)
    /// fails. Anything else holds, including wrong-tuple rows (never
    /// latched, read as absent) and logical `ScreenView` echoes (host-owned
    /// reads native only via [`WorldEngine::native_echo_for`]). A stronger
    /// fresh row never upgrades the latched original: only the independent
    /// current-target echo above proves on fresh facts. Genuine external
    /// moves hold here; the existing scope reconcile at publish supersedes
    /// the affected binding without an event.
    fn selection_native_proof(&self, binding: &RetainedSelection) -> NativeSelectionProof {
        if binding.view_targets.is_empty() {
            return NativeSelectionProof::Proven;
        }
        let host_owned = self.connection.host_owned_workspace_domains();
        let mut refused = false;
        for (output, home) in &binding.view_targets {
            let target = home.workspace.0.as_str();
            // Production view_targets freeze host-owned only: native proof
            // only, never logical echoes.
            let echo = self.native_echo_for(output);
            if echo.as_deref() == Some(target) {
                continue;
            }
            let fresh = self
                .world
                .staged
                .observation
                .property_attempts
                .iter()
                .filter_map(PropertyAttempt::as_view)
                .find(|evidence| {
                    evidence.operation_id == binding.parent_op
                        && evidence.target_output == *output
                        && evidence.target_workspace == target
                });
            let latched = binding
                .view_raw
                .get(output)
                .and_then(PropertyAttempt::as_view);
            let Some(attempt) = latched.or(fresh) else {
                return NativeSelectionProof::Pending;
            };
            if self.view_post_proves_target(attempt, host_owned) {
                continue;
            }
            if !self.view_refused(attempt, target, host_owned) {
                return NativeSelectionProof::Pending;
            }
            let pre = self.effective_view_pre(attempt, host_owned);
            let complete_screen = self
                .world
                .staged
                .screens
                .iter()
                .find(|screen| screen.output == *output)
                .map(|screen| screen.complete);
            if pre.is_some()
                && echo.as_deref() == pre.as_deref()
                && self.world.staged.observation.inventory_complete
                && complete_screen == Some(true)
                && !self.world.staged.gesture_barrier
            {
                refused = true;
                continue;
            }
            return NativeSelectionProof::Pending;
        }
        if refused {
            NativeSelectionProof::Refused
        } else {
            NativeSelectionProof::Proven
        }
    }

    /// Stage one settled switch terminal: release the finite feedback
    /// binding, retire the actual retained view parent flight, and update
    /// the output's retained `switch_settled` slot latest-wins. The single
    /// stager for both the normal settle path and the zero-obligation
    /// issuing point.
    fn stage_selection_terminal(&mut self, output: &OutputId, event: SwitchEvent) {
        let parent = event.operation_id.clone();
        self.world.settlement.retained_selection.remove(output);
        self.world.settlement.retained.retain(|retained| {
            !(retained.target.operation_id == parent
                && matches!(retained.target.kind, TransferKind::View))
        });
        self.world
            .settlement
            .switch_settled
            .insert(output.clone(), event);
    }
}

impl WorldState {
    /// Count of `(output_count, total_failed_rows)` for retained and
    /// simultaneous switch outcomes across `switch_settled` and
    /// `retained_selection`.
    pub(crate) fn switch_settle_counts(&self) -> (usize, usize) {
        if self.settlement.retained_selection.is_empty()
            && self.settlement.switch_settled.is_empty()
        {
            return (0, 0);
        }
        let mut count = 0usize;
        let mut failed = 0usize;
        for (output, binding) in &self.settlement.retained_selection {
            count += 1;
            let existing = self
                .settlement
                .switch_settled
                .get(output)
                .map_or(0, |event| event.failed.len());
            failed = failed.saturating_add(binding.switch_members.len().max(existing));
        }
        for (output, event) in &self.settlement.switch_settled {
            if !self.settlement.retained_selection.contains_key(output) {
                count += 1;
                failed = failed.saturating_add(event.failed.len());
            }
        }
        (count, failed)
    }

    /// Count of zero-obligation issuing terminals (`0` or `1`) a
    /// select-family or whole-workspace migration command can mint while old
    /// bindings settle.
    pub(crate) fn switch_issuing_count(&self, command: Option<&WorldCommand>) -> usize {
        let selects = matches!(
            command,
            Some(
                WorldCommand::SwitchView { .. }
                    | WorldCommand::SelectWorkspace { .. }
                    | WorldCommand::SelectNumbered { .. }
                    | WorldCommand::SelectRelative { .. }
                    | WorldCommand::SelectPrevious { .. }
                    | WorldCommand::SelectTrailing { .. }
                    | WorldCommand::MigrateWorkspace { .. }
                    | WorldCommand::MigrateDirectional { .. }
            )
        );
        usize::from(selects)
    }

    /// Count of policy-view terminal switch events knowable before
    /// convergence over one v4 report's screen lane.
    pub(crate) fn policy_terminal_count_v4(
        &self,
        report: &crate::world_report::WorldReportV4,
        workspace_mode: crate::workspace_scope::WorkspaceMode,
    ) -> usize {
        let live: BTreeSet<&str> = report
            .screens
            .iter()
            .map(|screen| screen.output.0.as_str())
            .collect();
        self.policy_terminal_count_for_live(&live, workspace_mode)
    }

    fn policy_terminal_count_for_live(
        &self,
        live: &BTreeSet<&str>,
        workspace_mode: crate::workspace_scope::WorkspaceMode,
    ) -> usize {
        if workspace_mode == crate::workspace_scope::WorkspaceMode::Shared {
            return 0;
        }
        let mut bound: usize = 0;
        for (origin, entry) in &self.workspace.displaced {
            if live.contains(origin.as_str()) {
                bound = bound.saturating_add(entry.workspaces.len());
            }
        }
        for (output, ring) in &self.workspace.mapping {
            if !live.contains(output.as_str()) {
                bound = bound.saturating_add(ring.len());
            }
        }
        bound
    }
}

impl WorldTransaction<'_> {
    /// Prune retained selection bindings on explicit close or proven
    /// complete departure of the focus target (focus-bearing only) or any
    /// frozen member, or on true output loss (grounded complete inventory
    /// with neither a live nor a held screen for the output, outside hotplug
    /// displacement). A held screen preserves the grant under held authority;
    /// incomplete inventories and held facts are not closure. Member closes
    /// drop only the finite grant: surviving output scopes and per-window
    /// targets persist independently. Pruning drops the binding without
    /// claiming readiness.
    pub(crate) fn prune_selection_grants(&mut self) {
        let observation = &self.world.staged.observation;
        if !observation.closed.is_empty() {
            let closed: BTreeSet<WindowId> = observation.closed.iter().cloned().collect();
            self.world
                .settlement
                .retained_selection
                .retain(|_, binding| {
                    if binding
                        .effect
                        .as_ref()
                        .is_some_and(|effect| closed.contains(&effect.target))
                    {
                        return false;
                    }
                    if binding
                        .members
                        .iter()
                        .any(|member| closed.contains(&member.window))
                    {
                        return false;
                    }
                    !binding
                        .switch_members
                        .iter()
                        .any(|member| closed.contains(&member.window))
                });
        }
        if observation.inventory_complete {
            let live = selection_known_outputs(
                &self.world.staged.screens,
                &self.world.staged.held_screens,
            );
            self.world
                .settlement
                .retained_selection
                .retain(|output, _| {
                    live.contains(output) || self.world.workspace.displaced.contains_key(&output.0)
                });
        }
        if !observation.inventory_complete {
            return;
        }
        let live: BTreeSet<&WindowId> = observation
            .raw_windows
            .iter()
            .map(|fact| &fact.window)
            .collect();
        let held: BTreeSet<&WindowId> = self
            .world
            .staged
            .held_windows
            .iter()
            .map(|held| &held.window)
            .collect();
        let excluded = self.world.staged.excluded.clone();
        let known: BTreeSet<WindowId> = self
            .world
            .settlement
            .retained_selection
            .values()
            .flat_map(|binding| {
                binding
                    .effect
                    .as_ref()
                    .map(|effect| effect.target.clone())
                    .into_iter()
                    .chain(binding.members.iter().map(|member| member.window.clone()))
            })
            .filter(|target| {
                self.world.workspace.members.contains_key(target)
                    || self.world.engine_knows_window_any(target)
            })
            .collect();
        self.world
            .settlement
            .retained_selection
            .retain(|_, binding| {
                binding
                    .effect
                    .as_ref()
                    .map(|effect| effect.target.clone())
                    .into_iter()
                    .chain(binding.members.iter().map(|member| member.window.clone()))
                    .all(|target| {
                        live.contains(&target)
                            || held.contains(&target)
                            || excluded.contains(&target)
                            || known.contains(&target)
                    })
            });
    }

    /// Prune active selection scopes on true output loss only (complete
    /// inventory with neither a live nor a held screen for the output,
    /// outside hotplug displacement). Held screens and incomplete
    /// inventories preserve every scope; closes and superseding views
    /// never clear scopes here (closes drop only the finite grant, views
    /// reconcile centrally before publication).
    pub(crate) fn prune_selection_scopes(&mut self) {
        if !self.world.staged.observation.inventory_complete {
            return;
        }
        let live =
            selection_known_outputs(&self.world.staged.screens, &self.world.staged.held_screens);
        self.world.settlement.selection_scopes.retain(|output, _| {
            live.contains(output) || self.world.workspace.displaced.contains_key(&output.0)
        });
    }

    /// Prune retained per-window selection visibility authorizations.
    /// Explicit closes always drop. A staged home change vetoes the stale
    /// entry (a reused lifetime must never inherit the old revision), and
    /// true output loss (neither live nor held screen) drops scopes homed
    /// there. Mere absence drops only on a complete inventory for windows
    /// neither live, held, excluded, nor engine/member-known; holds,
    /// exclusions, incomplete inventories, and unknown homings preserve
    /// the entry without granting authority.
    pub(crate) fn prune_selection_targets(&mut self) {
        let observation = &self.world.staged.observation;
        if !observation.closed.is_empty() {
            let closed: BTreeSet<WindowId> = observation.closed.iter().cloned().collect();
            self.world
                .settlement
                .selection_targets
                .retain(|window, _| !closed.contains(window));
        }
        let live_outputs =
            selection_known_outputs(&self.world.staged.screens, &self.world.staged.held_screens);
        let lost: BTreeSet<OutputId> = if observation.inventory_complete {
            self.world
                .settlement
                .selection_targets
                .values()
                .map(|entry| entry.home.output.clone())
                .filter(|output| {
                    !live_outputs.contains(output)
                        && !self.world.workspace.displaced.contains_key(&output.0)
                })
                .collect()
        } else {
            BTreeSet::new()
        };
        if !lost.is_empty() {
            self.world
                .settlement
                .selection_targets
                .retain(|_, entry| !lost.contains(&entry.home.output));
        }
        if observation.inventory_complete {
            let live: BTreeSet<&WindowId> = observation
                .raw_windows
                .iter()
                .map(|fact| &fact.window)
                .collect();
            let held: BTreeSet<&WindowId> = self
                .world
                .staged
                .held_windows
                .iter()
                .map(|held| &held.window)
                .collect();
            let excluded = self.world.staged.excluded.clone();
            let known: BTreeSet<WindowId> = self
                .world
                .settlement
                .selection_targets
                .keys()
                .filter(|window| {
                    self.world.workspace.members.contains_key(*window)
                        || self.world.engine_knows_window_any(window)
                })
                .cloned()
                .collect();
            self.world
                .settlement
                .selection_targets
                .retain(|window, entry| {
                    if lost.contains(&entry.home.output) {
                        return false;
                    }
                    if let Some(homed) = self.world.staged.homing.get(window) {
                        return homed == &entry.home;
                    }
                    live.contains(window)
                        || held.contains(window)
                        || excluded.contains(window)
                        || known.contains(window)
                });
        } else {
            self.world
                .settlement
                .selection_targets
                .retain(|window, entry| {
                    if lost.contains(&entry.home.output) {
                        return false;
                    }
                    self.world
                        .staged
                        .homing
                        .get(window)
                        .is_none_or(|homed| homed == &entry.home)
                });
        }
    }
}

/// Shared switch terminal predicate for both optional-focus states (the
/// one event construction): returns the terminal [`SwitchEvent`] once
/// every frozen switch member has a latched verdict, `None` while any
/// member is still pending. Zero members are vacuously terminal. The
/// feedback gate applies only when an effect was minted; without one the
/// focus reports [`SwitchFocus::Unproven`]. A pending native obligation
/// holds; a proven native refusal terminates with a failure outcome
/// through this same construction. Outcome is [`SwitchOutcome::Complete`]
/// iff nothing failed, no focus proof was required-or-missing, and no
/// native refusal latched.
fn selection_terminal(
    output: &OutputId,
    binding: &RetainedSelection,
    native: NativeSelectionProof,
) -> Option<SwitchEvent> {
    if binding
        .switch_members
        .iter()
        .any(|member| !binding.switch_evidence.contains_key(&member.window))
    {
        return None;
    }
    let mut failed: Vec<WindowId> = Vec::new();
    for member in &binding.switch_members {
        let evidence = binding.switch_evidence.get(&member.window)?;
        if !evidence.confirmed {
            failed.push(member.window.clone());
        }
    }
    let focus = match binding.effect.as_ref() {
        None => SwitchFocus::Unproven,
        Some(effect) => {
            let feedback = binding.feedback.as_ref()?;
            match classify_focus_feedback(&effect.target, feedback) {
                FocusVerdict::Proven if binding.arrival_ready => SwitchFocus::Proven,
                FocusVerdict::Proven => SwitchFocus::Unproven,
                FocusVerdict::Refused => SwitchFocus::Refused,
                FocusVerdict::Unproven => SwitchFocus::Unproven,
            }
        }
    };
    if !binding.view_targets.is_empty() && native == NativeSelectionProof::Pending {
        return None;
    }
    let complete = failed.is_empty()
        && (binding.effect.is_none() || focus == SwitchFocus::Proven)
        && native != NativeSelectionProof::Refused;
    let outcome = if complete {
        SwitchOutcome::Complete
    } else {
        SwitchOutcome::CompletedWithFailures
    };
    Some(SwitchEvent {
        output: output.clone(),
        operation_id: binding.parent_op.clone(),
        outcome,
        failed,
        focus,
        fallback: false,
    })
}

/// Native selector proof state for one host-owned focusless binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativeSelectionProof {
    Proven,
    Refused,
    Pending,
}

/// Target postcondition proof for one frozen switch member against its
/// first-correlated canonical visibility attempt: the identity-bound post
/// sample names the same live lifetime, shows the granted state with clean
/// native facts, and sits inside the frozen issuing-to-receipt observation
/// bracket. Acceptance alone never confirms; an unreadable or mismatched post
/// is a correlated failure, never a confirmation. Only the enclosing row
/// (exact target/revision, latched by the caller) reaches this proof. The
/// caller latches the verdict once at receipt; later watermarks never
/// re-evaluate it. This is the property proof policy above the shared
/// visibility retention, reading only canonical raw facts.
fn switch_confirms_target(
    member: &SwitchMember,
    evidence: &VisibilityAttemptEvidence,
    issuing_obs: u64,
    receipt_obs: u64,
) -> bool {
    let Some(post) = evidence.post else {
        return false;
    };
    if post.obs < issuing_obs || post.obs > receipt_obs {
        return false;
    }
    if post.lifetime_matched != Some(true) {
        return false;
    }
    if post.visibility.visible != Some(member.shown) {
        return false;
    }
    if post.minimized != Some(false) || post.host_hidden != Some(false) {
        return false;
    }
    true
}

/// Arrival proof over one frozen snapshot against one feedback row: every
/// frozen required shown member needs a coherent bracket matching its
/// retained logical home and output plus its frozen frame. The location
/// arm is capability-exclusive: host-owned bindings demand the Native
/// arm with the observed native home translated through this engine's
/// own bindings on the carried output and compared to the frozen core
/// target (never a logical echo); native-free bindings demand the
/// Output arm naming the frozen output. Mixed, wrong-host, wrong-home,
/// or null arms never prove. Both reads must show a matched lifetime
/// with visible, non-minimized, non-hidden state; the bracket watermark
/// must equal its own before/after watermarks (never the later report
/// watermark), must not predate the frozen issuing observation, and must
/// not postdate the feedback report watermark. A null desired frame
/// waives equality only, never the need for a readable live frame.
/// Unknown facts, wrong homes/outputs/frames, mismatched watermarks,
/// lifetime tags, and reused lifetimes never prove readiness.
fn prove_selection_arrival(
    members: &[FrozenArrivalMember],
    row: &FocusFeedback,
    homing: &BTreeMap<WindowId, DomainKey>,
    issuing_obs: u64,
    report_obs: u64,
    host_owned: bool,
    native_map: &BTreeMap<(String, String), String>,
) -> bool {
    if members.is_empty() {
        return false;
    }
    members.iter().all(|member| {
        if homing.get(&member.window) != Some(&member.home) {
            return false;
        }
        row.arrival.iter().any(|bracket| {
            if bracket.window != member.window {
                return false;
            }
            if bracket.obs < issuing_obs || bracket.obs > report_obs {
                return false;
            }
            if bracket.obs != bracket.before.obs || bracket.after.obs != bracket.obs {
                return false;
            }
            if host_owned {
                let crate::world_report::ArrivalLocation::Native { home } = &bracket.location
                else {
                    return false;
                };
                let Some(home) = home.as_ref() else {
                    return false;
                };
                let crate::world_report::WorkspaceRef::Native { output, workspace } = home else {
                    return false;
                };
                if output != &member.home.output {
                    return false;
                }
                if native_map.get(&(output.0.clone(), workspace.as_str().to_owned()))
                    != Some(&member.home.workspace.0)
                {
                    return false;
                }
            } else {
                let crate::world_report::ArrivalLocation::Output { out } = &bracket.location else {
                    return false;
                };
                if out.as_ref() != Some(&member.home.output) {
                    return false;
                }
            }
            for sample in [&bracket.before, &bracket.after] {
                if sample.lifetime_matched != Some(true) {
                    return false;
                }
                if sample.visibility.visible != Some(true) {
                    return false;
                }
                if sample.minimized != Some(false) || sample.host_hidden != Some(false) {
                    return false;
                }
            }
            let Some(live) = bracket.frame else {
                return false;
            };
            if let Some(desired) = member.frame
                && live != desired
            {
                return false;
            }
            true
        })
    })
}

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
//! visibility scope with no focus effect. Every other route (ordinary focus,
//! admission, hover, sends) keeps `awaitShown=false` with no visibility
//! targets, exactly as before.
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
use crate::world_engine::WorldEngine;
use crate::world_focus::{FocusVerdict, classify_focus_feedback};
use crate::world_report::{
    EffectKind, FocusFeedback, HeldScreenRow, HoldReason, LastWrite, LastWriteProperty,
    OneShotEffect, ScopeAuthority, ScreenReport, SwitchEvent, SwitchFocus, SwitchOutcome,
    TransferKind, VisibilityTarget, WindowDesire, WorldCommand, WorldReport,
};

/// Minimal carrier between the real selection command and publish only:
/// the dispatching command, the resolved focus target (if any) and its
/// selected home, the newly selected and departing homes per affected
/// output, and the issuing world revision and observation watermark.
/// `focus` is `None` for focusless selections (no required focus: empty,
/// minimized-only, or native-owned without a focus candidate); the target
/// selected home is still carried in `focus_home` for scope freezing and
/// binding identity. Dropped unadopted on envelope failure; restored away
/// on command refusal via the pre-command image.
#[derive(Debug, Clone)]
pub(crate) struct PendingSelection {
    pub command_seq: u64,
    pub focus: Option<WindowId>,
    pub focus_home: DomainKey,
    pub selected: Vec<DomainKey>,
    pub departing: Vec<DomainKey>,
    pub affected_outputs: Vec<OutputId>,
    pub rev: u64,
    pub obs: u64,
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
/// non-focus-output targets settle the switch through [`LastWrite`]
/// verdicts without proving arrival. Late admissions never join: only the
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
/// correlated [`LastWrite`](crate::world_report::LastWrite), the receiving
/// report observation (receipt watermark), and the confirmation verdict
/// computed once at receipt. Later reports never re-evaluate the historical
/// post bound against a newer watermark: a future-dated post stays failed,
/// raw facts are preserved, and later facts never upgrade the original.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LatchedSwitchEvidence {
    pub last: LastWrite,
    pub receipt_obs: u64,
    pub confirmed: bool,
}

/// Retained selection binding: the actually emitted focus effect (`None`
/// for focusless selections, which mint no focus effect), the parent view
/// operation identity (`op-<cmdseq>`, for the follow-on switch dependency;
/// recorded from the dispatching command, never a new wire id), the focus
/// selected home, the issuing revision and observation, the frozen arrival
/// snapshot, the frozen switch target set, the first exact-tuple correlated
/// feedback row (if any), the first-correlated [`LastWrite`]
/// evidence per frozen switch member, the two distinct readiness facts, and
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
    pub view_raw: BTreeMap<OutputId, crate::world_report::ViewAttempt>,
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

impl WorldEngine {
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
        if self.host_owned_workspace_domains() {
            return false;
        }
        if !self
            .capabilities()
            .is_some_and(|caps| caps.window_suppression)
        {
            return false;
        }
        if self.restored_sticky.contains_key(window) {
            return false;
        }
        if held_scopes.contains(home) {
            return false;
        }
        if self.is_held_window(window) || self.is_excluded_window(window) {
            return false;
        }
        let Some(fact) = self.staged.facts.get(window) else {
            return false;
        };
        if fact.suppression_available != Some(true) {
            return false;
        }
        if fact.visibility.visible.is_none() || fact.visibility.suppression_owned.is_none() {
            return false;
        }
        let owned_supported = self.capabilities().is_none_or(|caps| {
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
        let maximize_supported = self.capabilities().is_some_and(|caps| caps.maximize);
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
        self.selection_scopes
            .retain(|output, _| !affected.contains(output));
        self.selection_targets
            .retain(|_, entry| !affected.contains(&entry.home.output));
        self.retained_selection
            .retain(|output, _| !affected.contains(output));
        if self.pending_selection.as_ref().is_some_and(|pending| {
            pending
                .affected_outputs
                .iter()
                .any(|out| affected.contains(out))
        }) {
            self.pending_selection = None;
        }
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
        if !self.host_owned_workspace_domains() {
            return false;
        }
        if held_scopes.contains(home) {
            return false;
        }
        if self.is_held_window(window) || self.is_excluded_window(window) {
            return false;
        }
        let Some(fact) = self.staged.facts.get(window) else {
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
        if self.restored_sticky.contains_key(window) {
            return true;
        }
        let Some(fact) = self.staged.facts.get(window) else {
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
    pub(crate) fn reconcile_selection_scopes(
        &mut self,
        report: &WorldReport,
        held_scopes: &BTreeSet<DomainKey>,
    ) {
        if self.selection_scopes.is_empty() || !self.staged.observation.inventory_complete {
            return;
        }
        let mut stale: Vec<OutputId> = Vec::new();
        for (output, scope) in &self.selection_scopes {
            if scope.selected.iter().any(|home| held_scopes.contains(home)) {
                continue;
            }
            let own_flight = self.retained.iter().any(|retained| {
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
            if self.selection_refusal_pends(output, report) {
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
                // Shared host-owned focus-bearing bindings are keyed by the
                // focus output but obligate every affected output. A
                // non-focus output still echoing its frozen departing home
                // is merely outstanding while the key output already proves
                // the selection: keep the scope so a later genuine
                // third-location move still drops the covering binding
                // through the normal path. Effect-gated so native-free
                // (empty view_targets) and focusless (no effect) keep
                // key-only removal; key-proven-gated so a superseding
                // redundant select back home (key itself stale) still drops.
                let outstanding = self.retained_selection.iter().any(|(key, binding)| {
                    binding.effect.is_some()
                        && binding.view_targets.contains_key(output)
                        && scope
                            .departing
                            .iter()
                            .any(|home| home.output == *output && home.workspace.0 == current)
                        && binding.view_targets.get(key).is_some_and(|home| {
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
        // output drops the whole covering binding. Departing echoes never
        // reach this filter: they stay out of stale above.
        // Native-free bindings leave view_targets empty and host-owned
        // focusless bindings mint no effect, so both keep key-only removal.
        let covered: Vec<OutputId> = self
            .retained_selection
            .iter()
            .filter(|(key, binding)| {
                if stale.contains(*key) {
                    return true;
                }
                if binding.effect.is_none() {
                    return false;
                }
                stale
                    .iter()
                    .any(|output| binding.view_targets.contains_key(output))
            })
            .map(|(key, _)| key.clone())
            .collect();
        for output in &stale {
            self.selection_scopes.remove(output);
            self.selection_targets
                .retain(|_, entry| entry.home.output != *output);
        }
        for output in covered {
            self.retained_selection.remove(&output);
        }
    }

    /// Whether one scoped output's retained selection binding pends on its
    /// own proven refusal: the latched exact-tuple raw refuses the frozen
    /// native obligation with an unchanged readable preimage that still
    /// echoes. Newer commands prune elsewhere; third-location echoes,
    /// output loss, and closes never satisfy this gate, so genuine
    /// supersession still drops through the existing paths.
    fn selection_refusal_pends(&self, output: &OutputId, report: &WorldReport) -> bool {
        // Keyed binding first (original behavior); a shared host-owned
        // focus-bearing binding is keyed by its focus output, so a stale
        // non-focus output falls back to the covering binding whose
        // view_targets obligate it. Effect-gated: native-free bindings
        // leave view_targets empty and focusless bindings mint no effect,
        // so both keep key-only lookup.
        let binding = match self.retained_selection.get(output) {
            Some(binding) => binding,
            None => match self.retained_selection.values().find(|binding| {
                binding.effect.is_some() && binding.view_targets.contains_key(output)
            }) {
                Some(binding) => binding,
                None => return false,
            },
        };
        let Some(home) = binding.view_targets.get(output) else {
            return false;
        };
        let Some(attempt) = binding.view_raw.get(output) else {
            return false;
        };
        let host_owned = self.host_owned_workspace_domains();
        let echo = self.view_echo_for(report, output);
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
        let Some(pending) = self.pending_selection.clone() else {
            return;
        };
        self.selection_targets
            .retain(|_, entry| !pending.affected_outputs.contains(&entry.home.output));
        // Host-owned (focus-bearing and focusless alike) freezes native
        // selector obligations only:
        // per-window visibility targets stay null (no LastWrite is ever
        // permitted for native-homed windows, and decorating publishes
        // nothing on this lane), so an empty target set here is the
        // obligation set, never a gap.
        if self.host_owned_workspace_domains() {
            return;
        }
        let selected: BTreeSet<DomainKey> = pending.selected.into_iter().collect();
        let departing: BTreeSet<DomainKey> = pending.departing.into_iter().collect();
        let mut homed: Vec<(WindowId, DomainKey)> = self
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
            self.selection_targets.insert(
                window,
                SelectionTarget {
                    home,
                    shown,
                    rev: pending.rev,
                },
            );
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
        if self.host_owned_workspace_domains() {
            return;
        }
        if self.selection_scopes.is_empty() {
            return;
        }
        let rev = self.state_revision + 1;
        let mut grants: Vec<(WindowId, DomainKey, bool)> = Vec::new();
        for scope in self.selection_scopes.values() {
            for home in scope.selected.iter().chain(scope.departing.iter()) {
                let shown = scope.selected.contains(home);
                for (window, homed) in &self.staged.homing {
                    if homed != home {
                        continue;
                    }
                    // A home-mismatched entry is stale (the finish veto
                    // drops it): the in-scope homing registers fresh
                    // below instead of inheriting the old revision.
                    // Same-home entries keep their revision untouched.
                    let stale = self
                        .selection_targets
                        .get(window)
                        .is_some_and(|entry| entry.home != *homed);
                    if !stale && self.selection_targets.contains_key(window) {
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
            self.selection_targets
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
        if self.host_owned_workspace_domains() {
            return;
        }
        let caps_ok = self
            .capabilities()
            .is_some_and(|caps| caps.window_suppression);
        for desire in windows.iter_mut() {
            if desire.visibility.is_some() {
                continue;
            }
            let window = desire.lifetime.clone();
            let Some(entry) = self.selection_targets.get(&window) else {
                continue;
            };
            if desire.output != entry.home.output || desire.workspace != entry.home.workspace.0 {
                continue;
            }
            if self
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
                .staged
                .domains
                .iter()
                .any(|domain| domain.key == entry.home && domain.readable);
            let homing_current = self.staged_homing(&window).as_ref() == Some(&entry.home);
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
                && self.staged.observation.inventory_complete
                && !self.staged.gesture_barrier
                && !held_scopes.contains(&entry.home)
                && !self.is_held_window(&window)
                && !excluded_lane
                && domain_readable
                && homing_current
                && view_current
                && (self.engine_knows_window_any(&window) || self.members.contains_key(&window));
            match self.staged.facts.get(&window) {
                Some(fact) => {
                    if fact.suppression_available != Some(true) {
                        controlled = false;
                    }
                    if fact.visibility.visible.is_none()
                        || fact.visibility.suppression_owned.is_none()
                    {
                        controlled = false;
                    }
                    let owned_supported = self.capabilities().is_none_or(|caps| {
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
                    let maximize_supported = self.capabilities().is_some_and(|caps| caps.maximize);
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
            } else if self.staged.gesture_barrier {
                desire.visibility_authority = ScopeAuthority::Hold(HoldReason::GestureBarrier);
            } else if !self.staged.observation.inventory_complete {
                desire.visibility_authority = ScopeAuthority::Hold(HoldReason::IncompleteInventory);
            } else if self
                .staged
                .facts
                .get(&window)
                .is_some_and(|fact| fact.minimized == Some(true))
            {
                desire.visibility_authority = ScopeAuthority::Hold(HoldReason::Minimized);
            } else if let Some(reason) = self.hold_reasons.get(&entry.home).copied() {
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
    /// homed away from the target, never fabricated onto it); an
    /// exactly-homed focus with no published frame freezes frameless
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
        // fabricated onto it). Same fail-closed gates as the main loop;
        // an unfrozen focus simply holds, never completes.
        if let Some(focus) = pending.focus.clone()
            && !members.iter().any(|member| member.window == focus)
            && let Some(home) = self.staged_homing(&focus)
            && !self.is_held_window(&focus)
            && !self.is_excluded_window(&focus)
        {
            let derived = self.derived_for(&focus);
            let blocked =
                derived.excluded || derived.own_excluded || derived.hold || derived.hidden;
            let minimized = self
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
    /// operation identity (`op-<cmdseq>`) is recorded for the follow-on
    /// switch dependency; no new wire id is minted. Consumes the pending
    /// carrier. Focusless selections (no focus target, no effect) freeze an
    /// empty arrival snapshot with the same scope/switch-member rules and no
    /// focus effect; zero-member native-free focusless bindings complete
    /// synchronously into the issuing reply via the shared focusless
    /// terminal predicate.
    pub(crate) fn freeze_selection_arrival(
        &mut self,
        effects: &[OneShotEffect],
        windows: &[WindowDesire],
    ) {
        let Some(pending) = self.pending_selection.take() else {
            return;
        };
        let effect: Option<OneShotEffect> = match pending.focus.clone() {
            Some(focus) => effects
                .iter()
                .find(|effect| {
                    effect.kind == EffectKind::Focus
                        && effect.await_shown
                        && effect.cause == crate::world_report::EffectCause::ExplicitCommand
                        && effect.command_seq == Some(pending.command_seq)
                        && effect.target == focus
                })
                .cloned(),
            None => None,
        };
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
            self.selection_scopes.insert(
                output.clone(),
                SelectionScope {
                    selected,
                    departing,
                    rev: pending.rev,
                },
            );
        }
        let focus_output = pending.focus_home.output.clone();
        let host_owned = self.host_owned_workspace_domains();
        let mut members: Vec<FrozenArrivalMember> = Vec::new();
        if effect.is_some() {
            if host_owned {
                members = self.host_arrival_snapshot(&pending, windows);
            } else {
                for row in windows {
                    if row.output != focus_output {
                        continue;
                    }
                    if row.visibility.is_none_or(|target| !target.shown) {
                        continue;
                    }
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
            members.sort_by(|a, b| a.window.cmp(&b.window));
        }
        // Frozen switch target set: every command-granted visibility target
        // across all affected outputs (shows and hides alike, however
        // authorized), separate from the focus-output arrival snapshot
        // above. Frozen once at grant; later admissions register against
        // the surviving scopes with their own revision and never join.
        // Host-owned focusless freezes native obligations only, never
        // per-window members (no LastWrite is permitted there).
        let mut switch_members: Vec<SwitchMember> = if self.host_owned_workspace_domains() {
            Vec::new()
        } else {
            self.selection_targets
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
        if self.host_owned_workspace_domains() {
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
        let parent_op = format!("op-{}", pending.command_seq);
        let home = pending.focus_home.clone();
        let rev = pending.rev;
        let obs = pending.obs;
        self.retained_selection.insert(
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
            },
        );
        // Zero-obligation completion at the earliest normal publication: a
        // binding with no frozen switch members, no feedback gate, and no
        // native obligation is vacuously satisfied, so the issuing reply
        // carries its terminal through the shared predicate (no timer,
        // poll, or artificial future wake). Host-owned focusless never
        // completes here: its native obligation still reads Pending with
        // no raw yet. No evidence latches here: same-report raw facts
        // would guess a future issuing revision.
        if let Some(binding) = self.retained_selection.get(&focus_output).cloned()
            && let Some(event) = selection_terminal(&binding, NativeSelectionProof::Pending)
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
            .staged
            .homing
            .iter()
            .map(|(window, home)| (window.clone(), home.clone()))
            .collect();
        let report_obs = self.staged.observation.observation_seq;
        // Minimal native binding context for the arrival proof: the
        // host-owned flag selects the Native arm and the map translates
        // the observed native home through this engine's own bindings.
        // Cloned once per report ahead of the per-binding mutable borrow.
        let host_owned = self.host_owned_workspace_domains();
        let native_map = self.native_map.clone();
        for binding in self.retained_selection.values_mut() {
            // First exact-correlated native view raw per obligated output
            // latches here (same pre-rollback position as feedback and
            // LastWrite): the sequencer pending map forgets acknowledged
            // receipts, and a held Refused receipt consumed on one report
            // must still prove on a later recovery that omits it.
            // Wrong-tuple rows never correlate and never latch.
            for (output, home) in binding.view_targets.clone() {
                if binding.view_raw.contains_key(&output) {
                    continue;
                }
                if let Some(attempt) =
                    self.staged
                        .observation
                        .view_attempts
                        .iter()
                        .find(|attempt| {
                            attempt.operation_id == binding.parent_op
                                && attempt.target_output == output
                                && attempt.target_workspace == home.workspace.0.as_str()
                        })
                {
                    binding.view_raw.insert(output, attempt.clone());
                }
            }
            let Some(effect) = binding.effect.as_ref() else {
                continue;
            };
            if binding.feedback.is_some() {
                continue;
            }
            for row in &self.staged.observation.focus_feedback {
                if row.id == effect.id
                    && row.target == effect.target
                    && row.revision == effect.issued_revision
                {
                    binding.foreground_proven = classify_focus_feedback(effect, row)
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

    /// Settle retained selection switches against first-correlated
    /// [`LastWrite`] evidence plus (focus-bearing only) the latched focus
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
    pub(crate) fn settle_selection_switch(&mut self, report: &WorldReport) {
        if self.retained_selection.is_empty() {
            return;
        }
        let report_obs = self.staged.observation.observation_seq;
        let closed: BTreeSet<WindowId> = self.staged.closes.iter().cloned().collect();
        let outputs: Vec<OutputId> = self.retained_selection.keys().cloned().collect();
        for output in outputs {
            let issuing_obs = match self.retained_selection.get(&output) {
                Some(binding) => binding.obs,
                None => continue,
            };
            // Latch first-correlated evidence before any mutable borrow:
            // live facts first, else the lossless held raw (same
            // native-state pattern: accepted evidence before holds, without
            // treating held visibility as healthy).
            let mut pending: Vec<(WindowId, LastWrite)> = Vec::new();
            if let Some(binding) = self.retained_selection.get(&output) {
                for member in &binding.switch_members {
                    if binding.switch_evidence.contains_key(&member.window) {
                        continue;
                    }
                    let Some(last) = self.selection_last_write_for(&member.window) else {
                        continue;
                    };
                    if last.property == LastWriteProperty::Visibility
                        && last.target == member.shown
                        && last.revision == member.rev
                    {
                        pending.push((member.window.clone(), last));
                    }
                }
            }
            let native_proof = {
                let binding = match self.retained_selection.get(&output) {
                    Some(binding) => binding.clone(),
                    None => continue,
                };
                self.selection_native_proof(&binding, report)
            };
            let Some(binding) = self.retained_selection.get_mut(&output) else {
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
            for (window, last) in pending {
                // Verdict latches once at receipt: the historical post bound
                // is checked against this report only, never re-evaluated
                // against a later watermark.
                let member = binding
                    .switch_members
                    .iter()
                    .find(|member| member.window == window)
                    .cloned();
                let Some(member) = member else {
                    continue;
                };
                let confirmed = switch_confirms_target(&member, &last, issuing_obs, report_obs);
                binding.switch_evidence.insert(
                    window,
                    LatchedSwitchEvidence {
                        last,
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
            let event = match self.retained_selection.get(&output) {
                Some(current) => selection_terminal(current, proof),
                None => continue,
            };
            let Some(event) = event else {
                continue;
            };
            let output_key = output.clone();
            self.stage_selection_terminal(&output_key, event);
        }
    }

    /// Native selector proof for one host-owned focusless binding: exact
    /// fresh native echo per affected output proves; a proven refusal (exact
    /// correlated `Refused` attempt with unchanged readable preimage plus a
    /// readable echo naming the preimage, on a complete inventory with a
    /// complete screen and no gesture barrier) fails; anything else holds.
    /// Logical `ScreenView` echoes never count (host-owned reads native
    /// only via [`WorldEngine::view_echo_for`]); wrong-tuple attempts read
    /// as absent.
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
    /// reads native only via [`WorldEngine::view_echo_for`]). A stronger
    /// fresh row never upgrades the latched original: only the independent
    /// current-target echo above proves on fresh facts. Genuine external
    /// moves hold here; the existing scope reconcile at publish supersedes
    /// the affected binding without an event.
    fn selection_native_proof(
        &self,
        binding: &RetainedSelection,
        report: &WorldReport,
    ) -> NativeSelectionProof {
        if binding.view_targets.is_empty() {
            return NativeSelectionProof::Proven;
        }
        let host_owned = self.host_owned_workspace_domains();
        let mut refused = false;
        for (output, home) in &binding.view_targets {
            let target = home.workspace.0.as_str();
            if self.view_echo_for(report, output).as_deref() == Some(target) {
                continue;
            }
            let fresh = report.view_attempts.iter().find(|attempt| {
                attempt.operation_id == binding.parent_op
                    && attempt.target_output == *output
                    && attempt.target_workspace == target
            });
            let latched = binding.view_raw.get(output);
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
            let echo = self.view_echo_for(report, output);
            let complete_screen = report
                .screens
                .iter()
                .find(|screen| screen.output == *output)
                .map(|screen| screen.complete);
            if pre.is_some()
                && echo.as_deref() == pre.as_deref()
                && report.inventory_complete
                && complete_screen == Some(true)
                && !report.gesture_barrier
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
    /// binding, retire the actual retained view parent flight, and push the
    /// event into the take-once store. The single stager for both the
    /// normal settle path and the zero-obligation issuing point.
    fn stage_selection_terminal(&mut self, output: &OutputId, event: SwitchEvent) {
        let parent = event.operation_id.clone();
        self.retained_selection.remove(output);
        self.retained.retain(|retained| {
            !(retained.target.operation_id == parent
                && matches!(retained.target.kind, TransferKind::View))
        });
        self.switch_settled.push(event);
    }

    /// First raw visibility write for one frozen switch member: live staged
    /// facts first, else the lossless held-row raw decoded by lifetime.
    /// Held presence alone proves nothing; only the decoded [`LastWrite`]
    /// post proof (checked by the caller at receipt) decides.
    fn selection_last_write_for(&self, window: &WindowId) -> Option<LastWrite> {
        if let Some(fact) = self.staged.facts.get(window)
            && let Some(last) = fact.last_write
        {
            return Some(last);
        }
        for held in &self.staged.held_windows {
            if held.window != *window {
                continue;
            }
            if let Some(last) = decode_held_last_write(held) {
                return Some(last);
            }
        }
        None
    }

    /// Pre-mutation worst-case reserve for simultaneous switch completions:
    /// at most one terminal event per retained selection binding, each
    /// failed list a subset of its frozen switch members. Zero when no
    /// binding is retained.
    pub(crate) fn switch_settle_reserve(&self) -> usize {
        if self.retained_selection.is_empty() {
            return 0;
        }
        let failed: usize = self
            .retained_selection
            .values()
            .map(|binding| binding.switch_members.len())
            .sum();
        crate::world_sizing::max_switch_settle_reserve(self.retained_selection.len(), failed)
    }

    /// Pre-mutation reserve for the at-most-one zero-obligation issuing
    /// terminal a select-family command can mint while old bindings settle:
    /// a focusless select with no frozen members, no feedback gate, and no
    /// native obligation completes synchronously into the issuing reply
    /// through the shared terminal predicate, and it needs no retained
    /// binding, so [`Self::switch_settle_reserve`] alone would undercount
    /// the simultaneous case. Conservative: at most one empty terminal for
    /// the commands that route through [`WorldEngine::retain_view`]; every
    /// other report reserves nothing beyond the retained bindings.
    pub(crate) fn switch_issuing_reserve(&self, command: Option<&WorldCommand>) -> usize {
        let selects = matches!(
            command,
            Some(
                WorldCommand::SwitchView { .. }
                    | WorldCommand::SelectWorkspace { .. }
                    | WorldCommand::SelectNumbered { .. }
                    | WorldCommand::SelectRelative { .. }
                    | WorldCommand::SelectPrevious { .. }
                    | WorldCommand::SelectTrailing { .. }
            )
        );
        if selects {
            crate::world_sizing::max_switch_settle_reserve(1, 0)
        } else {
            0
        }
    }

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
        let observation = &self.staged.observation;
        if !observation.closed.is_empty() {
            let closed: BTreeSet<WindowId> = observation.closed.iter().cloned().collect();
            self.retained_selection.retain(|_, binding| {
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
            let live = selection_known_outputs(&self.staged.screens, &self.staged.held_screens);
            self.retained_selection.retain(|output, _| {
                live.contains(output) || self.displaced.contains_key(&output.0)
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
            .staged
            .held_windows
            .iter()
            .map(|held| &held.window)
            .collect();
        let excluded = self.staged.excluded.clone();
        let known: BTreeSet<WindowId> = self
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
                self.members.contains_key(target) || self.engine_knows_window_any(target)
            })
            .collect();
        self.retained_selection.retain(|_, binding| {
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
        if !self.staged.observation.inventory_complete {
            return;
        }
        let live = selection_known_outputs(&self.staged.screens, &self.staged.held_screens);
        self.selection_scopes
            .retain(|output, _| live.contains(output) || self.displaced.contains_key(&output.0));
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
        let observation = &self.staged.observation;
        if !observation.closed.is_empty() {
            let closed: BTreeSet<WindowId> = observation.closed.iter().cloned().collect();
            self.selection_targets
                .retain(|window, _| !closed.contains(window));
        }
        let live_outputs = selection_known_outputs(&self.staged.screens, &self.staged.held_screens);
        let lost: BTreeSet<OutputId> = if observation.inventory_complete {
            self.selection_targets
                .values()
                .map(|entry| entry.home.output.clone())
                .filter(|output| {
                    !live_outputs.contains(output) && !self.displaced.contains_key(&output.0)
                })
                .collect()
        } else {
            BTreeSet::new()
        };
        if !lost.is_empty() {
            self.selection_targets
                .retain(|_, entry| !lost.contains(&entry.home.output));
        }
        if observation.inventory_complete {
            let live: BTreeSet<&WindowId> = observation
                .raw_windows
                .iter()
                .map(|fact| &fact.window)
                .collect();
            let held: BTreeSet<&WindowId> = self
                .staged
                .held_windows
                .iter()
                .map(|held| &held.window)
                .collect();
            let excluded = self.staged.excluded.clone();
            let known: BTreeSet<WindowId> = self
                .selection_targets
                .keys()
                .filter(|window| {
                    self.members.contains_key(*window) || self.engine_knows_window_any(window)
                })
                .cloned()
                .collect();
            self.selection_targets.retain(|window, entry| {
                if lost.contains(&entry.home.output) {
                    return false;
                }
                if let Some(homed) = self.staged.homing.get(window) {
                    return homed == &entry.home;
                }
                live.contains(window)
                    || held.contains(window)
                    || excluded.contains(window)
                    || known.contains(window)
            });
        } else {
            self.selection_targets.retain(|window, entry| {
                if lost.contains(&entry.home.output) {
                    return false;
                }
                self.staged
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
            match classify_focus_feedback(effect, feedback) {
                FocusVerdict::Proven if binding.arrival_ready => SwitchFocus::Proven,
                FocusVerdict::Proven => SwitchFocus::Unproven,
                FocusVerdict::Refused => SwitchFocus::Refused,
                // Classify never returns Pending for a correlated row; the
                // arm is unreachable but exhaustive, and maps to Unproven.
                FocusVerdict::Unproven | FocusVerdict::Pending => SwitchFocus::Unproven,
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
/// first-correlated [`LastWrite`]: the post read names the same live
/// lifetime, shows the granted state with clean native facts, and sits
/// inside the frozen issuing-to-receipt observation bracket. Acceptance
/// alone never confirms; an unreadable or mismatched post is a correlated
/// failure, never a confirmation. Only the enclosing row (exact
/// target/revision, latched by the caller) reaches this proof. The caller
/// latches the verdict once at receipt; later watermarks never re-evaluate
/// it.
fn switch_confirms_target(
    member: &SwitchMember,
    last: &LastWrite,
    issuing_obs: u64,
    receipt_obs: u64,
) -> bool {
    let Some(post) = last.post else {
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

/// Decode one lossless held-row `lastWrite` from retained raw fields.
/// Returns `None` when the row carries no `lastWrite`, carries `null`, or
/// carries a shape that does not decode to a typed [`LastWrite`]:
/// uncorrelated, never a failure and never a bypass of the post proof.
fn decode_held_last_write(held: &crate::world_report::HeldWindowRow) -> Option<LastWrite> {
    use crate::connection::RawSettingValue;
    use crate::world_report::{
        LastWritePath, LastWriteProperty, VisibilityReason, VisibilitySample, VisibilitySetter,
    };
    let (_, raw) = held.fields.iter().find(|(key, _)| key == "lastWrite")?;
    let RawSettingValue::Object(entries) = raw else {
        return None;
    };
    let get = |key: &str| entries.iter().find(|(k, _)| k == key).map(|(_, v)| v);
    let as_str = |key: &str| match get(key)? {
        RawSettingValue::String(text) => Some(text.as_str()),
        _ => None,
    };
    let property = match as_str("property")? {
        "visibility" => LastWriteProperty::Visibility,
        _ => return None,
    };
    let target = match get("target")? {
        RawSettingValue::Bool(value) => *value,
        _ => return None,
    };
    let revision = as_str("revision")?.parse::<u64>().ok()?;
    let path = match as_str("path")? {
        "staged" => LastWritePath::Staged,
        "fallback" => LastWritePath::Fallback,
        "independent" => LastWritePath::Independent,
        _ => return None,
    };
    let setter = match as_str("setter")? {
        "accepted" => VisibilitySetter::Accepted,
        "refused" => VisibilitySetter::Refused,
        "threw" => VisibilitySetter::Threw,
        "not-called" => VisibilitySetter::NotCalled,
        _ => return None,
    };
    let reason = match as_str("reason")? {
        "" => VisibilityReason::Empty,
        "identity-stale" => VisibilityReason::IdentityStale,
        "scope-held" => VisibilityReason::ScopeHeld,
        "unreadable" => VisibilityReason::Unreadable,
        "unsupported" => VisibilityReason::Unsupported,
        "preimage-unavailable" => VisibilityReason::PreimageUnavailable,
        "foreground-changed" => VisibilityReason::ForegroundChanged,
        "fullscreen" => VisibilityReason::Fullscreen,
        "game" => VisibilityReason::Game,
        "show-blocked" => VisibilityReason::ShowBlocked,
        "focus-unproven" => VisibilityReason::FocusUnproven,
        "superseded" => VisibilityReason::Superseded,
        "native-refused" => VisibilityReason::NativeRefused,
        _ => return None,
    };
    fn decode_sample(value: &RawSettingValue) -> Option<VisibilitySample> {
        use crate::connection::RawSettingValue;
        use crate::world_report::VisibilityFact;
        let RawSettingValue::Object(entries) = value else {
            return None;
        };
        let get = |key: &str| entries.iter().find(|(k, _)| k == key).map(|(_, v)| v);
        let obs = match get("obs")? {
            RawSettingValue::String(text) => text.parse::<u64>().ok()?,
            _ => return None,
        };
        let opt_bool = |key: &str| match get(key)? {
            RawSettingValue::Null => Some(None),
            RawSettingValue::Bool(value) => Some(Some(*value)),
            _ => None,
        };
        let visibility = match get("visibility")? {
            RawSettingValue::Object(visibility) => {
                let find = |key: &str| visibility.iter().find(|(k, _)| k == key).map(|(_, v)| v);
                let opt = |key: &str| match find(key)? {
                    RawSettingValue::Null => Some(None),
                    RawSettingValue::Bool(value) => Some(Some(*value)),
                    _ => None,
                };
                VisibilityFact {
                    visible: opt("visible")?,
                    suppression_owned: opt("suppressionOwned")?,
                }
            }
            _ => return None,
        };
        Some(VisibilitySample {
            obs,
            lifetime_matched: opt_bool("lifetimeMatched")?,
            visibility,
            minimized: opt_bool("minimized")?,
            host_hidden: opt_bool("hostHidden")?,
        })
    }
    let pre = match get("pre")? {
        RawSettingValue::Null => None,
        value => Some(decode_sample(value)?),
    };
    let post = match get("post")? {
        RawSettingValue::Null => None,
        value => Some(decode_sample(value)?),
    };
    Some(LastWrite {
        property,
        target,
        revision,
        path,
        setter,
        reason,
        pre,
        post,
    })
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

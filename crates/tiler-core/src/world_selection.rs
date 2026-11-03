//! Native-free suppression-capable explicit selection visibility/readiness
//! prerequisite.
//!
//! One narrow route only: a non-host-owned engine with handshake
//! `windowSuppression` support, where an actual explicit `Select*`/`SwitchView`
//! routed through [`crate::world_tiling_views::WorldEngine::retain_view`]
//! changes the view and resolves an eligible focus target. That route
//! publishes, in the issuing reply, selected-home shows and departing-home
//! hides under the issuing world revision plus one `awaitShown` focus grant.
//! Every other route (ordinary focus, admission, hover, sends, non-focus and
//! native-owned selections) keeps `awaitShown=false` with no visibility
//! targets, exactly as before.
//!
//! This lane is an integrated visibility/readiness prerequisite only: it
//! emits no `SwitchEvent`, completes no selector flight, and mints no full
//! native-free selection completion. Later admissions never join an old
//! frozen arrival set; they register their own revision at the focus-bearing
//! selection.
//!
//! Visibility authorization is per live lifetime (`shown`, issuing revision,
//! never fallback), stable across observation-only replies, disagreeing facts
//! and holds; only a new selection intent (or a genuinely new member under
//! one) grants a new revision. The frozen arrival snapshot is fixed at
//! publish from the final post-layout desire frames and never updated by
//! later layout changes or admissions. Feedback binds by the full opaque
//! `(id, target, issuing revision)` tuple with the first row latched: raw
//! evidence is preserved, later rows never upgrade the original attempt, and
//! the foreground verdict stays distinct from arrival readiness. No switch
//! events, retries, timers, or fallback authority follow from this lane.

use std::collections::{BTreeMap, BTreeSet};

use crate::directional::{OutputId, WindowId};
use crate::geometry::Rect;
use crate::session::DomainKey;
use crate::world_engine::WorldEngine;
use crate::world_focus::classify_focus_feedback;
use crate::world_report::{
    EffectKind, FocusFeedback, HoldReason, OneShotEffect, ScopeAuthority, VisibilityTarget,
    WindowDesire, WorldReport,
};

/// Minimal carrier between the real selection command and publish only:
/// the dispatching command, the resolved focus target and its selected home,
/// the newly selected and departing homes per affected output, and the
/// issuing world revision and observation watermark. Dropped unadopted on
/// envelope failure; restored away on command refusal via the pre-command
/// image.
#[derive(Debug, Clone)]
pub(crate) struct PendingSelection {
    pub command_seq: u64,
    pub focus: WindowId,
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

/// Retained `awaitShown` selection binding: the actually emitted effect, the
/// parent view operation identity (`op-<cmdseq>`, for the follow-on switch
/// dependency; recorded from the dispatching command, never a new wire id),
/// the focus selected home, the issuing revision and observation, the frozen
/// arrival snapshot, the first exact-tuple correlated feedback row (if any),
/// and the two distinct readiness facts. Bounded to the latest selection
/// per focus output; observation-only revisions keep the binding. The
/// minimal active selected/departing home scope lives in
/// [`SelectionScope`] per output instead (late admissions register
/// against scopes, never the frozen set).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RetainedSelection {
    pub effect: OneShotEffect,
    pub parent_op: String,
    pub home: DomainKey,
    pub rev: u64,
    pub obs: u64,
    pub members: Vec<FrozenArrivalMember>,
    pub feedback: Option<FocusFeedback>,
    pub foreground_proven: bool,
    pub arrival_ready: bool,
}

/// Live outputs for selection retention: reported live screens plus held
/// (narrowly unreadable yet still known) screens. A held screen is never
/// true output loss: scopes, targets, and finite grants survive under it
/// with held authority until true loss or supersession. Recovery never
/// renews an unchanged target; legacy rehoming can change its home/polarity
/// and therefore require a fresh authorization.
fn selection_known_outputs(report: &WorldReport) -> BTreeSet<OutputId> {
    report
        .screens
        .iter()
        .map(|screen| screen.output.clone())
        .chain(report.held_screens.iter().map(|held| held.output.clone()))
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
    /// authority. Unknown views (no ring), held selected scopes, and
    /// incomplete inventories never prove a change. A pending new select
    /// applies after this reconcile, so `retain_view`'s own prune still
    /// owns the superseding intent.
    pub(crate) fn reconcile_selection_scopes(
        &mut self,
        report: &WorldReport,
        held_scopes: &BTreeSet<DomainKey>,
    ) {
        if self.selection_scopes.is_empty() || !report.inventory_complete {
            return;
        }
        let mut stale: Vec<OutputId> = Vec::new();
        for (output, scope) in &self.selection_scopes {
            if scope.selected.iter().any(|home| held_scopes.contains(home)) {
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
                stale.push(output.clone());
            }
        }
        for output in stale {
            self.selection_scopes.remove(&output);
            self.selection_targets
                .retain(|_, entry| entry.home.output != output);
            self.retained_selection.remove(&output);
        }
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
        report: &WorldReport,
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
                && report.inventory_complete
                && !report.gesture_barrier
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
            } else if report.gesture_barrier {
                desire.visibility_authority = ScopeAuthority::Hold(HoldReason::GestureBarrier);
            } else if !report.inventory_complete {
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
    /// carrier.
    pub(crate) fn freeze_selection_arrival(
        &mut self,
        effects: &[OneShotEffect],
        windows: &[WindowDesire],
    ) {
        let Some(pending) = self.pending_selection.take() else {
            return;
        };
        let Some(effect) = effects
            .iter()
            .find(|effect| {
                effect.kind == EffectKind::Focus
                    && effect.await_shown
                    && effect.cause == crate::world_report::EffectCause::ExplicitCommand
                    && effect.command_seq == Some(pending.command_seq)
                    && effect.target == pending.focus
            })
            .cloned()
        else {
            return;
        };
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
        let mut members: Vec<FrozenArrivalMember> = Vec::new();
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
        members.sort_by(|a, b| a.window.cmp(&b.window));
        self.retained_selection.insert(
            pending.focus_home.output.clone(),
            RetainedSelection {
                effect,
                parent_op: format!("op-{}", pending.command_seq),
                home: pending.focus_home,
                rev: pending.rev,
                obs: pending.obs,
                members,
                feedback: None,
                foreground_proven: false,
                arrival_ready: false,
            },
        );
    }

    /// Consume accepted-report focus feedback against the retained
    /// selection bindings, before the command-only rollback boundary and
    /// independent of sweep readability holds. First exact opaque
    /// `(id, target, issuing revision)` match wins per binding, within one
    /// report and across later ones; raw feedback is preserved and later
    /// contradictory rows never upgrade the original attempt. Fresh facts
    /// never upgrade the first row.
    pub(crate) fn settle_selection_feedback(&mut self, report: &WorldReport) {
        let homing: BTreeMap<WindowId, DomainKey> = self
            .staged
            .homing
            .iter()
            .map(|(window, home)| (window.clone(), home.clone()))
            .collect();
        let report_obs = report.observation_seq;
        for binding in self.retained_selection.values_mut() {
            if binding.feedback.is_some() {
                continue;
            }
            for row in &report.focus_feedback {
                if row.id == binding.effect.id
                    && row.target == binding.effect.target
                    && row.revision == binding.effect.issued_revision
                {
                    binding.foreground_proven = classify_focus_feedback(&binding.effect, row)
                        == crate::world_focus::FocusVerdict::Proven;
                    binding.arrival_ready = prove_selection_arrival(
                        &binding.members,
                        row,
                        &homing,
                        binding.obs,
                        report_obs,
                    );
                    binding.feedback = Some(row.clone());
                    break;
                }
            }
        }
    }

    /// Prune retained selection bindings on explicit close or proven
    /// complete departure of the focus target or any frozen member, or on
    /// true output loss (grounded complete inventory with neither a live
    /// nor a held screen for the output, outside hotplug displacement).
    /// A held screen preserves the grant under held authority; incomplete
    /// inventories and held facts are not closure. Member closes drop
    /// only the finite grant: surviving output scopes and per-window
    /// targets persist independently. Pruning drops the binding without
    /// claiming readiness.
    pub(crate) fn prune_selection_grants(&mut self, report: &WorldReport) {
        if !report.closed.is_empty() {
            let closed: BTreeSet<WindowId> = report.closed.iter().cloned().collect();
            self.retained_selection.retain(|_, binding| {
                if closed.contains(&binding.effect.target) {
                    return false;
                }
                !binding
                    .members
                    .iter()
                    .any(|member| closed.contains(&member.window))
            });
        }
        if report.inventory_complete {
            let live = selection_known_outputs(report);
            self.retained_selection.retain(|output, _| {
                live.contains(output) || self.displaced.contains_key(&output.0)
            });
        }
        if !report.inventory_complete {
            return;
        }
        let live: BTreeSet<&WindowId> = report.windows.iter().map(|fact| &fact.window).collect();
        let held: BTreeSet<&WindowId> = report
            .held_windows
            .iter()
            .map(|held| &held.window)
            .collect();
        let excluded = self.staged.excluded.clone();
        let known: BTreeSet<WindowId> = self
            .retained_selection
            .values()
            .flat_map(|binding| {
                std::iter::once(&binding.effect.target)
                    .chain(binding.members.iter().map(|member| &member.window))
            })
            .filter(|target| {
                self.members.contains_key(*target) || self.engine_knows_window_any(target)
            })
            .cloned()
            .collect();
        self.retained_selection.retain(|_, binding| {
            std::iter::once(&binding.effect.target)
                .chain(binding.members.iter().map(|member| &member.window))
                .all(|target| {
                    live.contains(target)
                        || held.contains(target)
                        || excluded.contains(target)
                        || known.contains(target)
                })
        });
    }

    /// Prune active selection scopes on true output loss only (complete
    /// inventory with neither a live nor a held screen for the output,
    /// outside hotplug displacement). Held screens and incomplete
    /// inventories preserve every scope; closes and superseding views
    /// never clear scopes here (closes drop only the finite grant, views
    /// reconcile centrally before publication).
    pub(crate) fn prune_selection_scopes(&mut self, report: &WorldReport) {
        if !report.inventory_complete {
            return;
        }
        let live = selection_known_outputs(report);
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
    pub(crate) fn prune_selection_targets(&mut self, report: &WorldReport) {
        if !report.closed.is_empty() {
            let closed: BTreeSet<WindowId> = report.closed.iter().cloned().collect();
            self.selection_targets
                .retain(|window, _| !closed.contains(window));
        }
        let lost: BTreeSet<OutputId> = if report.inventory_complete {
            let live = selection_known_outputs(report);
            self.selection_targets
                .values()
                .map(|entry| entry.home.output.clone())
                .filter(|output| !live.contains(output) && !self.displaced.contains_key(&output.0))
                .collect()
        } else {
            BTreeSet::new()
        };
        if !lost.is_empty() {
            self.selection_targets
                .retain(|_, entry| !lost.contains(&entry.home.output));
        }
        if report.inventory_complete {
            let live: BTreeSet<&WindowId> =
                report.windows.iter().map(|fact| &fact.window).collect();
            let held: BTreeSet<&WindowId> = report
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

/// Arrival proof over one frozen snapshot against one feedback row: every
/// frozen required shown member needs a coherent Output-arm bracket
/// matching its retained logical home and output plus its frozen frame.
/// Both reads must show a matched lifetime with visible, non-minimized,
/// non-hidden state; the bracket watermark must equal its own before/after
/// watermarks (never the later report watermark), must not predate the
/// frozen issuing observation, and must not postdate the feedback report
/// watermark. A null desired frame waives equality only, never the need
/// for a readable live frame. Unknown facts, wrong homes/outputs/frames,
/// mismatched watermarks, lifetime tags, and reused lifetimes never prove
/// readiness.
fn prove_selection_arrival(
    members: &[FrozenArrivalMember],
    row: &FocusFeedback,
    homing: &BTreeMap<WindowId, DomainKey>,
    issuing_obs: u64,
    report_obs: u64,
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
            let crate::world_report::ArrivalLocation::Output { out } = &bracket.location else {
                return false;
            };
            if out.as_ref() != Some(&member.home.output) {
                return false;
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

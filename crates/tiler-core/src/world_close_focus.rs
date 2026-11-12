//! Core-owned retained focus request: one slot for every cause.
//!
//! On a qualifying focus decision (explicit command, automatic admission,
//! focus-follows-cursor hover, or the positive close of the focused
//! lifetime) the core freezes the winning target plus its activation scope
//! once per accepted publication. The frozen request is retained state (not
//! a one-shot effect): every later publication re-emits the identical
//! `(id, target, revision)` until target equality satisfies it, a newer
//! focus decision of any cause supersedes it, an observed user focus change
//! supersedes it, a view/output change or target loss makes the target
//! ineligible, unknown foreground retires it, or a correlated refusal
//! retires it. Re-emission never marks the request attempted in core.
//! Non-focus commands (including refused ones, in any batch) never cancel
//! the pending request.

use std::collections::BTreeMap;

use crate::directional::{OutputId, WindowId, WorkspaceId};
use crate::session::DomainKey;
use crate::world_engine::WorldEngine;
use crate::world_focus::{FocusVerdict, classify_focus_feedback};
use crate::world_report::{
    AfterShown, FocusFeedback, FocusRequest, FocusRequestCause, VisibilityReason, VisibilitySetter,
    WorldReportV4,
};

/// One retained focus request: the frozen target of the winning focus
/// decision plus its frozen activation scope. Zero or one row per engine,
/// never a chain, never retried after retirement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PendingFocus {
    pub(crate) id: String,
    pub(crate) target: WindowId,
    pub(crate) output: OutputId,
    pub(crate) workspace: String,
    pub(crate) issued_revision: u64,
    pub(crate) cause: FocusRequestCause,
    pub(crate) cmd_seq: Option<u64>,
    pub(crate) after_shown: Option<AfterShown>,
    /// Issuing report's truthful observed focus (possibly empty).
    pub(crate) baseline_focused: String,
    /// Shown-view basis at mint: any later view/output change supersedes.
    pub(crate) view_basis: BTreeMap<String, String>,
    /// First exact-tuple correlated feedback row, if any.
    pub(crate) feedback: Option<FocusFeedback>,
}

impl PendingFocus {
    pub(crate) fn as_request(&self) -> FocusRequest {
        FocusRequest {
            id: self.id.clone(),
            target: self.target.clone(),
            issued_revision: self.issued_revision,
            cause: self.cause,
            output: self.output.clone(),
            workspace: self.workspace.clone(),
            cmd_seq: self.cmd_seq,
            after_shown: self.after_shown.clone(),
        }
    }
}

/// Whether a first-correlated no-call row is a terminal disposition rather
/// than unproven silence. Bare (`Empty`) and explicitly unproven
/// (`FocusUnproven`) rows retain the request; every informative mechanical
/// reason spends the id, so re-emission could never actuate again.
fn terminal_no_call(reason: VisibilityReason) -> bool {
    !matches!(
        reason,
        VisibilityReason::Empty | VisibilityReason::FocusUnproven
    )
}

/// Whether one first-correlated focus feedback row retires the request:
/// a proven refusal under the existing ordinary-focus rule, or a terminal
/// safety no-call. Mere disagreement after accepted dispatch stays unproven
/// and retains, exactly like ordinary focus grants.
fn focus_feedback_retires(pending: &PendingFocus, row: &FocusFeedback) -> bool {
    if classify_focus_feedback(&pending.target, row) == FocusVerdict::Refused {
        return true;
    }
    matches!(
        row.setter,
        VisibilitySetter::NotCalled | VisibilitySetter::Threw
    ) && terminal_no_call(row.reason)
}

impl WorldEngine {
    /// Execution-time subject for the next implicit command: the single
    /// logical cursor when a logical focus decision owns it. When the
    /// cursor is empty and no retained focus request owns the subject, a
    /// no-target command acts on the host-reported foreground, provided
    /// it is a live bound window of the current generation that core
    /// knows: present as a staged fact, homed this capture, neither
    /// closed nor excluded. The fallback never overrides or outlives a
    /// logical focus decision or a pending retained request, changes
    /// neither canonical focus memory nor session eligibility, mints no
    /// focus request, and never weakens the automatic guards (downstream
    /// dispatch still refuses hidden/held/in-flight subjects on its own
    /// terms). An unknown, unhomed, or stale foreground still resolves
    /// to no subject, so execution refuses it as focus-unknown exactly
    /// as before. Read-only: no state is written here.
    pub(crate) fn execution_subject(&self) -> Option<WindowId> {
        if let Some(cursor) = self.logical_focus_cursor.clone() {
            return Some(cursor);
        }
        // A live retained request owns the subject even while the cursor
        // slot reads empty: falling back to observed foreground would
        // override (or outlive) the logical focus decision it carries.
        if self.pending_focus.is_some() {
            return None;
        }
        let observed = self.staged.observation.focused_global.clone();
        if observed.is_empty() {
            return None;
        }
        let window = WindowId(observed);
        // Live bound window of the current generation that core knows:
        // the staged capture (already admission-bound to this adapter,
        // generation, and owner) carries the fact, assigns it logical
        // homing, and has not closed or excluded it. Stale, unknown,
        // unbound, held, or excluded foreground stays unresolved.
        if self.staged.closes.contains(&window)
            || self.staged.excluded.contains(&window)
            || !self.staged.facts.contains_key(&window)
            || !self.staged.homing.contains_key(&window)
        {
            return None;
        }
        Some(window)
    }

    /// Reset the execution-time cursor to the currently eligible observed
    /// focus after the pending authority retires, never to the abandoned
    /// target. Eligibility here is liveness only: homed, not closed, not
    /// excluded. Per-workspace history is untouched.
    pub(crate) fn reset_cursor_to_observed(&mut self, observed: &str) {
        let eligible = !observed.is_empty() && {
            let window = WindowId(observed.to_owned());
            !self.staged.closes.contains(&window)
                && !self.staged.excluded.contains(&window)
                && self.staged.homing.contains_key(&window)
        };
        if eligible {
            let window = WindowId(observed.to_owned());
            self.logical_focus_cursor = Some(window);
        } else {
            self.logical_focus_cursor = None;
        }
    }

    /// Report-order variant of [`WorldEngine::reset_cursor_to_observed`]
    /// for [`WorldEngine::retire_pending_focus`], which runs before
    /// convergence builds staged homing: liveness reads the report's own
    /// window rows instead of staged homing. Staged closes/exclusions
    /// are already staged from the same capture.
    pub(crate) fn reset_cursor_to_report_observed(&mut self, report: &WorldReportV4) {
        let observed = report.focused_global.clone();
        let window = WindowId(observed.clone());
        let eligible = !observed.is_empty()
            && report.windows.iter().any(|fact| fact.window == window)
            && !report.closed.contains(&window)
            && !self.staged.excluded.contains(&window);
        if eligible {
            self.logical_focus_cursor = Some(window);
        } else {
            self.logical_focus_cursor = None;
        }
    }

    /// Preserve a live retained target as the cursor across an
    /// unchanged-baseline report: stale observed A must not overwrite
    /// pending B.
    pub(crate) fn preserve_live_pending_cursor(&mut self, observed: &str) {
        let Some(pending) = self.pending_focus.clone() else {
            return;
        };
        if observed != pending.baseline_focused {
            return;
        }
        if self.staged.closes.contains(&pending.target)
            || self.staged.excluded.contains(&pending.target)
            || !self.staged.homing.contains_key(&pending.target)
        {
            return;
        }
        self.logical_focus_cursor = Some(pending.target);
    }
    /// Freeze the positive-close survivor before `converge_logical_pre`
    /// rewrites history: retire the existing row on terminal ingress facts,
    /// then mint at most one row for a newly qualifying focused positive
    /// close. A distinct newly qualifying close supersedes the old slot
    /// with a new id (Q1); observing the same pending close again keeps
    /// the id through [`WorldEngine::note_focus_decision`]. The run-level
    /// winner pass below owns every other cause and the same-report
    /// precedence between close and explicit/admission/hover (Q2): the
    /// close mints here even when the report carries a command, and a
    /// same-report explicit/admission winner supersedes it there. Non-focus
    /// and refused commands produce no focus candidate, so the minted
    /// close survives them unchanged.
    pub(crate) fn freeze_close_focus(&mut self, report: &WorldReportV4, issuing_revision: u64) {
        self.retire_pending_focus(report);
        // Incomplete inventory, bare absence, or duplicate silence never
        // qualifies: positive close evidence only.
        if !report.inventory_complete || report.closed.is_empty() {
            return;
        }
        // Unknown foreground never originates a request: only a proven
        // bound foreground (including known no-client) can anchor one.
        if !report.foreground_known {
            return;
        }
        // Only the positively closed globally focused lifetime originates a
        // request. The origin is the truthful prior global observation from
        // the last accepted report, never local per-domain session focus:
        // floats are not tiled-linked, other outputs sort first, and an
        // empty first domain must not veto a focused close elsewhere. A
        // nonempty prior focus was proven known by report validation.
        let cohort: std::collections::BTreeSet<WindowId> = report.closed.iter().cloned().collect();
        let prior = self.prior_context.prev_focused_global.clone();
        if prior.is_empty() {
            return;
        }
        let focused_window = WindowId(prior);
        if !cohort.contains(&focused_window) {
            return;
        }
        // Ordinary float close uses its retained source home: the tiled MRU
        // survivor still comes from the source domain. Members still carry
        // the pre-reconcile home here; staged homing is post-classification
        // and already dropped the closed lifetime.
        let source = match self.core_homing(&focused_window) {
            Some(source) => source,
            None => return,
        };
        // Background close on a non-current workspace/Space reflows only:
        // no activation, no request. The gate only applies where the core
        // tracks a current view for the output.
        if let Some(current) = self.views.get(&source.output.0)
            && current != &source.workspace.0
        {
            return;
        }
        // Existing tiled MRU/removal fallback, excluding the entire close
        // cohort: newest-first history, then deterministic layout order.
        // Tiled links only, never float/sticky/fullscreen broadening.
        let survivor = self.engine.session(&source).and_then(|session| {
            session
                .view_return_candidates(&source)
                .into_iter()
                .filter_map(|leaf| session.focused_window_for(&leaf, &source))
                .find(|window| !cohort.contains(window))
        });
        let survivor = match survivor {
            Some(survivor) => survivor,
            // Sole close: the shown empty workspace stays unfocused with no
            // member request and no native call.
            None => return,
        };
        // Native fallback already chose the winner: satisfied, no request.
        if report.focused_global == survivor.0 {
            return;
        }
        // A non-target same-report fallback authorizes the correction only
        // when it is itself an eligible same-domain tiled survivor: same
        // retained home, tiled-linked (never float/sticky exception),
        // readable, and free of minimize/hide/fullscreen/exclusion vetoes.
        // Foreign, other-output, float, hidden, or otherwise ineligible
        // fallbacks authorize nothing. A stale fallback still naming the
        // closed cohort carries no new information and does not block.
        if !report.focused_global.is_empty()
            && report.focused_global != survivor.0
            && !cohort.contains(&WindowId(report.focused_global.clone()))
            && !self.close_fallback_eligible(&report.focused_global, &source)
        {
            return;
        }
        // A distinct newly qualifying close supersedes the old slot; the
        // same pending close observed again keeps its id (no repeated
        // correction for the same close).
        if let Some(pending) = self.pending_focus.as_ref()
            && pending.cause == FocusRequestCause::PositiveClose
            && pending.target == survivor
        {
            return;
        }
        self.pending_focus = Some(PendingFocus {
            id: format!("focus-{issuing_revision}"),
            target: survivor.clone(),
            output: source.output.clone(),
            workspace: source.workspace.0.clone(),
            issued_revision: issuing_revision,
            cause: FocusRequestCause::PositiveClose,
            cmd_seq: None,
            after_shown: None,
            baseline_focused: report.focused_global.clone(),
            view_basis: self.views.clone(),
            feedback: None,
        });
        // The accepted close choice advances the execution-time cursor
        // immediately, like every other accepted focus choice.
        self.logical_focus_cursor = Some(survivor);
    }

    /// Whether the same-report non-target fallback is an eligible
    /// same-domain tiled survivor that the bounded correction may supersede.
    /// Reads retained pre-reconcile membership (still carrying the closed
    /// lifetime's home) plus the freshly classified facts: staged homing
    /// and domains are not built until after convergence, so they are
    /// unavailable at freeze time.
    fn close_fallback_eligible(&self, focused: &str, source: &DomainKey) -> bool {
        let fallback = WindowId(focused.to_owned());
        if self.core_homing(&fallback).as_ref() != Some(source) {
            return false;
        }
        let tiled = self.engine.session(source).is_some_and(|session| {
            session.snapshot().windows.iter().any(|link| {
                link.window == fallback
                    && link.output == source.output
                    && link.workspace == source.workspace
            })
        });
        if !tiled {
            return false;
        }
        if self.staged.excluded.contains(&fallback) {
            return false;
        }
        match self.staged.facts.get(&fallback) {
            Some(fact) => {
                fact.minimized != Some(true)
                    && fact.host_hidden != Some(true)
                    && fact.fullscreen != Some(true)
            }
            None => false,
        }
    }

    /// Report-order retirement for the retained row. Target equality
    /// satisfies first; the latched first exact-tuple correlated refusal or
    /// terminal safety no-call retires; target re-close, unknown foreground,
    /// later baseline change (a newer observed focus event), and view-basis
    /// drift retire. Unproven or unobserved attempts retain the identical
    /// row. Non-focus commands (including refused ones, in any batch) never
    /// retire: only a newer focus decision (via
    /// [`WorldEngine::note_focus_decision`]), an observed user focus change,
    /// or a view change or target loss making the target ineligible
    /// supersedes (Q1).
    pub(crate) fn retire_pending_focus(&mut self, report: &WorldReportV4) {
        let Some(mut pending) = self.pending_focus.clone() else {
            return;
        };
        if report.focused_global == pending.target.0 {
            self.pending_focus = None;
            self.logical_focus_cursor = Some(pending.target);
            return;
        }
        // Latch the first exact-tuple correlated row once, within and
        // across reports; later rows for the same id never replace it.
        // The verdict derives from that latched row only.
        if pending.feedback.is_none() {
            for row in &report.focus_feedback {
                if row.id == pending.id
                    && row.target == pending.target
                    && row.revision == pending.issued_revision
                {
                    pending.feedback = Some(row.clone());
                    break;
                }
            }
        }
        if let Some(row) = pending.feedback.clone()
            && focus_feedback_retires(&pending, &row)
        {
            self.pending_focus = None;
            self.reset_cursor_to_report_observed(report);
            return;
        }
        self.pending_focus = Some(pending.clone());
        if report.closed.iter().any(|id| id == &pending.target) {
            self.pending_focus = None;
            self.reset_cursor_to_report_observed(report);
            return;
        }
        // Unknown foreground retires the close lane only (close's approved
        // rule): explicit and admission requests keep their own
        // cause-appropriate settlement and never retire on a blanket
        // unknown-foreground verdict.
        if pending.cause == FocusRequestCause::PositiveClose
            && report.focused_global.is_empty()
            && !report.foreground_known
        {
            self.pending_focus = None;
            self.reset_cursor_to_report_observed(report);
            return;
        }
        if report.focused_global != pending.baseline_focused {
            self.pending_focus = None;
            self.reset_cursor_to_report_observed(report);
            return;
        }
        // View/output drift supersedes: any change in the frozen basis.
        if self.views != pending.view_basis
            && pending
                .view_basis
                .iter()
                .any(|(out, ws)| self.views.get(out).is_none_or(|current| current != ws))
        {
            self.pending_focus = None;
            self.reset_cursor_to_report_observed(report);
        }
    }

    /// Supersede the pending row for a newer focus decision of any cause
    /// (Q1/Q2 winner). A distinct newly qualifying event replaces the old
    /// slot; observing the same pending event again keeps the id. Callers
    /// resolve the new implicit subject from the execution-time cursor
    /// before calling, so supersession never erases the subject.
    pub(crate) fn note_focus_decision(
        &mut self,
        target: &WindowId,
        cause: FocusRequestCause,
        cmd_seq: Option<u64>,
        issuing_revision: u64,
        baseline_focused: &str,
    ) {
        // Hover freshness is decided by the producer: any hover effect that
        // reaches this slot is a fresh qualifying entry (continuous presence
        // over the pending target never mints), so it always remints even
        // for the same target. Other causes keep the id when observing the
        // same pending event again.
        let keep = cause != FocusRequestCause::FocusFollowsCursor
            && self.pending_focus.as_ref().is_some_and(|pending| {
                &pending.target == target && pending.cause == cause && pending.cmd_seq == cmd_seq
            });
        if keep {
            return;
        }
        let home = self
            .staged
            .homing
            .get(target)
            .cloned()
            .or_else(|| self.core_homing(target));
        let Some(home) = home else {
            return;
        };
        self.pending_focus = Some(PendingFocus {
            id: format!("focus-{issuing_revision}"),
            target: target.clone(),
            output: home.output.clone(),
            workspace: home.workspace.0.clone(),
            issued_revision: issuing_revision,
            cause,
            cmd_seq,
            after_shown: None,
            baseline_focused: baseline_focused.to_owned(),
            view_basis: self.views.clone(),
            feedback: None,
        });
    }

    /// Run-level winner pass: partition one-shot focus effects into
    /// candidates, keep the single latest-only winner under Q2 precedence
    /// (explicit-command, then automatic-admission, then the newly minted
    /// positive-close, then focus-follows-cursor), advance the
    /// execution-time cursor for the winner, and supersede the retained
    /// slot for a distinct new decision. Only candidates arising in THIS
    /// report compete: an unchanged old positive-close slot never outranks
    /// a genuinely new hover; with no newer candidate the live retained
    /// request survives unchanged (same id, same issuing revision). Within
    /// one cause the latest candidate in effect order wins. Focus carriers
    /// pass through untouched: the selection freeze below must still see
    /// them, and only the wire publication strips focus (focus never
    /// publishes on the v4 wire).
    pub(crate) fn drain_focus_candidates(
        &mut self,
        effects: Vec<crate::world_report::OneShotEffect>,
        held_scopes: &std::collections::BTreeSet<DomainKey>,
        report: &WorldReportV4,
        issuing_revision: u64,
    ) -> Vec<crate::world_report::OneShotEffect> {
        use crate::world_report::{EffectCause, EffectKind};
        let mut explicit: Vec<crate::world_report::OneShotEffect> = Vec::new();
        let mut admission: Vec<crate::world_report::OneShotEffect> = Vec::new();
        let mut hover: Vec<crate::world_report::OneShotEffect> = Vec::new();
        for effect in &effects {
            if effect.kind != EffectKind::Focus {
                continue;
            }
            match effect.cause {
                EffectCause::ExplicitCommand => explicit.push(effect.clone()),
                EffectCause::AutomaticAdmission => admission.push(effect.clone()),
                EffectCause::FocusFollowsCursor => hover.push(effect.clone()),
            }
        }
        // Only a close minted by THIS report's freeze competes: a pending
        // row whose issuing revision predates this report is retained state,
        // never a same-report candidate.
        let fresh_close: Option<(FocusRequestCause, Option<u64>, WindowId)> =
            self.pending_focus.as_ref().and_then(|pending| {
                if pending.cause == FocusRequestCause::PositiveClose
                    && pending.issued_revision == issuing_revision
                {
                    Some((pending.cause, pending.cmd_seq, pending.target.clone()))
                } else {
                    None
                }
            });
        let winner: Option<(FocusRequestCause, Option<u64>, WindowId)> = explicit
            .last()
            .map(|effect| {
                (
                    FocusRequestCause::ExplicitCommand,
                    effect.command_seq,
                    effect.target.clone(),
                )
            })
            .or_else(|| {
                admission.last().map(|effect| {
                    (
                        FocusRequestCause::AutomaticAdmission,
                        None,
                        effect.target.clone(),
                    )
                })
            })
            .or(fresh_close)
            .or_else(|| {
                hover.last().map(|effect| {
                    (
                        FocusRequestCause::FocusFollowsCursor,
                        None,
                        effect.target.clone(),
                    )
                })
            });
        if let Some((cause, cmd_seq, target)) = winner {
            let _ = self.record_canonical(&target, held_scopes, false);
            self.note_focus_decision(
                &target,
                cause,
                cmd_seq,
                issuing_revision,
                &report.focused_global,
            );
        }
        effects
    }

    /// Copy the frozen `afterShown` projection into the winning explicit
    /// request from its retained selection binding. The primary freeze
    /// happens at the same selection freeze that builds the binding (see
    /// `freeze_one_selection`); this publish-time copy is only a fallback
    /// for bindings frozen before the focus winner was known. It projects
    /// the binding's exact frozen state, never a parallel approximation
    /// from publication rows and never truncated: the binding members are
    /// already exact, so no cap applies here. Only selection-backed
    /// explicit-command requests gain dependencies (a binding whose parent
    /// operation and focus match the winner); direct/directional explicit
    /// focus and every other cause keep `None`. Native-free bindings carry
    /// empty `views` (no native view obligation); host-owned bindings
    /// carry their frozen native view targets. Re-emission never changes
    /// the frozen set: a request that already carries one keeps it, and
    /// parent settlement cannot erase the copy.
    pub(crate) fn freeze_focus_after_shown(&mut self) {
        let Some(pending) = self.pending_focus.clone() else {
            return;
        };
        if pending.cause != FocusRequestCause::ExplicitCommand || pending.after_shown.is_some() {
            return;
        }
        let Some(cmd_seq) = pending.cmd_seq else {
            return;
        };
        let Some(binding) = self
            .retained_selection
            .values()
            .find(|binding| {
                binding.effect.as_ref().is_some_and(|effect| {
                    effect.command_seq == Some(cmd_seq) && effect.target == pending.target
                })
            })
            .cloned()
        else {
            return;
        };
        if let Some(slot) = self.pending_focus.as_mut() {
            slot.after_shown = Some(binding.after_shown_projection(&self.selection_scopes));
        }
    }

    /// Post-convergence eligibility before publication, cause-appropriate:
    /// every cause requires the target to still home in the frozen scope
    /// with an unchanged view basis, while the automatic-focus fences apply
    /// only to the automatic lanes (admission, hover, positive-close).
    /// Explicit command focus keeps its automatic-focus exemptions: awaited
    /// selection holds stay holds under their own rules instead of
    /// retiring the request, and sibling/game/gesture vetoes never cancel
    /// an explicit decision. Independent safety (target loss, exclusion)
    /// still retires every cause. An unsafe frozen winner authorizes no
    /// alternate search or request; unrelated survivor geometry still
    /// reflows independently through the sweep.
    pub(crate) fn validate_pending_focus(&mut self) {
        let Some(pending) = self.pending_focus.clone() else {
            return;
        };
        let source = DomainKey {
            output: pending.output.clone(),
            workspace: WorkspaceId(pending.workspace.clone()),
        };
        // Newcomer admission (and hover/explicit on a fresh lifetime) has
        // no retained member until `reconcile_members` runs after
        // publication: staged homing is the truthful scope at mint. Close
        // keeps its frozen retained-source strictness; other causes accept
        // either staged or retained homing for the frozen scope.
        let core_ok = self.core_homing(&pending.target).as_ref() == Some(&source);
        let staged_ok = self.staged.homing.get(&pending.target) == Some(&source);
        let homing_ok = core_ok || (staged_ok && pending.cause != FocusRequestCause::PositiveClose);
        if !homing_ok {
            self.pending_focus = None;
            let observed = self.staged.observation.focused_global.clone();
            self.reset_cursor_to_observed(&observed);
            return;
        }
        if self.views != pending.view_basis
            && pending
                .view_basis
                .iter()
                .any(|(out, ws)| self.views.get(out).is_none_or(|current| current != ws))
        {
            self.pending_focus = None;
            let observed = self.staged.observation.focused_global.clone();
            self.reset_cursor_to_observed(&observed);
            return;
        }
        if self.staged.excluded.contains(&pending.target) {
            self.pending_focus = None;
            let observed = self.staged.observation.focused_global.clone();
            self.reset_cursor_to_observed(&observed);
            return;
        }
        // Explicit decisions carry their own view/arrival authority through
        // the existing selection dependency, never through this automatic
        // predicate: only target loss, view drift, exclusion, and the
        // report-order settlement above retire them.
        if pending.cause == FocusRequestCause::ExplicitCommand {
            return;
        }
        let readable = self
            .staged
            .domains
            .iter()
            .find(|domain| domain.key == source)
            .is_some_and(|domain| domain.readable);
        if !readable || self.hold_reasons.contains_key(&source) {
            self.pending_focus = None;
            let observed = self.staged.observation.focused_global.clone();
            self.reset_cursor_to_observed(&observed);
            return;
        }
        if let Some(fact) = self.staged.facts.get(&pending.target)
            && (fact.minimized == Some(true)
                || fact.host_hidden == Some(true)
                || fact.fullscreen == Some(true))
        {
            self.pending_focus = None;
            let observed = self.staged.observation.focused_global.clone();
            self.reset_cursor_to_observed(&observed);
            return;
        }
        // Normative automatic target restrictions: never a sticky target,
        // and an ordinary float never carries maximized (fullscreen is
        // already fenced above). The maximized veto rides the float
        // exception only: a maximized tiled target stays eligible, and
        // explicit decisions keep their exemption above.
        if self.derived_for(&pending.target).sticky
            || self
                .staged
                .facts
                .get(&pending.target)
                .is_some_and(|fact| fact.sticky == Some(true))
        {
            self.pending_focus = None;
            let observed = self.staged.observation.focused_global.clone();
            self.reset_cursor_to_observed(&observed);
            return;
        }
        if self.derived_for(&pending.target).floating
            && (self.derived_for(&pending.target).overlay_maximized
                || self
                    .staged
                    .facts
                    .get(&pending.target)
                    .is_some_and(|fact| fact.maximized == Some(true)))
        {
            self.pending_focus = None;
            let observed = self.staged.observation.focused_global.clone();
            self.reset_cursor_to_observed(&observed);
            return;
        }
        // Existing automatic-focus vetoes, never broader candidate search:
        // classified hide/hold/exclusion, the unexplained fit-excluded
        // game/overlay shape (ordinary floats stay eligible), floating
        // output scopes, a fullscreen/maximized sibling owning native
        // activation, the focused-fullscreen-game pause, and the gesture
        // barrier all fence the frozen automatic winner like any automatic
        // focus.
        let derived = self.derived_for(&pending.target);
        if derived.hidden || derived.hold || derived.excluded {
            self.pending_focus = None;
            let observed = self.staged.observation.focused_global.clone();
            self.reset_cursor_to_observed(&observed);
            return;
        }
        if derived.fit_excluded
            && !derived.floating
            && !derived.overlay_fullscreen
            && !derived.overlay_maximized
        {
            self.pending_focus = None;
            let observed = self.staged.observation.focused_global.clone();
            self.reset_cursor_to_observed(&observed);
            return;
        }
        if self.floating_scopes.contains(&source) {
            self.pending_focus = None;
            let observed = self.staged.observation.focused_global.clone();
            self.reset_cursor_to_observed(&observed);
            return;
        }
        let sibling_protected = self.staged.domains.iter().any(|domain| {
            domain.key == source
                && domain.windows.iter().any(|window| {
                    window.window != pending.target && (window.fullscreen || window.maximized)
                })
        });
        if sibling_protected {
            self.pending_focus = None;
            let observed = self.staged.observation.focused_global.clone();
            self.reset_cursor_to_observed(&observed);
            return;
        }
        // Focused float/sticky ownership: no automatic winner activates
        // over a different focused floating/sticky window. Explicit
        // decisions keep their exemption above; positive-close winners
        // can only reach here past the same-report fallback fence and
        // hover winners past the producer gate, so this retained
        // revalidation never authorizes a second mechanism.
        if !self.staged.observation.focused_global.is_empty()
            && self.staged.observation.focused_global != pending.target.0
        {
            let focused = WindowId(self.staged.observation.focused_global.clone());
            let shape = self.derived_for(&focused);
            if shape.floating || shape.sticky {
                self.pending_focus = None;
                let observed = self.staged.observation.focused_global.clone();
                self.reset_cursor_to_observed(&observed);
                return;
            }
        }
        if crate::world_cursor::cursor_game_paused(self) {
            self.pending_focus = None;
            let observed = self.staged.observation.focused_global.clone();
            self.reset_cursor_to_observed(&observed);
            return;
        }
        if self.staged.gesture_barrier {
            self.pending_focus = None;
            let observed = self.staged.observation.focused_global.clone();
            self.reset_cursor_to_observed(&observed);
        }
    }

    /// Current retained focus request for the reply lane, if any.
    pub(crate) fn pending_focus_request(&self) -> Option<FocusRequest> {
        self.pending_focus.as_ref().map(PendingFocus::as_request)
    }
}

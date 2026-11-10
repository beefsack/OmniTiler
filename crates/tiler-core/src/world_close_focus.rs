//! Core-owned positive-close refocus: one retained close-focus request.
//!
//! On positive close of the focused lifetime, the core freezes the pre-close
//! MRU tiled survivor in the source `(output, workspace)` domain before
//! observation convergence can rewrite history from the host fallback. The
//! frozen request is retained state (not a one-shot effect): every later
//! publication re-emits the identical `(id, target, revision)` until target
//! equality satisfies it, a later observed-focus change supersedes it, a
//! command/view/output change supersedes it, unknown foreground retires it,
//! the target becomes ineligible, or a correlated refusal retires it.
//! Re-emission never marks the request attempted in core.

use std::collections::{BTreeMap, BTreeSet};

use crate::directional::{OutputId, WindowId, WorkspaceId};
use crate::session::DomainKey;
use crate::world_engine::WorldEngine;
use crate::world_focus::{FocusVerdict, classify_focus_feedback};
use crate::world_report::{
    CloseFocusCause, CloseFocusRequest, FocusFeedback, VisibilityReason, VisibilitySetter,
    WorldReportV4,
};

/// One retained close-focus request: the frozen target of the positively
/// closed focused lifetime plus its frozen source domain. Zero or one row
/// per engine, never a chain, never retried after retirement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PendingCloseFocus {
    pub(crate) id: String,
    pub(crate) target: WindowId,
    pub(crate) output: OutputId,
    pub(crate) workspace: String,
    pub(crate) issued_revision: u64,
    /// Close report's truthful observed focus (host fallback, possibly empty).
    pub(crate) baseline_focused: String,
    /// Shown-view basis at mint: any later view/output change supersedes.
    pub(crate) view_basis: BTreeMap<String, String>,
    /// First exact-tuple correlated feedback row, if any.
    pub(crate) feedback: Option<FocusFeedback>,
}

impl PendingCloseFocus {
    pub(crate) fn as_request(&self) -> CloseFocusRequest {
        CloseFocusRequest {
            id: self.id.clone(),
            target: self.target.clone(),
            issued_revision: self.issued_revision,
            cause: CloseFocusCause::PositiveClose,
            output: self.output.clone(),
            workspace: self.workspace.clone(),
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

/// Whether one first-correlated close feedback row retires the request:
/// a proven refusal under the existing ordinary-focus rule, or a terminal
/// safety no-call. Mere disagreement after accepted dispatch stays unproven
/// and retains, exactly like ordinary focus grants.
fn close_feedback_retires(pending: &PendingCloseFocus, row: &FocusFeedback) -> bool {
    if classify_focus_feedback(&pending.target, row) == FocusVerdict::Refused {
        return true;
    }
    matches!(
        row.setter,
        VisibilitySetter::NotCalled | VisibilitySetter::Threw
    ) && terminal_no_call(row.reason)
}

impl WorldEngine {
    /// Freeze the close-focus request before `converge_logical_pre` rewrites
    /// history: retire the existing row on terminal ingress facts, then mint
    /// at most one row for a newly qualifying focused positive close.
    pub(crate) fn freeze_close_focus(
        &mut self,
        report: &WorldReportV4,
        issuing_revision: u64,
        consumed: bool,
    ) {
        self.retire_pending_close_focus(report, consumed);
        if self.pending_close_focus.is_some() {
            return;
        }
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
        // A command riding the close report (or an already-consumed
        // sequence) supersedes before any mint: newer input wins.
        if report.command.is_some() || consumed {
            return;
        }
        // Only the positively closed globally focused lifetime originates a
        // request. The origin is the truthful prior global observation from
        // the last accepted report, never local per-domain session focus:
        // floats are not tiled-linked, other outputs sort first, and an
        // empty first domain must not veto a focused close elsewhere. A
        // nonempty prior focus was proven known by report validation.
        let cohort: BTreeSet<WindowId> = report.closed.iter().cloned().collect();
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
        self.pending_close_focus = Some(PendingCloseFocus {
            id: format!("close-focus-{issuing_revision}"),
            target: survivor,
            output: source.output.clone(),
            workspace: source.workspace.0.clone(),
            issued_revision: issuing_revision,
            baseline_focused: report.focused_global.clone(),
            view_basis: self.views.clone(),
            feedback: None,
        });
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
    /// later baseline change, and commands (including consumed
    /// reconciliations) retire. Unproven or unobserved attempts retain the
    /// identical row.
    fn retire_pending_close_focus(&mut self, report: &WorldReportV4, consumed: bool) {
        let Some(mut pending) = self.pending_close_focus.clone() else {
            return;
        };
        if report.focused_global == pending.target.0 {
            self.pending_close_focus = None;
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
            && close_feedback_retires(&pending, &row)
        {
            self.pending_close_focus = None;
            return;
        }
        self.pending_close_focus = Some(pending.clone());
        if report.closed.iter().any(|id| id == &pending.target) {
            self.pending_close_focus = None;
            return;
        }
        if report.focused_global.is_empty() && !report.foreground_known {
            self.pending_close_focus = None;
            return;
        }
        if report.focused_global != pending.baseline_focused {
            self.pending_close_focus = None;
            return;
        }
        if report.command.is_some() || consumed {
            self.pending_close_focus = None;
        }
    }

    /// Post-convergence eligibility before publication: the target must
    /// still home in the frozen source domain on a shown readable scope,
    /// under every existing automatic-focus fence. An unsafe frozen winner
    /// authorizes no alternate search or request; unrelated survivor
    /// geometry still reflows independently through the sweep.
    pub(crate) fn validate_pending_close_focus(&mut self) {
        let Some(pending) = self.pending_close_focus.clone() else {
            return;
        };
        let source = DomainKey {
            output: pending.output.clone(),
            workspace: WorkspaceId(pending.workspace.clone()),
        };
        if self.core_homing(&pending.target).as_ref() != Some(&source) {
            self.pending_close_focus = None;
            return;
        }
        let readable = self
            .staged
            .domains
            .iter()
            .find(|domain| domain.key == source)
            .is_some_and(|domain| domain.readable);
        if !readable || self.hold_reasons.contains_key(&source) {
            self.pending_close_focus = None;
            return;
        }
        if self.staged.excluded.contains(&pending.target) {
            self.pending_close_focus = None;
            return;
        }
        if let Some(fact) = self.staged.facts.get(&pending.target)
            && (fact.minimized == Some(true)
                || fact.host_hidden == Some(true)
                || fact.fullscreen == Some(true))
        {
            self.pending_close_focus = None;
            return;
        }
        // Existing automatic-focus vetoes, never broader candidate search:
        // classified hide/hold/exclusion, the unexplained fit-excluded
        // game/overlay shape (ordinary floats stay eligible), floating
        // output scopes, a fullscreen/maximized sibling owning native
        // activation, the focused-fullscreen-game pause, and the gesture
        // barrier all fence the frozen winner like any automatic focus.
        let derived = self.derived_for(&pending.target);
        if derived.hidden || derived.hold || derived.excluded {
            self.pending_close_focus = None;
            return;
        }
        if derived.fit_excluded
            && !derived.floating
            && !derived.overlay_fullscreen
            && !derived.overlay_maximized
        {
            self.pending_close_focus = None;
            return;
        }
        if self.floating_scopes.contains(&source) {
            self.pending_close_focus = None;
            return;
        }
        let sibling_protected = self.staged.domains.iter().any(|domain| {
            domain.key == source
                && domain.windows.iter().any(|window| {
                    window.window != pending.target && (window.fullscreen || window.maximized)
                })
        });
        if sibling_protected {
            self.pending_close_focus = None;
            return;
        }
        if crate::world_cursor::cursor_game_paused(self) {
            self.pending_close_focus = None;
            return;
        }
        if self.staged.gesture_barrier {
            self.pending_close_focus = None;
            return;
        }
        if self.views != pending.view_basis {
            self.pending_close_focus = None;
        }
    }

    /// Current retained close-focus request for the reply lane, if any.
    pub(crate) fn pending_close_focus_request(&self) -> Option<CloseFocusRequest> {
        self.pending_close_focus
            .as_ref()
            .map(PendingCloseFocus::as_request)
    }
}

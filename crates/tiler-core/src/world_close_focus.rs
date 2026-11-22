//! Core-owned retained focus request: one slot for every cause.
//!
//! On a qualifying focus decision (explicit command, automatic admission,
//! focus-follows-cursor hover, or the positive close of the focused
//! lifetime) the core freezes the winning target plus its activation scope
//! once per accepted publication. The frozen request is retained state (not
//! a one-shot effect): every later publication re-emits the identical
//! `(id, target, revision)` until newer eligible intent (native or core),
//! equality, target loss, or cause-specific safety/refusal settles it.
//! Re-emission never marks the request attempted in core.
//! Non-focus commands (including refused ones, in any batch) never cancel
//! the pending request.

use crate::world_transaction::WorldTransaction;

use crate::directional::{OutputId, WindowId, WorkspaceId};
use crate::session::DomainKey;
use crate::world_focus::{FocusVerdict, classify_focus_feedback};
use crate::world_report::{
    AfterShown, FocusChange, FocusDisposition, FocusFeedback, FocusNoCallReason, FocusRequest,
    FocusRequestCause, FrameSetter, VisibilityReason, WorldReportV4,
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

fn emit_pending_focus_trace(
    report: &WorldReportV4,
    rev: u64,
    pending: &PendingFocus,
    decision: &str,
    reason: &str,
) {
    let cmd_seq = pending
        .cmd_seq
        .map(|seq| seq.to_string())
        .unwrap_or_else(|| "-".to_owned());
    crate::world_engine::emit_focus_trace(format!(
        "omnitiler:core:trace ev=focus-eval gen={} req={} obs={} rev={} id={} target={} issuingRev={} cause={} cmdSeq={} decision={decision} reason={reason}",
        crate::world_engine::diag_id(&report.core_generation),
        crate::world_engine::diag_id(&report.request_id),
        report.observation_seq,
        rev,
        crate::world_engine::diag_id(&pending.id),
        crate::world_engine::diag_id(&pending.target.0),
        pending.issued_revision,
        pending.cause.as_str(),
        cmd_seq,
    ));
}

fn trace_foreground_intent(
    report: &WorldReportV4,
    pending: &PendingFocus,
    event_index: Option<usize>,
    win: Option<&str>,
) {
    use crate::world_engine::{core_trace_enabled, diag_id, emit_focus_trace};
    if !core_trace_enabled() {
        return;
    }
    let evidence = event_index.map_or_else(|| "snapshot".to_owned(), |index| index.to_string());
    let win = match win {
        None => "<unbound>".to_owned(),
        Some("") => "<no-client>".to_owned(),
        Some(win) => diag_id(win).to_string(),
    };
    emit_focus_trace(format!(
        "omnitiler:core:trace ev=focus-foreground-intent gen={} req={} obs={} id={} eventIndex={evidence} win={win}",
        diag_id(&report.core_generation),
        diag_id(&report.request_id),
        report.observation_seq,
        diag_id(&pending.id),
    ));
}

/// Whether a first-correlated no-call row is a terminal disposition rather
/// than unproven silence. Bare (`NoReason`) and explicitly unproven
/// (`ForegroundUnproven`) rows retain the request; every informative mechanical
/// reason spends the id, so re-emission could never actuate again.
fn terminal_focus_no_call(reason: FocusNoCallReason) -> bool {
    !matches!(
        reason,
        FocusNoCallReason::NoReason | FocusNoCallReason::ForegroundUnproven
    )
}

/// Whether a first-correlated called-threw row carries a terminal informative
/// reason rather than unproven silence.
fn terminal_focus_called_reason(reason: VisibilityReason) -> bool {
    !matches!(
        reason,
        VisibilityReason::Empty | VisibilityReason::FocusUnproven
    )
}

/// Whether one first-correlated focus feedback row retires the request:
/// a proven refusal under the existing ordinary-focus rule, or a terminal
/// safety no-call / called-threw with informative reason. Mere disagreement
/// after accepted dispatch stays unproven and retains, exactly like ordinary
/// focus grants.
fn focus_feedback_retires(pending: &PendingFocus, row: &FocusFeedback) -> bool {
    if classify_focus_feedback(&pending.target, row) == FocusVerdict::Refused {
        return true;
    }
    match row.disposition {
        FocusDisposition::NotCalled { reason } => terminal_focus_no_call(reason),
        FocusDisposition::Called {
            setter: FrameSetter::Threw,
            reason,
        } => terminal_focus_called_reason(reason),
        FocusDisposition::Called { .. } => false,
    }
}

enum FocusEvidenceVerdict {
    Arrival,
    Superseded(&'static str),
}

impl WorldTransaction<'_> {
    /// Execution-time subject for the next implicit command: the single
    /// logical cursor when a logical focus decision owns it. When the
    /// cursor is empty and no focus decision from this report or live
    /// available pending request owns the subject, a no-target command acts
    /// on the host-reported foreground, provided it is a live bound window
    /// of the current generation that core knows: present as a staged fact,
    /// homed this capture, neither closed nor excluded. When a retained
    /// focus request is pending on an unavailable target (closed,
    /// excluded, or unhomed/absent from staged facts) and an accepted
    /// report strictly newer than the request still shows another live
    /// bound known window focused, the no-target command acts on that
    /// observed window while keeping the pending request intact; same-batch
    /// commands stay refused. The fallback never overrides or outlives a
    /// logical focus decision or an available pending request, changes
    /// neither canonical focus memory nor session eligibility, mints no
    /// focus request, and never weakens the automatic guards (downstream
    /// dispatch still refuses hidden/held/in-flight subjects on its own
    /// terms). An unknown, unhomed, or stale foreground still resolves to
    /// no subject, so execution refuses it as focus-unknown exactly as
    /// before. Read-only: no state is written here.
    pub(crate) fn execution_subject(&self) -> Option<WindowId> {
        if let Some(cursor) = self.world.focus.logical_focus_cursor.clone() {
            return Some(cursor);
        }
        if self.world.staged.batch_focus_issued {
            return None;
        }
        // A retained request owns the subject while the cursor slot reads
        // empty, except on a strictly newer accepted report whose pending
        // target is unavailable (where observed live bound foreground acts
        // without canceling or overriding the retained request).
        if let Some(pending) = self.world.focus.pending_focus.as_ref() {
            let pending_unavailable = !self.available_focus_subject(&pending.target);
            let newer_report = self.world.staged.revision > pending.issued_revision;
            if !(pending_unavailable && newer_report) {
                return None;
            }
        }
        self.observed_focus_subject()
    }

    /// Resolve only a live bound observation; no fallback to logical history.
    fn observed_focus_subject(&self) -> Option<WindowId> {
        let observed = &self.world.staged.observation.focused_global;
        if observed.is_empty() {
            return None;
        }
        let window = WindowId(observed.clone());
        self.available_focus_subject(&window).then_some(window)
    }

    fn available_focus_subject(&self, window: &WindowId) -> bool {
        self.world.staged.homing.contains_key(window)
            && self.world.staged.facts.contains_key(window)
            && !self.world.staged.closes.contains(window)
            && !self.world.staged.excluded.contains(window)
    }

    /// Seed the FIFO subject after incorporation. Native foreground owns
    /// the cursor unless an unsettled, available request owns it. Clearing
    /// an unavailable target preserves the narrow execution-subject exception.
    pub(crate) fn seed_focus_cursor(&mut self) {
        if let Some(pending) = self.world.focus.pending_focus.as_ref() {
            self.world.focus.logical_focus_cursor = self
                .available_focus_subject(&pending.target)
                .then(|| pending.target.clone());
        } else {
            self.world.focus.logical_focus_cursor = self.observed_focus_subject();
        }
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
    pub(crate) fn freeze_close_focus(
        &mut self,
        report: &WorldReportV4,
        issuing_revision: u64,
        predecessor: Option<u64>,
    ) {
        self.retire_pending_focus(report, predecessor);
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
        let prior = self.world.prior_context.prev_focused_global.clone();
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
        if let Some(current) = self.world.workspace.views.get(&source.output.0)
            && current != &source.workspace.0
        {
            return;
        }
        // Existing tiled MRU/removal fallback, excluding the entire close
        // cohort: newest-first history, then deterministic layout order.
        // Tiled links only, never float/sticky/fullscreen broadening.
        let survivor = self
            .world
            .layout
            .view_return_candidates(&source)
            .into_iter()
            .filter_map(|leaf| self.world.layout.window_for_leaf(&source, &leaf))
            .find(|window| !cohort.contains(window));
        let survivor = match survivor {
            Some(survivor) => survivor,
            // Sole close: the shown empty workspace stays unfocused with no
            // member request and no native call.
            None => return,
        };
        // Native fallback already chose the winner: satisfied, no request.
        if report.focused_global == survivor.0 {
            if crate::world_engine::core_trace_enabled() {
                crate::world_engine::emit_focus_trace(format!(
                    "omnitiler:core:trace ev=focus-eval gen={} req={} obs={} rev={} id=- target={} issuingRev=- cause=positive-close cmdSeq=- decision=equality reason=already-target request=not-created",
                    crate::world_engine::diag_id(&report.core_generation),
                    crate::world_engine::diag_id(&report.request_id),
                    report.observation_seq,
                    issuing_revision,
                    crate::world_engine::diag_id(&survivor.0),
                ));
            }
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
        if let Some(pending) = self.world.focus.pending_focus.as_ref()
            && pending.cause == FocusRequestCause::PositiveClose
            && pending.target == survivor
        {
            return;
        }
        self.world.focus.pending_focus = Some(PendingFocus {
            id: format!("focus-{issuing_revision}"),
            target: survivor.clone(),
            output: source.output.clone(),
            workspace: source.workspace.0.clone(),
            issued_revision: issuing_revision,
            cause: FocusRequestCause::PositiveClose,
            cmd_seq: None,
            after_shown: None,
            feedback: None,
        });
        // The accepted close choice advances the execution-time cursor
        // immediately, like every other accepted focus choice.
        self.world.focus.logical_focus_cursor = Some(survivor);
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
        let tiled = self.world.layout.is_tiled_window(source, &fallback);
        if !tiled {
            return false;
        }
        if self.world.staged.excluded.contains(&fallback) {
            return false;
        }
        match self.world.staged.facts.get(&fallback) {
            Some(fact) => {
                fact.minimized != Some(true)
                    && fact.host_hidden != Some(true)
                    && fact.fullscreen != Some(true)
            }
            None => false,
        }
    }

    /// Use the retained selected view, never the echo-first effective view:
    /// native may still show the departing workspace of an accepted switch.
    fn activation_supersedes(&self, win: Option<&WindowId>, pending: &PendingFocus) -> bool {
        let Some(win) = win.filter(|win| !win.0.is_empty()) else {
            return true;
        };
        // A final snapshot's absence or visibility cannot disprove an
        // activation earlier in the interval. Use the retained lifetime's
        // home; a selected mover uses its frozen destination instead.
        if self.world.staged.excluded.contains(win) {
            return false;
        }
        let frozen_home = pending
            .after_shown
            .as_ref()
            .and_then(|shown| shown.members.iter().find(|member| &member.win == win));
        let retained = if frozen_home.is_none() {
            self.core_homing(win)
        } else {
            None
        };
        let fact = self.world.staged.facts.get(win);
        let (out, workspace) = if let Some(member) = frozen_home {
            (&member.out, Some(member.ws.as_str()))
        } else if let Some(home) = retained.as_ref() {
            (&home.output, Some(home.workspace.0.as_str()))
        } else if let Some(out) = fact.and_then(|fact| fact.output.as_ref()) {
            // Newly bound clients have no retained home yet.
            (out, None)
        } else {
            // The lifetime can have opened and gone away inside this
            // interval. No known home is not proof it was ineligible.
            return true;
        };
        let Some(desired) = self.world.workspace.views.get(&out.0) else {
            return true;
        };
        if self.derived_for(win).sticky {
            return true;
        }
        if let Some(workspace) = workspace {
            return workspace == desired.as_str();
        }
        if !self.connection.host_owned_workspace_domains() {
            return true;
        }
        fact.and_then(|fact| fact.native_workspace.as_deref())
            .zip(self.native_binding_for(&out.0, desired))
            .is_none_or(|(native, selected)| native == selected)
    }

    fn selector_hides_target(&self, pending: &PendingFocus, out: &OutputId, native: &str) -> bool {
        if out != &pending.output || self.derived_for(&pending.target).sticky {
            return false;
        }
        let desired = pending
            .after_shown
            .as_ref()
            .and_then(|shown| shown.views.iter().find(|scope| &scope.out == out))
            .map_or(pending.workspace.as_str(), |scope| scope.ws.as_str());
        self.native_binding_for(&out.0, desired)
            .is_some_and(|expected| expected != native)
    }

    /// First decisive event wins: target arrival settles without giving a
    /// later activation a chance to reclassify the request.
    fn adjudicate_focus_changes(
        &self,
        report: &WorldReportV4,
        pending: &PendingFocus,
        predecessor: Option<u64>,
    ) -> Option<FocusEvidenceVerdict> {
        if let Some(changes) = &report.focus_changes {
            let Some(events) = changes
                .events
                .as_ref()
                .filter(|_| changes.after == predecessor)
            else {
                return Some(FocusEvidenceVerdict::Superseded("focus-evidence-gap"));
            };
            return events
                .iter()
                .enumerate()
                .find_map(|(index, event)| match event {
                    FocusChange::Foreground { win: Some(win) } if win == &pending.target => {
                        Some(FocusEvidenceVerdict::Arrival)
                    }
                    FocusChange::Foreground { win }
                        if self.activation_supersedes(win.as_ref(), pending) =>
                    {
                        trace_foreground_intent(
                            report,
                            pending,
                            Some(index),
                            win.as_ref().map(|id| id.0.as_str()),
                        );
                        Some(FocusEvidenceVerdict::Superseded("foreground-intent"))
                    }
                    FocusChange::View { out, native }
                        if self.selector_hides_target(pending, out, native) =>
                    {
                        Some(FocusEvidenceVerdict::Superseded("selector-intent"))
                    }
                    _ => None,
                });
        }

        // Snapshot-only cannot attribute a changed foreground to an own
        // setter. Compare to the previous incorporated observation, not the
        // mint baseline: an already adjudicated source echo is not new.
        if report.focused_global != pending.target.0
            && report.focused_global != self.world.prior_context.prev_focused_global
            && self.activation_supersedes(Some(&WindowId(report.focused_global.clone())), pending)
        {
            trace_foreground_intent(report, pending, None, Some(&report.focused_global));
            return Some(FocusEvidenceVerdict::Superseded("foreground-intent"));
        }
        // A positive selector change needs both a previous readable selector
        // and a newly observed one. An unchanged source echo is not intent.
        if !self.native_list_complete(&pending.output.0) {
            return None;
        }
        let prior = self.world.settlement.last_views.get(&pending.output)?;
        let prior = self.native_binding_for(&pending.output.0, prior)?;
        let current = self
            .world
            .staged
            .observation
            .native
            .iter()
            .find(|row| row.output == pending.output)?
            .current_native
            .as_deref()?;
        (prior != current && self.selector_hides_target(pending, &pending.output, current))
            .then_some(FocusEvidenceVerdict::Superseded("selector-intent"))
    }

    /// Settle only the older request. Newer intent wins before equality or
    /// correlated feedback: a final own arrival cannot erase earlier intent.
    /// Positive close correction is a separate decision made below this gate.
    fn retire_pending_focus(&mut self, report: &WorldReportV4, predecessor: Option<u64>) {
        let Some(mut pending) = self.world.focus.pending_focus.take() else {
            return;
        };
        let (decision, reason) = if self.world.staged.closes.contains(&pending.target) {
            ("no-call", "target-closed")
        } else if self.world.staged.excluded.contains(&pending.target) {
            ("no-call", "target-excluded")
        } else if pending.cause == FocusRequestCause::PositiveClose && !report.foreground_known {
            ("no-call", "focus-unproven")
        } else if let Some(verdict) = self.adjudicate_focus_changes(report, &pending, predecessor) {
            match verdict {
                FocusEvidenceVerdict::Arrival => ("equality", "already-target"),
                FocusEvidenceVerdict::Superseded(reason) => ("no-call", reason),
            }
        } else if report.focused_global == pending.target.0 {
            ("equality", "already-target")
        } else {
            // First exact-tuple feedback wins across and within reports.
            if pending.feedback.is_none() {
                pending.feedback = report
                    .focus_feedback
                    .iter()
                    .find(|row| {
                        row.id == pending.id
                            && row.target == pending.target
                            && row.revision == pending.issued_revision
                    })
                    .cloned();
            }
            if let Some(row) = pending
                .feedback
                .as_ref()
                .filter(|row| focus_feedback_retires(&pending, row))
            {
                let (decision, reason) = match row.disposition {
                    FocusDisposition::NotCalled { reason } => ("no-call", reason.as_str()),
                    FocusDisposition::Called { setter, reason } => (
                        "called",
                        if reason == VisibilityReason::Empty {
                            setter.as_str()
                        } else {
                            reason.as_str()
                        },
                    ),
                };
                (decision, reason)
            } else {
                self.world.focus.pending_focus = Some(pending);
                return;
            }
        };
        if crate::world_engine::core_trace_enabled() {
            emit_pending_focus_trace(
                report,
                self.world.staged.revision,
                &pending,
                decision,
                reason,
            );
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
    ) {
        // Hover freshness is decided by the producer: any hover effect that
        // reaches this slot is a fresh qualifying entry (continuous presence
        // over the pending target never mints), so it always remints even
        // for the same target. Other causes keep the id when observing the
        // same pending event again.
        let keep = cause != FocusRequestCause::FocusFollowsCursor
            && self
                .world
                .focus
                .pending_focus
                .as_ref()
                .is_some_and(|pending| {
                    &pending.target == target
                        && pending.cause == cause
                        && pending.cmd_seq == cmd_seq
                });
        if keep {
            return;
        }
        let home = self
            .world
            .staged
            .homing
            .get(target)
            .cloned()
            .or_else(|| self.core_homing(target));
        let Some(home) = home else {
            return;
        };
        self.world.focus.pending_focus = Some(PendingFocus {
            id: format!("focus-{issuing_revision}"),
            target: target.clone(),
            output: home.output.clone(),
            workspace: home.workspace.0.clone(),
            issued_revision: issuing_revision,
            cause,
            cmd_seq,
            after_shown: None,
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
            self.world.focus.pending_focus.as_ref().and_then(|pending| {
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
            self.note_focus_decision(&target, cause, cmd_seq, issuing_revision);
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
        let Some(pending) = self.world.focus.pending_focus.clone() else {
            return;
        };
        if pending.cause != FocusRequestCause::ExplicitCommand || pending.after_shown.is_some() {
            return;
        }
        let Some(cmd_seq) = pending.cmd_seq else {
            return;
        };
        let Some(binding) = self
            .world
            .settlement
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
        if let Some(slot) = self.world.focus.pending_focus.as_mut() {
            slot.after_shown =
                Some(binding.after_shown_projection(&self.world.settlement.selection_scopes));
            // Pub-time fallback for a slot minted only after its binding
            // froze: the activation scope follows the target member's
            // frozen post-layout home as the projection does, never the
            // aggregate selected home in `binding.home` nor the staged
            // home read at mint. If no member row exists, preserve the
            // slot's prior home.
            let target_home = binding
                .members
                .iter()
                .find(|member| member.window == pending.target)
                .map(|member| member.home.clone());
            if let Some(home) = target_home {
                slot.output = home.output.clone();
                slot.workspace = home.workspace.0.clone();
            }
        }
    }

    /// Post-convergence eligibility before publication, cause-appropriate:
    /// every cause requires the target to still home in the frozen scope,
    /// while the automatic-focus fences apply only to the automatic lanes
    /// (admission, hover, positive-close).
    /// Explicit command focus keeps its automatic-focus exemptions: awaited
    /// selection holds stay holds under their own rules instead of
    /// retiring the request, and sibling/game/gesture vetoes never cancel
    /// an explicit decision. Independent safety (target loss, exclusion)
    /// still retires every cause. An unsafe frozen winner authorizes no
    /// alternate search or request; unrelated survivor geometry still
    /// reflows independently through the sweep.
    pub(crate) fn validate_pending_focus(&mut self) {
        let Some(pending) = self.world.focus.pending_focus.clone() else {
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
        let staged_ok = self.world.staged.homing.get(&pending.target) == Some(&source);
        let homing_ok = core_ok || (staged_ok && pending.cause != FocusRequestCause::PositiveClose);
        if !homing_ok {
            self.world.focus.pending_focus = None;
            self.world.focus.logical_focus_cursor = self.observed_focus_subject();
            return;
        }
        if self.world.staged.excluded.contains(&pending.target) {
            self.world.focus.pending_focus = None;
            self.world.focus.logical_focus_cursor = self.observed_focus_subject();
            return;
        }
        // Explicit decisions carry their own view/arrival authority through
        // the existing selection dependency, never through this automatic
        // predicate: only target loss, exclusion, and the report-order
        // settlement above retire them.
        if pending.cause == FocusRequestCause::ExplicitCommand {
            return;
        }
        let readable = self
            .world
            .staged
            .domains
            .iter()
            .find(|domain| domain.key == source)
            .is_some_and(|domain| domain.readable);
        if !readable || self.world.hold_reasons.contains_key(&source) {
            self.world.focus.pending_focus = None;
            self.world.focus.logical_focus_cursor = self.observed_focus_subject();
            return;
        }
        if let Some(fact) = self.world.staged.facts.get(&pending.target)
            && (fact.minimized == Some(true)
                || fact.host_hidden == Some(true)
                || fact.fullscreen == Some(true))
        {
            self.world.focus.pending_focus = None;
            self.world.focus.logical_focus_cursor = self.observed_focus_subject();
            return;
        }
        // Normative automatic target restrictions: never a sticky target,
        // and an ordinary float never carries maximized (fullscreen is
        // already fenced above). The maximized veto rides the float
        // exception only: a maximized tiled target stays eligible, and
        // explicit decisions keep their exemption above.
        if self.derived_for(&pending.target).sticky
            || self
                .world
                .staged
                .facts
                .get(&pending.target)
                .is_some_and(|fact| fact.sticky == Some(true))
        {
            self.world.focus.pending_focus = None;
            self.world.focus.logical_focus_cursor = self.observed_focus_subject();
            return;
        }
        if self.derived_for(&pending.target).floating
            && (self.derived_for(&pending.target).overlay_maximized
                || self
                    .world
                    .staged
                    .facts
                    .get(&pending.target)
                    .is_some_and(|fact| fact.maximized == Some(true)))
        {
            self.world.focus.pending_focus = None;
            self.world.focus.logical_focus_cursor = self.observed_focus_subject();
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
            self.world.focus.pending_focus = None;
            self.world.focus.logical_focus_cursor = self.observed_focus_subject();
            return;
        }
        if derived.fit_excluded
            && !derived.floating
            && !derived.overlay_fullscreen
            && !derived.overlay_maximized
        {
            self.world.focus.pending_focus = None;
            self.world.focus.logical_focus_cursor = self.observed_focus_subject();
            return;
        }
        if self.world.floating_scopes.contains(&source) {
            self.world.focus.pending_focus = None;
            self.world.focus.logical_focus_cursor = self.observed_focus_subject();
            return;
        }
        let sibling_protected = self.world.staged.domains.iter().any(|domain| {
            domain.key == source
                && domain.windows.iter().any(|window| {
                    window.window != pending.target && (window.fullscreen || window.maximized)
                })
        });
        if sibling_protected {
            self.world.focus.pending_focus = None;
            self.world.focus.logical_focus_cursor = self.observed_focus_subject();
            return;
        }
        // Focused float/sticky ownership: no automatic winner activates
        // over a different focused floating/sticky window. Explicit
        // decisions keep their exemption above; positive-close winners
        // can only reach here past the same-report fallback fence and
        // hover winners past the producer gate, so this retained
        // revalidation never authorizes a second mechanism.
        if !self.world.staged.observation.focused_global.is_empty()
            && self.world.staged.observation.focused_global != pending.target.0
        {
            let focused = WindowId(self.world.staged.observation.focused_global.clone());
            let shape = self.derived_for(&focused);
            if shape.floating || shape.sticky {
                self.world.focus.pending_focus = None;
                self.world.focus.logical_focus_cursor = self.observed_focus_subject();
                return;
            }
        }
        if crate::world_cursor::cursor_game_paused(self) {
            self.world.focus.pending_focus = None;
            self.world.focus.logical_focus_cursor = self.observed_focus_subject();
            return;
        }
        if self.world.staged.gesture_barrier {
            self.world.focus.pending_focus = None;
            self.world.focus.logical_focus_cursor = self.observed_focus_subject();
        }
    }

    /// Current retained focus request for the reply lane, if any.
    pub(crate) fn pending_focus_request(&self) -> Option<FocusRequest> {
        self.world
            .focus
            .pending_focus
            .as_ref()
            .map(PendingFocus::as_request)
    }
}

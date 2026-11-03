//! Ordinary focus settlement: historical verdicts for actually published
//! `awaitShown=false` one-shot focus effects.
//!
//! The latest emitted focus batch only (currently at most admission plus
//! explicit command; focus-follows-cursor never coexists with another focus
//! effect) is retained at publish time. A newer emitted batch supersedes the
//! older one; observation-only reports keep the batch. Accepted-report
//! feedback rows bind by the full opaque tuple
//! `(id, target, issuing revision)` and latch once, including no-call and
//! unproven rows; unknown, mismatched, or forged tuples mint nothing. Later
//! contradictory, superseded, or ended-lifetime evidence never replaces the
//! first correlated row.
//!
//! Verdicts are historical ordinary-focus facts, never current foreground or
//! selection authority: they emit no switch events, visibility writes, or
//! focus effects, and accepted focus memory stays exactly as published.

use std::collections::BTreeSet;

use crate::directional::WindowId;
use crate::world_engine::WorldEngine;
use crate::world_report::{FocusFeedback, OneShotEffect, VisibilitySetter, WorldReport};

/// Historical verdict for one retained ordinary focus grant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FocusVerdict {
    /// No feedback row has correlated with this grant yet.
    Pending,
    /// A correlated row proved the foreground: `foreground_proven` with a
    /// reported `post` equal to the granted target, regardless of setter.
    Proven,
    /// A correlated row proved refusal: not proven, the setter refused, and
    /// both pre and post name the same unchanged non-target foreground.
    Refused,
    /// A correlated row neither proved nor refused the attempt (including
    /// accepted-without-equality, unknown/null/blocked facts, no-call, and
    /// threw rows).
    Unproven,
}

/// One retained ordinary focus grant: the emitted effect as published, the
/// first exact-tuple correlated feedback row (if any), and its verdict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RetainedFocusGrant {
    pub(crate) effect: OneShotEffect,
    pub(crate) feedback: Option<FocusFeedback>,
    pub(crate) verdict: FocusVerdict,
}

/// Classify one first-correlated feedback row against its granted effect.
/// Arrival brackets, mechanical reasons, and the current report foreground
/// never decide the verdict.
#[must_use]
pub(crate) fn classify_focus_feedback(effect: &OneShotEffect, row: &FocusFeedback) -> FocusVerdict {
    if row.foreground_proven && row.post.as_ref() == Some(&effect.target) {
        return FocusVerdict::Proven;
    }
    if row.setter == VisibilitySetter::Refused
        && let (Some(pre), Some(post)) = (&row.pre, &row.post)
        && pre == post
        && pre != &effect.target
    {
        return FocusVerdict::Refused;
    }
    FocusVerdict::Unproven
}

impl WorldEngine {
    /// Consume accepted-report focus feedback against the retained latest
    /// batch. Runs once per accepted report (including consumed-command
    /// reports) before the command-only rollback boundary and independent
    /// of sweep readability holds. First exact opaque
    /// `(id, target, issuing revision)` match wins per grant, both within
    /// one report's rows and across later reports; unmatched tuples change
    /// nothing.
    pub(crate) fn settle_focus_feedback(&mut self, report: &WorldReport) {
        for grant in &mut self.retained_focus {
            if grant.feedback.is_some() {
                continue;
            }
            for row in &report.focus_feedback {
                if row.id == grant.effect.id
                    && row.target == grant.effect.target
                    && row.revision == grant.effect.issued_revision
                {
                    grant.verdict = classify_focus_feedback(&grant.effect, row);
                    grant.feedback = Some(row.clone());
                    break;
                }
            }
        }
    }

    /// Freeze the actually emitted focus batch at publish. Only ordinary
    /// (`await_shown=false`) grants are retained; a nonempty emitted focus
    /// batch supersedes the previous one, while an observation-only reply
    /// keeps it. Bounded to the latest two emitted effects (admission plus
    /// explicit command in practice).
    pub(crate) fn retain_published_focus(&mut self, effects: &[OneShotEffect]) {
        use crate::world_report::EffectKind;
        let fresh: Vec<RetainedFocusGrant> = effects
            .iter()
            .filter(|effect| effect.kind == EffectKind::Focus && !effect.await_shown)
            .cloned()
            .map(|effect| RetainedFocusGrant {
                effect,
                feedback: None,
                verdict: FocusVerdict::Pending,
            })
            .collect();
        if fresh.is_empty() {
            return;
        }
        // Bound comes from the actual producers (at most admission plus
        // one explicit command; focus-follows-cursor never coexists): no
        // silent truncation, no new production cap.
        debug_assert!(fresh.len() <= 2);
        self.retained_focus = fresh;
    }

    /// Prune retained focus grants on explicit close or proven complete
    /// inventory absence. Held rows and excluded raw windows still count as
    /// live, as do engine/member-known lifetimes; a close plus a reused
    /// same id can never match the pruned old grant (its issuing revision
    /// differs from any fresh batch).
    pub(crate) fn prune_focus_grants(&mut self, report: &WorldReport) {
        if !report.closed.is_empty() {
            let closed: BTreeSet<WindowId> = report.closed.iter().cloned().collect();
            self.retained_focus
                .retain(|grant| !closed.contains(&grant.effect.target));
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
            .retained_focus
            .iter()
            .map(|grant| grant.effect.target.clone())
            .filter(|target| {
                self.members.contains_key(target) || self.engine_knows_window_any(target)
            })
            .collect();
        self.retained_focus.retain(|grant| {
            let target = &grant.effect.target;
            live.contains(target)
                || held.contains(target)
                || excluded.contains(target)
                || known.contains(target)
        });
    }
}

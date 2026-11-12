//! Ordinary focus feedback classification and automatic-focus tokens.
//!
//! The single settlement owner is the retained `PendingFocus` slot (see
//! [`crate::world_close_focus`]): target equality satisfies first, then the
//! first exact opaque-tuple `(id, target, issuing revision)` feedback row
//! latches once, with terminal refusal/no-call retirement and inert
//! unproven retention. This module only classifies one correlated row and
//! interns the compact automatic-focus tokens for ephemeral carrier ids.
//! Verdicts never confer foreground or selection authority: selection keeps
//! its own independent member/visibility completion.

use std::collections::BTreeSet;

use crate::directional::WindowId;
use crate::world_engine::WorldEngine;
use crate::world_report::{FocusFeedback, VisibilitySetter};

/// Verdict for one first-correlated feedback row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FocusVerdict {
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

/// Classify one first-correlated feedback row against its granted target.
/// Arrival brackets, mechanical reasons, and the current report foreground
/// never decide the verdict.
#[must_use]
pub(crate) fn classify_focus_feedback(target: &WindowId, row: &FocusFeedback) -> FocusVerdict {
    if row.foreground_proven && row.post.as_ref() == Some(target) {
        return FocusVerdict::Proven;
    }
    if row.setter == VisibilitySetter::Refused
        && let (Some(pre), Some(post)) = (&row.pre, &row.post)
        && pre == post
        && pre != target
    {
        return FocusVerdict::Refused;
    }
    FocusVerdict::Unproven
}

impl WorldEngine {
    /// Intern one compact automatic-focus token per live lifetime: the
    /// first automatic-focus candidate's issuing revision for that
    /// lifetime, not necessarily the first emitted effect revision
    /// (admission allocates its token before a same-target explicit dedup
    /// drops the admission row in favor of the explicit winner). The map
    /// entry is reused across hover transitions and observations, so a
    /// repeated hover for the same lifetime retains its identical effect id
    /// (the adapter's at-most-once spent fence stays authoritative).
    /// Callers run after the revision-exhaustion gate, so
    /// `state_revision + 1` never wraps; accepted issuing revisions only
    /// increase, giving exact distinct tokens without hashing, truncation,
    /// or another counter. Explicit `fx-` identities are disjoint.
    pub(crate) fn automatic_focus_token(&mut self, target: &WindowId) -> u64 {
        if let Some(token) = self.automatic_focus_tokens.get(target) {
            return *token;
        }
        let token = self.state_revision + 1;
        self.automatic_focus_tokens.insert(target.clone(), token);
        token
    }

    /// Prune automatic-focus tokens on explicit close or proven complete
    /// departure (live, held, excluded, or engine/member-known lifetimes
    /// stay). A same-lifetime temporary slot exclusion never drops the
    /// mapping, so the spent fence is never renewed for the same lifetime.
    pub(crate) fn prune_automatic_focus_tokens(&mut self) {
        let observation = &self.staged.observation;
        if !observation.closed.is_empty() {
            let closed: BTreeSet<WindowId> = observation.closed.iter().cloned().collect();
            self.automatic_focus_tokens
                .retain(|target, _| !closed.contains(target));
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
            .automatic_focus_tokens
            .keys()
            .filter(|target| {
                self.members.contains_key(target) || self.engine_knows_window_any(target)
            })
            .cloned()
            .collect();
        self.automatic_focus_tokens.retain(|target, _| {
            live.contains(target)
                || held.contains(target)
                || excluded.contains(target)
                || known.contains(target)
        });
    }
}

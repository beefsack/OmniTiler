//! Canonical property-attempt record and the shared settlement entry.
//!
//! Wire-facing attempt rows stay at the boundary. This module owns their
//! single closed core record, ordered settlement phases, shared correlation,
//! retention and replay mechanics. Property policy (capability, readability,
//! verdict and continuations) stays in the state, clear, transfer, selection
//! and frame modules.

use std::collections::BTreeMap;

use crate::directional::{OutputId, WindowId};
use crate::geometry::Rect;
use crate::world_engine::EventIssue;
use crate::world_report::{
    ClearAttempt, FrameAttempt, FrameSetter, LastWritePath, MemberAttempt, MemberDisposition,
    StateAttempt, StateProperty, TransferKind, ViewAttempt, VisibilityAttempt, VisibilityReason,
    VisibilitySample, VisibilitySetter, WorkspaceRef, WorldCommand,
};

/// Shared mechanical call disposition carried by canonical attempts. The
/// accepted/refused/threw facts are common to every property kind. Membership
/// extends this with the membership-only no-call consumption fact in
/// [`MemberAttemptDisposition`]; no other arm can produce it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum AttemptDisposition {
    Accepted,
    Refused,
    Threw,
}

impl From<FrameSetter> for AttemptDisposition {
    fn from(value: FrameSetter) -> Self {
        match value {
            FrameSetter::Accepted => Self::Accepted,
            FrameSetter::Refused => Self::Refused,
            FrameSetter::Threw => Self::Threw,
        }
    }
}

/// Mechanical membership call disposition as a closed extension of the shared
/// [`AttemptDisposition`] call facts: every actual native call is one
/// `Called(shared fact)`, and the membership-only `AlreadyAtTarget` no-call
/// consumption fact is a separate structural arm that no other property arm's
/// normalization can produce. No universal invalid no-call exists on state,
/// clear or view arms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum MemberAttemptDisposition {
    Called(AttemptDisposition),
    AlreadyAtTarget,
}

impl From<MemberDisposition> for MemberAttemptDisposition {
    fn from(value: MemberDisposition) -> Self {
        match value {
            MemberDisposition::Accepted => Self::Called(AttemptDisposition::Accepted),
            MemberDisposition::Refused => Self::Called(AttemptDisposition::Refused),
            MemberDisposition::Threw => Self::Called(AttemptDisposition::Threw),
            MemberDisposition::AlreadyAtTarget => Self::AlreadyAtTarget,
        }
    }
}

/// Mechanical visibility call disposition as a closed extension of the shared
/// [`AttemptDisposition`] call facts: every actual native visibility setter is
/// one `Called(shared fact)`, and the visibility-only `NotCalled` no-call is a
/// separate structural arm that no other property arm's normalization can
/// produce. Like [`MemberAttemptDisposition`], no universal invalid no-call
/// exists on state, clear, view or frame arms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum VisibilityAttemptDisposition {
    Called(AttemptDisposition),
    NotCalled,
}

impl From<VisibilitySetter> for VisibilityAttemptDisposition {
    fn from(value: VisibilitySetter) -> Self {
        match value {
            VisibilitySetter::Accepted => Self::Called(AttemptDisposition::Accepted),
            VisibilitySetter::Refused => Self::Called(AttemptDisposition::Refused),
            VisibilitySetter::Threw => Self::Called(AttemptDisposition::Threw),
            VisibilitySetter::NotCalled => Self::NotCalled,
        }
    }
}

/// Closed canonical evidence for one native-state write. The subject is a
/// window, the correlation is the acted-on publication revision, and every
/// payload value is a bool or a stamp tagged by `property`; no frame,
/// workspace or membership value can be placed in this arm.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StateAttemptEvidence {
    pub(crate) window: WindowId,
    pub(crate) revision: u64,
    pub(crate) property: StateProperty,
    pub(crate) target: bool,
    pub(crate) disposition: AttemptDisposition,
    pub(crate) pre: Option<bool>,
    pub(crate) post: Option<bool>,
    pub(crate) attempt_ms: u64,
}

/// Closed canonical evidence for one maximize-clear. The target is
/// structurally `maximized = false`: it is the meaning of the clear arm and
/// is never carried as a second, forgeable bool. Correlation is the exact
/// operation id plus window; pre/post and the stamp are the original raw
/// facts, never a computed verdict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ClearAttemptEvidence {
    pub(crate) operation_id: String,
    pub(crate) window: WindowId,
    pub(crate) disposition: AttemptDisposition,
    pub(crate) pre: Option<bool>,
    pub(crate) post: Option<bool>,
    pub(crate) attempt_ms: u64,
}

/// Closed canonical evidence for one native membership write. Correlation is
/// the exact operation id plus window plus target homing; pre/post are the
/// original namespaced raw facts and the disposition is the shared mechanical
/// call fact (including the membership-only no-call consumption).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MemberAttemptEvidence {
    pub(crate) operation_id: String,
    pub(crate) window: WindowId,
    pub(crate) target_output: OutputId,
    pub(crate) target_workspace: String,
    pub(crate) disposition: MemberAttemptDisposition,
    pub(crate) pre: Option<WorkspaceRef>,
    pub(crate) post: Option<WorkspaceRef>,
    pub(crate) attempt_ms: u64,
}

/// Closed canonical evidence for one native view write. Correlation is the
/// exact owning phase id plus target homing; pre/post/baseline are the original
/// namespaced raw facts and the disposition is the shared mechanical call
/// fact. No membership fact can be placed in this arm.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ViewAttemptEvidence {
    pub(crate) operation_id: String,
    pub(crate) target_output: OutputId,
    pub(crate) target_workspace: String,
    pub(crate) disposition: AttemptDisposition,
    pub(crate) pre: Option<WorkspaceRef>,
    pub(crate) post: Option<WorkspaceRef>,
    pub(crate) baseline: Option<WorkspaceRef>,
    pub(crate) attempt_ms: u64,
}

/// Closed canonical evidence for one native visibility write or no-call. The
/// subject is a window, the correlation is the exact granted authorization
/// (revision plus desired shown target), and every payload is the original
/// typed raw fact carried verbatim from the wire-bound `LastWrite`: the actual
/// execution route, mechanical reason, and identity-bound pre/post reads.
/// There is no synthetic stamp or operation id: the disposition is the shared
/// call fact plus the visibility-only `NotCalled` no-call arm. The wire
/// `property` tag is structural here (this arm can only be visibility).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VisibilityAttemptEvidence {
    pub(crate) window: WindowId,
    pub(crate) revision: u64,
    pub(crate) target: bool,
    pub(crate) path: LastWritePath,
    pub(crate) disposition: VisibilityAttemptDisposition,
    pub(crate) reason: VisibilityReason,
    pub(crate) pre: Option<VisibilitySample>,
    pub(crate) post: Option<VisibilitySample>,
}

/// Closed canonical evidence for one native frame write. Correlation is the
/// exact window lifetime and attempted target, with an optional explicit float
/// operation identity; no synthetic revision or id is invented for ordinary
/// frame writes. Every payload (prewrite, readback, both DPI bases, stamp) is
/// the original raw fact carried verbatim, never a clamp or completion verdict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FrameAttemptEvidence {
    pub(crate) window: WindowId,
    pub(crate) target: Rect,
    pub(crate) disposition: AttemptDisposition,
    pub(crate) readback: Option<Rect>,
    pub(crate) dpi_before: Option<u32>,
    pub(crate) dpi_after: Option<u32>,
    pub(crate) attempt_ms: u64,
    pub(crate) operation_id: Option<String>,
    pub(crate) prewrite: Option<Rect>,
}

/// Normalize one raw wire frame attempt into its typed canonical evidence at
/// the capture boundary. Every field is carried over verbatim: no stamp,
/// target or verdict is invented, and the raw row is never re-ingested
/// downstream.
#[must_use]
pub(crate) fn frame_evidence(raw: &FrameAttempt) -> FrameAttemptEvidence {
    FrameAttemptEvidence {
        window: raw.window.clone(),
        target: raw.target,
        disposition: raw.setter.into(),
        readback: raw.readback,
        dpi_before: raw.dpi_before,
        dpi_after: raw.dpi_after,
        attempt_ms: raw.attempt_ms,
        operation_id: raw.operation_id.clone(),
        prewrite: raw.prewrite,
    }
}

/// Exact window correlation for an ordinary frame write, or the exact
/// (window, target, operation) tuple for an explicit float placement. A
/// `None` operation identity matches an ordinary write only, so an explicit
/// float attempt can never be staged as ordinary evidence and vice versa.
#[must_use]
pub(crate) fn frame_evidence_matches(
    evidence: &FrameAttemptEvidence,
    window: &WindowId,
    target: &Rect,
    operation_id: Option<&str>,
) -> bool {
    evidence.window == *window
        && evidence.target == *target
        && evidence.operation_id.as_deref() == operation_id
}

/// True when `attempt` is a lost-ack redelivery of an already-consumed frame
/// write for one lifetime, and must not mutate suppression or spend budget:
///
/// - a strictly older stamp (`ms != 0`) is a replay regardless of pair;
/// - an exact duplicate of the consumed evidence (all facts plus stamp equal)
///   is the same write reported twice, even at an equal stamp.
///
/// Deliberately narrow: an unreported clock (`ms == 0`) never dedups (no
/// ordering claim is possible, and same-tick duplicates are routine there);
/// a changed pair at an equal-or-newer stamp always processes.
#[must_use]
pub(crate) fn is_stale_replay(
    prior: &FrameAttemptEvidence,
    attempt: &FrameAttemptEvidence,
) -> bool {
    if attempt.attempt_ms == 0 {
        return false;
    }
    attempt.attempt_ms < prior.attempt_ms || attempt == prior
}

/// Shared prior-watermark replay gate for canonical frame evidence. The
/// consumed watermark is keyed by exact window lifetime; a zero stamp never
/// dedups and an exact duplicate is a replay. Both the tiled anti-fighting
/// lane and explicit float staging use this same mechanically exact check.
#[must_use]
pub(crate) fn frame_watermark_stale(
    consumed: &BTreeMap<WindowId, PropertyAttempt>,
    attempt: &FrameAttemptEvidence,
) -> bool {
    consumed
        .get(&attempt.window)
        .and_then(PropertyAttempt::as_frame)
        .is_some_and(|prior| is_stale_replay(prior, attempt))
}

/// Retain one consumed frame write in the exact-window watermark. Zero-stamp
/// evidence never enters the watermark (it makes no ordering claim); every
/// fresh stamped write replaces the prior row, so the watermark survives an
/// exact-match suppression clear as redelivery protection.
pub(crate) fn retain_frame_watermark(
    consumed: &mut BTreeMap<WindowId, PropertyAttempt>,
    attempt: &FrameAttemptEvidence,
) {
    if attempt.attempt_ms != 0 {
        consumed.insert(
            attempt.window.clone(),
            PropertyAttempt::Frame(attempt.clone()),
        );
    }
}

/// One canonical property attempt. This closed record replaces the per-kind
/// raw vectors on the settlement side; each arm is a distinct typed payload so
/// a state bool can never be read as a clear, membership, view or frame value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PropertyAttempt {
    State(StateAttemptEvidence),
    Clear(ClearAttemptEvidence),
    Member(MemberAttemptEvidence),
    View(ViewAttemptEvidence),
    Visibility(VisibilityAttemptEvidence),
    Frame(FrameAttemptEvidence),
}

impl PropertyAttempt {
    /// Normalize one raw wire state attempt at the capture boundary. Every
    /// field is carried over verbatim: no revision, stamp or verdict is
    /// invented, and the raw row is never re-ingested downstream.
    pub(crate) fn from_state_attempt(raw: &StateAttempt) -> Self {
        Self::State(StateAttemptEvidence {
            window: raw.window.clone(),
            revision: raw.revision,
            property: raw.property,
            target: raw.target,
            disposition: raw.setter.into(),
            pre: raw.pre,
            post: raw.post,
            attempt_ms: raw.attempt_ms,
        })
    }

    /// Normalize one raw wire clear attempt at the capture boundary. The
    /// false target is structural to this arm; all other facts are verbatim.
    pub(crate) fn from_clear_attempt(raw: &ClearAttempt) -> Self {
        Self::Clear(ClearAttemptEvidence {
            operation_id: raw.operation_id.clone(),
            window: raw.window.clone(),
            disposition: raw.setter.into(),
            pre: raw.pre_maximized,
            post: raw.post_maximized,
            attempt_ms: raw.attempt_ms,
        })
    }

    /// Normalize one raw wire membership attempt at the capture boundary.
    /// Every field is carried over verbatim: no verdict is invented and the
    /// raw row is never re-ingested downstream.
    pub(crate) fn from_member_attempt(raw: &MemberAttempt) -> Self {
        Self::Member(MemberAttemptEvidence {
            operation_id: raw.operation_id.clone(),
            window: raw.window.clone(),
            target_output: raw.target_output.clone(),
            target_workspace: raw.target_workspace.clone(),
            disposition: raw.setter.into(),
            pre: raw.pre.clone(),
            post: raw.post.clone(),
            attempt_ms: raw.attempt_ms,
        })
    }

    /// Normalize one raw wire view attempt at the capture boundary. Every
    /// field is carried over verbatim; the raw row is never re-ingested
    /// downstream.
    pub(crate) fn from_view_attempt(raw: &ViewAttempt) -> Self {
        Self::View(ViewAttemptEvidence {
            operation_id: raw.operation_id.clone(),
            target_output: raw.target_output.clone(),
            target_workspace: raw.target_workspace.clone(),
            disposition: raw.setter.into(),
            pre: raw.pre.clone(),
            post: raw.post.clone(),
            baseline: raw.baseline.clone(),
            attempt_ms: raw.attempt_ms,
        })
    }

    /// Normalize one wire-bound visibility attempt into its typed canonical
    /// evidence at the staged boundary. Every payload fact is carried over
    /// verbatim (no stamp, id, property tag or verdict is invented), and the
    /// raw DTO is never reconstructed downstream.
    #[must_use]
    pub(crate) fn from_visibility_attempt(raw: &VisibilityAttempt) -> Self {
        Self::Visibility(VisibilityAttemptEvidence {
            window: raw.window.clone(),
            revision: raw.revision,
            target: raw.target,
            path: raw.path,
            disposition: raw.setter.into(),
            reason: raw.reason,
            pre: raw.pre,
            post: raw.post,
        })
    }

    /// Normalize one raw wire frame attempt at the capture boundary. Every
    /// field is carried over verbatim; the raw row is never re-ingested
    /// downstream.
    pub(crate) fn from_frame_attempt(raw: &FrameAttempt) -> Self {
        Self::Frame(frame_evidence(raw))
    }

    /// Typed state access. A clear (or any future arm) reads as absent, so
    /// wrong-kind evidence can never satisfy state correlation.
    pub(crate) fn as_state(&self) -> Option<&StateAttemptEvidence> {
        match self {
            Self::State(evidence) => Some(evidence),
            _ => None,
        }
    }

    /// Typed clear access. A state (or any future arm) reads as absent, so
    /// wrong-kind evidence can never satisfy a clear binding.
    pub(crate) fn as_clear(&self) -> Option<&ClearAttemptEvidence> {
        match self {
            Self::Clear(evidence) => Some(evidence),
            _ => None,
        }
    }

    /// Typed membership access. A view (or any other arm) reads as absent, so
    /// wrong-kind evidence can never satisfy a member binding.
    pub(crate) fn as_member(&self) -> Option<&MemberAttemptEvidence> {
        match self {
            Self::Member(evidence) => Some(evidence),
            _ => None,
        }
    }

    /// Typed view access. A member (or any other arm) reads as absent, so
    /// wrong-kind evidence can never satisfy a view binding.
    pub(crate) fn as_view(&self) -> Option<&ViewAttemptEvidence> {
        match self {
            Self::View(evidence) => Some(evidence),
            _ => None,
        }
    }

    /// Typed visibility access. Every other arm reads as absent, so a state,
    /// clear, member, view or frame row can never satisfy a visibility
    /// correlation.
    pub(crate) fn as_visibility(&self) -> Option<&VisibilityAttemptEvidence> {
        match self {
            Self::Visibility(evidence) => Some(evidence),
            _ => None,
        }
    }

    /// Typed frame access. Every other arm reads as absent, so wrong-kind
    /// evidence can never satisfy frame correlation or float staging.
    pub(crate) fn as_frame(&self) -> Option<&FrameAttemptEvidence> {
        match self {
            Self::Frame(evidence) => Some(evidence),
            _ => None,
        }
    }
}

/// Intake rule for one retained evidence slot. First-match latches the first
/// exact correlation and never replaces it; replace-pending lets a later
/// report's exact correlation replace the retained row. Both rules take the
/// first matching row within a single report, preserving input order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RetentionPolicy {
    FirstMatch,
    ReplacePending,
}

/// Shared slot-retain primitive. The already-correlated candidate is cloned
/// once into the retained slot under the explicit policy; no owner keeps its
/// own intake copy or scans an attempt vector here. Returns whether it staged.
pub(crate) fn retain_candidate(
    candidate: &PropertyAttempt,
    slot: &mut Option<PropertyAttempt>,
    policy: RetentionPolicy,
) -> bool {
    if policy == RetentionPolicy::FirstMatch && slot.is_some() {
        return false;
    }
    *slot = Some(candidate.clone());
    true
}

/// Shared exact-correlated lookup and retain for owners that scan the staged
/// attempt slice (the clear owners). State uses the same primitive through its
/// keyed BTreeMap lookup instead, keeping that intake attempts-major.
pub(crate) fn retain_matching_attempt<F>(
    attempts: &[PropertyAttempt],
    mut matches: F,
    slot: &mut Option<PropertyAttempt>,
    policy: RetentionPolicy,
) -> bool
where
    F: FnMut(&PropertyAttempt) -> bool,
{
    for attempt in attempts {
        if !matches(attempt) {
            continue;
        }
        return retain_candidate(attempt, slot, policy);
    }
    false
}

/// Exact op/window correlation for canonical clear evidence. Forged rows
/// (operation or window mismatch) are never evidence: they read as absent.
pub(crate) fn clear_evidence_matches(
    evidence: &ClearAttemptEvidence,
    operation_id: &str,
    window: &WindowId,
) -> bool {
    evidence.operation_id == operation_id && evidence.window == *window
}

/// Exact op/window/target correlation for canonical membership evidence.
/// Forged rows (operation, window or target mismatch) are never evidence: they
/// read as absent.
pub(crate) fn member_evidence_matches(
    evidence: &MemberAttemptEvidence,
    operation_id: &str,
    window: &WindowId,
    target_output: &OutputId,
    target_workspace: &str,
) -> bool {
    evidence.operation_id == operation_id
        && evidence.window == *window
        && evidence.target_output == *target_output
        && evidence.target_workspace == target_workspace
}

/// Exact op/target correlation for canonical view evidence. Forged rows
/// (operation or target mismatch) are never evidence: they read as absent.
pub(crate) fn view_evidence_matches(
    evidence: &ViewAttemptEvidence,
    operation_id: &str,
    target_output: &OutputId,
    target_workspace: &str,
) -> bool {
    evidence.operation_id == operation_id
        && evidence.target_output == *target_output
        && evidence.target_workspace == target_workspace
}

/// Exact window/revision/target correlation for canonical visibility evidence.
/// The revision and target are the exact granted authorization frozen for the
/// switch member; a stale revision, wrong polarity or foreign window is never
/// evidence. Wrong-kind rows read as absent through `as_visibility`.
#[must_use]
pub(crate) fn visibility_evidence_matches(
    evidence: &VisibilityAttemptEvidence,
    window: &WindowId,
    shown: bool,
    rev: u64,
) -> bool {
    evidence.window == *window && evidence.target == shown && evidence.revision == rev
}

/// Proven clear refusal: an explicit native refusal with an unchanged readable
/// maximized preimage. Anything else (accepted mismatch, throw, unreadable
/// proof, moved preimage) latches ambiguity, never refusal.
pub(crate) fn clear_refused(evidence: &ClearAttemptEvidence) -> bool {
    evidence.disposition == AttemptDisposition::Refused
        && evidence.pre == Some(true)
        && evidence.post == Some(true)
}

/// Proven clear dispatch: the setter ran (accepted, possibly completing
/// asynchronously) against a readable maximized preimage. Never proof on its
/// own: the staged observation must also show complete readable unmaximized
/// facts (checked by the owning policy).
pub(crate) fn clear_accepted(evidence: &ClearAttemptEvidence) -> bool {
    evidence.disposition == AttemptDisposition::Accepted && evidence.pre == Some(true)
}

/// Explicit ordered property phase. Production keeps `Clears` at the two old
/// `settle_rest` calls before member work, `Views` at the old `settle_views_pre`
/// position before rebuild, `Members` in the later `settle_rest` window before
/// migration (with float evidence staged inside that window), `States` at the
/// old `run_v4` position, `Visibility` at the old `settle_selection_switch`
/// position after selection feedback, and `Frames` at the old `settle_frames`
/// position after selection feedback; the phases do not commute with the
/// pipeline.
#[derive(Debug, Clone, Copy)]
pub(crate) enum PropertyAttemptPhase<'a> {
    States,
    Clears {
        current_command: Option<&'a WorldCommand>,
        issue: &'a EventIssue,
        consumed: bool,
    },
    Views,
    Members,
    Visibility,
    Frames,
}

impl crate::world_transaction::WorldTransaction<'_> {
    /// Unified ordered settlement entry for canonical property attempts.
    /// Each phase correlates/retains evidence under its property's rule,
    /// then runs that property's policy against the staged facts.
    pub(crate) fn settle_property_attempts(&mut self, phase: PropertyAttemptPhase<'_>) {
        match phase {
            PropertyAttemptPhase::States => {
                if self.world.settlement.retained_states.len()
                    > crate::world_state::MAX_RETAINED_STATE_BINDINGS
                {
                    return;
                }
                // Latest completed publication bounds incoming evidence.
                let latest = self.world.state_revision;
                self.correlate_retain_state_attempts(latest);
                self.evaluate_state_bindings();
            }
            PropertyAttemptPhase::Clears {
                current_command,
                issue,
                consumed,
            } => {
                self.correlate_retain_clear_attempts();
                self.settle_clears(current_command, issue, consumed);
                self.settle_born_clears();
            }
            PropertyAttemptPhase::Views => self.correlate_retain_view_attempts(),
            PropertyAttemptPhase::Members => self.correlate_retain_member_attempts(),
            // Visibility keeps its direct policy entry for existing callers;
            // the phase routes through it at the old settle position. The
            // shared keyed intake and proof policy stay in `world_selection`.
            PropertyAttemptPhase::Visibility => self.settle_selection_switch(),
            // The frame lane shares the common exact-window correlation and
            // replay gate above before its own tiled anti-fighting policy.
            // Float evidence was already staged in the earlier Members window
            // inside settle_rest; it is consumed against the same watermark.
            PropertyAttemptPhase::Frames => self.settle_frames(),
        }
    }

    /// Correlate and first-match retain the state arm, attempts-major: each
    /// staged row does one keyed BTreeMap lookup, so intake stays
    /// O(attempts log bindings) as before rather than scanning every attempt
    /// for every binding. Correlation is exact subject/property, exact target
    /// identity and a revision inside [binding rev, latest published]. A held
    /// or incomplete receipt still records first: readability is a policy gate
    /// at proof time, not an intake filter. Spent bindings never reopen, so
    /// later or contradictory rows cannot replace the retained row.
    fn correlate_retain_state_attempts(&mut self, latest: u64) {
        let attempts = &self.world.staged.observation.property_attempts;
        for attempt in attempts {
            let Some(evidence) = attempt.as_state() else {
                continue;
            };
            if !crate::world_state::state_property_supported(
                self.connection.capabilities(),
                evidence.property,
            ) {
                continue;
            }
            let key = (evidence.window.clone(), evidence.property);
            let Some(binding) = self.world.settlement.retained_states.get_mut(&key) else {
                continue;
            };
            if binding.target.window != evidence.window
                || binding.target.property != evidence.property
                || binding.target.value != evidence.target
            {
                continue;
            }
            if evidence.revision < binding.target.revision || evidence.revision > latest {
                continue;
            }
            if binding.spent {
                continue;
            }
            if retain_candidate(attempt, &mut binding.pending, RetentionPolicy::FirstMatch) {
                binding.spent = true;
            }
        }
    }

    /// Correlate both clear owners with the replace-pending rule. Evidence
    /// stages before any readability or authority hold, but an explicit close
    /// prunes first, so no evidence is retained for a binding the policy
    /// drops below. Exact op/window matching rejects forged rows.
    fn correlate_retain_clear_attempts(&mut self) {
        let attempts = &self.world.staged.observation.property_attempts;
        let closes = &self.world.staged.closes;
        for retained in &mut self.world.settlement.retained_clears {
            if closes.contains(&retained.window) {
                continue;
            }
            let operation_id = retained.operation_id.clone();
            let window = retained.window.clone();
            retain_matching_attempt(
                attempts,
                |attempt| {
                    attempt.as_clear().is_some_and(|evidence| {
                        clear_evidence_matches(evidence, &operation_id, &window)
                    })
                },
                &mut retained.pending,
                RetentionPolicy::ReplacePending,
            );
        }
        for retained in &mut self.world.settlement.retained_born {
            if closes.contains(&retained.window) {
                continue;
            }
            let operation_id = retained.operation_id.clone();
            let window = retained.window.clone();
            retain_matching_attempt(
                attempts,
                |attempt| {
                    attempt.as_clear().is_some_and(|evidence| {
                        clear_evidence_matches(evidence, &operation_id, &window)
                    })
                },
                &mut retained.pending,
                RetentionPolicy::ReplacePending,
            );
        }
    }

    /// Correlate and replace-pending retain the view arm into each retained
    /// view flight's exact (op, output, workspace) slot. Runs at the old
    /// `settle_views_pre` position before rebuild. Staging happens before any
    /// readability or authority hold; forged rows read as absent. Refill
    /// children and migration parent views use the same exact correlation.
    fn correlate_retain_view_attempts(&mut self) {
        // Native-free views are proved by the frozen selection binding, never
        // by the echo/attempt lane: the old `settle_views_pre` kept them
        // unstaged and that gate must stay here.
        if !self.connection.host_owned_workspace_domains() {
            return;
        }
        let attempts = &self.world.staged.observation.property_attempts;
        for retained in &mut self.world.settlement.retained {
            // Standalone/follow/refill view flights; the migration parent's
            // target-view proof rides the parent entry and is staged with its
            // migration settlement (still through the shared primitive).
            if !matches!(&retained.target.kind, TransferKind::View) {
                continue;
            }
            let operation_id = retained.target.operation_id.clone();
            let target_output = retained.target.target_output.clone();
            let target_workspace = retained.target.target_workspace.clone();
            retain_matching_attempt(
                attempts,
                |attempt| {
                    attempt.as_view().is_some_and(|evidence| {
                        view_evidence_matches(
                            evidence,
                            &operation_id,
                            &target_output,
                            &target_workspace,
                        )
                    })
                },
                &mut retained.pending_view,
                RetentionPolicy::ReplacePending,
            );
        }
    }

    /// Correlate and replace-pending retain the membership arm into each
    /// retained window/migration slot. Runs in the later `settle_rest` window,
    /// after clears and before migration/float settlement. Window transfers
    /// stage their exact latest and same-window predecessors; migrations stage
    /// per-member latest evidence. Staging happens before any hold; forged
    /// rows read as absent.
    fn correlate_retain_member_attempts(&mut self) {
        let attempts = &self.world.staged.observation.property_attempts;
        for retained in &mut self.world.settlement.retained {
            match &retained.target.kind {
                TransferKind::Window(window) => {
                    let operation_id = retained.target.operation_id.clone();
                    let target_output = retained.target.target_output.clone();
                    let target_workspace = retained.target.target_workspace.clone();
                    let window = window.clone();
                    retain_matching_attempt(
                        attempts,
                        |attempt| {
                            attempt.as_member().is_some_and(|evidence| {
                                member_evidence_matches(
                                    evidence,
                                    &operation_id,
                                    &window,
                                    &target_output,
                                    &target_workspace,
                                )
                            })
                        },
                        &mut retained.pending_member,
                        RetentionPolicy::ReplacePending,
                    );
                    for (pred_target, pred_attempt) in retained.member_predecessors.iter_mut() {
                        if !matches!(&pred_target.kind, TransferKind::Window(w) if w == &window) {
                            continue;
                        }
                        let pred_op = pred_target.operation_id.clone();
                        let pred_output = pred_target.target_output.clone();
                        let pred_workspace = pred_target.target_workspace.clone();
                        retain_matching_attempt(
                            attempts,
                            |attempt| {
                                attempt.as_member().is_some_and(|evidence| {
                                    member_evidence_matches(
                                        evidence,
                                        &pred_op,
                                        &window,
                                        &pred_output,
                                        &pred_workspace,
                                    )
                                })
                            },
                            pred_attempt,
                            RetentionPolicy::ReplacePending,
                        );
                    }
                }
                TransferKind::Migration => {
                    let operation_id = retained.target.operation_id.clone();
                    let target_output = retained.target.target_output.clone();
                    let target_workspace = retained.target.target_workspace.clone();
                    for member in retained.members.clone() {
                        let mut slot = retained.pending_members.remove(&member);
                        retain_matching_attempt(
                            attempts,
                            |attempt| {
                                attempt.as_member().is_some_and(|evidence| {
                                    member_evidence_matches(
                                        evidence,
                                        &operation_id,
                                        &member,
                                        &target_output,
                                        &target_workspace,
                                    )
                                })
                            },
                            &mut slot,
                            RetentionPolicy::ReplacePending,
                        );
                        if let Some(value) = slot {
                            retained.pending_members.insert(member, value);
                        }
                    }
                }
                TransferKind::View => {}
            }
        }
    }

    /// Evaluate non-terminal retained bindings through world_state policy,
    /// then store the verdict. Terminal verdicts stay stable and are never
    /// recomputed from a later observation.
    fn evaluate_state_bindings(&mut self) {
        let keys: Vec<(WindowId, StateProperty)> = self
            .world
            .settlement
            .retained_states
            .keys()
            .cloned()
            .collect();
        for key in keys {
            if self.state_binding_terminal(&key) {
                continue;
            }
            let (target, evidence) = match self.world.settlement.retained_states.get(&key) {
                Some(binding) => {
                    if binding.target.window != key.0 || binding.target.property != key.1 {
                        continue;
                    }
                    let Some(evidence) = binding
                        .pending
                        .as_ref()
                        .and_then(PropertyAttempt::as_state)
                        .cloned()
                    else {
                        continue;
                    };
                    (binding.target.clone(), evidence)
                }
                None => continue,
            };
            let next = self.classify_state_attempt(&target, &evidence);
            if let Some(binding) = self.world.settlement.retained_states.get_mut(&key) {
                binding.disposition = next;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world_report::VisibilityAttempt;

    fn window(name: &str) -> WindowId {
        WindowId(name.to_owned())
    }

    fn clear(op: &str, win: &str, disposition: AttemptDisposition) -> PropertyAttempt {
        PropertyAttempt::from_clear_attempt(&ClearAttempt {
            operation_id: op.to_owned(),
            window: window(win),
            setter: match disposition {
                AttemptDisposition::Accepted => FrameSetter::Accepted,
                AttemptDisposition::Refused => FrameSetter::Refused,
                AttemptDisposition::Threw => FrameSetter::Threw,
            },
            pre_maximized: Some(true),
            post_maximized: Some(true),
            attempt_ms: 7,
        })
    }

    fn state() -> PropertyAttempt {
        PropertyAttempt::from_state_attempt(&StateAttempt {
            window: window("win-a"),
            revision: 1,
            property: StateProperty::Maximized,
            target: false,
            setter: FrameSetter::Accepted,
            pre: Some(true),
            post: Some(false),
            attempt_ms: 9,
        })
    }

    #[test]
    fn shared_slot_retain_honours_first_match_and_replace() {
        let first = state();
        let second = clear("op-1", "win-a", AttemptDisposition::Accepted);
        let mut slot = Some(first.clone());
        assert!(!retain_candidate(
            &second,
            &mut slot,
            RetentionPolicy::FirstMatch
        ));
        assert_eq!(slot, Some(first));
        assert!(retain_candidate(
            &second,
            &mut slot,
            RetentionPolicy::ReplacePending
        ));
        assert_eq!(slot, Some(second));
    }

    #[test]
    fn mixed_state_and_clear_retain_only_the_matching_kind() {
        let attempts = vec![
            state(),
            clear("op-1", "win-a", AttemptDisposition::Accepted),
        ];
        let mut clear_slot = None;
        let staged = retain_matching_attempt(
            &attempts,
            |attempt| {
                attempt.as_clear().is_some_and(|evidence| {
                    clear_evidence_matches(evidence, "op-1", &window("win-a"))
                })
            },
            &mut clear_slot,
            RetentionPolicy::ReplacePending,
        );
        assert!(staged);
        assert_eq!(
            clear_slot
                .as_ref()
                .and_then(PropertyAttempt::as_clear)
                .map(|e| e.operation_id.as_str()),
            Some("op-1")
        );
        assert!(
            clear_slot
                .as_ref()
                .and_then(PropertyAttempt::as_state)
                .is_none()
        );
        let mut state_slot = None;
        let staged = retain_matching_attempt(
            &attempts,
            |attempt| {
                attempt
                    .as_state()
                    .is_some_and(|evidence| evidence.window == window("win-a"))
            },
            &mut state_slot,
            RetentionPolicy::FirstMatch,
        );
        assert!(staged);
        assert_eq!(
            state_slot
                .as_ref()
                .and_then(PropertyAttempt::as_state)
                .map(|e| e.attempt_ms),
            Some(9)
        );
        assert!(
            state_slot
                .as_ref()
                .and_then(PropertyAttempt::as_clear)
                .is_none()
        );
    }

    #[test]
    fn first_match_keeps_first_and_replace_pending_takes_report_first() {
        let attempts = vec![
            clear("op-1", "win-a", AttemptDisposition::Accepted),
            clear("op-1", "win-a", AttemptDisposition::Refused),
            state(),
        ];
        let mut first = Some(clear("op-1", "win-a", AttemptDisposition::Threw));
        let staged = retain_matching_attempt(
            &attempts,
            |attempt| {
                attempt.as_clear().is_some_and(|evidence| {
                    clear_evidence_matches(evidence, "op-1", &window("win-a"))
                })
            },
            &mut first,
            RetentionPolicy::FirstMatch,
        );
        assert!(!staged);
        assert_eq!(
            first
                .as_ref()
                .and_then(PropertyAttempt::as_clear)
                .map(|e| e.disposition),
            Some(AttemptDisposition::Threw)
        );
        let mut replaced = Some(clear("op-1", "win-a", AttemptDisposition::Threw));
        let staged = retain_matching_attempt(
            &attempts,
            |attempt| {
                attempt.as_clear().is_some_and(|evidence| {
                    clear_evidence_matches(evidence, "op-1", &window("win-a"))
                })
            },
            &mut replaced,
            RetentionPolicy::ReplacePending,
        );
        assert!(staged);
        assert_eq!(
            replaced
                .as_ref()
                .and_then(PropertyAttempt::as_clear)
                .map(|e| e.disposition),
            Some(AttemptDisposition::Accepted)
        );
    }

    #[test]
    fn normalized_clear_preserves_raw_facts_and_preimage_predicates() {
        let raw = ClearAttempt {
            operation_id: "op-9".to_owned(),
            window: window("win-b"),
            setter: FrameSetter::Refused,
            pre_maximized: Some(true),
            post_maximized: Some(true),
            attempt_ms: 42,
        };
        let normalized = PropertyAttempt::from_clear_attempt(&raw);
        let evidence = normalized.as_clear().expect("clear arm");
        assert_eq!(evidence.operation_id, "op-9");
        assert_eq!(evidence.window, window("win-b"));
        assert_eq!(evidence.disposition, AttemptDisposition::Refused);
        assert_eq!((evidence.pre, evidence.post), (Some(true), Some(true)));
        assert_eq!(evidence.attempt_ms, 42);
        assert!(clear_refused(evidence));
        assert!(!clear_accepted(evidence));
    }

    fn workspace(output: &str, name: &str) -> WorkspaceRef {
        WorkspaceRef::Logical {
            output: OutputId(output.to_owned()),
            workspace: crate::directional::WorkspaceId(name.to_owned()),
        }
    }

    fn member(op: &str, disposition: MemberDisposition) -> PropertyAttempt {
        PropertyAttempt::from_member_attempt(&MemberAttempt {
            operation_id: op.to_owned(),
            window: window("win-a"),
            target_output: OutputId("out-a".to_owned()),
            target_workspace: "ws-1".to_owned(),
            setter: disposition,
            pre: Some(workspace("out-b", "ws-9")),
            post: Some(workspace("out-a", "ws-1")),
            attempt_ms: 11,
        })
    }

    fn view(op: &str, disposition: FrameSetter) -> PropertyAttempt {
        PropertyAttempt::from_view_attempt(&ViewAttempt {
            operation_id: op.to_owned(),
            target_output: OutputId("out-a".to_owned()),
            target_workspace: "ws-1".to_owned(),
            setter: disposition,
            pre: Some(workspace("out-b", "ws-9")),
            post: Some(workspace("out-a", "ws-1")),
            baseline: Some(workspace("out-b", "ws-9")),
            attempt_ms: 13,
        })
    }

    #[test]
    fn normalized_member_and_view_preserve_raw_facts_and_dispositions() {
        let member = member("op-m", MemberDisposition::AlreadyAtTarget);
        let evidence = member.as_member().expect("member arm");
        assert_eq!(evidence.operation_id, "op-m");
        assert_eq!(evidence.window, window("win-a"));
        assert_eq!(evidence.target_output, OutputId("out-a".to_owned()));
        assert_eq!(evidence.target_workspace, "ws-1");
        assert_eq!(
            evidence.disposition,
            MemberAttemptDisposition::AlreadyAtTarget
        );
        assert_eq!(evidence.pre, Some(workspace("out-b", "ws-9")));
        assert_eq!(evidence.post, Some(workspace("out-a", "ws-1")));
        assert_eq!(evidence.attempt_ms, 11);
        assert!(member.as_view().is_none());

        let view = view("op-v", FrameSetter::Refused);
        let evidence = view.as_view().expect("view arm");
        assert_eq!(evidence.operation_id, "op-v");
        assert_eq!(evidence.target_output, OutputId("out-a".to_owned()));
        assert_eq!(evidence.target_workspace, "ws-1");
        assert_eq!(evidence.disposition, AttemptDisposition::Refused);
        assert_eq!(evidence.baseline, Some(workspace("out-b", "ws-9")));
        assert_eq!(evidence.attempt_ms, 13);
        assert!(view.as_member().is_none());
    }

    #[test]
    fn member_and_view_bindings_reject_wrong_kind_and_forged_rows() {
        let attempts = vec![
            member("op-m", MemberDisposition::Accepted),
            view("op-v", FrameSetter::Accepted),
        ];
        let out_a = OutputId("out-a".to_owned());
        let mut member_slot = None;
        assert!(retain_matching_attempt(
            &attempts,
            |attempt| {
                attempt.as_member().is_some_and(|evidence| {
                    member_evidence_matches(evidence, "op-m", &window("win-a"), &out_a, "ws-1")
                })
            },
            &mut member_slot,
            RetentionPolicy::ReplacePending,
        ));
        assert!(
            member_slot
                .as_ref()
                .and_then(PropertyAttempt::as_view)
                .is_none()
        );
        let mut view_slot = None;
        assert!(retain_matching_attempt(
            &attempts,
            |attempt| {
                attempt
                    .as_view()
                    .is_some_and(|evidence| view_evidence_matches(evidence, "op-v", &out_a, "ws-1"))
            },
            &mut view_slot,
            RetentionPolicy::FirstMatch,
        ));
        assert!(
            view_slot
                .as_ref()
                .and_then(PropertyAttempt::as_member)
                .is_none()
        );
        // Forged operation / target rows never correlate.
        let mut forged = None;
        assert!(!retain_matching_attempt(
            &attempts,
            |attempt| {
                attempt.as_member().is_some_and(|evidence| {
                    member_evidence_matches(evidence, "op-X", &window("win-a"), &out_a, "ws-1")
                })
            },
            &mut forged,
            RetentionPolicy::ReplacePending,
        ));
        assert!(forged.is_none());
    }

    #[test]
    fn member_view_retention_takes_first_row_within_report_and_replaces_later() {
        let out_a = OutputId("out-a".to_owned());
        let attempts = vec![
            member("op-m", MemberDisposition::Accepted),
            member("op-m", MemberDisposition::Refused),
        ];
        let mut slot = Some(member("op-m", MemberDisposition::Threw));
        assert!(retain_matching_attempt(
            &attempts,
            |attempt| {
                attempt.as_member().is_some_and(|evidence| {
                    member_evidence_matches(evidence, "op-m", &window("win-a"), &out_a, "ws-1")
                })
            },
            &mut slot,
            RetentionPolicy::ReplacePending,
        ));
        assert_eq!(
            slot.as_ref()
                .and_then(PropertyAttempt::as_member)
                .map(|evidence| evidence.disposition),
            Some(MemberAttemptDisposition::Called(
                AttemptDisposition::Accepted
            )),
            "first matching row within the report wins"
        );
        assert!(!retain_matching_attempt(
            &attempts,
            |attempt| {
                attempt.as_member().is_some_and(|evidence| {
                    member_evidence_matches(evidence, "op-m", &window("win-a"), &out_a, "ws-1")
                })
            },
            &mut slot,
            RetentionPolicy::FirstMatch,
        ));
    }

    #[test]
    fn member_view_normalization_shares_call_facts_and_isolates_kinds() {
        // Every actual call disposition, for both membership and view, is the
        // one shared AttemptDisposition; the no-call fact is membership-only.
        for (setter, shared) in [
            (FrameSetter::Accepted, AttemptDisposition::Accepted),
            (FrameSetter::Refused, AttemptDisposition::Refused),
            (FrameSetter::Threw, AttemptDisposition::Threw),
        ] {
            let member = member("op-m", MemberDisposition::from(setter));
            assert_eq!(
                member.as_member().map(|evidence| evidence.disposition),
                Some(MemberAttemptDisposition::Called(shared))
            );
            let view = view("op-v", setter);
            assert_eq!(
                view.as_view().map(|evidence| evidence.disposition),
                Some(shared)
            );
        }
        let no_call = member("op-m", MemberDisposition::AlreadyAtTarget);
        assert_eq!(
            no_call.as_member().map(|evidence| evidence.disposition),
            Some(MemberAttemptDisposition::AlreadyAtTarget)
        );

        // Kind isolation across all four canonical arms: a binding of one
        // kind never reads raw evidence of another kind, and no state/clear/
        // view arm can forge a membership no-call.
        let arms = [
            state(),
            clear("op-c", "win-a", AttemptDisposition::Accepted),
            member("op-m", MemberDisposition::AlreadyAtTarget),
            view("op-v", FrameSetter::Accepted),
        ];
        for (index, arm) in arms.iter().enumerate() {
            assert_eq!(arm.as_state().is_some(), index == 0, "state arm {index}");
            assert_eq!(arm.as_clear().is_some(), index == 1, "clear arm {index}");
            assert_eq!(arm.as_member().is_some(), index == 2, "member arm {index}");
            assert_eq!(arm.as_view().is_some(), index == 3, "view arm {index}");
        }
    }

    fn frame(op: Option<&str>, ms: u64) -> PropertyAttempt {
        PropertyAttempt::from_frame_attempt(&FrameAttempt {
            window: window("win-a"),
            target: Rect {
                x: 1,
                y: 2,
                w: 300,
                h: 400,
            },
            setter: FrameSetter::Threw,
            readback: Some(Rect {
                x: 1,
                y: 2,
                w: 200,
                h: 400,
            }),
            dpi_before: Some(96),
            dpi_after: Some(120),
            attempt_ms: ms,
            operation_id: op.map(str::to_owned),
            prewrite: Some(Rect {
                x: 1,
                y: 2,
                w: 150,
                h: 400,
            }),
        })
    }

    #[test]
    fn frame_normalization_preserves_raw_facts_and_isolates_kind() {
        let raw = FrameAttempt {
            window: window("win-b"),
            target: Rect {
                x: 3,
                y: 4,
                w: 500,
                h: 600,
            },
            setter: FrameSetter::Accepted,
            readback: None,
            dpi_before: Some(120),
            dpi_after: None,
            attempt_ms: 42,
            operation_id: Some("op-f".to_owned()),
            prewrite: Some(Rect {
                x: 3,
                y: 4,
                w: 100,
                h: 600,
            }),
        };
        let normalized = PropertyAttempt::from_frame_attempt(&raw);
        let evidence = normalized.as_frame().expect("frame arm");
        assert_eq!(evidence.window, window("win-b"));
        assert_eq!(evidence.target, raw.target);
        assert_eq!(evidence.disposition, AttemptDisposition::Accepted);
        assert_eq!(evidence.readback, None);
        assert_eq!((evidence.dpi_before, evidence.dpi_after), (Some(120), None));
        assert_eq!(evidence.attempt_ms, 42);
        assert_eq!(evidence.operation_id.as_deref(), Some("op-f"));
        assert_eq!(evidence.prewrite, raw.prewrite);
        // No other arm can read frame evidence and vice versa.
        assert!(normalized.as_state().is_none());
        assert!(normalized.as_clear().is_none());
        assert!(normalized.as_member().is_none());
        assert!(normalized.as_view().is_none());
        for arm in [state(), clear("op-c", "win-a", AttemptDisposition::Refused)] {
            assert!(arm.as_frame().is_none());
        }
    }

    #[test]
    fn frame_replay_predicate_truthfulness_and_exact_correlation() {
        let ordinary = frame(None, 100);
        let prior = ordinary.as_frame().expect("prior").clone();
        // Older stamp and exact duplicate are replays; a fresh same-window
        // changed pair and a zero stamp are not.
        assert!(is_stale_replay(&prior, frame(None, 99).as_frame().unwrap()));
        assert!(is_stale_replay(
            &prior,
            frame(None, 100).as_frame().unwrap()
        ));
        let mut changed = frame(None, 100).as_frame().unwrap().clone();
        changed.target.w += 1;
        assert!(!is_stale_replay(&prior, &changed));
        let clockless = frame(None, 0);
        assert!(!is_stale_replay(&prior, clockless.as_frame().unwrap()));
        assert!(!is_stale_replay(
            clockless.as_frame().unwrap(),
            clockless.as_frame().unwrap()
        ));
        // Ordinary correlation never matches an explicit float identity.
        let target = prior.target;
        assert!(frame_evidence_matches(
            &prior,
            &window("win-a"),
            &target,
            None
        ));
        assert!(!frame_evidence_matches(
            &prior,
            &window("win-a"),
            &target,
            Some("op-f")
        ));
        let explicit = frame(Some("op-f"), 100);
        assert!(frame_evidence_matches(
            explicit.as_frame().unwrap(),
            &window("win-a"),
            &target,
            Some("op-f")
        ));
        // The watermark gate and retention share the same exact-window key.
        let mut watermark = BTreeMap::new();
        retain_frame_watermark(&mut watermark, &prior);
        assert_eq!(watermark.len(), 1);
        assert!(frame_watermark_stale(
            &watermark,
            frame(None, 100).as_frame().unwrap()
        ));
        let mut zero = BTreeMap::new();
        retain_frame_watermark(&mut zero, clockless.as_frame().unwrap());
        assert!(zero.is_empty(), "zero stamp never enters the watermark");
    }

    fn visibility_last(setter: VisibilitySetter, with_post: bool) -> VisibilityAttempt {
        VisibilityAttempt {
            window: window("win-v"),
            target: true,
            revision: 7,
            path: LastWritePath::Fallback,
            setter,
            reason: VisibilityReason::ScopeHeld,
            pre: Some(VisibilitySample {
                obs: 2,
                lifetime_matched: Some(true),
                visibility: crate::world_report::VisibilityFact {
                    visible: Some(false),
                    suppression_owned: Some(false),
                },
                minimized: Some(false),
                host_hidden: Some(false),
            }),
            post: with_post.then_some(VisibilitySample {
                obs: 4,
                lifetime_matched: Some(true),
                visibility: crate::world_report::VisibilityFact {
                    visible: Some(true),
                    suppression_owned: Some(false),
                },
                minimized: Some(false),
                host_hidden: Some(false),
            }),
        }
    }

    #[test]
    fn visibility_normalization_preserves_raw_facts_and_isolates_kind() {
        let raw = visibility_last(VisibilitySetter::Refused, true);
        let normalized = PropertyAttempt::from_visibility_attempt(&raw);
        let evidence = normalized.as_visibility().expect("visibility arm");
        assert_eq!(evidence.window, window("win-v"));
        assert_eq!(evidence.revision, 7);
        assert!(evidence.target);
        assert_eq!(evidence.path, LastWritePath::Fallback);
        assert_eq!(evidence.reason, VisibilityReason::ScopeHeld);
        assert_eq!(
            evidence.disposition,
            VisibilityAttemptDisposition::Called(AttemptDisposition::Refused)
        );
        assert_eq!(evidence.pre, raw.pre);
        assert_eq!(evidence.post, raw.post);
        // No other arm can read visibility evidence and vice versa.
        assert!(normalized.as_state().is_none());
        assert!(normalized.as_clear().is_none());
        assert!(normalized.as_member().is_none());
        assert!(normalized.as_view().is_none());
        assert!(normalized.as_frame().is_none());
        for arm in [
            state(),
            clear("op-c", "win-a", AttemptDisposition::Refused),
            member("op-m", MemberDisposition::Accepted),
            view("op-v", FrameSetter::Accepted),
            frame(None, 42),
        ] {
            assert!(arm.as_visibility().is_none());
        }
    }

    #[test]
    fn visibility_disposition_shares_call_facts_and_isolates_notcalled() {
        for (setter, shared) in [
            (VisibilitySetter::Accepted, AttemptDisposition::Accepted),
            (VisibilitySetter::Refused, AttemptDisposition::Refused),
            (VisibilitySetter::Threw, AttemptDisposition::Threw),
        ] {
            let normalized =
                PropertyAttempt::from_visibility_attempt(&visibility_last(setter, false));
            assert_eq!(
                normalized.as_visibility().map(|e| e.disposition),
                Some(VisibilityAttemptDisposition::Called(shared))
            );
        }
        let not_called = PropertyAttempt::from_visibility_attempt(&visibility_last(
            VisibilitySetter::NotCalled,
            false,
        ));
        assert_eq!(
            not_called.as_visibility().map(|e| e.disposition),
            Some(VisibilityAttemptDisposition::NotCalled)
        );
    }

    #[test]
    fn visibility_evidence_matches_exact_authorization_only() {
        let normalized = PropertyAttempt::from_visibility_attempt(&visibility_last(
            VisibilitySetter::Accepted,
            true,
        ));
        let evidence = normalized.as_visibility().expect("visibility arm");
        assert!(visibility_evidence_matches(
            evidence,
            &window("win-v"),
            true,
            7
        ));
        assert!(!visibility_evidence_matches(
            evidence,
            &window("win-v"),
            false,
            7
        ));
        assert!(!visibility_evidence_matches(
            evidence,
            &window("win-v"),
            true,
            8
        ));
        assert!(!visibility_evidence_matches(
            evidence,
            &window("win-other"),
            true,
            7
        ));
    }

    #[test]
    fn wrong_kind_rows_never_satisfy_visibility_retention() {
        let attempts = vec![
            state(),
            clear("op-c", "win-v", AttemptDisposition::Accepted),
            PropertyAttempt::from_visibility_attempt(&visibility_last(
                VisibilitySetter::Accepted,
                true,
            )),
        ];
        let mut slot = None;
        assert!(retain_matching_attempt(
            &attempts,
            |attempt| {
                attempt.as_visibility().is_some_and(|evidence| {
                    visibility_evidence_matches(evidence, &window("win-v"), true, 7)
                })
            },
            &mut slot,
            RetentionPolicy::FirstMatch,
        ));
        assert!(
            slot.as_ref()
                .and_then(PropertyAttempt::as_visibility)
                .is_some()
        );
        assert!(slot.as_ref().and_then(PropertyAttempt::as_state).is_none());
        // A forged (window/target/revision) tuple never correlates.
        let mut forged = None;
        assert!(!retain_matching_attempt(
            &attempts,
            |attempt| {
                attempt.as_visibility().is_some_and(|evidence| {
                    visibility_evidence_matches(evidence, &window("win-v"), false, 7)
                })
            },
            &mut forged,
            RetentionPolicy::FirstMatch,
        ));
        assert!(forged.is_none());
    }
}

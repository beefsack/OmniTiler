//! Core-owned frame anti-fighting policy (stage 3a).
//!
//! The core compares its desired frame against the observed frame and owns
//! suppression, transient retry backoff, and completion. Adapters report
//! only mechanical write/readback facts ([`FrameAttempt`](crate::world_report::FrameAttempt));
//! they never send a clamp/suppress verdict, and a setter success alone
//! never proves geometry completion: only an exact desired/observed match
//! clears.
//!
//! Two narrow lanes, keyed by exact window lifetime:
//!
//! - `clamp`: an accepted setter whose stable readback still mismatches
//!   (the app holds its own size). The identical `(desired, observed)` pair
//!   suppresses until the desired rectangle or the native observation
//!   genuinely changes.
//! - `transient`: a refused/throwing setter (or an unreadable proof).
//!   Retried with bounded backoff (500ms doubling, capped at 5s) against
//!   the neutral adapter [`WorldReport::monotonic_ms`](crate::world_report::WorldReport::monotonic_ms)
//!   clock; any desired/observation change re-arms immediately, and an
//!   unreported clock (`0`) never suppresses.
//!
//! A changed DPI basis across the write is invalidated placement, never a
//! stable clamp: the adapter already reasserts once mechanically, and the
//! core records nothing for that attempt so the next tick retries under
//! fresh state. Unreadable evidence (no readback and no observed frame),
//! minimized lifetimes, unknown lifetimes, and explicit closes record and
//! publish nothing. This module never adopts host geometry and never
//! changes desired frames: suppression only downgrades authority (tiled
//! rows publish [`HoldReason::ConstraintClamp`](crate::world_report::HoldReason::ConstraintClamp),
//! explicit float rows withhold their one-shot token), decided in
//! [`crate::world_publish`].

use crate::directional::WindowId;
use crate::geometry::Rect;
use crate::world_engine::WorldEngine;
use crate::world_report::{FrameAttempt, FrameSetter, WorldReport};

/// Backoff base for transient setter failures.
pub(crate) const FRAME_TRANSIENT_BASE_MS: u64 = 500;
/// Backoff cap for transient setter failures.
pub(crate) const FRAME_TRANSIENT_MAX_MS: u64 = 5000;

/// Retained per-window frame policy memory: the last clamp/transient pair
/// plus the transient failure count and the attempt stamp of the last
/// consumed failure. Bounded by the window inventory itself (pruned in
/// [`crate::world_engine`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FrameMemory {
    pub(crate) desired: Rect,
    pub(crate) observed: Rect,
    pub(crate) failures: u32,
    pub(crate) last_ms: u64,
    pub(crate) clamped: bool,
}

/// Bounded backoff for `failures` consecutive transient failures on one
/// pair: 500ms doubling, capped at 5s. Matches the historical native lane
/// exactly (first failure waits 500ms).
#[must_use]
pub(crate) fn transient_backoff_ms(failures: u32) -> u64 {
    (FRAME_TRANSIENT_BASE_MS << failures.min(4)).min(FRAME_TRANSIENT_MAX_MS)
}

/// True when both DPI sides read and disagree: invalidated placement.
/// Unreadable DPI on either side never guesses a transition.
#[must_use]
pub(crate) const fn dpi_basis_changed(before: Option<u32>, after: Option<u32>) -> bool {
    matches!((before, after), (Some(b), Some(a)) if b != a)
}

impl FrameMemory {
    /// `true` when the current `(desired, observed)` pair is suppressed:
    /// a stable app-clamp, or a transient failure still inside its backoff
    /// window. Any desired or observation change re-arms immediately; an
    /// unreported clock never suppresses the transient lane.
    #[must_use]
    pub(crate) fn suppressed(&self, desired: &Rect, observed: &Rect, now_ms: u64) -> bool {
        if self.desired != *desired || self.observed != *observed {
            return false;
        }
        if self.clamped {
            return true;
        }
        if self.last_ms == 0 || now_ms == 0 {
            return false;
        }
        now_ms.saturating_sub(self.last_ms) < transient_backoff_ms(self.failures)
    }
}

/// Resolve the observed key for one attempt: the immediate readback where
/// readable, else the current staged fact. `None` means unreadable
/// evidence: the core records nothing for the attempt.
#[must_use]
pub(crate) fn attempt_observed(attempt: &FrameAttempt, fact: Option<Rect>) -> Option<Rect> {
    attempt.readback.or(fact)
}

/// Fold one mechanical frame attempt into retained memory. Returns the new
/// memory (`None` = clear the entry):
///
/// - resolved observed equal to target clears both lanes (completion is
///   desired == observed, never setter success; the staged fact resolves
///   an unreadable post-write proof exactly like a readable one);
/// - unreadable evidence (no readback and no staged frame) records
///   nothing;
/// - a stale redelivery (same pair at or before the consumed stamp)
///   changes nothing, so a lost acknowledgement never renews backoff;
/// - an accepted setter with a changed DPI basis records nothing
///   (invalidated placement retries under fresh state);
/// - an accepted setter with an immediately readable mismatch pins the
///   clamp lane; an accepted setter with unreadable post-write proof is a
///   transient like any refused/throwing setter (keyed on the staged
///   fact), never a permanent clamp;
/// - a refused/throwing setter (or an unreadable proof) advances the
///   bounded transient lane for the pair, resetting the count when the
///   pair changed.
///
/// `fact` is the current staged observed frame for the window (`None` when
/// unreadable). `minimized`/`unknown` lifetimes must be filtered by the
/// caller: this function only resolves evidence.
#[must_use]
pub(crate) fn consume_attempt(
    existing: Option<FrameMemory>,
    attempt: &FrameAttempt,
    fact: Option<Rect>,
) -> Option<FrameMemory> {
    let observed = attempt_observed(attempt, fact)?;
    if observed == attempt.target {
        return None;
    }
    if let Some(memory) = existing.clone()
        && memory.desired == attempt.target
        && memory.observed == observed
        && attempt.attempt_ms <= memory.last_ms
    {
        return existing;
    }
    let same_pair = existing
        .as_ref()
        .is_some_and(|memory| memory.desired == attempt.target && memory.observed == observed);
    // A changed DPI basis invalidates the placement for accepted setters:
    // record nothing, never a stable clamp; the next tick retries under
    // fresh state.
    if attempt.setter == FrameSetter::Accepted
        && dpi_basis_changed(attempt.dpi_before, attempt.dpi_after)
    {
        return existing;
    }
    // Only an immediately readable mismatch pins the clamp lane. Without a
    // post-write proof the attempt cannot distinguish an app-held size
    // from a setter that never landed, so it rides the transient lane
    // even when the setter reported success.
    let readable_mismatch = attempt
        .readback
        .is_some_and(|readback| readback != attempt.target);
    match attempt.setter {
        FrameSetter::Accepted if readable_mismatch => Some(FrameMemory {
            desired: attempt.target,
            observed,
            failures: existing.as_ref().map_or(
                0,
                |memory| {
                    if same_pair { memory.failures } else { 0 }
                },
            ),
            last_ms: existing.as_ref().map_or(attempt.attempt_ms, |memory| {
                memory.last_ms.max(attempt.attempt_ms)
            }),
            clamped: true,
        }),
        FrameSetter::Accepted | FrameSetter::Refused | FrameSetter::Threw => Some(FrameMemory {
            desired: attempt.target,
            observed,
            failures: existing.as_ref().map_or(0, |memory| {
                if same_pair {
                    memory.failures.saturating_add(1)
                } else {
                    0
                }
            }),
            last_ms: attempt.attempt_ms,
            clamped: existing
                .as_ref()
                .is_some_and(|memory| same_pair && memory.clamped),
        }),
    }
}

/// True when `attempt` is a lost-ack redelivery of already-consumed
/// evidence for one lifetime, and must not mutate suppression memory:
///
/// - an older attempt stamp (`ms != 0` and strictly below the consumed
///   stamp) is stale regardless of pair: reports are consumed in order
///   over a non-decreasing adapter clock, so an older stamp can only be
///   a replay; it never clobbers newer authority;
/// - an exact duplicate of the consumed attempt (all mechanical fields
///   plus stamp equal) is the same write reported twice, even at an
///   equal stamp.
///
/// Deliberately narrow: attempts with an unreported clock (`ms == 0`)
/// never dedup (no ordering claim is possible, and same-tick duplicates
/// are routine there); a changed pair at an equal-or-newer stamp always
/// processes, since time is not a unique attempt identity and the pair
/// change itself is the new evidence. Dropping a same-stamp duplicate
/// only shortens backoff growth, never extends suppression.
#[must_use]
pub(crate) fn is_stale_replay(prior: &FrameAttempt, attempt: &FrameAttempt) -> bool {
    if attempt.attempt_ms == 0 {
        return false;
    }
    attempt.attempt_ms < prior.attempt_ms || attempt == prior
}

impl WorldEngine {
    /// Settle mechanical frame attempts against the staged observation, then
    /// prune memory to live lifetimes. Runs after staging (needs staged
    /// facts for lifetime scoping and observed frames) and before the
    /// sweep/publish below, so suppression applies to this same reply.
    /// Attempts for unknown or minimized lifetimes record nothing;
    /// unreadable evidence records nothing. Lost-ack redeliveries (older
    /// stamp, or exact duplicate) are ignored via the retained consumed
    /// watermark, which survives exact-match clears: a replay never
    /// resurrects stale suppression nor clobbers newer authority. Both
    /// maps prune under one predicate (staged fact or retained member),
    /// so a watermark-only entry prunes exactly when suppression memory
    /// would.
    pub(crate) fn settle_frames(&mut self, report: &WorldReport) {
        for attempt in &report.frames {
            let (fact_rect, minimized) = match self.staged.facts.get(&attempt.window) {
                Some(fact) => (fact.rect, fact.minimized),
                None => continue,
            };
            if minimized {
                continue;
            }
            if self
                .consumed_frames
                .get(&attempt.window)
                .is_some_and(|prior| is_stale_replay(prior, attempt))
            {
                continue;
            }
            let existing = self.frames.get(&attempt.window).cloned();
            match consume_attempt(existing, attempt, fact_rect) {
                Some(memory) => {
                    self.frames.insert(attempt.window.clone(), memory);
                }
                None => {
                    self.frames.remove(&attempt.window);
                }
            }
            if attempt.attempt_ms != 0 {
                self.consumed_frames
                    .insert(attempt.window.clone(), attempt.clone());
            }
        }
        for id in &report.closed {
            self.frames.remove(id);
            self.consumed_frames.remove(id);
        }
        let doomed: Vec<WindowId> = self
            .frames
            .keys()
            .chain(self.consumed_frames.keys())
            .filter(|window| {
                !self.staged.facts.contains_key(window) && !self.members.contains_key(window)
            })
            .cloned()
            .collect();
        for window in doomed {
            self.frames.remove(&window);
            self.consumed_frames.remove(&window);
        }
    }

    /// True when the core suppresses a frame write for this exact
    /// `(desired, observed)` pair: a stable clamp or a transient failure
    /// still inside backoff. `None` observed never suppresses.
    #[must_use]
    pub(crate) fn frame_suppressed(
        &self,
        window: &WindowId,
        desired: &Rect,
        observed: Option<Rect>,
        now_ms: u64,
    ) -> bool {
        let Some(current) = observed else {
            return false;
        };
        self.frames
            .get(window)
            .is_some_and(|memory| memory.suppressed(desired, &current, now_ms))
    }
}

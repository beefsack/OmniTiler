//! Core-owned frame anti-fighting policy (stages 3a-3b).
//!
//! The core compares its desired frame against the observed frame and owns
//! suppression, transient retry backoff, completion, and conservative
//! learned minima. Adapters report only mechanical write/readback facts
//! ([`FrameAttempt`](crate::world_report::FrameAttempt)); they never send a
//! clamp/suppress verdict, and a setter success alone never proves geometry
//! completion: only an exact desired/observed match clears.
//!
//! Two suppression lanes plus one persistent floor, all keyed by exact
//! window lifetime:
//!
//! - `clamp`: an accepted setter with readable proof whose later observation
//!   still mismatches.
//!   The identical `(desired, observed)` pair suppresses until either side
//!   genuinely changes.
//! - `transient`: a refused/throwing setter (or an unreadable proof).
//!   Retried with bounded backoff (500ms doubling, capped at 5s) against
//!   the neutral adapter clock; any change re-arms, and an unreported
//!   clock (`0`) never suppresses.
//! - `learned`: proven app-held growth merges into the normalized hint
//!   path, so the next layout actually grows (applied in
//!   [`crate::world_model`]). A later stable eligible smaller observation
//!   disproves the floor and clears that axis; unrelated higher changes
//!   and exact arrivals never erase it.
//!
//! Evidence rules: unreadable evidence, minimized lifetimes, unknown
//! lifetimes, and explicit closes record nothing. Unknown DPI on either
//! side means no known transition (learning proceeds); a changed DPI basis
//! is invalidated placement and records nothing. Attempts without a
//! freshness stamp (`attempt_ms == 0`) never learn: with no ordering claim
//! the replay contract cannot tell fresh growth from redelivery.
//! Learning and counter-evidence both need a genuinely controlled tiled
//! row. Vetoed: minimized, floating, fullscreen, maximized, fit-excluded,
//! sticky, fixed-auto/suppress flags, missing or unreadable facts, unknown
//! homing, unreadable domains, floating tiling modes, incomplete
//! inventory, and gesture barriers.

use crate::bounds::GEOMETRY_BOUND;
use crate::directional::WindowId;
use crate::geometry::Rect;
use crate::size_hints::WindowSizeHints;
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

/// Resolve the observed key for one attempt: the current staged fact where
/// readable, else the immediate readback. The staged fact is the later
/// stable observation (taken after the previous write had time to settle),
/// while the readback is the synchronous post-write proof; when they
/// disagree the stable fact owns the pair, so a premature exact readback
/// never clears a still-mismatched window. `None` means unreadable
/// evidence: the core records nothing for the attempt.
#[must_use]
pub(crate) fn attempt_observed(attempt: &FrameAttempt, fact: Option<Rect>) -> Option<Rect> {
    fact.or(attempt.readback)
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
/// - an accepted setter with readable post-write proof and a stable
///   mismatch pins the clamp lane (synchronous readback mismatch, or an
///   exact synchronous readback with a later stable staged fact holding a
///   larger size); an accepted setter with unreadable post-write proof is a
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
    // A readable post-write proof plus a stable mismatch pins the clamp
    // lane. The proof shows the write dispatched; the staged fact (preferred
    // above) is the later stable size, covering both the synchronous shape
    // (readback already mismatched) and the async shape (exact synchronous
    // readback, stable fact drifted larger). Without a post-write proof the
    // attempt cannot distinguish an app-held size from a setter that never
    // landed, so it rides the transient lane even when the setter reported
    // success.
    let proven_dispatch = attempt.readback.is_some();
    match attempt.setter {
        FrameSetter::Accepted if proven_dispatch => Some(FrameMemory {
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

/// Retained conservative learned minimum per exact window lifetime:
/// proven app-held growth sizes feeding the size-hint/layout path.
/// Separate from pair suppression: exact matches clear suppression memory
/// but never a learned bound. Only exact-lifetime end (explicit close,
/// complete-inventory departure, incarnation reset, new generation) or
/// later stable eligible smaller evidence prunes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct LearnedMin {
    pub(crate) min_w: Option<i32>,
    pub(crate) min_h: Option<i32>,
}

/// Per-axis proven growth from one stable accepted mismatch: `Some` on an
/// axis only when the observed frame keeps the requested origin, never
/// shrinks on either axis, and strictly grows on that axis. Position drift
/// never learns; any shrink on either axis vetoes the whole attempt.
#[must_use]
pub(crate) fn learnable_growth(target: &Rect, observed: &Rect) -> (Option<i32>, Option<i32>) {
    if observed.x != target.x || observed.y != target.y {
        return (None, None);
    }
    if observed.w < target.w || observed.h < target.h {
        return (None, None);
    }
    if observed.w == target.w && observed.h == target.h {
        return (None, None);
    }
    let w = (observed.w > target.w).then_some(observed.w);
    let h = (observed.h > target.h).then_some(observed.h);
    (w, h)
}

/// Meaningful learned bound guard: positive and inside the shared
/// carried-geometry bound, exactly like declared hint sanitizing.
fn meaningful_learned(value: i32) -> bool {
    (1..=GEOMETRY_BOUND).contains(&value)
}

impl WorldEngine {
    /// Merge retained learned minima into one declared hint vector: a
    /// learned axis applies only when meaningful, above any meaningful
    /// declared minimum, and within any meaningful declared maximum
    /// (contradictions keep the declared pair). Infeasible minima keep the
    /// proportional allocation and flag overconstrained downstream, never
    /// refuse.
    #[must_use]
    pub(crate) fn effective_hints(
        &self,
        window: &WindowId,
        declared: WindowSizeHints,
    ) -> WindowSizeHints {
        let Some(learned) = self.learned_mins.get(window) else {
            return declared;
        };
        let mut out = declared;
        if let Some(lw) = learned.min_w
            && meaningful_learned(lw)
            && declared.meaningful_max_w().is_none_or(|mx| lw <= mx)
            && declared.meaningful_min_w().is_none_or(|mn| lw > mn)
        {
            out.min_w = Some(lw);
        }
        if let Some(lh) = learned.min_h
            && meaningful_learned(lh)
            && declared.meaningful_max_h().is_none_or(|mx| lh <= mx)
            && declared.meaningful_min_h().is_none_or(|mn| lh > mn)
        {
            out.min_h = Some(lh);
        }
        out
    }

    /// Shared row eligibility for learning and counter-evidence: a readable
    /// fact on a genuinely controlled tiled row under complete inventory
    /// without a gesture barrier. Vetoes minimized, floating, fullscreen,
    /// maximized, fit-excluded, sticky and fixed-auto/suppress flags, plus
    /// missing/unreadable facts, unknown homing, unreadable domains and
    /// floating tiling modes. Neither growth nor DPI is judged here.
    fn eligible_fact(&self, report: &WorldReport, window: &WindowId) -> Option<Rect> {
        if !report.inventory_complete || self.staged.gesture_barrier {
            return None;
        }
        let fact = self.staged.facts.get(window)?;
        if fact.minimized || fact.floating || fact.fullscreen || fact.maximized {
            return None;
        }
        if fact.fit_excluded || fact.sticky || fact.fixed_auto || fact.fixed_suppress {
            return None;
        }
        let rect = fact.rect?;
        let key = self.staged.homing.get(window)?;
        let domain = self
            .staged
            .domains
            .iter()
            .find(|staged| &staged.key == key)?;
        if !domain.readable {
            return None;
        }
        if self
            .tiling
            .get(key)
            .is_some_and(|mode| *mode != crate::world_report::TilingMode::Tiled)
        {
            return None;
        }
        Some(rect)
    }

    /// True when one frame attempt proves a learned minimum: fresh accepted
    /// write with a freshness stamp, stable DPI, staged fact corroborating
    /// the dispatch, same origin, no shrinking axis, at least one growing
    /// axis, eligible row, and no declared-maximum contradiction. Corroboration
    /// is either the stable synchronous shape (staged fact matches readback)
    /// or the async shape (readback proves the write landed at target while
    /// the later staged fact holds the larger stable size). Stale
    /// redeliveries never reach here (skipped first, like suppression).
    /// No just-learned protection needed: counter-evidence runs first
    /// against the prior floor, and learning then takes the per-axis
    /// maximum over it.
    fn learnable_attempt(
        &self,
        report: &WorldReport,
        attempt: &FrameAttempt,
        readback: &Rect,
    ) -> Option<(Option<i32>, Option<i32>)> {
        if attempt.setter != FrameSetter::Accepted || attempt.attempt_ms == 0 {
            return None;
        }
        if dpi_basis_changed(attempt.dpi_before, attempt.dpi_after) {
            return None;
        }
        let rect = self.eligible_fact(report, &attempt.window)?;
        // Stable corroboration: the staged fact matches the readback
        // (synchronous clamp), or the readback proves the write landed at
        // target and the later staged fact is the stable size (async clamp
        // settles after the synchronous proof). Anything else is not fresh
        // proof of an app-held size.
        let stable = if rect == *readback {
            readback
        } else if *readback == attempt.target {
            &rect
        } else {
            return None;
        };
        let fact = self.staged.facts.get(&attempt.window)?;
        let (mut grow_w, mut grow_h) = learnable_growth(&attempt.target, stable);
        if grow_w.is_none() && grow_h.is_none() {
            return None;
        }
        // Declared-maximum contradiction per axis: past a meaningful cap a
        // size proves nothing portable.
        if let Some(w) = grow_w
            && (!meaningful_learned(w) || fact.hints.meaningful_max_w().is_some_and(|mx| w > mx))
        {
            grow_w = None;
        }
        if let Some(h) = grow_h
            && (!meaningful_learned(h) || fact.hints.meaningful_max_h().is_some_and(|mx| h > mx))
        {
            grow_h = None;
        }
        if grow_w.is_none() && grow_h.is_none() {
            return None;
        }
        Some((grow_w, grow_h))
    }

    /// Fold one proven growth pair into the retained bound: per-axis maximum.
    fn retain_learned(&mut self, window: &WindowId, growth: (Option<i32>, Option<i32>)) {
        let entry = self.learned_mins.entry(window.clone()).or_default();
        if let Some(w) = growth.0 {
            entry.min_w = Some(entry.min_w.map_or(w, |cur| cur.max(w)));
        }
        if let Some(h) = growth.1 {
            entry.min_h = Some(entry.min_h.map_or(h, |cur| cur.max(h)));
        }
    }

    /// Drop disproven floors: a stable eligible observation smaller than a
    /// learned minimum on one axis clears that axis. Same row eligibility
    /// as learning. Runs BEFORE attempt consumption/learning against the
    /// prior watermark, so an existing smaller current disproves a prior
    /// higher floor before fresh growth in the same report establishes a
    /// new one. Attempts already consumed (exact redeliveries or older
    /// stamps) and clockless attempts (`ms == 0`, no ordering claim) never
    /// veto: they are skipped, not treated as fresh counter-proof.
    /// Otherwise skipped per window on any fresh refused/thrown or
    /// DPI-flipped current attempt (transient preimage, not external
    /// proof); per axis when the current accepted target already undercuts
    /// it (impossible-fit request) or a declared maximum contradicts it
    /// (the max explains smallness). Suppression memory never vetoes: an
    /// old clamp cannot defeat fresh smaller evidence.
    fn invalidate_learned(&mut self, report: &WorldReport) {
        let snapshot: Vec<(WindowId, LearnedMin)> = self
            .learned_mins
            .iter()
            .map(|(window, learned)| (window.clone(), *learned))
            .collect();
        for (window, learned) in snapshot {
            if learned.min_w.is_none() && learned.min_h.is_none() {
                continue;
            }
            let mut undercut_w = false;
            let mut undercut_h = false;
            let mut tainted = false;
            for attempt in report
                .frames
                .iter()
                .filter(|attempt| attempt.window == window)
            {
                // No ordering claim: clockless attempts never veto fresh
                // observation-only counter-evidence (nor prove anything).
                if attempt.attempt_ms == 0 {
                    continue;
                }
                // Redelivery of already-consumed evidence: not fresh proof.
                // Reads the prior watermark only; this runs before the loop
                // below consumes the current report.
                if self
                    .consumed_frames
                    .get(&window)
                    .is_some_and(|prior| is_stale_replay(prior, attempt))
                {
                    continue;
                }
                if attempt.setter != FrameSetter::Accepted
                    || dpi_basis_changed(attempt.dpi_before, attempt.dpi_after)
                {
                    tainted = true;
                    break;
                }
                undercut_w |= learned.min_w.is_some_and(|floor| attempt.target.w < floor);
                undercut_h |= learned.min_h.is_some_and(|floor| attempt.target.h < floor);
            }
            if tainted {
                continue;
            }
            let Some(rect) = self.eligible_fact(report, &window) else {
                continue;
            };
            let hints = self
                .staged
                .facts
                .get(&window)
                .map(|fact| fact.hints)
                .unwrap_or_else(WindowSizeHints::none);
            let conflict_w = learned
                .min_w
                .is_some_and(|floor| hints.meaningful_max_w().is_some_and(|cap| cap < floor));
            let conflict_h = learned
                .min_h
                .is_some_and(|floor| hints.meaningful_max_h().is_some_and(|cap| cap < floor));
            let clear_w =
                !undercut_w && !conflict_w && learned.min_w.is_some_and(|floor| rect.w < floor);
            let clear_h =
                !undercut_h && !conflict_h && learned.min_h.is_some_and(|floor| rect.h < floor);
            if !clear_w && !clear_h {
                continue;
            }
            if let Some(entry) = self.learned_mins.get_mut(&window) {
                if clear_w {
                    entry.min_w = None;
                }
                if clear_h {
                    entry.min_h = None;
                }
                if entry.min_w.is_none() && entry.min_h.is_none() {
                    self.learned_mins.remove(&window);
                }
            }
        }
    }
    /// Settle mechanical frame attempts against the staged observation:
    /// first drop floors disproven by fresh smaller evidence (prior
    /// watermark only), then consume attempts, learn new floors, and prune
    /// memory to live lifetimes. Runs after staging (needs staged facts for
    /// lifetime scoping and observed frames) and before the sweep/publish
    /// below, so suppression and learned floors apply to this same reply.
    /// Attempts for unknown or minimized lifetimes record nothing;
    /// unreadable evidence records nothing. Lost-ack redeliveries (older
    /// stamp, or exact duplicate) are ignored via the retained consumed
    /// watermark, which survives exact-match clears: a replay never
    /// resurrects stale suppression nor clobbers newer authority. All three
    /// maps prune under one predicate (staged fact or retained member).
    pub(crate) fn settle_frames(&mut self, report: &WorldReport) {
        // Counter-evidence first, against the prior watermark only: still
        // narrower than the post-loop maps, so newly consumed current
        // attempts can never masquerade as redeliveries here. Fresh growth
        // in the loop below may then establish a new floor in the same
        // reply. Filtering here must stay a subset decision (stale or
        // clockless); never consult post-loop state.
        self.invalidate_learned(report);
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
            // Learned minima ride the same fresh-evidence gate but persist:
            // only stable readable accepted growth at the requested origin
            // learns (synchronous shape, or async shape where the readback
            // proves dispatch at target and the later fact holds the size).
            // Counter-evidence already ran above, against the prior
            // floor and watermark.
            if let Some(readback) = attempt.readback
                && fact_rect.is_some()
                && let Some(growth) = self.learnable_attempt(report, attempt, &readback)
            {
                self.retain_learned(&attempt.window, growth);
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
            self.learned_mins.remove(id);
        }
        let doomed: Vec<WindowId> = self
            .frames
            .keys()
            .chain(self.consumed_frames.keys())
            .chain(self.learned_mins.keys())
            .filter(|window| {
                !self.staged.facts.contains_key(window) && !self.members.contains_key(window)
            })
            .cloned()
            .collect();
        for window in doomed {
            self.frames.remove(&window);
            self.consumed_frames.remove(&window);
            self.learned_mins.remove(&window);
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

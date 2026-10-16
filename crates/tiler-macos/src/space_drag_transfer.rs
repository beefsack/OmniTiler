#![forbid(unsafe_code)]

//! Owned-helper synthetic titlebar-drag + native switch-chord transfer.
//!
//! Approved route (user manual check 2026-10-16): hold a title-bar drag on
//! an owned helper window, post the native relative Space-switch chord
//! (enabled `Ctrl+Left`/`Ctrl+Right`), release. Membership readback proves
//! the transfer; a drag-back leg is the exact inverse. Numbered Desktop-N
//! chords stay out: their bindings are unresolved (no symbolic IDs), so
//! this unit is arrows-only by refusal, never a guess.
//!
//! Silica parity (local checkout `Silica/SIWindow.m moveToSpaceWithEvent:`):
//! move-to-titlebar -> left-down -> left-dragged (grab) -> 50 ms grab settle ->
//! chord down/up -> 400 ms animation hold -> left-up -> bounded effect poll.
//! Mouse posts go to the HID tap, chord posts to HID too (Silica parity)
//! with the own-switch provenance tag
//! ([`crate::input_match::OWN_SWITCH_USER_DATA`]) so the tap passes our own
//! switch chords while still consuming the user's. Tile-catalogue posts
//! stay untagged. The anchor is top-center titlebar like Silica's toolbar
//! point; full anchor parity is not claimed (Silica derives from the
//! minimize button, we derive from the owned frame).
//!
//! Structure: ONE generic executor ([`run_drag_leg`], [`run_drag_roundtrip`])
//! over [`DragBackend`], shared by the fake tests
//! ([`space_drag_transfer_tests`]) and the live command
//! ([`crate::space_drag_live`]). Errors are typed ([`LegError`]); the
//! roundtrip matches variants, never message strings. Once mouse-down
//! posts, every exit releases the mouse first: the pending key edge
//! completes exactly once before release, and release failures park as
//! residue.
//!
//! Timing contract ([`GRAB_SETTLE_MS`], [`ANIMATION_HOLD_MS`],
//! [`EFFECT_SETTLE_MS`]): the executor sleeps through the backend's
//! settle hooks (live: real sleeps; fake: order-logged, no sleep) and polls
//! membership + current AFTER mouse release. Nothing here waits unbounded:
//! synthetic legs additionally spend the caller's `ActionBudget` in the
//! live runner.

use crate::ffi::AxFrame;

/// Minimum owned-helper frame for a reliable titlebar grab (points).
const MIN_GRAB_W: f64 = 60.0;
const MIN_GRAB_H: f64 = 24.0;
/// Vertical offset below the frame top landing on the titlebar (AX top-left
/// origin).
const TITLEBAR_OFFSET: f64 = 8.0;

/// Grab settle after the dragged event before the chord (Silica parity:
/// 50 ms to make sure the window is grabbed).
pub const GRAB_SETTLE_MS: u64 = 50;
/// Animation hold after the chord while the mouse is still held (Silica
/// parity: 400 ms to finish the Space transition before release).
pub const ANIMATION_HOLD_MS: u64 = 400;
/// Post-release effect settle: exact membership + current readback poll
/// window (parity with the switch-probe and recovery settles).
pub const EFFECT_SETTLE_MS: u64 = 3_000;
/// Effect poll slice.
pub const EFFECT_POLL_MS: u64 = 200;

/// Minimum run budget: independent recovery plus two drag legs plus cleanup.
pub const MIN_DRAG_SECS: u64 = 60;
/// Maximum run budget: the helper lifetime cap.
pub const MAX_DRAG_SECS: u64 = crate::config::MAX_PROBE_SECONDS;

/// Pure argument gate: explicit opt-in, two distinct nonzero SIDs, budget
/// inside `60..=120`. No live touch.
pub fn validate_drag_args(
    original: u64,
    temp: u64,
    seconds: u64,
    allow_untested: bool,
) -> Result<u64, String> {
    if !allow_untested {
        return Err(
            "spaces-drag-transfer requires --allow-untested (synthetic titlebar drag plus visible Space switches; recovery is observer-verified per run, never assumed)"
                .to_owned(),
        );
    }
    if original == 0 || temp == 0 {
        return Err("spaces-drag-transfer needs nonzero --original and --temp SIDs".to_owned());
    }
    if original == temp {
        return Err("spaces-drag-transfer needs distinct --original and --temp SIDs".to_owned());
    }
    crate::config::ProbeConfig::validate_probe_seconds(seconds)?;
    if seconds < MIN_DRAG_SECS {
        return Err(format!(
            "spaces-drag-transfer needs --seconds >= {MIN_DRAG_SECS} (independent recovery plus two drag legs plus cleanup); refusing before any mutation"
        ));
    }
    Ok(seconds)
}

/// Pure argument gate for `--temporary` mode (the temp Space is created
/// internally, so no temp SID exists yet): explicit opt-in, nonzero
/// original, budget inside `60..=120`. No live touch.
pub fn validate_drag_args_temporary(
    original: u64,
    seconds: u64,
    allow_untested: bool,
) -> Result<u64, String> {
    if !allow_untested {
        return Err(
            "spaces-drag-transfer requires --allow-untested (synthetic titlebar drag plus visible Space switches; recovery is observer-verified per run, never assumed)"
                .to_owned(),
        );
    }
    if original == 0 {
        return Err("spaces-drag-transfer needs nonzero --original".to_owned());
    }
    crate::config::ProbeConfig::validate_probe_seconds(seconds)?;
    if seconds < MIN_DRAG_SECS {
        return Err(format!(
            "spaces-drag-transfer needs --seconds >= {MIN_DRAG_SECS} (create plus independent recovery plus two drag legs plus remove plus cleanup); refusing before any mutation"
        ));
    }
    Ok(seconds)
}

/// Pure single-Space baseline gate for `--temporary` mode over plain data:
/// the chosen display must read exactly `[original]` with the current on
/// the original, so the internally created temp is the only adoption and
/// its later removal restores this exact shape. Anything else (extra
/// Spaces, drifted current, unknown display) refuses before any mutation.
/// No live touch.
pub fn require_single_space_baseline(
    snapshot: &crate::mc_probe::TopologySnapshot,
    display: usize,
    original: u64,
) -> Result<(), String> {
    let row = snapshot.displays.get(display).ok_or_else(|| {
        format!(
            "temporary baseline: display {display} unknown ({} displays); refusing before any mutation",
            snapshot.displays.len()
        )
    })?;
    if row.spaces != vec![original] {
        return Err(format!(
            "temporary baseline: display {display} Spaces are {:?}, want exactly [{original}] (one preexisting Space only); refusing before any mutation",
            row.spaces,
        ));
    }
    if row.current != Some(original) {
        return Err(format!(
            "temporary baseline: display {display} current is {:?}, want {original}; refusing before any mutation",
            row.current,
        ));
    }
    Ok(())
}

/// One synthetic drag step in Silica order. The chord pair travels while
/// the mouse is held; `MouseUp` is always last.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragStep {
    MouseMove,
    MouseDown,
    MouseDragged,
    ChordDown,
    ChordUp,
    MouseUp,
}

/// Pure titlebar grab point from the owned AX frame (AX top-left origin):
/// horizontal center, [`TITLEBAR_OFFSET`] below the frame top. Refuses
/// non-finite, non-positive, or below-minimum frames before any post. No
/// live touch.
pub fn titlebar_grab_point(frame: AxFrame) -> Result<(f64, f64), String> {
    if !frame.x.is_finite() || !frame.y.is_finite() || !frame.w.is_finite() || !frame.h.is_finite()
    {
        return Err("grab refused: non-finite frame".to_owned());
    }
    if frame.w < MIN_GRAB_W || frame.h < MIN_GRAB_H {
        return Err(format!(
            "grab refused: frame {}x{} below minimum {}x{} (no reliable titlebar hit)",
            frame.w, frame.h, MIN_GRAB_W, MIN_GRAB_H
        ));
    }
    Ok((frame.x + frame.w / 2.0, frame.y + TITLEBAR_OFFSET))
}

/// Pure candidate titlebar points across the owned AX frame (AX top-left
/// origin): five fractions (0.2/0.35/0.5/0.65/0.8) at [`TITLEBAR_OFFSET`]
/// below the frame top. The fixed center alone can sit under unrelated
/// above-normal chrome while an edge stays provably clear; the live runner
/// tries these in order and posts at the first point passing BOTH the
/// conservative CG occlusion check and the fresh AX hit identity, refusing
/// when none is clear (never exempting chrome). No live touch.
pub fn candidate_grab_points(frame: AxFrame) -> Result<Vec<(f64, f64)>, String> {
    if !frame.x.is_finite() || !frame.y.is_finite() || !frame.w.is_finite() || !frame.h.is_finite()
    {
        return Err("grab refused: non-finite frame".to_owned());
    }
    if frame.w < MIN_GRAB_W || frame.h < MIN_GRAB_H {
        return Err(format!(
            "grab refused: frame {}x{} below minimum {}x{} (no reliable titlebar hit)",
            frame.w, frame.h, MIN_GRAB_W, MIN_GRAB_H
        ));
    }
    const FRACTIONS: [f64; 5] = [0.2, 0.35, 0.5, 0.65, 0.8];
    Ok(FRACTIONS
        .iter()
        .map(|f| (frame.x + frame.w * f, frame.y + TITLEBAR_OFFSET))
        .collect())
}

/// Pure clear-point selector over plain data: first candidate passing the
/// order-free CG [`hit_test_grab`] plus, when `hit_owner` is `Some`, the AX
/// [`hit_test_identity`] against `want`. Returns the point; refuses when no
/// candidate is provably clear (naming the tried count plus the last refusal,
/// never exempting chrome). No live touch; the live runner supplies fresh
/// snapshots and the actual AX owner per candidate.
pub fn select_clear_grab_point(
    frame: AxFrame,
    owned: &crate::ffi::CgWindow,
    all: &[crate::ffi::CgWindowBasic],
    ghostty: &[crate::mc_probe::GhosttyWindow],
    hit_owner: Option<(u32, u32)>,
    want: (u32, u32),
) -> Result<(f64, f64), String> {
    let candidates = candidate_grab_points(frame)?;
    let mut last_err = String::from("no candidate evaluated");
    for point in candidates {
        if let Err(e) = hit_test_grab(point, owned, all, ghostty) {
            last_err = e;
            continue;
        }
        if let Some(hit) = hit_owner
            && let Err(e) = hit_test_identity(hit, want)
        {
            last_err = e;
            continue;
        }
        return Ok(point);
    }
    Err(format!(
        "grab refused: no provably clear titlebar point (tried 5; last: {last_err})"
    ))
}

/// Pure AX-frame mismatch detail over plain data (no OS touch): `None` when
/// exactly equal, else the differing fields with values. The live runner
/// logs this when the exact-frame gate reads false, so a sub-point jitter or
/// a real move is distinguishable without weakening the exact gate.
#[must_use]
pub fn ax_frame_mismatch(before: AxFrame, after: AxFrame) -> Option<String> {
    if before == after {
        return None;
    }
    let mut parts = Vec::new();
    if before.x != after.x {
        parts.push(format!("x {} -> {}", before.x, after.x));
    }
    if before.y != after.y {
        parts.push(format!("y {} -> {}", before.y, after.y));
    }
    if before.w != after.w {
        parts.push(format!("w {} -> {}", before.w, after.w));
    }
    if before.h != after.h {
        parts.push(format!("h {} -> {}", before.h, after.h));
    }
    Some(format!("frame differs: {}", parts.join(", ")))
}

/// Whether a point lies inside a rectangle (edges inclusive). Pure;
/// shared by the grab-point containment and hit-test legs.
#[must_use]
pub const fn point_in_rect(px: f64, py: f64, x: f64, y: f64, w: f64, h: f64) -> bool {
    px >= x && px <= x + w && py >= y && py <= y + h
}

/// A posted-but-unreleased drag edge. The run completes it exactly once
/// (same-edge release only) or parks as residue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragPending {
    MouseDownWithoutUp,
    KeyDownWithoutUp,
}

impl DragPending {
    /// Report token marking the pending edge -- never a silent flag.
    #[must_use]
    pub fn token(self, reason: &str) -> String {
        match self {
            Self::MouseDownWithoutUp => {
                format!("pending-release: drag mouse-down-without-up refused: {reason}")
            }
            Self::KeyDownWithoutUp => {
                format!("pending-release: drag chord-down-without-up refused: {reason}")
            }
        }
    }
}

/// One leg's typed outcome. Variants -- never message strings -- drive the
/// roundtrip: only `Ok` (exact readback) runs the inverse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LegError {
    /// Refused before effect with nothing pending (setup/grab/chord-down/
    /// readback refusal; the mouse is already released).
    Refused { stage: &'static str, reason: String },
    /// Chord completed and released, but the post-release readback is not
    /// exactly goal membership on the goal current (`None` legs when
    /// unreadable): never repeated, never inverted as a no-op.
    NoEffect {
        observed: Option<Vec<u64>>,
        current: Option<u64>,
        goal: u64,
    },
    /// A posted edge could not be released: completed pairs are evidence
    /// only, and no further leg runs.
    UnresolvedPending {
        pending: DragPending,
        reason: String,
        mouse_released: bool,
    },
    /// The mouse-up release itself failed: the drag may still be held.
    ReleaseFailed { reason: String },
}

impl LegError {
    /// One-line evidence for notes/reports (formatting only; control flow
    /// matches on variants).
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Self::Refused { stage, reason } => format!("refused at {stage}: {reason}"),
            Self::NoEffect {
                observed,
                current,
                goal,
            } => format!(
                "no effect: membership {observed:?} current {current:?}, want exactly [{goal}] on current {goal}"
            ),
            Self::UnresolvedPending {
                pending,
                reason,
                mouse_released,
            } => format!("{}; mouse_released={mouse_released}", pending.token(reason)),
            Self::ReleaseFailed { reason } => {
                format!("mouse release failed (drag may be held): {reason}")
            }
        }
    }

    #[must_use]
    pub const fn unresolved(&self) -> bool {
        matches!(
            self,
            Self::UnresolvedPending { .. } | Self::ReleaseFailed { .. }
        )
    }
}

/// Balanced post counters: every down has its up on every path.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
pub struct PostCounts {
    pub mouse_down: u32,
    pub mouse_up: u32,
    pub key_down: u32,
    pub key_up: u32,
}

impl PostCounts {
    #[must_use]
    pub const fn balanced(self) -> bool {
        self.mouse_down == self.mouse_up && self.key_down == self.key_up
    }

    #[must_use]
    pub const fn total(self) -> u32 {
        self.mouse_down
            .saturating_add(self.mouse_up)
            .saturating_add(self.key_down)
            .saturating_add(self.key_up)
    }

    fn delta(before: Self, after: Self) -> Self {
        Self {
            mouse_down: after.mouse_down.saturating_sub(before.mouse_down),
            mouse_up: after.mouse_up.saturating_sub(before.mouse_up),
            key_down: after.key_down.saturating_sub(before.key_down),
            key_up: after.key_up.saturating_sub(before.key_up),
        }
    }
}

/// Successful leg: exact post-release readback plus release evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegOk {
    pub membership: Vec<u64>,
    pub current: u64,
    pub pending_recovered: bool,
    pub counts: PostCounts,
}

/// Drag post backend: the executor calls these in Silica order and reads
/// back membership + current AFTER mouse release. The fake stands in for
/// the WindowServer in tests; the live backend posts guarded HID events.
/// Timing hooks default to the real contract constants; the fake overrides
/// them to log ordering without sleeping.
pub trait DragBackend {
    /// Select this leg's goal before any post (fake: steer the scripted OS
    /// effect; live: select the leg's arrow keycode plus a fresh owned AX
    /// frame, grab point, and hit-test -- refusing here posts nothing).
    fn begin_leg(&mut self, goal: u64) -> Result<(), String>;
    fn post_mouse(&mut self, step: DragStep) -> Result<(), String>;
    fn post_key(&mut self, down: bool) -> Result<(), String>;
    /// Grab settle (50 ms): log-only override point for the fake.
    fn settle_grab(&mut self) {
        std::thread::sleep(std::time::Duration::from_millis(GRAB_SETTLE_MS));
    }
    /// Animation hold (400 ms): log-only override point for the fake.
    fn settle_hold(&mut self) {
        std::thread::sleep(std::time::Duration::from_millis(ANIMATION_HOLD_MS));
    }
    fn read_membership(&mut self) -> Option<Vec<u64>>;
    fn read_current(&mut self) -> Option<u64>;
    /// Post-release poll window (live: 3 s; fake: fast).
    fn effect_deadline_ms(&self) -> u64 {
        EFFECT_SETTLE_MS
    }
    /// Post-release poll slice (live: 200 ms; fake: fast).
    fn effect_poll_ms(&self) -> u64 {
        EFFECT_POLL_MS
    }
    fn counts(&self) -> PostCounts;
}

fn refused(stage: &'static str, reason: String) -> LegError {
    LegError::Refused { stage, reason }
}

/// ONE drag leg over any backend: goal selection, grab, 50 ms settle,
/// chord with once-only pending-key recovery, 400 ms animation hold,
/// guaranteed mouse release on every path after mouse-down, then the
/// bounded post-release poll for exact membership on the goal current.
/// Shared by fake tests and the live run.
pub fn run_drag_leg<B: DragBackend>(
    backend: &mut B,
    goal: u64,
    goal_current: u64,
    leg: &'static str,
    notes: &mut Vec<String>,
) -> Result<LegOk, LegError> {
    use DragStep::{MouseDown, MouseMove, MouseUp};
    backend
        .begin_leg(goal)
        .map_err(|e| refused("leg-setup", format!("{leg} {e}")))?;
    backend
        .post_mouse(MouseMove)
        .map_err(|e| refused("grab-move", format!("{leg} {e}")))?;
    backend
        .post_mouse(MouseDown)
        .map_err(|e| refused("grab-down", format!("{leg} {e}")))?;
    // The mouse is now held: every exit below releases it first, and the
    // effect poll runs only after release.
    let body = drag_body(backend, leg, notes);
    match (body, backend.post_mouse(MouseUp)) {
        (Ok(pending_recovered), Ok(())) => {
            await_effect(backend, goal, goal_current, leg, pending_recovered)
        }
        (
            Err(LegError::UnresolvedPending {
                pending, reason, ..
            }),
            Ok(()),
        ) => Err(LegError::UnresolvedPending {
            pending,
            reason,
            mouse_released: true,
        }),
        (Err(e), Ok(())) => Err(e),
        (body, Err(e)) => {
            let release_note = format!("{leg} mouse-up refused: {e}");
            match body {
                Ok(_) | Err(LegError::Refused { .. }) | Err(LegError::NoEffect { .. }) => {
                    notes.push(release_note.clone());
                    Err(LegError::ReleaseFailed {
                        reason: release_note,
                    })
                }
                Err(LegError::UnresolvedPending {
                    pending, reason, ..
                }) => Err(LegError::UnresolvedPending {
                    pending,
                    reason: format!("{reason}; {release_note}"),
                    mouse_released: false,
                }),
                Err(LegError::ReleaseFailed { .. }) => {
                    unreachable!("drag_body never returns ReleaseFailed")
                }
            }
        }
    }
}

/// Grab completion, grab settle, chord pair with once-only pending-key
/// recovery, and the animation hold. Runs only while the mouse is held;
/// the caller releases the mouse first and polls the effect after.
/// Returns whether a pending key edge was recovered.
fn drag_body<B: DragBackend>(
    backend: &mut B,
    leg: &str,
    notes: &mut Vec<String>,
) -> Result<bool, LegError> {
    backend
        .post_mouse(DragStep::MouseDragged)
        .map_err(|e| refused("grab-dragged", format!("{leg} {e}")))?;
    backend.settle_grab();
    backend
        .post_key(true)
        .map_err(|e| refused("chord-down", format!("{leg} {e}")))?;
    let mut pending_recovered = false;
    if let Err(e) = backend.post_key(false) {
        notes.push(format!(
            "{leg}: {}; completing once",
            DragPending::KeyDownWithoutUp.token(&e)
        ));
        backend
            .post_key(false)
            .map_err(|e2| LegError::UnresolvedPending {
                pending: DragPending::KeyDownWithoutUp,
                reason: format!("{leg} chord-up recovery refused: {e2}"),
                mouse_released: false,
            })?;
        pending_recovered = true;
        notes.push(format!("{leg}: pending chord-up completed once"));
    }
    backend.settle_hold();
    Ok(pending_recovered)
}

/// Bounded post-release poll: exact single-Space membership AND the goal
/// current, read fresh after mouse release. First read is immediate; later
/// reads slice until the deadline. A leg that never reads anything refuses
/// at `readback`; a persistent mismatch is `NoEffect` (never repeated,
/// never inverted).
fn await_effect<B: DragBackend>(
    backend: &mut B,
    goal: u64,
    goal_current: u64,
    leg: &str,
    pending_recovered: bool,
) -> Result<LegOk, LegError> {
    let deadline_ms = backend.effect_deadline_ms();
    let poll_ms = backend.effect_poll_ms().max(1);
    let start = std::time::Instant::now();
    // First read is immediate; later reads slice until the deadline.
    let mut last_membership = backend.read_membership();
    let mut last_current = backend.read_current();
    loop {
        if let (Some(m), Some(c)) = (&last_membership, last_current)
            && crate::space_recovery::membership_is_exact(m, goal)
            && c == goal_current
        {
            return Ok(LegOk {
                membership: m.clone(),
                current: c,
                pending_recovered,
                counts: backend.counts(),
            });
        }
        if start.elapsed() >= std::time::Duration::from_millis(deadline_ms) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(poll_ms));
        last_membership = backend.read_membership();
        last_current = backend.read_current();
    }
    if last_membership.is_none() && last_current.is_none() {
        return Err(refused(
            "readback",
            format!("{leg} membership/current unreadable"),
        ));
    }
    Err(LegError::NoEffect {
        observed: last_membership,
        current: last_current,
        goal,
    })
}

/// Per-leg record for reports (both legs share the shape).
#[derive(Debug, Clone)]
pub struct LegRecord {
    pub leg: &'static str,
    pub ok: bool,
    pub membership: Option<Vec<u64>>,
    pub current: Option<u64>,
    pub pending_recovered: bool,
    pub counts: PostCounts,
    pub error: Option<String>,
    pub unresolved: bool,
}

/// Roundtrip report: outward leg plus the exact drag-back inverse, which
/// runs ONLY after an exact outward readback.
#[derive(Debug, Clone)]
pub struct RoundtripReport {
    pub outward: LegRecord,
    pub inverse: Option<LegRecord>,
    pub outcome: &'static str,
    pub releases_balanced: bool,
    pub unresolved: bool,
    /// Total synthetic posts across both legs (derives
    /// `synthetic_events_posted` in reports).
    pub posts_total: u32,
}

/// Pure terminal outcome: exact memberships on both legs, balanced
/// releases, and no unresolved pending edge are all required for complete;
/// anything else is residue. Never a pretend clean.
#[must_use]
pub const fn drag_transfer_outcome(
    outward_ok: bool,
    inverse_ok: bool,
    releases_balanced: bool,
    unresolved: bool,
) -> &'static str {
    if outward_ok && inverse_ok && releases_balanced && !unresolved {
        "drag-transfer-complete"
    } else {
        "drag-transfer-residue"
    }
}

fn leg_record(
    leg: &'static str,
    before: PostCounts,
    after: PostCounts,
    result: Result<LegOk, LegError>,
    notes: &mut Vec<String>,
) -> LegRecord {
    match result {
        Ok(ok) => LegRecord {
            leg,
            ok: true,
            membership: Some(ok.membership),
            current: Some(ok.current),
            pending_recovered: ok.pending_recovered,
            counts: PostCounts::delta(before, after),
            error: None,
            unresolved: false,
        },
        Err(e) => {
            let unresolved = e.unresolved();
            notes.push(format!("{leg}: {}", e.describe()));
            LegRecord {
                leg,
                ok: false,
                membership: None,
                current: None,
                pending_recovered: false,
                counts: PostCounts::delta(before, after),
                error: Some(e.describe()),
                unresolved,
            }
        }
    }
}

/// ONE roundtrip over any backend: outward first (goal current = temp);
/// the inverse drag-back runs ONLY after an exact outward readback (a
/// failed leg never inverts as a no-op); any unresolved pending halts with
/// no further leg. Shared by fake tests and the live run.
pub fn run_drag_roundtrip<B: DragBackend>(
    backend: &mut B,
    original: u64,
    temp: u64,
    notes: &mut Vec<String>,
) -> RoundtripReport {
    let before = backend.counts();
    let outward_result = run_drag_leg(backend, temp, temp, "drag-o-t", notes);
    let outward = leg_record("drag-o-t", before, backend.counts(), outward_result, notes);
    if !outward.ok {
        let balanced = backend.counts().balanced();
        let unresolved = outward.unresolved;
        return RoundtripReport {
            outward,
            inverse: None,
            outcome: drag_transfer_outcome(false, false, balanced, unresolved),
            releases_balanced: balanced,
            unresolved,
            posts_total: PostCounts::delta(before, backend.counts()).total(),
        };
    }
    let mid = backend.counts();
    let inverse_result = run_drag_leg(backend, original, original, "drag-t-o", notes);
    let inverse = leg_record("drag-t-o", mid, backend.counts(), inverse_result, notes);
    let inverse_ok = inverse.ok;
    let unresolved = outward.unresolved || inverse.unresolved;
    let balanced = backend.counts().balanced();
    RoundtripReport {
        outward,
        inverse: Some(inverse),
        outcome: drag_transfer_outcome(true, inverse_ok, balanced, unresolved),
        releases_balanced: balanced,
        unresolved,
        posts_total: PostCounts::delta(before, backend.counts()).total(),
    }
}

/// Pure occluder detail over plain data (no OS touch): names the first
/// covering entry for `point` with IDs and geometry (never titles or
/// content) so a refusal distinguishes transient chrome (e.g. a banner
/// that disappears between candidates) from an ordinary persistent
/// document frame. Returns `None` when no entry covers the point. This
/// only describes; [`hit_test_grab`] still refuses every class below.
#[must_use]
pub fn occluder_detail(
    point: (f64, f64),
    owned: &crate::ffi::CgWindow,
    all: &[crate::ffi::CgWindowBasic],
    ghostty: &[crate::mc_probe::GhosttyWindow],
) -> Option<String> {
    if let Some(w) = all.iter().find(|w| {
        w.onscreen
            && w.layer > 0
            && point_in_rect(
                point.0, point.1, w.bounds.x, w.bounds.y, w.bounds.w, w.bounds.h,
            )
    }) {
        return Some(format!(
            "above-normal-layer WID{}/PID{} layer {} bounds {}x{}@{},{}",
            w.wid, w.pid, w.layer, w.bounds.w, w.bounds.h, w.bounds.x, w.bounds.y
        ));
    }
    if let Some(w) = all.iter().find(|w| {
        w.onscreen
            && w.layer == 0
            && !(w.pid == owned.pid && w.wid == owned.wid)
            && point_in_rect(
                point.0, point.1, w.bounds.x, w.bounds.y, w.bounds.w, w.bounds.h,
            )
    }) {
        return Some(format!(
            "same-layer WID{}/PID{} bounds {}x{}@{},{}",
            w.wid, w.pid, w.bounds.w, w.bounds.h, w.bounds.x, w.bounds.y
        ));
    }
    ghostty
        .iter()
        .find(|g| g.onscreen && point_in_rect(point.0, point.1, g.x, g.y, g.w, g.h))
        .map(|g| {
            format!(
                "Ghostty WID{}/PID{} bounds {}x{}@{},{}",
                g.wid, g.pid, g.w, g.h, g.x, g.y
            )
        })
}

/// Pure order-free grab hit-test over plain data (the CG list order is
/// unreliable per rift, so topmost is never inferred from position).
/// The owned entry must be onscreen on the normal layer with its CG bounds
/// containing the grab point (WindowServer-visible at the point means on a
/// display). Any onscreen above-normal-layer window containing the point
/// refuses, since menu bar, Dock, panels, or banners would intercept
/// instead. Any same-normal-layer foreign window containing the point also
/// refuses: list order cannot prove the owned window is on top, so an
/// unrelated same-layer occluder is a conservative fixture refusal (clear
/// the desktop), never a silently accepted grab. Below-normal-layer entries
/// cannot occlude a normal-layer grab and are ignored. Any Ghostty rect
/// containing the point refuses as well: never Ghostty, even when occluded.
/// The owned window's own list entry never counts as an occluder. Refusals
/// name the occluder class, never titles or content. No live touch; the
/// live runner passes fresh snapshots and additionally requires the actual
/// AX element-at-position identity (see [`hit_test_identity`]).
pub fn hit_test_grab(
    point: (f64, f64),
    owned: &crate::ffi::CgWindow,
    all: &[crate::ffi::CgWindowBasic],
    ghostty: &[crate::mc_probe::GhosttyWindow],
) -> Result<(), String> {
    if owned.pid <= 1 || owned.wid == 0 {
        return Err("hit-test refused: reserved identity".to_owned());
    }
    if !owned.onscreen || owned.layer != 0 {
        return Err(format!(
            "hit-test refused: owned window offscreen or off normal layer (onscreen={} layer={})",
            owned.onscreen, owned.layer
        ));
    }
    if !point_in_rect(
        point.0,
        point.1,
        owned.bounds.x,
        owned.bounds.y,
        owned.bounds.w,
        owned.bounds.h,
    ) {
        return Err("hit-test refused: grab point outside owned CG bounds".to_owned());
    }
    if all.iter().any(|w| {
        w.onscreen
            && w.layer > 0
            && point_in_rect(
                point.0, point.1, w.bounds.x, w.bounds.y, w.bounds.w, w.bounds.h,
            )
    }) {
        let detail =
            occluder_detail(point, owned, all, ghostty).unwrap_or_else(|| "unknown".to_owned());
        return Err(format!(
            "hit-test refused: above-normal-layer chrome covers the grab point ({detail})"
        ));
    }
    if all.iter().any(|w| {
        w.onscreen
            && w.layer == 0
            && !(w.pid == owned.pid && w.wid == owned.wid)
            && point_in_rect(
                point.0, point.1, w.bounds.x, w.bounds.y, w.bounds.w, w.bounds.h,
            )
    }) {
        let detail =
            occluder_detail(point, owned, all, ghostty).unwrap_or_else(|| "unknown".to_owned());
        return Err(format!(
            "hit-test refused: same-layer foreign window covers the grab point ({detail}; order-free conservative bar; clear the desktop)"
        ));
    }
    if ghostty
        .iter()
        .any(|g| g.onscreen && point_in_rect(point.0, point.1, g.x, g.y, g.w, g.h))
    {
        let detail =
            occluder_detail(point, owned, all, ghostty).unwrap_or_else(|| "unknown".to_owned());
        return Err(format!(
            "hit-test refused: Ghostty chrome covers the grab point ({detail}; never Ghostty)"
        ));
    }
    Ok(())
}

/// Pure AX hit-test identity verdict over plain data (no OS touch): the
/// actual element-at-position owner must equal the exact held `(pid, wid)`
/// before mouse-down/drag. The live runner reads the owner via
/// `crate::ffi::ax_hit_test_owner`; this only compares. A foreign owner
/// (unrelated occluder, menu bar, desktop, chrome) refuses by mismatch;
/// reserved hit identities refuse outright. Unit-tested.
pub fn hit_test_identity(hit: (u32, u32), want: (u32, u32)) -> Result<(), String> {
    if hit.0 <= 1 || hit.1 == 0 {
        return Err("hit-test refused: reserved hit identity".to_owned());
    }
    if hit == want {
        Ok(())
    } else {
        Err(format!(
            "hit-test refused: element at the grab point belongs to a foreign window {hit:?}, want {want:?} (never the drag target)"
        ))
    }
}

/// Input-reaching proof, not rectangle stacking. Both independent native
/// hit-tests must join the exact target. Transparent compositor backing and
/// windows behind the target are not input occluders; no owner-name exemption
/// is made. Unknown/mismatched AX or WindowServer identity always refuses.
pub fn hit_test_input_grab(
    point: (f64, f64),
    owned: &crate::ffi::CgWindow,
    all: &[crate::ffi::CgWindowBasic],
    ax_owner: (u32, u32),
    input_wid: u32,
) -> Result<(), String> {
    // Retain the owned normal-layer/onscreen/containment gates, without
    // treating unrelated rectangles as evidence of input interception.
    hit_test_grab(point, owned, &[], &[])?;
    if all.len() >= crate::config::MAX_WINDOWS {
        return Err("input hit-test refused: CG snapshot completeness unknown".to_owned());
    }
    hit_test_identity(ax_owner, (owned.pid, owned.wid))?;
    if input_wid != owned.wid
        || !all
            .iter()
            .any(|w| w.wid == input_wid && w.pid == owned.pid && w.onscreen && w.layer == 0)
    {
        return Err(
            "input hit-test refused: WindowServer hit does not join the exact owned window"
                .to_owned(),
        );
    }
    Ok(())
}

/// Pure pointer-restore decision over plain data: restore the baseline
/// cursor position only when the cursor still sits where the drag left it
/// (the grab point -- all synthetic mouse posts share one coordinate).
/// A user-moved cursor is left alone (never fight physical input); an
/// already-home cursor needs no post. No live touch.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PointerRestore {
    Restore { to: (f64, f64) },
    AlreadyHome,
    LeaveUserMoved,
}

#[must_use]
pub fn pointer_restore_decision(
    cursor_now: Option<(f64, f64)>,
    drag_end: (f64, f64),
    baseline: (f64, f64),
) -> PointerRestore {
    const EPS: f64 = 1.0;
    let near = |a: (f64, f64), b: (f64, f64)| (a.0 - b.0).abs() <= EPS && (a.1 - b.1).abs() <= EPS;
    match cursor_now {
        None => PointerRestore::LeaveUserMoved,
        Some(now) if near(now, baseline) => PointerRestore::AlreadyHome,
        Some(now) if near(now, drag_end) => PointerRestore::Restore { to: baseline },
        Some(_) => PointerRestore::LeaveUserMoved,
    }
}

/// Pure temp-residue hint for failure reports when an internally created
/// temp may still exist: names the exact SID plus the narrow `mc-recover`
/// recovery command. Never broadens removal; the live runner removes ONLY
/// this SID on its own path. No live touch.
#[must_use]
pub fn temp_residue_hint(temp: u64, original: u64) -> String {
    format!(
        "temporary temp {temp} possibly resident; residue hint: mc-recover --temp {temp} --original {original}"
    )
}

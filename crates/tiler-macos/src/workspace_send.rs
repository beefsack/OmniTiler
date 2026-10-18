#![forbid(unsafe_code)]

//! Numbered send-follow controller (OFFLINE PREPARATION — not product
//! behavior).
//!
//! Nothing here is product-armed this milestone: every send catalogue row
//! is `implemented: false`, the engine refuses every send at its
//! unsupported guard, and the product tap table carries no send chord. This
//! module is the offline-tested seam for the future live step: the fake-
//! backed drain core ([`drain_claimed_send`]), the send-shape admission
//! check ([`crate::runtime_thread::admit_send_claim`]), the holder-
//! ownership helpers ([`ensure_holders_for_preset`]), and the refusal
//! classifiers. Tests inject fake backends here; no runtime path dispatches
//! here in product (the engine guard refuses first, and
//! [`drain_live_claimed`] refuses until the live backend lands).
//!
//! Future product route (planned, not wired): the 9 send-follow shapes
//! (`Ctrl+Shift+1..9` under the default Ctrl role, see
//! [`crate::workspace_input::is_armed_send`]) would move the focused
//! managed window to the target Desktop-N ordinal and follow it there,
//! through the existing synthetic titlebar-drag plus tagged Desktop-N
//! route: hold a titlebar drag on the subject window, post the target
//! ordinal's enabled standard `Ctrl+digit` chord with the own-switch
//! provenance tag (so the tap passes our own switch chords while still
//! consuming the user's identical chords, see
//! [`crate::input_match::OWN_SWITCH_USER_DATA`]), release, then read back
//! exact membership on the target current. One leg only (never a roundtrip:
//! a failed leg never inverts as a no-op). The release-first executor
//! ([`crate::space_drag_transfer::run_drag_leg`]) guarantees the mouse
//! releases on every path after mouse-down, completes a pending key edge
//! exactly once, and polls the effect only after release inside the bounded
//! [`crate::space_drag_transfer::EFFECT_SETTLE_MS`] window — the only
//! polling on this path. Everything else is single fresh reads (events over
//! polling): gates, subject state, the fresh user row, the fresh holder,
//! and the target revalidation. No callback here does AX or blocks: the
//! engine admits asynchronously through the claim slot and the main-thread
//! tile controller drains.
//!
//! Subject: the focused managed window pinned at engine admission
//! ([`crate::runtime_types::ClaimedSelect::send_subject`]) — observed focus,
//! homed to a currently-actuated domain, never fullscreen (game/fullscreen
//! foregrounds never drag). The drain re-proves subject state fresh through
//! [`SendBackend::subject_state`] and refuses drift instead of grabbing a
//! different window. Target: the ordinal resolves fresh against the drain's
//! user row ([`send_target_sid`]) and must still name the admission-time SID
//! ([`crate::runtime_types::ClaimedSelect::expected_sid`]) under the same
//! topology revision — a reorder between admission and drain refuses instead
//! of sending to the new occupant. Holder: read fresh through
//! [`SendBackend::read_holder`] and resolved through
//! [`crate::desktop_shortcuts::resolve_numbered_chord`] — disabled,
//! customized, unreadable, or unmappable holders refuse before any post
//! (never a settings write here). Follow: verified arrival notes the source
//! in the per-display previous history (so `workspace-previous` keeps
//! working) and reports `Applied` under the claim's operation identity so
//! the core settles its retained `SendNumbered` transfer; pre-actuation
//! refusals report `RefusedProven`; ambiguous mid-actuation output
//! (no-effect, unreadable readback, release residue) holds as
//! `PossiblyExecuted` without re-actuation (see
//! [`classify_send_drain_error`]).
//!
//! Ownership (OFFLINE ONLY — not wired to any runtime path): the holder-
//! ensure helpers below are a planned, fake-proven seam only. No startup
//! path, no drain path, and no preset path calls them in product; Authentic
//! performs no holder enable and Compatible performs zero store touch
//! because neither ever reaches them. [`preset_enables_desktop_holders`]
//! names which preset a future wiring would enable holders for; it
//! authorizes nothing today. Revert stays explicit
//! ([`crate::desktop_shortcuts::revert_with_store_from`], conditional — a
//! later external edit is preserved, never overwritten); there is no
//! automatic revert on shutdown.
//!
//! Live boundary (offline preparation, no live mutation): the offline drain
//! core, the admission shape check, the ownership helpers, and every
//! refusal path below are implemented and fake-proven. The live
//! managed-window backend (revision-fenced facts frame, fresh CG/AX hit
//! identity, guarded HID posts) lands in the live step — see
//! [`drain_live_claimed`] for the exact entrypoint, which refuses until
//! then so any stray claim settles proven-refused instead of retaining
//! forever. Zero, stay, relative, and output sends stay out of the seam
//! (see [`crate::workspace_input::is_armed_send`]); no modifier-plus-drag
//! product trigger exists.

use std::path::Path;

use tiler_core::directional::OutputId;
use tiler_core::world_report::{AttemptKind, AttemptOutcome};

use crate::desktop_shortcuts::{
    ApplyOutcome, DesktopShortcutStore, ShortcutEntry, apply_with_store_to, desktop_symbolic_id,
    resolve_numbered_chord,
};
use crate::input_policy::InputPreset;
use crate::runtime_types::{ClaimedSelect, InputSemanticAction, TileBinding};
use crate::space_drag_transfer::{
    DragBackend, DragStep, LegError, PostCounts, SwitchChords, run_drag_leg,
};
use crate::workspace_select::{SelectionSnapshot, SelectionState};

/// Send-drain gates for the offline seam (same shape the future live drain
/// would sample on the main thread immediately before draining one claimed
/// send: plain booleans, no OS touch here). Any denial refuses the claim
/// narrowly without replay and advances nothing — exactly like the select
/// gates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SendGates {
    pub session_unlocked: bool,
    pub secure_active: bool,
    pub lock_screen: bool,
}

/// Fresh managed-window subject state for the pinned subject window: proof
/// enough to grab or refuse. Read once per drain through
/// [`SendBackend::subject_state`] (a single fresh read, never a poll).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SubjectState {
    /// True only when the subject is still homed to a currently-actuated
    /// domain (managed membership, not hidden/closed/unknown).
    pub managed_current: bool,
    /// True when the subject currently reads native fullscreen (a
    /// fullscreen app owns its Space; game/fullscreen foregrounds never
    /// drag, even when managed).
    pub fullscreen: bool,
}

/// Product send backend: the offline-testable seam the drain runs. Fake
/// backends script every leg in tests; the live backend (live step) serves
/// fresh reads plus guarded HID posts. Timing hooks default to the real
/// drag contract ([`crate::space_drag_transfer::GRAB_SETTLE_MS`] /
/// [`crate::space_drag_transfer::ANIMATION_HOLD_MS`]); the bounded
/// post-release poll rides [`SendBackend::effect_deadline_ms`] /
/// [`SendBackend::effect_poll_ms`] (live: 3 s / 200 ms — the only polling
/// on this path, explained: native arrival needs a bounded settle after
/// release; callbacks never block waiting for it).
pub trait SendBackend {
    /// Fresh single-display user-Space row (existing user Spaces in native
    /// row order plus the current Space), same rule as
    /// [`crate::workspace_select`].
    fn snapshot(&mut self) -> Result<SelectionSnapshot, String>;
    /// Fresh native display-row count for the product single-display rule.
    fn display_count(&mut self) -> Result<usize, String>;
    /// Live topology revision backing the snapshot (`None` for backends
    /// without one — the revision leg is skipped then).
    fn revision(&mut self) -> Option<u64> {
        None
    }
    /// Fresh subject state for the pinned subject window (single read).
    fn subject_state(&mut self, window: &str) -> Result<SubjectState, String>;
    /// Fresh Desktop-N holder read by symbolic id (single read, never a
    /// write — holder usability resolves purely through
    /// [`resolve_numbered_chord`]).
    fn read_holder(&mut self, symbolic_id: u32) -> Result<ShortcutEntry, String>;
    /// Select this leg's goal before any post (fake: steer the scripted OS
    /// effect; live: select the leg's digit chord plus a fresh proven grab).
    /// Refusing here posts nothing.
    fn begin_leg(&mut self, goal: u64, chords: SwitchChords) -> Result<(), String>;
    fn post_mouse(&mut self, step: DragStep) -> Result<(), String>;
    fn post_key(&mut self, down: bool) -> Result<(), String>;
    /// Grab settle (50 ms): log-only override point for fakes.
    fn settle_grab(&mut self) {
        std::thread::sleep(std::time::Duration::from_millis(
            crate::space_drag_transfer::GRAB_SETTLE_MS,
        ));
    }
    /// Animation hold (400 ms): log-only override point for fakes.
    fn settle_hold(&mut self) {
        std::thread::sleep(std::time::Duration::from_millis(
            crate::space_drag_transfer::ANIMATION_HOLD_MS,
        ));
    }
    fn read_membership(&mut self) -> Option<Vec<u64>>;
    fn read_current(&mut self) -> Option<u64>;
    /// Post-release poll window (live: 3 s; fakes: fast).
    fn effect_deadline_ms(&self) -> u64 {
        crate::space_drag_transfer::EFFECT_SETTLE_MS
    }
    /// Post-release poll slice (live: 200 ms; fakes: fast).
    fn effect_poll_ms(&self) -> u64 {
        crate::space_drag_transfer::EFFECT_POLL_MS
    }
    fn counts(&self) -> PostCounts;
}

/// Verified send-follow effect: the subject now rides exactly the target
/// membership on the target current.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SendEffect {
    pub from: u64,
    pub target: u64,
    pub target_desktop: u32,
    pub membership: Vec<u64>,
    pub current: u64,
    pub pending_recovered: bool,
}

/// Offline send-shape binding subset (currently EMPTY in product: every
/// send catalogue row is `implemented: false`, so this returns no rows).
/// The 9 send-follow shapes (`workspace-send-1..9`) are exercised by the
/// offline tests through direct catalogue-shape injection, never through
/// this function. Never adds select chords; trailing send-0, stays,
/// relative sends, and outputs stay out. Driven by the catalogue
/// `implemented` flags plus [`crate::workspace_input::is_armed_send`]
/// (single source with the offline admission shape check).
#[must_use]
pub fn bindings() -> Vec<TileBinding> {
    crate::workspace_input::workspace_catalogue()
        .into_iter()
        .filter(|row| row.implemented && crate::workspace_input::is_armed_send(&row.action))
        .filter_map(|row| row.binding())
        .collect()
}

/// Pure ordinal-to-SID resolution over one display's native-order user row:
/// Desktop-N IS the 1-based position (`None` for 0, past-end, or empty
/// rows — never guessed). The drain calls this on its fresh snapshot; the
/// engine admission calls it on the admission-time context row (see
/// [`context_target_sid`]).
#[must_use]
pub fn send_target_sid(spaces_in_order: &[u64], index: u8) -> Option<u64> {
    if index == 0 {
        return None;
    }
    spaces_in_order.get(usize::from(index) - 1).copied()
}

/// Pure admission-time ordinal-to-SID over the native context row for
/// `output`: the display's domains in context order name the native row
/// (see [`crate::runtime_types::NativeContext::derive`]: row order is kept
/// per display for numbered mapping). `None` when the output has no row or
/// the ordinal is out of range — the admission refuses instead of binding a
/// guess.
#[must_use]
pub fn context_target_sid(
    ctx: &crate::runtime_types::NativeContext,
    output: &OutputId,
    index: u8,
) -> Option<u64> {
    if index == 0 {
        return None;
    }
    let mut row: Vec<u64> = Vec::new();
    for domain in &ctx.domains {
        if domain.output != *output {
            continue;
        }
        if !row.contains(&domain.space) {
            row.push(domain.space);
        }
    }
    send_target_sid(&row, index)
}

/// Which preset a FUTURE holder-enable wiring would apply to (offline-only
/// classifier: no runtime path calls the ensure helpers below, so this
/// authorizes nothing today). True only under Authentic; Compatible would
/// leave holders to macOS and never touch shortcut settings.
#[must_use]
pub const fn preset_enables_desktop_holders(preset: InputPreset) -> bool {
    matches!(preset, InputPreset::Authentic)
}

/// Planned Authentic Desktop-N holder ensure (OFFLINE ONLY: no runtime
/// path calls this — neither startup nor any drain — so it performs no
/// product settings effect; fake-proven seam for the live step): Desktops
/// 1-9 coverage through the existing explicit-Apply machinery
/// preimage published before the first write, never overwritten (resume
/// replays stored first-observed records), external edits preserved, never
/// overwritten, per-ID failures never block siblings. Returns the terminal
/// [`ApplyOutcome`] (results plus failures plus transfer warnings).
pub fn ensure_authentic_holders<S: DesktopShortcutStore>(
    store: &mut S,
    preimage_path: &Path,
) -> Result<ApplyOutcome, String> {
    apply_with_store_to(
        store,
        crate::desktop_shortcuts::DEFAULT_APPLY_COUNT,
        preimage_path,
    )
}

/// Preset-gated holder ensure for the future send drain (OFFLINE ONLY:
/// same non-wiring as [`ensure_authentic_holders`]): Authentic would ensure
/// through [`ensure_authentic_holders`]; Compatible returns `Ok(None)` with
/// zero store touch (no read, no write — proven by the FakeStore counters).
/// `Err` is an Apply-level refusal (bad count, hostile preimage, lock held,
/// publish failure): the drain refuses before any post.
pub fn ensure_holders_for_preset<S: DesktopShortcutStore>(
    preset: InputPreset,
    store: &mut S,
    preimage_path: &Path,
) -> Result<Option<ApplyOutcome>, String> {
    if !preset_enables_desktop_holders(preset) {
        return Ok(None);
    }
    ensure_authentic_holders(store, preimage_path).map(Some)
}

/// Proven outcome of one send drain for attempt feedback (see
/// [`classify_send_drain_error`]): `Applied` settles the retained
/// `SendNumbered` transfer on observation confirm, `Refused` settles to
/// observed without actuation (pre-actuation only), `PossiblyExecuted`
/// holds without re-actuation (ambiguous mid-actuation output).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendDrainOutcome {
    Applied,
    Refused,
    PossiblyExecuted,
}

/// Wire outcome for one [`SendDrainOutcome`] (pure): transfer kind, never
/// the view kind (selects settle views; sends settle window transfers).
#[must_use]
pub const fn attempt_outcome_for(outcome: SendDrainOutcome) -> AttemptOutcome {
    match outcome {
        SendDrainOutcome::Applied => AttemptOutcome::Applied,
        SendDrainOutcome::Refused => AttemptOutcome::RefusedProven,
        SendDrainOutcome::PossiblyExecuted => AttemptOutcome::PossiblyExecuted,
    }
}

/// Wire kind for a workspace claim action (pure): sends settle transfers,
/// everything else settles views. Single home for the discard/feedback
/// kind branch so a dropped send claim never stages a view row.
#[must_use]
pub fn feedback_kind_for(action: &InputSemanticAction) -> AttemptKind {
    if crate::workspace_input::is_armed_send(action) {
        AttemptKind::Transfer
    } else {
        AttemptKind::View
    }
}

/// Classify one send-drain failure for attempt feedback: prefixes naming
/// pre-actuation refusals (gates, unarmed, display count, subject, target,
/// stale binding, holder, and drag setup/grab/chord-down — the drain never
/// completed a chord there) settle as proven refusals; every other
/// backend/execute failure is ambiguous mid-actuation output and holds as
/// possibly-executed (never blindly repeated). Unknown strings fail safe
/// toward the hold.
#[must_use]
pub fn classify_send_drain_error(reason: &str) -> SendDrainOutcome {
    for prefix in [
        "send gated:",
        "product send refuses",
        "stale bound send discarded:",
        "send drag refused before effect",
    ] {
        if reason.starts_with(prefix) {
            return SendDrainOutcome::Refused;
        }
    }
    SendDrainOutcome::PossiblyExecuted
}

/// One-leg [`DragBackend`] adapter over a [`SendBackend`]: the product send
/// is a single leg to the target, so both chord slots carry the target
/// ordinal's digit (the `for_goal` selection is constant). The drain builds
/// this inline; fakes observe the exact posted triple through their
/// `begin_leg` selection like the fixture backends.
#[cfg_attr(not(test), allow(dead_code))]
struct OneLeg<'b, B: SendBackend> {
    inner: &'b mut B,
    chords: SwitchChords,
}

impl<B: SendBackend> DragBackend for OneLeg<'_, B> {
    fn begin_leg(&mut self, goal: u64) -> Result<(), String> {
        self.inner.begin_leg(goal, self.chords)
    }
    fn post_mouse(&mut self, step: DragStep) -> Result<(), String> {
        self.inner.post_mouse(step)
    }
    fn post_key(&mut self, down: bool) -> Result<(), String> {
        self.inner.post_key(down)
    }
    fn settle_grab(&mut self) {
        self.inner.settle_grab();
    }
    fn settle_hold(&mut self) {
        self.inner.settle_hold();
    }
    fn read_membership(&mut self) -> Option<Vec<u64>> {
        self.inner.read_membership()
    }
    fn read_current(&mut self) -> Option<u64> {
        self.inner.read_current()
    }
    fn effect_deadline_ms(&self) -> u64 {
        self.inner.effect_deadline_ms()
    }
    fn effect_poll_ms(&self) -> u64 {
        self.inner.effect_poll_ms()
    }
    fn counts(&self) -> PostCounts {
        self.inner.counts()
    }
}

/// Full-claim send drain (offline-testable core of the future send
/// handoff: tests inject fake backends here; no product path dispatches
/// here this milestone — the engine refuses every send first, and the live
/// backend in [`drain_live_claimed`] refuses until the live step). Gates
/// refuse first, then the unarmed, display-count, subject, target, stale-binding, and
/// holder rules refuse before any post; only then one
/// [`run_drag_leg`](crate::space_drag_transfer::run_drag_leg) runs (single
/// dispatch with bounded post-release settle polling, never a replay).
/// Verified follow arrival notes the source in the per-display previous
/// history (so `workspace-previous` keeps working) and returns the proven
/// effect; every refusal or ambiguous outcome advances no history. The
/// claim/release protocol around this call (not the slot itself) serializes
/// concurrent presses: the slot stays claimed while this blocks, so engine
/// enqueues refuse busy instead of overlapping.
///
/// Test-gated offline seam: the live managed-window backend lands in the
/// live step (see [`drain_live_claimed`]), which promotes this to live use.
/// Until then no product path calls this (the engine refuses every send
/// first).
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn drain_claimed_send<B: SendBackend>(
    history: &mut SelectionState,
    backend: &mut B,
    claim: ClaimedSelect,
    gates: &SendGates,
) -> Result<SendEffect, String> {
    if !gates.session_unlocked {
        return Err("send gated: session not unlocked".to_owned());
    }
    if gates.secure_active {
        return Err("send gated: Secure Input active".to_owned());
    }
    if gates.lock_screen {
        return Err("send gated: lock screen".to_owned());
    }
    let InputSemanticAction::WorkspaceSend { index, follow } = claim.action else {
        return Err("product send refuses unarmed actions".to_owned());
    };
    if !follow || index == 0 || index > 9 {
        return Err("product send refuses unarmed actions".to_owned());
    }
    // Product targeting rule: exactly ONE native display row (same rule as
    // selects — first-display guessing could send to the wrong display).
    match backend.display_count() {
        Ok(1) => {}
        Ok(n) => {
            return Err(format!(
                "product send refuses: expected exactly one native display, found {n}"
            ));
        }
        Err(e) => {
            return Err(format!(
                "product send refuses: display count unreadable ({e})"
            ));
        }
    }
    let Some(subject) = claim.send_subject.clone() else {
        return Err(
            "product send refuses: send claim carries no subject window; refusing".to_owned(),
        );
    };
    match backend.subject_state(&subject) {
        Ok(state) => {
            if !state.managed_current {
                return Err(
                    "product send refuses: subject left managed membership (not homed current; refusing)"
                        .to_owned(),
                );
            }
            if state.fullscreen {
                return Err(
                    "product send refuses: subject is fullscreen (game/fullscreen never drag)"
                        .to_owned(),
                );
            }
        }
        Err(e) => {
            return Err(format!("product send refuses: subject unreadable ({e})"));
        }
    }
    let fresh = backend
        .snapshot()
        .map_err(|e| format!("product send refuses: fresh row unreadable ({e})"))?;
    fresh
        .validate()
        .map_err(|e| format!("product send refuses: fresh row unprovable ({e})"))?;
    // Bound-claim fence FIRST: the ordinal must still name the
    // admission-time SID and the topology revision must still match (same
    // rule as bound selects — an ordinal is never interpreted against a
    // different generation). A vanished ordinal refuses here as stale
    // (past-end for unbound claims refuses below as no-target).
    if let Some(expected_sid) = claim.expected_sid {
        match fresh.spaces.get(usize::from(index) - 1) {
            Some(sid) if *sid == expected_sid => {}
            Some(_) => {
                return Err(format!(
                    "stale bound send discarded: native row moved between admission and drain (row {index} changed); no send dispatched"
                ));
            }
            None => {
                return Err(format!(
                    "stale bound send discarded: native row {index} no longer exists; no send dispatched"
                ));
            }
        }
        if let (Some(want), Some(live)) = (claim.topology_revision, backend.revision())
            && want != live
        {
            return Err(format!(
                "stale bound send discarded: topology revision moved between admission and drain ({want} != {live}); no send dispatched"
            ));
        }
    }
    let Some(target) = send_target_sid(&fresh.spaces, index) else {
        return Err(format!(
            "product send refuses: target ordinal {index} has no Space (only {} Spaces); no send dispatched",
            fresh.spaces.len()
        ));
    };
    // Fresh holder read before any post: disabled, customized, unreadable,
    // or unmappable holders refuse (never a settings write here — the
    // Authentic ensure runs in the live drain before this core).
    let desktop = u32::from(index);
    let symbolic = desktop_symbolic_id(desktop).ok_or_else(|| {
        format!(
            "product send refuses: ordinal {index} has no Switch-to-Desktop holder id; refusing rather than guessing"
        )
    })?;
    let entry = backend
        .read_holder(symbolic)
        .map_err(|e| format!("product send refuses: holder unreadable ({e})"))?;
    let (keycode, flags) =
        resolve_numbered_chord(desktop, entry).map_err(|e| format!("product send refuses: {e}"))?;
    let chords = SwitchChords::numbered((keycode, flags), (keycode, flags));
    let source = fresh.current;
    let mut adapter = OneLeg {
        inner: backend,
        chords,
    };
    let mut notes = Vec::new();
    match run_drag_leg(&mut adapter, target, target, "send", &mut notes) {
        Ok(leg) => {
            if target != source {
                history.note_arrival(&fresh.display, source);
            }
            Ok(SendEffect {
                from: source,
                target,
                target_desktop: desktop,
                membership: leg.membership,
                current: leg.current,
                pending_recovered: leg.pending_recovered,
            })
        }
        Err(LegError::Refused { stage, reason }) if stage != "readback" => Err(format!(
            "send drag refused before effect at {stage}: {reason}"
        )),
        Err(other) => Err(match other {
            LegError::Refused { reason, .. } => {
                format!("send drag effect unreadable: {reason}")
            }
            LegError::NoEffect { .. } => format!("send drag no effect ({})", other.describe()),
            LegError::UnresolvedPending { .. } | LegError::ReleaseFailed { .. } => {
                format!("send drag residue ({})", other.describe())
            }
        }),
    }
}

/// Live product drain for one claimed send-shaped action (OFFLINE
/// PREPARATION boundary — exact live entrypoint for the follow-up live
/// step, unreachable in product this milestone: no send claim ever pends
/// because the engine refuses every send first). The live step would build
/// a [`SendBackend`] over the single native display row (fresh user row
/// plus revision, revision-fenced subject facts for
/// [`SendBackend::subject_state`], holder reads through the live
/// [`crate::desktop_shortcuts::LiveStore`], and guarded HID posts through
/// the shared release-first executor), ensure Authentic holders through
/// [`ensure_holders_for_preset`] (Compatible stays zero-touch), then run
/// [`drain_claimed_send`] under the same 30 s budget plus bounded reaped
/// caffeinate companion as selects. Until that backend lands this refuses
/// before any post: the tile reports Transfer/RefusedProven under the
/// claim's operation identity so the core settles instead of retaining
/// forever (honest: the desktop was never touched).
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn drain_live_claimed(
    _history: &mut SelectionState,
    _claim: ClaimedSelect,
    _gates: &SendGates,
) -> Result<(SendEffect, bool), String> {
    Err(
        "product send refuses: live managed-window backend lands in the live step; no post performed"
            .to_owned(),
    )
}

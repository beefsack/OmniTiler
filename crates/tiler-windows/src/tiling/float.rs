#![forbid(unsafe_code)]
use super::*;

/// Post-clear routing cause for G-06 unmaximize-before-move (REQ-MAX-08, KDE
/// `requestMove` clear discipline parity): exactly one nonactivating native
/// clear attempt precedes eligible-origin routing, and only an observed clear
/// with the same reference, focus and domain continues in that invocation
/// (a cleared float takes the half-snap, a cleared tile the ordinary move).
/// `clear_outcome` is the native attempt result (`restored` when the window no
/// longer reads maximized, `dispatched` when the setter accepted but async
/// completion is still pending, anything else when nothing was dispatched).
/// Returns `None` when the move may proceed on the fresh observation, else the
/// narrow refusal cause for `move-refused-maximize-clear` (a later press can
/// retry; never a structural move, delayed intent, or automatic retry).
#[must_use]
pub fn maximize_clear_refusal(
    clear_outcome: &str,
    observed_cleared: bool,
    same_ref: bool,
    same_domain: bool,
    same_focus: bool,
    fullscreen_now: bool,
) -> Option<&'static str> {
    // Async `dispatched` is not observed completion: the fresh checks below
    // still decide. Anything undispatched never proceeds.
    if clear_outcome != "restored" && clear_outcome != "dispatched" {
        return Some("native-write");
    }
    if !same_ref {
        return Some("identity-raced");
    }
    if !observed_cleared {
        return Some("still-maximized");
    }
    if !same_domain {
        return Some("domain-raced");
    }
    if fullscreen_now {
        return Some("fullscreen-raced");
    }
    if !same_focus {
        return Some("focus-raced");
    }
    None
}

/// Pre-clear authorization for G-06 unmaximize-before-move (REQ-MAX-08):
/// the exact subject must already be revalidated and fresh fullscreen
/// precedence plus the focused origin, live domain, suspension gate and
/// foreground elevation must all still hold just before the single native
/// clear. The focus-relaxed revalidation deliberately allows fullscreen
/// facts, so this fresh check closes the transition-after-snapshot hole: a
/// subject that became captionless-fullscreen (or carries project-owned
/// fullscreen metadata) is never cleared. Returns `None` when the clear may
/// be issued, else the narrow `move-refused-maximize-clear` cause (no clear,
/// no move; a later press retries). Fullscreen precedence wins first, then
/// domain and focus races, suspension, elevation last.
#[must_use]
pub const fn authorize_maximize_clear(
    fullscreen_now: bool,
    same_domain: bool,
    same_focus: bool,
    suspend_blocked: bool,
    elevated: bool,
) -> Option<&'static str> {
    if fullscreen_now {
        Some("fullscreen-raced")
    } else if !same_domain {
        Some("domain-raced")
    } else if !same_focus {
        Some("focus-raced")
    } else if suspend_blocked {
        Some("suspended")
    } else if elevated {
        Some("elevated-foreground")
    } else {
        None
    }
}

/// Post-clear fullscreen derivation shared by the directional and B9
/// maximize-clear routes: refuse when the fresh observed frame reads
/// captionless-fullscreen, the retained row still carries fullscreen, or
/// project-owned restoration metadata is present (a non-covering owned
/// transition is eligible-observed with no retained flag, so metadata alone
/// must still veto). Pure so both routes pin without native calls; callers
/// pass live reads.
#[must_use]
pub const fn postclear_fullscreen_now(
    observed_captionless: bool,
    retained_fullscreen: bool,
    owned_meta: bool,
) -> bool {
    observed_captionless || retained_fullscreen || owned_meta
}

/// Stale owned-fullscreen retirement eligibility: positive restoration proof
/// only. `changed` is the stored cleared-bits mask, `style_restored` is
/// whether the live style again contains all of `changed`, and `fullscreen`
/// is the live fullscreen probe (`None` when unreadable). True only when the
/// originally cleared chrome bits read back restored plus fresh proof the
/// frame no longer covers a monitor. A merely non-covering geometry (owned
/// transition) never retires; unreadable style/frame never retires. A
/// zero-bit preimage (originally captionless entry) never retires: it is
/// indistinguishable from a captionless non-cover transition with the stored
/// facts, so it stays vetoed for an explicit exit.
#[must_use]
pub const fn stale_fullscreen_retire_eligible(
    changed: u32,
    style_restored: bool,
    fullscreen: Option<bool>,
) -> bool {
    changed != 0 && style_restored && matches!(fullscreen, Some(false))
}

/// Sticky marker value for one pre-sticky float state: 1 when the window was
/// tiled before sticky-on, 2 when it was already a normal float. Nonzero
/// magic only; never a pointer, never trusted across window generations (a
/// recycled HWND starts without our property).
#[must_use]
pub const fn sticky_marker_value(prior_floating: bool) -> u64 {
    if prior_floating { 2 } else { 1 }
}

/// Decode one sticky marker value to its pre-sticky float state. `None` for
/// absent, zero, or unknown values: fail closed, never guess.
#[must_use]
pub const fn parse_sticky_marker(value: u64) -> Option<bool> {
    match value {
        1 => Some(false),
        2 => Some(true),
        _ => None,
    }
}

/// Intentional-float marker value: presence with 1 means explicit float.
/// Nonzero magic only; never a pointer, never trusted across window
/// generations (a recycled HWND starts without our property).
#[must_use]
pub const fn float_intent_marker_value() -> u64 {
    1
}

/// Decode one intentional-float marker value. `Some(())` only for 1:
/// absent, zero, or unknown values fail closed, never guess.
#[must_use]
pub const fn parse_float_intent_marker(value: u64) -> Option<()> {
    match value {
        1 => Some(()),
        _ => None,
    }
}

/// Reserved fixed-window tile-override marker value (item 13 D7): presence
/// with 1 will mean a user tile override. Codec only; no admission path
/// reads it in item 8.
#[must_use]
pub const fn tile_override_marker_value() -> u64 {
    1
}

/// Decode one reserved tile-override marker value. `Some(())` only for 1;
/// everything else fails closed. Reserved; never floats a window in item 8.
#[must_use]
pub const fn parse_tile_override_marker(value: u64) -> Option<()> {
    match value {
        1 => Some(()),
        _ => None,
    }
}

/// Project-owned topmost restore gate for graceful stop and unfloat: only a
/// band the project raised (`!prior && current`) restores. A pre-existing
/// topmost stays untouched even if the user later cleared it; crash leaves
/// every frame in place and restart resets this runtime-local preimage.
#[must_use]
pub const fn float_topmost_restore_needed(prior_topmost: bool, current_topmost: bool) -> bool {
    !prior_topmost && current_topmost
}

/// Retained canonical rectangle for an overlay member (KDE carried-snapshot
/// parity): a maximized/fullscreen member rides its last-known tile
/// rectangle instead of the compositor-owned native maximum frame, so tile
/// topology and sibling shares survive the overlay. Falls back to the fresh
/// frame only when no retained allocation exists yet (first sighting).
#[must_use]
pub const fn canonical_retained_rect(overlay: bool, fresh: Rect, retained: Option<Rect>) -> Rect {
    match (overlay, retained) {
        (true, Some(rect)) => rect,
        _ => fresh,
    }
}

/// Bound for the prompt-restore owner-loop wake after one async dispatch.
pub const RESTORE_WAKE_MS: u64 = 2000;

/// Last-known minimum hint for a retained tiled overlay member (maximized or
/// fullscreen sharing this path): the fresh `WM_GETMINMAXINFO` query is
/// unavailable while overlaid, so the last-known declared hint rides the
/// canonical slot until a normal fresh query resumes. Never derived from the
/// maximized frame, never stale: `None`/empty when the token moved on
/// (recycled HWND mints a fresh token), when the member floats or holds a
/// born-fullscreen slotless row, or when no canonical slot exists yet.
#[must_use]
pub fn retained_overlay_hint(
    logged: Option<tiler_core::size_hints::WindowSizeHints>,
    token_matches: bool,
    overlay: bool,
    is_float: bool,
    born_hold: bool,
    has_slot: bool,
) -> tiler_core::size_hints::WindowSizeHints {
    if !token_matches || !overlay || is_float || born_hold || !has_slot {
        return tiler_core::size_hints::WindowSizeHints::none();
    }
    match logged {
        Some(hints) if !hints.is_empty() => hints,
        _ => tiler_core::size_hints::WindowSizeHints::none(),
    }
}

/// Arm only on a restore request whose async setter is still pending.
#[must_use]
pub fn restore_wake_arm(outcome: &str, wanted_restored: bool) -> bool {
    wanted_restored && outcome == "dispatched"
}

/// One pump of the armed restore wake. Returns `(keep, woke)`: `woke`
/// demands a reconcile once completion is observed; `keep` survives pending
/// dispatches and gesture/suspend pauses and clears only after a gated
/// reconcile consumes it, on expiry, or on window/identity loss.
#[must_use]
pub const fn restore_wake_step(
    armed: bool,
    expired: bool,
    gone: bool,
    observed_done: bool,
    reconciled: bool,
) -> (bool, bool) {
    if !armed || expired || gone {
        return (false, false);
    }
    if observed_done {
        return (!reconciled, true);
    }
    (true, false)
}

/// Flag-stability gate before a workspace-send membership transfer (KDE
/// `flagsStillMatch` parity): the mover's live overlay flags must still
/// equal the dispatch snapshot, or the transfer refuses with no writes.
#[must_use]
pub const fn send_flags_stable(
    snapshot_fullscreen: bool,
    snapshot_maximized: bool,
    fresh_fullscreen: bool,
    fresh_maximized: bool,
) -> bool {
    snapshot_fullscreen == fresh_fullscreen && snapshot_maximized == fresh_maximized
}

/// Focus eligibility gate over host facts. Thin normalization over the shared
/// core ordered focus classification
/// ([`tiler_core::window_policy::classify_focus`]): focus carries no geometry
/// write, so both overlays are ignored while every other gate keeps its
/// geometry priority, including `no_activate` last.
pub fn classify_focus(facts: &WindowFacts) -> Result<(), SkipReason> {
    tiler_core::window_policy::classify_focus(&super::classification::safety_facts_of(facts))
        .map_err(SkipReason::from)
}

/// Project-owned fullscreen toggle decision for one verified managed window.
/// Our restoration metadata is authoritative: exiting needs that preimage, so
/// a press on any owned frame exits even when the frame no longer covers a
/// monitor (external move or partial restore). Entering is always
/// project-owned (the toggle stores its own restoration preimage first); an
/// app-requested fullscreen frame carries no known preimage, so a command
/// exit refuses with a bounded reason and the frame stays app-owned: never
/// synthesize input, never guess restoration. Re-entry never overwrites an
/// existing preimage: owned frames always exit first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FullscreenToggle {
    /// Window is not fullscreen: enter via the owned style/frame route.
    Enter,
    /// Window is fullscreen with our restoration metadata: restore it.
    ExitOwned,
    /// Window is fullscreen without our metadata: refuse, leave app-owned.
    RefuseAppOwned,
}

impl FullscreenToggle {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Enter => "enter",
            Self::ExitOwned => "exit-owned",
            Self::RefuseAppOwned => "refuse-app-owned",
        }
    }
}

/// Decide the toggle direction from fresh fullscreen state plus the presence
/// of our restoration metadata. Owned metadata wins over geometry: an owned
/// frame exits even when not monitor-covering. Pure so the command-exit
/// refusal pins without native calls.
#[must_use]
pub const fn fullscreen_toggle_decision(fullscreen: bool, owned: bool) -> FullscreenToggle {
    if owned {
        FullscreenToggle::ExitOwned
    } else if !fullscreen {
        FullscreenToggle::Enter
    } else {
        FullscreenToggle::RefuseAppOwned
    }
}

/// Fresh per-intent gate for the native toggle arms (maximize, fullscreen,
/// float, sticky): suspended sessions and elevated foregrounds settle without
/// side effects, matching the workspace dispatcher. Suspension wins when both
/// hold. `None` when the intent may proceed to its fresh revalidation. Pure
/// so the priority pins without native calls; callers pass fresh
/// `suspend_read(...).veto.block` and `foreground_elevated(...)` reads, whose
/// owned-fullscreen exemption keeps exiting our own fullscreen working.
#[must_use]
pub const fn toggle_gate_outcome(suspended: bool, elevated: bool) -> Option<&'static str> {
    if suspended {
        Some("suspended")
    } else if elevated {
        Some("elevated-foreground")
    } else {
        None
    }
}

/// Stateless pre-frame verdict for one allowlisted entry: frozen-identity
/// matching plus window state, available before (and without) any frame
/// query. This is the production decision path for the inspection report
/// when geometry is absent: legitimately hidden (passive) or minimized exact
/// helpers keep full identity match with an accurate skip, while a
/// non-matching identity is a true unknown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatelessVerdict {
    /// Verdict for this pass: report identity state with this skip and
    /// perform no geometry read and no actuation.
    Report {
        identity_match: bool,
        eligible: bool,
        skip: SkipReason,
    },
    /// Identity matched a visible, non-minimized window: frame queries and
    /// `classify` are still required.
    NeedsGeometry,
}

/// Map `(identity_match, visible, minimized)` to the inspection verdict.
/// Mismatch beats visibility (a foreign window at a frozen HWND is unknown
/// even when visible); hidden beats minimized; minimized needs no frame.
#[must_use]
pub const fn inspect_stateless_verdict(
    identity_match: bool,
    visible: bool,
    minimized: bool,
) -> StatelessVerdict {
    if !identity_match {
        return StatelessVerdict::Report {
            identity_match: false,
            eligible: false,
            skip: SkipReason::IdentityChanged,
        };
    }
    if !visible {
        return StatelessVerdict::Report {
            identity_match: true,
            eligible: false,
            skip: SkipReason::Hidden,
        };
    }
    if minimized {
        return StatelessVerdict::Report {
            identity_match: true,
            eligible: false,
            skip: SkipReason::Minimized,
        };
    }
    StatelessVerdict::NeedsGeometry
}

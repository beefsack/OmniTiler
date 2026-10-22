#![forbid(unsafe_code)]
//! Native window guards: member identity, restart markers, foreground veto,
//! and directional/send fence gates used by the live world/native routes.
//!
//! Moved from the deleted per-output session owners (`workspace`,
//! `workspace_owner`, `workspace_scope_owner`): the world core owns rings,
//! views, history, and trailing outright, so this module keeps only the
//! adapter-mechanical checks (full `WindowKey` identity, lifetime tags,
//! on-window marker classification, output geometry, pre-effect fences).
//! No HWNDs, no window properties, no logs here: pure predicates over
//! caller-owned facts.

use std::collections::{BTreeMap, BTreeSet};

use tiler_core::directional::Direction;
use tiler_core::geometry::Rect;

use crate::snapkey::SnapOp;

/// Stable session member identity: HWND plus process-lifetime evidence.
/// Hidden claims bind the ephemeral nonce separately in the owner table and
/// the ledger; same-process HWND reuse fences on `(hwnd, creation)` plus the
/// live nonce check before any effect.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WindowKey {
    pub hwnd: u64,
    pub pid: u32,
    pub creation: String,
}

/// Fixed workspace-scope helpers over plain key slices (world-owned
/// topology): the same foreground/pointer/cache/ordering rule as the removed
/// session query, without the removed session type.
#[must_use]
pub fn output_context_keys(
    keys: &[String],
    cached: Option<&str>,
    foreground: Option<&str>,
    pointer: Option<&str>,
) -> Option<String> {
    if let Some(key) = foreground
        && keys.iter().any(|k| k == key)
    {
        return Some(key.to_owned());
    }
    if let Some(key) = pointer
        && keys.iter().any(|k| k == key)
    {
        return Some(key.to_owned());
    }
    if let Some(key) = cached
        && keys.iter().any(|k| k == key)
    {
        return Some(key.to_owned());
    }
    keys.first().cloned()
}

/// Same-HWND reuse set: known member keys sharing the fresh window's HWND
/// with a different full identity. The fresh enumeration read is
/// authoritative: a HWND hosts exactly one process, so any other key for the
/// number is a destroyed window whose number the OS recycled while visible.
/// Live hidden claims are excluded: they own their HWND through the ledger
/// and audit path (retire with identity-safe reveal), never this repair.
#[must_use]
pub fn reused_hwnd_stale(
    known: &[WindowKey],
    hidden_claimed: &BTreeSet<WindowKey>,
    fresh: &WindowKey,
) -> Vec<WindowKey> {
    known
        .iter()
        .filter(|k| k.hwnd == fresh.hwnd && *k != fresh && !hidden_claimed.contains(*k))
        .cloned()
        .collect()
}

/// Visible-membership lifetime decision: the live member tag must equal the
/// tag stored at admission. A same-process HWND reuse starts without our
/// window-lifetime property (absent) or with a stale tag, so it fails closed
/// exactly where HWND/PID/creation all still agree. An empty stored tag
/// never matches: untracked windows are never trusted, only stamped.
#[must_use]
pub fn visible_lifetime_ok(stored: &str, live: Option<&str>) -> bool {
    !stored.is_empty() && live.is_some_and(|tag| tag == stored)
}

/// Recycled-HWND guard: the live window matches the stored member only when
/// HWND, PID, and process creation all agree. A same-process HWND reuse keeps
/// all three, so this guard alone never sees it: the visible-membership
/// lifetime property must also agree (see [`visible_lifetime_ok`]).
#[must_use]
pub fn member_matches(
    stored: &WindowKey,
    live_hwnd: u64,
    live_pid: u32,
    live_creation: &str,
) -> bool {
    stored.hwnd == live_hwnd && stored.pid == live_pid && stored.creation.as_str() == live_creation
}

/// Hidden snapshot rectangle for one member: intentional floats ride the live
/// float snapshot with the tiled allocation as fallback; every other member
/// (including born holds, which never seed a float snapshot) rides the tiled
/// allocation with the float snapshot as fallback. `None` only when both are
/// missing, so row assembly drops nothing it could keep.
#[must_use]
pub const fn hidden_snapshot_rect(
    is_float: bool,
    float_rect: Option<Rect>,
    member_rect: Option<Rect>,
) -> Option<Rect> {
    if is_float {
        match float_rect {
            Some(rect) => Some(rect),
            None => member_rect,
        }
    } else {
        match member_rect {
            Some(rect) => Some(rect),
            None => float_rect,
        }
    }
}

/// Portable writable-token subset: non-hidden members whose Engine token is
/// present in the fresh eligible observation. Retained members without a
/// fresh frame and hidden members never take geometry writes; hidden
/// workspace geometry waits for reveal. Scope and identity revalidation stay
/// with the native caller; this pins the hidden/retained exclusion.
#[must_use]
pub fn writable_subset(
    members: &BTreeSet<WindowKey>,
    is_hidden: impl Fn(&WindowKey) -> bool,
    token_of: &BTreeMap<WindowKey, String>,
    fresh_tokens: &std::collections::HashSet<String>,
) -> std::collections::HashSet<String> {
    members
        .iter()
        .filter(|k| !is_hidden(k))
        .filter_map(|k| token_of.get(k))
        .filter(|t| fresh_tokens.contains(*t))
        .cloned()
        .collect()
}

/// One on-window marker slot read (R-RST-01/R-FLT-05). `Value`
/// carries the live `GetProp` value, always nonzero (native NULL/zero reads
/// as [`MarkerSlot::Absent`); a `Value(0)` only arises in unit tests and
/// fails closed as corrupt. `Unreadable` is the fail-closed classifier input
/// for a genuinely failed read: diagnosed, no intent. No current native
/// reader produces it (`GetPropW` NULL reads absent); it stays so the
/// classifier never mistakes a future read failure for intent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkerSlot {
    Absent,
    Value(u64),
    Unreadable,
}

/// Restart marker classification for one window's slots. Priority is sticky,
/// then intentional float, then the reserved tile override; a failed read
/// with no valid higher lane is [`RestartMarker::Unreadable`] (diagnosed, no
/// intent), and a present-but-wrong value is [`RestartMarker::Corrupt`]
/// (logged, no intent). Missing means empty. Markers restore classification
/// only: hydration still needs admission plus identity and lifetime fences.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestartMarker {
    Absent,
    FloatIntent,
    Sticky(bool),
    TileOverride,
    Corrupt,
    Unreadable,
}

/// Classify one window's restart marker slots with sticky-first priority.
#[must_use]
pub fn classify_restart_marker(
    float: MarkerSlot,
    sticky: MarkerSlot,
    tile: MarkerSlot,
) -> RestartMarker {
    if let MarkerSlot::Value(raw) = sticky
        && let Some(prior) = crate::tiling::parse_sticky_marker(raw)
    {
        return RestartMarker::Sticky(prior);
    }
    if let MarkerSlot::Value(raw) = float
        && crate::tiling::parse_float_intent_marker(raw).is_some()
    {
        return RestartMarker::FloatIntent;
    }
    if let MarkerSlot::Value(raw) = tile
        && crate::tiling::parse_tile_override_marker(raw).is_some()
    {
        return RestartMarker::TileOverride;
    }
    if matches!(float, MarkerSlot::Unreadable)
        || matches!(sticky, MarkerSlot::Unreadable)
        || matches!(tile, MarkerSlot::Unreadable)
    {
        return RestartMarker::Unreadable;
    }
    if matches!(float, MarkerSlot::Value(_))
        || matches!(sticky, MarkerSlot::Value(_))
        || matches!(tile, MarkerSlot::Value(_))
    {
        RestartMarker::Corrupt
    } else {
        RestartMarker::Absent
    }
}

/// Whether one classified marker re-adopts an ordinary intentional float on
/// owner restart: admitted membership plus exact identity plus live lifetime
/// tag, with a valid float marker. Every other lane (sticky, reserved tile
/// override, corrupt, unreadable, absent) never floats here.
#[must_use]
pub const fn restart_float_eligible(
    admitted: bool,
    identity_ok: bool,
    lifetime_ok: bool,
    marker: RestartMarker,
) -> bool {
    admitted && identity_ok && lifetime_ok && matches!(marker, RestartMarker::FloatIntent)
}

/// Which pre-sticky state one classified marker re-adopts on owner restart:
/// `Some(prior)` under the same admission/identity/lifetime fences as
/// [`restart_float_eligible`], `None` otherwise. Both origins re-adopt as
/// sticky before any tiling (never as a normal float); the exact
/// post-restart un-stick result stays TBD under R-FLT-05/R-RST-01.
#[must_use]
pub const fn restart_sticky_eligible(
    admitted: bool,
    identity_ok: bool,
    lifetime_ok: bool,
    marker: RestartMarker,
) -> Option<bool> {
    if !(admitted && identity_ok && lifetime_ok) {
        return None;
    }
    match marker {
        RestartMarker::Sticky(prior) => Some(prior),
        _ => None,
    }
}

/// Whether one classified marker re-adopts an explicit fixed-window tile
/// override on owner restart (R-SPC-04 D7): admitted membership plus exact
/// identity plus live lifetime tag, with a valid tile-override marker. The
/// override restores classification only (the stamped suppress signal on
/// subsequent observations); hydration performs no writes, stages no Engine
/// commit, and installs no marker. Missing/degraded/unmatched records fall
/// back to ordinary fixed classification (recompute, no hold/retry). Every
/// other lane (sticky, float, corrupt, unreadable, absent) never overrides
/// here.
#[must_use]
pub const fn restart_tile_override_eligible(
    admitted: bool,
    identity_ok: bool,
    lifetime_ok: bool,
    marker: RestartMarker,
) -> bool {
    admitted && identity_ok && lifetime_ok && matches!(marker, RestartMarker::TileOverride)
}

/// Privacy-safe foreground veto reason. Bounded vocabulary only: no titles,
/// paths, HWNDs, PIDs, or content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForegroundVetoReason {
    /// No covering window (null foreground, captioned, or small borderless).
    None,
    /// Captioned foreground: never borderless fullscreen by construction.
    Captioned,
    /// Valid foreground handle that is not visible: invisible windows cannot
    /// be covering.
    Nonvisible,
    /// Exact desktop shell handle (`GetShellWindow`/`GetDesktopWindow`): the
    /// desktop, not a fullscreen application.
    Desktop,
    /// Visible captionless foreground whose frame covers a monitor.
    Fullscreen,
    /// Valid visible foreground that the compositor reports cloaked
    /// (`DWMWA_CLOAKED`): invisible to the compositor, so never a covering
    /// fullscreen even when its frame spans a monitor.
    Cloaked,
    /// Captionless monitor-covering foreground verified as a managed member
    /// or a born-held tracked window: it rides a retained overlay slot with
    /// no geometry writes, so it never suspends the workspace. Unverified
    /// fullscreen still reports `Fullscreen` and suspends.
    ManagedOverlay,
    /// Visible foreground whose frame could not be read: fail closed.
    Unreadable,
    /// Non-null foreground handle that fails validity: fail closed.
    Invalid,
}

impl ForegroundVetoReason {
    /// Stable log token for the bounded action summary.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Captioned => "captioned",
            Self::Nonvisible => "nonvisible",
            Self::Desktop => "desktop",
            Self::Fullscreen => "fullscreen",
            Self::Cloaked => "cloaked",
            Self::ManagedOverlay => "managed-overlay",
            Self::Unreadable => "unreadable",
            Self::Invalid => "invalid",
        }
    }
}

/// Portable foreground facts for the veto classifier. The native caller owns
/// all reads; this struct carries only booleans plus the already-computed
/// covering predicate, never handles, paths, or titles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ForegroundFacts {
    /// Foreground handle passed native validity (`IsWindow`, non-null).
    pub valid: bool,
    /// Exact desktop shell identity (`GetShellWindow`/`GetDesktopWindow`).
    pub is_desktop: bool,
    /// Fresh `IsWindowVisible` read on the valid handle.
    pub visible: bool,
    /// Caption bit (`WS_CAPTION`) from the live style.
    pub captioned: bool,
    /// DWM extended-frame-bounds read succeeded.
    pub dwm_readable: bool,
    /// DWM cloak-attribute (`DWMWA_CLOAKED`) read succeeded. Failure fails
    /// closed like an unreadable frame: an unknown cloak state never clears
    /// a covering frame.
    pub cloak_readable: bool,
    /// DWM reports the foreground cloaked (invisible to the compositor).
    pub cloaked: bool,
    /// Captionless frame covers a monitor full rect (portable predicate).
    pub covers_monitor: bool,
}

/// Veto decision plus reason. `block == true` preserves the existing gate:
/// visible real fullscreen, visible unreadable, and invalid foregrounds veto;
/// everything else (including valid non-visible and exact desktop) does not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ForegroundVeto {
    pub block: bool,
    pub reason: ForegroundVetoReason,
}

/// Portable fact-based foreground veto policy, used by the actual native path.
/// Order matters: validity first (fail closed), then exact desktop identity
/// (classifier correctness, not an intent exception), then fresh visibility
/// (invisible cannot cover), then readability (visible unknown stays blocked),
/// then cloak (cloaked is invisible to the compositor, unreadable cloak
/// stays blocked), then caption, then covering geometry.
#[must_use]
pub const fn classify_foreground(facts: ForegroundFacts) -> ForegroundVeto {
    if !facts.valid {
        return ForegroundVeto {
            block: true,
            reason: ForegroundVetoReason::Invalid,
        };
    }
    if facts.is_desktop {
        return ForegroundVeto {
            block: false,
            reason: ForegroundVetoReason::Desktop,
        };
    }
    if !facts.visible {
        return ForegroundVeto {
            block: false,
            reason: ForegroundVetoReason::Nonvisible,
        };
    }
    if !facts.dwm_readable {
        return ForegroundVeto {
            block: true,
            reason: ForegroundVetoReason::Unreadable,
        };
    }
    if !facts.cloak_readable {
        return ForegroundVeto {
            block: true,
            reason: ForegroundVetoReason::Unreadable,
        };
    }
    if facts.cloaked {
        return ForegroundVeto {
            block: false,
            reason: ForegroundVetoReason::Cloaked,
        };
    }
    if facts.captioned {
        return ForegroundVeto {
            block: false,
            reason: ForegroundVetoReason::Captioned,
        };
    }
    if facts.covers_monitor {
        return ForegroundVeto {
            block: true,
            reason: ForegroundVetoReason::Fullscreen,
        };
    }
    ForegroundVeto {
        block: false,
        reason: ForegroundVetoReason::None,
    }
}

/// One FULL-rectangle output for directional selection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectionalOutput {
    pub id: String,
    pub full: Rect,
}

/// Select the adjacent output for an exhausted directional move/focus.
///
/// `mover_rect` is the moving/focused window frame in FULL-coordinate space.
/// Returns `Ok(None)` when no candidate edge-touches (caller-side local
/// no-op, no wrap); `Err` on any unreadable topology (fail-closed, never a
/// silent skip). Work areas/gaps never participate: callers pass FULL rects.
pub fn select_directional_target(
    direction: Direction,
    source_output: &str,
    mover_rect: &Rect,
    outputs: &[DirectionalOutput],
) -> Result<Option<String>, tiler_core::output_selection::SelectionError> {
    use tiler_core::directional::OutputId;
    use tiler_core::output_selection::{OutputEntry, select_for_window};
    let mut entries = Vec::with_capacity(outputs.len());
    for output in outputs {
        entries.push(OutputEntry {
            id: OutputId(output.id.clone()),
            rect: output.full,
        });
    }
    let Some(source) = outputs.iter().find(|o| o.id == source_output) else {
        return Err(tiler_core::output_selection::SelectionError::UnreadableTopology);
    };
    let source_entry = OutputEntry {
        id: OutputId(source.id.clone()),
        rect: source.full,
    };
    match select_for_window(direction, &source_entry, mover_rect, &entries)? {
        Some(candidate) => Ok(Some(candidate.id.0)),
        None => Ok(None),
    }
}

/// Whether a directional focus may cross: horizontal only. Vertical always
/// stays local (callers never build a pair for Up/Down).
#[must_use]
pub const fn directional_focus_crosses(direction: Direction) -> bool {
    matches!(direction, Direction::Left | Direction::Right)
}

/// Resolve the output under a pointer point for pointer-driven cross-output
/// drops (REQ-MOU-03): the first FULL rectangle containing the point, in
/// enumeration order (primary first). FULL rectangles decide (panels never
/// break output identity); work areas govern placement later. Physical
/// coordinates throughout, negative origins included. `None` outside every
/// output: the caller hides/refuses without a drop. This is pointer
/// containment, not the directional adjacency selector and not position-based
/// output selection: differing arrangements all behave alike.
#[must_use]
pub fn output_at_point(outputs: &[DirectionalOutput], x: i32, y: i32) -> Option<String> {
    for output in outputs {
        let r = output.full;
        if r.w > 0
            && r.h > 0
            && x >= r.x
            && x < r.x.saturating_add(r.w)
            && y >= r.y
            && y < r.y.saturating_add(r.h)
        {
            return Some(output.id.clone());
        }
    }
    None
}

/// Carried pre-plan directional fences: both gaps, both workspace modes, both
/// current views, both FULL rectangles plus both inset domain bounds, owner
/// binding, overlay flags, and the resolved direction/target. Tails re-verify
/// live before any native mutation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectionalFenceSnapshot {
    pub inner_gap: i32,
    pub outer_gap: i32,
    pub source_output: String,
    pub source_workspace: String,
    pub target_output: String,
    pub target_workspace: String,
    pub source_tiled: bool,
    pub target_tiled: bool,
    pub direction: Direction,
    pub overlay_maximized: bool,
    pub overlay_fullscreen: bool,
    pub source_full: Rect,
    pub target_full: Rect,
    pub source_bounds: Rect,
    pub target_bounds: Rect,
    pub owner: String,
    pub generation: String,
}

/// Live tail-time directional fence readings matched against a snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectionalFenceLive {
    pub inner_gap: i32,
    pub outer_gap: i32,
    pub source_tiled: bool,
    pub target_tiled: bool,
    pub source_is_active: bool,
    pub target_is_active: bool,
    pub topology_ok: bool,
    pub scope_ok: bool,
    pub lifetime_ok: bool,
    pub membership_ok: bool,
    pub overlay_maximized: bool,
    pub overlay_fullscreen: bool,
    pub owner_ok: bool,
}

/// Shared directional fence evaluation: gaps, modes, current views,
/// topology, scope, lifetime, membership, overlay, and owner binding must all
/// hold. Returns the refusal token, or `None` when the tails may proceed.
/// Both move and focus share this gate; focus adds no geometry writes and
/// move adds the core cross preconditions on top.
#[must_use]
pub fn directional_fences_hold(
    snapshot: &DirectionalFenceSnapshot,
    live: &DirectionalFenceLive,
) -> Option<&'static str> {
    if !live.owner_ok {
        return Some("owner-changed");
    }
    if snapshot.inner_gap != live.inner_gap || snapshot.outer_gap != live.outer_gap {
        return Some("gap-changed");
    }
    if !live.source_tiled || !live.target_tiled {
        return Some("workspace-floating");
    }
    if snapshot.source_tiled != live.source_tiled || snapshot.target_tiled != live.target_tiled {
        return Some("mode-changed");
    }
    if !live.source_is_active || !live.target_is_active {
        return Some("view-changed");
    }
    if !live.topology_ok {
        return Some("topology-changed");
    }
    if !live.scope_ok {
        return Some("scope-changed");
    }
    if !live.lifetime_ok {
        return Some("identity-changed");
    }
    if !live.membership_ok {
        return Some("origin-vanished");
    }
    if live.overlay_fullscreen != snapshot.overlay_fullscreen {
        return Some("overlay-changed");
    }
    if live.overlay_maximized != snapshot.overlay_maximized {
        return Some("overlay-changed");
    }
    if live.overlay_fullscreen {
        return Some("move-refused-fullscreen");
    }
    None
}

/// Carried pre-plan drag-drop fences: both gaps, both workspace modes, both
/// current views, both FULL rectangles plus both inset domain bounds, and the
/// owner binding. Same fence shape as the directional snapshot minus the
/// direction (pointer drops are not adjacency-derived). Tails re-verify live
/// before any native mutation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DragDropFenceSnapshot {
    pub inner_gap: i32,
    pub outer_gap: i32,
    pub source_output: String,
    pub source_workspace: String,
    pub target_output: String,
    pub target_workspace: String,
    pub source_tiled: bool,
    pub target_tiled: bool,
    pub overlay_maximized: bool,
    pub overlay_fullscreen: bool,
    pub source_full: Rect,
    pub target_full: Rect,
    pub source_bounds: Rect,
    pub target_bounds: Rect,
    pub owner: String,
    pub generation: String,
}

/// View a directional fence snapshot through the direction-free drag-drop
/// fence shape (the direction selects the candidate only; evaluation never
/// reads it). Shared by live-fence reads and tail-time topology checks so the
/// mapping lives in exactly one place.
#[must_use]
pub fn drag_snapshot_for_directional(snapshot: &DirectionalFenceSnapshot) -> DragDropFenceSnapshot {
    DragDropFenceSnapshot {
        inner_gap: snapshot.inner_gap,
        outer_gap: snapshot.outer_gap,
        source_output: snapshot.source_output.clone(),
        source_workspace: snapshot.source_workspace.clone(),
        target_output: snapshot.target_output.clone(),
        target_workspace: snapshot.target_workspace.clone(),
        source_tiled: snapshot.source_tiled,
        target_tiled: snapshot.target_tiled,
        overlay_maximized: snapshot.overlay_maximized,
        overlay_fullscreen: snapshot.overlay_fullscreen,
        source_full: snapshot.source_full,
        target_full: snapshot.target_full,
        source_bounds: snapshot.source_bounds,
        target_bounds: snapshot.target_bounds,
        owner: snapshot.owner.clone(),
        generation: snapshot.generation.clone(),
    }
}

/// Shared drag-drop fence evaluation: gaps, modes, current views, topology,
/// scope, lifetime, membership, overlay, and owner binding must all hold.
/// Returns the refusal token, or `None` when the tails may proceed. Reuses
/// [`DirectionalFenceLive`] (which carries no direction); only the snapshot
/// differs from the directional gate.
#[must_use]
pub fn drag_drop_fences_hold(
    snapshot: &DragDropFenceSnapshot,
    live: &DirectionalFenceLive,
) -> Option<&'static str> {
    if !live.owner_ok {
        return Some("owner-changed");
    }
    if snapshot.inner_gap != live.inner_gap || snapshot.outer_gap != live.outer_gap {
        return Some("gap-changed");
    }
    if !live.source_tiled || !live.target_tiled {
        return Some("workspace-floating");
    }
    if snapshot.source_tiled != live.source_tiled || snapshot.target_tiled != live.target_tiled {
        return Some("mode-changed");
    }
    if !live.source_is_active || !live.target_is_active {
        return Some("view-changed");
    }
    if !live.topology_ok {
        return Some("topology-changed");
    }
    if !live.scope_ok {
        return Some("scope-changed");
    }
    if !live.lifetime_ok {
        return Some("identity-changed");
    }
    if !live.membership_ok {
        return Some("origin-vanished");
    }
    if live.overlay_fullscreen != snapshot.overlay_fullscreen {
        return Some("overlay-changed");
    }
    if live.overlay_maximized != snapshot.overlay_maximized {
        return Some("overlay-changed");
    }
    if live.overlay_fullscreen {
        return Some("move-refused-fullscreen");
    }
    None
}

/// Pre-plan fence snapshot carried into the send tails: both gaps, both
/// workspace modes, and the live overlay flags resolved at snapshot time
/// (live OS read with retained-row fallback, never fabricated). Tails
/// re-verify against live readings before source writes AND hide/focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SendFenceSnapshot {
    pub inner_gap: i32,
    pub outer_gap: i32,
    pub source_tiled: bool,
    pub target_tiled: bool,
    pub overlay_maximized: bool,
    pub overlay_fullscreen: bool,
}

/// Live tail-time fence readings matched against a [`SendFenceSnapshot`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SendFenceLive {
    pub inner_gap: i32,
    pub outer_gap: i32,
    pub source_tiled: bool,
    pub target_tiled: bool,
    pub active_is_source: bool,
    pub target_is_active: bool,
    pub scope_ok: bool,
    pub lifetime_ok: bool,
    pub membership_ok: bool,
    pub overlay_maximized: bool,
    pub overlay_fullscreen: bool,
}

/// Re-verify carried pre-plan fences against live tail-time readings.
/// `None` holds; `Some(outcome)` refuses with no further effects and no
/// replay. Ordinary movers ride `(false, false)` overlay flags through both
/// sides and pass trivially; a stable fullscreen snapshot carries exactly
/// like maximized; a newly arrived live fullscreen refuses outright (never
/// sends) and any other overlay drift defers with no restore.
/// Production tails MUST route through this gate; it never fabricates state,
/// only compares.
#[must_use]
pub const fn send_fences_hold(
    snap: SendFenceSnapshot,
    live: SendFenceLive,
) -> Option<&'static str> {
    if !live.active_is_source || live.target_is_active {
        return Some("view-changed");
    }
    if live.source_tiled != snap.source_tiled || live.target_tiled != snap.target_tiled {
        return Some("mode-changed");
    }
    if live.inner_gap != snap.inner_gap || live.outer_gap != snap.outer_gap {
        return Some("deferred");
    }
    if !live.scope_ok {
        return Some("scope-excluded");
    }
    if !live.lifetime_ok {
        return Some("identity-changed");
    }
    if !live.membership_ok {
        return Some("unverified");
    }
    if live.overlay_fullscreen && live.overlay_fullscreen != snap.overlay_fullscreen {
        return Some("send-refused-fullscreen");
    }
    if live.overlay_maximized != snap.overlay_maximized
        || live.overlay_fullscreen != snap.overlay_fullscreen
    {
        return Some("deferred");
    }
    None
}

/// Fullscreen-carry overlay-flag gate for a send (portable decision; the
/// caller supplies live OS reads, never fabricated): a stable fullscreen
/// snapshot carries exactly like maximized; a newly arrived live fullscreen
/// refuses outright; any other maximized/fullscreen drift between the
/// pre-plan snapshot and the live recheck defers with no writes and no
/// restore. A fullscreen snapshot with an unreadable live flag defers as
/// well (pre-effect: never trust the retained fallback before effects).
/// Production pre-plan, post-plan, and pre-effect checks MUST route through
/// this gate. Maximized/fullscreen carry itself never restores.
pub const fn send_overlay_gate(
    pre_maximized: bool,
    pre_fullscreen: bool,
    live_maximized: bool,
    live_fullscreen: Option<bool>,
    fallback_fullscreen: bool,
) -> Result<(), &'static str> {
    if pre_fullscreen && live_fullscreen.is_none() {
        return Err("deferred");
    }
    let live_fullscreen = match live_fullscreen {
        Some(flag) => flag,
        None => fallback_fullscreen,
    };
    if live_fullscreen && live_fullscreen != pre_fullscreen {
        return Err("send-refused-fullscreen");
    }
    if live_maximized != pre_maximized || live_fullscreen != pre_fullscreen {
        return Err("deferred");
    }
    Ok(())
}

/// Directional refusal on a floating workspace (KDE plan-adapter parity:
/// `focus-refused-workspace-floating` plus `move-refused-workspace-floating`).
/// Both focus and move refuse: a floating workspace runs no tile layout, so
/// directional navigation has no topology to move through or focus within.
/// `None` when the workspace is tiled.
#[must_use]
pub const fn directional_workspace_refusal(op: SnapOp, tiled: bool) -> Option<&'static str> {
    if tiled {
        return None;
    }
    Some(match op {
        SnapOp::Focus => "focus-refused-workspace-floating",
        SnapOp::Move => "move-refused-workspace-floating",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(hwnd: u64, creation: &str) -> WindowKey {
        WindowKey {
            hwnd,
            pid: 1000 + hwnd as u32,
            creation: creation.to_owned(),
        }
    }

    fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
        Rect { x, y, w, h }
    }

    #[test]
    fn member_identity_needs_full_triple_and_lifetime_tag() {
        let stored = key(11, "c1");
        assert!(member_matches(&stored, 11, 1011, "c1"));
        assert!(!member_matches(&stored, 12, 1011, "c1"));
        assert!(!member_matches(&stored, 11, 1012, "c1"));
        assert!(!member_matches(&stored, 11, 1011, "c2"));
        // Same-process HWND reuse keeps the triple: only the lifetime tag
        // fails closed.
        assert!(visible_lifetime_ok("tag-a", Some("tag-a")));
        assert!(!visible_lifetime_ok("tag-a", Some("tag-b")));
        assert!(!visible_lifetime_ok("tag-a", None));
        assert!(!visible_lifetime_ok("", Some("tag-a")));
        // Stale same-HWND keys drop; hidden claims never repair away.
        let hidden: BTreeSet<WindowKey> = BTreeSet::from([key(21, "sold")]);
        let known = vec![key(21, "sold"), key(21, "snew")];
        assert_eq!(
            reused_hwnd_stale(&known, &BTreeSet::new(), &key(21, "snew")),
            vec![key(21, "sold")]
        );
        assert!(
            reused_hwnd_stale(&known, &hidden, &key(21, "snew")).is_empty(),
            "hidden claims own their HWND through the ledger path"
        );
        assert!(reused_hwnd_stale(&known, &BTreeSet::new(), &key(22, "other")).is_empty());
    }

    #[test]
    fn output_context_prefers_foreground_then_pointer_then_cache() {
        let keys = vec!["mon-a".to_owned(), "mon-b".to_owned()];
        assert_eq!(
            output_context_keys(&keys, Some("mon-a"), Some("mon-b"), Some("mon-a")),
            Some("mon-b".to_owned())
        );
        assert_eq!(
            output_context_keys(&keys, Some("mon-a"), None, Some("mon-b")),
            Some("mon-b".to_owned())
        );
        assert_eq!(
            output_context_keys(&keys, Some("mon-a"), None, None),
            Some("mon-a".to_owned())
        );
        assert_eq!(
            output_context_keys(&keys, Some("mon-gone"), None, None),
            Some("mon-a".to_owned())
        );
        assert!(output_context_keys(&[], Some("mon-a"), None, None).is_none());
    }

    #[test]
    fn output_context_live_foreground_beats_live_stale_cache() {
        // Live focus/cache disagreement: focus moved to mon-b while the
        // last-switch cache still names the live mon-a. The cached value
        // stays last, so migration/chord source resolution follows live
        // focus instead of pinning the stale output.
        let keys = vec!["mon-a".to_owned(), "mon-b".to_owned()];
        assert_eq!(
            output_context_keys(&keys, Some("mon-a"), Some("mon-b"), None),
            Some("mon-b".to_owned())
        );
    }

    #[test]
    fn hidden_snapshot_prefers_float_lane_with_tiled_fallback() {
        let float = Some(rect(1, 2, 3, 4));
        let slot = Some(rect(5, 6, 7, 8));
        assert_eq!(hidden_snapshot_rect(true, float, slot), float);
        assert_eq!(hidden_snapshot_rect(true, None, slot), slot);
        assert_eq!(hidden_snapshot_rect(false, float, slot), slot);
        assert_eq!(hidden_snapshot_rect(false, float, None), float);
        assert_eq!(hidden_snapshot_rect(true, None, None), None);
    }

    #[test]
    fn writable_subset_never_includes_hidden_or_stale() {
        use std::collections::HashSet;
        let shown = key(31, "s");
        let hidden = key(32, "h");
        let members: BTreeSet<WindowKey> = BTreeSet::from([shown.clone(), hidden.clone()]);
        let token_of: BTreeMap<WindowKey, String> = BTreeMap::from([
            (shown.clone(), "wA".to_owned()),
            (hidden.clone(), "wC".to_owned()),
        ]);
        let fresh: HashSet<String> = HashSet::from(["wA".to_owned(), "wC".to_owned()]);
        let writable = writable_subset(&members, |k| k == &hidden, &token_of, &fresh);
        assert!(!writable.contains("wC"), "hidden target never writable");
        assert!(writable.contains("wA"), "visible survivor stays writable");
        let stale: HashSet<String> = HashSet::new();
        assert!(
            writable_subset(&members, |_| false, &token_of, &stale).is_empty(),
            "retained-only tokens never take writes"
        );
    }

    #[test]
    fn restart_markers_classify_sticky_first_with_exact_eligibility() {
        use MarkerSlot::{Absent, Unreadable, Value};
        // Sticky wins over float and tile-override lanes.
        let sticky = classify_restart_marker(Value(7), Value(1), Value(1));
        assert_eq!(sticky, RestartMarker::Sticky(false));
        assert_eq!(
            restart_sticky_eligible(true, true, true, sticky),
            Some(false)
        );
        assert!(!restart_float_eligible(true, true, true, sticky));
        assert!(!restart_tile_override_eligible(true, true, true, sticky));
        // Ordinary float intent floats; tile override never floats.
        let float = classify_restart_marker(Value(1), Absent, Absent);
        assert_eq!(float, RestartMarker::FloatIntent);
        assert!(restart_float_eligible(true, true, true, float));
        assert_eq!(restart_sticky_eligible(true, true, true, float), None);
        // Unreadable with no valid lane diagnoses without intent.
        let unreadable = classify_restart_marker(Unreadable, Absent, Absent);
        assert_eq!(unreadable, RestartMarker::Unreadable);
        assert!(!restart_float_eligible(true, true, true, unreadable));
        // Present-but-wrong values corrupt without intent.
        let corrupt = classify_restart_marker(Value(7), Value(3), Value(9));
        assert_eq!(corrupt, RestartMarker::Corrupt);
        assert!(!restart_float_eligible(true, true, true, corrupt));
        assert_eq!(
            classify_restart_marker(Absent, Absent, Absent),
            RestartMarker::Absent
        );
        // Every eligibility needs admission, identity, and lifetime.
        assert!(!restart_float_eligible(false, true, true, float));
        assert!(!restart_float_eligible(true, false, true, float));
        assert!(!restart_float_eligible(true, true, false, float));
    }

    #[test]
    fn foreground_classifier_keeps_gates_but_fixes_desktop_and_hidden() {
        let covering = ForegroundFacts {
            valid: true,
            is_desktop: false,
            visible: true,
            captioned: false,
            dwm_readable: true,
            cloak_readable: true,
            cloaked: false,
            covers_monitor: true,
        };
        assert_eq!(
            classify_foreground(covering),
            ForegroundVeto {
                block: true,
                reason: ForegroundVetoReason::Fullscreen
            }
        );
        // Exact desktop and invisible handles never veto; validity and
        // readability fail closed; cloak exempts; captions exempt.
        assert!(
            !classify_foreground(ForegroundFacts {
                is_desktop: true,
                ..covering
            })
            .block
        );
        assert!(
            !classify_foreground(ForegroundFacts {
                visible: false,
                ..covering
            })
            .block
        );
        assert!(
            classify_foreground(ForegroundFacts {
                valid: false,
                ..covering
            })
            .block
        );
        assert!(
            classify_foreground(ForegroundFacts {
                dwm_readable: false,
                ..covering
            })
            .block
        );
        assert!(
            classify_foreground(ForegroundFacts {
                cloak_readable: false,
                ..covering
            })
            .block
        );
        assert!(
            !classify_foreground(ForegroundFacts {
                cloaked: true,
                ..covering
            })
            .block
        );
        assert!(
            !classify_foreground(ForegroundFacts {
                captioned: true,
                ..covering
            })
            .block
        );
        assert!(
            !classify_foreground(ForegroundFacts {
                covers_monitor: false,
                ..covering
            })
            .block
        );
    }

    #[test]
    fn output_at_point_resolves_pointer_output_without_adjacency() {
        let outputs = vec![
            DirectionalOutput {
                id: "a".to_owned(),
                full: rect(0, 0, 1920, 1080),
            },
            DirectionalOutput {
                id: "b".to_owned(),
                full: rect(1920, 0, 1920, 1080),
            },
        ];
        assert_eq!(output_at_point(&outputs, 10, 10), Some("a".to_owned()));
        assert_eq!(output_at_point(&outputs, 2000, 10), Some("b".to_owned()));
        assert_eq!(output_at_point(&outputs, 5000, 10), None);
        assert_eq!(output_at_point(&[], 10, 10), None);
        // Degenerate rectangles never contain: left/top edges inclusive,
        // zero-area excluded.
        let flat = vec![DirectionalOutput {
            id: "z".to_owned(),
            full: rect(0, 0, 0, 1080),
        }];
        assert_eq!(output_at_point(&flat, 0, 10), None);
    }

    #[test]
    fn directional_select_needs_edge_touch_and_known_source() {
        let outputs = vec![
            DirectionalOutput {
                id: "left".to_owned(),
                full: rect(0, 0, 1920, 1080),
            },
            DirectionalOutput {
                id: "right".to_owned(),
                full: rect(1920, 0, 1920, 1080),
            },
        ];
        let mover = rect(1800, 400, 200, 200);
        assert_eq!(
            select_directional_target(Direction::Right, "left", &mover, &outputs)
                .expect("readable"),
            Some("right".to_owned())
        );
        assert!(
            select_directional_target(Direction::Left, "left", &mover, &outputs)
                .expect("readable")
                .is_none(),
            "no candidate past the outer edge"
        );
        assert!(
            select_directional_target(Direction::Right, "ghost", &mover, &outputs).is_err(),
            "unknown source fails closed"
        );
        assert!(directional_focus_crosses(Direction::Left));
        assert!(directional_focus_crosses(Direction::Right));
        assert!(!directional_focus_crosses(Direction::Up));
        assert!(!directional_focus_crosses(Direction::Down));
    }

    fn fence_snapshot() -> DirectionalFenceSnapshot {
        DirectionalFenceSnapshot {
            inner_gap: 8,
            outer_gap: 8,
            source_output: "mon-a".to_owned(),
            source_workspace: "ws-1".to_owned(),
            target_output: "mon-b".to_owned(),
            target_workspace: "ws-2".to_owned(),
            source_tiled: true,
            target_tiled: true,
            direction: Direction::Right,
            overlay_maximized: false,
            overlay_fullscreen: false,
            source_full: rect(0, 0, 1920, 1080),
            target_full: rect(1920, 0, 1920, 1080),
            source_bounds: rect(0, 0, 1920, 1040),
            target_bounds: rect(1920, 0, 1920, 1040),
            owner: "owner".to_owned(),
            generation: "gen".to_owned(),
        }
    }

    fn fence_live() -> DirectionalFenceLive {
        DirectionalFenceLive {
            inner_gap: 8,
            outer_gap: 8,
            source_tiled: true,
            target_tiled: true,
            source_is_active: true,
            target_is_active: true,
            topology_ok: true,
            scope_ok: true,
            lifetime_ok: true,
            membership_ok: true,
            overlay_maximized: false,
            overlay_fullscreen: false,
            owner_ok: true,
        }
    }

    #[test]
    fn directional_fences_pin_every_tail_gate() {
        let snap = fence_snapshot();
        assert_eq!(directional_fences_hold(&snap, &fence_live()), None);
        // Direction-free drag-drop view shares the tokens minus direction.
        let drag = drag_snapshot_for_directional(&snap);
        assert_eq!(drag.source_output, snap.source_output);
        assert_eq!(drag.target_workspace, snap.target_workspace);
        let mut live = fence_live();
        assert_eq!(drag_drop_fences_hold(&drag, &live), None);
        live.owner_ok = false;
        assert_eq!(directional_fences_hold(&snap, &live), Some("owner-changed"));
        assert_eq!(drag_drop_fences_hold(&drag, &live), Some("owner-changed"));
        live = fence_live();
        live.source_tiled = false;
        assert_eq!(
            directional_fences_hold(&snap, &live),
            Some("workspace-floating")
        );
        assert_eq!(
            drag_drop_fences_hold(&drag, &live),
            Some("workspace-floating")
        );
        live = fence_live();
        live.source_is_active = false;
        assert_eq!(directional_fences_hold(&snap, &live), Some("view-changed"));
        live = fence_live();
        live.membership_ok = false;
        assert_eq!(
            directional_fences_hold(&snap, &live),
            Some("origin-vanished")
        );
        live = fence_live();
        live.overlay_fullscreen = true;
        assert_eq!(
            directional_fences_hold(&snap, &live),
            Some("overlay-changed")
        );
    }

    fn send_snapshot() -> SendFenceSnapshot {
        SendFenceSnapshot {
            inner_gap: 8,
            outer_gap: 8,
            source_tiled: true,
            target_tiled: true,
            overlay_maximized: false,
            overlay_fullscreen: false,
        }
    }

    fn send_live() -> SendFenceLive {
        SendFenceLive {
            inner_gap: 8,
            outer_gap: 8,
            source_tiled: true,
            target_tiled: true,
            active_is_source: true,
            target_is_active: false,
            scope_ok: true,
            lifetime_ok: true,
            membership_ok: true,
            overlay_maximized: false,
            overlay_fullscreen: false,
        }
    }

    #[test]
    fn send_fences_pin_view_mode_gap_scope_lifetime_membership_and_overlay() {
        assert_eq!(send_fences_hold(send_snapshot(), send_live()), None);
        let mut live = send_live();
        live.active_is_source = false;
        assert_eq!(
            send_fences_hold(send_snapshot(), live),
            Some("view-changed")
        );
        live = send_live();
        live.source_tiled = false;
        assert_eq!(
            send_fences_hold(send_snapshot(), live),
            Some("mode-changed")
        );
        live = send_live();
        live.inner_gap = 4;
        assert_eq!(send_fences_hold(send_snapshot(), live), Some("deferred"));
        live = send_live();
        live.scope_ok = false;
        assert_eq!(
            send_fences_hold(send_snapshot(), live),
            Some("scope-excluded")
        );
        live = send_live();
        live.lifetime_ok = false;
        assert_eq!(
            send_fences_hold(send_snapshot(), live),
            Some("identity-changed")
        );
        live = send_live();
        live.membership_ok = false;
        assert_eq!(send_fences_hold(send_snapshot(), live), Some("unverified"));
        // Newly arrived fullscreen refuses; stable overlay carries.
        live = send_live();
        live.overlay_fullscreen = true;
        assert_eq!(
            send_fences_hold(send_snapshot(), live),
            Some("send-refused-fullscreen")
        );
        let snap_max = SendFenceSnapshot {
            overlay_maximized: true,
            ..send_snapshot()
        };
        let mut carried = send_live();
        carried.overlay_maximized = true;
        assert_eq!(send_fences_hold(snap_max, carried), None);
    }

    #[test]
    fn send_overlay_gate_carries_stable_refuses_arrival_defers_drift() {
        assert_eq!(
            send_overlay_gate(false, false, false, Some(false), false),
            Ok(())
        );
        assert_eq!(
            send_overlay_gate(false, true, false, Some(true), true),
            Ok(()),
            "stable fullscreen carries like maximized"
        );
        assert_eq!(
            send_overlay_gate(false, false, false, Some(true), false),
            Err("send-refused-fullscreen")
        );
        assert_eq!(
            send_overlay_gate(true, false, false, Some(false), false),
            Err("deferred")
        );
        assert_eq!(
            send_overlay_gate(false, true, false, None, true),
            Err("deferred"),
            "unreadable live flag defers"
        );
    }

    #[test]
    fn directional_refuses_both_focus_and_move_on_floating() {
        use crate::snapkey::SnapOp;
        assert_eq!(
            directional_workspace_refusal(SnapOp::Focus, false),
            Some("focus-refused-workspace-floating")
        );
        assert_eq!(
            directional_workspace_refusal(SnapOp::Move, false),
            Some("move-refused-workspace-floating")
        );
        assert_eq!(directional_workspace_refusal(SnapOp::Focus, true), None);
        assert_eq!(directional_workspace_refusal(SnapOp::Move, true), None);
    }
}

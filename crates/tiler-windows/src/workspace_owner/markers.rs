#![forbid(unsafe_code)]
/// One on-window marker slot read (item 8, R-RST-01/R-FLT-05). `Value`
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

/// Focus-before-geometry gate for a verified workspace transition: establish
/// the appropriate target focus before geometry only when the transition
/// verified and no fullscreen/elevated foreground arrived. A real fullscreen
/// or elevated arrival must never be stolen from just to defeat the geometry
/// veto; skipping focus there lets the write path veto honestly.
#[must_use]
pub const fn focus_before_geometry(
    transition_verified: bool,
    fullscreen_foreground: bool,
    elevated_foreground: bool,
) -> bool {
    transition_verified && !fullscreen_foreground && !elevated_foreground
}

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

use tiler_core::geometry::Rect;

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

/// Hidden float routing for one member: sticky wins unconditionally; the
/// D4 automatic-fixed latch floats unless an explicit tile win dominates;
/// marker-derived adapter float intent (floats, restart-holds, born holds)
/// floats unless a first-exit mark already returned the member to the tiled
/// lane. Pure predicate shared by row assembly and hide-commit snapshot
/// seeding so both route the same member to the same cache.
#[must_use]
#[allow(clippy::too_many_arguments)]
pub const fn float_lane(
    sticky: bool,
    fixed_auto: bool,
    fixed_suppress: bool,
    born_first_exit: bool,
    floated: bool,
    restart_hold: bool,
    born_fullscreen: bool,
    hidden: bool,
) -> bool {
    if sticky {
        return true;
    }
    if fixed_auto && !fixed_suppress {
        return true;
    }
    if born_first_exit && (!hidden || !floated) {
        return false;
    }
    floated || restart_hold || born_fullscreen
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

// Stage-4a note: the drag-drop fence snapshot shape is deleted with the adapter settle route.

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
}

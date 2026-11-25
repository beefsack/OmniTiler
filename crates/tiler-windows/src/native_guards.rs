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

/// Choose the float cache for a hidden snapshot or hide-commit seed.
/// Sticky wins, then a fixed automatic float without explicit tile intent;
/// a born-fullscreen hold ends at first exit unless the member opted to float.
#[must_use]
pub const fn hidden_snapshot_uses_float(
    sticky: bool,
    fixed_auto: bool,
    born_first_exit: bool,
    float_intent: Option<bool>,
    born_fullscreen: bool,
) -> bool {
    sticky
        || (fixed_auto && !matches!(float_intent, Some(false)))
        || matches!(float_intent, Some(true))
        || (!born_first_exit && born_fullscreen)
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

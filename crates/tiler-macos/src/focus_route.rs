#![forbid(unsafe_code)]

//! Bounded focus dispatch for scoped-tile focus requests.
//!
//! Pure routing plus a thin native seam. Focus requests route to the SAME
//! per-app AX owner that observed the window; dispatch here never verifies
//! focus — only a later observation (workspace frontmost plus AX focused
//! preimage agreement) is truth.
//!
//! - Default route is [`FocusRoute::Slps`]: the bounded SLPS dispatcher from
//!   [`crate::focus::dispatch_focus`]. Pending live agreement, per the unit
//!   brief.
//! - [`FocusRoute::PublicApiSeam`] is the public-API fallback seam.
//!   Controller-level dispatch here is bare activation only (same target,
//!   never automatic wider activation) and never claims arrival. The
//!   authoritative seam runs on the owning per-app AX thread (see
//!   [`crate::native_tile_backend`]): it sets the app's exact `AXFocusedWindow`
//!   to the joined window element, raises exactly that window, then
//!   activates exactly that PID — explicitly only, with each leg reported.
//!   Bare activation alone is not target focus and is never treated as such.
//! - [`global_focus_agrees`] is the input-focus truth gate: an app-local
//!   focused window from a background app is NEVER accepted as global focus.
//!   The SLPS front-process PID, the workspace-frontmost PID, the selected
//!   direct per-app window (focused, else main fallback), and CG ownership
//!   must ALL agree on the wanted target; anything else refuses. The
//!   system-wide `AXFocusedApplication` read is diagnostic-only and never
//!   feeds this gate; `AXFrontmost` never gates either. No titles, no
//!   raw input logs: booleans and IDs only.

use serde::{Deserialize, Serialize};

/// Which native route a scoped focus request dispatches through.
/// Serialized in diagnostics only; never user content.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FocusRoute {
    /// Bounded SLPS route (default, pending live agreement).
    #[default]
    Slps,
    /// Public-API fallback seam (explicit opt-in per request, same target
    /// only; never automatic wider activation).
    PublicApiSeam,
}

impl FocusRoute {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Slps => "slps",
            Self::PublicApiSeam => "public-api-seam",
        }
    }
}

/// Pure global-focus agreement: true only when the SLPS front-process PID,
/// the workspace frontmost PID, the selected direct per-app AX focus, and
/// CG ownership ALL agree on (`want_pid`, `want_wid`). The AX leg here is
/// the selected oracle's direct read (focused, else main fallback) — never
/// the diagnostic system-wide leg. An app-local focused window reported by a
/// background app (`app_local`) is never sufficient on its own — when the
/// frontmost PID or the AX PID differs from `want_pid`, this refuses even
/// if `app_local` names the window.
#[must_use]
pub fn global_focus_agrees(
    want_pid: u32,
    want_wid: u32,
    frontmost_pid: Option<u32>,
    ax_focus: Option<(u32, Option<u32>)>,
    cg_owned: bool,
) -> bool {
    if want_pid <= 1 || want_wid == 0 || !cg_owned {
        return false;
    }
    let Some(front) = frontmost_pid else {
        return false;
    };
    let Some((ax_pid, ax_wid)) = ax_focus else {
        return false;
    };
    front == want_pid && ax_pid == want_pid && ax_wid == Some(want_wid)
}

/// Pure global-focus verdict over one scoped snapshot's focus legs: returns
/// the promoted `(pid, wid)` only when the selected-oracle AX leg (the
/// owner-thread direct read: focused, else main fallback), the app-local
/// focused window, the CG owner join, and the main-thread frontmost ALL
/// agree on the reporting app's window. Anything else yields `None`
/// (arrives unfocused — authoritative loss, never a guess).
///
/// This is the single promotion rule shared by the worker send path (fresh
/// frontmost at send time, so evidence changes re-resolve even when the
/// window snapshot is identical) and the manual submit path. The AX leg
/// must be a real bounded owner-thread direct read; passing the app-local
/// window in its place fabricates agreement and is never done. The raw
/// system-wide leg never stands in for it.
#[must_use]
pub fn resolve_global_focus(
    pid: u32,
    focused_wid: Option<u32>,
    sys_focus: Option<(u32, Option<u32>)>,
    cg_owned: bool,
    frontmost_pid: Option<u32>,
) -> Option<(u32, u32)> {
    let wid = focused_wid?;
    if !global_focus_agrees(pid, wid, frontmost_pid, sys_focus, cg_owned) {
        return None;
    }
    Some((pid, wid))
}

/// Outcome of one bounded focus dispatch. `sent` means the OS accepted the
/// request; arrival is async and must be verified separately via
/// [`crate::focus::read_agreement`]. Status codes only, never content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FocusDispatchSummary {
    pub pid: u32,
    pub wid: u32,
    pub route: FocusRoute,
    pub sent: bool,
    pub detail: String,
}

/// Dispatch one scoped focus request through `route` with a finite wait.
/// SLPS failures and public-API refusals report the cause; nothing here
/// claims arrival. Off-platform builds fail closed without touching native
/// state.
pub fn dispatch_focus_bounded(pid: u32, wid: u32, route: FocusRoute) -> FocusDispatchSummary {
    let refused = |detail: String| FocusDispatchSummary {
        pid,
        wid,
        route,
        sent: false,
        detail,
    };
    if pid <= 1 {
        return refused(format!("refusing reserved pid {pid}"));
    }
    if wid == 0 {
        return refused("refusing window id 0".to_owned());
    }
    match route {
        FocusRoute::Slps => match crate::focus::dispatch_focus(pid, wid) {
            Ok(dispatch) => FocusDispatchSummary {
                pid,
                wid,
                route,
                sent: true,
                detail: format!(
                    "slps set_front={} posts={:?} latency_ms={}",
                    dispatch.set_front_status, dispatch.post_statuses, dispatch.dispatch_latency_ms
                ),
            },
            Err(e) => refused(format!("slps: {e}")),
        },
        FocusRoute::PublicApiSeam => {
            // Same-target public activation only. `activate_app` returns
            // whether the request was sent; arrival still needs the
            // agreement oracle. Never a wider activation: the target is
            // exactly (pid, wid) and anything else is out of scope.
            if crate::ffi::activate_app(pid as i32) {
                FocusDispatchSummary {
                    pid,
                    wid,
                    route,
                    sent: true,
                    detail: "public-api-seam sent (unverified)".to_owned(),
                }
            } else {
                refused("public-api-seam refused".to_owned())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_route_is_slps() {
        assert_eq!(FocusRoute::default(), FocusRoute::Slps);
        assert_eq!(FocusRoute::Slps.as_str(), "slps");
        assert_eq!(FocusRoute::PublicApiSeam.as_str(), "public-api-seam");
    }

    #[test]
    fn global_focus_requires_full_agreement() {
        // All three agree: truth.
        assert!(global_focus_agrees(
            100,
            7,
            Some(100),
            Some((100, Some(7))),
            true
        ));
        // App-local focused window from a background app is NOT global
        // focus: frontmost elsewhere refuses.
        assert!(!global_focus_agrees(
            100,
            7,
            Some(200),
            Some((100, Some(7))),
            true
        ));
        // AX focused elsewhere refuses.
        assert!(!global_focus_agrees(
            100,
            7,
            Some(100),
            Some((200, Some(7))),
            true
        ));
        // AX wid mismatch refuses.
        assert!(!global_focus_agrees(
            100,
            7,
            Some(100),
            Some((100, Some(8))),
            true
        ));
        // Missing CG ownership refuses.
        assert!(!global_focus_agrees(
            100,
            7,
            Some(100),
            Some((100, Some(7))),
            false
        ));
        // Unreadable sides refuse.
        assert!(!global_focus_agrees(
            100,
            7,
            None,
            Some((100, Some(7))),
            true
        ));
        assert!(!global_focus_agrees(100, 7, Some(100), None, true));
        // Reserved/zero identities refuse before any read.
        assert!(!global_focus_agrees(
            1,
            7,
            Some(1),
            Some((1, Some(7))),
            true
        ));
        assert!(!global_focus_agrees(
            100,
            0,
            Some(100),
            Some((100, Some(0))),
            true
        ));
    }

    #[test]
    fn bounded_dispatch_refuses_reserved_identities_before_native_touch() {
        for route in [FocusRoute::Slps, FocusRoute::PublicApiSeam] {
            let r = dispatch_focus_bounded(1, 7, route);
            assert!(!r.sent, "{route:?}");
            let r = dispatch_focus_bounded(100, 0, route);
            assert!(!r.sent, "{route:?}");
        }
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn off_macos_dispatch_fails_closed() {
        // No SLPS symbols and no activation target off macOS: refused, never
        // a pretend send.
        let r = dispatch_focus_bounded(100, 7, FocusRoute::Slps);
        assert!(!r.sent);
        let r = dispatch_focus_bounded(100, 7, FocusRoute::PublicApiSeam);
        assert!(!r.sent);
    }

    #[test]
    fn resolve_promotes_only_on_full_agreement() {
        // All four legs agree: promotes.
        assert_eq!(
            resolve_global_focus(100, Some(7), Some((100, Some(7))), true, Some(100)),
            Some((100, 7))
        );
        // Selected direct read names another app: withholds even though
        // local, CG, and frontmost agree (this is the fabrication guard: a
        // local echo of the app-local window must never stand in for it).
        assert_eq!(
            resolve_global_focus(100, Some(7), Some((200, Some(7))), true, Some(100)),
            None
        );
        // Selected read agrees but frontmost moved on: withholds.
        assert_eq!(
            resolve_global_focus(100, Some(7), Some((100, Some(7))), true, Some(200)),
            None
        );
        // Missing legs withhold.
        assert_eq!(
            resolve_global_focus(100, Some(7), None, true, Some(100)),
            None
        );
        assert_eq!(
            resolve_global_focus(100, Some(7), Some((100, Some(7))), false, Some(100)),
            None
        );
        assert_eq!(
            resolve_global_focus(100, Some(7), Some((100, Some(7))), true, None),
            None
        );
        assert_eq!(
            resolve_global_focus(100, None, Some((100, Some(7))), true, Some(100)),
            None
        );
    }

    #[test]
    fn selected_oracle_prefers_perapp_focused_over_main() {
        // Direct per-app focused leg joined: selects focused, never main.
        assert_eq!(
            crate::ffi::select_direct_window(Some(7), Some(8)),
            (Some(7), "focused")
        );
        assert_eq!(
            crate::ffi::select_direct_window(Some(7), None),
            (Some(7), "focused")
        );
    }

    #[test]
    fn selected_oracle_falls_back_to_main_only_when_focused_missing() {
        // Unavailable/missing focused leg: the main fallback joins instead.
        assert_eq!(
            crate::ffi::select_direct_window(None, Some(8)),
            (Some(8), "main")
        );
        // Zero focused WID is missing, so a usable main still selects.
        assert_eq!(
            crate::ffi::select_direct_window(Some(0), Some(8)),
            (Some(8), "main")
        );
    }

    #[test]
    fn selected_oracle_refuses_unknown_and_zero_windows() {
        // Both legs unknown: no selection (refuse downstream, never guess).
        assert_eq!(crate::ffi::select_direct_window(None, None), (None, "none"));
        // Zero WIDs are missing on every leg combination.
        assert_eq!(
            crate::ffi::select_direct_window(Some(0), None),
            (None, "none")
        );
        assert_eq!(
            crate::ffi::select_direct_window(None, Some(0)),
            (None, "none")
        );
        assert_eq!(
            crate::ffi::select_direct_window(Some(0), Some(0)),
            (None, "none")
        );
        // Agreement withholds on a missing selection even when the
        // foreground agrees and CG holds.
        assert!(!crate::ffi::selected_focus_agrees(
            100,
            7,
            Some(100),
            Some(100),
            None,
            true
        ));
        // Agreement withholds on a zero target WID.
        assert!(!crate::ffi::selected_focus_agrees(
            100,
            0,
            Some(100),
            Some(100),
            Some(0),
            true
        ));
    }

    #[test]
    fn selected_oracle_refuses_foreground_mismatch() {
        // SLPS/workspace disagreement refuses even with the exact window
        // selected and CG owned.
        assert!(!crate::ffi::selected_focus_agrees(
            100,
            7,
            Some(200),
            Some(100),
            Some(7),
            true
        ));
        assert!(!crate::ffi::selected_focus_agrees(
            100,
            7,
            Some(100),
            Some(200),
            Some(7),
            true
        ));
        // Unknown foreground legs refuse.
        assert!(!crate::ffi::selected_focus_agrees(
            100,
            7,
            None,
            Some(100),
            Some(7),
            true
        ));
        assert!(!crate::ffi::selected_focus_agrees(
            100,
            7,
            Some(100),
            None,
            Some(7),
            true
        ));
        // Non-exact selected window refuses.
        assert!(!crate::ffi::selected_focus_agrees(
            100,
            7,
            Some(100),
            Some(100),
            Some(8),
            true
        ));
    }

    #[test]
    fn selected_oracle_refuses_without_cg_ownership() {
        // Foreground plus exact window agree, but the CG join is lost:
        // refuses (never claims).
        assert!(!crate::ffi::selected_focus_agrees(
            100,
            7,
            Some(100),
            Some(100),
            Some(7),
            false
        ));
        // Full agreement passes.
        assert!(crate::ffi::selected_focus_agrees(
            100,
            7,
            Some(100),
            Some(100),
            Some(7),
            true
        ));
    }
}

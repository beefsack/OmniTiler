#![forbid(unsafe_code)]
//! Send target selector plus carried pre-transfer fences (world route).
//!
//! The core owns send resolution outright
//! ([`WorldCommand::SendNumbered`](tiler_core::world_report::WorldCommand::SendNumbered)
//! / `SendRelative`): this module only names the operator-facing target
//! selector and carries the pre-transfer fence snapshot into world-routed
//! tails. No rings, no history, no trailing, no Engine plan.

use super::*;

/// Send target selector resolved once before transfer (operator intent):
/// numbered digits address the scoped ring positionally (`0` is the trailing
/// path), relative steps move from the mover source. The core resolves the
/// frozen target; filling the trailing empty is ordinary lifecycle there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SendTarget {
    Numbered(u8),
    Relative(i32),
}

impl SendTarget {
    pub(super) fn log_index(self) -> u8 {
        match self {
            Self::Numbered(index) => index,
            Self::Relative(_) => 0,
        }
    }

    pub(super) fn log_op(self) -> &'static str {
        match self {
            Self::Numbered(_) => "send",
            Self::Relative(delta) if delta < 0 => "send-prev",
            Self::Relative(_) => "send-next",
        }
    }
}

/// Carried pre-transfer send fences: the pure fence snapshot plus the
/// retained-row fullscreen fallback for unreadable live frames. World-routed
/// tails re-verify overlay liveness before native effects; the core owns
/// every other fence.
#[derive(Debug, Clone, Copy)]
pub(super) struct SendPreflight {
    pub(super) fences: crate::native_guards::SendFenceSnapshot,
    pub(super) fallback_fullscreen: bool,
}

impl SendPreflight {
    /// Live tail-time readings for [`crate::native_guards::send_fences_hold`]:
    /// scope/lifetime/overlay state re-read live. View/mode/membership ride
    /// the adapter receipts (`applied_views`/`applied_homing`), never a local
    /// ring; the core remains the sole membership authority.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn live(
        &self,
        state: &TileLoop,
        stored: &ProcessIdentity,
        mover_key: &crate::native_guards::WindowKey,
        mover_hwnd: u64,
        output: &str,
        source_id: &str,
        target_id: &str,
        fulls: &[Rect],
    ) -> crate::native_guards::SendFenceLive {
        let wire = state
            .output_tokens
            .get(output)
            .cloned()
            .unwrap_or_default();
        let active_is_source = state
            .applied_views
            .get(&wire)
            .is_some_and(|view| view == source_id);
        let target_is_active = state
            .applied_views
            .get(&wire)
            .is_some_and(|view| view == target_id);
        let (live_max, live_full_opt) = live_overlay_flags(mover_hwnd, fulls);
        let token = state.member_tokens.get(mover_key).cloned().unwrap_or_default();
        let membership_ok = state
            .applied_homing
            .get(&token)
            .is_some_and(|(home_output, home_ws)| home_output == &wire && home_ws == target_id);
        crate::native_guards::SendFenceLive {
            inner_gap: state.inner_gap,
            outer_gap: state.outer_gap,
            source_tiled: self.fences.source_tiled,
            target_tiled: self.fences.target_tiled,
            active_is_source,
            target_is_active,
            scope_ok: scope_allows(&state.scope, &stored.exe_path)
                && hosted_gate_allows(&stored.exe_path, mover_hwnd, stored.pid, &state.scope_hosts),
            lifetime_ok: {
                let live_tag = live_member_tag(mover_hwnd);
                state.member_tags.get(mover_key).is_some_and(|tag| {
                    crate::native_guards::visible_lifetime_ok(tag, live_tag.as_deref())
                })
            },
            membership_ok,
            overlay_maximized: live_max,
            overlay_fullscreen: live_full_opt.unwrap_or(self.fallback_fullscreen),
        }
    }

    /// Live overlay-only recheck before native effects: a newly arrived
    /// fullscreen refuses, other drift defers.
    pub(super) fn overlay_hold(
        &self,
        mover_hwnd: u64,
        fulls: &[Rect],
    ) -> std::result::Result<(), &'static str> {
        let (live_max, live_full_opt) = live_overlay_flags(mover_hwnd, fulls);
        crate::native_guards::send_overlay_gate(
            self.fences.overlay_maximized,
            self.fences.overlay_fullscreen,
            live_max,
            live_full_opt,
            self.fallback_fullscreen,
        )
    }
}

#![forbid(unsafe_code)]
//! Send target selector plus carried pre-transfer fences (world route).
//!
//! The core owns send resolution outright
//! ([`WorldCommand::SendNumbered`](tiler_core::world_report::WorldCommand::SendNumbered)
//! / `SendRelative`): this module only names the operator-facing target
//! selector and carries the pre-transfer fence snapshot into world-routed
//! tails. No rings, no history, no trailing, no Engine plan.

#[cfg(test)]
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
#[cfg(test)]
#[derive(Debug, Clone, Copy)]
pub(super) struct SendPreflight {
    pub(super) fences: crate::native_guards::SendFenceSnapshot,
    pub(super) fallback_fullscreen: bool,
}

#[cfg(test)]
impl SendPreflight {
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

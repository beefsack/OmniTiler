#![forbid(unsafe_code)]
//! Send target selector (world route).
//!
//! The core owns send resolution outright
//! ([`WorldCommand::SendNumbered`](tiler_core::world_report::WorldCommand::SendNumbered)
//! / `SendRelative`): this module only names the operator-facing target
//! selector. No rings, no history, no trailing, no Engine plan.

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

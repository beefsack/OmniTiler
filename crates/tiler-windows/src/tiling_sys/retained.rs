#![forbid(unsafe_code)]
use super::*;

/// One hidden member: the committed ledger claim (carrying the durable
/// show-state preimage, the authority for independent restore) plus the iconic
/// state at hide time for the in-memory reveal fast path and preimage-less
/// legacy claims.
#[derive(Debug, Clone)]
pub(super) struct HiddenRecord {
    pub(super) claim: crate::model::WindowIdentity,
    pub(super) iconic: bool,
}

/// D23 verified-close homing: the owning `(output, workspace)` read while the
/// window was live. A bare absence from observation stays a minimize (stored
/// old slot); only a queued id rides the next valid domain observation as
/// `closed_windows` so the Engine forgets the slot instead of minting one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PendingClosed {
    pub(super) output: String,
    pub(super) workspace: String,
}

/// One retained-occupancy member: identity resolved but no tileable frame
/// (minimized without a DWM frame) or an ineligible state with a fresh
/// frame (maximized, fullscreen, cloaked). `rect` is `None` only when no
/// frame exists; the caller falls back to the last-known snapshot so hidden
/// Engine membership and layout survive. `maximized`/`fullscreen` carry the
/// fresh overlay flags for this tick so directional/pointer routes can
/// refuse before any Engine mutation (KDE maximize/fullscreen isolation
/// parity) and admission can clear a first-seen maximize exactly once.
#[derive(Debug, Clone)]
pub(super) struct RetainedRow {
    pub(super) key: crate::native_guards::WindowKey,
    pub(super) token: String,
    pub(super) rect: Option<Rect>,
    pub(super) maximized: bool,
    pub(super) fullscreen: bool,
    /// Fresh facts for observed-ineligible rows (`None` for frameless known
    /// rows): lets admission clear only otherwise-eligible maximized members.
    pub(super) facts: Option<WindowFacts>,
}

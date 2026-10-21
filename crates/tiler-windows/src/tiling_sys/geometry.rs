#![forbid(unsafe_code)]
use super::*;

/// Settled result of one geometry application pass: verified writes,
/// readback mismatches, and whether the independent readback observation ran
/// at all, plus the first privacy-safe veto diagnostic when a write vetoed.
/// Callers map this to their action outcome vocabulary instead of assuming
/// success. `veto` is `None` when no write vetoed: the action line serializes
/// null with reason, never zeros implying applied.
#[derive(Debug, Clone, Copy)]
pub(super) struct ApplySummary {
    pub(super) applied: usize,
    pub(super) mismatched: usize,
    pub(super) readback_ok: bool,
    pub(super) veto: Option<VetoDiag>,
}

/// Bounded privacy-safe veto diagnostic: reason token plus the native
/// booleans behind it. No handles, paths, PIDs, titles, or content; opaque
/// managed tokens travel in the existing skip list only.
#[derive(Debug, Clone, Copy)]
pub(super) struct VetoDiag {
    pub(super) reason: &'static str,
    pub(super) desktop: bool,
    pub(super) visible: bool,
    pub(super) captioned: bool,
    pub(super) dwm_readable: bool,
    pub(super) cloaked: bool,
}

pub(super) fn audit_json(state: &TileLoop, value: serde_json::Value) {
    if let Some(path) = state.audit_path.as_ref() {
        log_json_at(path, value);
    }
}

#![forbid(unsafe_code)]
use super::*;

/// Overlay refusal for one verified member before any Engine mutation (KDE
/// move/pointer isolation parity): fullscreen wins, then maximize. Returns
/// the refusal cause or `None` when the route may proceed.
pub(super) fn overlay_refusal_for(row: &RetainedRow) -> Option<&'static str> {
    tiler_core::window_policy::overlay_refusal(row.fullscreen, row.maximized)
}

/// Whether one Engine token is currently an intentional-float exception in
/// its domain session (floated windows hold no tile slot). Born-fullscreen
/// holds are exceptions too. `false` when the session is unknown.
///
/// World-owned: the last applied reply row plus the durable intent snapshot
/// carry the answer; no local session exists.
pub(super) fn engine_is_float(
    state: &TileLoop,
    _output: &str,
    _workspace: &str,
    token: &str,
) -> bool {
    state.core_is_float_token(token)
}

/// Whether one intentional-float mover may send tiled-to-tiled through the
/// shared slotless deferred path (D16 ordinary plus approved
/// floating+fullscreen / floating+maximized combos, 2026-10-13; Windows
/// automatic fixed-size floats as ordinary members, 2026-10-16). Mirrors
/// the shared [`tiler_core::session::Session::deferred_send_eligible`]
/// arms against adapter state: the mover must be an Engine exception in its
/// source session, never sticky (checked by the caller), and observed with
/// exactly zero or one overlay from the live `overlay_now` snapshot. Dual
/// overlays refuse. Automatic fixed-size origin (retained Engine mark or D4
/// latch) admits like intentional floats; only an explicit tile override
/// still refuses here. Pure in-memory read: no Engine mutation, no native
/// calls.
#[cfg(test)]
pub(super) fn float_send_admits(
    state: &TileLoop,
    mover_key: &crate::native_guards::WindowKey,
    output: &str,
    workspace: &str,
    token: &str,
) -> bool {
    if !engine_is_float(state, output, workspace, token) {
        return false;
    }
    if state.tile_override_of(mover_key) {
        return false;
    }
    match state.overlay_now.get(token) {
        Some((maximized, fullscreen)) => !(*maximized && *fullscreen),
        None => false,
    }
}

/// R-SPC-04 D1 predicate for subsequent fixed-size admissions: the last-good
/// live setting, read per admission so a saved change applies to later
/// admissions only and never reclassifies retained windows (D2). Proof
/// owners carry no live settings (`None`) and keep the both-axes default.
pub(super) fn fixed_predicate_for(
    live: Option<&LiveSettings>,
) -> tiler_core::size_hints::FixedSizePredicate {
    live.map(|live| live.settings.core.fixed_size_predicate_mode())
        .unwrap_or_default()
}

// D23 ordinary-observation close-drain fanout: deleted. The core owns
// close convergence outright: every world report carries the queued
// `pending_closed` obligations on its global close list, and the ack
// prunes them on receipt. No adapter fanout exists: fixed-size admission
// observation flags are deleted with the Engine-event path. The core
// reads fullscreen, fixed-auto, and fixed-suppress truth from every world
// report's window facts plus the adapter intent snapshot
// (`fixed_auto_seen`, `overlay_now`); no adapter stamping exists.
//
// R-MAX-11 (maximize clear at admission) is core-owned exclusively: the
// adapter no longer pre-clears maximizes and carries no attempt table.

#![forbid(unsafe_code)]
//! Scoped trailing lifecycle (world-owned): the trailing spare after a stay
//! send and migration completion are core lifecycle now. Only startup
//! output-context priming remains adapter-mechanical below.

use super::TileLoop;

/// Startup output-context priming: the adapter keeps its cached output
/// coherent after the mechanical topology sync. Trailing creation,
/// minimum, cleanup, select tails, and migration history are core
/// lifecycle now, so no other adapter entry point exists.
pub(super) fn prime_after_sync(state: &mut TileLoop) {
    if state.active_output.is_empty() {
        if let Some(first) = state.known_outputs.first().cloned() {
            state.active_output = first;
        }
    }
}

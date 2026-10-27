//! Consolidated offline integration tests for `tiler-macos`.
//!
//! Each module preserves one former standalone `tests/*.rs` target with
//! its original tests intact; only the module path changed, plus shared
//! condition-wait helpers for the converted tests. No live AX, no taps,
//! no desktop effects.

mod bounded;
mod focus_gaps_trace;
mod macos_classification;
mod runtime_freshness;
mod runtime_phase1;
mod scoped_close_reflow;
mod scoped_gates;
mod scoped_tile;
mod tile_entry;
mod window_policy_conformance;

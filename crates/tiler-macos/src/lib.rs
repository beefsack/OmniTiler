#![deny(unsafe_code)]

//! macOS Phase 0 capability probes and offline Phase 1 threading skeleton
//! (TIER 2, SIP left enabled). [`runtime`] owns the shared-core lifecycle;
//! existing probe entry points retain their bounded capability contracts.
//!
//! Threading contract (see `engine`):
//! - Main thread: AppKit/UI only (`probe_target` window runs here).
//! - Dedicated input thread: minimal event-tap callback, forwards via
//!   channel, never blocks.
//! - Per-app AX threads: one bounded worker per observed PID so a hung app
//!   isolates; every wait has a finite timeout.
//! - Engine thread: owns all mutable probe state; others send messages over
//!   channels.
//!
//! Safety contract: `unsafe` exists ONLY in [`ffi`]. Every other module
//! carries `#![forbid(unsafe_code)]` and uses trusted `objc2` /
//! `core-foundation` / `core-graphics` wrappers or the safe [`ffi`]
//! wrappers. No custom bindings outside [`ffi`].
//!
//! Default (`preflight`/`check`) and `help` never launch UI, taps, AX control,
//! or Space moves. Explicit
//! bounded modes (`ax-*`, `input-*`, `spaces-diagnose`, `probe-spawn`) require
//! flags, enforce finite durations, validate owned-probe identity, and fail
//! closed. Mutating Space operations report explicit unsupported status
//! (proven recovery not demonstrated); no guessed SLS writes or injection.

pub mod active_border;
pub mod ax;
pub mod ax_session;
pub mod border_runtime;
pub mod border_session;
pub mod cg;
pub mod classifier;
pub mod cli;
#[cfg(test)]
mod cli_tests;
pub mod config;
pub mod desktop_shortcuts;
#[cfg(test)]
mod desktop_shortcuts_tests;
pub mod dev_selection;
pub mod dock_swipe_select;
#[cfg(test)]
mod dock_swipe_select_tests;
pub mod drag_drop;
pub mod drag_fixture;
pub mod drag_native;
pub mod drag_session;
pub mod engine;
pub mod ffi;
pub mod focus;
pub mod focus_diagnose;
pub mod focus_route;
pub mod frontmost_restore;
pub mod group_underlay;
pub mod group_underlay_session;
pub mod input;
pub mod input_eligibility;
pub mod input_match;
pub mod input_policy;
pub mod input_session;
pub mod input_session_legs;
pub mod input_session_physical;
pub mod mc_lifecycle;
pub mod mc_members;
pub mod mc_observe;
pub mod mc_probe;
#[cfg(test)]
mod mc_probe_tests;
pub mod mc_recover;
pub mod native_facts;
pub mod native_input;
#[cfg(test)]
mod native_input_tests;
pub mod native_tile_backend;
#[cfg(test)]
mod native_tile_backend_tests;
pub mod owned_control;
pub mod ownership;
pub mod probe_target;
pub mod runtime;
pub mod runtime_apps;
pub mod runtime_backend;
mod runtime_core;
mod runtime_input;
pub mod runtime_lifecycle;
pub mod runtime_log;
mod runtime_native;
mod runtime_observe;
mod runtime_shared;
mod runtime_thread;
pub mod runtime_types;
mod runtime_worker;
pub mod scope;
pub mod scope_inspect;
mod select_feedback;
pub mod select_owner;
#[cfg(test)]
mod select_owner_tests;
pub mod session;
pub mod space_direct_probe;
pub mod space_drag_diag;
pub mod space_drag_diag_live;
#[cfg(test)]
mod space_drag_diag_tests;
pub mod space_drag_live;
pub mod space_drag_transfer;
#[cfg(test)]
mod space_drag_transfer_tests;
pub mod space_grab_diagnose;
pub mod space_join;
pub mod space_recovery;
pub mod space_switch_edges;
pub mod space_switch_probe;
#[cfg(test)]
mod space_switch_probe_tests;
pub mod spaces;
pub mod symbolic_hotkeys;
pub mod synth;
pub mod textedit_post;
pub mod tile;
pub mod topology;
pub mod watchdog;
pub mod workspace_indicator;
pub mod workspace_indicator_proof;
pub mod workspace_input;
#[cfg(test)]
mod workspace_input_tests;
pub mod workspace_select;
#[cfg(test)]
mod workspace_select_tests;
pub mod workspace_send;
pub mod workspace_send_effect;
#[cfg(test)]
mod workspace_send_live_tests;
pub mod workspace_send_managed;
mod workspace_send_managed_tests;
pub mod workspace_send_native;
#[cfg(test)]
mod workspace_send_native_tests;
#[cfg(test)]
mod workspace_send_tests;
pub(crate) mod world_builder;
pub(crate) mod world_command_map;
pub(crate) mod world_dispatch;
pub(crate) mod world_runtime;
#[cfg(test)]
mod world_runtime_tests;
pub(crate) mod world_sequencer;

/// Stable probe bundle identity. Fixed across runs for TCC stability.
pub const BUNDLE_ID: &str = "io.github.omnitiler.macos.probe";
/// Stable bundled-helper identity: the signed helper `.app` packaged by
/// `scripts/macos-app.sh` alongside the probe. Distinct from [`BUNDLE_ID`]
/// so an explicit helper-bundle scope admits the bundled helper (which
/// carries a LaunchServices entry plus launch date) without ever admitting
/// the probe itself. Direct-spawned (unbundled) helpers carry no bundle and
/// never match any allowlist entry.
pub const HELPER_BUNDLE_ID: &str = "io.github.omnitiler.macos.probe.helper";
/// Apple Silicon only for Phase 0.
pub const SUPPORTED_ARCH: &str = "aarch64";
/// Adopted minimum macOS release.
pub const MIN_MACOS_MAJOR: u32 = 15;

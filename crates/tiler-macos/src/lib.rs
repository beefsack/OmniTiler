#![deny(unsafe_code)]

//! macOS Phase 0 capability probe (TIER 2, SIP left enabled).
//!
//! Threading contract (see `engine`):
//! - Main thread: AppKit/UI only (`probe_target` window runs here).
//! - Dedicated input thread: minimal event-tap/hotkey callback, forwards via
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

pub mod ax;
pub mod cg;
pub mod cli;
pub mod config;
pub mod engine;
pub mod ffi;
pub mod input;
pub mod input_match;
pub mod ownership;
pub mod probe_target;
pub mod session;
pub mod spaces;

/// Stable probe bundle identity. Fixed across runs for TCC stability.
pub const BUNDLE_ID: &str = "io.github.omnitiler.macos.probe";
/// Apple Silicon only for Phase 0.
pub const SUPPORTED_ARCH: &str = "aarch64";
/// Adopted minimum macOS release.
pub const MIN_MACOS_MAJOR: u32 = 15;

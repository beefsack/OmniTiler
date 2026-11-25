//! Shared library surface for `tiler-live-fixture`.
//!
//! The binary owns the `serve`/`send` roles. Examples link this library only
//! to reach the crate's single approved Windows FFI root (`pipe_win`) instead
//! of declaring per-example `allow(unsafe_code)` roots.

#[cfg(windows)]
pub mod pipe_win;

#![forbid(unsafe_code)]

//! Input probe: bounded CGEventTap interception over [`crate::ffi`].
//!
//! Routing (live-macOS contract):
//! - CGEventTap runs on a dedicated input thread inside FFI with a minimal
//!   callback (exact-chord compare, non-blocking forward of exact matches
//!   only — never raw deliveries — and `Keep` unless suppression is armed).
//!   Default is observe-only (`Keep` always). The explicit `--suppress` flag
//!   arms exact `Ctrl`-chord `KeyDown`/`KeyUp` pair suppression (see
//!   [`crate::input_match::decide_tap_event`]): unrelated keys, inexact
//!   modifiers, lone ups, and `FlagsChanged` still `Keep`.
//! - Every tap has its own finite max duration ([`crate::config`] bound)
//!   with RAII teardown; a timeout-disable re-enables once, notifies the
//!   engine exactly once (informational, not recovery evidence), then stops
//!   on a second disable, while a user-disable never re-enables and stops
//!   the probe run truthfully instead (bounded-probe semantics only — the
//!   continuous tile owner recovers both kinds, see
//!   [`crate::input_policy`]). Secure Input `Active` refuses interception;
//!   `Unknown` proceeds but is reported as unknown (missing symbol is not
//!   inactivity).

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::SyncSender,
};

use crate::config::ProbeModifier;
use crate::engine::EngineMessage;
use crate::input_match::Chord;

/// Fixed Phase-0 probe chord key: Mac virtual keycode `0x04` (ANSI `h`
/// position on US layout; `CGKeyCode` space, *not* USB HID usage IDs).
pub const PROBE_KEY_CODE: u32 = 0x04;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputError {
    Unsupported(String),
    Ownership(String),
    SecureInputActive,
    Denied(String),
    Timeout,
}

impl std::fmt::Display for InputError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsupported(r) => write!(f, "unsupported: {r}"),
            Self::Ownership(r) => write!(f, "ownership refused: {r}"),
            Self::SecureInputActive => write!(f, "secure input active; interception stopped"),
            Self::Denied(r) => write!(f, "input registration denied: {r}"),
            Self::Timeout => write!(f, "input wait timed out"),
        }
    }
}

impl std::error::Error for InputError {}

/// CG modifier bits for a probe modifier (`CGEventFlags` values from
/// `CGEventTypes.h`: Shift 0x20000, Control 0x40000, Alternate 0x80000,
/// Command 0x100000). Returns `(want, mask)` for exact-chord compare.
/// A macOS-only test pins these against the trusted `core-graphics` wrapper.
pub fn cg_flag_bits(modifier: ProbeModifier) -> (u64, u64) {
    const SHIFT: u64 = 0x0002_0000;
    const CONTROL: u64 = 0x0004_0000;
    const ALTERNATE: u64 = 0x0008_0000;
    const COMMAND: u64 = 0x0010_0000;
    const MASK: u64 = SHIFT | CONTROL | ALTERNATE | COMMAND;
    let want = match modifier {
        ProbeModifier::Meta => COMMAND,
        ProbeModifier::Ctrl => CONTROL,
        ProbeModifier::Alt => ALTERNATE,
        ProbeModifier::Shift => SHIFT,
    };
    (want, MASK)
}

/// RAII tap lifetime. The guard is returned *after* the bounded run with the
/// port already invalidated; `disabled` is therefore true on return, and
/// `Drop` keeps it true idempotently.
#[derive(Debug)]
pub struct TapGuard {
    pub report: crate::ffi::TapRunReport,
    disabled: Arc<AtomicBool>,
}

impl TapGuard {
    #[cfg(target_os = "macos")]
    fn completed(report: crate::ffi::TapRunReport) -> Self {
        Self {
            report,
            disabled: Arc::new(AtomicBool::new(true)),
        }
    }

    pub fn is_disabled(&self) -> bool {
        self.disabled.load(Ordering::SeqCst)
    }
}

impl Drop for TapGuard {
    fn drop(&mut self) {
        self.disabled.store(true, Ordering::SeqCst);
    }
}

/// Validate a bounded intercept request without touching the OS.
pub fn validate_intercept(chord: &Chord, seconds: u64) -> Result<(), InputError> {
    if seconds == 0 || seconds > crate::config::MAX_TAP_SECONDS {
        return Err(InputError::Unsupported(format!(
            "seconds must be 1..={}, got {seconds}",
            crate::config::MAX_TAP_SECONDS
        )));
    }
    let _ = chord;
    Ok(())
}

/// Secure Input observation. `Unknown` (missing symbol) is distinct from
/// `Inactive`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SecureInput {
    Active,
    Inactive,
    Unknown,
}

pub fn secure_input() -> SecureInput {
    match crate::ffi::secure_input_known() {
        Some(true) => SecureInput::Active,
        Some(false) => SecureInput::Inactive,
        None => SecureInput::Unknown,
    }
}

/// Bounded tap for exactly the probe chord. Blocks at most `seconds`;
/// returns the measured run (matched/suppressed/deliveries/re-enable) with
/// the port already torn down. `suppress` arms exact-chord pair suppression
/// (explicit `--suppress` contract); default `false` is observe-only.
pub fn intercept_bounded(
    modifier: ProbeModifier,
    key_code: u32,
    seconds: u64,
    suppress: bool,
    tx: &SyncSender<EngineMessage>,
) -> Result<TapGuard, InputError> {
    let chord = Chord::new(modifier, key_code).map_err(InputError::Unsupported)?;
    validate_intercept(&chord, seconds)?;
    match secure_input() {
        SecureInput::Active => return Err(InputError::SecureInputActive),
        SecureInput::Inactive | SecureInput::Unknown => {}
    }
    #[cfg(target_os = "macos")]
    {
        let (want, mask) = cg_flag_bits(modifier);
        crate::ffi::run_event_tap_bounded(want, mask, key_code as i64, seconds, suppress, tx)
            .map(TapGuard::completed)
            .map_err(|e| match e {
                crate::ffi::TapSetupError::Denied(r) => InputError::Denied(r),
                crate::ffi::TapSetupError::Runloop(r) => InputError::Unsupported(r),
            })
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (suppress, tx);
        Err(InputError::Unsupported(
            "event tap is macOS-only".to_owned(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modifier_bit_maps_cover_all() {
        // Exact-chord compare: want is a single bit inside the mask.
        for modifier in [
            ProbeModifier::Meta,
            ProbeModifier::Ctrl,
            ProbeModifier::Alt,
            ProbeModifier::Shift,
        ] {
            let (want, mask) = cg_flag_bits(modifier);
            assert_eq!(want & mask, want);
            assert_ne!(want, 0);
        }
        // Distinct modifiers, distinct bits.
        assert_ne!(
            cg_flag_bits(ProbeModifier::Meta),
            cg_flag_bits(ProbeModifier::Ctrl)
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn cg_bits_match_trusted_wrapper() {
        use core_graphics::event::CGEventFlags;
        let (want, mask) = cg_flag_bits(ProbeModifier::Meta);
        assert_eq!(want, CGEventFlags::CGEventFlagCommand.bits());
        assert_eq!(
            mask,
            CGEventFlags::CGEventFlagShift.bits()
                | CGEventFlags::CGEventFlagControl.bits()
                | CGEventFlags::CGEventFlagAlternate.bits()
                | CGEventFlags::CGEventFlagCommand.bits()
        );
    }

    #[test]
    fn intercept_rejects_unbounded_before_os_touch() {
        let (engine, _rx) = crate::engine::Engine::spawn();
        assert!(
            intercept_bounded(ProbeModifier::Meta, PROBE_KEY_CODE, 0, false, &engine.tx).is_err()
        );
        assert!(
            intercept_bounded(
                ProbeModifier::Meta,
                PROBE_KEY_CODE,
                crate::config::MAX_TAP_SECONDS + 1,
                true,
                &engine.tx
            )
            .is_err()
        );
    }

    #[test]
    fn chord_mismatch_never_intercepts() {
        let registered = Chord::new(ProbeModifier::Meta, PROBE_KEY_CODE).unwrap();
        assert!(!registered.matches(ProbeModifier::Ctrl, PROBE_KEY_CODE));
    }

    #[test]
    fn secure_state_is_three_valued() {
        // Must exist as a distinct variant; platform backends map honestly.
        assert_ne!(SecureInput::Unknown, SecureInput::Inactive);
        assert_ne!(SecureInput::Unknown, SecureInput::Active);
    }
}

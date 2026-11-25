#![forbid(unsafe_code)]

//! `maximize-toggle` native evidence path for `fixture-leg`.
//!
//! Diagnostic-only chord: one owned window receives two real `maximize-m`
//! posts through the product tap, with native AX/CG frames captured before,
//! between and after. The first press must move the frame; the second must
//! restore the exact base. This is native behaviour evidence, not slice-5
//! acceptance and not a product command.
//!
//! The base is read after the initial tile/reflow settles, so the comparison
//! is against the applied tiled frame, not the fixture's spawn geometry. Each
//! press is followed by one bounded hold before the next native read: the core
//! reply and AX frame application are asynchronous, so a bounded settle
//! replaces an unbounded poll. The verdict is the shared
//! [`crate::fixture_sample::toggle_snapshot_verdict`] check.

use crate::config::ProbeModifier;
use crate::fixture_leg::settle_hold;
use crate::fixture_post::{FixtureDriver, LiveTransport};

/// Settle after each maximize press before the native frame is read
/// (milliseconds): the core reply and AX frame application are asynchronous,
/// so one bounded hold replaces an unbounded poll.
const MAXIMIZE_SETTLE_MS: u64 = 1_500;

/// Drive the diagnostic `maximize-toggle` evidence path on one owned window:
/// settle, read the base frame, post `maximize-m`, settle, read the maximized
/// frame, post again, settle, read the restored frame, and require the shared
/// apply-then-restore verdict. Returns the evidence JSON body.
pub(crate) fn run(
    driver: &FixtureDriver<'_, LiveTransport>,
    pid: u32,
    wid: u32,
    caller: &str,
    chord: &str,
    modifier: ProbeModifier,
) -> Result<serde_json::Value, (i32, String)> {
    // Let the initial tile/reflow settle before taking the base; the
    // first press must be compared with the applied tiled frame, not
    // the fixture's spawn geometry.
    settle_hold(MAXIMIZE_SETTLE_MS)?;
    let base = crate::fixture_sample::read_joined_frame(pid, wid).map_err(|e| (1, e))?;
    let first: serde_json::Value =
        serde_json::from_str(&driver.post(caller, wid, "maximize-m", modifier)?)
            .map_err(|e| (1, e.to_string()))?;
    settle_hold(MAXIMIZE_SETTLE_MS)?;
    let maximized = crate::fixture_sample::read_joined_frame(pid, wid).map_err(|e| (1, e))?;
    let second: serde_json::Value =
        serde_json::from_str(&driver.post(caller, wid, "maximize-m", modifier)?)
            .map_err(|e| (1, e.to_string()))?;
    settle_hold(MAXIMIZE_SETTLE_MS)?;
    let restored = crate::fixture_sample::read_joined_frame(pid, wid).map_err(|e| (1, e))?;
    crate::fixture_sample::toggle_snapshot_verdict(&base, &maximized, &restored)
        .map_err(|e| (1, e))?;
    Ok(serde_json::json!({
        "outcome": "maximize-toggle",
        "chord": chord,
        "caller": caller,
        "wid": wid,
        "base": base,
        "maximized": maximized,
        "restored": restored,
        "first_press": first,
        "second_press": second,
        "note": "native apply-then-restore evidence through the real product tap; exact work-area geometry is covered offline"
    }))
}

#![forbid(unsafe_code)]

//! Development-only guarded two-launch fixture controller.
//!
//! One authenticated fixture instance (its PID plus bound executable/launch
//! date, control nonce, owned windows, one controller-owned Space) and one
//! isolated intent store survive across two separately identified signed
//! product launches. The second launch is bound only after the first
//! publishes clean terminal evidence and its exact PID is independently
//! gone, with the fixture join, current owned Space/topology and exact store
//! fingerprint rechecked immediately before it. Any uncertainty or overlap
//! stops the run before the second launch.
//!
//! The controller never posts input to the second launch: the admitted
//! fixture driver is scoped to the pre-stop phase and dropped before the
//! first product stops. While no product is alive, one authenticated
//! signed-fixture AX frame write nudges a known tile; exact independent AX
//! and CG frames must show only that requested change, a fresh guard runs,
//! and only then does the second launch bind. The post-restart phase receives
//! no chord driver: it captures exact independent AX and CG frames and
//! re-verifies the product, fixture, owned-Space and window identity at that
//! capture.
//!
//! Two modes:
//! - `m3`: establish the owned Space and spawn only the tile callers that
//!   must survive, then launch the product and capture their genuinely fresh
//!   settled AX/CG layout as the baseline. Add the intended float windows
//!   afterwards (preserving the survivor WIDs and original caller order),
//!   float the first `--floats` callers, and require the surviving tile
//!   multisets to match the fresh baseline again before the stop. After the
//!   restart, require the same float frames, the same unordered survivor
//!   tile-rectangle multisets (duplicates count) against that fresh baseline,
//!   the store bytes plus metadata unchanged, and every intended float row
//!   bound exactly to the fixture lifetime. The historical pre-stop tile
//!   split is reported but is not the oracle: orderly restart re-adopts the
//!   survivors as a fresh tree.
//! - `m4`: capture the settled tiled base, apply exactly one composed
//!   maximize overlay, require it distinct, then require the exact base
//!   after restart with the store unchanged (absence is labelled as
//!   unchanged absence, not hydrated rows).
//!
//! Teardown is the existing [`crate::live_leg::LiveLeg`] pipeline on every
//! path: stop the exact product, restore the starting Space, close only the
//! owned windows, shut down and reap only the owned fixture, remove only the
//! created empty Space, stop the awake companion, compare the baseline and
//! remove the owned run directory. The private store is removed only once the
//! product is proven reaped, even when fixture shutdown fails; otherwise
//! product logs and the private store are preserved as residue. Every
//! restart outcome, pass or refusal, keeps both product lifetimes' stdout,
//! stderr and terminal outcome as auditable non-native evidence residue:
//! proven product absence still removes the private store and a clean native
//! restoration removes the blocking receipt, while the evidence files stay
//! for inspection. Non-restart legs keep their previous whole-directory
//! cleanup.

use std::path::{Path, PathBuf};

use crate::config::ProbeModifier;
use crate::fixture_leg::{LEG_MAX_SECONDS, LEG_MIN_SECONDS};
use crate::input_policy::InputPreset;
use crate::live_leg::{LiveLeg, LiveLegTransport};

use orchestration::run_body_then_finish;

mod arrangement;
mod frame;
mod frames;
mod guard;
mod orchestration;
mod store;

#[cfg(test)]
mod tests;

/// Maximum owned windows one restart run may spawn (bounded automation).
pub const RESTART_MAX_WINDOWS: usize = 8;
/// M3 needs at least two floats plus two tiles to exercise both arrangements.
pub const RESTART_MIN_FLOATS: usize = 2;
/// M3 needs at least two tiles so the multiset comparison has a duplicate
/// shape available and a slot swap can be observed.
pub const RESTART_MIN_TILES: usize = 2;

/// Bounded wait for one settled arrangement before an exact capture
/// (milliseconds). The product owns its own startup/hydration latency and
/// offers no readiness event for applied frames; stated here where the timer
/// is defined.
pub const RESTART_SETTLE_DEADLINE_MS: u64 = 20_000;
/// Minimum observed stability span before frames count as settled.
pub const RESTART_STABLE_SPAN_MS: u64 = 500;
/// Poll cadence while waiting for settled frames.
const RESTART_READ_INTERVAL_MS: u64 = 100;
/// Bounded settle after each pre-stop chord (milliseconds): the core reply
/// and native frame application are asynchronous, so one hold replaces an
/// unbounded poll (the same pacing the native diagnostic legs use).
const RESTART_CHORD_SETTLE_MS: u64 = 1_500;
/// Bounded wait for the product's own runtime-started stderr record (the
/// `tilestarted` announcement precedes runtime startup): frames are never
/// captured from a product that has not reached its runtime loop.
const RESTART_PRODUCT_START_DEADLINE_MS: u64 = 10_000;
/// Bounded startup settle after the runtime-started record and before the
/// first exact capture: the initial observation/layout dispatch and the
/// hydration import are engine jobs behind that record, not part of it.
/// Stated here where the timer lives; a changed layout still has to pass
/// the exact frame comparison below.
const RESTART_STARTUP_SETTLE_MS: u64 = 2_000;

/// One restart invocation mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestartMode {
    /// Multiple floats plus tiles retained across the relaunch (M3).
    M3,
    /// Composed maximize overlay drops to the exact base across relaunch (M4).
    M4,
}

impl RestartMode {
    /// Parse the exact CLI token (`m3` or `m4`). Pure; unit-tested.
    pub fn parse(raw: &str) -> Result<Self, String> {
        match raw {
            "m3" => Ok(Self::M3),
            "m4" => Ok(Self::M4),
            other => Err(format!(
                "fixture-restart: --mode must be m3 or m4 (got '{other}')"
            )),
        }
    }

    /// Exact CLI token for this mode.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::M3 => "m3",
            Self::M4 => "m4",
        }
    }

    /// Default owned-window count when `--windows` is omitted.
    #[must_use]
    pub const fn default_windows(self) -> usize {
        match self {
            Self::M3 => 4,
            Self::M4 => 2,
        }
    }
}

/// One `fixture-restart` invocation.
pub struct FixtureRestartOpts {
    pub mode: RestartMode,
    pub seconds: u64,
    /// Owned fixture windows spawned once and retained across both launches.
    pub windows: usize,
    /// Windows floated before the first stop (M3 only; zero for M4).
    pub floats: usize,
    pub preset: InputPreset,
    pub modifier: ProbeModifier,
    /// Expected fixture designated requirement (machine-specific, never
    /// hardcoded here). Empty refuses before any launch.
    pub signer: String,
}

/// Validate one restart invocation before EVERY effect (no run dir, no
/// fixture launch): bounded lifetime, a nonempty signer, bounded windows,
/// and the mode's float/tile shape. Pure; unit-tested.
pub fn validate_restart_opts(opts: &FixtureRestartOpts) -> Result<(), String> {
    if opts.seconds < LEG_MIN_SECONDS || opts.seconds > LEG_MAX_SECONDS {
        return Err(format!(
            "fixture-restart needs --seconds {LEG_MIN_SECONDS}..={LEG_MAX_SECONDS}, got {}",
            opts.seconds
        ));
    }
    if opts.signer.trim().is_empty() {
        return Err("fixture-restart needs a nonempty --signer".to_owned());
    }
    if opts.windows < 2 || opts.windows > RESTART_MAX_WINDOWS {
        return Err(format!(
            "fixture-restart needs --windows 2..={RESTART_MAX_WINDOWS}, got {}",
            opts.windows
        ));
    }
    match opts.mode {
        RestartMode::M3 => {
            let tiles = opts.windows.saturating_sub(opts.floats);
            if opts.floats < RESTART_MIN_FLOATS || tiles < RESTART_MIN_TILES {
                return Err(format!(
                    "fixture-restart m3 needs at least {RESTART_MIN_FLOATS} floats and {RESTART_MIN_TILES} tiles, got {} floats and {tiles} tiles",
                    opts.floats
                ));
            }
        }
        RestartMode::M4 => {
            if opts.floats != 0 {
                return Err(
                    "fixture-restart m4 takes no --floats (the maximize overlay targets the first window)"
                        .to_owned(),
                );
            }
        }
    }
    Ok(())
}

/// Guaranteed-distinct LaunchServices run ID for one restart launch. Pure;
/// unit-tested. The product echoes this exact ID in its `tilestarted` line,
/// so a stale/foreign instance can never bind.
#[must_use]
pub fn restart_run_id(controller_pid: u32, fixture_pid: u32, index: u8) -> String {
    format!("fixture-restart-{controller_pid}-{fixture_pid}-{index}")
}

/// Margin over both product lifetimes for setup, the between-product proof,
/// hydration and owned teardown (seconds).
pub const RESTART_CAFFEINATE_MARGIN_SECONDS: u64 = 120;

/// Bounded display-awake companion budget for one restart run: two product
/// lifetimes plus the fixed margin. One lifetime plus a small margin could
/// expire between the first stop and the second launch; the companion must
/// cover both launches and the between-product proof (300 s at the documented
/// 90 s + 90 s shape). Pure; unit-tested.
#[must_use]
pub fn restart_caffeine_seconds(seconds: u64) -> u64 {
    seconds
        .saturating_mul(2)
        .saturating_add(RESTART_CAFFEINATE_MARGIN_SECONDS)
}

/// Distinct output directory for one restart launch under the shared owned
/// product directory. Pure; unit-tested.
#[must_use]
pub fn restart_launch_dir(product_dir: &Path, index: u8) -> PathBuf {
    product_dir.join(format!("launch-{index}"))
}

/// Handler wrapper: validate, arm the interrupt latch, initialize AppKit
/// before the baseline, run the body, and always finish the owned teardown.
/// Exit codes: 2 usage/validation, 1 gate/proof/teardown failure with
/// evidence.
pub fn run_fixture_restart(opts: FixtureRestartOpts) -> Result<String, (i32, String)> {
    validate_restart_opts(&opts).map_err(|error| (2, error))?;
    let _interrupt = crate::space_drag_diag_live::InterruptArmed::arm().map_err(|error| {
        (
            2,
            format!("fixture-restart: interrupt arm refused ({error})"),
        )
    })?;
    #[cfg(target_os = "macos")]
    ensure_appkit_singleton();
    let mut transport = LiveLegTransport::new();
    // Two product lifetimes plus the fixed setup/mutation/hydration/teardown
    // margin (exactly 300 s at the documented 90 s + 90 s shape). The
    // independent `just launch --timeout 300` wrapper is a separate wall
    // clock capped at 300; neither the CLI options nor the wrapper change.
    let leg = LiveLeg::begin(&mut transport, restart_caffeine_seconds(opts.seconds))?;
    let (evidence, teardown) = run_body_then_finish(
        leg,
        |leg| {
            #[cfg(not(target_os = "macos"))]
            {
                let _ = (leg, &opts);
                Err((
                    2,
                    "fixture-restart needs macOS (LaunchServices launches plus native joins)"
                        .to_owned(),
                ))
            }
            #[cfg(target_os = "macos")]
            {
                // Restart legs keep both product lifetimes' stdout, stderr
                // and terminal outcome as auditable non-native evidence on
                // every outcome, pass or refusal. The flag is set before the
                // first launch so no later failure can delete it.
                leg.retain_restart_product_evidence();
                orchestration::run_restart_body_macos(leg, &opts)
            }
        },
        |leg| leg.finish(),
        |teardown| eprintln!("fixture-restart teardown after failure: {teardown}"),
    )?;
    let mut envelope = serde_json::json!({
        "outcome": "completed",
        "restart": evidence,
        "teardown": teardown,
    });
    envelope["mode"] = serde_json::json!(opts.mode.as_str());
    Ok(envelope.to_string())
}

#[cfg(target_os = "macos")]
fn ensure_appkit_singleton() {
    crate::fixture_leg::ensure_appkit_singleton();
}

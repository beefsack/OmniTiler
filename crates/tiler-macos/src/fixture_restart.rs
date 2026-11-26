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
//! - `m3`: float the first `--floats` owned windows, capture the settled
//!   pre-stop arrangement, then require the same per-window float frames and
//!   the same unordered tile-rectangle multiset (duplicates count) after the
//!   restarted product hydrates, with the store bytes plus metadata
//!   unchanged and every intended float row bound exactly to the fixture
//!   lifetime.
//! - `m4`: capture the settled tiled base, apply exactly one composed
//!   maximize overlay, require it distinct, then require the exact base
//!   after restart with the store unchanged (absence is labelled as
//!   unchanged absence, not hydrated rows).
//!
//! Teardown is the existing [`crate::live_leg::LiveLeg`] pipeline on every
//! path: stop the exact product, restore the starting Space, close only the
//! owned windows, shut down and reap only the owned fixture, remove only the
//! created empty Space, stop the awake companion, compare the baseline and
//! remove the owned run directory including only the private store. The
//! private store is removed once the product is proven reaped even when
//! fixture shutdown fails; receipts/logs stay for audit.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::config::ProbeModifier;
use crate::fixture_frame::FramePoint;
use crate::fixture_leg::{
    FIXTURE_SCOPE, FOCUS_SETTLE_MS, ISOLATED_STORE_LEAF, LEG_MAX_SECONDS, LEG_MIN_SECONDS,
    create_owned_run_dir, fixture_bundle_path, join_fixture_windows, launch_fixture_bundle,
    launch_product_ls, product_argv, product_bundle_path, read_announcement, read_run_nonce,
    settle_hold, spawn_owned_windows,
};
use crate::fixture_owned_space::{
    PostOnlyOwnedSpace, await_joined_on_owned_space, establish_post_only_owned_space,
    post_only_placement_refusal,
};
use crate::input_policy::InputPreset;
use crate::live_leg::{
    FixtureRecord, LegTransport, LiveLeg, LiveLegTransport, RestartStopEvidence,
    clean_terminal_verdict,
};
use crate::space_probe::NativeSpaces;

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

/// Distinct output directory for one restart launch under the shared owned
/// product directory. Pure; unit-tested.
#[must_use]
pub fn restart_launch_dir(product_dir: &Path, index: u8) -> PathBuf {
    product_dir.join(format!("launch-{index}"))
}

/// One native window's exact independent frame reads: AX frame plus CG
/// bounds plus the CG onscreen fact. Both legs are captured from independent
/// APIs; neither is derived from the other.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct WindowFrames {
    pub caller: String,
    pub wid: u32,
    pub ax: FramePoint,
    pub cg: FramePoint,
    pub cg_onscreen: bool,
}

/// Exact multisets use the bit pattern of every finite float component:
/// equality is bit equality and the sort order is deterministic without a
/// total-order float claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct RectKey {
    x: u64,
    y: u64,
    w: u64,
    h: u64,
}

impl From<&FramePoint> for RectKey {
    fn from(rect: &FramePoint) -> Self {
        Self {
            x: rect.x.to_bits(),
            y: rect.y.to_bits(),
            w: rect.w.to_bits(),
            h: rect.h.to_bits(),
        }
    }
}

fn rect_multiset(frames: impl Iterator<Item = FramePoint>) -> Vec<RectKey> {
    let mut keys: Vec<RectKey> = frames.map(|frame| RectKey::from(&frame)).collect();
    keys.sort_unstable();
    keys
}

/// Pre-stop arrangement split by role: floats compare by exact window, tiles
/// compare as an unordered rectangle multiset.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct ArrangementEvidence {
    pub floats: Vec<WindowFrames>,
    pub tiles: Vec<WindowFrames>,
    pub tile_reflowed: bool,
}

/// Split one settled frame set into the first `float_count` float windows and
/// the remaining tile windows. Pure; unit-tested.
pub(crate) fn partition_arrangement(
    frames: &[WindowFrames],
    float_count: usize,
) -> ArrangementEvidence {
    let floats: Vec<WindowFrames> = frames.iter().take(float_count).cloned().collect();
    let tiles: Vec<WindowFrames> = frames.iter().skip(float_count).cloned().collect();
    ArrangementEvidence {
        floats,
        tiles,
        tile_reflowed: false,
    }
}

fn frames_by_caller(frames: &[WindowFrames]) -> BTreeMap<&str, &WindowFrames> {
    frames
        .iter()
        .map(|frame| (frame.caller.as_str(), frame))
        .collect()
}

fn require_same_windows(
    label: &str,
    before: &[WindowFrames],
    after: &[WindowFrames],
) -> Result<(), String> {
    let mut before_callers: Vec<&str> = before.iter().map(|frame| frame.caller.as_str()).collect();
    let mut after_callers: Vec<&str> = after.iter().map(|frame| frame.caller.as_str()).collect();
    before_callers.sort_unstable();
    after_callers.sort_unstable();
    if before_callers != after_callers {
        return Err(format!(
            "restart {label}: window set changed (before {before_callers:?}, after {after_callers:?}); refusing"
        ));
    }
    Ok(())
}

fn require_exact_frame(
    label: &str,
    before: &WindowFrames,
    after: &WindowFrames,
) -> Result<(), String> {
    if !after.cg_onscreen {
        return Err(format!(
            "restart {label}: window {} ({}) is not CG-onscreen after restart; refusing",
            after.caller, after.wid
        ));
    }
    if before.ax != after.ax {
        return Err(format!(
            "restart {label}: window {} AX frame changed (before {:?}, after {:?}); refusing",
            after.caller, before.ax, after.ax
        ));
    }
    if before.cg != after.cg {
        return Err(format!(
            "restart {label}: window {} CG bounds changed (before {:?}, after {:?}); refusing",
            after.caller, before.cg, after.cg
        ));
    }
    Ok(())
}

/// M3 verdict: every float must exist after restart with the same caller and
/// exact AX plus CG frame; the tile window set must be unchanged and the
/// unordered AX-rectangle and CG-rectangle multisets must match exactly,
/// including duplicate counts. Slots may swap between tiles. Pure;
/// unit-tested.
pub(crate) fn compare_restart_arrangement(
    before: &ArrangementEvidence,
    after_all: &[WindowFrames],
) -> Result<(), String> {
    let after = frames_by_caller(after_all);
    let tile_callers: Vec<&str> = before
        .tiles
        .iter()
        .map(|frame| frame.caller.as_str())
        .collect();
    let mut after_tiles = Vec::new();
    for float in &before.floats {
        let Some(observed) = after.get(float.caller.as_str()) else {
            return Err(format!(
                "restart m3: float window {} is absent after restart; refusing",
                float.caller
            ));
        };
        if tile_callers.contains(&observed.caller.as_str()) {
            return Err(format!(
                "restart m3: window {} changed from float to tile across restart; refusing",
                observed.caller
            ));
        }
        require_exact_frame("m3", float, observed)?;
    }
    for tile in &before.tiles {
        let Some(observed) = after.get(tile.caller.as_str()) else {
            return Err(format!(
                "restart m3: tile window {} is absent after restart; refusing",
                tile.caller
            ));
        };
        if !observed.cg_onscreen {
            return Err(format!(
                "restart m3: tile window {} is not CG-onscreen after restart; refusing",
                observed.caller
            ));
        }
        after_tiles.push((*observed).clone());
    }
    let mut before_callers: Vec<&str> = before
        .floats
        .iter()
        .chain(before.tiles.iter())
        .map(|frame| frame.caller.as_str())
        .collect();
    let mut after_callers: Vec<&str> = after_all
        .iter()
        .map(|frame| frame.caller.as_str())
        .collect();
    before_callers.sort_unstable();
    after_callers.sort_unstable();
    if before_callers != after_callers {
        return Err(format!(
            "restart m3: window set changed (before {before_callers:?}, after {after_callers:?}); refusing"
        ));
    }
    let before_ax = rect_multiset(before.tiles.iter().map(|frame| frame.ax));
    let after_ax = rect_multiset(after_tiles.iter().map(|frame| frame.ax));
    if before_ax != after_ax {
        return Err(
            "restart m3: tile AX rectangle multiset changed across restart; refusing".to_owned(),
        );
    }
    let before_cg = rect_multiset(before.tiles.iter().map(|frame| frame.cg));
    let after_cg = rect_multiset(after_tiles.iter().map(|frame| frame.cg));
    if before_cg != after_cg {
        return Err(
            "restart m3: tile CG rectangle multiset changed across restart; refusing".to_owned(),
        );
    }
    Ok(())
}

/// Require at least one tile window to have reflowed from the initial tile
/// placement after the float toggles: the leg exercised an active tiled
/// arrangement, not two coincidental float windows.
pub(crate) fn require_tile_reflow(
    initial: &[WindowFrames],
    tiles: &[WindowFrames],
) -> Result<bool, String> {
    let initial = frames_by_caller(initial);
    let mut reflowed = false;
    for tile in tiles {
        let Some(before) = initial.get(tile.caller.as_str()) else {
            return Err(format!(
                "restart m3: tile window {} was absent before the float chords; refusing",
                tile.caller
            ));
        };
        if before.ax != tile.ax || before.cg != tile.cg {
            reflowed = true;
        }
    }
    if !reflowed {
        return Err(
            "restart m3: no tile window reflowed after the float chords (tiled arrangement unproven); refusing"
                .to_owned(),
        );
    }
    Ok(true)
}

/// M4 verdict: the pre-stop overlay must be distinct from the settled base
/// for the target window (both independent reads), and every window after
/// restart must equal the exact base in AX and CG. Any overlay surviving
/// restart refuses, as does a base that never changed. Pure; unit-tested.
pub(crate) fn compare_restart_base(
    base: &[WindowFrames],
    overlay: &[WindowFrames],
    after: &[WindowFrames],
    target: &str,
) -> Result<(), String> {
    require_distinct_overlay(base, overlay, target)?;
    let after_map = frames_by_caller(after);
    require_same_windows("m4", base, after)?;
    for base_frame in base {
        let Some(observed) = after_map.get(base_frame.caller.as_str()) else {
            return Err(format!(
                "restart m4: window {} is absent after restart; refusing",
                base_frame.caller
            ));
        };
        require_exact_frame("m4", base_frame, observed)?;
    }
    Ok(())
}

/// Require the pre-stop composed overlay to differ from the settled base for
/// the target window in both independent reads, over the same window set.
/// This is the M4 proof that a live overlay existed before the first stop.
pub(crate) fn require_distinct_overlay(
    base: &[WindowFrames],
    overlay: &[WindowFrames],
    target: &str,
) -> Result<(), String> {
    let base_map = frames_by_caller(base);
    let overlay_map = frames_by_caller(overlay);
    let Some(base_target) = base_map.get(target) else {
        return Err(format!(
            "restart m4: target window {target} is absent from the settled base; refusing"
        ));
    };
    let Some(overlay_target) = overlay_map.get(target) else {
        return Err(format!(
            "restart m4: target window {target} is absent from the pre-stop overlay; refusing"
        ));
    };
    if base_target.ax == overlay_target.ax || base_target.cg == overlay_target.cg {
        return Err(format!(
            "restart m4: target window {target} has no distinct composed overlay before stop (AX and CG must both move); refusing"
        ));
    }
    require_same_windows("m4", base, overlay)
}

/// Liveness read for one exact PID: gone, present, or unreadable (never a
/// guess). Pure data; the guard judges it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Liveness {
    Gone,
    Present,
    Unreadable(String),
}

/// Guard stage library (ordered): the first failing fact refuses the second
/// launch, so a later clean fact can never mask an earlier uncertainty.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GuardStage {
    LaunchIdentity,
    ProductOverlap,
    PriorTerminal,
    PriorPid,
    PriorLifetime,
    FixturePid,
    FixtureIdentity,
    FixtureNonce,
    FixtureJoin,
    OwnedSpace,
    WindowPlacement,
    Store,
}

impl GuardStage {
    const fn as_str(self) -> &'static str {
        match self {
            Self::LaunchIdentity => "launch-identity",
            Self::ProductOverlap => "product-overlap",
            Self::PriorTerminal => "prior-terminal",
            Self::PriorPid => "prior-pid",
            Self::PriorLifetime => "prior-lifetime",
            Self::FixturePid => "fixture-pid",
            Self::FixtureIdentity => "fixture-identity",
            Self::FixtureNonce => "fixture-nonce",
            Self::FixtureJoin => "fixture-join",
            Self::OwnedSpace => "owned-space",
            Self::WindowPlacement => "window-placement",
            Self::Store => "store",
        }
    }
}

/// Plain-data observations for the immediately-before-second-launch guard.
/// Every field is a read fact; the verdict is pure.
#[derive(Debug, Clone)]
pub(crate) struct RestartGuardInputs {
    pub launch_ids_distinct: bool,
    pub product_overlap: bool,
    pub product_overlap_note: Option<String>,
    pub prior_terminal_clean: bool,
    pub prior_liveness: Liveness,
    pub prior_lifetime_gone: bool,
    pub fixture_liveness: Liveness,
    pub fixture_identity_readable: bool,
    pub fixture_bundle_match: bool,
    pub fixture_exe_match: bool,
    pub fixture_launch_match: bool,
    pub fixture_nonce_match: bool,
    pub fixture_join_same: bool,
    pub fixture_join_note: Option<String>,
    pub topology_readable: bool,
    pub current_owned_space: bool,
    pub topology_row_exact: bool,
    pub windows_on_owned_space: bool,
    pub store_expected: StoreSnapshot,
    pub store_observed: StoreSnapshot,
}

impl RestartGuardInputs {
    /// Metadata-only evidence (no paths, launch tokens, nonces, titles, or
    /// requirement text).
    pub(crate) fn evidence(&self) -> serde_json::Value {
        serde_json::json!({
            "launch_ids_distinct": self.launch_ids_distinct,
            "product_overlap": self.product_overlap,
            "product_overlap_note": self.product_overlap_note,
            "prior_terminal_clean": self.prior_terminal_clean,
            "prior_liveness": liveness_evidence(&self.prior_liveness),
            "prior_lifetime_gone": self.prior_lifetime_gone,
            "fixture_liveness": liveness_evidence(&self.fixture_liveness),
            "fixture_identity_readable": self.fixture_identity_readable,
            "fixture_bundle_match": self.fixture_bundle_match,
            "fixture_exe_match": self.fixture_exe_match,
            "fixture_launch_match": self.fixture_launch_match,
            "fixture_nonce_match": self.fixture_nonce_match,
            "fixture_join_same": self.fixture_join_same,
            "fixture_join_note": self.fixture_join_note,
            "topology_readable": self.topology_readable,
            "current_owned_space": self.current_owned_space,
            "topology_row_exact": self.topology_row_exact,
            "windows_on_owned_space": self.windows_on_owned_space,
            "store_expected": self.store_expected.evidence(),
            "store_observed": self.store_observed.evidence(),
        })
    }
}

fn liveness_evidence(liveness: &Liveness) -> serde_json::Value {
    match liveness {
        Liveness::Gone => serde_json::json!({"state": "gone"}),
        Liveness::Present => serde_json::json!({"state": "present"}),
        Liveness::Unreadable(error) => {
            serde_json::json!({"state": "unreadable", "error": error})
        }
    }
}

fn guard_refusal(stage: GuardStage, detail: &str) -> String {
    format!("restart-guard {}: {detail}", stage.as_str())
}

/// Pure ordered restart guard. Checks the static launch identity, overlap,
/// the first launch's clean terminal evidence plus independent PID/lifetime
/// disappearance, the authenticated fixture lifetime and join, the owned
/// Space/topology/window placement, and finally exact store bytes plus
/// metadata. The first failing stage refuses; no later fact can mask it.
/// Pure; unit-tested.
pub(crate) fn restart_guard_verdict(inputs: &RestartGuardInputs) -> Result<(), String> {
    if !inputs.launch_ids_distinct {
        return Err(guard_refusal(
            GuardStage::LaunchIdentity,
            "the two launches share one run ID or output directory; refusing",
        ));
    }
    if inputs.product_overlap {
        return Err(guard_refusal(
            GuardStage::ProductOverlap,
            "another owned or pending product instance exists; refusing the second launch",
        ));
    }
    if !inputs.prior_terminal_clean {
        return Err(guard_refusal(
            GuardStage::PriorTerminal,
            "the first launch has no clean terminal evidence; refusing the second launch",
        ));
    }
    match &inputs.prior_liveness {
        Liveness::Gone => {}
        Liveness::Present => {
            return Err(guard_refusal(
                GuardStage::PriorPid,
                "the first product PID is still present; refusing overlapping launches",
            ));
        }
        Liveness::Unreadable(error) => {
            return Err(guard_refusal(
                GuardStage::PriorPid,
                &format!("the first product liveness is unreadable ({error}); refusing"),
            ));
        }
    }
    if !inputs.prior_lifetime_gone {
        return Err(guard_refusal(
            GuardStage::PriorLifetime,
            "the first product's bound executable/launch identity is still live; refusing",
        ));
    }
    match &inputs.fixture_liveness {
        Liveness::Present => {}
        Liveness::Gone => {
            return Err(guard_refusal(
                GuardStage::FixturePid,
                "the authenticated fixture PID is gone; refusing the second launch",
            ));
        }
        Liveness::Unreadable(error) => {
            return Err(guard_refusal(
                GuardStage::FixturePid,
                &format!("fixture liveness is unreadable ({error}); refusing"),
            ));
        }
    }
    if !inputs.fixture_identity_readable {
        return Err(guard_refusal(
            GuardStage::FixtureIdentity,
            "fixture identity is unreadable; refusing",
        ));
    }
    if !(inputs.fixture_bundle_match && inputs.fixture_exe_match && inputs.fixture_launch_match) {
        return Err(guard_refusal(
            GuardStage::FixtureIdentity,
            "fixture bundle/executable/launch identity drifted; refusing",
        ));
    }
    if !inputs.fixture_nonce_match {
        return Err(guard_refusal(
            GuardStage::FixtureNonce,
            "fixture announcement nonce changed; refusing",
        ));
    }
    if !inputs.fixture_join_same {
        return Err(guard_refusal(
            GuardStage::FixtureJoin,
            &format!(
                "authenticated fixture window join drifted{}; refusing",
                inputs
                    .fixture_join_note
                    .as_deref()
                    .map(|note| format!(" ({note})"))
                    .unwrap_or_default()
            ),
        ));
    }
    if !inputs.topology_readable {
        return Err(guard_refusal(
            GuardStage::OwnedSpace,
            "owned-Space topology is unreadable; refusing",
        ));
    }
    if !(inputs.current_owned_space && inputs.topology_row_exact) {
        return Err(guard_refusal(
            GuardStage::OwnedSpace,
            "the owned Space is no longer current or the row changed; refusing",
        ));
    }
    if !inputs.windows_on_owned_space {
        return Err(guard_refusal(
            GuardStage::WindowPlacement,
            "an owned window is no longer readable on the owned Space; refusing",
        ));
    }
    if inputs.store_expected != inputs.store_observed {
        return Err(guard_refusal(
            GuardStage::Store,
            "the isolated store bytes or metadata changed before the second launch; refusing",
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Isolated intent-store fingerprint (bytes plus metadata)
// ---------------------------------------------------------------------------

/// Maximum bytes read from any one store file (the store's own bound plus
/// one byte to detect an over-bound file).
const STORE_READ_BOUND: u64 = crate::intent_store::MAX_STORE_FILE_BYTES as u64;

/// Exact store-directory metadata retained for comparison.
#[derive(Debug, Clone, PartialEq, Eq)]
struct StoreDirMeta {
    mode: u32,
    uid: u32,
    ino: u64,
    mtime_ns: i64,
}

/// Exact one-file metadata plus bytes retained for comparison. The bytes are
/// kept in memory for exact equality and never emitted in evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
struct StoreFile {
    name: String,
    len: u64,
    mode: u32,
    uid: u32,
    ino: u64,
    mtime_ns: i64,
    bytes: Vec<u8>,
}

/// One isolated store fingerprint: absent, or an exact directory plus file
/// snapshot (names sorted). Equality covers every byte and every metadata
/// field retained here (mode, owner, inode, modification time, length), so a
/// same-bytes atomic rewrite still reports drift.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StoreSnapshot {
    present: bool,
    dir: Option<StoreDirMeta>,
    files: Vec<StoreFile>,
}

impl StoreSnapshot {
    fn absent() -> Self {
        Self {
            present: false,
            dir: None,
            files: Vec::new(),
        }
    }

    /// True when at least one intent-store row file is present.
    pub(crate) fn has_row(&self) -> bool {
        self.files
            .iter()
            .any(|file| file.name.starts_with("intent-v1-") && file.name.ends_with(".json"))
    }

    /// Metadata-only evidence: presence, entry counts, total bytes, exact
    /// mode/owner/timestamps, and a deterministic content digest. No file
    /// names (they carry the login namespace) and no raw bytes.
    pub(crate) fn evidence(&self) -> serde_json::Value {
        let total_bytes: u64 = self.files.iter().map(|file| file.len).sum();
        let rows = self
            .files
            .iter()
            .filter(|file| file.name.starts_with("intent-v1-") && file.name.ends_with(".json"))
            .count();
        serde_json::json!({
            "present": self.present,
            "files": self.files.len(),
            "rows": rows,
            "total_bytes": total_bytes,
            "dir_mode": self.dir.as_ref().map(|dir| format!("{:04o}", dir.mode)),
            "dir_uid": self.dir.as_ref().map(|dir| dir.uid),
            "dir_mtime_ns": self.dir.as_ref().map(|dir| dir.mtime_ns),
            "digest": format!("{:016x}", self.digest()),
        })
    }

    fn digest(&self) -> u64 {
        let mut hash = 0xcbf2_9ce4_8422_2325u64;
        if let Some(dir) = &self.dir {
            hash = fnv1a(hash, b"dir");
            hash = fnv1a(hash, &dir.mode.to_le_bytes());
            hash = fnv1a(hash, &dir.uid.to_le_bytes());
            hash = fnv1a(hash, &dir.ino.to_le_bytes());
            hash = fnv1a(hash, &dir.mtime_ns.to_le_bytes());
        }
        for file in &self.files {
            hash = fnv1a(hash, b"file");
            hash = fnv1a(hash, file.name.as_bytes());
            hash = fnv1a(hash, &file.len.to_le_bytes());
            hash = fnv1a(hash, &file.mode.to_le_bytes());
            hash = fnv1a(hash, &file.uid.to_le_bytes());
            hash = fnv1a(hash, &file.ino.to_le_bytes());
            hash = fnv1a(hash, &file.mtime_ns.to_le_bytes());
            hash = fnv1a(hash, &file.bytes);
        }
        hash
    }
}

fn fnv1a(mut hash: u64, bytes: &[u8]) -> u64 {
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Exact hydration label for the isolated store: an absent store is a valid
/// absence (no float intent was ever written), never a hydrated row set.
fn store_hydration_state(snapshot: &StoreSnapshot) -> &'static str {
    if !snapshot.present {
        "unchanged-absence"
    } else if snapshot.has_row() {
        "unchanged-rows"
    } else {
        "unchanged-directory-only"
    }
}

fn intent_row_file(name: &str) -> bool {
    name.starts_with("intent-v1-") && name.ends_with(".json")
}

/// Exact M3 float-persistence verdict over the isolated store bytes captured
/// after the first stop. Every intended float window must have exactly one
/// persisted row bound to the fixture PID, its exact launch lifetime and
/// bundle, and a decoded `float:true` intent; no other row may claim
/// `float:true`. Rows that do not claim a float are ignored here (the
/// fingerprint guard still covers their bytes). Any malformed/ambiguous
/// row refuses rather than guessing a binding.
fn require_m3_float_rows(
    snapshot: &StoreSnapshot,
    float_windows: &[(String, u32)],
    fixture_pid: u32,
    fixture_binding: &crate::intent_store::LiveBinding,
) -> Result<serde_json::Value, String> {
    if fixture_binding.bundle_id != FIXTURE_SCOPE {
        return Err(
            "restart m3: live fixture binding is not the signed fixture bundle; refusing"
                .to_owned(),
        );
    }
    if !snapshot.present {
        return Err(
            "restart m3: the isolated store is absent after the first stop (float persistence unproven); refusing the second launch"
                .to_owned(),
        );
    }
    let expected: BTreeMap<String, u32> = float_windows
        .iter()
        .map(|(_, wid)| (crate::runtime_types::window_id(fixture_pid, *wid), *wid))
        .collect();
    let mut rows: BTreeMap<String, crate::intent_store::StoredEntry> = BTreeMap::new();
    for file in snapshot
        .files
        .iter()
        .filter(|file| intent_row_file(&file.name))
    {
        for entry in crate::intent_store::decode_store_entries(&file.bytes)
            .map_err(|error| format!("restart m3: persisted store row unreadable ({error})"))?
        {
            if rows.insert(entry.win.clone(), entry).is_some() {
                return Err(
                    "restart m3: the isolated store repeats a window id across rows; refusing"
                        .to_owned(),
                );
            }
        }
    }
    if rows.is_empty() {
        return Err(
            "restart m3: the isolated store has no persisted row after the first stop (float persistence unproven); refusing the second launch"
                .to_owned(),
        );
    }
    let mut observed_floats = BTreeSet::new();
    for entry in rows.values() {
        let intent = tiler_core::intent_data::decode_intent_data(&entry.data).map_err(|error| {
            format!(
                "restart m3: persisted row intent is unusable ({}); refusing",
                error.as_str()
            )
        })?;
        if intent.float != Some(true) {
            continue;
        }
        observed_floats.insert(entry.win.clone());
        let Some(expected_wid) = expected.get(&entry.win) else {
            return Err(
                "restart m3: the isolated store holds a float row for a window that was not an intended float; refusing"
                    .to_owned(),
            );
        };
        if entry.pid != fixture_pid
            || entry.wid != *expected_wid
            || entry.launch_date_ns != fixture_binding.launch_date_ns
            || entry.bundle_id != fixture_binding.bundle_id
        {
            return Err(
                "restart m3: a persisted float row is not bound to the exact fixture lifetime; refusing"
                    .to_owned(),
            );
        }
    }
    for win in expected.keys() {
        if !observed_floats.contains(win) {
            return Err(
                "restart m3: an intended float window has no persisted float row after the first stop; refusing"
                    .to_owned(),
            );
        }
    }
    if observed_floats.len() != expected.len() {
        return Err(
            "restart m3: persisted float rows do not match the intended float set; refusing"
                .to_owned(),
        );
    }
    Ok(serde_json::json!({
        "expected_floats": expected.len(),
        "observed_float_rows": observed_floats.len(),
        "exact_lifetime_bindings": true,
        "no_unexpected_float_rows": true,
    }))
}

/// Read one isolated store fingerprint from an exact directory. Absent is a
/// valid fingerprint (no persistence yet); a symlink, non-directory, nested
/// entry, non-regular file, over-bound file, or unreadable metadata/bytes
/// refuses as uncertainty. Read-only; never creates, repairs, or deletes.
pub(crate) fn read_store_snapshot(store_dir: &Path) -> Result<StoreSnapshot, String> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let dir_meta = match std::fs::symlink_metadata(store_dir) {
        Ok(meta) if meta.file_type().is_symlink() => {
            return Err("restart store: isolated store is a symlink; refusing".to_owned());
        }
        Ok(meta) if meta.is_dir() => meta,
        Ok(_) => {
            return Err("restart store: isolated store is not a directory; refusing".to_owned());
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(StoreSnapshot::absent());
        }
        Err(error) => {
            return Err(format!(
                "restart store: isolated store metadata unreadable ({error}); refusing"
            ));
        }
    };
    let dir = StoreDirMeta {
        mode: dir_meta.permissions().mode(),
        uid: dir_meta.uid(),
        ino: dir_meta.ino(),
        mtime_ns: mtime_ns(&dir_meta),
    };
    let mut files = Vec::new();
    let entries = std::fs::read_dir(store_dir)
        .map_err(|error| format!("restart store: isolated store unreadable ({error}); refusing"))?;
    for entry in entries {
        let entry = entry.map_err(|error| {
            format!("restart store: store entry unreadable ({error}); refusing")
        })?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| "restart store: store entry name is not UTF-8; refusing".to_owned())?;
        // Open exactly once and fingerprint the opened file: a path stat
        // followed by a separate read is a TOCTOU window, and an unopened
        // path can grow between the length check and the read. `O_NOFOLLOW`
        // plus post-open metadata judge the object actually read, while a
        // bounded read of `bound + 1` refuses any file that grew past the
        // store envelope.
        use std::io::Read;
        use std::os::unix::fs::OpenOptionsExt;
        let opened = match std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(entry.path())
        {
            Ok(file) => file,
            Err(error) if error.raw_os_error() == Some(libc::ELOOP) => {
                return Err(format!(
                    "restart store: store entry {name} is a symlink; refusing"
                ));
            }
            Err(error) => {
                return Err(format!(
                    "restart store: store entry {name} unreadable ({error}); refusing"
                ));
            }
        };
        let meta = opened.metadata().map_err(|error| {
            format!("restart store: store entry {name} metadata unreadable ({error}); refusing")
        })?;
        if !meta.is_file() {
            return Err(format!(
                "restart store: store entry {name} is not a regular file; refusing"
            ));
        }
        if meta.len() > STORE_READ_BOUND {
            return Err(format!(
                "restart store: store entry {name} exceeds the store bound; refusing"
            ));
        }
        let mut bytes = Vec::with_capacity(usize::try_from(meta.len()).unwrap_or(0));
        opened
            .take(STORE_READ_BOUND + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| {
                format!("restart store: store entry {name} bytes unreadable ({error}); refusing")
            })?;
        if bytes.len() as u64 > STORE_READ_BOUND {
            return Err(format!(
                "restart store: store entry {name} exceeds the store bound; refusing"
            ));
        }
        files.push(StoreFile {
            name,
            len: bytes.len() as u64,
            mode: meta.permissions().mode(),
            uid: meta.uid(),
            ino: meta.ino(),
            mtime_ns: mtime_ns(&meta),
            bytes,
        });
    }
    files.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(StoreSnapshot {
        present: true,
        dir: Some(dir),
        files,
    })
}

fn mtime_ns(meta: &std::fs::Metadata) -> i64 {
    use std::os::unix::fs::MetadataExt;
    meta.mtime()
        .saturating_mul(1_000_000_000)
        .saturating_add(meta.mtime_nsec())
}

// ---------------------------------------------------------------------------
// Live orchestration (macOS only)
// ---------------------------------------------------------------------------

/// Capture one exact independent frame set for every joined window: AX frame
/// plus CG bounds plus the CG onscreen fact. A missing/off-screen CG entry or
/// unreadable AX frame refuses; nothing is defaulted. Read-only.
#[cfg(target_os = "macos")]
fn capture_window_frames(
    pid: u32,
    joined: &[(String, u32)],
) -> Result<Vec<WindowFrames>, (i32, String)> {
    let cg = crate::ffi::cg_window_list_basic().map_err(|error| {
        (
            1,
            format!("restart frames: CG snapshot unreadable ({error})"),
        )
    })?;
    let mut frames = Vec::new();
    for (caller, wid) in joined {
        let snapshot = crate::fixture_sample::read_joined_frame(pid, *wid).map_err(|error| {
            (
                1,
                format!("restart frames: {caller} AX unreadable ({error})"),
            )
        })?;
        if snapshot.cg_onscreen != Some(true) {
            return Err((1, format!("restart frames: {caller} is not CG-onscreen")));
        }
        let entry = cg
            .iter()
            .find(|entry| entry.pid == pid && entry.wid == *wid && entry.onscreen)
            .ok_or_else(|| {
                (
                    1,
                    format!("restart frames: {caller} has no live CG owner entry"),
                )
            })?;
        frames.push(WindowFrames {
            caller: caller.clone(),
            wid: *wid,
            ax: snapshot.frame,
            cg: FramePoint {
                x: entry.bounds.x,
                y: entry.bounds.y,
                w: entry.bounds.w,
                h: entry.bounds.h,
            },
            cg_onscreen: true,
        });
    }
    Ok(frames)
}

/// Exact owned-Space facts shared by the pre-launch guard and the
/// post-restart capture: topology readable, owned SID current, and the
/// display row exactly the owned row. One reader, two enforcement points.
#[cfg(target_os = "macos")]
fn owned_topology_facts(
    transport: &mut LiveLegTransport,
    owned: &PostOnlyOwnedSpace,
) -> (bool, bool, bool) {
    match transport.space_snapshot() {
        Ok(topology) => match topology.displays.first() {
            Some(row) => (
                true,
                row.current == owned.sid(),
                row.spaces.as_slice() == owned.spaces_after(),
            ),
            None => (false, false, false),
        },
        Err(_) => (false, false, false),
    }
}

/// Pure app-table test for an active foreign product instance. The
/// controller itself is excluded by exact PID, and only live processes
/// count; a terminated table entry is not an active instance.
#[cfg(target_os = "macos")]
fn foreign_product_in_apps(
    apps: &[(i32, String)],
    self_pid: i32,
    product_bundle: &str,
    alive: impl Fn(i32) -> bool,
) -> bool {
    apps.iter()
        .any(|(pid, bundle)| *pid != self_pid && bundle.as_str() == product_bundle && alive(*pid))
}

/// Fail-closed overlap proof over the live app table plus CG window owners:
/// any active product bundle instance other than the controller refuses the
/// restart, including a foreign product tile whose owner is still alive.
/// An unreadable/empty app table or CG list is an error, never "no overlap".
#[cfg(target_os = "macos")]
fn foreign_product_overlap(
    leg: &LiveLeg<LiveLegTransport>,
    fixture_pid: u32,
) -> Result<bool, String> {
    if leg.has_owned_product()
        || leg
            .pending_launches()
            .iter()
            .any(|pending| pending.what == "product")
    {
        return Ok(true);
    }
    let apps = crate::ffi::running_apps();
    if apps.is_empty() {
        return Err(
            "running-application table unreadable or empty; refusing overlap proof".to_owned(),
        );
    }
    let self_pid = i32::try_from(std::process::id()).unwrap_or(-1);
    if foreign_product_in_apps(&apps, self_pid, crate::BUNDLE_ID, |pid| {
        crate::ffi::process_exists(pid)
    }) {
        return Ok(true);
    }
    let cg = crate::ffi::cg_window_list_basic()
        .map_err(|error| format!("CG window list unreadable ({error}); refusing overlap proof"))?;
    for window in cg {
        if window.pid == std::process::id() || window.pid == fixture_pid {
            continue;
        }
        let pid = i32::try_from(window.pid).unwrap_or(-1);
        if pid <= 1 || !crate::ffi::process_exists(pid) {
            continue;
        }
        if crate::ffi::bundle_id_for_pid(pid).as_deref() == Some(crate::BUNDLE_ID) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Wait for the single authenticated post-stop frame write to land exactly:
/// target AX and CG both equal the requested ordinary frame, every other
/// joined window unchanged, all CG-onscreen. Bounded; a timeout reports the
/// observed target frame and never retries the write.
#[cfg(target_os = "macos")]
fn await_nudged_frames(
    pid: u32,
    joined: &[(String, u32)],
    before: &[WindowFrames],
    target: &str,
    requested: &FramePoint,
) -> Result<Vec<WindowFrames>, (i32, String)> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    loop {
        if crate::space_drag_diag_live::diag_interrupted() {
            return Err((
                1,
                "restart effect: interrupted after the post-stop nudge; refusing".to_owned(),
            ));
        }
        let current = capture_window_frames(pid, joined)?;
        let target_ok = current
            .iter()
            .find(|frame| frame.caller == target)
            .is_some_and(|frame| {
                frame.ax == *requested && frame.cg == *requested && frame.cg_onscreen
            });
        let others_ok = before.iter().all(|frame| {
            frame.caller == target
                || current
                    .iter()
                    .find(|candidate| candidate.caller == frame.caller)
                    .is_some_and(|candidate| candidate.ax == frame.ax && candidate.cg == frame.cg)
        });
        if target_ok && others_ok {
            return Ok(current);
        }
        if std::time::Instant::now() >= deadline {
            let observed = current
                .iter()
                .find(|frame| frame.caller == target)
                .map(|frame| serde_json::json!({"ax": frame.ax, "cg": frame.cg}))
                .unwrap_or_else(|| serde_json::json!({"absent": true}));
            return Err((
                1,
                serde_json::json!({
                    "stage": "restart-effect",
                    "error": "the post-stop nudge did not settle to the exact requested AX and CG frame",
                    "target": target,
                    "requested": requested,
                    "observed": observed,
                })
                .to_string(),
            ));
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

/// The one M3 post-stop effect: nudge one known tile inward by an ordinary
/// inset through the existing signed-fixture frame-write choke point (fresh
/// socket nonce, bundle/exe/launch/signature proof, single AX write). The
/// commit boundary additionally rechecks the owned Space row and the exact
/// expected fixture lifetime before that write. Afterwards, independently
/// read exact AX plus CG frames and require the target changed to the
/// requested frame with every other window unchanged. No product is alive or
/// launched in this phase.
#[cfg(target_os = "macos")]
fn nudge_one_known_tile(
    context: OwnedRestartFixture<'_>,
    arrangement: &ArrangementEvidence,
    signer: &str,
) -> Result<serde_json::Value, (i32, String)> {
    let fixture = context.fixture;
    let joined = context.joined;
    if crate::space_drag_diag_live::diag_interrupted() {
        return Err((
            1,
            "restart effect: interrupted before the post-stop nudge; refusing".to_owned(),
        ));
    }
    let target = arrangement.tiles.first().ok_or_else(|| {
        (
            1,
            "restart effect: no pre-stop tile is available to nudge; refusing".to_owned(),
        )
    })?;
    let before = capture_window_frames(fixture.pid, joined)?;
    let before_target = before
        .iter()
        .find(|frame| frame.caller == target.caller)
        .ok_or_else(|| {
            (
                1,
                format!(
                    "restart effect: nudge target {} is absent from the independent capture; refusing",
                    target.caller
                ),
            )
        })?;
    const INSET: f64 = 16.0;
    if before_target.ax.w <= INSET * 2.0 || before_target.ax.h <= INSET * 2.0 {
        return Err((
            1,
            format!(
                "restart effect: nudge target {} is too small for an ordinary inset; refusing",
                target.caller
            ),
        ));
    }
    let requested = FramePoint {
        x: before_target.ax.x + INSET,
        y: before_target.ax.y + INSET,
        w: before_target.ax.w - INSET * 2.0,
        h: before_target.ax.h - INSET * 2.0,
    };
    let args = crate::fixture_frame::FixtureFrameWriteArgs {
        announcement_file: fixture
            .run_dir
            .join("announcement.json")
            .to_string_lossy()
            .into_owned(),
        caller: target.caller.clone(),
        wid: target.wid,
        x: requested.x,
        y: requested.y,
        w: requested.w,
        h: requested.h,
        signer: signer.to_owned(),
    };
    // The signed writer's commit boundary rechecks the owned Space and the
    // exact expected fixture lifetime before its single AX write. Standalone
    // frame-write invocations pass no check and keep their existing proof.
    let fixture_pid = fixture.pid;
    let expected_exe = fixture.exe.clone();
    let expected_launch = fixture.launch.clone();
    let owned_sid = context.owned.sid();
    let owned_spaces = context.owned.spaces_after().to_vec();
    let commit_check: Option<crate::fixture_frame::FixtureFrameCommitCheck> =
        Some(Box::new(move || {
            let identity =
                crate::ffi::ns_app_identity_for_pid(i32::try_from(fixture_pid).unwrap_or(0));
            if identity.bundle.as_deref() != Some(FIXTURE_SCOPE)
                || identity.exe.as_deref() != Some(expected_exe.as_str())
                || identity.launch_date.as_deref() != Some(expected_launch.as_str())
            {
                return Err("restart effect commit: fixture lifetime drifted; refusing".to_owned());
            }
            let mut spaces = crate::space_probe::LiveNativeSpaces::new(
                crate::live_leg::LEG_CAFFEINATE_SECONDS,
            );
            let topology = spaces.space_snapshot().map_err(|error| {
                format!("restart effect commit: owned-Space snapshot unreadable ({error})")
            })?;
            let row = topology
                .displays
                .first()
                .ok_or_else(|| "restart effect commit: no display row".to_owned())?;
            if row.current != owned_sid || row.spaces.as_slice() != owned_spaces.as_slice() {
                return Err(
                    "restart effect commit: owned Space is no longer current or its row changed; refusing"
                        .to_owned(),
                );
            }
            Ok(())
        }));
    let raw = crate::fixture_frame::run_fixture_frame_write_under_interrupt(&args, commit_check)
        .map_err(|(code, body)| {
            (
                code,
                format!("restart effect: authenticated frame write refused ({body})"),
            )
        })?;
    let write: serde_json::Value = serde_json::from_str(&raw).map_err(|error| {
        (
            1,
            format!("restart effect: frame-write report is not JSON ({error})"),
        )
    })?;
    if write.get("outcome").and_then(serde_json::Value::as_str) != Some("written") {
        return Err((
            1,
            serde_json::json!({
                "stage": "restart-effect",
                "error": "the authenticated post-stop frame write did not report a written outcome",
                "write": write,
            })
            .to_string(),
        ));
    }
    let after = await_nudged_frames(fixture.pid, joined, &before, &target.caller, &requested)?;
    let after_target = after
        .iter()
        .find(|frame| frame.caller == target.caller)
        .cloned()
        .ok_or_else(|| {
            (
                1,
                format!(
                    "restart effect: nudge target {} vanished after the write; refusing",
                    target.caller
                ),
            )
        })?;
    Ok(serde_json::json!({
        "target_caller": target.caller,
        "target_wid": target.wid,
        "requested": requested,
        "before": before_target,
        "after": after_target,
        "ax_exact": true,
        "cg_exact": true,
        "other_windows_unchanged": true,
        "write": {
            "outcome": "written",
            "position_status": write.get("position_status").cloned(),
            "size_status": write.get("size_status").cloned(),
        },
    }))
}

/// Re-verify every identity and placement fact at the post-restart capture:
/// the registered second product is the exact live PID/lifetime, the
/// fixture bundle/executable/launch and nonce still bind, the joined window
/// set is unchanged, the owned Space is current with an exact topology row,
/// and every joined window is readable on it. Any uncertainty refuses.
#[cfg(target_os = "macos")]
fn require_post_restart_capture_bound(
    leg: &mut LiveLeg<LiveLegTransport>,
    context: OwnedRestartFixture<'_>,
    launch2_pid: u32,
    launch2_exe: &str,
    launch2_launch: &str,
) -> Result<serde_json::Value, (i32, String)> {
    let fixture = context.fixture;
    let joined = context.joined;
    let owned = context.owned;
    let record_exact = leg.owned_product_record().is_some_and(|record| {
        record.pid == launch2_pid
            && record.exe == launch2_exe
            && record.launch == launch2_launch
            && record.launch_services
    });
    let product_identity =
        crate::ffi::ns_app_identity_for_pid(i32::try_from(launch2_pid).unwrap_or(0));
    let product_live_exact = product_identity.exe.as_deref() == Some(launch2_exe)
        && product_identity.launch_date.as_deref() == Some(launch2_launch);
    let fixture_identity = crate::fixture_post::fixture_process_identity(fixture.pid);
    let (fixture_identity_readable, fixture_bundle_match, fixture_exe_match, fixture_launch_match) =
        match fixture_identity {
            Ok(identity) => (
                true,
                identity.bundle.as_deref() == Some(FIXTURE_SCOPE),
                identity.exe.as_deref() == Some(fixture.exe.as_str()),
                identity.launch.as_deref() == Some(fixture.launch.as_str()),
            ),
            Err(_) => (false, false, false, false),
        };
    let fixture_nonce_match =
        read_run_nonce(&fixture.run_dir).is_ok_and(|live| live == context.nonce);
    let callers: Vec<String> = joined.iter().map(|(caller, _)| caller.clone()).collect();
    let fixture_join_same =
        join_fixture_windows(
            fixture.pid,
            &fixture.socket_path,
            context.nonce,
            &callers,
        )
        .is_ok_and(|rejoined| rejoined == joined);
    let (topology_readable, current_owned_space, topology_row_exact) =
        owned_topology_facts(leg.transport_mut(), owned);
    let windows_on_owned_space = await_joined_on_owned_space(joined, owned.sid()).is_ok();
    let exact = record_exact
        && product_live_exact
        && fixture_identity_readable
        && fixture_bundle_match
        && fixture_exe_match
        && fixture_launch_match
        && fixture_nonce_match
        && fixture_join_same
        && topology_readable
        && current_owned_space
        && topology_row_exact
        && windows_on_owned_space;
    let evidence = serde_json::json!({
        "product_record_exact": record_exact,
        "product_pid": launch2_pid,
        "product_live_identity_exact": product_live_exact,
        "fixture_identity_readable": fixture_identity_readable,
        "fixture_bundle_match": fixture_bundle_match,
        "fixture_exe_match": fixture_exe_match,
        "fixture_launch_match": fixture_launch_match,
        "fixture_nonce_match": fixture_nonce_match,
        "fixture_join_same": fixture_join_same,
        "topology_readable": topology_readable,
        "current_owned_space": current_owned_space,
        "topology_row_exact": topology_row_exact,
        "windows_on_owned_space": windows_on_owned_space,
    });
    if !exact {
        return Err((
            1,
            serde_json::json!({
                "stage": "post-restart-capture",
                "error": "the post-restart capture is not bound to the exact product, fixture, owned Space and window topology; refusing",
                "capture": evidence,
            })
            .to_string(),
        ));
    }
    Ok(evidence)
}

/// True when one product stderr line is the runtime's own started record.
/// The log is per launch output directory; the record carries pid 0 by
/// design, so only the fixed kind/outcome pair is matched. Pure;
/// unit-tested.
fn runtime_started_seen(raw: &str) -> bool {
    raw.lines()
        .any(|line| line.contains("kind=runtime outcome=started"))
}

/// Bounded poll for the product's runtime-started stderr record. The
/// `tilestarted` announcement is printed before runtime startup, so this is
/// the earliest honest point to begin frame settlement. A log that never
/// arrives refuses rather than capturing pre-runtime frames.
#[cfg(target_os = "macos")]
fn await_product_runtime_started(product_dir: &Path) -> Result<(), (i32, String)> {
    let log = product_dir.join("stderr.log");
    let deadline = std::time::Instant::now()
        + std::time::Duration::from_millis(RESTART_PRODUCT_START_DEADLINE_MS);
    loop {
        if let Ok(raw) = std::fs::read_to_string(&log)
            && runtime_started_seen(&raw)
        {
            return Ok(());
        }
        if std::time::Instant::now() >= deadline {
            return Err((
                1,
                format!(
                    "restart product: no runtime-started evidence in {} within {RESTART_PRODUCT_START_DEADLINE_MS}ms; refusing before capture",
                    log.display()
                ),
            ));
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

/// Wait for one settled frame set: two consecutive exact captures agreeing
/// for at least [`RESTART_STABLE_SPAN_MS`] within
/// [`RESTART_SETTLE_DEADLINE_MS`]. Every capture is an independent AX plus CG
/// read; a failed read or an unsettled deadline refuses rather than
/// capturing a transient frame.
#[cfg(target_os = "macos")]
fn await_settled_frames(
    pid: u32,
    joined: &[(String, u32)],
) -> Result<Vec<WindowFrames>, (i32, String)> {
    let deadline =
        std::time::Instant::now() + std::time::Duration::from_millis(RESTART_SETTLE_DEADLINE_MS);
    let mut previous: Option<Vec<WindowFrames>> = None;
    let mut stable_since = std::time::Instant::now();
    loop {
        if crate::space_drag_diag_live::diag_interrupted() {
            return Err((
                1,
                "fixture-restart: interrupted (SIGINT/SIGTERM latched); restoring".to_owned(),
            ));
        }
        let current = capture_window_frames(pid, joined)?;
        match &previous {
            Some(last) if *last == current => {
                if stable_since.elapsed().as_millis() as u64 >= RESTART_STABLE_SPAN_MS {
                    return Ok(current);
                }
            }
            _ => {
                previous = Some(current);
                stable_since = std::time::Instant::now();
            }
        }
        if std::time::Instant::now() >= deadline {
            return Err((
                1,
                format!(
                    "fixture-restart: frames did not settle within {RESTART_SETTLE_DEADLINE_MS}ms; refusing"
                ),
            ));
        }
        std::thread::sleep(std::time::Duration::from_millis(RESTART_READ_INTERVAL_MS));
    }
}

/// Pre-stop evidence held across the relaunch.
struct PreStopEvidence {
    /// M3: settled arrangement after the floats (floats plus tiles).
    arrangement: Option<ArrangementEvidence>,
    /// M4: settled tiled base, then the active composed overlay.
    base: Option<Vec<WindowFrames>>,
    overlay: Option<Vec<WindowFrames>>,
}

/// One authenticated fixture instance plus its joined windows and the one
/// owned Space they were born on. Grouped so the guard, nudge and capture
/// checks all read the same bound facts rather than parallel arguments.
#[cfg(target_os = "macos")]
#[derive(Clone, Copy)]
struct OwnedRestartFixture<'a> {
    fixture: &'a FixtureRecord,
    nonce: &'a str,
    joined: &'a [(String, u32)],
    owned: &'a PostOnlyOwnedSpace,
}

/// Run every pre-stop chord through one admitted driver scoped to this
/// function: the driver (and its admission) cannot be used after the first
/// stop, so the second launch receives no new input by construction.
#[cfg(target_os = "macos")]
fn run_pre_stop_phase(
    leg: &mut LiveLeg<LiveLegTransport>,
    opts: &FixtureRestartOpts,
    announcement: &str,
    nonce: &str,
    joined: &[(String, u32)],
    fixture_pid: u32,
) -> Result<PreStopEvidence, (i32, String)> {
    let admission = leg.admit(fixture_pid, None)?;
    let live = crate::fixture_post::LiveTransport;
    let driver = crate::fixture_post::FixtureDriver::bind(
        &live,
        announcement,
        nonce,
        &opts.signer,
        admission,
    )?;
    let initial = await_settled_frames(fixture_pid, joined)?;
    match opts.mode {
        RestartMode::M3 => {
            for (caller, wid) in joined.iter().take(opts.floats) {
                driver.focus(caller, *wid)?;
                settle_hold(FOCUS_SETTLE_MS)?;
                driver.post(caller, *wid, "float-g", opts.modifier)?;
                settle_hold(RESTART_CHORD_SETTLE_MS)?;
            }
            let settled = await_settled_frames(fixture_pid, joined)?;
            let mut arrangement = partition_arrangement(&settled, opts.floats);
            arrangement.tile_reflowed =
                require_tile_reflow(&initial, &arrangement.tiles).map_err(|error| (1, error))?;
            Ok(PreStopEvidence {
                arrangement: Some(arrangement),
                base: None,
                overlay: None,
            })
        }
        RestartMode::M4 => {
            let (target_caller, target_wid) = joined
                .first()
                .cloned()
                .ok_or_else(|| (1, "fixture-restart: no joined window for m4".to_owned()))?;
            driver.focus(&target_caller, target_wid)?;
            settle_hold(FOCUS_SETTLE_MS)?;
            driver.post(&target_caller, target_wid, "maximize-m", opts.modifier)?;
            settle_hold(RESTART_CHORD_SETTLE_MS)?;
            let overlay = await_settled_frames(fixture_pid, joined)?;
            require_distinct_overlay(&initial, &overlay, &target_caller)
                .map_err(|error| (1, error))?;
            Ok(PreStopEvidence {
                arrangement: None,
                base: Some(initial),
                overlay: Some(overlay),
            })
        }
    }
}

/// Read every guard fact at one full checkpoint: before the one nudge and
/// again before the second launch. No effect except read-only socket/native
/// reads; an unreadable fact is recorded as an uncertainty for the pure
/// verdict, never defaulted to success.
#[cfg(target_os = "macos")]
fn observe_restart_guard(
    leg: &mut LiveLeg<LiveLegTransport>,
    context: OwnedRestartFixture<'_>,
    prior: &RestartStopEvidence,
    store_dir: &Path,
    expected_store: &StoreSnapshot,
    launch_ids_distinct: bool,
) -> Result<RestartGuardInputs, (i32, String)> {
    let fixture = context.fixture;
    let joined = context.joined;
    let owned = context.owned;
    let store_observed = read_store_snapshot(store_dir).map_err(|error| (1, error))?;
    let (product_overlap, product_overlap_note) = match foreign_product_overlap(leg, fixture.pid) {
        Ok(overlap) => (overlap, None),
        Err(error) => (true, Some(error)),
    };
    let prior_liveness = match leg.transport_mut().process_gone(prior.pid) {
        Ok(true) => Liveness::Gone,
        Ok(false) => Liveness::Present,
        Err(error) => Liveness::Unreadable(error),
    };
    let fixture_liveness = match leg.transport_mut().process_gone(fixture.pid) {
        Ok(true) => Liveness::Gone,
        Ok(false) => Liveness::Present,
        Err(error) => Liveness::Unreadable(error),
    };
    let pid = i32::try_from(prior.pid).unwrap_or(0);
    let identity = crate::ffi::ns_app_identity_for_pid(pid);
    let prior_lifetime_gone = !(identity.exe.as_deref() == Some(prior.exe.as_str())
        && identity.launch_date.as_deref() == Some(prior.launch.as_str()));
    let identity = crate::fixture_post::fixture_process_identity(fixture.pid);
    let (fixture_identity_readable, fixture_bundle_match, fixture_exe_match, fixture_launch_match) =
        match identity {
            Ok(identity) => (
                true,
                identity.bundle.as_deref() == Some(FIXTURE_SCOPE),
                identity.exe.as_deref() == Some(fixture.exe.as_str()),
                identity.launch.as_deref() == Some(fixture.launch.as_str()),
            ),
            Err(_) => (false, false, false, false),
        };
    let fixture_nonce_match =
        read_run_nonce(&fixture.run_dir).is_ok_and(|live| live == context.nonce);
    let callers: Vec<String> = joined.iter().map(|(caller, _)| caller.clone()).collect();
    let (fixture_join_same, fixture_join_note) =
        match join_fixture_windows(
            fixture.pid,
            &fixture.socket_path,
            context.nonce,
            &callers,
        ) {
            Ok(rejoined) => (rejoined == joined, None),
            Err(error) => (false, Some(error)),
        };
    let (topology_readable, current_owned_space, topology_row_exact) =
        owned_topology_facts(leg.transport_mut(), owned);
    let windows_on_owned_space = await_joined_on_owned_space(joined, owned.sid()).is_ok();
    Ok(RestartGuardInputs {
        launch_ids_distinct,
        product_overlap,
        product_overlap_note,
        prior_terminal_clean: clean_terminal_verdict(&prior.outcome).is_ok(),
        prior_liveness,
        prior_lifetime_gone,
        fixture_liveness,
        fixture_identity_readable,
        fixture_bundle_match,
        fixture_exe_match,
        fixture_launch_match,
        fixture_nonce_match,
        fixture_join_same,
        fixture_join_note,
        topology_readable,
        current_owned_space,
        topology_row_exact,
        windows_on_owned_space,
        store_expected: expected_store.clone(),
        store_observed,
    })
}

/// Post-restart phase: prove an initial full guard and the exact persisted
/// M3 rows before the one nudge, prove a fresh full guard after it and before
/// the second launch, bind the second authenticated instance into its distinct
/// run ID/output directory, capture the hydrated frame set with no driver in
/// scope, compare, then stop exactly that product. M4 never nudges. No chord
/// can be posted here.
#[cfg(target_os = "macos")]
#[allow(clippy::too_many_arguments)]
fn run_post_restart_phase(
    leg: &mut LiveLeg<LiveLegTransport>,
    opts: &FixtureRestartOpts,
    fixture: &FixtureRecord,
    nonce: &str,
    joined: &[(String, u32)],
    owned: &PostOnlyOwnedSpace,
    pre: &PreStopEvidence,
    prior: &RestartStopEvidence,
    store_dir: &Path,
    store_after_first: &StoreSnapshot,
    launch1_run_id: &str,
    launch2_run_id: &str,
    launch2_dir: &Path,
) -> Result<serde_json::Value, (i32, String)> {
    let launch_ids_distinct = launch1_run_id != launch2_run_id;
    let owned_fixture = OwnedRestartFixture { fixture, nonce, joined, owned };
    // An initial full guard proves the pre-nudge facts, including that no
    // product instance is alive, before the one allowed effect. M3 then
    // proves the exact persisted float rows before that effect.
    let initial_guard = observe_restart_guard(
        leg,
        owned_fixture,
        prior,
        store_dir,
        store_after_first,
        launch_ids_distinct,
    )?;
    let initial_guard_evidence = initial_guard.evidence();
    if let Err(error) = restart_guard_verdict(&initial_guard) {
        return Err((
            1,
            serde_json::json!({
                "stage": "restart-guard",
                "error": error,
                "guard": initial_guard_evidence,
            })
            .to_string(),
        ));
    }
    // M3: the store must hold an exact persisted float row for every intended
    // float window, bound to the authenticated fixture lifetime, with no
    // unexpected float row. Presence alone is not persistence evidence.
    let float_rows = if opts.mode == RestartMode::M3 {
        let fixture_binding = crate::intent_store::live_binding(fixture.pid).ok_or_else(|| {
            (
                1,
                "fixture-restart m3: live fixture lifetime binding is unreadable; refusing"
                    .to_owned(),
            )
        })?;
        Some(
            require_m3_float_rows(
                store_after_first,
                &joined[..opts.floats],
                fixture.pid,
                &fixture_binding,
            )
            .map_err(|error| (1, error))?,
        )
    } else {
        None
    };
    // M3 alone nudges one known tile after the persisted rows are proven.
    // M4 leaves the overlay untouched until the second launch restores base.
    let nudge = if opts.mode == RestartMode::M3 {
        let arrangement = pre.arrangement.as_ref().ok_or_else(|| {
            (
                1,
                "fixture-restart m3: pre-stop arrangement is missing".to_owned(),
            )
        })?;
        Some(nudge_one_known_tile(owned_fixture, arrangement, &opts.signer)?)
    } else {
        None
    };
    // A fresh full guard follows the effect and precedes the second launch,
    // so the nudge cannot race a hidden overlap or changed binding.
    let guard = if opts.mode == RestartMode::M3 {
        observe_restart_guard(
            leg,
            owned_fixture,
            prior,
            store_dir,
            store_after_first,
            launch_ids_distinct,
        )?
    } else {
        initial_guard
    };
    let guard_evidence = guard.evidence();
    if let Err(error) = restart_guard_verdict(&guard) {
        return Err((
            1,
            serde_json::json!({
                "stage": "restart-guard",
                "error": error,
                "guard": guard_evidence,
            })
            .to_string(),
        ));
    }
    let product_bundle = product_bundle_path().map_err(|error| (2, error))?;
    let argv = product_argv(
        opts.seconds,
        opts.preset,
        opts.modifier,
        false,
        launch2_dir,
        &[owned.sid()],
        Some(owned.starting_sid()),
    );
    std::fs::write(launch2_dir.join("run-id"), format!("{launch2_run_id}\n")).map_err(|error| {
        (
            1,
            format!("fixture-restart: second run-id refused ({error})"),
        )
    })?;
    leg.note_pending_launch(crate::live_leg::PendingLaunch {
        run_dir: launch2_dir.to_path_buf(),
        expected_bundle: crate::BUNDLE_ID.to_owned(),
        what: "product",
    });
    let record = match launch_product_ls(
        &product_bundle,
        launch2_dir,
        store_dir,
        &argv,
        launch2_run_id,
        false,
    ) {
        Ok(record) => record,
        Err(error) => {
            let _ = leg.resolve_pending_absence("product");
            return Err((1, error));
        }
    };
    let launch2_pid = record.pid;
    let launch2_exe = record.exe.clone();
    let launch2_launch = record.launch.clone();
    leg.register_product_ls(record)?;
    await_product_runtime_started(launch2_dir)?;
    settle_hold(RESTART_STARTUP_SETTLE_MS)?;
    // Hydration capture before ANY new input: the driver was dropped with the
    // pre-stop phase and is not constructed here. The joined windows remain
    // owned by the authenticated fixture process, never by the product.
    let after = await_settled_frames(fixture.pid, joined)?;
    let capture_binding = require_post_restart_capture_bound(
        leg,
        owned_fixture,
        launch2_pid,
        &launch2_exe,
        &launch2_launch,
    )?;
    let store_after_restart = read_store_snapshot(store_dir).map_err(|error| (1, error))?;
    if &store_after_restart != store_after_first {
        return Err((
            1,
            serde_json::json!({
                "stage": "restart-store",
                "error": "the isolated store bytes or metadata changed across the restart",
                "store_before": store_after_first.evidence(),
                "store_after": store_after_restart.evidence(),
            })
            .to_string(),
        ));
    }
    let comparison = match opts.mode {
        RestartMode::M3 => {
            let arrangement = pre.arrangement.as_ref().ok_or_else(|| {
                (
                    1,
                    "fixture-restart m3: pre-stop arrangement is missing".to_owned(),
                )
            })?;
            compare_restart_arrangement(arrangement, &after).map_err(|error| {
                (
                    1,
                    serde_json::json!({
                        "stage": "restart-compare",
                        "error": error,
                        "before": arrangement,
                        "after": after,
                    })
                    .to_string(),
                )
            })?;
            serde_json::json!({
                "outcome": "m3-exact",
                "float_frames_exact": true,
                "tile_ax_multiset_exact": true,
                "tile_cg_multiset_exact": true,
                "tile_reflowed": arrangement.tile_reflowed,
                "before": arrangement,
                "after": after,
            })
        }
        RestartMode::M4 => {
            let base = pre
                .base
                .as_ref()
                .ok_or_else(|| (1, "fixture-restart m4: settled base is missing".to_owned()))?;
            let overlay = pre.overlay.as_ref().ok_or_else(|| {
                (
                    1,
                    "fixture-restart m4: pre-stop overlay is missing".to_owned(),
                )
            })?;
            let target = joined
                .first()
                .map(|(caller, _)| caller.as_str())
                .ok_or_else(|| (1, "fixture-restart m4: no target window".to_owned()))?;
            compare_restart_base(base, overlay, &after, target).map_err(|error| {
                (
                    1,
                    serde_json::json!({
                        "stage": "restart-compare",
                        "error": error,
                        "base": base,
                        "overlay": overlay,
                        "after": after,
                    })
                    .to_string(),
                )
            })?;
            serde_json::json!({
                "outcome": "m4-base-restored",
                "overlay_distinct": true,
                "after_equals_base": true,
                "target": target,
                "base": base,
                "overlay": overlay,
                "after": after,
            })
        }
    };
    // Stop exactly the second instance, then prove the store still matches
    // after an orderly stop and before the fixture/private-store teardown.
    let stop2 = leg.stop_product_for_restart().map_err(|error| {
        (
            1,
            serde_json::json!({"stage": "restart-stop-2", "error": error}).to_string(),
        )
    })?;
    let store_after_stop = read_store_snapshot(store_dir).map_err(|error| (1, error))?;
    // The restart proof is the hydration store (checked before any new
    // input); a clean second shutdown may legitimately rewrite the same
    // logical row. Report the post-stop fingerprint, but never fail the
    // restart proof on a post-proof shutdown write.
    let store_unchanged_after_stop = &store_after_stop == store_after_first;
    Ok(serde_json::json!({
        "mode": opts.mode.as_str(),
        "fixture": {
            "pid": fixture.pid,
            "windows": joined
                .iter()
                .map(|(caller, wid)| serde_json::json!({"caller": caller, "wid": wid}))
                .collect::<Vec<_>>(),
        },
        "owned_space": owned.evidence(),
        "launch1": {
            "run_id": launch1_run_id,
            "pid": prior.pid,
            "terminal": prior.outcome,
        },
        "initial_guard": initial_guard_evidence,
        "guard": guard_evidence,
        "post_stop_nudge": nudge,
        "m3_float_rows": float_rows,
        "launch2": {
            "run_id": launch2_run_id,
            "pid": launch2_pid,
            "post_input": false,
            "capture_identity": capture_binding,
        },
        "comparison": comparison,
        "store": {
            "after_first_stop": store_after_first.evidence(),
            "after_restart": store_after_restart.evidence(),
            "after_second_stop": store_after_stop.evidence(),
            "hydration_state": store_hydration_state(store_after_first),
            "unchanged": true,
            "unchanged_after_second_stop": store_unchanged_after_stop,
        },
        "second_stop": {
            "pid": stop2.pid,
            "terminal": stop2.outcome,
        },
    }))
}

/// One two-launch body. Any failure returns before `finish`, which always
/// runs the owned teardown (including the private store removal).
#[cfg(target_os = "macos")]
#[allow(clippy::too_many_lines)]
fn run_restart_body_macos(
    leg: &mut LiveLeg<LiveLegTransport>,
    opts: &FixtureRestartOpts,
) -> Result<serde_json::Value, (i32, String)> {
    use crate::live_leg::PendingLaunch;
    let run_dir = create_owned_run_dir("restart").map_err(|error| (2, error))?;
    let product_dir = run_dir.join("product");
    std::fs::create_dir(&product_dir)
        .map_err(|error| (1, format!("fixture-restart: product dir refused ({error})")))?;
    let launch1_dir = restart_launch_dir(&product_dir, 1);
    let launch2_dir = restart_launch_dir(&product_dir, 2);
    for dir in [&launch1_dir, &launch2_dir] {
        std::fs::create_dir(dir)
            .map_err(|error| (1, format!("fixture-restart: launch dir refused ({error})")))?;
    }
    let store_dir = product_dir.join(ISOLATED_STORE_LEAF);
    leg.note_pending_launch(PendingLaunch {
        run_dir: run_dir.clone(),
        expected_bundle: FIXTURE_SCOPE.to_owned(),
        what: "fixture",
    });
    let bundle = fixture_bundle_path().map_err(|error| (2, error))?;
    if let Err(error) = launch_fixture_bundle(&bundle, &run_dir) {
        let _ = leg.resolve_pending_absence("fixture");
        return Err((1, error));
    }
    let announcement = read_announcement(&run_dir).map_err(|error| (1, error))?;
    let parsed = crate::fixture_post::parse_fixture_announcement(&announcement)
        .map_err(|error| (1, format!("fixture-restart: announcement refused: {error}")))?;
    let identity = crate::fixture_post::fixture_process_identity(parsed.pid)
        .map_err(|error| (1, format!("fixture-restart: identity unreadable ({error})")))?;
    if identity.bundle.as_deref() != Some(FIXTURE_SCOPE) {
        return Err((
            1,
            "fixture-restart: live bundle is not the signed fixture scope; refusing".to_owned(),
        ));
    }
    let (exe, launch) = match (identity.exe, identity.launch) {
        (Some(exe), Some(launch)) if !exe.is_empty() && !launch.is_empty() => (exe, launch),
        _ => {
            return Err((
                1,
                "fixture-restart: executable or launch date unreadable (no LaunchServices lifetime); refusing"
                    .to_owned(),
            ));
        }
    };
    let nonce = read_run_nonce(&run_dir).map_err(|error| (1, error))?;
    if nonce != parsed.nonce {
        return Err((
            1,
            "fixture-restart: nonce mismatch (stale or foreign run); refusing".to_owned(),
        ));
    }
    let fixture_record = FixtureRecord {
        pid: parsed.pid,
        exe,
        launch,
        socket_path: parsed.socket.clone(),
        run_dir: run_dir.clone(),
    };
    leg.register_fixture(fixture_record.clone())?;
    leg.transport_mut()
        .set_fixture_control(parsed.socket.clone(), run_dir.clone());
    let callers: Vec<String> = (0..opts.windows).map(|index| format!("w{index}")).collect();
    // Establish and switch to the owned Space BEFORE spawning any fixture
    // window, matching the single-leg order: windows are born on the owned
    // Space, never on a user Space and then moved.
    let established = establish_post_only_owned_space(leg)?;
    let owned = established.evidence;
    spawn_owned_windows(&parsed.socket, &nonce, &callers).map_err(|error| (1, error))?;
    let joined = join_fixture_windows(parsed.pid, &parsed.socket, &nonce, &callers)
        .map_err(|error| (1, error))?;
    for (caller, wid) in &joined {
        leg.note_owned_window(caller, parsed.pid, *wid);
    }
    if let Err(observed) = await_joined_on_owned_space(&joined, owned.sid()) {
        return Err((
            1,
            post_only_placement_refusal(
                &owned,
                &observed,
                "restart fixture windows are not readable on the owned Space before the first product launch; refusing without posting",
                None,
            )
            .to_string(),
        ));
    }
    // No first product launch may start while a foreign product instance is
    // already alive. This runs before the pending product registration.
    match foreign_product_overlap(leg, parsed.pid) {
        Ok(false) => {}
        Ok(true) => {
            return Err((
                1,
                serde_json::json!({
                    "stage": "restart-launch1-preflight",
                    "error": "another product instance is active before the first launch; refusing",
                })
                .to_string(),
            ));
        }
        Err(error) => {
            return Err((
                1,
                serde_json::json!({
                    "stage": "restart-launch1-preflight",
                    "error": error,
                })
                .to_string(),
            ));
        }
    }
    let fixture_pid = parsed.pid;
    let run_id1 = restart_run_id(std::process::id(), fixture_pid, 1);
    let run_id2 = restart_run_id(std::process::id(), fixture_pid, 2);
    let product_bundle = product_bundle_path().map_err(|error| (2, error))?;
    let argv1 = product_argv(
        opts.seconds,
        opts.preset,
        opts.modifier,
        false,
        &launch1_dir,
        &[owned.sid()],
        Some(owned.starting_sid()),
    );
    std::fs::write(launch1_dir.join("run-id"), format!("{run_id1}\n")).map_err(|error| {
        (
            1,
            format!("fixture-restart: first run-id refused ({error})"),
        )
    })?;
    leg.note_pending_launch(PendingLaunch {
        run_dir: launch1_dir.clone(),
        expected_bundle: crate::BUNDLE_ID.to_owned(),
        what: "product",
    });
    let record1 = match launch_product_ls(
        &product_bundle,
        &launch1_dir,
        &store_dir,
        &argv1,
        &run_id1,
        false,
    ) {
        Ok(record) => record,
        Err(error) => {
            let _ = leg.resolve_pending_absence("product");
            return Err((1, error));
        }
    };
    leg.register_product_ls(record1)?;
    await_product_runtime_started(&launch1_dir)?;
    settle_hold(RESTART_STARTUP_SETTLE_MS)?;
    let pre = run_pre_stop_phase(leg, opts, &announcement, &nonce, &joined, fixture_pid)?;
    let prior = leg.stop_product_for_restart().map_err(|error| {
        (
            1,
            serde_json::json!({"stage": "restart-stop-1", "error": error}).to_string(),
        )
    })?;
    let store_after_first = read_store_snapshot(&store_dir).map_err(|error| (1, error))?;
    run_post_restart_phase(
        leg,
        opts,
        &fixture_record,
        &nonce,
        &joined,
        &owned,
        &pre,
        &prior,
        &store_dir,
        &store_after_first,
        &run_id1,
        &run_id2,
        &launch2_dir,
    )
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
    let mut leg = LiveLeg::begin(&mut transport, opts.seconds.saturating_add(60))?;
    let outcome = {
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (&mut leg, &opts);
            Err((
                2,
                "fixture-restart needs macOS (LaunchServices launches plus native joins)"
                    .to_owned(),
            ))
        }
        #[cfg(target_os = "macos")]
        {
            run_restart_body_macos(&mut leg, &opts)
        }
    };
    match outcome {
        Ok(evidence) => match leg.finish() {
            Ok(teardown) => {
                let mut envelope = serde_json::json!({
                    "outcome": "completed",
                    "restart": evidence,
                    "teardown": teardown,
                });
                envelope["mode"] = serde_json::json!(opts.mode.as_str());
                Ok(envelope.to_string())
            }
            Err(teardown) => {
                let teardown_body = match serde_json::from_str::<serde_json::Value>(&teardown.1) {
                    Ok(value) => value,
                    Err(_) => serde_json::Value::String(teardown.1),
                };
                Err((
                    teardown.0,
                    serde_json::json!({
                        "stage": "leg-finish",
                        "error": "restart evidence captured but teardown refused",
                        "restart": evidence,
                        "teardown": teardown_body,
                    })
                    .to_string(),
                ))
            }
        },
        Err(error) => {
            if let Err(teardown) = leg.finish() {
                eprintln!("fixture-restart teardown after failure: {}", teardown.1);
            }
            Err(error)
        }
    }
}

#[cfg(target_os = "macos")]
fn ensure_appkit_singleton() {
    crate::fixture_leg::ensure_appkit_singleton();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frames(caller: &str, wid: u32, x: f64, y: f64, w: f64, h: f64) -> WindowFrames {
        WindowFrames {
            caller: caller.to_owned(),
            wid,
            ax: FramePoint { x, y, w, h },
            cg: FramePoint { x, y, w, h },
            cg_onscreen: true,
        }
    }

    fn opts(mode: RestartMode, windows: usize, floats: usize) -> FixtureRestartOpts {
        FixtureRestartOpts {
            mode,
            seconds: 90,
            windows,
            floats,
            preset: InputPreset::Compatible,
            modifier: ProbeModifier::Ctrl,
            signer: "designated => test".to_owned(),
        }
    }

    #[test]
    fn mode_and_shape_validate_before_any_effect() {
        assert_eq!(RestartMode::parse("m3").expect("m3"), RestartMode::M3);
        assert_eq!(RestartMode::parse("m4").expect("m4"), RestartMode::M4);
        assert!(RestartMode::parse("m5").is_err());
        assert!(validate_restart_opts(&opts(RestartMode::M3, 4, 2)).is_ok());
        assert!(validate_restart_opts(&opts(RestartMode::M3, 4, 1)).is_err());
        assert!(validate_restart_opts(&opts(RestartMode::M3, 3, 2)).is_err());
        assert!(validate_restart_opts(&opts(RestartMode::M4, 2, 2)).is_err());
        assert!(validate_restart_opts(&opts(RestartMode::M4, 1, 0)).is_err());
        assert!(validate_restart_opts(&opts(RestartMode::M4, 2, 0)).is_ok());
        let mut bad_seconds = opts(RestartMode::M3, 4, 2);
        bad_seconds.seconds = LEG_MIN_SECONDS - 1;
        assert!(validate_restart_opts(&bad_seconds).is_err());
        bad_seconds.seconds = LEG_MAX_SECONDS + 1;
        assert!(validate_restart_opts(&bad_seconds).is_err());
        let mut no_signer = opts(RestartMode::M3, 4, 2);
        no_signer.signer = "  ".to_owned();
        assert!(validate_restart_opts(&no_signer).is_err());
        assert!(validate_restart_opts(&opts(RestartMode::M3, 9, 2)).is_err());
    }

    #[test]
    fn launch_ids_and_output_dirs_are_distinct() {
        let product_dir = Path::new("/tmp/restart/product");
        let first = restart_run_id(100, 200, 1);
        let second = restart_run_id(100, 200, 2);
        assert_ne!(first, second);
        assert!(first.ends_with("-1") && second.ends_with("-2"));
        assert_ne!(
            restart_launch_dir(product_dir, 1),
            restart_launch_dir(product_dir, 2)
        );
        assert_eq!(
            restart_launch_dir(product_dir, 1),
            product_dir.join("launch-1")
        );
    }

    fn guard() -> RestartGuardInputs {
        let store = StoreSnapshot {
            present: true,
            dir: Some(StoreDirMeta {
                mode: 0o700,
                uid: 501,
                ino: 7,
                mtime_ns: 11,
            }),
            files: vec![StoreFile {
                name: "intent-v1-501-100.json".to_owned(),
                len: 4,
                mode: 0o600,
                uid: 501,
                ino: 8,
                mtime_ns: 12,
                bytes: b"row\n".to_vec(),
            }],
        };
        RestartGuardInputs {
            launch_ids_distinct: true,
            product_overlap: false,
            product_overlap_note: None,
            prior_terminal_clean: true,
            prior_liveness: Liveness::Gone,
            prior_lifetime_gone: true,
            fixture_liveness: Liveness::Present,
            fixture_identity_readable: true,
            fixture_bundle_match: true,
            fixture_exe_match: true,
            fixture_launch_match: true,
            fixture_nonce_match: true,
            fixture_join_same: true,
            fixture_join_note: None,
            topology_readable: true,
            current_owned_space: true,
            topology_row_exact: true,
            windows_on_owned_space: true,
            store_expected: store.clone(),
            store_observed: store,
        }
    }

    #[test]
    fn guard_refuses_an_alive_prior_lifetime_before_relaunch() {
        let mut inputs = guard();
        inputs.prior_liveness = Liveness::Present;
        let error = restart_guard_verdict(&inputs).expect_err("alive prior refuses");
        assert!(error.contains("prior-pid"), "{error}");
        inputs.prior_liveness = Liveness::Unreadable("probe failed".to_owned());
        let error = restart_guard_verdict(&inputs).expect_err("unreadable prior refuses");
        assert!(error.contains("prior-pid"), "{error}");
        inputs.prior_liveness = Liveness::Gone;
        inputs.prior_lifetime_gone = false;
        let error = restart_guard_verdict(&inputs).expect_err("live prior lifetime refuses");
        assert!(error.contains("prior-lifetime"), "{error}");
        inputs.prior_lifetime_gone = true;
        assert!(restart_guard_verdict(&inputs).is_ok());
    }

    #[test]
    fn guard_refuses_overlap_identity_topology_and_store_drift() {
        let mut inputs = guard();
        inputs.product_overlap = true;
        assert!(
            restart_guard_verdict(&inputs)
                .expect_err("overlap refuses")
                .contains("product-overlap")
        );
        inputs.product_overlap = false;
        inputs.fixture_exe_match = false;
        assert!(
            restart_guard_verdict(&inputs)
                .expect_err("fixture drift refuses")
                .contains("fixture-identity")
        );
        inputs.fixture_exe_match = true;
        inputs.fixture_join_same = false;
        inputs.fixture_join_note = Some("join read refused".to_owned());
        let error = restart_guard_verdict(&inputs).expect_err("join drift refuses");
        assert!(error.contains("fixture-join"), "{error}");
        inputs.fixture_join_same = true;
        inputs.current_owned_space = false;
        assert!(
            restart_guard_verdict(&inputs)
                .expect_err("space drift refuses")
                .contains("owned-space")
        );
        inputs.current_owned_space = true;
        inputs.topology_row_exact = false;
        assert!(
            restart_guard_verdict(&inputs)
                .expect_err("row drift refuses")
                .contains("owned-space")
        );
        inputs.topology_row_exact = true;
        inputs.windows_on_owned_space = false;
        assert!(
            restart_guard_verdict(&inputs)
                .expect_err("placement drift refuses")
                .contains("window-placement")
        );
        inputs.windows_on_owned_space = true;
        inputs.store_observed.files[0].bytes = b"roW\n".to_vec();
        let error = restart_guard_verdict(&inputs).expect_err("store drift refuses");
        assert!(error.contains("store"), "{error}");
    }

    #[test]
    fn guard_orders_the_first_failure() {
        let mut inputs = guard();
        inputs.product_overlap = true;
        inputs.prior_terminal_clean = false;
        inputs.store_observed.dir.as_mut().expect("dir").mtime_ns = 99;
        let error = restart_guard_verdict(&inputs).expect_err("first stage refuses");
        assert!(error.contains("product-overlap"), "{error}");
        inputs.product_overlap = false;
        let error = restart_guard_verdict(&inputs).expect_err("terminal stage refuses");
        assert!(error.contains("prior-terminal"), "{error}");
        inputs.prior_terminal_clean = true;
        let error = restart_guard_verdict(&inputs).expect_err("store stage refuses");
        assert!(error.contains("store"), "{error}");
        inputs.launch_ids_distinct = false;
        let error = restart_guard_verdict(&inputs).expect_err("identity first");
        assert!(error.contains("launch-identity"), "{error}");
    }

    #[test]
    fn terminal_evidence_requires_a_clean_stop() {
        assert!(
            clean_terminal_verdict(&serde_json::json!({
                "outcome": "stopped", "shutdown_clean": true
            }))
            .is_ok()
        );
        assert!(
            clean_terminal_verdict(&serde_json::json!({
                "outcome": "completed", "shutdown_clean": true
            }))
            .is_ok()
        );
        for body in [
            serde_json::json!({"outcome": "stopped"}),
            serde_json::json!({"outcome": "stopped", "shutdown_clean": false}),
            serde_json::json!({"outcome": "refused", "shutdown_clean": true}),
            serde_json::json!({"shutdown_clean": true}),
            serde_json::json!({}),
        ] {
            assert!(clean_terminal_verdict(&body).is_err(), "{body}");
        }
    }

    #[test]
    fn runtime_started_evidence_is_exact() {
        assert!(runtime_started_seen(
            "tiler-log seq=1 t=2ms pid=0 gen=0 ref=0 kind=runtime outcome=started count=0"
        ));
        assert!(runtime_started_seen(
            "noise\ntiler-log seq=9 t=1ms pid=0 gen=0 ref=0 kind=runtime outcome=started count=0\nmore"
        ));
        for raw in [
            "",
            "kind=runtime outcome=starting",
            "kind=startup outcome=started",
            "kind=runtimeoutcome=started",
        ] {
            assert!(!runtime_started_seen(raw), "{raw:?}");
        }
    }

    #[test]
    fn arrangement_floats_exact_and_tiles_unordered() {
        let before = ArrangementEvidence {
            floats: vec![
                frames("w0", 10, 0.0, 0.0, 400.0, 300.0),
                frames("w1", 11, 500.0, 0.0, 400.0, 300.0),
            ],
            tiles: vec![
                frames("w2", 12, 0.0, 400.0, 300.0, 200.0),
                frames("w3", 13, 300.0, 400.0, 300.0, 200.0),
            ],
            tile_reflowed: true,
        };
        // Exact (identical order) passes.
        assert!(
            compare_restart_arrangement(
                &before,
                &[before.floats.clone(), before.tiles.clone()].concat()
            )
            .is_ok()
        );
        // A swapped tile slot (same multiset, different windows) passes.
        let swapped = vec![
            before.floats[0].clone(),
            before.floats[1].clone(),
            frames("w2", 12, 300.0, 400.0, 300.0, 200.0),
            frames("w3", 13, 0.0, 400.0, 300.0, 200.0),
        ];
        assert!(compare_restart_arrangement(&before, &swapped).is_ok());
        // A float frame change refuses even when tiles match.
        let mut moved_float = swapped.clone();
        moved_float[0].ax.x += 1.0;
        assert!(compare_restart_arrangement(&before, &moved_float).is_err());
        // A float CG-only change refuses (independent legs).
        let mut moved_cg = swapped.clone();
        moved_cg[1].cg.y += 1.0;
        assert!(compare_restart_arrangement(&before, &moved_cg).is_err());
        // A tile rectangle change refuses.
        let mut moved_tile = swapped.clone();
        moved_tile[2].ax.w += 1.0;
        assert!(compare_restart_arrangement(&before, &moved_tile).is_err());
        // Role swap refuses.
        let roles = vec![
            frames("w0", 10, 0.0, 0.0, 400.0, 300.0),
            frames("w1", 11, 500.0, 0.0, 400.0, 300.0),
            before.floats[0].clone(),
            before.tiles[0].clone(),
        ];
        assert!(compare_restart_arrangement(&before, &roles).is_err());
        // Duplicate counts matter: two identical tiles before cannot match
        // one identical tile plus a different one after.
        let duplicate = ArrangementEvidence {
            floats: before.floats.clone(),
            tiles: vec![
                frames("w2", 12, 0.0, 400.0, 300.0, 200.0),
                frames("w3", 13, 0.0, 400.0, 300.0, 200.0),
            ],
            tile_reflowed: true,
        };
        let one_duplicate = vec![
            before.floats[0].clone(),
            before.floats[1].clone(),
            frames("w2", 12, 0.0, 400.0, 300.0, 200.0),
            frames("w3", 13, 300.0, 400.0, 300.0, 200.0),
        ];
        assert!(compare_restart_arrangement(&duplicate, &one_duplicate).is_err());
        // Missing window refuses.
        assert!(compare_restart_arrangement(&before, &swapped[..3]).is_err());
    }

    #[test]
    fn maximize_restart_restores_the_exact_base_not_the_overlay() {
        let base = vec![
            frames("w0", 10, 0.0, 0.0, 600.0, 400.0),
            frames("w1", 11, 600.0, 0.0, 600.0, 400.0),
        ];
        let overlay = vec![
            frames("w0", 10, 0.0, 0.0, 1200.0, 800.0),
            frames("w1", 11, 600.0, 0.0, 600.0, 400.0),
        ];
        assert!(compare_restart_base(&base, &overlay, &base, "w0").is_ok());
        // Overlay surviving restart refuses.
        assert!(compare_restart_base(&base, &overlay, &overlay, "w0").is_err());
        // No distinct overlay refuses.
        assert!(compare_restart_base(&base, &base, &base, "w0").is_err());
        // A non-target window drifting refuses.
        let mut moved = base.clone();
        moved[1].cg.x += 5.0;
        assert!(compare_restart_base(&base, &overlay, &moved, "w0").is_err());
        // Only one independent read moving is not a proven overlay.
        let mut ax_only = base.clone();
        ax_only[0].ax.w = 1200.0;
        let mut cg_only = base.clone();
        cg_only[0].cg.w = 1200.0;
        assert!(compare_restart_base(&base, &ax_only, &base, "w0").is_err());
        assert!(compare_restart_base(&base, &cg_only, &base, "w0").is_err());
    }

    #[test]
    fn tile_reflow_requires_a_real_tiled_movement() {
        let initial = vec![
            frames("w0", 10, 0.0, 0.0, 400.0, 300.0),
            frames("w1", 11, 500.0, 0.0, 400.0, 300.0),
            frames("w2", 12, 0.0, 400.0, 300.0, 200.0),
            frames("w3", 13, 300.0, 400.0, 300.0, 200.0),
        ];
        let tiles = vec![
            frames("w2", 12, 0.0, 0.0, 600.0, 600.0),
            frames("w3", 13, 600.0, 0.0, 600.0, 600.0),
        ];
        assert!(require_tile_reflow(&initial, &tiles).expect("tile reflowed"));
        assert!(require_tile_reflow(&initial, &initial[2..]).is_err());
    }

    fn store_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("restart-store-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn write_store(dir: &Path, bytes: &[u8]) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        std::fs::create_dir_all(dir).expect("store dir creates");
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
            .expect("store dir mode");
        let path = dir.join("intent-v1-501-100.json");
        std::fs::write(&path, bytes).expect("store row writes");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
            .expect("store row mode");
        path
    }

    fn store_snapshot_with_rows(rows: &[(u32, u32, u64, Option<bool>, &str)]) -> StoreSnapshot {
        let entries: Vec<serde_json::Value> = rows
            .iter()
            .map(|(pid, wid, launch, float, bundle)| {
                serde_json::json!({
                    "win": format!("{pid}_{wid}"),
                    "wid": wid,
                    "pid": pid,
                    "launch_date_ns": launch,
                    "bundle_id": bundle,
                    "data": tiler_core::intent_data::encode_intent_data(
                        &tiler_core::intent_data::IntentData::new(*float, None)
                    ),
                })
            })
            .collect();
        let bytes = serde_json::to_vec(&serde_json::json!({
            "version": 1,
            "session_uid": 501,
            "session_asid": 100,
            "entries": entries,
        }))
        .expect("store envelope encodes");
        StoreSnapshot {
            present: true,
            dir: Some(StoreDirMeta {
                mode: 0o700,
                uid: 501,
                ino: 7,
                mtime_ns: 11,
            }),
            files: vec![StoreFile {
                name: "intent-v1-501-100.json".to_owned(),
                len: bytes.len() as u64,
                mode: 0o600,
                uid: 501,
                ino: 8,
                mtime_ns: 12,
                bytes,
            }],
        }
    }

    #[test]
    fn m3_float_rows_require_exact_binding_and_no_unexpected_float() {
        let binding = crate::intent_store::LiveBinding {
            launch_date_ns: 777,
            bundle_id: FIXTURE_SCOPE.to_owned(),
        };
        let floats = vec![("w0".to_owned(), 10), ("w1".to_owned(), 11)];
        let exact = store_snapshot_with_rows(&[
            (4242, 10, 777, Some(true), FIXTURE_SCOPE),
            (4242, 11, 777, Some(true), FIXTURE_SCOPE),
            (4242, 12, 777, Some(false), FIXTURE_SCOPE),
        ]);
        let evidence =
            require_m3_float_rows(&exact, &floats, 4242, &binding).expect("exact rows pass");
        assert_eq!(evidence["expected_floats"], 2);
        assert_eq!(evidence["observed_float_rows"], 2);
        // Missing one intended float refuses.
        let missing = store_snapshot_with_rows(&[
            (4242, 10, 777, Some(true), FIXTURE_SCOPE),
            (4242, 11, 777, Some(false), FIXTURE_SCOPE),
        ]);
        assert!(require_m3_float_rows(&missing, &floats, 4242, &binding).is_err());
        // An unexpected float row refuses even when both intended rows exist.
        let extra = store_snapshot_with_rows(&[
            (4242, 10, 777, Some(true), FIXTURE_SCOPE),
            (4242, 11, 777, Some(true), FIXTURE_SCOPE),
            (4242, 12, 777, Some(true), FIXTURE_SCOPE),
        ]);
        assert!(require_m3_float_rows(&extra, &floats, 4242, &binding).is_err());
        // A wrong lifetime, PID, WID or bundle is not an exact binding.
        for bad in [
            (4242, 10, 778, Some(true), FIXTURE_SCOPE),
            (4243, 10, 777, Some(true), FIXTURE_SCOPE),
            (4242, 10, 777, Some(true), "com.example.other"),
        ] {
            let drifted =
                store_snapshot_with_rows(&[bad, (4242, 11, 777, Some(true), FIXTURE_SCOPE)]);
            assert!(require_m3_float_rows(&drifted, &floats, 4242, &binding).is_err());
        }
        // Absent store and a store with no row both refuse for M3.
        assert!(require_m3_float_rows(&StoreSnapshot::absent(), &floats, 4242, &binding).is_err());
        let empty = store_snapshot_with_rows(&[]);
        assert!(require_m3_float_rows(&empty, &floats, 4242, &binding).is_err());
    }

    #[test]
    fn store_hydration_label_distinguishes_absence_from_rows() {
        assert_eq!(
            store_hydration_state(&StoreSnapshot::absent()),
            "unchanged-absence"
        );
        let empty_dir = StoreSnapshot {
            present: true,
            dir: None,
            files: Vec::new(),
        };
        assert_eq!(
            store_hydration_state(&empty_dir),
            "unchanged-directory-only"
        );
        let rows = store_snapshot_with_rows(&[(4242, 10, 777, Some(true), FIXTURE_SCOPE)]);
        assert_eq!(store_hydration_state(&rows), "unchanged-rows");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn overlap_guard_excludes_the_controller_and_dead_product_entries() {
        let self_pid = 100;
        let controller_only = vec![
            (self_pid, crate::BUNDLE_ID.to_owned()),
            (300, "com.example.other".to_owned()),
        ];
        assert!(!foreign_product_in_apps(
            &controller_only,
            self_pid,
            crate::BUNDLE_ID,
            |_| true
        ));
        let with_product = vec![
            (self_pid, crate::BUNDLE_ID.to_owned()),
            (200, crate::BUNDLE_ID.to_owned()),
            (300, "com.example.other".to_owned()),
        ];
        assert!(foreign_product_in_apps(
            &with_product,
            self_pid,
            crate::BUNDLE_ID,
            |pid| pid == 200
        ));
        assert!(!foreign_product_in_apps(
            &with_product,
            self_pid,
            crate::BUNDLE_ID,
            |_| false
        ));
    }

    #[test]
    fn store_fingerprint_refuses_byte_and_metadata_drift() {
        let dir = store_dir("drift");
        write_store(&dir, b"row-one\n");
        let first = read_store_snapshot(&dir).expect("fingerprint reads");
        assert!(first.has_row());
        let again = read_store_snapshot(&dir).expect("fingerprint rereads");
        assert_eq!(first, again, "an unread store is unchanged");
        // Exact bytes, changed metadata (mode) still refuses.
        use std::os::unix::fs::PermissionsExt;
        let path = dir.join("intent-v1-501-100.json");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640))
            .expect("mode change");
        let mode_drift = read_store_snapshot(&dir).expect("fingerprint reads");
        assert_ne!(first, mode_drift);
        assert!(first.evidence()["digest"] != mode_drift.evidence()["digest"]);
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
            .expect("mode restore");
        // Same length, changed bytes refuses.
        std::fs::write(&path, b"row-two\n").expect("byte change");
        let byte_drift = read_store_snapshot(&dir).expect("fingerprint reads");
        assert_ne!(first, byte_drift);
        // Absent is its own fingerprint and needs no row.
        let absent = read_store_snapshot(&dir.join("missing")).expect("absent fingerprint");
        assert!(!absent.has_row());
        assert!(!absent.evidence()["present"].as_bool().expect("bool"));
        // A symlinked store directory refuses.
        #[cfg(unix)]
        std::os::unix::fs::symlink(&dir, dir.with_extension("link")).expect("symlink");
        assert!(read_store_snapshot(&dir.with_extension("link")).is_err());
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_file(dir.with_extension("link"));
    }

    #[test]
    fn program_ordering_keeps_the_driver_before_the_first_stop_and_the_guard_before_launch_two() {
        let source = include_str!("fixture_restart.rs");
        let pre = source
            .find("fn run_pre_stop_phase")
            .expect("pre-stop phase");
        let guard_doc = source.find("/// Read every guard fact").expect("guard doc");
        let post = source
            .find("fn run_post_restart_phase")
            .expect("post phase");
        let body_doc = source.find("/// One two-launch body").expect("body doc");
        let body = source
            .find("fn run_restart_body_macos")
            .expect("restart body");
        let wrapper = source.find("/// Handler wrapper").expect("wrapper");
        assert!(pre < guard_doc && guard_doc < post && post < body_doc);
        assert!(body_doc < body && body < wrapper);

        let pre_body = &source[pre..guard_doc];
        assert!(pre_body.contains("FixtureDriver::bind"));
        assert!(pre_body.contains("driver.post("));
        assert!(!pre_body.contains("stop_product_for_restart"));
        assert!(!pre_body.contains("launch_product_ls"));

        let body_body = &source[body..wrapper];
        let pre_call = body_body
            .find("run_pre_stop_phase(")
            .expect("pre-stop call");
        let stop_one = body_body
            .find("leg.stop_product_for_restart()")
            .expect("first stop");
        let post_call = body_body
            .find("run_post_restart_phase(")
            .expect("post call");
        assert!(pre_call < stop_one, "chords precede the first stop");
        assert!(stop_one < post_call, "first stop precedes launch two");

        let overlap = body_body
            .find("foreign_product_overlap(leg, parsed.pid)")
            .expect("pre-launch overlap scan");
        let pending_product = body_body
            .find("what: \"product\"")
            .expect("first product pending registration");
        let launch_one = body_body
            .find("launch_product_ls(")
            .expect("first product launch");
        assert!(
            overlap < pending_product && pending_product < launch_one,
            "the overlap scan precedes pending registration and the first launch"
        );

        let post_body = &source[post..body_doc];
        let initial_guard = post_body
            .find("let initial_guard = observe_restart_guard(")
            .expect("initial guard observation");
        let initial_verdict = post_body
            .find("restart_guard_verdict(&initial_guard)")
            .expect("initial guard verdict");
        let rows = post_body
            .find("require_m3_float_rows(")
            .expect("persisted float rows");
        let nudge = post_body
            .find("let nudge = if opts.mode == RestartMode::M3 {")
            .expect("post-stop nudge");
        let nudge_call = post_body
            .find("nudge_one_known_tile(")
            .expect("post-stop nudge call");
        let fresh_guard = post_body
            .rfind("observe_restart_guard(")
            .expect("fresh guard observation");
        let verdict = post_body
            .find("restart_guard_verdict(&guard)")
            .expect("guard verdict");
        let launch_two = post_body.find("launch_product_ls(").expect("second launch");
        let hydrate = post_body
            .find("let after = await_settled_frames(fixture.pid, joined)?;")
            .expect("hydration capture");
        let capture_bound = post_body
            .find("require_post_restart_capture_bound(")
            .expect("post-restart capture binding");
        let compare = post_body
            .find("compare_restart_arrangement(arrangement, &after)")
            .expect("M3 comparison");
        let compare_base = post_body
            .find("compare_restart_base(base, overlay, &after, target)")
            .expect("M4 comparison");
        assert!(
            initial_guard < initial_verdict,
            "the initial guard is observed before its verdict"
        );
        assert!(
            initial_verdict < rows,
            "the initial guard precedes the persisted float-row proof"
        );
        assert!(
            rows < nudge && nudge <= nudge_call && nudge_call < fresh_guard,
            "the persisted rows and initial guard precede the one nudge, which precedes the fresh guard"
        );
        assert!(
            fresh_guard < verdict,
            "the fresh guard is observed before its verdict"
        );
        assert!(verdict < launch_two, "verdict precedes the second launch");
        assert!(
            post_body.contains("let float_rows = if opts.mode == RestartMode::M3 {"),
            "the persisted float rows are an M3-only proof"
        );
        assert!(
            post_body.contains("let nudge = if opts.mode == RestartMode::M3 {"),
            "M4 takes the no-nudge branch"
        );
        let nudge_fn = source
            .find("fn nudge_one_known_tile(")
            .expect("nudge function");
        let nudge_fn_end = source[nudge_fn..]
            .find("/// Re-verify every identity")
            .expect("nudge function end");
        assert!(
            !source[nudge_fn..nudge_fn + nudge_fn_end].contains("RestartMode::M4"),
            "M4 never nudges the overlay"
        );
        assert!(
            launch_two < hydrate,
            "second launch precedes hydration capture"
        );
        assert!(
            hydrate < capture_bound,
            "capture precedes its identity/placement re-verification"
        );
        assert!(
            hydrate < compare && hydrate < compare_base,
            "capture precedes comparison"
        );
        assert!(
            !post_body.contains("FixtureDriver")
                && !post_body.contains(".post(")
                && !post_body.contains(".focus("),
            "the post-restart phase cannot bind a driver or post a chord"
        );
    }
}

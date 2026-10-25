#![forbid(unsafe_code)]

//! Tile development entry point (`tile --user-start ...`).
//!
//! Two product modes share one controller:
//!
//! - Scoped (bounded): an explicit non-empty bundle allowlist. Ghostty (any
//!   case/variant) never validates and never actuates in scoped mode. The
//!   signed bundled helper admits under an explicit helper-bundle scope
//!   (fixture exception).
//! - Unscoped (user dogfood): every ordinary app on each display's current
//!   user Space, minus both own bundles and optional exact bundle-ID
//!   exclusions. Ghostty is an ordinary product candidate here. An
//!   unscoped run with `--seconds 0` is unbounded (runs until the owner
//!   exits or a stop is published to its run directory); scoped runs stay
//!   finite with a hard watchdog.
//!
//! An explicitly started tile session over everyday apps (e.g.
//! TextEdit/Calculator class): the main (AppKit) thread
//! owns topology snapshots and lifecycle discovery, one dedicated input
//! thread owns its tap guard, one per-app AX owner observes and
//! actuates under pre-effect fences, and one engine thread plans. No
//! additive core API: input resolves to the existing `Focus`/`Move`/
//! `Resize`/`ToggleFloat`/`ToggleOrientation` commands, effects ride the
//! existing scoped backend.
//!
//! The same main thread owns one active-border carrier
//! ([`crate::ffi::BorderCarrier`]) driven by [`crate::border_runtime`]:
//! each per-app AX owner publishes its focused-window facts from already-read
//! snapshots, and the main loop joins the frontmost PID's record with the
//! live session/scope/Space/revision verdict plus the system accent. The
//! controller updates the panel only on observed frame/target/style change
//! and hides on every false/unknown leg (including lock and Space drift);
//! no app activation or policy flip happens on this path, and the carrier
//! never joins tiling (own-bundle exclusion). Scoped runs decorate only
//! in-scope frontmost apps (unrelated windows never decorate); unscoped
//! runs decorate ordinary apps through the same product gate. Border
//! configuration is startup-only (`--active-border-*`/settings file).
//!
//! Safety contract: explicit `--user-start` plus a non-empty bundle
//! allowlist are mandatory. Ghostty (any case/variant), the own bundle,
//! protected terminals, and unknown (`None`) bundles never write - at
//! validation AND at the effect boundary. A directly spawned probe-target
//! helper carries no LaunchServices bundle entry and no launch date, so a
//! named scope cannot admit it; the signed bundled helper (stable helper
//! bundle ID with a LaunchServices entry plus launch date) is admittable
//! under an explicit helper-bundle scope. Stop
//! and process loss leave windows in place; nothing restores geometry.
//!
//! The main thread never blocks on workers: discovery launches async,
//! shutdown runs under one global deadline independent of blocked AX, and
//! a cancellable watchdog enforces the hard lifetime (soft deadline plus
//! bounded teardown plus finite grace) with a dirty forced exit rather than
//! a hang. The main thread performs no AX reads (workspace-frontmost only):
//! the AX focused leg is read boundedly on each owning per-app AX thread
//! and stamped at send time, so a hung app can never stall the controller.
//! The process may exit unclean with diagnostics rather than hang.

#[cfg(target_os = "macos")]
use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::active_border::ActiveBorderOptions;
use crate::config::{MAX_PROBE_SECONDS, ProbeModifier};
use crate::dev_selection::{TileScope, launchable as selection_launchable};
use crate::input_policy::InputPreset;
use crate::scope::AppScope;

/// Default tile lifetime (seconds): finite and well under the probe max.
pub const DEFAULT_TILE_SECONDS: u64 = 30;
/// Tile lifetime bound (seconds): the configured probe max.
pub const MAX_TILE_SECONDS: u64 = MAX_PROBE_SECONDS;

/// Which input route the tile run arms. The event tap is the sole product
/// route with exact-chord pair suppression; off parks.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TileInput {
    /// Continuous event tap with exact-chord pair suppression (default).
    #[default]
    Tap,
    /// No interception: open-loop on AX observations.
    Off,
}

impl TileInput {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s.to_ascii_lowercase().as_str() {
            "tap" => Ok(Self::Tap),
            "off" => Ok(Self::Off),
            other => Err(format!("unknown --input '{other}' (expected tap|off)")),
        }
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Tap => "tap",
            Self::Off => "off",
        }
    }
}

/// Foreground log verbosity for a tile run: the structured stderr log lines
/// are the existing runtime events (static tokens plus counters only, never
/// user content). Normal emits lifecycle/recovery lines only; verbose adds
/// a periodic counter heartbeat; trace shortens the heartbeat cadence and
/// adds input forward/ignore counters.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TileLogLevel {
    /// Lifecycle and recovery lines only (default).
    #[default]
    Normal,
    /// Plus a periodic counter heartbeat (~10 s).
    Verbose,
    /// Faster heartbeat (~2 s) plus input forward/ignore counters.
    Trace,
}

impl TileLogLevel {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s.to_ascii_lowercase().as_str() {
            "normal" => Ok(Self::Normal),
            "verbose" => Ok(Self::Verbose),
            "trace" => Ok(Self::Trace),
            other => Err(format!(
                "unknown --log '{other}' (expected normal|verbose|trace)"
            )),
        }
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Verbose => "verbose",
            Self::Trace => "trace",
        }
    }
}

/// Validated tile request: explicit user start plus a product selection
/// (scoped non-empty allowlist, or unscoped ordinary apps) plus a lifetime.
/// Construction never touches the OS. Unscoped `--seconds 0` means
/// unbounded (owner exit or exact-PID stop ends the run); scoped runs are
/// always bounded.
#[derive(Debug, Clone, PartialEq)]
pub struct TileRequest {
    pub user_start: bool,
    pub scopes: Vec<String>,
    pub seconds: u64,
    pub input: TileInput,
    pub modifier: ProbeModifier,
    pub preset: InputPreset,
    /// Unscoped product mode (ordinary apps minus own bundles/exclusions).
    /// Without it the run is scoped (explicit allowlist, Ghostty refused).
    pub unscoped: bool,
    /// Diagnostic-only stay injection (`--diag-stay-index 1|2`): exactly one
    /// numbered stay chord (Desktop 1 or 2) appended to the effective tap
    /// table, so the normal tap/admit/drain path proves the armed stay.
    /// Gated in [`TileRequest::validate`]: explicit `--user-start` plus
    /// `--input tap` plus exactly `--scope com.apple.TextEdit` (never
    /// unscoped), finite scoped lifetime, and a duplicate-free effective
    /// chord. No product/default binding changes.
    pub diag_stay: Option<u8>,
    /// Diagnostic-only trailing-slot click injection
    /// (`--diag-trailing-click`): once per run, copy the ACTUAL rendered
    /// trailing binding off the live AppKit targets (never reconstructed,
    /// never a direct select-0 substitute) and loop it back through the
    /// same click channel an AppKit press would use, so the normal
    /// kind-routed drain plus engine admission plus fresh fences prove
    /// the trailing select-or-create. Gated in [`TileRequest::validate`]:
    /// explicit `--user-start` plus `--input tap` plus exactly
    /// `--scope com.apple.TextEdit` (never unscoped) plus finite scoped
    /// lifetime. No product/default binding changes.
    pub diag_trailing_click: bool,
    /// Exact bundle-ID exclusions (unscoped only).
    pub excluded: Vec<String>,
    /// Foreground log verbosity.
    pub log: TileLogLevel,
    /// Exact-run directory for graceful lifecycle (created by the
    /// foreground launcher and passed here): the owner polls
    /// `<dir>/stop-requested` every cadence and publishes its terminal
    /// outcome JSON to `<dir>/outcome.json`. `None` keeps the plain
    /// bounded behavior with stdout as the only outcome channel.
    pub run_dir: Option<String>,
    /// Startup border configuration (master switch plus ring style).
    /// Startup-only: a running session never re-reads it.
    pub border: ActiveBorderOptions,
    /// Startup drag-preview configuration (master switch plus fill).
    /// Startup-only: a running session never re-reads it.
    pub drag: crate::drag_drop::DragOptions,
    /// Startup workspace-indicator configuration (display mode plus
    /// representative icons). Startup-only: a running session never
    /// re-reads it.
    pub indicator: crate::workspace_indicator::IndicatorOptions,
}

impl TileRequest {
    pub fn validate(&self) -> Result<(), String> {
        if !self.user_start {
            return Err("tile requires explicit --user-start".to_owned());
        }
        if self.unscoped {
            if !self.scopes.is_empty() {
                return Err(
                    "tile: --scope and --unscoped are mutually exclusive (drop --unscoped for the allowlist mode)"
                        .to_owned(),
                );
            }
            if self.seconds > MAX_TILE_SECONDS {
                return Err(format!(
                    "seconds must be 0..={MAX_TILE_SECONDS} (0 = unbounded), got {}",
                    self.seconds
                ));
            }
        } else {
            if self.scopes.is_empty() {
                return Err("tile requires at least one --scope BUNDLE".to_owned());
            }
            if self.seconds == 0 || self.seconds > MAX_TILE_SECONDS {
                return Err(format!(
                    "seconds must be 1..={MAX_TILE_SECONDS}, got {}",
                    self.seconds
                ));
            }
            if !self.excluded.is_empty() {
                return Err("tile: --exclude needs --unscoped".to_owned());
            }
        }
        if let Some(dir) = &self.run_dir
            && dir.is_empty()
        {
            return Err("tile: --run-dir must not be empty".to_owned());
        }
        self.selection().validate()?;
        if let Some(index) = self.diag_stay {
            // Diagnostic-only stay injection: exactly one numbered stay
            // (Desktop 1 or 2) over the normal tap/admit/drain path. Narrow
            // gates: the tap route, a scoped TextEdit-only run (never
            // unscoped, so no ordinary-app input authority), and the finite
            // scoped lifetime above. The chord is the fixed Shift+Alt+digit
            // shape, refused here when it is not duplicate-free in the run's
            // own effective catalogue under its own preset/modifier.
            if index != 1 && index != 2 {
                return Err("tile: --diag-stay-index must be 1 or 2".to_owned());
            }
            if self.input != TileInput::Tap {
                return Err("tile: --diag-stay-index needs --input tap".to_owned());
            }
            if self.unscoped {
                return Err(
                    "tile: --diag-stay-index refuses --unscoped (TextEdit-only scope)".to_owned(),
                );
            }
            // TextEdit-only scope, spelled literally so this product file
            // never names the diagnostic route module (the isolation test
            // pins that): must equal the disposable-post bundle constant.
            if self.scopes != ["com.apple.TextEdit"] {
                return Err(
                    "tile: --diag-stay-index needs exactly --scope com.apple.TextEdit".to_owned(),
                );
            }
            let binding =
                diag_stay_binding(index).ok_or("tile: --diag-stay-index has no digit leg")?;
            let catalogue = crate::input_policy::effective_bindings(
                self.preset,
                self.modifier,
                &product_catalogue(),
            );
            let want = crate::workspace_input::effective_flags_for(self.modifier, &binding);
            if catalogue.iter().any(|existing| {
                existing.key_code == binding.key_code
                    && crate::workspace_input::effective_flags_for(self.modifier, existing) == want
            }) {
                return Err(
                    "tile: --diag-stay-index chord collides with the effective catalogue"
                        .to_owned(),
                );
            }
        }
        if self.diag_trailing_click {
            // Diagnostic-only trailing-click injection: the same narrow
            // gates as the stay injection (tap route, scoped
            // TextEdit-only run, finite scoped lifetime above) minus the
            // chord-collision leg (no tap chord is involved: the drain
            // consumes the actual rendered trailing binding through its
            // own kind/generation/revision fences).
            if self.input != TileInput::Tap {
                return Err("tile: --diag-trailing-click needs --input tap".to_owned());
            }
            if self.unscoped {
                return Err(
                    "tile: --diag-trailing-click refuses --unscoped (TextEdit-only scope)"
                        .to_owned(),
                );
            }
            // TextEdit-only scope, spelled literally so this product file
            // never names the diagnostic route module (the isolation test
            // pins that): must equal the disposable-post bundle constant.
            if self.scopes != ["com.apple.TextEdit"] {
                return Err(
                    "tile: --diag-trailing-click needs exactly --scope com.apple.TextEdit"
                        .to_owned(),
                );
            }
        }
        // Defensive: a preset must never arm an empty chord set (Compatible
        // only removes known-conflicting arrows/digits, so this is
        // unreachable today, but an empty tap would silently intercept
        // nothing).
        if crate::input_policy::effective_bindings(self.preset, self.modifier, &product_catalogue())
            .is_empty()
        {
            return Err("preset disables every chord; refusing an empty tap".to_owned());
        }
        Ok(())
    }

    #[must_use]
    pub fn selection(&self) -> TileScope {
        if self.unscoped {
            TileScope::Unscoped {
                excluded: self.excluded.clone(),
            }
        } else {
            TileScope::Scoped(self.scope())
        }
    }

    /// True for an unbounded product run (unscoped `--seconds 0`): no
    /// deadline and no hard watchdog; owner exit or exact-PID stop ends it.
    #[must_use]
    pub const fn is_unbounded(&self) -> bool {
        self.unscoped && self.seconds == 0
    }

    #[must_use]
    pub fn scope(&self) -> AppScope {
        AppScope {
            allowed: self.scopes.clone(),
        }
    }
}

/// Product chord catalogue: the basic tiling chords plus the armed
/// select subset (19 rows: numbered existing-Space `1..9`, the trailing
/// select-0, the 8 relative
/// selects, the previous toggle) plus the armed numbered send-follow
/// subset (10 rows: `workspace-send-0..9` as `Ctrl+Shift+digit` under the
/// default Ctrl role). Stays, relative sends, outputs, and the trailing
/// stay-0 never enter the tap table (the engine refuses them as
/// the backstop even if configured); the diagnostic `--diag-stay-index`
/// route appends exactly one stay chord explicitly (see
/// [`diag_stay_binding`]), never by default.
#[must_use]
pub fn product_catalogue() -> Vec<crate::runtime_types::TileBinding> {
    let mut catalogue = crate::native_input::tile_catalogue();
    catalogue.extend(crate::workspace_select::bindings());
    catalogue.extend(crate::workspace_send::bindings());
    catalogue
}

/// Diagnostic-only stay binding for the `--diag-stay-index` harness: the
/// Desktop-N digit keycode with the Shift+Alt arms carrying the stay
/// (`follow: false`) action for Desktop 1 or 2. The Shift+Alt+digit shape
/// is duplicate-free in the effective product catalogue (digits only carry
/// the plain select and Shifted send shapes), so the tap consumes it
/// exactly like a product chord and the engine admits it through the normal
/// tap/admit/drain/owner/settle path. `None` for any other index. Pure;
/// unit-tested.
#[must_use]
pub fn diag_stay_binding(index: u8) -> Option<crate::runtime_types::TileBinding> {
    if index != 1 && index != 2 {
        return None;
    }
    crate::desktop_shortcuts::desktop_digit_keycode(u32::from(index)).map(|key_code| {
        crate::runtime_types::TileBinding {
            key_code,
            shift: true,
            alt: true,
            aux: false,
            action: crate::runtime_types::InputSemanticAction::WorkspaceSend {
                index,
                follow: false,
            },
        }
    })
}

/// Whether a discovered (pid, bundle) may be launched under a scoped
/// allowlist: exact membership at the effect gate (Ghostty, own bundle,
/// protected terminals, and unknown bundles never pass), never the process
/// itself. Unscoped discovery uses
/// [`crate::dev_selection::launchable`] instead (same PID/bundle hygiene,
/// ordinary apps including Ghostty admitted).
#[must_use]
pub fn launchable(scope: &AppScope, pid: u32, bundle: &str) -> bool {
    selection_launchable(&TileScope::Scoped(scope.clone()), pid, bundle)
}

/// Terminal JSON outcome: honest counters and outcome vs live evidence.
/// Geometry is never restored (stop leaves windows in place); a refused or
/// dirty run reports so, never a bare claim.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TileOutcome {
    pub outcome: String,
    pub seconds: u64,
    /// Product mode: `scoped` (explicit allowlist, Ghostty refused) or
    /// `unscoped` (ordinary apps, Ghostty an ordinary candidate).
    pub mode: String,
    /// True for an unbounded product run (no deadline, no hard watchdog).
    pub unbounded: bool,
    pub input: TileInput,
    pub modifier: String,
    pub preset: String,
    /// Armed chord count under the selected preset (Compatible < catalogue).
    pub active_chords: usize,
    pub scopes: Vec<String>,
    /// Exact bundle-ID exclusions (unscoped only).
    pub excluded: Vec<String>,
    pub launched: u64,
    pub launch_refused: u64,
    pub terminated: u64,
    pub workers_ready: u64,
    pub workers_failed: u64,
    pub topology_updates: u64,
    pub topology_failures: u64,
    pub display_events: u64,
    pub lock_stops: u64,
    pub secure_active_seen: u64,
    pub gated_ms: u64,
    pub input_refusal: Option<String>,
    /// Latest continuous-tap counters (`None` for the off route): timeout
    /// re-enables, user-input notifications, and enabled-readback-only
    /// recoveries are first-class terminal evidence (cumulative across
    /// fresh-guard recreations).
    pub tap_report: Option<crate::ffi::TapSourceReport>,
    /// Drag-preview arm terminal state (`None` = the arm ran; `Some` =
    /// parked with its reason: disabled switch, off route, spawn denial,
    /// or tap-thread refusal).
    pub drag_parked: Option<String>,
    /// Latest mouse-tap counters for the drag arm (`None` when parked).
    pub drag_report: Option<crate::ffi::TapSourceReport>,
    /// Engine preview queries resolved on the drag arm.
    pub drag_previews: u64,
    /// Engine drop submits accepted on the drag arm.
    pub drag_drops: u64,
    /// Drag gestures cancelled dead (Escape, stale fence, failed submit).
    pub drag_cancels: u64,
    pub shutdown_outcome: String,
    pub shutdown_joined: usize,
    pub shutdown_retained: usize,
    /// True only when the run reached a verified terminal state with a
    /// clean bounded shutdown (no refusal, no retained workers). The
    /// foreground launcher parses this alongside `outcome`: stderr log
    /// lines never decide success.
    pub shutdown_clean: bool,
    pub input_forwarded: u64,
    pub input_ignored: u64,
    pub input_refused_no_focus: u64,
    /// Armed select-only actions handed to the main-thread drain.
    pub input_select_enqueued: u64,
    /// Armed select-only actions refused before enqueue (no topology yet,
    /// or a select already claimed/pending).
    pub input_select_refused: u64,
    /// Indicator segment/menu rebuilds applied to the native status item
    /// (change-only; 0 when the indicator is off or hidden).
    pub indicator_updates: u64,
    /// Indicator segment clicks submitted through the guarded engine lane
    /// (accepted submissions; refusals log with static reasons).
    pub indicator_clicks: u64,
    pub observations_published: u64,
    pub observation_failures: u64,
    /// Pushed border carrier effects (main-thread controller): creates,
    /// move/restyle updates, hides, and carrier-call failures.
    pub border_shows: u64,
    pub border_updates: u64,
    pub border_hides: u64,
    pub border_errors: u64,
    /// Maximum observed-frame→panel-application lag in milliseconds,
    /// sampled on pushed border shows/updates only (0 when none pushed).
    pub border_apply_lag_max_ms: u64,
    /// Pushed border show/update count backing the maximum above.
    pub border_apply_samples: u64,
    pub note: String,
}

impl TileOutcome {
    #[cfg(any(target_os = "macos", test))]
    fn refused(request: &TileRequest, reason: String) -> Self {
        Self {
            outcome: "refused".to_owned(),
            seconds: request.seconds,
            mode: mode_str(request.unscoped).to_owned(),
            unbounded: request.is_unbounded(),
            input: request.input,
            modifier: request.modifier.as_str().to_owned(),
            preset: request.preset.as_str().to_owned(),
            active_chords: 0,
            scopes: request.scopes.clone(),
            excluded: request.excluded.clone(),
            launched: 0,
            launch_refused: 0,
            terminated: 0,
            workers_ready: 0,
            workers_failed: 0,
            topology_updates: 0,
            topology_failures: 0,
            display_events: 0,
            lock_stops: 0,
            secure_active_seen: 0,
            gated_ms: 0,
            input_refusal: Some(reason),
            tap_report: None,
            drag_parked: Some("not-started".to_owned()),
            drag_report: None,
            drag_previews: 0,
            drag_drops: 0,
            drag_cancels: 0,
            shutdown_outcome: "not-started".to_owned(),
            shutdown_joined: 0,
            shutdown_retained: 0,
            shutdown_clean: false,
            input_forwarded: 0,
            input_ignored: 0,
            input_refused_no_focus: 0,
            input_select_enqueued: 0,
            input_select_refused: 0,
            indicator_updates: 0,
            indicator_clicks: 0,
            observations_published: 0,
            observation_failures: 0,
            border_shows: 0,
            border_updates: 0,
            border_hides: 0,
            border_errors: 0,
            border_apply_lag_max_ms: 0,
            border_apply_samples: 0,
            note: "no effects ran; no restoration performed".to_owned(),
        }
    }
}

/// Heartbeat stride in main-loop cadences (250 ms each): verbose every
/// ~10 s, trace every ~2 s. `None` disables the heartbeat (normal).
#[must_use]
pub const fn heartbeat_stride(log: TileLogLevel) -> Option<u64> {
    match log {
        TileLogLevel::Normal => None,
        TileLogLevel::Verbose => Some(40),
        TileLogLevel::Trace => Some(8),
    }
}

/// Change-only heartbeat snapshot: the counters one heartbeat line prints.
/// The main loop prints a heartbeat at most every stride AND only when this
/// snapshot differs from the last printed one, so an idle run goes quiet
/// instead of reprinting identical counters forever (bounded-diagnostic
/// parity: empty windows emit nothing). `input` carries the
/// forward/ignore pair on trace, `None` on verbose (which prints neither).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeartbeatState {
    pub launched: u64,
    pub terminated: u64,
    pub topo_updates: u64,
    pub topo_failures: u64,
    pub lock_stops: u64,
    pub secure_seen: u64,
    pub input: Option<(u64, u64)>,
}

/// Trailing-cleanup gate after the cooperative select drain: only a clean
/// drained/reaped selector (`select_unclean` none) may run MC removal
/// effects. An unclean drain means the owner is still live - e.g. a hung
/// MC-create owner that may still actuate - so every owned SID is retained
/// as explicit residue and ALL cleanup effects are skipped; the hung owner
/// can never overlap the main-thread remover. Returns the retained-residue
/// lines on the unclean arm (one per owned SID), or `None` on the clean arm
/// (the caller runs the preexisting cleanup loop). Pure and portable so
/// the decision path itself is unit-tested with real lifecycle ownership.
fn trailing_cleanup_decision(select_unclean: Option<&str>, owned: &[u64]) -> Option<Vec<String>> {
    select_unclean.map(|_| {
        owned
            .iter()
            .map(|sid| {
                format!(
                    "tiler-macos dev trailing residue sid={sid} reason=select drain unclean; retained without removal complete=true"
                )
            })
            .collect()
    })
}

/// Product-mode token for terminal JSON (`scoped` vs `unscoped`).
#[must_use]
pub const fn mode_str(unscoped: bool) -> &'static str {
    if unscoped { "unscoped" } else { "scoped" }
}

/// Best-effort terminal outcome persistence for exact-run lifecycle:
/// writes the terminal JSON body to `<run_dir>/outcome.json` when a run
/// directory was passed. Std-only, never fails the run (stdout stays the
/// primary outcome channel; a missing/unwritable directory simply skips).
/// The write is atomic (temporary file plus rename) so an observer that
/// sees the path always reads the complete body.
pub fn write_run_outcome(run_dir: &Option<String>, body: &str) {
    let Some(dir) = run_dir else { return };
    if dir.is_empty() {
        return;
    }
    let dir = std::path::Path::new(dir);
    let tmp = dir.join(".outcome.json.tmp");
    if std::fs::write(&tmp, body).is_err() {
        return;
    }
    let _ = std::fs::rename(&tmp, dir.join("outcome.json"));
}

/// Portable entry: validates, then runs the native controller (macOS-only).
/// Exit codes: 2 usage/validation, 1 runtime refusal or failure.
/// Terminal JSON bodies (both success and refusal) persist to the run
/// directory's `outcome.json` when `--run-dir` was passed, so an exact-run
/// launcher observes completion without process attribution.
pub fn run_tile(request: TileRequest) -> Result<String, (i32, String)> {
    request.validate().map_err(|reason| (2, reason))?;
    let run_dir = request.run_dir.clone();
    #[cfg(target_os = "macos")]
    {
        let result = native::run(request);
        match &result {
            Ok(body) => write_run_outcome(&run_dir, body),
            Err((_, body)) => write_run_outcome(&run_dir, body),
        }
        result
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = request;
        let _ = run_dir;
        Err((1, "tile is macOS-only".to_owned()))
    }
}

#[cfg(target_os = "macos")]
pub(crate) use native::IndicatorGuard;

#[cfg(target_os = "macos")]
mod native {
    use super::*;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use crate::input_policy::effective_bindings;
    use crate::native_input::{OffTileFactory, TapTileFactory, interception_gate};
    use crate::native_tile_backend::ScopedAxFactory;
    use crate::runtime::{FocusEvidence, Runtime, RuntimeConfig, TopologyMode, WorkerReadiness};
    use crate::runtime_backend::InputSourceFactory;
    use crate::topology::{TopologyPoll, TopologySnapshot, build_snapshot};

    /// Main-thread cadence for discovery/topology/evidence/lock polling.
    const CADENCE: Duration = Duration::from_millis(250);
    /// Bounded shutdown independent of blocked AX (joins proven exits only;
    /// the rest retain for reap instead of hanging the exit).
    const SHUTDOWN_BOUND: Duration = Duration::from_secs(5);
    /// Finite grace past the soft deadline plus bounded teardown before the
    /// watchdog forces the exit. The main loop is soft (a stalled main
    /// thread would otherwise overrun the tile lifetime silently); the
    /// watchdog is the hard lifetime.
    const WATCHDOG_GRACE: Duration = Duration::from_secs(30);
    /// Modest grace past the bounded shutdown for the stop-file deadline
    /// watcher on unbounded runs: after a stop is observed the controller
    /// gets `SHUTDOWN_BOUND` plus this grace, then a dirty forced exit if
    /// wedged. No finite lifetime on a normal (unstopped) dogfood run.
    const STOP_WATCHDOG_GRACE: Duration = Duration::from_secs(10);

    /// Pure watchdog limit: finite startup plus soft run lifetime plus
    /// bounded teardown plus finite grace. Unit-tested; the live watchdog
    /// below sleeps to exactly this.
    fn watchdog_limit(
        startup: Duration,
        seconds: u64,
        teardown: Duration,
        grace: Duration,
    ) -> Duration {
        startup + Duration::from_secs(seconds) + teardown + grace
    }

    /// Pure stop-file deadline: bounded teardown plus modest grace. The
    /// live stop watcher below sleeps to exactly this; unit-tested, never
    /// effectful.
    fn stop_limit(teardown: Duration, grace: Duration) -> Duration {
        teardown + grace
    }

    use crate::watchdog::Watchdog;

    struct TrackedApp {
        launch_date: Option<String>,
    }

    /// One live topology snapshot from the main thread: NSScreen/CG work
    /// areas plus scale plus SLS rows plus the separate-Spaces mode. Reads
    /// only; a failed read fails closed (the stale context fences plans).
    fn snapshot_live() -> Result<TopologySnapshot, String> {
        let ns = crate::ffi::ns_display_snapshot().map_err(|e| format!("display snapshot: {e}"))?;
        let rows = crate::ffi::sls_display_rows().map_err(|e| format!("space rows: {e}"))?;
        let separate = crate::ffi::ns_separate_spaces();
        Ok(build_snapshot(&ns, &rows, separate))
    }

    /// One bounded main-thread pump for the cooperative select drain below:
    /// service application events plus the runloop without entering `run`
    /// (the same pump as the main loop above; no windows open here).
    fn pump_once(app: &objc2_app_kit::NSApplication) {
        objc2::rc::autoreleasepool(|_| {
            crate::ffi::drain_app_events(app);
            crate::ffi::pump_runloop_once(50);
        });
    }

    /// Adopt one terminal selection completion into lifecycle ownership on
    /// Adopt one trailing append proof into lifecycle ownership
    /// (success or failure - the Space persists from the verified append).
    /// BOTH outcomes adopt: the drain took the exact-append proof exactly
    /// once, so lifecycle ownership can never double-adopt. The caller logs
    /// and reports the outcome.
    fn adopt_select_completion(
        handle: &crate::runtime::RuntimeHandle,
        trailing_lifecycle: &mut crate::workspace_select::TrailingLifecycle,
        completion: &crate::select_owner::SelectCompletion,
    ) {
        let _ = handle;
        if let Some(proof) = completion.proof.as_ref() {
            if let Err(reason) = trailing_lifecycle.adopt_created(proof.created) {
                eprintln!(
                    "tiler-macos dev trailing adopt sid={} refused reason={} complete=true",
                    proof.created, reason,
                );
            }
            eprintln!(
                "tiler-macos dev trailing adopted sid={} complete=true",
                proof.created,
            );
        }
    }

    /// Report one terminal selection outcome under the claim's operation
    /// identity with the drain's typed setter and genuine reads. The
    /// verified Space reads ride the raw view-attempt row on success
    /// (drain-verified before/after Space ids plus the observation-time
    /// baseline); failures carry the drain's genuine pre/post reads too,
    /// so the core - never a string classifier - derives confirmation,
    /// refusal, ambiguity, or supersession. A creation attempt rides
    /// alongside whenever the drain attempted the desired create.
    /// Shared by the live poll and the cooperative shutdown drain; the
    /// caller owns its settle-site log line.
    fn report_select_completion(
        handle: &crate::runtime::RuntimeHandle,
        completion: &crate::select_owner::SelectCompletion,
    ) {
        let stage_creation =
            |handle: &crate::runtime::RuntimeHandle,
             creation: &crate::workspace_select::CreationReport| {
                handle.report_creation_attempt(
                    &creation.operation_id,
                    &creation.logical,
                    &creation.output.0,
                    creation.setter,
                    &creation.order_before,
                    &creation.order_after,
                    creation.created,
                    creation.attempt_ms,
                );
            };
        match completion.result.as_ref() {
            Ok(success) => {
                if let Some(operation_id) = completion.operation_id.as_deref() {
                    if let Some(creation) = success.creation.as_ref() {
                        stage_creation(handle, creation);
                    }
                    handle.report_select_attempt(
                        operation_id,
                        success.setter,
                        success.pre,
                        success.post,
                        success.effect.baseline,
                    );
                }
            }
            Err(failure) => {
                if let Some(operation_id) = completion.operation_id.as_deref() {
                    if let Some(creation) = failure.creation.as_ref() {
                        stage_creation(handle, creation);
                    }
                    handle.report_select_attempt(
                        operation_id,
                        failure.setter,
                        failure.pre,
                        failure.post,
                        failure.baseline,
                    );
                }
            }
        }
    }

    /// Settle one terminal selection completion at shutdown with
    /// shutdown-site lines (adopt plus outcome report shared with the live
    /// poll above).
    fn settle_select_at_shutdown(
        handle: &crate::runtime::RuntimeHandle,
        trailing_lifecycle: &mut crate::workspace_select::TrailingLifecycle,
        completion: crate::select_owner::SelectCompletion,
        reaped: bool,
    ) {
        adopt_select_completion(handle, trailing_lifecycle, &completion);
        match completion.result.as_ref() {
            Ok(success) => {
                eprintln!(
                    "tiler-macos dev select settled at shutdown changed={} caffeinate_reaped={} complete=true",
                    success.effect.changed, reaped,
                );
                report_select_completion(handle, &completion);
            }
            Err(failure) => {
                eprintln!(
                    "tiler-macos dev select refused at shutdown reason={} complete=false",
                    failure.reason,
                );
                report_select_completion(handle, &completion);
            }
        }
    }

    /// True when the launcher published a stop for this exact run: the
    /// `<run_dir>/stop-requested` file exists. Std-only polling (one stat
    /// per main-loop cadence); `None` run dirs never stop this way.
    fn stop_requested(run_dir: &Option<String>) -> bool {
        run_dir.as_ref().is_some_and(|dir| {
            !dir.is_empty() && std::path::Path::new(dir).join("stop-requested").exists()
        })
    }

    /// Owned production border carrier on the tile main thread: created on
    /// the first show, updated only on observed change, hidden on every
    /// false/unknown leg, destroyed here at run stop on every path (`Drop`
    /// runs on all returns past construction - the watchdog forced exit
    /// excepted, where the OS reaps the panel with the process). No app
    /// activation and no activation-policy call happen on this path; the
    /// fixture-accepted create/update/hide/show_above/destroy surface is
    /// the only AppKit touch.
    struct BorderCarrierGuard {
        carrier: Option<crate::ffi::BorderCarrier>,
        mtm: objc2::MainThreadMarker,
    }

    impl BorderCarrierGuard {
        fn new(mtm: objc2::MainThreadMarker) -> Self {
            Self { carrier: None, mtm }
        }

        /// Drive one controller effect: at most one panel call. Create and
        /// update failures forget the pushed state (the next tick retries)
        /// and count; transitions log one line each (static tokens plus
        /// geometry - never titles, bundles, or paths). `engine_placed`
        /// reports the tick's placement join on show/update lines (live
        /// placement-truth diagnosis); hide-only callers pass false.
        fn drive(
            &mut self,
            controller: &mut crate::border_runtime::BorderController,
            effect: crate::border_runtime::BorderEffect,
            accent: Option<crate::active_border::SystemAccent>,
            engine_placed: bool,
        ) {
            use crate::border_runtime::BorderEffect;
            match effect {
                BorderEffect::None => {}
                BorderEffect::Show {
                    target_wid,
                    reorder_above,
                    outer,
                    style,
                    color,
                } => {
                    let created = self.carrier.is_none();
                    let outcome = match self.carrier.as_ref() {
                        None => {
                            match crate::ffi::BorderCarrier::create(
                                self.mtm,
                                &style,
                                outer,
                                accent,
                                "OmniTiler active border",
                            ) {
                                Ok(carrier) => {
                                    carrier.show_above(self.mtm, target_wid as isize);
                                    self.carrier = Some(carrier);
                                    Ok(())
                                }
                                Err(reason) => Err(reason),
                            }
                        }
                        Some(carrier) => match carrier.update(self.mtm, &style, outer, accent) {
                            Ok(()) => {
                                if reorder_above {
                                    carrier.show_above(self.mtm, target_wid as isize);
                                }
                                Ok(())
                            }
                            Err(reason) => Err(reason),
                        },
                    };
                    match outcome {
                        Ok(()) => eprintln!(
                            "tiler-macos dev border {} outer={:.0},{:.0},{:.0},{:.0} color={} reorder={} lag={}ms placed={}",
                            if created { "show" } else { "update" },
                            outer.x,
                            outer.y,
                            outer.w,
                            outer.h,
                            crate::active_border::render_color(color),
                            reorder_above,
                            controller.last_apply_lag_ms(),
                            engine_placed,
                        ),
                        Err(reason) => {
                            controller.note_carrier_error();
                            eprintln!("tiler-macos dev border carrier-error reason={reason}");
                        }
                    }
                }
                BorderEffect::Hide => {
                    if let Some(carrier) = self.carrier.as_ref() {
                        carrier.hide(self.mtm);
                    }
                    eprintln!(
                        "tiler-macos dev border hide reason={}",
                        controller.hide_reason().unwrap_or("unknown")
                    );
                }
            }
        }
    }

    impl Drop for BorderCarrierGuard {
        fn drop(&mut self) {
            if let Some(carrier) = self.carrier.take() {
                carrier.destroy(self.mtm);
            }
        }
    }

    /// Owned menu-bar indicator state on the tile main thread: one native
    /// status item showing every Space as a directly clickable segment in
    /// the bar itself (plus a mirror menu), driven change-only from the
    /// pure [`IndicatorDriver`] pipeline, plus the click-binding channel
    /// the per-segment AppKit targets forward into. Created only when the
    /// mode is not off; the status item itself exists only while the
    /// projection is visible (single display, provable row). Occupancy
    /// comes from the engine's retained domains (managed windows only -
    /// no independent desktop scan); the icon cache resolves
    /// representative bundles on rebuild only (no added polls). Teardown
    /// removes the item explicitly on every path (`Drop` runs on all
    /// returns past construction - the watchdog forced exit excepted,
    /// where the OS reaps the item with the process).
    ///
    /// [`IndicatorDriver`]: crate::workspace_indicator::IndicatorDriver
    ///
    /// The live-proof fixture
    /// ([`crate::workspace_indicator_proof`]) hosts this same guard
    /// directly (never a rebuilt lookalike): construction, tick, drain,
    /// and teardown below are `pub(crate)` for that harness.
    pub(crate) struct IndicatorGuard {
        mtm: objc2::MainThreadMarker,
        options: crate::workspace_indicator::IndicatorOptions,
        tx: Option<std::sync::mpsc::SyncSender<crate::workspace_indicator::IndicatorClick>>,
        rx: Option<std::sync::mpsc::Receiver<crate::workspace_indicator::IndicatorClick>>,
        handle: Option<crate::ffi::WorkspaceIndicator>,
        icons: std::collections::HashMap<String, objc2::rc::Retained<objc2_app_kit::NSImage>>,
        driver: crate::workspace_indicator::IndicatorDriver,
    }

    impl IndicatorGuard {
        pub(crate) fn new(
            mtm: objc2::MainThreadMarker,
            options: crate::workspace_indicator::IndicatorOptions,
        ) -> Self {
            let driver = crate::workspace_indicator::IndicatorDriver::new(options.mode);
            let (tx, rx) = if driver.armed() {
                let (tx, rx) = std::sync::mpsc::sync_channel(16);
                (Some(tx), Some(rx))
            } else {
                (None, None)
            };
            Self {
                mtm,
                options,
                tx,
                rx,
                handle: None,
                icons: std::collections::HashMap::new(),
                driver,
            }
        }

        fn armed(&self) -> bool {
            self.driver.armed()
        }

        /// Whether a fresh revision/observation pair needs a rebuild (edge
        /// trigger for the occupancy query and frontmost read below: no
        /// engine round-trip and no extra reads on a steady cadence).
        fn needs_tick(&self, revision: u64, observations: u64) -> bool {
            self.driver.needs_tick(revision, observations)
        }

        fn updates(&self) -> u64 {
            self.driver.updates()
        }

        fn clicks(&self) -> u64 {
            self.driver.clicks()
        }

        /// Explicit teardown: remove the status item and forget the cached
        /// render (the next good snapshot refreshes from scratch). The
        /// driver generation is NOT reset, so clicks queued before the
        /// teardown refuse as stale instead of dispatching under an unsafe
        /// context. The icon cache survives (bundle-keyed images stay
        /// valid).
        pub(crate) fn teardown(&mut self) {
            if let Some(handle) = self.handle.take() {
                handle.destroy(self.mtm);
            }
            self.driver.teardown();
        }

        /// Discard queued click bindings without dispatching (lock, Secure
        /// Input, and stale-topology paths refuse narrowly instead of
        /// switching under an unsafe context).
        fn discard_clicks(&mut self) {
            if let Some(rx) = self.rx.as_ref() {
                while rx.try_recv().is_ok() {}
            }
        }

        /// Apply one fresh snapshot revision plus one retained occupancy
        /// report: rebuild the projection only when the driver trigger
        /// fires (moved revision, published observation, or empty cache -
        /// never a blind per-cadence poll) and push it to the native item
        /// only on a changed render (change-only dedup inside the driver).
        /// Occupancy is retained-engine truth (managed windows only);
        /// any unproven frame renders every row `Unknown`. Counts,
        /// ordinals, and the UI generation in the log only; SIDs, PIDs,
        /// bundles, titles, and paths never appear.
        pub(crate) fn tick(
            &mut self,
            rows: &[crate::ffi::SlsDisplayRow],
            revision: u64,
            observations: u64,
            report: Option<crate::runtime::WorkspaceOccupancyReport>,
            frontmost_pid: Option<u32>,
        ) {
            if !self.driver.needs_tick(revision, observations) {
                return;
            }
            let mut trailing: Option<(
                crate::workspace_indicator::TrailingSlot,
                crate::workspace_indicator::SpaceOccupancy,
                u64,
            )> = None;
            let projection = match crate::workspace_select::user_row_from_rows(rows, 0) {
                Ok(row) => {
                    let (domains, complete) = Self::retained_domains(&report, revision);
                    let summary = crate::workspace_indicator::summarize_retained(
                        &row.spaces,
                        &domains,
                        complete,
                        frontmost_pid,
                    );
                    // The trailing action slot renders only while the
                    // capability gate arms it (production `false`: no extra
                    // segment, no trailing click binding - disabled and
                    // unshown while unarmed, never an enabled-looking slot).
                    // The binding carries the TRUE native row identity
                    // (`row.spaces.last()` over the full native row, never
                    // the filtered projection's last row, which in
                    // `Current` mode shows only the current Space).
                    if crate::workspace_input::TRAILING_CAPABILITY_ENABLED {
                        let last_occupancy = summary
                            .iter()
                            .zip(row.spaces.iter())
                            .find(|(_, s)| **s == *row.spaces.last().unwrap_or(&0))
                            .map(|((o, _), _)| *o)
                            .unwrap_or(crate::workspace_indicator::SpaceOccupancy::Unknown);
                        trailing = row.spaces.last().copied().and_then(|native_last| {
                            crate::workspace_indicator::project_trailing_slot(
                                &row.spaces,
                                row.current,
                                rows.len(),
                                self.options.mode,
                                last_occupancy,
                            )
                            .map(|slot| (slot, last_occupancy, native_last))
                        });
                    }
                    crate::workspace_indicator::project_indicator(
                        &row.spaces,
                        row.current,
                        rows.len(),
                        self.options.mode,
                        &|sid| {
                            summary
                                .iter()
                                .zip(row.spaces.iter())
                                .find(|(_, s)| **s == sid)
                                .map(|((o, _), _)| *o)
                                .unwrap_or(crate::workspace_indicator::SpaceOccupancy::Unknown)
                        },
                        &|sid| {
                            summary
                                .iter()
                                .zip(row.spaces.iter())
                                .find(|(_, s)| **s == sid)
                                .and_then(|((_, r), _)| *r)
                                .and_then(|(pid, _)| {
                                    i32::try_from(pid)
                                        .ok()
                                        .and_then(crate::ffi::bundle_id_for_pid)
                                })
                        },
                    )
                    .unwrap_or_else(|reason| {
                        crate::workspace_indicator::IndicatorProjection {
                            visible: false,
                            hidden_reason: Some(reason),
                            title: String::new(),
                            rows: Vec::new(),
                        }
                    })
                }
                Err(reason) => crate::workspace_indicator::IndicatorProjection {
                    visible: false,
                    hidden_reason: Some(reason),
                    title: String::new(),
                    rows: Vec::new(),
                },
            };
            match self.driver.apply(
                projection,
                trailing.map(|(slot, occupancy, _)| (slot, occupancy)),
                revision,
                observations,
            ) {
                crate::workspace_indicator::DriverApply::Applied(generation) => {
                    self.push_native(generation, revision, trailing);
                }
                crate::workspace_indicator::DriverApply::Duplicate
                | crate::workspace_indicator::DriverApply::Disarmed => {}
            }
        }

        /// Map one retained occupancy report onto the live revision: the
        /// per-Space retained domains plus whether completeness is proven
        /// (report present AND homed under this exact revision). Anything
        /// else - no report, timed-out query, revision drift - is
        /// incomplete, and the summary renders `Unknown` throughout.
        /// Domains whose workspace key does not parse to a SID are
        /// dropped here (the summary names the coverage gap `Unknown`,
        /// never empty).
        fn retained_domains(
            report: &Option<crate::runtime::WorkspaceOccupancyReport>,
            revision: u64,
        ) -> (Vec<crate::workspace_indicator::RetainedDomainState>, bool) {
            let Some(report) = report else {
                return (Vec::new(), false);
            };
            if !report.context_present || report.context_revision != revision {
                return (Vec::new(), false);
            }
            let mut domains = Vec::with_capacity(report.domains.len());
            for domain in &report.domains {
                let Some(sid) = domain
                    .workspace
                    .strip_prefix("space-")
                    .and_then(|n| n.parse::<u64>().ok())
                    .filter(|sid| *sid != 0)
                else {
                    continue;
                };
                domains.push(crate::workspace_indicator::RetainedDomainState {
                    sid,
                    members: domain.members.clone(),
                });
            }
            (domains, true)
        }

        /// Push the driver's current render to the native item (create plus
        /// one atomic segment/menu rebuild, or removal on hidden). Every
        /// segment binds its immutable render-time click
        /// (`ui_generation` + `topology_revision` ride into the engine
        /// claim and the drain). Icons resolve per bundle through a
        /// main-thread cache when the flag is on; any unreadable icon
        /// renders the plain number instead. The button title clears:
        /// the segments carry every ordinal. The trailing action slot
        /// (when the pure projection offers one) renders one extra
        /// segment after the existing rows: `"+"` live when the last
        /// Space is proven occupied (pressing appends), `"+?"` disabled
        /// when occupancy is unknown (shown unavailable, never falsely
        /// empty - the drain refuses unknown instead of appending).
        /// Proven-empty last reuses the existing slot with no extra
        /// segment. The trailing click carries kind Trailing with position
        /// `render_len + 1` and the render-time NATIVE last SID
        /// (the true native row identity, never the filtered projection's
        /// last row), fenced at drain by [`crate::workspace_indicator::resolve_indicator_click`]
        /// plus the same guarded engine lane (capability-disabled drains
        /// refuse at admission; the slot renders only while armed).
        fn push_native(
            &mut self,
            generation: u64,
            revision: u64,
            trailing: Option<(
                crate::workspace_indicator::TrailingSlot,
                crate::workspace_indicator::SpaceOccupancy,
                u64,
            )>,
        ) {
            let Some(projection) = self.driver.last().cloned() else {
                return;
            };
            if !projection.visible {
                if let Some(handle) = self.handle.take() {
                    handle.destroy(self.mtm);
                }
                eprintln!(
                    "tiler-macos dev indicator hidden reason={} complete=true",
                    projection.hidden_reason.as_deref().unwrap_or("unknown"),
                );
                return;
            }
            if self.handle.is_none() {
                let Some(tx) = self.tx.clone() else {
                    return;
                };
                match crate::ffi::WorkspaceIndicator::create(self.mtm, "", tx) {
                    Ok(handle) => self.handle = Some(handle),
                    Err(reason) => {
                        eprintln!(
                            "tiler-macos dev indicator create refused reason={} complete=false",
                            reason,
                        );
                        // Forget the render so the next cadence retries
                        // from scratch (the generation still advances, so
                        // nothing queued can validate against a render that
                        // never reached the bar).
                        self.driver.teardown();
                        return;
                    }
                }
            }
            let Some(handle) = self.handle.as_ref() else {
                return;
            };
            handle.set_title(self.mtm, "");
            let mut rows: Vec<crate::ffi::SegmentRow> = projection
                .rows
                .iter()
                .map(|row| {
                    let icon = if self.options.icons {
                        row.representative.as_ref().and_then(|bundle| {
                            if let Some(cached) = self.icons.get(bundle) {
                                return Some(cached.clone());
                            }
                            let image = crate::ffi::ns_image_for_bundle(bundle)?;
                            self.icons.insert(bundle.clone(), image.clone());
                            Some(image)
                        })
                    } else {
                        None
                    };
                    crate::ffi::SegmentRow {
                        position: row.position,
                        title: crate::workspace_indicator::segment_title(
                            row.position,
                            row.occupancy,
                        ),
                        current: row.current,
                        enabled: row.selectable,
                        occupancy: row.occupancy,
                        icon,
                        click: crate::workspace_indicator::IndicatorClick {
                            kind: crate::workspace_indicator::IndicatorClickKind::Numbered,
                            position: row.position,
                            expected_sid: row.sid,
                            ui_generation: generation,
                            topology_revision: revision,
                        },
                    }
                })
                .collect();
            // Trailing action segment: same channel, position `len + 1`,
            // the render-time NATIVE last SID as the binding. Live
            // (enabled) only when the last Space is proven occupied;
            // unknown occupancy renders disabled, never as empty.
            if let Some((slot, last_occupancy, native_last)) = trailing {
                rows.push(crate::ffi::SegmentRow {
                    position: slot.position,
                    title: crate::workspace_indicator::trailing_segment_title(slot, last_occupancy),
                    current: false,
                    enabled: last_occupancy == crate::workspace_indicator::SpaceOccupancy::Occupied,
                    occupancy: last_occupancy,
                    icon: None,
                    click: crate::workspace_indicator::IndicatorClick {
                        kind: crate::workspace_indicator::IndicatorClickKind::Trailing,
                        position: slot.position,
                        expected_sid: native_last,
                        ui_generation: generation,
                        topology_revision: revision,
                    },
                });
            }
            handle.rebuild(self.mtm, &rows);
            let occupied = projection
                .rows
                .iter()
                .filter(|r| r.occupancy == crate::workspace_indicator::SpaceOccupancy::Occupied)
                .count();
            let unknown = projection
                .rows
                .iter()
                .filter(|r| r.occupancy == crate::workspace_indicator::SpaceOccupancy::Unknown)
                .count();
            let current = projection
                .rows
                .iter()
                .find(|r| r.current)
                .map(|r| r.position)
                .unwrap_or(0);
            eprintln!(
                "tiler-macos dev indicator rows={} current={} occupied={} unknown={} trailing={} generation={} complete=true",
                projection.rows.len(),
                current,
                occupied,
                unknown,
                u8::from(trailing.is_some()),
                generation,
            );
        }

        /// Drain queued click bindings: each routes by its rendered kind
        /// through [`crate::workspace_indicator::resolve_indicator_click`]
        /// against the CURRENT render generation plus the cadence's fresh
        /// user row and revision (rebuilt UI, moved topology, moved rows,
        /// and past-nine positions all refuse before any backend touch)
        /// and submits through the guarded engine lane with its immutable
        /// binding - the same gates as a tap select, the same main-thread
        /// drain, the same verified arrival. Numbered clicks never fall
        /// through to trailing creation; trailing slot clicks resolve only
        /// while the capability gate arms the slot (unarmed discards
        /// before the engine lane) with the same generation/revision/row
        /// fencing and the same lane. Refusals log the honest bounded
        /// reason (position/kind tokens plus the refusing fence, never
        /// SIDs). Positions and outcome tokens in the log only.
        fn drain_clicks(
            &mut self,
            fresh: &crate::workspace_select::SelectionSnapshot,
            fresh_revision: u64,
            runtime: &crate::runtime::RuntimeHandle,
        ) {
            let Some(rx) = self.rx.as_ref() else {
                return;
            };
            if self.driver.last().is_none() {
                self.discard_clicks();
                return;
            }
            let current_generation = self.driver.generation();
            while let Ok(click) = rx.try_recv() {
                let position = click.position;
                let kind = click.kind.as_str();
                let action = crate::workspace_indicator::resolve_indicator_click(
                    click,
                    current_generation,
                    &fresh.spaces,
                    fresh_revision,
                    crate::workspace_input::TRAILING_CAPABILITY_ENABLED,
                );
                match action {
                    Ok(action) => {
                        let id = crate::workspace_input::workspace_action_id(&action)
                            .unwrap_or("unknown-select");
                        match runtime.submit_indicator_select(
                            action,
                            click.expected_sid,
                            click.ui_generation,
                            click.topology_revision,
                        ) {
                            crate::runtime::SubmitOutcome::Accepted => {
                                self.driver.click_submitted();
                                eprintln!(
                                    "tiler-macos dev indicator click action={} position={} submitted=true",
                                    id, position,
                                );
                            }
                            _ => {
                                eprintln!(
                                    "tiler-macos dev indicator click action={} position={} submitted=false",
                                    id, position,
                                );
                            }
                        }
                    }
                    Err(reason) => {
                        eprintln!(
                            "tiler-macos dev indicator click kind={} position={} discarded reason={}",
                            kind,
                            position,
                            crate::workspace_indicator::bound_click_discard_reason(&reason),
                        );
                    }
                }
            }
        }

        /// Live segment readbacks off the actual AppKit buttons (title,
        /// tag, current, enabled, occupancy word, bold, icon presence,
        /// alpha, screen frame); empty without a rendered item. The
        /// live-proof harness reads appearance here, never a cached model.
        pub(crate) fn segment_snapshot(&self) -> Vec<crate::ffi::SegmentReadback> {
            self.handle
                .as_ref()
                .map(|handle| handle.segment_snapshot())
                .unwrap_or_default()
        }

        /// Live click-target bindings in render order: `(position,
        /// expected SID, UI generation, topology revision, slot kind)`
        /// off the actual retained target objects. Empty without a
        /// rendered item.
        pub(crate) fn segment_targets(
            &self,
        ) -> Vec<(
            usize,
            u64,
            u64,
            u64,
            crate::workspace_indicator::IndicatorClickKind,
        )> {
            self.handle
                .as_ref()
                .map(|handle| handle.segment_targets())
                .unwrap_or_default()
        }

        /// Menu row titles in live order; empty without a rendered item.
        pub(crate) fn row_titles(&self) -> Vec<String> {
            self.handle
                .as_ref()
                .map(|handle| handle.row_titles())
                .unwrap_or_default()
        }

        /// Native status-item button frame in screen coordinates, or `None`
        /// without a rendered item.
        pub(crate) fn status_frame(&self) -> Option<(f64, f64, f64, f64)> {
            self.handle.as_ref()?.status_frame()
        }

        /// Nonblocking take of one delivered click binding forwarded by
        /// the per-segment AppKit targets (`None` when nothing was
        /// clicked). The live-proof harness resolves the binding through
        /// the product lane itself; the tile cadence keeps draining
        /// through its own path.
        pub(crate) fn try_recv_click(&self) -> Option<crate::workspace_indicator::IndicatorClick> {
            self.rx.as_ref()?.try_recv().ok()
        }

        /// Diagnostic-only trailing-click injection for
        /// `--diag-trailing-click`: copy the ACTUAL rendered trailing
        /// binding off the live AppKit targets via
        /// [`crate::workspace_indicator::diag_rendered_trailing_click`]
        /// (never reconstructed, never a direct select-0 substitute; a
        /// disabled `+?` slot or a missing slot refuses instead of
        /// injecting) and loop it back through the same click channel an
        /// AppKit press would use. Returns a metadata-only token line
        /// (position/kind tokens, never SIDs) for the run log. The normal
        /// [`Self::drain_clicks`] path still fences the binding fresh
        /// (kind, generation, revision, row identity, capability gate)
        /// plus engine admission plus verified arrival, so this injection
        /// is necessary but never sufficient for a switch.
        pub(crate) fn diag_inject_rendered_trailing(&self) -> Result<String, String> {
            let tx = self.tx.as_ref().ok_or_else(|| {
                "diag-trailing-click: indicator disarmed (off mode); nothing injected".to_owned()
            })?;
            let targets = self.segment_targets();
            let snapshot = self.segment_snapshot();
            let click =
                crate::workspace_indicator::diag_rendered_trailing_click(&targets, &|position| {
                    snapshot
                        .iter()
                        .any(|readback| readback.position == position && readback.enabled)
                })
                .ok_or_else(|| {
                    "diag-trailing-click: no live trailing slot rendered; nothing injected"
                        .to_owned()
                })?;
            let position = click.position;
            let kind = click.kind.as_str();
            tx.try_send(click).map_err(|e| {
                format!("diag-trailing-click: channel refused the binding ({e}); nothing injected")
            })?;
            Ok(format!(
                "tiler-macos dev diag-trailing-click kind={kind} position={position} injected=true complete=false"
            ))
        }
    }

    impl Drop for IndicatorGuard {
        fn drop(&mut self) {
            if let Some(handle) = self.handle.take() {
                handle.destroy(self.mtm);
            }
        }
    }

    /// Minimum interval between publisher-woken border-only joins: AX
    /// notification bursts re-join at most this often (never topology or
    /// tiling at AX frequency). The runloop pump below already quantizes
    /// wakeups to roughly its own period, so this throttle only caps
    /// pathological bursts.
    const BORDER_FAST_MIN: Duration = Duration::from_millis(33);
    /// Pure border fast-path gate over explicit inputs (fake-clock
    /// testable): enabled runs re-join only on publisher dirt with the
    /// throttle satisfied. The caller peeks (never takes) here - the take
    /// happens only on a joining tick, so a throttled burst never loses
    /// its wakeup (the 250 ms cadence stays the backstop regardless).
    fn border_fast_due(enabled: bool, dirty: bool, elapsed: Duration) -> bool {
        enabled && dirty && elapsed >= BORDER_FAST_MIN
    }

    /// Display-leg publish gate over explicit inputs (pure, testable): a
    /// screen-parameter notification always republishes latest-wins; an
    /// absent slot (failed startup read or transient unreadable snapshot)
    /// recovers from the next good snapshot with no notification; a quiet
    /// poll with a populated slot leaves it untouched.
    fn display_publish_due(display_notified: bool, slot_populated: bool) -> bool {
        display_notified || !slot_populated
    }

    /// Main-thread cadence gate over explicit inputs (pure, testable): the
    /// 250 ms cadence stays the idle backstop, but a pending display/Space
    /// notification (stashed before the gate, so the 50 ms app pump never
    /// loses it) runs the cadence body early - topology and the
    /// send/select completion polls settle on the event instead of the next
    /// cadence. The early run resets the cadence clock, so idle polling
    /// never permanently quickens.
    fn cadence_due(elapsed: Duration, display_pending: bool) -> bool {
        elapsed >= CADENCE || display_pending
    }

    /// Current visible work areas in CG top-left points for the native
    /// zoom veto (one entry per display with converted geometry).
    fn visibles_of(
        snapshot: &crate::topology::TopologySnapshot,
    ) -> Vec<tiler_core::visual::VisualRect> {
        snapshot
            .displays
            .iter()
            .filter_map(|d| d.visible)
            .map(|r| tiler_core::visual::VisualRect {
                x: r.x,
                y: r.y,
                w: r.w,
                h: r.h,
            })
            .collect()
    }

    /// Current full display bounds in CG top-left points for the
    /// borderless full-display fence (one entry per display with
    /// converted geometry - never work areas).
    fn displays_of(
        snapshot: &crate::topology::TopologySnapshot,
    ) -> Vec<tiler_core::visual::VisualRect> {
        snapshot
            .displays
            .iter()
            .filter_map(|d| d.cg_bounds)
            .map(|r| tiler_core::visual::VisualRect {
                x: r.x,
                y: r.y,
                w: r.w,
                h: r.h,
            })
            .collect()
    }

    /// Engine-placement truth for one window: true only when the core's
    /// latest plan geometry names exactly `(pid, wid)`. Pure over the
    /// runtime's last desired snapshot (window IDs round-trip through
    /// `window_id`/`parse_window_id`); a `ToggleFloat`-floated window
    /// drops out of plan truth on the next plan and reads unplaced, so
    /// the zoom heuristic applies to it. Unknown reads false (safe
    /// direction: a ring may hide, never wrongly show).
    fn engine_places(desired: &[crate::runtime_types::DesiredFrame], pid: u32, wid: u32) -> bool {
        desired
            .iter()
            .any(|frame| crate::runtime_types::parse_window_id(&frame.window) == Some((pid, wid)))
    }

    /// Active-border tick: join the frontmost PID's owner-published facts
    /// with the live session verdict, the product scope gate, the current
    /// user Spaces plus topology revision, the last applied reply
    /// presentation, the fresh native frame plus display geometry, and the
    /// system accent - then drive the one main-thread carrier. Core shapes
    /// decide bounds; freshness/lifetime/session joins plus fresh native
    /// safety vetoes (classifier exclusions, zoom approximation,
    /// full-display fence) decide visibility. Normalization stays partial:
    /// the ingress cannot name maximized/cloaked/shell/hidden truthfully
    /// yet, so the vetoes fail closed instead. `engine_placed` still
    /// reports the tick's placement join on show/update lines (live
    /// placement-truth diagnosis, never a gate on its own: the zoom veto
    /// consults it). `snapshot` is the last good topology read (`None`
    /// hides: no flip height, no work areas, no display bounds).
    #[allow(clippy::too_many_arguments)]
    fn border_tick_once(
        selection: &crate::dev_selection::TileScope,
        border_store: &crate::border_runtime::BorderFactsStore,
        facts_store: &crate::input_eligibility::EligibilityFactsStore,
        controller: &mut crate::border_runtime::BorderController,
        guard: &mut BorderCarrierGuard,
        options: &crate::active_border::ActiveBorderOptions,
        snapshot: Option<&crate::topology::TopologySnapshot>,
        handle: &crate::runtime::RuntimeHandle,
    ) {
        let frontmost = crate::ffi::frontmost_app();
        let (context_currents, context_revision) = facts_store.context();
        let accent = crate::ffi::border_system_accent();
        let record = frontmost.as_ref().and_then(|(pid, _)| {
            if *pid > 1 {
                border_store.snapshot_for(*pid as u32)
            } else {
                None
            }
        });
        // Engine-placement join over the latest plan truth (never AX, never
        // the classifier): diagnostic only, never a visibility gate.
        let engine_placed = match frontmost.as_ref() {
            Some((pid, _)) if *pid > 1 => record
                .as_ref()
                .and_then(|record| record.focused_wid)
                .is_some_and(|wid| {
                    engine_places(&handle.last_desired_geometry(), *pid as u32, wid)
                }),
            _ => false,
        };
        let presentation = handle.last_presentation();
        // Fresh native frame in CG top-left points for the zoom and
        // full-display vetoes (publisher-side read, never AX on this path).
        let native_frame_cg = record
            .as_ref()
            .and_then(|record| record.window.as_ref())
            .map(|window| tiler_core::visual::VisualRect {
                x: window.frame.x,
                y: window.frame.y,
                w: window.frame.w,
                h: window.frame.h,
            })
            .unwrap_or(tiler_core::visual::VisualRect {
                x: 0.0,
                y: 0.0,
                w: 0.0,
                h: 0.0,
            });
        let input = crate::border_runtime::BorderReplyTick {
            enabled: options.enabled,
            style: options.style,
            accent,
            session_unlocked: matches!(
                crate::ffi::session_state().lock,
                crate::ffi::SessionLock::Unlocked
            ),
            frontmost_pid: frontmost
                .as_ref()
                .filter(|(pid, _)| *pid > 1)
                .map(|(pid, _)| *pid as u32),
            frontmost_bundle: frontmost.as_ref().map(|(_, bundle)| bundle.clone()),
            scope_allowed: frontmost
                .as_ref()
                .is_some_and(|(_, bundle)| selection.allows_effect(Some(bundle))),
            record,
            context_currents,
            context_revision,
            engine_placed,
            native_frame_cg,
            visibles: snapshot.map(visibles_of).unwrap_or_default(),
            full_displays: snapshot.map(displays_of).unwrap_or_default(),
            primary_h: snapshot
                .and_then(|s| crate::border_runtime::snapshot_primary_height(&s.displays)),
            now_ms: border_store.now_ms(),
            presentation: &presentation,
        };
        let effect = controller.tick_reply(&input);
        guard.drive(controller, effect, accent, engine_placed);
    }

    /// Publish the eligibility current context from a snapshot the main
    /// thread already took: every user-typed current Space (one per
    /// display - multi-display supported, each display homes its own
    /// current) plus the snapshot-revision fingerprint the backends stamp
    /// their records with. Type checks are the only new reads (SLS type
    /// calls the main thread already makes each cadence; no AX anywhere).
    /// Non-user currents are excluded; an empty set denies downstream until
    /// a clean snapshot republishes. Never invented from geometry.
    fn publish_snapshot_context(
        store: &crate::input_eligibility::EligibilityFactsStore,
        snapshot: &TopologySnapshot,
    ) {
        let candidates: Vec<Option<u64>> = snapshot.rows.iter().map(|row| row.current).collect();
        let currents = crate::input_eligibility::select_user_currents(candidates, &|sid| {
            crate::ffi::sls_space_type(sid)
                .ok()
                .is_some_and(|t| t == crate::ffi::SlsSpaceType::User)
        });
        store.publish_context(currents, snapshot.revision);
    }

    pub(super) fn run(request: TileRequest) -> Result<String, (i32, String)> {
        use objc2::MainThreadMarker;
        use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};

        let mtm = MainThreadMarker::new()
            .ok_or_else(|| (2, "tile must run on the main thread".to_owned()))?;
        // Lock pre-check: the actual session verdict first (never act without
        // a readable unlocked desktop), then the loginwindow heuristic.
        match crate::ffi::session_state().lock {
            crate::ffi::SessionLock::Unlocked => {}
            crate::ffi::SessionLock::Locked => {
                let mut outcome = TileOutcome::refused(&request, "locked at start".to_owned());
                outcome.lock_stops = 1;
                return Err((1, serde_json::to_string(&outcome).unwrap_or_default()));
            }
            crate::ffi::SessionLock::Unknown => {
                return Err((
                    1,
                    serde_json::to_string(&TileOutcome::refused(
                        &request,
                        "session unreadable at start (unknown lock state)".to_owned(),
                    ))
                    .unwrap_or_default(),
                ));
            }
        }
        match crate::ffi::frontmost_app() {
            Some((_, bundle)) if crate::focus::is_loginwindow_bundle(&bundle) => {
                let mut outcome = TileOutcome::refused(&request, "locked at start".to_owned());
                outcome.lock_stops = 1;
                return Err((1, serde_json::to_string(&outcome).unwrap_or_default()));
            }
            None => {
                return Err((
                    1,
                    serde_json::to_string(&TileOutcome::refused(
                        &request,
                        "session unreadable at start (unknown lock state)".to_owned(),
                    ))
                    .unwrap_or_default(),
                ));
            }
            Some(_) => {}
        }
        // Exact-run directory (foreground graceful lifecycle): it must
        // exist when passed - the launcher always creates it, so a missing
        // directory is a usage error, never a silent stdout-only run.
        if let Some(dir) = &request.run_dir
            && !dir.is_empty()
            && !std::path::Path::new(dir).is_dir()
        {
            return Err((2, format!("tile: --run-dir is not a directory: '{dir}'")));
        }
        // Accessory, no activation UI: `finishLaunching` publishes the
        // app without entering `run`; the bounded manual pump below keeps
        // lifetime control. Never steals focus, never opens windows.
        let app = NSApplication::sharedApplication(mtm);
        app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
        app.finishLaunching();

        let selection = request.selection();
        // Effective chord set under the selected preset: Authentic arms the
        // whole product catalogue (basic plus the 19 armed selects plus
        // the 10 armed send-follows),
        // Compatible leaves known conflicting chords to macOS (unarmed,
        // passing through natively - under the Ctrl role that is the 4
        // unshifted arrow basics plus the 10 numbered selects, the latter
        // still switching Spaces natively via Switch to Desktop N).
        let mut catalogue =
            effective_bindings(request.preset, request.modifier, &product_catalogue());
        // Diagnostic-only stay injection: exactly one numbered stay chord
        // appended to the same effective table the tap tunes and the config
        // validates (duplicate-free proven in `TileRequest::validate`; the
        // `RuntimeConfig` validator below re-checks). No product/default
        // binding change.
        if let Some(index) = request.diag_stay {
            let binding = diag_stay_binding(index)
                .ok_or((2, "tile: --diag-stay-index must be 1 or 2".to_owned()))?;
            let want = crate::workspace_input::effective_flags_for(request.modifier, &binding);
            if catalogue.iter().any(|existing| {
                existing.key_code == binding.key_code
                    && crate::workspace_input::effective_flags_for(request.modifier, existing)
                        == want
            }) {
                return Err((
                    2,
                    "tile: --diag-stay-index chord collides with the effective catalogue"
                        .to_owned(),
                ));
            }
            catalogue.push(binding);
        }
        let active_chords = catalogue.len();
        // Authentic Desktop-N holder ensure (enable-only with owned durable
        // preimages; explicit `spaces-desktop-revert` restores; no
        // automatic revert on shutdown). Runs once at startup, before any
        // send could dispatch: Desktops 1-9 coverage with preimage
        // published before the first write, resume-safe, external edits
        // preserved. A refusal logs and proceeds - the per-drain holder
        // leg still refuses disabled/unreadable holders before any post.
        // Compatible performs zero store touch here (no read, no write).
        if request.preset == InputPreset::Authentic {
            match crate::desktop_shortcuts::canonical_preimage_path() {
                Some(path) => {
                    let mut store = crate::desktop_shortcuts::LiveStore::new();
                    match crate::workspace_send::ensure_holders_for_preset(
                        request.preset,
                        &mut store,
                        &path,
                    ) {
                        Ok(Some(outcome)) => eprintln!(
                            "tiler-macos desktop holders ensured: {} applied, {} failed, resumed={} path={}",
                            outcome.results.len(),
                            outcome.failures.len(),
                            outcome.resumed,
                            path.display(),
                        ),
                        Ok(None) => {
                            eprintln!("tiler-macos desktop holders ensure skipped by preset gate")
                        }
                        Err(reason) => eprintln!(
                            "tiler-macos desktop holders ensure refused: {reason} (sends refuse at the holder leg)"
                        ),
                    }
                }
                None => eprintln!(
                    "tiler-macos desktop holders ensure skipped: no $HOME (sends refuse at the holder leg)"
                ),
            }
        }
        // Shared per-PID eligibility facts: each per-app AX owner publishes
        // only its own PID from snapshot data it already read; the input
        // owner consults plain snapshots (no AX crosses threads). Built
        // here before `Runtime::start` and shared with both factories -
        // no new threads, no new channels.
        let facts_store = Arc::new(crate::input_eligibility::EligibilityFactsStore::new());
        // Border facts ride the same publisher shape: each per-app AX
        // owner publishes its focused-window facts from already-read
        // snapshots; the main loop below joins the frontmost record with
        // the live verdict and drives one carrier (no AX on this thread).
        let border_store = Arc::new(crate::border_runtime::BorderFactsStore::new());
        let mut border_controller = crate::border_runtime::BorderController::new();
        let mut border_carrier = BorderCarrierGuard::new(mtm);
        // Menu-bar workspace indicator (startup mode): one native status
        // item driven change-only from the cached projection, fed by the
        // cadence snapshot plus engine observation counts (no polling of
        // its own). Clicks drain through the guarded engine lane.
        let mut indicator = IndicatorGuard::new(mtm, request.indicator);
        // Diagnostic-only trailing-click injection state (single-shot per
        // run: exactly one rendered trailing binding loops back, then the
        // normal drain fences it fresh).
        let mut diag_trailing_injected = false;
        // Drag-preview Escape watch (tap route with the startup switch
        // on only): the keyboard tap forwards otherwise-unmatched Escape
        // edges unsuppressed on this channel while the arm lives, so the
        // client still receives the key and the drag session observes the
        // cancel edge. The coalesced pending flag survives a full channel
        // (the drag turn drains it alongside the channel); the dropped
        // counter evidences any channel loss. Owned here (created before
        // the factory so the builder can wire it); drained by the drag
        // turn below.
        let drag_escape_armed = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let drag_escape_pending = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let drag_escape_dropped = Arc::new(std::sync::atomic::AtomicU32::new(0));
        // Drag-preview Escape wiring exists only while the live acceptance
        // gate is open. While parked (`live-acceptance-pending`) no drag
        // Escape wiring is created at all, and the arm below parks without
        // installing any mouse tap - regardless of drag_preview
        // enabled/color settings.
        let (drag_escape_tx, drag_escape_rx) = if matches!(request.input, TileInput::Tap)
            && request.drag.enabled
            && crate::drag_session::DRAG_LIVE_ACCEPTED
        {
            let (tx, rx) = std::sync::mpsc::sync_channel::<crate::ffi::TapHit>(16);
            (Some(tx), Some(rx))
        } else {
            (None, None)
        };
        // Exactly one input route arms per run; no auto fallback. Scoped
        // sources start ineligible until attested scope/foreground/Space/
        // showstate facts (no global interception while out-of-scope).
        enum InputGuard {
            Tap(Arc<TapTileFactory>),
            Off(Arc<OffTileFactory>),
        }
        let input: InputGuard = match request.input {
            TileInput::Tap => InputGuard::Tap(Arc::new(
                TapTileFactory::new(request.modifier, catalogue.clone())
                    .map(|f| {
                        let wired = f
                            .with_scope(selection.clone())
                            .with_facts_store(Arc::clone(&facts_store));
                        match &drag_escape_tx {
                            Some(tx) => wired.with_drag_escape_watch(
                                Arc::clone(&drag_escape_armed),
                                Arc::clone(&drag_escape_pending),
                                tx.clone(),
                                Arc::clone(&drag_escape_dropped),
                            ),
                            None => wired,
                        }
                    })
                    .map_err(|reason| (2, reason))?,
            )),
            TileInput::Off => InputGuard::Off(Arc::new(OffTileFactory)),
        };
        let input_factory: Arc<dyn InputSourceFactory> = match &input {
            InputGuard::Tap(factory) => Arc::clone(factory) as Arc<dyn InputSourceFactory>,
            InputGuard::Off(factory) => Arc::clone(factory) as Arc<dyn InputSourceFactory>,
        };
        let config = RuntimeConfig {
            topology_mode: TopologyMode::Native,
            // Scoped runs carry the explicit allowlist; unscoped runs
            // enforce ordinary-app selection in the factories/discovery
            // instead (an empty allowlist disables scope enforcement, which
            // is exactly the unscoped product rule).
            allowed_bundles: if request.unscoped {
                Vec::new()
            } else {
                request.scopes.clone()
            },
            modifier: request.modifier,
            tile_bindings: Some(catalogue),
            ..RuntimeConfig::default()
        };
        config.validate().map_err(|reason| (2, reason))?;
        // Hard lifetime for BOUNDED runs, armed BEFORE any runtime/input
        // effect: finite startup plus soft run lifetime plus bounded
        // teardown plus finite grace, then a dirty forced exit. `Drop`
        // disarms as a backstop so early errors never leave a delayed
        // forced exit behind. Unbounded product runs (unscoped `--seconds
        // 0`) arm no watchdog: owner exit or exact-PID stop ends them.
        let unbounded = request.is_unbounded();
        let mut watchdog = if unbounded {
            None
        } else {
            Some(
                Watchdog::arm(watchdog_limit(
                    config.startup_ready_timeout,
                    request.seconds,
                    SHUTDOWN_BOUND,
                    WATCHDOG_GRACE,
                ))
                .map_err(|reason| (1, reason))?,
            )
        };
        // Observe stop independently from startup and all main-thread native
        // reads; a wedged controller must not postpone the stop deadline.
        let mut stop_watchdog = if unbounded {
            request
                .run_dir
                .as_ref()
                .map(|dir| {
                    Watchdog::arm_on_stop(
                        std::path::Path::new(dir).join("stop-requested"),
                        stop_limit(SHUTDOWN_BOUND, STOP_WATCHDOG_GRACE),
                    )
                })
                .transpose()
                .map_err(|reason| (1, reason))?
        } else {
            None
        };
        // Started marker for the fixture harness: exact tiler PID plus a
        // per-run ID on stdout BEFORE any runtime/input effect, so an
        // independent controller can bind (pid + start/exe) and address
        // exactly this process for the forced-kill leg. No user content:
        // IDs only. A nonce failure never fails the run (falls back to a
        // pid-anchored ID); the PID itself is the identity, the run ID only
        // disambiguates relaunches in one harness log.
        //
        // Exact-run lifecycle: when the launcher passed `--run-dir`, the
        // run ID is the launcher's per-run nonce from `<dir>/run-id`
        // (binding this output to that exact run); otherwise a fresh nonce
        // is minted as before.
        {
            let runid = request
                .run_dir
                .as_ref()
                .and_then(|dir| {
                    std::fs::read_to_string(std::path::Path::new(dir).join("run-id"))
                        .ok()
                        .map(|s| s.trim().to_owned())
                        .filter(|s| !s.is_empty())
                })
                .unwrap_or_else(|| {
                    crate::probe_target::mint_nonce().unwrap_or_else(|_| {
                        format!("run-{}-{}", std::process::id(), request.seconds)
                    })
                });
            println!(
                "{}",
                serde_json::json!({
                    "tilestarted": true,
                    "pid": std::process::id(),
                    "runid": runid,
                    "mode": mode_str(request.unscoped),
                    "unbounded": unbounded,
                })
            );
            use std::io::Write as _;
            let _ = std::io::stdout().flush();
        }
        // A stop published before startup still stops exactly this run
        // (before any input/AX effect): no runtime starts, the stopped
        // outcome publishes like any other terminal state.
        if stop_requested(&request.run_dir) {
            let mut outcome = TileOutcome::refused(&request, "stop requested".to_owned());
            outcome.outcome = "stopped".to_owned();
            outcome.shutdown_outcome = "not-started".to_owned();
            outcome.shutdown_clean = true;
            let body = serde_json::to_string(&outcome).map_err(|e| (1, e.to_string()))?;
            super::write_run_outcome(&request.run_dir, &body);
            return Err((1, body));
        }
        let ax_factory = Arc::new(
            if request.unscoped {
                ScopedAxFactory::unscoped(request.excluded.clone())
            } else {
                ScopedAxFactory::new(request.scope())
            }
            .map(|f| {
                f.with_facts_store(Arc::clone(&facts_store))
                    .with_border_store(Arc::clone(&border_store))
            })
            .map_err(|reason| (2, reason))?,
        );
        let handle = Runtime::start(config, ax_factory, input_factory).map_err(|failure| {
            (
                1,
                serde_json::to_string(&TileOutcome::refused(
                    &request,
                    format!("runtime start refused: {}", failure.error()),
                ))
                .unwrap_or_default(),
            )
        })?;
        // Display/Space-change observer (events trigger resnapshot;
        // bounded fallback polling below covers a missed event).
        let (guard, display_flag) = crate::ffi::install_display_change_guard()
            .map_err(|reason| (1, format!("display guard refused: {reason}")))?;
        // The guard owns the active-Space observer for the run lifetime;
        // its shared wake settles off-main Dock-swipe selects event-driven.
        handle.publish_select_wake(guard.select_wake());
        // Startup-resolved presentation style for the report ingress, so
        // core shapes use the configured width/gap instead of a default
        // outset. Startup-only: a running session never re-reads it.
        handle.publish_presentation_style(&request.border.style, request.border.enabled);
        // Meta+left drag-preview arm (tap route with the startup switch
        // on only): session plus filled carrier plus the mouse-tap owner
        // thread. A spawn denial parks the arm with its reason (keyboard
        // tiling continues); the tap thread's own creation denial lands
        // in its refusal slot and parks at shutdown with the same shape.
        let mut drag_parked = crate::drag_session::DragRunState::parked_reason(
            request.drag.enabled,
            matches!(request.input, TileInput::Tap),
        );
        let mut drag_state: Option<crate::drag_session::DragRunState> =
            match (drag_parked.is_none(), drag_escape_rx) {
                (true, Some(escape_rx)) => {
                    match crate::drag_session::DragRunState::arm(
                        request.drag,
                        request.modifier,
                        mtm,
                        &drag_escape_armed,
                        &drag_escape_pending,
                        escape_rx,
                    ) {
                        Ok(state) => Some(state),
                        Err(reason) => {
                            drag_parked = Some(reason);
                            None
                        }
                    }
                }
                _ => None,
            };
        // Dispatch-ready boundary: the runtime (engine plus input-tap
        // owner) started, the display observer installed, and the drag arm
        // resolved (armed, or parked with its reason above). Only now may a
        // live operator gate dispatch on this line instead of sleeping; the
        // holder-ensure outcome keeps its own earlier line. One static line
        // while the run lives; streams live under `--stream`.
        eprintln!(
            "{}",
            crate::workspace_send::format_send_readiness_line(
                request.input.as_str(),
                request.preset.as_str(),
                request.scopes.len(),
                crate::workspace_send::send_armed_chords(
                    matches!(request.input, TileInput::Tap),
                    active_chords,
                ),
            )
        );
        let mut poll = TopologyPoll::new();
        let mut tracked: BTreeMap<u32, TrackedApp> = BTreeMap::new();
        // The core owns product select history and targeting; the
        // serialized Dock-swipe selection owner holds only trailing
        // creation proofs (adopted on verified append for the whole run
        // lifetime - never persisted, never guessed). The main thread
        // submits claims nonblocking and polls completions nonblocking;
        // it never waits and never touches core state directly, so no
        // concurrent send/select effect can race it.
        let mut select_controller = crate::select_owner::SelectController::new();
        // Async live-send controller: the main-thread side of the dedicated
        // managed-send effect owner (spawned lazily on the first submit, so
        // runs that never send never spawn it). Submissions and completion
        // polls below are nonblocking; the effect runs off the engine and
        // AppKit. Numbered send-follow 1..9 is armed.
        let mut send_controller = crate::workspace_send::LiveSendController::new();
        // Bounded runs end at the soft deadline (the watchdog above is the
        // hard lifetime); unbounded product runs loop until owner exit or
        // exact-PID stop.
        let deadline = if unbounded {
            None
        } else {
            Some(Instant::now() + Duration::from_secs(request.seconds))
        };
        let mut cadence_count = 0u64;
        let mut last_cadence = Instant::now()
            .checked_sub(CADENCE)
            .unwrap_or_else(Instant::now);
        // Pending display/Space notification stashed before the cadence
        // gate: the 50 ms app pump takes the guard flag every iteration so
        // no event waits for the 250 ms cadence, and the stash (never the
        // raw flag) drives the early run below - a skipped early run keeps
        // its wakeup instead of losing it. Cleared when the cadence body
        // consumes it.
        let mut display_pending = false;
        // Last publisher-woken border-only join (fast path below throttles
        // against this; starts at first-pump so an early burst joins).
        let mut last_border_fast = Instant::now();
        let mut launched = 0u64;
        let mut launch_refused = 0u64;
        let mut terminated = 0u64;
        let mut topology_updates = 0u64;
        let mut topology_failures = 0u64;
        let mut display_events = 0u64;
        let mut lock_stops = 0u64;
        let mut secure_active_seen = 0u64;
        let mut early_refusal: Option<String> = None;
        // Last printed heartbeat: heartbeats are change-only (idle silence),
        // so this starts empty and the first stride tick always prints.
        let mut last_heartbeat: Option<HeartbeatState> = None;
        // Graceful exact-run stop: published by the launcher into this
        // run's directory, observed here, then the existing bounded
        // shutdown runs (never a signal kill).
        let mut stopped = false;
        // Last good topology read for the border tick (visible work areas
        // for the zoom heuristic, primary height for the AppKit flip).
        // Refreshed on every good read, not just on change.
        let mut last_snapshot: Option<TopologySnapshot> = None;

        // Trailing lifecycle owner: baselined on the preexisting native row
        // (never removed, even when proven empty). Exact-append SIDs this
        // runtime creates adopt on verified select effects; only owned SIDs
        // ever remove, and only when proven empty, inactive, above the
        // shared floor, with fresh native proof at bounded shutdown.
        let mut trailing_lifecycle = crate::workspace_select::TrailingLifecycle::default();
        // Initial snapshot before any discovery: scoped observations fence
        // without a published context, so publish first. A failed read
        // invalidates (clears the live cell and the stored snapshot alike)
        // so no stale revision keeps authorizing effects, and resets the
        // poll so the recovered snapshot republishes even when its revision
        // is identical to the pre-failure one.
        match snapshot_live() {
            Ok(snapshot) => {
                poll.poll(&snapshot);
                publish_snapshot_context(&facts_store, &snapshot);
                handle.publish_display_inputs(&snapshot);
                if let Ok(row) = crate::workspace_select::user_row_from_rows(&snapshot.rows, 0) {
                    trailing_lifecycle =
                        crate::workspace_select::TrailingLifecycle::new(&row.spaces);
                }
                last_snapshot = Some(snapshot.clone());
                handle.update_topology_with(snapshot, &|sid| crate::ffi::sls_space_type(sid).ok());
                topology_updates += 1;
            }
            Err(_) => {
                handle.invalidate_topology();
                facts_store.invalidate_context();
                poll = TopologyPoll::new();
                topology_failures += 1;
            }
        }

        while deadline.is_none_or(|d| Instant::now() < d) {
            // Bounded main-thread pump: service application events plus the
            // runloop without entering `run`. No windows open here.
            objc2::rc::autoreleasepool(|_| {
                crate::ffi::drain_app_events(&app);
                crate::ffi::pump_runloop_once(50);
            });
            // Event stash for the cadence gate below: take the guard flag
            // on every 50 ms pump so a display/Space notification never
            // waits for the 250 ms cadence, and stash it (never act here)
            // so a skipped early run keeps its wakeup. The cadence body
            // consumes the stash with the same lock/secure gates as ever.
            if display_flag.take_changed() {
                display_pending = true;
            }
            // Border-only fast path: a publisher publish/invalidation sets
            // the wakeup flag, and this re-runs ONLY the border join (no
            // topology, no tiling, no AX) - throttled so AX-frequency
            // publishes never spin the join. Idle runs never set the flag
            // (unchanged snapshots don't republish), so this costs two
            // atomic loads per pump. The take happens only on a joining
            // tick, so a throttled burst keeps its wakeup; the 250 ms
            // cadence below stays the backstop (slow polls, heartbeats,
            // stop file).
            if border_fast_due(
                request.border.enabled,
                border_store.is_dirty(),
                last_border_fast.elapsed(),
            ) && border_store.take_dirty()
            {
                last_border_fast = Instant::now();
                border_tick_once(
                    &selection,
                    &border_store,
                    &facts_store,
                    &mut border_controller,
                    &mut border_carrier,
                    &request.border,
                    last_snapshot.as_ref(),
                    &handle,
                );
            }
            if cadence_due(last_cadence.elapsed(), display_pending) {
                last_cadence = Instant::now();
            } else {
                // The app pump still took the guard flag above (stashed in
                // `display_pending`), so an early run skipped here keeps its
                // wakeup for the next pump - nothing is lost by waiting.
                continue;
            }
            // Consume the stashed notification at the topology site below
            // (after the lock/secure gates, which `continue` without
            // consuming): a concurrent flag set between the pre-gate take
            // and that poll joins via a second take, so no event is lost
            // either way.
            cadence_count += 1;
            // Foreground verbosity: lifecycle/recovery lines always stream;
            // verbose adds a counter heartbeat, trace shortens it and adds
            // input forward/ignore counters. Static tokens plus numbers
            // only - never titles, bundles, or paths. Heartbeats are
            // change-only: an idle run prints once, then stays quiet until
            // a counter moves.
            if let Some(stride) = heartbeat_stride(request.log)
                && cadence_count.is_multiple_of(stride)
            {
                if request.log == TileLogLevel::Trace {
                    let diagnostics = handle.diagnostics();
                    let state = HeartbeatState {
                        launched,
                        terminated,
                        topo_updates: topology_updates,
                        topo_failures: topology_failures,
                        lock_stops,
                        secure_seen: secure_active_seen,
                        input: Some((diagnostics.input_forwarded, diagnostics.input_ignored)),
                    };
                    if last_heartbeat.as_ref() != Some(&state) {
                        last_heartbeat = Some(state);
                        eprintln!(
                            "tiler-macos dev heartbeat mode={} launched={} terminated={} topo_updates={} topo_failures={} lock_stops={} secure_seen={} input_fwd={} input_ignored={}",
                            mode_str(request.unscoped),
                            launched,
                            terminated,
                            topology_updates,
                            topology_failures,
                            lock_stops,
                            secure_active_seen,
                            diagnostics.input_forwarded,
                            diagnostics.input_ignored,
                        );
                    }
                } else {
                    // Verbose prints four counters only: gate on exactly
                    // those, so an unprinted move (topology failure,
                    // secure-input sighting) never reprints identical text.
                    let state = HeartbeatState {
                        launched,
                        terminated,
                        topo_updates: topology_updates,
                        topo_failures: 0,
                        lock_stops,
                        secure_seen: 0,
                        input: None,
                    };
                    if last_heartbeat.as_ref() != Some(&state) {
                        last_heartbeat = Some(state);
                        eprintln!(
                            "tiler-macos dev heartbeat mode={} launched={} terminated={} topo_updates={} lock_stops={}",
                            mode_str(request.unscoped),
                            launched,
                            terminated,
                            topology_updates,
                            lock_stops,
                        );
                    }
                }
            }

            // Input refusal (tap user-disable) ends the run truthfully:
            // never a silent success.
            if let InputGuard::Tap(factory) = &input
                && let Some(reason) = factory.refusal()
            {
                early_refusal = Some(reason);
                break;
            }

            // Exact-run stop: break out to the existing bounded shutdown
            // below, which publishes the stopped terminal outcome.
            if stop_requested(&request.run_dir) {
                stopped = true;
                break;
            }

            // Lock gate: the actual session verdict plus the loginwindow
            // heuristic. A locked session stops effects (terminate every
            // worker; interception already released by the tap gate). An
            // unreadable session holds discovery without destroying.
            match crate::ffi::session_state().lock {
                crate::ffi::SessionLock::Unlocked => {}
                crate::ffi::SessionLock::Locked => {
                    lock_stops += 1;
                    // A locked session never dispatches a switch: drop any
                    // pending select without replay and cancel an inflight
                    // Dock-swipe select (its terminal completion settles
                    // honestly on a later tick; never replayed).
                    handle.discard_select_pending();
                    select_controller.cancel();
                    // A locked session never indicates either: remove the
                    // item and drop queued clicks (unlock refreshes below).
                    indicator.teardown();
                    indicator.discard_clicks();
                    // A locked session never decorates either: hide now
                    // (this branch skips the border tick below).
                    let effect = border_controller.reset();
                    border_carrier.drive(
                        &mut border_controller,
                        effect,
                        crate::ffi::border_system_accent(),
                        false,
                    );
                    // A locked session never drags either: cancel stale
                    // and hide the preview carrier (skips the turn below).
                    if let Some(drag) = drag_state.as_mut() {
                        crate::drag_session::park_drag_for_lock(drag);
                    }
                    for pid in tracked.keys().copied().collect::<Vec<_>>() {
                        handle.terminate_app(pid);
                        facts_store.invalidate_pid(pid);
                        border_store.invalidate_pid(pid);
                        tracked.remove(&pid);
                        terminated += 1;
                    }
                    continue;
                }
                crate::ffi::SessionLock::Unknown => continue,
            }
            match crate::ffi::frontmost_app() {
                Some((_, bundle)) if crate::focus::is_loginwindow_bundle(&bundle) => {
                    lock_stops += 1;
                    // Lock-screen foreground: same refusal as a locked
                    // session (no switch under the lock screen, no replay).
                    handle.discard_select_pending();
                    select_controller.cancel();
                    // Same indicator teardown (this branch skips the tick
                    // below).
                    indicator.teardown();
                    indicator.discard_clicks();
                    // Same border hide (this branch skips the tick below).
                    let effect = border_controller.reset();
                    border_carrier.drive(
                        &mut border_controller,
                        effect,
                        crate::ffi::border_system_accent(),
                        false,
                    );
                    // Same drag park (this branch skips the turn below).
                    if let Some(drag) = drag_state.as_mut() {
                        crate::drag_session::park_drag_for_lock(drag);
                    }
                    for pid in tracked.keys().copied().collect::<Vec<_>>() {
                        handle.terminate_app(pid);
                        facts_store.invalidate_pid(pid);
                        border_store.invalidate_pid(pid);
                        tracked.remove(&pid);
                        terminated += 1;
                    }
                    continue;
                }
                None => continue,
                Some(_) => {}
            }
            if matches!(crate::ffi::secure_input_known(), Some(true)) {
                secure_active_seen += 1;
                // Secure Input capture never dispatches a switch (the tap
                // gate already released interception; this is the backstop).
                handle.discard_select_pending();
                select_controller.cancel();
                // Same for indicator clicks queued under capture.
                indicator.discard_clicks();
            }

            // Topology: the 250 ms poll still owns the full snapshot; the
            // send display legs publish on screen-parameter notification,
            // plus startup above, plus recovery when the slot stands empty
            // (a transient unreadable read clears it - the next good
            // snapshot restores it without waiting for a notification), so
            // a quiet poll with a populated slot never moves it. The stash
            // take below runs after the lock/secure gates (which `continue`
            // without consuming), and joins a concurrent flag set racing
            // the gate, so no event is lost either way.
            let display_notified =
                std::mem::replace(&mut display_pending, false) || display_flag.take_changed();
            if display_notified {
                display_events += 1;
            }
            let topology_read_started_ms = crate::input_eligibility::now_ms();
            match snapshot_live() {
                Ok(snapshot) => {
                    if display_publish_due(display_notified, handle.has_display_inputs()) {
                        handle.publish_display_inputs(&snapshot);
                    }
                    last_snapshot = Some(snapshot.clone());
                    // Trailing bookkeeping reads the row before the move below.
                    let trailing_row =
                        crate::workspace_select::user_row_from_rows(&snapshot.rows, 0).ok();
                    if let Some(row) = trailing_row.as_ref() {
                        for receipt in select_controller.observe_creation_receipts(row, topology_read_started_ms) {
                            handle.report_creation_attempt(
                                &receipt.operation_id,
                                &receipt.logical,
                                &receipt.output.0,
                                receipt.setter,
                                &receipt.order_before,
                                &receipt.order_after,
                                receipt.created,
                                receipt.attempt_ms,
                            );
                        }
                    }
                    if !matches!(
                        poll.poll(&snapshot),
                        crate::topology::PollOutcome::Unchanged
                    ) {
                        publish_snapshot_context(&facts_store, &snapshot);
                        handle.update_topology_with(snapshot, &|sid| {
                            crate::ffi::sls_space_type(sid).ok()
                        });
                        topology_updates += 1;
                        // Trailing bookkeeping on a moved topology: forget
                        // owned SIDs that vanished from the native row
                        // (externally removed - never re-removed here, never
                        // replayed). No MC effects run mid-loop; actual
                        // removals wait for the bounded shutdown below.
                        if let Some(row) = trailing_row.as_ref() {
                            for sid in trailing_lifecycle.owned().to_vec() {
                                if !row.spaces.contains(&sid) {
                                    let _ = trailing_lifecycle.release_removed(sid);
                                    eprintln!(
                                        "tiler-macos dev trailing released-externally sid={} complete=true",
                                        sid,
                                    );
                                }
                            }
                        }
                    }
                    // Indicator refresh (event-driven, change-only): the
                    // snapshot revision, engine observation counts, and an
                    // empty cache are the only rebuild triggers - never a
                    // blind per-cadence rebuild. Occupancy rides the
                    // engine's retained domains over the control lane
                    // (bounded round-trip; a timeout renders Unknown, never
                    // a stale report - no independent desktop scan, no
                    // steady-state polls: the query and the frontmost read
                    // run only when a rebuild is actually due). Clicks drain
                    // against this same fresh row below.
                    if indicator.armed()
                        && let Some(live) = last_snapshot.as_ref()
                    {
                        let observations = handle.diagnostics().observations_published;
                        if indicator.needs_tick(live.revision, observations) {
                            let report =
                                handle.query_workspace_occupancy(Duration::from_millis(100));
                            let frontmost_pid = crate::ffi::frontmost_app()
                                .and_then(|(pid, _)| u32::try_from(pid).ok())
                                .filter(|pid| *pid > 1);
                            indicator.tick(
                                &live.rows,
                                live.revision,
                                observations,
                                report,
                                frontmost_pid,
                            );
                        }
                    }
                }
                Err(_) => {
                    handle.invalidate_topology();
                    facts_store.invalidate_context();
                    poll = TopologyPoll::new();
                    topology_failures += 1;
                    // Stale topology never dispatches: drop any pending
                    // select instead of switching under unreadable rows and
                    // cancel an inflight select (its drift legs refuse
                    // without replay; the terminal completion settles
                    // honestly on a later tick).
                    handle.discard_select_pending();
                    select_controller.cancel();
                    // Stale topology never indicates either: remove the
                    // item and drop queued clicks (a good snapshot
                    // refreshes from scratch below).
                    indicator.teardown();
                    indicator.discard_clicks();
                }
            }

            // Focus evidence: workspace frontmost only (plain IDs). The main
            // thread performs no AX reads under any cadence - a hung app
            // must never stall the controller - so `ax_focus` stays `None`
            // here: each owning per-app AX thread reads the bounded
            // selected-oracle leg on its own thread and stamps it at send
            // time (never the diagnostic system-wide leg). Stale foreground
            // out of scope clears observed focus engine-side via
            // disagreement - never guessed here.
            let frontmost = crate::ffi::frontmost_app();
            handle.update_focus_evidence(FocusEvidence {
                frontmost_pid: frontmost.as_ref().map(|(pid, _)| *pid as u32),
                frontmost_bundle: frontmost.map(|(_, bundle)| bundle),
                ax_focus: None,
            });

            // Product workspace drain: one claimed pending workspace action
            // runs its proven native leg with fresh topology/readback -
            // the tap callback and the serial engine handed off
            // asynchronously and never block. Selects (action plus, for
            // indicator clicks, the immutable render-time SID/revision
            // binding) run the proven native switch. Numbered sends 0..9
            // follow plus 1..9 stay
            // (engine-admitted; stays bindable unbound, so only the diag
            // chord delivers them): the send leg below
            // submits engine-admitted send claims to the dedicated
            // managed-send effect owner (fresh proof, tagged HID posts, and
            // bounded readback all run there - never on the engine or this
            // AppKit thread) and reconciles their completions nonblocking
            // on later ticks. The owner routes on the pinned intent
            // (forward drag for follows, forward drag plus released return
            // select for stays). Both lanes refuse unless exactly one native
            // display row exists (no first-display guessing), refuse bound
            // claims whose ordinal no longer names their SID or whose
            // revision drifted, recheck session/Secure Input/lock-screen
            // immediately pre-dispatch (the native legs re-gate
            // session/trust at every leg boundary; no atomicity past the
            // recheck is promised). Verified follow arrival advances
            // the per-display previous history (stays never note history);
            // every refusal (gates, unarmed, display count, subject,
            // target, holder, drift, stale binding, no effect) clears
            // without replay and advances nothing. A claim still held at
            // shutdown is dropped, never dispatched.
            //
            // Live-send completion poll (nonblocking): reconcile at most
            // one settled effect per tick before draining new claims. The
            // effect owner proved arrival post-release; the transfer
            // feedback under the claim's operation identity settles the
            // core's retained transfer here (core owns select history, so
            // no send-driven previous-history notes). Focus follows through
            // the observation loop (the proof held focus on the subject at
            // every edge; no focus substitution actuation repairs a send).
            if let Some(settle) = send_controller.poll() {
                let outcome = crate::workspace_send::send_drain_outcome_for(settle.outcome);
                // Owner-verified trailing append: the created SID adopts
                // into lifecycle ownership here (token or not: the Space
                // persists even when the drag failed). The owner-verified
                // target SID stays drain-side for arrival proof only.
                if let Some(proof) = settle.trailing.as_ref()
                    && let Some(created) = proof.created
                    && let Err(reason) = trailing_lifecycle.adopt_created(created)
                {
                    eprintln!(
                        "tiler-macos dev trailing adopt sid={created} refused reason={reason} complete=true",
                    );
                }
                eprintln!(
                    "tiler-macos send settled target={} outcome={:?} complete=true",
                    settle.target.target_sid(),
                    outcome,
                );
                // The setter is the owner's raw write-boundary truth, staged
                // verbatim — never re-derived here from the arrival outcome.
                handle.report_send_outcome(
                    settle.target.operation_id(),
                    settle.setter,
                    settle.pre_member,
                    settle.post_member,
                    settle.pre_view,
                    settle.post_view,
                    settle.baseline,
                    settle.follow_view_operation.as_deref(),
                );
            }
            // Proof-less trailing completions (preparation refused or was
            // cancelled before binding): feedback-only settlement under the
            // operation identity - no history, no proof, never replayed.
            if let Some(refusal) = send_controller.poll_trailing_refusal() {
                eprintln!(
                    "tiler-macos send trailing refused setter={:?} complete=true",
                    refusal.setter,
                );
                handle.report_send_outcome(
                    &refusal.operation_id,
                    refusal.setter,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                );
            }
            // Dock-swipe select completion poll (nonblocking): reconcile at
            // most one settled select per tick before draining new claims.
            // The selection owner proved arrival event-driven off-main; the
            // trailing append proof adopts into lifecycle ownership here
            // (success or failure - the Space persists from the verified
            // append), while the proven outcome reports back under the
            // claim's operation identity.
            if let Some((completion, reaped)) = select_controller.poll() {
                adopt_select_completion(&handle, &mut trailing_lifecycle, &completion);
                match completion.result.as_ref() {
                    Ok(success) => {
                        eprintln!(
                            "tiler-macos dev select action={} changed={} caffeinate_reaped={} complete=true",
                            completion
                                .operation_id
                                .as_deref()
                                .unwrap_or("unknown-select"),
                            success.effect.changed,
                            reaped,
                        );
                        report_select_completion(&handle, &completion);
                    }
                    Err(failure) => {
                        eprintln!(
                            "tiler-macos dev select action={} refused reason={} complete=false",
                            completion
                                .operation_id
                                .as_deref()
                                .unwrap_or("unknown-select"),
                            failure.reason,
                        );
                        report_select_completion(&handle, &completion);
                    }
                }
            }
            // Explicit cross-lane exclusion: the send owner and the select
            // owner never actuate concurrently. While either lane is
            // inflight the pending claim waits for a later tick (the slot
            // stays pending - never claimed, never dropped here).
            let effects_busy = select_controller.is_busy() || send_controller.is_busy();
            if effects_busy {
                // A send or select effect is inflight: leave the pending
                // claim for a later tick (never claimed, never dropped).
            } else if let Some(claim) = handle.claim_select_for_drain() {
                let id = crate::workspace_input::workspace_action_id(&claim.action)
                    .unwrap_or("unknown-select");
                // The operation identity rides out of the claim before the
                // drain consumes it: the proven outcome reports back under
                // the same id so the core settles its planner-owned view
                // target (selects) or window transfer (sends) - never
                // replayed. The trailing token rides out the same way so
                // the drain pairs the engine-minted pending token with its
                // exact native append proof.
                let operation_id = claim.operation_id.clone();
                // Numbered sends 1..9 route to the dedicated effect owner
                // (follows and stays share the single-flight lane; stays
                // still refused at the engine capability gate until a
                // reviewed arming, so no product stay reaches here yet).
                // Trailing send-0 follow routes the same lane (armed: the
                // drain ensures the
                // target natively first (reuse-or-append), binds it to the
                // admitted original pins, and submits the pinned target.
                if matches!(
                    claim.action,
                    crate::runtime_types::InputSemanticAction::WorkspaceSend { index: 1..=9, .. }
                ) || matches!(
                    claim.action,
                    crate::runtime_types::InputSemanticAction::WorkspaceSend {
                        index: 0,
                        follow: true,
                    }
                ) {
                    // Reply-driven sends route to the dedicated effect owner
                    // with reply-bound native targets (follows and stays
                    // share the single-flight lane; stays still refused at
                    // the engine capability gate until a reviewed arming, so
                    // no product stay reaches here yet). The reply binding
                    // (accepted transfer target plus its live native
                    // binding) authorizes the native send through a live SID
                    // lookup at submit. Unbound targets without an explicit
                    // creation desire refuse here before any post (missing
                    // bindings never create); unbound targets WITH a desire
                    // also refuse here: the desire-gated native creation
                    // seam is unavailable pending typed truthful receipts
                    // (prerequisite blocker, not a settled refusal). No
                    // local reuse-or-append preparation runs on this path.
                    // Follow-history attribution from this tick's fresh user
                    // row (single reads, never a poll): the display plus the
                    // source Space the send follows away from. A row that is
                    // unreadable here refuses the submit before any post.
                    let row = last_snapshot.as_ref().and_then(|live| {
                        crate::workspace_select::user_row_from_rows(&live.rows, 0).ok()
                    });
                    // Admission trace, formatted before the submit consumes
                    // the row: pins the exact subject/target/source/op while
                    // the run lives. Emitted on the submitted path only.
                    let admitted = row.as_ref().map(|row| {
                        crate::workspace_send::format_send_admitted_line(
                            id,
                            claim.send_subject.as_deref().unwrap_or("none"),
                            claim.expected_sid.unwrap_or(0),
                            row.current,
                            &row.display,
                            claim.operation_id.as_deref().unwrap_or("none"),
                            claim.topology_revision,
                            crate::input_eligibility::now_ms(),
                        )
                    });
                    let submit = match row {
                        Some(row) => {
                            let handle_clone = handle.clone();
                            send_controller.submit(&claim, &row.display, row.current, move || {
                                crate::workspace_send_native::LiveManagedSendBackend::new(
                                    crate::workspace_send_native::OsSendReaders::live(handle_clone),
                                )
                            })
                        }
                        None => {
                            Err("product send refuses: fresh row unreadable at submit".to_owned())
                        }
                    };
                    handle.release_select_after_drain();
                    match submit {
                        Ok(()) => {
                            eprintln!(
                                "{}",
                                admitted
                                    .as_deref()
                                    .unwrap_or("tiler-macos send admitted missing-context")
                            );
                        }
                        Err(reason) => {
                            eprintln!(
                                "tiler-macos dev send action={} refused reason={} complete=false",
                                id, reason,
                            );
                            if let Some(operation_id) = operation_id.as_deref() {
                                // Submit-time refusals never actuated (no
                                // post performed): stage `Refused` with no
                                // reads so the transfer holds without
                                // actuation — never a verdict. The follow
                                // view (if the reply published one) stages
                                // under the claim's own bound identity.
                                handle.report_send_outcome(
                                    operation_id,
                                    tiler_core::world_report::FrameSetter::Refused,
                                    None,
                                    None,
                                    None,
                                    None,
                                    claim.select_baseline_sid,
                                    claim.send_follow_view_operation.as_deref(),
                                );
                            }
                        }
                    }
                } else {
                    // Product selects ride the serialized Dock-swipe
                    // selection owner: this thread only submits nonblocking
                    // (never waits) and reconciles the completion on a
                    // later tick above. The core owns previous history and
                    // targeting; the owner holds only trailing creation
                    // proofs and reads fresh gates itself immediately
                    // before effecting; this side only pins the plain-data
                    // display legs. Proof adopts from the completion on
                    // BOTH outcomes.
                    let row = last_snapshot.as_ref().and_then(|live| {
                        crate::workspace_select::user_row_from_rows(&live.rows, 0).ok()
                    });
                    let published = handle.published_display_inputs();
                    let wake = handle.select_wake();
                    let submit = match (row, published, wake) {
                        (Some(_row), Some(published), Some(wake))
                            if published.displays.len() == 1 =>
                        {
                            match (
                                published.displays[0].uuid.clone(),
                                published.displays[0].cg_id,
                            ) {
                                (Some(uuid), cgid) if cgid != 0 => select_controller.submit(
                                    &claim,
                                    crate::select_owner::SelectSubmit {
                                        display_index: 0,
                                        pinned_cgid: cgid,
                                        pinned_uuid: &uuid,
                                        published_displays: &published.displays,
                                        published_separate: published.separate,
                                        trailing_enabled:
                                            crate::workspace_input::TRAILING_CAPABILITY_ENABLED,
                                        wake: &wake,
                                    },
                                ),
                                _ => Err(
                                    "product select refuses: published display legs unpinned"
                                        .to_owned(),
                                ),
                            }
                        }
                        _ => Err(
                            "product select refuses: fresh row, display legs, or select wake unpublished"
                                .to_owned(),
                        ),
                    };
                    handle.release_select_after_drain();
                    match submit {
                        Ok(()) => {
                            eprintln!(
                                "tiler-macos dev select action={} submitted route=dock-swipe complete=false",
                                id,
                            );
                        }
                        Err(reason) => {
                            eprintln!(
                                "tiler-macos dev select action={} refused reason={} complete=false",
                                id, reason,
                            );
                            if let Some(operation_id) = operation_id.as_deref() {
                                // Submit-time refusals never actuated (the
                                // owner never took the job): proven refusal
                                // with no reads settles without actuation.
                                use tiler_core::world_report::FrameSetter;
                                handle.report_select_attempt(
                                    operation_id,
                                    FrameSetter::Refused,
                                    None,
                                    None,
                                    None,
                                );
                            }
                        }
                    }
                }
            }

            // Indicator click drain against the cadence's fresh user row and
            // revision: rebuilt UI, moved topology, moved rows, and
            // past-nine positions all refuse before any backend touch, and
            // the rest submit through the guarded engine lane with their
            // immutable binding (same gates, same verified arrival - never
            // a stale index select). Diagnostic-only trailing-click
            // injection (single-shot): the actual rendered trailing
            // binding loops back through the click channel ahead of this
            // drain, which still fences it fresh below.
            if request.diag_trailing_click && !diag_trailing_injected {
                match indicator.diag_inject_rendered_trailing() {
                    Ok(line) => {
                        eprintln!("{line}");
                        diag_trailing_injected = true;
                    }
                    Err(_) => {
                        // No live trailing slot yet (or the channel is
                        // full): retry next tick, still exactly one
                        // injection per run. Silence keeps the cadence
                        // log bounded; the render lines carry the slot
                        // evidence meanwhile.
                    }
                }
            }
            if indicator.armed()
                && let Some(live) = last_snapshot.as_ref()
                && let Ok(row) = crate::workspace_select::user_row_from_rows(&live.rows, 0)
            {
                indicator.drain_clicks(&row, live.revision, &handle);
            }

            // Discovery: running scoped apps bind launchDate/exe/bundle and
            // launch async (never blocking AppKit); vanished apps terminate
            // without blocking; a launch-date change relaunches the
            // lifetime. No titles, no exe paths in any log.
            let mut live: BTreeSet<u32> = BTreeSet::new();
            for (pid_i32, bundle) in crate::ffi::running_apps() {
                if pid_i32 <= 1 {
                    continue;
                }
                let pid = pid_i32 as u32;
                if !selection_launchable(&selection, pid, &bundle) {
                    continue;
                }
                live.insert(pid);
                let launch_date = crate::ffi::ns_app_identity_for_pid(pid_i32).launch_date;
                let drifted = tracked
                    .get(&pid)
                    .map(|t| t.launch_date != launch_date)
                    .unwrap_or(true);
                if !drifted {
                    continue;
                }
                if tracked.contains_key(&pid) {
                    handle.terminate_app(pid);
                    facts_store.invalidate_pid(pid);
                    border_store.invalidate_pid(pid);
                    terminated += 1;
                }
                match handle.launch_app_async(pid) {
                    Ok(_) => {
                        tracked.insert(pid, TrackedApp { launch_date });
                        launched += 1;
                    }
                    Err(_) => launch_refused += 1,
                }
            }
            for pid in tracked
                .keys()
                .copied()
                .collect::<Vec<_>>()
                .into_iter()
                .filter(|pid| !live.contains(pid))
            {
                handle.terminate_app(pid);
                facts_store.invalidate_pid(pid);
                border_store.invalidate_pid(pid);
                tracked.remove(&pid);
                terminated += 1;
            }

            // Active border: join the frontmost PID's owner-published
            // facts with the live verdict and drive the one main-thread
            // carrier. Change-only and silent on steady state (no polling
            // threads: owner notifications plus the ~1 s backstop refresh
            // the records this reads; the 250 ms cadence only re-joins).
            border_tick_once(
                &selection,
                &border_store,
                &facts_store,
                &mut border_controller,
                &mut border_carrier,
                &request.border,
                last_snapshot.as_ref(),
                &handle,
            );
            // Meta+left drag preview (armed routes only): drain tap
            // batches, fence, one Engine contact, filled carrier.
            if let Some(drag) = drag_state.as_mut() {
                crate::drag_session::drag_turn_once(
                    &selection,
                    &border_store,
                    &facts_store,
                    drag,
                    last_snapshot.as_ref(),
                    &handle,
                );
            }
        }

        // Worker readiness at teardown (main thread never waited on it).
        let mut workers_ready = 0u64;
        let mut workers_failed = 0u64;
        for pid in tracked.keys() {
            match handle.worker_ready(*pid) {
                WorkerReadiness::Ready => workers_ready += 1,
                WorkerReadiness::Failed(_) => workers_failed += 1,
                WorkerReadiness::Pending | WorkerReadiness::UnknownPid => {}
            }
        }
        let (gated_ms, input_refusal, tap_report) = match &input {
            InputGuard::Tap(factory) => (
                factory.gated_ms(),
                early_refusal.clone().or_else(|| factory.refusal()),
                Some(factory.latest_report()),
            ),
            InputGuard::Off(_) => (0, early_refusal.clone(), None),
        };
        // Drag-preview teardown: disarm the Escape watch, stop exactly
        // the owned tap thread (own flag plus join), and publish its
        // final report. A tap-thread refusal parks the arm with its
        // reason (terminal evidence, never a silent loss). The carrier
        // destroys through its guard `Drop` at scope end.
        drag_escape_armed.store(false, std::sync::atomic::Ordering::SeqCst);
        let (drag_report, drag_previews, drag_drops, drag_cancels) = match drag_state.as_mut() {
            Some(drag) => {
                let (report, refusal) = drag.stop_owner();
                if let Some(reason) = refusal {
                    drag_parked = Some(reason);
                }
                (Some(report), drag.previews, drag.drops, drag.cancels)
            }
            None => (None, 0, 0, 0),
        };
        // Bounded shutdown independent of blocked AX: only proven exits
        // join; the rest retain for reap. Windows stay in place - nothing
        // restores geometry on any path. The independent stop-file watcher
        // was installed before runtime startup; normal disarm is one slice.
        // Indicator teardown first: remove the status item explicitly (the
        // guard `Drop` below is the backstop); queued clicks were already
        // refused on every gate above.
        indicator.teardown();
        let (indicator_updates, indicator_clicks) = (indicator.updates(), indicator.clicks());
        // Live-select cooperative shutdown drain: new admissions already
        // stopped with the loop exit above - no claim is submitted past
        // this point. An already-staged completion settles inline exactly
        // like the live poll; otherwise the inflight job is cancelled and
        // the main thread keeps pumping its runloop while polling
        // nonblocking until the terminal completion stages or the bound
        // expires. The proof adopts exactly like the live poll on BOTH
        // outcomes, so a late append is never silently lost. Only then is
        // the owner stop-signalled and reaped once finished, and only then
        // does the preexisting main-thread trailing cleanup below run.
        // Runs before the send shutdown so lane teardown order is
        // deterministic. The bound reuses `SHUTDOWN_BOUND`: a cancelled job
        // settles promptly through its latch, but a wedged owner (hung
        // WindowServer/AX leg inside the off-main MC create) must never
        // stall the main thread here - expiry reports unclean with the
        // retained ownership, never clean. Residual bound (no exact hard
        // lifetime claimed beyond the named constants): this drain spends
        // at most `SHUTDOWN_BOUND`, after which trailing cleanup (at most
        // `TRAILING_CLEANUP_BUDGET_SECS`) plus the runtime shutdown
        // (`SHUTDOWN_BOUND`) still follow - a fully wedged shutdown can
        // therefore overrun the watchdog grace into its forced dirty exit,
        // which stays honest (never a success claim). The common path (no
        // owned SIDs, prompt cancel) settles in milliseconds.
        let drain_deadline = Instant::now() + SHUTDOWN_BOUND;
        let mut select_unclean: Option<String> = None;
        if let Some((completion, reaped)) = select_controller.poll() {
            settle_select_at_shutdown(&handle, &mut trailing_lifecycle, completion, reaped);
        }
        if select_controller.is_busy() {
            select_controller.cancel();
            while select_controller.is_busy() && Instant::now() < drain_deadline {
                pump_once(&app);
                if let Some((completion, reaped)) = select_controller.poll() {
                    settle_select_at_shutdown(&handle, &mut trailing_lifecycle, completion, reaped);
                }
            }
            if select_controller.is_busy() {
                select_unclean = Some(
                    "select drain expired with an inflight job; terminal proof unadopted"
                        .to_owned(),
                );
            }
        }
        select_controller.request_shutdown();
        while !select_controller.is_finished() && Instant::now() < drain_deadline {
            pump_once(&app);
        }
        match select_controller.try_reap() {
            Some(_) => {}
            None => {
                select_unclean =
                    Some("select owner still live at the drain bound; lane retained".to_owned());
            }
        }
        if let Some(reason) = select_unclean.as_ref() {
            eprintln!(
                "tiler-macos dev select shutdown unclean reason={reason} owned={:?} complete=false",
                trailing_lifecycle.owned(),
            );
        }
        // Live-send terminal shutdown (nonblocking): take a terminal
        // completion poll first, then stop-request, finished-only reap, and
        // retention of an unfinished exact owner for eventual reap. A
        // dropped inflight settles honestly (possibly-executed when the
        // owner may have actuated, proven-refused otherwise) so the core
        // never retains a transfer the drain abandoned. Runs before the
        // runtime shutdown so the feedback still enqueues.
        if let Some(outcome) = send_controller.shutdown() {
            match outcome {
                crate::workspace_send::ShutdownOutcome::Settle(settle) => {
                    let outcome = crate::workspace_send::send_drain_outcome_for(settle.outcome);
                    // Core owns select history, so no follow-history notes
                    // at shutdown (or live): the retained previous-toggle
                    // resolves core-side. Terminal trailing creation adopts
                    // here too (same ownership rule as the live settle: the
                    // Space persists).
                    if let Some(proof) = settle.trailing.as_ref()
                        && let Some(created) = proof.created
                        && let Err(reason) = trailing_lifecycle.adopt_created(created)
                    {
                        eprintln!(
                            "tiler-macos dev trailing adopt sid={created} refused reason={reason} complete=true",
                        );
                    }
                    eprintln!(
                        "tiler-macos send settled at shutdown target={} outcome={:?} complete=true",
                        settle.target.target_sid(),
                        outcome,
                    );
                    handle.report_send_outcome(
                        settle.target.operation_id(),
                        settle.setter,
                        settle.pre_member,
                        settle.post_member,
                        settle.pre_view,
                        settle.post_view,
                        settle.baseline,
                        settle.follow_view_operation.as_deref(),
                    );
                }
                crate::workspace_send::ShutdownOutcome::TrailingRefusal(refusal) => {
                    // Preparation never bound a target: feedback-only
                    // settlement under the operation identity, no history.
                    eprintln!(
                        "tiler-macos send trailing refused at shutdown setter={:?} complete=true",
                        refusal.setter,
                    );
                    handle.report_send_outcome(
                        &refusal.operation_id,
                        refusal.setter,
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                    );
                }
            }
        }
        // Trailing shutdown cleanup (bounded, main thread only): each
        // removal re-proves fresh - one fresh native row plus fresh
        // per-SID emptiness proof per iteration, then at most one verified
        // removal via the verified remover (which itself re-verifies
        // emptiness and row identity per SID) before forgetting it. A row
        // that moved under an earlier removal re-plans instead of removing
        // under a stale plan. Never replays uncertain removal: occupied,
        // current-owned, unknown-emptiness, absent, and below-floor SIDs all
        // report residue and stay owned; a failed removal logs residue and
        // stops (budget or drift will not clear on retry in the same
        // shutdown). No MC effects run here when the fresh row is
        // unreadable. Runs before the runtime shutdown so no engine/AppKit
        // work is in flight. Gated on a clean drained/reaped selector (see
        // `trailing_cleanup_decision`): an unclean drain retains every
        // owned SID as explicit residue and skips ALL cleanup effects, so
        // a still-live (possibly hung MC-create) owner never overlaps the
        // remover below.
        if !trailing_lifecycle.owned().is_empty() {
            match trailing_cleanup_decision(select_unclean.as_deref(), trailing_lifecycle.owned()) {
                Some(residue) => {
                    for line in residue {
                        eprintln!("{line}");
                    }
                }
                None => {
                    let mut budget = crate::mc_probe::ActionBudget::new(
                        crate::workspace_select::TRAILING_CLEANUP_BUDGET_SECS,
                    );
                    let mut dispatched = false;
                    for _ in 0..trailing_lifecycle.owned().len() {
                        let row = match snapshot_live() {
                            Ok(snapshot) => {
                                match crate::workspace_select::user_row_from_rows(&snapshot.rows, 0)
                                {
                                    Ok(row) => row,
                                    Err(reason) => {
                                        eprintln!(
                                            "tiler-macos dev trailing cleanup skipped reason={} complete=true",
                                            reason,
                                        );
                                        break;
                                    }
                                }
                            }
                            Err(reason) => {
                                eprintln!(
                                    "tiler-macos dev trailing cleanup skipped reason={} complete=true",
                                    reason,
                                );
                                break;
                            }
                        };
                        let (removable, residue) =
                            trailing_lifecycle.cleanup_plan(&row.spaces, row.current, &|sid| {
                                crate::workspace_select::trailing_emptiness(sid, 0)
                            });
                        for sid in &residue {
                            eprintln!(
                                "tiler-macos dev trailing residue sid={} reason={} complete=true",
                                sid.0, sid.1,
                            );
                        }
                        let Some(sid) = removable.into_iter().next() else {
                            break;
                        };
                        match crate::mc_lifecycle::remove_sid_verified(
                            sid,
                            &mut budget,
                            false,
                            &mut dispatched,
                        ) {
                            Ok(_) => {
                                let _ = trailing_lifecycle.release_removed(sid);
                                eprintln!(
                                    "tiler-macos dev trailing removed sid={} complete=true",
                                    sid,
                                );
                            }
                            Err(reason) => {
                                eprintln!(
                                    "tiler-macos dev trailing residue sid={} reason={} complete=true",
                                    sid, reason,
                                );
                                break;
                            }
                        }
                    }
                }
            }
        }
        let report = handle.shutdown(SHUTDOWN_BOUND);
        let diagnostics = handle.diagnostics();
        // The hard lifetime is met on bounded runs: cancel the watchdog
        // before reporting (unbounded runs armed none). The stop-file
        // watcher disarms here on the normal path; its forced exit only
        // fires when shutdown wedges past the bound plus grace (honest
        // dirty exit, never a success claim).
        if let Some(watchdog) = stop_watchdog.as_mut() {
            watchdog.disarm();
        }
        if let Some(watchdog) = watchdog.as_mut() {
            watchdog.disarm();
        }
        let outcome = if select_unclean.is_some() {
            "unclean-shutdown"
        } else if stopped {
            "stopped"
        } else if early_refusal.is_some() || input_refusal.is_some() {
            "input-refused"
        } else if report.outcome == crate::runtime::ShutdownOutcome::Clean {
            "completed"
        } else {
            "unclean-shutdown"
        };
        // Verified clean only with a clean bounded shutdown, no refusal,
        // and a clean select drain: the launcher parses this (never stderr
        // volume).
        let shutdown_clean = report.outcome == crate::runtime::ShutdownOutcome::Clean
            && input_refusal.is_none()
            && select_unclean.is_none();
        let body = TileOutcome {
            outcome: outcome.to_owned(),
            seconds: request.seconds,
            mode: mode_str(request.unscoped).to_owned(),
            unbounded,
            input: request.input,
            modifier: request.modifier.as_str().to_owned(),
            preset: request.preset.as_str().to_owned(),
            active_chords,
            scopes: request.scopes.clone(),
            excluded: request.excluded.clone(),
            launched,
            launch_refused,
            terminated,
            workers_ready,
            workers_failed,
            topology_updates,
            topology_failures,
            display_events,
            lock_stops,
            secure_active_seen,
            gated_ms,
            input_refusal,
            tap_report,
            drag_parked,
            drag_report,
            drag_previews,
            drag_drops,
            drag_cancels,
            shutdown_outcome: format!("{:?}", report.outcome),
            shutdown_joined: report.joined,
            shutdown_retained: report.retained,
            shutdown_clean,
            input_forwarded: diagnostics.input_forwarded,
            input_ignored: diagnostics.input_ignored,
            input_refused_no_focus: diagnostics.input_refused_no_focus,
            input_select_enqueued: diagnostics.input_select_enqueued,
            input_select_refused: diagnostics.input_select_refused,
            indicator_updates,
            indicator_clicks,
            observations_published: diagnostics.observations_published,
            observation_failures: diagnostics.observation_failures,
            border_shows: border_controller.shows(),
            border_updates: border_controller.updates(),
            border_hides: border_controller.hides(),
            border_errors: border_controller.carrier_errors(),
            border_apply_lag_max_ms: border_controller.apply_lag_max_ms(),
            border_apply_samples: border_controller.apply_lag_samples(),
            note: "stop leaves windows in place; no restoration performed".to_owned(),
        };
        let body = serde_json::to_string(&body).map_err(|e| (1, e.to_string()))?;
        if outcome == "completed" {
            Ok(body)
        } else {
            Err((1, body))
        }
    }

    #[allow(dead_code)]
    fn gate_for_test(
        secure_active: Option<bool>,
        frontmost: Option<(i32, String)>,
        session: crate::ffi::SessionLock,
    ) -> Option<&'static str> {
        interception_gate(secure_active, frontmost, session)
    }

    #[cfg(test)]
    mod watchdog_tests {
        use super::*;

        #[test]
        fn limit_is_startup_plus_soft_plus_teardown_plus_grace() {
            assert_eq!(
                watchdog_limit(
                    Duration::from_secs(5),
                    30,
                    Duration::from_secs(5),
                    Duration::from_secs(30)
                ),
                Duration::from_secs(70)
            );
            assert_eq!(
                watchdog_limit(Duration::from_secs(5), 1, SHUTDOWN_BOUND, WATCHDOG_GRACE),
                Duration::from_secs(5) + Duration::from_secs(1) + SHUTDOWN_BOUND + WATCHDOG_GRACE
            );
        }

        #[test]
        fn stop_limit_is_teardown_plus_modest_grace() {
            assert_eq!(
                stop_limit(SHUTDOWN_BOUND, STOP_WATCHDOG_GRACE),
                SHUTDOWN_BOUND + STOP_WATCHDOG_GRACE
            );
            assert_eq!(
                stop_limit(Duration::from_secs(5), Duration::from_secs(10)),
                Duration::from_secs(15)
            );
        }

        #[test]
        fn disarm_cancels_before_the_limit() {
            // Armed long, disarmed at once: the waiter returns without
            // exiting (join proves the thread is gone; the test process
            // survives to assert).
            let mut watchdog = Watchdog::arm(Duration::from_secs(60)).expect("arm");
            watchdog.disarm();
        }

        #[test]
        fn drop_disarms_without_explicit_call() {
            // Early-error shape: no explicit disarm, just scope exit. The
            // `Drop` backstop cancels the waiter (no delayed forced exit
            // survives the run).
            {
                let _watchdog = Watchdog::arm(Duration::from_secs(60)).expect("arm");
            }
        }
    }

    #[cfg(test)]
    mod border_placement_tests {
        use super::*;
        use crate::runtime_types::DesiredFrame;

        fn desired(window: &str) -> DesiredFrame {
            DesiredFrame {
                window: window.to_owned(),
                x: 0,
                y: 0,
                w: 100,
                h: 100,
            }
        }

        #[test]
        fn engine_places_joins_exact_pid_and_wid() {
            // Plan truth names exactly (4242, 7): same wid under another
            // PID, another wid under the same PID, and malformed IDs all
            // read unplaced.
            let plan = vec![desired("4242_7"), desired("4242_9"), desired("nope")];
            assert!(engine_places(&plan, 4242, 7));
            assert!(engine_places(&plan, 4242, 9));
            assert!(!engine_places(&plan, 9999, 7));
            assert!(!engine_places(&plan, 4242, 77));
            assert!(!engine_places(&[], 4242, 7));
        }

        #[test]
        fn display_publish_gate_covers_notify_recover_and_quiet() {
            // Notification republishes latest-wins even over a full slot.
            assert!(display_publish_due(true, true));
            assert!(display_publish_due(true, false));
            // An emptied slot (failed startup read, transient unreadable
            // snapshot) recovers on the next good snapshot unnotified.
            assert!(display_publish_due(false, false));
            // A quiet poll with a populated slot leaves it untouched.
            assert!(!display_publish_due(false, true));
        }

        #[test]
        fn border_fast_gate_needs_enabled_dirty_and_throttle() {
            // Disabled runs never join (but the peek/take protocol still
            // clears elsewhere - this pure gate only answers).
            assert!(!border_fast_due(false, true, Duration::from_secs(10)));
            assert!(!border_fast_due(true, false, Duration::from_secs(10)));
            // Throttle: a fresh burst waits out the minimum interval.
            assert!(!border_fast_due(true, true, Duration::from_millis(0)));
            assert!(!border_fast_due(true, true, Duration::from_millis(32)));
            assert!(border_fast_due(true, true, Duration::from_millis(33)));
            assert!(border_fast_due(true, true, Duration::from_secs(10)));
        }

        #[test]
        fn cadence_gate_runs_early_only_on_a_stashed_event() {
            // Idle pumps stay quiet: a fresh elapsed time with no stashed
            // event never runs the body, so the 250 ms cadence stays the
            // idle backstop.
            assert!(!cadence_due(Duration::from_millis(0), false));
            assert!(!cadence_due(Duration::from_millis(50), false));
            assert!(!cadence_due(Duration::from_millis(249), false));
            // A stashed display/Space notification runs the body early
            // (topology plus the completion polls settle on the event).
            assert!(cadence_due(Duration::from_millis(0), true));
            assert!(cadence_due(Duration::from_millis(50), true));
            // The cadence itself still fires with no event.
            assert!(cadence_due(Duration::from_millis(250), false));
            assert!(cadence_due(Duration::from_secs(10), false));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(scopes: &[&str]) -> TileRequest {
        TileRequest {
            user_start: true,
            scopes: scopes.iter().map(|s| s.to_string()).collect(),
            seconds: 30,
            input: TileInput::Tap,
            modifier: ProbeModifier::Meta,
            preset: InputPreset::Authentic,
            unscoped: false,
            excluded: Vec::new(),
            log: TileLogLevel::Normal,
            run_dir: None,
            border: ActiveBorderOptions::default(),
            drag: crate::drag_drop::DragOptions::default(),
            indicator: crate::workspace_indicator::IndicatorOptions::default(),
            diag_stay: None,
            diag_trailing_click: false,
        }
    }

    fn unscoped_request(seconds: u64, excluded: &[&str]) -> TileRequest {
        TileRequest {
            user_start: true,
            scopes: Vec::new(),
            seconds,
            input: TileInput::Tap,
            modifier: ProbeModifier::Ctrl,
            preset: InputPreset::Authentic,
            unscoped: true,
            excluded: excluded.iter().map(|s| s.to_string()).collect(),
            log: TileLogLevel::Normal,
            run_dir: None,
            border: ActiveBorderOptions::default(),
            drag: crate::drag_drop::DragOptions::default(),
            indicator: crate::workspace_indicator::IndicatorOptions::default(),
            diag_stay: None,
            diag_trailing_click: false,
        }
    }

    #[test]
    fn log_level_parses_and_defaults_to_normal() {
        assert_eq!(TileLogLevel::parse("normal"), Ok(TileLogLevel::Normal));
        assert_eq!(TileLogLevel::parse("Verbose"), Ok(TileLogLevel::Verbose));
        assert_eq!(TileLogLevel::parse("TRACE"), Ok(TileLogLevel::Trace));
        assert!(TileLogLevel::parse("debug").is_err());
        assert_eq!(TileLogLevel::default(), TileLogLevel::Normal);
        assert_eq!(TileLogLevel::Trace.as_str(), "trace");
        // Heartbeat strides: normal off, verbose ~10 s, trace ~2 s.
        assert_eq!(heartbeat_stride(TileLogLevel::Normal), None);
        assert_eq!(heartbeat_stride(TileLogLevel::Verbose), Some(40));
        assert_eq!(heartbeat_stride(TileLogLevel::Trace), Some(8));
    }

    #[test]
    fn heartbeat_state_detects_every_printed_counter_move() {
        // Change-only gate: identical snapshots stay quiet, any printed
        // counter move (including one input forward) reprints. Verbose and
        // trace arms never compare equal (trace carries the input pair).
        let base = HeartbeatState {
            launched: 40,
            terminated: 0,
            topo_updates: 1,
            topo_failures: 0,
            lock_stops: 0,
            secure_seen: 0,
            input: Some((0, 0)),
        };
        assert_eq!(base, base);
        let mut moved = base;
        moved.input = Some((1, 1));
        assert_ne!(base, moved);
        for distinct in [
            HeartbeatState {
                launched: 41,
                ..base
            },
            HeartbeatState {
                terminated: 1,
                ..base
            },
            HeartbeatState {
                topo_updates: 2,
                ..base
            },
            HeartbeatState {
                topo_failures: 1,
                ..base
            },
            HeartbeatState {
                lock_stops: 1,
                ..base
            },
            HeartbeatState {
                secure_seen: 1,
                ..base
            },
            HeartbeatState {
                input: None,
                ..base
            },
            HeartbeatState {
                input: Some((0, 1)),
                ..base
            },
        ] {
            assert_ne!(base, distinct, "{distinct:?} must reprint");
        }
    }

    #[test]
    fn input_mode_parses_exactly() {
        assert_eq!(TileInput::parse("tap"), Ok(TileInput::Tap));
        assert_eq!(TileInput::parse("off"), Ok(TileInput::Off));
        assert!(TileInput::parse("auto").is_err());
        assert!(TileInput::parse("").is_err());
        // Carbon stays unparsed (no register probe, no product selection).
        assert!(TileInput::parse("carbon").is_err());
        assert_eq!(TileInput::default(), TileInput::Tap);
    }

    #[test]
    fn validation_requires_start_scope_and_finite_lifetime() {
        // Missing start flag refuses.
        let mut r = request(&["com.apple.TextEdit"]);
        r.user_start = false;
        assert!(r.validate().is_err());
        // Empty scope refuses.
        assert!(request(&[]).validate().is_err());
        // Unbounded lifetimes refuse (zero and over the probe max).
        let mut r = request(&["com.apple.TextEdit"]);
        r.seconds = 0;
        assert!(r.validate().is_err());
        r.seconds = MAX_TILE_SECONDS + 1;
        assert!(r.validate().is_err());
        r.seconds = MAX_TILE_SECONDS;
        assert!(r.validate().is_ok());
        // Named everyday scopes validate.
        assert!(
            request(&["com.apple.TextEdit", "com.apple.Calculator"])
                .validate()
                .is_ok()
        );
    }

    #[test]
    fn validation_rejects_ghostty_case_variants_and_own_bundle() {
        for scope in [
            "com.mitchellh.ghostty",
            "COM.MITCHELLH.GHOSTTY",
            "com.mitchellh.ghostty-beta",
            crate::BUNDLE_ID,
            "",
        ] {
            assert!(request(&[scope]).validate().is_err(), "{scope}");
        }
    }

    #[test]
    fn diag_stay_binding_carries_fixed_shift_alt_digit_stay() {
        // Fixed diagnostic shape: Desktop 1 and 2 only, digit keycode with
        // the Shift+Alt arms, stay (follow: false) action. Everything else
        // has no binding.
        for index in [1u8, 2] {
            let binding = diag_stay_binding(index).unwrap();
            assert_eq!(
                binding.key_code,
                crate::desktop_shortcuts::desktop_digit_keycode(u32::from(index)).unwrap(),
                "index {index}"
            );
            assert!((binding.shift, binding.alt, binding.aux) == (true, true, false));
            assert_eq!(
                binding.action,
                crate::runtime_types::InputSemanticAction::WorkspaceSend {
                    index,
                    follow: false
                },
                "index {index}"
            );
            assert!(binding.validate().is_ok());
        }
        for index in [0u8, 3, 9, 10, 255] {
            assert!(diag_stay_binding(index).is_none(), "index {index}");
        }
    }

    #[test]
    fn diag_stay_chord_is_duplicate_free_in_effective_catalogue() {
        // The Shift+Alt+digit shape shares no effective chord with the
        // product table: digits only carry the plain select and Shifted
        // send shapes. Pinned under both presets for the Ctrl role (the
        // live harness role), and the config validator admits the combined
        // table.
        for preset in [InputPreset::Authentic, InputPreset::Compatible] {
            let catalogue = crate::input_policy::effective_bindings(
                preset,
                ProbeModifier::Ctrl,
                &product_catalogue(),
            );
            for index in [1u8, 2] {
                let binding = diag_stay_binding(index).unwrap();
                let want =
                    crate::workspace_input::effective_flags_for(ProbeModifier::Ctrl, &binding);
                assert!(
                    !catalogue.iter().any(|existing| {
                        existing.key_code == binding.key_code
                            && crate::workspace_input::effective_flags_for(
                                ProbeModifier::Ctrl,
                                existing,
                            ) == want
                    }),
                    "{preset:?} index {index}"
                );
                let mut combined = catalogue.clone();
                combined.push(binding);
                assert!(
                    crate::runtime_types::RuntimeConfig {
                        tile_bindings: Some(combined),
                        ..crate::runtime_types::RuntimeConfig::default()
                    }
                    .validate()
                    .is_ok(),
                    "{preset:?} index {index}"
                );
            }
        }
        // The Alt Ctrl-role collapses the Shift arm onto the send shape
        // (same key, same effective flags): the validator refuses there, so
        // the harness stays on roles where the chord is provably free.
        let (alt_base, _) = crate::input::cg_flag_bits(ProbeModifier::Alt);
        let diag_flags =
            alt_base | crate::native_input::CG_SHIFT_BIT | crate::native_input::CG_ALT_BIT;
        let send_flags = alt_base | crate::native_input::CG_SHIFT_BIT;
        assert_eq!(
            diag_flags, send_flags,
            "Alt-role collapse documents the refusal"
        );
    }

    #[test]
    fn diag_stay_validate_gates_textedit_tap_only() {
        // Valid harness: TextEdit-only scoped tap run with a finite
        // lifetime under the Ctrl role.
        let mut r = request(&["com.apple.TextEdit"]);
        r.modifier = ProbeModifier::Ctrl;
        r.diag_stay = Some(2);
        assert!(r.validate().is_ok());
        // Index 1 validates too (the inverse leg); anything else refuses.
        r.diag_stay = Some(1);
        assert!(r.validate().is_ok());
        for index in [0u8, 3, 9] {
            r.diag_stay = Some(index);
            assert!(r.validate().is_err(), "index {index}");
        }
        // Non-tap routes refuse (no off-route injection).
        r.diag_stay = Some(2);
        r.input = TileInput::Off;
        assert!(r.validate().is_err());
        r.input = TileInput::Tap;
        // Unscoped refuses (no ordinary-app input authority).
        r.unscoped = true;
        assert!(r.validate().is_err());
        r.unscoped = false;
        // Non-TextEdit and multi scopes refuse (TextEdit-only).
        r.scopes = vec!["com.apple.Calculator".to_owned()];
        assert!(r.validate().is_err());
        r.scopes = vec![
            "com.apple.TextEdit".to_owned(),
            "com.apple.Calculator".to_owned(),
        ];
        assert!(r.validate().is_err());
        r.scopes = vec!["com.apple.TextEdit".to_owned()];
        assert!(r.validate().is_ok());
        // The Alt Ctrl-role collides with the send shape (same effective
        // flags): refused as a duplicate, never silently rebound.
        r.modifier = ProbeModifier::Alt;
        assert!(r.validate().is_err());
        r.modifier = ProbeModifier::Ctrl;
        assert!(r.validate().is_ok());
        // Absent injection leaves every existing validation unchanged.
        r.diag_stay = None;
        assert!(r.validate().is_ok());
    }

    #[test]
    fn diag_trailing_click_validate_gates_textedit_tap_only() {
        // Valid harness: TextEdit-only scoped tap run with a finite
        // lifetime (no chord involved, so any preset/modifier validates).
        let mut r = request(&["com.apple.TextEdit"]);
        r.diag_trailing_click = true;
        assert!(r.validate().is_ok());
        // Non-tap routes refuse (no off-route injection).
        r.input = TileInput::Off;
        assert!(r.validate().is_err());
        r.input = TileInput::Tap;
        // Unscoped refuses (no ordinary-app input authority).
        r.unscoped = true;
        assert!(r.validate().is_err());
        r.unscoped = false;
        // Non-TextEdit and multi scopes refuse (TextEdit-only).
        r.scopes = vec!["com.apple.Calculator".to_owned()];
        assert!(r.validate().is_err());
        r.scopes = vec![
            "com.apple.TextEdit".to_owned(),
            "com.apple.Calculator".to_owned(),
        ];
        assert!(r.validate().is_err());
        r.scopes = vec!["com.apple.TextEdit".to_owned()];
        assert!(r.validate().is_ok());
        // Absent injection leaves every existing validation unchanged.
        r.diag_trailing_click = false;
        assert!(r.validate().is_ok());
    }

    #[test]
    fn unscoped_validates_without_scope_and_admits_ghostty_shape() {
        // No allowlist needed; Ghostty-shaped scopes are meaningless here
        // (unscoped admits Ghostty by rule) and --scope is refused.
        let r = unscoped_request(30, &[]);
        assert!(r.validate().is_ok());
        assert!(!r.is_unbounded());
        assert_eq!(r.selection(), TileScope::Unscoped { excluded: vec![] });
        // --scope with --unscoped refuses (scoped stays allowlist-only).
        let mut both = unscoped_request(30, &[]);
        both.scopes = vec!["com.apple.TextEdit".to_owned()];
        assert!(both.validate().is_err());
        // --exclude without --unscoped refuses.
        let mut excluded_scoped = request(&["com.apple.TextEdit"]);
        excluded_scoped.excluded = vec!["com.example.skip".to_owned()];
        assert!(excluded_scoped.validate().is_err());
        // Unscoped exclusions validate; empty entries refuse.
        assert!(
            unscoped_request(30, &["com.example.skip"])
                .validate()
                .is_ok()
        );
        let mut empty_exclusion = unscoped_request(30, &[]);
        empty_exclusion.excluded = vec![String::new()];
        assert!(empty_exclusion.validate().is_err());
        // Unscoped lifetimes: 0 means unbounded, over-max refuses.
        assert!(unscoped_request(0, &[]).validate().is_ok());
        assert!(unscoped_request(0, &[]).is_unbounded());
        let mut over = unscoped_request(MAX_TILE_SECONDS + 1, &[]);
        assert!(over.validate().is_err());
        over.seconds = MAX_TILE_SECONDS;
        assert!(over.validate().is_ok());
        assert!(!over.is_unbounded());
        // Scoped stays finite: 0 refuses there.
        let mut scoped_zero = request(&["com.apple.TextEdit"]);
        scoped_zero.seconds = 0;
        assert!(scoped_zero.validate().is_err());
    }

    #[test]
    fn unscoped_outcome_carries_mode_and_exclusions() {
        let outcome = TileOutcome::refused(
            &unscoped_request(0, &["com.example.skip"]),
            "locked".to_owned(),
        );
        assert_eq!(outcome.mode, "unscoped");
        assert!(outcome.unbounded);
        assert_eq!(outcome.excluded, vec!["com.example.skip"]);
        let scoped = TileOutcome::refused(&request(&["com.apple.TextEdit"]), "locked".to_owned());
        assert_eq!(scoped.mode, "scoped");
        assert!(!scoped.unbounded);
        assert!(scoped.excluded.is_empty());
        // Refusals are never clean; the clean flag serializes for the
        // launcher to parse (stderr volume never decides).
        assert!(!outcome.shutdown_clean);
        let body = serde_json::to_string(&outcome).unwrap();
        assert!(body.contains("\"shutdown_clean\":false"));
    }

    #[test]
    fn run_dir_validates_and_persists_terminal_outcomes() {
        // Empty run dirs refuse at validation.
        let mut r = unscoped_request(30, &[]);
        r.run_dir = Some(String::new());
        assert!(r.validate().is_err());
        // A present directory round-trips the terminal body; absent dirs
        // and `None` never fail.
        let dir = std::env::temp_dir().join(format!("omnitiler-test-run-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("fixture dir");
        let dir_str = dir.to_string_lossy().into_owned();
        let mut r = unscoped_request(30, &[]);
        r.run_dir = Some(dir_str.clone());
        assert!(r.validate().is_ok());
        let body = serde_json::to_string(&TileOutcome::refused(&r, "locked".to_owned())).unwrap();
        write_run_outcome(&r.run_dir, &body);
        let read_back = std::fs::read_to_string(dir.join("outcome.json")).expect("outcome written");
        assert_eq!(read_back, body);
        std::fs::remove_dir_all(&dir).expect("fixture cleanup");
        // Missing directories skip silently (stdout stays primary).
        write_run_outcome(
            &Some("/nonexistent-omnitiler-run-dir-xyz".to_owned()),
            &body,
        );
        write_run_outcome(&None, &body);
    }

    #[test]
    fn launchable_never_admits_unknown_or_refused() {
        let scope = AppScope {
            allowed: vec!["com.apple.TextEdit".to_owned()],
        };
        assert!(launchable(&scope, 4242, "com.apple.TextEdit"));
        assert!(!launchable(&scope, 4242, ""), "unknown bundle never writer");
        assert!(!launchable(&scope, 4242, "com.apple.Calculator"));
        assert!(!launchable(&scope, 4242, "com.mitchellh.ghostty"));
        assert!(!launchable(&scope, 4242, crate::BUNDLE_ID));
        assert!(!launchable(&scope, 0, "com.apple.TextEdit"));
        assert!(!launchable(&scope, 1, "com.apple.TextEdit"));
        assert!(!launchable(
            &scope,
            std::process::id(),
            "com.apple.TextEdit"
        ));
    }

    #[test]
    fn refused_outcome_serializes_with_reason() {
        let outcome = TileOutcome::refused(&request(&["com.apple.TextEdit"]), "locked".to_owned());
        let body = serde_json::to_string(&outcome).unwrap();
        assert!(body.contains("\"outcome\":\"refused\""));
        assert!(body.contains("locked"));
    }

    #[test]
    fn unclean_select_drain_retains_every_owned_sid_without_cleanup() {
        // Decision path with real lifecycle ownership (never a fake
        // cleaner/effect): an unclean drain - the owner still live, e.g. a
        // hung MC-create owner that may still actuate - retains every owned
        // SID as explicit residue so the shutdown path skips ALL cleanup
        // effects; a clean drain proceeds to the preexisting cleanup loop.
        let mut lifecycle = crate::workspace_select::TrailingLifecycle::new(&[1001, 1002]);
        lifecycle.adopt_created(2001).expect("adopt");
        lifecycle.adopt_created(2002).expect("adopt");
        let owned = lifecycle.owned().to_vec();
        assert_eq!(owned, vec![2001, 2002]);
        let residue = trailing_cleanup_decision(
            Some("select owner still live at the drain bound; lane retained"),
            &owned,
        )
        .expect("unclean drain skips effects");
        assert_eq!(residue.len(), 2);
        for (sid, line) in owned.iter().zip(residue.iter()) {
            assert!(line.contains(&format!("sid={sid}")), "{line}");
            assert!(line.contains("retained without removal"), "{line}");
        }
        // Nothing released, nothing adopted: ownership untouched.
        assert_eq!(lifecycle.owned(), &[2001, 2002]);
        assert!(trailing_cleanup_decision(None, &owned).is_none());
        assert!(trailing_cleanup_decision(None, &[]).is_none());
    }
}

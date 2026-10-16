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
//! Safety contract: explicit `--user-start` plus a non-empty bundle
//! allowlist are mandatory. Ghostty (any case/variant), the own bundle,
//! protected terminals, and unknown (`None`) bundles never write — at
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

use crate::config::{MAX_PROBE_SECONDS, ProbeModifier};
use crate::dev_selection::{TileScope, launchable as selection_launchable};
use crate::input_policy::InputPreset;
use crate::scope::AppScope;

/// Default tile lifetime (seconds): finite and well under the probe max.
pub const DEFAULT_TILE_SECONDS: u64 = 30;
/// Tile lifetime bound (seconds): the configured probe max.
pub const MAX_TILE_SECONDS: u64 = MAX_PROBE_SECONDS;

/// Which input route the tile run arms. The event tap is the primary (and
/// now sole product) route with exact-chord pair suppression; off parks.
/// No auto fallback between them. Carbon stays available only as a probe
/// (`input-register`, `input-session --route carbon`), never as a product
/// selection.
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
            other => Err(format!(
                "unknown --input '{other}' (expected tap|off; carbon is probe-only: input-register, input-session --route carbon)"
            )),
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
#[derive(Debug, Clone, PartialEq, Eq)]
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
        // Defensive: a preset must never arm an empty chord set (Compatible
        // only removes known-conflicting arrows, so this is unreachable
        // today, but an empty tap would silently intercept nothing).
        if crate::input_policy::effective_bindings(
            self.preset,
            self.modifier,
            &crate::native_input::tile_catalogue(),
        )
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
    pub carbon_statuses: Vec<crate::ffi::CarbonChordStatus>,
    /// Latest continuous-tap counters (`None` for the off route): timeout
    /// re-enables, user-input notifications, and enabled-readback-only
    /// recoveries are first-class terminal evidence (cumulative across
    /// fresh-guard recreations).
    pub tap_report: Option<crate::ffi::TapSourceReport>,
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
    pub observations_published: u64,
    pub observation_failures: u64,
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
            carbon_statuses: Vec::new(),
            tap_report: None,
            shutdown_outcome: "not-started".to_owned(),
            shutdown_joined: 0,
            shutdown_retained: 0,
            shutdown_clean: false,
            input_forwarded: 0,
            input_ignored: 0,
            input_refused_no_focus: 0,
            observations_published: 0,
            observation_failures: 0,
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
mod native {
    use super::*;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use crate::input_policy::effective_bindings;
    use crate::native_input::{OffTileFactory, TapTileFactory, interception_gate, tile_catalogue};
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

    /// True when the launcher published a stop for this exact run: the
    /// `<run_dir>/stop-requested` file exists. Std-only polling (one stat
    /// per main-loop cadence); `None` run dirs never stop this way.
    fn stop_requested(run_dir: &Option<String>) -> bool {
        run_dir.as_ref().is_some_and(|dir| {
            !dir.is_empty() && std::path::Path::new(dir).join("stop-requested").exists()
        })
    }

    /// Publish the eligibility current context from a snapshot the main
    /// thread already took: every user-typed current Space (one per
    /// display — multi-display supported, each display homes its own
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
        // exist when passed — the launcher always creates it, so a missing
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
        // whole supported catalogue, Compatible leaves known conflicting
        // chords to macOS (unarmed, passing through natively).
        let catalogue = effective_bindings(request.preset, request.modifier, &tile_catalogue());
        let active_chords = catalogue.len();
        // Shared per-PID eligibility facts: each per-app AX owner publishes
        // only its own PID from snapshot data it already read; the input
        // owner consults plain snapshots (no AX crosses threads). Built
        // here before `Runtime::start` and shared with both factories —
        // no new threads, no new channels.
        let facts_store = Arc::new(crate::input_eligibility::EligibilityFactsStore::new());
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
                        f.with_scope(selection.clone())
                            .with_facts_store(Arc::clone(&facts_store))
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
            .map(|f| f.with_facts_store(Arc::clone(&facts_store)))
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
        let (_guard, display_flag) = crate::ffi::install_display_change_guard()
            .map_err(|reason| (1, format!("display guard refused: {reason}")))?;
        let mut poll = TopologyPoll::new();
        let mut tracked: BTreeMap<u32, TrackedApp> = BTreeMap::new();
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
            if last_cadence.elapsed() < CADENCE {
                continue;
            }
            last_cadence = Instant::now();
            cadence_count += 1;
            // Foreground verbosity: lifecycle/recovery lines always stream;
            // verbose adds a counter heartbeat, trace shortens it and adds
            // input forward/ignore counters. Static tokens plus numbers
            // only — never titles, bundles, or paths. Heartbeats are
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
                    for pid in tracked.keys().copied().collect::<Vec<_>>() {
                        handle.terminate_app(pid);
                        facts_store.invalidate_pid(pid);
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
                    for pid in tracked.keys().copied().collect::<Vec<_>>() {
                        handle.terminate_app(pid);
                        facts_store.invalidate_pid(pid);
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
            }

            // Topology: observer events plus bounded fallback polling both
            // funnel into resnapshot; revision drift publishes (purging
            // queued focus and superseding current plans).
            if display_flag.take_changed() {
                display_events += 1;
            }
            match snapshot_live() {
                Ok(snapshot) => {
                    if !matches!(
                        poll.poll(&snapshot),
                        crate::topology::PollOutcome::Unchanged
                    ) {
                        publish_snapshot_context(&facts_store, &snapshot);
                        handle.update_topology_with(snapshot, &|sid| {
                            crate::ffi::sls_space_type(sid).ok()
                        });
                        topology_updates += 1;
                    }
                }
                Err(_) => {
                    handle.invalidate_topology();
                    facts_store.invalidate_context();
                    poll = TopologyPoll::new();
                    topology_failures += 1;
                }
            }

            // Focus evidence: workspace frontmost only (plain IDs). The main
            // thread performs no AX reads under any cadence — a hung app
            // must never stall the controller — so `ax_focus` stays `None`
            // here: each owning per-app AX thread reads the bounded
            // selected-oracle leg on its own thread and stamps it at send
            // time (never the diagnostic system-wide leg). Stale foreground
            // out of scope clears observed focus engine-side via
            // disagreement — never guessed here.
            let frontmost = crate::ffi::frontmost_app();
            handle.update_focus_evidence(FocusEvidence {
                frontmost_pid: frontmost.as_ref().map(|(pid, _)| *pid as u32),
                frontmost_bundle: frontmost.map(|(_, bundle)| bundle),
                ax_focus: None,
            });

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
                tracked.remove(&pid);
                terminated += 1;
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
        let (gated_ms, input_refusal, carbon_statuses, tap_report) = match &input {
            InputGuard::Tap(factory) => (
                factory.gated_ms(),
                early_refusal.clone().or_else(|| factory.refusal()),
                Vec::new(),
                Some(factory.latest_report()),
            ),
            InputGuard::Off(_) => (0, early_refusal.clone(), Vec::new(), None),
        };
        // Bounded shutdown independent of blocked AX: only proven exits
        // join; the rest retain for reap. Windows stay in place — nothing
        // restores geometry on any path. The independent stop-file watcher
        // was installed before runtime startup; normal disarm is one slice.
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
        let outcome = if stopped {
            "stopped"
        } else if early_refusal.is_some() || input_refusal.is_some() {
            "input-refused"
        } else if report.outcome == crate::runtime::ShutdownOutcome::Clean {
            "completed"
        } else {
            "unclean-shutdown"
        };
        // Verified clean only with a clean bounded shutdown and no
        // refusal: the launcher parses this (never stderr volume).
        let shutdown_clean =
            report.outcome == crate::runtime::ShutdownOutcome::Clean && input_refusal.is_none();
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
            carbon_statuses,
            tap_report,
            shutdown_outcome: format!("{:?}", report.outcome),
            shutdown_joined: report.joined,
            shutdown_retained: report.retained,
            shutdown_clean,
            input_forwarded: diagnostics.input_forwarded,
            input_ignored: diagnostics.input_ignored,
            input_refused_no_focus: diagnostics.input_refused_no_focus,
            observations_published: diagnostics.observations_published,
            observation_failures: diagnostics.observation_failures,
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
        // Carbon is probe-only now (input-register, input-session
        // --route carbon), never a product tile selection.
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
}

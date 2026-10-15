#![forbid(unsafe_code)]

//! Scoped-tile development entry point (`tile --user-start ...`).
//!
//! A bounded, explicitly scoped, finite-lifetime tile session over named
//! everyday apps (e.g. TextEdit/Calculator class): the main (AppKit) thread
//! owns topology snapshots and lifecycle discovery, one dedicated input
//! thread owns its Carbon/tap guard, one per-app AX owner observes and
//! actuates under pre-effect fences, and one engine thread plans. No
//! additive core API: input resolves to the existing `Focus`/`Move`/
//! `ToggleFloat` commands, effects ride the existing scoped backend.
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

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::config::{MAX_PROBE_SECONDS, ProbeModifier};
use crate::scope::AppScope;

/// Default tile lifetime (seconds): finite and well under the probe max.
pub const DEFAULT_TILE_SECONDS: u64 = 30;
/// Tile lifetime bound (seconds): the configured probe max.
pub const MAX_TILE_SECONDS: u64 = MAX_PROBE_SECONDS;

/// Which input route the tile run arms. Exactly one arms per run: Carbon
/// discrete hotkeys by default, the event tap only under the explicit tap
/// flag, off parks. No auto fallback between them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TileInput {
    /// Discrete Carbon hotkeys, one exact chord each (default).
    #[default]
    Carbon,
    /// Continuous event tap with exact-chord pair suppression (explicit).
    Tap,
    /// No interception: open-loop on AX observations.
    Off,
}

impl TileInput {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s.to_ascii_lowercase().as_str() {
            "carbon" => Ok(Self::Carbon),
            "tap" => Ok(Self::Tap),
            "off" => Ok(Self::Off),
            other => Err(format!(
                "unknown --input '{other}' (expected carbon|tap|off)"
            )),
        }
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Carbon => "carbon",
            Self::Tap => "tap",
            Self::Off => "off",
        }
    }
}

/// Validated tile request: explicit user start plus a non-empty scope plus
/// a finite lifetime. Construction never touches the OS.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TileRequest {
    pub user_start: bool,
    pub scopes: Vec<String>,
    pub seconds: u64,
    pub input: TileInput,
    pub modifier: ProbeModifier,
}

impl TileRequest {
    pub fn validate(&self) -> Result<(), String> {
        if !self.user_start {
            return Err("tile requires explicit --user-start".to_owned());
        }
        if self.scopes.is_empty() {
            return Err("tile requires at least one --scope BUNDLE".to_owned());
        }
        if self.seconds == 0 || self.seconds > MAX_TILE_SECONDS {
            return Err(format!(
                "seconds must be 1..={MAX_TILE_SECONDS}, got {}",
                self.seconds
            ));
        }
        AppScope {
            allowed: self.scopes.clone(),
        }
        .validate()?;
        Ok(())
    }

    #[must_use]
    pub fn scope(&self) -> AppScope {
        AppScope {
            allowed: self.scopes.clone(),
        }
    }
}

/// Whether a discovered (pid, bundle) may be launched: exact allowlist
/// membership at the effect gate (Ghostty, own bundle, protected
/// terminals, and unknown bundles never pass), never the process itself.
#[must_use]
pub fn launchable(scope: &AppScope, pid: u32, bundle: &str) -> bool {
    if pid <= 1 || pid == std::process::id() {
        return false;
    }
    if bundle.is_empty() {
        return false;
    }
    scope.allows_effect(Some(bundle))
}

/// Terminal JSON outcome: honest counters and outcome vs live evidence.
/// Geometry is never restored (stop leaves windows in place); a refused or
/// dirty run reports so, never a bare claim.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TileOutcome {
    pub outcome: String,
    pub seconds: u64,
    pub input: TileInput,
    pub modifier: String,
    pub scopes: Vec<String>,
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
    pub shutdown_outcome: String,
    pub shutdown_joined: usize,
    pub shutdown_retained: usize,
    pub input_forwarded: u64,
    pub input_ignored: u64,
    pub input_refused_no_focus: u64,
    pub observations_published: u64,
    pub observation_failures: u64,
    pub note: String,
}

impl TileOutcome {
    fn refused(request: &TileRequest, reason: String) -> Self {
        Self {
            outcome: "refused".to_owned(),
            seconds: request.seconds,
            input: request.input,
            modifier: request.modifier.as_str().to_owned(),
            scopes: request.scopes.clone(),
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
            shutdown_outcome: "not-started".to_owned(),
            shutdown_joined: 0,
            shutdown_retained: 0,
            input_forwarded: 0,
            input_ignored: 0,
            input_refused_no_focus: 0,
            observations_published: 0,
            observation_failures: 0,
            note: "no effects ran; no restoration performed".to_owned(),
        }
    }
}

/// Portable entry: validates, then runs the native controller (macOS-only).
/// Exit codes: 2 usage/validation, 1 runtime refusal or failure.
pub fn run_tile(request: TileRequest) -> Result<String, (i32, String)> {
    request.validate().map_err(|reason| (2, reason))?;
    #[cfg(target_os = "macos")]
    {
        native::run(request)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = request;
        Err((1, "tile is macOS-only".to_owned()))
    }
}

#[cfg(target_os = "macos")]
mod native {
    use super::*;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use crate::native_input::{
        CarbonTileFactory, OffTileFactory, TapTileFactory, interception_gate, tile_catalogue,
    };
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
        // Accessory, no activation UI: `finishLaunching` publishes the
        // app without entering `run`; the bounded manual pump below keeps
        // lifetime control. Never steals focus, never opens windows.
        let app = NSApplication::sharedApplication(mtm);
        app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
        app.finishLaunching();

        let scope = request.scope();
        let catalogue = tile_catalogue();
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
            Carbon(Arc<CarbonTileFactory>),
            Tap(Arc<TapTileFactory>),
            Off(Arc<OffTileFactory>),
        }
        let input: InputGuard = match request.input {
            TileInput::Carbon => InputGuard::Carbon(Arc::new(
                CarbonTileFactory::new(request.modifier, catalogue.clone())
                    .map(|f| {
                        f.with_scope(scope.clone())
                            .with_facts_store(Arc::clone(&facts_store))
                    })
                    .map_err(|reason| (2, reason))?,
            )),
            TileInput::Tap => InputGuard::Tap(Arc::new(
                TapTileFactory::new(request.modifier, catalogue.clone())
                    .map(|f| {
                        f.with_scope(scope.clone())
                            .with_facts_store(Arc::clone(&facts_store))
                    })
                    .map_err(|reason| (2, reason))?,
            )),
            TileInput::Off => InputGuard::Off(Arc::new(OffTileFactory)),
        };
        let input_factory: Arc<dyn InputSourceFactory> = match &input {
            InputGuard::Carbon(factory) => Arc::clone(factory) as Arc<dyn InputSourceFactory>,
            InputGuard::Tap(factory) => Arc::clone(factory) as Arc<dyn InputSourceFactory>,
            InputGuard::Off(factory) => Arc::clone(factory) as Arc<dyn InputSourceFactory>,
        };
        let config = RuntimeConfig {
            topology_mode: TopologyMode::Native,
            allowed_bundles: request.scopes.clone(),
            modifier: request.modifier,
            tile_bindings: Some(catalogue),
            ..RuntimeConfig::default()
        };
        config.validate().map_err(|reason| (2, reason))?;
        // Hard lifetime armed BEFORE any runtime/input effect: finite
        // startup plus soft run lifetime plus bounded teardown plus finite
        // grace, then a dirty forced exit. `Drop` disarms as a backstop so
        // early errors never leave a delayed forced exit behind.
        let mut watchdog = Watchdog::arm(watchdog_limit(
            config.startup_ready_timeout,
            request.seconds,
            SHUTDOWN_BOUND,
            WATCHDOG_GRACE,
        ))
        .map_err(|reason| (1, reason))?;
        // Started marker for the fixture harness: exact tiler PID plus a
        // per-run ID on stdout BEFORE any runtime/input effect, so an
        // independent controller can bind (pid + start/exe) and address
        // exactly this process for the forced-kill leg. No user content:
        // IDs only. A nonce failure never fails the run (falls back to a
        // pid-anchored ID); the PID itself is the identity, the run ID only
        // disambiguates relaunches in one harness log.
        {
            let runid = crate::probe_target::mint_nonce()
                .unwrap_or_else(|_| format!("run-{}-{}", std::process::id(), request.seconds));
            println!(
                "{}",
                serde_json::json!({
                    "tilestarted": true,
                    "pid": std::process::id(),
                    "runid": runid,
                })
            );
            use std::io::Write as _;
            let _ = std::io::stdout().flush();
        }
        let ax_factory = Arc::new(
            ScopedAxFactory::new(scope.clone())
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
        let deadline = Instant::now() + Duration::from_secs(request.seconds);
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

        while Instant::now() < deadline {
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

            // Input refusal (tap user-disable) ends the run truthfully:
            // never a silent success.
            if let InputGuard::Tap(factory) = &input
                && let Some(reason) = factory.refusal()
            {
                early_refusal = Some(reason);
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
                if !launchable(&scope, pid, &bundle) {
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
        let (gated_ms, input_refusal, carbon_statuses) = match &input {
            InputGuard::Carbon(factory) => (
                factory.gated_ms(),
                early_refusal.clone().or_else(|| factory.refusal()),
                factory.statuses(),
            ),
            InputGuard::Tap(factory) => (
                factory.gated_ms(),
                early_refusal.clone().or_else(|| factory.refusal()),
                Vec::new(),
            ),
            InputGuard::Off(_) => (0, early_refusal.clone(), Vec::new()),
        };
        // Bounded shutdown independent of blocked AX: only proven exits
        // join; the rest retain for reap. Windows stay in place — nothing
        // restores geometry on any path.
        let report = handle.shutdown(SHUTDOWN_BOUND);
        let diagnostics = handle.diagnostics();
        // The hard lifetime is met: cancel the watchdog before reporting.
        watchdog.disarm();
        let outcome = if early_refusal.is_some() || input_refusal.is_some() {
            "input-refused"
        } else if report.outcome == crate::runtime::ShutdownOutcome::Clean {
            "completed"
        } else {
            "unclean-shutdown"
        };
        let body = TileOutcome {
            outcome: outcome.to_owned(),
            seconds: request.seconds,
            input: request.input,
            modifier: request.modifier.as_str().to_owned(),
            scopes: request.scopes.clone(),
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
            shutdown_outcome: format!("{:?}", report.outcome),
            shutdown_joined: report.joined,
            shutdown_retained: report.retained,
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
            input: TileInput::Carbon,
            modifier: ProbeModifier::Meta,
        }
    }

    #[test]
    fn input_mode_parses_exactly() {
        assert_eq!(TileInput::parse("carbon"), Ok(TileInput::Carbon));
        assert_eq!(TileInput::parse("tap"), Ok(TileInput::Tap));
        assert_eq!(TileInput::parse("off"), Ok(TileInput::Off));
        assert!(TileInput::parse("auto").is_err());
        assert!(TileInput::parse("").is_err());
        assert_eq!(TileInput::default(), TileInput::Carbon);
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

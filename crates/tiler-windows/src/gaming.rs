#![forbid(unsafe_code)]
//! Read-only Windows gaming-coexistence detection (portable policy, no Win32).
//!
//! Xbox mode and Game Bar own the authentic `Win+F11` fullscreen and `Win+G`
//! float chords. This module holds the categorical facts, the conflict
//! evidence, the cached refresh worker, and the allowlisted Settings
//! navigation model. Native reads live in the `tray_sys::gaming` child module
//! (`cfg(windows)` only); unit tests inject fakes, never the registry or the
//! package manager.
//!
//! Xbox mapping (per-user, undocumented recipe only): `GamingHomeApp` under
//! `HKCU\...\GamingConfiguration` carries the Xbox home-app AUMID when on
//! ([`XBOX_HOME_APP_ON`], verified against the research-cited ElevenForum
//! recipe on 2026-10-16) and the empty string when off. Absent, unreadable,
//! or unrecognised values stay [`XboxConfig::Unknown`] conservatively: unknown
//! never claims enabled, disabled, or resolved. Only the recognised empty
//! (off) value clears the Xbox warning. A successful current-user Game Bar
//! package-query absence clears the whole `Win+G` conflict (no residual
//! check); query failure stays unknown and keeps the explicit warning.
//!
//! Logging carries only categorical source/outcome/state: raw registry values
//! and package inventories never cross into logs.

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use crate::settings::{Settings, effective_bindings};
use crate::tray::{UnresolvedConflict, unresolved_conflicts};

// ---------------------------------------------------------------------------
// Stable native/model surface.
// ---------------------------------------------------------------------------

/// Registry subkey (under `HKEY_CURRENT_USER`) holding the Xbox home-app value.
pub const XBOX_CONFIG_SUBKEY: &str =
    "Software\\Microsoft\\Windows\\CurrentVersion\\GamingConfiguration";
/// Registry value naming the Xbox home app (undocumented recipe mapping).
pub const XBOX_CONFIG_VALUE: &str = "GamingHomeApp";
/// `GamingHomeApp` content meaning Xbox mode on, per the research-cited
/// ElevenForum recipe (REG-file contents: this exact AUMID for on, empty for
/// off). Undocumented and unstable: only exact matches classify.
pub const XBOX_HOME_APP_ON: &str = "Microsoft.GamingApp_8wekyb3d8bbwe!Microsoft.Xbox.App";
/// Game Bar package family for the current-user registration query.
pub const GAMEBAR_PACKAGE_FAMILY: &str = "Microsoft.XboxGamingOverlay_8wekyb3d8bbwe";

/// Allowlisted Xbox mode Settings page (documented Settings URI).
pub const XBOX_SETTINGS_URI: &str = "ms-settings:gaming-fullscreen";
/// Allowlisted Game Bar Settings page (documented Settings URI).
pub const GAMEBAR_SETTINGS_URI: &str = "ms-settings:gaming-gamebar";
/// Allowlisted Apps settings fallback page (documented Settings URI).
pub const APP_SETTINGS_URI: &str = "ms-settings:appsfeatures";

/// Allowlisted Game Bar app-settings page for the validated package family.
#[must_use]
pub fn gamebar_app_settings_uri() -> String {
    format!("ms-settings:appsfeatures-app?{GAMEBAR_PACKAGE_FAMILY}")
}

/// Whether a Settings URI is on the gaming-navigation allowlist. Settings
/// pages only (Store reinstall stays plain instructional text): exact
/// matches only, and navigation never claims resolution.
#[must_use]
pub fn is_allowed_navigation_uri(uri: &str) -> bool {
    uri == XBOX_SETTINGS_URI
        || uri == GAMEBAR_SETTINGS_URI
        || uri == APP_SETTINGS_URI
        || uri == gamebar_app_settings_uri()
}

// ---------------------------------------------------------------------------
// Categorical facts.
// ---------------------------------------------------------------------------

/// Xbox mode configuration: only exact recipe matches classify; everything
/// else stays unknown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XboxConfig {
    Unknown,
    Enabled,
    Disabled,
}

impl XboxConfig {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Enabled => "enabled",
            Self::Disabled => "disabled",
        }
    }
}

/// Categorical Xbox read outcome (never a raw value).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XboxOutcome {
    Matched,
    Absent,
    Unreadable,
    Unrecognised,
}

impl XboxOutcome {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Matched => "matched",
            Self::Absent => "absent",
            Self::Unreadable => "unreadable",
            Self::Unrecognised => "unrecognised",
        }
    }
}

/// Raw Xbox read handed from the native probe to the pure classifier. The raw
/// string never reaches logs; only the classified outcome is logged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum XboxRead {
    /// Key or value absent.
    Absent,
    /// Present but unreadable (denied, wrong type, undecodable, truncated).
    Unreadable,
    /// Present string content, classified by exact match below.
    Value(String),
}

/// Pure Xbox classification: the on AUMID enables, the empty (off) value
/// disables, anything else stays unknown.
#[must_use]
pub fn classify_xbox(read: &XboxRead) -> (XboxConfig, XboxOutcome) {
    match read {
        XboxRead::Absent => (XboxConfig::Unknown, XboxOutcome::Absent),
        XboxRead::Unreadable => (XboxConfig::Unknown, XboxOutcome::Unreadable),
        XboxRead::Value(text) => {
            if text == XBOX_HOME_APP_ON {
                (XboxConfig::Enabled, XboxOutcome::Matched)
            } else if text.is_empty() {
                (XboxConfig::Disabled, XboxOutcome::Matched)
            } else {
                (XboxConfig::Unknown, XboxOutcome::Unrecognised)
            }
        }
    }
}

/// Game Bar package registration for the current user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameBarPackage {
    Unknown,
    Registered,
    Absent,
}

impl GameBarPackage {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Registered => "registered",
            Self::Absent => "absent",
        }
    }
}

/// Categorical Game Bar query outcome (never a package inventory).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameBarOutcome {
    Counted,
    Absent,
    QueryFailed,
}

impl GameBarOutcome {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Counted => "counted",
            Self::Absent => "absent",
            Self::QueryFailed => "query-failed",
        }
    }
}

/// Pure `REG_SZ` payload classifier over raw value bytes (little-endian
/// UTF-16, as reported by the registry read). Rejects odd lengths, invalid
/// UTF-16, missing terminators, embedded NULs, and trailing data after the
/// terminator conservatively: none of those can report the off (empty)
/// state, so malformed payloads never classify as disabled.
#[must_use]
pub fn classify_xbox_bytes(raw: &[u8]) -> XboxRead {
    if !raw.len().is_multiple_of(2) {
        return XboxRead::Unreadable;
    }
    let units: Vec<u16> = raw
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u16::from_le_bytes(*pair))
        .collect();
    let Some(terminator) = units.iter().position(|unit| *unit == 0) else {
        return XboxRead::Unreadable;
    };
    // Exactly one trailing NUL: embedded NULs and extra trailing units mean
    // a non-`REG_SZ` shape (or truncation), never the off state.
    if terminator != units.len() - 1 {
        return XboxRead::Unreadable;
    }
    let text: String = match char::decode_utf16(units[..terminator].iter().copied()).collect() {
        Ok(text) => text,
        Err(_) => return XboxRead::Unreadable,
    };
    XboxRead::Value(text)
}

/// Pure Game Bar classification over the family match count. Query failure
/// stays unknown, never absent.
#[must_use]
pub const fn classify_package_query(ok: bool, count: u32) -> (GameBarPackage, GameBarOutcome) {
    if !ok {
        return (GameBarPackage::Unknown, GameBarOutcome::QueryFailed);
    }
    if count > 0 {
        (GameBarPackage::Registered, GameBarOutcome::Counted)
    } else {
        (GameBarPackage::Absent, GameBarOutcome::Absent)
    }
}

/// One cached detection pass: both categorical facts plus the worker
/// generation. Conservative initial state is unknown/unknown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GamingSnapshot {
    pub xbox: XboxConfig,
    pub xbox_outcome: XboxOutcome,
    pub gamebar: GameBarPackage,
    pub gamebar_outcome: GameBarOutcome,
    pub generation: u64,
}

impl GamingSnapshot {
    #[must_use]
    pub const fn unknown() -> Self {
        Self {
            xbox: XboxConfig::Unknown,
            xbox_outcome: XboxOutcome::Absent,
            gamebar: GameBarPackage::Unknown,
            gamebar_outcome: GameBarOutcome::QueryFailed,
            generation: 0,
        }
    }

    /// Categorical log lines for one pass: source/outcome/state only, never
    /// raw registry values or package inventories.
    #[must_use]
    pub fn log_lines(&self, reason: RefreshReason) -> [serde_json::Value; 2] {
        [
            serde_json::json!({
                "event": "gaming-probe",
                "source": "xbox-config",
                "detail": "undocumented-hkcu-gaminghomeapp",
                "outcome": self.xbox_outcome.as_str(),
                "state": self.xbox.as_str(),
                "reason": reason.as_str(),
                "generation": self.generation,
            }),
            serde_json::json!({
                "event": "gaming-probe",
                "source": "gamebar-package",
                "outcome": self.gamebar_outcome.as_str(),
                "state": self.gamebar.as_str(),
                "reason": reason.as_str(),
                "generation": self.generation,
            }),
        ]
    }
}

// ---------------------------------------------------------------------------
// Conflict evidence over effective chords.
// ---------------------------------------------------------------------------

/// Which OS gaming surface a conflict belongs to (matched by note, not id, so
/// rebound chords keep their true owner).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GamingConflictKind {
    GameBar,
    XboxMode,
}

impl GamingConflictKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::GameBar => "gamebar",
            Self::XboxMode => "xbox-mode",
        }
    }
}

/// One effective gaming conflict with its categorical evidence for the tray
/// and Settings UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GamingConflict {
    pub id: &'static str,
    pub chord: String,
    pub kind: GamingConflictKind,
    pub evidence: &'static str,
}

/// True for the Game Bar containment note (authentic `Win+G` and its rebinds).
#[must_use]
pub fn is_gamebar_note(note: &str) -> bool {
    note.contains("Game Bar")
}

/// True for the Xbox mode containment note (authentic `Win+F11` and rebinds).
#[must_use]
pub fn is_xbox_mode_note(note: &str) -> bool {
    note.contains("Xbox mode")
}

/// Game Bar evidence for one registration state. A successful package-query
/// absence clears the `Win+G` conflict (no residual check); query failure
/// stays unknown with the explicit warning below.
#[must_use]
pub const fn gamebar_evidence(state: GameBarPackage) -> &'static str {
    match state {
        GameBarPackage::Registered => {
            "Game Bar registered for this user; Win+G dispatch unproven, check required"
        }
        GameBarPackage::Absent => "no matching Game Bar package for this user; no Win+G conflict",
        GameBarPackage::Unknown => "Game Bar registration unknown (query failed); check required",
    }
}

/// Xbox evidence for one configuration state. Unknown (absent, unreadable,
/// unrecognised) keeps the check-required warning; only the recognised empty
/// (off) value clears it downstream.
#[must_use]
pub const fn xbox_evidence(state: XboxConfig) -> &'static str {
    match state {
        XboxConfig::Enabled => {
            "Xbox mode configuration on (per-user recipe); disable in Xbox mode settings"
        }
        XboxConfig::Disabled => "Xbox mode configuration off (per-user recipe)",
        XboxConfig::Unknown => "Xbox mode configuration unknown; check Xbox mode settings",
    }
}

/// Effective gaming conflicts: kept effective bindings carrying a Game Bar or
/// Xbox mode note, each with the cached categorical evidence. Takeover off
/// silences everything (global pass-through by user choice); Compatible
/// rebinds/disables carry no gaming note and stay silent; the Win+L opt-in
/// warning is untouched (never a gaming kind). Successful Game Bar package
/// absence clears the Game Bar conflict; only recognised Xbox off clears the
/// Xbox conflict. Query failure keeps its explicit unknown warning.
#[must_use]
pub fn gaming_conflicts(
    settings: &Settings,
    takeover: bool,
    snapshot: &GamingSnapshot,
) -> Vec<GamingConflict> {
    if !takeover {
        return Vec::new();
    }
    effective_bindings(settings)
        .into_iter()
        .filter(|row| row.active && row.effective)
        .filter_map(|row| {
            let note = row.conflict?;
            if is_gamebar_note(note) {
                if snapshot.gamebar == GameBarPackage::Absent {
                    return None;
                }
                return Some(GamingConflict {
                    id: row.id,
                    chord: row.chords.join(", "),
                    kind: GamingConflictKind::GameBar,
                    evidence: gamebar_evidence(snapshot.gamebar),
                });
            }
            if is_xbox_mode_note(note) {
                if snapshot.xbox == XboxConfig::Disabled {
                    return None;
                }
                return Some(GamingConflict {
                    id: row.id,
                    chord: row.chords.join(", "),
                    kind: GamingConflictKind::XboxMode,
                    evidence: xbox_evidence(snapshot.xbox),
                });
            }
            None
        })
        .collect()
}

/// Tray warning rows: the existing [`unresolved_conflicts`] rows minus the
/// Xbox row once the recognised off value reports Xbox mode off, and minus
/// the Game Bar row once a successful package query reports absence. Every
/// other row (query-failed Game Bar, Win+L opt-in) keeps existing behavior
/// exactly, so with an unknown snapshot this equals [`unresolved_conflicts`].
#[must_use]
pub fn tray_warning_conflicts(
    settings: &Settings,
    takeover: bool,
    allow_win_l: bool,
    snapshot: &GamingSnapshot,
) -> Vec<UnresolvedConflict> {
    unresolved_conflicts(settings, takeover, allow_win_l)
        .into_iter()
        .filter(|conflict| {
            // Note-matched like `gaming_conflicts` (rebound chords keep
            // their true owner): recognised off clears Xbox rows and a
            // successful package absence clears Game Bar rows, whatever
            // binding id carries them.
            !(is_xbox_mode_note(conflict.note) && snapshot.xbox == XboxConfig::Disabled)
                && !(is_gamebar_note(conflict.note) && snapshot.gamebar == GameBarPackage::Absent)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Cached refresh worker.
// ---------------------------------------------------------------------------

/// Why a detection pass ran (categorical log reason).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshReason {
    Startup,
    SettingsChange,
    Recheck,
    Interval,
}

impl RefreshReason {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Startup => "startup",
            Self::SettingsChange => "settings-change",
            Self::Recheck => "recheck",
            Self::Interval => "interval",
        }
    }
}

/// Infrequent bounded re-read cadence for the cached facts.
pub const REFRESH_INTERVAL: Duration = Duration::from_secs(300);

/// Native probe: both categorical facts in one pass. Runs on the worker
/// thread only, never on tile/input loops.
pub type ProbeFn = Arc<dyn Fn() -> GamingSnapshot + Send + Sync>;
/// Worker thread body (one probe pass plus its bookkeeping).
type ThreadBody = Box<dyn FnOnce() + Send + 'static>;
/// Thread spawner: the production spawner names the thread and propagates
/// OS spawn failure; tests inject fakes. Never panics on spawn failure.
pub type SpawnerFn =
    Arc<dyn Fn(ThreadBody) -> std::io::Result<std::thread::JoinHandle<()>> + Send + Sync>;

/// Owner completion hint: invoked once per finished worker pass (or
/// synchronous degradation) after the categorical lines queue. Never
/// blocks, never logs; a missing hint simply leaves the owner to its
/// interval deadline.
pub type CompletionNotify = Arc<dyn Fn() + Send + Sync>;
/// Bound for queued categorical log lines drained on the owner pump (eight
/// passes at two lines each). Overflow drops the oldest; the cache itself
/// always holds the latest pass.
const MAX_QUEUED_LOG_LINES: usize = 16;

fn default_spawner() -> SpawnerFn {
    Arc::new(|body| {
        std::thread::Builder::new()
            .name("gaming-probe".to_owned())
            .spawn(body)
    })
}

struct MonitorInner {
    cache: Mutex<GamingSnapshot>,
    probe: ProbeFn,
    spawner: SpawnerFn,
    in_flight: AtomicBool,
    pending: AtomicBool,
    pending_reason: Mutex<Option<RefreshReason>>,
    generation: AtomicU64,
    /// Last worker start or completion: the interval arm is gated on this
    /// while idle, so slow probes restart the cadence instead of requeueing.
    last_attempt: Mutex<Option<Instant>>,
    last_revision: Mutex<Option<u64>>,
    /// Optional owner wake hint invoked (never blocking, never logging)
    /// after every completed or degraded pass so an event-driven owner
    /// drains promptly instead of stranding results behind an idle wait.
    /// Plain function object: the worker never waits on the owner.
    notify: Mutex<Option<CompletionNotify>>,
    /// Completed-pass categorical lines for the owner pump to drain and
    /// write (single-writer log integrity: the worker never writes the log).
    logs: Mutex<std::collections::VecDeque<serde_json::Value>>,
}

/// Cached gaming facts with a detached worker: at most one probe in flight,
/// never waited on during tiling. Completed passes queue categorical lines
/// for the owner pump to drain and write; failures keep the conservative
/// cached state and never stop tiling.
pub struct GamingMonitor {
    inner: Arc<MonitorInner>,
}

impl GamingMonitor {
    /// Monitor with the production thread spawner.
    #[must_use]
    pub fn new(probe: ProbeFn) -> Self {
        Self::with_spawner(probe, default_spawner())
    }

    /// Monitor with an injected spawner (deterministic spawn-failure tests;
    /// production uses [`GamingMonitor::new`]).
    #[must_use]
    pub fn with_spawner(probe: ProbeFn, spawner: SpawnerFn) -> Self {
        Self {
            inner: Arc::new(MonitorInner {
                cache: Mutex::new(GamingSnapshot::unknown()),
                probe,
                spawner,
                in_flight: AtomicBool::new(false),
                pending: AtomicBool::new(false),
                pending_reason: Mutex::new(None),
                generation: AtomicU64::new(0),
                last_attempt: Mutex::new(None),
                last_revision: Mutex::new(None),
                logs: Mutex::new(std::collections::VecDeque::new()),
                notify: Mutex::new(None),
            }),
        }
    }

    /// Install the owner completion hint invoked after every finished pass
    /// (or synchronous degradation). Replaces any previous hint; `None`
    /// clears it. The hint must never block or log.
    pub fn set_completion_notify(&self, notify: Option<CompletionNotify>) {
        *self
            .inner
            .notify
            .lock()
            .unwrap_or_else(|poison| poison.into_inner()) = notify;
    }

    /// Next interval re-read due time for an event-driven owner's wait
    /// deadline, or `None` while a pass runs (the completion hint wakes
    /// the owner instead, so a running pass never arms a deadline that
    /// would busy-loop on attempt-gating). Attempt-gated like
    /// [`GamingMonitor::maybe_refresh`]: a slow probe restarts the
    /// cadence on completion instead of requeueing.
    #[must_use]
    pub fn interval_due_at(&self) -> Option<Instant> {
        if self.inner.in_flight.load(Ordering::SeqCst) {
            return None;
        }
        Some(
            self.inner
                .last_attempt
                .lock()
                .unwrap_or_else(|poison| poison.into_inner())
                .map(|at| at + REFRESH_INTERVAL)
                .unwrap_or_else(Instant::now),
        )
    }

    /// Current cached facts (non-blocking brief lock; the worker holds it
    /// only to store one pass).
    #[must_use]
    pub fn snapshot(&self) -> GamingSnapshot {
        *self
            .inner
            .cache
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }

    /// True while a worker pass runs.
    #[must_use]
    pub fn in_flight(&self) -> bool {
        self.inner.in_flight.load(Ordering::SeqCst)
    }

    /// Record the live settings revision without probing (startup seeding:
    /// the startup pass is requested explicitly).
    pub fn sync_revision(&self, revision: Option<u64>) {
        *self
            .inner
            .last_revision
            .lock()
            .unwrap_or_else(|poison| poison.into_inner()) = revision;
    }

    /// Drain queued categorical lines for the owner pump to write (the log
    /// single writer). Bounded; steady-state drains every pump.
    #[must_use]
    pub fn drain_logs(&self) -> Vec<serde_json::Value> {
        self.inner
            .logs
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .drain(..)
            .collect()
    }

    /// Fire-and-forget pass: coalesces with a running pass (at most one in
    /// flight; a racing request reruns once after with the latest reason).
    /// Never blocks tiling.
    pub fn request_refresh(&self, reason: RefreshReason) {
        spawn_pass(&self.inner, reason);
    }

    /// Explicit recheck for Settings UI.
    pub fn recheck(&self) {
        spawn_pass(&self.inner, RefreshReason::Recheck);
    }

    /// Pump-side trigger: a changed settings revision refreshes, otherwise an
    /// infrequent bounded interval pass covers external (Settings-made)
    /// changes. The interval arm is attempt-gated, so a slow or blocked probe
    /// never requeues work: steady state (and a running pass) spawns nothing.
    pub fn maybe_refresh(&self, revision: Option<u64>) {
        let mut last = self
            .inner
            .last_revision
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if revision != *last {
            *last = revision;
            drop(last);
            spawn_pass(&self.inner, RefreshReason::SettingsChange);
            return;
        }
        drop(last);
        // The interval arm never queues while a pass runs: with only
        // attempt-gating, a probe blocked past the interval would requeue
        // (as pending) on every pump and rerun continuously.
        if self.inner.in_flight.load(Ordering::SeqCst) {
            return;
        }
        let due = self
            .inner
            .last_attempt
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .is_none_or(|at| at.elapsed() >= REFRESH_INTERVAL);
        if due {
            spawn_pass(&self.inner, RefreshReason::Interval);
        }
    }
}

/// Queue one categorical line, dropping the oldest on overflow (bounded;
/// the cache always holds the latest pass regardless).
fn push_line(inner: &MonitorInner, line: serde_json::Value) {
    let mut logs = inner
        .logs
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    if logs.len() >= MAX_QUEUED_LOG_LINES {
        logs.pop_front();
    }
    logs.push_back(line);
}

/// Start one worker pass unless one already runs (then coalesce one rerun
/// with the latest reason). The attempt time is recorded synchronously with
/// the start decision, so interval gating never observes a stale clock while
/// a probe is blocked. Spawn failure degrades nonfatally: the in-flight flag
/// clears, one honest `spawn-failed` line queues, and the next interval or
/// explicit request retries. The tail re-check closes the race between the
/// final pending check and the in-flight clear: a request landing there
/// either saw in-flight and set pending (observed by the tail swap) or lands
/// after the clear and wins its own compare-exchange.
fn spawn_pass(inner: &Arc<MonitorInner>, reason: RefreshReason) {
    if inner
        .in_flight
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        *inner
            .pending_reason
            .lock()
            .unwrap_or_else(|poison| poison.into_inner()) = Some(reason);
        inner.pending.store(true, Ordering::SeqCst);
        return;
    }
    *inner
        .last_attempt
        .lock()
        .unwrap_or_else(|poison| poison.into_inner()) = Some(Instant::now());
    let worker = Arc::clone(inner);
    let spawner = Arc::clone(&worker.spawner);
    let failed = Arc::clone(&worker);
    let body: ThreadBody = Box::new(move || worker_loop(&worker, reason));
    if spawner(body).is_err() {
        // Nonfatal degradation: the stale cache must not survive (a
        // previously observed off state would keep hiding the warning), so
        // unknown publishes with a fresh generation; one honest line queues
        // for the owner drain, coalesced requests drop, and the next
        // interval or explicit request retries.
        failed.in_flight.store(false, Ordering::SeqCst);
        failed.pending.store(false, Ordering::SeqCst);
        *failed
            .pending_reason
            .lock()
            .unwrap_or_else(|poison| poison.into_inner()) = None;
        publish_unknown(&failed, reason, "spawn-failed");
        // Synchronous degradation still wakes an event-driven owner so
        // the honest line drains promptly.
        notify_owner(&failed);
    }
}

/// Invoke the owner completion hint, if one is installed. Panic-isolated:
/// a misbehaving hint must never take down the detached worker (which
/// would strand `in_flight` set and silence all future deadlines).
fn notify_owner(inner: &MonitorInner) {
    let notify = inner
        .notify
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .clone();
    if let Some(notify) = notify {
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| (notify)()));
    }
}

/// Publish a conservative unknown snapshot with a fresh generation plus one
/// honest worker line. Spawn and probe-panic degradation share this: the
/// cache must never keep a stale classified state hiding warnings, and the
/// generation bump redraws conservative warnings downstream.
fn publish_unknown(inner: &MonitorInner, reason: RefreshReason, outcome: &'static str) {
    let mut snapshot = GamingSnapshot::unknown();
    snapshot.generation = inner.generation.fetch_add(1, Ordering::SeqCst) + 1;
    *inner
        .cache
        .lock()
        .unwrap_or_else(|poison| poison.into_inner()) = snapshot;
    push_line(
        inner,
        serde_json::json!({
            "event": "gaming-probe",
            "source": "worker",
            "outcome": outcome,
            "state": "unknown",
            "reason": reason.as_str(),
            "generation": snapshot.generation,
        }),
    );
}

/// One worker pass plus at most one coalesced rerun carrying the latest
/// queued reason. Reruns are bounded: each consumes one coalesced request,
/// and pump triggers are attempt-gated, so a running pass can only gain
/// reruns from explicit requests.
fn worker_loop(worker: &Arc<MonitorInner>, reason: RefreshReason) {
    let mut current = reason;
    loop {
        // Nonfatal probe boundary: a panicking probe degrades to unknown
        // (never strands in-flight, never stops tiling).
        let probed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| (worker.probe)()));
        match probed {
            Ok(mut snapshot) => {
                snapshot.generation = worker.generation.fetch_add(1, Ordering::SeqCst) + 1;
                *worker
                    .cache
                    .lock()
                    .unwrap_or_else(|poison| poison.into_inner()) = snapshot;
                for line in snapshot.log_lines(current) {
                    push_line(worker, line);
                }
            }
            Err(_) => {
                publish_unknown(worker, current, "probe-panicked");
            }
        }
        // Re-arm the interval cadence at each completed pass, so a slow
        // probe neither looks due on completion nor (with the in-flight skip
        // above) requeues while running.
        *worker
            .last_attempt
            .lock()
            .unwrap_or_else(|poison| poison.into_inner()) = Some(Instant::now());
        if !worker.pending.swap(false, Ordering::SeqCst) {
            break;
        }
        current = worker
            .pending_reason
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .take()
            .unwrap_or(current);
    }
    worker.in_flight.store(false, Ordering::SeqCst);
    if worker.pending.swap(false, Ordering::SeqCst) {
        let rerun = worker
            .pending_reason
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .take()
            .unwrap_or(current);
        spawn_pass(worker, rerun);
    }
    // Completion (or the tail rerun's own completion) wakes an
    // event-driven owner after `in_flight` settles, so queued lines never
    // strand behind an idle wait. A tail rerun notifies again on its own
    // completion; draining works regardless of the flag.
    notify_owner(worker);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{BindingSetting, BindingState, apply_preset, validate_settings};
    use std::sync::atomic::{AtomicBool, AtomicUsize};

    fn authentic() -> Settings {
        Settings::default()
    }

    fn snapshot_with(xbox: XboxConfig, gamebar: GameBarPackage) -> GamingSnapshot {
        let (xbox_outcome, gamebar_outcome) = match (xbox, gamebar) {
            (XboxConfig::Unknown, GameBarPackage::Unknown) => {
                (XboxOutcome::Absent, GameBarOutcome::QueryFailed)
            }
            (XboxConfig::Unknown, _) => (XboxOutcome::Absent, GameBarOutcome::Counted),
            (_, GameBarPackage::Unknown) => (XboxOutcome::Matched, GameBarOutcome::QueryFailed),
            _ => (XboxOutcome::Matched, GameBarOutcome::Counted),
        };
        GamingSnapshot {
            xbox,
            xbox_outcome,
            gamebar,
            gamebar_outcome,
            generation: 1,
        }
    }

    fn unknown_snapshot() -> GamingSnapshot {
        GamingSnapshot::unknown()
    }

    #[test]
    fn xbox_recipe_values_classify_exactly() {
        assert_eq!(
            classify_xbox(&XboxRead::Value(XBOX_HOME_APP_ON.to_owned())),
            (XboxConfig::Enabled, XboxOutcome::Matched)
        );
        assert_eq!(
            classify_xbox(&XboxRead::Value(String::new())),
            (XboxConfig::Disabled, XboxOutcome::Matched)
        );
        // Near misses stay unknown: no prefix, suffix, case, or whitespace
        // leniency (the recipe is undocumented, so only exact matches count).
        for near in [
            "microsoft.gamingapp_8wekyb3d8bbwe!microsoft.xbox.app",
            "Microsoft.GamingApp_8wekyb3d8bbwe!Microsoft.Xbox.App ",
            " Microsoft.GamingApp_8wekyb3d8bbwe!Microsoft.Xbox.App",
            "Microsoft.GamingApp_8wekyb3d8bbwe!Microsoft.Xbox.AppX",
            "None",
            "0",
        ] {
            assert_eq!(
                classify_xbox(&XboxRead::Value(near.to_owned())),
                (XboxConfig::Unknown, XboxOutcome::Unrecognised),
                "{near:?}"
            );
        }
        assert_eq!(
            classify_xbox(&XboxRead::Absent),
            (XboxConfig::Unknown, XboxOutcome::Absent)
        );
        assert_eq!(
            classify_xbox(&XboxRead::Unreadable),
            (XboxConfig::Unknown, XboxOutcome::Unreadable)
        );
    }

    #[test]
    fn package_counts_classify_without_inventory() {
        assert_eq!(
            classify_package_query(true, 2),
            (GameBarPackage::Registered, GameBarOutcome::Counted)
        );
        assert_eq!(
            classify_package_query(true, 0),
            (GameBarPackage::Absent, GameBarOutcome::Absent)
        );
        // Failure never reports absent.
        assert_eq!(
            classify_package_query(false, 0),
            (GameBarPackage::Unknown, GameBarOutcome::QueryFailed)
        );
        assert_eq!(
            classify_package_query(false, 3),
            (GameBarPackage::Unknown, GameBarOutcome::QueryFailed)
        );
    }

    #[test]
    fn authentic_unknown_keeps_both_warnings_with_evidence() {
        let conflicts = gaming_conflicts(&authentic(), true, &unknown_snapshot());
        assert_eq!(conflicts.len(), 2);
        let float = conflicts
            .iter()
            .find(|c| c.id == "toggle-float")
            .expect("float warns");
        assert_eq!(float.kind, GamingConflictKind::GameBar);
        assert_eq!(float.chord, "Win+G");
        assert_eq!(float.evidence, gamebar_evidence(GameBarPackage::Unknown));
        let fullscreen = conflicts
            .iter()
            .find(|c| c.id == "toggle-fullscreen")
            .expect("fullscreen warns");
        assert_eq!(fullscreen.kind, GamingConflictKind::XboxMode);
        assert_eq!(fullscreen.chord, "Win+F11");
        assert_eq!(fullscreen.evidence, xbox_evidence(XboxConfig::Unknown));
    }

    #[test]
    fn package_absence_clears_gamebar_warning() {
        let snapshot = snapshot_with(XboxConfig::Disabled, GameBarPackage::Absent);
        let conflicts = gaming_conflicts(&authentic(), true, &snapshot);
        assert!(conflicts.is_empty());
        let warnings = tray_warning_conflicts(&authentic(), true, false, &snapshot);
        assert!(warnings.is_empty());
    }

    #[test]
    fn recognised_xbox_off_clears_only_the_xbox_warning() {
        let snapshot = snapshot_with(XboxConfig::Disabled, GameBarPackage::Registered);
        let conflicts = gaming_conflicts(&authentic(), true, &snapshot);
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].id, "toggle-float");
        let warnings = tray_warning_conflicts(&authentic(), true, false, &snapshot);
        let ids: Vec<&str> = warnings.iter().map(|c| c.id).collect();
        assert_eq!(ids, vec!["toggle-float"]);
    }

    #[test]
    fn recognised_xbox_on_warns_with_enabled_evidence() {
        let snapshot = snapshot_with(XboxConfig::Enabled, GameBarPackage::Registered);
        let conflicts = gaming_conflicts(&authentic(), true, &snapshot);
        assert_eq!(conflicts.len(), 2);
        let fullscreen = conflicts
            .iter()
            .find(|c| c.id == "toggle-fullscreen")
            .expect("fullscreen warns when on");
        assert!(fullscreen.evidence.contains("on (per-user recipe)"));
    }

    #[test]
    fn unknown_snapshot_matches_existing_tray_rows() {
        // With conservative unknown facts the new tray rows equal the
        // existing chord-driven rows exactly (Win+L gate included).
        for (takeover, allow_win_l) in [(true, false), (true, true), (false, false), (false, true)]
        {
            assert_eq!(
                tray_warning_conflicts(&authentic(), takeover, allow_win_l, &unknown_snapshot()),
                unresolved_conflicts(&authentic(), takeover, allow_win_l),
                "takeover={takeover} allow_win_l={allow_win_l}"
            );
        }
    }

    #[test]
    fn xbox_off_with_gamebar_absent_clears_gaming_rows_keeps_win_l() {
        let snapshot = snapshot_with(XboxConfig::Disabled, GameBarPackage::Absent);
        let warnings = tray_warning_conflicts(&authentic(), true, true, &snapshot);
        let ids: Vec<&str> = warnings.iter().map(|c| c.id).collect();
        assert_eq!(ids, vec!["focus-right"]);
    }

    #[test]
    fn compatible_rebind_disable_and_takeover_off_stay_silent() {
        let mut compatible = authentic();
        apply_preset(&mut compatible, crate::settings::Preset::Compatible);
        assert!(validate_settings(&compatible).is_ok());
        for snapshot in [
            unknown_snapshot(),
            snapshot_with(XboxConfig::Enabled, GameBarPackage::Registered),
            snapshot_with(XboxConfig::Disabled, GameBarPackage::Absent),
        ] {
            assert!(gaming_conflicts(&compatible, true, &snapshot).is_empty());
            assert!(tray_warning_conflicts(&compatible, true, false, &snapshot).is_empty());
            assert!(gaming_conflicts(&authentic(), false, &snapshot).is_empty());
            assert!(tray_warning_conflicts(&authentic(), false, true, &snapshot).is_empty());
        }
    }

    #[test]
    fn per_binding_disable_and_rebind_away_clear_gaming_rows() {
        for (id, chord) in [
            ("toggle-float", "Win+Q"),
            ("toggle-fullscreen", "Win+Ctrl+F11"),
        ] {
            let mut rebound = authentic();
            rebound.bindings.insert(
                id.to_owned(),
                BindingSetting {
                    state: BindingState::Rebind,
                    chord: Some(chord.to_owned()),
                },
            );
            assert!(validate_settings(&rebound).is_ok());
            let ids: Vec<&str> = gaming_conflicts(&rebound, true, &unknown_snapshot())
                .iter()
                .map(|c| c.id)
                .collect();
            assert_eq!(ids.len(), 1);
            assert!(!ids.contains(&id));
        }
        let mut disabled = authentic();
        disabled.bindings.insert(
            "toggle-float".to_owned(),
            BindingSetting {
                state: BindingState::Disabled,
                chord: None,
            },
        );
        assert!(validate_settings(&disabled).is_ok());
        let ids: Vec<&str> = gaming_conflicts(&disabled, true, &unknown_snapshot())
            .iter()
            .map(|c| c.id)
            .collect();
        assert_eq!(ids, vec!["toggle-fullscreen"]);
    }

    #[test]
    fn registry_payload_bytes_classify_conservatively() {
        fn units_bytes(units: &[u16]) -> Vec<u8> {
            units.iter().flat_map(|unit| unit.to_le_bytes()).collect()
        }
        fn text_payload(text: &str) -> Vec<u8> {
            let mut units: Vec<u16> = text.encode_utf16().collect();
            units.push(0);
            units_bytes(&units)
        }
        // Exact recipe values classify (empty means off).
        assert_eq!(
            classify_xbox_bytes(&text_payload(XBOX_HOME_APP_ON)),
            XboxRead::Value(XBOX_HOME_APP_ON.to_owned())
        );
        assert_eq!(
            classify_xbox_bytes(&text_payload("")),
            XboxRead::Value(String::new())
        );
        assert_eq!(
            classify_xbox(&classify_xbox_bytes(&text_payload(""))),
            (XboxConfig::Disabled, XboxOutcome::Matched)
        );
        // Malformed payloads never report off: odd lengths, invalid UTF-16,
        // missing terminators, embedded NULs, and trailing units after the
        // terminator all stay unreadable.
        for malformed in [
            vec![],
            vec![0u8],
            vec![0, 0, 0],
            units_bytes(&[0xD800, 0]),
            units_bytes(&[0x41, 0x42]),
            units_bytes(&[0x41, 0, 0x42, 0]),
            units_bytes(&[0x41, 0, 0]),
            units_bytes(&[0, 0, 0]),
        ] {
            assert_eq!(
                classify_xbox_bytes(&malformed),
                XboxRead::Unreadable,
                "{malformed:?}"
            );
        }
        // A well-formed non-recipe string stays unrecognised, never off.
        assert_eq!(
            classify_xbox(&classify_xbox_bytes(&text_payload("None"))),
            (XboxConfig::Unknown, XboxOutcome::Unrecognised)
        );
    }

    #[test]
    fn rebound_action_to_xbox_chord_tracks_note_not_id() {
        // Free Win+F11 from fullscreen, then bind it to maximize: the Xbox
        // conflict follows the note to the new owner, not the old id.
        let mut rebound = authentic();
        rebound.bindings.insert(
            "toggle-fullscreen".to_owned(),
            BindingSetting {
                state: BindingState::Disabled,
                chord: None,
            },
        );
        rebound.bindings.insert(
            "toggle-maximize".to_owned(),
            BindingSetting {
                state: BindingState::Rebind,
                chord: Some("Win+F11".to_owned()),
            },
        );
        assert!(validate_settings(&rebound).is_ok());
        let conflicts = gaming_conflicts(&rebound, true, &unknown_snapshot());
        let ids: Vec<&str> = conflicts.iter().map(|c| c.id).collect();
        assert_eq!(ids, vec!["toggle-float", "toggle-maximize"]);
        let maximize = conflicts
            .iter()
            .find(|c| c.id == "toggle-maximize")
            .expect("maximize warns at Win+F11");
        assert_eq!(maximize.kind, GamingConflictKind::XboxMode);
        assert_eq!(maximize.chord, "Win+F11");
        // The recognised off value clears the rebound Xbox row too.
        let off = snapshot_with(XboxConfig::Disabled, GameBarPackage::Registered);
        let ids: Vec<&str> = tray_warning_conflicts(&rebound, true, false, &off)
            .iter()
            .map(|c| c.id)
            .collect();
        assert_eq!(ids, vec!["toggle-float"]);
        let ids: Vec<&str> = tray_warning_conflicts(&rebound, true, false, &unknown_snapshot())
            .iter()
            .map(|c| c.id)
            .collect();
        assert_eq!(ids, vec!["toggle-float", "toggle-maximize"]);
    }

    #[test]
    fn navigation_allowlist_is_exact() {
        assert!(is_allowed_navigation_uri(XBOX_SETTINGS_URI));
        assert!(is_allowed_navigation_uri(GAMEBAR_SETTINGS_URI));
        assert!(is_allowed_navigation_uri(APP_SETTINGS_URI));
        assert_eq!(
            gamebar_app_settings_uri(),
            "ms-settings:appsfeatures-app?Microsoft.XboxGamingOverlay_8wekyb3d8bbwe"
        );
        assert!(is_allowed_navigation_uri(&gamebar_app_settings_uri()));
        for bad in [
            "ms-settings:gaming-fullscreen ",
            "ms-settings:gaming-fullscreen.evil",
            "MS-SETTINGS:gaming-gamebar",
            "ms-windows-store://search/?query=Xbox%20Game%20Bar",
            "https://example.invalid/xbox",
            "",
        ] {
            assert!(!is_allowed_navigation_uri(bad), "{bad:?}");
        }
    }

    #[test]
    fn log_lines_carry_only_categorical_facts() {
        let snapshot = snapshot_with(XboxConfig::Enabled, GameBarPackage::Registered);
        let lines = snapshot.log_lines(RefreshReason::Startup);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0]["event"], "gaming-probe");
        assert_eq!(lines[0]["source"], "xbox-config");
        assert_eq!(lines[0]["outcome"], "matched");
        assert_eq!(lines[0]["state"], "enabled");
        assert_eq!(lines[0]["reason"], "startup");
        assert_eq!(lines[1]["source"], "gamebar-package");
        assert_eq!(lines[1]["outcome"], "counted");
        assert_eq!(lines[1]["state"], "registered");
        // Xbox source is labeled explicitly as the undocumented recipe.
        assert_eq!(lines[0]["detail"], "undocumented-hkcu-gaminghomeapp");
        let text = serde_json::to_string(&lines).expect("serializable");
        assert!(!text.contains("GamingApp"));
        assert!(!text.contains("XboxGamingOverlay"));
    }

    fn scripted_monitor(script: Vec<GamingSnapshot>) -> (GamingMonitor, Arc<AtomicUsize>) {
        let queue = Arc::new(Mutex::new(script));
        let congestion = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let probe: ProbeFn = {
            let queue = Arc::clone(&queue);
            let congestion = Arc::clone(&congestion);
            let peak = Arc::clone(&peak);
            Arc::new(move || {
                let held = congestion.fetch_add(1, Ordering::SeqCst) + 1;
                peak.fetch_max(held, Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(5));
                let next = queue
                    .lock()
                    .unwrap_or_else(|poison| poison.into_inner())
                    .pop()
                    .unwrap_or_else(GamingSnapshot::unknown);
                congestion.fetch_sub(1, Ordering::SeqCst);
                next
            })
        };
        (GamingMonitor::new(probe), peak)
    }

    fn wait_idle(monitor: &GamingMonitor) {
        for _ in 0..500 {
            if !monitor.in_flight() {
                // One grace quantum lets a coalesced rerun start.
                std::thread::sleep(Duration::from_millis(2));
                if !monitor.in_flight() {
                    return;
                }
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        panic!("worker did not go idle");
    }

    #[test]
    fn worker_starts_unknown_caches_pass_and_never_blocks() {
        let enabled = snapshot_with(XboxConfig::Enabled, GameBarPackage::Registered);
        let (monitor, _) = scripted_monitor(vec![enabled]);
        assert_eq!(monitor.snapshot(), GamingSnapshot::unknown());
        monitor.request_refresh(RefreshReason::Startup);
        // The request returns at once (never waited on during tiling).
        assert!(monitor.snapshot().generation <= 1);
        wait_idle(&monitor);
        let cached = monitor.snapshot();
        assert_eq!(cached.xbox, XboxConfig::Enabled);
        assert_eq!(cached.gamebar, GameBarPackage::Registered);
        assert_eq!(cached.generation, 1);
        // Completed lines queue for the owner drain; nothing was written by
        // the worker itself.
        let lines = monitor.drain_logs();
        assert_eq!(lines.len(), 2);
        assert!(lines.iter().all(|line| line["reason"] == "startup"));
        assert!(monitor.drain_logs().is_empty());
    }

    #[test]
    fn concurrent_requests_keep_at_most_one_probe_in_flight() {
        let script = vec![GamingSnapshot::unknown(); 8];
        let (monitor, peak) = scripted_monitor(script);
        std::thread::scope(|scope| {
            for _ in 0..8 {
                let monitor = &monitor;
                scope.spawn(move || {
                    monitor.request_refresh(RefreshReason::Recheck);
                });
            }
        });
        wait_idle(&monitor);
        // Every racing request is served (at least one pass ran) while the
        // probe itself never overlapped: at most one pass in flight.
        assert!(monitor.snapshot().generation >= 1);
        assert_eq!(peak.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn settings_revision_change_refreshes_steady_state_does_not() {
        let first = snapshot_with(XboxConfig::Disabled, GameBarPackage::Absent);
        let second = snapshot_with(XboxConfig::Enabled, GameBarPackage::Registered);
        // `pop` serves the tail first: the first pass reads `second`.
        let (monitor, _) = scripted_monitor(vec![first, second]);
        monitor.sync_revision(Some(7));
        monitor.maybe_refresh(Some(7));
        wait_idle(&monitor);
        // Seeded revision with no change and no prior pass: the interval arm
        // covers it exactly once.
        assert_eq!(monitor.snapshot().generation, 1);
        assert_eq!(monitor.snapshot().xbox, XboxConfig::Enabled);
        monitor.maybe_refresh(Some(7));
        wait_idle(&monitor);
        assert_eq!(monitor.snapshot().generation, 1);
        monitor.maybe_refresh(Some(8));
        wait_idle(&monitor);
        assert_eq!(monitor.snapshot().generation, 2);
        assert_eq!(monitor.snapshot().xbox, XboxConfig::Disabled);
    }

    fn gated_monitor() -> (
        GamingMonitor,
        Arc<AtomicUsize>,
        Arc<AtomicBool>,
        Arc<AtomicBool>,
    ) {
        // Probe that blocks until the test releases it, recording entries.
        // Deterministic gating for attempt-rate assertions below.
        let entered = Arc::new(AtomicBool::new(false));
        let release = Arc::new(AtomicBool::new(false));
        let calls = Arc::new(AtomicUsize::new(0));
        let probe: ProbeFn = {
            let entered = Arc::clone(&entered);
            let release = Arc::clone(&release);
            let calls = Arc::clone(&calls);
            Arc::new(move || {
                calls.fetch_add(1, Ordering::SeqCst);
                entered.store(true, Ordering::SeqCst);
                while !release.load(Ordering::SeqCst) {
                    std::thread::sleep(Duration::from_millis(1));
                }
                GamingSnapshot::unknown()
            })
        };
        (GamingMonitor::new(probe), calls, entered, release)
    }

    fn wait_flag(flag: &AtomicBool) {
        for _ in 0..500 {
            if flag.load(Ordering::SeqCst) {
                return;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        panic!("flag never set");
    }

    #[test]
    fn blocked_probe_suppresses_interval_requeue() {
        let (monitor, calls, entered, release) = gated_monitor();
        monitor.sync_revision(Some(3));
        monitor.request_refresh(RefreshReason::Startup);
        wait_flag(&entered);
        // The pass is blocked inside the probe: repeated pump triggers must
        // neither start another pass nor queue coalesced work (attempt-gated,
        // revision unchanged).
        for _ in 0..20 {
            monitor.maybe_refresh(Some(3));
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(!monitor.inner.pending.load(Ordering::SeqCst));
        assert!(monitor.drain_logs().is_empty());
        release.store(true, Ordering::SeqCst);
        wait_idle(&monitor);
        assert_eq!(monitor.snapshot().generation, 1);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        // An explicit recheck still runs afterwards with its honest reason.
        monitor.recheck();
        wait_idle(&monitor);
        assert_eq!(monitor.snapshot().generation, 2);
        let reasons: Vec<String> = monitor
            .drain_logs()
            .iter()
            .map(|line| line["reason"].as_str().unwrap_or("?").to_owned())
            .collect();
        assert_eq!(reasons, vec!["startup", "startup", "recheck", "recheck"]);
    }

    #[test]
    fn coalesced_rerun_keeps_latest_reason() {
        let (monitor, _, entered, release) = gated_monitor();
        monitor.request_refresh(RefreshReason::Startup);
        wait_flag(&entered);
        // Lands while the startup pass is blocked: exactly one coalesced
        // rerun carrying the latest reason.
        monitor.request_refresh(RefreshReason::SettingsChange);
        release.store(true, Ordering::SeqCst);
        wait_idle(&monitor);
        assert_eq!(monitor.snapshot().generation, 2);
        let reasons: Vec<String> = monitor
            .drain_logs()
            .iter()
            .map(|line| line["reason"].as_str().unwrap_or("?").to_owned())
            .collect();
        assert_eq!(
            reasons,
            vec!["startup", "startup", "settings-change", "settings-change"]
        );
    }

    #[test]
    fn spawn_failure_degrades_with_honest_line() {
        let probe: ProbeFn = Arc::new(GamingSnapshot::unknown);
        let spawner: SpawnerFn = Arc::new(|_| Err(std::io::Error::other("thread cap")));
        let monitor = GamingMonitor::with_spawner(probe, spawner);
        monitor.request_refresh(RefreshReason::Startup);
        // Synchronous degradation: no thread, no panic; unknown publishes
        // with a fresh generation so downstream redraws conservatively.
        assert!(!monitor.in_flight());
        let degraded = monitor.snapshot();
        assert_eq!(degraded.xbox, XboxConfig::Unknown);
        assert_eq!(degraded.gamebar, GameBarPackage::Unknown);
        assert_eq!(degraded.generation, 1);
        let lines = monitor.drain_logs();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0]["event"], "gaming-probe");
        assert_eq!(lines[0]["source"], "worker");
        assert_eq!(lines[0]["outcome"], "spawn-failed");
        assert_eq!(lines[0]["state"], "unknown");
        assert_eq!(lines[0]["reason"], "startup");
    }

    #[test]
    fn completion_hint_fires_after_pass_and_on_spawn_failure() {
        // Event-driven owners sleep behind an idle wait: every finished
        // pass (and every synchronous degradation) invokes the hint once
        // queued lines are ready to drain, so nothing strands.
        let fired = Arc::new(AtomicBool::new(false));
        let (monitor, _) = scripted_monitor(vec![GamingSnapshot::unknown()]);
        monitor.set_completion_notify(Some(Arc::new({
            let fired = Arc::clone(&fired);
            move || fired.store(true, Ordering::SeqCst)
        })));
        monitor.request_refresh(RefreshReason::Startup);
        wait_idle(&monitor);
        assert!(fired.load(Ordering::SeqCst), "pass completion hints");
        assert_eq!(monitor.drain_logs().len(), 2);
        // Synchronous spawn failure hints too (the honest line above must
        // also drain promptly).
        let failed = Arc::new(AtomicBool::new(false));
        let probe: ProbeFn = Arc::new(GamingSnapshot::unknown);
        let spawner: SpawnerFn = Arc::new(|_| Err(std::io::Error::other("thread cap")));
        let degraded = GamingMonitor::with_spawner(probe, spawner);
        degraded.set_completion_notify(Some(Arc::new({
            let failed = Arc::clone(&failed);
            move || failed.store(true, Ordering::SeqCst)
        })));
        degraded.request_refresh(RefreshReason::Recheck);
        assert!(failed.load(Ordering::SeqCst), "degradation hints");
        assert_eq!(degraded.drain_logs().len(), 1);
    }

    #[test]
    fn interval_due_tracks_attempt_and_suppresses_while_running() {
        // The owner deadline input: no deadline while a pass runs (the
        // completion hint wakes instead, so attempt-gating can never
        // busy-loop); otherwise the last attempt plus the interval.
        let (monitor, _, entered, release) = gated_monitor();
        monitor.request_refresh(RefreshReason::Startup);
        wait_flag(&entered);
        assert!(
            monitor.interval_due_at().is_none(),
            "running pass arms no deadline"
        );
        release.store(true, Ordering::SeqCst);
        wait_idle(&monitor);
        let due = monitor
            .interval_due_at()
            .expect("idle pass arms a due time");
        assert!(
            due > Instant::now() + REFRESH_INTERVAL - Duration::from_secs(10),
            "due is one interval past the completed attempt"
        );
    }

    #[test]
    fn stale_attempt_while_blocked_skips_interval() {
        let (monitor, calls, entered, release) = gated_monitor();
        monitor.sync_revision(Some(3));
        monitor.request_refresh(RefreshReason::Startup);
        wait_flag(&entered);
        // Simulate a probe blocked past the interval: the attempt clock is
        // stale, but the running pass must neither requeue interval work nor
        // leave pending set across pumps.
        *monitor
            .inner
            .last_attempt
            .lock()
            .unwrap_or_else(|poison| poison.into_inner()) = Some(
            Instant::now()
                .checked_sub(REFRESH_INTERVAL + Duration::from_secs(60))
                .expect("clock"),
        );
        for _ in 0..20 {
            monitor.maybe_refresh(Some(3));
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(!monitor.inner.pending.load(Ordering::SeqCst));
        assert!(monitor.drain_logs().is_empty());
        release.store(true, Ordering::SeqCst);
        wait_idle(&monitor);
        // No coalesced interval rerun followed the blocked pass.
        assert_eq!(monitor.snapshot().generation, 1);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn spawn_failure_after_off_publishes_unknown() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let spawner: SpawnerFn = {
            let attempts = Arc::clone(&attempts);
            Arc::new(move |body: ThreadBody| {
                if attempts.fetch_add(1, Ordering::SeqCst) == 0 {
                    std::thread::Builder::new().spawn(body)
                } else {
                    Err(std::io::Error::other("thread cap"))
                }
            })
        };
        let off = snapshot_with(XboxConfig::Disabled, GameBarPackage::Absent);
        let probe: ProbeFn = Arc::new(move || off);
        let monitor = GamingMonitor::with_spawner(probe, spawner);
        monitor.request_refresh(RefreshReason::Startup);
        wait_idle(&monitor);
        assert_eq!(monitor.snapshot().xbox, XboxConfig::Disabled);
        // Xbox row cleared while off is observed.
        assert!(
            tray_warning_conflicts(&authentic(), true, false, &monitor.snapshot())
                .iter()
                .all(|c| c.id != "toggle-fullscreen")
        );
        // A later spawn failure must not leave the stale off state hiding
        // the warning: unknown publishes at once with a new generation.
        monitor.request_refresh(RefreshReason::Recheck);
        assert_eq!(monitor.snapshot().xbox, XboxConfig::Unknown);
        assert_eq!(monitor.snapshot().generation, 2);
        let ids: Vec<&str> = tray_warning_conflicts(&authentic(), true, false, &monitor.snapshot())
            .iter()
            .map(|c| c.id)
            .collect();
        assert!(ids.contains(&"toggle-fullscreen"));
        assert!(
            monitor
                .drain_logs()
                .iter()
                .any(|line| line["outcome"] == "spawn-failed")
        );
    }

    #[test]
    fn probe_panic_degrades_unknown_nonfatal() {
        let probe: ProbeFn = Arc::new(|| panic!("simulated probe panic"));
        let monitor = GamingMonitor::new(probe);
        monitor.request_refresh(RefreshReason::Startup);
        // The panic must not strand the worker: in-flight clears, unknown
        // publishes, and one honest line queues.
        wait_idle(&monitor);
        assert_eq!(monitor.snapshot().xbox, XboxConfig::Unknown);
        assert_eq!(monitor.snapshot().generation, 1);
        let lines = monitor.drain_logs();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0]["source"], "worker");
        assert_eq!(lines[0]["outcome"], "probe-panicked");
        assert_eq!(lines[0]["state"], "unknown");
    }

    #[test]
    fn gamebar_registration_states_drive_win_g_conflicts() {
        // Detected installation warns as today.
        let registered = snapshot_with(XboxConfig::Unknown, GameBarPackage::Registered);
        let conflicts = gaming_conflicts(&authentic(), true, &registered);
        assert_eq!(conflicts.len(), 2);
        let float = conflicts
            .iter()
            .find(|c| c.id == "toggle-float")
            .expect("float warns when registered");
        assert_eq!(float.kind, GamingConflictKind::GameBar);
        // Successful absence clears the Win+G rows while Xbox unknown keeps
        // its fullscreen warning, on both surfaces.
        let absent = snapshot_with(XboxConfig::Unknown, GameBarPackage::Absent);
        for ids in [
            gaming_conflicts(&authentic(), true, &absent)
                .iter()
                .map(|c| c.id)
                .collect::<Vec<_>>(),
            tray_warning_conflicts(&authentic(), true, false, &absent)
                .iter()
                .map(|c| c.id)
                .collect::<Vec<_>>(),
        ] {
            assert_eq!(ids, vec!["toggle-fullscreen"]);
        }
        // Query failure is distinct from absence: the exact unknown warning
        // stays on both surfaces.
        let failed = GamingSnapshot::unknown();
        assert_eq!(failed.gamebar, GameBarPackage::Unknown);
        let conflicts = gaming_conflicts(&authentic(), true, &failed);
        let float = conflicts
            .iter()
            .find(|c| c.id == "toggle-float")
            .expect("float warns on query failure");
        assert_eq!(
            float.evidence,
            "Game Bar registration unknown (query failed); check required"
        );
        assert!(
            tray_warning_conflicts(&authentic(), true, false, &failed)
                .iter()
                .any(|c| c.id == "toggle-float")
        );
    }

    #[test]
    fn rebound_gamebar_chord_tracks_note_not_id() {
        let mut rebound = authentic();
        rebound.bindings.insert(
            "toggle-float".to_owned(),
            BindingSetting {
                state: BindingState::Disabled,
                chord: None,
            },
        );
        rebound.bindings.insert(
            "toggle-maximize".to_owned(),
            BindingSetting {
                state: BindingState::Rebind,
                chord: Some("Win+G".to_owned()),
            },
        );
        assert!(validate_settings(&rebound).is_ok());
        let conflicts = gaming_conflicts(&rebound, true, &unknown_snapshot());
        let ids: Vec<&str> = conflicts.iter().map(|c| c.id).collect();
        assert_eq!(ids, vec!["toggle-maximize", "toggle-fullscreen"]);
        let absent = snapshot_with(XboxConfig::Unknown, GameBarPackage::Absent);
        let ids: Vec<&str> = gaming_conflicts(&rebound, true, &absent)
            .iter()
            .map(|c| c.id)
            .collect();
        assert_eq!(ids, vec!["toggle-fullscreen"]);
        let ids: Vec<&str> = tray_warning_conflicts(&rebound, true, false, &absent)
            .iter()
            .map(|c| c.id)
            .collect();
        assert_eq!(ids, vec!["toggle-fullscreen"]);
    }
}

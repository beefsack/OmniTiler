#![forbid(unsafe_code)]

//! Phase 1 runtime public types: configuration, typed handoffs,
//! diagnostics, and outcome tokens.
//!
//! Pure data plus validation only: no threads, locks, or sinks. The
//! supervisor surface in [`crate::runtime`] re-exports every public item
//! here, so paths like `crate::runtime::RuntimeConfig` keep working.

use std::collections::BTreeMap;
use std::time::Duration;

use tiler_core::geometry::Rect;
use tiler_core::size_hints::{FixedSizePredicate, WindowSizeHints};

use crate::config::ProbeModifier;

// -- Phase-1 single-domain skeleton constants --------------------------------
//
// One fixed domain stands in for display/Space topology until platform
// policy extraction lands. These are placeholders, not policy.

/// Skeleton output identity carried into every [`CoreEvent`].
pub const PHASE1_OUTPUT: &str = "macos-main";
/// Skeleton workspace identity carried into every [`CoreEvent`].
pub const PHASE1_WORKSPACE: &str = "macos-space-1";
/// Core owner binding for the skeleton engine.
pub const PHASE1_OWNER: &str = "macos-phase-1";
/// Core generation binding for the skeleton engine.
pub const PHASE1_GENERATION: &str = "phase-1";
/// Skeleton domain bounds (adapter-normalized integer units).
pub const PHASE1_BOUNDS: Rect = Rect {
    x: 0,
    y: 0,
    w: 1920,
    h: 1080,
};
/// Skeleton inner gap.
pub const PHASE1_GAP: i32 = 8;
/// Skeleton outer gap.
pub const PHASE1_OUTER_GAP: i32 = 8;

/// Advisory per-AX-call bound. Mirrors the existing 2 s element messaging
/// timeout used by the probe AX paths; backends enforce their own finite
/// timeout and report honestly.
pub const AX_CALL_TIMEOUT: Duration = Duration::from_secs(2);

// -- Typed handoffs ------------------------------------------------------------

/// Typed incoming app lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleEvent {
    Launch { pid: u32 },
    Terminate { pid: u32 },
}

/// One observed window: opaque IDs plus integer frame plus the normalized
/// classifier facts the core needs. No titles, no content, no PII.
///
/// `floating` carries normalized float exceptions (dialogs, sheets, modals);
/// `fullscreen` carries the native overlay flag truthfully on every shape;
/// `hints` carries the current size-hint vector for the core's
/// admission-only fixed classifier (D1/D2/D5 freshness). The wire layer
/// holds born fullscreen arrivals slotless (synthetic floating, never
/// automatic) and asserts first-sight fixed origins; retained overlays keep
/// their reserved slot/allocation in plan truth with no geometry writes.
/// Minimized, hidden-app, policy/own-bundle, and layer exclusions never
/// reach this shape: the classifier omits them from the submission instead
/// (the core folds absence into minimize/restore, never into close).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObservedWindow {
    pub wid: u32,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub floating: bool,
    pub fullscreen: bool,
    pub hints: WindowSizeHints,
}

/// Typed AX observation for one app. `generation` is the originating app
/// lifetime minted at launch: the runtime rejects observations whose
/// generation does not match the live generation, so a late observation from
/// a previous lifetime can never be stamped with the new generation after a
/// PID relaunch. `seq` is a per-app monotonic sequence; the runtime rejects
/// anything that is not newer than the last applied sequence for the current
/// generation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AxObservation {
    pub pid: u32,
    pub generation: u64,
    pub seq: u64,
    pub windows: Vec<ObservedWindow>,
    pub focused_wid: Option<u32>,
}

/// Step direction for semantic input actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputDirection {
    Left,
    Right,
    Up,
    Down,
}

impl InputDirection {
    /// Core direction token (`left`/`right`/`up`/`down`).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
            Self::Up => "up",
            Self::Down => "down",
        }
    }
}

/// Typed semantic input action. Produced by the input thread from exact
/// chord matches on key down only; the input thread never touches the core.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputSemanticAction {
    FocusStep { direction: InputDirection },
    MoveStep { direction: InputDirection },
}

/// Typed frame command to one app worker. `generation` fences PID reuse:
/// a worker applies only commands for the generation it was spawned with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameCommand {
    pub pid: u32,
    pub generation: u64,
    pub cmd_id: u64,
    pub window: String,
    pub wid: u32,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// Typed focus request out of the engine. `generation` fences PID reuse and
/// `request_id` is a per-runtime monotonic identity: queued requests are
/// purged on app terminate/shutdown and re-checked for freshness when
/// drained, so a stale request can never dispatch after relaunch.
/// `dispatched` means queued for the native dispatcher; `verified` is always
/// false in the skeleton — only a later [`AxObservation`] can verify focus.
/// Dispatch must never be read as arrival.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FocusRequest {
    pub pid: u32,
    pub generation: u64,
    pub request_id: u64,
    pub wid: u32,
    pub window: String,
    pub dispatched: bool,
    pub verified: bool,
}

/// Honest AX completion: dispatch truth only (accepted, refused, timed
/// out). A completion never substitutes observed truth, which arrives
/// exclusively through [`AxObservation`]. Reasons carry OS status names
/// only, never user content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AxCompletion {
    Applied { elapsed_ms: u64 },
    Failed { reason: String },
    Timeout { timeout_ms: u64 },
}

/// Opaque window identity minted by the runtime: `"<pid>_<wid>"`. The
/// separator is `_` (not `:`) so the identity stays inside the core
/// opaque-ID alphabet (`is_opaque_id`: alphanumeric plus `-_.`), which the
/// focus/move paths validate before proposing.
#[must_use]
pub fn window_id(pid: u32, wid: u32) -> String {
    format!("{pid}_{wid}")
}

/// Parse a runtime-minted window identity back into `(pid, wid)`.
#[must_use]
pub fn parse_window_id(value: &str) -> Option<(u32, u32)> {
    let (pid, wid) = value.split_once('_')?;
    Some((pid.parse().ok()?, wid.parse().ok()?))
}

// -- Configuration -------------------------------------------------------------

/// Runtime configuration. All queues are bounded; all waits are finite.
#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    /// Bounded engine inbox depth for data jobs (observations, input
    /// actions, worker results). Full queues drop with a counted outcome.
    pub engine_queue_depth: usize,
    /// Bounded outbox depth for focus requests. Full queues drop counted.
    pub focus_outbox_depth: usize,
    /// How long `launch_app` waits for a worker to create its backend
    /// inside the owning thread before failing the launch.
    pub worker_ready_timeout: Duration,
    /// How long [`Runtime::start`] waits for the engine and input owners to
    /// signal readiness before failing the start (bounded, configurable so
    /// tests can use a short bound against a blocked factory). The same
    /// bound limits failed-start cleanup.
    pub startup_ready_timeout: Duration,
    /// Bounded async normal-log queue depth. Senders use `try_send` only:
    /// a full queue drops counted and never stalls the engine/input/apps.
    pub log_queue_depth: usize,
    /// Modifier filling the Ctrl role for input chords (default Meta).
    pub modifier: ProbeModifier,
    /// Key code of the exact focus-step chord.
    pub focus_key: u32,
    /// Key code of the exact move-step chord.
    pub move_key: u32,
    /// Direction carried by focus/move steps.
    pub step_direction: InputDirection,
    /// Shared fixed-size admission predicate for the skeleton engine (D1).
    /// Both-axes default (current delivered behavior); changing it affects
    /// subsequent admissions only, never existing classifications.
    pub fixed_size_predicate: FixedSizePredicate,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            engine_queue_depth: 64,
            focus_outbox_depth: 16,
            worker_ready_timeout: Duration::from_secs(5),
            startup_ready_timeout: Duration::from_secs(5),
            log_queue_depth: crate::runtime_log::LOG_SINK_CAP,
            modifier: ProbeModifier::Meta,
            focus_key: crate::input::PROBE_KEY_CODE,
            move_key: 0x05,
            step_direction: InputDirection::Right,
            fixed_size_predicate: FixedSizePredicate::BothAxes,
        }
    }
}

impl RuntimeConfig {
    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.engine_queue_depth == 0 {
            return Err("engine_queue_depth must be nonzero".to_owned());
        }
        if self.focus_outbox_depth == 0 {
            return Err("focus_outbox_depth must be nonzero".to_owned());
        }
        if self.focus_key == self.move_key {
            return Err("focus_key and move_key must differ".to_owned());
        }
        if self.startup_ready_timeout.is_zero() {
            return Err("startup_ready_timeout must be nonzero".to_owned());
        }
        if self.log_queue_depth == 0 {
            return Err("log_queue_depth must be nonzero".to_owned());
        }
        if self.focus_key > 255 || self.move_key > 255 {
            return Err("chord key codes must fit 0..=255".to_owned());
        }
        Ok(())
    }
}

// -- Diagnostics ---------------------------------------------------------------

/// Snapshot of structured runtime counters. Bounded counts and static
/// outcome tokens only; never native identifiers beyond PIDs/WIDs already
/// present in typed handoffs, never user content.
#[derive(Debug, Clone, Default)]
pub struct RuntimeDiagnostics {
    pub engine_jobs_processed: u64,
    pub data_dropped_full: u64,
    pub result_dropped_full: u64,
    pub invalidated_on_stop: u64,
    pub frames_emitted: u64,
    pub frames_coalesced: u64,
    /// Desired frame writes skipped because the desired geometry already
    /// matches the last observed geometry (redundant once observers report
    /// back; a native clamp converges instead of rewriting endlessly).
    pub frames_redundant_skipped: u64,
    /// Desired frame writes withheld because the target window is a native
    /// overlay ([`tiler_core::window_policy::geometry_write_allowed`]
    /// refuses): retained fullscreen members keep their slot with no target
    /// geometry. Plan truth is untouched (only emission is filtered).
    pub frames_overlay_skipped: u64,
    pub focus_dispatched: u64,
    pub focus_dropped_full: u64,
    pub stale_dropped: u64,
    pub terminated_invalidated: u64,
    pub input_forwarded: u64,
    pub input_ignored: u64,
    pub input_refused_no_focus: u64,
    /// Backend-generated observations published to the engine by app workers
    /// (startup, streaming, and readback handoffs).
    pub observations_published: u64,
    /// Backend observation failures (snapshot/read errors that preserved
    /// retained state instead of publishing).
    pub observation_failures: u64,
    /// Unexpected end-of-stream exits (input source `None` without stop, or
    /// an engine/input owner exit that was never asked to stop). Counted and
    /// logged, never spun on.
    pub unexpected_eos: u64,
    /// Core reply outcomes by static variant token.
    pub core_replies: BTreeMap<String, u64>,
    /// Honest backend completion outcomes by static token.
    pub completions: BTreeMap<String, u64>,
    pub worker_panics_observed: u64,
    /// Returned-unclean teardown evidence (worker `shutdown()` refused).
    /// Distinct from panics, persistent like them: a reaped unclean worker
    /// still poisons the terminal shutdown.
    pub worker_unclean_observed: u64,
    /// Normal-log sink drops under saturation (bounded queue full or sink
    /// gone). The ring entry is still kept; only the async copy drops.
    pub logger_dropped: u64,
    pub apps_live: usize,
    pub log_rotated: u64,
}

/// One structured lifecycle log entry. Enough to reconstruct recovery
/// (app, generation, reference sequence/command id, outcome); no content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifecycleLogEntry {
    pub seq: u64,
    /// Monotonic milliseconds since runtime start (ring correlation aid).
    pub timestamp_ms: u64,
    pub pid: u32,
    pub generation: u64,
    pub ref_id: u64,
    pub kind: &'static str,
    pub outcome: &'static str,
    pub count: u64,
}

/// Last desired geometry snapshot: the authoritative core plan the runtime
/// is actuating toward. Windows plus rects only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesiredFrame {
    pub window: String,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

// -- Outcomes ------------------------------------------------------------------

/// Error starting the runtime or launching an app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartupError {
    InvalidConfig(String),
    EngineSpawn(String),
    EngineNotReady,
    InputSpawn(String),
    InputRefused(String),
    WorkerSpawn(String),
    BackendRefused(String),
    WorkerNotReady,
    EngineQueueFull,
    PreviousGenerationRunning { pid: u32, generation: u64 },
    AdmissionClosed,
}

impl std::fmt::Display for StartupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidConfig(r) => write!(f, "invalid runtime config: {r}"),
            Self::EngineSpawn(r) => write!(f, "engine thread spawn failed: {r}"),
            Self::EngineNotReady => write!(f, "engine thread did not signal readiness"),
            Self::InputSpawn(r) => write!(f, "input thread spawn failed: {r}"),
            Self::InputRefused(r) => write!(f, "input source refused startup: {r}"),
            Self::WorkerSpawn(r) => write!(f, "app worker spawn failed: {r}"),
            Self::BackendRefused(r) => write!(f, "AX backend refused startup: {r}"),
            Self::WorkerNotReady => write!(f, "app worker did not signal readiness"),
            Self::EngineQueueFull => write!(f, "engine queue full; launch refused"),
            Self::PreviousGenerationRunning { pid, generation } => write!(
                f,
                "pid {pid} generation {generation} still executes; refusing overlap"
            ),
            Self::AdmissionClosed => write!(f, "runtime is stopped; admission closed"),
        }
    }
}

impl std::error::Error for StartupError {}

/// Outcome of submitting data to the engine inbox.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubmitOutcome {
    Accepted,
    EngineQueueFull,
    UnknownApp,
    AppTerminated,
    /// The observation carries a stale app lifetime (PID relaunched since).
    StaleGeneration,
    AdmissionClosed,
}

/// Outcome of terminating an app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminateOutcome {
    Terminated { generation: u64 },
    UnknownPid,
    AlreadyTerminated,
    RuntimeStopped,
}

/// Readiness of one app worker's backend (created inside the owning thread).
/// Returned by [`RuntimeHandle::worker_ready`] for the nonblocking launch
/// path; the synchronous [`RuntimeHandle::launch_app`] waits for `Ready`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkerReadiness {
    UnknownPid,
    Pending,
    Ready,
    Failed(String),
}

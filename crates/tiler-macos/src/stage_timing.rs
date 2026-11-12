#![forbid(unsafe_code)]

//! Opt-in per-action stage timing (offline, portable).
//!
//! One user-run session with `TILER_MACOS_STAGE_TIMING=1` emits compact
//! per-stage records through the existing bounded async [`LogSink`](crate::runtime_log::LogSink)
//! path (same ring order, same counted drops, never synchronous logging in a
//! tap callback). Records join by numeric id: input-correlated stages share
//! the minted trace id as `ref_id`; worker apply records reuse the frame
//! `cmd_id` / focus `request_id` they already carry, and the engine emits
//! one bind record each mapping those back to the trace id. Feedback
//! arrivals (`AxResult`, `FocusResult`, drain evidence) emit settle records
//! under the trace through bounded shared maps, so one trace spans
//! receipt, queue, mapping, dispatch, native apply, and settlement. The
//! engine `timing-done` record is dispatch return only, never native
//! completion: the per-action input-to-feedback-arrival total is the settle
//! record (`timing-frame-settle` / `timing-focus-settle` /
//! `timing-op-settle`), whose `count` is microseconds from the trace origin
//! (receipt stamp) to settlement. Never read `timing-done` as
//! input-to-native latency, and never read a settle record as action done:
//! worker feedback is queued before the worker's post-batch snapshot, so the
//! post-batch readback has a separate `timing-observe` duration and
//! `timing-readback-total` origin-to-ready endpoint, never part of settle.
//!
//! Clocks: one process monotonic microsecond origin ([`now_us`]) shared by
//! the input, engine, and worker threads, so intake stamps, queue math, and
//! apply durations subtract directly. The sink's `t=` millisecond stamp is a
//! different (runtime-relative) origin and must never be mixed with `t0_us`.
//!
//! Disabled (the default) every helper costs one cached flag load and
//! performs no timestamp read, no formatting, and no emission. Nothing here
//! changes behavior: all timings are observations around existing calls.
//!
//! Privacy: stage and outcome tokens are a fixed static set; the action
//! token names the command class only (no index, output, workspace, title,
//! or bundle content); ids are opaque numerics (`trace id`, `cmd_id`,
//! `request_id`, `op-<seq>` numbers).

use std::cell::RefCell;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};
use std::time::Instant;

use crate::runtime::{Shared, emit};
use crate::runtime_types::InputSemanticAction;

/// Environment switch enabling stage timing. Any of `1`, `true`, `yes`,
/// `on` (case-insensitive, surrounding whitespace ignored).
pub const ENV_SWITCH: &str = "TILER_MACOS_STAGE_TIMING";

/// Per-stage record kinds. Every kind carries the `timing-` prefix so one
/// filter selects the whole schema.
pub mod stage {
    /// Tap/hotkey/indicator RECEIPT: stamped in the tap callback itself
    /// (`TapHit.tap_us`), before channel, pump, and chord mapping (`count`
    /// is always 0; the record marks the trace origin). Mapping duration
    /// rides `timing-input-map`. Non-tap ingress (programmatic submits)
    /// stamps at submit time. The callback stamp costs one flag load plus
    /// one clock read when enabled; it never logs synchronously.
    /// Host-change intake (no tap/notification callback exists on that
    /// path): poll-result receipt, stamped by `mint_observer_trace` and
    /// recorded by the engine `ScopedObservation` arm before `timing-queue`
    /// (`count` is likewise always 0). Source acquisition inside the poll
    /// is excluded (see `mint_observer_trace`).
    pub const INTAKE: &str = "timing-intake";
    /// Input-thread chord mapping: exact-match decision plus admission
    /// enqueue (`count` is microseconds from the callback receipt stamp to
    /// enqueue).
    pub const INPUT_MAP: &str = "timing-input-map";
    /// Engine inbox queue wait (`count` is microseconds from the admission
    /// enqueue stamp to engine dequeue: pure queue wait, never mapping).
    /// Receipt-to-enqueue rides `timing-input-map`; receipt-to-dequeue is
    /// their sum.
    pub const QUEUE: &str = "timing-queue";
    /// Input-to-command mapping/admission branch on the engine thread.
    pub const MAPPING: &str = "timing-mapping";
    /// World report construction (`build_dispatch_report`).
    pub const REPORT: &str = "timing-report";
    /// Codec report encode leg.
    pub const CODEC_REPORT_ENC: &str = "timing-codec-report-enc";
    /// Codec report decode leg.
    pub const CODEC_REPORT_DEC: &str = "timing-codec-report-dec";
    /// Core engine call (`handle_report`).
    pub const CORE: &str = "timing-core";
    /// Codec reply encode leg.
    pub const CODEC_REPLY_ENC: &str = "timing-codec-reply-enc";
    /// Codec reply decode leg.
    pub const CODEC_REPLY_DEC: &str = "timing-codec-reply-dec";
    /// Sequencer acceptance plus the emission-coherence gate.
    pub const ACCEPT: &str = "timing-accept";
    /// Reply consume: frame/focus publication, slot/outbox writes, claim
    /// binding, presentation submit (the applied reply's presentation is
    /// published here; this is submit, never painted proof - no capture
    /// proves pixels arrived).
    pub const PUBLISH: &str = "timing-publish";
    /// Whole dispatch (`dispatch_ticket`); `outcome` carries the static
    /// verdict (`applied`/`duplicate`/`refused`/`no-report`), not the
    /// action token.
    pub const DISPATCH: &str = "timing-dispatch";
    /// Whole input action from receipt stamp to engine dispatch return;
    /// `outcome` carries the static engine-side terminal token (a refusal
    /// token or `dispatched`), not the action token. Engine dispatch is
    /// NOT native completion: worker apply (`timing-frame-apply` /
    /// `timing-focus-apply`), drain settlement, and observed readback
    /// arrive later under the bind records below. Never read this as
    /// input-to-native latency.
    pub const DONE: &str = "timing-done";
    /// Frame bind: maps worker `cmd_id` (`ref_id`, with app `pid`) back to
    /// the trace id (`count`).
    pub const FRAME_BIND: &str = "timing-frame-bind";
    /// Worker frame apply: one native size-position-size attempt
    /// (`ref_id` is `cmd_id`, `count` is microseconds).
    pub const FRAME_APPLY: &str = "timing-frame-apply";
    /// Focus bind: maps worker `request_id` (`ref_id`) back to the trace
    /// id (`count`).
    pub const FOCUS_BIND: &str = "timing-focus-bind";
    /// Worker focus dispatch (`ref_id` is `request_id`, `count` is
    /// microseconds).
    pub const FOCUS_APPLY: &str = "timing-focus-apply";
    /// Worker post-batch observation readback (aggregate over the batch,
    /// not per window; `ref_id` is the first batch `cmd_id`, else 0).
    pub const OBSERVE: &str = "timing-observe";
    /// Select/send operation bind: maps the numeric `op-<seq>` (`count`)
    /// back to the trace id (`ref_id`) for drain-side joins.
    pub const OP_BIND: &str = "timing-op-bind";
    /// Worker frame settlement: the owning worker's honest `AxResult`
    /// arriving back on the engine thread. This is the action's
    /// input-to-feedback-arrival total: `ref_id` is the trace, `count` is
    /// microseconds from the trace origin (callback receipt) to result
    /// arrival, `outcome` is the static completion class
    /// (applied/partial/failed/timeout - never inferred arrival: a dispatch
    /// success is not painted proof). It is not action done: the owning
    /// `AxResult` is queued before the worker's post-batch snapshot, so the
    /// post-batch readback rides the separate `timing-observe` endpoint
    /// under the batch identity, never inside this total. Joins to the trace
    /// through the bounded cmd map (see `note_cmd`); unmapped results emit
    /// nothing. Timeouts, noops (duplicate/refused, no worker attempt), and
    /// incomplete (partial) completions are distinguished by `outcome`, and
    /// an action with no worker attempt emits no settle record at all.
    pub const FRAME_SETTLE: &str = "timing-frame-settle";
    /// Worker focus settlement: the owning worker's `FocusResult` arrival.
    /// Same input-to-feedback-arrival convention as frame settle (origin to
    /// arrival, never dispatch return, never the post-batch readback).
    /// Joins through the bounded focus map.
    pub const FOCUS_SETTLE: &str = "timing-focus-settle";
    /// Drain settlement: proven select/send drain evidence (`ViewAttempt`
    /// / `MemberAttempt`) staged for the next report. Same
    /// input-to-feedback-arrival convention (origin to staging). Joins
    /// through the bounded op map; unknown ops emit nothing. The total
    /// INCLUDES the send/select lane's fixed waits (drag fallback
    /// 50+400 ms holds, up-to-3000 ms effect poll, select
    /// arrival/Mission-Control-exit waits): the lane's exec class plus its
    /// native accumulation ride the separate lane report (`timing-native`
    /// split under the same trace via `flush_lane_native`). Named wait
    /// buckets are also emitted there; unclocked waits remain inclusive.
    pub const OP_SETTLE: &str = "timing-op-settle";
    /// Native AX accumulation window on one owner thread, split by honest
    /// call class (`outcome` names it: `ax-read`, `ax-set`, `ax-focus`).
    /// `count` is total AX microseconds since the last flush/reset; counts
    /// ride the `-n` companion as instrumented calls (1 per call, never
    /// windows observed). Whole-inventory reads are one call each and are
    /// inclusive of their internal legs: never wrap both a self-clocked
    /// helper and its caller. Flushed once per worker batch under the
    /// batch's command identity; absent when the window accumulated nothing.
    pub const NATIVE_AX: &str = "timing-native-ax";
    /// Native AX call count companion to `timing-native-ax` (`count` is
    /// instrumented calls in the flushed class, 1 per call).
    pub const NATIVE_AX_N: &str = "timing-native-ax-n";
    /// Window-server (CG list) accumulation window: same convention as AX
    /// (`outcome` is the static `cg` token).
    pub const NATIVE_CG: &str = "timing-native-cg";
    /// CG call count companion to `timing-native-cg`.
    pub const NATIVE_CG_N: &str = "timing-native-cg-n";
    /// Fixed-wait accumulation window, split by honest wait class
    /// (`outcome` names it: `grab-settle`, `animation-hold`,
    /// `arrival-poll`, `select-latch`). Lanes without sink access keep
    /// their waits inside the `timing-op-settle` total with no split
    /// (see above).
    pub const NATIVE_WAIT: &str = "timing-native-wait";
    /// Fixed-wait count companion to `timing-native-wait`
    /// (`count` is bounded sleep/poll calls in the flushed class).
    pub const NATIVE_WAIT_N: &str = "timing-native-wait-n";
    /// Whole select/send lane execution (owner-thread exec window around
    /// the lane job, flushed under the dispatch trace via
    /// `flush_lane_native`; `outcome` is the static lane class).
    /// Never mixed with the actual-wait split above: this is lane exec,
    /// not a wait.
    pub const LANE_EXEC: &str = "timing-lane-exec";
    /// Worker post-batch observed readback total: input-to-post-batch
    /// snapshot-ready per unique parent trace in the batch (`ref` is the
    /// trace, `count` is origin to just-after-snapshot, `outcome` is
    /// `snapshot`/`failed`). No new syscall/read; redundant-skipped
    /// commands (no `AxResult`) still join. Dispatch success stays
    /// distinct from observed convergence/paint: readiness only, never
    /// target equality.
    pub const READBACK_TOTAL: &str = "timing-readback-total";
}

/// Dispatch-done outcomes. Static only: refusal detail stays in the
/// existing `core-reply` lines, never here.
pub mod outcome {
    /// A reply applied (frames/focus/transfers consumed).
    pub const APPLIED: &str = "applied";
    /// Duplicate of the latest reply: no state change.
    pub const DUPLICATE: &str = "duplicate";
    /// Core or sequencer refusal: counted, no state change.
    pub const REFUSED: &str = "refused";
    /// No report built (empty context): never actuated.
    pub const NO_REPORT: &str = "no-report";
    /// Dispatched toward the core (input-level done before the verdict).
    pub const DISPATCHED: &str = "dispatched";
}

/// One process-wide trace-id counter shared by [`mint`], [`mint_at`], and
/// [`ensure_span`]: every span mints here, so ids are unique across tap,
/// programmatic, indicator, and engine-side origins. Starts at 1; 0 is
/// never minted (0 means untraced on every wire).
static NEXT_TRACE_ID: AtomicU64 = AtomicU64::new(1);

/// Mint the next unique trace id (never 0 short of 2^64 actions).
fn next_trace_id() -> u64 {
    NEXT_TRACE_ID.fetch_add(1, Ordering::Relaxed)
}

/// Pure switch-value check (the cached [`enabled`] wraps this).
#[must_use]
pub fn switch_value(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

/// Whether stage timing is enabled. Read once per process from
/// [`ENV_SWITCH`]; every disabled-path helper returns after this load.
#[must_use]
pub fn enabled() -> bool {
    static FLAG: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *FLAG.get_or_init(|| std::env::var(ENV_SWITCH).is_ok_and(|v| switch_value(&v)))
}

/// Monotonic microsecond clock shared by every stage. One process origin
/// (first call wins) so intake stamps (input thread), queue math (engine
/// thread), and apply durations (worker threads) subtract directly.
#[must_use]
pub fn now_us() -> u64 {
    static BASE: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    let base = BASE.get_or_init(Instant::now);
    base.elapsed().as_micros().min(u128::from(u64::MAX)) as u64
}

/// One traced input action: the opaque correlation id, the static action
/// class token, and the intake stamp (`t0_us`, microseconds since the
/// module clock origin).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActionSpan {
    pub id: u64,
    pub action: &'static str,
    pub t0_us: u64,
}

/// Mint a span for one mapped input action, or `None` when disabled.
/// Disabled costs the flag load only (no timestamp read). The stamp is the
/// receipt time threaded in by the caller (see `intake_stamp`): intake is
/// stamped BEFORE mapping, never at mapping.
#[must_use]
pub fn mint(action: &InputSemanticAction) -> Option<ActionSpan> {
    if !enabled() {
        return None;
    }
    let id = next_trace_id();
    Some(ActionSpan {
        id,
        action: action_token(action),
        t0_us: now_us(),
    })
}

/// Receipt stamp for one incoming input event, taken BEFORE chord mapping.
/// Disabled: one flag load, no clock read, returns 0 (untraced).
#[must_use]
pub fn intake_stamp() -> u64 {
    if !enabled() {
        return 0;
    }
    now_us()
}

/// Admission enqueue stamp, taken immediately BEFORE the inbox send so the
/// engine's `timing-queue` record is pure enqueue-to-dequeue wait (receipt-
/// to-enqueue rides the input-side mapping record). Same disabled cost as
/// [`intake_stamp`].
#[must_use]
pub fn enqueue_stamp() -> u64 {
    intake_stamp()
}

/// Mint one observer trace for a host-side poll result (scoped observation
/// intake): one id off the shared counter plus the poll-result receipt
/// stamp (`None` when disabled). There is no tap/notification callback on
/// this path, so poll-result receipt IS intake; source acquisition itself
/// (AX/CG reads inside the poll) is NOT separately clocked on this path -
/// the worker accumulation window cannot attribute those reads to the poll
/// (they flush under worker command identities or drop when a batch carries
/// none), so the host total runs poll-result receipt to dispatch and
/// excludes acquisition. Pure apart from the shared id counter
/// and one clock read; never 0.
#[must_use]
pub fn mint_observer_trace() -> Option<(u64, u64)> {
    if !enabled() {
        return None;
    }
    let id = next_trace_id();
    if id == 0 {
        return None;
    }
    Some((id, now_us()))
}

/// Mint a span for one mapped input action with an explicit receipt stamp
/// (`t0_us` from `intake_stamp`, 0 when disabled). Returns `None` when
/// disabled or when the stamp is 0 (untraced). Pure apart from the id
/// counter; never reads the clock.
#[must_use]
pub fn mint_at(action: &InputSemanticAction, t0_us: u64) -> Option<ActionSpan> {
    if !enabled() || t0_us == 0 {
        return None;
    }
    let id = next_trace_id();
    if id == 0 {
        return None;
    }
    Some(ActionSpan {
        id,
        action: action_token(action),
        t0_us,
    })
}

/// Testable mint core: the [`mint`] body with its inputs threaded through.
#[must_use]
pub fn mint_inner(
    enabled_flag: bool,
    action: &'static str,
    id: u64,
    t0_us: u64,
) -> Option<ActionSpan> {
    if !enabled_flag || id == 0 {
        return None;
    }
    Some(ActionSpan { id, action, t0_us })
}

/// Static action-class token for one semantic action. Class only: indexes,
/// outputs, workspaces, directions, and flags never reach the sink.
#[must_use]
pub fn action_token(action: &InputSemanticAction) -> &'static str {
    match action {
        InputSemanticAction::FocusStep { .. } => "focus-step",
        InputSemanticAction::MoveStep { .. } => "move-step",
        InputSemanticAction::ToggleFloat => "toggle-float",
        InputSemanticAction::ResizeStep { .. } => "resize-step",
        InputSemanticAction::ToggleOrientation => "toggle-orientation",
        InputSemanticAction::WorkspaceSelect { .. } => "select",
        InputSemanticAction::IndicatorSelectWorkspace { .. } => "indicator-select",
        InputSemanticAction::WorkspaceSend { .. } => "send",
        InputSemanticAction::WorkspaceRelative { .. } => "relative",
        InputSemanticAction::WorkspaceSendRelative { .. } => "send-relative",
        InputSemanticAction::WorkspacePrevious => "previous",
        InputSemanticAction::WorkspaceOutput { .. } => "output-send",
    }
}

thread_local! {
    static CURRENT: RefCell<Option<ActionSpan>> = const { RefCell::new(None) };
    static LEG_BUFFER: RefCell<Vec<(&'static str, u64)>> = const { RefCell::new(Vec::new()) };
}

/// Shared correlation registries (see the module docs above). One bounded
/// vector per map behind its own process-wide `Mutex`: dispatch notes on the
/// engine thread, feedback joins on the engine thread, and lane/drain joins
/// plus the deferred-focus release bind on the main thread.
static CMD_MAP: Mutex<Vec<(TraceKey, u64, u64, u64)>> = Mutex::new(Vec::new());
static FOCUS_MAP: Mutex<Vec<(TraceKey, u64, u64, u64)>> = Mutex::new(Vec::new());
static OP_MAP: Mutex<Vec<(u64, u64, u64, u64)>> = Mutex::new(Vec::new());

/// RAII guard for the engine-thread current span: installs `span` for the
/// synchronous dispatch below and restores the previous span on drop.
pub struct TraceGuard {
    prev: Option<ActionSpan>,
}

impl Drop for TraceGuard {
    fn drop(&mut self) {
        CURRENT.with(|cell| *cell.borrow_mut() = self.prev);
    }
}

/// Install `span` as the current thread's trace span.
#[must_use]
pub fn enter(span: ActionSpan) -> TraceGuard {
    let prev = CURRENT.with(|cell| cell.borrow_mut().replace(span));
    TraceGuard { prev }
}

/// Run `f` over the current thread's trace span (`None` when no span is
/// installed or timing is disabled).
pub fn with_current<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&ActionSpan) -> R,
{
    if !enabled() {
        return None;
    }
    CURRENT.with(|cell| cell.borrow().as_ref().map(f))
}

/// Current thread's trace id, if any.
#[must_use]
pub fn current_id() -> Option<u64> {
    with_current(|span| span.id)
}

/// Ensure the current thread holds a trace span: reuse the installed one
/// (input-correlated dispatch or intake-correlated scoped-observation
/// dispatch) or mint an engine-side span naming `op`
/// (legacy-observation/reflow/feedback dispatches carry no intake stamp, so the
/// span origin is dispatch entry and no queue record is emitted). Returns
/// `None` when disabled (no mint, no clock read). The returned guard (when
/// `Some`) holds the minted span installed across the synchronous dispatch.
pub fn ensure_span(op: &'static str) -> Option<(ActionSpan, Option<TraceGuard>)> {
    if !enabled() {
        return None;
    }
    if let Some(span) = CURRENT.with(|cell| *cell.borrow()) {
        return Some((span, None));
    }
    let span = ActionSpan {
        id: next_trace_id(),
        action: op,
        t0_us: now_us(),
    };
    let guard = enter(span);
    Some((span, Some(guard)))
}

/// Bounded shared join maps: dispatch binds a worker/drain identity to
/// the dispatching trace id plus the trace origin stamp, so later feedback
/// (`AxResult`, `FocusResult`, `ViewAttempt`) emits a settle record carrying
/// the per-action input-to-feedback-arrival total (origin to arrival). Shared
/// across the engine, main, and worker threads (one bounded `Mutex` per map,
/// FIFO evicting at `MAP_CAP`): the engine-thread dispatch writes, the
/// engine-thread feedback arms and the main-thread lane/drain arms read, and
/// the main-thread deferred-focus release writes through the completion
/// identity. `CURRENT` and `LEG_BUFFER` stay thread-local (per-thread span
/// only); only these registries are shared. Disabled: every helper returns
/// after the flag load (before any lock). No behavior change: misses emit
/// nothing, eviction only drops joins.
/// Worker command counters are per-app, so frame/focus keys are the full app
/// lifetime (`pid`, `generation`) plus the command id - a bare `cmd_id`
/// would join across apps. Drain `op-<seq>` values are dispatch-issued and
/// unique per process, so the bare sequence is the whole key.
const MAP_CAP: usize = 128;

/// Full worker-command identity: owning app lifetime plus command id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TraceKey {
    pub(crate) pid: u32,
    pub(crate) generation: u64,
    pub(crate) id: u64,
}

impl TraceKey {
    fn is_zero(self) -> bool {
        self.pid == 0 && self.generation == 0 && self.id == 0
    }
}

/// Static completion-class token for one honest AX completion (class only:
/// `Failed` reasons never reach the sink; the numeric detail rides the
/// existing `ax-result` lines). Lets worker apply records join by class.
#[must_use]
pub fn completion_token(completion: &crate::runtime_types::AxCompletion) -> &'static str {
    match completion {
        crate::runtime_types::AxCompletion::Applied { .. } => "applied",
        crate::runtime_types::AxCompletion::PartialApplied { .. } => "partial",
        crate::runtime_types::AxCompletion::Failed { .. } => "failed",
        crate::runtime_types::AxCompletion::Timeout { .. } => "timeout",
    }
}

/// Pure bounded-insert core shared by the three maps (testable): appends
/// `(key, trace, t0_us, bind_us)`, refreshing an existing key (moved to the
/// back as most-recently-used), evicting the oldest past `cap`. Zero traces
/// never bind (0 is untraced). Returns the resulting map length.
pub(crate) fn map_insert_core<K: PartialEq>(
    map: &mut Vec<(K, u64, u64, u64)>,
    cap: usize,
    key: K,
    trace: u64,
    t0_us: u64,
    bind_us: u64,
) -> usize {
    if trace == 0 {
        return map.len();
    }
    if let Some(pos) = map.iter().position(|(k, _, _, _)| *k == key) {
        map.remove(pos);
        map.push((key, trace, t0_us, bind_us));
        return map.len();
    }
    map.push((key, trace, t0_us, bind_us));
    while map.len() > cap {
        map.remove(0);
    }
    map.len()
}

/// Pure lookup core (testable): newest matching key wins. Returns
/// `(trace, t0_us, bind_us)`.
pub(crate) fn map_lookup_core<K: PartialEq>(
    map: &[(K, u64, u64, u64)],
    key: &K,
) -> Option<(u64, u64, u64)> {
    map.iter()
        .rev()
        .find(|(k, _, _, _)| *k == *key)
        .map(|(_, t, o, b)| (*t, *o, *b))
}

/// Bind one worker frame command (full app lifetime plus `cmd_id`) to its
/// dispatch trace and origin stamp. No-op when disabled or untraced. The
/// bind stamp is read here (one clock read per dispatched command, never
/// per window inside the backend). Zero `cmd_id`s never bind.
pub(crate) fn note_cmd(pid: u32, generation: u64, cmd_id: u64, trace_id: u64, t0_us: u64) {
    if !enabled() || cmd_id == 0 || trace_id == 0 {
        return;
    }
    let stamp = now_us();
    let key = TraceKey {
        pid,
        generation,
        id: cmd_id,
    };
    let mut map = CMD_MAP.lock().unwrap_or_else(|poison| poison.into_inner());
    let _ = map_insert_core(&mut map, MAP_CAP, key, trace_id, t0_us, stamp);
}

/// Bind one worker focus command (full app lifetime plus `request_id`) to
/// its dispatch trace and origin stamp.
pub(crate) fn note_focus(pid: u32, generation: u64, request_id: u64, trace_id: u64, t0_us: u64) {
    if !enabled() || request_id == 0 || trace_id == 0 {
        return;
    }
    let stamp = now_us();
    let key = TraceKey {
        pid,
        generation,
        id: request_id,
    };
    let mut map = FOCUS_MAP
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    let _ = map_insert_core(&mut map, MAP_CAP, key, trace_id, t0_us, stamp);
}

/// Bind one numeric `op-<seq>` to its dispatch trace and origin stamp.
pub(crate) fn note_op(op_seq: u64, trace_id: u64, t0_us: u64) {
    if !enabled() || op_seq == 0 || trace_id == 0 {
        return;
    }
    let stamp = now_us();
    let mut map = OP_MAP.lock().unwrap_or_else(|poison| poison.into_inner());
    let _ = map_insert_core(&mut map, MAP_CAP, op_seq, trace_id, t0_us, stamp);
}

/// Look up the trace bound to one worker frame command (`None` when
/// disabled, unmapped, or evicted). Returns `(trace, t0_us, bind_us)`.
#[must_use]
pub(crate) fn trace_for_cmd(pid: u32, generation: u64, cmd_id: u64) -> Option<(u64, u64, u64)> {
    if !enabled() {
        return None;
    }
    let key = TraceKey {
        pid,
        generation,
        id: cmd_id,
    };
    if key.is_zero() {
        return None;
    }
    let map = CMD_MAP.lock().unwrap_or_else(|poison| poison.into_inner());
    map_lookup_core(&map, &key)
}

/// Look up the trace bound to one focus command.
#[must_use]
pub(crate) fn trace_for_focus(
    pid: u32,
    generation: u64,
    request_id: u64,
) -> Option<(u64, u64, u64)> {
    if !enabled() {
        return None;
    }
    let key = TraceKey {
        pid,
        generation,
        id: request_id,
    };
    if key.is_zero() {
        return None;
    }
    let map = FOCUS_MAP
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    map_lookup_core(&map, &key)
}

/// Look up the trace bound to one numeric `op-<seq>`.
#[must_use]
pub(crate) fn trace_for_op(op_seq: u64) -> Option<(u64, u64, u64)> {
    if !enabled() || op_seq == 0 {
        return None;
    }
    let map = OP_MAP.lock().unwrap_or_else(|poison| poison.into_inner());
    map_lookup_core(&map, &op_seq)
}

/// One phase stopwatch. Disabled construction stores no timestamp;
/// [`PhaseClock::elapsed`] reads `0` without a clock call.
#[derive(Debug, Clone, Copy)]
pub struct PhaseClock {
    start_us: u64,
}

impl PhaseClock {
    #[must_use]
    pub fn start() -> Self {
        Self {
            start_us: if enabled() { now_us() } else { 0 },
        }
    }

    #[must_use]
    pub fn elapsed(&self) -> u64 {
        if !enabled() {
            return 0;
        }
        now_us().saturating_sub(self.start_us)
    }
}

/// Emit one per-stage record through the existing ring plus bounded async
/// sink (`ref_id` joins stages of one trace; `count` is microseconds
/// unless the stage documents otherwise; `detail` is the action token for
/// stage records, the static verdict for `timing-dispatch`/`timing-done`
/// records, and the honest completion class for settle records
/// (`timing-frame-settle` / `timing-focus-settle` carry applied/partial/
/// failed/timeout; `timing-op-settle` carries view-staged/member-staged).
/// Disabled: one flag load, no formatting, no emission.
pub(crate) fn record(
    shared: &Arc<Mutex<Shared>>,
    span_id: u64,
    stage: &'static str,
    detail: &'static str,
    us: u64,
) {
    if !enabled() {
        return;
    }
    emit(shared, 0, 0, span_id, stage, detail, us);
}

/// Emit one worker-side record keyed by the command identity the worker
/// already carries (`ref_id` is `cmd_id`/`request_id`, `pid`/`generation`
/// are the owning app lifetime).
pub(crate) fn record_for(
    shared: &Arc<Mutex<Shared>>,
    pid: u32,
    generation: u64,
    ref_id: u64,
    stage: &'static str,
    outcome: &'static str,
    us: u64,
) {
    if !enabled() {
        return;
    }
    emit(shared, pid, generation, ref_id, stage, outcome, us);
}

/// Emit the whole-action record (`timing-done`, `count` is microseconds
/// from the span origin to now) with a static outcome. Disabled or
/// `None` span: one flag load, no emission.
pub(crate) fn finish(shared: &Arc<Mutex<Shared>>, span: Option<ActionSpan>, outcome: &'static str) {
    if !enabled() {
        return;
    }
    if let Some(span) = span {
        record(
            shared,
            span.id,
            stage::DONE,
            outcome,
            now_us().saturating_sub(span.t0_us),
        );
    }
}

/// Stage one codec-leg duration for a later [`flush_legs`]. `codec_transact`
/// owns the leg boundaries but has no sink access, so legs buffer here
/// (bounded: the five genuine legs; overflow sheds, never grows) and the
/// dispatch site flushes them in order under its span. Disabled (or
/// untraced): one flag load, no clock read by the caller when it gates on
/// [`with_current`].
pub fn stage_leg(stage: &'static str, us: u64) {
    if !enabled() {
        return;
    }
    LEG_BUFFER.with(|buf| {
        let mut buffered = buf.borrow_mut();
        if buffered.len() < 8 {
            buffered.push((stage, us));
        }
    });
}

/// Flush staged codec legs as records under `span_id` (in stage order).
/// Disabled: one flag load, no emission. Always drains, even when empty.
pub(crate) fn flush_legs(shared: &Arc<Mutex<Shared>>, span_id: u64, action: &'static str) {
    if !enabled() {
        LEG_BUFFER.with(|buf| buf.borrow_mut().clear());
        return;
    }
    LEG_BUFFER.with(|buf| {
        for (stage, us) in buf.borrow_mut().drain(..) {
            record(shared, span_id, stage, action, us);
        }
    });
}

/// One leg stopwatch for the codec transact: inert without an installed
/// span (no clock reads beyond the flag loads). Each [`LegClock::leg`]
/// stages the elapsed microseconds since the previous leg (or
/// construction) under `stage` for a later [`flush_legs`].
pub struct LegClock {
    active: bool,
    clock: PhaseClock,
}

impl LegClock {
    #[must_use]
    pub fn new() -> Self {
        let active = with_current(|_| ()).is_some();
        Self {
            active,
            clock: PhaseClock::start(),
        }
    }

    pub fn leg(&mut self, stage: &'static str) {
        if !self.active {
            return;
        }
        stage_leg(stage, self.clock.elapsed());
        self.clock = PhaseClock::start();
    }
}

impl Default for LegClock {
    fn default() -> Self {
        Self::new()
    }
}
/// Format one sink line for `event`: the single source of truth for the
/// machine-parseable `tiler-log` schema (one filter, `key=value` tokens,
/// stable prefix). `seq`/`t_ms` are the ring sequence and runtime-relative
/// timestamp the sink path stamps. Output is byte-identical to the historic
/// logger format.
pub(crate) fn format_record(event: &crate::runtime_log::LogEvent) -> String {
    format!(
        "tiler-log seq={} t={}ms pid={} gen={} ref={} kind={} outcome={} count={}",
        event.seq,
        event.timestamp_ms,
        event.pid,
        event.generation,
        event.ref_id,
        event.kind,
        event.outcome,
        event.count,
    )
}

/// Honest fixed-wait classes: the only `add_wait` keys. Fixed small array,
/// never a dynamic map; the summary stays `Copy` across the lane handoff.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WaitKind {
    GrabSettle,
    AnimationHold,
    ArrivalPoll,
    SelectLatch,
}

impl WaitKind {
    pub(crate) const ALL: [WaitKind; 4] = [
        WaitKind::GrabSettle,
        WaitKind::AnimationHold,
        WaitKind::ArrivalPoll,
        WaitKind::SelectLatch,
    ];

    #[must_use]
    pub(crate) const fn index(self) -> usize {
        match self {
            WaitKind::GrabSettle => 0,
            WaitKind::AnimationHold => 1,
            WaitKind::ArrivalPoll => 2,
            WaitKind::SelectLatch => 3,
        }
    }

    #[must_use]
    pub(crate) const fn token(self) -> &'static str {
        match self {
            WaitKind::GrabSettle => "grab-settle",
            WaitKind::AnimationHold => "animation-hold",
            WaitKind::ArrivalPoll => "arrival-poll",
            WaitKind::SelectLatch => "select-latch",
        }
    }
}

/// Thread-local native-call accumulation window: per-thread `(total us,
/// calls)` split by honest call class - AX inventory/readback (`ax-read`),
/// AX setters (`ax-set`), AX focus dispatch (`ax-focus`), window-server (CG)
/// list reads, and four bounded fixed-wait classes (see [`WaitKind`]).
/// Callers add 1 per instrumented native call (never per window, never
/// invented); the owning thread flushes once per batch under its command
/// identity through the bounded async sink, then the window resets.
/// `reset_native_agg`
/// drops a stale window so idle periodic work is never billed to the next
/// action. Disabled: every helper returns after the flag load (no clock
/// reads inside loops beyond the caller's own gated clock).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct NativeAgg {
    ax_read_us: u64,
    ax_read_n: u64,
    ax_set_us: u64,
    ax_set_n: u64,
    ax_focus_us: u64,
    ax_focus_n: u64,
    cg_us: u64,
    cg_n: u64,
    wait_us: [u64; 4],
    wait_n: [u64; 4],
}

/// Transport copy of one owner thread's accumulation window: the same AX/CG
/// counters plus the four fixed-wait classes as bounded arrays, movable
/// single-flight lane (the owner thread snapshots, the main thread flushes
/// under the dispatch trace). `Copy` so the handoff stores by value.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NativeTimingSummary {
    pub(crate) ax_read_us: u64,
    pub(crate) ax_read_n: u64,
    pub(crate) ax_set_us: u64,
    pub(crate) ax_set_n: u64,
    pub(crate) ax_focus_us: u64,
    pub(crate) ax_focus_n: u64,
    pub(crate) cg_us: u64,
    pub(crate) cg_n: u64,
    pub(crate) wait_us: [u64; 4],
    pub(crate) wait_n: [u64; 4],
}

/// One select/send lane execution report: the owner's accumulation plus the
/// honest owner-thread execution time (`exec_us`) under its static lane
/// class (`kind`: `select-exec`, `send-bridged`, `send-drag`, `send-stay`,
/// or `send-trailing`, emitted as the `timing-lane-exec` outcome).
#[derive(Debug, Clone, Copy)]
pub(crate) struct LaneNativeReport {
    pub(crate) summary: NativeTimingSummary,
    pub(crate) exec_us: u64,
    pub(crate) kind: &'static str,
}

thread_local! {
    static NATIVE_AGG: RefCell<NativeAgg> = const { RefCell::new(NativeAgg {
        ax_read_us: 0,
        ax_read_n: 0,
        ax_set_us: 0,
        ax_set_n: 0,
        ax_focus_us: 0,
        ax_focus_n: 0,
        cg_us: 0,
        cg_n: 0,
        wait_us: [0; 4],
        wait_n: [0; 4],
    }) };
    static SEND_KIND: RefCell<Option<&'static str>> = const { RefCell::new(None) };
}

/// Remember the send-lane exec class for the owner thread's pending report
/// (set at the route decision, taken once per job by the owner loop).
pub(crate) fn set_send_kind(kind: &'static str) {
    if !enabled() {
        return;
    }
    SEND_KIND.with(|slot| *slot.borrow_mut() = Some(kind));
}

/// Take the pending send-lane exec class (`None` when unset or disabled).
/// Disabled: one flag load, no TLS access.
pub(crate) fn take_send_kind() -> Option<&'static str> {
    if !enabled() {
        return None;
    }
    SEND_KIND.with(|slot| slot.borrow_mut().take())
}

/// Accumulate one AX inventory/readback batch (`us` microseconds, `n`
/// instrumented calls, 1 per call).
pub(crate) fn add_ax_read(us: u64, n: u64) {
    if !enabled() {
        return;
    }
    NATIVE_AGG.with(|agg| {
        let mut held = agg.borrow_mut();
        held.ax_read_us = held.ax_read_us.saturating_add(us);
        held.ax_read_n = held.ax_read_n.saturating_add(n);
    });
}

/// Accumulate one AX setter leg (`us` microseconds, `n` setter calls).
pub(crate) fn add_ax_set(us: u64, n: u64) {
    if !enabled() {
        return;
    }
    NATIVE_AGG.with(|agg| {
        let mut held = agg.borrow_mut();
        held.ax_set_us = held.ax_set_us.saturating_add(us);
        held.ax_set_n = held.ax_set_n.saturating_add(n);
    });
}

/// Accumulate one AX focus dispatch (`us` microseconds, `n` dispatches).
pub(crate) fn add_ax_focus(us: u64, n: u64) {
    if !enabled() {
        return;
    }
    NATIVE_AGG.with(|agg| {
        let mut held = agg.borrow_mut();
        held.ax_focus_us = held.ax_focus_us.saturating_add(us);
        held.ax_focus_n = held.ax_focus_n.saturating_add(n);
    });
}

/// Accumulate one window-server (CG) list read.
pub(crate) fn add_cg(us: u64, n: u64) {
    if !enabled() {
        return;
    }
    NATIVE_AGG.with(|agg| {
        let mut held = agg.borrow_mut();
        held.cg_us = held.cg_us.saturating_add(us);
        held.cg_n = held.cg_n.saturating_add(n);
    });
}

/// Accumulate one bounded fixed wait/sleep/poll window under its honest
/// class (never a generic `wait` bucket).
pub(crate) fn add_wait(kind: WaitKind, us: u64, n: u64) {
    if !enabled() {
        return;
    }
    NATIVE_AGG.with(|agg| {
        let mut held = agg.borrow_mut();
        let i = kind.index();
        held.wait_us[i] = held.wait_us[i].saturating_add(us);
        held.wait_n[i] = held.wait_n[i].saturating_add(n);
    });
}

/// Drop this thread's accumulation window without emitting, so idle
/// periodic work is never billed to the next action. Disabled: one flag
/// load, no TLS access (nothing ever accumulates while disabled, so there
/// is nothing to drop).
pub(crate) fn reset_native_agg() {
    if !enabled() {
        return;
    }
    NATIVE_AGG.with(|agg| *agg.borrow_mut() = NativeAgg::default());
}

/// Snapshot this thread's accumulation window and reset it. Disabled: one
/// flag load, no TLS access, and the zero summary.
pub(crate) fn take_native_agg() -> NativeTimingSummary {
    if !enabled() {
        return NativeTimingSummary::default();
    }
    let window = NATIVE_AGG.with(|agg| *agg.borrow_mut());
    NATIVE_AGG.with(|agg| *agg.borrow_mut() = NativeAgg::default());
    NativeTimingSummary {
        ax_read_us: window.ax_read_us,
        ax_read_n: window.ax_read_n,
        ax_set_us: window.ax_set_us,
        ax_set_n: window.ax_set_n,
        ax_focus_us: window.ax_focus_us,
        ax_focus_n: window.ax_focus_n,
        cg_us: window.cg_us,
        cg_n: window.cg_n,
        wait_us: window.wait_us,
        wait_n: window.wait_n,
    }
}

/// Emit one accumulation summary under a trace identity (`pid` 0, `gen` 0,
/// `ref` is the trace; the lane has no worker command identity). Silent
/// when disabled or untraced.
pub(crate) fn emit_native_agg_for_trace(
    shared: &Arc<Mutex<Shared>>,
    trace_id: u64,
    summary: NativeTimingSummary,
) {
    if !enabled() || trace_id == 0 {
        return;
    }
    emit_summary(shared, 0, 0, trace_id, summary);
}

/// Flush this thread's accumulation window as worker-side records under the
/// batch command identity (`pid`/`generation`/`ref_id`, joining through the
/// frame/focus bind like `timing-frame-apply` does): up to two lines per
/// nonzero class (total microseconds, then call count). Always resets,
/// even when empty or disabled.
pub(crate) fn flush_native_agg(
    shared: &Arc<Mutex<Shared>>,
    pid: u32,
    generation: u64,
    ref_id: u64,
) {
    let summary = take_native_agg();
    if ref_id == 0 {
        return;
    }
    emit_summary(shared, pid, generation, ref_id, summary);
}
/// Shared emitter for one accumulation summary under `pid`/`generation`/
/// `ref_id`: up to two lines per nonzero class (total microseconds, then
/// call count). Disabled emits nothing.
fn emit_summary(
    shared: &Arc<Mutex<Shared>>,
    pid: u32,
    generation: u64,
    ref_id: u64,
    summary: NativeTimingSummary,
) {
    if !enabled() {
        return;
    }
    for (us, n, outcome) in [
        (summary.ax_read_us, summary.ax_read_n, "ax-read"),
        (summary.ax_set_us, summary.ax_set_n, "ax-set"),
        (summary.ax_focus_us, summary.ax_focus_n, "ax-focus"),
    ] {
        if n > 0 {
            record_for(
                shared,
                pid,
                generation,
                ref_id,
                stage::NATIVE_AX,
                outcome,
                us,
            );
            record_for(
                shared,
                pid,
                generation,
                ref_id,
                stage::NATIVE_AX_N,
                outcome,
                n,
            );
        }
    }
    if summary.cg_n > 0 {
        record_for(
            shared,
            pid,
            generation,
            ref_id,
            stage::NATIVE_CG,
            "cg",
            summary.cg_us,
        );
        record_for(
            shared,
            pid,
            generation,
            ref_id,
            stage::NATIVE_CG_N,
            "cg",
            summary.cg_n,
        );
    }
    for kind in WaitKind::ALL {
        let i = kind.index();
        if summary.wait_n[i] > 0 {
            record_for(
                shared,
                pid,
                generation,
                ref_id,
                stage::NATIVE_WAIT,
                kind.token(),
                summary.wait_us[i],
            );
            record_for(
                shared,
                pid,
                generation,
                ref_id,
                stage::NATIVE_WAIT_N,
                kind.token(),
                summary.wait_n[i],
            );
        }
    }
}

/// Latest applied-reply publication identity for coalesced submit attribution.
/// Set at the single `timing-publish` site with the dispatch trace id; read
/// by the border/indicator submit emitters. Coalesced by design: when
/// several dispatches publish before the next main-thread submit, only the
/// latest publication identity survives (an obsolete publication may be
/// skipped entirely and never submitted). Never claim exact causal
/// per-action render or paint: this is latest-publication attribution only
/// (submit, never painted proof). Zero means no publication yet (or
/// disabled); process-wide like the trace counter.
static LAST_PUBLISH: AtomicU64 = AtomicU64::new(0);

/// Remember the latest publication trace id (0 clears). Disabled: no store.
pub(crate) fn note_publish(trace_id: u64) {
    if !enabled() {
        return;
    }
    LAST_PUBLISH.store(trace_id, Ordering::Relaxed);
}

/// Latest publication trace id (0 when none yet or disabled). One atomic
/// load; callers emit nothing when 0.
#[must_use]
pub(crate) fn last_publish() -> u64 {
    if !enabled() {
        return 0;
    }
    LAST_PUBLISH.load(Ordering::Relaxed)
}

/// Test-only parsed timing line: the pure inverse of [`format_record`].
/// Struct (not tuple) return keeps the test-only parser small.
#[cfg(test)]
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ParsedRecord {
    pub(crate) seq: u64,
    pub(crate) t_ms: u64,
    pub(crate) pid: u32,
    pub(crate) generation: u64,
    pub(crate) ref_id: u64,
    pub(crate) stage: String,
    pub(crate) outcome: String,
    pub(crate) count: u64,
}

/// Test-only parse of one formatted timing record (`None` on any shape
/// drift). Never compiled into the production binary.
#[cfg(test)]
pub(crate) fn parse_record(line: &str) -> Option<ParsedRecord> {
    let rest = line.strip_prefix("tiler-log ")?;
    let mut seq = None;
    let mut t_ms = None;
    let mut pid = None;
    let mut generation = None;
    let mut ref_id = None;
    let mut stage = None;
    let mut outcome = None;
    let mut count = None;
    for token in rest.split(' ') {
        let (key, value) = token.split_once('=')?;
        match key {
            "seq" => seq = value.parse().ok(),
            "t" => t_ms = value.strip_suffix("ms")?.parse().ok(),
            "pid" => pid = value.parse().ok(),
            "gen" => generation = value.parse().ok(),
            "ref" => ref_id = value.parse().ok(),
            "kind" => stage = Some(value.to_owned()),
            "outcome" => outcome = Some(value.to_owned()),
            "count" => count = value.parse().ok(),
            _ => return None,
        }
    }
    Some(ParsedRecord {
        seq: seq?,
        t_ms: t_ms?,
        pid: pid?,
        generation: generation?,
        ref_id: ref_id?,
        stage: stage?,
        outcome: outcome?,
        count: count?,
    })
}

/// Parse a world-command operation identity (`op-<seq>`) to its numeric
/// sequence for drain-side joins. `None` on any malformed, zero, or
/// unparsable identity (never guessed).
#[must_use]
pub fn parse_op_seq(operation_id: &str) -> Option<u64> {
    operation_id
        .strip_prefix("op-")?
        .parse::<u64>()
        .ok()
        .filter(|seq| *seq != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_actions() -> Vec<InputSemanticAction> {
        use crate::runtime_types::InputDirection;
        use tiler_core::contract::ResizeMode;
        vec![
            InputSemanticAction::FocusStep {
                direction: InputDirection::Right,
            },
            InputSemanticAction::MoveStep {
                direction: InputDirection::Left,
            },
            InputSemanticAction::ToggleFloat,
            InputSemanticAction::ResizeStep {
                direction: InputDirection::Right,
                mode: ResizeMode::Outwards,
            },
            InputSemanticAction::ToggleOrientation,
            InputSemanticAction::WorkspaceSelect { index: 3 },
            InputSemanticAction::IndicatorSelectWorkspace {
                output: "macos-display-1".to_owned(),
                workspace: "secret-ws".to_owned(),
            },
            InputSemanticAction::WorkspaceSend {
                index: 9,
                follow: true,
            },
            InputSemanticAction::WorkspaceRelative { prev: true },
            InputSemanticAction::WorkspaceSendRelative {
                prev: false,
                follow: false,
            },
            InputSemanticAction::WorkspacePrevious,
            InputSemanticAction::WorkspaceOutput {
                direction: InputDirection::Right,
                follow: true,
            },
        ]
    }

    #[test]
    fn switch_values_accept_only_explicit_opt_ins() {
        for on in ["1", "true", "TRUE", " yes ", "on", "On"] {
            assert!(switch_value(on), "{on:?}");
        }
        for off in ["", "0", "false", "no", "off", "banana", "11", "true1"] {
            assert!(!switch_value(off), "{off:?}");
        }
    }

    #[test]
    fn action_tokens_are_static_class_names_without_content() {
        let tokens: Vec<&str> = all_actions().iter().map(action_token).collect();
        assert_eq!(tokens.len(), 12);
        for token in &tokens {
            assert!(
                token.bytes().all(|b| b.is_ascii_lowercase() || b == b'-'),
                "{token:?}"
            );
            assert!(
                !token.bytes().any(|b| b.is_ascii_digit()),
                "no index leaks: {token:?}"
            );
        }
        // Class-only: distinct indexes/directions share one token.
        assert_eq!(
            action_token(&InputSemanticAction::WorkspaceSelect { index: 1 }),
            action_token(&InputSemanticAction::WorkspaceSelect { index: 9 })
        );
        assert_eq!(
            action_token(&InputSemanticAction::WorkspaceSend {
                index: 0,
                follow: false
            }),
            "send"
        );
    }

    #[test]
    fn stage_schema_carries_a_single_stable_prefix() {
        for stage_kind in [
            stage::INTAKE,
            stage::INPUT_MAP,
            stage::QUEUE,
            stage::MAPPING,
            stage::REPORT,
            stage::CODEC_REPORT_ENC,
            stage::CODEC_REPORT_DEC,
            stage::CORE,
            stage::CODEC_REPLY_ENC,
            stage::CODEC_REPLY_DEC,
            stage::ACCEPT,
            stage::PUBLISH,
            stage::DISPATCH,
            stage::DONE,
            stage::FRAME_BIND,
            stage::FRAME_APPLY,
            stage::FOCUS_BIND,
            stage::FOCUS_APPLY,
            stage::OBSERVE,
            stage::OP_BIND,
            stage::FRAME_SETTLE,
            stage::FOCUS_SETTLE,
            stage::OP_SETTLE,
            stage::NATIVE_AX,
            stage::NATIVE_AX_N,
            stage::NATIVE_CG,
            stage::NATIVE_CG_N,
            stage::NATIVE_WAIT,
            stage::NATIVE_WAIT_N,
            stage::LANE_EXEC,
            stage::READBACK_TOTAL,
        ] {
            assert!(stage_kind.starts_with("timing-"), "{stage_kind:?}");
        }
    }

    #[test]
    fn mint_core_is_disabled_without_flag_and_never_mints_zero() {
        assert_eq!(mint_inner(false, "focus-step", 7, 100), None);
        assert_eq!(mint_inner(true, "focus-step", 0, 100), None);
        assert_eq!(
            mint_inner(true, "focus-step", 7, 100),
            Some(ActionSpan {
                id: 7,
                action: "focus-step",
                t0_us: 100
            })
        );
    }

    fn log_event(
        parts: (u64, u64, u32, u64, u64, &'static str, &'static str, u64),
    ) -> crate::runtime_log::LogEvent {
        let (seq, t_ms, pid, generation, ref_id, kind, outcome, count) = parts;
        crate::runtime_log::LogEvent {
            seq,
            timestamp_ms: t_ms,
            pid,
            generation,
            ref_id,
            kind,
            outcome,
            count,
        }
    }

    #[test]
    fn record_format_is_stable_and_machine_parseable() {
        let line = format_record(&log_event((
            41,
            12,
            0,
            0,
            7,
            stage::QUEUE,
            "focus-step",
            233,
        )));
        assert_eq!(
            line,
            "tiler-log seq=41 t=12ms pid=0 gen=0 ref=7 kind=timing-queue outcome=focus-step count=233"
        );
        let parsed = parse_record(&line).expect("roundtrip");
        assert_eq!(
            parsed,
            ParsedRecord {
                seq: 41,
                t_ms: 12,
                pid: 0,
                generation: 0,
                ref_id: 7,
                stage: "timing-queue".to_owned(),
                outcome: "focus-step".to_owned(),
                count: 233,
            }
        );
        assert!(parse_record("tiler-log seq=1 t=2ms pid=0 gen=0 ref=1 kind=x").is_none());
        assert!(parse_record("unrelated line").is_none());
    }

    #[test]
    fn cross_thread_records_join_on_shared_ids() {
        // Engine bind (pid + generation + cmd_id -> trace) joined with a
        // worker apply record produced on another thread: same keys.
        let bind = format_record(&log_event((1, 1, 501, 3, 9, stage::FRAME_BIND, "frame", 7)));
        let handle = std::thread::spawn(|| {
            format_record(&log_event((
                2,
                2,
                501,
                3,
                9,
                stage::FRAME_APPLY,
                "applied",
                812,
            )))
        });
        let apply = handle.join().expect("join");
        let bind_parsed = parse_record(&bind).expect("bind parses");
        let apply_parsed = parse_record(&apply).expect("apply parses");
        assert_eq!(
            (bind_parsed.pid, bind_parsed.generation, bind_parsed.ref_id),
            (
                apply_parsed.pid,
                apply_parsed.generation,
                apply_parsed.ref_id
            )
        );
        assert_eq!(
            (bind_parsed.pid, bind_parsed.ref_id, bind_parsed.count),
            (501, 9, 7)
        );
        // Op bind (trace -> op seq) joined with a drain record on the same
        // numeric op sequence from a third thread.
        let op_bind = format_record(&log_event((3, 3, 0, 0, 7, stage::OP_BIND, "select", 12)));
        let drain = std::thread::spawn(|| {
            format_record(&log_event((
                4,
                4,
                0,
                0,
                12,
                "timing-drain-proof",
                "select",
                3400,
            )))
        })
        .join()
        .expect("join");
        let op_parsed = parse_record(&op_bind).expect("op parses");
        let drain_parsed = parse_record(&drain).expect("drain parses");
        assert_eq!(op_parsed.ref_id, 7);
        assert_eq!(drain_parsed.ref_id, op_parsed.count);
    }

    #[test]
    fn trace_ids_are_unique_from_the_shared_counter() {
        // `mint`, `mint_at`, and `ensure_span` all draw from one counter:
        // consecutive draws never repeat and never yield 0 (untraced).
        let first = next_trace_id();
        let second = next_trace_id();
        let third = next_trace_id();
        assert_ne!(first, 0);
        assert_ne!(second, 0);
        assert_ne!(third, 0);
        assert_ne!(first, second);
        assert_ne!(second, third);
        assert_ne!(first, third);
    }

    #[test]
    fn disabled_paths_emit_nothing_and_read_no_clock() {
        // Strict only when the suite runs untraced (the default): every
        // helper returns after the flag load with no emission and no stamp.
        if enabled() {
            return;
        }
        assert_eq!(intake_stamp(), 0);
        assert_eq!(enqueue_stamp(), 0);
        assert_eq!(mint_observer_trace(), None);
        note_publish(7);
        assert_eq!(last_publish(), 0);
        let action = InputSemanticAction::ToggleFloat;
        assert_eq!(mint(&action), None);
        assert_eq!(mint_at(&action, 0), None);
        assert_eq!(mint_at(&action, 999), None);
        assert!(ensure_span("op").is_none());
        assert_eq!(PhaseClock::start().elapsed(), 0);
        assert_eq!(trace_for_cmd(1, 1, 1), None);
        assert_eq!(trace_for_focus(1, 1, 1), None);
        assert_eq!(trace_for_op(1), None);
    }

    #[test]
    fn bounded_maps_join_feedback_without_guessing() {
        let app_a = TraceKey {
            pid: 501,
            generation: 3,
            id: 9,
        };
        let app_b = TraceKey {
            pid: 502,
            generation: 3,
            id: 9,
        };
        let mut map = Vec::new();
        assert_eq!(map_insert_core(&mut map, 2, app_a, 7, 50, 100), 1);
        // SAME cmd_id under a different app lifetime binds separately: no
        // cross-app join.
        assert_eq!(map_insert_core(&mut map, 2, app_b, 8, 60, 110), 2);
        assert_eq!(map_lookup_core(&map, &app_a), Some((7, 50, 100)));
        assert_eq!(map_lookup_core(&map, &app_b), Some((8, 60, 110)));
        // Refresh in place carries the new origin stamp, no growth.
        assert_eq!(map_insert_core(&mut map, 2, app_a, 9, 70, 120), 2);
        assert_eq!(map_lookup_core(&map, &app_a), Some((9, 70, 120)));
        // Eviction drops the oldest join only.
        let app_c = TraceKey {
            pid: 503,
            generation: 3,
            id: 9,
        };
        assert_eq!(map_insert_core(&mut map, 2, app_c, 9, 80, 130), 2);
        assert_eq!(map_lookup_core(&map, &app_b), None);
        assert_eq!(map_lookup_core(&map, &app_a), Some((9, 70, 120)));
        assert_eq!(map_lookup_core(&map, &app_c), Some((9, 80, 130)));
        // Zero traces never bind; unknown keys miss.
        let mut op_map: Vec<(u64, u64, u64, u64)> = Vec::new();
        assert_eq!(map_insert_core(&mut op_map, 2, 12u64, 0, 50, 140), 0);
        assert_eq!(map_lookup_core(&op_map, &12u64), None);
        // Disabled mint core still refuses zero ids.
        assert_eq!(mint_inner(true, "focus-step", 0, 100), None);
    }

    #[test]
    fn op_identity_parses_numerically_and_refuses_guesses() {
        assert_eq!(parse_op_seq("op-3"), Some(3));
        assert_eq!(parse_op_seq("op-12"), Some(12));
        assert_eq!(parse_op_seq("op-0"), None);
        assert_eq!(parse_op_seq("op-"), None);
        assert_eq!(parse_op_seq("op-follow"), None);
        assert_eq!(parse_op_seq("op-native-2"), None);
        assert_eq!(parse_op_seq("banana"), None);
        assert_eq!(parse_op_seq(""), None);
    }
}

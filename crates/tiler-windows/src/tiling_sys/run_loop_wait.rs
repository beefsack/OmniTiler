#![forbid(unsafe_code)]
use super::*;

// Per-turn WinEvent edge drain for the tile loop: takes the queued hook
// callbacks once, filters own-overlay events, preserves foreground HWNDs
// in delivery order for the world diff's in-flight focus tracking, and
// captures gesture START state. Nested `continue`s below stay on the
// drain loop.
pub(super) struct EdgeDrain {
    pub woke: bool,
    pub batch_total: u64,
    pub wakes: u64,
    pub starts: u64,
    pub ends: u64,
    /// Drained `EVENT_SYSTEM_FOREGROUND` HWNDs in delivery order, after the
    /// own-overlay exclusion below. The world diff resolves each against
    /// full native identity plus lifetime at apply entry; nothing here
    /// resolves identity (no pid work in the drain, never in the callback).
    pub foreground: Vec<u64>,
    /// Per-kind effective-wake counts for this pass (own-overlay filtered,
    /// post-filter only): the actual pass relation for wake attribution,
    /// distinct from the aggregate once-per-throttle counters.
    pub kind_foreground: u64,
    pub kind_visibility: u64,
    pub kind_other: u64,
    pub kind_unknown: u64,
    /// Per-event wake counts for this pass (same filtered set as above,
    /// split by exact WinEvent so mouse/location traffic never hides in
    /// `other`): foreground, show, hide, create, destroy, location, and
    /// anything outside the wake set. Bounded fixed counters, never a dump.
    pub kind_location: u64,
    pub kind_create: u64,
    pub kind_destroy: u64,
    pub kind_show: u64,
    pub kind_hide: u64,
    /// Taskbar-subtree hits that woke the loop this pass (pid-scoped
    /// Explorer traffic, validated against the taskbar subtree).
    pub taskbar: u64,
    /// Trace-only drained foreground edges with delivery/owner timestamps,
    /// in delivery order, parallel to [`EdgeDrain::foreground`]. Capped at
    /// [`super::observe::MAX_FOREGROUND_TRACE_EDGES`] with the excess in
    /// [`EdgeDrain::foreground_omitted`]. Empty unless tracing.
    pub foreground_detail: Vec<super::observe::ForegroundEdgeMeta>,
    /// Drained foreground edges beyond the detail cap: honest omission,
    /// never a silent loss (the production `foreground` HWNDs above still
    /// resolve in the world diff).
    pub foreground_omitted: usize,
}

/// Gesture-open pointer-sampling cadence: while a managed gesture holds the
/// loop, pointer motion changes the drop preview without moving any target
/// window, and pointer motion produces no WinEvent, message, or input-pump
/// batch for the owner. This armed-only deadline keeps the existing ~100ms
/// preview sampling while held only; healthy idle carries no timer for it.
pub(super) const GESTURE_SAMPLE_MS: u64 = 100;
/// Trace-only pump-waits reporting throttle in seconds. This is never an
/// armed wait deadline: healthy trace idle still blocks infinitely, and a
/// due throttle only emits on a naturally occurring owner turn (never a
/// diagnostic-induced wake or reconcile). The counters accumulate across
/// idle, so a delayed line loses nothing; quiet runs additionally flush one
/// final read-only summary at clean loop exit.
pub(super) const TRACE_REPORT_SECS: u64 = 2;
/// Windowless monitor fallback cadence, armed only when the owner has no
/// top-level window to receive display-change broadcasts (tray-less runs).
/// Tray-carrying runs re-read monitors on message wakes instead, so healthy
/// idle carries no timer for this either.
pub(super) const MONITOR_FALLBACK_SECS: u64 = 2;
/// Degraded directory-watch fallback cadence, armed only when a watch
/// cannot be established or breaks. Bounds stop/request/settings latency
/// while unwatched; setup retries on every fallback wake.
pub(super) const DEGRADED_POLL_MS: u64 = 100;

/// Armed deadlines for one owner wait. Every field is `None`/false at
/// healthy idle (no retries, no restore, unbounded run, no gesture), which
/// selects an infinite wait. Each armed entry carries its own specific
/// reason; there is no unconditional poll. Trace reporting is deliberately
/// not a member: diagnostics must never wake or reconcile the owner.
pub(super) struct OwnerDeadlines {
    pub restore_until: Option<Instant>,
    /// Bridged gesture-finish hide-ack bound (`FINISH_WAIT` past END):
    /// without it an absent ack strands the pending slot behind an idle
    /// wait. The timeout path still refuses with no drop and no actuation,
    /// so the hide-before-drop contract is unchanged; this only guarantees
    /// the service turn that observes it.
    pub pending_until: Option<Instant>,
    /// Visible-pager maintenance bound (creation-retry backoff and topmost
    /// reassert combined, armed only while overlay strips show): without it
    /// a failed creation or a covering topmost window strands with no
    /// recovery until an unrelated wake. Hidden/suppressed pagers arm
    /// nothing, so this is never a global poll.
    pub pager_maintenance_at: Option<Instant>,
    pub snap_retry_at: Option<Instant>,
    pub pager_retry_at: Option<Instant>,
    pub pager_icon_retry_at: Option<Instant>,
    pub gaming_due_at: Option<Instant>,
    pub monitor_due_at: Option<Instant>,
    pub run_end: Option<Instant>,
    pub gesture_open: bool,
    pub degraded_poll: bool,
}

/// Deterministic owner-wait timeout cause for trace-only attribution: which
/// armed deadline selected the requested timeout. `Staged` is never produced
/// by [`owner_deadline_source`]: the caller forces it for same-thread
/// staged-work zero-timeout collects, which skip deadline selection. Order of
/// declaration matches [`OWNER_WAIT_SOURCE_ORDER`]; discriminants index
/// [`OwnerWaitCounts::src_timeouts`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum OwnerWaitSource {
    Staged = 0,
    Restore = 1,
    Pending = 2,
    Pager = 3,
    SnapRetry = 4,
    PagerRetry = 5,
    IconRetry = 6,
    Gaming = 7,
    Monitor = 8,
    RunEnd = 9,
    Gesture = 10,
    DegradedPoll = 11,
    NoneIdle = 12,
}

/// JSON column order for the `owner_src_timeouts` array in `pump-waits`
/// lines, parallel to [`OwnerWaitSource`] discriminants.
pub(super) const OWNER_WAIT_SOURCE_ORDER: [&str; 13] = [
    "staged",
    "restore",
    "pending",
    "pager",
    "snap-retry",
    "pager-retry",
    "icon-retry",
    "gaming",
    "monitor",
    "run-end",
    "gesture",
    "degraded-poll",
    "none",
];

/// Trace-only per-wait diagnostic accompanying one [`WaitWake`]: whether the
/// wait was a staged zero-timeout collect, the selected deadline source, the
/// exact requested timeout passed to the native wait, and the measured
/// elapsed time around the wait call, and whether the winning deadline was
/// already due at selection (past-due zero requests versus future
/// sub-millisecond remainders). Pure bookkeeping: never arms, wakes,
/// or actuates anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct OwnerWaitDiag {
    pub staged: bool,
    pub source: OwnerWaitSource,
    pub requested_ms: u32,
    pub elapsed_ms: u64,
    pub already_due: bool,
}

/// Selected owner wait timeout from one [`OwnerDeadlines`] scan: the winning
/// cause, the whole-millisecond request, and whether the winning deadline
/// was already due at the selection timestamp (`at <= now`). Future
/// durations round up, so a non-staged zero request implies `already_due`;
/// the flag still separates overdue deadlines from conversion effects in
/// the trace attribution.
pub(super) struct OwnerTimeout {
    pub source: OwnerWaitSource,
    pub requested_ms: u32,
    pub already_due: bool,
}

/// Read-only wait-counter snapshots for one `pump-waits` line: the input
/// six-tuple `(turns, waits, work, msg, timeout, failed)` under one
/// diag-slot lock plus the renderer six-tuple `(waits, work, msg,
/// timeout, failed, acks)` from atomics. No drains, no signals: safe on
/// any natural turn and at loop exit, and incapable of waking or actuating
/// anything.
pub(super) fn pump_waits_snapshots(state: &TileLoop) -> (InputWaitSplit, RenderWaitSplit) {
    let input = match state.input_bridge.as_ref() {
        Some(bridge) => {
            let diag = bridge.channels.diag.snapshot();
            (
                diag.turns,
                diag.waits,
                diag.wait_work,
                diag.wait_msg,
                diag.wait_timeout,
                diag.wait_failed,
            )
        }
        None => (0, 0, 0, 0, 0, 0),
    };
    let render = match state.render_bridge.as_ref() {
        Some(bridge) => {
            let (waits, work, msg, timeout, failed) = bridge.channels.wait_counts();
            (waits, work, msg, timeout, failed, bridge.channels.ack_seq())
        }
        None => (0, 0, 0, 0, 0, 0),
    };
    (input, render)
}

/// Selected owner wait timeout plus its deterministic cause: the minimum
/// over the armed deadlines (ties resolve to the earliest-listed deadline,
/// matching the previous `min` fold), or infinite at healthy idle with
/// [`OwnerWaitSource::NoneIdle`]. The degraded-poll fallback caps (never
/// extends) the wait and takes the cause only when it strictly reduces the
/// timeout. Pure (no handles, no syscalls): unit-testable. Future durations
/// round UP to whole milliseconds (a sub-millisecond remainder requests
/// 1 ms, never 0); already-due deadlines request 0 and unarmed stays
/// `u32::MAX`. At most 1 ms of conversion quantization: no cadence,
/// backoff, or contract changes.
pub(super) fn owner_deadline_source(now: Instant, deadlines: &OwnerDeadlines) -> OwnerTimeout {
    let mut earliest: Option<Instant> = None;
    let mut source = OwnerWaitSource::NoneIdle;
    let mut push = |at: Option<Instant>, candidate: OwnerWaitSource| {
        if let Some(at) = at {
            let replace = match earliest {
                None => true,
                // Strict: ties keep the earliest-listed deadline.
                Some(prev) => at < prev,
            };
            if replace {
                earliest = Some(at);
                source = candidate;
            }
        }
    };
    push(deadlines.restore_until, OwnerWaitSource::Restore);
    push(deadlines.pending_until, OwnerWaitSource::Pending);
    push(deadlines.pager_maintenance_at, OwnerWaitSource::Pager);
    push(deadlines.snap_retry_at, OwnerWaitSource::SnapRetry);
    push(deadlines.pager_retry_at, OwnerWaitSource::PagerRetry);
    push(deadlines.pager_icon_retry_at, OwnerWaitSource::IconRetry);
    push(deadlines.gaming_due_at, OwnerWaitSource::Gaming);
    push(deadlines.monitor_due_at, OwnerWaitSource::Monitor);
    push(deadlines.run_end, OwnerWaitSource::RunEnd);
    if deadlines.gesture_open {
        push(
            now.checked_add(Duration::from_millis(GESTURE_SAMPLE_MS)),
            OwnerWaitSource::Gesture,
        );
    }
    let already_due = earliest.is_some_and(|at| at <= now);
    let mut timeout = match earliest {
        None => u32::MAX,
        Some(at) => at
            .saturating_duration_since(now)
            .as_nanos()
            .div_ceil(1_000_000)
            .min(u128::from(u32::MAX)) as u32,
    };
    if deadlines.degraded_poll && (DEGRADED_POLL_MS as u32) < timeout {
        timeout = DEGRADED_POLL_MS as u32;
        source = OwnerWaitSource::DegradedPoll;
    }
    OwnerTimeout {
        source,
        requested_ms: timeout,
        already_due,
    }
}

/// Deterministic clean-loop-exit cause for the single `loop-exit` trace
/// line: exact-owner stop file first, then the tray Stop pick, then the
/// bounded-run deadline. Matches the loop-head evaluation order so the
/// logged cause is exactly the branch the loop would take. Pure
/// (unit-testable): the loop evaluates the three live predicates and logs
/// the returned cause before returning. No log content beyond the cause
/// token (no paths, ids, or window content).
#[must_use]
pub(super) fn loop_exit_cause(
    file_stop: bool,
    tray_stop: bool,
    expired: bool,
) -> Option<&'static str> {
    if file_stop {
        Some("stop-file")
    } else if tray_stop {
        Some("tray-stop")
    } else if expired {
        Some("deadline")
    } else {
        None
    }
}
/// Open-gesture sampling demand: a managed gesture with START capture holds
/// the loop and needs the periodic preview sample. Mirrors the tick's wake
/// condition exactly (same sets, same order).
pub(super) fn gesture_sampling_open(state: &TileLoop) -> bool {
    state
        .active
        .iter()
        .any(|hwnd| state.managed.contains(hwnd) && state.gesture_before.contains_key(hwnd))
}

/// Same-thread staged-work probe for the pre-wait skip: tray stop, toggle,
/// and select picks plus pager clicks are staged on this thread during
/// message dispatch, so a peek right after the pre-drain sees everything
/// the wait would otherwise strand. Brief mutex takes, never waits; all
/// peeks are non-draining (the intake and tick dispatch drain once below).
pub(super) fn tray_staged_pending(
    state: &TileLoop,
    tray: &Option<crate::tray_sys::TrayOwner>,
) -> bool {
    if tray.as_ref().is_some_and(|t| t.stop_requested()) {
        return true;
    }
    if tray.as_ref().is_some_and(|t| t.has_toggle_request()) {
        return true;
    }
    if tray.as_ref().is_some_and(|t| t.has_select_request()) {
        return true;
    }
    state
        .pager_clicks
        .lock()
        .map(|slot| slot.is_some())
        .unwrap_or(false)
}

pub(super) fn drain_hook_edges(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    log_path: &Path,
) -> EdgeDrain {
    // Trace-only pending-edge peek before the consuming take below: exact
    // kinds joined to managed tokens/classes with no drain and no gate
    // effect. Own-overlay HWNDs are filtered at drain, not here, so the
    // peek never decides what wakes.
    if state.trace && super::observe::hook_edges_pending() {
        let (rows, omitted) = super::observe::pending_edge_rows(
            |hwnd, snapshot| {
                // Latest visible observation, not retained membership: a
                // just-revealed token can match while this is still false.
                let managed = state.managed.contains(&hwnd);
                let views: Vec<super::observe::MemberJoinView<'_>> = state
                    .member_tokens
                    .iter()
                    .filter(|(key, _)| key.hwnd == hwnd)
                    .map(|(key, token)| super::observe::MemberJoinView {
                        hwnd: key.hwnd,
                        pid: key.pid,
                        token: token.as_str(),
                        stored_tag: state.member_tags.get(key).map(String::as_str),
                    })
                    .collect();
                let (token, reason) = super::observe::join_delivery_token(hwnd, snapshot, &views);
                (managed, token, reason)
            },
            |hwnd| {
                let class = super::observe::class_of(hwnd as isize as HWND);
                (!class.is_empty()).then_some(class)
            },
        );
        if !rows.is_empty() {
            super::trace::log_json_at(
                log_path,
                serde_json::json!({
                    "event": "pending-edges",
                    "tick": state.tick,
                    "edges": rows,
                    "omitted": omitted,
                }),
            );
        }
    }
    let events = hook_queue()
        .lock()
        .map(|mut queue| std::mem::take(&mut *queue))
        .unwrap_or_default();
    // Main queue drain: hook callbacks queued on this thread. No pid work
    // here; Explorer scoping lives in the separate taskbar queue below.
    let mut woke = false;
    // Trace-only batch breakdown for the owner-work sample; default
    // runs skip the counting.
    let ow_batch_total: u64 = if state.trace { events.len() as u64 } else { 0 };
    let mut ow_wakes: u64 = 0;
    let mut ow_starts: u64 = 0;
    let mut ow_ends: u64 = 0;
    let mut foreground: Vec<u64> = Vec::new();
    let mut kind_foreground: u64 = 0;
    let mut kind_visibility: u64 = 0;
    let mut kind_other: u64 = 0;
    let mut kind_unknown: u64 = 0;
    let mut kind_location: u64 = 0;
    let mut kind_create: u64 = 0;
    let mut kind_destroy: u64 = 0;
    let mut kind_show: u64 = 0;
    let mut kind_hide: u64 = 0;
    // Trace-only owner-drain timestamp shared by every foreground edge this
    // pass: the observation moment joining callback delivery
    // (`queued_mono_ms`) to apply-entry resolution below.
    let drain_mono_ms = super::trace::log_mono_ms();
    let mut foreground_detail: Vec<super::observe::ForegroundEdgeMeta> = Vec::new();
    let mut foreground_omitted: usize = 0;
    for event in events {
        match event {
            HookEvent::Wake(edge) => {
                let raw = edge.hwnd;
                // Own overlay events never wake the loop: no repeated
                // full reconcile off our own move/paint/show.
                if owned_overlay_hwnd(state, RenderCarrier::Border) == Some(raw as u64)
                    || owned_overlay_hwnd(state, RenderCarrier::Underlay) == Some(raw as u64)
                    || owned_overlay_hwnd(state, RenderCarrier::Preview) == Some(raw as u64)
                {
                    continue;
                }
                woke = true;
                match edge.cause {
                    super::observe::HookCause::Foreground => {
                        kind_foreground = kind_foreground.saturating_add(1);
                    }
                    super::observe::HookCause::Visibility => {
                        kind_visibility = kind_visibility.saturating_add(1);
                    }
                    super::observe::HookCause::Other => {
                        kind_other = kind_other.saturating_add(1);
                    }
                    super::observe::HookCause::Unknown => {
                        kind_unknown = kind_unknown.saturating_add(1);
                    }
                }
                // Exact-event split for the same filtered set: LOCATION
                // (mouse/location concern), create/destroy, and show/hide
                // stay distinct instead of folding into `other`. Gate
                // vocabulary above is unchanged; this only attributes.
                match edge.event {
                    EVENT_OBJECT_LOCATIONCHANGE => {
                        kind_location = kind_location.saturating_add(1);
                    }
                    EVENT_OBJECT_CREATE => {
                        kind_create = kind_create.saturating_add(1);
                    }
                    EVENT_OBJECT_DESTROY => {
                        kind_destroy = kind_destroy.saturating_add(1);
                    }
                    EVENT_OBJECT_SHOW => {
                        kind_show = kind_show.saturating_add(1);
                    }
                    EVENT_OBJECT_HIDE => {
                        kind_hide = kind_hide.saturating_add(1);
                    }
                    _ => {}
                }
                // Preserve the foreground edge for event-driven in-flight
                // focus: the world diff resolves it against full native
                // identity plus lifetime at apply entry, distinguishing the
                // product's own landing from a genuine competitor. The
                // production `wake_cause` classification decides here, and
                // the own-overlay exclusion above applies first, so our own
                // border/underlay/paint events never arm in-flight.
                if edge.cause == super::observe::HookCause::Foreground && raw > 0 {
                    foreground.push(raw as u64);
                    if state.trace {
                        if foreground_detail.len() < super::observe::MAX_FOREGROUND_TRACE_EDGES {
                            foreground_detail.push(super::observe::ForegroundEdgeMeta {
                                hwnd: raw as u64,
                                event_ms: edge.time,
                                thread: edge.thread,
                                queued_mono_ms: edge.queued_mono_ms,
                                drain_mono_ms,
                                delivery: edge.delivery.clone(),
                            });
                        } else {
                            foreground_omitted = foreground_omitted.saturating_add(1);
                        }
                    }
                }
                if state.trace {
                    ow_wakes += 1;
                }
            }
            HookEvent::MoveSizeStart(raw, start_seq) => {
                if owned_overlay_hwnd(state, RenderCarrier::Border) == Some(raw as u64)
                    || owned_overlay_hwnd(state, RenderCarrier::Underlay) == Some(raw as u64)
                    || owned_overlay_hwnd(state, RenderCarrier::Preview) == Some(raw as u64)
                {
                    continue;
                }
                woke = true;
                if state.trace {
                    ow_starts += 1;
                }
                let hwnd = raw as u64;
                // Shared START capture (Esc snapshot, PRE frame,
                // identity fence); see `capture_gesture_start`. The
                // single post-loop drain below latches edges for
                // both held and just-ended gestures.
                capture_gesture_start(state, me, hwnd, start_seq);
                state.active.insert(hwnd);
                // Move-vs-resize classification, sampled once at START
                // (the WinEvent itself does not distinguish). Hung or
                // unreadable windows classify Unknown: never a trigger.
                // The probe carries the single sample's facts (cursor
                // point, raw hit or failing stage, PRE rect captured
                // above) so an Unknown attributes without re-probing.
                // No HWND, titles, or identity: tokens and geometry only.
                let probe = classify_move_size_start(hwnd);
                state.move_kind.insert(hwnd, probe.kind);
                log_json_at(
                    log_path,
                    serde_json::json!({
                        "event": "move-kind",
                        "tick": state.tick,
                        "kind": match probe.kind {
                            MoveSizeKind::Move => "move",
                            MoveSizeKind::Resize => "resize",
                            MoveSizeKind::Unknown => "unknown",
                        },
                        "probe": match probe.probe {
                            crate::group_underlay::MoveSizeProbe::CursorUnavailable => "cursor-unavailable",
                            crate::group_underlay::MoveSizeProbe::HitTestUnavailable => "hit-test-unavailable",
                            crate::group_underlay::MoveSizeProbe::Hit(_) => "hit",
                        },
                        "hit": match probe.probe {
                            crate::group_underlay::MoveSizeProbe::Hit(raw) => serde_json::json!(raw),
                            _ => serde_json::Value::Null,
                        },
                        "cursor": probe.cursor.map(|(x, y)| [x, y]),
                        "start_rect": state.gesture_before.get(&hwnd).map(|r| [r.x, r.y, r.w, r.h]),
                    }),
                );
                // START-time pointer for the preview zero gate: a
                // title click with no pointer movement never shows.
                // A failed or out-of-bounds read fails closed to no
                // preview (the refresh hides on `no-start`); the
                // normalized sample also keeps every later ingress
                // carrying it inside report bounds.
                if let Some(point) = super::observation::sample_cursor() {
                    state.gesture_start_cursor.insert(hwnd, point);
                } else {
                    state.gesture_start_cursor.remove(&hwnd);
                }
            }
            HookEvent::MoveSizeEnd(raw, cursor, end_seq) => {
                if owned_overlay_hwnd(state, RenderCarrier::Border) == Some(raw as u64)
                    || owned_overlay_hwnd(state, RenderCarrier::Underlay) == Some(raw as u64)
                    || owned_overlay_hwnd(state, RenderCarrier::Preview) == Some(raw as u64)
                {
                    continue;
                }
                woke = true;
                if state.trace {
                    ow_ends += 1;
                }
                let hwnd = raw as u64;
                // Release cursor carried from the WinEvent callback,
                // never re-read later. Normalized like every other
                // sample (a missing or out-of-bounds capture fails
                // closed to no-change for moves at settle time), so
                // drop any stale entry rather than carrying it forward.
                // A stashed terminal built from these facts always
                // validates, so it can never wedge later reports.
                match super::observation::normalize_cursor(cursor) {
                    Some(point) => {
                        state.gesture_end_cursor.insert(hwnd, point);
                    }
                    None => {
                        state.gesture_end_cursor.remove(&hwnd);
                    }
                }
                // END-time Esc sequence, first END stands: the drain
                // latches ended gestures only against this snapshot,
                // never against a later counter read.
                if let std::collections::hash_map::Entry::Vacant(e) =
                    state.gesture_end_seq.entry(hwnd)
                {
                    e.insert(end_seq);
                }
                state.active.remove(&hwnd);
                // The START-sampled move/size kind travels to settle:
                // `gesture_tick` reads and clears it exactly once per
                // END, so caption-started moves survive a system
                // rescale (mixed-DPI WM_DPICHANGED) that rect
                // comparison alone would misread as a user resize.
                // Preview and underlay keep reading the held entry
                // until settle runs later this same turn.
            }
        }
    }
    // Taskbar-scope invalidation: the pid-scoped hooks deliver Explorer-only
    // create/destroy/show/hide/location traffic here, so game and app
    // windows never appear. Each surviving edge is validated against the
    // taskbar window subtree (bar itself or a bounded ancestor chain);
    // ordinary File Explorer traffic never matches, and unreadable or
    // destroyed handles fail conservatively to stale. While suspended no
    // filter work runs at all: any batch or coalesced overflow only retains
    // the dirty flag for resume, so gaming owns no taskbar work. Bounded
    // live reads, no logging, no sync work.
    let (taskbar_batch, taskbar_overflowed) = crate::ffi::take_taskbar_events();
    if taskbar_overflowed {
        state.pager_taskbar_dirty = true;
    }
    let mut taskbar_hits: u64 = 0;
    if state.suspended {
        if !taskbar_batch.is_empty() {
            state.pager_taskbar_dirty = true;
        }
    } else {
        for event in taskbar_batch {
            if !super::observe::window_edge_wakes(event.event, event.id_object) {
                continue;
            }
            if super::pager_host::taskbar_subtree_hit(event.hwnd) {
                state.pager_taskbar_dirty = true;
                taskbar_hits += 1;
                break;
            }
        }
    }
    if taskbar_hits > 0 {
        woke = true;
        if state.trace {
            ow_wakes += taskbar_hits;
        }
    }
    // Diagnostic-only journey capture (trace opt-in only): recording on
    // this natural turn, never a wake/gate/focus/memory/core input (`woke`
    // untouched). Rows carry the sampled command sequence plus core
    // revision at delivery alongside the drain-time pair: all explicitly
    // sampled, never proof of generation-time order. Array order is
    // callback order. `omitted` folds queue overflow plus the emission
    // cap, so dropped rows never masquerade as full evidence.
    if state.trace && super::observe::switch_diag_enabled() {
        let switch_edges = super::observe::take_switch_edges();
        let queue_dropped = super::observe::take_journey_dropped();
        if !switch_edges.is_empty() || queue_dropped > 0 {
            let drain_mono_ms = super::trace::log_mono_ms();
            let drain_command_seq = state.world.next_command_seq().saturating_sub(1);
            let drain_core_revision = state.world.engine().state_revision();
            let observed_foreground_at_drain = crate::ffi::foreground_window();
            let total = switch_edges.len();
            let mut rows = Vec::with_capacity(total.min(super::observe::MAX_SWITCH_TRACE_EDGES));
            for edge in switch_edges
                .iter()
                .take(super::observe::MAX_SWITCH_TRACE_EDGES)
            {
                rows.push(serde_json::json!({
                    "kind": super::observe::switch_kind_str(edge.event),
                    "hwnd": edge.hwnd,
                    "source_pid": edge.source_pid,
                    "source_tid": edge.source_tid,
                    "event_thread": edge.thread,
                    "event_time": edge.time,
                    "queued_mono_ms": edge.queued_mono_ms,
                    "drain_mono_ms": drain_mono_ms,
                    "observed_foreground_at_delivery": edge.observed_foreground_at_delivery,
                    "observed_foreground_at_drain": observed_foreground_at_drain,
                    "observed_command_seq_at_delivery": edge.observed_command_seq_at_delivery,
                    "observed_core_revision_at_delivery": edge.observed_core_revision_at_delivery,
                    "drain_command_seq": drain_command_seq,
                    "drain_core_revision": drain_core_revision,
                }));
            }
            let omitted = queue_dropped.saturating_add(total.saturating_sub(rows.len()) as u64);
            super::trace::log_json_at(
                log_path,
                serde_json::json!({
                    "event": "switch-journey",
                    "tick": state.tick,
                    "edges": rows,
                    "omitted": omitted,
                    "drain_command_seq": drain_command_seq,
                    "drain_core_revision": drain_core_revision,
                    "ordering": "sampled-published-not-order-proof",
                    "scope": "trace-only-diagnostic",
                }),
            );
        }
    }
    EdgeDrain {
        woke,
        batch_total: ow_batch_total,
        wakes: ow_wakes,
        starts: ow_starts,
        ends: ow_ends,
        foreground,
        kind_foreground,
        kind_visibility,
        kind_other,
        kind_unknown,
        kind_location,
        kind_create,
        kind_destroy,
        kind_show,
        kind_hide,
        taskbar: taskbar_hits,
        foreground_detail,
        foreground_omitted,
    }
}

/// Whether to omit one trace-only wake-attribution detail row as filtered
/// empty-message detail: unstaged message-only wait with no retained
/// effective hook evidence. The raw own-overlay-only batch count is
/// ignored. A filtered row means no retained wake detail, not proof of no
/// owner work.
#[must_use]
pub(super) fn suppress_wake_detail(
    wait: super::observe::WaitWake,
    diag: &OwnerWaitDiag,
    drain: &EdgeDrain,
) -> bool {
    if diag.staged || diag.already_due {
        return false;
    }
    if !(wait.message && !wait.input && !wait.ack && !wait.notify && !wait.timeout && !wait.failed)
    {
        return false;
    }
    if drain.woke {
        return false;
    }
    if drain.wakes != 0 || drain.starts != 0 || drain.ends != 0 || drain.taskbar != 0 {
        return false;
    }
    if drain.kind_foreground != 0
        || drain.kind_visibility != 0
        || drain.kind_other != 0
        || drain.kind_unknown != 0
        || drain.kind_location != 0
        || drain.kind_create != 0
        || drain.kind_destroy != 0
        || drain.kind_show != 0
        || drain.kind_hide != 0
    {
        return false;
    }
    if !drain.foreground.is_empty()
        || !drain.foreground_detail.is_empty()
        || drain.foreground_omitted != 0
    {
        return false;
    }
    true
}

/// Trace-only per-pass wake attribution: what woke this pass, on this
/// pass. The wait return cause set (input/ack/message/notify/timeout/
/// failed flags plus the selected deadline source and its requested vs
/// elapsed time) rides alongside the drained hook-kind counts and the
/// bounded correlated foreground rows, so a later leg can join the actual
/// waker to the pass instead of an aggregate once-per-throttle summary. A
/// cause set only: several flags may hold at once and no single waker is
/// claimed. Commands are attributed by tick: the `world-apply` row for the
/// same tick carries the consumed command kind. Trace-gated by the caller;
/// reads member tables and at most [`super::observe::MAX_FOREGROUND_TRACE_ROWS`]
/// native class names, never drains, arms, or actuates anything.
pub(super) fn log_wake_attribution(
    state: &TileLoop,
    wait: super::observe::WaitWake,
    diag: OwnerWaitDiag,
    drain: &EdgeDrain,
    log_path: &Path,
) {
    let rows: Vec<serde_json::Value> = drain
        .foreground_detail
        .iter()
        .take(super::observe::MAX_FOREGROUND_TRACE_ROWS)
        .map(|meta| {
            let managed = state.managed.contains(&meta.hwnd);
            let views: Vec<super::observe::MemberJoinView<'_>> = state
                .member_tokens
                .iter()
                .filter(|(key, _)| key.hwnd == meta.hwnd)
                .map(|(key, token)| super::observe::MemberJoinView {
                    hwnd: key.hwnd,
                    pid: key.pid,
                    token: token.as_str(),
                    stored_tag: state.member_tags.get(key).map(String::as_str),
                })
                .collect();
            let (token, reason) =
                super::observe::join_delivery_token(meta.hwnd, &meta.delivery, &views);
            let class = super::observe::class_of(meta.hwnd as isize as HWND);
            let mut row = serde_json::json!({
                "managed": managed,
                "token": token,
                "join": reason,
                "delivery": meta.delivery.as_str(),
                "tagged": meta.delivery.has_tag(),
                "event_ms": meta.event_ms,
                "thread": meta.thread,
                "queued_mono_ms": meta.queued_mono_ms,
                "drain_mono_ms": meta.drain_mono_ms,
            });
            if class.is_empty() {
                row["class"] = serde_json::Value::Null;
            } else {
                row["class"] = serde_json::Value::from(class);
            }
            row
        })
        .collect();
    let omitted = drain
        .foreground_detail
        .len()
        .saturating_sub(rows.len())
        .saturating_add(drain.foreground_omitted);
    let source = OWNER_WAIT_SOURCE_ORDER
        .get(diag.source as usize)
        .copied()
        .unwrap_or("unknown");
    super::trace::log_json_at(
        log_path,
        serde_json::json!({
            "event": "wake-attribution",
            "tick": state.tick,
            "staged": diag.staged,
            "wait_source": source,
            "requested_ms": diag.requested_ms,
            "elapsed_ms": diag.elapsed_ms,
            "already_due": diag.already_due,
            "woke": {
                "input": wait.input,
                "ack": wait.ack,
                "message": wait.message,
                "notify": wait.notify,
                "timeout": wait.timeout,
                "failed": wait.failed,
            },
            "hook_kinds": {
                "foreground": drain.kind_foreground,
                "visibility": drain.kind_visibility,
                "other": drain.kind_other,
                "unknown": drain.kind_unknown,
                "location": drain.kind_location,
                "create": drain.kind_create,
                "destroy": drain.kind_destroy,
                "show": drain.kind_show,
                "hide": drain.kind_hide,
                "taskbar": drain.taskbar,
            },
            "foreground": rows,
            "foreground_omitted": omitted,
        }),
    );
}

#[cfg(test)]
mod edge_kind_tests {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EVENT_OBJECT_CREATE, EVENT_OBJECT_DESTROY, EVENT_OBJECT_HIDE, EVENT_OBJECT_LOCATIONCHANGE,
        EVENT_OBJECT_SHOW, EVENT_SYSTEM_FOREGROUND,
    };

    /// Exact-event wake attribution stays distinct: LOCATION (mouse/location
    /// concern), create, destroy, show, and hide never fold into one
    /// `other` bucket. Gate vocabulary (`foreground`/`visibility`/
    /// `other`) is unchanged; this only attributes.
    #[test]
    fn drain_counts_exact_events_distinctly() {
        use super::super::observe::{DeliverySnapshot, HookEvent, WakeEdge, hook_queue};
        use super::drain_hook_edges;
        let mut state = super::super::rmax03_adapter_tests::test_state();
        state.trace = false;
        if let Ok(mut queue) = hook_queue().lock() {
            queue.clear();
        }
        let push = |hwnd: isize, event: u32| {
            if let Ok(mut queue) = hook_queue().lock() {
                queue.push(HookEvent::Wake(WakeEdge::test_with_event(
                    hwnd,
                    event,
                    DeliverySnapshot::Unknown,
                )));
            }
        };
        push(11, EVENT_SYSTEM_FOREGROUND);
        push(12, EVENT_OBJECT_SHOW);
        push(13, EVENT_OBJECT_HIDE);
        push(14, EVENT_OBJECT_CREATE);
        push(15, EVENT_OBJECT_DESTROY);
        push(16, EVENT_OBJECT_LOCATIONCHANGE);
        let me = crate::model::ProcessIdentity {
            pid: 9999,
            process_creation: "test-creation".to_owned(),
            user_sid: "S-1-5-21-1000".to_owned(),
            session_id: 1,
            exe_path: "C:\\Apps\\app.exe".to_owned(),
        };
        let log = state.log_path.clone();
        let drain = drain_hook_edges(&mut state, &me, &log);
        assert!(drain.woke);
        assert_eq!(drain.kind_foreground, 1);
        assert_eq!(drain.kind_show, 1);
        assert_eq!(drain.kind_hide, 1);
        assert_eq!(drain.kind_create, 1);
        assert_eq!(drain.kind_destroy, 1);
        assert_eq!(drain.kind_location, 1);
        // Gate vocabulary unchanged: show/hide ride visibility, the
        // create/destroy/location topology rides other.
        assert_eq!(drain.kind_visibility, 2);
        assert_eq!(drain.kind_other, 3);
        assert_eq!(drain.kind_unknown, 0);
        // Only foreground HWNDs arm the in-flight join.
        assert_eq!(drain.foreground, vec![11u64]);
    }

    /// Trace-only journey capture is recording-only: the drain logs one
    /// `switch-journey` row per edge in callback order without waking,
    /// gating, or touching foreground/core state. Delivery rows carry the
    /// sampled command sequence plus core revision; the drain pair rides
    /// alongside, never substituted.
    #[test]
    fn switch_drain_records_without_waking() {
        use super::super::observe::{
            SwitchEdge, hook_queue, publish_journey_snapshot, push_switch, set_switch_diag_enabled,
            take_journey_dropped, take_switch_edges,
        };
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_SWITCHEND, EVENT_SYSTEM_SWITCHSTART,
        };
        static SWITCH_LOG_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        use super::drain_hook_edges;
        let mut state = super::super::rmax03_adapter_tests::test_state();
        state.trace = true;
        // The shared harness points at the NUL sink (no log-byte asserts);
        // this test reroutes to a temp file for its reporting assertions.
        let seq = SWITCH_LOG_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "tiler-switch-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0),
            seq
        ));
        std::fs::create_dir_all(&dir).expect("switch log dir");
        let log = dir.join("run.log");
        state.log_path = log.clone();
        if let Ok(mut queue) = hook_queue().lock() {
            queue.clear();
        }
        assert!(take_switch_edges().is_empty());
        assert_eq!(take_journey_dropped(), 0);
        // Direct push (no native reads): the push path itself is ungated;
        // the callback gate and the drain log are what the flag controls.
        // Delivery snapshots are crafted here; the callback-sampling path
        // is covered in `observe::switch_tests`.
        for (hwnd, event, time, delivery_seq, delivery_rev) in [
            (71u64, EVENT_SYSTEM_SWITCHSTART, 31u32, 41u64, 7u64),
            (71u64, EVENT_SYSTEM_FOREGROUND, 32u32, 41u64, 7u64),
            (72u64, EVENT_SYSTEM_SWITCHEND, 33u32, 42u64, 8u64),
        ] {
            push_switch(SwitchEdge {
                hwnd,
                event,
                source_pid: 501,
                source_tid: 502,
                thread: 5,
                time,
                queued_mono_ms: 100,
                observed_foreground_at_delivery: 70,
                observed_command_seq_at_delivery: delivery_seq,
                observed_core_revision_at_delivery: delivery_rev,
            });
        }
        publish_journey_snapshot(0, 0);
        set_switch_diag_enabled(true);
        let me = crate::model::ProcessIdentity {
            pid: 9999,
            process_creation: "test-creation".to_owned(),
            user_sid: "S-1-5-21-1000".to_owned(),
            session_id: 1,
            exe_path: "C:\\Apps\\app.exe".to_owned(),
        };
        let drain_command_seq = state.world.next_command_seq().saturating_sub(1);
        let drain_core_revision = state.world.engine().state_revision();
        let drain = drain_hook_edges(&mut state, &me, &log);
        set_switch_diag_enabled(false);
        // Recording only: no wake, no foreground arming, no hook counts.
        assert!(!drain.woke);
        assert!(drain.foreground.is_empty());
        assert!(drain.foreground_detail.is_empty());
        assert_eq!(drain.wakes, 0);
        assert_eq!(drain.kind_foreground, 0);
        assert!(take_switch_edges().is_empty());
        let text = std::fs::read_to_string(&log).unwrap_or_default();
        let line = text
            .lines()
            .find(|line| line.contains("\"event\":\"switch-journey\""))
            .expect("switch-journey line logged");
        for token in [
            "\"kind\":\"switchstart\"",
            "\"kind\":\"foreground\"",
            "\"kind\":\"switchend\"",
            "\"hwnd\":71",
            "\"hwnd\":72",
            "\"source_pid\":501",
            "\"source_tid\":502",
            "\"event_thread\":5",
            "\"event_time\":31",
            "\"observed_foreground_at_delivery\":70",
            "\"observed_foreground_at_drain\":",
            "\"observed_command_seq_at_delivery\":41",
            "\"observed_command_seq_at_delivery\":42",
            "\"observed_core_revision_at_delivery\":7",
            "\"observed_core_revision_at_delivery\":8",
            "\"ordering\":\"sampled-published-not-order-proof\"",
            "\"scope\":\"trace-only-diagnostic\"",
        ] {
            assert!(line.contains(token), "missing {token}: {line}");
        }
        assert!(
            line.contains(&format!("\"drain_command_seq\":{drain_command_seq}")),
            "drain seq alongside delivery, never substituted: {line}"
        );
        assert!(
            line.contains(&format!("\"drain_core_revision\":{drain_core_revision}")),
            "drain revision alongside delivery, never substituted: {line}"
        );
        // Callback order survives: switchstart, foreground, switchend.
        let kinds: Vec<&str> = ["switchstart", "foreground", "switchend"].to_vec();
        let mut cursor = 0;
        for kind in kinds {
            let token = format!("\"kind\":\"{kind}\"");
            let next = line[cursor..]
                .find(&token)
                .unwrap_or_else(|| panic!("missing {token}: {line}"));
            cursor += next + token.len();
        }
    }

    /// Omission accounting: queue overflow plus the emission cap fold into
    /// one honest `omitted`, never masquerading as full evidence.
    #[test]
    fn switch_drain_omitted_counts_overflow_and_cap() {
        use super::super::observe::{
            MAX_JOURNEY_QUEUE, MAX_SWITCH_TRACE_EDGES, SwitchEdge, hook_queue, push_switch,
            set_switch_diag_enabled, take_journey_dropped, take_switch_edges,
        };
        use super::drain_hook_edges;
        use windows_sys::Win32::UI::WindowsAndMessaging::EVENT_SYSTEM_SWITCHEND;
        static SWITCH_OMIT_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let mut state = super::super::rmax03_adapter_tests::test_state();
        state.trace = true;
        let seq = SWITCH_OMIT_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "tiler-switch-omit-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0),
            seq
        ));
        std::fs::create_dir_all(&dir).expect("switch omit log dir");
        let log = dir.join("run.log");
        state.log_path = log.clone();
        if let Ok(mut queue) = hook_queue().lock() {
            queue.clear();
        }
        assert!(take_switch_edges().is_empty());
        assert_eq!(take_journey_dropped(), 0);
        for hwnd in 0..(MAX_JOURNEY_QUEUE as u64 + 7) {
            push_switch(SwitchEdge {
                hwnd,
                event: EVENT_SYSTEM_SWITCHEND,
                source_pid: 0,
                source_tid: 0,
                thread: 0,
                time: 0,
                queued_mono_ms: 0,
                observed_foreground_at_delivery: 0,
                observed_command_seq_at_delivery: 0,
                observed_core_revision_at_delivery: 0,
            });
        }
        set_switch_diag_enabled(true);
        let me = crate::model::ProcessIdentity {
            pid: 9999,
            process_creation: "test-creation".to_owned(),
            user_sid: "S-1-5-21-1000".to_owned(),
            session_id: 1,
            exe_path: "C:\\Apps\\app.exe".to_owned(),
        };
        let drain = drain_hook_edges(&mut state, &me, &log);
        set_switch_diag_enabled(false);
        assert!(!drain.woke);
        assert!(take_switch_edges().is_empty());
        let expected = 7 + (MAX_JOURNEY_QUEUE - MAX_SWITCH_TRACE_EDGES) as u64;
        let text = std::fs::read_to_string(&log).unwrap_or_default();
        let line = text
            .lines()
            .find(|line| line.contains("\"event\":\"switch-journey\""))
            .expect("switch-journey line logged");
        assert!(
            line.contains(&format!("\"omitted\":{expected}")),
            "overflow plus cap fold into omitted: {line}"
        );
    }

    /// Disabled (default) capture drains nothing and logs nothing:
    /// no hook installed, no callback work, no drain work.
    #[test]
    fn switch_drain_disabled_logs_nothing() {
        use super::super::observe::{
            SwitchEdge, push_switch, take_journey_dropped, take_switch_edges,
        };
        use super::drain_hook_edges;
        use windows_sys::Win32::UI::WindowsAndMessaging::EVENT_SYSTEM_SWITCHEND;
        let mut state = super::super::rmax03_adapter_tests::test_state();
        state.trace = true;
        assert!(take_switch_edges().is_empty());
        assert_eq!(take_journey_dropped(), 0);
        push_switch(SwitchEdge {
            hwnd: 81,
            event: EVENT_SYSTEM_SWITCHEND,
            source_pid: 0,
            source_tid: 0,
            thread: 0,
            time: 0,
            queued_mono_ms: 0,
            observed_foreground_at_delivery: 0,
            observed_command_seq_at_delivery: 0,
            observed_core_revision_at_delivery: 0,
        });
        // Flag off (default): the drain must not consume or report.
        let me = crate::model::ProcessIdentity {
            pid: 9999,
            process_creation: "test-creation".to_owned(),
            user_sid: "S-1-5-21-1000".to_owned(),
            session_id: 1,
            exe_path: "C:\\Apps\\app.exe".to_owned(),
        };
        let log = state.log_path.clone();
        let drain = drain_hook_edges(&mut state, &me, &log);
        assert!(!drain.woke);
        // Stale disabled edges stay queued (never consumed while off), so
        // clear explicitly to leave no residue for other tests.
        assert_eq!(take_switch_edges().len(), 1);
        assert_eq!(take_journey_dropped(), 0);
        assert!(take_switch_edges().is_empty());
        let text = std::fs::read_to_string(&log).unwrap_or_default();
        assert!(
            !text.contains("switch-journey"),
            "disabled must not log: {text}"
        );
    }
}

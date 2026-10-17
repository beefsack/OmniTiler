#![forbid(unsafe_code)]
use super::*;

// Per-turn WinEvent edge drain for the tile loop: takes the queued hook
// callbacks once, filters own-overlay events, and captures gesture START
// state. Nested `continue`s below stay on the drain loop.
pub(super) struct EdgeDrain {
    pub woke: bool,
    pub batch_total: u64,
    pub wakes: u64,
    pub starts: u64,
    pub ends: u64,
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
    let events = hook_queue()
        .lock()
        .map(|mut queue| std::mem::take(&mut *queue))
        .unwrap_or_default();
    let mut woke = false;
    // Trace-only batch breakdown for the owner-work sample; default
    // runs skip the counting.
    let ow_batch_total: u64 = if state.trace { events.len() as u64 } else { 0 };
    let mut ow_wakes: u64 = 0;
    let mut ow_starts: u64 = 0;
    let mut ow_ends: u64 = 0;
    for event in events {
        match event {
            HookEvent::Wake(raw) => {
                // Own overlay events never wake the loop: no repeated
                // full reconcile off our own move/paint/show.
                if owned_overlay_hwnd(state, RenderCarrier::Border) == Some(raw as u64)
                    || owned_overlay_hwnd(state, RenderCarrier::Underlay) == Some(raw as u64)
                    || owned_overlay_hwnd(state, RenderCarrier::Preview) == Some(raw as u64)
                {
                    continue;
                }
                woke = true;
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
                let kind = classify_move_size_start(hwnd);
                state.move_kind.insert(hwnd, kind);
                log_json_at(
                    log_path,
                    serde_json::json!({
                        "event": "move-kind",
                        "tick": state.tick,
                        "kind": match kind {
                            MoveSizeKind::Move => "move",
                            MoveSizeKind::Resize => "resize",
                            MoveSizeKind::Unknown => "unknown",
                        },
                    }),
                );
                // START-time pointer for the preview zero gate: a
                // title click with no pointer movement never shows.
                // A failed read fails closed to no preview (the
                // refresh hides on `no-start`).
                if let Some(point) = cursor_pos() {
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
                // never re-read later. A missing capture fails closed
                // to no-change for moves at settle time, so drop any
                // stale entry rather than carrying it forward.
                match cursor {
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
    EdgeDrain {
        woke,
        batch_total: ow_batch_total,
        wakes: ow_wakes,
        starts: ow_starts,
        ends: ow_ends,
    }
}

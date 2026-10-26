#![forbid(unsafe_code)]
use super::*;

use super::owner_work::OwnerWorkGuard;
use super::pager_host::poll_pager_select;

// Active-tick phases for the tile loop: monitor/taskbar refresh, CLI wakes,
// the idle gate, suspend/resume, gesture settle and dispatch, plus the
// end-of-turn commit.
// Outer-loop `continue`s become `TickFlow` returns; nested loop control flow
// below is untouched.
pub(super) enum TickFlow {
    Idle,
    Suspended,
    Paused,
    Done,
}

/// Name the suspension mode honestly in logs: `limited` keeps
/// geometry for other windows running, `full` suspends everything.
fn suspend_mode(limited: bool) -> &'static str {
    if limited { "limited" } else { "full" }
}

/// Enter or continue a suspension, logging the entry and any mode
/// transition while suspended (full <-> limited) so an ongoing mode
/// change is visible without per-tick noise. Geometry outcomes keep
/// flowing through the existing per-window/tick diagnostics; the
/// suspend line itself never claims all tiling stopped in limited mode.
#[allow(clippy::too_many_arguments)]
fn enter_suspend(
    state: &mut TileLoop,
    dir: &Path,
    me: &ProcessIdentity,
    store: &LedgerStore,
    snap_want: bool,
    now: Instant,
    log_path: &Path,
    limited: bool,
) {
    if !state.suspended {
        state.suspended = true;
        state.suspended_limited = limited;
        state.snap_advance = None;
        // Monotonic suspend start for the resume elapsed timing below;
        // logging only, never a deadline or behavior change.
        state.suspend_started = Some(now);
        log_json_at(
            log_path,
            serde_json::json!({"event":"suspend","tick":state.tick,"cause":"fullscreen-foreground","mode":suspend_mode(limited)}),
        );
        if snap_want {
            snap_suspend(dir, me, store);
        }
    } else {
        state.snap_advance = None;
        if state.suspended_limited != limited {
            state.suspended_limited = limited;
            log_json_at(
                log_path,
                serde_json::json!({"event":"suspend-mode","tick":state.tick,"cause":"fullscreen-foreground","mode":suspend_mode(limited)}),
            );
        }
    }
}

/// Drop one batch of intents on a blocked turn with the shared suspended
/// outcome (never dispatched under a covering foreground), then consume
/// the out-of-hook select/resize polls the same way. Used by both the
/// full and the limited lanes so the two never drift.
#[allow(clippy::too_many_arguments)]
fn drop_blocked_intents(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
    snap_events: Vec<QueuedSnapEvent>,
    workspace_events: Vec<crate::snapkey::QueuedWorkspaceIntent>,
    send_events: Vec<QueuedWorkspaceSendIntent>,
    output_events: Vec<QueuedWorkspaceOutputIntent>,
    migrate_events: Vec<QueuedWorkspaceMigrateIntent>,
    history_events: Vec<QueuedWorkspaceHistoryIntent>,
    tray: &mut Option<crate::tray_sys::TrayOwner>,
) {
    if !snap_events.is_empty() {
        keyboard_tick(
            state,
            me,
            store,
            dir,
            fulls,
            areas,
            snap_events,
            Some("suspended"),
        );
    }
    if !workspace_events.is_empty() {
        workspace_tick(
            state,
            me,
            store,
            dir,
            fulls,
            areas,
            workspace_events,
            Some("suspended"),
        );
    }
    if !send_events.is_empty() {
        workspace_relative_send_tick(
            state,
            me,
            store,
            dir,
            fulls,
            areas,
            send_events,
            Some("suspended"),
        );
    }
    if !output_events.is_empty() {
        workspace_output_send_tick(
            state,
            me,
            store,
            dir,
            fulls,
            areas,
            output_events,
            Some("suspended"),
        );
    }
    if !migrate_events.is_empty() {
        workspace_migrate_tick(
            state,
            me,
            store,
            dir,
            fulls,
            areas,
            migrate_events,
            Some("suspended"),
        );
    }
    if !history_events.is_empty() {
        workspace_history_tick(
            state,
            me,
            store,
            dir,
            fulls,
            areas,
            history_events,
            Some("suspended"),
        );
    }
    // Out-of-hook select observes the same suspension: consumed
    // once with a suspended outcome, never applied while a
    // fullscreen foreground holds the session. The test-needed
    // resize and directional polls carry their own suspend fences
    // and refuse the same way.
    poll_workspace_cli_request(state, me, store, dir, fulls, areas);
    tray_select::poll_tray_select(state, me, store, dir, fulls, areas, tray);
    poll_pager_select(state, me, store, dir, fulls, areas);
    poll_resize_cli_request(state, me, store, dir, fulls, areas);
    poll_directional_cli_request(state, me, store, dir, fulls, areas);
    poll_send_output_cli_request(state, me, store, dir, fulls, areas);
    poll_migrate_workspace_cli_request(state, me, store, dir, fulls, areas);
}

/// Clear held-gesture and preview state on a blocked turn plus the
/// process-global Esc edge (ordered FIFO through the input bridge, pump
/// order on proof paths), so a stale pre-suspend edge never cancels a
/// later gesture. Ordering, not atomicity, carries the guarantee: the
/// edge drains here before any later START can observe it.
///
/// Before clearing, any open gesture's terminal stashes as a Cancelled
/// ingress for its id: a suspend/stop must never leak a retained core
/// session, and the next eligible observation (after resume) incorporates
/// the terminal. No tick runs here, so nothing actuates while blocked.
pub(super) fn clear_blocked_gesture(state: &mut TileLoop) {
    // Stash at most one terminal (native single-gesture): the first open
    // gesture in stable HWND order. Any further open holds are unreachable
    // in production (the adapter fails multi-gesture closed), and their
    // tables clear below without a stash; a fresh START re-binds them.
    let mut open: Vec<u64> = state.gesture_seq.keys().copied().collect();
    open.sort_unstable();
    if let Some(hwnd) = open.first().copied()
        && let Some(id) = state.gesture_seq.get(&hwnd).copied()
    {
        let token = state
            .member_tokens
            .iter()
            .find(|(key, _)| key.hwnd == hwnd)
            .map(|(_, token)| token.clone())
            .or_else(|| {
                state
                    .gesture_preview_start
                    .get(&hwnd)
                    .map(|start| start.token.clone())
            });
        let kind = match state.move_kind.get(&hwnd) {
            Some(MoveSizeKind::Move) => tiler_core::gesture::GestureKind::Move,
            Some(MoveSizeKind::Resize) => tiler_core::gesture::GestureKind::Resize,
            _ => tiler_core::gesture::GestureKind::Move,
        };
        let frame = state
            .gesture_before
            .get(&hwnd)
            .copied()
            .or_else(|| state.stable.get(&hwnd).copied());
        if let (Some(token), Some(frame)) = (token, frame) {
            super::gesture_tick::stash_gesture_cancel(
                state,
                hwnd,
                id,
                &token,
                kind,
                kind == tiler_core::gesture::GestureKind::Move,
                frame,
            );
        }
    }
    state.gesture_before.clear();
    state.active.clear();
    state.move_kind.clear();
    state.esc_latched.clear();
    state.gesture_esc_seq.clear();
    state.gesture_end_seq.clear();
    state.gesture_end_cursor.clear();
    state.gesture_start_key.clear();
    state.gesture_start_tag.clear();
    state.preview_bound.clear();
    state.preview_dead.clear();
    state.gesture_preview_start.clear();
    state.gesture_start_cursor.clear();
    state.gesture_started_sent.clear();
    state.gesture_seq.clear();
    // A stale Esc edge must not leak into the next gesture.
    // Routes through the input bridge on the product path
    // (ordered FIFO with snapshots; held pairs keep their stored
    // verdicts until hook release) and clears the process-global
    // edge flags in pump order via the ClearEsc apply; direct
    // sys calls on proof paths.
    offer_input_control(state, InboxItem::ClearEsc);
}

#[allow(clippy::too_many_arguments)]
pub(super) fn run_active_tick(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    dir: &Path,
    store: &LedgerStore,
    areas: &mut Vec<MonitorArea>,
    monitor: &mut MonitorArea,
    monitor_count: &mut usize,
    now: Instant,
    monitor_check: bool,
    trace_emit: bool,
    mut woke: bool,
    hook_drain_woke: bool,
    log_path: &Path,
    owner_work: &mut OwnerWorkGuard<'_>,
    ow_counts: &OwnerWaitCounts,
    snap_want: bool,
    snap_primed: &mut bool,
    snap_retry_at: &mut Option<Instant>,
    ow_batch_total: u64,
    ow_wakes: u64,
    ow_starts: u64,
    ow_ends: u64,
    snap_events: Vec<QueuedSnapEvent>,
    workspace_events: Vec<crate::snapkey::QueuedWorkspaceIntent>,
    send_events: Vec<QueuedWorkspaceSendIntent>,
    output_events: Vec<QueuedWorkspaceOutputIntent>,
    migrate_events: Vec<QueuedWorkspaceMigrateIntent>,
    history_events: Vec<QueuedWorkspaceHistoryIntent>,
    tray: &mut Option<crate::tray_sys::TrayOwner>,
) -> Result<TickFlow> {
    // Monitor re-read on the display-broadcast dirty flag and on the
    // windowless fallback cadence: a changed topology relocates Engine
    // sessions exactly like before, only the trigger is event-driven
    // instead of a fixed 2s poll.
    if monitor_check {
        let fresh = all_monitors()?;
        let devices: Vec<String> = areas.iter().map(|a| a.device.clone()).collect();
        let fresh_devices: Vec<String> = fresh.iter().map(|a| a.device.clone()).collect();
        let geometry_changed = fresh.len() != *monitor_count
            || fresh_devices != devices
            || fresh.iter().any(|a| {
                areas
                    .iter()
                    .find(|b| b.device == a.device)
                    .is_none_or(|b| b.work != a.work || b.full != a.full)
            });
        if geometry_changed {
            let before: Vec<[i32; 4]> = areas
                .iter()
                .map(|a| [a.work.x, a.work.y, a.work.w, a.work.h])
                .collect();
            let after: Vec<[i32; 4]> = fresh
                .iter()
                .map(|a| [a.work.x, a.work.y, a.work.w, a.work.h])
                .collect();
            *areas = fresh;
            *monitor = areas[0].clone();
            *monitor_count = areas.len();
            // Arrival/removal plus bounds changes are core-owned topology:
            // the mechanical device-to-wire sync runs, and the next world
            // report carries the fresh screens for the core to reproject
            // against. No adapter session mutation.
            sync_monitor_outputs(state, areas);
            log_json_at(
                log_path,
                serde_json::json!({
                    "event": "work-area-changed",
                    "tick": state.tick,
                    "before": before,
                    "after": after,
                }),
            );
            *snap_retry_at = None;
            // A changed work area is a meaningful retry point for a
            // degraded Snap setup; quiet when the effect holds.
            if snap_want && !state.suspended {
                snap_resume(dir, me, store);
                *snap_primed = true;
            }
            woke = true;
        }
    }
    // Trace-only pump-waits summary on the elapsed throttle, riding this
    // existing natural turn only (never a diagnostic wake, never tied to
    // a monitor check): one cumulative read-only line per due turn. This
    // tick runs only on full turns (ack-only returns continue before it),
    // so the throttle never reconciles or wakes on its own. Reads only
    // (diag snapshot plus renderer atomics/ack seq); never arms owner
    // work and never signals a pump wake, so the summary cannot induce a
    // wake. Owner counts cover outer product waits only (focus/proof
    // nested waits excluded by construction); ack-only turns counted in
    // `owner_ack_only` return before this tick, so this line rides full
    // turns. Input `turns` counts completed iterations and equals
    // `input_waits`; stop before the wait increments neither; `*_timeout`
    // fields count genuine `WAIT_TIMEOUT` (258) expiries only while
    // `*_failed` counts `WAIT_FAILED` / unrecognized codes separately (a
    // failed wait returns immediately; not proven delayed). Reason splits
    // count native wait returns only, never context switches; render acks
    // ride `render_acks` (`ack_seq`), never the wait counts. The input
    // struct arrives under one diag-slot lock (one coherent snapshot);
    // the renderer tuple is four independent atomic loads, so a
    // concurrent bump can skew the sum by one, and cross-pump coherence
    // is not claimed. Start/end diffs take both lines from one traced
    // run; stop short-circuits (owner return, input early return,
    // renderer counted stop wait) are the only lifecycle exclusions; a
    // quiet run additionally flushes one final `end-of-run` line at clean
    // loop exit. The `owner_notify` hit counts directory-watch and
    // async-completion wakes.
    if trace_emit {
        let (input, render) = pump_waits_snapshots(state);
        log_json_at(log_path, pump_waits_line(ow_counts, input, render, None));
    }
    // Exact-owner out-of-hook select wakes the loop even with no other
    // event; the poll itself consumes once and dispatches through the
    // existing resolver. Checked before the idle skip so a pending
    // request never waits for an unrelated wake.
    if dir.join(WORKSPACE_REQUEST_FILE).exists()
        || dir.join(RESIZE_REQUEST_FILE).exists()
        || dir.join(DIRECTIONAL_REQUEST_FILE).exists()
        || dir.join(SEND_OUTPUT_REQUEST_FILE).exists()
    {
        woke = true;
    }
    // Bare-chord edge wake: GetAsyncKeyState levels are sampled here,
    // before the idle skip, so a Win+Shift press/release with no WinEvent
    // wakes the visual refresh on the transition's own wake (input-pump
    // chord signal on the product path, hook-callback messages on the
    // legacy path) instead of waiting for an unrelated event. Steady hold
    // stays quiet; the suspend/fullscreen path below still gates the
    // refresh, and tiling reconciliation keeps its normal pacing
    // (only transition ticks reconcile). No hook, no new framework.
    {
        let (win_held, shift_held) = chord_held_now();
        let chord = tiler_core::visual::group_underlay_chord_held(win_held, shift_held);
        if state.underlay_chord_last != chord {
            state.underlay_chord_last = chord;
            woke = true;
        }
    }
    // Armed restore demands a reconcile once observed; the wake
    // survives pending dispatches and gesture/suspend pauses and is
    // consumed only by a gated reconcile below, on expiry, or on loss.
    if state.restore_wake.is_some() {
        let (expired, lost, done) =
            restore_wake_probe(state, state.restore_wake.as_ref().expect("armed"), now);
        let (keep, demand) = crate::tiling::restore_wake_step(true, expired, lost, done, false);
        if demand {
            woke = true;
        }
        if !keep {
            state.restore_wake = None;
        }
    }
    // An open managed gesture holds the loop for the drop-preview track:
    // pointer moves coalesce into one Engine preview sample per tick on
    // the native frame-following producer, driven by the armed
    // gesture-sampling deadline (pointer motion itself produces no owner
    // wake). The refresh itself gates resize/unknown/invalid holds; the
    // wake only needs START capture.
    if !woke && gesture_sampling_open(state) {
        woke = true;
    }
    // Focus-follows-cursor motion wake (event-driven, no poll): consume a
    // pending tap wake with the flags the last suspend decision left.
    // Arming happens after the suspend decision below, so a game holds
    // the session before any install can run; an idle cursor never wakes.
    if !woke && state.cursor_wake.take_wake() {
        woke = true;
    }
    // Trace-only owner-work gate: quiet event-less iterations drop the
    // disabled guard silently above via `continue`. Active wakes arm
    // the single sample; RAII then records suspend/held/error exits.
    // Due health summaries also flush on the idle path below (no new
    // wake: the check rides this existing turn, and quiet windows
    // with no activity emit nothing).
    if state.trace && (woke || monitor_check) {
        let gesture_held = state.active.iter().any(|hwnd| state.managed.contains(hwnd));
        owner_work.arm(ow_batch_total, ow_wakes, ow_starts, ow_ends, gesture_held);
    }
    // Per-turn maintenance-quiet flag for the trailing health
    // flush at the end of the turn (set on the quiet branch).
    let mut maintenance_quiet = false;
    if state.trace && !(woke || monitor_check) {
        owner_work.finish_turn(false);
    }
    if !(woke || monitor_check) {
        return Ok(TickFlow::Idle);
    }
    let fulls = monitor_fulls(areas);
    let (suspend_read_cached, suspend_fg) = suspend_limited(state, me, &fulls);
    // Limited unmanageable fullscreen: the game/EAC window stays excluded
    // and untouched, automatic focus stays blocked, and overlays stay
    // hidden, but other verified managed windows still reconcile
    // non-activatingly through the shared geometry gate (which excludes the
    // foreground HWND per write). Any other blocking shape, or an unknown
    // foreground, keeps the existing full suspension with no reconcile.
    let limited = is_limited_fullscreen(&suspend_read_cached) && suspend_fg != 0;
    if suspend_read_cached.veto.block {
        enter_suspend(state, dir, me, store, snap_want, now, log_path, limited);
        // A fullscreen game holds the session: park the cursor-motion tap
        // on entry (no later tick may install it mid-game) and drain
        // pending motion so the resume turn never wakes on stale movement.
        // A failed release retains the handle and retries with backoff.
        state.cursor_wake.set_suspended(true, now);
        if let Some(diag) = state.cursor_wake.take_diag() {
            log_json_at(
                log_path,
                serde_json::json!({"event":"cursor-tap","op":diag.op,"ok":diag.ok}),
            );
        }
        drop_blocked_intents(
            state,
            me,
            store,
            dir,
            &fulls,
            areas,
            snap_events,
            workspace_events,
            send_events,
            output_events,
            migrate_events,
            history_events,
            tray,
        );
        clear_blocked_gesture(state);
        if limited {
            // Narrow geometry-only lane for other windows: the single
            // captured foreground HWND is excluded per write, focus is
            // forced off in planning, and no admission, un-maximize,
            // marker, visibility, close, or overlay effect runs. Intents
            // above already dropped; overlays below stay hidden.
            reconcile_limited_tick(state, me, store, dir, &fulls, areas, suspend_fg);
        }
        hide_border(state, "suspended");
        hide_underlay(state, "suspended");
        hide_preview(state, "suspended");
        // Suspended turns still commit their owner sample (the
        // guard would otherwise record it silently on drop with
        // no summary flush); then the disarmed guard drops.
        if state.trace {
            owner_work.finish_turn(false);
        }
        return Ok(TickFlow::Suspended);
    }
    if state.suspended {
        let was_limited = state.suspended_limited;
        state.suspended = false;
        state.suspended_limited = false;
        // Resume re-arms the cursor-motion tap when still wanted; motion
        // that arrived while parked never wakes.
        state.cursor_wake.set_suspended(false, now);
        // Monotonic suspended duration from the suspend start above, so a
        // later incident row can measure the reported delay from normal logs
        // alone. `None` (suspend predates this process snapshot) reads null.
        let suspended_ms: Option<u64> = state.suspend_started.take().map(|at| {
            now.saturating_duration_since(at)
                .as_millis()
                .min(u128::from(u64::MAX)) as u64
        });
        log_json_at(
            log_path,
            serde_json::json!({"event":"resume","tick":state.tick,"suspended_ms":suspended_ms,"mode":suspend_mode(was_limited)}),
        );
        // A fresh foreground may accept the hook now: retry soon.
        *snap_retry_at = None;
        // Fresh Snap preimage when the setting drifted during
        // suspension (or setup degraded earlier); quiet when the
        // effect is still ours. This is also the deferred initial
        // effect when the run started suspended.
        if snap_want {
            snap_resume(dir, me, store);
            *snap_primed = true;
        }
    } else if snap_want && !*snap_primed {
        // Deferred initial effect: the run started active but setup
        // no longer disables. Drive once on the first active tick.
        snap_resume(dir, me, store);
        *snap_primed = true;
    }
    // Focus-follows-cursor motion tap: armed only after the suspend
    // decision above, so a game holds the session before any install can
    // run (startup under a game never installs). The wake itself was
    // consumed before the idle gate; here the tap reconciles installs and
    // retries with backoff. Motion between game start and the next tick
    // still traverses the installed tap (detection interval, unmeasured).
    // Proof paths never arm it (no live settings).
    {
        let ffc_on = state
            .settings_live
            .as_ref()
            .is_some_and(|live| live.settings.core.focus_follows_cursor);
        state.cursor_wake.set_armed(ffc_on, now);
        state.cursor_wake.service(now);
        if let Some(diag) = state.cursor_wake.take_diag() {
            log_json_at(
                log_path,
                serde_json::json!({"event":"cursor-tap","op":diag.op,"ok":diag.ok}),
            );
        }
    }
    // Border follows the live foreground on every active wake,
    // including managed gestures that pause tiling below: the refresh
    // revalidates fresh identity/facts against the last applied reply and
    // only hides on staleness, never waiting for reconciliation.
    refresh_border_timed(state, me, &fulls, owner_work);
    // The group fill follows on the same wake: fresh chord/move trigger
    // over the reply-projected source union, same freshness contract.
    refresh_group_underlay(state, me, store, dir, &fulls, areas);
    // Only gestures on managed windows pause tiling; unrelated
    // windows never stall the loop.
    let ended: Vec<u64> = state
        .gesture_before
        .keys()
        .copied()
        .filter(|hwnd| state.managed.contains(hwnd) && !state.active.contains(hwnd))
        .collect();
    // Drop pre-gesture state for windows that left management.
    state
        .gesture_before
        .retain(|hwnd, _| state.managed.contains(hwnd));
    state
        .gesture_end_cursor
        .retain(|hwnd, _| state.managed.contains(hwnd));
    state
        .gesture_start_key
        .retain(|hwnd, _| state.managed.contains(hwnd));
    state
        .gesture_start_tag
        .retain(|hwnd, _| state.managed.contains(hwnd));
    state
        .gesture_esc_seq
        .retain(|hwnd, _| state.managed.contains(hwnd));
    state
        .gesture_end_seq
        .retain(|hwnd, _| state.managed.contains(hwnd));
    state
        .preview_bound
        .retain(|hwnd, _| state.managed.contains(hwnd));
    state
        .preview_dead
        .retain(|hwnd| state.managed.contains(hwnd));
    state
        .gesture_preview_start
        .retain(|hwnd, _| state.managed.contains(hwnd));
    state
        .gesture_start_cursor
        .retain(|hwnd, _| state.managed.contains(hwnd));
    state
        .gesture_started_sent
        .retain(|hwnd| state.managed.contains(hwnd));
    state
        .gesture_seq
        .retain(|hwnd, _| state.managed.contains(hwnd));
    state.active.retain(|hwnd| state.managed.contains(hwnd));
    // Sequence-bound Esc latch, drained once per pump after event and
    // suspend handling. Still-held gestures compare the live counter
    // against START. Just-ended gestures compare the END-callback
    // snapshot against START instead: a physical Esc that lands after
    // the END but before this drain must never cancel the completed
    // drop, while a fast cancel tap arriving with the END batch still
    // latches. Only edges newer than the START snapshot latch, so
    // pre-gesture taps never cancel. Fast taps arrive via the prompt
    // callback, never via level polling. Injected Esc never sets the
    // native edge (product hook filters injected keys), so synthetic
    // title-bar proof paths keep their no-change restore without
    // claiming an explicit cancel.
    {
        // Product path consumes the process-global edge published
        // directly in hook callbacks (no pump-forwarding delay can
        // hide an Esc landing just before a WinEvent END); the sequence
        // comes from the global atomic clock sampled directly, never
        // a delayed batch. Cancellation still requires a newer
        // sequence than START, so stale flags never cancel.
        let (esc_hit, now_seq) = if state.input_bridge.is_some() {
            (
                crate::input_handoff::global_esc_edge()
                    .swap(false, std::sync::atomic::Ordering::SeqCst),
                crate::snapkey::sys::shared_esc_seq(),
            )
        } else {
            (
                crate::snapkey::sys::take_esc_edge(),
                crate::snapkey::sys::esc_seq(),
            )
        };
        if esc_hit {
            for hwnd in state.active.iter() {
                if !state.managed.contains(hwnd) {
                    continue;
                }
                let start_seq = state.gesture_esc_seq.get(hwnd).copied().unwrap_or(0);
                if esc_edge_cancels(start_seq, now_seq) {
                    state.esc_latched.insert(*hwnd);
                }
            }
            for hwnd in ended.iter() {
                if !state.managed.contains(hwnd) {
                    continue;
                }
                // END-bound: absent snapshots never latch (fail
                // closed to no-cancel; settle classifies from
                // geometry, which a real cancel restored natively).
                let (Some(start_seq), Some(end_seq)) = (
                    state.gesture_esc_seq.get(hwnd).copied(),
                    state.gesture_end_seq.get(hwnd).copied(),
                ) else {
                    continue;
                };
                if esc_edge_cancels(start_seq, end_seq) {
                    state.esc_latched.insert(*hwnd);
                }
            }
        }
    }
    // Logical cancel hides the preview IMMEDIATELY on latch, never
    // waiting for the modal loop to end it.
    for hwnd in state.esc_latched.iter().copied().collect::<Vec<_>>() {
        if state.preview_bound.remove(&hwnd).is_some()
            || render_carrier_visible(state, RenderCarrier::Preview)
        {
            hide_preview(state, "esc-cancelled");
        }
        state.preview_dead.remove(&hwnd);
        state.gesture_preview_start.remove(&hwnd);
        state.gesture_start_cursor.remove(&hwnd);
        state.gesture_started_sent.remove(&hwnd);
    }
    let paused = state.active.iter().any(|hwnd| state.managed.contains(hwnd));
    if paused {
        // A managed gesture holds the loop and invalidates any
        // own-focus chain; keyboard intents of this batch are stale
        // and drop safely.
        state.snap_advance = None;
        if !snap_events.is_empty() {
            keyboard_tick(
                state,
                me,
                store,
                dir,
                &fulls,
                areas,
                snap_events,
                Some("gesture"),
            );
        }
        if !workspace_events.is_empty() {
            workspace_tick(
                state,
                me,
                store,
                dir,
                &fulls,
                areas,
                workspace_events,
                Some("gesture"),
            );
        }
        if !send_events.is_empty() {
            workspace_relative_send_tick(
                state,
                me,
                store,
                dir,
                &fulls,
                areas,
                send_events,
                Some("gesture"),
            );
        }
        if !output_events.is_empty() {
            workspace_output_send_tick(
                state,
                me,
                store,
                dir,
                &fulls,
                areas,
                output_events,
                Some("gesture"),
            );
        }
        if !migrate_events.is_empty() {
            workspace_migrate_tick(
                state,
                me,
                store,
                dir,
                &fulls,
                areas,
                migrate_events,
                Some("gesture"),
            );
        }
        if !history_events.is_empty() {
            workspace_history_tick(
                state,
                me,
                store,
                dir,
                &fulls,
                areas,
                history_events,
                Some("gesture"),
            );
        }
        // Drop-preview track on the armed gesture-sampling deadline while
        // held: fresh observation plus the shared Engine resolver per moved
        // pointer. No focus, no geometry writes.
        refresh_drag_preview(state, me, store, dir, &fulls, areas);
        // Due routine-frame summaries flush on this existing turn so
        // per-move preview aggregates never strand behind the hold.
        {
            let now = Instant::now();
            let lines = state.render_frames.take_due(now);
            for line in lines {
                log_json_at(log_path, line);
            }
        }
        return Ok(TickFlow::Paused);
    }
    if ended.is_empty() {
        if snap_events.is_empty()
            && workspace_events.is_empty()
            && send_events.is_empty()
            && output_events.is_empty()
            && migrate_events.is_empty()
            && history_events.is_empty()
        {
            reconcile_tick(state, me, store, dir, &fulls, areas);
            if state.restore_wake.is_some() {
                let (expired, lost, done) = restore_wake_probe(
                    state,
                    state.restore_wake.as_ref().expect("armed"),
                    Instant::now(),
                );
                let (keep, _) = crate::tiling::restore_wake_step(true, expired, lost, done, true);
                if !keep {
                    state.restore_wake = None;
                }
            }
            workspace_maintenance(state, me, store, dir, &fulls, areas);
            // Quiet-turn maintenance arms the trailing health
            // flush at the end of the turn so pending activity
            // never strands behind idle.
            maintenance_quiet = true;
            poll_workspace_cli_request(state, me, store, dir, &fulls, areas);
            tray_select::poll_tray_select(state, me, store, dir, &fulls, areas, tray);
            poll_pager_select(state, me, store, dir, &fulls, areas);
            poll_resize_cli_request(state, me, store, dir, &fulls, areas);
            poll_directional_cli_request(state, me, store, dir, &fulls, areas);
            poll_send_output_cli_request(state, me, store, dir, &fulls, areas);
            poll_migrate_workspace_cli_request(state, me, store, dir, &fulls, areas);
        } else {
            let obs_before = state.world.current_obs();
            if !snap_events.is_empty() {
                keyboard_tick(state, me, store, dir, &fulls, areas, snap_events, None);
            }
            if !workspace_events.is_empty() {
                workspace_tick(state, me, store, dir, &fulls, areas, workspace_events, None);
            }
            if !send_events.is_empty() {
                workspace_relative_send_tick(
                    state,
                    me,
                    store,
                    dir,
                    &fulls,
                    areas,
                    send_events,
                    None,
                );
            }
            if !output_events.is_empty() {
                workspace_output_send_tick(
                    state,
                    me,
                    store,
                    dir,
                    &fulls,
                    areas,
                    output_events,
                    None,
                );
            }
            if !migrate_events.is_empty() {
                workspace_migrate_tick(state, me, store, dir, &fulls, areas, migrate_events, None);
            }
            if !history_events.is_empty() {
                workspace_history_tick(state, me, store, dir, &fulls, areas, history_events, None);
            }
            poll_workspace_cli_request(state, me, store, dir, &fulls, areas);
            tray_select::poll_tray_select(state, me, store, dir, &fulls, areas, tray);
            poll_pager_select(state, me, store, dir, &fulls, areas);
            poll_directional_cli_request(state, me, store, dir, &fulls, areas);
            poll_send_output_cli_request(state, me, store, dir, &fulls, areas);
            poll_migrate_workspace_cli_request(state, me, store, dir, &fulls, areas);
            // Owe one observation when hook work drained and the lane never
            // entered a world tick (capture runs at tick entry, before
            // dispatch fences). Pending foreground edges owe it too.
            let entered_world_tick = state.world.current_obs() != obs_before;
            if state.world.has_pending_foreground() || (hook_drain_woke && !entered_world_tick) {
                reconcile_tick(state, me, store, dir, &fulls, areas);
            }
        }
    } else {
        // A just-ended managed gesture settles before keyboard
        // continuation resumes.
        state.snap_advance = None;
        if !snap_events.is_empty() {
            keyboard_tick(
                state,
                me,
                store,
                dir,
                &fulls,
                areas,
                snap_events,
                Some("gesture"),
            );
        }
        if !workspace_events.is_empty() {
            workspace_tick(
                state,
                me,
                store,
                dir,
                &fulls,
                areas,
                workspace_events,
                Some("gesture"),
            );
        }
        if !send_events.is_empty() {
            workspace_relative_send_tick(
                state,
                me,
                store,
                dir,
                &fulls,
                areas,
                send_events,
                Some("gesture"),
            );
        }
        if !output_events.is_empty() {
            workspace_output_send_tick(
                state,
                me,
                store,
                dir,
                &fulls,
                areas,
                output_events,
                Some("gesture"),
            );
        }
        if !migrate_events.is_empty() {
            workspace_migrate_tick(
                state,
                me,
                store,
                dir,
                &fulls,
                areas,
                migrate_events,
                Some("gesture"),
            );
        }
        if !history_events.is_empty() {
            workspace_history_tick(
                state,
                me,
                store,
                dir,
                &fulls,
                areas,
                history_events,
                Some("gesture"),
            );
        }
        gesture_tick(state, me, store, dir, &fulls, areas, &ended);
        poll_workspace_cli_request(state, me, store, dir, &fulls, areas);
        tray_select::poll_tray_select(state, me, store, dir, &fulls, areas, tray);
        poll_pager_select(state, me, store, dir, &fulls, areas);
        poll_directional_cli_request(state, me, store, dir, &fulls, areas);
        poll_send_output_cli_request(state, me, store, dir, &fulls, areas);
        poll_migrate_workspace_cli_request(state, me, store, dir, &fulls, areas);
    }
    // Post-mutation border refresh: reconciliation may have moved the
    // target this tick, so revalidate against the fresh reply instead of
    // leaving the pre-reconcile shapes stale until the next wake. Own
    // overlay events are filtered above, so no feedback loop.
    refresh_border_timed(state, me, &fulls, owner_work);
    // Same for the group fill: a moved target may have joined another
    // group, so the reply-projected union re-resolves instead of lingering.
    refresh_group_underlay(state, me, store, dir, &fulls, areas);
    // End-of-turn health commit: records the armed owner sample
    // (pre-log exclusive span, including this turn's refreshes)
    // and flushes due summaries, plus trailing activity on
    // maintenance-quiet turns. The guard then drops disarmed.
    if state.trace {
        owner_work.finish_turn(maintenance_quiet);
    }
    // Routine-frame summaries flush on this existing turn (no new
    // wake): due windows always, trailing activity on
    // maintenance-quiet turns. Runs in both trace and production
    // modes so aggregated geometry churn never strands.
    {
        let now = Instant::now();
        let lines = if maintenance_quiet {
            state.render_frames.take_trailing(now)
        } else {
            state.render_frames.take_due(now)
        };
        for line in lines {
            log_json_at(log_path, line);
        }
    }
    Ok(TickFlow::Done)
}

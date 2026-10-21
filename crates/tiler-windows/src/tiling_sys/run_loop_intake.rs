#![forbid(unsafe_code)]
use super::*;

use super::owner_work::OwnerWorkGuard;
use super::pager_host::sync_taskbar_pager;

// Per-turn input intake for the tile loop: input-pump maintenance, snap hook
// install, live settings, tray sync, snap batch takes with partition
// and settle, plus trace summaries. Returns the partitioned batches with the
// accumulated wake flag; the caller moves them into the active tick.
pub(super) struct Intake {
    pub woke: bool,
    pub snap: Vec<QueuedSnapEvent>,
    pub workspace: Vec<crate::snapkey::QueuedWorkspaceIntent>,
    pub send: Vec<QueuedWorkspaceSendIntent>,
    pub output: Vec<QueuedWorkspaceOutputIntent>,
    pub migrate: Vec<QueuedWorkspaceMigrateIntent>,
    pub history: Vec<QueuedWorkspaceHistoryIntent>,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn pump_intake(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    dir: &Path,
    store: &LedgerStore,
    fulls: &[Rect],
    areas: &[MonitorArea],
    log_path: &Path,
    owner_work: &mut OwnerWorkGuard<'_>,
    now: Instant,
    use_input_thread: bool,
    proof: bool,
    shortcut_proof: bool,
    workspace_proof: bool,
    snap_hook: &mut Option<crate::ffi::SnapKeyHook>,
    snap_failures: &mut u32,
    snap_retry_at: &mut Option<Instant>,
    snap_want: &mut bool,
    tray: &mut Option<crate::tray_sys::TrayOwner>,
    gaming: &mut Option<crate::gaming::GamingMonitor>,
    trace_last_turns: &mut u64,
    trace_last_offers: &mut u64,
    mut woke: bool,
    rediscovered: bool,
) -> Intake {
    // Best-effort keyboard hook with bounded backoff retries (5s
    // doubling, 60s cap): failure logs one `snap-unavailable` per
    // attempt, never per-poll noise, and the loop keeps tiling with
    // nothing consuming. Resume and work-area changes re-arm an
    // immediate retry; there is no permanent disable. The gate
    // follows the live takeover setting so a settings re-enable
    // installs the hook, and a fresh install publishes the live
    // rebound table immediately.
    let takeover_live = (state.keyboard.takeover && !proof) || shortcut_proof || workspace_proof;
    if state.input_bridge.is_some() {
        // Product path: the pump owns install/retry for the keyboard hook.
        // Drain failure events for the owner writer; late readiness
        // completes here if the bounded startup wait expired first.
        // Then retry pending inbox payloads and poll the apply ack.
        if let Some(bridge) = state.input_bridge.as_mut() {
            bridge.handle.drain_failures(&mut |event: &FailEvent| {
                log_json_at(
                    log_path,
                    if event.recovered {
                        serde_json::json!({
                            "event": "snap-available",
                            "producer": "input-pump",
                        })
                    } else {
                        serde_json::json!({
                            "event": "snap-unavailable",
                            "attempt": event.attempt,
                            "retry_secs": event.retry_secs,
                            "producer": "input-pump",
                        })
                    },
                );
            });
            let had = bridge.sender.pending_len() + bridge.sender.safety_len();
            bridge.sender.flush();
            // Retry payload moved into the inbox: wake the pump so a
            // deadline-idle wait applies it promptly. Quiet when
            // nothing was outstanding (no per-tick spurious wake).
            if had > 0 {
                bridge.channels.notify_work();
            }
            log_input_faults(bridge, log_path);
        }
        poll_input_ack(state);
    }
    // Renderer acknowledgements and lifecycle were already drained
    // above (before the ack-only branch), so ack accounting never
    // stalls behind the input gate: a failed input spawn must not
    // stall it, and an ack-only turn must not wait for it.
    if !use_input_thread
        && takeover_live
        && snap_hook.is_none()
        && snap_retry_at.is_none_or(|at| now >= at)
    {
        let installed = if shortcut_proof || workspace_proof {
            crate::snapkey::sys::install_proof(state.keyboard)
        } else {
            crate::snapkey::sys::install(state.keyboard)
        };
        match installed {
            Ok(hook) => {
                *snap_hook = Some(hook);
                // Trace-only collection starts here: the hook records
                // nothing until this setter runs, so default runs pay
                // no timing/filter/push work in the callback.
                crate::snapkey::sys::set_callback_diag_enabled(state.trace);
                // Publish the live policy plus the rebound/suppression
                // tables onto the fresh machine (a fresh install never
                // carries held keys, so nothing drains here).
                let remap = state.last_remap.clone();
                let disabled = state.last_disabled.clone();
                crate::snapkey::sys::apply_live_config(state.keyboard, &remap, &disabled);
                *snap_failures = 0;
                *snap_retry_at = None;
                log_json_at(log_path, serde_json::json!({"event":"snap-available"}));
            }
            Err(message) => {
                *snap_failures += 1;
                let backoff = (5u64 << (*snap_failures).min(4)).min(60);
                *snap_retry_at = Some(now + Duration::from_secs(backoff));
                log_json_at(
                    log_path,
                    serde_json::json!({
                        "event": "snap-unavailable",
                        "attempt": *snap_failures,
                        "retry_secs": backoff,
                        "cause": message,
                    }),
                );
            }
        }
    }
    // Live settings poll (normal owner only; proof owners never
    // poll): validated edits apply BEFORE the hook-queue drain, so
    // `apply_live_config` drains stale intents before this pump
    // pulls them and nothing dispatches under the old config.
    // Invalid files keep last-good. A changed apply wakes the pump
    // so gap adoption reconciles promptly.
    if poll_live_settings(state, dir, me, store, snap_want) {
        woke = true;
    }
    // Cached gaming facts (normal owner only; proof owners carry no
    // monitor): a changed settings revision re-arms detection, otherwise an
    // infrequent bounded interval covers Settings-made changes. The worker
    // holds at most one pass and is never waited on here.
    if let Some(monitor) = gaming.as_mut() {
        let revision = state
            .settings_live
            .as_ref()
            .map(|live| live.settings.revision);
        monitor.maybe_refresh(revision);
        // Owner-pump drain of completed worker lines (single-writer log:
        // the worker queues, only this pump writes).
        for line in monitor.drain_logs() {
            log_json_at(log_path, line);
        }
    }
    // Tray follows live settings on the same pump: badge and tooltip
    // track the validated effective bindings under the runtime
    // keyboard policy (takeover plus the runtime Win+L opt-in), and a
    // pending add retries through the state gate (no duplicate
    // icons). `sync` itself is change-gated.
    if let Some(tray) = tray.as_mut() {
        let snapshot = gaming
            .as_ref()
            .map(|monitor| monitor.snapshot())
            .unwrap_or_else(crate::gaming::GamingSnapshot::unknown);
        let (conflict_empty, settings_status) = match state.settings_live.as_ref() {
            Some(live) => (
                crate::gaming::tray_warning_conflicts(
                    &live.settings,
                    state.keyboard.takeover,
                    state.keyboard.allow_win_l,
                    &snapshot,
                )
                .is_empty(),
                live.status.clone(),
            ),
            None => (true, "saved:unknown".to_owned()),
        };
        // Focused-output pager: genuine focus (foreground, pointer, cache,
        // ordering) supplies the listed output, its workspace order, the
        // active id, and the 1-based tray number. The number follows
        // workspace switches and output focus changes; the toggle scope
        // below keeps its cached-output semantics untouched.
        let owner = tray.hwnd_value();
        let focused = tray_select::tray_focused_output(state, areas, owner);
        let (pager_ids, pager_active) = match focused.as_deref() {
            Some(output) => (state.workspace_ids(output), state.active_view(output)),
            None => (Vec::new(), None),
        };
        let ordinal = focused.as_deref().and_then(|output| {
            let ring = state.workspace_ids(output);
            let active = state.active_view(output);
            crate::tray_pager::active_ordinal_keys(&ring, active.as_deref())
        });
        tray.sync(conflict_empty, &settings_status, ordinal);
        tray.sync_pager(focused, pager_ids, pager_active);
        // A waiting pager pick wakes the pump; the tick dispatch site
        // drains it once (a gesture-held turn leaves it queued there).
        if tray.has_select_request() {
            woke = true;
        }
        // Truthful workspace section: the opaque live scope plus its
        // tiled state and the persisted default. The next menu renders
        // exactly this; a toggle pick drains once below and flips the
        // mode on the loop thread only when the live active pair still
        // matches the rendered scope (stale/mismatched refuses with no
        // guessed fallback).
        let scope = if state.active_output.is_empty() {
            None
        } else {
            state
                .active_view(&state.active_output.clone())
                .map(|workspace| (state.active_output.clone(), workspace))
        };
        let current = scope.as_ref().and_then(|(output, workspace)| {
            state
                .workspace_ids(output)
                .iter()
                .any(|id| id == workspace)
                .then(|| state.is_tiled_view(output, workspace))
        });
        tray.sync_workspace(scope, current, state.default_tiled_seed());
        if let Some(request) = tray.take_toggle_request() {
            let live = if state.active_output.is_empty() {
                None
            } else {
                state
                    .active_view(&state.active_output.clone())
                    .map(|workspace| (state.active_output.clone(), workspace))
            };
            let rendered = Some((request.output.as_str(), request.workspace.as_str()));
            let live_ref = live
                .as_ref()
                .map(|(output, workspace)| (output.as_str(), workspace.as_str()));
            if crate::tray::verify_toggle_scope(rendered, live_ref) {
                let toggle_fulls = monitor_fulls(areas);
                toggle_workspace_tiling(
                    state,
                    me,
                    store,
                    dir,
                    &toggle_fulls,
                    areas,
                    &request.output,
                );
                woke = true;
            } else {
                // Stale or mismatched menu scope (output switch or a
                // pruned workspace between render and pick): refuse
                // with opaque tokens only, never raw scope ids.
                log_json_at(
                    log_path,
                    serde_json::json!({
                        "event": "workspace-mode",
                        "op": "toggle",
                        "outcome": "refused-stale-scope",
                    }),
                );
            }
        }
    }
    // Taskbar pager (normal owner only; proof owners stay deterministic
    // with no overlays): per-output strips sync on this same pump, so frames
    // stay live on the event-driven cadence with change-gated repaints.
    // Secondary anchors revalidate only on the stale-anchor flag (scoped
    // Explorer edge, rediscovery broadcast, or topology change). A waiting
    // click wakes the pump; the tick dispatch drains it once (a gesture-held
    // turn leaves it queued there).
    if !proof && sync_taskbar_pager(state, me, fulls, areas, log_path, now, rediscovered) {
        woke = true;
    }
    // Shortcut-handling gate for the callback: fresh downs consume
    // iff takeover holds with this gate active (the eventual Xbox
    // pause will clear it; until then publication stays takeover
    // only). Fullscreen suspension, gesture, elevated foreground,
    // and unmanaged/unadmitted/shell foreground never enter this
    // gate, so inactive tiling never pauses interception: those
    // chords still swallow at the hook and the owner rechecks fresh
    // identity/scope/elevation/fullscreen/gesture per intent before
    // any action, never manipulating protected windows. Hook-side
    // work stays cheap reads, never expensive syscalls per key.
    let snap_gate_active = state.keyboard.takeover;
    let snap_events = if state.input_bridge.is_some() {
        // Product path: offer the whole ordered payload when content
        // changed (no redundant snapshots, no queue flushes) and take
        // the single-outstanding tagged batch. Batches tagged older
        // than the published policy are stale-dropped through the
        // config-change path (masks still resolve) instead of
        // dispatching under a replaced policy; held pairs ride
        // stored verdicts either way.
        let candidate = crate::input_handoff::InputSnapshot {
            generation: 0,
            keyboard: state.keyboard,
            remap: state.last_remap.clone(),
            disabled: state.last_disabled.clone(),
            snap_origins: state.snap_origins.clone(),
            active: snap_gate_active,
            trace: state.trace,
        };
        offer_input_snapshot(state, candidate);
        let taken: Option<crate::input_handoff::TaggedBatch<QueuedSnapEvent>> =
            state.input_bridge.as_ref().and_then(|bridge| {
                let taken = bridge.channels.snap_outbox.take();
                // Freed outbox space: wake the pump so parked
                // pending batches and retained queue terminals retry
                // promptly instead of stranding behind an idle wait.
                if taken.is_some() {
                    bridge.channels.notify_work();
                }
                taken
            });
        let mut batch = Vec::new();
        // Fresh take-time age for the health window (trace-only);
        // `None` unless a batch is taken below, so stale ages are
        // never recorded.
        let mut snap_age_us: Option<u64> = None;
        if let Some(tagged) = taken {
            if tagged.is_empty() {
                // Keep-alive empty: nothing to dispatch.
            } else {
                woke = true;
                let published = state
                    .input_bridge
                    .as_ref()
                    .map(|bridge| bridge.published_gen)
                    .unwrap_or(0);
                if tagged.is_stale(published) {
                    keyboard_tick(
                        state,
                        me,
                        store,
                        dir,
                        fulls,
                        areas,
                        tagged.items,
                        Some("config-change"),
                    );
                } else {
                    if state.trace {
                        snap_age_us = Some(crate::input_pump::snap_batch_age_us(
                            &tagged.items,
                            Instant::now(),
                        ));
                    }
                    batch = tagged.items;
                }
            }
        }
        // Trace-only input health from the bounded diag slot: drop
        // transitions keep their explicit `snap-drop` line, while
        // turn samples aggregate into the window (a taken batch, a
        // fresh drop, or any fresh pump turn: ordinary pump turns
        // carry dispatch/gap/pending evidence even with no batch).
        // Snap age is `Some` only for a freshly taken batch;
        // pump age is `Some` only for a fresh offer (the slot
        // value persists from the last offer); heartbeat/pending
        // are last-observed values. The heartbeat proves liveness;
        // native timestamp vs delivery rides the summary-only
        // `delivery_gap_ms` aggregate, tick-count domain.
        if state.trace
            && let Some(bridge) = state.input_bridge.as_mut()
        {
            let diag = bridge.channels.diag.snapshot();
            let drops_changed = diag.snap_dropped != state.snap_dropped;
            if drops_changed {
                let lost = diag.snap_dropped.saturating_sub(state.snap_dropped);
                state.snap_dropped = diag.snap_dropped;
                log_json_at(
                    log_path,
                    serde_json::json!({"event":"snap-drop","lost": lost}),
                );
            }
            let turns_changed = diag.turns != *trace_last_turns;
            let offers_changed = diag.offers != *trace_last_offers;
            if !batch.is_empty() || drops_changed || turns_changed {
                *trace_last_turns = diag.turns;
                if offers_changed {
                    *trace_last_offers = diag.offers;
                }
                let pump_age_us = if offers_changed {
                    Some(diag.queue_age_us)
                } else {
                    None
                };
                owner_work.observe_input(&diag, snap_age_us, pump_age_us);
            }
        }
        batch
    } else if snap_hook.is_some() {
        crate::snapkey::sys::publish_gate(&state.snap_origins, snap_gate_active);
        let batch = crate::snapkey::sys::drain_up_to(MAX_DISPATCH_PER_TICK);
        if !batch.is_empty() {
            woke = true;
        }
        batch
    } else {
        Vec::new()
    };
    // Workspace digits, relative sends, explicit output sends and
    // history ride the same hook/queue/mask authority but dispatch
    // through the workspace owner, never the directional Engine
    // route. Partition here so each batch keeps its verdict
    // vocabulary; `shortcut-proof` keeps workspace hides disabled
    // inside `workspace_tick` via the proof gate.
    let mut workspace_events = Vec::new();
    let mut send_events = Vec::new();
    let mut output_events = Vec::new();
    let mut migrate_events: Vec<QueuedWorkspaceMigrateIntent> = Vec::new();
    let mut history_events = Vec::new();
    let mut directional_events = Vec::new();
    for event in snap_events {
        match event {
            QueuedSnapEvent::Workspace(intent) => workspace_events.push(intent),
            QueuedSnapEvent::WorkspaceSend(intent) => send_events.push(intent),
            QueuedSnapEvent::WorkspaceOutput(intent) => output_events.push(intent),
            QueuedSnapEvent::WorkspaceMigrate(intent) => migrate_events.push(intent),
            QueuedSnapEvent::WorkspaceHistory(intent) => history_events.push(intent),
            QueuedSnapEvent::Mask(_)
            | QueuedSnapEvent::Intent(_)
            | QueuedSnapEvent::Resize(_)
            | QueuedSnapEvent::Maximize(_)
            | QueuedSnapEvent::Orientation(_)
            | QueuedSnapEvent::WorkspaceTiling(_)
            | QueuedSnapEvent::Fullscreen(_)
            | QueuedSnapEvent::Float(_)
            | QueuedSnapEvent::Launch(_)
            | QueuedSnapEvent::Sticky(_) => {
                directional_events.push(event);
            }
        }
    }
    if !workspace_events.is_empty()
        || !send_events.is_empty()
        || !output_events.is_empty()
        || !migrate_events.is_empty()
        || !history_events.is_empty()
    {
        woke = true;
    }
    let snap_events = directional_events;
    // Proof-only callback diagnostics for shortcut-proof: accepted
    // marked events as the callback saw them, drained to the proof
    // audit (never production logs) so live runs can distinguish
    // actual modifier delivery from classifier state. Product `tile`
    // never records or writes these.
    if (shortcut_proof || workspace_proof) && snap_hook.is_some() {
        let diag = crate::snapkey::sys::drain_marked_diag();
        if !diag.is_empty()
            && let Some(audit) = state.audit_path.as_ref()
        {
            let entries: Vec<serde_json::Value> = diag
                .iter()
                .map(crate::snapkey::marked_diag_evidence)
                .collect();
            log_json_at(
                audit,
                serde_json::json!({
                    "event": "proof-keys",
                    "entries": entries,
                    "dropped": crate::snapkey::sys::marked_diag_dropped(),
                }),
            );
        }
        // Proof-only complement: non-marked modifier traffic
        // (injected-filtered plus physical, modifiers only) drained
        // to the proof audit under its own event, so the actual
        // callback modifier sequence is complete. Product `tile`
        // never records or writes these.
        let mods = crate::snapkey::sys::drain_mod_diag();
        if !mods.is_empty()
            && let Some(audit) = state.audit_path.as_ref()
        {
            let entries: Vec<serde_json::Value> =
                mods.iter().map(crate::snapkey::mod_diag_evidence).collect();
            log_json_at(
                audit,
                serde_json::json!({
                    "event": "proof-mods",
                    "entries": entries,
                    "dropped": crate::snapkey::sys::mod_diag_dropped(),
                }),
            );
        }
    }
    // Trace-only callback summary for product and proof paths: closed
    // vocabulary plus verdict/source/state/local-timing only (no
    // HWNDs, PIDs, tokens, titles). One bounded line per tick at
    // most; losses stay visible via dropped/filtered even when the
    // batch is empty. Mask `SendInput` latency rides the separate
    // `mask_sends`/`mask_send_max_us` aggregate (never inside
    // `duration_us`, never the downstream hook chain). Product entries
    // ride the pump `cb_outbox`; counts ride the diag slot. Neither
    // measures native delivery latency.
    let trace_has_pump = state.input_bridge.is_some();
    if state.trace && (snap_hook.is_some() || trace_has_pump) {
        let (diags, dropped, filtered, mask_sends, mask_send_max_us, cb_outbox_dropped) =
            if state.input_bridge.is_some() {
                match state.input_bridge.as_ref() {
                    Some(bridge) => {
                        let diag = bridge.channels.diag.snapshot();
                        let batch = bridge.channels.cb_outbox.take();
                        // Freed trace slot: wake the pump so a parked
                        // callback summary retries promptly.
                        if batch.is_some() {
                            bridge.channels.notify_work();
                        }
                        let batch = batch.unwrap_or_default();
                        (
                            batch,
                            diag.cb_dropped,
                            diag.cb_filtered,
                            diag.mask_sends,
                            diag.mask_send_max_us,
                            diag.cb_outbox_dropped,
                        )
                    }
                    None => (Vec::new(), 0, 0, 0, 0, 0),
                }
            } else {
                (
                    crate::snapkey::sys::drain_callback_diag(),
                    crate::snapkey::sys::callback_diag_dropped(),
                    crate::snapkey::sys::callback_diag_filtered(),
                    crate::snapkey::sys::mask_send_stats().0,
                    crate::snapkey::sys::mask_send_stats().1,
                    0,
                )
            };
        if !diags.is_empty()
            || dropped != state.cb_diag_dropped
            || filtered != state.cb_diag_filtered
            || mask_sends != state.mask_sends
            || mask_send_max_us != state.mask_send_max_us
            || cb_outbox_dropped != state.cb_outbox_dropped
        {
            state.cb_diag_dropped = dropped;
            state.cb_diag_filtered = filtered;
            state.mask_sends = mask_sends;
            state.mask_send_max_us = mask_send_max_us;
            state.cb_outbox_dropped = cb_outbox_dropped;
            let max_duration_us = diags.iter().map(|d| d.duration_us).max().unwrap_or(0);
            let entries: Vec<serde_json::Value> = diags
                .iter()
                .map(crate::snapkey::callback_diag_evidence)
                .collect();
            log_json_at(
                log_path,
                serde_json::json!({
                    "event": "snap-callback",
                    "entries": entries,
                    "dropped": dropped,
                    "filtered": filtered,
                    "cb_outbox_dropped": cb_outbox_dropped,
                    "max_duration_us": max_duration_us,
                    "mask_sends": mask_sends,
                    "mask_send_max_us": mask_send_max_us,
                }),
            );
        }
    }
    Intake {
        woke,
        snap: snap_events,
        workspace: workspace_events,
        send: send_events,
        output: output_events,
        migrate: migrate_events,
        history: history_events,
    }
}

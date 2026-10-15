use super::*;

use super::owner_work::OwnerWorkGuard;
use windows_sys::Win32::UI::WindowsAndMessaging::HHOOK;

// Per-turn input intake for the tile loop: input-pump maintenance, snap hook
// install, live settings, tray sync, snap/windrag batch takes with partition
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
    snap_hook: &mut Option<HHOOK>,
    snap_failures: &mut u32,
    snap_retry_at: &mut Option<Instant>,
    windrag_available: bool,
    snap_want: &mut bool,
    tray: &mut Option<crate::tray_sys::TrayOwner>,
    trace_last_turns: &mut u64,
    trace_last_offers: &mut u64,
    trace_windrag_offers: &mut u64,
    mut woke: bool,
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
        // Product path: the pump owns install/retry for both hooks.
        // Drain failure events for the owner writer; late readiness
        // completes here if the bounded startup wait expired first.
        // Then retry pending inbox payloads and poll the apply ack.
        if let Some(bridge) = state.input_bridge.as_mut() {
            bridge.handle.drain_failures(&mut |event: &FailEvent| {
                log_json_at(
                    log_path,
                    if event.recovered {
                        serde_json::json!({
                            "event": if event.hook == "keyboard" { "snap-available" } else { "windrag-available" },
                            "producer": "input-pump",
                        })
                    } else {
                        serde_json::json!({
                            "event": if event.hook == "keyboard" { "snap-unavailable" } else { "windrag-unavailable" },
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
    // Tray follows live settings on the same pump: badge and tooltip
    // track the validated effective bindings under the runtime
    // keyboard policy (takeover plus the runtime Win+L opt-in), and a
    // pending add retries through the state gate (no duplicate
    // icons). `sync` itself is change-gated.
    if let Some(tray) = tray.as_mut() {
        let (conflict_empty, settings_status) = match state.settings_live.as_ref() {
            Some(live) => (
                crate::tray::unresolved_conflicts(
                    &live.settings,
                    state.keyboard.takeover,
                    state.keyboard.allow_win_l,
                )
                .is_empty(),
                live.status.clone(),
            ),
            None => (true, "saved:unknown".to_owned()),
        };
        tray.sync(conflict_empty, &settings_status);
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
                .workspaces
                .active_id(&state.active_output)
                .map(|workspace| (state.active_output.clone(), workspace))
        };
        let current = scope.as_ref().and_then(|(output, workspace)| {
            state
                .workspaces
                .workspace_ids(output)
                .iter()
                .any(|id| id == workspace)
                .then(|| state.workspaces.is_tiled(output, workspace))
        });
        tray.sync_workspace(scope, current, state.workspaces.default_tiled());
        if let Some(request) = tray.take_toggle_request() {
            let live = if state.active_output.is_empty() {
                None
            } else {
                state
                    .workspaces
                    .active_id(&state.active_output)
                    .map(|workspace| (state.active_output.clone(), workspace))
            };
            let rendered = Some((request.output.as_str(), request.workspace.as_str()));
            let live_ref = live
                .as_ref()
                .map(|(output, workspace)| (output.as_str(), workspace.as_str()));
            if crate::tray::verify_toggle_scope(rendered, live_ref) {
                let toggle_fulls = monitor_fulls(areas);
                toggle_workspace_tiling(state, me, &toggle_fulls, areas, &request.output);
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
            windrag_origins: state.windrag_origins.clone(),
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
                    keyboard_tick(state, me, fulls, areas, tagged.items, Some("config-change"));
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
    // Project Win+Left edges ride the same pump and gate: the hook
    // consumes downs/ups only while takeover holds, and every bound
    // candidate revalidates against the last complete observation in
    // `windrag_down` before anything arms. Downs pause tiling through
    // the shared `active` set (frames stay at source: no mid-gesture
    // writes); ups feed the shared `ended` settle below.
    //
    // Product path: edges arrive tagged from the input pump outbox.
    // Drop loss clears through ordered inbox invalidations so hook
    // disarm and owner state stay in FIFO order with snapshots.
    // Batches tagged under a superseded policy are stale: Downs are
    // refused without arming (a newer policy is live), while Ups and
    // Cancels still flow (owner-side closes are no-ops without open
    // gestures, preserving pair hygiene).
    let windrag_edges: Vec<WinDragEdge> = if state.input_bridge.is_some() {
        let taken: Option<crate::input_handoff::TaggedBatch<WinDragEdge>> =
            state.input_bridge.as_ref().and_then(|bridge| {
                let taken = bridge.channels.windrag_outbox.take();
                // Freed outbox space: wake the pump so retained
                // terminal Up/Cancel edges retry promptly instead of
                // stranding behind an idle wait.
                if taken.is_some() {
                    bridge.channels.notify_work();
                }
                taken
            });
        let mut edges = Vec::new();
        if let Some(tagged) = taken
            && !tagged.is_empty()
        {
            woke = true;
            let published = state
                .input_bridge
                .as_ref()
                .map(|bridge| bridge.published_gen)
                .unwrap_or(0);
            if tagged.is_stale(published) {
                let mut stale_downs = 0u32;
                for edge in tagged.items {
                    match edge.kind {
                        WinDragKind::Down => stale_downs += 1,
                        WinDragKind::Up | WinDragKind::Cancel => {
                            edges.push(edge);
                        }
                    }
                }
                if stale_downs > 0 {
                    log_json_at(
                        log_path,
                        serde_json::json!({
                            "event": "windrag-stale",
                            "cause": "config-change",
                            "dropped": stale_downs,
                        }),
                    );
                }
            } else {
                edges = tagged.items;
            }
        }
        // Trace-only windrag transport into the health window
        // (change-only offers; hook stats and drop lines below
        // stay explicit). The transport covers pump offer to
        // owner take only, never queue wait or native delivery.
        if state.trace {
            let (offers, last_offer) = state
                .input_bridge
                .as_ref()
                .map(|bridge| {
                    let diag = bridge.channels.diag.snapshot();
                    (diag.windrag_offers, diag.windrag_last_offer)
                })
                .unwrap_or((0, None));
            if offers != *trace_windrag_offers {
                *trace_windrag_offers = offers;
                if let Some(offered) = last_offer {
                    owner_work.observe_windrag(offered);
                }
            }
        }
        if let Some(bridge) = state.input_bridge.as_ref() {
            let diag = bridge.channels.diag.snapshot();
            let stats = diag.hook_stats;
            if stats != state.windrag_stats_logged {
                state.windrag_stats_logged = stats;
                log_json_at(
                    log_path,
                    serde_json::json!({
                        "event": "windrag-hook-stats",
                        "tick": state.tick,
                        "downs_seen": stats.0,
                        "downs_consumed": stats.1,
                        "ups_seen": stats.2,
                        "ups_consumed": stats.3,
                    }),
                );
            }
            if diag.windrag_dropped > state.windrag_dropped {
                let lost = diag.windrag_dropped - state.windrag_dropped;
                state.windrag_dropped = diag.windrag_dropped;
                log_json_at(
                    log_path,
                    serde_json::json!({"event":"windrag-drop","lost": lost}),
                );
                if !state.windrag_bound.is_empty() {
                    let open: Vec<u64> = state.windrag_bound.keys().copied().collect();
                    for hwnd in open {
                        clear_windrag_gesture(state, hwnd);
                        if !offer_input_control(state, InboxItem::InvalidateWindrag(hwnd)) {
                            log_json_at(
                                log_path,
                                serde_json::json!({
                                    "event": "windrag-drop-loss-unreported",
                                    "note": "invalidation held for retry; paired Up stays swallowed",
                                }),
                            );
                        }
                    }
                    log_json_at(
                        log_path,
                        serde_json::json!({"event":"windrag-drop-loss","cleared": true}),
                    );
                    let fulls_owned = fulls.to_vec();
                    reconcile_tick(state, me, &fulls_owned, areas);
                }
            }
        }
        edges
    } else if windrag_available {
        crate::win_mouse::sys::publish(&state.windrag_origins, snap_gate_active);
        let stats = crate::win_mouse::sys::hook_stats();
        if stats != state.windrag_stats_logged {
            state.windrag_stats_logged = stats;
            log_json_at(
                log_path,
                serde_json::json!({
                    "event": "windrag-hook-stats",
                    "tick": state.tick,
                    "downs_seen": stats.0,
                    "downs_consumed": stats.1,
                    "ups_seen": stats.2,
                    "ups_consumed": stats.3,
                }),
            );
        }
        let edges = crate::win_mouse::sys::drain_up_to(WINDRAG_MAX_DISPATCH_PER_TICK);
        if !edges.is_empty() {
            woke = true;
        }
        edges
    } else {
        Vec::new()
    };
    // Legacy drop-loss path (same-thread hooks only): the product
    // pump reports loss above through the diag slot and ordered
    // inbox invalidations, so this block stays proof/legacy only.
    if !use_input_thread && windrag_available {
        let dropped = crate::win_mouse::sys::queue_dropped();
        if dropped > state.windrag_dropped {
            let lost = dropped - state.windrag_dropped;
            state.windrag_dropped = dropped;
            log_json_at(
                log_path,
                serde_json::json!({"event":"windrag-drop","lost": lost}),
            );
            // A lost edge may have carried a Down whose gesture is
            // open or an Up that would have closed one: discard every
            // affected open project gesture rather than risk a
            // stranded hold or a journey without its close. The hook
            // is disarmed for them; their paired Ups stay swallowed.
            if !state.windrag_bound.is_empty() {
                let open: Vec<u64> = state.windrag_bound.keys().copied().collect();
                for hwnd in open {
                    clear_windrag_gesture(state, hwnd);
                    crate::win_mouse::sys::invalidate(hwnd);
                }
                log_json_at(
                    log_path,
                    serde_json::json!({"event":"windrag-drop-loss","cleared": true}),
                );
                let fulls_owned = fulls.to_vec();
                reconcile_tick(state, me, &fulls_owned, areas);
            }
        }
    }
    // Settle the drained edges (both paths): press-focus observation
    // plus the shared windrag_down/up/cancel vocabulary. Only runs
    // when edges wait; legacy and pump batches share this code.
    if !windrag_edges.is_empty() {
        // Press-focus observation for drained Downs: one fresh complete
        // enumeration per pump (only when a Down waits) feeds the
        // existing focus authority inside `windrag_down`. Token
        // minting is idempotent with the later phase observations;
        // an enumeration failure leaves empty slices so Downs refuse
        // closed with no arm and no writes.
        let mut press_observed: Vec<ObservedWindow> = Vec::new();
        let mut press_retained: Vec<RetainedRow> = Vec::new();
        if windrag_edges.iter().any(|e| e.kind == WinDragKind::Down) {
            let mut skipped: Vec<(String, String)> = Vec::new();
            if let Some(fresh) = state.observe(me, fulls, &mut skipped, &mut press_retained) {
                press_observed = fresh;
            }
        }
        for edge in windrag_edges {
            match edge.kind {
                WinDragKind::Down => {
                    match windrag_down(state, me, &edge, fulls, &press_observed, &press_retained) {
                        Ok(token) => {
                            log_json_at(
                                log_path,
                                serde_json::json!({
                                    "event": "windrag-down",
                                    "tick": state.tick,
                                    "window": token,
                                    "producer": "windrag",
                                }),
                            );
                        }
                        Err(reason) => {
                            log_json_at(
                                log_path,
                                serde_json::json!({
                                    "event": "windrag-down-refused",
                                    "tick": state.tick,
                                    "reason": reason,
                                    "producer": "windrag",
                                }),
                            );
                        }
                    }
                }
                WinDragKind::Up => {
                    windrag_up(state, edge.hwnd, edge.x, edge.y, edge.esc_seq);
                }
                WinDragKind::Cancel => {
                    // Hook-side disarm after a consumed Down: drop
                    // the owner gesture with no plan and no geometry
                    // change. The paired Up stays swallowed
                    // hook-side; native gestures are untouched.
                    if let Some(token) = clear_windrag_gesture(state, edge.hwnd) {
                        log_json_at(
                            log_path,
                            serde_json::json!({
                                "event": "windrag-cancel",
                                "tick": state.tick,
                                "outcome": "gesture-no-change",
                                "window": token,
                                "producer": "windrag",
                            }),
                        );
                    }
                }
            }
        }
    }
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

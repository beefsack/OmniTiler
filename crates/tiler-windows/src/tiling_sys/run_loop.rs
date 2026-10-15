use super::*;

pub(super) fn run_tile_loop(
    dir: &Path,
    me: &ProcessIdentity,
    store: &LedgerStore,
    run: TileRun,
) -> Result<()> {
    let TileRun {
        seconds,
        trace,
        allowlist,
        proof,
        shortcut_proof,
        workspace_proof,
        raw_argv,
        mut keyboard,
        mut mouse_snap,
        mut first_run,
        first_run_pending,
        mut inner_gap,
        mut outer_gap,
        mut settings_dir,
        mut settings_live,
        cli_overrides,
        mut scope,
        mut scope_hosts,
        mut border,
        mut underlay,
    } = run;
    // Prevention needs an active loop: proof never arms it, and the guarded
    // setup already captured the preimage plus the initial effect. The live
    // settings poll may flip this per pump (normal owner only).
    let mut snap_want = mouse_snap && (!proof || shortcut_proof || workspace_proof);
    ensure_pm_v2()?;
    let owner = OwnerId::parse(OWNER_ID).expect("static owner token is valid");
    let generation =
        GenerationId::parse(&me.process_creation).ok_or_else(|| err("error: bad generation"))?;
    let log_path = crate::lifecycle::sys::log_path_for(dir, &me.process_creation);
    let audit_path: Option<std::path::PathBuf> = if proof {
        Some(dir.join(format!("proof-audit-{}.jsonl", me.process_creation)))
    } else {
        None
    };
    // First-run preset under the owner lease (normal owner only): the lease
    // refusal already stopped a second owner before any UI, and the resolved
    // choice reapplies every explicit CLI lane before effects or hooks.
    // Proof paths carry no pending state and keep deterministic config.
    if !proof && let Some(pending) = first_run_pending {
        let resolved = resolve_first_run(dir, me, pending, &cli_overrides, &log_path)?;
        keyboard = KeyboardConfig {
            takeover: !resolved.options.no_keyboard_snap_takeover,
            allow_win_l: resolved.options.allow_win_l,
        };
        mouse_snap = !resolved.options.no_mouse_snap_prevention;
        snap_want = mouse_snap;
        first_run = resolved.first_run;
        inner_gap = resolved.options.inner_gap;
        outer_gap = resolved.options.outer_gap;
        settings_dir = resolved.settings_dir;
        settings_live = resolved.live;
        scope = resolved.options.scope_exes.clone();
        scope_hosts = resolved.options.scope_hosts.clone();
        border = resolved.options.border;
        underlay = resolved.options.underlay;
    }
    // Proof audit starts before any geometry: raw received argv (elements
    // stay separate strings so spaces survive without quoting), parsed
    // seconds/trace/mode, and allowlist digest/count. The frozen identity
    // record follows verification below. Raw argv carries flags and paths
    // only (no titles or window content) and never enters production logs.
    if let Some(audit) = audit_path.as_ref() {
        if audit.exists() {
            return Err(err("refuse: audit preexists, will not overwrite"));
        }
        let digest = allowlist
            .as_ref()
            .map(|entries| allowlist_digest(entries))
            .unwrap_or_default();
        let count = allowlist.as_ref().map(|e| e.len()).unwrap_or(0);
        let line = serde_json::json!({
            "event": "proof-start",
            "argv": raw_argv,
            "seconds": seconds,
            "trace": trace,
            "mode": if workspace_proof {
                "workspace-proof"
            } else if shortcut_proof {
                "shortcut-proof"
            } else {
                "proof"
            },
            "allowlist_digest": digest,
            "allowlist_count": count,
        });
        log_json_at(audit, line);
    }
    // Proof gate before first geometry: every frozen entry must verify as an
    // owned helper (sibling exe/class/tag plus full identity). Invisible
    // passive members verify here and stay frozen until `show`.
    if proof {
        let entries = allowlist
            .as_ref()
            .ok_or_else(|| err("refuse: allowlist missing"))?;
        if entries.is_empty() {
            return Err(err("refuse: empty allowlist"));
        }
        for (index, entry) in entries.iter().enumerate() {
            verify_proof_owned(entry.hwnd, entry, me).map_err(|reason| {
                err(format!(
                    "refuse: proof allowlist non-owned index={index} {reason}"
                ))
            })?;
        }
        if let Some(audit) = audit_path.as_ref() {
            let frozen: Vec<serde_json::Value> = entries
                .iter()
                .map(|e| {
                    serde_json::json!({
                        "hwnd": e.hwnd,
                        "pid": e.pid,
                        "process_creation": e.process_creation,
                        "exe_path": e.exe_path,
                        "user_sid": e.user_sid,
                        "session_id": e.session_id,
                        "tag": e.tag,
                    })
                })
                .collect();
            log_json_at(
                audit,
                serde_json::json!({"event": "proof-frozen", "windows": frozen}),
            );
        }
    }
    let last_remap = settings_live
        .as_ref()
        .map(|live| crate::settings::build_remap(&live.settings))
        .unwrap_or_default();
    let last_disabled = settings_live
        .as_ref()
        .map(|live| crate::settings::build_disabled(&live.settings))
        .unwrap_or_default();
    let mut state = TileLoop {
        engine: Engine::new(),
        owner: owner.clone(),
        generation: generation.clone(),
        tokens: TokenMap::default(),
        refused: RefusedTracker::default(),
        stable: HashMap::new(),
        managed: HashSet::new(),
        active: HashSet::new(),
        gesture_before: HashMap::new(),
        gesture_end_cursor: HashMap::new(),
        gesture_start_key: HashMap::new(),
        gesture_start_tag: HashMap::new(),
        tick: 0,
        allowlist,
        scope,
        scope_hosts,
        trace,
        suspended: false,
        last_summary: None,
        log_path: log_path.clone(),
        audit_path: audit_path.clone(),
        keyboard,
        inner_gap,
        outer_gap,
        settings_live,
        settings_dir,
        last_remap,
        last_disabled,
        cli_overrides,
        snap_dropped: 0,
        cb_diag_dropped: 0,
        cb_diag_filtered: 0,
        cb_outbox_dropped: 0,
        mask_sends: 0,
        mask_send_max_us: 0,
        snap_origins: HashMap::new(),
        windrag_origins: HashMap::new(),
        gesture_producer: HashMap::new(),
        windrag_start_cursor: HashMap::new(),
        windrag_bound: HashMap::new(),
        windrag_dropped: 0,
        windrag_origin_logged: None,
        windrag_stats_logged: (0, 0, 0, 0),
        snap_advance: None,
        resize_repeat: None,
        last_enumerated: 0,
        workspaces: ManagedWorkspaces::new(),
        scope_owner: crate::workspace_scope_owner::WorkspaceScopeOwner::new(),
        member_tokens: std::collections::BTreeMap::new(),
        member_rects: HashMap::new(),
        hidden_claims: std::collections::BTreeMap::new(),
        member_identity: std::collections::BTreeMap::new(),
        member_tags: std::collections::BTreeMap::new(),
        active_output: String::new(),
        workspace_proof: false,
        maximize_admission_attempted: HashSet::new(),
        born_keep_overlay: BTreeSet::new(),
        retile_keep_overlay: BTreeSet::new(),
        born_fullscreen: BTreeSet::new(),
        seen_nonfullscreen: BTreeSet::new(),
        last_foreground: 0,
        known_outputs: Vec::new(),
        last_hwnds: HashSet::new(),
        minimized_omitted: HashSet::new(),
        pending_closed: std::collections::BTreeMap::new(),
        last_areas: Vec::new(),
        hint_logged: HashMap::new(),
        restore_wake: None,
        border,
        border_overlay: BorderOverlay::default(),
        border_last: None,
        underlay,
        underlay_overlay: UnderlayOverlay::default(),
        underlay_last: None,
        move_kind: HashMap::new(),
        gesture_start_cursor: HashMap::new(),
        preview_overlay: PreviewOverlay::default(),
        preview_last: None,
        preview_bound: HashMap::new(),
        gesture_preview_start: HashMap::new(),
        preview_dead: HashSet::new(),
        esc_latched: HashSet::new(),
        gesture_esc_seq: HashMap::new(),
        gesture_end_seq: HashMap::new(),
        underlay_chord_last: false,
        float_topmost_prev: std::collections::BTreeMap::new(),
        float_rects: HashMap::new(),
        floated: BTreeSet::new(),
        sticky: std::collections::BTreeMap::new(),
        fixed_tile_overrides: BTreeSet::new(),
        fixed_origin_overrides: BTreeSet::new(),
        fixed_auto_seen: BTreeSet::new(),
        born_first_exit: BTreeSet::new(),
        overlay_now: HashMap::new(),
        input_bridge: None,
        render_bridge: None,
        render_frames: RenderFrameHealth::default(),
        pending_releases: crate::workspace::PendingReleases::new(),
        pending_retry_fp: 0,
    };
    state.engine.sync_binding(&owner, &generation);
    state.workspace_proof = workspace_proof;
    // R-SPC-04 D1-D8 + G-05: enable the shared fixed-size admission opt-in
    // now that the adapter wires complete observation (bounded min/max
    // tracks with seeded-untouched stripping), lifetime-bound origin and
    // override stamps, and no-touch admission lanes. `Engine::new` defaults
    // OFF to preserve exact prior behavior; the predicate starts from the
    // last-good startup settings (both-axes default, proof owners included)
    // and later adopts live edits for subsequent admissions only.
    state.engine.set_fixed_size_admission(true);
    state
        .engine
        .set_fixed_size_predicate(fixed_predicate_for(state.settings_live.as_ref()));
    // Existing workspaces take the saved default at owner startup (initially
    // tiled); live default edits later affect only newly created workspaces.
    // Overrides reset on owner restart: nothing persists per workspace.
    let startup_default = state
        .settings_live
        .as_ref()
        .map(|live| live.settings.core.workspace.default_tiled)
        .unwrap_or(true);
    state.workspaces.set_default_tiled(startup_default);
    log_json_at(
        &log_path,
        serde_json::json!({
            "event": "workspace-default",
            "default_tiled": startup_default,
            "outcome": "adopted",
            "scope": "startup",
        }),
    );
    // Item 12 stage B: the saved scope mode adopts only on owner restart.
    // The adopted mode stays in force for the whole run; later live edits
    // report drift without adopting. Startup priming follows the initial
    // monitor sync; later hotplug retains its current behavior.
    let startup_scope = crate::workspace_scope_owner::startup_mode(state.settings_live.as_ref());
    state.scope_owner =
        crate::workspace_scope_owner::WorkspaceScopeOwner::adopt_startup(startup_scope);
    log_json_at(&log_path, state.scope_owner.startup_event());
    let mut areas = all_monitors()?;
    let (mut monitor, mut monitor_count) = (areas[0].clone(), areas.len());
    let mode = if workspace_proof {
        "workspace-proof"
    } else if shortcut_proof {
        "shortcut-proof"
    } else if state.allowlist.is_some() {
        "proof"
    } else {
        "normal"
    };
    // Visible takeover state: default on, explicit off, proof never hooks
    // except shortcut-proof/workspace-proof with test-only marker acceptance.
    let takeover = (state.keyboard.takeover && !proof) || shortcut_proof || workspace_proof;
    // Per-output-local workspaces start from live monitors: one domain set
    // per device key, retained Engine sessions per (output, workspace).
    sync_monitor_outputs(&mut state, &areas);
    // Item 12 stage B: scoped startup priming on the fixed topology (mode
    // order/history baselines plus shared current convergence and the
    // trailing floor). Runs once here; later hotplug syncs mode-aware
    // through `scope_hotplug`.
    scope_trailing::prime_after_sync(&mut state);
    log_json_at(
        &log_path,
        serde_json::json!({
            "event": "tile-start",
            "mode": mode,
            "trace": trace,
            "monitors": monitor_count,
            "work": [monitor.work.x, monitor.work.y, monitor.work.w, monitor.work.h],
            "full": [monitor.full.x, monitor.full.y, monitor.full.w, monitor.full.h],
            "inner": state.inner_gap,
            "outer": state.outer_gap,
            "keyboard": {"takeover": takeover, "allow_win_l": state.keyboard.allow_win_l},
            "mouse_snap_prevention": snap_want,
            "settings": state.settings_live.as_ref().map(|live| live.status.clone()),
            "first_run": first_run,
            "scope_count": state.scope.len(),
            "active_border": {
                "enabled": state.border.enabled,
                "width": state.border.style.width,
                "gap": state.border.style.gap,
                "radius": state.border.style.radius,
                "color": crate::active_border::render_color(state.border.style.color),
                "use_theme": state.border.style.use_theme,
            },
        }),
    );
    // Hook on this thread; this thread pumps messages, so callbacks run here.
    // Four narrow hooks keep the callback cheap: foreground wakes, move/size
    // transitions queue losslessly, object create/destroy/show/hide wakes,
    // and explicit location-change wakes (a range up to LOCATIONCHANGE would
    // also pull unrelated state-change noise, so it stays separate).
    let hooks = unsafe {
        [
            SetWinEventHook(
                EVENT_SYSTEM_FOREGROUND,
                EVENT_SYSTEM_FOREGROUND,
                std::ptr::null_mut(),
                Some(winevent_proc),
                0,
                0,
                WINEVENT_OUTOFCONTEXT,
            ),
            SetWinEventHook(
                EVENT_SYSTEM_MOVESIZESTART,
                EVENT_SYSTEM_MOVESIZEEND,
                std::ptr::null_mut(),
                Some(winevent_proc),
                0,
                0,
                WINEVENT_OUTOFCONTEXT,
            ),
            SetWinEventHook(
                EVENT_OBJECT_CREATE,
                EVENT_OBJECT_SHOW
                    .max(EVENT_OBJECT_DESTROY)
                    .max(EVENT_OBJECT_HIDE),
                std::ptr::null_mut(),
                Some(winevent_proc),
                0,
                0,
                WINEVENT_OUTOFCONTEXT,
            ),
            SetWinEventHook(
                EVENT_OBJECT_LOCATIONCHANGE,
                EVENT_OBJECT_LOCATIONCHANGE,
                std::ptr::null_mut(),
                Some(winevent_proc),
                0,
                0,
                WINEVENT_OUTOFCONTEXT,
            ),
        ]
    };
    if hooks.iter().any(|hook| hook.is_null()) {
        for hook in hooks {
            if !hook.is_null() {
                unsafe {
                    UnhookWinEvent(hook);
                }
            }
        }
        return Err(err("error: SetWinEventHook failed"));
    }
    // Owner-owned notification tray (normal owner only; proof owners stay
    // deterministic with no icon). Created after the hooks so no hook-failure
    // early return can leak it, and RAII `Drop` covers every later path.
    // Creation failure degrades to tray-less tiling, never a refused run.
    // The hidden owner window rides the loop-thread pump below, so its
    // TaskbarCreated and tray callbacks dispatch on this same thread. The
    // conflict projection uses the effective runtime keyboard policy
    // (takeover off passes everything through, so nothing warns; the Win+L
    // opt-in follows the runtime allow_win_l) over the CLI-overridden
    // settings base.
    let mut tray: Option<crate::tray_sys::TrayOwner> = if proof {
        None
    } else {
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
        // Session-only workspace toggle: the menu carries its rendered
        // opaque scope, the owner loop drains it once on its pump via
        // `take_toggle_request` and verifies it below. Default picks persist
        // through the settings file (normal owner).
        let toggle = Arc::new(Mutex::new(None::<crate::tray::ToggleScope>));
        // Stop-aware menu tracking: the modal track blocks this loop's pump,
        // so the menu thread observes the exact-owner stop file itself on a
        // 50 ms window timer and cancels via `EndMenu`. Same file plus match
        // predicate the loop polls below, so no ownership change. Read errors
        // poll false (never cancel on I/O noise); the loop's own check still
        // reports them.
        let menu_stop_dir = dir.to_path_buf();
        let menu_stop_creation = me.process_creation.clone();
        let menu_stop_poll: Option<Arc<dyn Fn() -> bool + Send + Sync>> =
            Some(Arc::new(move || {
                match std::fs::read_to_string(
                    menu_stop_dir.join(crate::lifecycle::STOP_REQUEST_FILE),
                ) {
                    Ok(text) => crate::lifecycle::stop_request_matches(&text, &menu_stop_creation),
                    Err(_) => false,
                }
            }));
        match crate::tray_sys::TrayOwner::create(
            conflict_empty,
            &settings_status,
            Some(log_path.clone()),
            &toggle,
            state.settings_dir.clone(),
            menu_stop_poll,
        ) {
            Ok(owner) => Some(owner),
            Err(error) => {
                log_json_at(
                    &log_path,
                    serde_json::json!({"event":"tray-unavailable","cause": error.to_string()}),
                );
                None
            }
        }
    };
    // Product keyboard takeover on the loop thread: the callback runs during
    // pump_wait on this same thread, so install/uninstall bracket the loop
    // with no join. Plain proof mode never installs a hook; shortcut-proof
    // installs with test-only marker acceptance. Install is best-effort with
    // bounded backoff retries below: failure degrades to keyboard-unavailable
    // (nothing consumes) while tiling continues, never a refused run.
    //
    // Product `tile` runs both hooks on one dedicated input pump instead
    // (stage 1): the callbacks never run on this owner thread, so owner
    // observation, Engine, actuation and I/O cannot delay delivery. Proof
    // commands keep the legacy same-thread hooks below untouched.
    let use_input_thread = !proof && !shortcut_proof && !workspace_proof;
    let mut snap_hook = None;
    let mut snap_failures: u32 = 0;
    let mut snap_retry_at: Option<Instant> = None;
    // Project Win+Left hook on the loop thread, same pump as the keyboard
    // hook. Installed once when takeover holds; normal runs also install it
    // while takeover is off so a live settings re-enable takes effect (the
    // gate below keeps everything passing through until then). Install
    // failure degrades to title-bar-only movement (Win+Left passes through
    // natively) while tiling continues, never a refused run.
    //
    // Product path: owned by the input pump instead (same install-while-off
    // contract, same pass-through gate via snapshots).
    let windrag_hook = if !use_input_thread && (takeover || !proof) {
        match crate::win_mouse::sys::install() {
            Ok(hook) => {
                log_json_at(&log_path, serde_json::json!({"event":"windrag-available"}));
                Some(hook)
            }
            Err(message) => {
                log_json_at(
                    &log_path,
                    serde_json::json!({"event":"windrag-unavailable","cause": message}),
                );
                None
            }
        }
    } else {
        None
    };
    // Dedicated input pump for the product path: both LL hooks plus their
    // synchronous classifiers on one thread. Readiness uses a bounded wait;
    // install failures retry on the pump with the same 5s-doubling/60s-cap
    // vocabulary and the loop keeps tiling degraded meanwhile. A spawn
    // failure degrades the same way (no hooks, tiling continues).
    if use_input_thread {
        match InputHandle::spawn(state.keyboard) {
            Ok(mut handle) => {
                let ready = handle.wait_ready(READY_WAIT, &mut |event: &FailEvent| {
                    log_json_at(
                        &log_path,
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
                if ready {
                    log_json_at(
                        &log_path,
                        serde_json::json!({"event":"input-ready","bounded_wait":"met"}),
                    );
                } else {
                    log_json_at(
                        &log_path,
                        serde_json::json!({"event":"input-pending","note":"hooks retry on the input pump; tiling continues degraded"}),
                    );
                }
                let channels = handle.channels().clone();
                // Esc clocks and edges are process-global singletons sampled
                // directly by callbacks and WinEvent START/END; no per-handle
                // registration can mismatch them.
                // Fence generations start at the process-global maximum so a
                // second same-process owner run re-arms the callback fence
                // above every earlier allocation instead of idling below a
                // pinned `fetch_max` ceiling (a reset would race an active
                // pump; a leftover pending fence stays fail-closed).
                state.input_bridge = Some(InputBridge {
                    channels: Arc::clone(&channels),
                    sender: InboxSender::new(Arc::clone(&channels.inbox)),
                    handle,
                    last_snapshot: None,
                    retry_gen: None,
                    force_snapshot: false,
                    last_clear_seen: 0,
                    published_gen: fence_base_generation(),
                    applied_gen: 0,
                    last_faults: 0,
                    last_deduped: 0,
                    pending_ack: None,
                    last_ack_logged: 0,
                });
            }
            Err(cause) => {
                log_json_at(
                    &log_path,
                    serde_json::json!({"event":"input-spawn-failed","cause": cause}),
                );
            }
        }
    }
    // Dedicated renderer for the product path: all three overlay carriers
    // move to one renderer thread (stage 2, option B). Readiness uses a
    // bounded wait; a spawn failure degrades to same-thread overlays while
    // tiling continues, never a refused run. Proof paths keep `None` above
    // and stay same-thread.
    if use_input_thread {
        match RenderHandle::spawn() {
            Ok(mut handle) => {
                let ready = handle.wait_ready(RENDER_READY_WAIT, &mut |event: &RenderFailEvent| {
                    log_json_at(
                        &log_path,
                        if event.recovered {
                            serde_json::json!({
                                "event": "render-available",
                                "op": event.op,
                                "producer": "render-thread",
                            })
                        } else {
                            let mut line = serde_json::json!({
                                "event": "render-unavailable",
                                "op": event.op,
                                "attempt": event.attempt,
                                "retry_secs": event.retry_secs,
                                "producer": "render-thread",
                            });
                            if let Some(detail) = event.detail.as_ref() {
                                line["cause"] = serde_json::Value::from(detail.clone());
                            }
                            line
                        },
                    );
                });
                if ready {
                    log_json_at(
                        &log_path,
                        serde_json::json!({"event":"render-ready","bounded_wait":"met"}),
                    );
                } else {
                    log_json_at(
                        &log_path,
                        serde_json::json!({"event":"render-pending","note":"renderer retries creation on its pump; tiling continues with pending presents"}),
                    );
                }
                let channels = handle.channels().clone();
                state.render_bridge = Some(RenderBridge {
                    channels,
                    handle: Some(handle),
                    gens: [0, 0, 0],
                    hide_posted: [false, false, false],
                    hide_reason: [None, None, None],
                    hidden_ack: [None, None, None],
                    posted: std::array::from_fn(|_| VecDeque::new()),
                    applied_visible: [false, false, false],
                    applied_note: [None, None, None],
                    applied_ack: [None, None, None],
                    applied_known: [false, false, false],
                    admission_open: true,
                    pending: None,
                });
            }
            Err(cause) => {
                log_json_at(
                    &log_path,
                    serde_json::json!({"event":"render-spawn-failed","cause": cause}),
                );
            }
        }
    }
    let deadline = seconds.map(|s| Instant::now() + Duration::from_secs(s));
    let mut slow_last = Instant::now();
    // Trace-only health window (input ages, owner work, windrag transport):
    // plain local, never shared across threads. Each loop iteration lends
    // it to its `OwnerWorkGuard` for the turn; the pending end-of-run
    // summary flushes after the loop below. Untouched (and unborrowed)
    // when tracing is off, so default runs pay nothing here.
    let mut health = Health::new();
    let result = (|| -> Result<()> {
        let fulls = monitor_fulls(&areas);
        // Mouse prevention is deferred until an actual active tick or resume:
        // an initial fullscreen must not disable only to immediately restore.
        // The first active tick, resume, or work-area change drives the
        // effect; teardown and crash restore keep the lease-held guarantee.
        let mut snap_primed = false;
        // Fullscreen guard applies before the initial tick as well.
        // Verified managed overlays bypass like a managed maximize.
        if suspend_read(&mut state, me, &fulls).veto.block {
            state.suspended = true;
            state.snap_advance = None;
            log_json_at(
                &log_path,
                serde_json::json!({"event":"suspend","cause":"fullscreen-foreground"}),
            );
            if snap_want {
                snap_suspend(dir, me, store);
            }
        } else {
            if !areas
                .iter()
                .any(|a| tiling_domain_bounds_with(a.work, state.outer_gap).is_some())
            {
                return Err(err("error: work area cannot carry outer gap"));
            }
            if snap_want {
                snap_resume(dir, me, store);
                snap_primed = true;
            }
            reconcile_tick(&mut state, me, &fulls, &areas);
            refresh_active_border(&mut state, me, &fulls);
            refresh_group_underlay(&mut state, me, &fulls, &areas);
        }
        // Trace-only owner-work sequence: counted only when tracing so
        // default runs pay no timer/counter work.
        let mut owner_seq: u64 = 0;
        // Last observed windrag offer count for change-only transport
        // observations (replaces the per-offer `windrag-transport` line).
        let mut trace_windrag_offers: u64 = 0;
        // Last observed pump turn/offer counts for change-only input
        // observations: a fresh turn (or batch/drop) samples the slot
        // once; an unchanged diag is never resampled. No per-turn file
        // writes: observations aggregate, only throttled outliers and
        // window summaries log.
        let mut trace_last_turns: u64 = 0;
        let mut trace_last_offers: u64 = 0;
        // Trace-only outer-wait return counts (outer product wait only;
        // focus/proof `pump_wait` calls never touch these). `waits` counts
        // outer returns, `ack_only` the ack-only subset that returns early
        // before slow maintenance, and the reason hits count `WaitWake` flags
        // per return (combinations possible: ack plus messages is common, so
        // reason hits can exceed `waits`). Native wait returns only, never
        // thread context switches. Counted only when tracing so default runs
        // pay no counter work.
        let mut ow_counts = OwnerWaitCounts::default();
        loop {
            if stop_requested(dir, me)? || tray.as_ref().is_some_and(|t| t.stop_requested()) {
                return Ok(());
            }
            if deadline.is_some_and(|end| Instant::now() >= end) {
                return Ok(());
            }
            let wait = pump_wait_with_intent(
                TICK_POLL_MS,
                state
                    .input_bridge
                    .as_ref()
                    .and_then(|bridge| bridge.channels.intent_wake()),
                state
                    .render_bridge
                    .as_ref()
                    .and_then(|bridge| bridge.handle.as_ref())
                    .map(|handle| handle.ack_wake()),
            );
            // Trace-only owner-work clock starts here: `busy_us` excludes the
            // wait above plus callbacks delivered inside it. Scoped to this
            // iteration so every `continue`/error path below still samples.
            // Outer-wait return accounting lives here too (trace-only): one
            // `ow_waits` per outer return plus one hit per set `WaitWake`
            // flag. The ack-only subset increments later at the early return,
            // once hook events are known.
            let mut owner_work = OwnerWorkGuard::begin(
                state.trace,
                &log_path,
                owner_seq,
                state.tick,
                if state.trace { Some(&mut health) } else { None },
            );
            if state.trace {
                owner_seq = owner_seq.saturating_add(1);
                ow_counts.note_wait(wait);
            }
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
                        if owned_overlay_hwnd(&state, RenderCarrier::Border) == Some(raw as u64)
                            || owned_overlay_hwnd(&state, RenderCarrier::Underlay)
                                == Some(raw as u64)
                            || owned_overlay_hwnd(&state, RenderCarrier::Preview)
                                == Some(raw as u64)
                        {
                            continue;
                        }
                        woke = true;
                        if state.trace {
                            ow_wakes += 1;
                        }
                    }
                    HookEvent::MoveSizeStart(raw, start_seq) => {
                        if owned_overlay_hwnd(&state, RenderCarrier::Border) == Some(raw as u64)
                            || owned_overlay_hwnd(&state, RenderCarrier::Underlay)
                                == Some(raw as u64)
                            || owned_overlay_hwnd(&state, RenderCarrier::Preview)
                                == Some(raw as u64)
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
                        capture_gesture_start(&mut state, me, hwnd, start_seq);
                        state.active.insert(hwnd);
                        // Move-vs-resize classification, sampled once at START
                        // (the WinEvent itself does not distinguish). Hung or
                        // unreadable windows classify Unknown: never a trigger.
                        let kind = classify_move_size_start(hwnd);
                        state.move_kind.insert(hwnd, kind);
                        log_json_at(
                            &log_path,
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
                        if owned_overlay_hwnd(&state, RenderCarrier::Border) == Some(raw as u64)
                            || owned_overlay_hwnd(&state, RenderCarrier::Underlay)
                                == Some(raw as u64)
                            || owned_overlay_hwnd(&state, RenderCarrier::Preview)
                                == Some(raw as u64)
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
            // Ack accounting runs on every turn (cheap: three mutex takes
            // plus the bounded failure drain), then the pending finish is
            // serviced from the ack state with no blocking wait.
            drain_render_failures(&mut state);
            poll_render_acks(&mut state, Instant::now());
            service_pending_drop(&mut state, me, &fulls, &areas);
            // Ack-only turns (renderer ack with no input, message, timeout,
            // or hook-callback event) loop here without reposting frames,
            // reconciling, polling settings, or touching input: an ack alone
            // never triggers owner work, so the ack wake cannot feed back
            // into a post/ack/post loop. Hook events above force a full
            // turn, so no input starves; the timeout preserves the timed
            // follow/reassert cadence, and the pending finish above still
            // completes with no added latency.
            if !(wait.full_turn() || woke) {
                if state.trace {
                    ow_counts.note_ack_only();
                }
                continue;
            }
            let now = Instant::now();
            let slow = now.duration_since(slow_last).as_millis() >= u128::from(SLOW_POLL_MS);
            // Best-effort keyboard hook with bounded backoff retries (5s
            // doubling, 60s cap): failure logs one `snap-unavailable` per
            // attempt, never per-poll noise, and the loop keeps tiling with
            // nothing consuming. Resume and work-area changes re-arm an
            // immediate retry; there is no permanent disable. The gate
            // follows the live takeover setting so a settings re-enable
            // installs the hook, and a fresh install publishes the live
            // rebound table immediately.
            let takeover_live =
                (state.keyboard.takeover && !proof) || shortcut_proof || workspace_proof;
            if state.input_bridge.is_some() {
                // Product path: the pump owns install/retry for both hooks.
                // Drain failure events for the owner writer; late readiness
                // completes here if the bounded startup wait expired first.
                // Then retry pending inbox payloads and poll the apply ack.
                if let Some(bridge) = state.input_bridge.as_mut() {
                    bridge.handle.drain_failures(&mut |event: &FailEvent| {
                        log_json_at(
                            &log_path,
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
                    log_input_faults(bridge, &log_path);
                }
                poll_input_ack(&mut state);
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
                        snap_hook = Some(hook);
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
                        snap_failures = 0;
                        snap_retry_at = None;
                        log_json_at(&log_path, serde_json::json!({"event":"snap-available"}));
                    }
                    Err(message) => {
                        snap_failures += 1;
                        let backoff = (5u64 << snap_failures.min(4)).min(60);
                        snap_retry_at = Some(now + Duration::from_secs(backoff));
                        log_json_at(
                            &log_path,
                            serde_json::json!({
                                "event": "snap-unavailable",
                                "attempt": snap_failures,
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
            if poll_live_settings(&mut state, dir, me, store, &mut snap_want) {
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
                        let toggle_fulls = monitor_fulls(&areas);
                        toggle_workspace_tiling(
                            &mut state,
                            me,
                            &toggle_fulls,
                            &areas,
                            &request.output,
                        );
                        woke = true;
                    } else {
                        // Stale or mismatched menu scope (output switch or a
                        // pruned workspace between render and pick): refuse
                        // with opaque tokens only, never raw scope ids.
                        log_json_at(
                            &log_path,
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
                offer_input_snapshot(&mut state, candidate);
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
                                &mut state,
                                me,
                                &fulls,
                                &areas,
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
                            &log_path,
                            serde_json::json!({"event":"snap-drop","lost": lost}),
                        );
                    }
                    let turns_changed = diag.turns != trace_last_turns;
                    let offers_changed = diag.offers != trace_last_offers;
                    if !batch.is_empty() || drops_changed || turns_changed {
                        trace_last_turns = diag.turns;
                        if offers_changed {
                            trace_last_offers = diag.offers;
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
                                &log_path,
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
                    if offers != trace_windrag_offers {
                        trace_windrag_offers = offers;
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
                            &log_path,
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
                            &log_path,
                            serde_json::json!({"event":"windrag-drop","lost": lost}),
                        );
                        if !state.windrag_bound.is_empty() {
                            let open: Vec<u64> = state.windrag_bound.keys().copied().collect();
                            for hwnd in open {
                                clear_windrag_gesture(&mut state, hwnd);
                                if !offer_input_control(
                                    &mut state,
                                    InboxItem::InvalidateWindrag(hwnd),
                                ) {
                                    log_json_at(
                                        &log_path,
                                        serde_json::json!({
                                            "event": "windrag-drop-loss-unreported",
                                            "note": "invalidation held for retry; paired Up stays swallowed",
                                        }),
                                    );
                                }
                            }
                            log_json_at(
                                &log_path,
                                serde_json::json!({"event":"windrag-drop-loss","cleared": true}),
                            );
                            let fulls_owned = fulls.to_vec();
                            reconcile_tick(&mut state, me, &fulls_owned, &areas);
                        }
                    }
                }
                edges
            } else if windrag_hook.is_some() {
                crate::win_mouse::sys::publish(&state.windrag_origins, snap_gate_active);
                let stats = crate::win_mouse::sys::hook_stats();
                if stats != state.windrag_stats_logged {
                    state.windrag_stats_logged = stats;
                    log_json_at(
                        &log_path,
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
            if !use_input_thread && windrag_hook.is_some() {
                let dropped = crate::win_mouse::sys::queue_dropped();
                if dropped > state.windrag_dropped {
                    let lost = dropped - state.windrag_dropped;
                    state.windrag_dropped = dropped;
                    log_json_at(
                        &log_path,
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
                            clear_windrag_gesture(&mut state, hwnd);
                            crate::win_mouse::sys::invalidate(hwnd);
                        }
                        log_json_at(
                            &log_path,
                            serde_json::json!({"event":"windrag-drop-loss","cleared": true}),
                        );
                        let fulls_owned = fulls.to_vec();
                        reconcile_tick(&mut state, me, &fulls_owned, &areas);
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
                    if let Some(fresh) =
                        state.observe(me, &fulls, &mut skipped, &mut press_retained)
                    {
                        press_observed = fresh;
                    }
                }
                for edge in windrag_edges {
                    match edge.kind {
                        WinDragKind::Down => match windrag_down(
                            &mut state,
                            me,
                            &edge,
                            &fulls,
                            &press_observed,
                            &press_retained,
                        ) {
                            Ok(token) => {
                                log_json_at(
                                    &log_path,
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
                                    &log_path,
                                    serde_json::json!({
                                        "event": "windrag-down-refused",
                                        "tick": state.tick,
                                        "reason": reason,
                                        "producer": "windrag",
                                    }),
                                );
                            }
                        },
                        WinDragKind::Up => {
                            windrag_up(&mut state, edge.hwnd, edge.x, edge.y, edge.esc_seq);
                        }
                        WinDragKind::Cancel => {
                            // Hook-side disarm after a consumed Down: drop
                            // the owner gesture with no plan and no geometry
                            // change. The paired Up stays swallowed
                            // hook-side; native gestures are untouched.
                            if let Some(token) = clear_windrag_gesture(&mut state, edge.hwnd) {
                                log_json_at(
                                    &log_path,
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
                        &log_path,
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
            if slow {
                slow_last = now;
                let fresh = all_monitors()?;
                let devices: Vec<String> = areas.iter().map(|a| a.device.clone()).collect();
                let fresh_devices: Vec<String> = fresh.iter().map(|a| a.device.clone()).collect();
                let geometry_changed = fresh.len() != monitor_count
                    || fresh_devices != devices
                    || fresh.iter().any(|a| {
                        areas
                            .iter()
                            .find(|b| b.device == a.device)
                            .is_none_or(|b| b.work != a.work || b.full != a.full)
                    });
                if geometry_changed {
                    areas = fresh;
                    monitor = areas[0].clone();
                    monitor_count = areas.len();
                    // Arrival/removal relocates whole-workspace Engine
                    // sessions; bounds-only changes reproject retained
                    // sessions without touching topology.
                    sync_monitor_outputs(&mut state, &areas);
                    for output in state.workspaces.output_keys() {
                        let Some(active) = state.workspaces.active_id(&output) else {
                            continue;
                        };
                        if let Some((_, key)) = workspace_domain_for(
                            &output,
                            &active,
                            &areas,
                            state.inner_gap,
                            state.outer_gap,
                        ) && let Some(area) = areas.iter().find(|a| a.device == output)
                            && let Some(bounds) =
                                tiling_domain_bounds_with(area.work, state.outer_gap)
                        {
                            state.engine.reproject_retained(&key, bounds);
                        }
                    }
                    log_json_at(&log_path, serde_json::json!({"event":"work-area-changed"}));
                    snap_retry_at = None;
                    // A changed work area is a meaningful retry point for a
                    // degraded Snap setup; quiet when the effect holds.
                    if snap_want && !state.suspended {
                        snap_resume(dir, me, store);
                        snap_primed = true;
                    }
                    woke = true;
                }
                // Trace-only pump-wait summary at the existing slow cadence:
                // one cumulative line per slow tick, no new timer or events.
                // Reads only (diag snapshot plus renderer atomics/ack seq);
                // never arms owner work and never signals a pump wake, so the
                // summary cannot induce a wake. Owner counts cover outer
                // product waits only (focus/proof nested waits excluded by
                // construction); ack-only turns counted in `owner_ack_only`
                // return before this block, so this line rides full turns.
                // Input `turns` counts completed iterations and equals
                // `input_waits`; stop before the wait increments neither;
                // `*_timeout` fields count genuine `WAIT_TIMEOUT` (258)
                // expiries only while `*_failed` counts `WAIT_FAILED` /
                // unrecognized codes separately (a failed wait returns
                // immediately; not proven delayed). Reason splits count native
                // wait returns only, never context switches; render acks ride
                // `render_acks` (`ack_seq`), never the wait counts. The input
                // struct arrives under one diag-slot lock (one coherent
                // snapshot); the renderer tuple is four independent atomic
                // loads, so a concurrent bump can skew the sum by one, and
                // cross-pump coherence is not claimed. Idle diffs take
                // start/end lines from one traced run; stop short-circuits
                // (owner return, input early return, renderer counted stop
                // wait) are the only lifecycle exclusions.
                if state.trace {
                    let (in_turns, in_waits, in_work, in_msg, in_timeout, in_failed) =
                        match state.input_bridge.as_ref() {
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
                    let (re_waits, re_work, re_msg, re_timeout, re_failed, re_acks) =
                        match state.render_bridge.as_ref() {
                            Some(bridge) => {
                                let (waits, work, msg, timeout, failed) =
                                    bridge.channels.wait_counts();
                                (waits, work, msg, timeout, failed, bridge.channels.ack_seq())
                            }
                            None => (0, 0, 0, 0, 0, 0),
                        };
                    log_json_at(
                        &log_path,
                        serde_json::json!({
                            "event": "pump-waits",
                            "owner_waits": ow_counts.waits,
                            "owner_ack_only": ow_counts.ack_only,
                            "owner_input": ow_counts.input,
                            "owner_ack": ow_counts.ack,
                            "owner_msg": ow_counts.msg,
                            "owner_timeout": ow_counts.timeout,
                            "owner_failed": ow_counts.failed,
                            "input_turns": in_turns,
                            "input_waits": in_waits,
                            "input_work": in_work,
                            "input_msg": in_msg,
                            "input_timeout": in_timeout,
                            "input_failed": in_failed,
                            "render_waits": re_waits,
                            "render_work": re_work,
                            "render_msg": re_msg,
                            "render_timeout": re_timeout,
                            "render_failed": re_failed,
                            "render_acks": re_acks,
                            "scope": "outer-only-plus-native-waits",
                            "excludes": "focus-proof-nested-waits,stop-short-circuit",
                        }),
                    );
                }
            }
            // Exact-owner out-of-hook select wakes the loop even with no other
            // event; the poll itself consumes once and dispatches through the
            // existing resolver. Checked before the idle skip so a pending
            // request never waits for an unrelated wake.
            if dir.join(WORKSPACE_REQUEST_FILE).exists() || dir.join(RESIZE_REQUEST_FILE).exists() {
                woke = true;
            }
            // Bare-chord edge wake: GetAsyncKeyState levels are sampled here,
            // before the idle skip, so a Win+Shift press/release with no
            // WinEvent wakes the visual refresh on the 100ms pump cadence
            // instead of waiting for the 2s slow poll. Steady hold stays
            // quiet; the suspend/fullscreen path below still gates the
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
                    restore_wake_probe(&state, state.restore_wake.as_ref().expect("armed"), now);
                let (keep, demand) =
                    crate::tiling::restore_wake_step(true, expired, lost, done, false);
                if demand {
                    woke = true;
                }
                if !keep {
                    state.restore_wake = None;
                }
            }
            // An open managed gesture wakes the 100ms pump for the
            // drop-preview track: pointer moves coalesce into one Engine
            // preview sample per tick on both the native frame-following
            // and the stationary Win+Left producers. The refresh itself
            // gates resize/unknown/invalid holds; the wake only needs START
            // capture.
            if !woke
                && state.active.iter().any(|hwnd| {
                    state.managed.contains(hwnd) && state.gesture_before.contains_key(hwnd)
                })
            {
                woke = true;
            }
            // Trace-only owner-work gate: quiet short-poll iterations drop the
            // disabled guard silently above via `continue`. Active wakes arm
            // the single sample; RAII then records suspend/held/error exits.
            // Due window summaries also flush on the idle path below (no new
            // wake: the check rides this existing turn, and quiet windows
            // with no activity emit nothing).
            if state.trace && (woke || slow) {
                let gesture_held = state.active.iter().any(|hwnd| state.managed.contains(hwnd));
                owner_work.arm(ow_batch_total, ow_wakes, ow_starts, ow_ends, gesture_held);
            }
            // Per-turn maintenance-quiet flag for the trailing health
            // flush at the end of the turn (set on the quiet branch).
            let mut maintenance_quiet = false;
            if state.trace && !(woke || slow) {
                owner_work.finish_turn(false);
            }
            if !(woke || slow) {
                continue;
            }
            let fulls = monitor_fulls(&areas);
            if suspend_read(&mut state, me, &fulls).veto.block {
                if !state.suspended {
                    state.suspended = true;
                    state.snap_advance = None;
                    log_json_at(
                        &log_path,
                        serde_json::json!({"event":"suspend","cause":"fullscreen-foreground"}),
                    );
                    if snap_want {
                        snap_suspend(dir, me, store);
                    }
                } else {
                    state.snap_advance = None;
                }
                if !snap_events.is_empty() {
                    keyboard_tick(
                        &mut state,
                        me,
                        &fulls,
                        &areas,
                        snap_events,
                        Some("suspended"),
                    );
                }
                if !workspace_events.is_empty() {
                    workspace_tick(
                        &mut state,
                        me,
                        store,
                        dir,
                        &fulls,
                        &areas,
                        workspace_events,
                        Some("suspended"),
                    );
                }
                if !send_events.is_empty() {
                    workspace_relative_send_tick(
                        &mut state,
                        me,
                        store,
                        dir,
                        &fulls,
                        &areas,
                        send_events,
                        Some("suspended"),
                    );
                }
                if !output_events.is_empty() {
                    workspace_output_send_tick(
                        &mut state,
                        me,
                        &fulls,
                        &areas,
                        output_events,
                        Some("suspended"),
                    );
                }
                if !migrate_events.is_empty() {
                    workspace_migrate_tick(
                        &mut state,
                        me,
                        store,
                        dir,
                        &fulls,
                        &areas,
                        migrate_events,
                        Some("suspended"),
                    );
                }
                if !history_events.is_empty() {
                    workspace_history_tick(
                        &mut state,
                        me,
                        store,
                        dir,
                        &fulls,
                        &areas,
                        history_events,
                        Some("suspended"),
                    );
                }
                // Out-of-hook select observes the same suspension: consumed
                // once with a suspended outcome, never applied while a
                // fullscreen foreground holds the session. The test-needed
                // resize poll carries its own suspend fence and refuses the
                // same way.
                poll_workspace_cli_request(&mut state, me, store, dir, &fulls, &areas);
                poll_resize_cli_request(&mut state, me, dir, &fulls, &areas);
                state.gesture_before.clear();
                state.active.clear();
                state.move_kind.clear();
                state.esc_latched.clear();
                state.gesture_esc_seq.clear();
                state.gesture_end_seq.clear();
                state.gesture_end_cursor.clear();
                state.gesture_start_key.clear();
                state.gesture_start_tag.clear();
                state.gesture_producer.clear();
                state.windrag_start_cursor.clear();
                state.windrag_bound.clear();
                state.preview_bound.clear();
                state.preview_dead.clear();
                state.gesture_preview_start.clear();
                state.gesture_start_cursor.clear();
                // A stale Esc edge must not leak into the next gesture.
                // Routes through the input bridge on the product path
                // (ordered FIFO with snapshots; held pairs keep their stored
                // verdicts until hook release) and clears the process-global
                // edge flags in pump order via the ClearEsc apply; direct
                // sys calls on proof paths. Atomic sequencing guarantees a
                // pre-gesture edge never cancels a later gesture.
                offer_input_control(&mut state, InboxItem::ClearEsc);
                // A suspended session disarms the project hold with no
                // replay; the consumed click already passed, so no half
                // gesture can strand later.
                offer_input_control(&mut state, InboxItem::ClearWindrag);
                hide_border(&mut state, "suspended");
                hide_underlay(&mut state, "suspended");
                hide_preview(&mut state, "suspended");
                // Suspended turns still commit their owner sample (the
                // guard would otherwise record it silently on drop with
                // no summary flush); then the disarmed guard drops.
                if state.trace {
                    owner_work.finish_turn(false);
                }
                continue;
            }
            if state.suspended {
                state.suspended = false;
                log_json_at(&log_path, serde_json::json!({"event":"resume"}));
                // A fresh foreground may accept the hook now: retry soon.
                snap_retry_at = None;
                // Fresh Snap preimage when the setting drifted during
                // suspension (or setup degraded earlier); quiet when the
                // effect is still ours. This is also the deferred initial
                // effect when the run started suspended.
                if snap_want {
                    snap_resume(dir, me, store);
                    snap_primed = true;
                }
            } else if snap_want && !snap_primed {
                // Deferred initial effect: the run started active but setup
                // no longer disables. Drive once on the first active tick.
                snap_resume(dir, me, store);
                snap_primed = true;
            }
            // Border follows the live foreground on every active wake,
            // including managed gestures that pause tiling below: the refresh
            // reads one fresh frame and never waits for reconciliation.
            refresh_border_timed(&mut state, me, &fulls, &mut owner_work);
            // The group fill follows on the same wake: chord/move trigger over
            // the Engine-projected source union, same freshness contract.
            refresh_group_underlay(&mut state, me, &fulls, &areas);
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
                .gesture_producer
                .retain(|hwnd, _| state.managed.contains(hwnd));
            state
                .windrag_start_cursor
                .retain(|hwnd, _| state.managed.contains(hwnd));
            state
                .windrag_bound
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
            // claiming an explicit cancel; the project windrag edge below is
            // the one deliberate injected exception (the stationary hold has
            // no native modal loop to cancel it).
            {
                // Product path consumes the process-global edges published
                // directly in hook callbacks (no pump-forwarding delay can
                // hide an Esc landing just before a WinEvent END); sequences
                // come from the global atomic clocks sampled directly, never
                // a delayed batch. Cancellation still requires a newer
                // sequence than START, so stale flags never cancel.
                let (esc_hit, now_seq, windrag_hit, windrag_now) = if state.input_bridge.is_some() {
                    (
                        crate::input_handoff::global_esc_edge()
                            .swap(false, std::sync::atomic::Ordering::SeqCst),
                        crate::snapkey::sys::shared_esc_seq(),
                        crate::input_handoff::global_windrag_esc_edge()
                            .swap(false, std::sync::atomic::Ordering::SeqCst),
                        crate::snapkey::sys::shared_windrag_esc_seq(),
                    )
                } else {
                    (
                        crate::snapkey::sys::take_esc_edge(),
                        crate::snapkey::sys::esc_seq(),
                        crate::snapkey::sys::take_windrag_esc_edge(),
                        crate::snapkey::sys::windrag_esc_seq(),
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
                // Project-gesture cancel edge (physical or injected Esc):
                // latches open windrag gestures only, with the same
                // START/END sequence ordering. Never admits a command;
                // settle reports the explicit `gesture-cancelled-esc`.
                if windrag_hit {
                    let now_seq = windrag_now;
                    for hwnd in state.active.iter() {
                        if !state.managed.contains(hwnd) {
                            continue;
                        }
                        if state.gesture_producer.get(hwnd).copied() != Some("windrag") {
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
                        if state.gesture_producer.get(hwnd).copied() != Some("windrag") {
                            continue;
                        }
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
            // waiting for mouse-up (the stationary gesture has no native
            // modal loop to end it; the native loop ends on its own).
            for hwnd in state.esc_latched.iter().copied().collect::<Vec<_>>() {
                if state.preview_bound.remove(&hwnd).is_some()
                    || render_carrier_visible(&state, RenderCarrier::Preview)
                {
                    hide_preview(&mut state, "esc-cancelled");
                }
                state.preview_dead.remove(&hwnd);
                state.gesture_preview_start.remove(&hwnd);
                state.gesture_start_cursor.remove(&hwnd);
            }
            let paused = state.active.iter().any(|hwnd| state.managed.contains(hwnd));
            if paused {
                // A managed gesture holds the loop and invalidates any
                // own-focus chain; keyboard intents of this batch are stale
                // and drop safely.
                state.snap_advance = None;
                if !snap_events.is_empty() {
                    keyboard_tick(&mut state, me, &fulls, &areas, snap_events, Some("gesture"));
                }
                if !workspace_events.is_empty() {
                    workspace_tick(
                        &mut state,
                        me,
                        store,
                        dir,
                        &fulls,
                        &areas,
                        workspace_events,
                        Some("gesture"),
                    );
                }
                if !send_events.is_empty() {
                    workspace_relative_send_tick(
                        &mut state,
                        me,
                        store,
                        dir,
                        &fulls,
                        &areas,
                        send_events,
                        Some("gesture"),
                    );
                }
                if !output_events.is_empty() {
                    workspace_output_send_tick(
                        &mut state,
                        me,
                        &fulls,
                        &areas,
                        output_events,
                        Some("gesture"),
                    );
                }
                if !migrate_events.is_empty() {
                    workspace_migrate_tick(
                        &mut state,
                        me,
                        store,
                        dir,
                        &fulls,
                        &areas,
                        migrate_events,
                        Some("gesture"),
                    );
                }
                if !history_events.is_empty() {
                    workspace_history_tick(
                        &mut state,
                        me,
                        store,
                        dir,
                        &fulls,
                        &areas,
                        history_events,
                        Some("gesture"),
                    );
                }
                // Drop-preview track on the 100ms pump while held: fresh
                // observation plus the shared Engine resolver per moved
                // pointer, on both producers. No focus, no geometry writes.
                refresh_drag_preview(&mut state, me, &fulls, &areas);
                // Due routine-frame summaries flush on this existing turn so
                // per-move preview aggregates never strand behind the hold.
                {
                    let now = Instant::now();
                    let lines = state.render_frames.take_due(now);
                    for line in lines {
                        log_json_at(&log_path, line);
                    }
                }
                continue;
            }
            if ended.is_empty() {
                if snap_events.is_empty()
                    && workspace_events.is_empty()
                    && send_events.is_empty()
                    && output_events.is_empty()
                    && migrate_events.is_empty()
                    && history_events.is_empty()
                {
                    reconcile_tick(&mut state, me, &fulls, &areas);
                    if state.restore_wake.is_some() {
                        let (expired, lost, done) = restore_wake_probe(
                            &state,
                            state.restore_wake.as_ref().expect("armed"),
                            Instant::now(),
                        );
                        let (keep, _) =
                            crate::tiling::restore_wake_step(true, expired, lost, done, true);
                        if !keep {
                            state.restore_wake = None;
                        }
                    }
                    workspace_maintenance(&mut state, me, store, dir, &fulls, &areas);
                    // Quiet-turn maintenance arms the trailing health
                    // flush at the end of the turn so pending activity
                    // never strands behind idle.
                    maintenance_quiet = true;
                    poll_workspace_cli_request(&mut state, me, store, dir, &fulls, &areas);
                    poll_resize_cli_request(&mut state, me, dir, &fulls, &areas);
                } else {
                    if !snap_events.is_empty() {
                        keyboard_tick(&mut state, me, &fulls, &areas, snap_events, None);
                    }
                    if !workspace_events.is_empty() {
                        workspace_tick(
                            &mut state,
                            me,
                            store,
                            dir,
                            &fulls,
                            &areas,
                            workspace_events,
                            None,
                        );
                    }
                    if !send_events.is_empty() {
                        workspace_relative_send_tick(
                            &mut state,
                            me,
                            store,
                            dir,
                            &fulls,
                            &areas,
                            send_events,
                            None,
                        );
                    }
                    if !output_events.is_empty() {
                        workspace_output_send_tick(
                            &mut state,
                            me,
                            &fulls,
                            &areas,
                            output_events,
                            None,
                        );
                    }
                    if !migrate_events.is_empty() {
                        workspace_migrate_tick(
                            &mut state,
                            me,
                            store,
                            dir,
                            &fulls,
                            &areas,
                            migrate_events,
                            None,
                        );
                    }
                    if !history_events.is_empty() {
                        workspace_history_tick(
                            &mut state,
                            me,
                            store,
                            dir,
                            &fulls,
                            &areas,
                            history_events,
                            None,
                        );
                    }
                    poll_workspace_cli_request(&mut state, me, store, dir, &fulls, &areas);
                }
            } else {
                // A just-ended managed gesture settles before keyboard
                // continuation resumes.
                state.snap_advance = None;
                if !snap_events.is_empty() {
                    keyboard_tick(&mut state, me, &fulls, &areas, snap_events, Some("gesture"));
                }
                if !workspace_events.is_empty() {
                    workspace_tick(
                        &mut state,
                        me,
                        store,
                        dir,
                        &fulls,
                        &areas,
                        workspace_events,
                        Some("gesture"),
                    );
                }
                if !send_events.is_empty() {
                    workspace_relative_send_tick(
                        &mut state,
                        me,
                        store,
                        dir,
                        &fulls,
                        &areas,
                        send_events,
                        Some("gesture"),
                    );
                }
                if !output_events.is_empty() {
                    workspace_output_send_tick(
                        &mut state,
                        me,
                        &fulls,
                        &areas,
                        output_events,
                        Some("gesture"),
                    );
                }
                if !migrate_events.is_empty() {
                    workspace_migrate_tick(
                        &mut state,
                        me,
                        store,
                        dir,
                        &fulls,
                        &areas,
                        migrate_events,
                        Some("gesture"),
                    );
                }
                if !history_events.is_empty() {
                    workspace_history_tick(
                        &mut state,
                        me,
                        store,
                        dir,
                        &fulls,
                        &areas,
                        history_events,
                        Some("gesture"),
                    );
                }
                gesture_tick(&mut state, me, &fulls, &areas, &ended);
                poll_workspace_cli_request(&mut state, me, store, dir, &fulls, &areas);
            }
            // Post-mutation border refresh: reconciliation may have moved the
            // target this tick, so re-read the fresh frame instead of leaving
            // the pre-reconcile geometry stale until the next wake. Own
            // overlay events are filtered above, so no feedback loop.
            refresh_border_timed(&mut state, me, &fulls, &mut owner_work);
            // Same for the group fill: a moved target may have joined another
            // group, so the projected union re-resolves instead of lingering.
            refresh_group_underlay(&mut state, me, &fulls, &areas);
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
                    log_json_at(&log_path, line);
                }
            }
        }
    })();
    // Pending end-of-run health summary (trace-only, at most one marked
    // line per stream): retained activity below the window age still
    // reports instead of dying with the loop.
    if state.trace {
        for line in health.take_end_of_run(Instant::now()) {
            log_json_at(&log_path, line);
        }
    }
    // Pending routine-frame activity flushes regardless of trace mode so
    // aggregated repaints never die with the loop.
    for line in state.render_frames.take_end_of_run(Instant::now()) {
        log_json_at(&log_path, line);
    }
    for hook in hooks {
        unsafe {
            UnhookWinEvent(hook);
        }
    }
    // Render admission closes first: invalidate every carrier epoch (a hide
    // on each carrier bumps the shared safety epoch and drops queued Shows)
    // and signal the renderer to stop, before the input stop wait. No stale
    // show can present during the potentially 5s input shutdown, and the
    // renderer observes stop promptly through its work wake.
    if let Some(bridge) = state.render_bridge.as_mut() {
        bridge.admission_open = false;
        for carrier in RenderCarrier::all() {
            let index = carrier.index();
            if !bridge.hide_posted[index] {
                bridge.gens[index] += 1;
                let generation = bridge.gens[index];
                bridge.channels.hide(carrier, generation);
                bridge.hide_posted[index] = true;
            }
        }
        let _ = bridge.handle.as_mut().map(|handle| handle.request_stop());
    }
    // Product path graceful stop: close admission (owner stops pushing),
    // keep consumed pairs handled until the hook release, unhook on the
    // creator pump, and ack with a bounded wait. A missing ack returns an
    // unclean report without claiming clean and retains `Stopping` so no
    // overlapping worker can start; the existing recovery lifecycle is
    // untouched. Never waits indefinitely or holds owner locks while joining.
    // If this state is dropped after a timeout, the process-level reservation
    // still prevents a replacement pump while the old thread may be running.
    let mut input_stop_clean = true;
    if let Some(bridge) = state.input_bridge.as_mut() {
        let _ = bridge.handle.request_stop();
        // The loop has ended so no fresh snapshots enter after stop. The pump
        // closes admission and releases hooks without dispatching queued work.
        let report = bridge.handle.wait_stopped(crate::input_pump::STOP_WAIT);
        let mut event = serde_json::json!({"event":"input-stop","clean": report.clean, "unhook_ok": report.unhook_ok});
        if !report.clean {
            event["note"] = serde_json::json!(
                "input stop unclean; process exit releases hooks; no overlapping worker started"
            );
        }
        log_json_at(&log_path, event);
        input_stop_clean = report.clean;
    }
    // Renderer graceful stop: the loop has ended so no fresh frames enter
    // after stop (admission closed and epochs invalidated above), the
    // renderer destroys every carrier on its creator thread with verified
    // readbacks, and acks with a bounded wait. A missing ack returns an
    // unclean report without claiming clean and retains `Stopping` so no
    // overlapping worker can start; the existing recovery lifecycle is
    // untouched. Published overlay identities clear only after verified
    // destruction plus exit (never on timeout), so a recycled HWND value
    // can never linger as a stale exclusion while a live carrier may still
    // own it. Never waits indefinitely or holds owner locks while joining.
    let mut render_stop_clean = true;
    if let Some(bridge) = state.render_bridge.as_mut() {
        let _ = bridge.handle.as_mut().map(|handle| handle.request_stop());
        let report = bridge
            .handle
            .as_mut()
            .map(|handle| handle.wait_stopped(RENDER_STOP_WAIT))
            .unwrap_or(crate::render_handoff::RenderStopReport {
                destroyed_ok: false,
                clean: true,
            });
        let mut event = serde_json::json!({"event":"render-stop","clean": report.clean, "destroyed_ok": report.destroyed_ok});
        if !report.clean {
            event["note"] = serde_json::json!(
                "render stop unclean; process exit destroys carriers; no overlapping worker started"
            );
        }
        log_json_at(&log_path, event);
        render_stop_clean = report.clean;
        if report.clean {
            bridge.channels.clear_overlay_ids();
        }
    }
    if let Some(mut snap) = snap_hook {
        // Honest release accounting: process exit releases the hook in any
        // case, but success is recorded, never fabricated.
        let ok = crate::snapkey::sys::uninstall(&mut snap);
        let mut release = serde_json::json!({"event":"snap-release","ok": ok});
        if !ok {
            release["note"] = serde_json::json!("release failed; process exit releases the hook");
        }
        log_json_at(&log_path, release);
    }
    if let Some(mut mouse) = windrag_hook {
        let ok = crate::win_mouse::sys::uninstall(&mut mouse);
        let mut release = serde_json::json!({"event":"windrag-release","ok": ok});
        if !ok {
            release["note"] = serde_json::json!("release failed; process exit releases the hook");
        }
        log_json_at(&log_path, release);
    }
    // Owner tray teardown: graceful stop removes the icon (`NIM_DELETE`
    // through RAII `Drop`); a crash ghost is pruned by the next startup or
    // independent restore through the stable GUID. The explicit end line
    // marks graceful stop only.
    if let Some(tray) = tray {
        drop(tray);
        log_json_at(&log_path, serde_json::json!({"event":"tray-end"}));
    }
    // Owned overlay teardown: explicit destroy on graceful stop (process exit
    // destroys it implicitly after a crash, so no residue either way). With
    // a render bridge the renderer destroyed its carriers above and the
    // `*-end` lines report the last acknowledged applied state; otherwise
    // the same-thread carriers destroy inline as before.
    let border_snapshot = if state.render_bridge.is_some() {
        render_end_snapshot(&state, RenderCarrier::Border)
    } else {
        state.border_overlay.snapshot()
    };
    state.border_overlay.destroy();
    log_json_at(
        &log_path,
        serde_json::json!({"event":"active-border-end","overlay": border_snapshot}),
    );
    let underlay_snapshot = if state.render_bridge.is_some() {
        render_end_snapshot(&state, RenderCarrier::Underlay)
    } else {
        state.underlay_overlay.snapshot()
    };
    state.underlay_overlay.destroy();
    log_json_at(
        &log_path,
        serde_json::json!({"event":"group-underlay-end","overlay": underlay_snapshot}),
    );
    // Drop-preview teardown: explicit nonactivating destroy on graceful
    // stop (process exit destroys it implicitly after a crash, so no
    // residue either way). No ledger, no registry, no hidden windows.
    let preview_snapshot = if state.render_bridge.is_some() {
        render_end_snapshot(&state, RenderCarrier::Preview)
    } else {
        state.preview_overlay.snapshot()
    };
    state.preview_overlay.destroy();
    log_json_at(
        &log_path,
        serde_json::json!({"event":"drag-preview-end","overlay": preview_snapshot}),
    );
    // Graceful stop restores only project-raised topmost bands with no frame
    // change, under the same held ownership gates as dispatch. Geometry is
    // left in place; a crash leaves every frame where it is, and restart
    // resets this runtime-local preimage with the Engine.
    let mut float_topmost_restored = 0u32;
    let mut float_topmost_skipped = 0u32;
    let float_preimages: Vec<(crate::workspace::WindowKey, bool)> = state
        .float_topmost_prev
        .iter()
        .map(|(key, prior)| (key.clone(), *prior))
        .collect();
    for (key, prior) in float_preimages {
        let Some(held) = hold_band_target(&state, me, &key) else {
            float_topmost_skipped += 1;
            continue;
        };
        let current = read_topmost_now(key.hwnd);
        if !crate::tiling::float_topmost_restore_needed(prior, current) {
            continue;
        }
        let ok = set_topmost_band(key.hwnd, prior)
            && read_topmost_now(key.hwnd) == prior
            && pid_current(key.hwnd, key.pid);
        let _ = &held;
        if state.audit_path.is_some() {
            audit_json(
                &state,
                serde_json::json!({
                    "event": "proof-topmost",
                    "op": "stop",
                    "restored": ok,
                }),
            );
        }
        if ok {
            float_topmost_restored += 1;
        } else {
            float_topmost_skipped += 1;
        }
    }
    state.float_topmost_prev.clear();
    state.floated.clear();
    state.float_rects.clear();
    if float_topmost_restored > 0 || float_topmost_skipped > 0 {
        log_json_at(
            &log_path,
            serde_json::json!({
                "event": "float-topmost-restore",
                "restored": float_topmost_restored,
                "skipped": float_topmost_skipped,
            }),
        );
    }
    log_json_at(
        &log_path,
        serde_json::json!({"event":"tile-end","ticks":state.tick}),
    );
    // Truthful stop accounting: a missing input-pump or renderer
    // acknowledgement never claims clean. Teardown above still ran (tray,
    // overlays, restore), so this only changes the reported outcome, never
    // the recovery lifecycle.
    if !input_stop_clean {
        return Err(err(
            "error: input pump stop unacknowledged; hooks released by process exit",
        ));
    }
    if !render_stop_clean {
        return Err(err(
            "error: render thread stop unacknowledged; carriers destroyed by process exit",
        ));
    }
    result
}

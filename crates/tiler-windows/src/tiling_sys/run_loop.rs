#![forbid(unsafe_code)]
use super::*;
use tiler_core::ids::{GenerationId, OwnerId};

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
        mut pager_highlight,
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
        pager_highlight = resolved.options.pager_highlight;
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
    // Taskbar pager starts from the last-good saved opt-out (default on);
    // live edits hot-apply later through the settings lane.
    let startup_pager = settings_live
        .as_ref()
        .map(|live| live.settings.core.taskbar_pager)
        .unwrap_or(true);
    // Item 12 stage B: the saved scope mode adopts only on owner restart.
    // Computed before the owner moves the live settings in.
    let startup_scope = crate::settings::startup_mode(settings_live.as_ref());
    let mut state = TileLoop {
        world: world_owner::WorldOwner::bootstrap_ids(owner.as_str(), generation.as_str())
            .expect("world owner bootstrap"),
        tokens: TokenMap::default(),
        stable: HashMap::new(),
        managed: HashSet::new(),
        active: HashSet::new(),
        gesture_before: HashMap::new(),
        gesture_end_cursor: HashMap::new(),
        gesture_override: None,
        gesture_start_key: HashMap::new(),
        gesture_start_tag: HashMap::new(),
        gesture_seq: HashMap::new(),
        next_gesture_seq: 0,
        tick: 0,
        allowlist,
        scope,
        scope_hosts,
        trace,
        suspended: false,
        suspended_limited: false,
        log_path: log_path.clone(),
        audit_path: audit_path.clone(),
        keyboard,
        inner_gap,
        outer_gap,
        settings_live,
        settings_dir,
        last_remap,
        last_disabled,
        scope_adopted: startup_scope,
        cli_overrides,
        snap_dropped: 0,
        cb_diag_dropped: 0,
        cb_diag_filtered: 0,
        cb_outbox_dropped: 0,
        mask_sends: 0,
        mask_send_max_us: 0,
        snap_origins: HashMap::new(),
        snap_advance: None,
        resize_repeat: None,
        last_enumerated: 0,
        last_native_order: Vec::new(),
        applied_homing: HashMap::new(),
        applied_views: HashMap::new(),
        output_tokens: HashMap::new(),
        member_tokens: std::collections::BTreeMap::new(),
        member_rects: HashMap::new(),
        hidden_claims: std::collections::BTreeMap::new(),
        member_identity: std::collections::BTreeMap::new(),
        member_tags: std::collections::BTreeMap::new(),
        active_output: String::new(),
        last_genuine_output: None,
        taskbar_pager: startup_pager,
        pager_overlays: None,
        pager_clicks: Arc::new(Mutex::new(None)),
        pager_served: Vec::new(),
        pager_retry_at: None,
        pager_taskbar_dirty: false,
        pager_hook_pid: None,
        pager_areas_fp: 0,
        pager_primary: None,
        pager_secondaries: Vec::new(),
        pager_icon_cache: crate::pager_icons::PagerIconCache::new(),
        pager_icon_service: None,
        pager_icon_retry_at: None,
        pager_suppress_last: None,
        pager_gate_last: None,
        pager_refusals: HashMap::new(),
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
        hint_last_good: HashMap::new(),
        hint_fallback_uses: HashMap::new(),
        diag_pending: HashMap::new(),
        diag_emitted: HashMap::new(),
        diag_class: HashMap::new(),
        diag_emit_seq: HashMap::new(),
        diag_coalesced: HashSet::new(),
        vis_diag: VisibilityDiagnostics::default(),
        suspend_started: None,
        restore_wake: None,
        border,
        pager_highlight,
        border_overlay: BorderOverlay::default(),
        border_last: None,
        accent_seen: 0,
        accent_shared: crate::active_border::AccentCache::default(),
        accent_owner: crate::ffi::OwnerAccentSource::new(),
        underlay,
        underlay_overlay: UnderlayOverlay::default(),
        underlay_last: None,
        move_kind: HashMap::new(),
        gesture_start_cursor: HashMap::new(),
        gesture_started_sent: HashSet::new(),
        preview_overlay: PreviewOverlay::default(),
        preview_last: None,
        preview_bound: HashMap::new(),
        gesture_preview_start: HashMap::new(),
        preview_dead: HashSet::new(),
        esc_latched: HashSet::new(),
        gesture_esc_seq: HashMap::new(),
        gesture_end_seq: HashMap::new(),
        // Baseline from the physical levels at startup: a chord already
        // held across owner start must not count as a fresh transition on
        // the first wake (event-driven sampling has no 100ms poll to
        // absorb it).
        underlay_chord_last: {
            let (win_held, shift_held) = chord_held_now();
            tiler_core::visual::group_underlay_chord_held(win_held, shift_held)
        },
        float_topmost_prev: std::collections::BTreeMap::new(),
        float_rects: HashMap::new(),
        floated: BTreeSet::new(),
        sticky: std::collections::BTreeMap::new(),
        restart_reject_hold: BTreeSet::new(),
        fixed_tile_overrides: BTreeSet::new(),
        fixed_origin_overrides: BTreeSet::new(),
        fixed_auto_seen: BTreeSet::new(),
        born_first_exit: BTreeSet::new(),
        overlay_now: HashMap::new(),
        input_bridge: None,
        render_bridge: None,
        cursor_wake: super::cursor_wake::CursorWake::new(),
        render_frames: RenderFrameHealth::default(),
    };
    state.workspace_proof = workspace_proof;
    // Fixed-size admission, capacity anchor/no-space, default tiling, and the
    // workspace scope mode are core ingress now: every world report carries
    // them from the last-good live settings (proof owners keep defaults), so
    // no adapter Engine configuration exists to set. The core adopts them for
    // subsequent admissions only, never reclassifying retained state.
    let startup_default = state
        .settings_live
        .as_ref()
        .map(|live| live.settings.core.workspace.default_tiled)
        .unwrap_or(true);
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
    // report drift without adopting. The core reads the mode from every
    // report ingress.
    let startup_scope = state.scope_adopted;
    log_json_at(
        &log_path,
        serde_json::json!({
            "event": "scope-mode",
            "mode": tiler_core::workspace_scope::mode_token(startup_scope),
            "outcome": "adopted",
            "scope": "startup",
        }),
    );
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
    // Per-output-local workspaces start from live monitors: the mechanical
    // device-to-wire map assigns stable wire ids, and the core mints its
    // logical rings/views from the first report screens. The cached output
    // context primes below; trailing lifecycle is core-owned.
    sync_monitor_outputs(&mut state, &areas);
    if state.active_output.is_empty()
        && let Some(first) = state.known_outputs.first().cloned()
    {
        state.active_output = first;
    }
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
    // The `extern` trampoline lives in `crate::ffi`; the safe sink below
    // carries the unchanged hook-queue policy. Guards unhook on drop, so a
    // failed install cleans up simply by returning.
    crate::ffi::set_win_event_sink(std::sync::Arc::new(LoopEventSink));
    let hooks = [
        (EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND),
        (EVENT_SYSTEM_MOVESIZESTART, EVENT_SYSTEM_MOVESIZEEND),
        (
            EVENT_OBJECT_CREATE,
            EVENT_OBJECT_SHOW
                .max(EVENT_OBJECT_DESTROY)
                .max(EVENT_OBJECT_HIDE),
        ),
        (EVENT_OBJECT_LOCATIONCHANGE, EVENT_OBJECT_LOCATIONCHANGE),
    ]
    .into_iter()
    .map(|(min, max)| crate::ffi::install_win_event_hook(min, max))
    .collect::<Option<Vec<_>>>();
    let Some(hooks) = hooks else {
        return Err(err("error: SetWinEventHook failed"));
    };
    // Pid-scoped taskbar hooks (normal owner only; proof owners stay
    // deterministic with no hooks beyond the global set above): the OS
    // delivers only the live Explorer identity here, so game and app
    // windows never reach the taskbar queue. The owner drain then validates
    // each edge against the taskbar window subtree, so ordinary File
    // Explorer traffic never invalidates anchors. The scope refreshes on
    // the rediscovery broadcast below; install failure degrades to
    // broadcast/topology invalidation.
    let initial_scope = if proof {
        None
    } else {
        crate::pager_sys::primary_taskbar_pid()
    };
    if !proof {
        state.pager_hook_pid = initial_scope;
    }
    let mut taskbar_hooks = if proof {
        Vec::new()
    } else {
        super::pager_host::install_taskbar_hooks(initial_scope)
    };
    // Any missing band (failed or partial install, partial sets never stand)
    // logs once here; the next broadcast retries a coherent full scope.
    if !proof
        && initial_scope.is_some()
        && taskbar_hooks.len() < super::pager_host::taskbar_hook_ranges().len()
    {
        log_json_at(
            &log_path,
            serde_json::json!({"event":"taskbar-hook","outcome":"unavailable"}),
        );
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
                // Startup seeding is chord-driven (unknown cached facts): the
                // gaming worker below refreshes on its own pass and the pump
                // re-evaluates with cached facts from the first intake.
                crate::gaming::tray_warning_conflicts(
                    &live.settings,
                    state.keyboard.takeover,
                    state.keyboard.allow_win_l,
                    &crate::gaming::GamingSnapshot::unknown(),
                )
                .is_empty(),
                live.status.clone(),
            ),
            None => (true, "saved:unknown".to_owned()),
        };
        // Session-only workspace toggle: the menu carries its rendered
        // opaque scope, the owner loop drains it once on its pump via
        // `take_toggle_request` and verifies it below. The pager select
        // carries its rendered output+workspace the same way (drained once
        // via `take_select_request` by the tick poll). Default picks persist
        // through the settings file (normal owner).
        let toggle = Arc::new(Mutex::new(None::<crate::tray::ToggleScope>));
        let select = Arc::new(Mutex::new(None::<crate::tray_pager::SelectScope>));
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
            &select,
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
    // Read-only gaming detection (normal owner only; proof owners stay
    // deterministic with no worker thread): cached Xbox/Game Bar facts seed
    // one startup pass on a detached worker (at most one in flight, never
    // waited on), then re-arm on settings changes plus a bounded interval
    // inside `pump_intake`. Completed lines queue in the monitor and drain
    // onto the owner log on the pump below (the worker never writes the
    // log); probe/spawn failures keep conservative unknown facts and never
    // stop tiling. Only categorical source/outcome/state reaches the log,
    // never raw values or inventories.
    let mut gaming: Option<crate::gaming::GamingMonitor> = if proof {
        None
    } else {
        let monitor = crate::gaming::GamingMonitor::new(std::sync::Arc::new(
            crate::tray_sys::gaming::probe_all,
        ));
        monitor.request_refresh(crate::gaming::RefreshReason::Startup);
        monitor.sync_revision(
            state
                .settings_live
                .as_ref()
                .map(|live| live.settings.revision),
        );
        Some(monitor)
    };
    // Product keyboard takeover on the loop thread: the callback runs during
    // pump_wait on this same thread, so install/uninstall bracket the loop
    // with no join. Plain proof mode never installs a hook; shortcut-proof
    // installs with test-only marker acceptance. Install is best-effort with
    // bounded backoff retries below: failure degrades to keyboard-unavailable
    // (nothing consumes) while tiling continues, never a refused run.
    //
    // Product `tile` runs the keyboard hook on one dedicated input pump
    // instead (stage 1): the callback never runs on this owner thread, so
    // owner observation, Engine, actuation and I/O cannot delay delivery.
    // Proof commands keep the legacy same-thread hook below untouched.
    let use_input_thread = !proof && !shortcut_proof && !workspace_proof;
    let mut snap_hook = None;
    let mut snap_failures: u32 = 0;
    let mut snap_retry_at: Option<Instant> = None;
    // Dedicated input pump for the product path: the LL keyboard hook
    // plus its synchronous classifier on one thread. Readiness uses a
    // bounded wait; install failures retry on the pump with the same
    // 5s-doubling/60s-cap vocabulary and the loop keeps tiling degraded
    // meanwhile. A spawn failure degrades the same way (no hooks, tiling
    // continues).
    if use_input_thread {
        match InputHandle::spawn(state.keyboard) {
            Ok(mut handle) => {
                let ready = handle.wait_ready(READY_WAIT, &mut |event: &FailEvent| {
                    log_json_at(
                        &log_path,
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
    // Event-driven file wakes: one directory watch over the ledger dir
    // (stop plus workspace/resize requests) and one over the settings dir,
    // deduped when both name the same directory. The files themselves are
    // still polled and validated by the existing paths; these handles only
    // wake the wait. Setup failure degrades to a bounded poll with an
    // explicit reason (never a wedge, never a spin); setup retries on
    // every degraded wake.
    //
    // The ledger watch carries file-name events only: every request file
    // is published atomically, so a signaled rename always exposes
    // complete content, while the owner's own per-turn trace-log appends
    // (content-only) never wake it. When the settings dir is the ledger
    // dir, the ledger watch keeps the full mask instead: external
    // in-place settings edits have no rename to catch, and that shared
    // configuration accepts the log self-wakes to keep them prompt.
    let settings_same_dir = state.settings_dir.as_ref().is_some_and(|d| d == dir);
    let mut ledger_watch = if settings_same_dir {
        DirWatch::watched(dir)
    } else {
        DirWatch::watched_names(dir)
    };
    if ledger_watch.is_none() {
        log_json_at(
            &log_path,
            serde_json::json!({"event":"dir-watch","dir":"ledger","outcome":"degraded-poll","retry_ms": DEGRADED_POLL_MS}),
        );
    }
    let settings_same_dir = state.settings_dir.as_ref().is_some_and(|d| d == dir);
    let mut settings_watch = if state.settings_dir.is_none() || settings_same_dir {
        if settings_same_dir {
            log_json_at(
                &log_path,
                serde_json::json!({"event":"dir-watch","dir":"settings","outcome":"deduped-ledger"}),
            );
        }
        None
    } else {
        let watched = state
            .settings_dir
            .as_ref()
            .and_then(|d| DirWatch::watched(d));
        if watched.is_none() {
            log_json_at(
                &log_path,
                serde_json::json!({"event":"dir-watch","dir":"settings","outcome":"degraded-poll","retry_ms": DEGRADED_POLL_MS}),
            );
        }
        watched
    };
    // Async gaming completion wake: the worker signals this auto-reset
    // event after every pass (same pattern as the input/render wakes), so
    // queued categorical lines drain promptly instead of stranding behind
    // an idle wait. Bursts coalesce; the interval deadline below still
    // bounds the Settings-made external-change cadence. The closure owns a
    // shared clone of the event: the detached worker may complete after
    // owner teardown, and the handle must stay valid until the last clone
    // drops (never a use-after-close on a reused HANDLE value). Shutdown
    // also detaches the hint below.
    let gaming_wake: Option<Arc<OwnedHandle>> = crate::ffi::create_auto_reset_event().map(Arc::new);
    if let (Some(monitor), Some(wake)) = (gaming.as_ref(), gaming_wake.as_ref()) {
        let owned = Arc::clone(wake);
        monitor.set_completion_notify(Some(Arc::new(move || {
            crate::ffi::signal_event(&owned);
        })));
    } else if gaming.is_some() {
        log_json_at(
            &log_path,
            serde_json::json!({"event":"gaming-wake","outcome":"degraded-interval-only"}),
        );
    }
    // Windowless monitor fallback cadence (tray-less runs only): display
    // broadcasts flag a monitor re-read through the owner windows, so only
    // runs with no top-level window keep a bounded monitor poll.
    let mut monitor_last = Instant::now();
    // Trace-only pump-waits reporting throttle (trace runs only): elapsed
    // time only, never a wait deadline. A due throttle emits on the next
    // naturally occurring owner turn; quiet runs flush one final read-only
    // summary at clean loop exit instead of waking for it.
    let mut trace_last = Instant::now();
    // Trace-only health window (input ages plus owner work):
    // plain local, never shared across threads. Each loop iteration lends
    // it to its `OwnerWorkGuard` for the turn; the pending end-of-run
    // summary flushes after the loop below. Untouched (and unborrowed)
    // when tracing is off, so default runs pay nothing here.
    let mut health = Health::new();
    // Trace-only outer-wait return counts (outer product wait only;
    // focus/proof `pump_wait` calls never touch these). `waits` counts
    // outer returns, `ack_only` the ack-only subset that returns early
    // before maintenance, and the reason hits count `WaitWake` flags
    // per return (combinations possible: ack plus messages is common, so
    // reason hits can exceed `waits`). Native wait returns only, never
    // thread context switches. Counted only when tracing so default runs
    // pay no counter work. Declared outside the loop closure so the
    // clean-exit flush below can report the accumulated split even for
    // runs that never saw a monitor event.
    let mut ow_counts = OwnerWaitCounts::default();
    let result = (|| -> Result<()> {
        let fulls = monitor_fulls(&areas);
        // Mouse prevention is deferred until an actual active tick or resume:
        // an initial fullscreen must not disable only to immediately restore.
        // The first active tick, resume, or work-area change drives the
        // effect; teardown and crash restore keep the lease-held guarantee.
        let mut snap_primed = false;
        // Fullscreen guard applies before the initial tick as well.
        // Verified managed overlays bypass like a managed maximize. A
        // limited unmanageable fullscreen still reconciles other windows
        // (foreground excluded per write, focus off, no admission or
        // un-maximize) while overlays stay hidden.
        let (initial_suspend, initial_fg) = suspend_limited(&mut state, me, &fulls);
        let initial_limited = is_limited_fullscreen(&initial_suspend) && initial_fg != 0;
        if initial_suspend.veto.block {
            state.suspended = true;
            state.suspended_limited = initial_limited;
            state.snap_advance = None;
            state.suspend_started = Some(Instant::now());
            log_json_at(
                &log_path,
                serde_json::json!({"event":"suspend","tick":state.tick,"cause":"fullscreen-foreground","mode":if initial_limited {"limited"} else {"full"}}),
            );
            if snap_want {
                snap_suspend(dir, me, store);
            }
            if initial_limited {
                reconcile_limited_tick(&mut state, me, store, dir, &fulls, &areas, initial_fg);
            }
            hide_border(&mut state, "suspended");
            hide_underlay(&mut state, "suspended");
            hide_preview(&mut state, "suspended");
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
            reconcile_tick(&mut state, me, store, dir, &fulls, &areas);
            refresh_active_border(&mut state, me, &fulls);
            refresh_group_underlay(&mut state, me, store, dir, &fulls, &areas);
        }
        // Trace-only owner-work sequence: counted only when tracing so
        // default runs pay no timer/counter work.
        let mut owner_seq: u64 = 0;
        // Last observed pump turn/offer counts for change-only input
        // observations: a fresh turn (or batch/drop) samples the slot
        // once; an unchanged diag is never resampled. No per-turn file
        // writes: observations aggregate, only throttled outliers and
        // window summaries log.
        let mut trace_last_turns: u64 = 0;
        let mut trace_last_offers: u64 = 0;
        loop {
            // Single clean-exit attribution point: the loop historically
            // returned identically for all three causes, so a clean
            // `run-end stopped` could not name its initiator. Evaluate the
            // same three predicates in the same order and log the winning
            // cause (`stop-file` exact-owner request, `tray-stop` tray Stop
            // pick, `deadline` bounded-run expiry) before returning. The
            // cause token carries no paths, ids, or window content.
            let file_stop = stop_requested(dir, me)?;
            let tray_stop = tray.as_ref().is_some_and(|t| t.stop_requested());
            let expired = deadline.is_some_and(|end| Instant::now() >= end);
            if let Some(cause) = loop_exit_cause(file_stop, tray_stop, expired) {
                log_json_at(
                    &log_path,
                    serde_json::json!({"event":"loop-exit","cause": cause}),
                );
                return Ok(());
            }
            // Same-thread pre-drain: hook callbacks, tray picks, and overlay
            // clicks dispatched here stage work on this thread. Anything
            // staged skips the block below with a zero-timeout collect, so
            // no same-thread wake ever strands behind an idle wait. The
            // blocking wait itself stays atomic (queue plus handles), so no
            // cross-thread wake is lost either.
            let pre_messages = drain_owner_messages();
            let staged = pre_messages || hook_edges_pending() || tray_staged_pending(&state, &tray);
            // Event-driven wait: infinite at healthy idle; otherwise the
            // minimum over the armed deadlines (retries, restore, gaming,
            // monitor fallback, bounded run, open-gesture sampling,
            // degraded watch poll). Staged same-thread work skips deadline
            // selection entirely with a zero request, as before. Trace
            // reporting never arms the wait: its throttle rides naturally
            // occurring turns only. Extra handles carry the directory
            // watches plus the async completion wakes.
            //
            // Single deadline scan: the non-staged branch selects once and
            // reuses the cause/duration for trace bookkeeping. Diagnostic
            // construction is trace-gated; non-trace runs pay no selection
            // beyond the wait itself.
            let mut wait_diag: Option<OwnerWaitDiag> = None;
            let timeout = if staged {
                if state.trace {
                    wait_diag = Some(OwnerWaitDiag {
                        staged: true,
                        source: OwnerWaitSource::Staged,
                        requested_ms: 0,
                        elapsed_ms: 0,
                        already_due: false,
                    });
                }
                0
            } else {
                let deadlines = OwnerDeadlines {
                    restore_until: state.restore_wake.as_ref().map(|wake| wake.until),
                    pending_until: state
                        .render_bridge
                        .as_ref()
                        .and_then(|bridge| bridge.pending.as_ref())
                        .map(|pending| pending.deadline),
                    pager_maintenance_at: if state.pager_suppress_last.is_none() {
                        state
                            .pager_overlays
                            .as_ref()
                            .and_then(|overlays| overlays.maintenance_due_at())
                    } else {
                        None
                    },
                    snap_retry_at,
                    pager_retry_at: state.pager_retry_at,
                    pager_icon_retry_at: state.pager_icon_retry_at,
                    gaming_due_at: gaming.as_ref().and_then(|m| m.interval_due_at()),
                    monitor_due_at: if tray.is_none() {
                        monitor_last.checked_add(Duration::from_secs(MONITOR_FALLBACK_SECS))
                    } else {
                        None
                    },
                    run_end: deadline,
                    gesture_open: gesture_sampling_open(&state),
                    degraded_poll: ledger_watch.is_none()
                        || (state.settings_dir.is_some()
                            && !settings_same_dir
                            && settings_watch.is_none()),
                };
                let sel = owner_deadline_source(Instant::now(), &deadlines);
                if state.trace {
                    wait_diag = Some(OwnerWaitDiag {
                        staged: false,
                        source: sel.source,
                        requested_ms: sel.requested_ms,
                        elapsed_ms: 0,
                        already_due: sel.already_due,
                    });
                }
                sel.requested_ms
            };
            let mut extras = [std::ptr::null_mut(); 4];
            let mut extra_count: usize = 0;
            if let Some(watch) = ledger_watch.as_ref() {
                extras[extra_count] = watch.raw();
                extra_count += 1;
            }
            if let Some(watch) = settings_watch.as_ref() {
                extras[extra_count] = watch.raw();
                extra_count += 1;
            }
            if let Some(handle) = state
                .pager_icon_service
                .as_ref()
                .and_then(|service| service.completion_handle())
            {
                extras[extra_count] = handle.as_handle().as_raw_handle();
                extra_count += 1;
            }
            if extra_count < extras.len()
                && let Some(wake) = gaming_wake.as_ref()
            {
                extras[extra_count] = wake.as_handle().as_raw_handle();
                extra_count += 1;
            }
            // Trace-only wait-duration sample for early-vs-genuine timeout
            // attribution: two clock reads around the blocking wait, skipped
            // entirely when tracing is off.
            let wait_t0 = if state.trace {
                Some(Instant::now())
            } else {
                None
            };
            let mut wait = pump_wait_with_intent(
                timeout,
                state
                    .input_bridge
                    .as_ref()
                    .and_then(|bridge| bridge.channels.intent_wake()),
                state
                    .render_bridge
                    .as_ref()
                    .and_then(|bridge| bridge.handle.as_ref())
                    .map(|handle| handle.ack_wake()),
                &extras[..extra_count],
            );
            // Pre-drain traffic counts as a message wake: it was dispatched,
            // so this turn does full owner work like the post-drain below.
            wait.message |= pre_messages;
            let wait_elapsed_ms: u64 = wait_t0
                .map(|t0| t0.elapsed().as_millis().min(u128::from(u64::MAX)) as u64)
                .unwrap_or(0);
            // Race-safe rearm BEFORE any file inspection: changes concurrent
            // with the inspection below re-signal and return the next wait
            // immediately instead of being lost. A broken watch degrades to
            // the bounded poll (logged once per transition); setup retries
            // on every degraded turn.
            if let Some(watch) = ledger_watch.as_ref()
                && !watch.rearm()
            {
                ledger_watch = None;
                log_json_at(
                    &log_path,
                    serde_json::json!({"event":"dir-watch","dir":"ledger","outcome":"degraded-poll","retry_ms": DEGRADED_POLL_MS}),
                );
            }
            if let Some(watch) = settings_watch.as_ref()
                && !watch.rearm()
            {
                settings_watch = None;
                log_json_at(
                    &log_path,
                    serde_json::json!({"event":"dir-watch","dir":"settings","outcome":"degraded-poll","retry_ms": DEGRADED_POLL_MS}),
                );
            }
            if ledger_watch.is_none()
                && let Some(watched) = (if settings_same_dir {
                    DirWatch::watched(dir)
                } else {
                    DirWatch::watched_names(dir)
                })
            {
                ledger_watch = Some(watched);
                log_json_at(
                    &log_path,
                    serde_json::json!({"event":"dir-watch","dir":"ledger","outcome":"armed"}),
                );
            }
            if settings_watch.is_none()
                && !settings_same_dir
                && let Some(settings_dir) = state.settings_dir.clone()
                && let Some(watched) = DirWatch::watched(&settings_dir)
            {
                settings_watch = Some(watched);
                log_json_at(
                    &log_path,
                    serde_json::json!({"event":"dir-watch","dir":"settings","outcome":"armed"}),
                );
            }
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
                if let Some(mut diag) = wait_diag {
                    diag.elapsed_ms = wait_elapsed_ms;
                    ow_counts.note_wait(wait, diag);
                }
            }
            let drain = drain_hook_edges(&mut state, me, &log_path);
            let mut woke = drain.woke;
            // Trace-only batch breakdown for the owner-work sample; default
            // runs skip the counting.
            let ow_batch_total: u64 = drain.batch_total;
            let ow_wakes: u64 = drain.wakes;
            let ow_starts: u64 = drain.starts;
            let ow_ends: u64 = drain.ends;
            // Ack accounting runs on every turn (cheap: three mutex takes
            // plus the bounded failure drain), then the pending finish is
            // serviced from the ack state with no blocking wait.
            drain_render_failures(&mut state);
            poll_render_acks(&mut state, Instant::now());
            service_pending_drop(&mut state, me, store, dir, &fulls, &areas);
            // Accent-only wake: a Personalization broadcast reached a
            // renderer or loop-thread window while the owner slept (no
            // ack payload, no input, no message on this thread). Promote
            // to a full refresh turn so `refresh_active_border` reposts
            // from fresh observation with all foreground/game safety
            // fences intact; an unchanged version leaves ack-only turns
            // untouched. Never fakes an ack, never replays a stale frame.
            if crate::active_border::accent_version_moved(
                &mut state.accent_seen,
                crate::ffi::accent_version(),
            ) {
                woke = true;
            }
            // Ack-only turns (renderer ack with no input, message,
            // notification, timeout, or hook-callback event) loop here without
            // reposting frames, reconciling, polling settings, or touching
            // input: an ack alone never triggers owner work, so the ack wake
            // cannot feed back into a post/ack/post loop. Hook events above
            // force a full turn, so no input starves; armed deadlines wake
            // with a genuine timeout, and the pending finish above still
            // completes with no added latency.
            if !(wait.full_turn() || woke) {
                if state.trace {
                    ow_counts.note_ack_only();
                }
                continue;
            }
            let now = Instant::now();
            // Monitor re-read rides the display-broadcast dirty flag plus
            // the windowless fallback below; hook/intent/file wakes cannot
            // change topology, so they skip the enumeration. Trace
            // reporting rides no deadline of its own: past this point
            // every iteration is a natural full turn (ack-only already
            // continued above), so a due throttle emits on whatever work
            // woke the loop, independent of any monitor check.
            let monitor_check = take_monitor_dirty()
                || (tray.is_none()
                    && now.duration_since(monitor_last).as_secs() >= MONITOR_FALLBACK_SECS);
            if monitor_check {
                monitor_last = now;
            }
            let trace_emit =
                state.trace && now.duration_since(trace_last).as_secs() >= TRACE_REPORT_SECS;
            if trace_emit {
                trace_last = now;
            }
            // Single consumption point for the native rediscovery
            // broadcast: the pager sync folds it into the stale-anchor
            // flag, and the scope refresh below re-resolves Explorer
            // identity on this same turn. The durable tray owner forwards
            // its own `TaskbarCreated` here too, so this fires even with
            // no pager windows alive to receive the broadcast.
            let primary_had = state.pager_primary.is_some();
            let rediscovered = crate::pager_sys::take_rediscover();
            let Intake {
                woke: woke_out,
                snap: snap_events,
                workspace: workspace_events,
                send: send_events,
                output: output_events,
                migrate: migrate_events,
                history: history_events,
            } = pump_intake(
                &mut state,
                me,
                dir,
                store,
                &fulls,
                &areas,
                &log_path,
                &mut owner_work,
                now,
                use_input_thread,
                proof,
                shortcut_proof,
                workspace_proof,
                &mut snap_hook,
                &mut snap_failures,
                &mut snap_retry_at,
                &mut snap_want,
                &mut tray,
                &mut gaming,
                &mut trace_last_turns,
                &mut trace_last_offers,
                woke,
                rediscovered,
            );
            woke = woke_out;
            // Pre-tick suspend value: the tick below owns the
            // suspend/resume transition, observed after the call.
            let was_suspended = state.suspended;
            let tick_flow = run_active_tick(
                &mut state,
                me,
                dir,
                store,
                &mut areas,
                &mut monitor,
                &mut monitor_count,
                now,
                monitor_check,
                trace_emit,
                woke,
                &log_path,
                &mut owner_work,
                &ow_counts,
                snap_want,
                &mut snap_primed,
                &mut snap_retry_at,
                ow_batch_total,
                ow_wakes,
                ow_starts,
                ow_ends,
                snap_events,
                workspace_events,
                send_events,
                output_events,
                migrate_events,
                history_events,
                &mut tray,
            )?;
            // Taskbar-scope management on the loop thread (normal owner;
            // proof installs no scope), after the tick owns the
            // suspend/resume transition: while suspended the scope drops, so
            // a suspended game owns no scoped callbacks and no queue growth
            // (the cached pid is retained, never re-resolved). On broadcast
            // or on the resume transition, re-resolve the live pid and
            // reinstall when the identity moved or the installed set is
            // incomplete; a refused install degrades to broadcast/topology
            // invalidation until the next broadcast or resume retries it.
            // Runs before the flow dispatch so every flow reaches it.
            let primary_appeared = !primary_had && state.pager_primary.is_some();
            if !proof {
                if state.suspended {
                    if !taskbar_hooks.is_empty() {
                        taskbar_hooks.clear();
                    }
                } else if rediscovered || was_suspended || primary_appeared {
                    // Late primary appearance (broadcast consumed while
                    // Explorer was gone) refreshes through the existing
                    // `pager-taskbar` appeared transition above: no polling,
                    // no timer, and no extra traffic while suspended.
                    let old_present = state.pager_hook_pid.is_some();
                    let current = crate::pager_sys::primary_taskbar_pid();
                    if super::pager_host::taskbar_reinstall_due(
                        state.pager_hook_pid,
                        current,
                        taskbar_hooks.len(),
                    ) {
                        state.pager_hook_pid = current;
                        taskbar_hooks = super::pager_host::install_taskbar_hooks(current);
                        let installed = taskbar_hooks.len();
                        let full = installed >= super::pager_host::taskbar_hook_ranges().len();
                        let outcome = if current.is_none() {
                            "cleared"
                        } else if full {
                            "reinstalled"
                        } else {
                            "unavailable"
                        };
                        log_json_at(
                            &log_path,
                            serde_json::json!({
                                "event": "taskbar-hook",
                                "outcome": outcome,
                                "old_present": old_present,
                                "new_present": current.is_some(),
                                "installed": installed,
                            }),
                        );
                    } else if rediscovered || primary_appeared {
                        log_json_at(
                            &log_path,
                            serde_json::json!({
                                "event": "taskbar-hook",
                                "outcome": "unchanged",
                                "old_present": old_present,
                                "new_present": current.is_some(),
                                "installed": taskbar_hooks.len(),
                            }),
                        );
                    }
                }
            }
            match tick_flow {
                TickFlow::Done => {}
                TickFlow::Idle | TickFlow::Suspended | TickFlow::Paused => continue,
            }
        }
    })();
    // Detach the gaming completion hint now that the wait set is gone: a
    // delayed worker pass must not signal the torn-down event. The closure
    // owns a shared clone, so even a pass already past the detach still
    // signals a live handle harmlessly (never a use-after-close).
    if let Some(monitor) = gaming.as_ref() {
        monitor.set_completion_notify(None);
    }
    // Pending end-of-run health summary (trace-only, at most one marked
    // line per stream): retained activity below the window age still
    // reports instead of dying with the loop.
    if state.trace {
        for line in health.take_end_of_run(Instant::now()) {
            log_json_at(&log_path, line);
        }
        // Final accumulated outer-wait split (trace-only, read-only): the
        // counters rode every natural turn above, so even a run that never
        // saw a monitor event leaves one measurable line. Snapshot reads
        // only (one diag-slot lock plus renderer atomics/ack seq); no
        // drain, no wake, no actuation. Runs after stop/file-request and
        // bounded-run exits alike, before any teardown waits.
        let (input, render) = pump_waits_snapshots(&state);
        log_json_at(
            &log_path,
            pump_waits_line(&ow_counts, input, render, Some("end-of-run")),
        );
    }
    // Pending routine-frame activity flushes regardless of trace mode so
    // aggregated repaints never die with the loop.
    for line in state.render_frames.take_end_of_run(Instant::now()) {
        log_json_at(&log_path, line);
    }
    drop(hooks);
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
        // Final per-query drain: presents acked before the stop wait may
        // hold unpolled diagnostics, and their acks are never polled again.
        // The queue preserves every completed query; the ack slot cannot.
        super::render_ack::drain_accent_queries(&mut state);
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
    // Cursor-motion tap teardown: physically off at graceful stop (process
    // exit releases it in any case). `Drop` on the state is the backstop
    // for every other exit path. Never armed on proof paths.
    state.cursor_wake.shutdown(Instant::now());
    if let Some(diag) = state.cursor_wake.take_diag() {
        log_json_at(
            &log_path,
            serde_json::json!({"event":"cursor-tap","op":diag.op,"ok":diag.ok}),
        );
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
    // Same-thread carriers never query: the single shared owner resolve
    // already logged its one record per version, and cache reads never
    // fabricate a query record. Bridged owners likewise never query here.
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
    let float_preimages: Vec<(crate::native_guards::WindowKey, bool)> = state
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
    // Scope WinRT shutdown last: every resolve in this scope ran against the
    // one lazily initialized apartment, and all per-resolve `UISettings`
    // references are already dropped, so this balances exactly the one
    // `RoUninitialize` the scope owns (including S_FALSE; failed or absent
    // initialization owns nothing). The next scope in this process
    // initializes its own pair.
    state.accent_owner.shutdown();
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

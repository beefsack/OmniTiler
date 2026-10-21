#![forbid(unsafe_code)]
use super::*;

/// Win+G float toggle (KDE Meta+G parity): fresh observation, exact origin
/// re-resolution, full gates, one Engine `ToggleFloat` evaluated on a local
/// candidate that commits only after the target native effects verify, then
/// sibling reflow through the shared write path. No compensating toggle.
/// Owner-side launcher backend seam: production spawns detached processes;
/// tests inject a recorder. Launching never runs in the hook callback (the
/// callback only queues through [`SnapClassify`]); the owner spawns here on
/// the owner thread, and `Command::spawn`/`ShellExecuteW` return without
/// waiting, so the pump never blocks.
pub trait LaunchBackend {
    fn launch(&self, kind: LaunchKind) -> std::result::Result<(), String>;
}

/// Production launcher backend: default browser via `ShellExecuteW`, Windows
/// Terminal via the `wt.exe` app execution alias. Launched processes never
/// inherit owner handles and never elevate: stdio is nulled, no handle list
/// is passed, and the spawn runs as the current (normal) user token.
pub struct SystemLaunchBackend;

impl LaunchBackend for SystemLaunchBackend {
    fn launch(&self, kind: LaunchKind) -> std::result::Result<(), String> {
        match kind {
            LaunchKind::Browser => launch_default_browser(),
            LaunchKind::Terminal => launch_windows_terminal(),
        }
    }
}

/// Open the default browser on an empty `https://` URL through the shell
/// `open` verb: the shell resolves the user's default browser, never a
/// hard-coded executable.
fn launch_default_browser() -> std::result::Result<(), String> {
    #[cfg(windows)]
    {
        // SW_SHOWNORMAL = 1 inside the boundary; `Err(rc)` carries the
        // `ShellExecuteW` return, as before.
        crate::ffi::open_url("https://")
            .map_err(|rc| format!("ShellExecuteW open https:// failed: {rc}"))
    }
    #[cfg(not(windows))]
    {
        Err("default browser launch is Windows-only".to_owned())
    }
}

/// Spawn Windows Terminal through its `wt.exe` app execution alias with
/// nulled stdio and no inherited handles. A missing alias is a narrow
/// logged error, never a crash and never a fallback launch.
fn launch_windows_terminal() -> std::result::Result<(), String> {
    std::process::Command::new("wt.exe")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|err| format!("wt.exe spawn failed: {err}"))
}

/// Launcher dispatch (Win+B browser, Win+T terminal): fresh observation is
/// unnecessary (no window is touched), but the takeover and
/// suspend/elevated fences still apply, so a chord arriving while keyboard
/// takeover is off or the session is suspended/elevated settles without
/// spawning. The fence outcome is injected by the caller: the production
/// `keyboard_tick` arm passes the live `toggle_gate_outcome` read, while
/// tests pass an explicit outcome, so dispatch never depends on ambient
/// foreground state. The chord-time origin rides only for mask/queue parity
/// and is ignored: launchers act on no window. Only the down dispatches;
/// key-ups and repeats are trace-only. A failed spawn logs narrowly and keeps
/// the loop functioning.
pub(super) fn dispatch_launch_intent(
    state: &mut TileLoop,
    intent: QueuedLaunchIntent,
    backend: &dyn LaunchBackend,
    gate: Option<&'static str>,
) {
    let log_path = state.log_path.clone();
    // Key-ups close the pair and passed chords never dispatch: trace-only so
    // held-key traffic stays out of normal logs.
    if !intent.consumed || !intent.announce {
        if state.trace {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "launch",
                    "tick": state.tick,
                    "kind": intent.kind.as_str(),
                    "edge": intent.edge.as_str(),
                    "disposition": if intent.consumed { "consumed" } else { "passed" },
                    "outcome": if intent.consumed { "key-up" } else { "passed" },
                }),
            );
        }
        return;
    }
    state.tick += 1;
    let tick = state.tick;
    let correlation = state.correlation();
    let settle = |outcome: &'static str| {
        serde_json::json!({
            "event": "launch",
            "tick": tick,
            "correlation": correlation.as_str(),
            "kind": intent.kind.as_str(),
            "edge": intent.edge.as_str(),
            "disposition": "consumed",
            "outcome": outcome,
        })
    };
    // KDE `!enabled` parity: chords arriving while keyboard takeover is off
    // never dispatch (the classifier already passes them through; this
    // covers the exact-owner test-needed route, which bypasses it).
    if !state.keyboard.takeover {
        let line = settle("launch-refused-disabled");
        log_json_at(&log_path, line);
        return;
    }
    // Fresh per-intent suspension/elevation fence (workspace_tick parity),
    // injected precomputed by the caller: newly intercepted
    // suspended/elevated chords settle here with no side effects.
    if let Some(outcome) = gate {
        let line = settle(outcome);
        log_json_at(&log_path, line);
        return;
    }
    match backend.launch(intent.kind) {
        Ok(()) => {
            let line = settle("launched");
            log_json_at(&log_path, line);
        }
        Err(err) => {
            let mut line = settle("launch-failed");
            line["error"] = serde_json::Value::from(err);
            log_json_at(&log_path, line);
        }
    }
}

pub(super) fn dispatch_float_intent(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
    intent: crate::snapkey::QueuedFloatIntent,
) {
    let log_path = state.log_path.clone();
    // Key-ups close the pair and passed chords never dispatch: trace-only so
    // held-key traffic stays out of normal logs.
    if !intent.consumed || !intent.announce {
        if state.trace {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "float-toggle",
                    "tick": state.tick,
                    "edge": intent.edge.as_str(),
                    "disposition": if intent.consumed { "consumed" } else { "passed" },
                    "outcome": if intent.consumed { "key-up" } else { "passed" },
                }),
            );
        }
        return;
    }
    state.tick += 1;
    let tick = state.tick;
    let correlation = state.correlation();
    let settle = |outcome: &'static str| {
        serde_json::json!({
            "event": "float-toggle",
            "tick": tick,
            "correlation": correlation.as_str(),
            "edge": intent.edge.as_str(),
            "disposition": "consumed",
            "outcome": outcome,
        })
    };
    let Some(origin) = intent.origin.clone() else {
        state.snap_advance = None;
        let line = settle("origin-vanished");
        log_json_at(&log_path, line);
        return;
    };
    // Fresh per-intent suspension/elevation fence (workspace_tick parity):
    // newly intercepted suspended/elevated chords settle here with no side
    // effects. The suspend read exempts verified managed overlays; the target
    // revalidation below refuses protected windows.
    if let Some(outcome) = crate::tiling::toggle_gate_outcome(
        suspend_read(state, me, fulls).veto.block,
        foreground_elevated(me),
    ) {
        let mut line = settle(outcome);
        line["origin"] = serde_json::Value::from(origin.token.clone());
        log_json_at(&log_path, line);
        return;
    };
    let Some(chord) = observe_chord_intent(state, me, fulls, areas) else {
        let mut line = settle("observation-failed");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        log_json_at(&log_path, line);
        return;
    };
    let ChordObserved {
        mut observed,
        mut retained,
        mut skipped,
    } = chord;
    let ChordTarget {
        from,
        member_key,
        loc,
        ..
    } = match resolve_chord_target(state, me, areas, &origin) {
        Ok(target) => target,
        Err(reject) => {
            settle_chord_reject(&log_path, &settle, &origin, &reject);
            return;
        }
    };
    let is_float = engine_is_float(state, &loc.output, &loc.workspace, &from);
    // Overlay fences on live state: overlay targets never float, and a
    // natively overlaid float never unfloats until it reads normal again.
    // Sticky origins refuse with the sticky vocabulary (KDE parity).
    // Floating workspaces take no per-window float toggles: the domain is
    // released and every frame stays native.
    if !workspace_mode_tiled(state, &loc.output, &loc.workspace) {
        state.snap_advance = None;
        let mut line = settle("workspace-floating");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        line["window"] = serde_json::Value::from(from.clone());
        log_json_at(&log_path, line);
        return;
    }
    // Overlay fences on live state: overlay targets never float, and a
    // natively overlaid float never unfloats until it reads normal again.
    // Sticky origins refuse with the sticky vocabulary (KDE parity). B9
    // exception (R-FLT-06): an ordinary maximized float takes one
    // nonactivating clear plus observed-clear before fresh admission instead
    // of refusing; fullscreen, sticky, and tiled lanes keep refusing here.
    let is_sticky_origin = state.sticky.contains_key(&member_key);
    if let Some(refusal) =
        chord_overlay_refusal(is_sticky_origin, &observed, &retained, &member_key)
    {
        if refusal == "float-refused-maximize" && is_float && !is_sticky_origin {
            match unfloat_maximize_clear(
                state,
                &mut NativeUnfloatClear,
                me,
                fulls,
                &observed,
                &retained,
                &member_key,
                &from,
                &loc,
                &origin.token,
                &log_path,
                tick,
                &correlation,
            ) {
                Ok(fresh) => {
                    observed = fresh.observed;
                    retained = fresh.retained;
                    skipped = fresh.skipped;
                }
                Err((outcome, cause)) => {
                    state.snap_advance = None;
                    let mut line = settle(outcome);
                    line["origin"] = serde_json::Value::from(origin.token.clone());
                    line["window"] = serde_json::Value::from(from.clone());
                    if let Some(cause) = cause {
                        line["cause"] = serde_json::Value::from(cause);
                    }
                    log_json_at(&log_path, line);
                    return;
                }
            }
        } else {
            let mut line = settle(refusal);
            line["origin"] = serde_json::Value::from(origin.token.clone());
            line["window"] = serde_json::Value::from(from.clone());
            log_json_at(&log_path, line);
            return;
        }
    }
    let by_hwnd: HashMap<u64, &ObservedWindow> = observed.iter().map(|w| (w.hwnd, w)).collect();
    let live_visible = by_hwnd.get(&member_key.hwnd).map(|w| w.visible);
    // Win+G on any sticky origin clears sticky then tiles the CURRENT
    // workspace. Prior float or tiled both tile.
    if is_sticky_origin {
        sticky_off_to_current(
            state,
            me,
            store,
            dir,
            fulls,
            areas,
            &observed,
            &retained,
            &member_key,
            &from,
            &loc,
            &origin,
            &settle,
            &log_path,
            &correlation,
            StickyOffMode::Tile,
        );
        return;
    }
    // Unfloat carries the live frame rect so a moved/resized float is
    // retained; float carries no rect so the Engine picks the retained
    // placement, else the centered fallback. Frameless floats unfloat with no
    // rect and keep the Engine-retained placement.
    let live_rect = live_visible.or_else(|| {
        retained
            .iter()
            .find(|r| r.key == member_key)
            .and_then(|row| row.rect)
    });
    if is_float {
        apply_unfloat_to_tiled(
            state,
            me,
            store,
            dir,
            fulls,
            areas,
            &observed,
            &retained,
            skipped,
            &member_key,
            &from,
            &loc,
            live_rect,
            &origin,
            &settle,
            &log_path,
            tick,
            &correlation,
            "float",
            "unfloated",
            false,
        );
        return;
    }
    apply_float_from_tiled(
        state,
        me,
        store,
        dir,
        fulls,
        areas,
        &observed,
        &retained,
        skipped,
        &member_key,
        &from,
        &loc,
        &origin,
        &settle,
        &log_path,
        tick,
        &correlation,
        "float",
        "floated",
        "float-applied",
        false,
    );
}

/// Win+O orientation toggle (KDE Meta+O parity, item 4 R-LAY-01): fresh
/// observation, exact origin re-resolution, full gates, one Engine
/// `ToggleOrientation` on the retained session, then sibling reflow through
/// the shared write path. No focus or view mutation: focus stays on the same
/// window and the workspace never switches. Lone root, float/sticky
/// subjects, missing focus, floating workspaces, and focused overlays settle
/// with no writes; sibling overlays reproject as reserved slots and skip
/// native writes through the existing writable fence.
pub(super) fn dispatch_orientation_intent(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
    intent: crate::snapkey::QueuedOrientationIntent,
) {
    let log_path = state.log_path.clone();
    // Key-ups close the pair and passed chords never dispatch: trace-only so
    // held-key traffic stays out of normal logs.
    if !intent.consumed || !intent.announce {
        if state.trace {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "orientation-toggle",
                    "tick": state.tick,
                    "edge": intent.edge.as_str(),
                    "disposition": if intent.consumed { "consumed" } else { "passed" },
                    "outcome": if intent.consumed { "key-up" } else { "passed" },
                }),
            );
        }
        return;
    }
    state.tick += 1;
    let tick = state.tick;
    let correlation = state.correlation();
    let settle = |outcome: &'static str| {
        serde_json::json!({
            "event": "orientation-toggle",
            "tick": tick,
            "correlation": correlation.as_str(),
            "edge": intent.edge.as_str(),
            "disposition": "consumed",
            "outcome": outcome,
        })
    };
    let Some(origin) = intent.origin.clone() else {
        state.snap_advance = None;
        let line = settle("origin-vanished");
        log_json_at(&log_path, line);
        return;
    };
    // Fresh per-intent suspension/elevation fence (workspace_tick parity):
    // newly intercepted suspended/elevated chords settle here with no side
    // effects. The suspend read exempts verified managed overlays, and the
    // target revalidation below refuses protected windows.
    if let Some(outcome) = crate::tiling::toggle_gate_outcome(
        suspend_read(state, me, fulls).veto.block,
        foreground_elevated(me),
    ) {
        let mut line = settle(outcome);
        line["origin"] = serde_json::Value::from(origin.token.clone());
        log_json_at(&log_path, line);
        return;
    };
    let Some(chord) = observe_chord_intent(state, me, fulls, areas) else {
        let mut line = settle("observation-failed");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        log_json_at(&log_path, line);
        return;
    };
    let ChordObserved {
        observed,
        retained,
        skipped: _,
    } = chord;
    let ChordTarget {
        from,
        member_key,
        loc,
        ..
    } = match resolve_chord_target(state, me, areas, &origin) {
        Ok(target) => target,
        Err(reject) => {
            settle_chord_reject(&log_path, &settle, &origin, &reject);
            return;
        }
    };
    // Floating workspaces run no tile layout: the toggle settles with no
    // writes and every frame stays native.
    if !workspace_mode_tiled(state, &loc.output, &loc.workspace) {
        state.snap_advance = None;
        let mut line = settle("workspace-floating");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        line["window"] = serde_json::Value::from(from.clone());
        log_json_at(&log_path, line);
        return;
    }
    // Float/sticky subjects hold no tile slot: the toggle settles with no
    // writes, never retiles them.
    if state.sticky.contains_key(&member_key) {
        let mut line = settle("orientation-refused-sticky");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        line["window"] = serde_json::Value::from(from.clone());
        log_json_at(&log_path, line);
        return;
    }
    if engine_is_float(state, &loc.output, &loc.workspace, &from) {
        let mut line = settle("orientation-refused-floating");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        line["window"] = serde_json::Value::from(from.clone());
        log_json_at(&log_path, line);
        return;
    }
    // Focused overlays refuse before any Engine mutation: a maximized or
    // fullscreen focused window keeps its native state and its reserved slot.
    if let Some(refusal) = orientation_overlay_refusal(&observed, &retained, &member_key) {
        let mut line = settle(refusal);
        line["origin"] = serde_json::Value::from(origin.token.clone());
        line["window"] = serde_json::Value::from(from.clone());
        log_json_at(&log_path, line);
        return;
    }
    if !state.member_tokens.values().any(|token| token == &from) {
        let mut line = settle("unmanaged");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        log_json_at(&log_path, line);
        return;
    }
    let known_output = areas.iter().any(|a| a.device == loc.output);
    if !known_output {
        let mut line = settle("unknown-output");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        log_json_at(&log_path, line);
        return;
    };
    // World-routed toggle: the core owns orientation planning and the host
    // diff applies the accepted reply (sibling reflow included) with fresh
    // proof. No focus or view mutation. The applied reply IS the outcome:
    // changed allocations report applied, identical ones a no-op, and a
    // rejected or lost command reports unverified.
    let foreground = crate::ffi::foreground_window();
    let reply = super::world_tick::run_world_command(
        state,
        me,
        store,
        dir,
        &observed,
        &retained,
        areas,
        foreground,
        fulls,
        tiler_core::world_report::WorldCommand::Operation(CoreCommand::ToggleOrientation {
            window: from.clone(),
        }),
    );
    let outcome = match reply {
        Some(reply) => {
            let changed = reply.windows.iter().any(|row| {
                row.allocation.is_some_and(|alloc| {
                    observed
                        .iter()
                        .find(|w| w.token == row.lifetime.0)
                        .is_none_or(|w| w.visible != alloc)
                })
            });
            if changed {
                "orientation-applied"
            } else {
                "orientation-noop"
            }
        }
        None => "orientation-unverified",
    };
    let mut line = settle(outcome);
    line["origin"] = serde_json::Value::from(origin.token.clone());
    line["window"] = serde_json::Value::from(from.clone());
    log_json_at(&log_path, line);
}

/// Current-workspace tiling toggle dispatch (Win+Y, KDE Meta+Y parity, item
/// 21): the shortcut arm of the existing tray checkbox path. The toggle acts
/// on the active workspace of the active output through the shared
/// [`toggle_workspace_tiling`] (same per-workspace state, same release
/// domain, same pending-release coverage); the chord-time origin rides only
/// for mask/queue parity and is ignored, so no focus or window policy is
/// added. Only the down dispatches; key-ups and repeats are trace-only.
/// `gate` is the live suspend/elevated fence outcome injected by the
/// `keyboard_tick` call site (see [`dispatch_launch_intent`]), so dispatch
/// never depends on ambient foreground state and owner-dispatch tests stay
/// deterministic on headless runners.
pub(super) fn dispatch_workspace_tiling_intent(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
    intent: crate::snapkey::QueuedWorkspaceTilingIntent,
    gate: Option<&'static str>,
) {
    let log_path = state.log_path.clone();
    // Key-ups close the pair and passed chords never dispatch: trace-only so
    // held-key traffic stays out of normal logs.
    if !intent.consumed || !intent.announce {
        if state.trace {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "workspace-tiling-toggle",
                    "tick": state.tick,
                    "edge": intent.edge.as_str(),
                    "disposition": if intent.consumed { "consumed" } else { "passed" },
                    "outcome": if intent.consumed { "key-up" } else { "passed" },
                }),
            );
        }
        return;
    }
    state.tick += 1;
    let tick = state.tick;
    let correlation = state.correlation();
    let settle = |outcome: &'static str| {
        serde_json::json!({
            "event": "workspace-tiling-toggle",
            "tick": tick,
            "correlation": correlation.as_str(),
            "edge": intent.edge.as_str(),
            "disposition": "consumed",
            "outcome": outcome,
        })
    };
    // KDE `!enabled` parity: chords arriving while keyboard takeover is off
    // never dispatch (the classifier already passes them through; this
    // covers the exact-owner test-needed route, which bypasses it).
    if !state.keyboard.takeover {
        let line = settle("workspace-tiling-refused-disabled");
        log_json_at(&log_path, line);
        return;
    }
    // Fresh per-intent suspension/elevation fence (workspace_tick parity),
    // injected precomputed by the caller: newly intercepted
    // suspended/elevated chords settle here with no side effects.
    if let Some(outcome) = gate {
        let line = settle(outcome);
        log_json_at(&log_path, line);
        return;
    }
    // Same live scope the tray checkbox toggles: the active workspace of the
    // active output. Without a readable scope the chord settles with no side
    // effects (never a guessed fallback).
    let output = state.active_output.clone();
    if output.is_empty() {
        let line = settle("no-scope");
        log_json_at(&log_path, line);
        return;
    }
    toggle_workspace_tiling(state, me, store, dir, fulls, areas, &output);
}

/// Cross-domain sticky-off to the current workspace: drop the float exception
/// from the backing domain and admit on the current domain via two reconciles
/// on a local candidate, with no stale duplicate. With `floating` the window
/// stays a normal float preserving the live frame (no geometry writes);
/// otherwise it tiles current. Band restores under held gates for the tiled
/// destination; focus is retained in both.
#[allow(clippy::too_many_arguments)]
fn sticky_rehome_to_current(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
    member_key: &crate::native_guards::WindowKey,
    from: &str,
    loc: &super::world_reads::Home,
    current_ws: &str,
    origin: &SnapOrigin,
    settle: &dyn Fn(&'static str) -> serde_json::Value,
    log_path: &Path,
    _correlation: &CorrelationId,
    floating: bool,
) {
    let target = if floating { "float" } else { "unfloated" };
    let by_hwnd: HashMap<u64, &ObservedWindow> = observed.iter().map(|w| (w.hwnd, w)).collect();
    let live_frame = by_hwnd
        .get(&member_key.hwnd)
        .map(|w| w.visible)
        .or_else(|| state.float_rects.get(from).copied())
        .or_else(|| {
            retained
                .iter()
                .find(|r| r.key == *member_key)
                .and_then(|row| row.rect)
        });
    let Some(live_frame) = live_frame else {
        let mut line = settle("deferred");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        line["window"] = serde_json::Value::from(from.to_owned());
        log_json_at(log_path, line);
        return;
    };
    if !areas.iter().any(|a| a.device == loc.output) {
        let mut line = settle("unknown-output");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        log_json_at(log_path, line);
        return;
    };
    let topmost_restored = if floating {
        true
    } else {
        let Some(restored) = restore_raised_band(
            state, me, fulls, observed, member_key, from, target, origin, settle, log_path,
            state.tick, "sticky",
        ) else {
            return;
        };
        restored
    };
    let expected_prior = state.sticky.get(member_key).copied().unwrap_or(false);
    if !clear_sticky_mark(
        state,
        me,
        member_key,
        expected_prior,
        from,
        target,
        origin,
        settle,
        log_path,
    ) {
        return;
    }
    // Settled rehome to tile clears a carried float marker too (a prior-float
    // sticky tiling); rehome to float keeps (or repairs) it below.
    if !floating
        && !clear_float_mark(
            state, me, member_key, from, target, origin, settle, log_path,
        )
    {
        return;
    }
    // R-SPC-04 D7 provenance: capture BEFORE the world commit below.
    // Sticky intent withdrew latch/override lanes at sticky-on, so only a
    // surviving durable record or marker still proves fixed origin here.
    let fixed_win = fixed_origin_provenance(state, member_key);
    // World-owned lanes first: sticky leaves, and the float lanes settle by
    // destination (ordinary float rides on, tile clears). The send below
    // carries these facts, so the core homes the now-ordinary window.
    state.sticky.remove(member_key);
    if floating {
        // Sticky-off preserving a prior float: ordinary intent rides on and
        // any explicit tile win withdraws (R-SPC-04 D3/D7), like a settled
        // re-float. Clear the runtime lanes and the marker best-effort.
        // The float marker rides on: repair it when a foreign value crept
        // onto our property; a failed repair keeps local intent and native
        // state with a degraded log. A pending first exit resolves through
        // this ordinary-float lane too.
        state.fixed_tile_overrides.remove(member_key);
        state.fixed_origin_overrides.remove(member_key);
        state.fixed_auto_seen.remove(member_key);
        state.born_first_exit.remove(member_key);
        remove_tile_override_mark_best_effort(state, me, member_key, log_path, from);
        let float_ok = crate::product_hide::sys::read_float_intent_marker(member_key.hwnd)
            || install_float_mark_held(state, me, member_key);
        state.float_rects.insert(from.to_owned(), live_frame);
        state.floated.insert(member_key.clone());
        if !float_ok {
            let focus = match observed.iter().find(|w| w.hwnd == member_key.hwnd) {
                Some(expected) => {
                    retain_float_focus_validated(state, me, fulls, expected, member_key)
                }
                None => "float-focus-failed",
            };
            let mut line = settle("float-unverified");
            line["origin"] = serde_json::Value::from(origin.token.clone());
            line["window"] = serde_json::Value::from(from.to_owned());
            line["target"] = serde_json::Value::from(target);
            line["focus"] = serde_json::Value::from(focus);
            log_json_at(log_path, line);
            state.ensure_device(&loc.output);
            return;
        }
    } else {
        state.float_topmost_prev.remove(member_key);
        state.floated.remove(member_key);
        state.float_rects.remove(from);
        // Settled rehome to tile is an explicit user tile win (R-SPC-04
        // D3/D7), like a settled unfloat: record the runtime override; only
        // a proven fixed origin joins the durable set and persists the
        // marker; the D4 latch withdraws.
        state.fixed_auto_seen.remove(member_key);
        state.fixed_tile_overrides.insert(member_key.clone());
        if fixed_win {
            state.fixed_origin_overrides.insert(member_key.clone());
            if !install_tile_override_mark_held(state, me, member_key) {
                log_json_at(
                    log_path,
                    serde_json::json!({
                        "event": "tile-override",
                        "window": from,
                        "outcome": "marker-unverified",
                    }),
                );
            }
        }
    }
    // The core owns the rehome transfer: the facts above (sticky cleared,
    // float lanes settled) home this now-ordinary window, and the send
    // retains the transfer to the current view. The host diff applies it
    // with fresh proof; focus stays explicit below.
    let Some(wire) = state.wire_of(&loc.output) else {
        let mut line = settle("unknown-output");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        line["window"] = serde_json::Value::from(from.to_owned());
        line["target"] = serde_json::Value::from(target);
        log_json_at(log_path, line);
        return;
    };
    let foreground = crate::ffi::foreground_window();
    let reply = super::world_tick::run_world_command(
        state,
        me,
        store,
        dir,
        observed,
        retained,
        areas,
        foreground,
        fulls,
        tiler_core::world_report::WorldCommand::Send {
            window: tiler_core::directional::WindowId(from.to_owned()),
            target_output: tiler_core::directional::OutputId(wire),
            target_workspace: current_ws.to_owned(),
            follow: false,
        },
    );
    let focus = match observed.iter().find(|w| w.hwnd == member_key.hwnd) {
        Some(expected) => retain_float_focus_validated(state, me, fulls, expected, member_key),
        None => "float-focus-failed",
    };
    if floating {
        let mut line = settle(match reply {
            Some(_) => "sticky-applied",
            None => "sticky-unverified",
        });
        line["origin"] = serde_json::Value::from(origin.token.clone());
        line["window"] = serde_json::Value::from(from.to_owned());
        line["target"] = serde_json::Value::from(target);
        line["focus"] = serde_json::Value::from(focus);
        log_json_at(log_path, line);
        return;
    }
    let outcome: &'static str = match reply {
        Some(_) => "unfloat-applied",
        None => "unfloat-unverified",
    };
    let mut line = settle(outcome);
    line["origin"] = serde_json::Value::from(origin.token.clone());
    line["window"] = serde_json::Value::from(from.to_owned());
    line["target"] = serde_json::Value::from(target);
    line["focus"] = serde_json::Value::from(focus);
    line["topmost_restored"] = serde_json::Value::from(topmost_restored);
    log_json_at(log_path, line);
}

/// Win+Shift+G sticky toggle: origin-sensitive off to the current workspace
/// (prior float stays float, prior tiled tiles), sticky-on from float marks
/// only, sticky-on from tiled floats first via the shared transition.
/// Fullscreen/maximized refuse; failures commit nothing and never retry.
pub(super) fn dispatch_sticky_intent(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
    intent: crate::snapkey::QueuedStickyIntent,
) {
    let log_path = state.log_path.clone();
    if !intent.consumed || !intent.announce {
        if state.trace {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "sticky-toggle",
                    "tick": state.tick,
                    "edge": intent.edge.as_str(),
                    "disposition": if intent.consumed { "consumed" } else { "passed" },
                    "outcome": if intent.consumed { "key-up" } else { "passed" },
                }),
            );
        }
        return;
    }
    state.tick += 1;
    let tick = state.tick;
    let correlation = state.correlation();
    let settle = |outcome: &'static str| {
        serde_json::json!({
            "event": "sticky-toggle",
            "tick": tick,
            "correlation": correlation.as_str(),
            "edge": intent.edge.as_str(),
            "disposition": "consumed",
            "outcome": outcome,
        })
    };
    let Some(origin) = intent.origin.clone() else {
        state.snap_advance = None;
        let line = settle("origin-vanished");
        log_json_at(&log_path, line);
        return;
    };
    // Fresh per-intent suspension/elevation fence (workspace_tick parity):
    // newly intercepted suspended/elevated chords settle here with no side
    // effects; the sticky mark and Engine commit below only run for a live
    // managed member.
    if let Some(outcome) = crate::tiling::toggle_gate_outcome(
        suspend_read(state, me, fulls).veto.block,
        foreground_elevated(me),
    ) {
        let mut line = settle(outcome);
        line["origin"] = serde_json::Value::from(origin.token.clone());
        log_json_at(&log_path, line);
        return;
    };
    let Some(chord) = observe_chord_intent(state, me, fulls, areas) else {
        let mut line = settle("observation-failed");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        log_json_at(&log_path, line);
        return;
    };
    let ChordObserved {
        observed,
        retained,
        skipped,
    } = chord;
    let ChordTarget {
        from,
        member_key,
        loc,
        ..
    } = match resolve_chord_target(state, me, areas, &origin) {
        Ok(target) => target,
        Err(reject) => {
            settle_chord_reject(&log_path, &settle, &origin, &reject);
            return;
        }
    };
    if let Some(refusal) = chord_overlay_refusal(true, &observed, &retained, &member_key) {
        let mut line = settle(refusal);
        line["origin"] = serde_json::Value::from(origin.token.clone());
        line["window"] = serde_json::Value::from(from.clone());
        log_json_at(&log_path, line);
        return;
    }
    // Floating workspaces take no per-window sticky toggles: the domain is
    // released and every frame stays native.
    if !workspace_mode_tiled(state, &loc.output, &loc.workspace) {
        state.snap_advance = None;
        let mut line = settle("workspace-floating");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        line["window"] = serde_json::Value::from(from.clone());
        log_json_at(&log_path, line);
        return;
    }
    if state.sticky.contains_key(&member_key) {
        sticky_off_to_current(
            state,
            me,
            store,
            dir,
            fulls,
            areas,
            &observed,
            &retained,
            &member_key,
            &from,
            &loc,
            &origin,
            &settle,
            &log_path,
            &correlation,
            StickyOffMode::PreservePrior,
        );
        return;
    }
    let is_float = engine_is_float(state, &loc.output, &loc.workspace, &from);
    if is_float {
        // Sticky-on from a normal float: no Engine change, live frame
        // preserved, only the mark. Marker install verifies before the
        // runtime map changes, so a failed write leaves no partial state.
        // Sticky intent withdraws automatic origin and explicit tile wins
        // adapter-side (mirroring the core intentional-float mark
        // withdrawal): a sticky window is intentional, never automatic.
        if !install_sticky_mark_held(state, me, &member_key, true) {
            let mut line = settle("sticky-unverified");
            line["origin"] = serde_json::Value::from(origin.token.clone());
            line["window"] = serde_json::Value::from(from.clone());
            line["target"] = serde_json::Value::from("sticky-float");
            log_json_at(&log_path, line);
            return;
        }
        state.fixed_auto_seen.remove(&member_key);
        state.fixed_origin_overrides.remove(&member_key);
        state.fixed_tile_overrides.remove(&member_key);
        state.born_first_exit.remove(&member_key);
        remove_tile_override_mark_best_effort(state, me, &member_key, &log_path, &from);
        state.sticky.insert(member_key.clone(), true);
        let focus = match observed.iter().find(|w| w.hwnd == member_key.hwnd) {
            Some(expected) => retain_float_focus_validated(state, me, fulls, expected, &member_key),
            None => "float-focus-failed",
        };
        let mut line = settle("sticky-applied");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        line["window"] = serde_json::Value::from(from.clone());
        line["target"] = serde_json::Value::from("sticky-float");
        line["focus"] = serde_json::Value::from(focus);
        log_json_at(&log_path, line);
        return;
    }
    apply_float_from_tiled(
        state,
        me,
        store,
        dir,
        fulls,
        areas,
        &observed,
        &retained,
        skipped,
        &member_key,
        &from,
        &loc,
        &origin,
        &settle,
        &log_path,
        tick,
        &correlation,
        "sticky",
        "sticky-float",
        "sticky-applied",
        true,
    );
}

/// Sticky-off destination: Win+Shift+G preserves a prior float on the current
/// workspace, while Win+G always tiles the current workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StickyOffMode {
    PreservePrior,
    Tile,
}

/// Sticky-off to the current workspace: with `PreservePrior` a prior float
/// stays a normal float preserving the live frame, otherwise the window tiles
/// current. Same-domain tiles via the shared unfloat; cross-domain rehomes.
#[allow(clippy::too_many_arguments)]
fn sticky_off_to_current(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
    member_key: &crate::native_guards::WindowKey,
    from: &str,
    loc: &super::world_reads::Home,
    origin: &SnapOrigin,
    settle: &dyn Fn(&'static str) -> serde_json::Value,
    log_path: &Path,
    correlation: &CorrelationId,
    mode: StickyOffMode,
) {
    let prior = state.sticky.get(member_key).copied().unwrap_or(false);
    let to_float = mode == StickyOffMode::PreservePrior && prior;
    let Some(current_ws) = state.active_view(&loc.output) else {
        let mut line = settle("unknown-output");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        line["window"] = serde_json::Value::from(from.to_owned());
        log_json_at(log_path, line);
        return;
    };
    if to_float {
        if current_ws == loc.workspace {
            // Settling to an ordinary float keeps the float marker: repair
            // it first under held gates, so a failed repair mutates nothing
            // and the window stays sticky with its marker intact.
            if !crate::product_hide::sys::read_float_intent_marker(member_key.hwnd)
                && !install_float_mark_held(state, me, member_key)
            {
                let focus = match observed.iter().find(|w| w.hwnd == member_key.hwnd) {
                    Some(expected) => {
                        retain_float_focus_validated(state, me, fulls, expected, member_key)
                    }
                    None => "float-focus-failed",
                };
                let mut line = settle("float-unverified");
                line["origin"] = serde_json::Value::from(origin.token.clone());
                line["window"] = serde_json::Value::from(from.to_owned());
                line["target"] = serde_json::Value::from("float");
                line["focus"] = serde_json::Value::from(focus);
                log_json_at(log_path, line);
                return;
            }
            if clear_sticky_mark(
                state, me, member_key, true, from, "float", origin, settle, log_path,
            ) {
                state.sticky.remove(member_key);
                // Sticky-off settling to an ordinary float repairs ordinary
                // intent: any explicit tile win withdraws (R-SPC-04 D3/D7).
                // Clear the runtime lanes and the marker best-effort. A
                // pending first exit resolves through this lane too.
                state.fixed_tile_overrides.remove(member_key);
                state.fixed_origin_overrides.remove(member_key);
                state.fixed_auto_seen.remove(member_key);
                state.born_first_exit.remove(member_key);
                remove_tile_override_mark_best_effort(state, me, member_key, log_path, from);
                // Same-workspace settle converges through one observation
                // tick: the cleared sticky plus ordinary float intent homes
                // the window in place with no transfer.
                let foreground = crate::ffi::foreground_window();
                let reply = super::world_tick::run_observation_tick(
                    state,
                    me,
                    store,
                    dir,
                    observed,
                    retained,
                    areas,
                    foreground,
                    fulls,
                );
                let focus = match observed.iter().find(|w| w.hwnd == member_key.hwnd) {
                    Some(expected) => {
                        retain_float_focus_validated(state, me, fulls, expected, member_key)
                    }
                    None => "float-focus-failed",
                };
                let mut line = settle(match reply {
                    Some(_) => "sticky-applied",
                    None => "sticky-unverified",
                });
                line["origin"] = serde_json::Value::from(origin.token.clone());
                line["window"] = serde_json::Value::from(from.to_owned());
                line["target"] = serde_json::Value::from("float");
                line["focus"] = serde_json::Value::from(focus);
                log_json_at(log_path, line);
            }
            return;
        }
        sticky_rehome_to_current(
            state,
            me,
            store,
            dir,
            fulls,
            areas,
            observed,
            retained,
            member_key,
            from,
            loc,
            &current_ws,
            origin,
            settle,
            log_path,
            correlation,
            true,
        );
        return;
    }
    if current_ws == loc.workspace {
        let live_rect = observed
            .iter()
            .find(|w| w.hwnd == member_key.hwnd)
            .map(|w| w.visible)
            .or_else(|| {
                retained
                    .iter()
                    .find(|r| r.key == *member_key)
                    .and_then(|row| row.rect)
            });
        apply_unfloat_to_tiled(
            state,
            me,
            store,
            dir,
            fulls,
            areas,
            observed,
            retained,
            Vec::new(),
            member_key,
            from,
            loc,
            live_rect,
            origin,
            settle,
            log_path,
            state.tick,
            correlation,
            "sticky",
            "unfloated",
            true,
        );
        return;
    }
    sticky_rehome_to_current(
        state,
        me,
        store,
        dir,
        fulls,
        areas,
        observed,
        retained,
        member_key,
        from,
        loc,
        &current_ws,
        origin,
        settle,
        log_path,
        correlation,
        false,
    );
}

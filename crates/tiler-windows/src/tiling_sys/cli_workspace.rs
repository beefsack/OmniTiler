#![forbid(unsafe_code)]
use super::*;

/// Exact-owner out-of-hook fullscreen toggle for the normal `tile` loop only
/// (item 9 test-needed route; the hook still filters injected Win+F11).
/// Reuses the production Win+F11 authority verbatim: the `toggle_gate_outcome`
/// suspend/elevated fence (owned-fullscreen exemption preserved), a fresh
/// observation, the live-foreground `snap_origins` origin (never a carried
/// HWND), the `member_matches` origin binding, the `revalidate_target`
/// focus/identity/scope/lifetime/proof gates with the focus-relaxed overlay
/// allowance, and the `fullscreen_toggle_decision` direction (including the
/// app-owned R-MAX-05 refusal) plus `enter_fullscreen`/`exit_fullscreen_owned`
/// with the same post-restore reconcile. Consume-once, exact-owner, and proof
/// refusal ride the shared `poll_workspace_cli_request` prefix; this runs only
/// after those pass. Logs `fullscreen-toggle` with edge `cli`.
fn dispatch_fullscreen_cli_toggle(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
) {
    state.tick += 1;
    let tick = state.tick;
    let correlation = state.correlation();
    let settle = |outcome: &'static str| {
        serde_json::json!({
            "event": "fullscreen-toggle",
            "tick": tick,
            "correlation": correlation.as_str(),
            "edge": "cli",
            "disposition": "consumed",
            "outcome": outcome,
        })
    };
    // Production per-intent fence first, before any observation.
    if let Some(outcome) = crate::tiling::toggle_gate_outcome(
        suspend_read(state, me, fulls).veto.block,
        foreground_elevated(me),
    ) {
        log_json_at(&state.log_path, settle(outcome));
        return;
    }
    let mut skipped: Vec<(String, String)> = Vec::new();
    let mut retained: Vec<RetainedRow> = Vec::new();
    let Some(mut observed) = state.observe(me, fulls, &mut skipped, &mut retained) else {
        log_json_at(&state.log_path, settle("observation-failed"));
        return;
    };
    publish_managed(state, me, &observed, &retained);
    ensure_workspace_assignments(state, me, &mut observed, &retained, areas);
    clear_maximize_at_admission(state, me, &retained);
    // Live-foreground origin through the same map the hook binds chords
    // against; never a carried HWND, never a retarget.
    let foreground_hwnd = crate::ffi::foreground_window();
    let origin = state.snap_origins.get(&foreground_hwnd).cloned();
    let Some(origin) = origin.filter(|o| !o.token.is_empty()) else {
        log_json_at(&state.log_path, settle("unmanaged"));
        return;
    };
    let Some(member_key) = state
        .member_tokens
        .iter()
        .find(|(_, token)| token.as_str() == origin.token.as_str())
        .map(|(key, _)| key.clone())
    else {
        let mut line = settle("unmanaged");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        log_json_at(&state.log_path, line);
        return;
    };
    if !crate::native_guards::member_matches(
        &member_key,
        origin.hwnd,
        origin.pid,
        &origin.creation,
    ) {
        let mut line = settle("foreground-changed");
        line["origin"] = serde_json::Value::from(origin.token.clone());
        log_json_at(&state.log_path, line);
        return;
    };
    // Fresh pre-effect revalidation through the production target gate,
    // identical to the hook path (retained overlay construction included).
    let owned_expected: Option<ObservedWindow>;
    let expected = if let Some(found) = observed.iter().find(|w| w.hwnd == member_key.hwnd) {
        found
    } else {
        let Some(row) = retained.iter().find(|r| r.key == member_key) else {
            let mut line = settle("deferred");
            line["origin"] = serde_json::Value::from(origin.token.clone());
            log_json_at(&state.log_path, line);
            return;
        };
        let Some(stored) = state.member_identity.get(&row.key).cloned() else {
            let mut line = settle("unmanaged");
            line["origin"] = serde_json::Value::from(origin.token.clone());
            log_json_at(&state.log_path, line);
            return;
        };
        owned_expected = Some(ObservedWindow {
            hwnd: row.key.hwnd,
            token: row.token.clone(),
            outer: Rect {
                x: 0,
                y: 0,
                w: 0,
                h: 0,
            },
            visible: Rect {
                x: 0,
                y: 0,
                w: 0,
                h: 0,
            },
            insets: FrameInsets::default(),
            identity: crate::tiling::ObservedTarget {
                hwnd: row.key.hwnd,
                pid: row.key.pid,
                process_creation: row.key.creation.clone(),
                exe_path: stored.exe_path,
                user_sid: stored.user_sid,
                session_id: stored.session_id,
                tag: String::new(),
            },
            facts: row.facts.unwrap_or(crate::tiling::WindowFacts {
                visible: true,
                minimized: false,
                maximized: true,
                cloaked: false,
                elevated: false,
                shell: false,
                tool_window: false,
                owned: false,
                captionless_fullscreen: false,
                no_activate: false,
                dialog: false,
            }),
        });
        owned_expected.as_ref().expect("retained expected built")
    };
    let from = origin.token.clone();
    let proof_mode = state.allowlist.is_some();
    let member_tag = state.member_tags.get(&member_key).map(String::as_str);
    let target = match revalidate_target(
        expected,
        me,
        fulls,
        &mut state.tokens,
        proof_mode,
        state.allowlist.as_ref(),
        &state.scope,
        member_tag,
        &state.scope_hosts,
        true,
    ) {
        Ok(target) => target,
        Err(reason) => {
            if reason == "identity-changed" {
                drop_member_state(state, &member_key);
            }
            let mut line = settle(reason);
            line["origin"] = serde_json::Value::from(origin.token.clone());
            log_json_at(&state.log_path, line);
            return;
        }
    };
    let _held = &target.held;
    let now_fullscreen = target.window.facts.captionless_fullscreen;
    let meta = read_fullscreen_meta(member_key.hwnd);
    let (target_label, outcome) = match fullscreen_toggle_decision(now_fullscreen, meta.is_some()) {
        crate::tiling::FullscreenToggle::RefuseAppOwned => {
            ("fullscreen", "fullscreen-refused-app-owned")
        }
        crate::tiling::FullscreenToggle::Enter => {
            let full = output_for_rect(areas, &target.window.visible);
            let full = areas
                .iter()
                .find(|a| a.device == full)
                .map(|area| area.full)
                .or_else(|| areas.first().map(|area| area.full));
            match full {
                Some(full) => ("fullscreen", enter_fullscreen(member_key.hwnd, full)),
                None => ("fullscreen", "unknown-output"),
            }
        }
        crate::tiling::FullscreenToggle::ExitOwned => {
            let meta = meta.expect("decision owns metadata");
            let outcome = exit_fullscreen_owned(member_key.hwnd, &meta);
            if outcome == "restored" && !meta.was_maximized {
                let fulls_owned = fulls.to_vec();
                reconcile_tick(state, me, store, dir, &fulls_owned, areas);
            }
            ("restored", outcome)
        }
    };
    log_json_at(
        &state.log_path,
        serde_json::json!({
            "event": "fullscreen-toggle",
            "tick": tick,
            "correlation": correlation.as_str(),
            "edge": "cli",
            "disposition": "consumed",
            "outcome": outcome,
            "target": target_label,
            "origin": origin.token,
            "window": from,
        }),
    );
}

/// Exact-owner out-of-hook float toggle for the normal `tile` loop only
/// (item 8 test-needed route, tentative pending user review; the hook still
/// filters injected Win+G). Reuses the production Win+G authority verbatim
/// through `dispatch_float_intent`: the suspend/elevated fence, a fresh
/// observation, the live-foreground `snap_origins` origin (never a carried
/// HWND), origin re-resolution with member/lifetime/proof/scope fences, the
/// overlay and workspace-mode fences, and the Engine-candidate commit with
/// marker persistence. Consume-once, exact-owner, and proof refusal ride the
/// shared `poll_workspace_cli_request` prefix; this runs only after those
/// pass. The outer `dispatched` line carries edge `cli`; the dispatched arm
/// logs its own `float-toggle` outcome lines. No synthetic input accepted.
fn dispatch_float_cli_toggle(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
) {
    // Production per-intent fence first, before any observation.
    if let Some(outcome) = crate::tiling::toggle_gate_outcome(
        suspend_read(state, me, fulls).veto.block,
        foreground_elevated(me),
    ) {
        state.tick += 1;
        let tick = state.tick;
        log_json_at(
            &state.log_path,
            serde_json::json!({
                "event": "float-toggle",
                "tick": tick,
                "edge": "cli",
                "disposition": "consumed",
                "outcome": outcome,
            }),
        );
        return;
    }
    // Live-foreground origin through the same map the hook binds chords
    // against; never a carried HWND, never a retarget. The dispatch
    // re-resolves it against its own fresh observation, so a foreground
    // change between here and there refuses instead of misacting.
    let foreground_hwnd = crate::ffi::foreground_window();
    let origin = state.snap_origins.get(&foreground_hwnd).cloned();
    let Some(origin) = origin.filter(|o| !o.token.is_empty()) else {
        state.tick += 1;
        let tick = state.tick;
        log_json_at(
            &state.log_path,
            serde_json::json!({
                "event": "float-toggle",
                "tick": tick,
                "edge": "cli",
                "disposition": "consumed",
                "outcome": "unmanaged",
            }),
        );
        return;
    };
    state.tick += 1;
    let tick = state.tick;
    let mut queued = serde_json::json!({
        "event": "float-toggle",
        "tick": tick,
        "edge": "cli",
        "disposition": "consumed",
        "outcome": "dispatched",
    });
    queued["origin"] = serde_json::Value::from(origin.token.clone());
    log_json_at(&state.log_path, queued);
    dispatch_float_intent(
        state,
        me,
        store,
        dir,
        fulls,
        areas,
        QueuedFloatIntent {
            edge: SnapEdge::Down,
            origin: Some(origin),
            consumed: true,
            announce: true,
            tick: Instant::now(),
        },
    );
}

/// Exact-owner out-of-hook sticky toggle for the normal `tile` loop only
/// (item 8 test-needed route, tentative pending user review; the hook still
/// filters injected Win+Shift+G). Reuses the production Win+Shift+G
/// authority verbatim through `dispatch_sticky_intent` with the same fences
/// and origin contract as the float arm above. Logs `sticky-toggle`.
fn dispatch_sticky_cli_toggle(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
) {
    if let Some(outcome) = crate::tiling::toggle_gate_outcome(
        suspend_read(state, me, fulls).veto.block,
        foreground_elevated(me),
    ) {
        state.tick += 1;
        let tick = state.tick;
        log_json_at(
            &state.log_path,
            serde_json::json!({
                "event": "sticky-toggle",
                "tick": tick,
                "edge": "cli",
                "disposition": "consumed",
                "outcome": outcome,
            }),
        );
        return;
    }
    let foreground_hwnd = crate::ffi::foreground_window();
    let origin = state.snap_origins.get(&foreground_hwnd).cloned();
    let Some(origin) = origin.filter(|o| !o.token.is_empty()) else {
        state.tick += 1;
        let tick = state.tick;
        log_json_at(
            &state.log_path,
            serde_json::json!({
                "event": "sticky-toggle",
                "tick": tick,
                "edge": "cli",
                "disposition": "consumed",
                "outcome": "unmanaged",
            }),
        );
        return;
    };
    state.tick += 1;
    let tick = state.tick;
    let mut queued = serde_json::json!({
        "event": "sticky-toggle",
        "tick": tick,
        "edge": "cli",
        "disposition": "consumed",
        "outcome": "dispatched",
    });
    queued["origin"] = serde_json::Value::from(origin.token.clone());
    log_json_at(&state.log_path, queued);
    dispatch_sticky_intent(
        state,
        me,
        store,
        dir,
        fulls,
        areas,
        QueuedStickyIntent {
            edge: SnapEdge::Down,
            origin: Some(origin),
            consumed: true,
            announce: true,
            tick: Instant::now(),
        },
    );
}

/// Exact-owner out-of-hook workspace control: one bounded `workspace.request`
/// file consumed once through the existing `workspace_do_select` /
/// `workspace_do_send` resolvers (plus the pure history resolvers for the
/// previous/relative forms), through the existing project-owned fullscreen
/// toggle below for `fullscreen`, or through the existing hook float/sticky
/// dispatch arms below for `float`/`sticky`. Normal `tile` only (proof
/// owners refuse without effect); workspace ops gate fullscreen and elevated
/// foreground like the hook path while the fullscreen/float/sticky ops route
/// through the production `toggle_gate_outcome` (owned-fullscreen exemption
/// preserved); the keyboard takeover switch never gates this (out-of-hook
/// dogfood/recovery). The request is deleted before dispatch so there is no
/// replay; a malformed or mismatched body is consumed the same way with a
/// `refused` outcome. Sends carry no chord origin: the mover is the live
/// foreground managed window at dispatch, resolved through the same
/// `snap_origins` map the hook binds chords against, then the exact same
/// `workspace_do_send` focus/identity/scope/owner gates with the request's
/// explicit follow/stay intent. Fullscreen/float/sticky carry no HWND either:
/// they toggle the live foreground managed window through the same
/// `revalidate_target` + toggle authority as their hook chords (including
/// the app-owned R-MAX-05 refusal for fullscreen). Production log
/// carries op/index/edge/outcome only (no HWNDs, tokens, or identity bytes);
/// the client correlation is opaque and never logged.
pub(super) fn poll_workspace_cli_request(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
) {
    let path = dir.join(WORKSPACE_REQUEST_FILE);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
        Err(_) => return,
    };
    // Honest op label for the pre-parse refusal: the parsed op when the body
    // is well-formed JSON carrying a known action, else `select`.
    let op_hint = |body: &str| -> String {
        serde_json::from_str::<serde_json::Value>(body)
            .ok()
            .and_then(|v| v.get("action").and_then(|a| a.as_str().map(str::to_owned)))
            .filter(|a| {
                a == "select"
                    || a == "send"
                    || a == "stay"
                    || a == "previous"
                    || a == "fullscreen"
                    || a == "float"
                    || a == "sticky"
                    || a == "relative"
                    || a == "send-relative"
                    || a == "stay-relative"
            })
            .unwrap_or_else(|| "select".to_owned())
    };
    // Proof owners never serve the normal control: consume once with an
    // honest refusal and no native effect so the queue cannot wedge.
    if state.allowlist.is_some() {
        let op = op_hint(text.trim());
        let _ = std::fs::remove_file(&path);
        state.tick += 1;
        let tick = state.tick;
        log_json_at(
            &state.log_path,
            workspace_log(state, tick, &op, 0, "cli", "refused-proof"),
        );
        return;
    }
    let parsed = parse_workspace_request(text.trim());
    // Consume-before-dispatch: exactly once, never replayed, never timed out
    // and retried. A bad body is still consumed with a refused outcome.
    let _ = std::fs::remove_file(&path);
    let request = match parsed {
        Ok(request) => request,
        Err(_) => {
            let op = op_hint(text.trim());
            state.tick += 1;
            let tick = state.tick;
            log_json_at(
                &state.log_path,
                workspace_log(state, tick, &op, 0, "cli", "refused"),
            );
            return;
        }
    };
    let op = request.action.as_str();
    if request.creation != me.process_creation
        || request.pid != me.pid
        || !exe_paths_equal(&request.exe_path, &me.exe_path)
        || request.user_sid != me.user_sid
        || request.session_id != me.session_id
    {
        state.tick += 1;
        let tick = state.tick;
        log_json_at(
            &state.log_path,
            workspace_log(state, tick, op, request.index, "cli", "refused"),
        );
        return;
    }
    if request.action == WorkspaceAction::Fullscreen {
        dispatch_fullscreen_cli_toggle(state, me, store, dir, fulls, areas);
        return;
    }
    if request.action == WorkspaceAction::Float {
        dispatch_float_cli_toggle(state, me, store, dir, fulls, areas);
        return;
    }
    if request.action == WorkspaceAction::Sticky {
        dispatch_sticky_cli_toggle(state, me, store, dir, fulls, areas);
        return;
    }
    if suspend_read(state, me, fulls).veto.block {
        state.tick += 1;
        let tick = state.tick;
        log_json_at(
            &state.log_path,
            workspace_log(state, tick, op, request.index, "cli", "suspended"),
        );
        return;
    }
    if foreground_elevated(me) {
        state.tick += 1;
        let tick = state.tick;
        log_json_at(
            &state.log_path,
            workspace_log(state, tick, op, request.index, "cli", "elevated-foreground"),
        );
        return;
    }
    let Some(output) = chord_output(state, areas) else {
        state.tick += 1;
        let tick = state.tick;
        log_json_at(
            &state.log_path,
            workspace_log(state, tick, op, request.index, "cli", "unknown-output"),
        );
        return;
    };
    state.ensure_device(&output);
    state.tick += 1;
    let tick = state.tick;
    // Dispatch beginning for the CLI path: the clock starts before the
    // initial observation. No queued chord here, so the queue wait stays
    // zero by construction.
    let start = Instant::now();
    let mut skipped: Vec<(String, String)> = Vec::new();
    let mut retained: Vec<RetainedRow> = Vec::new();
    let Some(mut observed) = state.observe(me, fulls, &mut skipped, &mut retained) else {
        log_json_at(
            &state.log_path,
            workspace_log(state, tick, op, request.index, "cli", "observation-failed"),
        );
        return;
    };
    publish_managed(state, me, &observed, &retained);
    ensure_workspace_assignments(state, me, &mut observed, &retained, areas);
    // First-seen maximized windows restore once here: lifetime tags are
    // stamped, and the cleared window converges as eligible next tick.
    clear_maximize_at_admission(state, me, &retained);
    workspace_close_cleanup(state);
    let ctx = ActionCtx {
        correlation: format!("act-{tick}"),
        tick,
        queued_at: start,
        start,
    };
    if request.action.is_send() {
        // Boundary-send testing affordance: the normal loop deliberately
        // rejects injected keys, so the mover is the live foreground managed
        // window at dispatch (same `snap_origins` map the hook binds chords
        // against), never a carried HWND. The request's explicit follow/stay
        // intent threads the exact same `workspace_do_send` gates; old
        // `--send` keeps following. Every other gate stays inside the
        // existing `workspace_do_send`.
        let (target, follow, dispatch_op): (SendTarget, bool, &'static str) = match request.action {
            WorkspaceAction::Send => (SendTarget::Numbered(request.index), true, "send"),
            WorkspaceAction::Stay => (SendTarget::Numbered(request.index), false, "stay"),
            WorkspaceAction::RelativeSend => match request.direction {
                Some(direction) => {
                    let target = SendTarget::Relative(direction.delta());
                    (target, true, target.log_op())
                }
                None => {
                    log_json_at(
                        &state.log_path,
                        workspace_log(state, tick, op, 0, "cli", "refused"),
                    );
                    return;
                }
            },
            WorkspaceAction::RelativeStay => match request.direction {
                Some(direction) => {
                    let delta = direction.delta();
                    (
                        SendTarget::Relative(delta),
                        false,
                        if delta < 0 { "stay-prev" } else { "stay-next" },
                    )
                }
                None => {
                    log_json_at(
                        &state.log_path,
                        workspace_log(state, tick, op, 0, "cli", "refused"),
                    );
                    return;
                }
            },
            WorkspaceAction::Select
            | WorkspaceAction::Previous
            | WorkspaceAction::Fullscreen
            | WorkspaceAction::Float
            | WorkspaceAction::Sticky
            | WorkspaceAction::RelativeHistory => {
                log_json_at(
                    &state.log_path,
                    workspace_log(state, tick, op, request.index, "cli", "refused"),
                );
                return;
            }
        };
        let dispatch_index = match request.action {
            WorkspaceAction::Send | WorkspaceAction::Stay => request.index,
            _ => 0,
        };
        let foreground_hwnd = crate::ffi::foreground_window();
        let origin = state.snap_origins.get(&foreground_hwnd).cloned();
        match origin.filter(|o| !o.token.is_empty()) {
            Some(o) => {
                let effect = workspace_do_send(
                    state,
                    me,
                    store,
                    dir,
                    fulls,
                    areas,
                    &mut observed,
                    &retained,
                    &output,
                    target,
                    follow,
                    o.hwnd,
                    &o.token,
                    o.pid,
                    &o.creation,
                    &ctx,
                );
                log_json_at(
                    &state.log_path,
                    workspace_log(
                        state,
                        tick,
                        dispatch_op,
                        dispatch_index,
                        "cli",
                        effect.outcome,
                    ),
                );
                log_workspace_action(
                    state,
                    &state.log_path.clone(),
                    &ActionLine {
                        ctx: &ctx,
                        op: dispatch_op,
                        index: dispatch_index,
                        edge: "cli",
                        outcome: effect.outcome,
                        focus: effect.focus,
                        timings: &ActionTimings {
                            transition_ms: effect.transition_ms,
                            observation_ms: effect.observation_ms,
                            source_plan_ms: effect.source_plan_ms,
                            target_plan_ms: effect.target_plan_ms,
                            source_geometry_ms: effect.source_geometry_ms,
                            target_geometry_ms: effect.target_geometry_ms,
                            hide_ms: effect.hide_ms,
                            reveal_ms: effect.reveal_ms,
                            focus_ms: effect.focus_ms,
                        },
                        source_workspace: &effect.source_workspace,
                        target_workspace: &effect.target_workspace,
                        source: effect.source.as_ref(),
                        target: effect.target.as_ref(),
                    },
                );
            }
            _ => {
                log_json_at(
                    &state.log_path,
                    workspace_log(state, tick, dispatch_op, dispatch_index, "cli", "unmanaged"),
                );
            }
        }
        return;
    }
    // Resolve without preactivating (same as the hook select path).
    // Numbered digits resolve positionally over the core logical ring;
    // previous/relative/trailing resolve core-side through their dedicated
    // world commands (history and trailing are core-owned).
    let Some(wire) = state.wire_of(&output) else {
        log_json_at(
            &state.log_path,
            workspace_log(state, tick, op, request.index, "cli", "unknown-output"),
        );
        return;
    };
    enum CliSelect {
        Explicit(String),
        Cmd(tiler_core::world_report::WorldCommand),
    }
    let (target, none_outcome, dispatch_op, dispatch_index): (
        Option<CliSelect>,
        &'static str,
        &'static str,
        u8,
    ) = match request.action {
        WorkspaceAction::Select => {
            if request.index == 0 {
                (
                    Some(CliSelect::Cmd(
                        tiler_core::world_report::WorldCommand::SelectTrailing {
                            output: tiler_core::directional::OutputId(wire.clone()),
                        },
                    )),
                    "unknown-output",
                    "select",
                    request.index,
                )
            } else {
                let ring = state.world.engine().logical_ring(&wire);
                match tiler_core::workspace_scope::resolve_numbered(&ring, request.index) {
                    Some(id) => (
                        Some(CliSelect::Explicit(id)),
                        "unknown-target",
                        "select",
                        request.index,
                    ),
                    None => (None, "unknown-target", "select", request.index),
                }
            }
        }
        WorkspaceAction::Previous => (
            Some(CliSelect::Cmd(
                tiler_core::world_report::WorldCommand::SelectPrevious {
                    output: tiler_core::directional::OutputId(wire.clone()),
                },
            )),
            "unknown-target",
            "previous",
            0,
        ),
        WorkspaceAction::RelativeHistory => match request.direction {
            Some(direction) => {
                let delta = direction.delta();
                (
                    Some(CliSelect::Cmd(
                        tiler_core::world_report::WorldCommand::SelectRelative {
                            output: tiler_core::directional::OutputId(wire.clone()),
                            delta,
                        },
                    )),
                    "unknown-target",
                    if delta < 0 {
                        "relative-prev"
                    } else {
                        "relative-next"
                    },
                    0,
                )
            }
            None => (None, "refused", "relative", 0),
        },
        WorkspaceAction::Send
        | WorkspaceAction::Stay
        | WorkspaceAction::Fullscreen
        | WorkspaceAction::Float
        | WorkspaceAction::Sticky
        | WorkspaceAction::RelativeSend
        | WorkspaceAction::RelativeStay => (None, "refused", op, request.index),
    };
    let Some(target) = target else {
        log_json_at(
            &state.log_path,
            workspace_log(
                state,
                tick,
                dispatch_op,
                dispatch_index,
                "cli",
                none_outcome,
            ),
        );
        return;
    };
    // Departure memory is core-owned history now: the select command below
    // records it, so the adapter logs nothing here.
    let effect = match target {
        CliSelect::Explicit(id) => workspace_do_select(
            state,
            me,
            store,
            dir,
            fulls,
            areas,
            &mut observed,
            &output,
            &id,
            &ctx,
            None,
            false,
            false,
        ),
        CliSelect::Cmd(command) => {
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
                command,
            );
            let transition_ms = ctx
                .start
                .elapsed()
                .as_millis()
                .min(u128::from(u64::MAX)) as u64;
            match reply {
                Some(_) => SelectEffect {
                    outcome: "ok",
                    geometry: None,
                    focus: "none",
                    transition_ms,
                    observation_ms: 0,
                    geometry_ms: 0,
                    hide_ms: 0,
                    reveal_ms: 0,
                    focus_ms: 0,
                    plan_ms: 0,
                    source_workspace: state.active_output.clone(),
                    target_workspace: dispatch_op.to_owned(),
                },
                None => SelectEffect {
                    outcome: "unverified",
                    geometry: None,
                    focus: "none",
                    transition_ms,
                    observation_ms: 0,
                    geometry_ms: 0,
                    hide_ms: 0,
                    reveal_ms: 0,
                    focus_ms: 0,
                    plan_ms: 0,
                    source_workspace: state.active_output.clone(),
                    target_workspace: dispatch_op.to_owned(),
                },
            }
        }
    };
    log_json_at(
        &state.log_path,
        workspace_log(
            state,
            tick,
            dispatch_op,
            dispatch_index,
            "cli",
            effect.outcome,
        ),
    );
    log_workspace_action(
        state,
        &state.log_path.clone(),
        &ActionLine {
            ctx: &ctx,
            op: dispatch_op,
            index: dispatch_index,
            edge: "cli",
            outcome: effect.outcome,
            focus: effect.focus,
            timings: &ActionTimings {
                transition_ms: effect.transition_ms,
                observation_ms: effect.observation_ms,
                source_plan_ms: 0,
                target_plan_ms: effect.plan_ms,
                source_geometry_ms: 0,
                target_geometry_ms: effect.geometry_ms,
                hide_ms: effect.hide_ms,
                reveal_ms: effect.reveal_ms,
                focus_ms: effect.focus_ms,
            },
            source_workspace: &effect.source_workspace,
            target_workspace: &effect.target_workspace,
            source: None,
            target: effect.geometry.as_ref(),
        },
    );
}

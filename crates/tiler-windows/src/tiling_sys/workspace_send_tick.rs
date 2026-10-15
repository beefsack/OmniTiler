use super::*;

/// Drain one bounded batch of item 2 relative workspace-send intents.
/// Previous/next steps resolve once from the mover's source workspace through
/// the item 1 scoped ring (wrapping through the trailing empty and ordinals
/// beyond 9, never MRU, never creating); filling the trailing empty invokes
/// ordinary lifecycle for the next spare after transfer. Follow/stay ride the
/// explicit intent exactly like numbered sends (`--no-keyboard-snap-takeover`
/// off, fullscreen/elevated gates, unmanaged-focus refusal all apply). Only
/// verified follow selects record history; stay never does.
#[allow(clippy::too_many_arguments)]
pub(super) fn workspace_relative_send_tick(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
    events: Vec<QueuedWorkspaceSendIntent>,
    blocked: Option<&'static str>,
) {
    let log_path = state.log_path.clone();
    if let Some(cause) = blocked {
        let stale = events.len() as u32;
        if stale > 0 {
            log_json_at(
                &log_path,
                serde_json::json!({"event":"workspace-send-stale","cause": cause, "dropped": stale}),
            );
        }
        return;
    }
    if !state.keyboard.takeover {
        if !events.is_empty() {
            log_json_at(
                &log_path,
                serde_json::json!({"event":"workspace-send-stale","cause":"takeover-off","dropped": events.len()}),
            );
        }
        return;
    }
    for intent in events {
        if !intent.consumed || !intent.announce {
            if state.trace {
                log_json_at(
                    &log_path,
                    serde_json::json!({
                        "event": "workspace-send",
                        "tick": state.tick,
                        "delta": intent.delta,
                        "follow": intent.follow,
                        "edge": intent.edge.as_str(),
                        "disposition": if intent.consumed { "consumed" } else { "passed" },
                        "outcome": "key-up",
                    }),
                );
            }
            continue;
        }
        let dispatch_start = Instant::now();
        if suspend_read(state, me, fulls).veto.block {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "workspace-send",
                    "delta": intent.delta,
                    "follow": intent.follow,
                    "edge": intent.edge.as_str(),
                    "disposition": "consumed",
                    "outcome": "suspended",
                }),
            );
            continue;
        }
        if foreground_elevated(me) {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "workspace-send",
                    "delta": intent.delta,
                    "follow": intent.follow,
                    "edge": intent.edge.as_str(),
                    "disposition": "consumed",
                    "outcome": "elevated-foreground",
                }),
            );
            continue;
        }
        let Some(output) = chord_output(state, areas) else {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "workspace-send",
                    "delta": intent.delta,
                    "follow": intent.follow,
                    "edge": intent.edge.as_str(),
                    "disposition": "consumed",
                    "outcome": "unknown-output",
                }),
            );
            continue;
        };
        state.workspaces.ensure_output(&output);
        state.tick += 1;
        let tick = state.tick;
        let mut skipped: Vec<(String, String)> = Vec::new();
        let mut retained: Vec<RetainedRow> = Vec::new();
        let Some(mut observed) = state.observe(me, fulls, &mut skipped, &mut retained) else {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "workspace-send",
                    "tick": tick,
                    "delta": intent.delta,
                    "follow": intent.follow,
                    "edge": intent.edge.as_str(),
                    "disposition": "consumed",
                    "outcome": "observation-failed",
                }),
            );
            continue;
        };
        publish_managed(state, me, &observed, &retained);
        ensure_workspace_assignments(state, me, &mut observed, &retained, areas);
        clear_maximize_at_admission(state, me, &retained);
        workspace_close_cleanup(state);
        let target = SendTarget::Relative(intent.delta);
        let op = target.log_op();
        let edge = intent.edge;
        let ctx = ActionCtx {
            correlation: format!("act-{tick}"),
            tick,
            queued_at: intent.tick,
            start: dispatch_start,
        };
        let foreground_hwnd = unsafe { GetForegroundWindow() } as usize as u64;
        let origin = intent.origin.clone();
        match origin.filter(|o| o.hwnd == foreground_hwnd) {
            Some(o) if !o.token.is_empty() => {
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
                    intent.follow,
                    o.hwnd,
                    &o.token,
                    o.pid,
                    &o.creation,
                    &ctx,
                );
                log_json_at(
                    &log_path,
                    workspace_log(
                        state,
                        tick,
                        op,
                        target.log_index(),
                        edge.as_str(),
                        effect.outcome,
                    ),
                );
                log_workspace_action(
                    state,
                    &log_path,
                    &ActionLine {
                        ctx: &ctx,
                        op,
                        index: target.log_index(),
                        edge: edge.as_str(),
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
                    &log_path,
                    workspace_log(
                        state,
                        tick,
                        op,
                        target.log_index(),
                        edge.as_str(),
                        "unmanaged",
                    ),
                );
            }
        }
    }
}

/// Drain one bounded batch of item 5 explicit output-send intents.
/// Each intent resolves once from the mover's current frame through the
/// shared FULL-rectangle window-centre/span/left-top selector into the
/// destination output's CURRENT workspace (no candidate is a no-op,
/// unreadable topology refuses, outputs never wrap), then transfers through
/// the commit-last [`run_output_send`] route with command follow/stay exactly
/// like numbered sends (`--no-keyboard-snap-takeover` off, fullscreen/
/// elevated gates, unmanaged-focus refusal all apply). Follow focuses the
/// verified mover on the target output; stay keeps the source view with its
/// MRU (null takes no setter) while the target current view keeps the mover
/// visible.
#[allow(clippy::too_many_arguments)]
pub(super) fn workspace_output_send_tick(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    fulls: &[Rect],
    areas: &[MonitorArea],
    events: Vec<QueuedWorkspaceOutputIntent>,
    blocked: Option<&'static str>,
) {
    let log_path = state.log_path.clone();
    if let Some(cause) = blocked {
        let stale = events.len() as u32;
        if stale > 0 {
            log_json_at(
                &log_path,
                serde_json::json!({"event":"workspace-output-stale","cause": cause, "dropped": stale}),
            );
        }
        return;
    }
    if !state.keyboard.takeover {
        if !events.is_empty() {
            log_json_at(
                &log_path,
                serde_json::json!({"event":"workspace-output-stale","cause":"takeover-off","dropped": events.len()}),
            );
        }
        return;
    }
    for intent in events {
        if !intent.consumed || !intent.announce {
            if state.trace {
                log_json_at(
                    &log_path,
                    serde_json::json!({
                        "event": "workspace-output",
                        "tick": state.tick,
                        "direction": direction_name(intent.direction),
                        "follow": intent.follow,
                        "edge": intent.edge.as_str(),
                        "disposition": if intent.consumed { "consumed" } else { "passed" },
                        "outcome": "key-up",
                    }),
                );
            }
            continue;
        }
        let direction = direction_name(intent.direction);
        let edge = intent.edge.as_str();
        if suspend_read(state, me, fulls).veto.block {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "workspace-output",
                    "direction": direction,
                    "follow": intent.follow,
                    "edge": edge,
                    "disposition": "consumed",
                    "outcome": "suspended",
                }),
            );
            continue;
        }
        if foreground_elevated(me) {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "workspace-output",
                    "direction": direction,
                    "follow": intent.follow,
                    "edge": edge,
                    "disposition": "consumed",
                    "outcome": "elevated-foreground",
                }),
            );
            continue;
        }
        let Some(output) = chord_output(state, areas) else {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "workspace-output",
                    "direction": direction,
                    "follow": intent.follow,
                    "edge": edge,
                    "disposition": "consumed",
                    "outcome": "unknown-output",
                }),
            );
            continue;
        };
        state.workspaces.ensure_output(&output);
        state.tick += 1;
        let tick = state.tick;
        let mut skipped: Vec<(String, String)> = Vec::new();
        let mut retained: Vec<RetainedRow> = Vec::new();
        let Some(mut observed) = state.observe(me, fulls, &mut skipped, &mut retained) else {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "workspace-output",
                    "tick": tick,
                    "direction": direction_name(intent.direction),
                    "follow": intent.follow,
                    "edge": intent.edge.as_str(),
                    "disposition": "consumed",
                    "outcome": "observation-failed",
                }),
            );
            continue;
        };
        publish_managed(state, me, &observed, &retained);
        ensure_workspace_assignments(state, me, &mut observed, &retained, areas);
        clear_maximize_at_admission(state, me, &retained);
        workspace_close_cleanup(state);
        let foreground_hwnd = unsafe { GetForegroundWindow() } as usize as u64;
        let origin = intent.origin.clone();
        let Some(o) = origin.filter(|o| o.hwnd == foreground_hwnd) else {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "workspace-output",
                    "tick": tick,
                    "direction": direction_name(intent.direction),
                    "follow": intent.follow,
                    "edge": intent.edge.as_str(),
                    "disposition": "consumed",
                    "outcome": "unmanaged",
                }),
            );
            continue;
        };
        if o.token.is_empty() {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "workspace-output",
                    "tick": tick,
                    "direction": direction_name(intent.direction),
                    "follow": intent.follow,
                    "edge": intent.edge.as_str(),
                    "disposition": "consumed",
                    "outcome": "unmanaged",
                }),
            );
            continue;
        }
        let outcome = workspace_output_send_one(
            state,
            me,
            fulls,
            areas,
            &mut observed,
            &retained,
            &output,
            &intent,
            &o,
            tick,
        );
        // `run_output_send` logs the full action line; narrow pre-dispatch
        // refusals log here.
        let _ = outcome;
    }
}

/// Dispatch one foreground-verified explicit output-send intent: exact origin
/// binding, scope/lifetime/proof gates, pinned source plus complete source
/// observation, one selector resolution, then the commit-last transfer.
#[allow(clippy::too_many_arguments)]
fn workspace_output_send_one(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    fulls: &[Rect],
    areas: &[MonitorArea],
    observed: &mut [ObservedWindow],
    retained: &[RetainedRow],
    output: &str,
    intent: &QueuedWorkspaceOutputIntent,
    origin: &crate::snapkey::SnapOrigin,
    tick: u64,
) -> &'static str {
    let log_path = state.log_path.clone();
    let log_refuse = |outcome: &'static str| {
        log_json_at(
            &log_path,
            serde_json::json!({
                "event": "workspace-output",
                "tick": tick,
                "direction": direction_name(intent.direction),
                "follow": intent.follow,
                "edge": intent.edge.as_str(),
                "disposition": "consumed",
                "outcome": outcome,
                "origin": origin.token,
            }),
        );
    };
    let mover_key = state
        .member_tokens
        .iter()
        .find(|(_, t)| t.as_str() == origin.token)
        .map(|(k, _)| k.clone());
    let Some(mover_key) = mover_key else {
        log_refuse("unmanaged");
        return "unmanaged";
    };
    // Full chord-time binding: token plus HWND, PID, and process creation.
    // A recycled HWND or a retargeted token never dispatches.
    if !crate::workspace_owner::member_matches(
        &mover_key,
        origin.hwnd,
        origin.pid,
        &origin.creation,
    ) {
        log_refuse("foreground-changed");
        return "foreground-changed";
    }
    let Some(home) = state.workspaces.member_loc(&mover_key).cloned() else {
        log_refuse("unmanaged");
        return "unmanaged";
    };
    if home.output != output || state.workspaces.is_hidden(&mover_key) {
        log_refuse("unmanaged");
        return "unmanaged";
    }
    // Exact scope and visible-lifetime gates before any Engine or project
    // mutation; tail-time fences re-verify live inside `run_output_send`.
    // Sticky/float subject eligibility rides the same run entry so the
    // headless seams cover it without a foreground window.
    if let Some(stored) = state.member_identity.get(&mover_key) {
        if !scope_allows(&state.scope, &stored.exe_path) {
            log_refuse("scope-excluded");
            return "scope-excluded";
        }
        if !hosted_gate_allows(
            &stored.exe_path,
            origin.hwnd,
            stored.pid,
            &state.scope_hosts,
        ) {
            log_refuse("scope-excluded");
            return "scope-excluded";
        }
    }
    let live_tag = live_member_tag(origin.hwnd);
    let lifetime_ok = state.member_tags.get(&mover_key).is_some_and(|stored| {
        crate::workspace_owner::visible_lifetime_ok(stored, live_tag.as_deref())
    });
    if !lifetime_ok {
        drop_member_state(state, &mover_key);
        log_refuse("identity-changed");
        return "identity-changed";
    }
    if state.allowlist.is_some() && !state.workspace_proof {
        log_refuse("workspace-disabled");
        return "workspace-disabled";
    }
    // Complete pinned source observation (hidden snapshots included) so the
    // planned mutation reuses topology instead of remove/reseed. One shared
    // hint-query budget across both assemblies.
    let correlation = state.correlation();
    let mut hint_cx = HintCx::new();
    let Some(source_rows) = assemble_domain_rows(
        state,
        &home.output,
        &home.workspace,
        observed,
        retained,
        "output-send",
        correlation.as_str(),
        &mut hint_cx,
    ) else {
        log_refuse("deferred");
        return "deferred";
    };
    let Some(mover_row) = source_rows.iter().find(|r| r.token == origin.token) else {
        log_refuse("unmanaged");
        return "unmanaged";
    };
    let mover_rect = mover_row.rect;
    // Resolve once through the shared FULL-rectangle selector; the pinned
    // target never re-resolves off the relocated mover below.
    match resolve_output_send_target(state, areas, &home.output, intent.direction, &mover_rect) {
        Ok(None) => {
            log_refuse("no-op");
            "no-op"
        }
        Err(outcome) => {
            log_refuse(outcome);
            outcome
        }
        Ok(Some((target_output, target_workspace))) => {
            state.workspaces.ensure_output(&target_output);
            // Overlay/subject eligibility rides the run entry (which sees
            // both the observed and retained mover); the pinned source rows
            // and this resolution stay the only pre-transfer inputs here.
            run_output_send(
                state,
                me,
                fulls,
                areas,
                observed,
                retained,
                &mut hint_cx,
                &home,
                &mover_key,
                &origin.token,
                &source_rows,
                &target_output,
                &target_workspace,
                intent,
                origin,
                tick,
                &correlation,
                correlation.as_str(),
            )
        }
    }
}

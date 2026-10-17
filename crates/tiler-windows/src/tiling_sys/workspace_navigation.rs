#![forbid(unsafe_code)]
use super::*;

/// Drain one bounded batch of workspace digit intents. Select works on empty
/// workspaces and unmanaged foreground; send refuses unmanaged focus. The
/// `--no-keyboard-snap-takeover` off switch disables all product
/// interception (the hook is never installed; this is the defensive second
/// fence). Fullscreen and elevated foreground gate both ops.
#[allow(clippy::too_many_arguments)]
pub(super) fn workspace_tick(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
    events: Vec<crate::snapkey::QueuedWorkspaceIntent>,
    blocked: Option<&'static str>,
) {
    let log_path = state.log_path.clone();
    if let Some(cause) = blocked {
        let stale = events.len() as u32;
        if stale > 0 {
            log_json_at(
                &log_path,
                serde_json::json!({"event":"workspace-stale","cause": cause, "dropped": stale}),
            );
        }
        return;
    }
    if !state.keyboard.takeover {
        if !events.is_empty() {
            log_json_at(
                &log_path,
                serde_json::json!({"event":"workspace-stale","cause":"takeover-off","dropped": events.len()}),
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
                        "event": "workspace",
                        "tick": state.tick,
                        "op": intent.op.as_str(),
                        "index": intent.index,
                        "edge": intent.edge.as_str(),
                        "disposition": if intent.consumed { "consumed" } else { "passed" },
                        "outcome": "key-up",
                    }),
                );
            }
            continue;
        }
        // Dispatch beginning: the action clock starts here, before the
        // initial observation, so queue-to-effect latency includes the
        // observation itself. `queued_at` stays the cheap callback stamp.
        // Verified managed overlays bypass suspension like a managed maximize.
        let dispatch_start = Instant::now();
        if suspend_read(state, me, fulls).veto.block {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "workspace",
                    "op": intent.op.as_str(),
                    "index": intent.index,
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
                    "event": "workspace",
                    "op": intent.op.as_str(),
                    "index": intent.index,
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
                    "event": "workspace",
                    "op": intent.op.as_str(),
                    "index": intent.index,
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
                    "event": "workspace",
                    "tick": tick,
                    "op": intent.op.as_str(),
                    "index": intent.index,
                    "edge": intent.edge.as_str(),
                    "disposition": "consumed",
                    "outcome": "observation-failed",
                }),
            );
            continue;
        };
        publish_managed(state, me, &observed, &retained);
        ensure_workspace_assignments(state, me, &mut observed, &retained, areas);
        // First-seen maximized windows restore once here: lifetime tags are
        // stamped, and the cleared window converges as eligible next tick.
        clear_maximize_at_admission(state, me, &retained);
        workspace_close_cleanup(state);
        // Stable per-action context: one opaque correlation for the whole
        // select/send/follow lifecycle even when inner geometry ticks
        // increment. `queued_at` is the cheap callback stamp (no
        // logging/syscalls in the callback); `start` is the dispatch
        // beginning on the loop thread, captured before observation.
        let op = intent.op;
        let index = intent.index;
        let edge = intent.edge;
        let ctx = ActionCtx {
            correlation: format!("act-{tick}"),
            tick,
            queued_at: intent.tick,
            start: dispatch_start,
        };
        match op {
            WorkspaceOp::Select => {
                // Resolve without preactivating: `select`/`select_trailing`
                // mutate ACTIVE, which makes `workspace_do_select` see
                // `current == target` and return `already-active` with no
                // native effects. Scoped resolvers resolve (minting shared
                // trailing when needed) without touching ACTIVE; only the
                // transition activates after hide/reveal.
                let target: Option<String> = if index == 0 {
                    let sticky: BTreeSet<crate::workspace::WindowKey> =
                        state.sticky.keys().cloned().collect();
                    let (owner, spaces) = (&mut state.scope_owner, &mut state.workspaces);
                    match owner.resolve_trailing_select(spaces, &output, &sticky) {
                        crate::workspace_scope_owner::TrailingResolve::Reuse(id) => Some(id),
                        crate::workspace_scope_owner::TrailingResolve::Unknown => None,
                    }
                } else {
                    state
                        .scope_owner
                        .resolve_numbered(&state.workspaces, &output, index)
                };
                match target {
                    Some(id) => {
                        // REQ-WS-09 departure memory: the chord-time origin
                        // still holds live foreground, so the source
                        // remembers it before hiding.
                        let foreground_hwnd = crate::ffi::foreground_window();
                        if let Some(source_id) = state.workspaces.active_id(&output)
                            && let Some(origin) = intent.origin.as_ref()
                            && let Some(token) = remember_select_departure(
                                state,
                                origin,
                                &output,
                                &source_id,
                                foreground_hwnd,
                                &observed,
                                &retained,
                            )
                        {
                            log_json_at(
                                &state.log_path.clone(),
                                serde_json::json!({"event":"select-departure","correlation":ctx.correlation,"window":token}),
                            );
                        }
                        let effect = workspace_do_select(
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
                        );
                        log_json_at(
                            &log_path,
                            workspace_log(
                                state,
                                tick,
                                op.as_str(),
                                index,
                                edge.as_str(),
                                effect.outcome,
                            ),
                        );
                        log_workspace_action(
                            state,
                            &log_path,
                            &ActionLine {
                                ctx: &ctx,
                                op: op.as_str(),
                                index,
                                edge: edge.as_str(),
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
                    None => {
                        let outcome = if index == 0 {
                            "unknown-output"
                        } else {
                            "unknown-target"
                        };
                        log_json_at(
                            &log_path,
                            workspace_log(state, tick, op.as_str(), index, edge.as_str(), outcome),
                        );
                    }
                }
            }
            WorkspaceOp::Send => {
                let foreground_hwnd = crate::ffi::foreground_window();
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
                            SendTarget::Numbered(index),
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
                                op.as_str(),
                                index,
                                edge.as_str(),
                                effect.outcome,
                            ),
                        );
                        log_workspace_action(
                            state,
                            &log_path,
                            &ActionLine {
                                ctx: &ctx,
                                op: op.as_str(),
                                index,
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
                                op.as_str(),
                                index,
                                edge.as_str(),
                                "unmanaged",
                            ),
                        );
                    }
                }
            }
        }
    }
}

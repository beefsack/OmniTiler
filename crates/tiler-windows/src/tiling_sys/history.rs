#![forbid(unsafe_code)]
use super::*;

/// Native device key for the next world-routed history intent: the cached
/// output when it is still live, else the first live area. Pure native
/// topology; never rings or views.
fn history_device(state: &TileLoop, areas: &[MonitorArea]) -> Option<String> {
    if !state.active_output.is_empty() && areas.iter().any(|a| a.device == state.active_output) {
        return Some(state.active_output.clone());
    }
    areas.first().map(|a| a.device.clone())
}

/// Drain one bounded batch of workspace history intents through the world
/// owner. Toggle and relative steps select views only, never movers. The
/// core owns history outright (`SelectPrevious` / `SelectRelative`); this
/// tick only routes and logs the honest outcome per intent. Fullscreen and
/// elevated foreground gate dispatch like digits; the takeover-off switch
/// disables all interception.
#[allow(clippy::too_many_arguments)]
pub(super) fn workspace_history_tick(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    _store: &LedgerStore,
    _dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
    events: Vec<QueuedWorkspaceHistoryIntent>,
    blocked: Option<&'static str>,
) {
    let log_path = state.log_path.clone();
    if let Some(cause) = blocked {
        let stale = events.len() as u32;
        if stale > 0 {
            log_json_at(
                &log_path,
                serde_json::json!({"event":"workspace-history-stale","cause": cause, "dropped": stale}),
            );
        }
        return;
    }
    if !state.keyboard.takeover {
        if !events.is_empty() {
            log_json_at(
                &log_path,
                serde_json::json!({"event":"workspace-history-stale","cause":"takeover-off","dropped": events.len()}),
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
                        "event": "workspace-history",
                        "tick": state.tick,
                        "op": intent.op.as_str(),
                        "edge": intent.edge.as_str(),
                        "disposition": if intent.consumed { "consumed" } else { "passed" },
                        "outcome": "key-up",
                    }),
                );
            }
            continue;
        }
        if suspend_read(state, me, fulls).veto.block {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "workspace-history",
                    "op": intent.op.as_str(),
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
                    "event": "workspace-history",
                    "op": intent.op.as_str(),
                    "edge": intent.edge.as_str(),
                    "disposition": "consumed",
                    "outcome": "elevated-foreground",
                }),
            );
            continue;
        }
        let Some(device) = history_device(state, areas) else {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "workspace-history",
                    "op": intent.op.as_str(),
                    "edge": intent.edge.as_str(),
                    "disposition": "consumed",
                    "outcome": "unknown-output",
                }),
            );
            continue;
        };
        let Some(wire) = state.output_tokens.get(&device).cloned() else {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "workspace-history",
                    "tick": state.tick,
                    "op": intent.op.as_str(),
                    "edge": intent.edge.as_str(),
                    "disposition": "consumed",
                    "outcome": "unknown-output",
                }),
            );
            continue;
        };
        let output = tiler_core::directional::OutputId(wire);
        let command = match intent.op {
            WorkspaceHistoryOp::Previous => {
                tiler_core::world_report::WorldCommand::SelectPrevious { output }
            }
            WorkspaceHistoryOp::Prev => {
                tiler_core::world_report::WorldCommand::SelectRelative { output, delta: -1 }
            }
            WorkspaceHistoryOp::Next => {
                tiler_core::world_report::WorldCommand::SelectRelative { output, delta: 1 }
            }
        };
        let op = intent.op.as_str();
        let edge = intent.edge;
        let tick = state.tick;
        super::world_tick::observe_and_command(state, me, _store, _dir, fulls, areas, command);
        let outcome: &'static str = if state.world.last_reply().is_some() {
            state.tick += 1;
            "ok"
        } else {
            "unverified"
        };
        log_json_at(
            &log_path,
            workspace_history_log(state, tick, op, edge.as_str(), outcome),
        );
    }
}

/// Bounded history log line: opaque output/workspace plus op/edge/outcome
/// only. The workspace names the core's current logical view for the active
/// wire output (read-only projection); unknown reads `ws?`.
fn workspace_history_log(
    state: &TileLoop,
    tick: u64,
    op: &str,
    edge: &str,
    outcome: &str,
) -> serde_json::Value {
    let (output, workspace) = history_position(state);
    serde_json::json!({
        "event": "workspace-history",
        "tick": tick,
        "output": output,
        "workspace": workspace,
        "op": op,
        "edge": edge,
        "outcome": outcome,
    })
}

/// Opaque history position for logs: the active device plus the core's
/// current logical view on its wire id. No rings, no history peek.
fn history_position(state: &TileLoop) -> (String, String) {
    let output = if state.active_output.is_empty() {
        "o?".to_owned()
    } else {
        state.active_output.clone()
    };
    let workspace = state
        .output_tokens
        .get(&state.active_output)
        .and_then(|wire| state.world.engine().logical_view(wire))
        .unwrap_or_else(|| "ws?".to_owned());
    (output, workspace)
}

/// Bounded workspace log line: opaque output/workspace plus
/// op/index/edge/outcome only.
pub(super) fn workspace_log(
    state: &TileLoop,
    tick: u64,
    op: &str,
    index: u8,
    edge: &str,
    outcome: &str,
) -> serde_json::Value {
    let (output, workspace) = history_position(state);
    serde_json::json!({
        "event": "workspace",
        "tick": tick,
        "output": output,
        "workspace": workspace,
        "op": op,
        "index": index,
        "edge": edge,
        "outcome": outcome,
    })
}

pub(super) fn log_workspace_action(_state: &TileLoop, log_path: &Path, line: &ActionLine<'_>) {
    let queue_wait_ms = line
        .ctx
        .start
        .duration_since(line.ctx.queued_at)
        .as_millis()
        .min(u128::from(u64::MAX)) as u64;
    let action_ms = line
        .ctx
        .start
        .elapsed()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64;
    let phase = |summary: Option<&ApplySummary>| match summary {
        Some(s) => {
            let mut v = serde_json::json!({
                "applied": s.applied,
                "mismatched": s.mismatched,
                "readback_ok": s.readback_ok,
            });
            match s.veto {
                Some(diag) => {
                    v["veto"] = serde_json::json!({
                        "reason": diag.reason,
                        "desktop": diag.desktop,
                        "visible": diag.visible,
                        "captioned": diag.captioned,
                        "dwm_readable": diag.dwm_readable,
                        "cloaked": diag.cloaked,
                    });
                }
                None => {
                    v["veto"] = serde_json::Value::Null;
                }
            }
            v
        }
        None => serde_json::Value::Null,
    };
    log_json_at(
        log_path,
        serde_json::json!({
            "event": "workspace-action",
            "tick": line.ctx.tick,
            "correlation": line.ctx.correlation,
            "op": line.op,
            "index": line.index,
            "edge": line.edge,
            "outcome": line.outcome,
            "focus": line.focus,
            "queue_wait_ms": queue_wait_ms,
            "action_ms": action_ms,
            "transition_ms": line.timings.transition_ms,
            "observation_ms": line.timings.observation_ms,
            "source_plan_ms": line.timings.source_plan_ms,
            "target_plan_ms": line.timings.target_plan_ms,
            "source_geometry_ms": line.timings.source_geometry_ms,
            "target_geometry_ms": line.timings.target_geometry_ms,
            "hide_ms": line.timings.hide_ms,
            "reveal_ms": line.timings.reveal_ms,
            "focus_ms": line.timings.focus_ms,
            "source_workspace": line.source_workspace,
            "target_workspace": line.target_workspace,
            "source": phase(line.source),
            "target": phase(line.target),
        }),
    );
}

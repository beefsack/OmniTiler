#![forbid(unsafe_code)]
use super::*;
use std::collections::HashMap as SelectViewMap;
use tiler_core::world_report::{TransferKind as SelectTransferKind, WorldReply as SelectReply};

/// Core-resolved select diagnostics for one applied select reply: the
/// commanded output (native device key) plus the logical view there. The
/// adapter passes no output for numbered/relative/previous/trailing
/// selections, so only the core reply knows the target: prefer the output
/// whose view changed and carries the retained view transfer, else the
/// first changed view in wire order. `None` when no view moved (verified
/// no-op) or the wire has no live device.
pub(super) fn resolved_select_view(
    state: &TileLoop,
    before: &SelectViewMap<String, String>,
    reply: &SelectReply,
) -> Option<(String, String)> {
    let mut changed: Vec<(String, String)> = reply
        .outputs
        .iter()
        .filter_map(|out| {
            let view = out.logical_view.clone()?;
            (before.get(&out.output.0) != Some(&view)).then(|| (out.output.0.clone(), view))
        })
        .collect();
    changed.sort();
    changed
        .iter()
        .find(|(wire, _)| {
            reply.transfers.iter().any(|transfer| {
                matches!(transfer.kind, SelectTransferKind::View)
                    && transfer.target_output.0 == *wire
            })
        })
        .or_else(|| changed.first())
        .cloned()
        .and_then(|(wire, view)| state.device_of(&wire).map(|device| (device, view)))
}

/// Bounded select log line with an explicitly resolved output and view:
/// same vocabulary as [`workspace_log`], but the values come from the
/// applied core reply, never from the adapter output cache. `event` names
/// the originating route (`workspace` or `workspace-history`).
// Eight scalar log parts ride one call so every select site logs the same
// vocabulary without rebuilding the JSON shape per call.
#[allow(clippy::too_many_arguments)]
pub(super) fn select_resolved_log(
    event: &str,
    tick: u64,
    device: &str,
    view: &str,
    op: &str,
    index: u8,
    edge: &str,
    outcome: &'static str,
) -> serde_json::Value {
    serde_json::json!({
        "event": event,
        "tick": tick,
        "output": device,
        "workspace": view,
        "op": op,
        "index": index,
        "edge": edge,
        "outcome": outcome,
    })
}

/// Core command for one history intent: toggle, single-step back, or
/// single-step forward. All three are pure view selects with no mover
/// subject; the core resolves the target from the cursor-first switch
/// target and the retained history.
fn history_command(op: WorkspaceHistoryOp) -> tiler_core::world_report::WorldCommand {
    match op {
        WorkspaceHistoryOp::Previous => {
            tiler_core::world_report::WorldCommand::SelectPrevious { output: None }
        }
        WorkspaceHistoryOp::Prev => tiler_core::world_report::WorldCommand::SelectRelative {
            output: None,
            delta: -1,
        },
        WorkspaceHistoryOp::Next => tiler_core::world_report::WorldCommand::SelectRelative {
            output: None,
            delta: 1,
        },
    }
}

/// Full native application for one gate-passing history intent: fresh
/// observation, full plan/apply, exact final-state diagnostics from the
/// applied reply. Unchanged from the pre-latest-only path: the applied
/// reply IS the outcome, and all native gates run inside it.
#[allow(clippy::too_many_arguments)]
fn apply_history(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
    log_path: &Path,
    op: WorkspaceHistoryOp,
    edge: crate::snapkey::SnapEdge,
    tick: u64,
) {
    // Flush-time gate recheck (same gates as intake): a game change
    // between the queued intent's arrival and this flush drops the
    // pending intent instead of applying it. Never dispatches.
    if suspend_read(state, me, fulls).veto.block {
        log_json_at(
            log_path,
            serde_json::json!({
                "event": "workspace-history",
                "op": op.as_str(),
                "edge": edge.as_str(),
                "disposition": "consumed",
                "outcome": "suspended",
            }),
        );
        return;
    }
    if foreground_elevated(me) {
        log_json_at(
            log_path,
            serde_json::json!({
                "event": "workspace-history",
                "op": op.as_str(),
                "edge": edge.as_str(),
                "disposition": "consumed",
                "outcome": "elevated-foreground",
            }),
        );
        return;
    }
    if areas.is_empty() {
        log_json_at(
            log_path,
            serde_json::json!({
                "event": "workspace-history",
                "op": op.as_str(),
                "edge": edge.as_str(),
                "disposition": "consumed",
                "outcome": "unknown-output",
            }),
        );
        return;
    }
    // The adapter selects no output: numbered/relative/previous/trailing
    // resolve in core from the cursor-first switch target. Diagnostics
    // below read the resolved target back from the applied reply.
    let before = state.world.core_projections().2;
    let command = history_command(op);
    let reply =
        super::world_tick::observe_and_command(state, me, store, dir, fulls, areas, command);
    let line = match reply.as_ref() {
        Some(applied) => {
            state.tick += 1;
            match resolved_select_view(state, &before, applied) {
                Some((device, view)) => select_resolved_log(
                    "workspace-history",
                    tick,
                    &device,
                    &view,
                    op.as_str(),
                    0,
                    edge.as_str(),
                    "ok",
                ),
                None => workspace_history_log(state, tick, op.as_str(), edge.as_str(), "ok"),
            }
        }
        None => workspace_history_log(state, tick, op.as_str(), edge.as_str(), "unverified"),
    };
    log_json_at(log_path, line);
}

/// Core-only ordering for a superseded history intent: fresh observation
/// per dispatch (observations never render), exact receipts acked, no
/// native planning or application on this leg (the shared admission
/// preamble still binds fail-closed like the full path). `Some` means the
/// core ordered the command (log `superseded`, never `ok`: nothing
/// attempted is successful application); `None` means it never ordered
/// (log `unverified`, same as the full path's fence outcome).
#[allow(clippy::too_many_arguments)]
fn supersede_history(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
    log_path: &Path,
    op: WorkspaceHistoryOp,
    edge: crate::snapkey::SnapEdge,
    tick: u64,
) {
    let command = history_command(op);
    let ordered = super::world_tick::observe_and_command_core_only(
        state, me, store, dir, fulls, areas, command,
    );
    if ordered.is_some() {
        state.tick += 1;
    }
    log_json_at(
        log_path,
        workspace_history_log(
            state,
            tick,
            op.as_str(),
            edge.as_str(),
            if ordered.is_some() {
                "superseded"
            } else {
                "unverified"
            },
        ),
    );
}
#[allow(clippy::too_many_arguments)]
pub(super) fn workspace_history_tick(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
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
    // Batch-scoped pending latest history select: every intent orders
    // through the core exactly once, but only the latest reply plans and
    // applies natively. A superseded intent dispatches core-only on arrival
    // of the next intent; the surviving intent applies fully at batch end.
    // All three ops are pure view selects with no mover subject and no
    // origin binding, so intermediate native writes are strictly
    // overwritten by the latest view. Never crosses ticks.
    let mut pending_history: Option<QueuedWorkspaceHistoryIntent> = None;
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
        // Mechanical topology-existence fence (no policy): with no live
        // area the core has no registry to resolve from.
        if areas.is_empty() {
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
        // Order the previously pending history select core-only now: it is
        // superseded by this arrival, so its reply must never plan or
        // apply natively. Each dispatch observes fresh; only the surviving
        // select applies below.
        if let Some(prev) = pending_history.take() {
            let prev_tick = state.tick;
            supersede_history(
                state, me, store, dir, fulls, areas, &log_path, prev.op, prev.edge, prev_tick,
            );
        }
        pending_history = Some(intent);
    }
    // Flush the surviving history select (if any) through the full path.
    if let Some(pending) = pending_history.take() {
        let pending_tick = state.tick;
        apply_history(
            state,
            me,
            store,
            dir,
            fulls,
            areas,
            &log_path,
            pending.op,
            pending.edge,
            pending_tick,
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

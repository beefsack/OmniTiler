#![forbid(unsafe_code)]
//! World-routed workspace digit intents.
//!
//! The core owns selection and send planning outright (rings, order,
//! history, trailing, refill): digit selects route `SelectNumbered` /
//! `SelectTrailing` and digit sends route `SendNumbered` through the single
//! world owner. No local resolvers, no preactivation, no history peek here.

use super::*;
use tiler_core::world_report::WorldCommand;

/// Drain one bounded batch of workspace digit intents through the world
/// owner. Select works on empty workspaces and unmanaged foreground; send
/// refuses unmanaged focus. The takeover-off switch disables all
/// interception. Fullscreen and elevated foreground gate both ops.
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
        // Mechanical topology-existence fence (no policy): with no live
        // area the core has no registry to resolve from. Every command
        // below omits the output; the core resolves select targets from
        // the cursor-first switch target and keyboard send sources from
        // the mover's retained home else the cached-live fallback.
        if areas.is_empty() {
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
        let op = intent.op;
        let index = intent.index;
        let edge = intent.edge;
        let tick = state.tick;
        match op {
            WorkspaceOp::Select => {
                // The adapter selects no output: the core resolves
                // numbered/trailing from the cursor-first switch target.
                // Diagnostics read the resolved target from the reply.
                let before = state.world.core_projections().2;
                let command = if index == 0 {
                    WorldCommand::SelectTrailing { output: None }
                } else {
                    WorldCommand::SelectNumbered {
                        output: None,
                        index,
                    }
                };
                let reply = super::world_tick::observe_and_command(
                    state, me, store, dir, fulls, areas, command,
                );
                let line = match reply.as_ref() {
                    Some(applied) => {
                        state.tick += 1;
                        match super::history::resolved_select_view(state, &before, applied) {
                            Some((resolved, view)) => super::history::select_resolved_log(
                                "workspace",
                                tick,
                                &resolved,
                                &view,
                                op.as_str(),
                                index,
                                edge.as_str(),
                                "ok",
                            ),
                            None => {
                                workspace_log(state, tick, op.as_str(), index, edge.as_str(), "ok")
                            }
                        }
                    }
                    None => {
                        workspace_log(state, tick, op.as_str(), index, edge.as_str(), "unverified")
                    }
                };
                log_json_at(&log_path, line);
            }
            WorkspaceOp::Send => {
                let foreground_hwnd = crate::ffi::foreground_window();
                let origin = intent.origin.clone();
                match origin.filter(|o| o.hwnd == foreground_hwnd) {
                    Some(o) if !o.token.is_empty() => {
                        // The command carries a home-then-cached-live
                        // source: the core resolves the mover's retained
                        // home, else the cached output when live, else the
                        // first live screen. The cursor never steers a
                        // homed mover; only a homeless mover can fall
                        // through to a pointer-independent cached-live
                        // fallback, never an adapter decision.
                        let command = WorldCommand::SendNumbered {
                            window: tiler_core::directional::WindowId(o.token.clone()),
                            source: tiler_core::world_report::SendSource::HomeThenCachedLive,
                            index,
                            follow: intent.follow,
                        };
                        super::world_tick::observe_and_command(
                            state, me, store, dir, fulls, areas, command,
                        );
                        let outcome: &'static str = if state.world.last_reply().is_some() {
                            state.tick += 1;
                            "ok"
                        } else {
                            "unverified"
                        };
                        log_json_at(
                            &log_path,
                            workspace_log(state, tick, op.as_str(), index, edge.as_str(), outcome),
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

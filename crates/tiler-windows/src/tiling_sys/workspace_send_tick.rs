#![forbid(unsafe_code)]
//! World-routed relative and output workspace sends.
//!
//! The core owns send planning outright (rings, directional topology,
//! transfer, follow/stay): relative intents route `SendRelative` and
//! explicit output intents route `SendDirectional` through the single world
//! owner. No local rings, no Engine plan, no commit-last transfer here.

use super::*;
use tiler_core::world_report::WorldCommand;

/// Native device key for one send intent: the cached output when live, else
/// the first live area. Pure native topology; never rings or views.
fn send_device(state: &TileLoop, areas: &[MonitorArea]) -> Option<String> {
    if !state.active_output.is_empty() && areas.iter().any(|a| a.device == state.active_output) {
        return Some(state.active_output.clone());
    }
    areas.first().map(|a| a.device.clone())
}

/// Drain one bounded batch of relative workspace-send intents through the
/// world owner. Previous/next steps resolve from the mover source through
/// the core's scoped ring; follow/stay ride the operator intent. Only
/// verified routes report `ok`; every refusal logs its honest outcome.
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
        let Some(device) = send_device(state, areas) else {
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
        let target = SendTarget::Relative(intent.delta);
        let op = target.log_op();
        let edge = intent.edge;
        let foreground_hwnd = crate::ffi::foreground_window();
        let origin = intent.origin.clone();
        let tick = state.tick;
        match origin.filter(|o| o.hwnd == foreground_hwnd) {
            Some(o) if !o.token.is_empty() => {
                // Focused window source output as the existing contract:
                // the mover's own homed output, never the cursor output.
                let mover_wire = state
                    .member_tokens
                    .iter()
                    .find(|(_, token)| token.as_str() == o.token.as_str())
                    .and_then(|(key, _)| state.member_home(key))
                    .and_then(|home| state.wire_of(&home.output))
                    .or_else(|| state.output_tokens.get(&device).cloned());
                let Some(wire) = mover_wire else {
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
                let command = WorldCommand::SendRelative {
                    window: tiler_core::directional::WindowId(o.token.clone()),
                    output: tiler_core::directional::OutputId(wire),
                    delta: intent.delta,
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
                    workspace_log(state, tick, op, target.log_index(), edge.as_str(), outcome),
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

/// Drain one bounded batch of explicit output-send intents through the world
/// owner. Each intent routes `SendDirectional` (window, direction, follow
/// only); the core resolves the strict target plus its current view.
/// Fullscreen/elevated gates and unmanaged-focus refusal mirror the other
/// send ticks.
#[allow(clippy::too_many_arguments)]
pub(super) fn workspace_output_send_tick(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
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
        let foreground_hwnd = crate::ffi::foreground_window();
        let origin = intent.origin.clone();
        let tick = state.tick;
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
        let command = WorldCommand::SendDirectional {
            window: tiler_core::directional::WindowId(o.token.clone()),
            direction: direction_name(intent.direction).to_owned(),
            follow: intent.follow,
        };
        let reply =
            super::world_tick::observe_and_command(state, me, store, dir, fulls, areas, command);
        let outcome: &'static str = if reply.is_some() {
            state.tick += 1;
            "send-applied"
        } else {
            "send-unverified"
        };
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
                "origin": o.token,
            }),
        );
    }
}

/// Dispatch one foreground-verified explicit output-send intent through the
/// world owner: exact origin binding plus a single direction-only command.
/// The core resolves the strict target and retains the transfer.
#[allow(clippy::too_many_arguments)]
pub(super) fn workspace_output_send_one(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
    observed: &mut [ObservedWindow],
    retained: &[RetainedRow],
    _output: &str,
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
    if !crate::native_guards::member_matches(&mover_key, origin.hwnd, origin.pid, &origin.creation)
    {
        log_refuse("foreground-changed");
        return "foreground-changed";
    }
    if origin.token.is_empty() {
        log_refuse("unmanaged");
        return "unmanaged";
    }
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
        WorldCommand::SendDirectional {
            window: tiler_core::directional::WindowId(origin.token.clone()),
            direction: direction_name(intent.direction).to_owned(),
            follow: intent.follow,
        },
        super::observation::sample_cursor(),
    );
    match reply {
        Some(_) => {
            log_refuse("send-applied");
            "send-applied"
        }
        None => {
            log_refuse("send-unverified");
            "send-unverified"
        }
    }
}

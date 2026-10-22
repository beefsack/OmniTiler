#![forbid(unsafe_code)]
//! World-routed whole-workspace migration intents.
//!
//! The core owns migration planning outright (rings, refill, retained
//! transfer, both views): each intent resolves the adjacent output through
//! the shared largest-shared-edge FULL-rectangle selector, then routes
//! [`WorldCommand::MigrateDirectional`](tiler_core::world_report::WorldCommand::MigrateDirectional)
//! through the single world owner. No Engine clone, no candidate, no
//! commit-last native transfer in this module.

use super::*;
use tiler_core::world_report::WorldCommand;

/// Drain one bounded batch of whole-active-workspace migration intents.
/// Each intent migrates the ACTIVE workspace of the chord output only
/// (follow-only). Whole-workspace migration refuses in shared mode with the
/// established `mode-shared` outcome before any mutation; unreadable
/// topology refuses; no adjacent candidate is a no-op.
#[allow(clippy::too_many_arguments)]
pub(super) fn workspace_migrate_tick(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
    events: Vec<QueuedWorkspaceMigrateIntent>,
    blocked: Option<&'static str>,
) {
    let log_path = state.log_path.clone();
    if let Some(cause) = blocked {
        let stale = events.len() as u32;
        if stale > 0 {
            log_json_at(
                &log_path,
                serde_json::json!({"event":"workspace-migrate-stale","cause": cause, "dropped": stale}),
            );
        }
        return;
    }
    if !state.keyboard.takeover {
        if !events.is_empty() {
            log_json_at(
                &log_path,
                serde_json::json!({"event":"workspace-migrate-stale","cause":"takeover-off","dropped": events.len()}),
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
                        "event": "workspace-migrate",
                        "tick": state.tick,
                        "direction": direction_name(intent.direction),
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
                    "event": "workspace-migrate",
                    "direction": direction,
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
                    "event": "workspace-migrate",
                    "direction": direction,
                    "edge": edge,
                    "disposition": "consumed",
                    "outcome": "elevated-foreground",
                }),
            );
            continue;
        }
        let Some(device) = chord_output(state, areas) else {
            log_json_at(
                &log_path,
                serde_json::json!({
                    "event": "workspace-migrate",
                    "direction": direction,
                    "edge": edge,
                    "disposition": "consumed",
                    "outcome": "unknown-output",
                }),
            );
            continue;
        };
        state.tick += 1;
        let tick = state.tick;
        // The tick below re-observes through the world route; no separate
        // observation products are needed here.
        let outcome = workspace_migrate_one(
            state,
            me,
            store,
            dir,
            fulls,
            areas,
            &mut [],
            &[],
            &device,
            &intent,
            tick,
        );
        let _ = outcome;
    }
}

/// Dispatch one migration intent: shared-mode guard, then a single
/// direction-only world command. The core resolves the strict target and
/// runs the world migration path; the pinned source workspace names the
/// core's current logical view on the source wire output.
#[allow(clippy::too_many_arguments)]
pub(super) fn workspace_migrate_one(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
    _observed: &mut [ObservedWindow],
    _retained: &[RetainedRow],
    output: &str,
    intent: &QueuedWorkspaceMigrateIntent,
    tick: u64,
) -> &'static str {
    let log_path = state.log_path.clone();
    let log_refuse = |outcome: &'static str| {
        log_json_at(
            &log_path,
            serde_json::json!({
                "event": "workspace-migrate",
                "tick": tick,
                "direction": direction_name(intent.direction),
                "edge": intent.edge.as_str(),
                "disposition": "consumed",
                "outcome": outcome,
            }),
        );
    };
    if let Some(outcome) = scope_sends::migration_scope_refusal(state) {
        log_refuse(outcome);
        return outcome;
    }
    let Some(wire) = state.output_tokens.get(output).cloned() else {
        log_refuse("unknown-output");
        return "unknown-output";
    };
    let Some(workspace) = state.world.engine().logical_view(&wire) else {
        log_refuse("unknown-target");
        return "unknown-target";
    };
    let command = WorldCommand::MigrateDirectional {
        source_output: tiler_core::directional::OutputId(wire),
        workspace,
        direction: direction_name(intent.direction).to_owned(),
    };
    let reply =
        super::world_tick::observe_and_command(state, me, store, dir, fulls, areas, command);
    let outcome: &'static str = if reply.is_some() {
        "migrate-applied"
    } else {
        "migrate-unverified"
    };
    log_refuse(outcome);
    outcome
}

#![forbid(unsafe_code)]
//! World-routed whole-workspace migration transfer (deleted plan).
//!
//! The commit-last Engine-clone transfer, the ALL-member native output
//! remap, and the follow focus tail are core-owned now: the single
//! [`WorldCommand::MigrateWorkspace`](tiler_core::world_report::WorldCommand::MigrateWorkspace)
//! / `MigrateDirectional` route plans the move and the host diff applies
//! every accepted reply (member frames, arrival proof, both views). This
//! module only routes the pinned transfer and reports the honest outcome.

use super::*;
use tiler_core::world_report::WorldCommand;

/// Follow tail after a committed world-routed migration: the core retains
/// the moved active client and the host diff applies focus effects, so the
/// adapter only logs the honest outcome. Empty migrations (no active
/// window) take no focus.
#[allow(clippy::too_many_arguments)]
pub(super) fn migration_follow_tail(
    _state: &mut TileLoop,
    _me: &ProcessIdentity,
    _observed: &[ObservedWindow],
    _retained: &[RetainedRow],
    _members: &BTreeSet<crate::native_guards::WindowKey>,
    active: Option<tiler_core::directional::WindowId>,
    _focus_fulls_fallback: &[Rect],
    log_migrate: &dyn Fn(&'static str, serde_json::Value),
) -> &'static str {
    match active {
        Some(active) => {
            log_migrate(
                "migrate-applied",
                serde_json::json!({"window": active.0, "focus": "none"}),
            );
            "migrate-applied"
        }
        None => {
            log_migrate("migrate-applied", serde_json::json!({"focus": "none"}));
            "migrate-applied"
        }
    }
}

/// Route one pinned whole-workspace migration through the world owner.
///
/// `source_workspace` names the stable core logical workspace id;
/// `target_output` is the native device key of the adjacent screen. The
/// core resolves the strict target, plans the move, and retains member
/// arrival plus both view proofs. Returns `migrate-applied` when the owner
/// accepted and applied a reply, `unknown-output` when either device has no
/// wire id, and `migrate-unverified` when the command was rejected or lost.
#[allow(clippy::too_many_arguments)]
pub(super) fn run_workspace_migrate(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
    _hint_cx: &mut HintCx,
    source_output: &str,
    source_workspace: &str,
    target_output: &str,
    _target_prior: &str,
    intent: &QueuedWorkspaceMigrateIntent,
    tick: u64,
    _correlation: &CorrelationId,
    correlation_str: &str,
) -> &'static str {
    let log_path = state.log_path.clone();
    let log_migrate = |outcome: &'static str, extra: serde_json::Value| {
        let mut line = serde_json::Map::new();
        line.insert("event".to_owned(), serde_json::json!("workspace-migrate"));
        line.insert("tick".to_owned(), serde_json::json!(tick));
        line.insert("correlation".to_owned(), serde_json::json!(correlation_str));
        line.insert(
            "direction".to_owned(),
            serde_json::json!(crate::snapkey::direction_name(intent.direction)),
        );
        line.insert("edge".to_owned(), serde_json::json!(intent.edge.as_str()));
        line.insert("disposition".to_owned(), serde_json::json!("consumed"));
        line.insert("outcome".to_owned(), serde_json::json!(outcome));
        if let serde_json::Value::Object(map) = extra {
            for (k, v) in map {
                line.insert(k, v);
            }
        }
        log_json_at(&log_path, serde_json::Value::Object(line));
    };
    if let Some(outcome) = scope_sends::migration_scope_refusal(state) {
        log_migrate(outcome, serde_json::json!({}));
        return outcome;
    }
    let Some(source_wire) = state.output_tokens.get(source_output).cloned() else {
        log_migrate("unknown-output", serde_json::json!({}));
        return "unknown-output";
    };
    let Some(target_wire) = state.output_tokens.get(target_output).cloned() else {
        log_migrate("unknown-output", serde_json::json!({}));
        return "unknown-output";
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
        WorldCommand::MigrateWorkspace {
            source_output: tiler_core::directional::OutputId(source_wire),
            target_output: tiler_core::directional::OutputId(target_wire),
            workspace: source_workspace.to_owned(),
        },
    );
    match reply {
        Some(_) => {
            log_migrate("migrate-applied", serde_json::json!({}));
            "migrate-applied"
        }
        None => {
            log_migrate("migrate-unverified", serde_json::json!({}));
            "migrate-unverified"
        }
    }
}

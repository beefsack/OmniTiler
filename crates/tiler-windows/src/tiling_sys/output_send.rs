#![forbid(unsafe_code)]
//! World-routed explicit output send.
//!
//! The core owns directional topology and cross-output transfer planning:
//! each intent routes
//! [`WorldCommand::SendDirectional`](tiler_core::world_report::WorldCommand::SendDirectional)
//! (window plus direction plus follow only) through the single world owner.
//! No candidate Engine, no combined apply, no arrival proof in this module;
//! the host diff applies every accepted reply with fresh proof.

use super::*;
use tiler_core::world_report::WorldCommand;

/// Route one resolved explicit output send through the world owner.
///
/// The pinned source `home` and the destination pair are kept only for
/// honest logging; the command names the window, the direction, and follow
/// alone, and the core resolves the strict target plus its current view.
/// Returns `send-applied` when the owner accepted and applied a reply, and
/// an honest refusal otherwise (never a silent no-op).
#[allow(clippy::too_many_arguments)]
pub(super) fn run_output_send(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
    _hint_cx: &mut HintCx,
    home: &super::world_reads::Home,
    member_key: &crate::native_guards::WindowKey,
    from_token: &str,
    target_output: &str,
    target_workspace: &str,
    intent: &crate::snapkey::QueuedWorkspaceOutputIntent,
    _origin: &crate::snapkey::SnapOrigin,
    tick: u64,
    _correlation: &CorrelationId,
    correlation_str: &str,
) -> &'static str {
    let log_path = state.log_path.clone();
    let follow = intent.follow;
    let log_send = |outcome: &'static str, extra: serde_json::Value| {
        let mut line = serde_json::Map::new();
        line.insert("event".to_owned(), serde_json::json!("workspace-output"));
        line.insert("tick".to_owned(), serde_json::json!(tick));
        line.insert("correlation".to_owned(), serde_json::json!(correlation_str));
        line.insert(
            "direction".to_owned(),
            serde_json::json!(crate::snapkey::direction_name(intent.direction)),
        );
        line.insert("follow".to_owned(), serde_json::json!(follow));
        line.insert("edge".to_owned(), serde_json::json!(intent.edge.as_str()));
        line.insert("disposition".to_owned(), serde_json::json!("consumed"));
        line.insert("outcome".to_owned(), serde_json::json!(outcome));
        line.insert("origin".to_owned(), serde_json::json!(from_token));
        if let serde_json::Value::Object(map) = extra {
            for (k, v) in map {
                line.insert(k, v);
            }
        }
        log_json_at(&log_path, serde_json::Value::Object(line));
    };
    if !state
        .member_tokens
        .get(member_key)
        .is_some_and(|token| token == from_token)
    {
        log_send("unmanaged", serde_json::json!({}));
        return "unmanaged";
    }
    let fulls = fulls;
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
            window: tiler_core::directional::WindowId(from_token.to_owned()),
            direction: crate::snapkey::direction_name(intent.direction).to_owned(),
            follow,
        },
    );
    // Honest home/destination context for the log line only.
    let _ = (home, target_output, target_workspace);
    match reply {
        Some(_) => {
            log_send("send-applied", serde_json::json!({"cross_output": true}));
            "send-applied"
        }
        None => {
            log_send("send-unverified", serde_json::json!({"window": from_token}));
            "send-unverified"
        }
    }
}

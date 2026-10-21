#![forbid(unsafe_code)]
//! World-routed send follow tail.
//!
//! The core owns the send transfer and the follow view change: this tail
//! only routes the explicit [`WorldCommand::Send`](tiler_core::world_report::WorldCommand::Send)
//! with `follow=true` through the single world owner and reports the honest
//! outcome. No hide/reveal, no Engine plan, no history.

use super::*;
use tiler_core::world_report::WorldCommand;

/// Route one send-and-follow through the world owner.
///
/// `target_id` is the stable core logical workspace id on the wire output
/// for `output` (device key). Returns `ok` when the owner accepted and
/// applied a reply, `unknown-output` when the device has no wire id, and
/// `unverified` when the command was rejected or lost.
#[allow(clippy::too_many_arguments)]
pub(super) fn workspace_send_follow(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
    observed: &mut [ObservedWindow],
    mover_key: &crate::native_guards::WindowKey,
    _stored: &ProcessIdentity,
    _mover_minimized: bool,
    output: &str,
    target_id: &str,
    source_token: &str,
    target_token: &str,
    ctx: &ActionCtx,
    source: Option<ApplySummary>,
    source_ms: u64,
    source_plan_ms: u64,
    _preflight: SendPreflight,
    _deferred: bool,
    _to_float: bool,
) -> SendEffect {
    let fail = |outcome: &'static str| SendEffect {
        outcome,
        focus: "none",
        source,
        target: None,
        transition_ms: 0,
        observation_ms: 0,
        source_geometry_ms: source_ms,
        target_geometry_ms: 0,
        hide_ms: 0,
        reveal_ms: 0,
        focus_ms: 0,
        source_plan_ms,
        target_plan_ms: 0,
        source_workspace: source_token.to_owned(),
        target_workspace: target_token.to_owned(),
    };
    let Some(token) = state.member_tokens.get(mover_key).cloned() else {
        return fail("unmanaged");
    };
    let Some(wire) = state.output_tokens.get(output).cloned() else {
        return fail("unknown-output");
    };
    let foreground = crate::ffi::foreground_window();
    let reply = super::world_tick::run_world_command(
        state,
        me,
        store,
        dir,
        observed,
        &[],
        areas,
        foreground,
        fulls,
        WorldCommand::Send {
            window: tiler_core::directional::WindowId(token),
            target_output: tiler_core::directional::OutputId(wire),
            target_workspace: target_id.to_owned(),
            follow: true,
        },
    );
    match reply {
        Some(_) => SendEffect {
            outcome: "ok",
            focus: "none",
            source,
            target: None,
            transition_ms: ctx.start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
            observation_ms: 0,
            source_geometry_ms: source_ms,
            target_geometry_ms: 0,
            hide_ms: 0,
            reveal_ms: 0,
            focus_ms: 0,
            source_plan_ms,
            target_plan_ms: 0,
            source_workspace: source_token.to_owned(),
            target_workspace: target_token.to_owned(),
        },
        None => fail("unverified"),
    }
}

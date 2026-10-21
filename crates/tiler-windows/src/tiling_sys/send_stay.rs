#![forbid(unsafe_code)]
//! World-routed send stay tail.
//!
//! The core owns the send transfer, the stay source view, and trailing
//! lifecycle: stay tails only route
//! [`WorldCommand::Send`](tiler_core::world_report::WorldCommand::Send) with
//! `follow=false` through the single world owner. No Engine plan, no
//! hide/reveal, no focus setter, no history.

use super::*;
use tiler_core::world_report::WorldCommand;

/// Route one send-and-stay through the world owner.
///
/// The mover keeps the source view; only the core transfer applies. Returns
/// `ok` when the owner accepted and applied a reply, `unmanaged` when the
/// mover is unknown, `unknown-output` when the device has no wire id, and
/// `unverified` when the command was rejected or lost.
#[allow(clippy::too_many_arguments)]
pub(super) fn workspace_send_stay(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    observed: &mut [ObservedWindow],
    retained: &[RetainedRow],
    mover_key: &crate::native_guards::WindowKey,
    _stored: &ProcessIdentity,
    _mover_minimized: bool,
    _mover_hwnd: u64,
    output: &str,
    _source_id: &str,
    target_id: &str,
    source_token: &str,
    target_token: &str,
    ctx: &ActionCtx,
    _plan: &tiler_core::boundary::SendWorkspacePlan,
    _preflight: SendPreflight,
    source_plan_ms: u64,
    _deferred: bool,
    _to_float: bool,
) -> SendEffect {
    let fail = |outcome: &'static str| SendEffect {
        outcome,
        focus: "none",
        source: None,
        target: None,
        transition_ms: 0,
        observation_ms: 0,
        source_geometry_ms: 0,
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
        retained,
        &[],
        foreground,
        fulls,
        WorldCommand::Send {
            window: tiler_core::directional::WindowId(token),
            target_output: tiler_core::directional::OutputId(wire),
            target_workspace: target_id.to_owned(),
            follow: false,
        },
    );
    match reply {
        Some(_) => SendEffect {
            outcome: "ok",
            focus: "no-focus",
            source: None,
            target: None,
            transition_ms: ctx.start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
            observation_ms: 0,
            source_geometry_ms: 0,
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

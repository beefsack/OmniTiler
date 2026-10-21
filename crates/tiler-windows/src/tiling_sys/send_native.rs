#![forbid(unsafe_code)]
//! World-routed floating-boundary send.
//!
//! The core owns cross-boundary send planning (including floating
//! workspaces): this module only routes
//! [`WorldCommand::Send`](tiler_core::world_report::WorldCommand::Send)
//! through the single world owner and reports the honest outcome. No
//! two-domain Engine plan, no native membership projection.

use super::*;
use tiler_core::world_report::WorldCommand;

/// Route one floating-boundary send through the world owner.
///
/// `target_id` is the stable core logical workspace id on the wire output
/// for `output` (device key). Returns `ok` when the owner accepted and
/// applied a reply, `unmanaged` when the mover is unknown,
/// `unknown-output` when the device has no wire id, and `unverified` when
/// the command was rejected or lost.
#[allow(clippy::too_many_arguments)]
pub(super) fn workspace_do_send_native(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
    observed: &mut [ObservedWindow],
    retained: &[RetainedRow],
    output: &str,
    target_id: &str,
    mover_key: &crate::native_guards::WindowKey,
    _mover_hwnd: u64,
    source_token: &str,
    target_token: &str,
    _source_tiled: bool,
    _target_tiled: bool,
    ctx: &ActionCtx,
    follow: bool,
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
        source_plan_ms: 0,
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
        areas,
        foreground,
        fulls,
        WorldCommand::Send {
            window: tiler_core::directional::WindowId(token),
            target_output: tiler_core::directional::OutputId(wire),
            target_workspace: target_id.to_owned(),
            follow,
        },
    );
    let transition_ms = ctx
        .start
        .elapsed()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64;
    match reply {
        Some(_) => SendEffect {
            outcome: "ok",
            focus: "none",
            source: None,
            target: None,
            transition_ms,
            observation_ms: 0,
            source_geometry_ms: 0,
            target_geometry_ms: 0,
            hide_ms: 0,
            reveal_ms: 0,
            focus_ms: 0,
            source_plan_ms: 0,
            target_plan_ms: 0,
            source_workspace: source_token.to_owned(),
            target_workspace: target_token.to_owned(),
        },
        None => {
            let mut effect = fail("unverified");
            effect.transition_ms = transition_ms;
            effect
        }
    }
}

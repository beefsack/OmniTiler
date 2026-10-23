#![forbid(unsafe_code)]
//! World-routed same-output workspace send.
//!
//! The core owns send planning outright (rings, transfer, follow/stay,
//! trailing): this module only routes the operator intent through
//! [`WorldCommand::SendNumbered`](tiler_core::world_report::WorldCommand::SendNumbered)
//! / `SendRelative` via the single world owner and reports the honest
//! outcome. No local rings, no Engine plan, no hide/reveal, no history.

use super::*;
use tiler_core::world_report::WorldCommand;

/// Route one same-output send through the world owner.
///
/// `target` is the frozen operator intent. `follow` pins the explicit
/// intent. The command omits the source output: the core resolves the
/// mover's retained home, else the migration/control action context
/// (explicit send commands with named outputs are unaffected). Returns an
/// honest [`SendEffect`]: `ok` when the owner accepted and applied a
/// reply, `unmanaged` when the mover token is unknown, and `unverified`
/// when the command was rejected or lost (never a silent no-op).
#[allow(clippy::too_many_arguments)]
pub(super) fn workspace_do_send(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
    observed: &mut [ObservedWindow],
    retained: &[RetainedRow],
    target: SendTarget,
    follow: bool,
    mover_hwnd: u64,
    origin_token: &str,
    origin_pid: u32,
    origin_creation: &str,
    ctx: &ActionCtx,
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
        source_workspace: "ws?".to_owned(),
        target_workspace: "ws?".to_owned(),
    };
    let _ = (mover_hwnd, origin_pid, origin_creation);
    if origin_token.is_empty()
        || !state
            .member_tokens
            .values()
            .any(|token| token == origin_token)
    {
        return fail("unmanaged");
    }
    // The command carries a home-then-action source: the core resolves
    // the mover's retained home, else the full migration/control action
    // context (foreground, pointer, cache, registry). The mover log line
    // below stays diagnostic only; selection is core-owned.
    let mover_device = state
        .member_tokens
        .iter()
        .find(|(_, token)| token.as_str() == origin_token)
        .and_then(|(key, _)| state.member_home(key))
        .map(|home| home.output);
    if state.allowlist.is_some() && !state.workspace_proof {
        return fail("workspace-disabled");
    }
    let window = tiler_core::directional::WindowId(origin_token.to_owned());
    let command = match target {
        SendTarget::Numbered(index) => WorldCommand::SendNumbered {
            window,
            source: tiler_core::world_report::SendSource::HomeThenAction,
            index,
            follow,
        },
        SendTarget::Relative(delta) => WorldCommand::SendRelative {
            window,
            source: tiler_core::world_report::SendSource::HomeThenAction,
            delta,
            follow,
        },
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
        command,
        super::observation::sample_cursor(),
    );
    let transition_ms = ctx.start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
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
            source_workspace: mover_device.unwrap_or_else(|| "ws?".to_owned()),
            target_workspace: target.log_op().to_owned(),
        },
        None => {
            let mut effect = fail("unverified");
            effect.transition_ms = transition_ms;
            effect
        }
    }
}

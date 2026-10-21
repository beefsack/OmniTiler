#![forbid(unsafe_code)]
//! World-routed workspace selection.
//!
//! The core owns selection outright (rings, order, history, trailing,
//! refill): this module only routes the explicit target through
//! [`WorldCommand::SelectWorkspace`](tiler_core::world_report::WorldCommand::SelectWorkspace)
//! via the single world owner and reports the honest outcome. No local
//! rings, no hide/reveal/activate, no history, no trailing. Departure memory
//! is core history too, so no adapter helper exists.

use super::*;
use tiler_core::world_report::WorldCommand;

/// Route one explicit workspace select through the world owner.
///
/// `output` is the native device key; the wire id comes from the mechanical
/// `output_tokens` map only. `target` is the stable core logical workspace
/// id. Returns an honest [`SelectEffect`]: `ok` when the owner accepted and
/// applied a reply, `unknown-output` when the device has no wire id, and
/// `unverified` when the command was rejected or lost (never a silent
/// no-op). Per-phase timings collapse into `transition_ms`; geometry/focus
/// ride the applied reply, so those fields stay `None`/`none`.
#[allow(clippy::too_many_arguments)]
pub(super) fn workspace_do_select(
    state: &mut TileLoop,
    me: &ProcessIdentity,
    store: &LedgerStore,
    dir: &Path,
    fulls: &[Rect],
    areas: &[MonitorArea],
    observed: &mut [ObservedWindow],
    output: &str,
    target: &str,
    ctx: &ActionCtx,
    _focus_hint: Option<&crate::native_guards::WindowKey>,
    _suppress_focus: bool,
    _skip_geometry: bool,
) -> SelectEffect {
    let none =
        |outcome: &'static str, source_workspace: String, target_workspace: String| SelectEffect {
            outcome,
            geometry: None,
            focus: "none",
            transition_ms: 0,
            observation_ms: 0,
            geometry_ms: 0,
            hide_ms: 0,
            reveal_ms: 0,
            focus_ms: 0,
            plan_ms: 0,
            source_workspace,
            target_workspace,
        };
    let Some(wire) = state.output_tokens.get(output).cloned() else {
        return none("unknown-output", "ws?".to_owned(), target.to_owned());
    };
    let source_view = state
        .world
        .engine()
        .logical_view(&wire)
        .unwrap_or_else(|| "ws?".to_owned());
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
        WorldCommand::SelectWorkspace {
            output: tiler_core::directional::OutputId(wire),
            workspace: target.to_owned(),
        },
    );
    let transition_ms = ctx.start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
    match reply {
        Some(_) => SelectEffect {
            outcome: "ok",
            geometry: None,
            focus: "none",
            transition_ms,
            observation_ms: 0,
            geometry_ms: 0,
            hide_ms: 0,
            reveal_ms: 0,
            focus_ms: 0,
            plan_ms: 0,
            source_workspace: source_view,
            target_workspace: target.to_owned(),
        },
        None => {
            let mut effect = none("unverified", source_view, target.to_owned());
            effect.transition_ms = transition_ms;
            effect
        }
    }
}

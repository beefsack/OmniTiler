#![forbid(unsafe_code)]

//! World input-to-command mapping (pure translation).
//!
//! Split from [`crate::world_runtime`]: which typed [`InputSemanticAction`]
//! becomes which [`WorldCommand`](tiler_core::world_report::WorldCommand).
//! No sequencing, no core contact, no actuation — the armed select-only
//! subset maps to planner-owned select commands, the send-shaped actions map
//! to planner-owned send commands through the OFFLINE send seam only (no
//! product path dispatches sends: the engine refuses them first), every
//! other supported tiling action maps to its existing core command shape,
//! and anything else has no route (capability-refused by the caller). Pure
//! over retained input state except the keyboard-resize repeat cursor it
//! advances.

use tiler_core::directional::OutputId;
use tiler_core::world_report::WorldCommand;

use crate::runtime::{InputSemanticAction, NativeContext};
use crate::runtime_core::RetainedCore;

/// Static op tokens for diagnostics/ring (legacy `core_replies` vocabulary
/// carries over as `{op}-{outcome}`; tests pin the prefixes).
pub(crate) const OP_RECONCILE: &str = "reconcile";
pub(crate) const OP_FOCUS: &str = "focus";
pub(crate) const OP_MOVE: &str = "move";
pub(crate) const OP_RESIZE: &str = "resize";
pub(crate) const OP_TOGGLE_FLOAT: &str = "toggle-float";
pub(crate) const OP_TOGGLE_ORIENTATION: &str = "toggle-orientation";
pub(crate) const OP_DRAG_DROP: &str = "drag-drop";
pub(crate) const OP_SELECT: &str = "select";
pub(crate) const OP_SEND: &str = "send";
pub(crate) const OP_TERMINATE: &str = "terminate";

/// Resolve the output one armed select switches: the observed-focus home
/// when known, else the first currently-actuated output. `None` refuses
/// (no actuated view to switch).
pub(crate) fn select_output(retained: &RetainedCore, ctx: &NativeContext) -> Option<OutputId> {
    if let Some(focus) = retained.observed_focus.as_ref()
        && let Some(key) = retained.window_domains.get(&focus.0)
    {
        return Some(key.output.clone());
    }
    ctx.domains
        .iter()
        .find(|d| d.current)
        .map(|d| d.output.clone())
}

/// Map one armed select-only action to its planner-owned world command.
/// Unarmed actions (trailing select-0, sends, outputs) have no select route
/// and stay capability-refused by the caller.
pub(crate) fn select_command(
    action: &InputSemanticAction,
    output: OutputId,
) -> Option<WorldCommand> {
    match action {
        InputSemanticAction::WorkspaceSelect { index } if (1..=9).contains(index) => {
            Some(WorldCommand::SelectNumbered {
                output,
                index: *index,
            })
        }
        InputSemanticAction::WorkspaceRelative { prev } => Some(WorldCommand::SelectRelative {
            output,
            delta: if *prev { -1 } else { 1 },
        }),
        InputSemanticAction::WorkspacePrevious => Some(WorldCommand::SelectPrevious { output }),
        _ => None,
    }
}

/// Map one send-shaped action to its planner-owned world command for the
/// focused-window subject `window` (OFFLINE SEAM ONLY: no product path calls
/// this — the engine refuses every send first; fake-proven shape mapping
/// for the live step): `SendNumbered` retains a window transfer core-side
/// (follow pins the view intent; the main-thread drain actuates natively
/// and reports the proven outcome back under the stable operation identity
/// so the core settles instead of retaining forever). Non-send-shaped
/// actions (trailing send-0, stays, relative sends, outputs, selects,
/// basics) have no send route (`None`).
pub(crate) fn send_command(
    action: &InputSemanticAction,
    window: &str,
    output: OutputId,
) -> Option<WorldCommand> {
    match action {
        InputSemanticAction::WorkspaceSend { index, follow }
            if *follow && (1..=9).contains(index) =>
        {
            Some(WorldCommand::SendNumbered {
                window: tiler_core::directional::WindowId(window.to_owned()),
                output,
                index: *index,
                follow: true,
            })
        }
        _ => None,
    }
}

/// Map one supported tiling input to its existing core command (exact
/// legacy shapes) plus its diagnostic op token. Selects and unsupported
/// actions have no operation route here (`None`).
pub(crate) fn operation_command(
    action: &InputSemanticAction,
    window: &str,
    retained: &mut RetainedCore,
) -> Option<(tiler_core::boundary::CoreCommand, &'static str)> {
    use tiler_core::boundary::CoreCommand;
    use tiler_core::directional::SameAxisMove;
    match action {
        InputSemanticAction::FocusStep { direction } => Some((
            CoreCommand::Focus {
                window: window.to_owned(),
                direction: direction.as_str().to_owned(),
                cross_output_transfer: false,
                float_subject: false,
            },
            OP_FOCUS,
        )),
        InputSemanticAction::MoveStep { direction } => Some((
            CoreCommand::Move {
                window: window.to_owned(),
                direction: direction.as_str().to_owned(),
                cross_output_transfer: false,
                same_axis_move: SameAxisMove::default(),
            },
            OP_MOVE,
        )),
        InputSemanticAction::ResizeStep { direction, mode } => {
            // Exact Windows/KDE repeat rule, tracked macOS-side as before:
            // the press index continues only on a
            // focused-window/direction/mode match, restarts otherwise.
            let (press_index, repeat) = crate::runtime_core::resize_repeat_next(
                retained.resize_repeat.clone(),
                window,
                direction.as_str(),
                mode.as_str(),
            );
            retained.resize_repeat = Some(repeat);
            Some((
                CoreCommand::Resize {
                    window: window.to_owned(),
                    direction: direction.as_str().to_owned(),
                    mode: mode.as_str().to_owned(),
                    press_index,
                },
                OP_RESIZE,
            ))
        }
        InputSemanticAction::ToggleFloat => Some((
            CoreCommand::ToggleFloat {
                window: window.to_owned(),
                float_rect: None,
            },
            OP_TOGGLE_FLOAT,
        )),
        InputSemanticAction::ToggleOrientation => Some((
            CoreCommand::ToggleOrientation {
                window: window.to_owned(),
            },
            OP_TOGGLE_ORIENTATION,
        )),
        InputSemanticAction::DragDrop {
            x, y, hover_prior, ..
        } => Some((
            CoreCommand::DragDrop {
                window: window.to_owned(),
                x: *x,
                y: *y,
                hover_prior: hover_prior.clone().map(|prior| *prior),
                source: None,
            },
            OP_DRAG_DROP,
        )),
        _ => None,
    }
}

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
/// Numbered selects (`1..=9`) carry the admission-resolved native workspace
/// as an explicit [`WorldCommand::SelectWorkspace`]: the ordinal resolves
/// exactly once at admission against the native row — Desktop-N is the
/// 1-based native position — and the core executes the pinned SID directly,
/// never re-resolving the ordinal through the scoped selection ring (whose
/// stable-logical order need not match native Desktop order: a runtime
/// started on a non-first Space claims that Space at ring position 1, so a
/// positional re-resolution would self-target). Relative and previous
/// selects keep their shared core semantics untouched. Trailing `index 0`
/// maps only when `trailing_enabled` (see [`TRAILING_CAPABILITY_ENABLED`]);
/// production callers pass the gate const, tests an explicit local. Unarmed
/// actions have no select route and stay capability-refused by the caller.
pub(crate) fn select_command(
    action: &InputSemanticAction,
    output: OutputId,
    target_workspace: &str,
) -> Option<WorldCommand> {
    select_command_with_capability(
        action,
        output,
        target_workspace,
        crate::workspace_input::TRAILING_CAPABILITY_ENABLED,
    )
}

/// Capability-explicit select mapping: the production route above with
/// `trailing_enabled` threaded from the single capability gate. Trailing
/// `index 0` resolves core-side to reuse-or-create (`SelectTrailing`),
/// never to an explicit SID guess: the native drain ensures the trailing
/// Space first (reuse proven-empty last, else exact append) and reports
/// back under the claim's operation identity.
pub(crate) fn select_command_with_capability(
    action: &InputSemanticAction,
    output: OutputId,
    target_workspace: &str,
    trailing_enabled: bool,
) -> Option<WorldCommand> {
    match action {
        InputSemanticAction::WorkspaceSelect { index } if (1..=9).contains(index) => {
            Some(WorldCommand::SelectWorkspace {
                output,
                workspace: target_workspace.to_owned(),
            })
        }
        InputSemanticAction::WorkspaceRelative { prev } => Some(WorldCommand::SelectRelative {
            output,
            delta: if *prev { -1 } else { 1 },
        }),
        InputSemanticAction::WorkspacePrevious => Some(WorldCommand::SelectPrevious { output }),
        InputSemanticAction::WorkspaceSelect { index: 0 } if trailing_enabled => {
            Some(WorldCommand::SelectTrailing { output })
        }
        _ => None,
    }
}

/// Map one send-shaped action to its planner-owned world command for the
/// focused-window `window` subject, carrying the admission-resolved native
/// target workspace (see [`crate::workspace_send::context_target_sid`]).
/// The ordinal resolves exactly once at admission against the native row —
/// Desktop-N is the 1-based native position — and the core executes the
/// pinned SID directly: it never re-resolves the ordinal through the scoped
/// send ring, whose stable-logical order need not match native Desktop order
/// (a runtime started on a non-first Space claims that Space at ring
/// position 1, so a positional re-resolution would self-target). The `follow`
/// Non-send-shaped actions (relative sends, outputs, selects, basics) have
/// no send route (`None`). Trailing send-0 follow maps only when
/// `trailing_enabled` (the native send-0 owner preparation rides the
/// dedicated effect lane); stay-0 never maps.
pub(crate) fn send_command(
    action: &InputSemanticAction,
    window: &str,
    output: OutputId,
    target_workspace: &str,
) -> Option<WorldCommand> {
    send_command_with_capability(
        action,
        window,
        output,
        target_workspace,
        crate::workspace_input::TRAILING_CAPABILITY_ENABLED,
    )
}

/// Capability-explicit send mapping: the production route above with
/// `trailing_enabled` threaded from the single capability gate.
pub(crate) fn send_command_with_capability(
    action: &InputSemanticAction,
    window: &str,
    output: OutputId,
    target_workspace: &str,
    trailing_enabled: bool,
) -> Option<WorldCommand> {
    match action {
        InputSemanticAction::WorkspaceSend { index, follow } if (1..=9).contains(index) => {
            Some(WorldCommand::Send {
                window: tiler_core::directional::WindowId(window.to_owned()),
                target_output: output,
                target_workspace: target_workspace.to_owned(),
                follow: *follow,
            })
        }
        InputSemanticAction::WorkspaceSend {
            index: 0,
            follow: true,
        } if trailing_enabled => Some(WorldCommand::SendNumbered {
            window: tiler_core::directional::WindowId(window.to_owned()),
            output,
            index: 0,
            follow: true,
        }),
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

#[cfg(test)]
mod trailing_command_tests {
    use super::*;
    use crate::runtime_types::InputSemanticAction;

    #[test]
    fn trailing_capability_maps_through_same_production_route_when_armed() {
        let output = OutputId(crate::topology::output_key(1));
        // Armed production: the SAME production mapping routes select-0
        // to SelectTrailing and send-0 follow to SendNumbered 0, while
        // stay-0 never maps.
        let select0 = InputSemanticAction::WorkspaceSelect { index: 0 };
        assert_eq!(
            select_command(&select0, output.clone(), "space-1"),
            Some(WorldCommand::SelectTrailing {
                output: output.clone()
            })
        );
        assert_eq!(
            select_command_with_capability(&select0, output.clone(), "space-1", true),
            Some(WorldCommand::SelectTrailing {
                output: output.clone()
            })
        );
        assert_eq!(
            select_command_with_capability(&select0, output.clone(), "space-1", false),
            None,
            "capability-disabled select-0 has no route"
        );
        let send0 = InputSemanticAction::WorkspaceSend {
            index: 0,
            follow: true,
        };
        assert_eq!(
            send_command(&send0, "4242_7", output.clone(), "space-277"),
            Some(WorldCommand::SendNumbered {
                window: tiler_core::directional::WindowId("4242_7".to_owned()),
                output: output.clone(),
                index: 0,
                follow: true,
            })
        );
        assert_eq!(
            send_command_with_capability(&send0, "4242_7", output.clone(), "space-277", true),
            Some(WorldCommand::SendNumbered {
                window: tiler_core::directional::WindowId("4242_7".to_owned()),
                output: output.clone(),
                index: 0,
                follow: true,
            })
        );
        assert_eq!(
            send_command_with_capability(&send0, "4242_7", output.clone(), "space-277", false),
            None,
            "capability-disabled send-0 has no route"
        );
        let stay0 = InputSemanticAction::WorkspaceSend {
            index: 0,
            follow: false,
        };
        assert_eq!(
            send_command(&stay0, "4242_7", output.clone(), "space-277"),
            None,
            "stay-0 never maps"
        );
        assert_eq!(
            send_command_with_capability(&stay0, "4242_7", output.clone(), "space-277", true),
            None,
            "stay-0 never maps"
        );
        assert!(crate::workspace_input::is_armed_select(&select0));
        assert!(crate::workspace_input::is_armed_send(&send0));
        assert!(crate::workspace_input::is_armed_select_with(&select0, true));
        assert!(crate::workspace_input::is_armed_send_with(&send0, true));
        assert!(!crate::workspace_input::is_armed_send_with(&stay0, true));
    }
}

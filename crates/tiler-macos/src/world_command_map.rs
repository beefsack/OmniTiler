#![forbid(unsafe_code)]

//! World input-to-command mapping (pure translation).
//!
//! Split from [`crate::world_runtime`]: which typed [`InputSemanticAction`]
//! becomes which [`WorldCommand`](tiler_core::world_report::WorldCommand).
//! No sequencing, no core contact, no actuation — the armed select-only
//! subset maps to planner-owned select commands (positional numbered
//! selects over the adopted ring, relative/previous/trailing unchanged),
//! the send-shaped actions map to positional planner-owned send commands
//! for the admission-resolved explicit source, every other supported
//! tiling action maps to its existing core command shape, and anything
//! else has no route (capability-refused by the caller). Pure over
//! retained input state except the keyboard-resize repeat cursor it
//! advances. The core resolves all targeting from these commands; the
//! adapter never mints logical ids or re-resolves ordinals.

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
pub(crate) const OP_SELECT: &str = "select";
pub(crate) const OP_SEND: &str = "send";
pub(crate) const OP_TERMINATE: &str = "terminate";
/// World gesture lifecycle op for the ACTIVE Meta+left drag route
/// (Started/Updated/Released/Cancelled through gesture ingress plus
/// correlated reply results). Strict-v2 only: no Operation(DragPreview)
/// or Operation(DragDrop) route remains.
pub(crate) const OP_DRAG_GESTURE: &str = "drag-gesture";

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
/// Numbered selects (`1..=9`) ride positional
/// [`WorldCommand::SelectNumbered`]: the ordinal resolves core-side
/// against the adopted ring, whose order tracks native Desktop order
/// under host-owned adoption (initial adoption preserves native order;
/// outside-created desktops bind at native-order position) — so a
/// positional resolution names the same Desktop-N the admission pinned
/// natively, with no adapter-side logical mapping table. `output` is
/// `None` for ordinary tap commands (the core resolves the switch target
/// from cursor/focus); indicator taps that carry an explicit semantic
/// output pass `Some`. Relative and previous selects keep their shared core
/// semantics untouched. Trailing `index 0` resolves core-side to
/// reuse-or-create (`SelectTrailing`), never to an explicit guess.
pub(crate) fn select_command(
    action: &InputSemanticAction,
    output: Option<OutputId>,
) -> Option<WorldCommand> {
    select_command_with_capability(
        action,
        output,
        crate::workspace_input::TRAILING_CAPABILITY_ENABLED,
    )
}

/// Capability-explicit select mapping: the production route above with
/// `trailing_enabled` threaded from the single capability gate. Trailing
/// `index 0` resolves core-side to reuse-or-create (`SelectTrailing`),
/// never to an explicit guess: the native drain ensures the trailing
/// Space first (reuse proven-empty last, else exact append) and reports
/// back under the claim's operation identity.
pub(crate) fn select_command_with_capability(
    action: &InputSemanticAction,
    output: Option<OutputId>,
    trailing_enabled: bool,
) -> Option<WorldCommand> {
    match action {
        InputSemanticAction::IndicatorSelectWorkspace { output, workspace } => {
            Some(WorldCommand::SelectWorkspace {
                output: OutputId(output.clone()),
                workspace: workspace.clone(),
            })
        }
        InputSemanticAction::WorkspaceSelect { index } if (1..=9).contains(index) => {
            Some(WorldCommand::SelectNumbered {
                output: output.clone(),
                index: *index,
            })
        }
        InputSemanticAction::WorkspaceRelative { prev } => Some(WorldCommand::SelectRelative {
            output: output.clone(),
            delta: if *prev { -1 } else { 1 },
        }),
        InputSemanticAction::WorkspacePrevious => Some(WorldCommand::SelectPrevious {
            output: output.clone(),
        }),
        InputSemanticAction::WorkspaceSelect { index: 0 } if trailing_enabled => {
            Some(WorldCommand::SelectTrailing {
                output: output.clone(),
            })
        }
        _ => None,
    }
}

/// Map one send-shaped action to its planner-owned world command for the
/// focused-window `window` subject, with a core-owned implicit source.
/// Numbered sends (`1..=9`, follow or stay) and trailing send-0 follow ride
/// positional [`WorldCommand::SendNumbered`]: the ordinal resolves core-side
/// against the scoped send ring for the core-resolved source, whose order
/// tracks native Desktop order under host-owned adoption. The source stays
/// [`SendSource::HomeThenCachedLive`](tiler_core::world_report::SendSource)
/// (the keyboard send route: retained pre-observation home first, never an
/// adapter output decision and never a native-domain default). The adapter
/// pins only subject lifetime/generation/source freshness natively; targeting
/// is entirely core-owned. Non-send-shaped actions (relative sends, outputs,
/// selects, basics) have no send route (`None`). Trailing send-0 follow
/// maps only when `trailing_enabled`; stay-0 never maps.
/// Offline test seam over [`send_command_implicit_with_capability`].
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn send_command_implicit(
    action: &InputSemanticAction,
    window: &str,
) -> Option<WorldCommand> {
    send_command_implicit_with_capability(
        action,
        window,
        crate::workspace_input::TRAILING_CAPABILITY_ENABLED,
    )
}

/// Capability-explicit implicit-source send mapping: the production route
/// above with `trailing_enabled` threaded from the single capability gate.
pub(crate) fn send_command_implicit_with_capability(
    action: &InputSemanticAction,
    window: &str,
    trailing_enabled: bool,
) -> Option<WorldCommand> {
    use tiler_core::world_report::SendSource;
    match action {
        InputSemanticAction::WorkspaceSend { index, follow } if (1..=9).contains(index) => {
            Some(WorldCommand::SendNumbered {
                window: tiler_core::directional::WindowId(window.to_owned()),
                source: SendSource::HomeThenCachedLive,
                index: *index,
                follow: *follow,
            })
        }
        InputSemanticAction::WorkspaceSend {
            index: 0,
            follow: true,
        } if trailing_enabled => Some(WorldCommand::SendNumbered {
            window: tiler_core::directional::WindowId(window.to_owned()),
            source: SendSource::HomeThenCachedLive,
            index: 0,
            follow: true,
        }),
        _ => None,
    }
}

/// Map one send-shaped action to its planner-owned world command for the
/// focused-window `window` subject with an explicit source (offline test
/// seam: production resolves the source core-side through
/// [`send_command_implicit`]).
/// Capability-explicit send mapping: the production route above with
/// `trailing_enabled` threaded from the single capability gate.
/// Non-test builds route through the production wrapper below; the
/// positional form stays as the offline test seam.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn send_command(
    action: &InputSemanticAction,
    window: &str,
    output: OutputId,
) -> Option<WorldCommand> {
    send_command_with_capability(
        action,
        window,
        output,
        crate::workspace_input::TRAILING_CAPABILITY_ENABLED,
    )
}

/// Capability-explicit send mapping: the production route above with
/// `trailing_enabled` threaded from the single capability gate.
pub(crate) fn send_command_with_capability(
    action: &InputSemanticAction,
    window: &str,
    output: OutputId,
    trailing_enabled: bool,
) -> Option<WorldCommand> {
    use tiler_core::world_report::SendSource;
    match action {
        InputSemanticAction::WorkspaceSend { index, follow } if (1..=9).contains(index) => {
            Some(WorldCommand::SendNumbered {
                window: tiler_core::directional::WindowId(window.to_owned()),
                source: SendSource::Explicit(output),
                index: *index,
                follow: *follow,
            })
        }
        InputSemanticAction::WorkspaceSend {
            index: 0,
            follow: true,
        } if trailing_enabled => Some(WorldCommand::SendNumbered {
            window: tiler_core::directional::WindowId(window.to_owned()),
            source: SendSource::Explicit(output),
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
        _ => None,
    }
}

#[cfg(test)]
mod trailing_command_tests {
    use super::*;
    use crate::runtime_types::InputSemanticAction;

    #[test]
    fn trailing_capability_maps_through_same_production_route_when_armed() {
        use tiler_core::world_report::SendSource;
        let output = OutputId(crate::topology::output_key(1));
        // Armed production: the SAME production mapping routes select-0
        // to SelectTrailing and send-0 follow to SendNumbered 0, while
        // stay-0 never maps. Numbered selects ride positional
        // SelectNumbered over the adopted ring (Desktop-N order), never a
        // pinned logical guess. Ordinary taps carry no adapter output
        // (core resolves cursor/focus); explicit semantic outputs ride
        // `Some`.
        let select1 = InputSemanticAction::WorkspaceSelect { index: 1 };
        assert_eq!(
            select_command(&select1, None),
            Some(WorldCommand::SelectNumbered {
                output: None,
                index: 1,
            })
        );
        assert_eq!(
            select_command(&select1, Some(output.clone())),
            Some(WorldCommand::SelectNumbered {
                output: Some(output.clone()),
                index: 1,
            })
        );
        let select0 = InputSemanticAction::WorkspaceSelect { index: 0 };
        assert_eq!(
            select_command(&select0, None),
            Some(WorldCommand::SelectTrailing { output: None })
        );
        assert_eq!(
            select_command_with_capability(&select0, None, true),
            Some(WorldCommand::SelectTrailing { output: None })
        );
        assert_eq!(
            select_command_with_capability(&select0, None, false),
            None,
            "capability-disabled select-0 has no route"
        );
        let send1 = InputSemanticAction::WorkspaceSend {
            index: 1,
            follow: true,
        };
        assert_eq!(
            send_command(&send1, "4242_7", output.clone()),
            Some(WorldCommand::SendNumbered {
                window: tiler_core::directional::WindowId("4242_7".to_owned()),
                source: SendSource::Explicit(output.clone()),
                index: 1,
                follow: true,
            })
        );
        let send0 = InputSemanticAction::WorkspaceSend {
            index: 0,
            follow: true,
        };
        assert_eq!(
            send_command(&send0, "4242_7", output.clone()),
            Some(WorldCommand::SendNumbered {
                window: tiler_core::directional::WindowId("4242_7".to_owned()),
                source: SendSource::Explicit(output.clone()),
                index: 0,
                follow: true,
            })
        );
        assert_eq!(
            send_command_with_capability(&send0, "4242_7", output.clone(), true),
            Some(WorldCommand::SendNumbered {
                window: tiler_core::directional::WindowId("4242_7".to_owned()),
                source: SendSource::Explicit(output.clone()),
                index: 0,
                follow: true,
            })
        );
        assert_eq!(
            send_command_with_capability(&send0, "4242_7", output.clone(), false),
            None,
            "capability-disabled send-0 has no route"
        );
        let stay0 = InputSemanticAction::WorkspaceSend {
            index: 0,
            follow: false,
        };
        assert_eq!(
            send_command(&stay0, "4242_7", output.clone()),
            None,
            "stay-0 never maps"
        );
        assert_eq!(
            send_command_with_capability(&stay0, "4242_7", output.clone(), true),
            None,
            "stay-0 never maps"
        );
        assert!(crate::workspace_input::is_armed_select(&select0));
        assert!(crate::workspace_input::is_armed_send(&send0));
        assert!(crate::workspace_input::is_armed_select_with(&select0, true));
        assert!(crate::workspace_input::is_armed_send_with(&send0, true));
        assert!(!crate::workspace_input::is_armed_send_with(&stay0, true));
        // Implicit production mapping resolves the source core-side: no
        // adapter output decision, no native-domain default.
        assert_eq!(
            send_command_implicit(&send1, "4242_7"),
            Some(WorldCommand::SendNumbered {
                window: tiler_core::directional::WindowId("4242_7".to_owned()),
                source: SendSource::HomeThenCachedLive,
                index: 1,
                follow: true,
            })
        );
        assert_eq!(
            send_command_implicit_with_capability(&send0, "4242_7", true),
            Some(WorldCommand::SendNumbered {
                window: tiler_core::directional::WindowId("4242_7".to_owned()),
                source: SendSource::HomeThenCachedLive,
                index: 0,
                follow: true,
            })
        );
        assert_eq!(
            send_command_implicit_with_capability(&send0, "4242_7", false),
            None,
            "capability-disabled send-0 has no route"
        );
        assert_eq!(
            send_command_implicit_with_capability(&stay0, "4242_7", true),
            None,
            "stay-0 never maps"
        );
    }

    #[test]
    fn indicator_exact_id_maps_to_select_workspace() {
        let action = InputSemanticAction::IndicatorSelectWorkspace {
            output: "out-a".to_owned(),
            workspace: "ws-spare".to_owned(),
        };
        assert!(crate::workspace_input::is_armed_select(&action));
        assert!(crate::workspace_input::workspace_unsupported_reason(&action).is_none());
        assert_eq!(
            crate::workspace_input::workspace_action_id(&action),
            Some("indicator-select-workspace")
        );
        assert_eq!(
            select_command(&action, Some(OutputId(crate::topology::output_key(1)))),
            Some(WorldCommand::SelectWorkspace {
                output: OutputId("out-a".to_owned()),
                workspace: "ws-spare".to_owned(),
            })
        );
        assert!(
            crate::runtime_thread::admit_select_claim(
                true,
                crate::runtime_types::ClaimedSelect::indicator(action, 0, 1, 1)
            )
            .is_ok()
        );
    }

    #[test]
    fn indicator_spare_click_selects_exact_id_with_creation_desire_and_plan() {
        use crate::focus_route::FocusRoute;
        use crate::runtime_types::{ClaimedSelect, NativeContext, NativeDomain, PHASE1_GENERATION};
        use crate::workspace_indicator::{
            IndicatorClickKind, IndicatorMode, SpaceOccupancy, project_core_indicator,
            verify_core_row_click,
        };
        use crate::workspace_select::parse_native_binding;
        use crate::world_runtime::WorldSession;
        use crate::world_sequencer::{ObservationClock, ReplyVerdict};
        use std::sync::Arc;
        use tiler_core::directional::{OutputId, WorkspaceId};
        use tiler_core::geometry::Rect;
        use tiler_core::size_hints::FixedSizePredicate;
        use tiler_core::world_engine::{WorldEngine, WorldOutcome};
        use tiler_core::world_report::TransferKind;

        fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
            Rect { x, y, w, h }
        }
        // Offline spare fixture: one screen, both native spaces occupied
        // (win-a on space-1, win-b on space-2), so the lifecycle appends a
        // true unbound trailing spare. The core adopts the bound rows and
        // mints the spare; every id below comes from the accepted reply,
        // never a literal.
        fn spare_observation(
            incarnation: &str,
            observation_seq: u64,
            request_seq: u64,
            request_id: &str,
            command_seq: Option<u64>,
            command: Option<WorldCommand>,
        ) -> tiler_core::world_report::WorldReport {
            use tiler_core::directional::WindowId;
            use tiler_core::world_report::{
                PresentationIngress, WORLD_CONTRACT_VERSION, WorldReport,
            };
            WorldReport {
                contract_version: WORLD_CONTRACT_VERSION,
                owner: "owner-a".to_owned(),
                adapter_incarnation: incarnation.to_owned(),
                core_generation: PHASE1_GENERATION.to_owned(),
                observation_seq,
                request_seq,
                request_id: request_id.to_owned(),
                command_seq,
                command,
                workspace_mode: tiler_core::workspace_scope::WorkspaceMode::PerOutputLocal,
                screens: vec![tiler_core::world_report::ScreenReport {
                    output: OutputId("out-a".to_owned()),
                    full_rect: Some(rect(0, 0, 800, 600)),
                    work_area: Some(rect(0, 0, 800, 560)),
                    gap: 0,
                    outer_gap: 0,
                    complete: true,
                    primary: true,
                }],
                windows: vec![
                    tiler_core::world_report::WindowFact {
                        window: WindowId("win-a".to_owned()),
                        output: OutputId("out-a".to_owned()),
                        rect: Some(rect(0, 0, 40, 90)),
                        floating: false,
                        fit_excluded: false,
                        fullscreen: false,
                        maximized: false,
                        sticky: false,
                        fixed_auto: false,
                        fixed_suppress: false,
                        hints: tiler_core::size_hints::WindowSizeHints::none(),
                        minimized: false,
                        observed_workspace: None,
                        native_workspace: Some("space-1".to_owned()),
                    },
                    tiler_core::world_report::WindowFact {
                        window: WindowId("win-b".to_owned()),
                        output: OutputId("out-a".to_owned()),
                        rect: Some(rect(100, 0, 40, 90)),
                        floating: false,
                        fit_excluded: false,
                        fullscreen: false,
                        maximized: false,
                        sticky: false,
                        fixed_auto: false,
                        fixed_suppress: false,
                        hints: tiler_core::size_hints::WindowSizeHints::none(),
                        minimized: false,
                        observed_workspace: None,
                        native_workspace: Some("space-2".to_owned()),
                    },
                ],
                views: Vec::new(),
                closed: vec![],
                host_owned_workspace_domains: true,
                equal_workspace_count: false,
                birth_clear: false,
                native: vec![tiler_core::world_report::NativeObservation {
                    output: OutputId("out-a".to_owned()),
                    native_order: vec!["space-1".to_owned(), "space-2".to_owned()],
                    current_native: Some("space-1".to_owned()),
                    occupancy: vec![
                        tiler_core::world_report::NativeOccupancy {
                            native: "space-1".to_owned(),
                            count: None,
                        },
                        tiler_core::world_report::NativeOccupancy {
                            native: "space-2".to_owned(),
                            count: None,
                        },
                    ],
                }],
                fixed_size_predicate: tiler_core::size_hints::FixedSizePredicate::BothAxes,
                migration_source_refill:
                    tiler_core::output_selection::MigrationSourceRefill::default(),
                anchor_fallback: tiler_core::admission::AnchorFallback::default(),
                no_space_result: tiler_core::admission::NoSpaceResult::default(),
                born_maximized: tiler_core::born_maximized::BornMaximized::default(),
                switch_target: tiler_core::cursor_targeting::SwitchTarget::default(),
                fresh_placement: tiler_core::cursor_targeting::FreshPlacement::default(),
                focus_follows_cursor: false,
                cursor_warp: false,
                cursor: None,
                cursor_hit: None,
                focused_global: "win-a".to_owned(),
                action_foreground: None,
                action_cached: None,
                action_outputs: Vec::new(),
                default_tiled: true,
                inventory_complete: true,
                gesture_barrier: false,
                gesture: None,
                monotonic_ms: 0,
                frames: Vec::new(),
                member_attempts: Vec::new(),
                view_attempts: Vec::new(),
                clear_attempts: Vec::new(),
                creation_attempts: Vec::new(),
                presentation: {
                    let mut ingress = PresentationIngress::disabled();
                    ingress.foreground = "win-a".to_owned();
                    ingress.foreground_frame = Some(rect(0, 0, 40, 90));
                    ingress.border_enabled = true;
                    ingress.pager_frames = vec![
                        tiler_core::world_report::PresentationPagerFrame {
                            window: WindowId("win-a".to_owned()),
                            live: Some(rect(0, 0, 40, 90)),
                            float_frame: None,
                            tile_frame: None,
                        },
                        tiler_core::world_report::PresentationPagerFrame {
                            window: WindowId("win-b".to_owned()),
                            live: Some(rect(100, 0, 40, 90)),
                            float_frame: None,
                            tile_frame: None,
                        },
                    ];
                    ingress.pager_order =
                        vec![WindowId("win-a".to_owned()), WindowId("win-b".to_owned())];
                    ingress
                },
            }
        }

        // Observation through the session engine plus the sequencer flight:
        // the accepted reply names the trailing spare.
        let mut session = WorldSession::new(
            FixedSizePredicate::BothAxes,
            true,
            FocusRoute::Slps,
            Arc::new(ObservationClock::new()),
        );
        let incarnation = session.incarnation().to_owned();
        let ticket1 = session
            .sequencer()
            .try_begin_request(None)
            .expect("observation flight opens");
        let first = spare_observation(
            &incarnation,
            ticket1.observation_seq,
            ticket1.request_seq,
            &ticket1.request_id,
            ticket1.command_seq,
            None,
        );
        // Sanity: the standalone engine accepts the same fixture (pins the
        // shape to the engine contract, not the session harness).
        let mut probe = WorldEngine::new(PHASE1_GENERATION).expect("generation parses");
        assert!(
            matches!(probe.handle_report(&first), WorldOutcome::Accepted(_)),
            "spare fixture is engine-accepted"
        );
        let reply1 = match session.engine_mut().handle_report(&first) {
            WorldOutcome::Accepted(reply) => reply,
            other => panic!("expected accepted, got {other:?}"),
        };
        assert!(
            matches!(
                session.sequencer().accept_reply(&reply1),
                ReplyVerdict::Applied(_)
            ),
            "observation reply applies to the sequencer flight"
        );
        let trailing: Vec<String> = reply1
            .presentation
            .pager
            .iter()
            .flat_map(|screen| screen.workspaces.iter())
            .filter(|row| row.trailing)
            .map(|row| row.id.clone())
            .collect();
        assert_eq!(
            trailing.len(),
            1,
            "reply names exactly one trailing spare: {:?}",
            reply1
                .presentation
                .pager
                .iter()
                .flat_map(|screen| screen.workspaces.iter().map(|row| (
                    row.id.clone(),
                    row.ordinal,
                    row.trailing
                )))
                .collect::<Vec<_>>()
        );
        let spare = trailing.into_iter().next().expect("trailing spare");
        // Project the REAL reply pager plus its REAL desires: the spare
        // row selects like a bound row and carries the real spare id.
        let native_last = first.native[0]
            .native_order
            .last()
            .and_then(|binding| parse_native_binding(binding))
            .expect("fixture native last parses");
        let projection = project_core_indicator(
            &reply1.presentation.pager,
            &reply1.workspaces,
            1,
            IndicatorMode::All,
            &|_| SpaceOccupancy::Occupied,
            &|_| None,
            native_last,
        )
        .expect("real spare reply projects");
        let rendered = projection
            .rows
            .iter()
            .find(|row| row.core.as_ref().is_some_and(|core| core.spare))
            .expect("spare row projects");
        assert!(rendered.selectable, "spare row arms");
        assert_eq!(
            rendered.core.as_ref().expect("core").workspace,
            spare,
            "spare row binds the reply's real trailing id"
        );
        // Spare click verifies to the exact-id action (never positional,
        // never trailing-create).
        let click = crate::workspace_indicator::IndicatorClick {
            kind: IndicatorClickKind::Numbered,
            position: rendered.position,
            expected_sid: rendered.sid,
            ui_generation: 7,
            topology_revision: 9,
        };
        let action = verify_core_row_click(
            click,
            rendered,
            7,
            9,
            &reply1.core_generation,
            &reply1.presentation,
            &reply1.workspaces,
            &reply1.core_generation,
        )
        .expect("coherent spare click verifies");
        assert_eq!(
            action,
            InputSemanticAction::IndicatorSelectWorkspace {
                output: "out-a".to_owned(),
                workspace: spare.clone(),
            }
        );
        // The existing command route carries the exact id (no ordinal).
        let command = select_command(&action, Some(OutputId("out-a".to_owned())));
        assert_eq!(
            command,
            Some(WorldCommand::SelectWorkspace {
                output: OutputId("out-a".to_owned()),
                workspace: spare.clone(),
            })
        );
        // Dispatch the exact-id command: the core retains the View
        // transfer on the exact logical id and publishes the native
        // creation desire downstream (a desire record only — this offline
        // path creates no display).
        let ticket2 = session
            .sequencer()
            .try_begin_request(command.clone())
            .expect("command flight opens");
        let second = spare_observation(
            &incarnation,
            ticket2.observation_seq,
            ticket2.request_seq,
            &ticket2.request_id,
            ticket2.command_seq,
            command,
        );
        let reply2 = match session.engine_mut().handle_report(&second) {
            WorldOutcome::Accepted(reply) => reply,
            other => panic!("expected accepted, got {other:?}"),
        };
        assert!(
            matches!(
                session.sequencer().accept_reply(&reply2),
                ReplyVerdict::Applied(_)
            ),
            "command reply applies to the sequencer flight"
        );
        let transfer = session
            .engine_mut()
            .retained_transfers()
            .into_iter()
            .find(|target| {
                matches!(target.kind, TransferKind::View) && target.target_workspace == spare
            })
            .expect("exact-id View transfer retained");
        assert_eq!(transfer.target_output.0, "out-a");
        let creations: Vec<_> = reply2
            .native_creations
            .iter()
            .filter(|desire| desire.logical_workspace == spare)
            .collect();
        assert_eq!(
            creations.len(),
            1,
            "exactly one creation desire for the spare logical: {:?}",
            reply2.native_creations
        );
        assert_eq!(creations[0].target_output.0, "out-a");
        // The existing plan binder resolves the retained target to the
        // exact id/output/op with no binding (unbound spare) and the
        // creation desire attached — never an ordinal reinterpretation.
        let ctx = Arc::new(NativeContext {
            revision: 9,
            domains: vec![NativeDomain {
                cg_id: 1,
                uuid: None,
                output: OutputId("out-a".to_owned()),
                workspace: WorkspaceId("space-1".to_owned()),
                space: 1,
                current: true,
                readable: true,
                bounds: rect(0, 0, 800, 600),
                visible: rect(0, 0, 800, 560),
            }],
            skipped: Vec::new(),
        });
        let mut claim = ClaimedSelect::indicator(action, rendered.sid, 7, 9);
        crate::runtime_thread::bind_select_plan(
            &mut session,
            &ctx,
            &mut claim,
            &transfer.operation_id,
        );
        assert_eq!(claim.select_target_output.as_deref(), Some("out-a"));
        assert_eq!(
            claim.select_target_workspace.as_deref(),
            Some(spare.as_str())
        );
        assert_eq!(
            claim.select_native_binding, None,
            "unbound spare binds no SID"
        );
        assert_eq!(claim.select_baseline_sid, Some(1));
        assert_eq!(
            claim.select_creation_logical.as_deref(),
            Some(spare.as_str())
        );
        assert_eq!(claim.select_creation_output.as_deref(), Some("out-a"));
        assert_eq!(
            claim.select_creation_operation,
            Some(creations[0].operation_id.clone()),
            "creation op rides the claim"
        );
        assert!(
            !transfer.operation_id.is_empty(),
            "retained transfer carries the binding operation identity"
        );
    }
}

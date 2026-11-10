#![forbid(unsafe_code)]

//! World input-to-command mapping (pure translation).
//!
//! Split from [`crate::world_runtime`]: which typed [`InputSemanticAction`]
//! becomes which [`WorldCommand`](tiler_core::world_report::WorldCommand).
//! No sequencing, no core contact, no actuation — the armed select-only
//! subset maps to planner-owned select commands (positional numbered
//! selects over the adopted ring, relative/previous/trailing unchanged),
//! the send-shaped actions map to positional planner-owned send commands
//! for the admission-resolved explicit source, directional focus/move map
//! to execution-time implicit directional commands (core-owned subject at
//! execution), every other supported tiling action maps to its existing
//! core command shape, and anything
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
/// [`SendSource::HomeThenAction`](tiler_core::world_report::SendSource)
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
                source: SendSource::HomeThenAction,
                index: *index,
                follow: *follow,
            })
        }
        InputSemanticAction::WorkspaceSend {
            index: 0,
            follow: true,
        } if trailing_enabled => Some(WorldCommand::SendNumbered {
            window: tiler_core::directional::WindowId(window.to_owned()),
            source: SendSource::HomeThenAction,
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

/// Map one supported tiling input to its world command plus its
/// diagnostic op token. Directional focus/move ride the execution-time
/// implicit directional commands (the core resolves the subject from
/// retained canonical focus at execution; the inner carries a placeholder
/// empty subject that never executes), resize and the toggles keep their
/// exact legacy [`CoreCommand`](tiler_core::boundary::CoreCommand) shapes under
/// [`WorldCommand::Operation`](tiler_core::world_report::WorldCommand).
/// Selects and unsupported actions have no operation route here (`None`).
pub(crate) fn operation_command(
    action: &InputSemanticAction,
    window: &str,
    retained: &mut RetainedCore,
) -> Option<(WorldCommand, &'static str)> {
    use tiler_core::boundary::CoreCommand;
    use tiler_core::directional::{SameAxisMove, WindowId};
    let empty = || WindowId(String::new());
    match action {
        InputSemanticAction::FocusStep { direction } => Some((
            WorldCommand::Implicit(Box::new(WorldCommand::FocusDirectional {
                window: empty(),
                direction: direction.as_str().to_owned(),
            })),
            OP_FOCUS,
        )),
        InputSemanticAction::MoveStep { direction } => Some((
            WorldCommand::Implicit(Box::new(WorldCommand::MoveDirectional {
                window: empty(),
                direction: direction.as_str().to_owned(),
                same_axis: SameAxisMove::default(),
            })),
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
                WorldCommand::Operation(CoreCommand::Resize {
                    window: window.to_owned(),
                    direction: direction.as_str().to_owned(),
                    mode: mode.as_str().to_owned(),
                    press_index,
                }),
                OP_RESIZE,
            ))
        }
        InputSemanticAction::ToggleFloat => Some((
            WorldCommand::Operation(CoreCommand::ToggleFloat {
                window: window.to_owned(),
                float_rect: None,
            }),
            OP_TOGGLE_FLOAT,
        )),
        InputSemanticAction::ToggleOrientation => Some((
            WorldCommand::Operation(CoreCommand::ToggleOrientation {
                window: window.to_owned(),
            }),
            OP_TOGGLE_ORIENTATION,
        )),
        _ => None,
    }
}

#[cfg(test)]
mod directional_codec_tests {
    use super::*;
    use std::collections::BTreeMap;

    /// Production-map codec regression: directional focus/move map to the
    /// execution-time implicit directional commands and survive the genuine
    /// v4 byte codec (the runtime dispatches through `codec_transact`,
    /// whose `Operation` encoder only accepts toggle-float/orientation/
    /// resize). The legacy `Operation(Focus/Move)` spelling failed here
    /// with `command-invalid`.
    #[test]
    fn directional_production_map_encodes_through_genuine_v4_codec() {
        use tiler_core::directional::{OutputId, WindowId, WorkspaceId};
        use tiler_core::geometry::Rect;
        use tiler_core::session::DomainKey;
        use tiler_core::size_hints::WindowSizeHints;
        use tiler_core::world_report::{HoldReason, WorldCommand};
        use tiler_core::world_report_v4_checks::validate_report_v4_reason;
        use tiler_protocol::world_protocol::{
            CreationReceiptWire, CursorHitWire, DecodedReportV4, GestureIngressWire,
            ReportWireOptionals, decode_report_v4, encode_report_v4,
        };

        use crate::runtime_core::{LifetimeMemory, RetainedCore};
        use crate::runtime_types::{
            InputDirection, NativeContext, NativeDomain, ObservedWindow, WindowRawFacts, window_id,
        };
        use crate::world_builder::{ReportInput, build_report};

        fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
            Rect { x, y, w, h }
        }
        let output = OutputId(crate::topology::output_key(1));
        let ctx = NativeContext {
            revision: 9,
            domains: vec![NativeDomain {
                cg_id: 1,
                uuid: None,
                output: output.clone(),
                workspace: WorkspaceId(crate::topology::workspace_key(1)),
                space: 1,
                current: true,
                readable: true,
                bounds: rect(0, 0, 800, 600),
                visible: rect(0, 0, 800, 560),
            }],
            skipped: Vec::new(),
        };
        let domain = DomainKey {
            output: output.clone(),
            workspace: WorkspaceId(crate::topology::workspace_key(1)),
        };
        let members: BTreeMap<DomainKey, Vec<(u32, ObservedWindow)>> = BTreeMap::from([(
            domain,
            vec![
                (
                    4242,
                    ObservedWindow {
                        wid: 7,
                        x: 0,
                        y: 0,
                        w: 390,
                        h: 560,
                        floating: false,
                        fullscreen: false,
                        hints: WindowSizeHints::none(),
                    },
                ),
                (
                    4242,
                    ObservedWindow {
                        wid: 8,
                        x: 410,
                        y: 0,
                        w: 390,
                        h: 560,
                        floating: false,
                        fullscreen: false,
                        hints: WindowSizeHints::none(),
                    },
                ),
            ],
        )]);
        let mut raw: BTreeMap<String, WindowRawFacts> = BTreeMap::new();
        for pairs in members.values() {
            for (pid, window) in pairs {
                raw.insert(
                    window_id(*pid, window.wid),
                    WindowRawFacts {
                        role: Some(crate::classifier::ROLE_WINDOW.to_owned()),
                        subrole: Some(crate::classifier::SUBROLE_STANDARD.to_owned()),
                        modal: Some(false),
                        resizable: Some(true),
                        layer: Some(0),
                        app_id: Some("com.example.app".to_owned()),
                        activation_policy: Some(
                            tiler_core::world_report::ActivationPolicy::Regular,
                        ),
                        host_hidden: Some(false),
                    },
                );
            }
        }
        let memory = LifetimeMemory::empty();
        let unreadable: BTreeMap<DomainKey, HoldReason> = BTreeMap::new();
        let req_id = "req-7".to_owned();

        let mut retained = RetainedCore::default();
        // The approved v4 shapes: execution-time implicit subjects (the
        // core resolves the window from retained canonical focus at
        // execution; the inner carries a placeholder empty subject that
        // never executes). Same-axis stays the default
        // (`group-with-neighbor`, matching the prior legacy intent).
        let expected_focus = WorldCommand::Implicit(Box::new(WorldCommand::FocusDirectional {
            window: WindowId(String::new()),
            direction: "left".to_owned(),
        }));
        let expected_move = WorldCommand::Implicit(Box::new(WorldCommand::MoveDirectional {
            window: WindowId(String::new()),
            direction: "right".to_owned(),
            same_axis: tiler_core::directional::SameAxisMove::default(),
        }));
        for (action, op, expected) in [
            (
                InputSemanticAction::FocusStep {
                    direction: InputDirection::Left,
                },
                OP_FOCUS,
                expected_focus,
            ),
            (
                InputSemanticAction::MoveStep {
                    direction: InputDirection::Right,
                },
                OP_MOVE,
                expected_move,
            ),
        ] {
            let (command, token) = operation_command(&action, "4242_7", &mut retained)
                .unwrap_or_else(|| panic!("{action:?} has an operation route"));
            assert_eq!(token, op);
            assert_eq!(
                command, expected,
                "{action:?} maps to the approved implicit directional shape"
            );
            assert!(
                tiler_core::world_report::valid_command(&command),
                "{op} mapped command validates"
            );
            let world = command;
            let mut input = ReportInput::new(
                "adapter-1",
                "gen-1",
                &ctx,
                7,
                7,
                &req_id,
                &members,
                &memory,
                &[],
                &unreadable,
                &[],
                "4242_7",
                false,
            );
            input.command_seq = Some(7);
            input.command = Some(world.clone());
            input.raw = &raw;
            let report = build_report(&mut input);
            assert!(
                validate_report_v4_reason(&report).is_none(),
                "{op} production report validates"
            );
            let decoded = DecodedReportV4 {
                wire: ReportWireOptionals {
                    cursor_hit: match &report.cursor_hit {
                        None => CursorHitWire::Absent,
                        Some(_) => CursorHitWire::Id,
                    },
                    monotonic_ms_present: report.monotonic_ms != 0,
                    frame_attempts_present: !report.frames.is_empty(),
                    member_attempts_present: !report.member_attempts.is_empty(),
                    view_attempts_present: !report.view_attempts.is_empty(),
                    clear_attempts_present: !report.clear_attempts.is_empty(),
                    creation_attempts_present: !report.creation_attempts.is_empty(),
                    gesture_ingress: match &report.gesture {
                        None => GestureIngressWire::Absent,
                        Some(_) => GestureIngressWire::Present,
                    },
                },
                creation_receipts: report
                    .creation_attempts
                    .iter()
                    .map(|row| match &row.created_native {
                        None => CreationReceiptWire::Null,
                        Some(_) => CreationReceiptWire::Id,
                    })
                    .collect(),
                report,
            };
            let wire = encode_report_v4(&decoded).expect(
                "production-mapped directional command encodes through the genuine v4 codec",
            );
            let back = decode_report_v4(&wire).expect("round-trips");
            assert_eq!(back.report.command, Some(world), "{op} round-trips");
        }
    }

    /// Exhausted-edge execution proof through the real engine: the
    /// production-mapped implicit focus step reconciles its observation
    /// (Accepted envelope) while the command itself refuses with typed
    /// `Rejected(OperationRefused)` and no focus effect. Accepted envelope
    /// != command success. The ordinary (non-edge) control steps plan with
    /// a correlated explicit-focus effect and no command result. The refused
    /// sequence is consumed at-most-once: replaying its command_seq reports
    /// CommandConsumed instead of re-executing.
    #[test]
    fn directional_edge_is_accepted_envelope_with_typed_rejection() {
        use tiler_core::directional::{OutputId, WindowId, WorkspaceId};
        use tiler_core::geometry::Rect;
        use tiler_core::session::DomainKey;
        use tiler_core::size_hints::WindowSizeHints;
        use tiler_core::world_engine::{WorldEngine, WorldOutcome};
        use tiler_core::world_report::{
            CommandRejectKind, CommandResult, EffectKind, HoldReason, WorldCommand, WorldReportV4,
        };
        use tiler_core::world_report_v4_checks::validate_report_v4_reason;
        use tiler_protocol::world_protocol::{
            DecodedReplyV4, ReplyWireOptionals, decode_reply_v4_for_host, encode_reply_v4,
        };

        use crate::runtime_core::{LifetimeMemory, RetainedCore};
        use crate::runtime_types::{
            InputDirection, NativeContext, NativeDomain, ObservedWindow, WindowRawFacts,
        };
        use crate::world_builder::{ReportInput, build_report};

        fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
            Rect { x, y, w, h }
        }
        // Genuine known admit shape: mirrors the working
        // focus_gaps_trace startup (1440x900 display, three columns
        // tiling the inset area), so the engine builds the same row
        // tree the integration fixture walks. Do not invent geometry here:
        // the engine's split placement follows the admitted shape.
        fn observed(wid: u32, x: i32, w: i32) -> ObservedWindow {
            ObservedWindow {
                wid,
                x,
                y: 8,
                w,
                h: 884,
                floating: false,
                fullscreen: false,
                hints: WindowSizeHints::none(),
            }
        }
        let output = OutputId(crate::topology::output_key(1));
        let ctx = NativeContext {
            revision: 9,
            domains: vec![NativeDomain {
                cg_id: 1,
                uuid: None,
                output: output.clone(),
                workspace: WorkspaceId(crate::topology::workspace_key(1)),
                space: 1,
                current: true,
                readable: true,
                bounds: rect(0, 0, 1440, 900),
                visible: rect(0, 0, 1440, 900),
            }],
            skipped: Vec::new(),
        };
        let domain = DomainKey {
            output: output.clone(),
            workspace: WorkspaceId(crate::topology::workspace_key(1)),
        };
        let members: BTreeMap<DomainKey, Vec<(u32, ObservedWindow)>> = BTreeMap::from([(
            domain,
            vec![
                (4242, observed(7, 8, 466)),
                (4242, observed(8, 482, 466)),
                (4242, observed(9, 956, 468)),
            ],
        )]);
        let mut raw: BTreeMap<String, WindowRawFacts> = BTreeMap::new();
        for pairs in members.values() {
            for (pid, window) in pairs {
                raw.insert(
                    crate::runtime_types::window_id(*pid, window.wid),
                    WindowRawFacts {
                        role: Some(crate::classifier::ROLE_WINDOW.to_owned()),
                        subrole: Some(crate::classifier::SUBROLE_STANDARD.to_owned()),
                        modal: Some(false),
                        resizable: Some(true),
                        layer: Some(0),
                        app_id: Some("com.example.app".to_owned()),
                        activation_policy: Some(
                            tiler_core::world_report::ActivationPolicy::Regular,
                        ),
                        host_hidden: Some(false),
                    },
                );
            }
        }
        let memory = LifetimeMemory::empty();
        let unreadable: BTreeMap<DomainKey, HoldReason> = BTreeMap::new();

        fn command_report(
            ctx: &NativeContext,
            members: &BTreeMap<DomainKey, Vec<(u32, ObservedWindow)>>,
            raw: &BTreeMap<String, WindowRawFacts>,
            memory: &LifetimeMemory,
            unreadable: &BTreeMap<DomainKey, HoldReason>,
            obs: u64,
            req: u64,
            focused: &str,
            seq: Option<u64>,
            command: Option<WorldCommand>,
        ) -> WorldReportV4 {
            let req_id = format!("req-{req}");
            let mut input = ReportInput::new(
                "adapter-1",
                "gen-1",
                ctx,
                obs,
                req,
                &req_id,
                members,
                memory,
                &[],
                unreadable,
                &[],
                focused,
                false,
            );
            input.command_seq = seq;
            input.command = command;
            input.raw = raw;
            let report = build_report(&mut input);
            assert!(
                validate_report_v4_reason(&report).is_none(),
                "report validates"
            );
            report
        }

        let mut retained = RetainedCore::default();
        let mut engine = WorldEngine::new_test_bound_with_capabilities(
            "gen-1",
            "adapter-1",
            crate::world_connection::macos_capabilities(),
        )
        .expect("production capabilities bind");
        let right = InputSemanticAction::FocusStep {
            direction: InputDirection::Right,
        };

        // Admit with native focus on win1.
        let first = command_report(
            &ctx,
            &members,
            &raw,
            &memory,
            &unreadable,
            1,
            1,
            "4242_7",
            None,
            None,
        );
        match engine.handle_report(&first) {
            WorldOutcome::Accepted(_) => {}
            other => panic!("admit accepted, got {other:?}"),
        }

        // Control 1: win1 -> win2 plans with one correlated focus effect
        // and no command result (tiled success carries no result: the
        // effect IS the outcome).
        let (command, op) =
            operation_command(&right, "4242_7", &mut retained).expect("focus route");
        assert_eq!(op, OP_FOCUS);
        let second = command_report(
            &ctx,
            &members,
            &raw,
            &memory,
            &unreadable,
            2,
            2,
            "4242_7",
            Some(2),
            Some(command),
        );
        let reply = match engine.handle_report(&second) {
            WorldOutcome::Accepted(reply) => reply,
            other => panic!("control step accepted, got {other:?}"),
        };
        assert!(
            reply.command_result.is_none(),
            "tiled success carries no result, got {:?}",
            reply.command_result
        );
        let focus: Vec<_> = reply
            .effects
            .iter()
            .filter(|effect| effect.kind == EffectKind::Focus)
            .collect();
        assert_eq!(focus.len(), 1, "one focus effect, got {:?}", reply.effects);
        assert!(focus[0].is_explicit_focus_for(2), "correlated to seq 2");
        assert_eq!(focus[0].target, WindowId("4242_8".to_owned()));

        // Control 2: arrival on win2, then win2 -> win3.
        let (command, _) = operation_command(&right, "4242_8", &mut retained).expect("focus route");
        let third = command_report(
            &ctx,
            &members,
            &raw,
            &memory,
            &unreadable,
            3,
            3,
            "4242_8",
            Some(3),
            Some(command),
        );
        let reply = match engine.handle_report(&third) {
            WorldOutcome::Accepted(reply) => reply,
            other => panic!("control step accepted, got {other:?}"),
        };
        assert!(
            reply.command_result.is_none(),
            "tiled success carries no result, got {:?}",
            reply.command_result
        );
        let focus: Vec<_> = reply
            .effects
            .iter()
            .filter(|effect| effect.kind == EffectKind::Focus)
            .collect();
        assert_eq!(focus.len(), 1, "one focus effect, got {:?}", reply.effects);
        assert!(focus[0].is_explicit_focus_for(3), "correlated to seq 3");
        assert_eq!(focus[0].target, WindowId("4242_9".to_owned()));

        // Exhausted edge: arrival on win3, then win3 + right retains.
        // Accepted envelope, typed Rejected(OperationRefused), no focus
        // effect, sequence consumed.
        let (command, _) = operation_command(&right, "4242_9", &mut retained).expect("focus route");
        let edge_command = command.clone();
        let fourth = command_report(
            &ctx,
            &members,
            &raw,
            &memory,
            &unreadable,
            4,
            4,
            "4242_9",
            Some(4),
            Some(command),
        );
        let reply = match engine.handle_report(&fourth) {
            WorldOutcome::Accepted(reply) => reply,
            other => panic!("edge step reconciles Accepted, got {other:?}"),
        };
        assert_eq!(
            reply.command_result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::OperationRefused,
            }),
            "exhausted edge refuses typed operation-refused"
        );
        assert!(
            !reply.command_consumed,
            "fresh refusal is not a consumed replay"
        );
        assert!(
            reply
                .effects
                .iter()
                .all(|effect| effect.kind != EffectKind::Focus),
            "no focus effect at the exhausted edge: {:?}",
            reply.effects
        );
        // The typed refusal survives the genuine reply codec.
        let suspended = reply.presentation.suspended_output.is_some();
        let wire = encode_reply_v4(&DecodedReplyV4 {
            reply,
            wire: ReplyWireOptionals {
                suspended_output_present: suspended,
            },
        })
        .expect("edge reply encodes");
        let back = decode_reply_v4_for_host(&wire, &crate::world_connection::macos_capabilities())
            .expect("edge reply decodes");
        assert_eq!(
            back.reply.command_result,
            Some(CommandResult::Rejected {
                kind: CommandRejectKind::OperationRefused,
            }),
            "typed refusal round-trips the wire"
        );
        assert!(
            back.reply
                .effects
                .iter()
                .all(|effect| effect.kind != EffectKind::Focus),
            "no focus effect after wire round-trip: {:?}",
            back.reply.effects
        );
        // At-most-once: replaying the refused seq never re-executes.
        let replay = command_report(
            &ctx,
            &members,
            &raw,
            &memory,
            &unreadable,
            5,
            5,
            "4242_9",
            Some(4),
            Some(edge_command),
        );
        assert!(
            matches!(
                engine.handle_report(&replay),
                WorldOutcome::CommandConsumed(_)
            ),
            "refused seq replays as CommandConsumed"
        );
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
                source: SendSource::HomeThenAction,
                index: 1,
                follow: true,
            })
        );
        assert_eq!(
            send_command_implicit_with_capability(&send0, "4242_7", true),
            Some(WorldCommand::SendNumbered {
                window: tiler_core::directional::WindowId("4242_7".to_owned()),
                source: SendSource::HomeThenAction,
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
        use crate::runtime_core::LifetimeMemory;
        use crate::runtime_types::{
            ClaimedSelect, NativeContext, NativeDomain, ObservedWindow, WindowRawFacts, window_id,
        };
        use crate::workspace_indicator::{
            IndicatorClickKind, IndicatorMode, SpaceOccupancy, project_core_indicator,
            verify_core_row_click,
        };
        use crate::workspace_select::parse_native_binding;
        use crate::world_builder::{ReportInput, build_report};
        use crate::world_runtime::WorldSession;
        use crate::world_sequencer::{ObservationClock, ReplyVerdict};
        use std::collections::BTreeMap;
        use std::sync::Arc;
        use tiler_core::directional::{OutputId, WorkspaceId};
        use tiler_core::geometry::Rect;
        use tiler_core::session::DomainKey;
        use tiler_core::size_hints::WindowSizeHints;
        use tiler_core::world_engine::{WorldEngine, WorldOutcome};
        use tiler_core::world_report::TransferKind;

        fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
            Rect { x, y, w, h }
        }
        // Offline spare fixture via the genuine v4 production report
        // builder: one screen, both native spaces occupied (4242_7 on
        // space-1, 4242_9 on space-2), so the lifecycle appends a true
        // unbound trailing spare. The core adopts the bound rows and mints
        // the spare; every id below comes from the accepted reply, never
        // a literal. Raw captures are explicit synthetic fixture facts
        // (ordinary AXWindow/AXStandardWindow, non-modal, resizable, base
        // layer, regular app, visible) — never policy derivations — so the
        // rows stay support-consistent with the production macOS
        // capabilities (unavailable facts stay null via
        // `collect_window_facts`).
        fn spare_ctx(output: &OutputId) -> NativeContext {
            NativeContext {
                revision: 9,
                domains: vec![
                    NativeDomain {
                        cg_id: 1,
                        uuid: None,
                        output: output.clone(),
                        workspace: WorkspaceId(crate::topology::workspace_key(1)),
                        space: 1,
                        current: true,
                        readable: true,
                        bounds: rect(0, 0, 800, 600),
                        visible: rect(0, 0, 800, 560),
                    },
                    NativeDomain {
                        cg_id: 1,
                        uuid: None,
                        output: output.clone(),
                        workspace: WorkspaceId(crate::topology::workspace_key(2)),
                        space: 2,
                        current: false,
                        readable: true,
                        bounds: rect(0, 0, 800, 600),
                        visible: rect(0, 0, 800, 560),
                    },
                ],
                skipped: Vec::new(),
            }
        }
        fn spare_members(output: &OutputId) -> BTreeMap<DomainKey, Vec<(u32, ObservedWindow)>> {
            BTreeMap::from([
                (
                    DomainKey {
                        output: output.clone(),
                        workspace: WorkspaceId(crate::topology::workspace_key(1)),
                    },
                    vec![(
                        4242,
                        ObservedWindow {
                            wid: 7,
                            x: 0,
                            y: 0,
                            w: 40,
                            h: 90,
                            floating: false,
                            fullscreen: false,
                            hints: WindowSizeHints::none(),
                        },
                    )],
                ),
                (
                    DomainKey {
                        output: output.clone(),
                        workspace: WorkspaceId(crate::topology::workspace_key(2)),
                    },
                    vec![(
                        4242,
                        ObservedWindow {
                            wid: 9,
                            x: 100,
                            y: 0,
                            w: 40,
                            h: 90,
                            floating: false,
                            fullscreen: false,
                            hints: WindowSizeHints::none(),
                        },
                    )],
                ),
            ])
        }
        fn spare_raw_for(
            members: &BTreeMap<DomainKey, Vec<(u32, ObservedWindow)>>,
        ) -> BTreeMap<String, WindowRawFacts> {
            let mut raw = BTreeMap::new();
            for pairs in members.values() {
                for (pid, window) in pairs {
                    raw.insert(
                        window_id(*pid, window.wid),
                        WindowRawFacts {
                            role: Some(crate::classifier::ROLE_WINDOW.to_owned()),
                            subrole: Some(crate::classifier::SUBROLE_STANDARD.to_owned()),
                            modal: Some(false),
                            resizable: Some(true),
                            layer: Some(0),
                            app_id: Some("com.example.app".to_owned()),
                            activation_policy: Some(
                                tiler_core::world_report::ActivationPolicy::Regular,
                            ),
                            host_hidden: Some(false),
                        },
                    );
                }
            }
            raw
        }
        #[allow(clippy::too_many_arguments)]
        fn spare_observation(
            incarnation: &str,
            generation: &str,
            ctx: &NativeContext,
            members: &BTreeMap<DomainKey, Vec<(u32, ObservedWindow)>>,
            raw: &BTreeMap<String, WindowRawFacts>,
            memory: &LifetimeMemory,
            focused_global: &str,
            observation_seq: u64,
            request_seq: u64,
            request_id: &str,
            command_seq: Option<u64>,
            command: Option<WorldCommand>,
        ) -> tiler_core::world_report::WorldReportV4 {
            use tiler_core::world_report::HoldReason;
            let unreadable: BTreeMap<DomainKey, HoldReason> = BTreeMap::new();
            let mut input = ReportInput::new(
                incarnation,
                generation,
                ctx,
                observation_seq,
                request_seq,
                request_id,
                members,
                memory,
                &[],
                &unreadable,
                &[],
                focused_global,
                false,
            );
            input.command_seq = command_seq;
            input.command = command;
            input.raw = raw;
            build_report(&mut input)
        }

        // Observation through the session engine plus the sequencer flight:
        // the accepted reply names the trailing spare.
        let mut session = WorldSession::new(
            true,
            FocusRoute::Slps,
            Arc::new(ObservationClock::new()),
            &[],
        );
        let incarnation = session.incarnation().to_owned();
        let generation = session.sequencer().bound_generation();
        let output = OutputId(crate::topology::output_key(1));
        let ctx = spare_ctx(&output);
        let members = spare_members(&output);
        let raw = spare_raw_for(&members);
        let memory = LifetimeMemory::empty();
        let focused = window_id(4242, 7);
        let ticket1 = session
            .sequencer()
            .try_begin_request(None)
            .expect("observation flight opens");
        let first = spare_observation(
            &incarnation,
            &generation,
            &ctx,
            &members,
            &raw,
            &memory,
            &focused,
            ticket1.observation_seq,
            ticket1.request_seq,
            &ticket1.request_id,
            ticket1.command_seq,
            None,
        );
        // Sanity: a standalone engine bound to the SAME session tokens
        // through the production macOS capabilities accepts the same
        // fixture (pins the shape to the engine contract, not the session
        // harness; the synthetic full-support default would hold every
        // realistic row unreadable, and the capability-less default
        // refuses native evidence without `host_owned`).
        let mut probe = WorldEngine::new_test_bound_with_capabilities(
            &generation,
            &incarnation,
            crate::world_connection::macos_capabilities(),
        )
        .expect("production capabilities bind");
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
                output: output.0.clone(),
                workspace: spare.clone(),
            }
        );
        // The existing command route carries the exact id (no ordinal).
        let command = select_command(&action, Some(output.clone()));
        assert_eq!(
            command,
            Some(WorldCommand::SelectWorkspace {
                output: output.clone(),
                workspace: spare.clone(),
            })
        );
        // Dispatch the exact-id command: the core retains the View
        // transfer on the exact logical id and publishes the native
        // creation desire downstream (a desire record only — this offline
        // path creates no display, stages no creation attempt with an
        // inferred `created` identity, and mutates no native order).
        let ticket2 = session
            .sequencer()
            .try_begin_request(command.clone())
            .expect("command flight opens");
        let second = spare_observation(
            &incarnation,
            &generation,
            &ctx,
            &members,
            &raw,
            &memory,
            &focused,
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
        assert_eq!(transfer.target_output.0, output.0);
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
        assert_eq!(creations[0].target_output.0, output.0);
        // The existing plan binder resolves the retained target to the
        // exact id/output/op with no binding (unbound spare) and the
        // creation desire attached — never an ordinal reinterpretation.
        let ctx = Arc::new(ctx);
        let mut claim = ClaimedSelect::indicator(action, rendered.sid, 7, 9);
        crate::runtime_thread::bind_select_plan(
            &mut session,
            &ctx,
            &mut claim,
            &transfer.operation_id,
        );
        assert_eq!(
            claim.select_target_output.as_deref(),
            Some(output.0.as_str())
        );
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
        assert_eq!(
            claim.select_creation_output.as_deref(),
            Some(output.0.as_str())
        );
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

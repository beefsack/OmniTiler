use tiler_core::active_group::ActiveGroupMember;
use tiler_core::contract::DragSide;
use tiler_core::directional::{NodeId, OutputId, WindowId, WorkspaceId};
use tiler_core::geometry::Rect;
use tiler_core::gesture::GestureSettled;
use tiler_core::intent_data::IntentData;
use tiler_core::policy::PriorGroupEdge;
use tiler_core::session::{DomainKey, DragHoverPrior};
use tiler_core::world_presentation::{
    GroupHideReason, PagerScreen, PagerWindow, PagerWorkspace, PresentationUnderlay,
    PresentationUnderlayFound,
};
use tiler_core::world_report::{
    AfterShown, AfterShownMember, AfterShownScope, AuthorityScope, BorderHideReasonV4,
    BorderPresentationV4, DragPreviewV4, FloatOrigin, FocusRequest, FocusRequestCause,
    FrameWitness, HoldReason, IntentDesire, NativeCreationDesire, OperationEvent, OperationOutcome,
    OutputDesire, RowDiagnostic, RowDiagnosticKind, RowDiagnosticLane, ScopeAuthority, ScopeStatus,
    StateProperty, StateTarget, SwitchEvent, SwitchFocus, SwitchOutcome, TilingMode, TransferKind,
    TransferTargetV4, VisibilityTarget, WindowDesire, WorkspaceDesire, WorldPresentationV4,
    WorldReplyV4,
};
use tiler_protocol::world_protocol::DecodedReplyV4;

fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

fn out_scope(out: &str) -> AuthorityScope {
    AuthorityScope::Output(OutputId(out.to_owned()))
}

fn ws_scope(out: &str, ws: &str) -> AuthorityScope {
    AuthorityScope::Workspace {
        output: OutputId(out.to_owned()),
        workspace: WorkspaceId(ws.to_owned()),
    }
}

fn default_pager() -> Vec<PagerScreen> {
    vec![PagerScreen {
        output: "out-a".to_owned(),
        workspaces: vec![
            PagerWorkspace {
                id: "ws-a".to_owned(),
                ordinal: 1,
                label: "1".to_owned(),
                current: true,
                occupied: true,
                contains_focused: true,
                trailing: false,
                windows: vec![PagerWindow {
                    window: WindowId("win-a".to_owned()),
                    frame: rect(8, 8, 800, 600),
                }],
            },
            PagerWorkspace {
                id: "ws-spare".to_owned(),
                ordinal: 2,
                label: "2".to_owned(),
                current: false,
                occupied: false,
                contains_focused: false,
                trailing: true,
                windows: vec![],
            },
        ],
    }]
}

fn default_window() -> WindowDesire {
    WindowDesire {
        lifetime: WindowId("win-a".to_owned()),
        output: OutputId("out-a".to_owned()),
        workspace: "ws-a".to_owned(),
        allocation: Some(rect(8, 8, 800, 600)),
        effective_frame: Some(rect(8, 8, 800, 600)),
        frame_authority: ScopeAuthority::Controlled,
        membership_authority: ScopeAuthority::Controlled,
        frame_operation: None,
        floating: false,
        float_origin: None,
        command_seq: None,
        visibility: Some(VisibilityTarget {
            shown: true,
            revision: 1,
            fallback: false,
        }),
        visibility_authority: ScopeAuthority::Controlled,
        state: vec![],
    }
}

fn default_border_visible() -> BorderPresentationV4 {
    BorderPresentationV4 {
        visible: true,
        reason: None,
        subject: Some(WindowId("win-a".to_owned())),
        outsets: Some([3, 3, 3, 3]),
        width: Some(3),
        radius: Some(6),
        witness: Some(FrameWitness {
            win: WindowId("win-a".to_owned()),
            obs: 1,
            frame: rect(8, 8, 800, 600),
            foreground: WindowId("win-a".to_owned()),
        }),
    }
}

fn hidden_border(reason: BorderHideReasonV4) -> BorderPresentationV4 {
    BorderPresentationV4 {
        visible: false,
        reason: Some(reason),
        subject: None,
        outsets: None,
        width: None,
        radius: None,
        witness: None,
    }
}

pub fn base_bootstrap() -> DecodedReplyV4 {
    DecodedReplyV4 {
        roster_plan: None,
        reply: WorldReplyV4 {
            contract_version: 4,
            adapter_incarnation: "adapter-a".to_owned(),
            core_generation: "generation-a".to_owned(),
            request_id: "request-a".to_owned(),
            based_on_observation_seq: 1,
            state_revision: 1,
            ready: false,
            command_consumed: false,
            scopes: vec![],
            workspaces: vec![],
            outputs: vec![],
            windows: vec![],
            transfers: vec![],
            focus_request: None,
            drag_preview: None,
            gesture_settled: None,
            operation_settled: vec![],
            native_creations: vec![],
            warp_request: None,
            presentation: WorldPresentationV4 {
                foreground: String::new(),
                rev: 1,
                suspended_output: None,
                border: hidden_border(BorderHideReasonV4::NoTarget),
                underlay: PresentationUnderlay::Hidden {
                    base_revision: None,
                    reason: GroupHideReason::NoGroup,
                },
                pager: vec![],
            },
            intent_desired: vec![],
            switch_settled: vec![],
            diagnostics: vec![],
        },
    }
}

pub fn base_observe() -> DecodedReplyV4 {
    DecodedReplyV4 {
        roster_plan: None,
        reply: WorldReplyV4 {
            contract_version: 4,
            adapter_incarnation: "adapter-a".to_owned(),
            core_generation: "generation-a".to_owned(),
            request_id: "request-a".to_owned(),
            based_on_observation_seq: 1,
            state_revision: 1,
            ready: true,
            command_consumed: false,
            scopes: vec![ScopeStatus {
                scope: out_scope("out-a"),
                authority: ScopeAuthority::Controlled,
            }],
            workspaces: vec![
                WorkspaceDesire {
                    id: "ws-a".to_owned(),
                    output: OutputId("out-a".to_owned()),
                    native_binding: None,
                    scope: out_scope("out-a"),
                    order: 0,
                    tiling_mode: TilingMode::Tiled,
                    existence: true,
                    hold: None,
                    trailing: false,
                },
                WorkspaceDesire {
                    id: "ws-spare".to_owned(),
                    output: OutputId("out-a".to_owned()),
                    native_binding: None,
                    scope: out_scope("out-a"),
                    order: 1,
                    tiling_mode: TilingMode::Tiled,
                    existence: true,
                    hold: None,
                    trailing: true,
                },
            ],
            outputs: vec![OutputDesire {
                output: OutputId("out-a".to_owned()),
                logical_view: Some("ws-a".to_owned()),
                tiling_mode: TilingMode::Tiled,
                hold: None,
            }],
            windows: vec![default_window()],
            transfers: vec![],
            focus_request: None,
            drag_preview: None,
            gesture_settled: None,
            operation_settled: vec![],
            native_creations: vec![],
            warp_request: None,
            presentation: WorldPresentationV4 {
                foreground: "win-a".to_owned(),
                rev: 1,
                suspended_output: None,
                border: default_border_visible(),
                underlay: PresentationUnderlay::Hidden {
                    base_revision: None,
                    reason: GroupHideReason::NoGroup,
                },
                pager: default_pager(),
            },
            intent_desired: vec![],
            switch_settled: vec![],
            diagnostics: vec![],
        },
    }
}

fn obs_with(f: impl FnOnce(&mut DecodedReplyV4)) -> DecodedReplyV4 {
    let mut d = base_observe();
    f(&mut d);
    d
}

fn boot_with(f: impl FnOnce(&mut DecodedReplyV4)) -> DecodedReplyV4 {
    let mut d = base_bootstrap();
    f(&mut d);
    d
}

fn hold_case(reason: HoldReason, empty_view: bool) -> DecodedReplyV4 {
    obs_with(|d| {
        d.reply.scopes[0].authority = ScopeAuthority::Hold(reason);
        for ws in &mut d.reply.workspaces {
            ws.hold = Some(reason);
        }
        if empty_view {
            d.reply.outputs[0].logical_view = None;
        }
        d.reply.outputs[0].hold = Some(reason);
        let w = &mut d.reply.windows[0];
        w.frame_authority = ScopeAuthority::Hold(reason);
        w.membership_authority = ScopeAuthority::Hold(reason);
        w.visibility_authority = ScopeAuthority::Hold(reason);
    })
}

fn domain_key(out: &str, ws: &str) -> DomainKey {
    DomainKey {
        output: OutputId(out.to_owned()),
        workspace: WorkspaceId(ws.to_owned()),
    }
}

fn drag_preview(edge: Option<DragSide>, has_hover: bool) -> DecodedReplyV4 {
    obs_with(|d| {
        let domain = domain_key("out-a", "ws-a");
        d.reply.drag_preview = Some(DragPreviewV4 {
            id: 1,
            rect: rect(8, 8, 400, 600),
            hover: if has_hover {
                Some(DragHoverPrior {
                    domain: domain.clone(),
                    source_leaf: NodeId("leaf-a".to_owned()),
                    source_window: WindowId("win-a".to_owned()),
                    revision: 1,
                    prior: edge.map(|edge| PriorGroupEdge {
                        group: NodeId("group-a".to_owned()),
                        edge,
                    }),
                    prior_domain: domain.clone(),
                })
            } else {
                None
            },
            domain: domain.clone(),
            source: domain,
            target: WindowId("win-a".to_owned()),
            revision: 1,
        });
    })
}

fn operation_single(outcome: OperationOutcome) -> DecodedReplyV4 {
    obs_with(|d| {
        d.reply.operation_settled = vec![OperationEvent {
            window: WindowId("win-a".to_owned()),
            kind: outcome.kind(),
            operation_id: "operation-a".to_owned(),
            outcome,
            cause: None,
        }];
    })
}

fn presentation_hidden(
    border_reason: BorderHideReasonV4,
    underlay_reason: GroupHideReason,
    base_revision: Option<u64>,
) -> DecodedReplyV4 {
    obs_with(|d| {
        d.reply.presentation.border = hidden_border(border_reason);
        d.reply.presentation.underlay = PresentationUnderlay::Hidden {
            base_revision,
            reason: underlay_reason,
        };
    })
}

pub fn valid_reply_cases() -> Vec<(&'static str, DecodedReplyV4)> {
    let op_causes = [
        "",
        "focus-unknown",
        "send-target-unknown",
        "send-window-unknown",
        "send-refused",
        "view-output-unknown",
        "view-target-unknown",
        "transfer-in-flight",
        "direction-invalid",
        "scope-held",
        "selection-unknown",
        "migration-refused",
        "operation-refused",
        "tiling-target-unknown",
        "state-unsupported",
        "state-unknown",
        "fullscreen-not-exitable",
        "stale-render-target",
        "baseline-non-subject",
        "closed",
        "constraints",
        "early-verdict",
        "external",
        "fixed",
        "floating",
        "homing-loss",
        "invalid-direction",
        "invalid-geometry",
        "non-subject-focus",
        "refused-with-unmaximized",
        "sticky",
        "tiling-changed",
        "topology-unreadable",
        "unknown-output",
        "unknown-window",
    ];

    vec![
        ("reply-bootstrap", base_bootstrap()),
        ("reply-observe", base_observe()),
        (
            "reply-untouched-workspace-scope",
            obs_with(|d| {
                d.reply.scopes = vec![
                    ScopeStatus {
                        scope: ws_scope("out-a", "ws-a"),
                        authority: ScopeAuthority::Controlled,
                    },
                    ScopeStatus {
                        scope: ws_scope("out-a", "ws-spare"),
                        authority: ScopeAuthority::Untouched,
                    },
                ];
                d.reply.workspaces[0].scope = ws_scope("out-a", "ws-a");
                d.reply.workspaces[1].scope = ws_scope("out-a", "ws-spare");
                let w = &mut d.reply.windows[0];
                w.allocation = None;
                w.effective_frame = None;
                w.frame_authority = ScopeAuthority::Untouched;
                w.membership_authority = ScopeAuthority::Untouched;
                w.visibility = None;
                w.visibility_authority = ScopeAuthority::Untouched;
            }),
        ),
        (
            "reply-hold-unreadable",
            hold_case(HoldReason::Unreadable, true),
        ),
        (
            "reply-hold-incomplete-inventory",
            hold_case(HoldReason::IncompleteInventory, false),
        ),
        (
            "reply-hold-gesture-barrier",
            hold_case(HoldReason::GestureBarrier, false),
        ),
        (
            "reply-hold-refused-target",
            hold_case(HoldReason::RefusedTarget, false),
        ),
        (
            "reply-hold-ambiguous-creation",
            hold_case(HoldReason::AmbiguousCreation, false),
        ),
        (
            "reply-hold-occupied-removal",
            hold_case(HoldReason::OccupiedRemoval, false),
        ),
        (
            "reply-hold-constraint-clamp",
            hold_case(HoldReason::ConstraintClamp, false),
        ),
        (
            "reply-hold-minimized",
            hold_case(HoldReason::Minimized, false),
        ),
        (
            "reply-workspace-floating",
            obs_with(|d| {
                d.reply.workspaces[0].tiling_mode = TilingMode::Floating;
                d.reply.outputs[0].tiling_mode = TilingMode::Floating;
                let w = &mut d.reply.windows[0];
                w.allocation = None;
                w.effective_frame = None;
                w.frame_authority = ScopeAuthority::Untouched;
                w.floating = true;
            }),
        ),
        (
            "reply-capacity-float",
            obs_with(|d| {
                let w = &mut d.reply.windows[0];
                w.allocation = None;
                w.effective_frame = None;
                w.frame_authority = ScopeAuthority::Untouched;
                w.floating = true;
                w.float_origin = Some(FloatOrigin::Capacity);
            }),
        ),
        (
            "reply-native-create-retire",
            obs_with(|d| {
                d.reply.workspaces[0].native_binding = Some("native-a".to_owned());
                d.reply.workspaces[1].existence = false;
                d.reply.workspaces.push(WorkspaceDesire {
                    id: "ws-new".to_owned(),
                    output: OutputId("out-a".to_owned()),
                    native_binding: None,
                    scope: out_scope("out-a"),
                    order: 2,
                    tiling_mode: TilingMode::Tiled,
                    existence: true,
                    hold: None,
                    trailing: false,
                });
                d.reply.native_creations = vec![NativeCreationDesire {
                    operation_id: "create-a".to_owned(),
                    logical_workspace: "ws-new".to_owned(),
                    target_output: OutputId("out-a".to_owned()),
                }];
            }),
        ),
        (
            "reply-transfer-window",
            obs_with(|d| {
                d.reply.transfers = vec![TransferTargetV4 {
                    operation_id: "transfer-a".to_owned(),
                    kind: TransferKind::Window(WindowId("win-a".to_owned())),
                    target_output: OutputId("out-a".to_owned()),
                    target_workspace: "ws-a".to_owned(),
                    follow: true,
                    held: false,
                    command_seq: Some(1),
                    parent: None,
                    detached_destination: None,
                }];
            }),
        ),
        (
            "reply-transfer-view",
            obs_with(|d| {
                d.reply.transfers = vec![TransferTargetV4 {
                    operation_id: "transfer-a".to_owned(),
                    kind: TransferKind::View,
                    target_output: OutputId("out-a".to_owned()),
                    target_workspace: "ws-a".to_owned(),
                    follow: true,
                    held: false,
                    command_seq: Some(1),
                    parent: None,
                    detached_destination: None,
                }];
            }),
        ),
        (
            "reply-transfer-migration",
            obs_with(|d| {
                d.reply.transfers = vec![TransferTargetV4 {
                    operation_id: "transfer-a".to_owned(),
                    kind: TransferKind::Migration,
                    target_output: OutputId("out-a".to_owned()),
                    target_workspace: "ws-a".to_owned(),
                    follow: true,
                    held: false,
                    command_seq: Some(1),
                    parent: None,
                    detached_destination: None,
                }];
            }),
        ),
        (
            "reply-transfer-commandless-held",
            obs_with(|d| {
                d.reply.transfers = vec![TransferTargetV4 {
                    operation_id: "transfer-auto".to_owned(),
                    kind: TransferKind::Window(WindowId("win-a".to_owned())),
                    target_output: OutputId("out-a".to_owned()),
                    target_workspace: "ws-a".to_owned(),
                    follow: false,
                    held: true,
                    command_seq: None,
                    parent: None,
                    detached_destination: None,
                }];
            }),
        ),
        (
            "reply-effect-focus",
            obs_with(|d| {
                d.reply.focus_request = Some(FocusRequest {
                    id: "focus-1".to_owned(),
                    target: WindowId("win-a".to_owned()),
                    issued_revision: 1,
                    cause: FocusRequestCause::ExplicitCommand,
                    output: OutputId("out-a".to_owned()),
                    workspace: "ws-a".to_owned(),
                    cmd_seq: Some(1),
                    after_shown: Some(AfterShown {
                        op: "op-1".to_owned(),
                        obs: 1,
                        members: vec![AfterShownMember {
                            win: WindowId("win-a".to_owned()),
                            out: OutputId("out-a".to_owned()),
                            ws: "ws-a".to_owned(),
                            frame: Some(rect(8, 8, 800, 600)),
                        }],
                        views: vec![AfterShownScope {
                            out: OutputId("out-a".to_owned()),
                            ws: "ws-a".to_owned(),
                        }],
                        selected: vec![AfterShownScope {
                            out: OutputId("out-a".to_owned()),
                            ws: "ws-a".to_owned(),
                        }],
                        departing: vec![],
                    }),
                });
            }),
        ),
        (
            "reply-effect-automatic-admission",
            obs_with(|d| {
                d.reply.focus_request = Some(FocusRequest {
                    id: "focus-2".to_owned(),
                    target: WindowId("win-a".to_owned()),
                    issued_revision: 1,
                    cause: FocusRequestCause::AutomaticAdmission,
                    output: OutputId("out-a".to_owned()),
                    workspace: "ws-a".to_owned(),
                    cmd_seq: None,
                    after_shown: None,
                });
            }),
        ),
        (
            "reply-effect-focus-follows-cursor",
            obs_with(|d| {
                d.reply.focus_request = Some(FocusRequest {
                    id: "focus-3".to_owned(),
                    target: WindowId("win-a".to_owned()),
                    issued_revision: 1,
                    cause: FocusRequestCause::FocusFollowsCursor,
                    output: OutputId("out-a".to_owned()),
                    workspace: "ws-a".to_owned(),
                    cmd_seq: None,
                    after_shown: None,
                });
            }),
        ),
        (
            "reply-state-authority",
            obs_with(|d| {
                d.reply.windows[0].state = vec![
                    StateTarget {
                        property: StateProperty::Fullscreen,
                        value: false,
                        authority: ScopeAuthority::Controlled,
                        command_seq: Some(1),
                        automatic_clear: false,
                    },
                    StateTarget {
                        property: StateProperty::Maximized,
                        value: true,
                        authority: ScopeAuthority::Hold(HoldReason::RefusedTarget),
                        command_seq: None,
                        automatic_clear: false,
                    },
                    StateTarget {
                        property: StateProperty::Sticky,
                        value: false,
                        authority: ScopeAuthority::Untouched,
                        command_seq: None,
                        automatic_clear: false,
                    },
                    StateTarget {
                        property: StateProperty::Above,
                        value: true,
                        authority: ScopeAuthority::Controlled,
                        command_seq: None,
                        automatic_clear: false,
                    },
                ];
            }),
        ),
        (
            "reply-command-clear",
            obs_with(|d| {
                let w = &mut d.reply.windows[0];
                w.frame_authority = ScopeAuthority::Untouched;
                w.command_seq = Some(1);
                w.state = vec![StateTarget {
                    property: StateProperty::Maximized,
                    value: false,
                    authority: ScopeAuthority::Controlled,
                    command_seq: Some(1),
                    automatic_clear: true,
                }];
            }),
        ),
        (
            "reply-birth-clear",
            obs_with(|d| {
                let w = &mut d.reply.windows[0];
                w.frame_authority = ScopeAuthority::Untouched;
                w.command_seq = None;
                w.state = vec![StateTarget {
                    property: StateProperty::Maximized,
                    value: false,
                    authority: ScopeAuthority::Controlled,
                    command_seq: None,
                    automatic_clear: true,
                }];
            }),
        ),
        (
            "reply-visibility-hide-staged",
            obs_with(|d| {
                d.reply.state_revision = 2;
                d.reply.windows[0].visibility = Some(VisibilityTarget {
                    shown: false,
                    revision: 1,
                    fallback: false,
                });
            }),
        ),
        (
            "reply-visibility-hide-fallback",
            obs_with(|d| {
                d.reply.state_revision = 2;
                d.reply.windows[0].visibility = Some(VisibilityTarget {
                    shown: false,
                    revision: 1,
                    fallback: true,
                });
            }),
        ),
        (
            "reply-visibility-independent",
            obs_with(|d| {
                let w = &mut d.reply.windows[0];
                w.frame_authority = ScopeAuthority::Hold(HoldReason::GestureBarrier);
                w.membership_authority = ScopeAuthority::Untouched;
            }),
        ),
        (
            "reply-intent-save-clear",
            obs_with(|d| {
                let mut w1 = default_window();
                w1.visibility = None;
                w1.visibility_authority = ScopeAuthority::Untouched;
                let mut w2 = default_window();
                w2.lifetime = WindowId("win-b".to_owned());
                d.reply.windows = vec![w1, w2];
                d.reply.intent_desired = vec![
                    IntentDesire {
                        win: WindowId("win-a".to_owned()),
                        data: Some(IntentData::new(Some(true), Some(false))),
                    },
                    IntentDesire {
                        win: WindowId("win-b".to_owned()),
                        data: None,
                    },
                ];
            }),
        ),
        (
            "reply-result-drag-left",
            drag_preview(Some(DragSide::Left), true),
        ),
        (
            "reply-result-drag-right",
            drag_preview(Some(DragSide::Right), true),
        ),
        (
            "reply-result-drag-top",
            drag_preview(Some(DragSide::Top), true),
        ),
        (
            "reply-result-drag-bottom",
            drag_preview(Some(DragSide::Bottom), true),
        ),
        ("reply-result-drag-no-prior", drag_preview(None, true)),
        ("reply-result-drag-no-hover", drag_preview(None, false)),
        (
            "reply-result-float-placed",
            obs_with(|d| {
                let w = &mut d.reply.windows[0];
                w.allocation = None;
                w.effective_frame = Some(rect(10, 20, 640, 480));
                w.frame_operation = Some("float-a".to_owned());
                w.floating = true;
                w.command_seq = Some(1);
            }),
        ),
        (
            "reply-result-maximize",
            obs_with(|d| {
                d.reply.windows[0].state = vec![StateTarget {
                    property: StateProperty::Maximized,
                    value: true,
                    authority: ScopeAuthority::Controlled,
                    command_seq: Some(1),
                    automatic_clear: false,
                }];
            }),
        ),
        (
            "reply-operation-continued-tiled",
            operation_single(OperationOutcome::ClearContinuedTiled),
        ),
        (
            "reply-operation-continued-float",
            operation_single(OperationOutcome::ClearContinuedFloat),
        ),
        (
            "reply-operation-refused-proven",
            operation_single(OperationOutcome::ClearRefusedProven),
        ),
        (
            "reply-operation-abandoned-external-clear",
            operation_single(OperationOutcome::ClearAbandonedExternalClear),
        ),
        (
            "reply-operation-abandoned-focus-changed",
            operation_single(OperationOutcome::ClearAbandonedFocusChanged),
        ),
        (
            "reply-operation-abandoned-view-changed",
            operation_single(OperationOutcome::ClearAbandonedViewChanged),
        ),
        (
            "reply-operation-abandoned-homing-changed",
            operation_single(OperationOutcome::ClearAbandonedHomingChanged),
        ),
        (
            "reply-operation-abandoned-closed",
            operation_single(OperationOutcome::ClearAbandonedClosed),
        ),
        (
            "reply-operation-abandoned-fullscreen",
            operation_single(OperationOutcome::ClearAbandonedFullscreen),
        ),
        (
            "reply-operation-superseded",
            operation_single(OperationOutcome::ClearSuperseded),
        ),
        (
            "reply-operation-continuation-refused",
            operation_single(OperationOutcome::ClearContinuationRefused),
        ),
        (
            "reply-operation-float-frame-proven",
            operation_single(OperationOutcome::FloatFrameProven),
        ),
        (
            "reply-operation-float-refused-proven",
            operation_single(OperationOutcome::FloatRefusedProven),
        ),
        (
            "reply-operation-float-abandoned",
            operation_single(OperationOutcome::FloatAbandoned),
        ),
        (
            "reply-operation-causes",
            obs_with(|d| {
                d.reply.operation_settled = op_causes
                    .iter()
                    .enumerate()
                    .map(|(i, cause)| OperationEvent {
                        window: WindowId(format!("win-{i}")),
                        kind: OperationOutcome::ClearContinuationRefused.kind(),
                        operation_id: format!("operation-{i}"),
                        outcome: OperationOutcome::ClearContinuationRefused,
                        cause: (!cause.is_empty()).then(|| (*cause).to_owned()),
                    })
                    .collect();
            }),
        ),
        (
            "reply-gesture-committed",
            obs_with(|d| {
                d.reply.gesture_settled = Some(GestureSettled {
                    id: 1,
                    committed: true,
                });
            }),
        ),
        (
            "reply-gesture-cancelled",
            obs_with(|d| {
                d.reply.gesture_settled = Some(GestureSettled {
                    id: 1,
                    committed: false,
                });
            }),
        ),
        (
            "reply-switch-proven",
            obs_with(|d| {
                d.reply.switch_settled = vec![SwitchEvent {
                    output: OutputId("out-a".to_owned()),
                    operation_id: "view-a".to_owned(),
                    outcome: SwitchOutcome::Complete,
                    failed: vec![],
                    focus: SwitchFocus::Proven,
                    fallback: false,
                }];
            }),
        ),
        (
            "reply-switch-refused",
            obs_with(|d| {
                d.reply.switch_settled = vec![SwitchEvent {
                    output: OutputId("out-a".to_owned()),
                    operation_id: "view-a".to_owned(),
                    outcome: SwitchOutcome::CompletedWithFailures,
                    failed: vec![WindowId("win-a".to_owned())],
                    focus: SwitchFocus::Refused,
                    fallback: true,
                }];
            }),
        ),
        (
            "reply-switch-unproven",
            obs_with(|d| {
                d.reply.switch_settled = vec![SwitchEvent {
                    output: OutputId("out-a".to_owned()),
                    operation_id: "view-a".to_owned(),
                    outcome: SwitchOutcome::CompletedWithFailures,
                    failed: vec![WindowId("win-a".to_owned())],
                    focus: SwitchFocus::Unproven,
                    fallback: false,
                }];
            }),
        ),
        (
            "reply-row-diagnostics",
            obs_with(|d| {
                let mut h = hold_case(HoldReason::Unreadable, false);
                h.reply.diagnostics = vec![
                    RowDiagnostic {
                        scope: out_scope("out-a"),
                        row: RowDiagnosticLane::Screen,
                        id: Some("out-a".to_owned()),
                        kind: RowDiagnosticKind::Unreadable,
                    },
                    RowDiagnostic {
                        scope: out_scope("out-a"),
                        row: RowDiagnosticLane::Window,
                        id: Some("win-a".to_owned()),
                        kind: RowDiagnosticKind::InvalidFact,
                    },
                    RowDiagnostic {
                        scope: out_scope("out-a"),
                        row: RowDiagnosticLane::Native,
                        id: Some("out-a".to_owned()),
                        kind: RowDiagnosticKind::ConflictingFact,
                    },
                    RowDiagnostic {
                        scope: out_scope("out-a"),
                        row: RowDiagnosticLane::Attempt,
                        id: Some("attempt-a".to_owned()),
                        kind: RowDiagnosticKind::Unreadable,
                    },
                    RowDiagnostic {
                        scope: out_scope("out-a"),
                        row: RowDiagnosticLane::Presentation,
                        id: None,
                        kind: RowDiagnosticKind::InvalidFact,
                    },
                ];
                *d = h;
            }),
        ),
        (
            "reply-presentation-hidden-disabled",
            presentation_hidden(
                BorderHideReasonV4::Disabled,
                GroupHideReason::Disabled,
                None,
            ),
        ),
        (
            "reply-presentation-hidden-minimized",
            presentation_hidden(
                BorderHideReasonV4::NoTarget,
                GroupHideReason::Minimized,
                Some(1),
            ),
        ),
        (
            "reply-presentation-hidden-maximized",
            presentation_hidden(
                BorderHideReasonV4::Minimized,
                GroupHideReason::Maximized,
                None,
            ),
        ),
        (
            "reply-presentation-hidden-fullscreen",
            presentation_hidden(
                BorderHideReasonV4::Maximized,
                GroupHideReason::Fullscreen,
                Some(1),
            ),
        ),
        (
            "reply-presentation-hidden-floating",
            presentation_hidden(
                BorderHideReasonV4::Fullscreen,
                GroupHideReason::Floating,
                None,
            ),
        ),
        (
            "reply-presentation-hidden-pending",
            presentation_hidden(
                BorderHideReasonV4::HostHidden,
                GroupHideReason::Pending,
                Some(1),
            ),
        ),
        (
            "reply-presentation-hidden-diverged",
            presentation_hidden(
                BorderHideReasonV4::HiddenWorkspace,
                GroupHideReason::Diverged,
                None,
            ),
        ),
        (
            "reply-presentation-hidden-root-leaf",
            presentation_hidden(
                BorderHideReasonV4::Shell,
                GroupHideReason::RootLeaf,
                Some(1),
            ),
        ),
        (
            "reply-presentation-hidden-focus-changed",
            presentation_hidden(
                BorderHideReasonV4::Suspended,
                GroupHideReason::FocusChanged,
                None,
            ),
        ),
        (
            "reply-presentation-hidden-no-group",
            presentation_hidden(
                BorderHideReasonV4::Disabled,
                GroupHideReason::NoGroup,
                Some(1),
            ),
        ),
        (
            "reply-presentation-hidden-idle",
            presentation_hidden(BorderHideReasonV4::NoTarget, GroupHideReason::Idle, None),
        ),
        (
            "reply-presentation-hidden-suspended",
            obs_with(|d| {
                d.reply.presentation = WorldPresentationV4 {
                    foreground: "win-a".to_owned(),
                    rev: 1,
                    suspended_output: Some(OutputId("out-a".to_owned())),
                    border: hidden_border(BorderHideReasonV4::Minimized),
                    underlay: PresentationUnderlay::Hidden {
                        base_revision: Some(1),
                        reason: GroupHideReason::Suspended,
                    },
                    pager: vec![],
                };
            }),
        ),
        (
            "reply-presentation-underlay-found",
            obs_with(|d| {
                let mut w1 = default_window();
                w1.allocation = Some(rect(8, 8, 400, 600));
                w1.effective_frame = Some(rect(8, 8, 400, 600));
                let mut w2 = default_window();
                w2.lifetime = WindowId("win-b".to_owned());
                w2.allocation = Some(rect(416, 8, 400, 600));
                w2.effective_frame = Some(rect(416, 8, 400, 600));
                d.reply.windows = vec![w1, w2];
                d.reply.presentation = WorldPresentationV4 {
                    foreground: "win-a".to_owned(),
                    rev: 1,
                    suspended_output: None,
                    border: BorderPresentationV4 {
                        visible: true,
                        reason: None,
                        subject: Some(WindowId("win-a".to_owned())),
                        outsets: Some([3, 3, 3, 3]),
                        width: Some(3),
                        radius: Some(6),
                        witness: Some(FrameWitness {
                            win: WindowId("win-a".to_owned()),
                            obs: 1,
                            frame: rect(8, 8, 400, 600),
                            foreground: WindowId("win-a".to_owned()),
                        }),
                    },
                    underlay: PresentationUnderlay::Found(PresentationUnderlayFound {
                        base_revision: 1,
                        home: domain_key("out-a", "ws-a"),
                        group: NodeId("group-a".to_owned()),
                        focused_leaf: NodeId("leaf-a".to_owned()),
                        focused_window: WindowId("win-a".to_owned()),
                        members: vec![
                            ActiveGroupMember {
                                window: WindowId("win-a".to_owned()),
                                leaf: NodeId("leaf-a".to_owned()),
                                rect: rect(8, 8, 400, 600),
                            },
                            ActiveGroupMember {
                                window: WindowId("win-b".to_owned()),
                                leaf: NodeId("leaf-b".to_owned()),
                                rect: rect(416, 8, 400, 600),
                            },
                        ],
                        union_rect: rect(8, 8, 808, 600),
                        outer_rect: rect(5, 5, 814, 606),
                    }),
                    pager: vec![PagerScreen {
                        output: "out-a".to_owned(),
                        workspaces: vec![
                            PagerWorkspace {
                                id: "ws-a".to_owned(),
                                ordinal: 1,
                                label: "1".to_owned(),
                                current: true,
                                occupied: true,
                                contains_focused: true,
                                trailing: false,
                                windows: vec![
                                    PagerWindow {
                                        window: WindowId("win-a".to_owned()),
                                        frame: rect(8, 8, 400, 600),
                                    },
                                    PagerWindow {
                                        window: WindowId("win-b".to_owned()),
                                        frame: rect(416, 8, 400, 600),
                                    },
                                ],
                            },
                            PagerWorkspace {
                                id: "ws-spare".to_owned(),
                                ordinal: 2,
                                label: "2".to_owned(),
                                current: false,
                                occupied: false,
                                contains_focused: false,
                                trailing: true,
                                windows: vec![],
                            },
                        ],
                    }],
                };
            }),
        ),
        (
            "reply-above-true",
            obs_with(|d| {
                d.reply.windows[0].floating = true;
                d.reply.windows[0].state = vec![StateTarget {
                    property: StateProperty::Above,
                    value: true,
                    authority: ScopeAuthority::Controlled,
                    command_seq: None,
                    automatic_clear: false,
                }];
            }),
        ),
        (
            "reply-above-false",
            obs_with(|d| {
                d.reply.windows[0].floating = true;
                d.reply.windows[0].state = vec![StateTarget {
                    property: StateProperty::Above,
                    value: false,
                    authority: ScopeAuthority::Controlled,
                    command_seq: None,
                    automatic_clear: false,
                }];
            }),
        ),
        (
            "reply-close-focus-pending",
            boot_with(|d| {
                d.reply.focus_request = Some(FocusRequest {
                    id: "close-focus-42".to_owned(),
                    target: WindowId("C".to_owned()),
                    issued_revision: 42,
                    cause: FocusRequestCause::PositiveClose,
                    output: OutputId("L".to_owned()),
                    workspace: "W1".to_owned(),
                    cmd_seq: None,
                    after_shown: None,
                });
            }),
        ),
    ]
}

pub fn contextual_reject_cases() -> Vec<(&'static str, DecodedReplyV4)> {
    vec![
        (
            "invalid-above-unsupported",
            obs_with(|d| {
                d.reply.windows[0].floating = true;
                d.reply.windows[0].state = vec![StateTarget {
                    property: StateProperty::Above,
                    value: true,
                    authority: ScopeAuthority::Controlled,
                    command_seq: None,
                    automatic_clear: false,
                }];
            }),
        ),
        (
            "invalid-warp-unsupported",
            obs_with(|d| {
                d.reply.warp_request = Some(tiler_core::world_report::WarpRequest {
                    id: "warp-1".to_owned(),
                    target: tiler_core::directional::WindowId("win-a".to_owned()),
                    revision: 1,
                    output: tiler_core::directional::OutputId("out-a".to_owned()),
                    workspace: tiler_core::directional::WorkspaceId("ws-a".to_owned()),
                    point: (20, 30),
                    pointer_baseline: Some(1),
                });
            }),
        ),
        (
            "invalid-sticky-native-free-target",
            obs_with(|d| {
                d.reply.windows[0].state = vec![StateTarget {
                    property: StateProperty::Sticky,
                    value: true,
                    authority: ScopeAuthority::Controlled,
                    command_seq: Some(1),
                    automatic_clear: false,
                }];
            }),
        ),
    ]
}

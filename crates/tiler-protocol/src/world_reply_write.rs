//! Checked canonical v4 reply writer, exact byte counter, and broker
//! bound/projection emitters.
//!
//! Operates on a borrowed [`WorldReplyV4`] and drives exact byte counting,
//! canonical wire emission, broker bound emission, and renderer projection
//! emission through the same typed schema traversal and validation, with no
//! intermediate JSON object model and no round-trip re-parse.

use std::collections::BTreeSet;

use tiler_core::bounds::{is_opaque_id, valid_carried_rect};
use tiler_core::geometry::Rect;
use tiler_core::gesture::GestureSettled;
use tiler_core::intent_data::encode_intent_data;
use tiler_core::world_engine::ValidatedReply;
use tiler_core::world_presentation::{PagerScreen, PresentationUnderlay};
use tiler_core::world_reply_validation::{
    ReplyValidationError, validate_projection_reply_v4, validate_reply_v4,
};
use tiler_core::world_report::{
    AfterShown, AfterShownScope, AuthorityScope, BorderPresentationV4, DetachedDestination,
    DetachedRef, DragPreviewV4, FloatOrigin, FocusRequest, FocusRequestCause, FrameWitness,
    HoldReason, IntentDesire, MAX_FOCUS_DEPARTING, MAX_FOCUS_MEMBERS, MAX_FOCUS_SELECTED,
    MAX_FOCUS_VIEWS, NativeCreationDesire, OperationEvent, OutputDesire, RowDiagnostic,
    ScopeStatus, SwitchEvent, TransferKind, TransferTargetV4, WarpRequest, WindowDesire,
    WorkspaceDesire, WorldPresentationV4, WorldReplyV4,
};
use tiler_core::world_roster::SharedRosterPlan;

use crate::world_protocol::{
    WORLD_MAX_REPLY_BYTES, WorldProtocolError, parse_u64_decimal, reply_snapshot, unknown_value,
};
use crate::world_reply_sink::{
    CountSink, ObjectWriter, ReplySink, SortedStringSink, StringSink, push_serde_json_string,
};
use crate::world_roster_wire::{check_publication, validate_plan, write_plan};

type Result<T> = std::result::Result<T, WorldProtocolError>;

/// Borrowed actuator association attached by the broker to bound replies.
#[derive(Clone, Copy)]
pub struct BrokerGestureAssoc<'a> {
    pub instance: &'a str,
    pub serial: &'a str,
    pub core_id: u64,
}

/// Borrowed broker envelope binding fields appended to an accepted v4 reply.
#[derive(Clone, Copy)]
pub struct BrokerReplyBind<'a> {
    pub source: &'a str,
    pub native_invalid: u64,
    pub clock: u64,
    pub gesture_assoc: Option<BrokerGestureAssoc<'a>>,
    pub gesture_settled_assoc: Option<BrokerGestureAssoc<'a>>,
}

fn check_reply_cap(len: usize, max: usize) -> Result<()> {
    if len > max {
        return Err(reply_snapshot("section-over-cap"));
    }
    Ok(())
}

fn check_reply_id(text: &str) -> Result<()> {
    if !is_opaque_id(text) {
        return Err(unknown_value());
    }
    Ok(())
}

fn write_rect<S: ReplySink>(sink: &mut S, rect: &Rect) {
    sink.push_char('[');
    sink.push_i32(rect.x);
    sink.push_char(',');
    sink.push_i32(rect.y);
    sink.push_char(',');
    sink.push_i32(rect.w);
    sink.push_char(',');
    sink.push_i32(rect.h);
    sink.push_char(']');
}

fn write_opt_rect<S: ReplySink>(sink: &mut S, rect: &Option<Rect>) -> Result<()> {
    match rect {
        None => sink.push_str("null"),
        Some(rect) => write_rect(sink, rect),
    }
    Ok(())
}

fn write_authority_scope<S: ReplySink>(sink: &mut S, scope: &AuthorityScope) -> Result<()> {
    sink.write_object(|obj| match scope {
        AuthorityScope::Output(output) => {
            obj.field_str("kind", "output")?;
            obj.field_str("out", &output.0)
        }
        AuthorityScope::Workspace { output, workspace } => {
            obj.field_str("kind", "workspace")?;
            obj.field_str("out", &output.0)?;
            obj.field_str("ws", &workspace.0)
        }
    })
}

fn write_scope_status_v4<S: ReplySink>(row: &ScopeStatus, sink: &mut S) -> Result<()> {
    sink.write_object(|obj| {
        obj.field("scope", |s| write_authority_scope(s, &row.scope))?;
        obj.field_str("auth", row.authority.as_str())?;
        obj.field_str(
            "reason",
            row.authority.hold_reason().map_or("", HoldReason::as_str),
        )
    })
}

fn write_workspace_desire_v4<S: ReplySink>(row: &WorkspaceDesire, sink: &mut S) -> Result<()> {
    sink.write_object(|obj| {
        obj.field_str("id", &row.id)?;
        obj.field_str("out", &row.output.0)?;
        obj.field_str("bind", row.native_binding.as_deref().unwrap_or(""))?;
        obj.field("scope", |s| write_authority_scope(s, &row.scope))?;
        obj.field("order", |s| {
            s.push_usize(row.order);
            Ok(())
        })?;
        obj.field_str("mode", row.tiling_mode.as_str())?;
        obj.field_bool("exist", row.existence)?;
        obj.field_str("hold", row.hold.map_or("", HoldReason::as_str))?;
        if row.trailing {
            obj.field_bool("trailing", true)?;
        }
        Ok(())
    })
}

fn write_output_desire_v4<S: ReplySink>(row: &OutputDesire, sink: &mut S) -> Result<()> {
    sink.write_object(|obj| {
        obj.field_str("out", &row.output.0)?;
        obj.field_str("view", row.logical_view.as_deref().unwrap_or(""))?;
        obj.field_str("mode", row.tiling_mode.as_str())?;
        obj.field_str("hold", row.hold.map_or("", HoldReason::as_str))
    })
}

fn write_window_target_lanes<S: ReplySink>(
    row: &WindowDesire,
    obj: &mut ObjectWriter<'_, S>,
    include_ops: bool,
) -> Result<()> {
    obj.field_opt_u64_str("cmdSeq", row.command_seq)?;
    obj.field("visibility", |s| match &row.visibility {
        None => {
            s.push_str("null");
            Ok(())
        }
        Some(target) => s.write_object(|vis| {
            vis.field_bool("shown", target.shown)?;
            vis.field_u64_str("rev", target.revision)?;
            vis.field_bool("fallback", target.fallback)
        }),
    })?;
    obj.field_str("visibility_auth", row.visibility_authority.as_str())?;
    obj.field_str(
        "visibility_hold",
        row.visibility_authority
            .hold_reason()
            .map_or("", HoldReason::as_str),
    )?;
    let write_state_target = |target: &tiler_core::world_report::StateTarget, s: &mut S| {
        if target.automatic_clear
            && (target.property != tiler_core::world_report::StateProperty::Maximized
                || target.value)
        {
            return Err(unknown_value());
        }
        s.write_object(|st| {
            st.field_str("property", target.property.as_str())?;
            st.field_bool("value", target.value)?;
            st.field_str("auth", target.authority.as_str())?;
            st.field_str(
                "reason",
                target
                    .authority
                    .hold_reason()
                    .map_or("", HoldReason::as_str),
            )?;
            st.field_opt_u64_str("cmdSeq", target.command_seq)?;
            if target.automatic_clear {
                st.field_bool("automaticClear", true)?;
            }
            Ok(())
        })
    };
    obj.field("state", |s| {
        s.push_char('[');
        let mut first = true;
        for target in &row.state {
            if !include_ops && target.automatic_clear {
                continue;
            }
            if !first {
                s.push_char(',');
            }
            first = false;
            write_state_target(target, s)?;
        }
        s.push_char(']');
        Ok(())
    })
}

fn write_window_desire_v4<S: ReplySink>(
    row: &WindowDesire,
    sink: &mut S,
    include_ops: bool,
) -> Result<()> {
    sink.write_object(|obj| {
        obj.field_str("win", &row.lifetime.0)?;
        obj.field_str("out", &row.output.0)?;
        obj.field_str("ws", &row.workspace)?;
        obj.field("alloc", |s| write_opt_rect(s, &row.allocation))?;
        obj.field("frame", |s| write_opt_rect(s, &row.effective_frame))?;
        obj.field_str("frame_auth", row.frame_authority.as_str())?;
        obj.field_str(
            "frame_hold",
            row.frame_authority
                .hold_reason()
                .map_or("", HoldReason::as_str),
        )?;
        obj.field_str("member_auth", row.membership_authority.as_str())?;
        obj.field_str(
            "member_hold",
            row.membership_authority
                .hold_reason()
                .map_or("", HoldReason::as_str),
        )?;
        if include_ops {
            obj.field_str("frame_op", row.frame_operation.as_deref().unwrap_or(""))?;
        }
        obj.field_bool("floating", row.floating)?;
        obj.field_str(
            "float_origin",
            row.float_origin.map_or("", FloatOrigin::as_str),
        )?;
        write_window_target_lanes(row, obj, include_ops)
    })
}

fn write_detached_ref<S: ReplySink>(sink: &mut S, reference: &DetachedRef) -> Result<()> {
    sink.write_object(|obj| {
        obj.field_str("kind", "detached")?;
        obj.field_str("out", &reference.output.0)?;
        obj.field_str("surface", reference.surface.as_str())
    })
}

fn write_detached_destination<S: ReplySink>(
    destination: &Option<DetachedDestination>,
    sink: &mut S,
) -> Result<()> {
    match destination {
        None => {
            sink.push_str("null");
            Ok(())
        }
        Some(destination) => sink.write_object(|obj| {
            obj.field("surface", |s| {
                write_detached_ref(s, &destination.surface)
            })?;
            obj.field_str("member", &destination.member.0)
        }),
    }
}

fn write_transfer_v4<S: ReplySink>(row: &TransferTargetV4, sink: &mut S) -> Result<()> {
    sink.write_object(|obj| {
        obj.field_str("op", &row.operation_id)?;
        obj.field_str(
            "kind",
            match &row.kind {
                TransferKind::Window(_) => "window",
                TransferKind::View => "view",
                TransferKind::Migration => "migration",
            },
        )?;
        obj.field_str(
            "win",
            match &row.kind {
                TransferKind::Window(window) => &window.0,
                TransferKind::View | TransferKind::Migration => "",
            },
        )?;
        obj.field_str("out", &row.target_output.0)?;
        obj.field_str("ws", &row.target_workspace)?;
        obj.field_bool("follow", row.follow)?;
        obj.field_bool("held", row.held)?;
        obj.field_opt_u64_str("cmdSeq", row.command_seq)?;
        obj.field("parent", |s| match row.parent.as_deref() {
            None => {
                s.push_str("null");
                Ok(())
            }
            Some(parent) => {
                s.push_json_str(parent);
                Ok(())
            }
        })?;
        obj.field("detachedDestination", |s| {
            write_detached_destination(&row.detached_destination, s)
        })
    })
}

fn write_after_shown_scope_v4<S: ReplySink>(scope: &AfterShownScope, sink: &mut S) -> Result<()> {
    sink.write_object(|obj| {
        obj.field_str("out", &scope.out.0)?;
        obj.field_str("ws", &scope.ws)
    })
}

fn write_after_shown_v4<S: ReplySink>(shown: &Option<AfterShown>, sink: &mut S) -> Result<()> {
    let Some(shown) = shown else {
        sink.push_str("null");
        return Ok(());
    };
    sink.write_object(|obj| {
        obj.field_str("op", &shown.op)?;
        obj.field_u64_str("obs", shown.obs)?;
        obj.field_array("members", &shown.members, |member, s| {
            s.write_object(|m| {
                m.field_str("win", &member.win.0)?;
                m.field_str("out", &member.out.0)?;
                m.field_str("ws", &member.ws)?;
                m.field("frame", |s| write_opt_rect(s, &member.frame))
            })
        })?;
        obj.field_array("views", &shown.views, write_after_shown_scope_v4)?;
        obj.field_array("selected", &shown.selected, write_after_shown_scope_v4)?;
        obj.field_array("departing", &shown.departing, write_after_shown_scope_v4)
    })
}

pub(crate) fn validate_focus_request_v4(request: &FocusRequest) -> Result<()> {
    check_reply_id(&request.id)?;
    check_reply_id(&request.target.0)?;
    check_reply_id(&request.output.0)?;
    check_reply_id(&request.workspace)?;
    let explicit = request.cause == FocusRequestCause::ExplicitCommand;
    if explicit != request.cmd_seq.is_some() || (!explicit && request.after_shown.is_some()) {
        return Err(reply_snapshot("encode-mismatch"));
    }
    if let Some(shown) = request.after_shown.as_ref() {
        check_reply_id(&shown.op)?;
        check_reply_cap(shown.members.len(), MAX_FOCUS_MEMBERS)?;
        check_reply_cap(shown.views.len(), MAX_FOCUS_VIEWS)?;
        check_reply_cap(shown.selected.len(), MAX_FOCUS_SELECTED)?;
        check_reply_cap(shown.departing.len(), MAX_FOCUS_DEPARTING)?;
        let mut members = BTreeSet::new();
        let mut views = BTreeSet::new();
        let mut selected = BTreeSet::new();
        let mut departing = BTreeSet::new();
        if !shown.members.iter().all(|row| members.insert(&row.win))
            || !shown
                .selected
                .iter()
                .all(|row| selected.insert((&row.out, &row.ws)))
            || !shown
                .views
                .iter()
                .all(|row| views.insert(&row.out) && selected.contains(&(&row.out, &row.ws)))
            || !shown
                .departing
                .iter()
                .all(|row| departing.insert((&row.out, &row.ws)))
        {
            return Err(reply_snapshot("encode-mismatch"));
        }
        for member in &shown.members {
            check_reply_id(&member.win.0)?;
            check_reply_id(&member.out.0)?;
            check_reply_id(&member.ws)?;
            if let Some(rect) = member.frame.as_ref()
                && !valid_carried_rect(rect.x, rect.y, rect.w, rect.h)
            {
                return Err(unknown_value());
            }
        }
        for scope in shown
            .views
            .iter()
            .chain(shown.selected.iter())
            .chain(shown.departing.iter())
        {
            check_reply_id(&scope.out.0)?;
            check_reply_id(&scope.ws)?;
        }
    }
    Ok(())
}

/// One retained cursor-warp request. Every tuple is frozen at mint; the
/// writer emits it verbatim (the same id never retargets or recenters) so
/// a re-emitted request is byte-identical while nothing changes.
fn write_warp_request_v4<S: ReplySink>(request: &WarpRequest, sink: &mut S) -> Result<()> {
    sink.write_object(|obj| {
        obj.field_str("id", &request.id)?;
        obj.field_str("target", &request.target.0)?;
        obj.field_u64_str("rev", request.revision)?;
        obj.field_str("out", &request.output.0)?;
        obj.field_str("ws", &request.workspace.0)?;
        obj.field("point", |s| {
            s.push_char('[');
            s.push_i32(request.point.0);
            s.push_char(',');
            s.push_i32(request.point.1);
            s.push_char(']');
            Ok(())
        })?;
        obj.field_opt_u64_str("pointerBaseline", request.pointer_baseline)
    })
}

pub(crate) fn write_focus_request_v4<S: ReplySink>(
    request: &FocusRequest,
    sink: &mut S,
) -> Result<()> {
    sink.write_object(|obj| {
        obj.field_str("id", &request.id)?;
        obj.field_str("target", &request.target.0)?;
        obj.field_u64_str("rev", request.issued_revision)?;
        obj.field_str("cause", request.cause.as_str())?;
        obj.field_str("out", &request.output.0)?;
        obj.field_str("ws", &request.workspace)?;
        obj.field_opt_u64_str("cmdSeq", request.cmd_seq)?;
        obj.field("afterShown", |s| {
            write_after_shown_v4(&request.after_shown, s)
        })
    })
}

fn write_operation_event_v4<S: ReplySink>(row: &OperationEvent, sink: &mut S) -> Result<()> {
    sink.write_object(|obj| {
        obj.field_str("win", &row.window.0)?;
        obj.field_str("kind", row.kind.as_str())?;
        obj.field_str("op", &row.operation_id)?;
        obj.field_str("outcome", row.outcome.as_str())?;
        obj.field_str("cause", row.cause.as_deref().unwrap_or_default())
    })
}

fn write_native_creation_v4<S: ReplySink>(row: &NativeCreationDesire, sink: &mut S) -> Result<()> {
    sink.write_object(|obj| {
        obj.field_str("op", &row.operation_id)?;
        obj.field_str("ws", &row.logical_workspace)?;
        obj.field_str("out", &row.target_output.0)
    })
}

fn write_drag_preview_v4<S: ReplySink>(
    preview: &Option<DragPreviewV4>,
    sink: &mut S,
) -> Result<()> {
    let Some(preview) = preview else {
        sink.push_str("null");
        return Ok(());
    };
    sink.write_object(|obj| {
        obj.field_u64_str("id", preview.id)?;
        obj.field_u64_str("rev", preview.revision)?;
        obj.field_str("domainOut", &preview.domain.output.0)?;
        obj.field_str("domainWs", &preview.domain.workspace.0)?;
        obj.field_str("sourceOut", &preview.source.output.0)?;
        obj.field_str("sourceWs", &preview.source.workspace.0)?;
        obj.field("rect", |s| {
            write_rect(s, &preview.rect);
            Ok(())
        })?;
        obj.field("hover", |s| match &preview.hover {
            None => {
                s.push_str("null");
                Ok(())
            }
            Some(hover) => s.write_object(|h| {
                h.field_str("domainOut", &hover.domain.output.0)?;
                h.field_str("domainWs", &hover.domain.workspace.0)?;
                h.field_str("leaf", &hover.source_leaf.0)?;
                h.field_str("win", &hover.source_window.0)?;
                h.field_u64_str("rev", hover.revision)?;
                h.field("prior", |s| match &hover.prior {
                    None => {
                        s.push_str("null");
                        Ok(())
                    }
                    Some(prior) => s.write_object(|p| {
                        p.field_str("group", &prior.group.0)?;
                        p.field_str("edge", prior.edge.as_str())
                    }),
                })?;
                h.field_str("priorOut", &hover.prior_domain.output.0)?;
                h.field_str("priorWs", &hover.prior_domain.workspace.0)
            }),
        })?;
        obj.field_str("win", &preview.target.0)
    })
}

fn write_gesture_settled_v4<S: ReplySink>(
    settled: &Option<GestureSettled>,
    sink: &mut S,
) -> Result<()> {
    let Some(terminal) = settled else {
        sink.push_str("null");
        return Ok(());
    };
    sink.write_object(|obj| {
        obj.field_u64_str("id", terminal.id)?;
        obj.field_bool("committed", terminal.committed)
    })
}

fn write_intent_desire_v4<S: ReplySink>(row: &IntentDesire, sink: &mut S) -> Result<()> {
    let encoded_data = row.data.as_ref().map(encode_intent_data);
    sink.write_object(|obj| {
        obj.field_str("win", &row.win.0)?;
        obj.field("data", |s| {
            match encoded_data.as_deref() {
                Some(encoded) => s.push_json_str(encoded),
                None => s.push_str("null"),
            }
            Ok(())
        })
    })
}

fn write_switch_event_v4<S: ReplySink>(row: &SwitchEvent, sink: &mut S) -> Result<()> {
    sink.write_object(|obj| {
        obj.field_str("out", &row.output.0)?;
        obj.field_str("op", &row.operation_id)?;
        obj.field_str("outcome", row.outcome.as_str())?;
        obj.field_array("failed", &row.failed, |window, s| {
            s.push_json_str(&window.0);
            Ok(())
        })?;
        obj.field_str("focus", row.focus.as_str())?;
        obj.field_bool("fallback", row.fallback)
    })
}

fn write_row_diagnostic_v4<S: ReplySink>(row: &RowDiagnostic, sink: &mut S) -> Result<()> {
    sink.write_object(|obj| {
        obj.field("scope", |s| write_authority_scope(s, &row.scope))?;
        obj.field_str("row", row.row.as_str())?;
        obj.field("id", |s| {
            match row.id.as_deref() {
                Some(id) => s.push_json_str(id),
                None => s.push_str("null"),
            }
            Ok(())
        })?;
        obj.field_str("kind", row.kind.as_str())
    })
}

fn write_frame_witness_v4<S: ReplySink>(witness: &FrameWitness, sink: &mut S) -> Result<()> {
    sink.write_object(|obj| {
        obj.field_str("win", &witness.win.0)?;
        obj.field_u64_str("obs", witness.obs)?;
        obj.field("frame", |s| {
            write_rect(s, &witness.frame);
            Ok(())
        })?;
        obj.field_str("foreground", &witness.foreground.0)
    })
}

fn write_border_presentation_v4<S: ReplySink>(
    row: &BorderPresentationV4,
    sink: &mut S,
) -> Result<()> {
    sink.write_object(|obj| {
        obj.field_bool("visible", row.visible)?;
        obj.field_str("reason", row.reason.map_or("", |r| r.as_str()))?;
        obj.field("subject", |s| {
            match &row.subject {
                None => s.push_str("null"),
                Some(subject) => s.push_json_str(&subject.0),
            }
            Ok(())
        })?;
        obj.field("outsets", |s| {
            match &row.outsets {
                None => s.push_str("null"),
                Some(outsets) => {
                    s.push_char('[');
                    s.push_i32(outsets[0]);
                    s.push_char(',');
                    s.push_i32(outsets[1]);
                    s.push_char(',');
                    s.push_i32(outsets[2]);
                    s.push_char(',');
                    s.push_i32(outsets[3]);
                    s.push_char(']');
                }
            }
            Ok(())
        })?;
        obj.field("width", |s| {
            match row.width {
                None => s.push_str("null"),
                Some(width) => s.push_i32(width),
            }
            Ok(())
        })?;
        obj.field("radius", |s| {
            match row.radius {
                None => s.push_str("null"),
                Some(radius) => s.push_i32(radius),
            }
            Ok(())
        })?;
        obj.field("witness", |s| match &row.witness {
            None => {
                s.push_str("null");
                Ok(())
            }
            Some(witness) => write_frame_witness_v4(witness, s),
        })
    })
}

fn write_underlay_v4<S: ReplySink>(row: &PresentationUnderlay, sink: &mut S) -> Result<()> {
    match row {
        PresentationUnderlay::Found(found) => sink.write_object(|obj| {
            obj.field_str("status", "found")?;
            obj.field_raw("reason", "\"\"")?;
            obj.field_u64_str("baseRev", found.base_revision)?;
            obj.field_str("homeOut", &found.home.output.0)?;
            obj.field_str("homeWs", &found.home.workspace.0)?;
            obj.field_str("group", &found.group.0)?;
            obj.field_str("leaf", &found.focused_leaf.0)?;
            obj.field_str("win", &found.focused_window.0)?;
            obj.field("union", |s| {
                write_rect(s, &found.union_rect);
                Ok(())
            })?;
            obj.field("outer", |s| {
                write_rect(s, &found.outer_rect);
                Ok(())
            })?;
            obj.field_array("members", &found.members, |member, s| {
                s.write_object(|m| {
                    m.field_str("leaf", &member.leaf.0)?;
                    m.field_str("win", &member.window.0)?;
                    m.field("rect", |s| {
                        write_rect(s, &member.rect);
                        Ok(())
                    })
                })
            })
        }),
        PresentationUnderlay::Hidden {
            base_revision,
            reason,
        } => sink.write_object(|obj| {
            obj.field_str("status", "hidden")?;
            obj.field_str("reason", reason.as_str())?;
            obj.field_opt_u64_str("baseRev", *base_revision)
        }),
    }
}

fn write_pager_v4<S: ReplySink>(pager: &[PagerScreen], sink: &mut S) -> Result<()> {
    sink.push_char('[');
    for (i, screen) in pager.iter().enumerate() {
        if i > 0 {
            sink.push_char(',');
        }
        sink.write_object(|scr| {
            scr.field_str("out", &screen.output)?;
            scr.field_array("workspaces", &screen.workspaces, |workspace, s| {
                s.write_object(|ws| {
                    ws.field_str("id", &workspace.id)?;
                    ws.field("ordinal", |s| {
                        s.push_u64(u64::from(workspace.ordinal));
                        Ok(())
                    })?;
                    ws.field_str("label", &workspace.label)?;
                    ws.field_bool("current", workspace.current)?;
                    ws.field_bool("occupied", workspace.occupied)?;
                    ws.field_bool("containsFocused", workspace.contains_focused)?;
                    if workspace.trailing {
                        ws.field_bool("trailing", true)?;
                    }
                    ws.field_array("windows", &workspace.windows, |window, s| {
                        s.write_object(|w| {
                            w.field_str("win", &window.window.0)?;
                            w.field("frame", |s| {
                                write_rect(s, &window.frame);
                                Ok(())
                            })
                        })
                    })
                })
            })
        })?;
    }
    sink.push_char(']');
    Ok(())
}

/// Border and underlay presentation fields shared by the full reply and the
/// renderer projection.
fn write_presentation_drawn_fields<S: ReplySink>(
    presentation: &WorldPresentationV4,
    obj: &mut ObjectWriter<'_, S>,
) -> Result<()> {
    obj.field("border", |s| {
        write_border_presentation_v4(&presentation.border, s)
    })?;
    obj.field("underlay", |s| write_underlay_v4(&presentation.underlay, s))
}

fn write_reply_presentation_v4<S: ReplySink>(
    presentation: &WorldPresentationV4,
    sink: &mut S,
) -> Result<()> {
    sink.write_object(|obj| {
        obj.field_str("foreground", &presentation.foreground)?;
        obj.field_u64_str("rev", presentation.rev)?;
        if let Some(output) = presentation.suspended_output.as_ref() {
            obj.field_str("suspendedOutput", &output.0)?;
        }
        write_presentation_drawn_fields(presentation, obj)?;
        obj.field("pager", |s| write_pager_v4(&presentation.pager, s))
    })
}

/// Retained renderer presentation: foreground plus the drawn border and
/// underlay. The full-reply `rev`, `suspendedOutput`, and `pager` never project.
fn write_projection_presentation_v4<S: ReplySink>(
    presentation: &WorldPresentationV4,
    sink: &mut S,
) -> Result<()> {
    sink.write_object(|obj| {
        obj.field_str("foreground", &presentation.foreground)?;
        write_presentation_drawn_fields(presentation, obj)
    })
}

fn write_gesture_assoc<S: ReplySink>(
    assoc: Option<BrokerGestureAssoc<'_>>,
    sink: &mut S,
) -> Result<()> {
    match assoc {
        None => {
            sink.push_str("null");
            Ok(())
        }
        Some(a) => sink.write_object(|obj| {
            obj.field_str("instance", a.instance)?;
            obj.field_str("serial", a.serial)?;
            obj.field_u64_str("coreId", a.core_id)
        }),
    }
}

fn map_validation_error(err: ReplyValidationError) -> WorldProtocolError {
    WorldProtocolError {
        kind: err.kind,
        message: err.message,
    }
}

fn write_reply_v4_to_sink<S: ReplySink, const VALIDATE: bool>(
    reply: &WorldReplyV4,
    roster_plan: Option<&SharedRosterPlan>,
    broker_bind: Option<&BrokerReplyBind<'_>>,
    sink: &mut S,
) -> Result<()> {
    if VALIDATE {
        validate_reply_v4(reply, None).map_err(map_validation_error)?;
    }
    if let Some(plan) = roster_plan {
        check_publication(
            Some(plan),
            &reply.core_generation,
            reply.state_revision,
            reply.based_on_observation_seq,
            reply.ready,
        )?;
        validate_plan(plan)?;
    }
    sink.write_object(|obj| {
        obj.field("v", |s| {
            s.push_u64(u64::from(reply.contract_version));
            Ok(())
        })?;
        obj.field_str("adapter", &reply.adapter_incarnation)?;
        obj.field_str("gen", &reply.core_generation)?;
        obj.field_str("req", &reply.request_id)?;
        obj.field_u64_str("obs", reply.based_on_observation_seq)?;
        obj.field_u64_str("rev", reply.state_revision)?;
        obj.field_bool("ready", reply.ready)?;
        obj.field_bool("consumed", reply.command_consumed)?;
        obj.field_array("scopes", &reply.scopes, write_scope_status_v4)?;
        obj.field_array("workspaces", &reply.workspaces, write_workspace_desire_v4)?;
        obj.field_array("outputs", &reply.outputs, write_output_desire_v4)?;
        obj.field_array("windows", &reply.windows, |w, s| {
            write_window_desire_v4(w, s, true)
        })?;
        obj.field_array("transfers", &reply.transfers, write_transfer_v4)?;
        obj.field("focusRequest", |s| match &reply.focus_request {
            None => {
                s.push_str("null");
                Ok(())
            }
            Some(request) => write_focus_request_v4(request, s),
        })?;
        obj.field("dragPreview", |s| {
            write_drag_preview_v4(&reply.drag_preview, s)
        })?;
        obj.field("gestureSettled", |s| {
            write_gesture_settled_v4(&reply.gesture_settled, s)
        })?;
        obj.field_array(
            "operationSettled",
            &reply.operation_settled,
            write_operation_event_v4,
        )?;
        obj.field_array(
            "nativeCreations",
            &reply.native_creations,
            write_native_creation_v4,
        )?;
        obj.field("warpRequest", |s| match &reply.warp_request {
            None => {
                s.push_str("null");
                Ok(())
            }
            Some(request) => write_warp_request_v4(request, s),
        })?;
        obj.field("presentation", |s| {
            write_reply_presentation_v4(&reply.presentation, s)
        })?;
        obj.field_array(
            "intentDesired",
            &reply.intent_desired,
            write_intent_desire_v4,
        )?;
        obj.field_array(
            "switchSettled",
            &reply.switch_settled,
            write_switch_event_v4,
        )?;
        obj.field_array("diagnostics", &reply.diagnostics, write_row_diagnostic_v4)?;
        if let Some(plan) = roster_plan {
            obj.field("rosterPlan", |s| write_plan(plan, s))?;
        }
        if let Some(bind) = broker_bind {
            obj.field_str("source", bind.source)?;
            obj.field_u64_str("nativeInvalid", bind.native_invalid)?;
            obj.field_u64_str("clock", bind.clock)?;
            obj.field("gestureAssoc", |s| {
                write_gesture_assoc(bind.gesture_assoc, s)
            })?;
            obj.field("gestureSettledAssoc", |s| {
                write_gesture_assoc(bind.gesture_settled_assoc, s)
            })?;
        }
        Ok(())
    })?;
    Ok(())
}

fn write_projection_preview<S: ReplySink, const CHECK_PRODUCER: bool>(
    preview: Option<&DragPreviewV4>,
    suspended_output: Option<&tiler_core::directional::OutputId>,
    producer: Option<(&str, &str)>,
    sink: &mut S,
) -> Result<()> {
    let Some(preview) = preview else {
        sink.push_str("null");
        return Ok(());
    };
    if suspended_output.is_some_and(|suspended| *suspended == preview.domain.output) {
        sink.push_str("null");
        return Ok(());
    }
    if CHECK_PRODUCER
        && let Some((instance, serial)) = producer
        && (!is_opaque_id(instance) || parse_u64_decimal(serial).is_none_or(|value| value == 0))
    {
        return Err(unknown_value());
    }
    sink.write_object(|obj| {
        obj.field("rect", |s| {
            write_rect(s, &preview.rect);
            Ok(())
        })?;
        obj.field_str("sourceOut", &preview.source.output.0)?;
        obj.field_str("sourceWs", &preview.source.workspace.0)?;
        obj.field_str("win", &preview.target.0)?;
        obj.field("producerInstance", |s| {
            match producer {
                Some((instance, _)) => s.push_json_str(instance),
                None => s.push_str("null"),
            }
            Ok(())
        })?;
        obj.field("producerSerial", |s| {
            match producer {
                Some((_, serial)) => s.push_json_str(serial),
                None => s.push_str("null"),
            }
            Ok(())
        })
    })
}

/// Exact canonical v4 reply byte count over a borrowed [`WorldReplyV4`].
/// Measures canonical serialized size without running writer-only validation so
/// admission size gates preserve the final-prepare validation checkpoint.
#[must_use]
pub fn count_reply_v4_bytes(reply: &WorldReplyV4) -> usize {
    let mut sink = CountSink(0);
    let _ = write_reply_v4_to_sink::<_, false>(reply, None, None, &mut sink);
    sink.0
}

#[must_use]
pub(crate) fn count_operation_settled_rows_bytes(settled: &[OperationEvent]) -> usize {
    let mut sink = CountSink(0);
    for (i, event) in settled.iter().enumerate() {
        if i > 0 {
            sink.push_char(',');
        }
        let _ = write_operation_event_v4(event, &mut sink);
    }
    sink.0
}

#[must_use]
pub(crate) fn count_switch_settled_section_bytes(settled: &[SwitchEvent]) -> usize {
    let mut sink = CountSink(0);
    sink.push_str(",\"switchSettled\":[");
    for (i, event) in settled.iter().enumerate() {
        if i > 0 {
            sink.push_char(',');
        }
        let _ = write_switch_event_v4(event, &mut sink);
    }
    sink.push_char(']');
    sink.0
}

/// Exact canonical v4 reply byte count over a borrowed [`WorldReplyV4`]. Runs
/// the identical validation and traversal as [`write_reply_v4_into`], without
/// allocating output bytes or enforcing the 4 MiB transport cap so callers can
/// measure headroom or over-budget sizes.
pub fn count_reply_v4(reply: &WorldReplyV4) -> Result<usize> {
    let mut sink = CountSink(0);
    write_reply_v4_to_sink::<_, true>(reply, None, None, &mut sink)?;
    Ok(sink.0)
}

/// Checked canonical v4 reply writer appending into `out` from a borrowed
/// [`WorldReplyV4`]. Does not enforce the 4 MiB transport cap so outer envelope
/// callers can append broker fields before checking the final bound.
pub fn write_reply_v4_into(reply: &WorldReplyV4, out: &mut String) -> Result<()> {
    let mut sink = StringSink(out);
    write_reply_v4_to_sink::<_, true>(reply, None, None, &mut sink)
}

/// Checked canonical encode of a borrowed v4 reply, enforcing the 4 MiB
/// transport reply bound.
pub fn write_reply_v4(reply: &WorldReplyV4) -> Result<String> {
    write_reply_v4_with_plan(reply, None)
}

/// Checked canonical reply emission, including any shared-roster publication.
pub fn write_reply_v4_with_plan(
    reply: &WorldReplyV4,
    roster_plan: Option<&SharedRosterPlan>,
) -> Result<String> {
    let mut out = String::new();
    write_reply_v4_to_sink::<_, true>(reply, roster_plan, None, &mut StringSink(&mut out))?;
    if out.len() > WORLD_MAX_REPLY_BYTES {
        return Err(reply_snapshot("encode-mismatch"));
    }
    Ok(out)
}

/// Checked broker-bound v4 reply writer over a borrowed [`WorldReplyV4`] and
/// [`BrokerReplyBind`]. Emits lexicographic object key order and `serde_json`
/// string escaping directly without cloning the reply or reparsing JSON.
pub fn write_bound_reply_v4(
    reply: &WorldReplyV4,
    roster_plan: Option<&SharedRosterPlan>,
    bind: &BrokerReplyBind<'_>,
) -> Result<String> {
    check_reply_id(bind.source)?;
    let mut sink = SortedStringSink::new();
    write_reply_v4_to_sink::<_, true>(reply, roster_plan, Some(bind), &mut sink)?;
    Ok(sink.finish())
}

/// Prevalidated broker-bound v4 reply writer for callers that already proved
/// [`validate_reply_v4`] on `reply`. Performs envelope-only checks on `bind`
/// without revalidating typed reply rows.
pub fn write_bound_reply_v4_prevalidated(
    validated: ValidatedReply<'_>,
    roster_plan: Option<&SharedRosterPlan>,
    bind: &BrokerReplyBind<'_>,
) -> Result<String> {
    let reply = validated.reply();
    check_reply_id(bind.source)?;
    let mut sink = SortedStringSink::new();
    write_reply_v4_to_sink::<_, false>(reply, roster_plan, Some(bind), &mut sink)?;
    Ok(sink.finish())
}

/// Serialized canonical renderer projection with a known read-binding offset.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CachedProjectionV4 {
    body: String,
    req_split: usize,
}

impl CachedProjectionV4 {
    /// Assemble a bound read reply in lexicographic order with
    /// `serde_json`-compatible string escaping.
    pub fn bind_read(&self, generation: &str, req: &str, covered: Option<(&str, &str)>) -> String {
        // These three binding fields sort before every projection field;
        // the writer records the position of the remaining field (`req`).
        let covered_extra = covered.map_or(8, |(inst, inv)| inst.len() + inv.len() + 4);
        let mut out = String::with_capacity(
            self.body.len() + generation.len() + req.len() + covered_extra + 72,
        );
        out.push_str("{\"coveredInstance\":");
        match covered {
            Some((instance, _)) => push_serde_json_string(&mut out, instance),
            None => out.push_str("null"),
        }
        out.push_str(",\"coveredInvalid\":");
        match covered {
            Some((_, invalid)) => push_serde_json_string(&mut out, invalid),
            None => out.push_str("null"),
        }
        out.push_str(",\"gen\":");
        push_serde_json_string(&mut out, generation);
        out.push(',');
        out.push_str(&self.body[1..self.req_split]);
        out.push_str(",\"req\":");
        push_serde_json_string(&mut out, req);
        out.push_str(&self.body[self.req_split..]);
        out
    }
}

fn write_cached_projection_v4_inner<const CHECK_PRODUCER: bool>(
    reply: &WorldReplyV4,
    producer: Option<(&str, &str)>,
) -> Result<CachedProjectionV4> {
    let mut sink = SortedStringSink::new();
    let req_split = sink.write_object_split(Some("req"), |obj| {
        obj.field("v", |s| {
            s.push_u64(u64::from(reply.contract_version));
            Ok(())
        })?;
        obj.field_str("outcome", "presentation")?;
        obj.field_u64_str("rev", reply.state_revision)?;
        obj.field_u64_str("obs", reply.based_on_observation_seq)?;
        obj.field_array("windows", &reply.windows, |w, s| {
            write_window_desire_v4(w, s, false)
        })?;
        obj.field("presentation", |s| {
            write_projection_presentation_v4(&reply.presentation, s)
        })?;
        obj.field("preview", |s| {
            write_projection_preview::<_, CHECK_PRODUCER>(
                reply.drag_preview.as_ref(),
                reply.presentation.suspended_output.as_ref(),
                producer,
                s,
            )
        })
    })?;
    Ok(CachedProjectionV4 {
        body: sink.finish(),
        req_split,
    })
}

/// Checked renderer projection writer over a borrowed [`WorldReplyV4`] that
/// returns a cached canonical body ready for zero-parse read binding.
pub fn write_cached_projection_v4(
    reply: &WorldReplyV4,
    producer: Option<(&str, &str)>,
) -> Result<CachedProjectionV4> {
    validate_projection_reply_v4(reply).map_err(map_validation_error)?;
    write_cached_projection_v4_inner::<true>(reply, producer)
}

/// Prevalidated renderer projection writer that returns a cached canonical body
/// ready for zero-parse read binding without revalidating typed reply rows.
/// Any supplied producer must come from the validated broker gesture lane.
pub fn write_cached_projection_v4_prevalidated(
    validated: ValidatedReply<'_>,
    producer: Option<(&str, &str)>,
) -> Result<CachedProjectionV4> {
    write_cached_projection_v4_inner::<false>(validated.reply(), producer)
}

/// Checked renderer projection writer over a borrowed [`WorldReplyV4`].
/// Emits the retained surface only: `windows` without `frame_op` or
/// automatic-clear state targets, the foreground/border/underlay
/// `presentation`, and the carried `preview`, in lexicographic object key
/// order with `serde_json` string escaping.
pub fn write_projection_reply_v4(
    reply: &WorldReplyV4,
    producer: Option<(&str, &str)>,
) -> Result<String> {
    Ok(write_cached_projection_v4(reply, producer)?.body)
}

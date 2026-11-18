//! Checked canonical v4 reply writer, exact byte counter, and broker
//! bound/projection emitters.
//!
//! Operates on a borrowed [`WorldReplyV4`] plus [`ReplyWireOptionals`] and
//! drives exact byte counting, canonical wire emission, broker bound emission,
//! and renderer projection emission through the same typed schema traversal
//! and validation, with no intermediate JSON object model and no round-trip
//! re-parse.

use std::collections::BTreeSet;

use tiler_core::bounds::{GEOMETRY_BOUND, is_opaque_id, valid_carried_rect};
use tiler_core::geometry::Rect;
use tiler_core::gesture::GestureSettled;
use tiler_core::ids::{MAX_CORRELATION_LEN, MAX_GENERATION_LEN};
use tiler_core::intent_data::{MAX_INTENT_DATA_BYTES, encode_intent_data};
use tiler_core::world_presentation::{PagerScreen, PresentationUnderlay};
use tiler_core::world_report::{
    AfterShown, AfterShownScope, AuthorityScope, BorderPresentationV4, CommandRejectKind,
    DragPreviewV4, FloatOrigin, FocusRequest, FocusRequestCause, FrameWitness, HoldReason,
    IntentDesire, MAX_FOCUS_DEPARTING, MAX_FOCUS_MEMBERS, MAX_FOCUS_SELECTED, MAX_FOCUS_VIEWS,
    MAX_INTENT_DESIRED, MAX_REPLY_DIAGNOSTICS, MAX_REPLY_EFFECTS, MAX_REPLY_NATIVE_CREATIONS,
    MAX_REPLY_OPERATIONS, MAX_REPLY_OUTPUTS, MAX_REPLY_SCOPES, MAX_REPLY_SWITCH_EVENTS,
    MAX_REPLY_TRANSFERS, MAX_REPLY_WINDOWS, MAX_REPLY_WORKSPACES, MAX_STATE_TARGETS_PER_WINDOW,
    NativeCreationDesire, OperationEvent, OutputDesire, RowDiagnostic, ScopeStatus, SwitchEvent,
    TransferKind, TransferTargetV4, WindowDesire, WorkspaceDesire, WorldPresentationV4,
    WorldReplyV4,
};

use crate::world_protocol::{
    KIND_VERSION, MSG_VERSION, ReplyWireOptionals, WORLD_MAX_REPLY_BYTES,
    WORLD_PROTOCOL_VERSION_V4, WorldProtocolError, is_correlation_text, is_generation_text,
    reply_snapshot, unknown_value,
};
use crate::world_reply_sink::{CountSink, ObjectWriter, ReplySink, SortedStringSink, StringSink};

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

fn write_checked_rect<S: ReplySink, const VALIDATE: bool>(sink: &mut S, rect: &Rect) -> Result<()> {
    if VALIDATE && !valid_carried_rect(rect.x, rect.y, rect.w, rect.h) {
        return Err(unknown_value());
    }
    write_rect(sink, rect);
    Ok(())
}

fn write_checked_opt_rect<S: ReplySink, const VALIDATE: bool>(
    sink: &mut S,
    rect: &Option<Rect>,
) -> Result<()> {
    match rect {
        None => sink.push_str("null"),
        Some(rect) => write_checked_rect::<S, VALIDATE>(sink, rect)?,
    }
    Ok(())
}

fn write_empty_or_id<S: ReplySink, const VALIDATE: bool>(sink: &mut S, text: &str) -> Result<()> {
    if VALIDATE && !text.is_empty() {
        check_reply_id(text)?;
    }
    sink.push_json_str(text);
    Ok(())
}

fn write_opt_id_or_empty<S: ReplySink, const VALIDATE: bool>(
    sink: &mut S,
    value: &Option<String>,
) -> Result<()> {
    match value {
        None => sink.push_str("\"\""),
        Some(text) => {
            if VALIDATE {
                check_reply_id(text)?;
            }
            sink.push_json_str(text);
        }
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

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum ScopeKey<'a> {
    Output(&'a str),
    Workspace(&'a str, &'a str),
}

fn scope_status_key(scope: &AuthorityScope) -> ScopeKey<'_> {
    match scope {
        AuthorityScope::Output(output) => ScopeKey::Output(&output.0),
        AuthorityScope::Workspace { output, workspace } => {
            ScopeKey::Workspace(&output.0, &workspace.0)
        }
    }
}

fn write_scope_status_v4<S: ReplySink, const VALIDATE: bool>(
    row: &ScopeStatus,
    sink: &mut S,
) -> Result<()> {
    if VALIDATE {
        match &row.scope {
            AuthorityScope::Output(output) => check_reply_id(&output.0)?,
            AuthorityScope::Workspace { output, workspace } => {
                check_reply_id(&output.0)?;
                check_reply_id(&workspace.0)?;
            }
        }
    }
    sink.write_object(|obj| {
        obj.field("scope", |s| write_authority_scope(s, &row.scope))?;
        obj.field_str("auth", row.authority.as_str())?;
        obj.field_str(
            "reason",
            row.authority.hold_reason().map_or("", HoldReason::as_str),
        )
    })
}

fn write_workspace_desire_v4<S: ReplySink, const VALIDATE: bool>(
    row: &WorkspaceDesire,
    sink: &mut S,
) -> Result<()> {
    if VALIDATE {
        check_reply_id(&row.id)?;
        check_reply_id(&row.output.0)?;
        if let Some(bind) = row.native_binding.as_deref() {
            check_reply_id(bind)?;
        }
        match &row.scope {
            AuthorityScope::Output(output) => check_reply_id(&output.0)?,
            AuthorityScope::Workspace { output, workspace } => {
                check_reply_id(&output.0)?;
                check_reply_id(&workspace.0)?;
            }
        }
    }
    sink.write_object(|obj| {
        obj.field_str("id", &row.id)?;
        obj.field_str("out", &row.output.0)?;
        obj.field("bind", |s| {
            write_opt_id_or_empty::<S, VALIDATE>(s, &row.native_binding)
        })?;
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

fn write_output_desire_v4<S: ReplySink, const VALIDATE: bool>(
    row: &OutputDesire,
    sink: &mut S,
) -> Result<()> {
    if VALIDATE {
        check_reply_id(&row.output.0)?;
        if let Some(view) = row.logical_view.as_deref() {
            check_reply_id(view)?;
        }
    }
    sink.write_object(|obj| {
        obj.field_str("out", &row.output.0)?;
        obj.field("view", |s| {
            write_opt_id_or_empty::<S, VALIDATE>(s, &row.logical_view)
        })?;
        obj.field_str("mode", row.tiling_mode.as_str())?;
        obj.field_str("hold", row.hold.map_or("", HoldReason::as_str))
    })
}

fn write_window_target_lanes<S: ReplySink>(
    row: &WindowDesire,
    obj: &mut ObjectWriter<'_, S>,
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
    obj.field_array("state", &row.state, |target, s| {
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
            st.field_opt_u64_str("cmdSeq", target.command_seq)
        })
    })
}

fn write_window_desire_v4<S: ReplySink, const VALIDATE: bool>(
    row: &WindowDesire,
    sink: &mut S,
    include_ops: bool,
) -> Result<()> {
    if VALIDATE {
        check_reply_id(&row.lifetime.0)?;
        check_reply_id(&row.output.0)?;
        check_reply_id(&row.workspace)?;
        if let Some(rect) = row.allocation.as_ref()
            && !valid_carried_rect(rect.x, rect.y, rect.w, rect.h)
        {
            return Err(unknown_value());
        }
        if let Some(rect) = row.effective_frame.as_ref()
            && !valid_carried_rect(rect.x, rect.y, rect.w, rect.h)
        {
            return Err(unknown_value());
        }
        if let Some(op) = row.frame_operation.as_deref() {
            check_reply_id(op)?;
        }
        if let Some(op) = row.clear_operation.as_deref() {
            check_reply_id(op)?;
        }
        if let Some(visibility) = row.visibility.as_ref()
            && visibility.shown
            && visibility.fallback
        {
            return Err(reply_snapshot("visibility-show-fallback"));
        }
        if row.state.len() > MAX_STATE_TARGETS_PER_WINDOW {
            return Err(reply_snapshot("state-target-count"));
        }
        let mut seen_state = BTreeSet::new();
        for target in &row.state {
            if !seen_state.insert(target.property) {
                return Err(reply_snapshot("state-target-count"));
            }
        }
    }
    sink.write_object(|obj| {
        obj.field_str("win", &row.lifetime.0)?;
        obj.field_str("out", &row.output.0)?;
        obj.field_str("ws", &row.workspace)?;
        obj.field("alloc", |s| {
            write_checked_opt_rect::<S, VALIDATE>(s, &row.allocation)
        })?;
        obj.field("frame", |s| {
            write_checked_opt_rect::<S, VALIDATE>(s, &row.effective_frame)
        })?;
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
            obj.field("frame_op", |s| {
                write_opt_id_or_empty::<S, VALIDATE>(s, &row.frame_operation)
            })?;
        }
        obj.field_bool("floating", row.floating)?;
        obj.field_str(
            "float_origin",
            row.float_origin.map_or("", FloatOrigin::as_str),
        )?;
        if include_ops {
            obj.field("clear_op", |s| {
                write_opt_id_or_empty::<S, VALIDATE>(s, &row.clear_operation)
            })?;
        }
        write_window_target_lanes(row, obj)
    })
}

fn write_transfer_v4<S: ReplySink, const VALIDATE: bool>(
    row: &TransferTargetV4,
    sink: &mut S,
) -> Result<()> {
    if VALIDATE {
        check_reply_id(&row.operation_id)?;
        check_reply_id(&row.target_output.0)?;
        check_reply_id(&row.target_workspace)?;
        match &row.kind {
            TransferKind::Window(window) => check_reply_id(&window.0)?,
            TransferKind::View | TransferKind::Migration => {}
        }
    }
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
        obj.field_opt_u64_str("cmdSeq", row.command_seq)
    })
}

fn write_after_shown_scope_v4<S: ReplySink, const VALIDATE: bool>(
    scope: &AfterShownScope,
    sink: &mut S,
) -> Result<()> {
    if VALIDATE {
        check_reply_id(&scope.out.0)?;
        check_reply_id(&scope.ws)?;
    }
    sink.write_object(|obj| {
        obj.field_str("out", &scope.out.0)?;
        obj.field_str("ws", &scope.ws)
    })
}

fn write_after_shown_v4<S: ReplySink, const VALIDATE: bool>(
    shown: &Option<AfterShown>,
    sink: &mut S,
) -> Result<()> {
    let Some(shown) = shown else {
        sink.push_str("null");
        return Ok(());
    };
    if VALIDATE {
        if shown.members.len() > MAX_FOCUS_MEMBERS {
            return Err(reply_snapshot("focus-members-over-cap"));
        }
        if shown.views.len() > MAX_FOCUS_VIEWS {
            return Err(reply_snapshot("focus-views-over-cap"));
        }
        if shown.selected.len() > MAX_FOCUS_SELECTED {
            return Err(reply_snapshot("focus-selected-over-cap"));
        }
        if shown.departing.len() > MAX_FOCUS_DEPARTING {
            return Err(reply_snapshot("focus-departing-over-cap"));
        }
        check_reply_id(&shown.op)?;
    }
    sink.write_object(|obj| {
        obj.field_str("op", &shown.op)?;
        obj.field_u64_str("obs", shown.obs)?;
        obj.field_array("members", &shown.members, |member, s| {
            if VALIDATE {
                check_reply_id(&member.win.0)?;
                check_reply_id(&member.out.0)?;
                check_reply_id(&member.ws)?;
            }
            s.write_object(|m| {
                m.field_str("win", &member.win.0)?;
                m.field_str("out", &member.out.0)?;
                m.field_str("ws", &member.ws)?;
                m.field("frame", |s| {
                    write_checked_opt_rect::<S, VALIDATE>(s, &member.frame)
                })
            })
        })?;
        obj.field_array(
            "views",
            &shown.views,
            write_after_shown_scope_v4::<S, VALIDATE>,
        )?;
        obj.field_array(
            "selected",
            &shown.selected,
            write_after_shown_scope_v4::<S, VALIDATE>,
        )?;
        obj.field_array(
            "departing",
            &shown.departing,
            write_after_shown_scope_v4::<S, VALIDATE>,
        )
    })
}

fn check_after_shown_invariants(shown: &AfterShown) -> Result<()> {
    let mut seen_members = BTreeSet::new();
    for member in &shown.members {
        if !seen_members.insert(member.win.0.as_str()) {
            return Err(reply_snapshot("encode-mismatch"));
        }
    }
    let mut seen_view_outs = BTreeSet::new();
    for scope in &shown.views {
        if !seen_view_outs.insert(scope.out.0.as_str()) {
            return Err(reply_snapshot("encode-mismatch"));
        }
    }
    let mut seen_selected = BTreeSet::new();
    for scope in &shown.selected {
        if !seen_selected.insert((scope.out.0.as_str(), scope.ws.as_str())) {
            return Err(reply_snapshot("encode-mismatch"));
        }
    }
    for view in &shown.views {
        if !seen_selected.contains(&(view.out.0.as_str(), view.ws.as_str())) {
            return Err(reply_snapshot("encode-mismatch"));
        }
    }
    let mut seen_departing = BTreeSet::new();
    for scope in &shown.departing {
        if !seen_departing.insert((scope.out.0.as_str(), scope.ws.as_str())) {
            return Err(reply_snapshot("encode-mismatch"));
        }
    }
    Ok(())
}

fn write_focus_request_v4<S: ReplySink, const VALIDATE: bool>(
    request: &FocusRequest,
    sink: &mut S,
) -> Result<()> {
    if VALIDATE {
        check_reply_id(&request.id)?;
        check_reply_id(&request.target.0)?;
        check_reply_id(&request.output.0)?;
        check_reply_id(&request.workspace)?;
        let explicit = request.cause == FocusRequestCause::ExplicitCommand;
        if explicit != request.cmd_seq.is_some() {
            return Err(reply_snapshot("focus-request-cmdseq-mismatch"));
        }
        if request.after_shown.is_some() && !explicit {
            return Err(reply_snapshot("focus-request-aftershown-non-explicit"));
        }
    }
    sink.write_object(|obj| {
        obj.field_str("id", &request.id)?;
        obj.field_str("target", &request.target.0)?;
        obj.field_u64_str("rev", request.issued_revision)?;
        obj.field_str("cause", request.cause.as_str())?;
        obj.field_str("out", &request.output.0)?;
        obj.field_str("ws", &request.workspace)?;
        obj.field_opt_u64_str("cmdSeq", request.cmd_seq)?;
        obj.field("afterShown", |s| {
            write_after_shown_v4::<S, VALIDATE>(&request.after_shown, s)
        })
    })
}

fn valid_operation_cause(text: &str) -> bool {
    if CommandRejectKind::parse(text).is_some() {
        return true;
    }
    matches!(
        text,
        "baseline-non-subject"
            | "closed"
            | "constraints"
            | "early-verdict"
            | "external"
            | "fixed"
            | "floating"
            | "homing-loss"
            | "invalid-direction"
            | "invalid-geometry"
            | "non-subject-focus"
            | "refused-with-unmaximized"
            | "sticky"
            | "tiling-changed"
            | "topology-unreadable"
            | "unknown-output"
            | "unknown-window"
    )
}

fn write_operation_event_v4<S: ReplySink, const VALIDATE: bool>(
    row: &OperationEvent,
    sink: &mut S,
) -> Result<()> {
    if VALIDATE {
        check_reply_id(&row.window.0)?;
        check_reply_id(&row.operation_id)?;
        if row.outcome.kind() != row.kind {
            return Err(unknown_value());
        }
        if let Some(cause) = row.cause.as_deref()
            && !valid_operation_cause(cause)
        {
            return Err(unknown_value());
        }
    }
    sink.write_object(|obj| {
        obj.field_str("win", &row.window.0)?;
        obj.field_str("kind", row.kind.as_str())?;
        obj.field_str("op", &row.operation_id)?;
        obj.field_str("outcome", row.outcome.as_str())?;
        obj.field("cause", |s| {
            write_empty_or_id::<S, VALIDATE>(s, row.cause.as_deref().unwrap_or_default())
        })
    })
}

fn write_native_creation_v4<S: ReplySink, const VALIDATE: bool>(
    row: &NativeCreationDesire,
    sink: &mut S,
) -> Result<()> {
    if VALIDATE {
        check_reply_id(&row.operation_id)?;
        check_reply_id(&row.logical_workspace)?;
        check_reply_id(&row.target_output.0)?;
    }
    sink.write_object(|obj| {
        obj.field_str("op", &row.operation_id)?;
        obj.field_str("ws", &row.logical_workspace)?;
        obj.field_str("out", &row.target_output.0)
    })
}

fn write_drag_preview_v4<S: ReplySink, const VALIDATE: bool>(
    preview: &Option<DragPreviewV4>,
    sink: &mut S,
) -> Result<()> {
    let Some(preview) = preview else {
        sink.push_str("null");
        return Ok(());
    };
    if VALIDATE {
        if !valid_carried_rect(
            preview.rect.x,
            preview.rect.y,
            preview.rect.w,
            preview.rect.h,
        ) {
            return Err(unknown_value());
        }
        check_reply_id(&preview.domain.output.0)?;
        check_reply_id(&preview.domain.workspace.0)?;
        check_reply_id(&preview.source.output.0)?;
        check_reply_id(&preview.source.workspace.0)?;
        check_reply_id(&preview.target.0)?;
        if let Some(hover) = &preview.hover {
            check_reply_id(&hover.domain.output.0)?;
            check_reply_id(&hover.domain.workspace.0)?;
            check_reply_id(&hover.source_leaf.0)?;
            check_reply_id(&hover.source_window.0)?;
            if let Some(prior) = hover.prior.as_ref() {
                check_reply_id(&prior.group.0)?;
            }
            check_reply_id(&hover.prior_domain.output.0)?;
            check_reply_id(&hover.prior_domain.workspace.0)?;
        }
    }
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

fn write_intent_desire_v4<S: ReplySink, const VALIDATE: bool>(
    row: &IntentDesire,
    sink: &mut S,
) -> Result<()> {
    if VALIDATE {
        check_reply_id(&row.win.0)?;
    }
    let encoded_data = match row.data.as_ref() {
        Some(data) => {
            let encoded = encode_intent_data(data);
            if VALIDATE && encoded.len() > MAX_INTENT_DATA_BYTES {
                return Err(reply_snapshot("intent-data-invalid"));
            }
            Some(encoded)
        }
        None => None,
    };
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

fn write_switch_event_v4<S: ReplySink, const VALIDATE: bool>(
    row: &SwitchEvent,
    sink: &mut S,
) -> Result<()> {
    if VALIDATE {
        check_reply_id(&row.output.0)?;
        check_reply_id(&row.operation_id)?;
        check_reply_cap(row.failed.len(), MAX_REPLY_WINDOWS)?;
        for window in &row.failed {
            check_reply_id(&window.0)?;
        }
    }
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

fn write_row_diagnostic_v4<S: ReplySink, const VALIDATE: bool>(
    row: &RowDiagnostic,
    sink: &mut S,
) -> Result<()> {
    if VALIDATE {
        if let Some(id) = row.id.as_deref() {
            check_reply_id(id)?;
        }
        match &row.scope {
            AuthorityScope::Output(output) => check_reply_id(&output.0)?,
            AuthorityScope::Workspace { output, workspace } => {
                check_reply_id(&output.0)?;
                check_reply_id(&workspace.0)?;
            }
        }
    }
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

fn write_frame_witness_v4<S: ReplySink, const VALIDATE: bool>(
    witness: &FrameWitness,
    sink: &mut S,
) -> Result<()> {
    if VALIDATE {
        check_reply_id(&witness.win.0)?;
        if !valid_carried_rect(
            witness.frame.x,
            witness.frame.y,
            witness.frame.w,
            witness.frame.h,
        ) {
            return Err(unknown_value());
        }
        check_reply_id(&witness.foreground.0)?;
    }
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

fn write_border_presentation_v4<S: ReplySink, const VALIDATE: bool>(
    row: &BorderPresentationV4,
    sink: &mut S,
) -> Result<()> {
    if VALIDATE {
        if row.visible && row.reason.is_some() {
            return Err(unknown_value());
        }
        if let Some(subject) = row.subject.as_ref() {
            check_reply_id(&subject.0)?;
        }
        if let Some(width) = row.width
            && width < 0
        {
            return Err(unknown_value());
        }
        if let Some(radius) = row.radius
            && radius < 0
        {
            return Err(unknown_value());
        }
    }
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
            Some(witness) => write_frame_witness_v4::<S, VALIDATE>(witness, s),
        })
    })
}

fn write_underlay_v4<S: ReplySink, const VALIDATE: bool>(
    row: &PresentationUnderlay,
    sink: &mut S,
) -> Result<()> {
    match row {
        PresentationUnderlay::Found(found) => {
            if VALIDATE {
                check_reply_id(&found.home.output.0)?;
                check_reply_id(&found.home.workspace.0)?;
                check_reply_id(&found.group.0)?;
                check_reply_id(&found.focused_leaf.0)?;
                check_reply_id(&found.focused_window.0)?;
                if !valid_carried_rect(
                    found.union_rect.x,
                    found.union_rect.y,
                    found.union_rect.w,
                    found.union_rect.h,
                ) {
                    return Err(unknown_value());
                }
                if !valid_carried_rect(
                    found.outer_rect.x,
                    found.outer_rect.y,
                    found.outer_rect.w,
                    found.outer_rect.h,
                ) {
                    return Err(unknown_value());
                }
                check_reply_cap(found.members.len(), MAX_REPLY_WINDOWS)?;
                for member in &found.members {
                    check_reply_id(&member.leaf.0)?;
                    check_reply_id(&member.window.0)?;
                    if !valid_carried_rect(
                        member.rect.x,
                        member.rect.y,
                        member.rect.w,
                        member.rect.h,
                    ) {
                        return Err(unknown_value());
                    }
                }
            }
            sink.write_object(|obj| {
                obj.field_str("status", "found")?;
                obj.field_raw("reason", "\"\"")?;
                obj.field_u64_str("baseRev", found.base_revision)?;
                obj.field_str("homeOut", &found.home.output.0)?;
                obj.field_str("homeWs", &found.home.workspace.0)?;
                obj.field_str("group", &found.group.0)?;
                obj.field_str("leaf", &found.focused_leaf.0)?;
                obj.field_str("win", &found.focused_window.0)?;
                obj.field("union", |s| {
                    write_checked_rect::<S, VALIDATE>(s, &found.union_rect)
                })?;
                obj.field("outer", |s| {
                    write_checked_rect::<S, VALIDATE>(s, &found.outer_rect)
                })?;
                obj.field_array("members", &found.members, |member, s| {
                    s.write_object(|m| {
                        m.field_str("leaf", &member.leaf.0)?;
                        m.field_str("win", &member.window.0)?;
                        m.field("rect", |s| {
                            write_checked_rect::<S, VALIDATE>(s, &member.rect)
                        })
                    })
                })
            })
        }
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

fn write_pager_v4<S: ReplySink, const VALIDATE: bool>(
    pager: &[PagerScreen],
    sink: &mut S,
) -> Result<()> {
    if VALIDATE {
        check_reply_cap(pager.len(), MAX_REPLY_OUTPUTS)?;
    }
    sink.push_char('[');
    for (i, screen) in pager.iter().enumerate() {
        if i > 0 {
            sink.push_char(',');
        }
        if VALIDATE {
            check_reply_id(&screen.output)?;
            check_reply_cap(screen.workspaces.len(), MAX_REPLY_WORKSPACES)?;
        }
        sink.write_object(|scr| {
            scr.field_str("out", &screen.output)?;
            scr.field_array("workspaces", &screen.workspaces, |workspace, s| {
                if VALIDATE {
                    check_reply_id(&workspace.id)?;
                    check_reply_cap(workspace.windows.len(), MAX_REPLY_WINDOWS)?;
                    for window in &workspace.windows {
                        check_reply_id(&window.window.0)?;
                        if !valid_carried_rect(
                            window.frame.x,
                            window.frame.y,
                            window.frame.w,
                            window.frame.h,
                        ) {
                            return Err(unknown_value());
                        }
                    }
                }
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
                                write_checked_rect::<S, VALIDATE>(s, &window.frame)
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

fn write_reply_presentation_v4<S: ReplySink, const VALIDATE: bool>(
    presentation: &WorldPresentationV4,
    suspended_present: bool,
    sink: &mut S,
) -> Result<()> {
    if VALIDATE {
        if !presentation.foreground.is_empty() && !is_opaque_id(&presentation.foreground) {
            return Err(unknown_value());
        }
        match (&presentation.suspended_output, suspended_present) {
            (None, false) => {}
            (Some(_), true) => {}
            _ => return Err(reply_snapshot("suspended-output-inconsistent")),
        }
        if let Some(output) = presentation.suspended_output.as_ref() {
            check_reply_id(&output.0)?;
        }
    }
    sink.write_object(|obj| {
        obj.field_str("foreground", &presentation.foreground)?;
        obj.field_u64_str("rev", presentation.rev)?;
        if let Some(output) = presentation.suspended_output.as_ref() {
            obj.field_str("suspendedOutput", &output.0)?;
        }
        obj.field("border", |s| {
            write_border_presentation_v4::<S, VALIDATE>(&presentation.border, s)
        })?;
        obj.field("underlay", |s| {
            write_underlay_v4::<S, VALIDATE>(&presentation.underlay, s)
        })?;
        obj.field("pager", |s| {
            write_pager_v4::<S, VALIDATE>(&presentation.pager, s)
        })
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

fn validate_reply_v4_header_and_sets(reply: &WorldReplyV4) -> Result<()> {
    if reply.contract_version != WORLD_PROTOCOL_VERSION_V4 {
        return Err(WorldProtocolError {
            kind: KIND_VERSION,
            message: MSG_VERSION,
        });
    }
    if !is_opaque_id(&reply.adapter_incarnation) {
        return Err(unknown_value());
    }
    if reply.core_generation.len() > MAX_GENERATION_LEN
        || !is_generation_text(&reply.core_generation)
    {
        return Err(unknown_value());
    }
    if reply.request_id.len() > MAX_CORRELATION_LEN || !is_correlation_text(&reply.request_id) {
        return Err(unknown_value());
    }
    check_reply_cap(reply.scopes.len(), MAX_REPLY_SCOPES)?;
    check_reply_cap(reply.workspaces.len(), MAX_REPLY_WORKSPACES)?;
    check_reply_cap(reply.outputs.len(), MAX_REPLY_OUTPUTS)?;
    check_reply_cap(reply.windows.len(), MAX_REPLY_WINDOWS)?;
    check_reply_cap(reply.transfers.len(), MAX_REPLY_TRANSFERS)?;
    check_reply_cap(reply.effects.len(), MAX_REPLY_EFFECTS)?;
    check_reply_cap(reply.operation_settled.len(), MAX_REPLY_OPERATIONS)?;
    check_reply_cap(reply.native_creations.len(), MAX_REPLY_NATIVE_CREATIONS)?;
    check_reply_cap(reply.intent_desired.len(), MAX_INTENT_DESIRED)?;
    check_reply_cap(reply.switch_settled.len(), MAX_REPLY_SWITCH_EVENTS)?;
    check_reply_cap(reply.diagnostics.len(), MAX_REPLY_DIAGNOSTICS)?;
    let mut seen_scopes = BTreeSet::new();
    for scope in &reply.scopes {
        if !seen_scopes.insert(scope_status_key(&scope.scope)) {
            return Err(reply_snapshot("duplicate-scope"));
        }
    }
    let mut seen_workspaces = BTreeSet::new();
    for workspace in &reply.workspaces {
        if !seen_workspaces.insert((workspace.output.0.as_str(), workspace.id.as_str())) {
            return Err(reply_snapshot("duplicate-workspace"));
        }
    }
    let mut seen_outputs = BTreeSet::new();
    for output in &reply.outputs {
        if !seen_outputs.insert(output.output.0.as_str()) {
            return Err(reply_snapshot("duplicate-output"));
        }
    }
    let mut seen_windows = BTreeSet::new();
    for window in &reply.windows {
        if !seen_windows.insert(window.lifetime.0.as_str()) {
            return Err(reply_snapshot("duplicate-window"));
        }
    }
    let mut seen_transfers = BTreeSet::new();
    for transfer in &reply.transfers {
        if !seen_transfers.insert(transfer.operation_id.as_str()) {
            return Err(reply_snapshot("duplicate-transfer"));
        }
    }
    let mut seen_effects = BTreeSet::new();
    for effect in &reply.effects {
        if !seen_effects.insert(effect.id.as_str()) {
            return Err(reply_snapshot("duplicate-effect"));
        }
    }
    let mut seen_operations = BTreeSet::new();
    for event in &reply.operation_settled {
        if !seen_operations.insert((&event.window, event.kind)) {
            return Err(reply_snapshot("duplicate-operation"));
        }
    }
    let mut seen_creations = BTreeSet::new();
    for creation in &reply.native_creations {
        if !seen_creations.insert(creation.operation_id.as_str()) {
            return Err(reply_snapshot("duplicate-native-creation"));
        }
    }
    let mut seen_writes = BTreeSet::new();
    for write in &reply.intent_desired {
        if !seen_writes.insert(write.win.0.as_str()) {
            return Err(reply_snapshot("duplicate-intent-desire"));
        }
    }
    let mut seen_switches = BTreeSet::new();
    for event in &reply.switch_settled {
        if !seen_switches.insert(&event.output) {
            return Err(reply_snapshot("duplicate-switch"));
        }
    }
    if let Some((x, y)) = reply.cursor_warp
        && (!(-GEOMETRY_BOUND..=GEOMETRY_BOUND).contains(&x)
            || !(-GEOMETRY_BOUND..=GEOMETRY_BOUND).contains(&y))
    {
        return Err(unknown_value());
    }
    Ok(())
}

fn write_reply_v4_to_sink<S: ReplySink, const VALIDATE: bool>(
    reply: &WorldReplyV4,
    wire: &ReplyWireOptionals,
    broker_bind: Option<&BrokerReplyBind<'_>>,
    sink: &mut S,
) -> Result<()> {
    if VALIDATE {
        validate_reply_v4_header_and_sets(reply)?;
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
        obj.field_array(
            "scopes",
            &reply.scopes,
            write_scope_status_v4::<S, VALIDATE>,
        )?;
        obj.field_array(
            "workspaces",
            &reply.workspaces,
            write_workspace_desire_v4::<S, VALIDATE>,
        )?;
        obj.field_array(
            "outputs",
            &reply.outputs,
            write_output_desire_v4::<S, VALIDATE>,
        )?;
        obj.field_array("windows", &reply.windows, |w, s| {
            write_window_desire_v4::<S, VALIDATE>(w, s, true)
        })?;
        obj.field_array(
            "transfers",
            &reply.transfers,
            write_transfer_v4::<S, VALIDATE>,
        )?;
        obj.field("effects", |s| {
            s.push_str("[]");
            if VALIDATE && let Some(first) = reply.effects.first() {
                check_reply_id(&first.id)?;
                check_reply_id(&first.target.0)?;
                return Err(reply_snapshot("effect-focus-removed"));
            }
            Ok(())
        })?;
        obj.field("focusRequest", |s| match &reply.focus_request {
            None => {
                s.push_str("null");
                Ok(())
            }
            Some(request) => write_focus_request_v4::<S, VALIDATE>(request, s),
        })?;
        obj.field("dragPreview", |s| {
            write_drag_preview_v4::<S, VALIDATE>(&reply.drag_preview, s)
        })?;
        obj.field("gestureSettled", |s| {
            write_gesture_settled_v4(&reply.gesture_settled, s)
        })?;
        obj.field_array(
            "operationSettled",
            &reply.operation_settled,
            write_operation_event_v4::<S, VALIDATE>,
        )?;
        obj.field_array(
            "nativeCreations",
            &reply.native_creations,
            write_native_creation_v4::<S, VALIDATE>,
        )?;
        obj.field("warpCursor", |s| {
            match reply.cursor_warp {
                Some((x, y)) => {
                    s.push_char('[');
                    s.push_i32(x);
                    s.push_char(',');
                    s.push_i32(y);
                    s.push_char(']');
                }
                None => s.push_str("null"),
            }
            Ok(())
        })?;
        obj.field("presentation", |s| {
            write_reply_presentation_v4::<S, VALIDATE>(
                &reply.presentation,
                wire.suspended_output_present,
                s,
            )
        })?;
        obj.field_array(
            "intentDesired",
            &reply.intent_desired,
            write_intent_desire_v4::<S, VALIDATE>,
        )?;
        obj.field_array(
            "switchSettled",
            &reply.switch_settled,
            write_switch_event_v4::<S, VALIDATE>,
        )?;
        obj.field_array(
            "diagnostics",
            &reply.diagnostics,
            write_row_diagnostic_v4::<S, VALIDATE>,
        )?;
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
    if VALIDATE
        && let Some(request) = reply.focus_request.as_ref()
        && let Some(shown) = request.after_shown.as_ref()
    {
        check_after_shown_invariants(shown)?;
    }
    Ok(())
}

fn write_projection_preview<S: ReplySink>(
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
    if !valid_carried_rect(
        preview.rect.x,
        preview.rect.y,
        preview.rect.w,
        preview.rect.h,
    ) {
        return Err(unknown_value());
    }
    check_reply_id(&preview.source.output.0)?;
    check_reply_id(&preview.source.workspace.0)?;
    check_reply_id(&preview.target.0)?;
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

/// Exact canonical v4 reply byte count over a borrowed [`WorldReplyV4`] and
/// [`ReplyWireOptionals`]. Measures canonical serialized size without running
/// writer-only validation so admission size gates preserve the final-prepare
/// validation checkpoint.
#[must_use]
pub fn count_reply_v4_bytes(reply: &WorldReplyV4, wire: &ReplyWireOptionals) -> usize {
    let mut sink = CountSink(0);
    let _ = write_reply_v4_to_sink::<_, false>(reply, wire, None, &mut sink);
    sink.0
}

#[must_use]
pub(crate) fn count_operation_settled_rows_bytes(settled: &[OperationEvent]) -> usize {
    let mut sink = CountSink(0);
    for (i, event) in settled.iter().enumerate() {
        if i > 0 {
            sink.push_char(',');
        }
        let _ = write_operation_event_v4::<_, false>(event, &mut sink);
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
        let _ = write_switch_event_v4::<_, false>(event, &mut sink);
    }
    sink.push_char(']');
    sink.0
}

/// Exact canonical v4 reply byte count over a borrowed [`WorldReplyV4`] and
/// [`ReplyWireOptionals`]. Runs the identical validation and traversal as
/// [`write_reply_v4_into`], without allocating output bytes or enforcing the
/// 4 MiB transport cap so callers can measure headroom or over-budget sizes.
pub fn count_reply_v4(reply: &WorldReplyV4, wire: &ReplyWireOptionals) -> Result<usize> {
    let mut sink = CountSink(0);
    write_reply_v4_to_sink::<_, true>(reply, wire, None, &mut sink)?;
    Ok(sink.0)
}

/// Checked canonical v4 reply writer appending into `out` from a borrowed
/// [`WorldReplyV4`] and [`ReplyWireOptionals`]. Does not enforce the 4 MiB
/// transport cap so outer envelope callers can append broker fields before
/// checking the final bound.
pub fn write_reply_v4_into(
    reply: &WorldReplyV4,
    wire: &ReplyWireOptionals,
    out: &mut String,
) -> Result<()> {
    let mut sink = StringSink(out);
    write_reply_v4_to_sink::<_, true>(reply, wire, None, &mut sink)
}

/// Checked canonical encode of a borrowed v4 reply and wire optionals,
/// enforcing the 4 MiB transport reply bound.
pub fn write_reply_v4(reply: &WorldReplyV4, wire: &ReplyWireOptionals) -> Result<String> {
    let mut out = String::new();
    write_reply_v4_into(reply, wire, &mut out)?;
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
    wire: &ReplyWireOptionals,
    bind: &BrokerReplyBind<'_>,
) -> Result<String> {
    let mut sink = SortedStringSink::new();
    write_reply_v4_to_sink::<_, true>(reply, wire, Some(bind), &mut sink)?;
    Ok(sink.finish())
}

/// Checked renderer projection writer over a borrowed [`WorldReplyV4`].
/// Reuses the typed row writers while stripping `frame_op` and `clear_op`
/// from `windows` and retaining the exact preview/authority fields in
/// lexicographic object key order with `serde_json` string escaping.
pub fn write_projection_reply_v4(
    reply: &WorldReplyV4,
    wire: &ReplyWireOptionals,
    producer: Option<(&str, &str)>,
) -> Result<String> {
    let mut sink = SortedStringSink::new();
    sink.write_object(|obj| {
        obj.field("v", |s| {
            s.push_u64(u64::from(reply.contract_version));
            Ok(())
        })?;
        obj.field_str("outcome", "presentation")?;
        obj.field_u64_str("rev", reply.state_revision)?;
        obj.field_u64_str("obs", reply.based_on_observation_seq)?;
        obj.field_array("scopes", &reply.scopes, write_scope_status_v4::<_, true>)?;
        obj.field_array(
            "workspaces",
            &reply.workspaces,
            write_workspace_desire_v4::<_, true>,
        )?;
        obj.field_array("outputs", &reply.outputs, write_output_desire_v4::<_, true>)?;
        obj.field_array("windows", &reply.windows, |w, s| {
            write_window_desire_v4::<_, true>(w, s, false)
        })?;
        obj.field("presentation", |s| {
            write_reply_presentation_v4::<_, true>(
                &reply.presentation,
                wire.suspended_output_present,
                s,
            )
        })?;
        obj.field("preview", |s| {
            write_projection_preview(
                reply.drag_preview.as_ref(),
                reply.presentation.suspended_output.as_ref(),
                producer,
                s,
            )
        })
    })?;
    Ok(sink.finish())
}

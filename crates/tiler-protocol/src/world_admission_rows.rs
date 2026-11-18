//! Protocol-owned scalar width and row-level sizing helpers for
//! [`ProtocolAdmissionGate`](crate::world_admission::ProtocolAdmissionGate).

use tiler_core::boundary::CoreCommand;
use tiler_core::connection::RawSettingValue;
use tiler_core::geometry::Rect;
use tiler_core::gesture::GestureIngress;
use tiler_core::session::{DomainKey, DragHoverPrior};
use tiler_core::world_report::{
    ArrivalBracket, ArrivalLocation, FocusFeedback, FrameWitness, IntentRecord, LastWrite,
    PresentationIngressV4, PresentationRowV4, RenderTarget, RendererInput, SendSource,
    VisibilitySample, WorkspaceRef, WorldCommand,
};

pub(crate) fn json_string_len(value: &str) -> usize {
    let mut bytes = 2usize;
    for byte in value.bytes() {
        bytes += match byte {
            b'"' | b'\\' => 2,
            0x00..=0x1F => 6,
            _ => 1,
        };
    }
    bytes
}

pub(crate) const fn bool_len(value: bool) -> usize {
    if value { 4 } else { 5 }
}

pub(crate) const fn int_len(value: i32) -> usize {
    if value == i32::MIN {
        return 11;
    }
    let mut digits = 1usize;
    let mut rest = value.unsigned_abs() / 10;
    while rest > 0 {
        digits += 1;
        rest /= 10;
    }
    digits + if value < 0 { 1 } else { 0 }
}

pub(crate) const fn u64_len(mut value: u64) -> usize {
    let mut digits = 1usize;
    while value >= 10 {
        digits += 1;
        value /= 10;
    }
    digits
}

pub(crate) const fn usize_len(mut value: usize) -> usize {
    let mut digits = 1usize;
    while value >= 10 {
        digits += 1;
        value /= 10;
    }
    digits
}

pub(crate) const fn rect_len(rect: Option<Rect>) -> usize {
    match rect {
        Some(rect) => 5 + int_len(rect.x) + int_len(rect.y) + int_len(rect.w) + int_len(rect.h),
        None => 4,
    }
}

fn workspace_ref_len(reference: &WorkspaceRef) -> usize {
    match reference {
        WorkspaceRef::Logical { output, workspace } => {
            "{\"kind\":\"logical\",\"out\":,\"ws\":}".len()
                + json_string_len(&output.0)
                + json_string_len(&workspace.0)
        }
        WorkspaceRef::Native { output, workspace } => {
            "{\"kind\":\"native\",\"out\":,\"ws\":}".len()
                + json_string_len(&output.0)
                + json_string_len(workspace.as_str())
        }
    }
}

pub(crate) fn workspace_ref_option_len(reference: &Option<WorkspaceRef>) -> usize {
    reference.as_ref().map_or(4, workspace_ref_len)
}

fn visibility_sample_value_estimate(sample: &VisibilitySample) -> usize {
    102 + u64_len(sample.obs)
        + sample.lifetime_matched.map_or(4, bool_len)
        + sample.visibility.visible.map_or(4, bool_len)
        + sample.visibility.suppression_owned.map_or(4, bool_len)
        + sample.minimized.map_or(4, bool_len)
        + sample.host_hidden.map_or(4, bool_len)
}

fn arrival_bracket_estimate(bracket: &ArrivalBracket) -> usize {
    let fixed = match &bracket.location {
        ArrivalLocation::Native { .. } => 69,
        ArrivalLocation::Output { .. } => 68,
    };
    fixed
        + json_string_len(&bracket.window.0)
        + u64_len(bracket.obs)
        + visibility_sample_value_estimate(&bracket.before)
        + match &bracket.location {
            ArrivalLocation::Native { home } => {
                home.as_ref().map_or(4, |reference| match reference {
                    WorkspaceRef::Logical { output, workspace } => {
                        "{\"kind\":\"logical\",\"out\":,\"ws\":}".len()
                            + json_string_len(&output.0)
                            + json_string_len(&workspace.0)
                    }
                    WorkspaceRef::Native { output, workspace } => {
                        "{\"kind\":\"native\",\"out\":,\"ws\":}".len()
                            + json_string_len(&output.0)
                            + json_string_len(workspace.as_str())
                    }
                })
            }
            ArrivalLocation::Output { out } => out.as_ref().map_or(4, |id| json_string_len(&id.0)),
        }
        + rect_len(bracket.frame)
        + visibility_sample_value_estimate(&bracket.after)
}

pub(crate) fn focus_feedback_estimate(row: &FocusFeedback) -> usize {
    109 + json_string_len(&row.id)
        + json_string_len(&row.target.0)
        + u64_len(row.revision)
        + json_string_len(row.setter.as_str())
        + json_string_len(row.reason.as_str())
        + row.pre.as_ref().map_or(4, |id| json_string_len(&id.0))
        + row.post.as_ref().map_or(4, |id| json_string_len(&id.0))
        + bool_len(row.foreground_proven)
        + row
            .arrival
            .iter()
            .map(arrival_bracket_estimate)
            .sum::<usize>()
        + row.arrival.len().saturating_sub(1)
        + u64_len(row.attempt_ms)
}

fn visibility_sample_estimate(sample: &Option<VisibilitySample>) -> usize {
    match sample {
        None => 4,
        Some(sample) => {
            102 + u64_len(sample.obs)
                + sample.lifetime_matched.map_or(4, bool_len)
                + 32
                + sample.visibility.visible.map_or(4, bool_len)
                + sample.visibility.suppression_owned.map_or(4, bool_len)
                + sample.minimized.map_or(4, bool_len)
                + sample.host_hidden.map_or(4, bool_len)
        }
    }
}

pub(crate) fn last_write_estimate(last: &Option<LastWrite>) -> usize {
    match last {
        None => 4,
        Some(last) => {
            80 + json_string_len(last.property.as_str())
                + bool_len(last.target)
                + u64_len(last.revision)
                + json_string_len(last.path.as_str())
                + json_string_len(last.setter.as_str())
                + json_string_len(last.reason.as_str())
                + visibility_sample_estimate(&last.pre)
                + visibility_sample_estimate(&last.post)
        }
    }
}

fn send_source_estimate(source: &SendSource) -> usize {
    match source {
        SendSource::Explicit(output) => 13 + json_string_len(&output.0),
        SendSource::HomeThenAction => 31,
    }
}

fn hover_estimate(hover: &Option<DragHoverPrior>) -> usize {
    let Some(hover) = hover else {
        return 4;
    };
    let prior = hover.prior.as_ref().map_or(4, |edge| {
        18 + json_string_len(&edge.group.0) + json_string_len(edge.edge.as_str())
    });
    85 + u64_len(hover.revision)
        + json_string_len(&hover.domain.output.0)
        + json_string_len(&hover.domain.workspace.0)
        + json_string_len(&hover.source_leaf.0)
        + json_string_len(&hover.source_window.0)
        + prior
        + json_string_len(&hover.prior_domain.output.0)
        + json_string_len(&hover.prior_domain.workspace.0)
}

fn drag_source_estimate(source: &Option<DomainKey>) -> usize {
    source.as_ref().map_or(4, |key| {
        14 + json_string_len(&key.output.0) + json_string_len(&key.workspace.0)
    })
}

fn subject_estimate(subject: &tiler_core::directional::WindowSubject) -> usize {
    match subject {
        tiler_core::directional::WindowSubject::Window(w) => json_string_len(&w.0),
        tiler_core::directional::WindowSubject::CurrentFocus => 21,
    }
}

fn focus_directional_subject_estimate(
    subject: &tiler_core::world_report::FocusDirectionalSubject,
) -> usize {
    match subject {
        tiler_core::world_report::FocusDirectionalSubject::Window(w) => json_string_len(&w.0),
        tiler_core::world_report::FocusDirectionalSubject::NoFocus => json_string_len(""),
        tiler_core::world_report::FocusDirectionalSubject::CurrentFocus => 21,
    }
}

fn operation_estimate(command: &CoreCommand) -> usize {
    match command {
        CoreCommand::Reconcile => 7 + json_string_len("reconcile"),
        CoreCommand::UpdateGaps => 7 + json_string_len("update-gaps"),
        CoreCommand::ReleaseDomain => 7 + json_string_len("release-domain"),
        CoreCommand::Move {
            window,
            direction,
            cross_output_transfer,
            same_axis_move,
        } => {
            61 + bool_len(*cross_output_transfer)
                + json_string_len(&window.0)
                + json_string_len(direction.as_wire_str())
                + json_string_len(same_axis_move.as_wire_str())
        }
        CoreCommand::Focus {
            window,
            direction,
            cross_output_transfer,
            float_subject,
        } => {
            65 + bool_len(*cross_output_transfer)
                + bool_len(*float_subject)
                + json_string_len(&window.0)
                + json_string_len(direction.as_wire_str())
        }
        CoreCommand::Resize {
            window,
            direction,
            mode,
            press_index,
        } => {
            52 + subject_estimate(window)
                + json_string_len(direction.as_wire_str())
                + json_string_len(mode.as_wire_str())
                + u64_len(u64::from(*press_index))
        }
        CoreCommand::PointerResize {
            window,
            direction,
            boundary,
            second,
        } => {
            76 + json_string_len(&window.0)
                + json_string_len(direction.as_wire_str())
                + int_len(*boundary)
                + second.map_or(8, |(d, b)| json_string_len(d.as_wire_str()) + int_len(b))
        }
        CoreCommand::ToggleFloat { window, float_rect } => {
            36 + subject_estimate(window)
                + float_rect.map_or(4, |r| {
                    int_len(r.x) + int_len(r.y) + int_len(r.w) + int_len(r.h) + 5
                })
        }
        CoreCommand::ToggleOrientation { window } => 34 + subject_estimate(window),
        CoreCommand::SendToWorkspace {
            window,
            target_output,
            target_workspace,
            follow,
        } => {
            56 + bool_len(*follow)
                + json_string_len(&window.0)
                + json_string_len(&target_output.0)
                + json_string_len(&target_workspace.0)
        }
        CoreCommand::SendToOutput {
            window,
            target_output,
            target_workspace,
            follow,
        } => {
            53 + bool_len(*follow)
                + json_string_len(&window.0)
                + json_string_len(&target_output.0)
                + json_string_len(&target_workspace.0)
        }
        CoreCommand::MigrateWorkspace { direction } => {
            38 + json_string_len(direction.as_wire_str())
        }
        CoreCommand::DragDrop {
            window,
            x,
            y,
            hover_prior,
            source,
        }
        | CoreCommand::DragPreview {
            window,
            x,
            y,
            hover_prior,
            source,
        } => {
            let op_len = match command {
                CoreCommand::DragDrop { .. } => json_string_len("drag-drop"),
                _ => json_string_len("drag-preview"),
            };
            43 + op_len
                + json_string_len(&window.0)
                + int_len(*x)
                + int_len(*y)
                + hover_estimate(hover_prior)
                + drag_source_estimate(source)
        }
    }
}

pub(crate) fn command_estimate_owned(command: &WorldCommand) -> usize {
    match command {
        WorldCommand::Focus { window, direction } => {
            34 + subject_estimate(window)
                + json_string_len(
                    direction.map_or("", tiler_core::directional::Direction::as_wire_str),
                )
        }
        WorldCommand::Send {
            window,
            target_output,
            target_workspace,
            follow,
        } => {
            43 + bool_len(*follow)
                + subject_estimate(window)
                + json_string_len(&target_output.0)
                + json_string_len(&target_workspace.0)
        }
        WorldCommand::SwitchView { output, workspace } => {
            33 + json_string_len(&output.0) + json_string_len(&workspace.0)
        }
        WorldCommand::SelectWorkspace { output, workspace } => {
            38 + json_string_len(&output.0) + json_string_len(&workspace.0)
        }
        WorldCommand::SelectNumbered { output, index } => {
            40 + output.as_ref().map_or(4, |out| json_string_len(&out.0))
                + usize_len(usize::from(*index))
        }
        WorldCommand::SelectRelative { output, delta } => {
            40 + output.as_ref().map_or(4, |out| json_string_len(&out.0)) + int_len(*delta)
        }
        WorldCommand::SelectPrevious { output } | WorldCommand::SelectTrailing { output } => {
            31 + output.as_ref().map_or(4, |out| json_string_len(&out.0))
        }
        WorldCommand::SendRelative {
            window,
            source,
            delta,
            follow,
        } => {
            58 + bool_len(*follow)
                + subject_estimate(window)
                + send_source_estimate(source)
                + int_len(*delta)
        }
        WorldCommand::SendNumbered {
            window,
            source,
            index,
            follow,
        } => {
            59 + bool_len(*follow)
                + subject_estimate(window)
                + send_source_estimate(source)
                + usize_len(usize::from(*index))
        }
        WorldCommand::SetWorkspaceTiling {
            output,
            workspace,
            tiled,
        } => 52 + bool_len(*tiled) + json_string_len(&output.0) + json_string_len(&workspace.0),
        WorldCommand::ToggleWorkspaceTiling { output, workspace } => {
            44 + json_string_len(&output.0) + json_string_len(&workspace.0)
        }
        WorldCommand::MigrateWorkspace {
            source_output,
            target_output,
            workspace,
        } => {
            45 + json_string_len(&source_output.0)
                + json_string_len(&target_output.0)
                + json_string_len(&workspace.0)
        }
        WorldCommand::FocusDirectional { window, direction } => {
            44 + focus_directional_subject_estimate(window)
                + json_string_len(direction.as_wire_str())
        }
        WorldCommand::MoveDirectional {
            window,
            direction,
            same_axis,
        } => {
            57 + subject_estimate(window)
                + json_string_len(direction.as_wire_str())
                + json_string_len(same_axis.as_wire_str())
        }
        WorldCommand::SendDirectional {
            window,
            direction,
            follow,
        } => {
            55 + subject_estimate(window)
                + json_string_len(direction.as_wire_str())
                + bool_len(*follow)
        }
        WorldCommand::MigrateDirectional { direction } => {
            41 + json_string_len(direction.as_wire_str())
        }
        WorldCommand::ToggleMaximize { window } => 31 + subject_estimate(window),
        WorldCommand::SetSticky { window, value } => {
            35 + subject_estimate(window) + bool_len(*value)
        }
        WorldCommand::ToggleSticky { window } => 29 + subject_estimate(window),
        WorldCommand::SetFullscreen { window, value } => {
            39 + subject_estimate(window) + bool_len(*value)
        }
        WorldCommand::ToggleFullscreen { window } => 33 + subject_estimate(window),
        WorldCommand::Operation(command) => 30 + operation_estimate(command),
    }
}

fn raw_setting_value_estimate(value: &RawSettingValue) -> usize {
    match value {
        RawSettingValue::Null => 4,
        RawSettingValue::Bool(set) => bool_len(*set),
        RawSettingValue::Number(number) => {
            crate::canonical_json::json_number_len(*number).unwrap_or(4)
        }
        RawSettingValue::String(text) => json_string_len(text),
        RawSettingValue::Array(items) => {
            2 + items.iter().map(raw_setting_value_estimate).sum::<usize>()
                + items.len().saturating_sub(1)
        }
        RawSettingValue::Object(entries) => {
            2 + entries
                .iter()
                .map(|(key, item)| json_string_len(key) + 1 + raw_setting_value_estimate(item))
                .sum::<usize>()
                + entries.len().saturating_sub(1)
        }
    }
}

pub(crate) fn held_fields_estimate(fields: &[(String, RawSettingValue)]) -> usize {
    2 + fields
        .iter()
        .map(|(key, value)| json_string_len(key) + 1 + raw_setting_value_estimate(value))
        .sum::<usize>()
        + fields.len().saturating_sub(1)
}

fn witness_estimate(witness: &Option<FrameWitness>) -> usize {
    match witness {
        None => 4,
        Some(witness) => {
            40 + json_string_len(&witness.win.0)
                + u64_len(witness.obs)
                + rect_len(Some(witness.frame))
                + json_string_len(&witness.foreground.0)
        }
    }
}

fn presentation_ingress_v4_estimate(ingress: &PresentationIngressV4) -> usize {
    let mut bytes = 181
        + json_string_len(&ingress.foreground)
        + ingress.foreground_minimized.map_or(4, bool_len)
        + ingress.foreground_maximized.map_or(4, bool_len)
        + ingress.foreground_fullscreen.map_or(4, bool_len)
        + ingress.foreground_host_hidden.map_or(4, bool_len)
        + ingress.foreground_hidden_workspace.map_or(4, bool_len)
        + rect_len(ingress.foreground_frame)
        + bool_len(ingress.chord_held)
        + json_string_len(&ingress.move_window);
    for window in &ingress.pager_order {
        bytes += json_string_len(&window.0) + 1;
    }
    for row in &ingress.pager_frames {
        bytes += 16 + json_string_len(&row.window.0) + rect_len(row.live);
    }
    bytes += ingress.pager_frames.len().saturating_sub(1);
    bytes += witness_estimate(&ingress.witness);
    bytes += ingress
        .foreground_out
        .as_ref()
        .map_or(4, |out| json_string_len(&out.0));
    bytes
}

pub(crate) fn presentation_row_v4_estimate(
    row: &PresentationRowV4,
    residual: Option<&crate::world_report_residual::WorldReportResidual>,
) -> usize {
    match row {
        PresentationRowV4::Valid(ingress) => presentation_ingress_v4_estimate(ingress),
        PresentationRowV4::Held(_) => residual.map_or(0, |r| r.estimate_bytes()),
    }
}

pub(crate) fn renderer_input_estimate(input: &Option<RendererInput>) -> usize {
    match input {
        None => 4,
        Some(input) => {
            20 + u64_len(input.rev)
                + match &input.target {
                    RenderTarget::Workspace { out, ws } => {
                        33 + json_string_len(&out.0) + json_string_len(&ws.0)
                    }
                    RenderTarget::Window { out, ws, win } => {
                        40 + json_string_len(&out.0)
                            + json_string_len(&ws.0)
                            + json_string_len(&win.0)
                    }
                    RenderTarget::Group { out, ws, group } => {
                        42 + json_string_len(&out.0)
                            + json_string_len(&ws.0)
                            + json_string_len(&group.0)
                    }
                }
        }
    }
}

pub(crate) fn gesture_ingress_v4_estimate(gesture: &Option<GestureIngress>) -> usize {
    match gesture {
        None => 22,
        Some(gesture) => {
            18 + 83
                + json_string_len(&gesture.window.0)
                + json_string_len(gesture.kind.as_str())
                + json_string_len(gesture.phase.as_str())
                + u64_len(gesture.id)
                + gesture
                    .cursor
                    .map_or(4, |(x, y)| 3 + int_len(x) + int_len(y))
                + gesture
                    .start_cursor
                    .map_or(4, |(x, y)| 3 + int_len(x) + int_len(y))
                + bool_len(gesture.esc)
                + bool_len(gesture.modal_move)
                + rect_len(Some(gesture.start_frame))
        }
    }
}

pub(crate) fn intent_records_estimate(records: &[IntentRecord]) -> usize {
    let mut bytes = 19usize;
    for (i, record) in records.iter().enumerate() {
        if i > 0 {
            bytes += 1;
        }
        bytes += 16 + json_string_len(&record.win.0) + json_string_len(&record.data);
    }
    bytes
}

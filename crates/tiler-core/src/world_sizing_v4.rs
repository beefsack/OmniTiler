//! Staged v4 report size estimator: conservative upper bound of the
//! canonical v4 report wire bytes
//! (`encode_report_v4` in the protocol crate).
//! Covers the v4 envelope (`rendererInput` carried, `views` removed,
//! presentation held narrowly) with the shared row estimators. The
//! estimator sees only the typed report, never the wire optionals, so
//! every wire-optional lane counts its present form even when the wire
//! omits it: an overcount, never an undercount.

use crate::world_report::{
    AttemptLaneV4, CommandResult, PresentationIngressV4, PresentationRowV4, RendererInput,
    TransferKind, WorldCommand, WorldPresentationV4, WorldReplyV4, WorldReportV4,
};
use crate::world_sizing::{
    MAX_CANONICAL_F64_LEN, MAX_EFFECT_RESERVE_BYTES, MAX_STATE_ATTEMPT_DIAGNOSTIC_BYTES,
    MAX_TARGET_LANES_PER_WINDOW, anchored_border_estimate, authority_scope_len, bool_len,
    command_estimate_owned, command_result_estimate, focus_feedback_estimate,
    gesture_settled_estimate, int_len, intent_records_estimate, intent_writes_estimate,
    json_string_len, last_write_estimate, native_creations_estimate, operation_settled_estimate,
    raw_setting_value_estimate, rect_len, scope_status_estimate, switch_settled_estimate, u64_len,
    usize_len, warp_cursor_estimate, window_target_lanes_estimate, workspace_ref_option_len,
};

fn held_fields_estimate(fields: &[(String, crate::connection::RawSettingValue)]) -> usize {
    2 + fields
        .iter()
        .map(|(key, value)| json_string_len(key) + 1 + raw_setting_value_estimate(value))
        .sum::<usize>()
        + fields.len().saturating_sub(1)
}

fn witness_estimate(witness: &Option<crate::world_report::FrameWitness>) -> usize {
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

fn presentation_row_v4_estimate(row: &PresentationRowV4) -> usize {
    match row {
        PresentationRowV4::Valid(ingress) => presentation_ingress_v4_estimate(ingress),
        PresentationRowV4::Held(held) => held_fields_estimate(&held.fields),
    }
}

fn renderer_input_estimate(input: &Option<RendererInput>) -> usize {
    match input {
        None => 4,
        Some(input) => {
            20 + u64_len(input.rev)
                + match &input.target {
                    crate::world_report::RenderTarget::Workspace { out, ws } => {
                        33 + json_string_len(&out.0) + json_string_len(&ws.0)
                    }
                    crate::world_report::RenderTarget::Window { out, ws, win } => {
                        40 + json_string_len(&out.0)
                            + json_string_len(&ws.0)
                            + json_string_len(&win.0)
                    }
                    crate::world_report::RenderTarget::Group { out, ws, group } => {
                        42 + json_string_len(&out.0)
                            + json_string_len(&ws.0)
                            + json_string_len(&group.0)
                    }
                }
        }
    }
}

fn gesture_ingress_v4_estimate(gesture: &Option<crate::gesture::GestureIngress>) -> usize {
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

/// Conservative upper bound of the canonical v4 report wire bytes for
/// `report`. Wire-optional lanes (cursor hit, monotonic clock, attempt
/// lanes, gesture ingress) always count their carried form; creation
/// receipts count the carried `created` spelling at its widest.
#[must_use]
pub fn report_estimate_v4(report: &WorldReportV4) -> usize {
    let mut bytes = 311
        + usize_len(report.contract_version as usize)
        + json_string_len(&report.adapter_incarnation)
        + json_string_len(&report.core_generation)
        + json_string_len(&report.request_id)
        + report
            .cursor
            .map_or(4, |(x, y)| 3 + int_len(x) + int_len(y))
        + report
            .cursor_hit
            .as_ref()
            .map_or(4, |hit| json_string_len(&hit.0))
        + report
            .action_foreground
            .as_ref()
            .map_or(4, |out| json_string_len(&out.0))
        + report
            .action_cached
            .as_ref()
            .map_or(4, |out| json_string_len(&out.0))
        + report
            .action_outputs
            .iter()
            .map(|out| json_string_len(&out.0) + 1)
            .sum::<usize>()
        + u64_len(report.request_seq)
        + u64_len(report.observation_seq)
        + u64_len(report.monotonic_ms)
        + report.command_seq.map_or(4, |seq| 2 + u64_len(seq))
        + report.command.as_ref().map_or(4, command_estimate_owned)
        + json_string_len(&report.focused_global)
        + 19
        + bool_len(report.foreground_known)
        + bool_len(report.inventory_complete)
        + bool_len(report.gesture_barrier)
        + presentation_row_v4_estimate(&report.presentation);
    for screen in &report.screens {
        bytes += 106
            + rect_len(screen.full_rect)
            + rect_len(screen.work_area)
            + bool_len(screen.complete)
            + MAX_CANONICAL_F64_LEN
            + json_string_len(&screen.output.0)
            + screen
                .identity
                .manufacturer
                .as_deref()
                .map_or(4, json_string_len)
            + screen.identity.model.as_deref().map_or(4, json_string_len)
            + screen.identity.serial.as_deref().map_or(4, json_string_len)
            + screen
                .identity
                .connector
                .as_deref()
                .map_or(4, json_string_len);
    }
    bytes += 13
        + report
            .cursor_hit
            .as_ref()
            .map_or(4, |hit| json_string_len(&hit.0));
    bytes += 17 + u64_len(report.monotonic_ms);
    for held in &report.held_screens {
        bytes += held_fields_estimate(&held.fields);
    }
    for fact in &report.windows {
        bytes += 523
            + json_string_len(&fact.window.0)
            + fact
                .output
                .as_ref()
                .map_or(4, |out| json_string_len(&out.0))
            + rect_len(fact.rect)
            + fact.fullscreen.map_or(4, bool_len)
            + fact.maximized.map_or(4, bool_len)
            + fact.sticky.map_or(4, bool_len)
            + fact.minimized.map_or(4, bool_len)
            + fact.hints.map_or(4, |hints| {
                hints.min_w.map_or(4, int_len)
                    + hints.min_h.map_or(4, int_len)
                    + hints.max_w.map_or(4, int_len)
                    + hints.max_h.map_or(4, int_len)
            })
            + match &fact.native_workspace {
                crate::world_report::NativeWorkspaceFact::Omitted => 0,
                crate::world_report::NativeWorkspaceFact::Null => 4,
                crate::world_report::NativeWorkspaceFact::Value(id) => json_string_len(id),
            }
            + fact.host_hidden.map_or(4, bool_len)
            + fact.privileged.map_or(4, bool_len)
            + fact.managed.map_or(4, bool_len)
            + fact.accepts_input.map_or(4, bool_len)
            + fact.activation_inhibited.map_or(4, bool_len)
            + fact.tool_window.map_or(4, bool_len)
            + fact.owned.map_or(4, bool_len)
            + fact.transient.map_or(4, bool_len)
            + fact.modal.map_or(4, bool_len)
            + fact.resizable.map_or(4, bool_len)
            + fact.above.map_or(4, bool_len)
            + fact.below.map_or(4, bool_len)
            + fact.owner.as_ref().map_or(4, |id| json_string_len(&id.0))
            + fact.parent.as_ref().map_or(4, |id| json_string_len(&id.0))
            + fact.role.as_deref().map_or(4, json_string_len)
            + fact.app_id.as_deref().map_or(4, json_string_len)
            + fact.executable.as_deref().map_or(4, json_string_len)
            + fact.process_id.as_deref().map_or(4, json_string_len)
            + fact.class.as_deref().map_or(4, json_string_len)
            + fact.title.as_deref().map_or(4, json_string_len)
            + fact
                .window_type
                .map_or(4, |kind| json_string_len(kind.as_str()))
            + fact
                .activation_policy
                .map_or(4, |policy| json_string_len(policy.as_str()))
            + fact.layer.map_or(4, int_len)
            + fact
                .fullscreen_owner
                .map_or(4, |owner| json_string_len(owner.as_str()))
            + fact.fullscreen_exitable.map_or(4, bool_len)
            + fact.suppression_available.map_or(4, bool_len)
            + 32
            + fact.visibility.visible.map_or(4, bool_len)
            + fact.visibility.suppression_owned.map_or(4, bool_len)
            + last_write_estimate(&fact.last_write);
    }
    for held in &report.held_windows {
        bytes += held_fields_estimate(&held.fields);
    }
    for id in &report.closed {
        bytes += json_string_len(&id.0) + 1;
    }
    for native in &report.native {
        bytes += 58
            + json_string_len(&native.output.0)
            + native.current_native.as_deref().map_or(4, json_string_len);
        for id in &native.native_order {
            bytes += json_string_len(id) + 1;
        }
        for row in &native.occupancy {
            bytes += 22
                + json_string_len(&row.native)
                + row.count.map_or(4, |count| u64_len(u64::from(count)));
        }
    }
    for held in &report.held_native {
        bytes += held_fields_estimate(&held.fields);
    }
    bytes += 20;
    for attempt in &report.frames {
        bytes += 98
            + json_string_len(&attempt.window.0)
            + int_len(attempt.target.x)
            + int_len(attempt.target.y)
            + int_len(attempt.target.w)
            + int_len(attempt.target.h)
            + json_string_len(attempt.setter.as_str())
            + attempt.readback.map_or(4, |rect| {
                5 + int_len(rect.x) + int_len(rect.y) + int_len(rect.w) + int_len(rect.h)
            })
            + attempt.dpi_before.map_or(4, |dpi| u64_len(u64::from(dpi)))
            + attempt.dpi_after.map_or(4, |dpi| u64_len(u64::from(dpi)))
            + u64_len(attempt.attempt_ms)
            + json_string_len(attempt.operation_id.as_deref().unwrap_or_default())
            + attempt.prewrite.map_or(4, |rect| {
                5 + int_len(rect.x) + int_len(rect.y) + int_len(rect.w) + int_len(rect.h)
            });
    }
    for held in report
        .held_attempts
        .iter()
        .filter(|held| held.lane == AttemptLaneV4::Frame)
    {
        bytes += held_fields_estimate(&held.fields);
    }
    bytes += 21;
    for attempt in &report.member_attempts {
        bytes += "{\"op\":\"\",\"win\":\"\",\"out\":\"\",\"ws\":\"\",\"setter\":\"\",\"pre\":null,\"post\":null,\"attemptMs\":\"\"}".len() - 12
            + json_string_len(&attempt.operation_id)
            + json_string_len(&attempt.window.0)
            + json_string_len(&attempt.target_output.0)
            + json_string_len(&attempt.target_workspace)
            + json_string_len(attempt.setter.as_str())
            + workspace_ref_option_len(&attempt.pre)
            + workspace_ref_option_len(&attempt.post)
            + u64_len(attempt.attempt_ms);
    }
    for held in report
        .held_attempts
        .iter()
        .filter(|held| held.lane == AttemptLaneV4::Member)
    {
        bytes += held_fields_estimate(&held.fields);
    }
    bytes += 19;
    for attempt in &report.view_attempts {
        bytes += "{\"op\":\"\",\"out\":\"\",\"ws\":\"\",\"setter\":\"\",\"pre\":null,\"post\":null,\"base\":null,\"attemptMs\":\"\"}".len() - 10
            + json_string_len(&attempt.operation_id)
            + json_string_len(&attempt.target_output.0)
            + json_string_len(&attempt.target_workspace)
            + json_string_len(attempt.setter.as_str())
            + workspace_ref_option_len(&attempt.pre)
            + workspace_ref_option_len(&attempt.post)
            + workspace_ref_option_len(&attempt.baseline)
            + u64_len(attempt.attempt_ms);
    }
    for held in report
        .held_attempts
        .iter()
        .filter(|held| held.lane == AttemptLaneV4::View)
    {
        bytes += held_fields_estimate(&held.fields);
    }
    bytes += 20;
    for attempt in &report.clear_attempts {
        bytes += 62
            + json_string_len(&attempt.operation_id)
            + json_string_len(&attempt.window.0)
            + json_string_len(attempt.setter.as_str())
            + 5
            + 5
            + u64_len(attempt.attempt_ms);
    }
    for held in report
        .held_attempts
        .iter()
        .filter(|held| held.lane == AttemptLaneV4::Clear)
    {
        bytes += held_fields_estimate(&held.fields);
    }
    bytes += 22;
    for attempt in &report.creation_attempts {
        bytes += 85
            + json_string_len(&attempt.operation_id)
            + json_string_len(&attempt.logical_workspace)
            + json_string_len(&attempt.target_output.0)
            + json_string_len(attempt.setter.as_str())
            + attempt.created_native.as_deref().map_or(4, json_string_len)
            + u64_len(attempt.attempt_ms);
        for id in &attempt.order_before {
            bytes += json_string_len(id) + 1;
        }
        for id in &attempt.order_after {
            bytes += json_string_len(id) + 1;
        }
    }
    for held in report
        .held_attempts
        .iter()
        .filter(|held| held.lane == AttemptLaneV4::Creation)
    {
        bytes += held_fields_estimate(&held.fields);
    }
    bytes += gesture_ingress_v4_estimate(&report.gesture);
    if let Some(primary) = &report.primary_output {
        bytes += 11 + json_string_len(&primary.0);
    }
    bytes += intent_records_estimate(&report.intent_records);
    for attempt in &report.state_attempts {
        bytes += 79
            + json_string_len(&attempt.window.0)
            + json_string_len(attempt.property.as_str())
            + bool_len(attempt.target)
            + u64_len(attempt.revision)
            + json_string_len(attempt.setter.as_str())
            + attempt.pre.map_or(4, bool_len)
            + attempt.post.map_or(4, bool_len)
            + u64_len(attempt.attempt_ms);
    }
    for (i, row) in report.focus_feedback.iter().enumerate() {
        if i > 0 {
            bytes += 1;
        }
        bytes += focus_feedback_estimate(row);
    }
    bytes += 17 + renderer_input_estimate(&report.renderer_input);
    bytes
}

/// Conservative size of the retained close-focus lane body (after the
/// `,"closeFocus":` joiner): `null` (4) with no request, else the fixed
/// object shape with quoted revision and closed cause token.
fn close_focus_estimate_v4(request: &Option<crate::world_report::CloseFocusRequest>) -> usize {
    match request {
        None => 4,
        Some(request) => {
            63 + json_string_len(&request.id)
                + json_string_len(&request.target.0)
                + u64_len(request.issued_revision)
                + json_string_len(request.cause.as_str())
                + json_string_len(&request.output.0)
                + json_string_len(&request.workspace)
        }
    }
}

/// Structural reply size estimate over the staged v4 reply with the v4
/// deltas (transfers carry the quoted `cmdSeq` correlation, presentation
/// carries the anchored border). Shared lanes reuse the shared estimators;
/// the estimate stays conservative (verified by the protocol twin test).
#[must_use]
pub fn reply_estimate_v4(reply: &WorldReplyV4) -> usize {
    // Same envelope punctuation as the legacy twin (165 plus the two bools
    // = 175); the v3/v4 envelopes share every joiner literal.
    let mut bytes = 175
        + usize_len(reply.contract_version as usize)
        + json_string_len(&reply.request_id)
        + json_string_len(&reply.adapter_incarnation)
        + json_string_len(&reply.core_generation)
        + u64_len(reply.based_on_observation_seq)
        + u64_len(reply.state_revision);
    for scope in &reply.scopes {
        bytes += scope_status_estimate(scope);
    }
    bytes += reply.scopes.len().saturating_sub(1);
    for workspace in &reply.workspaces {
        bytes += 65
            + json_string_len(&workspace.id)
            + json_string_len(&workspace.output.0)
            + json_string_len(workspace.native_binding.as_deref().unwrap_or_default())
            + authority_scope_len(&workspace.scope)
            + usize_len(workspace.order)
            + json_string_len(workspace.tiling_mode.as_str())
            + bool_len(workspace.existence)
            + json_string_len(
                workspace
                    .hold
                    .map_or("", crate::world_report::HoldReason::as_str),
            );
        if workspace.trailing {
            bytes += 16;
        }
    }
    bytes += reply.workspaces.len().saturating_sub(1);
    for output in &reply.outputs {
        bytes += 32
            + json_string_len(&output.output.0)
            + json_string_len(output.logical_view.as_deref().unwrap_or_default())
            + json_string_len(output.tiling_mode.as_str())
            + json_string_len(
                output
                    .hold
                    .map_or("", crate::world_report::HoldReason::as_str),
            );
    }
    bytes += reply.outputs.len().saturating_sub(1);
    for diagnostic in &reply.diagnostics {
        bytes += 30
            + authority_scope_len(&diagnostic.scope)
            + json_string_len(diagnostic.row.as_str())
            + diagnostic.id.as_deref().map_or(4, json_string_len)
            + json_string_len(diagnostic.kind.as_str());
    }
    bytes += reply.diagnostics.len().saturating_sub(1);
    for window in &reply.windows {
        bytes += 151
            + json_string_len(&window.lifetime.0)
            + json_string_len(&window.output.0)
            + json_string_len(&window.workspace)
            + rect_len(window.allocation)
            + rect_len(window.effective_frame)
            + json_string_len(window.frame_authority.as_str())
            + json_string_len(
                window
                    .frame_authority
                    .hold_reason()
                    .map_or("", crate::world_report::HoldReason::as_str),
            )
            + json_string_len(window.membership_authority.as_str())
            + json_string_len(
                window
                    .membership_authority
                    .hold_reason()
                    .map_or("", crate::world_report::HoldReason::as_str),
            )
            + json_string_len(window.frame_operation.as_deref().unwrap_or_default())
            + bool_len(window.floating)
            + json_string_len(
                window
                    .float_origin
                    .map_or("", crate::world_report::FloatOrigin::as_str),
            )
            + json_string_len(window.clear_operation.as_deref().unwrap_or_default())
            + window_target_lanes_estimate(window);
    }
    bytes += reply.windows.len().saturating_sub(1);
    for transfer in &reply.transfers {
        // Legacy 63 punctuation (bools folded at their widest) plus the
        // `,"cmdSeq":` key (10) and the nullable quoted counter.
        let (kind, window_len) = match &transfer.kind {
            TransferKind::Window(window) => ("window", json_string_len(&window.0)),
            TransferKind::View => ("view", 2),
            TransferKind::Migration => ("migration", 2),
        };
        bytes += 73
            + json_string_len(&transfer.operation_id)
            + json_string_len(kind)
            + window_len
            + json_string_len(&transfer.target_output.0)
            + json_string_len(&transfer.target_workspace)
            + transfer.command_seq.map_or(4, |seq| 2 + u64_len(seq));
    }
    bytes += reply.transfers.len().saturating_sub(1);
    for effect in &reply.effects {
        bytes += 67
            + json_string_len(&effect.id)
            + json_string_len(effect.kind.as_str())
            + json_string_len(&effect.target.0)
            + u64_len(effect.issued_revision)
            + json_string_len(effect.cause.as_str())
            + effect.command_seq.map_or(4, |seq| 2 + u64_len(seq))
            + bool_len(effect.await_shown);
    }
    bytes += reply.effects.len().saturating_sub(1);
    bytes += 14 + close_focus_estimate_v4(&reply.close_focus);
    bytes += command_result_estimate(&reply.command_result);
    bytes += gesture_settled_estimate(&reply.gesture_settled);
    bytes += operation_settled_estimate(&reply.operation_settled);
    bytes += native_creations_estimate(&reply.native_creations);
    bytes += warp_cursor_estimate(&reply.cursor_warp);
    bytes += presentation_estimate_v4(&reply.presentation);
    bytes += intent_writes_estimate(&reply.intent_writes);
    bytes += switch_settled_estimate(&reply.switch_settled);
    bytes
}

/// Structural size estimate for the staged v4 reply presentation with
/// the anchored border row. Keeps the legacy 76 base (which counts
/// the border braces separately); the anchored row counts its own braces,
/// so the estimate rides one byte above exact here, conservatively.
fn presentation_estimate_v4(presentation: &WorldPresentationV4) -> usize {
    use crate::world_presentation::PresentationUnderlay;
    let mut bytes = 76 + json_string_len(&presentation.foreground) + u64_len(presentation.rev);
    if let Some(output) = presentation.suspended_output.as_ref() {
        bytes += 19 + json_string_len(&output.0);
    }
    bytes += anchored_border_estimate(&presentation.border);
    match &presentation.underlay {
        PresentationUnderlay::Found(found) => {
            bytes += 129
                + u64_len(found.base_revision)
                + json_string_len(&found.home.output.0)
                + json_string_len(&found.home.workspace.0)
                + json_string_len(&found.group.0)
                + json_string_len(&found.focused_leaf.0)
                + json_string_len(&found.focused_window.0)
                + rect_len(Some(found.union_rect))
                + rect_len(Some(found.outer_rect));
            for member in &found.members {
                bytes += 28
                    + json_string_len(&member.leaf.0)
                    + json_string_len(&member.window.0)
                    + rect_len(Some(member.rect));
            }
            bytes += found.members.len().saturating_sub(1);
        }
        PresentationUnderlay::Hidden {
            base_revision,
            reason,
        } => {
            bytes += 42
                + json_string_len(reason.as_str())
                + base_revision.map_or(4, |rev| 2 + u64_len(rev));
        }
    }
    for screen in &presentation.pager {
        bytes += 26 + json_string_len(&screen.output);
        for workspace in &screen.workspaces {
            bytes += 88
                + json_string_len(&workspace.id)
                + usize_len(workspace.ordinal as usize)
                + json_string_len(&workspace.label)
                + bool_len(workspace.current)
                + bool_len(workspace.occupied)
                + bool_len(workspace.contains_focused);
            if workspace.trailing {
                bytes += 16;
            }
            for window in &workspace.windows {
                bytes += 19 + json_string_len(&window.window.0) + rect_len(Some(window.frame));
            }
            bytes += workspace.windows.len().saturating_sub(1);
        }
        bytes += screen.workspaces.len().saturating_sub(1);
    }
    bytes += presentation.pager.len().saturating_sub(1);
    bytes
}

/// Exact canonical v4 reply byte size for the transactional commit gate.
/// Pure length arithmetic mirroring
/// `encode_reply_v4` in the protocol crate
/// field-by-field; no core encoder twin is built (core cannot depend on
/// the protocol crate). Exactness is pinned by the protocol twin test
/// (`encode_reply_v4(&decoded).unwrap().len()` over every valid fixture).
#[must_use]
pub fn reply_exact_bytes_v4(reply: &WorldReplyV4) -> usize {
    // 307 joiner literals: every `]` closing one array section rides in
    // the next section's `],` joiner, so no per-section closer is added.
    let mut bytes = 307
        + usize_len(reply.contract_version as usize)
        + json_string_len(&reply.adapter_incarnation)
        + json_string_len(&reply.core_generation)
        + json_string_len(&reply.request_id)
        + u64_len(reply.based_on_observation_seq)
        + u64_len(reply.state_revision)
        + bool_len(reply.ready)
        + bool_len(reply.command_consumed);
    for scope in &reply.scopes {
        bytes += scope_status_estimate(scope);
    }
    bytes += reply.scopes.len().saturating_sub(1);
    for workspace in &reply.workspaces {
        // {"id":,"out":,"bind":,"scope":,"order":,"mode":,"exist":,
        // "hold":} = 65 punctuation plus values; ,"trailing":true adds 16.
        bytes += 65
            + json_string_len(&workspace.id)
            + json_string_len(&workspace.output.0)
            + json_string_len(workspace.native_binding.as_deref().unwrap_or_default())
            + authority_scope_len(&workspace.scope)
            + usize_len(workspace.order)
            + json_string_len(workspace.tiling_mode.as_str())
            + bool_len(workspace.existence)
            + json_string_len(
                workspace
                    .hold
                    .map_or("", crate::world_report::HoldReason::as_str),
            );
        if workspace.trailing {
            bytes += 16;
        }
    }
    bytes += reply.workspaces.len().saturating_sub(1);
    for output in &reply.outputs {
        // {"out":,"view":,"mode":,"hold":} = 32 punctuation.
        bytes += 32
            + json_string_len(&output.output.0)
            + json_string_len(output.logical_view.as_deref().unwrap_or_default())
            + json_string_len(output.tiling_mode.as_str())
            + json_string_len(
                output
                    .hold
                    .map_or("", crate::world_report::HoldReason::as_str),
            );
    }
    bytes += reply.outputs.len().saturating_sub(1);
    for diagnostic in &reply.diagnostics {
        // {"scope":,"row":,"id":,"kind":} = 31 punctuation.
        bytes += 31
            + authority_scope_len(&diagnostic.scope)
            + json_string_len(diagnostic.row.as_str())
            + diagnostic.id.as_deref().map_or(4, json_string_len)
            + json_string_len(diagnostic.kind.as_str());
    }
    bytes += reply.diagnostics.len().saturating_sub(1);
    for window in &reply.windows {
        // {"win":,"out":,"ws":,"alloc":,"frame":,"frame_auth":,
        // "frame_hold":,"member_auth":,"member_hold":,"frame_op":,
        // "floating":,"float_origin":,"clear_op":} = 149 punctuation plus
        // the frozen outbound target lanes.
        bytes += 149
            + json_string_len(&window.lifetime.0)
            + json_string_len(&window.output.0)
            + json_string_len(&window.workspace)
            + rect_len(window.allocation)
            + rect_len(window.effective_frame)
            + json_string_len(window.frame_authority.as_str())
            + json_string_len(
                window
                    .frame_authority
                    .hold_reason()
                    .map_or("", crate::world_report::HoldReason::as_str),
            )
            + json_string_len(window.membership_authority.as_str())
            + json_string_len(
                window
                    .membership_authority
                    .hold_reason()
                    .map_or("", crate::world_report::HoldReason::as_str),
            )
            + json_string_len(window.frame_operation.as_deref().unwrap_or_default())
            + bool_len(window.floating)
            + json_string_len(
                window
                    .float_origin
                    .map_or("", crate::world_report::FloatOrigin::as_str),
            )
            + json_string_len(window.clear_operation.as_deref().unwrap_or_default())
            + window_target_lanes_estimate(window);
    }
    bytes += reply.windows.len().saturating_sub(1);
    for transfer in &reply.transfers {
        // 53 row punctuation plus the `,"cmdSeq":` key (10) plus values.
        let (kind, window_len) = match &transfer.kind {
            TransferKind::Window(window) => ("window", json_string_len(&window.0)),
            TransferKind::View => ("view", 2),
            TransferKind::Migration => ("migration", 2),
        };
        bytes += 53
            + json_string_len(&transfer.operation_id)
            + json_string_len(kind)
            + window_len
            + json_string_len(&transfer.target_output.0)
            + json_string_len(&transfer.target_workspace)
            + bool_len(transfer.follow)
            + bool_len(transfer.held)
            + 10
            + transfer.command_seq.map_or(4, |seq| 2 + u64_len(seq));
    }
    bytes += reply.transfers.len().saturating_sub(1);
    for effect in &reply.effects {
        // {"id":,"kind":,"target":,"rev":"","cause":,"cmdSeq":,
        // "awaitShown":} = 67 punctuation.
        bytes += 67
            + json_string_len(&effect.id)
            + json_string_len(effect.kind.as_str())
            + json_string_len(&effect.target.0)
            + u64_len(effect.issued_revision)
            + json_string_len(effect.cause.as_str())
            + effect.command_seq.map_or(4, |seq| 2 + u64_len(seq))
            + bool_len(effect.await_shown);
    }
    bytes += reply.effects.len().saturating_sub(1);
    bytes += 14
        + reply.close_focus.as_ref().map_or(4, |request| {
            // 48 fixed: 50 joiner literals plus the 8 quote pairs the four
            // opaque fields carry, minus the 10 quotes the five
            // `json_string_len` values below already count (rev is unquoted
            // on both sides; its quotes ride the joiners).
            48 + json_string_len(&request.id)
                + json_string_len(&request.target.0)
                + u64_len(request.issued_revision)
                + json_string_len(request.cause.as_str())
                + json_string_len(&request.output.0)
                + json_string_len(&request.workspace)
        });
    bytes += command_result_exact_v4(&reply.command_result);
    bytes += reply.gesture_settled.as_ref().map_or(4, |terminal| {
        // {"id":"","committed":} = 22 punctuation.
        22 + u64_len(terminal.id) + bool_len(terminal.committed)
    });
    for event in &reply.operation_settled {
        // {"op":,"win":,"outcome":,"cause":} = 34 punctuation.
        bytes += 34
            + json_string_len(&event.operation_id)
            + json_string_len(&event.window.0)
            + json_string_len(event.outcome.as_str())
            + json_string_len(event.cause.as_deref().unwrap_or_default());
    }
    bytes += reply.operation_settled.len().saturating_sub(1);
    for creation in &reply.native_creations {
        // {"op":,"ws":,"out":} = 20 punctuation.
        bytes += 20
            + json_string_len(&creation.operation_id)
            + json_string_len(&creation.logical_workspace)
            + json_string_len(&creation.target_output.0);
    }
    bytes += reply.native_creations.len().saturating_sub(1);
    bytes += reply
        .cursor_warp
        .map_or(4, |(x, y)| 3 + int_len(x) + int_len(y));
    bytes += presentation_exact_v4(&reply.presentation);
    for write in &reply.intent_writes {
        // {"win":,"data":} = 16 punctuation.
        bytes +=
            16 + json_string_len(&write.win.0) + write.data.as_deref().map_or(4, json_string_len);
    }
    bytes += reply.intent_writes.len().saturating_sub(1);
    for event in &reply.switch_settled {
        // {"op":,"outcome":,"failed":[],"focus":,"fallback":} = 51 fixed.
        bytes += 51
            + json_string_len(&event.operation_id)
            + json_string_len(event.outcome.as_str())
            + json_string_len(event.focus.as_str())
            + bool_len(event.fallback);
        for (j, window) in event.failed.iter().enumerate() {
            if j > 0 {
                bytes += 1;
            }
            bytes += json_string_len(&window.0);
        }
    }
    bytes += reply.switch_settled.len().saturating_sub(1);
    bytes
}

/// Exact byte size of the v4 `result` section body (after the `],"result":`
/// joiner): `null` (4) when absent, else the per-op object. Mirrors the
/// shared [`encode_command_result`](crate::world_sizing::encode_command_result)
/// the v4 writer reuses.
fn command_result_exact_v4(result: &Option<CommandResult>) -> usize {
    let Some(result) = result else {
        return 4;
    };
    let rect_body =
        |r: crate::geometry::Rect| 5 + int_len(r.x) + int_len(r.y) + int_len(r.w) + int_len(r.h);
    match result {
        CommandResult::DragPreview {
            rect,
            hover,
            domain,
            source,
            target,
            revision,
        } => {
            // {"op":"drag-preview","rev":"","domainOut":"","domainWs":"",
            // "sourceOut":"","sourceWs":"","rect":[],"hover":,"win":""}
            // = 104 punctuation plus the rect body.
            let mut bytes = 104
                + u64_len(*revision)
                + json_string_len(&domain.output.0)
                + json_string_len(&domain.workspace.0)
                + json_string_len(&source.output.0)
                + json_string_len(&source.workspace.0)
                + json_string_len(&target.0)
                + rect_body(*rect);
            bytes += match hover {
                None => 4,
                Some(hover) => {
                    // {"domainOut":"","domainWs":"","leaf":"","win":"",
                    // "rev":"","prior":,"priorOut":"","priorWs":""} = 82
                    // punctuation; prior is null (4) or
                    // {"group":"","edge":""} (18 + widths).
                    82 + json_string_len(&hover.domain.output.0)
                        + json_string_len(&hover.domain.workspace.0)
                        + json_string_len(&hover.source_leaf.0)
                        + json_string_len(&hover.source_window.0)
                        + u64_len(hover.revision)
                        + hover.prior.as_ref().map_or(4, |prior| {
                            18 + json_string_len(&prior.group.0)
                                + json_string_len(prior.edge.as_str())
                        })
                        + json_string_len(&hover.prior_domain.output.0)
                        + json_string_len(&hover.prior_domain.workspace.0)
                }
            };
            bytes
        }
        CommandResult::Float {
            window,
            rect,
            no_space,
        } => {
            36 + json_string_len(&window.0)
                + rect.map_or(4, rect_body)
                + if *no_space { 15 } else { 0 }
        }
        CommandResult::Migration {
            direction,
            source,
            target,
            members,
            floats,
        } => {
            // {"op":"migrate-workspace","direction":"","sourceOut":"",
            // "sourceWs":"","targetOut":"","targetWs":"","members":[],
            // "floats":} = 112 punctuation.
            let mut bytes =
                112 + json_string_len(match direction {
                    crate::directional::Direction::Left => "left",
                    crate::directional::Direction::Right => "right",
                    crate::directional::Direction::Up => "up",
                    crate::directional::Direction::Down => "down",
                }) + json_string_len(&source.output.0)
                    + json_string_len(&source.workspace.0)
                    + json_string_len(&target.output.0)
                    + json_string_len(&target.workspace.0)
                    + usize_len(*floats);
            for member in members {
                bytes += json_string_len(&member.0);
            }
            bytes += members.len().saturating_sub(1);
            bytes
        }
        CommandResult::Directional {
            outcome,
            target,
            rect,
            cause,
        } => {
            // {"op":"directional","outcome":"","win":,"rect":,"cause":}
            // = 55 punctuation.
            let mut bytes = 55
                + json_string_len(outcome.as_str())
                + target.as_ref().map_or(4, |t| json_string_len(&t.0));
            bytes += rect.map_or(4, rect_body);
            bytes += cause.as_deref().map_or(4, json_string_len);
            bytes
        }
        CommandResult::Maximize {
            window,
            maximized,
            clear_fullscreen,
            was_maximized,
        } => {
            72 + json_string_len(&window.0)
                + bool_len(*maximized)
                + bool_len(*clear_fullscreen)
                + bool_len(*was_maximized)
        }
        CommandResult::Rejected { kind } => 25 + json_string_len(kind.as_str()),
    }
}

/// Exact byte size of the v4 reply presentation body (after the
/// `,"presentation":` joiner): the foreground token, the required quoted
/// revision label, the optional suspension verdict, the anchored border,
/// the underlay, and the pager.
fn presentation_exact_v4(presentation: &WorldPresentationV4) -> usize {
    use crate::world_presentation::PresentationUnderlay;
    // {"foreground":,"rev":"",} minus values: 14 + 8 + 1.
    let mut bytes = 23 + json_string_len(&presentation.foreground) + u64_len(presentation.rev);
    if let Some(output) = presentation.suspended_output.as_ref() {
        bytes += 19 + json_string_len(&output.0);
    }
    // ,"border": (10) plus the anchored row.
    bytes += 10 + anchored_border_estimate(&presentation.border);
    // ,"underlay": (12) plus the shape.
    bytes += 12
        + match &presentation.underlay {
            PresentationUnderlay::Found(found) => {
                // {"status":"found","reason":"","baseRev":"","homeOut":,
                // "homeWs":,"group":,"leaf":,"win":,"union":,"outer":,
                // "members":[]} = 119 punctuation plus values and members.
                let mut found_bytes = 119
                    + u64_len(found.base_revision)
                    + json_string_len(&found.home.output.0)
                    + json_string_len(&found.home.workspace.0)
                    + json_string_len(&found.group.0)
                    + json_string_len(&found.focused_leaf.0)
                    + json_string_len(&found.focused_window.0)
                    + rect_len(Some(found.union_rect))
                    + rect_len(Some(found.outer_rect));
                for member in &found.members {
                    // {"leaf":,"win":,"rect":} = 24 punctuation.
                    found_bytes += 24
                        + json_string_len(&member.leaf.0)
                        + json_string_len(&member.window.0)
                        + rect_len(Some(member.rect));
                }
                found_bytes += found.members.len().saturating_sub(1);
                found_bytes
            }
            PresentationUnderlay::Hidden {
                base_revision,
                reason,
            } => {
                // {"status":"hidden","reason":,"baseRev":} = 40 punctuation.
                40 + json_string_len(reason.as_str())
                    + base_revision.map_or(4, |rev| 2 + u64_len(rev))
            }
        };
    // ,"pager":[ (10) plus screens plus ] plus the presentation }.
    bytes += 12;
    for screen in &presentation.pager {
        // {"out":,"workspaces":[]} = 24 punctuation.
        bytes += 24 + json_string_len(&screen.output);
        for workspace in &screen.workspaces {
            // {"id":,"ordinal":,"label":,"current":,"occupied":,
            // "containsFocused":,"windows":[]} = 82 punctuation.
            bytes += 82
                + json_string_len(&workspace.id)
                + usize_len(workspace.ordinal as usize)
                + json_string_len(&workspace.label)
                + bool_len(workspace.current)
                + bool_len(workspace.occupied)
                + bool_len(workspace.contains_focused);
            if workspace.trailing {
                bytes += 16;
            }
            for window in &workspace.windows {
                // {"win":,"frame":} = 17 punctuation.
                bytes += 17 + json_string_len(&window.window.0) + rect_len(Some(window.frame));
            }
            bytes += workspace.windows.len().saturating_sub(1);
        }
        bytes += screen.workspaces.len().saturating_sub(1);
    }
    bytes += presentation.pager.len().saturating_sub(1);
    bytes
}

/// Pre-mutation bound for the at-most-one sticky/float save row over a v4
/// report over the staged report command. Unwraps the execution-time
/// subject and reserves the longest opaque id for implicit save-capable
/// commands, never the empty placeholder width.
#[must_use]
pub fn max_intent_save_bound_v4(report: &WorldReportV4) -> usize {
    use crate::world_report::WorldCommand;
    let unwrapped = report
        .command
        .as_ref()
        .map(crate::world_report::unwrap_implicit);
    let is_implicit = matches!(report.command.as_ref(), Some(WorldCommand::Implicit(_)));
    let window: &str = match unwrapped {
        Some(WorldCommand::SetSticky { window, .. }) => window.0.as_str(),
        Some(WorldCommand::ToggleSticky { window }) => window.0.as_str(),
        Some(WorldCommand::Operation(crate::boundary::CoreCommand::ToggleFloat {
            window, ..
        })) => window.as_str(),
        _ => return 0,
    };
    if is_implicit {
        return 16 + 130 + crate::world_sizing::MAX_INTENT_SAVE_DATA_JSON;
    }
    16 + json_string_len(window) + crate::world_sizing::MAX_INTENT_SAVE_DATA_JSON
}

/// True when one drained command can stage an intent save row: `SetSticky`,
/// `ToggleSticky`, or the sticky-clearing `ToggleFloat`, explicit or
/// execution-time. Every other command reserves nothing.
#[must_use]
pub fn is_intent_save_command_v4(command: &WorldCommand) -> bool {
    use crate::world_report::WorldCommand;
    matches!(
        crate::world_report::unwrap_implicit(command),
        WorldCommand::SetSticky { .. }
            | WorldCommand::ToggleSticky { .. }
            | WorldCommand::Operation(crate::boundary::CoreCommand::ToggleFloat { .. })
    )
}

/// Distinct save-row count for one parked FIFO batch: distinct explicit
/// subjects plus one row per implicit save (unresolved subjects conservatively
/// count distinct at max width). Latest-wins per window means repeats of one
/// explicit subject count once; correctness before tightness keeps implicit
/// rows unmerged.
#[must_use]
pub fn max_intent_save_batch_rows_v4(queued: &[(u64, WorldCommand)]) -> usize {
    use crate::world_report::WorldCommand;
    use std::collections::BTreeSet;
    let mut distinct: BTreeSet<&str> = BTreeSet::new();
    let mut implicit: usize = 0;
    for (_, command) in queued {
        if !is_intent_save_command_v4(command) {
            continue;
        }
        if matches!(command, WorldCommand::Implicit(_)) {
            implicit = implicit.saturating_add(1);
            continue;
        }
        match crate::world_report::unwrap_implicit(command) {
            WorldCommand::SetSticky { window, .. } => {
                distinct.insert(window.0.as_str());
            }
            WorldCommand::ToggleSticky { window } => {
                distinct.insert(window.0.as_str());
            }
            WorldCommand::Operation(crate::boundary::CoreCommand::ToggleFloat {
                window, ..
            }) => {
                distinct.insert(window.as_str());
            }
            _ => {}
        }
    }
    distinct.len().saturating_add(implicit)
}

/// Pre-mutation byte reserve for one parked FIFO batch: one
/// [`MAX_INTENT_SAVE_ROW_BYTES`](crate::world_sizing::MAX_INTENT_SAVE_ROW_BYTES)
/// per distinct explicit subject at its exact id width (plus comma) and one
/// max row per implicit save. Zero when the batch holds no save-capable
/// commands. Conservative like every other reply bound term; the
/// exact-encoded staged reply stays the commit backstop.
#[must_use]
pub fn max_intent_save_batch_bound_v4(queued: &[(u64, WorldCommand)]) -> usize {
    use crate::world_report::WorldCommand;
    use std::collections::BTreeSet;
    let mut distinct: BTreeSet<&str> = BTreeSet::new();
    let mut implicit: usize = 0;
    for (_, command) in queued {
        if !is_intent_save_command_v4(command) {
            continue;
        }
        if matches!(command, WorldCommand::Implicit(_)) {
            implicit = implicit.saturating_add(1);
            continue;
        }
        match crate::world_report::unwrap_implicit(command) {
            WorldCommand::SetSticky { window, .. } => {
                distinct.insert(window.0.as_str());
            }
            WorldCommand::ToggleSticky { window } => {
                distinct.insert(window.0.as_str());
            }
            WorldCommand::Operation(crate::boundary::CoreCommand::ToggleFloat {
                window, ..
            }) => {
                distinct.insert(window.as_str());
            }
            _ => {}
        }
    }
    let mut bytes = 0usize;
    for id in distinct {
        bytes = bytes.saturating_add(
            16 + json_string_len(id) + crate::world_sizing::MAX_INTENT_SAVE_DATA_JSON + 1,
        );
    }
    bytes.saturating_add(implicit.saturating_mul(crate::world_sizing::MAX_INTENT_SAVE_ROW_BYTES))
}

/// Conservative pre-mutation v4 reply upper bound for one report over the
/// staged report estimate. Per-unit reserves match the legacy bound except
/// where the v4 shapes are larger: retained transfers reserve 544 (the
/// legacy 512 plus the 32-byte `,"cmdSeq":"<u64::MAX>"` correlation) and
/// the border reserves 704 (the legacy 256 does not fit the widest
/// anchored row: 74 punctuation, widest bool, longest hide reason,
/// longest subject, widest outsets/width/radius, and widest witness total
/// 667, rounded up).
#[must_use]
pub fn reply_upper_bound_v4(
    report: &WorldReportV4,
    retained_transfers: usize,
    logical_workspaces: usize,
    created_workspaces: usize,
    staged_windows: usize,
    retained_windows: usize,
) -> usize {
    let report_windows: usize = report.windows.len().saturating_add(report.closed.len());
    let published_windows: usize = report
        .windows
        .len()
        .saturating_add(report.held_windows.len())
        .saturating_add(staged_windows)
        .saturating_add(retained_windows);
    report_estimate_v4(report)
        .saturating_add(retained_transfers.saturating_mul(544))
        .saturating_add(created_workspaces.saturating_mul(512))
        .saturating_add(logical_workspaces.saturating_mul(336))
        .saturating_add(4096)
        .saturating_add(MAX_EFFECT_RESERVE_BYTES)
        .saturating_add(2048)
        .saturating_add(512)
        .saturating_add(704)
        .saturating_add(29)
        .saturating_add(report_windows.saturating_mul(300))
        .saturating_add(1024)
        .saturating_add(logical_workspaces.saturating_mul(320))
        .saturating_add(logical_workspaces.saturating_mul(32))
        .saturating_add(report_windows.saturating_mul(160))
        .saturating_add(96)
        .saturating_add(report_windows.saturating_mul(131))
        .saturating_add(report_windows.saturating_mul(768))
        .saturating_add(logical_workspaces.saturating_mul(384))
        .saturating_add(report.held_screens.len().saturating_mul(512))
        .saturating_add(
            report
                .state_attempts
                .len()
                .saturating_mul(MAX_STATE_ATTEMPT_DIAGNOSTIC_BYTES),
        )
        .saturating_add(max_intent_save_bound_v4(report))
        .saturating_add(published_windows.saturating_mul(MAX_TARGET_LANES_PER_WINDOW))
}

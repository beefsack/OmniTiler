//! Protocol-owned conservative report estimate, preflight upper bound, and
//! future reserve sizing for [`ProtocolAdmissionGate`](crate::world_admission::ProtocolAdmissionGate).

use std::collections::BTreeSet;

use tiler_core::boundary::CoreCommand;
use tiler_core::world_report::{
    FocusChange, FocusChanges, NativeWorkspaceFact, WorldCommand, WorldReportV4,
    is_intent_save_command_v4,
};

use crate::world_admission_rows::{
    bool_len, command_estimate_owned, focus_feedback_estimate, gesture_ingress_v4_estimate,
    int_len, intent_records_estimate, json_string_len, presentation_row_v4_estimate, rect_len,
    renderer_input_estimate, u64_len, usize_len, visibility_sample_estimate,
    workspace_ref_option_len,
};

pub const MAX_CANONICAL_F64_LEN: usize = 350;
pub const MAX_OPERATION_EVENT_BYTES: usize = 362;
pub const MAX_SWITCH_EVENT_FIXED_BYTES: usize = 254;
pub const MAX_SWITCH_FAILED_ROW_BYTES: usize = 131;
pub const MAX_INTENT_DESIRED_DATA_JSON: usize = 49;
pub const MAX_INTENT_DESIRED_ROW_BYTES: usize = 196;
pub const MAX_STATE_ATTEMPT_DIAGNOSTIC_BYTES: usize = 512;
pub const MAX_TARGET_LANES_PER_WINDOW: usize = 626;

/// Widest wire bytes for the one required-nullable `warpRequest` lane: the
/// `,"warpRequest":` key prefix (15 bytes) plus the widest closed request
/// object `{id,target,rev,out,ws,point,pointerBaseline}`. The object carries
/// 128-byte opaque `id`/`target`/`out`/`ws` values, 20-digit quoted `rev`
/// and `pointerBaseline` decimals, and the widest carried `[x,y]` point
/// integers, plus keys, quotes, separators and braces.
pub const MAX_WARP_REQUEST_WIRE_BYTES: usize = 669;

#[must_use]
pub fn max_operation_settle_reserve(slot_count: usize) -> usize {
    if slot_count == 0 {
        return 0;
    }
    21 + slot_count.saturating_mul(MAX_OPERATION_EVENT_BYTES)
}

#[must_use]
pub fn max_switch_settle_reserve(event_count: usize, failed_total: usize) -> usize {
    if event_count == 0 {
        return 0;
    }
    19 + event_count.saturating_mul(MAX_SWITCH_EVENT_FIXED_BYTES)
        + failed_total.saturating_mul(MAX_SWITCH_FAILED_ROW_BYTES)
}

#[must_use]
pub fn max_intent_desired_bound(count: usize) -> usize {
    count.saturating_mul(MAX_INTENT_DESIRED_ROW_BYTES)
}

#[must_use]
pub fn report_estimate_v4(report: &WorldReportV4) -> usize {
    report_estimate_v4_with_residual(report, None)
}

#[must_use]
pub fn report_estimate_v4_with_residual(
    report: &WorldReportV4,
    residual: Option<&crate::world_report_residual::WorldReportResidual>,
) -> usize {
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
        + ",\"focusChanges\":".len()
        + focus_changes_len(report.focus_changes.as_ref())
        + bool_len(report.inventory_complete)
        + bool_len(report.gesture_barrier)
        + ",\"pointerActivity\":".len()
        + report
            .pointer_activity
            .map_or(4, |activity| 2 + u64_len(activity))
        + ",\"warpSpent\":".len()
        + report.warp_spent.as_deref().map_or(4, json_string_len)
        + presentation_row_v4_estimate(&report.presentation)
        + residual.map_or(0, |r| r.estimate_bytes());
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
                NativeWorkspaceFact::Omitted => 0,
                NativeWorkspaceFact::Null => 4,
                NativeWorkspaceFact::Value(id) => json_string_len(id),
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
            + fact.visibility.suppression_owned.map_or(4, bool_len);
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
    bytes += 20;
    for attempt in &report.frames {
        let purpose_len = match &attempt.purpose {
            tiler_core::world_report::FramePurpose::Layout => "{\"kind\":\"layout\"}".len(),
            tiler_core::world_report::FramePurpose::FloatPlacement {
                operation_id,
                prewrite,
            } => {
                "{\"kind\":\"float-placement\",\"op\":,\"pre\":}".len()
                    + json_string_len(operation_id)
                    + prewrite.map_or(4, |rect| {
                        5 + int_len(rect.x) + int_len(rect.y) + int_len(rect.w) + int_len(rect.h)
                    })
            }
        };
        bytes += "{\"kind\":\"frame\",\"win\":,\"target\":[,,,],\"disposition\":{\"kind\":\"called\",\"setter\":},\"readback\":,\"dpiBefore\":,\"dpiAfter\":,\"attemptId\":\"\",\"attemptMs\":,\"purpose\":}".len()
            + json_string_len(&attempt.window.0)
            + int_len(attempt.target.x)
            + int_len(attempt.target.y)
            + int_len(attempt.target.w)
            + int_len(attempt.target.h)
            + json_string_len(attempt.disposition.as_str())
            + attempt.readback.map_or(4, |rect| {
                5 + int_len(rect.x) + int_len(rect.y) + int_len(rect.w) + int_len(rect.h)
            })
            + attempt.dpi_before.map_or(4, |dpi| u64_len(u64::from(dpi)))
            + attempt.dpi_after.map_or(4, |dpi| u64_len(u64::from(dpi)))
            + u64_len(attempt.attempt_id)
            + attempt.attempt_ms.map_or(4, |ms| 2 + u64_len(ms))
            + purpose_len;
    }
    for attempt in &report.member_attempts {
        let disp_len = match attempt.disposition {
            tiler_core::world_report::MemberDisposition::Called(setter) => {
                "{\"kind\":\"called\",\"setter\":}".len() + json_string_len(setter.as_str())
            }
            tiler_core::world_report::MemberDisposition::AlreadyAtTarget => {
                "{\"kind\":\"already-at-target\"}".len()
            }
        };
        bytes += "{\"kind\":\"member\",\"op\":,\"win\":,\"out\":,\"ws\":,\"disposition\":,\"pre\":,\"post\":,\"attemptMs\":\"\"}".len()
            + json_string_len(&attempt.operation_id)
            + json_string_len(&attempt.window.0)
            + json_string_len(&attempt.target_output.0)
            + json_string_len(&attempt.target_workspace)
            + disp_len
            + workspace_ref_option_len(&attempt.pre)
            + workspace_ref_option_len(&attempt.post)
            + u64_len(attempt.attempt_ms);
    }
    for attempt in &report.view_attempts {
        bytes += "{\"kind\":\"view\",\"op\":,\"out\":,\"ws\":,\"disposition\":{\"kind\":\"called\",\"setter\":},\"pre\":,\"post\":,\"base\":,\"attemptMs\":\"\"}".len()
            + json_string_len(&attempt.operation_id)
            + json_string_len(&attempt.target_output.0)
            + json_string_len(&attempt.target_workspace)
            + json_string_len(attempt.disposition.as_str())
            + workspace_ref_option_len(&attempt.pre)
            + workspace_ref_option_len(&attempt.post)
            + workspace_ref_option_len(&attempt.baseline)
            + u64_len(attempt.attempt_ms);
    }
    for attempt in &report.state_attempts {
        bytes += "{\"kind\":\"state\",\"win\":,\"property\":,\"target\":,\"rev\":\"\",\"disposition\":{\"kind\":\"called\",\"setter\":},\"pre\":,\"post\":,\"attemptMs\":\"\"}".len()
            + json_string_len(&attempt.window.0)
            + json_string_len(attempt.property.as_str())
            + bool_len(attempt.target)
            + u64_len(attempt.revision)
            + json_string_len(attempt.disposition.as_str())
            + attempt.pre.map_or(4, bool_len)
            + attempt.post.map_or(4, bool_len)
            + u64_len(attempt.attempt_ms);
    }
    for attempt in &report.visibility_attempts {
        let disp_len = match attempt.disposition {
            tiler_core::world_report::VisibilityDisposition::Called { setter, reason } => {
                "{\"kind\":\"called\",\"setter\":},\"reason\":".len()
                    + json_string_len(setter.as_str())
                    + json_string_len(reason.as_str())
            }
            tiler_core::world_report::VisibilityDisposition::NotCalled { reason } => {
                "{\"kind\":\"not-called\",\"reason\":}".len() + json_string_len(reason.as_str())
            }
        };
        bytes += "{\"kind\":\"visibility\",\"win\":,\"target\":,\"revision\":\"\",\"path\":,\"disposition\":,\"pre\":,\"post\":}".len()
            + json_string_len(&attempt.window.0)
            + bool_len(attempt.target)
            + u64_len(attempt.revision)
            + json_string_len(attempt.path.as_str())
            + disp_len
            + visibility_sample_estimate(&attempt.pre)
            + visibility_sample_estimate(&attempt.post);
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
    bytes += gesture_ingress_v4_estimate(&report.gesture);
    if let Some(primary) = &report.primary_output {
        bytes += 11 + json_string_len(&primary.0);
    }
    bytes += intent_records_estimate(&report.intent_records);
    for (i, row) in report.focus_feedback.iter().enumerate() {
        if i > 0 {
            bytes += 1;
        }
        bytes += focus_feedback_estimate(row);
    }
    bytes += 17 + renderer_input_estimate(&report.renderer_input);
    bytes
}

fn focus_changes_len(changes: Option<&FocusChanges>) -> usize {
    let Some(changes) = changes else {
        return "null".len();
    };
    let events_len = match &changes.events {
        None => "null".len(),
        Some(events) => {
            "[]".len()
                + events.len().saturating_sub(1)
                + events
                    .iter()
                    .map(|event| match event {
                        FocusChange::Foreground { win } => {
                            "{\"kind\":\"foreground\",\"win\":}".len()
                                + win.as_ref().map_or(4, |win| json_string_len(&win.0))
                        }
                        FocusChange::View { out, native } => {
                            "{\"kind\":\"view\",\"out\":,\"native\":}".len()
                                + json_string_len(&out.0)
                                + json_string_len(native)
                        }
                    })
                    .sum::<usize>()
        }
    };
    "{\"after\":,\"events\":}".len()
        + changes.after.map_or(4, |after| 2 + u64_len(after))
        + events_len
}

#[must_use]
pub fn max_intent_save_bound_v4(report: &WorldReportV4) -> usize {
    use tiler_core::directional::WindowSubject;
    let Some(command) = report.command.as_ref() else {
        return 0;
    };
    let subject = match command {
        WorldCommand::SetSticky { window, .. } | WorldCommand::ToggleSticky { window } => window,
        WorldCommand::Operation(CoreCommand::ToggleFloat { window, .. }) => window,
        _ => return 0,
    };
    match subject {
        WindowSubject::CurrentFocus => 16 + 130 + MAX_INTENT_DESIRED_DATA_JSON,
        WindowSubject::Window(window) => {
            16 + json_string_len(&window.0) + MAX_INTENT_DESIRED_DATA_JSON
        }
    }
}

#[must_use]
pub fn max_intent_save_batch_bound_v4(queued: &[(u64, WorldCommand)]) -> usize {
    use tiler_core::directional::WindowSubject;
    let mut distinct: BTreeSet<&str> = BTreeSet::new();
    let mut implicit: usize = 0;
    for (_, command) in queued {
        if !is_intent_save_command_v4(command) {
            continue;
        }
        let subject = match command {
            WorldCommand::SetSticky { window, .. } | WorldCommand::ToggleSticky { window } => {
                Some(window)
            }
            WorldCommand::Operation(CoreCommand::ToggleFloat { window, .. }) => Some(window),
            _ => None,
        };
        match subject {
            Some(WindowSubject::CurrentFocus) => {
                implicit = implicit.saturating_add(1);
            }
            Some(WindowSubject::Window(window)) => {
                distinct.insert(window.0.as_str());
            }
            None => {}
        }
    }
    let mut bytes = 0usize;
    for id in distinct {
        bytes = bytes.saturating_add(16 + json_string_len(id) + MAX_INTENT_DESIRED_DATA_JSON + 1);
    }
    bytes.saturating_add(implicit.saturating_mul(MAX_INTENT_DESIRED_ROW_BYTES))
}

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
        .saturating_add(retained_transfers.saturating_mul(768))
        .saturating_add(created_workspaces.saturating_mul(512))
        .saturating_add(logical_workspaces.saturating_mul(336))
        .saturating_add(4096)
        .saturating_add(2048)
        .saturating_add(512)
        .saturating_add(704)
        .saturating_add(MAX_WARP_REQUEST_WIRE_BYTES)
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

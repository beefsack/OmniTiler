//! Staged v4 report twins: pure-function counterparts of the legacy
//! [`crate::world_report`] report gate, conflicting-fact grader, and digest
//! over [`WorldReportV4`](crate::world_report::WorldReportV4). The engine,
//! broker, service, adapters, and codec do not call these yet; no wiring
//! changes meaning. Row-level helpers stay shared with v3.

use std::collections::{BTreeMap, BTreeSet};

use crate::bounds::{GEOMETRY_BOUND, is_opaque_id, valid_carried_rect};
use crate::connection::FactName;
use crate::geometry::Rect;
use crate::world_report::{
    ArrivalLocation, AuthorityScope, HeldAttemptRow, MAX_ARRIVAL_BRACKETS_TOTAL,
    MAX_IDENTITY_TEXT_BYTES, MAX_INTENT_RECORDS, MAX_METADATA_TEXT_BYTES,
    MAX_NATIVE_OCCUPANTS_PER_DESKTOP, MAX_NATIVE_PER_OUTPUT, MAX_PRESENTATION_ORDER,
    MAX_WORLD_CLEAR_ATTEMPTS, MAX_WORLD_CLOSED, MAX_WORLD_CREATION_ATTEMPTS, MAX_WORLD_FACTS,
    MAX_WORLD_FOCUS_FEEDBACK, MAX_WORLD_FRAMES, MAX_WORLD_MEMBER_ATTEMPTS, MAX_WORLD_NATIVE,
    MAX_WORLD_SCREENS, MAX_WORLD_STATE_ATTEMPTS, MAX_WORLD_VIEW_ATTEMPTS, PresentationRowV4,
    RenderTarget, RowDiagnosticKind, WORLD_CONTRACT_VERSION_V4, WindowFact, WorkspaceRef,
    WorldCommand, WorldReportV4, check_screen_structure, check_window_structure,
    conflicting_window, gradable_window, hidden_window_id_violation, retained_window_fields,
    valid_command,
};

fn held_in_lane(report: &WorldReportV4, lane: crate::world_report::AttemptLaneV4) -> usize {
    report
        .held_attempts
        .iter()
        .filter(|held| held.lane == lane)
        .count()
}

fn held_native_order_len(held: &crate::world_report::HeldNativeRow) -> usize {
    held.fields
        .iter()
        .find(|(key, _)| key == "order")
        .map(|(_, value)| match value {
            crate::connection::RawSettingValue::Array(items) => items.len(),
            _ => 0,
        })
        .unwrap_or(0)
}

/// Staged v4 twin of
/// [`validate_report_reason`](crate::world_report::validate_report_reason):
/// version gate against [`WORLD_CONTRACT_VERSION_V4`], no `views` lane,
/// section caps mirroring the protocol `decode_report_v4` valid-plus-held
/// counting (including `held_native` and per-lane
/// `held_attempts`), the v4 renderer/command pairing
/// (`renderer_input.is_some()` requires `command.is_some()`), and the
/// v4-only presentation/renderer/native-held lanes. Same fixed reason tokens
/// as legacy where the check is the same; wire-level JSON decode checks are
/// not re-implemented.
#[must_use]
pub fn validate_report_v4_reason(report: &WorldReportV4) -> Option<&'static str> {
    use crate::world_report::AttemptLaneV4;
    if report.contract_version != WORLD_CONTRACT_VERSION_V4 {
        return Some("version-mismatch");
    }
    if !is_opaque_id(&report.adapter_incarnation) || !is_opaque_id(&report.request_id) {
        return Some("identity-invalid");
    }
    if !crate::ids::is_generation_id(&report.core_generation) {
        return Some("generation-invalid");
    }
    if report
        .screens
        .len()
        .saturating_add(report.held_screens.len())
        > MAX_WORLD_SCREENS
        || report
            .windows
            .len()
            .saturating_add(report.held_windows.len())
            > MAX_WORLD_FACTS
        || report.closed.len() > MAX_WORLD_CLOSED
        || report.native.len().saturating_add(report.held_native.len()) > MAX_WORLD_SCREENS
        || report
            .frames
            .len()
            .saturating_add(held_in_lane(report, AttemptLaneV4::Frame))
            > MAX_WORLD_FRAMES
        || report
            .member_attempts
            .len()
            .saturating_add(held_in_lane(report, AttemptLaneV4::Member))
            > MAX_WORLD_MEMBER_ATTEMPTS
        || report
            .view_attempts
            .len()
            .saturating_add(held_in_lane(report, AttemptLaneV4::View))
            > MAX_WORLD_VIEW_ATTEMPTS
        || report
            .clear_attempts
            .len()
            .saturating_add(held_in_lane(report, AttemptLaneV4::Clear))
            > MAX_WORLD_CLEAR_ATTEMPTS
        || report
            .creation_attempts
            .len()
            .saturating_add(held_in_lane(report, AttemptLaneV4::Creation))
            > MAX_WORLD_CREATION_ATTEMPTS
        || report.state_attempts.len() > MAX_WORLD_STATE_ATTEMPTS
        || report.focus_feedback.len() > MAX_WORLD_FOCUS_FEEDBACK
        || report.intent_records.len() > MAX_INTENT_RECORDS
    {
        return Some("section-over-cap");
    }
    if report
        .focus_feedback
        .iter()
        .map(|row| row.arrival.len())
        .sum::<usize>()
        > MAX_ARRIVAL_BRACKETS_TOTAL
    {
        return Some("section-over-cap");
    }
    if let Some((x, y)) = report.cursor
        && (!(-GEOMETRY_BOUND..=GEOMETRY_BOUND).contains(&x)
            || !(-GEOMETRY_BOUND..=GEOMETRY_BOUND).contains(&y))
    {
        return Some("cursor-invalid");
    }
    if let Some(hit) = report.cursor_hit.as_ref() {
        if report.cursor.is_none() {
            return Some("cursor-hit-without-cursor");
        }
        if !report.windows.iter().any(|fact| fact.window == *hit) {
            return Some("cursor-hit-unknown");
        }
    }
    for output in [&report.action_foreground, &report.action_cached]
        .into_iter()
        .flatten()
    {
        if !is_opaque_id(&output.0) {
            return Some("action-output-invalid");
        }
    }
    {
        let mut seen_action_outputs = BTreeSet::new();
        for output in &report.action_outputs {
            if !is_opaque_id(&output.0) {
                return Some("action-outputs-invalid");
            }
            if !seen_action_outputs.insert(output.0.clone()) {
                return Some("duplicate-action-output");
            }
        }
    }
    if let Some(gesture) = report.gesture.as_ref() {
        if !is_opaque_id(&gesture.window.0) {
            return Some("gesture-window-invalid");
        }
        if gesture.id == 0 {
            return Some("gesture-id-invalid");
        }
        if let Some((x, y)) = gesture.cursor
            && (!(-GEOMETRY_BOUND..=GEOMETRY_BOUND).contains(&x)
                || !(-GEOMETRY_BOUND..=GEOMETRY_BOUND).contains(&y))
        {
            return Some("gesture-cursor-invalid");
        }
        if let Some((x, y)) = gesture.start_cursor
            && (!(-GEOMETRY_BOUND..=GEOMETRY_BOUND).contains(&x)
                || !(-GEOMETRY_BOUND..=GEOMETRY_BOUND).contains(&y))
        {
            return Some("gesture-cursor-invalid");
        }
        if !valid_carried_rect(
            gesture.start_frame.x,
            gesture.start_frame.y,
            gesture.start_frame.w,
            gesture.start_frame.h,
        ) {
            return Some("gesture-frame-invalid");
        }
    }
    if report.command_seq.is_none() != report.command.is_none() {
        return Some("command-pair-invalid");
    }
    if let Some(command) = report.command.as_ref() {
        // V4 by-id focus spells the removed direction as the empty string;
        // the legacy direction vocabulary would misread it as invalid.
        let bare_focus =
            matches!(command, WorldCommand::Focus { direction, .. } if direction.is_empty());
        if !bare_focus && !valid_command(command) {
            return Some("command-invalid");
        }
    }
    if report.renderer_input.is_some() && report.command.is_none() {
        return Some("command-pair-invalid");
    }
    if let Some(primary) = report.primary_output.as_ref()
        && !is_opaque_id(&primary.0)
    {
        return Some("primary-invalid");
    }
    let mut seen_windows = BTreeSet::new();
    let mut seen_outputs = BTreeSet::new();
    for screen in &report.screens {
        if let Some(reason) = check_screen_structure(screen, &mut seen_outputs) {
            return Some(reason);
        }
        if !screen.scale.is_finite() || screen.scale <= 0.0 {
            return Some("screen-scale-invalid");
        }
        for field in [
            screen.identity.manufacturer.as_deref(),
            screen.identity.model.as_deref(),
            screen.identity.serial.as_deref(),
            screen.identity.connector.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            if field.len() > MAX_IDENTITY_TEXT_BYTES {
                return Some("screen-identity-invalid");
            }
        }
    }
    for held in &report.held_screens {
        if !is_opaque_id(&held.output.0) {
            return Some("screen-id-invalid");
        }
        if !seen_outputs.insert(held.output.0.clone()) {
            return Some("duplicate-screen");
        }
    }
    {
        let total = report
            .screens
            .len()
            .saturating_add(report.held_screens.len());
        let mut seen_ordinals = BTreeSet::new();
        for held in &report.held_screens {
            if held.ordinal >= total || !seen_ordinals.insert(held.ordinal) {
                return Some("held-ordinal-invalid");
            }
        }
    }
    {
        for fact in &report.windows {
            if let Some(reason) = check_window_structure(fact, &mut seen_windows) {
                return Some(reason);
            }
            for field in [
                fact.role.as_deref(),
                fact.app_id.as_deref(),
                fact.executable.as_deref(),
                fact.class.as_deref(),
                fact.title.as_deref(),
            ]
            .into_iter()
            .flatten()
            {
                if field.len() > MAX_METADATA_TEXT_BYTES {
                    return Some("window-metadata-invalid");
                }
            }
            if let Some(native) = fact.native_workspace.as_deref()
                && !is_opaque_id(native)
            {
                return Some("window-native-invalid");
            }
            if let Some(process) = fact.process_id.as_deref()
                && !crate::connection::valid_process_identity_text(process)
            {
                return Some("window-identity-invalid");
            }
            for id in [&fact.owner, &fact.parent].into_iter().flatten() {
                if !is_opaque_id(&id.0) {
                    return Some("window-id-invalid");
                }
            }
        }
        for held in &report.held_windows {
            if !is_opaque_id(&held.window.0) {
                return Some("window-id-invalid");
            }
            if !seen_windows.insert(held.window.0.clone()) {
                return Some("duplicate-window");
            }
        }
    }
    {
        let total = report
            .windows
            .len()
            .saturating_add(report.held_windows.len());
        let mut seen_ordinals = BTreeSet::new();
        for held in &report.held_windows {
            if held.ordinal >= total || !seen_ordinals.insert(held.ordinal) {
                return Some("held-ordinal-invalid");
            }
        }
    }
    for id in &report.closed {
        if !is_opaque_id(&id.0) {
            return Some("closed-id-invalid");
        }
        if seen_windows.contains(&id.0) {
            return Some("closed-overlaps-live");
        }
    }
    {
        let mut seen_closed = BTreeSet::new();
        for id in &report.closed {
            if !seen_closed.insert(id.0.clone()) {
                return Some("duplicate-closed");
            }
        }
    }
    let mut total_native = 0usize;
    {
        let mut seen_native_outputs = BTreeSet::new();
        for native in &report.native {
            if !is_opaque_id(&native.output.0)
                || native
                    .current_native
                    .as_deref()
                    .is_some_and(|current| !is_opaque_id(current))
                || native.native_order.len() > MAX_NATIVE_PER_OUTPUT
            {
                return Some("native-invalid");
            }
            if !seen_outputs.contains(&native.output.0) {
                return Some("native-output-unknown");
            }
            if !seen_native_outputs.insert(native.output.0.clone()) {
                return Some("duplicate-native");
            }
            {
                let mut seen_ids = BTreeSet::new();
                for id in &native.native_order {
                    if !is_opaque_id(id) {
                        return Some("native-id-invalid");
                    }
                    if !seen_ids.insert(id.clone()) {
                        return Some("duplicate-native");
                    }
                }
            }
            {
                if native.occupancy.len() != native.native_order.len() {
                    return Some("native-occupancy-incomplete");
                }
                let mut seen_occupancy = BTreeSet::new();
                for row in &native.occupancy {
                    if !is_opaque_id(&row.native) {
                        return Some("native-occupancy-invalid");
                    }
                    if !seen_occupancy.insert(row.native.clone()) {
                        return Some("duplicate-native-occupancy");
                    }
                    if row
                        .count
                        .is_some_and(|count| count > MAX_NATIVE_OCCUPANTS_PER_DESKTOP)
                    {
                        return Some("native-occupancy-invalid");
                    }
                }
                for id in &native.native_order {
                    if !seen_occupancy.contains(id) {
                        return Some("native-occupancy-incomplete");
                    }
                }
            }
            total_native += native.native_order.len();
        }
        for held in &report.held_native {
            if !is_opaque_id(&held.output.0) {
                return Some("native-invalid");
            }
            if !seen_outputs.contains(&held.output.0) {
                return Some("native-output-unknown");
            }
            if !seen_native_outputs.insert(held.output.0.clone()) {
                return Some("duplicate-native");
            }
            total_native = total_native.saturating_add(held_native_order_len(held));
        }
    }
    if total_native > MAX_WORLD_NATIVE {
        return Some("native-over-cap");
    }
    {
        let total = report.native.len().saturating_add(report.held_native.len());
        let mut seen_ordinals = BTreeSet::new();
        for held in &report.held_native {
            if held.ordinal >= total || !seen_ordinals.insert(held.ordinal) {
                return Some("held-ordinal-invalid");
            }
        }
    }
    {
        let mut seen_frames = BTreeSet::new();
        for attempt in &report.frames {
            if !is_opaque_id(&attempt.window.0) {
                return Some("frame-window-invalid");
            }
            if !valid_carried_rect(
                attempt.target.x,
                attempt.target.y,
                attempt.target.w,
                attempt.target.h,
            ) {
                return Some("frame-target-invalid");
            }
            if let Some(readback) = attempt.readback
                && !valid_carried_rect(readback.x, readback.y, readback.w, readback.h)
            {
                return Some("frame-readback-invalid");
            }
            if let Some(prewrite) = attempt.prewrite
                && !valid_carried_rect(prewrite.x, prewrite.y, prewrite.w, prewrite.h)
            {
                return Some("frame-prewrite-invalid");
            }
            if let Some(op) = attempt.operation_id.as_deref()
                && !op.is_empty()
                && !is_opaque_id(op)
            {
                return Some("frame-operation-invalid");
            }
            if attempt.operation_id.as_deref().is_none_or(str::is_empty)
                && attempt.prewrite.is_some()
            {
                return Some("frame-pre-without-operation");
            }
            if !seen_frames.insert(attempt.window.0.clone()) {
                return Some("duplicate-frame");
            }
        }
    }
    {
        let mut seen_members = BTreeSet::new();
        for attempt in &report.member_attempts {
            if !is_opaque_id(&attempt.operation_id) || !is_opaque_id(&attempt.window.0) {
                return Some("member-window-invalid");
            }
            if !is_opaque_id(&attempt.target_output.0) || !is_opaque_id(&attempt.target_workspace) {
                return Some("member-target-invalid");
            }
            for reference in [&attempt.pre, &attempt.post].into_iter().flatten() {
                if !valid_attempt_workspace_ref(reference) {
                    return Some("member-homing-invalid");
                }
            }
            if !seen_members.insert((attempt.operation_id.clone(), attempt.window.0.clone())) {
                return Some("duplicate-member");
            }
        }
    }
    {
        let mut seen_views = BTreeSet::new();
        for attempt in &report.view_attempts {
            if !is_opaque_id(&attempt.operation_id) {
                return Some("view-operation-invalid");
            }
            if !is_opaque_id(&attempt.target_output.0) || !is_opaque_id(&attempt.target_workspace) {
                return Some("view-target-invalid");
            }
            for reference in [&attempt.pre, &attempt.post, &attempt.baseline]
                .into_iter()
                .flatten()
            {
                if !valid_attempt_workspace_ref(reference) {
                    return Some("view-proof-invalid");
                }
            }
            if !seen_views.insert(attempt.operation_id.clone()) {
                return Some("duplicate-view-attempt");
            }
        }
    }
    {
        let mut seen_clears = BTreeSet::new();
        for attempt in &report.clear_attempts {
            if !is_opaque_id(&attempt.operation_id) || !is_opaque_id(&attempt.window.0) {
                return Some("clear-window-invalid");
            }
            if !seen_clears.insert((attempt.operation_id.clone(), attempt.window.0.clone())) {
                return Some("duplicate-clear-attempt");
            }
        }
    }
    {
        let mut seen_states = BTreeSet::new();
        for attempt in &report.state_attempts {
            if !is_opaque_id(&attempt.window.0) {
                return Some("state-window-invalid");
            }
            if !seen_states.insert((attempt.window.0.clone(), attempt.property)) {
                return Some("duplicate-state-attempt");
            }
        }
    }
    {
        // Held attempt ordinals index their own carried lane array: each one
        // names a distinct slot shared with the valid rows of that lane.
        for lane in [
            AttemptLaneV4::Frame,
            AttemptLaneV4::Member,
            AttemptLaneV4::View,
            AttemptLaneV4::Clear,
            AttemptLaneV4::Creation,
        ] {
            let valid = match lane {
                AttemptLaneV4::Frame => report.frames.len(),
                AttemptLaneV4::Member => report.member_attempts.len(),
                AttemptLaneV4::View => report.view_attempts.len(),
                AttemptLaneV4::Clear => report.clear_attempts.len(),
                AttemptLaneV4::Creation => report.creation_attempts.len(),
            };
            let total = valid.saturating_add(held_in_lane(report, lane));
            let mut seen_ordinals = BTreeSet::new();
            for held in report.held_attempts.iter().filter(|held| held.lane == lane) {
                if held.ordinal >= total || !seen_ordinals.insert(held.ordinal) {
                    return Some("held-ordinal-invalid");
                }
            }
        }
    }
    {
        // Capability-named note: one execution authorization per op; exact
        // op-plus-logical-plus-output match, strict order bounds, replay
        // idempotent by op id.
        let mut creation_triples: BTreeMap<String, (String, String)> = BTreeMap::new();
        let mut creation_receipts: BTreeSet<String> = BTreeSet::new();
        let mut total_creation_ids = 0usize;
        for attempt in &report.creation_attempts {
            if !is_opaque_id(&attempt.operation_id)
                || !is_opaque_id(&attempt.logical_workspace)
                || !is_opaque_id(&attempt.target_output.0)
            {
                return Some("creation-target-invalid");
            }
            if let Some(created) = attempt.created_native.as_deref()
                && !is_opaque_id(created)
            {
                return Some("creation-id-invalid");
            }
            if attempt.order_before.len() > MAX_NATIVE_PER_OUTPUT
                || attempt.order_after.len() > MAX_NATIVE_PER_OUTPUT
            {
                return Some("creation-order-over-cap");
            }
            for id in attempt
                .order_before
                .iter()
                .chain(attempt.order_after.iter())
            {
                if !is_opaque_id(id) {
                    return Some("creation-id-invalid");
                }
            }
            {
                let mut seen_before = BTreeSet::new();
                for id in &attempt.order_before {
                    if !seen_before.insert(id.clone()) {
                        return Some("duplicate-creation-attempt");
                    }
                }
                let mut seen_after = BTreeSet::new();
                for id in &attempt.order_after {
                    if !seen_after.insert(id.clone()) {
                        return Some("duplicate-creation-attempt");
                    }
                }
            }
            match creation_triples.insert(
                attempt.operation_id.clone(),
                (
                    attempt.logical_workspace.clone(),
                    attempt.target_output.0.clone(),
                ),
            ) {
                None => {}
                Some(prev)
                    if prev
                        == (
                            attempt.logical_workspace.clone(),
                            attempt.target_output.0.clone(),
                        ) =>
                {
                    return Some("duplicate-creation-attempt");
                }
                Some(_) => return Some("creation-triple-mismatch"),
            }
            total_creation_ids += attempt.order_before.len() + attempt.order_after.len();
            if let Some(created) = attempt.created_native.as_deref() {
                total_creation_ids += 1;
                if !creation_receipts.insert(created.to_owned()) {
                    return Some("creation-receipt-reused");
                }
            }
        }
        if total_creation_ids > MAX_WORLD_NATIVE.saturating_mul(2) {
            return Some("creation-over-cap");
        }
    }
    {
        for row in &report.focus_feedback {
            if !is_opaque_id(&row.id) {
                return Some("focus-id-invalid");
            }
            if !is_opaque_id(&row.target.0) {
                return Some("focus-target-invalid");
            }
            for endpoint in [&row.pre, &row.post].into_iter().flatten() {
                if !is_opaque_id(&endpoint.0) {
                    return Some("focus-foreground-invalid");
                }
            }
            for bracket in &row.arrival {
                if !is_opaque_id(&bracket.window.0) {
                    return Some("arrival-window-invalid");
                }
                match &bracket.location {
                    ArrivalLocation::Native { home } => {
                        if let Some(home) = home.as_ref() {
                            let is_native = matches!(home, WorkspaceRef::Native { .. });
                            if !is_native || !valid_attempt_workspace_ref(home) {
                                return Some("arrival-home-invalid");
                            }
                        }
                    }
                    ArrivalLocation::Output { out } => {
                        if let Some(out) = out.as_ref()
                            && !is_opaque_id(&out.0)
                        {
                            return Some("arrival-output-invalid");
                        }
                    }
                }
                if let Some(frame) = bracket.frame
                    && !valid_carried_rect(frame.x, frame.y, frame.w, frame.h)
                {
                    return Some("arrival-frame-invalid");
                }
            }
        }
    }
    match &report.presentation {
        PresentationRowV4::Valid(ingress) => {
            if !ingress.foreground.is_empty() && !is_opaque_id(&ingress.foreground) {
                return Some("presentation-foreground-invalid");
            }
            if !ingress.move_window.is_empty() && !is_opaque_id(&ingress.move_window) {
                return Some("presentation-move-invalid");
            }
            if let Some(frame) = ingress.foreground_frame
                && !valid_carried_rect(frame.x, frame.y, frame.w, frame.h)
            {
                return Some("presentation-frame-invalid");
            }
            if ingress.pager_order.len() > MAX_PRESENTATION_ORDER {
                return Some("presentation-order-over-cap");
            }
            for window in &ingress.pager_order {
                if !is_opaque_id(&window.0) {
                    return Some("presentation-order-invalid");
                }
            }
            if ingress.pager_frames.len() > MAX_PRESENTATION_ORDER {
                return Some("presentation-frames-over-cap");
            }
            for row in &ingress.pager_frames {
                if !is_opaque_id(&row.window.0) {
                    return Some("presentation-frame-invalid");
                }
                if let Some(live) = row.live
                    && !valid_carried_rect(live.x, live.y, live.w, live.h)
                {
                    return Some("presentation-frame-invalid");
                }
            }
            if let Some(witness) = ingress.witness.as_ref() {
                if !is_opaque_id(&witness.win.0) || !is_opaque_id(&witness.foreground.0) {
                    return Some("presentation-frame-invalid");
                }
                if !valid_carried_rect(
                    witness.frame.x,
                    witness.frame.y,
                    witness.frame.w,
                    witness.frame.h,
                ) {
                    return Some("presentation-frame-invalid");
                }
            }
            if let Some(out) = ingress.foreground_out.as_ref()
                && !is_opaque_id(&out.0)
            {
                return Some("presentation-foreground-invalid");
            }
        }
        PresentationRowV4::Held(_) => {}
    }
    for record in &report.intent_records {
        if !is_opaque_id(&record.win.0) {
            return Some("window-id-invalid");
        }
        if record.data.len() > crate::intent_data::MAX_INTENT_DATA_BYTES {
            return Some("intent-data-invalid");
        }
    }
    if let Some(input) = report.renderer_input.as_ref() {
        match &input.target {
            RenderTarget::Workspace { out, ws } => {
                if !is_opaque_id(&out.0) || !is_opaque_id(&ws.0) {
                    return Some("renderer-target-invalid");
                }
            }
            RenderTarget::Window { out, ws, win } => {
                if !is_opaque_id(&out.0) || !is_opaque_id(&ws.0) || !is_opaque_id(&win.0) {
                    return Some("renderer-target-invalid");
                }
            }
            RenderTarget::Group { out, ws, group } => {
                if !is_opaque_id(&out.0) || !is_opaque_id(&ws.0) || !is_opaque_id(&group.0) {
                    return Some("renderer-target-invalid");
                }
            }
        }
    }
    None
}

fn valid_attempt_workspace_ref(reference: &WorkspaceRef) -> bool {
    match reference {
        WorkspaceRef::Logical { output, workspace } => {
            is_opaque_id(&output.0) && is_opaque_id(&workspace.0)
        }
        WorkspaceRef::Native { output, workspace } => {
            is_opaque_id(&output.0) && is_opaque_id(workspace.as_str())
        }
    }
}

/// Staged v4 twin of
/// [`grade_conflicting_windows`](crate::world_report::grade_conflicting_windows):
/// unavailable-fact contradictions move to `held_windows` with a
/// `ConflictingFact` diagnostic. A `cursor_hit` naming a newly graded window
/// is cleared on the effective (normalized) rows; the input report is never
/// mutated and every other normalized invalidity still refuses grading.
/// No twins of the invalid-screen/window
/// graders: the v4 decoder already holds invalid rows, and the unlikely
/// typed-value path is reported, not built.
#[must_use]
pub fn grade_conflicting_windows_v4(
    report: &WorldReportV4,
    unavailable: &[FactName],
    native_sticky_supported: bool,
    maximize_supported: bool,
) -> Option<WorldReportV4> {
    if unavailable.is_empty() && native_sticky_supported && maximize_supported {
        return None;
    }
    {
        let mut seen = BTreeSet::new();
        for held in &report.held_windows {
            if !is_opaque_id(&held.window.0) || !seen.insert(held.window.0.clone()) {
                return None;
            }
        }
        for fact in &report.windows {
            if check_window_structure(fact, &mut seen).is_some() {
                return None;
            }
            if hidden_window_id_violation(fact) {
                return None;
            }
        }
    }
    let total = report
        .windows
        .len()
        .saturating_add(report.held_windows.len());
    let used: BTreeSet<usize> = report
        .held_windows
        .iter()
        .map(|held| held.ordinal)
        .collect();
    let mut free = (0..total).filter(|slot| !used.contains(slot));
    let mut windows = Vec::with_capacity(report.windows.len());
    let mut graded = Vec::new();
    for fact in &report.windows {
        let slot = free.next()?;
        if !gradable_window(fact)
            && conflicting_window(
                fact,
                unavailable,
                native_sticky_supported,
                maximize_supported,
            )
        {
            graded.push(crate::world_report::HeldWindowRow {
                window: fact.window.clone(),
                ordinal: slot,
                output: fact.output.clone(),
                fields: retained_window_fields(fact),
                diagnostic: fact
                    .output
                    .as_ref()
                    .map(|out| crate::world_report::RowDiagnostic {
                        scope: AuthorityScope::Output(out.clone()),
                        row: crate::world_report::RowDiagnosticLane::Window,
                        id: Some(fact.window.0.clone()),
                        kind: RowDiagnosticKind::ConflictingFact,
                    }),
                kind: RowDiagnosticKind::ConflictingFact,
            });
        } else {
            windows.push(fact.clone());
        }
    }
    if graded.is_empty() {
        return None;
    }
    let mut normalized = report.clone();
    normalized.windows = windows;
    // The raw capture keeps the native hit verbatim; the effective copy
    // clears a hit naming a newly graded hold so the dangling reference
    // does not discard grading. `graded` holds only original typed rows,
    // so unknown or pre-existing-held hits cannot match, and a missing
    // cursor leaves the malformed original to the final recheck below.
    let hit_newly_held = report.cursor.is_some()
        && normalized
            .cursor_hit
            .as_ref()
            .is_some_and(|hit| graded.iter().any(|held| held.window == *hit));
    if hit_newly_held {
        normalized.cursor_hit = None;
    }
    normalized.held_windows.extend(graded);
    normalized.held_windows.sort_by_key(|held| held.ordinal);
    if validate_report_v4_reason(&normalized).is_some() {
        return None;
    }
    Some(normalized)
}

/// Staged v4 twin of [`report_digest`](crate::world_report::report_digest):
/// deterministic FNV-1a over the canonical v4 report content. Covers every
/// [`WorldReportV4`] lane including `held_native`, `held_attempts`, the
/// presentation row (Valid and Held arms), and `renderer_input`; no views.
/// The destructure names every field so a future field addition fails to
/// compile instead of escaping the digest.
#[must_use]
pub fn report_digest_v4(report: &WorldReportV4) -> u64 {
    fn feed(hash: &mut u64, text: &str) {
        for byte in text.bytes() {
            *hash ^= u64::from(byte);
            *hash = hash.wrapping_mul(1099511628211);
        }
    }
    fn sep(hash: &mut u64) {
        *hash ^= 0xff;
        *hash = hash.wrapping_mul(1099511628211);
    }
    fn flag(hash: &mut u64, set: bool) {
        feed(hash, if set { "1" } else { "0" });
        sep(hash);
    }
    fn rect_part(rect: Option<Rect>) -> String {
        rect.map(|rect| format!("{},{},{},{}", rect.x, rect.y, rect.w, rect.h))
            .unwrap_or_default()
    }
    fn feed_raw(hash: &mut u64, value: &crate::connection::RawSettingValue) {
        match value {
            crate::connection::RawSettingValue::Null => feed(hash, "null"),
            crate::connection::RawSettingValue::Bool(set) => {
                feed(hash, if *set { "true" } else { "false" });
            }
            crate::connection::RawSettingValue::Number(number) => {
                feed(hash, &number.to_bits().to_string());
            }
            crate::connection::RawSettingValue::String(text) => feed(hash, text),
            crate::connection::RawSettingValue::Array(items) => {
                for item in items {
                    feed_raw(hash, item);
                    sep(hash);
                }
            }
            crate::connection::RawSettingValue::Object(entries) => {
                for (key, item) in entries {
                    feed(hash, key);
                    sep(hash);
                    feed_raw(hash, item);
                    sep(hash);
                }
            }
        }
        sep(hash);
    }
    fn feed_diagnostic(hash: &mut u64, diagnostic: &crate::world_report::RowDiagnostic) {
        feed(hash, diagnostic.kind.as_str());
        sep(hash);
        feed(hash, diagnostic.row.as_str());
        sep(hash);
        feed(hash, diagnostic.id.as_deref().unwrap_or_default());
        sep(hash);
        match &diagnostic.scope {
            AuthorityScope::Output(output) => {
                feed(hash, "output");
                sep(hash);
                feed(hash, &output.0);
                sep(hash);
            }
            AuthorityScope::Workspace { output, workspace } => {
                feed(hash, "workspace");
                sep(hash);
                feed(hash, &output.0);
                sep(hash);
                feed(hash, &workspace.0);
                sep(hash);
            }
        }
        flag(hash, diagnostic.id.is_some());
    }
    fn feed_visibility_sample(hash: &mut u64, sample: &crate::world_report::VisibilitySample) {
        feed(hash, &sample.obs.to_string());
        sep(hash);
        for value in [
            sample.lifetime_matched,
            sample.visibility.visible,
            sample.visibility.suppression_owned,
            sample.minimized,
            sample.host_hidden,
        ] {
            feed(hash, &value.map(|set| set.to_string()).unwrap_or_default());
            sep(hash);
            flag(hash, value.is_some());
        }
    }
    fn workspace_ref_fingerprint(reference: &WorkspaceRef) -> String {
        match reference {
            WorkspaceRef::Logical { output, workspace } => {
                format!("logical/{}/{}", output.0, workspace.0)
            }
            WorkspaceRef::Native { output, workspace } => {
                format!("native/{}/{}", output.0, workspace.as_str())
            }
        }
    }
    let crate::world_report::WorldReportV4 {
        contract_version,
        adapter_incarnation,
        core_generation,
        observation_seq,
        request_seq,
        request_id,
        command_seq,
        command,
        screens,
        held_screens,
        windows,
        held_windows,
        closed,
        native,
        held_native,
        cursor,
        cursor_hit,
        focused_global,
        action_foreground,
        action_cached,
        action_outputs,
        primary_output,
        inventory_complete,
        gesture_barrier,
        gesture,
        monotonic_ms,
        frames,
        member_attempts,
        view_attempts,
        clear_attempts,
        creation_attempts,
        held_attempts,
        state_attempts,
        focus_feedback,
        presentation,
        intent_records,
        renderer_input,
    } = report;
    let mut hash: u64 = 14695981039346656037;
    for part in [
        request_seq.to_string(),
        observation_seq.to_string(),
        contract_version.to_string(),
        adapter_incarnation.clone(),
        core_generation.clone(),
        request_id.clone(),
        command_seq.map(|s| s.to_string()).unwrap_or_default(),
        format!("{command:?}"),
    ] {
        feed(&mut hash, &part);
        sep(&mut hash);
    }
    flag(&mut hash, *inventory_complete);
    feed(&mut hash, focused_global);
    sep(&mut hash);
    feed(
        &mut hash,
        &action_foreground
            .as_ref()
            .map(|out| out.0.clone())
            .unwrap_or_default(),
    );
    sep(&mut hash);
    feed(
        &mut hash,
        &action_cached
            .as_ref()
            .map(|out| out.0.clone())
            .unwrap_or_default(),
    );
    sep(&mut hash);
    for output in action_outputs {
        feed(&mut hash, &output.0);
        sep(&mut hash);
    }
    flag(&mut hash, *gesture_barrier);
    if let Some(gesture) = gesture.as_ref() {
        feed(&mut hash, &gesture.window.0);
        sep(&mut hash);
        feed(&mut hash, &gesture.id.to_string());
        sep(&mut hash);
        feed(&mut hash, gesture.kind.as_str());
        sep(&mut hash);
        feed(&mut hash, gesture.phase.as_str());
        sep(&mut hash);
        feed(
            &mut hash,
            &gesture
                .cursor
                .map(|(x, y)| format!("{x},{y}"))
                .unwrap_or_default(),
        );
        sep(&mut hash);
        feed(
            &mut hash,
            &gesture
                .start_cursor
                .map(|(x, y)| format!("{x},{y}"))
                .unwrap_or_default(),
        );
        sep(&mut hash);
        flag(&mut hash, gesture.esc);
        flag(&mut hash, gesture.modal_move);
        feed(
            &mut hash,
            &format!(
                "{},{},{},{}",
                gesture.start_frame.x,
                gesture.start_frame.y,
                gesture.start_frame.w,
                gesture.start_frame.h
            ),
        );
        sep(&mut hash);
    } else {
        feed(&mut hash, "no-gesture");
        sep(&mut hash);
    }
    feed(
        &mut hash,
        &cursor.map(|(x, y)| format!("{x},{y}")).unwrap_or_default(),
    );
    sep(&mut hash);
    feed(
        &mut hash,
        &cursor_hit
            .as_ref()
            .map(|hit| hit.0.clone())
            .unwrap_or_default(),
    );
    sep(&mut hash);
    for screen in screens {
        for part in [
            screen.output.0.clone(),
            rect_part(screen.full_rect),
            rect_part(screen.work_area),
            (if screen.complete { "c" } else { "i" }).to_owned(),
            screen.scale.to_bits().to_string(),
        ] {
            feed(&mut hash, &part);
            sep(&mut hash);
        }
        for part in [
            screen.identity.manufacturer.as_deref().unwrap_or_default(),
            screen.identity.model.as_deref().unwrap_or_default(),
            screen.identity.serial.as_deref().unwrap_or_default(),
            screen.identity.connector.as_deref().unwrap_or_default(),
        ] {
            feed(&mut hash, part);
            sep(&mut hash);
        }
        flag(&mut hash, screen.identity.manufacturer.is_some());
        flag(&mut hash, screen.identity.model.is_some());
        flag(&mut hash, screen.identity.serial.is_some());
        flag(&mut hash, screen.identity.connector.is_some());
    }
    feed(
        &mut hash,
        primary_output
            .as_ref()
            .map(|out| out.0.as_str())
            .unwrap_or_default(),
    );
    sep(&mut hash);
    flag(&mut hash, primary_output.is_some());
    for held in held_screens {
        feed(&mut hash, &held.output.0);
        sep(&mut hash);
        feed(&mut hash, &held.ordinal.to_string());
        sep(&mut hash);
        for (key, value) in &held.fields {
            feed(&mut hash, key);
            sep(&mut hash);
            feed_raw(&mut hash, value);
        }
        feed_diagnostic(&mut hash, &held.diagnostic);
    }
    for held in held_windows {
        feed(&mut hash, &held.window.0);
        sep(&mut hash);
        feed(&mut hash, &held.ordinal.to_string());
        sep(&mut hash);
        for (key, value) in &held.fields {
            feed(&mut hash, key);
            sep(&mut hash);
            feed_raw(&mut hash, value);
        }
        feed(&mut hash, held.kind.as_str());
        sep(&mut hash);
        flag(&mut hash, held.diagnostic.is_some());
        if let Some(diagnostic) = held.diagnostic.as_ref() {
            feed_diagnostic(&mut hash, diagnostic);
        }
    }
    feed_window_facts(&mut hash, windows);
    for id in closed {
        feed(&mut hash, &id.0);
        sep(&mut hash);
    }
    for native in native {
        feed(&mut hash, &native.output.0);
        sep(&mut hash);
        feed(
            &mut hash,
            native.current_native.as_deref().unwrap_or_default(),
        );
        flag(&mut hash, native.current_native.is_some());
        for id in &native.native_order {
            feed(&mut hash, id);
            sep(&mut hash);
        }
        for row in &native.occupancy {
            feed(&mut hash, &row.native);
            sep(&mut hash);
            feed(
                &mut hash,
                &row.count.map(|count| count.to_string()).unwrap_or_default(),
            );
            sep(&mut hash);
            flag(&mut hash, row.count.is_some());
        }
    }
    for held in held_native {
        feed(&mut hash, "held-native");
        sep(&mut hash);
        feed(&mut hash, &held.output.0);
        sep(&mut hash);
        feed(&mut hash, &held.ordinal.to_string());
        sep(&mut hash);
        for (key, value) in &held.fields {
            feed(&mut hash, key);
            sep(&mut hash);
            feed_raw(&mut hash, value);
        }
        feed_diagnostic(&mut hash, &held.diagnostic);
    }
    feed(&mut hash, &monotonic_ms.to_string());
    sep(&mut hash);
    for attempt in frames {
        feed(&mut hash, &attempt.window.0);
        sep(&mut hash);
        feed(
            &mut hash,
            &format!(
                "{},{},{},{}",
                attempt.target.x, attempt.target.y, attempt.target.w, attempt.target.h
            ),
        );
        sep(&mut hash);
        feed(&mut hash, attempt.setter.as_str());
        sep(&mut hash);
        feed(
            &mut hash,
            &attempt
                .readback
                .map(|rect| format!("{},{},{},{}", rect.x, rect.y, rect.w, rect.h))
                .unwrap_or_default(),
        );
        sep(&mut hash);
        feed(
            &mut hash,
            &attempt
                .dpi_before
                .map(|dpi| dpi.to_string())
                .unwrap_or_default(),
        );
        sep(&mut hash);
        feed(
            &mut hash,
            &attempt
                .dpi_after
                .map(|dpi| dpi.to_string())
                .unwrap_or_default(),
        );
        sep(&mut hash);
        feed(&mut hash, &attempt.attempt_ms.to_string());
        sep(&mut hash);
        feed(
            &mut hash,
            attempt.operation_id.as_deref().unwrap_or_default(),
        );
        sep(&mut hash);
        feed(
            &mut hash,
            &attempt
                .prewrite
                .map(|rect| format!("{},{},{},{}", rect.x, rect.y, rect.w, rect.h))
                .unwrap_or_default(),
        );
        sep(&mut hash);
    }
    for attempt in member_attempts {
        feed(&mut hash, &attempt.operation_id);
        sep(&mut hash);
        feed(&mut hash, &attempt.window.0);
        sep(&mut hash);
        feed(&mut hash, &attempt.target_output.0);
        sep(&mut hash);
        feed(&mut hash, &attempt.target_workspace);
        sep(&mut hash);
        feed(&mut hash, attempt.setter.as_str());
        sep(&mut hash);
        feed(
            &mut hash,
            &attempt
                .pre
                .as_ref()
                .map(workspace_ref_fingerprint)
                .unwrap_or_default(),
        );
        sep(&mut hash);
        feed(
            &mut hash,
            &attempt
                .post
                .as_ref()
                .map(workspace_ref_fingerprint)
                .unwrap_or_default(),
        );
        sep(&mut hash);
        feed(&mut hash, &attempt.attempt_ms.to_string());
        sep(&mut hash);
    }
    for attempt in view_attempts {
        feed(&mut hash, &attempt.operation_id);
        sep(&mut hash);
        feed(&mut hash, &attempt.target_output.0);
        sep(&mut hash);
        feed(&mut hash, &attempt.target_workspace);
        sep(&mut hash);
        feed(&mut hash, attempt.setter.as_str());
        sep(&mut hash);
        feed(
            &mut hash,
            &attempt
                .pre
                .as_ref()
                .map(workspace_ref_fingerprint)
                .unwrap_or_default(),
        );
        sep(&mut hash);
        feed(
            &mut hash,
            &attempt
                .post
                .as_ref()
                .map(workspace_ref_fingerprint)
                .unwrap_or_default(),
        );
        sep(&mut hash);
        feed(
            &mut hash,
            &attempt
                .baseline
                .as_ref()
                .map(workspace_ref_fingerprint)
                .unwrap_or_default(),
        );
        sep(&mut hash);
        feed(&mut hash, &attempt.attempt_ms.to_string());
        sep(&mut hash);
    }
    for attempt in clear_attempts {
        feed(&mut hash, &attempt.operation_id);
        sep(&mut hash);
        feed(&mut hash, &attempt.window.0);
        sep(&mut hash);
        feed(&mut hash, attempt.setter.as_str());
        sep(&mut hash);
        feed(
            &mut hash,
            &attempt
                .pre_maximized
                .map(|v| if v { "1" } else { "0" }.to_owned())
                .unwrap_or_default(),
        );
        sep(&mut hash);
        feed(
            &mut hash,
            &attempt
                .post_maximized
                .map(|v| if v { "1" } else { "0" }.to_owned())
                .unwrap_or_default(),
        );
        sep(&mut hash);
        feed(&mut hash, &attempt.attempt_ms.to_string());
        sep(&mut hash);
    }
    for attempt in creation_attempts {
        feed(&mut hash, &attempt.operation_id);
        sep(&mut hash);
        feed(&mut hash, &attempt.logical_workspace);
        sep(&mut hash);
        feed(&mut hash, &attempt.target_output.0);
        sep(&mut hash);
        feed(&mut hash, attempt.setter.as_str());
        sep(&mut hash);
        for id in &attempt.order_before {
            feed(&mut hash, id);
            sep(&mut hash);
        }
        feed(&mut hash, "#before-end");
        sep(&mut hash);
        for id in &attempt.order_after {
            feed(&mut hash, id);
            sep(&mut hash);
        }
        feed(
            &mut hash,
            attempt.created_native.as_deref().unwrap_or_default(),
        );
        sep(&mut hash);
        feed(&mut hash, &attempt.attempt_ms.to_string());
        sep(&mut hash);
    }
    for held in held_attempts {
        feed_held_attempt(&mut hash, held);
    }
    for attempt in state_attempts {
        feed(&mut hash, &attempt.window.0);
        sep(&mut hash);
        feed(&mut hash, attempt.property.as_str());
        sep(&mut hash);
        flag(&mut hash, attempt.target);
        feed(&mut hash, &attempt.revision.to_string());
        sep(&mut hash);
        feed(&mut hash, attempt.setter.as_str());
        sep(&mut hash);
        for value in [attempt.pre, attempt.post] {
            feed(
                &mut hash,
                &value
                    .map(|set| if set { "1" } else { "0" }.to_owned())
                    .unwrap_or_default(),
            );
            sep(&mut hash);
            flag(&mut hash, value.is_some());
        }
        feed(&mut hash, &attempt.attempt_ms.to_string());
        sep(&mut hash);
    }
    for row in focus_feedback {
        feed(&mut hash, &row.id);
        sep(&mut hash);
        feed(&mut hash, &row.target.0);
        sep(&mut hash);
        feed(&mut hash, &row.revision.to_string());
        sep(&mut hash);
        feed(&mut hash, row.setter.as_str());
        sep(&mut hash);
        feed(&mut hash, row.reason.as_str());
        sep(&mut hash);
        for endpoint in [&row.pre, &row.post] {
            feed(
                &mut hash,
                endpoint
                    .as_ref()
                    .map(|window| window.0.as_str())
                    .unwrap_or_default(),
            );
            sep(&mut hash);
            flag(&mut hash, endpoint.is_some());
        }
        flag(&mut hash, row.foreground_proven);
        feed(&mut hash, &row.attempt_ms.to_string());
        sep(&mut hash);
        for bracket in &row.arrival {
            feed(&mut hash, &bracket.window.0);
            sep(&mut hash);
            feed(&mut hash, &bracket.obs.to_string());
            sep(&mut hash);
            feed_visibility_sample(&mut hash, &bracket.before);
            feed(&mut hash, bracket.location.kind_str());
            sep(&mut hash);
            match &bracket.location {
                ArrivalLocation::Native { home } => {
                    flag(&mut hash, home.is_some());
                    if let Some(home) = home.as_ref() {
                        feed(&mut hash, &workspace_ref_fingerprint(home));
                        sep(&mut hash);
                        flag(&mut hash, matches!(home, WorkspaceRef::Native { .. }));
                    }
                }
                ArrivalLocation::Output { out } => {
                    feed(
                        &mut hash,
                        out.as_ref().map(|id| id.0.as_str()).unwrap_or_default(),
                    );
                    sep(&mut hash);
                    flag(&mut hash, out.is_some());
                }
            }
            feed(
                &mut hash,
                &bracket
                    .frame
                    .map(|rect| format!("{},{},{},{}", rect.x, rect.y, rect.w, rect.h))
                    .unwrap_or_default(),
            );
            sep(&mut hash);
            flag(&mut hash, bracket.frame.is_some());
            feed_visibility_sample(&mut hash, &bracket.after);
        }
        feed(&mut hash, "#arrival-end");
        sep(&mut hash);
    }
    match presentation {
        PresentationRowV4::Valid(ingress) => {
            feed(&mut hash, "presentation-valid");
            sep(&mut hash);
            feed(&mut hash, &ingress.foreground);
            sep(&mut hash);
            for value in [
                ingress.foreground_minimized,
                ingress.foreground_maximized,
                ingress.foreground_fullscreen,
                ingress.foreground_host_hidden,
                ingress.foreground_hidden_workspace,
            ] {
                feed(
                    &mut hash,
                    &value.map(|set| set.to_string()).unwrap_or_default(),
                );
                sep(&mut hash);
                flag(&mut hash, value.is_some());
            }
            feed(&mut hash, &rect_part(ingress.foreground_frame));
            sep(&mut hash);
            flag(&mut hash, ingress.foreground_frame.is_some());
            flag(&mut hash, ingress.chord_held);
            feed(&mut hash, &ingress.move_window);
            sep(&mut hash);
            for window in &ingress.pager_order {
                feed(&mut hash, &window.0);
                sep(&mut hash);
            }
            for row in &ingress.pager_frames {
                feed(&mut hash, &row.window.0);
                sep(&mut hash);
                feed(&mut hash, &rect_part(row.live));
                sep(&mut hash);
                flag(&mut hash, row.live.is_some());
            }
            flag(&mut hash, ingress.witness.is_some());
            if let Some(witness) = ingress.witness.as_ref() {
                feed(&mut hash, &witness.win.0);
                sep(&mut hash);
                feed(&mut hash, &witness.obs.to_string());
                sep(&mut hash);
                feed(
                    &mut hash,
                    &format!(
                        "{},{},{},{}",
                        witness.frame.x, witness.frame.y, witness.frame.w, witness.frame.h
                    ),
                );
                sep(&mut hash);
                feed(&mut hash, &witness.foreground.0);
                sep(&mut hash);
            }
            feed(
                &mut hash,
                ingress
                    .foreground_out
                    .as_ref()
                    .map(|out| out.0.as_str())
                    .unwrap_or_default(),
            );
            sep(&mut hash);
            flag(&mut hash, ingress.foreground_out.is_some());
        }
        PresentationRowV4::Held(held) => {
            feed(&mut hash, "presentation-held");
            sep(&mut hash);
            for (key, value) in &held.fields {
                feed(&mut hash, key);
                sep(&mut hash);
                feed_raw(&mut hash, value);
            }
            feed(&mut hash, held.kind.as_str());
            sep(&mut hash);
            flag(&mut hash, held.diagnostic.is_some());
            if let Some(diagnostic) = held.diagnostic.as_ref() {
                feed_diagnostic(&mut hash, diagnostic);
            }
        }
    }
    for record in intent_records {
        feed(&mut hash, &record.win.0);
        sep(&mut hash);
        feed(&mut hash, &record.data);
        sep(&mut hash);
    }
    match renderer_input {
        None => {
            feed(&mut hash, "no-renderer");
            sep(&mut hash);
        }
        Some(input) => {
            feed(&mut hash, &input.rev.to_string());
            sep(&mut hash);
            match &input.target {
                RenderTarget::Workspace { out, ws } => {
                    feed(&mut hash, "renderer-workspace");
                    sep(&mut hash);
                    feed(&mut hash, &out.0);
                    sep(&mut hash);
                    feed(&mut hash, &ws.0);
                    sep(&mut hash);
                }
                RenderTarget::Window { out, ws, win } => {
                    feed(&mut hash, "renderer-window");
                    sep(&mut hash);
                    feed(&mut hash, &out.0);
                    sep(&mut hash);
                    feed(&mut hash, &ws.0);
                    sep(&mut hash);
                    feed(&mut hash, &win.0);
                    sep(&mut hash);
                }
                RenderTarget::Group { out, ws, group } => {
                    feed(&mut hash, "renderer-group");
                    sep(&mut hash);
                    feed(&mut hash, &out.0);
                    sep(&mut hash);
                    feed(&mut hash, &ws.0);
                    sep(&mut hash);
                    feed(&mut hash, &group.0);
                    sep(&mut hash);
                }
            }
        }
    }
    hash
}

fn feed_window_facts(hash: &mut u64, facts: &[WindowFact]) {
    fn feed(hash: &mut u64, text: &str) {
        for byte in text.bytes() {
            *hash ^= u64::from(byte);
            *hash = hash.wrapping_mul(1099511628211);
        }
    }
    fn sep(hash: &mut u64) {
        *hash ^= 0xff;
        *hash = hash.wrapping_mul(1099511628211);
    }
    fn flag(hash: &mut u64, set: bool) {
        feed(hash, if set { "1" } else { "0" });
        sep(hash);
    }
    fn rect_part(rect: Option<Rect>) -> String {
        rect.map(|rect| format!("{},{},{},{}", rect.x, rect.y, rect.w, rect.h))
            .unwrap_or_default()
    }
    for fact in facts {
        for part in [
            fact.window.0.clone(),
            fact.output
                .as_ref()
                .map(|out| out.0.clone())
                .unwrap_or_default(),
            rect_part(fact.rect),
        ] {
            feed(hash, &part);
            sep(hash);
        }
        flag(hash, fact.output.is_some());
        for value in [fact.fullscreen, fact.maximized, fact.sticky, fact.minimized] {
            feed(hash, &value.map(|set| set.to_string()).unwrap_or_default());
            sep(hash);
            flag(hash, value.is_some());
        }
        feed(hash, fact.native_workspace.as_deref().unwrap_or_default());
        sep(hash);
        flag(hash, fact.native_workspace.as_deref().is_some());
        flag(
            hash,
            !matches!(
                fact.native_workspace,
                crate::world_report::NativeWorkspaceFact::Omitted
            ),
        );
        flag(
            hash,
            matches!(
                fact.native_workspace,
                crate::world_report::NativeWorkspaceFact::Value(_)
            ),
        );
        for value in [
            fact.host_hidden,
            fact.privileged,
            fact.managed,
            fact.accepts_input,
            fact.activation_inhibited,
            fact.tool_window,
            fact.owned,
            fact.transient,
            fact.modal,
            fact.resizable,
            fact.above,
            fact.below,
        ] {
            feed(hash, &value.map(|set| set.to_string()).unwrap_or_default());
            sep(hash);
            flag(hash, value.is_some());
        }
        for id in [&fact.owner, &fact.parent] {
            feed(
                hash,
                id.as_ref()
                    .map(|window| window.0.as_str())
                    .unwrap_or_default(),
            );
            sep(hash);
            flag(hash, id.is_some());
        }
        for text in [
            fact.role.as_deref(),
            fact.app_id.as_deref(),
            fact.executable.as_deref(),
            fact.process_id.as_deref(),
            fact.class.as_deref(),
            fact.title.as_deref(),
        ] {
            feed(hash, text.unwrap_or_default());
            sep(hash);
            flag(hash, text.is_some());
        }
        feed(
            hash,
            fact.window_type
                .map(|kind| kind.as_str())
                .unwrap_or_default(),
        );
        sep(hash);
        flag(hash, fact.window_type.is_some());
        feed(
            hash,
            fact.activation_policy
                .map(|policy| policy.as_str())
                .unwrap_or_default(),
        );
        sep(hash);
        flag(hash, fact.activation_policy.is_some());
        feed(
            hash,
            &fact
                .layer
                .map(|layer| layer.to_string())
                .unwrap_or_default(),
        );
        sep(hash);
        flag(hash, fact.layer.is_some());
        feed(
            hash,
            fact.fullscreen_owner
                .map(|owner| owner.as_str())
                .unwrap_or_default(),
        );
        sep(hash);
        flag(hash, fact.fullscreen_owner.is_some());
        for value in [fact.fullscreen_exitable, fact.suppression_available] {
            feed(hash, &value.map(|set| set.to_string()).unwrap_or_default());
            sep(hash);
            flag(hash, value.is_some());
        }
        for value in [fact.visibility.visible, fact.visibility.suppression_owned] {
            feed(hash, &value.map(|set| set.to_string()).unwrap_or_default());
            sep(hash);
            flag(hash, value.is_some());
        }
        match fact.last_write.as_ref() {
            None => {
                feed(hash, "no-last-write");
                sep(hash);
            }
            Some(last) => {
                feed(hash, last.property.as_str());
                sep(hash);
                flag(hash, last.target);
                feed(hash, &last.revision.to_string());
                sep(hash);
                feed(hash, last.path.as_str());
                sep(hash);
                feed(hash, last.setter.as_str());
                sep(hash);
                feed(hash, last.reason.as_str());
                sep(hash);
                for sample in [last.pre.as_ref(), last.post.as_ref()] {
                    match sample {
                        None => {
                            feed(hash, "no-sample");
                            sep(hash);
                        }
                        Some(sample) => {
                            feed(hash, &sample.obs.to_string());
                            sep(hash);
                            for value in [
                                sample.lifetime_matched,
                                sample.visibility.visible,
                                sample.visibility.suppression_owned,
                                sample.minimized,
                                sample.host_hidden,
                            ] {
                                feed(hash, &value.map(|set| set.to_string()).unwrap_or_default());
                                sep(hash);
                                flag(hash, value.is_some());
                            }
                        }
                    }
                }
            }
        }
        flag(hash, fact.last_write.is_some());
        flag(hash, fact.hints.is_some());
        let hints = fact
            .hints
            .unwrap_or(crate::size_hints::WindowSizeHints::none());
        for bound in [hints.min_w, hints.min_h, hints.max_w, hints.max_h] {
            feed(hash, &bound.map(|v| v.to_string()).unwrap_or_default());
            sep(hash);
        }
    }
}

fn feed_held_attempt(hash: &mut u64, held: &HeldAttemptRow) {
    fn feed(hash: &mut u64, text: &str) {
        for byte in text.bytes() {
            *hash ^= u64::from(byte);
            *hash = hash.wrapping_mul(1099511628211);
        }
    }
    fn sep(hash: &mut u64) {
        *hash ^= 0xff;
        *hash = hash.wrapping_mul(1099511628211);
    }
    fn flag(hash: &mut u64, set: bool) {
        feed(hash, if set { "1" } else { "0" });
        sep(hash);
    }
    fn feed_raw(hash: &mut u64, value: &crate::connection::RawSettingValue) {
        match value {
            crate::connection::RawSettingValue::Null => feed(hash, "null"),
            crate::connection::RawSettingValue::Bool(set) => {
                feed(hash, if *set { "true" } else { "false" });
            }
            crate::connection::RawSettingValue::Number(number) => {
                feed(hash, &number.to_bits().to_string());
            }
            crate::connection::RawSettingValue::String(text) => feed(hash, text),
            crate::connection::RawSettingValue::Array(items) => {
                for item in items {
                    feed_raw(hash, item);
                    sep(hash);
                }
            }
            crate::connection::RawSettingValue::Object(entries) => {
                for (key, item) in entries {
                    feed(hash, key);
                    sep(hash);
                    feed_raw(hash, item);
                    sep(hash);
                }
            }
        }
        sep(hash);
    }
    feed(hash, held.lane.as_str());
    sep(hash);
    feed(hash, &held.ordinal.to_string());
    sep(hash);
    for identity in &held.identity {
        feed(hash, identity);
        sep(hash);
    }
    feed(hash, "#identity-end");
    sep(hash);
    for (key, value) in &held.fields {
        feed(hash, key);
        sep(hash);
        feed_raw(hash, value);
    }
    feed(hash, held.kind.as_str());
    sep(hash);
    flag(hash, held.diagnostic.is_some());
    if let Some(diagnostic) = held.diagnostic.as_ref() {
        feed(hash, diagnostic.kind.as_str());
        sep(hash);
        feed(hash, diagnostic.row.as_str());
        sep(hash);
        feed(hash, diagnostic.id.as_deref().unwrap_or_default());
        sep(hash);
        match &diagnostic.scope {
            AuthorityScope::Output(output) => {
                feed(hash, "output");
                sep(hash);
                feed(hash, &output.0);
                sep(hash);
            }
            AuthorityScope::Workspace { output, workspace } => {
                feed(hash, "workspace");
                sep(hash);
                feed(hash, &output.0);
                sep(hash);
                feed(hash, &workspace.0);
                sep(hash);
            }
        }
        flag(hash, diagnostic.id.is_some());
    }
}

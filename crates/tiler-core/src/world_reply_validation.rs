//! Shared typed v4 reply semantic and host-capability validation.
//!
//! Validates a borrowed [`WorldReplyV4`] and optional [`HostCapabilities`]
//! using the fixed v4 protocol vocabulary without JSON serialization or
//! payload echoing.

use std::collections::BTreeSet;

use crate::bounds::{is_opaque_id, valid_carried_point, valid_carried_rect};
use crate::connection::HostCapabilities;
use crate::geometry::Rect;
use crate::ids::{is_correlation_id, is_generation_id};
use crate::intent_data::{MAX_INTENT_DATA_BYTES, encode_intent_data};
use crate::world_presentation::{PagerScreen, PresentationUnderlay};
use crate::world_report::{
    AfterShown, AuthorityScope, BorderPresentationV4, CommandRejectKind, DragPreviewV4,
    FocusRequest, FocusRequestCause, FrameWitness, IntentDesire, MAX_FOCUS_DEPARTING,
    MAX_FOCUS_MEMBERS, MAX_FOCUS_SELECTED, MAX_FOCUS_VIEWS, MAX_INTENT_DESIRED,
    MAX_REPLY_DIAGNOSTICS, MAX_REPLY_NATIVE_CREATIONS, MAX_REPLY_OPERATIONS, MAX_REPLY_OUTPUTS,
    MAX_REPLY_SCOPES, MAX_REPLY_SWITCH_EVENTS, MAX_REPLY_TRANSFERS, MAX_REPLY_WINDOWS,
    MAX_REPLY_WORKSPACES, MAX_STATE_TARGETS_PER_WINDOW, NativeCreationDesire, OperationEvent,
    OutputDesire, RowDiagnostic, ScopeStatus, StateProperty, SwitchEvent, TransferKind,
    TransferTargetV4, WORLD_CONTRACT_VERSION_V4, WindowDesire, WorkspaceDesire,
    WorldPresentationV4, WorldReplyV4,
};

const KIND_UNKNOWN_VALUE: &str = "unknown-value";
const MSG_UNKNOWN_VALUE: &str = "request contains an unknown value";
const KIND_VERSION: &str = "unsupported-version";
const MSG_VERSION: &str = "unsupported contract version";
const KIND_SNAPSHOT: &str = "snapshot-invalid";

/// Fixed-token semantic or host-capability reply validation failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReplyValidationError {
    pub kind: &'static str,
    pub message: &'static str,
}

impl ReplyValidationError {
    const fn unknown_value() -> Self {
        Self {
            kind: KIND_UNKNOWN_VALUE,
            message: MSG_UNKNOWN_VALUE,
        }
    }

    const fn snapshot(message: &'static str) -> Self {
        Self {
            kind: KIND_SNAPSHOT,
            message,
        }
    }

    const fn version() -> Self {
        Self {
            kind: KIND_VERSION,
            message: MSG_VERSION,
        }
    }
}

type Result<T> = std::result::Result<T, ReplyValidationError>;

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

fn check_cap(len: usize, max: usize) -> Result<()> {
    if len > max {
        return Err(ReplyValidationError::snapshot("section-over-cap"));
    }
    Ok(())
}

fn check_id(text: &str) -> Result<()> {
    if !is_opaque_id(text) {
        return Err(ReplyValidationError::unknown_value());
    }
    Ok(())
}

fn check_rect(rect: &Rect) -> Result<()> {
    if !valid_carried_rect(rect.x, rect.y, rect.w, rect.h) {
        return Err(ReplyValidationError::unknown_value());
    }
    Ok(())
}

fn check_authority_scope(scope: &AuthorityScope) -> Result<()> {
    match scope {
        AuthorityScope::Output(output) => check_id(&output.0),
        AuthorityScope::Workspace { output, workspace } => {
            check_id(&output.0)?;
            check_id(&workspace.0)
        }
    }
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

fn validate_scope_status(row: &ScopeStatus) -> Result<()> {
    check_authority_scope(&row.scope)
}

fn validate_workspace_desire(row: &WorkspaceDesire) -> Result<()> {
    check_id(&row.id)?;
    check_id(&row.output.0)?;
    if let Some(bind) = row.native_binding.as_deref() {
        check_id(bind)?;
    }
    check_authority_scope(&row.scope)
}

fn validate_output_desire(row: &OutputDesire) -> Result<()> {
    check_id(&row.output.0)?;
    if let Some(view) = row.logical_view.as_deref() {
        check_id(view)?;
    }
    Ok(())
}

/// `include_ops` is false for the renderer projection, which never carries
/// `frame_operation` or automatic-clear state targets.
fn validate_window_desire(
    row: &WindowDesire,
    host: Option<&HostCapabilities>,
    include_ops: bool,
) -> Result<()> {
    check_id(&row.lifetime.0)?;
    check_id(&row.output.0)?;
    check_id(&row.workspace)?;
    if let Some(rect) = row.allocation.as_ref() {
        check_rect(rect)?;
    }
    if let Some(rect) = row.effective_frame.as_ref() {
        check_rect(rect)?;
    }
    if include_ops && let Some(op) = row.frame_operation.as_deref() {
        check_id(op)?;
    }
    if let Some(visibility) = row.visibility.as_ref()
        && visibility.shown
        && visibility.fallback
    {
        return Err(ReplyValidationError::snapshot("visibility-show-fallback"));
    }
    if include_ops && row.state.len() > MAX_STATE_TARGETS_PER_WINDOW {
        return Err(ReplyValidationError::snapshot("state-target-count"));
    }
    let mut seen_state = BTreeSet::new();
    for target in row
        .state
        .iter()
        .filter(|target| include_ops || !target.automatic_clear)
    {
        if !seen_state.insert(target.property)
            || (!include_ops && seen_state.len() > MAX_STATE_TARGETS_PER_WINDOW)
        {
            return Err(ReplyValidationError::snapshot("state-target-count"));
        }
        if let Some(host) = host {
            if !host.above && target.property == StateProperty::Above {
                return Err(ReplyValidationError::snapshot("above-capability-false"));
            }
            if !host.native_sticky && target.property == StateProperty::Sticky {
                return Err(ReplyValidationError::snapshot("sticky-capability-false"));
            }
        }
    }
    Ok(())
}

fn validate_transfer(row: &TransferTargetV4) -> Result<()> {
    check_id(&row.operation_id)?;
    check_id(&row.target_output.0)?;
    check_id(&row.target_workspace)?;
    if let Some(parent) = row.parent.as_deref() {
        check_id(parent)?;
    }
    match &row.kind {
        TransferKind::Window(window) => check_id(&window.0),
        TransferKind::View | TransferKind::Migration => Ok(()),
    }
}

fn validate_after_shown_shapes(shown: &AfterShown) -> Result<()> {
    if shown.members.len() > MAX_FOCUS_MEMBERS {
        return Err(ReplyValidationError::snapshot("focus-members-over-cap"));
    }
    if shown.views.len() > MAX_FOCUS_VIEWS {
        return Err(ReplyValidationError::snapshot("focus-views-over-cap"));
    }
    if shown.selected.len() > MAX_FOCUS_SELECTED {
        return Err(ReplyValidationError::snapshot("focus-selected-over-cap"));
    }
    if shown.departing.len() > MAX_FOCUS_DEPARTING {
        return Err(ReplyValidationError::snapshot("focus-departing-over-cap"));
    }
    check_id(&shown.op)?;
    for member in &shown.members {
        check_id(&member.win.0)?;
        check_id(&member.out.0)?;
        check_id(&member.ws)?;
        if let Some(frame) = member.frame.as_ref() {
            check_rect(frame)?;
        }
    }
    for scope in &shown.views {
        check_id(&scope.out.0)?;
        check_id(&scope.ws)?;
    }
    for scope in &shown.selected {
        check_id(&scope.out.0)?;
        check_id(&scope.ws)?;
    }
    for scope in &shown.departing {
        check_id(&scope.out.0)?;
        check_id(&scope.ws)?;
    }
    Ok(())
}

fn check_after_shown_invariants(shown: &AfterShown) -> Result<()> {
    let mut seen_members = BTreeSet::new();
    for member in &shown.members {
        if !seen_members.insert(member.win.0.as_str()) {
            return Err(ReplyValidationError::snapshot("encode-mismatch"));
        }
    }
    let mut seen_view_outs = BTreeSet::new();
    for scope in &shown.views {
        if !seen_view_outs.insert(scope.out.0.as_str()) {
            return Err(ReplyValidationError::snapshot("encode-mismatch"));
        }
    }
    let mut seen_selected = BTreeSet::new();
    for scope in &shown.selected {
        if !seen_selected.insert((scope.out.0.as_str(), scope.ws.as_str())) {
            return Err(ReplyValidationError::snapshot("encode-mismatch"));
        }
    }
    for view in &shown.views {
        if !seen_selected.contains(&(view.out.0.as_str(), view.ws.as_str())) {
            return Err(ReplyValidationError::snapshot("encode-mismatch"));
        }
    }
    let mut seen_departing = BTreeSet::new();
    for scope in &shown.departing {
        if !seen_departing.insert((scope.out.0.as_str(), scope.ws.as_str())) {
            return Err(ReplyValidationError::snapshot("encode-mismatch"));
        }
    }
    Ok(())
}

fn validate_focus_request(request: &FocusRequest) -> Result<()> {
    check_id(&request.id)?;
    check_id(&request.target.0)?;
    check_id(&request.output.0)?;
    check_id(&request.workspace)?;
    let explicit = request.cause == FocusRequestCause::ExplicitCommand;
    if explicit != request.cmd_seq.is_some() {
        return Err(ReplyValidationError::snapshot(
            "focus-request-cmdseq-mismatch",
        ));
    }
    if request.after_shown.is_some() && !explicit {
        return Err(ReplyValidationError::snapshot(
            "focus-request-aftershown-non-explicit",
        ));
    }
    if let Some(shown) = request.after_shown.as_ref() {
        validate_after_shown_shapes(shown)?;
    }
    Ok(())
}

fn validate_operation_event(row: &OperationEvent) -> Result<()> {
    check_id(&row.window.0)?;
    check_id(&row.operation_id)?;
    if row.outcome.kind() != row.kind {
        return Err(ReplyValidationError::unknown_value());
    }
    if let Some(cause) = row.cause.as_deref()
        && !valid_operation_cause(cause)
    {
        return Err(ReplyValidationError::unknown_value());
    }
    Ok(())
}

fn validate_native_creation(row: &NativeCreationDesire) -> Result<()> {
    check_id(&row.operation_id)?;
    check_id(&row.logical_workspace)?;
    check_id(&row.target_output.0)
}

fn validate_drag_preview(preview: &DragPreviewV4) -> Result<()> {
    check_rect(&preview.rect)?;
    check_id(&preview.domain.output.0)?;
    check_id(&preview.domain.workspace.0)?;
    check_id(&preview.source.output.0)?;
    check_id(&preview.source.workspace.0)?;
    check_id(&preview.target.0)?;
    if let Some(hover) = &preview.hover {
        check_id(&hover.domain.output.0)?;
        check_id(&hover.domain.workspace.0)?;
        check_id(&hover.source_leaf.0)?;
        check_id(&hover.source_window.0)?;
        if let Some(prior) = hover.prior.as_ref() {
            check_id(&prior.group.0)?;
        }
        check_id(&hover.prior_domain.output.0)?;
        check_id(&hover.prior_domain.workspace.0)?;
    }
    Ok(())
}

fn validate_intent_desire(row: &IntentDesire) -> Result<()> {
    check_id(&row.win.0)?;
    if let Some(data) = row.data.as_ref()
        && encode_intent_data(data).len() > MAX_INTENT_DATA_BYTES
    {
        return Err(ReplyValidationError::snapshot("intent-data-invalid"));
    }
    Ok(())
}

fn validate_switch_event(row: &SwitchEvent) -> Result<()> {
    check_id(&row.output.0)?;
    check_id(&row.operation_id)?;
    check_cap(row.failed.len(), MAX_REPLY_WINDOWS)?;
    for window in &row.failed {
        check_id(&window.0)?;
    }
    Ok(())
}

fn validate_row_diagnostic(row: &RowDiagnostic) -> Result<()> {
    if let Some(id) = row.id.as_deref() {
        check_id(id)?;
    }
    check_authority_scope(&row.scope)
}

fn validate_frame_witness(witness: &FrameWitness) -> Result<()> {
    check_id(&witness.win.0)?;
    check_rect(&witness.frame)?;
    check_id(&witness.foreground.0)
}

fn validate_border_presentation(row: &BorderPresentationV4) -> Result<()> {
    if row.visible && row.reason.is_some() {
        return Err(ReplyValidationError::unknown_value());
    }
    if let Some(subject) = row.subject.as_ref() {
        check_id(&subject.0)?;
    }
    if let Some(width) = row.width
        && width < 0
    {
        return Err(ReplyValidationError::unknown_value());
    }
    if let Some(radius) = row.radius
        && radius < 0
    {
        return Err(ReplyValidationError::unknown_value());
    }
    if let Some(witness) = row.witness.as_ref() {
        validate_frame_witness(witness)?;
    }
    Ok(())
}

fn validate_underlay(underlay: &PresentationUnderlay) -> Result<()> {
    if let PresentationUnderlay::Found(found) = underlay {
        check_id(&found.home.output.0)?;
        check_id(&found.home.workspace.0)?;
        check_id(&found.group.0)?;
        check_id(&found.focused_leaf.0)?;
        check_id(&found.focused_window.0)?;
        check_rect(&found.union_rect)?;
        check_rect(&found.outer_rect)?;
        check_cap(found.members.len(), MAX_REPLY_WINDOWS)?;
        for member in &found.members {
            check_id(&member.leaf.0)?;
            check_id(&member.window.0)?;
            check_rect(&member.rect)?;
        }
    }
    Ok(())
}

fn validate_pager(pager: &[PagerScreen]) -> Result<()> {
    check_cap(pager.len(), MAX_REPLY_OUTPUTS)?;
    for screen in pager {
        check_id(&screen.output)?;
        check_cap(screen.workspaces.len(), MAX_REPLY_WORKSPACES)?;
        for workspace in &screen.workspaces {
            check_id(&workspace.id)?;
            check_cap(workspace.windows.len(), MAX_REPLY_WINDOWS)?;
            for window in &workspace.windows {
                check_id(&window.window.0)?;
                check_rect(&window.frame)?;
            }
        }
    }
    Ok(())
}

fn validate_presentation_rows(
    presentation: &WorldPresentationV4,
    include_suspension: bool,
) -> Result<()> {
    if !presentation.foreground.is_empty() && !is_opaque_id(&presentation.foreground) {
        return Err(ReplyValidationError::unknown_value());
    }
    if include_suspension && let Some(output) = presentation.suspended_output.as_ref() {
        check_id(&output.0)?;
    }
    validate_border_presentation(&presentation.border)?;
    validate_underlay(&presentation.underlay)
}

fn validate_presentation(presentation: &WorldPresentationV4) -> Result<()> {
    validate_presentation_rows(presentation, true)?;
    validate_pager(&presentation.pager)
}

/// Validate one typed v4 reply against all semantic rules and optional host
/// capabilities.
pub fn validate_reply_v4(reply: &WorldReplyV4, host: Option<&HostCapabilities>) -> Result<()> {
    if reply.contract_version != WORLD_CONTRACT_VERSION_V4 {
        return Err(ReplyValidationError::version());
    }
    if !is_opaque_id(&reply.adapter_incarnation) {
        return Err(ReplyValidationError::unknown_value());
    }
    if !is_generation_id(&reply.core_generation) {
        return Err(ReplyValidationError::unknown_value());
    }
    if !is_correlation_id(&reply.request_id) {
        return Err(ReplyValidationError::unknown_value());
    }
    check_cap(reply.scopes.len(), MAX_REPLY_SCOPES)?;
    check_cap(reply.workspaces.len(), MAX_REPLY_WORKSPACES)?;
    check_cap(reply.outputs.len(), MAX_REPLY_OUTPUTS)?;
    check_cap(reply.windows.len(), MAX_REPLY_WINDOWS)?;
    check_cap(reply.transfers.len(), MAX_REPLY_TRANSFERS)?;
    check_cap(reply.operation_settled.len(), MAX_REPLY_OPERATIONS)?;
    check_cap(reply.native_creations.len(), MAX_REPLY_NATIVE_CREATIONS)?;
    check_cap(reply.intent_desired.len(), MAX_INTENT_DESIRED)?;
    check_cap(reply.switch_settled.len(), MAX_REPLY_SWITCH_EVENTS)?;
    check_cap(reply.diagnostics.len(), MAX_REPLY_DIAGNOSTICS)?;

    let mut seen_scopes = BTreeSet::new();
    for scope in &reply.scopes {
        if !seen_scopes.insert(scope_status_key(&scope.scope)) {
            return Err(ReplyValidationError::snapshot("duplicate-scope"));
        }
        validate_scope_status(scope)?;
    }
    let mut seen_workspaces = BTreeSet::new();
    for workspace in &reply.workspaces {
        if !seen_workspaces.insert((workspace.output.0.as_str(), workspace.id.as_str())) {
            return Err(ReplyValidationError::snapshot("duplicate-workspace"));
        }
        validate_workspace_desire(workspace)?;
    }
    let mut seen_outputs = BTreeSet::new();
    for output in &reply.outputs {
        if !seen_outputs.insert(output.output.0.as_str()) {
            return Err(ReplyValidationError::snapshot("duplicate-output"));
        }
        validate_output_desire(output)?;
    }
    let mut seen_windows = BTreeSet::new();
    for window in &reply.windows {
        if !seen_windows.insert(window.lifetime.0.as_str()) {
            return Err(ReplyValidationError::snapshot("duplicate-window"));
        }
        validate_window_desire(window, host, true)?;
    }
    let mut seen_transfers = BTreeSet::new();
    for transfer in &reply.transfers {
        if !seen_transfers.insert(transfer.operation_id.as_str()) {
            return Err(ReplyValidationError::snapshot("duplicate-transfer"));
        }
        validate_transfer(transfer)?;
    }
    let mut seen_operations = BTreeSet::new();
    for event in &reply.operation_settled {
        if !seen_operations.insert((&event.window, event.kind)) {
            return Err(ReplyValidationError::snapshot("duplicate-operation"));
        }
        validate_operation_event(event)?;
    }
    let mut seen_creations = BTreeSet::new();
    for creation in &reply.native_creations {
        if !seen_creations.insert(creation.operation_id.as_str()) {
            return Err(ReplyValidationError::snapshot("duplicate-native-creation"));
        }
        validate_native_creation(creation)?;
    }
    let mut seen_writes = BTreeSet::new();
    for write in &reply.intent_desired {
        if !seen_writes.insert(write.win.0.as_str()) {
            return Err(ReplyValidationError::snapshot("duplicate-intent-desire"));
        }
        validate_intent_desire(write)?;
    }
    let mut seen_switches = BTreeSet::new();
    for event in &reply.switch_settled {
        if !seen_switches.insert(&event.output) {
            return Err(ReplyValidationError::snapshot("duplicate-switch"));
        }
        validate_switch_event(event)?;
    }
    if let Some((x, y)) = reply.cursor_warp {
        if !valid_carried_point(x, y) {
            return Err(ReplyValidationError::unknown_value());
        }
        if let Some(host) = host
            && !host.warp
        {
            return Err(ReplyValidationError::snapshot("warp-capability-false"));
        }
    }
    if let Some(request) = reply.focus_request.as_ref() {
        validate_focus_request(request)?;
    }
    if let Some(preview) = reply.drag_preview.as_ref() {
        validate_drag_preview(preview)?;
    }
    for diag in &reply.diagnostics {
        validate_row_diagnostic(diag)?;
    }
    validate_presentation(&reply.presentation)?;
    if let Some(request) = reply.focus_request.as_ref()
        && let Some(shown) = request.after_shown.as_ref()
    {
        check_after_shown_invariants(shown)?;
    }
    Ok(())
}

/// Validate the subset of typed v4 reply fields emitted in a renderer
/// projection.
pub fn validate_projection_reply_v4(reply: &WorldReplyV4) -> Result<()> {
    if reply.contract_version != WORLD_CONTRACT_VERSION_V4 {
        return Err(ReplyValidationError::version());
    }
    for window in &reply.windows {
        validate_window_desire(window, None, false)?;
    }
    validate_presentation_rows(&reply.presentation, false)?;
    if let Some(preview) = reply.drag_preview.as_ref()
        && !reply
            .presentation
            .suspended_output
            .as_ref()
            .is_some_and(|suspended| *suspended == preview.domain.output)
    {
        check_rect(&preview.rect)?;
        check_id(&preview.source.output.0)?;
        check_id(&preview.source.workspace.0)?;
        check_id(&preview.target.0)?;
    }
    Ok(())
}

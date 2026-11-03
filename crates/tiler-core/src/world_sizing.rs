//! World reply/request sizing and canonical encoding (D1-D5).
//!
//! Split from [`crate::world_report`]: exact JSON string/numeric widths,
//! the structural request/reply estimators, the conservative pre-mutation
//! reply upper bound, and the canonical reply writer that the transactional
//! commit gate measures. Shapes, validation, and freshness stay in
//! [`crate::world_report`], re-exported here for one stable path.

use crate::boundary::CoreCommand;
use crate::directional::Direction;
use crate::gesture::GestureSettled;
use crate::world_report::{
    AuthorityScope, CommandResult, FloatOrigin, HoldReason, TransferKind, WorldCommand, WorldReply,
    WorldReport,
};

/// JSON string byte length for the canonical codec: two quotes plus the
/// escaped content. Escaping is derived from the JSON rules (`"` and `\`
/// take two bytes, C0 controls take six as `\u00XX`), never from an
/// asserted constant. Validated opaque ids (`[A-Za-z0-9-_.]`) never escape,
/// so their encoded length equals their raw length; this helper stays
/// exact for every other string as well.
#[must_use]
pub fn json_string_len(value: &str) -> usize {
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

fn authority_scope_len(scope: &AuthorityScope) -> usize {
    match scope {
        AuthorityScope::Output(out) => {
            "{\"kind\":\"output\",\"out\":}".len() + json_string_len(&out.0)
        }
        AuthorityScope::Workspace { output, workspace } => {
            "{\"kind\":\"workspace\",\"out\":,\"ws\":}".len()
                + json_string_len(&output.0)
                + json_string_len(&workspace.0)
        }
    }
}

fn workspace_ref_len(reference: &crate::world_report::WorkspaceRef) -> usize {
    match reference {
        crate::world_report::WorkspaceRef::Logical { output, workspace } => {
            "{\"kind\":\"logical\",\"out\":,\"ws\":}".len()
                + json_string_len(&output.0)
                + json_string_len(&workspace.0)
        }
        crate::world_report::WorkspaceRef::Native { output, workspace } => {
            "{\"kind\":\"native\",\"out\":,\"ws\":}".len()
                + json_string_len(&output.0)
                + json_string_len(workspace.as_str())
        }
    }
}

fn workspace_ref_option_len(reference: &Option<crate::world_report::WorkspaceRef>) -> usize {
    reference.as_ref().map_or(4, workspace_ref_len)
}

/// Bound on one canonical binary64 spelling: the longest finite expansion
/// is the smallest subnormal (`5e-324`: `0.` plus 323 zeros plus one
/// digit, 327 bytes) and the longest large expansion is `f64::MAX` (309
/// digits); 350 covers every finite spelling with margin. Screen scales
/// ride this bound in the request estimator so the adapter's compact
/// canonical encoding always fits.
pub const MAX_CANONICAL_F64_LEN: usize = 350;

fn row_diagnostic_estimate(diagnostic: &crate::world_report::RowDiagnostic) -> usize {
    // {"scope":,"row":,"id":,"kind":} = 30 outer punctuation. A missing
    // id encodes as JSON null (4), never the empty string.
    30 + authority_scope_len(&diagnostic.scope)
        + json_string_len(diagnostic.row.as_str())
        + diagnostic.id.as_deref().map_or(4, json_string_len)
        + json_string_len(diagnostic.kind.as_str())
}

fn scope_status_estimate(scope: &crate::world_report::ScopeStatus) -> usize {
    // {"scope":,"auth":,"reason":} = 28 outer punctuation.
    28 + authority_scope_len(&scope.scope)
        + json_string_len(scope.authority.as_str())
        + json_string_len(scope.authority.hold_reason().map_or("", HoldReason::as_str))
}

/// Structural size estimate for the frozen outbound target suffix after
/// `clear_op`: `,"cmdSeq":null|"N"` (10 plus 4 or 2+digits),
/// `,"visibility":null|{...}` (14 plus 4 or 31+bools+digits),
/// `,"visibility_auth":S` (19 plus token), `,"visibility_hold":S` (19 plus
/// token), `,"state":[...]` (11 plus rows). Exact: mirrors
/// [`encode_window_target_lanes`] byte-for-byte.
#[must_use]
pub fn window_target_lanes_estimate(window: &crate::world_report::WindowDesire) -> usize {
    let mut bytes = 10 + window.command_seq.map_or(4, |seq| 2 + u64_len(seq));
    bytes += 14
        + match &window.visibility {
            None => 4,
            Some(target) => {
                // {"shown":B,"rev":"N","fallback":B} = 31 fixed (braces,
                // keys+colons, commas, rev quotes) plus values.
                31 + bool_len(target.shown) + u64_len(target.revision) + bool_len(target.fallback)
            }
        };
    bytes += 19 + json_string_len(window.visibility_authority.as_str());
    bytes += 19
        + json_string_len(
            window
                .visibility_authority
                .hold_reason()
                .map_or("", HoldReason::as_str),
        );
    // ,"state":[] = 11; each row {"property":S,"value":B,"auth":S,
    // "reason":S,"cmdSeq":null|"N"} = 50 fixed plus values.
    bytes += 11;
    for target in &window.state {
        bytes += 50
            + json_string_len(target.property.as_str())
            + bool_len(target.value)
            + json_string_len(target.authority.as_str())
            + json_string_len(
                target
                    .authority
                    .hold_reason()
                    .map_or("", HoldReason::as_str),
            )
            + target.command_seq.map_or(4, |seq| 2 + u64_len(seq));
    }
    bytes += window.state.len().saturating_sub(1);
    bytes
}

/// Exact canonical encoding for the frozen outbound target suffix after
/// `clear_op`, with frozen key order (mirrored by
/// [`window_target_lanes_estimate`] and the protocol codec):
/// `,"cmdSeq":null|"N","visibility":null|{"shown":B,"rev":"N",
/// "fallback":B},"visibility_auth":S,"visibility_hold":S,"state":[...]` where
/// each state row is `{"property":S,"value":B,"auth":S,"reason":S,
/// "cmdSeq":null|"N"}`. Counters encode as quoted decimal strings, never
/// JSON numbers.
pub fn encode_window_target_lanes(out: &mut String, window: &crate::world_report::WindowDesire) {
    out.push_str(",\"cmdSeq\":");
    match window.command_seq {
        None => out.push_str("null"),
        Some(seq) => {
            out.push('"');
            out.push_str(&seq.to_string());
            out.push('"');
        }
    }
    out.push_str(",\"visibility\":");
    match &window.visibility {
        None => out.push_str("null"),
        Some(target) => {
            out.push_str("{\"shown\":");
            out.push_str(if target.shown { "true" } else { "false" });
            out.push_str(",\"rev\":\"");
            out.push_str(&target.revision.to_string());
            out.push_str("\",\"fallback\":");
            out.push_str(if target.fallback { "true" } else { "false" });
            out.push('}');
        }
    }
    out.push_str(",\"visibility_auth\":");
    push_json_string(out, window.visibility_authority.as_str());
    out.push_str(",\"visibility_hold\":");
    push_json_string(
        out,
        window
            .visibility_authority
            .hold_reason()
            .map_or("", HoldReason::as_str),
    );
    out.push_str(",\"state\":[");
    for (i, target) in window.state.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str("{\"property\":");
        push_json_string(out, target.property.as_str());
        out.push_str(",\"value\":");
        out.push_str(if target.value { "true" } else { "false" });
        out.push_str(",\"auth\":");
        push_json_string(out, target.authority.as_str());
        out.push_str(",\"reason\":");
        push_json_string(
            out,
            target
                .authority
                .hold_reason()
                .map_or("", HoldReason::as_str),
        );
        out.push_str(",\"cmdSeq\":");
        match target.command_seq {
            None => out.push_str("null"),
            Some(seq) => {
                out.push('"');
                out.push_str(&seq.to_string());
                out.push('"');
            }
        }
        out.push('}');
    }
    out.push(']');
}

fn push_authority_scope(out: &mut String, scope: &AuthorityScope) {
    match scope {
        AuthorityScope::Output(output) => {
            out.push_str("{\"kind\":\"output\",\"out\":");
            push_json_string(out, &output.0);
            out.push('}');
        }
        AuthorityScope::Workspace { output, workspace } => {
            out.push_str("{\"kind\":\"workspace\",\"out\":");
            push_json_string(out, &output.0);
            out.push_str(",\"ws\":");
            push_json_string(out, &workspace.0);
            out.push('}');
        }
    }
}

/// Structural size estimate for one typed send source: the exact object
/// width for explicit ids (`{"explicit":S}`), and the exact token-object
/// width for the implicit leg (`{"implicit":"home-then-action"}`).
#[must_use]
pub fn send_source_estimate(source: &crate::world_report::SendSource) -> usize {
    match source {
        crate::world_report::SendSource::Explicit(output) => 13 + json_string_len(&output.0),
        crate::world_report::SendSource::HomeThenAction => 31,
    }
}

/// Canonical decimal width of an `i32` (sign plus up to 10 digits).
#[must_use]
pub const fn int_len(value: i32) -> usize {
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

/// Canonical decimal width of a `u64` (up to 20 digits).
#[must_use]
pub const fn u64_len(mut value: u64) -> usize {
    let mut digits = 1usize;
    while value >= 10 {
        digits += 1;
        value /= 10;
    }
    digits
}

/// Canonical decimal width of a `usize` (up to 20 digits on 64-bit).
#[must_use]
pub const fn usize_len(mut value: usize) -> usize {
    let mut digits = 1usize;
    while value >= 10 {
        digits += 1;
        value /= 10;
    }
    digits
}

/// Gesture ingress estimate: `,"gesture":null` (12) when absent; the
/// present object `{"win":S,"kind":S,"phase":S,"id":"N","cursor":[x,y]|null,
/// "startCursor":[x,y]|null,"esc":B,"modal":B,"start":[x,y,w,h]}` counts its
/// fixed punctuation plus the window id, kind/phase tokens, decimal id,
/// cursor digits (`null` counts 4), bools, and the start rect digits.
#[must_use]
pub fn gesture_estimate(report: &WorldReport) -> usize {
    match report.gesture.as_ref() {
        None => 12,
        Some(gesture) => {
            100 + json_string_len(&gesture.window.0)
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

/// Structural size estimate for the optional gesture-settled section:
/// `,"gestureSettled":null` (22) when absent; the present object
/// `{"id":"N","committed":B}` counts its fixed punctuation plus the
/// decimal id and the bool.
#[must_use]
pub fn gesture_settled_estimate(settled: &Option<GestureSettled>) -> usize {
    match settled {
        None => 22,
        Some(terminal) => 40 + u64_len(terminal.id) + bool_len(terminal.committed),
    }
}

/// Structural size estimate for the operation-settled section:
/// `,"operationSettled":[]` (21) when empty; each event
/// `{"op":,"win":,"outcome":,"cause":}` counts its fixed punctuation plus
/// the operation id, window, outcome token and cause widths.
#[must_use]
pub fn operation_settled_estimate(settled: &[crate::world_report::OperationEvent]) -> usize {
    // `,"operationSettled":[` (20) plus `]` (1) when empty = 21.
    let mut bytes = 21usize;
    for (i, event) in settled.iter().enumerate() {
        if i > 0 {
            bytes += 1;
        }
        // {"op":,"win":,"outcome":,"cause":} = 34 punctuation.
        bytes += 34
            + json_string_len(&event.operation_id)
            + json_string_len(&event.window.0)
            + json_string_len(event.outcome.as_str())
            + json_string_len(event.cause.as_deref().unwrap_or_default());
    }
    bytes
}

/// Structural size estimate for the optional cursor-warp section:
/// `,"warpCursor":null` (18) when absent; the present point `[x,y]` counts
/// its key plus brackets, comma, and decimal digits.
#[must_use]
pub fn warp_cursor_estimate(warp: &Option<(i32, i32)>) -> usize {
    match warp {
        None => 18,
        Some((x, y)) => 17 + int_len(*x) + int_len(*y),
    }
}

/// Structural reply size estimate: every value through its exact width
/// ([`json_string_len`], [`u64_len`], [`usize_len`], decimal rects,
/// `true`/`false`/`null` literals) plus per-row JSON object overhead
/// derived field-by-field from [`encode_reply_canonical`] (key punctuation
/// counted exactly, never an asserted bare constant). Covers every
/// section. Conservative: any encoding of this shape fits within this
/// bound (verified by `estimate_upper_bounds_canonical_encoding`).
#[must_use]
pub fn reply_estimate(reply: &WorldReply) -> usize {
    // Envelope punctuation: {"v":,"adapter":,"gen":,"req":,"obs":"","rev":"",
    // "ready":,"consumed":,"scopes":[],"workspaces":[],"outputs":[],
    // "windows":[],"transfers":[],"effects":[]} plus ,"diagnostics":[] last
    // (after presentation) = 165 plus bools (10). The three
    // u64 counters encode as decimal strings (two quotes each). The
    // correlated `command_result` section is counted separately in
    // [`command_result_estimate`] (`,\"result\":null` = 14 when absent),
    // the gesture-settled section in [`gesture_settled_estimate`]
    // (`,\"gestureSettled\":null` = 22 when absent), the operation-settled
    // section in [`operation_settled_estimate`]
    // (`,\"operationSettled\":[]` = 21 when empty), and the warp section
    // in [`warp_cursor_estimate`] (`,\"warpCursor\":null` = 18 when absent).
    // The intent save/clear section rides between presentation and
    // diagnostics ([`intent_writes_estimate`], `,\"intentWrites\":[]` = 18
    // when empty).
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
    // One comma between adjacent rows of every non-empty array.
    bytes += reply.scopes.len().saturating_sub(1);
    for workspace in &reply.workspaces {
        // {"id":,"out":,"bind":,"scope":,"order":,"mode":,
        // "exist":,"hold":} = 65 punctuation plus the exist bool.
        // The optional ,"trailing":true adds 16 only when marked.
        bytes += 65
            + json_string_len(&workspace.id)
            + json_string_len(&workspace.output.0)
            + json_string_len(workspace.native_binding.as_deref().unwrap_or_default())
            + authority_scope_len(&workspace.scope)
            + usize_len(workspace.order)
            + json_string_len(workspace.tiling_mode.as_str())
            + bool_len(workspace.existence)
            + json_string_len(workspace.hold.map_or("", HoldReason::as_str));
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
            + json_string_len(output.hold.map_or("", HoldReason::as_str));
    }
    bytes += reply.outputs.len().saturating_sub(1);
    for diagnostic in &reply.diagnostics {
        // Held-row diagnostics ride last beside the presentation they fence.
        bytes += row_diagnostic_estimate(diagnostic);
    }
    bytes += reply.diagnostics.len().saturating_sub(1);
    for window in &reply.windows {
        // {"win":,"out":,"ws":,"alloc":,"frame":,"frame_auth":,
        // "frame_hold":,"member_auth":,"member_hold":,"frame_op":,
        // "floating":,"float_origin":} = 137 punctuation (109 plus the
        // new 28: ,"floating": = 12 and ,"float_origin": = 16).
        // Each rect is [x,y,w,h] (numbers + 5) or null (4). The explicit
        // float-placement operation id is empty when no target is
        // outstanding; the float origin token is empty unless capacity.
        let rect_len = |rect: Option<crate::geometry::Rect>| {
            rect.map_or(4, |r| {
                int_len(r.x) + int_len(r.y) + int_len(r.w) + int_len(r.h) + 5
            })
        };
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
                    .map_or("", HoldReason::as_str),
            )
            + json_string_len(window.membership_authority.as_str())
            + json_string_len(
                window
                    .membership_authority
                    .hold_reason()
                    .map_or("", HoldReason::as_str),
            )
            + json_string_len(window.frame_operation.as_deref().unwrap_or_default())
            + bool_len(window.floating)
            + json_string_len(window.float_origin.map_or("", FloatOrigin::as_str))
            + json_string_len(window.clear_operation.as_deref().unwrap_or_default())
            + window_target_lanes_estimate(window);
    }
    bytes += reply.windows.len().saturating_sub(1);
    for transfer in &reply.transfers {
        // {"op":,"kind":,"win":,"out":,"ws":,"follow":,"held":} = 53
        // punctuation plus up to 10 for the two bools.
        let (kind, window_len) = match &transfer.kind {
            TransferKind::Window(window) => ("window", json_string_len(&window.0)),
            TransferKind::View => ("view", 2),
            TransferKind::Migration => ("migration", 2),
        };
        bytes += 63
            + json_string_len(&transfer.operation_id)
            + json_string_len(kind)
            + window_len
            + json_string_len(&transfer.target_output.0)
            + json_string_len(&transfer.target_workspace);
    }
    bytes += reply.transfers.len().saturating_sub(1);
    for effect in &reply.effects {
        // {"id":,"kind":,"target":,"rev":""} = 36 punctuation (two string
        // quotes around the u64 revision).
        bytes += 36
            + json_string_len(&effect.id)
            + json_string_len(effect.kind.as_str())
            + json_string_len(&effect.target.0)
            + u64_len(effect.issued_revision);
    }
    bytes += reply.effects.len().saturating_sub(1);
    bytes += command_result_estimate(&reply.command_result);
    bytes += gesture_settled_estimate(&reply.gesture_settled);
    bytes += operation_settled_estimate(&reply.operation_settled);
    bytes += native_creations_estimate(&reply.native_creations);
    bytes += warp_cursor_estimate(&reply.cursor_warp);
    bytes += presentation_estimate(&reply.presentation);
    bytes += intent_writes_estimate(&reply.intent_writes);
    bytes
}

/// Maximum escaped canonical JSON width of one staged sticky/float save
/// value: the sticky command path stages `IntentData` canonical forms with
/// an always-present float lane (`Some(true)` on sticky-on, the prior on
/// sticky-off, `Some(false)` on the sticky-clearing toggle) and a sticky
/// lane of `true`/`false` (sticky-on) or `null` (sticky-off/clear). The
/// longest staged forms are
/// `{"v":1,"float":true,"stickyPrior":false}` and
/// `{"v":1,"float":false,"stickyPrior":null}`: 40 bytes with six `"`
/// characters, so [`json_string_len`] spells each as 2 + 40 + 6 = 48.
/// Every other staged form is shorter (47). Canonical bytes are ASCII
/// without `\` or C0 controls, so no other escaping applies; opaque
/// subject ids likewise never escape ([`crate::bounds::is_opaque_id`]).
pub const MAX_INTENT_SAVE_DATA_JSON: usize = 48;

/// Pre-mutation bound for the at-most-one sticky/float save row, gated on
/// the report's own command: `{"win":,"data":}` is 16 punctuation plus the
/// subject id plus the save value above (a single row carries no comma).
/// Reserved for `SetSticky`, `ToggleSticky`, and the sticky-clearing
/// `ToggleFloat`; zero for every other report. Conservative like every
/// other [`reply_upper_bound`] term; the exact-encoded staged reply stays
/// the commit backstop.
#[must_use]
pub fn max_intent_save_bound(report: &WorldReport) -> usize {
    let window: &str = match report.command.as_ref() {
        Some(WorldCommand::SetSticky { window, .. }) => window.0.as_str(),
        Some(WorldCommand::ToggleSticky { window }) => window.0.as_str(),
        Some(WorldCommand::Operation(CoreCommand::ToggleFloat { window, .. })) => window.as_str(),
        _ => return 0,
    };
    16 + json_string_len(window) + MAX_INTENT_SAVE_DATA_JSON
}

/// Maximum escaped canonical JSON width of one rollback save value: every
/// `IntentData` shape the proven-refusal rollback can restore (all nine
/// `(float, sticky_prior)` combinations, including record-free priors never
/// carried on this report). The longest canonical form is
/// `{"v":1,"float":false,"stickyPrior":false}`: 41 bytes with six `"`
/// characters, so [`json_string_len`] spells it as 2 + 41 + 6 = 49. The
/// command-path staged family tops out at 48
/// ([`MAX_INTENT_SAVE_DATA_JSON`]); rollback reserves the wider shape. A
/// null-clear rollback (`data: null`, 4) is cheaper and stays covered.
pub const MAX_INTENT_ROLLBACK_DATA_JSON: usize = 49;

/// Maximum per-row rollback write width in the published `intentWrites`
/// section: `{"win":,"data":}` is 16 punctuation plus the longest opaque
/// subject id (128 raw bytes, never escaped, so 130 quoted) plus
/// [`MAX_INTENT_ROLLBACK_DATA_JSON`], plus one array comma. Exactly
/// 16 + 130 + 49 + 1 = 196. The comma overcounts the final row by one,
/// conservatively.
pub const MAX_INTENT_ROLLBACK_ROW_BYTES: usize = 196;

/// Pre-mutation bound for queued native sticky rollback writes: one
/// [`MAX_INTENT_ROLLBACK_ROW_BYTES`] per potentially generated write. The
/// count is the pending-rollback reserve (every sticky binding with
/// `sticky_meta` and `!followup_done`, any disposition, plus already-queued
/// writes), never the report's own `stateAttempts` length, so record-free
/// priors and commandless proofs reserve alike. Conservative like every
/// other [`reply_upper_bound`] term; the exact-encoded staged reply stays
/// the commit backstop. Kept separate from [`max_intent_save_bound`] (the
/// command's own save row, at most 194 with a max-length id).
#[must_use]
pub fn max_intent_rollback_bound(count: usize) -> usize {
    count.saturating_mul(MAX_INTENT_ROLLBACK_ROW_BYTES)
}

/// Structural size estimate for the core intent save/clear section:
/// `,"intentWrites":[]` (18) when empty; each row
/// `{"win":S,"data":S|null}` counts 16 fixed punctuation plus the subject
/// id and the save value (`null` counts 4 when the slot clears).
#[must_use]
pub fn intent_writes_estimate(writes: &[crate::world_report::IntentWrite]) -> usize {
    // `,"intentWrites":[` (17) plus `]` (1) when empty = 18.
    let mut bytes = 18usize;
    for (i, write) in writes.iter().enumerate() {
        if i > 0 {
            bytes += 1;
        }
        // `{"win":,"data":}` = 16 punctuation.
        bytes +=
            16 + json_string_len(&write.win.0) + write.data.as_deref().map_or(4, json_string_len);
    }
    bytes
}

/// Structural size estimate for the opaque host storage return section:
/// `,"intentRecords":[]` (19) when empty; each row `{"win":S,"data":S}`
/// counts 16 fixed punctuation plus the subject id and the raw record
/// string (verbatim bytes, never decoded here).
#[must_use]
pub fn intent_records_estimate(records: &[crate::world_report::IntentRecord]) -> usize {
    // `,"intentRecords":[` (18) plus `]` (1) when empty = 19.
    let mut bytes = 19usize;
    for (i, record) in records.iter().enumerate() {
        if i > 0 {
            bytes += 1;
        }
        // `{"win":,"data":}` = 16 punctuation.
        bytes += 16 + json_string_len(&record.win.0) + json_string_len(&record.data);
    }
    bytes
}

/// Structural size estimate for the native creation desire section:
/// `,"nativeCreations":[]` (21) when empty; each desire
/// `{"op":,"ws":,"out":}` counts its fixed punctuation plus the operation
/// id, logical id, and output widths.
#[must_use]
pub fn native_creations_estimate(creations: &[crate::world_report::NativeCreationDesire]) -> usize {
    // `,"nativeCreations":[` (20) plus `]` (1) when empty = 21.
    let mut bytes = 21usize;
    for (i, creation) in creations.iter().enumerate() {
        if i > 0 {
            bytes += 1;
        }
        // {"op":,"ws":,"out":} = 22 punctuation.
        bytes += 22
            + json_string_len(&creation.operation_id)
            + json_string_len(&creation.logical_workspace)
            + json_string_len(&creation.target_output.0);
    }
    bytes
}

/// Structural reply size estimate for the uniform presentation section,
/// field-by-field from [`encode_presentation`] below. `,"presentation":`
/// (16) plus the object: foreground id, border shape, underlay shape, and
/// pager content.
#[must_use]
pub fn presentation_estimate(presentation: &crate::world_presentation::WorldPresentation) -> usize {
    use crate::world_presentation::PresentationUnderlay;
    // `,"presentation":{"foreground":,"border":,"underlay":,"pager":[]}`
    // = 67 punctuation plus the foreground id.
    let mut bytes = 67 + json_string_len(&presentation.foreground);
    // `,"suspendedOutput":S` = 19 punctuation plus the output id, only when
    // the verdict is present (omitted otherwise, so default bytes stay exact).
    if let Some(output) = presentation.suspended_output.as_ref() {
        bytes += 19 + json_string_len(&output.0);
    }
    // Border `{"visible":,"reason":,"inner":,"outer":}` = 40 punctuation
    // plus the bool, the reason token, and the two rects.
    bytes += 40
        + bool_len(presentation.border.visible)
        + json_string_len(
            presentation
                .border
                .reason
                .map_or("", crate::visual::BorderHideReason::as_str),
        )
        + rect_len(presentation.border.inner)
        + rect_len(presentation.border.outer);
    match &presentation.underlay {
        PresentationUnderlay::Found(found) => {
            // `{"status":"found","reason":"","baseRev":"","homeOut":"",
            // "homeWs":"","group":"","leaf":"","win":"","union":[],
            // "outer":[],"members":[]}` = 129 punctuation plus ids, digits,
            // rects, and members.
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
                // `{"leaf":"","win":"","rect":[]}` = 28 punctuation.
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
            // `{"status":"hidden","reason":"","baseRev":}` = 42 punctuation;
            // the revision is `null` (4) or `"digits"` (2 + width).
            bytes += 42
                + json_string_len(reason.as_str())
                + base_revision.map_or(4, |rev| 2 + u64_len(rev));
        }
    }
    for screen in &presentation.pager {
        // `{"out":"","workspaces":[]}` = 26 punctuation plus the output id.
        bytes += 26 + json_string_len(&screen.output);
        for workspace in &screen.workspaces {
            // `{"id":"","ordinal":,"label":"","current":,"occupied":,
            // "containsFocused":,"windows":[]}` = 88 punctuation plus the
            // id, ordinal digits, label, and three bools. The optional
            // ,"trailing":true adds 16 only when marked.
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
                // `{"win":"","frame":[]}` = 19 punctuation plus the id and
                // the frame.
                bytes += 19 + json_string_len(&window.window.0) + rect_len(Some(window.frame));
            }
            bytes += workspace.windows.len().saturating_sub(1);
        }
        bytes += screen.workspaces.len().saturating_sub(1);
    }
    bytes += presentation.pager.len().saturating_sub(1);
    bytes
}

/// Structural size estimate for the correlated `command_result` section,
/// field-by-field from [`encode_command_result`] below. `None` encodes as
/// `,"result":null` (14); every variant counts the `,"result":` key (10)
/// plus its exact keys plus value
/// widths (member/window id lists scale with the result's own rows).
#[must_use]
pub fn command_result_estimate(result: &Option<CommandResult>) -> usize {
    // `,"result":` prefix (10) plus `null` (4) when absent.
    let Some(result) = result else {
        return 14;
    };
    let rect_len =
        |r: crate::geometry::Rect| int_len(r.x) + int_len(r.y) + int_len(r.w) + int_len(r.h) + 5;
    // Every variant body below plus the shared `,"result":` key (10).
    10 + match result {
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
            // = 113 punctuation plus the revision digits, hover body, and
            // target id.
            let mut bytes = 113
                + u64_len(*revision)
                + json_string_len(&domain.output.0)
                + json_string_len(&domain.workspace.0)
                + json_string_len(&source.output.0)
                + json_string_len(&source.workspace.0)
                + json_string_len(&target.0)
                + rect_len(*rect);
            bytes += match hover {
                None => 4,
                Some(hover) => {
                    // {"domainOut":"","domainWs":"","leaf":"","win":"",
                    // "rev":"","prior":,"priorOut":"","priorWs":""} = 88
                    // punctuation; prior is null (4) or
                    // {"group":"","edge":""} (26 + widths).
                    88 + json_string_len(&hover.domain.output.0)
                        + json_string_len(&hover.domain.workspace.0)
                        + json_string_len(&hover.source_leaf.0)
                        + json_string_len(&hover.source_window.0)
                        + u64_len(hover.revision)
                        + hover.prior.as_ref().map_or(4, |prior| {
                            26 + json_string_len(&prior.group.0)
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
            // {"op":"toggle-float","win":"","rect":} = 36 punctuation
            // plus the rect body (`null` or [x,y,w,h]) plus
            // `,"noSpace":true` (15) only on an explicit-tile no-space
            // retry (absent otherwise, so older replies stay byte-identical).
            36 + json_string_len(&window.0)
                + rect.map_or(4, rect_len)
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
            // "floats":} = 119 punctuation.
            let mut bytes =
                119 + json_string_len(match direction {
                    Direction::Left => "left",
                    Direction::Right => "right",
                    Direction::Up => "up",
                    Direction::Down => "down",
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
            // = 54 punctuation; win/rect/cause null (4 each) or values.
            let mut bytes = 54
                + json_string_len(outcome.as_str())
                + target.as_ref().map_or(4, |t| json_string_len(&t.0));
            bytes += rect.map_or(4, rect_len);
            bytes += cause.as_ref().map_or(4, |c| json_string_len(c));
            bytes
        }
        CommandResult::Maximize {
            window,
            maximized,
            clear_fullscreen,
            was_maximized,
        } => {
            // {"op":"maximize","win":S,"maximized":B,"clearFullscreen":B,
            // "wasMaximized":B} = 72 fixed.
            72 + json_string_len(&window.0)
                + bool_len(*maximized)
                + bool_len(*clear_fullscreen)
                + bool_len(*was_maximized)
        }
        CommandResult::Rejected { kind } => {
            // {"op":"rejected","kind":S} = 25 fixed.
            25 + json_string_len(kind.as_str())
        }
    }
}

/// Structural size estimate for one optional visibility write receipt:
/// `null` (4) when absent; the present object
/// `{"property":S,"target":B,"revision":"N","path":S,"setter":S,
/// "reason":S,"pre":sample|null,"post":sample|null}` counts 80 fixed
/// (braces, keys, commas, and the revision quotes) plus the property/path/
/// setter/reason tokens, the target bool, the revision digits, and the two
/// nullable samples below.
fn last_write_estimate(last: &Option<crate::world_report::LastWrite>) -> usize {
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

/// Structural size estimate for one nullable visibility sample:
/// `null` (4) when absent; the present object
/// `{"obs":"N","lifetimeMatched":B|null,
/// "visibility":{"visible":B|null,"suppressionOwned":B|null},
/// "minimized":B|null,"hostHidden":B|null}` counts 102 fixed plus the
/// observation digits and the five nullable bools.
fn visibility_sample_estimate(sample: &Option<crate::world_report::VisibilitySample>) -> usize {
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

/// Structural request size estimate over the same accounting: every
/// carried string through [`json_string_len`], exact numeric widths, and
/// per-row overhead derived field-by-field from the protocol wire layout
/// (`v`, `owner`, `adapter`, `gen`, `obs`/`reqSeq`/`cmdSeq` decimal strings,
/// nullable command, `screens`, `windows`, `views`, `closed`, capability
/// flags, `native`, `predicate`, `refill`, `anchorFallback`,
/// `noSpaceResult`, `switchTarget`, `freshPlacement`, nullable `cursor`,
/// nullable `cursorHit`, nullable action-context outputs, `defaultTiled`/`focused`/`complete`/`gesture`/
/// `focusFollowsCursor`/`cursorWarp` values, `monotonicMs` decimal string,
/// `frameAttempts`, `memberAttempts`, `viewAttempts`, `stateAttempts`).
/// Conservative: any compact encoding of the report fits within this
/// bound (locked by `request_estimate_bounds_actual_wire_bytes`). The
/// adapter must emit compact JSON with no whitespace; key order does not
/// matter because commas and keys count the same in any order.
#[must_use]
pub fn report_estimate(report: &WorldReport) -> usize {
    // Envelope fixed bytes: {"v":2,"owner":,"adapter":,"gen":,"obs":"","reqSeq":"",
    // "req":,"cmdSeq":,"command":,"screens":[],"windows":[],"views":[],"closed":[],
    // "hostOwned":,"equalCount":,"birthClear":,"native":[],"predicate":,"refill":,"defaultTiled":,
    // "complete":,"gesture":} = 219, plus the cursor-first
    // targeting keys (10 + 16 + 18 + 22 + 13 = 79):
    // ,"cursor":,"switchTarget":,"freshPlacement":,"focusFollowsCursor":,
    // "cursorWarp": = 308
    // plus ,"cursorHit": (13) = 321, plus ,"actionForeground": (20) and
    // ,"actionCached": (16) = 357, plus ,"actionOutputs":[] (20) = 377,
    // plus ,"monotonicMs":"" (19) and ,"frameAttempts":[] (19) = 415,
    // plus ,"memberAttempts":[] (20) and ,"viewAttempts":[] (18) = 453,
    // plus ,"clearAttempts":[] (18) = 471, plus ,"creationAttempts":[] (22)
    // = 493, plus ,"gesture":null (12) = 505, plus ,"birthClear": (14) =
    // 519, plus ,"bornMaximized":S (17
    // fixed plus the token) = 522, plus the version digits, plus
    // ,"presentation":{} below, plus ,"primary":null (12) = 534 for the
    // omitted host designation. Values
    // counted
    // separately below; nullable command, cursor, cursor-hit, and
    // action-context slots count `null` (4), while the action-output
    // registry counts its brackets plus one comma per entry. The workspace
    // mode token rides as `"mode":S` (9 fixed plus the token); membership
    // evidence rides per fact below (absent counts `null`).
    // The required ,"stateAttempts":[] section adds 20 to the base.
    let mut bytes = 471
        + 36
        + 20
        + 17
        + gesture_estimate(report)
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
        + bool_len(report.inventory_complete)
        + bool_len(report.gesture_barrier)
        + presentation_ingress_estimate(&report.presentation);
    for screen in &report.screens {
        // {"out":S,"full":R|null,"work":R|null,"complete":B,"scale":N,
        // "identity":{"manufacturer":S|null,"model":S|null,"serial":S|null,
        // "connector":S|null}} = 106 fixed plus rect digits, bools, the
        // canonical scale spelling, and identity strings.
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
    // ,"primary":S|null (12 fixed plus the designation when present).
    bytes += 12
        + report
            .primary_output
            .as_ref()
            .map_or(4, |out| json_string_len(&out.0));
    for held in &report.held_screens {
        // Held rows ride the wire as ordinary screen objects with their
        // carried keys: `{` + one `"key":value` per retained field +
        // commas + `}` (values through [`raw_setting_value_estimate`]).
        // The diagnostic is core-side only, never on the wire. The
        // estimator runs before validation, so a non-finite retained
        // number (which has no canonical spelling) counts the `null`
        // width per bad scalar as a pure size bound, never as an
        // encoding, while every carried structure and text byte counts.
        bytes += 2
            + held
                .fields
                .iter()
                .map(|(key, value)| json_string_len(key) + 1 + raw_setting_value_estimate(value))
                .sum::<usize>()
            + held.fields.len().saturating_sub(1);
    }
    for fact in &report.windows {
        // {"win":S,"out":S|null,"rect":R|null,"fullscreen":B|null,
        // "maximized":B|null,"sticky":B|null,"minimized":B|null,
        // "hints":{"minW":N,"minH":N,"maxW":N,"maxH":N}|null,
        // "nativeWs":S|null,"hostHidden":B|null,"privileged":B|null,
        // "managed":B|null,"acceptsInput":B|null,
        // "activationInhibited":B|null,"toolWindow":B|null,"owned":B|null,
        // "transient":B|null,"modal":B|null,"resizable":B|null,
        // "above":B|null,"below":B|null,"owner":S|null,"parent":S|null,
        // "role":S|null,"appId":S|null,"executable":S|null,
        // "processId":S|null,"class":S|null,"title":S|null,"type":S|null,
        // "activationPolicy":S|null,"layer":N|null,"fullscreenOwner":S|null,
        // "fullscreenExitable":B|null,"suppressionAvailable":B|null,
        // "visibility":{"visible":B|null,"suppressionOwned":B|null},
        // "lastWrite":{...}|null} = 523 fixed (outer braces, every key
        // plus colon, hints and visibility braces, and the separating
        // commas, with no values) plus ids, rect digits, hint bounds,
        // tokens, counters, and samples (`null` counts 4; absent nullable
        // scalars count `null`).
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
        // Held rows ride the wire as ordinary window objects with their
        // carried keys: `{` + one `"key":value` per retained field +
        // commas + `}` (values through [`raw_setting_value_estimate`]).
        // The diagnostic is core-side only, never on the wire. The
        // estimator runs before validation, so a non-finite retained
        // number (which has no canonical spelling) counts the `null`
        // width per bad scalar as a pure size bound, never as an
        // encoding, while every carried structure and text byte counts.
        bytes += 2
            + held
                .fields
                .iter()
                .map(|(key, value)| json_string_len(key) + 1 + raw_setting_value_estimate(value))
                .sum::<usize>()
            + held.fields.len().saturating_sub(1);
    }
    for view in &report.views {
        // {"out":S,"view":S} = 17 fixed.
        bytes += 17 + json_string_len(&view.output.0) + json_string_len(&view.view);
    }
    for id in &report.closed {
        bytes += json_string_len(&id.0) + 1;
    }
    for native in &report.native {
        // {"out":S,"order":[S...],"current":S|null,"occupancy":[{"native":S,
        // "count":N|null}...]} = 58 fixed plus strings, counts, and commas;
        // `null` (4) when the current surface is not an ordered desktop and
        // when an occupant count is unknown.
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
    for attempt in &report.frames {
        // {"win":S,"target":[x,y,w,h],"setter":S,"readback":R|null,
        // "dpiBefore":N|null,"dpiAfter":N|null,"attemptMs":"u64",
        // "op":S,"pre":R|null} = 98 fixed (the 85 above plus ,"op": (6)
        // plus ,"pre": (7)) plus ids, rect digits, setter token, DPI
        // digits, the stamp, the operation id, and the pre-write rect.
        // Absent readback/prewrite/DPI count `null` (4); an ordinary
        // attempt carries an empty operation id.
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
    for attempt in &report.member_attempts {
        // Empty-string placeholders plus null references establish exact
        // fixed punctuation; each string placeholder is replaced below.
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
    for attempt in &report.clear_attempts {
        bytes += 62
            + json_string_len(&attempt.operation_id)
            + json_string_len(&attempt.window.0)
            + json_string_len(attempt.setter.as_str())
            + 5
            + 5
            + u64_len(attempt.attempt_ms);
    }
    for attempt in &report.creation_attempts {
        // {"op":S,"ws":S,"out":S,"setter":S,"before":[S...],
        // "after":[S...],"created":S|null,"attemptMs":"u64"} = 85 fixed
        // (the 74 above plus ,"created": (11)) plus ids, setter token,
        // order lists, the correlated id (`null` counts 4 when the host
        // cannot prove which desktop its create call produced), and stamp.
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
    for attempt in &report.state_attempts {
        // {"win":S,"property":S,"target":B,"rev":"u64","setter":S,
        // "pre":B|null,"post":B|null,"attemptMs":"u64"} = 79 fixed (8
        // quoted keys plus colons, commas, braces, and the two quoted
        // decimal stamps) plus the window id, property token, target
        // bool, revision digits, setter token, nullable pre/post proofs
        // (`null` counts 4 each), and stamp digits.
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
    // No adapter-projected restoration key is admitted; opaque storage
    // returns are a separate lane below.
    // Opaque host storage return rides after the presentation ingress with
    // exact escaped widths (control/quote/backslash per `json_string_len`).
    bytes += intent_records_estimate(&report.intent_records);
    bytes
}

/// Structural request size estimate for the stage-5 presentation ingress:
/// `,"presentation":{...}` with the normalized foreground token, fresh
/// flags, host-scaled style numbers, movement facts, pause flag, pager
/// order, and per-window pager frame sources. Field-by-field from the
/// protocol wire layout below.
#[must_use]
pub fn presentation_ingress_estimate(ingress: &crate::world_report::PresentationIngress) -> usize {
    // `,"presentation":{` (18) plus the 17 fixed ingress keys with their
    // commas and the order bracket (239), the `,"pagerFrames":[` key (16)
    // and its closing bracket (1): 274 exact, rounded to 276 with the same
    // one-count conservatism as the order list. Value widths counted below;
    // frame rows mirror the reply pager-window shape with two extra
    // nullable rects.
    let mut bytes = 276
        + json_string_len(&ingress.foreground)
        + bool_len(ingress.foreground_minimized)
        + bool_len(ingress.foreground_maximized)
        + bool_len(ingress.foreground_fullscreen)
        + bool_len(ingress.foreground_cloaked)
        + bool_len(ingress.foreground_hidden_workspace)
        + bool_len(ingress.foreground_shell)
        + rect_len(ingress.foreground_frame)
        + bool_len(ingress.border_enabled)
        + bool_len(ingress.underlay_enabled)
        + int_len(ingress.border_gap_px)
        + int_len(ingress.border_width_px)
        + int_len(ingress.underlay_extension_px)
        + bool_len(ingress.chord_held)
        + json_string_len(&ingress.move_window)
        + bool_len(ingress.paused);
    for window in &ingress.pager_order {
        bytes += json_string_len(&window.0) + 1;
    }
    for row in &ingress.pager_frames {
        // `{"win":,"live":,"float":,"tile":}` = 37 punctuation plus the id
        // and the three rects (`null` counts 4 each).
        bytes += 37
            + json_string_len(&row.window.0)
            + rect_len(row.live)
            + rect_len(row.float_frame)
            + rect_len(row.tile_frame);
    }
    bytes += ingress.pager_frames.len().saturating_sub(1);
    bytes
}

/// Canonical width of a JSON bool literal.
#[must_use]
pub const fn bool_len(value: bool) -> usize {
    if value { 4 } else { 5 }
}

/// Canonical decimal width of a wire rect `[x,y,w,h]` (5 punctuation)
/// or `null` (4).
#[must_use]
pub const fn rect_len(rect: Option<crate::geometry::Rect>) -> usize {
    match rect {
        Some(rect) => 5 + int_len(rect.x) + int_len(rect.y) + int_len(rect.w) + int_len(rect.h),
        None => 4,
    }
}

/// Wire token for an adjacency direction, matching the protocol codec.
#[must_use]
pub const fn direction_token(direction: Direction) -> &'static str {
    match direction {
        Direction::Left => "left",
        Direction::Right => "right",
        Direction::Up => "up",
        Direction::Down => "down",
    }
}

/// Estimated encoded size of one world command: fixed wire punctuation
/// per variant (derived from the protocol shapes) plus exact carried
/// string/numeric widths. The caller counts `null` (4) when absent.
fn command_estimate_owned(command: &WorldCommand) -> usize {
    match command {
        WorldCommand::Focus { window, direction } => {
            // {"op":"focus","win":S,"direction":S} = 34 fixed.
            34 + json_string_len(&window.0) + json_string_len(direction)
        }
        WorldCommand::Send {
            window,
            target_output,
            target_workspace,
            follow,
        } => {
            // {"op":"send","win":S,"out":S,"ws":S,"follow":B} = 43 fixed.
            43 + bool_len(*follow)
                + json_string_len(&window.0)
                + json_string_len(&target_output.0)
                + json_string_len(target_workspace)
        }
        WorldCommand::SwitchView { output, workspace } => {
            // {"op":"switch-view","out":S,"ws":S} = 33 fixed.
            33 + json_string_len(&output.0) + json_string_len(workspace)
        }
        WorldCommand::SelectWorkspace { output, workspace } => {
            // {"op":"select-workspace","out":S,"ws":S} = 38 fixed.
            38 + json_string_len(&output.0) + json_string_len(workspace)
        }
        WorldCommand::SelectNumbered { output, index } => {
            // {"op":"select-numbered","out":S|null,"index":N} = 40 fixed;
            // a resolved (adapter-absent) output counts `null` (4).
            40 + output.as_ref().map_or(4, |out| json_string_len(&out.0))
                + usize_len(usize::from(*index))
        }
        WorldCommand::SelectRelative { output, delta } => {
            // {"op":"select-relative","out":S|null,"delta":N} = 40 fixed.
            40 + output.as_ref().map_or(4, |out| json_string_len(&out.0)) + int_len(*delta)
        }
        WorldCommand::SelectPrevious { output } => {
            // {"op":"select-previous","out":S|null} = 31 fixed.
            31 + output.as_ref().map_or(4, |out| json_string_len(&out.0))
        }
        WorldCommand::SelectTrailing { output } => {
            // {"op":"select-trailing","out":S|null} = 31 fixed.
            31 + output.as_ref().map_or(4, |out| json_string_len(&out.0))
        }
        WorldCommand::SendRelative {
            window,
            source,
            delta,
            follow,
        } => {
            // {"op":"send-relative","win":S,"source":{...},"delta":N,
            // "follow":B} = 58 fixed plus the source object, the delta
            // digits, and the bool.
            58 + bool_len(*follow)
                + json_string_len(&window.0)
                + send_source_estimate(source)
                + int_len(*delta)
        }
        WorldCommand::SendNumbered {
            window,
            source,
            index,
            follow,
        } => {
            // {"op":"send-numbered","win":S,"source":{...},"index":N,
            // "follow":B} = 59 fixed plus the source object, the index
            // digits, and the bool.
            59 + bool_len(*follow)
                + json_string_len(&window.0)
                + send_source_estimate(source)
                + usize_len(usize::from(*index))
        }
        WorldCommand::SetWorkspaceTiling {
            output,
            workspace,
            tiled,
        } => {
            // {"op":"set-workspace-tiling","out":S,"ws":S,"tiled":B}
            // = 52 fixed.
            52 + bool_len(*tiled) + json_string_len(&output.0) + json_string_len(workspace)
        }
        WorldCommand::ToggleWorkspaceTiling { output, workspace } => {
            // {"op":"toggle-workspace-tiling","out":S,"ws":S}
            // = 44 fixed.
            44 + json_string_len(&output.0) + json_string_len(workspace)
        }
        WorldCommand::MigrateWorkspace {
            source_output,
            target_output,
            workspace,
        } => {
            // {"op":"migrate-workspace","from":S,"to":S,"ws":S} = 45 fixed.
            45 + json_string_len(&source_output.0)
                + json_string_len(&target_output.0)
                + json_string_len(workspace)
        }
        WorldCommand::FocusDirectional { window, direction } => {
            // {"op":"focus-directional","win":S,"direction":S} = 44 fixed.
            44 + json_string_len(&window.0) + json_string_len(direction)
        }
        WorldCommand::MoveDirectional {
            window,
            direction,
            same_axis,
        } => {
            // {"op":"move-directional","win":S,"direction":S,"sameAxis":S}
            // = 57 fixed.
            57 + json_string_len(&window.0)
                + json_string_len(direction)
                + json_string_len(same_axis.as_wire_str())
        }
        WorldCommand::SendDirectional {
            window,
            direction,
            follow,
        } => {
            // {"op":"send-directional","win":S,"direction":S,"follow":B}
            // = 55 fixed.
            55 + json_string_len(&window.0) + json_string_len(direction) + bool_len(*follow)
        }
        WorldCommand::MigrateDirectional { direction } => {
            // {"op":"migrate-directional","direction":S} = 41 fixed.
            41 + json_string_len(direction)
        }
        WorldCommand::ToggleMaximize { window } => {
            // {"op":"toggle-maximize","win":S} = 31 fixed.
            31 + json_string_len(&window.0)
        }
        WorldCommand::SetSticky { window, value } => {
            // {"op":"set-sticky","win":S,"value":B} = 35 fixed.
            35 + json_string_len(&window.0) + bool_len(*value)
        }
        WorldCommand::ToggleSticky { window } => {
            // {"op":"toggle-sticky","win":S} = 28 fixed.
            28 + json_string_len(&window.0)
        }
        WorldCommand::Operation(command) => {
            // {"op":"operation","command":{...}} = 30 fixed.
            30 + operation_estimate(command)
        }
    }
}

/// Estimated encoded size of one operation command payload: fixed wire
/// punctuation per variant plus exact carried widths, including the full
/// drag hover/source objects.
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
            // {"op":"move","win":S,"direction":S,"crossOutput":B,
            // "sameAxis":S} = 61 fixed.
            61 + bool_len(*cross_output_transfer)
                + json_string_len(window)
                + json_string_len(direction)
                + json_string_len(same_axis_move.as_wire_str())
        }
        CoreCommand::Focus {
            window,
            direction,
            cross_output_transfer,
            float_subject,
        } => {
            // {"op":"focus","win":S,"direction":S,"crossOutput":B,
            // "floatSubject":B} = 65 fixed.
            65 + bool_len(*cross_output_transfer)
                + bool_len(*float_subject)
                + json_string_len(window)
                + json_string_len(direction)
        }
        CoreCommand::Resize {
            window,
            direction,
            mode,
            press_index,
        } => {
            // {"op":"resize","win":S,"direction":S,"mode":S,"press":N}
            // = 52 fixed.
            52 + json_string_len(window)
                + json_string_len(direction)
                + json_string_len(mode)
                + u64_len(u64::from(*press_index))
        }
        CoreCommand::PointerResize {
            window,
            direction,
            boundary,
            direction2,
            boundary2,
        } => {
            // {"op":"pointer-resize","win":S,"direction":S,"boundary":N,
            // "direction2":S,"boundary2":N} = 76 fixed; nullable slots
            // count `null` (4) when absent.
            76 + json_string_len(window)
                + json_string_len(direction)
                + int_len(*boundary)
                + direction2.as_deref().map_or(4, json_string_len)
                + boundary2.map_or(4, int_len)
        }
        CoreCommand::ToggleFloat { window, float_rect } => {
            // {"op":"toggle-float","win":S,"rect":R} = 36 fixed; the rect
            // counts [N,N,N,N] (numbers plus 5) or `null` (4).
            36 + json_string_len(window)
                + float_rect.map_or(4, |r| {
                    int_len(r.x) + int_len(r.y) + int_len(r.w) + int_len(r.h) + 5
                })
        }
        CoreCommand::ToggleOrientation { window } => {
            // {"op":"toggle-orientation","win":S} = 34 fixed.
            34 + json_string_len(window)
        }
        CoreCommand::SendToWorkspace {
            window,
            target_output,
            target_workspace,
            follow,
        } => {
            // {"op":"send-to-workspace","win":S,"out":S,"ws":S,
            // "follow":B} = 56 fixed (37 plus the 19-byte op token).
            56 + bool_len(*follow)
                + json_string_len(window)
                + json_string_len(target_output)
                + json_string_len(target_workspace)
        }
        CoreCommand::SendToOutput {
            window,
            target_output,
            target_workspace,
            follow,
        } => {
            // {"op":"send-to-output","win":S,"out":S,"ws":S,"follow":B}
            // = 53 fixed (37 plus the 16-byte op token).
            53 + bool_len(*follow)
                + json_string_len(window)
                + json_string_len(target_output)
                + json_string_len(target_workspace)
        }
        CoreCommand::MigrateWorkspace { direction } => {
            // {"op":"migrate-workspace","direction":S} = 38 fixed.
            38 + json_string_len(direction)
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
            // {"op":OP,"win":S,"x":N,"y":N,"hover":H,"source":R}:
            // 43 fixed plus the op token.
            let op_len = match command {
                CoreCommand::DragDrop { .. } => json_string_len("drag-drop"),
                _ => json_string_len("drag-preview"),
            };
            43 + op_len
                + json_string_len(window)
                + int_len(*x)
                + int_len(*y)
                + hover_estimate(hover_prior)
                + drag_source_estimate(source)
        }
    }
}

/// Estimated encoded size of one drag hover object: `null` (4) when
/// absent, else the full `{"domainOut":S,"domainWs":S,"leaf":S,"win":S,
/// "rev":"N","prior":P,"priorOut":S,"priorWs":S}` shape (85 fixed plus
/// the decimal revision and carried ids). The nested prior counts
/// `{"group":S,"edge":S}` (18 fixed) or `null` (4).
fn hover_estimate(hover: &Option<crate::session::DragHoverPrior>) -> usize {
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

/// Estimated encoded size of one drag source object: `null` (4) when
/// absent, else `{"out":S,"ws":S}` (14 fixed).
fn drag_source_estimate(source: &Option<crate::session::DomainKey>) -> usize {
    source.as_ref().map_or(4, |key| {
        14 + json_string_len(&key.output.0) + json_string_len(&key.workspace.0)
    })
}

/// Canonical JSON writer shared by the transactional commit gate and the
/// future protocol codec. Field order is fixed here; any encoder claiming
/// this shape must emit these fields in this order with [`json_string_len`]
/// strings and decimal numbers. The commit gate measures this encoding
/// exactly, so overflow discards the staged candidate rather than a
/// truncated page.
fn push_json_string(out: &mut String, value: &str) {
    out.push('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\0'..='\u{1f}' => {
                const HEX: &[u8; 16] = b"0123456789abcdef";
                let byte = c as u8;
                out.push_str("\\u00");
                out.push(char::from(HEX[usize::from(byte >> 4)]));
                out.push(char::from(HEX[usize::from(byte & 0x0F)]));
            }
            _ => out.push(c),
        }
    }
    out.push('"');
}

/// Shortest round-trip spelling for one finite binary64 value: Ryu digits
/// with a plain (never scientific) expansion. `-0.0` spells `"0"`, matching
/// the digest and the protocol number gate.
///
/// Finite precondition: callers must prove finiteness first (every
/// canonical writer below is checked and only calls this on the finite
/// leg). The `debug_assert` guards the precondition in debug builds; the
/// checked wrappers make it unreachable for non-finite input in all builds.
#[must_use]
pub fn encode_finite_json_number(value: f64) -> String {
    debug_assert!(value.is_finite(), "finite precondition");
    if value == 0.0 {
        return "0".into();
    }
    let mut buffer = ryu::Buffer::new();
    let mut spelling = buffer.format_finite(value).to_owned();
    if !spelling.contains(['e', 'E']) {
        if spelling.ends_with(".0") {
            spelling.truncate(spelling.len() - 2);
        }
        return spelling;
    }
    let (mantissa, exp) = spelling.split_once(['e', 'E']).expect("ryu exponent");
    let exp: i32 = exp.parse().expect("ryu exponent parses");
    let negative = mantissa.starts_with('-');
    let unsigned = mantissa.trim_start_matches('-');
    let dot = unsigned.find('.').unwrap_or(unsigned.len()) as i32;
    let digits: String = unsigned.chars().filter(|c| *c != '.').collect();
    let point = dot + exp;
    let mut expanded = if point <= 0 {
        format!("0.{}{}", "0".repeat((-point) as usize), digits)
    } else if point as usize >= digits.len() {
        format!("{}{}", digits, "0".repeat(point as usize - digits.len()))
    } else {
        format!(
            "{}.{}",
            &digits[..point as usize],
            &digits[point as usize..]
        )
    };
    if expanded.contains('.') {
        while expanded.ends_with('0') {
            expanded.pop();
        }
        if expanded.ends_with('.') {
            expanded.pop();
        }
    }
    if negative {
        expanded.insert(0, '-');
    }
    expanded
}

/// Checked-canonical failure: a non-finite binary64 reached a canonical
/// writer. Callers grade the row to a narrow hold or refuse the report;
/// writers never normalize to `null` and never panic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NonFiniteNumber;

impl NonFiniteNumber {
    /// Fixed check vocabulary for logs, never payload bytes.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        "nonfinite"
    }
}

/// Canonical JSON encoding for one JSON number: the finite shortest
/// round-trip spelling. Non-finite values (constructible only through the
/// public typed carriers, never from wire JSON) are a checked error: the
/// writer emits nothing, so a non-finite typed fact can never normalize
/// to `null` on the wire.
pub fn push_json_number(out: &mut String, value: f64) -> Result<(), NonFiniteNumber> {
    if value.is_finite() {
        out.push_str(&encode_finite_json_number(value));
        Ok(())
    } else {
        Err(NonFiniteNumber)
    }
}

/// Exact encoded width of [`push_json_number`] without encoding: `None`
/// when the value has no canonical spelling (non-finite). Pre-validation
/// estimators fall back to the `null` width (4) as a pure size bound, never
/// as an encoding.
#[must_use]
pub fn json_number_len(value: f64) -> Option<usize> {
    if value.is_finite() {
        Some(encode_finite_json_number(value).len())
    } else {
        None
    }
}

/// Canonical JSON encoding for one JSON-agnostic held-screen field value:
/// `null` and bools literally, finite numbers in the shared shortest
/// round-trip spelling, strings through the canonical escaping of
/// [`json_string_len`], arrays and objects in stored order (nested objects
/// iterate in stored order, so the spelling stays deterministic). A
/// non-finite number anywhere in the value is a checked error (see
/// [`push_json_number`]), never a silent `null`. Shared by the canonical
/// held-row writer and the request estimator so the two cannot differ.
/// Atomic: a non-finite value anywhere inside leaves `out` untouched.
pub fn push_raw_setting_value(
    out: &mut String,
    value: &crate::connection::RawSettingValue,
) -> Result<(), NonFiniteNumber> {
    let mut staged = String::new();
    push_raw_setting_value_inner(&mut staged, value)?;
    out.push_str(&staged);
    Ok(())
}

fn push_raw_setting_value_inner(
    out: &mut String,
    value: &crate::connection::RawSettingValue,
) -> Result<(), NonFiniteNumber> {
    use crate::connection::RawSettingValue;
    match value {
        RawSettingValue::Null => out.push_str("null"),
        RawSettingValue::Bool(set) => out.push_str(if *set { "true" } else { "false" }),
        RawSettingValue::Number(number) => push_json_number(out, *number)?,
        RawSettingValue::String(text) => push_json_string(out, text),
        RawSettingValue::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                push_raw_setting_value_inner(out, item)?;
            }
            out.push(']');
        }
        RawSettingValue::Object(entries) => {
            out.push('{');
            for (i, (key, item)) in entries.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                push_json_string(out, key);
                out.push(':');
                push_raw_setting_value_inner(out, item)?;
            }
            out.push('}');
        }
    }
    Ok(())
}
/// Exact encoded width of [`push_raw_setting_value`] without encoding:
/// `None` when the value has no canonical spelling (a non-finite number
/// anywhere inside). Pre-validation estimators fall back per value, never
/// as an encoding (see [`report_estimate`]).
#[must_use]
pub fn raw_setting_value_len(value: &crate::connection::RawSettingValue) -> Option<usize> {
    use crate::connection::RawSettingValue;
    match value {
        RawSettingValue::Null => Some(4),
        RawSettingValue::Bool(set) => Some(bool_len(*set)),
        RawSettingValue::Number(number) => json_number_len(*number),
        RawSettingValue::String(text) => Some(json_string_len(text)),
        RawSettingValue::Array(items) => {
            let mut bytes = 2usize;
            for item in items {
                bytes += raw_setting_value_len(item)?;
            }
            bytes += items.len().saturating_sub(1);
            Some(bytes)
        }
        RawSettingValue::Object(entries) => {
            let mut bytes = 2usize;
            for (key, item) in entries {
                bytes += json_string_len(key) + 1 + raw_setting_value_len(item)?;
            }
            bytes += entries.len().saturating_sub(1);
            Some(bytes)
        }
    }
}

/// Pre-validation size bound for one held-screen field value: the exact
/// canonical width where spellable, with a `null` width (4) bound per
/// non-finite scalar. Unlike [`raw_setting_value_len`] (checked: `None`
/// when any nested number is non-finite), this never collapses carried
/// structure or text around one bad scalar, so the request estimator
/// cannot undercount the report. A bound only, never an encoding.
fn raw_setting_value_estimate(value: &crate::connection::RawSettingValue) -> usize {
    use crate::connection::RawSettingValue;
    match value {
        RawSettingValue::Null => 4,
        RawSettingValue::Bool(set) => bool_len(*set),
        RawSettingValue::Number(number) => json_number_len(*number).unwrap_or(4),
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

/// Exact canonical encoding for one held screen row: the retained carried
/// fields as a JSON object in stored order (canonical schema order at
/// decode: `out`, `full`, `work`, `complete`, `scale`, `identity`, then any
/// other carried keys lexicographically). The diagnostic is core-side only,
/// never on the wire. Checked: a non-finite retained number is an error,
/// never a silent `null`.
pub fn encode_held_screen_canonical(
    held: &crate::world_report::HeldScreenRow,
) -> Result<String, NonFiniteNumber> {
    let mut out = String::new();
    out.push('{');
    for (i, (key, value)) in held.fields.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        push_json_string(&mut out, key);
        out.push(':');
        push_raw_setting_value(&mut out, value)?;
    }
    out.push('}');
    Ok(out)
}

/// Exact canonical encoding for one held window row: the retained carried
/// fields as a JSON object in stored order (canonical schema order at
/// decode: `win`, `out`, `rect`, `fullscreen`, `maximized`, `sticky`,
/// `minimized`, `hints`, `nativeWs`, `hostHidden`, `privileged`, `managed`,
/// `acceptsInput`, `activationInhibited`, `toolWindow`, `owned`,
/// `transient`, `modal`, `resizable`, `above`, `below`, `owner`, `parent`,
/// `role`, `appId`, `executable`, `processId`, `class`, `title`, `type`,
/// `activationPolicy`, `layer`, `fullscreenOwner`, `fullscreenExitable`,
/// `suppressionAvailable`, `visibility`, `lastWrite`, then any other carried
/// keys lexicographically, with the nested `hints`, `visibility`, and
/// `lastWrite` objects in schema order). The diagnostic is core-side only,
/// never on the wire. Checked: a non-finite retained number is an error,
/// never a silent `null`.
pub fn encode_held_window_canonical(
    held: &crate::world_report::HeldWindowRow,
) -> Result<String, NonFiniteNumber> {
    let mut out = String::new();
    out.push('{');
    for (i, (key, value)) in held.fields.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        push_json_string(&mut out, key);
        out.push(':');
        push_raw_setting_value(&mut out, value)?;
    }
    out.push('}');
    Ok(out)
}

/// Exact canonical encoding for one valid window row: schema key order
/// (`win`, `out`, `rect`, `fullscreen`, `maximized`, `sticky`, `minimized`,
/// `hints`, `nativeWs`, `hostHidden`, `privileged`, `managed`, `acceptsInput`,
/// `activationInhibited`, `toolWindow`, `owned`, `transient`, `modal`,
/// `resizable`, `above`, `below`, `owner`, `parent`, `role`, `appId`,
/// `executable`, `processId`, `class`, `title`, `type`, `activationPolicy`,
/// `layer`, `fullscreenOwner`, `fullscreenExitable`, `suppressionAvailable`,
/// `visibility`, `lastWrite`, with the nested `hints`, `visibility`, and
/// `lastWrite` objects in schema order). Reuses the retained raw fields so
/// the spelling cannot drift from the held-row writer. `nativeWs` retains
/// its wire spelling (absent stays absent, `null` stays `null`, `""` stays
/// `""`, a known id stays itself; the immutable fixtures pin omission for
/// ordinary rows), every other nullable fact spells `null`. Witness for the
/// immutable message bytes: a decoded valid row must re-encode to the exact
/// carried row bytes. Checked shape shared with the held-row writer; integer
/// geometry and u64 counters are always finite so valid rows never fail.
pub fn encode_window_canonical(
    fact: &crate::world_report::WindowFact,
) -> Result<String, NonFiniteNumber> {
    let mut out = String::new();
    out.push('{');
    let mut first = true;
    for (key, value) in crate::world_report::retained_window_fields(fact) {
        if !first {
            out.push(',');
        }
        first = false;
        push_json_string(&mut out, &key);
        out.push(':');
        push_raw_setting_value(&mut out, &value)?;
    }
    out.push('}');
    Ok(out)
}

/// Exact canonical encoding for one valid screen row: schema key order
/// (`out`, `full`, `work`, `complete`, `scale`, `identity`) with
/// schema-ordered identity and shared canonical scalars. Witness for the
/// immutable message bytes: a decoded valid row must re-encode to the exact
/// carried row bytes. Checked: a non-finite scale is an error, never a
/// silent `null`.
pub fn encode_screen_canonical(
    screen: &crate::world_report::ScreenReport,
) -> Result<String, NonFiniteNumber> {
    fn push_rect(out: &mut String, rect: Option<crate::geometry::Rect>) {
        match rect {
            None => out.push_str("null"),
            Some(rect) => {
                out.push('[');
                out.push_str(&rect.x.to_string());
                out.push(',');
                out.push_str(&rect.y.to_string());
                out.push(',');
                out.push_str(&rect.w.to_string());
                out.push(',');
                out.push_str(&rect.h.to_string());
                out.push(']');
            }
        }
    }
    fn push_identity_text(out: &mut String, text: &Option<String>) {
        match text {
            None => out.push_str("null"),
            Some(text) => push_json_string(out, text),
        }
    }
    let mut out = String::new();
    out.push_str("{\"out\":");
    push_json_string(&mut out, &screen.output.0);
    out.push_str(",\"full\":");
    push_rect(&mut out, screen.full_rect);
    out.push_str(",\"work\":");
    push_rect(&mut out, screen.work_area);
    out.push_str(",\"complete\":");
    out.push_str(if screen.complete { "true" } else { "false" });
    out.push_str(",\"scale\":");
    push_json_number(&mut out, screen.scale)?;
    out.push_str(",\"identity\":{\"manufacturer\":");
    push_identity_text(&mut out, &screen.identity.manufacturer);
    out.push_str(",\"model\":");
    push_identity_text(&mut out, &screen.identity.model);
    out.push_str(",\"serial\":");
    push_identity_text(&mut out, &screen.identity.serial);
    out.push_str(",\"connector\":");
    push_identity_text(&mut out, &screen.identity.connector);
    out.push_str("}}");
    Ok(out)
}

/// Exact canonical reply encoding for the transactional commit gate.
/// Measures, rather than asserts, the wire size: overflow is decided on
/// these bytes. Global `u64` counters (`obs`, `rev`, effect `rev`) encode
/// as bounded decimal strings, never JSON numbers, so JavaScript adapters
/// never lose precision past 2^53.
///
/// Caller precondition (existing unchecked-`String` pattern, shared with the
/// protocol transport mirror): `validate_intent_writes_reason` on the
/// reply's `intent_writes` must be `None`. This writer spells whatever rows
/// it is given and never refuses; the engine publication gate enforces the
/// precondition before any commit or encode.
#[must_use]
pub fn encode_reply_canonical(reply: &WorldReply) -> String {
    let mut out = String::new();
    out.push_str("{\"v\":");
    out.push_str(&reply.contract_version.to_string());
    out.push_str(",\"adapter\":");
    push_json_string(&mut out, &reply.adapter_incarnation);
    out.push_str(",\"gen\":");
    push_json_string(&mut out, &reply.core_generation);
    out.push_str(",\"req\":");
    push_json_string(&mut out, &reply.request_id);
    out.push_str(",\"obs\":\"");
    out.push_str(&reply.based_on_observation_seq.to_string());
    out.push_str("\",\"rev\":\"");
    out.push_str(&reply.state_revision.to_string());
    out.push_str("\",\"ready\":");
    out.push_str(if reply.ready { "true" } else { "false" });
    out.push_str(",\"consumed\":");
    out.push_str(if reply.command_consumed {
        "true"
    } else {
        "false"
    });
    out.push_str(",\"scopes\":[");
    for (i, scope) in reply.scopes.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str("{\"scope\":");
        push_authority_scope(&mut out, &scope.scope);
        out.push_str(",\"auth\":");
        push_json_string(&mut out, scope.authority.as_str());
        out.push_str(",\"reason\":");
        push_json_string(
            &mut out,
            scope.authority.hold_reason().map_or("", HoldReason::as_str),
        );
        out.push('}');
    }
    out.push_str("],\"workspaces\":[");
    for (i, workspace) in reply.workspaces.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str("{\"id\":");
        push_json_string(&mut out, &workspace.id);
        out.push_str(",\"out\":");
        push_json_string(&mut out, &workspace.output.0);
        out.push_str(",\"bind\":");
        push_json_string(
            &mut out,
            workspace.native_binding.as_deref().unwrap_or_default(),
        );
        out.push_str(",\"scope\":");
        push_authority_scope(&mut out, &workspace.scope);
        out.push_str(",\"order\":");
        out.push_str(&workspace.order.to_string());
        out.push_str(",\"mode\":");
        push_json_string(&mut out, workspace.tiling_mode.as_str());
        out.push_str(",\"exist\":");
        out.push_str(if workspace.existence { "true" } else { "false" });
        out.push_str(",\"hold\":");
        push_json_string(&mut out, workspace.hold.map_or("", HoldReason::as_str));
        if workspace.trailing {
            out.push_str(",\"trailing\":true");
        }
        out.push('}');
    }
    out.push_str("],\"outputs\":[");
    for (i, output) in reply.outputs.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str("{\"out\":");
        push_json_string(&mut out, &output.output.0);
        out.push_str(",\"view\":");
        push_json_string(&mut out, output.logical_view.as_deref().unwrap_or_default());
        out.push_str(",\"mode\":");
        push_json_string(&mut out, output.tiling_mode.as_str());
        out.push_str(",\"hold\":");
        push_json_string(&mut out, output.hold.map_or("", HoldReason::as_str));
        out.push('}');
    }
    out.push_str("],\"windows\":[");
    for (i, window) in reply.windows.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str("{\"win\":");
        push_json_string(&mut out, &window.lifetime.0);
        out.push_str(",\"out\":");
        push_json_string(&mut out, &window.output.0);
        out.push_str(",\"ws\":");
        push_json_string(&mut out, &window.workspace);
        out.push_str(",\"alloc\":");
        match window.allocation {
            Some(rect) => {
                out.push_str(&format!("[{},{},{},{}]", rect.x, rect.y, rect.w, rect.h));
            }
            None => out.push_str("null"),
        }
        out.push_str(",\"frame\":");
        match window.effective_frame {
            Some(rect) => {
                out.push_str(&format!("[{},{},{},{}]", rect.x, rect.y, rect.w, rect.h));
            }
            None => out.push_str("null"),
        }
        out.push_str(",\"frame_auth\":");
        push_json_string(&mut out, window.frame_authority.as_str());
        out.push_str(",\"frame_hold\":");
        push_json_string(
            &mut out,
            window
                .frame_authority
                .hold_reason()
                .map_or("", HoldReason::as_str),
        );
        out.push_str(",\"member_auth\":");
        push_json_string(&mut out, window.membership_authority.as_str());
        out.push_str(",\"member_hold\":");
        push_json_string(
            &mut out,
            window
                .membership_authority
                .hold_reason()
                .map_or("", HoldReason::as_str),
        );
        out.push_str(",\"frame_op\":");
        push_json_string(
            &mut out,
            window.frame_operation.as_deref().unwrap_or_default(),
        );
        out.push_str(",\"floating\":");
        out.push_str(if window.floating { "true" } else { "false" });
        out.push_str(",\"float_origin\":");
        push_json_string(
            &mut out,
            window.float_origin.map_or("", FloatOrigin::as_str),
        );
        out.push_str(",\"clear_op\":");
        push_json_string(
            &mut out,
            window.clear_operation.as_deref().unwrap_or_default(),
        );
        encode_window_target_lanes(&mut out, window);
        out.push('}');
    }
    out.push_str("],\"transfers\":[");
    for (i, transfer) in reply.transfers.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str("{\"op\":");
        push_json_string(&mut out, &transfer.operation_id);
        out.push_str(",\"kind\":");
        push_json_string(
            &mut out,
            match &transfer.kind {
                TransferKind::Window(_) => "window",
                TransferKind::View => "view",
                TransferKind::Migration => "migration",
            },
        );
        out.push_str(",\"win\":");
        match &transfer.kind {
            TransferKind::Window(window) => push_json_string(&mut out, &window.0),
            TransferKind::View | TransferKind::Migration => out.push_str("\"\""),
        }
        out.push_str(",\"out\":");
        push_json_string(&mut out, &transfer.target_output.0);
        out.push_str(",\"ws\":");
        push_json_string(&mut out, &transfer.target_workspace);
        out.push_str(",\"follow\":");
        out.push_str(if transfer.follow { "true" } else { "false" });
        out.push_str(",\"held\":");
        out.push_str(if transfer.held { "true" } else { "false" });
        out.push('}');
    }
    out.push_str("],\"effects\":[");
    for (i, effect) in reply.effects.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str("{\"id\":");
        push_json_string(&mut out, &effect.id);
        out.push_str(",\"kind\":");
        push_json_string(&mut out, effect.kind.as_str());
        out.push_str(",\"target\":");
        push_json_string(&mut out, &effect.target.0);
        out.push_str(",\"rev\":\"");
        out.push_str(&effect.issued_revision.to_string());
        out.push_str("\"}");
    }
    out.push_str("],\"result\":");
    encode_command_result(&mut out, &reply.command_result);
    out.push_str(",\"gestureSettled\":");
    encode_gesture_settled(&mut out, &reply.gesture_settled);
    out.push_str(",\"operationSettled\":");
    encode_operation_settled(&mut out, &reply.operation_settled);
    out.push_str(",\"nativeCreations\":");
    encode_native_creations(&mut out, &reply.native_creations);
    out.push_str(",\"warpCursor\":");
    match reply.cursor_warp {
        Some((x, y)) => {
            out.push_str(&format!("[{x},{y}]"));
        }
        None => out.push_str("null"),
    }
    out.push_str(",\"presentation\":");
    encode_presentation(&mut out, &reply.presentation);
    out.push_str(",\"intentWrites\":[");
    for (i, write) in reply.intent_writes.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str("{\"win\":");
        push_json_string(&mut out, &write.win.0);
        out.push_str(",\"data\":");
        match write.data.as_deref() {
            Some(data) => push_json_string(&mut out, data),
            None => out.push_str("null"),
        }
        out.push('}');
    }
    out.push_str("],\"diagnostics\":[");
    for (i, diagnostic) in reply.diagnostics.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        push_row_diagnostic(&mut out, diagnostic);
    }
    out.push(']');
    out.push('}');
    out
}

/// Exact canonical encoding for one narrow-hold row diagnostic, with
/// frozen key order (mirrored by [`row_diagnostic_estimate`] and the
/// protocol codec): `{"scope":..,"row":..,"id":..,"kind":..}` where a
/// missing id encodes as JSON null (rows without identity refuse the
/// envelope instead of holding, so only lanes with nullable ids emit
/// here; never the empty string).
pub fn push_row_diagnostic(out: &mut String, diagnostic: &crate::world_report::RowDiagnostic) {
    out.push_str("{\"scope\":");
    push_authority_scope(out, &diagnostic.scope);
    out.push_str(",\"row\":");
    push_json_string(out, diagnostic.row.as_str());
    out.push_str(",\"id\":");
    match diagnostic.id.as_deref() {
        Some(id) => push_json_string(out, id),
        None => out.push_str("null"),
    }
    out.push_str(",\"kind\":");
    push_json_string(out, diagnostic.kind.as_str());
    out.push('}');
}

/// Exact canonical encoding for the native creation desire section: `[]`
/// when no creation is authorized, else one object per outstanding
/// authorization with frozen key order (mirrored by
/// [`native_creations_estimate`] and the protocol codec).
pub fn encode_native_creations(
    out: &mut String,
    creations: &[crate::world_report::NativeCreationDesire],
) {
    out.push('[');
    for (i, creation) in creations.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str("{\"op\":");
        push_json_string(out, &creation.operation_id);
        out.push_str(",\"ws\":");
        push_json_string(out, &creation.logical_workspace);
        out.push_str(",\"out\":");
        push_json_string(out, &creation.target_output.0);
        out.push('}');
    }
    out.push(']');
}

/// Exact canonical encoding for the optional gesture-settled section:
/// `null` when this report incorporated no terminal, else the stable id
/// (decimal string) plus the commit flag, with frozen key order (mirrored
/// by [`gesture_settled_estimate`] and the protocol codec).
pub fn encode_gesture_settled(out: &mut String, settled: &Option<GestureSettled>) {
    let Some(terminal) = settled else {
        out.push_str("null");
        return;
    };
    out.push_str("{\"id\":\"");
    out.push_str(&terminal.id.to_string());
    out.push_str("\",\"committed\":");
    out.push_str(if terminal.committed { "true" } else { "false" });
    out.push('}');
}

/// Exact canonical encoding for the operation-settled section: `[]` when no
/// retained clear binding or float placement retired on this report, else
/// one object per retired item with frozen key order (mirrored by
/// [`operation_settled_estimate`] and the protocol codec). Public so the
/// protocol transport gate reuses the identical bytes.
pub fn encode_operation_settled(out: &mut String, settled: &[crate::world_report::OperationEvent]) {
    out.push('[');
    for (i, event) in settled.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str("{\"op\":");
        push_json_string(out, &event.operation_id);
        out.push_str(",\"win\":");
        push_json_string(out, &event.window.0);
        out.push_str(",\"outcome\":");
        push_json_string(out, event.outcome.as_str());
        out.push_str(",\"cause\":");
        push_json_string(out, event.cause.as_deref().unwrap_or_default());
        out.push('}');
    }
    out.push(']');
}

/// Exact canonical encoding for the uniform presentation section: the
/// tick-time foreground token plus the border shape, the underlay shape,
/// and the pager content, with frozen key order (mirrored by
/// [`presentation_estimate`] and the protocol codec). Public so the
/// protocol transport gate reuses the identical bytes.
pub fn encode_presentation(
    out: &mut String,
    presentation: &crate::world_presentation::WorldPresentation,
) {
    use crate::world_presentation::PresentationUnderlay;
    out.push_str("{\"foreground\":");
    push_json_string(out, &presentation.foreground);
    // Neutral focused-fullscreen suspension verdict: omitted entirely when
    // absent so default publication bytes stay exact.
    if let Some(output) = presentation.suspended_output.as_ref() {
        out.push_str(",\"suspendedOutput\":");
        push_json_string(out, &output.0);
    }
    out.push_str(",\"border\":{");
    out.push_str("\"visible\":");
    out.push_str(if presentation.border.visible {
        "true"
    } else {
        "false"
    });
    out.push_str(",\"reason\":");
    push_json_string(
        out,
        presentation
            .border
            .reason
            .map_or("", crate::visual::BorderHideReason::as_str),
    );
    out.push_str(",\"inner\":");
    match presentation.border.inner {
        Some(rect) => {
            out.push_str(&format!("[{},{},{},{}]", rect.x, rect.y, rect.w, rect.h));
        }
        None => out.push_str("null"),
    }
    out.push_str(",\"outer\":");
    match presentation.border.outer {
        Some(rect) => {
            out.push_str(&format!("[{},{},{},{}]", rect.x, rect.y, rect.w, rect.h));
        }
        None => out.push_str("null"),
    }
    out.push_str("},\"underlay\":");
    match &presentation.underlay {
        PresentationUnderlay::Found(found) => {
            out.push_str("{\"status\":\"found\",\"reason\":\"\",\"baseRev\":\"");
            out.push_str(&found.base_revision.to_string());
            out.push_str("\",\"homeOut\":");
            push_json_string(out, &found.home.output.0);
            out.push_str(",\"homeWs\":");
            push_json_string(out, &found.home.workspace.0);
            out.push_str(",\"group\":");
            push_json_string(out, &found.group.0);
            out.push_str(",\"leaf\":");
            push_json_string(out, &found.focused_leaf.0);
            out.push_str(",\"win\":");
            push_json_string(out, &found.focused_window.0);
            out.push_str(",\"union\":");
            out.push_str(&format!(
                "[{},{},{},{}]",
                found.union_rect.x, found.union_rect.y, found.union_rect.w, found.union_rect.h
            ));
            out.push_str(",\"outer\":");
            out.push_str(&format!(
                "[{},{},{},{}]",
                found.outer_rect.x, found.outer_rect.y, found.outer_rect.w, found.outer_rect.h
            ));
            out.push_str(",\"members\":[");
            for (i, member) in found.members.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str("{\"leaf\":");
                push_json_string(out, &member.leaf.0);
                out.push_str(",\"win\":");
                push_json_string(out, &member.window.0);
                out.push_str(",\"rect\":");
                out.push_str(&format!(
                    "[{},{},{},{}]",
                    member.rect.x, member.rect.y, member.rect.w, member.rect.h
                ));
                out.push('}');
            }
            out.push_str("]}");
        }
        PresentationUnderlay::Hidden {
            base_revision,
            reason,
        } => {
            out.push_str("{\"status\":\"hidden\",\"reason\":");
            push_json_string(out, reason.as_str());
            out.push_str(",\"baseRev\":");
            match base_revision {
                Some(rev) => {
                    out.push('"');
                    out.push_str(&rev.to_string());
                    out.push('"');
                }
                None => out.push_str("null"),
            }
            out.push('}');
        }
    }
    out.push_str(",\"pager\":[");
    for (i, screen) in presentation.pager.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str("{\"out\":");
        push_json_string(out, &screen.output);
        out.push_str(",\"workspaces\":[");
        for (j, workspace) in screen.workspaces.iter().enumerate() {
            if j > 0 {
                out.push(',');
            }
            out.push_str("{\"id\":");
            push_json_string(out, &workspace.id);
            out.push_str(",\"ordinal\":");
            out.push_str(&workspace.ordinal.to_string());
            out.push_str(",\"label\":");
            push_json_string(out, &workspace.label);
            out.push_str(",\"current\":");
            out.push_str(if workspace.current { "true" } else { "false" });
            out.push_str(",\"occupied\":");
            out.push_str(if workspace.occupied { "true" } else { "false" });
            out.push_str(",\"containsFocused\":");
            out.push_str(if workspace.contains_focused {
                "true"
            } else {
                "false"
            });
            if workspace.trailing {
                out.push_str(",\"trailing\":true");
            }
            out.push_str(",\"windows\":[");
            for (k, window) in workspace.windows.iter().enumerate() {
                if k > 0 {
                    out.push(',');
                }
                out.push_str("{\"win\":");
                push_json_string(out, &window.window.0);
                out.push_str(",\"frame\":");
                out.push_str(&format!(
                    "[{},{},{},{}]",
                    window.frame.x, window.frame.y, window.frame.w, window.frame.h
                ));
                out.push('}');
            }
            out.push_str("]}");
        }
        out.push_str("]}");
    }
    out.push_str("]}");
}

/// Exact canonical encoding for the correlated `command_result` section:
/// `null` when absent, else the per-op object with frozen canonical key
/// order (mirrored by [`command_result_estimate`] and the protocol codec).
/// Every `u64` counter encodes as a decimal string, never a JSON number.
/// Public so the protocol transport gate reuses the identical bytes.
pub fn encode_command_result(out: &mut String, result: &Option<CommandResult>) {
    use crate::world_report::CommandResult as Result;
    let Some(result) = result else {
        out.push_str("null");
        return;
    };
    let push_rect = |out: &mut String, r: crate::geometry::Rect| {
        out.push_str(&format!("[{},{},{},{}]", r.x, r.y, r.w, r.h));
    };
    match result {
        Result::DragPreview {
            rect: proposed,
            hover,
            domain,
            source,
            target,
            revision,
        } => {
            out.push_str("{\"op\":\"drag-preview\",\"rev\":\"");
            out.push_str(&revision.to_string());
            out.push_str("\",\"domainOut\":");
            push_json_string(out, &domain.output.0);
            out.push_str(",\"domainWs\":");
            push_json_string(out, &domain.workspace.0);
            out.push_str(",\"sourceOut\":");
            push_json_string(out, &source.output.0);
            out.push_str(",\"sourceWs\":");
            push_json_string(out, &source.workspace.0);
            out.push_str(",\"rect\":");
            push_rect(out, *proposed);
            out.push_str(",\"hover\":");
            match hover {
                None => out.push_str("null"),
                Some(hover) => {
                    out.push_str("{\"domainOut\":");
                    push_json_string(out, &hover.domain.output.0);
                    out.push_str(",\"domainWs\":");
                    push_json_string(out, &hover.domain.workspace.0);
                    out.push_str(",\"leaf\":");
                    push_json_string(out, &hover.source_leaf.0);
                    out.push_str(",\"win\":");
                    push_json_string(out, &hover.source_window.0);
                    out.push_str(",\"rev\":\"");
                    out.push_str(&hover.revision.to_string());
                    out.push_str("\",\"prior\":");
                    match &hover.prior {
                        None => out.push_str("null"),
                        Some(prior) => {
                            out.push_str("{\"group\":");
                            push_json_string(out, &prior.group.0);
                            out.push_str(",\"edge\":");
                            push_json_string(out, prior.edge.as_str());
                            out.push('}');
                        }
                    }
                    out.push_str(",\"priorOut\":");
                    push_json_string(out, &hover.prior_domain.output.0);
                    out.push_str(",\"priorWs\":");
                    push_json_string(out, &hover.prior_domain.workspace.0);
                    out.push('}');
                }
            }
            out.push_str(",\"win\":");
            push_json_string(out, &target.0);
            out.push('}');
        }
        Result::Float {
            window,
            rect,
            no_space,
        } => {
            out.push_str("{\"op\":\"toggle-float\",\"win\":");
            push_json_string(out, &window.0);
            out.push_str(",\"rect\":");
            match rect {
                Some(r) => push_rect(out, *r),
                None => out.push_str("null"),
            }
            // Explicit-tile no-space retry marker: present only when true
            // so older successful placements stay byte-identical.
            if *no_space {
                out.push_str(",\"noSpace\":true");
            }
            out.push('}');
        }
        Result::Migration {
            direction,
            source,
            target,
            members,
            floats,
        } => {
            out.push_str("{\"op\":\"migrate-workspace\",\"direction\":");
            push_json_string(
                out,
                match direction {
                    Direction::Left => "left",
                    Direction::Right => "right",
                    Direction::Up => "up",
                    Direction::Down => "down",
                },
            );
            out.push_str(",\"sourceOut\":");
            push_json_string(out, &source.output.0);
            out.push_str(",\"sourceWs\":");
            push_json_string(out, &source.workspace.0);
            out.push_str(",\"targetOut\":");
            push_json_string(out, &target.output.0);
            out.push_str(",\"targetWs\":");
            push_json_string(out, &target.workspace.0);
            out.push_str(",\"members\":[");
            for (i, member) in members.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                push_json_string(out, &member.0);
            }
            out.push_str("],\"floats\":");
            out.push_str(&floats.to_string());
            out.push('}');
        }
        Result::Directional {
            outcome,
            target,
            rect,
            cause,
        } => {
            out.push_str("{\"op\":\"directional\",\"outcome\":");
            push_json_string(out, outcome.as_str());
            out.push_str(",\"win\":");
            match target {
                Some(t) => push_json_string(out, &t.0),
                None => out.push_str("null"),
            }
            out.push_str(",\"rect\":");
            match rect {
                Some(r) => push_rect(out, *r),
                None => out.push_str("null"),
            }
            out.push_str(",\"cause\":");
            match cause {
                Some(c) => push_json_string(out, c),
                None => out.push_str("null"),
            }
            out.push('}');
        }
        Result::Maximize {
            window,
            maximized,
            clear_fullscreen,
            was_maximized,
        } => {
            out.push_str("{\"op\":\"maximize\",\"win\":");
            push_json_string(out, &window.0);
            out.push_str(",\"maximized\":");
            out.push_str(if *maximized { "true" } else { "false" });
            out.push_str(",\"clearFullscreen\":");
            out.push_str(if *clear_fullscreen { "true" } else { "false" });
            out.push_str(",\"wasMaximized\":");
            out.push_str(if *was_maximized { "true" } else { "false" });
            out.push('}');
        }
        Result::Rejected { kind } => {
            out.push_str("{\"op\":\"rejected\",\"kind\":");
            push_json_string(out, kind.as_str());
            out.push('}');
        }
    }
}

/// Exact canonical reply byte size for the transactional commit gate.
#[must_use]
pub fn reply_exact_bytes(reply: &WorldReply) -> usize {
    encode_reply_canonical(reply).len()
}

/// Maximum per-offroute-native-state-attempt diagnostic cost: scope (155)
/// plus lane/kind tokens (30) plus id (130) plus punctuation (30) = 345,
/// rounded to 512. Independently derived like the held-screen row cost, never
/// borrowed from other headroom.
pub const MAX_STATE_ATTEMPT_DIAGNOSTIC_BYTES: usize = 512;

/// Maximum per-published-window outbound target lane cost for the current
/// engine: the composed sticky show plus at most one native sticky state
/// target dominate the default suffix. The worst row carries the longest
/// quoted command correlation (`,"cmdSeq":"18446744073709551615"`, 32),
/// the composed show object (`,"visibility":{"shown":true,
/// "rev":"18446744073709551615","fallback":false}`, 14 + 31 + 4 + 20 + 5
/// = 74), the longest authority/hold pair (`Hold(IncompleteInventory)`:
/// `,"visibility_auth":"hold"` (25) plus
/// `,"visibility_hold":"incomplete-inventory"` (41)), the state array with
/// one sticky row (`,"state":[{...}]`, 11 plus 113, see below): 32 + 74 +
/// 25 + 41 + 11 + 113 = 296. The largest single sticky state row is a
/// `Hold` with the longest hold reason, a false value, and the longest
/// quoted command sequence: 50 + 8 (`"sticky"`) + 5 (`false`) + 6
/// (`"hold"`) + 22 (`"incomplete-inventory"`) + 22 (quoted `u64::MAX`) =
/// 113. The common `Controlled` show with a controlled sticky target costs
/// less; the precomposed default suffix costs 94 with null correlation and
/// 112 with the longest quoted counter. No speculative extra state budget;
/// the exact staged gate stays the backstop.
pub const MAX_TARGET_LANES_PER_WINDOW: usize = 296;

/// Conservative pre-mutation reply upper bound for one report: the request
/// estimate plus worst-case generated state. Any publishable reply for this
/// input fits within this bound, so exceeding it rejects before any
/// observation convergence, slot cleanup, or command mutation.
///
/// Per-item slack derives from the canonical encoding with maximum-length
/// (128) identifiers:
/// - retained transfer row: 63 punctuation + `op-<u64>` (25) + kind (11)
///   + three max ids (390) = 489, rounded to 512;
/// - created logical workspace row: 65 punctuation + id (132) + output
///   (130) + empty binding (2) + scope (130) + order (20) + mode (7) +
///   hold (23) = 509, rounded to 512;
/// - retained-missing held scope row: 28 punctuation + `output/workspace`
///   (259) + authority (12) + reason (23) = 322, rounded to 336;
/// - one emitted effect row (at most one per command): 32 + id (25) +
///   kind (9) + target (130) + revision (20) = 216, rounded to 256.
/// - correlated command result (at most one per command): the drag-preview
///   / float shapes fit in 2048; a migration member list scales with the
///   report's live window count (one max-length id + comma per window, 131
///   each) plus 512 base.
/// - uniform presentation: the border shape fits in 256; the underlay
///   members scale with the report's live window count (one max-length
///   leaf/window pair plus rect and comma per window, 300 each) plus 1024
///   base; pager workspaces scale with the logical workspace count (one
///   max-length id/label plus ordinal/flags per workspace, 320 each, plus
///   16 outputs of slack) and pager windows scale with the live window
///   count (one max-length id plus frame and comma per window, 160 each).
///   The optional explicit trailing marker (`,\"trailing\":true`, 16 bytes)
///   rides at most one plan row plus one pager row per logical workspace
///   (32 each).
/// - gesture-settled terminal (at most one per report): 40 fixed plus a
///   20-digit id plus a 5-byte bool = 65 worst case, rounded to 96.
/// - operation-settled events (at most two per window per report: one
///   clear verdict plus one float verdict): 34 punctuation +
///   `op-<u64>` (25) + window (130) + outcome (27) + cause (130) = 346,
///   rounded to 384 each, doubled to 768 per window.
/// - native creation desires (at most one per logical workspace): 22
///   punctuation + `op-native-<n>` (25) + logical (130) + output (130) =
///   307, rounded to 384 each.
/// - held-row diagnostics (at most one per held screen row): scope (155) plus
///   lane/kind tokens (30) plus id (130) plus punctuation (30) = 345,
///   rounded to 512 each.
/// - offroute native-state attempt diagnostics (at most one per carried
///   `stateAttempts` row): [`MAX_STATE_ATTEMPT_DIAGNOSTIC_BYTES`] each,
///   independently derived like the held-screen row cost.
/// - core intent save/clear section (`intentWrites`): the empty 18-byte
///   section is covered by the 4096 envelope slack below. The at-most-one
///   sticky/float save row reserves [`max_intent_save_bound`] (16
///   punctuation plus the command's subject id plus
///   [`MAX_INTENT_SAVE_DATA_JSON`], at most 194), gated on the report's
///   own command (`SetSticky`, `ToggleSticky`, or the sticky-clearing
///   `ToggleFloat`) so non-save reports reserve nothing.
/// - outbound target lanes (current engine only): each published window
///   carries at most one composed sticky show plus the command correlation
///   and at most one native sticky state target (296 worst case with the
///   longest quoted counter, longest revision, longest authority/hold pair,
///   and largest sticky state row; see [`MAX_TARGET_LANES_PER_WINDOW`]).
///   The default suffix costs 94 with a null correlation and 112 with the
///   longest quoted counter. The count is the union of every
///   publication source, summed as a conservative upper bound (deduped
///   lifetimes counted twice still fit): the current carried windows plus
///   held rows, plus the prior staged windows (held scopes publish retained
///   lifetimes without a live fact in this report), plus the retained
///   session/member footprint (tiled links plus exceptions plus minimize
///   slots across retained domains, which survive a gapped previous report
///   that left staged small), plus retained mover rows (one per outstanding
///   window transfer plus every migration member). No per-window headroom
///   beyond one composed show plus one sticky state target and no
///   operation-settled headroom reuse.
/// - envelope slack: 4096 base covers envelope slack and array commas.
#[must_use]
pub fn reply_upper_bound(
    report: &WorldReport,
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
    report_estimate(report)
        .saturating_add(retained_transfers.saturating_mul(512))
        .saturating_add(created_workspaces.saturating_mul(512))
        .saturating_add(logical_workspaces.saturating_mul(336))
        .saturating_add(4096)
        .saturating_add(256)
        .saturating_add(2048)
        .saturating_add(512)
        .saturating_add(256)
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
        .saturating_add(max_intent_save_bound(report))
        .saturating_add(published_windows.saturating_mul(MAX_TARGET_LANES_PER_WINDOW))
}

#[cfg(test)]
mod typed_scope_sizing_tests {
    use super::*;
    use crate::world_report::{AuthorityScope, ScopeAuthority, ScopeStatus};

    #[test]
    fn scope_estimate_matches_exact_isolated_scope_row() {
        let row = ScopeStatus {
            scope: AuthorityScope::Workspace {
                output: crate::directional::OutputId("out-1".into()),
                workspace: crate::directional::WorkspaceId("ws-2".into()),
            },
            authority: ScopeAuthority::Controlled,
        };
        let exact = "{\"scope\":{\"kind\":\"workspace\",\"out\":\"out-1\",\"ws\":\"ws-2\"},\"auth\":\"controlled\",\"reason\":\"\"}";
        assert_eq!(scope_status_estimate(&row), exact.len());
    }
}

#[cfg(test)]
mod canonical_scalar_tests {
    use super::*;
    use crate::connection::RawSettingValue;

    fn encoded_number(value: f64) -> String {
        let mut out = String::new();
        push_json_number(&mut out, value).expect("finite test scalar");
        out
    }

    fn encoded_string(value: &str) -> String {
        let mut out = String::new();
        push_json_string(&mut out, value);
        out
    }

    #[test]
    #[allow(
        clippy::excessive_precision,
        reason = "the tie witnesses intentionally carry full decimal precision; clippy's shorter suggestion does not round-trip"
    )]
    fn halfway_ties_use_shortest_even_digits() {
        // Journaled witness: Rust `Display` spells the 2^50 halfway case
        // `.3`; the shortest round-trip spelling ends `.2`.
        assert_eq!(
            encode_finite_json_number(1000000000000000.25),
            "1000000000000000.2"
        );
        assert_eq!(
            encode_finite_json_number(1000000000000000.75),
            "1000000000000000.8"
        );
        for text in ["1000000000000000.2", "1000000000000000.8"] {
            let parsed: f64 = text.parse().expect("parses");
            assert_eq!(encoded_number(parsed), text);
        }
    }

    #[test]
    fn signed_zero_spells_zero() {
        assert_eq!(encode_finite_json_number(-0.0), "0");
        assert_eq!(encoded_number(-0.0), "0");
        assert_eq!(json_number_len(-0.0), Some(1));
    }

    #[test]
    fn small_large_and_boundary_exponents_expand_plainly() {
        assert_eq!(encode_finite_json_number(1e21), "1000000000000000000000");
        assert_eq!(encode_finite_json_number(1.5e-7), "0.00000015");
        assert_eq!(encode_finite_json_number(0.000001), "0.000001");
        assert_eq!(encode_finite_json_number(1.25), "1.25");
        assert_eq!(encode_finite_json_number(-1.0), "-1");
        let smallest = encode_finite_json_number(5e-324);
        assert_eq!(smallest.len(), 326);
        assert!(smallest.starts_with("0.000"));
        assert!(smallest.ends_with('5'));
        let max = encode_finite_json_number(f64::MAX);
        assert_eq!(max.len(), 309);
        assert!(max.starts_with("17976931348623157"));
        assert!(!max.contains(['e', 'E', '.', '+', '-']));
    }

    #[test]
    fn non_finite_scalars_are_checked_errors_never_null() {
        // Finding A: non-finite typed facts must never normalize to
        // `null`. Every canonical scalar/raw writer reports the value and
        // emits nothing; only the pre-validation estimator substitutes a
        // pure size bound.
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let mut out = String::new();
            assert_eq!(
                push_json_number(&mut out, value),
                Err(super::NonFiniteNumber)
            );
            assert!(out.is_empty(), "no silent null normalization");
            assert_eq!(json_number_len(value), None);
            for held in [
                RawSettingValue::Number(value),
                RawSettingValue::Array(vec![
                    RawSettingValue::Number(1.0),
                    RawSettingValue::Number(value),
                ]),
                RawSettingValue::Object(vec![("scale".to_owned(), RawSettingValue::Number(value))]),
            ] {
                let mut raw = String::new();
                assert_eq!(
                    push_raw_setting_value(&mut raw, &held),
                    Err(super::NonFiniteNumber)
                );
                assert!(raw.is_empty(), "no silent null normalization");
                assert_eq!(raw_setting_value_len(&held), None);
            }
        }
    }

    #[test]
    fn non_finite_rows_are_checked_errors_never_null() {
        use crate::directional::OutputId;
        use crate::world_report::{
            HeldScreenRow, OutputIdentity, RowDiagnostic, RowDiagnosticKind, RowDiagnosticLane,
            ScreenReport,
        };
        // A non-finite scale never encodes as a valid row.
        let screen = ScreenReport {
            output: OutputId("out".to_owned()),
            full_rect: None,
            work_area: None,
            complete: false,
            scale: f64::NAN,
            identity: OutputIdentity::unknown(),
        };
        let mut out = String::new();
        assert_eq!(
            push_json_number(&mut out, screen.scale),
            Err(super::NonFiniteNumber)
        );
        assert!(encode_screen_canonical(&screen).is_err());
        // A non-finite retained field never encodes as a held row.
        let held = HeldScreenRow {
            output: OutputId("out".to_owned()),
            ordinal: 0,
            fields: vec![("scale".to_owned(), RawSettingValue::Number(f64::INFINITY))],
            diagnostic: RowDiagnostic {
                scope: crate::world_report::AuthorityScope::Output(OutputId("out".to_owned())),
                row: RowDiagnosticLane::Screen,
                id: Some("out".to_owned()),
                kind: RowDiagnosticKind::InvalidFact,
            },
        };
        assert!(encode_held_screen_canonical(&held).is_err());
    }

    #[test]
    #[allow(
        clippy::excessive_precision,
        reason = "the tie witness intentionally carries full decimal precision"
    )]
    fn number_len_matches_encoding() {
        for value in [
            0.0,
            -0.0,
            1.0,
            -1.0,
            1.25,
            1.5,
            100.0,
            0.1 + 0.2,
            1e21,
            1.5e-7,
            5e-324,
            f64::MAX,
            f64::MIN_POSITIVE,
            1000000000000000.25,
        ] {
            assert_eq!(
                json_number_len(value).expect("finite test scalar"),
                encode_finite_json_number(value).len(),
                "{value}"
            );
            assert_eq!(encoded_number(value), encode_finite_json_number(value));
        }
    }

    #[test]
    fn strings_preserve_multibyte_and_escape_c0() {
        assert_eq!(encoded_string("Müller-µ-日本語-🎛"), "\"Müller-µ-日本語-🎛\"");
        assert_eq!(
            encoded_string("quote\"back\\slash\0\u{1}\u{1f}newline\n"),
            "\"quote\\\"back\\\\slash\\u0000\\u0001\\u001fnewline\\u000a\""
        );
        assert_eq!(encoded_string(""), "\"\"");
        for text in [
            "Müller-µ-日本語-🎛",
            "quote\"back\\slash\0\u{1f}\n",
            "",
            "plain",
        ] {
            assert_eq!(
                json_string_len(text),
                encoded_string(text).len(),
                "{text:?}"
            );
        }
    }

    #[test]
    fn string_len_counts_utf8_bytes_at_identity_bound() {
        let exact = format!("{}é", "a".repeat(254));
        assert_eq!(exact.len(), 256);
        assert_eq!(json_string_len(&exact), 258);
        let over = format!("{}é", "a".repeat(255));
        assert_eq!(over.len(), 257);
        assert_eq!(json_string_len(&over), 259);
        assert_eq!(encoded_string(&over).len(), json_string_len(&over));
    }
}

#[cfg(test)]
mod target_lane_sizing_tests {
    use super::*;
    use crate::directional::{OutputId, WindowId};
    use crate::world_report::{
        HoldReason, ScopeAuthority, StateProperty, StateTarget, VisibilityTarget, WindowDesire,
    };

    fn lane_window(
        command_seq: Option<u64>,
        visibility: Option<VisibilityTarget>,
        visibility_authority: ScopeAuthority,
        state: Vec<StateTarget>,
    ) -> WindowDesire {
        WindowDesire {
            lifetime: WindowId("win-a".to_owned()),
            output: OutputId("out-a".to_owned()),
            workspace: "ws-a".to_owned(),
            allocation: None,
            effective_frame: None,
            frame_authority: ScopeAuthority::Untouched,
            membership_authority: ScopeAuthority::Untouched,
            floating: false,
            float_origin: None,
            frame_operation: None,
            clear_operation: None,
            command_seq,
            visibility,
            visibility_authority,
            state,
        }
    }

    fn encoded_lanes(window: &WindowDesire) -> String {
        let mut out = String::new();
        encode_window_target_lanes(&mut out, window);
        out
    }

    fn all_authorities() -> Vec<ScopeAuthority> {
        let mut authorities = vec![ScopeAuthority::Controlled, ScopeAuthority::Untouched];
        for reason in [
            HoldReason::Unreadable,
            HoldReason::IncompleteInventory,
            HoldReason::GestureBarrier,
            HoldReason::RefusedTarget,
            HoldReason::AmbiguousCreation,
            HoldReason::OccupiedRemoval,
            HoldReason::ConstraintClamp,
            HoldReason::Minimized,
        ] {
            authorities.push(ScopeAuthority::Hold(reason));
        }
        authorities
    }

    #[test]
    fn estimate_matches_encoding_across_lane_matrix() {
        let properties = [
            StateProperty::Fullscreen,
            StateProperty::Maximized,
            StateProperty::Sticky,
            StateProperty::Above,
        ];
        let revisions = [0, 1, u64::MAX];
        let seqs = [None, Some(0), Some(1), Some(u64::MAX)];
        for authority in all_authorities() {
            for seq in seqs {
                for shown in [false, true] {
                    for fallback in [false, true] {
                        for revision in revisions {
                            let window = lane_window(
                                seq,
                                Some(VisibilityTarget {
                                    shown,
                                    revision,
                                    fallback,
                                }),
                                authority,
                                Vec::new(),
                            );
                            assert_eq!(
                                window_target_lanes_estimate(&window),
                                encoded_lanes(&window).len(),
                                "visibility {shown}/{fallback}/{revision} {authority:?} {seq:?}"
                            );
                        }
                    }
                }
                let window = lane_window(seq, None, authority, Vec::new());
                assert_eq!(
                    window_target_lanes_estimate(&window),
                    encoded_lanes(&window).len(),
                    "null visibility {authority:?} {seq:?}"
                );
                for property in properties {
                    for value in [false, true] {
                        for state_seq in seqs {
                            let window = lane_window(
                                seq,
                                None,
                                authority,
                                vec![StateTarget {
                                    property,
                                    value,
                                    authority,
                                    command_seq: state_seq,
                                }],
                            );
                            assert_eq!(
                                window_target_lanes_estimate(&window),
                                encoded_lanes(&window).len(),
                                "state {property:?}/{value} {authority:?} {seq:?}/{state_seq:?}"
                            );
                        }
                    }
                }
            }
        }
        // Multi-row state arrays count one comma per extra row.
        let multi = lane_window(
            Some(7),
            Some(VisibilityTarget {
                shown: true,
                revision: 1,
                fallback: false,
            }),
            ScopeAuthority::Controlled,
            vec![
                StateTarget {
                    property: StateProperty::Above,
                    value: true,
                    authority: ScopeAuthority::Controlled,
                    command_seq: None,
                },
                StateTarget {
                    property: StateProperty::Fullscreen,
                    value: false,
                    authority: ScopeAuthority::Hold(HoldReason::RefusedTarget),
                    command_seq: Some(1),
                },
                StateTarget {
                    property: StateProperty::Maximized,
                    value: true,
                    authority: ScopeAuthority::Hold(HoldReason::IncompleteInventory),
                    command_seq: Some(u64::MAX),
                },
            ],
        );
        assert_eq!(
            window_target_lanes_estimate(&multi),
            encoded_lanes(&multi).len(),
            "multi-row lanes stay exact"
        );
    }

    #[test]
    fn engine_default_lanes_cost_94_and_per_window_max_296() {
        // The current engine emits null visibility, untouched authority,
        // and an empty state array on every row: 94 bytes with a null
        // correlation, 112 with the longest quoted counter. The
        // per-published-window pre-mutation reserve is wider: it also
        // covers the worst composed sticky show plus the largest single
        // native sticky state row (see below).
        let null = lane_window(None, None, ScopeAuthority::Untouched, Vec::new());
        let null_bytes = encoded_lanes(&null);
        assert_eq!(
            null_bytes,
            ",\"cmdSeq\":null,\"visibility\":null,\
             \"visibility_auth\":\"untouched\",\"visibility_hold\":\"\",\
             \"state\":[]"
        );
        assert_eq!(null_bytes.len(), 94);
        assert_eq!(window_target_lanes_estimate(&null), 94);
        let max = lane_window(Some(u64::MAX), None, ScopeAuthority::Untouched, Vec::new());
        let max_bytes = encoded_lanes(&max);
        assert_eq!(
            max_bytes,
            ",\"cmdSeq\":\"18446744073709551615\",\"visibility\":null,\
             \"visibility_auth\":\"untouched\",\"visibility_hold\":\"\",\
             \"state\":[]"
        );
        assert_eq!(max_bytes.len(), 112);
        assert_eq!(window_target_lanes_estimate(&max), 112);
        assert_eq!(MAX_TARGET_LANES_PER_WINDOW, 296);
    }

    #[test]
    fn composed_show_plus_sticky_state_max_row_sets_per_window_max() {
        // Worst publishable row: the longest quoted correlation, the
        // composed show with the longest revision, the longest
        // authority/hold pair, and the largest single sticky state row
        // (hold with longest reason, false value, longest quoted seq).
        // 32 + 74 + 25 + 41 + 11 + 113 = 296.
        let worst = lane_window(
            Some(u64::MAX),
            Some(VisibilityTarget {
                shown: true,
                revision: u64::MAX,
                fallback: false,
            }),
            ScopeAuthority::Hold(HoldReason::IncompleteInventory),
            vec![StateTarget {
                property: StateProperty::Sticky,
                value: false,
                authority: ScopeAuthority::Hold(HoldReason::IncompleteInventory),
                command_seq: Some(u64::MAX),
            }],
        );
        let bytes = encoded_lanes(&worst);
        assert_eq!(bytes.len(), 32 + 74 + 25 + 41 + 11 + 113);
        assert_eq!(window_target_lanes_estimate(&worst), bytes.len());
        assert_eq!(MAX_TARGET_LANES_PER_WINDOW, bytes.len());
        // The common controlled show is cheaper: 32 + 74 + 31 + 21 + 11.
        let controlled = lane_window(
            Some(u64::MAX),
            Some(VisibilityTarget {
                shown: true,
                revision: u64::MAX,
                fallback: false,
            }),
            ScopeAuthority::Controlled,
            Vec::new(),
        );
        let controlled_bytes = encoded_lanes(&controlled);
        assert_eq!(controlled_bytes.len(), 32 + 74 + 31 + 21 + 11);
        assert_eq!(
            window_target_lanes_estimate(&controlled),
            controlled_bytes.len()
        );
    }

    #[test]
    fn staged_sticky_save_family_max_is_48() {
        use crate::intent_data::{IntentData, encode_intent_data};
        // Every save the composed sticky path stages: sticky-on records
        // `(Some(true), prior)`, sticky-off records `(prior, None)`, and
        // the sticky-clearing toggle records `(Some(false), None)`.
        let staged = [
            IntentData::new(Some(true), Some(true)),
            IntentData::new(Some(true), Some(false)),
            IntentData::new(Some(true), None),
            IntentData::new(Some(false), None),
        ];
        let mut widest = 0;
        for save in staged {
            widest = widest.max(json_string_len(&encode_intent_data(&save)));
        }
        assert_eq!(widest, 48);
        assert_eq!(MAX_INTENT_SAVE_DATA_JSON, 48);
    }

    #[test]
    fn rollback_prior_family_max_is_49_and_rows_fit_bound() {
        use crate::intent_data::{IntentData, encode_intent_data};
        use crate::world_report::IntentWrite;
        // All nine rollback shapes the proven-refusal path can restore,
        // including the record-free `(None, None)` and the escaped-49
        // `(Some(false), Some(false))` outside the command-save family.
        let priors = [
            (IntentData::new(None, None), 47),
            (IntentData::new(None, Some(false)), 48),
            (IntentData::new(None, Some(true)), 47),
            (IntentData::new(Some(false), None), 48),
            (IntentData::new(Some(false), Some(false)), 49),
            (IntentData::new(Some(false), Some(true)), 48),
            (IntentData::new(Some(true), None), 47),
            (IntentData::new(Some(true), Some(false)), 48),
            (IntentData::new(Some(true), Some(true)), 47),
        ];
        let mut widest = 0;
        for (prior, expected) in priors {
            let encoded = encode_intent_data(&prior);
            assert_eq!(json_string_len(&encoded), expected, "{prior:?}");
            widest = widest.max(expected);
        }
        assert_eq!(widest, 49);
        assert_eq!(MAX_INTENT_ROLLBACK_DATA_JSON, 49);
        assert_eq!(MAX_INTENT_ROLLBACK_ROW_BYTES, 16 + 130 + 49 + 1);
        // One max-id rollback row costs 195 on the wire; the per-row bound
        // reserves one comma conservatively.
        let max_id = "w".repeat(128);
        let row = IntentWrite {
            win: crate::directional::WindowId(max_id),
            data: Some(encode_intent_data(&IntentData::new(
                Some(false),
                Some(false),
            ))),
        };
        let writes = vec![row];
        assert_eq!(intent_writes_estimate(&writes), 18 + 16 + 130 + 49);
        assert!(intent_writes_estimate(&writes) <= 18 + max_intent_rollback_bound(1));
        assert_eq!(max_intent_rollback_bound(1), 196);
        // Three max rows: three bodies plus two commas stay under the bound.
        let max_rows: Vec<IntentWrite> = (0..3)
            .map(|i| IntentWrite {
                win: crate::directional::WindowId(format!("w{i}") + &"w".repeat(126)),
                data: Some(encode_intent_data(&IntentData::new(
                    Some(false),
                    Some(false),
                ))),
            })
            .collect();
        let section = intent_writes_estimate(&max_rows) - 18;
        assert_eq!(section, 3 * (16 + 130 + 49) + 2);
        assert!(section <= max_intent_rollback_bound(3));
        // Rollback plus an unrelated max-id command save share one section:
        // 195 + 194 + 1 comma = 390 fits the split prebound exactly.
        let save_row = IntentWrite {
            win: crate::directional::WindowId("w".repeat(128)),
            data: Some(encode_intent_data(&IntentData::new(Some(false), None))),
        };
        let both = vec![max_rows[0].clone(), save_row];
        let both_section = intent_writes_estimate(&both) - 18;
        assert_eq!(both_section, (16 + 130 + 49) + (16 + 130 + 48) + 1);
        assert!(both_section <= max_intent_rollback_bound(1) + 194);
    }

    #[test]
    fn visibility_object_and_state_row_spell_frozen_order() {
        // Byte witnesses for the frozen suffix order after `clear_op`:
        // `cmdSeq`, `visibility`, `visibility_auth`, `visibility_hold`,
        // `state` (mirrors the immutable v4 reply fixtures).
        let shown = lane_window(
            None,
            Some(VisibilityTarget {
                shown: true,
                revision: 1,
                fallback: false,
            }),
            ScopeAuthority::Controlled,
            Vec::new(),
        );
        let shown_bytes = encoded_lanes(&shown);
        assert!(
            shown_bytes
                .contains("\"visibility\":{\"shown\":true,\"rev\":\"1\",\"fallback\":false}")
        );
        let row = lane_window(
            None,
            None,
            ScopeAuthority::Untouched,
            vec![StateTarget {
                property: StateProperty::Above,
                value: true,
                authority: ScopeAuthority::Controlled,
                command_seq: None,
            }],
        );
        assert!(encoded_lanes(&row).contains(
            "\"state\":[{\"property\":\"above\",\"value\":true,\
             \"auth\":\"controlled\",\"reason\":\"\",\"cmdSeq\":null}]"
        ));
    }
}

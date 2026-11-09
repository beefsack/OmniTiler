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
use crate::world_report::{AuthorityScope, CommandResult, HoldReason, WorldCommand};

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

pub(crate) fn authority_scope_len(scope: &AuthorityScope) -> usize {
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

pub(crate) fn workspace_ref_option_len(
    reference: &Option<crate::world_report::WorkspaceRef>,
) -> usize {
    reference.as_ref().map_or(4, workspace_ref_len)
}

/// Structural request size estimate for one identity-bound visibility
/// read: `{"obs":"N","lifetimeMatched":B|null,"visibility":{"visible":B|
/// null,"suppressionOwned":B|null},"minimized":B|null,"hostHidden":B|null}`
/// = 102 fixed (braces, quoted keys, colons, commas, quotes around the
/// decimal watermark, and the nested visibility object) plus the watermark
/// digits and the nullable bools (`null` counts 4 each). The sibling
/// nullable-sample estimator below keeps its own wider fixed count; this
/// value form is exact for the always-present bracket reads.
fn visibility_sample_value_estimate(sample: &crate::world_report::VisibilitySample) -> usize {
    102 + u64_len(sample.obs)
        + sample.lifetime_matched.map_or(4, bool_len)
        + sample.visibility.visible.map_or(4, bool_len)
        + sample.visibility.suppression_owned.map_or(4, bool_len)
        + sample.minimized.map_or(4, bool_len)
        + sample.host_hidden.map_or(4, bool_len)
}

/// Structural request size estimate for one readiness bracket: the subject,
/// watermark, both samples, the exclusive arm, and the frame. The native
/// arm carries `,"home":R|null` (a namespaced workspace ref, `null` when
/// unreadable); the output arm carries `,"out":S|null` (an opaque output,
/// `null` when unknown). The native arm costs one byte more
/// (`,"home":` versus `,"out":`); both kind tokens are equal length.
fn arrival_bracket_estimate(bracket: &crate::world_report::ArrivalBracket) -> usize {
    // {"win":S,"obs":"N","before":{...},"kind":"native"|"output",
    // "home"|"out":V,"frame":R|null,"after":{...}}: outer braces, quoted
    // keys, colons, commas, quotes around the decimal watermark and the
    // kind token (68 output, 69 native) plus ids, digits, samples, the
    // location value, and the frame (`null` counts 4).
    let fixed = match &bracket.location {
        crate::world_report::ArrivalLocation::Native { .. } => 69,
        crate::world_report::ArrivalLocation::Output { .. } => 68,
    };
    fixed
        + json_string_len(&bracket.window.0)
        + u64_len(bracket.obs)
        + visibility_sample_value_estimate(&bracket.before)
        + match &bracket.location {
            crate::world_report::ArrivalLocation::Native { home } => {
                home.as_ref().map_or(4, |reference| {
                    use crate::world_report::WorkspaceRef as Ref;
                    match reference {
                        Ref::Logical { output, workspace } => {
                            "{\"kind\":\"logical\",\"out\":,\"ws\":}".len()
                                + json_string_len(&output.0)
                                + json_string_len(&workspace.0)
                        }
                        Ref::Native { output, workspace } => {
                            "{\"kind\":\"native\",\"out\":,\"ws\":}".len()
                                + json_string_len(&output.0)
                                + json_string_len(workspace.as_str())
                        }
                    }
                })
            }
            crate::world_report::ArrivalLocation::Output { out } => {
                out.as_ref().map_or(4, |id| json_string_len(&id.0))
            }
        }
        + rect_len(bracket.frame)
        + visibility_sample_value_estimate(&bracket.after)
}

/// Structural request size estimate for one correlated focus evidence row:
/// `{"id":S,"target":S,"rev":"N","setter":S,"reason":S,"pre":S|null,
/// "post":S|null,"foregroundProven":B,"arrival":[...],"attemptMs":"N"}`
/// = 109 fixed (braces, quoted keys, colons, commas, quotes around the
/// two decimal stamps, and the empty arrival brackets) plus ids, tokens,
/// nullable endpoints (`null` counts 4 each), the proof bool, the brackets
/// (one comma per extra bracket), and the stamp digits.
pub(crate) fn focus_feedback_estimate(row: &crate::world_report::FocusFeedback) -> usize {
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

/// Bound on one canonical binary64 spelling: the longest finite expansion
/// is the smallest subnormal (`5e-324`: `0.` plus 323 zeros plus one
/// digit, 327 bytes) and the longest large expansion is `f64::MAX` (309
/// digits); 350 covers every finite spelling with margin. Screen scales
/// ride this bound in the request estimator so the adapter's compact
/// canonical encoding always fits.
pub const MAX_CANONICAL_F64_LEN: usize = 350;

pub(crate) fn scope_status_estimate(scope: &crate::world_report::ScopeStatus) -> usize {
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

/// Structural size estimate for the switch-settled section:
/// `,"switchSettled":[]` (19) when empty; each event
/// `{"op":,"outcome":,"failed":[],"focus":,"fallback":}` counts 51 fixed
/// punctuation plus the parent operation id, outcome/focus tokens, the
/// fallback bool, and every failed window id (one comma between rows).
#[must_use]
pub fn switch_settled_estimate(settled: &[crate::world_report::SwitchEvent]) -> usize {
    // `,"switchSettled":[` (18) plus `]` (1) when empty = 19.
    let mut bytes = 19usize;
    for (i, event) in settled.iter().enumerate() {
        if i > 0 {
            bytes += 1;
        }
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
    bytes
}

/// Worst-case fixed bytes per switch-settled event row: 51 punctuation
/// plus the longest parent operation id (`op-<u64>`, 23 raw / 25 quoted),
/// the longest outcome (`completed-with-failures`, 23 raw / 25 quoted),
/// the longest focus token (`unproven`, 8 raw / 10 quoted), and the
/// longer bool (`false`, 5), plus one array comma (overcounts the final
/// row, conservatively). 51 + 25 + 25 + 10 + 5 + 1 = 117.
pub const MAX_SWITCH_EVENT_FIXED_BYTES: usize = 117;

/// Worst-case bytes per failed window id in a switch event: the longest
/// opaque window id (128 raw bytes, never escaped, so 130 quoted) plus one
/// array comma (overcounts the final id, conservatively).
pub const MAX_SWITCH_FAILED_ROW_BYTES: usize = 131;

/// Pre-mutation reserve for simultaneous switch completions: at most one
/// terminal event per retained selection binding completing in this report
/// (bindings are keyed by focus output, so simultaneous completions across
/// outputs stay covered), each failed list a subset of its frozen switch
/// members. Zero when no binding is retained. Conservative like every
/// other [`reply_upper_bound_v4`](crate::world_sizing_v4::reply_upper_bound_v4) term; the exact-encoded staged reply stays
/// the commit backstop.
#[must_use]
pub fn max_switch_settle_reserve(event_count: usize, failed_total: usize) -> usize {
    if event_count == 0 {
        return 0;
    }
    19 + event_count.saturating_mul(MAX_SWITCH_EVENT_FIXED_BYTES)
        + failed_total.saturating_mul(MAX_SWITCH_FAILED_ROW_BYTES)
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

/// Maximum per-row intent-save write width with array comma: `{"win":,"data":}`
/// is 16 punctuation plus the longest opaque subject id (128 raw bytes,
/// never escaped, so 130 quoted) plus [`MAX_INTENT_SAVE_DATA_JSON`], plus
/// one array comma. Exactly 16 + 130 + 48 + 1 = 195. The comma overcounts
/// the final row by one, conservatively.
pub const MAX_INTENT_SAVE_ROW_BYTES: usize = 195;

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
/// other [`reply_upper_bound_v4`](crate::world_sizing_v4::reply_upper_bound_v4) term; the exact-encoded staged reply stays
/// the commit backstop. Kept separate from [`max_intent_save_bound_v4`](crate::world_sizing_v4::max_intent_save_bound_v4) (the
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

/// Structural size estimate for one staged v4 anchored-border row,
/// field-by-field from the staged canonical row writer
/// (`encode_border_presentation_v4` in `tiler-protocol`):
/// `{"visible":B,"reason":S,"subject":S|null,"outsets":[l,t,r,b]|null,
/// "width":N|null,"radius":N|null,"witness":W|null}`. Standalone: no
/// existing full-reply estimate or reservation changes. `None` slots count
/// `null` (4); the reason token counts its quoted token (`""` counts 2).
#[must_use]
pub fn anchored_border_estimate(row: &crate::world_report::BorderPresentationV4) -> usize {
    // Fixed punctuation: `{` (1) + `"visible":` (10) + `,"reason":` (10) +
    // `,"subject":` (11) + `,"outsets":` (11) + `,"width":` (9) +
    // `,"radius":` (10) + `,"witness":` (11) + `}` (1) = 74.
    let mut bytes = 74
        + bool_len(row.visible)
        + json_string_len(
            row.reason
                .map_or("", crate::world_report::BorderHideReasonV4::as_str),
        );
    bytes += match &row.subject {
        None => 4,
        Some(subject) => json_string_len(&subject.0),
    };
    bytes += match &row.outsets {
        // `[l,t,r,b]` = brackets plus three commas (5) plus digits.
        None => 4,
        Some(outsets) => {
            5 + int_len(outsets[0])
                + int_len(outsets[1])
                + int_len(outsets[2])
                + int_len(outsets[3])
        }
    };
    bytes += match row.width {
        None => 4,
        Some(width) => int_len(width),
    };
    bytes += match row.radius {
        None => 4,
        Some(radius) => int_len(radius),
    };
    bytes += match &row.witness {
        None => 4,
        Some(witness) => {
            // `{"win":S,"obs":"N","frame":R,"foreground":S}` = 40 fixed
            // plus ids, counter digits, and the rect.
            40 + json_string_len(&witness.win.0)
                + u64_len(witness.obs)
                + rect_len(Some(witness.frame))
                + json_string_len(&witness.foreground.0)
        }
    };
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
pub(crate) fn last_write_estimate(last: &Option<crate::world_report::LastWrite>) -> usize {
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
pub(crate) fn command_estimate_owned(command: &WorldCommand) -> usize {
    // Execution-time subjects ride `win:{"currentFocus":true}` (21 bytes);
    // the wrapped payload always carries the placeholder empty subject
    // (`""`, 2 bytes), so the implicit estimate is the explicit estimate
    // plus 19.
    if let WorldCommand::Implicit(inner) = command {
        return command_estimate_owned(inner) + 19;
    }
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
            // {"op":"toggle-sticky","win":S} = 29 fixed.
            29 + json_string_len(&window.0)
        }
        WorldCommand::SetFullscreen { window, value } => {
            // {"op":"set-fullscreen","win":S,"value":B} = 39 fixed.
            39 + json_string_len(&window.0) + bool_len(*value)
        }
        WorldCommand::ToggleFullscreen { window } => {
            // {"op":"toggle-fullscreen","win":S} = 33 fixed.
            33 + json_string_len(&window.0)
        }
        WorldCommand::Operation(command) => {
            // {"op":"operation","command":{...}} = 30 fixed.
            30 + operation_estimate(command)
        }
        WorldCommand::Implicit(_) => {
            unreachable!("implicit subjects unwrap before estimating")
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
/// as an encoding (see [`report_estimate_v4`](crate::world_sizing_v4::report_estimate_v4)).
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
pub(crate) fn raw_setting_value_estimate(value: &crate::connection::RawSettingValue) -> usize {
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

/// Exact canonical encoding for the switch-settled section, with frozen
/// key order (mirrored by [`switch_settled_estimate`] and the protocol
/// codec): `[]` when no switch settled, else one object per settled parent
/// `{"op":..,"outcome":..,"failed":..,"focus":..,"fallback":..}` where a
/// `complete` outcome carries an empty `failed` list.
pub fn encode_switch_settled(out: &mut String, settled: &[crate::world_report::SwitchEvent]) {
    out.push('[');
    for (i, event) in settled.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str("{\"op\":");
        push_json_string(out, &event.operation_id);
        out.push_str(",\"outcome\":");
        push_json_string(out, event.outcome.as_str());
        out.push_str(",\"failed\":[");
        for (j, window) in event.failed.iter().enumerate() {
            if j > 0 {
                out.push(',');
            }
            push_json_string(out, &window.0);
        }
        out.push_str("],\"focus\":");
        push_json_string(out, event.focus.as_str());
        out.push_str(",\"fallback\":");
        out.push_str(if event.fallback { "true" } else { "false" });
        out.push('}');
    }
    out.push(']');
}

/// Exact canonical encoding for one narrow-hold row diagnostic, with
/// frozen key order (mirroring the v4 inline diagnostic math at
/// `crate::world_sizing_v4` plus the protocol codec): `{"scope":..,"row":..,"id":..,"kind":..}` where a
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

/// Maximum per-offroute-native-state-attempt diagnostic cost: scope (155)
/// plus lane/kind tokens (30) plus id (130) plus punctuation (30) = 345,
/// rounded to 512. Independently derived like the held-screen row cost, never
/// borrowed from other headroom.
pub const MAX_STATE_ATTEMPT_DIAGNOSTIC_BYTES: usize = 512;

/// Maximum per-published-window outbound target lane cost for the current
/// engine: the composed sticky show plus at most one native sticky state
/// target plus at most one native fullscreen state target plus at most one
/// native maximize state target plus at most one commandless native above
/// state target dominate the default suffix. The worst row carries the
/// longest quoted command correlation
/// (`,"cmdSeq":"18446744073709551615"`, 32), the composed show object
/// (`,"visibility":{"shown":true,
/// "rev":"18446744073709551615","fallback":false}`, 14 + 31 + 4 + 20 + 5
/// = 74), the longest authority/hold pair (`Hold(IncompleteInventory)`:
/// `,"visibility_auth":"hold"` (25) plus
/// `,"visibility_hold":"incomplete-inventory"` (41)), the state array with
/// one sticky row (`,"state":[{...}]`, 11 plus 113, see below), one comma
/// (1), one fullscreen row (117, see below), one comma (1), one maximize
/// row (116, see below), one comma (1), and one commandless above row
/// (94, see below): 32 + 74 + 25 + 41 + 11 +
/// 113 + 1 + 117 + 1 + 116 + 1 + 94 = 626. The largest single sticky state row is a
/// `Hold` with the longest hold reason, a false value, and the longest
/// quoted command sequence: 50 + 8 (`"sticky"`) + 5 (`false`) + 6
/// (`"hold"`) + 22 (`"incomplete-inventory"`) + 22 (quoted `u64::MAX`) =
/// 113. The largest single fullscreen state row is the same shape with
/// `"fullscreen"` (10 letters, 12 quoted): 50 + 12 + 5 + 6 + 22 + 22 =
/// 117. The largest single maximize state row is the same shape with
/// `"maximized"` (9 letters, 11 quoted): 50 + 11 + 5 + 6 + 22 + 22 = 116.
/// The largest single above state row is commandless (`command_seq: None`,
/// a quoted `u64::MAX` is never produced here so no speculative 18 is
/// budgeted): 50 + 7 (`"above"`) + 5 (`false`) + 6 (`"hold"`) + 22
/// (`"incomplete-inventory"`) + 4 (`null`) = 94.
/// The common `Controlled` show with a controlled sticky target costs
/// less; the precomposed default suffix costs 94 with null correlation and
/// 112 with the longest quoted counter. The exact staged gate stays the
/// backstop.
pub const MAX_TARGET_LANES_PER_WINDOW: usize = 626;

/// Maximum canonical bytes of one emitted effect row with typed cause
/// correlation (`{"id":,"kind":,"target":,"rev":"","cause":,"cmdSeq":null|
/// `"N","awaitShown":B}`): 67 fixed punctuation plus the widest values
/// the actual engine producers emit. The joint widest row is the explicit
/// command focus over a maximum-length opaque window (`fx-<u64::MAX>` is
/// 23 raw, 25 quoted; compact `af-<token>` tops out at the same 23 raw /
/// 25 quoted and `ffc-<token>` at 24 raw / 26 quoted, with at most 20
/// token digits). Kind tops out at 7 (`"focus"`/`"raise"`/`"stack"`
/// quoted), target at 130 (128 opaque quoted), revision at 20
/// (`u64::MAX` digits), cause at 18 quoted on the explicit-command row
/// (`explicit-command`, 16 bytes), which carries a command sequence;
/// `focus-follows-cursor` (20 bytes / 22 quoted) rides the compact hover
/// id and `automatic-admission` (19 bytes / 21 quoted) rides the compact
/// admission id, both on commandless rows, command sequence at 22 for
/// rows that carry one (quoted `u64::MAX`) or 4 for the
/// correlation-valid commandless rows (`null`; automatic rows never carry
/// a sequence), and the bool at 5 (`false`).
/// Opaque ids never escape ([`crate::bounds::is_opaque_id`]), so quoted
/// widths are exact: 67 + 25 + 7 + 130 + 20 + 18 + 22 + 5 = 294. The
/// widest compact admission row is 279 (67 + 25 + 7 + 130 + 20 + 21 + 4
/// + 5) and the widest compact hover row is 281 (67 + 26 + 7 + 130 + 20
/// + 22 + 4 + 5) by the same accounting.
pub const MAX_EFFECT_ROW_BYTES: usize = 294;

/// Pre-mutation reserve for coexisting one-shot effects: at most one
/// fresh-admission focus plus at most one explicit-command focus per
/// report, conservatively two joint-widest rows plus one array comma
/// (2 * 294 + 1 = 589). The admission pass mints at most one intent;
/// command execution mints at most one explicit focus; a same-target
/// admission duplicate drops in favor of the explicit winner, so
/// different targets are the only two-row case, and a focus-follows-cursor
/// hover never mints when any focus effect is already present, so two
/// hovers never coexist: the actual widest coexistence is the compact
/// admission maximum (279) plus the explicit maximum (294) plus one comma
/// (574), asserted as a witness below. No raise/stack producer exists in
/// the engine; the closed kinds stay encodable without a reserve of their
/// own.
pub const MAX_EFFECT_RESERVE_BYTES: usize = MAX_EFFECT_ROW_BYTES * 2 + 1;

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
            "M\u{fc}ller-\u{b5}-\u{65e5}\u{672c}\u{8a9e}-\u{1f39b}",
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
    fn engine_default_lanes_cost_94_and_per_window_max_626() {
        // The current engine emits null visibility, untouched authority,
        // and an empty state array on every row: 94 bytes with a null
        // correlation, 112 with the longest quoted counter. The
        // per-published-window pre-mutation reserve is wider: it also
        // covers the worst composed sticky show plus the largest single
        // native sticky state row plus the largest single native
        // fullscreen state row plus the largest single native maximize
        // state row plus the largest single commandless native above row
        // (see below).
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
        assert_eq!(MAX_TARGET_LANES_PER_WINDOW, 626);
    }

    #[test]
    fn composed_show_plus_sticky_state_max_row_is_296() {
        // Sticky-only worst row: the longest quoted correlation, the
        // composed show with the longest revision, the longest
        // authority/hold pair, and the largest single sticky state row
        // (hold with longest reason, false value, longest quoted seq).
        // 32 + 74 + 25 + 41 + 11 + 113 = 296. The per-window max is
        // wider now that fullscreen and maximize rows can coexist
        // (see below).
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
        assert_eq!(bytes.len(), 296);
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
    fn composed_show_plus_sticky_and_fullscreen_max_rows_set_per_window_max() {
        // Two-row checkpoint: sticky plus fullscreen only.
        // 32 + 74 + 25 + 41 + 11 + 113 + 1 + 117 = 414. The per-window
        // max is wider now that a maximize row can coexist (see below).
        let two = lane_window(
            Some(u64::MAX),
            Some(VisibilityTarget {
                shown: true,
                revision: u64::MAX,
                fallback: false,
            }),
            ScopeAuthority::Hold(HoldReason::IncompleteInventory),
            vec![
                StateTarget {
                    property: StateProperty::Sticky,
                    value: false,
                    authority: ScopeAuthority::Hold(HoldReason::IncompleteInventory),
                    command_seq: Some(u64::MAX),
                },
                StateTarget {
                    property: StateProperty::Fullscreen,
                    value: false,
                    authority: ScopeAuthority::Hold(HoldReason::IncompleteInventory),
                    command_seq: Some(u64::MAX),
                },
            ],
        );
        let two_bytes = encoded_lanes(&two);
        assert_eq!(two_bytes.len(), 32 + 74 + 25 + 41 + 11 + 113 + 1 + 117);
        assert_eq!(window_target_lanes_estimate(&two), two_bytes.len());
        assert_eq!(two_bytes.len(), 414);
        // Worst publishable three-state row (historical checkpoint): the longest
        // quoted correlation, the composed show with the longest revision,
        // the longest authority/hold pair, the largest single sticky state
        // row, one comma, the largest single fullscreen state row (hold with
        // longest reason, false value, longest quoted seq), one comma, and
        // the largest single maximize state row (hold with longest reason,
        // false value, longest quoted seq; "maximized" is 9 letters, 11
        // quoted, so 50 + 11 + 5 + 6 + 22 + 22 = 116).
        // 32 + 74 + 25 + 41 + 11 + 113 + 1 + 117 + 1 + 116 = 531. The
        // per-window max is wider now that a commandless above row can
        // coexist (see below).
        let worst = lane_window(
            Some(u64::MAX),
            Some(VisibilityTarget {
                shown: true,
                revision: u64::MAX,
                fallback: false,
            }),
            ScopeAuthority::Hold(HoldReason::IncompleteInventory),
            vec![
                StateTarget {
                    property: StateProperty::Sticky,
                    value: false,
                    authority: ScopeAuthority::Hold(HoldReason::IncompleteInventory),
                    command_seq: Some(u64::MAX),
                },
                StateTarget {
                    property: StateProperty::Fullscreen,
                    value: false,
                    authority: ScopeAuthority::Hold(HoldReason::IncompleteInventory),
                    command_seq: Some(u64::MAX),
                },
                StateTarget {
                    property: StateProperty::Maximized,
                    value: false,
                    authority: ScopeAuthority::Hold(HoldReason::IncompleteInventory),
                    command_seq: Some(u64::MAX),
                },
            ],
        );
        let bytes = encoded_lanes(&worst);
        assert_eq!(
            bytes.len(),
            32 + 74 + 25 + 41 + 11 + 113 + 1 + 117 + 1 + 116
        );
        assert_eq!(window_target_lanes_estimate(&worst), bytes.len());
        assert_eq!(bytes.len(), 531);
        assert_eq!(MAX_TARGET_LANES_PER_WINDOW, 626);
    }

    #[test]
    fn commandless_above_coexists_with_three_state_rows_sets_per_window_max() {
        // Four-row witness: the longest quoted correlation, the composed
        // show with the longest revision, the longest authority/hold pair,
        // the largest single sticky/fullscreen/maximize rows (quoted
        // u64::MAX each), one comma, and the largest single commandless
        // above row (hold with longest reason, false value, command_seq
        // None: 50 + 7 (`"above"`) + 5 (`false`) + 6 (`"hold"`) + 22
        // (`"incomplete-inventory"`) + 4 (`null`) = 94). Derived from the
        // actual encoder, never asserted arithmetic.
        let worst = lane_window(
            Some(u64::MAX),
            Some(VisibilityTarget {
                shown: true,
                revision: u64::MAX,
                fallback: false,
            }),
            ScopeAuthority::Hold(HoldReason::IncompleteInventory),
            vec![
                StateTarget {
                    property: StateProperty::Sticky,
                    value: false,
                    authority: ScopeAuthority::Hold(HoldReason::IncompleteInventory),
                    command_seq: Some(u64::MAX),
                },
                StateTarget {
                    property: StateProperty::Fullscreen,
                    value: false,
                    authority: ScopeAuthority::Hold(HoldReason::IncompleteInventory),
                    command_seq: Some(u64::MAX),
                },
                StateTarget {
                    property: StateProperty::Maximized,
                    value: false,
                    authority: ScopeAuthority::Hold(HoldReason::IncompleteInventory),
                    command_seq: Some(u64::MAX),
                },
                StateTarget {
                    property: StateProperty::Above,
                    value: false,
                    authority: ScopeAuthority::Hold(HoldReason::IncompleteInventory),
                    command_seq: None,
                },
            ],
        );
        let bytes = encoded_lanes(&worst);
        assert_eq!(
            window_target_lanes_estimate(&worst),
            bytes.len(),
            "estimator stays exact on the four-row witness"
        );
        assert_eq!(
            bytes.len(),
            32 + 74 + 25 + 41 + 11 + 113 + 1 + 117 + 1 + 116 + 1 + 94
        );
        assert_eq!(bytes.len(), 626);
        assert_eq!(MAX_TARGET_LANES_PER_WINDOW, 626);
    }

    #[test]
    fn fullscreen_commands_estimate_matches_exact_bytes() {
        use crate::directional::WindowId;
        use crate::world_report::WorldCommand;
        // {"op":"set-fullscreen","win":S,"value":B} = 39 fixed.
        let set = WorldCommand::SetFullscreen {
            window: WindowId("win-a".to_owned()),
            value: true,
        };
        let set_wire = "{\"op\":\"set-fullscreen\",\"win\":\"win-a\",\"value\":true}";
        assert_eq!(command_estimate_owned(&set), set_wire.len());
        assert_eq!(
            command_estimate_owned(&set),
            39 + json_string_len("win-a") + bool_len(true)
        );
        let set_false = WorldCommand::SetFullscreen {
            window: WindowId("w".to_owned()),
            value: false,
        };
        let set_false_wire = "{\"op\":\"set-fullscreen\",\"win\":\"w\",\"value\":false}";
        assert_eq!(command_estimate_owned(&set_false), set_false_wire.len());
        assert_eq!(
            command_estimate_owned(&set_false),
            39 + json_string_len("w") + bool_len(false)
        );
        // {"op":"toggle-fullscreen","win":S} = 33 fixed.
        let toggle = WorldCommand::ToggleFullscreen {
            window: WindowId("win-a".to_owned()),
        };
        let toggle_wire = "{\"op\":\"toggle-fullscreen\",\"win\":\"win-a\"}";
        assert_eq!(command_estimate_owned(&toggle), toggle_wire.len());
        assert_eq!(
            command_estimate_owned(&toggle),
            33 + json_string_len("win-a")
        );
    }

    #[test]
    fn sticky_commands_estimate_matches_exact_bytes() {
        use crate::directional::WindowId;
        use crate::world_report::WorldCommand;
        // {"op":"set-sticky","win":S,"value":B} = 35 fixed.
        let set = WorldCommand::SetSticky {
            window: WindowId("win-a".to_owned()),
            value: true,
        };
        let set_wire = "{\"op\":\"set-sticky\",\"win\":\"win-a\",\"value\":true}";
        assert_eq!(command_estimate_owned(&set), set_wire.len());
        assert_eq!(
            command_estimate_owned(&set),
            35 + json_string_len("win-a") + bool_len(true)
        );
        // {"op":"toggle-sticky","win":S} = 29 fixed.
        let toggle = WorldCommand::ToggleSticky {
            window: WindowId("w".to_owned()),
        };
        let toggle_wire = "{\"op\":\"toggle-sticky\",\"win\":\"w\"}";
        assert_eq!(toggle_wire.len(), 32);
        assert_eq!(command_estimate_owned(&toggle), toggle_wire.len());
        assert_eq!(command_estimate_owned(&toggle), 29 + json_string_len("w"));
        // Ordinary id spells the exact literal through the real encoder.
        let ordinary = "win-a";
        let ordinary_wire = format!("{{\"op\":\"toggle-sticky\",\"win\":{}}}", {
            let mut encoded = String::new();
            push_json_string(&mut encoded, ordinary);
            encoded
        });
        assert_eq!(
            ordinary_wire,
            "{\"op\":\"toggle-sticky\",\"win\":\"win-a\"}"
        );
        let ordinary_cmd = WorldCommand::ToggleSticky {
            window: WindowId(ordinary.to_owned()),
        };
        assert_eq!(command_estimate_owned(&ordinary_cmd), ordinary_wire.len());
        assert_eq!(
            command_estimate_owned(&ordinary_cmd),
            29 + json_string_len(ordinary)
        );
        // Escaping, C0 controls, and multibyte UTF-8 ride the shared
        // string codec exactly (not valid opaque window ids, so the
        // string layer is the exactness witness here).
        let multibyte =
            String::from_utf8(vec![0xc3, 0xbc, 0xe6, 0x97, 0xa5, 0xf0, 0x9f, 0x8e, 0x9b])
                .expect("valid UTF-8 witness");
        for text in [
            "quote\"back\\slash",
            "control\0\u{1}\u{1f}newline\n",
            multibyte.as_str(),
        ] {
            let mut encoded = String::new();
            push_json_string(&mut encoded, text);
            assert_eq!(json_string_len(text), encoded.len(), "{text:?}");
        }
        // Max valid opaque id (128) spells exactly through both layers.
        let max_id = "w".repeat(crate::bounds::MAX_OPAQUE_ID_LEN);
        assert!(crate::bounds::is_opaque_id(&max_id));
        let max_wire = format!("{{\"op\":\"toggle-sticky\",\"win\":{}}}", {
            let mut encoded = String::new();
            push_json_string(&mut encoded, &max_id);
            encoded
        });
        let max_cmd = WorldCommand::ToggleSticky {
            window: WindowId(max_id.clone()),
        };
        assert_eq!(command_estimate_owned(&max_cmd), max_wire.len());
        assert_eq!(
            command_estimate_owned(&max_cmd),
            29 + json_string_len(&max_id)
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

#[cfg(test)]
mod effect_vocabulary_sizing_tests {
    use super::*;
    use crate::directional::WindowId;
    use crate::world_engine::WorldOutcome;
    use crate::world_report::{
        EffectCause, EffectKind, OneShotEffect, WORLD_MAX_REPLY_BYTES, WORLD_MAX_REQUEST_BYTES,
    };
    use crate::world_sizing_v4::{
        reply_estimate_v4, reply_exact_bytes_v4, reply_upper_bound_v4, report_estimate_v4,
    };
    use crate::world_v4_support_tests::{
        accepted as accepted_v4, fact, report_full as report_full_v4, screen,
    };

    fn reply_with_effects(effects: Vec<OneShotEffect>) -> crate::world_report::WorldReplyV4 {
        let mut engine =
            crate::world_engine::WorldEngine::new_test_bound("gen-1", "adapter-1").expect("engine");
        let seed = accepted_v4(
            &mut engine,
            &report_full_v4(
                1,
                1,
                vec![screen("out-a", 0)],
                vec![fact("win-a", "out-a", 0)],
                vec![],
                "",
                None,
                None,
            ),
        );
        let mut reply = seed;
        reply.effects = effects;
        reply
    }

    fn widest_hover() -> OneShotEffect {
        // Correlation-valid by construction: the compact hover identity
        // over a maximum-length opaque window (`ffc-` plus the longest
        // interned token), the longest cause that rides a commandless
        // row, the largest revision, no command sequence (hover rows never
        // correlate), and the costlier bool.
        OneShotEffect {
            id: format!("ffc-{}", u64::MAX),
            kind: EffectKind::Focus,
            target: WindowId("w".repeat(128)),
            issued_revision: u64::MAX,
            cause: EffectCause::FocusFollowsCursor,
            command_seq: None,
            await_shown: false,
        }
    }

    fn widest_admission() -> OneShotEffect {
        OneShotEffect {
            id: format!("af-{}", u64::MAX),
            kind: EffectKind::Focus,
            target: WindowId("a".repeat(128)),
            issued_revision: u64::MAX,
            cause: EffectCause::AutomaticAdmission,
            command_seq: None,
            await_shown: false,
        }
    }

    fn widest_explicit() -> OneShotEffect {
        OneShotEffect {
            id: format!("fx-{}", u64::MAX),
            kind: EffectKind::Focus,
            target: WindowId("b".repeat(128)),
            issued_revision: u64::MAX,
            cause: EffectCause::ExplicitCommand,
            command_seq: Some(u64::MAX),
            await_shown: false,
        }
    }

    #[test]
    fn effect_estimate_is_exact_over_causal_widths() {
        // Marginal accounting: `null` vs quoted-`u64` command sequences
        // and both bools are exact through the shared estimator, over
        // every cause and the closed kinds.
        let rows = vec![
            OneShotEffect {
                id: "fx-11".to_owned(),
                kind: EffectKind::Focus,
                target: WindowId("win-a".to_owned()),
                issued_revision: 7,
                cause: EffectCause::ExplicitCommand,
                command_seq: Some(11),
                await_shown: false,
            },
            OneShotEffect {
                id: "af-win-b".to_owned(),
                kind: EffectKind::Focus,
                target: WindowId("win-b".to_owned()),
                issued_revision: u64::MAX,
                cause: EffectCause::AutomaticAdmission,
                command_seq: None,
                await_shown: false,
            },
            OneShotEffect {
                id: "ffc-win-c".to_owned(),
                kind: EffectKind::Focus,
                target: WindowId("win-c".to_owned()),
                issued_revision: 1,
                cause: EffectCause::FocusFollowsCursor,
                command_seq: None,
                await_shown: false,
            },
            OneShotEffect {
                id: "effect-a".to_owned(),
                kind: EffectKind::Raise,
                target: WindowId("win-a".to_owned()),
                issued_revision: 1,
                cause: EffectCause::ExplicitCommand,
                command_seq: Some(u64::MAX),
                await_shown: true,
            },
            OneShotEffect {
                id: "effect-a".to_owned(),
                kind: EffectKind::Stack,
                target: WindowId("win-a".to_owned()),
                issued_revision: 1,
                cause: EffectCause::ExplicitCommand,
                command_seq: Some(1),
                await_shown: true,
            },
        ];
        let reply = reply_with_effects(rows);
        let wire_len = reply_exact_bytes_v4(&reply);
        let empty = reply_with_effects(Vec::new());
        let base_len = reply_exact_bytes_v4(&empty);
        // Marginal accounting is exact: the estimate delta over the same
        // reply with empty effects equals the exact bytes (rows plus one
        // comma per extra row), for null and quoted-`u64` sequences and
        // both bools alike. (The full-reply estimate keeps its
        // pre-existing conservative slack elsewhere; only the effect lane
        // is asserted exact here. The v4 wire key order itself is pinned
        // by the protocol twin test.)
        let row_bytes: usize = reply
            .effects
            .iter()
            .map(|row| {
                let one = reply_with_effects(vec![row.clone()]);
                reply_exact_bytes_v4(&one) - base_len
            })
            .sum();
        assert_eq!(
            wire_len - base_len,
            row_bytes + reply.effects.len().saturating_sub(1)
        );
        assert_eq!(
            reply_estimate_v4(&reply) - reply_estimate_v4(&empty),
            wire_len - base_len,
            "effect estimate stays exact over null/quoted sequences and both bools"
        );
        assert!(
            reply_estimate_v4(&reply) >= wire_len,
            "estimate covers the reply"
        );
    }

    #[test]
    fn presentation_rev_marginal_is_exact_and_reserved() {
        // Required revision label: quoted decimal riding the presentation.
        // The assertions below pin the marginal width only (exactly the
        // digit delta at every causal width: 0, 9, 10, u64::MAX); the
        // surrounding estimates keep their pre-existing conservative
        // slack. The pre-mutation bound carries the 29-byte worst case as
        // an explicit term, pinned by an exact headroom equality. The v4
        // wire key order itself is pinned by the protocol twin test.
        let mut zero = reply_with_effects(Vec::new());
        zero.presentation.foreground = "win-a".to_owned();
        zero.presentation.rev = 0;
        let zero_exact = reply_exact_bytes_v4(&zero);
        for rev in [0u64, 9, 10, u64::MAX] {
            let mut reply = zero.clone();
            reply.presentation.rev = rev;
            assert_eq!(
                reply_exact_bytes_v4(&reply) - zero_exact,
                u64_len(rev) - u64_len(0),
                "marginal presentation width is exact at rev {rev}"
            );
            assert_eq!(
                reply_estimate_v4(&reply) - reply_estimate_v4(&zero),
                reply_exact_bytes_v4(&reply) - zero_exact,
                "presentation estimate marginal stays exact at rev {rev}"
            );
            assert!(
                reply_estimate_v4(&reply) >= reply_exact_bytes_v4(&reply),
                "estimate covers the reply at rev {rev}"
            );
        }
        // Pre-mutation bound: for an empty report every report-dependent
        // term is zero, so the headroom over the request estimate must
        // equal the fixed budget plus the explicit 29-byte worst-case
        // label exactly. Dropping the reservation breaks this equality
        // (the 4096 envelope slack alone cannot cover it).
        let empty = report_full_v4(1, 1, Vec::new(), Vec::new(), Vec::new(), "", None, None);
        assert_eq!(
            reply_upper_bound_v4(&empty, 0, 0, 0, 0, 0).saturating_sub(report_estimate_v4(&empty)),
            4096 + MAX_EFFECT_RESERVE_BYTES + 2048 + 512 + 704 + 29 + 1024 + 96,
            "explicit 29-byte label reserve is load-bearing in the headroom"
        );
    }

    #[test]
    fn max_effect_widths_derive_the_coexistence_reserve() {
        // Encoder witnesses: the compact producer rows spell their maxima
        // exactly. The joint-widest single row is the explicit command
        // focus; the only coexistence the engine produces is admission
        // plus explicit on distinct targets (same-target admission dedups
        // in favor of the explicit winner), so the pair witness uses
        // distinct 128-byte subjects and stays correlation-valid by
        // construction: the widest compact admission row plus the widest
        // explicit row plus one comma spell 574, held under the
        // conservative two-widest reserve.
        let hover = widest_hover();
        let one = reply_with_effects(vec![hover.clone()]);
        let empty = reply_with_effects(Vec::new());
        let hover_len = reply_exact_bytes_v4(&one) - reply_exact_bytes_v4(&empty);
        assert_eq!(hover_len, 281, "widest hover row: {hover_len}");
        let explicit = widest_explicit();
        let explicit_one = reply_with_effects(vec![explicit.clone()]);
        let explicit_len = reply_exact_bytes_v4(&explicit_one) - reply_exact_bytes_v4(&empty);
        assert_eq!(
            explicit_len, MAX_EFFECT_ROW_BYTES,
            "worst row: {explicit_len}"
        );
        assert_eq!(
            MAX_EFFECT_ROW_BYTES, 294,
            "67 fixed + 25 id + 7 kind + 130 target + 20 rev + 18 cause + 22 seq + 5 bool"
        );
        let admission = widest_admission();
        let admission_one = reply_with_effects(vec![admission.clone()]);
        let admission_len = reply_exact_bytes_v4(&admission_one) - reply_exact_bytes_v4(&empty);
        assert_eq!(admission_len, 279, "widest admission row: {admission_len}");
        let pair = reply_with_effects(vec![admission.clone(), explicit.clone()]);
        assert_ne!(
            admission.target, explicit.target,
            "coexisting rows name distinct subjects"
        );
        assert_ne!(
            admission.id, explicit.id,
            "coexisting rows carry distinct identities"
        );
        let pair_len = reply_exact_bytes_v4(&pair) - reply_exact_bytes_v4(&empty);
        assert_eq!(
            pair_len, 574,
            "actual widest coexistence (admission + explicit + comma): {pair_len}"
        );
        assert!(
            pair_len <= MAX_EFFECT_RESERVE_BYTES,
            "coexistence fits the conservative reserve"
        );
        assert_eq!(MAX_EFFECT_RESERVE_BYTES, 589);
        assert_eq!(MAX_EFFECT_RESERVE_BYTES, 2 * MAX_EFFECT_ROW_BYTES + 1);
    }

    #[test]
    fn explicit_focus_supersedes_admission_within_the_reserve() {
        // v4 latest-wins (approved active difference): an explicit Focus
        // command supersedes every unissued prior Focus, so a newcomer's
        // automatic admission never co-publishes with the explicit
        // winner. The pre-mutation bound still covers the coexistence
        // worst case (the reserve stays conservative), and the token
        // mapping proves the admission minted before the supersession
        // dropped it.
        use crate::world_report::WorldCommand;
        let mut engine =
            crate::world_engine::WorldEngine::new_test_bound("gen-1", "adapter-1").expect("engine");
        accepted_v4(
            &mut engine,
            &report_full_v4(
                1,
                1,
                vec![screen("out-a", 0)],
                vec![fact("win-a", "out-a", 0)],
                vec![],
                "",
                None,
                None,
            ),
        );
        let input = report_full_v4(
            2,
            2,
            vec![screen("out-a", 0)],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
            vec![],
            "",
            Some(2),
            Some(WorldCommand::Focus {
                window: WindowId("win-a".to_owned()),
                direction: "right".to_owned(),
            }),
        );
        let pre_bound = reply_upper_bound_v4(
            &input,
            engine.retained.len() + engine.retained_floats.len(),
            engine.engine.domain_keys().len(),
            input.screens.len(),
            engine.staged.facts.len() + engine.staged.held_windows.len(),
            engine.retained_publish_footprint(),
        )
        .saturating_add(max_intent_rollback_bound(engine.rollback_reserve_count()));
        let reply = accepted_v4(&mut engine, &input);
        assert_eq!(
            reply.effects.len(),
            1,
            "explicit winner supersedes admission: {:?}",
            reply.effects
        );
        let explicit = reply
            .effects
            .iter()
            .find(|effect| effect.is_explicit_focus_for(2))
            .expect("explicit row");
        assert_eq!(explicit.target.0, "win-a");
        assert!(!explicit.await_shown);
        assert!(
            reply
                .effects
                .iter()
                .all(|effect| effect.cause != EffectCause::AutomaticAdmission),
            "admission superseded by the explicit winner: {:?}",
            reply.effects
        );
        assert!(
            engine
                .automatic_focus_tokens
                .contains_key(&WindowId("win-b".to_owned())),
            "admission minted before the supersession dropped it"
        );
        let actual = reply_exact_bytes_v4(&reply);
        assert!(
            reply_estimate_v4(&reply) >= actual,
            "estimate covers coexistence"
        );
        assert!(
            actual <= pre_bound,
            "coexistence fits the pre-mutation bound"
        );
    }

    #[test]
    fn effect_reserve_boundary_rejects_before_mutation() {
        // The added coexistence reserve is the discriminator: the bound
        // without it fits while the bound with it exceeds, so the gate
        // rejects before any revision or command-highwater mutation. The
        // same command sequence still works once the report fits.
        use crate::world_report::WorldCommand;
        let mut engine =
            crate::world_engine::WorldEngine::new_test_bound("gen-1", "adapter-1").expect("engine");
        accepted_v4(
            &mut engine,
            &report_full_v4(
                1,
                1,
                vec![screen("out-a", 0)],
                vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
                vec![],
                "",
                None,
                None,
            ),
        );
        let logical = engine.engine.domain_keys().len();
        let focus = || WorldCommand::Focus {
            window: WindowId("win-a".to_owned()),
            direction: "right".to_owned(),
        };
        let bound_of = |windows: &[crate::world_report::WindowFact]| {
            let report = report_full_v4(
                2,
                2,
                vec![screen("out-a", 0)],
                windows.to_vec(),
                vec![],
                "",
                Some(2),
                Some(focus()),
            );
            reply_upper_bound_v4(
                &report,
                0,
                logical,
                1,
                0,
                engine.retained_publish_footprint(),
            )
        };
        let mut windows = vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)];
        for i in 0..2200 {
            windows.push(fact(&format!("win-pad-{i:05}"), "out-a", 0));
        }
        while bound_of(&windows) - MAX_EFFECT_RESERVE_BYTES > WORLD_MAX_REPLY_BYTES {
            windows.pop();
        }
        // Fine-tune one title so the reserve-free bound lands just under
        // the cap: each title byte adds exactly one, plus the 2-byte
        // null-to-string swap.
        let shortfall = WORLD_MAX_REPLY_BYTES
            .saturating_sub(bound_of(&windows) - MAX_EFFECT_RESERVE_BYTES)
            .saturating_sub(10);
        windows[2].title = Some("t".repeat(shortfall + 2));
        let report = report_full_v4(
            2,
            2,
            vec![screen("out-a", 0)],
            windows,
            vec![],
            "",
            Some(2),
            Some(focus()),
        );
        let without_reserve = reply_upper_bound_v4(
            &report,
            0,
            logical,
            1,
            0,
            engine.retained_publish_footprint(),
        ) - MAX_EFFECT_RESERVE_BYTES;
        assert!(
            report_estimate_v4(&report) <= WORLD_MAX_REQUEST_BYTES,
            "request itself fits; only the reply bound trips"
        );
        assert!(
            without_reserve <= WORLD_MAX_REPLY_BYTES,
            "reserve-free bound fits: {without_reserve}"
        );
        assert!(
            without_reserve + MAX_EFFECT_RESERVE_BYTES > WORLD_MAX_REPLY_BYTES,
            "added reserve is the discriminator"
        );
        let revision = engine.state_revision;
        match engine.handle_report(&report) {
            WorldOutcome::Rejected { kind, .. } => {
                assert_eq!(kind, crate::world_engine::KIND_REPLY_SIZE);
            }
            other => panic!("expected reply-oversize, got {other:?}"),
        }
        assert_eq!(engine.state_revision, revision, "refusal mutates nothing");
        assert!(
            engine.command_highwater.is_none(),
            "command unconsumed by the refused report"
        );
        // The same sequence works once the report fits.
        let retry = report_full_v4(
            2,
            2,
            vec![screen("out-a", 0)],
            vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 400)],
            vec![],
            "",
            Some(2),
            Some(focus()),
        );
        let reply = accepted_v4(&mut engine, &retry);
        assert!(
            reply
                .effects
                .iter()
                .any(|effect| effect.is_explicit_focus_for(2)),
            "retry emits the explicit focus: {:?}",
            reply.effects
        );
    }
}

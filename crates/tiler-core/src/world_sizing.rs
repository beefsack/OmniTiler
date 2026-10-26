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
    CommandResult, FloatOrigin, HoldReason, TransferKind, WorldCommand, WorldReply, WorldReport,
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

/// Structural size estimate for one typed send source: the exact object
/// width for explicit ids (`{"explicit":S}`), and the exact token-object
/// widths for the implicit legs (`{"implicit":"home-then-action"}` and
/// `{"implicit":"home-then-cached-live"}`).
#[must_use]
pub fn send_source_estimate(source: &crate::world_report::SendSource) -> usize {
    match source {
        crate::world_report::SendSource::Explicit(output) => 13 + json_string_len(&output.0),
        crate::world_report::SendSource::HomeThenAction => 31,
        crate::world_report::SendSource::HomeThenCachedLive => 36,
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
    // "windows":[],"transfers":[],"effects":[]} = 151 plus bools. The three
    // u64 counters encode as decimal strings (two quotes each). The
    // correlated `command_result` section is counted separately in
    // [`command_result_estimate`] (`,\"result\":null` = 14 when absent),
    // the gesture-settled section in [`gesture_settled_estimate`]
    // (`,\"gestureSettled\":null` = 22 when absent), the operation-settled
    // section in [`operation_settled_estimate`]
    // (`,\"operationSettled\":[]` = 21 when empty), and the warp section
    // in [`warp_cursor_estimate`] (`,\"warpCursor\":null` = 18 when absent).
    let mut bytes = 166
        + usize_len(reply.contract_version as usize)
        + json_string_len(&reply.request_id)
        + json_string_len(&reply.adapter_incarnation)
        + json_string_len(&reply.core_generation)
        + u64_len(reply.based_on_observation_seq)
        + u64_len(reply.state_revision);
    for scope in &reply.scopes {
        // {"scope":,"auth":,"reason":} = 28 punctuation.
        bytes += 28
            + json_string_len(&scope.scope)
            + json_string_len(scope.authority.as_str())
            + json_string_len(scope.authority.hold_reason().map_or("", HoldReason::as_str));
    }
    // One comma between adjacent rows of every non-empty array.
    bytes += reply.scopes.len().saturating_sub(1);
    for workspace in &reply.workspaces {
        // {"id":,"out":,"bind":,"scope":,"order":,"mode":,
        // "exist":,"hold":} = 65 punctuation plus the exist bool.
        bytes += 65
            + json_string_len(&workspace.id)
            + json_string_len(&workspace.output.0)
            + json_string_len(workspace.native_binding.as_deref().unwrap_or_default())
            + json_string_len(&workspace.scope)
            + usize_len(workspace.order)
            + json_string_len(workspace.tiling_mode.as_str())
            + bool_len(workspace.existence)
            + json_string_len(workspace.hold.map_or("", HoldReason::as_str));
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
            + json_string_len(window.clear_operation.as_deref().unwrap_or_default());
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
            // id, ordinal digits, label, and three bools.
            bytes += 88
                + json_string_len(&workspace.id)
                + usize_len(workspace.ordinal as usize)
                + json_string_len(&workspace.label)
                + bool_len(workspace.current)
                + bool_len(workspace.occupied)
                + bool_len(workspace.contains_focused);
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
/// `frameAttempts`, `memberAttempts`, `viewAttempts`).
/// Conservative: any compact encoding of the report fits within this
/// bound (locked by `request_estimate_bounds_actual_wire_bytes`). The
/// adapter must emit compact JSON with no whitespace; key order does not
/// matter because commas and keys count the same in any order.
#[must_use]
pub fn report_estimate(report: &WorldReport) -> usize {
    // Envelope fixed bytes: {"v":2,"owner":,"adapter":,"gen":,"obs":"","reqSeq":"",
    // "req":,"cmdSeq":,"command":,"screens":[],"windows":[],"views":[],"closed":[],
    // "hostOwned":,"equalCount":,"native":[],"predicate":,"refill":,"defaultTiled":,
    // "complete":,"gesture":} = 219, plus the cursor-first
    // targeting keys (10 + 16 + 18 + 22 + 13 = 79):
    // ,"cursor":,"switchTarget":,"freshPlacement":,"focusFollowsCursor":,
    // "cursorWarp": = 308
    // plus ,"cursorHit": (13) = 321, plus ,"actionForeground": (20) and
    // ,"actionCached": (16) = 357, plus ,"actionOutputs":[] (20) = 377,
    // plus ,"monotonicMs":"" (19) and ,"frameAttempts":[] (19) = 415,
    // plus ,"memberAttempts":[] (20) and ,"viewAttempts":[] (18) = 453,
    // plus ,"clearAttempts":[] (18) = 471, plus ,"creationAttempts":[] (22)
    // = 493, plus ,"gesture":null (12) = 505, plus the version digits, plus
    // ,"presentation":{} below. Values
    // counted
    // separately below; nullable command, cursor, cursor-hit, and
    // action-context slots count `null` (4), while the action-output
    // registry counts its brackets plus one comma per entry. The workspace
    // mode token rides as `"mode":S` (9 fixed plus the token); membership
    // evidence rides per fact below (absent counts `null`).
    let mut bytes = 437
        + 36
        + 20
        + gesture_estimate(report)
        + usize_len(report.contract_version as usize)
        + json_string_len(&report.owner)
        + json_string_len(&report.adapter_incarnation)
        + json_string_len(&report.core_generation)
        + json_string_len(&report.request_id)
        + json_string_len(crate::workspace_scope::mode_token(report.workspace_mode))
        + json_string_len(report.fixed_size_predicate.as_wire_str())
        + json_string_len(report.migration_source_refill.as_wire_str())
        + json_string_len(report.anchor_fallback.as_wire_str())
        + json_string_len(report.no_space_result.as_wire_str())
        + json_string_len(report.switch_target.as_wire_str())
        + json_string_len(report.fresh_placement.as_wire_str())
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
        + bool_len(report.default_tiled)
        + bool_len(report.host_owned_workspace_domains)
        + bool_len(report.equal_workspace_count)
        + json_string_len(&report.focused_global)
        + bool_len(report.inventory_complete)
        + bool_len(report.gesture_barrier)
        + bool_len(report.focus_follows_cursor)
        + bool_len(report.cursor_warp)
        + presentation_ingress_estimate(&report.presentation);
    for screen in &report.screens {
        // {"out":S,"full":R|null,"work":R|null,"gap":N,"outer":N,
        // "complete":B,"primary":B} = 72 fixed plus rect digits, gap
        // widths, and bools.
        bytes += 72
            + rect_len(screen.full_rect)
            + rect_len(screen.work_area)
            + int_len(screen.gap)
            + int_len(screen.outer_gap)
            + bool_len(screen.complete)
            + bool_len(screen.primary)
            + json_string_len(&screen.output.0);
    }
    for fact in &report.windows {
        // {"win":S,"out":S,"rect":R|null,"floating":B,
        // "fitExcluded":B,"fullscreen":B,"maximized":B,"sticky":B,
        // "fixedAuto":B,"fixedSuppress":B,"minimized":B,
        // "observedWs":S|null,"nativeWs":S|null,
        // "hints":{"minW":N,"minH":N,"maxW":N,"maxH":N}} = 252 fixed
        // plus ids, rect digits, and hint bounds (`null` counts 4).
        bytes += 252
            + json_string_len(&fact.window.0)
            + json_string_len(&fact.output.0)
            + rect_len(fact.rect)
            + bool_len(fact.floating)
            + bool_len(fact.fit_excluded)
            + bool_len(fact.fullscreen)
            + bool_len(fact.maximized)
            + bool_len(fact.sticky)
            + bool_len(fact.fixed_auto)
            + bool_len(fact.fixed_suppress)
            + bool_len(fact.minimized)
            + fact
                .observed_workspace
                .as_deref()
                .map_or(4, json_string_len)
            + fact.native_workspace.as_deref().map_or(4, json_string_len)
            + fact.hints.min_w.map_or(4, int_len)
            + fact.hints.min_h.map_or(4, int_len)
            + fact.hints.max_w.map_or(4, int_len)
            + fact.hints.max_h.map_or(4, int_len);
    }
    for view in &report.views {
        // {"out":S,"view":S} = 17 fixed.
        bytes += 17 + json_string_len(&view.output.0) + json_string_len(&view.view);
    }
    for id in &report.closed {
        bytes += json_string_len(&id.0) + 1;
    }
    for native in &report.native {
        // {"out":S,"order":[S...],"current":S|null} = 32 fixed plus
        // strings; `null` (4) when the current surface is not an ordered
        // desktop.
        bytes += 32
            + json_string_len(&native.output.0)
            + native.current_native.as_deref().map_or(4, json_string_len);
        for id in &native.native_order {
            bytes += json_string_len(id) + 1;
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
        // {"op":S,"win":S,"out":S,"ws":S,"setter":S,"pre":H|null,
        // "post":H|null,"nativePre":S|null,"nativePost":S|null,
        // "attemptMs":"u64"} = 94 fixed plus ids, setter token, homing
        // pairs, native ids, and the stamp. A homing pair
        // `{"out":S,"ws":S}` counts 14 plus its ids; absent homings and
        // absent native ids count null (4).
        bytes += 94
            + json_string_len(&attempt.operation_id)
            + json_string_len(&attempt.window.0)
            + json_string_len(&attempt.target_output.0)
            + json_string_len(&attempt.target_workspace)
            + json_string_len(attempt.setter.as_str())
            + attempt.pre.as_ref().map_or(4, |(out, ws)| {
                14 + json_string_len(&out.0) + json_string_len(ws)
            })
            + attempt.post.as_ref().map_or(4, |(out, ws)| {
                14 + json_string_len(&out.0) + json_string_len(ws)
            })
            + attempt.native_pre.as_deref().map_or(4, json_string_len)
            + attempt.native_post.as_deref().map_or(4, json_string_len)
            + u64_len(attempt.attempt_ms);
    }
    for attempt in &report.view_attempts {
        // {"op":S,"out":S,"ws":S,"setter":S,"pre":S|null,"post":S|null,
        // "base":S|null,"nativePre":S|null,"nativePost":S|null,
        // "nativeBase":S|null,"attemptMs":"u64"} = 109 fixed plus ids,
        // setter token, views, native ids, and the stamp. Absent views and
        // absent native ids count null (4).
        bytes += 109
            + json_string_len(&attempt.operation_id)
            + json_string_len(&attempt.target_output.0)
            + json_string_len(&attempt.target_workspace)
            + json_string_len(attempt.setter.as_str())
            + attempt.pre.as_deref().map_or(4, json_string_len)
            + attempt.post.as_deref().map_or(4, json_string_len)
            + attempt.baseline.as_deref().map_or(4, json_string_len)
            + attempt.native_pre.as_deref().map_or(4, json_string_len)
            + attempt.native_post.as_deref().map_or(4, json_string_len)
            + attempt
                .native_baseline
                .as_deref()
                .map_or(4, json_string_len)
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
    // No restoration ingress exists on the wire: every report is a
    // fresh-observation report and the adapter sends no restoration key.
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
    for byte in value.bytes() {
        match byte {
            b'"' => out.push_str("\\\""),
            b'\\' => out.push_str("\\\\"),
            0x00..=0x1F => {
                const HEX: &[u8; 16] = b"0123456789abcdef";
                out.push_str("\\u00");
                out.push(char::from(HEX[usize::from(byte >> 4)]));
                out.push(char::from(HEX[usize::from(byte & 0x0F)]));
            }
            _ => out.push(char::from(byte)),
        }
    }
    out.push('"');
}

/// Exact canonical reply encoding for the transactional commit gate.
/// Measures, rather than asserts, the wire size: overflow is decided on
/// these bytes. Global `u64` counters (`obs`, `rev`, effect `rev`) encode
/// as bounded decimal strings, never JSON numbers, so JavaScript adapters
/// never lose precision past 2^53.
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
        push_json_string(&mut out, &scope.scope);
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
        push_json_string(&mut out, &workspace.scope);
        out.push_str(",\"order\":");
        out.push_str(&workspace.order.to_string());
        out.push_str(",\"mode\":");
        push_json_string(&mut out, workspace.tiling_mode.as_str());
        out.push_str(",\"exist\":");
        out.push_str(if workspace.existence { "true" } else { "false" });
        out.push_str(",\"hold\":");
        push_json_string(&mut out, workspace.hold.map_or("", HoldReason::as_str));
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
    out.push('}');
    out
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
    }
}

/// Exact canonical reply byte size for the transactional commit gate.
#[must_use]
pub fn reply_exact_bytes(reply: &WorldReply) -> usize {
    encode_reply_canonical(reply).len()
}

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
/// - gesture-settled terminal (at most one per report): 40 fixed plus a
///   20-digit id plus a 5-byte bool = 65 worst case, rounded to 96.
/// - operation-settled events (at most two per window per report: one
///   clear verdict plus one float verdict): 34 punctuation +
///   `op-<u64>` (25) + window (130) + outcome (27) + cause (130) = 346,
///   rounded to 384 each, doubled to 768 per window.
/// - native creation desires (at most one per logical workspace): 22
///   punctuation + `op-native-<n>` (25) + logical (130) + output (130) =
///   307, rounded to 384 each.
///   The 4096 base covers envelope slack and array commas.
#[must_use]
pub fn reply_upper_bound(
    report: &WorldReport,
    retained_transfers: usize,
    logical_workspaces: usize,
    created_workspaces: usize,
) -> usize {
    let report_windows: usize = report.windows.len().saturating_add(report.closed.len());
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
        .saturating_add(report_windows.saturating_mul(160))
        .saturating_add(96)
        .saturating_add(report_windows.saturating_mul(131))
        .saturating_add(report_windows.saturating_mul(768))
        .saturating_add(logical_workspaces.saturating_mul(384))
}

//! World reply/request sizing and canonical encoding (D1-D5).
//!
//! Split from [`crate::world_report`]: exact JSON string/numeric widths,
//! the structural request/reply estimators, the conservative pre-mutation
//! reply upper bound, and the canonical reply writer that the transactional
//! commit gate measures. Shapes, validation, and freshness stay in
//! [`crate::world_report`], re-exported here for one stable path.

use crate::boundary::CoreCommand;
use crate::directional::Direction;
use crate::workspace_scope::mode_token;
use crate::world_report::{
    CommandResult, HoldReason, TransferKind, WorldCommand, WorldReply, WorldReport,
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
    // [`command_result_estimate`] (`,\"result\":null` = 14 when absent).
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
        // {"id":,"out":,"bind":,"logical":,"scope":,"order":,"mode":,
        // "exist":,"hold":} = 75 punctuation plus up to 5 for exist.
        bytes += 80
            + json_string_len(&workspace.id)
            + json_string_len(&workspace.output.0)
            + json_string_len(workspace.native_binding.as_deref().unwrap_or_default())
            + json_string_len(workspace.logical_id.as_deref().unwrap_or_default())
            + json_string_len(&workspace.scope)
            + usize_len(workspace.order)
            + json_string_len(workspace.tiling_mode.as_str())
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
        // "frame_hold":,"member_auth":,"member_hold":,"frame_op":} = 109
        // punctuation.
        // Each rect is [x,y,w,h] (numbers + 5) or null (4). The explicit
        // float-placement operation id is empty when no target is
        // outstanding.
        let rect_len = |rect: Option<crate::geometry::Rect>| {
            rect.map_or(4, |r| {
                int_len(r.x) + int_len(r.y) + int_len(r.w) + int_len(r.h) + 5
            })
        };
        bytes += 109
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
            + json_string_len(window.frame_operation.as_deref().unwrap_or_default());
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
    bytes.saturating_add(reply.presentation_bytes)
}

/// Structural size estimate for the correlated `command_result` section,
/// field-by-field from [`encode_command_result`] below. `None` encodes as
/// `,"result":null` (13); every variant counts its exact keys plus value
/// widths (member/window id lists scale with the result's own rows).
#[must_use]
pub fn command_result_estimate(result: &Option<CommandResult>) -> usize {
    // `,"result":` prefix (10) plus `null` (4) when absent.
    let Some(result) = result else {
        return 14;
    };
    let rect_len =
        |r: crate::geometry::Rect| int_len(r.x) + int_len(r.y) + int_len(r.w) + int_len(r.h) + 5;
    match result {
        CommandResult::DragPreview {
            rect,
            hover,
            domain,
            source,
            revision,
        } => {
            // {"op":"drag-preview","rev":"","domainOut":"","domainWs":"",
            // "sourceOut":"","sourceWs":"","rect":[],"hover":} = 105
            // punctuation plus the revision digits and hover body.
            let mut bytes = 105
                + u64_len(*revision)
                + json_string_len(&domain.output.0)
                + json_string_len(&domain.workspace.0)
                + json_string_len(&source.output.0)
                + json_string_len(&source.workspace.0)
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
        CommandResult::Float { window, rect } => {
            // {"op":"toggle-float","win":"","rect":} = 36 punctuation
            // plus the rect body (`null` or [x,y,w,h]).
            36 + json_string_len(&window.0) + rect.map_or(4, rect_len)
        }
        CommandResult::ActiveGroup {
            base_revision,
            group,
            focused_leaf,
            focused_window,
            members,
            bounds,
        } => {
            // {"op":"active-group","rev":"","group":"","leaf":"","win":"",
            // "bounds":[],"members":[]} = 83 punctuation (two string quotes
            // around the u64 revision).
            let mut bytes = 83
                + u64_len(*base_revision)
                + json_string_len(&group.0)
                + json_string_len(&focused_leaf.0)
                + json_string_len(&focused_window.0)
                + rect_len(*bounds);
            for member in members {
                // {"leaf":"","win":"","rect":[]} = 27 punctuation.
                bytes += 27
                    + json_string_len(&member.leaf.0)
                    + json_string_len(&member.window.0)
                    + rect_len(member.rect);
            }
            bytes += members.len().saturating_sub(1);
            bytes
        }
        CommandResult::NoGroup {
            base_revision,
            reason,
        } => {
            // {"op":"no-group","reason":"","rev":} = 33 punctuation;
            // rev is null (4) or "digits" (2 + width).
            33 + json_string_len(reason.as_str()) + base_revision.map_or(4, |rev| 2 + u64_len(rev))
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
    }
}

/// Structural request size estimate over the same accounting: every
/// carried string through [`json_string_len`], exact numeric widths, and
/// per-row overhead derived field-by-field from the protocol wire layout
/// (`v`, `owner`, `adapter`, `gen`, `obs`/`reqSeq`/`cmdSeq` decimal strings,
/// nullable command, `mode`, `topology`, `bindings`, `domains`,
/// `minimized`, `predicate`, `refill`, `defaultTiled`/`focused`/`complete`/`gesture`
/// values, `feedback`).
/// Conservative: any compact encoding of the report fits within this
/// bound (locked by `request_estimate_bounds_actual_wire_bytes`). The
/// adapter must emit compact JSON with no whitespace; key order does not
/// matter because commas and keys count the same in any order.
#[must_use]
pub fn report_estimate(report: &WorldReport) -> usize {
    // Envelope fixed bytes: {"v":1,"owner":,"adapter":,"gen":,"obs":"","reqSeq":"",
    // "req":,"cmdSeq":,"command":,"mode":,"topology":[],"bindings":[],"domains":[],
    // "minimized":[],"predicate":,"refill":,"defaultTiled":,"complete":,"gesture":,"feedback":[]} = 222
    // plus the version digits, and the "focused" key (,"focused": = 11)
    // with its string value below. Values counted separately below; nullable
    // command slots count `null` (4) when absent.
    let mut bytes = 223
        + usize_len(report.contract_version as usize)
        + json_string_len(&report.owner)
        + json_string_len(&report.adapter_incarnation)
        + json_string_len(&report.core_generation)
        + json_string_len(&report.request_id)
        + json_string_len(mode_token(report.workspace_mode))
        + json_string_len(report.fixed_size_predicate.as_wire_str())
        + json_string_len(report.migration_source_refill.as_wire_str())
        + u64_len(report.request_seq)
        + u64_len(report.observation_seq)
        + report.command_seq.map_or(4, |seq| 2 + u64_len(seq))
        + report.command.as_ref().map_or(4, command_estimate_owned)
        + bool_len(report.default_tiled)
        + json_string_len(&report.focused_global)
        + bool_len(report.inventory_complete)
        + bool_len(report.gesture_barrier);
    for entry in &report.minimized {
        // {"win":S,"out":S,"ws":S} = 21 punctuation plus one comma.
        bytes += 22
            + json_string_len(&entry.window.0)
            + json_string_len(&entry.output.0)
            + json_string_len(&entry.workspace.0);
    }
    // "minimized" key plus array brackets (,"minimized":[] = 15), the
    // "predicate" key (,"predicate": = 13), the "refill" key
    // (,"refill": = 10), and the "defaultTiled" key
    // (,"defaultTiled": = 16); values counted above. The "focused" key
    // (11) counts in the envelope above; its string value counts below.
    bytes += 54;
    for topology in &report.topology {
        // {"out":S,"order":[S...],"current":S,"complete":B,"rect":R|null,"primary":B}
        // = 61 fixed plus the bool widths, the rect ints (or `null`),
        // and the native strings.
        bytes += 61
            + bool_len(topology.complete)
            + bool_len(topology.primary)
            + rect_len(topology.geometry)
            + json_string_len(&topology.output.0)
            + json_string_len(&topology.current_native);
        for native in &topology.native_order {
            bytes += json_string_len(native) + 1;
        }
    }
    for binding in &report.bindings {
        // {"logical":S,"native":S,"out":S,"pin":B,"disp":B,"managed":B} =
        // 55 fixed plus exact bool widths.
        bytes += 55
            + bool_len(binding.pinned)
            + bool_len(binding.displaced)
            + bool_len(binding.managed)
            + json_string_len(&binding.logical_id)
            + json_string_len(&binding.native_id)
            + json_string_len(&binding.output.0);
    }
    for domain in &report.domains {
        // {"id":S,"ws":S,"bounds":[N,N,N,N],"gap":N,"outer":N,
        // "focused":S,"readable":B,"hold":S,"adjacent":{...},
        // "windows":[...],"closed":[...]} = 114 fixed plus the readable
        // bool, the hold token, per-entry adjacency, and values.
        bytes += 114
            + bool_len(domain.readable)
            + json_string_len(domain.hold.map_or("", HoldReason::as_str))
            + json_string_len(&domain.domain.id.0)
            + json_string_len(&domain.domain.workspace.0)
            + int_len(domain.domain.bounds.x)
            + int_len(domain.domain.bounds.y)
            + int_len(domain.domain.bounds.w)
            + int_len(domain.domain.bounds.h)
            + int_len(domain.domain.gap)
            + int_len(domain.outer_gap)
            + json_string_len(&domain.focused.0);
        for (direction, target) in &domain.domain.adjacent {
            // "dir":S plus one comma (the last comma overcounts by one,
            // conservative).
            bytes += json_string_len(direction_token(*direction)) + json_string_len(&target.0) + 1;
        }
        for window in &domain.windows {
            // {"win":S,"out":S,"ws":S,"rect":[N,N,N,N],"floating":B,
            // "fitExcluded":B,"fullscreen":B,"maximized":B,"sticky":B,
            // "fixedAuto":B,"fixedSuppress":B,
            // "hints":{"minW":N,"minH":N,"maxW":N,"maxH":N}} = 204 fixed
            // plus ids, rect digits, and hint bounds (`null` counts 4).
            bytes += 204
                + json_string_len(&window.window.0)
                + json_string_len(&window.output.0)
                + json_string_len(&window.workspace.0)
                + int_len(window.rect.x)
                + int_len(window.rect.y)
                + int_len(window.rect.w)
                + int_len(window.rect.h)
                + bool_len(window.floating)
                + bool_len(window.fit_excluded)
                + bool_len(window.fullscreen)
                + bool_len(window.maximized)
                + bool_len(window.sticky)
                + bool_len(window.fixed_auto)
                + bool_len(window.fixed_suppress)
                + window.hints.min_w.map_or(4, int_len)
                + window.hints.min_h.map_or(4, int_len)
                + window.hints.max_w.map_or(4, int_len)
                + window.hints.max_h.map_or(4, int_len);
        }
        for id in &domain.closed {
            bytes += json_string_len(&id.0) + 1;
        }
    }
    for feedback in &report.feedback {
        // {"op":S,"kind":S,"outcome":S,"hold":S} = 34 fixed plus one comma
        // plus exact kind/outcome/hold tokens.
        bytes += 35
            + json_string_len(&feedback.operation_id)
            + json_string_len(feedback.kind.as_str())
            + json_string_len(feedback.outcome.as_str())
            + json_string_len(feedback.hold.map_or("", HoldReason::as_str));
    }
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
            // {"op":"select-numbered","out":S,"index":N} = 40 fixed.
            40 + json_string_len(&output.0) + usize_len(usize::from(*index))
        }
        WorldCommand::SelectRelative { output, delta } => {
            // {"op":"select-relative","out":S,"delta":N} = 40 fixed.
            40 + json_string_len(&output.0) + int_len(*delta)
        }
        WorldCommand::SelectPrevious { output } => {
            // {"op":"select-previous","out":S} = 31 fixed.
            31 + json_string_len(&output.0)
        }
        WorldCommand::SelectTrailing { output } => {
            // {"op":"select-trailing","out":S} = 31 fixed.
            31 + json_string_len(&output.0)
        }
        WorldCommand::SendRelative {
            window,
            output,
            delta,
            follow,
        } => {
            // {"op":"send-relative","win":S,"out":S,"delta":N,"follow":B}
            // = 55 fixed.
            55 + bool_len(*follow)
                + json_string_len(&window.0)
                + json_string_len(&output.0)
                + int_len(*delta)
        }
        WorldCommand::SendNumbered {
            window,
            output,
            index,
            follow,
        } => {
            // {"op":"send-numbered","win":S,"out":S,"index":N,"follow":B}
            // = 57 fixed.
            57 + bool_len(*follow)
                + json_string_len(&window.0)
                + json_string_len(&output.0)
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
        CoreCommand::ActiveGroup => 7 + json_string_len("active-group"),
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
        out.push_str(",\"logical\":");
        push_json_string(
            &mut out,
            workspace.logical_id.as_deref().unwrap_or_default(),
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
    out.push('}');
    out
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
            out.push('}');
        }
        Result::Float { window, rect } => {
            out.push_str("{\"op\":\"toggle-float\",\"win\":");
            push_json_string(out, &window.0);
            out.push_str(",\"rect\":");
            match rect {
                Some(r) => push_rect(out, *r),
                None => out.push_str("null"),
            }
            out.push('}');
        }
        Result::ActiveGroup {
            base_revision,
            group,
            focused_leaf,
            focused_window,
            members,
            bounds,
        } => {
            out.push_str("{\"op\":\"active-group\",\"rev\":\"");
            out.push_str(&base_revision.to_string());
            out.push_str("\",\"group\":");
            push_json_string(out, &group.0);
            out.push_str(",\"leaf\":");
            push_json_string(out, &focused_leaf.0);
            out.push_str(",\"win\":");
            push_json_string(out, &focused_window.0);
            out.push_str(",\"bounds\":");
            push_rect(out, *bounds);
            out.push_str(",\"members\":[");
            for (i, member) in members.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str("{\"leaf\":");
                push_json_string(out, &member.leaf.0);
                out.push_str(",\"win\":");
                push_json_string(out, &member.window.0);
                out.push_str(",\"rect\":");
                push_rect(out, member.rect);
                out.push('}');
            }
            out.push_str("]}");
        }
        Result::NoGroup {
            base_revision,
            reason,
        } => {
            out.push_str("{\"op\":\"no-group\",\"reason\":");
            push_json_string(out, reason.as_str());
            out.push_str(",\"rev\":");
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
/// - pending-creation workspace row: 80 punctuation + token (25) + output
///   (130) + empty binding (2) + logical (25) + scope (130) + order (20) +
///   mode (7) + hold (23) = 442, rounded to 448;
/// - retained-missing held scope row: 28 punctuation + `output/workspace`
///   (259) + authority (12) + reason (23) = 322, rounded to 336;
/// - one emitted effect row (at most one per command): 32 + id (25) +
///   kind (9) + target (130) + revision (20) = 216, rounded to 256.
/// - correlated command result (at most one per command): the drag-preview
///   / float / active-group shapes fit in 2048; a migration member list
///   scales with the report's live window count (one max-length id + comma
///   per window, 131 each) plus 512 base.
///   The 4096 base covers envelope slack and array commas.
#[must_use]
pub fn reply_upper_bound(
    report: &WorldReport,
    retained_transfers: usize,
    retained_sessions: usize,
    pending_creations: usize,
) -> usize {
    let report_windows: usize = report
        .domains
        .iter()
        .map(|domain| domain.windows.len())
        .sum::<usize>()
        .saturating_add(report.minimized.len());
    report_estimate(report)
        .saturating_add(retained_transfers.saturating_mul(512))
        .saturating_add(pending_creations.saturating_mul(448))
        .saturating_add(retained_sessions.saturating_mul(336))
        .saturating_add(4096)
        .saturating_add(256)
        .saturating_add(2048)
        .saturating_add(512)
        .saturating_add(report_windows.saturating_mul(131))
}

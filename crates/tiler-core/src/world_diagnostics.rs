//! Internal resize/trace diagnostics: bounded refusal lines and trace-only
//! stderr writes (or isolated thread-local capture under unit tests).

use crate::directional::WindowId;
use crate::world_engine::WorldState;
use crate::world_report::{CommandRejectKind, WorldCommand, WorldReportV4};
use crate::world_transaction::WorldTransaction;

/// Diagnostic bound for one resize-refusal stderr line, suffix included.
const RESIZE_REFUSAL_DIAG_MAX: usize = 960;
/// Staged domain rows rendered into one resize-refusal line.
const RESIZE_REFUSAL_DIAG_ROWS: usize = 4;
/// Root shares rendered into one resize-refusal line.
const RESIZE_REFUSAL_DIAG_SHARES: usize = 8;
/// Leaves scanned to locate the focused leaf.
const RESIZE_REFUSAL_DIAG_LEAVES: usize = 16;
/// Truncation marker; the byte cap reserves exactly this length.
const RESIZE_REFUSAL_DIAG_CUT: &str = "...";

/// Diagnostic bound for one generic per-command refusal stderr line, suffix
/// included. Covers the whole rejected-command evidence (executing identity
/// plus the verbatim internal kind); resize commands additionally keep their
/// existing detail line below.
const COMMAND_REFUSAL_DIAG_MAX: usize = 512;

/// Sanitized opaque id for diagnostics: the validated value or `-`.
pub(crate) fn diag_id(value: &str) -> &str {
    if crate::bounds::is_opaque_id(value) {
        value
    } else {
        "-"
    }
}

/// Sanitized closed-vocabulary token for diagnostics: lowercase/dash or `-`.
fn diag_token(value: &str, max: usize) -> &str {
    if !value.is_empty()
        && value.len() <= max
        && value.bytes().all(|b| b.is_ascii_lowercase() || b == b'-')
    {
        value
    } else {
        "-"
    }
}

/// Closed-vocabulary op token for a per-command refusal diagnostic: the
/// top-level world command name, or the core `op` token for the operation
/// route. Admission validates every shape, so every arm is reachable;
/// `Implicit` unwraps to its payload (bookkeeping normally resolves it
/// first, so that path only sees the raw inline spelling).
fn diag_command_op(command: &WorldCommand) -> &'static str {
    match command {
        WorldCommand::Focus { .. } => "focus",
        WorldCommand::Send { .. } => "send",
        WorldCommand::SwitchView { .. } => "switch-view",
        WorldCommand::SelectWorkspace { .. } => "select-workspace",
        WorldCommand::SelectNumbered { .. } => "select-numbered",
        WorldCommand::SelectRelative { .. } => "select-relative",
        WorldCommand::SelectPrevious { .. } => "select-previous",
        WorldCommand::SelectTrailing { .. } => "select-trailing",
        WorldCommand::SendRelative { .. } => "send-relative",
        WorldCommand::SendNumbered { .. } => "send-numbered",
        WorldCommand::SetWorkspaceTiling { .. } => "set-workspace-tiling",
        WorldCommand::ToggleWorkspaceTiling { .. } => "toggle-workspace-tiling",
        WorldCommand::MigrateWorkspace { .. } => "migrate-workspace",
        WorldCommand::FocusDirectional { .. } => "focus-directional",
        WorldCommand::MoveDirectional { .. } => "move-directional",
        WorldCommand::SendDirectional { .. } => "send-directional",
        WorldCommand::MigrateDirectional { .. } => "migrate-directional",
        WorldCommand::ToggleMaximize { .. } => "toggle-maximize",
        WorldCommand::SetSticky { .. } => "set-sticky",
        WorldCommand::ToggleSticky { .. } => "toggle-sticky",
        WorldCommand::SetFullscreen { .. } => "set-fullscreen",
        WorldCommand::ToggleFullscreen { .. } => "toggle-fullscreen",
        WorldCommand::Operation(core) => core.op(),
    }
}

/// Window subject of a per-command refusal diagnostic, when the command
/// names one. Execution-time-focus and no-focus subjects return `None`.
fn diag_command_subject(command: &WorldCommand) -> Option<&str> {
    use crate::boundary::CoreCommand;
    use crate::directional::WindowSubject;
    use crate::world_report::FocusDirectionalSubject;
    match command {
        WorldCommand::Focus {
            window: WindowSubject::Window(window),
            ..
        }
        | WorldCommand::Send {
            window: WindowSubject::Window(window),
            ..
        }
        | WorldCommand::SendRelative {
            window: WindowSubject::Window(window),
            ..
        }
        | WorldCommand::SendNumbered {
            window: WindowSubject::Window(window),
            ..
        }
        | WorldCommand::FocusDirectional {
            window: FocusDirectionalSubject::Window(window),
            ..
        }
        | WorldCommand::MoveDirectional {
            window: WindowSubject::Window(window),
            ..
        }
        | WorldCommand::SendDirectional {
            window: WindowSubject::Window(window),
            ..
        }
        | WorldCommand::ToggleMaximize {
            window: WindowSubject::Window(window),
        }
        | WorldCommand::SetSticky {
            window: WindowSubject::Window(window),
            ..
        }
        | WorldCommand::ToggleSticky {
            window: WindowSubject::Window(window),
        }
        | WorldCommand::SetFullscreen {
            window: WindowSubject::Window(window),
            ..
        }
        | WorldCommand::ToggleFullscreen {
            window: WindowSubject::Window(window),
        } => Some(window.0.as_str()),
        WorldCommand::Operation(
            CoreCommand::Move { window, .. }
            | CoreCommand::Focus { window, .. }
            | CoreCommand::PointerResize { window, .. }
            | CoreCommand::SendToWorkspace { window, .. }
            | CoreCommand::SendToOutput { window, .. }
            | CoreCommand::DragDrop { window, .. }
            | CoreCommand::DragPreview { window, .. },
        ) => Some(window.0.as_str()),
        WorldCommand::Operation(
            CoreCommand::Resize {
                window: WindowSubject::Window(window),
                ..
            }
            | CoreCommand::ToggleFloat {
                window: WindowSubject::Window(window),
                ..
            }
            | CoreCommand::ToggleOrientation {
                window: WindowSubject::Window(window),
            },
        ) => Some(window.0.as_str()),
        _ => None,
    }
}

/// Direction detail of a per-command refusal diagnostic, when the command
/// carries one. Closed vocabulary on the wire; anything else sanitizes at
/// the call site through [`diag_token`].
fn diag_command_dir(command: &WorldCommand) -> Option<&str> {
    match command {
        WorldCommand::Focus { direction, .. } => {
            direction.map(crate::directional::Direction::as_wire_str)
        }
        WorldCommand::FocusDirectional { direction, .. }
        | WorldCommand::MoveDirectional { direction, .. }
        | WorldCommand::SendDirectional { direction, .. }
        | WorldCommand::MigrateDirectional { direction } => Some(direction.as_wire_str()),
        WorldCommand::Operation(
            crate::boundary::CoreCommand::Move { direction, .. }
            | crate::boundary::CoreCommand::Focus { direction, .. }
            | crate::boundary::CoreCommand::Resize { direction, .. }
            | crate::boundary::CoreCommand::PointerResize { direction, .. }
            | crate::boundary::CoreCommand::MigrateWorkspace { direction },
        ) => Some(direction.as_wire_str()),
        WorldCommand::Operation(_) => None,
        _ => None,
    }
}

/// Root tree shape for diagnostics: axis, child count, capped shares, and
/// the focused position as a capped-DFS leaf index (not a root-child
/// index). Nested groups stay unexpanded, so this never classifies
/// nested-neighbor geometry and never re-derives policy selection.
fn diag_tree(
    node: &crate::directional::Node,
    focus: Option<&crate::directional::NodeId>,
) -> String {
    fn leaves_capped(node: &crate::directional::Node, out: &mut Vec<crate::directional::NodeId>) {
        if out.len() >= RESIZE_REFUSAL_DIAG_LEAVES {
            return;
        }
        match node {
            crate::directional::Node::Leaf { id } => out.push(id.clone()),
            crate::directional::Node::Group { children, .. } => {
                for child in children {
                    leaves_capped(child, out);
                    if out.len() >= RESIZE_REFUSAL_DIAG_LEAVES {
                        return;
                    }
                }
            }
        }
    }
    let mut leaves = Vec::new();
    leaves_capped(node, &mut leaves);
    let focus_pos = focus.and_then(|id| leaves.iter().position(|leaf| leaf == id));
    let shape = match node {
        crate::directional::Node::Leaf { .. } => "leaf".to_owned(),
        crate::directional::Node::Group {
            axis,
            children,
            shares,
            ..
        } => {
            let axis = match axis {
                crate::directional::Axis::Horizontal => "H",
                crate::directional::Axis::Vertical => "V",
            };
            let mut shown: Vec<String> = shares
                .iter()
                .take(RESIZE_REFUSAL_DIAG_SHARES)
                .map(|s| s.to_string())
                .collect();
            if shares.len() > RESIZE_REFUSAL_DIAG_SHARES {
                shown.push(format!("+{}", shares.len() - RESIZE_REFUSAL_DIAG_SHARES));
            }
            format!(
                "axis:{axis}/kids:{}/shares:[{}]",
                children.len(),
                shown.join(",")
            )
        }
    };
    match focus_pos {
        Some(pos) => format!("{shape}/leafIndex@{pos}"),
        None => format!("{shape}/leafIndex@-"),
    }
}

/// Meaningful min/max hints for diagnostics, one `-` per absent bound.
fn diag_hints(hints: crate::size_hints::WindowSizeHints) -> String {
    let opt = |v: Option<i32>| v.map(|n| n.to_string()).unwrap_or_else(|| "-".to_owned());
    format!(
        "min{}x{}/max{}x{}",
        opt(hints.meaningful_min_w()),
        opt(hints.meaningful_min_h()),
        opt(hints.meaningful_max_w()),
        opt(hints.meaningful_max_h()),
    )
}

/// Best-effort diagnostic write; stderr failure never fails the command.
pub(crate) fn emit_resize_refusal(line: String) {
    use std::io::Write as _;
    let _ = writeln!(std::io::stderr(), "{line}");
}

const TRACE_FAILED_CAP: usize = 8;

pub(crate) fn format_bounded_failed(failed: &[WindowId]) -> (String, usize) {
    let shown = failed
        .iter()
        .take(TRACE_FAILED_CAP)
        .map(|win| diag_id(&win.0))
        .collect::<Vec<_>>()
        .join(",");
    let omitted = failed.len().saturating_sub(TRACE_FAILED_CAP);
    (shown, omitted)
}

#[cfg(test)]
std::thread_local! {
    static TEST_FOCUS_TRACE_SINK: std::cell::RefCell<Option<Vec<String>>> =
        const { std::cell::RefCell::new(None) };
    static TEST_SETTLEMENT_TRACE_SINK: std::cell::RefCell<Option<Vec<String>>> =
        const { std::cell::RefCell::new(None) };
    static TEST_DRAG_PREVIEW_TRACE_SINK: std::cell::RefCell<Option<Vec<String>>> =
        const { std::cell::RefCell::new(None) };
    static TEST_RETAINED_STATE_TRACE_SINK: std::cell::RefCell<Option<Vec<String>>> =
        const { std::cell::RefCell::new(None) };
    static TEST_FOCUS_TRACE_OVERRIDE: std::cell::RefCell<Option<bool>> =
        const { std::cell::RefCell::new(None) };
}

/// Cached process-level `OMNITILER_TRACE=1` gate for core trace-only
/// diagnostics (zero formatting/allocation/output when disabled).
pub(crate) fn core_trace_enabled() -> bool {
    #[cfg(test)]
    if let Some(enabled) = TEST_FOCUS_TRACE_OVERRIDE.with(|flag| *flag.borrow()) {
        return enabled;
    }
    #[cfg(test)]
    if TEST_FOCUS_TRACE_SINK.with(|sink| sink.borrow().is_some())
        || TEST_SETTLEMENT_TRACE_SINK.with(|sink| sink.borrow().is_some())
        || TEST_DRAG_PREVIEW_TRACE_SINK.with(|sink| sink.borrow().is_some())
        || TEST_RETAINED_STATE_TRACE_SINK.with(|sink| sink.borrow().is_some())
    {
        return true;
    }
    static FLAG: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *FLAG.get_or_init(|| std::env::var_os("OMNITILER_TRACE").is_some_and(|v| v == "1"))
}

/// Best-effort trace-only focus diagnostic write (or isolated thread-local
/// capture in unit tests).
pub(crate) fn emit_focus_trace(line: String) {
    #[cfg(test)]
    {
        if TEST_FOCUS_TRACE_OVERRIDE.with(|flag| *flag.borrow() == Some(false)) {
            return;
        }
        let captured = TEST_FOCUS_TRACE_SINK.with(|sink| {
            let mut borrow = sink.borrow_mut();
            if let Some(lines) = borrow.as_mut() {
                lines.push(line.clone());
                true
            } else {
                false
            }
        });
        if captured {
            return;
        }
    }
    emit_resize_refusal(line);
}

/// Best-effort trace-only published-settlement diagnostic write (or isolated
/// thread-local capture in unit tests).
pub(crate) fn emit_settlement_trace(line: String) {
    #[cfg(test)]
    {
        if TEST_FOCUS_TRACE_OVERRIDE.with(|flag| *flag.borrow() == Some(false)) {
            return;
        }
        let captured = TEST_SETTLEMENT_TRACE_SINK.with(|sink| {
            let mut borrow = sink.borrow_mut();
            if let Some(lines) = borrow.as_mut() {
                lines.push(line.clone());
                true
            } else {
                false
            }
        });
        if captured {
            return;
        }
        if TEST_FOCUS_TRACE_SINK.with(|sink| sink.borrow().is_some()) {
            return;
        }
    }
    emit_resize_refusal(line);
}

/// Best-effort trace-only published drag-preview diagnostic write (or isolated
/// thread-local capture in unit tests).
pub(crate) fn emit_drag_preview_trace(line: String) {
    #[cfg(test)]
    {
        if TEST_FOCUS_TRACE_OVERRIDE.with(|flag| *flag.borrow() == Some(false)) {
            return;
        }
        let captured = TEST_DRAG_PREVIEW_TRACE_SINK.with(|sink| {
            let mut borrow = sink.borrow_mut();
            if let Some(lines) = borrow.as_mut() {
                lines.push(line.clone());
                true
            } else {
                false
            }
        });
        if captured {
            return;
        }
        if TEST_FOCUS_TRACE_SINK.with(|sink| sink.borrow().is_some())
            || TEST_SETTLEMENT_TRACE_SINK.with(|sink| sink.borrow().is_some())
        {
            return;
        }
    }
    emit_resize_refusal(line);
}

/// Best-effort trace-only published retained-state and pending-op diagnostic
/// write (or isolated thread-local capture in unit tests).
pub(crate) fn emit_retained_state_trace(line: String) {
    #[cfg(test)]
    {
        if TEST_FOCUS_TRACE_OVERRIDE.with(|flag| *flag.borrow() == Some(false)) {
            return;
        }
        let captured = TEST_RETAINED_STATE_TRACE_SINK.with(|sink| {
            let mut borrow = sink.borrow_mut();
            if let Some(lines) = borrow.as_mut() {
                lines.push(line.clone());
                true
            } else {
                false
            }
        });
        if captured {
            return;
        }
        if TEST_FOCUS_TRACE_SINK.with(|sink| sink.borrow().is_some())
            || TEST_SETTLEMENT_TRACE_SINK.with(|sink| sink.borrow().is_some())
            || TEST_DRAG_PREVIEW_TRACE_SINK.with(|sink| sink.borrow().is_some())
        {
            return;
        }
    }
    emit_resize_refusal(line);
}

#[cfg(test)]
pub(crate) fn with_focus_trace_capture<F: FnOnce()>(f: F) -> Vec<String> {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            TEST_FOCUS_TRACE_SINK.with(|sink| {
                *sink.borrow_mut() = None;
            });
        }
    }
    TEST_FOCUS_TRACE_SINK.with(|sink| {
        *sink.borrow_mut() = Some(Vec::new());
    });
    let _reset = Reset;
    f();
    TEST_FOCUS_TRACE_SINK.with(|sink| sink.borrow_mut().take().unwrap_or_default())
}

#[cfg(test)]
pub(crate) fn with_settlement_trace_capture<F: FnOnce()>(f: F) -> Vec<String> {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            TEST_SETTLEMENT_TRACE_SINK.with(|sink| {
                *sink.borrow_mut() = None;
            });
        }
    }
    TEST_SETTLEMENT_TRACE_SINK.with(|sink| {
        *sink.borrow_mut() = Some(Vec::new());
    });
    let _reset = Reset;
    f();
    TEST_SETTLEMENT_TRACE_SINK.with(|sink| sink.borrow_mut().take().unwrap_or_default())
}

#[cfg(test)]
pub(crate) fn with_drag_preview_trace_capture<F: FnOnce()>(f: F) -> Vec<String> {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            TEST_DRAG_PREVIEW_TRACE_SINK.with(|sink| {
                *sink.borrow_mut() = None;
            });
        }
    }
    TEST_DRAG_PREVIEW_TRACE_SINK.with(|sink| {
        *sink.borrow_mut() = Some(Vec::new());
    });
    let _reset = Reset;
    f();
    TEST_DRAG_PREVIEW_TRACE_SINK.with(|sink| sink.borrow_mut().take().unwrap_or_default())
}

#[cfg(test)]
pub(crate) fn with_retained_state_trace_capture<F: FnOnce()>(f: F) -> Vec<String> {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            TEST_RETAINED_STATE_TRACE_SINK.with(|sink| {
                *sink.borrow_mut() = None;
            });
        }
    }
    TEST_RETAINED_STATE_TRACE_SINK.with(|sink| {
        *sink.borrow_mut() = Some(Vec::new());
    });
    let _reset = Reset;
    f();
    TEST_RETAINED_STATE_TRACE_SINK.with(|sink| sink.borrow_mut().take().unwrap_or_default())
}

#[cfg(test)]
pub(crate) fn with_focus_trace_disabled<F: FnOnce()>(f: F) {
    struct Reset(Option<bool>);
    impl Drop for Reset {
        fn drop(&mut self) {
            TEST_FOCUS_TRACE_OVERRIDE.with(|flag| {
                *flag.borrow_mut() = self.0;
            });
        }
    }
    let prev = TEST_FOCUS_TRACE_OVERRIDE.with(|flag| flag.borrow_mut().replace(false));
    let _reset = Reset(prev);
    f();
}

impl WorldTransaction<'_> {
    /// Diagnostic-only generic refusal line for one rejected v4 batch
    /// command: the executing command sequence, the resolved bookkeeping
    /// identity (op/subject/dir), the verbatim internal `kind`, the same
    /// `parse`-or-default wire token production maps to, and the retained
    /// focus. Pure formatting; the caller owns the stderr write and the
    /// requester outcome is untouched. Always renders (never `None`), so
    /// every refusal joins to its command even when the report carries no
    /// inline command (queued entries) or the command is not a resize.
    /// Ids pass the opaque gate, kinds/directions the closed gate; no
    /// titles or process fields.
    pub(crate) fn command_refusal_diagnostic_v4(
        &self,
        report: &WorldReportV4,
        command: &WorldCommand,
        command_seq: u64,
        kind: &str,
    ) -> String {
        let wire = CommandRejectKind::parse(kind)
            .unwrap_or(CommandRejectKind::OperationRefused)
            .as_str();
        let mut line = format!(
            "omnitiler:command-refused req={} reqseq={} obsseq={} cmdseq={} op={} subject={} dir={} internal={} wire={wire} focused={}",
            diag_id(&report.request_id),
            report.request_seq,
            report.observation_seq,
            command_seq,
            diag_command_op(command),
            diag_command_subject(command).map(diag_id).unwrap_or("-"),
            diag_command_dir(command)
                .map(|dir| diag_token(dir, 16))
                .unwrap_or("-"),
            diag_token(kind, 32),
            diag_id(&self.world.staged.observation.focused_global),
        );
        if line.len() > COMMAND_REFUSAL_DIAG_MAX {
            let mut end = COMMAND_REFUSAL_DIAG_MAX - RESIZE_REFUSAL_DIAG_CUT.len();
            while !line.is_char_boundary(end) {
                end -= 1;
            }
            line.truncate(end);
            line.push_str(RESIZE_REFUSAL_DIAG_CUT);
        }
        line
    }

    /// v4 resize detail for the actually executing command: the same staged
    /// context as [`WorldEngine::resize_refusal_diagnostic`] but bound
    /// to the executing (bookkeeping-resolved) command and its sequence
    /// instead of the report's inline pair, so queued resizes keep their
    /// detail too. `None` for non-resize commands.
    pub(crate) fn resize_refusal_diagnostic_v4_for_command(
        &self,
        report: &WorldReportV4,
        command: &WorldCommand,
        command_seq: u64,
        kind: &str,
    ) -> Option<String> {
        self.resize_refusal_line(
            command,
            &report.request_id,
            report.request_seq,
            report.observation_seq,
            Some(command_seq),
            kind,
        )
    }

    fn resize_refusal_line(
        &self,
        command: &WorldCommand,
        request_id: &str,
        request_seq: u64,
        observation_seq: u64,
        command_seq: Option<u64>,
        kind: &str,
    ) -> Option<String> {
        let WorldCommand::Operation(crate::boundary::CoreCommand::Resize {
            window,
            direction,
            mode,
            press_index,
        }) = command
        else {
            return None;
        };
        let subject = match window {
            crate::directional::WindowSubject::Window(window) => Some(window),
            crate::directional::WindowSubject::CurrentFocus => None,
        };
        let homing = subject.and_then(|window| self.world.staged.homing.get(window));
        let homing_token = homing
            .map(|key| format!("{}/{}", diag_id(&key.output.0), diag_id(&key.workspace.0)))
            .unwrap_or_else(|| "-".to_owned());
        let (focus_leaf, tree_token) = homing
            .map(|key| {
                let leaf = self.world.layout.focus_leaf(key);
                let leaf_token = leaf
                    .map(|id| diag_id(&id.0).to_owned())
                    .unwrap_or_else(|| "-".to_owned());
                let tree = self
                    .world
                    .layout
                    .tree(key)
                    .map(|node| diag_tree(node, leaf))
                    .unwrap_or_else(|| "-".to_owned());
                (leaf_token, tree)
            })
            .unwrap_or_else(|| ("-".to_owned(), "-".to_owned()));
        let (subj_geom, subj_hints) = subject
            .and_then(|window| self.world.staged.facts.get(window))
            .map(|fact| {
                let geom = fact
                    .rect
                    .map(|rect| format!("{},{},{}x{}", rect.x, rect.y, rect.w, rect.h))
                    .unwrap_or_else(|| "-".to_owned());
                let hints = fact.hints.map(diag_hints).unwrap_or_else(|| "-".to_owned());
                (geom, hints)
            })
            .unwrap_or_else(|| ("-".to_owned(), "-".to_owned()));
        let mut rows = Vec::new();
        let mut rows_total = 0;
        if let Some(key) = homing
            && let Some(domain) = self.world.staged.domains.iter().find(|d| &d.key == key)
        {
            rows_total = domain.windows.len();
            for row in domain.windows.iter().take(RESIZE_REFUSAL_DIAG_ROWS) {
                rows.push(format!(
                    "{}@{},{},{}x{}/{}",
                    diag_id(&row.window.0),
                    row.rect.x,
                    row.rect.y,
                    row.rect.w,
                    row.rect.h,
                    diag_hints(row.hints),
                ));
            }
        }
        let rows_shown = rows.len();
        if rows_total > rows_shown {
            rows.push(format!("+{}-more", rows_total - rows_shown));
        }
        let wire = CommandRejectKind::parse(kind)
            .unwrap_or(CommandRejectKind::OperationRefused)
            .as_str();
        let mut line = format!(
            "omnitiler:resize-refused req={} reqseq={} obsseq={} cmdseq={} op=resize subject={} dir={} mode={} press={} internal={} wire={wire} focused={} homing={homing_token} focus={focus_leaf} tree={tree_token} subjgeom={subj_geom} subjhints={subj_hints} rowsTotal={rows_total} rowsShown={rows_shown} rows=[{}]",
            diag_id(request_id),
            request_seq,
            observation_seq,
            command_seq
                .map(|seq| seq.to_string())
                .unwrap_or_else(|| "-".to_owned()),
            subject.map(|window| diag_id(&window.0)).unwrap_or("-"),
            diag_token(direction.as_wire_str(), 16),
            diag_token(mode.as_wire_str(), 16),
            press_index,
            diag_token(kind, 32),
            diag_id(&self.world.staged.observation.focused_global),
            rows.join(" "),
        );
        if line.len() > RESIZE_REFUSAL_DIAG_MAX {
            let mut end = RESIZE_REFUSAL_DIAG_MAX - RESIZE_REFUSAL_DIAG_CUT.len();
            while !line.is_char_boundary(end) {
                end -= 1;
            }
            line.truncate(end);
            line.push_str(RESIZE_REFUSAL_DIAG_CUT);
        }
        Some(line)
    }
}

impl WorldTransaction<'_> {
    /// Record the current staged run's `drag_preview` branch reason when core
    /// tracing is enabled (zero cost when disabled).
    pub(crate) fn set_drag_preview_trace_reason(&mut self, reason: &'static str) {
        if core_trace_enabled() {
            self.world.gesture.staged_drag_preview_reason = Some(reason);
        }
    }
}

impl WorldState {
    /// Emit trace-only per-published-reply retained settlement summary and
    /// carried outcome rows (`OMNITILER_TRACE=1`, zero cost when disabled).
    pub(crate) fn emit_published_settlement_trace(
        &self,
        prev_reply: Option<&crate::world_report::WorldReplyV4>,
        reply: &crate::world_report::WorldReplyV4,
        is_duplicate: bool,
    ) {
        if !core_trace_enabled() {
            return;
        }
        let gen_str = diag_id(&reply.core_generation);
        let req_str = diag_id(&reply.request_id);
        let rev = reply.state_revision;
        let obs = reply.based_on_observation_seq;
        let gesture_state = if reply.gesture_settled.is_some() {
            "present"
        } else {
            "none"
        };
        emit_settlement_trace(format!(
            "omnitiler:core:trace ev=settlement-published gen={} req={} rev={} obs={} ready={} dup={} ops={} switches={} gesture={}",
            gen_str,
            req_str,
            rev,
            obs,
            reply.ready,
            is_duplicate,
            reply.operation_settled.len(),
            reply.switch_settled.len(),
            gesture_state,
        ));
        if let Some(gesture) = &reply.gesture_settled {
            let state = if !is_duplicate
                && prev_reply.and_then(|prev| prev.gesture_settled.as_ref()) != Some(gesture)
            {
                "new"
            } else {
                "retained"
            };
            let pending = self
                .gesture
                .retained
                .as_ref()
                .map(|g| g.id.to_string())
                .unwrap_or_else(|| "-".to_owned());
            emit_settlement_trace(format!(
                "omnitiler:core:trace ev=settlement-gesture gen={} req={} rev={} obs={} id={} owner={} kind=gesture outcome={} state={} pending={}",
                gen_str,
                req_str,
                rev,
                obs,
                gesture.id,
                gen_str,
                if gesture.committed {
                    "committed"
                } else {
                    "uncommitted"
                },
                state,
                pending,
            ));
        }
        for event in &reply.operation_settled {
            let win_str = diag_id(&event.window.0);
            let kind_str = event.kind.as_str();
            let state = if !is_duplicate
                && prev_reply.and_then(|prev| {
                    prev.operation_settled
                        .iter()
                        .find(|prior| prior.window == event.window && prior.kind == event.kind)
                }) != Some(event)
            {
                "new"
            } else {
                "retained"
            };
            let pending = self
                .settlement
                .pending_operation_id(&event.window, event.kind)
                .map_or("-", diag_id);
            emit_settlement_trace(format!(
                "omnitiler:core:trace ev=settlement-operation gen={} req={} rev={} obs={} key={}:{} owner={} kind={} op={} outcome={} cause={} state={} pending={}",
                gen_str,
                req_str,
                rev,
                obs,
                win_str,
                kind_str,
                win_str,
                kind_str,
                diag_id(&event.operation_id),
                event.outcome.as_str(),
                event.cause.as_deref().map_or("-", diag_id),
                state,
                pending,
            ));
        }
        for event in &reply.switch_settled {
            let out_str = diag_id(&event.output.0);
            let state = if !is_duplicate
                && prev_reply.and_then(|prev| {
                    prev.switch_settled
                        .iter()
                        .find(|prior| prior.output == event.output)
                }) != Some(event)
            {
                "new"
            } else {
                "retained"
            };
            let pending = self
                .settlement
                .retained_selection
                .get(&event.output)
                .map_or("-", |retained| diag_id(&retained.parent_op));
            let (failed_shown, omitted) = format_bounded_failed(&event.failed);
            if omitted > 0 {
                emit_settlement_trace(format!(
                    "omnitiler:core:trace ev=settlement-switch gen={} req={} rev={} obs={} key={} owner={} kind=switch op={} outcome={} focus={} fallback={} state={} pending={} failed={} failedOmitted={}",
                    gen_str,
                    req_str,
                    rev,
                    obs,
                    out_str,
                    out_str,
                    diag_id(&event.operation_id),
                    event.outcome.as_str(),
                    event.focus.as_str(),
                    event.fallback,
                    state,
                    pending,
                    failed_shown,
                    omitted,
                ));
            } else {
                emit_settlement_trace(format!(
                    "omnitiler:core:trace ev=settlement-switch gen={} req={} rev={} obs={} key={} owner={} kind=switch op={} outcome={} focus={} fallback={} state={} pending={} failed={}",
                    gen_str,
                    req_str,
                    rev,
                    obs,
                    out_str,
                    out_str,
                    diag_id(&event.operation_id),
                    event.outcome.as_str(),
                    event.focus.as_str(),
                    event.fallback,
                    state,
                    pending,
                    failed_shown,
                ));
            }
        }
        emit_retained_state_trace(format!(
            "omnitiler:core:trace ev=settlement-pending gen={} req={} rev={} obs={} ready={} dup={} pendingFloats={} pendingClears={} pendingBornClears={} pendingTransfers={} pendingCreations={} retainedStates={}",
            gen_str,
            req_str,
            rev,
            obs,
            reply.ready,
            is_duplicate,
            self.settlement.retained_floats.len(),
            self.settlement.retained_clears.len(),
            self.settlement.retained_born.len(),
            self.settlement.retained.len(),
            self.settlement.retained_creations.len(),
            self.settlement.retained_states.len(),
        ));
        for binding in self
            .settlement
            .retained_states
            .values()
            .take(crate::world_state::MAX_RETAINED_STATE_BINDINGS)
        {
            let win_str = diag_id(&binding.target.window.0);
            let prop_str = binding.target.property.as_str();
            let latched = binding
                .above_meta
                .as_ref()
                .map_or("-", |meta| if meta.latched { "1" } else { "0" });
            let published = reply
                .windows
                .iter()
                .find(|w| w.lifetime == binding.target.window)
                .and_then(|w| {
                    w.state
                        .iter()
                        .find(|s| s.property == binding.target.property)
                });
            let (pub_auth, pub_hold) = match published {
                Some(target) => (
                    target.authority.as_str(),
                    match target.authority {
                        crate::world_report::ScopeAuthority::Hold(reason) => reason.as_str(),
                        _ => "-",
                    },
                ),
                None => ("absent", "-"),
            };
            emit_retained_state_trace(format!(
                "omnitiler:core:trace ev=retained-state gen={} req={} rev={} obs={} win={} prop={} targetRev={} targetVal={} spent={} pending={} disposition={} latched={} pubAuth={} pubHold={}",
                gen_str,
                req_str,
                rev,
                obs,
                win_str,
                prop_str,
                binding.target.revision,
                binding.target.value,
                binding.spent,
                binding.pending.is_some(),
                binding.disposition.as_str(),
                latched,
                pub_auth,
                pub_hold,
            ));
        }
    }

    /// Emit trace-only per-published-reply `drag_preview` decision summary
    /// (`OMNITILER_TRACE=1`, zero cost when disabled).
    pub(crate) fn emit_published_drag_preview_trace(
        &self,
        reply: &crate::world_report::WorldReplyV4,
        is_duplicate: bool,
    ) {
        if !core_trace_enabled() {
            return;
        }
        let gen_str = diag_id(&reply.core_generation);
        let req_str = diag_id(&reply.request_id);
        let rev = reply.state_revision;
        let obs = reply.based_on_observation_seq;
        let (preview_state, id_str, domain_out) = match &reply.drag_preview {
            Some(preview) => (
                "present",
                preview.id.to_string(),
                diag_id(&preview.domain.output.0),
            ),
            None => ("none", "-".to_owned(), "-"),
        };
        let suspended_out = reply
            .presentation
            .suspended_output
            .as_ref()
            .map(|output| diag_id(&output.0))
            .unwrap_or("-");
        let reason = if is_duplicate {
            "duplicate"
        } else if !reply.ready {
            "bootstrap"
        } else {
            self.gesture.staged_drag_preview_reason.unwrap_or("unknown")
        };
        emit_drag_preview_trace(format!(
            "omnitiler:core:trace ev=drag-preview gen={} req={} rev={} obs={} ready={} dup={} preview={} id={} domainOut={} suspendedOut={} reason={}",
            gen_str,
            req_str,
            rev,
            obs,
            reply.ready,
            is_duplicate,
            preview_state,
            id_str,
            domain_out,
            suspended_out,
            reason,
        ));
    }
}

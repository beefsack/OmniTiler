//! Serde-free typed command vocabulary.
//!
//! Portable command carriers over adapter-normalized integer geometry only: no
//! transport, JSON, platform, or process imports.
//! This module owns no retained state.

use crate::directional::{
    Direction, OutputId, ResizeSelector, SameAxisMove, WindowId, WindowSubject, WorkspaceId,
};
use crate::geometry::Rect;
use crate::session::{DomainKey, DragHoverPrior};

/// Typed command for all 14 wire ops: reconcile, update-gaps,
/// release-domain, move, focus, resize, pointer-resize, toggle-float,
/// toggle-orientation, `send-to-workspace`, `send-to-output`,
/// `migrate-workspace`, `drag-drop`, and read-only `drag-preview`.
/// Payloads carry typed directions and closed resize selectors decoded at
/// the wire boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreCommand {
    Reconcile,
    UpdateGaps,
    ReleaseDomain,
    Move {
        window: WindowId,
        direction: Direction,
        cross_output_transfer: bool,
        /// R-MOV-03 same-axis mode: required on the wire at the protocol
        /// boundary; native adapters constructing events directly pass their
        /// default.
        same_axis_move: SameAxisMove,
    },
    Focus {
        window: WindowId,
        direction: Direction,
        cross_output_transfer: bool,
        /// Float-origin cross-output focus (Left/Right miss with no local
        /// float target): the subject is a floating/sticky exception, so the
        /// local tile-edge check is skipped and the existing reciprocal
        /// adjacent remembered-tiled-target fallback applies. False for every
        /// tile-origin request; true only from the adapter float path.
        float_subject: bool,
    },
    Resize {
        window: WindowSubject,
        direction: Direction,
        mode: ResizeSelector,
        press_index: u32,
    },
    PointerResize {
        window: WindowId,
        direction: Direction,
        boundary: i32,
        /// Second-axis corner pair: `Some((direction, boundary))` for an
        /// atomic corner (dual-axis) resize, `None` for an ordinary
        /// single-axis request. Half-present pairs are rejected at the
        /// protocol boundary as `unknown-value`.
        second: Option<(Direction, i32)>,
    },
    ToggleFloat {
        window: WindowSubject,
        float_rect: Option<Rect>,
    },
    ToggleOrientation {
        window: WindowSubject,
    },
    SendToWorkspace {
        window: WindowId,
        target_output: OutputId,
        target_workspace: WorkspaceId,
        /// True follows the mover into the target (default, preserves the
        /// historical follow behavior); false stays on the source domain.
        follow: bool,
    },
    /// Explicit output send (REQ-OUT-04, item 5.3/5.4): the destination
    /// output's current workspace resolved adapter-side, carried as an
    /// explicit cross-output target. Distinct from `SendToWorkspace`, which
    /// stays same-output. Ordinary admission and explicit follow/stay.
    SendToOutput {
        window: WindowId,
        target_output: OutputId,
        target_workspace: WorkspaceId,
        /// True follows the mover into the target (default); false stays on
        /// the source domain.
        follow: bool,
    },
    /// Explicit whole-workspace output migration (R-WS-12): the active
    /// workspace keeps its stable backing id and moves to the adjacent
    /// output resolved adapter-side via full-output-rect adjacency (no wrap).
    /// Follow-only; the adapter shows the migrated workspace on the target
    /// and refills the source natively.
    MigrateWorkspace {
        direction: Direction,
    },
    DragDrop {
        window: WindowId,
        x: i32,
        y: i32,
        /// Optional carried sticky hover prior, applied advisory-style between
        /// begin and drop. `None` preserves legacy behavior; mismatched carries
        /// are ignored, never refused.
        hover_prior: Option<DragHoverPrior>,
        /// Optional validated Started cross-output source binding (source
        /// domain key). `None` preserves same-output behavior; singleton
        /// success requires `Some` with a different output from the request
        /// destination (validated opaque, cross-output only).
        source: Option<DomainKey>,
    },
    DragPreview {
        window: WindowId,
        x: i32,
        y: i32,
        /// Optional carried sticky hover prior, applied advisory-style between
        /// begin and preview.
        hover_prior: Option<DragHoverPrior>,
        /// Optional validated Started cross-output source binding, mirroring
        /// `DragDrop.source`.
        source: Option<DomainKey>,
    },
}

impl CoreCommand {
    /// Wire `op` token for this command.
    #[must_use]
    pub const fn op(&self) -> &'static str {
        match self {
            Self::Reconcile => "reconcile",
            Self::UpdateGaps => "update-gaps",
            Self::ReleaseDomain => "release-domain",
            Self::Move { .. } => "move",
            Self::Focus { .. } => "focus",
            Self::Resize { .. } => "resize",
            Self::PointerResize { .. } => "pointer-resize",
            Self::ToggleFloat { .. } => "toggle-float",
            Self::ToggleOrientation { .. } => "toggle-orientation",
            Self::SendToWorkspace { .. } => "send-to-workspace",
            Self::SendToOutput { .. } => "send-to-output",
            Self::MigrateWorkspace { .. } => "migrate-workspace",
            Self::DragDrop { .. } => "drag-drop",
            Self::DragPreview { .. } => "drag-preview",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_fourteen_ops_have_distinct_wire_tokens() {
        use std::collections::HashSet;
        let commands = vec![
            CoreCommand::Reconcile,
            CoreCommand::UpdateGaps,
            CoreCommand::ReleaseDomain,
            CoreCommand::Move {
                window: WindowId::from("w"),
                direction: Direction::Left,
                cross_output_transfer: true,
                same_axis_move: SameAxisMove::GroupWithNeighbor,
            },
            CoreCommand::Focus {
                window: WindowId::from("w"),
                direction: Direction::Left,
                cross_output_transfer: true,
                float_subject: false,
            },
            CoreCommand::Resize {
                window: WindowSubject::Window(WindowId::from("w")),
                direction: Direction::Left,
                mode: ResizeSelector::Inwards,
                press_index: 0,
            },
            CoreCommand::PointerResize {
                window: WindowId::from("w"),
                direction: Direction::Left,
                boundary: 0,
                second: None,
            },
            CoreCommand::ToggleFloat {
                window: WindowSubject::Window(WindowId::from("w")),
                float_rect: None,
            },
            CoreCommand::ToggleOrientation {
                window: WindowSubject::Window(WindowId::from("w")),
            },
            CoreCommand::SendToWorkspace {
                window: WindowId::from("w"),
                target_output: OutputId::from("o"),
                target_workspace: WorkspaceId::from("s"),
                follow: true,
            },
            CoreCommand::SendToOutput {
                window: WindowId::from("w"),
                target_output: OutputId::from("o"),
                target_workspace: WorkspaceId::from("s"),
                follow: true,
            },
            CoreCommand::MigrateWorkspace {
                direction: Direction::Right,
            },
            CoreCommand::DragDrop {
                window: WindowId::from("w"),
                x: 0,
                y: 0,
                hover_prior: None,
                source: None,
            },
            CoreCommand::DragPreview {
                window: WindowId::from("w"),
                x: 0,
                y: 0,
                hover_prior: None,
                source: None,
            },
        ];
        let ops: HashSet<&str> = commands.iter().map(CoreCommand::op).collect();
        assert_eq!(ops.len(), 14);
    }
}

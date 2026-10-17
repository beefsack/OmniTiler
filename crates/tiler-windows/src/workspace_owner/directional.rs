#![forbid(unsafe_code)]
use super::*;

/// Windows multi-output directional foundation (REQ-OUT-01..03, REQ-MOV-08).
///
/// Portable selection/pair/fence helpers for exhausted directional tiled
/// movement (all four directions) and horizontal directional focus. Native
/// effects (geometry writes, focus actuation, monitor reads) stay in
/// `tiling_sys`; this module owns no HWNDs and no unsafe code.
///
/// Contract:
/// - Adjacency resolves over FULL output rectangles via the shared
///   [`tiler_core::output_selection`] window rule (centre projection, then
///   span overlap, then left/top). Work areas/gaps never gate selection.
/// - Domain bounds use each output's work area inset by the live outer gap;
///   topology uses FULL rectangles. Core rects are never rescaled here.
/// - Source/target current views are the per-output active workspace ids.
///   `(output, workspace)` keys stay distinct even when workspace ids repeat
///   across outputs.
/// - Empty targets support moves (core `CrossOutputTarget::Empty`); empty or
///   unreadable targets refuse focus (core `Unchanged`).
/// - Explicit output-send bindings, non-local modes, whole-workspace
///   migration, and migration source-refill are out of scope and live
///   elsewhere.
///
/// Portable opposite direction for reciprocal adjacency.
#[must_use]
pub const fn directional_opposite(
    direction: tiler_core::directional::Direction,
) -> tiler_core::directional::Direction {
    use tiler_core::directional::Direction;
    match direction {
        Direction::Left => Direction::Right,
        Direction::Right => Direction::Left,
        Direction::Up => Direction::Down,
        Direction::Down => Direction::Up,
    }
}

/// One FULL-rectangle output for directional selection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectionalOutput {
    pub id: String,
    pub full: Rect,
}

/// Select the adjacent output for an exhausted directional move/focus.
///
/// `mover_rect` is the moving/focused window frame in FULL-coordinate space.
/// Returns `Ok(None)` when no candidate edge-touches (caller-side local
/// no-op, no wrap); `Err` on any unreadable topology (fail-closed, never a
/// silent skip). Work areas/gaps never participate: callers pass FULL rects.
pub fn select_directional_target(
    direction: tiler_core::directional::Direction,
    source_output: &str,
    mover_rect: &Rect,
    outputs: &[DirectionalOutput],
) -> Result<Option<String>, tiler_core::output_selection::SelectionError> {
    use tiler_core::directional::OutputId;
    use tiler_core::output_selection::{OutputEntry, select_for_window};
    let mut entries = Vec::with_capacity(outputs.len());
    for output in outputs {
        entries.push(OutputEntry {
            id: OutputId(output.id.clone()),
            rect: output.full,
        });
    }
    let Some(source) = outputs.iter().find(|o| o.id == source_output) else {
        return Err(tiler_core::output_selection::SelectionError::UnreadableTopology);
    };
    let source_entry = OutputEntry {
        id: OutputId(source.id.clone()),
        rect: source.full,
    };
    match select_for_window(direction, &source_entry, mover_rect, &entries)? {
        Some(candidate) => Ok(Some(candidate.id.0)),
        None => Ok(None),
    }
}

/// Attach reciprocal adjacency for one resolved directional pair.
///
/// The source names the target in `direction`; the target names the source
/// back on the opposite side. No other adjacency is carried: core refuses
/// non-reciprocal or ambiguous pairs fail-closed.
pub fn directional_pair_with_adjacency(
    mut source: (OutputDomain, DomainKey),
    mut target: (OutputDomain, DomainKey),
    direction: tiler_core::directional::Direction,
) -> ((OutputDomain, DomainKey), (OutputDomain, DomainKey)) {
    let opposite = directional_opposite(direction);
    source.0.adjacent.insert(direction, target.0.id.clone());
    target.0.adjacent.insert(opposite, source.0.id.clone());
    (source, target)
}

/// Carried pre-plan directional fences: both gaps, both workspace modes, both
/// current views, both FULL rectangles plus both inset domain bounds, owner
/// binding, overlay flags, and the resolved direction/target. Tails re-verify
/// live before any Engine or project mutation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectionalFenceSnapshot {
    pub inner_gap: i32,
    pub outer_gap: i32,
    pub source_output: String,
    pub source_workspace: String,
    pub target_output: String,
    pub target_workspace: String,
    pub source_tiled: bool,
    pub target_tiled: bool,
    pub direction: tiler_core::directional::Direction,
    pub overlay_maximized: bool,
    pub overlay_fullscreen: bool,
    pub source_full: Rect,
    pub target_full: Rect,
    pub source_bounds: Rect,
    pub target_bounds: Rect,
    pub owner: String,
    pub generation: String,
}

/// Live tail-time directional fence readings matched against a snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectionalFenceLive {
    pub inner_gap: i32,
    pub outer_gap: i32,
    pub source_tiled: bool,
    pub target_tiled: bool,
    pub source_is_active: bool,
    pub target_is_active: bool,
    pub topology_ok: bool,
    pub scope_ok: bool,
    pub lifetime_ok: bool,
    pub membership_ok: bool,
    pub overlay_maximized: bool,
    pub overlay_fullscreen: bool,
    pub owner_ok: bool,
}

/// Shared directional fence evaluation: gaps, modes, current views,
/// topology, scope, lifetime, membership, overlay, and owner binding must all
/// hold. Returns the refusal token, or `None` when the tails may proceed.
/// Both move and focus share this gate; focus adds no geometry writes and
/// move adds the Engine cross preconditions on top.
#[must_use]
pub fn directional_fences_hold(
    snapshot: &DirectionalFenceSnapshot,
    live: &DirectionalFenceLive,
) -> Option<&'static str> {
    if !live.owner_ok {
        return Some("owner-changed");
    }
    if snapshot.inner_gap != live.inner_gap || snapshot.outer_gap != live.outer_gap {
        return Some("gap-changed");
    }
    if !live.source_tiled || !live.target_tiled {
        return Some("workspace-floating");
    }
    if snapshot.source_tiled != live.source_tiled || snapshot.target_tiled != live.target_tiled {
        return Some("mode-changed");
    }
    if !live.source_is_active || !live.target_is_active {
        return Some("view-changed");
    }
    if !live.topology_ok {
        return Some("topology-changed");
    }
    if !live.scope_ok {
        return Some("scope-changed");
    }
    if !live.lifetime_ok {
        return Some("identity-changed");
    }
    if !live.membership_ok {
        return Some("origin-vanished");
    }
    if live.overlay_fullscreen != snapshot.overlay_fullscreen {
        return Some("overlay-changed");
    }
    if live.overlay_maximized != snapshot.overlay_maximized {
        return Some("overlay-changed");
    }
    if live.overlay_fullscreen {
        return Some("move-refused-fullscreen");
    }
    None
}

/// Shared explicit output-send fence evaluation: the same pinned snapshot
/// and live readings as [`directional_fences_hold`], except a floating
/// boundary is a membership-only route rather than a refusal. Modes must
/// still equal the snapshot (`mode-changed` on drift); gaps, current views,
/// topology, scope, lifetime, membership, overlay, and owner binding must all
/// hold. Returns the refusal token, or `None` when the tails may proceed.
#[must_use]
pub fn output_send_fences_hold(
    snapshot: &DirectionalFenceSnapshot,
    live: &DirectionalFenceLive,
) -> Option<&'static str> {
    if !live.owner_ok {
        return Some("owner-changed");
    }
    if snapshot.inner_gap != live.inner_gap || snapshot.outer_gap != live.outer_gap {
        return Some("gap-changed");
    }
    if snapshot.source_tiled != live.source_tiled || snapshot.target_tiled != live.target_tiled {
        return Some("mode-changed");
    }
    if !live.source_is_active || !live.target_is_active {
        return Some("view-changed");
    }
    if !live.topology_ok {
        return Some("topology-changed");
    }
    if !live.scope_ok {
        return Some("scope-changed");
    }
    if !live.lifetime_ok {
        return Some("identity-changed");
    }
    if !live.membership_ok {
        return Some("origin-vanished");
    }
    if live.overlay_fullscreen != snapshot.overlay_fullscreen {
        return Some("overlay-changed");
    }
    if live.overlay_maximized != snapshot.overlay_maximized {
        return Some("overlay-changed");
    }
    if live.overlay_fullscreen {
        return Some("move-refused-fullscreen");
    }
    None
}

/// Verify the project-owned cross-output membership transfer after the
/// planned Engine mutation: the mover is absent from the source set and
/// present in the target set. Same rule as workspace send; domain keys stay
/// distinct even when workspace ids repeat across outputs.
#[must_use]
pub fn verify_directional_transfer(
    mover: &WindowKey,
    source_members: &BTreeSet<WindowKey>,
    target_members: &BTreeSet<WindowKey>,
) -> bool {
    verify_send_follow(mover, source_members, target_members)
}

/// Resolve the cross-output focus target without geometry or membership
/// writes: the destination's valid last-focused tile.
///
/// Returns the target when it is still a member of the destination workspace
/// and still visible in the fresh eligible set; otherwise `None` (caller
/// refuses, never fabricates a target). Empty targets always resolve `None`.
#[must_use]
pub fn directional_focus_target(
    spaces: &ManagedWorkspaces,
    target_output: &str,
    target_workspace: &str,
    visible: &BTreeSet<WindowKey>,
) -> Option<WindowKey> {
    spaces.focus_target(target_output, target_workspace, visible)
}

/// Whether a directional focus may cross: horizontal only. Vertical always
/// stays local (callers never build a pair for Up/Down).
#[must_use]
pub const fn directional_focus_crosses(direction: tiler_core::directional::Direction) -> bool {
    matches!(
        direction,
        tiler_core::directional::Direction::Left | tiler_core::directional::Direction::Right
    )
}

/// Resolve the output under a pointer point for pointer-driven cross-output
/// drops (REQ-MOU-03): the first FULL rectangle containing the point, in
/// enumeration order (primary first). FULL rectangles decide (panels never
/// break output identity); work areas govern placement later. Physical
/// coordinates throughout, negative origins included. `None` outside every
/// output: the caller hides/refuses without a drop. This is pointer
/// containment, not the directional adjacency selector and not position-based
/// output selection: differing arrangements all behave alike.
#[must_use]
pub fn output_at_point(outputs: &[DirectionalOutput], x: i32, y: i32) -> Option<String> {
    for output in outputs {
        let r = output.full;
        if r.w > 0
            && r.h > 0
            && x >= r.x
            && x < r.x.saturating_add(r.w)
            && y >= r.y
            && y < r.y.saturating_add(r.h)
        {
            return Some(output.id.clone());
        }
    }
    None
}

/// Carried pre-plan drag-drop fences: both gaps, both workspace modes, both
/// current views, both FULL rectangles plus both inset domain bounds, and the
/// owner binding. Same fence shape as the directional snapshot minus the
/// direction (pointer drops are not adjacency-derived). Tails re-verify live
/// before any Engine or project mutation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DragDropFenceSnapshot {
    pub inner_gap: i32,
    pub outer_gap: i32,
    pub source_output: String,
    pub source_workspace: String,
    pub target_output: String,
    pub target_workspace: String,
    pub source_tiled: bool,
    pub target_tiled: bool,
    pub overlay_maximized: bool,
    pub overlay_fullscreen: bool,
    pub source_full: Rect,
    pub target_full: Rect,
    pub source_bounds: Rect,
    pub target_bounds: Rect,
    pub owner: String,
    pub generation: String,
}

/// View a directional fence snapshot through the direction-free drag-drop
/// fence shape (the direction selects the candidate only; evaluation never
/// reads it). Shared by live-fence reads and tail-time topology checks so
/// the mapping lives in exactly one place.
#[must_use]
pub fn drag_snapshot_for_directional(snapshot: &DirectionalFenceSnapshot) -> DragDropFenceSnapshot {
    DragDropFenceSnapshot {
        inner_gap: snapshot.inner_gap,
        outer_gap: snapshot.outer_gap,
        source_output: snapshot.source_output.clone(),
        source_workspace: snapshot.source_workspace.clone(),
        target_output: snapshot.target_output.clone(),
        target_workspace: snapshot.target_workspace.clone(),
        source_tiled: snapshot.source_tiled,
        target_tiled: snapshot.target_tiled,
        overlay_maximized: snapshot.overlay_maximized,
        overlay_fullscreen: snapshot.overlay_fullscreen,
        source_full: snapshot.source_full,
        target_full: snapshot.target_full,
        source_bounds: snapshot.source_bounds,
        target_bounds: snapshot.target_bounds,
        owner: snapshot.owner.clone(),
        generation: snapshot.generation.clone(),
    }
}

/// Shared drag-drop fence evaluation: gaps, modes, current views, topology,
/// scope, lifetime, membership, overlay, and owner binding must all hold.
/// Returns the refusal token, or `None` when the tails may proceed. Reuses
/// [`DirectionalFenceLive`] (which carries no direction); only the snapshot
/// differs from the directional gate.
#[must_use]
pub fn drag_drop_fences_hold(
    snapshot: &DragDropFenceSnapshot,
    live: &DirectionalFenceLive,
) -> Option<&'static str> {
    if !live.owner_ok {
        return Some("owner-changed");
    }
    if snapshot.inner_gap != live.inner_gap || snapshot.outer_gap != live.outer_gap {
        return Some("gap-changed");
    }
    if !live.source_tiled || !live.target_tiled {
        return Some("workspace-floating");
    }
    if snapshot.source_tiled != live.source_tiled || snapshot.target_tiled != live.target_tiled {
        return Some("mode-changed");
    }
    if !live.source_is_active || !live.target_is_active {
        return Some("view-changed");
    }
    if !live.topology_ok {
        return Some("topology-changed");
    }
    if !live.scope_ok {
        return Some("scope-changed");
    }
    if !live.lifetime_ok {
        return Some("identity-changed");
    }
    if !live.membership_ok {
        return Some("origin-vanished");
    }
    if live.overlay_fullscreen != snapshot.overlay_fullscreen {
        return Some("overlay-changed");
    }
    if live.overlay_maximized != snapshot.overlay_maximized {
        return Some("overlay-changed");
    }
    if live.overlay_fullscreen {
        return Some("move-refused-fullscreen");
    }
    None
}

#![forbid(unsafe_code)]
use super::*;

/// One whole-workspace migration Engine row with live overlay/class flags.
/// Unlike the send builders (which default flags to false and stamp narrow
/// lanes), migration carries actual per-member observations: `maximized`
/// from the live native read, `fullscreen` from the live inference,
/// `sticky` from the runtime sticky set, `floating` from the session
/// exception, and fixed-origin flags from the admission lanes. The caller
/// supplies them; this builder never fabricates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrateRow {
    pub token: String,
    pub rect: Rect,
    pub hints: tiler_core::size_hints::WindowSizeHints,
    pub floating: bool,
    pub fullscreen: bool,
    pub maximized: bool,
    pub sticky: bool,
    pub fixed_auto: bool,
    pub fixed_suppress: bool,
    pub fit_excluded: bool,
}

/// Build the source observation plus empty-target pair for one Engine
/// `MigrateWorkspace` request (R-WS-12, item 14). DISTINCT from both send
/// builders: the target domain carries the SAME stable workspace id on a
/// DIFFERENT output with empty `target_windows` (any carried target window
/// is SAME-workspace residue and the Engine refuses it). The destination's
/// DIFFERENT current workspace stays adapter-owned and is never carried.
/// `focused_token` may be empty (empty/sticky-active migration carries no
/// invented focus). Returns `None` when the workspace ids differ across the
/// pair, the outputs match, or the focused token is dangling (named but not
/// carried - the Engine would refuse it as `FocusMismatch`).
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn build_migrate_event(
    owner: &OwnerId,
    generation: &GenerationId,
    correlation: &CorrelationId,
    revision: u64,
    fingerprint: u64,
    source_domain: (OutputDomain, DomainKey),
    target_domain: (OutputDomain, DomainKey),
    source_rows: &[MigrateRow],
    focused_token: &str,
    direction: &str,
    outer_gap: i32,
) -> Option<CoreEvent> {
    let (domain, domain_key) = source_domain;
    let (target_domain_value, target_key) = target_domain;
    if target_key.workspace != domain_key.workspace || target_key.output == domain_key.output {
        return None;
    }
    if !focused_token.is_empty() && !source_rows.iter().any(|r| r.token == focused_token) {
        return None;
    }
    let windows: Vec<EngineWindow> = source_rows
        .iter()
        .map(|row| EngineWindow {
            window: WindowId(row.token.clone()),
            output: domain_key.output.clone(),
            workspace: domain_key.workspace.clone(),
            rect: row.rect,
            floating: row.floating,
            fit_excluded: row.fit_excluded,
            fullscreen: row.fullscreen,
            maximized: row.maximized,
            sticky: row.sticky,
            fixed_auto: row.fixed_auto,
            fixed_suppress: row.fixed_suppress,
            hints: row.hints,
        })
        .collect();
    Some(CoreEvent {
        owner: owner.clone(),
        generation: generation.clone(),
        correlation: correlation.clone(),
        revision,
        fingerprint,
        domain,
        domain_key,
        outer_gap,
        focused_window: WindowId(focused_token.to_owned()),
        windows,
        directional: None,
        directional_target_outer_gap: None,
        target_domain: Some((target_domain_value, target_key)),
        target_windows: Vec::new(),
        closed_windows: Vec::new(),
        command: CoreCommand::MigrateWorkspace {
            direction: direction.to_owned(),
        },
    })
}

/// Verify the whole-workspace membership transfer plus both current views
/// after the planned Engine mutation and the project reassignment, before
/// publication or follow. This is the single publication gate: callers pass
/// the effective refill (the primitive's selected refill, or the lifecycle-
/// converged source current when the primitive reported no refill), never an
/// ad hoc fallback around it.
///
/// - Every migrating (non-sticky) member is absent from the source workspace
///   set and present in the target workspace set.
/// - Every sticky member stays out of the target set (it remains homed on
///   the source output's refill view, never the migrated id).
/// - The target current view shows the migrated id; the source current view
///   shows the effective refill id.
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn verify_migration_transfer(
    migrating: &BTreeSet<WindowKey>,
    sticky: &BTreeSet<WindowKey>,
    source_members: &BTreeSet<WindowKey>,
    target_members: &BTreeSet<WindowKey>,
    moved_id: &str,
    refill_id: Option<&str>,
    source_active: Option<&str>,
    target_active: Option<&str>,
) -> bool {
    if target_active != Some(moved_id) {
        return false;
    }
    match refill_id {
        Some(refill) => {
            if source_active != Some(refill) {
                return false;
            }
        }
        None => {
            if source_active.is_some() {
                return false;
            }
        }
    }
    for member in migrating {
        if source_members.contains(member) || !target_members.contains(member) {
            return false;
        }
    }
    for member in sticky {
        if target_members.contains(member) {
            return false;
        }
    }
    let _ = moved_id;
    true
}

/// Translate one frame rectangle across outputs by the FULL-rectangle origin
/// delta, preserving size: the native output translation for migrating float,
/// fullscreen, and overlay members (move-only, never a resize or refit).
/// Saturating so extreme coordinates never wrap. Pure: the caller owns live
/// currency (the pinned pair must still touch when this is used).
#[must_use]
pub fn translate_rect_to_output(frame: Rect, source_full: Rect, target_full: Rect) -> Rect {
    Rect {
        x: frame
            .x
            .saturating_sub(source_full.x)
            .saturating_add(target_full.x),
        y: frame
            .y
            .saturating_sub(source_full.y)
            .saturating_add(target_full.y),
        w: frame.w,
        h: frame.h,
    }
}

/// View a migration fence snapshot through the direction-free drag-drop fence
/// shape (the direction selects the candidate only; evaluation never reads
/// it). Lets migration tails reuse the shared exact FULL plus inset-bounds
/// topology check instead of a second implementation.
#[must_use]
pub fn migrate_snapshot_for_drag(snapshot: &MigrateFenceSnapshot) -> DragDropFenceSnapshot {
    DragDropFenceSnapshot {
        inner_gap: snapshot.inner_gap,
        outer_gap: snapshot.outer_gap,
        source_output: snapshot.source_output.clone(),
        source_workspace: snapshot.moved_workspace.clone(),
        target_output: snapshot.target_output.clone(),
        target_workspace: snapshot.target_prior.clone(),
        source_tiled: snapshot.source_tiled,
        target_tiled: snapshot.target_tiled,
        overlay_maximized: false,
        overlay_fullscreen: false,
        source_full: snapshot.source_full,
        target_full: snapshot.target_full,
        source_bounds: snapshot.source_bounds,
        target_bounds: snapshot.target_bounds,
        owner: snapshot.owner.clone(),
        generation: snapshot.generation.clone(),
    }
}

/// Select the adjacent output for one whole-workspace migration through the
/// shared largest-shared-edge rule (never window position). Returns
/// `Ok(None)` when no candidate edge-touches (caller-side no-op, no wrap);
/// `Err` on any unreadable topology, duplicate ids, source rect mismatch, or
/// unknown source (fail-closed, never a silent skip). Work areas and gaps
/// never participate: callers pass FULL rectangles.
pub fn select_migration_target(
    direction: tiler_core::directional::Direction,
    source_output: &str,
    outputs: &[DirectionalOutput],
) -> Result<Option<String>, tiler_core::output_selection::SelectionError> {
    use tiler_core::directional::OutputId;
    use tiler_core::output_selection::{OutputEntry, select_for_migration};
    let Some(source) = outputs.iter().find(|o| o.id == source_output) else {
        return Err(tiler_core::output_selection::SelectionError::UnreadableTopology);
    };
    let source_entry = OutputEntry {
        id: OutputId(source.id.clone()),
        rect: source.full,
    };
    let mut entries = Vec::with_capacity(outputs.len());
    for output in outputs {
        entries.push(OutputEntry {
            id: OutputId(output.id.clone()),
            rect: output.full,
        });
    }
    match select_for_migration(direction, &source_entry, &entries)? {
        Some(candidate) => Ok(Some(candidate.id.0)),
        None => Ok(None),
    }
}

/// Carried pre-plan migration fences: pinned source/target outputs, the moved
/// workspace id, both current views (target current before the move, source
/// refill after), gaps, modes, owner binding, and the resolved direction.
/// Tails re-verify live before any Engine or project mutation and again
/// before publication.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrateFenceSnapshot {
    pub inner_gap: i32,
    pub outer_gap: i32,
    pub source_output: String,
    pub target_output: String,
    pub moved_workspace: String,
    pub source_refill: Option<String>,
    pub target_prior: String,
    pub source_tiled: bool,
    pub target_tiled: bool,
    pub direction: tiler_core::directional::Direction,
    pub source_full: Rect,
    pub target_full: Rect,
    pub source_bounds: Rect,
    pub target_bounds: Rect,
    pub owner: String,
    pub generation: String,
    pub member_count: usize,
}

/// Live tail-time migration fence readings matched against a snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrateFenceLive {
    pub inner_gap: i32,
    pub outer_gap: i32,
    pub source_active: Option<String>,
    pub target_active: Option<String>,
    pub source_tiled: bool,
    pub target_tiled: bool,
    pub topology_ok: bool,
    pub scope_ok: bool,
    pub lifetime_ok: bool,
    pub membership_ok: bool,
    pub views_ok: bool,
    pub owner_ok: bool,
}

/// Re-verify carried pre-plan migration fences against live tail-time
/// readings. `None` holds; `Some(outcome)` refuses with no further effects
/// and no replay. Membership semantics differ before and after the project
/// reassignment (the caller passes the matching value); views are checked
/// pre-commit against the pinned prior views and post-commit through
/// [`verify_migration_transfer`]. Never fabricates state, only compares.
#[must_use]
pub const fn migrate_fences_hold(
    snap: &MigrateFenceSnapshot,
    live: &MigrateFenceLive,
) -> Option<&'static str> {
    if !live.owner_ok {
        return Some("owner-changed");
    }
    if !live.topology_ok {
        return Some("topology-changed");
    }
    if live.inner_gap != snap.inner_gap || live.outer_gap != snap.outer_gap {
        return Some("deferred");
    }
    if live.source_tiled != snap.source_tiled || live.target_tiled != snap.target_tiled {
        return Some("mode-changed");
    }
    if !live.scope_ok {
        return Some("scope-excluded");
    }
    if !live.lifetime_ok {
        return Some("identity-changed");
    }
    if !live.membership_ok {
        return Some("unverified");
    }
    if !live.views_ok {
        return Some("view-changed");
    }
    None
}

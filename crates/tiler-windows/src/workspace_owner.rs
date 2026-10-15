#![forbid(unsafe_code)]
//! Portable workspace-owner policy: output context, select/send resolution,
//! Engine observation assembly, and follow verification.
//!
//! Native effects (hide/reveal/focus/monitor identity) live in
//! `tiling_sys` (`cfg(windows)` only). This module owns no HWNDs, no window
//! properties, and no logs: pure resolution plus `CoreEvent` construction for
//! the retained `Engine` `SendToWorkspace`/`SendToOutput` routes.

use std::collections::{BTreeMap, BTreeSet};

use tiler_core::boundary::{CoreCommand, CoreEvent};
use tiler_core::directional::{OutputId, WindowId, WorkspaceId};
use tiler_core::geometry::Rect;
use tiler_core::ids::{CorrelationId, GenerationId, OwnerId};
use tiler_core::seed::EngineWindow;
use tiler_core::session::{DomainKey, OutputDomain};

use crate::snapkey::SnapOp;
use crate::workspace::{ManagedWorkspaces, WindowKey, verify_send_follow};

/// Resolve which output a digit chord acts on: the cached active output when
/// it still exists, else the foreground monitor, else the pointer monitor,
/// else output ordering. Never invents a key.
#[must_use]
pub fn output_context(
    spaces: &ManagedWorkspaces,
    cached: Option<&str>,
    foreground: Option<&str>,
    pointer: Option<&str>,
) -> Option<String> {
    if let Some(key) = cached
        && spaces.output_keys().iter().any(|k| k == key)
    {
        return Some(key.to_owned());
    }
    if let Some(key) = foreground
        && spaces.output_keys().iter().any(|k| k == key)
    {
        return Some(key.to_owned());
    }
    if let Some(key) = pointer
        && spaces.output_keys().iter().any(|k| k == key)
    {
        return Some(key.to_owned());
    }
    spaces.output_keys().into_iter().next()
}

/// One Engine window row: token, last-known rectangle, fresh
/// application-declared minimum-size hint, and the slotless floating
/// exception flag (intentional float or born-held fullscreen: the window
/// rides Engine membership with no tile slot so siblings keep the tile area).
/// Hidden snapshots ride the same rows so convergence never drops retained
/// membership; hidden rows carry a fresh hint like visible rows (fresh
/// observation only, never a stored floor) while retained rows carry none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnerRow {
    pub token: String,
    pub rect: Rect,
    pub hints: tiler_core::size_hints::WindowSizeHints,
    pub floating: bool,
}

/// Build the source/target domain pair plus window rows for one Engine
/// `SendToWorkspace` request. Rows already include hidden snapshots for both
/// domains; the caller supplies them. `follow` pins the explicit send intent
/// (true follows the mover into the target, false stays on the source with
/// the focused-removal MRU focus). Returns `None` when either workspace id
/// is unknown on the output.
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn build_send_event(
    owner: &OwnerId,
    generation: &GenerationId,
    correlation: &CorrelationId,
    revision: u64,
    fingerprint: u64,
    source_domain: (OutputDomain, DomainKey),
    target_domain: (OutputDomain, DomainKey),
    source_rows: &[OwnerRow],
    target_rows: &[OwnerRow],
    mover_token: &str,
    outer_gap: i32,
    follow: bool,
) -> Option<CoreEvent> {
    build_send_event_inner(
        owner,
        generation,
        correlation,
        revision,
        fingerprint,
        source_domain,
        target_domain,
        source_rows,
        target_rows,
        mover_token,
        outer_gap,
        follow,
        false,
    )
}

/// Build the source/target domain pair plus window rows for one Engine
/// `SendToOutput` request (item 5 explicit output send). DISTINCT from
/// [`build_send_event`]: the destination is the target output's current
/// workspace resolved adapter-side (cross-output, same workspace ids valid),
/// carried as an explicit cross-output target with ordinary admission and the
/// explicit follow/stay flag. Same row/observation contract as the workspace
/// route.
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn build_send_output_event(
    owner: &OwnerId,
    generation: &GenerationId,
    correlation: &CorrelationId,
    revision: u64,
    fingerprint: u64,
    source_domain: (OutputDomain, DomainKey),
    target_domain: (OutputDomain, DomainKey),
    source_rows: &[OwnerRow],
    target_rows: &[OwnerRow],
    mover_token: &str,
    outer_gap: i32,
    follow: bool,
) -> Option<CoreEvent> {
    build_send_event_inner(
        owner,
        generation,
        correlation,
        revision,
        fingerprint,
        source_domain,
        target_domain,
        source_rows,
        target_rows,
        mover_token,
        outer_gap,
        follow,
        true,
    )
}

/// Shared observation assembly for both send routes; only the command
/// differs (`output_send` selects `SendToOutput`, else `SendToWorkspace`).
#[allow(clippy::too_many_arguments)]
fn build_send_event_inner(
    owner: &OwnerId,
    generation: &GenerationId,
    correlation: &CorrelationId,
    revision: u64,
    fingerprint: u64,
    source_domain: (OutputDomain, DomainKey),
    target_domain: (OutputDomain, DomainKey),
    source_rows: &[OwnerRow],
    target_rows: &[OwnerRow],
    mover_token: &str,
    outer_gap: i32,
    follow: bool,
    output_send: bool,
) -> Option<CoreEvent> {
    if mover_token.is_empty() {
        return None;
    }
    let (domain, domain_key) = source_domain;
    let (target_domain_value, target_key) = target_domain;
    let windows: Vec<EngineWindow> = source_rows
        .iter()
        .map(|row| EngineWindow {
            window: WindowId(row.token.clone()),
            output: domain_key.output.clone(),
            workspace: domain_key.workspace.clone(),
            rect: row.rect,
            floating: row.floating,
            fit_excluded: false,
            fullscreen: false,
            maximized: false,
            sticky: false,
            fixed_auto: false,
            fixed_suppress: false,
            hints: row.hints,
        })
        .collect();
    let target_windows: Vec<EngineWindow> = target_rows
        .iter()
        .map(|row| EngineWindow {
            window: WindowId(row.token.clone()),
            output: target_key.output.clone(),
            workspace: target_key.workspace.clone(),
            rect: row.rect,
            floating: row.floating,
            fit_excluded: false,
            fullscreen: false,
            maximized: false,
            sticky: false,
            fixed_auto: false,
            fixed_suppress: false,
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
        focused_window: WindowId(mover_token.to_owned()),
        windows,
        directional: None,
        directional_target_outer_gap: None,
        target_domain: Some((target_domain_value, target_key)),
        target_windows,
        closed_windows: Vec::new(),
        command: if output_send {
            CoreCommand::SendToOutput {
                window: mover_token.to_owned(),
                target_output: String::new(),
                target_workspace: String::new(),
                follow,
            }
        } else {
            CoreCommand::SendToWorkspace {
                window: mover_token.to_owned(),
                target_output: String::new(),
                target_workspace: String::new(),
                follow,
            }
        },
    })
}

/// Fill the command target from the resolved domain key. Split from
/// [`build_send_event`] so tests assert the target binding explicitly.
pub fn stamp_send_target(event: &mut CoreEvent, target: &DomainKey) {
    if let CoreCommand::SendToWorkspace {
        target_output,
        target_workspace,
        ..
    } = &mut event.command
    {
        *target_output = target.output.0.clone();
        *target_workspace = target.workspace.0.clone();
    }
}

/// Fill the output-send command target from the resolved domain key. Split
/// from [`build_send_output_event`] like [`stamp_send_target`]; a workspace
/// stamp never touches an output command and vice versa.
pub fn stamp_send_output_target(event: &mut CoreEvent, target: &DomainKey) {
    if let CoreCommand::SendToOutput {
        target_output,
        target_workspace,
        ..
    } = &mut event.command
    {
        *target_output = target.output.0.clone();
        *target_workspace = target.workspace.0.clone();
    }
}

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

/// Send-specific overlay binding for a floating focused mover (D16 combo
/// carry, 2026-10-13): when the mover rides floating, carry its exact live
/// maximized flag on its own Engine rows so the shared deferred arm binds
/// the approved floating+maximized shape. Fullscreen already rides the
/// existing fixed-admission stamp for every row; tiled maximized movers,
/// sticky movers, and every survivor keep the builder defaults (false), so
/// tiled/survivor/admission encoding and protocol semantics are unchanged.
/// Pure: no reads, no writes, no topology effects.
pub fn stamp_send_float_overlay(
    event: &mut CoreEvent,
    mover_token: &str,
    floating: bool,
    maximized: bool,
) {
    if mover_token.is_empty() || !floating || !maximized {
        return;
    }
    for entry in event
        .windows
        .iter_mut()
        .chain(event.target_windows.iter_mut())
    {
        if entry.window.0 == mover_token {
            entry.maximized = true;
        }
    }
}

/// Verify the project-owned membership transfer after the planned Engine
/// mutation: the mover is absent from the source set and present in the
/// target set. No-op and foreign transfers never follow.
#[must_use]
pub fn verify_membership_transfer(
    mover: &WindowKey,
    source_members: &BTreeSet<WindowKey>,
    target_members: &BTreeSet<WindowKey>,
) -> bool {
    verify_send_follow(mover, source_members, target_members)
}

/// Planned send route across one workspace boundary. Tiled-to-tiled sends
/// run the shared two-domain Engine plan; any boundary touching a floating
/// workspace transfers project native membership instead (no two-domain
/// plan, no floating-side geometry) and reflows only the tiled side.
/// Eligibility derives from the actual source workspace mode, never from the
/// mover's per-window float flag alone: a mover leaving a floating source is
/// a native-boundary send even when it carries a slotless float exception.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendRoute {
    Engine,
    Native,
}

#[must_use]
pub const fn send_route(source_tiled: bool, target_tiled: bool) -> SendRoute {
    if source_tiled && target_tiled {
        SendRoute::Engine
    } else {
        SendRoute::Native
    }
}

/// Pre-plan fence snapshot carried into the send tails (item 2 + item 20):
/// both gaps, both workspace modes, and the live overlay flags resolved at
/// snapshot time (live OS read with retained-row fallback, never fabricated).
/// Tails re-verify against live readings before source writes AND hide/focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SendFenceSnapshot {
    pub inner_gap: i32,
    pub outer_gap: i32,
    pub source_tiled: bool,
    pub target_tiled: bool,
    pub overlay_maximized: bool,
    pub overlay_fullscreen: bool,
}

/// Live tail-time fence readings matched against a [`SendFenceSnapshot`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SendFenceLive {
    pub inner_gap: i32,
    pub outer_gap: i32,
    pub source_tiled: bool,
    pub target_tiled: bool,
    pub active_is_source: bool,
    pub target_is_active: bool,
    pub scope_ok: bool,
    pub lifetime_ok: bool,
    pub membership_ok: bool,
    pub overlay_maximized: bool,
    pub overlay_fullscreen: bool,
}

/// Re-verify carried pre-plan fences against live tail-time readings.
/// `None` holds; `Some(outcome)` refuses with no further effects and no
/// replay. Ordinary movers ride `(false, false)` overlay flags through both
/// sides and pass trivially; a stable fullscreen snapshot carries exactly
/// like maximized (item 9); a newly arrived live fullscreen refuses outright
/// (never sends) and any other overlay drift defers with no restore.
/// Production tails MUST route through this gate (tested decision table
/// below); it never fabricates state, only compares.
#[must_use]
pub const fn send_fences_hold(
    snap: SendFenceSnapshot,
    live: SendFenceLive,
) -> Option<&'static str> {
    if !live.active_is_source || live.target_is_active {
        return Some("view-changed");
    }
    if live.source_tiled != snap.source_tiled || live.target_tiled != snap.target_tiled {
        return Some("mode-changed");
    }
    if live.inner_gap != snap.inner_gap || live.outer_gap != snap.outer_gap {
        return Some("deferred");
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
    if live.overlay_fullscreen && live.overlay_fullscreen != snap.overlay_fullscreen {
        return Some("send-refused-fullscreen");
    }
    if live.overlay_maximized != snap.overlay_maximized
        || live.overlay_fullscreen != snap.overlay_fullscreen
    {
        return Some("deferred");
    }
    None
}

/// Item 9 fullscreen-carry overlay-flag gate for a send (portable decision;
/// the caller supplies live OS reads, never fabricated): a stable fullscreen
/// snapshot carries exactly like maximized; a newly arrived live fullscreen
/// refuses outright; any other maximized/fullscreen drift between the
/// pre-plan snapshot and the live recheck defers with no writes and no
/// restore. A fullscreen snapshot with an unreadable live flag defers as
/// well (pre-effect: never trust the retained fallback before effects; the
/// post-transfer hidden tails keep the bounded legacy fallback, unchanged).
/// Production pre-plan, post-plan, and pre-effect checks MUST route through
/// this gate (tested decision table below). Maximized/fullscreen carry
/// itself never restores.
pub const fn send_overlay_gate(
    pre_maximized: bool,
    pre_fullscreen: bool,
    live_maximized: bool,
    live_fullscreen: Option<bool>,
    fallback_fullscreen: bool,
) -> Result<(), &'static str> {
    if pre_fullscreen && live_fullscreen.is_none() {
        return Err("deferred");
    }
    let live_fullscreen = match live_fullscreen {
        Some(flag) => flag,
        None => fallback_fullscreen,
    };
    if live_fullscreen && live_fullscreen != pre_fullscreen {
        return Err("send-refused-fullscreen");
    }
    if live_maximized != pre_maximized || live_fullscreen != pre_fullscreen {
        return Err("deferred");
    }
    Ok(())
}

/// Item 9 retained-overlay admission for a send: a retained maximized or
/// fullscreen member resolves through its retained row with a live flag
/// recheck (never as an ordinary mover); any other retained row never sends
/// as an overlay mover. Slotless born-held windows never reach this (no
/// member token means `unmanaged` first). Production send dispatch MUST
/// route retained movers through this (tested table below).
#[must_use]
pub const fn send_retained_overlay_admits(row_maximized: bool, row_fullscreen: bool) -> bool {
    row_maximized || row_fullscreen
}

/// Stay-focus resolution class (item 2): `Null` is a genuine null plan (sole
/// mover emptied the source: ok/no-focus, no setter); `Stale` is a
/// some-but-unusable plan mapping (unmapped leaf, left source, hidden,
/// lifetime or eligibility failure: focus-unverified, no setter); `Ready`
/// attempts the single actuation. Production MUST route the gathered Engine
/// snapshot plus exact source/member/lifetime state through
/// [`classify_stay_focus`] (tested decision table below).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StayFocusClass {
    Null,
    Stale,
    Ready,
}

/// Classify one stay-focus resolution from gathered inputs: `plan_null` is a
/// genuine null plan; otherwise `mapped` (leaf resolves to a window with a
/// member key), `in_source`, `hidden`, `lifetime_ok` (exact member tag), and
/// `eligible` (fresh focus set) must all hold for `Ready`.
#[must_use]
pub const fn classify_stay_focus(
    plan_null: bool,
    mapped: bool,
    in_source: bool,
    hidden: bool,
    lifetime_ok: bool,
    eligible: bool,
) -> StayFocusClass {
    if plan_null {
        return StayFocusClass::Null;
    }
    if mapped && in_source && !hidden && lifetime_ok && eligible {
        StayFocusClass::Ready
    } else {
        StayFocusClass::Stale
    }
}

/// Directional refusal on a floating workspace (KDE plan-adapter parity:
/// `focus-refused-workspace-floating` plus `move-refused-workspace-floating`).
/// Both focus and move refuse: a floating workspace runs no tile layout, so
/// directional navigation has no topology to move through or focus within.
/// `None` when the workspace is tiled.
#[must_use]
pub const fn directional_workspace_refusal(op: SnapOp, tiled: bool) -> Option<&'static str> {
    if tiled {
        return None;
    }
    Some(match op {
        SnapOp::Focus => "focus-refused-workspace-floating",
        SnapOp::Move => "move-refused-workspace-floating",
    })
}

/// Engine tokens that must ride floating through the runtime intent even when
/// the Engine session carries no exception: a session freshly released by a
/// workspace-mode toggle, or a boundary-send adoption the target session has
/// never seen. Keyed by exact window lifetime, so a recycled HWND never
/// inherits and dead lifetimes prune with membership. Sticky and
/// born-fullscreen members keep their own lanes and never enter here.
#[must_use]
pub fn float_carry_tokens(
    members: &BTreeSet<WindowKey>,
    token_of: &BTreeMap<WindowKey, String>,
    intent: &BTreeSet<WindowKey>,
) -> BTreeSet<String> {
    members
        .iter()
        .filter(|key| intent.contains(*key))
        .filter_map(|key| token_of.get(key))
        .cloned()
        .collect()
}

/// One on-window marker slot read (item 8, R-RST-01/R-FLT-05). `Value`
/// carries the live `GetProp` value, always nonzero (native NULL/zero reads
/// as [`MarkerSlot::Absent`); a `Value(0)` only arises in unit tests and
/// fails closed as corrupt. `Unreadable` is the fail-closed classifier input
/// for a genuinely failed read: diagnosed, no intent. No current native
/// reader produces it (`GetPropW` NULL reads absent); it stays so the
/// classifier never mistakes a future read failure for intent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkerSlot {
    Absent,
    Value(u64),
    Unreadable,
}

/// Restart marker classification for one window's slots. Priority is sticky,
/// then intentional float, then the reserved tile override; a failed read
/// with no valid higher lane is [`RestartMarker::Unreadable`] (diagnosed, no
/// intent), and a present-but-wrong value is [`RestartMarker::Corrupt`]
/// (logged, no intent). Missing means empty. Markers restore classification
/// only: hydration still needs admission plus identity and lifetime fences.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestartMarker {
    Absent,
    FloatIntent,
    Sticky(bool),
    TileOverride,
    Corrupt,
    Unreadable,
}

/// Classify one window's restart marker slots with sticky-first priority.
#[must_use]
pub fn classify_restart_marker(
    float: MarkerSlot,
    sticky: MarkerSlot,
    tile: MarkerSlot,
) -> RestartMarker {
    if let MarkerSlot::Value(raw) = sticky
        && let Some(prior) = crate::tiling::parse_sticky_marker(raw)
    {
        return RestartMarker::Sticky(prior);
    }
    if let MarkerSlot::Value(raw) = float
        && crate::tiling::parse_float_intent_marker(raw).is_some()
    {
        return RestartMarker::FloatIntent;
    }
    if let MarkerSlot::Value(raw) = tile
        && crate::tiling::parse_tile_override_marker(raw).is_some()
    {
        return RestartMarker::TileOverride;
    }
    if matches!(float, MarkerSlot::Unreadable)
        || matches!(sticky, MarkerSlot::Unreadable)
        || matches!(tile, MarkerSlot::Unreadable)
    {
        return RestartMarker::Unreadable;
    }
    if matches!(float, MarkerSlot::Value(_))
        || matches!(sticky, MarkerSlot::Value(_))
        || matches!(tile, MarkerSlot::Value(_))
    {
        RestartMarker::Corrupt
    } else {
        RestartMarker::Absent
    }
}

/// Whether one classified marker re-adopts an ordinary intentional float on
/// owner restart: admitted membership plus exact identity plus live lifetime
/// tag, with a valid float marker. Every other lane (sticky, reserved tile
/// override, corrupt, unreadable, absent) never floats here.
#[must_use]
pub const fn restart_float_eligible(
    admitted: bool,
    identity_ok: bool,
    lifetime_ok: bool,
    marker: RestartMarker,
) -> bool {
    admitted && identity_ok && lifetime_ok && matches!(marker, RestartMarker::FloatIntent)
}

/// Which pre-sticky state one classified marker re-adopts on owner restart:
/// `Some(prior)` under the same admission/identity/lifetime fences as
/// [`restart_float_eligible`], `None` otherwise. Both origins re-adopt as
/// sticky before any tiling (never as a normal float); the exact
/// post-restart un-stick result stays TBD under R-FLT-05/R-RST-01.
#[must_use]
pub const fn restart_sticky_eligible(
    admitted: bool,
    identity_ok: bool,
    lifetime_ok: bool,
    marker: RestartMarker,
) -> Option<bool> {
    if !(admitted && identity_ok && lifetime_ok) {
        return None;
    }
    match marker {
        RestartMarker::Sticky(prior) => Some(prior),
        _ => None,
    }
}

/// Whether one classified marker re-adopts an explicit fixed-window tile
/// override on owner restart (R-SPC-04 D7): admitted membership plus exact
/// identity plus live lifetime tag, with a valid tile-override marker. The
/// override restores classification only (the stamped suppress signal on
/// subsequent observations); hydration performs no writes, stages no Engine
/// commit, and installs no marker. Missing/degraded/unmatched records fall
/// back to ordinary fixed classification (recompute, no hold/retry). Every
/// other lane (sticky, float, corrupt, unreadable, absent) never overrides
/// here.
#[must_use]
pub const fn restart_tile_override_eligible(
    admitted: bool,
    identity_ok: bool,
    lifetime_ok: bool,
    marker: RestartMarker,
) -> bool {
    admitted && identity_ok && lifetime_ok && matches!(marker, RestartMarker::TileOverride)
}

/// Focus-before-geometry gate for a verified workspace transition: establish
/// the appropriate target focus before geometry only when the transition
/// verified and no fullscreen/elevated foreground arrived. A real fullscreen
/// or elevated arrival must never be stolen from just to defeat the geometry
/// veto; skipping focus there lets the write path veto honestly.
#[must_use]
pub const fn focus_before_geometry(
    transition_verified: bool,
    fullscreen_foreground: bool,
    elevated_foreground: bool,
) -> bool {
    transition_verified && !fullscreen_foreground && !elevated_foreground
}

/// Privacy-safe foreground veto reason. Bounded vocabulary only: no titles,
/// paths, HWNDs, PIDs, or content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForegroundVetoReason {
    /// No covering window (null foreground, captioned, or small borderless).
    None,
    /// Captioned foreground: never borderless fullscreen by construction.
    Captioned,
    /// Valid foreground handle that is not visible: invisible windows cannot
    /// be covering.
    Nonvisible,
    /// Exact desktop shell handle (`GetShellWindow`/`GetDesktopWindow`): the
    /// desktop, not a fullscreen application.
    Desktop,
    /// Visible captionless foreground whose frame covers a monitor.
    Fullscreen,
    /// Valid visible foreground that the compositor reports cloaked
    /// (`DWMWA_CLOAKED`): invisible to the compositor, so never a covering
    /// fullscreen even when its frame spans a monitor.
    Cloaked,
    /// Captionless monitor-covering foreground verified as a managed member
    /// or a born-held tracked window: it rides a retained overlay slot with
    /// no geometry writes, so it never suspends the workspace. Unverified
    /// fullscreen still reports `Fullscreen` and suspends.
    ManagedOverlay,
    /// Visible foreground whose frame could not be read: fail closed.
    Unreadable,
    /// Non-null foreground handle that fails validity: fail closed.
    Invalid,
}

impl ForegroundVetoReason {
    /// Stable log token for the bounded action summary.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Captioned => "captioned",
            Self::Nonvisible => "nonvisible",
            Self::Desktop => "desktop",
            Self::Fullscreen => "fullscreen",
            Self::Cloaked => "cloaked",
            Self::ManagedOverlay => "managed-overlay",
            Self::Unreadable => "unreadable",
            Self::Invalid => "invalid",
        }
    }
}

/// Portable foreground facts for the veto classifier. The native caller owns
/// all reads; this struct carries only booleans plus the already-computed
/// covering predicate, never handles, paths, or titles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ForegroundFacts {
    /// Foreground handle passed native validity (`IsWindow`, non-null).
    pub valid: bool,
    /// Exact desktop shell identity (`GetShellWindow`/`GetDesktopWindow`).
    pub is_desktop: bool,
    /// Fresh `IsWindowVisible` read on the valid handle.
    pub visible: bool,
    /// Caption bit (`WS_CAPTION`) from the live style.
    pub captioned: bool,
    /// DWM extended-frame-bounds read succeeded.
    pub dwm_readable: bool,
    /// DWM cloak-attribute (`DWMWA_CLOAKED`) read succeeded. Failure fails
    /// closed like an unreadable frame: an unknown cloak state never clears
    /// a covering frame.
    pub cloak_readable: bool,
    /// DWM reports the foreground cloaked (invisible to the compositor).
    pub cloaked: bool,
    /// Captionless frame covers a monitor full rect (portable predicate).
    pub covers_monitor: bool,
}

/// Veto decision plus reason. `block == true` preserves the existing gate:
/// visible real fullscreen, visible unreadable, and invalid foregrounds veto;
/// everything else (including valid non-visible and exact desktop) does not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ForegroundVeto {
    pub block: bool,
    pub reason: ForegroundVetoReason,
}

/// Portable fact-based foreground veto policy, used by the actual native path.
/// Order matters: validity first (fail closed), then exact desktop identity
/// (classifier correctness, not an intent exception), then fresh visibility
/// (invisible cannot cover), then readability (visible unknown stays blocked),
/// then cloak (cloaked is invisible to the compositor, unreadable cloak
/// stays blocked), then caption, then covering geometry.
#[must_use]
pub const fn classify_foreground(facts: ForegroundFacts) -> ForegroundVeto {
    if !facts.valid {
        return ForegroundVeto {
            block: true,
            reason: ForegroundVetoReason::Invalid,
        };
    }
    if facts.is_desktop {
        return ForegroundVeto {
            block: false,
            reason: ForegroundVetoReason::Desktop,
        };
    }
    if !facts.visible {
        return ForegroundVeto {
            block: false,
            reason: ForegroundVetoReason::Nonvisible,
        };
    }
    if !facts.dwm_readable {
        return ForegroundVeto {
            block: true,
            reason: ForegroundVetoReason::Unreadable,
        };
    }
    if !facts.cloak_readable {
        return ForegroundVeto {
            block: true,
            reason: ForegroundVetoReason::Unreadable,
        };
    }
    if facts.cloaked {
        return ForegroundVeto {
            block: false,
            reason: ForegroundVetoReason::Cloaked,
        };
    }
    if facts.captioned {
        return ForegroundVeto {
            block: false,
            reason: ForegroundVetoReason::Captioned,
        };
    }
    if facts.covers_monitor {
        return ForegroundVeto {
            block: true,
            reason: ForegroundVetoReason::Fullscreen,
        };
    }
    ForegroundVeto {
        block: false,
        reason: ForegroundVetoReason::None,
    }
}

/// One planned geometry write extracted from an Engine reply: the portable
/// shape the native write path consumes. Workspace identity stays with the
/// caller; scoping rides the domain writable set, never a workspace-id
/// partition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedWrite {
    pub window: WindowId,
    pub rect: Rect,
    pub overconstrained: bool,
    pub client_clamped: bool,
}

/// Portable Engine-reply extraction used by the actual native write path.
/// Returns `None` for non-plan replies so callers serialize null with reason
/// instead of zeros. Both-domain `SendWorkspace`/`SendOutput` plans carry
/// source reflow and target placement together; the domain writable set
/// scopes each pass.
#[must_use]
pub fn planned_writes(reply: &tiler_core::boundary::CoreReply) -> Option<Vec<PlannedWrite>> {
    fn map(geometry: &[tiler_core::session::DesiredGeometry]) -> Vec<PlannedWrite> {
        geometry
            .iter()
            .map(|g| PlannedWrite {
                window: g.window.clone(),
                rect: g.rect,
                overconstrained: g.overconstrained,
                client_clamped: g.client_clamped,
            })
            .collect()
    }
    use tiler_core::boundary::CoreReply as R;
    match reply {
        R::Projection(plan) => Some(map(&plan.geometry)),
        R::Tiled(plan) => Some(map(&plan.geometry)),
        R::Resize(plan) => Some(map(&plan.geometry)),
        R::MoveDirectional(plan) => Some(map(&plan.geometry)),
        R::FocusDirectional(plan) => Some(map(&plan.geometry)),
        R::SendWorkspace(plan) => Some(map(&plan.geometry)),
        R::SendOutput(plan) => Some(map(&plan.geometry)),
        R::MigrateWorkspace(plan) => Some(map(&plan.geometry)),
        _ => None,
    }
}

/// Portable writable-token subset: non-hidden members whose Engine token is
/// present in the fresh eligible observation. Retained members without a
/// fresh frame and hidden members never take geometry writes; hidden
/// workspace geometry waits for reveal. Scope and identity revalidation stay
/// with the native caller; this pins the hidden/retained exclusion.
#[must_use]
pub fn writable_subset(
    members: &BTreeSet<WindowKey>,
    is_hidden: impl Fn(&WindowKey) -> bool,
    token_of: &BTreeMap<WindowKey, String>,
    fresh_tokens: &std::collections::HashSet<String>,
) -> std::collections::HashSet<String> {
    members
        .iter()
        .filter(|k| !is_hidden(k))
        .filter_map(|k| token_of.get(k))
        .filter(|t| fresh_tokens.contains(*t))
        .cloned()
        .collect()
}

/// Hidden snapshot rectangle for one member: intentional floats ride the live
/// float snapshot with the tiled allocation as fallback; every other member
/// (including born holds, which never seed a float snapshot) rides the tiled
/// allocation with the float snapshot as fallback. `None` only when both are
/// missing, so row assembly drops nothing it could keep.
#[must_use]
pub const fn hidden_snapshot_rect(
    is_float: bool,
    float_rect: Option<Rect>,
    member_rect: Option<Rect>,
) -> Option<Rect> {
    if is_float {
        match float_rect {
            Some(rect) => Some(rect),
            None => member_rect,
        }
    } else {
        match member_rect {
            Some(rect) => Some(rect),
            None => float_rect,
        }
    }
}

/// Pure owner dispatch gate for one workspace digit, shared by the native
/// loop and offline tests. `takeover` is the `--no-keyboard-snap-takeover`
/// switch (off disables all product interception); `suspended` covers
/// fullscreen suspension, managed gestures, and gesture settle; `send`
/// requires a managed focused member while select also serves empty
/// workspaces and unmanaged foreground.
#[must_use]
pub const fn dispatch_decision(
    takeover: bool,
    suspended: bool,
    send: bool,
    managed_focus: bool,
) -> Option<&'static str> {
    if !takeover {
        return Some("takeover-off");
    }
    if suspended {
        return Some("suspended");
    }
    if send && !managed_focus {
        return Some("unmanaged");
    }
    None
}

/// Membership snapshot helper: group tokens by domain for observation
/// assembly without touching native state.
#[must_use]
pub fn rows_for(
    members: &BTreeMap<String, OwnerRow>,
    workspace_members: &BTreeSet<WindowKey>,
    token_of: &BTreeMap<WindowKey, String>,
) -> Vec<OwnerRow> {
    let mut rows = Vec::new();
    for member in workspace_members {
        if let Some(token) = token_of.get(member)
            && let Some(row) = members.get(token)
        {
            rows.push(row.clone());
        }
    }
    rows.sort_by(|a, b| a.token.cmp(&b.token));
    rows
}

/// One member's portable view for domain-row assembly: stable session key,
/// Engine token, best-known rectangle (fresh visible read, fresh retained
/// frame, or hidden snapshot), fresh minimum-size hint (eligible visible
/// reads plus verified hidden snapshots; retained rows carry no hint), and
/// the slotless floating exception flag (intentional float or born-held
/// fullscreen, never a native write).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemberView {
    pub key: WindowKey,
    pub token: String,
    pub rect: Rect,
    pub hints: tiler_core::size_hints::WindowSizeHints,
    pub floating: bool,
}

/// Assemble complete Engine observation rows for one workspace domain from
/// the union of eligible visible views, retained-occupancy views
/// (minimized/maximized/fullscreen with last-known frames), and hidden
/// snapshots. Sorted by token for deterministic fingerprints; hidden Engine
/// membership survives because every retained member rides a row.
///
/// Returns `None` when any member lacks a view (unknown snapshot): the
/// caller must defer every Engine handle path with retained state instead of
/// presenting a falsely complete observation that would drop membership.
#[must_use]
pub fn domain_rows(
    workspace_members: &BTreeSet<WindowKey>,
    views: &[MemberView],
) -> Option<Vec<OwnerRow>> {
    let mut rows = Vec::new();
    for member in workspace_members {
        let view = views.iter().find(|v| v.key == *member)?;
        rows.push(OwnerRow {
            token: view.token.clone(),
            rect: view.rect,
            hints: view.hints,
            floating: view.floating,
        });
    }
    rows.sort_by(|a, b| a.token.cmp(&b.token));
    Some(rows)
}

/// Fixed-size admission observation flags for one Engine window
/// (R-SPC-04 D1-D8, G-05). Carried per observation from live adapter state,
/// never inferred in core and never retained across observations:
///
/// - `fullscreen`: live native fullscreen overlay (D5 first-exit routing and
///   brand-new fullscreen holds). Sticky/maximized overlays keep their
///   existing adapter lanes and stay false here.
/// - `fixed_auto`: adapter-asserted automatic fixed-float origin (D4
///   fixed max-births admitted slotless beneath native maximize). Never set
///   from an intentional floating flag or fixed hints alone.
/// - `fixed_suppress`: explicit user tile win for this live client (D3/D6/D7:
///   unfloats, workspace-enable re-adoption, and restart-hydrated tile
///   overrides). The Engine records the win transactionally; the adapter's
///   runtime set plus on-window marker carry it across domain releases and
///   restarts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FixedAdmissionFlags {
    pub fullscreen: bool,
    pub fixed_auto: bool,
    pub fixed_suppress: bool,
}

/// Stamp fixed-size admission observation flags onto an assembled Engine
/// event's windows. Windows without an entry keep the builder defaults
/// (all false), so existing builder call sites and fixtures behave exactly
/// as before until the production handle path stamps live state. Pure:
/// no reads, no writes, no topology effects.
///
/// Sticky-dominance note: the production stamp never asserts `fixed_auto`
/// for a sticky member (core never marks automatic for a sticky
/// observation), and sticky/maximized Engine flags stay adapter-owned
/// (false) because `SessionCommand::Admit` refuses overlaid observations -
/// stamping them would refuse legitimate ToggleFloat unfloats (notably
/// sticky-off-to-tile). Sticky intent therefore always wins over automatic
/// origin without touching the ToggleFloat binding.
pub fn stamp_fixed_admission_flags(
    windows: &mut [EngineWindow],
    flags: &std::collections::HashMap<WindowId, FixedAdmissionFlags>,
) {
    for window in windows.iter_mut() {
        if let Some(entry) = flags.get(&window.window) {
            window.fullscreen = entry.fullscreen;
            window.fixed_auto = entry.fixed_auto;
            window.fixed_suppress = entry.fixed_suppress;
        }
    }
}

/// Same-HWND reuse set: known member keys sharing the fresh window's HWND
/// with a different full identity. The fresh enumeration read is
/// authoritative: a HWND hosts exactly one process, so any other key for the
/// number is a destroyed window whose number the OS recycled while visible.
/// Live hidden claims are excluded: they own their HWND through the ledger
/// and audit path (retire with identity-safe reveal), never this repair.
#[must_use]
pub fn reused_hwnd_stale(
    known: &[WindowKey],
    hidden_claimed: &BTreeSet<WindowKey>,
    fresh: &WindowKey,
) -> Vec<WindowKey> {
    known
        .iter()
        .filter(|k| k.hwnd == fresh.hwnd && *k != fresh && !hidden_claimed.contains(*k))
        .cloned()
        .collect()
}

/// Visible-membership lifetime decision: the live member tag must equal the
/// tag stored at admission. A same-process HWND reuse starts without our
/// window-lifetime property (absent) or with a stale tag, so it fails closed
/// exactly where HWND/PID/creation all still agree. An empty stored tag
/// never matches: untracked windows are never trusted, only stamped.
#[must_use]
pub fn visible_lifetime_ok(stored: &str, live: Option<&str>) -> bool {
    !stored.is_empty() && live.is_some_and(|tag| tag == stored)
}

/// Recycled-HWND guard: the live window matches the stored member only when
/// HWND, PID, and process creation all agree. A same-process HWND reuse keeps
/// all three, so this guard alone never sees it: the visible-membership
/// lifetime property must also agree (see [`visible_lifetime_ok`]).
#[must_use]
pub fn member_matches(
    stored: &WindowKey,
    live_hwnd: u64,
    live_pid: u32,
    live_creation: &str,
) -> bool {
    stored.hwnd == live_hwnd && stored.pid == live_pid && stored.creation.as_str() == live_creation
}

/// Portable domain value for one `(output, workspace)` pair.
#[must_use]
pub fn workspace_domain(
    output: &str,
    workspace: &str,
    bounds: Rect,
    gap: i32,
) -> (OutputDomain, DomainKey) {
    let key = DomainKey {
        output: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
    };
    let domain = OutputDomain {
        id: OutputId(output.to_owned()),
        workspace: WorkspaceId(workspace.to_owned()),
        bounds,
        gap,
        adjacent: BTreeMap::new(),
    };
    (domain, key)
}

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

#[cfg(test)]
#[path = "workspace_owner/tests.rs"]
mod tests;

/// R-SPC-04 D1-D8 + G-05 fixed-size admission regressions at the adapter
/// boundary: real retained `Engine` observations through the production
/// builders plus [`stamp_fixed_admission_flags`], the same stamping the
/// production handle path applies. No native calls, no HWNDs.
#[cfg(test)]
#[path = "workspace_owner/tests_fixed_admission.rs"]
mod fixed_admission_tests;

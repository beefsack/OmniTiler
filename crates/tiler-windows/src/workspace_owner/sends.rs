#![forbid(unsafe_code)]
use super::*;

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

#![forbid(unsafe_code)]
//! Portable workspace-owner policy: output context, select/send resolution,
//! Engine observation assembly, and follow verification.
//!
//! Native effects (hide/reveal/focus/monitor identity) live in
//! `tiling_sys` (`cfg(windows)` only). This module owns no HWNDs, no window
//! properties, and no logs: pure resolution plus `CoreEvent` construction for
//! the retained `Engine` `SendToWorkspace` route.

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
        command: CoreCommand::SendToWorkspace {
            window: mover_token.to_owned(),
            target_output: String::new(),
            target_workspace: String::new(),
            follow,
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

/// Item 9 follow-focus policy for a workspace send: a stable fullscreen
/// snapshot suppresses ALL explicit focus actuation for the follow
/// transition (the select still reveals/reconciles the target; a fresh
/// foreground readback reports observed focus honestly). Ordinary and
/// maximized sends keep explicit focus. Production `workspace_send_follow`
/// MUST route through this; it never touches windows, only decides.
#[must_use]
pub const fn send_focus_suppressed(snapshot_fullscreen: bool) -> bool {
    snapshot_fullscreen
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
/// instead of zeros. Both-domain `SendWorkspace` plans carry source reflow
/// and target placement together; the domain writable set scopes each pass.
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
mod tests {
    use super::*;
    use crate::workspace::ManagedWorkspaces;

    fn rect(x: i32, y: i32) -> Rect {
        Rect {
            x,
            y,
            w: 800,
            h: 600,
        }
    }

    fn key(hwnd: u64) -> WindowKey {
        WindowKey {
            hwnd,
            pid: 1000 + hwnd as u32,
            creation: format!("c{hwnd:016x}"),
        }
    }

    fn seed_two_tiled(
        engine: &mut tiler_core::engine::Engine,
        owner: &OwnerId,
        generation: &GenerationId,
        domain: &(OutputDomain, DomainKey),
        bounds: Rect,
    ) {
        let correlation = CorrelationId::parse("seed").expect("correlation");
        let event = crate::tiling::build_reconcile_event_for(
            owner,
            generation,
            &correlation,
            0,
            2,
            &domain.0,
            &domain.1,
            8,
            &[
                (
                    WindowId("w1".to_owned()),
                    bounds,
                    tiler_core::size_hints::WindowSizeHints::none(),
                ),
                (
                    WindowId("w2".to_owned()),
                    bounds,
                    tiler_core::size_hints::WindowSizeHints::none(),
                ),
            ],
            Some(&WindowId("w1".to_owned())),
        );
        let reply = engine.handle(&event);
        assert!(
            matches!(
                reply,
                tiler_core::boundary::CoreReply::Projection(_)
                    | tiler_core::boundary::CoreReply::Tiled(_)
            ),
            "seed must converge, got {reply:?}"
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn float_event(
        owner: &OwnerId,
        generation: &GenerationId,
        correlation: &str,
        revision: u64,
        domain: &(OutputDomain, DomainKey),
        rows: &[(WindowId, Rect, bool)],
        focused: &str,
        window: &str,
        float_rect: Option<Rect>,
    ) -> tiler_core::boundary::CoreEvent {
        use tiler_core::boundary::CoreCommand;
        let correlation = CorrelationId::parse(correlation).expect("correlation");
        let fp = crate::tiling::fingerprint(
            &rows
                .iter()
                .map(|(token, rect, _)| (token.0.clone(), *rect))
                .collect::<Vec<_>>(),
        );
        let carried: Vec<(
            WindowId,
            Rect,
            tiler_core::size_hints::WindowSizeHints,
            bool,
        )> = rows
            .iter()
            .map(|(token, rect, floating)| {
                (
                    token.clone(),
                    *rect,
                    tiler_core::size_hints::WindowSizeHints::none(),
                    *floating,
                )
            })
            .collect();
        let mut event = crate::tiling::build_reconcile_event_for_floating(
            owner,
            generation,
            &correlation,
            revision,
            fp,
            &domain.0,
            &domain.1,
            8,
            &carried,
            Some(&WindowId(focused.to_owned())),
        );
        event.command = CoreCommand::ToggleFloat {
            window: window.to_owned(),
            float_rect,
        };
        event
    }

    fn revision_of(engine: &tiler_core::engine::Engine, domain: &(OutputDomain, DomainKey)) -> u64 {
        engine
            .session(&domain.1)
            .map(|s| s.accepted_revision())
            .unwrap_or(0)
    }

    #[test]
    fn toggle_float_first_float_centers_sixty_percent() {
        // Tiled-to-float with no rect selects the centered 60% work-area
        // fallback (KDE parity): the reply carries the placement for native
        // actuation while sibling reflow excludes the floated window.
        use tiler_core::boundary::CoreReply;
        let mut engine = tiler_core::engine::Engine::new();
        let owner = OwnerId::parse("tiler-windows").expect("owner");
        let generation = GenerationId::parse("aa").expect("generation");
        engine.sync_binding(&owner, &generation);
        let bounds = rect(0, 0);
        let domain = workspace_domain("mon-a", "ws-1", bounds, 8);
        seed_two_tiled(&mut engine, &owner, &generation, &domain, bounds);
        let event = float_event(
            &owner,
            &generation,
            "float-1",
            revision_of(&engine, &domain),
            &domain,
            &[
                (WindowId("w1".to_owned()), bounds, false),
                (WindowId("w2".to_owned()), bounds, false),
            ],
            "w1",
            "w1",
            None,
        );
        let reply = engine.handle(&event);
        let CoreReply::Tiled(plan) = reply else {
            panic!("float commits, got {reply:?}");
        };
        // Centered 60% of the 800x600 domain: 480x360 at (160, 120).
        let centered = Rect {
            x: 160,
            y: 120,
            w: 480,
            h: 360,
        };
        assert_eq!(plan.float_rect, Some(centered));
        assert!(
            !plan.geometry.iter().any(|g| g.window.0 == "w1"),
            "floated window leaves the tree"
        );
        assert!(
            plan.geometry.iter().any(|g| g.window.0 == "w2"),
            "sibling reflows"
        );
    }

    #[test]
    fn toggle_float_retains_moved_frame() {
        // Unfloat carries the live frame rect so a user moved/resized float is
        // retained: floating again with no rect reuses the moved frame, never
        // the centered fallback.
        use tiler_core::boundary::CoreReply;
        let mut engine = tiler_core::engine::Engine::new();
        let owner = OwnerId::parse("tiler-windows").expect("owner");
        let generation = GenerationId::parse("aa").expect("generation");
        engine.sync_binding(&owner, &generation);
        let bounds = rect(0, 0);
        let domain = workspace_domain("mon-a", "ws-1", bounds, 8);
        seed_two_tiled(&mut engine, &owner, &generation, &domain, bounds);
        let float = |engine: &tiler_core::engine::Engine,
                     rows: &[(WindowId, Rect, bool)],
                     correlation: &str,
                     window: &str,
                     float_rect: Option<Rect>| {
            float_event(
                &owner,
                &generation,
                correlation,
                revision_of(engine, &domain),
                &domain,
                rows,
                window,
                window,
                float_rect,
            )
        };
        let moved = Rect {
            x: 210,
            y: 150,
            w: 480,
            h: 360,
        };
        let reply = engine.handle(&float(
            &engine,
            &[
                (WindowId("w1".to_owned()), bounds, false),
                (WindowId("w2".to_owned()), bounds, false),
            ],
            "float-1",
            "w1",
            None,
        ));
        assert!(
            matches!(reply, CoreReply::Tiled(_)),
            "float commits, got {reply:?}"
        );
        let reply = engine.handle(&float(
            &engine,
            &[
                (WindowId("w1".to_owned()), moved, true),
                (WindowId("w2".to_owned()), bounds, false),
            ],
            "float-2",
            "w1",
            Some(moved),
        ));
        let CoreReply::Tiled(plan) = reply else {
            panic!("unfloat commits, got {reply:?}");
        };
        assert_eq!(plan.float_rect, None, "unfloat stages no float rect");
        assert!(
            plan.geometry.iter().any(|g| g.window.0 == "w2"),
            "sibling survives readmission"
        );
        let reply = engine.handle(&float(
            &engine,
            &[
                (
                    WindowId("w1".to_owned()),
                    plan.geometry
                        .iter()
                        .find(|g| g.window.0 == "w1")
                        .expect("admission places the mover")
                        .rect,
                    false,
                ),
                (WindowId("w2".to_owned()), bounds, false),
            ],
            "float-3",
            "w1",
            None,
        ));
        let CoreReply::Tiled(plan) = reply else {
            panic!("refloat commits, got {reply:?}");
        };
        assert_eq!(plan.float_rect, Some(moved));
    }

    #[allow(clippy::too_many_arguments)]
    fn orientation_event(
        owner: &OwnerId,
        generation: &GenerationId,
        correlation: &str,
        revision: u64,
        domain: &(OutputDomain, DomainKey),
        rows: &[(
            WindowId,
            Rect,
            tiler_core::size_hints::WindowSizeHints,
            bool,
        )],
        focused: &str,
        window: &str,
    ) -> tiler_core::boundary::CoreEvent {
        use tiler_core::boundary::CoreCommand;
        let correlation = CorrelationId::parse(correlation).expect("correlation");
        let fp = crate::tiling::fingerprint(
            &rows
                .iter()
                .map(|(token, rect, _, _)| (token.0.clone(), *rect))
                .collect::<Vec<_>>(),
        );
        let carried: Vec<(
            WindowId,
            Rect,
            tiler_core::size_hints::WindowSizeHints,
            bool,
        )> = rows.to_vec();
        let mut event = crate::tiling::build_reconcile_event_for_floating(
            owner,
            generation,
            &correlation,
            revision,
            fp,
            &domain.0,
            &domain.1,
            8,
            &carried,
            Some(&WindowId(focused.to_owned())),
        );
        event.command = CoreCommand::ToggleOrientation {
            window: window.to_owned(),
        };
        event
    }

    fn seed_pair_for_orientation(
        engine: &mut tiler_core::engine::Engine,
        owner: &OwnerId,
        generation: &GenerationId,
        domain: &(OutputDomain, DomainKey),
        bounds: Rect,
        focused: &str,
    ) {
        use tiler_core::boundary::CoreReply;
        for (correlation, tokens, focus) in [
            ("ori-seed-1", vec!["w1"], "w1"),
            ("ori-seed-2", vec!["w1", "w2"], focused),
        ] {
            let rows: Vec<(
                WindowId,
                Rect,
                tiler_core::size_hints::WindowSizeHints,
                bool,
            )> = tokens
                .iter()
                .map(|token| {
                    (
                        WindowId((*token).to_owned()),
                        bounds,
                        tiler_core::size_hints::WindowSizeHints::none(),
                        false,
                    )
                })
                .collect();
            let correlation_id = CorrelationId::parse(correlation).expect("correlation");
            let fp = crate::tiling::fingerprint(
                &rows
                    .iter()
                    .map(|(token, rect, _, _)| (token.0.clone(), *rect))
                    .collect::<Vec<_>>(),
            );
            let event = crate::tiling::build_reconcile_event_for_floating(
                owner,
                generation,
                &correlation_id,
                revision_of(engine, domain),
                fp,
                &domain.0,
                &domain.1,
                8,
                &rows,
                Some(&WindowId(focus.to_owned())),
            );
            let reply = engine.handle(&event);
            assert!(
                matches!(reply, CoreReply::Projection(_) | CoreReply::Tiled(_)),
                "seed {correlation} converges, got {reply:?}"
            );
        }
    }

    fn root_group_of(
        engine: &tiler_core::engine::Engine,
        domain: &(OutputDomain, DomainKey),
    ) -> (tiler_core::directional::Axis, Vec<String>, Vec<u64>) {
        use tiler_core::directional::Node;
        let tree = engine
            .session(&domain.1)
            .expect("session")
            .snapshot()
            .domains
            .into_iter()
            .next()
            .expect("domain")
            .tree
            .expect("tree");
        match tree {
            Node::Group {
                axis,
                children,
                shares,
                ..
            } => (
                axis,
                children.iter().map(|child| child.id().0.clone()).collect(),
                shares,
            ),
            Node::Leaf { .. } => panic!("expected a root group, got a lone leaf"),
        }
    }

    fn focus_leaf_of(
        engine: &tiler_core::engine::Engine,
        domain: &(OutputDomain, DomainKey),
    ) -> Option<String> {
        engine
            .session(&domain.1)
            .expect("session")
            .focus()
            .1
            .map(|leaf| leaf.0.clone())
    }

    #[test]
    fn toggle_orientation_flips_root_horizontal_to_vertical() {
        // Wide pair admits an H root; Win+O flips it to V, preserving child
        // order, shares, and focus with a matching Tiled kind.
        use tiler_core::boundary::{CoreReply, TiledKind};
        use tiler_core::directional::Axis;
        let mut engine = tiler_core::engine::Engine::new();
        let owner = OwnerId::parse("tiler-windows").expect("owner");
        let generation = GenerationId::parse("aa").expect("generation");
        engine.sync_binding(&owner, &generation);
        let bounds = rect(0, 0);
        let domain = workspace_domain("mon-a", "ws-1", bounds, 8);
        seed_pair_for_orientation(&mut engine, &owner, &generation, &domain, bounds, "w1");
        let (axis_before, order_before, shares_before) = root_group_of(&engine, &domain);
        assert_eq!(axis_before, Axis::Horizontal);
        let focus_before = focus_leaf_of(&engine, &domain);
        let reply = engine.handle(&orientation_event(
            &owner,
            &generation,
            "ori-1",
            revision_of(&engine, &domain),
            &domain,
            &[
                (
                    WindowId("w1".to_owned()),
                    bounds,
                    tiler_core::size_hints::WindowSizeHints::none(),
                    false,
                ),
                (
                    WindowId("w2".to_owned()),
                    bounds,
                    tiler_core::size_hints::WindowSizeHints::none(),
                    false,
                ),
            ],
            "w1",
            "w1",
        ));
        let CoreReply::Tiled(plan) = reply else {
            panic!("toggle commits, got {reply:?}");
        };
        assert_eq!(plan.kind, TiledKind::ToggleOrientation);
        let (axis_after, order_after, shares_after) = root_group_of(&engine, &domain);
        assert_eq!(axis_after, Axis::Vertical);
        assert_eq!(order_after, order_before, "child order retained");
        assert_eq!(shares_after, shares_before, "shares retained");
        assert_eq!(
            focus_leaf_of(&engine, &domain),
            focus_before,
            "focus retained"
        );
        assert_eq!(plan.focus_leaf.map(|leaf| leaf.0), focus_before);
        assert_eq!(plan.geometry.len(), 2);
    }

    #[test]
    fn toggle_orientation_flips_root_vertical_to_horizontal() {
        // Tall pair admits a V root; Win+O flips it to H.
        use tiler_core::boundary::{CoreReply, TiledKind};
        use tiler_core::directional::Axis;
        let mut engine = tiler_core::engine::Engine::new();
        let owner = OwnerId::parse("tiler-windows").expect("owner");
        let generation = GenerationId::parse("aa").expect("generation");
        engine.sync_binding(&owner, &generation);
        let bounds = Rect {
            x: 0,
            y: 0,
            w: 600,
            h: 800,
        };
        let domain = workspace_domain("mon-a", "ws-1", bounds, 8);
        seed_pair_for_orientation(&mut engine, &owner, &generation, &domain, bounds, "w2");
        let (axis_before, _, _) = root_group_of(&engine, &domain);
        assert_eq!(axis_before, Axis::Vertical);
        let reply = engine.handle(&orientation_event(
            &owner,
            &generation,
            "ori-1",
            revision_of(&engine, &domain),
            &domain,
            &[
                (
                    WindowId("w1".to_owned()),
                    bounds,
                    tiler_core::size_hints::WindowSizeHints::none(),
                    false,
                ),
                (
                    WindowId("w2".to_owned()),
                    bounds,
                    tiler_core::size_hints::WindowSizeHints::none(),
                    false,
                ),
            ],
            "w2",
            "w2",
        ));
        let CoreReply::Tiled(plan) = reply else {
            panic!("toggle commits, got {reply:?}");
        };
        assert_eq!(plan.kind, TiledKind::ToggleOrientation);
        let (axis_after, _, _) = root_group_of(&engine, &domain);
        assert_eq!(axis_after, Axis::Horizontal);
    }

    #[test]
    fn toggle_orientation_flips_only_the_nested_immediate_parent() {
        // H[w1 V[w2 w3]] with w3 focused: only the inner V flips to H; the
        // root stays H with order/shares/focus retained.
        use tiler_core::boundary::{CoreReply, TiledKind};
        use tiler_core::directional::{Axis, Node};
        let mut engine = tiler_core::engine::Engine::new();
        let owner = OwnerId::parse("tiler-windows").expect("owner");
        let generation = GenerationId::parse("aa").expect("generation");
        engine.sync_binding(&owner, &generation);
        let bounds = Rect {
            x: 0,
            y: 0,
            w: 120,
            h: 80,
        };
        let domain = workspace_domain("mon-a", "ws-1", bounds, 0);
        let wide = Rect {
            x: 0,
            y: 0,
            w: 120,
            h: 80,
        };
        let tall = Rect {
            x: 0,
            y: 0,
            w: 80,
            h: 120,
        };
        let none = tiler_core::size_hints::WindowSizeHints::none();
        for (correlation, rows, focused) in [
            (
                "ori-seed-1",
                vec![(WindowId("w1".to_owned()), wide, none, false)],
                "w1",
            ),
            (
                "ori-seed-2",
                vec![
                    (WindowId("w1".to_owned()), wide, none, false),
                    (WindowId("w2".to_owned()), wide, none, false),
                ],
                "w2",
            ),
            (
                "ori-seed-3",
                vec![
                    (WindowId("w1".to_owned()), wide, none, false),
                    (WindowId("w2".to_owned()), wide, none, false),
                    (WindowId("w3".to_owned()), tall, none, false),
                ],
                "w3",
            ),
        ] {
            let correlation_id = CorrelationId::parse(correlation).expect("correlation");
            let fp = crate::tiling::fingerprint(
                &rows
                    .iter()
                    .map(|(token, rect, _, _)| (token.0.clone(), *rect))
                    .collect::<Vec<_>>(),
            );
            let event = crate::tiling::build_reconcile_event_for_floating(
                &owner,
                &generation,
                &correlation_id,
                revision_of(&engine, &domain),
                fp,
                &domain.0,
                &domain.1,
                0,
                &rows,
                Some(&WindowId(focused.to_owned())),
            );
            let reply = engine.handle(&event);
            assert!(
                matches!(reply, CoreReply::Projection(_) | CoreReply::Tiled(_)),
                "seed {correlation} converges, got {reply:?}"
            );
        }
        let (root_before, _, shares_before) = root_group_of(&engine, &domain);
        assert_eq!(root_before, Axis::Horizontal);
        let focus_before = focus_leaf_of(&engine, &domain);
        let reply = engine.handle(&orientation_event(
            &owner,
            &generation,
            "ori-nested",
            revision_of(&engine, &domain),
            &domain,
            &[
                (WindowId("w1".to_owned()), wide, none, false),
                (WindowId("w2".to_owned()), wide, none, false),
                (WindowId("w3".to_owned()), tall, none, false),
            ],
            "w3",
            "w3",
        ));
        let CoreReply::Tiled(plan) = reply else {
            panic!("toggle commits, got {reply:?}");
        };
        assert_eq!(plan.kind, TiledKind::ToggleOrientation);
        let tree = engine
            .session(&domain.1)
            .expect("session")
            .snapshot()
            .domains
            .into_iter()
            .next()
            .expect("domain")
            .tree
            .expect("tree");
        match tree {
            Node::Group {
                axis,
                children,
                shares,
                ..
            } => {
                assert_eq!(axis, Axis::Horizontal, "root stays H");
                assert_eq!(shares, shares_before, "root shares retained");
                assert_eq!(children.len(), 2);
                assert_eq!(children[0].id().0, "leaf-w1");
                match &children[1] {
                    Node::Group { axis, children, .. } => {
                        assert_eq!(*axis, Axis::Horizontal, "inner V flips to H");
                        assert_eq!(children.len(), 2);
                        assert_eq!(children[0].id().0, "leaf-w2");
                        assert_eq!(children[1].id().0, "leaf-w3");
                    }
                    other => panic!("expected inner H[w2 w3], got {other:?}"),
                }
            }
            other => panic!("expected root H[w1 H[w2 w3]], got {other:?}"),
        }
        assert_eq!(
            focus_leaf_of(&engine, &domain),
            focus_before,
            "focus retained"
        );
        assert_eq!(plan.geometry.len(), 3);
    }

    #[test]
    fn toggle_orientation_twice_restores_exact_tree_with_unequal_shares() {
        // A resize makes root shares unequal; two toggles restore the exact
        // pre-toggle tree, geometry, and focus.
        use tiler_core::boundary::{CoreCommand, CoreReply, TiledKind};
        use tiler_core::directional::Axis;
        let mut engine = tiler_core::engine::Engine::new();
        let owner = OwnerId::parse("tiler-windows").expect("owner");
        let generation = GenerationId::parse("aa").expect("generation");
        engine.sync_binding(&owner, &generation);
        let bounds = rect(0, 0);
        let domain = workspace_domain("mon-a", "ws-1", bounds, 8);
        seed_pair_for_orientation(&mut engine, &owner, &generation, &domain, bounds, "w2");
        let none = tiler_core::size_hints::WindowSizeHints::none();
        let rows = [
            (WindowId("w1".to_owned()), bounds, none, false),
            (WindowId("w2".to_owned()), bounds, none, false),
        ];
        let resize = |engine: &tiler_core::engine::Engine, correlation: &str| {
            let correlation_id = CorrelationId::parse(correlation).expect("correlation");
            let fp = crate::tiling::fingerprint(
                &rows
                    .iter()
                    .map(|(token, rect, _, _)| (token.0.clone(), *rect))
                    .collect::<Vec<_>>(),
            );
            let mut event = crate::tiling::build_reconcile_event_for_floating(
                &owner,
                &generation,
                &correlation_id,
                revision_of(engine, &domain),
                fp,
                &domain.0,
                &domain.1,
                8,
                &rows,
                Some(&WindowId("w2".to_owned())),
            );
            event.command = CoreCommand::Resize {
                window: "w2".to_owned(),
                direction: "left".to_owned(),
                mode: "outwards".to_owned(),
                press_index: 0,
            };
            event
        };
        let reply = engine.handle(&resize(&engine, "ori-resize-1"));
        assert!(
            matches!(reply, CoreReply::Resize(_)),
            "resize commits, got {reply:?}"
        );
        let (_, _, shares) = root_group_of(&engine, &domain);
        assert_eq!(shares.len(), 2);
        assert_ne!(
            shares[0], shares[1],
            "resize makes root shares unequal: {shares:?}"
        );
        let before = root_group_of(&engine, &domain);
        assert_eq!(before.0, Axis::Horizontal);
        let focus_before = focus_leaf_of(&engine, &domain);
        let geometry_before: Vec<(String, Rect)> = match engine.handle(&orientation_event(
            &owner,
            &generation,
            "ori-first",
            revision_of(&engine, &domain),
            &domain,
            &rows,
            "w2",
            "w2",
        )) {
            CoreReply::Tiled(plan) => {
                assert_eq!(plan.kind, TiledKind::ToggleOrientation);
                plan.geometry
                    .iter()
                    .map(|g| (g.window.0.clone(), g.rect))
                    .collect()
            }
            reply => panic!("first toggle commits, got {reply:?}"),
        };
        let mid = root_group_of(&engine, &domain);
        assert_eq!(mid.0, Axis::Vertical);
        assert_eq!(mid.1, before.1, "order retained through the flip");
        assert_eq!(mid.2, before.2, "unequal shares retained through the flip");
        // Second toggle restores the exact pre-toggle tree, geometry, focus.
        let geometry_second: Vec<(String, Rect)> = match engine.handle(&orientation_event(
            &owner,
            &generation,
            "ori-second",
            revision_of(&engine, &domain),
            &domain,
            &rows,
            "w2",
            "w2",
        )) {
            CoreReply::Tiled(plan) => {
                assert_eq!(plan.kind, TiledKind::ToggleOrientation);
                plan.geometry
                    .iter()
                    .map(|g| (g.window.0.clone(), g.rect))
                    .collect()
            }
            reply => panic!("second toggle commits, got {reply:?}"),
        };
        // Geometry rects in the toggle plans describe the flipped allocation;
        // the committed tree is the exact roundtrip signal.
        let after = root_group_of(&engine, &domain);
        assert_eq!(after, before, "double toggle restores the exact tree");
        assert_eq!(
            focus_leaf_of(&engine, &domain),
            focus_before,
            "focus retained through both toggles"
        );
        assert_ne!(
            geometry_before, geometry_second,
            "the two toggles allocate opposite axes"
        );
    }

    #[test]
    fn toggle_orientation_honors_minimum_allocation() {
        // A satisfiable minimum on w1 takes slack through the hints-aware
        // projector, exactly like every other workflow.
        use tiler_core::boundary::{CoreReply, TiledKind};
        use tiler_core::size_hints::WindowSizeHints;
        let mut engine = tiler_core::engine::Engine::new();
        let owner = OwnerId::parse("tiler-windows").expect("owner");
        let generation = GenerationId::parse("aa").expect("generation");
        engine.sync_binding(&owner, &generation);
        let bounds = rect(0, 0);
        let domain = workspace_domain("mon-a", "ws-1", bounds, 8);
        seed_pair_for_orientation(&mut engine, &owner, &generation, &domain, bounds, "w1");
        let hints = WindowSizeHints {
            min_w: None,
            min_h: Some(400),
            max_w: None,
            max_h: None,
        };
        let reply = engine.handle(&orientation_event(
            &owner,
            &generation,
            "ori-min",
            revision_of(&engine, &domain),
            &domain,
            &[
                (WindowId("w1".to_owned()), bounds, hints, false),
                (
                    WindowId("w2".to_owned()),
                    bounds,
                    WindowSizeHints::none(),
                    false,
                ),
            ],
            "w1",
            "w1",
        ));
        let CoreReply::Tiled(plan) = reply else {
            panic!("toggle commits, got {reply:?}");
        };
        assert_eq!(plan.kind, TiledKind::ToggleOrientation);
        let placed = plan
            .geometry
            .iter()
            .find(|g| g.window.0 == "w1")
            .expect("w1 placed");
        assert!(
            placed.rect.h >= 400,
            "minimum takes slack: {:?}",
            placed.rect
        );
        assert!(
            plan.geometry.iter().all(|g| !g.overconstrained),
            "satisfiable minima stay unconstrained"
        );
    }

    #[test]
    fn toggle_orientation_lone_leaf_noop_then_long_edge_admission() {
        // A sole root leaf refuses as unchanged with no plan; admitting a
        // second window afterwards still uses the long-edge rule (no saved
        // orientation hint from the no-op).
        use tiler_core::boundary::CoreReply;
        let mut engine = tiler_core::engine::Engine::new();
        let owner = OwnerId::parse("tiler-windows").expect("owner");
        let generation = GenerationId::parse("aa").expect("generation");
        engine.sync_binding(&owner, &generation);
        let bounds = rect(0, 0);
        let domain = workspace_domain("mon-a", "ws-1", bounds, 8);
        let none = tiler_core::size_hints::WindowSizeHints::none();
        let correlation_id = CorrelationId::parse("ori-lone-seed").expect("correlation");
        let seed_rows = [(WindowId("w1".to_owned()), bounds, none, false)];
        let fp = crate::tiling::fingerprint(
            &seed_rows
                .iter()
                .map(|(token, rect, _, _)| (token.0.clone(), *rect))
                .collect::<Vec<_>>(),
        );
        let seed = crate::tiling::build_reconcile_event_for_floating(
            &owner,
            &generation,
            &correlation_id,
            revision_of(&engine, &domain),
            fp,
            &domain.0,
            &domain.1,
            8,
            &seed_rows,
            Some(&WindowId("w1".to_owned())),
        );
        let reply = engine.handle(&seed);
        assert!(
            matches!(reply, CoreReply::Projection(_) | CoreReply::Tiled(_)),
            "lone seed converges, got {reply:?}"
        );
        let reply = engine.handle(&orientation_event(
            &owner,
            &generation,
            "ori-lone",
            revision_of(&engine, &domain),
            &domain,
            &seed_rows,
            "w1",
            "w1",
        ));
        match reply {
            CoreReply::Rejected { kind, .. } => assert_eq!(kind, "unchanged"),
            reply => panic!("lone toggle refuses unchanged, got {reply:?}"),
        }
        // Fresh admission beside the lone leaf still splits the long edge.
        let admit_rows = [
            (WindowId("w1".to_owned()), bounds, none, false),
            (WindowId("w2".to_owned()), bounds, none, false),
        ];
        let admit_id = CorrelationId::parse("ori-lone-admit").expect("correlation");
        let admit_fp = crate::tiling::fingerprint(
            &admit_rows
                .iter()
                .map(|(token, rect, _, _)| (token.0.clone(), *rect))
                .collect::<Vec<_>>(),
        );
        let admit = crate::tiling::build_reconcile_event_for_floating(
            &owner,
            &generation,
            &admit_id,
            revision_of(&engine, &domain),
            admit_fp,
            &domain.0,
            &domain.1,
            8,
            &admit_rows,
            Some(&WindowId("w2".to_owned())),
        );
        let reply = engine.handle(&admit);
        assert!(
            matches!(reply, CoreReply::Projection(_) | CoreReply::Tiled(_)),
            "admission converges, got {reply:?}"
        );
        let (axis, _, _) = root_group_of(&engine, &domain);
        assert_eq!(
            axis,
            tiler_core::directional::Axis::Horizontal,
            "wide bounds still admit along the long edge"
        );
    }

    #[test]
    fn toggle_orientation_refuses_non_tiled_subjects() {
        // Float subjects, focus mismatches, and unknown windows refuse with
        // no plan: the adapter settles these as no-write outcomes.
        use tiler_core::boundary::CoreReply;
        let mut engine = tiler_core::engine::Engine::new();
        let owner = OwnerId::parse("tiler-windows").expect("owner");
        let generation = GenerationId::parse("aa").expect("generation");
        engine.sync_binding(&owner, &generation);
        let bounds = rect(0, 0);
        let domain = workspace_domain("mon-a", "ws-1", bounds, 8);
        seed_pair_for_orientation(&mut engine, &owner, &generation, &domain, bounds, "w1");
        let none = tiler_core::size_hints::WindowSizeHints::none();
        let rows = [
            (WindowId("w1".to_owned()), bounds, none, false),
            (WindowId("w2".to_owned()), bounds, none, false),
        ];
        // Float w1, then toggle it: not tiled.
        let reply = engine.handle(&float_event(
            &owner,
            &generation,
            "ori-float-1",
            revision_of(&engine, &domain),
            &domain,
            &[
                (WindowId("w1".to_owned()), bounds, false),
                (WindowId("w2".to_owned()), bounds, false),
            ],
            "w1",
            "w1",
            None,
        ));
        assert!(
            matches!(reply, CoreReply::Tiled(_)),
            "float commits, got {reply:?}"
        );
        let floated_rows = [
            (WindowId("w1".to_owned()), bounds, none, true),
            (WindowId("w2".to_owned()), bounds, none, false),
        ];
        let reply = engine.handle(&orientation_event(
            &owner,
            &generation,
            "ori-float-toggle",
            revision_of(&engine, &domain),
            &domain,
            &floated_rows,
            "w1",
            "w1",
        ));
        match reply {
            CoreReply::Rejected { kind, .. } => assert_eq!(kind, "not-tiled"),
            reply => panic!("float toggle refuses not-tiled, got {reply:?}"),
        }
        // Unfloat w1 again so later legs run tiled.
        let reply = engine.handle(&float_event(
            &owner,
            &generation,
            "ori-unfloat-1",
            revision_of(&engine, &domain),
            &domain,
            &[
                (WindowId("w1".to_owned()), bounds, true),
                (WindowId("w2".to_owned()), bounds, false),
            ],
            "w1",
            "w1",
            Some(bounds),
        ));
        assert!(
            matches!(reply, CoreReply::Tiled(_)),
            "unfloat commits, got {reply:?}"
        );
        // Toggle w2 while w1 is focused: focus mismatch.
        let reply = engine.handle(&orientation_event(
            &owner,
            &generation,
            "ori-mismatch",
            revision_of(&engine, &domain),
            &domain,
            &rows,
            "w1",
            "w2",
        ));
        match reply {
            CoreReply::Rejected { kind, .. } => assert_eq!(kind, "focus-mismatch"),
            reply => panic!("mismatched toggle refuses, got {reply:?}"),
        }
        // Toggle an unknown window: unknown window.
        let reply = engine.handle(&orientation_event(
            &owner,
            &generation,
            "ori-unknown",
            revision_of(&engine, &domain),
            &domain,
            &rows,
            "w1",
            "w9",
        ));
        match reply {
            CoreReply::Rejected { kind, .. } => assert_eq!(kind, "unknown-window"),
            reply => panic!("unknown toggle refuses, got {reply:?}"),
        }
    }

    #[test]
    fn toggle_orientation_fail_closed_on_focused_overlay_flags() {
        // A focused window observed with overlay flags refuses with no plan.
        // Production never sends flags (the adapter zeroes them and refuses
        // focused overlays first with orientation vocabulary); this pins the
        // shared fail-closed fence behind it.
        use tiler_core::boundary::CoreReply;
        let mut engine = tiler_core::engine::Engine::new();
        let owner = OwnerId::parse("tiler-windows").expect("owner");
        let generation = GenerationId::parse("aa").expect("generation");
        engine.sync_binding(&owner, &generation);
        let bounds = rect(0, 0);
        let domain = workspace_domain("mon-a", "ws-1", bounds, 8);
        seed_pair_for_orientation(&mut engine, &owner, &generation, &domain, bounds, "w1");
        let none = tiler_core::size_hints::WindowSizeHints::none();
        let rows = [
            (WindowId("w1".to_owned()), bounds, none, false),
            (WindowId("w2".to_owned()), bounds, none, false),
        ];
        for (correlation, flag) in [
            ("ori-overlay-max", "maximized"),
            ("ori-overlay-full", "fullscreen"),
        ] {
            let mut event = orientation_event(
                &owner,
                &generation,
                correlation,
                revision_of(&engine, &domain),
                &domain,
                &rows,
                "w1",
                "w1",
            );
            let focused = event
                .windows
                .iter_mut()
                .find(|w| w.window.0 == "w1")
                .expect("focused row");
            if flag == "maximized" {
                focused.maximized = true;
            } else {
                focused.fullscreen = true;
            }
            match engine.handle(&event) {
                CoreReply::Rejected { .. } => {}
                reply => panic!("flagged focus refuses, got {reply:?}"),
            }
        }
    }

    #[test]
    fn toggle_orientation_plan_carries_every_tile_for_write_scoping() {
        // The ToggleOrientation plan reprojects every tile (sibling reserved
        // slots included); the portable writable fence scopes native writes
        // to fresh non-hidden members, so hidden siblings take no writes.
        use std::collections::{BTreeMap, BTreeSet, HashSet};
        use tiler_core::boundary::{CoreReply, TiledKind};
        let mut engine = tiler_core::engine::Engine::new();
        let owner = OwnerId::parse("tiler-windows").expect("owner");
        let generation = GenerationId::parse("aa").expect("generation");
        engine.sync_binding(&owner, &generation);
        let bounds = rect(0, 0);
        let domain = workspace_domain("mon-a", "ws-1", bounds, 8);
        seed_pair_for_orientation(&mut engine, &owner, &generation, &domain, bounds, "w1");
        let none = tiler_core::size_hints::WindowSizeHints::none();
        let rows = [
            (WindowId("w1".to_owned()), bounds, none, false),
            (WindowId("w2".to_owned()), bounds, none, false),
        ];
        let reply = engine.handle(&orientation_event(
            &owner,
            &generation,
            "ori-scope",
            revision_of(&engine, &domain),
            &domain,
            &rows,
            "w1",
            "w1",
        ));
        let CoreReply::Tiled(plan) = reply else {
            panic!("toggle commits, got {reply:?}");
        };
        assert_eq!(plan.kind, TiledKind::ToggleOrientation);
        let writes = super::planned_writes(&CoreReply::Tiled(plan)).expect("plan extracts");
        assert_eq!(writes.len(), 2, "both tiles reproject");
        // Hidden members converge membership but never take geometry writes.
        let members: BTreeSet<WindowKey> = [key(1), key(2)].into_iter().collect();
        let mut token_of = BTreeMap::new();
        token_of.insert(key(1), "w1".to_owned());
        let fresh: HashSet<String> = ["w1".to_owned()].into_iter().collect();
        let scoped = super::writable_subset(&members, |k| *k == key(2), &token_of, &fresh);
        assert_eq!(scoped, fresh, "hidden w2 takes no writes");
    }

    #[test]
    fn hidden_snapshot_rect_prefers_live_float_then_tiled() {
        // Intentional floats ride the live float snapshot; every other member
        // (including born holds, which never seed a float snapshot) rides the
        // tiled allocation. Each lane falls back to the other snapshot so a
        // hidden member is never dropped while either exists.
        use super::hidden_snapshot_rect;
        let float_rect = Some(Rect {
            x: 1,
            y: 2,
            w: 3,
            h: 4,
        });
        let member_rect = Some(Rect {
            x: 5,
            y: 6,
            w: 7,
            h: 8,
        });
        assert_eq!(
            hidden_snapshot_rect(true, float_rect, member_rect),
            float_rect
        );
        assert_eq!(hidden_snapshot_rect(true, None, member_rect), member_rect);
        assert_eq!(
            hidden_snapshot_rect(false, float_rect, member_rect),
            member_rect
        );
        assert_eq!(hidden_snapshot_rect(false, float_rect, None), float_rect);
        assert_eq!(hidden_snapshot_rect(true, None, None), None);
        assert_eq!(hidden_snapshot_rect(false, None, None), None);
    }

    #[test]
    fn unfloat_matches_fresh_admission_topology() {
        // H[w1 V[w2 w3]] floats w3 to H[w1 w2]; unfloating w3 must equal a
        // fresh admission of a new window into H[w1 w2]: same tree topology
        // and same admitted geometry, never the old slot or domain bounds.
        use tiler_core::boundary::CoreReply;
        use tiler_core::directional::{Axis, Node};
        fn wide() -> Rect {
            Rect {
                x: 0,
                y: 0,
                w: 120,
                h: 80,
            }
        }
        fn tall() -> Rect {
            Rect {
                x: 0,
                y: 0,
                w: 80,
                h: 120,
            }
        }
        fn same_topology(left: &Node, right: &Node) -> bool {
            match (left, right) {
                (Node::Leaf { id: l }, Node::Leaf { id: r }) => l == r,
                (
                    Node::Group {
                        axis: l_axis,
                        children: l_children,
                        shares: l_shares,
                        ..
                    },
                    Node::Group {
                        axis: r_axis,
                        children: r_children,
                        shares: r_shares,
                        ..
                    },
                ) => {
                    l_axis == r_axis
                        && l_shares == r_shares
                        && l_children.len() == r_children.len()
                        && l_children
                            .iter()
                            .zip(r_children.iter())
                            .all(|(l, r)| same_topology(l, r))
                }
                _ => false,
            }
        }
        fn tree_of(
            engine: &tiler_core::engine::Engine,
            domain: &(OutputDomain, DomainKey),
        ) -> Node {
            engine
                .session(&domain.1)
                .expect("session")
                .snapshot()
                .domains
                .into_iter()
                .next()
                .expect("domain")
                .tree
                .expect("tree")
        }
        fn assert_nested_h_v(tree: &Node) {
            match tree {
                Node::Group { axis, children, .. } => {
                    assert_eq!(*axis, Axis::Horizontal);
                    assert_eq!(children.len(), 2);
                    assert_eq!(children[0].id().0, "leaf-w1");
                    match &children[1] {
                        Node::Group { axis, children, .. } => {
                            assert_eq!(*axis, Axis::Vertical);
                            assert_eq!(children.len(), 2);
                            assert_eq!(children[0].id().0, "leaf-w2");
                            assert_eq!(children[1].id().0, "leaf-w3");
                        }
                        other => panic!("expected inner V[w2 w3], got {other:?}"),
                    }
                }
                other => panic!("expected root H[w1 V[w2 w3]], got {other:?}"),
            }
        }
        fn reconcile(
            engine: &mut tiler_core::engine::Engine,
            owner: &OwnerId,
            generation: &GenerationId,
            domain: &(OutputDomain, DomainKey),
            correlation: &str,
            rows: &[(WindowId, Rect)],
            focused: &str,
        ) -> CoreReply {
            let correlation = CorrelationId::parse(correlation).expect("correlation");
            let carried: Vec<(
                WindowId,
                Rect,
                tiler_core::size_hints::WindowSizeHints,
                bool,
            )> = rows
                .iter()
                .map(|(token, rect)| {
                    (
                        token.clone(),
                        *rect,
                        tiler_core::size_hints::WindowSizeHints::none(),
                        false,
                    )
                })
                .collect();
            let fp = crate::tiling::fingerprint(
                &rows
                    .iter()
                    .map(|(token, rect)| (token.0.clone(), *rect))
                    .collect::<Vec<_>>(),
            );
            let event = crate::tiling::build_reconcile_event_for_floating(
                owner,
                generation,
                &correlation,
                revision_of(engine, domain),
                fp,
                &domain.0,
                &domain.1,
                0,
                &carried,
                Some(&WindowId(focused.to_owned())),
            );
            engine.handle(&event)
        }
        let bounds = Rect {
            x: 0,
            y: 0,
            w: 120,
            h: 80,
        };
        let domain = workspace_domain("mon-a", "ws-1", bounds, 0);
        let w1 = WindowId("w1".to_owned());
        let w2 = WindowId("w2".to_owned());
        let w3 = WindowId("w3".to_owned());
        // Engine A: seed H[w1 V[w2 w3]], float w3, unfloat with the live frame.
        let mut engine_a = tiler_core::engine::Engine::new();
        let owner = OwnerId::parse("tiler-windows").expect("owner");
        let generation = GenerationId::parse("aa").expect("generation");
        engine_a.sync_binding(&owner, &generation);
        for (correlation, rows, focused) in [
            ("seed-1", vec![(w1.clone(), wide())], "w1"),
            (
                "seed-2",
                vec![(w1.clone(), wide()), (w2.clone(), wide())],
                "w2",
            ),
            (
                "seed-3",
                vec![
                    (w1.clone(), wide()),
                    (w2.clone(), wide()),
                    (w3.clone(), tall()),
                ],
                "w3",
            ),
        ] {
            let reply = reconcile(
                &mut engine_a,
                &owner,
                &generation,
                &domain,
                correlation,
                &rows,
                focused,
            );
            assert!(
                matches!(
                    reply,
                    CoreReply::Projection(_) | CoreReply::Tiled(_) | CoreReply::SendWorkspace(_)
                ),
                "seed {correlation} converges, got {reply:?}"
            );
        }
        assert_nested_h_v(&tree_of(&engine_a, &domain));
        let reply = engine_a.handle(&float_event(
            &owner,
            &generation,
            "float-a",
            revision_of(&engine_a, &domain),
            &domain,
            &[
                (w1.clone(), wide(), false),
                (w2.clone(), wide(), false),
                (w3.clone(), tall(), false),
            ],
            "w3",
            "w3",
            None,
        ));
        let CoreReply::Tiled(float_plan) = reply else {
            panic!("float commits, got {reply:?}");
        };
        let live = float_plan.float_rect.expect("float places");
        let reply = engine_a.handle(&float_event(
            &owner,
            &generation,
            "unfloat-a",
            revision_of(&engine_a, &domain),
            &domain,
            &[
                (w1.clone(), wide(), false),
                (w2.clone(), wide(), false),
                (w3.clone(), live, true),
            ],
            "w3",
            "w3",
            Some(live),
        ));
        let CoreReply::Tiled(unfloat_plan) = reply else {
            panic!("unfloat commits, got {reply:?}");
        };
        let admitted_a = unfloat_plan
            .geometry
            .iter()
            .find(|g| g.window.0 == "w3")
            .expect("admission places w3")
            .rect;
        let tree_a = tree_of(&engine_a, &domain);
        assert_nested_h_v(&tree_a);
        // Engine B: seed H[w1 w2], then freshly admit w3 with the same frame.
        let mut engine_b = tiler_core::engine::Engine::new();
        engine_b.sync_binding(&owner, &generation);
        for (correlation, rows, focused) in [
            ("seed-1", vec![(w1.clone(), wide())], "w1"),
            (
                "seed-2",
                vec![(w1.clone(), wide()), (w2.clone(), wide())],
                "w2",
            ),
        ] {
            let reply = reconcile(
                &mut engine_b,
                &owner,
                &generation,
                &domain,
                correlation,
                &rows,
                focused,
            );
            assert!(
                matches!(
                    reply,
                    CoreReply::Projection(_) | CoreReply::Tiled(_) | CoreReply::SendWorkspace(_)
                ),
                "seed {correlation} converges, got {reply:?}"
            );
        }
        let reply = reconcile(
            &mut engine_b,
            &owner,
            &generation,
            &domain,
            "admit-b",
            &[
                (w1.clone(), wide()),
                (w2.clone(), wide()),
                (w3.clone(), live),
            ],
            "w3",
        );
        assert!(
            matches!(
                reply,
                CoreReply::Projection(_) | CoreReply::Tiled(_) | CoreReply::SendWorkspace(_)
            ),
            "fresh admission converges, got {reply:?}"
        );
        let tree_b = tree_of(&engine_b, &domain);
        assert!(
            same_topology(&tree_a, &tree_b),
            "unfloat reuses the fresh admission axis: {tree_a:?} vs {tree_b:?}"
        );
        // Admitted geometry matches the same fresh admission: compare via the
        // converged session projection rather than the old slot.
        let geometry_b: Vec<(String, Rect)> = match reply {
            CoreReply::Projection(plan) => plan
                .geometry
                .iter()
                .map(|g| (g.window.0.clone(), g.rect))
                .collect(),
            CoreReply::Tiled(plan) => plan
                .geometry
                .iter()
                .map(|g| (g.window.0.clone(), g.rect))
                .collect(),
            CoreReply::SendWorkspace(plan) => plan
                .geometry
                .iter()
                .map(|g| (g.window.0.clone(), g.rect))
                .collect(),
            other => panic!("fresh admission converges, got {other:?}"),
        };
        let admitted_b = geometry_b
            .iter()
            .find(|(token, _)| token == "w3")
            .expect("fresh admission places w3")
            .1;
        assert_eq!(admitted_a, admitted_b, "unfloat places like a new window");
    }

    #[test]
    fn output_context_prefers_cached_then_foreground_then_order() {
        let mut spaces = ManagedWorkspaces::new();
        spaces.ensure_output("mon-a");
        spaces.ensure_output("mon-b");
        assert_eq!(
            output_context(&spaces, Some("mon-b"), Some("mon-a"), Some("mon-a")).as_deref(),
            Some("mon-b")
        );
        assert_eq!(
            output_context(&spaces, Some("gone"), Some("mon-a"), Some("mon-b")).as_deref(),
            Some("mon-a")
        );
        assert_eq!(
            output_context(&spaces, Some("gone"), None, Some("mon-b")).as_deref(),
            Some("mon-b")
        );
        assert_eq!(
            output_context(&spaces, None, None, None).as_deref(),
            Some("mon-a")
        );
        let empty = ManagedWorkspaces::new();
        assert_eq!(output_context(&empty, None, None, None), None);
    }

    #[test]
    fn send_event_binds_target_and_focused_mover() {
        let owner = OwnerId::parse("tiler-windows").expect("owner");
        let generation = GenerationId::parse("aa").expect("generation");
        let correlation = CorrelationId::parse("tick-1").expect("correlation");
        let bounds = rect(0, 0);
        let source = workspace_domain("mon-a", "ws-1", bounds, 8);
        let target = workspace_domain("mon-a", "ws-2", bounds, 8);
        let source_rows = vec![OwnerRow {
            token: "w1".to_owned(),
            rect: bounds,
            hints: tiler_core::size_hints::WindowSizeHints::none(),
            floating: false,
        }];
        let target_rows = vec![OwnerRow {
            token: "w2".to_owned(),
            rect: bounds,
            hints: tiler_core::size_hints::WindowSizeHints::none(),
            floating: false,
        }];
        let mut event = build_send_event(
            &owner,
            &generation,
            &correlation,
            0,
            1,
            source,
            target.clone(),
            &source_rows,
            &target_rows,
            "w1",
            8,
            true,
        )
        .expect("event");
        stamp_send_target(&mut event, &target.1);
        match &event.command {
            CoreCommand::SendToWorkspace {
                window,
                target_output,
                target_workspace,
                follow: true,
            } => {
                assert_eq!(window, "w1");
                assert_eq!(target_output, "mon-a");
                assert_eq!(target_workspace, "ws-2");
            }
            _ => panic!("expected send-to-workspace"),
        }
        assert_eq!(event.focused_window.0, "w1");
        assert_eq!(event.windows.len(), 1);
        assert_eq!(event.target_windows.len(), 1);
        assert!(event.target_domain.is_some());
    }

    #[test]
    fn membership_transfer_rejects_noop_and_foreign() {
        let mover = key(7);
        let other = key(8);
        let source: BTreeSet<WindowKey> = [other.clone()].into_iter().collect();
        let target: BTreeSet<WindowKey> = [mover.clone(), other.clone()].into_iter().collect();
        assert!(verify_membership_transfer(&mover, &source, &target));
        let source: BTreeSet<WindowKey> = [mover.clone()].into_iter().collect();
        assert!(!verify_membership_transfer(&mover, &source, &target));
        assert!(!verify_membership_transfer(
            &mover,
            &BTreeSet::new(),
            &BTreeSet::new()
        ));
    }

    #[test]
    fn send_route_splits_engine_from_native_boundary() {
        use super::{SendRoute, send_route};
        // Tiled-to-tiled keeps the shared two-domain Engine plan.
        assert_eq!(send_route(true, true), SendRoute::Engine);
        // Any floating boundary transfers native membership instead: no
        // two-domain plan, no floating-side geometry, tiled side reflows.
        assert_eq!(send_route(false, true), SendRoute::Native);
        assert_eq!(send_route(true, false), SendRoute::Native);
        assert_eq!(send_route(false, false), SendRoute::Native);
    }

    #[test]
    fn directional_refuses_both_focus_and_move_on_floating() {
        // KDE plan-adapter parity: a floating workspace runs no tile layout,
        // so directional focus refuses exactly like directional move, with
        // the matching workspace-floating vocabulary. Tiled workspaces pass.
        use super::directional_workspace_refusal;
        use crate::snapkey::SnapOp;
        assert_eq!(
            directional_workspace_refusal(SnapOp::Focus, false),
            Some("focus-refused-workspace-floating")
        );
        assert_eq!(
            directional_workspace_refusal(SnapOp::Move, false),
            Some("move-refused-workspace-floating")
        );
        assert_eq!(directional_workspace_refusal(SnapOp::Focus, true), None);
        assert_eq!(directional_workspace_refusal(SnapOp::Move, true), None);
    }

    #[test]
    fn output_at_point_resolves_pointer_output_without_adjacency() {
        use super::{DirectionalOutput, output_at_point};
        // Sideways pair with a negative-x secondary: arrangement-independent
        // containment, no adjacency derivation involved.
        let outputs = vec![
            DirectionalOutput {
                id: "primary".to_owned(),
                full: Rect {
                    x: 0,
                    y: 0,
                    w: 2560,
                    h: 1440,
                },
            },
            DirectionalOutput {
                id: "left".to_owned(),
                full: Rect {
                    x: -1920,
                    y: -200,
                    w: 1920,
                    h: 1080,
                },
            },
        ];
        assert_eq!(
            output_at_point(&outputs, 100, 100).as_deref(),
            Some("primary")
        );
        assert_eq!(output_at_point(&outputs, -100, 0).as_deref(), Some("left"));
        assert_eq!(
            output_at_point(&outputs, -1920, -200).as_deref(),
            Some("left")
        );
        // Exclusive far edges belong to no output.
        assert_eq!(output_at_point(&outputs, 2560, 100), None);
        assert_eq!(output_at_point(&outputs, 0, 1440), None);
        // Between-monitor gap and far outside resolve nowhere.
        assert_eq!(output_at_point(&outputs, -10, 2000), None);
        assert_eq!(output_at_point(&outputs, 9999, 9999), None);
        // Degenerate rectangles never claim a point.
        let degenerate = vec![DirectionalOutput {
            id: "flat".to_owned(),
            full: Rect {
                x: 0,
                y: 0,
                w: 0,
                h: 600,
            },
        }];
        assert_eq!(output_at_point(&degenerate, 0, 100), None);
        assert!(output_at_point(&[], 0, 0).is_none());
    }

    #[test]
    fn drag_drop_fences_mirror_directional_tokens_without_direction() {
        use super::{DirectionalFenceLive, DragDropFenceSnapshot, drag_drop_fences_hold};
        fn snapshot() -> DragDropFenceSnapshot {
            DragDropFenceSnapshot {
                inner_gap: 8,
                outer_gap: 8,
                source_output: "mon-a".to_owned(),
                source_workspace: "ws-1".to_owned(),
                target_output: "mon-b".to_owned(),
                target_workspace: "ws-1".to_owned(),
                source_tiled: true,
                target_tiled: true,
                overlay_maximized: false,
                overlay_fullscreen: false,
                source_full: rect(0, 0),
                target_full: rect(-1920, 0),
                source_bounds: rect(0, 0),
                target_bounds: rect(-1920, 0),
                owner: "owner".to_owned(),
                generation: "gen".to_owned(),
            }
        }
        fn live() -> DirectionalFenceLive {
            DirectionalFenceLive {
                inner_gap: 8,
                outer_gap: 8,
                source_tiled: true,
                target_tiled: true,
                source_is_active: true,
                target_is_active: true,
                topology_ok: true,
                scope_ok: true,
                lifetime_ok: true,
                membership_ok: true,
                overlay_maximized: false,
                overlay_fullscreen: false,
                owner_ok: true,
            }
        }
        let snap = snapshot();
        assert_eq!(drag_drop_fences_hold(&snap, &live()), None);
        // Each fence maps to its directional token; view drift (the
        // destination current workspace changed under the pointer) refuses
        // instead of dropping into a stale layout.
        let mut view = live();
        view.target_is_active = false;
        assert_eq!(drag_drop_fences_hold(&snap, &view), Some("view-changed"));
        let mut topology = live();
        topology.topology_ok = false;
        assert_eq!(
            drag_drop_fences_hold(&snap, &topology),
            Some("topology-changed")
        );
        let mut lifetime = live();
        lifetime.lifetime_ok = false;
        assert_eq!(
            drag_drop_fences_hold(&snap, &lifetime),
            Some("identity-changed")
        );
        let mut membership = live();
        membership.membership_ok = false;
        assert_eq!(
            drag_drop_fences_hold(&snap, &membership),
            Some("origin-vanished")
        );
        let mut mode = live();
        mode.target_tiled = false;
        assert_eq!(
            drag_drop_fences_hold(&snap, &mode),
            Some("workspace-floating")
        );
        let mut overlay = live();
        overlay.overlay_fullscreen = true;
        assert_eq!(
            drag_drop_fences_hold(&snap, &overlay),
            Some("overlay-changed")
        );
        // A fullscreen that was already present at snapshot time vetoes.
        let mut snap_full = snapshot();
        snap_full.overlay_fullscreen = true;
        assert_eq!(
            drag_drop_fences_hold(&snap_full, &overlay),
            Some("move-refused-fullscreen")
        );
        let mut owner = live();
        owner.owner_ok = false;
        assert_eq!(drag_drop_fences_hold(&snap, &owner), Some("owner-changed"));
    }

    #[test]
    fn float_carry_survives_a_fresh_engine_session() {
        // After a workspace-mode release the Engine session carries no
        // exception, and a boundary-send target session has never seen the
        // mover: the lifetime-keyed intent still rides the token floating,
        // while tiled members and recycled lifetimes stay out.
        use super::float_carry_tokens;
        use std::collections::{BTreeMap, BTreeSet};
        let floated = key(11);
        let tiled = key(12);
        let members: BTreeSet<WindowKey> = [floated.clone(), tiled.clone()].into_iter().collect();
        let mut token_of: BTreeMap<WindowKey, String> = BTreeMap::new();
        token_of.insert(floated.clone(), "w-float".to_owned());
        token_of.insert(tiled.clone(), "w-tiled".to_owned());
        let intent: BTreeSet<WindowKey> = [floated.clone()].into_iter().collect();
        // Empty Engine set (fresh after release): intent still carries.
        let carried = float_carry_tokens(&members, &token_of, &intent);
        assert_eq!(carried, ["w-float".to_owned()].into_iter().collect());
        // A recycled HWND (same number, fresh lifetime) never inherits.
        let recycled = WindowKey {
            hwnd: 11,
            pid: 9000,
            creation: "c000000000009000".to_owned(),
        };
        let members: BTreeSet<WindowKey> = [recycled.clone(), tiled.clone()].into_iter().collect();
        let mut token_of: BTreeMap<WindowKey, String> = BTreeMap::new();
        token_of.insert(recycled, "w-new".to_owned());
        token_of.insert(tiled, "w-tiled".to_owned());
        assert!(float_carry_tokens(&members, &token_of, &intent).is_empty());
        // Actual Engine sequence: per-window float, release the exact domain,
        // then a fresh reconcile carrying the intent. The floated window stays
        // a slotless exception with no geometry target; the sibling reflows.
        use tiler_core::boundary::{CoreCommand, CoreReply};
        use tiler_core::directional::WindowId;
        let mut engine = tiler_core::engine::Engine::new();
        let owner = OwnerId::parse("tiler-windows").expect("owner");
        let generation = GenerationId::parse("aa").expect("generation");
        engine.sync_binding(&owner, &generation);
        let bounds = rect(0, 0);
        let domain = workspace_domain("mon-a", "ws-1", bounds, 8);
        seed_two_tiled(&mut engine, &owner, &generation, &domain, bounds);
        let float = engine.handle(&float_event(
            &owner,
            &generation,
            "float-carry-1",
            revision_of(&engine, &domain),
            &domain,
            &[
                (WindowId("w1".to_owned()), bounds, false),
                (WindowId("w2".to_owned()), bounds, false),
            ],
            "w1",
            "w1",
            None,
        ));
        let CoreReply::Tiled(float_plan) = float else {
            panic!("float commits, got {float:?}");
        };
        assert!(
            !float_plan.geometry.iter().any(|g| g.window.0 == "w1"),
            "floated window leaves the tree"
        );
        assert!(
            engine
                .session(&domain.1)
                .is_some_and(|s| s.is_exception(&WindowId("w1".to_owned()))),
            "floated window rides as an exception"
        );
        let mut release = crate::tiling::build_reconcile_event_for(
            &owner,
            &generation,
            &CorrelationId::parse("float-carry-release").expect("correlation"),
            revision_of(&engine, &domain),
            2,
            &domain.0,
            &domain.1,
            8,
            &[
                (
                    WindowId("w1".to_owned()),
                    bounds,
                    tiler_core::size_hints::WindowSizeHints::none(),
                ),
                (
                    WindowId("w2".to_owned()),
                    bounds,
                    tiler_core::size_hints::WindowSizeHints::none(),
                ),
            ],
            Some(&WindowId("w2".to_owned())),
        );
        release.command = CoreCommand::ReleaseDomain;
        match engine.handle(&release) {
            CoreReply::Released => {}
            other => panic!("release drops the exact domain, got {other:?}"),
        }
        assert!(!engine.contains(&domain.1));
        // The intent outlives the released session: the fresh observation
        // still carries the floated token while the tiled sibling stays out.
        let carry_keys: BTreeSet<WindowKey> = [key(11), key(12)].into_iter().collect();
        let carry_tokens: BTreeMap<WindowKey, String> =
            [(key(11), "w1".to_owned()), (key(12), "w2".to_owned())]
                .into_iter()
                .collect();
        let carry_intent: BTreeSet<WindowKey> = [key(11)].into_iter().collect();
        let carried = float_carry_tokens(&carry_keys, &carry_tokens, &carry_intent);
        assert_eq!(carried, ["w1".to_owned()].into_iter().collect());
        let carried_rows: Vec<(
            WindowId,
            Rect,
            tiler_core::size_hints::WindowSizeHints,
            bool,
        )> = ["w1", "w2"]
            .iter()
            .map(|token| {
                (
                    WindowId((*token).to_owned()),
                    bounds,
                    tiler_core::size_hints::WindowSizeHints::none(),
                    carried.contains(*token),
                )
            })
            .collect();
        let fp = crate::tiling::fingerprint(
            &carried_rows
                .iter()
                .map(|(token, rect, _, _)| (token.0.clone(), *rect))
                .collect::<Vec<_>>(),
        );
        let fresh = crate::tiling::build_reconcile_event_for_floating(
            &owner,
            &generation,
            &CorrelationId::parse("float-carry-fresh").expect("correlation"),
            0,
            fp,
            &domain.0,
            &domain.1,
            8,
            &carried_rows,
            Some(&WindowId("w2".to_owned())),
        );
        match engine.handle(&fresh) {
            CoreReply::Tiled(plan) => {
                assert!(
                    !plan.geometry.iter().any(|g| g.window.0 == "w1"),
                    "carried float takes no geometry target"
                );
                assert!(
                    plan.geometry.iter().any(|g| g.window.0 == "w2"),
                    "sibling reflows around the exception"
                );
            }
            CoreReply::Projection(plan) => {
                assert!(
                    !plan.geometry.iter().any(|g| g.window.0 == "w1"),
                    "carried float takes no geometry target"
                );
                assert!(
                    plan.geometry.iter().any(|g| g.window.0 == "w2"),
                    "sibling reflows around the exception"
                );
            }
            CoreReply::SendWorkspace(plan) => {
                assert!(
                    !plan.geometry.iter().any(|g| g.window.0 == "w1"),
                    "carried float takes no geometry target"
                );
                assert!(
                    plan.geometry.iter().any(|g| g.window.0 == "w2"),
                    "sibling reflows around the exception"
                );
            }
            other => panic!("fresh reconcile converges, got {other:?}"),
        }
        assert!(
            engine
                .session(&domain.1)
                .is_some_and(|s| s.is_exception(&WindowId("w1".to_owned()))),
            "exception survives the release through the carried intent"
        );
    }

    #[test]
    fn native_boundary_source_reconcile_drops_mover_and_reflows_survivor() {
        // Native-boundary sends transfer membership with no Engine plan and no
        // floating-side geometry: the runtime source phase is a complete
        // reconcile carrying only the survivors. The shared Engine must drop
        // the mover and reflow the survivor, never retain the stale member.
        use tiler_core::boundary::CoreReply;
        use tiler_core::directional::WindowId;
        let mut engine = tiler_core::engine::Engine::new();
        let owner = OwnerId::parse("tiler-windows").expect("owner");
        let generation = GenerationId::parse("aa").expect("generation");
        engine.sync_binding(&owner, &generation);
        let bounds = rect(0, 0);
        let source = workspace_domain("mon-a", "ws-1", bounds, 8);
        seed_two_tiled(&mut engine, &owner, &generation, &source, bounds);
        // Native transfer of w1 out: no send plan, source membership is now
        // exactly the survivor.
        let correlation = CorrelationId::parse("native-source-1").expect("correlation");
        let event = crate::tiling::build_reconcile_event_for(
            &owner,
            &generation,
            &correlation,
            revision_of(&engine, &source),
            1,
            &source.0,
            &source.1,
            8,
            &[(
                WindowId("w2".to_owned()),
                bounds,
                tiler_core::size_hints::WindowSizeHints::none(),
            )],
            Some(&WindowId("w2".to_owned())),
        );
        match engine.handle(&event) {
            CoreReply::Tiled(plan) => {
                assert!(
                    !plan.geometry.iter().any(|g| g.window.0 == "w1"),
                    "dropped mover takes no source geometry"
                );
                let survivor = plan
                    .geometry
                    .iter()
                    .find(|g| g.window.0 == "w2")
                    .expect("survivor reflows");
                assert_eq!(survivor.workspace.0, "ws-1");
                assert!(
                    survivor.rect.w > 0 && survivor.rect.h > 0,
                    "survivor reflows, got {:?}",
                    survivor.rect
                );
            }
            CoreReply::Projection(plan) => {
                assert!(
                    !plan.geometry.iter().any(|g| g.window.0 == "w1"),
                    "dropped mover takes no source geometry"
                );
                let survivor = plan
                    .geometry
                    .iter()
                    .find(|g| g.window.0 == "w2")
                    .expect("survivor reflows");
                assert_eq!(survivor.workspace.0, "ws-1");
                assert!(
                    survivor.rect.w > 0 && survivor.rect.h > 0,
                    "survivor reflows, got {:?}",
                    survivor.rect
                );
            }
            CoreReply::SendWorkspace(plan) => {
                assert!(
                    !plan.geometry.iter().any(|g| g.window.0 == "w1"),
                    "dropped mover takes no source geometry"
                );
                assert!(
                    plan.geometry.iter().any(|g| g.window.0 == "w2"),
                    "survivor reflows"
                );
            }
            other => panic!("source survivor converges, got {other:?}"),
        }
    }

    #[test]
    fn dispatch_gate_covers_off_suspended_unmanaged() {
        use super::dispatch_decision;
        assert_eq!(
            dispatch_decision(false, false, false, false),
            Some("takeover-off")
        );
        assert_eq!(
            dispatch_decision(true, true, false, false),
            Some("suspended")
        );
        assert_eq!(
            dispatch_decision(true, false, true, false),
            Some("unmanaged")
        );
        // Select serves empty workspaces and unmanaged foreground.
        assert_eq!(dispatch_decision(true, false, false, false), None);
        // Send with a managed focus dispatches.
        assert_eq!(dispatch_decision(true, false, true, true), None);
    }

    #[test]
    fn domain_rows_union_visible_retained_hidden() {
        use super::{MemberView, domain_rows, member_matches};
        let a = key(1);
        let b = key(2);
        let c = key(3);
        let members: BTreeSet<WindowKey> = [a.clone(), b.clone(), c.clone()].into_iter().collect();
        // Visible, retained-minimized (last-known frame), hidden snapshot.
        let views = vec![
            MemberView {
                key: a.clone(),
                token: "w1".to_owned(),
                rect: rect(0, 0),
                hints: tiler_core::size_hints::WindowSizeHints::none(),
                floating: false,
            },
            MemberView {
                key: b.clone(),
                token: "w2".to_owned(),
                rect: rect(100, 100),
                hints: tiler_core::size_hints::WindowSizeHints::none(),
                floating: false,
            },
            MemberView {
                key: c.clone(),
                token: "w3".to_owned(),
                rect: rect(200, 200),
                hints: tiler_core::size_hints::WindowSizeHints::none(),
                floating: false,
            },
        ];
        let rows = domain_rows(&members, &views).expect("complete rows");
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].token, "w1");
        // A member without any view defers instead of a falsely complete
        // observation: the caller retains Engine state with no layout loss.
        assert_eq!(domain_rows(&members, &views[..2]), None);
        // Recycled HWND (same hwnd, fresh creation) never matches.
        assert!(member_matches(&a, 1, 1001, "c0000000000000001"));
        assert!(!member_matches(&a, 1, 1001, "c0000000000000002"));
        assert!(!member_matches(&a, 1, 9999, "c0000000000000001"));
        assert!(!member_matches(&a, 9, 1001, "c0000000000000001"));
    }

    #[test]
    fn domain_rows_and_send_carry_born_floating_only() {
        // Slotless born holds ride the Engine as floating exceptions while
        // the tiled member stays tiled; the flag is an Engine observation,
        // never a native write, and send carries it into both domains.
        use super::{MemberView, build_send_event, domain_rows};
        let born = key(11);
        let tiled = key(12);
        let members: BTreeSet<WindowKey> = [born.clone(), tiled.clone()].into_iter().collect();
        let views = vec![
            MemberView {
                key: born.clone(),
                token: "w-born".to_owned(),
                rect: rect(0, 0),
                hints: tiler_core::size_hints::WindowSizeHints::none(),
                floating: true,
            },
            MemberView {
                key: tiled.clone(),
                token: "w-tiled".to_owned(),
                rect: rect(100, 100),
                hints: tiler_core::size_hints::WindowSizeHints::none(),
                floating: false,
            },
        ];
        let rows = domain_rows(&members, &views).expect("complete rows");
        assert!(
            rows.iter()
                .find(|r| r.token == "w-born")
                .expect("born")
                .floating
        );
        assert!(
            !rows
                .iter()
                .find(|r| r.token == "w-tiled")
                .expect("tiled")
                .floating
        );
        let owner = OwnerId::parse("tiler-windows").expect("owner");
        let generation = GenerationId::parse("aa").expect("generation");
        let correlation = CorrelationId::parse("tick-1").expect("correlation");
        let bounds = rect(0, 0);
        let source = workspace_domain("mon-a", "ws-1", bounds, 8);
        let target = workspace_domain("mon-a", "ws-2", bounds, 8);
        let event = build_send_event(
            &owner,
            &generation,
            &correlation,
            0,
            3,
            source,
            target,
            &rows,
            &[],
            "w-tiled",
            8,
            true,
        )
        .expect("event");
        assert!(
            event
                .windows
                .iter()
                .find(|w| w.window.0 == "w-born")
                .expect("born")
                .floating
        );
        assert!(
            !event
                .windows
                .iter()
                .find(|w| w.window.0 == "w-tiled")
                .expect("tiled")
                .floating
        );
    }

    #[test]
    fn reused_hwnd_repair_drops_only_unclaimed_stale() {
        use super::reused_hwnd_stale;
        use std::collections::BTreeSet;
        let stale = key(1);
        // Same HWND, fresh process: new key plus an unrelated member.
        let fresh = WindowKey {
            hwnd: 1,
            pid: 9000,
            creation: "c000000000009000".to_owned(),
        };
        let other = key(2);
        let known = vec![stale.clone(), fresh.clone(), other.clone()];
        // No claims: exactly the stale key drops.
        assert_eq!(
            reused_hwnd_stale(&known, &BTreeSet::new(), &fresh),
            vec![stale.clone()]
        );
        // A live hidden claim for the stale key owns its HWND through the
        // ledger/audit path: never dropped here.
        let claimed: BTreeSet<WindowKey> = [stale.clone()].into_iter().collect();
        assert!(reused_hwnd_stale(&known, &claimed, &fresh).is_empty());
        // An unknown HWND repairs nothing.
        let unknown = WindowKey {
            hwnd: 99,
            pid: 9000,
            creation: "c000000000009000".to_owned(),
        };
        assert!(reused_hwnd_stale(&known, &BTreeSet::new(), &unknown).is_empty());
    }

    #[test]
    fn visible_lifetime_needs_exact_stored_tag() {
        use super::visible_lifetime_ok;
        // Exact match is the only pass: same-process reuse starts without the
        // property (absent) or with the previous generation's tag.
        assert!(visible_lifetime_ok("abc123", Some("abc123")));
        assert!(!visible_lifetime_ok("abc123", Some("abc124")));
        assert!(!visible_lifetime_ok("abc123", None));
        // An untracked window (no stored tag) is never trusted, only stamped.
        assert!(!visible_lifetime_ok("", Some("abc123")));
        assert!(!visible_lifetime_ok("", None));
    }

    #[test]
    fn same_process_reuse_invisible_to_key_guards_but_not_to_tag() {
        use super::{member_matches, reused_hwnd_stale, visible_lifetime_ok};
        use std::collections::BTreeSet;
        // Destroy plus same-process recreate: HWND, PID, and creation all
        // still agree, so every (hwnd, pid, creation) guard passes and the
        // cross-process repair sees no stale key. Only the visible lifetime
        // tag distinguishes the new generation.
        let previous = key(1);
        assert!(member_matches(&previous, 1, 1001, "c0000000000000001"));
        let known = vec![previous.clone()];
        assert!(reused_hwnd_stale(&known, &BTreeSet::new(), &previous).is_empty());
        // Previous generation's tag stored; the fresh window carries none, so
        // no write, hide, focus, or Engine row is authorized for it.
        assert!(!visible_lifetime_ok("9f2c41aa07bd33e0", None));
        assert!(!visible_lifetime_ok(
            "9f2c41aa07bd33e0",
            Some("9f2c41aa07bd33e1")
        ));
        assert!(visible_lifetime_ok(
            "9f2c41aa07bd33e0",
            Some("9f2c41aa07bd33e0")
        ));
    }

    #[test]
    fn unified_domain_reuses_session_across_ticks() {
        // Same member, same domain, two ticks: one retained session, no
        // reseed, revision advances monotonically.
        let mut engine = tiler_core::engine::Engine::new();
        let owner = OwnerId::parse("tiler-windows").expect("owner");
        let generation = GenerationId::parse("aa").expect("generation");
        engine.sync_binding(&owner, &generation);
        let bounds = rect(0, 0);
        let (domain, key) = workspace_domain("mon-a", "ws-1", bounds, 8);
        for tick in 1..=2u64 {
            let correlation = CorrelationId::parse(&format!("tick-{tick}")).expect("correlation");
            let revision = engine
                .session(&key)
                .map(|s| s.accepted_revision())
                .unwrap_or(0);
            let event = crate::tiling::build_reconcile_event_for(
                &owner,
                &generation,
                &correlation,
                revision,
                tick,
                &domain,
                &key,
                8,
                &[(
                    tiler_core::directional::WindowId("w1".to_owned()),
                    bounds,
                    tiler_core::size_hints::WindowSizeHints::none(),
                )],
                None,
            );
            let _ = engine.handle(&event);
        }
        assert_eq!(engine.len(), 1);
    }

    #[test]
    fn engine_topology_reuses_both_domains() {
        // Both source and target rows (including hidden snapshots) reach the
        // Engine; the planned mutation is topology reuse, never remove/reseed.
        use tiler_core::boundary::CoreReply;
        let mut engine = tiler_core::engine::Engine::new();
        let owner = OwnerId::parse("tiler-windows").expect("owner");
        let generation = GenerationId::parse("aa").expect("generation");
        engine.sync_binding(&owner, &generation);
        let bounds = rect(0, 0);
        let source = workspace_domain("mon-a", "ws-1", bounds, 8);
        let target = workspace_domain("mon-a", "ws-2", bounds, 8);
        // Real converge first: source holds two windows, target holds none.
        for (key, rows) in [
            (&source.1, vec![("w1", bounds), ("w2", bounds)]),
            (&target.1, vec![]),
        ] {
            let domain = if key.workspace.0 == "ws-1" {
                &source.0
            } else {
                &target.0
            };
            let correlation = CorrelationId::parse("seed").expect("correlation");
            let event = crate::tiling::build_reconcile_event_for(
                &owner,
                &generation,
                &correlation,
                0,
                rows.len() as u64,
                domain,
                key,
                8,
                &rows
                    .iter()
                    .map(|(t, r)| {
                        (
                            tiler_core::directional::WindowId((*t).to_owned()),
                            *r,
                            tiler_core::size_hints::WindowSizeHints::none(),
                        )
                    })
                    .collect::<Vec<_>>(),
                None,
            );
            let _ = engine.handle(&event);
        }
        let revision = engine
            .session(&source.1)
            .map(|s| s.accepted_revision())
            .unwrap_or(0);
        let correlation = CorrelationId::parse("tick-1").expect("correlation");
        let mut event = build_send_event(
            &owner,
            &generation,
            &correlation,
            revision,
            42,
            source.clone(),
            target.clone(),
            &[
                OwnerRow {
                    token: "w1".to_owned(),
                    rect: bounds,
                    hints: tiler_core::size_hints::WindowSizeHints::none(),
                    floating: false,
                },
                OwnerRow {
                    token: "w2".to_owned(),
                    rect: bounds,
                    hints: tiler_core::size_hints::WindowSizeHints::none(),
                    floating: false,
                },
            ],
            &[],
            "w1",
            8,
            true,
        )
        .expect("event");
        stamp_send_target(&mut event, &target.1);
        // Typed success: the Engine moved the tile with desired geometry,
        // and both domain sessions exist after the send.
        let reply = engine.handle(&event);
        let CoreReply::SendWorkspace(plan) = reply else {
            panic!("expected SendWorkspace, got {reply:?}");
        };
        assert!(!plan.geometry.is_empty(), "send carries geometry");
        assert!(
            matches!(
                plan.operation,
                tiler_core::contract::LifecycleOperation::MoveTiled { .. }
            ),
            "send is a tiled move, got {:?}",
            plan.operation
        );
        assert!(engine.session(&source.1).is_some(), "source retained");
        assert!(engine.session(&target.1).is_some(), "target retained");
        // Retained-tree roundtrip with a window left behind: the source still
        // converges its remaining window and the target converges the mover.
        for (key, domain, rows) in [
            (&source.1, &event.domain, vec![("w2", bounds)]),
            (
                &target.1,
                event
                    .target_domain
                    .as_ref()
                    .map(|(d, _)| d)
                    .expect("target"),
                vec![("w1", bounds)],
            ),
        ] {
            let correlation = CorrelationId::parse("roundtrip").expect("correlation");
            let event = crate::tiling::build_reconcile_event_for(
                &owner,
                &generation,
                &correlation,
                engine
                    .session(key)
                    .map(|s| s.accepted_revision())
                    .unwrap_or(0),
                rows.len() as u64,
                domain,
                key,
                engine.outer_gap(key).unwrap_or(8),
                &rows
                    .iter()
                    .map(|(t, r)| {
                        (
                            tiler_core::directional::WindowId((*t).to_owned()),
                            *r,
                            tiler_core::size_hints::WindowSizeHints::none(),
                        )
                    })
                    .collect::<Vec<_>>(),
                None,
            );
            let reply = engine.handle(&event);
            assert!(
                matches!(
                    reply,
                    CoreReply::Projection(_) | CoreReply::Tiled(_) | CoreReply::SendWorkspace(_)
                ),
                "roundtrip converges, got {reply:?}"
            );
        }
    }

    #[test]
    fn send_to_empty_then_product_reconcile_with_fixed_gap() {
        // Send to an empty workspace, then reconcile both sides with the
        // same carried outer gap: the destination must place the mover and
        // the source must reflow its survivor, never `domain-mismatch`.
        use tiler_core::boundary::CoreReply;
        use tiler_core::geometry::Rect;
        fn contains(bounds: Rect, r: Rect) -> bool {
            r.w > 0
                && r.h > 0
                && r.x >= bounds.x
                && r.y >= bounds.y
                && r.x + r.w <= bounds.x + bounds.w
                && r.y + r.h <= bounds.y + bounds.h
        }
        let mut engine = tiler_core::engine::Engine::new();
        let owner = OwnerId::parse("tiler-windows").expect("owner");
        let generation = GenerationId::parse("aa").expect("generation");
        engine.sync_binding(&owner, &generation);
        let bounds = rect(0, 0);
        let source = workspace_domain("mon-a", "ws-1", bounds, 8);
        let target = workspace_domain("mon-a", "ws-2", bounds, 8);
        for (key, rows) in [
            (&source.1, vec![("w1", bounds), ("w2", bounds)]),
            (&target.1, vec![]),
        ] {
            let domain = if key.workspace.0 == "ws-1" {
                &source.0
            } else {
                &target.0
            };
            let correlation = CorrelationId::parse("seed").expect("correlation");
            let event = crate::tiling::build_reconcile_event_for(
                &owner,
                &generation,
                &correlation,
                0,
                rows.len() as u64,
                domain,
                key,
                8,
                &rows
                    .iter()
                    .map(|(t, r)| {
                        (
                            tiler_core::directional::WindowId((*t).to_owned()),
                            *r,
                            tiler_core::size_hints::WindowSizeHints::none(),
                        )
                    })
                    .collect::<Vec<_>>(),
                None,
            );
            let _ = engine.handle(&event);
        }
        let revision = engine
            .session(&source.1)
            .map(|s| s.accepted_revision())
            .unwrap_or(0);
        let correlation = CorrelationId::parse("tick-1").expect("correlation");
        let mut event = build_send_event(
            &owner,
            &generation,
            &correlation,
            revision,
            42,
            source.clone(),
            target.clone(),
            &[
                OwnerRow {
                    token: "w1".to_owned(),
                    rect: bounds,
                    hints: tiler_core::size_hints::WindowSizeHints::none(),
                    floating: false,
                },
                OwnerRow {
                    token: "w2".to_owned(),
                    rect: bounds,
                    hints: tiler_core::size_hints::WindowSizeHints::none(),
                    floating: false,
                },
            ],
            &[],
            "w1",
            8,
            true,
        )
        .expect("event");
        stamp_send_target(&mut event, &target.1);
        let reply = engine.handle(&event);
        let CoreReply::SendWorkspace(plan) = reply else {
            panic!("send commits, got {reply:?}");
        };
        assert!(
            matches!(
                plan.operation,
                tiler_core::contract::LifecycleOperation::MoveTiled { .. }
            ),
            "send is a tiled move, got {:?}",
            plan.operation
        );
        let mover = plan
            .geometry
            .iter()
            .find(|g| g.window.0 == "w1")
            .expect("send places mover");
        assert_eq!(mover.workspace.0, "ws-2");
        assert!(contains(bounds, mover.rect), "mover inside target");
        assert_eq!(
            engine.outer_gap(&target.1),
            Some(8),
            "new destination inherits carried gap"
        );
        // Destination follow-up with the same carried gap places the mover.
        let correlation = CorrelationId::parse("tick-2").expect("correlation");
        let (target_domain, target_key) = target.clone();
        let event = crate::tiling::build_reconcile_event_for(
            &owner,
            &generation,
            &correlation,
            engine
                .session(&target_key)
                .map(|s| s.accepted_revision())
                .unwrap_or(0),
            1,
            &target_domain,
            &target_key,
            8,
            &[(
                tiler_core::directional::WindowId("w1".to_owned()),
                bounds,
                tiler_core::size_hints::WindowSizeHints::none(),
            )],
            None,
        );
        match engine.handle(&event) {
            CoreReply::Tiled(plan) => {
                let placed = plan
                    .geometry
                    .iter()
                    .find(|g| g.window.0 == "w1")
                    .expect("destination places mover");
                assert_eq!(placed.workspace.0, "ws-2");
                assert!(contains(bounds, placed.rect), "mover inside target");
            }
            CoreReply::Projection(plan) => {
                let placed = plan
                    .geometry
                    .iter()
                    .find(|g| g.window.0 == "w1")
                    .expect("destination places mover");
                assert_eq!(placed.workspace.0, "ws-2");
                assert!(contains(bounds, placed.rect), "mover inside target");
            }
            CoreReply::SendWorkspace(plan) => {
                let placed = plan
                    .geometry
                    .iter()
                    .find(|g| g.window.0 == "w1")
                    .expect("destination places mover");
                assert_eq!(placed.workspace.0, "ws-2");
                assert!(contains(bounds, placed.rect), "mover inside target");
            }
            reply => panic!("destination converges, got {reply:?}"),
        }
        // Source survivor reflows with the same carried gap.
        let correlation = CorrelationId::parse("tick-3").expect("correlation");
        let (source_domain, source_key) = source.clone();
        let event = crate::tiling::build_reconcile_event_for(
            &owner,
            &generation,
            &correlation,
            engine
                .session(&source_key)
                .map(|s| s.accepted_revision())
                .unwrap_or(0),
            1,
            &source_domain,
            &source_key,
            8,
            &[(
                tiler_core::directional::WindowId("w2".to_owned()),
                bounds,
                tiler_core::size_hints::WindowSizeHints::none(),
            )],
            None,
        );
        match engine.handle(&event) {
            CoreReply::Tiled(plan) => {
                let survivor = plan
                    .geometry
                    .iter()
                    .find(|g| g.window.0 == "w2")
                    .expect("source reflows survivor");
                assert_eq!(survivor.workspace.0, "ws-1");
                assert!(contains(bounds, survivor.rect), "survivor inside source");
            }
            CoreReply::Projection(plan) => {
                let survivor = plan
                    .geometry
                    .iter()
                    .find(|g| g.window.0 == "w2")
                    .expect("source reflows survivor");
                assert_eq!(survivor.workspace.0, "ws-1");
                assert!(contains(bounds, survivor.rect), "survivor inside source");
            }
            CoreReply::SendWorkspace(plan) => {
                let survivor = plan
                    .geometry
                    .iter()
                    .find(|g| g.window.0 == "w2")
                    .expect("source reflows survivor");
                assert_eq!(survivor.workspace.0, "ws-1");
                assert!(contains(bounds, survivor.rect), "survivor inside source");
            }
            reply => panic!("source converges, got {reply:?}"),
        }
    }

    #[test]
    fn send_plan_scopes_source_reflow_through_writable_subset() {
        // Both-domain send regression through the actual native seams: one
        // Engine `SendWorkspace` plan carries source survivor reflow plus
        // target mover placement together (`planned_writes`, the extraction
        // the native write path consumes), and the domain writable set
        // (`writable_subset`, which the native `writable_tokens` delegates to)
        // scopes the pre-hide source pass to the survivor only.
        use tiler_core::boundary::CoreReply;
        let mut engine = tiler_core::engine::Engine::new();
        let owner = OwnerId::parse("tiler-windows").expect("owner");
        let generation = GenerationId::parse("aa").expect("generation");
        engine.sync_binding(&owner, &generation);
        let bounds = rect(0, 0);
        let source = workspace_domain("mon-a", "ws-1", bounds, 8);
        let target = workspace_domain("mon-a", "ws-2", bounds, 8);
        for (key, rows) in [
            (&source.1, vec![("w1", bounds), ("w2", bounds)]),
            (&target.1, vec![]),
        ] {
            let domain = if key.workspace.0 == "ws-1" {
                &source.0
            } else {
                &target.0
            };
            let correlation = CorrelationId::parse("seed").expect("correlation");
            let event = crate::tiling::build_reconcile_event_for(
                &owner,
                &generation,
                &correlation,
                0,
                rows.len() as u64,
                domain,
                key,
                8,
                &rows
                    .iter()
                    .map(|(t, r)| {
                        (
                            tiler_core::directional::WindowId((*t).to_owned()),
                            *r,
                            tiler_core::size_hints::WindowSizeHints::none(),
                        )
                    })
                    .collect::<Vec<_>>(),
                None,
            );
            let _ = engine.handle(&event);
        }
        let revision = engine
            .session(&source.1)
            .map(|s| s.accepted_revision())
            .unwrap_or(0);
        let correlation = CorrelationId::parse("tick-1").expect("correlation");
        let mut event = build_send_event(
            &owner,
            &generation,
            &correlation,
            revision,
            42,
            source.clone(),
            target.clone(),
            &[
                OwnerRow {
                    token: "w1".to_owned(),
                    rect: bounds,
                    hints: tiler_core::size_hints::WindowSizeHints::none(),
                    floating: false,
                },
                OwnerRow {
                    token: "w2".to_owned(),
                    rect: bounds,
                    hints: tiler_core::size_hints::WindowSizeHints::none(),
                    floating: false,
                },
            ],
            &[],
            "w1",
            8,
            true,
        )
        .expect("event");
        stamp_send_target(&mut event, &target.1);
        let reply = engine.handle(&event);
        let CoreReply::SendWorkspace(plan) = &reply else {
            panic!("send commits");
        };
        // Native extraction seam: the single plan covers both domains.
        let writes = super::planned_writes(&reply).expect("plan extracts");
        assert_eq!(writes.len(), plan.geometry.len(), "nothing dropped");
        assert!(writes.iter().any(|w| w.window.0 == "w1"), "mover placed");
        assert!(
            writes.iter().any(|w| w.window.0 == "w2"),
            "survivor reflowed"
        );
        // Domain eligibility seam: after the membership transfer the source
        // domain holds only the survivor, so the pre-hide source pass writes
        // exactly the survivor while the moved token is no longer writable
        // there. Hidden members never take writes.
        let mover = key(1);
        let survivor = key(2);
        let hidden = key(3);
        let members: BTreeSet<WindowKey> = [mover.clone(), survivor.clone(), hidden.clone()]
            .into_iter()
            .collect();
        let token_of: std::collections::BTreeMap<WindowKey, String> = [
            (mover.clone(), "w1".to_owned()),
            (survivor.clone(), "w2".to_owned()),
            (hidden.clone(), "w3".to_owned()),
        ]
        .into_iter()
        .collect();
        let hidden_set: BTreeSet<WindowKey> = [hidden.clone()].into_iter().collect();
        // Fresh observation holds survivor plus mover (still visible pre-hide
        // in this portable model); the source pass scopes by membership, so
        // model the post-transfer source membership explicitly.
        let source_members: BTreeSet<WindowKey> =
            [survivor.clone(), hidden.clone()].into_iter().collect();
        let fresh: std::collections::HashSet<String> =
            ["w1".to_owned(), "w2".to_owned()].into_iter().collect();
        let writable = super::writable_subset(
            &source_members,
            |k| hidden_set.contains(k),
            &token_of,
            &fresh,
        );
        assert!(writable.contains("w2"), "survivor stays writable");
        assert!(
            !writable.contains("w1"),
            "moved token not writable in source"
        );
        assert!(!writable.contains("w3"), "hidden rows never writable");
        let _ = members;
        // Non-plan replies extract to `None` so callers serialize null with
        // reason instead of zeros.
        let released = tiler_core::boundary::CoreReply::Released;
        assert!(super::planned_writes(&released).is_none());
    }

    #[test]
    fn non_plan_replies_extract_to_none() {
        use tiler_core::boundary::{CoreReply, NoGroupReason};
        use tiler_core::contract::DivergenceKind;
        for reply in [
            CoreReply::Released,
            CoreReply::Rejected {
                kind: "empty",
                message: "empty domain",
            },
            CoreReply::Diverged(DivergenceKind::StaleRevision),
            CoreReply::SnapshotInvalid {
                message: "stale",
                detail: "stale snapshot",
            },
            CoreReply::NoGroup {
                base_revision: None,
                reason: NoGroupReason::NoSession,
            },
        ] {
            assert!(super::planned_writes(&reply).is_none(), "{reply:?}");
        }
    }

    #[test]
    fn focus_before_geometry_gate_blocks_real_fullscreen_and_elevated() {
        use super::focus_before_geometry;
        // Verified transition with a clean foreground focuses before geometry.
        assert!(focus_before_geometry(true, false, false));
        // A real fullscreen or elevated arrival must never be stolen from.
        assert!(!focus_before_geometry(true, true, false));
        assert!(!focus_before_geometry(true, false, true));
        assert!(!focus_before_geometry(true, true, true));
        // An unverified transition never focuses first, even when clean.
        assert!(!focus_before_geometry(false, false, false));
    }

    #[test]
    fn writable_subset_never_includes_hidden_or_stale() {
        use super::writable_subset;
        use std::collections::BTreeMap;
        use std::collections::BTreeSet;
        let a = key(1);
        let b = key(2);
        let c = key(3);
        let members: BTreeSet<WindowKey> = [a.clone(), b.clone(), c.clone()].into_iter().collect();
        let token_of: BTreeMap<WindowKey, String> = [
            (a.clone(), "w1".to_owned()),
            (b.clone(), "w2".to_owned()),
            (c.clone(), "w3".to_owned()),
        ]
        .into_iter()
        .collect();
        // b is hidden; c has no fresh frame (retained without observation).
        let hidden: BTreeSet<WindowKey> = [b.clone()].into_iter().collect();
        let fresh: std::collections::HashSet<String> =
            ["w1".to_owned(), "w2".to_owned()].into_iter().collect();
        let writable = writable_subset(&members, |k| hidden.contains(k), &token_of, &fresh);
        assert!(
            writable.contains("w1"),
            "visible fresh member stays writable"
        );
        assert!(!writable.contains("w2"), "hidden rows never writable");
        assert!(!writable.contains("w3"), "retained rows never writable");
    }

    #[test]
    fn foreground_classifier_keeps_gates_but_fixes_desktop_and_hidden() {
        use super::{ForegroundFacts, ForegroundVetoReason, classify_foreground};
        let facts = |valid: bool,
                     is_desktop: bool,
                     visible: bool,
                     captioned: bool,
                     dwm_readable: bool,
                     cloak_readable: bool,
                     cloaked: bool,
                     covers_monitor: bool| {
            classify_foreground(ForegroundFacts {
                valid,
                is_desktop,
                visible,
                captioned,
                dwm_readable,
                cloak_readable,
                cloaked,
                covers_monitor,
            })
        };
        // Visible real fullscreen still vetoes.
        let v = facts(true, false, true, false, true, true, false, true);
        assert!(v.block && v.reason == ForegroundVetoReason::Fullscreen);
        // Visible unreadable foreground stays blocked (fail closed).
        let v = facts(true, false, true, false, false, true, false, false);
        assert!(v.block && v.reason == ForegroundVetoReason::Unreadable);
        // Unreadable cloak state stays blocked (fail closed): an unknown
        // cloak never clears a covering frame.
        let v = facts(true, false, true, false, true, false, false, true);
        assert!(v.block && v.reason == ForegroundVetoReason::Unreadable);
        // Cloaked captionless monitor cover never vetoes: invisible to the
        // compositor, so it cannot be a covering fullscreen.
        let v = facts(true, false, true, false, true, true, true, true);
        assert!(!v.block && v.reason == ForegroundVetoReason::Cloaked);
        // Cloaked small frame never vetoes either.
        let v = facts(true, false, true, false, true, true, true, false);
        assert!(!v.block && v.reason == ForegroundVetoReason::Cloaked);
        // Invalid handle fails closed.
        let v = facts(false, false, false, false, false, false, false, false);
        assert!(v.block && v.reason == ForegroundVetoReason::Invalid);
        // Exact desktop shell handle never vetoes, even when covering.
        let v = facts(true, true, true, false, true, true, false, true);
        assert!(!v.block && v.reason == ForegroundVetoReason::Desktop);
        // Valid non-visible foreground never vetoes: invisible cannot cover.
        let v = facts(true, false, false, false, true, true, false, true);
        assert!(!v.block && v.reason == ForegroundVetoReason::Nonvisible);
        // Captioned foreground never vetoes.
        let v = facts(true, false, true, true, true, true, false, false);
        assert!(!v.block && v.reason == ForegroundVetoReason::Captioned);
        // Small visible borderless window never vetoes.
        let v = facts(true, false, true, false, true, true, false, false);
        assert!(!v.block && v.reason == ForegroundVetoReason::None);
    }

    #[test]
    fn managed_overlay_reason_is_opaque_and_distinct() {
        // The managed-overlay bypass (verified member or born-held track)
        // carries its own bounded token, never the raw fullscreen one, so
        // lifecycle logs stay distinguishable. The classifier itself never
        // produces it: only the native managed check may.
        use super::ForegroundVetoReason;
        assert_eq!(
            ForegroundVetoReason::ManagedOverlay.as_str(),
            "managed-overlay"
        );
        assert_ne!(
            ForegroundVetoReason::ManagedOverlay,
            ForegroundVetoReason::Fullscreen
        );
    }

    #[test]
    fn domain_rows_propagates_hints_and_send_carries_them() {
        use super::{MemberView, build_send_event, domain_rows, stamp_send_target};
        use tiler_core::boundary::CoreReply;
        use tiler_core::size_hints::WindowSizeHints;
        let a = key(1);
        let b = key(2);
        let members: BTreeSet<WindowKey> = [a.clone(), b.clone()].into_iter().collect();
        let hinted = WindowSizeHints {
            min_w: Some(500),
            min_h: None,
            max_w: None,
            max_h: None,
        };
        let views = vec![
            MemberView {
                key: a.clone(),
                token: "w1".to_owned(),
                rect: rect(0, 0),
                hints: hinted,
                floating: false,
            },
            MemberView {
                key: b.clone(),
                token: "w2".to_owned(),
                rect: rect(100, 100),
                hints: WindowSizeHints::none(),
                floating: false,
            },
        ];
        let rows = domain_rows(&members, &views).expect("complete rows");
        assert_eq!(rows[0].hints, hinted);
        assert!(rows[1].hints.is_empty());
        // The destination domain carries its own fresh hints (a hidden
        // snapshot row included): the send path must not drop them.
        let c = key(3);
        let target_members: BTreeSet<WindowKey> = [c.clone()].into_iter().collect();
        let target_views = vec![MemberView {
            key: c.clone(),
            token: "w3".to_owned(),
            rect: rect(200, 200),
            hints: hinted,
            floating: false,
        }];
        let target_rows = domain_rows(&target_members, &target_views).expect("target rows");
        assert_eq!(target_rows[0].hints, hinted);
        // The send path carries the same rows into both Engine domains.
        let owner = OwnerId::parse("tiler-windows").expect("owner");
        let generation = GenerationId::parse("aa").expect("generation");
        let correlation = CorrelationId::parse("tick-1").expect("correlation");
        let bounds = rect(0, 0);
        let source = workspace_domain("mon-a", "ws-1", bounds, 8);
        let target = workspace_domain("mon-a", "ws-2", bounds, 8);
        let mut engine = tiler_core::engine::Engine::new();
        engine.sync_binding(&owner, &generation);
        for (key, domain, rows) in [
            (&source.1, &source.0, vec![("w1", bounds), ("w2", bounds)]),
            (&target.1, &target.0, vec![("w3", bounds)]),
        ] {
            let seed_correlation = CorrelationId::parse("seed").expect("correlation");
            let event = crate::tiling::build_reconcile_event_for(
                &owner,
                &generation,
                &seed_correlation,
                0,
                rows.len() as u64,
                domain,
                key,
                8,
                &rows
                    .iter()
                    .map(|(t, r)| {
                        (
                            tiler_core::directional::WindowId((*t).to_owned()),
                            *r,
                            WindowSizeHints::none(),
                        )
                    })
                    .collect::<Vec<_>>(),
                None,
            );
            let _ = engine.handle(&event);
        }
        let mut event = build_send_event(
            &owner,
            &generation,
            &correlation,
            engine
                .session(&source.1)
                .map(|s| s.accepted_revision())
                .unwrap_or(0),
            3,
            source,
            target.clone(),
            &rows,
            &target_rows,
            "w1",
            8,
            true,
        )
        .expect("event");
        stamp_send_target(&mut event, &target.1);
        assert_eq!(event.windows.len(), 2);
        assert_eq!(event.windows[0].hints, hinted);
        assert!(event.windows[1].hints.is_empty());
        assert_eq!(event.target_windows.len(), 1);
        assert_eq!(event.target_windows[0].hints, hinted);
        let CoreReply::SendWorkspace(_) = engine.handle(&event) else {
            panic!("hinted send commits through the real Engine");
        };
    }

    #[test]
    fn hint_only_change_reprojects_without_topology_change() {
        // Same rectangles and fingerprint, only a fresh minimum hint on w2:
        // the Engine still projects, and the deficit comes from w1's slack.
        use tiler_core::boundary::CoreReply;
        use tiler_core::geometry::Rect;
        use tiler_core::size_hints::WindowSizeHints;
        let mut engine = tiler_core::engine::Engine::new();
        let owner = OwnerId::parse("tiler-windows").expect("owner");
        let generation = GenerationId::parse("aa").expect("generation");
        engine.sync_binding(&owner, &generation);
        let bounds = Rect {
            x: 0,
            y: 0,
            w: 800,
            h: 600,
        };
        let (domain, key) = workspace_domain("mon-a", "ws-1", bounds, 8);
        let plain = || {
            vec![
                (
                    tiler_core::directional::WindowId("w1".to_owned()),
                    bounds,
                    WindowSizeHints::none(),
                ),
                (
                    tiler_core::directional::WindowId("w2".to_owned()),
                    bounds,
                    WindowSizeHints::none(),
                ),
            ]
        };
        let correlation = CorrelationId::parse("tick-1").expect("correlation");
        let seed = crate::tiling::build_reconcile_event_for(
            &owner,
            &generation,
            &correlation,
            0,
            2,
            &domain,
            &key,
            8,
            &plain(),
            None,
        );
        let base = match engine.handle(&seed) {
            CoreReply::Tiled(plan) => plan.base_revision,
            CoreReply::Projection(plan) => plan.base_revision,
            reply => panic!("seed converges, got {reply:?}"),
        };
        // Hint-only second observation: identical rectangles, one fresh hint.
        let hinted = WindowSizeHints {
            min_w: Some(500),
            min_h: None,
            max_w: None,
            max_h: None,
        };
        let correlation = CorrelationId::parse("tick-2").expect("correlation");
        let event = crate::tiling::build_reconcile_event_for(
            &owner,
            &generation,
            &correlation,
            engine
                .session(&key)
                .map(|s| s.accepted_revision())
                .unwrap_or(base),
            2,
            &domain,
            &key,
            8,
            &[
                (
                    tiler_core::directional::WindowId("w1".to_owned()),
                    bounds,
                    WindowSizeHints::none(),
                ),
                (
                    tiler_core::directional::WindowId("w2".to_owned()),
                    bounds,
                    hinted,
                ),
            ],
            None,
        );
        let reply = engine.handle(&event);
        let CoreReply::Projection(plan) = reply else {
            panic!("hint-only change projects, got {reply:?}");
        };
        assert!(plan.geometry.iter().all(|g| !g.overconstrained));
        let widths: std::collections::BTreeMap<&str, i32> = plan
            .geometry
            .iter()
            .map(|g| (g.window.0.as_str(), g.rect.w))
            .collect();
        // 800 extent, 8 inner gap: 792 shared; w2 keeps 500, w1 yields slack.
        assert_eq!(widths["w2"], 500);
        assert_eq!(widths["w1"], 792 - 500);
    }

    #[test]
    fn infeasible_hints_keep_proportional_and_flag_overconstrained() {
        // Minimums exceeding the extent keep the proportional allocation and
        // flag every violating window: shared projection never reasserts, and
        // the Windows native write path places writable overconstrained
        // windows at the tile origin clamped to the known minimum (KDE keeps
        // its existing skip).
        use tiler_core::boundary::CoreReply;
        use tiler_core::geometry::Rect;
        use tiler_core::size_hints::WindowSizeHints;
        let mut engine = tiler_core::engine::Engine::new();
        let owner = OwnerId::parse("tiler-windows").expect("owner");
        let generation = GenerationId::parse("aa").expect("generation");
        engine.sync_binding(&owner, &generation);
        let bounds = Rect {
            x: 0,
            y: 0,
            w: 800,
            h: 600,
        };
        let (domain, key) = workspace_domain("mon-a", "ws-1", bounds, 8);
        let hinted = WindowSizeHints {
            min_w: Some(500),
            min_h: None,
            max_w: None,
            max_h: None,
        };
        let rows = vec![
            (
                tiler_core::directional::WindowId("w1".to_owned()),
                bounds,
                hinted,
            ),
            (
                tiler_core::directional::WindowId("w2".to_owned()),
                bounds,
                hinted,
            ),
        ];
        let correlation = CorrelationId::parse("tick-1").expect("correlation");
        let seed = crate::tiling::build_reconcile_event_for(
            &owner,
            &generation,
            &correlation,
            0,
            2,
            &domain,
            &key,
            8,
            &rows,
            None,
        );
        let _ = engine.handle(&seed);
        let correlation = CorrelationId::parse("tick-2").expect("correlation");
        let event = crate::tiling::build_reconcile_event_for(
            &owner,
            &generation,
            &correlation,
            engine
                .session(&key)
                .map(|s| s.accepted_revision())
                .unwrap_or(0),
            2,
            &domain,
            &key,
            8,
            &rows,
            None,
        );
        let reply = engine.handle(&event);
        let widths: Vec<i32>;
        let flags: Vec<bool>;
        match reply {
            CoreReply::Projection(plan) => {
                widths = plan.geometry.iter().map(|g| g.rect.w).collect();
                flags = plan.geometry.iter().map(|g| g.overconstrained).collect();
                assert_eq!(plan.geometry.len(), 2);
            }
            CoreReply::Tiled(plan) => {
                widths = plan.geometry.iter().map(|g| g.rect.w).collect();
                flags = plan.geometry.iter().map(|g| g.overconstrained).collect();
                assert_eq!(plan.geometry.len(), 2);
            }
            reply => panic!("infeasible hints still project, got {reply:?}"),
        }
        assert!(
            flags.iter().all(|f| *f),
            "both windows flag overconstrained"
        );
        // Proportional fallback: identical to the no-hint allocation.
        let correlation = CorrelationId::parse("tick-3").expect("correlation");
        let plain: Vec<(tiler_core::directional::WindowId, Rect, WindowSizeHints)> = rows
            .iter()
            .map(|(w, r, _)| (w.clone(), *r, WindowSizeHints::none()))
            .collect();
        let event = crate::tiling::build_reconcile_event_for(
            &owner,
            &generation,
            &correlation,
            engine
                .session(&key)
                .map(|s| s.accepted_revision())
                .unwrap_or(0),
            2,
            &domain,
            &key,
            8,
            &plain,
            None,
        );
        let reply = engine.handle(&event);
        let plain_widths: Vec<i32> = match reply {
            CoreReply::Projection(plan) => plan.geometry.iter().map(|g| g.rect.w).collect(),
            CoreReply::Tiled(plan) => plan.geometry.iter().map(|g| g.rect.w).collect(),
            reply => panic!("plain projects, got {reply:?}"),
        };
        assert_eq!(widths, plain_widths, "infeasible keeps proportional sizes");
        // The native extraction preserves the flags the write path skips on:
        // rebuild the infeasible plan through the same builder and check the
        // seam carries every overconstrained flag.
        let correlation = CorrelationId::parse("tick-4").expect("correlation");
        let event = crate::tiling::build_reconcile_event_for(
            &owner,
            &generation,
            &correlation,
            engine
                .session(&key)
                .map(|s| s.accepted_revision())
                .unwrap_or(0),
            2,
            &domain,
            &key,
            8,
            &rows,
            None,
        );
        let reply = engine.handle(&event);
        let plan = match reply {
            CoreReply::Projection(plan) => plan,
            reply => panic!("infeasible re-projects, got {reply:?}"),
        };
        let writes = super::planned_writes(&tiler_core::boundary::CoreReply::Projection(plan))
            .expect("plan extracts");
        assert!(writes.iter().all(|w| w.overconstrained));
    }

    #[test]
    fn max_only_hints_clamp_without_reallocation() {
        // AR12 shared policy, no adapter copy: maximums never reallocate
        // siblings; they only bound clamp acceptance (`clamp(desired, min,
        // max)` with per-axis tolerance). A max-only window plans the
        // identical share to its unhinted control, while a client frame
        // clamped to the shared policy accepts as `client_clamped` and the
        // same shortfall with no usable max (unhinted, or sentinel which
        // the adapter normalizes to absent) counts as drift. Hints come
        // from the production outer-track conversion, never hand-built.
        use tiler_core::boundary::CoreReply;
        use tiler_core::geometry::Rect;
        use tiler_core::size_hints::{
            FixedSizePredicate, WindowSizeHints, assess_window_clamp, is_fixed_size_with,
        };
        let mut engine = tiler_core::engine::Engine::new();
        engine.set_fixed_size_admission(true);
        let owner = OwnerId::parse("tiler-windows").expect("owner");
        let generation = GenerationId::parse("aa").expect("generation");
        engine.sync_binding(&owner, &generation);
        let bounds = Rect {
            x: 0,
            y: 0,
            w: 800,
            h: 600,
        };
        let (domain, key) = workspace_domain("mon-a", "ws-1", bounds, 8);
        // Production conversion: max-only outer tracks with zero insets.
        // A seeded-untouched maximum (sentinel) normalizes to absent, and
        // min/max failure is unknown (no hint), never a floor or cap.
        let maxed = crate::tiling::hints_from_outer(
            None,
            Some((216, 316)),
            crate::tiling::FrameInsets::default(),
        );
        assert_eq!(
            (maxed.min_w, maxed.min_h, maxed.max_w, maxed.max_h),
            (None, None, Some(216), Some(316))
        );
        let sentinel = crate::tiling::hints_from_outer(
            None,
            Some((i32::MAX, i32::MAX)),
            crate::tiling::FrameInsets::default(),
        );
        assert!(
            sentinel.max_w.is_none() && sentinel.max_h.is_none(),
            "sentinel maximums normalize to absent"
        );
        assert!(
            !is_fixed_size_with(maxed, FixedSizePredicate::BothAxes)
                && !is_fixed_size_with(maxed, FixedSizePredicate::EitherAxis),
            "max-only hints never classify fixed"
        );
        let wmax = tiler_core::directional::WindowId("wmax".to_owned());
        let w1 = tiler_core::directional::WindowId("w1".to_owned());
        let rows = vec![
            (wmax.clone(), bounds, maxed),
            (w1.clone(), bounds, WindowSizeHints::none()),
        ];
        let project = |engine: &mut tiler_core::engine::Engine,
                       rows: &[(tiler_core::directional::WindowId, Rect, WindowSizeHints)],
                       fp: u64,
                       correlation: &str| {
            let correlation = CorrelationId::parse(correlation).expect("correlation");
            let event = crate::tiling::build_reconcile_event_for(
                &owner,
                &generation,
                &correlation,
                engine
                    .session(&key)
                    .map(|s| s.accepted_revision())
                    .unwrap_or(0),
                fp,
                &domain,
                &key,
                8,
                rows,
                None,
            );
            match engine.handle(&event) {
                CoreReply::Projection(plan) => plan.geometry,
                CoreReply::Tiled(plan) => plan.geometry,
                reply => panic!("max-only projects, got {reply:?}"),
            }
        };
        let fp_full =
            crate::tiling::fingerprint(&[("wmax".to_owned(), bounds), ("w1".to_owned(), bounds)]);
        let seed = project(&mut engine, &rows, fp_full, "max-tick-1");
        assert_eq!(seed.len(), 2);
        let planned = seed
            .iter()
            .find(|g| g.window == wmax)
            .expect("max window planned");
        assert!(
            !planned.overconstrained,
            "max-only plans a full share, never overconstrained"
        );
        // Retained tick with the client frame clamped to the shared policy:
        // same origin, sizes at the maximum. The share must not move and
        // the clamped member must flag `client_clamped`, not drift.
        let clamped = Rect {
            x: planned.rect.x,
            y: planned.rect.y,
            w: 216,
            h: 316,
        };
        let fp_clamped =
            crate::tiling::fingerprint(&[("wmax".to_owned(), clamped), ("w1".to_owned(), bounds)]);
        let retained = project(
            &mut engine,
            &[
                (wmax.clone(), clamped, maxed),
                (w1.clone(), bounds, WindowSizeHints::none()),
            ],
            fp_clamped,
            "max-tick-2",
        );
        let kept = retained
            .iter()
            .find(|g| g.window == wmax)
            .expect("max window retained");
        assert_eq!(
            (kept.rect.w, kept.rect.h),
            (planned.rect.w, planned.rect.h),
            "maximums never reallocate the share"
        );
        assert!(
            kept.client_clamped,
            "max-clamped client frame accepts instead of drifting"
        );
        assert!(
            retained
                .iter()
                .find(|g| g.window == w1)
                .is_some_and(|g| !g.client_clamped),
            "unclamped sibling flags nothing"
        );
        // Unhinted control plans the identical share: the maximum moved
        // nothing in either direction.
        let plain: Vec<(tiler_core::directional::WindowId, Rect, WindowSizeHints)> = rows
            .iter()
            .map(|(w, r, _)| (w.clone(), *r, WindowSizeHints::none()))
            .collect();
        let control = project(&mut engine, &plain, fp_full, "max-tick-3");
        let control_widths: Vec<i32> = control.iter().map(|g| g.rect.w).collect();
        let maxed_widths: Vec<i32> = seed.iter().map(|g| g.rect.w).collect();
        assert_eq!(
            maxed_widths, control_widths,
            "max hints plan byte-identical shares to no hints"
        );
        // Shared assessment direct: clamped accepts under max-only hints,
        // the same shortfall drifts unhinted and under sentinel-absent.
        assert!(
            assess_window_clamp(&clamped, &planned.rect, &maxed).accepted,
            "shared policy accepts the max clamp"
        );
        assert!(
            !assess_window_clamp(&clamped, &planned.rect, &WindowSizeHints::none()).accepted,
            "unhinted shortfall is drift, never accepted"
        );
        assert!(
            !assess_window_clamp(&clamped, &planned.rect, &sentinel).accepted,
            "sentinel-absent shortfall is drift, never accepted"
        );
    }

    #[test]
    fn sticky_toggle_refusals_match_float_overlays_with_sticky_vocabulary() {
        // Fullscreen wins, then maximize; normal proceeds. Sticky uses its
        // own outcome strings so logs distinguish the arm.
        assert_eq!(
            crate::tiling::sticky_toggle_refusal(true, false),
            Some("sticky-refused-fullscreen")
        );
        assert_eq!(
            crate::tiling::sticky_toggle_refusal(false, true),
            Some("sticky-refused-maximize")
        );
        assert_eq!(
            crate::tiling::sticky_toggle_refusal(true, true),
            Some("sticky-refused-fullscreen")
        );
        assert_eq!(crate::tiling::sticky_toggle_refusal(false, false), None);
        assert_eq!(
            crate::tiling::sticky_directional_refusal(true, false),
            Some("focus-refused-sticky")
        );
        assert_eq!(
            crate::tiling::sticky_directional_refusal(true, true),
            Some("move-refused-sticky")
        );
        assert_eq!(
            crate::tiling::sticky_directional_refusal(false, false),
            None
        );
        assert_eq!(crate::tiling::sticky_directional_refusal(false, true), None);
    }

    #[test]
    fn orientation_toggle_refusals_carry_orientation_vocabulary() {
        // Fullscreen wins, then maximize; normal proceeds. The native
        // orientation path calls this before any Engine mutation, so a
        // focused overlay keeps its native state and reserved slot.
        assert_eq!(
            crate::tiling::orientation_toggle_refusal(true, false),
            Some("orientation-refused-fullscreen")
        );
        assert_eq!(
            crate::tiling::orientation_toggle_refusal(false, true),
            Some("orientation-refused-maximize")
        );
        assert_eq!(
            crate::tiling::orientation_toggle_refusal(true, true),
            Some("orientation-refused-fullscreen")
        );
        assert_eq!(
            crate::tiling::orientation_toggle_refusal(false, false),
            None
        );
    }

    #[test]
    fn sticky_marker_values_round_trip_and_reject_unknown() {
        // 1 = prior tiled, 2 = prior float; zero/unknown fail closed.
        assert_eq!(crate::tiling::sticky_marker_value(false), 1);
        assert_eq!(crate::tiling::sticky_marker_value(true), 2);
        assert_eq!(crate::tiling::parse_sticky_marker(1), Some(false));
        assert_eq!(crate::tiling::parse_sticky_marker(2), Some(true));
        assert_eq!(crate::tiling::parse_sticky_marker(0), None);
        assert_eq!(crate::tiling::parse_sticky_marker(3), None);
        assert_eq!(crate::tiling::parse_sticky_marker(u64::MAX), None);
    }

    #[test]
    fn sticky_float_rehome_keeps_no_duplicate_across_domains() {
        // Backing-domain removal plus current-domain admit as tiled via two
        // reconciles: the mover leaves the source set, joins the target set,
        // and no stale duplicate remains. Mirrors the native cross-domain
        // sticky-off path without native calls.
        use tiler_core::boundary::CoreReply;
        let mut engine = tiler_core::engine::Engine::new();
        let owner = OwnerId::parse("tiler-windows").expect("owner");
        let generation = GenerationId::parse("aa").expect("generation");
        engine.sync_binding(&owner, &generation);
        let bounds = rect(0, 0);
        let source = workspace_domain("mon-a", "ws-1", bounds, 8);
        let target = workspace_domain("mon-a", "ws-2", bounds, 8);
        seed_two_tiled(&mut engine, &owner, &generation, &source, bounds);
        // Float w1 in the source (sticky-on tiled half), then rehome: source
        // without w1 converges removal, target with w1 as tiled admits.
        let float = float_event(
            &owner,
            &generation,
            "float-1",
            revision_of(&engine, &source),
            &source,
            &[
                (WindowId("w1".to_owned()), bounds, false),
                (WindowId("w2".to_owned()), bounds, false),
            ],
            "w1",
            "w1",
            None,
        );
        let reply = engine.handle(&float);
        assert!(matches!(reply, CoreReply::Tiled(_)), "float commits");
        let revision = |engine: &tiler_core::engine::Engine, domain: &(OutputDomain, DomainKey)| {
            revision_of(engine, domain)
        };
        let reconcile = |engine: &mut tiler_core::engine::Engine,
                         domain: &(OutputDomain, DomainKey),
                         correlation: &str,
                         rows: &[(WindowId, Rect, bool)]| {
            let correlation = CorrelationId::parse(correlation).expect("correlation");
            let carried: Vec<(
                WindowId,
                Rect,
                tiler_core::size_hints::WindowSizeHints,
                bool,
            )> = rows
                .iter()
                .map(|(token, rect, floating)| {
                    (
                        token.clone(),
                        *rect,
                        tiler_core::size_hints::WindowSizeHints::none(),
                        *floating,
                    )
                })
                .collect();
            let fp = crate::tiling::fingerprint(
                &rows
                    .iter()
                    .map(|(token, rect, _)| (token.0.clone(), *rect))
                    .collect::<Vec<_>>(),
            );
            let event = crate::tiling::build_reconcile_event_for_floating(
                &owner,
                &generation,
                &correlation,
                revision(engine, domain),
                fp,
                &domain.0,
                &domain.1,
                8,
                &carried,
                None,
            );
            engine.handle(&event)
        };
        // Source without the floated window: exception drops, sibling keeps
        // the tile area with no duplication.
        let reply = reconcile(
            &mut engine,
            &source,
            "prune-old",
            &[(WindowId("w2".to_owned()), bounds, false)],
        );
        assert!(
            matches!(
                reply,
                CoreReply::Projection(_) | CoreReply::Tiled(_) | CoreReply::SendWorkspace(_)
            ),
            "old removal converges, got {reply:?}"
        );
        assert!(
            !engine
                .session(&source.1)
                .is_some_and(|s| s.is_exception(&WindowId("w1".to_owned()))),
            "old exception gone"
        );
        // Current admits w1 as tiled: same topology as a fresh admission,
        // no stale retained duplicate in the source. The target plan carries
        // w1 as a tile (not an exception), so the first Win+G after the move
        // tiles rather than re-floating.
        let reply = reconcile(
            &mut engine,
            &target,
            "admit-current",
            &[(WindowId("w1".to_owned()), bounds, false)],
        );
        let tiled_windows: Vec<WindowId> = match reply {
            CoreReply::Tiled(plan) => plan.geometry.iter().map(|g| g.window.clone()).collect(),
            CoreReply::Projection(plan) => plan.geometry.iter().map(|g| g.window.clone()).collect(),
            CoreReply::SendWorkspace(plan) => {
                plan.geometry.iter().map(|g| g.window.clone()).collect()
            }
            reply => panic!("current admit converges, got {reply:?}"),
        };
        assert!(
            tiled_windows.contains(&WindowId("w1".to_owned())),
            "target tiles w1, got {tiled_windows:?}"
        );
        assert!(
            !engine
                .session(&target.1)
                .is_some_and(|s| s.is_exception(&WindowId("w1".to_owned()))),
            "w1 rides tiled in target"
        );
        assert!(
            !engine
                .session(&source.1)
                .is_some_and(|s| s.is_exception(&WindowId("w1".to_owned()))),
            "no stale duplicate in old"
        );
    }

    #[test]
    fn sticky_pruned_backing_rehomes_to_current_without_duplicate() {
        // Sticky-only backing workspace prunes while the sticky membership
        // dangles, then sticky-off rehomes to current: the old domain drops
        // the mover with no duplicate and current tiles (prior tiled) or
        // keeps the live frame as a normal float (prior float). Ties the
        // production pure seams: `plan_cleanup_excluding` /
        // `workspace_members` (missing returns empty) plus `domain_rows`
        // and the floating reconcile builder, for both prior origins.
        use tiler_core::boundary::CoreReply;
        let bounds = rect(0, 0);
        let live_frame = Rect {
            x: 10,
            y: 20,
            w: 400,
            h: 300,
        };
        // Workspace seam: sticky-only intermediate prunes, membership dangles.
        let mut spaces = ManagedWorkspaces::new();
        spaces.ensure_output("mon-a");
        let current = spaces.active_id("mon-a").expect("active");
        let backing = spaces.resolve_send("mon-a", 2).expect("second");
        let filler = key(43);
        assert!(spaces.assign(filler.clone(), "mon-a", &backing, false));
        let (third, _) = spaces.select_trailing("mon-a").expect("trailing");
        assert!(spaces.assign(filler.clone(), "mon-a", &third, false));
        let mover = key(41);
        assert!(spaces.assign(mover.clone(), "mon-a", &backing, false));
        let sibling = key(42);
        assert!(spaces.assign(sibling.clone(), "mon-a", &current, false));
        assert!(spaces.activate("mon-a", &current));
        let sticky: BTreeSet<WindowKey> = [mover.clone()].into_iter().collect();
        let (removed, append) =
            spaces.plan_cleanup_excluding("mon-a", std::slice::from_ref(&current), &[], &sticky);
        assert_eq!(removed, vec![backing.clone()], "sticky-only prunes");
        spaces.apply_cleanup("mon-a", &removed, append);
        assert_eq!(
            spaces.member_loc(&mover).map(|l| l.workspace.clone()),
            Some(backing.clone()),
            "membership dangles on the pruned id"
        );
        assert!(
            spaces.workspace_members("mon-a", &backing).is_empty(),
            "pruned members read back empty"
        );
        // Engine seam per prior origin: old domain without the mover, current
        // with the mover explicit (mirrors the native rehome construct).
        for prior in [false, true] {
            let floating = prior;
            let mut engine = tiler_core::engine::Engine::new();
            let owner = OwnerId::parse("tiler-windows").expect("owner");
            let generation = GenerationId::parse("aa").expect("generation");
            engine.sync_binding(&owner, &generation);
            let source = workspace_domain("mon-a", &backing, bounds, 8);
            let target = workspace_domain("mon-a", &current, bounds, 8);
            // Sticky-on: tiled mover floats in the backing domain.
            let seed = float_event(
                &owner,
                &generation,
                "sticky-on",
                revision_of(&engine, &source),
                &source,
                &[(WindowId("w1".to_owned()), bounds, false)],
                "w1",
                "w1",
                None,
            );
            let reply = engine.handle(&seed);
            assert!(
                matches!(reply, CoreReply::Tiled(_)),
                "prior {prior}: sticky-on floats, got {reply:?}"
            );
            // Old domain rows via the production seam: pruned members read
            // empty, so the removal observation carries no mover.
            let old_members = spaces.workspace_members("mon-a", &backing);
            assert!(old_members.is_empty(), "prior {prior}: backing pruned");
            let old_rows = domain_rows(&old_members, &[]).expect("prior {prior}: pruned rows");
            assert!(old_rows.is_empty(), "prior {prior}: old rows empty");
            let old_fp = crate::tiling::fingerprint(&[]);
            let correlation = CorrelationId::parse("prune-old").expect("correlation");
            let old_event = crate::tiling::build_reconcile_event_for_floating(
                &owner,
                &generation,
                &correlation,
                revision_of(&engine, &source),
                old_fp,
                &source.0,
                &source.1,
                8,
                &[],
                None,
            );
            let reply = engine.handle(&old_event);
            assert!(
                matches!(
                    reply,
                    CoreReply::Projection(_) | CoreReply::Tiled(_) | CoreReply::SendWorkspace(_)
                ),
                "prior {prior}: old removal converges, got {reply:?}"
            );
            assert!(
                !engine
                    .session(&source.1)
                    .is_some_and(|s| s.is_exception(&WindowId("w1".to_owned()))),
                "prior {prior}: old exception gone"
            );
            // Current rows via the production seam: existing sibling rows
            // plus the mover explicit with the live frame, floating by
            // origin (prior float stays float, prior tiled tiles).
            let cur_members = spaces.workspace_members("mon-a", &current);
            assert!(
                cur_members.contains(&sibling),
                "prior {prior}: sibling stays current"
            );
            let cur_views = vec![MemberView {
                key: sibling.clone(),
                token: "w2".to_owned(),
                rect: bounds,
                hints: tiler_core::size_hints::WindowSizeHints::none(),
                floating: false,
            }];
            let mut cur_rows =
                domain_rows(&cur_members, &cur_views).expect("prior {prior}: base rows");
            cur_rows.push(OwnerRow {
                token: "w1".to_owned(),
                rect: live_frame,
                hints: tiler_core::size_hints::WindowSizeHints::none(),
                floating,
            });
            assert!(
                cur_rows
                    .iter()
                    .any(|r| r.token == "w1" && r.rect == live_frame),
                "prior {prior}: explicit current row carries the live frame"
            );
            let cur_windows: Vec<(
                WindowId,
                Rect,
                tiler_core::size_hints::WindowSizeHints,
                bool,
            )> = cur_rows
                .iter()
                .map(|r| (WindowId(r.token.clone()), r.rect, r.hints, r.floating))
                .collect();
            let cur_fp = crate::tiling::fingerprint(
                &cur_rows
                    .iter()
                    .map(|r| (r.token.clone(), r.rect))
                    .collect::<Vec<_>>(),
            );
            let correlation = CorrelationId::parse("admit-current").expect("correlation");
            let cur_event = crate::tiling::build_reconcile_event_for_floating(
                &owner,
                &generation,
                &correlation,
                revision_of(&engine, &target),
                cur_fp,
                &target.0,
                &target.1,
                8,
                &cur_windows,
                None,
            );
            let reply = engine.handle(&cur_event);
            assert!(
                matches!(
                    reply,
                    CoreReply::Projection(_) | CoreReply::Tiled(_) | CoreReply::SendWorkspace(_)
                ),
                "prior {prior}: current admit converges, got {reply:?}"
            );
            let cur_float = engine
                .session(&target.1)
                .is_some_and(|s| s.is_exception(&WindowId("w1".to_owned())));
            assert_eq!(
                cur_float, prior,
                "prior {prior}: current float follows origin"
            );
            if !prior {
                let tiled: Vec<WindowId> = match reply {
                    CoreReply::Tiled(plan) => {
                        plan.geometry.iter().map(|g| g.window.clone()).collect()
                    }
                    CoreReply::Projection(plan) => {
                        plan.geometry.iter().map(|g| g.window.clone()).collect()
                    }
                    CoreReply::SendWorkspace(plan) => {
                        plan.geometry.iter().map(|g| g.window.clone()).collect()
                    }
                    reply => panic!("prior {prior}: current tiles, got {reply:?}"),
                };
                assert!(
                    tiled.contains(&WindowId("w1".to_owned())),
                    "prior {prior}: target tiles w1, got {tiled:?}"
                );
            }
            assert!(
                !engine
                    .session(&source.1)
                    .is_some_and(|s| s.is_exception(&WindowId("w1".to_owned()))),
                "prior {prior}: no stale duplicate in old"
            );
        }
    }

    #[test]
    fn sticky_adopt_reads_both_markers_as_sticky_before_tiling() {
        // Restart adoption re-adopts either marker value as sticky with its
        // pre-sticky origin before any tiling (the old normal-float
        // consumption was the item 8 gap). Mirrors the native adoption
        // preamble through the production builders. Post-restart un-stick
        // stays TBD under R-FLT-05/R-RST-01: no un-stick outcome asserted.
        use tiler_core::boundary::CoreReply;
        for prior in [false, true] {
            let value = crate::tiling::sticky_marker_value(prior);
            assert_eq!(
                crate::tiling::parse_sticky_marker(value),
                Some(prior),
                "marker round-trips"
            );
            // Portable classifier plus fences agree before the Engine moves.
            let marker = classify_restart_marker(
                MarkerSlot::Absent,
                MarkerSlot::Value(value),
                MarkerSlot::Absent,
            );
            assert_eq!(marker, RestartMarker::Sticky(prior));
            assert_eq!(
                restart_sticky_eligible(true, true, true, marker),
                Some(prior),
                "prior {prior}: fences admit sticky"
            );
            assert!(
                !restart_float_eligible(true, true, true, marker),
                "prior {prior}: sticky never hydrates as ordinary float"
            );
            let mut engine = tiler_core::engine::Engine::new();
            let owner = OwnerId::parse("tiler-windows").expect("owner");
            let generation = GenerationId::parse("aa").expect("generation");
            engine.sync_binding(&owner, &generation);
            let bounds = rect(0, 0);
            let domain = workspace_domain("mon-a", "ws-1", bounds, 8);
            let correlation = CorrelationId::parse("adopt").expect("correlation");
            let carried = vec![(
                WindowId("w1".to_owned()),
                bounds,
                tiler_core::size_hints::WindowSizeHints::none(),
                true,
            )];
            let fp = crate::tiling::fingerprint(&[("w1".to_owned(), bounds)]);
            let adopt = crate::tiling::build_reconcile_event_for_floating(
                &owner,
                &generation,
                &correlation,
                revision_of(&engine, &domain),
                fp,
                &domain.0,
                &domain.1,
                8,
                &carried,
                None,
            );
            let reply = engine.handle(&adopt);
            assert!(
                matches!(
                    reply,
                    CoreReply::Projection(_) | CoreReply::Tiled(_) | CoreReply::SendWorkspace(_)
                ),
                "prior {prior}: adoption converges, got {reply:?}"
            );
            assert!(
                engine
                    .session(&domain.1)
                    .is_some_and(|s| s.is_exception(&WindowId("w1".to_owned()))),
                "prior {prior}: adopted window floats"
            );
            // Fixed verdict: the marker re-adopts sticky with its origin
            // (runtime sticky entry kept, marker kept for later restarts),
            // never a normal float.
            let mut sticky: BTreeMap<WindowKey, bool> = BTreeMap::new();
            sticky.insert(key(11), prior);
            assert_eq!(
                sticky.get(&key(11)),
                Some(&prior),
                "prior {prior}: sticky origin retained"
            );
        }
    }

    #[test]
    fn restart_ordinary_float_hydrates_before_tiling() {
        // An intentional ordinary float marker re-adopts floating before any
        // tiling: classifier plus fences admit, the Engine carries the
        // exception, and no sticky entry appears.
        use tiler_core::boundary::CoreReply;
        let raw = crate::tiling::float_intent_marker_value();
        assert!(crate::tiling::parse_float_intent_marker(raw).is_some());
        let marker = classify_restart_marker(
            MarkerSlot::Value(raw),
            MarkerSlot::Absent,
            MarkerSlot::Absent,
        );
        assert_eq!(marker, RestartMarker::FloatIntent);
        assert!(restart_float_eligible(true, true, true, marker));
        assert_eq!(restart_sticky_eligible(true, true, true, marker), None);
        let mut engine = tiler_core::engine::Engine::new();
        let owner = OwnerId::parse("tiler-windows").expect("owner");
        let generation = GenerationId::parse("aa").expect("generation");
        engine.sync_binding(&owner, &generation);
        let bounds = rect(0, 0);
        let domain = workspace_domain("mon-a", "ws-1", bounds, 8);
        let correlation = CorrelationId::parse("adopt").expect("correlation");
        let carried = vec![(
            WindowId("w1".to_owned()),
            bounds,
            tiler_core::size_hints::WindowSizeHints::none(),
            true,
        )];
        let fp = crate::tiling::fingerprint(&[("w1".to_owned(), bounds)]);
        let adopt = crate::tiling::build_reconcile_event_for_floating(
            &owner,
            &generation,
            &correlation,
            revision_of(&engine, &domain),
            fp,
            &domain.0,
            &domain.1,
            8,
            &carried,
            None,
        );
        let reply = engine.handle(&adopt);
        assert!(
            matches!(
                reply,
                CoreReply::Projection(_) | CoreReply::Tiled(_) | CoreReply::SendWorkspace(_)
            ),
            "adoption converges, got {reply:?}"
        );
        assert!(
            engine
                .session(&domain.1)
                .is_some_and(|s| s.is_exception(&WindowId("w1".to_owned()))),
            "ordinary float re-adopts floating"
        );
    }

    #[test]
    fn restart_marker_mismatch_and_reused_identity_refuse() {
        // Corrupt values, failed reads, lifetime mismatch, identity
        // mismatch, unadmitted membership, and recycled HWNDs all fail
        // closed: no float, no sticky, marker alone never grants admission.
        use MarkerSlot::{Absent, Unreadable, Value};
        assert_eq!(
            classify_restart_marker(Absent, Absent, Absent),
            RestartMarker::Absent
        );
        assert_eq!(
            classify_restart_marker(Value(7), Value(3), Value(9)),
            RestartMarker::Corrupt
        );
        assert_eq!(
            classify_restart_marker(Value(u64::MAX), Absent, Absent),
            RestartMarker::Corrupt
        );
        // Defensive only: native NULL/zero reads as Absent, so a Value(0)
        // never arrives from production; it still fails closed here.
        assert_eq!(
            classify_restart_marker(Value(0), Value(0), Value(0)),
            RestartMarker::Corrupt
        );
        // Failed reads diagnose as unreadable, never as intent.
        assert_eq!(
            classify_restart_marker(Unreadable, Absent, Absent),
            RestartMarker::Unreadable
        );
        assert_eq!(
            classify_restart_marker(Absent, Absent, Unreadable),
            RestartMarker::Unreadable
        );
        // A valid lane outranks a failed read elsewhere.
        assert_eq!(
            classify_restart_marker(
                Unreadable,
                Value(crate::tiling::sticky_marker_value(false)),
                Unreadable,
            ),
            RestartMarker::Sticky(false)
        );
        for marker in [RestartMarker::Corrupt, RestartMarker::Unreadable] {
            assert!(!restart_float_eligible(true, true, true, marker));
            assert_eq!(restart_sticky_eligible(true, true, true, marker), None);
        }
        // Lifetime mismatch (live tag differs): the visible-lifetime fence
        // refuses exactly where HWND/PID/creation still agree.
        assert!(!visible_lifetime_ok(
            "9f2c41aa07bd33e0",
            Some("0000000000000001")
        ));
        assert!(!visible_lifetime_ok("9f2c41aa07bd33e0", None));
        let float_marker = classify_restart_marker(
            MarkerSlot::Value(crate::tiling::float_intent_marker_value()),
            MarkerSlot::Absent,
            MarkerSlot::Absent,
        );
        assert!(!restart_float_eligible(true, true, false, float_marker));
        assert!(!restart_float_eligible(true, false, true, float_marker));
        assert!(!restart_float_eligible(false, true, true, float_marker));
        let sticky_marker = classify_restart_marker(
            MarkerSlot::Absent,
            MarkerSlot::Value(crate::tiling::sticky_marker_value(true)),
            MarkerSlot::Absent,
        );
        assert_eq!(
            restart_sticky_eligible(true, true, false, sticky_marker),
            None
        );
        assert_eq!(
            restart_sticky_eligible(true, false, true, sticky_marker),
            None
        );
        assert_eq!(
            restart_sticky_eligible(false, true, true, sticky_marker),
            None
        );
        assert_eq!(
            restart_sticky_eligible(true, true, true, RestartMarker::Absent),
            None
        );
        // Recycled HWND: the fresh enumeration read is authoritative, so a
        // stale same-HWND key drops and the new generation (fresh tag, no
        // marker) hydrates nothing.
        let known = vec![key(41)];
        let fresh = WindowKey {
            hwnd: 41,
            pid: 9999,
            creation: "c000000000000041".to_owned(),
        };
        assert_eq!(
            reused_hwnd_stale(&known, &BTreeSet::new(), &fresh),
            vec![key(41)]
        );
        assert!(!visible_lifetime_ok("9f2c41aa07bd33e0", None));
        assert!(!restart_float_eligible(true, true, false, float_marker));
    }

    #[test]
    fn restart_marker_values_roundtrip_and_reject() {
        // Codec behind the native install/remove ops (exercised for real in
        // `tiling_sys::restart_adoption_tests`): well-formed values
        // round-trip; zero/unknown fail closed.
        assert_eq!(crate::tiling::float_intent_marker_value(), 1);
        assert!(crate::tiling::parse_float_intent_marker(1).is_some());
        assert!(crate::tiling::parse_float_intent_marker(0).is_none());
        assert!(crate::tiling::parse_float_intent_marker(2).is_none());
        assert!(crate::tiling::parse_float_intent_marker(u64::MAX).is_none());
        assert_eq!(crate::tiling::tile_override_marker_value(), 1);
        assert!(crate::tiling::parse_tile_override_marker(1).is_some());
        assert!(crate::tiling::parse_tile_override_marker(0).is_none());
        assert!(crate::tiling::parse_tile_override_marker(2).is_none());
        assert!(crate::tiling::parse_tile_override_marker(u64::MAX).is_none());
    }

    #[test]
    fn restart_tile_override_reserved_never_floats() {
        // The D7 tile-override marker classifies but hydrates nothing in
        // item 8: no float, no sticky, fixed-size admission unwired.
        let marker = classify_restart_marker(
            MarkerSlot::Absent,
            MarkerSlot::Absent,
            MarkerSlot::Value(crate::tiling::tile_override_marker_value()),
        );
        assert_eq!(marker, RestartMarker::TileOverride);
        assert!(!restart_float_eligible(true, true, true, marker));
        assert_eq!(restart_sticky_eligible(true, true, true, marker), None);
        // Sticky and float markers outrank the reserved lane.
        assert_eq!(
            classify_restart_marker(
                MarkerSlot::Absent,
                MarkerSlot::Value(crate::tiling::sticky_marker_value(false)),
                MarkerSlot::Value(crate::tiling::tile_override_marker_value()),
            ),
            RestartMarker::Sticky(false)
        );
        assert_eq!(
            classify_restart_marker(
                MarkerSlot::Value(crate::tiling::float_intent_marker_value()),
                MarkerSlot::Absent,
                MarkerSlot::Value(crate::tiling::tile_override_marker_value()),
            ),
            RestartMarker::FloatIntent
        );
    }

    #[test]
    fn restart_second_owner_rehydrates_kept_markers() {
        // Adoption keeps durable markers, so a second owner restart reads
        // the same slots and reaches the same verdict. No post-restart
        // un-stick outcome is selected on either pass.
        let lanes = [
            (
                MarkerSlot::Absent,
                MarkerSlot::Value(crate::tiling::sticky_marker_value(false)),
                MarkerSlot::Absent,
            ),
            (
                MarkerSlot::Absent,
                MarkerSlot::Value(crate::tiling::sticky_marker_value(true)),
                MarkerSlot::Absent,
            ),
            (
                MarkerSlot::Value(crate::tiling::float_intent_marker_value()),
                MarkerSlot::Absent,
                MarkerSlot::Absent,
            ),
        ];
        for (float, sticky, tile) in lanes {
            // First owner: hydrate.
            let first = classify_restart_marker(float, sticky, tile);
            let first_float = restart_float_eligible(true, true, true, first);
            let first_sticky = restart_sticky_eligible(true, true, true, first);
            // Second owner with the kept marker: identical verdict.
            let second = classify_restart_marker(float, sticky, tile);
            assert_eq!(second, first);
            assert_eq!(
                restart_float_eligible(true, true, true, second),
                first_float
            );
            assert_eq!(
                restart_sticky_eligible(true, true, true, second),
                first_sticky
            );
        }
    }

    #[test]
    fn restart_prop_namespaces_are_distinct_and_legacy_retired() {
        // Stronger dotted namespaces for all three lanes; the legacy
        // ambiguous sticky name hydrates nothing.
        for prop in [
            crate::model::STICKY_PROP,
            crate::model::FLOAT_INTENT_PROP,
            crate::model::TILE_OVERRIDE_PROP,
        ] {
            assert!(prop.starts_with("OmniTiler"), "distinctive prefix: {prop}");
            assert!(prop.contains('.'), "stronger namespace: {prop}");
        }
        assert_ne!(crate::model::STICKY_PROP, crate::model::FLOAT_INTENT_PROP);
        assert_ne!(crate::model::STICKY_PROP, crate::model::TILE_OVERRIDE_PROP);
        assert_ne!(
            crate::model::FLOAT_INTENT_PROP,
            crate::model::TILE_OVERRIDE_PROP
        );
        assert_eq!(crate::model::STICKY_PROP, "OmniTiler.Sticky.v1");
        // The legacy ambiguous name hydrates nothing: only the dotted name
        // above is ever read.
        assert_ne!(crate::model::STICKY_PROP, "OmniTilerSticky");
    }

    #[test]
    fn hidden_float_rows_stay_floating_until_reveal() {
        // Hidden intentional floats ride the live float snapshot with the
        // tiled allocation as fallback, so convergence never drops retained
        // hidden membership. Sticky rides the same slotless float lane.
        use super::{MemberView, domain_rows, hidden_snapshot_rect};
        let float_rect = Some(Rect {
            x: 10,
            y: 20,
            w: 400,
            h: 300,
        });
        let tiled_rect = Some(Rect {
            x: 0,
            y: 0,
            w: 800,
            h: 600,
        });
        assert_eq!(
            hidden_snapshot_rect(true, float_rect, tiled_rect),
            float_rect
        );
        assert_eq!(hidden_snapshot_rect(true, None, tiled_rect), tiled_rect);
        let member = key(21);
        let members: BTreeSet<WindowKey> = [member.clone()].into_iter().collect();
        let float_frame = Rect {
            x: 10,
            y: 20,
            w: 400,
            h: 300,
        };
        let views = vec![MemberView {
            key: member,
            token: "w-float".to_owned(),
            rect: float_frame,
            hints: tiler_core::size_hints::WindowSizeHints::none(),
            floating: true,
        }];
        let rows = domain_rows(&members, &views).expect("hidden float rows");
        assert_eq!(rows.len(), 1);
        assert!(rows[0].floating, "hidden float stays floating");
    }

    // Item 2 follow/stay through the real retained Engine (offline
    // verification; native hide/reveal/focus/hook wiring is Windows-only and
    // stays user-owned live). A maximized mover rides this same ordinary
    // Engine route (the retained overlay skip and live flag races are
    // adapter-native, preserved unchanged): admission equality here plus the
    // flag predicate below is the portable half of the item 20 check.

    #[allow(clippy::too_many_arguments)]
    fn seed_send_domains(
        engine: &mut tiler_core::engine::Engine,
        owner: &OwnerId,
        generation: &GenerationId,
        source: &(OutputDomain, DomainKey),
        target: &(OutputDomain, DomainKey),
        source_tokens: &[&str],
        target_tokens: &[&str],
        mover: &str,
        bounds: Rect,
    ) {
        // Target converges first so the source focus (the mover) is the last
        // focus the Engine sees; the send below must carry the focused mover.
        for (domain, tokens, focused) in [
            (target, target_tokens, None),
            (
                source,
                source_tokens,
                Some(tiler_core::directional::WindowId(mover.to_owned())),
            ),
        ] {
            let correlation = CorrelationId::parse("seed").expect("correlation");
            let revision = engine
                .session(&domain.1)
                .map(|s| s.accepted_revision())
                .unwrap_or(0);
            let event = crate::tiling::build_reconcile_event_for(
                owner,
                generation,
                &correlation,
                revision,
                tokens.len() as u64,
                &domain.0,
                &domain.1,
                8,
                &tokens
                    .iter()
                    .map(|t| {
                        (
                            tiler_core::directional::WindowId((*t).to_owned()),
                            bounds,
                            tiler_core::size_hints::WindowSizeHints::none(),
                        )
                    })
                    .collect::<Vec<_>>(),
                focused.as_ref(),
            );
            let reply = engine.handle(&event);
            assert!(
                matches!(
                    reply,
                    tiler_core::boundary::CoreReply::Projection(_)
                        | tiler_core::boundary::CoreReply::Tiled(_)
                        | tiler_core::boundary::CoreReply::SendWorkspace(_)
                ),
                "seed converges, got {reply:?}"
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn send_through_engine(
        engine: &mut tiler_core::engine::Engine,
        owner: &OwnerId,
        generation: &GenerationId,
        source: &(OutputDomain, DomainKey),
        target: &(OutputDomain, DomainKey),
        source_tokens: &[&str],
        target_tokens: &[&str],
        mover: &str,
        bounds: Rect,
        follow: bool,
    ) -> tiler_core::boundary::SendWorkspacePlan {
        use tiler_core::boundary::CoreReply;
        let revision = engine
            .session(&source.1)
            .map(|s| s.accepted_revision())
            .unwrap_or(0);
        let correlation = CorrelationId::parse("tick-1").expect("correlation");
        let rows: Vec<OwnerRow> = source_tokens
            .iter()
            .map(|t| OwnerRow {
                token: (*t).to_owned(),
                rect: bounds,
                hints: tiler_core::size_hints::WindowSizeHints::none(),
                floating: false,
            })
            .collect();
        let target_rows: Vec<OwnerRow> = target_tokens
            .iter()
            .map(|t| OwnerRow {
                token: (*t).to_owned(),
                rect: bounds,
                hints: tiler_core::size_hints::WindowSizeHints::none(),
                floating: false,
            })
            .collect();
        let mut event = build_send_event(
            owner,
            generation,
            &correlation,
            revision,
            42,
            source.clone(),
            target.clone(),
            &rows,
            &target_rows,
            mover,
            8,
            follow,
        )
        .expect("event");
        stamp_send_target(&mut event, &target.1);
        let reply = engine.handle(&event);
        let CoreReply::SendWorkspace(plan) = reply else {
            panic!("send commits, got {reply:?}");
        };
        assert_eq!(plan.follow, follow, "plan echoes the explicit intent");
        plan
    }

    fn snapshot_window(
        engine: &tiler_core::engine::Engine,
        domain: &DomainKey,
        leaf: &tiler_core::directional::NodeId,
    ) -> Option<String> {
        engine
            .session(domain)?
            .snapshot()
            .windows
            .iter()
            .find(|link| {
                &link.leaf == leaf
                    && link.output == domain.output
                    && link.workspace == domain.workspace
            })
            .map(|link| link.window.0.clone())
    }

    #[test]
    fn send_follow_and_stay_share_admission_geometry() {
        // WS1 [wA, wB] (mover wB focused), WS2 [wC]: follow and stay commit
        // identical destination admission, source collapse and destination
        // geometry; only the desired focus and the intent flag differ (target
        // mover vs source MRU).
        let bounds = rect(0, 0);
        let mut plans = Vec::new();
        for follow in [true, false] {
            let mut engine = tiler_core::engine::Engine::new();
            let owner = OwnerId::parse("tiler-windows").expect("owner");
            let generation = GenerationId::parse("aa").expect("generation");
            engine.sync_binding(&owner, &generation);
            let source = workspace_domain("mon-a", "ws-1", bounds, 8);
            let target = workspace_domain("mon-a", "ws-2", bounds, 8);
            seed_send_domains(
                &mut engine,
                &owner,
                &generation,
                &source,
                &target,
                &["wA", "wB"],
                &["wC"],
                "wB",
                bounds,
            );
            let plan = send_through_engine(
                &mut engine,
                &owner,
                &generation,
                &source,
                &target,
                &["wA", "wB"],
                &["wC"],
                "wB",
                bounds,
                follow,
            );
            plans.push((source, target, plan, engine));
        }
        let (follow_source, follow_target, follow_plan, follow_engine) = &plans[0];
        let (stay_source, stay_target, stay_plan, stay_engine) = &plans[1];
        // Identical admission: same operation, same placed geometry per
        // window, same mover/target domains.
        assert_eq!(
            std::mem::discriminant(&follow_plan.operation),
            std::mem::discriminant(&stay_plan.operation),
            "same structural operation"
        );
        let mut follow_geometry: Vec<(String, Rect)> = follow_plan
            .geometry
            .iter()
            .map(|g| (g.window.0.clone(), g.rect))
            .collect();
        follow_geometry.sort_by(|a, b| a.0.cmp(&b.0));
        let mut stay_geometry: Vec<(String, Rect)> = stay_plan
            .geometry
            .iter()
            .map(|g| (g.window.0.clone(), g.rect))
            .collect();
        stay_geometry.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(
            follow_geometry, stay_geometry,
            "follow/stay place identical geometry"
        );
        assert!(
            follow_geometry.iter().any(|(w, _)| w == "wB"),
            "mover admitted"
        );
        assert!(
            follow_geometry.iter().any(|(w, _)| w == "wA"),
            "source survivor reflowed"
        );
        assert!(
            follow_geometry.iter().any(|(w, _)| w == "wC"),
            "destination survivor kept"
        );
        // Only focus differs: follow names the mover on the target, stay
        // names the source focused-removal MRU (wA, never the target).
        let follow_focus = follow_plan.focus_domain.clone().expect("follow focus");
        assert_eq!(follow_focus, follow_target.1, "follow focuses the target");
        let follow_leaf = follow_plan.focus_leaf.clone().expect("follow leaf");
        assert_eq!(
            snapshot_window(follow_engine, &follow_focus, &follow_leaf).as_deref(),
            Some("wB"),
            "follow focuses the mover"
        );
        let stay_focus = stay_plan.focus_domain.clone().expect("stay focus");
        assert_eq!(stay_focus, stay_source.1, "stay focuses the source");
        let stay_leaf = stay_plan.focus_leaf.clone().expect("stay leaf");
        assert_eq!(
            snapshot_window(stay_engine, &stay_focus, &stay_leaf).as_deref(),
            Some("wA"),
            "stay focuses the source MRU, never the mover"
        );
        let _ = (follow_source, stay_target);
    }

    #[test]
    fn send_stay_reports_null_focus_for_a_sole_mover() {
        // WS1 [wB] sole mover, WS2 [wC]: stay empties the source, so the plan
        // carries null focus (no setter; the native removal focus stands).
        // Follow on the same pair still focuses the mover on the target.
        let bounds = rect(0, 0);
        let mut engine = tiler_core::engine::Engine::new();
        let owner = OwnerId::parse("tiler-windows").expect("owner");
        let generation = GenerationId::parse("aa").expect("generation");
        engine.sync_binding(&owner, &generation);
        let source = workspace_domain("mon-a", "ws-1", bounds, 8);
        let target = workspace_domain("mon-a", "ws-2", bounds, 8);
        seed_send_domains(
            &mut engine,
            &owner,
            &generation,
            &source,
            &target,
            &["wB"],
            &["wC"],
            "wB",
            bounds,
        );
        let stay = send_through_engine(
            &mut engine,
            &owner,
            &generation,
            &source,
            &target,
            &["wB"],
            &["wC"],
            "wB",
            bounds,
            false,
        );
        assert_eq!(stay.focus_domain, None, "sole stay has no focus domain");
        assert_eq!(stay.focus_leaf, None, "sole stay has no focus leaf");
        let mut engine = tiler_core::engine::Engine::new();
        engine.sync_binding(&owner, &generation);
        seed_send_domains(
            &mut engine,
            &owner,
            &generation,
            &source,
            &target,
            &["wB"],
            &["wC"],
            "wB",
            bounds,
        );
        let follow = send_through_engine(
            &mut engine,
            &owner,
            &generation,
            &source,
            &target,
            &["wB"],
            &["wC"],
            "wB",
            bounds,
            true,
        );
        assert_eq!(follow.focus_domain, Some(target.1.clone()));
        let leaf = follow.focus_leaf.clone().expect("follow leaf");
        assert_eq!(
            snapshot_window(&engine, &target.1, &leaf).as_deref(),
            Some("wB")
        );
    }

    #[test]
    fn send_hidden_target_admits_but_never_writes() {
        // WS1 [wA, wB], WS2 [wC hidden]: the plan still admits the mover onto
        // the hidden target (placement computed), but the portable writable
        // subset excludes the hidden member, so no target write ever issues
        // while hidden. Stay never reveals; follow reveals through select.
        use std::collections::{BTreeMap, HashSet};
        let bounds = rect(0, 0);
        let mut engine = tiler_core::engine::Engine::new();
        let owner = OwnerId::parse("tiler-windows").expect("owner");
        let generation = GenerationId::parse("aa").expect("generation");
        engine.sync_binding(&owner, &generation);
        let source = workspace_domain("mon-a", "ws-1", bounds, 8);
        let target = workspace_domain("mon-a", "ws-2", bounds, 8);
        seed_send_domains(
            &mut engine,
            &owner,
            &generation,
            &source,
            &target,
            &["wA", "wB"],
            &["wC"],
            "wB",
            bounds,
        );
        let plan = send_through_engine(
            &mut engine,
            &owner,
            &generation,
            &source,
            &target,
            &["wA", "wB"],
            &["wC"],
            "wB",
            bounds,
            false,
        );
        assert!(
            plan.geometry.iter().any(|g| g.window.0 == "wB"),
            "mover admitted onto the hidden target"
        );
        let hidden = key(30);
        let visible_a = key(31);
        let members: BTreeSet<WindowKey> =
            [hidden.clone(), visible_a.clone()].into_iter().collect();
        let token_of: BTreeMap<WindowKey, String> = BTreeMap::from([
            (hidden.clone(), "wC".to_owned()),
            (visible_a.clone(), "wA".to_owned()),
        ]);
        let fresh: HashSet<String> = ["wC".to_owned(), "wA".to_owned()].into_iter().collect();
        let writable = super::writable_subset(&members, |k| k == &hidden, &token_of, &fresh);
        assert!(!writable.contains("wC"), "hidden target never writable");
        assert!(
            writable.contains("wA"),
            "visible source survivor stays writable"
        );
    }

    #[test]
    fn send_fullscreen_carry_matches_maximized_allocation() {
        // Item 9: each overlay kind resolves through retained admission plus
        // the live overlay gate, then takes the identical actual retained
        // Engine allocation in both intents (the Engine never sees overlay
        // flags). Fails if either leg's admission or gate is removed.
        use super::{send_overlay_gate, send_retained_overlay_admits};
        let bounds = rect(0, 0);
        let mut allocations = Vec::new();
        for (row_maximized, row_fullscreen) in [(true, false), (false, true)] {
            // Retained resolution, production order: admission first, then the
            // live recheck against the row snapshot (readable stable frame).
            assert!(
                send_retained_overlay_admits(row_maximized, row_fullscreen),
                "overlay leg admits"
            );
            assert_eq!(
                send_overlay_gate(
                    row_maximized,
                    row_fullscreen,
                    row_maximized,
                    Some(row_fullscreen),
                    row_fullscreen
                ),
                Ok(()),
                "stable overlay leg carries"
            );
            for follow in [true, false] {
                let mut engine = tiler_core::engine::Engine::new();
                let owner = OwnerId::parse("tiler-windows").expect("owner");
                let generation = GenerationId::parse("aa").expect("generation");
                engine.sync_binding(&owner, &generation);
                let source = workspace_domain("mon-a", "ws-1", bounds, 8);
                let target = workspace_domain("mon-a", "ws-2", bounds, 8);
                seed_send_domains(
                    &mut engine,
                    &owner,
                    &generation,
                    &source,
                    &target,
                    &["wA", "wB"],
                    &["wC"],
                    "wB",
                    bounds,
                );
                let plan = send_through_engine(
                    &mut engine,
                    &owner,
                    &generation,
                    &source,
                    &target,
                    &["wA", "wB"],
                    &["wC"],
                    "wB",
                    bounds,
                    follow,
                );
                let mut geometry: Vec<(String, Rect)> = plan
                    .geometry
                    .iter()
                    .map(|g| (g.window.0.clone(), g.rect))
                    .collect();
                geometry.sort_by(|a, b| a.0.cmp(&b.0));
                allocations.push((
                    row_maximized,
                    row_fullscreen,
                    follow,
                    plan.operation,
                    geometry,
                ));
            }
        }
        let first = &allocations[0];
        for entry in &allocations[1..] {
            assert_eq!(
                std::mem::discriminant(&entry.3),
                std::mem::discriminant(&first.3),
                "same structural operation across overlay kinds and intents"
            );
            assert_eq!(
                entry.4, first.4,
                "identical allocation across overlay kinds and intents"
            );
        }
        assert!(first.4.iter().any(|(w, _)| w == "wB"), "mover admitted");
        assert!(first.4.iter().any(|(w, _)| w == "wA"), "survivor reflowed");
    }

    #[test]
    fn send_fullscreen_mover_never_takes_geometry_writes() {
        // Item 9: a fullscreen mover classifies out of the eligible
        // observation exactly like maximized, so the portable writable
        // subset (the same `writable_subset` production calls) never
        // includes its retained-only token: no native size/position write.
        use crate::tiling::{SkipReason, classify, classify_focus};
        use std::collections::{BTreeMap, BTreeSet, HashSet};
        let facts = crate::tiling::WindowFacts {
            visible: true,
            minimized: false,
            maximized: false,
            cloaked: false,
            elevated: false,
            shell: false,
            tool_window: false,
            owned: false,
            captionless_fullscreen: true,
            no_activate: false,
            dialog: false,
        };
        assert_eq!(classify(&facts), Err(SkipReason::Fullscreen));
        assert!(classify_focus(&facts).is_ok());
        let key = key(77);
        let members: BTreeSet<WindowKey> = BTreeSet::from([key.clone()]);
        let token_of: BTreeMap<WindowKey, String> =
            BTreeMap::from([(key.clone(), "wB".to_owned())]);
        let fresh: HashSet<String> = HashSet::new();
        assert!(
            super::writable_subset(&members, |_| false, &token_of, &fresh).is_empty(),
            "retained fullscreen token never writable"
        );
        let fresh: HashSet<String> = HashSet::from(["wB".to_owned()]);
        assert!(
            super::writable_subset(&members, |_| true, &token_of, &fresh).is_empty(),
            "hidden fullscreen target never writable"
        );
    }

    #[test]
    fn send_fullscreen_stay_keeps_source_mru_and_null_without_setter() {
        // Item 9: stay reuses the existing MRU/null policy unchanged. A
        // non-empty source names the source MRU (never the mover), and the
        // genuine sole-mover Engine null plan resolves Null (no setter).
        use super::{StayFocusClass, classify_stay_focus};
        let bounds = rect(0, 0);
        let owner = OwnerId::parse("tiler-windows").expect("owner");
        let generation = GenerationId::parse("aa").expect("generation");
        let mut engine = tiler_core::engine::Engine::new();
        engine.sync_binding(&owner, &generation);
        let source = workspace_domain("mon-a", "ws-1", bounds, 8);
        let target = workspace_domain("mon-a", "ws-2", bounds, 8);
        seed_send_domains(
            &mut engine,
            &owner,
            &generation,
            &source,
            &target,
            &["wA", "wB"],
            &["wC"],
            "wB",
            bounds,
        );
        let stay = send_through_engine(
            &mut engine,
            &owner,
            &generation,
            &source,
            &target,
            &["wA", "wB"],
            &["wC"],
            "wB",
            bounds,
            false,
        );
        let stay_focus = stay.focus_domain.clone().expect("stay focus");
        assert_eq!(stay_focus, source.1, "stay focuses the source");
        let stay_leaf = stay.focus_leaf.clone().expect("stay leaf");
        assert_eq!(
            snapshot_window(&engine, &stay_focus, &stay_leaf).as_deref(),
            Some("wA"),
            "stay names the source MRU, never the mover"
        );
        // Genuine sole-mover null plan from the actual Engine, then the
        // production policy on that plan state: Null means no setter.
        let mut engine = tiler_core::engine::Engine::new();
        engine.sync_binding(&owner, &generation);
        seed_send_domains(
            &mut engine,
            &owner,
            &generation,
            &source,
            &target,
            &["wB"],
            &["wC"],
            "wB",
            bounds,
        );
        let sole = send_through_engine(
            &mut engine,
            &owner,
            &generation,
            &source,
            &target,
            &["wB"],
            &["wC"],
            "wB",
            bounds,
            false,
        );
        assert_eq!(sole.focus_domain, None, "sole stay has no focus domain");
        assert_eq!(sole.focus_leaf, None, "sole stay has no focus leaf");
        let plan_null = sole.focus_domain.is_none() || sole.focus_leaf.is_none();
        assert_eq!(
            classify_stay_focus(plan_null, false, false, false, false, false),
            StayFocusClass::Null,
            "actual null plan resolves Null with no setter"
        );
    }

    #[test]
    fn relative_send_target_freezes_once_across_trailing_and_wrap() {
        // Ring resolution is pure: no activation, no append, no creation.
        // Targets freeze once pre-transfer (filling the trailing empty
        // invokes ordinary lifecycle for the next spare afterwards).
        let mut m = ManagedWorkspaces::new();
        m.ensure_output("mon-1");
        for i in 0..10u64 {
            let ids: Vec<String> = m.workspace_ids("mon-1");
            for ws in &ids {
                let members = m.workspace_members("mon-1", ws);
                if members.is_empty() {
                    m.assign(
                        WindowKey {
                            hwnd: 900 + i,
                            pid: 1000 + i as u32,
                            creation: format!("c{i:016x}"),
                        },
                        "mon-1",
                        ws,
                        false,
                    );
                    break;
                }
            }
            let _ = m.select_trailing("mon-1");
        }
        assert!(m.workspace_count("mon-1") > 9, "ordinals run past 9");
        let ring = m.scoped_ring_ids("mon-1");
        let first = ring.first().cloned().expect("first");
        let last = ring.last().cloned().expect("last");
        let mid = ring[ring.len() / 2].clone();
        // Wrap both ends, trailing empty included, ordinals beyond 9 in ring.
        assert_eq!(
            m.resolve_relative_from("mon-1", &first, -1).as_deref(),
            Some(last.as_str())
        );
        assert_eq!(
            m.resolve_relative_from("mon-1", &last, 1).as_deref(),
            Some(first.as_str())
        );
        let at = ring.iter().position(|id| id == &mid).expect("mid");
        assert_eq!(
            m.resolve_relative_from("mon-1", &mid, -1).as_deref(),
            Some(ring[at - 1].as_str())
        );
        assert_eq!(
            m.resolve_relative_from("mon-1", &mid, 1).as_deref(),
            Some(ring[at + 1].as_str())
        );
        // Refusals: bad delta, unknown output/current, empty ring.
        assert_eq!(m.resolve_relative_from("mon-1", &mid, 0), None);
        assert_eq!(m.resolve_relative_from("mon-1", &mid, 2), None);
        assert_eq!(m.resolve_relative_from("mon-gone", &mid, 1), None);
        assert_eq!(m.resolve_relative_from("mon-1", "ws-gone", 1), None);
        // Pure: resolution never activates, appends, or creates.
        let active = m.active_id("mon-1").expect("active");
        let count = m.workspace_count("mon-1");
        let _ = m.resolve_relative_from("mon-1", &mid, 1);
        let _ = m.resolve_relative("mon-1", 1);
        assert_eq!(m.active_id("mon-1").as_deref(), Some(active.as_str()));
        assert_eq!(m.workspace_count("mon-1"), count);
        // Active-anchored resolution agrees with the source-anchored form at
        // the active view (sends anchor at the mover source instead).
        assert_eq!(
            m.resolve_relative("mon-1", 1),
            m.resolve_relative_from("mon-1", &active, 1)
        );
        // Frozen target: resolving once, then mutating (filling the trailing
        // empty plus lifecycle spare), never rewrites the frozen id.
        let frozen = m
            .resolve_relative_from("mon-1", &first, -1)
            .expect("frozen");
        assert_eq!(frozen, last, "pre-transfer trailing target");
        m.assign(
            WindowKey {
                hwnd: 4242,
                pid: 4242,
                creation: "c000000000004242".to_owned(),
            },
            "mon-1",
            &frozen,
            false,
        );
        let _ = m.resolve_send_trailing("mon-1");
        assert!(
            m.workspace_ids("mon-1").contains(&frozen),
            "frozen target survives lifecycle, never re-resolved"
        );
    }

    #[test]
    fn relative_send_resolves_from_the_mover_source() {
        // Three workspaces with the active view parked on the third: a
        // relative step from the mover's source (first) still steps the
        // source ring, never the active view.
        let mut m = ManagedWorkspaces::new();
        m.ensure_output("mon-1");
        let ring = m.scoped_ring_ids("mon-1");
        assert!(ring.len() >= 2);
        let source = ring[0].clone();
        assert!(m.activate("mon-1", &ring[ring.len() - 1]));
        assert_ne!(m.active_id("mon-1").as_deref(), Some(source.as_str()));
        assert_eq!(
            m.resolve_relative_from("mon-1", &source, 1).as_deref(),
            Some(ring[1].as_str())
        );
        assert_eq!(
            m.resolve_relative_from("mon-1", &source, -1).as_deref(),
            Some(ring.last().expect("last").as_str())
        );
    }

    #[test]
    fn send_flags_stable_gates_both_intents_identically() {
        // Item 20 flag races (KDE `flagsStillMatch` parity): the live overlay
        // flags must still equal the dispatch snapshot or the transfer
        // refuses with no writes. The predicate is intent-independent: follow
        // and stay refuse identically on any flip (both legs of G-D2 carry).
        use crate::tiling::send_flags_stable;
        assert!(send_flags_stable(false, false, false, false));
        assert!(send_flags_stable(false, true, false, true));
        assert!(send_flags_stable(true, false, true, false));
        assert!(send_flags_stable(true, true, true, true));
        assert!(!send_flags_stable(false, true, false, false));
        assert!(!send_flags_stable(false, false, false, true));
        assert!(!send_flags_stable(true, false, false, false));
        assert!(!send_flags_stable(false, true, true, true));
    }

    #[test]
    fn send_fences_hold_pins_view_mode_gap_scope_lifetime_membership_and_overlay() {
        // Production tails MUST route through `send_fences_hold`: every fence
        // below is a live tail-time recheck of a carried pre-plan snapshot,
        // refusing with no further effects and no replay.
        use super::{SendFenceLive, SendFenceSnapshot, send_fences_hold};
        let snap = SendFenceSnapshot {
            inner_gap: 8,
            outer_gap: 8,
            source_tiled: true,
            target_tiled: true,
            overlay_maximized: false,
            overlay_fullscreen: false,
        };
        let live = SendFenceLive {
            inner_gap: 8,
            outer_gap: 8,
            source_tiled: true,
            target_tiled: true,
            active_is_source: true,
            target_is_active: false,
            scope_ok: true,
            lifetime_ok: true,
            membership_ok: true,
            overlay_maximized: false,
            overlay_fullscreen: false,
        };
        assert_eq!(send_fences_hold(snap, live), None);
        // Source-selected view fence (both directions).
        assert_eq!(
            send_fences_hold(
                snap,
                SendFenceLive {
                    active_is_source: false,
                    ..live
                }
            ),
            Some("view-changed")
        );
        assert_eq!(
            send_fences_hold(
                snap,
                SendFenceLive {
                    target_is_active: true,
                    ..live
                }
            ),
            Some("view-changed")
        );
        // Both workspace modes, both gaps.
        assert_eq!(
            send_fences_hold(
                snap,
                SendFenceLive {
                    source_tiled: false,
                    ..live
                }
            ),
            Some("mode-changed")
        );
        assert_eq!(
            send_fences_hold(
                snap,
                SendFenceLive {
                    target_tiled: false,
                    ..live
                }
            ),
            Some("mode-changed")
        );
        assert_eq!(
            send_fences_hold(
                snap,
                SendFenceLive {
                    inner_gap: 4,
                    ..live
                }
            ),
            Some("deferred")
        );
        assert_eq!(
            send_fences_hold(
                snap,
                SendFenceLive {
                    outer_gap: 12,
                    ..live
                }
            ),
            Some("deferred")
        );
        // Scope, lifetime, membership.
        assert_eq!(
            send_fences_hold(
                snap,
                SendFenceLive {
                    scope_ok: false,
                    ..live
                }
            ),
            Some("scope-excluded")
        );
        assert_eq!(
            send_fences_hold(
                snap,
                SendFenceLive {
                    lifetime_ok: false,
                    ..live
                }
            ),
            Some("identity-changed")
        );
        assert_eq!(
            send_fences_hold(
                snap,
                SendFenceLive {
                    membership_ok: false,
                    ..live
                }
            ),
            Some("unverified")
        );
        // Item 9: a newly arrived live fullscreen refuses outright, other
        // overlay drift defers.
        assert_eq!(
            send_fences_hold(
                snap,
                SendFenceLive {
                    overlay_fullscreen: true,
                    ..live
                }
            ),
            Some("send-refused-fullscreen")
        );
        assert_eq!(
            send_fences_hold(
                snap,
                SendFenceLive {
                    overlay_maximized: true,
                    ..live
                }
            ),
            Some("deferred")
        );
        // Retained maximized carry holds while flags stay stable.
        let snap_max = SendFenceSnapshot {
            overlay_maximized: true,
            ..snap
        };
        assert_eq!(
            send_fences_hold(
                snap_max,
                SendFenceLive {
                    overlay_maximized: true,
                    ..live
                }
            ),
            None
        );
        assert_eq!(
            send_fences_hold(
                snap_max,
                SendFenceLive {
                    overlay_maximized: false,
                    ..live
                }
            ),
            Some("deferred")
        );
        // Retained fullscreen carry holds exactly like maximized while flags
        // stay stable; a fullscreen exit defers and a maximized drift on a
        // fullscreen snapshot defers.
        let snap_full = SendFenceSnapshot {
            overlay_fullscreen: true,
            ..snap
        };
        assert_eq!(
            send_fences_hold(
                snap_full,
                SendFenceLive {
                    overlay_fullscreen: true,
                    ..live
                }
            ),
            None
        );
        assert_eq!(
            send_fences_hold(
                snap_full,
                SendFenceLive {
                    overlay_fullscreen: false,
                    ..live
                }
            ),
            Some("deferred")
        );
        assert_eq!(
            send_fences_hold(
                snap_full,
                SendFenceLive {
                    overlay_fullscreen: true,
                    overlay_maximized: true,
                    ..live
                }
            ),
            Some("deferred")
        );
    }

    #[test]
    fn send_overlay_gate_refuses_fullscreen_and_defers_drift() {
        // Production pre-plan, post-plan, and pre-effect checks MUST route
        // through `send_overlay_gate` with live OS reads: a newly arrived
        // fullscreen refuses outright, other drift defers with no restore,
        // and stable maximized/fullscreen carry passes (item 9). A
        // fullscreen snapshot with an unreadable live flag defers pre-effect
        // (never trusts the fallback); ordinary/maximized snapshots keep the
        // legacy fallback.
        use super::send_overlay_gate;
        assert_eq!(
            send_overlay_gate(false, false, false, Some(false), false),
            Ok(())
        );
        assert_eq!(
            send_overlay_gate(true, false, true, Some(false), false),
            Ok(())
        );
        assert_eq!(
            send_overlay_gate(false, true, false, Some(true), true),
            Ok(())
        );
        assert_eq!(
            send_overlay_gate(true, true, true, Some(true), true),
            Ok(())
        );
        assert_eq!(
            send_overlay_gate(false, false, false, Some(true), false),
            Err("send-refused-fullscreen")
        );
        assert_eq!(
            send_overlay_gate(true, false, true, Some(true), true),
            Err("send-refused-fullscreen")
        );
        assert_eq!(
            send_overlay_gate(false, true, true, Some(true), true),
            Err("deferred")
        );
        assert_eq!(
            send_overlay_gate(false, true, false, Some(false), true),
            Err("deferred")
        );
        assert_eq!(
            send_overlay_gate(true, false, false, Some(false), false),
            Err("deferred")
        );
        assert_eq!(
            send_overlay_gate(false, false, true, Some(false), false),
            Err("deferred")
        );
        // Unreadable live flag: fullscreen snapshots defer, others ride the
        // legacy fallback (unreadable ordinary holds, retained maximized
        // still defers its maximize drift).
        assert_eq!(
            send_overlay_gate(false, true, false, None, true),
            Err("deferred")
        );
        assert_eq!(send_overlay_gate(false, false, false, None, false), Ok(()));
        assert_eq!(
            send_overlay_gate(true, false, false, None, false),
            Err("deferred")
        );
    }

    #[test]
    fn send_retained_overlay_admission_covers_maximized_and_fullscreen() {
        // Production send dispatch MUST route retained movers through
        // `send_retained_overlay_admits`: maximized and fullscreen members
        // resolve through the retained row with a live recheck, never as
        // ordinary; any other retained row never sends as an overlay mover
        // (slotless born-held windows never reach this: no member token
        // means `unmanaged` first).
        use super::send_retained_overlay_admits;
        assert!(send_retained_overlay_admits(true, false));
        assert!(send_retained_overlay_admits(false, true));
        assert!(send_retained_overlay_admits(true, true));
        assert!(!send_retained_overlay_admits(false, false));
    }

    #[test]
    fn send_focus_suppressed_only_for_fullscreen_snapshots() {
        // Production `workspace_send_follow` MUST route through
        // `send_focus_suppressed`: a fullscreen snapshot suppresses ALL
        // explicit focus actuation for the follow transition; ordinary and
        // maximized snapshots keep explicit focus.
        use super::send_focus_suppressed;
        assert!(!send_focus_suppressed(false));
        assert!(send_focus_suppressed(true));
    }

    #[test]
    fn classify_stay_focus_separates_null_from_stale() {
        // Production stay-focus MUST route through `classify_stay_focus`: a
        // genuine null plan is Null (ok/no-focus, no setter); a some-but-
        // unusable mapping is Stale (focus-unverified, no setter) for every
        // single failure; only the fully exact mapping is Ready.
        use super::{StayFocusClass, classify_stay_focus};
        assert_eq!(
            classify_stay_focus(true, false, false, false, false, false),
            StayFocusClass::Null
        );
        assert_eq!(
            classify_stay_focus(true, true, true, false, true, true),
            StayFocusClass::Null,
            "null plan stays Null even with a live mapping"
        );
        assert_eq!(
            classify_stay_focus(false, true, true, false, true, true),
            StayFocusClass::Ready
        );
        for (mapped, in_source, hidden, lifetime_ok, eligible) in [
            (false, false, false, false, false),
            (true, false, false, true, true),
            (true, true, true, true, true),
            (true, true, false, false, true),
            (true, true, false, true, false),
        ] {
            assert_eq!(
                classify_stay_focus(false, mapped, in_source, hidden, lifetime_ok, eligible),
                StayFocusClass::Stale,
                "mapped={mapped} in_source={in_source} hidden={hidden} lifetime={lifetime_ok} eligible={eligible}"
            );
        }
    }

    #[test]
    fn send_event_carries_floating_flags_for_both_intents() {
        // Floating-boundary membership rides the event rows for follow and
        // stay alike (native transfer, tiled-side reflow only); the command
        // intent flag is the only routing difference at this layer.
        let owner = OwnerId::parse("tiler-windows").expect("owner");
        let generation = GenerationId::parse("aa").expect("generation");
        let correlation = CorrelationId::parse("tick-1").expect("correlation");
        let bounds = rect(0, 0);
        let source = workspace_domain("mon-a", "ws-1", bounds, 8);
        let target = workspace_domain("mon-a", "ws-2", bounds, 8);
        for follow in [true, false] {
            let rows = vec![
                OwnerRow {
                    token: "w-mover".to_owned(),
                    rect: bounds,
                    hints: tiler_core::size_hints::WindowSizeHints::none(),
                    floating: false,
                },
                OwnerRow {
                    token: "w-float".to_owned(),
                    rect: bounds,
                    hints: tiler_core::size_hints::WindowSizeHints::none(),
                    floating: true,
                },
            ];
            let mut event = build_send_event(
                &owner,
                &generation,
                &correlation,
                0,
                5,
                source.clone(),
                target.clone(),
                &rows,
                &[],
                "w-mover",
                8,
                follow,
            )
            .expect("event");
            stamp_send_target(&mut event, &target.1);
            assert!(
                event
                    .windows
                    .iter()
                    .find(|w| w.window.0 == "w-float")
                    .expect("float row")
                    .floating,
                "follow={follow}: float survivor rides"
            );
            match &event.command {
                CoreCommand::SendToWorkspace {
                    window,
                    follow: actual,
                    ..
                } => {
                    assert_eq!(window, "w-mover");
                    assert_eq!(*actual, follow, "explicit intent, never inferred");
                }
                _ => panic!("expected send-to-workspace"),
            }
        }
    }

    /// R-MOV-03 retained-Engine coverage through the Windows reconcile route.
    ///
    /// The rig builds `CoreEvent`s exactly like `keyboard_tick` does
    /// (`build_reconcile_event_for` plus a `CoreCommand::Move` carrying the
    /// typed mode) and drives the retained `Engine`, so these tests prove the
    /// Windows plumbing honors `core.same_axis_move`. Core planning itself is
    /// owned by `tiler-core` (`session_movement.rs` `flat_swap_*`, `r2c_*`);
    /// what is proven here is mode threading plus the observable Windows-side
    /// geometry/topology/focus outcomes. The admitted 4-window tree nests, so
    /// the R2b insert grows the vertical pair `(w1, w2)` into the N-ary R2c
    /// position; moves then run along that vertical axis.
    mod same_axis_move {
        use std::collections::BTreeMap;

        use tiler_core::boundary::{CoreCommand, CoreEvent, CoreReply, MovePlanReply};
        use tiler_core::directional::{Node, Rule, SameAxisMove, WindowId};
        use tiler_core::engine::Engine;
        use tiler_core::geometry::Rect;
        use tiler_core::ids::{CorrelationId, GenerationId, OwnerId};
        use tiler_core::session::DomainKey;

        use super::workspace_domain;
        use crate::settings::Settings;

        struct Rig {
            engine: Engine,
            owner: OwnerId,
            generation: GenerationId,
            domain: (tiler_core::session::OutputDomain, DomainKey),
            rects: Vec<(String, Rect)>,
            focused: String,
            corr: u64,
        }

        impl Rig {
            fn seed_n(names: &[&str]) -> Self {
                let bounds = Rect {
                    x: 0,
                    y: 0,
                    w: 1600,
                    h: 900,
                };
                let pairs: Vec<(&str, Rect)> = names.iter().map(|w| (*w, bounds)).collect();
                Self::seed_rects(&pairs)
            }

            fn seed_rects(pairs: &[(&str, Rect)]) -> Self {
                let mut engine = Engine::new();
                let owner = OwnerId::parse("tiler-windows").expect("owner");
                let generation = GenerationId::parse("aa").expect("generation");
                engine.sync_binding(&owner, &generation);
                let bounds = Rect {
                    x: 0,
                    y: 0,
                    w: 1600,
                    h: 900,
                };
                let domain = workspace_domain("mon-a", "ws-1", bounds, 8);
                let last = (*pairs.last().expect("names").0).to_owned();
                let mut rig = Self {
                    engine,
                    owner,
                    generation,
                    domain,
                    rects: pairs.iter().map(|(w, r)| ((*w).to_owned(), *r)).collect(),
                    focused: last,
                    corr: 0,
                };
                let seed = rig.event(CoreCommand::Reconcile);
                let reply = rig.engine.handle(&seed);
                assert!(
                    matches!(reply, CoreReply::Projection(_) | CoreReply::Tiled(_)),
                    "seed must converge, got {reply:?}"
                );
                rig
            }

            fn seed4() -> Self {
                Self::seed_n(&["w1", "w2", "w3", "w4"])
            }

            /// Seed 4, walk focus to w1, grow the V group via the R2b insert:
            /// w1's up-neighbor is then the direct leaf sibling w2 (R2c pair).
            fn setup_vertical_pair() -> Self {
                let mut rig = Self::seed4();
                rig.focus("left");
                rig.focus("left");
                let reply = rig.mv("w1", "right", SameAxisMove::GroupWithNeighbor);
                assert!(
                    matches!(reply, CoreReply::MoveDirectional(_)),
                    "setup insert must plan, got {reply:?}"
                );
                rig
            }

            fn event(&mut self, command: CoreCommand) -> CoreEvent {
                self.corr += 1;
                let revision = self
                    .engine
                    .session(&self.domain.1)
                    .map(|s| s.accepted_revision())
                    .unwrap_or(0);
                let fp = crate::tiling::fingerprint(&self.rects);
                let correlation =
                    CorrelationId::parse(&format!("sam-{}", self.corr)).expect("correlation");
                let windows: Vec<(WindowId, Rect, tiler_core::size_hints::WindowSizeHints)> = self
                    .rects
                    .iter()
                    .map(|(w, r)| {
                        (
                            WindowId(w.clone()),
                            *r,
                            tiler_core::size_hints::WindowSizeHints::none(),
                        )
                    })
                    .collect();
                let mut event = crate::tiling::build_reconcile_event_for(
                    &self.owner,
                    &self.generation,
                    &correlation,
                    revision,
                    fp,
                    &self.domain.0,
                    &self.domain.1,
                    8,
                    &windows,
                    Some(&WindowId(self.focused.clone())),
                );
                event.command = command;
                event
            }

            fn focus(&mut self, direction: &str) -> CoreReply {
                let window = self.focused.clone();
                let event = self.event(CoreCommand::Focus {
                    window,
                    direction: direction.to_owned(),
                    cross_output_transfer: false,
                    float_subject: false,
                });
                let reply = self.engine.handle(&event);
                if let CoreReply::FocusDirectional(plan) = &reply {
                    self.focused = plan.to_window.0.clone();
                    self.rects = plan
                        .geometry
                        .iter()
                        .map(|g| (g.window.0.clone(), g.rect))
                        .collect();
                }
                reply
            }

            fn mv(&mut self, window: &str, direction: &str, mode: SameAxisMove) -> CoreReply {
                let event = self.event(CoreCommand::Move {
                    window: window.to_owned(),
                    direction: direction.to_owned(),
                    cross_output_transfer: false,
                    same_axis_move: mode,
                });
                let reply = self.engine.handle(&event);
                if let CoreReply::MoveDirectional(plan) = &reply {
                    self.rects = plan
                        .geometry
                        .iter()
                        .map(|g| (g.window.0.clone(), g.rect))
                        .collect();
                }
                reply
            }

            fn rz(&mut self, window: &str, direction: &str, mode: &str) -> CoreReply {
                let event = self.event(CoreCommand::Resize {
                    window: window.to_owned(),
                    direction: direction.to_owned(),
                    mode: mode.to_owned(),
                    press_index: 0,
                });
                let reply = self.engine.handle(&event);
                if let CoreReply::Resize(plan) = &reply {
                    self.rects = plan
                        .geometry
                        .iter()
                        .map(|g| (g.window.0.clone(), g.rect))
                        .collect();
                }
                reply
            }

            fn snapshot(&self) -> tiler_core::session::SessionSnapshot {
                self.engine
                    .session(&self.domain.1)
                    .expect("session")
                    .snapshot()
            }

            fn root_group(&self) -> (Vec<String>, Vec<u64>) {
                let snap = self.snapshot();
                let Some(Node::Group {
                    children, shares, ..
                }) = &snap.domains[0].tree
                else {
                    panic!("root must be a group, got {:?}", snap.domains[0].tree);
                };
                (
                    children.iter().map(|c| c.id().0.clone()).collect(),
                    shares.clone(),
                )
            }
        }

        fn move_plan(reply: CoreReply, what: &str) -> MovePlanReply {
            match reply {
                CoreReply::MoveDirectional(plan) => plan,
                other => panic!("{what} must plan directionally, got {other:?}"),
            }
        }

        fn geom(plan: &MovePlanReply, window: &str) -> Rect {
            plan.geometry
                .iter()
                .find(|g| g.window.0 == window)
                .unwrap_or_else(|| panic!("geometry for {window}"))
                .rect
        }

        fn leaf_id(window: &str) -> String {
            format!("leaf-{window}")
        }

        #[test]
        fn swap_exchanges_unequal_pair_both_directions_with_traveling_shares() {
            // Every move below threads its mode from live settings text, the
            // same selection `keyboard_tick` makes per move.
            let mut swap_settings = Settings::default();
            swap_settings.core.same_axis_move = crate::settings::SAME_AXIS_MOVE_SWAP.to_owned();
            crate::settings::validate_settings(&swap_settings).expect("valid token");
            let swap = swap_settings.core.same_axis_move_mode();
            assert_eq!(swap, SameAxisMove::SwapWithNeighbor);

            let mut rig = Rig::setup_vertical_pair();
            // Equal-share pair first: the swap exchanges positions exactly,
            // keeps every share, adds no wrapper, and retains focus.
            let pre_w1 = geom_of(&rig.rects, "w1");
            let pre_w2 = geom_of(&rig.rects, "w2");
            let plan = move_plan(rig.mv("w1", "up", swap), "swap up");
            assert_eq!(plan.rule, Rule::R2c);
            assert_eq!(plan.focus_leaf.0, leaf_id("w1"));
            assert_eq!(geom(&plan, "w1"), pre_w2);
            assert_eq!(geom(&plan, "w2"), pre_w1);
            let (children, shares) = rig.root_group();
            assert_eq!(children.len(), 3, "no wrapper group appears");
            assert_eq!(shares, vec![1, 1, 1]);
            // Unbalance the pair through the ordinary keyboard-resize path,
            // then swap down: extents travel with their windows.
            let resize = rig.rz("w1", "down", "outwards");
            assert!(
                matches!(resize, CoreReply::Resize(_)),
                "resize must plan, got {resize:?}"
            );
            let (children, shares) = rig.root_group();
            assert_eq!(children[..2], [leaf_id("w1"), leaf_id("w2")]);
            assert_ne!(shares[0], shares[1], "pair must be unbalanced");
            let pre = rig.rects.clone();
            let plan = move_plan(rig.mv("w1", "down", swap), "unequal swap down");
            assert_eq!(plan.rule, Rule::R2c);
            assert_eq!(plan.focus_leaf.0, leaf_id("w1"));
            assert_eq!(geom(&plan, "w1").h, geom_of(&pre, "w1").h);
            assert_eq!(geom(&plan, "w1").w, geom_of(&pre, "w1").w);
            assert_eq!(geom(&plan, "w2").h, geom_of(&pre, "w2").h);
            assert_eq!(geom(&plan, "w2").w, geom_of(&pre, "w2").w);
            let (children, post_shares) = rig.root_group();
            assert_eq!(children[..2], [leaf_id("w2"), leaf_id("w1")]);
            assert_eq!(post_shares[0], shares[1]);
            assert_eq!(post_shares[1], shares[0]);
            assert_eq!(post_shares[2], shares[2]);
            // Opposite direction restores order, shares, and geometry exactly.
            let back = move_plan(rig.mv("w1", "up", swap), "swap back up");
            assert_eq!(back.rule, Rule::R2c);
            assert_eq!(back.focus_leaf.0, leaf_id("w1"));
            let (children, restored) = rig.root_group();
            assert_eq!(children[..2], [leaf_id("w1"), leaf_id("w2")]);
            assert_eq!(restored, shares);
            for (window, rect) in &pre {
                assert_eq!(geom(&back, window), *rect, "{window} restores");
            }
        }

        #[test]
        fn group_wrap_default_halves_combined_extent_without_reordering() {
            let group = Settings::default().core.same_axis_move_mode();
            assert_eq!(group, SameAxisMove::GroupWithNeighbor);
            let mut rig = Rig::setup_vertical_pair();
            let pre_w1 = geom_of(&rig.rects, "w1");
            let pre_w2 = geom_of(&rig.rects, "w2");
            let start = pre_w2.y;
            let end = pre_w1.y + pre_w1.h;
            let plan = move_plan(rig.mv("w1", "up", group), "wrap up");
            assert_eq!(plan.rule, Rule::R2c);
            assert_eq!(plan.focus_leaf.0, leaf_id("w1"));
            // Wrapper keeps the pair combined extent and halves inside, with
            // the neighbor first (the mover moved up into it from below).
            let snap = rig.snapshot();
            let Some(Node::Group {
                children, shares, ..
            }) = &snap.domains[0].tree
            else {
                panic!("root must be a group");
            };
            assert_eq!(*shares, vec![2, 1]);
            let [wrapper, _] = children.as_slice() else {
                panic!("root must keep two children, got {children:?}");
            };
            let Node::Group {
                children: inner,
                shares: inner_shares,
                ..
            } = wrapper
            else {
                panic!("first root child must be the wrapper, got {wrapper:?}");
            };
            assert_eq!(
                inner.iter().map(|c| c.id().0.clone()).collect::<Vec<_>>(),
                [leaf_id("w2"), leaf_id("w1")]
            );
            assert_eq!(*inner_shares, vec![1, 1]);
            let post_w1 = geom(&plan, "w1");
            let post_w2 = geom(&plan, "w2");
            assert_eq!(post_w2.y, start, "top edge retained");
            assert_eq!(post_w1.h, post_w2.h, "halves inside");
            assert_eq!(post_w1.w, pre_w1.w);
            // Bottom edge retains the combined extent up to core projection
            // rounding (one inner gap); the topology pins the wrap outcome.
            assert!(
                (post_w1.y + post_w1.h).abs_diff(end) <= 8,
                "combined extent retained: {} vs {end}",
                post_w1.y + post_w1.h
            );
        }

        #[test]
        fn mode_change_alone_mutates_no_topology_until_next_move() {
            let mut rig = Rig::setup_vertical_pair();
            let before = rig.snapshot();
            // Flipping only this setting touches no Engine state: the
            // validated document changes, the retained session does not.
            let mut settings = Settings::default();
            assert_eq!(
                settings.core.same_axis_move_mode(),
                SameAxisMove::GroupWithNeighbor
            );
            settings.core.same_axis_move = crate::settings::SAME_AXIS_MOVE_SWAP.to_owned();
            crate::settings::validate_settings(&settings).expect("valid token");
            assert_eq!(rig.snapshot(), before);
            // The next move then uses the new mode: a flat swap, no wrapper.
            let plan = move_plan(
                rig.mv("w1", "up", settings.core.same_axis_move_mode()),
                "post-change move",
            );
            assert_eq!(plan.rule, Rule::R2c);
            let (children, _) = rig.root_group();
            assert_eq!(children.len(), 3, "no wrapper group appears");
            assert_eq!(children[0], leaf_id("w1"));
        }

        #[test]
        fn binary_group_neighbor_and_boundary_parity_across_modes() {
            // Binary R2a: identical plan under both modes (mover is the
            // seed-focused w2, moving left).
            let plan_group = move_plan(
                Rig::seed_n(&["w1", "w2"]).mv("w2", "left", SameAxisMove::GroupWithNeighbor),
                "binary group",
            );
            let plan_swap = move_plan(
                Rig::seed_n(&["w1", "w2"]).mv("w2", "left", SameAxisMove::SwapWithNeighbor),
                "binary swap",
            );
            assert_eq!(plan_group.rule, Rule::R2a);
            assert_eq!(plan_group, plan_swap);
            // Group neighbor (R2b insertion after the R1 wrap): identical.
            let mut rig_group = Rig::seed4();
            rig_group.focus("left");
            rig_group.focus("left");
            rig_group.mv("w1", "right", SameAxisMove::GroupWithNeighbor);
            rig_group.mv("w1", "right", SameAxisMove::GroupWithNeighbor);
            let left_group = rig_group.mv("w1", "left", SameAxisMove::GroupWithNeighbor);
            let mut rig_swap = Rig::seed4();
            rig_swap.focus("left");
            rig_swap.focus("left");
            rig_swap.mv("w1", "right", SameAxisMove::GroupWithNeighbor);
            rig_swap.mv("w1", "right", SameAxisMove::GroupWithNeighbor);
            let left_swap = rig_swap.mv("w1", "left", SameAxisMove::SwapWithNeighbor);
            assert_eq!(left_group, left_swap);
            assert_eq!(move_plan(left_group, "group neighbor").rule, Rule::R2b);
            // Sole root leaf boundary: identical refusal under both modes.
            let lone_group =
                Rig::seed_n(&["w1"]).mv("w1", "right", SameAxisMove::GroupWithNeighbor);
            let lone_swap = Rig::seed_n(&["w1"]).mv("w1", "right", SameAxisMove::SwapWithNeighbor);
            assert_eq!(lone_group, lone_swap);
            assert!(
                matches!(lone_group, CoreReply::Rejected { .. }),
                "sole leaf refuses, got {lone_group:?}"
            );
        }

        fn geom_of(rects: &[(String, Rect)], window: &str) -> Rect {
            rects
                .iter()
                .find(|(w, _)| w == window)
                .unwrap_or_else(|| panic!("rect for {window}"))
                .1
        }

        /// Window to retained-leaf map from the session snapshot links: the
        /// fit admission names leaves `fit-lN`, so tests must resolve the
        /// mover's leaf instead of assuming `leaf-{window}`.
        fn leaf_map(rig: &Rig) -> BTreeMap<String, String> {
            rig.snapshot()
                .windows
                .iter()
                .map(|link| (link.window.0.clone(), link.leaf.0.clone()))
                .collect()
        }

        /// Rect equality up to core projection rounding: reprojected slot
        /// geometry lands within a pixel of the observed share rects.
        fn assert_rect_near(actual: Rect, expected: Rect, what: &str) {
            for (a, e, axis) in [
                (actual.x, expected.x, "x"),
                (actual.y, expected.y, "y"),
                (actual.w, expected.w, "w"),
                (actual.h, expected.h, "h"),
            ] {
                assert!(
                    a.abs_diff(e) <= 1,
                    "{what}: {axis} {a} vs {e} ({actual:?} vs {expected:?})"
                );
            }
        }

        /// Seed the checklist row: four pre-tiled unequal columns admit as a
        /// flat horizontal N-ary group `H[w1,w2,w3,w4]` with proportional
        /// shares `[300,500,376,400]`, then walk focus to `w2` (`B*`).
        fn setup_checklist_row() -> (Rig, BTreeMap<String, String>) {
            let cols = [
                (
                    "w1",
                    Rect {
                        x: 0,
                        y: 0,
                        w: 300,
                        h: 900,
                    },
                ),
                (
                    "w2",
                    Rect {
                        x: 308,
                        y: 0,
                        w: 500,
                        h: 900,
                    },
                ),
                (
                    "w3",
                    Rect {
                        x: 816,
                        y: 0,
                        w: 376,
                        h: 900,
                    },
                ),
                (
                    "w4",
                    Rect {
                        x: 1200,
                        y: 0,
                        w: 400,
                        h: 900,
                    },
                ),
            ];
            let mut rig = Rig::seed_rects(&cols);
            let leaves = leaf_map(&rig);
            assert_eq!(
                rig.root_group(),
                (
                    vec![
                        leaves["w1"].clone(),
                        leaves["w2"].clone(),
                        leaves["w3"].clone(),
                        leaves["w4"].clone(),
                    ],
                    vec![300, 500, 376, 400],
                ),
                "checklist row must admit flat with proportional shares"
            );
            for (step, want) in [("left", "w3"), ("left", "w2")] {
                match rig.focus(step) {
                    CoreReply::FocusDirectional(plan) => {
                        assert_eq!(plan.to_window.0, want, "focus walk step");
                    }
                    other => panic!("focus walk must plan, got {other:?}"),
                }
            }
            assert_eq!(rig.focused, "w2");
            (rig, leaves)
        }

        #[test]
        fn horizontal_swaps_both_directions_with_traveling_shares() {
            // Every move threads its mode from live settings text, the same
            // selection `keyboard_tick` makes per move.
            let mut swap_settings = Settings::default();
            swap_settings.core.same_axis_move = crate::settings::SAME_AXIS_MOVE_SWAP.to_owned();
            crate::settings::validate_settings(&swap_settings).expect("valid token");
            let swap = swap_settings.core.same_axis_move_mode();
            assert_eq!(swap, SameAxisMove::SwapWithNeighbor);

            // Right: `H[w1,w2*,w3,w4]` -> `H[w1,w3,w2*,w4]`, shares travel.
            // Slots reproject from the new share order, so positions derive
            // from order while widths travel with their windows.
            let (mut rig, leaves) = setup_checklist_row();
            let pre = rig.rects.clone();
            let plan = move_plan(rig.mv("w2", "right", swap), "swap right");
            assert_eq!(plan.rule, Rule::R2c);
            assert_eq!(plan.focus_leaf.0, leaves["w2"]);
            assert_eq!(plan.geometry.len(), 4);
            let (children, shares) = rig.root_group();
            assert_eq!(
                children,
                [
                    leaves["w1"].clone(),
                    leaves["w3"].clone(),
                    leaves["w2"].clone(),
                    leaves["w4"].clone()
                ]
            );
            assert_eq!(
                shares,
                vec![300, 376, 500, 400],
                "shares travel with windows"
            );
            assert!(geom(&plan, "w2").w.abs_diff(geom_of(&pre, "w2").w) <= 1);
            assert!(geom(&plan, "w3").w.abs_diff(geom_of(&pre, "w3").w) <= 1);
            assert!(
                geom(&plan, "w2").x > geom_of(&pre, "w2").x,
                "w2 moves right"
            );
            assert!(geom(&plan, "w3").x < geom_of(&pre, "w3").x, "w3 moves left");
            assert!(geom(&plan, "w1").x < geom(&plan, "w3").x);
            assert!(geom(&plan, "w3").x < geom(&plan, "w2").x);
            assert!(geom(&plan, "w2").x < geom(&plan, "w4").x);
            // Left back: exact order, shares, and focus restore.
            let back = move_plan(rig.mv("w2", "left", swap), "swap left back");
            assert_eq!(back.rule, Rule::R2c);
            assert_eq!(back.focus_leaf.0, leaves["w2"]);
            let (children, shares) = rig.root_group();
            assert_eq!(
                children,
                [
                    leaves["w1"].clone(),
                    leaves["w2"].clone(),
                    leaves["w3"].clone(),
                    leaves["w4"].clone()
                ]
            );
            assert_eq!(shares, vec![300, 500, 376, 400]);
            for (window, rect) in &pre {
                assert_rect_near(geom(&back, window), *rect, &format!("{window} restores"));
            }
            // Left into the most unequal pair (500 vs 300), then right back.
            let plan = move_plan(rig.mv("w2", "left", swap), "swap left");
            assert_eq!(plan.rule, Rule::R2c);
            assert_eq!(plan.focus_leaf.0, leaves["w2"]);
            let (children, shares) = rig.root_group();
            assert_eq!(
                children,
                [
                    leaves["w2"].clone(),
                    leaves["w1"].clone(),
                    leaves["w3"].clone(),
                    leaves["w4"].clone()
                ]
            );
            assert_eq!(
                shares,
                vec![500, 300, 376, 400],
                "shares travel with windows"
            );
            assert!(geom(&plan, "w2").w.abs_diff(geom_of(&pre, "w2").w) <= 1);
            assert!(geom(&plan, "w1").w.abs_diff(geom_of(&pre, "w1").w) <= 1);
            assert!(geom(&plan, "w2").x < geom_of(&pre, "w2").x, "w2 moves left");
            assert!(
                geom(&plan, "w1").x > geom_of(&pre, "w1").x,
                "w1 moves right"
            );
            let back = move_plan(rig.mv("w2", "right", swap), "swap right back");
            assert_eq!(back.rule, Rule::R2c);
            assert_eq!(back.focus_leaf.0, leaves["w2"]);
            let (children, shares) = rig.root_group();
            assert_eq!(
                children,
                [
                    leaves["w1"].clone(),
                    leaves["w2"].clone(),
                    leaves["w3"].clone(),
                    leaves["w4"].clone()
                ]
            );
            assert_eq!(shares, vec![300, 500, 376, 400]);
            for (window, rect) in &pre {
                assert_rect_near(geom(&back, window), *rect, &format!("{window} restores"));
            }
        }

        #[test]
        fn horizontal_wrap_default_groups_neighbor_pair() {
            let group = Settings::default().core.same_axis_move_mode();
            assert_eq!(group, SameAxisMove::GroupWithNeighbor);
            let (mut rig, leaves) = setup_checklist_row();
            let pre_w2 = geom_of(&rig.rects, "w2");
            let pre_w3 = geom_of(&rig.rects, "w3");
            let start = pre_w2.x;
            let end = pre_w3.x + pre_w3.w;
            // Default right wraps the pair: `H[w1,H[w2*,w3],w4]`.
            let plan = move_plan(rig.mv("w2", "right", group), "wrap right");
            assert_eq!(plan.rule, Rule::R2c);
            assert_eq!(plan.focus_leaf.0, leaves["w2"]);
            let snap = rig.snapshot();
            let Some(Node::Group {
                children, shares, ..
            }) = &snap.domains[0].tree
            else {
                panic!("root must be a group");
            };
            assert_eq!(
                *shares,
                vec![300, 876, 400],
                "wrapper carries the summed pair share"
            );
            let [first, wrapper, _] = children.as_slice() else {
                panic!("root must keep three children, got {children:?}");
            };
            assert_eq!(first.id().0, leaves["w1"]);
            let Node::Group {
                children: inner,
                shares: inner_shares,
                ..
            } = wrapper
            else {
                panic!("middle root child must be the wrapper, got {wrapper:?}");
            };
            assert_eq!(
                inner.iter().map(|c| c.id().0.clone()).collect::<Vec<_>>(),
                [leaves["w2"].clone(), leaves["w3"].clone()]
            );
            // New splits inside the wrapper are `[1, 1]`; the summed pair
            // share rides the wrapper itself (outer `[300, 876, 400]`).
            assert_eq!(*inner_shares, vec![1, 1]);
            // Wrapper keeps the pair combined extent and halves inside.
            let post_w2 = geom(&plan, "w2");
            let post_w3 = geom(&plan, "w3");
            assert!(post_w2.x.abs_diff(start) <= 1, "left edge retained");
            assert!(
                (post_w3.x + post_w3.w).abs_diff(end) <= 8,
                "combined extent retained"
            );
            assert!(
                post_w2.w.abs_diff(post_w3.w) <= 1,
                "halves inside: {} vs {}",
                post_w2.w,
                post_w3.w
            );
        }

        #[test]
        fn nested_escape_and_edge_parity_across_modes() {
            // Nested R3 escape (depth-2 leaf at its group edge): identical.
            let escape_group = Rig::seed4();
            let mut escape_group = escape_group;
            escape_group.focus("left");
            let escape_group = escape_group.mv("w3", "left", SameAxisMove::GroupWithNeighbor);
            let mut escape_swap = Rig::seed4();
            escape_swap.focus("left");
            let escape_swap = escape_swap.mv("w3", "left", SameAxisMove::SwapWithNeighbor);
            assert_eq!(escape_group, escape_swap);
            let plan = move_plan(escape_group, "nested escape");
            assert_eq!(plan.rule, Rule::R3);
            // Perpendicular R1 wrap at a nested edge: identical.
            let wrap_group =
                Rig::setup_vertical_pair().mv("w1", "right", SameAxisMove::GroupWithNeighbor);
            let wrap_swap =
                Rig::setup_vertical_pair().mv("w1", "right", SameAxisMove::SwapWithNeighbor);
            assert_eq!(wrap_group, wrap_swap);
            assert_eq!(move_plan(wrap_group, "nested wrap").rule, Rule::R1);
            // R2c with a group neighbor keeps the wrap under both modes.
            let groupwrap_group =
                Rig::setup_vertical_pair().mv("w1", "down", SameAxisMove::GroupWithNeighbor);
            let groupwrap_swap =
                Rig::setup_vertical_pair().mv("w1", "down", SameAxisMove::SwapWithNeighbor);
            assert_eq!(groupwrap_group, groupwrap_swap);
            assert_eq!(
                move_plan(groupwrap_group, "group-neighbor wrap").rule,
                Rule::R2c
            );
            // Root-edge boundary noop on a group: identical refusal.
            let mut edge_group = Rig::seed_n(&["w1", "w2"]);
            edge_group.focus("left");
            let edge_group = edge_group.mv("w1", "left", SameAxisMove::GroupWithNeighbor);
            let mut edge_swap = Rig::seed_n(&["w1", "w2"]);
            edge_swap.focus("left");
            let edge_swap = edge_swap.mv("w1", "left", SameAxisMove::SwapWithNeighbor);
            assert_eq!(edge_group, edge_swap);
            assert!(
                matches!(edge_group, CoreReply::Rejected { .. }),
                "root edge refuses, got {edge_group:?}"
            );
        }
    }
}

/// R-SPC-04 D1-D8 + G-05 fixed-size admission regressions at the adapter
/// boundary: real retained `Engine` observations through the production
/// builders plus [`stamp_fixed_admission_flags`], the same stamping the
/// production handle path applies. No native calls, no HWNDs.
#[cfg(test)]
mod fixed_admission_tests {
    use super::*;
    use std::collections::HashMap;
    use tiler_core::boundary::{CoreCommand, CoreReply};
    use tiler_core::size_hints::{FixedSizePredicate, WindowSizeHints};

    fn fixed_hints() -> WindowSizeHints {
        WindowSizeHints {
            min_w: Some(640),
            min_h: Some(480),
            max_w: Some(640),
            max_h: Some(480),
        }
    }

    fn partial_w_hints() -> WindowSizeHints {
        WindowSizeHints {
            min_w: Some(640),
            min_h: Some(0),
            max_w: Some(640),
            max_h: Some(0),
        }
    }

    fn partial_h_hints() -> WindowSizeHints {
        WindowSizeHints {
            min_w: Some(0),
            min_h: Some(480),
            max_w: Some(0),
            max_h: Some(480),
        }
    }

    fn rig() -> (
        tiler_core::engine::Engine,
        OwnerId,
        GenerationId,
        (OutputDomain, DomainKey),
        Rect,
    ) {
        let mut engine = tiler_core::engine::Engine::new();
        let owner = OwnerId::parse("tiler-windows").expect("owner");
        let generation = GenerationId::parse("aa").expect("generation");
        engine.sync_binding(&owner, &generation);
        engine.set_fixed_size_admission(true);
        let bounds = Rect {
            x: 0,
            y: 0,
            w: 1920,
            h: 1080,
        };
        let key = DomainKey {
            output: OutputId("mon-a".to_owned()),
            workspace: WorkspaceId("ws-1".to_owned()),
        };
        let domain = OutputDomain {
            id: OutputId("mon-a".to_owned()),
            workspace: WorkspaceId("ws-1".to_owned()),
            bounds,
            gap: 8,
            adjacent: BTreeMap::new(),
        };
        (engine, owner, generation, (domain, key), bounds)
    }

    fn revision_of(engine: &tiler_core::engine::Engine, domain: &(OutputDomain, DomainKey)) -> u64 {
        engine
            .session(&domain.1)
            .map(|s| s.accepted_revision())
            .unwrap_or(0)
    }

    #[allow(clippy::too_many_arguments)]
    fn reconcile(
        engine: &mut tiler_core::engine::Engine,
        owner: &OwnerId,
        generation: &GenerationId,
        domain: &(OutputDomain, DomainKey),
        rows: &[(WindowId, Rect, WindowSizeHints, bool, FixedAdmissionFlags)],
        focused: &str,
        correlation: &str,
    ) -> CoreReply {
        let carried: Vec<(WindowId, Rect, WindowSizeHints, bool)> = rows
            .iter()
            .map(|(id, rect, hints, floating, _)| (id.clone(), *rect, *hints, *floating))
            .collect();
        let fp = crate::tiling::fingerprint(
            &rows
                .iter()
                .map(|(id, rect, _, _, _)| (id.0.clone(), *rect))
                .collect::<Vec<_>>(),
        );
        let correlation = CorrelationId::parse(correlation).expect("correlation");
        let mut event = crate::tiling::build_reconcile_event_for_floating(
            owner,
            generation,
            &correlation,
            revision_of(engine, domain),
            fp,
            &domain.0,
            &domain.1,
            8,
            &carried,
            Some(&WindowId(focused.to_owned())),
        );
        let flags: HashMap<WindowId, FixedAdmissionFlags> = rows
            .iter()
            .map(|(id, _, _, _, flags)| (id.clone(), *flags))
            .collect();
        stamp_fixed_admission_flags(&mut event.windows, &flags);
        engine.handle(&event)
    }

    fn plain_row(
        id: &str,
        bounds: Rect,
    ) -> (WindowId, Rect, WindowSizeHints, bool, FixedAdmissionFlags) {
        (
            WindowId(id.to_owned()),
            bounds,
            WindowSizeHints::none(),
            false,
            FixedAdmissionFlags::default(),
        )
    }

    fn fixed_row(
        id: &str,
        bounds: Rect,
    ) -> (WindowId, Rect, WindowSizeHints, bool, FixedAdmissionFlags) {
        (
            WindowId(id.to_owned()),
            bounds,
            fixed_hints(),
            false,
            FixedAdmissionFlags::default(),
        )
    }

    fn is_tiled(
        engine: &tiler_core::engine::Engine,
        domain: &(OutputDomain, DomainKey),
        id: &str,
    ) -> bool {
        engine.session(&domain.1).is_some_and(|s| {
            s.snapshot().windows.iter().any(|w| w.window.0 == id)
                && !s.is_exception(&WindowId(id.to_owned()))
        })
    }

    fn is_float(
        engine: &tiler_core::engine::Engine,
        domain: &(OutputDomain, DomainKey),
        id: &str,
    ) -> bool {
        engine
            .session(&domain.1)
            .is_some_and(|s| s.is_exception(&WindowId(id.to_owned())))
    }

    fn is_automatic(
        engine: &tiler_core::engine::Engine,
        domain: &(OutputDomain, DomainKey),
        id: &str,
    ) -> bool {
        engine
            .session(&domain.1)
            .is_some_and(|s| s.is_automatic_fixed_float(&WindowId(id.to_owned())))
    }

    fn has_override(
        engine: &tiler_core::engine::Engine,
        domain: &(OutputDomain, DomainKey),
        id: &str,
    ) -> bool {
        engine
            .session(&domain.1)
            .is_some_and(|s| s.has_fixed_tile_override(&WindowId(id.to_owned())))
    }

    #[test]
    fn tile_override_marker_hydrates_only_with_full_fences() {
        use crate::tiling::{parse_tile_override_marker, tile_override_marker_value};
        use crate::workspace_owner::{
            MarkerSlot, RestartMarker, classify_restart_marker, restart_tile_override_eligible,
        };
        assert!(parse_tile_override_marker(tile_override_marker_value()).is_some());
        assert!(parse_tile_override_marker(0).is_none());
        let tile = classify_restart_marker(
            MarkerSlot::Absent,
            MarkerSlot::Absent,
            MarkerSlot::Value(tile_override_marker_value()),
        );
        assert_eq!(tile, RestartMarker::TileOverride);
        // Full fences hydrate; any missing fence fails closed with no
        // intent (recompute fallback covers the window through its hints).
        assert!(restart_tile_override_eligible(true, true, true, tile));
        assert!(!restart_tile_override_eligible(false, true, true, tile));
        assert!(!restart_tile_override_eligible(true, false, true, tile));
        assert!(!restart_tile_override_eligible(true, true, false, tile));
        // Every other lane never overrides here.
        for marker in [
            RestartMarker::Absent,
            RestartMarker::FloatIntent,
            RestartMarker::Sticky(false),
            RestartMarker::Sticky(true),
            RestartMarker::Corrupt,
            RestartMarker::Unreadable,
        ] {
            assert!(
                !restart_tile_override_eligible(true, true, true, marker),
                "{marker:?} never hydrates an override"
            );
        }
    }

    #[test]
    fn stamp_helper_sets_only_listed_windows() {
        let bounds = Rect {
            x: 0,
            y: 0,
            w: 800,
            h: 600,
        };
        let correlation = CorrelationId::parse("stamp-1").expect("correlation");
        let owner = OwnerId::parse("tiler-windows").expect("owner");
        let generation = GenerationId::parse("aa").expect("generation");
        let key = DomainKey {
            output: OutputId("mon-a".to_owned()),
            workspace: WorkspaceId("ws-1".to_owned()),
        };
        let domain = OutputDomain {
            id: OutputId("mon-a".to_owned()),
            workspace: WorkspaceId("ws-1".to_owned()),
            bounds,
            gap: 8,
            adjacent: BTreeMap::new(),
        };
        let mut event = crate::tiling::build_reconcile_event_for_floating(
            &owner,
            &generation,
            &correlation,
            0,
            7,
            &domain,
            &key,
            8,
            &[
                (
                    WindowId("w1".to_owned()),
                    bounds,
                    WindowSizeHints::none(),
                    false,
                ),
                (
                    WindowId("w2".to_owned()),
                    bounds,
                    WindowSizeHints::none(),
                    true,
                ),
            ],
            Some(&WindowId("w1".to_owned())),
        );
        for window in &event.windows {
            assert!(!window.fullscreen);
            assert!(!window.fixed_auto);
            assert!(!window.fixed_suppress);
        }
        let flags: HashMap<WindowId, FixedAdmissionFlags> = [(
            WindowId("w2".to_owned()),
            FixedAdmissionFlags {
                fullscreen: true,
                fixed_auto: true,
                fixed_suppress: true,
            },
        )]
        .into_iter()
        .collect();
        stamp_fixed_admission_flags(&mut event.windows, &flags);
        let w1 = event
            .windows
            .iter()
            .find(|w| w.window.0 == "w1")
            .expect("w1");
        let w2 = event
            .windows
            .iter()
            .find(|w| w.window.0 == "w2")
            .expect("w2");
        assert!(!w1.fullscreen && !w1.fixed_auto && !w1.fixed_suppress);
        assert!(w2.fullscreen && w2.fixed_auto && w2.fixed_suppress);
        // Sticky/maximized stay adapter-owned (false) through the stamp.
        assert!(!w2.sticky);
        assert!(!w2.maximized);
    }

    #[test]
    fn fixed_brand_new_floats_automatic_with_bounded_report_and_no_geometry() {
        let (mut engine, owner, generation, domain, bounds) = rig();
        let reply = reconcile(
            &mut engine,
            &owner,
            &generation,
            &domain,
            &[fixed_row("wfix", bounds), plain_row("w1", bounds)],
            "w1",
            "reconcile-1",
        );
        assert!(
            !matches!(reply, CoreReply::Rejected { .. } | CoreReply::Diverged(_)),
            "admission converges, got {reply:?}"
        );
        assert!(is_float(&engine, &domain, "wfix"), "fixed floats");
        assert!(is_automatic(&engine, &domain, "wfix"), "automatic mark");
        assert!(is_tiled(&engine, &domain, "w1"), "ordinary tiles");
        // D8: the automatic admission plans no native target writes: only
        // the ordinary tile takes geometry.
        if let CoreReply::Tiled(plan) = &reply {
            assert!(
                plan.geometry.iter().all(|g| g.window.0 != "wfix"),
                "no geometry for the automatic float"
            );
            assert!(
                plan.geometry.iter().any(|g| g.window.0 == "w1"),
                "ordinary tile placed"
            );
        }
        let report = engine.last_fixed_admission().expect("bounded report");
        assert_eq!(report.reason, "fixed-equal");
        assert_eq!(report.admitted, 1);
    }

    #[test]
    fn partial_zero_tiles_under_default_and_floats_under_either_axis() {
        // G-05: zero is unset per axis. Both partial-zero orientations tile
        // under both-axes-fixed and float through the nonzero axis under
        // either-axis-fixed. Missing and full-zero tile under both.
        for (name, hints) in [("width", partial_w_hints()), ("height", partial_h_hints())] {
            let (mut engine, owner, generation, domain, bounds) = rig();
            let rows = vec![
                (
                    WindowId(format!("p-{name}")),
                    bounds,
                    hints,
                    false,
                    FixedAdmissionFlags::default(),
                ),
                plain_row("w1", bounds),
            ];
            let reply = reconcile(
                &mut engine,
                &owner,
                &generation,
                &domain,
                &rows,
                "w1",
                "reconcile-1",
            );
            assert!(
                !matches!(reply, CoreReply::Rejected { .. } | CoreReply::Diverged(_)),
                "{name} converges, got {reply:?}"
            );
            assert!(
                is_tiled(&engine, &domain, &format!("p-{name}")),
                "{name} partial-zero tiles under the default"
            );
            assert!(
                engine.last_fixed_admission().is_none(),
                "{name} default admits nothing automatic"
            );
            engine.set_fixed_size_predicate(FixedSizePredicate::EitherAxis);
            let rows = vec![
                (
                    WindowId(format!("q-{name}")),
                    bounds,
                    hints,
                    false,
                    FixedAdmissionFlags::default(),
                ),
                plain_row("w1", bounds),
            ];
            let reply = reconcile(
                &mut engine,
                &owner,
                &generation,
                &domain,
                &rows,
                "w1",
                "reconcile-2",
            );
            assert!(
                !matches!(reply, CoreReply::Rejected { .. } | CoreReply::Diverged(_)),
                "{name} either-axis converges, got {reply:?}"
            );
            assert!(
                is_automatic(&engine, &domain, &format!("q-{name}")),
                "{name} partial-zero floats under either-axis"
            );
        }
        // Missing and full-zero tile under both predicates.
        for hints in [
            WindowSizeHints::none(),
            WindowSizeHints {
                min_w: Some(0),
                min_h: Some(0),
                max_w: Some(0),
                max_h: Some(0),
            },
        ] {
            for predicate in [FixedSizePredicate::BothAxes, FixedSizePredicate::EitherAxis] {
                let (mut engine, owner, generation, domain, bounds) = rig();
                engine.set_fixed_size_predicate(predicate);
                let rows = vec![
                    (
                        WindowId("z".to_owned()),
                        bounds,
                        hints,
                        false,
                        FixedAdmissionFlags::default(),
                    ),
                    plain_row("w1", bounds),
                ];
                reconcile(
                    &mut engine,
                    &owner,
                    &generation,
                    &domain,
                    &rows,
                    "w1",
                    "reconcile-1",
                );
                assert!(
                    is_tiled(&engine, &domain, "z"),
                    "zero/missing tiles under {predicate:?}"
                );
            }
        }
    }

    #[test]
    fn predicate_switch_never_reclassifies_retained_windows() {
        // D2: a one-axis-fixed window tiles under the default; switching to
        // either-axis leaves the retained tile alone. Only subsequent
        // admissions use the new predicate.
        let (mut engine, owner, generation, domain, bounds) = rig();
        let one_axis = WindowSizeHints {
            min_w: Some(640),
            min_h: Some(480),
            max_w: Some(640),
            max_h: Some(960),
        };
        let rows = vec![
            (
                WindowId("wone".to_owned()),
                bounds,
                one_axis,
                false,
                FixedAdmissionFlags::default(),
            ),
            plain_row("w1", bounds),
        ];
        reconcile(
            &mut engine,
            &owner,
            &generation,
            &domain,
            &rows,
            "w1",
            "reconcile-1",
        );
        assert!(is_tiled(&engine, &domain, "wone"));
        engine.set_fixed_size_predicate(FixedSizePredicate::EitherAxis);
        let reply = reconcile(
            &mut engine,
            &owner,
            &generation,
            &domain,
            &rows,
            "w1",
            "reconcile-2",
        );
        assert!(
            !matches!(reply, CoreReply::Rejected { .. } | CoreReply::Diverged(_)),
            "switch converges, got {reply:?}"
        );
        assert!(
            is_tiled(&engine, &domain, "wone"),
            "retained tile survives the switch"
        );
        assert!(!is_automatic(&engine, &domain, "wone"));
        // A subsequent identical admission floats under the new predicate.
        let rows = vec![
            (
                WindowId("wone".to_owned()),
                bounds,
                one_axis,
                false,
                FixedAdmissionFlags::default(),
            ),
            (
                WindowId("wtwo".to_owned()),
                bounds,
                one_axis,
                false,
                FixedAdmissionFlags::default(),
            ),
            plain_row("w1", bounds),
        ];
        reconcile(
            &mut engine,
            &owner,
            &generation,
            &domain,
            &rows,
            "w1",
            "reconcile-3",
        );
        assert!(is_tiled(&engine, &domain, "wone"));
        assert!(
            is_automatic(&engine, &domain, "wtwo"),
            "later admission uses either-axis"
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn toggle_float(
        engine: &mut tiler_core::engine::Engine,
        owner: &OwnerId,
        generation: &GenerationId,
        domain: &(OutputDomain, DomainKey),
        rows: &[(WindowId, Rect, WindowSizeHints, bool, FixedAdmissionFlags)],
        focused: &str,
        window: &str,
        correlation: &str,
        float_rect: Option<Rect>,
    ) -> CoreReply {
        let carried: Vec<(WindowId, Rect, WindowSizeHints, bool)> = rows
            .iter()
            .map(|(id, rect, hints, floating, _)| (id.clone(), *rect, *hints, *floating))
            .collect();
        let fp = crate::tiling::fingerprint(
            &rows
                .iter()
                .map(|(id, rect, _, _, _)| (id.0.clone(), *rect))
                .collect::<Vec<_>>(),
        );
        let correlation = CorrelationId::parse(correlation).expect("correlation");
        let mut event = crate::tiling::build_reconcile_event_for_floating(
            owner,
            generation,
            &correlation,
            revision_of(engine, domain),
            fp,
            &domain.0,
            &domain.1,
            8,
            &carried,
            Some(&WindowId(focused.to_owned())),
        );
        let flags: HashMap<WindowId, FixedAdmissionFlags> = rows
            .iter()
            .map(|(id, _, _, _, flags)| (id.clone(), *flags))
            .collect();
        stamp_fixed_admission_flags(&mut event.windows, &flags);
        event.command = CoreCommand::ToggleFloat {
            window: window.to_owned(),
            float_rect,
        };
        engine.handle(&event)
    }

    #[test]
    fn explicit_unfloat_wins_and_survives_release_with_suppress() {
        // D3/D6/D7: unfloating an automatic fixed float records the
        // override; after a domain release (workspace enable), the stamped
        // suppress signal re-tiles it instead of re-floating. Without the
        // signal the same hints re-float (control leg).
        let (mut engine, owner, generation, domain, bounds) = rig();
        reconcile(
            &mut engine,
            &owner,
            &generation,
            &domain,
            &[fixed_row("wfix", bounds), plain_row("w1", bounds)],
            "w1",
            "reconcile-1",
        );
        assert!(is_automatic(&engine, &domain, "wfix"));
        let rows = vec![
            (
                WindowId("wfix".to_owned()),
                bounds,
                fixed_hints(),
                true,
                FixedAdmissionFlags::default(),
            ),
            plain_row("w1", bounds),
        ];
        let reply = toggle_float(
            &mut engine,
            &owner,
            &generation,
            &domain,
            &rows,
            "wfix",
            "wfix",
            "unfloat-1",
            None,
        );
        assert!(
            !matches!(reply, CoreReply::Rejected { .. } | CoreReply::Diverged(_)),
            "unfloat commits, got {reply:?}"
        );
        assert!(is_tiled(&engine, &domain, "wfix"), "explicit tile wins");
        assert!(
            !is_automatic(&engine, &domain, "wfix"),
            "automatic mark withdraws"
        );
        assert!(has_override(&engine, &domain, "wfix"), "override recorded");
        // Workspace enable: release drops the session including the marks.
        let correlation = CorrelationId::parse("release-1").expect("correlation");
        let mut release = crate::tiling::build_reconcile_event_for_floating(
            &owner,
            &generation,
            &correlation,
            revision_of(&engine, &domain),
            11,
            &domain.0,
            &domain.1,
            8,
            &[
                (WindowId("wfix".to_owned()), bounds, fixed_hints(), false),
                (
                    WindowId("w1".to_owned()),
                    bounds,
                    WindowSizeHints::none(),
                    false,
                ),
            ],
            Some(&WindowId("w1".to_owned())),
        );
        release.command = CoreCommand::ReleaseDomain;
        assert!(matches!(engine.handle(&release), CoreReply::Released));
        // Fresh re-admission with the adapter suppress signal (override set /
        // marker): the fixed window tiles instead of re-floating.
        let suppress = FixedAdmissionFlags {
            fullscreen: false,
            fixed_auto: false,
            fixed_suppress: true,
        };
        let rows = vec![
            (
                WindowId("wfix".to_owned()),
                bounds,
                fixed_hints(),
                false,
                suppress,
            ),
            plain_row("w1", bounds),
        ];
        let reply = reconcile(
            &mut engine,
            &owner,
            &generation,
            &domain,
            &rows,
            "w1",
            "reconcile-2",
        );
        assert!(
            !matches!(reply, CoreReply::Rejected { .. } | CoreReply::Diverged(_)),
            "enable converges, got {reply:?}"
        );
        assert!(
            is_tiled(&engine, &domain, "wfix"),
            "override stays tiled at enable"
        );
        assert!(!is_automatic(&engine, &domain, "wfix"));
        // Control: a fresh fixed window without the signal floats.
        let rows = vec![
            fixed_row("wnew", bounds),
            (
                WindowId("wfix".to_owned()),
                bounds,
                fixed_hints(),
                false,
                suppress,
            ),
            plain_row("w1", bounds),
        ];
        reconcile(
            &mut engine,
            &owner,
            &generation,
            &domain,
            &rows,
            "w1",
            "reconcile-3",
        );
        assert!(
            is_automatic(&engine, &domain, "wnew"),
            "unsuppressed fixed floats"
        );
        assert!(
            is_tiled(&engine, &domain, "wfix"),
            "suppressed override keeps tiling"
        );
    }

    #[test]
    fn born_fullscreen_first_exit_uses_current_hints_then_retains_identity() {
        // D5: born-fullscreen arrivals hold slotless; the first exit
        // classifies with current hints and predicate (fixed floats
        // untouched, otherwise tiles); later toggles retain identity (D2).
        let (mut engine, owner, generation, domain, bounds) = rig();
        let held = FixedAdmissionFlags {
            fullscreen: true,
            fixed_auto: false,
            fixed_suppress: false,
        };
        let rows = vec![
            (
                WindowId("efix".to_owned()),
                bounds,
                fixed_hints(),
                true,
                held,
            ),
            (
                WindowId("nplain".to_owned()),
                bounds,
                WindowSizeHints::none(),
                true,
                held,
            ),
            plain_row("w1", bounds),
        ];
        let reply = reconcile(
            &mut engine,
            &owner,
            &generation,
            &domain,
            &rows,
            "w1",
            "reconcile-1",
        );
        assert!(
            !matches!(reply, CoreReply::Rejected { .. } | CoreReply::Diverged(_)),
            "birth hold converges, got {reply:?}"
        );
        assert!(is_float(&engine, &domain, "efix"), "born hold floats");
        assert!(
            !is_automatic(&engine, &domain, "efix"),
            "hold carries no automatic mark"
        );
        // First exit: fixed floats automatic, non-fixed tiles.
        let rows = vec![
            fixed_row("efix", bounds),
            plain_row("nplain", bounds),
            plain_row("w1", bounds),
        ];
        let reply = reconcile(
            &mut engine,
            &owner,
            &generation,
            &domain,
            &rows,
            "w1",
            "exit-1",
        );
        assert!(
            !matches!(reply, CoreReply::Rejected { .. } | CoreReply::Diverged(_)),
            "first exit converges, got {reply:?}"
        );
        assert!(
            is_automatic(&engine, &domain, "efix"),
            "fixed first exit floats automatic"
        );
        assert!(
            is_tiled(&engine, &domain, "nplain"),
            "non-fixed first exit tiles"
        );
        // Repeated toggles retain identity: re-enter fullscreen, exit again,
        // and the layers persist instead of reclassifying. A later
        // fullscreen transition on the slotted member rides tiled with the
        // overlay flag (managed overlay transition, never a float flip).
        let held_full = FixedAdmissionFlags {
            fullscreen: true,
            fixed_auto: false,
            fixed_suppress: false,
        };
        let rows = vec![
            (
                WindowId("efix".to_owned()),
                bounds,
                fixed_hints(),
                true,
                held,
            ),
            (
                WindowId("nplain".to_owned()),
                bounds,
                WindowSizeHints::none(),
                false,
                held_full,
            ),
            plain_row("w1", bounds),
        ];
        reconcile(
            &mut engine,
            &owner,
            &generation,
            &domain,
            &rows,
            "w1",
            "refull-1",
        );
        assert!(is_float(&engine, &domain, "efix"));
        assert!(
            is_tiled(&engine, &domain, "nplain"),
            "tiled member keeps its slot under fullscreen"
        );
        let rows = vec![
            fixed_row("efix", bounds),
            plain_row("nplain", bounds),
            plain_row("w1", bounds),
        ];
        reconcile(
            &mut engine,
            &owner,
            &generation,
            &domain,
            &rows,
            "w1",
            "exit-2",
        );
        assert!(
            is_float(&engine, &domain, "efix"),
            "automatic stays floating"
        );
        assert!(is_tiled(&engine, &domain, "nplain"), "tiled stays tiled");
    }

    #[test]
    fn workspace_enable_rechecks_every_candidate_and_keeps_intent() {
        // D6: enabling workspace tiling checks every window being tiled,
        // including arrivals while floating: fixed stays/becomes untouched
        // float, others tile. Explicit overrides stay tiled; intentional and
        // sticky floats keep their existing rules.
        let (mut engine, owner, generation, domain, bounds) = rig();
        reconcile(
            &mut engine,
            &owner,
            &generation,
            &domain,
            &[fixed_row("wfix", bounds), plain_row("w1", bounds)],
            "w1",
            "reconcile-1",
        );
        assert!(is_automatic(&engine, &domain, "wfix"));
        let correlation = CorrelationId::parse("release-1").expect("correlation");
        let mut release = crate::tiling::build_reconcile_event_for_floating(
            &owner,
            &generation,
            &correlation,
            revision_of(&engine, &domain),
            11,
            &domain.0,
            &domain.1,
            8,
            &[
                (WindowId("wfix".to_owned()), bounds, fixed_hints(), false),
                (
                    WindowId("w1".to_owned()),
                    bounds,
                    WindowSizeHints::none(),
                    false,
                ),
            ],
            Some(&WindowId("w1".to_owned())),
        );
        release.command = CoreCommand::ReleaseDomain;
        assert!(matches!(engine.handle(&release), CoreReply::Released));
        // Enable re-admission: the fixed arrival floats, the hint-changed
        // ordinary arrival tiles, the explicit override tiles, and the
        // intentional float stays intentional (never automatic).
        let suppress = FixedAdmissionFlags {
            fullscreen: false,
            fixed_auto: false,
            fixed_suppress: true,
        };
        let rows = vec![
            fixed_row("wfix", bounds),
            plain_row("wchanged", bounds),
            (
                WindowId("wover".to_owned()),
                bounds,
                fixed_hints(),
                false,
                suppress,
            ),
            (
                WindowId("wintent".to_owned()),
                bounds,
                WindowSizeHints::none(),
                true,
                FixedAdmissionFlags::default(),
            ),
            plain_row("w1", bounds),
        ];
        let reply = reconcile(
            &mut engine,
            &owner,
            &generation,
            &domain,
            &rows,
            "w1",
            "enable-1",
        );
        assert!(
            !matches!(reply, CoreReply::Rejected { .. } | CoreReply::Diverged(_)),
            "enable converges, got {reply:?}"
        );
        assert!(
            is_automatic(&engine, &domain, "wfix"),
            "fixed stays automatic float"
        );
        assert!(
            is_tiled(&engine, &domain, "wchanged"),
            "hint-changed ordinary tiles"
        );
        assert!(
            is_tiled(&engine, &domain, "wover"),
            "explicit override stays tiled"
        );
        assert!(!is_automatic(&engine, &domain, "wover"));
        assert!(
            is_float(&engine, &domain, "wintent"),
            "intentional stays floating"
        );
        assert!(
            !is_automatic(&engine, &domain, "wintent"),
            "intentional never marked automatic"
        );
    }

    #[test]
    fn id_reuse_after_removal_classifies_again() {
        // D3: removal drops fixed-size bookkeeping, so a reused id with
        // different hints classifies fresh.
        let (mut engine, owner, generation, domain, bounds) = rig();
        reconcile(
            &mut engine,
            &owner,
            &generation,
            &domain,
            &[fixed_row("wfix", bounds), plain_row("w1", bounds)],
            "w1",
            "reconcile-1",
        );
        assert!(is_automatic(&engine, &domain, "wfix"));
        reconcile(
            &mut engine,
            &owner,
            &generation,
            &domain,
            &[plain_row("w1", bounds)],
            "w1",
            "reconcile-2",
        );
        assert!(
            engine
                .session(&domain.1)
                .is_some_and(|s| !s.is_exception(&WindowId("wfix".to_owned()))),
            "removal drops membership"
        );
        reconcile(
            &mut engine,
            &owner,
            &generation,
            &domain,
            &[plain_row("wfix", bounds), plain_row("w1", bounds)],
            "w1",
            "reconcile-3",
        );
        assert!(
            is_tiled(&engine, &domain, "wfix"),
            "reused id with plain hints tiles"
        );
        assert!(!is_automatic(&engine, &domain, "wfix"));
        assert!(!has_override(&engine, &domain, "wfix"));
    }

    #[test]
    fn send_carries_suppress_so_override_mover_stays_tiled() {
        // An explicit tile override survives a workspace send when the
        // adapter stamps its suppress origin on the mover.
        let (mut engine, owner, generation, domain, bounds) = rig();
        reconcile(
            &mut engine,
            &owner,
            &generation,
            &domain,
            &[fixed_row("wfix", bounds), plain_row("w1", bounds)],
            "w1",
            "reconcile-1",
        );
        let rows = vec![
            (
                WindowId("wfix".to_owned()),
                bounds,
                fixed_hints(),
                true,
                FixedAdmissionFlags::default(),
            ),
            plain_row("w1", bounds),
        ];
        toggle_float(
            &mut engine,
            &owner,
            &generation,
            &domain,
            &rows,
            "wfix",
            "wfix",
            "unfloat-1",
            None,
        );
        assert!(has_override(&engine, &domain, "wfix"));
        let target_domain = {
            let key = DomainKey {
                output: OutputId("mon-a".to_owned()),
                workspace: WorkspaceId("ws-2".to_owned()),
            };
            let domain = OutputDomain {
                id: OutputId("mon-a".to_owned()),
                workspace: WorkspaceId("ws-2".to_owned()),
                bounds,
                gap: 8,
                adjacent: BTreeMap::new(),
            };
            (domain, key)
        };
        let source_rows = vec![
            OwnerRow {
                token: "wfix".to_owned(),
                rect: bounds,
                hints: fixed_hints(),
                floating: false,
            },
            OwnerRow {
                token: "w1".to_owned(),
                rect: bounds,
                hints: WindowSizeHints::none(),
                floating: false,
            },
        ];
        let target_rows = vec![OwnerRow {
            token: "w2".to_owned(),
            rect: bounds,
            hints: WindowSizeHints::none(),
            floating: false,
        }];
        // The target workspace session exists (per-workspace sessions);
        // seed it so the two-domain plan has a retained target.
        reconcile(
            &mut engine,
            &owner,
            &generation,
            &target_domain,
            &[plain_row("w2", bounds)],
            "w2",
            "seed-target",
        );
        let correlation = CorrelationId::parse("send-1").expect("correlation");
        let mut event = build_send_event(
            &owner,
            &generation,
            &correlation,
            revision_of(&engine, &domain),
            23,
            (domain.0.clone(), domain.1.clone()),
            (target_domain.0.clone(), target_domain.1.clone()),
            &source_rows,
            &target_rows,
            "wfix",
            8,
            true,
        )
        .expect("send event builds");
        stamp_send_target(&mut event, &target_domain.1);
        let flags: HashMap<WindowId, FixedAdmissionFlags> = [(
            WindowId("wfix".to_owned()),
            FixedAdmissionFlags {
                fullscreen: false,
                fixed_auto: false,
                fixed_suppress: true,
            },
        )]
        .into_iter()
        .collect();
        stamp_fixed_admission_flags(&mut event.windows, &flags);
        stamp_fixed_admission_flags(&mut event.target_windows, &flags);
        let reply = engine.handle(&event);
        assert!(
            !matches!(
                reply,
                CoreReply::Rejected { .. }
                    | CoreReply::Diverged(_)
                    | CoreReply::SnapshotInvalid { .. }
            ),
            "send converges, got {reply:?}"
        );
        assert!(
            engine.session(&target_domain.1).is_some_and(|s| s
                .snapshot()
                .windows
                .iter()
                .any(|w| w.window.0 == "wfix")),
            "mover arrives in the target"
        );
    }

    fn converted(min_outer: Option<(i32, i32)>, max_outer: Option<(i32, i32)>) -> WindowSizeHints {
        // Production conversion path with a nonzero frame (8px every side):
        // the same inset subtraction the native query feeds the Engine.
        crate::tiling::hints_from_outer(
            min_outer,
            max_outer,
            crate::tiling::FrameInsets {
                left: 8,
                top: 8,
                right: 8,
                bottom: 8,
            },
        )
    }

    #[test]
    fn production_conversion_partial_zero_width_floats_only_under_either_axis() {
        // G-05 through the real production conversion (raw outer tracks plus
        // nonzero frame insets) into a real Engine: min=max=(640,0) outer
        // converts to visible (624,0)/(624,0), which tiles under the default
        // and floats through its nonzero axis under either-axis-fixed.
        for predicate in [FixedSizePredicate::BothAxes, FixedSizePredicate::EitherAxis] {
            let (mut engine, owner, generation, domain, bounds) = rig();
            engine.set_fixed_size_predicate(predicate);
            let hints = converted(Some((640, 0)), Some((640, 0)));
            assert_eq!(
                (hints.min_w, hints.min_h, hints.max_w, hints.max_h),
                (Some(624), Some(0), Some(624), Some(0)),
                "conversion preserves the unset axis"
            );
            let rows = vec![
                (
                    WindowId("p".to_owned()),
                    bounds,
                    hints,
                    false,
                    FixedAdmissionFlags::default(),
                ),
                plain_row("w1", bounds),
            ];
            let reply = reconcile(
                &mut engine,
                &owner,
                &generation,
                &domain,
                &rows,
                "w1",
                "reconcile-1",
            );
            assert!(
                !matches!(
                    reply,
                    CoreReply::Rejected { .. }
                        | CoreReply::Diverged(_)
                        | CoreReply::SnapshotInvalid { .. }
                ),
                "converges under {predicate:?}, got {reply:?}"
            );
            match predicate {
                FixedSizePredicate::BothAxes => assert!(
                    is_tiled(&engine, &domain, "p"),
                    "partial-zero width tiles under the default"
                ),
                FixedSizePredicate::EitherAxis => assert!(
                    is_automatic(&engine, &domain, "p"),
                    "partial-zero width floats under either-axis"
                ),
            }
        }
    }

    #[test]
    fn production_conversion_partial_zero_height_floats_only_under_either_axis() {
        // Mirror orientation: min=max=(0,480) outer converts to visible
        // (0,464)/(0,464) and behaves symmetrically.
        for predicate in [FixedSizePredicate::BothAxes, FixedSizePredicate::EitherAxis] {
            let (mut engine, owner, generation, domain, bounds) = rig();
            engine.set_fixed_size_predicate(predicate);
            let hints = converted(Some((0, 480)), Some((0, 480)));
            assert_eq!(
                (hints.min_w, hints.min_h, hints.max_w, hints.max_h),
                (Some(0), Some(464), Some(0), Some(464)),
                "conversion preserves the unset axis"
            );
            let rows = vec![
                (
                    WindowId("p".to_owned()),
                    bounds,
                    hints,
                    false,
                    FixedAdmissionFlags::default(),
                ),
                plain_row("w1", bounds),
            ];
            reconcile(
                &mut engine,
                &owner,
                &generation,
                &domain,
                &rows,
                "w1",
                "reconcile-1",
            );
            match predicate {
                FixedSizePredicate::BothAxes => assert!(
                    is_tiled(&engine, &domain, "p"),
                    "partial-zero height tiles under the default"
                ),
                FixedSizePredicate::EitherAxis => assert!(
                    is_automatic(&engine, &domain, "p"),
                    "partial-zero height floats under either-axis"
                ),
            }
        }
    }

    #[test]
    fn production_conversion_fixed_with_insets_floats_and_reports() {
        // A 640x480 fixed client behind an 8px frame converts to visible
        // 624x464 on both sides: still equal and nonzero, so it floats
        // automatic with the bounded fixed-equal report and no geometry.
        let (mut engine, owner, generation, domain, bounds) = rig();
        let hints = converted(Some((640, 480)), Some((640, 480)));
        assert_eq!(
            (hints.min_w, hints.min_h, hints.max_w, hints.max_h),
            (Some(624), Some(464), Some(624), Some(464))
        );
        let rows = vec![
            (
                WindowId("wfix".to_owned()),
                bounds,
                hints,
                false,
                FixedAdmissionFlags::default(),
            ),
            plain_row("w1", bounds),
        ];
        let reply = reconcile(
            &mut engine,
            &owner,
            &generation,
            &domain,
            &rows,
            "w1",
            "reconcile-1",
        );
        assert!(is_automatic(&engine, &domain, "wfix"));
        if let CoreReply::Tiled(plan) = &reply {
            assert!(
                plan.geometry.iter().all(|g| g.window.0 != "wfix"),
                "no native target writes for the automatic float"
            );
        } else {
            panic!("admission plans, got {reply:?}");
        }
        let report = engine.last_fixed_admission().expect("bounded report");
        assert_eq!((report.reason, report.admitted), ("fixed-equal", 1));
    }

    #[test]
    fn first_exit_with_converted_current_hints_and_predicate() {
        // D5 through production conversion: the exit classifies with the
        // hints and predicate current at exit, not at birth. Birth holds
        // slotless; a predicate flip plus hint change mid-hold selects the
        // exit layer; a fixed exit floats untouched.
        let (mut engine, owner, generation, domain, bounds) = rig();
        let held = FixedAdmissionFlags {
            fullscreen: true,
            fixed_auto: false,
            fixed_suppress: false,
        };
        // Born one-axis-fixed under the default: held, not classified.
        let birth_hints = converted(Some((640, 480)), Some((640, 960)));
        let rows = vec![
            (WindowId("e".to_owned()), bounds, birth_hints, true, held),
            plain_row("w1", bounds),
        ];
        reconcile(
            &mut engine,
            &owner,
            &generation,
            &domain,
            &rows,
            "w1",
            "reconcile-1",
        );
        assert!(is_float(&engine, &domain, "e"));
        assert!(!is_automatic(&engine, &domain, "e"));
        // Mid-hold the predicate flips to either-axis: the first exit now
        // sees a fixed window and floats it automatic.
        engine.set_fixed_size_predicate(FixedSizePredicate::EitherAxis);
        let exit_hints = converted(Some((640, 480)), Some((640, 960)));
        let rows = vec![
            (
                WindowId("e".to_owned()),
                bounds,
                exit_hints,
                false,
                FixedAdmissionFlags::default(),
            ),
            plain_row("w1", bounds),
        ];
        reconcile(
            &mut engine,
            &owner,
            &generation,
            &domain,
            &rows,
            "w1",
            "exit-1",
        );
        assert!(
            is_automatic(&engine, &domain, "e"),
            "exit uses the current predicate, not the birth one"
        );
        // Reverse: hints lose fixedness before exit (ordinary at exit).
        let (mut engine, owner, generation, domain, bounds) = rig();
        let rows = vec![
            (
                WindowId("e".to_owned()),
                bounds,
                converted(Some((640, 480)), Some((640, 480))),
                true,
                held,
            ),
            plain_row("w1", bounds),
        ];
        reconcile(
            &mut engine,
            &owner,
            &generation,
            &domain,
            &rows,
            "w1",
            "reconcile-1",
        );
        let rows = vec![
            (
                WindowId("e".to_owned()),
                bounds,
                converted(Some((640, 480)), Some((800, 600))),
                false,
                FixedAdmissionFlags::default(),
            ),
            plain_row("w1", bounds),
        ];
        reconcile(
            &mut engine,
            &owner,
            &generation,
            &domain,
            &rows,
            "w1",
            "exit-1",
        );
        assert!(
            is_tiled(&engine, &domain, "e"),
            "hint loss before exit tiles (admission-only, current hints)"
        );
    }

    #[test]
    fn suppress_on_non_fixed_window_is_behavior_neutral() {
        // Verified inert-suppress evidence (D3/D7 scope): stamping an
        // explicit tile win on a window whose hints never classify fixed
        // changes no outcome across fresh admission, release, and enable.
        // Core records an override on every unfloat; the marker mirrors it,
        // and neutrality here is why mirroring is safe.
        let run = |suppress: bool| {
            let (mut engine, owner, generation, domain, bounds) = rig();
            let flags = FixedAdmissionFlags {
                fullscreen: false,
                fixed_auto: false,
                fixed_suppress: suppress,
            };
            let rows = vec![
                (
                    WindowId("w".to_owned()),
                    bounds,
                    WindowSizeHints::none(),
                    false,
                    flags,
                ),
                plain_row("w1", bounds),
            ];
            let first = reconcile(
                &mut engine,
                &owner,
                &generation,
                &domain,
                &rows,
                "w1",
                "reconcile-1",
            );
            let tiled_first = is_tiled(&engine, &domain, "w");
            let correlation = CorrelationId::parse("release-1").expect("correlation");
            let mut release = crate::tiling::build_reconcile_event_for_floating(
                &owner,
                &generation,
                &correlation,
                revision_of(&engine, &domain),
                11,
                &domain.0,
                &domain.1,
                8,
                &[
                    (
                        WindowId("w".to_owned()),
                        bounds,
                        WindowSizeHints::none(),
                        false,
                    ),
                    (
                        WindowId("w1".to_owned()),
                        bounds,
                        WindowSizeHints::none(),
                        false,
                    ),
                ],
                Some(&WindowId("w1".to_owned())),
            );
            release.command = CoreCommand::ReleaseDomain;
            assert!(matches!(engine.handle(&release), CoreReply::Released));
            let second = reconcile(
                &mut engine,
                &owner,
                &generation,
                &domain,
                &rows,
                "w1",
                "reconcile-2",
            );
            let tiled_second = is_tiled(&engine, &domain, "w");
            let auto_second = is_automatic(&engine, &domain, "w");
            (
                std::mem::discriminant(&first),
                std::mem::discriminant(&second),
                tiled_first,
                tiled_second,
                auto_second,
            )
        };
        let plain = run(false);
        let suppressed = run(true);
        // Identical reply shapes, identical tiled outcomes, and no
        // automatic mark in either leg: the signal is inert here.
        assert_eq!(
            plain, suppressed,
            "suppress changes nothing for non-fixed hints"
        );
        assert!(plain.2 && plain.3 && !plain.4);
    }

    #[test]
    fn override_survives_hint_loss_at_enable() {
        // D3/D7 durability: an explicit tile win is hint-independent. After
        // the hints are lost entirely, the stamped suppress signal still
        // tiles the window at enable instead of floating or dropping it.
        // (A fixed window with lost hints and no suppress tiles too, but
        // only the suppress preserves the override identity.)
        let (mut engine, owner, generation, domain, bounds) = rig();
        reconcile(
            &mut engine,
            &owner,
            &generation,
            &domain,
            &[fixed_row("wfix", bounds), plain_row("w1", bounds)],
            "w1",
            "reconcile-1",
        );
        let rows = vec![
            (
                WindowId("wfix".to_owned()),
                bounds,
                fixed_hints(),
                true,
                FixedAdmissionFlags::default(),
            ),
            plain_row("w1", bounds),
        ];
        toggle_float(
            &mut engine,
            &owner,
            &generation,
            &domain,
            &rows,
            "wfix",
            "wfix",
            "unfloat-1",
            None,
        );
        assert!(has_override(&engine, &domain, "wfix"));
        let correlation = CorrelationId::parse("release-1").expect("correlation");
        let mut release = crate::tiling::build_reconcile_event_for_floating(
            &owner,
            &generation,
            &correlation,
            revision_of(&engine, &domain),
            11,
            &domain.0,
            &domain.1,
            8,
            &[
                (WindowId("wfix".to_owned()), bounds, fixed_hints(), false),
                (
                    WindowId("w1".to_owned()),
                    bounds,
                    WindowSizeHints::none(),
                    false,
                ),
            ],
            Some(&WindowId("w1".to_owned())),
        );
        release.command = CoreCommand::ReleaseDomain;
        assert!(matches!(engine.handle(&release), CoreReply::Released));
        // Enable with the hints lost AND the suppress signal: still tiled.
        let suppress = FixedAdmissionFlags {
            fullscreen: false,
            fixed_auto: false,
            fixed_suppress: true,
        };
        let rows = vec![
            (
                WindowId("wfix".to_owned()),
                bounds,
                WindowSizeHints::none(),
                false,
                suppress,
            ),
            plain_row("w1", bounds),
        ];
        let reply = reconcile(
            &mut engine,
            &owner,
            &generation,
            &domain,
            &rows,
            "w1",
            "enable-1",
        );
        assert!(
            !matches!(
                reply,
                CoreReply::Rejected { .. }
                    | CoreReply::Diverged(_)
                    | CoreReply::SnapshotInvalid { .. }
            ),
            "enable converges, got {reply:?}"
        );
        assert!(
            is_tiled(&engine, &domain, "wfix"),
            "override persists through hint loss"
        );
    }

    #[test]
    fn fresh_hintless_admission_tiles_fail_closed() {
        // Unknown hints never classify: a fresh window with no usable hints
        // tiles, even under either-axis-fixed. Fail-closed evidence for the
        // enable path when a query fails on the classification tick.
        for predicate in [FixedSizePredicate::BothAxes, FixedSizePredicate::EitherAxis] {
            let (mut engine, owner, generation, domain, bounds) = rig();
            engine.set_fixed_size_predicate(predicate);
            let rows = vec![
                (
                    WindowId("wnew".to_owned()),
                    bounds,
                    WindowSizeHints::none(),
                    false,
                    FixedAdmissionFlags::default(),
                ),
                plain_row("w1", bounds),
            ];
            reconcile(
                &mut engine,
                &owner,
                &generation,
                &domain,
                &rows,
                "w1",
                "reconcile-1",
            );
            assert!(
                is_tiled(&engine, &domain, "wnew"),
                "hintless fresh admission tiles under {predicate:?}"
            );
            assert!(engine.last_fixed_admission().is_none());
        }
    }

    #[test]
    fn enable_admits_brand_new_fixed_arrival_automatic() {
        // D6 every-candidate recheck includes arrivals: a fixed window first
        // seen on the enable observation floats automatic like any other
        // fresh fixed admission.
        let (mut engine, owner, generation, domain, bounds) = rig();
        reconcile(
            &mut engine,
            &owner,
            &generation,
            &domain,
            &[plain_row("w1", bounds)],
            "w1",
            "reconcile-1",
        );
        let correlation = CorrelationId::parse("release-1").expect("correlation");
        let mut release = crate::tiling::build_reconcile_event_for_floating(
            &owner,
            &generation,
            &correlation,
            revision_of(&engine, &domain),
            11,
            &domain.0,
            &domain.1,
            8,
            &[(
                WindowId("w1".to_owned()),
                bounds,
                WindowSizeHints::none(),
                false,
            )],
            Some(&WindowId("w1".to_owned())),
        );
        release.command = CoreCommand::ReleaseDomain;
        assert!(matches!(engine.handle(&release), CoreReply::Released));
        let rows = vec![fixed_row("warr", bounds), plain_row("w1", bounds)];
        reconcile(
            &mut engine,
            &owner,
            &generation,
            &domain,
            &rows,
            "w1",
            "enable-1",
        );
        assert!(
            is_automatic(&engine, &domain, "warr"),
            "fixed arrival at enable floats automatic"
        );
    }

    mod directional_foundation {
        use super::*;
        use tiler_core::directional::Direction;

        fn dkey(hwnd: u64) -> WindowKey {
            WindowKey {
                hwnd,
                pid: 1000 + hwnd as u32,
                creation: format!("c{hwnd:016x}"),
            }
        }

        fn full(id: &str, x: i32, y: i32, w: i32, h: i32) -> DirectionalOutput {
            DirectionalOutput {
                id: id.to_owned(),
                full: Rect { x, y, w, h },
            }
        }

        fn mover(x: i32, y: i32, w: i32, h: i32) -> Rect {
            Rect { x, y, w, h }
        }

        #[test]
        fn primary_left_right_round_trip_selects_reciprocal_pair() {
            let outputs = vec![
                full("primary", 0, 0, 1920, 1080),
                full("right", 1920, 0, 1920, 1080),
            ];
            let window = mover(1600, 400, 200, 200);
            assert_eq!(
                select_directional_target(Direction::Right, "primary", &window, &outputs)
                    .expect("readable"),
                Some("right".to_owned())
            );
            let back = mover(2000, 400, 200, 200);
            assert_eq!(
                select_directional_target(Direction::Left, "right", &back, &outputs)
                    .expect("readable"),
                Some("primary".to_owned())
            );
        }

        #[test]
        fn other_compass_and_negative_coords_select_without_wrap() {
            let outputs = vec![
                full("south", 0, 0, 1920, 1080),
                full("north", 0, -1080, 1920, 1080),
                full("west", -1920, -1080, 1920, 1080),
            ];
            let window = mover(800, -200, 200, 200);
            assert_eq!(
                select_directional_target(Direction::Up, "south", &window, &outputs)
                    .expect("readable"),
                Some("north".to_owned())
            );
            assert_eq!(
                select_directional_target(Direction::Left, "north", &window, &outputs)
                    .expect("readable"),
                Some("west".to_owned())
            );
            // No candidate is a local no-op, never a wrap.
            let lonely = vec![full("solo", 0, 0, 1920, 1080)];
            assert_eq!(
                select_directional_target(Direction::Right, "solo", &window, &lonely)
                    .expect("readable"),
                None
            );
        }

        #[test]
        fn unreadable_topology_and_unknown_source_refuse() {
            let outputs = vec![full("a", 0, 0, 1920, 1080), full("bad", 1920, 0, 0, 1080)];
            let window = mover(1600, 400, 200, 200);
            assert!(select_directional_target(Direction::Right, "a", &window, &outputs).is_err());
            let outputs = vec![full("a", 0, 0, 1920, 1080)];
            assert!(
                select_directional_target(Direction::Right, "ghost", &window, &outputs).is_err()
            );
        }

        #[test]
        fn centre_projection_then_span_then_left_top_ranking() {
            // Two stacked candidates on the right: the mover centre picks the
            // containing edge; otherwise the larger span wins; then left/top.
            let outputs = vec![
                full("src", 0, 0, 800, 1000),
                full("upper", 800, 0, 800, 500),
                full("lower", 800, 500, 800, 500),
            ];
            let centred_upper = mover(100, 100, 200, 100);
            assert_eq!(
                select_directional_target(Direction::Right, "src", &centred_upper, &outputs)
                    .expect("readable"),
                Some("upper".to_owned())
            );
            let centred_lower = mover(100, 700, 200, 100);
            assert_eq!(
                select_directional_target(Direction::Right, "src", &centred_lower, &outputs)
                    .expect("readable"),
                Some("lower".to_owned())
            );
        }

        #[test]
        fn pair_attaches_reciprocal_adjacency_only() {
            let bounds = Rect {
                x: 0,
                y: 0,
                w: 1920,
                h: 1040,
            };
            let source = workspace_domain("mon-a", "ws-1", bounds, 8);
            let target = workspace_domain("mon-b", "ws-2", bounds, 8);
            let ((source_domain, _), (target_domain, _)) =
                directional_pair_with_adjacency(source, target, Direction::Right);
            assert_eq!(
                source_domain
                    .adjacent
                    .get(&Direction::Right)
                    .map(|id| id.0.as_str()),
                Some("mon-b")
            );
            assert_eq!(
                target_domain
                    .adjacent
                    .get(&Direction::Left)
                    .map(|id| id.0.as_str()),
                Some("mon-a")
            );
            assert_eq!(source_domain.adjacent.len(), 1);
            assert_eq!(target_domain.adjacent.len(), 1);
        }

        #[test]
        fn per_monitor_identities_stay_distinct_for_same_workspace_index() {
            // Same workspace id string on two outputs still yields distinct
            // domain keys: the output scope is part of the identity.
            let bounds = Rect {
                x: 0,
                y: 0,
                w: 800,
                h: 600,
            };
            let left = workspace_domain("mon-a", "ws-same", bounds, 8);
            let right = workspace_domain("mon-b", "ws-same", bounds, 8);
            assert_ne!(left.1, right.1);
            assert_ne!(left.0.key(), right.0.key());
        }

        fn fence_snapshot() -> DirectionalFenceSnapshot {
            DirectionalFenceSnapshot {
                inner_gap: 8,
                outer_gap: 8,
                source_output: "mon-a".to_owned(),
                source_workspace: "ws-1".to_owned(),
                target_output: "mon-b".to_owned(),
                target_workspace: "ws-2".to_owned(),
                source_tiled: true,
                target_tiled: true,
                direction: Direction::Right,
                overlay_maximized: false,
                overlay_fullscreen: false,
                source_full: Rect {
                    x: 0,
                    y: 0,
                    w: 1920,
                    h: 1080,
                },
                target_full: Rect {
                    x: 1920,
                    y: 0,
                    w: 1920,
                    h: 1080,
                },
                source_bounds: Rect {
                    x: 8,
                    y: 8,
                    w: 1904,
                    h: 1064,
                },
                target_bounds: Rect {
                    x: 1928,
                    y: 8,
                    w: 1904,
                    h: 1064,
                },
                owner: "owner".to_owned(),
                generation: "gen".to_owned(),
            }
        }

        fn fence_live() -> DirectionalFenceLive {
            DirectionalFenceLive {
                inner_gap: 8,
                outer_gap: 8,
                source_tiled: true,
                target_tiled: true,
                source_is_active: true,
                target_is_active: true,
                topology_ok: true,
                scope_ok: true,
                lifetime_ok: true,
                membership_ok: true,
                overlay_maximized: false,
                overlay_fullscreen: false,
                owner_ok: true,
            }
        }

        #[test]
        fn fences_hold_when_fresh_and_refuse_each_drift() {
            let snapshot = fence_snapshot();
            assert_eq!(directional_fences_hold(&snapshot, &fence_live()), None);
            let mut stale_view = fence_live();
            stale_view.target_is_active = false;
            assert_eq!(
                directional_fences_hold(&snapshot, &stale_view),
                Some("view-changed")
            );
            let mut stale_topology = fence_live();
            stale_topology.topology_ok = false;
            assert_eq!(
                directional_fences_hold(&snapshot, &stale_topology),
                Some("topology-changed")
            );
            let mut stale_lifetime = fence_live();
            stale_lifetime.lifetime_ok = false;
            assert_eq!(
                directional_fences_hold(&snapshot, &stale_lifetime),
                Some("identity-changed")
            );
            let mut stale_scope = fence_live();
            stale_scope.scope_ok = false;
            assert_eq!(
                directional_fences_hold(&snapshot, &stale_scope),
                Some("scope-changed")
            );
            let mut floating = fence_live();
            floating.target_tiled = false;
            assert_eq!(
                directional_fences_hold(&snapshot, &floating),
                Some("workspace-floating")
            );
            let mut fullscreen = fence_live();
            fullscreen.overlay_fullscreen = true;
            assert_eq!(
                directional_fences_hold(&snapshot, &fullscreen),
                Some("overlay-changed")
            );
        }

        #[test]
        fn transfer_requires_absence_source_and_presence_target() {
            let mover = dkey(7);
            let other = dkey(8);
            let source: BTreeSet<WindowKey> = [other.clone()].into_iter().collect();
            let target: BTreeSet<WindowKey> = [mover.clone(), other.clone()].into_iter().collect();
            assert!(verify_directional_transfer(&mover, &source, &target));
            let bad_source: BTreeSet<WindowKey> =
                [mover.clone(), other.clone()].into_iter().collect();
            assert!(!verify_directional_transfer(&mover, &bad_source, &target));
            assert!(!verify_directional_transfer(&mover, &source, &source));
        }

        #[test]
        fn focus_target_prefers_last_focus_then_visible_member() {
            let mut spaces = ManagedWorkspaces::new();
            spaces.ensure_output("mon-a");
            spaces.ensure_output("mon-b");
            let active_b = spaces.active_id("mon-b").expect("active");
            let (first, second) = (dkey(21), dkey(22));
            assert!(spaces.assign(first.clone(), "mon-b", &active_b, false));
            assert!(spaces.assign(second.clone(), "mon-b", &active_b, false));
            spaces.note_foreground(&second);
            let visible: BTreeSet<WindowKey> =
                [first.clone(), second.clone()].into_iter().collect();
            assert_eq!(
                directional_focus_target(&spaces, "mon-b", &active_b, &visible),
                Some(second.clone())
            );
            // Hidden last-focus falls back to a visible member, never a hidden one.
            spaces.set_hidden(&second, true);
            let visible: BTreeSet<WindowKey> = [first.clone()].into_iter().collect();
            assert_eq!(
                directional_focus_target(&spaces, "mon-b", &active_b, &visible),
                Some(first.clone())
            );
            // Empty target refuses.
            let empty: BTreeSet<WindowKey> = BTreeSet::new();
            assert_eq!(
                directional_focus_target(&spaces, "mon-b", &active_b, &empty),
                None
            );
        }

        #[test]
        fn focus_crosses_horizontally_only() {
            assert!(directional_focus_crosses(Direction::Left));
            assert!(directional_focus_crosses(Direction::Right));
            assert!(!directional_focus_crosses(Direction::Up));
            assert!(!directional_focus_crosses(Direction::Down));
        }
    }
}

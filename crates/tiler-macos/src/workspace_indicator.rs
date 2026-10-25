#![forbid(unsafe_code)]

//! Menu-bar workspace indicator: pure projection, occupancy summary, click
//! mapping, and startup configuration.
//!
//! One native status item shows every existing native Space in native order
//! as directly clickable per-workspace segments in the menu-bar item itself
//! (plain numbers, current distinctly highlighted, occupancy treatments
//! distinct even with icons off); the dropdown menu mirrors the same rows
//! for accessibility. Selection reuses the guarded engine lane
//! ([`crate::workspace_select`]): a segment click submits the same armed
//! [`crate::runtime_types::InputSemanticAction`] the tap submits, so the
//! unarmed, single-display, gate, drift, and verified-arrival rules all
//! apply unchanged. No synthetic trailing workspace is ever added, no
//! trailing lifecycle is managed, and no preference file is written.
//!
//! Appearance policy (rendered natively in [`crate::ffi`], read back from
//! the live buttons by fixtures — never from a cached model):
//! - Order is native order, titles are 1-based ordinals with an explicit
//!   `'?'` suffix if and only if occupancy is Unknown (e.g. `"2?"`);
//!   numeric labels otherwise. Icons (when the flag resolves one) ride
//!   LEFT of the number, never replacing it.
//! - Current is orthogonal and unmissable: bordered bezel plus bold title.
//! - Occupancy is a second channel, distinct with icons off: Occupied reads
//!   full-alpha bold, Empty reads dimmed-alpha regular, Unknown reads
//!   full-alpha regular plus the `'?'` title suffix (the text colour is
//!   never changed — the native render sets no label colour, so the suffix
//!   is the honest Unknown marker, not a colour). Unknown is never dimmed
//!   like Empty and never bold like Occupied (unless also current, where
//!   the bezel plus bold are the current channel and `'?'` still names the
//!   Unknown occupancy).
//! - Positions past the armed `1..=9` select range stay visible but disabled
//!   (no armed route — clicking one must refuse, never select wrong).
//!
//! Occupancy is conservative: a Space reads `Occupied` only on a proven
//! positively-sized member window homed exactly to it; any unreadable
//! membership or unreadable/zero-area geometry reads `Unknown`, which must
//! never be presented as empty. Production occupancy comes from the
//! retained engine domains (managed windows only — the same homing the
//! engine plans from, current and hidden alike), never from an independent
//! desktop scan: [`summarize_retained`] maps one engine occupancy report
//! over the native row, and any unproven frame (no report, revision drift,
//! domain unknown) reads `Unknown`. The older live CG/SLS scan
//! ([`summarize_spaces`]/[`scan_space_state`]) survives ONLY as the
//! read-only `workspace-indicator-status` fixture's explicitly-labelled
//! live-scan evidence — never as retained-state evidence.
//!
//! Representative application identity is a bundle ID only (never a title):
//! the frontmost PID's member wins when it sits on the Space, else the
//! lowest `(pid, wid)` member. Deterministic over the same enumeration, so
//! the icon flag renders a stable image per Space. Nothing here logs bundle
//! IDs, titles, or paths; callers must keep them out of every log line.
//!
//! Click identity: every rendered segment binds an immutable
//! [`IndicatorClick`] (slot kind plus 1-based position, expected SID, UI
//! generation, and topology revision at render time).
//! [`resolve_indicator_click`] routes by the rendered kind alone:
//! numbered clicks resolve only through [`resolve_click`], trailing clicks
//! only through [`resolve_trailing_click`] — stale numbered clicks refuse
//! instead of falling back to trailing creation. Any payload resolved
//! against a different generation or revision refuses — a rebuild between
//! render and drain discards stale instead of selecting the wrong Space.
//! The engine claim and the native drain carry the same binding (see
//! [`crate::runtime_types::ClaimedSelect`]), so an ordinal is never
//! interpreted against a different generation there either.

use serde::{Deserialize, Serialize};

use crate::runtime_types::InputSemanticAction;

// -- Startup configuration ---------------------------------------------------

/// Which rows the indicator shows. Default [`IndicatorMode::All`]: every
/// existing native Space in native order, including empty ones.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IndicatorMode {
    /// Every existing native Space in native order (default).
    #[default]
    All,
    /// Only the current Space (space-constrained menu bars).
    Current,
    /// No status item at all.
    Off,
}

impl IndicatorMode {
    /// Parse one CLI/file mode token (case-insensitive).
    pub fn parse(s: &str) -> Result<Self, String> {
        match s.to_ascii_lowercase().as_str() {
            "all" => Ok(Self::All),
            "current" | "current-only" | "current_only" => Ok(Self::Current),
            "off" => Ok(Self::Off),
            other => Err(format!(
                "unknown --workspace-indicator '{other}' (expected all|current|off)"
            )),
        }
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Current => "current",
            Self::Off => "off",
        }
    }
}

/// Raw indicator selections (all unset = defaults; the settings file fills
/// the gaps in `main`, then startup config — the
/// [`crate::dev_selection::BorderCli`] shape).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IndicatorCli {
    /// Display mode (`Some` via `--workspace-indicator all|current|off`).
    pub mode: Option<IndicatorMode>,
    /// Representative app icons (`Some(true)` via
    /// `--workspace-indicator-icons`, `Some(false)` via
    /// `--no-workspace-indicator-icons`).
    pub icons: Option<bool>,
}

/// Resolved indicator options. Startup-only: a running session never
/// re-reads them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct IndicatorOptions {
    pub mode: IndicatorMode,
    pub icons: bool,
}

/// Merge CLI selections over the settings file: an explicitly passed CLI
/// flag always wins; otherwise the file value applies; otherwise the
/// built-in default (all rows, no icons).
pub fn resolve_indicator_config(
    cli: &IndicatorCli,
    file: Option<&crate::dev_selection::WorkspaceIndicatorSettings>,
) -> Result<IndicatorOptions, String> {
    Ok(IndicatorOptions {
        mode: cli.mode.or(file.and_then(|f| f.mode)).unwrap_or_default(),
        icons: cli.icons.or(file.and_then(|f| f.icons)).unwrap_or(false),
    })
}

// -- Projection ---------------------------------------------------------------

/// Conservative per-Space occupancy. `Unknown` must never be presented as
/// empty: it names unreadable membership or unreadable/zero-area geometry,
/// never absence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpaceOccupancy {
    Occupied,
    Empty,
    Unknown,
}

/// One projected menu row: plain native ordinal, highlight, occupancy, and
/// whether a click may dispatch (positions past the armed `1..=9` select
/// range are shown but never dispatched).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndicatorRow {
    /// 1-based position in native order (also the menu item tag and the
    /// armed select index while `selectable`).
    pub position: usize,
    pub sid: u64,
    pub current: bool,
    pub occupancy: SpaceOccupancy,
    pub selectable: bool,
    /// Deterministic representative bundle ID for the icons flag (`None`
    /// without a member to name). Never logged.
    pub representative: Option<String>,
}

/// The cached view-model the main thread applies change-only to the native
/// status item. `visible == false` means no status item (off mode or a
/// refused multi-display/zero-display topology) with the explicit reason in
/// `hidden_reason`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndicatorProjection {
    pub visible: bool,
    pub hidden_reason: Option<String>,
    /// Status item title: the current Space's plain native ordinal.
    pub title: String,
    pub rows: Vec<IndicatorRow>,
}

/// Project one indicator frame. `spaces` is the display's native-order user
/// Space row (existing Spaces only, trailing included, never invented);
/// `current` is its current Space. Order is preserved exactly, never sorted
/// and never current-first. Returns `Err` only when the row/current identity
/// is unprovable (empty, zero, duplicate, or current outside the row —
/// the [`crate::workspace_select::SelectionSnapshot`] rule); a non-single
/// display row or `Off` mode projects hidden with an explicit reason instead
/// of guessing.
pub fn project_indicator(
    spaces: &[u64],
    current: u64,
    display_count: usize,
    mode: IndicatorMode,
    occupancy: &dyn Fn(u64) -> SpaceOccupancy,
    representative: &dyn Fn(u64) -> Option<String>,
) -> Result<IndicatorProjection, String> {
    if spaces.is_empty() || spaces.contains(&0) || !spaces.contains(&current) || {
        let mut seen = std::collections::BTreeSet::new();
        spaces.iter().any(|s| !seen.insert(s))
    } {
        return Err("workspace row/current identity unprovable".to_owned());
    }
    if mode == IndicatorMode::Off {
        return Ok(IndicatorProjection {
            visible: false,
            hidden_reason: Some("disabled by configuration".to_owned()),
            title: String::new(),
            rows: Vec::new(),
        });
    }
    if display_count != 1 {
        return Ok(IndicatorProjection {
            visible: false,
            hidden_reason: Some(format!(
                "expected exactly one native display, found {display_count}; refusing ambiguous attribution"
            )),
            title: String::new(),
            rows: Vec::new(),
        });
    }
    let current_position = spaces
        .iter()
        .position(|s| *s == current)
        .expect("validated current member")
        + 1;
    let mut rows = Vec::with_capacity(spaces.len());
    for (index, sid) in spaces.iter().enumerate() {
        let position = index + 1;
        if mode == IndicatorMode::Current && *sid != current {
            continue;
        }
        rows.push(IndicatorRow {
            position,
            sid: *sid,
            current: *sid == current,
            occupancy: occupancy(*sid),
            // The armed select range is `1..=9` (see
            // `is_armed_select`): further ordinals are shown, never
            // dispatched — clicking one refuses with an explicit reason
            // instead of selecting the wrong Space.
            selectable: position <= 9,
            representative: representative(*sid),
        });
    }
    Ok(IndicatorProjection {
        visible: true,
        hidden_reason: None,
        title: current_position.to_string(),
        rows,
    })
}

// -- Click mapping ------------------------------------------------------------

/// Which rendered slot a click binding came from. The drain routes
/// on this kind alone: numbered bindings never fall back to trailing
/// creation, trailing bindings never resolve as numbered selects.
/// Carried by value from render to drain so a queued click can never
/// be reinterpreted against a newer render.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndicatorClickKind {
    Numbered,
    Trailing,
}

impl IndicatorClickKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Numbered => "numbered",
            Self::Trailing => "trailing",
        }
    }
}

/// Immutable per-segment click binding, captured at render time: the
/// slot kind plus the 1-based position the segment showed, the SID it
/// showed (numbered: the Space at that position; trailing: the
/// render-time native last SID), the UI generation that rendered it
/// (see [`IndicatorDriver`]), and the topology revision the row was
/// built from. Plain data; crosses the AppKit channel by value so a
/// queued click can never be reinterpreted against a newer render.
/// Never logged beyond position/kind tokens by callers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndicatorClick {
    pub kind: IndicatorClickKind,
    pub position: usize,
    pub expected_sid: u64,
    pub ui_generation: u64,
    pub topology_revision: u64,
}

/// Resolve one queued NUMBERED segment click against the CURRENT render
/// generation and a FRESH native-order user row plus its revision to the
/// armed select action. Numbered-only: a trailing-kind payload refuses
/// here (never selects), and stale numbered clicks never fall back to
/// trailing creation (route through [`resolve_indicator_click`], never
/// this function plus a trailing retry). Every staleness refuses with an
/// explicit reason instead of selecting the wrong Space: a UI rebuild
/// between render and drain (generation drift), a topology change between
/// render and drain (revision drift), a row that moved under the click
/// (SID mismatch), a vanished position, and positions past the armed
/// `1..=9` range (no armed route). The returned action still travels the
/// full guarded lane (engine unarmed / busy / topology gates plus the
/// drain's fresh fences and verified arrival), so this mapping is
/// necessary but never sufficient for a switch.
pub fn resolve_click(
    click: IndicatorClick,
    current_generation: u64,
    fresh_spaces: &[u64],
    fresh_revision: u64,
) -> Result<InputSemanticAction, String> {
    if click.kind != IndicatorClickKind::Numbered {
        return Err(
            "stale workspace click discarded: trailing slot click is never a numbered select; no select dispatched"
                .to_owned(),
        );
    }
    if click.position == 0 || click.position > 9 {
        return Err(format!(
            "workspace segment {} has no armed select route (armed range is 1..=9); refusing instead of selecting the wrong Space",
            click.position
        ));
    }
    if click.expected_sid == 0 {
        return Err("workspace click identity unprovable".to_owned());
    }
    if click.ui_generation != current_generation {
        return Err(format!(
            "stale workspace click discarded: status view rebuilt after render (generation {} != current {current_generation}); no select dispatched",
            click.ui_generation
        ));
    }
    if click.topology_revision != fresh_revision {
        return Err(format!(
            "stale workspace click discarded: topology revision moved after render ({} != current {fresh_revision}); no select dispatched",
            click.topology_revision
        ));
    }
    match fresh_spaces.get(click.position - 1) {
        Some(sid) if *sid == click.expected_sid => Ok(InputSemanticAction::WorkspaceSelect {
            index: click.position as u8,
        }),
        Some(_) => Err(format!(
            "stale workspace click discarded: native row moved under the click (row {} changed); no select dispatched",
            click.position
        )),
        None => Err(format!(
            "stale workspace click discarded: native row {} no longer exists; no select dispatched",
            click.position
        )),
    }
}

/// Map one menu click (1-based `position` tag plus the SID the row showed at
/// menu-build time) against a FRESH native-order user row to the armed
/// select action. The fresh row — not the tag alone — decides: a row that
/// moved under the open menu discards stale instead of selecting the wrong
/// Space, positions past the armed `1..=9` range refuse with an explicit
/// reason (they have no armed route), and a vanished position refuses.
/// The returned action still travels the full guarded lane (engine unarmed /
/// busy / topology gates plus the drain's fresh fences and verified
/// arrival), so this mapping is necessary but never sufficient for a
/// switch.
pub fn click_action(
    position: usize,
    expected_sid: u64,
    fresh_spaces: &[u64],
) -> Result<InputSemanticAction, String> {
    if position == 0 || position > 9 {
        return Err(format!(
            "workspace row {position} has no armed select route (armed range is 1..=9); refusing instead of selecting the wrong Space"
        ));
    }
    if expected_sid == 0 {
        return Err("workspace click identity unprovable".to_owned());
    }
    match fresh_spaces.get(position - 1) {
        Some(sid) if *sid == expected_sid => Ok(InputSemanticAction::WorkspaceSelect {
            index: position as u8,
        }),
        Some(_) => Err(format!(
            "stale workspace click discarded: native row moved under the menu (row {position} changed); no select dispatched"
        )),
        None => Err(format!(
            "stale workspace click discarded: native row {position} no longer exists; no select dispatched"
        )),
    }
}

// -- Trailing slot (pure offline seam; product indicator stays existing-only)

/// One trailing action slot: the 1-based position the extra segment would
/// show (always `spaces.len() + 1` at render time). The slot carries no SID
/// (the trailing Space may not exist yet); the drain ensures it (reuse
/// proven-empty last, else exact append) and never double-creates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrailingSlot {
    pub position: usize,
}

/// Project the trailing action slot over one native-order user row
/// (`None` = no extra slot: reuse the last existing slot instead of
/// duplicating it, or no slot at all). Rules:
/// - `Off` mode, non-single display, or unprovable row: `None` (same
///   hidden/provable rules as [`project_indicator`]).
/// - `spaces.len() >= MAX_ORDERED`: `None` (shared bound reached).
/// - Last occupancy `Empty`: `None` (reuse the last existing slot — the
///   drain reuses proven-empty last; never double-create).
/// - Last `Occupied` or `Unknown`: `Some` slot at `len + 1` — the
///   indicator offers the trailing action even when the last is occupied;
///   unknown still offers the slot (the drain refuses unknown emptiness
///   instead of appending).
/// - `Current` mode: trailing slot is offered only when the single shown
///   row is the last row (same rule over the full row); callers pass the
///   full row regardless of mode.
#[must_use]
pub fn project_trailing_slot(
    spaces: &[u64],
    current: u64,
    display_count: usize,
    mode: IndicatorMode,
    last_occupancy: SpaceOccupancy,
) -> Option<TrailingSlot> {
    if mode == IndicatorMode::Off || display_count != 1 {
        return None;
    }
    if spaces.is_empty() || spaces.contains(&0) || !spaces.contains(&current) {
        return None;
    }
    if spaces.len() >= tiler_core::workspace::MAX_ORDERED {
        return None;
    }
    match last_occupancy {
        SpaceOccupancy::Empty => None,
        SpaceOccupancy::Occupied | SpaceOccupancy::Unknown => Some(TrailingSlot {
            position: spaces.len() + 1,
        }),
    }
}

/// Title for the trailing action segment: `"+"` when the last Space is
/// proven occupied (pressing appends), `"+?"` when occupancy is unknown
/// (shown unavailable and disabled, never falsely empty). Proven-empty
/// last reuses the existing slot, so no trailing segment renders.
#[must_use]
pub fn trailing_segment_title(slot: TrailingSlot, last_occupancy: SpaceOccupancy) -> String {
    let _ = slot;
    match last_occupancy {
        SpaceOccupancy::Occupied => "+".to_owned(),
        SpaceOccupancy::Empty | SpaceOccupancy::Unknown => "+?".to_owned(),
    }
}

/// Immutable trailing click binding, captured at render time: the UI
/// generation and topology revision that rendered the trailing slot plus
/// the render-time row length and last SID. Plain data; a queued click can
/// never be reinterpreted against a newer render or topology.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrailingClick {
    pub ui_generation: u64,
    pub topology_revision: u64,
    pub render_len: usize,
    pub render_last_sid: u64,
}

/// Resolve one queued trailing click against the CURRENT render generation
/// and a FRESH native-order user row plus its revision to the trailing
/// select action (`index 0`). Every staleness refuses with an explicit
/// reason instead of creating/selecting the wrong Space: a UI rebuild
/// between render and drain (generation drift), a topology change between
/// render and drain (revision drift), or a row that moved under the click
/// (length or last-SID mismatch). The returned action still travels the
/// full guarded lane (explicit test arming, single-display, gates, drain
/// fences, verified arrival), so this mapping is necessary but never
/// sufficient for a switch. Stay-0 has no route and is never constructed.
pub fn resolve_trailing_click(
    click: TrailingClick,
    current_generation: u64,
    fresh_spaces: &[u64],
    fresh_revision: u64,
) -> Result<InputSemanticAction, String> {
    if click.render_len == 0 || click.render_last_sid == 0 {
        return Err("trailing click identity unprovable".to_owned());
    }
    if click.ui_generation != current_generation {
        return Err(format!(
            "stale trailing click discarded: status view rebuilt after render (generation {} != current {current_generation}); no select dispatched",
            click.ui_generation
        ));
    }
    if click.topology_revision != fresh_revision {
        return Err(format!(
            "stale trailing click discarded: topology revision moved after render ({} != current {fresh_revision}); no select dispatched",
            click.topology_revision
        ));
    }
    if fresh_spaces.len() != click.render_len {
        return Err(
            "stale trailing click discarded: native row length moved under the click; no select dispatched"
                .to_owned(),
        );
    }
    match fresh_spaces.last() {
        Some(sid) if *sid == click.render_last_sid => Ok(InputSemanticAction::WorkspaceSelect {
            index: 0,
        }),
        Some(_) => Err(
            "stale trailing click discarded: native row moved under the click (last SID changed); no select dispatched"
                .to_owned(),
        ),
        None => Err(
            "stale trailing click discarded: native row empty; no select dispatched".to_owned(),
        ),
    }
}

/// Route one queued render binding to its armed select action by the kind
/// carried at render time: numbered bindings resolve only through
/// [`resolve_click`], trailing bindings only through
/// [`resolve_trailing_click`]. No fallback exists in either direction: a
/// stale numbered click (row shrank, reordered, or drifted) refuses as
/// stale instead of creating a trailing Space, and a numbered-kind
/// payload at a trailing position refuses the same way. `trailing_enabled`
/// threads the capability gate explicitly (production `false` refuses
/// trailing before the engine lane); callers pass
/// [`crate::workspace_input::TRAILING_CAPABILITY_ENABLED`], tests pass
/// explicit `true`/`false`. The returned action still travels the full
/// guarded lane, so this routing is necessary but never sufficient.
pub fn resolve_indicator_click(
    click: IndicatorClick,
    current_generation: u64,
    fresh_spaces: &[u64],
    fresh_revision: u64,
    trailing_enabled: bool,
) -> Result<InputSemanticAction, String> {
    match click.kind {
        IndicatorClickKind::Numbered => {
            resolve_click(click, current_generation, fresh_spaces, fresh_revision)
        }
        IndicatorClickKind::Trailing => {
            if !trailing_enabled {
                return Err(
                    "trailing slot unarmed: no trailing append runs; no select dispatched"
                        .to_owned(),
                );
            }
            resolve_trailing_click(
                TrailingClick {
                    ui_generation: click.ui_generation,
                    topology_revision: click.topology_revision,
                    render_len: click.position.saturating_sub(1),
                    render_last_sid: click.expected_sid,
                },
                current_generation,
                fresh_spaces,
                fresh_revision,
            )
        }
    }
}

/// Diagnostic-only pick of the ACTUAL rendered trailing binding for the
/// `--diag-trailing-click` harness: copies the trailing binding off the
/// live AppKit targets (position, expected SID, generation, revision -
/// the same tuple [`crate::ffi::WorkspaceIndicator::segment_targets`]
/// reads off the retained target objects, never the cached driver model
/// and never a reconstructed select-0 substitute) and returns it only
/// when its segment reads enabled there (the live trailing slot: proven
/// occupied last; a disabled `+?` slot refuses instead of injecting -
/// the drain would refuse its unknown occupancy anyway). `None` when no
/// trailing slot rendered, when several render (ambiguous, never guessed),
/// or when the slot reads disabled. The returned binding still travels
/// the full guarded lane ([`resolve_indicator_click`] with the capability
/// gate plus engine admission plus the drain's fresh fences and verified
/// arrival), so this pick is necessary but never sufficient for a switch.
/// Pure over the readback tuples; unit-tested.
#[must_use]
pub fn diag_rendered_trailing_click(
    targets: &[(usize, u64, u64, u64, IndicatorClickKind)],
    enabled_at: &dyn Fn(usize) -> bool,
) -> Option<IndicatorClick> {
    let mut trailing = targets
        .iter()
        .filter(|row| row.4 == IndicatorClickKind::Trailing);
    let (position, expected_sid, ui_generation, topology_revision, kind) = *trailing.next()?;
    if trailing.next().is_some() {
        return None;
    }
    if !enabled_at(position) {
        return None;
    }
    Some(IndicatorClick {
        kind,
        position,
        expected_sid,
        ui_generation,
        topology_revision,
    })
}

/// Bound one discard reason for the log: reasons name positions,
/// generations, and revisions only (never SIDs, bundles, or titles);
/// truncation keeps the line bounded while staying honest about which
/// fence refused.
#[must_use]
pub fn bound_click_discard_reason(reason: &str) -> String {
    const LIMIT: usize = 160;
    if reason.len() <= LIMIT {
        reason.to_owned()
    } else {
        format!("{}...", &reason[..LIMIT])
    }
}

// -- Occupancy summary (pure) -------------------------------------------------

/// One enumerated window for the occupancy summary: plain IDs plus the
/// classifier facts plus the live membership read (`Err` = unknown, never
/// "no Space"). Built live from
/// [`crate::mc_observe::enumerate_window_members`]; tests inject fakes.
#[derive(Debug, Clone, PartialEq)]
pub struct MemberView {
    pub wid: u32,
    pub pid: u32,
    pub bundle: String,
    pub layer: i32,
    pub onscreen: bool,
    pub w: f64,
    pub h: f64,
    pub exe: Option<String>,
    pub exe_alias: Option<String>,
    pub ax_windows_absent: Option<bool>,
    pub spaces: Result<Vec<u64>, ()>,
}

/// Summarize occupancy plus the representative bundle for every Space in
/// `spaces` over one enumeration. Shell chrome never counts (the
/// [`crate::mc_probe::classify_window_full`] rule with `spaces` as the user
/// set, per Space as the temp — exactly the
/// [`crate::workspace_select::SelectionBackend::empty`] rule, batched).
/// A Space is `Occupied` only on a proven positively-sized member homed
/// exactly to it; any unknown membership or unreadable/zero-area geometry
/// that could sit on it reads `Unknown`; otherwise `Empty`.
///
/// The representative bundle is deterministic: the frontmost PID's member
/// wins when it sits on the Space, else the lowest `(pid, wid)` member;
/// `None` without a member. Hidden (off-current) members count exactly like
/// current ones — retained layouts report occupancy after a native switch
/// away.
pub fn summarize_spaces(
    spaces: &[u64],
    members: &[MemberView],
    frontmost_pid: Option<u32>,
) -> Vec<(SpaceOccupancy, Option<String>)> {
    spaces
        .iter()
        .map(|sid| {
            let mut occupied = false;
            let mut unknown = false;
            let mut candidates: Vec<(u32, u32, String)> = Vec::new();
            for m in members {
                let window = crate::mc_probe::WindowMember {
                    wid: m.wid,
                    pid: m.pid,
                    bundle: m.bundle.clone(),
                    layer: m.layer,
                    onscreen: m.onscreen,
                    w: m.w,
                    h: m.h,
                    exe: m.exe.clone(),
                    exe_alias: m.exe_alias.clone(),
                    ax_windows_absent: m.ax_windows_absent,
                };
                let membership = m.spaces.as_ref().ok().map(Vec::as_slice);
                if crate::mc_probe::classify_window_full(&window, membership, spaces, *sid).0
                    == crate::mc_probe::WindowClass::Shell
                {
                    continue;
                }
                let homed = match &m.spaces {
                    Ok(homed) => homed,
                    Err(()) => {
                        // Unknown membership could sit on this Space:
                        // never claim empty past it.
                        unknown = true;
                        continue;
                    }
                };
                match crate::workspace_select::candidate_occupied(
                    m.wid, m.pid, m.w, m.h, homed, *sid,
                ) {
                    Ok(true) => {
                        occupied = true;
                        candidates.push((m.pid, m.wid, m.bundle.clone()));
                    }
                    Ok(false) => {}
                    // Zero-area/unreadable geometry is ambiguous, never an
                    // empty verdict (the `candidate_occupied` rule).
                    Err(_) => {
                        unknown = true;
                    }
                }
            }
            let occupancy = if occupied {
                SpaceOccupancy::Occupied
            } else if unknown {
                SpaceOccupancy::Unknown
            } else {
                SpaceOccupancy::Empty
            };
            candidates.sort();
            let representative = frontmost_pid
                .and_then(|pid| {
                    candidates
                        .iter()
                        .filter(|(cpid, _, _)| *cpid == pid)
                        .map(|(_, _, bundle)| bundle.clone())
                        .next()
                })
                .or_else(|| candidates.first().map(|(_, _, bundle)| bundle.clone()));
            (occupancy, representative)
        })
        .collect()
}

/// Live occupancy scan for one native-order user row: one full CG + SLS
/// membership enumeration plus the frontmost PID (plain IDs only), through
/// the pure [`summarize_spaces`]. Read-only (CG/SLS/LaunchServices reads,
/// no AX, no writes). An unreadable enumeration or user set degrades every
/// row to `Unknown` — never to `Empty` — so the indicator withholds the
/// empty verdict instead of claiming it.
pub(crate) fn scan_space_state(
    spaces: &[u64],
) -> Result<Vec<(SpaceOccupancy, Option<String>)>, String> {
    let members = (|| {
        let user = crate::mc_observe::fresh_user_spaces().map_err(|e| e.to_string())?;
        let live = crate::mc_observe::enumerate_window_members().map_err(|e| e.to_string())?;
        // The shell rule needs the queried Space inside the user set; the
        // row already reads user-typed, and `user` proves it fresh.
        for sid in spaces {
            if !user.contains(sid) {
                return Err(format!(
                    "Space {sid} left the fresh user set; occupancy unknown"
                ));
            }
        }
        Ok::<Vec<MemberView>, String>(
            live.into_iter()
                .map(|m| MemberView {
                    wid: m.wid,
                    pid: m.pid,
                    bundle: m.bundle,
                    layer: m.layer,
                    onscreen: m.onscreen,
                    w: m.w,
                    h: m.h,
                    exe: m.exe,
                    exe_alias: m.exe_alias,
                    ax_windows_absent: m.ax_windows_absent,
                    spaces: m.spaces.map_err(|_| ()),
                })
                .collect(),
        )
    })();
    match members {
        Ok(members) => {
            let frontmost = crate::ffi::frontmost_app()
                .map(|(pid, _)| pid as u32)
                .filter(|pid| *pid > 1);
            Ok(summarize_spaces(spaces, &members, frontmost))
        }
        Err(_) => Ok(spaces
            .iter()
            .map(|_| (SpaceOccupancy::Unknown, None))
            .collect()),
    }
}

/// Read-only status dump for the `workspace-indicator-status` fixture: the
/// fresh single-display user row plus per-row occupancy over one live
/// scan. No UI is created, nothing is written. Counts and ordinals only —
/// no bundle IDs, no titles, no paths.
///
/// Explicitly live-scan evidence, NOT retained-state evidence: this reads
/// one CG/SLS enumeration directly (the production indicator instead
/// renders the engine's retained domains via [`summarize_retained`]).
/// Off-scope staleness and duplicate reads are fixture-accepted here and
/// must never be quoted as the indicator's live occupancy.
pub fn run_status() -> Result<String, (i32, String)> {
    let display_count = crate::workspace_select::fresh_display_count().map_err(|e| (1, e))?;
    if display_count != 1 {
        return Err((
            1,
            serde_json::json!({
                "action": "workspace-indicator-status",
                "display_count": display_count,
                "refused": format!("expected exactly one native display, found {display_count}; refusing ambiguous attribution"),
            })
            .to_string(),
        ));
    }
    let row = crate::workspace_select::fresh_user_row(0).map_err(|e| (1, e))?;
    let state = scan_space_state(&row.spaces).map_err(|e| (1, e))?;
    let rows: Vec<serde_json::Value> = row
        .spaces
        .iter()
        .enumerate()
        .map(|(index, sid)| {
            let position = index + 1;
            let (occupancy, representative) = &state[index];
            serde_json::json!({
                "position": position,
                "current": *sid == row.current,
                "occupancy": format!("{occupancy:?}").to_lowercase(),
                "selectable": position <= 9,
                "representative_present": representative.is_some(),
            })
        })
        .collect();
    Ok(serde_json::json!({
        "action": "workspace-indicator-status",
        "display_count": display_count,
        "space_count": row.spaces.len(),
        "current_position": row.spaces.iter().position(|s| *s == row.current).map(|p| p + 1),
        "rows": rows,
        "note": "read-only live CG/SLS scan evidence (not retained engine state); no status item created, no writes, no titles/bundles/paths",
    })
    .to_string())
}

// -- Retained-domain occupancy summary (pure, production path) ---------------

/// One engine-retained domain's managed membership for the occupancy
/// summary: the native Space plus its exactly-homed retained `(pid, wid)`
/// members (the same homing the engine plans from — current and hidden
/// alike). No titles, no bundles, no paths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetainedDomainState {
    pub sid: u64,
    pub members: Vec<(u32, u32)>,
}

/// Summarize occupancy plus the representative member over the retained
/// engine domains for every Space in `spaces` (native order in, native
/// order out). Managed windows only: a Space reads `Occupied` on a
/// non-empty retained membership, `Empty` on a proven-known-but-memberless
/// domain, and `Unknown` whenever completeness is unproven — no report
/// (`complete == false`), or a Space the report never names (never claim
/// empty past a coverage gap). The representative member is deterministic:
/// the frontmost PID's member wins when it sits on the Space, else the
/// lowest `(pid, wid)` member; `None` without a member. Callers map the
/// representative PID to a bundle ID for the icons flag (never logged).
pub fn summarize_retained(
    spaces: &[u64],
    domains: &[RetainedDomainState],
    complete: bool,
    frontmost_pid: Option<u32>,
) -> Vec<(SpaceOccupancy, Option<(u32, u32)>)> {
    spaces
        .iter()
        .map(|sid| {
            if !complete {
                return (SpaceOccupancy::Unknown, None);
            }
            let Some(domain) = domains.iter().find(|d| d.sid == *sid) else {
                // The report never named this Space: coverage gap, never
                // an empty verdict.
                return (SpaceOccupancy::Unknown, None);
            };
            if domain.members.is_empty() {
                return (SpaceOccupancy::Empty, None);
            }
            let mut members = domain.members.clone();
            members.sort();
            let representative = frontmost_pid
                .and_then(|pid| {
                    members
                        .iter()
                        .filter(|(mpid, _)| *mpid == pid)
                        .map(|(_, wid)| (pid, *wid))
                        .next()
                })
                .or_else(|| members.first().copied());
            (SpaceOccupancy::Occupied, representative)
        })
        .collect()
}

/// Rendered title for one segment button (and its mirror menu item):
/// the 1-based ordinal with an explicit `'?'` suffix if and only if
/// occupancy is Unknown. The suffix is the honest Unknown marker: the
/// native render sets no label colour, and a current-plus-Unknown segment
/// is otherwise pixel-identical to current-plus-Occupied (bordered bezel
/// plus bold in both), so the tooltip word alone is not enough. Icons
/// never replace the number; the suffix rides the number.
#[must_use]
pub fn segment_title(position: usize, occupancy: SpaceOccupancy) -> String {
    match occupancy {
        SpaceOccupancy::Unknown => format!("{position}?"),
        SpaceOccupancy::Occupied | SpaceOccupancy::Empty => position.to_string(),
    }
}

/// User-facing tooltip for one rendered segment, doubling as the
/// machine-readable appearance contract fixtures parse back: plain ordinal
/// plus occupancy word plus flags. Positions past the armed range name the
/// missing route instead of silently disabling.
#[must_use]
pub fn segment_tooltip(
    position: usize,
    occupancy: SpaceOccupancy,
    current: bool,
    enabled: bool,
) -> String {
    let occupancy_word = match occupancy {
        SpaceOccupancy::Occupied => "occupied",
        SpaceOccupancy::Empty => "empty",
        SpaceOccupancy::Unknown => "unknown",
    };
    let mut tip = format!("Space {position}: {occupancy_word}");
    if current {
        tip.push_str(", current");
    }
    if !enabled {
        tip.push_str(", no armed select route");
    }
    tip
}

/// Parse one live segment tooltip back into `(position, occupancy,
/// current)`: `None` on any shape drift, so fixtures fail loudly instead
/// of trusting a stale contract.
#[must_use]
pub fn parse_segment_tooltip(tip: &str) -> Option<(usize, SpaceOccupancy, bool)> {
    let (head, rest) = tip.split_once(':')?;
    let position: usize = head.strip_prefix("Space ")?.trim().parse().ok()?;
    if position == 0 {
        return None;
    }
    let rest = rest.trim();
    let occupancy = if rest.starts_with("occupied") {
        SpaceOccupancy::Occupied
    } else if rest.starts_with("empty") {
        SpaceOccupancy::Empty
    } else if rest.starts_with("unknown") {
        SpaceOccupancy::Unknown
    } else {
        return None;
    };
    Some((position, occupancy, rest.contains("current")))
}

// -- Driver (pure production-path state machine) -----------------------------

/// Outcome of offering one fresh projection to the driver.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriverApply {
    /// Stored as the new current render; carries the fresh UI generation
    /// every segment click from this render must bind.
    Applied(u64),
    /// Identical to the cached render: the native item stays untouched.
    Duplicate,
    /// The driver is disarmed (off mode): nothing is stored, no native
    /// item may exist.
    Disarmed,
}

/// Pure menu-bar indicator driver: the production tick/teardown/click
/// pipeline without any AppKit. The tile controller owns one and delegates
/// every state decision here, so the dedup, lifecycle, and identity rules
/// below are offline-testable and the native shell stays a thin applier.
///
/// The dedup key is the projection PLUS the trailing action slot (slot
/// presence plus its occupancy): a slot appearing, disappearing, or
/// flipping availability while the numbered rows read identical still
/// applies with a new generation — otherwise the extra segment would go
/// stale while the rows dedup.
///
/// Generations are monotonic per applied render (never reset, not even by
/// [`IndicatorDriver::teardown`]): a click rendered before a teardown or a
/// rebuild carries an older generation and
/// [`resolve_click`] refuses it — queued clicks can never outlive their
/// render.
#[derive(Debug, Clone)]
pub struct IndicatorDriver {
    armed: bool,
    generation: u64,
    last: Option<IndicatorProjection>,
    last_trailing: Option<(TrailingSlot, SpaceOccupancy)>,
    last_revision: Option<u64>,
    last_observations: u64,
    updates: u64,
    clicks: u64,
}

impl IndicatorDriver {
    /// New driver: armed unless the mode is off (off mode never renders,
    /// never queues, never dispatches).
    pub fn new(mode: IndicatorMode) -> Self {
        Self {
            armed: mode != IndicatorMode::Off,
            generation: 0,
            last: None,
            last_trailing: None,
            last_revision: None,
            last_observations: 0,
            updates: 0,
            clicks: 0,
        }
    }

    /// Whether the controller may render and drain at all.
    #[must_use]
    pub const fn armed(&self) -> bool {
        self.armed
    }

    /// Current UI generation: every segment rendered from the latest
    /// applied projection binds this; older queued clicks refuse.
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    /// Cached render (the clicks drain against this).
    #[must_use]
    pub fn last(&self) -> Option<&IndicatorProjection> {
        self.last.as_ref()
    }

    /// Cached trailing slot the clicks drain against (`None` when the last
    /// applied render offered no extra slot).
    #[must_use]
    pub const fn last_trailing(&self) -> Option<(TrailingSlot, SpaceOccupancy)> {
        self.last_trailing
    }

    /// Applied native pushes (visible renders only).
    #[must_use]
    pub const fn updates(&self) -> u64 {
        self.updates
    }

    /// Accepted click submissions through the guarded lane.
    #[must_use]
    pub const fn clicks(&self) -> u64 {
        self.clicks
    }

    /// Whether a fresh snapshot revision / engine observation count pair
    /// requires a projection rebuild: only a moved revision, a published
    /// observation, or an empty cache rebuilds — never a blind per-cadence
    /// poll. Disarmed drivers never rebuild.
    #[must_use]
    pub fn needs_tick(&self, revision: u64, observations: u64) -> bool {
        if !self.armed {
            return false;
        }
        self.last.is_none()
            || self.last_revision.is_some_and(|r| r != revision)
            || self.last_observations != observations
    }

    /// Record that a rebuild was computed at `revision`/`observations`
    /// (call on every computed tick, applied or duplicate, so the trigger
    /// stays edge-driven). Returns the apply verdict: changed projections
    /// advance the generation, identical ones stay untouched, hidden
    /// projections store without counting a visible update, and disarmed
    /// drivers store nothing. The trailing slot rides the dedup key with
    /// the projection (see the struct docs): a slot-only change applies.
    pub fn apply(
        &mut self,
        projection: IndicatorProjection,
        trailing: Option<(TrailingSlot, SpaceOccupancy)>,
        revision: u64,
        observations: u64,
    ) -> DriverApply {
        if !self.armed {
            return DriverApply::Disarmed;
        }
        self.last_revision = Some(revision);
        self.last_observations = observations;
        if self.last.as_ref() == Some(&projection) && self.last_trailing == trailing {
            return DriverApply::Duplicate;
        }
        self.generation += 1;
        let visible = projection.visible;
        self.last = Some(projection);
        self.last_trailing = trailing;
        if visible {
            self.updates += 1;
        }
        DriverApply::Applied(self.generation)
    }

    /// Lifecycle/gate teardown (lock, lock screen, stale topology,
    /// shutdown): forget the cached render so the next good snapshot
    /// refreshes from scratch. The generation is NOT reset, so clicks
    /// queued before the teardown refuse as stale instead of dispatching
    /// under an unsafe context; the caller drops the queued payloads
    /// without dispatching.
    pub fn teardown(&mut self) {
        self.last = None;
        self.last_trailing = None;
    }

    /// Record one accepted click submission (guarded-lane `Accepted` only;
    /// refusals and discards never count).
    pub fn click_submitted(&mut self) {
        self.clicks += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn occupancy(state: &[(SpaceOccupancy, Option<String>)]) -> Vec<SpaceOccupancy> {
        state.iter().map(|(o, _)| *o).collect()
    }

    fn member(wid: u32, pid: u32, bundle: &str, spaces: Result<Vec<u64>, ()>) -> MemberView {
        MemberView {
            wid,
            pid,
            bundle: bundle.to_owned(),
            layer: 0,
            onscreen: true,
            w: 480.0,
            h: 320.0,
            exe: None,
            exe_alias: None,
            ax_windows_absent: None,
            spaces,
        }
    }

    #[test]
    fn mode_parses_and_defaults_to_all() {
        assert_eq!(IndicatorMode::parse("all"), Ok(IndicatorMode::All));
        assert_eq!(IndicatorMode::parse("Current"), Ok(IndicatorMode::Current));
        assert_eq!(
            IndicatorMode::parse("current-only"),
            Ok(IndicatorMode::Current)
        );
        assert_eq!(IndicatorMode::parse("OFF"), Ok(IndicatorMode::Off));
        assert!(IndicatorMode::parse("numbers").is_err());
        assert!(IndicatorMode::parse("").is_err());
        assert_eq!(IndicatorMode::default(), IndicatorMode::All);
        assert_eq!(IndicatorMode::All.as_str(), "all");
        assert_eq!(IndicatorOptions::default().mode, IndicatorMode::All);
        assert!(!IndicatorOptions::default().icons);
    }

    #[test]
    fn cli_overrides_file_over_defaults() {
        let file = crate::dev_selection::WorkspaceIndicatorSettings {
            mode: Some(IndicatorMode::Current),
            icons: Some(true),
        };
        // Nothing explicit: file wins.
        let resolved = resolve_indicator_config(&IndicatorCli::default(), Some(&file)).unwrap();
        assert_eq!(
            resolved,
            IndicatorOptions {
                mode: IndicatorMode::Current,
                icons: true,
            }
        );
        // Explicit CLI wins per field.
        let cli = IndicatorCli {
            mode: Some(IndicatorMode::Off),
            icons: None,
        };
        let resolved = resolve_indicator_config(&cli, Some(&file)).unwrap();
        assert_eq!(
            resolved,
            IndicatorOptions {
                mode: IndicatorMode::Off,
                icons: true,
            }
        );
        // Nothing anywhere: all rows, no icons.
        let resolved = resolve_indicator_config(&IndicatorCli::default(), None).unwrap();
        assert_eq!(resolved, IndicatorOptions::default());
    }

    #[test]
    fn settings_section_parses_and_refuses_unknown_fields() {
        let settings: crate::dev_selection::DevSettings =
            serde_json::from_str(r#"{"workspace_indicator":{"mode":"current","icons":true}}"#)
                .expect("parses");
        let section = settings.workspace_indicator.as_ref().expect("section");
        assert_eq!(section.mode, Some(IndicatorMode::Current));
        assert_eq!(section.icons, Some(true));
        assert!(settings.validate().is_ok());
        assert!(
            serde_json::from_str::<crate::dev_selection::DevSettings>(
                r#"{"workspace_indicator":{"style":"fancy"}}"#
            )
            .is_err()
        );
        assert!(
            serde_json::from_str::<crate::dev_selection::DevSettings>(
                r#"{"workspace_indicator":{"mode":"numbers"}}"#
            )
            .is_err()
        );
    }

    #[test]
    fn projection_preserves_native_order_with_current_title() {
        // Deliberately nonascending SIDs: order preserved, never sorted,
        // never current-first. Trailing empty included.
        let spaces = vec![20, 3, 41];
        let projection = project_indicator(
            &spaces,
            3,
            1,
            IndicatorMode::All,
            &|_| SpaceOccupancy::Empty,
            &|_| None,
        )
        .unwrap();
        assert!(projection.visible);
        assert_eq!(
            projection.rows.iter().map(|r| r.sid).collect::<Vec<_>>(),
            spaces
        );
        assert_eq!(
            projection
                .rows
                .iter()
                .map(|r| r.position)
                .collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
        assert_eq!(projection.title, "2");
        assert_eq!(
            projection
                .rows
                .iter()
                .map(|r| r.current)
                .collect::<Vec<_>>(),
            vec![false, true, false]
        );
        assert!(projection.rows.iter().all(|r| r.selectable));
    }

    #[test]
    fn projection_marks_current_occupied_empty_unknown() {
        let spaces = vec![11, 12, 13, 14];
        let projection = project_indicator(
            &spaces,
            12,
            1,
            IndicatorMode::All,
            &|sid| match sid {
                11 => SpaceOccupancy::Occupied,
                12 => SpaceOccupancy::Occupied,
                13 => SpaceOccupancy::Empty,
                _ => SpaceOccupancy::Unknown,
            },
            &|_| None,
        )
        .unwrap();
        // Unknown (14) must not read empty: the verdict rides through
        // verbatim, never collapsed.
        assert_eq!(
            projection
                .rows
                .iter()
                .map(|r| r.occupancy)
                .collect::<Vec<_>>(),
            vec![
                SpaceOccupancy::Occupied,
                SpaceOccupancy::Occupied,
                SpaceOccupancy::Empty,
                SpaceOccupancy::Unknown,
            ]
        );
        assert_eq!(projection.title, "2");
    }

    #[test]
    fn projection_beyond_nine_shows_but_never_dispatches() {
        let spaces: Vec<u64> = (101..=112).collect();
        let projection = project_indicator(
            &spaces,
            101,
            1,
            IndicatorMode::All,
            &|_| SpaceOccupancy::Empty,
            &|_| None,
        )
        .unwrap();
        assert_eq!(projection.rows.len(), 12);
        assert!(
            projection.rows[..9].iter().all(|r| r.selectable),
            "first nine dispatch"
        );
        assert!(
            projection.rows[9..].iter().all(|r| !r.selectable),
            "tenth-plus shown, never dispatched"
        );
        // Click mapping refuses past-nine with an explicit reason (no
        // silent wrong-Space select, no index-0 trailing confusion).
        let err = click_action(10, 110, &spaces).expect_err("past-nine refuses");
        assert!(err.contains("1..=9"), "{err}");
        assert!(click_action(0, 101, &spaces).is_err());
    }

    #[test]
    fn projection_current_only_off_and_multidisplay() {
        let spaces = vec![20, 3];
        let current_only = project_indicator(
            &spaces,
            3,
            1,
            IndicatorMode::Current,
            &|_| SpaceOccupancy::Occupied,
            &|_| None,
        )
        .unwrap();
        assert!(current_only.visible);
        assert_eq!(current_only.rows.len(), 1);
        assert_eq!(current_only.rows[0].sid, 3);
        assert_eq!(current_only.title, "2");
        let off = project_indicator(
            &spaces,
            3,
            1,
            IndicatorMode::Off,
            &|_| SpaceOccupancy::Occupied,
            &|_| None,
        )
        .unwrap();
        assert!(!off.visible);
        assert!(off.rows.is_empty());
        assert!(
            off.hidden_reason
                .as_deref()
                .is_some_and(|r| r.contains("configuration"))
        );
        // Zero or multiple displays hide with an explicit reason instead of
        // guessing attribution (the single-display restriction, rendered).
        for displays in [0, 2] {
            let hidden = project_indicator(
                &spaces,
                3,
                displays,
                IndicatorMode::All,
                &|_| SpaceOccupancy::Occupied,
                &|_| None,
            )
            .unwrap();
            assert!(!hidden.visible, "{displays} displays hide");
            assert!(
                hidden
                    .hidden_reason
                    .as_deref()
                    .is_some_and(|r| r.contains("exactly one native display")),
                "{hidden:?}"
            );
        }
    }

    #[test]
    fn projection_refuses_unprovable_rows() {
        let none = |_: u64| SpaceOccupancy::Empty;
        let no_rep = |_: u64| None;
        assert!(project_indicator(&[], 3, 1, IndicatorMode::All, &none, &no_rep).is_err());
        assert!(project_indicator(&[0, 3], 3, 1, IndicatorMode::All, &none, &no_rep).is_err());
        assert!(project_indicator(&[20, 3], 7, 1, IndicatorMode::All, &none, &no_rep).is_err());
        assert!(project_indicator(&[3, 3], 3, 1, IndicatorMode::All, &none, &no_rep).is_err());
    }

    #[test]
    fn click_mapping_needs_fresh_identity_match() {
        // Exact match dispatches the armed ordinal (native order, not SID).
        assert_eq!(
            click_action(2, 3, &[20, 3]),
            Ok(InputSemanticAction::WorkspaceSelect { index: 2 })
        );
        // Row moved under the open menu: stale discards, never the wrong
        // Space (a deletion shifts every later ordinal).
        let err = click_action(2, 3, &[20, 41]).expect_err("moved row discards");
        assert!(err.contains("stale"), "{err}");
        // Vanished position discards too.
        let err = click_action(2, 3, &[20]).expect_err("vanished row discards");
        assert!(err.contains("stale"), "{err}");
        // Zero identity never dispatches.
        assert!(click_action(1, 0, &[20]).is_err());
    }

    /// Fake guarded selector backend (the [`crate::workspace_select`]
    /// contract): one native display row, single verified dispatch, no
    /// replay. The click chain ends here exactly like a tap select.
    struct ClickFake {
        row: crate::workspace_select::SelectionSnapshot,
        selects: Vec<u64>,
    }

    impl crate::workspace_select::SelectionBackend for ClickFake {
        fn display_count(&mut self) -> Result<usize, String> {
            Ok(1)
        }
        fn snapshot(&mut self) -> Result<crate::workspace_select::SelectionSnapshot, String> {
            Ok(self.row.clone())
        }
        fn check_output(&mut self, output: &str) -> Result<(), String> {
            if output == "macos-display-1" {
                Ok(())
            } else {
                Err(format!(
                    "product select refuses: reply target output {output} is not the live display macos-display-1; no actuation"
                ))
            }
        }
        fn empty(&mut self, _: u64) -> Result<bool, String> {
            Ok(true)
        }
        fn create(&mut self, _before: &crate::workspace_select::SelectionSnapshot) -> Result<(), crate::workspace_select::DispatchError> {
            unreachable!("armed clicks never create");
        }
        fn select(
            &mut self,
            _: &crate::workspace_select::SelectionSnapshot,
            sid: u64,
        ) -> Result<(), crate::workspace_select::DispatchError> {
            self.selects.push(sid);
            self.row.current = sid;
            Ok(())
        }
    }

    fn click_fake() -> ClickFake {
        ClickFake {
            row: crate::workspace_select::SelectionSnapshot {
                display: "display-a".to_owned(),
                spaces: vec![20, 3],
                current: 20,
            },
            selects: Vec::new(),
        }
    }

    fn open_gates() -> crate::workspace_select::SelectGates {
        crate::workspace_select::SelectGates {
            session_unlocked: true,
            secure_active: false,
            lock_screen: false,
        }
    }

    #[test]
    fn click_chain_reaches_verified_dispatch_or_refuses_before_touch() {
        // Happy path: menu row 2 (SID 3) rides a reply plan (core-resolved
        // target plus live binding) with the rendered SID as a staleness
        // fence only. The guarded drain verifies arrival with exactly one
        // dispatch.
        use crate::runtime_types::ClaimedSelect;
        fn plan(action: crate::runtime_types::InputSemanticAction, op: &str) -> ClaimedSelect {
            let mut claim = ClaimedSelect::tap(action);
            claim.operation_id = Some(op.to_owned());
            claim.select_target_output = Some("macos-display-1".to_owned());
            claim.select_target_workspace = Some("ws-2".to_owned());
            claim.select_native_binding = Some("space-3".to_owned());
            claim.select_baseline_sid = Some(20);
            claim.expected_sid = Some(3);
            claim.topology_revision = None;
            claim
        }
        let mut fake = click_fake();
        let mut state = crate::workspace_select::SelectionState::default();
        let action = click_action(2, 3, &fake.row.spaces.clone()).expect("fresh click maps");
        let success = crate::workspace_select::drain_claimed_select(
            &mut state,
            &mut fake,
            plan(action, "op-1"),
            &open_gates(),
        )
        .expect("guarded drain dispatches");
        assert_eq!(fake.selects, vec![3], "single dispatch, never replayed");
        assert!(success.effect.changed);
        assert_eq!(fake.row.current, 3, "verified arrival");
        // Stale menu (row moved under the open menu): the mapping refuses
        // before any backend touch — the wrong Space is never selected.
        let fake = click_fake();
        let err = click_action(2, 3, &[20, 41]).expect_err("stale click refuses");
        assert!(err.contains("stale"), "{err}");
        assert!(fake.selects.is_empty());
        // Past-nine rows refuse at the mapping (no armed route), before
        // any backend touch.
        let err = click_action(10, 110, &[20, 3]).expect_err("past-nine refuses");
        assert!(err.contains("1..=9"), "{err}");
        assert!(fake.selects.is_empty());
        // Gated contexts (lock, Secure Input, lock screen) refuse inside
        // the drain with no dispatch and no history advance — the same
        // gates a tap select meets.
        for gates in [
            crate::workspace_select::SelectGates {
                session_unlocked: false,
                ..open_gates()
            },
            crate::workspace_select::SelectGates {
                secure_active: true,
                ..open_gates()
            },
            crate::workspace_select::SelectGates {
                lock_screen: true,
                ..open_gates()
            },
        ] {
            let mut fake = click_fake();
            let mut state = crate::workspace_select::SelectionState::default();
            let action =
                click_action(2, 3, &fake.row.spaces.clone()).expect("mapping precedes gates");
            let failure = crate::workspace_select::drain_claimed_select(
                &mut state,
                &mut fake,
                plan(action, "op-1"),
                &gates,
            )
            .expect_err("gated drain refuses");
            assert!(failure.reason.contains("gated"), "{}", failure.reason);
            assert!(fake.selects.is_empty());
        }
    }

    #[test]
    fn summary_marks_occupied_empty_and_unknown_conservatively() {
        let spaces = vec![11, 12, 13];
        // 11 has a real member; 12 has nothing; 13 has only a zero-area
        // candidate (ambiguous, never empty) plus an unknown-membership
        // window that could sit anywhere (poisons 12 too — honestly).
        let members = vec![
            member(1, 100, "com.example.app", Ok(vec![11])),
            MemberView {
                w: 0.0,
                h: 0.0,
                ..member(2, 101, "com.example.zero", Ok(vec![13]))
            },
            member(3, 102, "com.example.lost", Err(())),
        ];
        let state = summarize_spaces(&spaces, &members, None);
        assert_eq!(
            occupancy(&state),
            vec![
                SpaceOccupancy::Occupied,
                SpaceOccupancy::Unknown,
                SpaceOccupancy::Unknown,
            ]
        );
        // Without the ambiguous windows, 12 and 13 read honestly empty.
        let clean = summarize_spaces(
            &spaces,
            &[member(1, 100, "com.example.app", Ok(vec![11]))],
            None,
        );
        assert_eq!(
            occupancy(&clean),
            vec![
                SpaceOccupancy::Occupied,
                SpaceOccupancy::Empty,
                SpaceOccupancy::Empty,
            ]
        );
    }

    #[test]
    fn summary_counts_hidden_membership_and_shell_never() {
        // Off-current membership (a retained hidden layout) occupies exactly
        // like a current one; shell chrome (Dock bundle, sub-normal layer)
        // on the Space never does.
        let spaces = vec![11, 12];
        let members = vec![
            member(1, 100, "com.example.app", Ok(vec![12])),
            member(9, 50, "com.apple.dock", Ok(vec![11])),
            MemberView {
                layer: -20,
                ..member(8, 51, "com.example.wallpaper", Ok(vec![11]))
            },
        ];
        let state = summarize_spaces(&spaces, &members, Some(100));
        assert_eq!(
            occupancy(&state),
            vec![SpaceOccupancy::Empty, SpaceOccupancy::Occupied]
        );
        // Frontmost PID wins the representative; otherwise lowest (pid, wid).
        assert_eq!(
            state[1].1.as_deref(),
            Some("com.example.app"),
            "frontmost member names the icon"
        );
        let state = summarize_spaces(&spaces, &members, Some(999));
        assert_eq!(
            state[1].1.as_deref(),
            Some("com.example.app"),
            "lowest member names the icon without frontmost"
        );
        assert_eq!(state[0].1, None, "empty Space names nothing");
    }

    #[test]
    fn summary_representative_is_deterministic_without_focus() {
        let spaces = vec![11];
        let members = vec![
            member(7, 300, "com.example.zed", Ok(vec![11])),
            member(5, 200, "com.example.alpha", Ok(vec![11])),
            member(6, 200, "com.example.alpha-second", Ok(vec![11])),
        ];
        let state = summarize_spaces(&spaces, &members, None);
        // Lowest (pid, wid): pid 200 wins, then wid 5.
        assert_eq!(state[0].1.as_deref(), Some("com.example.alpha"));
        // Frontmost pid 300 overrides deterministically (its lowest wid).
        let state = summarize_spaces(&spaces, &members, Some(300));
        assert_eq!(state[0].1.as_deref(), Some("com.example.zed"));
        // Unknown membership never names a representative.
        let state = summarize_spaces(
            &spaces,
            &[member(9, 400, "com.example.lost", Err(()))],
            None,
        );
        assert_eq!(state[0], (SpaceOccupancy::Unknown, None));
        // A sticky multi-Space window homes to each of its Spaces (the
        // shared `candidate_occupied` rule: it is visibly there), so it
        // occupies and names the representative here.
        let state = summarize_spaces(
            &spaces,
            &[member(4, 401, "com.example.sticky", Ok(vec![11, 12]))],
            None,
        );
        assert_eq!(
            state[0],
            (
                SpaceOccupancy::Occupied,
                Some("com.example.sticky".to_owned())
            )
        );
    }

    fn retained(sid: u64, members: &[(u32, u32)]) -> RetainedDomainState {
        RetainedDomainState {
            sid,
            members: members.to_vec(),
        }
    }

    #[test]
    fn retained_summary_needs_completeness_and_names_coverage_gaps_unknown() {
        let spaces = vec![11, 12, 13];
        let domains = vec![retained(11, &[(100, 1)]), retained(12, &[])];
        // No report: every row Unknown, never Empty — not even the
        // memberless domain.
        let state = summarize_retained(&spaces, &domains, false, None);
        assert!(
            state
                .iter()
                .all(|(o, r)| *o == SpaceOccupancy::Unknown && r.is_none()),
            "{state:?}"
        );
        // Complete: occupied, empty, and the unnamed Space (13) Unknown —
        // a coverage gap is never an empty verdict.
        let state = summarize_retained(&spaces, &domains, true, None);
        assert_eq!(
            state,
            vec![
                (SpaceOccupancy::Occupied, Some((100, 1))),
                (SpaceOccupancy::Empty, None),
                (SpaceOccupancy::Unknown, None),
            ]
        );
    }

    #[test]
    fn retained_representative_prefers_frontmost_then_lowest_member() {
        let spaces = vec![11];
        let domains = vec![retained(11, &[(300, 7), (200, 5), (200, 6)])];
        // Lowest (pid, wid) without frontmost evidence.
        assert_eq!(
            summarize_retained(&spaces, &domains, true, None)[0],
            (SpaceOccupancy::Occupied, Some((200, 5)))
        );
        // Frontmost PID wins when it sits on the Space (its lowest wid).
        assert_eq!(
            summarize_retained(&spaces, &domains, true, Some(300))[0],
            (SpaceOccupancy::Occupied, Some((300, 7)))
        );
        // A frontmost PID with no member there never invents one.
        assert_eq!(
            summarize_retained(&spaces, &domains, true, Some(999))[0],
            (SpaceOccupancy::Occupied, Some((200, 5)))
        );
    }

    fn typed_click(position: usize, sid: u64, generation: u64, revision: u64) -> IndicatorClick {
        IndicatorClick {
            kind: IndicatorClickKind::Numbered,
            position,
            expected_sid: sid,
            ui_generation: generation,
            topology_revision: revision,
        }
    }

    fn trailing_binding(
        position: usize,
        sid: u64,
        generation: u64,
        revision: u64,
    ) -> IndicatorClick {
        IndicatorClick {
            kind: IndicatorClickKind::Trailing,
            position,
            expected_sid: sid,
            ui_generation: generation,
            topology_revision: revision,
        }
    }

    #[test]
    fn typed_click_resolves_only_against_its_own_generation_and_revision() {
        // Happy path: render generation 4 at revision 9, fresh row agrees.
        assert_eq!(
            resolve_click(typed_click(2, 3, 4, 9), 4, &[20, 3], 9),
            Ok(InputSemanticAction::WorkspaceSelect { index: 2 })
        );
        // A UI rebuild between render and drain (new current generation)
        // discards the queued click — the segment it names no longer
        // exists as rendered.
        let err = resolve_click(typed_click(2, 3, 4, 9), 5, &[20, 3], 9)
            .expect_err("rebuilt UI discards queued click");
        assert!(err.contains("generation"), "{err}");
        // A topology move between render and drain discards too, even when
        // the row itself looks identical.
        let err = resolve_click(typed_click(2, 3, 4, 9), 4, &[20, 3], 10)
            .expect_err("moved revision discards queued click");
        assert!(err.contains("revision"), "{err}");
        // A row that moved under the click discards instead of selecting
        // the new occupant (a deletion shifts every later ordinal).
        let err = resolve_click(typed_click(2, 3, 4, 9), 4, &[20, 41], 9)
            .expect_err("moved row discards");
        assert!(err.contains("moved"), "{err}");
        // Vanished positions, zero identity, and past-nine segments refuse.
        assert!(resolve_click(typed_click(2, 3, 4, 9), 4, &[20], 9).is_err());
        assert!(resolve_click(typed_click(1, 0, 4, 9), 4, &[20], 9).is_err());
        let err = resolve_click(typed_click(10, 110, 4, 9), 4, &[20, 3], 9)
            .expect_err("past-nine refuses");
        assert!(err.contains("1..=9"), "{err}");
        // A trailing-kind payload never resolves as a numbered select,
        // even when the row agrees.
        assert!(resolve_click(trailing_binding(3, 277, 4, 9), 4, &[1, 277], 9).is_err());
    }

    #[test]
    fn indicator_click_routing_retains_rendered_kind_without_fallback() {
        // Producer-shaped row `[1, 277]`: numbered click on position 2
        // renders with the position-2 SID; trailing slot renders at
        // position 3 with the native last SID.
        let numbered = typed_click(2, 277, 4, 9);
        let trailing = trailing_binding(3, 277, 4, 9);
        // Valid clicks route by kind.
        assert_eq!(
            resolve_indicator_click(numbered, 4, &[1, 277], 9, false),
            Ok(InputSemanticAction::WorkspaceSelect { index: 2 })
        );
        assert_eq!(
            resolve_indicator_click(trailing, 4, &[1, 277], 9, true),
            Ok(InputSemanticAction::WorkspaceSelect { index: 0 })
        );
        // Unarmed production refuses trailing before the engine lane.
        let err = resolve_indicator_click(trailing, 4, &[1, 277], 9, false)
            .expect_err("unarmed trailing refuses");
        assert!(err.contains("unarmed"), "{err}");
        // Stale numbered click when the row shrinks: refuses, never falls
        // back to trailing creation even when armed.
        let err = resolve_indicator_click(numbered, 4, &[1], 9, true)
            .expect_err("shrunk row discards numbered");
        assert!(err.contains("stale"), "{err}");
        // Stale numbered click when the row reorders: refuses, never
        // selects the new occupant and never creates trailing.
        let err = resolve_indicator_click(numbered, 4, &[277, 1], 9, true)
            .expect_err("reordered row discards numbered");
        assert!(err.contains("stale"), "{err}");
        // Stale trailing click on topology change: refuses even when the
        // row contents agree.
        let err = resolve_indicator_click(trailing, 4, &[1, 277], 10, true)
            .expect_err("moved revision discards trailing");
        assert!(err.contains("revision"), "{err}");
        // Stale trailing click when the row appends under it: refuses.
        let err = resolve_indicator_click(trailing, 4, &[1, 277, 21], 9, true)
            .expect_err("appended row discards trailing");
        assert!(err.contains("stale"), "{err}");
        // Discard reasons stay bounded for the log.
        assert!(bound_click_discard_reason(&err).len() <= 163);
    }

    #[test]
    fn diag_pick_copies_only_the_live_enabled_trailing_binding() {
        use super::IndicatorClickKind::{Numbered, Trailing};
        // Rendered row: two numbered segments plus a live trailing slot
        // at position 3 bound to the native last SID under generation 4
        // and revision 9. The pick copies that exact binding.
        let targets = vec![
            (1, 20, 4, 9, Numbered),
            (2, 277, 4, 9, Numbered),
            (3, 277, 4, 9, Trailing),
        ];
        assert_eq!(
            diag_rendered_trailing_click(&targets, &|position| position == 3),
            Some(trailing_binding(3, 277, 4, 9))
        );
        // A disabled slot (unknown occupancy `+?`) refuses: nothing
        // injects past the honest drain refusal.
        assert_eq!(diag_rendered_trailing_click(&targets, &|_| false), None);
        // No trailing slot rendered: nothing to copy (never a direct
        // select-0 substitute).
        let numbered_only = vec![(1, 20, 4, 9, Numbered), (2, 277, 4, 9, Numbered)];
        assert_eq!(
            diag_rendered_trailing_click(&numbered_only, &|_| true),
            None
        );
        assert_eq!(diag_rendered_trailing_click(&[], &|_| true), None);
        // Two trailing bindings (impossible in one render): ambiguous,
        // never guessed.
        let doubled = vec![(3, 277, 4, 9, Trailing), (4, 277, 4, 9, Trailing)];
        assert_eq!(diag_rendered_trailing_click(&doubled, &|_| true), None);
    }

    #[test]
    fn trailing_segment_title_never_reads_empty() {
        let slot = TrailingSlot { position: 3 };
        assert_eq!(trailing_segment_title(slot, SpaceOccupancy::Occupied), "+");
        assert_eq!(trailing_segment_title(slot, SpaceOccupancy::Unknown), "+?");
        assert_eq!(trailing_segment_title(slot, SpaceOccupancy::Empty), "+?");
    }

    #[test]
    fn trailing_slot_offers_only_when_last_not_proven_empty_never_duplicates() {
        // Producer-shaped `[1, 277]`: occupied last offers one trailing
        // slot at position 3; unknown occupancy also offers (the drain
        // refuses unknown instead of appending).
        assert_eq!(
            project_trailing_slot(
                &[1, 277],
                1,
                1,
                IndicatorMode::All,
                SpaceOccupancy::Occupied
            ),
            Some(TrailingSlot { position: 3 })
        );
        assert_eq!(
            project_trailing_slot(
                &[1, 277],
                277,
                1,
                IndicatorMode::All,
                SpaceOccupancy::Unknown
            ),
            Some(TrailingSlot { position: 3 })
        );
        // Proven-empty last reuses the existing slot: no duplicate slot.
        assert_eq!(
            project_trailing_slot(&[1, 277], 1, 1, IndicatorMode::All, SpaceOccupancy::Empty),
            None
        );
        assert_eq!(
            project_trailing_slot(&[1, 277], 277, 1, IndicatorMode::All, SpaceOccupancy::Empty),
            None,
            "current-empty reuses, never duplicates"
        );
        // Off mode, multi-display, unprovable rows, and the shared bound
        // offer nothing.
        assert_eq!(
            project_trailing_slot(
                &[1, 277],
                1,
                1,
                IndicatorMode::Off,
                SpaceOccupancy::Occupied
            ),
            None
        );
        assert_eq!(
            project_trailing_slot(
                &[1, 277],
                1,
                2,
                IndicatorMode::All,
                SpaceOccupancy::Occupied
            ),
            None
        );
        assert_eq!(
            project_trailing_slot(&[], 1, 1, IndicatorMode::All, SpaceOccupancy::Occupied),
            None
        );
        assert_eq!(
            project_trailing_slot(&[1, 0], 1, 1, IndicatorMode::All, SpaceOccupancy::Occupied),
            None
        );
        let full: Vec<u64> = (1..=32).collect();
        assert_eq!(
            project_trailing_slot(&full, 1, 1, IndicatorMode::All, SpaceOccupancy::Occupied),
            None,
            "shared bound holds"
        );
    }

    #[test]
    fn trailing_click_routes_to_index_zero_with_staleness_fences() {
        let click = TrailingClick {
            ui_generation: 4,
            topology_revision: 9,
            render_len: 2,
            render_last_sid: 277,
        };
        // Happy path: fresh row agrees, routes to trailing select-0.
        assert_eq!(
            resolve_trailing_click(click, 4, &[1, 277], 9),
            Ok(InputSemanticAction::WorkspaceSelect { index: 0 })
        );
        // Generation/revision drift discards.
        assert!(
            resolve_trailing_click(click, 5, &[1, 277], 9).is_err(),
            "rebuilt UI discards"
        );
        assert!(
            resolve_trailing_click(click, 4, &[1, 277], 10).is_err(),
            "moved revision discards"
        );
        // Length or last-SID move discards (never the wrong Space).
        assert!(
            resolve_trailing_click(click, 4, &[1], 9).is_err(),
            "shortened row discards"
        );
        assert!(
            resolve_trailing_click(click, 4, &[1, 277, 21], 9).is_err(),
            "appended row discards"
        );
        assert!(
            resolve_trailing_click(click, 4, &[1, 41], 9).is_err(),
            "moved last discards"
        );
        // Zero identity never dispatches.
        let bad = TrailingClick {
            render_last_sid: 0,
            ..click
        };
        assert!(resolve_trailing_click(bad, 4, &[1, 277], 9).is_err());
    }

    fn projected(spaces: &[u64], current: u64) -> IndicatorProjection {
        project_indicator(
            spaces,
            current,
            1,
            IndicatorMode::All,
            &|_| SpaceOccupancy::Empty,
            &|_| None,
        )
        .expect("provable row projects")
    }

    #[test]
    fn driver_dedups_change_only_and_counts_visible_applies() {
        let mut driver = IndicatorDriver::new(IndicatorMode::All);
        assert!(driver.armed());
        assert!(driver.needs_tick(9, 0), "empty cache always rebuilds");
        // First render applies as generation 1.
        assert_eq!(
            driver.apply(projected(&[20, 3], 20), None, 9, 0),
            DriverApply::Applied(1)
        );
        assert_eq!(driver.updates(), 1);
        // Identical projection at new observation counts: trigger fires
        // (observations moved) but the apply dedups — no native push.
        assert!(driver.needs_tick(9, 1), "observation publish retriggers");
        assert_eq!(
            driver.apply(projected(&[20, 3], 20), None, 9, 1),
            DriverApply::Duplicate
        );
        assert_eq!(driver.updates(), 1, "duplicate never pushes");
        assert_eq!(
            driver.generation(),
            1,
            "duplicate never advances generation"
        );
        // A changed render (native switch) applies with a new generation.
        assert!(driver.needs_tick(10, 1), "revision move retriggers");
        assert_eq!(
            driver.apply(projected(&[20, 3], 3), None, 10, 1),
            DriverApply::Applied(2)
        );
        assert_eq!(driver.updates(), 2);
        // Steady state stays silent: no trigger, no apply.
        assert!(!driver.needs_tick(10, 1));
    }

    #[test]
    fn driver_teardown_forces_refresh_and_orphans_queued_clicks() {
        let mut driver = IndicatorDriver::new(IndicatorMode::All);
        assert_eq!(
            driver.apply(projected(&[20, 3], 20), None, 9, 0),
            DriverApply::Applied(1)
        );
        // Lifecycle teardown (lock/stale/shutdown): the cache is gone so
        // the next good snapshot refreshes from scratch, but the
        // generation advances monotonically — clicks queued before the
        // teardown can never validate against the refresh.
        driver.teardown();
        assert!(driver.needs_tick(9, 0), "torn-down cache rebuilds");
        assert_eq!(
            driver.apply(projected(&[20, 3], 20), None, 9, 0),
            DriverApply::Applied(2)
        );
        let stale = typed_click(2, 3, 1, 9);
        let err = resolve_click(stale, driver.generation(), &[20, 3], 9)
            .expect_err("pre-teardown click refuses after refresh");
        assert!(err.contains("generation"), "{err}");
        // Accepted submissions count; refusals never do.
        driver.click_submitted();
        assert_eq!(driver.clicks(), 1);
    }

    #[test]
    fn driver_off_never_renders_or_queues() {
        let mut driver = IndicatorDriver::new(IndicatorMode::Off);
        assert!(!driver.armed());
        assert!(!driver.needs_tick(9, 0));
        assert_eq!(
            driver.apply(projected(&[20, 3], 20), None, 9, 0),
            DriverApply::Disarmed
        );
        assert_eq!(driver.updates(), 0);
        assert!(driver.last().is_none());
        assert_eq!(driver.last_trailing(), None);
    }

    #[test]
    fn driver_trailing_slot_rides_the_dedup_key() {
        // Same numbered rows throughout: only the extra slot state moves.
        let mut driver = IndicatorDriver::new(IndicatorMode::All);
        assert_eq!(
            driver.apply(projected(&[20, 3], 20), None, 9, 0),
            DriverApply::Applied(1)
        );
        // An identical render with no slot still dedups.
        assert_eq!(
            driver.apply(projected(&[20, 3], 20), None, 9, 1),
            DriverApply::Duplicate
        );
        // The slot appearing applies with a new generation (no stale
        // missing segment while the rows read identical).
        let slot = TrailingSlot { position: 3 };
        assert_eq!(
            driver.apply(
                projected(&[20, 3], 20),
                Some((slot, SpaceOccupancy::Occupied)),
                9,
                2
            ),
            DriverApply::Applied(2)
        );
        assert_eq!(
            driver.last_trailing(),
            Some((slot, SpaceOccupancy::Occupied))
        );
        // Same slot, same occupancy: duplicate again.
        assert_eq!(
            driver.apply(
                projected(&[20, 3], 20),
                Some((slot, SpaceOccupancy::Occupied)),
                9,
                3
            ),
            DriverApply::Duplicate
        );
        assert_eq!(driver.generation(), 2);
        // Same slot position flipping availability applies (a disabled
        // `+?` must not render as a live `+`).
        assert_eq!(
            driver.apply(
                projected(&[20, 3], 20),
                Some((slot, SpaceOccupancy::Unknown)),
                9,
                4
            ),
            DriverApply::Applied(3)
        );
        // The slot disappearing applies too.
        assert_eq!(
            driver.apply(projected(&[20, 3], 20), None, 9, 5),
            DriverApply::Applied(4)
        );
        assert_eq!(driver.last_trailing(), None);
    }

    #[test]
    fn segment_tooltip_round_trips_appearance_contract() {
        for (position, occupancy, current, enabled) in [
            (1, SpaceOccupancy::Occupied, false, true),
            (2, SpaceOccupancy::Empty, true, true),
            (3, SpaceOccupancy::Unknown, false, true),
            (10, SpaceOccupancy::Empty, false, false),
        ] {
            let tip = segment_tooltip(position, occupancy, current, enabled);
            assert!(tip.starts_with(&format!("Space {position}: ")), "{tip}");
            let (parsed_position, parsed_occupancy, parsed_current) =
                parse_segment_tooltip(&tip).expect("live tooltip parses");
            assert_eq!(parsed_position, position);
            assert_eq!(parsed_occupancy, occupancy);
            assert_eq!(parsed_current, current);
            if enabled {
                assert!(!tip.contains("no armed"), "{tip}");
            } else {
                assert!(tip.contains("no armed select route"), "{tip}");
            }
        }
        assert!(parse_segment_tooltip("Space 0: empty").is_none());
        assert!(parse_segment_tooltip("Workspace 2: empty").is_none());
        assert!(parse_segment_tooltip("Space 2: full").is_none());
    }

    #[test]
    fn segment_title_marks_unknown_only() {
        // '?' if and only if Unknown: numeric labels otherwise, including
        // past-nine positions and current segments (current is a separate
        // bezel/bold channel, never a title change).
        assert_eq!(segment_title(1, SpaceOccupancy::Occupied), "1");
        assert_eq!(segment_title(2, SpaceOccupancy::Empty), "2");
        assert_eq!(segment_title(2, SpaceOccupancy::Unknown), "2?");
        assert_eq!(segment_title(10, SpaceOccupancy::Empty), "10");
        assert_eq!(segment_title(10, SpaceOccupancy::Unknown), "10?");
        // The readback contract: a live title parses back to its position
        // with the '?' present exactly on Unknown.
        for (position, occupancy) in [
            (1, SpaceOccupancy::Occupied),
            (2, SpaceOccupancy::Empty),
            (3, SpaceOccupancy::Unknown),
        ] {
            let title = segment_title(position, occupancy);
            let (digits, marked) = title
                .strip_suffix('?')
                .map(|digits| (digits, true))
                .unwrap_or((title.as_str(), false));
            assert_eq!(digits.parse::<usize>().expect("numeric stem"), position);
            assert_eq!(marked, occupancy == SpaceOccupancy::Unknown);
        }
    }
}

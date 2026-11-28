#![forbid(unsafe_code)]

//! Menu-bar workspace indicator: pure projection, occupancy summary, click
//! mapping, and startup configuration.
//!
//! One native status item shows every existing native Space in native order
//! as directly clickable slots in one drawn menu-bar image (plain numbers,
//! current distinctly highlighted, occupancy treatments distinct even with
//! icons off); the on-demand dropdown menu mirrors the same rows for
//! accessibility and alternate clicks. Selection reuses the guarded engine
//! lane ([`crate::workspace_select`]): a segment click submits the same
//! armed [`crate::runtime_types::InputSemanticAction`] the tap submits, so
//! the unarmed, output identity, capability, drift, and verified-arrival
//! rules all apply unchanged. No synthetic trailing workspace is ever added, no
//! trailing lifecycle is managed, and no preference file is written.
//!
//! Appearance policy (rendered natively in [`crate::ffi`], read back from
//! the live render state by fixtures — never from a cached model):
//! - Order is native order, titles are 1-based ordinals with an explicit
//!   `'?'` suffix if and only if occupancy is Unknown (e.g. `"2?"`);
//!   numeric labels otherwise. Icons (when the flag resolves one) ride
//!   LEFT of the number, never replacing it.
//! - Current is orthogonal and unmissable: a drawn rounded border plus
//!   bold title.
//! - Occupancy is a second channel, distinct with icons off: Occupied reads
//!   full-alpha bold, Empty reads dimmed-alpha regular, Unknown reads
//!   full-alpha regular plus the `'?'` title suffix (the text hue stays the
//!   label colour — only its alpha follows the treatment — so the suffix is
//!   the honest Unknown marker, not a hue). Unknown is never dimmed
//!   like Empty and never bold like Occupied (unless also current, where
//!   the border plus bold are the current channel and `'?'` still names the
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

/// Explain a host-level native gesture refusal without changing action
/// admission. Mission Control still handles eligible existing-Space switches.
#[cfg(any(target_os = "macos", test))]
pub(crate) fn native_capability_notice(macos_major: Option<u32>) -> Option<String> {
    if crate::space_switch_candidate_27::candidate_27_allowed(macos_major).is_ok() {
        return None;
    }
    crate::space_switch_edges::gesture_payload_allowed(macos_major)
        .err()
        .map(|reason| {
            format!(
                "Native workspaces unavailable on this macOS: {reason}. Eligible existing Spaces can still be switched through Mission Control."
            )
        })
}

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

/// One projected menu row: plain 1-based ordinal, highlight, occupancy, and
/// whether a click may dispatch. Positions past the armed `1..=9` select
/// range are shown but never dispatched (no armed route — clicking one
/// must refuse, never select wrong). Core-order rows (see
/// [`project_core_indicator`]) additionally carry their immutable core
/// identity in `core`: the driver generation-scoped side-table the drain
/// resolves through — the FFI click stays a `Copy` native positional
/// token, never a heap id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndicatorRow {
    /// 1-based UNIQUE render slot across the whole item (the menu/segment
    /// tag and the click payload position). A grouped multi-display render
    /// assigns slots sequentially across groups, so two displays can both
    /// show ordinal 1 without colliding; a single-display render keeps
    /// slot == ordinal exactly as before.
    pub position: usize,
    /// 1-based ordinal the row SHOWS within its own output group (the
    /// native ordinal for native-order rows, the core ordinal for
    /// core-order rows). Title rendering and the armed `1..=9` range read
    /// this, never the global slot.
    pub ordinal: usize,
    /// Native anchor SID: the bound desktop for bound rows, the
    /// render-time native last for spare rows, `0` for unbound pending
    /// rows. Freshness fence only — never the actuation target.
    pub sid: u64,
    pub current: bool,
    pub occupancy: SpaceOccupancy,
    pub selectable: bool,
    /// Deterministic representative bundle ID for the icons flag (`None`
    /// without a member to name). Never logged.
    pub representative: Option<String>,
    /// Core identity for core-order rows (`None` for legacy native-order
    /// rows, which keep the positional select path).
    pub core: Option<CoreRowTarget>,
}

/// Immutable core identity for one core-order indicator row, captured at
/// render time: the core output plus the stable core logical workspace id
/// plus the explicit native binding (`None` exactly when the applied
/// reply leaves the logical unbound) plus whether the applied reply
/// explicitly designates the row as the trailing spare. Plain data;
/// the drain revalidates all of it against the coherent latest reply
/// (same UI generation, core generation, topology, and matching
/// id/binding/designation) before submitting the exact-id select.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreRowTarget {
    pub output: String,
    pub workspace: String,
    pub native_binding: Option<String>,
    pub spare: bool,
}

/// The cached view-model the main thread applies change-only to the native
/// status item. `visible == false` means no status item (off mode or a
/// refused multi-display/zero-display topology) with the explicit reason in
/// `hidden_reason`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndicatorProjection {
    pub visible: bool,
    pub hidden_reason: Option<String>,
    /// Status item title: the current Space's plain ordinal when exactly
    /// one group renders; empty for several groups (each row carries its
    /// own group ordinal instead, and no single ordinal names the item).
    pub title: String,
    pub rows: Vec<IndicatorRow>,
    /// The current workspace's retained planner mode from the coherent
    /// applied reply. `None` means unproven (legacy native-order
    /// projection, hidden projection, or no matching desire row) and is
    /// never guessed: the menu toggle disables itself instead of offering
    /// a mode the reply does not name.
    pub tiling: Option<tiler_core::world_report::TilingMode>,
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
            tiling: None,
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
            tiling: None,
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
            ordinal: position,
            sid: *sid,
            current: *sid == current,
            occupancy: occupancy(*sid),
            // The armed select range is `1..=9` (see
            // `is_armed_select`): further ordinals are shown, never
            // dispatched — clicking one refuses with an explicit reason
            // instead of selecting the wrong Space.
            selectable: position <= 9,
            representative: representative(*sid),
            core: None,
        });
    }
    Ok(IndicatorProjection {
        visible: true,
        hidden_reason: None,
        title: current_position.to_string(),
        rows,
        // Native-order rows carry no core reply: the retained planner
        // mode is unproven here, never inferred from the native order.
        tiling: None,
    })
}

// -- Core-order projection (DIRECT CORE ROW projection) ---------------------

/// Project one indicator frame in CORE order: every pager workspace row
/// of the single applied-reply screen in core order using core
/// ids/ordinals/current/occupied. Binding-map absence is NOT unbound
/// proof: only an explicit `trailing` designation on BOTH the pager row
/// and its matching desire row plus an explicit `None` native binding
/// names the spare. A non-tail `None` binding is never a spare and, with
/// no live native Space to prove it, never renders as a phantom number.
/// A bound trailing row is ordinary (selectable like any bound row). The
/// live native row is authoritative for which bound rows still exist: a
/// retained core binding alone does not prove its native Space is still
/// here, so a vanished or rehomed binding is dropped instead of rendering
/// an Unknown placeholder. The single-display and mode gates match
/// [`project_indicator`]; pager screens empty/missing or not exactly one
/// screen projects hidden with an explicit reason. Several displays are
/// never guessed here: the grouped path
/// ([`project_grouped_core_indicator`]) attributes each output's rows by
/// exact key. No
/// occupancy-driven `+` slot is ever invented here: the spare comes only
/// from the explicit core designation above.
///
/// Arguments: `screens` is the applied reply presentation pager content;
/// `desires` the same reply's workspace desires; `native` the fresh live
/// native row for this single output (its last SID anchors the spare and
/// its Space inventory proves or drops every bound row) — never an
/// actuation target; `occupancy`/`representative` resolve over bound
/// native SIDs (unbound rows read `Unknown` with no representative).
pub fn project_core_indicator(
    screens: &[tiler_core::world_presentation::PagerScreen],
    desires: &[tiler_core::world_report::WorkspaceDesire],
    display_count: usize,
    mode: IndicatorMode,
    occupancy: &dyn Fn(u64) -> SpaceOccupancy,
    representative: &dyn Fn(u64) -> Option<String>,
    native: &crate::workspace_select::SelectionSnapshot,
) -> Result<IndicatorProjection, String> {
    if mode == IndicatorMode::Off {
        return Ok(IndicatorProjection {
            visible: false,
            hidden_reason: Some("disabled by configuration".to_owned()),
            title: String::new(),
            rows: Vec::new(),
            tiling: None,
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
            tiling: None,
        });
    }
    if screens.len() != 1 {
        return Ok(IndicatorProjection {
            visible: false,
            hidden_reason: Some(format!(
                "expected exactly one core pager screen, found {}; refusing ambiguous attribution",
                screens.len()
            )),
            title: String::new(),
            rows: Vec::new(),
            tiling: None,
        });
    }
    let screen = &screens[0];
    if screen.workspaces.is_empty() {
        return Ok(IndicatorProjection {
            visible: false,
            hidden_reason: Some("core pager screen carries no workspaces".to_owned()),
            title: String::new(),
            rows: Vec::new(),
            tiling: None,
        });
    }
    let native_last = native.spaces.last().copied().unwrap_or(0);
    let (mut rows, current_ordinal, tiling) = project_core_screen_rows(
        screen,
        desires,
        mode,
        occupancy,
        representative,
        native_last,
    )?;
    // The live row decides which retained core identities still name a
    // native Space: drop vanished/rehomed bindings and orphan unbound
    // non-spares (the explicit spare alone survives unbound). Renumber the
    // render slots so the painted columns stay contiguous; the displayed
    // ordinals stay core truth.
    let mut current_live = false;
    rows.retain(|row| {
        let live = core_row_is_live(row, &|sid: u64| native.spaces.contains(&sid));
        if live && row.current {
            current_live = true;
        }
        live
    });
    if rows.is_empty() {
        return Ok(IndicatorProjection {
            visible: false,
            hidden_reason: Some(
                "no live native Space backs any retained core pager row; refusing phantom rows"
                    .to_owned(),
            ),
            title: String::new(),
            rows: Vec::new(),
            tiling: None,
        });
    }
    if !current_live {
        return Ok(IndicatorProjection {
            visible: false,
            hidden_reason: Some(
                "core current workspace is not backed by the live native Space row; refusing stale highlight"
                    .to_owned(),
            ),
            title: String::new(),
            rows: Vec::new(),
            tiling: None,
        });
    }
    for (index, row) in rows.iter_mut().enumerate() {
        row.position = index + 1;
    }
    Ok(IndicatorProjection {
        visible: true,
        hidden_reason: None,
        title: current_ordinal.to_string(),
        rows,
        tiling,
    })
}

/// Shared per-screen core row builder: one applied-reply pager screen's
/// workspaces in core order with core ids/ordinals/current/occupied. Rows
/// carry `position == ordinal` here; a grouped caller reassigns the global
/// render slots after concatenation. The single-screen
/// [`project_core_indicator`] and the multi-display
/// [`project_grouped_core_indicator`] share this builder, so occupancy,
/// spare designation, pending-row disabling, and the armed range can never
/// drift between the single- and multi-display paths.
///
/// Returns the rows, the current row's 1-based ordinal, and the retained
/// planner mode of the CURRENT workspace (`None` when the reply does not
/// name it — never guessed).
fn project_core_screen_rows(
    screen: &tiler_core::world_presentation::PagerScreen,
    desires: &[tiler_core::world_report::WorkspaceDesire],
    mode: IndicatorMode,
    occupancy: &dyn Fn(u64) -> SpaceOccupancy,
    representative: &dyn Fn(u64) -> Option<String>,
    native_last: u64,
) -> Result<
    (
        Vec<IndicatorRow>,
        usize,
        Option<tiler_core::world_report::TilingMode>,
    ),
    String,
> {
    let mut seen = std::collections::BTreeSet::new();
    for workspace in &screen.workspaces {
        if workspace.ordinal == 0 || !seen.insert(workspace.ordinal) {
            return Err(format!(
                "workspace row/core ordinal identity unprovable for output {}",
                screen.output
            ));
        }
    }
    let Some(current_workspace) = screen.workspaces.iter().find(|row| row.current) else {
        return Err(format!(
            "workspace row/current identity unprovable for output {}",
            screen.output
        ));
    };
    let current_ordinal = usize::try_from(current_workspace.ordinal)
        .map_err(|_| "workspace row/core ordinal identity unprovable".to_owned())?;
    let mut rows = Vec::with_capacity(screen.workspaces.len());
    for workspace in &screen.workspaces {
        let ordinal = usize::try_from(workspace.ordinal)
            .map_err(|_| "workspace row/core ordinal identity unprovable".to_owned())?;
        // Explicit spare rule: BOTH the pager row and its matching desire
        // row designate trailing AND the desire binding is explicitly
        // `None`. A binding-map absence alone (no desire row, or a
        // non-trailing desire) is never a spare.
        let desire = desires
            .iter()
            .find(|row| row.output.0 == screen.output && row.id == workspace.id);
        let spare = workspace.trailing
            && desire.is_some_and(|row| row.trailing && row.native_binding.is_none());
        let bound_sid = desire
            .and_then(|row| row.native_binding.as_deref())
            .and_then(crate::workspace_select::parse_native_binding);
        // A `Some` binding that does not parse is unproven: treat the row
        // as unbound pending (never guess a desktop from a malformed
        // binding).
        let bound = bound_sid.is_some();
        if mode == IndicatorMode::Current && !workspace.current && !spare {
            continue;
        }
        let (anchor, row_occupancy, row_representative) = if bound {
            let sid = bound_sid.unwrap_or(0);
            // Core occupancy is authoritative for core rows: the pager
            // `occupied` fact decides Occupied vs Empty. The host summary
            // only styles Unknown (incomplete/coverage gap) plus the
            // representative, never flips the core ring fact.
            let host = occupancy(sid);
            let core = if workspace.occupied {
                SpaceOccupancy::Occupied
            } else {
                SpaceOccupancy::Empty
            };
            let styled = match host {
                SpaceOccupancy::Unknown => SpaceOccupancy::Unknown,
                _ => core,
            };
            (sid, styled, representative(sid))
        } else if spare {
            (native_last, SpaceOccupancy::Unknown, None)
        } else {
            (0, SpaceOccupancy::Unknown, None)
        };
        rows.push(IndicatorRow {
            position: ordinal,
            ordinal,
            sid: anchor,
            current: workspace.current,
            occupancy: row_occupancy,
            // Unbound non-spare rows never arm (both projection callers
            // drop them as orphans with no live native Space to prove);
            // ordinals past the armed `1..=9` range stay disabled like
            // native rows. The spare selects exactly like a bound row.
            selectable: (bound || spare) && ordinal <= 9,
            representative: row_representative,
            core: Some(CoreRowTarget {
                output: screen.output.clone(),
                workspace: workspace.id.clone(),
                native_binding: desire.and_then(|row| row.native_binding.clone()),
                spare,
            }),
        });
    }
    Ok((
        rows,
        current_ordinal,
        // The menu toggle renders the retained planner mode of the
        // CURRENT workspace only: a reply without its desire row leaves
        // it unproven (never guessed).
        desires
            .iter()
            .find(|row| row.output.0 == screen.output && row.id == current_workspace.id)
            .map(|row| row.tiling_mode),
    ))
}

/// Whether one projected core row is still proven by the live native row.
/// The explicit unbound spare is the only unbound row that survives (it is
/// a designated core slot, not a native Space); every bound row must name
/// an exact SID the live row proves, and a non-spare row with no parseable
/// binding is an orphan that never renders. Legacy native-order rows
/// (`core == None`) are their own evidence and always survive.
fn core_row_is_live(row: &IndicatorRow, proves_sid: &dyn Fn(u64) -> bool) -> bool {
    let Some(core) = row.core.as_ref() else {
        return true;
    };
    core.spare
        || core
            .native_binding
            .as_deref()
            .and_then(crate::workspace_select::parse_native_binding)
            .is_some_and(proves_sid)
}

/// Whether `sid` appears exactly once across every mapped user row and that
/// occurrence belongs to `row_index`. A bound row whose SID is absent,
/// duplicated, or owned by another display is stale: it must never remain
/// armed after a rehome.
fn sid_uniquely_owned(
    user_rows: &[Option<crate::workspace_select::SelectionSnapshot>],
    row_index: usize,
    sid: u64,
) -> bool {
    let mut found = 0usize;
    for (index, row) in user_rows.iter().enumerate() {
        let Some(row) = row else {
            continue;
        };
        for space in &row.spaces {
            if *space != sid {
                continue;
            }
            if index != row_index {
                return false;
            }
            found += 1;
        }
    }
    found == 1
}

/// Project one INDICATOR frame for several outputs: one shared status item
/// showing every display's pager Spaces in per-output groups, ordered
/// left-to-right by the native display arrangement (global frame x; the
/// primary display wins an equal-x tie; further ties stay deterministic —
/// see [`crate::topology::order_display_indices`]). Each group highlights
/// ITS OWN output's current Space, so several rows can read current at
/// once. Clicks stay exact-id selects on the clicked row's output; there
/// is no pointer or focus inference and no cross-output move here.
///
/// Safety boundaries:
/// - A group renders only when its output-to-Space scope is proven: the
///   pager screen's output key names exactly one native display, that
///   display's UUID names exactly one SLS row, and that row's user-Space
///   read is provable. Any unprovable scope hides THAT group (safe-hide),
///   never a guessed display or row; when no group is provable the whole
///   item hides with an explicit reason.
/// - Core contract failures (duplicate screen outputs, duplicate/missing
///   ordinals, missing current) fail closed for the whole render.
/// - The live native row proves every rendered row: a bound row whose SID
///   vanished or is owned by another output, and an unbound non-spare with
///   no native Space, are dropped instead of painted as unknown numbers.
///   The explicit spare is the sole unbound survivor; a stale current
///   retains only its still-present rows at Unknown.
/// - `screens` is the applied reply's pager content, `desires` the same
///   reply's workspace desires, `rows` the live SLS rows in read order,
///   `user_rows` the same rows resolved through
///   [`crate::workspace_select::user_row_from_rows`] (`None` = unprovable
///   scope). `occupancy`/`representative` resolve over bound native SIDs
///   across every output (unbound rows read `Unknown`).
///
/// The tiling toggle stays single-group only: with several groups, or
/// several native displays attached even when just one group renders
/// provably, there is no unambiguous "current workspace" to toggle, so
/// `tiling` is `None` and the toggle renders disabled instead of inventing
/// a target. Native spare creation likewise stays single-display; grouped
/// multi-display spare rows render disabled.
#[allow(clippy::too_many_arguments)]
pub fn project_grouped_core_indicator(
    screens: &[tiler_core::world_presentation::PagerScreen],
    desires: &[tiler_core::world_report::WorkspaceDesire],
    ns: &[crate::ffi::NsDisplay],
    rows: &[crate::ffi::SlsDisplayRow],
    user_rows: &[Option<crate::workspace_select::SelectionSnapshot>],
    mode: IndicatorMode,
    occupancy: &dyn Fn(u64) -> SpaceOccupancy,
    representative: &dyn Fn(u64) -> Option<String>,
) -> Result<IndicatorProjection, String> {
    let hidden = |reason: String| IndicatorProjection {
        visible: false,
        hidden_reason: Some(reason),
        title: String::new(),
        rows: Vec::new(),
        tiling: None,
    };
    if mode == IndicatorMode::Off {
        return Ok(hidden("disabled by configuration".to_owned()));
    }
    if screens.is_empty() {
        return Ok(hidden(
            "no coherent core pager reply; refusing native inference".to_owned(),
        ));
    }
    let mut outputs = std::collections::BTreeSet::new();
    for screen in screens {
        if !outputs.insert(screen.output.as_str()) {
            return Err(format!(
                "duplicate core pager screen for output {}; refusing ambiguous attribution",
                screen.output
            ));
        }
    }
    let order = match crate::topology::order_display_indices(ns) {
        Ok(order) => order,
        Err(reason) => return Ok(hidden(reason)),
    };
    let mut projected: Vec<IndicatorRow> = Vec::new();
    let mut groups = 0usize;
    let mut only_group: Option<(usize, Option<tiler_core::world_report::TilingMode>)> = None;
    for index in order {
        let display = &ns[index];
        if display.cg_id == 0 {
            continue;
        }
        let Some(uuid) = display.uuid.as_deref() else {
            // Unreadable display identity: this group's output-to-Space
            // scope is unproven, so it safe-hides alone.
            continue;
        };
        let output = crate::topology::output_key(display.cg_id);
        let Some(screen) = screens.iter().find(|screen| screen.output == output) else {
            // No core pager content for this display (for example a
            // suspended output): no group is invented.
            continue;
        };
        if screen.workspaces.is_empty() {
            continue;
        }
        let Ok(row_index) = crate::topology::unique_row_for_display(rows, uuid) else {
            continue;
        };
        let Some(user) = user_rows.get(row_index).and_then(Option::as_ref) else {
            continue;
        };
        let native_last = user.spaces.last().copied().unwrap_or(0);
        let (mut group_rows, current_ordinal, mut tiling) = project_core_screen_rows(
            screen,
            desires,
            mode,
            occupancy,
            representative,
            native_last,
        )?;
        if group_rows.is_empty() {
            continue;
        }
        // The applied reply is authoritative for the group's identity, but
        // the live native row is authoritative for what is actually here,
        // current and owned. Drop every retained row the live row cannot
        // prove (vanished or rehomed bindings, orphan unbound non-spares):
        // a core identity alone never keeps a phantom number. A stale/held
        // core current must retain the group's still-present rows (at
        // Unknown) while suppressing the stale highlight and every action
        // in that group only.
        let proves = |sid: u64| sid_uniquely_owned(user_rows, row_index, sid);
        let core_current = group_rows
            .iter()
            .find(|row| row.current)
            .and_then(|row| row.core.as_ref())
            .and_then(|core| core.native_binding.as_deref())
            .and_then(crate::workspace_select::parse_native_binding);
        let current_stale = !core_current.is_some_and(|sid| sid == user.current && proves(sid));
        if current_stale {
            group_rows.retain(|row| core_row_is_live(row, &proves));
            if group_rows.is_empty() {
                // Nothing native-backed survives here: no group, no
                // phantom numbers.
                continue;
            }
            for row in &mut group_rows {
                row.current = false;
                row.occupancy = SpaceOccupancy::Unknown;
                row.selectable = false;
                row.sid = 0;
            }
            tiling = None;
        } else {
            group_rows.retain(|row| core_row_is_live(row, &proves));
            if group_rows.is_empty() {
                continue;
            }
        }
        groups += 1;
        if !current_stale {
            only_group = Some((current_ordinal, tiling));
        }
        let first_slot = projected.len() + 1;
        for (offset, mut row) in group_rows.into_iter().enumerate() {
            row.position = first_slot + offset;
            projected.push(row);
        }
    }
    if projected.is_empty() {
        return Ok(hidden(
            "no provable core pager group: no live native Space backs a retained core row; refusing phantom rows"
                .to_owned(),
        ));
    }
    // Several native displays attached: native spare creation stays a
    // single-display capability (the owner also refuses it), so render the
    // spare disabled instead of offering an action that cannot dispatch.
    // The single-display MX6/creation path is unchanged.
    let multi_attached = ns.len() > 1;
    if multi_attached {
        for row in &mut projected {
            if row.core.as_ref().is_some_and(|core| core.spare) {
                row.selectable = false;
            }
        }
    }
    let (title, tiling) = match (groups, only_group) {
        (1, Some((current_ordinal, tiling))) if !multi_attached => {
            (current_ordinal.to_string(), tiling)
        }
        // Several outputs are current at once, or several displays are
        // attached: no single workspace unambiguously owns the toggle.
        _ => (String::new(), None),
    };
    Ok(IndicatorProjection {
        visible: true,
        hidden_reason: None,
        title,
        rows: projected,
        tiling,
    })
}

/// Resolve one queued CORE-row segment click binding: the UI generation
/// and topology revision the render was built from. Render-time position
/// and anchor SID are deliberately NOT consulted here (core order may
/// skew against native order — the click targets the exact core id
/// through the driver's generation-scoped side-table, verified against
/// the coherent latest reply by [`verify_core_row_click`]). Any
/// generation or revision drift refuses instead of selecting the wrong
/// workspace.
pub fn resolve_core_click(
    click: IndicatorClick,
    current_generation: u64,
    fresh_revision: u64,
) -> Result<(), String> {
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
    Ok(())
}

/// Verify one queued CORE-row click against the rendered row plus the
/// coherent latest reply, returning the exact-id select action. Fences,
/// in order:
/// - render identity: the click payload must name the rendered row's own
///   position and anchor SID (a forged payload naming another row, or a
///   payload replayed against a different render, refuses);
/// - armed route: positions past the `1..=9` select range refuse, and a
///   rendered row that is not `selectable` refuses (disabled past-nine
///   rows and pending unbound rows never arm);
/// - UI generation plus topology revision (rebuilt UI or moved topology
///   refuses, via [`resolve_core_click`]);
/// - pending unbound non-spare rows refuse as pending backing;
/// - the latest coherent snapshot must carry the same core generation as
///   the render (a core-side move the native revision cannot see
///   refuses);
/// - its desires must still name the exact `(output, workspace)` id with
///   the same native binding and the same effective spare predicate
///   (`trailing && binding is None`, mirroring the projection rule — a
///   bound trailing row stays ordinary, so raw marker flips under a
///   stable binding do not refuse; an arrived/moved binding, a vanished
///   desire, or a flipped effective spare refuses);
/// - its presentation pager must still carry the workspace with the same
///   effective spare predicate (a vanished row or a lost unbound marker
///   refuses instead of selecting the id against a moved ring; a bound
///   row's raw marker flip under the same stable binding stays ordinary
///   and does not refuse).
///
/// The returned action selects exactly that core id through the existing
/// `SelectWorkspace` route — the native anchor stays a freshness fence
/// only, never the actuation target.
#[allow(clippy::too_many_arguments)]
pub fn verify_core_row_click(
    click: IndicatorClick,
    rendered: &IndicatorRow,
    current_generation: u64,
    fresh_revision: u64,
    render_core_generation: &str,
    presentation: &tiler_core::world_report::WorldPresentationV4,
    desires: &[tiler_core::world_report::WorkspaceDesire],
    snapshot_generation: &str,
) -> Result<InputSemanticAction, String> {
    let Some(core) = rendered.core.as_ref() else {
        return Err(
            "stale workspace click discarded: rendered row carries no core identity; no select dispatched"
                .to_owned(),
        );
    };
    if click.position != rendered.position {
        return Err(format!(
            "stale workspace click discarded: click position {} does not match rendered row {}; no select dispatched",
            click.position, rendered.position
        ));
    }
    if click.expected_sid != rendered.sid {
        return Err(format!(
            "stale workspace click discarded: click identity moved under the click (row {} changed); no select dispatched",
            rendered.position
        ));
    }
    if rendered.ordinal == 0 || rendered.ordinal > 9 {
        return Err(format!(
            "workspace segment {} has no armed select route (armed range is 1..=9); refusing instead of selecting the wrong Space",
            rendered.ordinal
        ));
    }
    if !rendered.selectable {
        return Err(format!(
            "stale workspace click discarded: rendered row {} is not selectable; no select dispatched",
            rendered.position
        ));
    }
    resolve_core_click(click, current_generation, fresh_revision)?;
    if !core.spare && core.native_binding.is_none() {
        return Err(format!(
            "stale workspace click discarded: row {} still pending native backing; no select dispatched",
            rendered.position
        ));
    }
    if snapshot_generation != render_core_generation {
        return Err(
            "stale workspace click discarded: core generation moved after render; no select dispatched"
                .to_owned(),
        );
    }
    let Some(desire) = desires
        .iter()
        .find(|row| row.output.0 == core.output && row.id == core.workspace)
    else {
        return Err(
            "stale workspace click discarded: rendered workspace no longer desired; no select dispatched"
                .to_owned(),
        );
    };
    if desire.native_binding != core.native_binding {
        return Err(format!(
            "stale workspace click discarded: native binding arrived or moved under the click (row {} changed); no select dispatched",
            rendered.position
        ));
    }
    // Effective spare mirrors the projection rule (marker && unbound):
    // a bound trailing row projects ordinary (spare=false) with both raw
    // markers true, so comparing raw markers to `core.spare` would always
    // refuse it. Bound marker role changes under a stable binding keep the
    // effective predicate false and do not refuse: the exact-id select
    // does not depend on the trailing role once bound. Unbound spare
    // transitions (marker loss or binding arrival) flip the predicate and
    // still refuse.
    let desire_spare = desire.trailing && desire.native_binding.is_none();
    if desire_spare != core.spare {
        return Err(format!(
            "stale workspace click discarded: trailing designation moved under the click (row {} changed); no select dispatched",
            rendered.position
        ));
    }
    let pager_holds = presentation
        .pager
        .iter()
        .find(|screen| screen.output == core.output)
        .and_then(|screen| {
            screen
                .workspaces
                .iter()
                .find(|row| row.id == core.workspace)
        })
        .is_some_and(|row| (row.trailing && desire.native_binding.is_none()) == core.spare);
    if !pager_holds {
        return Err(format!(
            "stale workspace click discarded: pager row vanished or marker moved under the click (row {} changed); no select dispatched",
            rendered.position
        ));
    }
    Ok(InputSemanticAction::IndicatorSelectWorkspace {
        output: core.output.clone(),
        workspace: core.workspace.clone(),
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

/// Render-time binding of the current-workspace tiling toggle menu item:
/// the UI generation the item was rendered under. Plain data forwarded by
/// value over the bounded channel, so an AppKit click can never be
/// reinterpreted against a newer render (only generation drift refuses;
/// there is no position, SID, or ordinal to reinterpret).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndicatorToggleClick {
    pub ui_generation: u64,
}

/// Sentinel tag of the tiling toggle menu item. Workspace rows carry
/// 1-based positions, so tag 0 can never name a workspace row: the
/// workspace-row readback filter ([`is_workspace_row_tag`]) therefore
/// never reports the prepended toggle item as a row.
pub const TOGGLE_TILING_MENU_TAG: isize = 0;

/// Whether one live menu item tag names a workspace row (1-based position)
/// rather than the prepended toggle item. The toggle is a control, not a
/// workspace row, so row readbacks must skip it.
#[must_use]
pub fn is_workspace_row_tag(tag: isize) -> bool {
    tag > 0
}

/// Resolve one queued tiling-toggle click to its neutral intent. Equal
/// generations forward [`InputSemanticAction::ToggleWorkspaceTiling`];
/// any drift refuses instead of toggling under a render the user did not
/// see. No ordinal is consulted: core resolves the mode at execution, so
/// a stale ordinal can never select the wrong workspace's mode.
pub fn toggle_click_action(
    ui_generation: u64,
    current_generation: u64,
) -> Result<InputSemanticAction, String> {
    if ui_generation != current_generation {
        return Err(format!(
            "stale tiling toggle discarded: status view rebuilt after render (g{ui_generation} != current {current_generation}); no toggle dispatched"
        ));
    }
    Ok(InputSemanticAction::ToggleWorkspaceTiling)
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

/// Miniature band height in AppKit points: when any Space paints pager
/// miniatures, the fixed 22pt indicator image reserves this band along its
/// bottom edge and the numbers share the remaining height; with no
/// miniatures anywhere the numbers use the full item height. Single source
/// of truth for the native draw and the pager miniature band layout below.
pub const MINIATURE_BAND_H: f64 = 8.0;

/// Segment column width in AppKit points: compact for bare ordinals, wider
/// with a representative icon or a wide (`10+`-position or `?`-suffixed)
/// title. Single source of truth: the native rebuild and the pager band
/// layout share this, so the drawable rects always match the drawn column.
#[must_use]
pub const fn segment_column_width(has_icon: bool, wide: bool) -> f64 {
    let base = if has_icon { 44.0 } else { 26.0 };
    if wide { base + 8.0 } else { base }
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

// -- Shared status-image geometry --------------------------------------------

/// One drawn status-image column in item-local AppKit points. `position`
/// is the 1-based clickable slot the painted pixel and the hit test both
/// name (`None` for a native group divider, which advances `start` without
/// ever becoming a target).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SlotColumn {
    pub position: Option<usize>,
    pub start: f64,
    pub width: f64,
}

impl SlotColumn {
    /// Right edge in item-local points.
    #[must_use]
    pub fn end(self) -> f64 {
        self.start + self.width
    }
}

/// Resolve ordered `(slot position, width)` columns into item-local spans
/// in one pass. The status image renderer and the click hit test consume
/// this SAME vector, so a painted number and its target can never drift.
/// Non-finite or negative widths collapse to zero instead of corrupting
/// every later offset.
#[must_use]
pub fn slot_columns(columns: &[(Option<usize>, f64)]) -> Vec<SlotColumn> {
    let mut start = 0.0;
    let mut out = Vec::with_capacity(columns.len());
    for (position, width) in columns {
        let width = if width.is_finite() {
            width.max(0.0)
        } else {
            0.0
        };
        out.push(SlotColumn {
            position: *position,
            start,
            width,
        });
        start += width;
    }
    out
}

/// Total drawn width of a resolved column list (item length).
#[must_use]
pub fn slot_columns_width(columns: &[SlotColumn]) -> f64 {
    columns.last().map_or(0.0, |column| column.end())
}

/// Hit-test the rendered status image at item-local `x`, returning the
/// matching column INDEX. Only clickable columns participate: group
/// dividers and everything outside the image are "no target" (never a
/// neighbouring row). The upper bound is inclusive and the first match
/// wins, matching the verified WhichSpace v1.5.0
/// `StatusBarLayout.slot(at:)` rule (`x >= start && x <= start + width`),
/// so an exact shared edge resolves to the left slot. Non-finite input
/// refuses.
#[must_use]
pub fn slot_index_at_x(columns: &[SlotColumn], x: f64) -> Option<usize> {
    if !x.is_finite() {
        return None;
    }
    columns
        .iter()
        .position(|column| column.position.is_some() && x >= column.start && x <= column.end())
}

/// Hit-test the rendered status image at item-local `x` (the matched
/// column), or `None` for a divider gap, empty input, or out-of-image x.
#[must_use]
pub fn slot_at_x(columns: &[SlotColumn], x: f64) -> Option<SlotColumn> {
    slot_index_at_x(columns, x).map(|index| columns[index])
}

/// One rendered slot's accessibility facts for the status button label.
/// `group` is the 1-based display-group ordinal in arrangement order (the
/// same group the image separates), never a private output identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccessibleSlot {
    pub group: usize,
    pub ordinal: usize,
    pub occupancy: SpaceOccupancy,
    pub current: bool,
    pub enabled: bool,
}

/// Truthful status-button accessibility description: every rendered
/// workspace with its own group, ordinal, occupancy verdict, current
/// marker and whether a select route is armed. The image itself paints no
/// text for accessibility clients, so this label is the only honest
/// description; empty slots produce an empty string (the caller clears the
/// label instead of inventing a workspace).
#[must_use]
pub fn indicator_accessibility_description(slots: &[AccessibleSlot]) -> String {
    if slots.is_empty() {
        return String::new();
    }
    let groups = slots.iter().map(|slot| slot.group).max().unwrap_or(1);
    let mut out = String::from("OmniTiler workspaces");
    if groups > 1 {
        out.push_str(&format!(", {groups} display groups"));
    }
    out.push('.');
    let mut index = 0;
    while index < slots.len() {
        let group = slots[index].group;
        if groups > 1 {
            out.push_str(&format!(" Group {group}:"));
        }
        let mut first = true;
        while index < slots.len() && slots[index].group == group {
            let slot = slots[index];
            out.push_str(if first { " workspace " } else { "; workspace " });
            first = false;
            out.push_str(&slot.ordinal.to_string());
            if slot.current {
                out.push_str(", current");
            }
            out.push_str(match slot.occupancy {
                SpaceOccupancy::Occupied => ", occupied",
                SpaceOccupancy::Empty => ", empty",
                SpaceOccupancy::Unknown => ", unknown",
            });
            if !slot.enabled {
                out.push_str(", unavailable");
            }
            index += 1;
        }
        out.push('.');
    }
    out
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
/// presence plus its occupancy) PLUS the pager miniature rows (per-SID
/// drawable miniatures with their resolved colours): a pager-only change
/// (focus move, highlight/accent change) with identical rows still applies
/// — otherwise the miniatures would go stale while the rows dedup. Pager-
/// only applies keep the SAME generation (no new clickable render), so
/// queued clicks stay valid across a focus move: only a changed projection
/// or trailing slot advances the generation and orphans queued bindings.
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
    last_pager: Vec<(usize, Vec<crate::pager::MiniatureDraw>)>,
    last_accent: Option<crate::active_border::SystemAccent>,
    last_revision: Option<u64>,
    last_observations: u64,
    /// Indicator revision of the applied reply the cached render was built
    /// from: the change trigger that repaints a toggle reply's checkmark
    /// before any topology/observation tick moves.
    last_indicator_revision: Option<u64>,
    /// Core generation of the applied reply the cached render was built
    /// from: the drain refuses any click whose render-time core
    /// generation no longer matches the coherent latest reply (same rule
    /// as the UI generation, but for core-side moves the native revision
    /// cannot see).
    last_core_generation: String,
    updates: u64,
    clicks: u64,
    toggles: u64,
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
            last_pager: Vec::new(),
            last_accent: None,
            last_revision: None,
            last_observations: 0,
            last_indicator_revision: None,
            last_core_generation: String::new(),
            updates: 0,
            clicks: 0,
            toggles: 0,
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

    /// Cached pager miniature rows the native surface draws
    /// (`(row position, drawable miniatures)` per rendered row, empty
    /// without pager content). Keyed by the unique render slot (position),
    /// never by inferred native slot, so skewed core/native orders and
    /// grouped multi-display renders paint exactly. Pager-only
    /// changes repaint without advancing the generation, so queued clicks
    /// stay valid across a focus move.
    #[must_use]
    pub fn last_pager(&self) -> &[(usize, Vec<crate::pager::MiniatureDraw>)] {
        &self.last_pager
    }

    /// Record the core generation the current tick's coherent snapshot
    /// came from (call on every computed tick, applied or duplicate, so
    /// the drain's fence names the render's reply even across
    /// pager-only repaints that keep the clickable generation).
    pub fn note_core_generation(&mut self, core_generation: &str) {
        if self.armed {
            self.last_core_generation = core_generation.to_owned();
        }
    }

    /// Core generation the cached render was built from (empty before the
    /// first core-backed tick; the drain refuses core-row clicks then).
    #[must_use]
    pub fn last_core_generation(&self) -> &str {
        &self.last_core_generation
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

    /// Accepted tiling-toggle submissions through the guarded lane.
    #[must_use]
    pub const fn toggles(&self) -> u64 {
        self.toggles
    }

    /// Whether a fresh snapshot revision / engine observation count pair
    /// plus the live system accent requires a projection rebuild: only a
    /// moved revision, a published observation, a changed accent, an
    /// advanced indicator revision, or an empty cache rebuilds — never a
    /// blind per-cadence poll. The accent rides the trigger (settings
    /// cannot change at runtime, but the live accent can) so a
    /// resolved-colour change repaints even on a steady snapshot. Pager
    /// content itself (focus moves, reply updates) arrives with
    /// observations and dedups inside [`apply`]. Disarmed drivers never
    /// rebuild. This legacy shape pins indicator revision `0` so existing
    /// callers keep their edge trigger unchanged.
    #[must_use]
    pub fn needs_tick(
        &self,
        revision: u64,
        observations: u64,
        accent: Option<crate::active_border::SystemAccent>,
    ) -> bool {
        self.needs_tick_with_revision(revision, observations, accent, 0)
    }

    /// [`needs_tick`] plus the applied-reply indicator revision: a moved
    /// revision repaints even on a steady topology/observation pair, so a
    /// toggle reply refreshes the checkmark on its own reply instead of
    /// waiting for the next native tick.
    #[must_use]
    pub fn needs_tick_with_revision(
        &self,
        revision: u64,
        observations: u64,
        accent: Option<crate::active_border::SystemAccent>,
        indicator_revision: u64,
    ) -> bool {
        if !self.armed {
            return false;
        }
        self.last.is_none()
            || self.last_revision.is_some_and(|r| r != revision)
            || self.last_observations != observations
            || self.last_accent != accent
            || self
                .last_indicator_revision
                .is_some_and(|r| r != indicator_revision)
    }

    /// Record that a rebuild was computed at `revision`/`observations`
    /// (call on every computed tick, applied or duplicate, so the trigger
    /// stays edge-driven). Returns the apply verdict: changed projections
    /// or trailing slots advance the generation, identical ones with changed
    /// pager rows apply under the SAME generation (the clickable render is
    /// unchanged, so queued clicks stay valid across a focus or colour
    /// move), fully identical ones stay untouched, hidden projections store
    /// without counting a visible update, and disarmed drivers store
    /// nothing. The trailing slot rides the dedup key with the projection
    /// (see the struct docs): a slot-only change applies with a new
    /// generation. This legacy shape pins indicator revision `0`.
    pub fn apply(
        &mut self,
        projection: IndicatorProjection,
        trailing: Option<(TrailingSlot, SpaceOccupancy)>,
        pager: Vec<(usize, Vec<crate::pager::MiniatureDraw>)>,
        revision: u64,
        observations: u64,
        accent: Option<crate::active_border::SystemAccent>,
    ) -> DriverApply {
        self.apply_with_revision(
            projection,
            trailing,
            pager,
            revision,
            observations,
            accent,
            0,
        )
    }

    /// [`apply`] plus the applied-reply indicator revision: recorded
    /// before the dedup comparison so a desires/pager change that leaves
    /// the visible projection identical does not retrigger every tick
    /// (the trigger compares the last APPLIED revision, not the last
    /// computing one).
    #[allow(clippy::too_many_arguments)]
    pub fn apply_with_revision(
        &mut self,
        projection: IndicatorProjection,
        trailing: Option<(TrailingSlot, SpaceOccupancy)>,
        pager: Vec<(usize, Vec<crate::pager::MiniatureDraw>)>,
        revision: u64,
        observations: u64,
        accent: Option<crate::active_border::SystemAccent>,
        indicator_revision: u64,
    ) -> DriverApply {
        if !self.armed {
            return DriverApply::Disarmed;
        }
        self.last_revision = Some(revision);
        self.last_observations = observations;
        self.last_accent = accent;
        self.last_indicator_revision = Some(indicator_revision);
        if self.last.as_ref() == Some(&projection) && self.last_trailing == trailing {
            if self.last_pager == pager {
                return DriverApply::Duplicate;
            }
            self.last_pager = pager;
            if projection.visible {
                self.updates += 1;
            }
            return DriverApply::Applied(self.generation);
        }
        self.generation += 1;
        let visible = projection.visible;
        self.last = Some(projection);
        self.last_trailing = trailing;
        self.last_pager = pager;
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
        self.last_pager = Vec::new();
        self.last_accent = None;
        self.last_indicator_revision = None;
        self.last_core_generation = String::new();
    }

    /// Record one accepted click submission (guarded-lane `Accepted` only;
    /// refusals and discards never count).
    pub fn click_submitted(&mut self) {
        self.clicks += 1;
    }

    /// Record one accepted tiling-toggle submission (guarded-lane
    /// `Accepted` only; refusals and discards never count).
    pub fn toggle_submitted(&mut self) {
        self.toggles += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_capability_notice_preserves_gate_reason() {
        assert!(native_capability_notice(Some(26)).is_none());
        assert!(native_capability_notice(Some(27)).is_none());
        for major in [Some(28), None] {
            let reason = crate::space_switch_edges::gesture_payload_allowed(major).unwrap_err();
            let notice = native_capability_notice(major).unwrap();
            assert!(notice.starts_with("Native workspaces unavailable on this macOS"));
            assert!(notice.contains(&reason));
            assert!(notice.contains(
                "Eligible existing Spaces can still be switched through Mission Control."
            ));
        }
    }

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
        fn create(
            &mut self,
            _before: &crate::workspace_select::SelectionSnapshot,
        ) -> Result<(), crate::workspace_select::DispatchError> {
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
                detached: None,
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
            claim.select_baseline = Some(crate::runtime_types::SelectPhysical::Desktop(20));
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
    fn core_projection_carries_current_workspace_tiling_mode() {
        use tiler_core::world_report::TilingMode;
        let screens = vec![core_screen(
            "out-a",
            vec![
                core_workspace("ws-a", 1, true, true, false),
                core_workspace("ws-b", 2, false, false, false),
            ],
        )];
        let mut desires = vec![
            core_desire("ws-a", "out-a", Some("space-20"), false, 0),
            core_desire("ws-b", "out-a", Some("space-10"), false, 1),
        ];
        let project = |desires: &[tiler_core::world_report::WorkspaceDesire]| {
            project_core_indicator(
                &screens,
                desires,
                1,
                IndicatorMode::All,
                &|_| SpaceOccupancy::Empty,
                &|_| None,
                &native_row(&[20, 10], 20),
            )
            .expect("provable core rows project")
        };
        // Default retained mode is tiled.
        assert_eq!(project(&desires).tiling, Some(TilingMode::Tiled));
        // Only the CURRENT workspace's mode matters: floating the
        // non-current row leaves the toggle tiled.
        desires[1].tiling_mode = TilingMode::Floating;
        assert_eq!(project(&desires).tiling, Some(TilingMode::Tiled));
        // Floating the current row carries through.
        desires[0].tiling_mode = TilingMode::Floating;
        assert_eq!(project(&desires).tiling, Some(TilingMode::Floating));
    }

    #[test]
    fn unproven_tiling_stays_none_never_guessed() {
        use tiler_core::world_report::TilingMode;
        let screens = vec![core_screen(
            "out-a",
            vec![
                core_workspace("ws-a", 1, true, true, false),
                core_workspace("ws-b", 2, false, false, false),
            ],
        )];
        // No desire row for the CURRENT workspace: it has no live native
        // backing to prove, so the projection safe-hides instead of
        // painting an unprovable current.
        let no_current_desire = vec![core_desire("ws-b", "out-a", Some("space-10"), false, 1)];
        let projection = project_core_indicator(
            &screens,
            &no_current_desire,
            1,
            IndicatorMode::All,
            &|_| SpaceOccupancy::Empty,
            &|_| None,
            &native_row(&[20, 10], 20),
        )
        .expect("unprovable current hides");
        assert!(!projection.visible);
        assert_eq!(projection.tiling, None);
        // A hidden projection carries nothing.
        let desires = vec![
            core_desire("ws-a", "out-a", Some("space-20"), false, 0),
            core_desire("ws-b", "out-a", Some("space-10"), false, 1),
        ];
        let off = project_core_indicator(
            &screens,
            &desires,
            1,
            IndicatorMode::Off,
            &|_| SpaceOccupancy::Empty,
            &|_| None,
            &native_row(&[20, 10], 20),
        )
        .expect("off mode projects hidden");
        assert_eq!(off.tiling, None);
        // The legacy native-order projection never names a core mode.
        assert_eq!(projected(&[20, 3], 20).tiling, None);
        // Sanity: the proven path still reports the mode.
        let proven = project_core_indicator(
            &screens,
            &desires,
            1,
            IndicatorMode::All,
            &|_| SpaceOccupancy::Empty,
            &|_| None,
            &native_row(&[20, 10], 20),
        )
        .expect("provable core rows project");
        assert_eq!(proven.tiling, Some(TilingMode::Tiled));
    }

    #[test]
    fn projection_tiling_change_advances_driver_generation() {
        use tiler_core::world_report::TilingMode;
        let mut driver = IndicatorDriver::new(IndicatorMode::All);
        let base = projected(&[20, 3], 20);
        assert_eq!(base.tiling, None);
        assert_eq!(
            driver.apply(base.clone(), None, Vec::new(), 9, 0, None),
            DriverApply::Applied(1)
        );
        // The projection equality is the dedup key: a proven mode change
        // applies with a new generation (the checkmark is part of the
        // clickable render).
        let mut tiled = base.clone();
        tiled.tiling = Some(TilingMode::Tiled);
        assert_eq!(
            driver.apply(tiled.clone(), None, Vec::new(), 9, 0, None),
            DriverApply::Applied(2)
        );
        let mut floating = tiled;
        floating.tiling = Some(TilingMode::Floating);
        assert_eq!(
            driver.apply(floating, None, Vec::new(), 9, 0, None),
            DriverApply::Applied(3)
        );
    }

    #[test]
    fn driver_needs_tick_tracks_indicator_revision() {
        let mut driver = IndicatorDriver::new(IndicatorMode::All);
        assert!(
            driver.needs_tick_with_revision(9, 0, None, 4),
            "empty cache always rebuilds"
        );
        assert_eq!(
            driver.apply_with_revision(projected(&[20, 3], 20), None, Vec::new(), 9, 0, None, 4),
            DriverApply::Applied(1)
        );
        assert!(
            !driver.needs_tick_with_revision(9, 0, None, 4),
            "a steady applied revision does not retrigger"
        );
        assert!(
            driver.needs_tick_with_revision(9, 0, None, 5),
            "a moved indicator revision repaints on the reply alone"
        );
        // The legacy wrapper pins revision 0 and stays stable after its
        // own apply.
        let mut legacy = IndicatorDriver::new(IndicatorMode::All);
        assert_eq!(
            legacy.apply(projected(&[20, 3], 20), None, Vec::new(), 9, 0, None),
            DriverApply::Applied(1)
        );
        assert!(!legacy.needs_tick(9, 0, None));
        // Teardown forgets the revision too; the empty-cache clause
        // already forces the rebuild.
        driver.teardown();
        assert!(driver.needs_tick_with_revision(9, 0, None, 4));
    }

    #[test]
    fn driver_counts_only_accepted_toggles() {
        let mut driver = IndicatorDriver::new(IndicatorMode::All);
        assert_eq!(driver.toggles(), 0);
        driver.toggle_submitted();
        assert_eq!(driver.toggles(), 1);
    }

    #[test]
    fn toggle_click_forwards_neutral_intent_or_discards_stale() {
        assert_eq!(
            toggle_click_action(7, 7),
            Ok(InputSemanticAction::ToggleWorkspaceTiling)
        );
        let reason = toggle_click_action(7, 8).expect_err("generation drift discards");
        assert!(reason.contains("stale tiling toggle discarded"));
        assert!(reason.contains("g7"));
        assert!(reason.contains("current 8"));
        // Sentinel tag: workspace rows are 1-based, so 0 is never a row.
        assert_eq!(TOGGLE_TILING_MENU_TAG, 0);
        assert!(!is_workspace_row_tag(TOGGLE_TILING_MENU_TAG));
        assert!(is_workspace_row_tag(1));
        assert!(is_workspace_row_tag(9));
    }

    #[test]
    fn driver_dedups_change_only_and_counts_visible_applies() {
        let mut driver = IndicatorDriver::new(IndicatorMode::All);
        assert!(driver.armed());
        assert!(driver.needs_tick(9, 0, None), "empty cache always rebuilds");
        // First render applies as generation 1.
        assert_eq!(
            driver.apply(projected(&[20, 3], 20), None, Vec::new(), 9, 0, None),
            DriverApply::Applied(1)
        );
        assert_eq!(driver.updates(), 1);
        // Identical projection at new observation counts: trigger fires
        // (observations moved) but the apply dedups — no native push.
        assert!(
            driver.needs_tick(9, 1, None),
            "observation publish retriggers"
        );
        assert_eq!(
            driver.apply(projected(&[20, 3], 20), None, Vec::new(), 9, 1, None),
            DriverApply::Duplicate
        );
        assert_eq!(driver.updates(), 1, "duplicate never pushes");
        assert_eq!(
            driver.generation(),
            1,
            "duplicate never advances generation"
        );
        // A changed render (native switch) applies with a new generation.
        assert!(driver.needs_tick(10, 1, None), "revision move retriggers");
        assert_eq!(
            driver.apply(projected(&[20, 3], 3), None, Vec::new(), 10, 1, None),
            DriverApply::Applied(2)
        );
        assert_eq!(driver.updates(), 2);
        // Steady state stays silent: no trigger, no apply.
        assert!(!driver.needs_tick(10, 1, None));
    }

    #[test]
    fn driver_teardown_forces_refresh_and_orphans_queued_clicks() {
        let mut driver = IndicatorDriver::new(IndicatorMode::All);
        assert_eq!(
            driver.apply(projected(&[20, 3], 20), None, Vec::new(), 9, 0, None),
            DriverApply::Applied(1)
        );
        // Lifecycle teardown (lock/stale/shutdown): the cache is gone so
        // the next good snapshot refreshes from scratch, but the
        // generation advances monotonically — clicks queued before the
        // teardown can never validate against the refresh.
        driver.teardown();
        assert!(driver.needs_tick(9, 0, None), "torn-down cache rebuilds");
        assert_eq!(
            driver.apply(projected(&[20, 3], 20), None, Vec::new(), 9, 0, None),
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
        assert!(!driver.needs_tick(9, 0, None));
        assert_eq!(
            driver.apply(projected(&[20, 3], 20), None, Vec::new(), 9, 0, None),
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
            driver.apply(projected(&[20, 3], 20), None, Vec::new(), 9, 0, None),
            DriverApply::Applied(1)
        );
        // An identical render with no slot still dedups.
        assert_eq!(
            driver.apply(projected(&[20, 3], 20), None, Vec::new(), 9, 1, None),
            DriverApply::Duplicate
        );
        // The slot appearing applies with a new generation (no stale
        // missing segment while the rows read identical).
        let slot = TrailingSlot { position: 3 };
        assert_eq!(
            driver.apply(
                projected(&[20, 3], 20),
                Some((slot, SpaceOccupancy::Occupied)),
                Vec::new(),
                9,
                2,
                None
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
                Vec::new(),
                9,
                3,
                None
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
                Vec::new(),
                9,
                4,
                None
            ),
            DriverApply::Applied(3)
        );
        // The slot disappearing applies too.
        assert_eq!(
            driver.apply(projected(&[20, 3], 20), None, Vec::new(), 9, 5, None),
            DriverApply::Applied(4)
        );
        assert_eq!(driver.last_trailing(), None);
    }

    #[test]
    fn driver_pager_only_change_applies_without_new_generation() {
        use crate::active_border::SystemAccent;
        use crate::pager::MiniatureDraw;
        // Identical rows throughout: only the pager miniature rows move
        // (focus move or highlight/accent change with steady Spaces).
        let mut driver = IndicatorDriver::new(IndicatorMode::All);
        assert_eq!(
            driver.apply(projected(&[20, 3], 20), None, Vec::new(), 9, 0, None),
            DriverApply::Applied(1)
        );
        // A pager-only change retriggers and applies under the SAME
        // generation: the clickable render is unchanged, so queued clicks
        // stay valid across the focus move.
        let mini = MiniatureDraw {
            x: 1.0,
            y: 1.0,
            w: 4.0,
            h: 4.0,
            focused: true,
            border: (0xbd, 0x93, 0xf9),
            fill: (0x49, 0x3a, 0x62),
        };
        let pager = vec![(1usize, vec![mini])];
        assert!(driver.needs_tick(9, 1, None), "observation retriggers");
        assert_eq!(
            driver.apply(projected(&[20, 3], 20), None, pager.clone(), 9, 1, None),
            DriverApply::Applied(1),
            "pager-only apply keeps the generation"
        );
        assert_eq!(driver.updates(), 2, "pager-only change still pushes");
        assert_eq!(driver.generation(), 1);
        assert_eq!(driver.last_pager(), pager.as_slice());
        let accent = Some(SystemAccent {
            color: (0x11, 0x22, 0x33),
            alpha: 0xC4,
        });
        // A live accent change with steady revision/observations also
        // retriggers (resolved colours move even when nothing else does);
        // the tick recomputes the pager rows under the new accent, so the
        // apply still carries changed rows under the same generation.
        assert!(driver.needs_tick(9, 1, accent), "accent change retriggers");
        let mini_accent = MiniatureDraw {
            border: (0x11, 0x22, 0x33),
            fill: (0x07, 0x0d, 0x14),
            ..mini
        };
        let pager_accent = vec![(1usize, vec![mini_accent])];
        assert_eq!(
            driver.apply(
                projected(&[20, 3], 20),
                None,
                pager_accent.clone(),
                9,
                1,
                accent
            ),
            DriverApply::Applied(1),
            "accent-only apply keeps the generation too"
        );
        assert_eq!(driver.updates(), 3, "accent change still pushes");
        // Steady state stays silent: no trigger, duplicate apply.
        assert!(!driver.needs_tick(9, 1, accent));
        assert_eq!(
            driver.apply(projected(&[20, 3], 20), None, pager_accent, 9, 1, accent),
            DriverApply::Duplicate
        );
        assert_eq!(driver.updates(), 3, "duplicate never pushes");
        // A queued click from the first render still validates: the
        // generation never moved under the pager-only repaint.
        let click = typed_click(1, 20, 1, 9);
        assert!(
            resolve_click(click, driver.generation(), &[20, 3], 9).is_ok(),
            "queued click survives pager-only repaint"
        );
    }

    #[test]
    fn segment_tooltip_names_current_and_unavailable_slots() {
        assert_eq!(
            segment_tooltip(1, SpaceOccupancy::Occupied, false, true),
            "Space 1: occupied"
        );
        assert_eq!(
            segment_tooltip(2, SpaceOccupancy::Empty, true, true),
            "Space 2: empty, current"
        );
        assert_eq!(
            segment_tooltip(3, SpaceOccupancy::Unknown, false, true),
            "Space 3: unknown"
        );
        assert_eq!(
            segment_tooltip(10, SpaceOccupancy::Empty, false, false),
            "Space 10: empty, no armed select route"
        );
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

    #[test]
    fn slot_columns_advance_dividers_without_creating_targets() {
        // Two slots, a 7pt group divider, then one wide slot: the divider
        // consumes pixels but appears in `slot_columns` with no position.
        let columns = slot_columns(&[
            (Some(1), 26.0),
            (Some(2), 26.0),
            (None, 7.0),
            (Some(3), 44.0),
        ]);
        assert_eq!(
            columns,
            vec![
                SlotColumn {
                    position: Some(1),
                    start: 0.0,
                    width: 26.0
                },
                SlotColumn {
                    position: Some(2),
                    start: 26.0,
                    width: 26.0
                },
                SlotColumn {
                    position: None,
                    start: 52.0,
                    width: 7.0
                },
                SlotColumn {
                    position: Some(3),
                    start: 59.0,
                    width: 44.0
                },
            ]
        );
        assert_eq!(slot_columns_width(&columns), 103.0);
        // Degenerate widths never corrupt later offsets.
        let repaired = slot_columns(&[(Some(1), -3.0), (Some(2), f64::NAN), (Some(3), 5.0)]);
        assert_eq!(repaired[2].start, 0.0);
        assert_eq!(slot_columns_width(&repaired), 5.0);
    }

    #[test]
    fn slot_at_x_maps_edges_and_refuses_gaps() {
        let columns = slot_columns(&[
            (Some(1), 26.0),
            (Some(2), 26.0),
            (None, 7.0),
            (Some(3), 44.0),
        ]);
        let position = |x: f64| slot_at_x(&columns, x).and_then(|slot| slot.position);
        // Exact slot starts and interiors hit their own slot.
        assert_eq!(position(0.0), Some(1));
        assert_eq!(position(12.5), Some(1));
        assert_eq!(position(26.0), Some(1), "shared edge resolves left first");
        assert_eq!(position(26.1), Some(2));
        assert_eq!(
            position(52.0),
            Some(2),
            "slot keeps its inclusive right edge"
        );
        // The divider is a real gap: no target anywhere inside it.
        assert_eq!(position(52.1), None);
        assert_eq!(position(58.9), None);
        // The next group starts immediately after the divider.
        assert_eq!(position(59.0), Some(3));
        assert_eq!(position(103.0), Some(3), "last slot keeps its right edge");
        // Outside the image and non-finite input refuse instead of guessing.
        assert_eq!(position(-0.1), None);
        assert_eq!(position(103.1), None);
        assert_eq!(position(f64::NAN), None);
        assert_eq!(position(f64::INFINITY), None);
        assert_eq!(slot_at_x(&[], 0.0), None);
        // The index form is what the AppKit action uses, so pin it too.
        assert_eq!(slot_index_at_x(&columns, 0.0), Some(0));
        assert_eq!(slot_index_at_x(&columns, 52.0), Some(1));
        assert_eq!(slot_index_at_x(&columns, 52.1), None);
        assert_eq!(slot_index_at_x(&columns, 59.0), Some(3));
        assert_eq!(slot_index_at_x(&columns, f64::NAN), None);
    }

    fn core_workspace(
        id: &str,
        ordinal: u32,
        current: bool,
        occupied: bool,
        trailing: bool,
    ) -> tiler_core::world_presentation::PagerWorkspace {
        tiler_core::world_presentation::PagerWorkspace {
            id: id.to_owned(),
            ordinal,
            label: tiler_core::world_presentation::pager_workspace_label(ordinal),
            current,
            occupied,
            contains_focused: current,
            trailing,
            windows: Vec::new(),
        }
    }

    fn core_desire(
        id: &str,
        output: &str,
        native_binding: Option<&str>,
        trailing: bool,
        order: usize,
    ) -> tiler_core::world_report::WorkspaceDesire {
        let out = tiler_core::directional::OutputId(output.to_owned());
        tiler_core::world_report::WorkspaceDesire {
            id: id.to_owned(),
            output: out.clone(),
            native_binding: native_binding.map(str::to_owned),
            scope: tiler_core::world_report::workspace_authority_scope(&out, id),
            order,
            tiling_mode: tiler_core::world_report::TilingMode::Tiled,
            existence: true,
            hold: None,
            trailing,
        }
    }

    fn core_screen(
        output: &str,
        rows: Vec<tiler_core::world_presentation::PagerWorkspace>,
    ) -> tiler_core::world_presentation::PagerScreen {
        tiler_core::world_presentation::PagerScreen {
            output: output.to_owned(),
            workspaces: rows,
        }
    }

    /// One fresh live native row for the single-output projection tests.
    fn native_row(spaces: &[u64], current: u64) -> crate::workspace_select::SelectionSnapshot {
        crate::workspace_select::SelectionSnapshot {
            detached: None,
            display: "display-a".to_owned(),
            spaces: spaces.to_vec(),
            current,
        }
    }

    #[test]
    fn core_rows_follow_core_order_ids_and_ordinals_not_native_positions() {
        // Skew: native SIDs [20, 10] but core ordinals [1, 2] on ids
        // ws-a/ws-b. Rows must carry CORE ordinals/ids in core order.
        let screens = vec![core_screen(
            "out-a",
            vec![
                core_workspace("ws-a", 1, true, true, false),
                core_workspace("ws-b", 2, false, false, false),
            ],
        )];
        let desires = vec![
            core_desire("ws-a", "out-a", Some("space-20"), false, 0),
            core_desire("ws-b", "out-a", Some("space-10"), false, 1),
        ];
        let projection = project_core_indicator(
            &screens,
            &desires,
            1,
            IndicatorMode::All,
            &|_| SpaceOccupancy::Empty,
            &|_| None,
            &native_row(&[20, 10], 20),
        )
        .expect("provable core row projects");
        assert!(projection.visible);
        assert_eq!(projection.title, "1");
        assert_eq!(projection.rows.len(), 2);
        assert_eq!(projection.rows[0].position, 1);
        assert_eq!(projection.rows[0].sid, 20);
        assert!(projection.rows[0].current);
        assert_eq!(projection.rows[0].occupancy, SpaceOccupancy::Occupied);
        assert_eq!(
            projection.rows[0].core.as_ref().expect("core").workspace,
            "ws-a"
        );
        assert_eq!(projection.rows[1].position, 2);
        assert_eq!(projection.rows[1].sid, 10);
        assert!(!projection.rows[1].current);
        assert_eq!(projection.rows[1].occupancy, SpaceOccupancy::Empty);
        assert_eq!(
            projection.rows[1].core.as_ref().expect("core").workspace,
            "ws-b"
        );
        // Click identity targets the exact core id, never a native slot:
        // generation/revision fences pass here, mismatch refuses.
        let click = IndicatorClick {
            kind: IndicatorClickKind::Numbered,
            position: 2,
            expected_sid: 10,
            ui_generation: 7,
            topology_revision: 9,
        };
        assert!(resolve_core_click(click, 7, 9).is_ok());
        assert!(resolve_core_click(click, 8, 9).is_err());
        assert!(resolve_core_click(click, 7, 10).is_err());
    }

    fn core_click_harness() -> (
        Vec<tiler_core::world_presentation::PagerScreen>,
        Vec<tiler_core::world_report::WorkspaceDesire>,
        Vec<IndicatorRow>,
    ) {
        // Two coherent core rows: bound ordinary ws-a plus trailing spare
        // ws-spare, projected through the production projector.
        let screens = vec![core_screen(
            "out-a",
            vec![
                core_workspace("ws-a", 1, true, true, false),
                core_workspace("ws-spare", 2, false, false, true),
            ],
        )];
        let desires = vec![
            core_desire("ws-a", "out-a", Some("space-20"), false, 0),
            core_desire("ws-spare", "out-a", None, true, 1),
        ];
        let projection = project_core_indicator(
            &screens,
            &desires,
            1,
            IndicatorMode::All,
            &|_| SpaceOccupancy::Occupied,
            &|_| None,
            &native_row(&[20], 20),
        )
        .expect("provable core rows project");
        assert_eq!(projection.rows.len(), 2);
        assert!(projection.rows[0].selectable);
        assert!(projection.rows[1].selectable);
        assert!(projection.rows[1].core.as_ref().expect("core").spare);
        (screens, desires, projection.rows)
    }

    fn core_click_for(row: &IndicatorRow, generation: u64, revision: u64) -> IndicatorClick {
        IndicatorClick {
            kind: IndicatorClickKind::Numbered,
            position: row.position,
            expected_sid: row.sid,
            ui_generation: generation,
            topology_revision: revision,
        }
    }

    fn core_presentation_for(
        screens: &[tiler_core::world_presentation::PagerScreen],
    ) -> tiler_core::world_report::WorldPresentationV4 {
        let mut base = crate::runtime::empty_world_presentation_v4();
        base.pager = screens.to_vec();
        base
    }

    #[test]
    fn core_click_verifier_selects_exact_id_on_coherent_snapshot() {
        use crate::runtime_types::InputSemanticAction;
        let (screens, desires, rows) = core_click_harness();
        let presentation = core_presentation_for(&screens);
        // Bound ordinary row selects its exact core id.
        let action = verify_core_row_click(
            core_click_for(&rows[0], 7, 9),
            &rows[0],
            7,
            9,
            "gen-9",
            &presentation,
            &desires,
            "gen-9",
        )
        .expect("coherent bound click selects");
        assert_eq!(
            action,
            InputSemanticAction::IndicatorSelectWorkspace {
                output: "out-a".to_owned(),
                workspace: "ws-a".to_owned(),
            }
        );
        // Trailing spare row selects its exact spare id, never a
        // positional reinterpretation and never a trailing-create route.
        let spare_action = verify_core_row_click(
            core_click_for(&rows[1], 7, 9),
            &rows[1],
            7,
            9,
            "gen-9",
            &presentation,
            &desires,
            "gen-9",
        )
        .expect("coherent spare click selects");
        assert_eq!(
            spare_action,
            InputSemanticAction::IndicatorSelectWorkspace {
                output: "out-a".to_owned(),
                workspace: "ws-spare".to_owned(),
            }
        );
    }

    #[test]
    fn core_click_verifier_refuses_disabled_rows_and_forged_payloads() {
        let (screens, desires, rows) = core_click_harness();
        let presentation = core_presentation_for(&screens);
        // Forged payload: position names another rendered row.
        let mut wrong_position = core_click_for(&rows[1], 7, 9);
        wrong_position.position = rows[0].position;
        assert!(
            verify_core_row_click(
                wrong_position,
                &rows[1],
                7,
                9,
                "gen-9",
                &presentation,
                &desires,
                "gen-9",
            )
            .is_err()
        );
        // Forged payload: anchor SID does not match the rendered row.
        let mut wrong_sid = core_click_for(&rows[1], 7, 9);
        wrong_sid.expected_sid = rows[1].sid + 1;
        assert!(
            verify_core_row_click(
                wrong_sid,
                &rows[1],
                7,
                9,
                "gen-9",
                &presentation,
                &desires,
                "gen-9",
            )
            .is_err()
        );
        // Disabled past-nine row refuses even with a coherent snapshot: the
        // projector marks ordinal 10 unselectable and the verifier enforces
        // both the range and the rendered flag.
        let ids: Vec<String> = (1u32..=10)
            .map(|ordinal| format!("ws-wide-{ordinal}"))
            .collect();
        let bindings: Vec<String> = (0..10)
            .map(|index| format!("space-{}", 30 + index))
            .collect();
        let wide_rows: Vec<tiler_core::world_presentation::PagerWorkspace> = ids
            .iter()
            .enumerate()
            .map(|(index, id)| {
                let ordinal = (index + 1) as u32;
                core_workspace(id, ordinal, ordinal == 1, false, false)
            })
            .collect();
        let wide_desires: Vec<tiler_core::world_report::WorkspaceDesire> = ids
            .iter()
            .enumerate()
            .map(|(index, id)| core_desire(id, "out-a", Some(&bindings[index]), false, index))
            .collect();
        let wide_screens = vec![core_screen("out-a", wide_rows)];
        let wide_projection = project_core_indicator(
            &wide_screens,
            &wide_desires,
            1,
            IndicatorMode::All,
            &|_| SpaceOccupancy::Occupied,
            &|_| None,
            &native_row(&(30u64..40).collect::<Vec<_>>(), 30),
        )
        .expect("ten core rows project");
        assert_eq!(wide_projection.rows.len(), 10);
        let past_nine = &wide_projection.rows[9];
        assert_eq!(past_nine.position, 10);
        assert!(!past_nine.selectable, "ordinal 10 never arms");
        let wide_presentation = core_presentation_for(&wide_screens);
        assert!(
            verify_core_row_click(
                core_click_for(past_nine, 7, 9),
                past_nine,
                7,
                9,
                "gen-9",
                &wide_presentation,
                &wide_desires,
                "gen-9",
            )
            .is_err()
        );
        // Pending unbound non-spare rows have no live native Space to
        // prove them: the projection drops them instead of painting a
        // phantom number, and the verifier still refuses any unbound row
        // that manages to reach it.
        let pending_screens = vec![core_screen(
            "out-a",
            vec![
                core_workspace("ws-1", 1, true, true, false),
                core_workspace("ws-pending", 2, false, false, false),
            ],
        )];
        let pending_desires = vec![
            core_desire("ws-1", "out-a", Some("space-11"), false, 0),
            core_desire("ws-pending", "out-a", None, false, 1),
        ];
        let pending_projection = project_core_indicator(
            &pending_screens,
            &pending_desires,
            1,
            IndicatorMode::All,
            &|_| SpaceOccupancy::Occupied,
            &|_| None,
            &native_row(&[11], 11),
        )
        .expect("live core row projects");
        assert_eq!(pending_projection.rows.len(), 1);
        assert!(
            pending_projection
                .rows
                .iter()
                .all(|row| { row.core.as_ref().expect("core row").workspace != "ws-pending" })
        );
        let pending = IndicatorRow {
            position: 2,
            ordinal: 2,
            sid: 0,
            current: false,
            occupancy: SpaceOccupancy::Unknown,
            selectable: true,
            representative: None,
            core: Some(CoreRowTarget {
                output: "out-a".to_owned(),
                workspace: "ws-pending".to_owned(),
                native_binding: None,
                spare: false,
            }),
        };
        let pending_presentation = core_presentation_for(&pending_screens);
        assert!(
            verify_core_row_click(
                core_click_for(&pending, 7, 9),
                &pending,
                7,
                9,
                "gen-9",
                &pending_presentation,
                &pending_desires,
                "gen-9",
            )
            .is_err()
        );
    }

    #[test]
    fn core_click_verifier_fences_generation_binding_and_pager() {
        let (screens, desires, rows) = core_click_harness();
        let presentation = core_presentation_for(&screens);
        let check = |click: IndicatorClick,
                     rendered: &IndicatorRow,
                     current_generation: u64,
                     fresh_revision: u64,
                     render_core_generation: &str,
                     presentation: &tiler_core::world_report::WorldPresentationV4,
                     desires: &[tiler_core::world_report::WorkspaceDesire],
                     snapshot_generation: &str| {
            verify_core_row_click(
                click,
                rendered,
                current_generation,
                fresh_revision,
                render_core_generation,
                presentation,
                desires,
                snapshot_generation,
            )
        };
        // UI generation drift refuses.
        assert!(
            check(
                core_click_for(&rows[1], 7, 9),
                &rows[1],
                8,
                9,
                "gen-9",
                &presentation,
                &desires,
                "gen-9",
            )
            .is_err()
        );
        // Topology revision drift refuses.
        assert!(
            check(
                core_click_for(&rows[1], 7, 9),
                &rows[1],
                7,
                10,
                "gen-9",
                &presentation,
                &desires,
                "gen-9",
            )
            .is_err()
        );
        // Core generation moved after render refuses.
        assert!(
            check(
                core_click_for(&rows[1], 7, 9),
                &rows[1],
                7,
                9,
                "gen-9",
                &presentation,
                &desires,
                "gen-10",
            )
            .is_err()
        );
        // Rendered workspace no longer desired refuses.
        let vanished_desires: Vec<tiler_core::world_report::WorkspaceDesire> = desires
            .iter()
            .filter(|row| row.id != "ws-spare")
            .cloned()
            .collect();
        assert!(
            check(
                core_click_for(&rows[1], 7, 9),
                &rows[1],
                7,
                9,
                "gen-9",
                &presentation,
                &vanished_desires,
                "gen-9",
            )
            .is_err()
        );
        // Native binding arrived under the click (None -> Some) refuses.
        let mut arrived = desires.clone();
        arrived[1].native_binding = Some("space-99".to_owned());
        assert!(
            check(
                core_click_for(&rows[1], 7, 9),
                &rows[1],
                7,
                9,
                "gen-9",
                &presentation,
                &arrived,
                "gen-9",
            )
            .is_err()
        );
        // Native binding moved under the click refuses.
        let mut moved = desires.clone();
        moved[0].native_binding = Some("space-21".to_owned());
        assert!(
            check(
                core_click_for(&rows[0], 7, 9),
                &rows[0],
                7,
                9,
                "gen-9",
                &presentation,
                &moved,
                "gen-9",
            )
            .is_err()
        );
        // Trailing designation flipped under the click refuses.
        let mut flipped = desires.clone();
        flipped[1].trailing = false;
        assert!(
            check(
                core_click_for(&rows[1], 7, 9),
                &rows[1],
                7,
                9,
                "gen-9",
                &presentation,
                &flipped,
                "gen-9",
            )
            .is_err()
        );
        // Pager row vanished refuses.
        let shrunk_screens = vec![core_screen(
            "out-a",
            vec![core_workspace("ws-a", 1, true, true, false)],
        )];
        let shrunk_presentation = core_presentation_for(&shrunk_screens);
        assert!(
            check(
                core_click_for(&rows[1], 7, 9),
                &rows[1],
                7,
                9,
                "gen-9",
                &shrunk_presentation,
                &desires,
                "gen-9",
            )
            .is_err()
        );
        // Pager trailing marker lost refuses.
        let mut unmarked = screens.clone();
        for workspace in &mut unmarked[0].workspaces {
            workspace.trailing = false;
        }
        let unmarked_presentation = core_presentation_for(&unmarked);
        assert!(
            check(
                core_click_for(&rows[1], 7, 9),
                &rows[1],
                7,
                9,
                "gen-9",
                &unmarked_presentation,
                &desires,
                "gen-9",
            )
            .is_err()
        );
    }

    #[test]
    fn core_spare_renders_ordinal_and_selects_while_orphan_unbound_rows_drop() {
        // ws-spare: pager trailing + desire trailing + None binding = spare.
        // ws-pending: non-tail None binding = orphan with no live native
        // Space, so it never renders. ws-bound-trailing: bound trailing row
        // is ordinary bound.
        let screens = vec![core_screen(
            "out-a",
            vec![
                core_workspace("ws-1", 1, true, true, false),
                core_workspace("ws-pending", 2, false, false, false),
                core_workspace("ws-bound-tail", 3, false, true, true),
                core_workspace("ws-spare", 4, false, false, true),
            ],
        )];
        let desires = vec![
            core_desire("ws-1", "out-a", Some("space-11"), false, 0),
            core_desire("ws-pending", "out-a", None, false, 1),
            core_desire("ws-bound-tail", "out-a", Some("space-13"), true, 2),
            core_desire("ws-spare", "out-a", None, true, 3),
        ];
        let projection = project_core_indicator(
            &screens,
            &desires,
            1,
            IndicatorMode::All,
            &|_| SpaceOccupancy::Occupied,
            &|_| None,
            &native_row(&[11, 13], 11),
        )
        .expect("provable core row projects");
        assert_eq!(projection.rows.len(), 3);
        assert!(
            projection
                .rows
                .iter()
                .all(|row| row.core.as_ref().expect("core").workspace != "ws-pending")
        );
        assert_eq!(
            projection
                .rows
                .iter()
                .map(|row| (row.position, row.ordinal))
                .collect::<Vec<_>>(),
            vec![(1, 1), (2, 3), (3, 4)],
            "painted slots stay contiguous while ordinals stay core truth"
        );
        let spare = &projection.rows[2];
        assert_eq!(spare.ordinal, 4);
        assert!(spare.core.as_ref().expect("core").spare);
        assert!(spare.selectable, "spare selects like a bound row");
        assert_eq!(spare.occupancy, SpaceOccupancy::Unknown);
        // Ordinal title, never '+'/'?'-plus: the segment title helper
        // renders the core ordinal with the Unknown suffix only.
        assert_eq!(
            segment_title(spare.ordinal, spare.occupancy),
            "4?",
            "spare renders its core ordinal, never a plus marker"
        );
        assert!(!segment_title(spare.ordinal, spare.occupancy).contains('+'));
        let bound_tail = &projection.rows[1];
        assert!(!bound_tail.core.as_ref().expect("core").spare);
        assert!(bound_tail.selectable, "bound trailing is ordinary");
        assert_eq!(bound_tail.sid, 13);
    }

    #[test]
    fn single_output_drops_vanished_and_orphan_rows_against_live_row() {
        // Fresh native row [13, 11]: core rows 11 (current) and 13 remain,
        // the vanished binding 12 and the orphan unbound non-spare drop,
        // and the explicit spare survives. Only the live row decides.
        let screens = vec![core_screen(
            "out-a",
            vec![
                core_workspace("ws-live", 1, true, true, false),
                core_workspace("ws-gone", 2, false, false, false),
                core_workspace("ws-other", 3, false, false, false),
                core_workspace("ws-orphan", 4, false, false, false),
                core_workspace("ws-spare", 5, false, false, true),
            ],
        )];
        let desires = vec![
            core_desire("ws-live", "out-a", Some("space-11"), false, 0),
            core_desire("ws-gone", "out-a", Some("space-12"), false, 1),
            core_desire("ws-other", "out-a", Some("space-13"), false, 2),
            core_desire("ws-orphan", "out-a", None, false, 3),
            core_desire("ws-spare", "out-a", None, true, 4),
        ];
        let projection = project_core_indicator(
            &screens,
            &desires,
            1,
            IndicatorMode::All,
            &|_| SpaceOccupancy::Occupied,
            &|_| None,
            &native_row(&[13, 11], 11),
        )
        .expect("live rows project");
        assert!(projection.visible);
        assert_eq!(
            projection
                .rows
                .iter()
                .map(|row| (
                    row.position,
                    row.ordinal,
                    row.core.as_ref().expect("core").workspace.as_str(),
                ))
                .collect::<Vec<_>>(),
            vec![(1, 1, "ws-live"), (2, 3, "ws-other"), (3, 5, "ws-spare")],
            "phantom rows drop and painted slots stay contiguous"
        );
        assert_eq!(projection.title, "1");
        // The surviving live row still targets its exact core id.
        let other = &projection.rows[1];
        let presentation = core_presentation_for(&screens);
        assert_eq!(
            verify_core_row_click(
                core_click_for(other, 7, 9),
                other,
                7,
                9,
                "gen-9",
                &presentation,
                &desires,
                "gen-9",
            )
            .expect("live row verifies"),
            InputSemanticAction::IndicatorSelectWorkspace {
                output: "out-a".to_owned(),
                workspace: "ws-other".to_owned(),
            }
        );
        // A vanished core current never keeps a stale highlight: the
        // single-output screen safe-hides instead.
        let hidden = project_core_indicator(
            &screens,
            &desires,
            1,
            IndicatorMode::All,
            &|_| SpaceOccupancy::Occupied,
            &|_| None,
            &native_row(&[13], 13),
        )
        .expect("unprovable current hides");
        assert!(!hidden.visible);
        assert!(
            hidden
                .hidden_reason
                .as_deref()
                .is_some_and(|reason| reason.contains("current")),
            "{:?}",
            hidden.hidden_reason
        );
    }

    #[test]
    fn core_click_verifier_selects_bound_trailing_ordinary() {
        // Bound trailing projects ordinary (spare=false) with both raw
        // markers true: the exact-id click must succeed, and a bound
        // marker role flip to ordinary (both markers false, same binding)
        // stays effective-spare false and must not refuse.
        let screens = vec![core_screen(
            "out-a",
            vec![
                core_workspace("ws-1", 1, true, true, false),
                core_workspace("ws-bound-tail", 2, false, false, true),
                core_workspace("ws-spare", 3, false, false, true),
            ],
        )];
        let desires = vec![
            core_desire("ws-1", "out-a", Some("space-11"), false, 0),
            core_desire("ws-bound-tail", "out-a", Some("space-13"), true, 1),
            core_desire("ws-spare", "out-a", None, true, 2),
        ];
        let projection = project_core_indicator(
            &screens,
            &desires,
            1,
            IndicatorMode::All,
            &|_| SpaceOccupancy::Occupied,
            &|_| None,
            &native_row(&[11, 13], 11),
        )
        .expect("bound trailing projects");
        let bound_tail = &projection.rows[1];
        assert!(!bound_tail.core.as_ref().expect("core").spare);
        assert!(bound_tail.selectable, "bound trailing is ordinary");
        let presentation = core_presentation_for(&screens);
        verify_core_row_click(
            core_click_for(bound_tail, 7, 9),
            bound_tail,
            7,
            9,
            "gen-9",
            &presentation,
            &desires,
            "gen-9",
        )
        .expect("bound trailing exact-id click selects");
        // Bound markers flip to ordinary (both false, binding stable):
        // effective spare stays false, click still selects the same id.
        let mut ordinary_desires = desires.clone();
        ordinary_desires[1].trailing = false;
        let mut ordinary_screens = screens.clone();
        ordinary_screens[0].workspaces[1].trailing = false;
        let ordinary_presentation = core_presentation_for(&ordinary_screens);
        verify_core_row_click(
            core_click_for(bound_tail, 7, 9),
            bound_tail,
            7,
            9,
            "gen-9",
            &ordinary_presentation,
            &ordinary_desires,
            "gen-9",
        )
        .expect("bound trailing->ordinary marker flip stays ordinary");
        // Unbound spare still fences: marker loss refuses.
        let spare = &projection.rows[2];
        let mut lost = screens.clone();
        lost[0].workspaces[2].trailing = false;
        assert!(
            verify_core_row_click(
                core_click_for(spare, 7, 9),
                spare,
                7,
                9,
                "gen-9",
                &core_presentation_for(&lost),
                &desires,
                "gen-9",
            )
            .is_err(),
            "unbound spare pager marker loss still refuses"
        );
        // Unbound spare still fences: binding arrival refuses.
        let mut bound = desires.clone();
        bound[2].native_binding = Some("space-99".to_owned());
        assert!(
            verify_core_row_click(
                core_click_for(spare, 7, 9),
                spare,
                7,
                9,
                "gen-9",
                &presentation,
                &bound,
                "gen-9",
            )
            .is_err(),
            "unbound spare binding arrival still refuses"
        );
    }

    #[test]
    fn core_occupancy_is_authoritative_over_host_summary() {
        // Core occupied=true beats a host Empty; core false beats host
        // Occupied; host Unknown still styles Unknown.
        let screens = vec![core_screen(
            "out-a",
            vec![
                core_workspace("ws-occ", 1, true, true, false),
                core_workspace("ws-empty", 2, false, false, false),
                core_workspace("ws-unk", 3, false, true, false),
            ],
        )];
        let desires = vec![
            core_desire("ws-occ", "out-a", Some("space-21"), false, 0),
            core_desire("ws-empty", "out-a", Some("space-22"), false, 1),
            core_desire("ws-unk", "out-a", Some("space-23"), false, 2),
        ];
        let occupancy_of = |sid: u64| match sid {
            21 => SpaceOccupancy::Empty,
            22 => SpaceOccupancy::Occupied,
            23 => SpaceOccupancy::Unknown,
            _ => SpaceOccupancy::Unknown,
        };
        let projection = project_core_indicator(
            &screens,
            &desires,
            1,
            IndicatorMode::All,
            &occupancy_of,
            &|_| None,
            &native_row(&[21, 22, 23], 21),
        )
        .expect("projects");
        assert_eq!(projection.rows[0].occupancy, SpaceOccupancy::Occupied);
        assert_eq!(projection.rows[1].occupancy, SpaceOccupancy::Empty);
        assert_eq!(projection.rows[2].occupancy, SpaceOccupancy::Unknown);
    }

    #[test]
    fn core_projection_hides_without_pager_and_gates_modes() {
        // No screens: hidden, never guessed. Empty pager screen: hidden.
        let hidden = project_core_indicator(
            &[],
            &[],
            1,
            IndicatorMode::All,
            &|_| SpaceOccupancy::Occupied,
            &|_| None,
            &native_row(&[11], 11),
        )
        .expect("hidden projects");
        assert!(!hidden.visible);
        let empty_screen = vec![core_screen("out-a", Vec::new())];
        let hidden_empty = project_core_indicator(
            &empty_screen,
            &[],
            1,
            IndicatorMode::All,
            &|_| SpaceOccupancy::Occupied,
            &|_| None,
            &native_row(&[11], 11),
        )
        .expect("hidden projects");
        assert!(!hidden_empty.visible);
        // Mode gates: Off hides; Current keeps current plus spare only.
        let screens = vec![core_screen(
            "out-a",
            vec![
                core_workspace("ws-1", 1, true, true, false),
                core_workspace("ws-2", 2, false, false, false),
                core_workspace("ws-spare", 3, false, false, true),
            ],
        )];
        let desires = vec![
            core_desire("ws-1", "out-a", Some("space-11"), false, 0),
            core_desire("ws-2", "out-a", Some("space-12"), false, 1),
            core_desire("ws-spare", "out-a", None, true, 2),
        ];
        let off = project_core_indicator(
            &screens,
            &desires,
            1,
            IndicatorMode::Off,
            &|_| SpaceOccupancy::Occupied,
            &|_| None,
            &native_row(&[11, 12], 11),
        )
        .expect("off projects hidden");
        assert!(!off.visible);
        let current = project_core_indicator(
            &screens,
            &desires,
            1,
            IndicatorMode::Current,
            &|_| SpaceOccupancy::Occupied,
            &|_| None,
            &native_row(&[11, 12], 11),
        )
        .expect("current projects");
        assert_eq!(current.rows.len(), 2);
        assert!(current.rows.iter().any(|row| row.current));
        assert!(
            current
                .rows
                .iter()
                .any(|row| row.core.as_ref().expect("core").spare)
        );
        // Multi-display and malformed bindings stay safe: a malformed Some
        // binding names no native Space and drops rather than guessing a
        // desktop or painting a phantom row.
        let malformed_desires = vec![
            core_desire("ws-1", "out-a", Some("space-11"), false, 0),
            core_desire("ws-2", "out-a", Some("bogus"), false, 1),
            core_desire("ws-spare", "out-a", None, true, 2),
        ];
        let malformed = project_core_indicator(
            &screens,
            &malformed_desires,
            1,
            IndicatorMode::All,
            &|_| SpaceOccupancy::Occupied,
            &|_| None,
            &native_row(&[11], 11),
        )
        .expect("projects");
        assert_eq!(malformed.rows.len(), 2);
        assert!(
            malformed
                .rows
                .iter()
                .all(|row| row.core.as_ref().expect("core").workspace != "ws-2")
        );
        assert!(malformed.rows[1].core.as_ref().expect("core").spare);
        let multi = project_core_indicator(
            &screens,
            &desires,
            2,
            IndicatorMode::All,
            &|_| SpaceOccupancy::Occupied,
            &|_| None,
            &native_row(&[11, 12], 11),
        )
        .expect("multi projects hidden");
        assert!(!multi.visible);
    }
}

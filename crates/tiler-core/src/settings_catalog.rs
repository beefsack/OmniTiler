//! Shared portable settings catalog: static dependency-free descriptors.
//!
//! Portable field/tab/section/type/default/bounds/token/label/help/timing
//! metadata plus explicit validity outcomes. No toolkit types, no serialized
//! JSON, no storage I/O, no native operations here. Platform adapters map
//! these stable field IDs to their own storage keys and own all widgets,
//! persistence, and operations. A future exporter outside this crate may
//! read these tables to emit checked-in native descriptors; nothing here
//! generates or requires generated code.

use crate::admission::{AnchorFallback, NoSpaceResult};
use crate::born_maximized::BornMaximized;
use crate::cursor_targeting::{FreshPlacement, SwitchTarget};
use crate::directional::SameAxisMove;
use crate::output_selection::MigrationSourceRefill;
use crate::size_hints::FixedSizePredicate;
use crate::workspace_scope;

// ---------------------------------------------------------------------------
// Tabs (exactly four)
// ---------------------------------------------------------------------------

/// Tiling and workspaces tab.
pub const TAB_TILING_WORKSPACES: &str = "tiling-workspaces";
/// Appearance tab.
pub const TAB_APPEARANCE: &str = "appearance";
/// Shortcuts tab (portable fields: none; native panels live in adapters).
pub const TAB_SHORTCUTS: &str = "shortcuts";
/// Desktop integration tab (portable fields: none; native panels in adapters).
pub const TAB_DESKTOP_INTEGRATION: &str = "desktop-integration";

/// One settings tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TabDef {
    pub id: &'static str,
    pub label: &'static str,
}

/// All four tabs in stable order.
pub const ALL_TABS: &[TabDef] = &[
    TabDef {
        id: TAB_TILING_WORKSPACES,
        label: "Tiling & Workspaces",
    },
    TabDef {
        id: TAB_APPEARANCE,
        label: "Appearance",
    },
    TabDef {
        id: TAB_SHORTCUTS,
        label: "Shortcuts",
    },
    TabDef {
        id: TAB_DESKTOP_INTEGRATION,
        label: "Desktop Integration",
    },
];

// ---------------------------------------------------------------------------
// Sections
// ---------------------------------------------------------------------------

/// Gaps section.
pub const SECTION_GAPS: &str = "tiling.gaps";
/// Workspaces section (default tiling seed plus scope mode).
pub const SECTION_WORKSPACES: &str = "tiling.workspaces";
/// Same-axis move section.
pub const SECTION_MOVE: &str = "tiling.move";
/// Admission section (born-maximized plus fixed-size).
pub const SECTION_ADMISSION: &str = "tiling.admission";
/// Migration refill section.
pub const SECTION_MIGRATION_REFILL: &str = "tiling.migration-refill";
/// Output-targeting section (workspace-switch target, fresh placement,
/// focus-follows-cursor, cursor-warp).
pub const SECTION_OUTPUT_TARGETING: &str = "tiling.output-targeting";
/// Border section.
pub const SECTION_BORDER: &str = "appearance.border";
/// Underlay section.
pub const SECTION_UNDERLAY: &str = "appearance.underlay";

/// One section inside a tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SectionDef {
    pub id: &'static str,
    pub tab: &'static str,
    pub label: &'static str,
}

/// All portable sections in stable order.
pub const ALL_SECTIONS: &[SectionDef] = &[
    SectionDef {
        id: SECTION_GAPS,
        tab: TAB_TILING_WORKSPACES,
        label: "Gaps",
    },
    SectionDef {
        id: SECTION_WORKSPACES,
        tab: TAB_TILING_WORKSPACES,
        label: "Workspaces",
    },
    SectionDef {
        id: SECTION_MOVE,
        tab: TAB_TILING_WORKSPACES,
        label: "Same-axis move",
    },
    SectionDef {
        id: SECTION_ADMISSION,
        tab: TAB_TILING_WORKSPACES,
        label: "Admission",
    },
    SectionDef {
        id: SECTION_MIGRATION_REFILL,
        tab: TAB_TILING_WORKSPACES,
        label: "Migration refill",
    },
    SectionDef {
        id: SECTION_OUTPUT_TARGETING,
        tab: TAB_TILING_WORKSPACES,
        label: "Output targeting",
    },
    SectionDef {
        id: SECTION_BORDER,
        tab: TAB_APPEARANCE,
        label: "Active border",
    },
    SectionDef {
        id: SECTION_UNDERLAY,
        tab: TAB_APPEARANCE,
        label: "Group underlay",
    },
];

// ---------------------------------------------------------------------------
// Generic descriptor/slot types (adapters reuse these, no platform policy)
// ---------------------------------------------------------------------------

/// Field value type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldType {
    Bool,
    Int,
    Float,
    ColorRgb,
    ColorArgb,
    Enum,
}

/// When a saved value takes effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Timing {
    /// Takes effect without restart (next layout or pump).
    Live,
    /// Saved now, applies to subsequently admitted/moved/migrated items or
    /// subsequently created workspaces. See `timing_help` for the exact
    /// honest scope; never claims runtime adoption of existing state.
    NextAdmission,
    /// Saved now, the running owner keeps its startup value until restart.
    Restart,
}

impl Timing {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Live => "live",
            Self::NextAdmission => "next-admission",
            Self::Restart => "restart",
        }
    }
}

/// Host capability flags for one field. Portable fields list every host
/// that exposes them; unsupported hosts omit the field from their UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HostCaps {
    pub windows: bool,
    pub kde: bool,
    pub macos: bool,
}

/// Capabilities for every portable field below: exposed on Windows and KDE,
/// not on macOS (no tiling-settings UI there yet).
pub const PORTABLE_CAPS: HostCaps = HostCaps {
    windows: true,
    kde: true,
    macos: false,
};

/// Typed field value (also used for defaults). Text carries enum tokens
/// and color strings; no JSON travels through this type.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SettingValue {
    Bool(bool),
    Int(i32),
    Float(f64),
    Text(&'static str),
}

/// One enum option: exact wire token, functional label, tooltip-only
/// reference note (never a setting id).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EnumOption {
    pub token: &'static str,
    pub label: &'static str,
    pub tip: &'static str,
}

/// One portable field descriptor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FieldDef {
    pub id: &'static str,
    pub section: &'static str,
    pub label: &'static str,
    pub help: &'static str,
    pub field_type: FieldType,
    pub default: SettingValue,
    pub min_int: Option<i32>,
    pub max_int: Option<i32>,
    pub min_float: Option<f64>,
    pub max_float: Option<f64>,
    pub options: &'static [EnumOption],
    pub timing: Timing,
    pub timing_help: &'static str,
    pub capabilities: HostCaps,
    /// Why the bound exists. Required at every bounded definition.
    pub bounds_reason: &'static str,
}

/// Generic native panel slot. Adapters declare their own explicit panels
/// with these (presets, binding editor, gaming operations, takeover and
/// Win+L remediation, snap policy); no generic setter may execute them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PanelSlot {
    pub id: &'static str,
    pub tab: &'static str,
    pub section: &'static str,
    pub label: &'static str,
}

/// Explicit validity outcome for one typed value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Validity {
    Valid,
    Invalid(&'static str),
}

// ---------------------------------------------------------------------------
// Stable portable field IDs
// ---------------------------------------------------------------------------

pub const FIELD_INNER_GAP: &str = "gaps.inner";
pub const FIELD_OUTER_GAP: &str = "gaps.outer";
pub const FIELD_SAME_AXIS_MOVE: &str = "move.same-axis";
pub const FIELD_BORN_MAXIMIZED: &str = "admission.born-maximized";
pub const FIELD_FIXED_SIZE: &str = "admission.fixed-size";
pub const FIELD_ANCHOR_FALLBACK: &str = "admission.anchor-fallback";
pub const FIELD_NO_SPACE: &str = "admission.no-space";
pub const FIELD_MIGRATION_REFILL: &str = "migration.refill";
pub const FIELD_BORDER_ENABLED: &str = "border.enabled";
pub const FIELD_BORDER_WIDTH: &str = "border.width";
pub const FIELD_BORDER_GAP: &str = "border.gap";
pub const FIELD_BORDER_RADIUS: &str = "border.radius";
pub const FIELD_BORDER_COLOR: &str = "border.color";
pub const FIELD_BORDER_USE_THEME: &str = "border.use-theme";
pub const FIELD_UNDERLAY_ENABLED: &str = "underlay.enabled";
pub const FIELD_UNDERLAY_COLOR: &str = "underlay.color";
pub const FIELD_UNDERLAY_EXTENSION: &str = "underlay.extension";
pub const FIELD_WORKSPACE_DEFAULT_TILED: &str = "workspace.default-tiled";
pub const FIELD_WORKSPACE_MODE: &str = "workspace.mode";
pub const FIELD_SWITCH_TARGET: &str = "output.switch-target";
pub const FIELD_FRESH_PLACEMENT: &str = "output.fresh-placement";
pub const FIELD_FOCUS_FOLLOWS_CURSOR: &str = "output.focus-follows-cursor";
pub const FIELD_CURSOR_WARP: &str = "output.cursor-warp";

// ---------------------------------------------------------------------------
// Portable defaults and bounds (single source owned here)
// ---------------------------------------------------------------------------

/// Default gap 8: retains the existing accepted gap default.
pub const DEFAULT_GAP: i32 = 8;
/// Gap floor 0: negative gaps are meaningless; retains the existing
/// accepted settings range floor.
pub const MIN_CATALOG_GAP: i32 = 0;
/// Gap bound 0..=64: aliases the core carried-geometry gate; retains the
/// existing accepted settings range and bounds carried work-area geometry.
pub const MAX_CATALOG_GAP: i32 = crate::bounds::MAX_GAP;
/// Default border width 3.0: retains the existing accepted default.
pub const DEFAULT_CATALOG_BORDER_WIDTH: f64 = 3.0;
/// Border width floor 0.0: negative widths are meaningless; retains the
/// existing accepted settings range floor.
pub const MIN_CATALOG_BORDER_WIDTH: f64 = 0.0;
/// Border width bound 0..=32: retains the existing accepted settings range
/// and bounds overlay geometry.
pub const MAX_CATALOG_BORDER_WIDTH: f64 = 32.0;
/// Default border gap 0.0: retains the existing accepted default.
pub const DEFAULT_CATALOG_BORDER_GAP: f64 = 0.0;
/// Border gap floor 0.0: negative offsets are meaningless; retains the
/// existing accepted settings range floor.
pub const MIN_CATALOG_BORDER_GAP: f64 = 0.0;
/// Border gap bound 0..=64: retains the existing accepted settings range
/// and bounds overlay geometry.
pub const MAX_CATALOG_BORDER_GAP: f64 = 64.0;
/// Default border radius 6.0: retains the existing accepted default,
/// matching `kwin/native-effect/activeborderconfig.kcfg`.
pub const DEFAULT_CATALOG_BORDER_RADIUS: f64 = 6.0;
/// Border radius floor 0.0: negative radii are meaningless; retains the
/// existing accepted settings range floor.
pub const MIN_CATALOG_BORDER_RADIUS: f64 = 0.0;
/// Border radius bound 0..=64: retains the existing accepted settings range
/// and bounds overlay geometry.
pub const MAX_CATALOG_BORDER_RADIUS: f64 = 64.0;
/// Default border color: retains the existing accepted default.
pub const DEFAULT_CATALOG_BORDER_COLOR: &str = "#2a82da";
/// Default theme flag: retains the existing accepted default.
pub const DEFAULT_CATALOG_BORDER_USE_THEME: bool = true;
/// Default underlay color: retains the existing accepted default.
pub const DEFAULT_CATALOG_UNDERLAY_COLOR: &str = "#662a82da";
/// Default underlay extension -1.0: retains the existing accepted default;
/// -1 is the follow-border sentinel.
pub const DEFAULT_CATALOG_UNDERLAY_EXTENSION: f64 = -1.0;
/// Underlay extension floor -1.0: values below the follow-border sentinel
/// are meaningless; retains the existing accepted settings range floor.
pub const MIN_CATALOG_UNDERLAY_EXTENSION: f64 = -1.0;
/// Underlay extension bound -1..=32: retains the existing accepted settings
/// range (-1 is the follow-border sentinel) and bounds overlay geometry.
pub const MAX_CATALOG_UNDERLAY_EXTENSION: f64 = 32.0;

// ---------------------------------------------------------------------------
// Enum option tables (exact tokens; functional labels; tooltip-only refs)
// ---------------------------------------------------------------------------

pub const SAME_AXIS_MOVE_OPTIONS: [EnumOption; 2] = [
    EnumOption {
        token: SameAxisMove::GroupWithNeighbor.as_wire_str(),
        label: "Group with neighbor",
        tip: "COSMIC",
    },
    EnumOption {
        token: SameAxisMove::SwapWithNeighbor.as_wire_str(),
        label: "Swap with neighbor",
        tip: "i3, sway",
    },
];

pub const BORN_MAXIMIZED_OPTIONS: [EnumOption; 2] = [
    EnumOption {
        token: BornMaximized::TileOnLaunch.as_wire_str(),
        label: "Tile on launch",
        tip: "bspwm, i3, xmonad, sway, qtile",
    },
    EnumOption {
        token: BornMaximized::KeepMaximizedOverlay.as_wire_str(),
        label: "Keep maximized overlay",
        tip: "COSMIC, Hyprland",
    },
];

pub const FIXED_SIZE_OPTIONS: [EnumOption; 2] = [
    EnumOption {
        token: FixedSizePredicate::BothAxes.as_wire_str(),
        label: "Width and height both fixed",
        tip: "COSMIC",
    },
    EnumOption {
        token: FixedSizePredicate::EitherAxis.as_wire_str(),
        label: "Width or height fixed",
        tip: "Hyprland (Wayland), sway",
    },
];

pub const ANCHOR_FALLBACK_OPTIONS: [EnumOption; 3] = [
    EnumOption {
        token: AnchorFallback::LargestSplittable.as_wire_str(),
        label: "Largest splittable window",
        tip: "Most remaining area first",
    },
    EnumOption {
        token: AnchorFallback::ClosestRelative.as_wire_str(),
        label: "Closest splittable relative",
        tip: "Nearest branch first",
    },
    EnumOption {
        token: AnchorFallback::NoSearch.as_wire_str(),
        label: "No search",
        tip: "Go directly to the no-space setting",
    },
];

pub const NO_SPACE_OPTIONS: [EnumOption; 2] = [
    EnumOption {
        token: NoSpaceResult::Float.as_wire_str(),
        label: "Float",
        tip: "Keep tiling intact on a separate layer",
    },
    EnumOption {
        token: NoSpaceResult::AllowOverlap.as_wire_str(),
        label: "Allow overlap",
        tip: "Keep tiled membership with overflowing geometry",
    },
];

pub const MIGRATION_REFILL_OPTIONS: [EnumOption; 2] = [
    EnumOption {
        token: MigrationSourceRefill::LastRemaining.as_wire_str(),
        label: "Last remaining workspace",
        tip: "COSMIC",
    },
    EnumOption {
        token: MigrationSourceRefill::MostRecentlyUsed.as_wire_str(),
        label: "Most recently used workspace",
        tip: "bspwm, i3, awesome",
    },
];

pub const SWITCH_TARGET_OPTIONS: [EnumOption; 2] = [
    EnumOption {
        token: SwitchTarget::Cursor.as_wire_str(),
        label: "Cursor output",
        tip: "COSMIC",
    },
    EnumOption {
        token: SwitchTarget::Focused.as_wire_str(),
        label: "Focused output",
        tip: "i3, sway",
    },
];

pub const FRESH_PLACEMENT_OPTIONS: [EnumOption; 3] = [
    EnumOption {
        token: FreshPlacement::CursorOutputMru.as_wire_str(),
        label: "Cursor output, last-focused window",
        tip: "COSMIC",
    },
    EnumOption {
        token: FreshPlacement::CursorOutputCursorClosest.as_wire_str(),
        label: "Cursor output, cursor-closest window",
        tip: "Hyprland",
    },
    EnumOption {
        token: FreshPlacement::FocusedOutputMru.as_wire_str(),
        label: "Focused output, last-focused window",
        tip: "i3, sway",
    },
];

pub const WORKSPACE_MODE_OPTIONS: [EnumOption; 3] = [
    EnumOption {
        token: workspace_scope::WORKSPACE_MODE_TOKENS[0],
        label: "Per-output local",
        tip: "Each output keeps its own set",
    },
    EnumOption {
        token: workspace_scope::WORKSPACE_MODE_TOKENS[1],
        label: "Global unique",
        tip: "Distinct sets with cross-output swap",
    },
    EnumOption {
        token: workspace_scope::WORKSPACE_MODE_TOKENS[2],
        label: "Shared",
        tip: "One set shown across all outputs",
    },
];

// ---------------------------------------------------------------------------
// Portable field table (23 fields)
// ---------------------------------------------------------------------------

/// All portable fields in stable order.
pub const ALL_FIELDS: &[FieldDef] = &[
    FieldDef {
        id: FIELD_INNER_GAP,
        section: SECTION_GAPS,
        label: "Inner gap",
        help: "Gap between tiled windows in logical pixels.",
        field_type: FieldType::Int,
        default: SettingValue::Int(DEFAULT_GAP),
        min_int: Some(MIN_CATALOG_GAP),
        max_int: Some(MAX_CATALOG_GAP),
        min_float: None,
        max_float: None,
        options: &[],
        timing: Timing::Live,
        timing_help: "Applies to subsequent layouts without restart.",
        capabilities: PORTABLE_CAPS,
        bounds_reason: "Carried work-area geometry bound (0..64); retains the existing accepted settings range.",
    },
    FieldDef {
        id: FIELD_OUTER_GAP,
        section: SECTION_GAPS,
        label: "Outer gap",
        help: "Gap between tiled windows and the work-area edge in logical pixels.",
        field_type: FieldType::Int,
        default: SettingValue::Int(DEFAULT_GAP),
        min_int: Some(MIN_CATALOG_GAP),
        max_int: Some(MAX_CATALOG_GAP),
        min_float: None,
        max_float: None,
        options: &[],
        timing: Timing::Live,
        timing_help: "Applies to subsequent layouts without restart.",
        capabilities: PORTABLE_CAPS,
        bounds_reason: "Carried work-area geometry bound (0..64); retains the existing accepted settings range.",
    },
    FieldDef {
        id: FIELD_WORKSPACE_DEFAULT_TILED,
        section: SECTION_WORKSPACES,
        label: "New workspaces start tiled",
        help: "Seeds every newly created workspace; per-workspace session overrides never persist.",
        field_type: FieldType::Bool,
        default: SettingValue::Bool(true),
        min_int: None,
        max_int: None,
        min_float: None,
        max_float: None,
        options: &[],
        timing: Timing::NextAdmission,
        timing_help: "Applies to workspaces created after Apply; existing workspaces keep their session tiling.",
        capabilities: PORTABLE_CAPS,
        bounds_reason: "Boolean field; no numeric bound.",
    },
    FieldDef {
        id: FIELD_WORKSPACE_MODE,
        section: SECTION_WORKSPACES,
        label: "Workspace scope mode",
        help: "Per-output local sets, global-unique sets with cross-output swap, or one shared set.",
        field_type: FieldType::Enum,
        default: SettingValue::Text(workspace_scope::WORKSPACE_MODE_TOKENS[0]),
        min_int: None,
        max_int: None,
        min_float: None,
        max_float: None,
        options: &WORKSPACE_MODE_OPTIONS,
        timing: Timing::Restart,
        timing_help: "Saved immediately; the running owner keeps its startup scope until restart.",
        capabilities: PORTABLE_CAPS,
        bounds_reason: "Closed token set; unknown tokens refuse instead of migrating.",
    },
    FieldDef {
        id: FIELD_SAME_AXIS_MOVE,
        section: SECTION_MOVE,
        label: "Same-axis move",
        help: "Group with the neighbor or swap with the neighbor when moving along its own axis.",
        field_type: FieldType::Enum,
        default: SettingValue::Text(SameAxisMove::GroupWithNeighbor.as_wire_str()),
        min_int: None,
        max_int: None,
        min_float: None,
        max_float: None,
        options: &SAME_AXIS_MOVE_OPTIONS,
        timing: Timing::NextAdmission,
        timing_help: "Applies to subsequent moves only; existing trees keep their layout until the next move.",
        capabilities: PORTABLE_CAPS,
        bounds_reason: "Closed token set; unknown tokens refuse instead of migrating.",
    },
    FieldDef {
        id: FIELD_BORN_MAXIMIZED,
        section: SECTION_ADMISSION,
        label: "Born-maximized admission",
        help: "Tile a born-maximized window on launch or keep its maximized overlay.",
        field_type: FieldType::Enum,
        default: SettingValue::Text(BornMaximized::TileOnLaunch.as_wire_str()),
        min_int: None,
        max_int: None,
        min_float: None,
        max_float: None,
        options: &BORN_MAXIMIZED_OPTIONS,
        timing: Timing::NextAdmission,
        timing_help: "Applies to subsequent admissions only; admitted windows keep their state.",
        capabilities: PORTABLE_CAPS,
        bounds_reason: "Closed token set; unknown tokens refuse instead of migrating.",
    },
    FieldDef {
        id: FIELD_FIXED_SIZE,
        section: SECTION_ADMISSION,
        label: "Fixed-size admission",
        help: "Float windows whose width and height are both fixed, or where either axis is fixed.",
        field_type: FieldType::Enum,
        default: SettingValue::Text(FixedSizePredicate::BothAxes.as_wire_str()),
        min_int: None,
        max_int: None,
        min_float: None,
        max_float: None,
        options: &FIXED_SIZE_OPTIONS,
        timing: Timing::NextAdmission,
        timing_help: "Applies to subsequent admissions only; retained classifications never change.",
        capabilities: PORTABLE_CAPS,
        bounds_reason: "Closed token set; unknown tokens refuse instead of migrating.",
    },
    FieldDef {
        id: FIELD_ANCHOR_FALLBACK,
        section: SECTION_ADMISSION,
        label: "When the anchor cannot split",
        help: "Search this workspace for another tile to split. Geometric positive extent only; declared-minimum overlap remains. Search never leaves the domain; no-search uses the no-space setting directly.",
        field_type: FieldType::Enum,
        default: SettingValue::Text(AnchorFallback::LargestSplittable.as_wire_str()),
        min_int: None,
        max_int: None,
        min_float: None,
        max_float: None,
        options: &ANCHOR_FALLBACK_OPTIONS,
        timing: Timing::NextAdmission,
        timing_help: "Applies to subsequent admissions only; admitted windows keep their state.",
        capabilities: PORTABLE_CAPS,
        bounds_reason: "Closed token set; unknown tokens refuse instead of migrating.",
    },
    FieldDef {
        id: FIELD_NO_SPACE,
        section: SECTION_ADMISSION,
        label: "When no split is available",
        help: "Float the window or allow tiled overlap. Overlap keeps positive extents but may cover tiles; declared-minimum overlap remains. Floats stay floating until explicitly tiled.",
        field_type: FieldType::Enum,
        default: SettingValue::Text(NoSpaceResult::Float.as_wire_str()),
        min_int: None,
        max_int: None,
        min_float: None,
        max_float: None,
        options: &NO_SPACE_OPTIONS,
        timing: Timing::NextAdmission,
        timing_help: "Applies to subsequent admission or actual capacity-repair decisions only; existing tiles and floats keep their state, no reflow on Apply.",
        capabilities: PORTABLE_CAPS,
        bounds_reason: "Closed token set; unknown tokens refuse instead of migrating.",
    },
    FieldDef {
        id: FIELD_MIGRATION_REFILL,
        section: SECTION_MIGRATION_REFILL,
        label: "Migration source refill",
        help: "Which remaining workspace refills a source scope emptied by whole-workspace migration.",
        field_type: FieldType::Enum,
        default: SettingValue::Text(MigrationSourceRefill::LastRemaining.as_wire_str()),
        min_int: None,
        max_int: None,
        min_float: None,
        max_float: None,
        options: &MIGRATION_REFILL_OPTIONS,
        timing: Timing::NextAdmission,
        timing_help: "Applies to subsequent migrations only; already-migrated workspaces keep their state.",
        capabilities: PORTABLE_CAPS,
        bounds_reason: "Closed token set; unknown tokens refuse instead of migrating.",
    },
    FieldDef {
        id: FIELD_SWITCH_TARGET,
        section: SECTION_OUTPUT_TARGETING,
        label: "Workspace-switch target",
        help: "Which output numbered, relative, previous and trailing selections act on: the cursor output or the focused window's output.",
        field_type: FieldType::Enum,
        default: SettingValue::Text(SwitchTarget::Cursor.as_wire_str()),
        min_int: None,
        max_int: None,
        min_float: None,
        max_float: None,
        options: &SWITCH_TARGET_OPTIONS,
        timing: Timing::NextAdmission,
        timing_help: "Applies to subsequent selections only; retained views keep their state.",
        capabilities: PORTABLE_CAPS,
        bounds_reason: "Closed token set; unknown tokens refuse instead of migrating.",
    },
    FieldDef {
        id: FIELD_FRESH_PLACEMENT,
        section: SECTION_OUTPUT_TARGETING,
        label: "New-window placement",
        help: "Which output a newly opened window admits into and which anchor it splits: cursor output with its last-focused window, cursor output with the cursor-closest window, or focused output with its last-focused window.",
        field_type: FieldType::Enum,
        default: SettingValue::Text(FreshPlacement::CursorOutputMru.as_wire_str()),
        min_int: None,
        max_int: None,
        min_float: None,
        max_float: None,
        options: &FRESH_PLACEMENT_OPTIONS,
        timing: Timing::NextAdmission,
        timing_help: "Applies to subsequent admissions only; admitted windows keep their state.",
        capabilities: PORTABLE_CAPS,
        bounds_reason: "Closed token set; unknown tokens refuse instead of migrating.",
    },
    FieldDef {
        id: FIELD_FOCUS_FOLLOWS_CURSOR,
        section: SECTION_OUTPUT_TARGETING,
        label: "Focus follows cursor",
        help: "Move keyboard focus when the cursor crosses into another managed window on a readable unheld output. Pauses under a focused fullscreen game, while a drag/resize gesture runs, and across incomplete inventory.",
        field_type: FieldType::Bool,
        default: SettingValue::Bool(false),
        min_int: None,
        max_int: None,
        min_float: None,
        max_float: None,
        options: &[],
        timing: Timing::Live,
        timing_help: "Applies live without restart; hover focus mints on the next sampled report.",
        capabilities: PORTABLE_CAPS,
        bounds_reason: "Boolean field; no numeric bound.",
    },
    FieldDef {
        id: FIELD_CURSOR_WARP,
        section: SECTION_OUTPUT_TARGETING,
        label: "Cursor warp",
        help: "Move the cursor to the focused window when keyboard focus crosses to another output. Same-output focus never warps. Pauses under a focused fullscreen game, while a drag/resize gesture runs, and across incomplete inventory.",
        field_type: FieldType::Bool,
        default: SettingValue::Bool(false),
        min_int: None,
        max_int: None,
        min_float: None,
        max_float: None,
        options: &[],
        timing: Timing::Live,
        timing_help: "Applies live without restart; the next cross-output focus publishes the warp point.",
        capabilities: PORTABLE_CAPS,
        bounds_reason: "Boolean field; no numeric bound.",
    },
    FieldDef {
        id: FIELD_BORDER_ENABLED,
        section: SECTION_BORDER,
        label: "Active border enabled",
        help: "Draw the active-window border.",
        field_type: FieldType::Bool,
        default: SettingValue::Bool(true),
        min_int: None,
        max_int: None,
        min_float: None,
        max_float: None,
        options: &[],
        timing: Timing::Live,
        timing_help: "Applies live without restart.",
        capabilities: PORTABLE_CAPS,
        bounds_reason: "Boolean field; no numeric bound.",
    },
    FieldDef {
        id: FIELD_BORDER_WIDTH,
        section: SECTION_BORDER,
        label: "Border width",
        help: "Active border width in logical points.",
        field_type: FieldType::Float,
        default: SettingValue::Float(DEFAULT_CATALOG_BORDER_WIDTH),
        min_int: None,
        max_int: None,
        min_float: Some(MIN_CATALOG_BORDER_WIDTH),
        max_float: Some(MAX_CATALOG_BORDER_WIDTH),
        options: &[],
        timing: Timing::Live,
        timing_help: "Applies live without restart.",
        capabilities: PORTABLE_CAPS,
        bounds_reason: "Retains the existing accepted settings range (0..32); bounds overlay geometry.",
    },
    FieldDef {
        id: FIELD_BORDER_GAP,
        section: SECTION_BORDER,
        label: "Border gap",
        help: "Gap between the window frame and the active border in logical points.",
        field_type: FieldType::Float,
        default: SettingValue::Float(DEFAULT_CATALOG_BORDER_GAP),
        min_int: None,
        max_int: None,
        min_float: Some(MIN_CATALOG_BORDER_GAP),
        max_float: Some(MAX_CATALOG_BORDER_GAP),
        options: &[],
        timing: Timing::Live,
        timing_help: "Applies live without restart.",
        capabilities: PORTABLE_CAPS,
        bounds_reason: "Retains the existing accepted settings range (0..64); bounds overlay geometry.",
    },
    FieldDef {
        id: FIELD_BORDER_RADIUS,
        section: SECTION_BORDER,
        label: "Border radius",
        help: "Active border corner radius in logical points.",
        field_type: FieldType::Float,
        default: SettingValue::Float(DEFAULT_CATALOG_BORDER_RADIUS),
        min_int: None,
        max_int: None,
        min_float: Some(MIN_CATALOG_BORDER_RADIUS),
        max_float: Some(MAX_CATALOG_BORDER_RADIUS),
        options: &[],
        timing: Timing::Live,
        timing_help: "Applies live without restart.",
        capabilities: PORTABLE_CAPS,
        bounds_reason: "Retains the existing accepted settings range (0..64); bounds overlay geometry.",
    },
    FieldDef {
        id: FIELD_BORDER_COLOR,
        section: SECTION_BORDER,
        label: "Border color",
        help: "Configured color as #rrggbb; wins when theming is off or the system accent is unavailable.",
        field_type: FieldType::ColorRgb,
        default: SettingValue::Text(DEFAULT_CATALOG_BORDER_COLOR),
        min_int: None,
        max_int: None,
        min_float: None,
        max_float: None,
        options: &[],
        timing: Timing::Live,
        timing_help: "Applies live without restart.",
        capabilities: PORTABLE_CAPS,
        bounds_reason: "Fixed #rrggbb grammar; malformed colors refuse instead of guessing.",
    },
    FieldDef {
        id: FIELD_BORDER_USE_THEME,
        section: SECTION_BORDER,
        label: "Use system theme accent",
        help: "Follow the live system accent with the configured color as fallback.",
        field_type: FieldType::Bool,
        default: SettingValue::Bool(DEFAULT_CATALOG_BORDER_USE_THEME),
        min_int: None,
        max_int: None,
        min_float: None,
        max_float: None,
        options: &[],
        timing: Timing::Live,
        timing_help: "Applies live without restart.",
        capabilities: PORTABLE_CAPS,
        bounds_reason: "Boolean field; no numeric bound.",
    },
    FieldDef {
        id: FIELD_UNDERLAY_ENABLED,
        section: SECTION_UNDERLAY,
        label: "Group underlay enabled",
        help: "Draw the group underlay behind tiled groups.",
        field_type: FieldType::Bool,
        default: SettingValue::Bool(true),
        min_int: None,
        max_int: None,
        min_float: None,
        max_float: None,
        options: &[],
        timing: Timing::Live,
        timing_help: "Applies live without restart.",
        capabilities: PORTABLE_CAPS,
        bounds_reason: "Boolean field; no numeric bound.",
    },
    FieldDef {
        id: FIELD_UNDERLAY_COLOR,
        section: SECTION_UNDERLAY,
        label: "Underlay color",
        help: "Underlay color as #aarrggbb including alpha.",
        field_type: FieldType::ColorArgb,
        default: SettingValue::Text(DEFAULT_CATALOG_UNDERLAY_COLOR),
        min_int: None,
        max_int: None,
        min_float: None,
        max_float: None,
        options: &[],
        timing: Timing::Live,
        timing_help: "Applies live without restart.",
        capabilities: PORTABLE_CAPS,
        bounds_reason: "Fixed #aarrggbb grammar; malformed colors refuse instead of guessing.",
    },
    FieldDef {
        id: FIELD_UNDERLAY_EXTENSION,
        section: SECTION_UNDERLAY,
        label: "Underlay extension",
        help: "Logical outset beyond gap plus border width; -1 follows the border width.",
        field_type: FieldType::Float,
        default: SettingValue::Float(DEFAULT_CATALOG_UNDERLAY_EXTENSION),
        min_int: None,
        max_int: None,
        min_float: Some(MIN_CATALOG_UNDERLAY_EXTENSION),
        max_float: Some(MAX_CATALOG_UNDERLAY_EXTENSION),
        options: &[],
        timing: Timing::Live,
        timing_help: "Applies live without restart.",
        capabilities: PORTABLE_CAPS,
        bounds_reason: "Retains the existing accepted settings range (-1..32); -1 is the follow-border sentinel and values bound overlay geometry.",
    },
];

// ---------------------------------------------------------------------------
// Lookup and validation
// ---------------------------------------------------------------------------

/// Find one portable field by stable ID.
#[must_use]
pub fn find_field(id: &str) -> Option<&'static FieldDef> {
    ALL_FIELDS.iter().find(|field| field.id == id)
}

/// Find one tab by stable ID.
#[must_use]
pub fn find_tab(id: &str) -> Option<&'static TabDef> {
    ALL_TABS.iter().find(|tab| tab.id == id)
}

/// Find one section by stable ID.
#[must_use]
pub fn find_section(id: &str) -> Option<&'static SectionDef> {
    ALL_SECTIONS.iter().find(|section| section.id == id)
}

fn is_hex_byte(value: u8) -> bool {
    value.is_ascii_hexdigit()
}

fn valid_rgb(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 7 || bytes[0] != b'#' {
        return false;
    }
    bytes[1..].iter().all(|byte| is_hex_byte(*byte))
}

fn valid_argb(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 9 || bytes[0] != b'#' {
        return false;
    }
    bytes[1..].iter().all(|byte| is_hex_byte(*byte))
}

/// Validate one typed value against its field descriptor. Wrong-type
/// values, out-of-range numbers, malformed colors, and unknown enum
/// tokens all return [`Validity::Invalid`]; never clamps or migrates.
#[must_use]
pub fn validate_field_value(field: &FieldDef, value: &SettingValue) -> Validity {
    match (field.field_type, value) {
        (FieldType::Bool, SettingValue::Bool(_)) => Validity::Valid,
        (FieldType::Bool, _) => Validity::Invalid("needs a boolean"),
        (FieldType::Int, SettingValue::Int(got)) => {
            if let Some(min) = field.min_int
                && *got < min
            {
                return Validity::Invalid("below minimum");
            }
            if let Some(max) = field.max_int
                && *got > max
            {
                return Validity::Invalid("above maximum");
            }
            Validity::Valid
        }
        (FieldType::Int, _) => Validity::Invalid("needs an integer"),
        (FieldType::Float, SettingValue::Float(got)) => {
            if !got.is_finite() {
                return Validity::Invalid("needs a finite number");
            }
            if let Some(min) = field.min_float
                && *got < min
            {
                return Validity::Invalid("below minimum");
            }
            if let Some(max) = field.max_float
                && *got > max
            {
                return Validity::Invalid("above maximum");
            }
            Validity::Valid
        }
        (FieldType::Float, _) => Validity::Invalid("needs a number"),
        (FieldType::ColorRgb, SettingValue::Text(text)) => {
            if valid_rgb(text) {
                Validity::Valid
            } else {
                Validity::Invalid("needs #rrggbb")
            }
        }
        (FieldType::ColorRgb, _) => Validity::Invalid("needs #rrggbb"),
        (FieldType::ColorArgb, SettingValue::Text(text)) => {
            if valid_argb(text) {
                Validity::Valid
            } else {
                Validity::Invalid("needs #aarrggbb")
            }
        }
        (FieldType::ColorArgb, _) => Validity::Invalid("needs #aarrggbb"),
        (FieldType::Enum, SettingValue::Text(text)) => {
            if field.options.iter().any(|option| option.token == *text) {
                Validity::Valid
            } else {
                Validity::Invalid("unknown token")
            }
        }
        (FieldType::Enum, _) => Validity::Invalid("unknown token"),
    }
}

/// Validate one text value (enum token or color string) against its
/// field descriptor. Lets adapters stage owned runtime strings without
/// inventing static lifetimes; same refusal rules as
/// [`validate_field_value`].
#[must_use]
pub fn validate_text(field: &FieldDef, text: &str) -> Validity {
    match field.field_type {
        FieldType::ColorRgb => {
            if valid_rgb(text) {
                Validity::Valid
            } else {
                Validity::Invalid("needs #rrggbb")
            }
        }
        FieldType::ColorArgb => {
            if valid_argb(text) {
                Validity::Valid
            } else {
                Validity::Invalid("needs #aarrggbb")
            }
        }
        FieldType::Enum => {
            if field.options.iter().any(|option| option.token == text) {
                Validity::Valid
            } else {
                Validity::Invalid("unknown token")
            }
        }
        _ => Validity::Invalid("needs a non-text value"),
    }
}

/// Validate one field ID plus typed value.
#[must_use]
pub fn validate_field_id(id: &str, value: &SettingValue) -> Validity {
    match find_field(id) {
        Some(field) => validate_field_value(field, value),
        None => Validity::Invalid("unknown field"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn four_tabs_with_stable_ids() {
        let ids: Vec<&str> = ALL_TABS.iter().map(|tab| tab.id).collect();
        assert_eq!(
            ids,
            vec![
                TAB_TILING_WORKSPACES,
                TAB_APPEARANCE,
                TAB_SHORTCUTS,
                TAB_DESKTOP_INTEGRATION
            ]
        );
        let mut seen = BTreeSet::new();
        for tab in ALL_TABS {
            assert!(seen.insert(tab.id), "duplicate {}", tab.id);
            assert!(!tab.label.is_empty(), "{}", tab.id);
        }
        let labels: Vec<&str> = ALL_TABS.iter().map(|tab| tab.label).collect();
        assert_eq!(
            labels,
            vec![
                "Tiling & Workspaces",
                "Appearance",
                "Shortcuts",
                "Desktop Integration"
            ]
        );
    }

    #[test]
    fn sections_point_at_known_tabs() {
        let mut seen = BTreeSet::new();
        for section in ALL_SECTIONS {
            assert!(seen.insert(section.id), "duplicate {}", section.id);
            assert!(find_tab(section.tab).is_some(), "{}", section.id);
            assert!(!section.label.is_empty(), "{}", section.id);
        }
    }

    #[test]
    fn field_ids_unique_and_placed() {
        let mut seen = BTreeSet::new();
        for field in ALL_FIELDS {
            assert!(seen.insert(field.id), "duplicate {}", field.id);
            assert!(find_section(field.section).is_some(), "{}", field.id);
            assert!(!field.label.is_empty(), "{}", field.id);
            assert!(!field.help.is_empty(), "{}", field.id);
            assert!(!field.timing_help.is_empty(), "{}", field.id);
            assert!(!field.bounds_reason.is_empty(), "{}", field.id);
        }
        assert_eq!(ALL_FIELDS.len(), 23);
    }

    #[test]
    fn defaults_are_valid() {
        for field in ALL_FIELDS {
            assert_eq!(
                validate_field_value(field, &field.default),
                Validity::Valid,
                "{}",
                field.id
            );
        }
    }

    #[test]
    fn boundaries_refuse_outside() {
        let gaps = find_field(FIELD_INNER_GAP).expect("inner gap");
        assert_eq!(
            validate_field_value(gaps, &SettingValue::Int(-1)),
            Validity::Invalid("below minimum")
        );
        assert_eq!(
            validate_field_value(gaps, &SettingValue::Int(65)),
            Validity::Invalid("above maximum")
        );
        let width = find_field(FIELD_BORDER_WIDTH).expect("width");
        assert_eq!(
            validate_field_value(width, &SettingValue::Float(f64::NAN)),
            Validity::Invalid("needs a finite number")
        );
        assert_eq!(
            validate_field_value(width, &SettingValue::Float(33.0)),
            Validity::Invalid("above maximum")
        );
        let ext = find_field(FIELD_UNDERLAY_EXTENSION).expect("extension");
        assert_eq!(
            validate_field_value(ext, &SettingValue::Float(-2.0)),
            Validity::Invalid("below minimum")
        );
        assert_eq!(
            validate_field_value(ext, &SettingValue::Float(-1.0)),
            Validity::Valid
        );
        let color = find_field(FIELD_BORDER_COLOR).expect("color");
        assert_eq!(
            validate_field_value(color, &SettingValue::Text("#ffff")),
            Validity::Invalid("needs #rrggbb")
        );
        let argb = find_field(FIELD_UNDERLAY_COLOR).expect("argb");
        assert_eq!(
            validate_field_value(argb, &SettingValue::Text("#808080")),
            Validity::Invalid("needs #aarrggbb")
        );
    }

    #[test]
    fn numeric_defaults_sit_inside_bounds_and_edges_refuse() {
        for field in ALL_FIELDS {
            match field.field_type {
                FieldType::Int => {
                    let (min, max) = (
                        field.min_int.expect("int floor"),
                        field.max_int.expect("int ceiling"),
                    );
                    assert!(min <= max, "{}", field.id);
                    let SettingValue::Int(default) = field.default else {
                        panic!("int default {}", field.id);
                    };
                    assert!((min..=max).contains(&default), "{}", field.id);
                    assert_eq!(
                        validate_field_value(field, &SettingValue::Int(min - 1)),
                        Validity::Invalid("below minimum"),
                        "{}",
                        field.id
                    );
                    assert_eq!(
                        validate_field_value(field, &SettingValue::Int(max + 1)),
                        Validity::Invalid("above maximum"),
                        "{}",
                        field.id
                    );
                }
                FieldType::Float => {
                    let (min, max) = (
                        field.min_float.expect("float floor"),
                        field.max_float.expect("float ceiling"),
                    );
                    assert!(min <= max, "{}", field.id);
                    let SettingValue::Float(default) = field.default else {
                        panic!("float default {}", field.id);
                    };
                    assert!((min..=max).contains(&default), "{}", field.id);
                    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
                        assert_eq!(
                            validate_field_value(field, &SettingValue::Float(bad)),
                            Validity::Invalid("needs a finite number"),
                            "{}",
                            field.id
                        );
                    }
                }
                _ => {}
            }
        }
    }

    #[test]
    fn capabilities_timing_and_tab_placement_are_explicit() {
        for field in ALL_FIELDS {
            assert_eq!(field.capabilities, PORTABLE_CAPS, "caps drift {}", field.id);
            assert!(field.capabilities.windows, "{}", field.id);
            assert!(field.capabilities.kde, "{}", field.id);
            assert!(!field.capabilities.macos, "{}", field.id);
            let section = find_section(field.section).expect("section");
            assert!(
                section.tab == TAB_TILING_WORKSPACES || section.tab == TAB_APPEARANCE,
                "portable {} on native tab {}",
                field.id,
                section.tab
            );
        }
    }

    #[test]
    fn enum_options_match_core_parsers() {
        // Every table token parses through its shared core parser, options
        // are unique with functional labels, and the field default is the
        // first (default) token.
        type WireCheck = fn(&str) -> bool;
        let tables: &[(&str, WireCheck)] = &[
            (FIELD_SAME_AXIS_MOVE, |token| {
                SameAxisMove::parse_wire(token).is_some()
            }),
            (FIELD_BORN_MAXIMIZED, |token| {
                BornMaximized::parse_wire(token).is_some()
            }),
            (FIELD_FIXED_SIZE, |token| {
                FixedSizePredicate::parse_wire(token).is_some()
            }),
            (FIELD_MIGRATION_REFILL, |token| {
                MigrationSourceRefill::parse_wire(token).is_some()
            }),
            (FIELD_ANCHOR_FALLBACK, |token| {
                AnchorFallback::parse_wire(token).is_some()
            }),
            (FIELD_NO_SPACE, |token| {
                NoSpaceResult::parse_wire(token).is_some()
            }),
            (FIELD_WORKSPACE_MODE, |token| {
                workspace_scope::WORKSPACE_MODE_TOKENS.contains(&token)
            }),
            (FIELD_SWITCH_TARGET, |token| {
                SwitchTarget::parse_wire(token).is_some()
            }),
            (FIELD_FRESH_PLACEMENT, |token| {
                FreshPlacement::parse_wire(token).is_some()
            }),
        ];
        assert_eq!(tables.len(), 9);
        for (field_id, parses) in tables {
            let field = find_field(field_id).expect("enum field");
            assert!(!field.options.is_empty(), "{field_id}");
            let mut seen = BTreeSet::new();
            for option in field.options {
                assert!(seen.insert(option.token), "duplicate {}", option.token);
                assert!(!option.label.is_empty(), "{}", option.token);
                assert!(!option.tip.is_empty(), "{}", option.token);
                assert!(parses(option.token), "{}", option.token);
            }
            let SettingValue::Text(default) = field.default else {
                panic!("enum default {field_id}");
            };
            assert_eq!(default, field.options[0].token, "{field_id}");
            assert_eq!(
                validate_field_value(field, &SettingValue::Text("bogus")),
                Validity::Invalid("unknown token")
            );
        }
        let mode = find_field(FIELD_WORKSPACE_MODE).expect("mode");
        assert_eq!(mode.options.len(), 3);
    }
}

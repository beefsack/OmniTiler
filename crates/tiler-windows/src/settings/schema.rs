#![forbid(unsafe_code)]
use super::*;
use tiler_core::settings_catalog as catalog;
use tiler_core::{
    admission::{AnchorFallback, NoSpaceResult},
    output_selection::MigrationSourceRefill,
    size_hints::FixedSizePredicate,
};
use tiler_core::{born_maximized::BornMaximized, directional::SameAxisMove};

/// Settings schema version. Older readers refuse newer versions outright;
/// newer readers refuse anything but known versions (no silent migration).
pub const SETTINGS_SCHEMA_VERSION: u32 = 1;
/// Settings file name under the per-user product directory.
pub const SETTINGS_FILE_NAME: &str = "settings.json";
/// Temporary suffix used during atomic replacement (same directory).
pub const SETTINGS_PENDING_SUFFIX: &str = "settings.json.pending";
/// Bounded read: files larger than this refuse instead of loading.
pub const SETTINGS_MAX_BYTES: u64 = 65_536;

/// KDE-parity gap defaults and bounds (`domain-gap.ts`). Single source:
/// the shared portable catalog.
pub const DEFAULT_INNER_GAP: i32 = catalog::DEFAULT_GAP;
pub const DEFAULT_OUTER_GAP: i32 = catalog::DEFAULT_GAP;
pub const MAX_GAP: i32 = catalog::MAX_CATALOG_GAP;

/// Border defaults: KDE `activeborderconfig.kcfg` parity (width 3, gap 0,
/// radius 6, theme on with the configured `#2a82da` fallback). The Windows
/// analogue of the Plasma highlight colour is the live system accent; the
/// configured colour wins only when theming is off or the accent query is
/// unavailable (see [`crate::active_border::effective_color`]).
/// Single source: the shared portable catalog.
pub const DEFAULT_BORDER_WIDTH: f64 = catalog::DEFAULT_CATALOG_BORDER_WIDTH;
pub const DEFAULT_BORDER_GAP: f64 = catalog::DEFAULT_CATALOG_BORDER_GAP;
pub const DEFAULT_BORDER_RADIUS: f64 = catalog::DEFAULT_CATALOG_BORDER_RADIUS;
pub const DEFAULT_BORDER_COLOR_HEX: &str = catalog::DEFAULT_CATALOG_BORDER_COLOR;
pub const DEFAULT_BORDER_USE_THEME: bool = catalog::DEFAULT_CATALOG_BORDER_USE_THEME;
pub const MAX_BORDER_WIDTH: f64 = catalog::MAX_CATALOG_BORDER_WIDTH;
pub const MAX_BORDER_GAP: f64 = catalog::MAX_CATALOG_BORDER_GAP;
pub const MAX_BORDER_RADIUS: f64 = catalog::MAX_CATALOG_BORDER_RADIUS;

/// Underlay defaults: KDE parity (`#662a82da`, extension -1 follows border).
/// Single source: the shared portable catalog.
pub const DEFAULT_UNDERLAY_COLOR_HEX: &str = catalog::DEFAULT_CATALOG_UNDERLAY_COLOR;
pub const DEFAULT_UNDERLAY_EXTENSION: f64 = catalog::DEFAULT_CATALOG_UNDERLAY_EXTENSION;
pub const MAX_UNDERLAY_EXTENSION: f64 = catalog::MAX_CATALOG_UNDERLAY_EXTENSION;

fn default_revision() -> u64 {
    1
}

fn default_true() -> bool {
    true
}

fn default_taskbar_pager() -> bool {
    true
}

/// Closed store error vocabulary. `Invalid` carries the bounded reason; the
/// file bytes are never modified on any error path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsError {
    Io(String),
    TooLarge,
    Malformed,
    UnsupportedVersion,
    Invalid(String),
}

impl std::fmt::Display for SettingsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(_) => f.write_str("io-error"),
            Self::TooLarge => f.write_str("too-large"),
            Self::Malformed => f.write_str("malformed"),
            Self::UnsupportedVersion => f.write_str("unsupported-version"),
            Self::Invalid(reason) => write!(f, "invalid: {reason}"),
        }
    }
}

impl std::error::Error for SettingsError {}

/// R-MOV-03 same-axis move wire values (decisions 3.1/3.2, functional IDs
/// 2026-10-08). Exact tokens only: no aliases, no migration; unknown or
/// mistyped values refuse through the existing invalid-file/last-good path.
/// Single source: the shared portable catalog and core enum.
pub const SAME_AXIS_MOVE_GROUP: &str = SameAxisMove::GroupWithNeighbor.as_wire_str();
/// R-MOV-03 flat-swap alternative wire value.
pub const SAME_AXIS_MOVE_SWAP: &str = SameAxisMove::SwapWithNeighbor.as_wire_str();
/// Functional label for the default (KCM parity).
pub const SAME_AXIS_MOVE_GROUP_LABEL: &str = catalog::SAME_AXIS_MOVE_OPTIONS[0].label;
/// Reference WM named by the default's tooltip (KCM parity: never a setting id).
pub const SAME_AXIS_MOVE_GROUP_TIP: &str = catalog::SAME_AXIS_MOVE_OPTIONS[0].tip;
/// Functional label for the flat-swap alternative (KCM parity).
pub const SAME_AXIS_MOVE_SWAP_LABEL: &str = catalog::SAME_AXIS_MOVE_OPTIONS[1].label;
/// Reference WMs named by the alternative's tooltip (KCM parity).
pub const SAME_AXIS_MOVE_SWAP_TIP: &str = catalog::SAME_AXIS_MOVE_OPTIONS[1].tip;

/// R-MAX-06 born-maximized admission wire values (user 2026-10-11, option B,
/// cross-platform functional IDs `core.born_maximized`). Exact tokens only:
/// no aliases, no migration; unknown or mistyped values refuse through the
/// existing invalid-file/last-good path.
/// Single source: the shared portable catalog and core enum.
pub const BORN_MAXIMIZED_TILE: &str = BornMaximized::TileOnLaunch.as_wire_str();
/// R-MAX-06 keep-overlay alternative wire value.
pub const BORN_MAXIMIZED_OVERLAY: &str = BornMaximized::KeepMaximizedOverlay.as_wire_str();
/// Functional label for the default (KCM parity).
pub const BORN_MAXIMIZED_TILE_LABEL: &str = catalog::BORN_MAXIMIZED_OPTIONS[0].label;
/// Reference WMs named by the default's tooltip (KCM parity: never a setting id).
pub const BORN_MAXIMIZED_TILE_TIP: &str = catalog::BORN_MAXIMIZED_OPTIONS[0].tip;
/// Functional label for the keep-overlay alternative (KCM parity).
pub const BORN_MAXIMIZED_OVERLAY_LABEL: &str = catalog::BORN_MAXIMIZED_OPTIONS[1].label;
/// Reference WMs named by the alternative's tooltip (KCM parity).
pub const BORN_MAXIMIZED_OVERLAY_TIP: &str = catalog::BORN_MAXIMIZED_OPTIONS[1].tip;

/// R-SPC-04 D1 fixed-size admission predicate wire values (user 2026-10-08,
/// functional IDs delivered offline; G-05 per-axis-zero user 2026-10-10).
/// Exact tokens only: no aliases, no migration; unknown or mistyped values
/// refuse through the existing invalid-file/last-good path.
/// Single source: the shared portable catalog and core enum.
pub const FIXED_SIZE_BOTH_AXES: &str = FixedSizePredicate::BothAxes.as_wire_str();
/// R-SPC-04 D1 either-axis alternative wire value.
pub const FIXED_SIZE_EITHER_AXIS: &str = FixedSizePredicate::EitherAxis.as_wire_str();
/// Functional label for the default (KCM parity).
pub const FIXED_SIZE_BOTH_AXES_LABEL: &str = catalog::FIXED_SIZE_OPTIONS[0].label;
/// Reference WM named by the default's tooltip (KCM parity: never a setting id).
pub const FIXED_SIZE_BOTH_AXES_TIP: &str = catalog::FIXED_SIZE_OPTIONS[0].tip;
/// Functional label for the either-axis alternative (KCM parity).
pub const FIXED_SIZE_EITHER_AXIS_LABEL: &str = catalog::FIXED_SIZE_OPTIONS[1].label;
/// Reference WMs named by the alternative's tooltip (KCM parity).
pub const FIXED_SIZE_EITHER_AXIS_TIP: &str = catalog::FIXED_SIZE_OPTIONS[1].tip;

/// R-MAX-06 two-choice control rows: `(wire token, functional label, WM tooltip)`.
#[must_use]
pub const fn born_maximized_options() -> [(&'static str, &'static str, &'static str); 2] {
    [
        (
            BORN_MAXIMIZED_TILE,
            BORN_MAXIMIZED_TILE_LABEL,
            BORN_MAXIMIZED_TILE_TIP,
        ),
        (
            BORN_MAXIMIZED_OVERLAY,
            BORN_MAXIMIZED_OVERLAY_LABEL,
            BORN_MAXIMIZED_OVERLAY_TIP,
        ),
    ]
}

fn default_born_maximized() -> String {
    tiler_core::born_maximized::BornMaximized::default()
        .as_wire_str()
        .to_owned()
}

/// R-WS-12e G-37 migration source-refill wire values (user 2026-10-10,
/// functional IDs delivered offline in shared core/KDE). Exact tokens only:
/// no aliases, no migration; unknown or mistyped values refuse through the
/// existing invalid-file/last-good path.
/// Single source: the shared portable catalog and core enum.
pub const MIGRATION_REFILL_LAST_REMAINING: &str =
    MigrationSourceRefill::LastRemaining.as_wire_str();
/// R-WS-12e G-37 most-recently-used alternative wire value.
pub const MIGRATION_REFILL_MRU: &str = MigrationSourceRefill::MostRecentlyUsed.as_wire_str();
/// Functional label for the default (KCM parity).
pub const MIGRATION_REFILL_LAST_REMAINING_LABEL: &str = catalog::MIGRATION_REFILL_OPTIONS[0].label;
/// Reference WM named by the default's tooltip (KCM parity: never a setting id).
pub const MIGRATION_REFILL_LAST_REMAINING_TIP: &str = catalog::MIGRATION_REFILL_OPTIONS[0].tip;
/// Functional label for the most-recently-used alternative (KCM parity).
pub const MIGRATION_REFILL_MRU_LABEL: &str = catalog::MIGRATION_REFILL_OPTIONS[1].label;
/// Reference WMs named by the alternative's tooltip (KCM parity).
pub const MIGRATION_REFILL_MRU_TIP: &str = catalog::MIGRATION_REFILL_OPTIONS[1].tip;

/// R-MOV-03 two-choice control rows: `(wire token, functional label, WM tooltip)`.
#[must_use]
pub const fn same_axis_move_options() -> [(&'static str, &'static str, &'static str); 2] {
    [
        (
            SAME_AXIS_MOVE_GROUP,
            SAME_AXIS_MOVE_GROUP_LABEL,
            SAME_AXIS_MOVE_GROUP_TIP,
        ),
        (
            SAME_AXIS_MOVE_SWAP,
            SAME_AXIS_MOVE_SWAP_LABEL,
            SAME_AXIS_MOVE_SWAP_TIP,
        ),
    ]
}

fn default_same_axis_move() -> String {
    SAME_AXIS_MOVE_GROUP.to_owned()
}

/// R-SPC-04 D1 two-choice control rows: `(wire token, functional label, WM tooltip)`.
#[must_use]
pub const fn fixed_size_predicate_options() -> [(&'static str, &'static str, &'static str); 2] {
    [
        (
            FIXED_SIZE_BOTH_AXES,
            FIXED_SIZE_BOTH_AXES_LABEL,
            FIXED_SIZE_BOTH_AXES_TIP,
        ),
        (
            FIXED_SIZE_EITHER_AXIS,
            FIXED_SIZE_EITHER_AXIS_LABEL,
            FIXED_SIZE_EITHER_AXIS_TIP,
        ),
    ]
}

fn default_fixed_size_predicate() -> String {
    FIXED_SIZE_BOTH_AXES.to_owned()
}

/// Best-effort admission D2 anchor-fallback wire values (user 2026-10-17).
/// Exact tokens only: no aliases, no migration; unknown or mistyped values
/// refuse through the existing invalid-file/last-good path.
/// Single source: the shared portable catalog and core enum.
pub const ANCHOR_FALLBACK_LARGEST: &str = AnchorFallback::LargestSplittable.as_wire_str();
/// Best-effort admission D2 closest-relative alternative wire value.
pub const ANCHOR_FALLBACK_CLOSEST: &str = AnchorFallback::ClosestRelative.as_wire_str();
/// Best-effort admission D2 no-search alternative wire value.
pub const ANCHOR_FALLBACK_NO_SEARCH: &str = AnchorFallback::NoSearch.as_wire_str();
/// Functional label for the default (catalog parity).
pub const ANCHOR_FALLBACK_LARGEST_LABEL: &str = catalog::ANCHOR_FALLBACK_OPTIONS[0].label;
/// Tooltip note for the default (catalog parity: never a setting id).
pub const ANCHOR_FALLBACK_LARGEST_TIP: &str = catalog::ANCHOR_FALLBACK_OPTIONS[0].tip;
/// Functional label for the closest-relative alternative.
pub const ANCHOR_FALLBACK_CLOSEST_LABEL: &str = catalog::ANCHOR_FALLBACK_OPTIONS[1].label;
/// Tooltip note for the closest-relative alternative.
pub const ANCHOR_FALLBACK_CLOSEST_TIP: &str = catalog::ANCHOR_FALLBACK_OPTIONS[1].tip;
/// Functional label for the no-search alternative.
pub const ANCHOR_FALLBACK_NO_SEARCH_LABEL: &str = catalog::ANCHOR_FALLBACK_OPTIONS[2].label;
/// Tooltip note for the no-search alternative.
pub const ANCHOR_FALLBACK_NO_SEARCH_TIP: &str = catalog::ANCHOR_FALLBACK_OPTIONS[2].tip;

/// Best-effort admission D3 no-space wire values (user 2026-10-17).
/// Exact tokens only: no aliases, no migration; unknown or mistyped values
/// refuse through the existing invalid-file/last-good path.
/// Single source: the shared portable catalog and core enum.
pub const NO_SPACE_FLOAT: &str = NoSpaceResult::Float.as_wire_str();
/// Best-effort admission D3 allow-overlap alternative wire value.
pub const NO_SPACE_OVERLAP: &str = NoSpaceResult::AllowOverlap.as_wire_str();
/// Functional label for the default (catalog parity).
pub const NO_SPACE_FLOAT_LABEL: &str = catalog::NO_SPACE_OPTIONS[0].label;
/// Tooltip note for the default (catalog parity: never a setting id).
pub const NO_SPACE_FLOAT_TIP: &str = catalog::NO_SPACE_OPTIONS[0].tip;
/// Functional label for the allow-overlap alternative.
pub const NO_SPACE_OVERLAP_LABEL: &str = catalog::NO_SPACE_OPTIONS[1].label;
/// Tooltip note for the allow-overlap alternative.
pub const NO_SPACE_OVERLAP_TIP: &str = catalog::NO_SPACE_OPTIONS[1].tip;

/// Best-effort admission D2 three-choice control rows: `(wire token, functional label, note)`.
#[must_use]
pub const fn anchor_fallback_options() -> [(&'static str, &'static str, &'static str); 3] {
    [
        (
            ANCHOR_FALLBACK_LARGEST,
            ANCHOR_FALLBACK_LARGEST_LABEL,
            ANCHOR_FALLBACK_LARGEST_TIP,
        ),
        (
            ANCHOR_FALLBACK_CLOSEST,
            ANCHOR_FALLBACK_CLOSEST_LABEL,
            ANCHOR_FALLBACK_CLOSEST_TIP,
        ),
        (
            ANCHOR_FALLBACK_NO_SEARCH,
            ANCHOR_FALLBACK_NO_SEARCH_LABEL,
            ANCHOR_FALLBACK_NO_SEARCH_TIP,
        ),
    ]
}

fn default_anchor_fallback() -> String {
    ANCHOR_FALLBACK_LARGEST.to_owned()
}

/// Best-effort admission D3 two-choice control rows: `(wire token, functional label, note)`.
#[must_use]
pub const fn no_space_options() -> [(&'static str, &'static str, &'static str); 2] {
    [
        (NO_SPACE_FLOAT, NO_SPACE_FLOAT_LABEL, NO_SPACE_FLOAT_TIP),
        (
            NO_SPACE_OVERLAP,
            NO_SPACE_OVERLAP_LABEL,
            NO_SPACE_OVERLAP_TIP,
        ),
    ]
}

fn default_no_space() -> String {
    NO_SPACE_FLOAT.to_owned()
}

/// R-WS-12e G-37 two-choice control rows: `(wire token, functional label, WM tooltip)`.
#[must_use]
pub const fn migration_source_refill_options() -> [(&'static str, &'static str, &'static str); 2] {
    [
        (
            MIGRATION_REFILL_LAST_REMAINING,
            MIGRATION_REFILL_LAST_REMAINING_LABEL,
            MIGRATION_REFILL_LAST_REMAINING_TIP,
        ),
        (
            MIGRATION_REFILL_MRU,
            MIGRATION_REFILL_MRU_LABEL,
            MIGRATION_REFILL_MRU_TIP,
        ),
    ]
}

fn default_migration_source_refill() -> String {
    MIGRATION_REFILL_LAST_REMAINING.to_owned()
}

/// Item 12 workspace scope mode wire values (shared core/KDE
/// `workspace-native.ts` parity: `per-output-local` default, `global-unique`,
/// `shared`). Exact tokens only: no aliases, no migration; unknown or
/// mistyped values refuse through the existing invalid-file/last-good path.
/// Single source: the shared core workspace token constants and catalog.
pub const WORKSPACE_MODE_PER_OUTPUT_LOCAL: &str =
    tiler_core::workspace_scope::WORKSPACE_MODE_TOKENS[0];
/// Item 12 global-unique alternative wire value.
pub const WORKSPACE_MODE_GLOBAL_UNIQUE: &str =
    tiler_core::workspace_scope::WORKSPACE_MODE_TOKENS[1];
/// Item 12 shared alternative wire value.
pub const WORKSPACE_MODE_SHARED: &str = tiler_core::workspace_scope::WORKSPACE_MODE_TOKENS[2];
/// Functional label for the default.
pub const WORKSPACE_MODE_PER_OUTPUT_LOCAL_LABEL: &str = catalog::WORKSPACE_MODE_OPTIONS[0].label;
/// Functional label for the global-unique alternative.
pub const WORKSPACE_MODE_GLOBAL_UNIQUE_LABEL: &str = catalog::WORKSPACE_MODE_OPTIONS[1].label;
/// Functional label for the shared alternative.
pub const WORKSPACE_MODE_SHARED_LABEL: &str = catalog::WORKSPACE_MODE_OPTIONS[2].label;
/// Functional scope note for the default (no WM reference established).
pub const WORKSPACE_MODE_PER_OUTPUT_LOCAL_TIP: &str = catalog::WORKSPACE_MODE_OPTIONS[0].tip;
/// Functional scope note for the global-unique alternative.
pub const WORKSPACE_MODE_GLOBAL_UNIQUE_TIP: &str = catalog::WORKSPACE_MODE_OPTIONS[1].tip;
/// Functional scope note for the shared alternative.
pub const WORKSPACE_MODE_SHARED_TIP: &str = catalog::WORKSPACE_MODE_OPTIONS[2].tip;

/// Item 12 three-choice control rows: `(wire token, functional label, scope note)`.
#[must_use]
pub const fn workspace_mode_options() -> [(&'static str, &'static str, &'static str); 3] {
    [
        (
            WORKSPACE_MODE_PER_OUTPUT_LOCAL,
            WORKSPACE_MODE_PER_OUTPUT_LOCAL_LABEL,
            WORKSPACE_MODE_PER_OUTPUT_LOCAL_TIP,
        ),
        (
            WORKSPACE_MODE_GLOBAL_UNIQUE,
            WORKSPACE_MODE_GLOBAL_UNIQUE_LABEL,
            WORKSPACE_MODE_GLOBAL_UNIQUE_TIP,
        ),
        (
            WORKSPACE_MODE_SHARED,
            WORKSPACE_MODE_SHARED_LABEL,
            WORKSPACE_MODE_SHARED_TIP,
        ),
    ]
}

fn default_workspace_mode() -> String {
    tiler_core::workspace_scope::mode_token(tiler_core::workspace_scope::DEFAULT_WORKSPACE_MODE)
        .to_owned()
}

/// Core tiling/visual/takeover settings. All values are validated on load
/// and on save; out-of-range values refuse instead of clamping.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CoreSettings {
    #[serde(default = "default_inner_gap")]
    pub inner_gap: i32,
    #[serde(default = "default_outer_gap")]
    pub outer_gap: i32,
    /// R-MOV-03 global same-axis move mode. Additive schema-v1 field: a
    /// missing value defaults to [`SAME_AXIS_MOVE_GROUP`]; any other value
    /// besides the two exact tokens refuses in [`validate_settings`].
    #[serde(default = "default_same_axis_move")]
    pub same_axis_move: String,
    /// R-MAX-06 born-maximized admission mode. Additive schema-v1 field: a
    /// missing value defaults to [`BORN_MAXIMIZED_TILE`]; any other value
    /// besides the two exact tokens refuses in [`validate_settings`].
    #[serde(default = "default_born_maximized")]
    pub born_maximized: String,
    /// R-SPC-04 D1 fixed-size admission predicate. Additive schema-v1 field:
    /// a missing value defaults to [`FIXED_SIZE_BOTH_AXES`]; any other value
    /// besides the two exact tokens refuses in [`validate_settings`].
    /// Changes apply to subsequent admissions only, never to retained
    /// classifications (D2).
    #[serde(default = "default_fixed_size_predicate")]
    pub fixed_size_predicate: String,
    /// Best-effort admission D2 anchor fallback. Additive schema-v1 field:
    /// a missing value defaults to [`ANCHOR_FALLBACK_LARGEST`]; any other
    /// value besides the three exact tokens refuses in [`validate_settings`].
    /// Changes apply to subsequent capacity decisions only, never to
    /// retained topology (D10).
    #[serde(default = "default_anchor_fallback")]
    pub anchor_fallback: String,
    /// Best-effort admission D3 no-space result. Additive schema-v1 field:
    /// a missing value defaults to [`NO_SPACE_FLOAT`]; any other value
    /// besides the two exact tokens refuses in [`validate_settings`].
    /// Changes apply to subsequent admission or actual capacity-repair
    /// decisions only, never to retained tiles/floats (D10).
    #[serde(default = "default_no_space")]
    pub no_space_result: String,
    /// R-WS-12e G-37 source workspace after migration. Additive schema-v1
    /// field: a missing value defaults to
    /// [`MIGRATION_REFILL_LAST_REMAINING`]; any other value besides the two
    /// exact tokens refuses in [`validate_settings`]. Changes apply to
    /// subsequent migrations only, never to an already-migrated workspace.
    #[serde(default = "default_migration_source_refill")]
    pub migration_source_refill: String,
    #[serde(default)]
    pub border: BorderSettings,
    #[serde(default)]
    pub underlay: UnderlaySettings,
    #[serde(default)]
    pub keyboard: KeyboardSettings,
    #[serde(default)]
    pub mouse: MouseSettings,
    #[serde(default)]
    pub workspace: WorkspaceSettings,
    /// Taskbar pager (per-output workspace strip overlaid in the taskbar).
    /// Additive schema-v1 field: missing defaults on; the Settings window
    /// surfaces the opt-out and live edits hot-apply.
    #[serde(default = "default_taskbar_pager")]
    pub taskbar_pager: bool,
}

fn default_inner_gap() -> i32 {
    DEFAULT_INNER_GAP
}

fn default_outer_gap() -> i32 {
    DEFAULT_OUTER_GAP
}

impl Default for CoreSettings {
    fn default() -> Self {
        Self {
            inner_gap: DEFAULT_INNER_GAP,
            outer_gap: DEFAULT_OUTER_GAP,
            same_axis_move: default_same_axis_move(),
            born_maximized: default_born_maximized(),
            fixed_size_predicate: default_fixed_size_predicate(),
            anchor_fallback: default_anchor_fallback(),
            no_space_result: default_no_space(),
            migration_source_refill: default_migration_source_refill(),
            border: BorderSettings::default(),
            underlay: UnderlaySettings::default(),
            keyboard: KeyboardSettings::default(),
            mouse: MouseSettings::default(),
            workspace: WorkspaceSettings::default(),
            taskbar_pager: default_taskbar_pager(),
        }
    }
}

impl CoreSettings {
    /// Typed R-MOV-03 mode for the next move. Live settings are always
    /// validated, so the fallback only covers hand-built defaults; invalid
    /// files never become live (they keep last-good instead).
    #[must_use]
    pub fn same_axis_move_mode(&self) -> tiler_core::directional::SameAxisMove {
        tiler_core::directional::SameAxisMove::parse_wire(&self.same_axis_move).unwrap_or_default()
    }

    /// Typed R-MAX-06 mode for the next first-seen maximized admission.
    /// Shared policy only: the exact tokens parse through
    /// [`tiler_core::born_maximized::BornMaximized::parse_wire`]. Live
    /// settings are always validated, so the fallback only covers hand-built
    /// defaults; invalid files never become live (they keep last-good
    /// instead).
    #[must_use]
    pub fn born_maximized_mode(&self) -> tiler_core::born_maximized::BornMaximized {
        tiler_core::born_maximized::BornMaximized::parse_wire(&self.born_maximized)
            .unwrap_or_default()
    }

    /// Typed R-MAX-06 mode for the next first-seen maximized admission: true
    /// keeps the Q3 retained-slot overlay, false clears once then admits
    /// ordinarily. Live settings are always validated, so the fallback only
    /// covers hand-built defaults; invalid files never become live (they keep
    /// last-good instead).
    #[must_use]
    pub fn born_maximized_keep_overlay(&self) -> bool {
        !self.born_maximized_mode().should_clear_at_admission()
    }

    /// Typed R-SPC-04 D1 predicate for subsequent fixed-size admissions.
    /// Shared policy only: the exact tokens parse through
    /// [`tiler_core::size_hints::FixedSizePredicate::parse_wire`], so
    /// per-axis-zero handling (G-05) is never copied here. Live settings are
    /// always validated, so the fallback only covers hand-built defaults;
    /// invalid files never become live (they keep last-good instead).
    #[must_use]
    pub fn fixed_size_predicate_mode(&self) -> tiler_core::size_hints::FixedSizePredicate {
        tiler_core::size_hints::FixedSizePredicate::parse_wire(&self.fixed_size_predicate)
            .unwrap_or_default()
    }

    /// Typed best-effort admission D2 fallback for subsequent capacity
    /// decisions. Shared policy only: the exact tokens parse through
    /// [`tiler_core::admission::AnchorFallback::parse_wire`]. Live settings
    /// are always validated, so the fallback only covers hand-built
    /// defaults; invalid files never become live (they keep last-good
    /// instead).
    #[must_use]
    pub fn anchor_fallback_mode(&self) -> tiler_core::admission::AnchorFallback {
        tiler_core::admission::AnchorFallback::parse_wire(&self.anchor_fallback).unwrap_or_default()
    }

    /// Typed best-effort admission D3 result for subsequent admission or
    /// actual capacity-repair decisions. Shared policy only: the exact
    /// tokens parse through [`tiler_core::admission::NoSpaceResult::parse_wire`].
    /// Live settings are always validated, so the fallback only covers
    /// hand-built defaults; invalid files never become live (they keep
    /// last-good instead).
    #[must_use]
    pub fn no_space_result_mode(&self) -> tiler_core::admission::NoSpaceResult {
        tiler_core::admission::NoSpaceResult::parse_wire(&self.no_space_result).unwrap_or_default()
    }

    /// Typed R-WS-12e G-37 source-refill mode for subsequent whole-workspace
    /// migrations. Shared policy only: the exact tokens parse through
    /// [`tiler_core::output_selection::MigrationSourceRefill::parse_wire`],
    /// so MRU eligibility (pre-mutation item-1.2 previous id, surviving
    /// empties valid, migrated/removed/out-of-scope fallback) is never copied
    /// here. Live settings are always validated, so the fallback only covers
    /// hand-built defaults; invalid files never become live (they keep
    /// last-good instead).
    #[must_use]
    pub fn migration_source_refill_mode(
        &self,
    ) -> tiler_core::output_selection::MigrationSourceRefill {
        tiler_core::output_selection::MigrationSourceRefill::parse_wire(
            &self.migration_source_refill,
        )
        .unwrap_or_default()
    }
}

/// Active-border settings. Logical KDE points; the native layer scales at
/// the target DPI. Colour is `#rrggbb`; `use_theme` selects the live system
/// accent with the configured colour as fallback.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BorderSettings {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_border_width")]
    pub width: f64,
    #[serde(default)]
    pub gap: f64,
    #[serde(default = "default_border_radius")]
    pub radius: f64,
    #[serde(default = "default_border_color")]
    pub color: String,
    #[serde(default = "default_border_theme")]
    pub use_theme: bool,
}

fn default_border_width() -> f64 {
    DEFAULT_BORDER_WIDTH
}

fn default_border_radius() -> f64 {
    DEFAULT_BORDER_RADIUS
}

fn default_border_color() -> String {
    DEFAULT_BORDER_COLOR_HEX.to_owned()
}

fn default_border_theme() -> bool {
    DEFAULT_BORDER_USE_THEME
}

impl Default for BorderSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            width: DEFAULT_BORDER_WIDTH,
            gap: DEFAULT_BORDER_GAP,
            radius: DEFAULT_BORDER_RADIUS,
            color: DEFAULT_BORDER_COLOR_HEX.to_owned(),
            use_theme: DEFAULT_BORDER_USE_THEME,
        }
    }
}

/// Group-underlay settings. Colour is `#aarrggbb`; extension is the logical
/// outset beyond gap plus border width (-1 follows the border width).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnderlaySettings {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_underlay_color")]
    pub color: String,
    #[serde(default = "default_underlay_extension")]
    pub extension: f64,
}

fn default_underlay_color() -> String {
    DEFAULT_UNDERLAY_COLOR_HEX.to_owned()
}

fn default_underlay_extension() -> f64 {
    DEFAULT_UNDERLAY_EXTENSION
}

impl Default for UnderlaySettings {
    fn default() -> Self {
        Self {
            enabled: true,
            color: DEFAULT_UNDERLAY_COLOR_HEX.to_owned(),
            extension: DEFAULT_UNDERLAY_EXTENSION,
        }
    }
}

/// Keyboard takeover policy. `takeover` defaults on; `allow_win_l` is the
/// explicit opt-in for unshifted Win+L (default off). The physical OS lock
/// chord keeps its safety fence regardless of rebinding (see
/// [`is_lock_chord`]): no remap may target unshifted Win+L.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyboardSettings {
    #[serde(default = "default_true")]
    pub takeover: bool,
    #[serde(default)]
    pub allow_win_l: bool,
}

impl Default for KeyboardSettings {
    fn default() -> Self {
        Self {
            takeover: true,
            allow_win_l: false,
        }
    }
}

/// Mouse settings. Session-only Snap prevention defaults on; turning it off
/// restores the captured owned SPI preimage (existing ledger/conditional
/// write/readback path), never registry/policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MouseSettings {
    #[serde(default = "default_true")]
    pub snap_prevention: bool,
}

impl Default for MouseSettings {
    fn default() -> Self {
        Self {
            snap_prevention: true,
        }
    }
}

/// Managed-workspace settings. `default_tiled` (KDE `defaultTiled` parity,
/// initially true) seeds every newly created workspace; live edits affect
/// only workspaces created after the edit. Per-workspace session overrides
/// never persist.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceSettings {
    #[serde(default = "default_true")]
    pub default_tiled: bool,
    /// Item 12 workspace scope mode. Additive schema-v1 field: a missing
    /// value defaults to [`WORKSPACE_MODE_PER_OUTPUT_LOCAL`]; any other value
    /// besides the three exact tokens refuses in [`validate_settings`].
    /// Stage A persists the choice only; the running owner keeps its startup
    /// scope until restart (stage B adopts and reports restart-required).
    #[serde(default = "default_workspace_mode")]
    pub mode: String,
}

impl Default for WorkspaceSettings {
    fn default() -> Self {
        Self {
            default_tiled: true,
            mode: default_workspace_mode(),
        }
    }
}

impl WorkspaceSettings {
    /// Typed item-12 scope mode for stage B startup adoption. Live settings
    /// are always validated, so the fallback only covers hand-built defaults;
    /// invalid files never become live (they keep last-good instead). The
    /// explicit match never uses
    /// [`tiler_core::workspace_scope::parse_workspace_mode`] (which silently
    /// defaults unknown values); unknown saved values refuse in validation.
    #[must_use]
    pub fn mode_enum(&self) -> tiler_core::workspace_scope::WorkspaceMode {
        match self.mode.as_str() {
            s if s == WORKSPACE_MODE_GLOBAL_UNIQUE => {
                tiler_core::workspace_scope::WorkspaceMode::GlobalUnique
            }
            s if s == WORKSPACE_MODE_SHARED => tiler_core::workspace_scope::WorkspaceMode::Shared,
            _ => tiler_core::workspace_scope::WorkspaceMode::PerOutputLocal,
        }
    }
}

/// Per-binding state. `Keep` uses the catalog default chord(s); `Disabled`
/// passes the chord through natively; `Rebind` replaces the default with a
/// validated canonical Win-family chord. The custom chord travels in
/// `chord` only for `Rebind`; any other combination refuses validation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BindingState {
    Keep,
    Disabled,
    Rebind,
}

/// One persisted binding override. Absent entries mean `Keep`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BindingSetting {
    pub state: BindingState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chord: Option<String>,
}

/// Full settings document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    pub v: u32,
    #[serde(default = "default_revision")]
    pub revision: u64,
    #[serde(default)]
    pub core: CoreSettings,
    /// Binding overrides by catalog action id. Absent means `Keep`.
    #[serde(default)]
    pub bindings: BTreeMap<String, BindingSetting>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            v: SETTINGS_SCHEMA_VERSION,
            revision: 1,
            core: CoreSettings::default(),
            bindings: BTreeMap::new(),
        }
    }
}

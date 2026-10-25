//! KDE adapter over the shared portable catalog.
//!
//! Maps catalog fields to their unchanged KDE storage keys, widget bindings
//! and KDE-native presentation strings. The catalog owns every value the
//! exporter emits: defaults, bounds, enum tokens and enum order. The adapter
//! owns only KDE placement (keys, widgets, native row order), KDE-native
//! labels/tooltips where they genuinely differ, and the explicit
//! unsupported/schema-only/native-only sets. Shared catalog labels are never
//! substituted where KDE differs. This adapter stages no values and invokes
//! no native operations (no Fix/Force/Revert, no shortcut apply); it only
//! describes mappings for the development-only exporter.
//!
//! Coverage: 16 of the 23 portable fields have KCM controls. Six portable
//! fields are explicitly unsupported on KDE (no KConfig key, no widget):
//! `border.enabled`, `underlay.enabled`, plus the four output-targeting
//! fields `output.switch-target`, `output.fresh-placement`,
//! `output.focus-follows-cursor` and `output.cursor-warp`, which the KDE
//! runtime does not expose today (fixed focused-output/last-focused
//! targeting, false cursor toggles). One portable field,
//! `workspace.default-tiled`, is schema-only on KDE (persisted in
//! `main.xml`, consumed by the tray/script; intentionally no KCM control).
//! Two entries are KDE-native-only and stay out of the catalog: the
//! `DragPreviewColor` kcfg entry and the hidden `shortcutProfile` schema
//! entry. Shared catalog metadata is unchanged.

use tiler_core::settings_catalog::{self, EnumOption, FieldDef};

// ---------------------------------------------------------------------------
// Config groups (unchanged KDE storage locations)
// ---------------------------------------------------------------------------

/// Script settings group in `kwinrc`.
pub const GROUP_SCRIPT: &str = "Script-omnitiler-kwin";
/// Effect settings group in `kwinrc` (KConfigXT-managed).
pub const GROUP_EFFECT: &str = "Effect-omnitiler-active-border";

// ---------------------------------------------------------------------------
// Tabs (native order and titles preserved)
// ---------------------------------------------------------------------------

/// One KDE settings tab: catalog tab id plus native page widget and title.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KdeTab {
    pub tab_id: &'static str,
    pub page_widget: &'static str,
    /// Display title. The tiling tab is stored escaped in the `.ui`
    /// (`Tiling &amp;&amp; Workspaces`); see
    /// [`TILING_TAB_UI_SOURCE`].
    pub native_title: &'static str,
}

/// Escaped tiling-tab title as it appears in `unifiedsettings.ui`.
pub const TILING_TAB_UI_SOURCE: &str = "Tiling &amp;&amp; Workspaces";

/// All four tabs in native page order.
pub const KDE_TABS: &[KdeTab] = &[
    KdeTab {
        tab_id: settings_catalog::TAB_TILING_WORKSPACES,
        page_widget: "tilingTab",
        native_title: "Tiling && Workspaces",
    },
    KdeTab {
        tab_id: settings_catalog::TAB_APPEARANCE,
        page_widget: "appearanceTab",
        native_title: "Appearance",
    },
    KdeTab {
        tab_id: settings_catalog::TAB_SHORTCUTS,
        page_widget: "shortcutsTab",
        native_title: "Shortcuts",
    },
    KdeTab {
        tab_id: settings_catalog::TAB_DESKTOP_INTEGRATION,
        page_widget: "desktopTab",
        native_title: "Desktop Integration",
    },
];

// ---------------------------------------------------------------------------
// Exposed field mappings in native UI row order (exactly once each)
// ---------------------------------------------------------------------------

/// One exposed KDE field: catalog field plus unchanged storage key, widget
/// binding and KDE-native presentation strings. Defaults, bounds, enum
/// tokens and enum order always come from the catalog; only labels, tips,
/// keys, widgets and row order live here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KdeField {
    pub field_id: &'static str,
    pub group: &'static str,
    pub key: &'static str,
    pub widget: &'static str,
    /// Schema type: `Int`/`Double`/`Bool`/`Color`/`Enum` as written in the
    /// corresponding `.kcfg`/script schema file.
    pub schema_type: &'static str,
    /// Label as written in the schema (kcfg entry label / script schema
    /// label, no trailing colon).
    pub schema_label: &'static str,
    /// Script-schema tooltip; empty where the schema declares none and for
    /// all effect fields.
    pub schema_tooltip: &'static str,
    /// Exact widget text in `unifiedsettings.ui` (colon-terminated labels).
    /// Kept separate so schema labels never leak into visible dialog text.
    pub ui_label: &'static str,
}

/// One native enum presentation override, indexed by stable catalog token.
/// Only genuinely different rows live here; coincident labels/tips flow
/// from the catalog. The catalog still owns token order and defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KdeEnumOverride {
    pub field_id: &'static str,
    pub token: &'static str,
    pub label: &'static str,
    /// Empty where the KCM sets no tooltip for this row.
    pub tip: &'static str,
}

/// Native workspace-mode presentation: labels and tips genuinely differ
/// from the catalog (`Per output, local` vs `Per-output local`, KWin
/// version notes vs scope notes).
pub const KDE_ENUM_OVERRIDES: &[KdeEnumOverride] = &[
    KdeEnumOverride {
        field_id: settings_catalog::FIELD_WORKSPACE_MODE,
        token: "per-output-local",
        label: "Per output, local",
        tip: "Requires KWin 6.7 or later; KWin 6.6 uses shared",
    },
    KdeEnumOverride {
        field_id: settings_catalog::FIELD_WORKSPACE_MODE,
        token: "shared",
        label: "Shared",
        tip: "",
    },
];

/// Native override for one catalog enum option, if any.
#[must_use]
pub fn kde_enum_override(field_id: &str, token: &str) -> Option<&'static KdeEnumOverride> {
    KDE_ENUM_OVERRIDES
        .iter()
        .find(|entry| entry.field_id == field_id && entry.token == token)
}

/// Resolved presentation text for one catalog enum option: the native
/// override where one exists, otherwise the catalog label/tip.
#[must_use]
pub fn kde_option_text(field_id: &str, option: &EnumOption) -> (&'static str, &'static str) {
    match kde_enum_override(field_id, option.token) {
        Some(entry) => (entry.label, entry.tip),
        None => (option.label, option.tip),
    }
}

/// KDE-withheld catalog tokens: valid in the shared catalog (and on other
/// platforms) but offered by no KDE surface. KDE cannot present globally
/// unique numbers because desktop positions and names are global; a
/// persisted `global-unique` value falls back to `per-output-local` on the
/// world route with a logged reason (Tentative, user review pending).
pub const KDE_WITHHELD_OPTIONS: &[(&str, &str)] =
    &[(settings_catalog::FIELD_WORKSPACE_MODE, "global-unique")];

/// Catalog enum options actually offered on KDE: catalog order minus the
/// withheld tokens above. Emitters must use this (never raw
/// `def.options`) so schema, UI and descriptors agree.
#[must_use]
pub fn kde_exposed_options(field_id: &str) -> Vec<&'static EnumOption> {
    let Some(def) = settings_catalog::find_field(field_id) else {
        return Vec::new();
    };
    def.options
        .iter()
        .filter(|option| {
            !KDE_WITHHELD_OPTIONS.iter().any(|(held_field, held_token)| {
                *held_field == field_id && *held_token == option.token
            })
        })
        .collect()
}

/// Every KDE-exposed portable field in native UI row order (16 entries:
/// script rows as laid out in the Tiling & Workspaces form, then effect
/// rows as laid out in the Appearance form).
pub const KDE_EXPOSED_FIELDS: &[KdeField] = &[
    KdeField {
        field_id: settings_catalog::FIELD_WORKSPACE_MODE,
        group: GROUP_SCRIPT,
        key: "workspaceMode",
        widget: "workspaceModeCombo",
        schema_type: "Enum",
        schema_label: "The multi-output workspace model",
        schema_tooltip: "Per-output modes need KWin 6.7 or later. On KWin 6.6 the tiler uses the shared model and logs the fallback.",
        ui_label: "The multi-output workspace model (requires session restart; per-output needs KWin 6.7+, 6.6 uses shared):",
    },
    KdeField {
        field_id: settings_catalog::FIELD_SAME_AXIS_MOVE,
        group: GROUP_SCRIPT,
        key: "sameAxisMove",
        widget: "sameAxisMoveCombo",
        schema_type: "Enum",
        schema_label: "The same-axis move behavior",
        schema_tooltip: "",
        ui_label: "Same-axis move behavior:",
    },
    KdeField {
        field_id: settings_catalog::FIELD_FIXED_SIZE,
        group: GROUP_SCRIPT,
        key: "fixedSizePredicate",
        widget: "fixedSizePredicateCombo",
        schema_type: "Enum",
        schema_label: "The fixed-size admission predicate",
        schema_tooltip: "",
        ui_label: "Fixed-size admission predicate:",
    },
    KdeField {
        field_id: settings_catalog::FIELD_BORN_MAXIMIZED,
        group: GROUP_SCRIPT,
        key: "bornMaximized",
        widget: "bornMaximizedCombo",
        schema_type: "Enum",
        schema_label: "The born-maximized admission behavior",
        schema_tooltip: "",
        ui_label: "Born-maximized admission behavior:",
    },
    KdeField {
        field_id: settings_catalog::FIELD_ANCHOR_FALLBACK,
        group: GROUP_SCRIPT,
        key: "anchorFallback",
        widget: "anchorFallbackCombo",
        schema_type: "Enum",
        schema_label: "When the anchor cannot split",
        schema_tooltip: "",
        ui_label: "When the anchor cannot split:",
    },
    KdeField {
        field_id: settings_catalog::FIELD_NO_SPACE,
        group: GROUP_SCRIPT,
        key: "noSpaceResult",
        widget: "noSpaceResultCombo",
        schema_type: "Enum",
        schema_label: "When no split is available",
        schema_tooltip: "",
        ui_label: "When no split is available:",
    },
    KdeField {
        field_id: settings_catalog::FIELD_MIGRATION_REFILL,
        group: GROUP_SCRIPT,
        key: "migrationSourceRefill",
        widget: "migrationSourceRefillCombo",
        schema_type: "Enum",
        schema_label: "Source workspace after migration",
        schema_tooltip: "",
        ui_label: "Source workspace after migration:",
    },
    KdeField {
        field_id: settings_catalog::FIELD_INNER_GAP,
        group: GROUP_SCRIPT,
        key: "innerGap",
        widget: "innerGapSpinBox",
        schema_type: "Int",
        schema_label: "Gap between tiled windows",
        schema_tooltip: "",
        ui_label: "Gap between tiled windows:",
    },
    KdeField {
        field_id: settings_catalog::FIELD_OUTER_GAP,
        group: GROUP_SCRIPT,
        key: "outerGap",
        widget: "outerGapSpinBox",
        schema_type: "Int",
        schema_label: "Gap between tiled windows and the work area edge",
        schema_tooltip: "",
        ui_label: "Gap between tiled windows and the work area edge:",
    },
    KdeField {
        field_id: settings_catalog::FIELD_BORDER_COLOR,
        group: GROUP_EFFECT,
        key: "BorderColor",
        widget: "kcfg_BorderColor",
        schema_type: "Color",
        schema_label: "Fallback border color",
        schema_tooltip: "",
        ui_label: "Fallback border color:",
    },
    KdeField {
        field_id: settings_catalog::FIELD_BORDER_WIDTH,
        group: GROUP_EFFECT,
        key: "BorderWidth",
        widget: "kcfg_BorderWidth",
        schema_type: "Double",
        schema_label: "Border width",
        schema_tooltip: "",
        ui_label: "Border width:",
    },
    KdeField {
        field_id: settings_catalog::FIELD_BORDER_RADIUS,
        group: GROUP_EFFECT,
        key: "BorderRadius",
        widget: "kcfg_BorderRadius",
        schema_type: "Double",
        schema_label: "Outline radius",
        schema_tooltip: "",
        ui_label: "Outline radius:",
    },
    KdeField {
        field_id: settings_catalog::FIELD_BORDER_GAP,
        group: GROUP_EFFECT,
        key: "BorderGap",
        widget: "kcfg_BorderGap",
        schema_type: "Double",
        schema_label: "Gap from the window",
        schema_tooltip: "",
        ui_label: "Gap from the window:",
    },
    KdeField {
        field_id: settings_catalog::FIELD_BORDER_USE_THEME,
        group: GROUP_EFFECT,
        key: "UseThemeColor",
        widget: "kcfg_UseThemeColor",
        schema_type: "Bool",
        schema_label: "Use theme highlight color when available",
        schema_tooltip: "",
        ui_label: "Use theme highlight color when available",
    },
    KdeField {
        field_id: settings_catalog::FIELD_UNDERLAY_COLOR,
        group: GROUP_EFFECT,
        key: "GroupUnderlayColor",
        widget: "kcfg_GroupUnderlayColor",
        schema_type: "Color",
        schema_label: "Group underlay color",
        schema_tooltip: "",
        ui_label: "Group underlay color:",
    },
    KdeField {
        field_id: settings_catalog::FIELD_UNDERLAY_EXTENSION,
        group: GROUP_EFFECT,
        key: "GroupUnderlayExtension",
        widget: "kcfg_GroupUnderlayExtension",
        schema_type: "Double",
        schema_label: "Group underlay extension",
        schema_tooltip: "",
        ui_label: "Group underlay extension:",
    },
];

// ---------------------------------------------------------------------------
// Explicit exclusions, schema-only fields and KDE-native-only entries
// ---------------------------------------------------------------------------

/// Portable fields with no KDE control (no KConfig key, no widget).
/// Adapter-only exclusion list; shared capabilities are unchanged. The four
/// output-targeting entries stay catalog-portable but have no KCM control
/// or schema key on KDE today (not a Stage 6 control decision).
pub const KDE_UNSUPPORTED_FIELDS: &[&str] = &[
    settings_catalog::FIELD_BORDER_ENABLED,
    settings_catalog::FIELD_UNDERLAY_ENABLED,
    settings_catalog::FIELD_SWITCH_TARGET,
    settings_catalog::FIELD_FRESH_PLACEMENT,
    settings_catalog::FIELD_FOCUS_FOLLOWS_CURSOR,
    settings_catalog::FIELD_CURSOR_WARP,
];

/// Portable field kept in the script schema without a KCM control
/// (tray persist + script read for newly discovered workspaces).
pub const KDE_SCHEMA_ONLY_FIELD: &str = settings_catalog::FIELD_WORKSPACE_DEFAULT_TILED;
/// Its storage key in the script schema.
pub const KDE_SCHEMA_ONLY_KEY: &str = "defaultTiled";
/// Its schema label (no KCM widget shows this text).
pub const KDE_SCHEMA_ONLY_LABEL: &str = "Tiled behavior for newly discovered workspaces";

/// KDE-only kcfg entry retained verbatim (never in the catalog).
pub const NATIVE_ONLY_KCFG_KEY: &str = "DragPreviewColor";
/// KDE-only kcfg default retained verbatim.
pub const NATIVE_ONLY_KCFG_DEFAULT: &str = "#402a82da";
/// KDE-only kcfg label retained verbatim.
pub const NATIVE_ONLY_KCFG_LABEL: &str = "Drop-target preview color";

/// Hidden KDE-only script schema entry retained verbatim.
pub const NATIVE_ONLY_SCHEMA_KEY: &str = "shortcutProfile";
/// Hidden KDE-only schema label retained verbatim.
pub const NATIVE_ONLY_SCHEMA_LABEL: &str = "The startup shortcut profile";
/// Hidden KDE-only schema default retained verbatim.
pub const NATIVE_ONLY_SCHEMA_DEFAULT: &str = "cosmic";
/// Hidden KDE-only schema choices retained verbatim.
pub const NATIVE_ONLY_SCHEMA_CHOICES: &[&str] = &["cosmic", "hyprland", "bspwm"];

// ---------------------------------------------------------------------------
// Native panel slots (metadata only; no operation setters)
// ---------------------------------------------------------------------------

/// Panel placement on a tab.
pub const PLACEMENT_TAB: &str = "tab";
/// Panel placement below the tab strip (outside every tab).
pub const PLACEMENT_BELOW_TABS: &str = "below-tabs";

/// One native panel: explicit KDE-owned controls. Ordinary field staging
/// and Save never invoke these panels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KdePanel {
    pub panel_id: &'static str,
    /// Empty when the panel sits outside every tab (see [`PLACEMENT_BELOW_TABS`]).
    pub tab_id: &'static str,
    pub placement: &'static str,
    pub widget: &'static str,
}

/// Native shortcut/conflict panels on the Shortcuts tab, the window-edge
/// conflict rows on Desktop Integration, and the persistent script status
/// group below the tab strip (honestly placed outside every tab).
pub const KDE_NATIVE_PANELS: &[KdePanel] = &[
    KdePanel {
        panel_id: "native.shortcut-status",
        tab_id: settings_catalog::TAB_SHORTCUTS,
        placement: PLACEMENT_TAB,
        widget: "shortcutStatusLabel",
    },
    KdePanel {
        panel_id: "native.shortcut-error",
        tab_id: settings_catalog::TAB_SHORTCUTS,
        placement: PLACEMENT_TAB,
        widget: "shortcutErrorLabel",
    },
    KdePanel {
        panel_id: "native.shortcut-conflicts",
        tab_id: settings_catalog::TAB_SHORTCUTS,
        placement: PLACEMENT_TAB,
        widget: "shortcutConflictList",
    },
    KdePanel {
        panel_id: "native.shortcut-presets",
        tab_id: settings_catalog::TAB_SHORTCUTS,
        placement: PLACEMENT_TAB,
        widget: "shortcutAuthenticButton",
    },
    KdePanel {
        panel_id: "native.shortcut-presets-compatible",
        tab_id: settings_catalog::TAB_SHORTCUTS,
        placement: PLACEMENT_TAB,
        widget: "shortcutCompatibleButton",
    },
    KdePanel {
        panel_id: "native.shortcut-apply",
        tab_id: settings_catalog::TAB_SHORTCUTS,
        placement: PLACEMENT_TAB,
        widget: "shortcutApplyButton",
    },
    KdePanel {
        panel_id: "native.shortcut-revert",
        tab_id: settings_catalog::TAB_SHORTCUTS,
        placement: PLACEMENT_TAB,
        widget: "shortcutRevertButton",
    },
    KdePanel {
        panel_id: "native.window-tiling-conflict",
        tab_id: settings_catalog::TAB_DESKTOP_INTEGRATION,
        placement: PLACEMENT_TAB,
        widget: "windowTilingRow",
    },
    KdePanel {
        panel_id: "native.window-maximize-conflict",
        tab_id: settings_catalog::TAB_DESKTOP_INTEGRATION,
        placement: PLACEMENT_TAB,
        widget: "windowMaximizeRow",
    },
    KdePanel {
        panel_id: "native.window-borders-conflict",
        tab_id: settings_catalog::TAB_DESKTOP_INTEGRATION,
        placement: PLACEMENT_TAB,
        widget: "windowBordersRow",
    },
    KdePanel {
        panel_id: "native.script-status",
        tab_id: "",
        placement: PLACEMENT_BELOW_TABS,
        widget: "scriptStatusGroup",
    },
];

/// Find one exposed KDE field by catalog field id.
#[must_use]
pub fn kde_field(field_id: &str) -> Option<&'static KdeField> {
    KDE_EXPOSED_FIELDS
        .iter()
        .find(|field| field.field_id == field_id)
}

/// Look up the catalog descriptor for one exposed KDE field.
#[must_use]
pub fn kde_catalog_def(field: &KdeField) -> Option<&'static FieldDef> {
    settings_catalog::find_field(field.field_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use tiler_core::settings_catalog::{
        ALL_FIELDS, ALL_TABS, FieldType, SettingValue, find_section, find_tab,
    };

    /// Independent golden expectations: rendered default, schema type and
    /// numeric bounds per exposed field. The emitter must derive exactly
    /// these from the catalog; the literals below are the compatibility
    /// contract, not copies of adapter tables.
    const GOLDEN_DEFAULTS: &[(&str, &str, &str)] = &[
        ("workspace.mode", "per-output-local", "Enum"),
        ("move.same-axis", "group-with-neighbor", "Enum"),
        ("admission.fixed-size", "both-axes-fixed", "Enum"),
        ("admission.born-maximized", "tile-on-launch", "Enum"),
        (
            "admission.anchor-fallback",
            "largest-splittable-window",
            "Enum",
        ),
        ("admission.no-space", "float", "Enum"),
        ("migration.refill", "last-remaining-workspace", "Enum"),
        ("gaps.inner", "8", "Int"),
        ("gaps.outer", "8", "Int"),
        ("border.color", "#2a82da", "Color"),
        ("border.width", "3.0", "Double"),
        ("border.radius", "6.0", "Double"),
        ("border.gap", "0.0", "Double"),
        ("border.use-theme", "true", "Bool"),
        ("underlay.color", "#662a82da", "Color"),
        ("underlay.extension", "-1", "Double"),
    ];

    /// Independent golden enum tokens in catalog order per enum field.
    const GOLDEN_TOKENS: &[(&str, &[&str])] = &[
        (
            "workspace.mode",
            &["per-output-local", "global-unique", "shared"],
        ),
        (
            "move.same-axis",
            &["group-with-neighbor", "swap-with-neighbor"],
        ),
        (
            "admission.fixed-size",
            &["both-axes-fixed", "either-axis-fixed"],
        ),
        (
            "admission.born-maximized",
            &["tile-on-launch", "keep-maximized-overlay"],
        ),
        (
            "admission.anchor-fallback",
            &[
                "largest-splittable-window",
                "closest-splittable-relative",
                "no-search",
            ],
        ),
        ("admission.no-space", &["float", "allow-overlap"]),
        (
            "migration.refill",
            &["last-remaining-workspace", "most-recently-used-workspace"],
        ),
    ];

    /// Independent golden native key order (script form rows, then
    /// appearance form rows with the native-only preview row in place).
    const GOLDEN_KEY_ORDER: &[&str] = &[
        "workspaceMode",
        "sameAxisMove",
        "fixedSizePredicate",
        "bornMaximized",
        "anchorFallback",
        "noSpaceResult",
        "migrationSourceRefill",
        "innerGap",
        "outerGap",
        "BorderColor",
        "BorderWidth",
        "BorderRadius",
        "BorderGap",
        "UseThemeColor",
        "GroupUnderlayColor",
        "GroupUnderlayExtension",
    ];

    /// Independent golden rendered texts that must not change: UI labels
    /// (colon-terminated where the dialog shows colons), schema labels and
    /// the genuinely different workspace-mode rows.
    const GOLDEN_UI_LABELS: &[&str] = &[
        "The multi-output workspace model (requires session restart; per-output needs KWin 6.7+, 6.6 uses shared):",
        "Same-axis move behavior:",
        "Fixed-size admission predicate:",
        "Born-maximized admission behavior:",
        "When the anchor cannot split:",
        "When no split is available:",
        "Source workspace after migration:",
        "Gap between tiled windows:",
        "Gap between tiled windows and the work area edge:",
        "Fallback border color:",
        "Border width:",
        "Outline radius:",
        "Gap from the window:",
        "Use theme highlight color when available",
        "Group underlay color:",
        "Group underlay extension:",
    ];

    #[test]
    fn exposed_unsupported_and_schema_only_partition_the_catalog() {
        let mut seen = BTreeSet::new();
        for field in KDE_EXPOSED_FIELDS {
            assert!(seen.insert(field.field_id), "duplicate {}", field.field_id);
            assert!(
                settings_catalog::find_field(field.field_id).is_some(),
                "unknown {}",
                field.field_id
            );
        }
        assert_eq!(KDE_EXPOSED_FIELDS.len(), 16);
        assert_eq!(KDE_UNSUPPORTED_FIELDS.len(), 6);
        for unsupported in KDE_UNSUPPORTED_FIELDS {
            assert!(
                settings_catalog::find_field(unsupported).is_some(),
                "unknown {unsupported}"
            );
            assert!(
                !seen.contains(*unsupported),
                "exposed {unsupported} despite exclusion"
            );
            seen.insert(*unsupported);
        }
        assert!(settings_catalog::find_field(KDE_SCHEMA_ONLY_FIELD).is_some());
        assert!(!seen.contains(KDE_SCHEMA_ONLY_FIELD));
        seen.insert(KDE_SCHEMA_ONLY_FIELD);
        assert_eq!(seen.len(), ALL_FIELDS.len());
        for field in ALL_FIELDS {
            assert!(seen.contains(field.id), "unmapped {}", field.id);
        }
        assert!(
            KDE_EXPOSED_FIELDS
                .iter()
                .all(|field| field.field_id != KDE_SCHEMA_ONLY_FIELD),
            "schema-only field must not gain a KCM control"
        );
    }

    #[test]
    fn exposed_fields_follow_native_row_order() {
        let keys: Vec<&str> = KDE_EXPOSED_FIELDS.iter().map(|field| field.key).collect();
        assert_eq!(keys, GOLDEN_KEY_ORDER);
    }

    #[test]
    fn widget_bindings_are_unique_and_native_only_entries_stay_out() {
        let mut widgets = BTreeSet::new();
        let mut keys = BTreeSet::new();
        for field in KDE_EXPOSED_FIELDS {
            assert!(widgets.insert(field.widget), "duplicate {}", field.widget);
            assert!(keys.insert((field.group, field.key)), "duplicate key");
            assert!(!field.schema_label.is_empty(), "{}", field.field_id);
            assert!(!field.ui_label.is_empty(), "{}", field.field_id);
        }
        for native_only in [NATIVE_ONLY_KCFG_KEY, NATIVE_ONLY_SCHEMA_KEY] {
            assert!(
                KDE_EXPOSED_FIELDS
                    .iter()
                    .all(|field| field.key != native_only),
                "{native_only} must stay adapter-retained, never mapped"
            );
        }
        assert!(
            KDE_EXPOSED_FIELDS
                .iter()
                .all(|field| field.key != KDE_SCHEMA_ONLY_KEY),
            "schema-only key must not gain a widget mapping"
        );
    }

    #[test]
    fn catalog_derivation_matches_golden_defaults_types_and_bounds() {
        assert_eq!(GOLDEN_DEFAULTS.len(), KDE_EXPOSED_FIELDS.len());
        for (field_id, want_default, want_type) in GOLDEN_DEFAULTS {
            let field = kde_field(field_id).expect("exposed field");
            assert_eq!(field.schema_type, *want_type, "{field_id}");
            let def = kde_catalog_def(field).expect("catalog def");
            assert_eq!(
                crate::emit::rendered_default(def),
                *want_default,
                "default drift {field_id}"
            );
            match def.field_type {
                FieldType::Int => {
                    assert_eq!((def.min_int, def.max_int), (Some(0), Some(64)));
                }
                FieldType::Float => {
                    let (min, max) = (
                        def.min_float.expect("floor"),
                        def.max_float.expect("ceiling"),
                    );
                    let SettingValue::Float(default) = def.default else {
                        panic!("float default {field_id}");
                    };
                    assert!((min..=max).contains(&default), "{field_id}");
                    // Independent golden bounds per float field.
                    let want = match *field_id {
                        "border.width" => (0.0, 32.0),
                        "border.gap" => (0.0, 64.0),
                        "border.radius" => (0.0, 64.0),
                        "underlay.extension" => (-1.0, 32.0),
                        _ => panic!("golden bounds for {field_id}"),
                    };
                    assert_eq!((min, max), want, "bounds drift {field_id}");
                }
                FieldType::Bool => {
                    assert!(matches!(def.default, SettingValue::Bool(_)));
                }
                FieldType::ColorRgb | FieldType::ColorArgb => {
                    assert!(matches!(def.default, SettingValue::Text(_)));
                }
                FieldType::Enum => {
                    let tokens: Vec<&str> = def.options.iter().map(|option| option.token).collect();
                    let want: Vec<&str> = GOLDEN_TOKENS
                        .iter()
                        .find(|(id, _)| id == field_id)
                        .unwrap_or_else(|| panic!("golden tokens for {field_id}"))
                        .1
                        .to_vec();
                    assert_eq!(tokens, want, "token drift {field_id}");
                    assert_eq!(tokens[0], crate::emit::rendered_default(def), "{field_id}");
                }
            }
        }
    }

    #[test]
    fn enum_overrides_cover_only_genuine_differences() {
        // Workspace mode is the only enum with genuinely different native
        // presentation; every other enum flows from the catalog untouched.
        for entry in KDE_ENUM_OVERRIDES {
            assert_eq!(entry.field_id, settings_catalog::FIELD_WORKSPACE_MODE);
        }
        assert_eq!(KDE_ENUM_OVERRIDES.len(), 2);
        for option in kde_exposed_options(settings_catalog::FIELD_WORKSPACE_MODE) {
            let override_entry =
                kde_enum_override(settings_catalog::FIELD_WORKSPACE_MODE, option.token)
                    .expect("override per token");
            // The override is genuine: at least the label or the tip differs.
            assert!(
                override_entry.label != option.label || override_entry.tip != option.tip,
                "{}",
                option.token
            );
        }
        for field in KDE_EXPOSED_FIELDS {
            let def = kde_catalog_def(field).expect("catalog def");
            if def.field_type != FieldType::Enum
                || field.field_id == settings_catalog::FIELD_WORKSPACE_MODE
            {
                continue;
            }
            for option in def.options {
                assert!(
                    kde_enum_override(field.field_id, option.token).is_none(),
                    "coincident {}.{} must flow from the catalog",
                    field.field_id,
                    option.token
                );
                assert_eq!(
                    kde_option_text(field.field_id, option),
                    (option.label, option.tip)
                );
            }
        }
    }

    #[test]
    fn withheld_options_stay_catalog_valid_but_unoffered() {
        // `global-unique` remains a valid catalog token (other platforms,
        // legacy callers) but no KDE surface offers it.
        for (field_id, token) in KDE_WITHHELD_OPTIONS {
            let def = settings_catalog::find_field(field_id).expect("catalog def");
            assert!(
                def.options.iter().any(|option| option.token == *token),
                "withheld {field_id}.{token} must exist in the catalog"
            );
            assert!(
                !kde_exposed_options(field_id)
                    .iter()
                    .any(|option| option.token == *token),
                "withheld {field_id}.{token} must not be offered"
            );
        }
        let exposed: Vec<&str> = kde_exposed_options(settings_catalog::FIELD_WORKSPACE_MODE)
            .iter()
            .map(|option| option.token)
            .collect();
        assert_eq!(exposed, vec!["per-output-local", "shared"]);
        // The catalog default is never withheld.
        let def =
            settings_catalog::find_field(settings_catalog::FIELD_WORKSPACE_MODE).expect("mode def");
        assert!(exposed.contains(&crate::emit::rendered_default(def).as_str()));
    }

    #[test]
    fn native_ui_texts_exist_in_the_retained_dialog_layout() {
        // The `.ui` layout persists across the migration: widget names and
        // visible texts pinned here must keep rendering byte-identically.
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let ui = std::fs::read_to_string(root.join("kwin/native-effect/unifiedsettings.ui"))
            .expect("ui readable");
        assert!(ui.contains(TILING_TAB_UI_SOURCE));
        for tab in KDE_TABS {
            assert!(ui.contains(tab.page_widget), "{}", tab.page_widget);
            if tab.tab_id != settings_catalog::TAB_TILING_WORKSPACES {
                assert!(ui.contains(tab.native_title), "{}", tab.native_title);
            }
            assert!(find_tab(tab.tab_id).is_some(), "{}", tab.tab_id);
        }
        for label in GOLDEN_UI_LABELS {
            assert!(ui.contains(label), "{label}");
        }
        for field in KDE_EXPOSED_FIELDS {
            assert!(ui.contains(field.widget), "{}", field.widget);
            assert!(ui.contains(field.ui_label), "{}", field.key);
        }
        assert!(ui.contains("kcfg_DragPreviewColor"));
        assert!(ui.contains("Drop-target preview color:"));
    }

    #[test]
    fn tabs_and_native_panels_match_native_structure() {
        let ids: Vec<&str> = KDE_TABS.iter().map(|tab| tab.tab_id).collect();
        let want: Vec<&str> = ALL_TABS.iter().map(|tab| tab.id).collect();
        assert_eq!(ids, want);
        assert_eq!(KDE_TABS.len(), 4);
        let mut seen_panels = BTreeSet::new();
        for panel in KDE_NATIVE_PANELS {
            assert!(
                seen_panels.insert(panel.panel_id),
                "duplicate {}",
                panel.panel_id
            );
            assert!(!panel.widget.is_empty(), "{}", panel.panel_id);
            assert!(
                panel.placement == PLACEMENT_TAB || panel.placement == PLACEMENT_BELOW_TABS,
                "{}",
                panel.panel_id
            );
            if panel.placement == PLACEMENT_TAB {
                assert!(find_tab(panel.tab_id).is_some(), "{}", panel.panel_id);
            } else {
                // Below-strip panels sit outside every tab; claiming a tab
                // would misplace them on integration.
                assert!(panel.tab_id.is_empty(), "{}", panel.panel_id);
            }
        }
        for field in KDE_EXPOSED_FIELDS {
            let def = kde_catalog_def(field).expect("catalog def");
            let section = find_section(def.section).expect("section");
            assert!(
                section.tab == settings_catalog::TAB_TILING_WORKSPACES
                    || section.tab == settings_catalog::TAB_APPEARANCE,
                "{}",
                field.field_id
            );
        }
        for wanted in [
            "native.shortcut-apply",
            "native.shortcut-revert",
            "native.window-tiling-conflict",
            "native.script-status",
        ] {
            assert!(seen_panels.contains(wanted), "{wanted}");
        }
        let status = KDE_NATIVE_PANELS
            .iter()
            .find(|panel| panel.panel_id == "native.script-status")
            .expect("status panel");
        assert_eq!(status.placement, PLACEMENT_BELOW_TABS);
        assert_eq!(status.widget, "scriptStatusGroup");
    }
}

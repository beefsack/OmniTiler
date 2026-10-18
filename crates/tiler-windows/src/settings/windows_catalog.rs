#![forbid(unsafe_code)]
//! Windows settings adapter over the shared portable catalog.
//!
//! Maps every scalar Windows-exposed field exactly once to its unchanged
//! schema-v1 storage key, gets/stages typed values through the catalog
//! defaults/bounds/labels/validation, and declares Windows-only fields
//! plus explicit native action panels. Staging only mutates a draft
//! `Settings`; it never invokes native operations (presets, Win+L
//! remediation, takeover apply, snap ledger writes, gaming navigation).
//! `v`, `revision`, and `bindings` stay native and are never mapped here.

use tiler_core::settings_catalog::{
    FieldDef, HostCaps, PanelSlot, SectionDef, SettingValue, Timing, Validity, find_field,
};
use tiler_core::settings_catalog::{TAB_DESKTOP_INTEGRATION, TAB_SHORTCUTS};

use super::{Settings, validate_settings};

// ---------------------------------------------------------------------------
// Windows-only sections and panels
// ---------------------------------------------------------------------------

/// Takeover and Win+L section on the Shortcuts tab.
pub const SECTION_SHORTCUTS_TAKEOVER: &str = "shortcuts.takeover";
/// Preset section on the Shortcuts tab.
pub const SECTION_SHORTCUTS_PRESETS: &str = "shortcuts.presets";
/// Binding editor section on the Shortcuts tab.
pub const SECTION_SHORTCUTS_BINDINGS: &str = "shortcuts.bindings";
/// Gaming operations section on the Shortcuts tab.
pub const SECTION_SHORTCUTS_GAMING: &str = "shortcuts.gaming";
/// Snap policy section on the Desktop Integration tab.
pub const SECTION_DESKTOP_SNAP: &str = "desktop.snap";
/// Taskbar pager section on the Desktop Integration tab.
pub const SECTION_DESKTOP_PAGER: &str = "desktop.pager";

/// Windows-only sections in stable order. Lets renderers derive each
/// Windows field/panel tab from its section (like the portable
/// `ALL_SECTIONS`) instead of keeping a second hardcoded tab list.
pub const WINDOWS_SECTIONS: &[SectionDef] = &[
    SectionDef {
        id: SECTION_SHORTCUTS_TAKEOVER,
        tab: TAB_SHORTCUTS,
        label: "Takeover",
    },
    SectionDef {
        id: SECTION_SHORTCUTS_PRESETS,
        tab: TAB_SHORTCUTS,
        label: "Presets",
    },
    SectionDef {
        id: SECTION_SHORTCUTS_BINDINGS,
        tab: TAB_SHORTCUTS,
        label: "Binding editor",
    },
    SectionDef {
        id: SECTION_SHORTCUTS_GAMING,
        tab: TAB_SHORTCUTS,
        label: "Gaming operations",
    },
    SectionDef {
        id: SECTION_DESKTOP_SNAP,
        tab: TAB_DESKTOP_INTEGRATION,
        label: "Snap policy",
    },
    SectionDef {
        id: SECTION_DESKTOP_PAGER,
        tab: TAB_DESKTOP_INTEGRATION,
        label: "Taskbar pager",
    },
];

/// Windows-only capabilities: exposed on Windows only.
pub const WINDOWS_ONLY_CAPS: HostCaps = HostCaps {
    windows: true,
    kde: false,
    macos: false,
};

/// Windows-only field IDs.
pub const WINDOWS_FIELD_TAKEOVER: &str = "keyboard.takeover";
pub const WINDOWS_FIELD_ALLOW_WIN_L: &str = "keyboard.allow-win-l";
pub const WINDOWS_FIELD_SNAP_PREVENTION: &str = "mouse.snap-prevention";
pub const WINDOWS_FIELD_TASKBAR_PAGER: &str = "desktop.taskbar-pager";

/// Windows-only field descriptors (bool scalars reusing the generic catalog
/// types; no platform policy in core).
pub const WINDOWS_ONLY_FIELDS: &[FieldDef] = &[
    FieldDef {
        id: WINDOWS_FIELD_TAKEOVER,
        section: SECTION_SHORTCUTS_TAKEOVER,
        label: "Keyboard takeover",
        help: "Intercept catalog chords before Windows; off passes every chord through natively.",
        field_type: tiler_core::settings_catalog::FieldType::Bool,
        default: SettingValue::Bool(true),
        min_int: None,
        max_int: None,
        min_float: None,
        max_float: None,
        options: &[],
        timing: Timing::Live,
        timing_help: "Applies live on the next owner pump. Existing staged takeover stays on the native apply path.",
        capabilities: WINDOWS_ONLY_CAPS,
        bounds_reason: "Boolean field; no numeric bound.",
    },
    FieldDef {
        id: WINDOWS_FIELD_ALLOW_WIN_L,
        section: SECTION_SHORTCUTS_TAKEOVER,
        label: "Allow Win+L interception",
        help: "Explicit opt-in for unshifted Win+L. The OS lock chord keeps its safety fence regardless of rebinding.",
        field_type: tiler_core::settings_catalog::FieldType::Bool,
        default: SettingValue::Bool(false),
        min_int: None,
        max_int: None,
        min_float: None,
        max_float: None,
        options: &[],
        timing: Timing::Live,
        timing_help: "Applies live on the next owner pump; never claims reliable OS lock suppression.",
        capabilities: WINDOWS_ONLY_CAPS,
        bounds_reason: "Boolean field; no numeric bound.",
    },
    FieldDef {
        id: WINDOWS_FIELD_SNAP_PREVENTION,
        section: SECTION_DESKTOP_SNAP,
        label: "Snap prevention",
        help: "Session-only Snap prevention; off restores the captured owned preimage through the existing ledger path.",
        field_type: tiler_core::settings_catalog::FieldType::Bool,
        default: SettingValue::Bool(true),
        min_int: None,
        max_int: None,
        min_float: None,
        max_float: None,
        options: &[],
        timing: Timing::Live,
        timing_help: "Applies live on the next owner pump; never touches registry or policy.",
        capabilities: WINDOWS_ONLY_CAPS,
        bounds_reason: "Boolean field; no numeric bound.",
    },
    FieldDef {
        id: WINDOWS_FIELD_TASKBAR_PAGER,
        section: SECTION_DESKTOP_PAGER,
        label: "Taskbar pager",
        help: "Per-output workspace strip overlaid in the taskbar.",
        field_type: tiler_core::settings_catalog::FieldType::Bool,
        default: SettingValue::Bool(true),
        min_int: None,
        max_int: None,
        min_float: None,
        max_float: None,
        options: &[],
        timing: Timing::Live,
        timing_help: "Live edits hot-apply on the next owner pump.",
        capabilities: WINDOWS_ONLY_CAPS,
        bounds_reason: "Boolean field; no numeric bound.",
    },
];

/// One scalar storage mapping: stable field ID to its unchanged schema-v1
/// nested key. `v`, `revision`, and `bindings` are native and never mapped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FieldStorageMap {
    pub field: &'static str,
    pub storage_key: &'static str,
}

/// Portable fields with no Windows mapping: none. Both best-effort
/// admission settings (`admission.anchor-fallback`, `admission.no-space`)
/// are mapped below and consumed by the legacy Engine since the runtime
/// adoption; the table stays as the explicit empty partition point so a
/// future unmapped portable field cannot silently drift.
pub const WINDOWS_UNSUPPORTED_FIELDS: &[&str] = &[];

/// Every scalar Windows-exposed field mapped exactly once (19 portable plus
/// 4 Windows-only). Storage keys are the existing schema-v1 nested
/// snake_case keys; this table renames nothing.
pub const WINDOWS_FIELD_MAPPINGS: &[FieldStorageMap] = &[
    FieldStorageMap {
        field: "gaps.inner",
        storage_key: "core.inner_gap",
    },
    FieldStorageMap {
        field: "gaps.outer",
        storage_key: "core.outer_gap",
    },
    FieldStorageMap {
        field: "workspace.default-tiled",
        storage_key: "core.workspace.default_tiled",
    },
    FieldStorageMap {
        field: "workspace.mode",
        storage_key: "core.workspace.mode",
    },
    FieldStorageMap {
        field: "move.same-axis",
        storage_key: "core.same_axis_move",
    },
    FieldStorageMap {
        field: "admission.born-maximized",
        storage_key: "core.born_maximized",
    },
    FieldStorageMap {
        field: "admission.fixed-size",
        storage_key: "core.fixed_size_predicate",
    },
    FieldStorageMap {
        field: "admission.anchor-fallback",
        storage_key: "core.anchor_fallback",
    },
    FieldStorageMap {
        field: "admission.no-space",
        storage_key: "core.no_space_result",
    },
    FieldStorageMap {
        field: "migration.refill",
        storage_key: "core.migration_source_refill",
    },
    FieldStorageMap {
        field: "border.enabled",
        storage_key: "core.border.enabled",
    },
    FieldStorageMap {
        field: "border.width",
        storage_key: "core.border.width",
    },
    FieldStorageMap {
        field: "border.gap",
        storage_key: "core.border.gap",
    },
    FieldStorageMap {
        field: "border.radius",
        storage_key: "core.border.radius",
    },
    FieldStorageMap {
        field: "border.color",
        storage_key: "core.border.color",
    },
    FieldStorageMap {
        field: "border.use-theme",
        storage_key: "core.border.use_theme",
    },
    FieldStorageMap {
        field: "underlay.enabled",
        storage_key: "core.underlay.enabled",
    },
    FieldStorageMap {
        field: "underlay.color",
        storage_key: "core.underlay.color",
    },
    FieldStorageMap {
        field: "underlay.extension",
        storage_key: "core.underlay.extension",
    },
    FieldStorageMap {
        field: WINDOWS_FIELD_TAKEOVER,
        storage_key: "core.keyboard.takeover",
    },
    FieldStorageMap {
        field: WINDOWS_FIELD_ALLOW_WIN_L,
        storage_key: "core.keyboard.allow_win_l",
    },
    FieldStorageMap {
        field: WINDOWS_FIELD_SNAP_PREVENTION,
        storage_key: "core.mouse.snap_prevention",
    },
    FieldStorageMap {
        field: WINDOWS_FIELD_TASKBAR_PAGER,
        storage_key: "core.taskbar_pager",
    },
];

/// Explicit native action panels. Metadata only: no generic setter may
/// execute these; presets, Win+L remediation, takeover apply, snap ledger
/// writes, binding routing changes, and gaming navigation stay on their
/// existing explicit native paths.
pub const NATIVE_PANELS: &[PanelSlot] = &[
    PanelSlot {
        id: "native.takeover-policy",
        tab: tiler_core::settings_catalog::TAB_SHORTCUTS,
        section: SECTION_SHORTCUTS_TAKEOVER,
        label: "Takeover policy",
    },
    PanelSlot {
        id: "native.win-l-remediation",
        tab: tiler_core::settings_catalog::TAB_SHORTCUTS,
        section: SECTION_SHORTCUTS_TAKEOVER,
        label: "Win+L remediation",
    },
    PanelSlot {
        id: "native.shortcut-presets",
        tab: tiler_core::settings_catalog::TAB_SHORTCUTS,
        section: SECTION_SHORTCUTS_PRESETS,
        label: "Shortcut presets",
    },
    PanelSlot {
        id: "native.binding-editor",
        tab: tiler_core::settings_catalog::TAB_SHORTCUTS,
        section: SECTION_SHORTCUTS_BINDINGS,
        label: "Binding editor",
    },
    PanelSlot {
        id: "native.gaming-operations",
        tab: tiler_core::settings_catalog::TAB_SHORTCUTS,
        section: SECTION_SHORTCUTS_GAMING,
        label: "Gaming operations",
    },
    PanelSlot {
        id: "native.snap-policy",
        tab: tiler_core::settings_catalog::TAB_DESKTOP_INTEGRATION,
        section: SECTION_DESKTOP_SNAP,
        label: "Snap policy",
    },
];

/// Look up any Windows-exposed field descriptor (portable first, then
/// Windows-only).
#[must_use]
pub fn windows_field_def(id: &str) -> Option<&'static FieldDef> {
    find_field(id).or_else(|| WINDOWS_ONLY_FIELDS.iter().find(|field| field.id == id))
}

/// Owned typed scalar for adapter get/stage. Text carries enum tokens
/// and color strings as owned data; the catalog still owns the exact
/// token tables and grammars.
#[derive(Debug, Clone, PartialEq)]
pub enum WindowsValue {
    Bool(bool),
    Int(i32),
    Float(f64),
    Text(String),
}

/// Read one typed scalar from live settings. Returns `None` for unknown
/// field IDs. Never invokes native operations.
#[must_use]
pub fn windows_field_value(settings: &Settings, field: &str) -> Option<WindowsValue> {
    match field {
        "gaps.inner" => Some(WindowsValue::Int(settings.core.inner_gap)),
        "gaps.outer" => Some(WindowsValue::Int(settings.core.outer_gap)),
        "workspace.default-tiled" => {
            Some(WindowsValue::Bool(settings.core.workspace.default_tiled))
        }
        "border.enabled" => Some(WindowsValue::Bool(settings.core.border.enabled)),
        "border.width" => Some(WindowsValue::Float(settings.core.border.width)),
        "border.gap" => Some(WindowsValue::Float(settings.core.border.gap)),
        "border.radius" => Some(WindowsValue::Float(settings.core.border.radius)),
        "border.use-theme" => Some(WindowsValue::Bool(settings.core.border.use_theme)),
        "border.color" => Some(WindowsValue::Text(settings.core.border.color.clone())),
        "underlay.enabled" => Some(WindowsValue::Bool(settings.core.underlay.enabled)),
        "underlay.color" => Some(WindowsValue::Text(settings.core.underlay.color.clone())),
        "underlay.extension" => Some(WindowsValue::Float(settings.core.underlay.extension)),
        "move.same-axis" => Some(WindowsValue::Text(settings.core.same_axis_move.clone())),
        "admission.born-maximized" => {
            Some(WindowsValue::Text(settings.core.born_maximized.clone()))
        }
        "admission.fixed-size" => Some(WindowsValue::Text(
            settings.core.fixed_size_predicate.clone(),
        )),
        "admission.anchor-fallback" => {
            Some(WindowsValue::Text(settings.core.anchor_fallback.clone()))
        }
        "admission.no-space" => Some(WindowsValue::Text(settings.core.no_space_result.clone())),
        "migration.refill" => Some(WindowsValue::Text(
            settings.core.migration_source_refill.clone(),
        )),
        "workspace.mode" => Some(WindowsValue::Text(settings.core.workspace.mode.clone())),
        "keyboard.takeover" => Some(WindowsValue::Bool(settings.core.keyboard.takeover)),
        "keyboard.allow-win-l" => Some(WindowsValue::Bool(settings.core.keyboard.allow_win_l)),
        "mouse.snap-prevention" => Some(WindowsValue::Bool(settings.core.mouse.snap_prevention)),
        "desktop.taskbar-pager" => Some(WindowsValue::Bool(settings.core.taskbar_pager)),
        _ => None,
    }
}

/// Stage one typed scalar into a draft document. Wrong-type values and
/// catalog-invalid values refuse with a reason; the draft is then checked
/// with the unchanged strict [`validate_settings`] policy, so storage
/// semantics and error vocabulary never change. Never invokes native
/// operations.
pub fn stage_windows_field_value(
    settings: &mut Settings,
    field: &str,
    value: &WindowsValue,
) -> Result<(), String> {
    let def = windows_field_def(field).ok_or_else(|| format!("unknown field {field}"))?;
    let validity = match value {
        WindowsValue::Bool(got) => {
            tiler_core::settings_catalog::validate_field_value(def, &SettingValue::Bool(*got))
        }
        WindowsValue::Int(got) => {
            tiler_core::settings_catalog::validate_field_value(def, &SettingValue::Int(*got))
        }
        WindowsValue::Float(got) => {
            tiler_core::settings_catalog::validate_field_value(def, &SettingValue::Float(*got))
        }
        WindowsValue::Text(got) => tiler_core::settings_catalog::validate_text(def, got),
    };
    if validity != Validity::Valid {
        return Err(format!("field {field} refuses this value"));
    }
    let mut next = settings.clone();
    apply_typed(&mut next, field, value)?;
    validate_settings(&next).map_err(|error| error.to_string())?;
    *settings = next;
    Ok(())
}

fn apply_typed(settings: &mut Settings, field: &str, value: &WindowsValue) -> Result<(), String> {
    match (field, value) {
        ("gaps.inner", WindowsValue::Int(got)) => settings.core.inner_gap = *got,
        ("gaps.outer", WindowsValue::Int(got)) => settings.core.outer_gap = *got,
        ("workspace.default-tiled", WindowsValue::Bool(got)) => {
            settings.core.workspace.default_tiled = *got;
        }
        ("border.enabled", WindowsValue::Bool(got)) => settings.core.border.enabled = *got,
        ("border.width", WindowsValue::Float(got)) => settings.core.border.width = *got,
        ("border.gap", WindowsValue::Float(got)) => settings.core.border.gap = *got,
        ("border.radius", WindowsValue::Float(got)) => settings.core.border.radius = *got,
        ("border.use-theme", WindowsValue::Bool(got)) => settings.core.border.use_theme = *got,
        ("underlay.enabled", WindowsValue::Bool(got)) => settings.core.underlay.enabled = *got,
        ("underlay.extension", WindowsValue::Float(got)) => settings.core.underlay.extension = *got,
        ("keyboard.takeover", WindowsValue::Bool(got)) => settings.core.keyboard.takeover = *got,
        ("keyboard.allow-win-l", WindowsValue::Bool(got)) => {
            settings.core.keyboard.allow_win_l = *got;
        }
        ("mouse.snap-prevention", WindowsValue::Bool(got)) => {
            settings.core.mouse.snap_prevention = *got;
        }
        ("desktop.taskbar-pager", WindowsValue::Bool(got)) => settings.core.taskbar_pager = *got,
        ("move.same-axis", WindowsValue::Text(got)) => settings.core.same_axis_move = got.clone(),
        ("admission.born-maximized", WindowsValue::Text(got)) => {
            settings.core.born_maximized = got.clone();
        }
        ("admission.fixed-size", WindowsValue::Text(got)) => {
            settings.core.fixed_size_predicate = got.clone();
        }
        ("admission.anchor-fallback", WindowsValue::Text(got)) => {
            settings.core.anchor_fallback = got.clone();
        }
        ("admission.no-space", WindowsValue::Text(got)) => {
            settings.core.no_space_result = got.clone();
        }
        ("migration.refill", WindowsValue::Text(got)) => {
            settings.core.migration_source_refill = got.clone();
        }
        ("workspace.mode", WindowsValue::Text(got)) => settings.core.workspace.mode = got.clone(),
        ("border.color", WindowsValue::Text(got)) => settings.core.border.color = got.clone(),
        ("underlay.color", WindowsValue::Text(got)) => {
            settings.core.underlay.color = got.clone();
        }
        _ => return Err(format!("field {field} refuses this value")),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use tiler_core::settings_catalog::{
        ALL_FIELDS, ALL_SECTIONS, ALL_TABS, FieldType, SettingValue, Validity, validate_text,
    };

    /// All scalar leaves under `core`: flattens the serialized `CoreSettings`
    /// subtree into `core.*` storage paths.
    fn core_leaf_keys(value: &serde_json::Value) -> BTreeSet<String> {
        fn walk(node: &serde_json::Value, path: String, out: &mut BTreeSet<String>) {
            match node {
                serde_json::Value::Object(map) => {
                    for (key, child) in map {
                        walk(child, format!("{path}.{key}"), out);
                    }
                }
                _ => {
                    out.insert(path);
                }
            }
        }
        let mut out = BTreeSet::new();
        walk(value, "core".to_owned(), &mut out);
        out
    }

    fn windows_value_to_json(value: &WindowsValue) -> serde_json::Value {
        match value {
            WindowsValue::Bool(got) => serde_json::Value::Bool(*got),
            WindowsValue::Int(got) => serde_json::Value::from(*got),
            WindowsValue::Float(got) => serde_json::Value::from(*got),
            WindowsValue::Text(got) => serde_json::Value::from(got.clone()),
        }
    }

    fn descriptor_default_matches(def: &FieldDef, value: &WindowsValue) -> bool {
        match (&def.default, value) {
            (SettingValue::Bool(want), WindowsValue::Bool(got)) => want == got,
            (SettingValue::Int(want), WindowsValue::Int(got)) => want == got,
            (SettingValue::Float(want), WindowsValue::Float(got)) => want == got,
            (SettingValue::Text(want), WindowsValue::Text(got)) => *want == got.as_str(),
            _ => false,
        }
    }

    /// One valid non-default value per mapped field.
    fn non_default_value(field: &str) -> WindowsValue {
        match field {
            "gaps.inner" | "gaps.outer" => WindowsValue::Int(0),
            "workspace.default-tiled" => WindowsValue::Bool(false),
            "workspace.mode" => WindowsValue::Text("shared".to_owned()),
            "move.same-axis" => WindowsValue::Text("swap-with-neighbor".to_owned()),
            "admission.born-maximized" => WindowsValue::Text("keep-maximized-overlay".to_owned()),
            "admission.fixed-size" => WindowsValue::Text("either-axis-fixed".to_owned()),
            "admission.anchor-fallback" => {
                WindowsValue::Text("closest-splittable-relative".to_owned())
            }
            "admission.no-space" => WindowsValue::Text("allow-overlap".to_owned()),
            "migration.refill" => WindowsValue::Text("most-recently-used-workspace".to_owned()),
            "border.enabled" => WindowsValue::Bool(false),
            "border.width" => WindowsValue::Float(4.0),
            "border.gap" => WindowsValue::Float(1.0),
            "border.radius" => WindowsValue::Float(2.0),
            "border.color" => WindowsValue::Text("#ffffff".to_owned()),
            "border.use-theme" => WindowsValue::Bool(false),
            "underlay.enabled" => WindowsValue::Bool(false),
            "underlay.color" => WindowsValue::Text("#80ffffff".to_owned()),
            "underlay.extension" => WindowsValue::Float(0.0),
            "keyboard.takeover" => WindowsValue::Bool(false),
            "keyboard.allow-win-l" => WindowsValue::Bool(true),
            "mouse.snap-prevention" => WindowsValue::Bool(false),
            "desktop.taskbar-pager" => WindowsValue::Bool(false),
            _ => panic!("no non-default for {field}"),
        }
    }

    #[test]
    fn every_scalar_field_mapped_exactly_once() {
        assert_eq!(WINDOWS_FIELD_MAPPINGS.len(), 23);
        let mut fields = BTreeSet::new();
        let mut keys = BTreeSet::new();
        for entry in WINDOWS_FIELD_MAPPINGS {
            assert!(fields.insert(entry.field), "duplicate {}", entry.field);
            assert!(
                keys.insert(entry.storage_key),
                "duplicate {}",
                entry.storage_key
            );
            assert!(windows_field_def(entry.field).is_some(), "{}", entry.field);
        }
        for field in ALL_FIELDS.iter().chain(WINDOWS_ONLY_FIELDS.iter()) {
            if WINDOWS_UNSUPPORTED_FIELDS.contains(&field.id) {
                assert!(
                    !fields.contains(field.id),
                    "supported {} must stay mapped",
                    field.id
                );
                continue;
            }
            assert!(fields.contains(field.id), "unmapped {}", field.id);
        }
        assert!(!keys.contains("v"));
        assert!(!keys.contains("revision"));
        assert!(!keys.iter().any(|key| key.starts_with("bindings")));
    }

    #[test]
    fn unsupported_fields_partition_the_catalog() {
        // Best-effort admission settings are mapped since the runtime
        // adoption: no portable field stays unmapped. Mapped plus
        // unsupported still partition every portable field.
        assert_eq!(WINDOWS_UNSUPPORTED_FIELDS, &[] as &[&str]);
        let mapped: BTreeSet<&str> = WINDOWS_FIELD_MAPPINGS
            .iter()
            .map(|entry| entry.field)
            .collect();
        for unsupported in WINDOWS_UNSUPPORTED_FIELDS {
            assert!(
                tiler_core::settings_catalog::find_field(unsupported).is_some(),
                "unknown {unsupported}"
            );
            assert!(!mapped.contains(unsupported), "mapped {unsupported}");
            assert!(
                windows_field_value(&Settings::default(), unsupported).is_none(),
                "readable {unsupported}"
            );
            let mut settings = Settings::default();
            assert!(
                stage_windows_field_value(&mut settings, unsupported, &WindowsValue::Bool(true))
                    .is_err(),
                "stageable {unsupported}"
            );
        }
        let mut seen = BTreeSet::new();
        for entry in WINDOWS_FIELD_MAPPINGS {
            assert!(seen.insert(entry.field), "duplicate {}", entry.field);
        }
        for unsupported in WINDOWS_UNSUPPORTED_FIELDS {
            assert!(seen.insert(*unsupported), "duplicate {unsupported}");
        }
        for field in ALL_FIELDS.iter().chain(WINDOWS_ONLY_FIELDS.iter()) {
            assert!(seen.contains(field.id), "unaccounted {}", field.id);
        }
        assert_eq!(seen.len(), ALL_FIELDS.len() + WINDOWS_ONLY_FIELDS.len());
    }

    #[test]
    fn mapped_keys_cover_every_scalar_leaf_under_core() {
        let settings = Settings::default();
        let core = serde_json::to_value(&settings.core).expect("core serializes");
        let leaves = core_leaf_keys(&core);
        let mapped: BTreeSet<String> = WINDOWS_FIELD_MAPPINGS
            .iter()
            .map(|entry| entry.storage_key.to_owned())
            .collect();
        assert_eq!(leaves, mapped);
    }

    #[test]
    fn storage_keys_round_trip_unchanged() {
        let settings = Settings::default();
        let json = serde_json::to_value(&settings).expect("serializes");
        for entry in WINDOWS_FIELD_MAPPINGS {
            let mut node = &json;
            for part in entry.storage_key.split('.') {
                node = node.get(part).unwrap_or_else(|| {
                    panic!("missing storage key {}", entry.storage_key);
                });
            }
            assert!(!node.is_null(), "{}", entry.storage_key);
        }
        let back: Settings = serde_json::from_value(json).expect("round-trips");
        assert_eq!(back, settings);
        assert!(validate_settings(&back).is_ok());
    }

    #[test]
    fn defaults_match_settings_and_serialized_nodes() {
        let settings = Settings::default();
        let json = serde_json::to_value(&settings).expect("serializes");
        for entry in WINDOWS_FIELD_MAPPINGS {
            let def = windows_field_def(entry.field).expect("descriptor");
            let value = windows_field_value(&settings, entry.field)
                .unwrap_or_else(|| panic!("readable {}", entry.field));
            assert!(
                descriptor_default_matches(def, &value),
                "default drift {}",
                entry.field
            );
            let mut node = &json;
            for part in entry.storage_key.split('.') {
                node = node.get(part).expect("storage key");
            }
            assert_eq!(*node, windows_value_to_json(&value), "{}", entry.field);
        }
    }

    #[test]
    fn non_default_stage_round_trips_every_mapping() {
        let mut settings = Settings::default();
        for entry in WINDOWS_FIELD_MAPPINGS {
            let staged = non_default_value(entry.field);
            stage_windows_field_value(&mut settings, entry.field, &staged)
                .unwrap_or_else(|error| panic!("stages {}: {error}", entry.field));
            let read = windows_field_value(&settings, entry.field).expect("readable");
            assert_eq!(read, staged, "{}", entry.field);
            let json = serde_json::to_value(&settings).expect("serializes");
            let mut node = &json;
            for part in entry.storage_key.split('.') {
                node = node.get(part).expect("storage key");
            }
            assert_eq!(*node, windows_value_to_json(&staged), "{}", entry.field);
        }
        assert!(validate_settings(&settings).is_ok());
        let json = serde_json::to_value(&settings).expect("serializes");
        let back: Settings = serde_json::from_value(json).expect("round-trips");
        assert_eq!(back, settings);
    }

    #[test]
    fn catalog_refusal_matches_strict_validation() {
        // The catalog pre-check refuses exactly what the unchanged strict
        // validator refuses; reason strings and order of the old validators
        // are untouched.
        let mut settings = Settings::default();
        assert!(
            stage_windows_field_value(&mut settings, "gaps.inner", &WindowsValue::Int(65)).is_err()
        );
        settings.core.inner_gap = 65;
        assert!(validate_settings(&settings).is_err());
        settings.core.inner_gap = 8;
        assert!(
            stage_windows_field_value(
                &mut settings,
                "workspace.mode",
                &WindowsValue::Text("bogus".to_owned())
            )
            .is_err()
        );
        settings.core.workspace.mode = "bogus".to_owned();
        assert!(validate_settings(&settings).is_err());
        settings.core.workspace.mode = "per-output-local".to_owned();
        assert!(
            stage_windows_field_value(
                &mut settings,
                "border.color",
                &WindowsValue::Text("#ffff".to_owned())
            )
            .is_err()
        );
        settings.core.border.color = "#ffff".to_owned();
        assert!(validate_settings(&settings).is_err());
        assert!(validate_settings(&Settings::default()).is_ok());
    }

    #[test]
    fn typed_get_stage_round_trip() {
        let mut settings = Settings::default();
        for entry in WINDOWS_FIELD_MAPPINGS {
            let value = windows_field_value(&settings, entry.field)
                .unwrap_or_else(|| panic!("readable {}", entry.field));
            let def = windows_field_def(entry.field).expect("descriptor");
            let validity = match &value {
                WindowsValue::Bool(got) => tiler_core::settings_catalog::validate_field_value(
                    def,
                    &SettingValue::Bool(*got),
                ),
                WindowsValue::Int(got) => tiler_core::settings_catalog::validate_field_value(
                    def,
                    &SettingValue::Int(*got),
                ),
                WindowsValue::Float(got) => tiler_core::settings_catalog::validate_field_value(
                    def,
                    &SettingValue::Float(*got),
                ),
                WindowsValue::Text(got) => validate_text(def, got),
            };
            assert_eq!(validity, Validity::Valid, "{}", entry.field);
            if def.field_type == FieldType::Enum {
                assert!(!def.options.is_empty(), "{}", entry.field);
            }
            stage_windows_field_value(&mut settings, entry.field, &value).expect("stages");
        }
        assert!(validate_settings(&settings).is_ok());
        assert!(
            stage_windows_field_value(&mut settings, "gaps.inner", &WindowsValue::Int(65)).is_err()
        );
        assert!(
            stage_windows_field_value(
                &mut settings,
                "workspace.mode",
                &WindowsValue::Text("bogus".to_owned())
            )
            .is_err()
        );
        assert!(
            stage_windows_field_value(&mut settings, "nope", &WindowsValue::Bool(true)).is_err()
        );
        assert!(
            stage_windows_field_value(&mut settings, "gaps.inner", &WindowsValue::Bool(true))
                .is_err()
        );
    }

    #[test]
    fn windows_sections_and_fields_are_valid() {
        let tabs: BTreeSet<&str> = ALL_TABS.iter().map(|tab| tab.id).collect();
        let mut seen_sections = BTreeSet::new();
        for section in WINDOWS_SECTIONS {
            assert!(seen_sections.insert(section.id), "duplicate {}", section.id);
            assert!(tabs.contains(section.tab), "{}", section.id);
            assert!(!section.label.is_empty(), "{}", section.id);
        }
        assert_eq!(WINDOWS_SECTIONS.len(), 6);
        let sections: BTreeSet<&str> = ALL_SECTIONS
            .iter()
            .map(|section| section.id)
            .chain(WINDOWS_SECTIONS.iter().map(|section| section.id))
            .collect();
        assert_eq!(sections.len(), ALL_SECTIONS.len() + WINDOWS_SECTIONS.len());
        let mut seen_fields = BTreeSet::new();
        for field in WINDOWS_ONLY_FIELDS {
            assert!(seen_fields.insert(field.id), "duplicate {}", field.id);
            assert!(
                WINDOWS_SECTIONS.iter().any(|s| s.id == field.section),
                "{}",
                field.id
            );
            assert!(!field.label.is_empty(), "{}", field.id);
            assert!(!field.help.is_empty(), "{}", field.id);
            assert!(!field.timing_help.is_empty(), "{}", field.id);
            assert!(!field.bounds_reason.is_empty(), "{}", field.id);
            assert_eq!(field.capabilities, WINDOWS_ONLY_CAPS, "{}", field.id);
            assert_eq!(
                tiler_core::settings_catalog::validate_field_value(field, &field.default),
                Validity::Valid,
                "{}",
                field.id
            );
        }
        assert_eq!(WINDOWS_ONLY_FIELDS.len(), 4);
    }

    #[test]
    fn portable_and_windows_ids_share_no_collisions() {
        let mut seen = BTreeSet::new();
        for id in ALL_TABS
            .iter()
            .map(|tab| tab.id)
            .chain(ALL_SECTIONS.iter().map(|section| section.id))
            .chain(WINDOWS_SECTIONS.iter().map(|section| section.id))
            .chain(ALL_FIELDS.iter().map(|field| field.id))
            .chain(WINDOWS_ONLY_FIELDS.iter().map(|field| field.id))
            .chain(NATIVE_PANELS.iter().map(|panel| panel.id))
        {
            assert!(seen.insert(id), "duplicate id {id}");
        }
    }

    #[test]
    fn native_panels_declared_exactly_once_with_valid_tabs() {
        let tabs: BTreeSet<&str> = ALL_TABS.iter().map(|tab| tab.id).collect();
        let sections: BTreeSet<&str> = ALL_SECTIONS
            .iter()
            .map(|section| section.id)
            .chain([
                SECTION_SHORTCUTS_TAKEOVER,
                SECTION_SHORTCUTS_PRESETS,
                SECTION_SHORTCUTS_BINDINGS,
                SECTION_SHORTCUTS_GAMING,
                SECTION_DESKTOP_SNAP,
                SECTION_DESKTOP_PAGER,
            ])
            .collect();
        assert_eq!(sections.len(), ALL_SECTIONS.len() + 6);
        let mut seen = BTreeSet::new();
        for panel in NATIVE_PANELS {
            assert!(seen.insert(panel.id), "duplicate {}", panel.id);
            assert!(tabs.contains(panel.tab), "{}", panel.id);
            assert!(sections.contains(panel.section), "{}", panel.id);
            assert!(!panel.label.is_empty(), "{}", panel.id);
        }
        assert_eq!(NATIVE_PANELS.len(), 6);
        for wanted in [
            "native.takeover-policy",
            "native.win-l-remediation",
            "native.shortcut-presets",
            "native.binding-editor",
            "native.gaming-operations",
            "native.snap-policy",
        ] {
            assert!(seen.contains(wanted), "{wanted}");
        }
    }
}

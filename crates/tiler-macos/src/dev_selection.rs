#![forbid(unsafe_code)]

//! Product selection for the macOS dogfood preview: scoped allowlist vs
//! unscoped ordinary-app tiling, plus the tentative user settings file.
//!
//! The tap remains the sole product input route over the shared basic
//! catalogue (8 focus + 8 move + float `G`; see
//! [`crate::native_input::tile_catalogue`]); this module only decides WHICH
//! apps a run may observe and actuate:
//!
//! - Scoped: an explicit non-empty bundle allowlist. Ghostty
//!   (any case/variant) never validates and never actuates in scoped mode.
//!   The signed bundled helper ([`crate::HELPER_BUNDLE_ID`]) admits when
//!   listed explicitly (fixture exception); the own probe bundle
//!   ([`crate::BUNDLE_ID`]) never does.
//! - Unscoped (user dogfood): every ordinary app on each display's current
//!   user Space. Ghostty is an ordinary product candidate here, as are both
//!   own product bundles when observed from a different process (M1:
//!   own-process protection is pid-based at launch, never bundle-wide).
//!   Optional exact bundle-ID exclusions still apply. Unknown (`None`/empty)
//!   bundles never authorize in either mode.
//!
//! Identity, Space-membership, show-state, and topology-revision fences stay
//! in the existing per-app AX owners and effect gates; this module only
//! answers the scope question those gates ask.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::active_border::{
    ActiveBorderOptions, DEFAULT_COLOR_RGB, DEFAULT_USE_THEME, parse_color,
};
use crate::config::ProbeModifier;
use crate::input_policy::InputPreset;
use crate::scope::AppScope;

/// Product selection for one tile run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TileScope {
    /// Explicit bundle allowlist (Ghostty refused).
    Scoped(AppScope),
    /// All ordinary apps minus own bundles and exact exclusions.
    Unscoped { excluded: Vec<String> },
}

impl TileScope {
    /// Disabled scoped selection (legacy fixtures; authorizes nothing).
    #[must_use]
    pub fn scoped_disabled() -> Self {
        Self::Scoped(AppScope::disabled())
    }

    /// True only for the unscoped product mode.
    #[must_use]
    pub const fn is_unscoped(&self) -> bool {
        matches!(self, Self::Unscoped { .. })
    }

    /// Validate the selection itself (no OS reads).
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::Scoped(scope) => scope.validate(),
            Self::Unscoped { excluded } => {
                for bundle in excluded {
                    if bundle.is_empty() {
                        return Err("exclusion list must not contain empty bundle IDs".to_owned());
                    }
                }
                Ok(())
            }
        }
    }

    /// True when `bundle_id` is one of the crate's own product bundles
    /// (probe or signed helper): never an unscoped candidate. The helper
    /// stays admittable ONLY as an explicit scoped fixture member.
    #[must_use]
    pub fn is_product_bundle(bundle_id: &str) -> bool {
        bundle_id == crate::BUNDLE_ID || bundle_id == crate::HELPER_BUNDLE_ID
    }

    /// Effect-boundary scope check: scoped delegates to the allowlist gate
    /// (Ghostty/own/protected/unknown refused); unscoped admits any
    /// non-empty, non-excluded bundle — including Ghostty and both own
    /// product bundles when they arrive from a different process — as
    /// ordinary product candidates. Unknown (`None`) never authorizes in
    /// either mode. Own-process protection stays pid-based at launch
    /// (`launchable` refuses the current pid); M1 keeps no bundle-wide
    /// probe/helper exclusion unscoped. Scoped fixture/test fences are
    /// unchanged (see `AppScope`).
    #[must_use]
    pub fn allows_effect(&self, bundle: Option<&str>) -> bool {
        match self {
            Self::Scoped(scope) => scope.allows_effect(bundle),
            Self::Unscoped { excluded } => match bundle {
                Some(id) => !id.is_empty() && !excluded.contains(&id.to_owned()),
                None => false,
            },
        }
    }

    /// Admission check twin of [`allows_effect`] (same rule; the tile
    /// controller consults this at discovery, the backends at the boundary).
    #[must_use]
    pub fn allows(&self, bundle: Option<&str>) -> bool {
        self.allows_effect(bundle)
    }
}

/// Whether a discovered `(pid, bundle)` may be launched under `selection`:
/// live-PID range plus the selection's effect gate (own bundles, unknown
/// bundles, scoped Ghostty/protected/out-of-scope, and unscoped exclusions
/// never pass), never the process itself.
#[must_use]
pub fn launchable(selection: &TileScope, pid: u32, bundle: &str) -> bool {
    if pid <= 1 || pid == std::process::id() {
        return false;
    }
    if bundle.is_empty() {
        return false;
    }
    selection.allows_effect(Some(bundle))
}

// -- Tentative user settings file --------------------------------------------

/// Settings file name under the per-user application-support directory.
pub const SETTINGS_FILE_NAME: &str = "settings.json";
/// Application-support subdirectory for the settings file.
pub const SETTINGS_APP_DIR: &str = "OmniTiler";

/// Tentative user settings (all optional; CLI flags override each field).
/// Unknown fields refuse loudly so typos never silently no-op.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DevSettings {
    /// Ctrl-role modifier override (else the CLI `--ctrl`, else Ctrl).
    pub modifier: Option<ProbeModifier>,
    /// Input preset override (else the CLI `--preset`, else Authentic).
    pub preset: Option<InputPreset>,
    /// Exact bundle-ID exclusions for unscoped runs (else CLI `--exclude`).
    pub excluded_bundles: Option<Vec<String>>,
    /// Active-border section (else CLI `--active-border-*`, else defaults).
    pub active_border: Option<ActiveBorderSettings>,
    /// Pager-highlight section (else CLI `--pager-highlight-*`, else
    /// defaults: fallback `#2a82da`, theme on).
    pub pager_highlight: Option<PagerHighlightSettings>,
    /// Workspace-indicator section (else CLI `--workspace-indicator*`, else
    /// defaults: all rows, no icons).
    pub workspace_indicator: Option<WorkspaceIndicatorSettings>,
    /// Drag-preview section (else CLI `--drag-preview-*`, else defaults).
    pub drag_preview: Option<crate::drag_drop::DragPreviewSettings>,
}

/// Active-border settings file section (`"active_border"` object; every
/// field optional; CLI wins per field). Key naming mirrors the Windows
/// settings catalog fields (`border.enabled/width/gap/radius/color/use-theme`,
/// storage `core.border.*`) under the `--active-border-*` CLI prefix:
/// width 3, gap 0, host-derived centerline radius (17.5 on Tahoe 26+,
/// 10.5 older/unknown with default width/gap), fallback `#2a82da`, theme
/// on, switch on.
///
/// The four core-owned keys (`enabled/width/gap/radius`) ride to core as
/// raw acquisition values (see [`collect_raw_core_settings`]): core owns
/// validation/defaulting with per-field diagnostics, so the adapter never
/// refuses them here. `width/gap/radius` stay `serde_json::Value` (not
/// `f64`) so a wrong-typed file value (`"wide"`, `true`) survives loading
/// and reaches core diagnostics instead of refusing at file parse.
/// `color/use_theme` stay host-local (no core wire slot): they keep the
/// strict local validation below.
/// Presence-preserving raw acquisition field: absent stays `None`
/// (key omitted, core diagnoses `Missing` on defaults) while an explicit
/// JSON `null` loads as `Some(Value::Null)` (forwarded verbatim, core
/// diagnoses `WrongType`). Stock `Option` would erase explicit null into
/// missing; this keeps the two truthfully distinct for all four core keys.
fn de_opt_raw<'de, D>(deserializer: D) -> Result<Option<serde_json::Value>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    serde_json::Value::deserialize(deserializer).map(Some)
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ActiveBorderSettings {
    /// Master switch (default on). Raw: forwarded verbatim to core, which
    /// validates (`"bad"`, `null`, `[]` diagnose `WrongType` there).
    #[serde(default, deserialize_with = "de_opt_raw")]
    pub enabled: Option<serde_json::Value>,
    /// Ring thickness in points. Raw: forwarded verbatim to core.
    #[serde(default, deserialize_with = "de_opt_raw")]
    pub width: Option<serde_json::Value>,
    /// Gap between target frame and ring in points. Raw: forwarded
    /// verbatim to core, which validates.
    #[serde(default, deserialize_with = "de_opt_raw")]
    pub gap: Option<serde_json::Value>,
    /// Centerline stroke-path corner radius in points. Raw: forwarded
    /// verbatim to core.
    #[serde(default, deserialize_with = "de_opt_raw")]
    pub radius: Option<serde_json::Value>,
    /// Fallback colour as `#rrggbb` (default `#2a82da`).
    pub color: Option<String>,
    /// Follow the live system accent (default on).
    pub use_theme: Option<bool>,
}

impl ActiveBorderSettings {
    /// Validate merged file content (no OS reads). Only host-local fields
    /// validate here (`color`); the core-owned numeric keys (`width/gap/
    /// radius`) never refuse adapter-side — core owns their
    /// validation/defaulting with per-field diagnostics.
    pub fn validate(&self) -> Result<(), String> {
        if let Some(color) = &self.color {
            parse_color(color).map_err(|reason| format!("settings: {reason}"))?;
        }
        Ok(())
    }
}

/// Raw border CLI selections (all `None` = unset). Strings stay raw so
/// `Command` keeps its `Eq` derive; parsing happens in
/// [`resolve_border_config`] with the shared bounds (Windows parity:
/// `--no-active-border`, `--active-border-width/gap/radius/color`,
/// `--active-border-theme` / `--no-active-border-theme`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BorderCli {
    /// Master switch (`Some(false)` via `--no-active-border`).
    pub enabled: Option<bool>,
    /// Ring thickness `0..=32`.
    pub width: Option<String>,
    /// Target-to-ring gap `0..=64`.
    pub gap: Option<String>,
    /// Centerline stroke-path corner radius `0..=64` (explicit values apply
    /// verbatim; unset derives the host default after width/gap resolve).
    pub radius: Option<String>,
    /// Fallback colour `#rrggbb`.
    pub color: Option<String>,
    /// Theme source (`Some(true)` via `--active-border-theme`,
    /// `Some(false)` via `--no-active-border-theme`).
    pub use_theme: Option<bool>,
}

/// Merge CLI border selections over the settings file: an explicitly passed
/// CLI flag always wins; otherwise the file value applies; otherwise the
/// core catalog default (on, width 3, gap 0, radius 6, `#2a82da`, theme on).
///
/// This merger feeds the host-local renderer only. Core-owned numeric keys
/// (`enabled/width/gap/radius`) never validate here: a wrong-typed or
/// non-finite value falls back to the core catalog default for the renderer
/// while the raw value still rides to core via
/// [`collect_raw_core_settings`], which diagnoses per field. Only
/// host-local fields (`color`) refuse closed (never a silent fallback).
/// Border configuration is startup-only: a running tile session never
/// re-reads it.
pub fn resolve_border_config(
    cli: &BorderCli,
    file: Option<&ActiveBorderSettings>,
) -> Result<ActiveBorderOptions, String> {
    let enabled = cli
        .enabled
        .or_else(|| match file.and_then(|f| f.enabled.as_ref()) {
            Some(serde_json::Value::Bool(b)) => Some(*b),
            _ => None,
        })
        .unwrap_or(true);
    // Renderer-side numeric view of a core-owned key: finite numbers apply
    // verbatim with no range check (core owns OutOfRange diagnostics);
    // anything else falls back to the core catalog default without refusing.
    fn carrier_number(raw: Option<&str>, file: Option<&serde_json::Value>, fallback: f64) -> f64 {
        if let Some(text) = raw {
            if let Ok(parsed) = text.parse::<f64>()
                && parsed.is_finite()
            {
                return parsed;
            }
            return fallback;
        }
        match file {
            Some(serde_json::Value::Number(n)) => {
                let parsed = n.as_f64().unwrap_or(f64::NAN);
                if parsed.is_finite() { parsed } else { fallback }
            }
            _ => fallback,
        }
    }
    let width = carrier_number(
        cli.width.as_deref(),
        file.and_then(|f| f.width.as_ref()),
        tiler_core::settings_catalog::DEFAULT_CATALOG_BORDER_WIDTH,
    );
    let gap = carrier_number(
        cli.gap.as_deref(),
        file.and_then(|f| f.gap.as_ref()),
        tiler_core::settings_catalog::DEFAULT_CATALOG_BORDER_GAP,
    );
    let radius = carrier_number(
        cli.radius.as_deref(),
        file.and_then(|f| f.radius.as_ref()),
        tiler_core::settings_catalog::DEFAULT_CATALOG_BORDER_RADIUS,
    );
    let color = match &cli.color {
        Some(raw) => parse_color(raw)?,
        None => match file.and_then(|f| f.color.as_ref()) {
            Some(raw) => parse_color(raw).map_err(|reason| format!("settings: {reason}"))?,
            None => DEFAULT_COLOR_RGB,
        },
    };
    let use_theme = cli
        .use_theme
        .or(file.and_then(|f| f.use_theme))
        .unwrap_or(DEFAULT_USE_THEME);
    Ok(ActiveBorderOptions {
        enabled,
        style: crate::active_border::ActiveBorderStyle {
            width,
            gap,
            radius,
            color,
            use_theme,
        },
    })
}

/// Raw core wire keys for the active-border section, in stable order.
const CORE_BORDER_KEYS: [(&str, &str); 4] = [
    ("borderEnabled", "enabled"),
    ("borderWidth", "width"),
    ("borderGap", "gap"),
    ("borderRadius", "radius"),
];

/// Convert one raw JSON acquisition value to its core wire form,
/// preserving JSON types and values verbatim (no adapter validation,
/// no silent defaulting). Full-document syntax errors still refuse at
/// [`load_settings`]; per-field wrong types ride through here so core
/// emits per-field diagnostics.
#[must_use]
pub fn raw_setting_value_from_json(
    value: &serde_json::Value,
) -> tiler_core::connection::RawSettingValue {
    use tiler_core::connection::RawSettingValue as Raw;
    match value {
        serde_json::Value::Null => Raw::Null,
        serde_json::Value::Bool(b) => Raw::Bool(*b),
        serde_json::Value::Number(n) => {
            if let Some(parsed) = n.as_f64() {
                Raw::Number(parsed)
            } else {
                Raw::String(n.to_string())
            }
        }
        serde_json::Value::String(s) => Raw::String(s.clone()),
        serde_json::Value::Array(items) => {
            Raw::Array(items.iter().map(raw_setting_value_from_json).collect())
        }
        serde_json::Value::Object(map) => Raw::Object(
            map.iter()
                .map(|(k, v)| (k.clone(), raw_setting_value_from_json(v)))
                .collect(),
        ),
    }
}

/// Convert one raw CLI string for a core numeric key to its wire form.
/// Numeric strings that parse to a finite `f64` ride as `Number` verbatim
/// (even when out of core range — core diagnoses `OutOfRange`); anything
/// else (non-numeric text, `nan`/`inf` spellings the JSON codec cannot
/// carry as numbers) rides as `String` so core diagnoses `WrongType`.
#[must_use]
fn raw_core_number_from_cli(text: &str) -> tiler_core::connection::RawSettingValue {
    match text.parse::<f64>() {
        Ok(parsed) if parsed.is_finite() => tiler_core::connection::RawSettingValue::Number(parsed),
        _ => tiler_core::connection::RawSettingValue::String(text.to_owned()),
    }
}

/// Collect the raw core-owned border settings for the revisioned settings
/// route: the four core wire keys (`borderEnabled/borderWidth/borderGap/
/// borderRadius`) with CLI-over-file precedence per key. An explicitly
/// passed CLI flag always wins (even a non-numeric string, which core
/// diagnoses); otherwise the raw file value applies verbatim (even a
/// wrong-typed JSON value, which core diagnoses); otherwise the key is
/// omitted and core applies its default (`Missing` diagnostic). Never
/// refuses, never defaults adapter-side: core owns validation/defaulting
/// with per-field diagnostics while valid sibling fields still apply.
#[must_use]
pub fn collect_raw_core_settings(
    cli: &BorderCli,
    file: Option<&ActiveBorderSettings>,
) -> Vec<(String, tiler_core::connection::RawSettingValue)> {
    use tiler_core::connection::RawSettingValue as Raw;
    let mut values = Vec::with_capacity(CORE_BORDER_KEYS.len());
    if let Some(enabled) = cli.enabled.map(Raw::Bool).or_else(|| {
        file.and_then(|f| f.enabled.as_ref())
            .map(raw_setting_value_from_json)
    }) {
        values.push(("borderEnabled".to_owned(), enabled));
    }
    let file_value = |name: &str| -> Option<&serde_json::Value> {
        let section = file?;
        match name {
            "width" => section.width.as_ref(),
            "gap" => section.gap.as_ref(),
            "radius" => section.radius.as_ref(),
            _ => None,
        }
    };
    for (wire, name, cli_raw) in [
        ("borderWidth", "width", cli.width.as_deref()),
        ("borderGap", "gap", cli.gap.as_deref()),
        ("borderRadius", "radius", cli.radius.as_deref()),
    ] {
        debug_assert!(CORE_BORDER_KEYS.iter().any(|(key, _)| *key == wire));
        if let Some(text) = cli_raw {
            values.push((wire.to_owned(), raw_core_number_from_cli(text)));
        } else if let Some(raw) = file_value(name) {
            values.push((wire.to_owned(), raw_setting_value_from_json(raw)));
        }
    }
    values
}

/// Pager-highlight settings file section (`"pager_highlight"` object; every
/// field optional; CLI wins per field). Key naming mirrors the Windows
/// settings catalog fields (`core.pager_highlight.color/use-theme`)
/// under the `--pager-highlight-*` CLI prefix: fallback `#2a82da`, theme
/// on. Independent from the active-border tokens: an override here never
/// touches the ring, and a border override never touches the focused
/// miniature.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PagerHighlightSettings {
    /// Fallback colour as `#rrggbb` (default `#2a82da`).
    pub color: Option<String>,
    /// Follow the live system accent (default on).
    pub use_theme: Option<bool>,
}

impl PagerHighlightSettings {
    /// Validate merged file content (no OS reads).
    pub fn validate(&self) -> Result<(), String> {
        if let Some(color) = &self.color {
            parse_color(color).map_err(|reason| format!("settings: {reason}"))?;
        }
        Ok(())
    }
}

/// Raw pager-highlight CLI selections (all `None` = unset). Strings stay raw
/// so `Command` keeps its `Eq` derive; parsing happens in
/// [`resolve_pager_highlight_config`] with the shared colour bound (Windows
/// parity: `--pager-highlight-color`, `--pager-highlight-theme` /
/// `--no-pager-highlight-theme`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PagerHighlightCli {
    /// Fallback colour `#rrggbb`.
    pub color: Option<String>,
    /// Theme source (`Some(true)` via `--pager-highlight-theme`,
    /// `Some(false)` via `--no-pager-highlight-theme`).
    pub use_theme: Option<bool>,
}

/// Merge CLI pager-highlight selections over the settings file: an
/// explicitly passed CLI flag always wins; otherwise the file value applies;
/// otherwise the built-in default (`#2a82da`, theme on). Failures refuse
/// closed (never a silent fallback). Pager-highlight configuration is
/// startup-only: a running tile session never re-reads it; the live accent
/// itself resolves per paint, never a frozen boot snapshot.
pub fn resolve_pager_highlight_config(
    cli: &PagerHighlightCli,
    file: Option<&PagerHighlightSettings>,
) -> Result<crate::pager_highlight::PagerHighlightStyle, String> {
    let color = match &cli.color {
        Some(raw) => parse_color(raw)?,
        None => match file.and_then(|f| f.color.as_ref()) {
            Some(raw) => parse_color(raw).map_err(|reason| format!("settings: {reason}"))?,
            None => crate::pager_highlight::DEFAULT_COLOR_RGB,
        },
    };
    let use_theme = cli
        .use_theme
        .or(file.and_then(|f| f.use_theme))
        .unwrap_or(crate::pager_highlight::DEFAULT_USE_THEME);
    Ok(crate::pager_highlight::PagerHighlightStyle { color, use_theme })
}

/// Workspace-indicator settings file section (`"workspace_indicator"`
/// object; every field optional; CLI wins per field). `mode` names the
/// displayed rows (`"all"` default: every existing native Space in native
/// order; `"current"`: only the current Space; `"off"`: no status item).
/// `icons` opts into compact representative application icons per row
/// (default off). Key naming mirrors the `--workspace-indicator*` CLI
/// prefix. Unknown mode strings refuse loudly at parse (typos never
/// silently no-op).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct WorkspaceIndicatorSettings {
    /// Display mode (default all rows).
    pub mode: Option<crate::workspace_indicator::IndicatorMode>,
    /// Representative app icons per row (default off).
    pub icons: Option<bool>,
}

impl DevSettings {
    /// Validate merged file content (no OS reads).
    pub fn validate(&self) -> Result<(), String> {
        if let Some(excluded) = &self.excluded_bundles {
            for bundle in excluded {
                if bundle.is_empty() {
                    return Err(
                        "settings: excluded_bundles must not contain empty bundle IDs".to_owned(),
                    );
                }
            }
        }
        if let Some(border) = &self.active_border {
            border.validate()?;
        }
        if let Some(highlight) = &self.pager_highlight {
            highlight.validate()?;
        }
        if let Some(drag) = &self.drag_preview {
            drag.validate()?;
        }
        Ok(())
    }
}

/// Settings path for one home directory (`None` without a home): the file
/// lives under the per-user application-support directory; the path is
/// derived at runtime, never hardcoded.
#[must_use]
pub fn settings_path_for(home: Option<&str>) -> Option<PathBuf> {
    home.map(|h| {
        PathBuf::from(h)
            .join("Library")
            .join("Application Support")
            .join(SETTINGS_APP_DIR)
            .join(SETTINGS_FILE_NAME)
    })
}

/// Runtime settings path from `$HOME` (`None` when unset).
#[must_use]
pub fn settings_path() -> Option<PathBuf> {
    settings_path_for(std::env::var("HOME").ok().as_deref())
}

/// How the settings file resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsSource {
    /// No path (no `$HOME`) or no file there: built-in defaults apply.
    Absent,
    /// File present and valid.
    File,
}

/// Load and validate the tentative user settings file. A missing file (or
/// no home) maps to defaults; malformed content or validation failures fail
/// closed with the path and reason (never a silent fallback).
pub fn load_settings() -> Result<(DevSettings, SettingsSource), String> {
    let Some(path) = settings_path() else {
        return Ok((DevSettings::default(), SettingsSource::Absent));
    };
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok((DevSettings::default(), SettingsSource::Absent));
        }
        Err(e) => {
            return Err(format!("settings: cannot read {}: {e}", path.display()));
        }
    };
    let text = std::str::from_utf8(&bytes)
        .map_err(|_| format!("settings: {} is not valid UTF-8", path.display()))?;
    let settings: DevSettings = serde_json::from_str(text)
        .map_err(|e| format!("settings: {} is malformed: {e}", path.display()))?;
    settings
        .validate()
        .map_err(|e| format!("settings: {}: {e}", path.display()))?;
    Ok((settings, SettingsSource::File))
}

/// Effective dev configuration after CLI-overrides-file resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedDev {
    pub modifier: ProbeModifier,
    pub preset: InputPreset,
    pub excluded: Vec<String>,
}

/// Merge CLI selections (with per-field explicitness) over the settings
/// file: an explicitly passed CLI flag always wins; otherwise the file
/// value applies; otherwise the built-in default. CLI exclusions win when
/// non-empty, else the file list — but only in unscoped mode: scoped runs
/// never inherit file exclusions (an explicit CLI `--exclude` with a scope
/// still refuses later in request validation). The merged exclusions
/// validate here.
#[allow(clippy::too_many_arguments)]
pub fn resolve_dev_config(
    unscoped: bool,
    cli_modifier: ProbeModifier,
    modifier_explicit: bool,
    cli_preset: InputPreset,
    preset_explicit: bool,
    cli_excluded: &[String],
    file: &DevSettings,
) -> Result<ResolvedDev, String> {
    let modifier = if modifier_explicit {
        cli_modifier
    } else {
        file.modifier.unwrap_or(cli_modifier)
    };
    let preset = if preset_explicit {
        cli_preset
    } else {
        file.preset.unwrap_or(cli_preset)
    };
    let file_excluded = if unscoped {
        file.excluded_bundles.clone().unwrap_or_default()
    } else {
        Vec::new()
    };
    let excluded = if cli_excluded.is_empty() {
        file_excluded
    } else {
        cli_excluded.to_vec()
    };
    for bundle in &excluded {
        if bundle.is_empty() {
            return Err("exclusion list must not contain empty bundle IDs".to_owned());
        }
    }
    Ok(ResolvedDev {
        modifier,
        preset,
        excluded,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unscoped(excluded: &[&str]) -> TileScope {
        TileScope::Unscoped {
            excluded: excluded.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn scoped(allowed: &[&str]) -> TileScope {
        TileScope::Scoped(AppScope {
            allowed: allowed.iter().map(|s| s.to_string()).collect(),
        })
    }

    #[test]
    fn ghostty_is_ordinary_unscoped_but_refused_scoped() {
        // Only the scoped mode refuses Ghostty: unscoped admits it (any
        // case/variant) as an ordinary product candidate.
        for id in [
            "com.mitchellh.ghostty",
            "COM.MITCHELLH.GHOSTTY",
            "com.mitchellh.ghostty-beta",
        ] {
            assert!(unscoped(&[]).allows_effect(Some(id)), "{id}");
            assert!(launchable(&unscoped(&[]), 4242, id), "{id}");
            assert!(!scoped(&["com.mitchellh.ghostty"]).allows_effect(Some(id)));
        }
        // Scoped validation refuses Ghostty allowlists outright.
        assert!(scoped(&["com.mitchellh.ghostty"]).validate().is_err());
    }

    #[test]
    fn own_bundles_scoped_fence_holds_unscoped_admits_other_processes() {
        // Scoped fences are unchanged: the own probe bundle never authorizes
        // anywhere scoped, even listed.
        assert!(!scoped(&[crate::BUNDLE_ID]).allows_effect(Some(crate::BUNDLE_ID)));
        // The signed helper stays admittable as an explicit scoped fixture
        // member — the only scoped exception.
        assert!(scoped(&[crate::HELPER_BUNDLE_ID]).validate().is_ok());
        assert!(scoped(&[crate::HELPER_BUNDLE_ID]).allows_effect(Some(crate::HELPER_BUNDLE_ID)));
        assert!(!scoped(&["com.example.app"]).allows_effect(Some(crate::HELPER_BUNDLE_ID)));
        // M1 unscoped: same bundle in a different process is eligible (no
        // bundle-wide probe/helper exclusion). Own-process protection stays
        // pid-based at launch: a non-self pid with the probe bundle admits
        // at the effect gate, while the current pid never launches.
        assert!(unscoped(&[]).allows_effect(Some(crate::BUNDLE_ID)));
        assert!(unscoped(&[]).allows_effect(Some(crate::HELPER_BUNDLE_ID)));
        assert!(!launchable(
            &unscoped(&[]),
            std::process::id(),
            crate::BUNDLE_ID
        ));
        // Explicit unscoped "exclusions" of own bundles cannot re-admit or
        // re-refuse beyond the exact exclusion rule: listing the probe bundle
        // as excluded refuses it, otherwise it admits.
        assert!(!unscoped(&[crate::BUNDLE_ID]).allows_effect(Some(crate::BUNDLE_ID)));
        assert!(unscoped(&[]).allows_effect(Some(crate::BUNDLE_ID)));
    }

    #[test]
    fn exclusions_are_exact_and_unscoped_only() {
        let selection = unscoped(&["com.example.skip"]);
        assert!(!selection.allows_effect(Some("com.example.skip")));
        assert!(!launchable(&selection, 4242, "com.example.skip"));
        // Exact only: case differs, or merely a prefix, still admits.
        assert!(selection.allows_effect(Some("com.example.Skip")));
        assert!(selection.allows_effect(Some("com.example.skip2")));
        assert!(selection.allows_effect(Some("com.example.keep")));
        assert!(selection.validate().is_ok());
        assert!(unscoped(&[""]).validate().is_err());
    }

    #[test]
    fn unknown_bundles_never_authorize() {
        assert!(!unscoped(&[]).allows_effect(None));
        assert!(!scoped(&["com.example.app"]).allows_effect(None));
        assert!(!unscoped(&[]).allows_effect(Some("")));
        assert!(!launchable(&unscoped(&[]), 4242, ""));
        assert!(!launchable(&unscoped(&[]), 0, "com.example.app"));
        assert!(!launchable(&unscoped(&[]), 1, "com.example.app"));
        assert!(!launchable(
            &unscoped(&[]),
            std::process::id(),
            "com.example.app"
        ));
    }

    #[test]
    fn scoped_ordinary_apps_admit_when_listed() {
        let selection = scoped(&["com.example.app"]);
        assert!(selection.allows_effect(Some("com.example.app")));
        assert!(launchable(&selection, 4242, "com.example.app"));
        assert!(!selection.allows_effect(Some("com.example.other")));
        // The bundled helper admits ONLY as an explicit scoped member.
        let helper = scoped(&[crate::HELPER_BUNDLE_ID]);
        assert!(helper.validate().is_ok());
        assert!(helper.allows_effect(Some(crate::HELPER_BUNDLE_ID)));
    }

    #[test]
    fn settings_path_derives_from_home() {
        let path = settings_path_for(Some("/example-home")).expect("path");
        assert_eq!(
            path,
            PathBuf::from("/example-home/Library/Application Support/OmniTiler/settings.json")
        );
        assert!(settings_path_for(None).is_none());
    }

    #[test]
    fn settings_parse_validates_and_refuses_unknown_fields() {
        let settings: DevSettings = serde_json::from_str(
            r#"{"modifier":"meta","preset":"compatible","excluded_bundles":["com.example.skip"]}"#,
        )
        .expect("parses");
        assert_eq!(settings.modifier, Some(ProbeModifier::Meta));
        assert_eq!(settings.preset, Some(InputPreset::Compatible));
        assert!(settings.validate().is_ok());
        // Unknown fields refuse loudly (typos never silently no-op).
        assert!(serde_json::from_str::<DevSettings>(r#"{"meta":"ctrl"}"#).is_err());
        // Empty exclusions refuse.
        let bad = DevSettings {
            excluded_bundles: Some(vec![String::new()]),
            ..DevSettings::default()
        };
        assert!(bad.validate().is_err());
        // Missing file content maps to defaults.
        let empty: DevSettings = serde_json::from_str("{}").expect("parses");
        assert_eq!(empty, DevSettings::default());
    }

    #[test]
    fn cli_overrides_file_over_defaults() {
        let file = DevSettings {
            modifier: Some(ProbeModifier::Meta),
            preset: Some(InputPreset::Compatible),
            excluded_bundles: Some(vec!["com.example.skip".to_owned()]),
            active_border: None,
            pager_highlight: None,
            workspace_indicator: None,
            drag_preview: None,
        };
        // Nothing explicit: file wins over built-in defaults (unscoped, so
        // the file exclusions apply).
        let resolved = resolve_dev_config(
            true,
            ProbeModifier::Ctrl,
            false,
            InputPreset::Authentic,
            false,
            &[],
            &file,
        )
        .expect("resolves");
        assert_eq!(
            resolved,
            ResolvedDev {
                modifier: ProbeModifier::Meta,
                preset: InputPreset::Compatible,
                excluded: vec!["com.example.skip".to_owned()],
            }
        );
        // Explicit CLI wins over the file.
        let resolved = resolve_dev_config(
            true,
            ProbeModifier::Alt,
            true,
            InputPreset::Authentic,
            true,
            &["com.example.cli".to_owned()],
            &file,
        )
        .expect("resolves");
        assert_eq!(
            resolved,
            ResolvedDev {
                modifier: ProbeModifier::Alt,
                preset: InputPreset::Authentic,
                excluded: vec!["com.example.cli".to_owned()],
            }
        );
        // Empty CLI exclusions fall back to the file list.
        let resolved = resolve_dev_config(
            true,
            ProbeModifier::Ctrl,
            false,
            InputPreset::Authentic,
            false,
            &[],
            &DevSettings::default(),
        )
        .expect("resolves");
        assert_eq!(resolved.excluded, Vec::<String>::new());
        // Empty CLI exclusion entries refuse.
        assert!(
            resolve_dev_config(
                true,
                ProbeModifier::Ctrl,
                false,
                InputPreset::Authentic,
                false,
                &[String::new()],
                &DevSettings::default(),
            )
            .is_err()
        );
    }

    #[test]
    fn scoped_runs_never_inherit_file_exclusions() {
        // Exclusions are an unscoped-only field: a scoped run ignores the
        // file list (modifier/preset still resolve CLI-over-file).
        let file = DevSettings {
            modifier: Some(ProbeModifier::Meta),
            preset: Some(InputPreset::Compatible),
            excluded_bundles: Some(vec!["com.example.skip".to_owned()]),
            active_border: None,
            pager_highlight: None,
            workspace_indicator: None,
            drag_preview: None,
        };
        let resolved = resolve_dev_config(
            false,
            ProbeModifier::Ctrl,
            false,
            InputPreset::Authentic,
            false,
            &[],
            &file,
        )
        .expect("resolves");
        assert_eq!(resolved.modifier, ProbeModifier::Meta);
        assert_eq!(resolved.preset, InputPreset::Compatible);
        assert!(resolved.excluded.is_empty());
        // Explicit CLI modifier/preset still win for scoped runs.
        let resolved = resolve_dev_config(
            false,
            ProbeModifier::Alt,
            true,
            InputPreset::Authentic,
            true,
            &[],
            &file,
        )
        .expect("resolves");
        assert_eq!(resolved.modifier, ProbeModifier::Alt);
        assert_eq!(resolved.preset, InputPreset::Authentic);
        assert!(resolved.excluded.is_empty());
    }

    #[test]
    fn border_defaults_match_shared_parity() {
        // Nothing set anywhere: on, core catalog width 3 / gap 0 /
        // radius 6 (`#2a82da`, theme on). No host derivation: the renderer
        // carrier falls back to the core catalog defaults.
        let resolved = resolve_border_config(&BorderCli::default(), None).expect("resolves");
        assert!(resolved.enabled);
        assert_eq!(
            resolved.style.width,
            tiler_core::settings_catalog::DEFAULT_CATALOG_BORDER_WIDTH
        );
        assert_eq!(
            resolved.style.gap,
            tiler_core::settings_catalog::DEFAULT_CATALOG_BORDER_GAP
        );
        assert_eq!(
            resolved.style.radius,
            tiler_core::settings_catalog::DEFAULT_CATALOG_BORDER_RADIUS
        );
        assert_eq!(
            crate::active_border::render_color(resolved.style.color),
            "#2a82da"
        );
        assert!(resolved.style.use_theme);
    }

    #[test]
    fn border_carrier_passes_finite_verbatim_without_range_checks() {
        // Finite numbers apply verbatim with no adapter range check (core
        // owns OutOfRange); unset keys fall back to the core catalog
        // default. Radius never derives from width/gap/host.
        let file = ActiveBorderSettings {
            width: Some(serde_json::json!(5.0)),
            gap: Some(serde_json::json!(2.0)),
            ..ActiveBorderSettings::default()
        };
        let resolved = resolve_border_config(&BorderCli::default(), Some(&file)).expect("resolves");
        assert_eq!(resolved.style.width, 5.0);
        assert_eq!(resolved.style.gap, 2.0);
        assert_eq!(
            resolved.style.radius,
            tiler_core::settings_catalog::DEFAULT_CATALOG_BORDER_RADIUS
        );
        // Empty section is the same as absent: core catalog defaults.
        let empty = ActiveBorderSettings::default();
        let resolved =
            resolve_border_config(&BorderCli::default(), Some(&empty)).expect("resolves");
        assert_eq!(
            resolved.style.radius,
            tiler_core::settings_catalog::DEFAULT_CATALOG_BORDER_RADIUS
        );
        // Explicit radius (CLI or file) applies verbatim.
        let cli = BorderCli {
            radius: Some("12".to_owned()),
            ..BorderCli::default()
        };
        assert_eq!(
            resolve_border_config(&cli, Some(&file))
                .expect("resolves")
                .style
                .radius,
            12.0
        );
        // Out-of-range finite numbers ride verbatim (no adapter refusal).
        let wide = ActiveBorderSettings {
            width: Some(serde_json::json!(99.0)),
            gap: Some(serde_json::json!(-1.0)),
            radius: Some(serde_json::json!(65.0)),
            ..ActiveBorderSettings::default()
        };
        let resolved = resolve_border_config(&BorderCli::default(), Some(&wide)).expect("resolves");
        assert_eq!(resolved.style.width, 99.0);
        assert_eq!(resolved.style.gap, -1.0);
        assert_eq!(resolved.style.radius, 65.0);
        let cli = BorderCli {
            width: Some("32.1".to_owned()),
            ..BorderCli::default()
        };
        assert_eq!(
            resolve_border_config(&cli, None)
                .expect("resolves")
                .style
                .width,
            32.1
        );
    }

    #[test]
    fn border_file_applies_and_cli_wins_per_field() {
        let file = ActiveBorderSettings {
            enabled: Some(serde_json::json!(false)),
            width: Some(serde_json::json!(5.0)),
            gap: Some(serde_json::json!(2.0)),
            radius: Some(serde_json::json!(10.0)),
            color: Some("#112233".to_owned()),
            use_theme: Some(false),
        };
        // File alone applies wholesale (switch off included).
        let resolved = resolve_border_config(&BorderCli::default(), Some(&file)).expect("resolves");
        assert!(!resolved.enabled);
        assert_eq!(resolved.style.width, 5.0);
        assert_eq!(resolved.style.gap, 2.0);
        assert_eq!(resolved.style.radius, 10.0);
        assert_eq!(resolved.style.color, (0x11, 0x22, 0x33));
        assert!(!resolved.style.use_theme);
        // Explicit CLI wins per field; unset fields keep the file.
        let cli = BorderCli {
            width: Some("7".to_owned()),
            color: Some("#aabbcc".to_owned()),
            use_theme: Some(true),
            ..BorderCli::default()
        };
        let resolved = resolve_border_config(&cli, Some(&file)).expect("resolves");
        assert!(!resolved.enabled);
        assert_eq!(resolved.style.width, 7.0);
        assert_eq!(resolved.style.gap, 2.0);
        assert_eq!(resolved.style.color, (0xaa, 0xbb, 0xcc));
        assert!(resolved.style.use_theme);
        // `--no-active-border` switches off; `--active-border-theme`
        // re-enables the theme source over a file opt-out.
        let cli = BorderCli {
            enabled: Some(false),
            ..BorderCli::default()
        };
        assert!(!resolve_border_config(&cli, None).expect("resolves").enabled);
        let cli = BorderCli {
            use_theme: Some(true),
            ..BorderCli::default()
        };
        assert!(
            resolve_border_config(&cli, Some(&file))
                .expect("resolves")
                .style
                .use_theme
        );
        // Wrong-typed file `enabled` is ignored renderer-side (stays on)
        // while the raw lane still forwards it to core diagnostics.
        for bad in [
            serde_json::json!("bad"),
            serde_json::Value::Null,
            serde_json::json!([]),
            serde_json::json!(1),
        ] {
            let file = ActiveBorderSettings {
                enabled: Some(bad),
                ..ActiveBorderSettings::default()
            };
            assert!(
                resolve_border_config(&BorderCli::default(), Some(&file))
                    .expect("resolves")
                    .enabled
            );
        }
    }

    #[test]
    fn border_config_falls_back_on_bad_core_numbers_but_refuses_bad_color() {
        // Core-owned numeric keys never refuse adapter-side: a wrong-typed
        // or non-finite CLI/file value falls back to the core catalog
        // default for the renderer while the raw value still rides to core
        // (see `collect_raw_core_settings`), which diagnoses per field.
        // Finite numbers (even out-of-range) ride verbatim. Host-local
        // `color` still refuses.
        use tiler_core::settings_catalog as catalog;
        for (cli, width, gap) in [
            (
                BorderCli {
                    width: Some("abc".to_owned()),
                    ..BorderCli::default()
                },
                catalog::DEFAULT_CATALOG_BORDER_WIDTH,
                catalog::DEFAULT_CATALOG_BORDER_GAP,
            ),
            (
                BorderCli {
                    radius: Some("nan".to_owned()),
                    ..BorderCli::default()
                },
                catalog::DEFAULT_CATALOG_BORDER_WIDTH,
                catalog::DEFAULT_CATALOG_BORDER_GAP,
            ),
            (
                BorderCli {
                    width: Some("32.1".to_owned()),
                    ..BorderCli::default()
                },
                32.1,
                catalog::DEFAULT_CATALOG_BORDER_GAP,
            ),
            (
                BorderCli {
                    width: Some("-1".to_owned()),
                    ..BorderCli::default()
                },
                -1.0,
                catalog::DEFAULT_CATALOG_BORDER_GAP,
            ),
        ] {
            let resolved = resolve_border_config(&cli, None).expect("resolves");
            assert_eq!(resolved.style.width, width);
            assert_eq!(resolved.style.gap, gap);
        }
        // Wrong-typed file numbers fall back the same way (and validate
        // cleanly: core owns those diagnostics); out-of-range finite file
        // numbers ride verbatim.
        for (file, width, gap) in [
            (
                ActiveBorderSettings {
                    width: Some(serde_json::json!("wide")),
                    ..ActiveBorderSettings::default()
                },
                catalog::DEFAULT_CATALOG_BORDER_WIDTH,
                catalog::DEFAULT_CATALOG_BORDER_GAP,
            ),
            (
                ActiveBorderSettings {
                    radius: Some(serde_json::json!(true)),
                    ..ActiveBorderSettings::default()
                },
                catalog::DEFAULT_CATALOG_BORDER_WIDTH,
                catalog::DEFAULT_CATALOG_BORDER_GAP,
            ),
            (
                ActiveBorderSettings {
                    width: Some(serde_json::json!(33.0)),
                    ..ActiveBorderSettings::default()
                },
                33.0,
                catalog::DEFAULT_CATALOG_BORDER_GAP,
            ),
            (
                ActiveBorderSettings {
                    gap: Some(serde_json::json!(-1.0)),
                    ..ActiveBorderSettings::default()
                },
                catalog::DEFAULT_CATALOG_BORDER_WIDTH,
                -1.0,
            ),
        ] {
            assert!(file.validate().is_ok());
            let resolved =
                resolve_border_config(&BorderCli::default(), Some(&file)).expect("resolves");
            assert_eq!(resolved.style.width, width);
            assert_eq!(resolved.style.gap, gap);
        }
        // Host-local colour still refuses on both routes.
        for cli in [
            BorderCli {
                color: Some("2a82da".to_owned()),
                ..BorderCli::default()
            },
            BorderCli {
                color: Some("#fff".to_owned()),
                ..BorderCli::default()
            },
        ] {
            assert!(resolve_border_config(&cli, None).is_err());
        }
        let bad_color = ActiveBorderSettings {
            color: Some("#zzzzzz".to_owned()),
            ..ActiveBorderSettings::default()
        };
        assert!(bad_color.validate().is_err());
        assert!(resolve_border_config(&BorderCli::default(), Some(&bad_color)).is_err());
        // Boundary values pass on both routes.
        let cli = BorderCli {
            width: Some("32".to_owned()),
            gap: Some("64".to_owned()),
            radius: Some("64".to_owned()),
            color: Some("#2A82DA".to_owned()),
            ..BorderCli::default()
        };
        let resolved = resolve_border_config(&cli, None).expect("resolves");
        assert_eq!(resolved.style.width, 32.0);
        assert_eq!(resolved.style.color, (0x2a, 0x82, 0xda));
    }

    #[test]
    fn pager_highlight_defaults_match_shared_parity() {
        // Nothing set anywhere: fallback `#2a82da`, theme on
        // (Windows `pager_highlight` parity), independent of the ring.
        let resolved =
            resolve_pager_highlight_config(&PagerHighlightCli::default(), None).expect("resolves");
        assert_eq!(
            resolved,
            crate::pager_highlight::PagerHighlightStyle::default()
        );
        assert_eq!(resolved.color, (0x2a, 0x82, 0xda));
        assert!(resolved.use_theme);
    }

    #[test]
    fn pager_highlight_file_applies_and_cli_wins_per_field() {
        let file = PagerHighlightSettings {
            color: Some("#112233".to_owned()),
            use_theme: Some(false),
        };
        // File alone applies wholesale.
        let resolved = resolve_pager_highlight_config(&PagerHighlightCli::default(), Some(&file))
            .expect("resolves");
        assert_eq!(resolved.color, (0x11, 0x22, 0x33));
        assert!(!resolved.use_theme);
        // Explicit CLI wins per field; unset fields keep the file.
        let cli = PagerHighlightCli {
            color: Some("#aabbcc".to_owned()),
            use_theme: None,
        };
        let resolved = resolve_pager_highlight_config(&cli, Some(&file)).expect("resolves");
        assert_eq!(resolved.color, (0xaa, 0xbb, 0xcc));
        assert!(!resolved.use_theme);
        let cli = PagerHighlightCli {
            color: None,
            use_theme: Some(true),
        };
        let resolved = resolve_pager_highlight_config(&cli, Some(&file)).expect("resolves");
        assert_eq!(resolved.color, (0x11, 0x22, 0x33));
        assert!(resolved.use_theme);
    }

    #[test]
    fn pager_highlight_config_refuses_bad_values() {
        for cli in [
            PagerHighlightCli {
                color: Some("2a82da".to_owned()),
                use_theme: None,
            },
            PagerHighlightCli {
                color: Some("#fff".to_owned()),
                use_theme: None,
            },
        ] {
            assert!(resolve_pager_highlight_config(&cli, None).is_err());
        }
        let bad = PagerHighlightSettings {
            color: Some("#zzzzzz".to_owned()),
            use_theme: None,
        };
        assert!(bad.validate().is_err());
        assert!(resolve_pager_highlight_config(&PagerHighlightCli::default(), Some(&bad)).is_err());
        // A border override never touches the highlight and vice versa:
        // resolving one leaves the other at its default.
        let border_cli = BorderCli {
            color: Some("#ff0000".to_owned()),
            ..BorderCli::default()
        };
        let border = resolve_border_config(&border_cli, None).expect("resolves");
        let highlight =
            resolve_pager_highlight_config(&PagerHighlightCli::default(), None).expect("resolves");
        assert_eq!(border.style.color, (0xff, 0x00, 0x00));
        assert_eq!(highlight.color, (0x2a, 0x82, 0xda));
        assert!(highlight.use_theme);
    }

    #[test]
    fn pager_highlight_settings_file_section_parses_and_validates() {
        let settings: DevSettings =
            serde_json::from_str(r##"{"pager_highlight":{"color":"#112233","use_theme":false}}"##)
                .expect("parses");
        let section = settings.pager_highlight.as_ref().expect("section");
        assert_eq!(section.color.as_deref(), Some("#112233"));
        assert_eq!(section.use_theme, Some(false));
        assert!(settings.validate().is_ok());
        let resolved = resolve_pager_highlight_config(
            &PagerHighlightCli::default(),
            settings.pager_highlight.as_ref(),
        )
        .expect("resolves");
        assert_eq!(resolved.color, (0x11, 0x22, 0x33));
        assert!(!resolved.use_theme);
        // Unknown nested fields refuse loudly (typos never silently no-op).
        assert!(
            serde_json::from_str::<DevSettings>(r##"{"pager_highlight":{"colour":"#112233"}}"##)
                .is_err()
        );
        // A bad section fails the whole file closed.
        let bad: DevSettings = serde_json::from_str(r##"{"pager_highlight":{"color":"#zzzzzz"}}"##)
            .expect("parses shape");
        assert!(bad.validate().is_err());
        assert!(
            resolve_pager_highlight_config(
                &PagerHighlightCli::default(),
                bad.pager_highlight.as_ref()
            )
            .is_err()
        );
    }

    #[test]
    fn border_settings_file_section_parses_and_validates() {
        let settings: DevSettings = serde_json::from_str(
            r##"{"active_border":{"enabled":false,"width":5,"gap":1,"radius":8,"color":"#112233","use_theme":false}}"##,
        )
        .expect("parses");
        let section = settings.active_border.as_ref().expect("section");
        assert_eq!(section.enabled, Some(serde_json::json!(false)));
        assert_eq!(section.width, Some(serde_json::json!(5)));
        assert_eq!(section.color.as_deref(), Some("#112233"));
        assert!(settings.validate().is_ok());
        let resolved =
            resolve_border_config(&BorderCli::default(), settings.active_border.as_ref())
                .expect("resolves");
        assert!(!resolved.enabled);
        assert_eq!(resolved.style.radius, 8.0);
        // Unknown nested fields refuse loudly (typos never silently no-op).
        assert!(
            serde_json::from_str::<DevSettings>(r#"{"active_border":{"thicknes":5}}"#).is_err()
        );
        // A wrong-typed core number still parses and validates file-side:
        // core owns that diagnostic via the raw lane (never a file refusal).
        let raw: DevSettings =
            serde_json::from_str(r#"{"active_border":{"width":"wide"}}"#).expect("parses shape");
        assert!(raw.validate().is_ok());
        let values = collect_raw_core_settings(&BorderCli::default(), raw.active_border.as_ref());
        assert_eq!(
            values,
            vec![(
                "borderWidth".to_owned(),
                tiler_core::connection::RawSettingValue::String("wide".to_owned())
            )]
        );
        // An out-of-range finite core number rides verbatim renderer-side;
        // the raw value still rides to core diagnostics.
        let bad: DevSettings =
            serde_json::from_str(r#"{"active_border":{"width":99}}"#).expect("parses shape");
        assert!(bad.validate().is_ok());
        let resolved = resolve_border_config(&BorderCli::default(), bad.active_border.as_ref())
            .expect("resolves");
        assert_eq!(resolved.style.width, 99.0);
        let values = collect_raw_core_settings(&BorderCli::default(), bad.active_border.as_ref());
        assert_eq!(
            values,
            vec![(
                "borderWidth".to_owned(),
                tiler_core::connection::RawSettingValue::Number(99.0)
            )]
        );
        // Wrong-typed `enabled` variants parse file-side and ride raw to
        // core (WrongType there); absent omits (Missing on defaults).
        for (text, want) in [
            (
                r#"{"active_border":{"enabled":"bad"}}"#,
                tiler_core::connection::RawSettingValue::String("bad".to_owned()),
            ),
            (
                r#"{"active_border":{"enabled":null}}"#,
                tiler_core::connection::RawSettingValue::Null,
            ),
            (
                r#"{"active_border":{"enabled":[]}}"#,
                tiler_core::connection::RawSettingValue::Array(vec![]),
            ),
        ] {
            let parsed: DevSettings = serde_json::from_str(text).expect("parses shape");
            assert!(parsed.validate().is_ok());
            assert_eq!(
                collect_raw_core_settings(&BorderCli::default(), parsed.active_border.as_ref()),
                vec![("borderEnabled".to_owned(), want)]
            );
        }
        let absent: DevSettings =
            serde_json::from_str(r#"{"active_border":{"width":5}}"#).expect("parses");
        assert!(
            collect_raw_core_settings(&BorderCli::default(), absent.active_border.as_ref())
                .iter()
                .all(|(k, _)| k != "borderEnabled")
        );
        let explicit_null: DevSettings =
            serde_json::from_str(r#"{"active_border":{"enabled":null}}"#).expect("parses");
        assert_eq!(
            collect_raw_core_settings(&BorderCli::default(), explicit_null.active_border.as_ref()),
            vec![(
                "borderEnabled".to_owned(),
                tiler_core::connection::RawSettingValue::Null
            )]
        );
    }

    #[test]
    fn raw_core_settings_merge_cli_over_file_per_key() {
        use tiler_core::connection::RawSettingValue as Raw;
        // File alone maps to the four core wire keys verbatim.
        let file = ActiveBorderSettings {
            enabled: Some(serde_json::json!(true)),
            width: Some(serde_json::json!(5)),
            gap: Some(serde_json::json!(2)),
            radius: Some(serde_json::json!(10)),
            ..ActiveBorderSettings::default()
        };
        assert_eq!(
            collect_raw_core_settings(&BorderCli::default(), Some(&file)),
            vec![
                ("borderEnabled".to_owned(), Raw::Bool(true)),
                ("borderWidth".to_owned(), Raw::Number(5.0)),
                ("borderGap".to_owned(), Raw::Number(2.0)),
                ("borderRadius".to_owned(), Raw::Number(10.0)),
            ]
        );
        // Explicit CLI wins per key; unset CLI keys keep the file value.
        let cli = BorderCli {
            enabled: Some(false),
            width: Some("7".to_owned()),
            gap: None,
            radius: Some("abc".to_owned()),
            ..BorderCli::default()
        };
        assert_eq!(
            collect_raw_core_settings(&cli, Some(&file)),
            vec![
                ("borderEnabled".to_owned(), Raw::Bool(false)),
                ("borderWidth".to_owned(), Raw::Number(7.0)),
                ("borderGap".to_owned(), Raw::Number(2.0)),
                ("borderRadius".to_owned(), Raw::String("abc".to_owned())),
            ]
        );
        // Empty acquisition omits every key (core defaults apply).
        assert!(collect_raw_core_settings(&BorderCli::default(), None).is_empty());
        assert!(
            collect_raw_core_settings(
                &BorderCli::default(),
                Some(&ActiveBorderSettings::default())
            )
            .is_empty()
        );
        // Non-finite CLI spellings ride as strings: the JSON codec cannot
        // carry them as numbers, and core diagnoses the truthful wrong type.
        let cli = BorderCli {
            width: Some("nan".to_owned()),
            gap: Some("inf".to_owned()),
            radius: Some("-inf".to_owned()),
            ..BorderCli::default()
        };
        assert_eq!(
            collect_raw_core_settings(&cli, None),
            vec![
                ("borderWidth".to_owned(), Raw::String("nan".to_owned())),
                ("borderGap".to_owned(), Raw::String("inf".to_owned())),
                ("borderRadius".to_owned(), Raw::String("-inf".to_owned())),
            ]
        );
        // Out-of-range numerics ride verbatim (core diagnoses per field).
        let cli = BorderCli {
            width: Some("32.1".to_owned()),
            ..BorderCli::default()
        };
        assert_eq!(
            collect_raw_core_settings(&cli, None),
            vec![("borderWidth".to_owned(), Raw::Number(32.1))]
        );
    }
}

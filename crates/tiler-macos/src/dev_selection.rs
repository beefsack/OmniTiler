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
//!   user Space. Ghostty is an ordinary product candidate here. Both own
//!   bundles (probe and helper) stay excluded, plus optional exact
//!   bundle-ID exclusions. Unknown (`None`/empty) bundles never authorize
//!   in either mode.
//!
//! Identity, Space-membership, show-state, and topology-revision fences stay
//! in the existing per-app AX owners and effect gates; this module only
//! answers the scope question those gates ask.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::active_border::{
    ActiveBorderOptions, DEFAULT_COLOR_RGB, DEFAULT_GAP, DEFAULT_USE_THEME, DEFAULT_WIDTH, MAX_GAP,
    MAX_RADIUS, MAX_WIDTH, host_default_radius, parse_color, parse_gap, parse_radius, parse_width,
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
    /// non-empty, non-own, non-excluded bundle — including Ghostty, which
    /// is an ordinary product candidate outside the scoped allowlist mode.
    /// Unknown (`None`) never authorizes in either mode.
    #[must_use]
    pub fn allows_effect(&self, bundle: Option<&str>) -> bool {
        match self {
            Self::Scoped(scope) => scope.allows_effect(bundle),
            Self::Unscoped { excluded } => match bundle {
                Some(id) => {
                    !id.is_empty()
                        && !Self::is_product_bundle(id)
                        && !excluded.contains(&id.to_owned())
                }
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
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ActiveBorderSettings {
    /// Master switch (default on).
    pub enabled: Option<bool>,
    /// Ring thickness in points, `0..=32` (default 3).
    pub width: Option<f64>,
    /// Gap between target frame and ring in points, `0..=64` (default 0).
    pub gap: Option<f64>,
    /// Centerline stroke-path corner radius in points, `0..=64` (default
    /// host-derived: native base + gap + width / 2; explicit values apply
    /// verbatim, never re-offset).
    pub radius: Option<f64>,
    /// Fallback colour as `#rrggbb` (default `#2a82da`).
    pub color: Option<String>,
    /// Follow the live system accent (default on).
    pub use_theme: Option<bool>,
}

impl ActiveBorderSettings {
    /// Validate merged file content (no OS reads).
    pub fn validate(&self) -> Result<(), String> {
        if let Some(width) = self.width {
            file_bound(width, MAX_WIDTH, "width")?;
        }
        if let Some(gap) = self.gap {
            file_bound(gap, MAX_GAP, "gap")?;
        }
        if let Some(radius) = self.radius {
            file_bound(radius, MAX_RADIUS, "radius")?;
        }
        if let Some(color) = &self.color {
            parse_color(color).map_err(|reason| format!("settings: {reason}"))?;
        }
        Ok(())
    }
}

/// Numeric file bound shared by the border section validator and merger.
fn file_bound(value: f64, max: f64, name: &str) -> Result<f64, String> {
    if value.is_finite() && value >= 0.0 && value <= max {
        Ok(value)
    } else {
        Err(format!("settings: active_border.{name} needs 0..={max}"))
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
/// host default (on, width 3, gap 0, centerline radius derived as native
/// base + gap + width / 2 after width/gap resolve, `#2a82da`, theme on).
/// Every value validates through the shared bounds; failures refuse closed
/// (never a silent fallback). Border configuration is startup-only: a
/// running tile session never re-reads it.
pub fn resolve_border_config(
    cli: &BorderCli,
    file: Option<&ActiveBorderSettings>,
    os_major: Option<u32>,
) -> Result<ActiveBorderOptions, String> {
    let enabled = cli.enabled.or(file.and_then(|f| f.enabled)).unwrap_or(true);
    let width = match &cli.width {
        Some(raw) => parse_width(raw)?,
        None => match file.and_then(|f| f.width) {
            Some(value) => file_bound(value, MAX_WIDTH, "width")?,
            None => DEFAULT_WIDTH,
        },
    };
    let gap = match &cli.gap {
        Some(raw) => parse_gap(raw)?,
        None => match file.and_then(|f| f.gap) {
            Some(value) => file_bound(value, MAX_GAP, "gap")?,
            None => DEFAULT_GAP,
        },
    };
    let radius = match &cli.radius {
        Some(raw) => parse_radius(raw)?,
        None => match file.and_then(|f| f.radius) {
            Some(value) => file_bound(value, MAX_RADIUS, "radius")?,
            None => host_default_radius(os_major, width, gap),
        },
    };
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
    fn own_bundles_excluded_in_both_modes() {
        // The own probe bundle never authorizes anywhere, even listed.
        assert!(!unscoped(&[]).allows_effect(Some(crate::BUNDLE_ID)));
        assert!(!launchable(&unscoped(&[]), 4242, crate::BUNDLE_ID));
        assert!(!scoped(&[crate::BUNDLE_ID]).allows_effect(Some(crate::BUNDLE_ID)));
        // The signed helper never authorizes unscoped (own product
        // bundle), but stays admittable as an explicit scoped fixture
        // member — the only exception.
        assert!(!unscoped(&[]).allows_effect(Some(crate::HELPER_BUNDLE_ID)));
        assert!(!launchable(&unscoped(&[]), 4242, crate::HELPER_BUNDLE_ID));
        assert!(scoped(&[crate::HELPER_BUNDLE_ID]).validate().is_ok());
        assert!(scoped(&[crate::HELPER_BUNDLE_ID]).allows_effect(Some(crate::HELPER_BUNDLE_ID)));
        assert!(!scoped(&["com.example.app"]).allows_effect(Some(crate::HELPER_BUNDLE_ID)));
        // Explicitly listing an own bundle unscoped is still refused (the
        // "exclusion" cannot re-admit: unscoped admits by rule, not list).
        assert!(!unscoped(&[crate::BUNDLE_ID]).allows_effect(Some(crate::BUNDLE_ID)));
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
        // Nothing set anywhere: on, width 3, gap 0, host-derived centerline
        // radius (`#2a82da`, theme on). Tahoe 26+ derives 16 + 0 + 3/2;
        // older/unknown derive 9 + 0 + 3/2 (portable `None` route matches
        // `ActiveBorderStyle::default`).
        for (family, want) in [(Some(26), 17.5), (Some(15), 10.5), (None, 10.5)] {
            let resolved =
                resolve_border_config(&BorderCli::default(), None, family).expect("resolves");
            assert!(resolved.enabled);
            assert_eq!(resolved.style.width, 3.0);
            assert_eq!(resolved.style.gap, 0.0);
            assert_eq!(resolved.style.radius, want, "family={family:?}");
            assert_eq!(
                crate::active_border::render_color(resolved.style.color),
                "#2a82da"
            );
            assert!(resolved.style.use_theme);
        }
        assert_eq!(
            resolve_border_config(&BorderCli::default(), None, None).expect("resolves"),
            crate::active_border::ActiveBorderOptions::default()
        );
    }

    #[test]
    fn border_host_default_derives_after_width_gap() {
        // Unset radius follows the RESOLVED width/gap, not the 3/0 defaults.
        let file = ActiveBorderSettings {
            width: Some(5.0),
            gap: Some(2.0),
            ..ActiveBorderSettings::default()
        };
        let modern =
            resolve_border_config(&BorderCli::default(), Some(&file), Some(26)).expect("resolves");
        assert_eq!(modern.style.radius, 16.0 + 2.0 + 2.5);
        let legacy =
            resolve_border_config(&BorderCli::default(), Some(&file), Some(15)).expect("resolves");
        assert_eq!(legacy.style.radius, 9.0 + 2.0 + 2.5);
        // Empty section is the same as absent: pure host default.
        let empty = ActiveBorderSettings::default();
        assert_eq!(
            resolve_border_config(&BorderCli::default(), Some(&empty), Some(26))
                .expect("resolves")
                .style
                .radius,
            17.5
        );
        // Explicit radius (CLI or file) applies verbatim on every family.
        let cli = BorderCli {
            radius: Some("12".to_owned()),
            ..BorderCli::default()
        };
        for family in [Some(26), Some(15), None] {
            assert_eq!(
                resolve_border_config(&cli, Some(&file), family)
                    .expect("resolves")
                    .style
                    .radius,
                12.0,
                "family={family:?}"
            );
        }
        // Auto-derived default caps at the configured radius range; the
        // width/gap config itself still resolves.
        let wide = ActiveBorderSettings {
            width: Some(32.0),
            gap: Some(64.0),
            ..ActiveBorderSettings::default()
        };
        for family in [Some(26), None] {
            let capped = resolve_border_config(&BorderCli::default(), Some(&wide), family)
                .expect("resolves");
            assert_eq!(capped.style.width, 32.0);
            assert_eq!(capped.style.gap, 64.0);
            assert_eq!(capped.style.radius, 64.0, "family={family:?}");
        }
    }

    #[test]
    fn border_file_applies_and_cli_wins_per_field() {
        let file = ActiveBorderSettings {
            enabled: Some(false),
            width: Some(5.0),
            gap: Some(2.0),
            radius: Some(10.0),
            color: Some("#112233".to_owned()),
            use_theme: Some(false),
        };
        // File alone applies wholesale (switch off included).
        let resolved =
            resolve_border_config(&BorderCli::default(), Some(&file), None).expect("resolves");
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
        let resolved = resolve_border_config(&cli, Some(&file), Some(26)).expect("resolves");
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
        assert!(
            !resolve_border_config(&cli, None, None)
                .expect("resolves")
                .enabled
        );
        let cli = BorderCli {
            use_theme: Some(true),
            ..BorderCli::default()
        };
        assert!(
            resolve_border_config(&cli, Some(&file), Some(26))
                .expect("resolves")
                .style
                .use_theme
        );
    }

    #[test]
    fn border_config_refuses_bad_values() {
        // CLI strings validate through the shared bounds (Windows parity).
        for cli in [
            BorderCli {
                width: Some("32.1".to_owned()),
                ..BorderCli::default()
            },
            BorderCli {
                width: Some("-1".to_owned()),
                ..BorderCli::default()
            },
            BorderCli {
                gap: Some("65".to_owned()),
                ..BorderCli::default()
            },
            BorderCli {
                radius: Some("nan".to_owned()),
                ..BorderCli::default()
            },
            BorderCli {
                color: Some("2a82da".to_owned()),
                ..BorderCli::default()
            },
            BorderCli {
                color: Some("#fff".to_owned()),
                ..BorderCli::default()
            },
        ] {
            assert!(resolve_border_config(&cli, None, None).is_err());
        }
        // File numbers validate the same way (never a silent fallback).
        for file in [
            ActiveBorderSettings {
                width: Some(33.0),
                ..ActiveBorderSettings::default()
            },
            ActiveBorderSettings {
                gap: Some(-1.0),
                ..ActiveBorderSettings::default()
            },
            ActiveBorderSettings {
                radius: Some(f64::INFINITY),
                ..ActiveBorderSettings::default()
            },
            ActiveBorderSettings {
                color: Some("#zzzzzz".to_owned()),
                ..ActiveBorderSettings::default()
            },
        ] {
            assert!(file.validate().is_err());
            assert!(resolve_border_config(&BorderCli::default(), Some(&file), None).is_err());
        }
        // Boundary values pass on both routes.
        let cli = BorderCli {
            width: Some("32".to_owned()),
            gap: Some("64".to_owned()),
            radius: Some("64".to_owned()),
            color: Some("#2A82DA".to_owned()),
            ..BorderCli::default()
        };
        let resolved = resolve_border_config(&cli, None, None).expect("resolves");
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
        let border = resolve_border_config(&border_cli, None, None).expect("resolves");
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
        assert_eq!(section.enabled, Some(false));
        assert_eq!(section.width, Some(5.0));
        assert_eq!(section.color.as_deref(), Some("#112233"));
        assert!(settings.validate().is_ok());
        let resolved = resolve_border_config(
            &BorderCli::default(),
            settings.active_border.as_ref(),
            Some(26),
        )
        .expect("resolves");
        assert!(!resolved.enabled);
        assert_eq!(resolved.style.radius, 8.0);
        // Unknown nested fields refuse loudly (typos never silently no-op).
        assert!(
            serde_json::from_str::<DevSettings>(r#"{"active_border":{"thicknes":5}}"#).is_err()
        );
        // A bad section fails the whole file closed.
        let bad: DevSettings =
            serde_json::from_str(r#"{"active_border":{"width":99}}"#).expect("parses shape");
        assert!(bad.validate().is_err());
        assert!(
            resolve_border_config(&BorderCli::default(), bad.active_border.as_ref(), None).is_err()
        );
    }
}

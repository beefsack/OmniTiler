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
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DevSettings {
    /// Ctrl-role modifier override (else the CLI `--ctrl`, else Ctrl).
    pub modifier: Option<ProbeModifier>,
    /// Input preset override (else the CLI `--preset`, else Authentic).
    pub preset: Option<InputPreset>,
    /// Exact bundle-ID exclusions for unscoped runs (else CLI `--exclude`).
    pub excluded_bundles: Option<Vec<String>>,
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
}

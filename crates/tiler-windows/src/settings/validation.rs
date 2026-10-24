#![forbid(unsafe_code)]
use super::*;

/// Validate one gap value (KDE 0..64).
#[must_use]
pub const fn gap_valid(value: i32) -> bool {
    value >= 0 && value <= MAX_GAP
}

/// Strict full-document validation. Unknown binding ids, incoherent
/// state/chord pairs, malformed chords, lock-chord targets, wrong-polarity
/// (Shift/Ctrl/Alt must match the binding's native arm, resize included),
/// and duplicate active chords all refuse.
pub fn validate_settings(settings: &Settings) -> Result<(), SettingsError> {
    if settings.v != SETTINGS_SCHEMA_VERSION {
        return Err(SettingsError::UnsupportedVersion);
    }
    if settings.revision == 0 {
        return Err(SettingsError::Invalid(
            "revision must be nonzero".to_owned(),
        ));
    }
    if !gap_valid(settings.core.inner_gap) {
        return Err(SettingsError::Invalid("inner_gap needs 0..=64".to_owned()));
    }
    if !gap_valid(settings.core.outer_gap) {
        return Err(SettingsError::Invalid("outer_gap needs 0..=64".to_owned()));
    }
    if tiler_core::directional::SameAxisMove::parse_wire(&settings.core.same_axis_move).is_none() {
        return Err(SettingsError::Invalid(
            "core.same_axis_move needs group-with-neighbor|swap-with-neighbor".to_owned(),
        ));
    }
    if tiler_core::born_maximized::BornMaximized::parse_wire(&settings.core.born_maximized)
        .is_none()
    {
        return Err(SettingsError::Invalid(
            "core.born_maximized needs tile-on-launch|keep-maximized-overlay".to_owned(),
        ));
    }
    if tiler_core::size_hints::FixedSizePredicate::parse_wire(&settings.core.fixed_size_predicate)
        .is_none()
    {
        return Err(SettingsError::Invalid(
            "core.fixed_size_predicate needs both-axes-fixed|either-axis-fixed".to_owned(),
        ));
    }
    if tiler_core::admission::AnchorFallback::parse_wire(&settings.core.anchor_fallback).is_none() {
        return Err(SettingsError::Invalid(
            "core.anchor_fallback needs largest-splittable-window|closest-splittable-relative|no-search"
                .to_owned(),
        ));
    }
    if tiler_core::admission::NoSpaceResult::parse_wire(&settings.core.no_space_result).is_none() {
        return Err(SettingsError::Invalid(
            "core.no_space_result needs float|allow-overlap".to_owned(),
        ));
    }
    if tiler_core::output_selection::MigrationSourceRefill::parse_wire(
        &settings.core.migration_source_refill,
    )
    .is_none()
    {
        return Err(SettingsError::Invalid(
            "core.migration_source_refill needs last-remaining-workspace|most-recently-used-workspace"
                .to_owned(),
        ));
    }
    // Stage 2c: exact switch-target tokens only (never a silent default);
    // unknown values refuse through the existing invalid-file path.
    if tiler_core::cursor_targeting::SwitchTarget::parse_wire(&settings.core.switch_target)
        .is_none()
    {
        return Err(SettingsError::Invalid(
            "core.switch_target needs cursor-output|focused-output".to_owned(),
        ));
    }
    // Stage 2c: exact fresh-placement tokens only; unknown values refuse.
    if tiler_core::cursor_targeting::FreshPlacement::parse_wire(&settings.core.fresh_placement)
        .is_none()
    {
        return Err(SettingsError::Invalid(
            "core.fresh_placement needs cursor-output-last-focused|cursor-output-cursor-closest|focused-output-last-focused"
                .to_owned(),
        ));
    }
    // Item 12: strict token check against the shared table (never
    // `parse_workspace_mode`, which silently defaults unknown values).
    if !tiler_core::workspace_scope::WORKSPACE_MODE_TOKENS
        .contains(&settings.core.workspace.mode.as_str())
    {
        return Err(SettingsError::Invalid(
            "core.workspace.mode needs per-output-local|global-unique|shared".to_owned(),
        ));
    }
    validate_border(&settings.core.border)?;
    validate_underlay(&settings.core.underlay)?;
    validate_pager_highlight(&settings.core.pager_highlight)?;
    validate_bindings(settings)?;
    Ok(())
}

fn validate_border(style: &BorderSettings) -> Result<(), SettingsError> {
    let invalid = |reason: &str| SettingsError::Invalid(reason.to_owned());
    if !style.width.is_finite() || style.width < 0.0 || style.width > MAX_BORDER_WIDTH {
        return Err(invalid("border.width needs 0..=32"));
    }
    if !style.gap.is_finite() || style.gap < 0.0 || style.gap > MAX_BORDER_GAP {
        return Err(invalid("border.gap needs 0..=64"));
    }
    if !style.radius.is_finite() || style.radius < 0.0 || style.radius > MAX_BORDER_RADIUS {
        return Err(invalid("border.radius needs 0..=64"));
    }
    if crate::active_border::parse_color(&style.color).is_err() {
        return Err(invalid("border.color needs #rrggbb"));
    }
    Ok(())
}

fn validate_pager_highlight(style: &PagerHighlightSettings) -> Result<(), SettingsError> {
    let invalid = |reason: &str| SettingsError::Invalid(reason.to_owned());
    if crate::active_border::parse_color(&style.color).is_err() {
        return Err(invalid("pager_highlight.color needs #rrggbb"));
    }
    Ok(())
}

fn validate_underlay(style: &UnderlaySettings) -> Result<(), SettingsError> {
    let invalid = |reason: &str| SettingsError::Invalid(reason.to_owned());
    if crate::group_underlay::parse_color_argb(&style.color).is_err() {
        return Err(invalid("underlay.color needs #aarrggbb"));
    }
    if crate::group_underlay::parse_extension(&format!("{}", style.extension)).is_err() {
        return Err(invalid("underlay.extension needs -1..=32"));
    }
    Ok(())
}

fn validate_bindings(settings: &Settings) -> Result<(), SettingsError> {
    let invalid = |reason: String| SettingsError::Invalid(reason);
    let mut defs = BTreeMap::new();
    for def in binding_catalog() {
        if defs.insert(def.id, def).is_some() {
            return Err(invalid(format!("duplicate catalog id {}", def.id)));
        }
    }
    // State/chord coherence plus per-rebind safety. Duplicate detection runs
    // on the effective chord set below, so disabled bindings free their
    // default chords for reuse.
    let mut custom_keys: std::collections::HashSet<(u32, bool, bool, bool)> =
        std::collections::HashSet::new();
    // Canonical targets already claimed by a rebind: two rebound rows
    // routing into one (action, canonical key) would share one classifier
    // slot and clear each other's holds (routing is pinned per physical key,
    // but the hold slot is per canonical route), so the second refuses.
    // Distinct actions may share one VK across separate arm slots (float vs
    // sticky on G; history vs directional on H/J/K/L/arrows with Ctrl), so
    // the key is the explicit (action, canonical) pair. Residual: a rebound
    // sharing its canonical route with a kept row on another physical key
    // still shares that slot; concurrent cross-physical holds then ride the
    // pinned hold (swallowed, pair closes on first up, no OS leak, no stuck
    // slot). Refusing that too would forbid nearly every rebind, since
    // focus/move arms share canonicals by design.
    let mut canon_targets: std::collections::HashMap<(crate::snapkey::ChordAction, u32), String> =
        std::collections::HashMap::new();
    for (id, setting) in &settings.bindings {
        let Some(def) = defs.get(id.as_str()) else {
            return Err(invalid(format!("unknown binding {id}")));
        };
        match (&setting.state, &setting.chord) {
            (BindingState::Keep | BindingState::Disabled, None) => {}
            (BindingState::Keep | BindingState::Disabled, Some(_)) => {
                return Err(invalid(format!(
                    "binding {id} carries a chord without rebind"
                )));
            }
            (BindingState::Rebind, None) => {
                return Err(invalid(format!("binding {id} rebinds without a chord")));
            }
            (BindingState::Rebind, Some(text)) => {
                let parsed = parse_chord(text)
                    .map_err(|_| invalid(format!("binding {id} has a malformed chord")))?;
                if is_lock_chord(&parsed) {
                    return Err(invalid(format!(
                        "binding {id} must not target unshifted Win+L"
                    )));
                }
                if !def.implemented {
                    return Err(invalid(format!(
                        "binding {id} is not intercepted on Windows and cannot rebind"
                    )));
                }
                // The rebound chord alone routes (see `build_remap`): its
                // Shift/Ctrl/Alt must match one of the binding's native arms
                // (numbered stay rides Win+Shift plus Win+Ctrl+Shift), or a
                // focus-only rebind would also arm an unintended arm (and vice
                // versa). The classifier derives the arm from live modifiers
                // plus the explicit action.
                if !binding_modifiers_ok(def, parsed.shift, parsed.ctrl, parsed.alt) {
                    let want = binding_arm_text(def);
                    return Err(invalid(format!("binding {id} rebind needs {want}")));
                }
                // One rebound chord serves one binding: entries are
                // full-modifier physical chords, so the same key may serve
                // two arms (like float/sticky share G, or history shares
                // H/J/K/L/arrows with Ctrl) but never the same chord twice.
                if !custom_keys.insert((parsed.vk, parsed.shift, parsed.ctrl, parsed.alt)) {
                    return Err(invalid(format!(
                        "binding {id} rebinds an already-rebound chord"
                    )));
                }
                // One canonical route serves one rebound row: two physical keys
                // routing into one (action, canonical) slot would share one
                // hold and clear each other, so the classifier pins routing
                // per physical key instead. A keep-equivalent rebind (custom
                // equals its own default) claims nothing new.
                let own_default = def
                    .defaults
                    .iter()
                    .any(|default| parse_chord(default).ok() == Some(parsed));
                if !own_default {
                    let action = binding_action(def);
                    // Unbound stay rows claim their canonical slot by row
                    // identity (shared with the follow arm on the same key):
                    // two rebound rows into one (action, canonical) slot
                    // would share one hold, so the second refuses. Distinct
                    // follow/stay actions share one VK across separate arm
                    // slots, like float/sticky on G.
                    if let Some(to_vk) = binding_canonical_vk(def)
                        && let Some(first) = canon_targets.insert((action, to_vk), id.clone())
                    {
                        return Err(invalid(format!(
                            "binding {id} rebinds onto the action already rebound by {first}"
                        )));
                    }
                }
            }
        }
    }
    // Effective chord set: keep-bindings contribute their catalog defaults,
    // rebinds their custom chord, disabled bindings nothing. Catalog defaults
    // must parse; any duplicate active chord refuses.
    let mut seen: Vec<String> = Vec::new();
    for def in defs.values() {
        match settings.bindings.get(def.id) {
            None
            | Some(BindingSetting {
                state: BindingState::Keep,
                ..
            }) => {
                for default in def.defaults {
                    let parsed = parse_chord(default)
                        .map_err(|_| invalid(format!("bad catalog chord {default}")))?;
                    seen.push(render_chord(&parsed));
                }
            }
            Some(BindingSetting {
                state: BindingState::Disabled,
                ..
            }) => {}
            Some(BindingSetting {
                state: BindingState::Rebind,
                chord: Some(text),
            }) => {
                let parsed = parse_chord(text)
                    .map_err(|_| invalid(format!("binding {} has a malformed chord", def.id)))?;
                seen.push(render_chord(&parsed));
            }
            Some(BindingSetting {
                state: BindingState::Rebind,
                chord: None,
            }) => {
                return Err(invalid(format!(
                    "binding {} rebinds without a chord",
                    def.id
                )));
            }
        }
    }
    seen.sort();
    for pair in seen.windows(2) {
        if pair[0] == pair[1] {
            return Err(invalid(format!("duplicate active chord {}", pair[0])));
        }
    }
    Ok(())
}

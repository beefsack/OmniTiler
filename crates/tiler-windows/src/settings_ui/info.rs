//! Binding list and selected-row info for the settings window.
//!
//! Rows come from [`effective_bindings`] over the draft, so a rebound row
//! reports the rebound chord's owner, never the stale original. Gaming-kind
//! notes that carry no active effective conflict (recognised Xbox off, or
//! keyboard takeover off) show the honest cleared-state note from
//! [`cleared_gaming_note`] instead of the actionable warning, mirroring the
//! tray; active conflicts append the full Settings-only detail and actions.
//!
//! All listbox/text/enable effects delegate to
//! [`crate::ffi::settings_controls`]; no Win32 lives here.

#![forbid(unsafe_code)]
use crate::gaming::{GameBarPackage, XboxConfig};
use crate::gaming_settings::{
    cleared_gaming_note, gaming_detail_text, selected_gaming_conflict, visible_os_note,
};
use crate::settings::{BindingDef, BindingState, Settings, binding_catalog, effective_bindings};

use super::ctl::{enable, get_text, set_text};
use super::tabs;
use super::{
    App, ID_BIND_CHORD, ID_BINDING_INFO, ID_BINDING_LIST, ID_GAMING_OPEN_APP,
    ID_GAMING_OPEN_GAMEBAR, ID_GAMING_OPEN_XBOX, ID_GAMING_RECHECK, ID_TAB_BADGE,
};
use crate::gaming_settings::badge_conflict_counts;

/// One listbox row: action, live chord(s), and the honest physical OS
/// conflict. Without positive probe proof (Game Bar `Registered`, Xbox
/// `Enabled`) the gaming ownership text hides so the row reports no known
/// OS conflict; a gaming-kind conflict with no active effective conflict
/// otherwise shows the cleared-state note, never the stale actionable warning.
pub(super) fn row_text(
    def: &BindingDef,
    work: &Settings,
    takeover: bool,
    xbox: XboxConfig,
    gamebar: GameBarPackage,
) -> String {
    let effective = effective_bindings(work);
    let row = effective.iter().find(|row| row.id == def.id);
    let mut out = format!("{}: {}", def.id, def.text);
    match row {
        None => out.push_str(" [unknown]"),
        Some(row) => {
            if !row.active {
                out.push_str(" [disabled: passes through natively]");
            } else if row.chords.is_empty() {
                out.push_str(" [no chord]");
            } else {
                out.push_str(&format!(" [{}]", row.chords.join(", ")));
            }
            if !def.implemented {
                out.push_str(" (not available on Windows)");
            }
            match row.conflict {
                Some(conflict) => {
                    let shown = cleared_gaming_note(
                        row.active,
                        row.effective,
                        Some(conflict),
                        takeover,
                        xbox,
                    )
                    .or_else(|| visible_os_note(Some(conflict), gamebar, xbox));
                    match shown {
                        Some(text) => {
                            let short: String = text.chars().take(96).collect();
                            out.push_str(&format!(" -- {short}"));
                        }
                        None => out.push_str(" -- no known OS conflict"),
                    }
                }
                None => out.push_str(" -- no known OS conflict"),
            }
            if !row.effective && !row.reason.is_empty() {
                out.push_str(&format!(" ({})", row.reason));
            }
        }
    }
    out
}

pub(super) fn refresh_list(app: &App) {
    use crate::ffi::settings_controls::{list_add, list_reset, list_set_sel};
    let list = app.ctl(ID_BINDING_LIST);
    list_reset(list);
    let takeover = app.work.core.keyboard.takeover;
    let snapshot = app.gaming.snapshot();
    for def in binding_catalog() {
        list_add(
            list,
            &row_text(&def, &app.work, takeover, snapshot.xbox, snapshot.gamebar),
        );
    }
    if let Some(index) = app.selected {
        list_set_sel(list, index);
    }
    refresh_info(app);
}

/// Selected-row editor text: current state, live chords, and the conflict the
/// OS actually owns at those chords. Without positive probe proof the `OS:`
/// line reads as ordinary generic text; cleared gaming rows (recognised Xbox
/// off, takeover off) show the honest cleared-state note with no actions;
/// active gaming conflicts append the full detail and scoped Open buttons.
/// The readout is a readonly multiline edit with its own vertical scrollbar
/// and tab stop, so long gaming detail stays readable and keyboard reachable.
pub(super) fn refresh_info(app: &App) {
    let catalog = binding_catalog();
    let Some(index) = app.selected else {
        set_text(
            app.ctl(ID_BINDING_INFO),
            "Select a row to keep, disable, or rebind it.",
        );
        set_text(app.ctl(ID_BIND_CHORD), "");
        disable_hidden_page_controls(app);
        return;
    };
    let Some(def) = catalog.get(index) else {
        disable_hidden_page_controls(app);
        return;
    };
    let snapshot = app.gaming.snapshot();
    let takeover = app.work.core.keyboard.takeover;
    let effective = effective_bindings(&app.work);
    let row = effective.iter().find(|row| row.id == def.id);
    let mut info = format!("{}: {}", def.id, def.text);
    if let Some(row) = row {
        if row.active && !row.chords.is_empty() {
            info.push_str(&format!("\nChord: {}", row.chords.join(", ")));
        } else if row.active && row.chords.is_empty() {
            info.push_str("\nChord: unbound (bindable; passes through until rebound)");
        } else {
            info.push_str("\nChord: disabled (passes through natively)");
        }
        match row.conflict {
            Some(conflict) => {
                let shown = cleared_gaming_note(
                    row.active,
                    row.effective,
                    Some(conflict),
                    takeover,
                    snapshot.xbox,
                )
                .or_else(|| visible_os_note(Some(conflict), snapshot.gamebar, snapshot.xbox));
                match shown {
                    Some(text) => info.push_str(&format!("\nOS: {text}")),
                    None => info.push_str("\nOS: no known conflict"),
                }
            }
            None => info.push_str("\nOS: no known conflict"),
        }
    }
    match app.work.bindings.get(def.id) {
        None => {
            let dirty = app.work.bindings.get(def.id) != app.base.bindings.get(def.id);
            let tag = if dirty { " (unsaved)" } else { "" };
            info.push_str(&format!("\nState: keep (catalog default){tag}"));
        }
        Some(setting) => match (&setting.state, &setting.chord) {
            (BindingState::Disabled, _) => {
                let dirty = app.work.bindings.get(def.id) != app.base.bindings.get(def.id);
                let tag = if dirty { " (unsaved)" } else { "" };
                info.push_str(&format!("\nState: disabled{tag}"));
            }
            (BindingState::Rebind, chord) => {
                let dirty = app.work.bindings.get(def.id) != app.base.bindings.get(def.id);
                let tag = if dirty { " (unsaved)" } else { "" };
                info.push_str(&format!(
                    "\nState: rebind to {}{tag}",
                    chord.as_deref().unwrap_or("?")
                ));
            }
            _ => {
                let dirty = app.work.bindings.get(def.id) != app.base.bindings.get(def.id);
                let tag = if dirty { " (unsaved)" } else { "" };
                info.push_str(&format!("\nState: keep (catalog default){tag}"));
            }
        },
    }
    if !def.implemented {
        info.push_str(
            "\nNot available on Windows: this binding is not intercepted; rebind is refused.",
        );
    }
    // Gaming coexistence: effective conflicts only (never id/preset guesses).
    // The detail text carries the categorical evidence, manual paths,
    // feature/data loss, and the navigation-never-resolves rule; the three
    // Open buttons stay scoped to the selected conflict kind and Recheck
    // re-queries asynchronously. Cleared rows show the honest cleared-state
    // note with no actions; Win+L and non-gaming rows keep existing behavior
    // exactly (all Open buttons disabled).
    {
        let conflict = selected_gaming_conflict(&app.work, takeover, &snapshot, def.id);
        match conflict {
            Some(conflict) => {
                info.push_str(&format!("\n{}", gaming_detail_text(&conflict)));
                let xbox = conflict.kind == crate::gaming::GamingConflictKind::XboxMode;
                let gamebar = conflict.kind == crate::gaming::GamingConflictKind::GameBar;
                enable(app.ctl(ID_GAMING_OPEN_XBOX), xbox);
                enable(app.ctl(ID_GAMING_OPEN_GAMEBAR), gamebar);
                enable(app.ctl(ID_GAMING_OPEN_APP), gamebar);
            }
            None => {
                if let Some(cleared) = row.and_then(|row| {
                    cleared_gaming_note(
                        row.active,
                        row.effective,
                        row.conflict,
                        takeover,
                        snapshot.xbox,
                    )
                }) {
                    info.push_str(&format!("\nGaming status (no active conflict): {cleared}"));
                }
                enable(app.ctl(ID_GAMING_OPEN_XBOX), false);
                enable(app.ctl(ID_GAMING_OPEN_GAMEBAR), false);
                enable(app.ctl(ID_GAMING_OPEN_APP), false);
            }
        }
        enable(app.ctl(ID_GAMING_RECHECK), true);
    }
    set_text(app.ctl(ID_BINDING_INFO), &info.replace('\n', "\r\n"));
    disable_hidden_page_controls(app);
}

/// Automated controls owned by another tab than `current_tab`: the exact
/// set the native gate keeps disabled so `IsDialogMessageW` never parks
/// focus off-tab. Chrome (strip, badge, status, buttons) is never hidden.
/// Pure over the catalog tables so offline tests pin the gate.
fn hidden_page_ids(current_tab: usize) -> Vec<u32> {
    tabs::ALL_CONTROL_IDS
        .iter()
        .copied()
        .filter(|id| matches!(tabs::control_tab(*id), Some(tab) if tab != current_tab))
        .collect()
}

/// Hidden pages stay disabled: `refresh_list` re-enables the gaming
/// buttons by conflict kind without regard to the active tab, and this
/// runs on every preset/stage/revert/timer/selection path, not just
/// `show_tab` — including the no-selection and bad-index early returns
/// above, which otherwise leave freshly created (enabled) off-tab buttons
/// in the tab order. Without this a Recheck landing on another tab would
/// enable an off-tab button and hand `IsDialogMessageW` a hidden tab stop.
fn disable_hidden_page_controls(app: &App) {
    let hidden = hidden_page_ids(app.current_tab);
    for (id, hwnd) in &app.controls {
        if hidden.contains(id) {
            enable(*hwnd, false);
        }
    }
}

/// Global badge outside the tab pages: invalid file, unsaved draft, and
/// aggregated Shortcuts conflicts plus warnings stay visible on every
/// tab. Unsaved means any unproven channel: HWND edits since the last
/// push (`dirty`), staged `work != base` drift (presets, rebinding,
/// keep/disable), or a still-typed pending chord. Conflict counts mirror
/// the tray warnings over every row (never just the selection). Saved,
/// requested, and confirmed stay distinct: file state only, no live
/// native-effect claim; the Shortcuts tab's own buttons do the work.
pub(super) fn refresh_badge(app: &App) {
    let badge = app.ctl(ID_TAB_BADGE);
    if badge.is_null() {
        return;
    }
    if app.invalid.is_some() {
        set_text(badge, &tabs::badge_text(true, false, 0, 0));
        return;
    }
    let snapshot = app.gaming.snapshot();
    let takeover = app.work.core.keyboard.takeover;
    let allow_win_l = app.work.core.keyboard.allow_win_l;
    let pending = !get_text(app.ctl(ID_BIND_CHORD)).trim().is_empty();
    let unsaved = !tabs::badge_proven_saved(app.dirty, app.work != app.base, pending);
    let (conflicts, warnings) = badge_conflict_counts(&app.work, takeover, allow_win_l, &snapshot);
    set_text(
        badge,
        &tabs::badge_text(false, unsaved, conflicts, warnings),
    );
}

#[cfg(test)]
mod tests {
    use super::super::{ID_APPLY, ID_CLOSE, ID_REVERT, ID_STATUS, ID_TAB_BADGE, ID_TABS};
    use super::{hidden_page_ids, row_text, tabs};
    use crate::gaming::GameBarPackage;
    use crate::gaming::XboxConfig;
    use crate::settings::{BindingSetting, BindingState, Settings, binding_catalog};
    use std::collections::BTreeSet;

    fn float_def() -> crate::settings::BindingDef {
        binding_catalog()
            .into_iter()
            .find(|def| def.id == "toggle-float")
            .expect("float def")
    }

    fn fullscreen_def() -> crate::settings::BindingDef {
        binding_catalog()
            .into_iter()
            .find(|def| def.id == "toggle-fullscreen")
            .expect("fullscreen def")
    }

    #[test]
    fn hidden_gate_covers_every_off_tab_control_from_any_entry_path() {
        // The gate `refresh_info` enforces on every path — normal render
        // plus the no-selection and bad-index early returns — and
        // `show_tab` re-enforces on switches: every automated control
        // owned by another tab, never chrome.
        for current in 0..tabs::TAB_COUNT {
            let hidden: BTreeSet<u32> = hidden_page_ids(current).into_iter().collect();
            for id in tabs::ALL_CONTROL_IDS {
                match tabs::control_tab(*id) {
                    Some(tab) if tab != current => {
                        assert!(hidden.contains(id), "{id} hidden on tab {current}")
                    }
                    _ => assert!(!hidden.contains(id), "{id} kept on tab {current}"),
                }
            }
            for chrome in [
                ID_TABS,
                ID_TAB_BADGE,
                ID_STATUS,
                ID_APPLY,
                ID_REVERT,
                ID_CLOSE,
            ] {
                assert!(!hidden.contains(&chrome), "{chrome} is chrome on {current}");
            }
        }
        // Every page control hides on exactly the tabs it does not own.
        for id in tabs::ALL_CONTROL_IDS {
            if tabs::control_tab(*id).is_some() {
                let count = (0..tabs::TAB_COUNT)
                    .filter(|tab| hidden_page_ids(*tab).contains(id))
                    .count();
                assert_eq!(count, tabs::TAB_COUNT - 1, "{id}");
            }
        }
    }

    #[test]
    fn float_row_text_warns_only_when_registered_else_generic() {
        let work = Settings::default();
        let def = float_def();
        // Positive registration warns; absence, unknown, and query failure
        // all read as ordinary generic rows (Part B provably-present rule).
        let text = row_text(
            &def,
            &work,
            true,
            XboxConfig::Unknown,
            GameBarPackage::Registered,
        );
        assert!(text.contains("Win+G"), "{text}");
        assert!(text.contains("Game Bar"), "{text}");
        for gamebar in [GameBarPackage::Absent, GameBarPackage::Unknown] {
            let cleared = row_text(&def, &work, true, XboxConfig::Unknown, gamebar);
            assert!(cleared.contains("Win+G"), "{gamebar:?}: {cleared}");
            assert!(
                cleared.contains("no known OS conflict"),
                "{gamebar:?}: {cleared}"
            );
            assert!(!cleared.contains("Game Bar"), "{gamebar:?}: {cleared}");
            assert!(
                !cleared.contains("cannot fully contain"),
                "{gamebar:?}: {cleared}"
            );
            assert!(
                !cleared.contains("check required"),
                "{gamebar:?}: {cleared}"
            );
            assert!(!cleared.contains("unproven"), "{gamebar:?}: {cleared}");
            assert!(!cleared.contains("no matching"), "{gamebar:?}: {cleared}");
        }
    }

    #[test]
    fn disabled_float_row_hides_gamebar_note_without_positive_proof() {
        let mut work = Settings::default();
        work.bindings.insert(
            "toggle-float".to_owned(),
            BindingSetting {
                state: BindingState::Disabled,
                chord: None,
            },
        );
        assert!(crate::settings::validate_settings(&work).is_ok());
        // Disabled rows name the live OS owner only while positively proven;
        // absence, unknown, and failure hide the stale catalog note.
        let text = row_text(
            &float_def(),
            &work,
            true,
            XboxConfig::Unknown,
            GameBarPackage::Registered,
        );
        assert!(text.contains("disabled"), "{text}");
        assert!(text.contains("Game Bar"), "{text}");
        for gamebar in [GameBarPackage::Absent, GameBarPackage::Unknown] {
            let cleared = row_text(&float_def(), &work, true, XboxConfig::Unknown, gamebar);
            assert!(cleared.contains("disabled"), "{gamebar:?}: {cleared}");
            assert!(
                cleared.contains("no known OS conflict"),
                "{gamebar:?}: {cleared}"
            );
            assert!(!cleared.contains("Game Bar"), "{gamebar:?}: {cleared}");
        }
    }

    #[test]
    fn xbox_row_text_warns_only_when_enabled_else_generic() {
        let work = Settings::default();
        // Positive Xbox proof warns; unknown, off, and Game Bar absence all
        // hide the Xbox warning (off shows its cleared status elsewhere, not
        // here: this row has no cleared note for unknown).
        let text = row_text(
            &fullscreen_def(),
            &work,
            true,
            XboxConfig::Enabled,
            GameBarPackage::Absent,
        );
        assert!(text.contains("Win+F11"), "{text}");
        assert!(text.contains("Xbox mode"), "{text}");
        for xbox in [XboxConfig::Unknown, XboxConfig::Disabled] {
            let cleared = row_text(&fullscreen_def(), &work, true, xbox, GameBarPackage::Absent);
            // Disabled carries its cleared off evidence through
            // `cleared_gaming_note`, not the actionable warning.
            assert!(
                !cleared.contains("containment is incomplete"),
                "{xbox:?}: {cleared}"
            );
        }
        let unknown = row_text(
            &fullscreen_def(),
            &work,
            true,
            XboxConfig::Unknown,
            GameBarPackage::Registered,
        );
        assert!(unknown.contains("Win+F11"), "{unknown}");
        assert!(unknown.contains("no known OS conflict"), "{unknown}");
        assert!(!unknown.contains("Xbox mode"), "{unknown}");
    }

    #[test]
    fn rebound_ctrl_rows_carry_no_gaming_note() {
        let mut work = Settings::default();
        work.bindings.insert(
            "toggle-float".to_owned(),
            BindingSetting {
                state: BindingState::Rebind,
                chord: Some("Win+Ctrl+G".to_owned()),
            },
        );
        work.bindings.insert(
            "toggle-fullscreen".to_owned(),
            BindingSetting {
                state: BindingState::Rebind,
                chord: Some("Win+Ctrl+F11".to_owned()),
            },
        );
        assert!(crate::settings::validate_settings(&work).is_ok());
        for gamebar in [
            GameBarPackage::Registered,
            GameBarPackage::Absent,
            GameBarPackage::Unknown,
        ] {
            let text = row_text(&float_def(), &work, true, XboxConfig::Unknown, gamebar);
            assert!(text.contains("Win+Ctrl+G"), "{gamebar:?}: {text}");
            assert!(!text.contains("Game Bar"), "{gamebar:?}: {text}");
            let text = row_text(&fullscreen_def(), &work, true, XboxConfig::Unknown, gamebar);
            assert!(text.contains("Win+Ctrl+F11"), "{gamebar:?}: {text}");
            assert!(!text.contains("Xbox mode"), "{gamebar:?}: {text}");
        }
    }
}

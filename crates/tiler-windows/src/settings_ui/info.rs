//! Binding list and selected-row info for the settings window.
//!
//! Rows come from [`effective_bindings`] over the draft, so a rebound row
//! reports the rebound chord's owner, never the stale original. Gaming-kind
//! notes that carry no active effective conflict (recognised Xbox off, or
//! keyboard takeover off) show the honest cleared-state note from
//! [`cleared_gaming_note`] instead of the actionable warning, mirroring the
//! tray; active conflicts append the full Settings-only detail and actions.

use crate::gaming::XboxConfig;
use crate::gaming_settings::{cleared_gaming_note, gaming_detail_text, selected_gaming_conflict};
use crate::settings::{BindingDef, BindingState, Settings, binding_catalog, effective_bindings};

use super::ctl::{enable, set_text, wide};
use super::{
    App, ID_BIND_CHORD, ID_BINDING_INFO, ID_BINDING_LIST, ID_GAMING_OPEN_APP,
    ID_GAMING_OPEN_GAMEBAR, ID_GAMING_OPEN_XBOX, ID_GAMING_RECHECK,
};

/// One listbox row: action, live chord(s), and the honest physical OS
/// conflict. A gaming-kind conflict with no active effective conflict shows
/// the cleared-state note, never the stale actionable warning.
pub(super) fn row_text(
    def: &BindingDef,
    work: &Settings,
    takeover: bool,
    xbox: XboxConfig,
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
                    .unwrap_or(conflict);
                    let short: String = shown.chars().take(96).collect();
                    out.push_str(&format!(" -- {short}"));
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
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        LB_ADDSTRING, LB_RESETCONTENT, LB_SETCURSEL, SendMessageW,
    };
    let list = app.ctl(ID_BINDING_LIST);
    unsafe {
        SendMessageW(list, LB_RESETCONTENT, 0, 0);
    }
    let takeover = app.work.core.keyboard.takeover;
    let xbox = app.gaming.snapshot().xbox;
    for def in binding_catalog() {
        let text = wide(&row_text(&def, &app.work, takeover, xbox));
        unsafe {
            SendMessageW(list, LB_ADDSTRING, 0, text.as_ptr() as isize);
        }
    }
    if let Some(index) = app.selected {
        unsafe {
            SendMessageW(list, LB_SETCURSEL, index, 0);
        }
    }
    refresh_info(app);
}

/// Selected-row editor text: current state, live chords, and the conflict the
/// OS actually owns at those chords. Cleared gaming rows (recognised Xbox
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
        return;
    };
    let Some(def) = catalog.get(index) else {
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
                .unwrap_or(conflict);
                info.push_str(&format!("\nOS: {shown}"));
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
}

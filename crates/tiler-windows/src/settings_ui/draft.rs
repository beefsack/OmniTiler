//! Draft editing for the settings window: pushing saved values into controls,
//! collecting them back into a validated document, and the binding editor,
//! Apply/Revert/preset staging. Nothing here writes except `do_apply`'s
//! single atomic save (after stale-content refusal).

use crate::settings::{
    BindingSetting, BindingState, LoadOutcome, Settings, apply_preset, binding_catalog,
    parse_chord, validate_settings,
};

use super::choice::{
    apply_born_maximized_selection, apply_fixed_size_selection, apply_migration_refill_selection,
    apply_same_axis_selection, apply_workspace_mode_selection, born_overlay_selected,
    fixed_size_either_selected, migration_refill_mru_selected, same_axis_swap_selected,
    workspace_mode_selected,
};
use super::ctl::{checked, enable, get_text, set_checked, set_text};
use super::info::refresh_list;
use super::{App, status};
use super::{
    ID_ALLOW_WIN_L, ID_APPLY, ID_BIND_CHORD, ID_BORDER_COLOR, ID_BORDER_ENABLED, ID_BORDER_GAP,
    ID_BORDER_RADIUS, ID_BORDER_THEME, ID_BORDER_WIDTH, ID_BORN_MAX_OVERLAY, ID_BORN_MAX_TILE,
    ID_DEFAULT_FLOATING, ID_DEFAULT_TILED, ID_FIXED_SIZE_BOTH, ID_FIXED_SIZE_EITHER, ID_INNER_GAP,
    ID_KEYBOARD_TAKEOVER, ID_MIGRATION_REFILL_LAST, ID_MIGRATION_REFILL_MRU, ID_MOUSE_SNAP,
    ID_OUTER_GAP, ID_SAME_AXIS_GROUP, ID_SAME_AXIS_SWAP, ID_TASKBAR_PAGER, ID_UNDERLAY_COLOR,
    ID_UNDERLAY_ENABLED, ID_UNDERLAY_EXTENSION, ID_WORKSPACE_MODE_GLOBAL, ID_WORKSPACE_MODE_LOCAL,
    ID_WORKSPACE_MODE_SHARED,
};

/// Push the saved/draft values into every control and rebuild the list.
pub(super) fn refresh_all(app: &App) {
    let core = &app.work.core;
    set_text(app.ctl(ID_INNER_GAP), &format!("{}", core.inner_gap));
    set_text(app.ctl(ID_OUTER_GAP), &format!("{}", core.outer_gap));
    set_checked(app.ctl(ID_BORDER_ENABLED), core.border.enabled);
    set_checked(app.ctl(ID_BORDER_THEME), core.border.use_theme);
    set_text(app.ctl(ID_BORDER_COLOR), &core.border.color);
    set_text(app.ctl(ID_BORDER_WIDTH), &format!("{}", core.border.width));
    set_text(app.ctl(ID_BORDER_GAP), &format!("{}", core.border.gap));
    set_text(
        app.ctl(ID_BORDER_RADIUS),
        &format!("{}", core.border.radius),
    );
    set_checked(app.ctl(ID_UNDERLAY_ENABLED), core.underlay.enabled);
    set_text(app.ctl(ID_UNDERLAY_COLOR), &core.underlay.color);
    set_text(
        app.ctl(ID_UNDERLAY_EXTENSION),
        &format!("{}", core.underlay.extension),
    );
    set_checked(app.ctl(ID_KEYBOARD_TAKEOVER), core.keyboard.takeover);
    set_checked(app.ctl(ID_MOUSE_SNAP), core.mouse.snap_prevention);
    set_checked(app.ctl(ID_ALLOW_WIN_L), core.keyboard.allow_win_l);
    set_checked(app.ctl(ID_DEFAULT_TILED), core.workspace.default_tiled);
    set_checked(app.ctl(ID_DEFAULT_FLOATING), !core.workspace.default_tiled);
    set_checked(app.ctl(ID_TASKBAR_PAGER), core.taskbar_pager);
    // R-MOV-03 two-choice control: functional labels only; the WM
    // reference lives in the native radio tooltips, not in visible text.
    let swap = same_axis_swap_selected(&app.work);
    set_checked(app.ctl(ID_SAME_AXIS_GROUP), !swap);
    set_checked(app.ctl(ID_SAME_AXIS_SWAP), swap);
    // R-MAX-06 two-choice control: same functional-labels-only rule; its own
    // WS_GROUP pair below keeps it independent from the same-axis pair.
    let overlay = born_overlay_selected(&app.work);
    set_checked(app.ctl(ID_BORN_MAX_TILE), !overlay);
    set_checked(app.ctl(ID_BORN_MAX_OVERLAY), overlay);
    // R-SPC-04 D1 two-choice control: same functional-labels-only rule; its
    // own WS_GROUP pair below keeps it independent from the pairs above.
    let either = fixed_size_either_selected(&app.work);
    set_checked(app.ctl(ID_FIXED_SIZE_BOTH), !either);
    set_checked(app.ctl(ID_FIXED_SIZE_EITHER), either);
    // R-WS-12e G-37 two-choice control: same functional-labels-only rule;
    // its own WS_GROUP pair below keeps it independent from the pairs above.
    let mru = migration_refill_mru_selected(&app.work);
    set_checked(app.ctl(ID_MIGRATION_REFILL_LAST), !mru);
    set_checked(app.ctl(ID_MIGRATION_REFILL_MRU), mru);
    // Item 12 three-choice control: functional labels only; the group title
    // carries the honest restart-required note (stage A persists the choice,
    // the running owner keeps its startup scope until restart). Its own
    // WS_GROUP triple below keeps it independent from every pair above.
    let mode = workspace_mode_selected(&app.work);
    set_checked(app.ctl(ID_WORKSPACE_MODE_LOCAL), mode == 0);
    set_checked(app.ctl(ID_WORKSPACE_MODE_GLOBAL), mode == 1);
    set_checked(app.ctl(ID_WORKSPACE_MODE_SHARED), mode == 2);
    refresh_list(app);
    enable(app.ctl(ID_APPLY), app.invalid.is_none());
}

fn parse_gap(text: &str, field: &str) -> Result<i32, String> {
    let value: i32 = text
        .trim()
        .parse()
        .map_err(|_| format!("{field} needs 0..=64"))?;
    if !(0..=crate::settings::MAX_GAP).contains(&value) {
        return Err(format!("{field} needs 0..=64"));
    }
    Ok(value)
}

fn parse_float(text: &str, field: &str, min: f64, max: f64, range: &str) -> Result<f64, String> {
    let value: f64 = text
        .trim()
        .parse()
        .map_err(|_| format!("{field} needs {range}"))?;
    if !value.is_finite() || value < min || value > max {
        return Err(format!("{field} needs {range}"));
    }
    Ok(value)
}

/// Collect every draft input (all core controls plus the draft bindings and
/// any still-pending chord in the rebind editor) into one validated document.
/// Failing validation returns the visible reason and writes nothing.
pub(super) fn collect_draft(app: &mut App) -> Result<Settings, String> {
    // A typed-but-unset rebind is still a draft input: flush it first so Apply
    // never silently drops the selected binding's edit.
    let pending = get_text(app.ctl(ID_BIND_CHORD));
    if !pending.trim().is_empty() {
        apply_rebind_text(app, &pending)?;
        set_text(app.ctl(ID_BIND_CHORD), "");
    }
    let mut draft = app.work.clone();
    draft.core.inner_gap = parse_gap(&get_text(app.ctl(ID_INNER_GAP)), "inner_gap")?;
    draft.core.outer_gap = parse_gap(&get_text(app.ctl(ID_OUTER_GAP)), "outer_gap")?;
    draft.core.border.enabled = checked(app.ctl(ID_BORDER_ENABLED));
    draft.core.border.use_theme = checked(app.ctl(ID_BORDER_THEME));
    draft.core.border.color = get_text(app.ctl(ID_BORDER_COLOR)).trim().to_owned();
    draft.core.border.width = parse_float(
        &get_text(app.ctl(ID_BORDER_WIDTH)),
        "border.width",
        0.0,
        crate::settings::MAX_BORDER_WIDTH,
        "0..=32",
    )?;
    draft.core.border.gap = parse_float(
        &get_text(app.ctl(ID_BORDER_GAP)),
        "border.gap",
        0.0,
        crate::settings::MAX_BORDER_GAP,
        "0..=64",
    )?;
    draft.core.border.radius = parse_float(
        &get_text(app.ctl(ID_BORDER_RADIUS)),
        "border.radius",
        0.0,
        crate::settings::MAX_BORDER_RADIUS,
        "0..=64",
    )?;
    draft.core.underlay.enabled = checked(app.ctl(ID_UNDERLAY_ENABLED));
    draft.core.underlay.color = get_text(app.ctl(ID_UNDERLAY_COLOR)).trim().to_owned();
    draft.core.underlay.extension = parse_float(
        &get_text(app.ctl(ID_UNDERLAY_EXTENSION)),
        "underlay.extension",
        -1.0,
        crate::settings::MAX_UNDERLAY_EXTENSION,
        "-1..=32",
    )?;
    draft.core.keyboard.takeover = checked(app.ctl(ID_KEYBOARD_TAKEOVER));
    draft.core.keyboard.allow_win_l = checked(app.ctl(ID_ALLOW_WIN_L));
    draft.core.mouse.snap_prevention = checked(app.ctl(ID_MOUSE_SNAP));
    // New-workspace default radios: exactly one is checked after any load;
    // a checked Floating wins, otherwise the default stays tiled.
    draft.core.workspace.default_tiled = !checked(app.ctl(ID_DEFAULT_FLOATING));
    // Taskbar pager opt-out (default on): the checkbox stages the bool
    // directly; hot-applies on the next owner pump.
    draft.core.taskbar_pager = checked(app.ctl(ID_TASKBAR_PAGER));
    // R-MOV-03: the swap radio stages the exact swap token, otherwise the
    // exact group token; only these two values can reach validation.
    apply_same_axis_selection(&mut draft, checked(app.ctl(ID_SAME_AXIS_SWAP)));
    // R-MAX-06: the overlay radio stages the exact overlay token, otherwise
    // the exact tile token; only these two values can reach validation.
    apply_born_maximized_selection(&mut draft, checked(app.ctl(ID_BORN_MAX_OVERLAY)));
    // R-SPC-04 D1: the either-axis radio stages the exact either-axis token,
    // otherwise the exact both-axes token; only these two values can reach
    // validation.
    apply_fixed_size_selection(&mut draft, checked(app.ctl(ID_FIXED_SIZE_EITHER)));
    // R-WS-12e G-37: the most-recently-used radio stages the exact MRU token,
    // otherwise the exact last-remaining token; only these two values can
    // reach validation.
    apply_migration_refill_selection(&mut draft, checked(app.ctl(ID_MIGRATION_REFILL_MRU)));
    // Item 12: the global-unique radio stages the exact global-unique token,
    // the shared radio the exact shared token, otherwise the exact
    // per-output-local token; only these three values can reach validation.
    // The choice saves immediately but takes effect on restart (stage B).
    let mode_index = if checked(app.ctl(ID_WORKSPACE_MODE_GLOBAL)) {
        1
    } else if checked(app.ctl(ID_WORKSPACE_MODE_SHARED)) {
        2
    } else {
        0
    };
    apply_workspace_mode_selection(&mut draft, mode_index);
    draft.revision = app.base.revision;
    validate_settings(&draft).map_err(|e| e.to_string())?;
    Ok(draft)
}

/// Validate one rebind against the selected row and stage it in the draft.
/// The Shift/Ctrl/Alt arm must match one of the action's native arms
/// (history rows ride Win+Ctrl, float/fullscreen ride Win or Win+Ctrl
/// (Compatible Ctrl arms), relative sends ride Win+Ctrl+Shift,
/// numbered stay rides Win+Shift plus Win+Ctrl+Shift, output follow rides
/// Win+Ctrl+Alt and output stay rides Win+Ctrl+Alt plus Win+Ctrl+Alt+Shift,
/// workspace migration rides Win+Ctrl+Alt+Shift,
/// resize-out rides Win+Alt and resize-in rides Win+Shift+Alt).
pub(super) fn apply_rebind_text(app: &mut App, text: &str) -> Result<(), String> {
    let index = app
        .selected
        .ok_or_else(|| "select a binding row first".to_owned())?;
    let def = binding_catalog()
        .into_iter()
        .nth(index)
        .ok_or_else(|| "select a binding row first".to_owned())?;
    if !def.implemented {
        return Err(format!(
            "binding {} is not intercepted on Windows and cannot rebind",
            def.id
        ));
    }
    let parsed = parse_chord(text)
        .map_err(|_| "refuse: chord needs Win[+Shift][+Alt][+Ctrl]+Key".to_owned())?;
    if crate::settings::is_lock_chord(&parsed) {
        return Err(format!(
            "binding {} must not target unshifted Win+L",
            def.id
        ));
    }
    if !crate::settings::binding_modifiers_ok(&def, parsed.shift, parsed.ctrl, parsed.alt) {
        let want = crate::settings::binding_arm_text(&def);
        return Err(format!("binding {} rebind needs {want}", def.id));
    }
    let rendered = crate::settings::render_chord(&parsed);
    let previous = app.work.bindings.insert(
        def.id.to_owned(),
        BindingSetting {
            state: BindingState::Rebind,
            chord: Some(rendered),
        },
    );
    if let Err(error) = validate_settings(&app.work) {
        if let Some(old) = previous {
            app.work.bindings.insert(def.id.to_owned(), old);
        } else {
            app.work.bindings.remove(def.id);
        }
        return Err(error.to_string());
    }
    refresh_list(app);
    Ok(())
}

pub(super) fn stage_state(app: &mut App, state: BindingState) -> Result<(), String> {
    let index = app
        .selected
        .ok_or_else(|| "select a binding row first".to_owned())?;
    let def = binding_catalog()
        .into_iter()
        .nth(index)
        .ok_or_else(|| "select a binding row first".to_owned())?;
    let previous = match state {
        BindingState::Keep => app.work.bindings.remove(def.id),
        BindingState::Disabled => app.work.bindings.insert(
            def.id.to_owned(),
            BindingSetting {
                state: BindingState::Disabled,
                chord: None,
            },
        ),
        BindingState::Rebind => {
            return apply_rebind_text(app, &get_text(app.ctl(ID_BIND_CHORD)));
        }
    };
    if let Err(error) = validate_settings(&app.work) {
        if let Some(old) = previous {
            app.work.bindings.insert(def.id.to_owned(), old);
        } else {
            app.work.bindings.remove(def.id);
        }
        return Err(error.to_string());
    }
    // A staged Keep/Disable wins over any still-typed chord: clear it so a
    // later Apply cannot flush a stale edit over this explicit choice.
    set_text(app.ctl(ID_BIND_CHORD), "");
    refresh_list(app);
    Ok(())
}

pub(super) fn do_apply(app: &mut App) {
    if app.invalid.is_some() {
        status(
            app,
            "Error: settings file is invalid; fix or delete it, then Revert. Nothing was written.",
        );
        return;
    }
    let mut draft = match collect_draft(app) {
        Ok(draft) => draft,
        Err(reason) => {
            status(app, &format!("Error: {reason}. Nothing was written."));
            return;
        }
    };
    // Stale-content refusal: another writer (e.g. a second Settings window
    // or another tool) created or changed the file since this window loaded,
    // so never silently overwrite unrelated edits. Compares the full document
    // (not just the revision) and treats a newly-appeared file as changed
    // even when its bytes equal the defaults. Reloads the newer values and
    // asks for a fresh Apply. A second Settings window cannot reach this
    // race: the per-user named mutex below serializes UI instances.
    match crate::settings::load_from_dir(&app.dir) {
        LoadOutcome::Loaded(current) if current != app.base || !app.base_present => {
            app.base = current.clone();
            app.work = current;
            app.base_present = true;
            refresh_all(app);
            status(
                app,
                "Settings changed on disk (another window saved); reloaded. Re-apply your edits.",
            );
            return;
        }
        LoadOutcome::Missing if app.base_present => {
            app.base = Settings::default();
            app.work = Settings::default();
            app.base_present = false;
            refresh_all(app);
            status(
                app,
                "Settings file vanished; reloaded defaults. Re-apply your edits.",
            );
            return;
        }
        LoadOutcome::Invalid(error) => {
            app.invalid = Some(error.to_string());
            enable(app.ctl(ID_APPLY), false);
            status(
                app,
                &format!("Error: settings file turned invalid ({error}). Nothing was written."),
            );
            return;
        }
        LoadOutcome::Loaded(_) | LoadOutcome::Missing => {}
    }
    match crate::settings::save_to_dir(&app.dir, &mut draft) {
        Ok(()) => {
            app.base = draft.clone();
            app.work = draft.clone();
            app.base_present = true;
            refresh_all(app);
            // Unconditional wording: the save never verifies a live owner
            // (ledger presence alone proves nothing) and never confirms a
            // native effect.
            status(
                app,
                &format!(
                    "Saved revision {}. Workspace scope requires owner restart; other settings load live. Explicit CLI overrides still apply. Save is not native-effect confirmation.",
                    draft.revision
                ),
            );
        }
        Err(error) => status(app, &format!("Error: {error}. Nothing was written.")),
    }
}

pub(super) fn load_base(dir: &std::path::Path) -> (Settings, bool, Option<String>) {
    match crate::settings::load_from_dir(dir) {
        LoadOutcome::Loaded(settings) => (settings, true, None),
        LoadOutcome::Missing => (Settings::default(), false, None),
        LoadOutcome::Invalid(error) => (Settings::default(), false, Some(error.to_string())),
    }
}

pub(super) fn do_revert(app: &mut App) {
    // Revert discards every unsaved edit and reloads saved/default; it never
    // writes and never undoes an already-applied save.
    let (base, present, invalid) = load_base(&app.dir);
    app.base = base.clone();
    app.base_present = present;
    app.work = base;
    app.invalid = invalid.clone();
    refresh_all(app);
    match invalid {
        Some(reason) => status(
            app,
            &format!(
                "Error: settings file is invalid ({reason}). Showing defaults; Apply is disabled, nothing was written."
            ),
        ),
        None => status(
            app,
            "Reverted to saved settings. No disk write; prior Apply calls stand.",
        ),
    }
}

pub(super) fn do_preset(app: &mut App, preset: crate::settings::Preset) {
    apply_preset(&mut app.work, preset);
    if let Err(error) = validate_settings(&app.work) {
        // Presets over a valid draft always validate; a failure here means the
        // draft was already incoherent, so fall back to the saved base.
        app.work = app.base.clone();
        refresh_all(app);
        status(
            app,
            &format!("Error: preset refused ({error}). Draft reset to saved."),
        );
        return;
    }
    // Preset buttons never touch core controls; Apply still collects them.
    // A preset wins over any still-typed chord: clear it so a later Apply
    // cannot flush a stale edit over the staged preset.
    set_text(app.ctl(ID_BIND_CHORD), "");
    refresh_list(app);
    let name = match preset {
        crate::settings::Preset::Authentic => "Authentic",
        crate::settings::Preset::Compatible => "Compatible",
    };
    status(
        app,
        &format!("{name} staged (unsaved). Apply to save, Revert to discard."),
    );
}

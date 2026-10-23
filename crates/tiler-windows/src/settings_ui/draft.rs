//! Draft editing for the settings window: pushing saved values into controls,
//! collecting them back into a validated document, and the binding editor,
//! Apply/Revert/preset staging. Nothing here writes except `do_apply`'s
//! single atomic save (after stale-content refusal).
//!
//! All control effects delegate to [`crate::ffi::settings_controls`];
//! dialogs go through [`super::dialogs`]; no Win32 lives here.
#![forbid(unsafe_code)]

use crate::settings::{
    BindingSetting, BindingState, LoadOutcome, Settings, apply_preset, binding_catalog,
    parse_chord, validate_settings,
};

use super::choice::{
    anchor_fallback_selected, apply_anchor_fallback_selection, apply_born_maximized_selection,
    apply_fixed_size_selection, apply_fresh_placement_selection, apply_migration_refill_selection,
    apply_no_space_selection, apply_same_axis_selection, apply_switch_target_selection,
    apply_workspace_mode_selection, born_overlay_selected, fixed_size_either_selected,
    fresh_placement_selected, migration_refill_mru_selected, no_space_overlap_selected,
    same_axis_swap_selected, switch_focused_selected, workspace_mode_selected,
};
use super::ctl::{checked, enable, get_text, set_checked, set_text};
use super::dialogs::{DiscardEnd, PreviewEnd};
use super::info::refresh_list;
use super::{App, status};
use super::{
    ID_ALLOW_WIN_L, ID_ANCHOR_CLOSEST, ID_ANCHOR_LARGEST, ID_ANCHOR_NO_SEARCH, ID_APPLY,
    ID_BIND_CHORD, ID_BORDER_COLOR, ID_BORDER_ENABLED, ID_BORDER_GAP, ID_BORDER_RADIUS,
    ID_BORDER_THEME, ID_BORDER_WIDTH, ID_BORN_MAX_OVERLAY, ID_BORN_MAX_TILE, ID_CURSOR_WARP,
    ID_DEFAULT_FLOATING, ID_DEFAULT_TILED, ID_FIXED_SIZE_BOTH, ID_FIXED_SIZE_EITHER,
    ID_FOCUS_FOLLOWS, ID_INNER_GAP, ID_KEYBOARD_TAKEOVER, ID_MIGRATION_REFILL_LAST,
    ID_MIGRATION_REFILL_MRU, ID_MOUSE_SNAP, ID_NO_SPACE_FLOAT, ID_NO_SPACE_OVERLAP, ID_OUTER_GAP,
    ID_PLACE_CURSOR_CLOSEST, ID_PLACE_CURSOR_MRU, ID_PLACE_FOCUSED_MRU, ID_SAME_AXIS_GROUP,
    ID_SAME_AXIS_SWAP, ID_SWITCH_CURSOR, ID_SWITCH_FOCUSED, ID_TASKBAR_PAGER, ID_UNDERLAY_COLOR,
    ID_UNDERLAY_ENABLED, ID_UNDERLAY_EXTENSION, ID_WORKSPACE_MODE_GLOBAL, ID_WORKSPACE_MODE_LOCAL,
    ID_WORKSPACE_MODE_SHARED,
};

/// Push the saved/draft values into every control and rebuild the list.
/// Clears the HWND-dirty flag: every control now matches `work`, so only
/// a later user edit, staged binding, or pending chord can unsave it.
pub(super) fn refresh_all(app: &mut App) {
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
    // Best-effort admission D2 three-choice control: same
    // functional-labels-only rule; its own WS_GROUP triple below keeps it
    // independent from every pair above.
    let anchor = anchor_fallback_selected(&app.work);
    set_checked(app.ctl(ID_ANCHOR_LARGEST), anchor == 0);
    set_checked(app.ctl(ID_ANCHOR_CLOSEST), anchor == 1);
    set_checked(app.ctl(ID_ANCHOR_NO_SEARCH), anchor == 2);
    // Best-effort admission D3 two-choice control: same
    // functional-labels-only rule; its own WS_GROUP pair below keeps it
    // independent from every control above.
    let overlap = no_space_overlap_selected(&app.work);
    set_checked(app.ctl(ID_NO_SPACE_FLOAT), !overlap);
    set_checked(app.ctl(ID_NO_SPACE_OVERLAP), overlap);
    // Stage 2c two-choice control: cursor output versus the focused
    // window's output for numbered/relative/previous/trailing selections.
    // Same functional-labels-only rule; its own WS_GROUP pair below keeps
    // it independent from every control above.
    let switch_focused = switch_focused_selected(&app.work);
    set_checked(app.ctl(ID_SWITCH_CURSOR), !switch_focused);
    set_checked(app.ctl(ID_SWITCH_FOCUSED), switch_focused);
    // Stage 2c three-choice control: cursor output with its last-focused
    // window (COSMIC), cursor output with the cursor-closest window
    // (Hyprland), or focused output with its last-focused window
    // (i3/sway). Same functional-labels-only rule; its own WS_GROUP triple
    // below keeps it independent from every pair above.
    let placement = fresh_placement_selected(&app.work);
    set_checked(app.ctl(ID_PLACE_CURSOR_MRU), placement == 0);
    set_checked(app.ctl(ID_PLACE_CURSOR_CLOSEST), placement == 1);
    set_checked(app.ctl(ID_PLACE_FOCUSED_MRU), placement == 2);
    // Stage 4b cursor coupling checkboxes: hover focus plus cross-output
    // warp. Both stage directly; no preset touches them.
    set_checked(
        app.ctl(ID_FOCUS_FOLLOWS),
        app.work.core.focus_follows_cursor,
    );
    set_checked(app.ctl(ID_CURSOR_WARP), app.work.core.cursor_warp);
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
    app.dirty = false;
    super::info::refresh_badge(app);
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
    // Best-effort admission D2: the closest radio stages the exact
    // closest token, the no-search radio the exact no-search token,
    // otherwise the exact largest token; only these three values can reach
    // validation. The choice saves immediately but takes effect on
    // subsequent capacity decisions only (D10).
    let anchor_index = if checked(app.ctl(ID_ANCHOR_CLOSEST)) {
        1
    } else if checked(app.ctl(ID_ANCHOR_NO_SEARCH)) {
        2
    } else {
        0
    };
    apply_anchor_fallback_selection(&mut draft, anchor_index);
    // Best-effort admission D3: the overlap radio stages the exact
    // allow-overlap token, otherwise the exact float token; only these two
    // values can reach validation. Same subsequent-decisions-only timing.
    apply_no_space_selection(&mut draft, checked(app.ctl(ID_NO_SPACE_OVERLAP)));
    // Stage 2c: the focused radio stages the exact focused-output token,
    // otherwise the exact cursor-output token; only these two values can
    // reach validation. The choice saves immediately but takes effect on
    // subsequent selections only.
    apply_switch_target_selection(&mut draft, checked(app.ctl(ID_SWITCH_FOCUSED)));
    // Stage 2c: the cursor-closest radio stages the exact cursor-closest
    // token, the focused radio the exact focused-output token, otherwise
    // the exact cursor-output/last-focused token; only these three values
    // can reach validation. Same subsequent-admissions-only timing.
    let placement_index = if checked(app.ctl(ID_PLACE_CURSOR_CLOSEST)) {
        1
    } else if checked(app.ctl(ID_PLACE_FOCUSED_MRU)) {
        2
    } else {
        0
    };
    apply_fresh_placement_selection(&mut draft, placement_index);
    // Stage 4b: the hover-focus and cross-output-warp checkboxes stage
    // their bools directly; validation is the shared boolean gate.
    draft.core.focus_follows_cursor = checked(app.ctl(ID_FOCUS_FOLLOWS));
    draft.core.cursor_warp = checked(app.ctl(ID_CURSOR_WARP));
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

/// Pure stale verdict over one directory read against the window's base:
/// `Changed` when another writer created or changed the document since load
/// (full-document compare; a newly-appeared file counts even when its bytes
/// equal the defaults), `Vanished` when our base file disappeared, `Invalid`
/// carrying the reason, else `Fresh`. Both Apply refusal points (before the
/// preview and after its unbounded think time) share this predicate so the
/// second check cannot silently overwrite content written mid-preview.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum StaleVerdict {
    Fresh,
    Changed,
    Vanished,
    Invalid(String),
}

pub(super) fn stale_verdict(
    base: &Settings,
    base_present: bool,
    loaded: &LoadOutcome,
) -> StaleVerdict {
    match loaded {
        LoadOutcome::Loaded(current) if *current != *base || !base_present => StaleVerdict::Changed,
        LoadOutcome::Loaded(_) => StaleVerdict::Fresh,
        LoadOutcome::Missing if base_present => StaleVerdict::Vanished,
        LoadOutcome::Missing => StaleVerdict::Fresh,
        LoadOutcome::Invalid(error) => StaleVerdict::Invalid(error.to_string()),
    }
}

/// Apply one directory read as a stale refusal: reloads the newer values,
/// reports the full diagnostic on the status line plus a capped dialog, and
/// writes nothing. Returns `None` when the owner died inside the refusal
/// dialog (caller must abandon its `App` borrow); otherwise whether a
/// refusal fired (`Fresh` performs no dialog and returns `Some(false)`).
fn apply_stale_refusal(
    app: &mut App,
    owner: windows_sys::Win32::Foundation::HWND,
    loaded: LoadOutcome,
) -> Option<bool> {
    match stale_verdict(&app.base, app.base_present, &loaded) {
        StaleVerdict::Fresh => Some(false),
        StaleVerdict::Changed => {
            if let LoadOutcome::Loaded(current) = loaded {
                app.base = current.clone();
                app.work = current;
                app.base_present = true;
            }
            refresh_all(app);
            status(
                app,
                "Settings changed on disk (another window saved); reloaded. Re-apply your edits.",
            );
            if !super::dialogs::show_info(
                owner,
                super::dialogs::APPLY_TITLE,
                &super::dialogs::error_body(
                    "settings changed on disk; reloaded. Re-apply your edits",
                ),
            ) {
                return None;
            }
            Some(true)
        }
        StaleVerdict::Vanished => {
            app.base = Settings::default();
            app.work = Settings::default();
            app.base_present = false;
            refresh_all(app);
            status(
                app,
                "Settings file vanished; reloaded defaults. Re-apply your edits.",
            );
            if !super::dialogs::show_info(
                owner,
                super::dialogs::APPLY_TITLE,
                &super::dialogs::error_body("settings file vanished; reloaded defaults"),
            ) {
                return None;
            }
            Some(true)
        }
        StaleVerdict::Invalid(reason) => {
            app.invalid = Some(reason.clone());
            enable(app.ctl(ID_APPLY), false);
            status(
                app,
                &format!("Error: settings file turned invalid ({reason}). Nothing was written."),
            );
            if !super::dialogs::show_info(
                owner,
                super::dialogs::APPLY_TITLE,
                &super::dialogs::error_body(&reason),
            ) {
                return None;
            }
            Some(true)
        }
    }
}

/// Apply with conflict preview and result dialogs. Returns false only when
/// the owner died inside one of the owned modals (the `App` box was freed
/// with it): the caller must abandon its borrow immediately and touch
/// nothing, including the post-command badge refresh. Every modal exit is
/// checked; `WM_CLOSE` during a modal is deferred by the owner `wnd_proc`
/// (never run while borrowed), so OwnerGone is a backstop, not a path.
pub(super) fn do_apply(app: &mut App) -> bool {
    let owner = app.window;
    if app.invalid.is_some() {
        let detail = app.invalid.clone().unwrap_or_default();
        status(
            app,
            "Error: settings file is invalid; fix or delete it, then Revert. Nothing was written.",
        );
        if !super::dialogs::show_info(
            owner,
            super::dialogs::APPLY_TITLE,
            &super::dialogs::error_body(&detail),
        ) {
            return false;
        }
        return true;
    }
    let mut draft = match collect_draft(app) {
        Ok(draft) => draft,
        Err(reason) => {
            status(app, &format!("Error: {reason}. Nothing was written."));
            if !super::dialogs::show_info(
                owner,
                super::dialogs::APPLY_TITLE,
                &super::dialogs::error_body(&reason),
            ) {
                return false;
            }
            return true;
        }
    };
    // Stale-content refusal before the preview: another writer (e.g. a
    // second Settings window or another tool) created or changed the file
    // since this window loaded, so never silently overwrite unrelated edits.
    // A second Settings window cannot reach this race: the per-user named
    // mutex serializes UI instances.
    match apply_stale_refusal(app, owner, crate::settings::load_from_dir(&app.dir)) {
        Some(false) => {}
        Some(true) => return true,
        None => return false,
    }
    // Advisory conflict preview: the draft's current tray-warning set only
    // (no Force, no generic confirmation). Cancel writes nothing and opens
    // no second dialog.
    {
        let snapshot = app.gaming.snapshot();
        let conflicts = super::dialogs::preview_conflicts(
            &draft,
            draft.core.keyboard.takeover,
            draft.core.keyboard.allow_win_l,
            &snapshot,
        );
        if super::dialogs::apply_gate(conflicts.len()) == super::dialogs::ApplyGate::Preview {
            let body = super::dialogs::preview_body(&conflicts);
            match super::dialogs::ask_preview(owner, &body) {
                PreviewEnd::Apply => {}
                PreviewEnd::Cancel => {
                    status(app, "Apply cancelled; draft and file preserved.");
                    return true;
                }
                PreviewEnd::OwnerGone => return false,
            }
            // Revalidate after the unbounded preview think time: the disk
            // may have moved while the dialog sat open, and the save below
            // would otherwise overwrite that content (it only bumps the
            // revision over whatever it reads). Same refusal, no write.
            match apply_stale_refusal(app, owner, crate::settings::load_from_dir(&app.dir)) {
                Some(false) => {}
                Some(true) => return true,
                None => return false,
            }
        }
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
            if !super::dialogs::show_info(
                owner,
                super::dialogs::APPLY_TITLE,
                super::dialogs::APPLY_SUCCESS,
            ) {
                return false;
            }
        }
        Err(error) => {
            let detail = error.to_string();
            status(app, &format!("Error: {error}. Nothing was written."));
            if !super::dialogs::show_info(
                owner,
                super::dialogs::APPLY_TITLE,
                &super::dialogs::error_body(&detail),
            ) {
                return false;
            }
        }
    }
    true
}

pub(super) fn load_base(dir: &std::path::Path) -> (Settings, bool, Option<String>) {
    match crate::settings::load_from_dir(dir) {
        LoadOutcome::Loaded(settings) => (settings, true, None),
        LoadOutcome::Missing => (Settings::default(), false, None),
        LoadOutcome::Invalid(error) => (Settings::default(), false, Some(error.to_string())),
    }
}

pub(super) fn do_revert(app: &mut App) -> bool {
    // Revert discards every unsaved edit and reloads saved/default; it never
    // writes and never undoes an already-applied save.
    let owner = app.window;
    // Unsaved gate covers the whole draft: HWND-dirty edits, staged
    // `work != base` drift (presets, rebinding, keep/disable), and a
    // still-typed pending chord. Cancel leaves every edit intact.
    let pending = !get_text(app.ctl(ID_BIND_CHORD)).trim().is_empty();
    let unsaved = super::dialogs::has_unsaved_edits(app.dirty, app.work != app.base, pending);
    if super::dialogs::revert_gate(unsaved) == super::dialogs::RevertGate::ConfirmDiscard {
        match super::dialogs::ask_discard(owner, super::dialogs::DISCARD_PROMPT) {
            DiscardEnd::Discard => {}
            DiscardEnd::Cancel => {
                status(app, "Revert cancelled; edits preserved.");
                return true;
            }
            DiscardEnd::OwnerGone => return false,
        }
    }
    let (base, present, invalid) = load_base(&app.dir);
    app.base = base.clone();
    app.base_present = present;
    app.work = base;
    app.invalid = invalid.clone();
    refresh_all(app);
    match invalid {
        Some(reason) => {
            status(
                app,
                &format!(
                    "Error: settings file is invalid ({reason}). Showing defaults; Apply is disabled, nothing was written."
                ),
            );
            if !super::dialogs::show_info(
                owner,
                super::dialogs::REVERT_TITLE,
                &super::dialogs::error_body(&reason),
            ) {
                return false;
            }
        }
        None => {
            status(
                app,
                "Reverted to saved settings. No disk write; prior Apply calls stand.",
            );
            if !super::dialogs::show_info(
                owner,
                super::dialogs::REVERT_TITLE,
                super::dialogs::REVERT_SUCCESS,
            ) {
                return false;
            }
        }
    }
    true
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

#[cfg(test)]
mod tests {
    use super::stale_verdict;
    use super::{LoadOutcome, Settings, StaleVerdict};
    use crate::settings::SettingsError;

    fn changed_doc() -> Settings {
        let mut settings = Settings::default();
        settings.core.inner_gap = 7;
        settings
    }

    #[test]
    fn stale_verdict_pins_both_refusal_points() {
        let base = Settings::default();
        // Fresh: identical document, file present at load.
        assert_eq!(
            stale_verdict(&base, true, &LoadOutcome::Loaded(base.clone())),
            StaleVerdict::Fresh
        );
        // Changed content refuses even though the revision still matches.
        assert_eq!(
            stale_verdict(&base, true, &LoadOutcome::Loaded(changed_doc())),
            StaleVerdict::Changed
        );
        // Newly-appeared file counts as changed even when its bytes equal
        // the defaults: two fresh windows must not silently merge.
        assert_eq!(
            stale_verdict(&base, false, &LoadOutcome::Loaded(base.clone())),
            StaleVerdict::Changed
        );
        // Our base file vanished refuses; a window born missing stays fresh.
        assert_eq!(
            stale_verdict(&base, true, &LoadOutcome::Missing),
            StaleVerdict::Vanished
        );
        assert_eq!(
            stale_verdict(&base, false, &LoadOutcome::Missing),
            StaleVerdict::Fresh
        );
        // Invalid files report their reason through the same refusal.
        let verdict = stale_verdict(&base, true, &LoadOutcome::Invalid(SettingsError::Malformed));
        match verdict {
            StaleVerdict::Invalid(reason) => assert!(!reason.is_empty()),
            other => panic!("expected Invalid, got {other:?}"),
        }
    }
}

//! Native Win32 settings window (`cfg(windows)` only).
//!
//! Mechanism: one overlapped settings window with plain Win32 child controls
//! (`STATIC`, `EDIT`, `BUTTON`, `LISTBOX`) created through the existing
//! `windows-sys` route. No new dependency, no registry/policy writes, no tray.
//! The window edits a draft [`crate::settings::Settings`]; Apply validates the
//! whole draft (including the selected binding editor) and saves atomically,
//! Revert reloads the last saved values without writing, and Close never
//! applies. The running owner adopts saved revisions live on its own pump
//! (`settings-applied`); this window never claims the save as applied.
//!
//! Stable automation surface: window class [`SETTINGS_WINDOW_CLASS`] with the
//! control ids below, launched via `tiler-windows settings` (plus the
//! `settings` just recipe through the Explorer broker) and closed with the
//! Close button, `Alt+F4`, `Escape`, or `WM_CLOSE`.
//!
//! Concurrency is stale-content refusal plus one UI instance, not a file
//! lock: Apply re-reads the file and refuses (reloading the newer values)
//! when another writer created or changed the document since this window
//! loaded (full-document compare, with a missing file distinguished from a
//! saved revision-1 file). A per-user named mutex (`Local\` plus the caller
//! SID) keeps a second Settings window from racing the first: the second
//! call brings the existing window forward and returns. Edits from other
//! tools stay best-effort freshness. An invalid on-disk file shows its error
//! and disables Apply; nothing is ever silently reset.

use std::path::PathBuf;

use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::Graphics::Gdi::HFONT;

use crate::settings::{
    BindingDef, BindingSetting, BindingState, LoadOutcome, Settings, apply_preset, binding_catalog,
    effective_bindings, parse_chord, validate_settings,
};

type DynError = Box<dyn std::error::Error>;

fn err(msg: impl Into<String>) -> DynError {
    Box::new(std::io::Error::other(msg.into()))
}

/// Native window class for the settings window.
pub const SETTINGS_WINDOW_CLASS: &str = "OmniTilerSettings";
/// Window title for the settings window.
pub const SETTINGS_WINDOW_TITLE: &str = "OmniTiler Settings";

// Stable control ids for later `SendMessage` automation.
pub const ID_INNER_GAP: u32 = 101;
pub const ID_OUTER_GAP: u32 = 102;
pub const ID_BORDER_ENABLED: u32 = 103;
pub const ID_BORDER_THEME: u32 = 104;
pub const ID_BORDER_COLOR: u32 = 105;
pub const ID_BORDER_WIDTH: u32 = 106;
pub const ID_BORDER_GAP: u32 = 107;
pub const ID_BORDER_RADIUS: u32 = 108;
pub const ID_UNDERLAY_ENABLED: u32 = 109;
pub const ID_UNDERLAY_COLOR: u32 = 110;
pub const ID_UNDERLAY_EXTENSION: u32 = 111;
pub const ID_KEYBOARD_TAKEOVER: u32 = 112;
pub const ID_MOUSE_SNAP: u32 = 113;
pub const ID_SNAP_MEANING: u32 = 114;
pub const ID_ALLOW_WIN_L: u32 = 115;
pub const ID_LOCK_WARNING: u32 = 116;
pub const ID_PRESET_AUTHENTIC: u32 = 117;
pub const ID_PRESET_COMPATIBLE: u32 = 118;
pub const ID_PRESET_EXPLAIN: u32 = 119;
pub const ID_BINDING_LIST: u32 = 120;
pub const ID_BINDING_INFO: u32 = 121;
pub const ID_BIND_KEEP: u32 = 122;
pub const ID_BIND_DISABLE: u32 = 123;
pub const ID_BIND_CHORD: u32 = 124;
pub const ID_BIND_REBIND: u32 = 125;
pub const ID_REBIND_NOTE: u32 = 126;
pub const ID_APPLY: u32 = 127;
pub const ID_REVERT: u32 = 128;
pub const ID_CLOSE: u32 = 129;
pub const ID_STATUS: u32 = 130;
pub const ID_DEFAULT_TILED: u32 = 131;
pub const ID_DEFAULT_FLOATING: u32 = 132;
pub const ID_SAME_AXIS_GROUP: u32 = 133;
pub const ID_SAME_AXIS_SWAP: u32 = 134;
pub const ID_BORN_MAX_TILE: u32 = 135;
pub const ID_BORN_MAX_OVERLAY: u32 = 136;

const ESCAPE_VK: usize = 0x1B;

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain([0]).collect()
}

fn set_text(hwnd: HWND, text: &str) {
    use windows_sys::Win32::UI::WindowsAndMessaging::SetWindowTextW;
    let text_w = wide(text);
    unsafe {
        SetWindowTextW(hwnd, text_w.as_ptr());
    }
}

fn get_text(hwnd: HWND) -> String {
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindowTextLengthW, GetWindowTextW};
    let len = unsafe { GetWindowTextLengthW(hwnd) };
    if len <= 0 {
        return String::new();
    }
    let capped = len.min(512) as usize;
    let mut buf = vec![0u16; capped + 1];
    let read = unsafe { GetWindowTextW(hwnd, buf.as_mut_ptr(), capped as i32 + 1) };
    if read <= 0 {
        return String::new();
    }
    String::from_utf16_lossy(&buf[..read as usize])
}

fn checked(hwnd: HWND) -> bool {
    use windows_sys::Win32::UI::Controls::BST_CHECKED;
    use windows_sys::Win32::UI::WindowsAndMessaging::{BM_GETCHECK, SendMessageW};
    unsafe { SendMessageW(hwnd, BM_GETCHECK, 0, 0) == isize::try_from(BST_CHECKED).unwrap_or(1) }
}

fn set_checked(hwnd: HWND, value: bool) {
    use windows_sys::Win32::UI::Controls::{BST_CHECKED, BST_UNCHECKED};
    use windows_sys::Win32::UI::WindowsAndMessaging::{BM_SETCHECK, SendMessageW};
    let flag = if value { BST_CHECKED } else { BST_UNCHECKED };
    unsafe {
        SendMessageW(hwnd, BM_SETCHECK, flag as usize, 0);
    }
}

fn enable(hwnd: HWND, value: bool) {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::EnableWindow;
    unsafe {
        EnableWindow(hwnd, if value { 1 } else { 0 });
    }
}

/// Draft plus saved state for one open settings window.
struct App {
    font: HFONT,
    controls: Vec<(u32, HWND)>,
    dir: PathBuf,
    /// Last-good saved values (what Revert restores).
    base: Settings,
    /// True when `base` came from an existing file; false when the file was
    /// absent at load (defaults). Distinguishes a missing file from a saved
    /// revision-1 file so two fresh windows cannot silently overwrite each
    /// other.
    base_present: bool,
    /// Editable draft; presets and the binding editor mutate the bindings,
    /// Apply additionally reads every core control into the core section.
    work: Settings,
    /// Selection into the catalog order backing the listbox.
    selected: Option<usize>,
    /// Set while the on-disk file is invalid: Apply stays disabled.
    invalid: Option<String>,
    /// R-MOV-03 native tooltip text buffers (wide, NUL-terminated), one per
    /// radio, built from the shared options table, followed by the two
    /// R-MAX-06 born-maximized buffers in the same order. Owned by the boxed
    /// `App` so the pointers handed to the tooltip control stay valid for the
    /// whole window lifetime: the tooltip is a child window, destroyed with
    /// its parent before `WM_DESTROY` reclaims this box.
    tip_text: [Vec<u16>; 4],
}

impl App {
    fn ctl(&self, id: u32) -> HWND {
        self.controls
            .iter()
            .find(|(known, _)| *known == id)
            .map(|(_, hwnd)| *hwnd)
            .unwrap_or(std::ptr::null_mut())
    }
}

fn status(app: &App, text: &str) {
    set_text(app.ctl(ID_STATUS), text);
}

fn catalog_index_of(id: &str) -> Option<usize> {
    binding_catalog().iter().position(|def| def.id == id)
}

/// One listbox row: action, live chord(s), and the honest physical OS
/// conflict. Conflicts come from [`effective_bindings`] over the draft, so a
/// rebound row reports the rebound chord's owner, never the stale original.
fn row_text(def: &BindingDef, work: &Settings) -> String {
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
                    let short: String = conflict.chars().take(96).collect();
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

fn refresh_list(app: &App) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        LB_ADDSTRING, LB_RESETCONTENT, LB_SETCURSEL, SendMessageW,
    };
    let list = app.ctl(ID_BINDING_LIST);
    unsafe {
        SendMessageW(list, LB_RESETCONTENT, 0, 0);
    }
    for def in binding_catalog() {
        let text = wide(&row_text(&def, &app.work));
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
/// OS actually owns at those chords.
fn refresh_info(app: &App) {
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
            Some(conflict) => info.push_str(&format!("\nOS: {conflict}")),
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
    set_text(app.ctl(ID_BINDING_INFO), &info.replace('\n', "\r\n"));
}

/// R-MOV-03 radio selection from saved/draft settings: true selects the
/// flat-swap radio, false the group default. Unknown values (possible only
/// in hand-built drafts, never from disk) show the group default.
fn same_axis_swap_selected(settings: &Settings) -> bool {
    settings.core.same_axis_move == crate::settings::SAME_AXIS_MOVE_SWAP
}

/// R-MOV-03 draft staging from the swap radio state: writes the exact wire
/// token for the selected choice, so Apply persists a validated value.
fn apply_same_axis_selection(draft: &mut Settings, swap: bool) {
    draft.core.same_axis_move = if swap {
        crate::settings::SAME_AXIS_MOVE_SWAP.to_owned()
    } else {
        crate::settings::SAME_AXIS_MOVE_GROUP.to_owned()
    };
}

/// R-MOV-03 native tooltip text buffers from the shared options table: the
/// WM reference per choice (`COSMIC` / `i3, sway`), wide-encoded for the
/// tooltip control. Radio labels stay functional-only per the naming rule.
fn same_axis_tip_texts() -> [Vec<u16>; 2] {
    let options = crate::settings::same_axis_move_options();
    [wide(options[0].2), wide(options[1].2)]
}

/// R-MAX-06 radio selection from saved/draft settings: true selects the
/// keep-overlay radio, false the tile-on-launch default. Unknown values
/// (possible only in hand-built drafts, never from disk) show the default.
fn born_overlay_selected(settings: &Settings) -> bool {
    settings.core.born_maximized == crate::settings::BORN_MAXIMIZED_OVERLAY
}

/// R-MAX-06 draft staging from the overlay radio state: writes the exact wire
/// token for the selected choice, so Apply persists a validated value.
fn apply_born_maximized_selection(draft: &mut Settings, overlay: bool) {
    draft.core.born_maximized = if overlay {
        crate::settings::BORN_MAXIMIZED_OVERLAY.to_owned()
    } else {
        crate::settings::BORN_MAXIMIZED_TILE.to_owned()
    };
}

/// R-MAX-06 native tooltip text buffers from the shared options table: the
/// WM reference per choice (`bspwm, i3, xmonad, sway, qtile` /
/// `COSMIC, Hyprland`), wide-encoded for the tooltip control. Radio labels
/// stay functional-only per the naming rule.
fn born_maximized_tip_texts() -> [Vec<u16>; 2] {
    let options = crate::settings::born_maximized_options();
    [wide(options[0].2), wide(options[1].2)]
}

/// All four two-choice radio tooltip buffers in control order: the two
/// R-MOV-03 buffers then the two R-MAX-06 buffers.
fn choice_tip_texts() -> [Vec<u16>; 4] {
    let axis = same_axis_tip_texts();
    let born = born_maximized_tip_texts();
    let [a0, a1] = axis;
    let [b0, b1] = born;
    [a0, a1, b0, b1]
}

/// Attach native tooltips naming the reference WM to the two R-MOV-03
/// radios and the two R-MAX-06 born-maximized radios. One
/// `tooltips_class32` child (`TTS_ALWAYSTIP`) with `TTF_SUBCLASS` tools, so
/// hover tracking needs no manual relay. Text pointers ride the `App`-owned
/// `tip_text` buffers (see the field docs for the lifetime). Best-effort: a
/// failed tooltip leaves the functional dialog untouched.
fn attach_choice_tips(app: &App, hwnd: HWND, hinst: windows_sys::Win32::Foundation::HINSTANCE) {
    use windows_sys::Win32::UI::Controls::{
        ICC_BAR_CLASSES, INITCOMMONCONTROLSEX, InitCommonControlsEx, TTF_IDISHWND, TTF_SUBCLASS,
        TTM_ADDTOOLW, TTS_ALWAYSTIP, TTTOOLINFOW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{CreateWindowExW, SendMessageW, WS_POPUP};
    let icc = INITCOMMONCONTROLSEX {
        dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
        dwICC: ICC_BAR_CLASSES,
    };
    unsafe {
        InitCommonControlsEx(&icc);
    }
    let class_w = wide("tooltips_class32");
    let tip = unsafe {
        CreateWindowExW(
            0,
            class_w.as_ptr(),
            std::ptr::null(),
            WS_POPUP | TTS_ALWAYSTIP,
            0,
            0,
            0,
            0,
            hwnd,
            std::ptr::null_mut(),
            hinst,
            std::ptr::null(),
        )
    };
    if tip.is_null() {
        return;
    }
    for (id, text) in [
        (ID_SAME_AXIS_GROUP, &app.tip_text[0]),
        (ID_SAME_AXIS_SWAP, &app.tip_text[1]),
        (ID_BORN_MAX_TILE, &app.tip_text[2]),
        (ID_BORN_MAX_OVERLAY, &app.tip_text[3]),
    ] {
        let control = app.ctl(id);
        if control.is_null() {
            continue;
        }
        let info = TTTOOLINFOW {
            cbSize: std::mem::size_of::<TTTOOLINFOW>() as u32,
            uFlags: TTF_IDISHWND | TTF_SUBCLASS,
            hwnd,
            uId: control as usize,
            rect: unsafe { std::mem::zeroed() },
            hinst: std::ptr::null_mut(),
            lpszText: text.as_ptr() as *mut u16,
            lParam: 0,
            lpReserved: std::ptr::null_mut(),
        };
        unsafe {
            SendMessageW(tip, TTM_ADDTOOLW, 0, &info as *const TTTOOLINFOW as isize);
        }
    }
}

/// Push the saved/draft values into every control and rebuild the list.
fn refresh_all(app: &App) {
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
fn collect_draft(app: &mut App) -> Result<Settings, String> {
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
    // R-MOV-03: the swap radio stages the exact swap token, otherwise the
    // exact group token; only these two values can reach validation.
    apply_same_axis_selection(&mut draft, checked(app.ctl(ID_SAME_AXIS_SWAP)));
    // R-MAX-06: the overlay radio stages the exact overlay token, otherwise
    // the exact tile token; only these two values can reach validation.
    apply_born_maximized_selection(&mut draft, checked(app.ctl(ID_BORN_MAX_OVERLAY)));
    draft.revision = app.base.revision;
    validate_settings(&draft).map_err(|e| e.to_string())?;
    Ok(draft)
}

/// Validate one rebind against the selected row and stage it in the draft.
/// The Shift/Ctrl/Alt arm must match one of the action's native arms
/// (history rows ride Win+Ctrl, relative sends ride Win+Ctrl+Shift,
/// numbered stay rides Win+Shift plus Win+Ctrl+Shift, resize-out rides
/// Win+Alt and resize-in rides Win+Shift+Alt).
fn apply_rebind_text(app: &mut App, text: &str) -> Result<(), String> {
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

fn stage_state(app: &mut App, state: BindingState) -> Result<(), String> {
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

fn do_apply(app: &mut App) {
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
                    "Saved revision {}. Normal owners load it live; explicit CLI overrides still apply. Save is not native-effect confirmation.",
                    draft.revision
                ),
            );
        }
        Err(error) => status(app, &format!("Error: {error}. Nothing was written.")),
    }
}

fn load_base(dir: &std::path::Path) -> (Settings, bool, Option<String>) {
    match crate::settings::load_from_dir(dir) {
        LoadOutcome::Loaded(settings) => (settings, true, None),
        LoadOutcome::Missing => (Settings::default(), false, None),
        LoadOutcome::Invalid(error) => (Settings::default(), false, Some(error.to_string())),
    }
}

fn do_revert(app: &mut App) {
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

fn do_preset(app: &mut App, preset: crate::settings::Preset) {
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

fn on_command(app: &mut App, id: u32, notify: u32) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        BN_CLICKED, LB_GETCURSEL, LB_SETCURSEL, LBN_SELCHANGE, SendMessageW,
    };
    if id == ID_BINDING_LIST {
        if notify == LBN_SELCHANGE {
            let picked = unsafe { SendMessageW(app.ctl(ID_BINDING_LIST), LB_GETCURSEL, 0, 0) };
            if picked >= 0 {
                // A typed-but-unset chord belongs to the old row: flush it
                // there first. On refusal the edit is preserved, the old row
                // is re-selected, and the switch does not happen silently.
                let pending = get_text(app.ctl(ID_BIND_CHORD));
                if !pending.trim().is_empty() {
                    if let Err(reason) = apply_rebind_text(app, &pending) {
                        if let Some(old) = app.selected {
                            unsafe {
                                SendMessageW(app.ctl(ID_BINDING_LIST), LB_SETCURSEL, old, 0);
                            }
                        }
                        status(
                            app,
                            &format!(
                                "Error: {reason}. Press Set rebind (or clear the edit) before switching rows."
                            ),
                        );
                        return;
                    }
                    set_text(app.ctl(ID_BIND_CHORD), "");
                }
                app.selected = Some(picked as usize);
                refresh_info(app);
            }
        }
        return;
    }
    if notify != BN_CLICKED {
        return;
    }
    match id {
        ID_APPLY => do_apply(app),
        ID_REVERT => do_revert(app),
        // ID_CLOSE never reaches here: `wnd_proc` destroys the window without
        // materializing the `App` borrow (see below), so the synchronous
        // `WM_DESTROY` free below never nests inside a live `&mut App`.
        ID_PRESET_AUTHENTIC => do_preset(app, crate::settings::Preset::Authentic),
        ID_PRESET_COMPATIBLE => do_preset(app, crate::settings::Preset::Compatible),
        ID_BIND_KEEP => {
            if let Err(reason) = stage_state(app, BindingState::Keep) {
                status(app, &format!("Error: {reason}."));
            }
        }
        ID_BIND_DISABLE => {
            if let Err(reason) = stage_state(app, BindingState::Disabled) {
                status(app, &format!("Error: {reason}."));
            }
        }
        ID_BIND_REBIND => {
            let text = get_text(app.ctl(ID_BIND_CHORD));
            if let Err(reason) = apply_rebind_text(app, &text) {
                status(app, &format!("Error: {reason}."));
            } else {
                set_text(app.ctl(ID_BIND_CHORD), "");
                status(
                    app,
                    "Rebind staged (unsaved). Apply to save, Revert to discard.",
                );
            }
        }
        _ => {}
    }
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: windows_sys::Win32::Foundation::WPARAM,
    lparam: windows_sys::Win32::Foundation::LPARAM,
) -> windows_sys::Win32::Foundation::LRESULT {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CREATESTRUCTW, DefWindowProcW, DestroyWindow, GWLP_USERDATA, GetWindowLongPtrW,
        PostQuitMessage, SetWindowLongPtrW, WM_CLOSE, WM_COMMAND, WM_CREATE, WM_DESTROY,
    };
    match msg {
        WM_CREATE => {
            let create = unsafe { &*(lparam as *const CREATESTRUCTW) };
            unsafe {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
            }
            0
        }
        WM_COMMAND => {
            let id = (wparam & 0xffff) as u32;
            if id == ID_CLOSE {
                // Close without borrowing `App`: `DestroyWindow` runs
                // `WM_DESTROY` synchronously, which reclaims the box. Routing
                // through `on_command` would nest that free inside a live
                // `&mut App`; destroying here keeps exactly one owner.
                unsafe {
                    DestroyWindow(hwnd);
                }
                return 0;
            }
            let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) };
            if ptr != 0 {
                let app = unsafe { &mut *(ptr as *mut App) };
                let notify = ((wparam >> 16) & 0xffff) as u32;
                on_command(app, id, notify);
            }
            0
        }
        WM_CLOSE => {
            unsafe {
                DestroyWindow(hwnd);
            }
            0
        }
        WM_DESTROY => {
            let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) };
            if ptr != 0 {
                unsafe {
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                    let app = Box::from_raw(ptr as *mut App);
                    if !app.font.is_null() {
                        windows_sys::Win32::Graphics::Gdi::DeleteObject(app.font);
                    }
                    drop(app);
                }
            }
            unsafe {
                PostQuitMessage(0);
            }
            0
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

struct Ctl {
    id: u32,
    class: &'static str,
    text: String,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    style: u32,
}

fn px(value: i32, dpi: u32) -> i32 {
    ((i64::from(value) * i64::from(dpi) + 48) / 96) as i32
}

/// Create one child control with the DPI-scaled message font.
/// `WS_CLIPSIBLINGS` plus the parent's `WS_CLIPCHILDREN` keep the groupbox
/// frames from smearing over their children; the groupboxes are additionally
/// pinned to the bottom of the sibling order after creation (see below),
/// because creation order alone left the first-created groupboxes topmost and
/// each opaque groupbox then covered every control inside it. Returns the new
/// control handle.
#[allow(clippy::too_many_arguments)]
fn create_ctl(
    parent: HWND,
    hinst: windows_sys::Win32::Foundation::HINSTANCE,
    ctl: &Ctl,
    dpi: u32,
    font: HFONT,
    out: &mut Vec<(u32, HWND)>,
) -> Result<HWND, DynError> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, WM_SETFONT, WS_CHILD, WS_CLIPSIBLINGS, WS_VISIBLE,
    };
    let class_w = wide(ctl.class);
    let text_w = wide(&ctl.text);
    let hwnd = unsafe {
        CreateWindowExW(
            0,
            class_w.as_ptr(),
            text_w.as_ptr(),
            WS_CHILD | WS_VISIBLE | WS_CLIPSIBLINGS | ctl.style,
            px(ctl.x, dpi),
            px(ctl.y, dpi),
            px(ctl.w, dpi),
            px(ctl.h, dpi),
            parent,
            ctl.id as isize as windows_sys::Win32::UI::WindowsAndMessaging::HMENU,
            hinst,
            std::ptr::null(),
        )
    };
    if hwnd.is_null() {
        return Err(err(format!("error: settings control {} failed", ctl.id)));
    }
    if !font.is_null() {
        use windows_sys::Win32::UI::WindowsAndMessaging::SendMessageW;
        unsafe {
            SendMessageW(hwnd, WM_SETFONT, font as usize, 1);
        }
    }
    out.push((ctl.id, hwnd));
    Ok(hwnd)
}

/// Bounded native input length per edit control (the `get_text` 512 cap stays
/// as backstop). Group/label ids carry no limit.
fn edit_limit(id: u32) -> Option<usize> {
    match id {
        ID_INNER_GAP | ID_OUTER_GAP => Some(2),
        ID_BORDER_COLOR => Some(7),
        ID_BORDER_WIDTH | ID_BORDER_GAP | ID_BORDER_RADIUS => Some(5),
        ID_UNDERLAY_COLOR => Some(9),
        ID_UNDERLAY_EXTENSION => Some(5),
        ID_BIND_CHORD => Some(24),
        _ => None,
    }
}

/// Tear down a half-built window after a control-creation failure. `WM_DESTROY`
/// owns the box through `GWLP_USERDATA`, so detach first: the synchronous
/// `DestroyWindow` below then reclaims nothing, and this drop is the single
/// owner. Exactly one free either way.
fn abort_create(hwnd: HWND, raw: *mut App) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DestroyWindow, GWLP_USERDATA, SetWindowLongPtrW,
    };
    unsafe {
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
        DestroyWindow(hwnd);
        drop(Box::from_raw(raw));
    }
}

fn message_font(dpi: u32) -> HFONT {
    use windows_sys::Win32::Graphics::Gdi::{CreateFontW, DEFAULT_GUI_FONT, GetStockObject};
    let height = -((9 * dpi as i32 + 36) / 72);
    let face = wide("Segoe UI");
    // Weight/style/charset defaults; the stock fallback covers failure.
    let font = unsafe { CreateFontW(height, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, face.as_ptr()) };
    if font.is_null() {
        unsafe { GetStockObject(DEFAULT_GUI_FONT) }
    } else {
        font
    }
}

/// One held per-user settings mutex for the life of the open window.
struct SingletonGuard {
    handle: windows_sys::Win32::Foundation::HANDLE,
}

impl Drop for SingletonGuard {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            unsafe {
                windows_sys::Win32::Foundation::CloseHandle(self.handle);
            }
        }
    }
}

/// Bring an already-open settings window forward (restore then foreground).
fn focus_existing_settings() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        FindWindowW, SW_RESTORE, SetForegroundWindow, ShowWindow,
    };
    let class_w = wide(SETTINGS_WINDOW_CLASS);
    let hwnd = unsafe { FindWindowW(class_w.as_ptr(), std::ptr::null()) };
    if hwnd.is_null() {
        return;
    }
    unsafe {
        ShowWindow(hwnd, SW_RESTORE);
        SetForegroundWindow(hwnd);
    }
}

/// Acquire the per-user settings singleton (`Local\` plus the caller SID).
/// `Ok(None)` means another Settings window already holds it: the existing
/// window was brought forward and the caller must return without writing.
fn acquire_settings_singleton() -> Result<Option<SingletonGuard>, DynError> {
    use windows_sys::Win32::Foundation::GetLastError;
    use windows_sys::Win32::System::Threading::CreateMutexW;
    let sid = crate::native::current_identity()
        .map_err(|error| err(format!("error: settings identity: {error}")))?
        .user_sid;
    let safe: String = sid
        .chars()
        .map(|c| if c == '\\' || c == '/' { '_' } else { c })
        .collect();
    let name = format!("Local\\OmniTilerSettings-{safe}");
    let name_w = wide(&name);
    let handle = unsafe { CreateMutexW(std::ptr::null(), 0, name_w.as_ptr()) };
    if handle.is_null() {
        return Err(err("error: settings single-instance lock failed"));
    }
    // 183 = ERROR_ALREADY_EXISTS: we do not own the mutex, so close our
    // handle, forward the existing window, and report the clean refusal.
    if unsafe { GetLastError() } == 183 {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(handle);
        }
        focus_existing_settings();
        return Ok(None);
    }
    Ok(Some(SingletonGuard { handle }))
}

/// Open the modal settings window and run its message loop. Returns when the
/// window closes; Close never applies.
pub fn cmd_settings() -> Result<String, DynError> {
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::System::SystemServices::SS_LEFT;
    use windows_sys::Win32::UI::HiDpi::GetDpiForSystem;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        AdjustWindowRect, BS_AUTOCHECKBOX, BS_AUTORADIOBUTTON, BS_DEFPUSHBUTTON, BS_GROUPBOX,
        BS_PUSHBUTTON, CreateWindowExW, DispatchMessageW, ES_AUTOHSCROLL, ES_AUTOVSCROLL,
        ES_MULTILINE, ES_READONLY, GetMessageW, IDC_ARROW, IsDialogMessageW, LBS_NOINTEGRALHEIGHT,
        LBS_NOTIFY, LoadCursorW, RegisterClassW, SW_SHOWNORMAL, ShowWindow, TranslateMessage,
        WM_KEYDOWN, WNDCLASSW, WS_BORDER, WS_CAPTION, WS_CLIPCHILDREN, WS_EX_DLGMODALFRAME,
        WS_GROUP, WS_HSCROLL, WS_MINIMIZEBOX, WS_SYSMENU, WS_TABSTOP, WS_VSCROLL,
    };
    crate::tiling_sys::ensure_pm_v2()?;
    let _singleton = match acquire_settings_singleton()? {
        Some(guard) => guard,
        None => return Ok("settings already open".to_owned()),
    };
    let dir = crate::native::settings_directory().map_err(|e| err(format!("error: {e}")))?;
    let (base, present, invalid) = load_base(&dir);
    let dpi = unsafe { GetDpiForSystem() };
    let dpi = if dpi == 0 { 96 } else { dpi };
    let font = message_font(dpi);

    let hinst = unsafe { GetModuleHandleW(std::ptr::null()) };
    let class_w = wide(SETTINGS_WINDOW_CLASS);
    let mut cls: WNDCLASSW = unsafe { std::mem::zeroed() };
    cls.lpfnWndProc = Some(wnd_proc);
    cls.hInstance = hinst;
    cls.hCursor = unsafe { LoadCursorW(std::ptr::null_mut(), IDC_ARROW) };
    // System dialog-face brush: a zero brush leaves the background dark and
    // unpainted. The brush is owned by the system, never deleted here.
    cls.hbrBackground = unsafe {
        windows_sys::Win32::Graphics::Gdi::GetSysColorBrush(
            windows_sys::Win32::Graphics::Gdi::COLOR_BTNFACE,
        )
    };
    cls.lpszClassName = class_w.as_ptr();
    let atom = unsafe { RegisterClassW(&cls) };
    if atom == 0 {
        let code = unsafe { windows_sys::Win32::Foundation::GetLastError() };
        if code != 1410 {
            return Err(err("error: settings RegisterClass failed"));
        }
    }

    let style = WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_CLIPCHILDREN;
    let client_w = px(924, dpi);
    let client_h = px(924, dpi);
    let mut rect = windows_sys::Win32::Foundation::RECT {
        left: 0,
        top: 0,
        right: client_w,
        bottom: client_h,
    };
    unsafe {
        AdjustWindowRect(&mut rect, style, 0);
    }
    let title_w = wide(SETTINGS_WINDOW_TITLE);
    // Boxed before creation; reclaimed in WM_DESTROY. Pointer travels via
    // CREATESTRUCT lpCreateParams into GWLP_USERDATA.
    let app = Box::new(App {
        font,
        controls: Vec::new(),
        dir,
        base: base.clone(),
        base_present: present,
        work: base,
        selected: Some(catalog_index_of("focus-left").unwrap_or(0)),
        invalid: invalid.clone(),
        tip_text: choice_tip_texts(),
    });
    let raw = Box::into_raw(app);
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_DLGMODALFRAME,
            class_w.as_ptr(),
            title_w.as_ptr(),
            style,
            120,
            80,
            rect.right - rect.left,
            rect.bottom - rect.top,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            hinst,
            raw.cast(),
        )
    };
    if hwnd.is_null() {
        // No window exists, so WM_CREATE never ran and WM_DESTROY never will:
        // the manual drop is the single owner (no WM_NCDESTROY reclaim here).
        unsafe {
            drop(Box::from_raw(raw));
        }
        return Err(err("error: settings CreateWindow failed"));
    }
    let tab = WS_TABSTOP;
    let edits = ES_AUTOHSCROLL as u32;
    let push = BS_PUSHBUTTON as u32;
    let check = BS_AUTOCHECKBOX as u32;
    let radio = BS_AUTORADIOBUTTON as u32;
    // First radio of each auto pair starts its own WS_GROUP: without this
    // every auto radio in the dialog shares one exclusion group, so picking
    // a same-axis choice would clear the workspace default (and vice versa).
    let radio_first = radio | WS_GROUP;
    let group = BS_GROUPBOX as u32;
    let defpush = BS_DEFPUSHBUTTON as u32;
    let label = SS_LEFT;
    let list_style =
        LBS_NOTIFY as u32 | LBS_NOINTEGRALHEIGHT as u32 | WS_VSCROLL | WS_HSCROLL | WS_BORDER;
    let edit_style = WS_BORDER | edits;
    // Selected-row readout: a readonly multiline edit with its own scrollbar
    // so the full OS-conflict text stays readable instead of clipping in an
    // 84px static.
    let info_style = ES_MULTILINE as u32
        | ES_AUTOVSCROLL as u32
        | ES_READONLY as u32
        | WS_VSCROLL
        | WS_BORDER
        | tab;
    let controls = [
        Ctl { id: 0, class: "BUTTON", text: "Gaps, tiles (0..64)".to_owned(), x: 10, y: 8, w: 272, h: 64, style: group },
        Ctl { id: 0, class: "STATIC", text: "Inner:".to_owned(), x: 20, y: 32, w: 52, h: 20, style: label },
        Ctl { id: ID_INNER_GAP, class: "EDIT", text: String::new(), x: 76, y: 30, w: 56, h: 22, style: edit_style | tab },
        Ctl { id: 0, class: "STATIC", text: "Outer:".to_owned(), x: 142, y: 32, w: 52, h: 20, style: label },
        Ctl { id: ID_OUTER_GAP, class: "EDIT", text: String::new(), x: 198, y: 30, w: 56, h: 22, style: edit_style | tab },
        Ctl { id: 0, class: "BUTTON", text: "Active border".to_owned(), x: 292, y: 8, w: 320, h: 200, style: group },
        Ctl { id: ID_BORDER_ENABLED, class: "BUTTON", text: "Enabled".to_owned(), x: 302, y: 28, w: 200, h: 20, style: check | tab },
        Ctl { id: ID_BORDER_THEME, class: "BUTTON", text: "Use system accent (theme)".to_owned(), x: 302, y: 50, w: 290, h: 20, style: check | tab },
        Ctl { id: 0, class: "STATIC", text: "Colour #rrggbb:".to_owned(), x: 302, y: 74, w: 120, h: 20, style: label },
        Ctl { id: ID_BORDER_COLOR, class: "EDIT", text: String::new(), x: 428, y: 72, w: 120, h: 22, style: edit_style | tab },
        Ctl { id: 0, class: "STATIC", text: "Width 0..32:".to_owned(), x: 302, y: 98, w: 120, h: 20, style: label },
        Ctl { id: ID_BORDER_WIDTH, class: "EDIT", text: String::new(), x: 428, y: 96, w: 120, h: 22, style: edit_style | tab },
        Ctl { id: 0, class: "STATIC", text: "Gap 0..64:".to_owned(), x: 302, y: 122, w: 120, h: 20, style: label },
        Ctl { id: ID_BORDER_GAP, class: "EDIT", text: String::new(), x: 428, y: 120, w: 120, h: 22, style: edit_style | tab },
        Ctl { id: 0, class: "STATIC", text: "Radius 0..64:".to_owned(), x: 302, y: 146, w: 120, h: 20, style: label },
        Ctl { id: ID_BORDER_RADIUS, class: "EDIT", text: String::new(), x: 428, y: 144, w: 120, h: 22, style: edit_style | tab },
        Ctl { id: 0, class: "STATIC", text: "Configured colour wins when theme is off or no accent.".to_owned(), x: 302, y: 170, w: 300, h: 30, style: label },
        Ctl { id: 0, class: "BUTTON", text: "Group underlay".to_owned(), x: 622, y: 8, w: 292, h: 124, style: group },
        Ctl { id: ID_UNDERLAY_ENABLED, class: "BUTTON", text: "Enabled".to_owned(), x: 632, y: 28, w: 200, h: 20, style: check | tab },
        Ctl { id: 0, class: "STATIC", text: "Colour #aarrggbb:".to_owned(), x: 632, y: 52, w: 140, h: 20, style: label },
        Ctl { id: ID_UNDERLAY_COLOR, class: "EDIT", text: String::new(), x: 776, y: 50, w: 120, h: 22, style: edit_style | tab },
        Ctl { id: 0, class: "STATIC", text: "Extension -1..32:".to_owned(), x: 632, y: 76, w: 140, h: 20, style: label },
        Ctl { id: ID_UNDERLAY_EXTENSION, class: "EDIT", text: String::new(), x: 776, y: 74, w: 120, h: 22, style: edit_style | tab },
        Ctl { id: 0, class: "STATIC", text: "-1 follows the border width.".to_owned(), x: 632, y: 100, w: 272, h: 22, style: label },
        Ctl { id: 0, class: "BUTTON", text: "Keyboard and mouse takeover".to_owned(), x: 10, y: 212, w: 602, h: 168, style: group },
        Ctl { id: ID_KEYBOARD_TAKEOVER, class: "BUTTON", text: "Keyboard takeover (Win shortcut interception)".to_owned(), x: 20, y: 234, w: 560, h: 20, style: check | tab },
        Ctl { id: ID_MOUSE_SNAP, class: "BUTTON", text: "Mouse Snap prevention (session only, no registry)".to_owned(), x: 20, y: 256, w: 560, h: 20, style: check | tab },
        Ctl { id: ID_SNAP_MEANING, class: "STATIC", text: "Off restores native keyboard chords / captured mouse-Snap value. Keyboard takeover follows individual bindings. Snap Layouts coverage is unverified.".to_owned(), x: 20, y: 278, w: 572, h: 44, style: label },
        Ctl { id: ID_ALLOW_WIN_L, class: "BUTTON", text: "Allow unshifted Win+L (explicit opt-in)".to_owned(), x: 20, y: 324, w: 560, h: 20, style: check | tab },
        Ctl { id: ID_LOCK_WARNING, class: "STATIC", text: "Warning: the hook cannot reliably suppress the OS lock chord.".to_owned(), x: 20, y: 346, w: 572, h: 24, style: label },
        Ctl { id: 0, class: "BUTTON", text: "Presets".to_owned(), x: 622, y: 140, w: 292, h: 240, style: group },
        Ctl { id: ID_PRESET_AUTHENTIC, class: "BUTTON", text: "Authentic".to_owned(), x: 632, y: 162, w: 132, h: 28, style: push | tab },
        Ctl { id: ID_PRESET_COMPATIBLE, class: "BUTTON", text: "Compatible".to_owned(), x: 772, y: 162, w: 132, h: 28, style: push | tab },
        Ctl { id: ID_PRESET_EXPLAIN, class: "STATIC", text: "Authentic restores KDE defaults for every binding (Win+L opt-in preserved). Compatible disables known OS-conflicting chords only (including Win+Ctrl+Left/Right); every other chord is kept, including ones with unknown Windows ownership (letter moves, sticky, Win+Ctrl+Tab/letters/Up/Down, relative-send follows, keyboard resize Alt chords). Kept does not mean conflict-free. Stay rows stay unbound under both presets. No replacement defaults are invented; manual rebind stays available.".to_owned(), x: 632, y: 196, w: 272, h: 174, style: label },
        Ctl { id: 0, class: "BUTTON", text: "New workspaces".to_owned(), x: 10, y: 388, w: 904, h: 56, style: group },
        Ctl { id: ID_DEFAULT_TILED, class: "BUTTON", text: "Tiled".to_owned(), x: 20, y: 410, w: 140, h: 24, style: radio_first | tab },
        Ctl { id: ID_DEFAULT_FLOATING, class: "BUTTON", text: "Floating".to_owned(), x: 170, y: 410, w: 140, h: 24, style: radio | tab },
        Ctl { id: 0, class: "STATIC", text: "New workspaces start tiled or floating. Applies to workspaces created after Apply; existing workspaces keep their session tiling. Only the default is saved.".to_owned(), x: 320, y: 408, w: 584, h: 30, style: label },
        Ctl { id: 0, class: "BUTTON", text: "Same-axis move".to_owned(), x: 10, y: 452, w: 904, h: 56, style: group },
        Ctl { id: ID_SAME_AXIS_GROUP, class: "BUTTON", text: "Group with neighbor".to_owned(), x: 20, y: 474, w: 200, h: 24, style: radio_first | tab },
        Ctl { id: ID_SAME_AXIS_SWAP, class: "BUTTON", text: "Swap with neighbor".to_owned(), x: 230, y: 474, w: 200, h: 24, style: radio | tab },
        Ctl { id: 0, class: "STATIC", text: "Same-axis moves apply to subsequent moves only; existing trees keep their layout until the next move.".to_owned(), x: 440, y: 472, w: 464, h: 30, style: label },
        Ctl { id: 0, class: "BUTTON", text: "Born maximized".to_owned(), x: 10, y: 516, w: 904, h: 56, style: group },
        Ctl { id: ID_BORN_MAX_TILE, class: "BUTTON", text: "Tile on launch".to_owned(), x: 20, y: 538, w: 200, h: 24, style: radio_first | tab },
        Ctl { id: ID_BORN_MAX_OVERLAY, class: "BUTTON", text: "Keep maximized overlay".to_owned(), x: 230, y: 538, w: 200, h: 24, style: radio | tab },
        Ctl { id: 0, class: "STATIC", text: "Born-maximized admission applies to subsequent admissions only; admitted windows keep their state.".to_owned(), x: 440, y: 536, w: 464, h: 30, style: label },
        Ctl { id: 0, class: "BUTTON", text: "Shortcuts (84 rows)".to_owned(), x: 10, y: 580, w: 904, h: 268, style: group },
        Ctl { id: ID_BINDING_LIST, class: "LISTBOX", text: String::new(), x: 20, y: 602, w: 540, h: 230, style: list_style | tab },
        Ctl { id: ID_BINDING_INFO, class: "EDIT", text: String::new(), x: 570, y: 602, w: 324, h: 100, style: info_style },
        Ctl { id: ID_BIND_KEEP, class: "BUTTON", text: "Keep".to_owned(), x: 570, y: 706, w: 100, h: 26, style: push | tab },
        Ctl { id: ID_BIND_DISABLE, class: "BUTTON", text: "Disable".to_owned(), x: 676, y: 706, w: 100, h: 26, style: push | tab },
        Ctl { id: ID_BIND_CHORD, class: "EDIT", text: String::new(), x: 570, y: 738, w: 150, h: 24, style: edit_style | tab },
        Ctl { id: ID_BIND_REBIND, class: "BUTTON", text: "Set rebind".to_owned(), x: 726, y: 736, w: 120, h: 26, style: push | tab },
        Ctl { id: ID_REBIND_NOTE, class: "STATIC", text: "Rebind: Win[+Shift][+Alt][+Ctrl]+Key, keeping this action's modifier arm (focus/select unshifted, move/send shifted, numbered stay Win+Shift or Win+Ctrl+Shift, previous/relative Win+Ctrl, relative send Win+Ctrl+Shift, resize out Win+Alt, resize in Win+Shift+Alt). Win+L can never be a target. Win+G / Win+F11 cannot fully contain the OS Xbox/Game Bar handlers; Win+Ctrl+Left/Right virtual-desktop, Win+Ctrl+Shift arrow, and Win+Alt chord ownership are unverified.".to_owned(), x: 570, y: 766, w: 324, h: 74, style: label },
        Ctl { id: ID_STATUS, class: "STATIC", text: "Status: ready.".to_owned(), x: 20, y: 856, w: 540, h: 60, style: label },
        Ctl { id: ID_APPLY, class: "BUTTON", text: "Apply".to_owned(), x: 580, y: 856, w: 100, h: 30, style: defpush | tab },
        Ctl { id: ID_REVERT, class: "BUTTON", text: "Revert".to_owned(), x: 690, y: 856, w: 100, h: 30, style: push | tab },
        Ctl { id: ID_CLOSE, class: "BUTTON", text: "Close".to_owned(), x: 800, y: 856, w: 100, h: 30, style: push | tab },
    ];
    // Group/label/statics carry id 0 and skip automation lookup. Groupboxes
    // are pinned behind every sibling (see below); their handles ride here.
    let mut owned: Vec<(u32, HWND)> = Vec::new();
    let mut groups: Vec<HWND> = Vec::new();
    for ctl in &controls {
        let handle = if ctl.id == 0 {
            let mut sink: Vec<(u32, HWND)> = Vec::new();
            match create_ctl(hwnd, hinst, ctl, dpi, font, &mut sink) {
                Ok(handle) => {
                    if ctl.class == "BUTTON" {
                        groups.push(handle);
                    }
                    handle
                }
                Err(error) => {
                    abort_create(hwnd, raw);
                    return Err(error);
                }
            }
        } else {
            match create_ctl(hwnd, hinst, ctl, dpi, font, &mut owned) {
                Ok(handle) => handle,
                Err(error) => {
                    abort_create(hwnd, raw);
                    return Err(error);
                }
            }
        };
        if ctl.class == "EDIT"
            && let Some(limit) = edit_limit(ctl.id)
        {
            use windows_sys::Win32::UI::Controls::EM_SETLIMITTEXT;
            use windows_sys::Win32::UI::WindowsAndMessaging::SendMessageW;
            unsafe {
                SendMessageW(handle, EM_SETLIMITTEXT, limit, 0);
            }
        }
    }
    let app = unsafe { &mut *raw };
    app.controls = owned;
    // Pin every groupbox behind all siblings: creation order alone left the
    // first-created groupboxes topmost, and each opaque groupbox then covered
    // every control inside it (blank frames in both screen and `WM_PRINT`
    // captures, machine-proven live). `SWP_NOACTIVATE` keeps focus untouched;
    // geometry flags keep every rect. A failure aborts like any other
    // control-creation failure.
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            HWND_BOTTOM, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SetWindowPos,
        };
        for grouped in &groups {
            let moved = unsafe {
                SetWindowPos(
                    *grouped,
                    HWND_BOTTOM,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                )
            };
            if moved == 0 {
                abort_create(hwnd, raw);
                return Err(err("error: settings groupbox order failed"));
            }
        }
    }
    refresh_all(app);
    // Native tooltips for the R-MOV-03 and R-MAX-06 radios; best-effort (see
    // the helper).
    attach_choice_tips(app, hwnd, hinst);
    if let Some(reason) = invalid {
        status(
            app,
            &format!(
                "Error: settings file is invalid ({reason}). Showing defaults; Apply is disabled, nothing was written."
            ),
        );
    } else {
        status(
            app,
            "Status: editing saved settings. Apply validates and saves; Revert discards edits; Close never applies.",
        );
    }

    unsafe {
        ShowWindow(hwnd, SW_SHOWNORMAL);
    }
    let mut msg: windows_sys::Win32::UI::WindowsAndMessaging::MSG = unsafe { std::mem::zeroed() };
    loop {
        let turn = unsafe { GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) };
        if turn == 0 {
            break;
        }
        if turn == -1 {
            return Err(err("error: settings message loop failed"));
        }
        if msg.message == WM_KEYDOWN && msg.wParam == ESCAPE_VK {
            use windows_sys::Win32::UI::WindowsAndMessaging::DestroyWindow;
            unsafe {
                DestroyWindow(hwnd);
            }
            continue;
        }
        let eaten = unsafe { IsDialogMessageW(hwnd, std::ptr::addr_of!(msg)) };
        if eaten == 0 {
            unsafe {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }
    // WM_DESTROY reclaimed the box and ended the loop; the window is gone
    // and Close never applied. Save counts rode the status line while open.
    Ok("settings closed".to_owned())
}

#[cfg(test)]
mod tests {
    use super::{
        apply_born_maximized_selection, apply_same_axis_selection, born_overlay_selected,
        same_axis_swap_selected,
    };
    use crate::settings::{
        BORN_MAXIMIZED_OVERLAY, BORN_MAXIMIZED_TILE, SAME_AXIS_MOVE_GROUP, SAME_AXIS_MOVE_SWAP,
        Settings,
    };

    #[test]
    fn same_axis_radio_round_trips_both_tokens() {
        let mut draft = Settings::default();
        assert!(!same_axis_swap_selected(&draft));
        apply_same_axis_selection(&mut draft, true);
        assert_eq!(draft.core.same_axis_move, SAME_AXIS_MOVE_SWAP);
        assert!(same_axis_swap_selected(&draft));
        apply_same_axis_selection(&mut draft, false);
        assert_eq!(draft.core.same_axis_move, SAME_AXIS_MOVE_GROUP);
        assert!(!same_axis_swap_selected(&draft));
    }

    #[test]
    fn same_axis_radio_shows_group_for_unknown_hand_built_value() {
        let mut draft = Settings::default();
        draft.core.same_axis_move = "retired-alias".to_owned();
        assert!(!same_axis_swap_selected(&draft));
    }

    #[test]
    fn same_axis_selection_validates_after_apply() {
        for swap in [false, true] {
            let mut draft = Settings::default();
            apply_same_axis_selection(&mut draft, swap);
            crate::settings::validate_settings(&draft).expect("radio stages a valid token");
        }
    }

    #[test]
    fn same_axis_tip_texts_come_from_shared_options() {
        let options = crate::settings::same_axis_move_options();
        let tips = super::same_axis_tip_texts();
        assert_eq!(tips.len(), 2);
        for (buffer, option) in tips.iter().zip(options.iter()) {
            assert_eq!(*buffer, super::wide(option.2));
        }
        // Exact WM references, functional labels kept out of the tips.
        assert_eq!(tips[0], super::wide("COSMIC"));
        assert_eq!(tips[1], super::wide("i3, sway"));
    }

    #[test]
    fn born_maximized_radio_round_trips_both_tokens() {
        let mut draft = Settings::default();
        assert!(!born_overlay_selected(&draft));
        apply_born_maximized_selection(&mut draft, true);
        assert_eq!(draft.core.born_maximized, BORN_MAXIMIZED_OVERLAY);
        assert!(born_overlay_selected(&draft));
        apply_born_maximized_selection(&mut draft, false);
        assert_eq!(draft.core.born_maximized, BORN_MAXIMIZED_TILE);
        assert!(!born_overlay_selected(&draft));
    }

    #[test]
    fn born_maximized_radio_shows_tile_for_unknown_hand_built_value() {
        let mut draft = Settings::default();
        draft.core.born_maximized = "retired-alias".to_owned();
        assert!(!born_overlay_selected(&draft));
    }

    #[test]
    fn born_maximized_selection_validates_after_apply() {
        for overlay in [false, true] {
            let mut draft = Settings::default();
            apply_born_maximized_selection(&mut draft, overlay);
            crate::settings::validate_settings(&draft).expect("radio stages a valid token");
        }
    }

    #[test]
    fn born_maximized_tip_texts_come_from_shared_options() {
        let options = crate::settings::born_maximized_options();
        let tips = super::born_maximized_tip_texts();
        assert_eq!(tips.len(), 2);
        for (buffer, option) in tips.iter().zip(options.iter()) {
            assert_eq!(*buffer, super::wide(option.2));
        }
        // Exact WM references, functional labels kept out of the tips.
        assert_eq!(tips[0], super::wide("bspwm, i3, xmonad, sway, qtile"));
        assert_eq!(tips[1], super::wide("COSMIC, Hyprland"));
    }

    #[test]
    fn choice_tip_buffers_cover_both_radio_pairs() {
        // Four buffers in control order: same-axis pair then born-maximized
        // pair, so each tooltip tool reads its own choice text.
        let tips = super::choice_tip_texts();
        assert_eq!(tips.len(), 4);
        assert_eq!(tips[0], super::wide("COSMIC"));
        assert_eq!(tips[1], super::wide("i3, sway"));
        assert_eq!(tips[0], super::same_axis_tip_texts()[0]);
        assert_eq!(tips[1], super::same_axis_tip_texts()[1]);
        assert_eq!(tips[2], super::born_maximized_tip_texts()[0]);
        assert_eq!(tips[3], super::born_maximized_tip_texts()[1]);
    }

    #[test]
    fn dialog_fits_1380_work_area_at_dpi120() {
        // Lowest control edge is the status line (y856 h60); it must sit
        // inside the 924-high client area, which itself must fit a
        // 1380-high work area at DPI 120 (px(924, 120) == 1155, leaving room
        // for the non-client frame on top).
        assert_eq!(super::px(924, 120), 1155);
        assert!(super::px(856 + 60, 120) <= super::px(924, 120));
        assert!(super::px(924, 120) <= 1380);
    }
}

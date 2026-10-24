//! Catalog-driven tab layout for the native Win32 settings window.
//!
#![forbid(unsafe_code)]
//! Geometry (native Win32 metrics) plus text rendered from the shared
//! portable catalog (`tiler_core::settings_catalog`) and the Windows
//! adapter (`crate::settings`). The native shell (`super`, `ctl`) owns
//! styles, HWND lifetime, and messages; this module owns which tab each
//! control lives on, where its rect sits, what catalog string it shows,
//! and the offline layout/keyboard/badge models the tests assert. No
//! storage semantics, no native operations, no generic field setter
//! triggers an operation here.
//!
//! Tabs (from `ALL_TABS`): 0 Tiling & Workspaces, 1 Appearance,
//! 2 Shortcuts, 3 Desktop Integration. `None` tab means window chrome
//! (tab strip, global badge, status, buttons): always visible.

use tiler_core::settings_catalog::{ALL_SECTIONS, ALL_TABS, FieldType, find_section};

use crate::settings::{NATIVE_PANELS, WINDOWS_SECTIONS, windows_field_def};

use super::{
    ID_ALLOW_WIN_L, ID_ANCHOR_CLOSEST, ID_ANCHOR_LARGEST, ID_ANCHOR_NO_SEARCH, ID_APPLY,
    ID_BIND_CHORD, ID_BIND_DISABLE, ID_BIND_KEEP, ID_BIND_REBIND, ID_BINDING_INFO, ID_BINDING_LIST,
    ID_BORDER_COLOR, ID_BORDER_ENABLED, ID_BORDER_GAP, ID_BORDER_RADIUS, ID_BORDER_THEME,
    ID_BORDER_WIDTH, ID_BORN_MAX_OVERLAY, ID_BORN_MAX_TILE, ID_CLOSE, ID_CURSOR_WARP,
    ID_DEFAULT_FLOATING, ID_DEFAULT_TILED, ID_FIXED_SIZE_BOTH, ID_FIXED_SIZE_EITHER,
    ID_FOCUS_FOLLOWS, ID_GAMING_OPEN_APP, ID_GAMING_OPEN_GAMEBAR, ID_GAMING_OPEN_XBOX,
    ID_GAMING_RECHECK, ID_INNER_GAP, ID_KEYBOARD_TAKEOVER, ID_LOCK_WARNING,
    ID_MIGRATION_REFILL_LAST, ID_MIGRATION_REFILL_MRU, ID_MOUSE_SNAP, ID_NO_SPACE_FLOAT,
    ID_NO_SPACE_OVERLAP, ID_OUTER_GAP, ID_PAGER_HIGHLIGHT_COLOR, ID_PAGER_HIGHLIGHT_THEME,
    ID_PLACE_CURSOR_CLOSEST, ID_PLACE_CURSOR_MRU, ID_PLACE_FOCUSED_MRU, ID_PRESET_AUTHENTIC,
    ID_PRESET_COMPATIBLE, ID_PRESET_EXPLAIN, ID_REBIND_NOTE, ID_REVERT, ID_SAME_AXIS_GROUP,
    ID_SAME_AXIS_SWAP, ID_SNAP_MEANING, ID_STATUS, ID_SWITCH_CURSOR, ID_SWITCH_FOCUSED,
    ID_TASKBAR_PAGER, ID_UNDERLAY_COLOR, ID_UNDERLAY_ENABLED, ID_UNDERLAY_EXTENSION,
    ID_WORKSPACE_MODE_GLOBAL, ID_WORKSPACE_MODE_LOCAL, ID_WORKSPACE_MODE_SHARED,
};
use super::{ID_TAB_BADGE, ID_TABS};

/// Logical client size at 96 DPI. Scaled by `ctl::px` at runtime; the fit
/// tests below prove the scaled outer fits a 1080p work area at 100% and
/// 150% with room for the non-client frame.
pub const CLIENT_W: i32 = 924;
pub const CLIENT_H: i32 = 626;

/// Top of the tab display area (logical). The tab strip occupies roughly
/// the first 28px of the tab control; content starts at 40 with margin.
pub const TAB_DISPLAY_TOP: i32 = 40;
/// Bottom of the tab display area (logical): the tab control rect
/// `(10, 8, 904, 520)` ends at 528. Every page control, groupbox, and
/// label must end at or above this line; nothing relies on slack.
pub const TAB_DISPLAY_BOTTOM: i32 = 528;
/// Footer (global badge, status, buttons) lives below the tab control.
pub const FOOTER_TOP: i32 = 532;

/// Horizontal non-client allowance (borders) added to the scaled client
/// when proving 1080p fit. Reason: `AdjustWindowRect` side borders plus
/// a small margin at either DPI.
pub const FRAME_W_ALLOW: i32 = 32;
/// Vertical non-client allowance (caption plus borders) added to the
/// scaled client when proving 1080p fit. Reason: dialog caption and
/// borders at either DPI with a small margin.
pub const FRAME_H_ALLOW: i32 = 80;

/// Initial window origin (logical px) at creation. Reason: the size fit
/// above proves dimensions only; `origin_fits_1080p` proves this origin
/// plus the worst-case outer still sits inside a 1920x1040 work area
/// (1080p minus a 40px taskbar) at 100% and 150%.
pub const ORIGIN_X: i32 = 40;
pub const ORIGIN_Y: i32 = 20;

/// Average advance of one 9pt Segoe UI character at 96 DPI (logical px).
/// Reason: mixed-case message-font text averages 6-7px per character;
/// 7px keeps single-line fit checks honest without per-glyph measuring.
pub const AVG_CHAR_PX: i32 = 7;
/// One 9pt message-font line including spacing (logical px). Reason:
/// 9pt is 12px with inter-line spacing; wrapped STATICs budget 16px
/// per line so two lines never claim a 30px rect.
pub const LINE_PX: i32 = 16;
/// Longest badge text the 540px footer static holds: two wrapped lines
/// at the message font. Reason: 540px fits ~77 characters per line;
/// conflict plus warning counts stay fully visible instead of clipping.
pub const BADGE_MAX_CHARS: usize = 154;

/// Number of tabs; always the catalog length, never a second count.
pub const TAB_COUNT: usize = ALL_TABS.len();

/// Tab index for Tiling & Workspaces.
pub const TAB_TILING: usize = 0;
/// Tab index for Appearance.
pub const TAB_APPEARANCE: usize = 1;
/// Tab index for Shortcuts.
pub const TAB_SHORTCUTS: usize = 2;
/// Tab index for Desktop Integration.
pub const TAB_DESKTOP: usize = 3;

/// Tab titles in order, straight from the portable catalog.
#[must_use]
pub fn tab_titles() -> [&'static str; TAB_COUNT] {
    [
        ALL_TABS[TAB_TILING].label,
        ALL_TABS[TAB_APPEARANCE].label,
        ALL_TABS[TAB_SHORTCUTS].label,
        ALL_TABS[TAB_DESKTOP].label,
    ]
}

/// Cycle tabs for Ctrl+Tab (`forward`) / Ctrl+Shift+Tab (backward). Wraps.
#[must_use]
pub const fn next_tab(current: usize, forward: bool) -> usize {
    if forward {
        (current + 1) % TAB_COUNT
    } else {
        (current + TAB_COUNT - 1) % TAB_COUNT
    }
}

/// Tab index for one section id: portable `ALL_SECTIONS` first, then the
/// Windows adapter sections. `None` for unknown sections (never rendered).
#[must_use]
pub fn section_tab_index(section: &str) -> Option<usize> {
    let tab = ALL_SECTIONS
        .iter()
        .find(|known| known.id == section)
        .map(|known| known.tab)
        .or_else(|| {
            WINDOWS_SECTIONS
                .iter()
                .find(|known| known.id == section)
                .map(|known| known.tab)
        })?;
    ALL_TABS.iter().position(|known| known.id == tab)
}

/// Tab index for one native panel id, through its section.
#[must_use]
pub fn panel_tab_index(panel: &str) -> Option<usize> {
    let slot = NATIVE_PANELS.iter().find(|known| known.id == panel)?;
    section_tab_index(slot.section)
}

/// Which tab owns one automated control id, derived from the field
/// section (scalars) or the panel slot section (actions) through the
/// catalog tab tables. `None` means window chrome (always visible): tab
/// strip, global badge, status, Apply/Revert/Close.
#[must_use]
pub fn control_tab(id: u32) -> Option<usize> {
    if matches!(
        id,
        ID_TABS | ID_TAB_BADGE | ID_STATUS | ID_APPLY | ID_REVERT | ID_CLOSE
    ) {
        return None;
    }
    if let Some(field) = control_field(id) {
        let def = windows_field_def(field).expect("mapped field has a descriptor");
        return section_tab_index(def.section);
    }
    if let Some(panel) = control_panel(id) {
        return panel_tab_index(panel);
    }
    None
}

/// Stable field id behind one scalar control id, for the exact-once test.
/// Radio pairs/triples share one field; buttons, list, info, and notes
/// carry none.
#[must_use]
pub const fn control_field(id: u32) -> Option<&'static str> {
    match id {
        ID_INNER_GAP => Some("gaps.inner"),
        ID_OUTER_GAP => Some("gaps.outer"),
        ID_DEFAULT_TILED | ID_DEFAULT_FLOATING => Some("workspace.default-tiled"),
        ID_WORKSPACE_MODE_LOCAL | ID_WORKSPACE_MODE_GLOBAL | ID_WORKSPACE_MODE_SHARED => {
            Some("workspace.mode")
        }
        ID_SAME_AXIS_GROUP | ID_SAME_AXIS_SWAP => Some("move.same-axis"),
        ID_BORN_MAX_TILE | ID_BORN_MAX_OVERLAY => Some("admission.born-maximized"),
        ID_FIXED_SIZE_BOTH | ID_FIXED_SIZE_EITHER => Some("admission.fixed-size"),
        ID_ANCHOR_LARGEST | ID_ANCHOR_CLOSEST | ID_ANCHOR_NO_SEARCH => {
            Some("admission.anchor-fallback")
        }
        ID_NO_SPACE_FLOAT | ID_NO_SPACE_OVERLAP => Some("admission.no-space"),
        ID_MIGRATION_REFILL_LAST | ID_MIGRATION_REFILL_MRU => Some("migration.refill"),
        ID_SWITCH_CURSOR | ID_SWITCH_FOCUSED => Some("output.switch-target"),
        ID_FOCUS_FOLLOWS => Some("output.focus-follows-cursor"),
        ID_CURSOR_WARP => Some("output.cursor-warp"),
        ID_PLACE_CURSOR_MRU | ID_PLACE_CURSOR_CLOSEST | ID_PLACE_FOCUSED_MRU => {
            Some("output.fresh-placement")
        }
        ID_BORDER_ENABLED => Some("border.enabled"),
        ID_BORDER_WIDTH => Some("border.width"),
        ID_BORDER_GAP => Some("border.gap"),
        ID_BORDER_RADIUS => Some("border.radius"),
        ID_BORDER_COLOR => Some("border.color"),
        ID_BORDER_THEME => Some("border.use-theme"),
        ID_UNDERLAY_ENABLED => Some("underlay.enabled"),
        ID_UNDERLAY_COLOR => Some("underlay.color"),
        ID_UNDERLAY_EXTENSION => Some("underlay.extension"),
        ID_KEYBOARD_TAKEOVER => Some("keyboard.takeover"),
        ID_ALLOW_WIN_L => Some("keyboard.allow-win-l"),
        ID_MOUSE_SNAP => Some("mouse.snap-prevention"),
        ID_TASKBAR_PAGER => Some("desktop.taskbar-pager"),
        ID_PAGER_HIGHLIGHT_THEME => Some("desktop.pager-highlight-use-theme"),
        ID_PAGER_HIGHLIGHT_COLOR => Some("desktop.pager-highlight-color"),
        _ => None,
    }
}

/// Native panel slot behind one action control id, consumed in production
/// by [`control_tab`] (action tabs derive from their panel slot) and by
/// group titles below. Scalar editors belong to none: they never trigger
/// operations.
#[must_use]
pub const fn control_panel(id: u32) -> Option<&'static str> {
    match id {
        ID_KEYBOARD_TAKEOVER => Some("native.takeover-policy"),
        ID_ALLOW_WIN_L | ID_LOCK_WARNING => Some("native.win-l-remediation"),
        ID_PRESET_AUTHENTIC | ID_PRESET_COMPATIBLE | ID_PRESET_EXPLAIN => {
            Some("native.shortcut-presets")
        }
        ID_BINDING_LIST | ID_BINDING_INFO | ID_BIND_KEEP | ID_BIND_DISABLE | ID_BIND_CHORD
        | ID_BIND_REBIND | ID_REBIND_NOTE => Some("native.binding-editor"),
        ID_GAMING_OPEN_XBOX | ID_GAMING_OPEN_GAMEBAR | ID_GAMING_OPEN_APP | ID_GAMING_RECHECK => {
            Some("native.gaming-operations")
        }
        ID_MOUSE_SNAP | ID_SNAP_MEANING => Some("native.snap-policy"),
        _ => None,
    }
}

/// Logical rect for one automated control: `(x, y, w, h)` at 96 DPI.
/// Page controls sit inside the tab display area; chrome sits below it.
#[must_use]
pub const fn control_rect(id: u32) -> Option<(i32, i32, i32, i32)> {
    match id {
        ID_TABS => Some((10, 8, 904, 520)),
        ID_TAB_BADGE => Some((20, 532, 540, 32)),
        ID_STATUS => Some((20, 566, 540, 48)),
        ID_APPLY => Some((580, 560, 100, 30)),
        ID_REVERT => Some((690, 560, 100, 30)),
        ID_CLOSE => Some((800, 560, 100, 30)),
        // Tiling & Workspaces: gaps row, workspaces block, five radio rows.
        ID_INNER_GAP => Some((84, 66, 56, 22)),
        ID_OUTER_GAP => Some((206, 66, 56, 22)),
        ID_DEFAULT_TILED => Some((310, 66, 140, 24)),
        ID_DEFAULT_FLOATING => Some((460, 66, 140, 24)),
        ID_WORKSPACE_MODE_LOCAL => Some((310, 100, 170, 24)),
        ID_WORKSPACE_MODE_GLOBAL => Some((490, 100, 160, 24)),
        ID_WORKSPACE_MODE_SHARED => Some((660, 100, 220, 24)),
        ID_SAME_AXIS_GROUP => Some((28, 202, 200, 24)),
        ID_SAME_AXIS_SWAP => Some((238, 202, 200, 24)),
        ID_BORN_MAX_TILE => Some((28, 250, 200, 24)),
        ID_BORN_MAX_OVERLAY => Some((238, 250, 200, 24)),
        ID_FIXED_SIZE_BOTH => Some((28, 296, 200, 24)),
        ID_FIXED_SIZE_EITHER => Some((238, 296, 200, 24)),
        ID_ANCHOR_LARGEST => Some((28, 342, 210, 24)),
        ID_ANCHOR_CLOSEST => Some((248, 342, 210, 24)),
        ID_ANCHOR_NO_SEARCH => Some((468, 342, 150, 24)),
        ID_NO_SPACE_FLOAT => Some((28, 400, 200, 24)),
        ID_NO_SPACE_OVERLAP => Some((238, 400, 200, 24)),
        // Migration refill rides the workspaces group third row: the
        // bottom slot now belongs to the output-targeting group.
        ID_MIGRATION_REFILL_LAST => Some((600, 134, 200, 24)),
        ID_MIGRATION_REFILL_MRU => Some((600, 158, 220, 24)),
        // Stage 2c output targeting: switch-target pair plus
        // fresh-placement triple, one groupbox, two rows. Stage 4b adds
        // the hover-focus plus cross-output warp checkboxes on the
        // switch row.
        ID_SWITCH_CURSOR => Some((28, 466, 150, 24)),
        ID_SWITCH_FOCUSED => Some((188, 466, 170, 24)),
        ID_FOCUS_FOLLOWS => Some((368, 466, 220, 24)),
        ID_CURSOR_WARP => Some((598, 466, 250, 24)),
        ID_PLACE_CURSOR_MRU => Some((28, 490, 258, 24)),
        ID_PLACE_CURSOR_CLOSEST => Some((296, 490, 286, 24)),
        ID_PLACE_FOCUSED_MRU => Some((592, 490, 270, 24)),
        // Appearance: border block (left) plus underlay block (right).
        ID_BORDER_ENABLED => Some((28, 66, 200, 20)),
        ID_BORDER_THEME => Some((28, 88, 290, 20)),
        ID_BORDER_COLOR => Some((168, 110, 120, 22)),
        ID_BORDER_WIDTH => Some((168, 134, 120, 22)),
        ID_BORDER_GAP => Some((168, 158, 120, 22)),
        ID_BORDER_RADIUS => Some((168, 182, 120, 22)),
        ID_UNDERLAY_ENABLED => Some((598, 66, 200, 20)),
        ID_UNDERLAY_COLOR => Some((752, 88, 110, 22)),
        ID_UNDERLAY_EXTENSION => Some((752, 112, 110, 22)),
        // Shortcuts: policy plus remediation row, presets row, editor, ops.
        ID_KEYBOARD_TAKEOVER => Some((28, 66, 520, 20)),
        ID_ALLOW_WIN_L => Some((28, 150, 520, 20)),
        ID_LOCK_WARNING => Some((28, 172, 520, 24)),
        ID_PRESET_AUTHENTIC => Some((598, 66, 140, 28)),
        ID_PRESET_COMPATIBLE => Some((746, 66, 140, 28)),
        ID_PRESET_EXPLAIN => Some((598, 100, 296, 100)),
        ID_BINDING_LIST => Some((28, 238, 520, 150)),
        ID_BINDING_INFO => Some((558, 238, 326, 56)),
        ID_BIND_KEEP => Some((558, 300, 100, 26)),
        ID_BIND_DISABLE => Some((664, 300, 100, 26)),
        ID_BIND_CHORD => Some((558, 348, 150, 24)),
        ID_BIND_REBIND => Some((714, 346, 120, 26)),
        ID_REBIND_NOTE => Some((558, 378, 326, 34)),
        ID_GAMING_OPEN_XBOX => Some((28, 448, 205, 24)),
        ID_GAMING_OPEN_GAMEBAR => Some((243, 448, 205, 24)),
        ID_GAMING_OPEN_APP => Some((458, 448, 215, 24)),
        ID_GAMING_RECHECK => Some((683, 448, 201, 24)),
        // Desktop Integration: snap block plus pager block.
        ID_MOUSE_SNAP => Some((28, 66, 560, 20)),
        ID_SNAP_MEANING => Some((28, 90, 856, 44)),
        ID_TASKBAR_PAGER => Some((28, 234, 560, 20)),
        ID_PAGER_HIGHLIGHT_THEME => Some((28, 258, 560, 20)),
        ID_PAGER_HIGHLIGHT_COLOR => Some((298, 282, 120, 22)),
        _ => None,
    }
}

/// Win32 class for one automated control. Long native notes are readonly
/// multiline edits with their own scrollbar (scrolling, not a new
/// surface) so 500+ character guidance stays keyboard reachable instead
/// of clipping inside a static.
#[must_use]
pub const fn control_class(id: u32) -> Option<&'static str> {
    match id {
        ID_TABS => Some("SysTabControl32"),
        ID_TAB_BADGE | ID_STATUS | ID_SNAP_MEANING | ID_LOCK_WARNING => Some("STATIC"),
        ID_APPLY
        | ID_REVERT
        | ID_CLOSE
        | ID_PRESET_AUTHENTIC
        | ID_PRESET_COMPATIBLE
        | ID_BIND_KEEP
        | ID_BIND_DISABLE
        | ID_BIND_REBIND
        | ID_GAMING_OPEN_XBOX
        | ID_GAMING_OPEN_GAMEBAR
        | ID_GAMING_OPEN_APP
        | ID_GAMING_RECHECK => Some("BUTTON"),
        ID_INNER_GAP
        | ID_OUTER_GAP
        | ID_BORDER_COLOR
        | ID_BORDER_WIDTH
        | ID_BORDER_GAP
        | ID_BORDER_RADIUS
        | ID_PAGER_HIGHLIGHT_COLOR
        | ID_UNDERLAY_COLOR
        | ID_UNDERLAY_EXTENSION
        | ID_BIND_CHORD => Some("EDIT"),
        ID_PRESET_EXPLAIN | ID_REBIND_NOTE | ID_BINDING_INFO => Some("EDIT"),
        ID_BINDING_LIST => Some("LISTBOX"),
        ID_BORDER_ENABLED
        | ID_BORDER_THEME
        | ID_UNDERLAY_ENABLED
        | ID_KEYBOARD_TAKEOVER
        | ID_ALLOW_WIN_L
        | ID_MOUSE_SNAP
        | ID_TASKBAR_PAGER
        | ID_PAGER_HIGHLIGHT_THEME
        | ID_FOCUS_FOLLOWS
        | ID_CURSOR_WARP => Some("BUTTON"),
        ID_DEFAULT_TILED
        | ID_DEFAULT_FLOATING
        | ID_SAME_AXIS_GROUP
        | ID_SAME_AXIS_SWAP
        | ID_BORN_MAX_TILE
        | ID_BORN_MAX_OVERLAY
        | ID_FIXED_SIZE_BOTH
        | ID_FIXED_SIZE_EITHER
        | ID_ANCHOR_LARGEST
        | ID_ANCHOR_CLOSEST
        | ID_ANCHOR_NO_SEARCH
        | ID_NO_SPACE_FLOAT
        | ID_NO_SPACE_OVERLAP
        | ID_MIGRATION_REFILL_LAST
        | ID_MIGRATION_REFILL_MRU
        | ID_SWITCH_CURSOR
        | ID_SWITCH_FOCUSED
        | ID_PLACE_CURSOR_MRU
        | ID_PLACE_CURSOR_CLOSEST
        | ID_PLACE_FOCUSED_MRU
        | ID_WORKSPACE_MODE_LOCAL
        | ID_WORKSPACE_MODE_GLOBAL
        | ID_WORKSPACE_MODE_SHARED => Some("BUTTON"),
        _ => None,
    }
}

/// True for controls whose content scrolls instead of clipping: the
/// listbox plus readonly multiline edits. Text-fit checks exempt these.
#[must_use]
pub const fn control_scrollable(id: u32) -> bool {
    matches!(
        id,
        ID_BINDING_LIST | ID_BINDING_INFO | ID_PRESET_EXPLAIN | ID_REBIND_NOTE
    )
}

/// One groupbox key: a native panel slot, a portable/Windows section, or
/// a section disambiguated by one of its field labels. Titles and tabs
/// both derive from this key in production, never from parallel lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupKey {
    Panel(&'static str),
    Section(&'static str),
    SectionField(&'static str, &'static str),
}

/// Groupboxes in creation order: `(key, x, y, w, h)` at 96 DPI. Id 0,
/// never automated, pinned behind siblings by the shell. The four
/// admission groups share one section id, so each carries its field key:
/// titles read `Admission: <field label>`, never bare duplicates.
pub const GROUPS: &[(GroupKey, i32, i32, i32, i32)] = &[
    (GroupKey::Section("tiling.gaps"), 18, 44, 272, 56),
    (GroupKey::Section("tiling.workspaces"), 300, 44, 604, 140),
    (GroupKey::Section("tiling.move"), 18, 190, 886, 44),
    (
        GroupKey::SectionField("tiling.admission", "admission.born-maximized"),
        18,
        240,
        886,
        40,
    ),
    (
        GroupKey::SectionField("tiling.admission", "admission.fixed-size"),
        18,
        286,
        886,
        40,
    ),
    (
        GroupKey::SectionField("tiling.admission", "admission.anchor-fallback"),
        18,
        332,
        886,
        52,
    ),
    (
        GroupKey::SectionField("tiling.admission", "admission.no-space"),
        18,
        390,
        886,
        52,
    ),
    // Stage 2c output targeting owns the bottom slot: the
    // migration-refill radios ride the workspaces group third row above.
    (
        GroupKey::Section("tiling.output-targeting"),
        18,
        448,
        886,
        72,
    ),
    (GroupKey::Section("appearance.border"), 18, 44, 560, 240),
    (GroupKey::Section("appearance.underlay"), 588, 44, 316, 180),
    (GroupKey::Panel("native.takeover-policy"), 18, 44, 560, 76),
    (
        GroupKey::Panel("native.win-l-remediation"),
        18,
        128,
        560,
        80,
    ),
    (
        GroupKey::Panel("native.shortcut-presets"),
        588,
        44,
        316,
        164,
    ),
    (GroupKey::Panel("native.binding-editor"), 18, 216, 886, 204),
    (
        GroupKey::Panel("native.gaming-operations"),
        18,
        428,
        886,
        56,
    ),
    (GroupKey::Panel("native.snap-policy"), 18, 44, 886, 160),
    (GroupKey::Section("desktop.pager"), 18, 212, 886, 150),
];

/// Tab index for one group key, derived through the same section tables
/// as [`control_tab`].
#[must_use]
pub fn group_tab(key: GroupKey) -> Option<usize> {
    match key {
        GroupKey::Panel(panel) => panel_tab_index(panel),
        GroupKey::Section(section) | GroupKey::SectionField(section, _) => {
            section_tab_index(section)
        }
    }
}

/// Groupbox title for one key: native panel labels for panels, catalog
/// section labels for sections, and `section: field` for the four
/// admission groups sharing one section id.
#[must_use]
pub fn group_title(key: GroupKey) -> String {
    match key {
        GroupKey::Panel(panel) => NATIVE_PANELS
            .iter()
            .find(|known| known.id == panel)
            .map(|known| known.label.to_owned())
            .unwrap_or_else(|| panel.to_owned()),
        GroupKey::Section(section) => section_label(section),
        GroupKey::SectionField(section, field) => {
            let label = windows_field_def(field)
                .map(|def| def.label)
                .unwrap_or(field);
            format!("{}: {label}", section_label(section))
        }
    }
}

fn section_label(section: &str) -> String {
    if let Some(known) = find_section(section) {
        return known.label.to_owned();
    }
    WINDOWS_SECTIONS
        .iter()
        .find(|known| known.id == section)
        .map(|known| known.label.to_owned())
        .unwrap_or_else(|| section.to_owned())
}

/// Label-only statics (id 0) with their owning tab: `(tab, x, y, w, h)` at
/// 96 DPI. Texts derive from the catalog at runtime ([`label_text`]);
/// rects are native metrics. Long workspace notes (indices 2, 3) are
/// readonly multiline edits like the other long notes.
pub const LABEL_RECTS: &[(usize, i32, i32, i32, i32)] = &[
    // Tiling: gap labels, workspace notes, six radio-row notes.
    (TAB_TILING, 28, 68, 52, 20),
    (TAB_TILING, 150, 68, 52, 20),
    (TAB_TILING, 610, 64, 284, 60),
    // Scope note narrowed to the left column: the migration-refill radios
    // ride the right column third row of the same group.
    (TAB_TILING, 310, 132, 280, 48),
    (TAB_TILING, 448, 200, 448, 32),
    (TAB_TILING, 448, 246, 448, 32),
    (TAB_TILING, 448, 292, 448, 32),
    // Stage 2c: the migration-refill radios ride the workspaces group
    // third row, so this slot carries no timing note (timing truth stays
    // in the catalog and the adoption log lines).
    (TAB_TILING, 628, 342, 266, 32),
    (TAB_TILING, 448, 394, 448, 48),
    // Appearance: border/underlay field labels plus two notes.
    (TAB_APPEARANCE, 28, 112, 136, 20),
    (TAB_APPEARANCE, 28, 136, 136, 20),
    (TAB_APPEARANCE, 28, 160, 136, 20),
    (TAB_APPEARANCE, 28, 184, 136, 20),
    (TAB_APPEARANCE, 28, 208, 540, 60),
    (TAB_APPEARANCE, 598, 90, 150, 20),
    (TAB_APPEARANCE, 598, 114, 150, 20),
    (TAB_APPEARANCE, 598, 138, 296, 60),
    // Desktop: snap timing note, pager help, plus pager highlight buddy.
    (TAB_DESKTOP, 28, 138, 856, 50),
    (TAB_DESKTOP, 28, 308, 856, 30),
    // Shortcuts: rebind-chord buddy label, ahead of its editor.
    (TAB_SHORTCUTS, 558, 330, 150, 16),
    (TAB_DESKTOP, 28, 282, 260, 24),
];

/// Label index of the rebind-chord buddy label (no catalog field; the
/// editor carries the pending chord, so the label is native wording).
pub const LABEL_CHORD: usize = 19;

/// Index of the pager highlight colour buddy label.
pub const LABEL_PAGER_HIGHLIGHT_COLOR: usize = 20;

/// Catalog field behind a label-static index above.
const LABEL_FIELDS: &[&str] = &[
    "gaps.inner",
    "gaps.outer",
    "workspace.default-tiled",
    "workspace.mode",
    "move.same-axis",
    "admission.born-maximized",
    "admission.fixed-size",
    "admission.anchor-fallback",
    "admission.no-space",
    "border.color",
    "border.width",
    "border.gap",
    "border.radius",
    "border.color",
    "underlay.color",
    "underlay.extension",
    "underlay.enabled",
    "mouse.snap-prevention",
    "desktop.taskbar-pager",
    "",
    "desktop.pager-highlight-color",
];

/// Win32 class for one label index: the two long workspace notes scroll
/// like the other long notes; every other label is a plain static.
#[must_use]
pub const fn label_class(index: usize) -> &'static str {
    match index {
        2 | 3 => "EDIT",
        _ => "STATIC",
    }
}

/// Editor id behind one buddy-label index. The shell creates the label
/// immediately before its editor so assistive tech associates the name.
pub const BUDDY_LABELS: &[(u32, usize)] = &[
    (ID_INNER_GAP, 0),
    (ID_OUTER_GAP, 1),
    (ID_BORDER_COLOR, 9),
    (ID_BORDER_WIDTH, 10),
    (ID_BORDER_GAP, 11),
    (ID_BORDER_RADIUS, 12),
    (ID_UNDERLAY_COLOR, 14),
    (ID_UNDERLAY_EXTENSION, 15),
    (ID_PAGER_HIGHLIGHT_COLOR, LABEL_PAGER_HIGHLIGHT_COLOR),
    (ID_BIND_CHORD, LABEL_CHORD),
];

/// Text for one automated control, rendered from catalog metadata.
/// Checkbox/radio labels are field/option labels; edits and the listbox
/// carry functional window text (never rendered as content) so screen
/// readers name them; notes keep their existing native wording (timing
/// truth rides the neighboring catalog note statics, never replaced).
#[must_use]
pub fn control_text(id: u32) -> String {
    if id == ID_TABS {
        return "Settings tabs".to_owned();
    }
    if id == ID_BINDING_LIST {
        return "Shortcut bindings".to_owned();
    }
    if id == ID_DEFAULT_TILED {
        return "Tiled".to_owned();
    }
    if id == ID_DEFAULT_FLOATING {
        return "Floating".to_owned();
    }
    if let Some(field_id) = control_field(id) {
        let def = windows_field_def(field_id).expect("mapped field has a descriptor");
        match def.field_type {
            FieldType::Bool => return def.label.to_owned(),
            FieldType::Enum => {
                let index = match id {
                    ID_SAME_AXIS_GROUP
                    | ID_BORN_MAX_TILE
                    | ID_FIXED_SIZE_BOTH
                    | ID_MIGRATION_REFILL_LAST
                    | ID_ANCHOR_LARGEST
                    | ID_NO_SPACE_FLOAT
                    | ID_SWITCH_CURSOR
                    | ID_PLACE_CURSOR_MRU
                    | ID_WORKSPACE_MODE_LOCAL => 0,
                    ID_SAME_AXIS_SWAP
                    | ID_BORN_MAX_OVERLAY
                    | ID_FIXED_SIZE_EITHER
                    | ID_MIGRATION_REFILL_MRU
                    | ID_ANCHOR_CLOSEST
                    | ID_NO_SPACE_OVERLAP
                    | ID_SWITCH_FOCUSED
                    | ID_PLACE_CURSOR_CLOSEST
                    | ID_WORKSPACE_MODE_GLOBAL => 1,
                    ID_ANCHOR_NO_SEARCH | ID_PLACE_FOCUSED_MRU | ID_WORKSPACE_MODE_SHARED => 2,
                    _ => return String::new(),
                };
                return def
                    .options
                    .get(index)
                    .map(|option| option.label.to_owned())
                    .unwrap_or_default();
            }
            _ => return String::new(),
        }
    }
    match id {
        ID_TAB_BADGE | ID_STATUS => String::new(),
        ID_APPLY => "Apply".to_owned(),
        ID_REVERT => "Revert".to_owned(),
        ID_CLOSE => "Close".to_owned(),
        ID_PRESET_AUTHENTIC => "Authentic".to_owned(),
        ID_PRESET_COMPATIBLE => "Compatible".to_owned(),
        ID_BIND_KEEP => "Keep".to_owned(),
        ID_BIND_DISABLE => "Disable".to_owned(),
        ID_BIND_REBIND => "Set rebind".to_owned(),
        ID_GAMING_OPEN_XBOX => "Open Xbox mode settings".to_owned(),
        ID_GAMING_OPEN_GAMEBAR => "Open Game Bar settings".to_owned(),
        ID_GAMING_OPEN_APP => "Open Game Bar app settings".to_owned(),
        ID_GAMING_RECHECK => "Recheck gaming status".to_owned(),
        // Native-panel notes keep existing wording and scroll instead of
        // clipping; catalog timing truth rides the neighboring statics.
        ID_SNAP_MEANING => "Off restores native keyboard chords / captured mouse-Snap value. Keyboard takeover follows individual bindings. Snap Layouts coverage is unverified.".to_owned(),
        ID_LOCK_WARNING => {
            "Warning: the hook cannot reliably suppress the OS lock chord.".to_owned()
        }
        ID_PRESET_EXPLAIN => "Authentic restores KDE defaults for every binding (Win+L opt-in preserved). Compatible disables known OS-conflicting chords only (including Win+Ctrl+Left/Right) and rebinds float to Win+Ctrl+G and fullscreen to Win+Ctrl+F11; every other chord is kept, including ones with unknown Windows ownership (letter moves, sticky, Win+Ctrl+Tab/letters/Up/Down, relative-send follows, keyboard resize Alt chords). Kept does not mean conflict-free. Stay rows and workspace-migration rows stay unbound under both presets. No other replacement defaults are invented; manual rebind stays available.".to_owned(),
        ID_REBIND_NOTE => "Rebind: Win[+Shift][+Alt][+Ctrl]+Key, keeping this action's modifier arm (focus/select unshifted, float/fullscreen Win or Win+Ctrl, move/send shifted, numbered stay Win+Shift or Win+Ctrl+Shift, previous/relative Win+Ctrl, relative send Win+Ctrl+Shift, output follow Win+Ctrl+Alt, output stay Win+Ctrl+Alt or Win+Ctrl+Alt+Shift, workspace migration Win+Ctrl+Alt+Shift, resize out Win+Alt, resize in Win+Shift+Alt). Win+L can never be a target. Win+G / Win+F11 cannot fully contain the OS Xbox/Game Bar handlers; Win+Ctrl+Left/Right virtual-desktop, Win+Ctrl+Shift arrow, Win+Ctrl+Alt, and Win+Alt chord ownership are unverified.".to_owned(),
        _ => String::new(),
    }
}

/// Label-static text for one `LABEL_RECTS` index: field labels for
/// editors, help plus timing notes for section notes. All catalog-sourced
/// except the native chord buddy label.
#[must_use]
pub fn label_text(index: usize) -> String {
    if index == LABEL_CHORD {
        return "New chord:".to_owned();
    }
    let field_id = LABEL_FIELDS.get(index).copied().unwrap_or("");
    let def = windows_field_def(field_id).expect("label field has a descriptor");
    match index {
        0 | 1 => format!("{}:", short_field_noun(field_id)),
        2 => format!("{} {}", def.help, def.timing_help),
        3 => format!("Scope (restart required): {}", def.timing_help),
        4..=8 => def.timing_help.to_owned(),
        9..=12 => format!("{}:", def.label),
        13 => "Configured colour wins when theme is off or no accent.".to_owned(),
        14 | 15 | LABEL_PAGER_HIGHLIGHT_COLOR => format!("{}:", def.label),
        16 => "-1 follows the border width.".to_owned(),
        17 => def.timing_help.to_owned(),
        18 => def.help.to_owned(),
        _ => def.label.to_owned(),
    }
}

fn short_field_noun(field_id: &str) -> &'static str {
    match field_id {
        "gaps.inner" => "Inner",
        "gaps.outer" => "Outer",
        _ => "Value",
    }
}

/// Single-line fit at the message font: every character budgets
/// [`AVG_CHAR_PX`] logical pixels.
#[must_use]
pub fn single_line_fits(text: &str, w: i32) -> bool {
    text.chars().count() as i32 * AVG_CHAR_PX <= w
}

/// Wrapped STATIC lines for one text at width `w`: word wrap is
/// approximated by character count, which overestimates CJK-narrow text
/// and stays conservative for the message font.
#[must_use]
pub fn wrapped_lines(text: &str, w: i32) -> i32 {
    let per_line = (w / AVG_CHAR_PX).max(1);
    let chars = text.chars().count() as i32;
    (chars + per_line - 1) / per_line
}

/// Wrapped STATIC fit: every line budgets [`LINE_PX`] logical pixels.
#[must_use]
pub fn static_fits(text: &str, w: i32, h: i32) -> bool {
    wrapped_lines(text, w) * LINE_PX <= h
}

/// True while the settings window still proves every HWND matches the
/// saved file: no `dirty` flag, no `work != base` drift, no pending
/// chord. Anything else is requested-but-unsaved, never "saved".
#[must_use]
pub const fn badge_proven_saved(dirty: bool, work_unsaved: bool, pending_chord: bool) -> bool {
    !dirty && !work_unsaved && !pending_chord
}

/// Global badge outside the tab pages: invalid file, unsaved draft
/// (dirty HWND edits, `work != base` staging like presets/rebinds, or a
/// still-typed pending chord), and aggregated Shortcuts conflicts plus
/// warnings. Saved, requested, and confirmed stay distinct: the badge
/// reports file state only and never claims a live native effect.
#[must_use]
pub fn badge_text(invalid: bool, unsaved: bool, conflicts: usize, warnings: usize) -> String {
    if invalid {
        return "Error: settings file is invalid; Apply disabled.".to_owned();
    }
    let saved = if unsaved {
        "Unsaved edits; Apply saves (never confirms effect)."
    } else {
        "All tabs match saved settings."
    };
    let scope = match (conflicts, warnings) {
        (0, 0) => None,
        (conflicts, 0) => Some(format!(
            "{} {}",
            conflicts,
            if conflicts == 1 {
                "conflict"
            } else {
                "conflicts"
            }
        )),
        (0, warnings) => Some(format!(
            "{} {}",
            warnings,
            if warnings == 1 { "warning" } else { "warnings" }
        )),
        (conflicts, warnings) => Some(format!(
            "{conflicts} {}, {warnings} {}",
            if conflicts == 1 {
                "conflict"
            } else {
                "conflicts"
            },
            if warnings == 1 { "warning" } else { "warnings" }
        )),
    };
    match scope {
        None => saved.to_owned(),
        Some(scope) => format!("{saved} Shortcuts: {scope}."),
    }
}

/// One creation step for the native shell: the tab strip, one groupbox,
/// one label static, or one automated control. The shell creates steps
/// in [`creation_sequence`] order so every buddy label immediately
/// precedes its editor (accessible-name association) and tab-stop order
/// follows the visual top-to-bottom flow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CreateStep {
    TabStrip,
    Group(usize),
    Label(usize),
    Control(u32),
}

/// Rect center of one automated control, for group membership checks.
fn control_center(id: u32) -> Option<(i32, i32)> {
    control_rect(id).map(|(x, y, w, h)| (x + w / 2, y + h / 2))
}

/// Rect center of one label static.
fn label_center(index: usize) -> Option<(i32, i32)> {
    LABEL_RECTS
        .get(index)
        .map(|(_, x, y, w, h)| (x + w / 2, y + h / 2))
}

/// True when a point sits inside one groupbox rect.
fn group_contains(entry: &(GroupKey, i32, i32, i32, i32), point: (i32, i32)) -> bool {
    let (_, x, y, w, h) = *entry;
    point.0 >= x && point.0 <= x + w && point.1 >= y && point.1 <= y + h
}

/// Creation order for the whole window: tab strip, then each tab's
/// groupboxes with their contents top-to-bottom (buddy labels injected
/// immediately before their editors), then the footer chrome. Hidden
/// pages disable so `IsDialogMessageW` skips them.
#[must_use]
pub fn creation_sequence() -> Vec<CreateStep> {
    let mut seq = vec![CreateStep::TabStrip];
    for tab in 0..TAB_COUNT {
        for (group_index, entry) in GROUPS.iter().enumerate() {
            if group_tab(entry.0) != Some(tab) {
                continue;
            }
            seq.push(CreateStep::Group(group_index));
            // Contents sorted top-to-bottom, left-to-right; then each
            // buddy label moves directly ahead of its editor.
            let mut items: Vec<(i32, i32, CreateStep)> = Vec::new();
            for id in ALL_CONTROL_IDS {
                if control_tab(*id) != Some(tab) {
                    continue;
                }
                // Buddy editors emit in the pairing phase below, with
                // their label directly ahead.
                if BUDDY_LABELS.iter().any(|(editor, _)| editor == id) {
                    continue;
                }
                let Some(center) = control_center(*id) else {
                    continue;
                };
                if !group_contains(entry, center) {
                    continue;
                }
                let (x, y, _, _) = control_rect(*id).expect("rect");
                items.push((y, x, CreateStep::Control(*id)));
            }
            for (label_index, rect) in LABEL_RECTS.iter().enumerate() {
                if rect.0 != tab {
                    continue;
                }
                let Some(center) = label_center(label_index) else {
                    continue;
                };
                if !group_contains(entry, center) {
                    continue;
                }
                if BUDDY_LABELS.iter().any(|(_, label)| *label == label_index) {
                    continue;
                }
                items.push((rect.2, rect.1, CreateStep::Label(label_index)));
            }
            items.sort_by_key(|item| (item.0, item.1));
            let mut ordered: Vec<CreateStep> = items.into_iter().map(|(_, _, step)| step).collect();
            for (editor, label) in BUDDY_LABELS {
                if control_tab(*editor) != Some(tab) {
                    continue;
                }
                let Some(center) = control_center(*editor) else {
                    continue;
                };
                if !group_contains(entry, center) {
                    continue;
                }
                ordered.push(CreateStep::Label(*label));
                ordered.push(CreateStep::Control(*editor));
            }
            seq.extend(ordered);
        }
    }
    seq.extend([ID_TAB_BADGE, ID_STATUS, ID_APPLY, ID_REVERT, ID_CLOSE].map(CreateStep::Control));
    seq
}

/// True when keyboard focus must leave `focused_id` for the tab strip on
/// a switch to `new_tab`: the focused control hides with its old page.
/// Chrome never moves; id-0 note edits are compared by HWND in the shell.
#[must_use]
pub fn needs_focus_move(focused_id: u32, new_tab: usize) -> bool {
    matches!(control_tab(focused_id), Some(tab) if tab != new_tab)
}

/// One sibling-order operation for the native shell, in execution order.
/// The shell runs [`zorder_plan`] with `SetWindowPos(..., HWND_BOTTOM,
/// `SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE)`: each target goes to the very
/// bottom, so no `hWndInsertAfter` anchor semantics are involved at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZOrderOp {
    /// Send one groupbox (index into [`GROUPS`], i.e. creation order) to
    /// the very bottom.
    GroupToBottom(usize),
    /// Send the tab strip to the very bottom, below every groupbox.
    StripToBottom,
}

/// Execution plan: groupboxes to `HWND_BOTTOM` in reverse creation order,
/// then the strip to `HWND_BOTTOM` last. Reverse-then-strip yields bottom
/// to top: strip, groupboxes in creation order, then every unmoved page
/// control and footer control on top in unchanged relative order.
///
/// Why no anchors: `SetWindowPos` puts the moved window BELOW/BEHIND its
/// `hWndInsertAfter` anchor (the anchor precedes it), so chaining each
/// groupbox "after" the strip or its predecessor leaves the strip on top.
/// Absolute `HWND_BOTTOM` placements compose without that trap.
#[must_use]
pub fn zorder_plan() -> Vec<ZOrderOp> {
    let mut plan: Vec<ZOrderOp> = (0..GROUPS.len())
        .rev()
        .map(ZOrderOp::GroupToBottom)
        .collect();
    plan.push(ZOrderOp::StripToBottom);
    plan
}

/// Scaled client fit for one DPI against a 1080p work area (1920x1040
/// physical after a 40px taskbar): the DPI-scaled client plus the frame
/// allowances must fit. The shell debug-asserts this for the live DPI.
#[must_use]
pub fn client_fits_1080p(dpi: u32) -> bool {
    let scaled_w = (i64::from(CLIENT_W) * i64::from(dpi) + 48) / 96;
    let scaled_h = (i64::from(CLIENT_H) * i64::from(dpi) + 48) / 96;
    scaled_w + i64::from(FRAME_W_ALLOW) <= 1920 && scaled_h + i64::from(FRAME_H_ALLOW) <= 1040
}

/// Origin plus outer fit for one DPI: the fixed creation origin plus the
/// worst-case outer (scaled client plus frame allowances) must sit inside
/// the same 1920x1040 work area. Size fit alone still allows a clipped
/// bottom edge when the top is fixed (e.g. top 80 plus a 980px outer at
/// 150% runs past 1040), so the shell asserts this too.
#[must_use]
pub fn origin_fits_1080p(dpi: u32) -> bool {
    let scaled_w = (i64::from(CLIENT_W) * i64::from(dpi) + 48) / 96;
    let scaled_h = (i64::from(CLIENT_H) * i64::from(dpi) + 48) / 96;
    i64::from(ORIGIN_X) + scaled_w + i64::from(FRAME_W_ALLOW) <= 1920
        && i64::from(ORIGIN_Y) + scaled_h + i64::from(FRAME_H_ALLOW) <= 1040
}

/// True for the first radio of each auto pair/triple: the shell gives
/// these `WS_GROUP` so every pair stays its own exclusion group. Single
/// source for the shell's `style_for`: exactly one starter per field
/// (the workspace triple's starter covers all three), so reordering
/// groupboxes in the sibling order can never merge two fields into one
/// exclusion group.
#[must_use]
pub const fn is_radio_first(id: u32) -> bool {
    matches!(
        id,
        ID_DEFAULT_TILED
            | ID_WORKSPACE_MODE_LOCAL
            | ID_SAME_AXIS_GROUP
            | ID_BORN_MAX_TILE
            | ID_FIXED_SIZE_BOTH
            | ID_ANCHOR_LARGEST
            | ID_NO_SPACE_FLOAT
            | ID_MIGRATION_REFILL_LAST
            | ID_SWITCH_CURSOR
            | ID_PLACE_CURSOR_MRU
    )
}

/// Whole-layout invariant: every page rect sits inside the tab display
/// area, every chrome rect inside the client, every single-line text
/// fits its rect, and the longest badge wording fits its static. The
/// shell debug-asserts this after building specs (zero release cost).
#[must_use]
pub fn layout_valid() -> bool {
    for id in ALL_CONTROL_IDS {
        let Some((x, y, w, h)) = control_rect(*id) else {
            return false;
        };
        if w <= 0 || h <= 0 || x < 0 || y < 0 || x + w > CLIENT_W || y + h > CLIENT_H {
            return false;
        }
        match control_tab(*id) {
            None => {
                if y < FOOTER_TOP && *id != ID_TABS {
                    return false;
                }
            }
            Some(_) => {
                if y < TAB_DISPLAY_TOP || y + h > TAB_DISPLAY_BOTTOM {
                    return false;
                }
            }
        }
        if control_class(*id).is_none() {
            return false;
        }
        if !control_scrollable(*id) {
            let text = control_text(*id);
            let class = control_class(*id).unwrap_or("STATIC");
            if text.is_empty() || class == "EDIT" || class == "LISTBOX" || *id == ID_TABS {
                continue;
            }
            // Buttons read on one line; statics may wrap within their
            // rect height instead of clipping.
            let fits = if class == "STATIC" {
                static_fits(&text, w, h)
            } else {
                single_line_fits(&text, w)
            };
            if !fits {
                return false;
            }
        }
    }
    for entry in GROUPS {
        let (_, x, y, w, h) = *entry;
        if x + w > CLIENT_W || y < TAB_DISPLAY_TOP || y + h > TAB_DISPLAY_BOTTOM {
            return false;
        }
        if !single_line_fits(&group_title(entry.0), w) {
            return false;
        }
    }
    for (index, rect) in LABEL_RECTS.iter().enumerate() {
        let (_, x, y, w, h) = *rect;
        if x + w > CLIENT_W || y < TAB_DISPLAY_TOP || y + h > TAB_DISPLAY_BOTTOM {
            return false;
        }
        if label_class(index) == "EDIT" {
            continue;
        }
        if !static_fits(&label_text(index), w, h) {
            return false;
        }
    }
    badge_text(false, true, 103, 99).chars().count() <= BADGE_MAX_CHARS
}

/// Every automated control id in one place for the exact-once tests.
pub const ALL_CONTROL_IDS: &[u32] = &[
    ID_TABS,
    ID_INNER_GAP,
    ID_OUTER_GAP,
    ID_BORDER_ENABLED,
    ID_BORDER_THEME,
    ID_BORDER_COLOR,
    ID_BORDER_WIDTH,
    ID_BORDER_GAP,
    ID_BORDER_RADIUS,
    ID_UNDERLAY_ENABLED,
    ID_UNDERLAY_COLOR,
    ID_UNDERLAY_EXTENSION,
    ID_KEYBOARD_TAKEOVER,
    ID_MOUSE_SNAP,
    ID_SNAP_MEANING,
    ID_ALLOW_WIN_L,
    ID_LOCK_WARNING,
    ID_PRESET_AUTHENTIC,
    ID_PRESET_COMPATIBLE,
    ID_PRESET_EXPLAIN,
    ID_BINDING_LIST,
    ID_BINDING_INFO,
    ID_BIND_KEEP,
    ID_BIND_DISABLE,
    ID_BIND_CHORD,
    ID_BIND_REBIND,
    ID_REBIND_NOTE,
    ID_APPLY,
    ID_REVERT,
    ID_CLOSE,
    ID_STATUS,
    ID_DEFAULT_TILED,
    ID_DEFAULT_FLOATING,
    ID_SAME_AXIS_GROUP,
    ID_SAME_AXIS_SWAP,
    ID_BORN_MAX_TILE,
    ID_BORN_MAX_OVERLAY,
    ID_FIXED_SIZE_BOTH,
    ID_FIXED_SIZE_EITHER,
    ID_MIGRATION_REFILL_LAST,
    ID_MIGRATION_REFILL_MRU,
    ID_ANCHOR_LARGEST,
    ID_ANCHOR_CLOSEST,
    ID_ANCHOR_NO_SEARCH,
    ID_NO_SPACE_FLOAT,
    ID_NO_SPACE_OVERLAP,
    ID_SWITCH_CURSOR,
    ID_SWITCH_FOCUSED,
    ID_FOCUS_FOLLOWS,
    ID_CURSOR_WARP,
    ID_PLACE_CURSOR_MRU,
    ID_PLACE_CURSOR_CLOSEST,
    ID_PLACE_FOCUSED_MRU,
    ID_WORKSPACE_MODE_LOCAL,
    ID_WORKSPACE_MODE_GLOBAL,
    ID_WORKSPACE_MODE_SHARED,
    ID_TASKBAR_PAGER,
    ID_PAGER_HIGHLIGHT_THEME,
    ID_PAGER_HIGHLIGHT_COLOR,
    ID_GAMING_OPEN_XBOX,
    ID_GAMING_OPEN_GAMEBAR,
    ID_GAMING_OPEN_APP,
    ID_GAMING_RECHECK,
    ID_TAB_BADGE,
];

#[cfg(test)]
#[path = "tabs_tests.rs"]
mod tests;

#![forbid(unsafe_code)]
//! Owner-owned Windows notification tray plus the first-run preset choice.
//!
//! Portable policy only (no Win32 here): unresolved-conflict computation over
//! validated effective bindings, the menu model, the first-run decision and
//! disclosure text, the 32x32 BGRA tiling glyph (plus warning overlay), and
//! the icon add/remove gate (duplicate-add refusal plus MODIFY-first
//! TaskbarCreated recovery).
//!
//! KDE parity sources: `crates/omnitiler/src/tray.rs` (disabled
//! status row, conflict warning overlay plus a top `Conflicting KDE
//! settings...` row opening Settings, left-click opens the menu) and
//! `docs/reference-cosmic-tray-menu.md` (status/menu/Settings shape). Windows
//! differences are explicit:
//!
//! - KDE parity: a current-workspace tiled checkbox plus new-workspace
//!   Tiled/Floating default choices. The checkbox reflects the core logical
//!   tiling mode for the current view; a missing current scope disables only
//!   the checkbox while the default choices stay usable. Only the default
//!   persists.
//! - The conflict row names Windows OS shortcut conflicts (there is no
//!   `kwinrc [Windows]` edge-key analogue on Windows).
//! - The warning fires only for kept effective bindings with known-unresolved
//!   OS conflicts (Win+G Game Bar, Win+F11 Xbox mode, plus the Win+L lock
//!   chord only when it is actually kept as Win+L and the runtime Win+L
//!   opt-in allows it, evaluated at the actual rebound chords). Other kept
//!   chords stay silent (physical containment remains unproven; the tray
//!   warns only known unresolved conflicts); disabled bindings, unimplemented
//!   resize rows, and a gated/passing-through Win+L never warn.
//!
//! Native effects live in `crate::tray_sys` (`cfg(windows)` only) through
//! official `Shell_NotifyIconW` on a hidden owner-pump top-level window. The
//! icon holds no persistent state (no file, no registry): graceful stop
//! removes it with `NIM_DELETE`, recovery deletes a crash ghost by the stable
//! project GUID under the lease, and the single-owner ledger lease guarantees
//! at most one live owner (hence at most one live icon).

use crate::settings::{Settings, apply_preset, effective_bindings};

// ---------------------------------------------------------------------------
// Stable native surface (window class, callback, menu ids, prompt title).
// ---------------------------------------------------------------------------

/// Hidden owner-pump top-level window class for the tray icon.
pub const TRAY_WINDOW_CLASS: &str = "OmniTilerTray";
/// Broadcast name re-registering the icon after Explorer restarts.
pub const TASKBAR_CREATED_MESSAGE: &str = "TaskbarCreated";
/// First-run prompt title (stable for UI Automation lookup).
pub const FIRST_RUN_TITLE: &str = "OmniTiler - First Run";
/// Tray tooltip title.
pub const TRAY_TOOLTIP_TITLE: &str = "OmniTiler";

/// Tray callback message id (`WM_APP + 100`, private to the owner window).
pub const TRAY_CALLBACK_MESSAGE: u32 = 0x8000 + 100;

/// Menu command: top conflict row, visible only with unresolved conflicts.
pub const MENU_ID_CONFLICT: u32 = 1001;
/// Menu row: disabled live-status line (not a command).
pub const MENU_ID_STATUS: u32 = 1002;
/// Menu command: open the existing native Settings window.
pub const MENU_ID_SETTINGS: u32 = 1003;
/// Menu command: graceful owner stop through the normal cleanup path.
pub const MENU_ID_STOP: u32 = 1004;
/// Menu command: toggle tiling for the current workspace (session-only).
pub const MENU_ID_WORKSPACE_TOGGLE: u32 = 1005;
/// Menu command: new workspaces start tiled (persisted default).
pub const MENU_ID_DEFAULT_TILED: u32 = 1006;
/// Menu command: new workspaces start floating (persisted default).
pub const MENU_ID_DEFAULT_FLOATING: u32 = 1007;

/// Top conflict row label (KDE names its `[Windows]` edge keys here; Windows
/// names OS shortcut conflicts instead).
pub const CONFLICT_LABEL: &str = "Conflicting Windows settings...";
/// Settings row label.
pub const SETTINGS_LABEL: &str = "Settings...";
/// Stop row label.
pub const STOP_LABEL: &str = "Stop";
/// Current-workspace tiling checkbox label (checked means tiled).
pub const WORKSPACE_TOGGLE_LABEL: &str = "Tiling for current workspace";
/// New-workspace default choice labels (exactly one checked).
pub const DEFAULT_TILED_LABEL: &str = "New workspaces: Tiled";
pub const DEFAULT_FLOATING_LABEL: &str = "New workspaces: Floating";

/// Icon pixel dimension (square).
pub const TRAY_ICON_SIZE: usize = 32;

// ---------------------------------------------------------------------------
// Unresolved conflicts.
// ---------------------------------------------------------------------------

/// One kept effective binding whose OS containment is known-incomplete.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnresolvedConflict {
    pub id: &'static str,
    pub chord: String,
    pub note: &'static str,
}

/// True for the two known-incomplete containment notes (Xbox Game Bar on
/// Win+G, Xbox mode on Win+F11). Other kept chords stay silent (physical
/// containment remains unproven; the tray warns only known unresolved
/// conflicts).
#[must_use]
pub fn is_incomplete_containment(note: &str) -> bool {
    note.contains("cannot fully contain") || note.contains("containment is incomplete")
}

/// Unresolved conflicts for the tray: active plus effective bindings with
/// known-unresolved OS conflicts, evaluated at the actual enabled/rebound
/// chords. `takeover` is the runtime keyboard policy: with takeover off every
/// kept chord passes through natively by user choice, so nothing warns (even
/// with the Win+L opt-in on). `allow_win_l` is the runtime Win+L opt-in
/// (CLI `--allow-win-l` via `state.keyboard.allow_win_l`): the OS lock chord
/// warns only when it is actually kept as Win+L and the opt-in allows it (the
/// hook cannot reliably intercept it); a gated/passing-through, rebound-away,
/// or disabled Win+L never warns. Disabled bindings pass through natively by
/// user choice and unimplemented rows never intercept: none of those warn.
/// Other kept chords stay silent (physical containment remains unproven; the
/// tray warns only known unresolved conflicts).
#[must_use]
pub fn unresolved_conflicts(
    settings: &Settings,
    takeover: bool,
    allow_win_l: bool,
) -> Vec<UnresolvedConflict> {
    if !takeover {
        return Vec::new();
    }
    effective_bindings(settings)
        .into_iter()
        .filter(|row| row.active && row.effective)
        .filter_map(|row| {
            let note = row.conflict?;
            if is_incomplete_containment(note) {
                return Some(UnresolvedConflict {
                    id: row.id,
                    chord: row.chords.join(", "),
                    note,
                });
            }
            if allow_win_l
                && note.contains("cannot reliably intercept")
                && row.chords.iter().any(|chord| chord == "Win+L")
            {
                return Some(UnresolvedConflict {
                    id: row.id,
                    chord: row.chords.join(", "),
                    note,
                });
            }
            None
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Menu model.
// ---------------------------------------------------------------------------

/// One popup-menu entry in top-to-bottom order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuItem {
    Command {
        id: u32,
        label: String,
        enabled: bool,
        visible: bool,
        /// Checked presentation (tiled checkbox, default radios).
        checked: bool,
    },
    Separator,
}

/// Workspace section of the tray menu: the current-workspace tiled state
/// (`None` when the scope is unreadable: the checkbox disables while the
/// default choices stay usable) plus the persisted new-workspace default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkspaceMenu {
    pub current_tiled: Option<bool>,
    pub default_tiled: bool,
}

impl WorkspaceMenu {
    #[must_use]
    pub const fn new(current_tiled: Option<bool>, default_tiled: bool) -> Self {
        Self {
            current_tiled,
            default_tiled,
        }
    }
}

/// Scoped tray-toggle request: the opaque `(output, workspace)` rendered with
/// the menu. The owner drains it once on its pump and applies it only when
/// the live active workspace still matches exactly; a stale render or a
/// switched output refuses with no guessed fallback scope. Same-process
/// menu-to-pump lifetime needs no generation: the modal menu blocks the pump
/// between render and pick, so a drained request is either current or refused
/// below, never replayed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToggleScope {
    pub output: String,
    pub workspace: String,
}

impl ToggleScope {
    #[must_use]
    pub fn new(output: &str, workspace: &str) -> Self {
        Self {
            output: output.to_owned(),
            workspace: workspace.to_owned(),
        }
    }
}

/// Verify one drained toggle request against the live active workspace.
/// True only when the rendered scope still names the live active pair;
/// `None` on either side (unreadable render scope, unknown live output)
/// refuses. The checkbox truth visible on the immediately following menu
/// comes from the per-pump `sync_workspace` refresh, never from this.
#[must_use]
pub fn verify_toggle_scope(rendered: Option<(&str, &str)>, live: Option<(&str, &str)>) -> bool {
    match (rendered, live) {
        (Some((output, workspace)), Some((live_output, live_workspace))) => {
            output == live_output && workspace == live_workspace
        }
        _ => false,
    }
}

/// Build the tray menu: the conflict row (top, only with unresolved
/// conflicts), the disabled live-status row, the current-workspace tiling
/// checkbox (disabled without a readable scope), the new-workspace
/// Tiled/Floating choices (exactly one checked), then Settings and Stop.
#[must_use]
pub fn menu_items(
    conflict_visible: bool,
    status_line: &str,
    workspace: WorkspaceMenu,
) -> Vec<MenuItem> {
    menu_items_with_pager(conflict_visible, status_line, workspace, &[], None)
}

/// Build the tray menu with the focused-output pager section: one plain row
/// per workspace id in order, 1-based tentative `Workspace N` labels, with
/// the active workspace radio-checked. The section sits after the
/// toggle/default rows and before Settings; it is omitted entirely when
/// `pager_ids` is empty (unknown focused output). Clicking a row selects
/// that exact workspace through a scoped select request (stale renders
/// refuse at dispatch, like the toggle fence).
#[must_use]
pub fn menu_items_with_pager(
    conflict_visible: bool,
    status_line: &str,
    workspace: WorkspaceMenu,
    pager_ids: &[String],
    pager_active: Option<&str>,
) -> Vec<MenuItem> {
    let mut items = vec![
        MenuItem::Command {
            id: MENU_ID_CONFLICT,
            label: CONFLICT_LABEL.to_owned(),
            enabled: true,
            visible: conflict_visible,
            checked: false,
        },
        MenuItem::Command {
            id: MENU_ID_STATUS,
            label: status_line.to_owned(),
            enabled: false,
            visible: true,
            checked: false,
        },
        MenuItem::Separator,
        MenuItem::Command {
            id: MENU_ID_WORKSPACE_TOGGLE,
            label: WORKSPACE_TOGGLE_LABEL.to_owned(),
            enabled: workspace.current_tiled.is_some(),
            visible: true,
            checked: workspace.current_tiled.unwrap_or(false),
        },
        MenuItem::Command {
            id: MENU_ID_DEFAULT_TILED,
            label: DEFAULT_TILED_LABEL.to_owned(),
            enabled: true,
            visible: true,
            checked: workspace.default_tiled,
        },
        MenuItem::Command {
            id: MENU_ID_DEFAULT_FLOATING,
            label: DEFAULT_FLOATING_LABEL.to_owned(),
            enabled: true,
            visible: true,
            checked: !workspace.default_tiled,
        },
    ];
    if !pager_ids.is_empty() {
        items.push(MenuItem::Separator);
        for (index, id) in pager_ids.iter().enumerate() {
            items.push(MenuItem::Command {
                id: crate::tray_pager::menu_id_for_index(index),
                label: crate::tray_pager::pager_label(index as u32 + 1),
                enabled: true,
                visible: true,
                checked: pager_active.is_some_and(|active| active == id),
            });
        }
    }
    items.push(MenuItem::Separator);
    items.push(MenuItem::Command {
        id: MENU_ID_SETTINGS,
        label: SETTINGS_LABEL.to_owned(),
        enabled: true,
        visible: true,
        checked: false,
    });
    items.push(MenuItem::Command {
        id: MENU_ID_STOP,
        label: STOP_LABEL.to_owned(),
        enabled: true,
        visible: true,
        checked: false,
    });
    items
}

/// Disabled status-row text for the menu: enabled state plus the owner's
/// settings status (`saved:N`, `missing: defaults`, `degraded:...`).
#[must_use]
pub fn status_line(settings_status: &str) -> String {
    let short: String = settings_status.chars().take(64).collect();
    format!("Status: Enabled ({short})")
}

// ---------------------------------------------------------------------------
// Menu tracking cancellation and pick outcome.
// ---------------------------------------------------------------------------

/// Outcome of one tray-menu pick after tracking returns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuPickOutcome {
    /// Open Settings (top conflict row or Settings row).
    SpawnSettings,
    /// Graceful owner stop through the normal cleanup path.
    RequestStop,
    /// Flip the current-workspace tiling mode on the owner loop.
    ToggleWorkspace,
    /// Persist the new-workspace default (never a session override).
    PersistDefault { default_tiled: bool },
    /// Select one pager workspace row (0-based row in the rendered pager).
    SelectWorkspace { index: usize },
    /// Dismissal, disabled status row, unknown id, or a pick discarded by a
    /// racing graceful stop: no spawn, no toggle, no write.
    Noop,
}

/// Pure post-tracking decision used by the native menu path. `picked` is the
/// tracking return (0 when cancelled or dismissed), `stop_during` whether a
/// graceful stop was observed while the menu was open, and
/// `toggle_available` whether the rendered scope can carry a toggle (live
/// current scope present, the same fence the native side enforces). A stop
/// wins over any racing pick except the Stop pick itself (already stopping):
/// settings spawns, workspace toggles and default persists are discarded so
/// teardown mutates nothing. Dismissal, the disabled status row and unknown
/// ids are always noop.
#[must_use]
pub const fn resolve_menu_pick(
    picked: u32,
    stop_during: bool,
    toggle_available: bool,
) -> MenuPickOutcome {
    resolve_menu_pick_with_pager(picked, stop_during, toggle_available, 0)
}

/// Pure post-tracking decision with the pager section: `pager_count` is the
/// rendered pager row count. Pager rows resolve to `SelectWorkspace` by row;
/// out-of-range ids stay noop, and a racing stop discards pager picks like
/// every other pick except Stop itself.
#[must_use]
pub const fn resolve_menu_pick_with_pager(
    picked: u32,
    stop_during: bool,
    toggle_available: bool,
    pager_count: usize,
) -> MenuPickOutcome {
    if stop_during && picked != MENU_ID_STOP {
        return MenuPickOutcome::Noop;
    }
    if picked >= crate::tray_pager::MENU_ID_WORKSPACE_FIRST {
        let index = (picked - crate::tray_pager::MENU_ID_WORKSPACE_FIRST) as usize;
        if index < pager_count {
            return MenuPickOutcome::SelectWorkspace { index };
        }
        return MenuPickOutcome::Noop;
    }
    match picked {
        MENU_ID_CONFLICT | MENU_ID_SETTINGS => MenuPickOutcome::SpawnSettings,
        MENU_ID_STOP => MenuPickOutcome::RequestStop,
        MENU_ID_STATUS => MenuPickOutcome::Noop,
        MENU_ID_WORKSPACE_TOGGLE => {
            if toggle_available {
                MenuPickOutcome::ToggleWorkspace
            } else {
                MenuPickOutcome::Noop
            }
        }
        MENU_ID_DEFAULT_TILED => MenuPickOutcome::PersistDefault {
            default_tiled: true,
        },
        MENU_ID_DEFAULT_FLOATING => MenuPickOutcome::PersistDefault {
            default_tiled: false,
        },
        _ => MenuPickOutcome::Noop,
    }
}

/// Pure menu-cancel predicate used by the native stop-timer handler: cancel
/// only when a stop is observed while a track is actually active (the handler
/// passes the live tracking guard, cleared before teardown so a queued stale
/// timer cannot cancel anything).
#[must_use]
pub const fn menu_cancel_due(stop_requested: bool, menu_open: bool) -> bool {
    stop_requested && menu_open
}

/// Whether a fresh menu track may start: refuses while a track is already
/// active on this owner (a second tray callback dispatched by the modal track
/// never opens its own menu; nested submenus stay internal to the one track).
#[must_use]
pub const fn menu_track_enter(menu_tracking: bool) -> bool {
    !menu_tracking
}

/// Whether menu tracking may proceed: the legacy path needs no timer, while
/// the stop-aware path proceeds only with its timer installed. A failed
/// install fails closed (no untracked blocking menu) so graceful stop cannot
/// starve again.
#[must_use]
pub const fn menu_track_proceed(stop_timer_needed: bool, timer_installed: bool) -> bool {
    !stop_timer_needed || timer_installed
}

// ---------------------------------------------------------------------------
// First run.
// ---------------------------------------------------------------------------

/// First-run preset choice offered when the settings file is absent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FirstRunChoice {
    Authentic,
    Compatible,
}

impl FirstRunChoice {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Authentic => "authentic",
            Self::Compatible => "compatible",
        }
    }
}

/// Native prompt body: the two presets plus the brief Win+G/Win+F11 Xbox and
/// Game Bar implication. The owned dialog labels its buttons directly, so no
/// button mapping is encoded here.
#[must_use]
pub fn first_run_body() -> String {
    "Choose the shortcut preset for this PC.\n\n\
     Authentic keeps the KDE shortcut catalog, including \
     Win+G (float) and Win+F11 (fullscreen). Windows also owns those chords \
     for Xbox Game Bar and Xbox mode, and the tiler cannot fully contain \
     the OS handlers there.\n\n\
     Compatible disables every other OS-conflicting chord and rebinds float \
     to Win+Ctrl+G and fullscreen to Win+Ctrl+F11; rows can be re-enabled or rebound later in \
     Settings.\n\n\
     Choose Authentic or Compatible."
        .to_owned()
}

/// Validated settings for one first-run choice. Authentic is the catalog
/// defaults; compatible is the deterministic reset disabling the 39
/// OS-conflicting rows and rebinding float/fullscreen to Win+Ctrl+G /
/// Win+Ctrl+F11 (Win+L opt-in preserved in both).
#[must_use]
pub fn settings_for_choice(choice: FirstRunChoice) -> Settings {
    let mut settings = Settings::default();
    if choice == FirstRunChoice::Compatible {
        apply_preset(&mut settings, crate::settings::Preset::Compatible);
    }
    settings
}

/// Whether the owner shows the first-run prompt: only when the settings file
/// is absent and no graceful stop was already requested. A stop racing
/// startup skips the modal UI so the loop exits promptly.
#[must_use]
pub const fn first_run_should_prompt(file_missing_before: bool, stop_before: bool) -> bool {
    file_missing_before && !stop_before
}

/// Post-prompt first-run outcome: what the owner does with a prompt result.
/// `choice` is the prompt outcome (`None` when dismissed), `file_missing_after`
/// is the lease-held recheck after the prompt, and `stop_during` is the
/// lease-held stop recheck after the prompt. A stop observed while the prompt
/// was pending wins over any pending choice: nothing is published.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FirstRunPostDecision {
    Publish(FirstRunChoice),
    DiscardPresent,
    DismissedUnsaved,
    CancelledStopped,
}

/// Pure post-prompt decision used by the actual owner path. Order matters:
/// stop-during wins over a racing choice (no write), then dismissal (no
/// write), then a file that appeared mid-prompt (discard, no overwrite), else
/// publish the choice.
#[must_use]
pub const fn first_run_post_decision(
    choice: Option<FirstRunChoice>,
    file_missing_after: bool,
    stop_during: bool,
) -> FirstRunPostDecision {
    if stop_during {
        return FirstRunPostDecision::CancelledStopped;
    }
    let Some(choice) = choice else {
        return FirstRunPostDecision::DismissedUnsaved;
    };
    if !file_missing_after {
        return FirstRunPostDecision::DiscardPresent;
    }
    FirstRunPostDecision::Publish(choice)
}

// ---------------------------------------------------------------------------
// Tiling glyph (32x32 BGRA, native byte order per pixel: B, G, R, A).
// ---------------------------------------------------------------------------

const ACCENT: [u8; 4] = [0xD8, 0xB4, 0x7F, 0xFF];
const PANE_HOT: [u8; 4] = [0x22, 0x7E, 0xE6, 0xFF];
const PANE_DARK: [u8; 4] = [0x38, 0x2A, 0x1E, 0xFF];
const WARN: [u8; 4] = [0x29, 0xB4, 0xF0, 0xFF];
const WARN_INK: [u8; 4] = [0x1A, 0x1A, 0x1A, 0xFF];

fn put(pixels: &mut [u8], x: usize, y: usize, color: [u8; 4]) {
    let at = (y * TRAY_ICON_SIZE + x) * 4;
    pixels[at..at + 4].copy_from_slice(&color);
}

/// Render the tiling-identity glyph: dark panes with an accent frame plus
/// center dividers (a 2x2 tile grid) and a hot top-left pane. With `warning`,
/// an amber corner badge with a dark exclamation overlays the bottom-right.
#[must_use]
pub fn tray_icon_bgra(warning: bool) -> Vec<u8> {
    let size = TRAY_ICON_SIZE;
    let mut pixels = vec![0u8; size * size * 4];
    for y in 0..size {
        for x in 0..size {
            let frame = x < 2 || y < 2 || x >= size - 2 || y >= size - 2;
            let divider_v = (x == size / 2 - 1 || x == size / 2) && y >= 2 && y < size - 2;
            let divider_h = (y == size / 2 - 1 || y == size / 2) && x >= 2 && x < size - 2;
            let hot = x >= 2 && x < size / 2 - 1 && y >= 2 && y < size / 2 - 1;
            let color = if frame || divider_v || divider_h {
                ACCENT
            } else if hot {
                PANE_HOT
            } else {
                PANE_DARK
            };
            put(&mut pixels, x, y, color);
        }
    }
    if warning {
        // Amber right-triangle badge (legs 13px at the bottom-right corner).
        for y in 0..size {
            for x in 0..size {
                if (size - 1 - x) + (size - 1 - y) <= 12 {
                    put(&mut pixels, x, y, WARN);
                }
            }
        }
        // Dark exclamation: bar plus dot, inside the badge.
        for y in 22..=27 {
            for x in 27..=28 {
                put(&mut pixels, x, y, WARN_INK);
            }
        }
        for y in 29..=30 {
            for x in 27..=28 {
                put(&mut pixels, x, y, WARN_INK);
            }
        }
    }
    pixels
}

/// Digit ink for the tray workspace number (white with a dark shadow), drawn
/// over the hot top-left pane, clear of the bottom-right warning badge.
const DIGIT_INK: [u8; 4] = [0xFF, 0xFF, 0xFF, 0xFF];
const DIGIT_SHADOW: [u8; 4] = [0x10, 0x10, 0x10, 0xFF];

/// 3x5 micro font for `0..=9`: each row holds 3 bits (high bit left).
const DIGIT_FONT: [[u8; 5]; 10] = [
    [0b111, 0b101, 0b101, 0b101, 0b111],
    [0b010, 0b110, 0b010, 0b010, 0b111],
    [0b111, 0b001, 0b111, 0b100, 0b111],
    [0b111, 0b001, 0b111, 0b001, 0b111],
    [0b101, 0b101, 0b111, 0b001, 0b001],
    [0b111, 0b100, 0b111, 0b001, 0b111],
    [0b111, 0b100, 0b111, 0b101, 0b111],
    [0b111, 0b001, 0b001, 0b010, 0b010],
    [0b111, 0b101, 0b111, 0b101, 0b111],
    [0b111, 0b101, 0b111, 0b001, 0b111],
];

fn put_clipped(pixels: &mut [u8], x: i32, y: i32, color: [u8; 4]) {
    if x < 0 || y < 0 || x >= TRAY_ICON_SIZE as i32 || y >= TRAY_ICON_SIZE as i32 {
        return;
    }
    put(pixels, x as usize, y as usize, color);
}

/// Render the tiling-identity glyph with the focused-output active 1-based
/// ordinal overlaid (`None` keeps the plain glyph byte-for-byte). The
/// warning badge is preserved underneath the number either way; app icons
/// and urgency markers are deferred. Single digits render doubled in the
/// hot pane; multi-digit ordinals render doubled from the left frame edge
/// and clip at the icon bounds.
#[must_use]
pub fn tray_icon_bgra_with_number(warning: bool, ordinal: Option<u32>) -> Vec<u8> {
    let mut pixels = tray_icon_bgra(warning);
    let Some(ordinal) = ordinal else {
        return pixels;
    };
    let text = ordinal.to_string();
    // Doubled 3x5 cells with a 2px column gap: one cell is 6px wide.
    let scale = 2;
    let step = 3 * scale + 2;
    let origin_x = if text.len() == 1 { 5 } else { 2 };
    let origin_y = 3;
    for (slot, byte) in text.bytes().enumerate() {
        let Some(digit) = byte.checked_sub(b'0').filter(|d| *d < 10) else {
            continue;
        };
        let glyph = DIGIT_FONT[digit as usize];
        for (row, bits) in glyph.iter().enumerate() {
            for col in 0..3 {
                if bits & (1 << (2 - col)) == 0 {
                    continue;
                }
                let base_x = origin_x + slot as i32 * step + col * scale;
                let base_y = origin_y + row as i32 * scale;
                for dy in 0..scale {
                    for dx in 0..scale {
                        put_clipped(
                            pixels.as_mut(),
                            base_x + dx + 1,
                            base_y + dy + 1,
                            DIGIT_SHADOW,
                        );
                    }
                }
                for dy in 0..scale {
                    for dx in 0..scale {
                        put_clipped(pixels.as_mut(), base_x + dx, base_y + dy, DIGIT_INK);
                    }
                }
            }
        }
    }
    pixels
}

// ---------------------------------------------------------------------------
// Icon add/remove state machine.
// ---------------------------------------------------------------------------

/// Duplicate-add gate. The caller issues exactly one `NIM_ADD` per
/// `needs_add() == true`, marks `on_added()`, and issues `NIM_DELETE` once
/// via `on_removed()`. Explorer-restart recovery is MODIFY-first through
/// [`taskbar_recovery`]: a surviving icon answers `NIM_MODIFY`, so no
/// duplicate add is ever issued.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TrayIconState {
    added: bool,
}

impl TrayIconState {
    #[must_use]
    pub const fn new() -> Self {
        Self { added: false }
    }

    #[must_use]
    pub const fn is_added(self) -> bool {
        self.added
    }

    #[must_use]
    pub const fn needs_add(self) -> bool {
        !self.added
    }

    pub const fn on_added(&mut self) {
        self.added = true;
    }

    pub const fn on_removed(&mut self) {
        self.added = false;
    }
}

/// Outcome of one `TaskbarCreated` recovery attempt. The native side tries
/// `NIM_MODIFY` first (the icon survived: a duplicate `NIM_ADD` would fail),
/// else a tolerant remove plus re-add. `Pending` keeps the state gate open
/// so the normal sync retries the add; exactly one bounded `tray-taskbar`
/// log line carries the outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskbarRecovery {
    Revalidated,
    ReAdded,
    Pending,
}

/// Pure `TaskbarCreated` recovery decision: a successful modify means the
/// icon survived (no add attempted); otherwise the re-add result decides.
#[must_use]
pub const fn taskbar_recovery(modify_ok: bool, add_ok: bool) -> TaskbarRecovery {
    if modify_ok {
        TaskbarRecovery::Revalidated
    } else if add_ok {
        TaskbarRecovery::ReAdded
    } else {
        TaskbarRecovery::Pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{
        BindingSetting, BindingState, compatible_disabled_ids, validate_settings,
    };

    fn authentic() -> Settings {
        Settings::default()
    }

    #[test]
    fn authentic_default_warns_only_g_and_f11() {
        let conflicts = unresolved_conflicts(&authentic(), true, false);
        let ids: Vec<&str> = conflicts.iter().map(|c| c.id).collect();
        assert_eq!(ids, vec!["toggle-float", "toggle-fullscreen"]);
        assert!(
            conflicts
                .iter()
                .all(|c| is_incomplete_containment(c.note) && !c.chord.is_empty())
        );
    }

    #[test]
    fn compatible_preset_clears_every_warning() {
        let mut settings = authentic();
        apply_preset(&mut settings, crate::settings::Preset::Compatible);
        assert!(validate_settings(&settings).is_ok());
        assert!(unresolved_conflicts(&settings, true, false).is_empty());
        assert!(unresolved_conflicts(&settings, true, true).is_empty());
    }

    #[test]
    fn rebound_g_away_clears_the_float_warning() {
        let mut settings = authentic();
        settings.bindings.insert(
            "toggle-float".to_owned(),
            BindingSetting {
                state: BindingState::Rebind,
                chord: Some("Win+Q".to_owned()),
            },
        );
        assert!(validate_settings(&settings).is_ok());
        let ids: Vec<&str> = unresolved_conflicts(&settings, true, false)
            .iter()
            .map(|c| c.id)
            .collect();
        assert_eq!(ids, vec!["toggle-fullscreen"]);
    }

    #[test]
    fn disabled_g_clears_the_float_warning() {
        let mut settings = authentic();
        settings.bindings.insert(
            "toggle-float".to_owned(),
            BindingSetting {
                state: BindingState::Disabled,
                chord: None,
            },
        );
        assert!(validate_settings(&settings).is_ok());
        let ids: Vec<&str> = unresolved_conflicts(&settings, true, false)
            .iter()
            .map(|c| c.id)
            .collect();
        assert_eq!(ids, vec!["toggle-fullscreen"]);
    }

    #[test]
    fn takeover_contained_chords_and_win_l_gate_stay_silent() {
        // Other kept chords stay silent (physical containment remains
        // unproven; the tray warns only known unresolved conflicts) and the
        // gated Win+L pass-through never warns.
        let conflicts = unresolved_conflicts(&authentic(), true, false);
        let ids: Vec<&str> = conflicts.iter().map(|c| c.id).collect();
        assert!(!ids.contains(&"focus-left"));
        assert!(!ids.contains(&"focus-right"));
        assert!(!ids.contains(&"move-left"));
        assert!(!ids.contains(&"toggle-sticky"));
        assert!(!ids.contains(&"workspace-select-1"));
    }

    #[test]
    fn takeover_off_passes_everything_through_silently() {
        // Takeover off is a global pass-through by user choice: even the
        // known-incomplete Xbox chords and an allowed Win+L are no longer
        // intercepted, so the tray must not warn.
        assert!(unresolved_conflicts(&authentic(), false, false).is_empty());
        assert!(unresolved_conflicts(&authentic(), false, true).is_empty());
        let mut compatible_base = authentic();
        apply_preset(&mut compatible_base, crate::settings::Preset::Compatible);
        assert!(unresolved_conflicts(&compatible_base, true, false).is_empty());
    }

    #[test]
    fn allowed_win_l_warns_at_actual_lock_chord() {
        let conflicts = unresolved_conflicts(&authentic(), true, true);
        let ids: Vec<&str> = conflicts.iter().map(|c| c.id).collect();
        assert_eq!(
            ids,
            vec!["focus-right", "toggle-float", "toggle-fullscreen"]
        );
        let lock = conflicts
            .iter()
            .find(|c| c.id == "focus-right")
            .expect("lock warns");
        assert_eq!(lock.chord, "Win+L");
        assert!(lock.note.contains("cannot reliably intercept"));
    }

    #[test]
    fn win_l_gate_rebound_and_disabled_stay_silent() {
        let mut rebound = authentic();
        rebound.bindings.insert(
            "focus-right".to_owned(),
            BindingSetting {
                state: BindingState::Rebind,
                chord: Some("Win+Q".to_owned()),
            },
        );
        assert!(validate_settings(&rebound).is_ok());
        let ids: Vec<&str> = unresolved_conflicts(&rebound, true, true)
            .iter()
            .map(|c| c.id)
            .collect();
        assert_eq!(ids, vec!["toggle-float", "toggle-fullscreen"]);
        let mut disabled = authentic();
        disabled.bindings.insert(
            "focus-right".to_owned(),
            BindingSetting {
                state: BindingState::Disabled,
                chord: None,
            },
        );
        assert!(validate_settings(&disabled).is_ok());
        let ids: Vec<&str> = unresolved_conflicts(&disabled, true, true)
            .iter()
            .map(|c| c.id)
            .collect();
        assert_eq!(ids, vec!["toggle-float", "toggle-fullscreen"]);
    }

    #[test]
    fn win_l_gate_uses_runtime_parameter_not_file() {
        let mut file_opt_in = authentic();
        file_opt_in.core.keyboard.allow_win_l = true;
        assert!(validate_settings(&file_opt_in).is_ok());
        let gated_ids: Vec<&str> = unresolved_conflicts(&file_opt_in, true, false)
            .iter()
            .map(|c| c.id)
            .collect();
        assert_eq!(gated_ids, vec!["toggle-float", "toggle-fullscreen"]);
        let allowed_ids: Vec<&str> = unresolved_conflicts(&authentic(), true, true)
            .iter()
            .map(|c| c.id)
            .collect();
        assert!(allowed_ids.contains(&"focus-right"));
    }

    #[test]
    fn taskbar_recovery_prefers_modify_then_readd() {
        // Icon survived: modify succeeds, no add attempted.
        assert_eq!(taskbar_recovery(true, false), TaskbarRecovery::Revalidated);
        // Icon gone: tolerant re-add decides between re-added and pending.
        assert_eq!(taskbar_recovery(false, true), TaskbarRecovery::ReAdded);
        assert_eq!(taskbar_recovery(false, false), TaskbarRecovery::Pending);
    }

    #[test]
    fn menu_hides_conflict_row_without_conflicts() {
        let tiled = WorkspaceMenu::new(Some(true), true);
        let with = menu_items(
            !unresolved_conflicts(&authentic(), true, false).is_empty(),
            "Status: Enabled (saved:1)",
            tiled,
        );
        let conflict = with.iter().find_map(|item| match item {
            MenuItem::Command { id, visible, .. } if *id == MENU_ID_CONFLICT => Some(*visible),
            _ => None,
        });
        assert_eq!(conflict, Some(true));
        let without = menu_items(false, "Status: Enabled (saved:1)", tiled);
        let conflict = without.iter().find_map(|item| match item {
            MenuItem::Command { id, visible, .. } if *id == MENU_ID_CONFLICT => Some(*visible),
            _ => None,
        });
        assert_eq!(conflict, Some(false));
        // Status row is present but disabled; Settings and Stop are enabled.
        let status_enabled = without.iter().find_map(|item| match item {
            MenuItem::Command { id, enabled, .. } if *id == MENU_ID_STATUS => Some(*enabled),
            _ => None,
        });
        assert_eq!(status_enabled, Some(false));
        let mut ids = Vec::new();
        for item in &without {
            if let MenuItem::Command {
                id, visible: true, ..
            } = item
            {
                ids.push(*id);
            }
        }
        assert_eq!(
            ids,
            vec![
                MENU_ID_STATUS,
                MENU_ID_WORKSPACE_TOGGLE,
                MENU_ID_DEFAULT_TILED,
                MENU_ID_DEFAULT_FLOATING,
                MENU_ID_SETTINGS,
                MENU_ID_STOP
            ]
        );
    }

    #[test]
    fn menu_workspace_toggle_reflects_current_scope_truthfully() {
        let entry = |items: &Vec<MenuItem>, id: u32| {
            items.iter().find_map(|item| match item {
                MenuItem::Command {
                    id: found,
                    enabled,
                    checked,
                    visible,
                    ..
                } if *found == id => Some((*enabled, *checked, *visible)),
                _ => None,
            })
        };
        // Tiled scope: checkbox enabled and checked; Tiled default checked.
        let tiled = menu_items(
            false,
            "Status: Enabled (saved:1)",
            WorkspaceMenu::new(Some(true), true),
        );
        assert_eq!(
            entry(&tiled, MENU_ID_WORKSPACE_TOGGLE),
            Some((true, true, true))
        );
        assert_eq!(
            entry(&tiled, MENU_ID_DEFAULT_TILED),
            Some((true, true, true))
        );
        assert_eq!(
            entry(&tiled, MENU_ID_DEFAULT_FLOATING),
            Some((true, false, true))
        );
        // Floating scope with a floating default: checkbox enabled,
        // unchecked; Floating default checked.
        let floating = menu_items(
            false,
            "Status: Enabled (saved:1)",
            WorkspaceMenu::new(Some(false), false),
        );
        assert_eq!(
            entry(&floating, MENU_ID_WORKSPACE_TOGGLE),
            Some((true, false, true))
        );
        assert_eq!(
            entry(&floating, MENU_ID_DEFAULT_TILED),
            Some((true, false, true))
        );
        assert_eq!(
            entry(&floating, MENU_ID_DEFAULT_FLOATING),
            Some((true, true, true))
        );
        // Missing scope disables only the checkbox; the default choices
        // stay usable and exactly one stays checked.
        let missing = menu_items(
            false,
            "Status: Enabled (saved:1)",
            WorkspaceMenu::new(None, true),
        );
        assert_eq!(
            entry(&missing, MENU_ID_WORKSPACE_TOGGLE),
            Some((false, false, true))
        );
        assert_eq!(
            entry(&missing, MENU_ID_DEFAULT_TILED),
            Some((true, true, true))
        );
        assert_eq!(
            entry(&missing, MENU_ID_DEFAULT_FLOATING),
            Some((true, false, true))
        );
    }

    #[test]
    fn toggle_scope_refuses_stale_or_mismatched_without_fallback() {
        // The drained request applies only when it still names the live
        // active pair: a switched workspace or output refuses, and an
        // unreadable side never guesses a scope.
        assert!(verify_toggle_scope(
            Some(("mon-1", "ws-1")),
            Some(("mon-1", "ws-1"))
        ));
        assert!(!verify_toggle_scope(
            Some(("mon-1", "ws-1")),
            Some(("mon-1", "ws-2"))
        ));
        assert!(!verify_toggle_scope(
            Some(("mon-1", "ws-1")),
            Some(("mon-2", "ws-1"))
        ));
        assert!(!verify_toggle_scope(None, Some(("mon-1", "ws-1"))));
        assert!(!verify_toggle_scope(Some(("mon-1", "ws-1")), None));
        assert!(!verify_toggle_scope(None, None));
    }

    #[test]
    fn menu_cancel_needs_stop_and_open_menu() {
        assert!(menu_cancel_due(true, true));
        assert!(!menu_cancel_due(true, false));
        assert!(!menu_cancel_due(false, true));
        assert!(!menu_cancel_due(false, false));
    }

    #[test]
    fn menu_pick_routes_without_stop() {
        use MenuPickOutcome as O;
        assert_eq!(
            resolve_menu_pick(MENU_ID_CONFLICT, false, true),
            O::SpawnSettings
        );
        assert_eq!(
            resolve_menu_pick(MENU_ID_SETTINGS, false, true),
            O::SpawnSettings
        );
        assert_eq!(resolve_menu_pick(MENU_ID_STOP, false, true), O::RequestStop);
        assert_eq!(resolve_menu_pick(MENU_ID_STATUS, false, true), O::Noop);
        assert_eq!(
            resolve_menu_pick(MENU_ID_WORKSPACE_TOGGLE, false, true),
            O::ToggleWorkspace
        );
        assert_eq!(
            resolve_menu_pick(MENU_ID_WORKSPACE_TOGGLE, false, false),
            O::Noop
        );
        assert_eq!(
            resolve_menu_pick(MENU_ID_DEFAULT_TILED, false, true),
            O::PersistDefault {
                default_tiled: true
            }
        );
        assert_eq!(
            resolve_menu_pick(MENU_ID_DEFAULT_FLOATING, false, true),
            O::PersistDefault {
                default_tiled: false
            }
        );
        // Dismissal (tracking cancel returns 0) and unknown ids noop.
        assert_eq!(resolve_menu_pick(0, false, true), O::Noop);
        assert_eq!(resolve_menu_pick(9999, false, true), O::Noop);
    }

    #[test]
    fn menu_pick_stop_wins_over_every_racing_pick() {
        use MenuPickOutcome as O;
        // A stop observed while open discards every racing pick except Stop
        // itself: no settings spawn, no workspace flip, no default persist.
        for picked in [
            MENU_ID_CONFLICT,
            MENU_ID_SETTINGS,
            MENU_ID_STATUS,
            MENU_ID_WORKSPACE_TOGGLE,
            MENU_ID_DEFAULT_TILED,
            MENU_ID_DEFAULT_FLOATING,
            0,
            9999,
        ] {
            assert_eq!(
                resolve_menu_pick(picked, true, true),
                O::Noop,
                "pick {picked} must discard under stop"
            );
            assert_eq!(
                resolve_menu_pick(picked, true, false),
                O::Noop,
                "pick {picked} must discard under stop"
            );
        }
        assert_eq!(resolve_menu_pick(MENU_ID_STOP, true, true), O::RequestStop);
        assert_eq!(resolve_menu_pick(MENU_ID_STOP, true, false), O::RequestStop);
    }

    #[test]
    fn menu_track_enter_refuses_while_tracking() {
        assert!(menu_track_enter(false));
        assert!(!menu_track_enter(true));
    }

    #[test]
    fn menu_track_proceed_fails_closed_without_timer() {
        assert!(menu_track_proceed(false, false));
        assert!(menu_track_proceed(false, true));
        assert!(menu_track_proceed(true, true));
        assert!(!menu_track_proceed(true, false));
    }

    #[test]
    fn first_run_choices_validate_and_match_presets() {
        for choice in [FirstRunChoice::Authentic, FirstRunChoice::Compatible] {
            let settings = settings_for_choice(choice);
            assert!(validate_settings(&settings).is_ok(), "{choice:?}");
        }
        assert_eq!(
            settings_for_choice(FirstRunChoice::Authentic),
            Settings::default()
        );
        let compatible = settings_for_choice(FirstRunChoice::Compatible);
        assert_eq!(
            compatible.bindings.len(),
            compatible_disabled_ids().len() + crate::settings::compatible_rebinds().len()
        );
        for id in compatible_disabled_ids() {
            assert_eq!(
                compatible.bindings.get(id).map(|b| &b.state),
                Some(&BindingState::Disabled),
                "{id}"
            );
        }
        for (id, chord) in crate::settings::compatible_rebinds() {
            assert_eq!(
                compatible.bindings.get(id).map(|b| &b.state),
                Some(&BindingState::Rebind),
                "{id}"
            );
            assert_eq!(
                compatible.bindings.get(id).and_then(|b| b.chord.as_deref()),
                Some(chord),
                "{id}"
            );
        }
        assert!(!compatible.bindings.contains_key("move-left"));
        assert!(!compatible.bindings.contains_key("toggle-sticky"));
        // Item 1 history: Compatible disables only the two recorded
        // virtual-desktop arrows; the toggle and the other six steps stay
        // kept with the honest ownership-unknown note (no warning: unknown
        // containment adds none).
        assert!(!compatible.bindings.contains_key("workspace-previous"));
        assert!(!compatible.bindings.contains_key("workspace-prev-h"));
        assert!(unresolved_conflicts(&compatible, true, false).is_empty());
    }

    #[test]
    fn first_run_discloses_xbox_chords() {
        assert!(first_run_body().contains("Win+G"));
        assert!(first_run_body().contains("Win+F11"));
        assert!(first_run_body().contains("Win+Ctrl+G"));
        assert!(first_run_body().contains("Win+Ctrl+F11"));
        assert!(first_run_body().contains("Choose Authentic or Compatible"));
    }

    #[test]
    fn first_run_prompt_gate_skips_on_file_or_stop() {
        assert!(first_run_should_prompt(true, false));
        assert!(!first_run_should_prompt(false, false));
        assert!(!first_run_should_prompt(false, true));
        assert!(!first_run_should_prompt(true, true));
    }

    #[test]
    fn first_run_post_decision_routes_stop_choice_and_file() {
        use FirstRunPostDecision as D;
        // Ordinary paths: publish either choice when the file is still
        // absent and no stop arrived; dismissal publishes nothing.
        assert_eq!(
            first_run_post_decision(Some(FirstRunChoice::Authentic), true, false),
            D::Publish(FirstRunChoice::Authentic)
        );
        assert_eq!(
            first_run_post_decision(Some(FirstRunChoice::Compatible), true, false),
            D::Publish(FirstRunChoice::Compatible)
        );
        assert_eq!(
            first_run_post_decision(None, true, false),
            D::DismissedUnsaved
        );
        // A file appearing mid-prompt discards either choice (no overwrite).
        assert_eq!(
            first_run_post_decision(Some(FirstRunChoice::Authentic), false, false),
            D::DiscardPresent
        );
        assert_eq!(
            first_run_post_decision(Some(FirstRunChoice::Compatible), false, false),
            D::DiscardPresent
        );
        // Stop while pending wins over any pending choice: no publication
        // even when the user picked a preset and the file is still absent.
        assert_eq!(
            first_run_post_decision(Some(FirstRunChoice::Authentic), true, true),
            D::CancelledStopped
        );
        assert_eq!(
            first_run_post_decision(Some(FirstRunChoice::Compatible), true, true),
            D::CancelledStopped
        );
        assert_eq!(
            first_run_post_decision(None, true, true),
            D::CancelledStopped
        );
        assert_eq!(
            first_run_post_decision(Some(FirstRunChoice::Authentic), false, true),
            D::CancelledStopped
        );
    }

    fn pixel(pixels: &[u8], x: usize, y: usize) -> [u8; 4] {
        let at = (y * TRAY_ICON_SIZE + x) * 4;
        [pixels[at], pixels[at + 1], pixels[at + 2], pixels[at + 3]]
    }

    #[test]
    fn glyph_carries_tiling_identity_and_warning_overlay() {
        let normal = tray_icon_bgra(false);
        let warning = tray_icon_bgra(true);
        assert_eq!(normal.len(), TRAY_ICON_SIZE * TRAY_ICON_SIZE * 4);
        assert_eq!(warning.len(), normal.len());
        // Accent frame, hot top-left pane, dark bottom-right pane.
        assert_eq!(pixel(&normal, 0, 0), ACCENT);
        assert_eq!(pixel(&normal, 5, 5), PANE_HOT);
        assert_eq!(pixel(&normal, 24, 24), PANE_DARK);
        assert_eq!(pixel(&normal, 15, 20), ACCENT);
        // Warning badge repaints the corner amber with a dark exclamation,
        // leaving the frame and hot pane untouched.
        assert_eq!(pixel(&warning, 30, 30), WARN);
        assert_eq!(pixel(&warning, 27, 25), WARN_INK);
        assert_eq!(pixel(&warning, 0, 0), ACCENT);
        assert_eq!(pixel(&warning, 5, 5), PANE_HOT);
        assert_ne!(pixel(&normal, 30, 30), WARN);
    }

    #[test]
    fn pager_section_orders_rows_with_single_active_radio() {
        let ids = vec!["ws-a".to_owned(), "ws-b".to_owned(), "ws-c".to_owned()];
        let items = menu_items_with_pager(
            false,
            "Status: Enabled (saved:1)",
            WorkspaceMenu::new(Some(true), true),
            &ids,
            Some("ws-b"),
        );
        // Visible command order: status, toggle, defaults, pager rows in
        // workspace order, then Settings and Stop (conflict hidden here).
        let mut order = Vec::new();
        for item in &items {
            if let MenuItem::Command {
                id, visible: true, ..
            } = item
            {
                order.push(*id);
            }
        }
        assert_eq!(
            order,
            vec![
                MENU_ID_STATUS,
                MENU_ID_WORKSPACE_TOGGLE,
                MENU_ID_DEFAULT_TILED,
                MENU_ID_DEFAULT_FLOATING,
                crate::tray_pager::MENU_ID_WORKSPACE_FIRST,
                crate::tray_pager::MENU_ID_WORKSPACE_FIRST + 1,
                crate::tray_pager::MENU_ID_WORKSPACE_FIRST + 2,
                MENU_ID_SETTINGS,
                MENU_ID_STOP,
            ]
        );
        // Exactly the active row is checked, with tentative labels.
        let mut checked_ids = Vec::new();
        let mut labels = Vec::new();
        for item in &items {
            if let MenuItem::Command {
                id,
                checked: is_checked,
                label,
                ..
            } = item
                && *id >= crate::tray_pager::MENU_ID_WORKSPACE_FIRST
            {
                if *is_checked {
                    checked_ids.push(*id);
                }
                labels.push(label.clone());
            }
        }
        assert_eq!(
            checked_ids,
            vec![crate::tray_pager::MENU_ID_WORKSPACE_FIRST + 1]
        );
        assert_eq!(labels, vec!["Workspace 1", "Workspace 2", "Workspace 3"]);
        // Empty pager omits the section without a stray separator: the
        // legacy order is unchanged.
        let legacy = menu_items(
            false,
            "Status: Enabled (saved:1)",
            WorkspaceMenu::new(None, true),
        );
        let bare = menu_items_with_pager(
            false,
            "Status: Enabled (saved:1)",
            WorkspaceMenu::new(None, true),
            &[],
            None,
        );
        assert_eq!(bare, legacy);
    }

    #[test]
    fn pager_pick_resolves_by_row_with_stop_precedence() {
        use MenuPickOutcome as O;
        assert_eq!(
            resolve_menu_pick_with_pager(
                crate::tray_pager::MENU_ID_WORKSPACE_FIRST + 1,
                false,
                true,
                3
            ),
            O::SelectWorkspace { index: 1 }
        );
        // Out-of-range pager ids and legacy ids behave as before.
        assert_eq!(
            resolve_menu_pick_with_pager(
                crate::tray_pager::MENU_ID_WORKSPACE_FIRST + 3,
                false,
                true,
                3
            ),
            O::Noop
        );
        assert_eq!(
            resolve_menu_pick_with_pager(MENU_ID_SETTINGS, false, true, 3),
            O::SpawnSettings
        );
        // A racing stop discards pager picks; Stop itself still stops.
        assert_eq!(
            resolve_menu_pick_with_pager(crate::tray_pager::MENU_ID_WORKSPACE_FIRST, true, true, 3),
            O::Noop
        );
        assert_eq!(
            resolve_menu_pick_with_pager(MENU_ID_STOP, true, true, 3),
            O::RequestStop
        );
    }

    #[test]
    fn numbered_glyph_keeps_identity_and_warning() {
        // No ordinal keeps the plain glyph byte-for-byte.
        assert_eq!(
            tray_icon_bgra_with_number(false, None),
            tray_icon_bgra(false)
        );
        assert_eq!(tray_icon_bgra_with_number(true, None), tray_icon_bgra(true));
        // Single and multi-digit ordinals paint digit ink and differ.
        let one = tray_icon_bgra_with_number(false, Some(1));
        let twelve = tray_icon_bgra_with_number(false, Some(12));
        assert_ne!(one, tray_icon_bgra(false));
        assert_ne!(twelve, one);
        let inked = |pixels: &[u8]| {
            pixels
                .as_chunks::<4>()
                .0
                .iter()
                .filter(|px| **px == DIGIT_INK)
                .count()
        };
        assert!(inked(&one) > 0);
        assert!(inked(&twelve) > inked(&one));
        // The warning badge survives underneath the number.
        let warned = tray_icon_bgra_with_number(true, Some(3));
        assert_eq!(pixel(&warned, 30, 30), WARN);
        assert_eq!(pixel(&warned, 27, 25), WARN_INK);
        assert_eq!(pixel(&warned, 0, 0), ACCENT);
        assert_ne!(pixel(&warned, 30, 30), pixel(&one, 30, 30));
    }

    #[test]
    fn icon_state_adds_once_and_clears_on_remove() {
        let mut state = TrayIconState::new();
        assert!(state.needs_add());
        state.on_added();
        assert!(!state.needs_add());
        // Duplicate adds are refused by the gate, never reissued.
        state.on_added();
        assert!(!state.needs_add());
        // Graceful stop removes; a fresh owner starts clean.
        state.on_removed();
        assert!(state.needs_add());
    }
}

#![forbid(unsafe_code)]
//! Validated persistent Windows settings plus the per-binding shortcut model.
//!
//! Portable policy only (no Win32 here): schema, strict validation,
//! atomic file store, canonical chord grammar, the KDE-parity binding
//! catalog, authentic/compatible presets, effective-chord resolution, and
//! the live-poll types the owner consumes on its ~100ms pump.
//!
//! KDE parity sources: `kwin/src/plan-adapter-entry.ts`
//! (`planShortcutCatalog`: focus/move letters plus separate arrow rows,
//! resize parameterized by direction plus inwards/outwards mode,
//! float/sticky/maximize/fullscreen toggles), `kwin/src/workspace-native.ts`
//! (`workspaceShortcutCatalog`: select `Meta+1..0`, send `Meta+Shift+1..0`
//! with shifted-symbol aliases), and `kwin/src/domain-gap.ts`
//! (`innerGap`/`outerGap` default 8 each, bounded 0..64).
//!
//! The file lives at `%LOCALAPPDATA%\omnitiler\settings.json`
//! (known-folder root, no session suffix). Reads are bounded; writes are
//! validated then atomically replaced with the existing Win32
//! `MoveFileExW` replace pattern (see [`crate::storage`]). A malformed or
//! version-unknown file never resets state: loading reports the error and
//! the live owner keeps its last-good values with a degraded status.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

/// Settings schema version. Older readers refuse newer versions outright;
/// newer readers refuse anything but known versions (no silent migration).
pub const SETTINGS_SCHEMA_VERSION: u32 = 1;
/// Settings file name under the per-user product directory.
pub const SETTINGS_FILE_NAME: &str = "settings.json";
/// Temporary suffix used during atomic replacement (same directory).
pub const SETTINGS_PENDING_SUFFIX: &str = "settings.json.pending";
/// Bounded read: files larger than this refuse instead of loading.
pub const SETTINGS_MAX_BYTES: u64 = 65_536;

/// KDE-parity gap defaults and bounds (`domain-gap.ts`).
pub const DEFAULT_INNER_GAP: i32 = 8;
pub const DEFAULT_OUTER_GAP: i32 = 8;
pub const MAX_GAP: i32 = 64;

/// Border defaults: KDE `activeborderconfig.kcfg` parity (width 3, gap 0,
/// theme on with the configured `#2a82da` fallback) except radius, which
/// defaults to 6 per 2026-10-11. The Windows
/// analogue of the Plasma highlight colour is the live system accent; the
/// configured colour wins only when theming is off or the accent query is
/// unavailable (see [`crate::active_border::effective_color`]).
pub const DEFAULT_BORDER_WIDTH: f64 = 3.0;
pub const DEFAULT_BORDER_GAP: f64 = 0.0;
pub const DEFAULT_BORDER_RADIUS: f64 = 6.0;
pub const DEFAULT_BORDER_COLOR_HEX: &str = "#2a82da";
pub const DEFAULT_BORDER_USE_THEME: bool = true;
pub const MAX_BORDER_WIDTH: f64 = 32.0;
pub const MAX_BORDER_GAP: f64 = 64.0;
pub const MAX_BORDER_RADIUS: f64 = 64.0;

/// Underlay defaults: KDE parity (`#662a82da`, extension -1 follows border).
pub const DEFAULT_UNDERLAY_COLOR_HEX: &str = "#662a82da";
pub const DEFAULT_UNDERLAY_EXTENSION: f64 = -1.0;
pub const MAX_UNDERLAY_EXTENSION: f64 = 32.0;

fn default_revision() -> u64 {
    1
}

fn default_true() -> bool {
    true
}

/// Closed store error vocabulary. `Invalid` carries the bounded reason; the
/// file bytes are never modified on any error path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsError {
    Io(String),
    TooLarge,
    Malformed,
    UnsupportedVersion,
    Invalid(String),
}

impl std::fmt::Display for SettingsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(_) => f.write_str("io-error"),
            Self::TooLarge => f.write_str("too-large"),
            Self::Malformed => f.write_str("malformed"),
            Self::UnsupportedVersion => f.write_str("unsupported-version"),
            Self::Invalid(reason) => write!(f, "invalid: {reason}"),
        }
    }
}

impl std::error::Error for SettingsError {}

/// R-MOV-03 same-axis move wire values (decisions 3.1/3.2, functional IDs
/// 2026-10-08). Exact tokens only: no aliases, no migration; unknown or
/// mistyped values refuse through the existing invalid-file/last-good path.
pub const SAME_AXIS_MOVE_GROUP: &str = "group-with-neighbor";
/// R-MOV-03 flat-swap alternative wire value.
pub const SAME_AXIS_MOVE_SWAP: &str = "swap-with-neighbor";
/// Functional label for the default (KCM parity).
pub const SAME_AXIS_MOVE_GROUP_LABEL: &str = "Group with neighbor";
/// Reference WM named by the default's tooltip (KCM parity: never a setting id).
pub const SAME_AXIS_MOVE_GROUP_TIP: &str = "COSMIC";
/// Functional label for the flat-swap alternative (KCM parity).
pub const SAME_AXIS_MOVE_SWAP_LABEL: &str = "Swap with neighbor";
/// Reference WMs named by the alternative's tooltip (KCM parity).
pub const SAME_AXIS_MOVE_SWAP_TIP: &str = "i3, sway";

/// R-MAX-06 born-maximized admission wire values (user 2026-10-11, option B,
/// cross-platform functional IDs `core.born_maximized`). Exact tokens only:
/// no aliases, no migration; unknown or mistyped values refuse through the
/// existing invalid-file/last-good path.
pub const BORN_MAXIMIZED_TILE: &str = "tile-on-launch";
/// R-MAX-06 keep-overlay alternative wire value.
pub const BORN_MAXIMIZED_OVERLAY: &str = "keep-maximized-overlay";
/// Functional label for the default (KCM parity).
pub const BORN_MAXIMIZED_TILE_LABEL: &str = "Tile on launch";
/// Reference WMs named by the default's tooltip (KCM parity: never a setting id).
pub const BORN_MAXIMIZED_TILE_TIP: &str = "bspwm, i3, xmonad, sway, qtile";
/// Functional label for the keep-overlay alternative (KCM parity).
pub const BORN_MAXIMIZED_OVERLAY_LABEL: &str = "Keep maximized overlay";
/// Reference WMs named by the alternative's tooltip (KCM parity).
pub const BORN_MAXIMIZED_OVERLAY_TIP: &str = "COSMIC, Hyprland";

/// R-SPC-04 D1 fixed-size admission predicate wire values (user 2026-10-08,
/// functional IDs delivered offline; G-05 per-axis-zero user 2026-10-10).
/// Exact tokens only: no aliases, no migration; unknown or mistyped values
/// refuse through the existing invalid-file/last-good path.
pub const FIXED_SIZE_BOTH_AXES: &str = "both-axes-fixed";
/// R-SPC-04 D1 either-axis alternative wire value.
pub const FIXED_SIZE_EITHER_AXIS: &str = "either-axis-fixed";
/// Functional label for the default (KCM parity).
pub const FIXED_SIZE_BOTH_AXES_LABEL: &str = "Width and height both fixed";
/// Reference WM named by the default's tooltip (KCM parity: never a setting id).
pub const FIXED_SIZE_BOTH_AXES_TIP: &str = "COSMIC";
/// Functional label for the either-axis alternative (KCM parity).
pub const FIXED_SIZE_EITHER_AXIS_LABEL: &str = "Width or height fixed";
/// Reference WMs named by the alternative's tooltip (KCM parity).
pub const FIXED_SIZE_EITHER_AXIS_TIP: &str = "Hyprland (Wayland), sway";

/// R-MAX-06 two-choice control rows: `(wire token, functional label, WM tooltip)`.
#[must_use]
pub const fn born_maximized_options() -> [(&'static str, &'static str, &'static str); 2] {
    [
        (
            BORN_MAXIMIZED_TILE,
            BORN_MAXIMIZED_TILE_LABEL,
            BORN_MAXIMIZED_TILE_TIP,
        ),
        (
            BORN_MAXIMIZED_OVERLAY,
            BORN_MAXIMIZED_OVERLAY_LABEL,
            BORN_MAXIMIZED_OVERLAY_TIP,
        ),
    ]
}

fn default_born_maximized() -> String {
    BORN_MAXIMIZED_TILE.to_owned()
}

/// R-MOV-03 two-choice control rows: `(wire token, functional label, WM tooltip)`.
#[must_use]
pub const fn same_axis_move_options() -> [(&'static str, &'static str, &'static str); 2] {
    [
        (
            SAME_AXIS_MOVE_GROUP,
            SAME_AXIS_MOVE_GROUP_LABEL,
            SAME_AXIS_MOVE_GROUP_TIP,
        ),
        (
            SAME_AXIS_MOVE_SWAP,
            SAME_AXIS_MOVE_SWAP_LABEL,
            SAME_AXIS_MOVE_SWAP_TIP,
        ),
    ]
}

fn default_same_axis_move() -> String {
    SAME_AXIS_MOVE_GROUP.to_owned()
}

/// R-SPC-04 D1 two-choice control rows: `(wire token, functional label, WM tooltip)`.
#[must_use]
pub const fn fixed_size_predicate_options() -> [(&'static str, &'static str, &'static str); 2] {
    [
        (
            FIXED_SIZE_BOTH_AXES,
            FIXED_SIZE_BOTH_AXES_LABEL,
            FIXED_SIZE_BOTH_AXES_TIP,
        ),
        (
            FIXED_SIZE_EITHER_AXIS,
            FIXED_SIZE_EITHER_AXIS_LABEL,
            FIXED_SIZE_EITHER_AXIS_TIP,
        ),
    ]
}

fn default_fixed_size_predicate() -> String {
    FIXED_SIZE_BOTH_AXES.to_owned()
}

/// Core tiling/visual/takeover settings. All values are validated on load
/// and on save; out-of-range values refuse instead of clamping.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CoreSettings {
    #[serde(default = "default_inner_gap")]
    pub inner_gap: i32,
    #[serde(default = "default_outer_gap")]
    pub outer_gap: i32,
    /// R-MOV-03 global same-axis move mode. Additive schema-v1 field: a
    /// missing value defaults to [`SAME_AXIS_MOVE_GROUP`]; any other value
    /// besides the two exact tokens refuses in [`validate_settings`].
    #[serde(default = "default_same_axis_move")]
    pub same_axis_move: String,
    /// R-MAX-06 born-maximized admission mode. Additive schema-v1 field: a
    /// missing value defaults to [`BORN_MAXIMIZED_TILE`]; any other value
    /// besides the two exact tokens refuses in [`validate_settings`].
    #[serde(default = "default_born_maximized")]
    pub born_maximized: String,
    /// R-SPC-04 D1 fixed-size admission predicate. Additive schema-v1 field:
    /// a missing value defaults to [`FIXED_SIZE_BOTH_AXES`]; any other value
    /// besides the two exact tokens refuses in [`validate_settings`].
    /// Changes apply to subsequent admissions only, never to retained
    /// classifications (D2).
    #[serde(default = "default_fixed_size_predicate")]
    pub fixed_size_predicate: String,
    #[serde(default)]
    pub border: BorderSettings,
    #[serde(default)]
    pub underlay: UnderlaySettings,
    #[serde(default)]
    pub keyboard: KeyboardSettings,
    #[serde(default)]
    pub mouse: MouseSettings,
    #[serde(default)]
    pub workspace: WorkspaceSettings,
}

fn default_inner_gap() -> i32 {
    DEFAULT_INNER_GAP
}

fn default_outer_gap() -> i32 {
    DEFAULT_OUTER_GAP
}

impl Default for CoreSettings {
    fn default() -> Self {
        Self {
            inner_gap: DEFAULT_INNER_GAP,
            outer_gap: DEFAULT_OUTER_GAP,
            same_axis_move: default_same_axis_move(),
            born_maximized: default_born_maximized(),
            fixed_size_predicate: default_fixed_size_predicate(),
            border: BorderSettings::default(),
            underlay: UnderlaySettings::default(),
            keyboard: KeyboardSettings::default(),
            mouse: MouseSettings::default(),
            workspace: WorkspaceSettings::default(),
        }
    }
}

impl CoreSettings {
    /// Typed R-MOV-03 mode for the next move. Live settings are always
    /// validated, so the fallback only covers hand-built defaults; invalid
    /// files never become live (they keep last-good instead).
    #[must_use]
    pub fn same_axis_move_mode(&self) -> tiler_core::directional::SameAxisMove {
        tiler_core::directional::SameAxisMove::parse_wire(&self.same_axis_move).unwrap_or_default()
    }

    /// Typed R-MAX-06 mode for the next first-seen maximized admission: true
    /// keeps the Q3 retained-slot overlay, false clears once then admits
    /// ordinarily. Live settings are always validated, so the fallback only
    /// covers hand-built defaults; invalid files never become live (they keep
    /// last-good instead).
    #[must_use]
    pub fn born_maximized_keep_overlay(&self) -> bool {
        self.born_maximized == BORN_MAXIMIZED_OVERLAY
    }

    /// Typed R-SPC-04 D1 predicate for subsequent fixed-size admissions.
    /// Shared policy only: the exact tokens parse through
    /// [`tiler_core::size_hints::FixedSizePredicate::parse_wire`], so
    /// per-axis-zero handling (G-05) is never copied here. Live settings are
    /// always validated, so the fallback only covers hand-built defaults;
    /// invalid files never become live (they keep last-good instead).
    #[must_use]
    pub fn fixed_size_predicate_mode(&self) -> tiler_core::size_hints::FixedSizePredicate {
        tiler_core::size_hints::FixedSizePredicate::parse_wire(&self.fixed_size_predicate)
            .unwrap_or_default()
    }
}

/// Active-border settings. Logical KDE points; the native layer scales at
/// the target DPI. Colour is `#rrggbb`; `use_theme` selects the live system
/// accent with the configured colour as fallback.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BorderSettings {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_border_width")]
    pub width: f64,
    #[serde(default)]
    pub gap: f64,
    #[serde(default = "default_border_radius")]
    pub radius: f64,
    #[serde(default = "default_border_color")]
    pub color: String,
    #[serde(default = "default_border_theme")]
    pub use_theme: bool,
}

fn default_border_width() -> f64 {
    DEFAULT_BORDER_WIDTH
}

fn default_border_radius() -> f64 {
    DEFAULT_BORDER_RADIUS
}

fn default_border_color() -> String {
    DEFAULT_BORDER_COLOR_HEX.to_owned()
}

fn default_border_theme() -> bool {
    DEFAULT_BORDER_USE_THEME
}

impl Default for BorderSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            width: DEFAULT_BORDER_WIDTH,
            gap: DEFAULT_BORDER_GAP,
            radius: DEFAULT_BORDER_RADIUS,
            color: DEFAULT_BORDER_COLOR_HEX.to_owned(),
            use_theme: DEFAULT_BORDER_USE_THEME,
        }
    }
}

/// Group-underlay settings. Colour is `#aarrggbb`; extension is the logical
/// outset beyond gap plus border width (-1 follows the border width).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnderlaySettings {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_underlay_color")]
    pub color: String,
    #[serde(default = "default_underlay_extension")]
    pub extension: f64,
}

fn default_underlay_color() -> String {
    DEFAULT_UNDERLAY_COLOR_HEX.to_owned()
}

fn default_underlay_extension() -> f64 {
    DEFAULT_UNDERLAY_EXTENSION
}

impl Default for UnderlaySettings {
    fn default() -> Self {
        Self {
            enabled: true,
            color: DEFAULT_UNDERLAY_COLOR_HEX.to_owned(),
            extension: DEFAULT_UNDERLAY_EXTENSION,
        }
    }
}

/// Keyboard takeover policy. `takeover` defaults on; `allow_win_l` is the
/// explicit opt-in for unshifted Win+L (default off). The physical OS lock
/// chord keeps its safety fence regardless of rebinding (see
/// [`is_lock_chord`]): no remap may target unshifted Win+L.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyboardSettings {
    #[serde(default = "default_true")]
    pub takeover: bool,
    #[serde(default)]
    pub allow_win_l: bool,
}

impl Default for KeyboardSettings {
    fn default() -> Self {
        Self {
            takeover: true,
            allow_win_l: false,
        }
    }
}

/// Mouse settings. Session-only Snap prevention defaults on; turning it off
/// restores the captured owned SPI preimage (existing ledger/conditional
/// write/readback path), never registry/policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MouseSettings {
    #[serde(default = "default_true")]
    pub snap_prevention: bool,
}

impl Default for MouseSettings {
    fn default() -> Self {
        Self {
            snap_prevention: true,
        }
    }
}

/// Managed-workspace settings. `default_tiled` (KDE `defaultTiled` parity,
/// initially true) seeds every newly created workspace; live edits affect
/// only workspaces created after the edit. Per-workspace session overrides
/// never persist.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceSettings {
    #[serde(default = "default_true")]
    pub default_tiled: bool,
}

impl Default for WorkspaceSettings {
    fn default() -> Self {
        Self {
            default_tiled: true,
        }
    }
}

/// Per-binding state. `Keep` uses the catalog default chord(s); `Disabled`
/// passes the chord through natively; `Rebind` replaces the default with a
/// validated canonical Win-family chord. The custom chord travels in
/// `chord` only for `Rebind`; any other combination refuses validation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BindingState {
    Keep,
    Disabled,
    Rebind,
}

/// One persisted binding override. Absent entries mean `Keep`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BindingSetting {
    pub state: BindingState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chord: Option<String>,
}

/// Full settings document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    pub v: u32,
    #[serde(default = "default_revision")]
    pub revision: u64,
    #[serde(default)]
    pub core: CoreSettings,
    /// Binding overrides by catalog action id. Absent means `Keep`.
    #[serde(default)]
    pub bindings: BTreeMap<String, BindingSetting>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            v: SETTINGS_SCHEMA_VERSION,
            revision: 1,
            core: CoreSettings::default(),
            bindings: BTreeMap::new(),
        }
    }
}

/// Parsed canonical chord: Win mandatory, optional Shift/Alt/Ctrl, one key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chord {
    pub shift: bool,
    pub alt: bool,
    pub ctrl: bool,
    pub vk: u32,
    pub key_name: &'static str,
}

/// Virtual-key constants shared with the classifier (letters/digits/F-keys
/// derive arithmetically; arrows/tab mirror `snapkey`).
pub const VK_LEFT: u32 = 37;
pub const VK_UP: u32 = 38;
pub const VK_RIGHT: u32 = 39;
pub const VK_DOWN: u32 = 40;
/// Digit base for the item 2 numbered-stay canonical slots (shared with the
/// numbered follow sends on the same digit).
pub const VK_0: u32 = 0x30;
/// Tab chord key for the item 1 previous-view toggle (Win+Ctrl+Tab).
pub const VK_TAB: u32 = 0x09;
/// Float/sticky shared virtual key: the only canonical target two rebound
/// rows may share (separate arm slots), every other shared canonical target
/// would collide in one slot and refuses in validation.
pub const VK_G: u32 = 0x47;

/// Map a canonical key name to its virtual key. Letters A-Z, digits 0-9,
/// F1-F24, the four arrows, and Tab. Anything else is malformed: chords may
/// only name known keys.
#[must_use]
pub fn vk_for_key_name(name: &str) -> Option<(u32, &'static str)> {
    if name.eq_ignore_ascii_case("tab") {
        return Some((VK_TAB, "Tab"));
    }
    const ARROWS: [(&str, u32); 4] = [
        ("Left", VK_LEFT),
        ("Up", VK_UP),
        ("Right", VK_RIGHT),
        ("Down", VK_DOWN),
    ];
    if name.len() == 1 {
        let byte = name.as_bytes()[0];
        if byte.is_ascii_alphabetic() {
            let upper = byte.to_ascii_uppercase();
            // Leak-free static names for A-Z.
            const LETTERS: [&str; 26] = [
                "A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P",
                "Q", "R", "S", "T", "U", "V", "W", "X", "Y", "Z",
            ];
            let index = usize::from(upper - b'A');
            return Some((u32::from(upper), LETTERS[index]));
        }
        if byte.is_ascii_digit() {
            const DIGITS: [&str; 10] = ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9"];
            let index = usize::from(byte - b'0');
            return Some((u32::from(byte), DIGITS[index]));
        }
        return None;
    }
    for (text, vk) in ARROWS {
        if name.eq_ignore_ascii_case(text) {
            return Some((vk, text));
        }
    }
    if name.len() >= 2
        && name.len() <= 3
        && (name.as_bytes()[0] == b'F' || name.as_bytes()[0] == b'f')
        && name[1..].bytes().all(|b| b.is_ascii_digit())
    {
        let number: u32 = name[1..].parse().ok()?;
        if (1..=24).contains(&number) {
            // Canonical F-key names live behind one static table.
            const FNAMES: [&str; 24] = [
                "F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F10", "F11", "F12", "F13",
                "F14", "F15", "F16", "F17", "F18", "F19", "F20", "F21", "F22", "F23", "F24",
            ];
            return Some((0x6F + number, FNAMES[number as usize - 1]));
        }
    }
    None
}

/// Parse one canonical textual chord (`Win[+Shift][+Alt][+Ctrl]+Key`,
/// case-insensitive, any token order). `Win` is mandatory and exactly one
/// known key is required; duplicate tokens refuse.
pub fn parse_chord(text: &str) -> Result<Chord, String> {
    let usage = "refuse: chord needs Win[+Shift][+Alt][+Ctrl]+Key".to_owned();
    let mut win = false;
    let mut shift = false;
    let mut alt = false;
    let mut ctrl = false;
    let mut key: Option<(u32, &'static str)> = None;
    let mut parts = 0u32;
    for raw in text.split('+') {
        let token = raw.trim();
        if token.is_empty() {
            return Err(usage);
        }
        parts += 1;
        if token.eq_ignore_ascii_case("win") {
            if win {
                return Err(usage);
            }
            win = true;
        } else if token.eq_ignore_ascii_case("shift") {
            if shift {
                return Err(usage);
            }
            shift = true;
        } else if token.eq_ignore_ascii_case("alt") {
            if alt {
                return Err(usage);
            }
            alt = true;
        } else if token.eq_ignore_ascii_case("ctrl") {
            if ctrl {
                return Err(usage);
            }
            ctrl = true;
        } else if key.is_none() && vk_for_key_name(token).is_some() {
            key = vk_for_key_name(token);
        } else {
            return Err(usage);
        }
    }
    if !win || parts < 2 {
        return Err(usage);
    }
    let Some((vk, key_name)) = key else {
        return Err(usage);
    };
    Ok(Chord {
        shift,
        alt,
        ctrl,
        vk,
        key_name,
    })
}

/// Render a parsed chord back to canonical order.
#[must_use]
pub fn render_chord(chord: &Chord) -> String {
    let mut out = String::from("Win");
    if chord.shift {
        out.push_str("+Shift");
    }
    if chord.alt {
        out.push_str("+Alt");
    }
    if chord.ctrl {
        out.push_str("+Ctrl");
    }
    out.push('+');
    out.push_str(chord.key_name);
    out
}

/// True for the physical OS lock chord: unshifted Win+L without Alt/Ctrl.
/// The low-level hook cannot reliably intercept it, so it always passes
/// through unless explicitly opted in, and no remap may ever target it
/// (no insecure bypass via remap).
#[must_use]
pub const fn is_lock_chord(chord: &Chord) -> bool {
    chord.vk == 0x4C && !chord.shift && !chord.alt && !chord.ctrl
}

/// Which runtime arm a catalog action belongs to. Directional/workspace arms
/// split on Shift; history arms split on Ctrl (item 1); toggles carry their
/// fixed polarity; resize arms split on Shift with Alt always held (outwards
/// unshifted Win+Alt, inwards Win+Shift+Alt).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindingFamily {
    Directional {
        direction: &'static str,
        op: SnapFamilyOp,
    },
    Workspace {
        index: u8,
        op: SnapFamilyOp,
    },
    /// Item 2 numbered send-and-stay: bindable, unbound by default (empty
    /// defaults). Keep means unbound, never Disabled/effective interception;
    /// rebinds ride the shifted digit arm through the stay action.
    WorkspaceStay {
        index: u8,
    },
    /// Item 2 relative send: previous/next ordinal step in the item 1 scoped
    /// ring (never MRU), resolved once before transfer. Follow rows carry
    /// Win+Ctrl+Shift defaults; stay rows are bindable unbound. `prev` picks
    /// the step direction; `follow` pins the explicit intent.
    WorkspaceSendRelative {
        prev: bool,
        follow: bool,
    },
    WorkspaceHistory {
        kind: WorkspaceHistoryKind,
    },
    Toggle {
        kind: ToggleKind,
    },
    Launch {
        kind: LaunchKind,
    },
    Resize {
        direction: &'static str,
        mode: &'static str,
    },
}

/// Item 1 history arm: `Previous` toggles the previous view (Win+Ctrl+Tab),
/// `Prev` steps to the previous ordinal, `Next` steps to the next ordinal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceHistoryKind {
    Previous,
    Prev,
    Next,
}

impl WorkspaceHistoryKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Previous => "previous",
            Self::Prev => "prev",
            Self::Next => "next",
        }
    }
}

/// Focus/move/select/send arm selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapFamilyOp {
    Primary,
    Shifted,
}

impl SnapFamilyOp {
    /// Native Shift polarity of this arm (unshifted focus/select vs shifted
    /// move/send). Rebinds must match it so the classifier's Shift-derived op
    /// keeps meaning what the binding says.
    #[must_use]
    pub const fn wants_shift(self) -> bool {
        match self {
            Self::Primary => false,
            Self::Shifted => true,
        }
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Primary => "primary",
            Self::Shifted => "shifted",
        }
    }
}

/// Toggle arm selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToggleKind {
    Float,
    Sticky,
    Maximize,
    Fullscreen,
    Orientation,
}

impl ToggleKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Float => "float",
            Self::Sticky => "sticky",
            Self::Maximize => "maximize",
            Self::Fullscreen => "fullscreen",
            Self::Orientation => "orientation",
        }
    }

    /// Native Shift polarity: only sticky rides the shifted G arm.
    #[must_use]
    pub const fn wants_shift(self) -> bool {
        match self {
            Self::Sticky => true,
            Self::Float | Self::Maximize | Self::Fullscreen | Self::Orientation => false,
        }
    }
}

/// Launcher arm selector: default browser (Win+B) or Windows Terminal
/// (Win+T). Both ride unshifted Win chords with no Ctrl/Alt; only the down
/// dispatches, ups close the pair, held repeats stay swallowed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchKind {
    Browser,
    Terminal,
}

impl LaunchKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Browser => "browser",
            Self::Terminal => "terminal",
        }
    }
}

/// One catalog row: the KDE action, its default chord(s), and the honest
/// Windows conflict/effect model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BindingDef {
    pub id: &'static str,
    pub text: &'static str,
    pub family: BindingFamily,
    /// Default chord(s). Directional rows carry exactly one chord each
    /// (letters and arrows are separate rows); resize rows carry two (the
    /// letter plus its arrow alias); every other row carries exactly one.
    pub defaults: &'static [&'static str],
    /// False only for rows the Windows hook never intercepts. All catalog
    /// rows (including resize) are currently implemented.
    pub implemented: bool,
    /// Truthful per-physical-chord conflict note, if any.
    pub conflict: Option<&'static str>,
}

/// Directional focus/move rows: one row per physical chord (KDE
/// `planShortcutCatalog` parity: `focus-<dir>` on the letter plus a separate
/// `focus-<dir>-arrow` row on the arrow, same for move). Splitting keeps the
/// per-binding user model truthful: the letter and arrow aliases ride
/// different OS conflicts, so disabling or rebinding one never touches the
/// other. Shift selects the move arm.
const FOCUS_MOVE_ROWS: [(&str, &str, &str, &[&str], SnapFamilyOp); 16] = [
    (
        "focus-left",
        "Focus window left",
        "left",
        &["Win+H"],
        SnapFamilyOp::Primary,
    ),
    (
        "focus-left-arrow",
        "Focus window left",
        "left",
        &["Win+Left"],
        SnapFamilyOp::Primary,
    ),
    (
        "focus-down",
        "Focus window down",
        "down",
        &["Win+J"],
        SnapFamilyOp::Primary,
    ),
    (
        "focus-down-arrow",
        "Focus window down",
        "down",
        &["Win+Down"],
        SnapFamilyOp::Primary,
    ),
    (
        "focus-up",
        "Focus window up",
        "up",
        &["Win+K"],
        SnapFamilyOp::Primary,
    ),
    (
        "focus-up-arrow",
        "Focus window up",
        "up",
        &["Win+Up"],
        SnapFamilyOp::Primary,
    ),
    (
        "focus-right",
        "Focus window right",
        "right",
        &["Win+L"],
        SnapFamilyOp::Primary,
    ),
    (
        "focus-right-arrow",
        "Focus window right",
        "right",
        &["Win+Right"],
        SnapFamilyOp::Primary,
    ),
    (
        "move-left",
        "Move window left",
        "left",
        &["Win+Shift+H"],
        SnapFamilyOp::Shifted,
    ),
    (
        "move-left-arrow",
        "Move window left",
        "left",
        &["Win+Shift+Left"],
        SnapFamilyOp::Shifted,
    ),
    (
        "move-down",
        "Move window down",
        "down",
        &["Win+Shift+J"],
        SnapFamilyOp::Shifted,
    ),
    (
        "move-down-arrow",
        "Move window down",
        "down",
        &["Win+Shift+Down"],
        SnapFamilyOp::Shifted,
    ),
    (
        "move-up",
        "Move window up",
        "up",
        &["Win+Shift+K"],
        SnapFamilyOp::Shifted,
    ),
    (
        "move-up-arrow",
        "Move window up",
        "up",
        &["Win+Shift+Up"],
        SnapFamilyOp::Shifted,
    ),
    (
        "move-right",
        "Move window right",
        "right",
        &["Win+Shift+L"],
        SnapFamilyOp::Shifted,
    ),
    (
        "move-right-arrow",
        "Move window right",
        "right",
        &["Win+Shift+Right"],
        SnapFamilyOp::Shifted,
    ),
];

/// Resize rows: KDE `Meta+Alt+key` (grow/outwards) and
/// `Meta+Alt+Shift+key` (shrink/inwards) plus arrow aliases. Intercepted on
/// Windows through the dedicated resize classifier arm (never a move):
/// each row carries both its letter and arrow defaults (8 logical rows,
/// 16 physical chords). Windows ownership of these chords is unverified in
/// the repository, so both presets keep the defaults; containment is
/// unproven live.
const RESIZE_ROWS: [(&str, &str, &str, &str, &[&str]); 8] = [
    (
        "resize-out-left",
        "Grow window towards left",
        "left",
        "outwards",
        &["Win+Alt+H", "Win+Alt+Left"],
    ),
    (
        "resize-out-down",
        "Grow window towards down",
        "down",
        "outwards",
        &["Win+Alt+J", "Win+Alt+Down"],
    ),
    (
        "resize-out-up",
        "Grow window towards up",
        "up",
        "outwards",
        &["Win+Alt+K", "Win+Alt+Up"],
    ),
    (
        "resize-out-right",
        "Grow window towards right",
        "right",
        "outwards",
        &["Win+Alt+L", "Win+Alt+Right"],
    ),
    (
        "resize-in-left",
        "Shrink window from left",
        "left",
        "inwards",
        &["Win+Alt+Shift+H", "Win+Alt+Shift+Left"],
    ),
    (
        "resize-in-down",
        "Shrink window from down",
        "down",
        "inwards",
        &["Win+Alt+Shift+J", "Win+Alt+Shift+Down"],
    ),
    (
        "resize-in-up",
        "Shrink window from up",
        "up",
        "inwards",
        &["Win+Alt+Shift+K", "Win+Alt+Shift+Up"],
    ),
    (
        "resize-in-right",
        "Shrink window from right",
        "right",
        "inwards",
        &["Win+Alt+Shift+L", "Win+Alt+Shift+Right"],
    ),
];

/// Honest Windows conflict note for keyboard resize chords: ownership is
/// unverified in the repository and containment is unproven live. Never a
/// stock-holder or conflict-free claim.
pub const RESIZE_CONFLICT: &str =
    "Windows shortcut ownership unverified in repository; containment unproven live";

/// Build the full binding catalog: directional focus/move (one row per
/// physical chord: letters plus separate arrow rows), resize (8 logical
/// rows, each with its letter plus arrow defaults, intercepted through the
/// dedicated resize arm), float/sticky/maximize/fullscreen toggles, and
/// workspace select/send digits 0-9.
#[must_use]
pub fn binding_catalog() -> Vec<BindingDef> {
    let mut rows = Vec::new();
    for (id, text, direction, defaults, op) in FOCUS_MOVE_ROWS {
        rows.push(BindingDef {
            id,
            text,
            family: BindingFamily::Directional { direction, op },
            defaults,
            implemented: true,
            conflict: directional_conflict(id),
        });
    }
    for (id, text, direction, mode, defaults) in RESIZE_ROWS {
        rows.push(BindingDef {
            id,
            text,
            family: BindingFamily::Resize { direction, mode },
            defaults,
            implemented: true,
            conflict: Some(RESIZE_CONFLICT),
        });
    }
    rows.push(BindingDef {
        id: "toggle-float",
        text: "Toggle floating window",
        family: BindingFamily::Toggle {
            kind: ToggleKind::Float,
        },
        defaults: &["Win+G"],
        implemented: true,
        conflict: chord_conflict("Win+G"),
    });
    rows.push(BindingDef {
        id: "toggle-sticky",
        text: "Toggle sticky floating window",
        family: BindingFamily::Toggle {
            kind: ToggleKind::Sticky,
        },
        defaults: &["Win+Shift+G"],
        implemented: true,
        conflict: chord_conflict("Win+Shift+G"),
    });
    rows.push(BindingDef {
        id: "toggle-maximize",
        text: "Toggle maximize window",
        family: BindingFamily::Toggle {
            kind: ToggleKind::Maximize,
        },
        defaults: &["Win+M"],
        implemented: true,
        conflict: chord_conflict("Win+M"),
    });
    rows.push(BindingDef {
        id: "toggle-fullscreen",
        text: "Toggle fullscreen window",
        family: BindingFamily::Toggle {
            kind: ToggleKind::Fullscreen,
        },
        defaults: &["Win+F11"],
        implemented: true,
        conflict: chord_conflict("Win+F11"),
    });
    rows.push(BindingDef {
        id: "toggle-orientation",
        text: "Toggle parent split orientation",
        family: BindingFamily::Toggle {
            kind: ToggleKind::Orientation,
        },
        defaults: &["Win+O"],
        implemented: true,
        conflict: chord_conflict("Win+O"),
    });
    rows.push(BindingDef {
        id: "launch-browser",
        text: "Open default browser",
        family: BindingFamily::Launch {
            kind: LaunchKind::Browser,
        },
        defaults: &["Win+B"],
        implemented: true,
        conflict: chord_conflict("Win+B"),
    });
    rows.push(BindingDef {
        id: "launch-terminal",
        text: "Open Windows Terminal",
        family: BindingFamily::Launch {
            kind: LaunchKind::Terminal,
        },
        defaults: &["Win+T"],
        implemented: true,
        conflict: chord_conflict("Win+T"),
    });
    for index in 1..=9u8 {
        rows.push(workspace_row(index, SnapFamilyOp::Primary));
    }
    rows.push(workspace_row(0, SnapFamilyOp::Primary));
    for index in 1..=9u8 {
        rows.push(workspace_row(index, SnapFamilyOp::Shifted));
    }
    rows.push(workspace_row(0, SnapFamilyOp::Shifted));
    // Item 2 numbered send-and-stay rows: bindable, unbound by default.
    // Keep means unbound (never Disabled/effective interception); the shifted
    // digit arm plus the explicit stay action route rebinds.
    for index in 1..=9u8 {
        rows.push(BindingDef {
            id: stay_id(index),
            text: "Move window to workspace without following",
            family: BindingFamily::WorkspaceStay { index },
            defaults: &[],
            implemented: true,
            conflict: None,
        });
    }
    rows.push(BindingDef {
        id: stay_id(0),
        text: "Move window to a newly appended workspace without following",
        family: BindingFamily::WorkspaceStay { index: 0 },
        defaults: &[],
        implemented: true,
        conflict: None,
    });
    // Item 2 relative sends: previous/next ordinal step in the item 1 scoped
    // ring. Follow rows carry Win+Ctrl+Shift defaults; stay rows are
    // bindable unbound. Windows Ctrl+Shift arrow ownership is UNKNOWN:
    // defaults keep pending evidenced conflicts with the honest
    // ownership-unknown note (no new Compatible disables).
    for (id, text, prev, follow, chord) in RELATIVE_SEND_ROWS {
        rows.push(BindingDef {
            id,
            text,
            family: BindingFamily::WorkspaceSendRelative { prev, follow },
            defaults: chord,
            implemented: true,
            conflict: if chord.is_empty() {
                None
            } else {
                chord_conflict(chord[0])
            },
        });
    }
    // Item 1 history rows: one Win+Ctrl+Tab previous-view toggle plus eight
    // Win+Ctrl relative steps (H/K/Left/Up previous, J/L/Down/Right next).
    // Authentic keeps all nine; Compatible disables only Left/Right (native
    // virtual-desktop ownership, unverified in repo); Tab/letters/Up/Down
    // carry the honest ownership-unknown note.
    for (id, text, kind, chord) in HISTORY_ROWS {
        rows.push(BindingDef {
            id,
            text,
            family: BindingFamily::WorkspaceHistory { kind },
            defaults: chord,
            implemented: true,
            conflict: chord_conflict(chord[0]),
        });
    }
    rows
}

/// Item 1 history catalog rows: toggle plus four previous and four next
/// ordinals. Separate rows keep the per-binding model truthful: arrows and
/// letters ride different OS conflicts, so disabling or rebinding one never
/// touches the other.
const HISTORY_ROWS: [(&str, &str, WorkspaceHistoryKind, &[&str]); 9] = [
    (
        "workspace-previous",
        "Toggle previous workspace",
        WorkspaceHistoryKind::Previous,
        &["Win+Ctrl+Tab"],
    ),
    (
        "workspace-prev-h",
        "Previous workspace",
        WorkspaceHistoryKind::Prev,
        &["Win+Ctrl+H"],
    ),
    (
        "workspace-prev-k",
        "Previous workspace",
        WorkspaceHistoryKind::Prev,
        &["Win+Ctrl+K"],
    ),
    (
        "workspace-prev-left-arrow",
        "Previous workspace",
        WorkspaceHistoryKind::Prev,
        &["Win+Ctrl+Left"],
    ),
    (
        "workspace-prev-up-arrow",
        "Previous workspace",
        WorkspaceHistoryKind::Prev,
        &["Win+Ctrl+Up"],
    ),
    (
        "workspace-next-j",
        "Next workspace",
        WorkspaceHistoryKind::Next,
        &["Win+Ctrl+J"],
    ),
    (
        "workspace-next-l",
        "Next workspace",
        WorkspaceHistoryKind::Next,
        &["Win+Ctrl+L"],
    ),
    (
        "workspace-next-down-arrow",
        "Next workspace",
        WorkspaceHistoryKind::Next,
        &["Win+Ctrl+Down"],
    ),
    (
        "workspace-next-right-arrow",
        "Next workspace",
        WorkspaceHistoryKind::Next,
        &["Win+Ctrl+Right"],
    ),
];

/// Item 2 numbered stay ids: ten bindable unbound rows mirroring the
/// numbered follow sends.
fn stay_id(index: u8) -> &'static str {
    match index {
        1 => "stay-workspace-1",
        2 => "stay-workspace-2",
        3 => "stay-workspace-3",
        4 => "stay-workspace-4",
        5 => "stay-workspace-5",
        6 => "stay-workspace-6",
        7 => "stay-workspace-7",
        8 => "stay-workspace-8",
        9 => "stay-workspace-9",
        _ => "stay-workspace-0",
    }
}

/// Item 2 relative-send catalog rows: four previous + four next follow rows
/// with Win+Ctrl+Shift defaults, plus eight matching stay rows bindable
/// unbound. Separate rows keep the per-binding model truthful: rebinding one
/// never touches the other, and arrows/letters ride the honest
/// ownership-unknown note (no new Compatible disables).
const RELATIVE_SEND_ROWS: [(&str, &str, bool, bool, &[&str]); 16] = [
    (
        "send-prev-h",
        "Move window to previous workspace",
        true,
        true,
        &["Win+Ctrl+Shift+H"],
    ),
    (
        "send-prev-k",
        "Move window to previous workspace",
        true,
        true,
        &["Win+Ctrl+Shift+K"],
    ),
    (
        "send-prev-left-arrow",
        "Move window to previous workspace",
        true,
        true,
        &["Win+Ctrl+Shift+Left"],
    ),
    (
        "send-prev-up-arrow",
        "Move window to previous workspace",
        true,
        true,
        &["Win+Ctrl+Shift+Up"],
    ),
    (
        "send-next-j",
        "Move window to next workspace",
        false,
        true,
        &["Win+Ctrl+Shift+J"],
    ),
    (
        "send-next-l",
        "Move window to next workspace",
        false,
        true,
        &["Win+Ctrl+Shift+L"],
    ),
    (
        "send-next-down-arrow",
        "Move window to next workspace",
        false,
        true,
        &["Win+Ctrl+Shift+Down"],
    ),
    (
        "send-next-right-arrow",
        "Move window to next workspace",
        false,
        true,
        &["Win+Ctrl+Shift+Right"],
    ),
    (
        "send-stay-prev-h",
        "Move window to previous workspace without following",
        true,
        false,
        &[],
    ),
    (
        "send-stay-prev-k",
        "Move window to previous workspace without following",
        true,
        false,
        &[],
    ),
    (
        "send-stay-prev-left-arrow",
        "Move window to previous workspace without following",
        true,
        false,
        &[],
    ),
    (
        "send-stay-prev-up-arrow",
        "Move window to previous workspace without following",
        true,
        false,
        &[],
    ),
    (
        "send-stay-next-j",
        "Move window to next workspace without following",
        false,
        false,
        &[],
    ),
    (
        "send-stay-next-l",
        "Move window to next workspace without following",
        false,
        false,
        &[],
    ),
    (
        "send-stay-next-down-arrow",
        "Move window to next workspace without following",
        false,
        false,
        &[],
    ),
    (
        "send-stay-next-right-arrow",
        "Move window to next workspace without following",
        false,
        false,
        &[],
    ),
];
fn workspace_row(index: u8, op: SnapFamilyOp) -> BindingDef {
    // Static per-index rows (KDE `workspaceShortcutCatalog` parity: select
    // `Meta+1..0`, send `Meta+Shift+1..0`). Shifted US symbols share the
    // digit virtual key, so they act as the send arm, never a separate row.
    // Conflict comes from the row's own default chord (taskbar owns digits).
    macro_rules! row {
        ($id:expr, $text:expr, $chord:expr) => {
            BindingDef {
                id: $id,
                text: $text,
                family: BindingFamily::Workspace { index, op },
                defaults: $chord,
                implemented: true,
                conflict: chord_conflict($chord[0]),
            }
        };
    }
    match (index, op) {
        (1, SnapFamilyOp::Primary) => row!("workspace-select-1", "Focus workspace 1", &["Win+1"]),
        (2, SnapFamilyOp::Primary) => row!("workspace-select-2", "Focus workspace 2", &["Win+2"]),
        (3, SnapFamilyOp::Primary) => row!("workspace-select-3", "Focus workspace 3", &["Win+3"]),
        (4, SnapFamilyOp::Primary) => row!("workspace-select-4", "Focus workspace 4", &["Win+4"]),
        (5, SnapFamilyOp::Primary) => row!("workspace-select-5", "Focus workspace 5", &["Win+5"]),
        (6, SnapFamilyOp::Primary) => row!("workspace-select-6", "Focus workspace 6", &["Win+6"]),
        (7, SnapFamilyOp::Primary) => row!("workspace-select-7", "Focus workspace 7", &["Win+7"]),
        (8, SnapFamilyOp::Primary) => row!("workspace-select-8", "Focus workspace 8", &["Win+8"]),
        (9, SnapFamilyOp::Primary) => row!("workspace-select-9", "Focus workspace 9", &["Win+9"]),
        (0, SnapFamilyOp::Primary) => row!(
            "workspace-select-0",
            "Focus or create the trailing empty workspace",
            &["Win+0"]
        ),
        (1, SnapFamilyOp::Shifted) => row!(
            "workspace-send-1",
            "Move window to workspace 1",
            &["Win+Shift+1"]
        ),
        (2, SnapFamilyOp::Shifted) => row!(
            "workspace-send-2",
            "Move window to workspace 2",
            &["Win+Shift+2"]
        ),
        (3, SnapFamilyOp::Shifted) => row!(
            "workspace-send-3",
            "Move window to workspace 3",
            &["Win+Shift+3"]
        ),
        (4, SnapFamilyOp::Shifted) => row!(
            "workspace-send-4",
            "Move window to workspace 4",
            &["Win+Shift+4"]
        ),
        (5, SnapFamilyOp::Shifted) => row!(
            "workspace-send-5",
            "Move window to workspace 5",
            &["Win+Shift+5"]
        ),
        (6, SnapFamilyOp::Shifted) => row!(
            "workspace-send-6",
            "Move window to workspace 6",
            &["Win+Shift+6"]
        ),
        (7, SnapFamilyOp::Shifted) => row!(
            "workspace-send-7",
            "Move window to workspace 7",
            &["Win+Shift+7"]
        ),
        (8, SnapFamilyOp::Shifted) => row!(
            "workspace-send-8",
            "Move window to workspace 8",
            &["Win+Shift+8"]
        ),
        (9, SnapFamilyOp::Shifted) => row!(
            "workspace-send-9",
            "Move window to workspace 9",
            &["Win+Shift+9"]
        ),
        (_, SnapFamilyOp::Shifted) => row!(
            "workspace-send-0",
            "Move window to a newly appended workspace",
            &["Win+Shift+0"]
        ),
        (_, SnapFamilyOp::Primary) => row!(
            "workspace-select-0",
            "Focus or create the trailing empty workspace",
            &["Win+0"]
        ),
    }
}

fn directional_conflict(id: &str) -> Option<&'static str> {
    let chord = match id {
        "focus-left" => "Win+H",
        "focus-left-arrow" => "Win+Left",
        "focus-down" => "Win+J",
        "focus-down-arrow" => "Win+Down",
        "focus-up" => "Win+K",
        "focus-up-arrow" => "Win+Up",
        "focus-right" => "Win+L",
        "focus-right-arrow" => "Win+Right",
        "move-left" => "Win+Shift+H",
        "move-left-arrow" => "Win+Shift+Left",
        "move-down" => "Win+Shift+J",
        "move-down-arrow" => "Win+Shift+Down",
        "move-up" => "Win+Shift+K",
        "move-up-arrow" => "Win+Shift+Up",
        "move-right" => "Win+Shift+L",
        "move-right-arrow" => "Win+Shift+Right",
        _ => return None,
    };
    chord_conflict(chord)
}

/// OS conflict for one physical Win-family chord, for the settings UI: what
/// the OS owns at the rebound chord, not the stale original. Documented
/// owners follow the official Microsoft "Keyboard shortcuts in Windows" list
/// (Windows 11 tab; Windows 10 tab for the Win+U Ease-of-Access origin).
/// Chords the list does not document carry the honest unverified note instead
/// of a definitive clean claim; unparsable chords report `None` (not
/// applicable). Win+Alt chords (keyboard resize) report the honest
/// ownership-unknown note: Windows Alt-chord ownership is unverified in the
/// repository and containment is unproven live, never a stock-holder or
/// conflict-free claim.
/// Unshifted Win+Ctrl chords (item 1 history) report the recorded
/// virtual-desktop note for Left/Right and the honest ownership-unknown note
/// for Tab/letters/Up/Down, never a stock-holder or conflict-free claim.
/// Shifted Win+Ctrl chords (item 2 relative sends) report the honest
/// ownership-unknown note: Windows Ctrl+Shift arrow ownership is UNKNOWN
/// (decision 2.1), no new Compatible disables are selected, and containment
/// is unproven live. Only containment with live trace evidence claims
/// containment (Game Bar, Xbox mode); every other kept chord honestly
/// reports override-needs-takeover with containment unproven.
#[must_use]
pub fn chord_conflict(text: &str) -> Option<&'static str> {
    let chord = parse_chord(text).ok()?;
    if chord.alt {
        // Keyboard resize arm (Win+Alt / Win+Shift+Alt, no Ctrl): Windows
        // Alt-chord ownership is unverified in the repository; containment
        // is unproven live. Ctrl+Alt chords also land here with the same
        // honest note rather than a definitive claim.
        return Some(RESIZE_CONFLICT);
    }
    if chord.ctrl {
        // Item 2 relative-send arm: shifted Win+Ctrl. Windows Ctrl+Shift
        // arrow ownership is unknown; keep defaults pending evidenced
        // conflicts with the honest unknown-ownership text (no new
        // Compatible disables, no invented holder claims).
        if chord.shift {
            return Some("Windows shortcut ownership unknown; containment unproven live");
        }
        return Some(match chord.vk {
            VK_LEFT | VK_RIGHT => {
                "Windows virtual desktop switch (ownership unverified in repository); override needs takeover, containment unproven live"
            }
            VK_TAB | 0x48 | 0x4A | 0x4B | 0x4C | VK_UP | VK_DOWN => {
                "No documented conflict in this list; other apps may bind it (Windows virtual-desktop ownership unverified in repository)"
            }
            _ => {
                "No documented conflict in this list; other apps may bind it (Windows virtual-desktop ownership unverified in repository)"
            }
        });
    }
    match (chord.vk, chord.shift) {
        (0x41, false) => Some("Action Center owns Win+A; override needs takeover"),
        (0x42, false) => Some("notification area focus owns Win+B; override needs takeover"),
        (0x43, false) => Some(
            "Copilot owns Win+C (search when Copilot unavailable); override needs takeover, containment unproven live",
        ),
        (0x44, false) => Some("desktop show/hide owns Win+D; override needs takeover"),
        (0x45, false) => Some("File Explorer owns Win+E; override needs takeover"),
        (0x46, false) => Some("Feedback Hub owns Win+F; override needs takeover"),
        (0x48, false) => {
            Some("Voice dictation owns Win+H; override needs takeover, containment unproven live")
        }
        (0x49, false) => Some("Settings owns Win+I; override needs takeover"),
        (0x4A, false) => Some(
            "Recall owns Win+J on supported devices; override needs takeover, containment unproven live",
        ),
        (0x4B, false) => {
            Some("Cast owns Win+K; override needs takeover, containment unproven live")
        }
        (0x4C, false) => Some(
            "OS lock owns unshifted Win+L: the hook cannot reliably intercept it; needs explicit opt-in",
        ),
        (0x4D, false) => Some("minimize-all owns Win+M; override needs takeover"),
        (0x4E, false) => Some("notification center owns Win+N; override needs takeover"),
        (0x4F, false) => Some("orientation lock owns Win+O; override needs takeover"),
        (0x50, false) => Some("project mode owns Win+P; override needs takeover"),
        (0x51, false) => Some("search owns Win+Q; override needs takeover"),
        (0x52, false) => Some("Run dialog owns Win+R; override needs takeover"),
        (0x53, false) => Some("search owns Win+S; override needs takeover"),
        (0x54, false) => Some("taskbar cycle owns Win+T; override needs takeover"),
        (0x55, false) => Some(
            "Accessibility settings owns Win+U; override needs takeover, containment unproven live",
        ),
        (0x56, false) => Some("clipboard history owns Win+V; override needs takeover"),
        (0x57, false) => Some("Widgets owns Win+W; override needs takeover"),
        (0x58, false) => Some("Quick Link menu owns Win+X; override needs takeover"),
        (0x59, false) => Some("Mixed Reality owns Win+Y; override needs takeover"),
        (0x5A, false) => Some("snap layouts owns Win+Z; override needs takeover"),
        (VK_LEFT, false) => Some("Snap owns Win+Left; override needs takeover"),
        (VK_RIGHT, false) => Some("Snap owns Win+Right; override needs takeover"),
        (VK_UP, false) => Some("native maximize owns Win+Up; override needs takeover"),
        (VK_DOWN, false) => Some("native minimize owns Win+Down; override needs takeover"),
        (VK_LEFT, true) | (VK_RIGHT, true) => {
            Some("move between monitors owns Win+Shift+Left/Right; override needs takeover")
        }
        (VK_UP, true) | (VK_DOWN, true) => {
            Some("stretch vertically / restore owns Win+Shift+Up/Down; override needs takeover")
        }
        (0x41, true) => Some(
            "Windows tip focus owns Win+Shift+A; override needs takeover, containment unproven live",
        ),
        (0x47, false) => {
            Some("Xbox Game Bar owns Win+G; the hook cannot fully contain the OS override")
        }
        (0x4D, true) => Some("restore minimized owns Win+Shift+M; override needs takeover"),
        (0x52, true) => Some("Snipping screen recording owns Win+Shift+R; override needs takeover"),
        (0x53, true) => Some("Snipping screenshot owns Win+Shift+S; override needs takeover"),
        (0x54, true) => Some("previous taskbar app owns Win+Shift+T; override needs takeover"),
        (0x56, true) => Some("notifications cycle owns Win+Shift+V; override needs takeover"),
        (0x7A, false) => Some("Xbox mode owns Win+F11; containment is incomplete"),
        (vk, false) if (0x30..=0x39).contains(&vk) => {
            Some("taskbar launch owns Win+digits; override needs takeover")
        }
        (vk, true) if (0x30..=0x39).contains(&vk) => {
            Some("taskbar new instance owns Win+Shift+digits; override needs takeover")
        }
        _ => Some("No documented conflict in this list; other apps may bind it"),
    }
}

/// Native Shift polarity of one catalog binding: the Shift state the
/// classifier must observe for the chord to dispatch this action.
#[must_use]
pub fn binding_wants_shift(def: &BindingDef) -> bool {
    match def.family {
        BindingFamily::Directional { op, .. } | BindingFamily::Workspace { op, .. } => {
            op.wants_shift()
        }
        BindingFamily::WorkspaceStay { .. } | BindingFamily::WorkspaceSendRelative { .. } => true,
        BindingFamily::WorkspaceHistory { .. } => false,
        BindingFamily::Toggle { kind } => kind.wants_shift(),
        BindingFamily::Launch { .. } => false,
        BindingFamily::Resize { mode, .. } => mode == "inwards",
    }
}

/// Native Ctrl polarity of one catalog binding: item 1 history arms ride
/// Win+Ctrl; item 2 relative sends ride Win+Ctrl+Shift; every other
/// implemented arm rides without Ctrl by default. Numbered stay additionally
/// accepts the Win+Ctrl+Shift backlog arm via [`binding_modifiers_ok`]; this
/// default stays Ctrl-free so plain Win+Shift stay rebinds keep working.
/// Carried through routing, duplicate detection, canonical hold slots, and
/// release pins so unbound stay actions reuse the plumbing without rework.
#[must_use]
pub fn binding_wants_ctrl(def: &BindingDef) -> bool {
    matches!(
        def.family,
        BindingFamily::WorkspaceHistory { .. } | BindingFamily::WorkspaceSendRelative { .. }
    )
}

/// Whether a parsed chord's modifiers match one of the binding's native
/// arms. Every binding rides exactly its [`binding_wants_shift`] /
/// [`binding_wants_ctrl`] / [`binding_wants_alt`] arm, except numbered stay,
/// which additionally accepts the Win+Ctrl+Shift backlog example arm
/// (Shift required, Alt never). Digit follow and all other arms are
/// untouched: Ctrl stays refused there.
#[must_use]
pub fn binding_modifiers_ok(def: &BindingDef, shift: bool, ctrl: bool, alt: bool) -> bool {
    if shift == binding_wants_shift(def)
        && ctrl == binding_wants_ctrl(def)
        && alt == binding_wants_alt(def)
    {
        return true;
    }
    matches!(def.family, BindingFamily::WorkspaceStay { .. }) && shift && ctrl && !alt
}

/// Native modifier-arm text for rebind errors: the single arm, or both stay
/// arms (`Win+Shift or Win+Ctrl+Shift`).
#[must_use]
pub fn binding_arm_text(def: &BindingDef) -> &'static str {
    if matches!(def.family, BindingFamily::WorkspaceStay { .. }) {
        return "Win+Shift or Win+Ctrl+Shift";
    }
    match (
        binding_wants_shift(def),
        binding_wants_ctrl(def),
        binding_wants_alt(def),
    ) {
        (true, false, false) => "Win+Shift",
        (false, true, false) => "Win+Ctrl",
        (true, true, false) => "Win+Ctrl+Shift",
        (false, false, false) => "Win without Shift",
        (false, false, true) => "Win+Alt",
        (true, false, true) => "Win+Shift+Alt",
        _ => "the binding's modifier arm",
    }
}

/// Native Alt polarity of one catalog binding: keyboard resize arms ride
/// Win+Alt (outwards) and Win+Shift+Alt (inwards); every other arm rides
/// without Alt. Carried explicitly so later output-follow arms (Ctrl+Alt)
/// slot in without rework.
#[must_use]
pub const fn binding_wants_alt(def: &BindingDef) -> bool {
    matches!(def.family, BindingFamily::Resize { .. })
}

/// Explicit classifier action for one catalog binding: VK alone cannot
/// identify focus vs relative select vs relative follow vs output follow
/// arms sharing one key, so the action pins the arm. History toggle/prev/
/// next ride distinct actions; resize outwards/inwards ride distinct resize
/// actions carrying the mode (never a directional move); existing arms keep
/// theirs.
#[must_use]
pub fn binding_action(def: &BindingDef) -> crate::snapkey::ChordAction {
    use crate::snapkey::ChordAction;
    match def.family {
        BindingFamily::Directional { .. } => ChordAction::Directional,
        BindingFamily::Resize { mode, .. } => {
            if mode == "inwards" {
                ChordAction::ResizeIn
            } else {
                ChordAction::ResizeOut
            }
        }
        BindingFamily::Workspace { .. } => ChordAction::WorkspaceDigit,
        BindingFamily::WorkspaceStay { .. } => ChordAction::WorkspaceStayDigit,
        BindingFamily::WorkspaceSendRelative { prev, follow } => match (prev, follow) {
            (true, true) => ChordAction::WorkspaceSendPrev,
            (false, true) => ChordAction::WorkspaceSendNext,
            (true, false) => ChordAction::WorkspaceSendStayPrev,
            (false, false) => ChordAction::WorkspaceSendStayNext,
        },
        BindingFamily::WorkspaceHistory { kind } => match kind {
            WorkspaceHistoryKind::Previous => ChordAction::WorkspacePrevious,
            WorkspaceHistoryKind::Prev => ChordAction::WorkspacePrev,
            WorkspaceHistoryKind::Next => ChordAction::WorkspaceNext,
        },
        BindingFamily::Toggle { kind } => match kind {
            ToggleKind::Float => ChordAction::Float,
            ToggleKind::Sticky => ChordAction::Sticky,
            ToggleKind::Maximize => ChordAction::Maximize,
            ToggleKind::Fullscreen => ChordAction::Fullscreen,
            ToggleKind::Orientation => ChordAction::Orientation,
        },
        BindingFamily::Launch { kind } => match kind {
            LaunchKind::Browser => ChordAction::LaunchBrowser,
            LaunchKind::Terminal => ChordAction::LaunchTerminal,
        },
    }
}

/// Canonical virtual key of one catalog binding (first default chord).
/// History rows carry their canonical Tab/letter/arrow VK. Resize rows carry
/// their letter VK (the arrow alias shares the direction through its own
/// hold slot). An unbound stay row has no default chord but still carries
/// its canonical slot by row identity (numbered stay shares the digit VK
/// with its follow send; relative stay shares the H/K/arrow/J/L key with its
/// follow arm), so a rebind routes into the shared hold slot through its
/// explicit action.
#[must_use]
pub fn binding_canonical_vk(def: &BindingDef) -> Option<u32> {
    if let Some(first) = def
        .defaults
        .first()
        .and_then(|text| parse_chord(text).ok().map(|chord| chord.vk))
    {
        return Some(first);
    }
    match def.family {
        BindingFamily::WorkspaceStay { index } => Some(VK_0 + u32::from(index)),
        BindingFamily::WorkspaceSendRelative { .. } => Some(match def.id {
            "send-stay-prev-h" => 0x48,
            "send-stay-prev-k" => 0x4B,
            "send-stay-prev-left-arrow" => VK_LEFT,
            "send-stay-prev-up-arrow" => VK_UP,
            "send-stay-next-j" => 0x4A,
            "send-stay-next-l" => 0x4C,
            "send-stay-next-down-arrow" => VK_DOWN,
            "send-stay-next-right-arrow" => VK_RIGHT,
            _ => return None,
        }),
        _ => None,
    }
}

/// One effective (validated) binding: the persisted state plus the resolved
/// live chords and whether the owner can intercept them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveBinding {
    pub id: &'static str,
    pub active: bool,
    pub chords: Vec<String>,
    pub effective: bool,
    pub reason: &'static str,
    /// Live OS conflict for the UI: the catalog conflict for kept bindings,
    /// the same OS-owner text for disabled bindings (the OS action is live
    /// there), and the rebound chord's own conflict (see [`chord_conflict`])
    /// for rebinds. `None` means not applicable (unparsable or unbound);
    /// every Win-family chord carries either its documented owner or the
    /// honest unverified note (resize Alt chords included).
    pub conflict: Option<&'static str>,
}

/// Resolve the effective bindings for live use. Assumes `settings` already
/// passed [`validate_settings`]; unknown ids are skipped defensively.
#[must_use]
pub fn effective_bindings(settings: &Settings) -> Vec<EffectiveBinding> {
    let catalog = binding_catalog();
    let mut out = Vec::with_capacity(catalog.len());
    for def in catalog {
        let setting = settings.bindings.get(def.id);
        let (active, chords, effective, reason, conflict) = match setting {
            None
            | Some(BindingSetting {
                state: BindingState::Keep,
                ..
            }) => {
                let chords: Vec<String> = def.defaults.iter().map(|s| (*s).to_owned()).collect();
                if def.defaults.is_empty() {
                    // Unbound bindable row (item 2 stay): Keep means unbound,
                    // never Disabled/effective interception. The chord stays
                    // pass-through until rebound; the UI shows it unbound,
                    // distinctly from disabled.
                    (true, chords, false, "unbound: bindable", None)
                } else if def.implemented {
                    (true, chords, true, "", def.conflict)
                } else {
                    (
                        true,
                        chords,
                        false,
                        "not intercepted: Alt chords pass through",
                        def.conflict,
                    )
                }
            }
            Some(BindingSetting {
                state: BindingState::Disabled,
                ..
            }) => (
                false,
                Vec::new(),
                false,
                "disabled: passes through natively",
                def.conflict,
            ),
            Some(BindingSetting {
                state: BindingState::Rebind,
                chord,
            }) => match chord.as_deref().and_then(|text| parse_chord(text).ok()) {
                Some(parsed) if def.implemented => {
                    let rendered = render_chord(&parsed);
                    let conflict = chord_conflict(&rendered);
                    (true, vec![rendered], true, "", conflict)
                }
                Some(_) => (
                    true,
                    Vec::new(),
                    false,
                    "not intercepted: Alt chords pass through",
                    def.conflict,
                ),
                None => (
                    false,
                    Vec::new(),
                    false,
                    "malformed chord: passes through",
                    None,
                ),
            },
        };
        out.push(EffectiveBinding {
            id: def.id,
            active,
            chords,
            effective,
            reason,
            conflict,
        });
    }
    out
}

/// One classifier remap entry: a rebound physical chord routes into the
/// existing action classifier at its canonical virtual key plus explicit
/// action (see [`crate::snapkey::ChordRemap`]).
use crate::snapkey::{ChordDisable, ChordRemap};

/// Build the live classifier remap from validated settings: one full-modifier
/// entry per effective rebind (Shift/Ctrl/Alt matching the binding's native
/// arm, resize included). The rebound chord alone routes; the binding's old
/// default chords pass through via [`build_disabled`]. Unbound stay rows
/// route into their canonical slot (shared with the follow arm on the same
/// key) through the explicit stay action; rebound resize rows route into
/// their letter canonical slot through the mode-carrying resize action.
#[must_use]
pub fn build_remap(settings: &Settings) -> Vec<ChordRemap> {
    let mut defs = BTreeMap::new();
    for def in binding_catalog() {
        defs.insert(def.id, def);
    }
    let mut out = Vec::new();
    for (id, setting) in &settings.bindings {
        if setting.state != BindingState::Rebind {
            continue;
        }
        let Some(def) = defs.get(id.as_str()) else {
            continue;
        };
        if !def.implemented {
            continue;
        }
        let Some(text) = setting.chord.as_deref() else {
            continue;
        };
        let Ok(parsed) = parse_chord(text) else {
            continue;
        };
        if !binding_modifiers_ok(def, parsed.shift, parsed.ctrl, parsed.alt) {
            continue;
        }
        let action = binding_action(def);
        let Some(to_vk) = binding_canonical_vk(def) else {
            continue;
        };
        if parsed.vk == to_vk {
            continue;
        }
        out.push(ChordRemap {
            from_vk: parsed.vk,
            from_shift: parsed.shift,
            from_ctrl: parsed.ctrl,
            from_alt: parsed.alt,
            action,
            to_vk,
        });
    }
    out.sort_by_key(|entry| {
        (
            entry.from_vk,
            entry.from_shift,
            entry.from_ctrl,
            entry.from_alt,
        )
    });
    out
}

/// Build the live classifier suppression table from validated settings: every
/// disabled binding's default chord passes through, and every rebind's old
/// default chord passes through (only the custom chord routes, via
/// [`build_remap`]). Entries are full-modifier physical chords (resize rows
/// contribute both their letter and arrow defaults). The rebound table wins
/// over suppression for fresh downs, so a key claimed by a rebind still
/// routes even when another row's old default names it.
#[must_use]
pub fn build_disabled(settings: &Settings) -> Vec<ChordDisable> {
    let mut defs = BTreeMap::new();
    for def in binding_catalog() {
        defs.insert(def.id, def);
    }
    let mut out = Vec::new();
    for (id, setting) in &settings.bindings {
        let Some(def) = defs.get(id.as_str()) else {
            continue;
        };
        if !def.implemented {
            continue;
        }
        match setting.state {
            BindingState::Keep => {}
            BindingState::Disabled => {
                for default in def.defaults {
                    if let Ok(parsed) = parse_chord(default) {
                        out.push(ChordDisable {
                            vk: parsed.vk,
                            shift: parsed.shift,
                            ctrl: parsed.ctrl,
                            alt: parsed.alt,
                        });
                    }
                }
            }
            BindingState::Rebind => {
                // Keep-equivalent rebind (custom chord equals the row's own
                // default): the chord still routes natively with no remap
                // entry, so it must not be suppressed either.
                let custom = setting
                    .chord
                    .as_deref()
                    .and_then(|text| parse_chord(text).ok());
                for default in def.defaults {
                    if let Ok(parsed) = parse_chord(default) {
                        if custom == Some(parsed) {
                            continue;
                        }
                        out.push(ChordDisable {
                            vk: parsed.vk,
                            shift: parsed.shift,
                            ctrl: parsed.ctrl,
                            alt: parsed.alt,
                        });
                    }
                }
            }
        }
    }
    out.sort_by_key(|entry| (entry.vk, entry.shift, entry.ctrl, entry.alt));
    out.dedup_by_key(|entry| (entry.vk, entry.shift, entry.ctrl, entry.alt));
    out
}

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
    if settings.core.born_maximized != BORN_MAXIMIZED_TILE
        && settings.core.born_maximized != BORN_MAXIMIZED_OVERLAY
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
    validate_border(&settings.core.border)?;
    validate_underlay(&settings.core.underlay)?;
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

/// Binding preset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    /// KDE catalog defaults for every binding. The Win+L explicit opt-in is
    /// preserved: `allow_win_l` keeps its current value.
    Authentic,
    /// Authentic minus every known OS-conflicting chord: all focus rows (Win+H
    /// voice, Win+J recall, Win+K cast, Win+L lock, arrows Snap/maximize/
    /// minimize), all move-arrow rows (Win+Shift+arrows monitor-move/stretch),
    /// float (Win+G Game Bar), maximize (Win+M minimize-all), fullscreen
    /// (Win+F11 Xbox mode), orientation (Win+O orientation lock), both
    /// launchers (Win+B notification-area focus, Win+T taskbar cycle), all
    /// workspace digits (Win[/Shift]+digits
    /// taskbar launch/new-instance), and the two item 1 history arrows
    /// (Win+Ctrl+Left/Right native virtual-desktop switch, ownership
    /// unverified in repository) are disabled. What stays is exactly the
    /// undocumented set: letter moves (Win+Shift+H/J/K/L) and sticky
    /// (Win+Shift+G) carry no documented owner in the official list, plus
    /// the seven kept history rows (Tab/letters/Up/Down with the honest
    /// ownership-unknown note) and the eight item 2 relative-send follow
    /// rows (Win+Ctrl+Shift ownership unknown, no new disables selected).
    /// Both presets keep the keyboard resize defaults: Windows Alt-chord
    /// ownership is unverified in the repository, so neither preset invents
    /// an owner nor claims conflict-free; Compatible disables known
    /// conflicts only. Item 2 stay rows stay unbound under both presets.
    /// Applies as a deterministic reset: all overrides are
    /// dropped first, then the conflicts disable. Manual rebinding stays
    /// available afterwards; no replacement defaults are invented. The Win+L
    /// opt-in is preserved.
    Compatible,
}

/// Apply one preset in place. Core settings are untouched; only binding
/// overrides change. Returns the ids whose override changed.
pub fn apply_preset(settings: &mut Settings, preset: Preset) -> Vec<&'static str> {
    let mut changed = Vec::new();
    match preset {
        Preset::Authentic => {
            for def in binding_catalog() {
                if settings.bindings.remove(def.id).is_some() {
                    changed.push(def.id);
                }
            }
        }
        Preset::Compatible => {
            // Deterministic reset to the known safe set: drop every override
            // first (a stale rebound pointing at an OS-owned chord must not
            // survive the preset), then disable the conflicts.
            for def in binding_catalog() {
                if settings.bindings.remove(def.id).is_some() {
                    changed.push(def.id);
                }
            }
            for id in compatible_disabled_ids() {
                settings.bindings.insert(
                    id.to_owned(),
                    BindingSetting {
                        state: BindingState::Disabled,
                        chord: None,
                    },
                );
                changed.push(id);
            }
        }
    }
    settings.revision = settings.revision.saturating_add(1);
    changed
}

/// Preset decision helper: the ids the compatible preset disables (every
/// known OS-conflicting implemented row; resize rows keep their defaults
/// under both presets since Windows Alt-chord ownership is unverified;
/// see [`Preset::Compatible`]).
#[must_use]
pub const fn compatible_disabled_ids() -> [&'static str; 40] {
    [
        "focus-left",
        "focus-left-arrow",
        "focus-down",
        "focus-down-arrow",
        "focus-up",
        "focus-up-arrow",
        "focus-right",
        "focus-right-arrow",
        "move-left-arrow",
        "move-down-arrow",
        "move-up-arrow",
        "move-right-arrow",
        "toggle-float",
        "toggle-maximize",
        "toggle-fullscreen",
        "toggle-orientation",
        "launch-browser",
        "launch-terminal",
        "workspace-select-1",
        "workspace-select-2",
        "workspace-select-3",
        "workspace-select-4",
        "workspace-select-5",
        "workspace-select-6",
        "workspace-select-7",
        "workspace-select-8",
        "workspace-select-9",
        "workspace-select-0",
        "workspace-send-1",
        "workspace-send-2",
        "workspace-send-3",
        "workspace-send-4",
        "workspace-send-5",
        "workspace-send-6",
        "workspace-send-7",
        "workspace-send-8",
        "workspace-send-9",
        "workspace-send-0",
        "workspace-prev-left-arrow",
        "workspace-next-right-arrow",
    ]
}

/// Settings file path under a product directory.
#[must_use]
pub fn settings_file_path(dir: &Path) -> PathBuf {
    dir.join(SETTINGS_FILE_NAME)
}

/// Load and strictly validate the settings file. Missing files map to
/// defaults at revision 1 (the caller distinguishes via [`LoadOutcome`]).
fn load_validated(bytes: &[u8]) -> Result<Settings, SettingsError> {
    let text = std::str::from_utf8(bytes).map_err(|_| SettingsError::Malformed)?;
    let settings: Settings = serde_json::from_str(text).map_err(|_| SettingsError::Malformed)?;
    validate_settings(&settings)?;
    Ok(settings)
}

/// Load outcome for one directory read. Invalid files report their reason
/// and leave the bytes untouched (no blind reset, no rewrite).
#[derive(Debug, Clone, PartialEq)]
pub enum LoadOutcome {
    Loaded(Settings),
    Missing,
    Invalid(SettingsError),
}

/// Load the settings file from `dir`. Bounded read (oversized files refuse
/// before allocating); strict validation.
pub fn load_from_dir(dir: &Path) -> LoadOutcome {
    let path = settings_file_path(dir);
    let metadata = match std::fs::metadata(&path) {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return LoadOutcome::Missing,
        Err(e) => return LoadOutcome::Invalid(SettingsError::Io(e.to_string())),
    };
    if metadata.len() > SETTINGS_MAX_BYTES {
        return LoadOutcome::Invalid(SettingsError::TooLarge);
    }
    // Re-check after the bounded read: the file may have grown between the
    // metadata probe and the read, and `take` caps the allocation.
    let mut bytes = Vec::new();
    let read = (|| -> std::io::Result<()> {
        use std::io::Read;
        let file = std::fs::File::open(&path)?;
        file.take(SETTINGS_MAX_BYTES + 1).read_to_end(&mut bytes)?;
        Ok(())
    })();
    if let Err(e) = read {
        return LoadOutcome::Invalid(SettingsError::Io(e.to_string()));
    }
    if bytes.len() as u64 > SETTINGS_MAX_BYTES {
        return LoadOutcome::Invalid(SettingsError::TooLarge);
    }
    match load_validated(&bytes) {
        Ok(settings) => LoadOutcome::Loaded(settings),
        Err(e) => LoadOutcome::Invalid(e),
    }
}

/// Validate and atomically save settings to `dir`, bumping the revision past
/// both the file's and the caller's revision. The body goes to a unique
/// same-directory temp (created with `create_new`, so concurrent writers
/// never share a pending body) and is committed with the existing Win32
/// replace pattern on Windows (`MoveFileExW` with replace+write-through)
/// and same-directory rename elsewhere. The temp is removed on every error
/// path; only complete validated bytes ever appear under the final name.
pub fn save_to_dir(dir: &Path, settings: &mut Settings) -> Result<(), SettingsError> {
    validate_settings(settings)?;
    let existing_revision = match load_from_dir(dir) {
        LoadOutcome::Loaded(current) => current.revision,
        LoadOutcome::Missing => 0,
        LoadOutcome::Invalid(e) => return Err(e),
    };
    settings.revision = existing_revision.saturating_add(1).max(settings.revision);
    validate_settings(settings)?;
    let bytes = serde_json::to_vec_pretty(settings).map_err(|_| SettingsError::Malformed)?;
    std::fs::create_dir_all(dir).map_err(|e| SettingsError::Io(e.to_string()))?;
    static SAVE_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let mut attempt = 0u32;
    let pending = loop {
        let count = SAVE_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(1);
        let candidate = dir.join(format!(
            "{SETTINGS_FILE_NAME}.{}-{nanos}-{count}.pending",
            std::process::id()
        ));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(mut file) => {
                use std::io::Write;
                let write = (|| -> std::io::Result<()> {
                    file.write_all(&bytes)?;
                    file.sync_all()?;
                    Ok(())
                })();
                drop(file);
                if let Err(e) = write {
                    let _ = std::fs::remove_file(&candidate);
                    return Err(SettingsError::Io(e.to_string()));
                }
                break candidate;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && attempt < 8 => {
                attempt += 1;
            }
            Err(e) => return Err(SettingsError::Io(e.to_string())),
        }
    };
    if let Err(e) = crate::storage::atomic_replace(&pending, &settings_file_path(dir)) {
        let _ = std::fs::remove_file(&pending);
        return Err(SettingsError::Io(e.to_string()));
    }
    Ok(())
}

/// Last-good live state held by the owner across polls.
#[derive(Debug, Clone)]
pub struct LiveSettings {
    pub settings: Settings,
    pub mtime: Option<SystemTime>,
    /// Observable status for the later UI proof: `saved` carries the applied
    /// revision, `degraded` carries the bounded invalid-file reason while the
    /// last-good values stay live.
    pub status: String,
}

impl LiveSettings {
    #[must_use]
    pub fn fresh(settings: Settings, mtime: Option<SystemTime>) -> Self {
        let status = format!("saved:{}", settings.revision);
        Self {
            settings,
            mtime,
            status,
        }
    }
}

/// Poll outcome for one owner-pump check. `InvalidKept` keeps the last-good
/// values live with a degraded status; the file is never rewritten.
#[derive(Debug, Clone, PartialEq)]
pub enum PollOutcome {
    Unchanged,
    Changed(Settings),
    InvalidKept(String),
}

/// Poll `dir` for settings changes against the owner's last-good state.
/// Compares modification time first so unchanged files cost one stat; a
/// changed file reloads and validates, and an invalid file keeps last-good.
pub fn poll_for_change(dir: &Path, last: &LiveSettings) -> PollOutcome {
    let path = settings_file_path(dir);
    let mtime = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
    if mtime == last.mtime && last.mtime.is_some() {
        return PollOutcome::Unchanged;
    }
    match load_from_dir(dir) {
        LoadOutcome::Loaded(settings) => {
            if (settings.revision == last.settings.revision && mtime == last.mtime)
                || settings == last.settings
            {
                PollOutcome::Unchanged
            } else {
                PollOutcome::Changed(settings)
            }
        }
        LoadOutcome::Missing => {
            if last.mtime.is_none() {
                PollOutcome::Unchanged
            } else {
                // The file vanished: keep last-good, stay degraded-visible.
                PollOutcome::InvalidKept("missing: keeping last saved settings".to_owned())
            }
        }
        LoadOutcome::Invalid(e) => PollOutcome::InvalidKept(e.to_string()),
    }
}

/// Portable `TileOptions` construction from settings. Every explicit CLI
/// switch later overrides its corresponding value (the parser starts from
/// this base), so saved values are defaults, never fences.
#[must_use]
pub fn tile_options_from_settings(settings: &Settings) -> crate::tiling::TileOptions {
    let core = &settings.core;
    crate::tiling::TileOptions {
        seconds: None,
        trace: false,
        user_start: true,
        no_keyboard_snap_takeover: !core.keyboard.takeover,
        allow_win_l: core.keyboard.allow_win_l,
        no_mouse_snap_prevention: !core.mouse.snap_prevention,
        border: crate::active_border::ActiveBorderOptions {
            enabled: core.border.enabled,
            style: crate::active_border::ActiveBorderStyle {
                width: core.border.width,
                gap: core.border.gap,
                radius: core.border.radius,
                color: crate::active_border::parse_color(&core.border.color)
                    .unwrap_or(crate::active_border::DEFAULT_COLOR_RGB),
                use_theme: core.border.use_theme,
            },
        },
        underlay: crate::group_underlay::GroupUnderlayOptions {
            enabled: core.underlay.enabled,
            style: crate::group_underlay::GroupUnderlayStyle {
                color: crate::group_underlay::parse_color_argb(&core.underlay.color)
                    .map(|(rgb, _)| rgb)
                    .unwrap_or((0x2a, 0x82, 0xda)),
                alpha: crate::group_underlay::parse_color_argb(&core.underlay.color)
                    .map(|(_, alpha)| alpha)
                    .unwrap_or(0x66),
                extension: core.underlay.extension,
            },
        },
        inner_gap: core.inner_gap,
        outer_gap: core.outer_gap,
        scope_exes: Vec::new(),
        scope_hosts: Vec::new(),
    }
}

/// True when the per-domain reconcile path may use `Reconcile`: either no
/// session is retained yet (fresh domains seed through `Reconcile`) or the
/// retained inner/outer gaps already equal the carried values. A retained
/// mismatch must issue `UpdateGaps` so topology survives: plain reconcile
/// refuses gap drift instead of adopting it.
#[must_use]
pub fn retained_gaps_match(
    engine: &tiler_core::engine::Engine,
    key: &tiler_core::session::DomainKey,
    gap: i32,
    outer_gap: i32,
) -> bool {
    let Some(session) = engine.session(key) else {
        return true;
    };
    let domain_matches = session
        .domains()
        .iter()
        .any(|domain| domain.key() == *key && domain.gap == gap);
    domain_matches && engine.outer_gap_matches(key, outer_gap)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_kde_parity() {
        let settings = Settings::default();
        assert_eq!(settings.v, SETTINGS_SCHEMA_VERSION);
        assert_eq!(settings.core.inner_gap, 8);
        assert_eq!(settings.core.outer_gap, 8);
        assert!(settings.core.border.enabled);
        assert_eq!(settings.core.border.width, 3.0);
        assert_eq!(settings.core.border.gap, 0.0);
        assert_eq!(settings.core.border.radius, 6.0);
        assert_eq!(settings.core.border.color, "#2a82da");
        assert!(settings.core.border.use_theme);
        assert!(settings.core.underlay.enabled);
        assert_eq!(settings.core.underlay.color, "#662a82da");
        assert_eq!(settings.core.underlay.extension, -1.0);
        assert!(settings.core.keyboard.takeover);
        assert!(!settings.core.keyboard.allow_win_l);
        assert!(settings.core.mouse.snap_prevention);
        assert!(settings.core.workspace.default_tiled);
        assert_eq!(settings.core.same_axis_move, SAME_AXIS_MOVE_GROUP);
        assert_eq!(
            settings.core.same_axis_move_mode(),
            tiler_core::directional::SameAxisMove::GroupWithNeighbor
        );
        assert_eq!(settings.core.born_maximized, BORN_MAXIMIZED_TILE);
        assert!(!settings.core.born_maximized_keep_overlay());
        assert_eq!(settings.core.fixed_size_predicate, FIXED_SIZE_BOTH_AXES);
        assert_eq!(
            settings.core.fixed_size_predicate_mode(),
            tiler_core::size_hints::FixedSizePredicate::BothAxes
        );
        assert!(validate_settings(&settings).is_ok());
    }

    #[test]
    fn same_axis_move_tokens_map_exactly_with_group_default() {
        assert_eq!(
            tiler_core::directional::SameAxisMove::parse_wire(SAME_AXIS_MOVE_GROUP),
            Some(tiler_core::directional::SameAxisMove::GroupWithNeighbor)
        );
        assert_eq!(
            tiler_core::directional::SameAxisMove::parse_wire(SAME_AXIS_MOVE_SWAP),
            Some(tiler_core::directional::SameAxisMove::SwapWithNeighbor)
        );
        assert_eq!(
            tiler_core::directional::SameAxisMove::GroupWithNeighbor.as_wire_str(),
            SAME_AXIS_MOVE_GROUP
        );
        assert_eq!(
            tiler_core::directional::SameAxisMove::SwapWithNeighbor.as_wire_str(),
            SAME_AXIS_MOVE_SWAP
        );
        // No retired aliases, no case folding, no empty token.
        for bad in [
            "",
            "cosmic-wrap",
            "flat-swap",
            "GROUP-WITH-NEIGHBOR",
            "swap",
        ] {
            assert!(
                tiler_core::directional::SameAxisMove::parse_wire(bad).is_none(),
                "{bad}"
            );
        }
        let mut settings = Settings::default();
        settings.core.same_axis_move = SAME_AXIS_MOVE_SWAP.to_owned();
        assert_eq!(
            settings.core.same_axis_move_mode(),
            tiler_core::directional::SameAxisMove::SwapWithNeighbor
        );
        assert!(validate_settings(&settings).is_ok());
    }

    #[test]
    fn same_axis_move_options_carry_functional_labels_and_wm_tips() {
        assert_eq!(
            same_axis_move_options(),
            [
                (SAME_AXIS_MOVE_GROUP, "Group with neighbor", "COSMIC"),
                (SAME_AXIS_MOVE_SWAP, "Swap with neighbor", "i3, sway"),
            ]
        );
    }

    #[test]
    fn same_axis_move_invalid_value_refuses() {
        let mut settings = Settings::default();
        settings.core.same_axis_move = "cosmic-wrap".to_owned();
        assert!(matches!(
            validate_settings(&settings),
            Err(SettingsError::Invalid(_))
        ));
    }

    #[test]
    fn same_axis_move_presets_leave_core_field_unchanged() {
        for preset in [Preset::Authentic, Preset::Compatible] {
            let mut settings = Settings::default();
            settings.core.same_axis_move = SAME_AXIS_MOVE_SWAP.to_owned();
            apply_preset(&mut settings, preset);
            assert_eq!(settings.core.same_axis_move, SAME_AXIS_MOVE_SWAP);
            assert!(validate_settings(&settings).is_ok());
        }
    }

    #[test]
    fn fixed_size_predicate_tokens_map_exactly_with_both_axes_default() {
        use tiler_core::size_hints::FixedSizePredicate::{BothAxes, EitherAxis};
        assert_eq!(
            tiler_core::size_hints::FixedSizePredicate::parse_wire(FIXED_SIZE_BOTH_AXES),
            Some(BothAxes)
        );
        assert_eq!(
            tiler_core::size_hints::FixedSizePredicate::parse_wire(FIXED_SIZE_EITHER_AXIS),
            Some(EitherAxis)
        );
        assert_eq!(BothAxes.as_wire_str(), FIXED_SIZE_BOTH_AXES);
        assert_eq!(EitherAxis.as_wire_str(), FIXED_SIZE_EITHER_AXIS);
        // No aliases, no case folding, no empty token, no retired IDs.
        for bad in [
            "",
            "both",
            "either",
            "BOTH-AXES-FIXED",
            "width-and-height",
            "fixed",
        ] {
            assert!(
                tiler_core::size_hints::FixedSizePredicate::parse_wire(bad).is_none(),
                "{bad}"
            );
        }
        let mut settings = Settings::default();
        settings.core.fixed_size_predicate = FIXED_SIZE_EITHER_AXIS.to_owned();
        assert_eq!(settings.core.fixed_size_predicate_mode(), EitherAxis);
        assert!(validate_settings(&settings).is_ok());
        // Hand-built unknown values fail closed to the default mode but
        // refuse through validation (last-good on disk, never live).
        settings.core.fixed_size_predicate = "either".to_owned();
        assert_eq!(settings.core.fixed_size_predicate_mode(), BothAxes);
        assert!(matches!(
            validate_settings(&settings),
            Err(SettingsError::Invalid(_))
        ));
    }

    #[test]
    fn fixed_size_predicate_options_carry_functional_labels_and_wm_tips() {
        assert_eq!(
            fixed_size_predicate_options(),
            [
                (
                    FIXED_SIZE_BOTH_AXES,
                    "Width and height both fixed",
                    "COSMIC"
                ),
                (
                    FIXED_SIZE_EITHER_AXIS,
                    "Width or height fixed",
                    "Hyprland (Wayland), sway"
                ),
            ]
        );
    }

    #[test]
    fn fixed_size_predicate_presets_leave_core_field_unchanged() {
        for preset in [Preset::Authentic, Preset::Compatible] {
            let mut settings = Settings::default();
            settings.core.fixed_size_predicate = FIXED_SIZE_EITHER_AXIS.to_owned();
            apply_preset(&mut settings, preset);
            assert_eq!(settings.core.fixed_size_predicate, FIXED_SIZE_EITHER_AXIS);
            assert!(validate_settings(&settings).is_ok());
        }
    }

    #[test]
    fn born_maximized_options_carry_functional_labels_and_wm_tips() {
        assert_eq!(
            born_maximized_options(),
            [
                (
                    BORN_MAXIMIZED_TILE,
                    "Tile on launch",
                    "bspwm, i3, xmonad, sway, qtile"
                ),
                (
                    BORN_MAXIMIZED_OVERLAY,
                    "Keep maximized overlay",
                    "COSMIC, Hyprland"
                ),
            ]
        );
    }

    #[test]
    fn born_maximized_tokens_map_exactly_with_tile_default() {
        // Exact tokens only: no aliases, no case folding, no empty token.
        let mut settings = Settings::default();
        assert_eq!(settings.core.born_maximized, BORN_MAXIMIZED_TILE);
        assert!(!settings.core.born_maximized_keep_overlay());
        settings.core.born_maximized = BORN_MAXIMIZED_OVERLAY.to_owned();
        assert!(settings.core.born_maximized_keep_overlay());
        assert!(validate_settings(&settings).is_ok());
        for bad in [
            "",
            "tile",
            "keep-overlay",
            "TILE-ON-LAUNCH",
            "overlay",
            "group-with-neighbor",
        ] {
            let mut settings = Settings::default();
            settings.core.born_maximized = bad.to_owned();
            assert!(!settings.core.born_maximized_keep_overlay(), "{bad}");
            assert!(
                matches!(validate_settings(&settings), Err(SettingsError::Invalid(_))),
                "{bad}"
            );
        }
    }

    #[test]
    fn born_maximized_presets_leave_core_field_unchanged() {
        for preset in [Preset::Authentic, Preset::Compatible] {
            let mut settings = Settings::default();
            settings.core.born_maximized = BORN_MAXIMIZED_OVERLAY.to_owned();
            apply_preset(&mut settings, preset);
            assert_eq!(settings.core.born_maximized, BORN_MAXIMIZED_OVERLAY);
            assert!(validate_settings(&settings).is_ok());
        }
    }

    #[test]
    fn chord_grammar_accepts_win_family_and_renders_canonically() {
        let parsed = parse_chord("win+shift+L").expect("parse");
        assert!(parsed.shift && !parsed.alt && !parsed.ctrl);
        assert_eq!(parsed.vk, 0x4C);
        assert_eq!(render_chord(&parsed), "Win+Shift+L");
        assert_eq!(
            render_chord(&parse_chord("Shift+Win+h").expect("order")),
            "Win+Shift+H"
        );
        assert_eq!(
            render_chord(&parse_chord("WIN+F11").expect("fkey")),
            "Win+F11"
        );
        assert_eq!(
            render_chord(&parse_chord("win+left").expect("arrow")),
            "Win+Left"
        );
        assert!(is_lock_chord(&parse_chord("Win+L").expect("lock")));
        assert!(!is_lock_chord(&parse_chord("Win+Shift+L").expect("move")));
        assert!(!is_lock_chord(&parse_chord("Win+Alt+L").expect("alt")));
    }

    #[test]
    fn chord_grammar_refuses_malformed() {
        for bad in [
            "H",
            "Win",
            "Win+",
            "+Win+H",
            "Win+H+H",
            "Win+Win+H",
            "Ctrl+H",
            "Win+Foo",
            "Win+Shift",
            "Win++H",
            "",
            "Win+Escape",
        ] {
            assert!(parse_chord(bad).is_err(), "{bad}");
        }
        // Tab is a known key for the item 1 toggle.
        assert_eq!(
            render_chord(&parse_chord("Win+Ctrl+Tab").expect("tab")),
            "Win+Ctrl+Tab"
        );
    }

    #[test]
    fn catalog_covers_kde_actions_without_collisions() {
        let catalog = binding_catalog();
        // 16 focus/move (letter plus separate arrow rows) + 8 resize + 5
        // toggles + 2 launchers + 20 workspace + 9 item 1 history + 10
        // numbered stay + 8 relative follow + 8 relative stay (item 2) = 86.
        assert_eq!(catalog.len(), 86);
        let mut ids: Vec<&str> = catalog.iter().map(|def| def.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 86);
        // Every default parses and every row (resize included) intercepts.
        for def in &catalog {
            for default in def.defaults {
                parse_chord(default).expect("catalog chord parses");
            }
            assert!(def.implemented, "{}", def.id);
        }
        // Resize rows carry both letter and arrow defaults with the honest
        // unknown-ownership note, and ride the Alt arms through
        // mode-carrying actions.
        let resize_out = catalog
            .iter()
            .find(|def| def.id == "resize-out-left")
            .expect("row");
        assert_eq!(resize_out.defaults, &["Win+Alt+H", "Win+Alt+Left"]);
        assert!(
            resize_out.conflict.is_some_and(
                |c| c.contains("unverified in repository") && c.contains("unproven live")
            )
        );
        assert!(!binding_wants_shift(resize_out));
        assert!(!binding_wants_ctrl(resize_out));
        assert!(binding_wants_alt(resize_out));
        assert_eq!(
            binding_action(resize_out),
            crate::snapkey::ChordAction::ResizeOut
        );
        let resize_in = catalog
            .iter()
            .find(|def| def.id == "resize-in-left")
            .expect("row");
        assert_eq!(
            resize_in.defaults,
            &["Win+Alt+Shift+H", "Win+Alt+Shift+Left"]
        );
        assert!(binding_wants_shift(resize_in));
        assert!(binding_wants_alt(resize_in));
        assert_eq!(
            binding_action(resize_in),
            crate::snapkey::ChordAction::ResizeIn
        );
        assert_eq!(binding_arm_text(resize_out), "Win+Alt");
        assert_eq!(binding_arm_text(resize_in), "Win+Shift+Alt");
        // Focus-right keeps the lock-gated default with a truthful conflict.
        let focus_right = catalog
            .iter()
            .find(|def| def.id == "focus-right")
            .expect("row");
        assert!(focus_right.conflict.is_some());
        // Letter and arrow rows are separate bindings with distinct chords.
        let focus_left_arrow = catalog
            .iter()
            .find(|def| def.id == "focus-left-arrow")
            .expect("row");
        assert_eq!(focus_left_arrow.defaults, &["Win+Left"]);
        assert!(focus_left_arrow.conflict.is_some());
        // Item 1 history rows: toggle plus eight relative steps, all
        // implemented with honest ownership text.
        let previous = catalog
            .iter()
            .find(|def| def.id == "workspace-previous")
            .expect("row");
        assert_eq!(previous.defaults, &["Win+Ctrl+Tab"]);
        assert!(
            previous
                .conflict
                .is_some_and(|c| c.contains("unverified in repository"))
        );
        let prev_arrow = catalog
            .iter()
            .find(|def| def.id == "workspace-prev-left-arrow")
            .expect("row");
        assert_eq!(prev_arrow.defaults, &["Win+Ctrl+Left"]);
        assert!(
            prev_arrow
                .conflict
                .is_some_and(|c| c.contains("virtual desktop"))
        );
        assert!(binding_wants_ctrl(previous));
        assert!(!binding_wants_shift(previous));
        assert!(!binding_wants_alt(previous));
    }

    #[test]
    fn validation_rejects_bad_core_values() {
        let mut settings = Settings::default();
        settings.core.inner_gap = 65;
        assert!(validate_settings(&settings).is_err());
        settings.core.inner_gap = 8;
        settings.core.outer_gap = -1;
        assert!(validate_settings(&settings).is_err());
        settings.core.outer_gap = 8;
        settings.core.border.width = 33.0;
        assert!(validate_settings(&settings).is_err());
        settings.core.border.width = 3.0;
        settings.core.border.color = "#ffff".to_owned();
        assert!(validate_settings(&settings).is_err());
        settings.core.border.color = "#2a82da".to_owned();
        settings.core.underlay.color = "#808080".to_owned();
        assert!(validate_settings(&settings).is_err());
        settings.core.underlay.color = "#40808080".to_owned();
        settings.core.underlay.extension = -2.0;
        assert!(validate_settings(&settings).is_err());
        settings.core.underlay.extension = -1.0;
        assert!(validate_settings(&settings).is_ok());
        // Explicitly saved colors survive the default change: the superseded
        // default and the dogfood value both validate with schema still v1.
        for explicit in ["#662a82da", "#40808080", "#50909090"] {
            settings.core.underlay.color = explicit.to_owned();
            assert!(validate_settings(&settings).is_ok(), "{explicit}");
            let options = tile_options_from_settings(&settings);
            assert_eq!(
                crate::group_underlay::render_color_argb((
                    options.underlay.style.color,
                    options.underlay.style.alpha
                )),
                explicit
            );
        }
        assert_eq!(SETTINGS_SCHEMA_VERSION, 1);
    }

    #[test]
    fn omitted_underlay_color_defaults_to_candidate_c() {
        // Omitted fields deserialize to the new default; an explicit old
        // default round-trips unchanged through serde.
        let settings: Settings =
            serde_json::from_str(r#"{"v":1,"core":{}}"#).expect("minimal parses");
        assert_eq!(settings.core.underlay.color, "#662a82da");
        assert!(validate_settings(&settings).is_ok());
        let settings: Settings =
            serde_json::from_str(r##"{"v":1,"core":{"underlay":{"color":"#40808080"}}}"##)
                .expect("explicit old parses");
        assert_eq!(settings.core.underlay.color, "#40808080");
        assert!(validate_settings(&settings).is_ok());
        let settings: Settings =
            serde_json::from_str(r##"{"v":1,"core":{"underlay":{"color":"#50909090"}}}"##)
                .expect("explicit dogfood parses");
        assert_eq!(settings.core.underlay.color, "#50909090");
        assert!(validate_settings(&settings).is_ok());
    }

    #[test]
    fn validation_rejects_unknown_incoherent_and_unsafe_bindings() {
        let mut settings = Settings::default();
        settings.bindings.insert(
            "nope".to_owned(),
            BindingSetting {
                state: BindingState::Keep,
                chord: None,
            },
        );
        assert!(validate_settings(&settings).is_err());
        settings.bindings.clear();
        // Chord without rebind refuses.
        settings.bindings.insert(
            "focus-left".to_owned(),
            BindingSetting {
                state: BindingState::Keep,
                chord: Some("Win+U".to_owned()),
            },
        );
        assert!(validate_settings(&settings).is_err());
        // Rebind without chord refuses.
        settings.bindings.insert(
            "focus-left".to_owned(),
            BindingSetting {
                state: BindingState::Rebind,
                chord: None,
            },
        );
        assert!(validate_settings(&settings).is_err());
        // Lock-chord target refuses (no insecure bypass via remap).
        settings.bindings.insert(
            "focus-left".to_owned(),
            BindingSetting {
                state: BindingState::Rebind,
                chord: Some("Win+L".to_owned()),
            },
        );
        assert!(validate_settings(&settings).is_err());
        // Matching Shift polarity validates: the rebound chord alone routes,
        // so focus (unshifted arm) needs Win without Shift.
        settings.bindings.insert(
            "focus-left".to_owned(),
            BindingSetting {
                state: BindingState::Rebind,
                chord: Some("Win+U".to_owned()),
            },
        );
        assert!(validate_settings(&settings).is_ok());
        // ...but the shifted arm refuses for a focus binding: it would arm an
        // unintended move instead of the rebound focus.
        settings.bindings.insert(
            "focus-left".to_owned(),
            BindingSetting {
                state: BindingState::Rebind,
                chord: Some("Win+Shift+U".to_owned()),
            },
        );
        assert!(validate_settings(&settings).is_err());
        // ...and the shifted arm validates for a move binding on the same key:
        // one physical key may serve two arms (like float/sticky share G),
        // just never the same chord twice.
        settings.bindings.insert(
            "focus-left".to_owned(),
            BindingSetting {
                state: BindingState::Rebind,
                chord: Some("Win+U".to_owned()),
            },
        );
        settings.bindings.insert(
            "move-left".to_owned(),
            BindingSetting {
                state: BindingState::Rebind,
                chord: Some("Win+Shift+U".to_owned()),
            },
        );
        // ...but both route into one canonical key (H) sharing one hold
        // slot, so the second refuses: a rebound action takes exactly one
        // physical key (float/sticky excepted below).
        assert!(validate_settings(&settings).is_err());
        settings.bindings.remove("move-left");
        assert!(validate_settings(&settings).is_ok());
        // ...but two bindings may never claim the same rebound chord.
        settings.bindings.insert(
            "focus-down".to_owned(),
            BindingSetting {
                state: BindingState::Rebind,
                chord: Some("Win+U".to_owned()),
            },
        );
        assert!(validate_settings(&settings).is_err());
        settings.bindings.remove("focus-down");
        assert!(validate_settings(&settings).is_ok());
        // Float/sticky may share the G key across their separate arm slots:
        // the same physical key serves both arms with matching polarity.
        settings.bindings.clear();
        settings.bindings.insert(
            "toggle-float".to_owned(),
            BindingSetting {
                state: BindingState::Rebind,
                chord: Some("Win+U".to_owned()),
            },
        );
        settings.bindings.insert(
            "toggle-sticky".to_owned(),
            BindingSetting {
                state: BindingState::Rebind,
                chord: Some("Win+Shift+U".to_owned()),
            },
        );
        assert!(validate_settings(&settings).is_ok());
        settings.bindings.clear();
        // Keep-equivalent rebind (custom equals the row's own default):
        // valid, routes natively, claims nothing.
        settings.bindings.insert(
            "focus-left".to_owned(),
            BindingSetting {
                state: BindingState::Rebind,
                chord: Some("Win+H".to_owned()),
            },
        );
        assert!(validate_settings(&settings).is_ok());
        assert!(build_remap(&settings).is_empty());
        assert!(build_disabled(&settings).is_empty());
        settings.bindings.clear();
        // Resize rebinds ride the dedicated Alt arms: outwards needs Win+Alt,
        // inwards needs Win+Shift+Alt, each carrying its mode. The wrong
        // Shift polarity, a missing Alt, or an extra Ctrl refuses.
        settings.bindings.insert(
            "resize-out-left".to_owned(),
            BindingSetting {
                state: BindingState::Rebind,
                chord: Some("Win+Alt+U".to_owned()),
            },
        );
        assert!(validate_settings(&settings).is_ok());
        let remap = build_remap(&settings);
        assert_eq!(remap.len(), 1);
        assert_eq!(
            remap[0],
            ChordRemap {
                from_vk: 0x55,
                from_shift: false,
                from_ctrl: false,
                from_alt: true,
                action: crate::snapkey::ChordAction::ResizeOut,
                to_vk: 0x48,
            }
        );
        // The rebound-away defaults pass through via suppression (both the
        // letter and arrow defaults of the rebound row).
        let disabled = build_disabled(&settings);
        assert!(disabled.contains(&ChordDisable {
            vk: 0x48,
            shift: false,
            ctrl: false,
            alt: true,
        }));
        assert!(disabled.contains(&ChordDisable {
            vk: VK_LEFT,
            shift: false,
            ctrl: false,
            alt: true,
        }));
        settings.bindings.clear();
        for bad in ["Win+U", "Win+Shift+U", "Win+Ctrl+Alt+U", "Win+Alt+Shift+U"] {
            settings.bindings.clear();
            settings.bindings.insert(
                "resize-out-left".to_owned(),
                BindingSetting {
                    state: BindingState::Rebind,
                    chord: Some(bad.to_owned()),
                },
            );
            assert!(validate_settings(&settings).is_err(), "{bad}");
        }
        settings.bindings.clear();
        settings.bindings.insert(
            "resize-in-left".to_owned(),
            BindingSetting {
                state: BindingState::Rebind,
                chord: Some("Win+Alt+Shift+U".to_owned()),
            },
        );
        assert!(validate_settings(&settings).is_ok());
        settings.bindings.clear();
        // Unshifted Win+L can never be a rebind target, resize included.
        settings.bindings.insert(
            "resize-out-left".to_owned(),
            BindingSetting {
                state: BindingState::Rebind,
                chord: Some("Win+L".to_owned()),
            },
        );
        assert!(validate_settings(&settings).is_err());
        settings.bindings.remove("resize-out-left");
        assert!(validate_settings(&settings).is_ok());
        // Alt rebind refuses on non-Alt arms.
        settings.bindings.insert(
            "focus-left".to_owned(),
            BindingSetting {
                state: BindingState::Rebind,
                chord: Some("Win+Alt+U".to_owned()),
            },
        );
        assert!(validate_settings(&settings).is_err());
    }

    #[test]
    fn validation_rejects_duplicate_active_chords() {
        let mut settings = Settings::default();
        // Win+J is focus-down: rebinding focus-left onto it collides.
        settings.bindings.insert(
            "focus-left".to_owned(),
            BindingSetting {
                state: BindingState::Rebind,
                chord: Some("Win+J".to_owned()),
            },
        );
        assert!(validate_settings(&settings).is_err());
        // A disabled binding frees its chord: disabling focus-down then
        // rebinding focus-left onto Win+J validates. Rows are per-chord, so
        // the Win+Down arrow row stays active on its own chord.
        settings.bindings.insert(
            "focus-down".to_owned(),
            BindingSetting {
                state: BindingState::Disabled,
                chord: None,
            },
        );
        assert!(validate_settings(&settings).is_ok());
    }

    #[test]
    fn presets_restore_defaults_and_preserve_win_l_opt_in() {
        let mut settings = Settings::default();
        settings.core.keyboard.allow_win_l = true;
        settings.bindings.insert(
            "focus-left".to_owned(),
            BindingSetting {
                state: BindingState::Rebind,
                chord: Some("Win+U".to_owned()),
            },
        );
        settings.bindings.insert(
            "toggle-float".to_owned(),
            BindingSetting {
                state: BindingState::Disabled,
                chord: None,
            },
        );
        let changed = apply_preset(&mut settings, Preset::Authentic);
        assert!(changed.contains(&"focus-left"));
        assert!(changed.contains(&"toggle-float"));
        assert!(settings.bindings.is_empty());
        assert!(settings.core.keyboard.allow_win_l);
        assert!(validate_settings(&settings).is_ok());
        // Compatible disables every OS-conflicting row (38), preserving the
        // opt-in. From empty state the change is exactly the disable list.
        let changed = apply_preset(&mut settings, Preset::Compatible);
        assert_eq!(changed, compatible_disabled_ids());
        assert!(changed.contains(&"focus-left"));
        assert!(changed.contains(&"focus-left-arrow"));
        assert!(changed.contains(&"toggle-float"));
        assert!(changed.contains(&"toggle-fullscreen"));
        assert!(changed.contains(&"workspace-select-1"));
        assert!(changed.contains(&"workspace-prev-left-arrow"));
        assert!(changed.contains(&"workspace-next-right-arrow"));
        assert!(!changed.contains(&"workspace-previous"));
        assert!(!changed.contains(&"workspace-prev-h"));
        assert_eq!(
            settings.bindings.get("toggle-float").map(|b| &b.state),
            Some(&BindingState::Disabled)
        );
        assert!(!settings.bindings.contains_key("move-left"));
        assert!(!settings.bindings.contains_key("toggle-sticky"));
        assert!(!settings.bindings.contains_key("workspace-previous"));
        let effective = effective_bindings(&settings);
        assert_eq!(effective.len(), 86);
        for row in &effective {
            if compatible_disabled_ids().contains(&row.id) {
                assert!(!row.active, "{}", row.id);
                assert!(!row.effective, "{}", row.id);
            } else if row.id == "move-left"
                || row.id == "move-down"
                || row.id == "move-up"
                || row.id == "move-right"
                || row.id == "toggle-sticky"
            {
                assert!(row.active && row.effective, "{}", row.id);
            }
        }
        assert!(settings.core.keyboard.allow_win_l);
        assert!(validate_settings(&settings).is_ok());
        // Deterministic reset: a stale rebound (even at an OS-owned chord)
        // does not survive the preset.
        settings.bindings.insert(
            "move-left".to_owned(),
            BindingSetting {
                state: BindingState::Rebind,
                chord: Some("Win+Shift+Q".to_owned()),
            },
        );
        let changed = apply_preset(&mut settings, Preset::Compatible);
        assert!(!settings.bindings.contains_key("move-left"));
        assert_eq!(
            settings.bindings.get("focus-left").map(|b| &b.state),
            Some(&BindingState::Disabled)
        );
        assert!(changed.contains(&"move-left"));
        assert!(validate_settings(&settings).is_ok());
    }

    #[test]
    fn chord_conflict_names_the_physical_os_owner() {
        assert!(
            chord_conflict("Win+H")
                .is_some_and(|text| text.contains("Voice") && text.contains("unproven"))
        );
        // Win+Shift+H has no documented owner: honest unverified note, never
        // a definitive clean claim.
        assert!(
            chord_conflict("Win+Shift+H")
                .is_some_and(|text| text.contains("No documented conflict"))
        );
        // Win+U opens Accessibility settings (official list, Windows 11 tab;
        // Ease of Access Center on Windows 10): rebinding onto it stays
        // allowed (conflict text is advisory; only lock-chord targets refuse
        // in validation), but the UI must name the owner.
        assert!(chord_conflict("Win+U").is_some_and(|text| text.contains("Accessibility")));
        assert!(chord_conflict("Win+E").is_some_and(|text| text.contains("File Explorer")));
        assert!(chord_conflict("Win+Z").is_some_and(|text| text.contains("snap layouts")));
        assert!(chord_conflict("Win+Shift+S").is_some_and(|text| text.contains("Snipping")));
        assert!(chord_conflict("Win+G").is_some_and(|text| text.contains("cannot fully contain")));
        assert!(
            chord_conflict("Win+Shift+G")
                .is_some_and(|text| text.contains("No documented conflict"))
        );
        assert!(chord_conflict("Win+L").is_some_and(|text| text.contains("opt-in")));
        assert!(chord_conflict("Win+1").is_some_and(|text| text.contains("taskbar")));
        assert!(chord_conflict("bogus").is_none());
        // Rebind rows report the rebound chord's conflict, not the stale
        // original: focus-left onto Win+U names Accessibility despite H's
        // voice-dictation owner, and the rebind itself still validates.
        let mut settings = Settings::default();
        settings.bindings.insert(
            "focus-left".to_owned(),
            BindingSetting {
                state: BindingState::Rebind,
                chord: Some("Win+U".to_owned()),
            },
        );
        assert!(validate_settings(&settings).is_ok());
        let effective = effective_bindings(&settings);
        let row = effective
            .iter()
            .find(|row| row.id == "focus-left")
            .expect("row");
        assert_eq!(row.chords, vec!["Win+U".to_owned()]);
        assert!(
            row.conflict
                .is_some_and(|text| text.contains("Accessibility"))
        );
    }

    #[test]
    fn remap_routes_win_family_rebinds_only() {
        let mut settings = Settings::default();
        settings.bindings.insert(
            "focus-left".to_owned(),
            BindingSetting {
                state: BindingState::Rebind,
                chord: Some("Win+U".to_owned()),
            },
        );
        // One full-modifier entry: the unshifted rebound routes to focus.
        // The shifted chord on the same key stays native (no unintended move).
        let remap = build_remap(&settings);
        assert_eq!(
            remap,
            vec![ChordRemap {
                from_vk: 0x55,
                from_shift: false,
                from_ctrl: false,
                from_alt: false,
                action: crate::snapkey::ChordAction::Directional,
                to_vk: 0x48
            },]
        );
        // The rebound-away default passes through via the suppression table.
        let disabled = build_disabled(&settings);
        assert_eq!(
            disabled,
            vec![ChordDisable {
                vk: 0x48,
                shift: false,
                ctrl: false,
                alt: false
            }]
        );
        // Rebinding onto the canonical key is a valid no-op (no entry).
        let mut same = Settings::default();
        same.bindings.insert(
            "focus-left".to_owned(),
            BindingSetting {
                state: BindingState::Rebind,
                chord: Some("Win+H".to_owned()),
            },
        );
        assert!(validate_settings(&same).is_ok());
        assert!(build_remap(&same).is_empty());
    }

    #[test]
    fn disabled_rows_suppress_their_chords_and_rebinds_free_theirs() {
        let mut settings = Settings::default();
        settings.bindings.insert(
            "toggle-float".to_owned(),
            BindingSetting {
                state: BindingState::Disabled,
                chord: None,
            },
        );
        settings.bindings.insert(
            "focus-left-arrow".to_owned(),
            BindingSetting {
                state: BindingState::Disabled,
                chord: None,
            },
        );
        assert!(validate_settings(&settings).is_ok());
        // Win+G unshifted and Win+Left unshifted pass through; the Win+H
        // letter row is untouched (per-chord rows).
        assert_eq!(
            build_disabled(&settings),
            vec![
                ChordDisable {
                    vk: VK_LEFT,
                    shift: false,
                    ctrl: false,
                    alt: false
                },
                ChordDisable {
                    vk: 0x47,
                    shift: false,
                    ctrl: false,
                    alt: false
                },
            ]
        );
        assert!(build_remap(&settings).is_empty());
    }

    #[test]
    fn version_gate_refuses_unknown_schema() {
        let settings = Settings {
            v: 2,
            ..Settings::default()
        };
        assert_eq!(
            validate_settings(&settings),
            Err(SettingsError::UnsupportedVersion)
        );
    }

    #[test]
    fn history_rebind_needs_ctrl_and_shares_vk_across_actions() {
        // Item 1 history arms ride Win+Ctrl: unshifted Win-only refuses,
        // shifted Ctrl refuses, Alt refuses, and the Ctrl arm validates.
        for bad in ["Win+U", "Win+Shift+U", "Win+Alt+U", "Win+Ctrl+Shift+U"] {
            let mut settings = Settings::default();
            settings.bindings.insert(
                "workspace-prev-h".to_owned(),
                BindingSetting {
                    state: BindingState::Rebind,
                    chord: Some(bad.to_owned()),
                },
            );
            assert!(validate_settings(&settings).is_err(), "{bad}");
        }
        let mut settings = Settings::default();
        settings.bindings.insert(
            "workspace-prev-h".to_owned(),
            BindingSetting {
                state: BindingState::Rebind,
                chord: Some("Win+Ctrl+U".to_owned()),
            },
        );
        assert!(validate_settings(&settings).is_ok());
        // History shares H with the directional focus arm across separate
        // slots: keeping Win+H focus while rebound history rides Win+Ctrl+U
        // validates (distinct explicit actions).
        let remap = build_remap(&settings);
        assert_eq!(remap.len(), 1);
        assert_eq!(remap[0].from_vk, 0x55);
        assert!(remap[0].from_ctrl);
        assert_eq!(remap[0].action, crate::snapkey::ChordAction::WorkspacePrev);
        // Two history prev rows share no canonical slot (distinct VKs), but
        // two rebinds onto the same history action+VK refuse.
        settings.bindings.insert(
            "workspace-prev-k".to_owned(),
            BindingSetting {
                state: BindingState::Rebind,
                chord: Some("Win+Ctrl+U".to_owned()),
            },
        );
        assert!(validate_settings(&settings).is_err());
    }
}

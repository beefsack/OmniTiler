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

/// One classifier remap entry: a rebound physical chord routes into the
/// existing action classifier at its canonical virtual key plus explicit
/// action (see [`crate::snapkey::ChordRemap`]).
use crate::snapkey::{ChordDisable, ChordRemap};

mod catalog;
mod chords;
mod effective;
mod persistence;
mod presets;
mod schema;
mod tile_bridge;
mod validation;
mod windows_catalog;

pub use catalog::{
    BindingDef, BindingFamily, LaunchKind, OUTPUT_SEND_CONFLICT, RESIZE_CONFLICT, SnapFamilyOp,
    ToggleKind, WorkspaceHistoryKind, binding_action, binding_arm_text, binding_canonical_vk,
    binding_catalog, binding_modifiers_ok, binding_wants_alt, binding_wants_ctrl,
    binding_wants_shift, chord_conflict,
};
pub use chords::{
    Chord, VK_0, VK_DOWN, VK_G, VK_LEFT, VK_RIGHT, VK_TAB, VK_UP, is_lock_chord, parse_chord,
    render_chord, vk_for_key_name,
};
pub use effective::{EffectiveBinding, build_disabled, build_remap, effective_bindings};
pub use persistence::{
    LiveSettings, LoadOutcome, PollOutcome, load_from_dir, poll_for_change, save_to_dir,
    settings_file_path,
};
pub use presets::{Preset, apply_preset, compatible_disabled_ids, compatible_rebinds};
pub use schema::{
    ANCHOR_FALLBACK_CLOSEST, ANCHOR_FALLBACK_CLOSEST_LABEL, ANCHOR_FALLBACK_CLOSEST_TIP,
    ANCHOR_FALLBACK_LARGEST, ANCHOR_FALLBACK_LARGEST_LABEL, ANCHOR_FALLBACK_LARGEST_TIP,
    ANCHOR_FALLBACK_NO_SEARCH, ANCHOR_FALLBACK_NO_SEARCH_LABEL, ANCHOR_FALLBACK_NO_SEARCH_TIP,
    BORN_MAXIMIZED_OVERLAY, BORN_MAXIMIZED_OVERLAY_LABEL, BORN_MAXIMIZED_OVERLAY_TIP,
    BORN_MAXIMIZED_TILE, BORN_MAXIMIZED_TILE_LABEL, BORN_MAXIMIZED_TILE_TIP, BindingSetting,
    BindingState, BorderSettings, CoreSettings, DEFAULT_BORDER_COLOR_HEX, DEFAULT_BORDER_GAP,
    DEFAULT_BORDER_RADIUS, DEFAULT_BORDER_USE_THEME, DEFAULT_BORDER_WIDTH, DEFAULT_INNER_GAP,
    DEFAULT_OUTER_GAP, DEFAULT_UNDERLAY_COLOR_HEX, DEFAULT_UNDERLAY_EXTENSION,
    FIXED_SIZE_BOTH_AXES, FIXED_SIZE_BOTH_AXES_LABEL, FIXED_SIZE_BOTH_AXES_TIP,
    FIXED_SIZE_EITHER_AXIS, FIXED_SIZE_EITHER_AXIS_LABEL, FIXED_SIZE_EITHER_AXIS_TIP,
    KeyboardSettings, MAX_BORDER_GAP, MAX_BORDER_RADIUS, MAX_BORDER_WIDTH, MAX_GAP,
    MAX_UNDERLAY_EXTENSION, MIGRATION_REFILL_LAST_REMAINING, MIGRATION_REFILL_LAST_REMAINING_LABEL,
    MIGRATION_REFILL_LAST_REMAINING_TIP, MIGRATION_REFILL_MRU, MIGRATION_REFILL_MRU_LABEL,
    MIGRATION_REFILL_MRU_TIP, MouseSettings, NO_SPACE_FLOAT, NO_SPACE_FLOAT_LABEL,
    NO_SPACE_FLOAT_TIP, NO_SPACE_OVERLAP, NO_SPACE_OVERLAP_LABEL, NO_SPACE_OVERLAP_TIP,
    SAME_AXIS_MOVE_GROUP, SAME_AXIS_MOVE_GROUP_LABEL, SAME_AXIS_MOVE_GROUP_TIP,
    SAME_AXIS_MOVE_SWAP, SAME_AXIS_MOVE_SWAP_LABEL, SAME_AXIS_MOVE_SWAP_TIP, SETTINGS_FILE_NAME,
    SETTINGS_MAX_BYTES, SETTINGS_PENDING_SUFFIX, SETTINGS_SCHEMA_VERSION, Settings, SettingsError,
    UnderlaySettings, WORKSPACE_MODE_GLOBAL_UNIQUE, WORKSPACE_MODE_GLOBAL_UNIQUE_LABEL,
    WORKSPACE_MODE_GLOBAL_UNIQUE_TIP, WORKSPACE_MODE_PER_OUTPUT_LOCAL,
    WORKSPACE_MODE_PER_OUTPUT_LOCAL_LABEL, WORKSPACE_MODE_PER_OUTPUT_LOCAL_TIP,
    WORKSPACE_MODE_SHARED, WORKSPACE_MODE_SHARED_LABEL, WORKSPACE_MODE_SHARED_TIP,
    WorkspaceSettings, anchor_fallback_options, born_maximized_options,
    fixed_size_predicate_options, migration_source_refill_options, no_space_options,
    same_axis_move_options, workspace_mode_options,
};
pub use tile_bridge::{retained_gaps_match, tile_options_from_settings};
pub use validation::{gap_valid, validate_settings};
pub use windows_catalog::{
    FieldStorageMap, NATIVE_PANELS, SECTION_DESKTOP_PAGER, SECTION_DESKTOP_SNAP,
    SECTION_SHORTCUTS_BINDINGS, SECTION_SHORTCUTS_GAMING, SECTION_SHORTCUTS_PRESETS,
    SECTION_SHORTCUTS_TAKEOVER, WINDOWS_FIELD_ALLOW_WIN_L, WINDOWS_FIELD_MAPPINGS,
    WINDOWS_FIELD_SNAP_PREVENTION, WINDOWS_FIELD_TAKEOVER, WINDOWS_FIELD_TASKBAR_PAGER,
    WINDOWS_ONLY_CAPS, WINDOWS_ONLY_FIELDS, WINDOWS_SECTIONS, WINDOWS_UNSUPPORTED_FIELDS,
    WindowsValue, stage_windows_field_value, windows_field_def, windows_field_value,
};

#[cfg(test)]
mod tests;

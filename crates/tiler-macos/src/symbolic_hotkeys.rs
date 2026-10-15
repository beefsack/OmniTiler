#![forbid(unsafe_code)]

//! Read-only Mission Control binding corroboration (no writes, no IDs).
//!
//! Two sources, kept separate:
//! - Private `com.apple.symbolichotkeys` overrides (`defaults read ...`):
//!   incomplete overrides, not the effective table. A missing `value` or ID
//!   never means disabled. Conventional IDs (79/80 previous Space, 81/82
//!   next Space, 118–133 Desktop 1–16) are lookup hypotheses, not verified
//!   ID metadata.
//! - Public Carbon `CopySymbolicHotKeys` effective table
//!   ([`crate::ffi::copy_symbolic_hotkeys`]): effective chord + enabled
//!   dictionaries with NO symbolic IDs (the SDK documents no mapping from
//!   an entry back to a preference-pane hotkey). Array index is never an ID.
//!
//! Corroboration only: an enabled `Ctrl+Left` / `Ctrl+Right` effective chord
//! supports the arrow-hypothesis for IDs 79/81; it never certifies that a
//! chord activates a Space. The bounded actual switch experiment proves the
//! mapping ([`crate::space_switch_probe`]). Digit/Desktop-N and drag routes
//! stay ineligible (unresolved SID/index mappings, no settings writes).
//!
//! Prior art (local checkouts first; pinned upstream URLs only):
//! effective-table shape plus the automatic Fn-bit strip follows Hammerspoon
//! `extensions/hotkey/libhotkey.m`
//! (https://github.com/Hammerspoon/hammerspoon/blob/23e387e2805a9890066366e0ac96c71b27f0cfd5/extensions/hotkey/libhotkey.m#L208-L241);
//! Carbon-vs-CG modifier domains plus the index-is-not-ID rule follow the
//! routes survey
//! (https://github.com/Hammerspoon/hammerspoon/blob/23e387e2805a9890066366e0ac96c71b27f0cfd5/extensions/spaces/spaces.lua#L689-L882
//! context in docs/research/macos-port/native-spaces-routes.md).

use serde::{Deserialize, Serialize};

/// Automatic Fn bit on effective Carbon modifiers: `1 << 17`, planted by
/// the OS for some keys (notably arrows/function keys) and unsettable, so
/// it is stripped before every chord match (Hammerspoon shape above).
pub const CARBON_FN_BIT: u32 = 1 << 17;
/// Carbon `controlKey` (`Events.h`): the Ctrl role in effective chords.
pub const CARBON_CTRL_BIT: u32 = 0x1000;
/// Mac virtual keycodes for the arrow legs (`CGKeyCode` space).
pub const KEY_LEFT_ARROW: u32 = 0x7B;
pub const KEY_RIGHT_ARROW: u32 = 0x7C;
/// `CGEventFlags` Control bit for the synthetic arrow posts (CG domain,
/// not Carbon): matches [`crate::input::cg_flag_bits`] for Ctrl.
pub const CG_CTRL_BIT: u64 = 0x0004_0000;

/// Strip the automatic Fn bit from raw effective Carbon modifiers.
#[must_use]
pub const fn strip_fn_bit(raw_carbon_modifiers: u32) -> u32 {
    raw_carbon_modifiers & !CARBON_FN_BIT
}

/// Pure chord match over plain data: the entry's Fn-stripped Carbon
/// modifiers must equal `want_mods` exactly and the keycode must equal
/// `want_keycode`. Extra modifiers fail; the Fn bit alone never fails.
#[must_use]
pub const fn chord_matches(
    entry_keycode: u32,
    entry_raw_modifiers: u32,
    want_keycode: u32,
    want_mods: u32,
) -> bool {
    entry_keycode == want_keycode && strip_fn_bit(entry_raw_modifiers) == want_mods
}

/// Whether an effective entry is the enabled `Ctrl+Left` chord.
#[must_use]
pub const fn is_enabled_ctrl_left(keycode: u32, raw_modifiers: u32, enabled: bool) -> bool {
    enabled && chord_matches(keycode, raw_modifiers, KEY_LEFT_ARROW, CARBON_CTRL_BIT)
}

/// Whether an effective entry is the enabled `Ctrl+Right` chord.
#[must_use]
pub const fn is_enabled_ctrl_right(keycode: u32, raw_modifiers: u32, enabled: bool) -> bool {
    enabled && chord_matches(keycode, raw_modifiers, KEY_RIGHT_ARROW, CARBON_CTRL_BIT)
}

/// Effective-table corroboration for the two arrow chords: exactly which
/// of `Ctrl+Left` / `Ctrl+Right` appear enabled in the effective table.
/// Either `false` blocks its arrow leg (no settings write to enable it);
/// either `true` is corroboration only — the switch experiment proves the
/// mapping. Never indexed to symbolic IDs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArrowCorroboration {
    pub ctrl_left_enabled: bool,
    pub ctrl_right_enabled: bool,
    pub entries_seen: usize,
}

impl ArrowCorroboration {
    #[must_use]
    pub const fn both_enabled(self) -> bool {
        self.ctrl_left_enabled && self.ctrl_right_enabled
    }
}

/// Pure corroboration over plain entries: at least one enabled match per
/// chord. No live touch.
#[must_use]
pub fn corroborate_entries(entries: &[crate::ffi::SymbolicHotKeyEntry]) -> ArrowCorroboration {
    let mut left = false;
    let mut right = false;
    for entry in entries {
        if is_enabled_ctrl_left(entry.keycode, entry.carbon_modifiers, entry.enabled) {
            left = true;
        }
        if is_enabled_ctrl_right(entry.keycode, entry.carbon_modifiers, entry.enabled) {
            right = true;
        }
    }
    ArrowCorroboration {
        ctrl_left_enabled: left,
        ctrl_right_enabled: right,
        entries_seen: entries.len(),
    }
}

/// Live read-only effective-table query plus arrow corroboration. No
/// symbolic IDs, no settings reads or writes: the returned struct reports
/// which arrow chords the OS currently reports enabled.
pub fn query_arrow_corroboration() -> Result<ArrowCorroboration, String> {
    let entries = crate::ffi::copy_symbolic_hotkeys()?;
    Ok(corroborate_entries(&entries))
}

/// Full live effective-table read for diagnostics: raw entries plus the
/// arrow corroboration. IDs and counts only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BindingsReport {
    pub entries: Vec<crate::ffi::SymbolicHotKeyEntry>,
    pub corroboration: ArrowCorroboration,
    /// Conventional override hypothesis (IDs 79/81 enabled without values):
    /// carried as a hypothesis string, never a mapping claim.
    pub override_note: String,
}

pub fn read_bindings() -> Result<BindingsReport, String> {
    let entries = crate::ffi::copy_symbolic_hotkeys()?;
    let corroboration = corroborate_entries(&entries);
    Ok(BindingsReport {
        entries,
        corroboration,
        override_note: "IDs 79/81 enabled-without-value in com.apple.symbolichotkeys overrides are a candidate arrow hypothesis only; effective-chord corroboration above never indexes entries to IDs, and only the bounded switch experiment proves the mapping".to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(keycode: u32, mods: u32, enabled: bool) -> crate::ffi::SymbolicHotKeyEntry {
        crate::ffi::SymbolicHotKeyEntry {
            keycode,
            carbon_modifiers: mods,
            enabled,
        }
    }

    #[test]
    fn fn_bit_strips_before_match() {
        assert_eq!(
            strip_fn_bit(CARBON_CTRL_BIT | CARBON_FN_BIT),
            CARBON_CTRL_BIT
        );
        assert_eq!(strip_fn_bit(CARBON_CTRL_BIT), CARBON_CTRL_BIT);
        assert!(chord_matches(
            KEY_RIGHT_ARROW,
            CARBON_CTRL_BIT | CARBON_FN_BIT,
            KEY_RIGHT_ARROW,
            CARBON_CTRL_BIT
        ));
    }

    #[test]
    fn extra_modifiers_fail_exact_match() {
        assert!(!chord_matches(
            KEY_RIGHT_ARROW,
            CARBON_CTRL_BIT | 0x200,
            KEY_RIGHT_ARROW,
            CARBON_CTRL_BIT
        ));
        assert!(!chord_matches(
            KEY_LEFT_ARROW,
            CARBON_CTRL_BIT,
            KEY_RIGHT_ARROW,
            CARBON_CTRL_BIT
        ));
    }

    #[test]
    fn disabled_entries_never_corroborate() {
        assert!(!is_enabled_ctrl_right(
            KEY_RIGHT_ARROW,
            CARBON_CTRL_BIT,
            false
        ));
        assert!(!is_enabled_ctrl_left(
            KEY_LEFT_ARROW,
            CARBON_CTRL_BIT,
            false
        ));
        assert!(is_enabled_ctrl_right(
            KEY_RIGHT_ARROW,
            CARBON_CTRL_BIT,
            true
        ));
    }

    #[test]
    fn corroboration_needs_one_enabled_match_per_chord() {
        let entries = vec![
            entry(KEY_RIGHT_ARROW, CARBON_CTRL_BIT | CARBON_FN_BIT, true),
            entry(KEY_LEFT_ARROW, CARBON_CTRL_BIT, false),
            entry(0x04, CARBON_CTRL_BIT, true),
        ];
        let report = corroborate_entries(&entries);
        assert!(report.ctrl_right_enabled);
        assert!(!report.ctrl_left_enabled);
        assert!(!report.both_enabled());
        assert_eq!(report.entries_seen, 3);
    }

    #[test]
    fn empty_table_corroborates_nothing() {
        let report = corroborate_entries(&[]);
        assert!(!report.ctrl_left_enabled);
        assert!(!report.ctrl_right_enabled);
        assert_eq!(report.entries_seen, 0);
    }

    #[test]
    fn arrow_keycodes_are_mac_virtual_codes() {
        assert_eq!(KEY_LEFT_ARROW, 0x7B);
        assert_eq!(KEY_RIGHT_ARROW, 0x7C);
        assert_eq!(crate::input_policy::KEY_LEFT, KEY_LEFT_ARROW);
        assert_eq!(crate::input_policy::KEY_RIGHT, KEY_RIGHT_ARROW);
    }
}

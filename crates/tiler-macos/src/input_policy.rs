#![forbid(unsafe_code)]

//! Product input policy for the macOS basic dogfood preview.
//!
//! The tap is the sole product interception route over the
//! shared basic tiling catalogue ([`crate::native_input::tile_catalogue`]:
//! 8 focus + 8 move + float, 8 grow + 8 shrink resize steps, and the parent
//! orientation toggle; Ctrl-role modifier defaulting to Ctrl).
//!
//! Presets mirror the Windows `Preset::Authentic` / `Preset::Compatible`
//! contract ([`apply semantics`]: Authentic keeps every supported chord,
//! Compatible disables known OS-conflicting chords, neither invents
//! replacements):
//! - Authentic consumes the exact supported chords while running (tap `Drop`
//!   suppression, no persistent native-setting writes — notably no
//!   `com.apple.symbolichotkeys` writes anywhere in this crate).
//! - Compatible leaves known conflicting chords to macOS by not arming them
//!   (they pass through natively): under the Ctrl Meta that is the 4
//!   unshifted no-Alt arrow chords (Mission Control Ctrl+arrows are the
//!   repo-established expected conflict; `Ctrl+Alt+arrows` resize chords and
//!   `Ctrl+Shift+arrows` move chords are distinct effective chords with no
//!   established holder, so Compatible keeps them; there are no
//!   digit/workspace bindings to conflict with Ctrl+digits by construction).
//!   Other Metas have no repo-established conflicts, so Compatible keeps the
//!   full set there rather than inventing holders. A `None` conflict answer
//!   means "no established holder", never "proven conflict-free".
//!
//! Disable recovery is a stateless table over the two native disable kinds
//! plus the owner-side enabled readback (each observed disable re-enables
//! the same tap when it verifies with an enabled readback, an unverified
//! re-enable installs a fresh tap, and only a hard creation denial refuses
//! — a notification name alone never proves consent revocation, and the
//! surveyed skhd tap-disabled restart re-enables both kinds). Native owners
//! consult [`disable_action`] / [`unverified_reenable_action`] /
//! [`unnotified_disable_action`] exhaustively; counters ride
//! [`crate::ffi::TapSourceReport`].
//!
//! Eligibility gating (scoped foreground/Space/show-state) and the no-raw-key
//! rule (counters and static tokens only, never key codes/flags/titles) are
//! unchanged and live in [`crate::native_input`]; this module only selects
//! which catalogue chords arm.

use serde::{Deserialize, Serialize};

use crate::config::ProbeModifier;
use crate::runtime_types::TileBinding;

/// Mac virtual keycodes for the basic catalogue (`CGKeyCode` space, US
/// layout positions for letters). Single home; [`crate::native_input`]
/// re-exports these for the catalogue and matcher tables.
pub const KEY_H: u32 = 0x04;
pub const KEY_J: u32 = 0x26;
pub const KEY_K: u32 = 0x28;
pub const KEY_L: u32 = 0x25;
pub const KEY_G: u32 = 0x05;
pub const KEY_O: u32 = 0x1F;
pub const KEY_LEFT: u32 = 0x7B;
pub const KEY_RIGHT: u32 = 0x7C;
pub const KEY_DOWN: u32 = 0x7D;
pub const KEY_UP: u32 = 0x7E;

/// The exact basic key set: HJKL letters, four arrows, G, and O. No digits,
/// so no preset can swallow native Ctrl+digits (no workspace bindings
/// exist).
const BASIC_KEYCODES: [u32; 10] = [
    KEY_H, KEY_J, KEY_K, KEY_L, KEY_G, KEY_O, KEY_LEFT, KEY_RIGHT, KEY_DOWN, KEY_UP,
];

/// Arrow keycodes: the Mission Control Ctrl+arrow conflict surface.
const ARROW_KEYCODES: [u32; 4] = [KEY_LEFT, KEY_RIGHT, KEY_DOWN, KEY_UP];

/// Which interception preset the product tile arms. Defaults to Authentic
/// (the selected product behavior: own conflicting window-management
/// shortcuts while running, with no persistent native writes).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InputPreset {
    /// Consume every supported exact chord while running.
    #[default]
    Authentic,
    /// Leave known conflicting chords to macOS (unarmed, passing through).
    Compatible,
}

impl InputPreset {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s.to_ascii_lowercase().as_str() {
            "authentic" => Ok(Self::Authentic),
            "compatible" => Ok(Self::Compatible),
            other => Err(format!(
                "unknown --preset '{other}' (expected authentic|compatible)"
            )),
        }
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Authentic => "authentic",
            Self::Compatible => "compatible",
        }
    }
}

/// Known native conflict for one EFFECTIVE chord, if any. Computed on the
/// full effective chord (post `--ctrl` substitution plus the binding's Shift
/// and Alt arms), never a hardcoded Ctrl or a blanket keycode assumption:
/// only unshifted arrow chords conflict when the effective modifier is Ctrl
/// WITHOUT the Alt arm (Mission Control Ctrl+arrows, the repo-established
/// expected conflict — `Ctrl+Alt+arrows` resize chords and `Ctrl+Shift+arrows`
/// move chords are distinct effective chords with no established holder, so
/// Compatible keeps them). Letters, G, and O have no established native
/// holder under any Meta, and non-Ctrl Metas have no established arrow
/// conflict either — those return `None` ("no established holder", never a
/// proven-conflict-free claim) rather than inventing a holder.
#[must_use]
pub fn conflict_reason_for(
    key_code: u32,
    shift: bool,
    alt: bool,
    modifier: ProbeModifier,
) -> Option<&'static str> {
    if modifier == ProbeModifier::Ctrl && !shift && !alt && ARROW_KEYCODES.contains(&key_code) {
        Some("Mission Control Ctrl+arrow; left to macOS under Compatible")
    } else {
        None
    }
}

/// The armed chord set for one product run: Authentic keeps the whole
/// supported catalogue; Compatible drops chords with a known native conflict
/// (unarmed chords pass through to macOS natively — disabling, never
/// rebinding, inventing no replacements). Returns the bindings in catalogue
/// order; callers validate the result through
/// [`crate::runtime_types::RuntimeConfig::validate`] as usual.
#[must_use]
pub fn effective_bindings(
    preset: InputPreset,
    modifier: ProbeModifier,
    catalogue: &[TileBinding],
) -> Vec<TileBinding> {
    match preset {
        InputPreset::Authentic => catalogue.to_vec(),
        InputPreset::Compatible => catalogue
            .iter()
            .filter(|binding| {
                conflict_reason_for(binding.key_code, binding.shift, binding.alt, modifier)
                    .is_none()
            })
            .copied()
            .collect(),
    }
}

/// True only for keycodes in the supported basic set (letters/arrows/G/O).
/// Guards against speculative workspace/digit bindings entering a preset.
#[must_use]
pub fn is_basic_keycode(key_code: u32) -> bool {
    BASIC_KEYCODES.contains(&key_code)
}

/// One native tap-disable kind, as observed by the tap callback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisableKind {
    /// The window server disabled a slow tap (`TapDisabledByTimeout`).
    Timeout,
    /// The tap was disabled by user input / Secure Input
    /// (`TapDisabledByUserInput`). Transient delivery pause only: the owner
    /// releases interception while the lock / Secure Input / eligibility
    /// gates report blocked and resumes or recreates when eligible. The
    /// notification name alone never proves consent revocation (the surveyed
    /// skhd tap-disabled restart re-enables both kinds).
    UserInput,
}

/// What the owner must do about one observed disable. The table is total:
/// every observed disable re-enables the same tap when it verifies with an
/// enabled readback (counted under its own kind); an unverified re-enable
/// installs a fresh tap via [`unverified_reenable_action`]. Only a hard
/// creation denial (for example revoked Accessibility consent) refuses —
/// never a transient notification alone, never silently stopping tiling.
#[must_use]
pub const fn disable_action(kind: DisableKind) -> DisableAction {
    match kind {
        DisableKind::Timeout => DisableAction::ReenableSameTap,
        DisableKind::UserInput => DisableAction::ReenableSameTap,
    }
}

/// What the owner must do on a bounded pump turn that reported NO OS
/// notification yet reads directly disabled (`CGEventTapIsEnabled` false):
/// re-enable the same tap when it verifies, else install a fresh tap (same
/// chords, same eligibility, same owner thread). Returns `None` while the
/// tap reads enabled (healthy) or while `explicit_fixture_hold` is set —
/// the explicit direct-disable-hold / re-arm fixture owns its deliberate
/// disabled observation and release, and the health check must never fight
/// it. Production code never sets that hold, so the bypass is structurally
/// fixture-only. An unnotified recovery counts under
/// `unnotified_disables`, never as a timeout.
#[must_use]
pub const fn unnotified_disable_action(
    direct_enabled: bool,
    explicit_fixture_hold: bool,
) -> Option<DisableAction> {
    if explicit_fixture_hold || direct_enabled {
        None
    } else {
        Some(DisableAction::ReenableSameTap)
    }
}

/// What the owner must do when a same-tap re-enable does not verify
/// (`CGEventTapIsEnabled` still false afterwards): install a FRESH tap over
/// the same chords on the same owner thread (never a second same-tap
/// enable). A fresh creation refusal fails closed with the refusal.
#[must_use]
pub const fn unverified_reenable_action() -> DisableAction {
    DisableAction::RecreateFresh
}

/// Owner action for one disable observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisableAction {
    /// Re-enable the same tap, then verify with an enabled readback before
    /// counting (an unverified re-enable falls through to
    /// [`unverified_reenable_action`]).
    ReenableSameTap,
    /// Drop the old guard without a same-tap re-enable and install a fresh
    /// tap over the same chords on the same owner thread.
    RecreateFresh,
    /// Record a truthful refusal and end the run; never touch the tap.
    /// Reserved for hard creation/permission denials (for example revoked
    /// Accessibility consent) — never for a transient disable notification
    /// alone.
    RefuseUserDisabled,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_input::tile_catalogue;
    use crate::runtime::InputDirection;
    use crate::runtime_types::InputSemanticAction;

    fn bindings() -> Vec<TileBinding> {
        tile_catalogue()
    }

    #[test]
    fn preset_parses_and_defaults_to_authentic() {
        assert_eq!(InputPreset::parse("authentic"), Ok(InputPreset::Authentic));
        assert_eq!(
            InputPreset::parse("Compatible"),
            Ok(InputPreset::Compatible)
        );
        assert!(InputPreset::parse("keep").is_err());
        assert_eq!(InputPreset::default(), InputPreset::Authentic);
        assert_eq!(InputPreset::Authentic.as_str(), "authentic");
        assert_eq!(InputPreset::Compatible.as_str(), "compatible");
    }

    #[test]
    fn authentic_consumes_the_whole_supported_catalogue() {
        for modifier in [
            ProbeModifier::Ctrl,
            ProbeModifier::Meta,
            ProbeModifier::Alt,
            ProbeModifier::Shift,
        ] {
            let effective = effective_bindings(InputPreset::Authentic, modifier, &bindings());
            assert_eq!(effective, bindings(), "modifier={modifier:?}");
        }
    }

    #[test]
    fn compatible_under_ctrl_leaves_only_unshifted_arrows_to_macos() {
        let effective =
            effective_bindings(InputPreset::Compatible, ProbeModifier::Ctrl, &bindings());
        // 8 focus + 8 move chords on letters, float, orientation, all 16
        // Alt-arm resize chords (`Ctrl+Alt+arrows` are distinct effective
        // chords with no established holder), plus the 4 shifted no-Alt
        // arrow move chords (no established holder for the shifted moves
        // either); only the 4 unshifted no-Alt arrow chords pass through.
        // Authentic 34, Compatible 30.
        assert_eq!(bindings().len(), 34);
        assert_eq!(effective.len(), 30);
        for key in [KEY_H, KEY_J, KEY_K, KEY_L, KEY_G, KEY_O] {
            assert!(
                effective
                    .iter()
                    .any(|b| b.key_code == key && !b.shift && !b.alt)
            );
        }
        for key in [KEY_H, KEY_J, KEY_K, KEY_L] {
            assert!(
                effective
                    .iter()
                    .any(|b| b.key_code == key && b.shift && !b.alt)
            );
            assert!(
                effective
                    .iter()
                    .any(|b| b.key_code == key && !b.shift && b.alt)
            );
            assert!(
                effective
                    .iter()
                    .any(|b| b.key_code == key && b.shift && b.alt)
            );
        }
        // Resize-arm arrows survive Compatible exactly like resize letters.
        for key in ARROW_KEYCODES {
            assert!(
                effective
                    .iter()
                    .any(|b| b.key_code == key && !b.shift && b.alt)
            );
            assert!(
                effective
                    .iter()
                    .any(|b| b.key_code == key && b.shift && b.alt)
            );
        }
        // Shifted no-Alt arrows (move arm) survive: no established holder.
        for key in ARROW_KEYCODES {
            assert!(
                effective
                    .iter()
                    .any(|b| b.key_code == key && b.shift && !b.alt),
                "shifted move arrow {key:#x} must stay armed"
            );
            assert!(
                !effective
                    .iter()
                    .any(|b| b.key_code == key && !b.shift && !b.alt),
                "unshifted arrow {key:#x} passes through to macOS"
            );
        }
        // Only unshifted no-Alt arrows conflict; the Shift (move) arm and
        // the Alt arms never do.
        for key in ARROW_KEYCODES {
            assert_eq!(
                conflict_reason_for(key, false, false, ProbeModifier::Ctrl),
                Some("Mission Control Ctrl+arrow; left to macOS under Compatible")
            );
            for (shift, alt) in [(true, false), (false, true), (true, true)] {
                assert_eq!(
                    conflict_reason_for(key, shift, alt, ProbeModifier::Ctrl),
                    None,
                    "shift={shift} alt={alt}: no established holder"
                );
            }
        }
    }

    #[test]
    fn compatible_invents_no_conflicts_for_other_metas() {
        // No repo-established native holders under non-Ctrl Metas: the full
        // set stays armed rather than guessing displacements.
        for modifier in [
            ProbeModifier::Meta,
            ProbeModifier::Alt,
            ProbeModifier::Shift,
        ] {
            let effective = effective_bindings(InputPreset::Compatible, modifier, &bindings());
            assert_eq!(effective, bindings(), "modifier={modifier:?}");
        }
    }

    #[test]
    fn effective_sets_hold_no_workspace_or_effect_bindings() {
        // Supported actions only: focus/move steps plus float. No digits
        // (native Ctrl+digits never swallowed), no foreign keycodes.
        for preset in [InputPreset::Authentic, InputPreset::Compatible] {
            for modifier in [
                ProbeModifier::Ctrl,
                ProbeModifier::Meta,
                ProbeModifier::Alt,
                ProbeModifier::Shift,
            ] {
                let effective = effective_bindings(preset, modifier, &bindings());
                assert!(!effective.is_empty(), "{preset:?}/{modifier:?}");
                for binding in &effective {
                    assert!(
                        is_basic_keycode(binding.key_code),
                        "{preset:?}/{modifier:?}: unexpected key {:#x}",
                        binding.key_code
                    );
                    assert!(matches!(
                        binding.action,
                        InputSemanticAction::FocusStep { .. }
                            | InputSemanticAction::MoveStep { .. }
                            | InputSemanticAction::ResizeStep { .. }
                            | InputSemanticAction::ToggleFloat
                            | InputSemanticAction::ToggleOrientation
                    ));
                }
            }
        }
    }

    #[test]
    fn effective_sets_stay_duplicate_free_under_every_modifier() {
        // The runtime validator refuses effective-chord collisions; presets
        // must never introduce one (Compatible only removes chords).
        for preset in [InputPreset::Authentic, InputPreset::Compatible] {
            for modifier in [ProbeModifier::Ctrl, ProbeModifier::Meta] {
                let config = crate::runtime_types::RuntimeConfig {
                    modifier,
                    tile_bindings: Some(effective_bindings(preset, modifier, &bindings())),
                    ..crate::runtime_types::RuntimeConfig::default()
                };
                assert!(config.validate().is_ok(), "{preset:?}/{modifier:?}");
            }
        }
        // Pre-existing degeneracy, unchanged here: Shift-as-Meta collapses
        // the catalogue's Shift (move) arm onto the focus arm, so the
        // validator refuses it — for Authentic and for every Compatible
        // subset alike. Shift was never a usable tile Meta.
        //
        // Alt-as-Meta collapses the same way through the Alt (resize) arm:
        // grow (`Alt+Alt+H`) lands on the focus chord (`Alt+H`), so the
        // validator refuses it too. Only Ctrl (default) and Meta (Cmd)
        // carry the full catalogue distinctly.
        for preset in [InputPreset::Authentic, InputPreset::Compatible] {
            for modifier in [ProbeModifier::Shift, ProbeModifier::Alt] {
                let config = crate::runtime_types::RuntimeConfig {
                    modifier,
                    tile_bindings: Some(effective_bindings(preset, modifier, &bindings())),
                    ..crate::runtime_types::RuntimeConfig::default()
                };
                assert!(config.validate().is_err(), "{preset:?}/{modifier:?}");
            }
        }
        // A deliberately duplicated effective chord still refuses.
        let mut dup = bindings();
        dup.push(TileBinding {
            key_code: KEY_H,
            shift: false,
            alt: false,
            action: InputSemanticAction::FocusStep {
                direction: InputDirection::Left,
            },
        });
        let config = crate::runtime_types::RuntimeConfig {
            modifier: ProbeModifier::Ctrl,
            tile_bindings: Some(dup),
            ..crate::runtime_types::RuntimeConfig::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn disable_table_reenables_both_kinds_and_recreates_when_unverified() {
        // Both native notification kinds recover the same way: re-enable
        // the same tap when it verifies with an enabled readback (the enum
        // name alone never proves consent revocation — the surveyed skhd
        // tap-disabled restart re-enables both kinds). Only an unverified
        // re-enable escalates to a fresh tap, and only a hard creation
        // denial refuses.
        assert_eq!(
            disable_action(DisableKind::Timeout),
            DisableAction::ReenableSameTap
        );
        assert_eq!(
            disable_action(DisableKind::UserInput),
            DisableAction::ReenableSameTap
        );
        // Even after timeouts, a user-input disable still recovers — never
        // a terminal refusal for the notification alone.
        assert_eq!(
            disable_action(DisableKind::UserInput),
            DisableAction::ReenableSameTap
        );
        assert_eq!(unverified_reenable_action(), DisableAction::RecreateFresh);
    }

    #[test]
    fn unnotified_health_action_recovers_only_outside_the_fixture_hold() {
        // Healthy readback: nothing to do (neither a timeout nor a user
        // notification is claimed).
        assert_eq!(unnotified_disable_action(true, false), None);
        assert_eq!(unnotified_disable_action(true, true), None);
        // Directly disabled with no OS notification and no fixture hold:
        // re-enable the same tap when it verifies (counted as
        // unnotified-disabled, never as a timeout).
        assert_eq!(
            unnotified_disable_action(false, false),
            Some(DisableAction::ReenableSameTap)
        );
        // Explicit fixture hold owns its deliberate disabled observation
        // and release: the production health check bypasses it (production
        // never sets the hold, so the bypass is structurally fixture-only).
        assert_eq!(unnotified_disable_action(false, true), None);
    }

    #[test]
    fn basic_keycodes_cover_letters_arrows_float_and_orientation() {
        for key in [KEY_H, KEY_J, KEY_K, KEY_L] {
            assert!(is_basic_keycode(key));
        }
        for key in [KEY_LEFT, KEY_RIGHT, KEY_DOWN, KEY_UP] {
            assert!(is_basic_keycode(key));
        }
        assert!(is_basic_keycode(KEY_G));
        assert!(is_basic_keycode(KEY_O));
        assert!(!is_basic_keycode(0x00));
        assert!(!is_basic_keycode(0x12));
    }
}

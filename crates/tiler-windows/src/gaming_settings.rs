#![forbid(unsafe_code)]
//! Portable Settings presentation/action policy for gaming conflicts.
//!
//! Xbox mode and Game Bar own the authentic `Win+F11` fullscreen and `Win+G`
//! float chords. This module is the offline policy behind the existing
//! Settings binding-info area: which selected row carries an effective gaming
//! conflict, what honest detail text it shows, and which Settings-only actions
//! it offers. No registry/package reads or launches here; native reads live
//! in `tray_sys::gaming` (`cfg(windows)` only) and navigation runs through
//! the allowlisted URIs in [`crate::gaming`]. Unit tests inject snapshots,
//! never the OS.

use crate::gaming::{
    APP_SETTINGS_URI, GAMEBAR_SETTINGS_URI, GamingConflict, GamingConflictKind, GamingSnapshot,
    XBOX_SETTINGS_URI, XboxConfig, gamebar_app_settings_uri, gaming_conflicts,
    is_allowed_navigation_uri, is_gamebar_note, is_xbox_mode_note, tray_warning_conflicts,
    xbox_evidence,
};
use crate::settings::Settings;

// ---------------------------------------------------------------------------
// Manual paths and safety copy (plain text, no launches).
// ---------------------------------------------------------------------------

/// Manual path for Xbox mode (PC plus handheld variant).
pub const XBOX_MANUAL_PATH: &str =
    "Settings > Gaming > Xbox mode > Enable Xbox mode off (PC); handheld: Choose home app = None";
/// Manual path for Game Bar settings inspection.
pub const GAMEBAR_MANUAL_PATH: &str =
    "Settings > Gaming > Game Bar (inspection only; may not disable Win+G)";
/// Manual path for the Game Bar app page (user-owned uninstall, if offered).
pub const GAMEBAR_APP_MANUAL_PATH: &str = "Settings > Apps > Installed apps > Xbox Game Bar > Advanced options > Uninstall (if Windows offers it; availability varies)";

/// Settings-only resolve action for one gaming conflict.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GamingSettingsAction {
    OpenXboxSettings,
    OpenGameBarSettings,
    OpenGameBarAppSettings,
    Recheck,
}

impl GamingSettingsAction {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::OpenXboxSettings => "Open Xbox mode settings",
            Self::OpenGameBarSettings => "Open Game Bar settings",
            Self::OpenGameBarAppSettings => "Open Game Bar app settings",
            Self::Recheck => "Recheck gaming status",
        }
    }

    /// Allowlisted Settings URI for the action, if it navigates. Recheck
    /// navigates nowhere. Store reinstall stays plain instructional text in
    /// the detail below: no Store-launch URI is ever offered.
    #[must_use]
    pub fn uri(self) -> Option<String> {
        match self {
            Self::OpenXboxSettings => Some(XBOX_SETTINGS_URI.to_owned()),
            Self::OpenGameBarSettings => Some(GAMEBAR_SETTINGS_URI.to_owned()),
            Self::OpenGameBarAppSettings => Some(gamebar_app_settings_uri()),
            Self::Recheck => None,
        }
    }
}

/// Settings-only actions for one conflict kind, Recheck included. Xbox rows
/// offer only the Xbox page; Game Bar rows offer the settings page (as
/// inspection, not a guaranteed fix) plus the app-settings page (the
/// preferred user-owned removal route). No uninstall, registry/policy
/// write, or Store-launch action exists.
#[must_use]
pub const fn gaming_actions_for(kind: GamingConflictKind) -> &'static [GamingSettingsAction] {
    match kind {
        GamingConflictKind::XboxMode => &[
            GamingSettingsAction::OpenXboxSettings,
            GamingSettingsAction::Recheck,
        ],
        GamingConflictKind::GameBar => &[
            GamingSettingsAction::OpenGameBarSettings,
            GamingSettingsAction::OpenGameBarAppSettings,
            GamingSettingsAction::Recheck,
        ],
    }
}

/// The effective gaming conflict for the currently selected binding id, if
/// any. Effective chords only (never id/preset guesses): the row must carry
/// a live Game Bar or Xbox mode note under the draft plus takeover.
/// Takeover off, Compatible rebinds/disables, per-binding disable/rebind-away
/// and the Win+L opt-in warning (never a gaming kind) all yield `None`.
/// Recognised Xbox off clears the Xbox row; Game Bar package absence keeps a
/// residual `Win+G` check row.
#[must_use]
pub fn selected_gaming_conflict(
    settings: &Settings,
    takeover: bool,
    snapshot: &GamingSnapshot,
    selected_id: &str,
) -> Option<GamingConflict> {
    gaming_conflicts(settings, takeover, snapshot)
        .into_iter()
        .find(|conflict| conflict.id == selected_id)
}

/// Honest detail text appended to the existing binding-info area for one
/// effective conflict: categorical evidence, manual paths, feature and data
/// loss, reinstall limits, non-destructive alternatives, and the
/// navigation-never-resolves rule. Unknown states stay check-required and
/// never claim enabled, disabled, or resolved.
#[must_use]
pub fn gaming_detail_text(conflict: &GamingConflict) -> String {
    const SHARED_TAIL: &str = "Opening Settings never marks this resolved; Recheck or reopen to refresh evidence. A missing, redirected, or blocked page leaves this unresolved. To keep Windows gaming features, choose Compatible, rebind or disable this binding, or turn keyboard takeover off. No registry or policy change is offered here.";
    match conflict.kind {
        GamingConflictKind::XboxMode => format!(
            "Gaming: {} at {}. {XBOX_MANUAL_PATH}. Startup and Task View visibility controls are not disable controls; change this outside a game. Turning Xbox mode off removes the console-style experience and entry points. {SHARED_TAIL}",
            conflict.evidence, conflict.chord,
        ),
        GamingConflictKind::GameBar => format!(
            "Gaming: {} at {}. {GAMEBAR_MANUAL_PATH}. Controller and capture switches may not disable Win+G; recording off is not resolved. {GAMEBAR_APP_MANUAL_PATH}. Uninstalling may stop captures, widgets, party, or controller integrations; Windows removes the package's managed app data on uninstall, so preferences must be recreated. Reinstall through the Microsoft Store is not an exact settings rollback and may be unavailable or blocked. {SHARED_TAIL} Fallback apps page if needed: {APP_SETTINGS_URI}.",
            conflict.evidence, conflict.chord,
        ),
    }
}

/// Honest cleared-state note for a row whose generic catalog conflict names a
/// gaming owner but carries no active effective conflict: the recognised Xbox
/// off value for a still-bound F11-family chord, or keyboard takeover off
/// (every chord passes through natively, so there is no interception to
/// conflict with). Returns `None` for inactive/unbound rows, non-gaming
/// notes, and rows the caller already reports as active conflicts (check
/// [`selected_gaming_conflict`] first). Package absence never clears: the
/// residual `Win+G` check stays an active conflict, not this note.
pub const TAKEOVER_OFF_GAMING_NOTE: &str =
    "keyboard takeover off: chords pass through natively, no interception conflict";

/// Cleared-state gaming note over one effective row's primitives. Mirrors
/// [`crate::gaming::gaming_conflicts`] exactly (active plus effective rows
/// only), so the Settings list and detail never show an actionable gaming
/// warning the tray already cleared, while a recognised Xbox off still shows
/// its honest evidence for a bound F11 chord without offering actions.
#[must_use]
pub fn cleared_gaming_note(
    active: bool,
    effective: bool,
    note: Option<&str>,
    takeover: bool,
    xbox: XboxConfig,
) -> Option<&'static str> {
    if !active || !effective {
        return None;
    }
    let note = note?;
    if !takeover && (is_gamebar_note(note) || is_xbox_mode_note(note)) {
        return Some(TAKEOVER_OFF_GAMING_NOTE);
    }
    if takeover && is_xbox_mode_note(note) && xbox == XboxConfig::Disabled {
        return Some(xbox_evidence(XboxConfig::Disabled));
    }
    None
}

/// Badge counts over every effective row for the Settings global badge:
/// `(conflicts, warnings)`. Mirrors the tray warnings exactly
/// ([`tray_warning_conflicts`]): known-incomplete gaming containment plus
/// the Win+L opt-in warning while its chord stays effective, across all
/// rows (never just the selection) and all owner kinds (never just
/// gaming). Recognised Xbox off clears the Xbox row and takeover off
/// silences everything (pass-through by user choice), matching
/// [`cleared_gaming_note`]. Gaming containment notes count as conflicts;
/// the Win+L opt-in counts as a warning.
#[must_use]
pub fn badge_conflict_counts(
    settings: &Settings,
    takeover: bool,
    allow_win_l: bool,
    snapshot: &GamingSnapshot,
) -> (usize, usize) {
    let mut conflicts = 0;
    let mut warnings = 0;
    for row in tray_warning_conflicts(settings, takeover, allow_win_l, snapshot) {
        if is_gamebar_note(row.note) || is_xbox_mode_note(row.note) {
            conflicts += 1;
        } else {
            warnings += 1;
        }
    }
    (conflicts, warnings)
}

/// True when every action URI offered for a kind is on the navigation
/// allowlist (navigation never claims resolution; Recheck has no URI).
#[must_use]
pub fn gaming_actions_allowlisted(kind: GamingConflictKind) -> bool {
    gaming_actions_for(kind)
        .iter()
        .all(|action| match action.uri() {
            None => true,
            Some(uri) => is_allowed_navigation_uri(&uri),
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gaming::{GameBarPackage, XboxConfig, gamebar_evidence, xbox_evidence};
    use crate::settings::{BindingSetting, BindingState, Preset, apply_preset, validate_settings};

    fn authentic() -> Settings {
        Settings::default()
    }

    fn snapshot_with(xbox: XboxConfig, gamebar: GameBarPackage) -> GamingSnapshot {
        let mut snapshot = GamingSnapshot::unknown();
        snapshot.xbox = xbox;
        snapshot.gamebar = gamebar;
        snapshot
    }

    fn unknown_snapshot() -> GamingSnapshot {
        GamingSnapshot::unknown()
    }

    #[test]
    fn xbox_row_offers_only_xbox_page_plus_recheck() {
        let snapshot = unknown_snapshot();
        let conflict = selected_gaming_conflict(&authentic(), true, &snapshot, "toggle-fullscreen")
            .expect("fullscreen warns while unknown");
        assert_eq!(conflict.kind, GamingConflictKind::XboxMode);
        let actions = gaming_actions_for(conflict.kind);
        assert_eq!(
            actions,
            &[
                GamingSettingsAction::OpenXboxSettings,
                GamingSettingsAction::Recheck,
            ]
        );
        assert_eq!(
            GamingSettingsAction::OpenXboxSettings.uri().as_deref(),
            Some(XBOX_SETTINGS_URI)
        );
        assert_eq!(GamingSettingsAction::Recheck.uri(), None);
        assert!(gaming_actions_allowlisted(conflict.kind));
    }

    #[test]
    fn xbox_detail_states_manual_path_loss_and_never_resolved() {
        let snapshot = unknown_snapshot();
        let conflict = selected_gaming_conflict(&authentic(), true, &snapshot, "toggle-fullscreen")
            .expect("fullscreen warns");
        let detail = gaming_detail_text(&conflict);
        assert!(detail.contains(xbox_evidence(XboxConfig::Unknown)));
        assert!(detail.contains(XBOX_MANUAL_PATH));
        assert!(detail.contains("not disable controls"));
        assert!(detail.contains("removes the console-style experience"));
        assert!(detail.contains("never marks this resolved"));
        assert!(detail.contains("Compatible"));
        assert!(detail.contains("takeover off"));
        assert!(detail.contains("No registry or policy change"));
        assert!(!detail.contains("ms-windows-store://"));
    }

    #[test]
    fn gamebar_row_offers_settings_app_settings_plus_recheck() {
        let snapshot = unknown_snapshot();
        let conflict = selected_gaming_conflict(&authentic(), true, &snapshot, "toggle-float")
            .expect("float warns while unknown");
        assert_eq!(conflict.kind, GamingConflictKind::GameBar);
        let actions = gaming_actions_for(conflict.kind);
        assert_eq!(
            actions,
            &[
                GamingSettingsAction::OpenGameBarSettings,
                GamingSettingsAction::OpenGameBarAppSettings,
                GamingSettingsAction::Recheck,
            ]
        );
        assert_eq!(
            GamingSettingsAction::OpenGameBarSettings.uri().as_deref(),
            Some(GAMEBAR_SETTINGS_URI)
        );
        assert_eq!(
            GamingSettingsAction::OpenGameBarAppSettings.uri(),
            Some(gamebar_app_settings_uri())
        );
        assert!(gaming_actions_allowlisted(conflict.kind));
    }

    #[test]
    fn gamebar_detail_states_inspection_limits_loss_and_store_limits() {
        for gamebar in [
            GameBarPackage::Registered,
            GameBarPackage::Absent,
            GameBarPackage::Unknown,
        ] {
            let snapshot = snapshot_with(XboxConfig::Unknown, gamebar);
            let conflict = selected_gaming_conflict(&authentic(), true, &snapshot, "toggle-float")
                .expect("float always warns");
            let detail = gaming_detail_text(&conflict);
            assert!(detail.contains(gamebar_evidence(gamebar)), "{gamebar:?}");
            assert!(detail.contains(GAMEBAR_MANUAL_PATH));
            assert!(detail.contains(GAMEBAR_APP_MANUAL_PATH));
            assert!(detail.contains("may not disable Win+G"));
            assert!(detail.contains("recording off is not resolved"));
            assert!(detail.contains("captures, widgets, party, or controller"));
            assert!(detail.contains("managed app data"));
            assert!(detail.contains("not an exact settings rollback"));
            assert!(detail.contains("may be unavailable or blocked"));
            assert!(detail.contains("never marks this resolved"));
            assert!(detail.contains(APP_SETTINGS_URI));
            assert!(!detail.contains("ms-windows-store://"));
        }
    }

    #[test]
    fn package_absence_keeps_residual_float_check() {
        let snapshot = snapshot_with(XboxConfig::Disabled, GameBarPackage::Absent);
        let conflict = selected_gaming_conflict(&authentic(), true, &snapshot, "toggle-float")
            .expect("absence keeps the residual Win+G check");
        assert!(gaming_detail_text(&conflict).contains("still unproven"));
    }

    #[test]
    fn recognised_xbox_off_clears_only_fullscreen_row() {
        let snapshot = snapshot_with(XboxConfig::Disabled, GameBarPackage::Registered);
        assert!(
            selected_gaming_conflict(&authentic(), true, &snapshot, "toggle-fullscreen").is_none()
        );
        assert!(selected_gaming_conflict(&authentic(), true, &snapshot, "toggle-float").is_some());
    }

    #[test]
    fn non_gaming_rows_takeover_off_and_compatible_yield_none() {
        let unknown = unknown_snapshot();
        // Win+L opt-in warning is untouched: never a gaming conflict.
        assert!(selected_gaming_conflict(&authentic(), true, &unknown, "focus-right").is_none());
        assert!(selected_gaming_conflict(&authentic(), false, &unknown, "toggle-float").is_none());
        assert!(
            selected_gaming_conflict(&authentic(), false, &unknown, "toggle-fullscreen").is_none()
        );
        let mut compatible = authentic();
        apply_preset(&mut compatible, Preset::Compatible);
        assert!(validate_settings(&compatible).is_ok());
        assert!(selected_gaming_conflict(&compatible, true, &unknown, "toggle-float").is_none());
        assert!(
            selected_gaming_conflict(&compatible, true, &unknown, "toggle-fullscreen").is_none()
        );
    }

    #[test]
    fn per_binding_disable_and_rebind_away_clear_their_rows() {
        let unknown = unknown_snapshot();
        let mut disabled = authentic();
        disabled.bindings.insert(
            "toggle-float".to_owned(),
            BindingSetting {
                state: BindingState::Disabled,
                chord: None,
            },
        );
        assert!(validate_settings(&disabled).is_ok());
        assert!(selected_gaming_conflict(&disabled, true, &unknown, "toggle-float").is_none());
        assert!(selected_gaming_conflict(&disabled, true, &unknown, "toggle-fullscreen").is_some());

        let mut rebound = authentic();
        rebound.bindings.insert(
            "toggle-fullscreen".to_owned(),
            BindingSetting {
                state: BindingState::Rebind,
                chord: Some("Win+Q".to_owned()),
            },
        );
        assert!(validate_settings(&rebound).is_ok());
        assert!(selected_gaming_conflict(&rebound, true, &unknown, "toggle-fullscreen").is_none());
    }

    #[test]
    fn rebound_xbox_chord_tracks_note_not_id() {
        // Free Win+F11 from fullscreen onto maximize: the Xbox row follows the
        // note to the new owner.
        let mut rebound = authentic();
        rebound.bindings.insert(
            "toggle-fullscreen".to_owned(),
            BindingSetting {
                state: BindingState::Disabled,
                chord: None,
            },
        );
        rebound.bindings.insert(
            "toggle-maximize".to_owned(),
            BindingSetting {
                state: BindingState::Rebind,
                chord: Some("Win+F11".to_owned()),
            },
        );
        assert!(validate_settings(&rebound).is_ok());
        let unknown = unknown_snapshot();
        let conflict = selected_gaming_conflict(&rebound, true, &unknown, "toggle-maximize")
            .expect("note follows the rebound chord");
        assert_eq!(conflict.kind, GamingConflictKind::XboxMode);
        assert_eq!(conflict.chord, "Win+F11");
        assert!(selected_gaming_conflict(&rebound, true, &unknown, "toggle-fullscreen").is_none());
        // Recognised off clears the rebound row too, consistently with tray.
        let off = snapshot_with(XboxConfig::Disabled, GameBarPackage::Registered);
        assert!(selected_gaming_conflict(&rebound, true, &off, "toggle-maximize").is_none());
    }

    #[test]
    fn no_action_offers_store_uninstall_or_registry_writes() {
        for kind in [GamingConflictKind::XboxMode, GamingConflictKind::GameBar] {
            for action in gaming_actions_for(kind) {
                let uri = action.uri();
                if let Some(uri) = uri.as_deref() {
                    assert!(!uri.contains("ms-windows-store"));
                    assert!(!uri.contains("store"));
                    assert!(is_allowed_navigation_uri(uri));
                }
                assert!(!action.label().contains("Uninstall"));
                assert!(!action.label().contains("Store"));
            }
            // Detail prose names the Store only as plain reinstall explanation
            // (no launch URI) and discloses the no-write boundary.
            let snapshot = unknown_snapshot();
            let id = match kind {
                GamingConflictKind::XboxMode => "toggle-fullscreen",
                GamingConflictKind::GameBar => "toggle-float",
            };
            let conflict =
                selected_gaming_conflict(&authentic(), true, &snapshot, id).expect("warns");
            let detail = gaming_detail_text(&conflict);
            assert!(!detail.contains("ms-windows-store://"));
            assert!(detail.contains("No registry or policy change"));
        }
    }

    #[test]
    fn cleared_note_covers_xbox_off_and_takeover_off_only() {
        use crate::gaming::{is_gamebar_note, is_xbox_mode_note};
        let xbox_note = "Xbox mode owns Win+F11; containment is incomplete";
        let gamebar_note =
            "Xbox Game Bar owns Win+G; the hook cannot fully contain the OS override";
        assert!(is_xbox_mode_note(xbox_note));
        assert!(is_gamebar_note(gamebar_note));
        // Recognised Xbox off clears a bound F11 row with honest evidence.
        assert_eq!(
            cleared_gaming_note(true, true, Some(xbox_note), true, XboxConfig::Disabled),
            Some(xbox_evidence(XboxConfig::Disabled))
        );
        // Takeover off clears either gaming row as pass-through.
        for note in [xbox_note, gamebar_note] {
            assert_eq!(
                cleared_gaming_note(true, true, Some(note), false, XboxConfig::Unknown),
                Some(TAKEOVER_OFF_GAMING_NOTE)
            );
        }
        // No cleared note while a conflict is still active, for inactive or
        // uneffective rows, without a note, or for non-gaming owners.
        assert_eq!(
            cleared_gaming_note(true, true, Some(xbox_note), true, XboxConfig::Unknown),
            None
        );
        assert_eq!(
            cleared_gaming_note(true, true, Some(gamebar_note), true, XboxConfig::Disabled),
            None
        );
        assert_eq!(
            cleared_gaming_note(false, true, Some(xbox_note), false, XboxConfig::Unknown),
            None
        );
        assert_eq!(
            cleared_gaming_note(true, false, Some(xbox_note), false, XboxConfig::Unknown),
            None
        );
        assert_eq!(
            cleared_gaming_note(true, true, None, false, XboxConfig::Unknown),
            None
        );
        assert_eq!(
            cleared_gaming_note(
                true,
                true,
                Some("Snap owns Win+Left; override needs takeover"),
                false,
                XboxConfig::Unknown
            ),
            None
        );
    }

    #[test]
    fn cleared_composition_matches_tray_for_off_and_takeover_off() {
        // Xbox off: fullscreen cleared with evidence, float keeps its active
        // residual conflict; the cleared note never fires for the active row.
        let off = snapshot_with(XboxConfig::Disabled, GameBarPackage::Registered);
        assert!(selected_gaming_conflict(&authentic(), true, &off, "toggle-fullscreen").is_none());
        let rows = crate::settings::effective_bindings(&authentic());
        let row = rows
            .iter()
            .find(|row| row.id == "toggle-fullscreen")
            .expect("fullscreen row");
        assert_eq!(
            cleared_gaming_note(row.active, row.effective, row.conflict, true, off.xbox),
            Some(xbox_evidence(XboxConfig::Disabled))
        );
        let float = rows
            .iter()
            .find(|row| row.id == "toggle-float")
            .expect("float row");
        assert!(selected_gaming_conflict(&authentic(), true, &off, "toggle-float").is_some());
        assert_eq!(
            cleared_gaming_note(
                float.active,
                float.effective,
                float.conflict,
                true,
                off.xbox
            ),
            None
        );
        // Takeover off: no active conflicts, both gaming rows cleared.
        let unknown = unknown_snapshot();
        for id in ["toggle-float", "toggle-fullscreen"] {
            assert!(selected_gaming_conflict(&authentic(), false, &unknown, id).is_none());
            let row = rows.iter().find(|row| row.id == id).expect("row");
            assert_eq!(
                cleared_gaming_note(row.active, row.effective, row.conflict, false, unknown.xbox),
                Some(TAKEOVER_OFF_GAMING_NOTE)
            );
        }
    }

    #[test]
    fn badge_counts_cover_every_row_not_just_the_selection() {
        // Authentic draft, takeover on, Win+L opt-in off, unknown
        // snapshot: the two gaming rows count as conflicts however the
        // list selection sits; nothing else warns.
        let unknown = unknown_snapshot();
        assert_eq!(
            badge_conflict_counts(&authentic(), true, false, &unknown),
            (2, 0)
        );
        // Win+L opt-in on surfaces the lock-chord warning beside them.
        assert_eq!(
            badge_conflict_counts(&authentic(), true, true, &unknown),
            (2, 1)
        );
        // Recognised Xbox off clears only the fullscreen conflict; the
        // Game Bar residual and the Win+L warning stay badged.
        let off = snapshot_with(XboxConfig::Disabled, GameBarPackage::Registered);
        assert_eq!(
            badge_conflict_counts(&authentic(), true, true, &off),
            (1, 1)
        );
        // Takeover off silences everything: pass-through is user choice.
        assert_eq!(
            badge_conflict_counts(&authentic(), false, true, &unknown),
            (0, 0)
        );
        // Disabling the float row drops its conflict; fullscreen still warns.
        let mut disabled = authentic();
        disabled.bindings.insert(
            "toggle-float".to_owned(),
            BindingSetting {
                state: BindingState::Disabled,
                chord: None,
            },
        );
        assert!(validate_settings(&disabled).is_ok());
        assert_eq!(
            badge_conflict_counts(&disabled, true, false, &unknown),
            (1, 0)
        );
    }
}

#![forbid(unsafe_code)]
//! Owned modal dialogs for the Settings window (Apply/Revert).
//!
//! Pure policy (portable, tested) plus safe wrappers over the Win32
//! owned-modal implementation in [`crate::ffi::settings_dialog`]
//! (`cfg(windows)` only). The native dialogs are exact owned popups of the
//! Settings window: no `MessageBoxW`, no title search. The owner is disabled
//! while a dialog runs and re-enabled on every exit; the modal loop pumps
//! the thread queue, answers `Escape`/close as cancel, preserves a pending
//! `WM_QUIT` for the outer Settings loop, and exits promptly when the owner
//! goes away. Keyboard traversal uses `IsDialogMessageW` on the dialog.
//!
//! Conflict semantics reuse the tray warnings exactly
//! (`tray_warning_conflicts` over `effective_bindings`): the preview lists
//! the draft's current warning set in catalog order, never a separate
//! documented-owner inventory.

use windows_sys::Win32::Foundation::HWND;

use crate::gaming::GamingSnapshot;
use crate::settings::Settings;
use crate::tray::UnresolvedConflict;

pub use crate::ffi::settings_dialog::{DiscardEnd, PreviewEnd};

/// Title for every Apply dialog (preview and result).
pub const APPLY_TITLE: &str = "Apply Settings";
/// Title for every Revert dialog (discard confirm and result).
pub const REVERT_TITLE: &str = "Revert Settings";
/// Short Apply success body shown in the result dialog.
pub const APPLY_SUCCESS: &str = "Settings saved.";
/// Short Revert success body shown in the result dialog.
pub const REVERT_SUCCESS: &str = "Reverted to saved settings.";
/// Discard-confirm body: shown only when unsaved state would be lost.
pub const DISCARD_PROMPT: &str = "Discard unsaved edits?";
/// Preview intro line above the readonly conflict list.
pub const PREVIEW_INTRO: &str =
    "These shortcuts conflict with Windows. Apply anyway? (Cancel keeps your draft.)";
/// Single-line cap for concise dialog error reasons.
pub const MAX_REASON_CHARS: usize = 240;

/// Collapse to one line and cap at `cap` characters (by char, not byte).
/// Newlines, carriage returns and tabs become single spaces; surrounding
/// whitespace is trimmed; longer input truncates on a char boundary.
#[must_use]
pub fn single_line_capped(text: &str, cap: usize) -> String {
    let flat = text.replace("\r\n", "\n").replace(['\r', '\n', '\t'], " ");
    let collapsed: String = flat.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() > cap {
        collapsed.chars().take(cap).collect()
    } else {
        collapsed
    }
}

/// Concise single-line dialog reason, capped at [`MAX_REASON_CHARS`].
#[must_use]
pub fn short_reason(reason: &str) -> String {
    single_line_capped(reason, MAX_REASON_CHARS)
}

/// Concise error dialog body; the full diagnostic stays on the status line.
#[must_use]
pub fn error_body(reason: &str) -> String {
    format!("Error: {}", short_reason(reason))
}

/// True when any unsaved channel holds edits: HWND-dirty flag, staged
/// `work != base` drift, or a still-typed pending chord.
#[must_use]
pub const fn has_unsaved_edits(dirty: bool, work_drift: bool, pending: bool) -> bool {
    dirty || work_drift || pending
}

/// Apply routing: a nonempty draft warning set gates on one preview.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyGate {
    Direct,
    Preview,
}

#[must_use]
pub const fn apply_gate(conflict_count: usize) -> ApplyGate {
    if conflict_count == 0 {
        ApplyGate::Direct
    } else {
        ApplyGate::Preview
    }
}

/// Revert routing: unsaved state needs one discard confirm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RevertGate {
    Proceed,
    ConfirmDiscard,
}

#[must_use]
pub const fn revert_gate(has_unsaved: bool) -> RevertGate {
    if has_unsaved {
        RevertGate::ConfirmDiscard
    } else {
        RevertGate::Proceed
    }
}

/// The draft's current tray-warning set: `effective_bindings` rows filtered
/// through the existing `tray_warning_conflicts` semantics (takeover-gated,
/// cleared-row aware, snapshot-aware). Catalog order is preserved.
#[must_use]
pub fn preview_conflicts(
    settings: &Settings,
    takeover: bool,
    allow_win_l: bool,
    snapshot: &GamingSnapshot,
) -> Vec<UnresolvedConflict> {
    crate::gaming::tray_warning_conflicts(settings, takeover, allow_win_l, snapshot)
}

/// Readonly preview body: intro plus one stable `id [chord] -- note` line
/// per conflict in the given (catalog) order. Empty input yields the intro
/// only; callers gate on [`apply_gate`] so the preview never opens empty.
#[must_use]
pub fn preview_body(conflicts: &[UnresolvedConflict]) -> String {
    let mut out = PREVIEW_INTRO.to_owned();
    for conflict in conflicts {
        out.push('\n');
        out.push_str(&format!(
            "{} [{}] -- {}",
            conflict.id, conflict.chord, conflict.note
        ));
    }
    out
}

#[allow(dead_code)]
pub fn dialog_open() -> bool {
    crate::ffi::settings_dialog::dialog_open()
}

#[allow(dead_code)]
pub fn defer_close() {
    crate::ffi::settings_dialog::defer_close();
}

#[allow(dead_code)]
pub fn take_close_deferred() -> bool {
    crate::ffi::settings_dialog::take_close_deferred()
}

pub fn ask_preview(owner: HWND, body: &str) -> PreviewEnd {
    crate::ffi::settings_dialog::ask_preview(owner, body, APPLY_TITLE)
}

pub fn ask_discard(owner: HWND, body: &str) -> DiscardEnd {
    crate::ffi::settings_dialog::ask_discard(owner, body, REVERT_TITLE)
}

pub fn show_info(owner: HWND, title: &str, body: &str) -> bool {
    crate::ffi::settings_dialog::show_info(owner, title, body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gaming::{GameBarOutcome, GameBarPackage, GamingSnapshot, XboxConfig, XboxOutcome};
    use crate::settings::{
        BindingSetting, BindingState, Settings, apply_preset, validate_settings,
    };

    fn unknown_snapshot() -> GamingSnapshot {
        GamingSnapshot {
            xbox: XboxConfig::Unknown,
            xbox_outcome: XboxOutcome::Absent,
            gamebar: GameBarPackage::Unknown,
            gamebar_outcome: GameBarOutcome::QueryFailed,
            generation: 0,
        }
    }

    fn known_snapshot() -> GamingSnapshot {
        GamingSnapshot {
            xbox: XboxConfig::Unknown,
            xbox_outcome: XboxOutcome::Absent,
            gamebar: GameBarPackage::Registered,
            gamebar_outcome: GameBarOutcome::Counted,
            generation: 1,
        }
    }

    fn proven_snapshot() -> GamingSnapshot {
        GamingSnapshot {
            xbox: XboxConfig::Enabled,
            xbox_outcome: XboxOutcome::Matched,
            gamebar: GameBarPackage::Registered,
            gamebar_outcome: GameBarOutcome::Counted,
            generation: 1,
        }
    }

    #[test]
    fn authentic_preview_lists_two_rows_in_catalog_order() {
        let settings = Settings::default();
        assert!(preview_conflicts(&settings, true, false, &unknown_snapshot()).is_empty());
        let conflicts = preview_conflicts(&settings, true, false, &proven_snapshot());
        let ids: Vec<&str> = conflicts.iter().map(|c| c.id).collect();
        assert_eq!(ids, vec!["toggle-float", "toggle-fullscreen"]);
        assert_eq!(apply_gate(conflicts.len()), ApplyGate::Preview);
        let body = preview_body(&conflicts);
        assert!(body.starts_with(PREVIEW_INTRO));
        let float_at = body.find("toggle-float").expect("float row");
        let full_at = body.find("toggle-fullscreen").expect("fullscreen row");
        assert!(float_at < full_at);
        for id in &ids {
            assert_eq!(body.matches(id).count(), 1, "{id} exactly once");
        }
        assert!(body.contains("Win+G"));
        assert!(body.contains("Win+F11"));
    }

    #[test]
    fn compatible_needs_no_preview() {
        let mut settings = Settings::default();
        apply_preset(&mut settings, crate::settings::Preset::Compatible);
        assert!(validate_settings(&settings).is_ok());
        let conflicts = preview_conflicts(&settings, true, false, &known_snapshot());
        assert!(conflicts.is_empty());
        assert_eq!(apply_gate(conflicts.len()), ApplyGate::Direct);
        assert_eq!(preview_body(&conflicts), PREVIEW_INTRO);
    }

    #[test]
    fn rebound_and_disabled_rows_leave_the_set() {
        let mut rebound = Settings::default();
        rebound.bindings.insert(
            "toggle-float".to_owned(),
            BindingSetting {
                state: BindingState::Rebind,
                chord: Some("Win+Q".to_owned()),
            },
        );
        assert!(validate_settings(&rebound).is_ok());
        let ids: Vec<&str> = preview_conflicts(&rebound, true, false, &proven_snapshot())
            .iter()
            .map(|c| c.id)
            .collect();
        assert_eq!(ids, vec!["toggle-fullscreen"]);
        let mut disabled = Settings::default();
        disabled.bindings.insert(
            "toggle-float".to_owned(),
            BindingSetting {
                state: BindingState::Disabled,
                chord: None,
            },
        );
        assert!(validate_settings(&disabled).is_ok());
        let ids: Vec<&str> = preview_conflicts(&disabled, true, false, &proven_snapshot())
            .iter()
            .map(|c| c.id)
            .collect();
        assert_eq!(ids, vec!["toggle-fullscreen"]);
        assert!(preview_conflicts(&rebound, true, false, &unknown_snapshot()).is_empty());
        assert!(preview_conflicts(&disabled, true, false, &unknown_snapshot()).is_empty());
    }

    #[test]
    fn takeover_off_silences_preview_and_win_l_stays_a_warning() {
        let settings = Settings::default();
        assert!(preview_conflicts(&settings, false, false, &unknown_snapshot()).is_empty());
        assert!(preview_conflicts(&settings, false, true, &unknown_snapshot()).is_empty());
        assert!(preview_conflicts(&settings, false, false, &proven_snapshot()).is_empty());
        assert!(preview_conflicts(&settings, false, true, &proven_snapshot()).is_empty());
        let unknown_lock = preview_conflicts(&settings, true, true, &unknown_snapshot());
        let ids: Vec<&str> = unknown_lock.iter().map(|c| c.id).collect();
        assert_eq!(ids, vec!["focus-right"]);
        let with_lock = preview_conflicts(&settings, true, true, &proven_snapshot());
        let ids: Vec<&str> = with_lock.iter().map(|c| c.id).collect();
        assert_eq!(
            ids,
            vec!["focus-right", "toggle-float", "toggle-fullscreen"]
        );
        let body = preview_body(&with_lock);
        assert!(body.contains("Win+L"));
        assert_eq!(revert_gate(true), RevertGate::ConfirmDiscard);
        assert_eq!(revert_gate(false), RevertGate::Proceed);
    }

    #[test]
    fn unsaved_detection_covers_full_draft_and_pending_edits() {
        assert!(!has_unsaved_edits(false, false, false));
        assert!(has_unsaved_edits(true, false, false));
        assert!(has_unsaved_edits(false, true, false));
        assert!(has_unsaved_edits(false, false, true));
        assert!(has_unsaved_edits(true, true, true));
        assert_eq!(
            revert_gate(has_unsaved_edits(false, true, false)),
            RevertGate::ConfirmDiscard
        );
        assert_eq!(
            revert_gate(has_unsaved_edits(false, false, false)),
            RevertGate::Proceed
        );
    }

    #[test]
    fn error_reason_is_single_line_capped() {
        let reason = "line one\nline two\r\nline three\twith tab plus trailing spaces   ";
        assert_eq!(
            short_reason(reason),
            "line one line two line three with tab plus trailing spaces"
        );
        assert!(!short_reason("a\nb").contains('\n'));
        let long = "x".repeat(400);
        assert_eq!(short_reason(&long).chars().count(), MAX_REASON_CHARS);
        assert_eq!(short_reason(&long), "x".repeat(MAX_REASON_CHARS));
        let body = error_body(&long);
        assert!(body.starts_with("Error: "));
        assert!(!body.contains('\n'));
        assert_eq!(APPLY_SUCCESS, "Settings saved.");
        assert_eq!(REVERT_SUCCESS, "Reverted to saved settings.");
        assert!(!APPLY_SUCCESS.to_lowercase().contains("applied"));
        assert!(!REVERT_SUCCESS.to_lowercase().contains("default"));
    }

    #[test]
    fn stale_and_save_reasons_sanitize_to_one_capped_line() {
        let stale = "Settings changed on disk (another window saved); reloaded. Re-apply your edits.\nSecond line.";
        let short = short_reason(stale);
        assert!(!short.contains('\n'));
        assert!(short.chars().count() <= MAX_REASON_CHARS);
        assert!(error_body("Settings file turned invalid (boom)").contains("boom"));
    }

    #[test]
    fn dead_owner_headless_reports_owner_gone_not_cancel() {
        use std::ptr::null_mut;
        assert_eq!(ask_preview(null_mut(), "body"), PreviewEnd::OwnerGone);
        assert_eq!(ask_discard(null_mut(), "body"), DiscardEnd::OwnerGone);
        assert!(!show_info(null_mut(), "title", "body"));
        assert!(!dialog_open());
    }

    #[test]
    fn deferred_close_drains_exactly_once() {
        assert!(!take_close_deferred());
        defer_close();
        assert!(take_close_deferred());
        assert!(!take_close_deferred());
    }
}

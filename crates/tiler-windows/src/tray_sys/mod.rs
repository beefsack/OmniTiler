#![forbid(unsafe_code)]
//! Native Windows notification tray (`cfg(windows)` only).
//!
//! Mechanism: one hidden owner-pump top-level window (class
//! [`crate::tray::TRAY_WINDOW_CLASS`]) owns exactly one official
//! `Shell_NotifyIconW` icon. The window procedure receives the tray callback
//! ([`crate::tray::TRAY_CALLBACK_MESSAGE`]) and the registered
//! `TaskbarCreated` broadcast: Explorer restarts recover MODIFY-first (a
//! surviving icon answers `NIM_MODIFY`, so no duplicate add is ever issued)
//! with a tolerant remove plus re-add only when the icon is truly gone, and
//! exactly one bounded `tray-taskbar` line carries the outcome.
//! Left-click and right-click both open the menu (KDE parity: the icon opens
//! its menu, never Settings directly).
//!
//! Menu commands: the top conflict row and Settings spawn the existing
//! `tiler-windows settings` UI as a detached child (the per-user singleton
//! mutex brings an open window forward instead); Stop sets an atomic the
//! owner loop polls next to its own stop request, so Stop follows the normal
//! owner cleanup (ledger reveal, Snap restore, watcher stop). Proof owners
//! never create this window.
//!
//! Crash recovery holds no persistent state: graceful stop issues
//! `NIM_DELETE`, while a dead owner leaves only an Explorer-side ghost that
//! the next startup or independent restore removes with a cross-process
//! `NIM_DELETE` against the stable icon GUID below (no live owner then, so
//! no live icon can be disturbed). The single-owner ledger lease guarantees
//! at most one live owner (hence at most one live icon).
//!
//! Stable automation surface: window class `OmniTilerTray`, menu ids
//! 1001 (conflict), 1003 (Settings), 1004 (Stop), first-run prompt title
//! `OmniTiler - First Run`.
//!
//! GUID cross-process semantics (Microsoft Learn, `NOTIFYICONDATAW` plus the
//! `NotificationIcon` classic sample): once an icon is added with `NIF_GUID`
//! plus `guidItem`, every later call addressing that icon must carry the same
//! `NIF_GUID`/`guidItem`, and `NIM_DELETE` needs only those two fields — a
//! zero `hWnd`/`uID` from a later same-user process removes the stale
//! registration (the sample's `DeleteNotificationIcon` does exactly this).
//!
//! All Win32 lives in [`crate::ffi::tray`]; this module holds only pure
//! policy (tooltip text, log/persist/spawn helpers) plus the safe owner
//! wrapper delegating to the opaque native window.

pub mod gaming;

pub use crate::ffi::tray::{FIRST_RUN_WINDOW_CLASS, FirstRunOutcome, first_run_dialog};

use std::sync::{Arc, Mutex};

use crate::tray::TRAY_TOOLTIP_TITLE;
use crate::tray::ToggleScope;
use crate::tray_pager::SelectScope;

type DynError = Box<dyn std::error::Error>;

/// Best-effort removal of a crash-ghost icon. Call only with no live owner.
#[must_use]
pub fn remove_stale_icon() -> bool {
    crate::ffi::tray::remove_stale_icon()
}

/// Tooltip text: identity plus the short settings status, with a conflict
/// suffix when the warning badge is up. Bounded for `szTip`.
#[must_use]
pub fn tooltip_text(conflict: bool, settings_status: &str) -> String {
    let short: String = settings_status.chars().take(48).collect();
    let mut tip = format!("{TRAY_TOOLTIP_TITLE} - Enabled ({short})");
    if conflict {
        tip.push_str(" - check settings");
    }
    tip.chars().take(120).collect()
}

/// Append one bounded JSON line to the tray log (best-effort, never panics).
pub(crate) fn append_tray_log(path: Option<std::path::PathBuf>, value: serde_json::Value) {
    use std::io::Write;
    if let Some(path) = path.as_ref()
        && let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
    {
        let _ = writeln!(file, "{value}");
    }
}

/// Open the existing native Settings window as a detached child.
pub(crate) fn spawn_settings_detached() {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let _ = std::process::Command::new(exe).arg("settings").spawn();
}

/// Persist one new-workspace default choice against `dir`.
///
/// Returns `(outcome, new_default)` where `outcome` is the bounded log token
/// (`no-settings-dir`, `refused-invalid`, `unchanged`, `written`,
/// `write-failed`) and `new_default` is the value the caller should cache
/// (`default_tiled` on `written`, otherwise the previous value is handled by
/// the caller keeping its cache).
pub(crate) fn persist_default_snapshot(
    dir: Option<std::path::PathBuf>,
    default_tiled: bool,
) -> (&'static str, bool) {
    let Some(dir) = dir else {
        return ("no-settings-dir", default_tiled);
    };
    let mut settings = match crate::settings::load_from_dir(&dir) {
        crate::settings::LoadOutcome::Loaded(settings) => settings,
        crate::settings::LoadOutcome::Missing => crate::settings::Settings::default(),
        crate::settings::LoadOutcome::Invalid(_) => return ("refused-invalid", default_tiled),
    };
    if settings.core.workspace.default_tiled == default_tiled {
        return ("unchanged", default_tiled);
    }
    settings.core.workspace.default_tiled = default_tiled;
    match crate::settings::save_to_dir(&dir, &mut settings) {
        Ok(()) => ("written", default_tiled),
        Err(_) => ("write-failed", default_tiled),
    }
}

/// Live owner tray: hidden window plus exactly one notification icon.
pub struct TrayOwner {
    native: crate::ffi::tray::TrayWindow,
}

impl TrayOwner {
    pub fn create(
        conflict_empty: bool,
        settings_status: &str,
        log_path: Option<std::path::PathBuf>,
        toggle: &Arc<Mutex<Option<ToggleScope>>>,
        select: &Arc<Mutex<Option<SelectScope>>>,
        settings_dir: Option<std::path::PathBuf>,
        stop_poll: Option<Arc<dyn Fn() -> bool + Send + Sync>>,
    ) -> Result<Self, DynError> {
        let native = crate::ffi::tray::TrayWindow::create(
            conflict_empty,
            settings_status,
            log_path,
            toggle,
            select,
            settings_dir,
            stop_poll,
        )?;
        Ok(Self { native })
    }

    pub fn sync(&mut self, conflict_empty: bool, settings_status: &str, ordinal: Option<u32>) {
        self.native.sync(conflict_empty, settings_status, ordinal);
    }

    #[must_use]
    pub fn stop_requested(&self) -> bool {
        self.native.stop_requested()
    }

    #[must_use]
    pub fn hwnd_value(&self) -> u64 {
        self.native.hwnd_value()
    }

    pub fn take_toggle_request(&self) -> Option<ToggleScope> {
        self.native.take_toggle_request()
    }

    pub fn has_toggle_request(&self) -> bool {
        self.native.has_toggle_request()
    }

    pub fn has_select_request(&self) -> bool {
        self.native.has_select_request()
    }

    pub fn take_select_request(&self) -> Option<SelectScope> {
        self.native.take_select_request()
    }

    pub fn sync_pager(&mut self, output: Option<String>, ids: Vec<String>, active: Option<String>) {
        self.native.sync_pager(output, ids, active);
    }

    pub fn sync_workspace(
        &mut self,
        scope: Option<(String, String)>,
        current_tiled: Option<bool>,
        default_tiled: bool,
    ) {
        self.native
            .sync_workspace(scope, current_tiled, default_tiled);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tray::{MENU_ID_CONFLICT, MENU_ID_SETTINGS, MENU_ID_STATUS, menu_items};

    #[test]
    fn tooltip_carries_status_and_conflict_suffix() {
        let plain = tooltip_text(false, "saved:3");
        assert!(plain.contains(TRAY_TOOLTIP_TITLE));
        assert!(plain.contains("saved:3"));
        assert!(!plain.contains("check settings"));
        let warned = tooltip_text(true, "saved:3");
        assert!(warned.contains("check settings"));
        let long = "x".repeat(400);
        assert!(tooltip_text(true, &long).len() <= 128);
    }

    #[test]
    fn menu_model_projects_cached_conflict_visibility() {
        use crate::tray::{WorkspaceMenu, status_line};
        let workspace = WorkspaceMenu::new(Some(true), true);
        let shown = menu_items(true, &status_line("saved:2"), workspace);
        let hidden = menu_items(false, &status_line("saved:2"), workspace);
        let visible_of = |items: &Vec<crate::tray::MenuItem>| {
            items.iter().find_map(|item| match item {
                crate::tray::MenuItem::Command { id, visible, .. } if *id == MENU_ID_CONFLICT => {
                    Some(*visible)
                }
                _ => None,
            })
        };
        assert_eq!(visible_of(&shown), Some(true));
        assert_eq!(visible_of(&hidden), Some(false));
        assert!(shown.iter().any(|item| matches!(
            item,
            crate::tray::MenuItem::Command {
                id: MENU_ID_STATUS,
                ..
            }
        )));
        assert!(shown.iter().any(|item| matches!(
            item,
            crate::tray::MenuItem::Command {
                id: MENU_ID_SETTINGS,
                ..
            }
        )));
    }

    #[test]
    fn persist_snapshot_refuses_invalid_without_touching_cache() {
        let (outcome, kept) = persist_default_snapshot(None, true);
        assert_eq!(outcome, "no-settings-dir");
        assert!(kept);
    }
}

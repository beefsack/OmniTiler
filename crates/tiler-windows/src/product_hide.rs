#![forbid(unsafe_code)]
//! Product hiding and recovery: identity-safe `SW_HIDE` with nonactivating
//! reveal, commit-before-hide, and a same-executable crash watcher.
//!
//! Product claims use [`crate::model::PRODUCT_CLAIM_PROP`] plus
//! [`crate::model::WindowClaimKind::Product`], so helper proof gates never
//! mistake them for helper ownership. The full process tuple alone does not
//! protect same-process HWND reuse: every effect checks fresh HWND, live PID,
//! full process identity, and the window-lifetime nonce before and after,
//! committed to the ledger BEFORE hiding. The nonce leaves only after a
//! verified reveal or release. Missing/destroyed/recycled windows retire with
//! no writes; uncertain identities refuse and keep the residue.
//!
//! Actuation posts `ShowWindowAsync` (never blocks on a hung target) and
//! succeeds only after a pumped visibility readback observes the effect.

use crate::model::{ProcessIdentity, WindowClaimKind, WindowIdentity, WindowShowState};

/// Shell classes that are never hide targets.
#[must_use]
pub fn shell_class_excluded(class: &str) -> bool {
    matches!(
        class,
        "Progman"
            | "WorkerW"
            | "Shell_TrayWnd"
            | "Shell_SecondaryTrayWnd"
            | "DV2ControlHost"
            | "MSCTFIME UI"
            | "SystemTray_Main"
    )
}

/// Generic dialog class: unowned top-level dialogs are never hide targets.
/// The project's own Settings UI (own executable plus settings class, see
/// [`crate::tiling::workspace_dialog_refuses`]) is an ordinary slotless
/// member and never refuses on this gate.
pub const DIALOG_CLASS: &str = "#32770";

/// Watcher readiness marker `watcher-<owner-creation>.ready`: three lines of
/// `owner-creation`, watcher pid, watcher creation. Readiness verifies the
/// live watcher process, never stale text alone.
#[must_use]
pub fn watcher_ready_filename(owner_creation: &str) -> String {
    format!("watcher-{owner_creation}.ready")
}

#[must_use]
pub fn render_watcher_ready(
    owner_creation: &str,
    watcher_pid: u32,
    watcher_creation: &str,
) -> String {
    format!("{owner_creation}\n{watcher_pid}\n{watcher_creation}\n")
}

#[must_use]
pub fn parse_watcher_ready(contents: &str) -> Option<(String, u32, String)> {
    let mut lines = contents.lines();
    let owner = lines.next()?.trim();
    let pid: u32 = lines.next()?.trim().parse().ok()?;
    let creation = lines.next()?.trim();
    if owner.is_empty() || creation.is_empty() || pid == 0 {
        return None;
    }
    Some((owner.to_owned(), pid, creation.to_owned()))
}

/// Visibility readback bound (ms) for one posted effect.
pub const HIDE_TIMEOUT_MS: u64 = 2000;
pub const REVEAL_TIMEOUT_MS: u64 = 2000;
pub const WATCHER_READY_TIMEOUT_MS: u64 = 5000;
pub const WATCHER_POLL_MS: u64 = 50;
pub const STORE_ACQUIRE_TIMEOUT_MS: u64 = 5000;

/// Per-window teardown outcome for one product claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProductTeardown {
    Revealed,
    AlreadyVisible,
    /// Verifiably gone or recycled: drop the claim with no window writes.
    Retired,
    /// Identity not established: keep the ledger residue, never touch.
    Uncertain,
}

#[must_use]
pub const fn teardown_keep_ledger(outcomes: &[ProductTeardown]) -> bool {
    let mut i = 0;
    while i < outcomes.len() {
        if matches!(outcomes[i], ProductTeardown::Uncertain) {
            return true;
        }
        i += 1;
    }
    false
}

#[must_use]
pub fn committed_has_same_claim(
    windows: &[crate::model::WindowIdentity],
    hwnd: u64,
    tag: &str,
) -> bool {
    windows
        .iter()
        .any(|w| w.hwnd == hwnd && w.tag == tag && w.kind == crate::model::WindowClaimKind::Product)
}

#[must_use]
pub fn product_claim(hwnd: u64, process: ProcessIdentity, tag: String) -> WindowIdentity {
    WindowIdentity {
        hwnd,
        process,
        tag,
        kind: WindowClaimKind::Product,
        show: WindowShowState::default(),
    }
}

/// Pure gate for the narrow project-raised topmost hide exception: true only
/// when runtime evidence proves the project raised the keep-above band from
/// a non-topmost prior (`Some(false)`) and the band is live now. A marker
/// alone never authorizes (no marker input here); preexisting topmost
/// (`Some(true)`), unknown history (`None`), recycled identity (no exact key),
/// and a currently non-topmost band all refuse. The caller still enforces the
/// fresh full admitted identity plus window-lifetime gates; this bit only
/// selects the topmost bypass.
#[must_use]
pub fn project_topmost_hide_allowed(prior_topmost: Option<bool>, live_topmost: bool) -> bool {
    matches!(prior_topmost, Some(false)) && live_topmost
}

#[cfg(windows)]
#[path = "product_hide/sys/mod.rs"]
pub mod sys;

// Joint visibility batch: post all effects, confirm together under one
// shared deadline (used by the tiling workspace select/send/rollback
// phases). Windows-only in normal builds (sole consumer is the
// cfg(windows) tiling_sys driver); included under cfg(test) so the pure
// fake state-machine tests stay portable off-Windows.
#[cfg(any(windows, test))]
pub(crate) mod batch;

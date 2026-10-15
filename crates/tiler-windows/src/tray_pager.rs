#![forbid(unsafe_code)]
//! Focused-output tray menu pager stopgap (decided 2026-10-15).
//!
//! Portable policy only (no Win32 here): which output the tray number and
//! menu list (the genuine focused output, tentative), the 1-based pager rows
//! in workspace order with the active radio check, and the scoped select
//! request with its stale-scope fence. Native effects live in
//! `crate::tray_sys` plus `crate::tiling_sys::tray_select` (`cfg(windows)`
//! only); the existing tray settings/stop/toggle semantics there are
//! untouched.
//!
//! Tentative product choices (documented for later user review): the
//! focused output prefers the genuine foreground monitor over the cached
//! last-switch output; pager labels read `Workspace N`; the pager section
//! sits after the workspace toggle/default rows and before Settings.

use crate::workspace::ManagedWorkspaces;

/// First menu id for pager workspace rows: row `i` (0-based) uses
/// `MENU_ID_WORKSPACE_FIRST + i`. Above the fixed 1001-1007 range.
pub const MENU_ID_WORKSPACE_FIRST: u32 = 1100;

/// Tentative pager label for a 1-based workspace ordinal.
#[must_use]
pub fn pager_label(ordinal: u32) -> String {
    format!("Workspace {ordinal}")
}

/// Menu id for one 0-based pager row.
#[must_use]
pub const fn menu_id_for_index(index: usize) -> u32 {
    MENU_ID_WORKSPACE_FIRST + index as u32
}

/// 0-based pager row for a tracked menu pick, or `None` when the pick is not
/// a pager row (`count` is the rendered row count).
#[must_use]
pub const fn workspace_pick_index(picked: u32, count: usize) -> Option<usize> {
    if picked < MENU_ID_WORKSPACE_FIRST {
        return None;
    }
    let index = (picked - MENU_ID_WORKSPACE_FIRST) as usize;
    if index < count { Some(index) } else { None }
}

/// 1-based ordinal of an output's active workspace in workspace order, or
/// `None` when the output or its active workspace is unknown.
#[must_use]
pub fn active_ordinal(spaces: &ManagedWorkspaces, output: &str) -> Option<u32> {
    let ids = spaces.workspace_ids(output);
    let active = spaces.active_id(output)?;
    ids.iter()
        .position(|id| id == &active)
        .map(|index| index as u32 + 1)
}

/// Genuine focused output with transient owned-UI retention (pure core of
/// the per-pump focus computation): `foreground_owned_ui` is true only when
/// the live foreground window is exactly the tray owner (nonzero hwnd
/// match). Our own transient menu then keeps the last genuine output
/// instead of turning the tray click point into a focus change; a missing
/// foreground keeps the ordinary pointer/cache/ordering fallback without
/// touching retention. Returns `(focused, retained)`: retention updates only
/// from a known foreground monitor, never from owned UI, pointer, cache, or
/// ordering. Never invents a key.
#[must_use]
pub fn focused_output_transient(
    keys: &[String],
    cached: Option<&str>,
    foreground: Option<&str>,
    foreground_owned_ui: bool,
    pointer: Option<&str>,
    retained: Option<&str>,
) -> (Option<String>, Option<String>) {
    let known = |key: &str| keys.iter().any(|k| k == key);
    if foreground_owned_ui {
        let focused = retained
            .filter(|output| known(output))
            .map(str::to_owned)
            .or_else(|| cached.filter(|output| known(output)).map(str::to_owned))
            .or_else(|| keys.first().cloned());
        return (focused, retained.map(str::to_owned));
    }
    let genuine = foreground.filter(|output| known(output));
    let focused = genuine
        .map(str::to_owned)
        .or_else(|| pointer.filter(|output| known(output)).map(str::to_owned))
        .or_else(|| cached.filter(|output| known(output)).map(str::to_owned))
        .or_else(|| keys.first().cloned());
    let retained_next = genuine
        .map(str::to_owned)
        .or_else(|| retained.map(str::to_owned));
    (focused, retained_next)
}

/// Scoped tray-select request: the exact `(output, workspace)` the pager
/// rendered, plus its 1-based ordinal for bounded logging. The owner drains
/// it once on its pump and selects it only when the live focused output
/// still matches and the workspace is still live there (see
/// [`verify_select_scope`]); anything else refuses with no guessed fallback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectScope {
    pub output: String,
    pub workspace: String,
    pub index: u32,
}

impl SelectScope {
    #[must_use]
    pub fn new(output: &str, workspace: &str, index: u32) -> Self {
        Self {
            output: output.to_owned(),
            workspace: workspace.to_owned(),
            index,
        }
    }
}

/// Verify one drained select request: true only when the rendered output is
/// still the live focused output and the rendered workspace is still live on
/// it. `None` focus or a pruned workspace refuses, like the existing toggle
/// fence. The already-active workspace is NOT stale here: selecting it is an
/// honest no-op (`already-active`) at dispatch.
#[must_use]
pub fn verify_select_scope(
    rendered: &SelectScope,
    live_focused_output: Option<&str>,
    workspace_live: bool,
) -> bool {
    live_focused_output.is_some_and(|live| live == rendered.output) && workspace_live
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys() -> Vec<String> {
        vec!["mon-a".to_owned(), "mon-b".to_owned()]
    }

    #[test]
    fn focused_output_prefers_genuine_focus_over_stale_cache() {
        let transient = |cached: Option<&str>, foreground: Option<&str>, pointer: Option<&str>| {
            focused_output_transient(&keys(), cached, foreground, false, pointer, None).0
        };
        // Foreground on B while the cache still says A: the tray follows B.
        assert_eq!(
            transient(Some("mon-a"), Some("mon-b"), Some("mon-a")),
            Some("mon-b".to_owned())
        );
        // No foreground: the pointer decides, still ahead of the cache.
        assert_eq!(
            transient(Some("mon-a"), None, Some("mon-b")),
            Some("mon-b".to_owned())
        );
        // Neither focus proxy known: fall back to the cache, then ordering.
        assert_eq!(
            transient(Some("mon-a"), None, None),
            Some("mon-a".to_owned())
        );
        assert_eq!(transient(None, None, None), Some("mon-a".to_owned()));
        // Unknown proxies never invent a key.
        assert_eq!(
            transient(Some("gone"), Some("gone"), Some("gone")),
            Some("mon-a".to_owned())
        );
        assert_eq!(
            focused_output_transient(&[], None, None, false, None, None),
            (None, None)
        );
    }

    #[test]
    fn transient_owned_ui_preserves_genuine_focus_for_pick() {
        let keys = keys();
        // Last switch A, real focus B, pointer on the tray monitor A: B wins
        // and retention records the genuine output.
        let (focused, retained) = focused_output_transient(
            &keys,
            Some("mon-a"),
            Some("mon-b"),
            false,
            Some("mon-a"),
            None,
        );
        assert_eq!(focused, Some("mon-b".to_owned()));
        assert_eq!(retained, Some("mon-b".to_owned()));
        // The tray menu opens on A (foreground becomes exactly the owned
        // owner): the click point must not become a focus change, so the
        // B pick still verifies against live B and succeeds.
        let (focused, retained) = focused_output_transient(
            &keys,
            Some("mon-a"),
            None,
            true,
            Some("mon-a"),
            retained.as_deref(),
        );
        assert_eq!(focused, Some("mon-b".to_owned()));
        assert_eq!(retained, Some("mon-b".to_owned()));
        assert!(verify_select_scope(
            &SelectScope::new("mon-b", "ws-2", 2),
            focused.as_deref(),
            true
        ));
        // Actual real focus moves to A afterwards: the stale B pick refuses.
        let (focused, retained) = focused_output_transient(
            &keys,
            Some("mon-a"),
            Some("mon-a"),
            false,
            Some("mon-a"),
            retained.as_deref(),
        );
        assert_eq!(focused, Some("mon-a".to_owned()));
        assert_eq!(retained, Some("mon-a".to_owned()));
        assert!(!verify_select_scope(
            &SelectScope::new("mon-b", "ws-2", 2),
            focused.as_deref(),
            true
        ));
    }

    #[test]
    fn select_scope_verifies_focus_and_liveness() {
        let rendered = SelectScope::new("mon-a", "ws-1", 1);
        assert!(verify_select_scope(&rendered, Some("mon-a"), true));
        // Output focus moved away: stale, no fallback select.
        assert!(!verify_select_scope(&rendered, Some("mon-b"), true));
        // Unknown focus or a pruned workspace: stale.
        assert!(!verify_select_scope(&rendered, None, true));
        assert!(!verify_select_scope(&rendered, Some("mon-a"), false));
        assert!(!verify_select_scope(&rendered, None, false));
    }

    #[test]
    fn menu_ids_round_trip_rows() {
        assert_eq!(menu_id_for_index(0), MENU_ID_WORKSPACE_FIRST);
        assert_eq!(
            workspace_pick_index(MENU_ID_WORKSPACE_FIRST + 2, 3),
            Some(2)
        );
        assert_eq!(workspace_pick_index(MENU_ID_WORKSPACE_FIRST + 3, 3), None);
        assert_eq!(workspace_pick_index(1003, 3), None);
        assert_eq!(workspace_pick_index(0, 3), None);
    }

    #[test]
    fn pager_labels_are_one_based() {
        assert_eq!(pager_label(1), "Workspace 1");
        assert_eq!(pager_label(12), "Workspace 12");
    }

    #[test]
    fn active_ordinal_is_one_based_in_workspace_order() {
        use crate::workspace::ManagedWorkspaces;
        let mut spaces = ManagedWorkspaces::new();
        spaces.ensure_output("mon-a");
        // Fresh output starts on its first workspace: ordinal 1.
        assert_eq!(active_ordinal(&spaces, "mon-a"), Some(1));
        let second = spaces.select("mon-a", 2).expect("second workspace");
        assert_eq!(spaces.active_id("mon-a").as_deref(), Some(second.as_str()));
        assert_eq!(active_ordinal(&spaces, "mon-a"), Some(2));
        assert_eq!(active_ordinal(&spaces, "mon-missing"), None);
    }
}

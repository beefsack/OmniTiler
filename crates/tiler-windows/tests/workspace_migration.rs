#![forbid(unsafe_code)]
//! Items 14/19 portable wiring: the saved `core.migration_source_refill`
//! setting drives `ManagedWorkspaces::migrate_workspace` through the shared
//! G-37 selector, with no core changes and no per-window-send simulation.
//!
//! Portable only (no Win32 here): native arrival verification, overlay
//! transfer and live two-output journeys ride the next unit's runtime and
//! user-owned live checks. Local project-owned outputs only.

use tiler_core::output_selection::MigrationSourceRefill;
use tiler_windows::settings::Settings;
use tiler_windows::workspace::{ManagedWorkspaces, WindowKey};

fn key(hwnd: u64) -> WindowKey {
    WindowKey {
        hwnd,
        pid: 1000 + hwnd as u32,
        creation: format!("c{hwnd:016x}"),
    }
}

fn ids(m: &ManagedWorkspaces, output: &str) -> Vec<String> {
    // Workspace ids are test-opaque: read the scoped order through the
    // public ring helper (selection creates nothing).
    m.scoped_ring_ids(output)
}

fn two_output_fixture() -> (ManagedWorkspaces, String, String, String) {
    // Source holds ws1 (occupied, previous view) and ws2 (active, occupied);
    // the target holds its own workspaces with the second current.
    let mut m = ManagedWorkspaces::new();
    m.ensure_output("mon-1");
    m.ensure_output("mon-2");
    let src = ids(&m, "mon-1");
    let dst = ids(&m, "mon-2");
    let (ws1, ws2) = (src[0].clone(), src[1].clone());
    assert!(m.assign(key(31), "mon-1", &ws1, false));
    assert!(m.assign(key(32), "mon-1", &ws2, false));
    // Record ws1 as the previous view and show ws2 for the move (the fresh
    // baseline is the initial ws1, so observing ws2 records ws1).
    assert!(m.activate("mon-1", &ws2));
    assert!(m.observe_workspace_change("mon-1", &ws2));
    assert!(m.activate("mon-2", &dst[1]));
    (m, ws1, ws2, dst[1].clone())
}

#[test]
fn configured_mru_refill_drives_migration_through_shared_selector() {
    let mut settings = Settings::default();
    settings.core.migration_source_refill = "most-recently-used-workspace".to_owned();
    assert!(tiler_windows::settings::validate_settings(&settings).is_ok());
    assert_eq!(
        settings.core.migration_source_refill_mode(),
        MigrationSourceRefill::MostRecentlyUsed
    );
    let (mut m, ws1, ws2, target_current) = two_output_fixture();
    let outcome = m
        .migrate_workspace(
            "mon-1",
            "mon-2",
            settings.core.migration_source_refill_mode(),
        )
        .expect("migrate");
    assert_eq!(outcome.moved_id, ws2);
    // MRU refills the remembered previous view, not the trailing entry.
    assert_eq!(outcome.source_refill.as_deref(), Some(ws1.as_str()));
    assert_eq!(m.active_id("mon-1").as_deref(), Some(ws1.as_str()));
    // Destination inserts immediately after the target current and shows.
    let order = m.scoped_ring_ids("mon-2");
    let at = order
        .iter()
        .position(|id| id == &target_current)
        .expect("current");
    assert_eq!(order[at + 1], ws2);
    assert_eq!(m.active_id("mon-2").as_deref(), Some(ws2.as_str()));
}

#[test]
fn default_setting_refills_last_remaining() {
    let settings = Settings::default();
    assert_eq!(
        settings.core.migration_source_refill_mode(),
        MigrationSourceRefill::LastRemaining
    );
    let (mut m, _, ws2, _) = two_output_fixture();
    let outcome = m
        .migrate_workspace(
            "mon-1",
            "mon-2",
            settings.core.migration_source_refill_mode(),
        )
        .expect("migrate");
    assert_eq!(outcome.moved_id, ws2);
    // Source order after removal is [ws1]; the default refills the last
    // remaining entry even though MRU history (ws1) is eligible here too.
    let remaining = m.scoped_ring_ids("mon-1");
    assert_eq!(remaining.len(), 1);
    assert_eq!(
        outcome.source_refill.as_deref(),
        Some(remaining.last().expect("last").as_str())
    );
    assert_eq!(
        m.active_id("mon-1").as_deref(),
        outcome.source_refill.as_deref()
    );
}

#[test]
fn unknown_hand_built_value_fails_closed_to_last_remaining_mode() {
    // Validation refuses the unknown token (never live); the typed accessor
    // fails closed to the default mode for hand-built drafts.
    let mut settings = Settings::default();
    settings.core.migration_source_refill = "mru".to_owned();
    assert!(tiler_windows::settings::validate_settings(&settings).is_err());
    assert_eq!(
        settings.core.migration_source_refill_mode(),
        MigrationSourceRefill::LastRemaining
    );
    let (mut m, _, ws2, _) = two_output_fixture();
    let outcome = m
        .migrate_workspace(
            "mon-1",
            "mon-2",
            settings.core.migration_source_refill_mode(),
        )
        .expect("migrate");
    assert_eq!(outcome.moved_id, ws2);
    let remaining = m.scoped_ring_ids("mon-1");
    assert_eq!(
        outcome.source_refill.as_deref(),
        Some(remaining.last().expect("last").as_str())
    );
}

#[test]
fn migration_moves_membership_wholesale_without_window_sends() {
    // One primitive call rekeys every member's output: no per-window send
    // loop, no admission, hidden state preserved.
    let (mut m, _, ws2, _) = two_output_fixture();
    assert!(m.assign(key(33), "mon-1", &ws2, true));
    let before: Vec<String> = m.scoped_ring_ids("mon-1");
    let outcome = m
        .migrate_workspace("mon-1", "mon-2", MigrationSourceRefill::LastRemaining)
        .expect("migrate");
    assert_eq!(outcome.moved_id, ws2);
    for hwnd in [32, 33] {
        let loc = m.member_loc(&key(hwnd)).expect("member");
        assert_eq!(loc.output, "mon-2");
        assert_eq!(loc.workspace, ws2);
    }
    assert!(m.member_loc(&key(33)).expect("hidden").hidden);
    assert!(!m.member_loc(&key(32)).expect("shown").hidden);
    // Source scope shrank by exactly the moved id; nothing else moved.
    let after: Vec<String> = m.scoped_ring_ids("mon-1");
    assert_eq!(after.len(), before.len() - 1);
    assert!(!after.contains(&ws2));
}

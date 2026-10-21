#![forbid(unsafe_code)]
//! Item 1 portable regression: settings catalog/presets, exact Ctrl/Tab
//! classifier routing, and world-owned history/ring convergence.
//!
//! Portable only (no Win32 here): the Windows-only owner selection,
//! foreground/CLI dispatch, hook release/suppression, and Apply/Revert UI
//! wiring ride `tiling_sys`/`settings_ui` (`cfg(windows)`) and are covered by
//! the native offline gates plus user-owned live journeys. Local-only: this
//! file keeps non-local/multi-output runtime pending.

use tiler_windows::settings::{
    BindingSetting, BindingState, Preset, Settings, apply_preset, binding_catalog,
    compatible_disabled_ids,
};
use tiler_windows::snapkey::{
    ChordAction, Classified, KeyboardConfig, SnapClassify, VK_CONTROL, VK_H, VK_J, VK_LWIN, VK_TAB,
    WorkspaceHistoryOp,
};

fn takeover() -> KeyboardConfig {
    KeyboardConfig {
        takeover: true,
        allow_win_l: false,
    }
}

fn win_ctrl(m: &mut SnapClassify) {
    SnapClassify::push(m, VK_LWIN, false, true, false);
    SnapClassify::push(m, VK_CONTROL, false, true, false);
}

fn history_down(m: &mut SnapClassify, vk: u32) -> WorkspaceHistoryOp {
    match SnapClassify::push(m, vk, false, true, false).expect("history down") {
        Classified::WorkspaceHistory(intent) => {
            assert!(intent.consumed && intent.announce);
            intent.op
        }
        other => panic!("expected history, got {other:?}"),
    }
}

#[test]
fn catalog_exposes_nine_history_rows_with_honest_conflicts() {
    let catalog = binding_catalog();
    assert_eq!(catalog.len(), 103);
    let ids: Vec<&str> = catalog
        .iter()
        .filter(|def| {
            def.id.starts_with("workspace-prev")
                || def.id.starts_with("workspace-next")
                || def.id == "workspace-previous"
        })
        .map(|def| def.id)
        .collect();
    assert_eq!(
        ids,
        vec![
            "workspace-previous",
            "workspace-prev-h",
            "workspace-prev-k",
            "workspace-prev-left-arrow",
            "workspace-prev-up-arrow",
            "workspace-next-j",
            "workspace-next-l",
            "workspace-next-down-arrow",
            "workspace-next-right-arrow",
        ]
    );
    for def in catalog.iter().filter(|def| {
        def.id.starts_with("workspace-prev")
            || def.id.starts_with("workspace-next")
            || def.id == "workspace-previous"
    }) {
        assert!(def.implemented, "{}", def.id);
        let conflict = def.conflict.expect("honest ownership text");
        assert!(
            conflict.contains("unverified in repository"),
            "{}: {conflict}",
            def.id
        );
    }
    // Left/Right name the virtual-desktop switch; Tab/letters/Up/Down stay
    // ownership-unknown without a stock-holder claim.
    let left = catalog
        .iter()
        .find(|def| def.id == "workspace-prev-left-arrow")
        .expect("row");
    assert!(left.conflict.is_some_and(|c| c.contains("virtual desktop")));
    let tab = catalog
        .iter()
        .find(|def| def.id == "workspace-previous")
        .expect("row");
    assert!(
        tab.conflict
            .is_some_and(|c| c.contains("No documented conflict"))
    );
}

#[test]
fn presets_authentic_keeps_all_and_compatible_disables_only_arrows() {
    // Authentic keeps all nine history rows.
    let effective = tiler_windows::settings::effective_bindings(&Settings::default());
    for id in [
        "workspace-previous",
        "workspace-prev-h",
        "workspace-prev-left-arrow",
        "workspace-next-right-arrow",
    ] {
        let row = effective.iter().find(|row| row.id == id).expect("row");
        assert!(row.active && row.effective, "{id}");
    }
    // Compatible disables only the two recorded virtual-desktop arrows
    // (39 disabled rows total) and rebinds float/fullscreen to Win+Ctrl.
    let mut settings = Settings::default();
    apply_preset(&mut settings, Preset::Compatible);
    assert_eq!(compatible_disabled_ids().len(), 39);
    for id in ["workspace-prev-left-arrow", "workspace-next-right-arrow"] {
        assert!(compatible_disabled_ids().contains(&id), "{id} disabled");
        assert_eq!(
            settings.bindings.get(id).map(|b| &b.state),
            Some(&BindingState::Disabled)
        );
    }
    for id in [
        "workspace-previous",
        "workspace-prev-h",
        "workspace-prev-k",
        "workspace-prev-up-arrow",
        "workspace-next-j",
        "workspace-next-l",
        "workspace-next-down-arrow",
    ] {
        assert!(!compatible_disabled_ids().contains(&id), "{id} kept");
        assert!(!settings.bindings.contains_key(id), "{id} kept");
    }
    // Unknown OS ownership adds no tray warning: only G/F11 warn.
    let conflicts = tiler_windows::tray::unresolved_conflicts(&settings, true, false);
    assert!(
        conflicts.iter().all(|c| c.id == "toggle-float"
            || c.id == "toggle-fullscreen"
            || !c.id.starts_with("workspace-p")),
        "{conflicts:?}"
    );
}

#[test]
fn duplicate_modifiers_refuse_and_rebound_conflict_is_honest() {
    // A Win+Ctrl+U history rebind validates; the same chord twice refuses;
    // the effective row reports the rebound chord's owner, not the stale
    // original.
    let mut settings = Settings::default();
    settings.bindings.insert(
        "workspace-prev-h".to_owned(),
        BindingSetting {
            state: BindingState::Rebind,
            chord: Some("Win+Ctrl+U".to_owned()),
        },
    );
    tiler_windows::settings::validate_settings(&settings).expect("valid");
    let effective = tiler_windows::settings::effective_bindings(&settings);
    let row = effective
        .iter()
        .find(|row| row.id == "workspace-prev-h")
        .expect("row");
    assert_eq!(row.chords, vec!["Win+Ctrl+U".to_owned()]);
    assert!(
        row.conflict
            .is_some_and(|c| c.contains("unverified in repository")),
        "{:?}",
        row.conflict
    );
    settings.bindings.insert(
        "workspace-prev-k".to_owned(),
        BindingSetting {
            state: BindingState::Rebind,
            chord: Some("Win+Ctrl+U".to_owned()),
        },
    );
    assert!(tiler_windows::settings::validate_settings(&settings).is_err());
}

#[test]
fn classifier_routes_history_through_remap_and_suppression() {
    // Rebound history (Win+Ctrl+U -> Prev/H) routes with explicit action;
    // the rebound-away Win+Ctrl+H passes through; disabling the row
    // suppresses its chord with its pair passing too.
    let mut settings = Settings::default();
    settings.bindings.insert(
        "workspace-prev-h".to_owned(),
        BindingSetting {
            state: BindingState::Rebind,
            chord: Some("Win+Ctrl+U".to_owned()),
        },
    );
    tiler_windows::settings::validate_settings(&settings).expect("valid");
    let remap = tiler_windows::settings::build_remap(&settings);
    assert_eq!(remap.len(), 1);
    assert_eq!(remap[0].action, ChordAction::WorkspacePrev);
    let mut m = SnapClassify::new(takeover());
    m.set_remap(remap);
    m.set_disabled(tiler_windows::settings::build_disabled(&settings));
    win_ctrl(&mut m);
    assert_eq!(history_down(&mut m, 0x55), WorkspaceHistoryOp::Prev);
    // Rebound-away canonical passes through on a fresh machine.
    let mut plain = SnapClassify::new(takeover());
    plain.set_remap(tiler_windows::settings::build_remap(&settings));
    plain.set_disabled(tiler_windows::settings::build_disabled(&settings));
    win_ctrl(&mut plain);
    assert_eq!(
        SnapClassify::push(&mut plain, VK_H, false, true, false),
        None
    );
    // Disabled row suppresses with pair passing.
    let mut settings = Settings::default();
    settings.bindings.insert(
        "workspace-previous".to_owned(),
        BindingSetting {
            state: BindingState::Disabled,
            chord: None,
        },
    );
    let mut m = SnapClassify::new(takeover());
    m.set_disabled(tiler_windows::settings::build_disabled(&settings));
    win_ctrl(&mut m);
    assert_eq!(SnapClassify::push(&mut m, VK_TAB, false, true, false), None);
    assert_eq!(SnapClassify::push(&mut m, VK_TAB, true, true, false), None);
}

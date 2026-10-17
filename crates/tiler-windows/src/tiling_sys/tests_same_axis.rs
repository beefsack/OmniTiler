#![forbid(unsafe_code)]
use super::same_axis_move_for;
use crate::settings::{LiveSettings, SAME_AXIS_MOVE_SWAP, Settings};
use tiler_core::directional::SameAxisMove;

#[test]
fn proof_owners_without_live_settings_keep_group_wrap() {
    assert_eq!(same_axis_move_for(None), SameAxisMove::GroupWithNeighbor);
}

#[test]
fn live_setting_selects_subsequent_move_mode() {
    let live = LiveSettings::fresh(Settings::default(), None);
    assert_eq!(
        same_axis_move_for(Some(&live)),
        SameAxisMove::GroupWithNeighbor
    );
    let mut swap = Settings::default();
    swap.core.same_axis_move = SAME_AXIS_MOVE_SWAP.to_owned();
    let live = LiveSettings::fresh(swap, None);
    assert_eq!(
        same_axis_move_for(Some(&live)),
        SameAxisMove::SwapWithNeighbor
    );
}

#[test]
fn apply_same_axis_only_returns_false_adopts_and_rebuilds_nothing() {
    use crate::settings::{build_disabled, build_remap, tile_options_from_settings};
    use tiler_core::boundary::{CoreCommand, CoreReply};
    use tiler_core::directional::WindowId;
    use tiler_core::geometry::Rect;
    use tiler_core::ids::CorrelationId;

    // Scratch home for the ledger lock and the owner log line: never the
    // repo or the live product directory. No window, hook, suspend, or
    // registry effect exists on this path (every native lane below is
    // aligned away); the temp dir is removed at the end.
    let tmp = std::env::temp_dir().join(format!(
        "tiler-same-axis-live-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let store = crate::storage::LedgerStore::open(&tmp).expect("ledger temp dir");
    let mut state = super::rmax03_adapter_tests::test_state();
    state.log_path = tmp.join("owner.log");
    // Align every lane with the defaults so the incoming document differs
    // in exactly one field. In particular the keyboard/remap lanes must
    // match, or `apply_live_config` would touch the native hook config.
    let base = tile_options_from_settings(&Settings::default());
    state.border = base.border;
    state.underlay = base.underlay;
    state.keyboard = crate::snapkey::KeyboardConfig {
        takeover: true,
        allow_win_l: false,
    };
    state.last_remap = build_remap(&Settings::default());
    state.last_disabled = build_disabled(&Settings::default());
    state.settings_live = Some(LiveSettings::fresh(Settings::default(), None));
    let mut snap_want = true;
    // Retained session: two tiled windows, so "no rebuild" is observable
    // instead of vacuous.
    let bounds = Rect {
        x: 0,
        y: 0,
        w: 1600,
        h: 900,
    };
    let domain = crate::workspace_owner::workspace_domain("mon-a", "ws-1", bounds, 8);
    let seed_fp =
        crate::tiling::fingerprint(&[("w1".to_owned(), bounds), ("w2".to_owned(), bounds)]);
    let seed = crate::tiling::build_reconcile_event_for(
        &state.owner,
        &state.generation,
        &CorrelationId::parse("live-seed").expect("correlation"),
        0,
        seed_fp,
        &domain.0,
        &domain.1,
        8,
        &[
            (
                WindowId("w1".to_owned()),
                bounds,
                tiler_core::size_hints::WindowSizeHints::none(),
            ),
            (
                WindowId("w2".to_owned()),
                bounds,
                tiler_core::size_hints::WindowSizeHints::none(),
            ),
        ],
        Some(&WindowId("w2".to_owned())),
    );
    let reply = state.engine.handle(&seed);
    assert!(
        matches!(reply, CoreReply::Projection(_) | CoreReply::Tiled(_)),
        "seed must converge, got {reply:?}"
    );
    // A move command builds here exactly like `keyboard_tick` builds it.
    let moved = {
        let mut event = seed.clone();
        event.revision = state
            .engine
            .session(&domain.1)
            .map(|session| session.accepted_revision())
            .unwrap_or(0);
        event.focused_window = WindowId("w2".to_owned());
        event.command = CoreCommand::Move {
            window: "w2".to_owned(),
            direction: "left".to_owned(),
            cross_output_transfer: false,
            same_axis_move: same_axis_move_for(state.settings_live.as_ref()),
        };
        state.engine.handle(&event)
    };
    assert!(
        matches!(moved, CoreReply::MoveDirectional(_)),
        "pre-apply move must plan, got {moved:?}"
    );
    let before = state.engine.session(&domain.1).expect("session").snapshot();
    let me = crate::model::ProcessIdentity {
        pid: 4242,
        process_creation: "test-creation".to_owned(),
        user_sid: "S-1-5-test".to_owned(),
        session_id: 1,
        exe_path: "test.exe".to_owned(),
    };
    let mut incoming = Settings::default();
    incoming.core.same_axis_move = SAME_AXIS_MOVE_SWAP.to_owned();
    incoming.revision = 7;
    let changed = super::apply_live_settings(
        &mut state,
        &tmp,
        &me,
        &store,
        &mut snap_want,
        incoming,
        None,
    );
    assert!(
        !changed,
        "same-axis-only adoption must report no owner change"
    );
    assert_eq!(
        state.engine.session(&domain.1).expect("session").snapshot(),
        before,
        "retained session untouched: no rebuild, no resync"
    );
    let live = state.settings_live.as_ref().expect("live");
    assert_eq!(live.settings.core.same_axis_move, SAME_AXIS_MOVE_SWAP);
    assert_eq!(live.status, "saved:7");
    assert_eq!(
        same_axis_move_for(state.settings_live.as_ref()),
        SameAxisMove::SwapWithNeighbor,
        "subsequent moves use the adopted setting"
    );
    assert_eq!((state.inner_gap, state.outer_gap), (8, 8));
    drop(store);
    let _ = std::fs::remove_dir_all(&tmp);
}

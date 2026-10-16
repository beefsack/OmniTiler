use super::{
    HintCx, RenderFrameHealth, RetainedRow, TileLoop, assemble_domain_rows, workspace_mode_known,
    writable_tokens,
};

/// Shared proof-style owner harness (no HWNDs, hooks, or live effects).
/// `pub(super)` so the R-MOV-03 live-adoption test reuses it directly.
pub(super) fn test_state() -> TileLoop {
    use tiler_core::ids::{GenerationId, OwnerId};
    let owner = OwnerId::parse("tiler-windows").expect("owner");
    let generation = GenerationId::parse("rmax03-adapter").expect("generation");
    let mut state = TileLoop {
        engine: tiler_core::engine::Engine::new(),
        owner: owner.clone(),
        generation: generation.clone(),
        tokens: crate::tiling::TokenMap::default(),
        refused: crate::tiling::RefusedTracker::default(),
        stable: std::collections::HashMap::new(),
        managed: std::collections::HashSet::new(),
        active: std::collections::HashSet::new(),
        gesture_before: std::collections::HashMap::new(),
        gesture_end_cursor: std::collections::HashMap::new(),
        gesture_start_key: std::collections::HashMap::new(),
        gesture_start_tag: std::collections::HashMap::new(),
        tick: 0,
        allowlist: None,
        scope: Vec::new(),
        scope_hosts: Vec::new(),
        trace: false,
        suspended: false,
        last_summary: None,
        // NUL log sink: no test asserts on log bytes, and a relative
        // file would interleave garbage across parallel tests into the
        // crate directory on every run.
        log_path: std::path::PathBuf::from("NUL"),
        audit_path: None,
        keyboard: crate::snapkey::KeyboardConfig::disabled(),
        inner_gap: crate::tiling::INNER_GAP,
        outer_gap: crate::tiling::OUTER_GAP,
        settings_live: None,
        settings_dir: None,
        last_remap: Vec::new(),
        last_disabled: Vec::new(),
        cli_overrides: crate::tiling::CliOverrides::default(),
        snap_dropped: 0,
        cb_diag_dropped: 0,
        cb_diag_filtered: 0,
        cb_outbox_dropped: 0,
        mask_sends: 0,
        mask_send_max_us: 0,
        snap_origins: std::collections::HashMap::new(),
        windrag_origins: std::collections::HashMap::new(),
        gesture_producer: std::collections::HashMap::new(),
        windrag_start_cursor: std::collections::HashMap::new(),
        windrag_bound: std::collections::HashMap::new(),
        windrag_dropped: 0,
        windrag_origin_logged: None,
        windrag_stats_logged: (0, 0, 0, 0),
        snap_advance: None,
        resize_repeat: None,
        last_enumerated: 0,
        workspaces: crate::workspace::ManagedWorkspaces::new(),
        scope_owner: crate::workspace_scope_owner::WorkspaceScopeOwner::new(),
        member_tokens: std::collections::BTreeMap::new(),
        member_rects: std::collections::HashMap::new(),
        hidden_claims: std::collections::BTreeMap::new(),
        member_identity: std::collections::BTreeMap::new(),
        member_tags: std::collections::BTreeMap::new(),
        active_output: String::new(),
        last_genuine_output: None,
        taskbar_pager: true,
        pager_overlays: None,
        pager_clicks: std::sync::Arc::new(std::sync::Mutex::new(None)),
        pager_served: Vec::new(),
        pager_retry_at: None,
        pager_revalidate_at: std::time::Instant::now(),
        pager_areas_fp: 0,
        pager_primary: None,
        pager_secondaries: Vec::new(),
        pager_live: std::collections::HashMap::new(),
        pager_icon_cache: crate::pager_icons::PagerIconCache::new(),
        pager_icon_service: None,
        pager_icon_retry_at: None,
        pager_suppress_last: None,
        pager_refusals: std::collections::HashMap::new(),
        workspace_proof: false,
        maximize_admission_attempted: std::collections::HashSet::new(),
        born_keep_overlay: std::collections::BTreeSet::new(),
        retile_keep_overlay: std::collections::BTreeSet::new(),
        born_fullscreen: std::collections::BTreeSet::new(),
        seen_nonfullscreen: std::collections::BTreeSet::new(),
        last_foreground: 0,
        known_outputs: Vec::new(),
        last_hwnds: std::collections::HashSet::new(),
        minimized_omitted: std::collections::HashSet::new(),
        pending_closed: std::collections::BTreeMap::new(),
        last_areas: Vec::new(),
        hint_logged: std::collections::HashMap::new(),
        hint_last_good: std::collections::HashMap::new(),
        hint_fallback_uses: std::collections::HashMap::new(),
        diag_pending: std::collections::HashMap::new(),
        diag_emitted: std::collections::HashMap::new(),
        diag_class: std::collections::HashMap::new(),
        diag_last_emit: None,
        diag_emit_seq: std::collections::HashMap::new(),
        diag_emit_clock: 0,
        diag_coalesced: std::collections::HashSet::new(),
        diag_query_ms: 0,
        vis_diag: super::VisibilityDiagnostics::default(),
        suspend_started: None,
        restore_wake: None,
        border: crate::active_border::ActiveBorderOptions::default(),
        border_overlay: crate::active_border_sys::BorderOverlay::default(),
        border_last: None,
        underlay: crate::group_underlay::GroupUnderlayOptions::default(),
        underlay_overlay: crate::active_border_sys::UnderlayOverlay::default(),
        underlay_last: None,
        move_kind: std::collections::HashMap::new(),
        gesture_start_cursor: std::collections::HashMap::new(),
        preview_overlay: crate::active_border_sys::PreviewOverlay::default(),
        preview_last: None,
        preview_bound: std::collections::HashMap::new(),
        gesture_preview_start: std::collections::HashMap::new(),
        preview_dead: std::collections::HashSet::new(),
        esc_latched: std::collections::HashSet::new(),
        gesture_esc_seq: std::collections::HashMap::new(),
        gesture_end_seq: std::collections::HashMap::new(),
        underlay_chord_last: false,
        float_topmost_prev: std::collections::BTreeMap::new(),
        float_rects: std::collections::HashMap::new(),
        floated: std::collections::BTreeSet::new(),
        sticky: std::collections::BTreeMap::new(),
        restart_reject_hold: std::collections::BTreeSet::new(),
        fixed_tile_overrides: std::collections::BTreeSet::new(),
        fixed_origin_overrides: std::collections::BTreeSet::new(),
        fixed_auto_seen: std::collections::BTreeSet::new(),
        born_first_exit: std::collections::BTreeSet::new(),
        overlay_now: std::collections::HashMap::new(),
        input_bridge: None,
        render_bridge: None,
        render_frames: RenderFrameHealth::default(),
        pending_releases: crate::workspace::PendingReleases::new(),
        pending_retry_fp: 0,
    };
    state.engine.sync_binding(&owner, &generation);
    state
}

#[test]
fn floating_retained_max_stays_slotless_then_tiled_seeds_and_plans() {
    use tiler_core::boundary::CoreReply;
    use tiler_core::directional::WindowId;
    use tiler_core::geometry::Rect;
    use tiler_core::ids::{CorrelationId, GenerationId, OwnerId};

    let mut state = test_state();
    let bounds = Rect {
        x: 0,
        y: 0,
        w: 1920,
        h: 1040,
    };
    let native_max = Rect {
        x: -8,
        y: -8,
        w: 1936,
        h: 1056,
    };
    let restored = Rect {
        x: 120,
        y: 120,
        w: 800,
        h: 600,
    };
    let output = "mon-1".to_owned();
    state.workspaces.ensure_output(&output);
    let active = state.workspaces.active_id(&output).expect("active");
    assert!(state.workspaces.set_tiled(&output, &active, false));
    assert_eq!(workspace_mode_known(&state, &output, &active), Some(false));

    // Slotless admission outcome without native effects: production
    // `admit_slotless_maximized` membership plus stable token, deliberately
    // no `member_rects` seed, no Engine exception, no hold.
    let key = crate::workspace::WindowKey {
        hwnd: 101,
        pid: 7,
        creation: "creation-max".to_owned(),
    };
    assert!(
        state
            .workspaces
            .assign(key.clone(), &output, &active, false)
    );
    state.member_tokens.insert(key.clone(), "w8".to_owned());
    assert!(!state.member_rects.contains_key("w8"));

    // Floating release assembly over the retained maximum: the actual
    // production row path, not the pure `should_seed_member_slot`
    // predicate. Mutation oracle: removing the `should_seed` guard (always
    // seeding) inserts `w8` here and fails the assertion below.
    let retained = vec![RetainedRow {
        key: key.clone(),
        token: "w8".to_owned(),
        rect: Some(native_max),
        maximized: true,
        fullscreen: false,
        facts: Some(crate::tiling::WindowFacts {
            visible: true,
            minimized: false,
            maximized: true,
            cloaked: false,
            elevated: false,
            shell: false,
            tool_window: false,
            owned: false,
            captionless_fullscreen: false,
            no_activate: false,
            dialog: false,
        }),
    }];
    let observed: Vec<super::ObservedWindow> = Vec::new();
    let mut hint_cx = HintCx::new();
    let rows = assemble_domain_rows(
        &mut state,
        &output,
        &active,
        &observed,
        &retained,
        "release",
        "rmax-03",
        &mut hint_cx,
    )
    .expect("floating rows assemble");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].token, "w8");
    assert!(
        !state.member_rects.contains_key("w8"),
        "floating retained maximum must stay slotless so the tiled admission clear still fires"
    );
    let (_, domain_key) = crate::workspace_owner::workspace_domain(
        &output,
        &active,
        bounds,
        crate::tiling::INNER_GAP,
    );
    assert!(state.engine.session(&domain_key).is_none());
    assert!(writable_tokens(&state, &output, &active, &observed).is_empty());

    // Toggle tiled, then refetch the restored frame into tiled assembly:
    // member-path seed plus an actual fresh Engine tile with an eligible
    // production writable token.
    assert!(state.workspaces.set_tiled(&output, &active, true));
    assert_eq!(workspace_mode_known(&state, &output, &active), Some(true));
    let refetched = vec![super::ObservedWindow {
        hwnd: 101,
        token: "w8".to_owned(),
        outer: restored,
        visible: restored,
        insets: crate::tiling::FrameInsets {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        },
        identity: crate::tiling::ObservedTarget {
            hwnd: 101,
            pid: 7,
            process_creation: "creation-max".to_owned(),
            exe_path: "C:\\test\\app.exe".to_owned(),
            user_sid: "S-1".to_owned(),
            session_id: 1,
            tag: String::new(),
        },
        facts: crate::tiling::WindowFacts {
            visible: true,
            minimized: false,
            maximized: false,
            cloaked: false,
            elevated: false,
            shell: false,
            tool_window: false,
            owned: false,
            captionless_fullscreen: false,
            no_activate: false,
            dialog: false,
        },
    }];
    let mut hint_cx = HintCx::new();
    let rows = assemble_domain_rows(
        &mut state,
        &output,
        &active,
        &refetched,
        &[],
        "reconcile",
        "rmax-03",
        &mut hint_cx,
    )
    .expect("tiled rows assemble");
    assert_eq!(rows.len(), 1);
    assert!(
        state.member_rects.contains_key("w8"),
        "tiled refetch must seed the tile slot"
    );
    let (domain, domain_key) = crate::workspace_owner::workspace_domain(
        &output,
        &active,
        bounds,
        crate::tiling::INNER_GAP,
    );
    let windows: Vec<(
        WindowId,
        Rect,
        tiler_core::size_hints::WindowSizeHints,
        bool,
    )> = rows
        .iter()
        .map(|r| (WindowId(r.token.clone()), r.rect, r.hints, r.floating))
        .collect();
    let owner = OwnerId::parse("tiler-windows").expect("owner");
    let generation = GenerationId::parse("rmax03-adapter").expect("generation");
    let fp = crate::tiling::fingerprint(
        &rows
            .iter()
            .map(|r| (r.token.clone(), r.rect))
            .collect::<Vec<_>>(),
    );
    let event = crate::tiling::build_reconcile_event_for_floating(
        &owner,
        &generation,
        &CorrelationId::parse("rmax-03").expect("correlation"),
        0,
        fp,
        &domain,
        &domain_key,
        crate::tiling::OUTER_GAP,
        &windows,
        None,
    );
    let reply = state.engine.handle(&event);
    let writes = crate::workspace_owner::planned_writes(&reply).expect("fresh tile plan");
    let tile = writes
        .iter()
        .find(|w| w.window.0 == "w8")
        .expect("tile contains the restored token");
    assert_ne!(tile.rect, native_max);
    assert_ne!(tile.rect, restored);
    assert!(
        matches!(reply, CoreReply::Tiled(_) | CoreReply::Projection(_)),
        "unexpected reply: {reply:?}"
    );
    assert_eq!(
        writable_tokens(&state, &output, &active, &refetched),
        std::collections::HashSet::from(["w8".to_owned()]),
        "restored member takes the production tiled write"
    );
}

#[test]
fn select_departure_remembers_live_fullscreen_game_for_return() {
    // REQ-WS-09: the chord-time game origin still holds live foreground
    // at dispatch, so the source remembers it before hiding. A slotless
    // born-fullscreen game carries membership (`admit_born_fullscreen`)
    // and rides the retained overlay row, so the return `focus_target`
    // restores it while `writable_tokens` still excludes it. Table
    // negatives pin exact identity, source membership, and liveness.
    // Hermetic: fabricated keys/tokens/rows only.
    use std::collections::HashSet;
    let output = "mon-9".to_owned();
    let game_key = crate::workspace::WindowKey {
        hwnd: 501,
        pid: 7,
        creation: "creation-game".to_owned(),
    };
    let steam_key = crate::workspace::WindowKey {
        hwnd: 502,
        pid: 8,
        creation: "creation-steam".to_owned(),
    };
    let other_key = crate::workspace::WindowKey {
        hwnd: 503,
        pid: 9,
        creation: "creation-other".to_owned(),
    };
    let game_token = "tok-game".to_owned();
    let steam_token = "tok-steam".to_owned();
    let other_token = "tok-other".to_owned();
    let game_origin = crate::snapkey::SnapOrigin {
        hwnd: 501,
        token: game_token.clone(),
        pid: 7,
        creation: "creation-game".to_owned(),
    };
    let rect = tiler_core::geometry::Rect {
        x: 0,
        y: 0,
        w: 800,
        h: 600,
    };
    let mk_observed = |hwnd: u64, token: String, pid: u32, creation: &str| super::ObservedWindow {
        hwnd,
        token,
        outer: rect,
        visible: rect,
        insets: crate::tiling::FrameInsets {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        },
        identity: crate::tiling::ObservedTarget {
            hwnd,
            pid,
            process_creation: creation.to_owned(),
            exe_path: "C:\\test\\app.exe".to_owned(),
            user_sid: "S-1-5-test".to_owned(),
            session_id: 1,
            tag: String::new(),
        },
        facts: crate::tiling::WindowFacts {
            visible: true,
            minimized: false,
            maximized: false,
            cloaked: false,
            elevated: false,
            shell: false,
            tool_window: false,
            owned: false,
            captionless_fullscreen: false,
            no_activate: false,
            dialog: false,
        },
    };
    let mk_game_row = || RetainedRow {
        key: game_key.clone(),
        token: game_token.clone(),
        rect: None,
        maximized: false,
        fullscreen: true,
        facts: None,
    };
    // Stale sibling memory on ws1 plus a bystander on ws2.
    let setup = || {
        let mut state = test_state();
        state.workspaces.ensure_output(&output);
        let ws1 = state.workspaces.active_id(&output).expect("active");
        let ws2 = state.workspaces.resolve_send(&output, 2).expect("ws2");
        assert!(
            state
                .workspaces
                .assign(game_key.clone(), &output, &ws1, false)
        );
        assert!(
            state
                .workspaces
                .assign(steam_key.clone(), &output, &ws1, false)
        );
        assert!(
            state
                .workspaces
                .assign(other_key.clone(), &output, &ws2, false)
        );
        state
            .member_tokens
            .insert(game_key.clone(), game_token.clone());
        state
            .member_tokens
            .insert(steam_key.clone(), steam_token.clone());
        state
            .member_tokens
            .insert(other_key.clone(), other_token.clone());
        state.workspaces.note_foreground(&steam_key);
        (state, ws1)
    };
    // Production return-target assembly: source members plus the
    // retained-inclusive fresh token union (focus only).
    let return_target = |state: &TileLoop,
                         ws1: &str,
                         observed: &[super::ObservedWindow],
                         retained: &[RetainedRow]| {
        let members = state.workspaces.workspace_members(&output, ws1);
        let mut fresh: HashSet<String> = observed.iter().map(|w| w.token.clone()).collect();
        for row in retained {
            if row.maximized || row.fullscreen {
                fresh.insert(row.token.clone());
            }
        }
        let eligible = state
            .workspaces
            .eligible_focus_set(&members, &state.member_tokens, &fresh);
        state.workspaces.focus_target(&output, ws1, &eligible)
    };
    // Live game: remembered, restored, and still geometry-exempt.
    let (mut state, ws1) = setup();
    let observed = [mk_observed(502, steam_token.clone(), 8, "creation-steam")];
    let retained = [mk_game_row()];
    assert_eq!(
        super::remember_select_departure(
            &mut state,
            &game_origin,
            &output,
            &ws1,
            501,
            &observed,
            &retained
        ),
        Some(game_token.clone())
    );
    assert_eq!(
        return_target(&state, &ws1, &observed, &retained),
        Some(game_key.clone()),
        "departure memory restores the live fullscreen game"
    );
    let writable = writable_tokens(&state, &output, &ws1, &observed);
    assert!(writable.contains(&steam_token));
    assert!(
        !writable.contains(&game_token),
        "fullscreen game takes no geometry writes"
    );
    // Changed foreground remembers nothing.
    let (mut state, ws1) = setup();
    let observed = [mk_observed(502, steam_token.clone(), 8, "creation-steam")];
    let retained = [mk_game_row()];
    assert_eq!(
        super::remember_select_departure(
            &mut state,
            &game_origin,
            &output,
            &ws1,
            502,
            &observed,
            &retained
        ),
        None
    );
    assert_eq!(
        return_target(&state, &ws1, &observed, &retained),
        Some(steam_key.clone())
    );
    // Wrong token: no member maps, nothing remembered.
    let (mut state, ws1) = setup();
    let observed = [mk_observed(502, steam_token.clone(), 8, "creation-steam")];
    let retained = [mk_game_row()];
    let evil = crate::snapkey::SnapOrigin {
        hwnd: 501,
        token: "tok-evil".to_owned(),
        pid: 7,
        creation: "creation-game".to_owned(),
    };
    assert_eq!(
        super::remember_select_departure(
            &mut state, &evil, &output, &ws1, 501, &observed, &retained
        ),
        None
    );
    assert_eq!(
        return_target(&state, &ws1, &observed, &retained),
        Some(steam_key.clone())
    );
    // PID mismatch: a recycled identity never matches.
    let (mut state, ws1) = setup();
    let observed = [mk_observed(502, steam_token.clone(), 8, "creation-steam")];
    let retained = [mk_game_row()];
    let recycled = crate::snapkey::SnapOrigin {
        hwnd: 501,
        token: game_token.clone(),
        pid: 999,
        creation: "creation-game".to_owned(),
    };
    assert_eq!(
        super::remember_select_departure(
            &mut state, &recycled, &output, &ws1, 501, &observed, &retained
        ),
        None
    );
    assert_eq!(
        return_target(&state, &ws1, &observed, &retained),
        Some(steam_key.clone())
    );
    // Other-workspace origin: source membership required.
    let (mut state, ws1) = setup();
    let observed = [
        mk_observed(502, steam_token.clone(), 8, "creation-steam"),
        mk_observed(503, other_token.clone(), 9, "creation-other"),
    ];
    let retained = [mk_game_row()];
    let elsewhere = crate::snapkey::SnapOrigin {
        hwnd: 503,
        token: other_token.clone(),
        pid: 9,
        creation: "creation-other".to_owned(),
    };
    assert_eq!(
        super::remember_select_departure(
            &mut state, &elsewhere, &output, &ws1, 503, &observed, &retained
        ),
        None
    );
    assert_eq!(
        return_target(&state, &ws1, &observed, &retained),
        Some(steam_key.clone())
    );
    // Vanished: game live nowhere (absent observed, no retained row).
    let (mut state, ws1) = setup();
    let observed = [mk_observed(502, steam_token.clone(), 8, "creation-steam")];
    let retained: [RetainedRow; 0] = [];
    assert_eq!(
        super::remember_select_departure(
            &mut state,
            &game_origin,
            &output,
            &ws1,
            501,
            &observed,
            &retained
        ),
        None
    );
    assert_eq!(
        return_target(&state, &ws1, &observed, &retained),
        Some(steam_key.clone())
    );
}

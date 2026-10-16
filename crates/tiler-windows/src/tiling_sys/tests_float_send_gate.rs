use super::float_send_admits;
use tiler_core::directional::WindowId;

fn bounds() -> tiler_core::geometry::Rect {
    tiler_core::geometry::Rect {
        x: 0,
        y: 0,
        w: 800,
        h: 600,
    }
}

fn mover_key() -> crate::workspace::WindowKey {
    crate::workspace::WindowKey {
        hwnd: 101,
        pid: 1101,
        creation: "c000000000000101".to_owned(),
    }
}

/// Seed one Engine session on (mon-a, ws-1) with wS tiled and wF
/// floated through the real Engine. Returns the domain key.
fn seed_float_session(state: &mut super::TileLoop) -> tiler_core::session::DomainKey {
    use tiler_core::boundary::CoreReply;
    use tiler_core::ids::CorrelationId;
    let bounds = bounds();
    let (domain, key) = crate::workspace_owner::workspace_domain("mon-a", "ws-1", bounds, 8);
    let owner = state.owner.clone();
    let generation = state.generation.clone();
    let seed = crate::tiling::build_reconcile_event_for(
        &owner,
        &generation,
        &CorrelationId::parse("seed").expect("correlation"),
        0,
        2,
        &domain,
        &key,
        8,
        &[
            (
                WindowId("wS".to_owned()),
                bounds,
                tiler_core::size_hints::WindowSizeHints::none(),
            ),
            (
                WindowId("wF".to_owned()),
                bounds,
                tiler_core::size_hints::WindowSizeHints::none(),
            ),
        ],
        Some(&WindowId("wS".to_owned())),
    );
    let reply = state.engine.handle(&seed);
    assert!(
        matches!(reply, CoreReply::Projection(_) | CoreReply::Tiled(_)),
        "seed converges, got {reply:?}"
    );
    let revision = state
        .engine
        .session(&key)
        .map(|s| s.accepted_revision())
        .unwrap_or(0);
    let float_frame = tiler_core::geometry::Rect {
        x: 100,
        y: 100,
        w: 400,
        h: 300,
    };
    let fp = crate::tiling::fingerprint(&[("wS".to_owned(), bounds), ("wF".to_owned(), bounds)]);
    let mut event = crate::tiling::build_reconcile_event_for_floating(
        &owner,
        &generation,
        &CorrelationId::parse("float").expect("correlation"),
        revision,
        fp,
        &domain,
        &key,
        8,
        &[
            (
                WindowId("wS".to_owned()),
                bounds,
                tiler_core::size_hints::WindowSizeHints::none(),
                false,
            ),
            (
                WindowId("wF".to_owned()),
                bounds,
                tiler_core::size_hints::WindowSizeHints::none(),
                false,
            ),
        ],
        Some(&WindowId("wS".to_owned())),
    );
    event.command = tiler_core::boundary::CoreCommand::ToggleFloat {
        window: "wF".to_owned(),
        float_rect: Some(float_frame),
    };
    let reply = state.engine.handle(&event);
    assert!(
        matches!(reply, CoreReply::Tiled(_)),
        "floatify commits, got {reply:?}"
    );
    assert!(
        state
            .engine
            .session(&key)
            .is_some_and(|s| s.is_exception(&WindowId("wF".to_owned()))),
        "mover floats slotless"
    );
    key
}

#[test]
fn float_send_gate_admits_ordinary_and_single_overlays_only() {
    // Narrowed gate (2026-10-13, extended 2026-10-16): ordinary
    // intentional floats, automatic fixed-size floats as ordinary members,
    // and approved floating+fullscreen / floating+maximized combos admit on
    // tiled-to-tiled sends; dual overlays, explicit tile overrides, and
    // non-floats refuse. Sticky stays with the caller's explicit subject
    // gate. Pure in-memory reads: no native calls, no Engine mutation.
    let mut state = super::rmax03_adapter_tests::test_state();
    let key = seed_float_session(&mut state);
    let mover = mover_key();
    let (output, workspace) = ("mon-a", "ws-1");
    assert_eq!(
        (key.output.0.as_str(), key.workspace.0.as_str()),
        (output, workspace)
    );
    // Ordinary float admits.
    state.overlay_now.insert("wF".to_owned(), (false, false));
    assert!(
        float_send_admits(&state, &mover, output, workspace, "wF"),
        "ordinary intentional float admits"
    );
    // Each approved single-overlay combo admits.
    state.overlay_now.insert("wF".to_owned(), (false, true));
    assert!(
        float_send_admits(&state, &mover, output, workspace, "wF"),
        "floating+fullscreen admits"
    );
    state.overlay_now.insert("wF".to_owned(), (true, false));
    assert!(
        float_send_admits(&state, &mover, output, workspace, "wF"),
        "floating+maximized admits"
    );
    // Dual overlays refuse.
    state.overlay_now.insert("wF".to_owned(), (true, true));
    assert!(
        !float_send_admits(&state, &mover, output, workspace, "wF"),
        "dual overlay refuses"
    );
    // Missing snapshot refuses closed.
    state.overlay_now.remove("wF");
    assert!(
        !float_send_admits(&state, &mover, output, workspace, "wF"),
        "missing overlay refuses"
    );
    // Automatic-fixed origin admits as an ordinary member (retained Engine
    // mark or D4 latch); only the explicit tile override refuses.
    state.overlay_now.insert("wF".to_owned(), (false, false));
    state.fixed_auto_seen.insert(mover.clone());
    assert!(
        float_send_admits(&state, &mover, output, workspace, "wF"),
        "D4-latched automatic admits"
    );
    state.fixed_auto_seen.remove(&mover);
    state.fixed_tile_overrides.insert(mover.clone());
    assert!(
        !float_send_admits(&state, &mover, output, workspace, "wF"),
        "tile-suppressed float refuses"
    );
    state.fixed_tile_overrides.remove(&mover);
    assert!(
        float_send_admits(&state, &mover, output, workspace, "wF"),
        "ordinary admits again after latch clear"
    );
    // A tiled (non-exception) token never admits.
    state.overlay_now.insert("wS".to_owned(), (false, false));
    assert!(
        !float_send_admits(&state, &mover, output, workspace, "wS"),
        "tiled token refuses"
    );
}

#[test]
fn float_send_gate_admits_automatic_fixed_as_ordinary_member() {
    // Windows workspace-send policy: automatic fixed-size floats are
    // ordinary slotless members and send like intentional floats (state
    // preserved, no geometry writes). Only the explicit tile override,
    // sticky (caller gate), and dual overlays refuse.
    let mut state = super::rmax03_adapter_tests::test_state();
    let _key = seed_float_session(&mut state);
    let mover = mover_key();
    let (output, workspace) = ("mon-a", "ws-1");
    state.overlay_now.insert("wF".to_owned(), (false, false));
    state.fixed_auto_seen.insert(mover.clone());
    assert!(
        float_send_admits(&state, &mover, output, workspace, "wF"),
        "D4-latched automatic fixed float admits as ordinary member"
    );
    state.overlay_now.insert("wF".to_owned(), (true, false));
    assert!(
        float_send_admits(&state, &mover, output, workspace, "wF"),
        "automatic fixed float with single overlay admits"
    );
    state.overlay_now.insert("wF".to_owned(), (true, true));
    assert!(
        !float_send_admits(&state, &mover, output, workspace, "wF"),
        "dual overlay still refuses"
    );
    state.overlay_now.insert("wF".to_owned(), (false, false));
    state.fixed_auto_seen.remove(&mover);
    state.fixed_tile_overrides.insert(mover.clone());
    assert!(
        !float_send_admits(&state, &mover, output, workspace, "wF"),
        "explicit tile override still refuses"
    );
    state.fixed_tile_overrides.remove(&mover);
}

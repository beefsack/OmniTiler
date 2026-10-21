#![forbid(unsafe_code)]
use super::admission::float_send_admits;

fn mover_key() -> crate::native_guards::WindowKey {
    crate::native_guards::WindowKey {
        hwnd: 101,
        pid: 1101,
        creation: "c000000000000101".to_owned(),
    }
}

/// Seed the world-owned float lane for `wF`: the adapter intent lane
/// (`floated` plus the stable token binding) carries the answer now that
/// the per-domain Engine session is deleted. station `wS` stays tiled
/// (bound but unmarked).
fn seed_float_session(state: &mut super::TileLoop) {
    let mover = mover_key();
    state.member_tokens.insert(mover.clone(), "wF".to_owned());
    state.floated.insert(mover);
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
    seed_float_session(&mut state);
    let mover = mover_key();
    let (output, workspace) = ("mon-a", "ws-1");
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
    seed_float_session(&mut state);
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

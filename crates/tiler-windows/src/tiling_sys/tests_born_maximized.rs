#![forbid(unsafe_code)]
use super::{born_maximized_keep_overlay_for, rmax03_adapter_tests::test_state};
use crate::settings::{BORN_MAXIMIZED_OVERLAY, BORN_MAXIMIZED_TILE, LiveSettings, Settings};
use tiler_core::geometry::Rect;

const BOUNDS: Rect = Rect {
    x: 0,
    y: 0,
    w: 1920,
    h: 1040,
};
const NATIVE_MAX: Rect = Rect {
    x: -8,
    y: -8,
    w: 1936,
    h: 1056,
};
const RESTORED: Rect = Rect {
    x: 120,
    y: 120,
    w: 800,
    h: 600,
};
const SIBLING: Rect = Rect {
    x: 0,
    y: 0,
    w: 800,
    h: 600,
};

/// Shared harness: proof-style owner state plus one workspace in the
/// requested mode. No HWNDs, hooks, or live effects.
fn harness(tiled: bool) -> (super::TileLoop, String, String) {
    let mut state = test_state();
    let output = "mon-1".to_owned();
    state.workspaces.ensure_output(&output);
    let active = state.workspaces.active_id(&output).expect("active");
    state.workspaces.set_tiled(&output, &active, tiled);
    assert_eq!(state.workspaces.is_tiled(&output, &active), tiled);
    (state, output, active)
}

fn max_key(hwnd: u64) -> crate::workspace::WindowKey {
    crate::workspace::WindowKey {
        hwnd,
        pid: 7,
        creation: "creation-live".to_owned(),
    }
}

fn live_with(born: &str) -> LiveSettings {
    let mut settings = Settings::default();
    settings.core.born_maximized = born.to_owned();
    LiveSettings::fresh(settings, None)
}

/// Stage the slotless-admission outcome for one fresh lifetime through the
/// actual production binding helpers: workspace membership without a tile
/// slot, plus the born latch exactly when production binds it and the
/// R-MAX-03 retile latch for floating-first admissions. Returns the key.
/// (`admit_slotless_maximized` itself needs live HWNDs for its ownership
/// fences, so staging its committed outcome through the real helpers is
/// the hermetic boundary; the helpers, the latch sets, and the gate below
/// are the production selection.)
fn admit_slotless(
    state: &mut super::TileLoop,
    output: &str,
    active: &str,
    hwnd: u64,
    token: &str,
) -> crate::workspace::WindowKey {
    let key = max_key(hwnd);
    assert!(state.workspaces.assign(key.clone(), output, active, false));
    state.member_tokens.insert(key.clone(), token.to_owned());
    assert!(!state.member_rects.contains_key(token));
    let tiled_first = state.workspaces.is_tiled(output, active);
    if tiler_core::window_policy::born_keep_latch(
        tiled_first,
        born_maximized_keep_overlay_for(state.settings_live.as_ref()),
    ) {
        state.born_keep_overlay.insert(key.clone());
    } else if !tiled_first {
        state.retile_keep_overlay.insert(key.clone());
    }
    key
}

/// Assemble one domain round through the production row path.
fn assemble(
    state: &mut super::TileLoop,
    output: &str,
    active: &str,
    observed: &[super::ObservedWindow],
    retained: &[super::RetainedRow],
) -> Vec<crate::workspace_owner::OwnerRow> {
    let mut hint_cx = super::HintCx::new();
    super::assemble_domain_rows(
        state,
        output,
        active,
        observed,
        retained,
        "reconcile",
        "rmax-06",
        &mut hint_cx,
    )
    .expect("rows assemble")
}

/// Accepted Engine revision for one domain, for follow-up rounds.
fn accepted_revision(state: &super::TileLoop, output: &str, active: &str) -> u64 {
    let (_, key) =
        crate::workspace_owner::workspace_domain(output, active, BOUNDS, crate::tiling::INNER_GAP);
    state
        .engine
        .session(&key)
        .map(|session| session.accepted_revision())
        .unwrap_or(0)
}

/// Drive one production reconcile round through the real retained Engine
/// and return the planned writes.
fn reconcile(
    state: &mut super::TileLoop,
    output: &str,
    active: &str,
    rows: &[crate::workspace_owner::OwnerRow],
    revision: u64,
) -> Vec<crate::workspace_owner::PlannedWrite> {
    use tiler_core::directional::WindowId;
    use tiler_core::ids::{CorrelationId, GenerationId, OwnerId};
    let (domain, domain_key) =
        crate::workspace_owner::workspace_domain(output, active, BOUNDS, crate::tiling::INNER_GAP);
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
    let generation = GenerationId::parse("rmax06-adapter").expect("generation");
    let fp = crate::tiling::fingerprint(
        &rows
            .iter()
            .map(|r| (r.token.clone(), r.rect))
            .collect::<Vec<_>>(),
    );
    let event = crate::tiling::build_reconcile_event_for_floating(
        &owner,
        &generation,
        &CorrelationId::parse("rmax-06").expect("correlation"),
        revision,
        fp,
        &domain,
        &domain_key,
        crate::tiling::OUTER_GAP,
        &windows,
        None,
    );
    let reply = state.engine.handle(&event);
    assert!(
        matches!(
            reply,
            tiler_core::boundary::CoreReply::Tiled(_)
                | tiler_core::boundary::CoreReply::Projection(_)
        ),
        "unexpected reply: {reply:?}"
    );
    crate::workspace_owner::planned_writes(&reply).expect("tile plan")
}

#[test]
fn latch_binds_tiled_first_overlay_only() {
    use tiler_core::born_maximized::BornMaximized;
    use tiler_core::window_policy::born_keep_latch;
    // Only the exact overlay wire token preserves maximize at admission.
    let keep_overlay = |value: &str| {
        BornMaximized::parse_wire(value).is_some_and(|mode| !mode.should_clear_at_admission())
    };
    // Exact wire values first: only the overlay token keeps; hand-built
    // near-misses tile, and proof owners (no live settings) read default.
    assert!(keep_overlay(BORN_MAXIMIZED_OVERLAY));
    assert!(!keep_overlay(BORN_MAXIMIZED_TILE));
    for bad in [
        "",
        "tile",
        "keep-overlay",
        "TILE-ON-LAUNCH",
        "KEEP-MAXIMIZED-OVERLAY",
        "group-with-neighbor",
    ] {
        assert!(!keep_overlay(bad), "{bad}");
    }
    assert!(!born_maximized_keep_overlay_for(None));
    assert!(!born_keep_latch(
        true,
        born_maximized_keep_overlay_for(None)
    ));
    // Lane matrix through the actual binding helper: tiled-first under
    // the overlay value latches; floating-first (R-MAX-03) never latches
    // under either value, and tiled-first under the tile default does not.
    assert!(born_keep_latch(true, true));
    assert!(!born_keep_latch(true, false));
    assert!(!born_keep_latch(false, true));
    assert!(!born_keep_latch(false, false));
}

#[test]
fn gate_honors_latch_with_legacy_gates_intact() {
    use tiler_core::window_policy::should_clear_maximize_at_admission_with_mode as gate;
    // The latched first tiled sighting keeps; the unlatched one clears.
    assert!(gate(false, true, false, false, true, false));
    assert!(!gate(false, true, false, false, true, true));
    // Every other gate is latch-independent, so R-MAX-03 retile,
    // born-fullscreen exits, and later maxima behave identically under
    // both values: fullscreen never clears, slotted members (later
    // maxima) never re-clear, attempts never retry, floating defers, and
    // non-maximized rows never clear.
    for (fullscreen, maximized, known_slot, attempted, tiled) in [
        (true, true, false, false, true),
        (false, true, true, false, true),
        (false, true, false, true, true),
        (false, true, false, false, false),
        (false, false, false, false, true),
    ] {
        for keep in [false, true] {
            assert!(
                !gate(fullscreen, maximized, known_slot, attempted, tiled, keep),
                "no clear for ({fullscreen},{maximized},{known_slot},{attempted},{tiled},{keep})"
            );
        }
    }
    // The legacy five-argument gate pins the unlatched lane exactly.
    for (fullscreen, maximized, known_slot, attempted, tiled) in [
        (false, true, false, false, true),
        (true, true, false, false, true),
        (false, true, true, false, true),
        (false, true, false, true, true),
        (false, true, false, false, false),
        (false, false, false, false, true),
    ] {
        assert_eq!(
            tiler_core::window_policy::should_clear_maximize_at_admission(
                fullscreen, maximized, known_slot, attempted, tiled
            ),
            gate(fullscreen, maximized, known_slot, attempted, tiled, false),
            "legacy gate pins tile lane"
        );
    }
}

#[test]
fn tile_bound_lifetime_clears_despite_overlay_flip() {
    // Tiled-first under the tile default binds no latch; flipping live to
    // overlay afterwards cannot move the earlier admission: it still clears.
    let (mut state, output, active) = harness(true);
    state.settings_live = Some(live_with(BORN_MAXIMIZED_TILE));
    let max = admit_slotless(&mut state, &output, &active, 101, "w8");
    assert!(!state.born_keep_overlay.contains(&max));
    state.settings_live = Some(live_with(BORN_MAXIMIZED_OVERLAY));
    assert!(!state.born_keep_overlay.contains(&max));
    assert!(
        tiler_core::window_policy::should_clear_maximize_at_admission_with_mode(
            false, true, false, false, true, false
        ),
        "earlier tile-bound admission still clears after the flip"
    );
}

fn maximized_row(
    key: &crate::workspace::WindowKey,
    token: &str,
    frame: tiler_core::geometry::Rect,
    maximized: bool,
) -> super::RetainedRow {
    super::RetainedRow {
        key: key.clone(),
        token: token.to_owned(),
        rect: Some(frame),
        maximized,
        fullscreen: false,
        facts: Some(crate::tiling::WindowFacts {
            visible: true,
            minimized: false,
            maximized,
            cloaked: false,
            elevated: false,
            shell: false,
            tool_window: false,
            owned: false,
            captionless_fullscreen: false,
            no_activate: false,
            dialog: false,
        }),
    }
}

fn observed_normal(
    hwnd: u64,
    token: &str,
    frame: tiler_core::geometry::Rect,
) -> super::ObservedWindow {
    super::ObservedWindow {
        hwnd,
        token: token.to_owned(),
        outer: frame,
        visible: frame,
        insets: crate::tiling::FrameInsets {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        },
        identity: crate::tiling::ObservedTarget {
            hwnd,
            pid: 7,
            process_creation: "creation-live".to_owned(),
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
    }
}

#[test]
fn overlay_born_reserves_slot_and_restore_lands_exact() {
    // Fixed-size detection is unwired on Windows (item 13 owns the gap):
    // every maximized row here rides the ordinary overlay path with no
    // fixed branch, so this delivery adds none.
    let (mut state, output, active) = harness(true);
    state.settings_live = Some(live_with(BORN_MAXIMIZED_OVERLAY));
    // Sibling already tiled; first-seen maximum admitted slotless through
    // the production binding helper (tiled-first + overlay latches).
    let sib = max_key(100);
    assert!(
        state
            .workspaces
            .assign(sib.clone(), &output, &active, false)
    );
    state.member_tokens.insert(sib.clone(), "w7".to_owned());
    state.member_rects.insert("w7".to_owned(), SIBLING);
    let max = admit_slotless(&mut state, &output, &active, 101, "w8");
    assert!(state.born_keep_overlay.contains(&max));
    // The bound lifetime, not the live value, gates its clear: flipping
    // live to tile cannot move it, and no attempt ever arms.
    state.settings_live = Some(live_with(BORN_MAXIMIZED_TILE));
    assert!(state.born_keep_overlay.contains(&max));
    assert!(
        !tiler_core::window_policy::should_clear_maximize_at_admission_with_mode(
            false, true, false, false, true, true
        )
    );
    assert!(state.maximize_admission_attempted.is_empty());
    state.settings_live = Some(live_with(BORN_MAXIMIZED_OVERLAY));

    // Production row assembly over the retained maximum plus the live
    // sibling: the slot seeds and both rows reach the real Engine.
    let observed = vec![observed_normal(100, "w7", SIBLING)];
    let retained = vec![maximized_row(&max, "w8", NATIVE_MAX, true)];
    let rows = assemble(&mut state, &output, &active, &observed, &retained);
    assert_eq!(rows.len(), 2);
    let overlaid = rows.iter().find(|r| r.token == "w8").expect("slot row");
    assert!(
        !overlaid.floating,
        "overlay keeps a tile slot, not a float exception"
    );
    assert!(
        state.member_rects.contains_key("w8"),
        "tiled overlay must seed the retained slot"
    );
    let writes = reconcile(&mut state, &output, &active, &rows, 0);
    let reserved = writes
        .iter()
        .find(|w| w.window.0 == "w8")
        .expect("reserved slot")
        .rect;
    assert_ne!(reserved, NATIVE_MAX, "slot is layout, not the max frame");
    assert!(
        writes.iter().any(|w| w.window.0 == "w7"),
        "sibling keeps its allocated share beside the overlay"
    );
    let writable = super::writable_tokens(&state, &output, &active, &observed);
    assert!(
        writable.contains("w7"),
        "sibling takes the production tiled write"
    );
    assert!(
        !writable.contains("w8"),
        "retained-only overlay takes no geometry write: no native fight"
    );

    // Repeated maximized signals settle: reassembly is stable, still no
    // attempt arms, still no writable fight.
    let slot = state.member_rects.get("w8").copied().expect("slot");
    let again = assemble(&mut state, &output, &active, &observed, &retained);
    assert_eq!(again.len(), 2);
    assert_eq!(
        state.member_rects.get("w8").copied(),
        Some(slot),
        "repeat signals must not move the reserved slot"
    );
    assert!(state.maximize_admission_attempted.is_empty());
    assert!(
        !super::writable_tokens(&state, &output, &active, &observed).contains("w8"),
        "repeat signals must not fight native state"
    );

    // Native restore refetches ordinary beside the sibling and lands
    // EXACTLY the reserved Engine geometry on the next Engine round.
    let refetched = vec![
        observed_normal(100, "w7", SIBLING),
        observed_normal(101, "w8", RESTORED),
    ];
    let rows = assemble(&mut state, &output, &active, &refetched, &[]);
    assert_eq!(rows.len(), 2);
    let revision = accepted_revision(&state, &output, &active);
    let writes = reconcile(&mut state, &output, &active, &rows, revision);
    let landed = writes
        .iter()
        .find(|w| w.window.0 == "w8")
        .expect("restored tile")
        .rect;
    assert_eq!(
        landed, reserved,
        "restore must land exactly the reserved slot"
    );
    assert_eq!(
        super::writable_tokens(&state, &output, &active, &refetched),
        std::collections::HashSet::from(["w7".to_owned(), "w8".to_owned()]),
        "restored member rejoins production writes beside its sibling"
    );
}

#[test]
fn floating_first_retile_overlays_under_both_values() {
    // R-MAX-03: a floating-first admission binds the retile latch under
    // EITHER `core.born_maximized` value (only born R-MAX-06 reads that
    // setting), so its first tiled admission reserves the overlay slot
    // with no one-shot clear; a live flip cannot move the bound lifetime.
    // Real retained Engine proof: sibling share, overlay write exclusion,
    // exact restore, and repeat stability.
    let (mut state, output, active) = harness(false);
    state.settings_live = Some(live_with(BORN_MAXIMIZED_TILE));
    let max = admit_slotless(&mut state, &output, &active, 101, "w8");
    assert!(
        state.retile_keep_overlay.contains(&max),
        "R-MAX-03 binds independently of the born setting"
    );
    assert!(
        !state.born_keep_overlay.contains(&max),
        "R-MAX-03 never takes the born latch"
    );
    // Floating assembly keeps it slotless (existing outcome).
    let retained = vec![maximized_row(&max, "w8", NATIVE_MAX, true)];
    let rows = assemble(&mut state, &output, &active, &[], &retained);
    assert_eq!(rows.len(), 1);
    assert!(
        !state.member_rects.contains_key("w8"),
        "floating maximum stays slotless"
    );
    // Retile under a flipped live value: no clear planned, slot seeds.
    state.settings_live = Some(live_with(BORN_MAXIMIZED_OVERLAY));
    assert!(state.workspaces.set_tiled(&output, &active, true));
    assert!(
        state.retile_keep_overlay.contains(&max),
        "live flip cannot move the bound lifetime"
    );
    assert!(
        !super::admission_clear_planned(&state, &retained[0], &super::HintCx::new()),
        "retile overlay never plans the one-shot clear"
    );
    // Sibling already tiled; production row assembly over the retained
    // maximum plus the live sibling reserves the slot with no clear.
    let sib = max_key(100);
    assert!(
        state
            .workspaces
            .assign(sib.clone(), &output, &active, false)
    );
    state.member_tokens.insert(sib.clone(), "w7".to_owned());
    state.member_rects.insert("w7".to_owned(), SIBLING);
    let observed = vec![observed_normal(100, "w7", SIBLING)];
    let rows = assemble(&mut state, &output, &active, &observed, &retained);
    assert_eq!(rows.len(), 2);
    let overlaid = rows.iter().find(|r| r.token == "w8").expect("slot row");
    assert!(
        !overlaid.floating,
        "retile overlay keeps a tile slot, not a float exception"
    );
    assert!(
        state.member_rects.contains_key("w8"),
        "tiled retile must seed the retained slot"
    );
    assert!(
        state.maximize_admission_attempted.is_empty(),
        "retile overlay arms no clear attempt"
    );
    let writes = reconcile(&mut state, &output, &active, &rows, 0);
    let reserved = writes
        .iter()
        .find(|w| w.window.0 == "w8")
        .expect("reserved slot")
        .rect;
    assert_ne!(reserved, NATIVE_MAX, "slot is layout, not the max frame");
    assert!(
        writes.iter().any(|w| w.window.0 == "w7"),
        "sibling keeps its allocated share beside the overlay"
    );
    let writable = super::writable_tokens(&state, &output, &active, &observed);
    assert!(
        writable.contains("w7"),
        "sibling takes the production tiled write"
    );
    assert!(
        !writable.contains("w8"),
        "retained-only overlay takes no geometry write: no native fight"
    );

    // Flipping live back to the tile default cannot move the bound
    // lifetime either: still no clear planned, still no attempt.
    state.settings_live = Some(live_with(BORN_MAXIMIZED_TILE));
    assert!(state.retile_keep_overlay.contains(&max));
    assert!(
        !super::admission_clear_planned(&state, &retained[0], &super::HintCx::new()),
        "retile overlay never plans the one-shot clear under either value"
    );
    assert!(state.maximize_admission_attempted.is_empty());

    // Repeated maximized signals settle: reassembly is stable, still no
    // attempt arms, still no writable fight.
    let slot = state.member_rects.get("w8").copied().expect("slot");
    let again = assemble(&mut state, &output, &active, &observed, &retained);
    assert_eq!(again.len(), 2);
    assert_eq!(
        state.member_rects.get("w8").copied(),
        Some(slot),
        "repeat signals must not move the reserved slot"
    );
    assert!(state.maximize_admission_attempted.is_empty());
    assert!(
        !super::writable_tokens(&state, &output, &active, &observed).contains("w8"),
        "repeat signals must not fight native state"
    );

    // Native restore refetches ordinary beside the sibling and lands
    // EXACTLY the reserved Engine geometry on the next Engine round.
    let refetched = vec![
        observed_normal(100, "w7", SIBLING),
        observed_normal(101, "w8", RESTORED),
    ];
    let rows = assemble(&mut state, &output, &active, &refetched, &[]);
    assert_eq!(rows.len(), 2);
    let revision = accepted_revision(&state, &output, &active);
    let writes = reconcile(&mut state, &output, &active, &rows, revision);
    let landed = writes
        .iter()
        .find(|w| w.window.0 == "w8")
        .expect("restored tile")
        .rect;
    assert_eq!(
        landed, reserved,
        "restore must land exactly the reserved slot"
    );
    assert_eq!(
        super::writable_tokens(&state, &output, &active, &refetched),
        std::collections::HashSet::from(["w7".to_owned(), "w8".to_owned()]),
        "restored member rejoins production writes beside its sibling"
    );

    // A recycled HWND mints a fresh lifetime: the stale member drops the
    // retile latch with its slot, so no retained overlay leaks across
    // windows.
    let fresh = crate::workspace::WindowKey {
        hwnd: 101,
        pid: 9,
        creation: "creation-new".to_owned(),
    };
    let stale = crate::workspace_owner::reused_hwnd_stale(
        &state.member_tokens.keys().cloned().collect::<Vec<_>>(),
        &state.hidden_claims.keys().cloned().collect(),
        &fresh,
    );
    assert_eq!(stale, vec![max.clone()]);
    for dead in stale {
        super::drop_member_state(&mut state, &dead);
    }
    assert!(
        !state.retile_keep_overlay.contains(&max),
        "latch drops with the lifetime"
    );
}

#[test]
fn floating_first_overlay_value_binds_retile_lane() {
    // LOW control beside the TILE-first main test: floating-first under
    // the OVERLAY value also binds the retile latch (never the born
    // latch); no clear planned; production assembly seeds the slot and
    // the real Engine reserves it beside the sibling.
    let (mut state, output, active) = harness(false);
    state.settings_live = Some(live_with(BORN_MAXIMIZED_OVERLAY));
    let max = admit_slotless(&mut state, &output, &active, 101, "w8");
    assert!(
        state.retile_keep_overlay.contains(&max),
        "floating-first binds retile under either value"
    );
    assert!(
        !state.born_keep_overlay.contains(&max),
        "floating-first never takes the born latch"
    );
    assert!(state.workspaces.set_tiled(&output, &active, true));
    let retained = vec![maximized_row(&max, "w8", NATIVE_MAX, true)];
    assert!(
        !super::admission_clear_planned(&state, &retained[0], &super::HintCx::new()),
        "overlay value keeps the retile overlay too"
    );
    let sib = max_key(100);
    assert!(
        state
            .workspaces
            .assign(sib.clone(), &output, &active, false)
    );
    state.member_tokens.insert(sib.clone(), "w7".to_owned());
    state.member_rects.insert("w7".to_owned(), SIBLING);
    let observed = vec![observed_normal(100, "w7", SIBLING)];
    let rows = assemble(&mut state, &output, &active, &observed, &retained);
    assert_eq!(rows.len(), 2);
    assert!(
        state.member_rects.contains_key("w8"),
        "tiled retile seeds the slot"
    );
    let writes = reconcile(&mut state, &output, &active, &rows, 0);
    let reserved = writes
        .iter()
        .find(|w| w.window.0 == "w8")
        .expect("reserved slot")
        .rect;
    assert_ne!(reserved, NATIVE_MAX, "slot is layout, not the max frame");
    assert!(
        writes.iter().any(|w| w.window.0 == "w7"),
        "sibling keeps its share"
    );
    assert!(
        state.maximize_admission_attempted.is_empty(),
        "retile overlay arms no clear attempt"
    );
}

#[test]
fn overlay_hwmd_reuse_stays_lifetime_safe() {
    // A recycled HWND mints a fresh lifetime: the stale member drops all
    // runtime state (slot included) and the new occupant starts slotless,
    // so no retained overlay leaks across windows.
    let mut state = test_state();
    let output = "mon-1".to_owned();
    state.workspaces.ensure_output(&output);
    let active = state.workspaces.active_id(&output).expect("active");
    state.workspaces.set_tiled(&output, &active, true);
    let dead = crate::workspace::WindowKey {
        hwnd: 101,
        pid: 7,
        creation: "creation-old".to_owned(),
    };
    assert!(
        state
            .workspaces
            .assign(dead.clone(), &output, &active, false)
    );
    state.member_tokens.insert(dead.clone(), "w8".to_owned());
    state.member_rects.insert(
        "w8".to_owned(),
        tiler_core::geometry::Rect {
            x: 0,
            y: 0,
            w: 400,
            h: 300,
        },
    );
    state.born_keep_overlay.insert(dead.clone());
    let fresh = crate::workspace::WindowKey {
        hwnd: 101,
        pid: 9,
        creation: "creation-new".to_owned(),
    };
    let stale = crate::workspace_owner::reused_hwnd_stale(
        &state.member_tokens.keys().cloned().collect::<Vec<_>>(),
        &state.hidden_claims.keys().cloned().collect(),
        &fresh,
    );
    assert_eq!(stale, vec![dead.clone()]);
    for dead in stale {
        super::drop_member_state(&mut state, &dead);
    }
    assert!(!state.member_tokens.contains_key(&dead));
    assert!(
        !state.born_keep_overlay.contains(&dead),
        "latch drops with the lifetime"
    );
    assert!(
        !state.member_rects.contains_key("w8"),
        "reused lifetime must not inherit the overlay slot"
    );
    assert!(state.workspaces.member_loc(&fresh).is_none());
}

#[test]
fn born_fullscreen_exit_and_later_maxima_stay_independent() {
    use tiler_core::window_policy::should_clear_maximize_at_admission_with_mode as gate;
    // Later maxima (retained slot) never clear under either latch state.
    for keep in [false, true] {
        assert!(!gate(false, true, true, false, true, keep));
    }
    // Born-fullscreen admission stages no latch (production
    // `admit_born_fullscreen` never touches the set): after the real
    // production release drops its seed, the exit to maximized is
    // slotless and clears under both live values.
    let (mut state, output, active) = harness(true);
    state.settings_live = Some(live_with(BORN_MAXIMIZED_OVERLAY));
    // The production release below logs its once-per-lifecycle line:
    // point the owner log at system temp and remove it afterwards, so the
    // hermetic run leaves no repo residue.
    let log_path = std::env::temp_dir().join(format!(
        "tiler-born-release-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    state.log_path = log_path.clone();
    let key = max_key(101);
    assert!(
        state
            .workspaces
            .assign(key.clone(), &output, &active, false)
    );
    state.member_tokens.insert(key.clone(), "w8".to_owned());
    state.member_rects.insert("w8".to_owned(), RESTORED);
    state.born_fullscreen.insert(key.clone());
    assert!(!state.born_keep_overlay.contains(&key));
    super::track_fullscreen_holds(&mut state, &[], &[]);
    assert!(!state.born_fullscreen.contains(&key));
    assert!(
        !state.member_rects.contains_key("w8"),
        "release drops the seed"
    );
    for born in [BORN_MAXIMIZED_TILE, BORN_MAXIMIZED_OVERLAY] {
        state.settings_live = Some(live_with(born));
        assert!(
            gate(
                false,
                true,
                false,
                false,
                true,
                state.born_keep_overlay.contains(&key)
            ),
            "born-fullscreen exit clears under {born}"
        );
    }
    let _ = std::fs::remove_file(&log_path);
}

#[test]
fn apply_born_only_returns_false_adopts_and_rebuilds_nothing() {
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
        "tiler-born-max-live-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let store = crate::storage::LedgerStore::open(&tmp).expect("ledger temp dir");
    let mut state = test_state();
    state.log_path = tmp.join("owner.log");
    // Align every lane with the defaults so the incoming document differs
    // in exactly one field.
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
            same_axis_move: super::same_axis_move_for(state.settings_live.as_ref()),
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
    incoming.core.born_maximized = BORN_MAXIMIZED_OVERLAY.to_owned();
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
        "born-maximized-only adoption must report no owner change"
    );
    assert_eq!(
        state.engine.session(&domain.1).expect("session").snapshot(),
        before,
        "retained session untouched: no rebuild, no resync"
    );
    let live = state.settings_live.as_ref().expect("live");
    assert_eq!(live.settings.core.born_maximized, BORN_MAXIMIZED_OVERLAY);
    assert_eq!(live.status, "saved:7");
    assert!(
        born_maximized_keep_overlay_for(state.settings_live.as_ref()),
        "subsequent admissions use the adopted setting"
    );
    assert!(
        state.born_keep_overlay.is_empty(),
        "setting-only Apply binds no lifetime"
    );
    assert_eq!((state.inner_gap, state.outer_gap), (8, 8));
    drop(store);
    let _ = std::fs::remove_dir_all(&tmp);
}

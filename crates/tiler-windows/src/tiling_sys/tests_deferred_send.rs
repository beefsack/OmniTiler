#![forbid(unsafe_code)]
use super::{
    ActionCtx, LedgerStore, MonitorArea, SendTarget, clear_pending_seams, inject_pending_tag,
    pending_counters,
};
use tiler_core::directional::WindowId;
use tiler_core::geometry::Rect;
use tiler_core::ids::CorrelationId;

const BOUNDS: Rect = Rect {
    x: 0,
    y: 0,
    w: 800,
    h: 600,
};
const FLOAT_FRAME: Rect = Rect {
    x: 100,
    y: 100,
    w: 400,
    h: 300,
};
/// Drifted survivor frame: nothing in the plan may chase it on a
/// deferred route.
const DRIFT: Rect = Rect {
    x: 50,
    y: 60,
    w: 700,
    h: 500,
};

fn mover_key() -> crate::workspace::WindowKey {
    crate::workspace::WindowKey {
        hwnd: 0x401,
        pid: 1401,
        creation: "c000000000000401".to_owned(),
    }
}

fn survivor_key() -> crate::workspace::WindowKey {
    crate::workspace::WindowKey {
        hwnd: 0x402,
        pid: 1402,
        creation: "c000000000000402".to_owned(),
    }
}

fn target_key() -> crate::workspace::WindowKey {
    crate::workspace::WindowKey {
        hwnd: 0x403,
        pid: 1403,
        creation: "c000000000000403".to_owned(),
    }
}

fn me() -> crate::model::ProcessIdentity {
    crate::model::ProcessIdentity {
        pid: 4242,
        process_creation: "test-creation".to_owned(),
        user_sid: "S-1-5-test".to_owned(),
        session_id: 1,
        exe_path: "test.exe".to_owned(),
    }
}

fn observed_window(
    key: &crate::workspace::WindowKey,
    token: &str,
    frame: Rect,
) -> super::ObservedWindow {
    super::ObservedWindow {
        hwnd: key.hwnd,
        token: token.to_owned(),
        outer: frame,
        visible: frame,
        insets: crate::tiling::FrameInsets::default(),
        identity: crate::tiling::ObservedTarget {
            hwnd: key.hwnd,
            pid: key.pid,
            process_creation: key.creation.clone(),
            exe_path: "test.exe".to_owned(),
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
    }
}

fn stage_member(
    state: &mut super::TileLoop,
    output: &str,
    workspace: &str,
    key: &crate::workspace::WindowKey,
    token: &str,
    tag: &str,
) {
    assert!(
        state
            .workspaces
            .assign(key.clone(), output, workspace, false)
    );
    state.member_tokens.insert(key.clone(), token.to_owned());
    state.member_identity.insert(
        key.clone(),
        crate::model::ProcessIdentity {
            pid: key.pid,
            process_creation: key.creation.clone(),
            user_sid: "S-1-5-test".to_owned(),
            session_id: 1,
            exe_path: "test.exe".to_owned(),
        },
    );
    state.member_tags.insert(key.clone(), tag.to_owned());
    inject_pending_tag(key.hwnd, Some(tag.to_owned()));
}

struct SendRig {
    state: super::TileLoop,
    me: crate::model::ProcessIdentity,
    store: LedgerStore,
    dir: std::path::PathBuf,
    areas: Vec<MonitorArea>,
    fulls: Vec<Rect>,
    source: String,
    target: String,
    mover: crate::workspace::WindowKey,
    survivor: crate::workspace::WindowKey,
}

/// Full send fixture: WS-source [wS survivor, wF mover floated through
/// the real Engine], WS-target [wT]. The survivor observes drifted; the
/// mover's live overlay snapshot is staged exactly as `overlay` (the
/// ordinary/combo shapes under test). Returns the rig plus the
/// caller-owned observed/retained pair.
fn send_fixture(
    overlay: (bool, bool),
) -> (SendRig, Vec<super::ObservedWindow>, Vec<super::RetainedRow>) {
    use tiler_core::boundary::{CoreCommand, CoreReply};
    clear_pending_seams();
    let mut state = super::rmax03_adapter_tests::test_state();
    let me = me();
    let tmp = std::env::temp_dir().join(format!(
        "tiler-d16-send-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let store = LedgerStore::open(&tmp).expect("ledger temp dir");
    let output = "mon-a".to_owned();
    state.workspaces.ensure_output(&output);
    let ids = state.workspaces.workspace_ids(&output);
    assert!(ids.len() >= 2, "source plus target");
    let (source, target) = (ids[0].clone(), ids[1].clone());
    state.workspaces.activate(&output, &source);
    let mover = mover_key();
    let survivor = survivor_key();
    let target_member = target_key();
    stage_member(&mut state, &output, &source, &mover, "wF", "tag-wf");
    stage_member(&mut state, &output, &source, &survivor, "wS", "tag-ws");
    stage_member(&mut state, &output, &target, &target_member, "wT", "tag-wt");
    // Intentional-float intent plus the live float frame, as the toggle
    // path leaves them.
    state.floated.insert(mover.clone());
    state.float_rects.insert("wF".to_owned(), FLOAT_FRAME);
    state.overlay_now.insert("wF".to_owned(), overlay);
    let areas = vec![MonitorArea {
        device: output.clone(),
        work: BOUNDS,
        full: BOUNDS,
    }];
    // Seed both Engine sessions through the exact production domains.
    let (source_domain, source_key) =
        super::workspace_domain_for(&output, &source, &areas, state.inner_gap, state.outer_gap)
            .expect("source domain");
    let (target_domain, target_key) =
        super::workspace_domain_for(&output, &target, &areas, state.inner_gap, state.outer_gap)
            .expect("target domain");
    let owner = state.owner.clone();
    let generation = state.generation.clone();
    for (domain, key, rows, focused) in [
        (
            &target_domain,
            &target_key,
            vec![(WindowId("wT".to_owned()), BOUNDS)],
            None,
        ),
        (
            &source_domain,
            &source_key,
            vec![
                (WindowId("wS".to_owned()), BOUNDS),
                (WindowId("wF".to_owned()), BOUNDS),
            ],
            Some(WindowId("wS".to_owned())),
        ),
    ] {
        let rows: Vec<(WindowId, Rect, tiler_core::size_hints::WindowSizeHints)> = rows
            .into_iter()
            .map(|(w, r)| (w, r, tiler_core::size_hints::WindowSizeHints::none()))
            .collect();
        let event = crate::tiling::build_reconcile_event_for(
            &owner,
            &generation,
            &CorrelationId::parse("seed").expect("correlation"),
            0,
            rows.len() as u64,
            domain,
            key,
            state.outer_gap,
            &rows,
            focused.as_ref(),
        );
        let reply = state.engine.handle(&event);
        assert!(
            matches!(reply, CoreReply::Projection(_) | CoreReply::Tiled(_)),
            "seed converges, got {reply:?}"
        );
    }
    // Floatify the mover through the real Engine with the pinned frame.
    let revision = state
        .engine
        .session(&source_key)
        .map(|s| s.accepted_revision())
        .unwrap_or(0);
    let fp = crate::tiling::fingerprint(&[("wS".to_owned(), BOUNDS), ("wF".to_owned(), BOUNDS)]);
    let mut event = crate::tiling::build_reconcile_event_for_floating(
        &owner,
        &generation,
        &CorrelationId::parse("float").expect("correlation"),
        revision,
        fp,
        &source_domain,
        &source_key,
        state.outer_gap,
        &[
            (
                WindowId("wS".to_owned()),
                BOUNDS,
                tiler_core::size_hints::WindowSizeHints::none(),
                false,
            ),
            (
                WindowId("wF".to_owned()),
                BOUNDS,
                tiler_core::size_hints::WindowSizeHints::none(),
                false,
            ),
        ],
        Some(&WindowId("wS".to_owned())),
    );
    event.command = CoreCommand::ToggleFloat {
        window: "wF".to_owned(),
        float_rect: Some(FLOAT_FRAME),
    };
    let reply = state.engine.handle(&event);
    assert!(
        matches!(reply, CoreReply::Tiled(_)),
        "floatify commits, got {reply:?}"
    );
    assert!(
        state
            .engine
            .session(&source_key)
            .is_some_and(|s| s.is_exception(&WindowId("wF".to_owned()))),
        "mover floats slotless"
    );
    // Caller-owned observation: the survivor observes drifted, the
    // mover at its float frame, the target at bounds.
    let observed = vec![
        observed_window(&mover, "wF", FLOAT_FRAME),
        observed_window(&survivor, "wS", DRIFT),
        observed_window(&target_member, "wT", BOUNDS),
    ];
    let rig = SendRig {
        state,
        me,
        store,
        dir: tmp,
        areas,
        fulls: vec![BOUNDS],
        source,
        target,
        mover,
        survivor,
    };
    // Route observation through the counting stub: every
    // `apply_geometry` call below increments the apply counter instead
    // of touching native windows.
    super::inject_pending_observe(observed.clone(), Vec::new(), false);
    (rig, observed, Vec::new())
}

impl Drop for SendRig {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn action_ctx(state: &super::TileLoop) -> ActionCtx {
    ActionCtx {
        correlation: "act-1".to_owned(),
        tick: state.tick,
        queued_at: std::time::Instant::now(),
        start: std::time::Instant::now(),
    }
}

#[test]
fn deferred_transfer_matrix_skips_applies_with_drift() {
    // Ordinary float x numbered/relative x follow/stay through the real
    // route with a drifted survivor: the deferred plan commits,
    // membership transfers, the target session holds the exception, and
    // intent/frame/overlay are retained with zero applies. Native hide
    // fails closed on the fake HWND (honest non-ok tails); routing
    // evidence is counts plus membership plus Engine state.
    for numbered in [false, true] {
        for follow in [false, true] {
            let (rig, effect, counters) = drive_send((false, false), numbered, follow, true);
            let shape = format!("numbered={numbered} follow={follow}");
            assert_eq!(counters.apply, 0, "{shape}: no geometry applies");
            assert!(effect.source.is_none(), "{shape}: no source summary");
            assert_eq!(
                rig.state
                    .workspaces
                    .member_loc(&rig.mover)
                    .map(|loc| loc.workspace.clone()),
                Some(rig.target.clone()),
                "{shape}: membership transfers"
            );
            let (_, target_key) = super::workspace_domain_for(
                "mon-a",
                &rig.target,
                &rig.areas,
                rig.state.inner_gap,
                rig.state.outer_gap,
            )
            .expect("target domain");
            assert!(
                rig.state
                    .engine
                    .session(&target_key)
                    .is_some_and(|s| { s.is_exception(&WindowId("wF".to_owned())) }),
                "{shape}: mover rehomes as a target exception"
            );
            assert!(
                rig.state.floated.contains(&rig.mover),
                "{shape}: float intent retained"
            );
            assert_eq!(
                rig.state.float_rects.get("wF").copied(),
                Some(FLOAT_FRAME),
                "{shape}: float frame retained"
            );
            assert_eq!(
                rig.state.overlay_now.get("wF").copied(),
                Some((false, false)),
                "{shape}: exact overlay snapshot retained"
            );
            clear_pending_seams();
        }
    }
}

#[test]
fn ordinary_tiled_matrix_still_applies_with_drift() {
    // Control matrix: the same route with a tiled mover still runs the
    // source application on every leg.
    for numbered in [false, true] {
        for follow in [false, true] {
            let (rig, effect, counters) = drive_send((false, false), numbered, follow, false);
            let shape = format!("numbered={numbered} follow={follow}");
            assert_eq!(counters.apply, 1, "{shape}: ordinary still applies");
            assert!(
                effect.source.is_some(),
                "{shape}: ordinary carries a source summary"
            );
            assert_eq!(
                rig.state
                    .workspaces
                    .member_loc(&rig.survivor)
                    .map(|loc| loc.workspace.clone()),
                Some(rig.target.clone()),
                "{shape}: ordinary membership transfers"
            );
            clear_pending_seams();
        }
    }
}

#[test]
fn combo_sends_defer_without_writes_on_unreadable_overlay() {
    // Floating+fullscreen / floating+maximized x numbered/relative x
    // follow/stay through the real route: the dispatch gate admits via
    // the staged `overlay_now` combo and the Engine plans, but the
    // post-plan live recheck cannot confirm the overlay on an unreadable
    // frame, so the overlay-hold seam defers honestly with no applies,
    // no membership change, and the source exception intact. Readable
    // native overlays proceed to transfer (native acceptance pending).
    for (overlay, name) in [((false, true), "fullscreen"), ((true, false), "maximized")] {
        for numbered in [false, true] {
            for follow in [false, true] {
                let (rig, effect, counters) = drive_send(overlay, numbered, follow, true);
                let shape = format!("{name} numbered={numbered} follow={follow}");
                assert_eq!(effect.outcome, "deferred", "{shape}: overlay hold defers");
                assert_eq!(counters.apply, 0, "{shape}: no geometry applies");
                assert!(effect.source.is_none(), "{shape}: no source summary");
                assert_eq!(
                    rig.state
                        .workspaces
                        .member_loc(&rig.mover)
                        .map(|loc| loc.workspace.clone()),
                    Some(rig.source.clone()),
                    "{shape}: membership unchanged"
                );
                let (_, source_key) = super::workspace_domain_for(
                    "mon-a",
                    &rig.source,
                    &rig.areas,
                    rig.state.inner_gap,
                    rig.state.outer_gap,
                )
                .expect("source domain");
                let (_, target_key) = super::workspace_domain_for(
                    "mon-a",
                    &rig.target,
                    &rig.areas,
                    rig.state.inner_gap,
                    rig.state.outer_gap,
                )
                .expect("target domain");
                // Post-commit deferral: the Engine already rehomes the
                // exception (commit-before-fences, converged next tick)
                // while adapter membership stands - the documented
                // no-replay shape. No geometry applies on either side.
                assert!(
                    rig.state
                        .engine
                        .session(&target_key)
                        .is_some_and(|s| { s.is_exception(&WindowId("wF".to_owned())) }),
                    "{shape}: Engine commits the deferred rehome"
                );
                assert!(
                    !rig.state
                        .engine
                        .session(&source_key)
                        .is_some_and(|s| { s.is_exception(&WindowId("wF".to_owned())) }),
                    "{shape}: source exception rehomes in the Engine"
                );
                clear_pending_seams();
            }
        }
    }
}

#[test]
fn ineligible_movers_refuse_without_transfer_or_writes() {
    // Sticky / explicit tile override / dual overlay through the real
    // dispatch: refusal vocabulary, zero applies, zero Engine contact,
    // membership and Engine state untouched. The D4 automatic latch no
    // longer refuses at dispatch (ordinary-member policy); it carries
    // past the float gate like an intentional float (pinned below).
    for (name, expected, overlay) in [
        ("sticky", "send-refused-sticky", (false, false)),
        ("suppress", "send-refused-floating", (false, false)),
        ("dual", "send-refused-floating", (true, true)),
    ] {
        let (mut rig, mut observed, _) = send_fixture(overlay);
        match name {
            "sticky" => {
                rig.state.sticky.insert(rig.mover.clone(), false);
            }
            "suppress" => {
                rig.state.fixed_tile_overrides.insert(rig.mover.clone());
            }
            _ => {}
        }
        let engine_before = format!("{:?}", rig.state.engine);
        let ctx = action_ctx(&rig.state);
        let effect = super::workspace_do_send(
            &mut rig.state,
            &rig.me,
            &rig.store,
            &rig.dir,
            &rig.fulls,
            &rig.areas,
            &mut observed,
            &[],
            "mon-a",
            SendTarget::Relative(1),
            true,
            rig.mover.hwnd,
            "wF",
            rig.mover.pid,
            &rig.mover.creation,
            &ctx,
        );
        let counters = pending_counters();
        assert_eq!(effect.outcome, expected, "{name}: refusal vocabulary");
        assert_eq!(counters.apply, 0, "{name}: no geometry applies");
        assert_eq!(counters.engine, 0, "{name}: no Engine contact");
        assert_eq!(
            rig.state
                .workspaces
                .member_loc(&rig.mover)
                .map(|loc| loc.workspace.clone()),
            Some(rig.source.clone()),
            "{name}: membership unchanged"
        );
        assert_eq!(
            format!("{:?}", rig.state.engine),
            engine_before,
            "{name}: Engine untouched"
        );
        clear_pending_seams();
    }
}

#[test]
fn automatic_latch_carries_past_dispatch_like_ordinary() {
    // D4-latched automatic fixed float through the real dispatch with the
    // production opt-in mirrored: passes the float gate (never
    // `send-refused-floating`), contacts the Engine, and transfers
    // membership to the target. Native hide fails closed on the fake
    // HWND, so only the pre-tail transfer is pinned here; the slotless
    // Engine carry itself is pinned in
    // `d16_automatic_fixed_carry_with_windows_send_opt_in`.
    let (mut rig, mut observed, _) = send_fixture((false, false));
    rig.state.fixed_auto_seen.insert(rig.mover.clone());
    rig.state.engine.set_fixed_send_admission(true);
    let ctx = action_ctx(&rig.state);
    let effect = super::workspace_do_send(
        &mut rig.state,
        &rig.me,
        &rig.store,
        &rig.dir,
        &rig.fulls,
        &rig.areas,
        &mut observed,
        &[],
        "mon-a",
        SendTarget::Relative(1),
        true,
        rig.mover.hwnd,
        "wF",
        rig.mover.pid,
        &rig.mover.creation,
        &ctx,
    );
    assert_ne!(
        effect.outcome, "send-refused-floating",
        "latch passes the float gate"
    );
    let counters = pending_counters();
    assert!(
        counters.engine > 0,
        "latch contacts the Engine, got {:?}",
        counters
    );
    assert_eq!(
        rig.state
            .workspaces
            .member_loc(&rig.mover)
            .map(|loc| loc.workspace.clone()),
        Some(rig.target.clone()),
        "membership transfers to the target"
    );
    clear_pending_seams();
}

/// Drives one real `workspace_do_send` for the matrix tests:
/// `overlay` stages the mover's live snapshot, `numbered` selects the
/// numbered ordinal (else the relative ring step) for the same target,
/// `mover_float` selects the float mover (else the tiled survivor).
/// Returns routing evidence; native hide fails closed on the fake HWND.
fn drive_send(
    overlay: (bool, bool),
    numbered: bool,
    follow: bool,
    mover_float: bool,
) -> (SendRig, super::SendEffect, super::PendingCounters) {
    let (mut rig, mut observed, _) = send_fixture(overlay);
    let target = if numbered {
        let index = (1..=9u8)
            .find(|i| {
                rig.state.workspaces.resolve_send("mon-a", *i).as_deref()
                    == Some(rig.target.as_str())
            })
            .expect("numbered ordinal for the target");
        SendTarget::Numbered(index)
    } else {
        SendTarget::Relative(1)
    };
    let (hwnd, token, pid, creation) = if mover_float {
        (
            rig.mover.hwnd,
            "wF",
            rig.mover.pid,
            rig.mover.creation.clone(),
        )
    } else {
        (
            rig.survivor.hwnd,
            "wS",
            rig.survivor.pid,
            rig.survivor.creation.clone(),
        )
    };
    let ctx = action_ctx(&rig.state);
    let effect = super::workspace_do_send(
        &mut rig.state,
        &rig.me,
        &rig.store,
        &rig.dir,
        &rig.fulls,
        &rig.areas,
        &mut observed,
        &[],
        "mon-a",
        target,
        follow,
        hwnd,
        token,
        pid,
        &creation,
        &ctx,
    );
    let counters = pending_counters();
    (rig, effect, counters)
}

/// Direct select skip: the deferred follow's target select runs
/// observation, reconcile, reveal, focus, and fences but applies no
/// geometry even with drifted frames; the ordinary select still
/// applies. The leaving set is fully hidden so the hide phase is a
/// no-op and the route reaches the reconcile application honestly.
fn select_fixture(
    skip_geometry: bool,
) -> (
    super::TileLoop,
    crate::model::ProcessIdentity,
    LedgerStore,
    std::path::PathBuf,
) {
    use super::{
        inject_pending_elevated, inject_pending_focus, inject_pending_observe,
        inject_pending_suspend,
    };
    use tiler_core::boundary::CoreReply;
    clear_pending_seams();
    let mut state = super::rmax03_adapter_tests::test_state();
    let me = me();
    let tmp = std::env::temp_dir().join(format!(
        "tiler-d16-select-{}-{}-{}",
        skip_geometry,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let store = LedgerStore::open(&tmp).expect("ledger temp dir");
    let output = "mon-a".to_owned();
    state.workspaces.ensure_output(&output);
    let ids = state.workspaces.workspace_ids(&output);
    assert!(ids.len() >= 2, "current plus target");
    let (current, target) = (ids[0].clone(), ids[1].clone());
    state.workspaces.activate(&output, &current);
    let mk = |hwnd: u64, pid: u32, creation: &str| crate::workspace::WindowKey {
        hwnd,
        pid,
        creation: creation.to_owned(),
    };
    let h1 = mk(0x501, 1501, "c000000000000501");
    let h2 = mk(0x502, 1502, "c000000000000502");
    let b1 = mk(0x503, 1503, "c000000000000503");
    for (key, ws, token, tag) in [
        (&h1, &current, "h1", "tag-h1"),
        (&h2, &current, "h2", "tag-h2"),
        (&b1, &target, "wB", "tag-wb"),
    ] {
        stage_member(&mut state, &output, ws, key, token, tag);
    }
    state.workspaces.set_hidden(&h1, true);
    state.workspaces.set_hidden(&h2, true);
    let areas = vec![MonitorArea {
        device: output.clone(),
        work: BOUNDS,
        full: BOUNDS,
    }];
    // Seed the target Engine session through the production domain.
    let (domain, key) =
        super::workspace_domain_for(&output, &target, &areas, state.inner_gap, state.outer_gap)
            .expect("target domain");
    let event = crate::tiling::build_reconcile_event_for(
        &state.owner.clone(),
        &state.generation.clone(),
        &CorrelationId::parse("seed").expect("correlation"),
        0,
        1,
        &domain,
        &key,
        state.outer_gap,
        &[(
            WindowId("wB".to_owned()),
            BOUNDS,
            tiler_core::size_hints::WindowSizeHints::none(),
        )],
        Some(&WindowId("wB".to_owned())),
    );
    let reply = state.engine.handle(&event);
    assert!(
        matches!(reply, CoreReply::Projection(_) | CoreReply::Tiled(_)),
        "target seed converges, got {reply:?}"
    );
    // Scripted fresh observation: the target member observes drifted;
    // the hidden leaving members ride retained so cleanup keeps them.
    let retained = |key: &crate::workspace::WindowKey, token: &str| super::RetainedRow {
        key: key.clone(),
        token: token.to_owned(),
        rect: Some(BOUNDS),
        maximized: false,
        fullscreen: false,
        facts: None,
    };
    inject_pending_observe(
        vec![observed_window(&b1, "wB", DRIFT)],
        vec![retained(&h1, "h1"), retained(&h2, "h2")],
        false,
    );
    inject_pending_suspend(Some(false));
    inject_pending_elevated(Some(false));
    inject_pending_focus(Some("focus-ok"));
    state.overlay_now.insert("wB".to_owned(), (false, false));
    let _ = (current, target);
    (state, me, store, tmp)
}

#[test]
fn deferred_select_skips_geometry_application_with_drift() {
    let (mut state, me, store, tmp) = select_fixture(true);
    let areas = vec![MonitorArea {
        device: "mon-a".to_owned(),
        work: BOUNDS,
        full: BOUNDS,
    }];
    let ids = state.workspaces.workspace_ids("mon-a");
    let target = ids[1].clone();
    let ctx = action_ctx(&state);
    let mut observed: Vec<super::ObservedWindow> = Vec::new();
    let effect = super::workspace_do_select(
        &mut state,
        &me,
        &store,
        &tmp,
        &[BOUNDS],
        &areas,
        &mut observed,
        "mon-a",
        &target,
        &ctx,
        None,
        false,
        true,
    );
    let counters = pending_counters();
    assert_eq!(effect.outcome, "ok", "select still completes");
    assert_eq!(counters.apply, 0, "deferred select applies no geometry");
    assert!(
        effect.geometry.is_none(),
        "no geometry summary on the skip route"
    );
    let _ = std::fs::remove_dir_all(&tmp);
    clear_pending_seams();
}

#[test]
fn ordinary_select_still_applies_with_drift() {
    let (mut state, me, store, tmp) = select_fixture(false);
    let areas = vec![MonitorArea {
        device: "mon-a".to_owned(),
        work: BOUNDS,
        full: BOUNDS,
    }];
    let ids = state.workspaces.workspace_ids("mon-a");
    let target = ids[1].clone();
    let ctx = action_ctx(&state);
    let mut observed: Vec<super::ObservedWindow> = Vec::new();
    let effect = super::workspace_do_select(
        &mut state,
        &me,
        &store,
        &tmp,
        &[BOUNDS],
        &areas,
        &mut observed,
        "mon-a",
        &target,
        &ctx,
        None,
        false,
        false,
    );
    let counters = pending_counters();
    assert_eq!(effect.outcome, "ok", "select still completes");
    assert_eq!(counters.apply, 1, "ordinary select still applies");
    assert!(
        effect.geometry.is_some(),
        "ordinary select carries a geometry summary"
    );
    let _ = std::fs::remove_dir_all(&tmp);
    clear_pending_seams();
}

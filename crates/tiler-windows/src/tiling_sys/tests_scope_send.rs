#![forbid(unsafe_code)]
//! Scoped send tests (item 12 stage B).
//!
//! Path-included from `tiling_sys.rs` (`#[path]`), so `super` is the
//! `tiling_sys` module and private seams (`workspace_do_send`,
//! `workspace_do_select`, `workspace_migrate_one`, the pending-injection
//! seams, `rmax03_adapter_tests::test_state`) stay reachable. Covers the
//! thin scoped-send call sites in `scope_sends.rs`: scoped
//! numbered/relative/trailing resolution, shared entry convergence,
//! output-bound transfers with real retained Engine sessions, float overlay
//! retention, follow convergence with one shared history entry, stay with no
//! history, shared trailing creation, shared migration refusal, and global
//! migration history validation. Fake HWNDs fail native effects closed, so
//! transfer/history/topology evidence (not native pixels) carries each leg.

use tiler_core::directional::WindowId;
use tiler_core::geometry::Rect;
use tiler_core::ids::CorrelationId;
use tiler_core::workspace_scope::WorkspaceMode;

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
const DRIFT: Rect = Rect {
    x: 50,
    y: 60,
    w: 700,
    h: 500,
};

fn me() -> crate::model::ProcessIdentity {
    crate::model::ProcessIdentity {
        pid: 4242,
        process_creation: "test-creation".to_owned(),
        user_sid: "S-1-5-test".to_owned(),
        session_id: 1,
        exe_path: "test.exe".to_owned(),
    }
}

fn key(hwnd: u64, pid: u32, creation: &str) -> crate::workspace::WindowKey {
    crate::workspace::WindowKey {
        hwnd,
        pid,
        creation: creation.to_owned(),
    }
}

/// Two-output owner state with the mode adopted and the shared order synced:
/// mon-a holds ws-1/ws-2 (ws-1 active), mon-b holds ws-3/ws-4 (ws-3 active).
fn two_output_state(mode: WorkspaceMode) -> super::TileLoop {
    let mut state = super::rmax03_adapter_tests::test_state();
    state.workspaces.ensure_output("mon-a");
    state.workspaces.ensure_output("mon-b");
    state.scope_owner = crate::workspace_scope_owner::WorkspaceScopeOwner::adopt_startup(mode);
    state.scope_owner.sync_shared_order(&state.workspaces);
    state.active_output = "mon-a".to_owned();
    state
}

fn areas() -> Vec<super::MonitorArea> {
    vec![
        super::MonitorArea {
            device: "mon-a".to_owned(),
            work: BOUNDS,
            full: BOUNDS,
        },
        super::MonitorArea {
            device: "mon-b".to_owned(),
            work: BOUNDS,
            full: BOUNDS,
        },
    ]
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
            .assign(key.clone(), output, workspace, false),
        "stage assigns"
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
    super::inject_pending_tag(key.hwnd, Some(tag.to_owned()));
}

fn action_ctx(state: &super::TileLoop) -> super::ActionCtx {
    super::ActionCtx {
        correlation: "act-1".to_owned(),
        tick: state.tick,
        queued_at: std::time::Instant::now(),
        start: std::time::Instant::now(),
    }
}

/// Seed one Engine session through the production domain so send plans run
/// against retained state instead of reseeding.
fn seed_session(
    state: &mut super::TileLoop,
    areas: &[super::MonitorArea],
    output: &str,
    workspace: &str,
    rows: Vec<(String, Rect)>,
) {
    let (domain, key) =
        super::workspace_domain_for(output, workspace, areas, state.inner_gap, state.outer_gap)
            .expect("seed domain");
    let rows: Vec<(WindowId, Rect, tiler_core::size_hints::WindowSizeHints)> = rows
        .into_iter()
        .map(|(w, r)| {
            (
                WindowId(w),
                r,
                tiler_core::size_hints::WindowSizeHints::none(),
            )
        })
        .collect();
    let event = crate::tiling::build_reconcile_event_for(
        &state.owner.clone(),
        &state.generation.clone(),
        &CorrelationId::parse("seed").expect("correlation"),
        0,
        rows.len() as u64,
        &domain,
        &key,
        state.outer_gap,
        &rows,
        None,
    );
    let reply = state.engine.handle(&event);
    assert!(
        matches!(
            reply,
            tiler_core::boundary::CoreReply::Projection(_)
                | tiler_core::boundary::CoreReply::Tiled(_)
        ),
        "seed converges, got {reply:?}"
    );
}

/// Floatify one staged mover through the real Engine with its pinned frame,
/// exactly like the production toggle path leaves it.
fn floatify_mover(
    state: &mut super::TileLoop,
    areas: &[super::MonitorArea],
    output: &str,
    ws: &str,
) {
    let (domain, key) =
        super::workspace_domain_for(output, ws, areas, state.inner_gap, state.outer_gap)
            .expect("float domain");
    let revision = state
        .engine
        .session(&key)
        .map(|s| s.accepted_revision())
        .unwrap_or(0);
    let fp = crate::tiling::fingerprint(&[("wS".to_owned(), BOUNDS), ("wF".to_owned(), BOUNDS)]);
    let mut event = crate::tiling::build_reconcile_event_for_floating(
        &state.owner.clone(),
        &state.generation.clone(),
        &CorrelationId::parse("float").expect("correlation"),
        revision,
        fp,
        &domain,
        &key,
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
    event.command = tiler_core::boundary::CoreCommand::ToggleFloat {
        window: "wF".to_owned(),
        float_rect: Some(FLOAT_FRAME),
    };
    let reply = state.engine.handle(&event);
    assert!(
        matches!(reply, tiler_core::boundary::CoreReply::Tiled(_)),
        "floatify commits, got {reply:?}"
    );
}

fn tmp_ledger(tag: &str) -> (crate::storage::LedgerStore, std::path::PathBuf) {
    let tmp = std::env::temp_dir().join(format!(
        "tiler-scope-send-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let store = crate::storage::LedgerStore::open(&tmp).expect("ledger temp dir");
    (store, tmp)
}

use super::scope_sends::{ScopedSendTarget, resolve_scoped_send_target};

#[test]
fn local_send_resolvers_match_managed_equivalents() {
    super::clear_pending_seams();
    let mut state = two_output_state(WorkspaceMode::PerOutputLocal);
    for output in ["mon-a", "mon-b"] {
        for index in 1..=9u8 {
            let scoped = resolve_scoped_send_target(
                &mut state,
                output,
                "ws-1",
                super::SendTarget::Numbered(index),
            );
            assert_eq!(
                scoped,
                match state.workspaces.resolve_send(output, index) {
                    Some(id) => ScopedSendTarget::Id(id),
                    None => ScopedSendTarget::Unknown("unknown-target"),
                },
                "numbered {index} on {output}"
            );
        }
        for delta in [-1, 1] {
            let source = state.workspaces.active_id(output).expect("active");
            let scoped = resolve_scoped_send_target(
                &mut state,
                output,
                &source,
                super::SendTarget::Relative(delta),
            );
            assert_eq!(
                scoped,
                match state
                    .workspaces
                    .resolve_relative_from(output, &source, delta)
                {
                    Some(id) => ScopedSendTarget::Id(id),
                    None => ScopedSendTarget::Unknown("unknown-target"),
                },
                "relative {delta} on {output}"
            );
        }
    }
    // Local trailing reuse matches the workspace resolver without creating.
    let scoped =
        resolve_scoped_send_target(&mut state, "mon-a", "ws-1", super::SendTarget::Numbered(0));
    assert_eq!(
        scoped,
        ScopedSendTarget::Id("ws-2".to_owned()),
        "local trailing reuses the empty tail"
    );
    super::clear_pending_seams();
}

#[test]
fn shared_numbered_resolves_over_shared_ring() {
    super::clear_pending_seams();
    let mut state = two_output_state(WorkspaceMode::Shared);
    // Shared ring is the synced union: mon-a's indices reach mon-b-homed ids.
    for (index, want) in [(1, "ws-1"), (2, "ws-2"), (3, "ws-3"), (4, "ws-4")] {
        let scoped = resolve_scoped_send_target(
            &mut state,
            "mon-a",
            "ws-1",
            super::SendTarget::Numbered(index),
        );
        assert_eq!(scoped, ScopedSendTarget::Id(want.to_owned()));
    }
    assert_eq!(
        resolve_scoped_send_target(&mut state, "mon-a", "ws-1", super::SendTarget::Numbered(5)),
        ScopedSendTarget::Unknown("unknown-target")
    );
    super::clear_pending_seams();
}

#[test]
fn shared_relative_resolves_from_mover_source() {
    super::clear_pending_seams();
    let mut state = two_output_state(WorkspaceMode::Shared);
    // From the mover source (not the active view): ws-1 +1 is ws-2, ws-1 -1
    // wraps to ws-4, and an unknown source refuses without creating.
    assert_eq!(
        resolve_scoped_send_target(&mut state, "mon-a", "ws-1", super::SendTarget::Relative(1)),
        ScopedSendTarget::Id("ws-2".to_owned())
    );
    assert_eq!(
        resolve_scoped_send_target(&mut state, "mon-a", "ws-1", super::SendTarget::Relative(-1)),
        ScopedSendTarget::Id("ws-4".to_owned())
    );
    assert_eq!(
        resolve_scoped_send_target(
            &mut state,
            "mon-a",
            "ws-gone",
            super::SendTarget::Relative(1)
        ),
        ScopedSendTarget::Unknown("unknown-target")
    );
    super::clear_pending_seams();
}

#[test]
fn shared_trailing_occupied_creates_one_logical_id() {
    super::clear_pending_seams();
    let mut state = two_output_state(WorkspaceMode::Shared);
    let before_a = state.workspaces.workspace_ids("mon-a");
    let before_b = state.workspaces.workspace_ids("mon-b");
    // Occupy the shared trailing id (ws-4, the ring tail) on its home output.
    let filler = key(0x701, 1701, "c000000000000701");
    stage_member(&mut state, "mon-b", "ws-4", &filler, "wFill", "tag-fill");
    // Occupied shared tail mints one logical id across every output domain
    // (no deferral, no refusal): the send proceeds to the fresh id.
    let scoped =
        resolve_scoped_send_target(&mut state, "mon-a", "ws-1", super::SendTarget::Numbered(0));
    let created = match scoped {
        ScopedSendTarget::Id(id) => id,
        ScopedSendTarget::Unknown(outcome) => {
            panic!("occupied shared tail must create, got refusal {outcome}")
        }
    };
    assert!(
        !before_a.contains(&created) && !before_b.contains(&created),
        "created id {created} is fresh"
    );
    for output in ["mon-a", "mon-b"] {
        assert!(
            state.workspaces.workspace_ids(output).contains(&created),
            "logical {created} converged onto {output}"
        );
    }
    assert!(state.scope_owner.shared_order().contains(&created));
    // The filler stays homed on the prior tail.
    assert_eq!(
        state
            .workspaces
            .member_loc(&filler)
            .expect("filler homed")
            .workspace,
        "ws-4"
    );
    super::clear_pending_seams();
}

#[test]
fn global_numbered_never_invents_cross_output_targets() {
    super::clear_pending_seams();
    let mut state = two_output_state(WorkspaceMode::GlobalUnique);
    // mon-a holds two ids: index 3 is unknown, never mon-b's ws-3.
    assert_eq!(
        resolve_scoped_send_target(&mut state, "mon-a", "ws-1", super::SendTarget::Numbered(2)),
        ScopedSendTarget::Id("ws-2".to_owned())
    );
    assert_eq!(
        resolve_scoped_send_target(&mut state, "mon-a", "ws-1", super::SendTarget::Numbered(3)),
        ScopedSendTarget::Unknown("unknown-target")
    );
    // Relative wraps inside the requesting output's ring only.
    assert_eq!(
        resolve_scoped_send_target(&mut state, "mon-a", "ws-2", super::SendTarget::Relative(1)),
        ScopedSendTarget::Id("ws-1".to_owned())
    );
    super::clear_pending_seams();
}

#[test]
fn shared_stay_converges_cross_homed_target_without_history() {
    super::clear_pending_seams();
    let me = me();
    let mut state = two_output_state(WorkspaceMode::Shared);
    let areas = areas();
    let (store, tmp) = tmp_ledger("stay-converge");
    // Tiled mover plus survivor on mon-a ws-1; target ws-3 lives only on
    // mon-b, so the transfer must converge it onto mon-a first.
    let mover = key(0x711, 1711, "c000000000000711");
    let survivor = key(0x712, 1712, "c000000000000712");
    stage_member(&mut state, "mon-a", "ws-1", &mover, "wM", "tag-wm");
    stage_member(&mut state, "mon-a", "ws-1", &survivor, "wS", "tag-ws");
    seed_session(
        &mut state,
        &areas,
        "mon-a",
        "ws-1",
        vec![("wM".to_owned(), BOUNDS), ("wS".to_owned(), BOUNDS)],
    );
    let count_a_before = state.workspaces.workspace_ids("mon-a").len();
    let count_b_before = state.workspaces.workspace_ids("mon-b").len();
    let observed = vec![
        observed_window(&mover, "wM", BOUNDS),
        observed_window(&survivor, "wS", DRIFT),
    ];
    let ctx = action_ctx(&state);
    let effect = super::workspace_do_send(
        &mut state,
        &me,
        &store,
        &tmp,
        &[BOUNDS],
        &areas,
        &mut observed.clone(),
        &[],
        "mon-a",
        super::SendTarget::Numbered(3),
        false,
        mover.hwnd,
        "wM",
        mover.pid,
        &mover.creation,
        &ctx,
    );
    // Output-bound domain preserved: same output, new logical workspace.
    let loc = state.workspaces.member_loc(&mover).expect("mover homed");
    assert_eq!(loc.output, "mon-a");
    assert_eq!(loc.workspace, "ws-3");
    // The requesting output converged the logical id; mon-b untouched and no
    // trailing spare was needed in this fixture.
    assert!(
        state
            .workspaces
            .workspace_ids("mon-a")
            .contains(&"ws-3".to_owned())
    );
    assert_eq!(
        state.workspaces.workspace_ids("mon-a").len(),
        count_a_before + 1
    );
    assert_eq!(
        state.workspaces.workspace_ids("mon-b").len(),
        count_b_before
    );
    // Stay leaves the logical current unchanged with no history on any lane.
    assert_eq!(state.workspaces.active_id("mon-a").as_deref(), Some("ws-1"));
    assert_eq!(state.scope_owner.history().shared_previous(), None);
    assert_eq!(state.scope_owner.history().previous("mon-a"), None);
    assert_eq!(state.workspaces.history_previous("mon-a"), None);
    // The Engine route ran its source reflow before the honest native tail.
    assert!(effect.source.is_some(), "source reflow ran");
    let _ = std::fs::remove_dir_all(&tmp);
    super::clear_pending_seams();
}

#[test]
fn shared_stay_retains_float_overlay_and_engine_exception() {
    super::clear_pending_seams();
    for (overlay, name) in [((false, false), "plain"), ((true, false), "maximized")] {
        let me = me();
        let mut state = two_output_state(WorkspaceMode::Shared);
        let areas = areas();
        let (store, tmp) = tmp_ledger("stay-float");
        // D16 admitted float mover plus tiled survivor on mon-a ws-1;
        // target ws-2 already lives on mon-a with its own member.
        let mover = key(0x721, 1721, "c000000000000721");
        let survivor = key(0x722, 1722, "c000000000000722");
        let target_member = key(0x723, 1723, "c000000000000723");
        stage_member(&mut state, "mon-a", "ws-1", &mover, "wF", "tag-wf");
        stage_member(&mut state, "mon-a", "ws-1", &survivor, "wS", "tag-ws");
        stage_member(&mut state, "mon-a", "ws-2", &target_member, "wT", "tag-wt");
        state.floated.insert(mover.clone());
        state.float_rects.insert("wF".to_owned(), FLOAT_FRAME);
        state.overlay_now.insert("wF".to_owned(), overlay);
        seed_session(
            &mut state,
            &areas,
            "mon-a",
            "ws-2",
            vec![("wT".to_owned(), BOUNDS)],
        );
        seed_session(
            &mut state,
            &areas,
            "mon-a",
            "ws-1",
            vec![("wS".to_owned(), BOUNDS), ("wF".to_owned(), BOUNDS)],
        );
        floatify_mover(&mut state, &areas, "mon-a", "ws-1");
        // Script every live overlay read to the staged snapshot so the
        // post-plan gates compare like-for-like on the fake HWND.
        super::inject_pending_overlay_seq(vec![(overlay.0, Some(overlay.1)); 16]);
        let observed = vec![
            observed_window(&mover, "wF", FLOAT_FRAME),
            observed_window(&survivor, "wS", DRIFT),
            observed_window(&target_member, "wT", BOUNDS),
        ];
        let ctx = action_ctx(&state);
        let effect = super::workspace_do_send(
            &mut state,
            &me,
            &store,
            &tmp,
            &[BOUNDS],
            &areas,
            &mut observed.clone(),
            &[],
            "mon-a",
            super::SendTarget::Numbered(2),
            false,
            mover.hwnd,
            "wF",
            mover.pid,
            &mover.creation,
            &ctx,
        );
        let loc = state.workspaces.member_loc(&mover).expect("mover homed");
        assert_eq!(
            (loc.output.as_str(), loc.workspace.as_str()),
            ("mon-a", "ws-2"),
            "{name}"
        );
        // Real retained Engine transfer: the slotless mover rehomes as a
        // target exception with no geometry application.
        let (_, target_key) =
            super::workspace_domain_for("mon-a", "ws-2", &areas, state.inner_gap, state.outer_gap)
                .expect("target domain");
        assert!(
            state
                .engine
                .session(&target_key)
                .is_some_and(|s| s.is_exception(&WindowId("wF".to_owned()))),
            "{name}: mover rehomes as a target exception"
        );
        assert!(
            state.floated.contains(&mover),
            "{name}: float intent retained"
        );
        assert_eq!(
            state.float_rects.get("wF").copied(),
            Some(FLOAT_FRAME),
            "{name}: float frame retained"
        );
        assert_eq!(
            state.overlay_now.get("wF").copied(),
            Some(overlay),
            "{name}: exact overlay snapshot retained"
        );
        assert!(effect.source.is_none(), "{name}: deferred skips applies");
        // Stay records nothing and keeps the source view selected.
        assert_eq!(
            state.workspaces.active_id("mon-a").as_deref(),
            Some("ws-1"),
            "{name}"
        );
        assert_eq!(
            state.scope_owner.history().shared_previous(),
            None,
            "{name}"
        );
        assert_eq!(state.workspaces.history_previous("mon-a"), None, "{name}");
        let _ = std::fs::remove_dir_all(&tmp);
        super::clear_pending_seams();
    }
}

#[test]
fn shared_follow_select_converges_all_outputs_with_one_history() {
    super::clear_pending_seams();
    let me = me();
    let mut state = two_output_state(WorkspaceMode::Shared);
    let areas = areas();
    let (store, tmp) = tmp_ledger("follow-converge");
    // mon-a shows ws-1, mon-b shows ws-3; ws-2 lives only on mon-a with a
    // claim-free member. Both leaving sets are empty so the hide phase is a
    // no-op and the route reaches the shared tail honestly.
    let member = key(0x731, 1731, "c000000000000731");
    stage_member(&mut state, "mon-a", "ws-2", &member, "wT", "tag-wt");
    state
        .scope_owner
        .prime_shared_view(&state.workspaces, "mon-a");
    super::inject_pending_observe(Vec::new(), Vec::new(), false);
    super::inject_pending_suspend(Some(false));
    super::inject_pending_elevated(Some(false));
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
        "ws-2",
        &ctx,
        None,
        false,
        false,
    );
    assert_eq!(effect.outcome, "ok", "shared select completes");
    // Follow switches every output onto the logical id through the common
    // dispatcher, converging mon-b's missing entry.
    assert_eq!(state.workspaces.active_id("mon-a").as_deref(), Some("ws-2"));
    assert_eq!(state.workspaces.active_id("mon-b").as_deref(), Some("ws-2"));
    assert!(
        state
            .workspaces
            .workspace_ids("mon-b")
            .contains(&"ws-2".to_owned())
    );
    // Exactly one shared history entry for the completed transition.
    // Trailing lifecycle then converges the ring past it: the vacated
    // empties (ws-1, ws-3) prune as invisible non-final ids exactly like
    // the local select path, so the just-recorded previous clears like a
    // removal instead of dangling at a pruned id.
    let ring = state.scope_owner.shared_ring(&state.workspaces);
    assert!(
        !ring.contains(&"ws-1".to_owned()) && !ring.contains(&"ws-3".to_owned()),
        "vacated empties prune from the one logical ring, got {ring:?}"
    );
    assert!(
        ring.len() >= tiler_core::workspace::MIN_WORKSPACES,
        "shared floor holds, got {ring:?}"
    );
    assert_eq!(
        state.scope_owner.history().shared_previous(),
        None,
        "pruned previous clears like removal"
    );
    assert_eq!(state.scope_owner.history().previous("mon-a"), None);
    let _ = std::fs::remove_dir_all(&tmp);
    super::clear_pending_seams();
}

#[test]
fn shared_migration_refuses_without_mutation() {
    super::clear_pending_seams();
    let me = me();
    let mut state = two_output_state(WorkspaceMode::Shared);
    let areas = areas();
    let (store, tmp) = tmp_ledger("migrate-refuse");
    let engine_before = format!("{:?}", state.engine);
    let ids_a_before = state.workspaces.workspace_ids("mon-a");
    let ids_b_before = state.workspaces.workspace_ids("mon-b");
    let intent = crate::snapkey::QueuedWorkspaceMigrateIntent {
        direction: tiler_core::directional::Direction::Right,
        edge: crate::snapkey::SnapEdge::Down,
        origin: None,
        consumed: true,
        announce: true,
        tick: std::time::Instant::now(),
    };
    let mut observed: Vec<super::ObservedWindow> = Vec::new();
    let outcome = super::workspace_migrate_one(
        &mut state,
        &me,
        &store,
        &tmp,
        &[BOUNDS],
        &areas,
        &mut observed,
        &[],
        "mon-a",
        &intent,
        1,
    );
    assert_eq!(outcome, super::scope_sends::MIGRATION_SHARED_REFUSAL);
    assert_eq!(state.workspaces.workspace_ids("mon-a"), ids_a_before);
    assert_eq!(state.workspaces.workspace_ids("mon-b"), ids_b_before);
    assert_eq!(format!("{:?}", state.engine), engine_before);
    let _ = std::fs::remove_dir_all(&tmp);
    super::clear_pending_seams();
}

#[test]
fn global_migration_validates_moved_previous_only() {
    super::clear_pending_seams();
    let mut state = two_output_state(WorkspaceMode::GlobalUnique);
    // Record a previous on each output: mon-a ws-1, mon-b ws-3.
    for (output, first, second) in [("mon-a", "ws-1", "ws-2"), ("mon-b", "ws-3", "ws-4")] {
        {
            let (owner, spaces) = (&mut state.scope_owner, &mut state.workspaces);
            owner.record_completed(spaces, output, first);
        }
        assert!(state.workspaces.activate(output, second));
        {
            let (owner, spaces) = (&mut state.scope_owner, &mut state.workspaces);
            assert!(owner.record_completed(spaces, output, second));
        }
    }
    assert_eq!(
        state.scope_owner.history().previous("mon-a").as_deref(),
        Some("ws-1")
    );
    // Migrate mon-a's active ws-1 to mon-b through the real primitive, then
    // run the scoped history hook: the moved previous clears, the sibling
    // entry survives.
    assert!(state.workspaces.activate("mon-a", "ws-1"));
    let outcome = state
        .workspaces
        .migrate_workspace_excluding(
            "mon-a",
            "mon-b",
            tiler_core::output_selection::MigrationSourceRefill::LastRemaining,
            &std::collections::BTreeSet::new(),
        )
        .expect("migrate");
    assert_eq!(outcome.moved_id, "ws-1");
    super::scope_sends::note_migration_history(&mut state, "mon-a", "ws-1");
    assert_eq!(state.scope_owner.history().previous("mon-a"), None);
    assert_eq!(
        state.scope_owner.history().previous("mon-b").as_deref(),
        Some("ws-3")
    );
    // A previous that never named the moved id is untouched.
    super::scope_sends::note_migration_history(&mut state, "mon-b", "ws-1");
    assert_eq!(
        state.scope_owner.history().previous("mon-b").as_deref(),
        Some("ws-3")
    );
    super::clear_pending_seams();
}

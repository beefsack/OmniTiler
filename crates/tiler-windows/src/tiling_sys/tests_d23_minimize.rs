// D23 Windows adapter boundary through a real `TileLoop` against the real
// retained Engine (no HWNDs, hooks, or live effects): minimized windows
// omit their Engine rows so convergence releases the leaf, reflows
// survivors, resolves MRU-or-none focus, and stores the `MinimizedSlot`;
// native restores re-carry purely tiled for best-effort rejoin with no
// focus write; verified closes ride `closed_windows` until acceptance.
// Native reveal (`model.rs` show lane), live minimize journeys, and
// ledger-backed hidden-retirement stays user-owned acceptance.
use super::rmax03_adapter_tests::test_state;
use super::{
    HintCx, MonitorArea, ObservedWindow, RetainedRow, assemble_domain_rows, audit_hidden_claims,
    clear_pending_seams, dispatch_workspace_release, ensure_workspace_assignments,
    fanout_pending_closed, handle_engine_event, inject_pending_focus, inject_pending_iconic,
    inject_pending_observe, inject_pending_recovery, inject_pending_tag, inject_pending_unreadable,
    pending_counters, workspace_close_cleanup, writable_tokens,
};

fn key(hwnd: u64) -> crate::workspace::WindowKey {
    crate::workspace::WindowKey {
        hwnd,
        pid: 1000 + hwnd as u32,
        creation: format!("c{hwnd:016x}"),
    }
}

fn bounds() -> tiler_core::geometry::Rect {
    tiler_core::geometry::Rect {
        x: 0,
        y: 0,
        w: 1920,
        h: 1040,
    }
}

fn mk_observed(hwnd: u64, token: &str, x: i32, y: i32) -> ObservedWindow {
    let rect = tiler_core::geometry::Rect {
        x,
        y,
        w: 300,
        h: 200,
    };
    ObservedWindow {
        hwnd,
        token: token.to_owned(),
        outer: rect,
        visible: rect,
        insets: crate::tiling::FrameInsets::default(),
        identity: crate::tiling::ObservedTarget {
            hwnd,
            pid: 1000 + hwnd as u32,
            process_creation: format!("c{hwnd:016x}"),
            exe_path: "app.exe".to_owned(),
            user_sid: "sid".to_owned(),
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

fn setup(
    output: &str,
) -> (
    super::TileLoop,
    String,
    (
        tiler_core::session::OutputDomain,
        tiler_core::session::DomainKey,
    ),
) {
    let mut state = test_state();
    state.workspaces.ensure_output(output);
    let ws = state.workspaces.active_id(output).expect("active");
    let domain = crate::workspace_owner::workspace_domain(output, &ws, bounds(), 8);
    (state, ws, domain)
}

/// Adapter membership wiring for one tiled member (workspace assignment,
/// stable token, frozen tile slot, lifetime tag).
fn wire_member(
    state: &mut super::TileLoop,
    output: &str,
    ws: &str,
    hwnd: u64,
    token: &str,
    rect: tiler_core::geometry::Rect,
) {
    let member = key(hwnd);
    assert!(state.workspaces.assign(member.clone(), output, ws, false));
    state.member_tokens.insert(member.clone(), token.to_owned());
    state.member_rects.insert(token.to_owned(), rect);
    state.member_tags.insert(member, format!("tag-{token}"));
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

/// Hide one wired member with a real ledger claim (durable input, exactly
/// what `workspace_hide_one` commits): full-identity claim with the
/// hide-time show preimage plus the workspace hidden flag. `iconic`
/// records minimization at hide time and drives D23 omission; it is the
/// only minimize signal for windows the raw observation gate skips.
/// A flag-only hidden member (no claim) is NOT production state and is
/// never constructed here.
fn hide_member(state: &mut super::TileLoop, hwnd: u64, claim_tag: &str, iconic: bool) {
    let member = key(hwnd);
    let claim = crate::model::WindowIdentity {
        hwnd,
        process: crate::model::ProcessIdentity {
            pid: 1000 + hwnd as u32,
            process_creation: format!("c{hwnd:016x}"),
            user_sid: "sid".to_owned(),
            session_id: 1,
            exe_path: "app.exe".to_owned(),
        },
        tag: claim_tag.to_owned(),
        kind: crate::model::WindowClaimKind::Product,
        show: crate::model::WindowShowState {
            minimized: iconic,
            maximized: false,
        },
    };
    state
        .hidden_claims
        .insert(member.clone(), super::HiddenRecord { claim, iconic });
    state.workspaces.set_hidden(&member, true);
}

fn tmp_store(name: &str) -> (crate::storage::LedgerStore, std::path::PathBuf) {
    let tmp = std::env::temp_dir().join(format!(
        "tiler-d23-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let store = crate::storage::LedgerStore::open(&tmp).expect("ledger temp dir");
    (store, tmp)
}

/// One production tick leg in `reconcile_tick` order (fixture ->
/// observe -> assign -> cleanup); callers assemble+commit per output
/// after, fanning out exactly like the tick loop. Admission itself
/// (native tag stamp) is the one non-coverable step, so membership is
/// pre-wired post-admission state and every downstream step is real.
fn tick_leg(
    state: &mut super::TileLoop,
    me: &crate::model::ProcessIdentity,
    fulls: &[tiler_core::geometry::Rect],
    areas: &[MonitorArea],
    observed_fixture: Vec<ObservedWindow>,
    iconic: Vec<u64>,
    unreadable: Vec<u64>,
) -> (Vec<ObservedWindow>, Vec<RetainedRow>) {
    inject_pending_observe(observed_fixture, Vec::new(), false);
    inject_pending_iconic(iconic);
    inject_pending_unreadable(unreadable);
    let mut skipped = Vec::new();
    let mut retained = Vec::new();
    let mut observed = state
        .observe(me, fulls, &mut skipped, &mut retained)
        .expect("injected enumeration succeeds");
    ensure_workspace_assignments(state, me, &mut observed, &retained, areas);
    workspace_close_cleanup(state);
    (observed, retained)
}

/// Script live lifetime tags for wired members (`wire_member` stamps
/// `tag-{token}`); pass `None` tokens for destroyed windows.
fn live_tags(members: &[(u64, Option<&str>)]) {
    for (hwnd, token) in members {
        inject_pending_tag(*hwnd, token.map(|token| format!("tag-{token}")));
    }
}

/// Single-output observation topology for `tick_leg`.
fn solo_topology(output: &str) -> (Vec<tiler_core::geometry::Rect>, Vec<MonitorArea>) {
    (
        vec![bounds()],
        vec![MonitorArea {
            device: output.to_owned(),
            work: bounds(),
            full: bounds(),
        }],
    )
}

fn revision_of(state: &super::TileLoop, domain_key: &tiler_core::session::DomainKey) -> u64 {
    state
        .engine
        .session(domain_key)
        .map(|session| session.accepted_revision())
        .unwrap_or(0)
}

/// Full production dispatch: assemble rows from adapter state, then route
/// the valid domain observation through `handle_engine_event` (stamp +
/// drain included). Panics on incomplete assembly: callers assert
/// deferral (`None`) explicitly instead.
#[allow(clippy::too_many_arguments)]
fn dispatch(
    state: &mut super::TileLoop,
    output: &str,
    ws: &str,
    domain: &(
        tiler_core::session::OutputDomain,
        tiler_core::session::DomainKey,
    ),
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
    focused: Option<&str>,
    correlation: &str,
) -> tiler_core::boundary::CoreReply {
    let carried = assemble_carried(state, output, ws, observed, retained, correlation);
    commit_rows(
        state,
        domain,
        &carried,
        focused,
        correlation,
        tiler_core::boundary::CoreCommand::Reconcile,
    )
}

/// Assemble one domain's carried observation rows (panics when the
/// observation is incomplete; deferral legs call `assemble_domain_rows`
/// directly instead).
fn assemble_carried(
    state: &mut super::TileLoop,
    output: &str,
    ws: &str,
    observed: &[ObservedWindow],
    retained: &[RetainedRow],
    correlation: &str,
) -> Vec<(
    tiler_core::directional::WindowId,
    tiler_core::geometry::Rect,
    tiler_core::size_hints::WindowSizeHints,
    bool,
)> {
    use tiler_core::directional::WindowId;
    let mut hint_cx = HintCx::new();
    let rows = assemble_domain_rows(
        state,
        output,
        ws,
        observed,
        retained,
        "reconcile",
        correlation,
        &mut hint_cx,
    )
    .expect("complete observation");
    rows.iter()
        .map(|row| {
            (
                WindowId(row.token.clone()),
                row.rect,
                row.hints,
                row.floating,
            )
        })
        .collect()
}

/// Commit one assembled observation through the production choke
/// (`handle_engine_event`: fixed-admission stamp, close stamp, Engine
/// handle, close drain) under any command: reconcile-shaped rows drive
/// Resize/Move/UpdateGaps legs exactly as their production builders do.
fn commit_rows(
    state: &mut super::TileLoop,
    domain: &(
        tiler_core::session::OutputDomain,
        tiler_core::session::DomainKey,
    ),
    carried: &[(
        tiler_core::directional::WindowId,
        tiler_core::geometry::Rect,
        tiler_core::size_hints::WindowSizeHints,
        bool,
    )],
    focused: Option<&str>,
    correlation: &str,
    command: tiler_core::boundary::CoreCommand,
) -> tiler_core::boundary::CoreReply {
    use tiler_core::directional::WindowId;
    use tiler_core::ids::CorrelationId;
    let fp = crate::tiling::fingerprint(
        &carried
            .iter()
            .map(|(token, rect, _, _)| (token.0.clone(), *rect))
            .collect::<Vec<_>>(),
    );
    let correlation = CorrelationId::parse(correlation).expect("correlation");
    let mut event = crate::tiling::build_reconcile_event_for_floating(
        &state.owner,
        &state.generation,
        &correlation,
        revision_of(state, &domain.1),
        fp,
        &domain.0,
        &domain.1,
        8,
        carried,
        focused.map(|token| WindowId(token.to_owned())).as_ref(),
    );
    event.command = command;
    handle_engine_event(state, &mut event)
}

fn geometry_x(
    reply: &tiler_core::boundary::CoreReply,
) -> std::collections::HashMap<String, tiler_core::geometry::Rect> {
    use tiler_core::boundary::CoreReply;
    match reply {
        CoreReply::Projection(plan) => plan
            .geometry
            .iter()
            .map(|entry| (entry.window.0.clone(), entry.rect))
            .collect(),
        CoreReply::Tiled(plan) => plan
            .geometry
            .iter()
            .map(|entry| (entry.window.0.clone(), entry.rect))
            .collect(),
        other => panic!("expected geometry plan, got {other:?}"),
    }
}

fn focused_token(
    state: &super::TileLoop,
    domain_key: &tiler_core::session::DomainKey,
) -> Option<String> {
    let session = state.engine.session(domain_key)?;
    let (domain, leaf) = (session.focus().0?, session.focus().1?);
    if &domain != domain_key {
        return None;
    }
    session
        .snapshot()
        .windows
        .iter()
        .find(|link| link.leaf == leaf)
        .map(|link| link.window.0.clone())
}

/// Tree leaf order for one domain, mirroring the portable
/// `minimize_restore` assertions: proves best-effort rejoin beside the
/// former neighbor (not fresh admission) independent of pixel geometry.
fn leaf_window_order(
    state: &super::TileLoop,
    domain_key: &tiler_core::session::DomainKey,
) -> Vec<String> {
    use tiler_core::directional::{Node, NodeId};
    fn collect(node: &Node, out: &mut Vec<NodeId>) {
        match node {
            Node::Leaf { id } => out.push(id.clone()),
            Node::Group { children, .. } => {
                for child in children {
                    collect(child, out);
                }
            }
        }
    }
    let session = state.engine.session(domain_key).expect("session");
    let snapshot = session.snapshot();
    let tree = snapshot
        .domains
        .iter()
        .find(|view| view.output == domain_key.output && view.workspace == domain_key.workspace)
        .expect("domain")
        .tree
        .clone()
        .expect("tree");
    let mut leaves = Vec::new();
    collect(&tree, &mut leaves);
    leaves
        .into_iter()
        .map(|leaf| {
            snapshot
                .windows
                .iter()
                .find(|link| link.leaf == leaf)
                .unwrap_or_else(|| panic!("leaf {leaf:?} linked"))
                .window
                .0
                .clone()
        })
        .collect()
}

fn slot_present(
    state: &super::TileLoop,
    domain_key: &tiler_core::session::DomainKey,
    token: &str,
) -> bool {
    use tiler_core::directional::WindowId;
    state.engine.session(domain_key).is_some_and(|session| {
        session
            .minimized_slot(&WindowId(token.to_owned()))
            .is_some()
    })
}

/// Seed three tiled members A/B/C with MRU focus on C, all through the
/// production dispatch (stamp + drain active, empty queue).
fn seed_abc(
    state: &mut super::TileLoop,
    output: &str,
    ws: &str,
    domain: &(
        tiler_core::session::OutputDomain,
        tiler_core::session::DomainKey,
    ),
) -> (Vec<ObservedWindow>, tiler_core::geometry::Rect) {
    let rect = bounds();
    for (hwnd, token) in [(101, "wA"), (102, "wB"), (103, "wC")] {
        wire_member(state, output, ws, hwnd, token, rect);
    }
    let observed = vec![
        mk_observed(101, "wA", 0, 0),
        mk_observed(102, "wB", 0, 0),
        mk_observed(103, "wC", 0, 0),
    ];
    let reply = dispatch(
        state,
        output,
        ws,
        domain,
        &observed,
        &[],
        Some("wC"),
        "d23-seed",
    );
    assert!(
        matches!(
            reply,
            tiler_core::boundary::CoreReply::Projection(_)
                | tiler_core::boundary::CoreReply::Tiled(_)
        ),
        "seed must converge, got {reply:?}"
    );
    assert_eq!(focused_token(state, &domain.1).as_deref(), Some("wC"));
    (observed, rect)
}

#[test]
fn minimize_omits_releases_reflows_mru_focus() {
    clear_pending_seams();
    let (mut state, ws, domain) = setup("mon-1");
    let output = "mon-1".to_owned();
    let _ = seed_abc(&mut state, &output, &ws, &domain);
    // Native minimize of B: the iconic fixture derives the omission
    // through the real observe path, so no Engine row is carried.
    let me = me();
    let (fulls, areas) = solo_topology(&output);
    live_tags(&[(101, Some("wA")), (102, Some("wB")), (103, Some("wC"))]);
    let (observed, retained) = tick_leg(
        &mut state,
        &me,
        &fulls,
        &areas,
        vec![mk_observed(101, "wA", 0, 0), mk_observed(103, "wC", 0, 0)],
        vec![102],
        Vec::new(),
    );
    assert_eq!(observed.len(), 2, "B omits its observation, survivors stay");
    let mut hint_cx = HintCx::new();
    let rows = assemble_domain_rows(
        &mut state,
        &output,
        &ws,
        &observed,
        &retained,
        "reconcile",
        "d23-minimize",
        &mut hint_cx,
    )
    .expect("minimized omission stays complete");
    assert_eq!(rows.len(), 2, "B omits its row, survivors stay");
    assert!(rows.iter().all(|row| row.token != "wB"));
    let reply = dispatch(
        &mut state,
        &output,
        &ws,
        &domain,
        &observed,
        &retained,
        Some("wC"),
        "d23-minimize",
    );
    let xs = geometry_x(&reply);
    assert_eq!(xs.len(), 2, "survivors reflow without B");
    assert!(slot_present(&state, &domain.1, "wB"), "B stores its slot");
    assert_eq!(
        focused_token(&state, &domain.1).as_deref(),
        Some("wC"),
        "MRU focus survives the minimize"
    );
    clear_pending_seams();
}

#[test]
fn sole_minimize_retains_session_with_focus_none() {
    clear_pending_seams();
    let (mut state, ws, domain) = setup("mon-1");
    let output = "mon-1".to_owned();
    let rect = bounds();
    wire_member(&mut state, &output, &ws, 101, "wA", rect);
    let observed = vec![mk_observed(101, "wA", 0, 0)];
    let reply = dispatch(
        &mut state,
        &output,
        &ws,
        &domain,
        &observed,
        &[],
        Some("wA"),
        "d23-seed",
    );
    assert!(
        matches!(
            reply,
            tiler_core::boundary::CoreReply::Projection(_)
                | tiler_core::boundary::CoreReply::Tiled(_)
        ),
        "seed must converge"
    );
    // Sole minimize: the carried observation is empty with focus none,
    // yet the session (and its slot) survives.
    let me = me();
    let (fulls, areas) = solo_topology(&output);
    live_tags(&[(101, Some("wA"))]);
    let (empty, retained) = tick_leg(
        &mut state,
        &me,
        &fulls,
        &areas,
        Vec::new(),
        vec![101],
        Vec::new(),
    );
    assert!(empty.is_empty());
    let reply = dispatch(
        &mut state,
        &output,
        &ws,
        &domain,
        &empty,
        &retained,
        None,
        "d23-sole-min",
    );
    let xs = geometry_x(&reply);
    assert!(xs.is_empty(), "no tiles while sole-minimized");
    assert!(
        state.engine.session(&domain.1).is_some(),
        "sole-minimized session is retained, never retired"
    );
    assert!(slot_present(&state, &domain.1, "wA"));
    assert_eq!(focused_token(&state, &domain.1), None, "sole focus is none");
    assert!(
        !tiler_core::engine::committed_session_is_empty(
            state.engine.session(&domain.1).expect("session")
        ),
        "slot-aware emptiness keeps the managed workspace"
    );
    clear_pending_seams();
}

#[test]
fn restore_rejoins_between_former_neighbors_without_focus_write() {
    clear_pending_seams();
    let (mut state, ws, domain) = setup("mon-1");
    let output = "mon-1".to_owned();
    let _ = seed_abc(&mut state, &output, &ws, &domain);
    let me = me();
    let (fulls, areas) = solo_topology(&output);
    live_tags(&[(101, Some("wA")), (102, Some("wB")), (103, Some("wC"))]);
    let (minimized, retained) = tick_leg(
        &mut state,
        &me,
        &fulls,
        &areas,
        vec![mk_observed(101, "wA", 0, 0), mk_observed(103, "wC", 0, 0)],
        vec![102],
        Vec::new(),
    );
    let _ = dispatch(
        &mut state,
        &output,
        &ws,
        &domain,
        &minimized,
        &retained,
        Some("wC"),
        "d23-min",
    );
    assert!(slot_present(&state, &domain.1, "wB"));
    // Native nonactivating restore: host focus stays on C while B
    // re-carries purely tiled.
    let (restored, retained) = tick_leg(
        &mut state,
        &me,
        &fulls,
        &areas,
        vec![
            mk_observed(101, "wA", 0, 0),
            mk_observed(102, "wB", 0, 0),
            mk_observed(103, "wC", 0, 0),
        ],
        Vec::new(),
        Vec::new(),
    );
    let reply = dispatch(
        &mut state,
        &output,
        &ws,
        &domain,
        &restored,
        &retained,
        Some("wC"),
        "d23-restore",
    );
    let xs = geometry_x(&reply);
    assert_eq!(xs.len(), 3);
    assert_eq!(
        leaf_window_order(&state, &domain.1),
        vec!["wA".to_owned(), "wB".to_owned(), "wC".to_owned()],
        "B rejoins between its former neighbors, never fresh"
    );
    assert!(
        !slot_present(&state, &domain.1, "wB"),
        "rejoin consumes the slot"
    );
    assert_eq!(
        focused_token(&state, &domain.1).as_deref(),
        Some("wC"),
        "pre-restore MRU focus stays: no focus write on restore"
    );
    clear_pending_seams();
}

#[test]
fn restore_naming_restored_window_keeps_mru_without_setter() {
    // Host natively focuses the restored window (observed focus names
    // just-restored B): the Engine keeps pre-restore MRU focus C, and
    // the adapter issues no focus setter on this path. The focus stub
    // is armed so any future `actuate_focus` call on the restore path
    // fails this test via the counter.
    clear_pending_seams();
    let (mut state, ws, domain) = setup("mon-1");
    let output = "mon-1".to_owned();
    let _ = seed_abc(&mut state, &output, &ws, &domain);
    let me = me();
    let (fulls, areas) = solo_topology(&output);
    live_tags(&[(101, Some("wA")), (102, Some("wB")), (103, Some("wC"))]);
    let (minimized, retained) = tick_leg(
        &mut state,
        &me,
        &fulls,
        &areas,
        vec![mk_observed(101, "wA", 0, 0), mk_observed(103, "wC", 0, 0)],
        vec![102],
        Vec::new(),
    );
    let _ = dispatch(
        &mut state,
        &output,
        &ws,
        &domain,
        &minimized,
        &retained,
        Some("wC"),
        "d23-min",
    );
    assert!(slot_present(&state, &domain.1, "wB"));
    inject_pending_focus(Some("focus-ok"));
    let (restored, retained) = tick_leg(
        &mut state,
        &me,
        &fulls,
        &areas,
        vec![
            mk_observed(101, "wA", 0, 0),
            mk_observed(102, "wB", 0, 0),
            mk_observed(103, "wC", 0, 0),
        ],
        Vec::new(),
        Vec::new(),
    );
    let reply = dispatch(
        &mut state,
        &output,
        &ws,
        &domain,
        &restored,
        &retained,
        Some("wB"),
        "d23-restore",
    );
    assert!(
        matches!(
            reply,
            tiler_core::boundary::CoreReply::Projection(_)
                | tiler_core::boundary::CoreReply::Tiled(_)
        ),
        "restore must converge, got {reply:?}"
    );
    assert_eq!(
        leaf_window_order(&state, &domain.1),
        vec!["wA".to_owned(), "wB".to_owned(), "wC".to_owned()],
        "B rejoins even when the host focused it"
    );
    assert!(
        !slot_present(&state, &domain.1, "wB"),
        "rejoin consumes the slot"
    );
    assert_eq!(
        focused_token(&state, &domain.1).as_deref(),
        Some("wC"),
        "observed focus naming just-restored B never takes MRU focus"
    );
    assert_eq!(
        pending_counters().focus,
        0,
        "restore path issues no focus setter"
    );
    clear_pending_seams();
}

#[test]
fn restore_survives_committed_resize_op() {
    // A real shared `Resize` op through the adapter event path while B
    // is minimized: the committed share change must not alone invalidate
    // restore context; B rejoins between its former neighbors.
    clear_pending_seams();
    let (mut state, ws, domain) = setup("mon-1");
    let output = "mon-1".to_owned();
    let _ = seed_abc(&mut state, &output, &ws, &domain);
    let me = me();
    let (fulls, areas) = solo_topology(&output);
    live_tags(&[(101, Some("wA")), (102, Some("wB")), (103, Some("wC"))]);
    let (minimized, retained) = tick_leg(
        &mut state,
        &me,
        &fulls,
        &areas,
        vec![mk_observed(101, "wA", 0, 0), mk_observed(103, "wC", 0, 0)],
        vec![102],
        Vec::new(),
    );
    let _ = dispatch(
        &mut state,
        &output,
        &ws,
        &domain,
        &minimized,
        &retained,
        Some("wC"),
        "d23-min",
    );
    assert!(slot_present(&state, &domain.1, "wB"));
    let carried = assemble_carried(
        &mut state,
        &output,
        &ws,
        &minimized,
        &retained,
        "d23-leg-resize",
    );
    let reply = commit_rows(
        &mut state,
        &domain,
        &carried,
        Some("wC"),
        "d23-leg-resize",
        tiler_core::boundary::CoreCommand::Resize {
            window: "wC".to_owned(),
            direction: "left".to_owned(),
            mode: "outwards".to_owned(),
            press_index: 0,
        },
    );
    assert!(
        matches!(reply, tiler_core::boundary::CoreReply::Resize(_)),
        "resize op must plan, got {reply:?}"
    );
    assert!(
        slot_present(&state, &domain.1, "wB"),
        "resize keeps the slot"
    );
    let (restored, retained) = tick_leg(
        &mut state,
        &me,
        &fulls,
        &areas,
        vec![
            mk_observed(101, "wA", 0, 0),
            mk_observed(102, "wB", 0, 0),
            mk_observed(103, "wC", 0, 0),
        ],
        Vec::new(),
        Vec::new(),
    );
    let _ = dispatch(
        &mut state,
        &output,
        &ws,
        &domain,
        &restored,
        &retained,
        Some("wC"),
        "d23-restore",
    );
    assert_eq!(
        leaf_window_order(&state, &domain.1),
        vec!["wA".to_owned(), "wB".to_owned(), "wC".to_owned()],
        "B rejoins between its former neighbors after a committed resize"
    );
    assert_eq!(
        focused_token(&state, &domain.1).as_deref(),
        Some("wC"),
        "no focus write on restore"
    );
    clear_pending_seams();
}

#[test]
fn restore_survives_same_axis_swap_op() {
    // A real same-axis `Move` swap of the survivors through the adapter
    // event path while B is minimized, mirroring the portable swap leg:
    // the order change alone must not invalidate; B rejoins beside
    // former neighbor C on its old side.
    clear_pending_seams();
    let (mut state, ws, domain) = setup("mon-1");
    let output = "mon-1".to_owned();
    let _ = seed_abc(&mut state, &output, &ws, &domain);
    let me = me();
    let (fulls, areas) = solo_topology(&output);
    live_tags(&[(101, Some("wA")), (102, Some("wB")), (103, Some("wC"))]);
    let (minimized, retained) = tick_leg(
        &mut state,
        &me,
        &fulls,
        &areas,
        vec![mk_observed(101, "wA", 0, 0), mk_observed(103, "wC", 0, 0)],
        vec![102],
        Vec::new(),
    );
    let _ = dispatch(
        &mut state,
        &output,
        &ws,
        &domain,
        &minimized,
        &retained,
        Some("wC"),
        "d23-min",
    );
    let carried = assemble_carried(
        &mut state,
        &output,
        &ws,
        &minimized,
        &retained,
        "d23-leg-swap",
    );
    let reply = commit_rows(
        &mut state,
        &domain,
        &carried,
        Some("wC"),
        "d23-leg-swap",
        tiler_core::boundary::CoreCommand::Move {
            window: "wC".to_owned(),
            direction: "left".to_owned(),
            cross_output_transfer: false,
            same_axis_move: tiler_core::directional::SameAxisMove::SwapWithNeighbor,
        },
    );
    assert!(
        matches!(reply, tiler_core::boundary::CoreReply::MoveDirectional(_)),
        "swap move must plan, got {reply:?}"
    );
    assert_eq!(
        leaf_window_order(&state, &domain.1),
        vec!["wC".to_owned(), "wA".to_owned()],
        "swap leg precondition"
    );
    assert!(slot_present(&state, &domain.1, "wB"), "swap keeps the slot");
    let (restored, retained) = tick_leg(
        &mut state,
        &me,
        &fulls,
        &areas,
        vec![
            mk_observed(101, "wA", 0, 0),
            mk_observed(102, "wB", 0, 0),
            mk_observed(103, "wC", 0, 0),
        ],
        Vec::new(),
        Vec::new(),
    );
    let _ = dispatch(
        &mut state,
        &output,
        &ws,
        &domain,
        &restored,
        &retained,
        Some("wC"),
        "d23-restore",
    );
    assert_eq!(
        leaf_window_order(&state, &domain.1),
        vec!["wB".to_owned(), "wC".to_owned(), "wA".to_owned()],
        "B rejoins beside former neighbor C on its old side"
    );
    assert_eq!(
        focused_token(&state, &domain.1).as_deref(),
        Some("wC"),
        "no focus write on restore"
    );
    clear_pending_seams();
}

#[test]
fn restore_survives_newcomer_and_focus_change() {
    // A fresh admission plus an MRU focus change while B is minimized:
    // neither alone invalidates restore context (committed resize and
    // swap ops have their own tests above).
    clear_pending_seams();
    let (mut state, ws, domain) = setup("mon-1");
    let output = "mon-1".to_owned();
    let _ = seed_abc(&mut state, &output, &ws, &domain);
    let me = me();
    let (fulls, areas) = solo_topology(&output);
    live_tags(&[(101, Some("wA")), (102, Some("wB")), (103, Some("wC"))]);
    let (minimized, retained) = tick_leg(
        &mut state,
        &me,
        &fulls,
        &areas,
        vec![mk_observed(101, "wA", 0, 0), mk_observed(103, "wC", 0, 0)],
        vec![102],
        Vec::new(),
    );
    let _ = dispatch(
        &mut state,
        &output,
        &ws,
        &domain,
        &minimized,
        &retained,
        Some("wC"),
        "d23-min",
    );
    // Newcomer D admits fresh with host focus; B's slot survives.
    wire_member(&mut state, &output, &ws, 104, "wD", bounds());
    live_tags(&[(104, Some("wD"))]);
    let (admitted, retained) = tick_leg(
        &mut state,
        &me,
        &fulls,
        &areas,
        vec![
            mk_observed(101, "wA", 0, 0),
            mk_observed(103, "wC", 0, 0),
            mk_observed(104, "wD", 1300, 20),
        ],
        vec![102],
        Vec::new(),
    );
    let _ = dispatch(
        &mut state,
        &output,
        &ws,
        &domain,
        &admitted,
        &retained,
        Some("wD"),
        "d23-admit",
    );
    assert!(
        slot_present(&state, &domain.1, "wB"),
        "admission keeps the slot"
    );
    // Restore B with host focus on D: best-effort rejoin between the
    // former neighbors, never fresh beside D, host focus kept.
    let (restored, retained) = tick_leg(
        &mut state,
        &me,
        &fulls,
        &areas,
        vec![
            mk_observed(101, "wA", 0, 0),
            mk_observed(102, "wB", 0, 0),
            mk_observed(103, "wC", 0, 0),
            mk_observed(104, "wD", 1300, 20),
        ],
        Vec::new(),
        Vec::new(),
    );
    let reply = dispatch(
        &mut state,
        &output,
        &ws,
        &domain,
        &restored,
        &retained,
        Some("wD"),
        "d23-restore",
    );
    let xs = geometry_x(&reply);
    assert_eq!(xs.len(), 4);
    assert_eq!(
        leaf_window_order(&state, &domain.1),
        vec![
            "wA".to_owned(),
            "wB".to_owned(),
            "wC".to_owned(),
            "wD".to_owned()
        ],
        "B rejoins beside former neighbor C, never fresh beside newcomer D"
    );
    assert_eq!(
        focused_token(&state, &domain.1).as_deref(),
        Some("wD"),
        "host focus (D) is kept: no focus write on restore"
    );
    clear_pending_seams();
}

#[test]
fn restore_survives_sibling_close_and_other_minimize_roundtrip() {
    clear_pending_seams();
    let (mut state, ws, domain) = setup("mon-1");
    let output = "mon-1".to_owned();
    let _ = seed_abc(&mut state, &output, &ws, &domain);
    let me = me();
    let (fulls, areas) = solo_topology(&output);
    live_tags(&[(101, Some("wA")), (102, Some("wB")), (103, Some("wC"))]);
    let (minimized, retained) = tick_leg(
        &mut state,
        &me,
        &fulls,
        &areas,
        vec![mk_observed(101, "wA", 0, 0), mk_observed(103, "wC", 0, 0)],
        vec![102],
        Vec::new(),
    );
    let _ = dispatch(
        &mut state,
        &output,
        &ws,
        &domain,
        &minimized,
        &retained,
        Some("wC"),
        "d23-min",
    );
    // Sibling C closes: the production inventory drops its HWND with a
    // dead lifetime tag, so cleanup drops the member and queues the
    // verified close; the next leg forgets C's ordinary slot.
    live_tags(&[(101, Some("wA")), (102, Some("wB")), (103, None)]);
    let (survivor, retained) = tick_leg(
        &mut state,
        &me,
        &fulls,
        &areas,
        vec![mk_observed(101, "wA", 0, 0)],
        vec![102],
        Vec::new(),
    );
    assert!(
        state.pending_closed.contains_key("wC"),
        "sibling close queues"
    );
    assert!(
        !state.member_tokens.contains_key(&key(103)),
        "sibling member drops"
    );
    let _ = dispatch(
        &mut state,
        &output,
        &ws,
        &domain,
        &survivor,
        &retained,
        Some("wA"),
        "d23-sibclose",
    );
    assert!(
        !state.pending_closed.contains_key("wC"),
        "sibling close drains"
    );
    assert!(
        !slot_present(&state, &domain.1, "wC"),
        "sibling slot forgotten"
    );
    // Other-minimize roundtrip: A minimizes while B stays minimized; A
    // restores beside the survivor while B's slot coexists untouched.
    let (empty, retained) = tick_leg(
        &mut state,
        &me,
        &fulls,
        &areas,
        Vec::new(),
        vec![101, 102],
        Vec::new(),
    );
    assert!(empty.is_empty());
    let _ = dispatch(
        &mut state, &output, &ws, &domain, &empty, &retained, None, "d23-omin",
    );
    assert!(slot_present(&state, &domain.1, "wA"));
    assert!(slot_present(&state, &domain.1, "wB"));
    let (a_back, retained) = tick_leg(
        &mut state,
        &me,
        &fulls,
        &areas,
        vec![mk_observed(101, "wA", 0, 0)],
        vec![102],
        Vec::new(),
    );
    let _ = dispatch(
        &mut state,
        &output,
        &ws,
        &domain,
        &a_back,
        &retained,
        Some("wA"),
        "d23-orestore",
    );
    assert!(
        !slot_present(&state, &domain.1, "wA"),
        "A's roundtrip consumes its slot"
    );
    // Restore B beside its sole surviving former neighbor A.
    let (restored, retained) = tick_leg(
        &mut state,
        &me,
        &fulls,
        &areas,
        vec![mk_observed(101, "wA", 0, 0), mk_observed(102, "wB", 0, 0)],
        Vec::new(),
        Vec::new(),
    );
    let reply = dispatch(
        &mut state,
        &output,
        &ws,
        &domain,
        &restored,
        &retained,
        Some("wA"),
        "d23-restore",
    );
    let xs = geometry_x(&reply);
    assert_eq!(xs.len(), 2);
    assert_eq!(
        leaf_window_order(&state, &domain.1),
        vec!["wA".to_owned(), "wB".to_owned()],
        "B regroups beside surviving neighbor A on its old side"
    );
    assert_eq!(focused_token(&state, &domain.1).as_deref(), Some("wA"));
    clear_pending_seams();
}

#[test]
fn restore_survives_gap_and_resolution_change() {
    // A gap/work-area leg converges the same omission through the
    // topology-preserving path, then the work area itself shrinks
    // (resolution change): neither alone invalidates restore context.
    clear_pending_seams();
    let (mut state, ws, domain) = setup("mon-1");
    let output = "mon-1".to_owned();
    let _ = seed_abc(&mut state, &output, &ws, &domain);
    let me = me();
    let (fulls, areas) = solo_topology(&output);
    live_tags(&[(101, Some("wA")), (102, Some("wB")), (103, Some("wC"))]);
    let (minimized, retained) = tick_leg(
        &mut state,
        &me,
        &fulls,
        &areas,
        vec![mk_observed(101, "wA", 0, 0), mk_observed(103, "wC", 0, 0)],
        vec![102],
        Vec::new(),
    );
    let _ = dispatch(
        &mut state,
        &output,
        &ws,
        &domain,
        &minimized,
        &retained,
        Some("wC"),
        "d23-min",
    );
    let carried = assemble_carried(&mut state, &output, &ws, &minimized, &retained, "d23-gaps");
    let reply = commit_rows(
        &mut state,
        &domain,
        &carried,
        Some("wC"),
        "d23-gaps",
        tiler_core::boundary::CoreCommand::UpdateGaps,
    );
    assert!(
        matches!(
            reply,
            tiler_core::boundary::CoreReply::Projection(_)
                | tiler_core::boundary::CoreReply::Tiled(_)
        ),
        "gap leg must converge, got {reply:?}"
    );
    assert!(
        slot_present(&state, &domain.1, "wB"),
        "gap leg keeps the slot"
    );
    // Resolution change: the same domain reprojects smaller before the
    // restore converges against it.
    let small = tiler_core::geometry::Rect {
        x: 0,
        y: 0,
        w: 1600,
        h: 900,
    };
    let domain_small = crate::workspace_owner::workspace_domain(&output, &ws, small, 8);
    let (restored, retained) = tick_leg(
        &mut state,
        &me,
        &fulls,
        &areas,
        vec![
            mk_observed(101, "wA", 0, 0),
            mk_observed(102, "wB", 0, 0),
            mk_observed(103, "wC", 0, 0),
        ],
        Vec::new(),
        Vec::new(),
    );
    let reply = dispatch(
        &mut state,
        &output,
        &ws,
        &domain_small,
        &restored,
        &retained,
        Some("wC"),
        "d23-restore",
    );
    assert!(
        matches!(
            reply,
            tiler_core::boundary::CoreReply::Projection(_)
                | tiler_core::boundary::CoreReply::Tiled(_)
        ),
        "restore must converge, got {reply:?}"
    );
    assert_eq!(
        leaf_window_order(&state, &domain.1),
        vec!["wA".to_owned(), "wB".to_owned(), "wC".to_owned()],
        "gap change rejoins the old neighborhood"
    );
    clear_pending_seams();
}

#[test]
fn restart_discards_slots_for_fresh_admission() {
    // Restart is a fresh process: no slots survive, so the same tokens
    // admit as brand-new tiles (session-only memory, never persisted).
    clear_pending_seams();
    let (mut state, ws, domain) = setup("mon-1");
    let output = "mon-1".to_owned();
    let _ = seed_abc(&mut state, &output, &ws, &domain);
    let me = me();
    let (fulls, areas) = solo_topology(&output);
    live_tags(&[(101, Some("wA")), (102, Some("wB")), (103, Some("wC"))]);
    let (minimized, retained) = tick_leg(
        &mut state,
        &me,
        &fulls,
        &areas,
        vec![mk_observed(101, "wA", 0, 0), mk_observed(103, "wC", 0, 0)],
        vec![102],
        Vec::new(),
    );
    let _ = dispatch(
        &mut state,
        &output,
        &ws,
        &domain,
        &minimized,
        &retained,
        Some("wC"),
        "d23-min",
    );
    assert!(slot_present(&state, &domain.1, "wB"));
    let (mut fresh, ws2, domain2) = setup("mon-1");
    assert_eq!(ws2, ws);
    let rect = bounds();
    for (hwnd, token) in [(101, "wA"), (102, "wB"), (103, "wC")] {
        wire_member(&mut fresh, &output, &ws2, hwnd, token, rect);
    }
    let observed = vec![
        mk_observed(101, "wA", 0, 0),
        mk_observed(102, "wB", 0, 0),
        mk_observed(103, "wC", 0, 0),
    ];
    let reply = dispatch(
        &mut fresh,
        &output,
        &ws2,
        &domain2,
        &observed,
        &[],
        Some("wC"),
        "d23-restart",
    );
    let xs = geometry_x(&reply);
    assert_eq!(xs.len(), 3, "all three admit fresh after restart");
    assert!(
        !slot_present(&fresh, &domain2.1, "wB"),
        "restart carries no restore slot"
    );
    assert_eq!(focused_token(&fresh, &domain2.1).as_deref(), Some("wC"));
    clear_pending_seams();
}

#[test]
fn fallback_admits_fresh_when_no_former_neighbor_survives() {
    clear_pending_seams();
    let (mut state, ws, domain) = setup("mon-1");
    let output = "mon-1".to_owned();
    let _ = seed_abc(&mut state, &output, &ws, &domain);
    let me = me();
    let (fulls, areas) = solo_topology(&output);
    live_tags(&[(101, Some("wA")), (102, Some("wB")), (103, Some("wC"))]);
    let (minimized, retained) = tick_leg(
        &mut state,
        &me,
        &fulls,
        &areas,
        vec![mk_observed(101, "wA", 0, 0), mk_observed(103, "wC", 0, 0)],
        vec![102],
        Vec::new(),
    );
    let _ = dispatch(
        &mut state,
        &output,
        &ws,
        &domain,
        &minimized,
        &retained,
        Some("wC"),
        "d23-min",
    );
    // Both former neighbors close (verified closes drain their ordinary
    // slots in the same observation); B's slot outlives them.
    live_tags(&[(101, None), (102, Some("wB")), (103, None)]);
    let (empty, retained) = tick_leg(
        &mut state,
        &me,
        &fulls,
        &areas,
        Vec::new(),
        vec![102],
        Vec::new(),
    );
    assert!(empty.is_empty());
    assert_eq!(state.pending_closed.len(), 2, "both closes queue");
    let reply = dispatch(
        &mut state,
        &output,
        &ws,
        &domain,
        &empty,
        &retained,
        None,
        "d23-closeac",
    );
    assert!(
        matches!(
            reply,
            tiler_core::boundary::CoreReply::Projection(_)
                | tiler_core::boundary::CoreReply::Tiled(_)
        ),
        "close leg must converge, got {reply:?}"
    );
    assert!(
        state.pending_closed.is_empty(),
        "close leg drains on acceptance"
    );
    assert!(!slot_present(&state, &domain.1, "wA"));
    assert!(!slot_present(&state, &domain.1, "wC"));
    assert!(slot_present(&state, &domain.1, "wB"), "B's slot survives");
    // Restore B with no live former neighbor: fresh ordinary admission
    // as the sole tile, never the old slot position.
    let (restored, retained) = tick_leg(
        &mut state,
        &me,
        &fulls,
        &areas,
        vec![mk_observed(102, "wB", 0, 0)],
        Vec::new(),
        Vec::new(),
    );
    let reply = dispatch(
        &mut state,
        &output,
        &ws,
        &domain,
        &restored,
        &retained,
        Some("wB"),
        "d23-fallback",
    );
    let xs = geometry_x(&reply);
    assert_eq!(xs.len(), 1, "B admits fresh as the sole tile");
    assert!(xs.contains_key("wB"));
    assert!(
        !slot_present(&state, &domain.1, "wB"),
        "fallback consumes the slot"
    );
    assert_eq!(focused_token(&state, &domain.1).as_deref(), Some("wB"));
    clear_pending_seams();
}

#[test]
fn minimized_close_reports_and_forgets_slot_until_acceptance() {
    clear_pending_seams();
    let (mut state, ws, domain) = setup("mon-1");
    let output = "mon-1".to_owned();
    let _ = seed_abc(&mut state, &output, &ws, &domain);
    let me = me();
    let (fulls, areas) = solo_topology(&output);
    live_tags(&[(101, Some("wA")), (102, Some("wB")), (103, Some("wC"))]);
    let (minimized, retained) = tick_leg(
        &mut state,
        &me,
        &fulls,
        &areas,
        vec![mk_observed(101, "wA", 0, 0), mk_observed(103, "wC", 0, 0)],
        vec![102],
        Vec::new(),
    );
    let _ = dispatch(
        &mut state,
        &output,
        &ws,
        &domain,
        &minimized,
        &retained,
        Some("wC"),
        "d23-min",
    );
    assert!(slot_present(&state, &domain.1, "wB"));
    // B still iconic, then destroyed: the production inventory drops its
    // HWND with a dead lifetime tag, so the close verifies.
    live_tags(&[(101, Some("wA")), (102, None), (103, Some("wC"))]);
    let (survivors, retained) = tick_leg(
        &mut state,
        &me,
        &fulls,
        &areas,
        vec![mk_observed(101, "wA", 0, 0), mk_observed(103, "wC", 0, 0)],
        Vec::new(),
        Vec::new(),
    );
    assert_eq!(
        state.pending_closed.len(),
        1,
        "only the verified minimized close queues"
    );
    assert!(state.pending_closed.contains_key("wB"));
    assert!(
        !state.member_tokens.contains_key(&key(102)),
        "member state still drops"
    );
    // Survivors converge with the close report: the slot is forgotten
    // (never re-minted) and the report drains on acceptance.
    let reply = dispatch(
        &mut state,
        &output,
        &ws,
        &domain,
        &survivors,
        &retained,
        Some("wC"),
        "d23-close",
    );
    let xs = geometry_x(&reply);
    assert_eq!(xs.len(), 2);
    assert!(
        !slot_present(&state, &domain.1, "wB"),
        "reported close forgets the slot"
    );
    assert!(state.pending_closed.is_empty(), "delivered close drains");
    assert_eq!(focused_token(&state, &domain.1).as_deref(), Some("wC"));
    // A later same-token window is a fresh admission, never the old slot.
    wire_member(&mut state, &output, &ws, 102, "wB", bounds());
    live_tags(&[(102, Some("wB"))]);
    let (readmit, retained) = tick_leg(
        &mut state,
        &me,
        &fulls,
        &areas,
        vec![
            mk_observed(101, "wA", 0, 0),
            mk_observed(102, "wB", 0, 0),
            mk_observed(103, "wC", 0, 0),
        ],
        Vec::new(),
        Vec::new(),
    );
    let reply = dispatch(
        &mut state,
        &output,
        &ws,
        &domain,
        &readmit,
        &retained,
        Some("wC"),
        "d23-readmit",
    );
    let xs = geometry_x(&reply);
    assert_eq!(xs.len(), 3, "same token re-admits fresh");
    assert!(
        !slot_present(&state, &domain.1, "wB"),
        "no slot lingers for re-admission"
    );
    clear_pending_seams();
}

#[test]
fn sole_minimize_close_delivers_empty_cleanup_and_retires() {
    clear_pending_seams();
    let (mut state, ws, domain) = setup("mon-1");
    let output = "mon-1".to_owned();
    let rect = bounds();
    wire_member(&mut state, &output, &ws, 101, "wA", rect);
    let observed = vec![mk_observed(101, "wA", 0, 0)];
    let _ = dispatch(
        &mut state,
        &output,
        &ws,
        &domain,
        &observed,
        &[],
        Some("wA"),
        "d23-seed",
    );
    let me = me();
    let (fulls, areas) = solo_topology(&output);
    live_tags(&[(101, Some("wA"))]);
    let (empty, retained) = tick_leg(
        &mut state,
        &me,
        &fulls,
        &areas,
        Vec::new(),
        vec![101],
        Vec::new(),
    );
    assert!(empty.is_empty());
    let _ = dispatch(
        &mut state,
        &output,
        &ws,
        &domain,
        &empty,
        &retained,
        None,
        "d23-sole-min",
    );
    assert!(state.engine.session(&domain.1).is_some());
    // The sole minimized window closes: the production inventory empties
    // with a dead tag, yet one explicit empty-domain cleanup still
    // delivers the close with focus none and retires the session.
    live_tags(&[(101, None)]);
    let (gone, retained) = tick_leg(
        &mut state,
        &me,
        &fulls,
        &areas,
        Vec::new(),
        Vec::new(),
        Vec::new(),
    );
    assert!(gone.is_empty());
    assert!(state.pending_closed.contains_key("wA"));
    let reply = dispatch(
        &mut state,
        &output,
        &ws,
        &domain,
        &gone,
        &retained,
        None,
        "d23-sole-close",
    );
    let xs = geometry_x(&reply);
    assert!(xs.is_empty());
    assert!(
        state.engine.session(&domain.1).is_none(),
        "forgotten sole slot retires the session"
    );
    assert!(state.pending_closed.is_empty());
    clear_pending_seams();
}

#[test]
fn production_tick_journey_minimize_restore_close_fanout_with_hidden() {
    // Production per-tick sequence across two outputs with a hidden
    // ledger member: seed -> minimize+hide -> unreadable defer ->
    // restore -> hidden close -> visible close. Every leg runs real
    // observe/assign/cleanup/assemble/handle in tick order with the
    // iconic/unreadable/recovery fixture seams; only admission (native
    // tag stamp) is pre-wired. The tick loop visits selected workspaces
    // only, so the hidden-domain report delivers on that domain's next
    // covering observation with no hidden polling, no fabricated rows,
    // and no hidden writes.
    clear_pending_seams();
    let output1 = "mon-1".to_owned();
    let output2 = "mon-2".to_owned();
    let mut state = test_state();
    state.workspaces.ensure_output(&output1);
    state.workspaces.ensure_output(&output2);
    let ws1 = state.workspaces.active_id(&output1).expect("active");
    let ws2 = state.workspaces.active_id(&output2).expect("active");
    let domain1 = crate::workspace_owner::workspace_domain(&output1, &ws1, bounds(), 8);
    let domain2 = crate::workspace_owner::workspace_domain(&output2, &ws2, bounds(), 8);
    let rect = bounds();
    for (output, ws, hwnd, token) in [
        (&output1, &ws1, 101u64, "wA"),
        (&output1, &ws1, 102, "wB"),
        (&output1, &ws1, 103, "wC"),
        (&output2, &ws2, 201, "wX"),
        (&output2, &ws2, 202, "wH"),
    ] {
        wire_member(&mut state, output, ws, hwnd, token, rect);
    }
    let me = me();
    let fulls = vec![bounds(), bounds()];
    let areas = vec![
        MonitorArea {
            device: output1.clone(),
            work: bounds(),
            full: bounds(),
        },
        MonitorArea {
            device: output2.clone(),
            work: bounds(),
            full: bounds(),
        },
    ];
    let (store, tmp) = tmp_store("journey");
    // Seed both domains through the full sequence.
    live_tags(&[
        (101, Some("wA")),
        (102, Some("wB")),
        (103, Some("wC")),
        (201, Some("wX")),
        (202, Some("wH")),
    ]);
    let (observed, retained) = tick_leg(
        &mut state,
        &me,
        &fulls,
        &areas,
        vec![
            mk_observed(101, "wA", 0, 0),
            mk_observed(102, "wB", 0, 0),
            mk_observed(103, "wC", 0, 0),
            mk_observed(201, "wX", 0, 0),
            mk_observed(202, "wH", 0, 0),
        ],
        Vec::new(),
        Vec::new(),
    );
    assert_eq!(observed.len(), 5, "seed observes everything");
    assert!(state.minimized_omitted.is_empty());
    let reply = dispatch(
        &mut state,
        &output1,
        &ws1,
        &domain1,
        &observed[..3],
        &retained,
        Some("wC"),
        "d23-j0",
    );
    assert!(
        matches!(
            reply,
            tiler_core::boundary::CoreReply::Projection(_)
                | tiler_core::boundary::CoreReply::Tiled(_)
        ),
        "mon-1 seed must converge"
    );
    let reply = dispatch(
        &mut state,
        &output2,
        &ws2,
        &domain2,
        &observed[3..],
        &retained,
        Some("wX"),
        "d23-j0b",
    );
    assert!(
        matches!(
            reply,
            tiler_core::boundary::CoreReply::Projection(_)
                | tiler_core::boundary::CoreReply::Tiled(_)
        ),
        "mon-2 seed must converge"
    );
    // Minimize B (iconic fixture -> real omission) and hide H iconic
    // with a real claim; H stays enumerated-but-hidden via the
    // unreadable seam, exactly as the raw gate leaves it.
    hide_member(&mut state, 202, "claim-wh", true);
    live_tags(&[
        (101, Some("wA")),
        (102, Some("wB")),
        (103, Some("wC")),
        (201, Some("wX")),
        (202, Some("wH")),
    ]);
    let (observed, retained) = tick_leg(
        &mut state,
        &me,
        &fulls,
        &areas,
        vec![
            mk_observed(101, "wA", 0, 0),
            mk_observed(103, "wC", 0, 0),
            mk_observed(201, "wX", 0, 0),
        ],
        vec![102],
        vec![202],
    );
    assert_eq!(observed.len(), 3);
    assert!(
        state.minimized_omitted.contains(&key(102)),
        "iconic fixture derives omission through real observe"
    );
    assert!(
        state.member_tokens.contains_key(&key(102)),
        "minimized membership retained"
    );
    assert!(state.pending_closed.is_empty(), "no close on minimize");
    let reply = dispatch(
        &mut state,
        &output1,
        &ws1,
        &domain1,
        &observed[..2],
        &retained,
        Some("wC"),
        "d23-j1",
    );
    assert!(
        matches!(
            reply,
            tiler_core::boundary::CoreReply::Projection(_)
                | tiler_core::boundary::CoreReply::Tiled(_)
        ),
        "minimize leg must converge"
    );
    assert!(slot_present(&state, &domain1.1, "wB"), "B stores its slot");
    let mut hint_cx = HintCx::new();
    let rows2 = assemble_domain_rows(
        &mut state,
        &output2,
        &ws2,
        &observed[2..],
        &retained,
        "reconcile",
        "d23-j1b",
        &mut hint_cx,
    )
    .expect("hidden omission stays complete");
    assert_eq!(rows2.len(), 1, "only X rides; iconic-hidden H omits");
    assert_eq!(rows2[0].token, "wX");
    assert!(!rows2[0].floating, "hidden survivor stays tiled");
    assert!(
        !writable_tokens(&state, &output2, &ws2, &observed[2..]).contains("wH"),
        "hidden members take no geometry writes"
    );
    let reply = dispatch(
        &mut state,
        &output2,
        &ws2,
        &domain2,
        &observed[2..],
        &retained,
        Some("wX"),
        "d23-j1b",
    );
    assert!(
        matches!(
            reply,
            tiler_core::boundary::CoreReply::Projection(_)
                | tiler_core::boundary::CoreReply::Tiled(_)
        ),
        "hide leg must converge"
    );
    assert!(
        slot_present(&state, &domain2.1, "wH"),
        "H stores its slot on hide"
    );
    assert_eq!(focused_token(&state, &domain1.1).as_deref(), Some("wC"));
    assert_eq!(focused_token(&state, &domain2.1).as_deref(), Some("wX"));
    // C goes unreadable for a leg: inventoried but viewless and not
    // omitted, so mon-1 defers with retained state while mon-2 still
    // converges; nothing queues anywhere.
    live_tags(&[
        (101, Some("wA")),
        (102, Some("wB")),
        (103, Some("wC")),
        (201, Some("wX")),
        (202, Some("wH")),
    ]);
    let (observed, retained) = tick_leg(
        &mut state,
        &me,
        &fulls,
        &areas,
        vec![mk_observed(101, "wA", 0, 0), mk_observed(201, "wX", 0, 0)],
        vec![102],
        vec![103, 202],
    );
    assert_eq!(observed.len(), 2);
    let mut hint_cx = HintCx::new();
    assert!(
        assemble_domain_rows(
            &mut state,
            &output1,
            &ws1,
            &observed[..1],
            &retained,
            "reconcile",
            "d23-j2",
            &mut hint_cx,
        )
        .is_none(),
        "viewless non-minimized member defers, never fabricates"
    );
    let reply = dispatch(
        &mut state,
        &output2,
        &ws2,
        &domain2,
        &observed[1..],
        &retained,
        Some("wX"),
        "d23-j2b",
    );
    assert!(
        matches!(
            reply,
            tiler_core::boundary::CoreReply::Projection(_)
                | tiler_core::boundary::CoreReply::Tiled(_)
        ),
        "unaffected output still converges during deferral"
    );
    assert!(state.pending_closed.is_empty(), "unreadable never queues");
    assert!(
        state.member_tokens.contains_key(&key(103)),
        "C membership retained"
    );
    // Restore B with host focus on C: rejoins between former neighbors.
    live_tags(&[
        (101, Some("wA")),
        (102, Some("wB")),
        (103, Some("wC")),
        (201, Some("wX")),
        (202, Some("wH")),
    ]);
    let (observed, retained) = tick_leg(
        &mut state,
        &me,
        &fulls,
        &areas,
        vec![
            mk_observed(101, "wA", 0, 0),
            mk_observed(102, "wB", 0, 0),
            mk_observed(103, "wC", 0, 0),
            mk_observed(201, "wX", 0, 0),
        ],
        Vec::new(),
        vec![202],
    );
    assert_eq!(observed.len(), 4);
    let reply = dispatch(
        &mut state,
        &output1,
        &ws1,
        &domain1,
        &observed[..3],
        &retained,
        Some("wC"),
        "d23-j3",
    );
    assert!(
        matches!(
            reply,
            tiler_core::boundary::CoreReply::Projection(_)
                | tiler_core::boundary::CoreReply::Tiled(_)
        ),
        "restore leg must converge"
    );
    assert_eq!(
        leaf_window_order(&state, &domain1.1),
        vec!["wA".to_owned(), "wB".to_owned(), "wC".to_owned()],
        "production restore rejoins the old neighborhood"
    );
    assert_eq!(focused_token(&state, &domain1.1).as_deref(), Some("wC"));
    // H closes while hidden: the audit retires the real claim and the
    // next mon-2 observation delivers the close beside X's snapshot.
    inject_pending_recovery(202, Err("absent: hwnd destroyed".to_owned()));
    inject_pending_tag(202, None);
    audit_hidden_claims(&mut state, &me, &store);
    assert!(
        !state.hidden_claims.contains_key(&key(202)),
        "hidden claim retires"
    );
    assert_eq!(
        state.pending_closed.get("wH"),
        Some(&super::PendingClosed {
            output: output2.clone(),
            workspace: ws2.clone(),
        }),
        "hidden close queues with hidden-domain homing"
    );
    live_tags(&[
        (101, Some("wA")),
        (102, Some("wB")),
        (103, Some("wC")),
        (201, Some("wX")),
    ]);
    let (observed, retained) = tick_leg(
        &mut state,
        &me,
        &fulls,
        &areas,
        vec![
            mk_observed(101, "wA", 0, 0),
            mk_observed(102, "wB", 0, 0),
            mk_observed(103, "wC", 0, 0),
            mk_observed(201, "wX", 0, 0),
        ],
        Vec::new(),
        Vec::new(),
    );
    let reply = dispatch(
        &mut state,
        &output2,
        &ws2,
        &domain2,
        &observed[3..],
        &retained,
        Some("wX"),
        "d23-j4",
    );
    assert!(
        matches!(
            reply,
            tiler_core::boundary::CoreReply::Projection(_)
                | tiler_core::boundary::CoreReply::Tiled(_)
        ),
        "hidden-close leg must converge"
    );
    assert!(
        !slot_present(&state, &domain2.1, "wH"),
        "hidden close forgets the slot"
    );
    assert!(state.pending_closed.is_empty(), "hidden close drains");
    // B closes visible: inventory absence plus dead tag queues through
    // the ordinary cleanup, and mon-1 delivers beside the survivors.
    live_tags(&[
        (101, Some("wA")),
        (102, None),
        (103, Some("wC")),
        (201, Some("wX")),
    ]);
    let (observed, retained) = tick_leg(
        &mut state,
        &me,
        &fulls,
        &areas,
        vec![
            mk_observed(101, "wA", 0, 0),
            mk_observed(103, "wC", 0, 0),
            mk_observed(201, "wX", 0, 0),
        ],
        Vec::new(),
        Vec::new(),
    );
    assert_eq!(observed.len(), 3);
    assert!(
        state.pending_closed.contains_key("wB"),
        "visible close queues through production cleanup"
    );
    let reply = dispatch(
        &mut state,
        &output1,
        &ws1,
        &domain1,
        &observed[..2],
        &retained,
        Some("wC"),
        "d23-j5",
    );
    assert!(
        matches!(
            reply,
            tiler_core::boundary::CoreReply::Projection(_)
                | tiler_core::boundary::CoreReply::Tiled(_)
        ),
        "visible-close leg must converge"
    );
    assert_eq!(
        leaf_window_order(&state, &domain1.1),
        vec!["wA".to_owned(), "wC".to_owned()],
        "survivors converge after the close"
    );
    assert!(!slot_present(&state, &domain1.1, "wB"));
    assert!(state.pending_closed.is_empty());
    assert_eq!(focused_token(&state, &domain1.1).as_deref(), Some("wC"));
    let _ = std::fs::remove_dir_all(&tmp);
    clear_pending_seams();
}

include!("tests_d23_minimize_2.rs");

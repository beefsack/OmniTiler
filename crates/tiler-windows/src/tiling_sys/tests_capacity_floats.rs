#![forbid(unsafe_code)]
// Committed capacity floats through the real adapter seams: fresh/seed
// exhaustion and shrink-repair victims register `floated`/`float_rects`
// plus the ordinary intent marker; staged-only candidates and refused
// observations persist nothing; same-output `MoveToFloat` sends commit
// through the tiled branch with proof-gated registration while
// cross-output floats refuse without a remap; explicit unfloat retries
// stay floating with markers and a truthful no-space; restart
// re-adopts marked floats as ordinary. Fake HWNDs fail closed, so these
// pin Engine commits, membership, registration, and routing vocabulary
// (never faked OS successes).
use super::rmax03_adapter_tests::test_state;
use super::{
    ActionCtx, LedgerStore, MonitorArea, SendTarget, clear_pending_seams, inject_pending_apply,
    inject_pending_elevated, inject_pending_observe, inject_pending_suspend, inject_pending_tag,
    pending_counters,
};
use crate::product_hide::sys::fake;
use tiler_core::directional::WindowId;
use tiler_core::geometry::Rect;
use tiler_core::ids::CorrelationId;

/// Domain bounds with room for exactly one tile at gap 8: a lone leaf
/// projects, but any binary split needs a `>= gap + 2 = 10` long edge no
/// `9 x 9` tree offers on either axis. The second window exhausts
/// deterministically through ordinary anchor plus D2 fallback into the
/// D3 float, independent of tree shape or split axis.
const TIGHT_BOUNDS: Rect = Rect {
    x: 0,
    y: 0,
    w: 9,
    h: 9,
};
const ROOMY_BOUNDS: Rect = Rect {
    x: 0,
    y: 0,
    w: 800,
    h: 600,
};
const GAP: i32 = 8;

fn me() -> crate::model::ProcessIdentity {
    crate::model::ProcessIdentity {
        pid: 4242,
        process_creation: "test-creation".to_owned(),
        user_sid: "S-1-5-test".to_owned(),
        session_id: 1,
        exe_path: "test.exe".to_owned(),
    }
}

fn member_key(hwnd: u64, pid: u32) -> crate::workspace::WindowKey {
    crate::workspace::WindowKey {
        hwnd,
        pid,
        creation: format!("c{hwnd:016x}"),
    }
}

fn frame(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
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

/// Register one scripted native window for marker tests: alive with
/// matching pid/process/tag, standing in for the OS reads behind the
/// hold, pid, lifetime, and property gates. No desktop window exists.
/// Scripted integrity is medium, so the held gate passes; high-integrity
/// refusal is scripted per-window (see the stale-pid test).
fn register_fake(key: &crate::workspace::WindowKey, tag: &str) {
    fake::register(
        key.hwnd,
        fake::FakeWindow {
            pid: key.pid,
            process: crate::model::ProcessIdentity {
                pid: key.pid,
                process_creation: key.creation.clone(),
                user_sid: "S-1-5-test".to_owned(),
                session_id: 1,
                exe_path: "test.exe".to_owned(),
            },
            integrity_rid: crate::lifecycle::MEDIUM_RID_MIN,
            nonce: None,
            member_tag: Some(tag.to_owned()),
            visible: true,
            iconic: false,
            maximized: false,
            alive: true,
            post_ok: true,
            defer: false,
        },
    );
}

fn tiny_domain(
    output: &str,
    workspace: &str,
) -> (
    tiler_core::session::OutputDomain,
    tiler_core::session::DomainKey,
) {
    crate::workspace_owner::workspace_domain(output, workspace, TIGHT_BOUNDS, GAP)
}

fn committed_revision(state: &super::TileLoop, key: &tiler_core::session::DomainKey) -> u64 {
    state
        .engine
        .session(key)
        .map(|session| session.accepted_revision())
        .unwrap_or(0)
}

/// Drive one Reconcile through the production adapter seam
/// (`handle_engine_event`: stamping plus commit) over explicit rows.
fn drive_reconcile(
    state: &mut super::TileLoop,
    domain: &tiler_core::session::OutputDomain,
    key: &tiler_core::session::DomainKey,
    rows: &[(&str, Rect, bool)],
    focused: Option<&str>,
) -> tiler_core::boundary::CoreReply {
    use tiler_core::boundary::CoreReply;
    let owner = state.owner.clone();
    let generation = state.generation.clone();
    let carried: Vec<(
        WindowId,
        Rect,
        tiler_core::size_hints::WindowSizeHints,
        bool,
    )> = rows
        .iter()
        .map(|(token, rect, floating)| {
            (
                WindowId((*token).to_owned()),
                *rect,
                tiler_core::size_hints::WindowSizeHints::none(),
                *floating,
            )
        })
        .collect();
    let revision = committed_revision(state, key);
    let fp = crate::tiling::fingerprint(
        &rows
            .iter()
            .map(|(token, rect, _)| ((*token).to_owned(), *rect))
            .collect::<Vec<_>>(),
    );
    let mut event = crate::tiling::build_reconcile_event_for_floating(
        &owner,
        &generation,
        &CorrelationId::parse("capacity-float").expect("correlation"),
        revision,
        fp,
        domain,
        key,
        state.outer_gap,
        &carried,
        focused.map(|token| WindowId(token.to_owned())).as_ref(),
    );
    let reply = super::handle_engine_event(state, &mut event);
    assert!(
        !matches!(reply, CoreReply::Diverged(_)),
        "no divergence, got {reply:?}"
    );
    reply
}

/// Tiled tokens in one plan reply: capacity floats and exceptions never
/// ride desired geometry, so presence here means a committed tile.
fn tiled_tokens(reply: &tiler_core::boundary::CoreReply) -> Vec<String> {
    crate::workspace_owner::planned_writes(reply)
        .unwrap_or_default()
        .iter()
        .map(|write| write.window.0.clone())
        .collect()
}

#[test]
fn fresh_capacity_float_registers_ordinary_lanes() {
    // Fresh exhaustion in tight bounds: the newcomer commits slotless
    // with diagnostic capacity origin and no tile geometry, then joins
    // the ordinary float lanes with its live frame. Marker install
    // fails closed on the fake HWND while runtime state still
    // registers; a second pass is a silent no-op.
    clear_pending_seams();
    let mut state = test_state();
    let me = me();
    let output = "mon-a".to_owned();
    state.workspaces.ensure_output(&output);
    let ws = state.workspaces.active_id(&output).expect("active");
    let (domain, key) = tiny_domain(&output, &ws);
    let key_a = member_key(0x601, 1601);
    let key_b = member_key(0x602, 1602);
    stage_member(&mut state, &output, &ws, &key_a, "wA", "tag-wa");
    stage_member(&mut state, &output, &ws, &key_b, "wB", "tag-wb");
    let frame_a = TIGHT_BOUNDS;
    let frame_b = frame(2, 2, 5, 5);
    let reply = drive_reconcile(
        &mut state,
        &domain,
        &key,
        &[("wA", frame_a, false)],
        Some("wA"),
    );
    assert!(
        matches!(
            reply,
            tiler_core::boundary::CoreReply::Projection(_)
                | tiler_core::boundary::CoreReply::Tiled(_)
        ),
        "first seed converges, got {reply:?}"
    );
    assert!(
        tiled_tokens(&reply).contains(&"wA".to_owned()),
        "anchor tiles"
    );
    let reply = drive_reconcile(
        &mut state,
        &domain,
        &key,
        &[("wA", frame_a, false), ("wB", frame_b, false)],
        Some("wA"),
    );
    assert!(
        matches!(
            reply,
            tiler_core::boundary::CoreReply::Projection(_)
                | tiler_core::boundary::CoreReply::Tiled(_)
        ),
        "exhausted admission converges, got {reply:?}"
    );
    let session = state.engine.session(&key).expect("session");
    assert!(
        session.is_capacity_float(&WindowId("wB".to_owned())),
        "newcomer carries capacity origin"
    );
    assert!(
        session.is_exception(&WindowId("wB".to_owned())),
        "newcomer floats slotless"
    );
    assert!(
        tiled_tokens(&reply).contains(&"wA".to_owned()),
        "anchor keeps its tile"
    );
    assert!(
        !tiled_tokens(&reply).contains(&"wB".to_owned()),
        "float rides no tile geometry"
    );
    // Tiled focus stays on the prior MRU: the capacity float takes no
    // tiled focus and no native activation runs on this path.
    let (focus_domain, focus_leaf) = session.focus();
    assert!(focus_domain.is_some() && focus_leaf.is_some());
    let mut frames = std::collections::HashMap::new();
    frames.insert("wB".to_owned(), frame_b);
    let adopted = super::register_committed_capacity_floats(&mut state, &me, &key, &frames);
    assert_eq!(adopted.len(), 1, "one newcomer registers");
    assert_eq!(adopted[0].token, "wB");
    assert!(!adopted[0].marker_ok, "fake HWND fails the marker closed");
    assert!(
        state.floated.contains(&key_b),
        "ordinary floated lane joins"
    );
    assert_eq!(
        state.float_rects.get("wB").copied(),
        Some(frame_b),
        "live frame retains"
    );
    let again = super::register_committed_capacity_floats(&mut state, &me, &key, &frames);
    assert!(again.is_empty(), "replay registers nothing");
    clear_pending_seams();
}

#[test]
fn cross_output_capacity_float_refuses_without_remap() {
    // Explicit output send into an exhausted cross-output destination
    // through the real `run_output_send` route: core stages the
    // slotless float, but the adapter has no safe float remap (no
    // move-only frame primitive; arrival proof is rect-based), so the
    // transfer refuses truthfully with the source exactly preserved:
    // no Engine mutation, no membership change, no float lane.
    use crate::snapkey::{QueuedWorkspaceOutputIntent, SnapEdge, SnapOrigin};
    use tiler_core::directional::Direction;
    clear_pending_seams();
    let mut state = test_state();
    let me = me();
    let areas = vec![
        MonitorArea {
            device: "primary".to_owned(),
            work: frame(0, 40, 1920, 1040),
            full: frame(0, 0, 1920, 1080),
        },
        MonitorArea {
            device: "right".to_owned(),
            work: frame(1920, 0, 25, 25),
            full: frame(1920, 0, 25, 25),
        },
    ];
    let fulls: Vec<Rect> = areas.iter().map(|area| area.full).collect();
    for output in ["primary", "right"] {
        state.workspaces.ensure_output(output);
    }
    let ws_a = state.workspaces.active_id("primary").expect("active");
    let ws_b = state.workspaces.active_id("right").expect("active");
    state.workspaces.set_tiled("primary", &ws_a, true);
    state.workspaces.set_tiled("right", &ws_b, true);
    let (source_domain, source_key) =
        super::workspace_domain_for("primary", &ws_a, &areas, state.inner_gap, state.outer_gap)
            .expect("source domain");
    let (target_domain, target_key) =
        super::workspace_domain_for("right", &ws_b, &areas, state.inner_gap, state.outer_gap)
            .expect("target domain");
    assert_eq!(target_domain.bounds.w, 9, "single-tile target");
    let mover = member_key(0x671, 1671);
    let key_t1 = member_key(0x672, 1672);
    stage_member(&mut state, "primary", &ws_a, &mover, "wM", "tag-wm");
    stage_member(&mut state, "right", &ws_b, &key_t1, "wT1", "tag-wt1");
    let frame_m = frame(100, 100, 400, 300);
    for (domain, key, rows, focused) in [
        (
            &source_domain,
            &source_key,
            vec![("wM", frame_m, false)],
            "wM",
        ),
        (
            &target_domain,
            &target_key,
            vec![("wT1", target_domain.bounds, false)],
            "wT1",
        ),
    ] {
        let reply = drive_reconcile(&mut state, domain, key, &rows, Some(focused));
        assert!(
            matches!(
                reply,
                tiler_core::boundary::CoreReply::Projection(_)
                    | tiler_core::boundary::CoreReply::Tiled(_)
            ),
            "seed converges, got {reply:?}"
        );
    }
    assert_eq!(
        tiled_tokens(&drive_reconcile(
            &mut state,
            &target_domain,
            &target_key,
            &[("wT1", target_domain.bounds, false)],
            Some("wT1"),
        )),
        vec!["wT1".to_owned()],
        "lone target tiles"
    );
    let observed = vec![
        observed_window(&mover, "wM", frame_m),
        observed_window(&key_t1, "wT1", target_domain.bounds),
    ];
    let source_rows = vec![crate::workspace_owner::OwnerRow {
        token: "wM".to_owned(),
        rect: frame_m,
        hints: tiler_core::size_hints::WindowSizeHints::none(),
        floating: false,
    }];
    inject_pending_suspend(Some(false));
    inject_pending_elevated(Some(false));
    super::inject_monitor_areas(Some(areas.clone()));
    let home = crate::workspace::MemberLoc {
        output: "primary".to_owned(),
        workspace: ws_a.clone(),
        hidden: false,
    };
    let engine_before = format!("{:?}", state.engine);
    let tick = state.tick;
    let mut hint_cx = super::HintCx::new();
    let outcome = super::run_output_send(
        &mut state,
        &me,
        &fulls,
        &areas,
        &observed,
        &[],
        &mut hint_cx,
        &home,
        &mover,
        "wM",
        &source_rows,
        "right",
        &ws_b,
        &QueuedWorkspaceOutputIntent {
            direction: Direction::Right,
            follow: true,
            edge: SnapEdge::Down,
            origin: None,
            consumed: true,
            announce: true,
            tick: std::time::Instant::now(),
        },
        &SnapOrigin {
            hwnd: mover.hwnd,
            token: "wM".to_owned(),
            pid: mover.pid,
            creation: mover.creation.clone(),
        },
        tick,
        &CorrelationId::parse("cap-output").expect("correlation"),
        "cap-output",
    );
    assert_eq!(outcome, "float-no-remap", "no safe remap refuses");
    assert_eq!(
        format!("{:?}", state.engine),
        engine_before,
        "refused candidate mutates no Engine state"
    );
    assert_eq!(
        state
            .workspaces
            .member_loc(&mover)
            .map(|loc| (loc.output.clone(), loc.workspace.clone())),
        Some(("primary".to_owned(), ws_a.clone())),
        "source membership stands"
    );
    assert!(
        state
            .engine
            .session(&target_key)
            .is_some_and(|s| !s.is_exception(&WindowId("wM".to_owned()))),
        "destination takes no float"
    );
    assert!(
        !state.floated.contains(&mover),
        "refused transfers persist no lane"
    );
    clear_pending_seams();
}
#[test]
fn unfloat_retry_without_space_stays_floating() {
    // Explicit tile retry of a registered capacity float through the
    // real `apply_unfloat_to_tiled` route on an exhausted domain: the
    // Engine re-floats (names the native float target, no frame
    // authority), so the floated lanes and marker persist, no tile win
    // records, and the outcome reports the truthful no-space instead
    // of a tile success. Marker repair fails closed on the fake HWND
    // while state still holds.
    clear_pending_seams();
    let mut state = test_state();
    let me = me();
    let output = "mon-a".to_owned();
    state.workspaces.ensure_output(&output);
    let ws = state.workspaces.active_id(&output).expect("active");
    let (domain, key) = tiny_domain(&output, &ws);
    let key_a = member_key(0x681, 1681);
    let key_b = member_key(0x682, 1682);
    stage_member(&mut state, &output, &ws, &key_a, "wA", "tag-wa");
    stage_member(&mut state, &output, &ws, &key_b, "wB", "tag-wb");
    let frame_b = frame(2, 2, 5, 5);
    for rows in [
        vec![("wA", TIGHT_BOUNDS, false)],
        vec![("wA", TIGHT_BOUNDS, false), ("wB", frame_b, false)],
    ] {
        let focused = rows.last().map(|(token, _, _)| *token).expect("row");
        let reply = drive_reconcile(&mut state, &domain, &key, &rows, Some(focused));
        assert!(
            matches!(
                reply,
                tiler_core::boundary::CoreReply::Projection(_)
                    | tiler_core::boundary::CoreReply::Tiled(_)
            ),
            "seed converges, got {reply:?}"
        );
    }
    assert!(
        state
            .engine
            .session(&key)
            .is_some_and(|s| s.is_capacity_float(&WindowId("wB".to_owned()))),
        "retry subject floats with capacity origin"
    );
    // As if registered: ordinary lanes hold the float with its frame.
    state.floated.insert(key_b.clone());
    state.float_rects.insert("wB".to_owned(), frame_b);
    assert!(!state.fixed_tile_overrides.contains(&key_b));
    let observed = vec![
        observed_window(&key_a, "wA", TIGHT_BOUNDS),
        observed_window(&key_b, "wB", frame_b),
    ];
    // Canned geometry effects only: the survivor reflow never touches
    // native windows here, and no observation pass runs inside.
    inject_pending_observe(Vec::new(), Vec::new(), false);
    inject_pending_apply(Some((true, 0)));
    let settled: std::cell::RefCell<Vec<&'static str>> = Default::default();
    let settle = |outcome: &'static str| {
        settled.borrow_mut().push(outcome);
        serde_json::json!({})
    };
    let loc = state.workspaces.member_loc(&key_b).expect("homed").clone();
    let log_path = state.log_path.clone();
    let tick = state.tick;
    super::apply_unfloat_to_tiled(
        &mut state,
        &me,
        &[frame(0, 0, 36, 36)],
        &[MonitorArea {
            device: output.clone(),
            work: frame(-8, -8, 25, 25),
            full: frame(-8, -8, 25, 25),
        }],
        &observed,
        &[],
        Vec::new(),
        &key_b,
        "wB",
        &loc,
        Some(frame_b),
        &crate::snapkey::SnapOrigin {
            hwnd: key_b.hwnd,
            token: "wB".to_owned(),
            pid: key_b.pid,
            creation: key_b.creation.clone(),
        },
        &settle,
        &log_path,
        tick,
        &CorrelationId::parse("unfloat-cap").expect("correlation"),
        "toggle-float",
        "tile",
        false,
    );
    assert_eq!(
        settled.borrow().as_slice(),
        ["unfloat-no-space"],
        "truthful no-space, no tile success"
    );
    let session = state.engine.session(&key).expect("session");
    assert!(
        session.is_exception(&WindowId("wB".to_owned())),
        "still a floating exception"
    );
    assert!(
        session.is_capacity_float(&WindowId("wB".to_owned())),
        "capacity origin persists"
    );
    assert!(state.floated.contains(&key_b), "floated lane persists");
    assert_eq!(
        state.float_rects.get("wB").copied(),
        Some(frame_b),
        "live frame persists"
    );
    assert!(
        !state.fixed_tile_overrides.contains(&key_b),
        "no false tile win records"
    );
    clear_pending_seams();
}

#[test]
fn tick_helper_adopts_fresh_float_without_activation() {
    // The shared tick-leg helper over a committed fresh float: the
    // exhausted newcomer registers with its live frame, tiled focus
    // stays on the prior MRU (never the float), and a replayed call
    // adopts nothing. The helper performs no setters by construction;
    // native activation behavior stays live acceptance.
    clear_pending_seams();
    let mut state = test_state();
    let me = me();
    let output = "mon-a".to_owned();
    state.workspaces.ensure_output(&output);
    let ws = state.workspaces.active_id(&output).expect("active");
    let (domain, key) = tiny_domain(&output, &ws);
    let key_a = member_key(0x691, 1691);
    let key_b = member_key(0x692, 1692);
    stage_member(&mut state, &output, &ws, &key_a, "wA", "tag-wa");
    stage_member(&mut state, &output, &ws, &key_b, "wB", "tag-wb");
    let frame_a = TIGHT_BOUNDS;
    let frame_b = frame(1, 1, 7, 7);
    let reply = drive_reconcile(
        &mut state,
        &domain,
        &key,
        &[("wA", frame_a, false)],
        Some("wA"),
    );
    assert!(
        matches!(
            reply,
            tiler_core::boundary::CoreReply::Projection(_)
                | tiler_core::boundary::CoreReply::Tiled(_)
        ),
        "seed converges, got {reply:?}"
    );
    let reply = drive_reconcile(
        &mut state,
        &domain,
        &key,
        &[("wA", frame_a, false), ("wB", frame_b, false)],
        Some("wA"),
    );
    let observed = vec![
        observed_window(&key_a, "wA", frame_a),
        observed_window(&key_b, "wB", frame_b),
    ];
    super::adopt_tick_capacity_floats(&mut state, &me, 7, "tick-cap", &reply, &observed, &key);
    let session = state.engine.session(&key).expect("session");
    assert!(
        session.is_capacity_float(&WindowId("wB".to_owned())),
        "newcomer floats with capacity origin"
    );
    assert!(state.floated.contains(&key_b), "helper registers floated");
    assert_eq!(
        state.float_rects.get("wB").copied(),
        Some(frame_b),
        "helper retains the live frame"
    );
    let (focus_domain, focus_leaf) = session.focus();
    assert!(focus_domain.is_some(), "tiled focus stays homed");
    assert_eq!(
        focus_leaf.map(|leaf| leaf.0.clone()),
        Some("leaf-wA".to_owned()),
        "prior MRU keeps tiled focus, never the float"
    );
    // Stable replay: the same call changes nothing further.
    let revision = session.accepted_revision();
    let floated = state.floated.len();
    super::adopt_tick_capacity_floats(&mut state, &me, 8, "tick-cap", &reply, &observed, &key);
    assert_eq!(
        state
            .engine
            .session(&key)
            .map(|s| s.accepted_revision())
            .unwrap_or(0),
        revision,
        "replay bumps no revision"
    );
    assert_eq!(state.floated.len(), floated, "replay adopts nothing");
    // Diverged replies stay untouched.
    super::adopt_tick_capacity_floats(
        &mut state,
        &me,
        9,
        "tick-cap",
        &tiler_core::boundary::CoreReply::Diverged(
            tiler_core::contract::DivergenceKind::OwnerMismatch,
        ),
        &observed,
        &key,
    );
    assert_eq!(state.floated.len(), floated, "divergence adopts nothing");
    clear_pending_seams();
}

#[test]
fn marker_failure_recovers_on_later_pass() {
    // Lane registration and marker repair are independent: with no
    // scripted window the lanes still register while the marker
    // reports unverified; once the window answers, the next
    // event-driven pass repairs the marker without touching the lanes,
    // and a further pass stays silent (no redundant writes).
    clear_pending_seams();
    fake::clear();
    let mut state = test_state();
    let me = me();
    let output = "mon-a".to_owned();
    state.workspaces.ensure_output(&output);
    let ws = state.workspaces.active_id(&output).expect("active");
    let (domain, key) = tiny_domain(&output, &ws);
    let key_a = member_key(0x6A1, 1691);
    let key_b = member_key(0x6A2, 1692);
    stage_member(&mut state, &output, &ws, &key_a, "wA", "tag-wa");
    stage_member(&mut state, &output, &ws, &key_b, "wB", "tag-wb");
    let frame_b = frame(2, 2, 5, 5);
    for rows in [
        vec![("wA", TIGHT_BOUNDS, false)],
        vec![("wA", TIGHT_BOUNDS, false), ("wB", frame_b, false)],
    ] {
        let focused = rows.last().map(|(token, _, _)| *token).expect("row");
        let reply = drive_reconcile(&mut state, &domain, &key, &rows, Some(focused));
        assert!(
            matches!(
                reply,
                tiler_core::boundary::CoreReply::Projection(_)
                    | tiler_core::boundary::CoreReply::Tiled(_)
            ),
            "seed converges, got {reply:?}"
        );
    }
    let mut frames = std::collections::HashMap::new();
    frames.insert("wB".to_owned(), frame_b);
    let adopted = super::register_committed_capacity_floats(&mut state, &me, &key, &frames);
    assert_eq!(adopted.len(), 1, "lanes register without the marker");
    assert!(!adopted[0].marker_ok, "unscripted window fails closed");
    assert!(state.floated.contains(&key_b));
    assert_eq!(state.float_rects.get("wB").copied(), Some(frame_b));
    // The window starts answering: the next pass repairs the marker,
    // keeps the lanes byte-identical, and reports the repair.
    register_fake(&key_b, "tag-wb");
    let adopted = super::register_committed_capacity_floats(&mut state, &me, &key, &frames);
    assert_eq!(adopted.len(), 1, "repair reports once");
    assert_eq!(adopted[0].token, "wB");
    assert!(adopted[0].marker_ok, "marker reads back");
    assert!(state.floated.contains(&key_b), "lane undisturbed");
    assert_eq!(
        state.float_rects.get("wB").copied(),
        Some(frame_b),
        "frame never rewritten"
    );
    assert!(
        crate::product_hide::sys::read_float_intent_marker(key_b.hwnd),
        "marker present on the window"
    );
    // A further pass stays silent: present marker, settled lanes.
    let adopted = super::register_committed_capacity_floats(&mut state, &me, &key, &frames);
    assert!(adopted.is_empty(), "no redundant reports");
    assert!(
        crate::product_hide::sys::read_float_intent_marker(key_b.hwnd),
        "marker still present"
    );
    fake::clear();
    clear_pending_seams();
}
#[test]
fn hidden_shrink_registers_victims_through_fanout() {
    // Hidden-domain D7 through the production close-drain fanout: two
    // tiles seeded roomy, hidden, then fanned out under shrunken
    // bounds. The newest evicts with capacity origin and registers
    // its lanes (last live frame, marker attempted) with no geometry
    // or focus writes on this path; the queued close still drains.
    clear_pending_seams();
    let mut state = test_state();
    let me = me();
    let output = "mon-a".to_owned();
    state.workspaces.ensure_output(&output);
    let ids = state.workspaces.workspace_ids(&output);
    let (hidden, active) = (ids[0].clone(), ids[1].clone());
    state.workspaces.activate(&output, &active);
    let roomy = crate::workspace_owner::workspace_domain(&output, &hidden, ROOMY_BOUNDS, GAP);
    let key1 = member_key(0x6C1, 1691);
    let key2 = member_key(0x6C2, 1692);
    stage_member(&mut state, &output, &hidden, &key1, "w1", "tag-w1");
    stage_member(&mut state, &output, &hidden, &key2, "w2", "tag-w2");
    for (i, token) in ["w1", "w2"].iter().enumerate() {
        let rows: Vec<(&str, Rect, bool)> = ["w1", "w2"][..=i]
            .iter()
            .map(|t| (*t, ROOMY_BOUNDS, false))
            .collect();
        let reply = drive_reconcile(&mut state, &roomy.0, &roomy.1, &rows, Some(token));
        assert!(
            matches!(
                reply,
                tiler_core::boundary::CoreReply::Projection(_)
                    | tiler_core::boundary::CoreReply::Tiled(_)
            ),
            "seed converges, got {reply:?}"
        );
    }
    // Hide with frozen tile snapshots; nothing visible remains.
    state.member_rects.insert("w1".to_owned(), ROOMY_BOUNDS);
    state.member_rects.insert("w2".to_owned(), ROOMY_BOUNDS);
    state.workspaces.set_hidden(&key1, true);
    state.workspaces.set_hidden(&key2, true);
    // An unrelated queued close pulls the hidden domain into the
    // drain; the close itself is unknown and rides harmlessly.
    state.pending_closed.insert(
        "wGhost".to_owned(),
        super::PendingClosed {
            output: output.clone(),
            workspace: hidden.clone(),
        },
    );
    let areas = vec![MonitorArea {
        device: output.clone(),
        work: frame(0, 0, 25, 25),
        full: frame(0, 0, 25, 25),
    }];
    let tight =
        super::workspace_domain_for(&output, &hidden, &areas, state.inner_gap, state.outer_gap)
            .expect("tight domain");
    assert_eq!(tight.0.bounds.w, 9, "shrunken hidden domain");
    super::fanout_pending_closed(&mut state, &me, &areas, &[], &[], "cap-fanout");
    assert!(
        !state.pending_closed.contains_key("wGhost"),
        "queued close still drains"
    );
    let session = state.engine.session(&tight.1).expect("session");
    assert!(
        session.is_capacity_float(&WindowId("w2".to_owned())),
        "newest hidden victim evicts"
    );
    assert!(
        !session.is_capacity_float(&WindowId("w1".to_owned())),
        "oldest hidden survivor stays tiled"
    );
    assert!(state.floated.contains(&key2), "victim lane registers");
    assert_eq!(
        state.float_rects.get("w2").copied(),
        Some(ROOMY_BOUNDS),
        "last live frame retains"
    );
    assert!(
        !state.floated.contains(&key1),
        "survivor takes no float lane"
    );
    clear_pending_seams();
}

#[test]
fn overlap_admission_keeps_tiles_without_float_lanes() {
    // Overlap no-space result through the reconcile path: an exhausted
    // newcomer keeps tiled membership with positive overflowing
    // geometry instead of floating. No capacity origin records, the
    // domain marks overlap-accepted, tiled focus names the newcomer,
    // and the adoption helper finds nothing to register.
    clear_pending_seams();
    let mut state = test_state();
    let me = me();
    state
        .engine
        .set_no_space_result(tiler_core::admission::NoSpaceResult::AllowOverlap);
    let output = "mon-a".to_owned();
    state.workspaces.ensure_output(&output);
    let ws = state.workspaces.active_id(&output).expect("active");
    let (domain, key) = tiny_domain(&output, &ws);
    let key_a = member_key(0x6D1, 1691);
    let key_b = member_key(0x6D2, 1692);
    stage_member(&mut state, &output, &ws, &key_a, "wA", "tag-wa");
    stage_member(&mut state, &output, &ws, &key_b, "wB", "tag-wb");
    let frame_b = frame(2, 2, 5, 5);
    for rows in [
        vec![("wA", TIGHT_BOUNDS, false)],
        vec![("wA", TIGHT_BOUNDS, false), ("wB", frame_b, false)],
    ] {
        let focused = rows.last().map(|(token, _, _)| *token).expect("row");
        let reply = drive_reconcile(&mut state, &domain, &key, &rows, Some(focused));
        assert!(
            matches!(
                reply,
                tiler_core::boundary::CoreReply::Projection(_)
                    | tiler_core::boundary::CoreReply::Tiled(_)
            ),
            "seed converges, got {reply:?}"
        );
    }
    let reply = drive_reconcile(
        &mut state,
        &domain,
        &key,
        &[("wA", TIGHT_BOUNDS, false), ("wB", frame_b, false)],
        Some("wB"),
    );
    let tiled = tiled_tokens(&reply);
    assert_eq!(
        tiled.len(),
        2,
        "both members keep tile geometry, got {tiled:?}"
    );
    let writes = crate::workspace_owner::planned_writes(&reply).expect("plan");
    for write in &writes {
        assert!(
            write.rect.w >= 1 && write.rect.h >= 1,
            "positive extents only, got {:?}",
            write.rect
        );
    }
    assert!(
        writes.iter().any(|write| {
            write.rect.x + write.rect.w > TIGHT_BOUNDS.w
                || write.rect.y + write.rect.h > TIGHT_BOUNDS.h
        }),
        "overflow projects past the domain, got {:?}",
        writes.iter().map(|write| write.rect).collect::<Vec<_>>(),
    );
    let session = state.engine.session(&key).expect("session");
    assert!(
        !session.is_capacity_float(&WindowId("wB".to_owned())),
        "overlap admits never float"
    );
    assert!(
        !session.is_exception(&WindowId("wB".to_owned())),
        "newcomer holds a tile slot"
    );
    assert!(session.overlap_accepted(&key), "domain marks overlap");
    let (_, focus_leaf) = session.focus();
    assert_eq!(
        focus_leaf.map(|leaf| leaf.0.clone()),
        Some("leaf-wB".to_owned()),
        "tiled focus names the newcomer"
    );
    let frames = std::collections::HashMap::new();
    let adopted = super::register_committed_capacity_floats(&mut state, &me, &key, &frames);
    assert!(adopted.is_empty(), "no float lanes engage");
    assert!(!state.floated.contains(&key_b));
    clear_pending_seams();
}

#[test]
fn shrink_protects_fullscreen_newest() {
    // Shrink with the newest observed fullscreen (staged live overlay
    // snapshot through the production stamp): the protected newest
    // never evicts, so the oldest eligible victim floats instead while
    // the protected member keeps projecting.
    clear_pending_seams();
    let mut state = test_state();
    let me = me();
    let output = "mon-a".to_owned();
    state.workspaces.ensure_output(&output);
    let ws = state.workspaces.active_id(&output).expect("active");
    let roomy = crate::workspace_owner::workspace_domain(output.as_str(), &ws, ROOMY_BOUNDS, GAP);
    let (tight, tight_key) = tiny_domain(&output, &ws);
    let keys = [
        member_key(0x621, 1621),
        member_key(0x622, 1622),
        member_key(0x623, 1623),
    ];
    let tokens = ["w1", "w2", "w3"];
    for (i, token) in tokens.iter().enumerate() {
        stage_member(
            &mut state,
            &output,
            &ws,
            &keys[i],
            token,
            &format!("tag-{token}"),
        );
    }
    for (i, token) in tokens.iter().enumerate() {
        let rows: Vec<(&str, Rect, bool)> = tokens[..=i]
            .iter()
            .map(|t| (*t, ROOMY_BOUNDS, false))
            .collect();
        let reply = drive_reconcile(&mut state, &roomy.0, &roomy.1, &rows, Some(token));
        assert!(
            matches!(
                reply,
                tiler_core::boundary::CoreReply::Projection(_)
                    | tiler_core::boundary::CoreReply::Tiled(_)
            ),
            "serial seed converges, got {reply:?}"
        );
    }
    // Live fullscreen overlay on the newest, exactly as the per-tick
    // snapshot stages it for the Engine stamp.
    state.overlay_now.insert("w3".to_owned(), (false, true));
    let rows: Vec<(&str, Rect, bool)> = tokens.iter().map(|t| (*t, ROOMY_BOUNDS, false)).collect();
    let reply = drive_reconcile(&mut state, &tight, &tight_key, &rows, Some("w3"));
    assert!(
        matches!(
            reply,
            tiler_core::boundary::CoreReply::Projection(_)
                | tiler_core::boundary::CoreReply::Tiled(_)
        ),
        "protected shrink repairs, got {reply:?}"
    );
    let session = state.engine.session(&tight_key).expect("session");
    assert!(
        !session.is_capacity_float(&WindowId("w3".to_owned())),
        "protected newest never evicts"
    );
    // The protected survivor consumes the single projectable slot, so
    // both eligible members evict newest-first (w2, then w1).
    for victim in ["w1", "w2"] {
        assert!(
            session.is_capacity_float(&WindowId(victim.to_owned())),
            "{victim} evicts with capacity origin"
        );
    }
    let tiled = tiled_tokens(&reply);
    assert_eq!(
        tiled,
        vec!["w3".to_owned()],
        "protected member alone keeps projecting, got {tiled:?}"
    );
    let mut frames = std::collections::HashMap::new();
    frames.insert("w1".to_owned(), frame(3, 3, 3, 3));
    frames.insert("w2".to_owned(), frame(4, 4, 2, 2));
    let mut adopted =
        super::register_committed_capacity_floats(&mut state, &me, &tight_key, &frames);
    adopted.sort_by(|a, b| a.token.cmp(&b.token));
    assert_eq!(
        adopted.iter().map(|a| a.token.clone()).collect::<Vec<_>>(),
        vec!["w1".to_owned(), "w2".to_owned()],
        "both victims register"
    );
    clear_pending_seams();
}

#[test]
fn staged_candidates_and_foreign_homes_never_register() {
    // A capacity float committed on a discarded candidate never reads
    // as committed on the live Engine; an unknown domain holds
    // nothing; a capacity member homed on another workspace skips
    // under this domain.
    clear_pending_seams();
    let mut state = test_state();
    let me = me();
    let output = "mon-a".to_owned();
    state.workspaces.ensure_output(&output);
    let ws = state.workspaces.active_id(&output).expect("active");
    let (domain, key) = tiny_domain(&output, &ws);
    let key_a = member_key(0x631, 1631);
    let key_b = member_key(0x632, 1632);
    stage_member(&mut state, &output, &ws, &key_a, "wA", "tag-wa");
    stage_member(&mut state, &output, &ws, &key_b, "wB", "tag-wb");
    let reply = drive_reconcile(
        &mut state,
        &domain,
        &key,
        &[("wA", TIGHT_BOUNDS, false)],
        Some("wA"),
    );
    assert!(
        matches!(
            reply,
            tiler_core::boundary::CoreReply::Projection(_)
                | tiler_core::boundary::CoreReply::Tiled(_)
        ),
        "seed converges, got {reply:?}"
    );
    // Commit the float on a throwaway candidate only.
    let owner = state.owner.clone();
    let generation = state.generation.clone();
    let carried = vec![
        (
            WindowId("wA".to_owned()),
            TIGHT_BOUNDS,
            tiler_core::size_hints::WindowSizeHints::none(),
            false,
        ),
        (
            WindowId("wB".to_owned()),
            TIGHT_BOUNDS,
            tiler_core::size_hints::WindowSizeHints::none(),
            false,
        ),
    ];
    let fp = crate::tiling::fingerprint(&[
        ("wA".to_owned(), TIGHT_BOUNDS),
        ("wB".to_owned(), TIGHT_BOUNDS),
    ]);
    let event = crate::tiling::build_reconcile_event_for_floating(
        &owner,
        &generation,
        &CorrelationId::parse("staged").expect("correlation"),
        committed_revision(&state, &key),
        fp,
        &domain,
        &key,
        state.outer_gap,
        &carried,
        Some(&WindowId("wA".to_owned())),
    );
    let mut candidate = state.engine.clone();
    let candidate_reply = candidate.handle(&event);
    assert!(
        matches!(
            candidate_reply,
            tiler_core::boundary::CoreReply::Projection(_)
                | tiler_core::boundary::CoreReply::Tiled(_)
        ),
        "candidate converges, got {candidate_reply:?}"
    );
    assert!(
        candidate
            .session(&key)
            .is_some_and(|s| s.is_capacity_float(&WindowId("wB".to_owned()))),
        "candidate floats the newcomer"
    );
    assert!(
        !state
            .engine
            .session(&key)
            .is_some_and(|s| s.is_exception(&WindowId("wB".to_owned()))),
        "live engine never saw the staged float"
    );
    let frames = std::collections::HashMap::new();
    let adopted = super::register_committed_capacity_floats(&mut state, &me, &key, &frames);
    assert!(adopted.is_empty(), "staged-only floats persist nothing");
    // Unknown domain: no session, no registration.
    let (other_domain, other_key) =
        crate::workspace_owner::workspace_domain(&output, "ws-nope", TIGHT_BOUNDS, GAP);
    let _ = other_domain;
    let adopted = super::register_committed_capacity_floats(&mut state, &me, &other_key, &frames);
    assert!(adopted.is_empty(), "unknown domain registers nothing");
    // Commit the float live, then rehome the member elsewhere without
    // touching the Engine: registration under the committed domain
    // skips the foreign-homed member.
    let reply = drive_reconcile(
        &mut state,
        &domain,
        &key,
        &[("wA", TIGHT_BOUNDS, false), ("wB", TIGHT_BOUNDS, false)],
        Some("wA"),
    );
    assert!(
        matches!(
            reply,
            tiler_core::boundary::CoreReply::Projection(_)
                | tiler_core::boundary::CoreReply::Tiled(_)
        ),
        "live float commits, got {reply:?}"
    );
    let ids = state.workspaces.workspace_ids(&output);
    let elsewhere = ids.iter().find(|id| *id != &ws).expect("second ws").clone();
    assert!(
        state
            .workspaces
            .assign(key_b.clone(), &output, &elsewhere, false),
        "rehome stages"
    );
    let adopted = super::register_committed_capacity_floats(&mut state, &me, &key, &frames);
    assert!(
        adopted.is_empty(),
        "foreign-homed members skip under this domain"
    );
    clear_pending_seams();
}
#[test]
fn shrink_evicts_newest_first_victim() {
    // Three tiles seeded roomy, then shrunk below any 3-leaf
    // projection: exactly the newest admission evicts with capacity
    // origin while the two oldest keep projecting tiles.
    clear_pending_seams();
    let mut state = test_state();
    let me = me();
    let output = "mon-a".to_owned();
    state.workspaces.ensure_output(&output);
    let ws = state.workspaces.active_id(&output).expect("active");
    let roomy = crate::workspace_owner::workspace_domain(output.as_str(), &ws, ROOMY_BOUNDS, GAP);
    let (tight, tight_key) = tiny_domain(&output, &ws);
    assert_eq!(roomy.1, tight_key, "same domain across the shrink");
    let keys = [
        member_key(0x611, 1611),
        member_key(0x612, 1612),
        member_key(0x613, 1613),
    ];
    let tokens = ["w1", "w2", "w3"];
    for (i, token) in tokens.iter().enumerate() {
        stage_member(
            &mut state,
            &output,
            &ws,
            &keys[i],
            token,
            &format!("tag-{token}"),
        );
    }
    for (i, token) in tokens.iter().enumerate() {
        let rows: Vec<(&str, Rect, bool)> = tokens[..=i]
            .iter()
            .map(|t| (*t, ROOMY_BOUNDS, false))
            .collect();
        let reply = drive_reconcile(&mut state, &roomy.0, &roomy.1, &rows, Some(token));
        assert!(
            matches!(
                reply,
                tiler_core::boundary::CoreReply::Projection(_)
                    | tiler_core::boundary::CoreReply::Tiled(_)
            ),
            "serial seed converges, got {reply:?}"
        );
        let tiled = tiled_tokens(&reply);
        for want in &tokens[..=i] {
            assert!(
                tiled.contains(&(*want).to_owned()),
                "{want} tiles roomy, got {tiled:?}"
            );
        }
    }
    let order: Vec<Option<u64>> = tokens
        .iter()
        .map(|token| {
            state
                .engine
                .session(&tight_key)
                .and_then(|s| s.admission_sequence(&WindowId((*token).to_owned())))
        })
        .collect();
    assert_eq!(
        order,
        vec![Some(0), Some(1), Some(2)],
        "first-successful order tracks serial admission"
    );
    let rows: Vec<(&str, Rect, bool)> = tokens.iter().map(|t| (*t, ROOMY_BOUNDS, false)).collect();
    let reply = drive_reconcile(&mut state, &tight, &tight_key, &rows, Some("w3"));
    assert!(
        matches!(
            reply,
            tiler_core::boundary::CoreReply::Projection(_)
                | tiler_core::boundary::CoreReply::Tiled(_)
        ),
        "shrink repairs, got {reply:?}"
    );
    let session = state.engine.session(&tight_key).expect("session");
    assert!(
        session.is_capacity_float(&WindowId("w3".to_owned())),
        "newest evicts with capacity origin"
    );
    assert!(
        session.is_capacity_float(&WindowId("w2".to_owned())),
        "second-newest evicts next until the tree projects"
    );
    let tiled = tiled_tokens(&reply);
    assert_eq!(
        tiled,
        vec!["w1".to_owned()],
        "oldest alone survives, got {tiled:?}"
    );
    let mut frames = std::collections::HashMap::new();
    frames.insert("w2".to_owned(), frame(1, 1, 4, 4));
    frames.insert("w3".to_owned(), frame(2, 2, 5, 5));
    let mut adopted =
        super::register_committed_capacity_floats(&mut state, &me, &tight_key, &frames);
    adopted.sort_by(|a, b| a.token.cmp(&b.token));
    assert_eq!(
        adopted.iter().map(|a| a.token.clone()).collect::<Vec<_>>(),
        vec!["w2".to_owned(), "w3".to_owned()],
        "both victims register"
    );
    // Eviction keeps the last live frame (D4): the Engine-retained
    // pre-shrink observation wins over the caller-carried frames.
    for token in ["w2", "w3"] {
        let key = keys[tokens.iter().position(|t| *t == token).expect("key")].clone();
        assert!(state.floated.contains(&key), "{token} joins floated");
        assert_eq!(
            state.float_rects.get(token).copied(),
            Some(ROOMY_BOUNDS),
            "{token} keeps its last live frame"
        );
    }
    // Stable replay: the same observation changes nothing and
    // registers nothing further.
    let revision = committed_revision(&state, &tight_key);
    let reply = drive_reconcile(&mut state, &tight, &tight_key, &rows, Some("w2"));
    assert!(
        matches!(
            reply,
            tiler_core::boundary::CoreReply::Projection(_)
                | tiler_core::boundary::CoreReply::Tiled(_)
        ),
        "replay converges, got {reply:?}"
    );
    assert_eq!(
        committed_revision(&state, &tight_key),
        revision,
        "replay bumps no revision"
    );
    let again = super::register_committed_capacity_floats(&mut state, &me, &tight_key, &frames);
    assert!(again.is_empty(), "replay registers nothing");
    clear_pending_seams();
}

#[test]
fn exhausted_send_stages_move_to_float() {
    // Same-output send into an exhausted destination through the
    // production send-event builder plus the real Engine: the sole
    // source mover finds no splittable target leaf, so it commits
    // slotless at the target with the verbatim live frame
    // (`MoveToFloat`) while the emptied source retires. The reply
    // classifies as a capacity-float transfer; tiled admission and
    // refusal replies classify false.
    use tiler_core::boundary::CoreReply;
    clear_pending_seams();
    let mut state = test_state();
    let me = me();
    let output = "mon-a".to_owned();
    state.workspaces.ensure_output(&output);
    let ids = state.workspaces.workspace_ids(&output);
    let (source, target) = (ids[0].clone(), ids[1].clone());
    let (source_domain, source_key) =
        crate::workspace_owner::workspace_domain(&output, &source, TIGHT_BOUNDS, GAP);
    let (target_domain, target_key) =
        crate::workspace_owner::workspace_domain(&output, &target, TIGHT_BOUNDS, GAP);
    // Single-tile domains: each admits its lone member, while the
    // mover is the exhausting second leaf at the destination.
    let live_frame = frame(1, 1, 7, 7);
    let source_rows = [crate::workspace_owner::OwnerRow {
        token: "wM".to_owned(),
        rect: live_frame,
        hints: tiler_core::size_hints::WindowSizeHints::none(),
        floating: false,
    }];
    let target_rows = [crate::workspace_owner::OwnerRow {
        token: "wT".to_owned(),
        rect: TIGHT_BOUNDS,
        hints: tiler_core::size_hints::WindowSizeHints::none(),
        floating: false,
    }];
    for (domain, key, rows, focused) in [
        (&source_domain, &source_key, &source_rows, "wM"),
        (&target_domain, &target_key, &target_rows, "wT1"),
    ] {
        let carried: Vec<(
            WindowId,
            Rect,
            tiler_core::size_hints::WindowSizeHints,
            bool,
        )> = rows
            .iter()
            .map(|row| {
                (
                    WindowId(row.token.clone()),
                    row.rect,
                    row.hints,
                    row.floating,
                )
            })
            .collect();
        let revision = committed_revision(&state, key);
        let fp = crate::tiling::fingerprint(
            &carried
                .iter()
                .map(|(w, r, _, _)| (w.0.clone(), *r))
                .collect::<Vec<_>>(),
        );
        let event = crate::tiling::build_reconcile_event_for_floating(
            &state.owner.clone(),
            &state.generation.clone(),
            &CorrelationId::parse("send-seed").expect("correlation"),
            revision,
            fp,
            domain,
            key,
            state.outer_gap,
            &carried,
            Some(&WindowId(focused.to_owned())),
        );
        let reply = state.engine.handle(&event);
        assert!(
            matches!(reply, CoreReply::Projection(_) | CoreReply::Tiled(_)),
            "seed converges, got {reply:?}"
        );
    }
    let mut pairs: Vec<(String, Rect)> = source_rows
        .iter()
        .chain(target_rows.iter())
        .map(|row| (row.token.clone(), row.rect))
        .collect();
    pairs.sort_by(|a, b| a.0.cmp(&b.0));
    let fp = crate::tiling::fingerprint(
        &pairs
            .iter()
            .map(|(t, r)| (t.clone(), *r))
            .collect::<Vec<_>>(),
    );
    let mut event = crate::workspace_owner::build_send_event(
        &state.owner.clone(),
        &state.generation.clone(),
        &CorrelationId::parse("send-float").expect("correlation"),
        committed_revision(&state, &source_key),
        fp,
        (source_domain.clone(), source_key.clone()),
        (target_domain.clone(), target_key.clone()),
        &source_rows,
        &target_rows,
        "wM",
        state.outer_gap,
        true,
    )
    .expect("send builds");
    crate::workspace_owner::stamp_send_target(&mut event, &target_key);
    let reply = state.engine.handle(&event);
    let plan = match &reply {
        CoreReply::SendWorkspace(plan) => plan,
        other => panic!("send commits, got {other:?}"),
    };
    assert!(
        matches!(
            plan.operation,
            tiler_core::contract::LifecycleOperation::MoveToFloat { .. }
        ),
        "mover floats slotless at the named destination"
    );
    assert!(
        super::reply_is_capacity_float_transfer(&reply),
        "reply classifies as a capacity-float transfer"
    );
    let geometry: Vec<String> = plan
        .geometry
        .iter()
        .map(|entry| entry.window.0.clone())
        .collect();
    assert!(
        geometry.contains(&"wT".to_owned()),
        "destination survivor still projects, got {geometry:?}"
    );
    assert!(
        !geometry.contains(&"wM".to_owned()),
        "mover rides no tile geometry"
    );
    let target_session = state.engine.session(&target_key).expect("target");
    assert!(
        target_session.is_exception(&WindowId("wM".to_owned())),
        "mover rehomes as a target exception"
    );
    assert!(
        target_session.is_capacity_float(&WindowId("wM".to_owned())),
        "mover carries capacity origin"
    );
    assert_eq!(
        target_session.retained_float_geometry(&WindowId("wM".to_owned())),
        Some(live_frame),
        "verbatim live frame retains"
    );
    // The emptied source retires its session; a surviving source would
    // drop the mover instead.
    assert!(
        state.engine.session(&source_key).is_none_or(|s| {
            !s.is_exception(&WindowId("wM".to_owned()))
                && !s.is_capacity_float(&WindowId("wM".to_owned()))
        }),
        "emptied source retires"
    );
    // Registration prefers the Engine-retained live frame over the
    // caller-carried frame. Member tables ride the destination.
    let mover_key = member_key(0x641, 1641);
    assert!(
        state
            .workspaces
            .assign(mover_key.clone(), &output, &target, false)
    );
    state
        .member_tokens
        .insert(mover_key.clone(), "wM".to_owned());
    let mut frames = std::collections::HashMap::new();
    frames.insert("wM".to_owned(), frame(0, 0, 1, 1));
    let adopted = super::register_committed_capacity_floats(&mut state, &me, &target_key, &frames);
    assert_eq!(adopted.len(), 1, "mover registers");
    assert_eq!(
        state.float_rects.get("wM").copied(),
        Some(live_frame),
        "retained live frame wins"
    );
    assert!(state.floated.contains(&mover_key));
    // Negative shapes classify false.
    assert!(
        !super::reply_is_capacity_float_transfer(&CoreReply::Rejected {
            kind: "x",
            message: "y",
        }),
        "refusals never classify"
    );
    let seed_reply = drive_reconcile(
        &mut state,
        &target_domain,
        &target_key,
        &[("wT", TIGHT_BOUNDS, false), ("wM", live_frame, true)],
        Some("wT"),
    );
    assert!(
        !super::reply_is_capacity_float_transfer(&seed_reply),
        "ordinary plans never classify"
    );
    clear_pending_seams();
}

#[test]
fn exhausted_send_transfers_without_proof_persistence() {
    // Same-output capacity-float send through the real
    // `workspace_do_send` route on a single-tile monitor: the Engine
    // commits the slotless transfer and membership follows, but the
    // fake-HWND hide tail cannot prove the transfer, so no ordinary
    // float lane persists yet. The post-proof registration then adopts
    // the committed mover exactly once.
    for follow in [false, true] {
        clear_pending_seams();
        let mut state = test_state();
        let me = me();
        let tmp = std::env::temp_dir().join(format!(
            "tiler-cap-send-{follow}-{}-{}",
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
        let (source, target) = (ids[0].clone(), ids[1].clone());
        state.workspaces.activate(&output, &source);
        state.workspaces.set_tiled(&output, &source, true);
        state.workspaces.set_tiled(&output, &target, true);
        // Single-tile monitor: inset(25, 8) == 9, so each lone member
        // tiles while the mover is the exhausting second leaf.
        let areas = vec![MonitorArea {
            device: output.clone(),
            work: frame(0, 0, 25, 25),
            full: frame(0, 0, 25, 25),
        }];
        let fulls = vec![frame(0, 0, 25, 25)];
        let (source_domain, source_key) =
            super::workspace_domain_for(&output, &source, &areas, state.inner_gap, state.outer_gap)
                .expect("source domain");
        assert_eq!(
            source_domain.bounds,
            Rect {
                x: 8,
                y: 8,
                w: 9,
                h: 9
            },
            "single-tile domain"
        );
        let (target_domain, target_key) =
            super::workspace_domain_for(&output, &target, &areas, state.inner_gap, state.outer_gap)
                .expect("target domain");
        let mover = member_key(0x651, 1651);
        let target_member = member_key(0x652, 1652);
        stage_member(&mut state, &output, &source, &mover, "wM", "tag-wm");
        stage_member(&mut state, &output, &target, &target_member, "wT", "tag-wt");
        let frame_m = frame(9, 9, 7, 7);
        let frame_t = source_domain.bounds;
        for (domain, key, token, rect) in [
            (&source_domain, &source_key, "wM", frame_m),
            (&target_domain, &target_key, "wT", frame_t),
        ] {
            let reply = drive_reconcile(
                &mut state,
                domain,
                key,
                &[(token, rect, false)],
                Some(token),
            );
            assert!(
                matches!(
                    reply,
                    tiler_core::boundary::CoreReply::Projection(_)
                        | tiler_core::boundary::CoreReply::Tiled(_)
                ),
                "seed converges, got {reply:?}"
            );
            assert_eq!(tiled_tokens(&reply), vec![token.to_owned()]);
        }
        let mut observed = vec![
            observed_window(&mover, "wM", frame_m),
            observed_window(&target_member, "wT", frame_t),
        ];
        super::inject_pending_observe(observed.clone(), Vec::new(), false);
        let ctx = ActionCtx {
            correlation: "cap-send-1".to_owned(),
            tick: state.tick,
            queued_at: std::time::Instant::now(),
            start: std::time::Instant::now(),
        };
        let effect = super::workspace_do_send(
            &mut state,
            &me,
            &store,
            &tmp,
            &fulls,
            &areas,
            &mut observed,
            &[],
            "mon-a",
            SendTarget::Relative(1),
            follow,
            mover.hwnd,
            "wM",
            mover.pid,
            &mover.creation,
            &ctx,
        );
        let shape = format!("follow={follow}");
        assert_ne!(effect.outcome, "ok", "{shape}: hide stays unproven");
        assert_eq!(
            state
                .workspaces
                .member_loc(&mover)
                .map(|loc| loc.workspace.clone()),
            Some(target.clone()),
            "{shape}: membership transfers"
        );
        let target_session = state.engine.session(&target_key).expect("target");
        assert!(
            target_session.is_exception(&WindowId("wM".to_owned())),
            "{shape}: mover rehomes as a target exception"
        );
        assert!(
            target_session.is_capacity_float(&WindowId("wM".to_owned())),
            "{shape}: mover carries capacity origin"
        );
        assert!(
            !state.floated.contains(&mover),
            "{shape}: no lane persists before proof"
        );
        assert!(
            !state.float_rects.contains_key("wM"),
            "{shape}: no frame persists before proof"
        );
        let counters = pending_counters();
        assert!(
            counters.apply >= 1,
            "{shape}: source leg still runs, got {counters:?}"
        );
        // Simulated arrival proof: the committed mover adopts once with
        // its live frame.
        let mut frames = std::collections::HashMap::new();
        frames.insert("wM".to_owned(), frame_m);
        let adopted =
            super::register_committed_capacity_floats(&mut state, &me, &target_key, &frames);
        assert_eq!(adopted.len(), 1, "{shape}: mover adopts after proof");
        assert!(state.floated.contains(&mover));
        assert_eq!(state.float_rects.get("wM").copied(), Some(frame_m));
        let _ = std::fs::remove_dir_all(&tmp);
        clear_pending_seams();
    }
}

#[test]
fn capacity_float_survives_owner_restart_as_ordinary_float() {
    // D5 offline roundtrip with scripted native windows only: a live
    // owner commits a capacity float and persists the ordinary intent
    // marker; a fresh owner with no Engine state re-adopts the marked
    // window as an ordinary float at its live frame while the tiled
    // survivor keeps its exact allocation. The capacity origin does
    // not survive; unmarked, corrupt, and stale-lifetime windows adopt
    // no lane.
    clear_pending_seams();
    fake::clear();
    let me = me();
    let output = "mon-a".to_owned();
    let areas = vec![MonitorArea {
        device: output.clone(),
        work: frame(0, 0, 25, 25),
        full: frame(0, 0, 25, 25),
    }];
    // Live owner: single-tile domain, wA tiled, wB capacity-floated,
    // marker installed through the real helper (holds included).
    let mut state = test_state();
    state.workspaces.ensure_output(&output);
    let ws = state.workspaces.active_id(&output).expect("active");
    let (domain, key) =
        super::workspace_domain_for(&output, &ws, &areas, state.inner_gap, state.outer_gap)
            .expect("domain");
    assert_eq!(domain.bounds.w, 9, "single-tile domain");
    let key_a = member_key(0x6B1, 1691);
    let key_b = member_key(0x6B2, 1692);
    stage_member(&mut state, &output, &ws, &key_a, "wA", "tag-wa");
    stage_member(&mut state, &output, &ws, &key_b, "wB", "tag-wb");
    let live_b = frame(9, 9, 7, 7);
    for rows in [
        vec![("wA", domain.bounds, false)],
        vec![("wA", domain.bounds, false), ("wB", live_b, false)],
    ] {
        let focused = rows.last().map(|(token, _, _)| *token).expect("row");
        let reply = drive_reconcile(&mut state, &domain, &key, &rows, Some(focused));
        assert!(
            matches!(
                reply,
                tiler_core::boundary::CoreReply::Projection(_)
                    | tiler_core::boundary::CoreReply::Tiled(_)
            ),
            "seed converges, got {reply:?}"
        );
    }
    register_fake(&key_b, "tag-wb");
    let mut frames = std::collections::HashMap::new();
    frames.insert("wB".to_owned(), live_b);
    let adopted = super::register_committed_capacity_floats(&mut state, &me, &key, &frames);
    assert_eq!(adopted.len(), 1, "live float registers");
    assert!(adopted[0].marker_ok, "marker persists on the window");
    let survivor_reply = drive_reconcile(
        &mut state,
        &domain,
        &key,
        &[("wA", domain.bounds, false), ("wB", live_b, true)],
        Some("wA"),
    );
    let survivor_rect = crate::workspace_owner::planned_writes(&survivor_reply)
        .expect("plan")
        .iter()
        .find(|write| write.window.0 == "wA")
        .map(|write| write.rect)
        .expect("survivor projects");
    // Fresh owner: same members, same tags, same scripted windows (the
    // fake property table outlives the owner like glass), but no
    // Engine sessions and no lanes. wC rides unmarked, wD corrupt, wE
    // under a replaced lifetime tag.
    let mut fresh = test_state();
    fresh.workspaces.ensure_output(&output);
    let fresh_ws = fresh.workspaces.active_id(&output).expect("active");
    let (fresh_domain, fresh_key) =
        super::workspace_domain_for(&output, &fresh_ws, &areas, fresh.inner_gap, fresh.outer_gap)
            .expect("domain");
    assert_eq!(fresh_key, key, "stable domain identity");
    assert_eq!(fresh_domain.bounds, domain.bounds, "stable domain bounds");
    let key_c = member_key(0x6B3, 1693);
    let key_d = member_key(0x6B4, 1694);
    let key_e = member_key(0x6B5, 1695);
    stage_member(&mut fresh, &output, &fresh_ws, &key_a, "wA", "tag-wa");
    stage_member(&mut fresh, &output, &fresh_ws, &key_b, "wB", "tag-wb");
    stage_member(&mut fresh, &output, &fresh_ws, &key_c, "wC", "tag-wc");
    stage_member(&mut fresh, &output, &fresh_ws, &key_d, "wD", "tag-wd");
    stage_member(&mut fresh, &output, &fresh_ws, &key_e, "wE", "tag-we-fresh");
    register_fake(&key_a, "tag-wa");
    register_fake(&key_b, "tag-wb");
    register_fake(&key_d, "tag-wd");
    register_fake(&key_e, "tag-we-stale");
    // wD carries a corrupt marker value; wC reads honestly absent.
    crate::product_hide::sys::fake::store_float_mark(key_d.hwnd, 999);
    let observed = vec![
        observed_window(&key_a, "wA", domain.bounds),
        observed_window(&key_b, "wB", live_b),
        observed_window(&key_c, "wC", domain.bounds),
        observed_window(&key_d, "wD", domain.bounds),
        observed_window(&key_e, "wE", domain.bounds),
    ];
    let mut hint_cx = super::HintCx::new();
    super::adopt_restart_markers(&mut fresh, &me, &observed, &[], &areas, &mut hint_cx);
    let session = fresh.engine.session(&fresh_key).expect("fresh session");
    // The marked window floats as an ordinary float at its live frame;
    // the diagnostic origin stays behind with the old owner.
    assert!(
        session.is_exception(&WindowId("wB".to_owned())),
        "marked window re-adopts floating"
    );
    assert!(
        !session.is_capacity_float(&WindowId("wB".to_owned())),
        "capacity origin does not persist"
    );
    assert!(fresh.floated.contains(&key_b), "ordinary lane rejoins");
    assert_eq!(
        fresh.float_rects.get("wB").copied(),
        Some(live_b),
        "original frame restores"
    );
    // The tiled survivor keeps its exact allocation.
    let retiled = drive_reconcile(
        &mut fresh,
        &fresh_domain,
        &fresh_key,
        &[
            ("wA", domain.bounds, false),
            ("wB", live_b, true),
            ("wC", domain.bounds, true),
            ("wD", domain.bounds, true),
            ("wE", domain.bounds, true),
        ],
        Some("wA"),
    );
    let retiled_rect = crate::workspace_owner::planned_writes(&retiled)
        .expect("plan")
        .iter()
        .find(|write| write.window.0 == "wA")
        .map(|write| write.rect);
    assert_eq!(
        retiled_rect,
        Some(survivor_rect),
        "survivor allocation unchanged"
    );
    // Unmarked, corrupt, and stale windows adopt no lane.
    for (key, name) in [(&key_c, "wC"), (&key_d, "wD"), (&key_e, "wE")] {
        assert!(!fresh.floated.contains(key), "{name} adopts no float lane");
        assert!(
            !fresh.float_rects.contains_key(name),
            "{name} adopts no frame"
        );
    }
    fake::clear();
    clear_pending_seams();
}

#[test]
fn gap_growth_repairs_newest_then_ordinary_adoption_backstops() {
    // Producer-shaped D7 gap growth through the real seams: two tiles
    // seeded at gap 8, then a pure `adopt_gaps_for_route` UpdateGaps with
    // a grown inner gap. The retained tree no longer projects, so the
    // newest admission evicts with capacity origin through the real
    // Engine event; the next ordinary helper then backstops the ordinary
    // lanes plus the marker attempt. Native-free: unregistered HWNDs
    // fail the marker closed while the lanes still register.
    clear_pending_seams();
    fake::clear();
    let mut state = test_state();
    let me = me();
    let output = "mon-a".to_owned();
    state.workspaces.ensure_output(&output);
    let ws = state.workspaces.active_id(&output).expect("active");
    let bounds = frame(0, 0, 30, 30);
    let key_a = member_key(0x6E1, 1691);
    let key_b = member_key(0x6E2, 1692);
    stage_member(&mut state, &output, &ws, &key_a, "wA", "tag-wa");
    stage_member(&mut state, &output, &ws, &key_b, "wB", "tag-wb");
    let (small_domain, key) = crate::workspace_owner::workspace_domain(&output, &ws, bounds, GAP);
    for rows in [
        vec![("wA", bounds, false)],
        vec![("wA", bounds, false), ("wB", bounds, false)],
    ] {
        let reply = drive_reconcile(&mut state, &small_domain, &key, &rows, Some("wA"));
        assert!(
            matches!(
                reply,
                tiler_core::boundary::CoreReply::Projection(_)
                    | tiler_core::boundary::CoreReply::Tiled(_)
            ),
            "gap-8 seed converges, got {reply:?}"
        );
    }
    let session = state.engine.session(&key).expect("session");
    assert!(
        !session.is_capacity_float(&WindowId("wB".to_owned())),
        "both tiles project at gap 8"
    );
    assert_eq!(
        tiled_tokens(&drive_reconcile(
            &mut state,
            &small_domain,
            &key,
            &[("wA", bounds, false), ("wB", bounds, false)],
            Some("wA"),
        ))
        .len(),
        2,
        "gap-8 roster keeps both tiles"
    );
    // Grow only the inner gap: 30-wide bounds project two leaves at gap
    // 8 (needs >= 10) but not at gap 32 (needs >= 34).
    state.inner_gap = 32;
    let (grown_domain, grown_key) =
        crate::workspace_owner::workspace_domain(&output, &ws, bounds, state.inner_gap);
    assert_eq!(grown_key, key, "gap growth keeps domain identity");
    let windows = vec![
        (
            WindowId("wA".to_owned()),
            bounds,
            tiler_core::size_hints::WindowSizeHints::none(),
            false,
        ),
        (
            WindowId("wB".to_owned()),
            bounds,
            tiler_core::size_hints::WindowSizeHints::none(),
            false,
        ),
    ];
    let revision = committed_revision(&state, &key);
    let fp = crate::tiling::fingerprint(&[("wA".to_owned(), bounds), ("wB".to_owned(), bounds)]);
    let correlation = CorrelationId::parse("gap-growth").expect("correlation");
    let outer_gap = state.outer_gap;
    assert!(
        super::adopt_gaps_for_route(
            &mut state,
            &grown_domain,
            &key,
            outer_gap,
            &windows,
            Some(&WindowId("wA".to_owned())),
            revision,
            fp,
            &correlation,
        ),
        "grown gaps adopt through UpdateGaps"
    );
    let session = state.engine.session(&key).expect("session");
    assert!(
        session.is_capacity_float(&WindowId("wB".to_owned())),
        "newest evicts with capacity origin on gap growth"
    );
    assert!(
        session.is_exception(&WindowId("wB".to_owned())),
        "victim floats slotless"
    );
    assert!(
        !session.is_capacity_float(&WindowId("wA".to_owned())),
        "oldest survivor stays tiled"
    );
    // Next ordinary helper backstops the lanes plus the marker attempt:
    // no scripted window, so the marker fails closed while the lanes
    // still register with the Engine-retained live frame.
    let mut frames = std::collections::HashMap::new();
    frames.insert("wB".to_owned(), frame(2, 2, 5, 5));
    let adopted = super::register_committed_capacity_floats(&mut state, &me, &key, &frames);
    assert_eq!(adopted.len(), 1, "gap victim registers");
    assert_eq!(adopted[0].token, "wB");
    assert!(!adopted[0].marker_ok, "unscripted window fails closed");
    assert!(state.floated.contains(&key_b), "victim joins floated");
    assert_eq!(
        state.float_rects.get("wB").copied(),
        Some(bounds),
        "Engine-retained live frame wins"
    );
    assert!(!state.floated.contains(&key_a), "survivor takes no lane");
    let again = super::register_committed_capacity_floats(&mut state, &me, &key, &frames);
    assert!(again.is_empty(), "replay registers nothing");
    fake::clear();
    clear_pending_seams();
}

#[test]
fn stale_pid_or_tag_refuses_float_marker_install() {
    // Held-gate negatives through the real `install_float_mark_held`:
    // a recycled HWND (live pid moved on) and a stale lifetime tag
    // (same-process reuse) both refuse with no marker write, while a
    // healthy fake installs and reads back. A scripted high-integrity
    // target refuses the same way on every runner. Native-free fake harness.
    clear_pending_seams();
    fake::clear();
    let mut state = test_state();
    let me = me();
    let output = "mon-a".to_owned();
    state.workspaces.ensure_output(&output);
    let ws = state.workspaces.active_id(&output).expect("active");
    let key_ok = member_key(0x6F1, 1691);
    let key_pid = member_key(0x6F2, 1692);
    let key_tag = member_key(0x6F3, 1693);
    let key_high = member_key(0x6F4, 1694);
    stage_member(&mut state, &output, &ws, &key_ok, "wOk", "tag-ok");
    stage_member(&mut state, &output, &ws, &key_pid, "wPid", "tag-pid");
    stage_member(&mut state, &output, &ws, &key_tag, "wTag", "tag-fresh");
    stage_member(&mut state, &output, &ws, &key_high, "wHigh", "tag-high");
    register_fake(&key_ok, "tag-ok");
    register_fake(&key_pid, "tag-pid");
    register_fake(&key_high, "tag-high");
    // Elevated scripted target: every other fact matches, only the
    // integrity RID sits outside the medium band.
    fake::with_mut(key_high.hwnd, |w| {
        w.integrity_rid = crate::lifecycle::MEDIUM_RID_MAX;
    });
    // Same-process reuse: the HWND still hosts the stored pid/creation
    // but the live lifetime property moved on.
    register_fake(&key_tag, "tag-stale");
    assert!(
        super::install_float_mark_held(&state, &me, &key_ok),
        "healthy fake installs"
    );
    assert!(
        crate::product_hide::sys::read_float_intent_marker(key_ok.hwnd),
        "healthy marker reads back"
    );
    // Recycled HWND: the number now hosts a foreign pid.
    fake::with_mut(key_pid.hwnd, |w| w.pid = 9999);
    assert!(
        !super::install_float_mark_held(&state, &me, &key_pid),
        "recycled pid refuses"
    );
    assert!(
        !crate::product_hide::sys::read_float_intent_marker(key_pid.hwnd),
        "recycled pid writes no marker"
    );
    assert!(
        !super::install_float_mark_held(&state, &me, &key_tag),
        "stale lifetime tag refuses"
    );
    assert!(
        !crate::product_hide::sys::read_float_intent_marker(key_tag.hwnd),
        "stale tag writes no marker"
    );
    assert!(
        !super::install_float_mark_held(&state, &me, &key_high),
        "high-integrity target refuses"
    );
    assert!(
        !crate::product_hide::sys::read_float_intent_marker(key_high.hwnd),
        "high-integrity target writes no marker"
    );
    fake::clear();
    clear_pending_seams();
}

#![forbid(unsafe_code)]
// Oracle-first scaled proxy for fresh exhaustion, retry and same-output sends.
// A local binary split needs long_edge >= gap + 2; whole-tree feasibility
// still comes from Engine, not a guessed count or declared minimum threshold.
// These native-free seams do not establish a practical native helper count.
use super::rmax03_adapter_tests::test_state;
use super::{MonitorArea, clear_pending_seams, inject_pending_apply, inject_pending_observe};
use tiler_core::directional::WindowId;
use tiler_core::geometry::Rect;
use tiler_core::ids::CorrelationId;

/// Proxy domain with room for a handful of tiles at gap 32: small enough
/// for a fast offline oracle, large enough that exhaustion runs the full
/// D2 largest-splittable search. Distinct from the 9x9/gap-8 tiny domain
/// owned by `tests_capacity_floats.rs` (untouched here).
const PROXY_BOUNDS: Rect = Rect {
    x: 0,
    y: 0,
    w: 70,
    h: 70,
};
const PROXY_GAP: i32 = 32;
/// Realistic full-size shape: 1920x1040 work inset by outer 64.
/// Written literally so the budget test needs no desktop read.
const REAL_WORK: Rect = Rect {
    x: 0,
    y: 0,
    w: 1920,
    h: 1040,
};
const REAL_OUTER: i32 = 64;
const REAL_INNER: i32 = 64;
/// Monitor work area paired with the proxy domain: inset by the proxy
/// outer gap 32 it reproduces `PROXY_BOUNDS` exactly (mirrors the
/// 25/8 pairing behind the 9x9 tiny domain), so the real
/// `apply_unfloat_to_tiled` route resolves the same domain the oracle
/// seeded. Never a desktop read.
const PROXY_AREA: Rect = Rect {
    x: -32,
    y: -32,
    w: 134,
    h: 134,
};
/// Oracle ceiling for the proxy domain (area math bounds exhaustion far
/// below this; hitting the ceiling fails loudly instead of guessing).
const PROXY_MAX: usize = 40;

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
    super::inject_pending_tag(key.hwnd, Some(tag.to_owned()));
}

fn proxy_domain(
    output: &str,
    workspace: &str,
) -> (
    tiler_core::session::OutputDomain,
    tiler_core::session::DomainKey,
) {
    crate::workspace_owner::workspace_domain(output, workspace, PROXY_BOUNDS, PROXY_GAP)
}

fn committed_revision(state: &super::TileLoop, key: &tiler_core::session::DomainKey) -> u64 {
    state
        .engine
        .session(key)
        .map(|session| session.accepted_revision())
        .unwrap_or(0)
}

/// Drive one Reconcile through the production adapter seam over explicit
/// rows (serial producer order: callers pass the full roster with the
/// newest last and focus it, matching live helper arrival).
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
        &CorrelationId::parse("capacity-recipe").expect("correlation"),
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

fn tiled_tokens(reply: &tiler_core::boundary::CoreReply) -> Vec<String> {
    crate::workspace_owner::planned_writes(reply)
        .unwrap_or_default()
        .iter()
        .map(|write| write.window.0.clone())
        .collect()
}

/// Serial producer fill on one proxy domain: stage `tokens` with live
/// frames, admit one per pass with focus tracking the newest arrival, and
/// return the oracle `(victim_index, reply)` at the first pass where the
/// newcomer commits as a capacity float. `None` when `tokens` all tile
/// (caller treats that as budget evidence, never as exhaustion).
#[allow(clippy::too_many_arguments)]
fn fill_until_float(
    state: &mut super::TileLoop,
    domain: &tiler_core::session::OutputDomain,
    key: &tiler_core::session::DomainKey,
    output: &str,
    ws: &str,
    tokens: &[String],
    keys: &[crate::workspace::WindowKey],
    frames: &[Rect],
) -> Option<(usize, tiler_core::boundary::CoreReply)> {
    for n in 1..=tokens.len() {
        // Lazy producer staging: the newcomer joins membership exactly on
        // its arrival pass, like a live helper spawn (never pre-staged).
        stage_member(
            &mut *state,
            output,
            ws,
            &keys[n - 1],
            &tokens[n - 1],
            &format!("tag-{}", tokens[n - 1]),
        );
        let rows: Vec<(&str, Rect, bool)> = tokens[..n]
            .iter()
            .zip(frames[..n].iter())
            .map(|(token, rect)| (token.as_str(), *rect, false))
            .collect();
        let reply = drive_reconcile(
            &mut *state,
            domain,
            key,
            &rows,
            Some(tokens[n - 1].as_str()),
        );
        assert!(
            matches!(
                reply,
                tiler_core::boundary::CoreReply::Projection(_)
                    | tiler_core::boundary::CoreReply::Tiled(_)
            ),
            "n={n} converges, got {reply:?}"
        );
        let session = state.engine.session(key).expect("session");
        if session.is_capacity_float(&WindowId(tokens[n - 1].clone())) {
            return Some((n - 1, reply));
        }
    }
    None
}

#[test]
fn realistic_forty_helpers_still_tile_at_max_gaps() {
    // This representative domain admits all 40 arrivals. It neither proves
    // exhaustion nor supplies a universal count for other native bounds.
    clear_pending_seams();
    let mut state = test_state();
    assert_eq!(
        super::admission::anchor_fallback_for(None),
        tiler_core::admission::AnchorFallback::LargestSplittable,
        "oracle runs under real D2 search, never NoSearch"
    );
    assert_eq!(
        super::admission::no_space_for(None),
        tiler_core::admission::NoSpaceResult::Float,
        "oracle runs under the Float result"
    );
    let bounds =
        crate::tiling::tiling_domain_bounds_with(REAL_WORK, REAL_OUTER).expect("realistic insets");
    assert_eq!(
        bounds,
        Rect {
            x: 64,
            y: 64,
            w: 1792,
            h: 912
        },
        "measured-shape inset math"
    );
    let output = "mon-a".to_owned();
    state.workspaces.ensure_output(&output);
    state.inner_gap = REAL_INNER;
    state.outer_gap = REAL_OUTER;
    let ws = state.workspaces.active_id(&output).expect("active");
    let (domain, key) = crate::workspace_owner::workspace_domain(&output, &ws, bounds, REAL_INNER);
    let mut keys = Vec::new();
    let mut tokens = Vec::new();
    for i in 0..40 {
        keys.push(member_key(0x7000 + i as u64, 1700 + i as u32));
        tokens.push(format!("w{i:02}"));
    }
    let frames = vec![bounds; 40];
    let found = fill_until_float(
        &mut state, &domain, &key, &output, &ws, &tokens, &keys, &frames,
    );
    assert!(
        found.is_none(),
        "40 serial admissions still tile 1792x912 at 64/64: fresh no-space needs oracle N, not 30-40"
    );
    clear_pending_seams();
}

#[test]
fn recipe_fresh_no_space_proof_first() {
    // Checkpoint 1 (no-space proof, proxy scale): the serial oracle admits
    // tiles until the newcomer exhausts through whole-tree D2 search with
    // capacity origin, slotless exception, no tile geometry, and tiled
    // focus homed away from the float. Checkpoint 2 registers the ordinary
    // lanes with the live frame; the marker fails closed on the fake HWND;
    // replay is silent.
    clear_pending_seams();
    let mut state = test_state();
    state.inner_gap = PROXY_GAP;
    state.outer_gap = PROXY_GAP;
    let me = me();
    let output = "mon-a".to_owned();
    state.workspaces.ensure_output(&output);
    let ws = state.workspaces.active_id(&output).expect("active");
    let (domain, key) = proxy_domain(&output, &ws);
    let mut keys = Vec::new();
    let mut tokens = Vec::new();
    for i in 0..PROXY_MAX {
        keys.push(member_key(0x7100 + i as u64, 1710 + i as u32));
        tokens.push(format!("w{i:02}"));
    }
    let frames = vec![PROXY_BOUNDS; PROXY_MAX];
    let (victim, reply) = fill_until_float(
        &mut state, &domain, &key, &output, &ws, &tokens, &keys, &frames,
    )
    .expect("proxy domain exhausts within its oracle ceiling");
    assert!(victim >= 2, "at least a pair tiles first, got {victim}");
    let victim_token = tokens[victim].clone();
    let victim_key = keys[victim].clone();
    let session = state.engine.session(&key).expect("session");
    assert!(
        session.is_capacity_float(&WindowId(victim_token.clone())),
        "newcomer carries capacity origin"
    );
    assert!(
        session.is_exception(&WindowId(victim_token.clone())),
        "newcomer floats slotless"
    );
    for token in tokens[..victim].iter() {
        assert!(
            !session.is_capacity_float(&WindowId(token.clone())),
            "{token} keeps its tile slot"
        );
    }
    let tiled = tiled_tokens(&reply);
    assert_eq!(
        tiled.len(),
        victim,
        "survivors keep tiles, float rides none: {tiled:?}"
    );
    assert!(!tiled.contains(&victim_token), "float rides no geometry");
    let (focus_domain, focus_leaf) = session.focus();
    assert!(
        focus_domain.is_some() && focus_leaf.is_some(),
        "tiled focus homed"
    );
    assert_ne!(
        focus_leaf.map(|leaf| leaf.0.clone()),
        Some(format!("leaf-{victim_token}")),
        "tiled focus never names the float"
    );
    // Checkpoint 2: ordinary lane registration with the live frame.
    let mut lane_frames = std::collections::HashMap::new();
    lane_frames.insert(victim_token.clone(), PROXY_BOUNDS);
    let adopted = super::register_committed_capacity_floats(&mut state, &me, &key, &lane_frames);
    assert_eq!(adopted.len(), 1, "one newcomer registers");
    assert_eq!(adopted[0].token, victim_token);
    assert!(!adopted[0].marker_ok, "fake HWND fails the marker closed");
    assert!(
        state.floated.contains(&victim_key),
        "ordinary floated lane joins"
    );
    assert_eq!(
        state.float_rects.get(&victim_token).copied(),
        Some(PROXY_BOUNDS),
        "live frame retains"
    );
    let again = super::register_committed_capacity_floats(&mut state, &me, &key, &lane_frames);
    assert!(again.is_empty(), "replay registers nothing");
    clear_pending_seams();
}

#[test]
fn recipe_failed_unfloat_then_successful_retry() {
    // Engine retry/refloat decisions are real; geometry readback summaries
    // are stubbed as (verified, zero mismatches). The first retry preserves
    // the float, then retained gap adoption 32->8 permits explicit tiling.
    clear_pending_seams();
    let mut state = test_state();
    state.inner_gap = PROXY_GAP;
    state.outer_gap = PROXY_GAP;
    let me = me();
    let output = "mon-a".to_owned();
    state.workspaces.ensure_output(&output);
    let ws = state.workspaces.active_id(&output).expect("active");
    let (domain, key) = proxy_domain(&output, &ws);
    let mut keys = Vec::new();
    let mut tokens = Vec::new();
    for i in 0..PROXY_MAX {
        keys.push(member_key(0x7200 + i as u64, 1720 + i as u32));
        tokens.push(format!("r{i:02}"));
    }
    let frames = vec![PROXY_BOUNDS; PROXY_MAX];
    let (victim, _) = fill_until_float(
        &mut state, &domain, &key, &output, &ws, &tokens, &keys, &frames,
    )
    .expect("proxy domain exhausts within its oracle ceiling");
    let victim_token = tokens[victim].clone();
    let victim_key = keys[victim].clone();
    state.floated.insert(victim_key.clone());
    state.float_rects.insert(victim_token.clone(), PROXY_BOUNDS);
    // Roster-only observation in producer order: staged-but-unarrived
    // members never ride the observation (they have no live frames).
    let observed: Vec<super::ObservedWindow> = tokens[..=victim]
        .iter()
        .zip(keys[..=victim].iter())
        .map(|(token, key)| observed_window(key, token, PROXY_BOUNDS))
        .collect();
    inject_pending_observe(Vec::new(), Vec::new(), false);
    inject_pending_apply(Some((true, 0)));
    let settled: std::cell::RefCell<Vec<&'static str>> = Default::default();
    let settle = |outcome: &'static str| {
        settled.borrow_mut().push(outcome);
        serde_json::json!({})
    };
    let loc = state
        .workspaces
        .member_loc(&victim_key)
        .expect("homed")
        .clone();
    let log_path = state.log_path.clone();
    let tick = state.tick;
    super::apply_unfloat_to_tiled(
        &mut state,
        &me,
        &[PROXY_AREA],
        &[MonitorArea {
            device: output.clone(),
            work: PROXY_AREA,
            full: PROXY_AREA,
        }],
        &observed,
        &[],
        Vec::new(),
        &victim_key,
        &victim_token,
        &loc,
        Some(PROXY_BOUNDS),
        &crate::snapkey::SnapOrigin {
            hwnd: victim_key.hwnd,
            token: victim_token.clone(),
            pid: victim_key.pid,
            creation: victim_key.creation.clone(),
        },
        &settle,
        &log_path,
        tick,
        &CorrelationId::parse("recipe-unfloat-fail").expect("correlation"),
        "toggle-float",
        "tile",
        false,
    );
    assert_eq!(
        settled.borrow().as_slice(),
        ["unfloat-no-space"],
        "truthful no-space, no tile success"
    );
    assert!(
        state
            .engine
            .session(&key)
            .is_some_and(|s| s.is_exception(&WindowId(victim_token.clone()))),
        "still a floating exception"
    );
    assert!(state.floated.contains(&victim_key), "floated lane persists");
    assert_eq!(
        state.float_rects.get(&victim_token).copied(),
        Some(PROXY_BOUNDS),
        "live frame persists"
    );
    assert!(
        !state.fixed_tile_overrides.contains(&victim_key),
        "no false tile win records"
    );
    // Checkpoint 2: free space by shrinking the inner gap 32->8
    // (supported range, existing overrides), then the identical retry
    // applies.
    state.inner_gap = 8;
    let (small_domain, small_key) =
        crate::workspace_owner::workspace_domain(&output, &ws, PROXY_BOUNDS, 8);
    assert_eq!(small_key, key, "gap change keeps domain identity");
    let windows: Vec<(
        WindowId,
        Rect,
        tiler_core::size_hints::WindowSizeHints,
        bool,
    )> = tokens[..=victim]
        .iter()
        .map(|token| {
            (
                WindowId(token.clone()),
                PROXY_BOUNDS,
                tiler_core::size_hints::WindowSizeHints::none(),
                false,
            )
        })
        .collect();
    let revision = committed_revision(&state, &key);
    let fp = crate::tiling::fingerprint(
        &tokens[..=victim]
            .iter()
            .map(|token| (token.clone(), PROXY_BOUNDS))
            .collect::<Vec<_>>(),
    );
    // The victim rides floating in the live observation, so the gap event
    // carries it floating like the production row assembler does.
    let mut gap_windows = windows.clone();
    if let Some(entry) = gap_windows
        .iter_mut()
        .find(|(id, _, _, _)| id.0 == victim_token)
    {
        entry.3 = true;
    }
    let gap_focused = tokens[..=victim]
        .iter()
        .find(|token| *token != &victim_token)
        .map(|token| WindowId(token.clone()));
    assert!(
        super::adopt_gaps_for_route(
            &mut state,
            &small_domain,
            &key,
            8,
            &gap_windows,
            gap_focused.as_ref(),
            revision,
            fp,
            &CorrelationId::parse("recipe-gap-shrink").expect("correlation"),
        ),
        "gap shrink adopts through UpdateGaps"
    );
    inject_pending_observe(Vec::new(), Vec::new(), false);
    inject_pending_apply(Some((true, 0)));
    let settled2: std::cell::RefCell<Vec<&'static str>> = Default::default();
    let settle2 = |outcome: &'static str| {
        settled2.borrow_mut().push(outcome);
        serde_json::json!({})
    };
    let tick2 = state.tick;
    super::apply_unfloat_to_tiled(
        &mut state,
        &me,
        &[PROXY_AREA],
        &[MonitorArea {
            device: output.clone(),
            work: PROXY_AREA,
            full: PROXY_AREA,
        }],
        &observed,
        &[],
        Vec::new(),
        &victim_key,
        &victim_token,
        &loc,
        Some(PROXY_BOUNDS),
        &crate::snapkey::SnapOrigin {
            hwnd: victim_key.hwnd,
            token: victim_token.clone(),
            pid: victim_key.pid,
            creation: victim_key.creation.clone(),
        },
        &settle2,
        &log_path,
        tick2,
        &CorrelationId::parse("recipe-unfloat-ok").expect("correlation"),
        "toggle-float",
        "tile",
        false,
    );
    assert_eq!(
        settled2.borrow().as_slice(),
        ["unfloat-applied"],
        "freed space retries to a real tile, got {:?}",
        settled2.borrow()
    );
    assert!(
        !state.floated.contains(&victim_key),
        "lane drops on success"
    );
    assert!(
        state.fixed_tile_overrides.contains(&victim_key),
        "explicit win records runtime suppress"
    );
    clear_pending_seams();
}

/// Send-route destination-capacity probe: seed the source mover plus `n`
/// serial destination tiles (serial producer order, real Engine), then run
/// this exact send. True when the send commits `MoveToFloat`. False when
/// the destination still tiles (or a seed leg fails to converge, which the
/// journey re-asserts loudly rather than guessing).
fn send_probe_is_float(n: usize, follow: bool) -> bool {
    use tiler_core::boundary::CoreReply;
    let mut probe = test_state();
    probe.inner_gap = PROXY_GAP;
    probe.outer_gap = PROXY_GAP;
    let output = "mon-a".to_owned();
    probe.workspaces.ensure_output(&output);
    let ids = probe.workspaces.workspace_ids(&output);
    let (source, target) = (ids[0].clone(), ids[1].clone());
    let (source_domain, source_key) =
        crate::workspace_owner::workspace_domain(&output, &source, PROXY_BOUNDS, PROXY_GAP);
    let (target_domain, target_key) =
        crate::workspace_owner::workspace_domain(&output, &target, PROXY_BOUNDS, PROXY_GAP);
    let live = frame(4, 4, 9, 9);
    let source_rows = [crate::workspace_owner::OwnerRow {
        token: "wM".to_owned(),
        rect: live,
        hints: tiler_core::size_hints::WindowSizeHints::none(),
        floating: false,
    }];
    let target_rows: Vec<crate::workspace_owner::OwnerRow> = (0..n)
        .map(|i| crate::workspace_owner::OwnerRow {
            token: format!("wT{i:02}"),
            rect: PROXY_BOUNDS,
            hints: tiler_core::size_hints::WindowSizeHints::none(),
            floating: false,
        })
        .collect();
    // Source seed: single mover tiles.
    let carried: Vec<(
        WindowId,
        Rect,
        tiler_core::size_hints::WindowSizeHints,
        bool,
    )> = source_rows
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
    let revision = committed_revision(&probe, &source_key);
    let fp = crate::tiling::fingerprint(
        &carried
            .iter()
            .map(|(w, r, _, _)| (w.0.clone(), *r))
            .collect::<Vec<_>>(),
    );
    let event = crate::tiling::build_reconcile_event_for_floating(
        &probe.owner.clone(),
        &probe.generation.clone(),
        &CorrelationId::parse("recipe-send-seed").expect("correlation"),
        revision,
        fp,
        &source_domain,
        &source_key,
        probe.outer_gap,
        &carried,
        Some(&WindowId("wM".to_owned())),
    );
    if !matches!(
        probe.engine.handle(&event),
        CoreReply::Projection(_) | CoreReply::Tiled(_)
    ) {
        return false;
    }
    // Destination seed, serial producer order: admit targets one per pass
    // so the retained tree, not a batch guess, sets capacity. Any floated
    // target means this `n` overshoots a pure tiled seed.
    for m in 1..=n {
        let carried: Vec<(
            WindowId,
            Rect,
            tiler_core::size_hints::WindowSizeHints,
            bool,
        )> = target_rows[..m]
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
        let revision = committed_revision(&probe, &target_key);
        let fp = crate::tiling::fingerprint(
            &carried
                .iter()
                .map(|(w, r, _, _)| (w.0.clone(), *r))
                .collect::<Vec<_>>(),
        );
        let event = crate::tiling::build_reconcile_event_for_floating(
            &probe.owner.clone(),
            &probe.generation.clone(),
            &CorrelationId::parse("recipe-send-seed").expect("correlation"),
            revision,
            fp,
            &target_domain,
            &target_key,
            probe.outer_gap,
            &carried,
            Some(&WindowId(target_rows[m - 1].token.clone())),
        );
        let reply = probe.engine.handle(&event);
        if !matches!(reply, CoreReply::Projection(_) | CoreReply::Tiled(_)) {
            return false;
        }
        if probe.engine.session(&target_key).is_some_and(|s| {
            target_rows[..m]
                .iter()
                .any(|row| s.is_capacity_float(&WindowId(row.token.clone())))
        }) {
            return false;
        }
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
        &probe.owner.clone(),
        &probe.generation.clone(),
        &CorrelationId::parse("recipe-send-probe").expect("correlation"),
        committed_revision(&probe, &source_key),
        fp,
        (source_domain.clone(), source_key.clone()),
        (target_domain.clone(), target_key.clone()),
        &source_rows,
        &target_rows,
        "wM",
        probe.outer_gap,
        follow,
    )
    .expect("send builds");
    crate::workspace_owner::stamp_send_target(&mut event, &target_key);
    matches!(
        probe.engine.handle(&event),
        CoreReply::SendWorkspace(plan)
            if matches!(
                plan.operation,
                tiler_core::contract::LifecycleOperation::MoveToFloat { .. }
            )
    )
}

#[test]
fn recipe_same_output_capacity_send_follow_and_stay() {
    // Same-output capacity send through the production send-event builder
    // plus the real Engine, for both follow and stay. The destination
    // capacity oracle fills the destination serially until the send route
    // itself commits `MoveToFloat` (no reconciled-count guess): the mover
    // finds no splittable D2 target, commits slotless with the verbatim
    // live frame, survivors still project, the emptied source retires, and
    // the reply classifies as a capacity-float transfer. Membership is
    // staged manually after Engine commit: this models ordering but does
    // not exercise the production send-tail or native arrival fence.
    use tiler_core::boundary::CoreReply;
    for follow in [false, true] {
        clear_pending_seams();
        // Destination-capacity oracle over the send route: grow the
        // destination roster until this exact send commits MoveToFloat.
        let mut dest_n = None;
        for n in 1..PROXY_MAX {
            if send_probe_is_float(n, follow) {
                dest_n = Some(n);
                break;
            }
        }
        let dest_n =
            dest_n.expect("send-route oracle finds destination capacity within its ceiling");
        assert!(
            dest_n >= 2,
            "follow={follow}: destination holds at least a pair first"
        );
        // Replay the oracle count through the send builder and Engine.
        let mut state = test_state();
        state.inner_gap = PROXY_GAP;
        state.outer_gap = PROXY_GAP;
        let me = me();
        let output = "mon-a".to_owned();
        state.workspaces.ensure_output(&output);
        let ids = state.workspaces.workspace_ids(&output);
        let (source, target) = (ids[0].clone(), ids[1].clone());
        let (source_domain, source_key) =
            crate::workspace_owner::workspace_domain(&output, &source, PROXY_BOUNDS, PROXY_GAP);
        let (target_domain, target_key) =
            crate::workspace_owner::workspace_domain(&output, &target, PROXY_BOUNDS, PROXY_GAP);
        let live_frame = frame(4, 4, 9, 9);
        let source_rows = [crate::workspace_owner::OwnerRow {
            token: "wM".to_owned(),
            rect: live_frame,
            hints: tiler_core::size_hints::WindowSizeHints::none(),
            floating: false,
        }];
        let target_rows: Vec<crate::workspace_owner::OwnerRow> = (0..dest_n)
            .map(|i| crate::workspace_owner::OwnerRow {
                token: format!("wT{i:02}"),
                rect: PROXY_BOUNDS,
                hints: tiler_core::size_hints::WindowSizeHints::none(),
                floating: false,
            })
            .collect();
        for m in 1..=dest_n {
            let carried: Vec<(
                WindowId,
                Rect,
                tiler_core::size_hints::WindowSizeHints,
                bool,
            )> = target_rows[..m]
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
            let revision = committed_revision(&state, &target_key);
            let fp = crate::tiling::fingerprint(
                &carried
                    .iter()
                    .map(|(w, r, _, _)| (w.0.clone(), *r))
                    .collect::<Vec<_>>(),
            );
            let event = crate::tiling::build_reconcile_event_for_floating(
                &state.owner.clone(),
                &state.generation.clone(),
                &CorrelationId::parse("recipe-send-seed").expect("correlation"),
                revision,
                fp,
                &target_domain,
                &target_key,
                state.outer_gap,
                &carried,
                Some(&WindowId(target_rows[m - 1].token.clone())),
            );
            let reply = state.engine.handle(&event);
            assert!(
                matches!(reply, CoreReply::Projection(_) | CoreReply::Tiled(_)),
                "follow={follow}: dest seed m={m} converges, got {reply:?}"
            );
        }
        let carried: Vec<(
            WindowId,
            Rect,
            tiler_core::size_hints::WindowSizeHints,
            bool,
        )> = source_rows
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
        let revision = committed_revision(&state, &source_key);
        let fp = crate::tiling::fingerprint(
            &carried
                .iter()
                .map(|(w, r, _, _)| (w.0.clone(), *r))
                .collect::<Vec<_>>(),
        );
        let event = crate::tiling::build_reconcile_event_for_floating(
            &state.owner.clone(),
            &state.generation.clone(),
            &CorrelationId::parse("recipe-send-seed").expect("correlation"),
            revision,
            fp,
            &source_domain,
            &source_key,
            state.outer_gap,
            &carried,
            Some(&WindowId("wM".to_owned())),
        );
        let reply = state.engine.handle(&event);
        assert!(
            matches!(reply, CoreReply::Projection(_) | CoreReply::Tiled(_)),
            "follow={follow}: source seed converges, got {reply:?}"
        );
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
        // Pre-commit destination snapshot.
        assert!(
            state
                .engine
                .session(&target_key)
                .is_some_and(|s| !s.is_exception(&WindowId("wM".to_owned()))),
            "follow={follow}: destination takes no float before commit"
        );
        let mut event = crate::workspace_owner::build_send_event(
            &state.owner.clone(),
            &state.generation.clone(),
            &CorrelationId::parse("recipe-send").expect("correlation"),
            committed_revision(&state, &source_key),
            fp,
            (source_domain.clone(), source_key.clone()),
            (target_domain.clone(), target_key.clone()),
            &source_rows,
            &target_rows,
            "wM",
            state.outer_gap,
            follow,
        )
        .expect("send builds");
        crate::workspace_owner::stamp_send_target(&mut event, &target_key);
        let reply = state.engine.handle(&event);
        let shape = format!("follow={follow}");
        let plan = match &reply {
            CoreReply::SendWorkspace(plan) => plan,
            other => panic!("{shape}: send commits, got {other:?}"),
        };
        assert!(
            matches!(
                plan.operation,
                tiler_core::contract::LifecycleOperation::MoveToFloat { .. }
            ),
            "{shape}: mover floats slotless at the named destination (oracle n={dest_n})"
        );
        assert!(
            super::reply_is_capacity_float_transfer(&reply),
            "{shape}: reply classifies as a capacity-float transfer"
        );
        let geometry: Vec<String> = plan
            .geometry
            .iter()
            .map(|entry| entry.window.0.clone())
            .collect();
        for row in target_rows.iter() {
            assert!(
                geometry.contains(&row.token),
                "{shape}: survivor {} still projects, got {geometry:?}",
                row.token
            );
        }
        assert!(
            !geometry.contains(&"wM".to_owned()),
            "{shape}: mover rides no tile geometry"
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
        assert_eq!(
            target_session.retained_float_geometry(&WindowId("wM".to_owned())),
            Some(live_frame),
            "{shape}: verbatim live frame retains"
        );
        assert!(
            state.engine.session(&source_key).is_none_or(|s| {
                !s.is_exception(&WindowId("wM".to_owned()))
                    && !s.is_capacity_float(&WindowId("wM".to_owned()))
            }),
            "{shape}: emptied source retires"
        );
        // Manually stage post-commit membership before ordinary-float
        // registration; the production transfer fence is not under test.
        let mover_key = member_key(0x7301, 1731);
        assert!(
            state
                .workspaces
                .assign(mover_key.clone(), &output, &target, false)
        );
        state
            .member_tokens
            .insert(mover_key.clone(), "wM".to_owned());
        let mut lane_frames = std::collections::HashMap::new();
        lane_frames.insert("wM".to_owned(), frame(0, 0, 1, 1));
        let adopted =
            super::register_committed_capacity_floats(&mut state, &me, &target_key, &lane_frames);
        assert_eq!(adopted.len(), 1, "{shape}: mover registers");
        assert_eq!(
            state.float_rects.get("wM").copied(),
            Some(live_frame),
            "{shape}: retained live frame wins over the caller-carried frame"
        );
        assert!(state.floated.contains(&mover_key));
        assert!(
            !super::reply_is_capacity_float_transfer(&CoreReply::Rejected {
                kind: "x",
                message: "y",
            }),
            "{shape}: refusals never classify"
        );
        clear_pending_seams();
    }
}

#![forbid(unsafe_code)]
use super::*;

#[test]
fn hidden_minimize_after_hide_releases_then_rejoins() {
    // `HiddenRecord.iconic` freezes at hide time, but a hidden window can
    // minimize after hide while the raw gate skips it as invisible. The
    // observation-boundary refresh must release its leaf (storing the
    // slot) instead of retaining it: a noniconic hidden member rides its
    // snapshot, then iconic-after-hide omits, then reveal rejoins with
    // no false restore from the frozen snapshot.
    clear_pending_seams();
    let (mut state, ws, domain) = setup("mon-1");
    let output = "mon-1".to_owned();
    let rect = bounds();
    wire_member(&mut state, &output, &ws, 101, "wA", rect);
    wire_member(&mut state, &output, &ws, 102, "wB", rect);
    let seed = vec![mk_observed(101, "wA", 0, 0), mk_observed(102, "wB", 0, 0)];
    let _ = dispatch(
        &mut state,
        &output,
        &ws,
        &domain,
        &seed,
        &[],
        Some("wA"),
        "d23-seed",
    );
    let seed_order = leaf_window_order(&state, &domain.1);
    let me = me();
    let (fulls, areas) = solo_topology(&output);
    // Hide B non-iconic: it keeps riding its hidden snapshot row with no
    // slot, exactly like a visible tile.
    hide_member(&mut state, 102, "claim-wb", false);
    live_tags(&[(101, Some("wA")), (102, Some("wB"))]);
    let (observed, retained) = tick_leg(
        &mut state,
        &me,
        &fulls,
        &areas,
        vec![mk_observed(101, "wA", 0, 0)],
        Vec::new(),
        vec![102],
    );
    assert_eq!(observed.len(), 1);
    let mut hint_cx = HintCx::new();
    let rows = assemble_domain_rows(
        &mut state,
        &output,
        &ws,
        &observed,
        &retained,
        "reconcile",
        "d23-hide",
        &mut hint_cx,
    )
    .expect("non-iconic hidden rows stay complete");
    assert_eq!(
        rows.len(),
        2,
        "non-iconic hidden B still rides its snapshot"
    );
    assert!(rows.iter().any(|row| row.token == "wB"));
    let _ = dispatch(
        &mut state,
        &output,
        &ws,
        &domain,
        &observed,
        &retained,
        Some("wA"),
        "d23-hide",
    );
    assert!(
        !slot_present(&state, &domain.1, "wB"),
        "no slot while carried"
    );
    // B minimizes after hide: the refresh observes iconic state through
    // the hidden gate and omits B, releasing the leaf into a slot. The
    // frozen snapshot/rect must not restore anything on its own.
    let (observed, retained) = tick_leg(
        &mut state,
        &me,
        &fulls,
        &areas,
        vec![mk_observed(101, "wA", 0, 0)],
        vec![102],
        vec![102],
    );
    assert_eq!(observed.len(), 1);
    assert!(
        state.minimized_omitted.contains(&key(102)),
        "hidden iconic refresh derives omission through real observe"
    );
    let mut hint_cx = HintCx::new();
    let rows = assemble_domain_rows(
        &mut state,
        &output,
        &ws,
        &observed,
        &retained,
        "reconcile",
        "d23-hmin",
        &mut hint_cx,
    )
    .expect("hidden-iconic omission stays complete");
    assert_eq!(rows.len(), 1, "iconic-after-hide B carries no row");
    assert!(rows.iter().all(|row| row.token != "wB"));
    let _ = dispatch(
        &mut state,
        &output,
        &ws,
        &domain,
        &observed,
        &retained,
        Some("wA"),
        "d23-hmin",
    );
    assert!(
        slot_present(&state, &domain.1, "wB"),
        "leaf released into a slot"
    );
    // Reveal B (post-reveal bookkeeping): it rejoins beside A with MRU
    // focus untouched, never the frozen snapshot position alone.
    state.hidden_claims.remove(&key(102));
    state.workspaces.set_hidden(&key(102), false);
    let (restored, retained) = tick_leg(
        &mut state,
        &me,
        &fulls,
        &areas,
        vec![mk_observed(101, "wA", 0, 0), mk_observed(102, "wB", 0, 0)],
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
        Some("wA"),
        "d23-hrestore",
    );
    assert_eq!(
        leaf_window_order(&state, &domain.1),
        seed_order,
        "revealed B rejoins the old neighborhood exactly, never a fresh admission"
    );
    assert!(
        !slot_present(&state, &domain.1, "wB"),
        "rejoin consumes the slot"
    );
    assert_eq!(focused_token(&state, &domain.1).as_deref(), Some("wA"));
    clear_pending_seams();
}

#[test]
fn close_drain_fanout_covers_hidden_and_other_output_automatically() {
    // Queued closes on a hidden workspace and another output drain
    // through ONE production fanout call over the tick observation: no
    // per-domain manual dispatch, no hidden polling. Active-domain
    // queues are excluded (ordinary legs own them); unreadable and
    // rejected domains retain; a close on a pruned workspace still
    // delivers and retires instead of stranding an Engine ghost.
    clear_pending_seams();
    let output1 = "mon-1".to_owned();
    let output2 = "mon-2".to_owned();
    let mut state = test_state();
    state.workspaces.ensure_output(&output1);
    state.workspaces.ensure_output(&output2);
    let ws1 = state.workspaces.active_id(&output1).expect("active");
    let ids2 = state.workspaces.workspace_ids(&output2);
    assert!(ids2.len() >= 2, "hidden plus selected workspaces");
    let (ws_hidden, ws_other) = (ids2[0].clone(), ids2[1].clone());
    state.workspaces.activate(&output2, &ws_other);
    let domain1 = crate::workspace_owner::workspace_domain(&output1, &ws1, bounds(), 8);
    let domain_h = crate::workspace_owner::workspace_domain(&output2, &ws_hidden, bounds(), 8);
    let rect = bounds();
    wire_member(&mut state, &output1, &ws1, 101, "wA", rect);
    wire_member(&mut state, &output1, &ws1, 111, "wS", rect);
    wire_member(&mut state, &output2, &ws_hidden, 201, "wX", rect);
    wire_member(&mut state, &output2, &ws_hidden, 202, "wH", rect);
    let seed1 = vec![mk_observed(101, "wA", 0, 0), mk_observed(111, "wS", 0, 0)];
    let _ = dispatch(
        &mut state,
        &output1,
        &ws1,
        &domain1,
        &seed1,
        &[],
        Some("wA"),
        "d23-seed1",
    );
    let seed_h = vec![mk_observed(201, "wX", 0, 0), mk_observed(202, "wH", 0, 0)];
    let _ = dispatch(
        &mut state,
        &output2,
        &ws_hidden,
        &domain_h,
        &seed_h,
        &[],
        Some("wX"),
        "d23-seedh",
    );
    // Hide mon-2 members with real claims (X ordinary, H iconic), then
    // converge the hidden domain once: X rides, H stores its slot.
    hide_member(&mut state, 201, "claim-wx", false);
    hide_member(&mut state, 202, "claim-wh", true);
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
    let reply = dispatch(
        &mut state,
        &output2,
        &ws_hidden,
        &domain_h,
        &[],
        &[],
        None,
        "d23-h",
    );
    assert!(
        matches!(
            reply,
            tiler_core::boundary::CoreReply::Projection(_)
                | tiler_core::boundary::CoreReply::Tiled(_)
        ),
        "hidden hide leg must converge"
    );
    assert!(
        slot_present(&state, &domain_h.1, "wH"),
        "H stores its slot on hide"
    );
    // Burst: A closes visible on mon-1 while H closes hidden. One tick
    // leg derives mon-1's inventory; the audit retires H's real claim.
    live_tags(&[(101, None), (111, Some("wS"))]);
    let (observed, retained) = tick_leg(
        &mut state,
        &me,
        &fulls,
        &areas,
        vec![mk_observed(111, "wS", 0, 0)],
        Vec::new(),
        vec![201, 202],
    );
    assert_eq!(observed.len(), 1);
    assert!(
        state.pending_closed.contains_key("wA"),
        "visible close queues"
    );
    let (store, tmp) = tmp_store("fanout");
    inject_pending_recovery(202, Err("absent: hwnd destroyed".to_owned()));
    inject_pending_tag(202, None);
    audit_hidden_claims(&mut state, &me, &store);
    assert!(
        state.pending_closed.contains_key("wH"),
        "hidden close queues"
    );
    // Drift the live gaps first: retained Engine domains still carry
    // the seed gaps, so the drain must converge through the same
    // topology-preserving update-gaps branch as the active legs instead
    // of refusing domain-mismatch forever.
    state.inner_gap = 12;
    state.outer_gap = 12;
    // ONE production fanout over the tick observation: the hidden
    // domain delivers while the active domain's queue is excluded.
    fanout_pending_closed(&mut state, &me, &areas, &observed, &retained, "d23-fanout");
    assert!(
        !state.pending_closed.contains_key("wH"),
        "hidden close drains through fanout despite gap drift"
    );
    assert!(
        state.pending_closed.contains_key("wA"),
        "active-domain close stays for its ordinary leg"
    );
    assert!(
        !slot_present(&state, &domain_h.1, "wH"),
        "hidden slot forgotten"
    );
    assert_eq!(
        leaf_window_order(&state, &domain_h.1),
        vec!["wX".to_owned()],
        "survivor keeps its current projection through the drain"
    );
    assert_eq!(pending_counters().apply, 0, "fanout applies no geometry");
    assert_eq!(
        focused_token(&state, &domain_h.1).as_deref(),
        Some("wX"),
        "hidden MRU retained with no setter"
    );
    // Ordinary mon-1 leg delivers the active close afterwards.
    let reply = dispatch(
        &mut state,
        &output1,
        &ws1,
        &domain1,
        &observed,
        &retained,
        Some("wS"),
        "d23-close1",
    );
    assert!(
        matches!(
            reply,
            tiler_core::boundary::CoreReply::Projection(_)
                | tiler_core::boundary::CoreReply::Tiled(_)
        ),
        "mon-1 close leg must converge"
    );
    assert!(state.pending_closed.is_empty());
    // X closes hidden last, emptying (and possibly pruning) its
    // workspace: the next fanout still delivers and retires the Engine
    // session instead of stranding a ghost.
    inject_pending_recovery(201, Err("absent: hwnd destroyed".to_owned()));
    inject_pending_tag(201, None);
    audit_hidden_claims(&mut state, &me, &store);
    assert!(
        state.pending_closed.contains_key("wX"),
        "sole hidden close queues"
    );
    let (observed, retained) = tick_leg(
        &mut state,
        &me,
        &fulls,
        &areas,
        vec![mk_observed(111, "wS", 0, 0)],
        Vec::new(),
        Vec::new(),
    );
    fanout_pending_closed(&mut state, &me, &areas, &observed, &retained, "d23-fanout2");
    assert!(
        state.pending_closed.is_empty(),
        "pruned-domain close still delivers"
    );
    assert!(
        state.engine.session(&domain_h.1).is_none(),
        "emptied hidden session retires, no Engine ghost"
    );
    let _ = std::fs::remove_dir_all(&tmp);
    clear_pending_seams();
}

#[test]
fn hidden_iconic_omits_preserves_slot_then_close_delivers() {
    // Fix-1 end to end: B minimizes (slot stored), then hides iconic
    // with a real ledger claim. The hidden lane omits B (never consumes
    // the slot on hide); a later hidden close reports and forgets it,
    // and reveal rejoins the old neighborhood.
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
    // Hide B iconic: real claim + hidden flag, exactly the hide commit.
    hide_member(&mut state, 102, "claim-wb", true);
    let mut hint_cx = HintCx::new();
    let rows = assemble_domain_rows(
        &mut state,
        &output,
        &ws,
        &minimized,
        &retained,
        "reconcile",
        "d23-hide",
        &mut hint_cx,
    )
    .expect("iconic-hidden omission stays complete");
    assert_eq!(rows.len(), 2, "iconic-hidden B carries no row");
    assert!(rows.iter().all(|row| row.token != "wB"));
    let reply = dispatch(
        &mut state,
        &output,
        &ws,
        &domain,
        &minimized,
        &retained,
        Some("wC"),
        "d23-hide",
    );
    assert!(
        matches!(
            reply,
            tiler_core::boundary::CoreReply::Projection(_)
                | tiler_core::boundary::CoreReply::Tiled(_)
        ),
        "hide leg must converge, got {reply:?}"
    );
    assert!(
        slot_present(&state, &domain.1, "wB"),
        "hide never consumes the stored slot"
    );
    // B closes while hidden: the audit retires the real claim and queues
    // the verified close; the next valid observation delivers it.
    let (store, tmp) = tmp_store("hideclose");
    inject_pending_recovery(102, Err("absent: hwnd destroyed".to_owned()));
    inject_pending_tag(102, None);
    audit_hidden_claims(&mut state, &me, &store);
    assert!(
        !state.hidden_claims.contains_key(&key(102)),
        "claim retires"
    );
    assert!(
        state.pending_closed.contains_key("wB"),
        "hidden close queues with live-time homing"
    );
    let reply = dispatch(
        &mut state,
        &output,
        &ws,
        &domain,
        &minimized,
        &retained,
        Some("wC"),
        "d23-hclose",
    );
    assert!(
        matches!(
            reply,
            tiler_core::boundary::CoreReply::Projection(_)
                | tiler_core::boundary::CoreReply::Tiled(_)
        ),
        "hidden-close leg must converge, got {reply:?}"
    );
    assert!(
        !slot_present(&state, &domain.1, "wB"),
        "hidden close forgets the slot"
    );
    assert!(state.pending_closed.is_empty(), "hidden close drains");
    let _ = std::fs::remove_dir_all(&tmp);
    clear_pending_seams();
}

#[test]
fn hidden_close_burst_queues_and_drains_per_domain() {
    // True hidden-workspace closes (real ledger claims, retired through
    // the audit) burst across two domains: each domain's next valid
    // observation carries only its own close; non-iconic hidden members
    // keep snapshot rows with no geometry/focus writes.
    clear_pending_seams();
    let (mut state, ws1, domain1) = setup("mon-1");
    let output1 = "mon-1".to_owned();
    state.workspaces.ensure_output("mon-2");
    let output2 = "mon-2".to_owned();
    let ws2 = state.workspaces.active_id(&output2).expect("active");
    let domain2 = crate::workspace_owner::workspace_domain(&output2, &ws2, bounds(), 8);
    let rect = bounds();
    wire_member(&mut state, &output1, &ws1, 101, "wA", rect);
    wire_member(&mut state, &output1, &ws1, 111, "wS", rect);
    wire_member(&mut state, &output2, &ws2, 201, "wX", rect);
    wire_member(&mut state, &output2, &ws2, 202, "wH", rect);
    let seed1 = vec![mk_observed(101, "wA", 0, 0), mk_observed(111, "wS", 0, 0)];
    let _ = dispatch(
        &mut state,
        &output1,
        &ws1,
        &domain1,
        &seed1,
        &[],
        Some("wA"),
        "d23-seed1",
    );
    let seed2 = vec![mk_observed(201, "wX", 0, 0), mk_observed(202, "wH", 0, 0)];
    let _ = dispatch(
        &mut state,
        &output2,
        &ws2,
        &domain2,
        &seed2,
        &[],
        Some("wX"),
        "d23-seed2",
    );
    // Hide mon-2 members with real claims: X ordinary, H iconic
    // (minimized at hide time, slot stored on the next omission leg).
    hide_member(&mut state, 201, "claim-wx", false);
    hide_member(&mut state, 202, "claim-wh", true);
    let mut hint_cx = HintCx::new();
    let rows = assemble_domain_rows(
        &mut state,
        &output2,
        &ws2,
        &[],
        &[],
        "reconcile",
        "d23-hide2",
        &mut hint_cx,
    )
    .expect("hidden omission stays complete");
    assert_eq!(rows.len(), 1, "only non-iconic X rides a snapshot row");
    assert_eq!(rows[0].token, "wX");
    assert!(!rows[0].floating, "hidden survivor stays tiled");
    let writable = writable_tokens(&state, &output2, &ws2, &[]);
    assert!(
        !writable.contains("wX") && !writable.contains("wH"),
        "hidden members take no geometry writes"
    );
    let reply = dispatch(
        &mut state,
        &output2,
        &ws2,
        &domain2,
        &[],
        &[],
        None,
        "d23-hide2",
    );
    assert!(
        matches!(
            reply,
            tiler_core::boundary::CoreReply::Projection(_)
                | tiler_core::boundary::CoreReply::Tiled(_)
        ),
        "hide leg must converge, got {reply:?}"
    );
    assert!(
        slot_present(&state, &domain2.1, "wH"),
        "H stores its slot on hide"
    );
    assert!(
        !slot_present(&state, &domain2.1, "wX"),
        "snapshot-carried X mints no slot"
    );
    // Burst: A closes visible on mon-1 while H closes hidden on mon-2.
    state.last_hwnds = [111].into_iter().collect();
    inject_pending_tag(111, Some("tag-wS".to_owned()));
    inject_pending_tag(101, None);
    workspace_close_cleanup(&mut state);
    let (store, tmp) = tmp_store("burst");
    inject_pending_recovery(202, Err("absent: hwnd destroyed".to_owned()));
    inject_pending_tag(202, None);
    audit_hidden_claims(&mut state, &me(), &store);
    assert_eq!(
        state.pending_closed.len(),
        2,
        "visible + hidden closes queue"
    );
    assert_eq!(state.pending_closed["wA"].workspace, ws1);
    assert_eq!(
        state.pending_closed["wH"].workspace, ws2,
        "hidden close keeps its hidden-domain homing"
    );
    // mon-1 leg carries only its own close and retains the other.
    let survivors1 = vec![mk_observed(111, "wS", 0, 0)];
    let reply = dispatch(
        &mut state,
        &output1,
        &ws1,
        &domain1,
        &survivors1,
        &[],
        Some("wS"),
        "d23-close1",
    );
    assert!(
        matches!(
            reply,
            tiler_core::boundary::CoreReply::Projection(_)
                | tiler_core::boundary::CoreReply::Tiled(_)
        ),
        "mon-1 close leg must converge, got {reply:?}"
    );
    assert!(
        !state.pending_closed.contains_key("wA"),
        "homed close drains"
    );
    assert!(
        state.pending_closed.contains_key("wH"),
        "remote close retained"
    );
    // mon-2 hidden leg delivers H's close beside X's snapshot row.
    let reply = dispatch(
        &mut state,
        &output2,
        &ws2,
        &domain2,
        &[],
        &[],
        None,
        "d23-close2",
    );
    assert!(
        matches!(
            reply,
            tiler_core::boundary::CoreReply::Projection(_)
                | tiler_core::boundary::CoreReply::Tiled(_)
        ),
        "mon-2 close leg must converge, got {reply:?}"
    );
    assert!(
        state.pending_closed.is_empty(),
        "burst fully drains per domain"
    );
    assert!(
        !slot_present(&state, &domain2.1, "wH"),
        "hidden close forgets the slot"
    );
    assert_eq!(
        focused_token(&state, &domain2.1).as_deref(),
        Some("wX"),
        "hidden domain keeps retained MRU focus with no setter: reconcile never focuses"
    );
    let _ = std::fs::remove_dir_all(&tmp);
    clear_pending_seams();
}

#[test]
fn rejected_observation_retains_pending_close_for_retry() {
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
    assert!(state.pending_closed.contains_key("wB"));
    // A foreign-owner observation diverges: the report must survive for
    // the next ordinary signal (never a retry ledger, never a poll).
    {
        use tiler_core::directional::WindowId;
        use tiler_core::ids::{CorrelationId, OwnerId};
        let mut hint_cx = HintCx::new();
        let rows = assemble_domain_rows(
            &mut state,
            &output,
            &ws,
            &survivors,
            &retained,
            "reconcile",
            "d23-stale",
            &mut hint_cx,
        )
        .expect("rows assemble");
        let carried: Vec<(
            WindowId,
            tiler_core::geometry::Rect,
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
        let fp = crate::tiling::fingerprint(
            &rows
                .iter()
                .map(|row| (row.token.clone(), row.rect))
                .collect::<Vec<_>>(),
        );
        let correlation = CorrelationId::parse("d23-stale").expect("correlation");
        let foreign = OwnerId::parse("other-owner").expect("owner");
        let mut event = crate::tiling::build_reconcile_event_for_floating(
            &foreign,
            &state.generation,
            &correlation,
            revision_of(&state, &domain.1),
            fp,
            &domain.0,
            &domain.1,
            8,
            &carried,
            Some(&WindowId("wC".to_owned())),
        );
        let reply = handle_engine_event(&mut state, &mut event);
        assert!(
            matches!(
                reply,
                tiler_core::boundary::CoreReply::Rejected { .. }
                    | tiler_core::boundary::CoreReply::SnapshotInvalid { .. }
                    | tiler_core::boundary::CoreReply::Diverged(_)
            ),
            "foreign owner must not accept, got {reply:?}"
        );
        assert_eq!(
            event.closed_windows.len(),
            1,
            "stale leg still stamps the report"
        );
    }
    assert!(
        state.pending_closed.contains_key("wB"),
        "rejection retains the report"
    );
    assert!(
        slot_present(&state, &domain.1, "wB"),
        "rejection keeps the slot"
    );
    // The next valid observation delivers and drains.
    live_tags(&[(101, Some("wA")), (103, Some("wC"))]);
    let (survivors2, retained2) = tick_leg(
        &mut state,
        &me,
        &fulls,
        &areas,
        vec![mk_observed(101, "wA", 0, 0), mk_observed(103, "wC", 0, 0)],
        Vec::new(),
        Vec::new(),
    );
    let _ = dispatch(
        &mut state,
        &output,
        &ws,
        &domain,
        &survivors2,
        &retained2,
        Some("wC"),
        "d23-retry",
    );
    assert!(state.pending_closed.is_empty());
    assert!(!slot_present(&state, &domain.1, "wB"));
    clear_pending_seams();
}

#[test]
fn live_resight_drops_stale_close_without_dispatching_it() {
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
    live_tags(&[(101, Some("wA")), (102, None), (103, Some("wC"))]);
    let (_, _) = tick_leg(
        &mut state,
        &me,
        &fulls,
        &areas,
        vec![mk_observed(101, "wA", 0, 0), mk_observed(103, "wC", 0, 0)],
        Vec::new(),
        Vec::new(),
    );
    assert!(state.pending_closed.contains_key("wB"));
    // The id is observed live again before delivery (same-token reuse):
    // the close is stale and must drop so no contradictory live+closed
    // row dispatches; B rejoins its slot instead.
    wire_member(&mut state, &output, &ws, 102, "wB", bounds());
    live_tags(&[(102, Some("wB"))]);
    let (resighted, retained) = tick_leg(
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
    {
        use tiler_core::directional::WindowId;
        use tiler_core::ids::CorrelationId;
        let mut hint_cx = HintCx::new();
        let rows = assemble_domain_rows(
            &mut state,
            &output,
            &ws,
            &resighted,
            &retained,
            "reconcile",
            "d23-resight",
            &mut hint_cx,
        )
        .expect("rows assemble");
        let carried: Vec<(
            WindowId,
            tiler_core::geometry::Rect,
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
        let fp = crate::tiling::fingerprint(
            &rows
                .iter()
                .map(|row| (row.token.clone(), row.rect))
                .collect::<Vec<_>>(),
        );
        let correlation = CorrelationId::parse("d23-resight").expect("correlation");
        let mut event = crate::tiling::build_reconcile_event_for_floating(
            &state.owner,
            &state.generation,
            &correlation,
            revision_of(&state, &domain.1),
            fp,
            &domain.0,
            &domain.1,
            8,
            &carried,
            Some(&WindowId("wC".to_owned())),
        );
        let reply = handle_engine_event(&mut state, &mut event);
        assert!(
            event.closed_windows.is_empty(),
            "stale close never dispatches beside its live id"
        );
        assert!(
            matches!(
                reply,
                tiler_core::boundary::CoreReply::Projection(_)
                    | tiler_core::boundary::CoreReply::Tiled(_)
            ),
            "resight must converge, got {reply:?}"
        );
    }
    assert!(
        !state.pending_closed.contains_key("wB"),
        "stale close drops"
    );
    assert!(
        !slot_present(&state, &domain.1, "wB"),
        "live id rejoins its slot"
    );
    clear_pending_seams();
}

#[test]
fn transient_and_unreadable_evidence_never_closes_and_defers() {
    // A member present in the inventory but without a fresh view (hung
    // frame query, transient miss) is unknown, never closed: the real
    // observe path inventories it via the unreadable seam, assembly
    // defers with retained Engine state, and cleanup queues nothing.
    clear_pending_seams();
    let (mut state, ws, domain) = setup("mon-1");
    let output = "mon-1".to_owned();
    let rect = bounds();
    wire_member(&mut state, &output, &ws, 101, "wA", rect);
    wire_member(&mut state, &output, &ws, 102, "wD", rect);
    let observed = vec![mk_observed(101, "wA", 0, 0), mk_observed(102, "wD", 0, 0)];
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
    // D's frame hangs: still enumerated, still tagged live, but viewless
    // and NOT iconic.
    let me = me();
    let fulls = vec![bounds()];
    let areas = vec![MonitorArea {
        device: output.clone(),
        work: bounds(),
        full: bounds(),
    }];
    inject_pending_tag(101, Some("tag-wA".to_owned()));
    inject_pending_tag(102, Some("tag-wD".to_owned()));
    let (partial, retained) = tick_leg(
        &mut state,
        &me,
        &fulls,
        &areas,
        vec![mk_observed(101, "wA", 0, 0)],
        Vec::new(),
        vec![102],
    );
    assert_eq!(partial.len(), 1);
    assert!(
        state.last_hwnds.contains(&102),
        "unreadable fixture stays inventoried like production Unknown"
    );
    let mut hint_cx = HintCx::new();
    assert!(
        assemble_domain_rows(
            &mut state,
            &output,
            &ws,
            &partial,
            &retained,
            "reconcile",
            "d23-hung",
            &mut hint_cx
        )
        .is_none(),
        "viewless non-minimized member defers, never fabricates emptiness"
    );
    assert!(
        state.pending_closed.is_empty(),
        "transient evidence never queues a close"
    );
    assert!(
        state.member_tokens.contains_key(&key(102)),
        "membership retained"
    );
    // Recovery next tick converges normally with no slot minted for D.
    let reply = dispatch(
        &mut state,
        &output,
        &ws,
        &domain,
        &observed,
        &[],
        Some("wA"),
        "d23-recover",
    );
    let xs = geometry_x(&reply);
    assert_eq!(xs.len(), 2);
    assert!(
        !slot_present(&state, &domain.1, "wD"),
        "no slot is minted for transient absence"
    );
    clear_pending_seams();
}

#[test]
fn minimized_present_never_queues_and_stays_omitted() {
    // Minimized windows stay enumerated: the real observe path derives
    // the omission from the iconic fixture, cleanup never mistakes the
    // iconic state for a close, and assembly keeps omitting the row.
    clear_pending_seams();
    let (mut state, ws, domain) = setup("mon-1");
    let output = "mon-1".to_owned();
    let _ = seed_abc(&mut state, &output, &ws, &domain);
    let me = me();
    let fulls = vec![bounds()];
    let areas = vec![MonitorArea {
        device: output.clone(),
        work: bounds(),
        full: bounds(),
    }];
    inject_pending_tag(101, Some("tag-wA".to_owned()));
    inject_pending_tag(102, Some("tag-wB".to_owned()));
    inject_pending_tag(103, Some("tag-wC".to_owned()));
    let (observed, retained) = tick_leg(
        &mut state,
        &me,
        &fulls,
        &areas,
        vec![mk_observed(101, "wA", 0, 0), mk_observed(103, "wC", 0, 0)],
        vec![102],
        Vec::new(),
    );
    assert_eq!(observed.len(), 2);
    assert!(
        state.minimized_omitted.contains(&key(102)),
        "iconic fixture derives omission through real observe"
    );
    assert!(
        state.pending_closed.is_empty(),
        "minimize is never classified as close"
    );
    assert!(
        state.member_tokens.contains_key(&key(102)),
        "minimized membership retained"
    );
    let mut hint_cx = HintCx::new();
    let rows = assemble_domain_rows(
        &mut state,
        &output,
        &ws,
        &observed,
        &retained,
        "reconcile",
        "d23-omit",
        &mut hint_cx,
    )
    .expect("omission stays complete");
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|row| row.token != "wB"));
    clear_pending_seams();
}

#[test]
fn untagged_absence_keeps_drop_only_behavior() {
    // Members without a stored lifetime tag (never admitted through the
    // tag stamp) keep the exact prior behavior: drop with no report, so
    // no empty id is ever fabricated onto the wire.
    clear_pending_seams();
    let (mut state, ws, domain) = setup("mon-1");
    let output = "mon-1".to_owned();
    let rect = bounds();
    wire_member(&mut state, &output, &ws, 101, "wA", rect);
    state.member_tags.remove(&key(101));
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
    state.last_hwnds = std::collections::HashSet::new();
    workspace_close_cleanup(&mut state);
    assert!(
        state.pending_closed.is_empty(),
        "untagged absence reports nothing"
    );
    assert!(
        !state.member_tokens.contains_key(&key(101)),
        "member still drops"
    );
    clear_pending_seams();
}

#[test]
fn release_confirmed_discards_homed_closes_keeps_remote() {
    // A confirmed `ReleaseDomain` drops the domain's Engine slots, so
    // queued closes homed there must drop with them (nothing left to
    // forget); other domains' queues survive for their own legs.
    clear_pending_seams();
    let (mut state, ws1, domain1) = setup("mon-1");
    let output1 = "mon-1".to_owned();
    state.workspaces.ensure_output("mon-2");
    let output2 = "mon-2".to_owned();
    let ws2 = state.workspaces.active_id(&output2).expect("active");
    let domain2 = crate::workspace_owner::workspace_domain(&output2, &ws2, bounds(), 8);
    let rect = bounds();
    wire_member(&mut state, &output1, &ws1, 101, "wA", rect);
    wire_member(&mut state, &output2, &ws2, 201, "wX", rect);
    let observed1 = vec![mk_observed(101, "wA", 0, 0)];
    let _ = dispatch(
        &mut state,
        &output1,
        &ws1,
        &domain1,
        &observed1,
        &[],
        Some("wA"),
        "d23-seed1",
    );
    let observed2 = vec![mk_observed(201, "wX", 0, 0)];
    let _ = dispatch(
        &mut state,
        &output2,
        &ws2,
        &domain2,
        &observed2,
        &[],
        Some("wX"),
        "d23-seed2",
    );
    // Both sole members close: each domain queues its verified close.
    state.last_hwnds = std::collections::HashSet::new();
    inject_pending_tag(101, None);
    inject_pending_tag(201, None);
    workspace_close_cleanup(&mut state);
    assert_eq!(state.pending_closed.len(), 2);
    // Releasing mon-2 through the real release path confirms and drops
    // only mon-2's queue entry.
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
    let released =
        dispatch_workspace_release(&mut state, &output2, &ws2, &areas, &[], &[], "d23-rel");
    assert!(released, "empty-domain release confirms");
    assert!(
        !state.pending_closed.contains_key("wX"),
        "released-domain close discards with its slots"
    );
    assert!(
        state.pending_closed.contains_key("wA"),
        "remote-domain close survives the release"
    );
    // mon-1 still delivers its own close on the next valid observation.
    let reply = dispatch(
        &mut state,
        &output1,
        &ws1,
        &domain1,
        &[],
        &[],
        None,
        "d23-close1",
    );
    assert!(
        matches!(
            reply,
            tiler_core::boundary::CoreReply::Projection(_)
                | tiler_core::boundary::CoreReply::Tiled(_)
        ),
        "mon-1 close leg must converge"
    );
    assert!(state.pending_closed.is_empty());
    clear_pending_seams();
}

#[test]
fn audit_recovery_classifies_dead_live_and_uncertain() {
    // The hidden-claim audit seam drives the real classification:
    // destroyed HWNDs retire + queue, live claims retain, access-denied
    // retains, and a removed nonce on a live window retires (existing
    // classification) but never queues (tag guard).
    clear_pending_seams();
    let (mut state, ws, _domain) = setup("mon-1");
    let output = "mon-1".to_owned();
    let rect = bounds();
    let me = me();
    let (store, tmp) = tmp_store("audit");
    let live_process = || crate::model::ProcessIdentity {
        pid: 1101,
        process_creation: format!("c{:016x}", 101u64),
        user_sid: "sid".to_owned(),
        session_id: 1,
        exe_path: "app.exe".to_owned(),
    };
    let live_snap = || crate::product_hide::sys::ProductSnapshot {
        hwnd: 101,
        pid: 1101,
        process: live_process(),
        nonce: Some("claim-w".to_owned()),
        visible: false,
        iconic: false,
        maximized: false,
        class: String::new(),
    };
    // Destroyed: retires the claim, drops the member, queues the close.
    wire_member(&mut state, &output, &ws, 101, "wH", rect);
    hide_member(&mut state, 101, "claim-w", false);
    inject_pending_recovery(101, Err("absent: hwnd destroyed".to_owned()));
    inject_pending_tag(101, None);
    audit_hidden_claims(&mut state, &me, &store);
    assert!(
        !state.hidden_claims.contains_key(&key(101)),
        "dead claim retires"
    );
    assert!(
        !state.member_tokens.contains_key(&key(101)),
        "dead member drops"
    );
    assert!(
        state.pending_closed.contains_key("wH"),
        "verified hidden close queues"
    );
    // Live claim with matching process + nonce: retains everything.
    wire_member(&mut state, &output, &ws, 101, "wH", rect);
    hide_member(&mut state, 101, "claim-w", false);
    state.pending_closed.clear();
    inject_pending_recovery(101, Ok(live_snap()));
    inject_pending_tag(101, Some("tag-wH".to_owned()));
    audit_hidden_claims(&mut state, &me, &store);
    assert!(
        state.hidden_claims.contains_key(&key(101)),
        "live claim retained"
    );
    assert!(
        state.member_tokens.contains_key(&key(101)),
        "live member retained"
    );
    assert!(state.pending_closed.is_empty(), "live claims never queue");
    // Access-denied on a live window: uncertain, retains with ledger.
    inject_pending_recovery(101, Err("uncertain: target access-denied".to_owned()));
    audit_hidden_claims(&mut state, &me, &store);
    assert!(
        state.hidden_claims.contains_key(&key(101)),
        "uncertain retains the ledger"
    );
    assert!(
        state.member_tokens.contains_key(&key(101)),
        "uncertain retains membership"
    );
    assert!(state.pending_closed.is_empty(), "uncertain never queues");
    // Nonce removed but window live (tag still reads): existing
    // classification retires the claim, yet the tag guard refuses the
    // close report, so a live window is never reported.
    let mut bare = live_snap();
    bare.nonce = None;
    inject_pending_recovery(101, Ok(bare));
    inject_pending_tag(101, Some("tag-wH".to_owned()));
    audit_hidden_claims(&mut state, &me, &store);
    assert!(
        !state.hidden_claims.contains_key(&key(101)),
        "nonce-mismatched claim retires (existing classification)"
    );
    assert!(
        state.pending_closed.is_empty(),
        "live tag refuses the close report"
    );
    // Recycled HWND (same number, foreign process): retires + queues,
    // since the claimed window is verifiably gone.
    wire_member(&mut state, &output, &ws, 101, "wH", rect);
    hide_member(&mut state, 101, "claim-w", false);
    let mut foreign = live_snap();
    foreign.process.exe_path = "other.exe".to_owned();
    inject_pending_recovery(101, Ok(foreign));
    inject_pending_tag(101, None);
    audit_hidden_claims(&mut state, &me, &store);
    assert!(
        !state.hidden_claims.contains_key(&key(101)),
        "recycled claim retires"
    );
    assert!(
        state.pending_closed.contains_key("wH"),
        "recycled close queues: the claimed window is gone"
    );
    let _ = std::fs::remove_dir_all(&tmp);
    clear_pending_seams();
}

#[test]
fn fallback_domain_gone_seeds_fresh_without_slots() {
    // The homing domain is gone (owner restart path drops the Engine
    // session): the next full observation seeds fresh with no restore
    // slots, admitting every carried window as brand new.
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
    state.engine.remove(&domain.1);
    assert!(state.engine.session(&domain.1).is_none(), "domain gone");
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
        "d23-regone",
    );
    assert!(
        matches!(
            reply,
            tiler_core::boundary::CoreReply::Projection(_)
                | tiler_core::boundary::CoreReply::Tiled(_)
        ),
        "regone observation seeds fresh, got {reply:?}"
    );
    let session = state.engine.session(&domain.1).expect("reseeded session");
    assert_eq!(
        session.minimized_count(),
        0,
        "no slots survive a gone domain"
    );
    assert_eq!(
        leaf_window_order(&state, &domain.1),
        vec!["wA".to_owned(), "wB".to_owned(), "wC".to_owned()],
        "all three admit fresh"
    );
    assert_eq!(focused_token(&state, &domain.1).as_deref(), Some("wC"));
    clear_pending_seams();
}

#[test]
fn fallback_sent_elsewhere_admits_fresh_at_destination() {
    // B moves to mon-2 (membership re-homed as a workspace send leaves
    // it) while its mon-1 slot still exists: the destination session
    // holds no slot, so B admits fresh there. The source slot lifecycle
    // on cross-session moves is shared-core behavior (only reported
    // `closed_windows` forget slots); this test pins the destination
    // side and the survivors' convergence only.
    clear_pending_seams();
    let (mut state, ws1, domain1) = setup("mon-1");
    let output1 = "mon-1".to_owned();
    state.workspaces.ensure_output("mon-2");
    let output2 = "mon-2".to_owned();
    let ws2 = state.workspaces.active_id(&output2).expect("active");
    let domain2 = crate::workspace_owner::workspace_domain(&output2, &ws2, bounds(), 8);
    let _ = seed_abc(&mut state, &output1, &ws1, &domain1);
    wire_member(&mut state, &output2, &ws2, 201, "wX", bounds());
    let seed_x = vec![mk_observed(201, "wX", 0, 0)];
    let _ = dispatch(
        &mut state,
        &output2,
        &ws2,
        &domain2,
        &seed_x,
        &[],
        Some("wX"),
        "d23-seedx",
    );
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
    live_tags(&[
        (101, Some("wA")),
        (102, Some("wB")),
        (103, Some("wC")),
        (201, Some("wX")),
    ]);
    let (minimized, retained) = tick_leg(
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
        Vec::new(),
    );
    let _ = dispatch(
        &mut state,
        &output1,
        &ws1,
        &domain1,
        &minimized[..2],
        &retained,
        Some("wC"),
        "d23-min",
    );
    assert!(slot_present(&state, &domain1.1, "wB"));
    // Re-home B on mon-2: the destination observation carries B under
    // the new homing with no slot there.
    state.workspaces.remove_window(&key(102));
    assert!(state.workspaces.assign(key(102), &output2, &ws2, false));
    live_tags(&[(201, Some("wX")), (102, Some("wB"))]);
    let (arrived, retained) = tick_leg(
        &mut state,
        &me,
        &fulls,
        &areas,
        vec![
            mk_observed(101, "wA", 0, 0),
            mk_observed(103, "wC", 0, 0),
            mk_observed(201, "wX", 0, 0),
            mk_observed(102, "wB", 0, 0),
        ],
        Vec::new(),
        Vec::new(),
    );
    let reply = dispatch(
        &mut state,
        &output2,
        &ws2,
        &domain2,
        &arrived[2..],
        &retained,
        Some("wX"),
        "d23-sent",
    );
    assert!(
        matches!(
            reply,
            tiler_core::boundary::CoreReply::Projection(_)
                | tiler_core::boundary::CoreReply::Tiled(_)
        ),
        "destination observation must converge, got {reply:?}"
    );
    assert_eq!(
        leaf_window_order(&state, &domain2.1),
        vec!["wX".to_owned(), "wB".to_owned()],
        "B admits fresh at the destination, never the old slot"
    );
    assert!(
        !slot_present(&state, &domain2.1, "wB"),
        "no slot at the destination"
    );
    // Source survivors converge undisturbed around the omission.
    let (survivors, retained) = tick_leg(
        &mut state,
        &me,
        &fulls,
        &areas,
        vec![
            mk_observed(101, "wA", 0, 0),
            mk_observed(103, "wC", 0, 0),
            mk_observed(201, "wX", 0, 0),
            mk_observed(102, "wB", 0, 0),
        ],
        Vec::new(),
        Vec::new(),
    );
    let reply = dispatch(
        &mut state,
        &output1,
        &ws1,
        &domain1,
        &survivors[..2],
        &retained,
        Some("wC"),
        "d23-src",
    );
    assert!(
        matches!(
            reply,
            tiler_core::boundary::CoreReply::Projection(_)
                | tiler_core::boundary::CoreReply::Tiled(_)
        ),
        "source survivors must converge, got {reply:?}"
    );
    assert_eq!(
        leaf_window_order(&state, &domain1.1),
        vec!["wA".to_owned(), "wC".to_owned()]
    );
    clear_pending_seams();
}

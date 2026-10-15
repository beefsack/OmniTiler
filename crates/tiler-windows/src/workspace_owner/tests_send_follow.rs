use super::*;

#[test]
fn send_follow_and_stay_share_admission_geometry() {
    // WS1 [wA, wB] (mover wB focused), WS2 [wC]: follow and stay commit
    // identical destination admission, source collapse and destination
    // geometry; only the desired focus and the intent flag differ (target
    // mover vs source MRU).
    let bounds = rect(0, 0);
    let mut plans = Vec::new();
    for follow in [true, false] {
        let mut engine = tiler_core::engine::Engine::new();
        let owner = OwnerId::parse("tiler-windows").expect("owner");
        let generation = GenerationId::parse("aa").expect("generation");
        engine.sync_binding(&owner, &generation);
        let source = workspace_domain("mon-a", "ws-1", bounds, 8);
        let target = workspace_domain("mon-a", "ws-2", bounds, 8);
        seed_send_domains(
            &mut engine,
            &owner,
            &generation,
            &source,
            &target,
            &["wA", "wB"],
            &["wC"],
            "wB",
            bounds,
        );
        let plan = send_through_engine(
            &mut engine,
            &owner,
            &generation,
            &source,
            &target,
            &["wA", "wB"],
            &["wC"],
            "wB",
            bounds,
            follow,
        );
        plans.push((source, target, plan, engine));
    }
    let (follow_source, follow_target, follow_plan, follow_engine) = &plans[0];
    let (stay_source, stay_target, stay_plan, stay_engine) = &plans[1];
    // Identical admission: same operation, same placed geometry per
    // window, same mover/target domains.
    assert_eq!(
        std::mem::discriminant(&follow_plan.operation),
        std::mem::discriminant(&stay_plan.operation),
        "same structural operation"
    );
    let mut follow_geometry: Vec<(String, Rect)> = follow_plan
        .geometry
        .iter()
        .map(|g| (g.window.0.clone(), g.rect))
        .collect();
    follow_geometry.sort_by(|a, b| a.0.cmp(&b.0));
    let mut stay_geometry: Vec<(String, Rect)> = stay_plan
        .geometry
        .iter()
        .map(|g| (g.window.0.clone(), g.rect))
        .collect();
    stay_geometry.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        follow_geometry, stay_geometry,
        "follow/stay place identical geometry"
    );
    assert!(
        follow_geometry.iter().any(|(w, _)| w == "wB"),
        "mover admitted"
    );
    assert!(
        follow_geometry.iter().any(|(w, _)| w == "wA"),
        "source survivor reflowed"
    );
    assert!(
        follow_geometry.iter().any(|(w, _)| w == "wC"),
        "destination survivor kept"
    );
    // Only focus differs: follow names the mover on the target, stay
    // names the source focused-removal MRU (wA, never the target).
    let follow_focus = follow_plan.focus_domain.clone().expect("follow focus");
    assert_eq!(follow_focus, follow_target.1, "follow focuses the target");
    let follow_leaf = follow_plan.focus_leaf.clone().expect("follow leaf");
    assert_eq!(
        snapshot_window(follow_engine, &follow_focus, &follow_leaf).as_deref(),
        Some("wB"),
        "follow focuses the mover"
    );
    let stay_focus = stay_plan.focus_domain.clone().expect("stay focus");
    assert_eq!(stay_focus, stay_source.1, "stay focuses the source");
    let stay_leaf = stay_plan.focus_leaf.clone().expect("stay leaf");
    assert_eq!(
        snapshot_window(stay_engine, &stay_focus, &stay_leaf).as_deref(),
        Some("wA"),
        "stay focuses the source MRU, never the mover"
    );
    let _ = (follow_source, stay_target);
}

#[test]
fn send_stay_reports_null_focus_for_a_sole_mover() {
    // WS1 [wB] sole mover, WS2 [wC]: stay empties the source, so the plan
    // carries null focus (no setter; the native removal focus stands).
    // Follow on the same pair still focuses the mover on the target.
    let bounds = rect(0, 0);
    let mut engine = tiler_core::engine::Engine::new();
    let owner = OwnerId::parse("tiler-windows").expect("owner");
    let generation = GenerationId::parse("aa").expect("generation");
    engine.sync_binding(&owner, &generation);
    let source = workspace_domain("mon-a", "ws-1", bounds, 8);
    let target = workspace_domain("mon-a", "ws-2", bounds, 8);
    seed_send_domains(
        &mut engine,
        &owner,
        &generation,
        &source,
        &target,
        &["wB"],
        &["wC"],
        "wB",
        bounds,
    );
    let stay = send_through_engine(
        &mut engine,
        &owner,
        &generation,
        &source,
        &target,
        &["wB"],
        &["wC"],
        "wB",
        bounds,
        false,
    );
    assert_eq!(stay.focus_domain, None, "sole stay has no focus domain");
    assert_eq!(stay.focus_leaf, None, "sole stay has no focus leaf");
    let mut engine = tiler_core::engine::Engine::new();
    engine.sync_binding(&owner, &generation);
    seed_send_domains(
        &mut engine,
        &owner,
        &generation,
        &source,
        &target,
        &["wB"],
        &["wC"],
        "wB",
        bounds,
    );
    let follow = send_through_engine(
        &mut engine,
        &owner,
        &generation,
        &source,
        &target,
        &["wB"],
        &["wC"],
        "wB",
        bounds,
        true,
    );
    assert_eq!(follow.focus_domain, Some(target.1.clone()));
    let leaf = follow.focus_leaf.clone().expect("follow leaf");
    assert_eq!(
        snapshot_window(&engine, &target.1, &leaf).as_deref(),
        Some("wB")
    );
}

#[test]
fn send_hidden_target_admits_but_never_writes() {
    // WS1 [wA, wB], WS2 [wC hidden]: the plan still admits the mover onto
    // the hidden target (placement computed), but the portable writable
    // subset excludes the hidden member, so no target write ever issues
    // while hidden. Stay never reveals; follow reveals through select.
    use std::collections::{BTreeMap, HashSet};
    let bounds = rect(0, 0);
    let mut engine = tiler_core::engine::Engine::new();
    let owner = OwnerId::parse("tiler-windows").expect("owner");
    let generation = GenerationId::parse("aa").expect("generation");
    engine.sync_binding(&owner, &generation);
    let source = workspace_domain("mon-a", "ws-1", bounds, 8);
    let target = workspace_domain("mon-a", "ws-2", bounds, 8);
    seed_send_domains(
        &mut engine,
        &owner,
        &generation,
        &source,
        &target,
        &["wA", "wB"],
        &["wC"],
        "wB",
        bounds,
    );
    let plan = send_through_engine(
        &mut engine,
        &owner,
        &generation,
        &source,
        &target,
        &["wA", "wB"],
        &["wC"],
        "wB",
        bounds,
        false,
    );
    assert!(
        plan.geometry.iter().any(|g| g.window.0 == "wB"),
        "mover admitted onto the hidden target"
    );
    let hidden = key(30);
    let visible_a = key(31);
    let members: BTreeSet<WindowKey> = [hidden.clone(), visible_a.clone()].into_iter().collect();
    let token_of: BTreeMap<WindowKey, String> = BTreeMap::from([
        (hidden.clone(), "wC".to_owned()),
        (visible_a.clone(), "wA".to_owned()),
    ]);
    let fresh: HashSet<String> = ["wC".to_owned(), "wA".to_owned()].into_iter().collect();
    let writable = super::super::writable_subset(&members, |k| k == &hidden, &token_of, &fresh);
    assert!(!writable.contains("wC"), "hidden target never writable");
    assert!(
        writable.contains("wA"),
        "visible source survivor stays writable"
    );
}

#[test]
fn send_fullscreen_carry_matches_maximized_allocation() {
    // Item 9: each overlay kind resolves through retained admission plus
    // the live overlay gate, then takes the identical actual retained
    // Engine allocation in both intents (the Engine never sees overlay
    // flags). Fails if either leg's admission or gate is removed.
    use super::super::{send_overlay_gate, send_retained_overlay_admits};
    let bounds = rect(0, 0);
    let mut allocations = Vec::new();
    for (row_maximized, row_fullscreen) in [(true, false), (false, true)] {
        // Retained resolution, production order: admission first, then the
        // live recheck against the row snapshot (readable stable frame).
        assert!(
            send_retained_overlay_admits(row_maximized, row_fullscreen),
            "overlay leg admits"
        );
        assert_eq!(
            send_overlay_gate(
                row_maximized,
                row_fullscreen,
                row_maximized,
                Some(row_fullscreen),
                row_fullscreen
            ),
            Ok(()),
            "stable overlay leg carries"
        );
        for follow in [true, false] {
            let mut engine = tiler_core::engine::Engine::new();
            let owner = OwnerId::parse("tiler-windows").expect("owner");
            let generation = GenerationId::parse("aa").expect("generation");
            engine.sync_binding(&owner, &generation);
            let source = workspace_domain("mon-a", "ws-1", bounds, 8);
            let target = workspace_domain("mon-a", "ws-2", bounds, 8);
            seed_send_domains(
                &mut engine,
                &owner,
                &generation,
                &source,
                &target,
                &["wA", "wB"],
                &["wC"],
                "wB",
                bounds,
            );
            let plan = send_through_engine(
                &mut engine,
                &owner,
                &generation,
                &source,
                &target,
                &["wA", "wB"],
                &["wC"],
                "wB",
                bounds,
                follow,
            );
            let mut geometry: Vec<(String, Rect)> = plan
                .geometry
                .iter()
                .map(|g| (g.window.0.clone(), g.rect))
                .collect();
            geometry.sort_by(|a, b| a.0.cmp(&b.0));
            allocations.push((
                row_maximized,
                row_fullscreen,
                follow,
                plan.operation,
                geometry,
            ));
        }
    }
    let first = &allocations[0];
    for entry in &allocations[1..] {
        assert_eq!(
            std::mem::discriminant(&entry.3),
            std::mem::discriminant(&first.3),
            "same structural operation across overlay kinds and intents"
        );
        assert_eq!(
            entry.4, first.4,
            "identical allocation across overlay kinds and intents"
        );
    }
    assert!(first.4.iter().any(|(w, _)| w == "wB"), "mover admitted");
    assert!(first.4.iter().any(|(w, _)| w == "wA"), "survivor reflowed");
}

#[test]
fn send_fullscreen_mover_never_takes_geometry_writes() {
    // Item 9: a fullscreen mover classifies out of the eligible
    // observation exactly like maximized, so the portable writable
    // subset (the same `writable_subset` production calls) never
    // includes its retained-only token: no native size/position write.
    use crate::tiling::{SkipReason, classify, classify_focus};
    use std::collections::{BTreeMap, BTreeSet, HashSet};
    let facts = crate::tiling::WindowFacts {
        visible: true,
        minimized: false,
        maximized: false,
        cloaked: false,
        elevated: false,
        shell: false,
        tool_window: false,
        owned: false,
        captionless_fullscreen: true,
        no_activate: false,
        dialog: false,
    };
    assert_eq!(classify(&facts), Err(SkipReason::Fullscreen));
    assert!(classify_focus(&facts).is_ok());
    let key = key(77);
    let members: BTreeSet<WindowKey> = BTreeSet::from([key.clone()]);
    let token_of: BTreeMap<WindowKey, String> = BTreeMap::from([(key.clone(), "wB".to_owned())]);
    let fresh: HashSet<String> = HashSet::new();
    assert!(
        super::super::writable_subset(&members, |_| false, &token_of, &fresh).is_empty(),
        "retained fullscreen token never writable"
    );
    let fresh: HashSet<String> = HashSet::from(["wB".to_owned()]);
    assert!(
        super::super::writable_subset(&members, |_| true, &token_of, &fresh).is_empty(),
        "hidden fullscreen target never writable"
    );
}

#[test]
fn send_fullscreen_stay_keeps_source_mru_and_null_without_setter() {
    // Item 9: stay reuses the existing MRU/null policy unchanged. A
    // non-empty source names the source MRU (never the mover), and the
    // genuine sole-mover Engine null plan resolves Null (no setter).
    use super::super::{StayFocusClass, classify_stay_focus};
    let bounds = rect(0, 0);
    let owner = OwnerId::parse("tiler-windows").expect("owner");
    let generation = GenerationId::parse("aa").expect("generation");
    let mut engine = tiler_core::engine::Engine::new();
    engine.sync_binding(&owner, &generation);
    let source = workspace_domain("mon-a", "ws-1", bounds, 8);
    let target = workspace_domain("mon-a", "ws-2", bounds, 8);
    seed_send_domains(
        &mut engine,
        &owner,
        &generation,
        &source,
        &target,
        &["wA", "wB"],
        &["wC"],
        "wB",
        bounds,
    );
    let stay = send_through_engine(
        &mut engine,
        &owner,
        &generation,
        &source,
        &target,
        &["wA", "wB"],
        &["wC"],
        "wB",
        bounds,
        false,
    );
    let stay_focus = stay.focus_domain.clone().expect("stay focus");
    assert_eq!(stay_focus, source.1, "stay focuses the source");
    let stay_leaf = stay.focus_leaf.clone().expect("stay leaf");
    assert_eq!(
        snapshot_window(&engine, &stay_focus, &stay_leaf).as_deref(),
        Some("wA"),
        "stay names the source MRU, never the mover"
    );
    // Genuine sole-mover null plan from the actual Engine, then the
    // production policy on that plan state: Null means no setter.
    let mut engine = tiler_core::engine::Engine::new();
    engine.sync_binding(&owner, &generation);
    seed_send_domains(
        &mut engine,
        &owner,
        &generation,
        &source,
        &target,
        &["wB"],
        &["wC"],
        "wB",
        bounds,
    );
    let sole = send_through_engine(
        &mut engine,
        &owner,
        &generation,
        &source,
        &target,
        &["wB"],
        &["wC"],
        "wB",
        bounds,
        false,
    );
    assert_eq!(sole.focus_domain, None, "sole stay has no focus domain");
    assert_eq!(sole.focus_leaf, None, "sole stay has no focus leaf");
    let plan_null = sole.focus_domain.is_none() || sole.focus_leaf.is_none();
    assert_eq!(
        classify_stay_focus(plan_null, false, false, false, false, false),
        StayFocusClass::Null,
        "actual null plan resolves Null with no setter"
    );
}

#[test]
fn relative_send_target_freezes_once_across_trailing_and_wrap() {
    // Ring resolution is pure: no activation, no append, no creation.
    // Targets freeze once pre-transfer (filling the trailing empty
    // invokes ordinary lifecycle for the next spare afterwards).
    let mut m = ManagedWorkspaces::new();
    m.ensure_output("mon-1");
    for i in 0..10u64 {
        let ids: Vec<String> = m.workspace_ids("mon-1");
        for ws in &ids {
            let members = m.workspace_members("mon-1", ws);
            if members.is_empty() {
                m.assign(
                    WindowKey {
                        hwnd: 900 + i,
                        pid: 1000 + i as u32,
                        creation: format!("c{i:016x}"),
                    },
                    "mon-1",
                    ws,
                    false,
                );
                break;
            }
        }
        let _ = m.select_trailing("mon-1");
    }
    assert!(m.workspace_count("mon-1") > 9, "ordinals run past 9");
    let ring = m.scoped_ring_ids("mon-1");
    let first = ring.first().cloned().expect("first");
    let last = ring.last().cloned().expect("last");
    let mid = ring[ring.len() / 2].clone();
    // Wrap both ends, trailing empty included, ordinals beyond 9 in ring.
    assert_eq!(
        m.resolve_relative_from("mon-1", &first, -1).as_deref(),
        Some(last.as_str())
    );
    assert_eq!(
        m.resolve_relative_from("mon-1", &last, 1).as_deref(),
        Some(first.as_str())
    );
    let at = ring.iter().position(|id| id == &mid).expect("mid");
    assert_eq!(
        m.resolve_relative_from("mon-1", &mid, -1).as_deref(),
        Some(ring[at - 1].as_str())
    );
    assert_eq!(
        m.resolve_relative_from("mon-1", &mid, 1).as_deref(),
        Some(ring[at + 1].as_str())
    );
    // Refusals: bad delta, unknown output/current, empty ring.
    assert_eq!(m.resolve_relative_from("mon-1", &mid, 0), None);
    assert_eq!(m.resolve_relative_from("mon-1", &mid, 2), None);
    assert_eq!(m.resolve_relative_from("mon-gone", &mid, 1), None);
    assert_eq!(m.resolve_relative_from("mon-1", "ws-gone", 1), None);
    // Pure: resolution never activates, appends, or creates.
    let active = m.active_id("mon-1").expect("active");
    let count = m.workspace_count("mon-1");
    let _ = m.resolve_relative_from("mon-1", &mid, 1);
    let _ = m.resolve_relative("mon-1", 1);
    assert_eq!(m.active_id("mon-1").as_deref(), Some(active.as_str()));
    assert_eq!(m.workspace_count("mon-1"), count);
    // Active-anchored resolution agrees with the source-anchored form at
    // the active view (sends anchor at the mover source instead).
    assert_eq!(
        m.resolve_relative("mon-1", 1),
        m.resolve_relative_from("mon-1", &active, 1)
    );
    // Frozen target: resolving once, then mutating (filling the trailing
    // empty plus lifecycle spare), never rewrites the frozen id.
    let frozen = m
        .resolve_relative_from("mon-1", &first, -1)
        .expect("frozen");
    assert_eq!(frozen, last, "pre-transfer trailing target");
    m.assign(
        WindowKey {
            hwnd: 4242,
            pid: 4242,
            creation: "c000000000004242".to_owned(),
        },
        "mon-1",
        &frozen,
        false,
    );
    let _ = m.resolve_send_trailing("mon-1");
    assert!(
        m.workspace_ids("mon-1").contains(&frozen),
        "frozen target survives lifecycle, never re-resolved"
    );
}

#[test]
fn relative_send_resolves_from_the_mover_source() {
    // Three workspaces with the active view parked on the third: a
    // relative step from the mover's source (first) still steps the
    // source ring, never the active view.
    let mut m = ManagedWorkspaces::new();
    m.ensure_output("mon-1");
    let ring = m.scoped_ring_ids("mon-1");
    assert!(ring.len() >= 2);
    let source = ring[0].clone();
    assert!(m.activate("mon-1", &ring[ring.len() - 1]));
    assert_ne!(m.active_id("mon-1").as_deref(), Some(source.as_str()));
    assert_eq!(
        m.resolve_relative_from("mon-1", &source, 1).as_deref(),
        Some(ring[1].as_str())
    );
    assert_eq!(
        m.resolve_relative_from("mon-1", &source, -1).as_deref(),
        Some(ring.last().expect("last").as_str())
    );
}

#[test]
fn send_flags_stable_gates_both_intents_identically() {
    // Item 20 flag races (KDE `flagsStillMatch` parity): the live overlay
    // flags must still equal the dispatch snapshot or the transfer
    // refuses with no writes. The predicate is intent-independent: follow
    // and stay refuse identically on any flip (both legs of G-D2 carry).
    use crate::tiling::send_flags_stable;
    assert!(send_flags_stable(false, false, false, false));
    assert!(send_flags_stable(false, true, false, true));
    assert!(send_flags_stable(true, false, true, false));
    assert!(send_flags_stable(true, true, true, true));
    assert!(!send_flags_stable(false, true, false, false));
    assert!(!send_flags_stable(false, false, false, true));
    assert!(!send_flags_stable(true, false, false, false));
    assert!(!send_flags_stable(false, true, true, true));
}

#[test]
fn send_fences_hold_pins_view_mode_gap_scope_lifetime_membership_and_overlay() {
    // Production tails MUST route through `send_fences_hold`: every fence
    // below is a live tail-time recheck of a carried pre-plan snapshot,
    // refusing with no further effects and no replay.
    use super::super::{SendFenceLive, SendFenceSnapshot, send_fences_hold};
    let snap = SendFenceSnapshot {
        inner_gap: 8,
        outer_gap: 8,
        source_tiled: true,
        target_tiled: true,
        overlay_maximized: false,
        overlay_fullscreen: false,
    };
    let live = SendFenceLive {
        inner_gap: 8,
        outer_gap: 8,
        source_tiled: true,
        target_tiled: true,
        active_is_source: true,
        target_is_active: false,
        scope_ok: true,
        lifetime_ok: true,
        membership_ok: true,
        overlay_maximized: false,
        overlay_fullscreen: false,
    };
    assert_eq!(send_fences_hold(snap, live), None);
    // Source-selected view fence (both directions).
    assert_eq!(
        send_fences_hold(
            snap,
            SendFenceLive {
                active_is_source: false,
                ..live
            }
        ),
        Some("view-changed")
    );
    assert_eq!(
        send_fences_hold(
            snap,
            SendFenceLive {
                target_is_active: true,
                ..live
            }
        ),
        Some("view-changed")
    );
    // Both workspace modes, both gaps.
    assert_eq!(
        send_fences_hold(
            snap,
            SendFenceLive {
                source_tiled: false,
                ..live
            }
        ),
        Some("mode-changed")
    );
    assert_eq!(
        send_fences_hold(
            snap,
            SendFenceLive {
                target_tiled: false,
                ..live
            }
        ),
        Some("mode-changed")
    );
    assert_eq!(
        send_fences_hold(
            snap,
            SendFenceLive {
                inner_gap: 4,
                ..live
            }
        ),
        Some("deferred")
    );
    assert_eq!(
        send_fences_hold(
            snap,
            SendFenceLive {
                outer_gap: 12,
                ..live
            }
        ),
        Some("deferred")
    );
    // Scope, lifetime, membership.
    assert_eq!(
        send_fences_hold(
            snap,
            SendFenceLive {
                scope_ok: false,
                ..live
            }
        ),
        Some("scope-excluded")
    );
    assert_eq!(
        send_fences_hold(
            snap,
            SendFenceLive {
                lifetime_ok: false,
                ..live
            }
        ),
        Some("identity-changed")
    );
    assert_eq!(
        send_fences_hold(
            snap,
            SendFenceLive {
                membership_ok: false,
                ..live
            }
        ),
        Some("unverified")
    );
    // Item 9: a newly arrived live fullscreen refuses outright, other
    // overlay drift defers.
    assert_eq!(
        send_fences_hold(
            snap,
            SendFenceLive {
                overlay_fullscreen: true,
                ..live
            }
        ),
        Some("send-refused-fullscreen")
    );
    assert_eq!(
        send_fences_hold(
            snap,
            SendFenceLive {
                overlay_maximized: true,
                ..live
            }
        ),
        Some("deferred")
    );
    // Retained maximized carry holds while flags stay stable.
    let snap_max = SendFenceSnapshot {
        overlay_maximized: true,
        ..snap
    };
    assert_eq!(
        send_fences_hold(
            snap_max,
            SendFenceLive {
                overlay_maximized: true,
                ..live
            }
        ),
        None
    );
    assert_eq!(
        send_fences_hold(
            snap_max,
            SendFenceLive {
                overlay_maximized: false,
                ..live
            }
        ),
        Some("deferred")
    );
    // Retained fullscreen carry holds exactly like maximized while flags
    // stay stable; a fullscreen exit defers and a maximized drift on a
    // fullscreen snapshot defers.
    let snap_full = SendFenceSnapshot {
        overlay_fullscreen: true,
        ..snap
    };
    assert_eq!(
        send_fences_hold(
            snap_full,
            SendFenceLive {
                overlay_fullscreen: true,
                ..live
            }
        ),
        None
    );
    assert_eq!(
        send_fences_hold(
            snap_full,
            SendFenceLive {
                overlay_fullscreen: false,
                ..live
            }
        ),
        Some("deferred")
    );
    assert_eq!(
        send_fences_hold(
            snap_full,
            SendFenceLive {
                overlay_fullscreen: true,
                overlay_maximized: true,
                ..live
            }
        ),
        Some("deferred")
    );
}

#[test]
fn send_overlay_gate_refuses_fullscreen_and_defers_drift() {
    // Production pre-plan, post-plan, and pre-effect checks MUST route
    // through `send_overlay_gate` with live OS reads: a newly arrived
    // fullscreen refuses outright, other drift defers with no restore,
    // and stable maximized/fullscreen carry passes (item 9). A
    // fullscreen snapshot with an unreadable live flag defers pre-effect
    // (never trusts the fallback); ordinary/maximized snapshots keep the
    // legacy fallback.
    use super::super::send_overlay_gate;
    assert_eq!(
        send_overlay_gate(false, false, false, Some(false), false),
        Ok(())
    );
    assert_eq!(
        send_overlay_gate(true, false, true, Some(false), false),
        Ok(())
    );
    assert_eq!(
        send_overlay_gate(false, true, false, Some(true), true),
        Ok(())
    );
    assert_eq!(
        send_overlay_gate(true, true, true, Some(true), true),
        Ok(())
    );
    assert_eq!(
        send_overlay_gate(false, false, false, Some(true), false),
        Err("send-refused-fullscreen")
    );
    assert_eq!(
        send_overlay_gate(true, false, true, Some(true), true),
        Err("send-refused-fullscreen")
    );
    assert_eq!(
        send_overlay_gate(false, true, true, Some(true), true),
        Err("deferred")
    );
    assert_eq!(
        send_overlay_gate(false, true, false, Some(false), true),
        Err("deferred")
    );
    assert_eq!(
        send_overlay_gate(true, false, false, Some(false), false),
        Err("deferred")
    );
    assert_eq!(
        send_overlay_gate(false, false, true, Some(false), false),
        Err("deferred")
    );
    // Unreadable live flag: fullscreen snapshots defer, others ride the
    // legacy fallback (unreadable ordinary holds, retained maximized
    // still defers its maximize drift).
    assert_eq!(
        send_overlay_gate(false, true, false, None, true),
        Err("deferred")
    );
    assert_eq!(send_overlay_gate(false, false, false, None, false), Ok(()));
    assert_eq!(
        send_overlay_gate(true, false, false, None, false),
        Err("deferred")
    );
}

#[test]
fn send_retained_overlay_admission_covers_maximized_and_fullscreen() {
    // Production send dispatch MUST route retained movers through
    // `send_retained_overlay_admits`: maximized and fullscreen members
    // resolve through the retained row with a live recheck, never as
    // ordinary; any other retained row never sends as an overlay mover
    // (slotless born-held windows never reach this: no member token
    // means `unmanaged` first).
    use super::super::send_retained_overlay_admits;
    assert!(send_retained_overlay_admits(true, false));
    assert!(send_retained_overlay_admits(false, true));
    assert!(send_retained_overlay_admits(true, true));
    assert!(!send_retained_overlay_admits(false, false));
}

#[test]
fn classify_stay_focus_separates_null_from_stale() {
    // Production stay-focus MUST route through `classify_stay_focus`: a
    // genuine null plan is Null (ok/no-focus, no setter); a some-but-
    // unusable mapping is Stale (focus-unverified, no setter) for every
    // single failure; only the fully exact mapping is Ready.
    use super::super::{StayFocusClass, classify_stay_focus};
    assert_eq!(
        classify_stay_focus(true, false, false, false, false, false),
        StayFocusClass::Null
    );
    assert_eq!(
        classify_stay_focus(true, true, true, false, true, true),
        StayFocusClass::Null,
        "null plan stays Null even with a live mapping"
    );
    assert_eq!(
        classify_stay_focus(false, true, true, false, true, true),
        StayFocusClass::Ready
    );
    for (mapped, in_source, hidden, lifetime_ok, eligible) in [
        (false, false, false, false, false),
        (true, false, false, true, true),
        (true, true, true, true, true),
        (true, true, false, false, true),
        (true, true, false, true, false),
    ] {
        assert_eq!(
            classify_stay_focus(false, mapped, in_source, hidden, lifetime_ok, eligible),
            StayFocusClass::Stale,
            "mapped={mapped} in_source={in_source} hidden={hidden} lifetime={lifetime_ok} eligible={eligible}"
        );
    }
}

#[test]
fn send_event_carries_floating_flags_for_both_intents() {
    // Floating-boundary membership rides the event rows for follow and
    // stay alike (native transfer, tiled-side reflow only); the command
    // intent flag is the only routing difference at this layer.
    let owner = OwnerId::parse("tiler-windows").expect("owner");
    let generation = GenerationId::parse("aa").expect("generation");
    let correlation = CorrelationId::parse("tick-1").expect("correlation");
    let bounds = rect(0, 0);
    let source = workspace_domain("mon-a", "ws-1", bounds, 8);
    let target = workspace_domain("mon-a", "ws-2", bounds, 8);
    for follow in [true, false] {
        let rows = vec![
            OwnerRow {
                token: "w-mover".to_owned(),
                rect: bounds,
                hints: tiler_core::size_hints::WindowSizeHints::none(),
                floating: false,
            },
            OwnerRow {
                token: "w-float".to_owned(),
                rect: bounds,
                hints: tiler_core::size_hints::WindowSizeHints::none(),
                floating: true,
            },
        ];
        let mut event = build_send_event(
            &owner,
            &generation,
            &correlation,
            0,
            5,
            source.clone(),
            target.clone(),
            &rows,
            &[],
            "w-mover",
            8,
            follow,
        )
        .expect("event");
        stamp_send_target(&mut event, &target.1);
        assert!(
            event
                .windows
                .iter()
                .find(|w| w.window.0 == "w-float")
                .expect("float row")
                .floating,
            "follow={follow}: float survivor rides"
        );
        match &event.command {
            CoreCommand::SendToWorkspace {
                window,
                follow: actual,
                ..
            } => {
                assert_eq!(window, "w-mover");
                assert_eq!(*actual, follow, "explicit intent, never inferred");
            }
            _ => panic!("expected send-to-workspace"),
        }
    }
}

#[test]
fn stamp_send_float_overlay_binds_only_floating_maximized_mover() {
    // D16 combo wire (2026-10-13): the focused float mover's exact live
    // maximized flag rides its own send rows so the shared deferred arm
    // binds floating+maximized. Fullscreen rides the existing
    // fixed-admission stamp; tiled maximized movers, sticky movers, and
    // every survivor keep the builder defaults, so no other encoding
    // changes.
    use super::super::stamp_send_float_overlay;
    let owner = OwnerId::parse("tiler-windows").expect("owner");
    let generation = GenerationId::parse("aa").expect("generation");
    let correlation = CorrelationId::parse("tick-1").expect("correlation");
    let bounds = rect(0, 0);
    let source = workspace_domain("mon-a", "ws-1", bounds, 8);
    let target = workspace_domain("mon-a", "ws-2", bounds, 8);
    let rows = |mover_floating: bool| {
        vec![
            OwnerRow {
                token: "w-mover".to_owned(),
                rect: bounds,
                hints: tiler_core::size_hints::WindowSizeHints::none(),
                floating: mover_floating,
            },
            OwnerRow {
                token: "w-survivor".to_owned(),
                rect: bounds,
                hints: tiler_core::size_hints::WindowSizeHints::none(),
                floating: false,
            },
        ]
    };
    // Floating + maximized mover binds maximized on its own rows only.
    let mut event = build_send_event(
        &owner,
        &generation,
        &correlation,
        0,
        5,
        source.clone(),
        target.clone(),
        &rows(true),
        &[],
        "w-mover",
        8,
        true,
    )
    .expect("event");
    stamp_send_float_overlay(&mut event, "w-mover", true, true);
    assert!(
        event
            .windows
            .iter()
            .find(|w| w.window.0 == "w-mover")
            .expect("mover")
            .maximized,
        "floating maximized mover rides the wire"
    );
    assert!(
        !event
            .windows
            .iter()
            .find(|w| w.window.0 == "w-survivor")
            .expect("survivor")
            .maximized,
        "survivor encoding unchanged"
    );
    // Ordinary float mover: no maximized flag.
    let mut event = build_send_event(
        &owner,
        &generation,
        &correlation,
        0,
        5,
        source.clone(),
        target.clone(),
        &rows(true),
        &[],
        "w-mover",
        8,
        true,
    )
    .expect("event");
    stamp_send_float_overlay(&mut event, "w-mover", true, false);
    assert!(
        !event
            .windows
            .iter()
            .find(|w| w.window.0 == "w-mover")
            .expect("mover")
            .maximized,
        "ordinary float carries no overlay"
    );
    // Tiled maximized mover: never rides the wire (KDE parity).
    let mut event = build_send_event(
        &owner,
        &generation,
        &correlation,
        0,
        5,
        source.clone(),
        target.clone(),
        &rows(false),
        &[],
        "w-mover",
        8,
        true,
    )
    .expect("event");
    stamp_send_float_overlay(&mut event, "w-mover", false, true);
    assert!(
        !event
            .windows
            .iter()
            .find(|w| w.window.0 == "w-mover")
            .expect("mover")
            .maximized,
        "tiled maximized never rides the wire"
    );
}

// Item 2 follow/stay through the real retained Engine (offline
// verification; native hide/reveal/focus/hook wiring is Windows-only and
// stays user-owned live). A maximized mover rides this same ordinary
// Engine route (the retained overlay skip and live flag races are
// adapter-native, preserved unchanged): admission equality here plus the
// flag predicate below is the portable half of the item 20 check.

#[allow(clippy::too_many_arguments)]
fn seed_send_domains(
    engine: &mut tiler_core::engine::Engine,
    owner: &OwnerId,
    generation: &GenerationId,
    source: &(OutputDomain, DomainKey),
    target: &(OutputDomain, DomainKey),
    source_tokens: &[&str],
    target_tokens: &[&str],
    mover: &str,
    bounds: Rect,
) {
    // Target converges first so the source focus (the mover) is the last
    // focus the Engine sees; the send below must carry the focused mover.
    for (domain, tokens, focused) in [
        (target, target_tokens, None),
        (
            source,
            source_tokens,
            Some(tiler_core::directional::WindowId(mover.to_owned())),
        ),
    ] {
        let correlation = CorrelationId::parse("seed").expect("correlation");
        let revision = engine
            .session(&domain.1)
            .map(|s| s.accepted_revision())
            .unwrap_or(0);
        let event = crate::tiling::build_reconcile_event_for(
            owner,
            generation,
            &correlation,
            revision,
            tokens.len() as u64,
            &domain.0,
            &domain.1,
            8,
            &tokens
                .iter()
                .map(|t| {
                    (
                        tiler_core::directional::WindowId((*t).to_owned()),
                        bounds,
                        tiler_core::size_hints::WindowSizeHints::none(),
                    )
                })
                .collect::<Vec<_>>(),
            focused.as_ref(),
        );
        let reply = engine.handle(&event);
        assert!(
            matches!(
                reply,
                tiler_core::boundary::CoreReply::Projection(_)
                    | tiler_core::boundary::CoreReply::Tiled(_)
                    | tiler_core::boundary::CoreReply::SendWorkspace(_)
            ),
            "seed converges, got {reply:?}"
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn send_through_engine(
    engine: &mut tiler_core::engine::Engine,
    owner: &OwnerId,
    generation: &GenerationId,
    source: &(OutputDomain, DomainKey),
    target: &(OutputDomain, DomainKey),
    source_tokens: &[&str],
    target_tokens: &[&str],
    mover: &str,
    bounds: Rect,
    follow: bool,
) -> tiler_core::boundary::SendWorkspacePlan {
    use tiler_core::boundary::CoreReply;
    let revision = engine
        .session(&source.1)
        .map(|s| s.accepted_revision())
        .unwrap_or(0);
    let correlation = CorrelationId::parse("tick-1").expect("correlation");
    let rows: Vec<OwnerRow> = source_tokens
        .iter()
        .map(|t| OwnerRow {
            token: (*t).to_owned(),
            rect: bounds,
            hints: tiler_core::size_hints::WindowSizeHints::none(),
            floating: false,
        })
        .collect();
    let target_rows: Vec<OwnerRow> = target_tokens
        .iter()
        .map(|t| OwnerRow {
            token: (*t).to_owned(),
            rect: bounds,
            hints: tiler_core::size_hints::WindowSizeHints::none(),
            floating: false,
        })
        .collect();
    let mut event = build_send_event(
        owner,
        generation,
        &correlation,
        revision,
        42,
        source.clone(),
        target.clone(),
        &rows,
        &target_rows,
        mover,
        8,
        follow,
    )
    .expect("event");
    stamp_send_target(&mut event, &target.1);
    let reply = engine.handle(&event);
    let CoreReply::SendWorkspace(plan) = reply else {
        panic!("send commits, got {reply:?}");
    };
    assert_eq!(plan.follow, follow, "plan echoes the explicit intent");
    plan
}

fn snapshot_window(
    engine: &tiler_core::engine::Engine,
    domain: &DomainKey,
    leaf: &tiler_core::directional::NodeId,
) -> Option<String> {
    engine
        .session(domain)?
        .snapshot()
        .windows
        .iter()
        .find(|link| {
            &link.leaf == leaf && link.output == domain.output && link.workspace == domain.workspace
        })
        .map(|link| link.window.0.clone())
}

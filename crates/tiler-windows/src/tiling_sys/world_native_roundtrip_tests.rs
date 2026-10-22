// Whole-workspace migration roundtrip regressions over the production seam.
//
// A->B and B->A migration with occupied workspaces on both outputs, then a
// select into the occupied hidden workspace, then a close of a
// twice-migrated member. Split from the composition suite (line-count
// bound); no semantic change. Shared rig helpers (env_with,
// dmit_bare, script_live, live_tick, 	wo_output_areas, ...) come
// from the parent module via use super::*;.

use super::*;

#[test]
fn source_refill_migration_arrival_and_views_settle() {
    let mut env = env_with(208_001);
    env.state.ensure_device(DEV2);
    let o1 = env.state.output_token(DEV1);
    let o2 = env.state.output_token(DEV2);
    let (areas, fulls) = two_output_areas();
    let a1 = win_key(208_001, 2211, "proc-121");
    let a2 = win_key(208_002, 2212, "proc-122");
    admit_bare(&mut env, &a1, "w1");
    admit_bare(&mut env, &a2, "w2");
    script_live(&a1, "w1", rect(0, 0, 800, 600), plain_facts());
    script_live(&a2, "w2", rect(0, 610, 800, 400), plain_facts());
    let mut owner = WorldOwner::bootstrap_ids(ADAPTER, GENERATION).expect("owner");
    let observed = live_visible_observed();
    live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        0,
        None,
    );
    let ws_m = owner
        .engine()
        .logical_view(&o1)
        .expect("migrated workspace");
    // Migrate the whole workspace o1 -> o2 through the real command.
    let observed = live_visible_observed();
    let moved = live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        0,
        Some(WorldCommand::MigrateWorkspace {
            source_output: OutputId(o1.clone()),
            target_output: OutputId(o2.clone()),
            workspace: ws_m.clone(),
        }),
    );
    let migration_op = moved
        .reply
        .transfers
        .iter()
        .find(|t| matches!(t.kind, TransferKind::Migration))
        .expect("migration retained")
        .operation_id
        .clone();
    let _refill_op = moved
        .reply
        .transfers
        .iter()
        .find(|t| t.operation_id == format!("{migration_op}-refill"))
        .expect("source refill retained")
        .operation_id
        .clone();
    // Tick 1 actuates member arrival; the source refill view cannot prove
    // until every member arrival is proven, so the migration holds.
    let arrivals: Vec<_> = moved
        .members
        .iter()
        .filter(|a| a.operation_id == migration_op)
        .collect();
    assert!(
        !arrivals.is_empty(),
        "member attempts ride: {:?}",
        moved.members
    );
    assert!(
        arrivals
            .iter()
            .all(|a| a.post.as_ref() == Some(&(OutputId(o2.clone()), ws_m.clone()))),
        "arrival proves: {:?}",
        moved.members
    );
    assert!(
        moved
            .reply
            .transfers
            .iter()
            .any(|t| t.operation_id == migration_op),
        "migration holds for the refill: {:?}",
        moved.reply.transfers
    );
    for token in ["w1", "w2"] {
        assert_eq!(
            env.state.applied_homing.get(token),
            Some(&(o2.clone(), ws_m.clone())),
            "{token} arrival proves"
        );
    }
    // Tick 2 proves the refill view behind the arrived members.
    let observed = live_visible_observed();
    let proved = live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        0,
        None,
    );
    // Tick 2: members and the target view already prove, so the seam
    // dispatches nothing; the refill echo from the next observation is
    // what settles the parent. Assert the hold plus the proven receipts
    // rather than a redundant re-dispatch.
    assert!(proved.members.is_empty() && proved.views.is_empty());
    assert!(
        proved
            .reply
            .transfers
            .iter()
            .any(|t| t.operation_id == migration_op),
        "migration holds for the refill echo: {:?}",
        proved.reply.transfers
    );
    // Tick 3 settles: no migration flight survives proven arrival.
    let observed = live_visible_observed();
    let settled = live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        0,
        None,
    );
    assert!(
        settled
            .reply
            .transfers
            .iter()
            .all(|t| !matches!(t.kind, TransferKind::Migration)),
        "migration settles: {:?}",
        settled.reply.transfers
    );
}

#[test]
fn occupied_target_migration_arrival_proves_without_deferred_view() {
    // Live-proportionate migration: three members move onto an occupied
    // target output while the deferred target view still names the old
    // workspace. Member arrival must prove through the core-published
    // actuation frame's screen affinity (never the deferred view, never
    // the setter boolean): the pre-fix seam held here with zero membership
    // writes because the physical proof demanded the not-yet-flipped view.
    // Two occupants ride the target's other workspace with working
    // hide legs so the target-view batch can complete after arrival.
    let mut env = env_with(208_101);
    env.state.ensure_device(DEV2);
    let o1 = env.state.output_token(DEV1);
    let o2 = env.state.output_token(DEV2);
    let (areas, fulls) = two_output_areas();
    let a1 = win_key(208_101, 2211, "proc-121");
    let a2 = win_key(208_102, 2212, "proc-122");
    let a3 = win_key(208_103, 2213, "proc-123");
    let v1 = win_key(208_111, 2231, "proc-131");
    let v2 = win_key(208_112, 2232, "proc-132");
    for (key, token) in [
        (&a1, "w1"),
        (&a2, "w2"),
        (&a3, "w3"),
        (&v1, "v1"),
        (&v2, "v2"),
    ] {
        admit_bare(&mut env, key, token);
    }
    script_live(&a1, "w1", rect(10, 10, 600, 400), plain_facts());
    script_live(&a2, "w2", rect(10, 420, 600, 400), plain_facts());
    script_live(&a3, "w3", rect(620, 10, 600, 400), plain_facts());
    script_live(&v1, "v1", rect(2000, 10, 700, 500), plain_facts());
    script_live(&v2, "v2", rect(2800, 10, 700, 500), plain_facts());
    for key in [&v1, &v2] {
        S_ADMIT.with(|m| {
            m.borrow_mut()
                .insert(key.hwnd, Ok(script_claim(key.hwnd, &identity_of(key))))
        });
        S_HIDE.with(|m| {
            m.borrow_mut()
                .insert(key.hwnd, Ok(valid_claim(key.hwnd, &identity_of(key))))
        });
        S_REVEAL.with(|m| m.borrow_mut().insert(key.hwnd, Ok(Revealed::Shown)));
    }
    let mut owner = WorldOwner::bootstrap_ids(ADAPTER, GENERATION).expect("owner");
    let observed = live_visible_observed();
    live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        0,
        None,
    );
    let ws_m = owner
        .engine()
        .logical_view(&o1)
        .expect("migrated workspace");
    // Production route: direction-only command off the live source output,
    // exactly like the chord and CLI dispatches.
    let observed = live_visible_observed();
    let moved = live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        0,
        Some(WorldCommand::MigrateDirectional {
            source_output: OutputId(o1.clone()),
            workspace: ws_m.clone(),
            direction: "right".to_owned(),
        }),
    );
    let migration_op = moved
        .reply
        .transfers
        .iter()
        .find(|t| matches!(t.kind, TransferKind::Migration))
        .expect("migration retained")
        .operation_id
        .clone();
    // All three members prove arrival in the command tick even though the
    // target view still names the occupants' workspace natively: the
    // core-published frames land on the target output. The migration itself
    // holds only for the deferred source refill.
    let arrivals: Vec<_> = moved
        .members
        .iter()
        .filter(|a| a.operation_id == migration_op)
        .collect();
    assert_eq!(
        arrivals.len(),
        3,
        "member attempts ride: {:?}",
        moved.members
    );
    assert!(
        arrivals
            .iter()
            .all(|a| a.post.as_ref() == Some(&(OutputId(o2.clone()), ws_m.clone()))),
        "arrival proves: {:?}",
        moved.members
    );
    assert!(
        moved
            .reply
            .transfers
            .iter()
            .any(|t| t.operation_id == migration_op),
        "migration holds only for the refill: {:?}",
        moved.reply.transfers
    );
    for token in ["w1", "w2", "w3"] {
        assert_eq!(
            env.state.applied_homing.get(token),
            Some(&(o2.clone(), ws_m.clone())),
            "{token} arrival proves without the deferred view"
        );
    }
    let live = live_visible_observed();
    for token in ["w1", "w2", "w3"] {
        let frame = live
            .iter()
            .find(|w| w.token == token)
            .expect("mover stays visible")
            .visible;
        assert!(
            frame.x >= 1920,
            "{token} physically lands on the target output: {frame:?}"
        );
    }
    // Tick 2 proves the refill view behind the arrived members.
    let observed = live_visible_observed();
    assert_eq!(
        observed.len(),
        3,
        "occupants hide, movers stay: {observed:?}"
    );
    let proved = live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        0,
        None,
    );
    // Tick 2: members and the target view already prove, so the seam
    // dispatches nothing; the refill echo from the next observation is
    // what settles the parent.
    assert!(proved.members.is_empty() && proved.views.is_empty());
    assert!(
        proved
            .reply
            .transfers
            .iter()
            .any(|t| t.operation_id == migration_op),
        "migration holds for the refill echo: {:?}",
        proved.reply.transfers
    );
    // Tick 3 settles: no migration flight survives proven arrival.
    let observed = live_visible_observed();
    let settled = live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        0,
        None,
    );
    assert!(
        settled
            .reply
            .transfers
            .iter()
            .all(|t| !matches!(t.kind, TransferKind::Migration)),
        "migration settles: {:?}",
        settled.reply.transfers
    );
}

/// Asymmetric migration topology: a large negative-origin source and a
/// small y-offset target. A blind FULL-delta translation of a source-edge
/// member lands past the target's exclusive edge with zero overlap, so
/// only the core-published target slot can place and prove arrival.
fn asymmetric_migration_areas() -> (Vec<super::super::MonitorArea>, Vec<Rect>) {
    let areas = vec![
        super::super::MonitorArea {
            device: DEV1.to_owned(),
            work: rect(-2560, 0, 2560, 1380),
            full: rect(-2560, 0, 2560, 1440),
        },
        super::super::MonitorArea {
            device: DEV2.to_owned(),
            work: rect(0, 100, 640, 320),
            full: rect(0, 100, 640, 360),
        },
    ];
    let fulls = vec![rect(-2560, 0, 2560, 1440), rect(0, 100, 640, 360)];
    (areas, fulls)
}

/// The published migration slot must sit fully inside the target work
/// area: the live defect class stranded translated members beyond it.
fn assert_migration_slot(reply: &WorldReply, window: &str, work: Rect) -> Rect {
    let row = reply
        .windows
        .iter()
        .find(|w| w.lifetime.0 == window)
        .expect("member row");
    assert_eq!(
        row.membership_authority,
        Auth::Controlled,
        "{window} membership desired"
    );
    assert_eq!(
        row.frame_authority,
        Auth::Controlled,
        "{window} core owns the target geometry, never a native remap guess"
    );
    let slot = row
        .effective_frame
        .expect("{window} core publishes the actionable target slot pre-arrival");
    assert!(
        slot.x >= work.x
            && slot.y >= work.y
            && slot.x + slot.w <= work.x + work.w
            && slot.y + slot.h <= work.y + work.h,
        "{window} slot {slot:?} inside work {work:?}"
    );
    slot
}

#[test]
fn asymmetric_migration_members_land_on_published_slots_both_prove() {
    // Producer-shaped migration hazard: two members move from a large
    // negative-origin source onto a small offset target whose other
    // workspace stays occupied (hidden batch after arrival). Pre-fix the
    // edge member's blind FULL-delta translation would land past the
    // target exclusive edge with zero overlap: no affinity, no proof,
    // PossiblyExecuted while the window walks further each retry. With
    // core-owned slots both members place exactly and prove in the
    // command tick.
    let mut env = env_with(208_201);
    env.state.ensure_device(DEV2);
    let o1 = env.state.output_token(DEV1);
    let o2 = env.state.output_token(DEV2);
    let (areas, fulls) = asymmetric_migration_areas();
    let a1 = win_key(208_201, 2241, "proc-141");
    let a2 = win_key(208_202, 2242, "proc-142");
    let v1 = win_key(208_211, 2251, "proc-151");
    let v2 = win_key(208_212, 2252, "proc-152");
    for (key, token) in [(&a1, "w1"), (&a2, "w2"), (&v1, "v1"), (&v2, "v2")] {
        admit_bare(&mut env, key, token);
    }
    script_live(&a1, "w1", rect(-2500, 10, 400, 300), plain_facts());
    script_live(&a2, "w2", rect(-2500, 400, 400, 300), plain_facts());
    script_live(&v1, "v1", rect(10, 110, 200, 100), plain_facts());
    script_live(&v2, "v2", rect(220, 110, 200, 100), plain_facts());
    for key in [&v1, &v2] {
        S_ADMIT.with(|m| {
            m.borrow_mut()
                .insert(key.hwnd, Ok(script_claim(key.hwnd, &identity_of(key))))
        });
        S_HIDE.with(|m| {
            m.borrow_mut()
                .insert(key.hwnd, Ok(valid_claim(key.hwnd, &identity_of(key))))
        });
        S_REVEAL.with(|m| m.borrow_mut().insert(key.hwnd, Ok(Revealed::Shown)));
    }
    let mut owner = WorldOwner::bootstrap_ids(ADAPTER, GENERATION).expect("owner");
    let observed = live_visible_observed();
    live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        0,
        None,
    );
    // Sanity: negative-origin source homed both members on o1.
    for token in ["w1", "w2"] {
        let home = env.state.applied_homing.get(token).expect("homed");
        assert_eq!(home.0, o1, "{token} starts on the source output");
    }
    let ws_m = owner
        .engine()
        .logical_view(&o1)
        .expect("migrated workspace");
    let observed = live_visible_observed();
    let moved = live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        0,
        Some(WorldCommand::MigrateDirectional {
            source_output: OutputId(o1.clone()),
            workspace: ws_m.clone(),
            direction: "right".to_owned(),
        }),
    );
    let migration_op = moved
        .reply
        .transfers
        .iter()
        .find(|t| matches!(t.kind, TransferKind::Migration))
        .expect("migration retained")
        .operation_id
        .clone();
    // Both members carry actionable slots inside the small target work
    // area and prove arrival in the command tick; the migration itself
    // holds only for the deferred source refill.
    let work2 = areas[1].work;
    let mut slots = Vec::new();
    for token in ["w1", "w2"] {
        slots.push(assert_migration_slot(&moved.reply, token, work2));
    }
    let arrivals: Vec<_> = moved
        .members
        .iter()
        .filter(|a| a.operation_id == migration_op)
        .collect();
    assert_eq!(
        arrivals.len(),
        2,
        "member attempts ride: {:?}",
        moved.members
    );
    assert!(
        arrivals
            .iter()
            .all(|a| a.post.as_ref() == Some(&(OutputId(o2.clone()), ws_m.clone()))),
        "arrival proves: {:?}",
        moved.members
    );
    assert!(
        moved
            .reply
            .transfers
            .iter()
            .any(|t| t.operation_id == migration_op),
        "migration holds only for the refill: {:?}",
        moved.reply.transfers
    );
    for token in ["w1", "w2"] {
        assert_eq!(
            env.state.applied_homing.get(token),
            Some(&(o2.clone(), ws_m.clone())),
            "{token} arrival proves without the deferred view"
        );
    }
    let live = live_visible_observed();
    for (token, slot) in ["w1", "w2"].iter().zip(slots.iter()) {
        let frame = live
            .iter()
            .find(|w| &w.token == token)
            .expect("mover stays visible")
            .visible;
        assert_eq!(frame, *slot, "{token} physically lands on the slot");
    }
    // Tick 2 proves the refill view behind the arrived members.
    let observed = live_visible_observed();
    let proved = live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        0,
        None,
    );
    // Tick 2: members and the target view already prove, so the seam
    // dispatches nothing; the refill echo from the next observation is
    // what settles the parent.
    assert!(proved.members.is_empty() && proved.views.is_empty());
    assert!(
        proved
            .reply
            .transfers
            .iter()
            .any(|t| t.operation_id == migration_op),
        "migration holds for the refill echo: {:?}",
        proved.reply.transfers
    );
    // Tick 3 settles: no migration flight survives proven arrival.
    let observed = live_visible_observed();
    let settled = live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        0,
        None,
    );
    assert!(
        settled
            .reply
            .transfers
            .iter()
            .all(|t| !matches!(t.kind, TransferKind::Migration)),
        "migration settles: {:?}",
        settled.reply.transfers
    );
}
/// Live-leg roundtrip regression (migrate-ws-roundtrip7 findings): two-way
/// migration with occupied workspaces on both outputs, then a select into
/// the occupied hidden workspace, then a close of a twice-migrated member.
/// The one-way occupied-target test above settles in three ticks and never
/// covers either finding, so this test drives the full composition:
///
/// 1. post-roundtrip select into the occupied hidden workspace must reveal
///    it natively (never an accepted model-only advance with the old unit
///    still visible and the hidden member re-tiled in place);
/// 2. closing a twice-migrated member must reflow the survivors with real
///    geometry writes and settle every migration flight (never a
///    held:true/possibly-executed parent with zero writes pinning later
///    ticks).
///
/// Assertions read independent readbacks only (`S_VISIBLE`, observed
/// frames, `S_SET_CALLS` writes, applied receipts): never setter echoes.
#[test]
fn roundtrip_migration_select_occupied_then_close_reflows_and_settles() {
    let mut env = env_with(208_301);
    env.state.ensure_device(DEV2);
    let o1 = env.state.output_token(DEV1);
    let o2 = env.state.output_token(DEV2);
    let (areas, fulls) = two_output_areas();
    let m1 = win_key(208_301, 2311, "proc-231");
    let m2 = win_key(208_302, 2312, "proc-232");
    let m3 = win_key(208_303, 2313, "proc-233");
    let r1 = win_key(208_304, 2314, "proc-234");
    let n1 = win_key(208_311, 2321, "proc-241");
    let n2 = win_key(208_312, 2322, "proc-242");
    for (key, token) in [
        (&m1, "w1"),
        (&m2, "w2"),
        (&m3, "w3"),
        (&r1, "w4"),
        (&n1, "v1"),
        (&n2, "v2"),
    ] {
        admit_bare(&mut env, key, token);
        S_ADMIT.with(|m| {
            m.borrow_mut()
                .insert(key.hwnd, Ok(script_claim(key.hwnd, &identity_of(key))))
        });
        S_HIDE.with(|m| {
            m.borrow_mut()
                .insert(key.hwnd, Ok(valid_claim(key.hwnd, &identity_of(key))))
        });
        S_REVEAL.with(|m| m.borrow_mut().insert(key.hwnd, Ok(Revealed::Shown)));
    }
    script_live(&m1, "w1", rect(10, 10, 600, 400), plain_facts());
    script_live(&m2, "w2", rect(10, 420, 600, 400), plain_facts());
    script_live(&m3, "w3", rect(620, 10, 600, 400), plain_facts());
    script_live(&r1, "w4", rect(620, 420, 600, 400), plain_facts());
    script_live(&n1, "v1", rect(1930, 10, 700, 500), plain_facts());
    script_live(&n2, "v2", rect(2730, 10, 700, 500), plain_facts());
    let mut owner = WorldOwner::bootstrap_ids(ADAPTER, GENERATION).expect("owner");
    let observed = live_visible_observed();
    live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        0,
        None,
    );
    let ws_m = owner
        .engine()
        .logical_view(&o1)
        .expect("migrated workspace");
    let index_of = |owner: &WorldOwner, out: &str, id: &str| {
        owner
            .engine()
            .logical_ring(out)
            .iter()
            .position(|w| w == id)
            .expect("in ring") as u8
            + 1
    };
    fn settle_until_clean(
        env: &mut super::Env,
        owner: &mut super::WorldOwner,
        areas: &[super::super::super::MonitorArea],
        fulls: &[Rect],
        label: &str,
    ) {
        for _ in 0..8 {
            let observed = live_visible_observed();
            let tick = live_tick(
                env,
                owner,
                &observed,
                &[],
                areas,
                fulls,
                moving_prims(),
                0,
                None,
            );
            let clean = tick
                .reply
                .transfers
                .iter()
                .all(|t| !matches!(t.kind, TransferKind::Migration))
                && tick.members.is_empty()
                && tick.views.is_empty();
            if clean {
                return;
            }
        }
        let observed = live_visible_observed();
        let tick = live_tick(
            env,
            owner,
            &observed,
            &[],
            areas,
            fulls,
            moving_prims(),
            0,
            None,
        );
        assert!(
            tick.reply
                .transfers
                .iter()
                .all(|t| !matches!(t.kind, TransferKind::Migration)),
            "{label}: migration flight survives: {:?} members {:?} views {:?}",
            tick.reply.transfers,
            tick.members,
            tick.views
        );
        assert!(
            tick.members.is_empty() && tick.views.is_empty(),
            "{label}: held attempts survive: {:?} {:?}",
            tick.members,
            tick.views
        );
    }
    // Stay-send r1 to trailing: the occupied hidden second workspace on o1.
    let observed = live_visible_observed();
    let stay = live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        r1.hwnd,
        Some(WorldCommand::SendNumbered {
            window: WindowId("w4".to_owned()),
            output: OutputId(o1.clone()),
            index: 0,
            follow: false,
        }),
    );
    let stay_op = window_op(&stay.reply, "w4");
    assert!(
        applied_of_members(&stay.members, &stay.views, &stay_op),
        "stay applies: {:?} {:?}",
        stay.members,
        stay.views
    );
    assert!(
        !S_VISIBLE.with(|m| m.borrow().contains(&r1.hwnd)),
        "r1 hidden by stay"
    );
    let ws_r = env
        .state
        .applied_homing
        .get("w4")
        .expect("stay homing proves")
        .1
        .clone();
    assert_ne!(ws_r, ws_m, "stay leaves the source view");
    let observed = live_visible_observed();
    let settled = live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        0,
        None,
    );
    assert!(
        settled.reply.transfers.is_empty(),
        "stay settles: {:?}",
        settled.reply.transfers
    );
    // A->B with the source-side occupied workspace retained hidden.
    let observed = live_visible_observed();
    live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        0,
        Some(WorldCommand::MigrateDirectional {
            source_output: OutputId(o1.clone()),
            workspace: ws_m.clone(),
            direction: "right".to_owned(),
        }),
    );
    settle_until_clean(&mut env, &mut owner, &areas, &fulls, "A->B");
    for token in ["w1", "w2", "w3"] {
        assert_eq!(
            env.state.applied_homing.get(token),
            Some(&(o2.clone(), ws_m.clone())),
            "{token} arrives on o2"
        );
    }
    assert_eq!(
        owner.engine().logical_view(&o2).as_deref(),
        Some(ws_m.as_str()),
        "o2 shows the migrated unit"
    );
    let refill_a = owner.engine().logical_view(&o1).expect("source refill");
    assert_ne!(refill_a, ws_m, "source refills away from the unit");
    assert_ne!(refill_a, ws_r, "source refill is the fresh empty, not r1's");
    assert!(
        S_VISIBLE.with(|m| m.borrow().contains(&m1.hwnd)),
        "m-unit visible on o2 after A->B"
    );
    assert!(
        !S_VISIBLE.with(|m| m.borrow().contains(&r1.hwnd)),
        "r1 stays hidden through A->B"
    );
    // B->A back onto the occupied source output.
    let observed = live_visible_observed();
    live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        0,
        Some(WorldCommand::MigrateDirectional {
            source_output: OutputId(o2.clone()),
            workspace: ws_m.clone(),
            direction: "left".to_owned(),
        }),
    );
    settle_until_clean(&mut env, &mut owner, &areas, &fulls, "B->A");
    for token in ["w1", "w2", "w3"] {
        assert_eq!(
            env.state.applied_homing.get(token),
            Some(&(o1.clone(), ws_m.clone())),
            "{token} arrives back on o1"
        );
    }
    assert_eq!(
        owner.engine().logical_view(&o1).as_deref(),
        Some(ws_m.as_str()),
        "o1 shows the migrated unit again"
    );
    assert!(
        !S_VISIBLE.with(|m| m.borrow().contains(&r1.hwnd)),
        "r1 still hidden after B->A"
    );
    // Post-roundtrip select into the occupied hidden workspace: the native
    // flip must follow the accepted model advance (finding 1).
    let r_index = index_of(&owner, &o1, &ws_r);
    let observed = live_visible_observed();
    let select = live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        m1.hwnd,
        Some(WorldCommand::SelectNumbered {
            output: Some(OutputId(o1.clone())),
            index: r_index,
        }),
    );
    assert_eq!(
        owner.engine().logical_view(&o1).as_deref(),
        Some(ws_r.as_str()),
        "model advances to the occupied workspace"
    );
    assert!(
        select
            .reply
            .transfers
            .iter()
            .all(|t| !matches!(t.kind, TransferKind::Migration)),
        "select carries no migration flight: {:?}",
        select.reply.transfers
    );
    assert!(
        S_VISIBLE.with(|m| m.borrow().contains(&r1.hwnd)),
        "select reveals r1 natively: members {:?} views {:?}",
        select.members,
        select.views
    );
    for (key, token) in [(&m1, "w1"), (&m2, "w2"), (&m3, "w3")] {
        assert!(
            !S_VISIBLE.with(|m| m.borrow().contains(&key.hwnd)),
            "select hides {token} natively"
        );
    }
    // Select back: the unit returns whole.
    let m_index = index_of(&owner, &o1, &ws_m);
    let observed = live_visible_observed();
    live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        r1.hwnd,
        Some(WorldCommand::SelectNumbered {
            output: Some(OutputId(o1.clone())),
            index: m_index,
        }),
    );
    settle_until_clean(&mut env, &mut owner, &areas, &fulls, "select-back");
    assert_eq!(
        owner.engine().logical_view(&o1).as_deref(),
        Some(ws_m.as_str()),
        "model back on the unit"
    );
    for key in [&m1, &m2, &m3] {
        assert!(
            S_VISIBLE.with(|m| m.borrow().contains(&key.hwnd)),
            "unit visible after select-back"
        );
    }
    assert!(
        !S_VISIBLE.with(|m| m.borrow().contains(&r1.hwnd)),
        "r1 hidden after select-back"
    );
    // Close a twice-migrated member: the survivors must reflow with real
    // geometry writes and every flight must settle (finding 2).
    let pre: std::collections::HashMap<u64, Rect> = live_visible_observed()
        .into_iter()
        .map(|w| (w.hwnd, w.visible))
        .collect();
    let pre_m1 = pre.get(&m1.hwnd).copied().expect("m1 visible pre-close");
    let pre_m2 = pre.get(&m2.hwnd).copied().expect("m2 visible pre-close");
    S_OBSERVE.with(|m| m.borrow_mut().remove(&m3.hwnd));
    S_VISIBLE.with(|m| m.borrow_mut().remove(&m3.hwnd));
    S_TAG.with(|m| m.borrow_mut().remove(&m3.hwnd));
    S_IDENT.with(|m| m.borrow_mut().remove(&m3.hwnd));
    S_PID.with(|m| m.borrow_mut().remove(&m3.hwnd));
    // Verified-retirement mirror (production drops the dead member tables
    // before dispatch; a queued close with live tables never rides): the
    // HWND is gone, so the member rows retire and the close lane carries.
    if let Some(token) = env.state.member_tokens.remove(&m3) {
        env.state.member_rects.remove(&token);
        env.state.float_rects.remove(&token);
    }
    env.state.member_identity.remove(&m3);
    env.state.member_tags.remove(&m3);
    env.state.pending_closed.insert(
        "w3".to_owned(),
        super::super::super::retained::PendingClosed {
            output: DEV1.to_owned(),
            workspace: ws_m.clone(),
        },
    );
    let calls_before = set_calls();
    let observed = live_visible_observed();
    assert_eq!(observed.len(), 2, "only the survivors read visible");
    // The forget plus reflow may pipeline across ticks (close report,
    // then re-tile); the live stall held zero writes across sixteen.
    // Bound the wait tightly so a genuine stall still fails.
    let mut reflowed = false;
    let mut last_members = Vec::new();
    let mut last_views = Vec::new();
    for _ in 0..5 {
        let observed = live_visible_observed();
        let tick = live_tick(
            &mut env,
            &mut owner,
            &observed,
            &[],
            &areas,
            &fulls,
            moving_prims(),
            0,
            None,
        );
        last_members = tick.members.clone();
        last_views = tick.views.clone();
        let forgotten = !env.state.applied_homing.contains_key("w3");
        let post: std::collections::HashMap<u64, Rect> = live_visible_observed()
            .into_iter()
            .map(|w| (w.hwnd, w.visible))
            .collect();
        if forgotten && post.get(&m1.hwnd) != Some(&pre_m1) && post.get(&m2.hwnd) != Some(&pre_m2) {
            reflowed = true;
            break;
        }
    }
    assert!(
        !env.state.applied_homing.contains_key("w3"),
        "closed member forgotten: {:?}",
        env.state.applied_homing.get("w3")
    );
    let post: std::collections::HashMap<u64, Rect> = live_visible_observed()
        .into_iter()
        .map(|w| (w.hwnd, w.visible))
        .collect();
    assert!(
        reflowed,
        "survivors reflow after close (no zero-write stall): pre_m1 {pre_m1:?} post {:?} members {last_members:?} views {last_views:?}",
        post.get(&m1.hwnd)
    );
    assert_ne!(
        post.get(&m1.hwnd),
        Some(&pre_m1),
        "m1 reflows after close (no zero-write stall): {:?} {:?}",
        last_members,
        last_views
    );
    assert_ne!(
        post.get(&m2.hwnd),
        Some(&pre_m2),
        "m2 reflows after close (no zero-write stall): {:?} {:?}",
        last_members,
        last_views
    );
    assert!(
        S_SET_CALLS.with(|m| m
            .borrow()
            .iter()
            .any(|(hwnd, _)| *hwnd == m1.hwnd || *hwnd == m2.hwnd)),
        "geometry writes name the survivors: calls {}",
        set_calls() - calls_before
    );
    settle_until_clean(&mut env, &mut owner, &areas, &fulls, "close");
    assert!(
        !S_VISIBLE.with(|m| m.borrow().contains(&m3.hwnd)),
        "closed member stays gone"
    );
}

/// Sessionless floating-source migration on asymmetric outputs: the
/// non-float member's source rect FULL-delta translates off the small
/// destination, so only the core-owned constrained placement can land and
/// prove arrival. The seam must write the published slot as part of the
/// membership move (never a native origin-delta guess), while the float
/// member preserves its frame with no placement and still reports its
/// membership attempt.
#[test]
fn floating_source_migration_writes_core_slot_and_preserves_float() {
    let mut env = env_with(208_501);
    env.state.ensure_device(DEV2);
    let o1 = env.state.output_token(DEV1);
    let o2 = env.state.output_token(DEV2);
    let (areas, fulls) = asymmetric_migration_areas();
    let a1 = win_key(208_501, 2261, "proc-161");
    let f1 = win_key(208_502, 2262, "proc-162");
    admit_bare(&mut env, &a1, "w1");
    admit_bare(&mut env, &f1, "wf");
    env.state.floated.insert(f1.clone());
    let float_rect = rect(-2000, 100, 300, 200);
    script_live(&a1, "w1", rect(-300, 1000, 400, 300), plain_facts());
    script_live(&f1, "wf", float_rect, plain_facts());
    let mut owner = WorldOwner::bootstrap_ids(ADAPTER, GENERATION).expect("owner");
    let observed = live_visible_observed();
    live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        0,
        None,
    );
    let ws = owner.engine().logical_view(&o1).expect("source view");
    // Toggle the source workspace to floating: the layout session releases,
    // so the later migration carries by sessionless core membership.
    let observed = live_visible_observed();
    live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        0,
        Some(WorldCommand::SetWorkspaceTiling {
            output: OutputId(o1.clone()),
            workspace: ws.clone(),
            tiled: false,
        }),
    );
    // The bootstrap tick tiles w1 into the roomy source slot; reposition
    // it like an external app move so the migrate tick places from a
    // source-edge rect whose blind FULL-delta translation lands off the
    // small destination.
    script_live(&a1, "w1", rect(-300, 1000, 400, 300), plain_facts());
    // Migrate the floating workspace onto the small offset target.
    let observed = live_visible_observed();
    let writes_before = S_SET_CALLS.with(|m| m.borrow().len());
    let moved = live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        0,
        Some(WorldCommand::MigrateWorkspace {
            source_output: OutputId(o1.clone()),
            target_output: OutputId(o2.clone()),
            workspace: ws.clone(),
        }),
    );
    let migration_op = moved
        .reply
        .transfers
        .iter()
        .find(|t| matches!(t.kind, TransferKind::Migration))
        .expect("migration retained")
        .operation_id
        .clone();
    // The sessionless member carries a core-owned placement constrained
    // into the destination work area (the blind FULL-delta translation
    // lands off-screen); the float member carries none.
    let work2 = areas[1].work;
    let row_w1 = moved
        .reply
        .windows
        .iter()
        .find(|w| w.lifetime.0 == "w1")
        .expect("sessionless member row");
    assert_eq!(
        row_w1.membership_authority,
        Auth::Controlled,
        "w1 membership desired"
    );
    assert_eq!(
        row_w1.frame_authority,
        Auth::Untouched,
        "w1 placement rides the membership move, never a sweep write"
    );
    let slot = row_w1
        .effective_frame
        .expect("core publishes the sessionless placement pre-arrival");
    assert_eq!((slot.w, slot.h), (400, 300), "size preserved: {slot:?}");
    assert_eq!(
        slot,
        Rect {
            x: 240,
            y: 120,
            w: 400,
            h: 300,
        },
        "FULL-delta translation constrained into the destination work area"
    );
    assert!(
        slot.x >= work2.x
            && slot.y >= work2.y
            && slot.x + slot.w <= work2.x + work2.w
            && slot.y + slot.h <= work2.y + work2.h,
        "slot {slot:?} constrained into work {work2:?}"
    );
    let row_wf = moved
        .reply
        .windows
        .iter()
        .find(|w| w.lifetime.0 == "wf")
        .expect("float member row");
    assert!(
        row_wf.effective_frame.is_none(),
        "float member carries no placement: {row_wf:?}"
    );
    // The seam writes the published slot for w1 exactly once and never
    // touches the float's frame.
    let calls = S_SET_CALLS.with(|m| m.borrow().clone());
    let fresh: Vec<(u64, Rect)> = calls.into_iter().skip(writes_before).collect();
    let w1_calls: Vec<Rect> = fresh
        .iter()
        .filter(|(hwnd, _)| *hwnd == a1.hwnd)
        .map(|(_, frame)| *frame)
        .collect();
    assert_eq!(
        w1_calls,
        vec![Rect {
            x: slot.x - 8,
            y: slot.y - 8,
            w: slot.w + 16,
            h: slot.h + 16,
        }],
        "one placement write at the published slot, never a remap guess"
    );
    assert!(
        fresh.iter().all(|(hwnd, _)| *hwnd != f1.hwnd),
        "float takes no frame writes: {fresh:?}"
    );
    let live = live_visible_observed();
    assert_eq!(
        live.iter()
            .find(|w| w.token == "w1")
            .expect("mover stays visible")
            .visible,
        slot,
        "w1 physically lands on the published slot"
    );
    assert_eq!(
        live.iter()
            .find(|w| w.token == "wf")
            .expect("float stays visible")
            .visible,
        float_rect,
        "float rectangle preserved across the migration"
    );
    // w1 reports its arrival attempt through the exact slot plus
    // affinity; wf proves by preservation alone (no dispatch, no write).
    // The migration holds only for the deferred source refill.
    let arrivals: Vec<_> = moved
        .members
        .iter()
        .filter(|a| a.operation_id == migration_op)
        .collect();
    assert_eq!(
        arrivals.len(),
        1,
        "only the placed member dispatches: {:?}",
        moved.members
    );
    assert_eq!(
        arrivals[0].post.as_ref(),
        Some(&(OutputId(o2.clone()), ws.clone())),
        "arrival proves: {:?}",
        moved.members
    );
    assert!(
        moved
            .reply
            .transfers
            .iter()
            .any(|t| t.operation_id == migration_op),
        "migration holds only for the refill: {:?}",
        moved.reply.transfers
    );
    assert_eq!(
        env.state.applied_homing.get("w1"),
        Some(&(o2.clone(), ws.clone())),
        "w1 arrival proves on the published slot"
    );
}
/// D1 stages m1 as the full-left column with m2/m3 stacked in the right
/// column, the live helper foregrounds m3, then closes focused m3. The
/// sibling absorbs the right column while the left column keeps its slot,
/// so only m2 grows: the live harness's both-survivors-strictly-greater
/// predicate is unsatisfiable for this (correct) absorption and must never
/// be read as a reflow stall. Survivor focus rides the close tick exactly
/// like the native auto-activate plus helper wake toggles, so the scope
/// stays controlled throughout.
#[test]
fn native_arrangement_close_focused_bottom_right_absorbs_right_column() {
    let mut env = env_with(208_401);
    env.state.ensure_device(DEV2);
    let o1 = env.state.output_token(DEV1);
    let (areas, fulls) = two_output_areas();
    let m1 = win_key(208_401, 2411, "proc-241");
    let m2 = win_key(208_402, 2412, "proc-242");
    let m3 = win_key(208_403, 2413, "proc-243");
    for (key, token) in [(&m1, "w1"), (&m2, "w2"), (&m3, "w3")] {
        admit_bare(&mut env, key, token);
    }
    // Native-shape seed: m1 left column, m2/m3 stacked right column.
    script_live(&m1, "w1", rect(0, 0, 950, 1040), plain_facts());
    script_live(&m2, "w2", rect(960, 0, 960, 510), plain_facts());
    script_live(&m3, "w3", rect(960, 530, 960, 510), plain_facts());
    let mut owner = WorldOwner::bootstrap_ids(ADAPTER, GENERATION).expect("owner");
    for _ in 0..3 {
        let observed = live_visible_observed();
        live_tick(
            &mut env,
            &mut owner,
            &observed,
            &[],
            &areas,
            &fulls,
            moving_prims(),
            m1.hwnd,
            None,
        );
    }
    let ws_m = owner.engine().logical_view(&o1).expect("single workspace");
    // Native topology class (not token labels): one full-height singleton
    // column plus a two-stacked column. Admission transients may rotate
    // which token lands where versus the live staging, so discover roles
    // geometrically: the singleton keeps its slot, the stacked sibling of
    // the focused bottom victim absorbs. (The live leg stages m1 left with
    // m2/m3 right; the seam settles the same shape with rotated labels.)
    let frame_of = |hwnd: u64| {
        live_visible_observed()
            .into_iter()
            .find(|w| w.hwnd == hwnd)
            .expect("member visible")
            .visible
    };
    let f1 = frame_of(m1.hwnd);
    let f2 = frame_of(m2.hwnd);
    let f3 = frame_of(m3.hwnd);
    let by_x: std::collections::BTreeMap<i32, Vec<(u64, Rect)>> = {
        let mut map: std::collections::BTreeMap<i32, Vec<(u64, Rect)>> =
            std::collections::BTreeMap::new();
        for (hwnd, fr) in [(m1.hwnd, f1), (m2.hwnd, f2), (m3.hwnd, f3)] {
            map.entry(fr.x).or_default().push((hwnd, fr));
        }
        map
    };
    assert_eq!(
        by_x.len(),
        2,
        "two columns (singleton plus stacked): {by_x:?}"
    );
    let mut cols: Vec<Vec<(u64, Rect)>> = by_x.into_values().collect();
    for col in &mut cols {
        col.sort_by_key(|(_, fr)| fr.y);
    }
    let (singleton, stacked) = if cols[0].len() == 1 {
        (&cols[0], &cols[1])
    } else {
        (&cols[1], &cols[0])
    };
    assert_eq!(singleton.len(), 1, "one singleton column");
    assert_eq!(stacked.len(), 2, "one two-stacked column");
    let (singleton_hwnd, singleton_pre) = singleton[0];
    let (top_hwnd, _) = stacked[0];
    let (victim_hwnd, victim_pre) = stacked[1];
    let area = |r: Rect| (r.w as i64) * (r.h as i64);
    assert!(
        area(singleton_pre) > area(victim_pre),
        "singleton column larger than each stacked cell"
    );
    // Foreground the bottom stacked victim like the live helper, so the
    // close lands on the focused window; the close ticks below recover
    // focus onto the stacked survivor like the native auto-activate.
    let observed = live_visible_observed();
    live_tick(
        &mut env,
        &mut owner,
        &observed,
        &[],
        &areas,
        &fulls,
        moving_prims(),
        victim_hwnd,
        None,
    );
    // Verified-retirement mirror (production drops the dead member tables
    // before dispatch; a queued close with live tables never rides).
    let victim_key = env
        .state
        .member_tokens
        .keys()
        .find(|k| k.hwnd == victim_hwnd)
        .cloned()
        .expect("victim member key");
    let victim_token = env
        .state
        .member_tokens
        .get(&victim_key)
        .cloned()
        .expect("victim token");
    S_OBSERVE.with(|m| m.borrow_mut().remove(&victim_hwnd));
    S_VISIBLE.with(|m| m.borrow_mut().remove(&victim_hwnd));
    S_TAG.with(|m| m.borrow_mut().remove(&victim_hwnd));
    S_IDENT.with(|m| m.borrow_mut().remove(&victim_hwnd));
    S_PID.with(|m| m.borrow_mut().remove(&victim_hwnd));
    if let Some(token) = env.state.member_tokens.remove(&victim_key) {
        env.state.member_rects.remove(&token);
        env.state.float_rects.remove(&token);
    }
    env.state.member_identity.remove(&victim_key);
    env.state.member_tags.remove(&victim_key);
    env.state.pending_closed.insert(
        victim_token.clone(),
        super::super::super::retained::PendingClosed {
            output: DEV1.to_owned(),
            workspace: ws_m.clone(),
        },
    );
    let observed = live_visible_observed();
    assert_eq!(observed.len(), 2, "only the survivors read visible");
    let sibling_pre = frame_of(top_hwnd);
    let _ = victim_pre;
    let mut reflowed = false;
    for _ in 0..5 {
        let observed = live_visible_observed();
        let tick = live_tick(
            &mut env,
            &mut owner,
            &observed,
            &[],
            &areas,
            &fulls,
            moving_prims(),
            top_hwnd,
            None,
        );
        assert!(
            tick.reply
                .transfers
                .iter()
                .all(|t| !matches!(t.kind, TransferKind::Migration)),
            "close carries no migration flight: {:?}",
            tick.reply.transfers
        );
        let forgotten = !env.state.applied_homing.contains_key(&victim_token);
        let post_singleton = frame_of(singleton_hwnd);
        let post_sibling = frame_of(top_hwnd);
        if forgotten && post_singleton == singleton_pre && area(post_sibling) > area(sibling_pre) {
            reflowed = true;
            break;
        }
    }
    assert!(
        !env.state.applied_homing.contains_key(&victim_token),
        "closed member forgotten"
    );
    let post_singleton = frame_of(singleton_hwnd);
    let post_sibling = frame_of(top_hwnd);
    assert!(
        reflowed,
        "sibling absorbs the stacked column while the singleton column keeps its slot (no both-grow stall): singleton {singleton_pre:?} -> {post_singleton:?} sibling {sibling_pre:?} -> {post_sibling:?}"
    );
    assert_eq!(
        post_singleton, singleton_pre,
        "singleton column keeps its slot after the stacked-leaf close"
    );
    assert!(
        area(post_sibling) > area(sibling_pre),
        "stacked sibling grows after the stacked-leaf close"
    );
    assert!(
        !S_VISIBLE.with(|m| m.borrow().contains(&victim_hwnd)),
        "closed member stays gone"
    );
}

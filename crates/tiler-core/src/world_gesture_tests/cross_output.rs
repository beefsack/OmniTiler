//! Cross-output gesture release: pair-targeted slot agreement, retained
//! transfer fences, and gap bookkeeping.
//!
//! Shared rig helpers live in the parent (`world_gesture_tests`); this
//! module holds the cross-output suite and uses them via `super::*`.

use super::*;

/// Mixed-output rig mirroring the physical case: left output 2048x1152
/// at 125%, right output 1920x1080 at 100% offset x=2048,y=72. Work
/// areas are distinctly non-full per output (top/bottom panels on
/// out-a, left/bottom struts on out-b), so absolute slot geometry
/// discriminates panel handling rather than echoing full rects.
fn mixed_screens() -> Vec<ScreenReport> {
    vec![
        ScreenReport {
            output: OutputId("out-a".to_owned()),
            full_rect: Some(rect(0, 0, 2048, 1152)),
            work_area: Some(rect(0, 40, 2048, 1072)),
            complete: true,
            scale: 1.0,
            identity: crate::world_report::OutputIdentity::unknown(),
        },
        ScreenReport {
            output: OutputId("out-b".to_owned()),
            full_rect: Some(rect(2048, 72, 1920, 1080)),
            work_area: Some(rect(2072, 72, 1896, 1016)),
            complete: true,
            scale: 1.0,
            identity: crate::world_report::OutputIdentity::unknown(),
        },
    ]
}

#[allow(clippy::too_many_arguments)]
fn mixed_report(
    request_seq: u64,
    observation_seq: u64,
    windows: Vec<WindowFact>,
    focused: &str,
    gesture: Option<GestureIngress>,
    closed: Vec<WindowId>,
    inventory_complete: bool,
) -> WorldReportV4 {
    report_on(
        mixed_screens(),
        request_seq,
        observation_seq,
        windows,
        focused,
        gesture,
        closed,
        inventory_complete,
    )
}

/// Work area for one mixed output, mirroring `mixed_screens`.
fn mixed_work(output: &str) -> Rect {
    match output {
        "out-a" => rect(0, 40, 2048, 1072),
        _ => rect(2072, 72, 1896, 1016),
    }
}

fn mixed_fact_rows(seed: Vec<(&str, &str, Rect)>) -> Vec<WindowFact> {
    seed.iter()
        .map(|(window, output, frame)| fact(window, output, *frame))
        .collect()
}

fn engine_without_gaps() -> WorldEngine {
    WorldEngine::new_test_bound_with_initial_settings(
        "gen-1",
        "adapter-1",
        crate::connection::HostCapabilities {
            // Fixtures carry native sticky and maximize facts; the gesture
            // lane under test is the native route.
            native_sticky: true,
            maximize: true,
            ..Default::default()
        },
        vec![
            (
                "gap".to_owned(),
                crate::connection::RawSettingValue::Number(0.0),
            ),
            (
                "outer".to_owned(),
                crate::connection::RawSettingValue::Number(0.0),
            ),
        ],
    )
    .expect("valid generation")
}

fn seed_mixed(seed_facts: Vec<WindowFact>) -> WorldEngine {
    let mut engine = engine_without_gaps();
    accepted(
        &mut engine,
        &mixed_report(1, 1, seed_facts, "", None, vec![], true),
    );
    engine
}

/// Preview rect plus the resolving output name, for cross-output slot
/// agreement checks.
fn preview_slot(reply: &crate::world_report::WorldReplyV4) -> (Rect, String) {
    let Some(DragPreviewV4 { rect, domain, .. }) = reply.drag_preview.clone() else {
        panic!("expected drag-preview, got {:?}", reply.drag_preview);
    };
    (rect, domain.output.0.clone())
}

#[derive(Clone, Copy, Debug)]
enum Half {
    Left,
    Right,
}

/// One parameterized cross-output slot case: the preview must name the
/// absolute expected half-slot on the destination output, the release
/// must commit exactly that slot with follow settlement, and the
/// arrival report must retire the transfer with the mover geometry
/// pinned at the slot. `seed` names (window, output, frame) rows; the
/// destination window must be its output's sole window so the split
/// halves are exact.
#[allow(clippy::too_many_arguments)]
fn cross_slot(
    seed: Vec<(&str, &str, Rect)>,
    mover: &str,
    dest: &str,
    dest_output: &str,
    side: Half,
    slot: Rect,
) {
    let seed_facts = mixed_fact_rows(seed);
    let start = seed_facts
        .iter()
        .find(|row| row.window.0 == mover)
        .and_then(|row| row.rect)
        .expect("seed frames the mover");
    let mut engine = seed_mixed(seed_facts.clone());
    let synced = accepted(
        &mut engine,
        &mixed_report(2, 2, seed_facts.clone(), mover, None, vec![], true),
    );
    // Sole-window destination fills its asymmetric work area exactly.
    let home = row_of(&synced, dest).allocation.expect("seed places dest");
    assert_eq!(home, mixed_work(dest_output), "dest fills its work area");
    let cursor = match side {
        Half::Left => (home.x + 2, home.y + home.h / 2),
        Half::Right => (home.x + home.w - 2, home.y + home.h / 2),
    };
    let previewed = accepted(
        &mut engine,
        &mixed_report(
            3,
            3,
            seed_facts.clone(),
            mover,
            Some(ingress(
                mover,
                GestureKind::Move,
                Some(cursor),
                GesturePhase::Updated,
                start,
            )),
            vec![],
            true,
        ),
    );
    let (shown, output) = preview_slot(&previewed);
    assert_eq!(output, dest_output, "preview resolves the pointer output");
    assert_eq!(shown, slot, "preview names the absolute {side:?} slot");
    // Release at the same live coordinates with the natively moved frame
    // (same size, new origin: a true move).
    let moved = rect(slot.x + 60, slot.y + 60, start.w, start.h);
    let release_facts: Vec<WindowFact> = seed_facts
        .iter()
        .map(|row| {
            if row.window.0 == mover {
                fact(mover, dest_output, moved)
            } else {
                row.clone()
            }
        })
        .collect();
    let released = accepted(
        &mut engine,
        &mixed_report(
            4,
            4,
            release_facts,
            mover,
            Some(ingress(
                mover,
                GestureKind::Move,
                Some(cursor),
                GesturePhase::Released,
                start,
            )),
            vec![],
            true,
        ),
    );
    assert_eq!(
        released.gesture_settled.map(|t| (t.id, t.committed)),
        Some((1, true)),
        "move release records a committed terminal"
    );
    assert!(
        released.drag_preview.is_none(),
        "release rides the snapshot"
    );
    let transfer = released
        .transfers
        .iter()
        .find(|t| matches!(&t.kind, TransferKind::Window(w) if w.0 == mover))
        .expect("cross release retains a window transfer");
    assert_eq!(transfer.target_output.0, dest_output);
    assert!(transfer.follow, "cross drop keeps follow settlement");
    assert_eq!(row_of(&released, mover).output.0, dest_output);
    assert_eq!(
        row_of(&released, mover).allocation,
        Some(slot),
        "release must land on the previewed slot, got {:?}",
        row_of(&released, mover).allocation,
    );
    // Arrival receipt retires the transfer with the mover geometry
    // pinned at the slot: settlement plus observed geometry. The
    // legacy setup also sent the target-view echo, but echoes are
    // inert on native-free engines (and have no v4 slot), so the v4
    // port observes the same arrival with an empty lane.
    let arrival_facts: Vec<WindowFact> = seed_facts
        .iter()
        .map(|row| {
            if row.window.0 == mover {
                fact(mover, dest_output, slot)
            } else {
                row.clone()
            }
        })
        .collect();
    let settle = mixed_report(5, 5, arrival_facts, mover, None, vec![], true);
    let settled = accepted(&mut engine, &settle);
    assert!(
        settled
            .transfers
            .iter()
            .all(|t| !matches!(&t.kind, TransferKind::Window(w) if w.0 == mover)),
        "arrival receipt retires the gesture transfer"
    );
    assert_eq!(
        row_of(&settled, mover).allocation,
        Some(slot),
        "arrival keeps the mover on the slot, got {:?}",
        row_of(&settled, mover).allocation,
    );
}

#[test]
fn cross_slot_left_into_primary() {
    // Drag win-3 onto the LEFT half of win-1: absolute slot plus
    // preview agreement plus arrival geometry.
    cross_slot(
        vec![
            ("win-1", "out-a", rect(10, 50, 40, 90)),
            ("win-2", "out-b", rect(2090, 90, 40, 90)),
            ("win-3", "out-b", rect(2230, 90, 40, 90)),
        ],
        "win-3",
        "win-1",
        "out-a",
        Half::Left,
        rect(0, 40, 1024, 1072),
    );
}

#[test]
fn cross_slot_right_into_primary() {
    cross_slot(
        vec![
            ("win-1", "out-a", rect(10, 50, 40, 90)),
            ("win-2", "out-b", rect(2090, 90, 40, 90)),
            ("win-3", "out-b", rect(2230, 90, 40, 90)),
        ],
        "win-3",
        "win-1",
        "out-a",
        Half::Right,
        rect(1024, 40, 1024, 1072),
    );
}

#[test]
fn cross_slot_left_into_secondary() {
    // Reverse direction: drag win-1 onto the LEFT half of win-2.
    cross_slot(
        vec![
            ("win-1", "out-a", rect(10, 50, 40, 90)),
            ("win-2", "out-b", rect(2090, 90, 40, 90)),
            ("win-3", "out-a", rect(200, 50, 40, 90)),
        ],
        "win-1",
        "win-2",
        "out-b",
        Half::Left,
        rect(2072, 72, 948, 1016),
    );
}

#[test]
fn cross_slot_right_into_secondary() {
    cross_slot(
        vec![
            ("win-1", "out-a", rect(10, 50, 40, 90)),
            ("win-2", "out-b", rect(2090, 90, 40, 90)),
            ("win-3", "out-a", rect(200, 50, 40, 90)),
        ],
        "win-1",
        "win-2",
        "out-b",
        Half::Right,
        rect(3020, 72, 948, 1016),
    );
}

#[test]
fn cross_release_with_mover_in_flight_refuses_without_duplicate() {
    // A first cross release retains the window flight (no arrival
    // yet). A second gesture releasing cross-output must not
    // pair-commit a duplicate transfer: the send fallback refuses as
    // in-flight, with no Engine mutation.
    let seed_facts = mixed_fact_rows(vec![
        ("win-1", "out-a", rect(10, 50, 40, 90)),
        ("win-2", "out-b", rect(2090, 90, 40, 90)),
        ("win-3", "out-b", rect(2230, 90, 40, 90)),
    ]);
    let start = seed_facts
        .iter()
        .find(|row| row.window.0 == "win-3")
        .and_then(|row| row.rect)
        .expect("seed frames the mover");
    let mut engine = seed_mixed(seed_facts.clone());
    let synced = accepted(
        &mut engine,
        &mixed_report(2, 2, seed_facts.clone(), "win-3", None, vec![], true),
    );
    let home = row_of(&synced, "win-1")
        .allocation
        .expect("seed places dest");
    let cursor = (home.x + 2, home.y + home.h / 2);
    accepted(
        &mut engine,
        &mixed_report(
            3,
            3,
            seed_facts.clone(),
            "win-3",
            Some(ingress(
                "win-3",
                GestureKind::Move,
                Some(cursor),
                GesturePhase::Updated,
                start,
            )),
            vec![],
            true,
        ),
    );
    let moved1 = rect(home.x + 200, home.y + 100, start.w, start.h);
    let release_facts: Vec<WindowFact> = seed_facts
        .iter()
        .map(|row| {
            if row.window.0 == "win-3" {
                fact("win-3", "out-a", moved1)
            } else {
                row.clone()
            }
        })
        .collect();
    let released1 = accepted(
        &mut engine,
        &mixed_report(
            4,
            4,
            release_facts,
            "win-3",
            Some(ingress(
                "win-3",
                GestureKind::Move,
                Some(cursor),
                GesturePhase::Released,
                start,
            )),
            vec![],
            true,
        ),
    );
    assert_eq!(
        released1.gesture_settled.map(|t| (t.id, t.committed)),
        Some((1, true)),
        "first release commits the cross drop"
    );
    let op1 = released1
        .transfers
        .iter()
        .find(|t| matches!(&t.kind, TransferKind::Window(w) if w.0 == "win-3"))
        .expect("first release retains the flight")
        .operation_id
        .clone();
    let first_alloc = row_of(&released1, "win-3").allocation;
    // Fresh gesture while the flight is outstanding: release back onto
    // out-b at live coordinates. The adapter still observes the mover at
    // the source with its readable seed frame (delayed capture of the
    // first drop never arrived): the fresh gesture binds against the
    // retained target home, yet staged homing must not self-confirm
    // from that shielded state. The first Window flight holds through
    // settlement instead of retiring on homing, and the mover keeps
    // its target redirect row.
    let back = row_of(&released1, "win-2")
        .allocation
        .expect("first release keeps win-2");
    let cursor2 = (back.x + 2, back.y + back.h / 2);
    // The mover never arrived: the source observation stays readable
    // at out-b (delayed capture), so settlement must hold the flight
    // on the source key rather than retire it.
    let updated_facts: Vec<WindowFact> = seed_facts.clone();
    let updated5 = accepted(
        &mut engine,
        &mixed_report(
            5,
            5,
            updated_facts,
            "win-3",
            Some(ingress_id(
                "win-3",
                GestureKind::Move,
                Some(cursor2),
                GesturePhase::Updated,
                start,
                2,
            )),
            vec![],
            true,
        ),
    );
    assert!(
        updated5.transfers.iter().any(|t| t.operation_id == op1
            && matches!(&t.kind, TransferKind::Window(w) if w.0 == "win-3")),
        "first Window flight still present before the second release, got {:?}",
        updated5.transfers
    );
    let moved2 = rect(2100, 200, start.w, start.h);
    // Second release still observes the mover at the source with a
    // readable frame: no arrival, so the fresh release must refuse.
    let release_facts2: Vec<WindowFact> = seed_facts
        .iter()
        .map(|row| {
            if row.window.0 == "win-3" {
                fact("win-3", "out-b", moved2)
            } else {
                row.clone()
            }
        })
        .collect();
    let released2 = accepted(
        &mut engine,
        &mixed_report(
            6,
            6,
            release_facts2,
            "win-3",
            Some(ingress_id(
                "win-3",
                GestureKind::Move,
                Some(cursor2),
                GesturePhase::Released,
                start,
                2,
            )),
            vec![],
            true,
        ),
    );
    assert_eq!(
        released2.gesture_settled.map(|t| (t.id, t.committed)),
        Some((2, false)),
        "second release under the flight must not commit"
    );
    let flights: Vec<_> = released2
        .transfers
        .iter()
        .filter(|t| matches!(&t.kind, TransferKind::Window(w) if w.0 == "win-3"))
        .collect();
    assert_eq!(
        flights.len(),
        1,
        "no duplicate window transfer, got {:?}",
        released2.transfers
    );
    assert_eq!(
        flights[0].operation_id, op1,
        "the original flight is untouched"
    );
    assert_eq!(
        row_of(&released2, "win-3").allocation,
        first_alloc,
        "refused release mutates no Engine geometry"
    );
}

/// Third-output rig for the migration-occupancy fence: the gesture
/// mover homes out-c while a whole-workspace migration owns its
/// destination cell on out-b.
fn migration_screens() -> Vec<ScreenReport> {
    let mut screens = mixed_screens();
    screens.push(ScreenReport {
        output: OutputId("out-c".to_owned()),
        full_rect: Some(rect(3968, 0, 1280, 800)),
        work_area: Some(rect(3968, 0, 1280, 800)),
        complete: true,
        scale: 1.0,
        identity: crate::world_report::OutputIdentity::unknown(),
    });
    screens
}

#[allow(clippy::too_many_arguments)]
fn migration_report(
    request_seq: u64,
    observation_seq: u64,
    windows: Vec<WindowFact>,
    focused: &str,
    gesture: Option<GestureIngress>,
) -> WorldReportV4 {
    report_on(
        migration_screens(),
        request_seq,
        observation_seq,
        windows,
        focused,
        gesture,
        vec![],
        true,
    )
}

#[test]
fn cross_release_into_migration_cell_refuses_without_double_book() {
    // A retained whole-workspace migration owns its destination cell.
    // A gesture releasing into that cell must not pair-commit a
    // double-book: the send fallback refuses as send-target, with no
    // Engine mutation and the migration untouched.
    use crate::world_report::WorldCommand;
    let r1 = rect(10, 50, 40, 90);
    let r2 = rect(200, 50, 40, 90);
    let r3 = rect(3980, 20, 40, 90);
    let seed_facts = vec![
        fact("win-1", "out-a", r1),
        fact("win-2", "out-a", r2),
        fact("win-3", "out-c", r3),
    ];
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
    accepted(
        &mut engine,
        &migration_report(1, 1, seed_facts.clone(), "", None),
    );
    let synced = accepted(
        &mut engine,
        &migration_report(2, 2, seed_facts.clone(), "", None),
    );
    let before = row_of(&synced, "win-3").allocation;
    let view_a = synced
        .outputs
        .iter()
        .find(|row| row.output.0 == "out-a")
        .and_then(|row| row.logical_view.clone())
        .expect("out-a names its view");
    let mut migrate = migration_report(3, 3, seed_facts.clone(), "", None);
    migrate.command_seq = Some(11);
    migrate.command = Some(WorldCommand::MigrateWorkspace {
        source_output: OutputId("out-a".to_owned()),
        target_output: OutputId("out-b".to_owned()),
        workspace: crate::directional::WorkspaceId(view_a.clone()),
    });
    let migrated = accepted(&mut engine, &migrate);
    let cell = migrated
        .transfers
        .iter()
        .find(|t| t.operation_id == "op-11" && matches!(t.kind, TransferKind::Migration))
        .expect("migration retains its cell");
    assert_eq!(cell.target_output.0, "out-b");
    let cell_ws = cell.target_workspace.clone();
    let target_view = migrated
        .outputs
        .iter()
        .find(|row| row.output.0 == "out-b")
        .and_then(|row| row.logical_view.clone())
        .expect("out-b names its view");
    assert_eq!(
        target_view, cell_ws,
        "gesture destination routes into the migration cell"
    );
    // Fresh gesture: win-3 (homed out-c) releases onto out-b at live
    // coordinates inside the occupied cell's work area.
    let cursor = (2500, 300);
    accepted(
        &mut engine,
        &migration_report(
            4,
            4,
            seed_facts.clone(),
            "win-3",
            Some(ingress(
                "win-3",
                GestureKind::Move,
                Some(cursor),
                GesturePhase::Updated,
                r3,
            )),
        ),
    );
    let moved = rect(2500, 300, r3.w, r3.h);
    let release_facts: Vec<WindowFact> = seed_facts
        .iter()
        .map(|row| {
            if row.window.0 == "win-3" {
                fact("win-3", "out-b", moved)
            } else {
                row.clone()
            }
        })
        .collect();
    let released = accepted(
        &mut engine,
        &migration_report(
            5,
            5,
            release_facts,
            "win-3",
            Some(ingress(
                "win-3",
                GestureKind::Move,
                Some(cursor),
                GesturePhase::Released,
                r3,
            )),
        ),
    );
    assert_eq!(
        released.gesture_settled.map(|t| (t.id, t.committed)),
        Some((1, false)),
        "release into the migration cell must not commit"
    );
    assert!(
        released
            .transfers
            .iter()
            .all(|t| !matches!(&t.kind, TransferKind::Window(w) if w.0 == "win-3")),
        "no window transfer double-books the cell, got {:?}",
        released.transfers
    );
    assert!(
        released
            .transfers
            .iter()
            .any(|t| t.operation_id == "op-11" && matches!(t.kind, TransferKind::Migration)),
        "the migration flight is untouched"
    );
    assert_eq!(
        row_of(&released, "win-3").allocation,
        before,
        "refused release mutates no Engine geometry"
    );
}

/// Gapped variant of the mixed rig: out-b carries a nonzero
/// outer_gap, so the pair commit's gap bookkeeping is exercised.
fn gapped_screens() -> Vec<ScreenReport> {
    let mut screens = mixed_screens();
    screens[1].work_area = Some(rect(2084, 84, 1872, 992));
    screens
}

fn gapped_report(
    request_seq: u64,
    observation_seq: u64,
    windows: Vec<WindowFact>,
    focused: &str,
    gesture: Option<GestureIngress>,
) -> WorldReportV4 {
    report_on(
        gapped_screens(),
        request_seq,
        observation_seq,
        windows,
        focused,
        gesture,
        vec![],
        true,
    )
}

#[test]
fn cross_pair_release_with_outer_gap_keeps_local_route() {
    // Out-b carries outer_gap 12. The pair release must still agree
    // with its preview, and a follow-up same-output gesture on the
    // destination must still resolve through the local route (the
    // pair commit's stored gaps must not refuse it).
    let seed_facts = vec![
        fact("win-1", "out-a", rect(10, 50, 40, 90)),
        fact("win-2", "out-b", rect(2090, 90, 40, 90)),
        fact("win-3", "out-a", rect(200, 50, 40, 90)),
    ];
    let start = rect(10, 50, 40, 90);
    let mut engine = engine_without_gaps();
    accepted(
        &mut engine,
        &gapped_report(1, 1, seed_facts.clone(), "", None),
    );
    let synced = accepted(
        &mut engine,
        &gapped_report(2, 2, seed_facts.clone(), "win-1", None),
    );
    let dest = row_of(&synced, "win-2")
        .allocation
        .expect("seed places win-2");
    // The destination session retains the gap-inset work area.
    let inset = rect(2084, 84, 1872, 992);
    assert_eq!(dest, inset, "seed places win-2 on the inset area");
    // Inside the 12px inset and the 32px edge depth: a left-edge split
    // of the inset area.
    let cursor = (dest.x + 14, dest.y + dest.h / 2);
    let previewed = accepted(
        &mut engine,
        &gapped_report(
            3,
            3,
            seed_facts.clone(),
            "win-1",
            Some(ingress(
                "win-1",
                GestureKind::Move,
                Some(cursor),
                GesturePhase::Updated,
                start,
            )),
        ),
    );
    let (slot, output) = preview_slot(&previewed);
    assert_eq!(output, "out-b", "preview resolves the pointer output");
    assert_eq!(
        slot,
        rect(2084, 84, 936, 992),
        "preview names the absolute inset LEFT slot"
    );
    let moved = rect(dest.x + 100, dest.y + 50, start.w, start.h);
    let release_facts: Vec<WindowFact> = seed_facts
        .iter()
        .map(|row| {
            if row.window.0 == "win-1" {
                fact("win-1", "out-b", moved)
            } else {
                row.clone()
            }
        })
        .collect();
    let released = accepted(
        &mut engine,
        &gapped_report(
            4,
            4,
            release_facts.clone(),
            "win-1",
            Some(ingress(
                "win-1",
                GestureKind::Move,
                Some(cursor),
                GesturePhase::Released,
                start,
            )),
        ),
    );
    assert_eq!(
        released.gesture_settled.map(|t| (t.id, t.committed)),
        Some((1, true)),
        "gapped pair release commits"
    );
    assert_eq!(
        row_of(&released, "win-1").allocation,
        Some(slot),
        "gapped release agrees with its preview, got {:?}",
        row_of(&released, "win-1").allocation,
    );
    // Follow-up same-output gesture on the destination output while
    // the flight is still retained: the local route must resolve.
    let next = row_of(&released, "win-2")
        .allocation
        .expect("release keeps win-2");
    let cursor2 = (next.x + 2, next.y + next.h / 2);
    let mut follow_facts = release_facts;
    follow_facts[0] = fact("win-1", "out-b", moved);
    let previewed2 = accepted(
        &mut engine,
        &gapped_report(
            5,
            5,
            follow_facts.clone(),
            "win-1",
            Some(ingress_id(
                "win-1",
                GestureKind::Move,
                Some(cursor2),
                GesturePhase::Updated,
                moved,
                2,
            )),
        ),
    );
    preview_slot(&previewed2);
    // The native gesture moves the frame before release: same size at
    // a new origin, otherwise the release is a no-change snap-back.
    let moved2 = rect(cursor2.0 - 20, cursor2.1 - 40, start.w, start.h);
    let release_facts2: Vec<WindowFact> = follow_facts
        .iter()
        .map(|row| {
            if row.window.0 == "win-1" {
                fact("win-1", "out-b", moved2)
            } else {
                row.clone()
            }
        })
        .collect();
    let released2 = accepted(
        &mut engine,
        &gapped_report(
            6,
            6,
            release_facts2,
            "win-1",
            Some(ingress_id(
                "win-1",
                GestureKind::Move,
                Some(cursor2),
                GesturePhase::Released,
                moved,
                2,
            )),
        ),
    );
    assert_eq!(
        released2.gesture_settled.map(|t| (t.id, t.committed)),
        Some((2, true)),
        "follow-up local release commits through stored gaps"
    );
}

#[test]
fn cross_release_to_empty_destination_keeps_send_admission() {
    // out-b stages no windows: the pair route is inapplicable, so the
    // release keeps the legacy send admission (membership plus follow)
    // and its settlement.
    let mut engine = WorldEngine::new_test_bound("gen-1", "adapter-1").expect("valid generation");
    let r1 = rect(10, 10, 40, 90);
    let r2 = rect(100, 10, 40, 90);
    accepted(
        &mut engine,
        &mixed_report(
            1,
            1,
            vec![fact("win-1", "out-a", r1), fact("win-2", "out-a", r2)],
            "",
            None,
            vec![],
            true,
        ),
    );
    let cursor = (3000, 500);
    let moved = rect(2900, 400, r2.w, r2.h);
    let released = accepted(
        &mut engine,
        &mixed_report(
            2,
            2,
            vec![fact("win-1", "out-a", r1), fact("win-2", "out-b", moved)],
            "win-2",
            Some(ingress(
                "win-2",
                GestureKind::Move,
                Some(cursor),
                GesturePhase::Released,
                r2,
            )),
            vec![],
            true,
        ),
    );
    assert_eq!(
        released.gesture_settled.map(|t| (t.id, t.committed)),
        Some((1, true)),
        "empty-destination release records a committed terminal"
    );
    let transfer = released
        .transfers
        .iter()
        .find(|t| matches!(&t.kind, TransferKind::Window(w) if w.0 == "win-2"))
        .expect("empty-destination release retains a window transfer");
    assert_eq!(transfer.target_output.0, "out-b");
    assert!(transfer.follow, "empty-destination drop follows");
    assert_eq!(row_of(&released, "win-2").output.0, "out-b");
    let arrived = fact("win-2", "out-b", moved);
    let settle = mixed_report(
        3,
        3,
        vec![fact("win-1", "out-a", r1), arrived],
        "win-2",
        None,
        vec![],
        true,
    );
    let settled = accepted(&mut engine, &settle);
    assert!(
        settled
            .transfers
            .iter()
            .all(|t| !matches!(&t.kind, TransferKind::Window(w) if w.0 == "win-2")),
        "arrival receipt retires the empty-destination transfer"
    );
}

#[test]
fn cross_output_sticky_scope_survives_same_dest_and_clears_on_dest_change() {
    let mut engine = WorldEngine::new_test_bound_with_initial_settings(
        "gen-1",
        "adapter-1",
        crate::connection::HostCapabilities {
            native_sticky: true,
            maximize: true,
            ..Default::default()
        },
        vec![
            (
                "gap".to_owned(),
                crate::connection::RawSettingValue::Number(8.0),
            ),
            (
                "outer".to_owned(),
                crate::connection::RawSettingValue::Number(0.0),
            ),
        ],
    )
    .expect("valid generation");
    let screens = vec![
        ScreenReport {
            output: OutputId("out-a".to_owned()),
            full_rect: Some(rect(0, 0, 800, 600)),
            work_area: Some(rect(0, 0, 800, 600)),
            complete: true,
            scale: 1.0,
            identity: crate::world_report::OutputIdentity::unknown(),
        },
        ScreenReport {
            output: OutputId("out-b".to_owned()),
            full_rect: Some(rect(800, 0, 800, 600)),
            work_area: Some(rect(800, 0, 800, 600)),
            complete: true,
            scale: 1.0,
            identity: crate::world_report::OutputIdentity::unknown(),
        },
        ScreenReport {
            output: OutputId("out-c".to_owned()),
            full_rect: Some(rect(1600, 0, 800, 600)),
            work_area: Some(rect(1600, 0, 800, 600)),
            complete: true,
            scale: 1.0,
            identity: crate::world_report::OutputIdentity::unknown(),
        },
    ];
    // out-a: H[win-1, win-2]; out-b: H[win-3, win-4] first, then admit win-5 under tall win-4 -> H[win-3, V[win-4, win-5]]; out-c: win-6.
    let initial_facts = vec![
        fact("win-1", "out-a", rect(0, 0, 400, 600)),
        fact("win-2", "out-a", rect(400, 0, 400, 600)),
        fact("win-3", "out-b", rect(800, 0, 400, 600)),
        fact("win-4", "out-b", rect(1200, 0, 400, 600)),
        fact("win-6", "out-c", rect(1600, 0, 800, 600)),
    ];
    accepted(
        &mut engine,
        &report_on(
            screens.clone(),
            1,
            1,
            initial_facts.clone(),
            "win-4",
            None,
            vec![],
            true,
        ),
    );
    let seed_facts = vec![
        fact("win-1", "out-a", rect(0, 0, 400, 600)),
        fact("win-2", "out-a", rect(400, 0, 400, 600)),
        fact("win-3", "out-b", rect(800, 0, 400, 600)),
        fact("win-4", "out-b", rect(1200, 0, 400, 600)),
        fact("win-5", "out-b", rect(1200, 0, 200, 600)),
        fact("win-6", "out-c", rect(1600, 0, 800, 600)),
    ];
    accepted(
        &mut engine,
        &report_on(
            screens.clone(),
            2,
            2,
            seed_facts.clone(),
            "win-4",
            None,
            vec![],
            true,
        ),
    );
    let seeded = accepted(
        &mut engine,
        &report_on(
            screens.clone(),
            3,
            3,
            seed_facts.clone(),
            "win-2",
            None,
            vec![],
            true,
        ),
    );

    let r3 = row_of(&seeded, "win-3").allocation.expect("win-3 alloc");
    let r4 = row_of(&seeded, "win-4").allocation.expect("win-4 alloc");
    let r5 = row_of(&seeded, "win-5").allocation.expect("win-5 alloc");
    let inner_x = r4.x.min(r5.x);
    let inner_y = r4.y.min(r5.y);
    let inner_b = (r4.y + r4.h).max(r5.y + r5.h);
    let inner_cx = inner_x + r4.w.max(r5.w) / 2;
    let contains =
        |r: &Rect, x: i32, y: i32| x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h;
    let gy = (inner_y..inner_b)
        .find(|&y| {
            !contains(&r3, inner_cx, y)
                && !contains(&r4, inner_cx, y)
                && !contains(&r5, inner_cx, y)
                && y - inner_y >= 32
                && inner_b - y > 32
        })
        .expect("inner V gap row on out-b");
    let gx = inner_x + 5;
    let sticky_x = inner_x + 50;
    assert!(gx >= 800, "gx={gx} gy={gy} r3={r3:?} r4={r4:?} r5={r5:?}");
    let r2 = row_of(&seeded, "win-2").allocation.expect("win-2 alloc");

    // 1) Hover on out-b inner V left strip arms sticky Left prior.
    let edge = accepted(
        &mut engine,
        &report_on(
            screens.clone(),
            4,
            4,
            seed_facts.clone(),
            "win-2",
            Some(ingress(
                "win-2",
                GestureKind::Move,
                Some((gx, gy)),
                GesturePhase::Updated,
                r2,
            )),
            vec![],
            true,
        ),
    );
    let edge_preview = edge.drag_preview.expect("edge preview on out-b");
    assert_eq!(edge_preview.domain.output.0, "out-b");
    let edge_prior = edge_preview
        .hover
        .as_ref()
        .and_then(|h| h.prior.clone())
        .expect("arms sticky group edge prior");
    assert_eq!(edge_prior.edge, crate::contract::DragSide::Left);

    // 2) Probe 50px inside out-b inner V group: same destination keeps sticky Left.
    let sticky = accepted(
        &mut engine,
        &report_on(
            screens.clone(),
            5,
            5,
            seed_facts.clone(),
            "win-2",
            Some(ingress(
                "win-2",
                GestureKind::Move,
                Some((sticky_x, gy)),
                GesturePhase::Updated,
                r2,
            )),
            vec![],
            true,
        ),
    );
    let sticky_preview = sticky.drag_preview.expect("sticky preview on out-b");
    assert_eq!(sticky_preview.domain.output.0, "out-b");
    assert_eq!(sticky_preview.rect, edge_preview.rect);
    assert_eq!(
        sticky_preview.hover.as_ref().and_then(|h| h.prior.clone()),
        Some(edge_prior)
    );

    // 3) Moving to out-c resolves out-c cleanly without out-b's prior steering it.
    let other_dest = accepted(
        &mut engine,
        &report_on(
            screens,
            6,
            6,
            seed_facts,
            "win-2",
            Some(ingress(
                "win-2",
                GestureKind::Move,
                Some((1610, 300)),
                GesturePhase::Updated,
                r2,
            )),
            vec![],
            true,
        ),
    );
    let other_preview = other_dest.drag_preview.expect("out-c preview");
    assert_eq!(other_preview.domain.output.0, "out-c");
    assert_eq!(other_preview.target.0, "win-6");
    assert!(
        other_preview
            .hover
            .as_ref()
            .and_then(|h| h.prior.as_ref())
            .is_none()
    );
}

#[test]
fn unit_i_gesture_transfer_128_byte_window_and_max_counter_typed_route() {
    let w128 = "w".repeat(crate::bounds::MAX_OPAQUE_ID_LEN);
    let r1 = rect(0, 40, 2048, 1072);
    let r2 = rect(2072, 72, 1896, 1016);
    let seed_facts = vec![fact(&w128, "out-a", r1), fact("win-2", "out-b", r2)];
    let mut engine = seed_mixed(seed_facts.clone());

    // 1) Cancelled (rolled-back) gesture on 128-byte window leaves no transfer.
    let cancel_reply = accepted(
        &mut engine,
        &mixed_report(
            2,
            2,
            seed_facts.clone(),
            &w128,
            Some(ingress_id(
                &w128,
                GestureKind::Move,
                Some((r2.x + 10, r2.y + 10)),
                GesturePhase::Cancelled,
                r1,
                1,
            )),
            vec![],
            true,
        ),
    );
    assert_eq!(
        cancel_reply.gesture_settled.map(|t| (t.id, t.committed)),
        Some((1, false))
    );
    assert!(cancel_reply.transfers.is_empty());

    // 2) Cross-output gesture with 128-byte window and u64::MAX typed counter.
    let updated = accepted(
        &mut engine,
        &mixed_report(
            3,
            3,
            seed_facts.clone(),
            &w128,
            Some(ingress_id(
                &w128,
                GestureKind::Move,
                Some((r2.x + 10, r2.y + 10)),
                GesturePhase::Updated,
                r1,
                u64::MAX,
            )),
            vec![],
            true,
        ),
    );
    let preview = updated.drag_preview.as_ref().expect("drag preview");
    assert_eq!(preview.domain.output.0, "out-b");
    let leaf_b = engine
        .world
        .layout
        .leaves(&preview.domain)
        .and_then(|leaves| leaves.get(&WindowId("win-2".to_owned())))
        .expect("leaf for win-2");
    assert!(leaf_b.0.starts_with("leaf-"));
    assert!(leaf_b.0.len() <= crate::bounds::MAX_OPAQUE_ID_LEN);

    let mut release_facts = seed_facts.clone();
    release_facts[0] = fact(&w128, "out-a", rect(r1.x + 200, r1.y + 100, r1.w, r1.h));
    let released = accepted(
        &mut engine,
        &mixed_report(
            4,
            4,
            release_facts,
            &w128,
            Some(ingress_id(
                &w128,
                GestureKind::Move,
                Some((r2.x + 10, r2.y + 10)),
                GesturePhase::Released,
                r1,
                u64::MAX,
            )),
            vec![],
            true,
        ),
    );
    assert_eq!(
        released.gesture_settled.map(|t| (t.id, t.committed)),
        Some((u64::MAX, true))
    );
    let transfer = released
        .transfers
        .iter()
        .find(|t| matches!(&t.kind, TransferKind::Window(w) if w.0 == w128))
        .expect("cross-output transfer for 128-byte window");
    assert_eq!(transfer.operation_id, format!("gesture-{}", u64::MAX));
    assert!(transfer.operation_id.starts_with("gesture-"));
    assert!(!transfer.operation_id.starts_with("op-"));
    assert!(transfer.operation_id.len() <= crate::bounds::MAX_OPAQUE_ID_LEN);

    // 3) Wrong operation attempt on source capture does not retire the transfer,
    //    while exact timely attempt with identical pre-source/post-target does.
    let source_ws = engine
        .tx_ro(|tx| tx.effective_view(&OutputId("out-a".to_owned())))
        .expect("active view on out-a");
    let pre_ref = Some(crate::world_report::WorkspaceRef::Logical {
        output: OutputId("out-a".to_owned()),
        workspace: crate::directional::WorkspaceId(source_ws),
    });
    let post_ref = Some(crate::world_report::WorkspaceRef::Logical {
        output: transfer.target_output.clone(),
        workspace: crate::directional::WorkspaceId(transfer.target_workspace.clone()),
    });
    let mut wrong_report = mixed_report(5, 5, seed_facts.clone(), &w128, None, vec![], true);
    wrong_report.monotonic_ms = 5;
    wrong_report
        .member_attempts
        .push(crate::world_report::MemberAttempt {
            operation_id: "op-wrong".to_owned(),
            window: WindowId(w128.clone()),
            target_output: transfer.target_output.clone(),
            target_workspace: transfer.target_workspace.clone(),
            disposition: crate::world_report::MemberDisposition::Called(
                crate::world_report::FrameSetter::Accepted,
            ),
            pre: pre_ref.clone(),
            post: post_ref.clone(),
            attempt_ms: 5,
        });
    let wrong_reply = accepted(&mut engine, &wrong_report);
    assert!(
        wrong_reply
            .transfers
            .iter()
            .any(|t| t.operation_id == transfer.operation_id),
        "wrong operation attempt must not retire gesture transfer"
    );

    let mut ack_report = mixed_report(6, 6, seed_facts, &w128, None, vec![], true);
    ack_report.monotonic_ms = 6;
    ack_report
        .member_attempts
        .push(crate::world_report::MemberAttempt {
            operation_id: transfer.operation_id.clone(),
            window: WindowId(w128.clone()),
            target_output: transfer.target_output.clone(),
            target_workspace: transfer.target_workspace.clone(),
            disposition: crate::world_report::MemberDisposition::Called(
                crate::world_report::FrameSetter::Accepted,
            ),
            pre: pre_ref,
            post: post_ref,
            attempt_ms: 6,
        });
    let settled = accepted(&mut engine, &ack_report);
    assert!(
        settled
            .transfers
            .iter()
            .all(|t| t.operation_id != transfer.operation_id),
        "exact timely attempt retires gesture transfer"
    );

    // 4) Old generation report is fenced.
    let mut stale = ack_report;
    stale.core_generation = "gen-old".to_owned();
    match engine.handle_report(&stale, &[]).0 {
        WorldOutcome::Rejected { kind, .. } => {
            assert_eq!(kind, "generation-mismatch");
        }
        other => panic!("expected rejected stale generation, got {other:?}"),
    }
}

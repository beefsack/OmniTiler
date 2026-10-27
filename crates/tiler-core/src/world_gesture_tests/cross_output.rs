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
            gap: 0,
            outer_gap: 0,
            complete: true,
            primary: true,
        },
        ScreenReport {
            output: OutputId("out-b".to_owned()),
            full_rect: Some(rect(2048, 72, 1920, 1080)),
            work_area: Some(rect(2072, 72, 1896, 1016)),
            gap: 0,
            outer_gap: 0,
            complete: true,
            primary: false,
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
) -> WorldReport {
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

fn seed_mixed(seed_facts: Vec<WindowFact>) -> WorldEngine {
    let mut engine = WorldEngine::new("gen-1").expect("valid generation");
    accepted(
        &mut engine,
        &mixed_report(1, 1, seed_facts, "", None, vec![], true),
    );
    engine
}

/// Preview rect plus the resolving output name, for cross-output slot
/// agreement checks.
fn preview_slot(reply: &crate::world_report::WorldReply) -> (Rect, String) {
    let Some(CommandResult::DragPreview { rect, domain, .. }) = reply.command_result.clone() else {
        panic!(
            "expected drag-preview result, got {:?}",
            reply.command_result
        );
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
        released.command_result.is_none(),
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
    // pinned at the slot: settlement plus observed geometry.
    let target_view = transfer.target_workspace.clone();
    let arrival_facts: Vec<WindowFact> = seed_facts
        .iter()
        .map(|row| {
            if row.window.0 == mover {
                let mut arrived = fact(mover, dest_output, slot);
                arrived.observed_workspace = Some(target_view.clone());
                arrived
            } else {
                row.clone()
            }
        })
        .collect();
    let mut settle = mixed_report(5, 5, arrival_facts, mover, None, vec![], true);
    settle.views = vec![crate::world_report::ScreenView {
        output: OutputId(dest_output.to_owned()),
        view: target_view,
    }];
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
    // out-b at live coordinates.
    let back = row_of(&released1, "win-2")
        .allocation
        .expect("first release keeps win-2");
    let cursor2 = (back.x + 2, back.y + back.h / 2);
    let updated_facts: Vec<WindowFact> = seed_facts
        .iter()
        .map(|row| {
            if row.window.0 == "win-3" {
                fact("win-3", "out-a", moved1)
            } else {
                row.clone()
            }
        })
        .collect();
    accepted(
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
                moved1,
                2,
            )),
            vec![],
            true,
        ),
    );
    let moved2 = rect(2100, 200, start.w, start.h);
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
                moved1,
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
        gap: 0,
        outer_gap: 0,
        complete: true,
        primary: false,
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
) -> WorldReport {
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
    let mut engine = WorldEngine::new("gen-1").expect("valid generation");
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
        workspace: view_a.clone(),
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
    screens[1].outer_gap = 12;
    screens
}

fn gapped_report(
    request_seq: u64,
    observation_seq: u64,
    windows: Vec<WindowFact>,
    focused: &str,
    gesture: Option<GestureIngress>,
) -> WorldReport {
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
    let mut engine = WorldEngine::new("gen-1").expect("valid generation");
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
    let mut engine = WorldEngine::new("gen-1").expect("valid generation");
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
    let target_view = transfer.target_workspace.clone();
    let mut arrived = fact("win-2", "out-b", moved);
    arrived.observed_workspace = Some(target_view.clone());
    let mut settle = mixed_report(
        3,
        3,
        vec![fact("win-1", "out-a", r1), arrived],
        "win-2",
        None,
        vec![],
        true,
    );
    settle.views = vec![crate::world_report::ScreenView {
        output: OutputId("out-b".to_owned()),
        view: target_view,
    }];
    let settled = accepted(&mut engine, &settle);
    assert!(
        settled
            .transfers
            .iter()
            .all(|t| !matches!(&t.kind, TransferKind::Window(w) if w.0 == "win-2")),
        "arrival receipt retires the empty-destination transfer"
    );
}

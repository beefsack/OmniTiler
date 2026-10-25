#![forbid(unsafe_code)]

//! Offline tests for the pinned Dock-swipe selector: injected topology,
//! poster, latch, gate, and deadline seams only — never a WindowServer
//! post, never a live read. Latch fixtures drive notifications and
//! cancellation deterministically through clones; short injected deadlines
//! keep event tests fast (no 3 s waits from tests).

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::dock_swipe_select::{
    FullRow, GateReads, McCreateEffect, McExitGate, OwnerSelectBackend, PairsPosted,
    SELECT_ARRIVAL_DEADLINE_MS, SwipePoster, SwipeRequest, SwipeSeams, SwipeTopology,
    assemble_pinned_row, execute_dock_swipe_select,
};
use crate::ffi::{LatchWait, SlsDisplayRow, SlsSpaceType, SpaceSelectLatch, SpaceSelectWake};
use crate::runtime_types::ClaimedSelect;
use crate::workspace_select::{
    DispatchError, SelectGates, SelectionBackend, SelectionSnapshot, SelectionState, drain_claimed_select_with,
};
use tiler_core::world_report::FrameSetter;

/// Producer-shaped display identity fixture (managed-UUID shape, never a
/// machine UUID).
const FIX_UUID: &str = "550E8400-E29B-41D4-A716-446655440000";
const FIX_DISPLAY: &str = "550E8400-E29B-41D4-A716-446655440000";

struct FakeTopo {
    counts: VecDeque<Result<usize, String>>,
    rows: VecDeque<Result<FullRow, String>>,
    revisions: VecDeque<Option<u64>>,
    /// (read index, latch) pairs: the latch notifies when the indexed
    /// `fresh_full_row` call runs, deterministically interleaving a
    /// notification mid-execution on one thread.
    notify_on_read: Vec<(usize, crate::ffi::SpaceSelectLatch)>,
    reads: usize,
}

impl FakeTopo {
    fn row(display_id: &str, order: &[u64], current: u64) -> FullRow {
        FullRow {
            display_id: display_id.to_owned(),
            full_order: order.to_vec(),
            user_spaces: order.to_vec(),
            current,
        }
    }
}

impl SwipeTopology for FakeTopo {
    fn display_count(&mut self) -> Result<usize, String> {
        self.counts.pop_front().expect("scripted display count")
    }

    fn fresh_full_row(&mut self) -> Result<FullRow, String> {
        self.reads += 1;
        for (index, latch) in &self.notify_on_read {
            if *index == self.reads {
                latch.notify();
            }
        }
        self.rows.pop_front().expect("scripted full row")
    }

    fn revision(&mut self) -> Option<u64> {
        self.revisions.pop_front().flatten()
    }
}

struct FakePoster {
    reports: VecDeque<PairsPosted>,
    releases: VecDeque<Result<(), String>>,
    posted: Vec<(f64, usize)>,
    released: Vec<f64>,
}

impl SwipePoster for FakePoster {
    fn post_pairs(&mut self, sign: f64, pairs: usize) -> PairsPosted {
        self.posted.push((sign, pairs));
        self.reports.pop_front().expect("scripted post report")
    }

    fn release(&mut self, sign: f64) -> Result<(), String> {
        self.released.push(sign);
        self.releases.pop_front().unwrap_or(Ok(()))
    }
}

struct FakeGates {
    gates: VecDeque<SelectGates>,
}

impl GateReads for FakeGates {
    fn read_gates(&mut self) -> SelectGates {
        self.gates.pop_front().expect("scripted gates")
    }
}

/// Shared order log proving the exit watch arms BEFORE the append runs.
type EventLog = std::rc::Rc<std::cell::RefCell<Vec<String>>>;

/// Scripted pinned-create leg plus a shared order log proving the exit
/// watch arms BEFORE the append runs. Never opens Mission Control.
struct FakeCreate {
    results: VecDeque<Result<(), String>>,
    log: EventLog,
    calls: Vec<(usize, u32, String)>,
}

impl McCreateEffect for FakeCreate {
    fn create_append(
        &mut self,
        display: usize,
        cgid: u32,
        uuid: &str,
        _budget: &mut crate::mc_probe::ActionBudget,
    ) -> Result<(), DispatchError> {
        self.calls.push((display, cgid, uuid.to_owned()));
        self.log.borrow_mut().push("append".to_owned());
        self.results.pop_front().expect("scripted create result").map_err(DispatchError::threw)
    }
}

/// Scripted MC-exit gate: records arm/await/disarm calls plus the
/// cancellation flag observed at await time. Awaits mirror the production
/// one-shot shape only in counting (the fake never pumps a runloop).
struct FakeExitGate {
    arm_results: VecDeque<Result<(), String>>,
    await_results: VecDeque<Result<(), String>>,
    log: EventLog,
    saw_cancel_at_await: Vec<bool>,
    arms: usize,
    awaits: usize,
    disarms: usize,
}

impl FakeExitGate {
    fn new(log: EventLog) -> Self {
        Self {
            arm_results: VecDeque::new(),
            await_results: VecDeque::new(),
            log,
            saw_cancel_at_await: Vec::new(),
            arms: 0,
            awaits: 0,
            disarms: 0,
        }
    }
}

impl McExitGate for FakeExitGate {
    fn arm(&mut self) -> Result<(), crate::workspace_select::DispatchError> {
        self.arms += 1;
        self.log.borrow_mut().push("arm".to_owned());
        self.arm_results.pop_front().expect("scripted arm result").map_err(DispatchError::refused)
    }

    fn await_exit(&mut self, cancel: &AtomicBool) -> Result<(), crate::workspace_select::DispatchError> {
        self.awaits += 1;
        self.log.borrow_mut().push("await".to_owned());
        let flag = cancel.load(Ordering::SeqCst);
        self.saw_cancel_at_await.push(flag);
        // Mirror the production contract: an observed cancellation refuses
        // promptly without consuming a scripted outcome, so the test proves
        // the backend actually threads its cancel flag to the gate.
        if flag {
            return Err(DispatchError::refused("trailing post-create select refused: cancelled while awaiting Mission Control exit; created Space retained with proof; no gesture dispatched"));
        }
        self.await_results
            .pop_front()
            .expect("scripted await result")
            .map_err(DispatchError::refused)
    }

    fn disarm(&mut self) {
        self.disarms += 1;
        self.log.borrow_mut().push("disarm".to_owned());
    }
}

fn open_gates() -> SelectGates {
    SelectGates {
        session_unlocked: true,
        secure_active: false,
        lock_screen: false,
    }
}

fn topo_row(order: &[u64], current: u64) -> FakeTopo {
    FakeTopo {
        counts: vec![Ok(1)].into(),
        rows: vec![Ok(FakeTopo::row(FIX_DISPLAY, order, current))].into(),
        revisions: vec![].into(),
        notify_on_read: vec![],
        reads: 0,
    }
}

fn seams<'a>(
    topo: &'a mut FakeTopo,
    poster: &'a mut FakePoster,
    latch: &'a crate::ffi::SpaceSelectLatch,
    cancel: &'a AtomicBool,
    gates: &'a SelectGates,
) -> SwipeSeams<'a> {
    SwipeSeams {
        topo: topo as &mut dyn SwipeTopology,
        poster: poster as &mut dyn SwipePoster,
        latch,
        cancel,
        gates,
    }
}

fn request<'a>(
    target: u64,
    before: Option<&'a SelectionSnapshot>,
    op: Option<&'a str>,
    deadline_ms: u64,
) -> SwipeRequest<'a> {
    SwipeRequest {
        target,
        os_major: Some(26),
        before,
        op,
        t_base: Instant::now(),
        deadline: Duration::from_millis(deadline_ms),
    }
}

fn before_row(spaces: &[u64], current: u64) -> SelectionSnapshot {
    SelectionSnapshot {
        display: FIX_DISPLAY.to_owned(),
        spaces: spaces.to_vec(),
        current,
    }
}

#[test]
fn nonadjacent_both_directions_settle_on_exact_current() {
    for (order, current, target, sign, pairs) in [
        (vec![1001, 1002, 1003], 1001, 1003, 1.0, 2),
        (vec![1001, 1002, 1003], 1003, 1001, -1.0, 2),
    ] {
        // Call sequence: entry row, pre-post row (target already there),
        // so arrival proves with zero posts... instead script the
        // wake-driven path: pre-post still at source, loop reads target.
        let mut topo = FakeTopo {
            counts: vec![Ok(1), Ok(1)].into(),
            rows: vec![
                Ok(FakeTopo::row(FIX_DISPLAY, &order, current)),
                Ok(FakeTopo::row(FIX_DISPLAY, &order, current)),
                Ok(FakeTopo::row(FIX_DISPLAY, &order, target)),
            ]
            .into(),
            revisions: vec![].into(),
            notify_on_read: vec![],
            reads: 0,
        };
        let mut poster = FakePoster {
            reports: vec![PairsPosted {
                began_posted: pairs,
                ended_posted: pairs,
                pending_sign: None,
                error: None,
            }]
            .into(),
            releases: vec![].into(),
            posted: vec![],
            released: vec![],
        };
        let wake = SpaceSelectWake::new();
        let latch = wake.subscribe();
        let cancel = AtomicBool::new(false);
        let gates = open_gates();
        let before = before_row(&order, current);
        let changed = execute_dock_swipe_select(
            request(
                target,
                Some(&before),
                Some("op-1"),
                SELECT_ARRIVAL_DEADLINE_MS,
            ),
            seams(&mut topo, &mut poster, &latch, &cancel, &gates),
        )
        .expect("settles");
        assert!(changed);
        assert_eq!(poster.posted, vec![(sign, pairs)]);
        assert!(poster.released.is_empty());
    }
}

#[test]
fn intermediate_notification_then_final_arrival_confirms() {
    // Genuine event-driven path: loop read 1 misses, a notification fires
    // mid-execution, loop read 2 confirms. No deadline consumed.
    let order = vec![1001, 1002, 1003];
    let wake = SpaceSelectWake::new();
    let latch = wake.subscribe();
    let mut topo = FakeTopo {
        counts: vec![Ok(1), Ok(1)].into(),
        rows: vec![
            Ok(FakeTopo::row(FIX_DISPLAY, &order, 1001)),
            Ok(FakeTopo::row(FIX_DISPLAY, &order, 1001)),
            Ok(FakeTopo::row(FIX_DISPLAY, &order, 1001)),
            Ok(FakeTopo::row(FIX_DISPLAY, &order, 1003)),
        ]
        .into(),
        revisions: vec![].into(),
        notify_on_read: vec![(3, latch.clone())],
        reads: 0,
    };
    let mut poster = FakePoster {
        reports: vec![PairsPosted {
            began_posted: 2,
            ended_posted: 2,
            pending_sign: None,
            error: None,
        }]
        .into(),
        releases: vec![].into(),
        posted: vec![],
        released: vec![],
    };
    let cancel = AtomicBool::new(false);
    let gates = open_gates();
    let before = before_row(&order, 1001);
    let t0 = Instant::now();
    let changed = execute_dock_swipe_select(
        request(
            1003,
            Some(&before),
            Some("op-2"),
            SELECT_ARRIVAL_DEADLINE_MS,
        ),
        seams(&mut topo, &mut poster, &latch, &cancel, &gates),
    )
    .expect("event-driven arrival confirms");
    assert!(changed);
    assert!(t0.elapsed() < Duration::from_secs(3));
    assert_eq!(poster.posted, vec![(1.0, 2)]);
}

#[test]
fn early_notify_before_immediate_read_proves_without_posting() {
    // The notification landed before the first settlement read: the
    // already-there target proves with zero posts.
    let order = vec![1001, 1002];
    let wake = SpaceSelectWake::new();
    let latch = wake.subscribe();
    latch.notify();
    assert!(latch.generation() > 0);
    let mut topo = FakeTopo {
        counts: vec![Ok(1)].into(),
        rows: vec![
            Ok(FakeTopo::row(FIX_DISPLAY, &order, 1001)),
            Ok(FakeTopo::row(FIX_DISPLAY, &order, 1002)),
        ]
        .into(),
        revisions: vec![].into(),
        notify_on_read: vec![],
        reads: 0,
    };
    let mut poster = FakePoster {
        reports: vec![].into(),
        releases: vec![].into(),
        posted: vec![],
        released: vec![],
    };
    let cancel = AtomicBool::new(false);
    let gates = open_gates();
    let before = before_row(&order, 1001);
    let changed = execute_dock_swipe_select(
        request(1002, Some(&before), None, SELECT_ARRIVAL_DEADLINE_MS),
        seams(&mut topo, &mut poster, &latch, &cancel, &gates),
    )
    .expect("early arrival proves without posting");
    assert!(!changed);
    assert!(poster.posted.is_empty());
}

#[test]
fn mixed_space_types_traverse_fullscreen_inserts() {
    // Fullscreen SID 9001 sits between user Spaces in native order: the
    // plan walks the full order (2 pairs) while the target still resolves
    // from the user row.
    let order = vec![1001, 9001, 1003];
    let mut topo = FakeTopo {
        counts: vec![Ok(1), Ok(1)].into(),
        rows: vec![
            Ok(FullRow {
                display_id: FIX_DISPLAY.to_owned(),
                full_order: order.clone(),
                user_spaces: vec![1001, 1003],
                current: 1001,
            }),
            Ok(FullRow {
                display_id: FIX_DISPLAY.to_owned(),
                full_order: order.clone(),
                user_spaces: vec![1001, 1003],
                current: 1001,
            }),
            Ok(FullRow {
                display_id: FIX_DISPLAY.to_owned(),
                full_order: order.clone(),
                user_spaces: vec![1001, 1003],
                current: 1003,
            }),
        ]
        .into(),
        revisions: vec![].into(),
        notify_on_read: vec![],
        reads: 0,
    };
    let mut poster = FakePoster {
        reports: vec![PairsPosted {
            began_posted: 2,
            ended_posted: 2,
            pending_sign: None,
            error: None,
        }]
        .into(),
        releases: vec![].into(),
        posted: vec![],
        released: vec![],
    };
    let wake = SpaceSelectWake::new();
    let latch = wake.subscribe();
    let cancel = AtomicBool::new(false);
    let gates = open_gates();
    let before = before_row(&[1001, 1003], 1001);
    execute_dock_swipe_select(
        request(1003, Some(&before), None, SELECT_ARRIVAL_DEADLINE_MS),
        seams(&mut topo, &mut poster, &latch, &cancel, &gates),
    )
    .expect("cross-fullscreen settles");
    assert_eq!(poster.posted, vec![(1.0, 2)]);
}

#[test]
fn noop_posts_nothing() {
    let mut topo = topo_row(&[1001, 1002], 1001);
    topo.rows = vec![
        Ok(FakeTopo::row(FIX_DISPLAY, &[1001, 1002], 1001)),
        Ok(FakeTopo::row(FIX_DISPLAY, &[1001, 1002], 1001)),
    ]
    .into();
    let mut poster = FakePoster {
        reports: vec![].into(),
        releases: vec![].into(),
        posted: vec![],
        released: vec![],
    };
    let wake = SpaceSelectWake::new();
    let latch = wake.subscribe();
    let cancel = AtomicBool::new(false);
    let gates = open_gates();
    let before = before_row(&[1001, 1002], 1001);
    let changed = execute_dock_swipe_select(
        request(1001, Some(&before), None, SELECT_ARRIVAL_DEADLINE_MS),
        seams(&mut topo, &mut poster, &latch, &cancel, &gates),
    )
    .expect("noop settles");
    assert!(!changed);
    assert!(poster.posted.is_empty());
}

#[test]
fn entry_drift_against_before_refuses_without_posting() {
    // Claim-time row names [1001, 1002] but the fresh row moved on: refuse
    // before any post, never a stale ordinal.
    let mut topo = FakeTopo {
        counts: vec![Ok(1)].into(),
        rows: vec![Ok(FakeTopo::row(FIX_DISPLAY, &[1001, 1003], 1001))].into(),
        revisions: vec![].into(),
        notify_on_read: vec![],
        reads: 0,
    };
    let mut poster = FakePoster {
        reports: vec![].into(),
        releases: vec![].into(),
        posted: vec![],
        released: vec![],
    };
    let wake = SpaceSelectWake::new();
    let latch = wake.subscribe();
    let cancel = AtomicBool::new(false);
    let gates = open_gates();
    let before = before_row(&[1001, 1002], 1001);
    let err = execute_dock_swipe_select(
        request(1003, Some(&before), None, SELECT_ARRIVAL_DEADLINE_MS),
        seams(&mut topo, &mut poster, &latch, &cancel, &gates),
    )
    .expect_err("entry drift refuses");
    assert!(err.reason.starts_with("dock-swipe refused:"));
    assert!(poster.posted.is_empty());
}

#[test]
fn source_moved_pre_post_refuses_without_posting() {
    // Plan source 1001, but the pre-post read shows 1002 (something else
    // moved us): refuse rather than posting a stale distance.
    let order = vec![1001, 1002, 1003];
    let mut topo = FakeTopo {
        counts: vec![Ok(1)].into(),
        rows: vec![
            Ok(FakeTopo::row(FIX_DISPLAY, &order, 1001)),
            Ok(FakeTopo::row(FIX_DISPLAY, &order, 1002)),
        ]
        .into(),
        revisions: vec![].into(),
        notify_on_read: vec![],
        reads: 0,
    };
    let mut poster = FakePoster {
        reports: vec![].into(),
        releases: vec![].into(),
        posted: vec![],
        released: vec![],
    };
    let wake = SpaceSelectWake::new();
    let latch = wake.subscribe();
    let cancel = AtomicBool::new(false);
    let gates = open_gates();
    let before = before_row(&order, 1001);
    let err = execute_dock_swipe_select(
        request(1003, Some(&before), None, SELECT_ARRIVAL_DEADLINE_MS),
        seams(&mut topo, &mut poster, &latch, &cancel, &gates),
    )
    .expect_err("moved source refuses");
    assert!(err.reason.contains("moved"));
    assert!(poster.posted.is_empty());
}

#[test]
fn multi_display_refuses_before_any_post() {
    let mut topo = FakeTopo {
        counts: vec![Ok(2)].into(),
        rows: vec![].into(),
        revisions: vec![].into(),
        notify_on_read: vec![],
        reads: 0,
    };
    let mut poster = FakePoster {
        reports: vec![].into(),
        releases: vec![].into(),
        posted: vec![],
        released: vec![],
    };
    let wake = SpaceSelectWake::new();
    let latch = wake.subscribe();
    let cancel = AtomicBool::new(false);
    let gates = open_gates();
    let err = execute_dock_swipe_select(
        request(1002, None, None, SELECT_ARRIVAL_DEADLINE_MS),
        seams(&mut topo, &mut poster, &latch, &cancel, &gates),
    )
    .expect_err("multi-display refuses");
    assert!(err.reason.contains("exactly one native display"));
    assert!(poster.posted.is_empty());
}

#[test]
fn wrong_sid_arrival_is_unproven_without_replay() {
    // Both pairs posted, but reads keep showing other SIDs through the
    // short injected deadline: unproven, exactly one post call, no replay.
    let order = vec![1001, 1002, 1003];
    let mut topo = FakeTopo {
        counts: vec![Ok(1)].into(),
        rows: vec![
            Ok(FakeTopo::row(FIX_DISPLAY, &order, 1001)),
            Ok(FakeTopo::row(FIX_DISPLAY, &order, 1001)),
            Ok(FakeTopo::row(FIX_DISPLAY, &order, 1001)),
            Ok(FakeTopo::row(FIX_DISPLAY, &order, 1002)),
        ]
        .into(),
        revisions: vec![].into(),
        notify_on_read: vec![],
        reads: 0,
    };
    let mut poster = FakePoster {
        reports: vec![PairsPosted {
            began_posted: 2,
            ended_posted: 2,
            pending_sign: None,
            error: None,
        }]
        .into(),
        releases: vec![].into(),
        posted: vec![],
        released: vec![],
    };
    let wake = SpaceSelectWake::new();
    let latch = wake.subscribe();
    let cancel = AtomicBool::new(false);
    let gates = open_gates();
    let before = before_row(&order, 1001);
    let t0 = Instant::now();
    let err = execute_dock_swipe_select(
        request(1003, Some(&before), None, 120),
        seams(&mut topo, &mut poster, &latch, &cancel, &gates),
    )
    .expect_err("wrong SID is unproven");
    assert!(err.reason.starts_with("dock-swipe unproven:"));
    assert!(t0.elapsed() < Duration::from_secs(5));
    assert_eq!(poster.posted.len(), 1);
    assert!(poster.released.is_empty());
}

#[test]
fn multiple_wrong_then_deadline_takes_final_read() {
    // A notification per wrong SID still never confirms: the deadline
    // takes one final read and refuses unproven.
    let order = vec![1001, 1002, 1003];
    let wake = SpaceSelectWake::new();
    let latch = wake.subscribe();
    let mut topo = FakeTopo {
        counts: vec![Ok(1)].into(),
        rows: vec![
            Ok(FakeTopo::row(FIX_DISPLAY, &order, 1001)),
            Ok(FakeTopo::row(FIX_DISPLAY, &order, 1001)),
            Ok(FakeTopo::row(FIX_DISPLAY, &order, 1002)),
            Ok(FakeTopo::row(FIX_DISPLAY, &order, 1002)),
            Ok(FakeTopo::row(FIX_DISPLAY, &order, 1001)),
            Ok(FakeTopo::row(FIX_DISPLAY, &order, 1001)),
        ]
        .into(),
        revisions: vec![].into(),
        notify_on_read: vec![(3, latch.clone()), (4, latch.clone())],
        reads: 0,
    };
    let mut poster = FakePoster {
        reports: vec![PairsPosted {
            began_posted: 2,
            ended_posted: 2,
            pending_sign: None,
            error: None,
        }]
        .into(),
        releases: vec![].into(),
        posted: vec![],
        released: vec![],
    };
    let cancel = AtomicBool::new(false);
    let gates = open_gates();
    let before = before_row(&order, 1001);
    let err = execute_dock_swipe_select(
        request(1003, Some(&before), None, 150),
        seams(&mut topo, &mut poster, &latch, &cancel, &gates),
    )
    .expect_err("deadline after wrong SIDs is unproven");
    assert!(err.reason.starts_with("dock-swipe unproven:"));
    assert_eq!(poster.posted.len(), 1);
}

#[test]
fn no_event_final_read_is_unproven_within_injected_deadline() {
    // No notification ever arrives: the injected deadline bounds the wait
    // (fast deterministic test, no 3 s consumption) and the final read
    // still misses — unproven, no replay.
    let mut topo = topo_row(&[1001, 1002], 1001);
    topo.rows = vec![
        Ok(FakeTopo::row(FIX_DISPLAY, &[1001, 1002], 1001)),
        Ok(FakeTopo::row(FIX_DISPLAY, &[1001, 1002], 1001)),
        Ok(FakeTopo::row(FIX_DISPLAY, &[1001, 1002], 1001)),
        Ok(FakeTopo::row(FIX_DISPLAY, &[1001, 1002], 1001)),
    ]
    .into();
    let mut poster = FakePoster {
        reports: vec![PairsPosted {
            began_posted: 1,
            ended_posted: 1,
            pending_sign: None,
            error: None,
        }]
        .into(),
        releases: vec![].into(),
        posted: vec![],
        released: vec![],
    };
    let wake = SpaceSelectWake::new();
    let latch = wake.subscribe();
    let cancel = AtomicBool::new(false);
    let gates = open_gates();
    let before = before_row(&[1001, 1002], 1001);
    let t0 = Instant::now();
    let err = execute_dock_swipe_select(
        request(1002, Some(&before), None, 100),
        seams(&mut topo, &mut poster, &latch, &cancel, &gates),
    )
    .expect_err("deadline expiry is unproven");
    assert!(err.reason.starts_with("dock-swipe unproven:"));
    assert!(t0.elapsed() < Duration::from_secs(5));
    assert_eq!(poster.posted.len(), 1);
}

#[test]
fn partial_began_without_ended_releases_once_and_never_replays() {
    let mut topo = topo_row(&[1001, 1002, 1003], 1001);
    topo.rows = vec![
        Ok(FakeTopo::row(FIX_DISPLAY, &[1001, 1002, 1003], 1001)),
        Ok(FakeTopo::row(FIX_DISPLAY, &[1001, 1002, 1003], 1001)),
    ]
    .into();
    let mut poster = FakePoster {
        reports: vec![PairsPosted {
            began_posted: 1,
            ended_posted: 0,
            pending_sign: Some(1.0),
            error: Some("ended event creation returned null".to_owned()),
        }]
        .into(),
        releases: vec![Ok(())].into(),
        posted: vec![],
        released: vec![],
    };
    let wake = SpaceSelectWake::new();
    let latch = wake.subscribe();
    let cancel = AtomicBool::new(false);
    let gates = open_gates();
    let before = before_row(&[1001, 1002, 1003], 1001);
    let err = execute_dock_swipe_select(
        request(1003, Some(&before), None, SELECT_ARRIVAL_DEADLINE_MS),
        seams(&mut topo, &mut poster, &latch, &cancel, &gates),
    )
    .expect_err("partial is unproven");
    assert!(err.reason.contains("began-without-ended"));
    assert_eq!(poster.posted.len(), 1);
    assert_eq!(poster.released, vec![1.0]);
}

#[test]
fn zero_posted_pairs_refuse_proven_while_partial_holds() {
    // Nothing posted (first Began creation failed): proven refusal — the
    // desktop is untouched. A partially posted multi-pair dispatch is
    // ambiguous output and holds without replay.
    let mut topo = topo_row(&[1001, 1002, 1003], 1001);
    topo.rows = vec![
        Ok(FakeTopo::row(FIX_DISPLAY, &[1001, 1002, 1003], 1001)),
        Ok(FakeTopo::row(FIX_DISPLAY, &[1001, 1002, 1003], 1001)),
    ]
    .into();
    let mut poster = FakePoster {
        reports: vec![PairsPosted {
            began_posted: 0,
            ended_posted: 0,
            pending_sign: None,
            error: Some("began event creation returned null".to_owned()),
        }]
        .into(),
        releases: vec![].into(),
        posted: vec![],
        released: vec![],
    };
    let wake = SpaceSelectWake::new();
    let latch = wake.subscribe();
    let cancel = AtomicBool::new(false);
    let gates = open_gates();
    let before = before_row(&[1001, 1002, 1003], 1001);
    let err = execute_dock_swipe_select(
        request(1003, Some(&before), None, SELECT_ARRIVAL_DEADLINE_MS),
        seams(&mut topo, &mut poster, &latch, &cancel, &gates),
    )
    .expect_err("zero posted refuses");
    assert!(err.reason.starts_with("dock-swipe refused:"));

    let mut topo = topo_row(&[1001, 1002, 1003], 1001);
    topo.rows = vec![
        Ok(FakeTopo::row(FIX_DISPLAY, &[1001, 1002, 1003], 1001)),
        Ok(FakeTopo::row(FIX_DISPLAY, &[1001, 1002, 1003], 1001)),
    ]
    .into();
    let mut poster = FakePoster {
        reports: vec![PairsPosted {
            began_posted: 1,
            ended_posted: 1,
            pending_sign: None,
            error: None,
        }]
        .into(),
        releases: vec![].into(),
        posted: vec![],
        released: vec![],
    };
    let wake = SpaceSelectWake::new();
    let latch = wake.subscribe();
    let cancel = AtomicBool::new(false);
    let gates = open_gates();
    let err = execute_dock_swipe_select(
        request(1003, Some(&before), None, SELECT_ARRIVAL_DEADLINE_MS),
        seams(&mut topo, &mut poster, &latch, &cancel, &gates),
    )
    .expect_err("partial posting holds");
    assert!(err.reason.starts_with("dock-swipe unproven:"));
    assert_eq!(poster.posted.len(), 1);
    assert!(poster.released.is_empty());
}

#[test]
fn cancelled_job_refuses_without_posting() {
    let mut topo = topo_row(&[1001, 1002], 1001);
    let mut poster = FakePoster {
        reports: vec![].into(),
        releases: vec![].into(),
        posted: vec![],
        released: vec![],
    };
    let wake = SpaceSelectWake::new();
    let latch = wake.subscribe();
    let cancel = AtomicBool::new(true);
    let gates = open_gates();
    let before = before_row(&[1001, 1002], 1001);
    let err = execute_dock_swipe_select(
        request(1002, Some(&before), None, SELECT_ARRIVAL_DEADLINE_MS),
        seams(&mut topo, &mut poster, &latch, &cancel, &gates),
    )
    .expect_err("cancel refuses");
    assert!(err.reason.contains("cancelled"));
    assert!(poster.posted.is_empty());
}

#[test]
fn cancellation_wake_settles_promptly_without_deadline() {
    // A cancellation pulsed mid-wait returns promptly: far inside the
    // generous injected deadline, with no replay and no history advance.
    let mut topo = topo_row(&[1001, 1002], 1001);
    topo.rows = vec![
        Ok(FakeTopo::row(FIX_DISPLAY, &[1001, 1002], 1001)),
        Ok(FakeTopo::row(FIX_DISPLAY, &[1001, 1002], 1001)),
        Ok(FakeTopo::row(FIX_DISPLAY, &[1001, 1002], 1001)),
    ]
    .into();
    let mut poster = FakePoster {
        reports: vec![PairsPosted {
            began_posted: 1,
            ended_posted: 1,
            pending_sign: None,
            error: None,
        }]
        .into(),
        releases: vec![].into(),
        posted: vec![],
        released: vec![],
    };
    let wake = SpaceSelectWake::new();
    let latch = wake.subscribe();
    let canceller = latch.clone();
    let handle = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(30));
        canceller.cancel();
    });
    let cancel = AtomicBool::new(false);
    let gates = open_gates();
    let before = before_row(&[1001, 1002], 1001);
    let t0 = Instant::now();
    let err = execute_dock_swipe_select(
        request(1002, Some(&before), None, 30_000),
        seams(&mut topo, &mut poster, &latch, &cancel, &gates),
    )
    .expect_err("cancellation wake refuses");
    assert!(err.reason.contains("cancelled"));
    assert!(t0.elapsed() < Duration::from_secs(10));
    assert_eq!(poster.posted.len(), 1);
    let _ = handle.join();
}

#[test]
fn cancel_during_settlement_refuses_without_history() {
    // Cancel lands after the post-phase check but before settlement
    // completes: even a confirming final read refuses, so history never
    // advances on a cancelled job.
    struct CancellingTopo {
        reads: usize,
        cancel: Arc<AtomicBool>,
    }
    impl SwipeTopology for CancellingTopo {
        fn display_count(&mut self) -> Result<usize, String> {
            Ok(1)
        }

        fn fresh_full_row(&mut self) -> Result<FullRow, String> {
            self.reads += 1;
            let current = match self.reads {
                1 | 2 => 1001,
                _ => {
                    self.cancel.store(true, Ordering::SeqCst);
                    1001
                }
            };
            Ok(FullRow {
                display_id: FIX_DISPLAY.to_owned(),
                full_order: vec![1001, 1002],
                user_spaces: vec![1001, 1002],
                current,
            })
        }
    }
    struct FullPoster;
    impl SwipePoster for FullPoster {
        fn post_pairs(&mut self, _sign: f64, pairs: usize) -> PairsPosted {
            PairsPosted {
                began_posted: pairs,
                ended_posted: pairs,
                pending_sign: None,
                error: None,
            }
        }

        fn release(&mut self, _sign: f64) -> Result<(), String> {
            Ok(())
        }
    }
    let cancel = Arc::new(AtomicBool::new(false));
    let mut topo = CancellingTopo {
        reads: 0,
        cancel: Arc::clone(&cancel),
    };
    let mut poster = FullPoster;
    let wake = SpaceSelectWake::new();
    let latch = wake.subscribe();
    let gates = open_gates();
    let before = before_row(&[1001, 1002], 1001);
    // The flip lands on the third read (first settlement-loop read);
    // shrink the deadline so the test stays fast either way.
    let err = execute_dock_swipe_select(
        request(1002, Some(&before), None, 200),
        SwipeSeams {
            topo: &mut topo as &mut dyn SwipeTopology,
            poster: &mut poster as &mut dyn SwipePoster,
            latch: &latch,
            cancel: &cancel,
            gates: &gates,
        },
    )
    .expect_err("cancel during settlement refuses");
    assert!(err.reason.contains("cancelled"));
}

#[test]
fn pre_post_fullscreen_insert_refuses_without_posting() {
    // A fullscreen insert lands between the entry and pre-post reads while
    // the current SID is unchanged: the computed ordinal distance is stale,
    // so the dispatch refuses with zero posts.
    let row0_order = vec![1001, 1002, 1003];
    let pre_order = vec![1001, 9001, 1002, 1003];
    let mut topo = FakeTopo {
        counts: vec![Ok(1)].into(),
        rows: vec![
            Ok(FullRow {
                display_id: FIX_DISPLAY.to_owned(),
                full_order: row0_order.clone(),
                user_spaces: row0_order.clone(),
                current: 1001,
            }),
            Ok(FullRow {
                display_id: FIX_DISPLAY.to_owned(),
                full_order: pre_order,
                user_spaces: row0_order.clone(),
                current: 1001,
            }),
        ]
        .into(),
        revisions: vec![].into(),
        notify_on_read: vec![],
        reads: 0,
    };
    let mut poster = FakePoster {
        reports: vec![].into(),
        releases: vec![].into(),
        posted: vec![],
        released: vec![],
    };
    let wake = SpaceSelectWake::new();
    let latch = wake.subscribe();
    let cancel = AtomicBool::new(false);
    let gates = open_gates();
    let before = before_row(&row0_order, 1001);
    let err = execute_dock_swipe_select(
        request(1003, Some(&before), None, SELECT_ARRIVAL_DEADLINE_MS),
        seams(&mut topo, &mut poster, &latch, &cancel, &gates),
    )
    .expect_err("fullscreen insert pre-post refuses");
    assert!(err.reason.contains("drifted pre-post"), "{err}");
    assert!(poster.posted.is_empty());
}

#[test]
fn pre_post_identity_and_user_row_drift_refuse_without_posting() {
    // Same current SID, but the display identity or the user row moved
    // under the plan: stale ordinal, zero posts either way.
    for (display_id, user_spaces) in [
        (
            "DEADBEEF-DEAD-BEEF-DEAD-DEADBEEFDEAD",
            vec![1001, 1002, 1003],
        ),
        (FIX_DISPLAY, vec![1001, 1002, 1004]),
    ] {
        let order = vec![1001, 1002, 1003];
        let mut topo = FakeTopo {
            counts: vec![Ok(1)].into(),
            rows: vec![
                Ok(FakeTopo::row(FIX_DISPLAY, &order, 1001)),
                Ok(FullRow {
                    display_id: display_id.to_owned(),
                    full_order: order.clone(),
                    user_spaces: user_spaces.clone(),
                    current: 1001,
                }),
            ]
            .into(),
            revisions: vec![].into(),
            notify_on_read: vec![],
            reads: 0,
        };
        let mut poster = FakePoster {
            reports: vec![].into(),
            releases: vec![].into(),
            posted: vec![],
            released: vec![],
        };
        let wake = SpaceSelectWake::new();
        let latch = wake.subscribe();
        let cancel = AtomicBool::new(false);
        let gates = open_gates();
        let before = before_row(&order, 1001);
        let err = execute_dock_swipe_select(
            request(1003, Some(&before), None, SELECT_ARRIVAL_DEADLINE_MS),
            seams(&mut topo, &mut poster, &latch, &cancel, &gates),
        )
        .expect_err("identity/user-row drift pre-post refuses");
        assert!(err.reason.contains("drifted pre-post"), "{err}");
        assert!(poster.posted.is_empty());
    }
}

#[test]
fn target_arrival_under_drifted_topology_refuses_without_posting() {
    // The current already reads the target, but the full order moved
    // underneath: no-post proof is only honest on unchanged topology.
    let mut topo = FakeTopo {
        counts: vec![Ok(1)].into(),
        rows: vec![
            Ok(FakeTopo::row(FIX_DISPLAY, &[1001, 1002], 1001)),
            Ok(FakeTopo::row(FIX_DISPLAY, &[1001, 1002, 1003], 1002)),
        ]
        .into(),
        revisions: vec![].into(),
        notify_on_read: vec![],
        reads: 0,
    };
    let mut poster = FakePoster {
        reports: vec![].into(),
        releases: vec![].into(),
        posted: vec![],
        released: vec![],
    };
    let wake = SpaceSelectWake::new();
    let latch = wake.subscribe();
    let cancel = AtomicBool::new(false);
    let gates = open_gates();
    let before = before_row(&[1001, 1002], 1001);
    let err = execute_dock_swipe_select(
        request(1002, Some(&before), None, SELECT_ARRIVAL_DEADLINE_MS),
        seams(&mut topo, &mut poster, &latch, &cancel, &gates),
    )
    .expect_err("drifted no-post arrival refuses");
    assert!(err.reason.contains("drifted"), "{err}");
    assert!(poster.posted.is_empty());
}

#[test]
fn version_gate_refuses_without_posting() {
    // The executor consults the shared version gate over the resolved
    // major: >= 27 (and unreadable) refuse with zero posts; < 27 runs.
    // Both branches scripted — no live OS version read from tests.
    for major in [Some(27), Some(28), None] {
        let mut topo = topo_row(&[1001, 1002], 1001);
        let mut poster = FakePoster {
            reports: vec![].into(),
            releases: vec![].into(),
            posted: vec![],
            released: vec![],
        };
        let wake = SpaceSelectWake::new();
        let latch = wake.subscribe();
        let cancel = AtomicBool::new(false);
        let gates = open_gates();
        let req = SwipeRequest {
            target: 1002,
            os_major: major,
            before: None,
            op: None,
            t_base: Instant::now(),
            deadline: Duration::from_millis(100),
        };
        let err =
            execute_dock_swipe_select(req, seams(&mut topo, &mut poster, &latch, &cancel, &gates))
                .expect_err("new payload refuses");
        assert!(err.reason.starts_with("dock-swipe refused:"));
        assert!(poster.posted.is_empty());
    }
    let mut topo = topo_row(&[1001, 1002], 1001);
    topo.rows = vec![
        Ok(FakeTopo::row(FIX_DISPLAY, &[1001, 1002], 1001)),
        Ok(FakeTopo::row(FIX_DISPLAY, &[1001, 1002], 1002)),
    ]
    .into();
    let mut poster = FakePoster {
        reports: vec![].into(),
        releases: vec![].into(),
        posted: vec![],
        released: vec![],
    };
    let wake = SpaceSelectWake::new();
    let latch = wake.subscribe();
    let cancel = AtomicBool::new(false);
    let gates = open_gates();
    let outcome = execute_dock_swipe_select(
        request(1002, None, None, SELECT_ARRIVAL_DEADLINE_MS),
        seams(&mut topo, &mut poster, &latch, &cancel, &gates),
    );
    assert!(outcome.is_ok());
}

#[test]
fn confirmation_drift_refuses_despite_target_current() {
    // The current reads the target, but the full order moved underneath
    // (a concurrent create): unproven, history unchanged, no replay.
    let mut topo = FakeTopo {
        counts: vec![Ok(1), Ok(1)].into(),
        rows: vec![
            Ok(FakeTopo::row(FIX_DISPLAY, &[1001, 1002], 1001)),
            Ok(FakeTopo::row(FIX_DISPLAY, &[1001, 1002], 1001)),
            Ok(FakeTopo::row(FIX_DISPLAY, &[1001, 1002, 1003], 1002)),
        ]
        .into(),
        revisions: vec![].into(),
        notify_on_read: vec![],
        reads: 0,
    };
    let mut poster = FakePoster {
        reports: vec![PairsPosted {
            began_posted: 1,
            ended_posted: 1,
            pending_sign: None,
            error: None,
        }]
        .into(),
        releases: vec![].into(),
        posted: vec![],
        released: vec![],
    };
    let wake = SpaceSelectWake::new();
    let latch = wake.subscribe();
    let cancel = AtomicBool::new(false);
    let gates = open_gates();
    let before = before_row(&[1001, 1002], 1001);
    let err = execute_dock_swipe_select(
        request(1002, Some(&before), None, SELECT_ARRIVAL_DEADLINE_MS),
        seams(&mut topo, &mut poster, &latch, &cancel, &gates),
    )
    .expect_err("confirmation drift refuses");
    assert!(err.reason.contains("drifted"));
    assert_eq!(poster.posted.len(), 1);
}

#[test]
fn session_version_gates_reject_unknown_and_new_payloads() {
    use crate::space_switch_edges::gesture_payload_allowed;
    assert!(gesture_payload_allowed(Some(15)).is_ok());
    assert!(gesture_payload_allowed(Some(26)).is_ok());
    assert!(gesture_payload_allowed(Some(27)).is_err());
    assert!(gesture_payload_allowed(None).is_err());
}

#[test]
fn swipe_plan_mapping_is_native_order_not_user_row() {
    use crate::space_switch_edges::swipe_plan;
    // The count walks native order (fullscreen 9001 included); a
    // user-row-only distance would miscount it as adjacent.
    assert_eq!(swipe_plan(&[1001, 9001, 1003], 1001, 1003), Ok((1.0, 2)));
    assert_eq!(swipe_plan(&[1001, 9001, 1003], 1003, 1001), Ok((-1.0, 2)));
}

#[test]
fn own_switch_tag_bypass_contract_holds_for_key_shapes() {
    // The gesture route carries the own-switch tag for parity (the tap
    // observes key chords only, so gestures pass by construction); the
    // key-chord own-switch pass itself stays exact.
    assert_ne!(crate::input_match::OWN_SWITCH_USER_DATA, 0);
    assert!(crate::input_match::is_own_switch_chord(
        crate::input_match::OWN_SWITCH_USER_DATA,
        crate::input_match::TapKeyKind::KeyDown,
        crate::input_match::SWITCH_CTRL_BIT,
        i64::from(crate::input_match::SWITCH_KEY_RIGHT),
    ));
    assert!(!crate::input_match::is_own_switch_chord(
        0,
        crate::input_match::TapKeyKind::KeyDown,
        crate::input_match::SWITCH_CTRL_BIT,
        i64::from(crate::input_match::SWITCH_KEY_RIGHT),
    ));
    // Route pins: the multi-pair poster emits through the session tap
    // with the tagged pinned branch (source-shape pin, never a live
    // post from tests).
    let source = include_str!("space_switch_edges.rs");
    assert!(source.contains("SessionEventTap"));
    assert!(source.contains("post_gesture_pairs"));
    assert!(source.contains("OWN_SWITCH_USER_DATA"));
}

/// Producer-shaped SLS row fixture: managed-identifier UUID shape, native
/// order mixing user, fullscreen, and system Spaces, realistic SID scale.
fn producer_row() -> SlsDisplayRow {
    SlsDisplayRow {
        identifier: Some(FIX_UUID.to_owned()),
        spaces: vec![48201, 48202, 91500, 48203, 70001],
        current: Some(48201),
    }
}

fn producer_type(sid: u64) -> Result<SlsSpaceType, String> {
    match sid {
        48201..=48203 => Ok(SlsSpaceType::User),
        91500 => Ok(SlsSpaceType::Fullscreen),
        70001 => Ok(SlsSpaceType::System),
        _ => Ok(SlsSpaceType::Other(-1)),
    }
}

#[test]
fn pinned_row_assembly_proves_identity_order_and_types() {
    let row = producer_row();
    let full = assemble_pinned_row(
        &row.spaces,
        row.current,
        row.identifier.as_deref(),
        &producer_type,
        0,
        7,
        FIX_UUID,
    )
    .expect("producer row assembles");
    assert_eq!(full.display_id, FIX_UUID);
    assert_eq!(full.full_order, vec![48201, 48202, 91500, 48203, 70001]);
    assert_eq!(full.user_spaces, vec![48201, 48202, 48203]);
    assert_eq!(full.current, 48201);
}

#[test]
fn pinned_row_assembly_refuses_every_unpinned_shape() {
    let row = producer_row();
    // Zero CG ID.
    assert!(
        assemble_pinned_row(
            &row.spaces,
            row.current,
            row.identifier.as_deref(),
            &producer_type,
            0,
            0,
            FIX_UUID
        )
        .is_err()
    );
    // Stale publication (identifier mismatch).
    assert!(
        assemble_pinned_row(
            &row.spaces,
            row.current,
            row.identifier.as_deref(),
            &producer_type,
            0,
            7,
            "DEADBEEF-DEAD-BEEF-DEAD-DEADBEEFDEAD"
        )
        .is_err()
    );
    // Missing identity, empty order, missing current, current outside
    // order, unreadable type, and user-empty rows all refuse.
    assert!(
        assemble_pinned_row(
            &row.spaces,
            row.current,
            None,
            &producer_type,
            0,
            7,
            FIX_UUID
        )
        .is_err()
    );
    assert!(
        assemble_pinned_row(
            &[],
            Some(48201),
            row.identifier.as_deref(),
            &producer_type,
            0,
            7,
            FIX_UUID
        )
        .is_err()
    );
    assert!(
        assemble_pinned_row(
            &row.spaces,
            None,
            row.identifier.as_deref(),
            &producer_type,
            0,
            7,
            FIX_UUID
        )
        .is_err()
    );
    assert!(
        assemble_pinned_row(
            &row.spaces,
            Some(99999),
            row.identifier.as_deref(),
            &producer_type,
            0,
            7,
            FIX_UUID
        )
        .is_err()
    );
    assert!(
        assemble_pinned_row(
            &row.spaces,
            row.current,
            row.identifier.as_deref(),
            &|_| Err::<SlsSpaceType, String>("type symbols unavailable".to_owned()),
            0,
            7,
            FIX_UUID
        )
        .is_err()
    );
    assert!(
        assemble_pinned_row(
            &[91500, 70001],
            Some(91500),
            row.identifier.as_deref(),
            &producer_type,
            0,
            7,
            FIX_UUID
        )
        .is_err()
    );
}

#[test]
fn owner_backend_select_wires_before_gates_and_seams() {
    // Numbered select through the owner backend with every seam scripted:
    // the `before` row fences entry, owner-fresh gates authorize, and the
    // swipe settles on the exact SID.
    let order = vec![48201, 48202, 48203];
    let topo = FakeTopo {
        counts: vec![Ok(1), Ok(1)].into(),
        rows: vec![
            Ok(FakeTopo::row(FIX_DISPLAY, &order, 48201)),
            Ok(FakeTopo::row(FIX_DISPLAY, &order, 48201)),
            Ok(FakeTopo::row(FIX_DISPLAY, &order, 48203)),
        ]
        .into(),
        revisions: vec![].into(),
        notify_on_read: vec![],
        reads: 0,
    };
    let poster = FakePoster {
        reports: vec![PairsPosted {
            began_posted: 2,
            ended_posted: 2,
            pending_sign: None,
            error: None,
        }]
        .into(),
        releases: vec![].into(),
        posted: vec![],
        released: vec![],
    };
    let wake = SpaceSelectWake::new();
    let latch = Arc::new(wake.subscribe());
    let cancel = Arc::new(AtomicBool::new(false));
    let legs = crate::dock_swipe_select::PinnedLegs {
        display_index: 0,
        pinned_cgid: 7,
        pinned_uuid: FIX_UUID.to_owned(),
        published_displays: vec![],
        published_separate: None,
        settle_deadline: Duration::from_millis(3000),
    };
    let mut backend = OwnerSelectBackend::pinned_with(crate::dock_swipe_select::OwnerBackendArgs {
        legs,
        latch,
        cancel,
        budget_secs: 30,
        gate_reads: Box::new(FakeGates {
            gates: vec![open_gates()].into(),
        }),
        creator: Box::new(FakeCreate {
            results: VecDeque::new(),
            log: std::rc::Rc::new(std::cell::RefCell::new(Vec::new())),
            calls: Vec::new(),
        }),
        exit_gate: Box::new(FakeExitGate::new(std::rc::Rc::new(
            std::cell::RefCell::new(Vec::new()),
        ))),
        topo,
        poster,
        op: Some("op-9".to_owned()),
        t_base: Instant::now(),
    });
    let before = before_row(&order, 48201);
    backend
        .select(&before, 48203)
        .expect("owner backend select settles");
}

#[test]
fn owner_backend_create_refuses_cancelled_and_locked_without_touch() {
    // Cancellation and owner-fresh locked gates refuse the create leg
    // before any topology or MC touch (empty scripted queues would panic
    // on any read).
    for gates in [
        None,
        Some(SelectGates {
            session_unlocked: false,
            secure_active: false,
            lock_screen: false,
        }),
        Some(SelectGates {
            session_unlocked: true,
            secure_active: true,
            lock_screen: false,
        }),
        Some(SelectGates {
            session_unlocked: true,
            secure_active: false,
            lock_screen: true,
        }),
    ] {
        let topo = FakeTopo {
            counts: vec![].into(),
            rows: vec![].into(),
            revisions: vec![].into(),
            notify_on_read: vec![],
            reads: 0,
        };
        let poster = FakePoster {
            reports: vec![].into(),
            releases: vec![].into(),
            posted: vec![],
            released: vec![],
        };
        let wake = SpaceSelectWake::new();
        let latch = Arc::new(wake.subscribe());
        let cancel = Arc::new(AtomicBool::new(gates.is_none()));
        let legs = crate::dock_swipe_select::PinnedLegs {
            display_index: 0,
            pinned_cgid: 7,
            pinned_uuid: FIX_UUID.to_owned(),
            published_displays: vec![],
            published_separate: None,
            settle_deadline: Duration::from_millis(100),
        };
        let mut backend =
            OwnerSelectBackend::pinned_with(crate::dock_swipe_select::OwnerBackendArgs {
                legs,
                latch,
                cancel,
                budget_secs: 30,
                gate_reads: Box::new(FakeGates {
                    gates: gates.map(|g| vec![g]).unwrap_or_default().into(),
                }),
                creator: Box::new(FakeCreate {
                    results: VecDeque::new(),
                    log: std::rc::Rc::new(std::cell::RefCell::new(Vec::new())),
                    calls: Vec::new(),
                }),
                exit_gate: Box::new(FakeExitGate::new(std::rc::Rc::new(
                    std::cell::RefCell::new(Vec::new()),
                ))),
                topo,
                poster,
                op: None,
                t_base: Instant::now(),
            });
        let before = before_row(&[48201], 48201);
        assert!(
            backend.create(&before).is_err(),
            "cancelled/locked create refuses without touch"
        );
    }
}

// --- Post-create Mission Control exit gate (Dock AXExposeExit) ---
//
// Regression for the reproduced product failure: after a trailing create,
// the two-pair gesture posted while Mission Control was still Dock-active
// had no effect through the 3 s deadline. The AX `mc`-group read the close
// poll uses can report absent while Dock still gates gestures
// (`space_manager_focus_space` refuses under `mission_control_is_active`,
// which clears only on Dock `AXExposeExit` — yabai `dd84572`
// `src/mission_control.c`). These tests drive the production
// `OwnerSelectBackend` create/select legs with scripted topology, poster,
// create, and exit-gate seams: no WindowServer post, no live read.

fn paired_poster(pairs: usize) -> FakePoster {
    FakePoster {
        reports: vec![PairsPosted {
            began_posted: pairs,
            ended_posted: pairs,
            pending_sign: None,
            error: None,
        }]
        .into(),
        releases: vec![].into(),
        posted: vec![],
        released: vec![],
    }
}

fn trailing_topo() -> FakeTopo {
    // Call order: create drift snapshot ([71,72] cur 72), select entry
    // row ([71,72,73] cur 72), pre-post row (unchanged), settle arrival
    // (target 73 current with identical topology).
    FakeTopo {
        counts: vec![Ok(1), Ok(1)].into(),
        rows: vec![
            Ok(FakeTopo::row(FIX_DISPLAY, &[71, 72], 72)),
            Ok(FakeTopo::row(FIX_DISPLAY, &[71, 72, 73], 72)),
            Ok(FakeTopo::row(FIX_DISPLAY, &[71, 72, 73], 72)),
            Ok(FakeTopo::row(FIX_DISPLAY, &[71, 72, 73], 73)),
        ]
        .into(),
        revisions: vec![].into(),
        notify_on_read: vec![],
        reads: 0,
    }
}

#[allow(clippy::too_many_arguments)]
fn trailing_backend(
    topo: FakeTopo,
    poster: FakePoster,
    gates: Vec<SelectGates>,
    creator_results: Vec<Result<(), String>>,
    arm_results: Vec<Result<(), String>>,
    await_results: Vec<Result<(), String>>,
) -> (
    OwnerSelectBackend<FakeTopo, FakePoster>,
    EventLog,
    Arc<AtomicBool>,
) {
    let log = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let wake = SpaceSelectWake::new();
    let latch = Arc::new(wake.subscribe());
    let cancel = Arc::new(AtomicBool::new(false));
    let legs = crate::dock_swipe_select::PinnedLegs {
        display_index: 0,
        pinned_cgid: 7,
        pinned_uuid: FIX_UUID.to_owned(),
        published_displays: vec![],
        published_separate: None,
        settle_deadline: Duration::from_millis(100),
    };
    let mut gate = FakeExitGate::new(std::rc::Rc::clone(&log));
    gate.arm_results = arm_results.into();
    gate.await_results = await_results.into();
    let backend = OwnerSelectBackend::pinned_with(crate::dock_swipe_select::OwnerBackendArgs {
        legs,
        latch,
        cancel: Arc::clone(&cancel),
        budget_secs: 30,
        gate_reads: Box::new(FakeGates {
            gates: gates.into(),
        }),
        creator: Box::new(FakeCreate {
            results: creator_results.into(),
            log: std::rc::Rc::clone(&log),
            calls: Vec::new(),
        }),
        exit_gate: Box::new(gate),
        topo,
        poster,
        op: Some("op-exit".to_owned()),
        t_base: Instant::now(),
    });
    (backend, log, cancel)
}

const EXIT_TIMEOUT: &str = "trailing post-create select refused: Mission Control exit (Dock AXExposeExit) unobserved within 2000ms (wedged-Dock guard); created Space retained with proof; no gesture dispatched";

#[test]
fn post_create_select_dispatches_no_gesture_until_exit_observed() {
    // Core regression: the exit never arrives, so the select refuses and
    // posts zero gesture pairs — the reproduced no-effect dispatch cannot
    // recur. The shared order log proves the watch armed BEFORE the append.
    let (mut backend, log, _cancel) = trailing_backend(
        trailing_topo(),
        paired_poster(1),
        vec![open_gates(), open_gates()],
        vec![Ok(())],
        vec![Ok(())],
        vec![Err(EXIT_TIMEOUT.to_owned())],
    );
    backend
        .create(&before_row(&[71, 72], 72))
        .expect("scripted append succeeds");
    let err = backend
        .select(&before_row(&[71, 72, 73], 72), 73)
        .expect_err("unobserved exit refuses the post-create select");
    assert!(
        err.reason.contains("no gesture dispatched"),
        "refusal names the no-dispatch contract: {err}"
    );
    assert!(
        backend.test_poster().posted.is_empty(),
        "no gesture pair posts before the exit: {:?}",
        backend.test_poster().posted
    );
    assert!(
        backend.test_poster().released.is_empty(),
        "no release edge posts on the refused path"
    );
    assert_eq!(
        *log.borrow(),
        vec!["arm", "append", "await"],
        "watch arms before the append, await runs at select"
    );
}

#[test]
fn post_create_select_posts_once_exit_observed() {
    // Exact exit delivered: the single-pair gesture posts and the select
    // settles on the fresh target read.
    let (mut backend, log, _cancel) = trailing_backend(
        trailing_topo(),
        paired_poster(1),
        vec![open_gates(), open_gates()],
        vec![Ok(())],
        vec![Ok(())],
        vec![Ok(())],
    );
    backend
        .create(&before_row(&[71, 72], 72))
        .expect("scripted append succeeds");
    backend
        .select(&before_row(&[71, 72, 73], 72), 73)
        .expect("observed exit releases the post-create select");
    assert_eq!(
        backend.test_poster().posted,
        vec![(1.0, 1)],
        "exactly the planned pair posts after the exit"
    );
    assert_eq!(*log.borrow(), vec!["arm", "append", "await"]);
}

#[test]
fn exit_watch_arm_failure_refuses_create_pre_effect() {
    // Registration failure refuses BEFORE any append: nothing created,
    // nothing posted, nothing to adopt.
    let (mut backend, log, _cancel) = trailing_backend(
        trailing_topo(),
        paired_poster(1),
        vec![open_gates()],
        vec![],
        vec![Err(
            "exit watch refused pre-effect: AXObserverCreate pid 42: kAXErrorAPIDisabled"
                .to_owned(),
        )],
        vec![],
    );
    let err = backend
        .create(&before_row(&[71, 72], 72))
        .expect_err("failed arm refuses pre-effect");
    assert!(
        err.reason.contains("trailing create refused"),
        "refusal is honest pre-effect: {err}"
    );
    assert_eq!(
        *log.borrow(),
        vec!["arm"],
        "no append runs after a refused arm"
    );
    assert!(
        backend.test_poster().posted.is_empty(),
        "no gesture posts without an append"
    );
}

#[test]
fn create_failure_disarms_unawaited_watch() {
    // The append itself fails after a successful arm: the watch tears down
    // promptly, and the follow-up select runs as an ordinary (unarmed)
    // select — no stale await, no leaked registration.
    let (mut backend, log, _cancel) = trailing_backend(
        trailing_topo(),
        paired_poster(1),
        vec![open_gates(), open_gates()],
        vec![Err("mc gone".to_owned())],
        vec![Ok(())],
        vec![],
    );
    let err = backend
        .create(&before_row(&[71, 72], 72))
        .expect_err("failed append refuses");
    assert_eq!(err.reason, "mc gone");
    backend
        .select(&before_row(&[71, 72, 73], 72), 73)
        .expect("disarmed watch leaves an ordinary select");
    assert_eq!(
        backend.test_poster().posted,
        vec![(1.0, 1)],
        "the ordinary select still posts its pair"
    );
    assert_eq!(
        *log.borrow(),
        vec!["arm", "append", "disarm"],
        "armed-but-never-awaited watch disarms on the create failure"
    );
}

#[test]
fn normal_existing_select_arms_no_exit_watch() {
    // Non-trailing selects never open Mission Control for the create leg,
    // so they arm no watch and pay no wait.
    let topo = FakeTopo {
        counts: vec![Ok(1), Ok(1)].into(),
        rows: vec![
            Ok(FakeTopo::row(FIX_DISPLAY, &[71, 72, 73], 71)),
            Ok(FakeTopo::row(FIX_DISPLAY, &[71, 72, 73], 71)),
            Ok(FakeTopo::row(FIX_DISPLAY, &[71, 72, 73], 73)),
        ]
        .into(),
        revisions: vec![].into(),
        notify_on_read: vec![],
        reads: 0,
    };
    let (mut backend, log, _cancel) = trailing_backend(
        topo,
        paired_poster(2),
        vec![open_gates()],
        vec![],
        vec![],
        vec![],
    );
    backend
        .select(&before_row(&[71, 72, 73], 71), 73)
        .expect("existing select settles");
    assert_eq!(backend.test_poster().posted, vec![(1.0, 2)]);
    assert!(
        log.borrow().is_empty(),
        "no arm/await/disarm on the normal path: {:?}",
        *log.borrow()
    );
}

#[test]
fn post_create_exit_wait_answers_cancellation() {
    // The job cancel flag reaches the gate: a cancelled wait refuses
    // promptly with no gesture post.
    let (mut backend, log, cancel) = trailing_backend(
        trailing_topo(),
        paired_poster(1),
        vec![open_gates(), open_gates()],
        vec![Ok(())],
        vec![Ok(())],
        vec![],
    );
    backend
        .create(&before_row(&[71, 72], 72))
        .expect("scripted append succeeds");
    cancel.store(true, Ordering::SeqCst);
    let err = backend
        .select(&before_row(&[71, 72, 73], 72), 73)
        .expect_err("cancelled exit wait refuses");
    assert!(
        err.reason.contains("cancelled while awaiting Mission Control exit"),
        "cancellation refuses honestly: {err}"
    );
    assert!(
        backend.test_poster().posted.is_empty(),
        "no gesture posts on the cancelled path"
    );
    assert_eq!(*log.borrow(), vec!["arm", "append", "await"]);
}

#[test]
fn mc_exit_notification_matches_exact_dock_name_only() {
    // Host-boundary name contract: exactly `AXExposeExit` signals — the
    // yabai show/entry siblings and every other AX name never do.
    assert!(crate::ffi::is_mc_exit_notification("AXExposeExit"));
    for wrong in [
        "AXExposeShowAllWindows",
        "AXExposeShowFrontWindows",
        "AXExposeShowDesktop",
        "AXWindowCreated",
        "AXFocusedWindowChanged",
        "AXMoved",
        "AXUIElementDestroyed",
        "",
        "axexposeexit",
        "AXExposeExit ",
        " AXExposeExit",
        "AXExposeExitX",
    ] {
        assert!(
            !crate::ffi::is_mc_exit_notification(wrong),
            "wrong AX name must not signal: {wrong:?}"
        );
    }
}

#[test]
fn dock_exit_delegate_signals_exact_exit_and_retains_early_delivery() {
    // Injected event stream at the safe host delegate (no live AX): wrong
    // notifications ignored, the exact exit advances the latch, and an
    // exit delivered before the waiter starts is retained (the
    // before-close-returns case needs no re-wait).
    let latch = SpaceSelectLatch::detached();
    let from = latch.generation();
    for wrong in ["AXExposeShowAllWindows", "AXWindowCreated", "AXMoved"] {
        assert!(
            !crate::ffi::handle_dock_exit_notification(wrong, &latch),
            "wrong notification must not signal"
        );
    }
    assert_eq!(
        latch.generation(),
        from,
        "wrong notifications leave the generation untouched"
    );
    assert!(
        crate::ffi::handle_dock_exit_notification("AXExposeExit", &latch),
        "exact exit signals"
    );
    assert_ne!(latch.generation(), from);
    // Early delivery retained: a wait starting after the signal returns
    // at once instead of timing out.
    match latch.wait(from, Duration::from_millis(20)) {
        LatchWait::Advanced(next) => assert_eq!(next, latch.generation()),
        other => panic!("early exit must be retained, got {other:?}"),
    }
    // No delivery: a bounded wait times out (genuine bound, never a hang).
    let quiet = SpaceSelectLatch::detached();
    match quiet.wait(quiet.generation(), Duration::from_millis(20)) {
        LatchWait::TimedOut => {}
        other => panic!("quiet latch must time out, got {other:?}"),
    }
    // Cancellation pulses promptly without faking a delivery.
    let cancelled = SpaceSelectLatch::detached();
    let before = cancelled.generation();
    cancelled.cancel();
    match cancelled.wait(before, Duration::from_millis(20)) {
        LatchWait::Cancelled => {}
        other => panic!("cancelled latch must report cancel, got {other:?}"),
    }
    assert_eq!(cancelled.generation(), before);
}

/// Fully scripted backend proving the shared `execute` contract this fix
/// relies on: a select-phase refusal AFTER a verified append still leaves
/// the exact append proof adoptable (the exit-gate failure shape), so no
/// created Space is ever silently lost.
struct ProofBackend {
    snapshots: VecDeque<SelectionSnapshot>,
    empty_result: bool,
    select_result: Result<(), String>,
    selected: Vec<(SelectionSnapshot, u64)>,
}

impl SelectionBackend for ProofBackend {
    fn snapshot(&mut self) -> Result<SelectionSnapshot, String> {
        Ok(self.snapshots.pop_front().expect("scripted snapshot"))
    }

    fn display_count(&mut self) -> Result<usize, String> {
        Ok(1)
    }

    fn revision(&mut self) -> Option<u64> {
        None
    }

    fn check_output(&mut self, output: &str) -> Result<(), String> {
        if output == "macos-display-1" {
            Ok(())
        } else {
            Err(format!(
                "product select refuses: reply target output {output} is not the live display macos-display-1; no actuation"
            ))
        }
    }

    fn empty(&mut self, _sid: u64) -> Result<bool, String> {
        Ok(self.empty_result)
    }

    fn create(&mut self, _before: &SelectionSnapshot) -> Result<(), crate::workspace_select::DispatchError> {
        Ok(())
    }

    fn select(&mut self, before: &SelectionSnapshot, sid: u64) -> Result<(), crate::workspace_select::DispatchError> {
        self.selected.push((before.clone(), sid));
        self.select_result.clone().map_err(DispatchError::refused)
    }
}

#[test]
fn execute_preserves_append_proof_when_post_create_select_refuses() {
    use crate::runtime_types::{ClaimedSelect, InputSemanticAction};
    use crate::workspace_select::{SelectionState, TrailingAppendProof};

    let mut state = SelectionState::default();
    let mut backend = ProofBackend {
        snapshots: vec![
            before_row(&[71, 72], 72),
            before_row(&[71, 72, 73], 72),
            before_row(&[71, 72, 73], 72),
            before_row(&[71, 72, 73], 72),
        ]
        .into(),
        empty_result: false,
        select_result: Err(EXIT_TIMEOUT.to_owned()),
        selected: Vec::new(),
    };
    // Reply plan authorizes the trailing creation (unbound target plus
    // explicit desire); the post-create select then refuses via the
    // scripted exit-gate failure. The verified append proof must still be
    // adoptable.
    let mut claim = ClaimedSelect::tap(InputSemanticAction::WorkspaceSelect { index: 0 });
    claim.operation_id = Some("op-9".to_owned());
    claim.select_target_output = Some("macos-display-1".to_owned());
    claim.select_target_workspace = Some("ws-new".to_owned());
    claim.select_native_binding = None;
    claim.select_creation_operation = Some("op-native-1".to_owned());
    claim.select_creation_logical = Some("ws-new".to_owned());
    claim.select_creation_output = Some("macos-display-1".to_owned());
    claim.select_baseline_sid = Some(72);
    let failure = crate::workspace_select::drain_claimed_select_with(
        &mut state,
        &mut backend,
        claim,
        &crate::workspace_select::SelectGates {
            session_unlocked: true,
            secure_active: false,
            lock_screen: false,
        },
        true,
    )
    .expect_err("exit-gate refusal fails the select");
    assert!(
        failure.reason.contains("created SID Some(73)"),
        "refusal carries the created SID for adoption: {}",
        failure.reason
    );
    assert_eq!(
        failure.setter,
        tiler_core::world_report::FrameSetter::Threw,
        "ambiguous post-actuation output holds, never a proven refusal"
    );
    assert_eq!(failure.pre, Some(72));
    assert_eq!(failure.baseline, Some(72));
    // The creation leg itself verified: its report rides the failure.
    let creation = failure.creation.expect("append proof rides the failure");
    assert_eq!(creation.operation_id, "op-native-1");
    assert_eq!(
        creation.setter,
        tiler_core::world_report::FrameSetter::Accepted
    );
    assert_eq!(creation.order_after, vec![71, 72, 73]);
    assert_eq!(creation.created, Some(73));
    assert_eq!(
        backend.selected,
        vec![(before_row(&[71, 72, 73], 72), 73)],
        "the refused select still targeted the exact append"
    );
    assert_eq!(
        state.take_trailing_proof(),
        Some(TrailingAppendProof {
            display_id: FIX_DISPLAY.to_owned(),
            before: vec![71, 72],
            after: vec![71, 72, 73],
            created: 73,
        }),
        "append proof survives the select-phase refusal for lifecycle adoption"
    );
}

/// Reply-plan claim for the async drain tests below: the engine would bind
/// these from the accepted reply; tests inject them directly.
fn drain_plan(
    action: crate::runtime_types::InputSemanticAction,
    op: &str,
    binding: &str,
    baseline: u64,
) -> ClaimedSelect {
    let mut claim = ClaimedSelect::tap(action);
    claim.operation_id = Some(op.to_owned());
    claim.select_target_output = Some("macos-display-7".to_owned());
    claim.select_target_workspace = Some("ws-async".to_owned());
    claim.select_native_binding = Some(binding.to_owned());
    claim.select_baseline_sid = Some(baseline);
    claim
}

/// Async drain backend over scripted seams: pinned to CG display 7
/// (`macos-display-7`), so the output fence passes for the async plans.
fn async_backend(
    topo: FakeTopo,
    poster: FakePoster,
    deadline_ms: u64,
) -> (OwnerSelectBackend<FakeTopo, FakePoster>, Arc<AtomicBool>) {
    let wake = SpaceSelectWake::new();
    let latch = Arc::new(wake.subscribe());
    let cancel = Arc::new(AtomicBool::new(false));
    let legs = crate::dock_swipe_select::PinnedLegs {
        display_index: 0,
        pinned_cgid: 7,
        pinned_uuid: FIX_UUID.to_owned(),
        published_displays: vec![],
        published_separate: None,
        settle_deadline: Duration::from_millis(deadline_ms),
    };
    let backend = OwnerSelectBackend::pinned_with(crate::dock_swipe_select::OwnerBackendArgs {
        legs,
        latch,
        cancel: Arc::clone(&cancel),
        budget_secs: 30,
        gate_reads: Box::new(FakeGates {
            gates: vec![open_gates(), open_gates()].into(),
        }),
        creator: Box::new(FakeCreate {
            results: VecDeque::new(),
            log: std::rc::Rc::new(std::cell::RefCell::new(Vec::new())),
            calls: Vec::new(),
        }),
        exit_gate: Box::new(FakeExitGate::new(std::rc::Rc::new(
            std::cell::RefCell::new(Vec::new()),
        ))),
        topo,
        poster,
        op: Some("op-async".to_owned()),
        t_base: Instant::now(),
    });
    (backend, cancel)
}

fn async_row(order: &[u64], current: u64) -> FullRow {
    FakeTopo::row(FIX_DISPLAY, order, current)
}

#[test]
fn drain_delayed_arrival_settles_accepted_without_replay() {
    // Slow WindowServer: the target appears only on a later settlement
    // read, after the bounded wait times out once. The production drain
    // still settles Accepted with genuine reads and posts exactly once -
    // never an invented Threw, never a replay.
    let order = vec![1001, 1002, 1003];
    let topo = FakeTopo {
        counts: vec![Ok(1), Ok(1), Ok(1)].into(),
        rows: vec![
            Ok(async_row(&order, 1001)),
            Ok(async_row(&order, 1001)),
            Ok(async_row(&order, 1001)),
            Ok(async_row(&order, 1001)),
            Ok(async_row(&order, 1003)),
            Ok(async_row(&order, 1003)),
        ]
        .into(),
        revisions: vec![].into(),
        notify_on_read: vec![],
        reads: 0,
    };
    let poster = FakePoster {
        reports: vec![PairsPosted {
            began_posted: 2,
            ended_posted: 2,
            pending_sign: None,
            error: None,
        }]
        .into(),
        releases: vec![].into(),
        posted: vec![],
        released: vec![],
    };
    let (mut backend, _cancel) = async_backend(topo, poster, 150);
    let mut state = SelectionState::default();
    let success = drain_claimed_select_with(
        &mut state,
        &mut backend,
        drain_plan(
            crate::runtime_types::InputSemanticAction::WorkspaceSelect { index: 3 },
            "op-async-1",
            "space-1003",
            1001,
        ),
        &open_gates(),
        true,
    )
    .expect("delayed arrival settles");
    assert_eq!(success.setter, FrameSetter::Accepted);
    assert_eq!((success.pre, success.post), (Some(1001), Some(1003)));
    assert_eq!(success.effect.target, 1003);
    assert!(success.effect.changed);
    assert_eq!(backend.test_poster().posted, vec![(1.0, 2)]);
    assert!(backend.test_poster().released.is_empty());
    assert!(state.take_trailing_proof().is_none());
}

#[test]
fn drain_intermediate_notification_confirms_without_replay() {
    // A non-target intermediate current is normal on nonadjacent swipes:
    // the notification wakes the settlement loop and the later target
    // read confirms. One post, no replay.
    let order = vec![1001, 1002, 1003];
    let wake = SpaceSelectWake::new();
    let latch = wake.subscribe();
    let topo = FakeTopo {
        counts: vec![Ok(1), Ok(1), Ok(1)].into(),
        rows: vec![
            Ok(async_row(&order, 1001)),
            Ok(async_row(&order, 1001)),
            Ok(async_row(&order, 1001)),
            Ok(async_row(&order, 1002)),
            Ok(async_row(&order, 1003)),
            Ok(async_row(&order, 1003)),
        ]
        .into(),
        revisions: vec![].into(),
        notify_on_read: vec![(4, latch.clone())],
        reads: 0,
    };
    let poster = FakePoster {
        reports: vec![PairsPosted {
            began_posted: 2,
            ended_posted: 2,
            pending_sign: None,
            error: None,
        }]
        .into(),
        releases: vec![].into(),
        posted: vec![],
        released: vec![],
    };
    // Rebuild with the notifying latch wired into the backend seam: the
    // topo notifies a clone of the SAME latch the backend waits on
    // (clones share the latch state; a second subscription would never
    // observe the notification).
    let cancel = Arc::new(AtomicBool::new(false));
    let legs = crate::dock_swipe_select::PinnedLegs {
        display_index: 0,
        pinned_cgid: 7,
        pinned_uuid: FIX_UUID.to_owned(),
        published_displays: vec![],
        published_separate: None,
        settle_deadline: Duration::from_millis(500),
    };
    let mut backend = OwnerSelectBackend::pinned_with(crate::dock_swipe_select::OwnerBackendArgs {
        legs,
        latch: Arc::new(latch.clone()),
        cancel,
        budget_secs: 30,
        gate_reads: Box::new(FakeGates {
            gates: vec![open_gates()].into(),
        }),
        creator: Box::new(FakeCreate {
            results: VecDeque::new(),
            log: std::rc::Rc::new(std::cell::RefCell::new(Vec::new())),
            calls: Vec::new(),
        }),
        exit_gate: Box::new(FakeExitGate::new(std::rc::Rc::new(
            std::cell::RefCell::new(Vec::new()),
        ))),
        topo,
        poster,
        op: Some("op-async".to_owned()),
        t_base: Instant::now(),
    });
    let mut state = SelectionState::default();
    let success = drain_claimed_select_with(
        &mut state,
        &mut backend,
        drain_plan(
            crate::runtime_types::InputSemanticAction::WorkspaceSelect { index: 3 },
            "op-async-2",
            "space-1003",
            1001,
        ),
        &open_gates(),
        true,
    )
    .expect("intermediate notification still confirms");
    assert_eq!(success.setter, FrameSetter::Accepted);
    assert_eq!((success.pre, success.post), (Some(1001), Some(1003)));
    assert_eq!(backend.test_poster().posted, vec![(1.0, 2)]);
}

#[test]
fn drain_partial_post_holds_unknown_without_replay() {
    // Began-without-ended partial posting is ambiguous mid-actuation
    // output: hold as Threw with the genuine pre-write read and an
    // unreadable post - never a proven refusal, never a replay.
    let order = vec![1001, 1002, 1003];
    let topo = FakeTopo {
        counts: vec![Ok(1), Ok(1), Ok(1)].into(),
        rows: vec![
            Ok(async_row(&order, 1001)),
            Ok(async_row(&order, 1001)),
            Ok(async_row(&order, 1001)),
            Err("post snapshot unreadable".to_owned()),
        ]
        .into(),
        revisions: vec![].into(),
        notify_on_read: vec![],
        reads: 0,
    };
    let poster = FakePoster {
        reports: vec![PairsPosted {
            began_posted: 2,
            ended_posted: 1,
            pending_sign: Some(1.0),
            error: Some("ended event creation returned null".to_owned()),
        }]
        .into(),
        releases: vec![Ok(())].into(),
        posted: vec![],
        released: vec![],
    };
    let (mut backend, _cancel) = async_backend(topo, poster, 100);
    let mut state = SelectionState::default();
    let failure = drain_claimed_select_with(
        &mut state,
        &mut backend,
        drain_plan(
            crate::runtime_types::InputSemanticAction::WorkspaceSelect { index: 3 },
            "op-async-3",
            "space-1003",
            1001,
        ),
        &open_gates(),
        true,
    )
    .expect_err("partial posting holds");
    assert_eq!(failure.setter, FrameSetter::Threw);
    assert_eq!(failure.pre, Some(1001));
    assert_eq!(failure.post, None, "unreadable post stays unknown");
    assert_eq!(failure.baseline, Some(1001));
    assert_eq!(
        backend.test_poster().posted.len(),
        1,
        "posted once, never replayed"
    );
    assert!(state.take_trailing_proof().is_none());
}

#[test]
fn drain_newer_switch_supersedes_without_replay() {
    // Newer input supersedes: the second drain reads the first drain's
    // arrival as its fresh pre-image and actuates exactly once more. Each
    // dispatch posts exactly one gesture pair; nothing replays.
    let topo = FakeTopo {
        counts: vec![Ok(1), Ok(1), Ok(1), Ok(1), Ok(1), Ok(1)].into(),
        rows: vec![
            Ok(async_row(&[1001, 1002], 1001)),
            Ok(async_row(&[1001, 1002], 1001)),
            Ok(async_row(&[1001, 1002], 1001)),
            Ok(async_row(&[1001, 1002], 1002)),
            Ok(async_row(&[1001, 1002], 1002)),
            Ok(async_row(&[1001, 1002], 1002)),
            Ok(async_row(&[1001, 1002], 1002)),
            Ok(async_row(&[1001, 1002], 1002)),
            Ok(async_row(&[1001, 1002], 1001)),
            Ok(async_row(&[1001, 1002], 1001)),
        ]
        .into(),
        revisions: vec![].into(),
        notify_on_read: vec![],
        reads: 0,
    };
    let poster = FakePoster {
        reports: vec![
            PairsPosted {
                began_posted: 1,
                ended_posted: 1,
                pending_sign: None,
                error: None,
            },
            PairsPosted {
                began_posted: 1,
                ended_posted: 1,
                pending_sign: None,
                error: None,
            },
        ]
        .into(),
        releases: vec![].into(),
        posted: vec![],
        released: vec![],
    };
    let (mut backend, _cancel) = async_backend(topo, poster, 100);
    let mut state = SelectionState::default();
    let first = drain_claimed_select_with(
        &mut state,
        &mut backend,
        drain_plan(
            crate::runtime_types::InputSemanticAction::WorkspaceSelect { index: 2 },
            "op-async-4",
            "space-1002",
            1001,
        ),
        &open_gates(),
        true,
    )
    .expect("first switch actuates");
    assert_eq!((first.pre, first.post), (Some(1001), Some(1002)));
    let second = drain_claimed_select_with(
        &mut state,
        &mut backend,
        drain_plan(
            crate::runtime_types::InputSemanticAction::WorkspaceSelect { index: 1 },
            "op-async-5",
            "space-1001",
            1002,
        ),
        &open_gates(),
        true,
    )
    .expect("newer switch supersedes");
    // The newer drain observed the older arrival as its pre-image: exact
    // supersession, no stale actuation, no replay.
    assert_eq!((second.pre, second.post), (Some(1002), Some(1001)));
    assert_eq!(
        backend.test_poster().posted,
        vec![(1.0, 1), (-1.0, 1)],
        "each switch posts exactly once, in order"
    );
}

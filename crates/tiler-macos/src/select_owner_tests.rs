#![forbid(unsafe_code)]

//! Offline integration tests for the serialized selection owner: injected
//! job executors and cleanup seams only — never a live Space effect. The
//! owner thread, lane serialization, history-note FIFO, cancellation
//! recovery, failure-carried proofs, and shutdown settlement all run on
//! real threads with scripted work, so no main or engine thread ever
//! blocks. Controller-level submit tests (caffeinate companion) are
//! macOS-only; thread/handoff/shutdown coverage is portable.

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::ffi::SpaceSelectWake;
use crate::runtime_types::{ClaimedSelect, InputSemanticAction};
use crate::select_owner::{
    SelectCompletion, SelectController, SelectJob, SelectSubmit, SelectThread,
};
use crate::workspace_select::{
    SelectFailure, SelectGates, SelectSuccess, SelectionBackend, SelectionEffect,
    SelectionSnapshot, SelectionState, TrailingAppendProof, TrailingLifecycle,
};

const FIX_UUID: &str = "550E8400-E29B-41D4-A716-446655440000";

fn displays() -> Vec<crate::topology::DisplayGeometry> {
    vec![crate::topology::DisplayGeometry {
        cg_id: 7,
        uuid: Some(FIX_UUID.to_owned()),
        cg_bounds: None,
        visible: None,
        scale: 2.0,
    }]
}

fn tap_claim(action: InputSemanticAction, op: &str) -> ClaimedSelect {
    let mut claim = ClaimedSelect::tap(action);
    claim.operation_id = Some(op.to_owned());
    claim
}

fn submit_legs<'a>(
    wake: &'a SpaceSelectWake,
    published: &'a [crate::topology::DisplayGeometry],
    trailing: bool,
) -> SelectSubmit<'a> {
    SelectSubmit {
        display_index: 0,
        pinned_cgid: 7,
        pinned_uuid: FIX_UUID,
        published_displays: published,
        published_separate: Some(true),
        trailing_enabled: trailing,
        wake,
    }
}

fn effect(from: u64, target: u64) -> SelectSuccess {
    SelectSuccess {
        effect: SelectionEffect {
            from,
            target,
            created: None,
            changed: from != target,
            baseline: None,
        },
        setter: tiler_core::world_report::FrameSetter::Accepted,
        pre: Some(from),
        post: Some(target),
        creation: None,
        attempt_ms: 0,
    }
}

fn refusal(reason: &str) -> SelectFailure {
    SelectFailure {
        reason: reason.to_owned(),
        setter: tiler_core::world_report::FrameSetter::Refused,
        pre: None,
        post: None,
        baseline: None,
        creation: None,
        attempt_ms: 0,
    }
}

fn proof(sid: u64) -> TrailingAppendProof {
    TrailingAppendProof {
        display_id: FIX_UUID.to_owned(),
        before: vec![1001, 1002],
        after: vec![1001, 1002, sid],
        created: sid,
    }
}

/// Scripted job outcome queue shared with an injected owner executor.
#[derive(Clone)]
struct Script {
    outcomes: Arc<std::sync::Mutex<VecDeque<ScriptedOutcome>>>,
    executed_ops: Arc<std::sync::Mutex<Vec<String>>>,
}

struct ScriptedOutcome {
    result: Result<SelectSuccess, SelectFailure>,
    proof: Option<TrailingAppendProof>,
}

impl Script {
    fn new(outcomes: Vec<ScriptedOutcome>) -> Self {
        Self {
            outcomes: Arc::new(std::sync::Mutex::new(outcomes.into())),
            executed_ops: Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }

    fn executor(&self) -> crate::select_owner::SelectExecutor {
        let outcomes = Arc::clone(&self.outcomes);
        let executed = Arc::clone(&self.executed_ops);
        Box::new(move |job: &SelectJob, _state: &mut SelectionState| {
            executed
                .lock()
                .unwrap()
                .push(job.claim.operation_id.clone().unwrap_or_default());
            let out = outcomes
                .lock()
                .unwrap()
                .pop_front()
                .expect("scripted job outcome");
            SelectCompletion {
                operation_id: job.claim.operation_id.clone(),
                result: out.result,
                proof: out.proof,
                base: job.submitted_at,
            }
        })
    }
}

fn poll_block(thread: &SelectThread) -> SelectCompletion {
    let t0 = Instant::now();
    loop {
        if let Some(done) = thread.poll_completion() {
            return done;
        }
        assert!(
            t0.elapsed() < Duration::from_secs(10),
            "owner completion never staged"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn serialized_admission_frees_only_on_poll() {
    let wake = SpaceSelectWake::new();
    let published = displays();
    let script = Script::new(vec![ScriptedOutcome {
        result: Ok(effect(1001, 1002)),
        proof: None,
    }]);
    let thread = SelectThread::spawn_with(script.executor());
    let job = |op: &str| SelectJob {
        claim: tap_claim(InputSemanticAction::WorkspaceSelect { index: 2 }, op),
        display_index: 0,
        pinned_cgid: 7,
        pinned_uuid: FIX_UUID.to_owned(),
        published_displays: published.clone(),
        published_separate: Some(true),
        trailing_enabled: true,
        fixture_gate: None,
        latch: Arc::new(wake.subscribe()),
        wake: wake.clone(),
        cancel: Arc::new(AtomicBool::new(false)),
        submitted_at: Instant::now(),
    };
    thread.submit(job("op-1")).expect("first submits");
    assert!(thread.is_busy());
    assert_eq!(
        thread.submit(job("op-2")).expect_err("overlap refuses"),
        "select-busy"
    );
    let done = poll_block(&thread);
    assert_eq!(done.operation_id.as_deref(), Some("op-1"));
    assert!(!thread.is_busy());
    // A later request after the terminal completion submits cleanly.
    thread.submit(job("op-2")).expect("lane free after poll");
    assert!(thread.is_busy());
}

#[test]
fn settle_wake_sets_once_clears_on_poll_and_covers_refusals() {
    // A staged terminal completion wakes the main pump (early cadence
    // run) instead of waiting for the next 250 ms tick: worst case
    // settle-to-reconcile drops to one 50 ms pump quantum. The wake is
    // coalesced (one bit, not a count), consumed at most once, and
    // cleared by the poll take - so steady state never quickens and a
    // promptly reconciled settle causes no spurious extra run. Ambiguous
    // (refused) terminals wake exactly like arrivals: recovery readback
    // runs on the event, never on the 1 s backstop.
    let wake = SpaceSelectWake::new();
    let published = displays();
    let script = Script::new(vec![
        ScriptedOutcome {
            result: Ok(effect(1001, 1002)),
            proof: None,
        },
        ScriptedOutcome {
            result: Err(refusal("injected ambiguous")),
            proof: None,
        },
    ]);
    let thread = SelectThread::spawn_with(script.executor());
    assert!(!thread.take_settled(), "idle owner wakes nothing");
    let job = |op: &str| SelectJob {
        claim: tap_claim(InputSemanticAction::WorkspaceSelect { index: 2 }, op),
        display_index: 0,
        pinned_cgid: 7,
        pinned_uuid: FIX_UUID.to_owned(),
        published_displays: published.clone(),
        published_separate: Some(true),
        trailing_enabled: true,
        fixture_gate: None,
        latch: Arc::new(wake.subscribe()),
        wake: wake.clone(),
        cancel: Arc::new(AtomicBool::new(false)),
        submitted_at: Instant::now(),
    };
    thread.submit(job("op-1")).expect("first submits");
    // Bounded wait for the owner to stage (5 ms sleeps like poll_block).
    let t0 = Instant::now();
    while !thread.take_settled() {
        assert!(
            t0.elapsed() < Duration::from_secs(10),
            "owner settle never woke"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(!thread.take_settled(), "wake consumed at most once");
    let done = thread.poll_completion().expect("completion staged");
    assert_eq!(done.operation_id.as_deref(), Some("op-1"));
    assert!(done.result.is_ok());
    assert!(!thread.take_settled(), "no residual wake after poll");
    // An ambiguous terminal stages and wakes the same way; this cycle
    // never touches the flag before the poll, so the false below proves
    // the poll take clears it (the staging path sets it unconditionally,
    // as the first cycle proved).
    thread.submit(job("op-2")).expect("lane free after poll");
    let done = poll_block(&thread);
    assert_eq!(done.operation_id.as_deref(), Some("op-2"));
    assert!(done.result.is_err(), "refused terminal still stages");
    assert!(
        !thread.take_settled(),
        "poll take clears even a refused settle"
    );
}

#[test]
fn reply_plan_previous_actuates_bound_target() {
    // Reply-driven previous toggle: the claim carries the core-resolved
    // target plus its live binding and baseline (no owner history). The
    // drain looks the binding up live and actuates it once.
    struct ScriptBackend {
        current: u64,
    }
    impl SelectionBackend for ScriptBackend {
        fn snapshot(&mut self) -> Result<SelectionSnapshot, String> {
            Ok(SelectionSnapshot {
                display: FIX_UUID.to_owned(),
                spaces: vec![4242, 7777],
                current: self.current,
            })
        }

        fn display_count(&mut self) -> Result<usize, String> {
            Ok(1)
        }

        fn check_output(&mut self, output: &str) -> Result<(), String> {
            if output == "macos-display-7" {
                Ok(())
            } else {
                Err(format!(
                    "product select refuses: reply target output {output} is not the pinned display macos-display-7; no actuation"
                ))
            }
        }

        fn empty(&mut self, _sid: u64) -> Result<bool, String> {
            Ok(true)
        }

        fn create(
            &mut self,
            _before: &SelectionSnapshot,
        ) -> Result<(), crate::workspace_select::DispatchError> {
            Err(crate::workspace_select::DispatchError::refused(
                "no create in this test",
            ))
        }

        fn select(
            &mut self,
            _before: &SelectionSnapshot,
            sid: u64,
        ) -> Result<(), crate::workspace_select::DispatchError> {
            self.current = sid;
            Ok(())
        }
    }
    let wake = SpaceSelectWake::new();
    let published = displays();
    let thread =
        SelectThread::spawn_with(Box::new(|job: &SelectJob, state: &mut SelectionState| {
            let mut backend = ScriptBackend { current: 7777 };
            let result = crate::workspace_select::drain_claimed_select_with(
                state,
                &mut backend,
                job.claim.clone(),
                &SelectGates {
                    session_unlocked: true,
                    secure_active: false,
                    lock_screen: false,
                },
                true,
            );
            let proof = state.take_trailing_proof();
            SelectCompletion {
                operation_id: job.claim.operation_id.clone(),
                result,
                proof,
                base: job.submitted_at,
            }
        }));
    // Core-resolved previous target rides the reply plan (binding plus
    // baseline); no history note is applied anywhere on this path.
    let mut claim = tap_claim(InputSemanticAction::WorkspacePrevious, "op-prev");
    claim.select_target_output = Some("macos-display-7".to_owned());
    claim.select_target_workspace = Some("ws-prev".to_owned());
    claim.select_native_binding = Some("space-4242".to_owned());
    claim.select_baseline_sid = Some(7777);
    let job = SelectJob {
        claim,
        display_index: 0,
        pinned_cgid: 7,
        pinned_uuid: FIX_UUID.to_owned(),
        published_displays: published,
        published_separate: Some(true),
        trailing_enabled: true,
        fixture_gate: None,
        latch: Arc::new(wake.subscribe()),
        wake: wake.clone(),
        cancel: Arc::new(AtomicBool::new(false)),
        submitted_at: Instant::now(),
    };
    thread.submit(job).expect("previous submits");
    let done = poll_block(&thread);
    let settled = done.result.expect("previous actuates the bound SID");
    assert_eq!(settled.effect.target, 4242);
    assert_eq!(settled.effect.from, 7777);
    assert_eq!(settled.effect.baseline, Some(7777));
}

#[test]
fn cancelled_pending_job_terminal_completes_and_frees_lane() {
    let wake = SpaceSelectWake::new();
    let published = displays();
    let executed = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&executed);
    let thread = SelectThread::spawn_with(Box::new(
        move |job: &SelectJob, _state: &mut SelectionState| {
            flag.store(true, Ordering::SeqCst);
            SelectCompletion {
                operation_id: job.claim.operation_id.clone(),
                result: Err(refusal("executor must not run a cancelled job")),
                proof: None,
                base: job.submitted_at,
            }
        },
    ));
    thread
        .submit(SelectJob {
            claim: tap_claim(
                InputSemanticAction::WorkspaceSelect { index: 1 },
                "op-cancel",
            ),
            display_index: 0,
            pinned_cgid: 7,
            pinned_uuid: FIX_UUID.to_owned(),
            published_displays: published.clone(),
            published_separate: Some(true),
            trailing_enabled: true,
            fixture_gate: None,
            latch: Arc::new(wake.subscribe()),
            wake: wake.clone(),
            cancel: Arc::new(AtomicBool::new(false)),
            submitted_at: Instant::now(),
        })
        .expect("submits");
    assert!(thread.cancel());
    let done = poll_block(&thread);
    assert_eq!(done.operation_id.as_deref(), Some("op-cancel"));
    assert!(
        done.result
            .expect_err("cancelled")
            .reason
            .contains("cancelled")
    );
    assert!(
        !executed.load(Ordering::SeqCst),
        "cancelled pending job never actuates"
    );
    assert!(!thread.is_busy());
    // A later request after the cancelled terminal completion submits.
    thread
        .submit(SelectJob {
            claim: tap_claim(
                InputSemanticAction::WorkspaceSelect { index: 1 },
                "op-after",
            ),
            display_index: 0,
            pinned_cgid: 7,
            pinned_uuid: FIX_UUID.to_owned(),
            published_displays: published,
            published_separate: Some(true),
            trailing_enabled: true,
            fixture_gate: None,
            latch: Arc::new(wake.subscribe()),
            wake: wake.clone(),
            cancel: Arc::new(AtomicBool::new(false)),
            submitted_at: Instant::now(),
        })
        .expect("lane free after cancel");
}

#[test]
fn cancelled_inflight_job_settles_promptly_through_latch() {
    let wake = SpaceSelectWake::new();
    let published = displays();
    let started = Arc::new(AtomicBool::new(false));
    let gate = Arc::clone(&started);
    let thread = SelectThread::spawn_with(Box::new(
        move |job: &SelectJob, _state: &mut SelectionState| {
            gate.store(true, Ordering::SeqCst);
            // Bounded scripted effect: honor the job latch promptly.
            let t0 = Instant::now();
            while !job.latch.cancelled() && !job.cancel.load(Ordering::SeqCst) {
                assert!(
                    t0.elapsed() < Duration::from_secs(10),
                    "scripted effect must observe cancel"
                );
                std::thread::sleep(Duration::from_millis(5));
            }
            SelectCompletion {
                operation_id: job.claim.operation_id.clone(),
                result: Err(refusal(
                    "dock-swipe unproven: cancelled during settlement; no replay",
                )),
                proof: None,
                base: job.submitted_at,
            }
        },
    ));
    thread
        .submit(SelectJob {
            claim: tap_claim(
                InputSemanticAction::WorkspaceSelect { index: 1 },
                "op-flight",
            ),
            display_index: 0,
            pinned_cgid: 7,
            pinned_uuid: FIX_UUID.to_owned(),
            published_displays: published,
            published_separate: Some(true),
            trailing_enabled: true,
            fixture_gate: None,
            latch: Arc::new(wake.subscribe()),
            wake: wake.clone(),
            cancel: Arc::new(AtomicBool::new(false)),
            submitted_at: Instant::now(),
        })
        .expect("submits");
    let t0 = Instant::now();
    while !started.load(Ordering::SeqCst) {
        assert!(t0.elapsed() < Duration::from_secs(10));
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(thread.cancel());
    let t0 = Instant::now();
    let done = poll_block(&thread);
    assert!(
        done.result
            .expect_err("cancelled")
            .reason
            .contains("cancelled")
    );
    // Prompt: far inside any production deadline.
    assert!(t0.elapsed() < Duration::from_secs(5));
    assert!(!thread.is_busy());
}

#[test]
fn failure_carries_proof_and_later_request_succeeds() {
    let wake = SpaceSelectWake::new();
    let published = displays();
    let script = Script::new(vec![
        ScriptedOutcome {
            result: Err(refusal(
                "dock-swipe unproven: posted 1 pairs but native current reads None",
            )),
            proof: Some(proof(555)),
        },
        ScriptedOutcome {
            result: Ok(effect(1001, 1002)),
            proof: None,
        },
    ]);
    let thread = SelectThread::spawn_with(script.executor());
    let job = |op: &str| SelectJob {
        claim: tap_claim(InputSemanticAction::WorkspaceSelect { index: 2 }, op),
        display_index: 0,
        pinned_cgid: 7,
        pinned_uuid: FIX_UUID.to_owned(),
        published_displays: published.clone(),
        published_separate: Some(true),
        trailing_enabled: true,
        fixture_gate: None,
        latch: Arc::new(wake.subscribe()),
        wake: wake.clone(),
        cancel: Arc::new(AtomicBool::new(false)),
        submitted_at: Instant::now(),
    };
    thread.submit(job("op-fail")).expect("submits");
    let failed = poll_block(&thread);
    assert!(failed.result.is_err());
    // The append proof survives the failed select for lifecycle adoption.
    assert_eq!(failed.proof.as_ref().map(|p| p.created), Some(555));
    thread.submit(job("op-ok")).expect("later request submits");
    let done = poll_block(&thread);
    assert!(done.result.is_ok());
}

#[test]
fn busy_poll_is_nonblocking_while_owner_runs() {
    // The main/engine side never blocks: a 300 ms scripted effect still
    // polls `None` instantly until staged.
    let wake = SpaceSelectWake::new();
    let published = displays();
    let thread =
        SelectThread::spawn_with(Box::new(|job: &SelectJob, _state: &mut SelectionState| {
            std::thread::sleep(Duration::from_millis(300));
            SelectCompletion {
                operation_id: job.claim.operation_id.clone(),
                result: Ok(effect(1001, 1002)),
                proof: None,
                base: job.submitted_at,
            }
        }));
    thread
        .submit(SelectJob {
            claim: tap_claim(InputSemanticAction::WorkspaceSelect { index: 2 }, "op-slow"),
            display_index: 0,
            pinned_cgid: 7,
            pinned_uuid: FIX_UUID.to_owned(),
            published_displays: published,
            published_separate: Some(true),
            trailing_enabled: true,
            fixture_gate: None,
            latch: Arc::new(wake.subscribe()),
            wake: wake.clone(),
            cancel: Arc::new(AtomicBool::new(false)),
            submitted_at: Instant::now(),
        })
        .expect("submits");
    let t0 = Instant::now();
    assert!(thread.poll_completion().is_none());
    assert!(
        t0.elapsed() < Duration::from_millis(100),
        "busy poll must not block"
    );
    let done = poll_block(&thread);
    assert!(done.result.is_ok());
}

/// Nonblocking terminal poll mirroring the tile shutdown drain (minus the
/// runloop pump, which has no headless equivalent): poll until the
/// terminal completion stages or the bound expires.
fn drain_block(thread: &SelectThread) -> SelectCompletion {
    let t0 = Instant::now();
    loop {
        if let Some(done) = thread.poll_completion() {
            return done;
        }
        assert!(
            t0.elapsed() < Duration::from_secs(10),
            "drained completion never staged"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Nonblocking reap mirroring the tile shutdown drain: spin until the
/// stop-signalled owner finishes or the bound expires.
fn reap_block(thread: &mut SelectThread) -> bool {
    let t0 = Instant::now();
    loop {
        if let Some(had) = thread.try_reap() {
            return had;
        }
        assert!(
            t0.elapsed() < Duration::from_secs(10),
            "stop-signalled owner never finished"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn shutdown_pending_job_terminal_completes_without_actuation() {
    // Shutdown races the owner take: either the pending job
    // terminal-completes cancelled without the executor running, or the
    // owner takes it first and the executor observes the shutdown cancel
    // before actuating. Both orders settle exactly one honest cancelled
    // completion with no proof and no actuation, and the stop-signalled
    // owner reaps.
    let wake = SpaceSelectWake::new();
    let published = displays();
    let actuated = Arc::new(AtomicBool::new(false));
    let act = Arc::clone(&actuated);
    let release = Arc::new(AtomicBool::new(false));
    let go = Arc::clone(&release);
    let mut thread = SelectThread::spawn_with(Box::new(
        move |job: &SelectJob, _state: &mut SelectionState| {
            // Wait for the test's shutdown (or a deadline) before even
            // checking cancellation, so both take orders are settled
            // without actuating under either.
            let t0 = Instant::now();
            while !go.load(Ordering::SeqCst)
                && !job.cancel.load(Ordering::SeqCst)
                && !job.latch.cancelled()
            {
                assert!(
                    t0.elapsed() < Duration::from_secs(10),
                    "shutdown never arrived at the taken job"
                );
                std::thread::sleep(Duration::from_millis(5));
            }
            if job.cancel.load(Ordering::SeqCst) || job.latch.cancelled() {
                return SelectCompletion {
                    operation_id: job.claim.operation_id.clone(),
                    result: Err(refusal("select refused: cancelled before dispatch")),
                    proof: None,
                    base: job.submitted_at,
                };
            }
            act.store(true, Ordering::SeqCst);
            SelectCompletion {
                operation_id: job.claim.operation_id.clone(),
                result: Err(refusal("executor actuated a shutdown job")),
                proof: None,
                base: job.submitted_at,
            }
        },
    ));
    thread
        .submit(SelectJob {
            claim: tap_claim(
                InputSemanticAction::WorkspaceSelect { index: 1 },
                "op-pending",
            ),
            display_index: 0,
            pinned_cgid: 7,
            pinned_uuid: FIX_UUID.to_owned(),
            published_displays: published.clone(),
            published_separate: Some(true),
            trailing_enabled: true,
            fixture_gate: None,
            latch: Arc::new(wake.subscribe()),
            wake: wake.clone(),
            cancel: Arc::new(AtomicBool::new(false)),
            submitted_at: Instant::now(),
        })
        .expect("submits");
    thread.request_shutdown();
    release.store(true, Ordering::SeqCst);
    let done = drain_block(&thread);
    assert_eq!(done.operation_id.as_deref(), Some("op-pending"));
    assert!(
        done.result
            .expect_err("shutdown-pending")
            .reason
            .contains("cancelled")
    );
    assert!(done.proof.is_none());
    assert!(
        !actuated.load(Ordering::SeqCst),
        "shutdown job never actuates under either take order"
    );
    reap_block(&mut thread);
    assert!(!thread.is_busy());
}

#[test]
fn shutdown_inflight_failure_proof_adopts_as_owned_residue() {
    // Live owner mid-effect at shutdown: the stop signal cancels the lane,
    // the terminal completion still carries the failure proof, and adoption
    // into the real lifecycle owns the late append — an occupied late SID
    // then plans as explicit residue (never silently lost), with no
    // off-main remover anywhere on the path.
    let wake = SpaceSelectWake::new();
    let published = displays();
    let started = Arc::new(AtomicBool::new(false));
    let gate = Arc::clone(&started);
    let mut thread = SelectThread::spawn_with(Box::new(
        move |job: &SelectJob, _state: &mut SelectionState| {
            gate.store(true, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(200));
            SelectCompletion {
                operation_id: job.claim.operation_id.clone(),
                result: Err(refusal(
                    "dock-swipe unproven: posted 1 pairs but native current reads None",
                )),
                proof: Some(proof(888)),
                base: job.submitted_at,
            }
        },
    ));
    thread
        .submit(SelectJob {
            claim: tap_claim(InputSemanticAction::WorkspaceSelect { index: 0 }, "op-res"),
            display_index: 0,
            pinned_cgid: 7,
            pinned_uuid: FIX_UUID.to_owned(),
            published_displays: published,
            published_separate: Some(true),
            trailing_enabled: true,
            fixture_gate: None,
            latch: Arc::new(wake.subscribe()),
            wake: wake.clone(),
            cancel: Arc::new(AtomicBool::new(false)),
            submitted_at: Instant::now(),
        })
        .expect("submits");
    // Shutdown only after the owner took the job inflight so the scripted
    // failure (with proof) is what settles — not a pending cancel.
    let t0 = Instant::now();
    while !started.load(Ordering::SeqCst) {
        assert!(t0.elapsed() < Duration::from_secs(10));
        std::thread::sleep(Duration::from_millis(5));
    }
    thread.request_shutdown();
    let done = drain_block(&thread);
    assert!(done.result.is_err());
    let proof = done.proof.as_ref().expect("failure carries the late proof");
    assert_eq!(proof.created, 888);
    let mut lifecycle = TrailingLifecycle::new(&[1001, 1002]);
    lifecycle
        .adopt_created(proof.created)
        .expect("adopt late append");
    assert_eq!(lifecycle.owned(), &[888]);
    let (removable, residue) = lifecycle.cleanup_plan(&[1001, 1002, 888], 1001, &|_| Ok(false));
    assert!(removable.is_empty());
    assert!(
        residue
            .iter()
            .any(|(sid, reason)| *sid == 888 && reason.contains("occupied")),
        "{residue:?}"
    );
    reap_block(&mut thread);
}

#[test]
fn shutdown_after_create_proof_terminal_orders_cleanup_newest_first() {
    // Success completion carrying the after-create proof: adoption then
    // planning orders the empty late append removable ahead of older owned
    // residue — the exact order the main-thread cleanup consumes.
    let wake = SpaceSelectWake::new();
    let published = displays();
    let started = Arc::new(AtomicBool::new(false));
    let gate = Arc::clone(&started);
    let mut thread = SelectThread::spawn_with(Box::new(
        move |job: &SelectJob, _state: &mut SelectionState| {
            gate.store(true, Ordering::SeqCst);
            // The shutdown cancel lands mid-flight; the scripted success
            // ignores it and settles terminally with its proof either way.
            std::thread::sleep(Duration::from_millis(200));
            SelectCompletion {
                operation_id: job.claim.operation_id.clone(),
                result: Ok(effect(1001, 2003)),
                proof: Some(proof(2003)),
                base: job.submitted_at,
            }
        },
    ));
    thread
        .submit(SelectJob {
            claim: tap_claim(InputSemanticAction::WorkspaceSelect { index: 0 }, "op-ok"),
            display_index: 0,
            pinned_cgid: 7,
            pinned_uuid: FIX_UUID.to_owned(),
            published_displays: published,
            published_separate: Some(true),
            trailing_enabled: true,
            fixture_gate: None,
            latch: Arc::new(wake.subscribe()),
            wake: wake.clone(),
            cancel: Arc::new(AtomicBool::new(false)),
            submitted_at: Instant::now(),
        })
        .expect("submits");
    // Shutdown only after the owner took the job inflight so the scripted
    // success (with proof) is what settles — not a pending cancel.
    let t0 = Instant::now();
    while !started.load(Ordering::SeqCst) {
        assert!(t0.elapsed() < Duration::from_secs(10));
        std::thread::sleep(Duration::from_millis(5));
    }
    thread.request_shutdown();
    let done = drain_block(&thread);
    assert!(done.result.is_ok());
    let mut lifecycle = TrailingLifecycle::new(&[1001, 1002]);
    lifecycle.adopt_created(2001).expect("adopt older append");
    let proof = done.proof.as_ref().expect("success carries the proof");
    lifecycle
        .adopt_created(proof.created)
        .expect("adopt late append");
    assert_eq!(proof.created, 2003);
    let (removable, residue) =
        lifecycle.cleanup_plan(&[1001, 1002, 2001, 2003], 1001, &|sid| Ok(sid == 2003));
    assert_eq!(removable, vec![2003]);
    assert!(
        residue
            .iter()
            .any(|(sid, reason)| *sid == 2001 && reason.contains("occupied")),
        "{residue:?}"
    );
    lifecycle
        .release_removed(2003)
        .expect("release verified removal");
    assert_eq!(lifecycle.owned(), &[2001]);
    reap_block(&mut thread);
}

#[test]
fn controller_shutdown_request_reaps_idle() {
    // Idle shutdown (never selected) reaps trivially without spawning work.
    let mut idle = SelectController::new();
    assert!(!idle.is_busy());
    idle.request_shutdown();
    assert!(idle.is_finished());
    assert_eq!(idle.try_reap(), Some(false));
}

/// Controller submit path (caffeinate companion plus production owner)
/// is macOS-only. The previous toggle with empty owner history refuses
/// before any topology post leg, so this performs zero Space effects while
/// proving the real submit/poll/refusal/resubmit lane end to end.
#[cfg(target_os = "macos")]
#[test]
fn controller_submit_poll_refusal_then_resubmit() {
    let wake = SpaceSelectWake::new();
    let published = displays();
    let mut controller = SelectController::new();
    assert!(!controller.is_busy());
    // Warm the one-time Secure-Input framework init outside the measured
    // window: a cold `IsSecureEventInputEnabled` resolution initializes
    // SkyLight/CFBundle directory state once per process (~10+s on some
    // hosts). The product amortizes it over its lifetime, so the lane
    // bound below measures the lane, not framework init.
    let _ = crate::ffi::secure_input_known();
    controller
        .submit(
            &tap_claim(InputSemanticAction::WorkspacePrevious, "op-prev"),
            submit_legs(&wake, &published, true),
        )
        .expect("first submits");
    assert!(controller.is_busy());
    // Generous bound: the lane itself settles in milliseconds, but a loaded
    // host (full parallel suite) slows even warmed framework reads; the
    // bound still detects a wedged owner.
    let t0 = Instant::now();
    let (completion, _reaped) = loop {
        if let Some(done) = controller.poll() {
            break done;
        }
        assert!(
            t0.elapsed() < Duration::from_secs(60),
            "controller completion never staged"
        );
        std::thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(completion.operation_id.as_deref(), Some("op-prev"));
    assert!(
        completion.result.is_err(),
        "empty previous history refuses without actuation"
    );
    assert!(completion.proof.is_none());
    assert!(!controller.is_busy());
    controller
        .submit(
            &tap_claim(InputSemanticAction::WorkspacePrevious, "op-prev2"),
            submit_legs(&wake, &published, true),
        )
        .expect("lane free after refusal poll");
}

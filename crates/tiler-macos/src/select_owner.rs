#![forbid(unsafe_code)]

//! Serialized Dock-swipe selection owner (single-flight, off-main).
//!
//! One dedicated owner thread holds the run-lifetime [`SelectionState`]
//! (previous history plus trailing proofs) and executes one [`SelectJob`]
//! at a time through [`execute_select_job`]. The main thread submits
//! nonblocking via [`SelectController`] and polls completions nonblocking;
//! it never waits and never touches history, so no concurrent send/select
//! effect can race it. Cross-lane exclusion with the managed-send owner is
//! explicit at the submit site (see the tile controller): at most one lane
//! is ever inflight.
//!
//! Cancellation retains the lane until a terminal completion: a cancelled
//! pending job stays queued for the owner to terminal-complete (never
//! actuated), and a cancelled inflight job is pulsed through its settle
//! latch for a prompt terminal completion. The lane frees only on poll.
//!
//! Shutdown never blocks the main thread and never detaches: the main
//! thread stops admitting new selects, cancels the inflight job, and keeps
//! pumping its runloop while polling nonblocking until the terminal
//! completion stages (adopting its trailing proof exactly like the live
//! poll). Only then does it stop the owner, reap it once finished, and run
//! the preexisting main-thread trailing cleanup — Mission Control removal
//! keeps its main-thread semantics (never an off-main remover). A drain
//! that outlasts its bound reports unclean with the retained ownership,
//! never clean.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::{Duration, Instant};

use crate::dock_swipe_select::{
    GateReads, LiveGateReads, LiveOwnerSelectBackend, PinnedLegs, SELECT_ARRIVAL_DEADLINE_MS,
};
use crate::ffi::{SpaceSelectLatch, SpaceSelectWake};
use crate::runtime_types::ClaimedSelect;
use crate::workspace_select::{SelectionEffect, SelectionState, TrailingAppendProof};

/// One selection job for the owner: the claimed action plus the pinned
/// plain-data display legs (never an AppKit read on the owner), the
/// capability flag, the per-job settle latch (created at submit from the
/// guard wake — retained for inflight cancellation), and submit-time
/// bookkeeping for chained timing logs.
#[derive(Debug, Clone)]
pub struct SelectJob {
    pub claim: ClaimedSelect,
    pub display_index: usize,
    pub pinned_cgid: u32,
    pub pinned_uuid: String,
    pub published_displays: Vec<crate::topology::DisplayGeometry>,
    pub published_separate: Option<bool>,
    pub trailing_enabled: bool,
    pub latch: Arc<SpaceSelectLatch>,
    pub wake: SpaceSelectWake,
    pub cancel: Arc<AtomicBool>,
    pub submitted_at: Instant,
}

/// One owner completion: the job identity plus the proven outcome plus the
/// single-take trailing proof (present on BOTH success and failure when
/// the job appended — the main side adopts it exactly once) plus the
/// submit instant for completion-observed timing.
#[derive(Debug, Clone)]
pub struct SelectCompletion {
    pub operation_id: Option<String>,
    pub result: Result<SelectionEffect, String>,
    pub proof: Option<TrailingAppendProof>,
    pub base: Instant,
}

/// Main-thread-published legs for one Dock-swipe select submit: the
/// plain-data display identity (never an AppKit read here), the capability
/// flag, and the guard's shared active-Space wake (the submit subscribes
/// the per-job latch from it). Bundled so the submit stays within the
/// argument-count lint.
pub struct SelectSubmit<'a> {
    pub display_index: usize,
    pub pinned_cgid: u32,
    pub pinned_uuid: &'a str,
    pub published_displays: &'a [crate::topology::DisplayGeometry],
    pub published_separate: Option<bool>,
    pub trailing_enabled: bool,
    pub wake: &'a SpaceSelectWake,
}

struct SelectLane {
    pending: Option<SelectJob>,
    inflight: Option<SelectJob>,
    completion: Option<SelectCompletion>,
}

impl SelectLane {
    fn is_busy(&self) -> bool {
        self.pending.is_some() || self.inflight.is_some() || self.completion.is_some()
    }
}

pub(crate) struct SelectHandoff {
    lane: std::sync::Mutex<SelectLane>,
    /// Follow-history attributions from settled send follows, applied by
    /// the owner to its `SelectionState` before the next select job (FIFO,
    /// serialized with selects — never a cross-thread history race).
    notes: std::sync::Mutex<Vec<(String, u64)>>,
}

impl SelectHandoff {
    fn new() -> Self {
        Self {
            lane: std::sync::Mutex::new(SelectLane {
                pending: None,
                inflight: None,
                completion: None,
            }),
            notes: std::sync::Mutex::new(Vec::new()),
        }
    }

    fn submit(&self, job: SelectJob) -> Result<(), &'static str> {
        let mut lane = self.lane.lock().unwrap_or_else(|e| e.into_inner());
        if lane.is_busy() {
            return Err("select-busy");
        }
        lane.pending = Some(job);
        Ok(())
    }

    fn take_for_owner(&self) -> Option<SelectJob> {
        let mut lane = self.lane.lock().unwrap_or_else(|e| e.into_inner());
        if lane.inflight.is_some() || lane.pending.is_none() {
            return None;
        }
        let job = lane.pending.take()?;
        lane.inflight = Some(job.clone());
        Some(job)
    }

    fn finish(&self, completion: SelectCompletion) -> Result<(), &'static str> {
        let mut lane = self.lane.lock().unwrap_or_else(|e| e.into_inner());
        if lane.inflight.is_none() {
            return Err("select-lane-idle");
        }
        lane.inflight = None;
        lane.completion = Some(completion);
        Ok(())
    }

    fn poll_completion(&self) -> Option<SelectCompletion> {
        let mut lane = self.lane.lock().unwrap_or_else(|e| e.into_inner());
        if lane.inflight.is_some() || lane.pending.is_some() {
            return None;
        }
        lane.completion.take()
    }

    /// Flag every queued and inflight job cancelled and pulse their settle
    /// latches, then wake the owner. The lane stays busy until each job's
    /// terminal completion: a cancelled pending job is still taken by the
    /// owner and completed without actuation, and an inflight job settles
    /// promptly through its latch. Returns whether anything was queued or
    /// inflight.
    fn cancel(&self) -> bool {
        let lane = self.lane.lock().unwrap_or_else(|e| e.into_inner());
        let had = lane.pending.is_some() || lane.inflight.is_some();
        if let Some(job) = lane.pending.as_ref() {
            job.cancel.store(true, Ordering::SeqCst);
            job.latch.cancel();
        }
        if let Some(job) = lane.inflight.as_ref() {
            job.cancel.store(true, Ordering::SeqCst);
            job.latch.cancel();
        }
        had
    }

    fn note_arrival(&self, display: &str, from: u64) {
        self.notes
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push((display.to_owned(), from));
    }

    fn take_notes(&self) -> Vec<(String, u64)> {
        std::mem::take(&mut *self.notes.lock().unwrap_or_else(|e| e.into_inner()))
    }

    fn is_busy(&self) -> bool {
        self.lane
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_busy()
    }

    fn release(&self) -> bool {
        let mut lane = self.lane.lock().unwrap_or_else(|e| e.into_inner());
        let had = lane.is_busy();
        lane.pending = None;
        lane.inflight = None;
        lane.completion = None;
        had
    }
}

/// Execute one selection job on the calling (owner) thread with
/// owner-fresh gates: the armed and single-display rules, the bound-claim
/// fence, then one [`SelectionState::execute`] (single dispatch,
/// event-driven settlement, never a replay; history advances only on
/// verified arrival). The trailing proof is taken on BOTH outcomes so the
/// main side adopts an append even when its post-create select failed.
pub(crate) fn execute_select_job(job: &SelectJob, state: &mut SelectionState) -> SelectCompletion {
    let mut gate_reads = LiveGateReads;
    let fresh = gate_reads.read_gates();
    let legs = PinnedLegs {
        display_index: job.display_index,
        pinned_cgid: job.pinned_cgid,
        pinned_uuid: job.pinned_uuid.clone(),
        published_displays: job.published_displays.clone(),
        published_separate: job.published_separate,
        settle_deadline: Duration::from_millis(SELECT_ARRIVAL_DEADLINE_MS),
    };
    let mut backend = LiveOwnerSelectBackend::pinned(
        legs,
        Arc::clone(&job.latch),
        Arc::clone(&job.cancel),
        30,
        Box::new(gate_reads),
        job.claim.operation_id.clone(),
        job.submitted_at,
    );
    let result = crate::workspace_select::drain_claimed_select_with(
        state,
        &mut backend,
        job.claim.clone(),
        &fresh,
        job.trailing_enabled,
    );
    let proof = state.take_trailing_proof();
    SelectCompletion {
        operation_id: job.claim.operation_id.clone(),
        result,
        proof,
        base: job.submitted_at,
    }
}

/// Terminal cancelled completion for a job the owner never actuated
/// (cancelled while pending). Carries the job identity with no proof —
/// nothing was created, so there is nothing to adopt.
fn cancelled_completion(job: &SelectJob) -> SelectCompletion {
    SelectCompletion {
        operation_id: job.claim.operation_id.clone(),
        result: Err("select refused: cancelled before dispatch".to_owned()),
        proof: None,
        base: job.submitted_at,
    }
}

/// Dedicated serialized selection owner. Every fresh read, post, and
/// settlement runs on this thread; it blocks on the wake channel while
/// idle (no sleep polling).
pub struct SelectThread {
    handoff: Arc<SelectHandoff>,
    stop: Arc<AtomicBool>,
    wake_tx: mpsc::Sender<()>,
    handle: Option<std::thread::JoinHandle<()>>,
}

/// One job execution on the owner thread.
pub type SelectExecutor =
    Box<dyn Fn(&SelectJob, &mut SelectionState) -> SelectCompletion + Send + 'static>;

impl SelectThread {
    /// Spawn the dedicated selection owner with an injected job executor
    /// (production runs [`execute_select_job`]; tests script every leg, so
    /// no main/engine thread ever blocks on real effects).
    pub fn spawn_with(executor: SelectExecutor) -> Self {
        let handoff = Arc::new(SelectHandoff::new());
        let stop = Arc::new(AtomicBool::new(false));
        let (wake_tx, wake_rx) = mpsc::channel::<()>();
        let thread_handoff = Arc::clone(&handoff);
        let thread_stop = Arc::clone(&stop);
        let handle = std::thread::Builder::new()
            .name("dock-swipe-select".to_owned())
            .spawn(move || {
                let mut state = SelectionState::default();
                loop {
                    // Serialized follow-history attribution: settled send
                    // follows land here before the next select job runs, so
                    // `workspace-previous` flips back to the followed-from
                    // Space with no cross-thread history race.
                    for (display, from) in thread_handoff.take_notes() {
                        state.note_arrival(&display, from);
                    }
                    if let Some(job) = thread_handoff.take_for_owner() {
                        // Cancellation prevents new create/select work: a
                        // flagged job terminal-completes without actuation.
                        if job.cancel.load(Ordering::SeqCst) || job.latch.cancelled() {
                            let _ = thread_handoff.finish(cancelled_completion(&job));
                            continue;
                        }
                        let completion = executor(&job, &mut state);
                        let _ = thread_handoff.finish(completion);
                        continue;
                    }
                    if thread_stop.load(Ordering::SeqCst) {
                        break;
                    }
                    match wake_rx.recv() {
                        Ok(()) => continue,
                        Err(_) => break,
                    }
                }
            })
            .expect("dock-swipe select thread spawns");
        Self {
            handoff,
            stop,
            wake_tx,
            handle: Some(handle),
        }
    }

    /// Spawn the dedicated selection owner running the production executor.
    #[must_use]
    pub fn spawn() -> Self {
        Self::spawn_with(Box::new(execute_select_job))
    }

    /// Nonblocking submit through the serialized lane: refuses busy while a
    /// job is pending, inflight, or completed-but-unpolled. Wakes the owner
    /// exactly once on success.
    pub fn submit(&self, job: SelectJob) -> Result<(), &'static str> {
        let result = self.handoff.submit(job);
        if result.is_ok() {
            let _ = self.wake_tx.send(());
        }
        result
    }

    /// Nonblocking poll for the staged completion (`None` while the owner
    /// runs or when idle).
    pub fn poll_completion(&self) -> Option<SelectCompletion> {
        self.handoff.poll_completion()
    }

    /// Nonblocking request-to-cancel plus an owner wake so an idle owner
    /// re-checks immediately. Flags and latch-pulses every queued and
    /// inflight job; the lane stays busy until each terminal completion.
    pub fn cancel(&self) -> bool {
        let had = self.handoff.cancel();
        let _ = self.wake_tx.send(());
        had
    }

    /// Record a verified send-follow arrival for the owner-held previous
    /// history (applied follows only — stays never note). Serialized
    /// through the lane ahead of later selects.
    pub fn note_arrival(&self, display: &str, from: u64) {
        self.handoff.note_arrival(display, from);
        let _ = self.wake_tx.send(());
    }

    /// Whether a job is pending, inflight, or completed-but-unpolled.
    #[must_use]
    pub fn is_busy(&self) -> bool {
        self.handoff.is_busy()
    }

    /// Whether the owner thread already finished (join would not block).
    #[must_use]
    pub fn is_finished(&self) -> bool {
        self.handle
            .as_ref()
            .is_some_and(|handle| handle.is_finished())
    }

    /// Nonblocking shutdown request: signal stop, cancel the lane, wake.
    /// Never joins — pair with cooperative drain (poll until the terminal
    /// completion stages) then [`SelectThread::try_reap`].
    pub fn request_shutdown(&self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = self.handoff.cancel();
        let _ = self.wake_tx.send(());
    }

    /// Nonblocking reap: when the owner already finished, join it (which
    /// no longer blocks) and only then release the lane.
    pub fn try_reap(&mut self) -> Option<bool> {
        if !self.is_finished() {
            return None;
        }
        let handle = self.handle.take()?;
        let _ = handle.join();
        Some(self.handoff.release())
    }
}

impl Drop for SelectThread {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = self.handoff.cancel();
        let _ = self.wake_tx.send(());
        if self
            .handle
            .as_ref()
            .is_some_and(|handle| handle.is_finished())
            && let Some(handle) = self.handle.take()
        {
            let _ = handle.join();
            self.handoff.release();
        }
    }
}

/// Terminal select shutdown state is observed cooperatively: the main
/// thread polls [`SelectController::poll`] until the terminal completion
/// stages, then [`SelectController::request_shutdown`] plus
/// [`SelectController::try_reap`] once finished. No detached settler, no
/// off-main remover — the preexisting main-thread trailing cleanup runs
/// after the reap.
//
// Main-side async select controller: the nonblocking submit/poll
// counterpart to the send [`crate::workspace_send::LiveSendController`].
// Owns the selection thread (spawned lazily on first submit, so runs that
// never select never spawn it) plus the inflight job identity plus the
// bounded caffeinate companion (held from submit to polled completion).
#[derive(Default)]
pub struct SelectController {
    thread: Option<SelectThread>,
    inflight: Option<SelectInflight>,
    awake: Option<crate::mc_lifecycle::live::CaffeinateGuard>,
    /// Follow-history attributions recorded before the owner exists (runs
    /// that send before their first select): flushed into the lane on the
    /// next submit, FIFO before the job, so no attribution is lost.
    pending_notes: Vec<(String, u64)>,
}

#[derive(Debug, Clone)]
struct SelectInflight {
    operation_id: Option<String>,
}

impl SelectController {
    /// Empty controller (no thread, no inflight).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether a select is inflight (submitted, completion unpolled).
    #[must_use]
    pub fn is_busy(&self) -> bool {
        self.inflight.is_some()
    }

    /// Nonblocking submit of one claimed select: spawns the owner on first
    /// use, subscribes the per-job settle latch from the guard wake,
    /// spawns the bounded caffeinate companion, and submits through the
    /// serialized lane. Refuses busy while a select is inflight.
    pub fn submit(&mut self, claim: &ClaimedSelect, legs: SelectSubmit<'_>) -> Result<(), String> {
        if self.inflight.is_some() {
            return Err(
                "select-busy: one Dock-swipe select already inflight; refusing overlap".to_owned(),
            );
        }
        if self.thread.is_none() {
            self.thread = Some(SelectThread::spawn());
        }
        let held = crate::mc_lifecycle::live::CaffeinateGuard::spawn(60)
            .map_err(|e| format!("product select refuses: caffeinate unspawnable ({e})"))?;
        let job = SelectJob {
            claim: claim.clone(),
            display_index: legs.display_index,
            pinned_cgid: legs.pinned_cgid,
            pinned_uuid: legs.pinned_uuid.to_owned(),
            published_displays: legs.published_displays.to_vec(),
            published_separate: legs.published_separate,
            trailing_enabled: legs.trailing_enabled,
            latch: Arc::new(legs.wake.subscribe()),
            wake: legs.wake.clone(),
            cancel: Arc::new(AtomicBool::new(false)),
            submitted_at: Instant::now(),
        };
        let Some(thread) = self.thread.as_ref() else {
            return Err("select-busy: no select owner; refusing".to_owned());
        };
        // Flush pre-owner follow-history attributions FIFO before the job.
        for (display, from) in std::mem::take(&mut self.pending_notes) {
            thread.note_arrival(&display, from);
        }
        thread.submit(job).map_err(|e| e.to_owned())?;
        self.inflight = Some(SelectInflight {
            operation_id: claim.operation_id.clone(),
        });
        self.awake = Some(held);
        Ok(())
    }

    /// Nonblocking poll for the inflight completion. Stops the caffeinate
    /// companion and reports whether it reaped cleanly alongside the
    /// completion (arrival truth dominates: a verified effect still
    /// returns `Ok` when only the reaper failed). Logs the
    /// completion-observed latency against the submit instant.
    pub fn poll(&mut self) -> Option<(SelectCompletion, bool)> {
        let completion = self.thread.as_ref()?.poll_completion()?;
        let inflight = self.inflight.take()?;
        debug_assert_eq!(completion.operation_id, inflight.operation_id);
        let reaped = self.awake.as_mut().map(|a| a.stop()).unwrap_or(true);
        let _ = self.awake.take();
        eprintln!(
            "tiler-macos dev select action={} observed route=dock-swipe complete={} observed_ms={}",
            completion
                .operation_id
                .as_deref()
                .unwrap_or("unknown-select"),
            completion.result.is_ok(),
            completion
                .base
                .elapsed()
                .as_millis()
                .min(u128::from(u64::MAX)) as u64,
        );
        Some((completion, reaped))
    }

    /// Cancel the inflight select (lock/secure/stale paths): flags and
    /// latch-pulses the job and wakes the owner; the lane stays busy until
    /// the terminal completion, which the next [`SelectController::poll`]
    /// settles honestly. Stops no caffeinate here (it stops at poll).
    pub fn cancel(&mut self) -> bool {
        self.thread.as_ref().is_some_and(|t| t.cancel())
    }

    /// Record a verified send-follow arrival for the owner-held previous
    /// history (applied follows only — stays never note). Before the owner
    /// exists the attribution parks here and flushes FIFO on the next
    /// submit; afterwards it rides the lane ahead of later selects.
    pub fn note_arrival(&mut self, display: &str, from: u64) {
        if let Some(thread) = self.thread.as_ref() {
            thread.note_arrival(display, from);
        } else {
            self.pending_notes.push((display.to_owned(), from));
        }
    }

    /// Nonblocking terminal shutdown request: stop the caffeinate
    /// companion first (so no companion outlives the handoff), then signal
    /// owner stop, cancel the lane, and wake the owner. Never blocks, never
    /// joins — the caller drains cooperatively (poll until the terminal
    /// completion stages, adopting its proof exactly like the live poll),
    /// then pumps until [`SelectController::is_finished`] and
    /// [`SelectController::try_reap`]. Never panics.
    pub fn request_shutdown(&mut self) {
        if let Some(mut awake) = self.awake.take() {
            awake.stop();
        }
        if let Some(thread) = self.thread.as_ref() {
            thread.request_shutdown();
        }
    }

    /// Whether the owner thread already finished (reap would not block).
    /// Idle (never spawned) counts as finished: there is nothing to join.
    #[must_use]
    pub fn is_finished(&self) -> bool {
        self.thread
            .as_ref()
            .is_none_or(|thread| thread.is_finished())
    }

    /// Nonblocking reap after [`SelectController::request_shutdown`]: idle
    /// (never spawned) reaps trivially as `Some(false)`; a finished owner
    /// joins and releases the lane exactly once; a live owner reports
    /// `None` so the caller keeps pumping or reports retained.
    pub fn try_reap(&mut self) -> Option<bool> {
        if self.thread.is_none() {
            return Some(false);
        }
        self.thread.as_mut()?.try_reap()
    }
}

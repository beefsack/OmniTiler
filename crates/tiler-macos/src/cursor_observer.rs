#![forbid(unsafe_code)]
//! Bounded pointer/hover observer for the world route: the ONLY thread that
//! performs AX hit tests on a cadence.
//!
//! Invariants:
//! - Sample-time attribution: the hit owner is read here and resolved by the
//!   engine against live retained truth at job time. Nothing re-reads AX at
//!   report-build time, so a resting pointer keeps its hover memory.
//! - Paired emission: the builder carries `cursor`/`cursor_hit` together,
//!   and only when the sampled hit names a window row emitted in that same
//!   report. An unpaired or unresolved hit is withheld, never guessed.
//! - No focus claim: hover is observation. The report's `focused_global`,
//!   `foreground_known`, and `focus_changes` never derive from this sample.
//! - Bounded retry: exactly one sample is in flight at a time. A refused or
//!   unacknowledged post is reposted verbatim on the next tick (same
//!   sequence, point, and owner), and no new sample is taken until the
//!   engine acks that sequence. While sampling the live thread parks 250 ms
//!   between ticks and never blocks beyond the bounded AX call; while
//!   focus-follows is off or a focused fullscreen game is paused it reads
//!   only the two gate atomics and idles without touching any host state.
//!
//! The decision state machine is separated from the live thread so unit
//! tests drive it with fake position/hit/post closures (no live AX, no
//! threads).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// Observer tick: one position read per tick. A sample is posted only on
/// the first tick, on movement, on the hit refresh, or as a pending retry.
const TICK: Duration = Duration::from_millis(250);

/// Idle park while `focusFollowsCursor` is off. With sampling disabled the
/// loop has no host work to do, so it idles well below the tick cadence
/// instead of waking 4x/s; the engine's republished setting is picked up on
/// the next idle wake.
const IDLE_PARK: Duration = Duration::from_secs(1);

/// Hit-refresh interval while the pointer rests: the hit is re-read at
/// least this often so a window moving (or appearing) under a still
/// pointer is noticed, and the core's hover memory stays freshly stamped.
const HIT_REFRESH_MS: u64 = 1000;

/// Core carried-point bound (`valid_carried_point`): samples outside this
/// inclusive range are skipped rather than emitted unvalidated.
const CURSOR_LIMIT: f64 = 32768.0;

/// Adapter-shared core cursor state: the effective core
/// `focusFollowsCursor` setting plus the focused-fullscreen game pause,
/// published by the engine thread whenever it finishes a job (and once
/// after the initial settings exchange) and read lock-free by the observer.
/// Two atomics; never a timer, never an extra thread.
#[derive(Debug, Default)]
pub(crate) struct CoreCursorGate {
    focus_follows_cursor: AtomicBool,
    game_paused: AtomicBool,
}

impl CoreCursorGate {
    /// Publish the latest core values (engine thread only).
    pub(crate) fn publish(&self, focus_follows_cursor: bool, game_paused: bool) {
        self.focus_follows_cursor
            .store(focus_follows_cursor, Ordering::Relaxed);
        self.game_paused.store(game_paused, Ordering::Relaxed);
    }

    /// Read `(focus_follows_cursor, game_paused)` (observer thread).
    pub(crate) fn read(&self) -> (bool, bool) {
        (
            self.focus_follows_cursor.load(Ordering::Relaxed),
            self.game_paused.load(Ordering::Relaxed),
        )
    }
}

/// The sampling gate for one observer tick: the effective core
/// `focusFollowsCursor` setting plus every host condition that suspends
/// live AX hit-testing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SamplingGate {
    /// Effective core setting (default off).
    pub(crate) focus_follows_cursor: bool,
    /// `ffi::session_state().unlocked()`: locked and unknown both suspend.
    pub(crate) session_unlocked: bool,
    /// `ffi::secure_input_known() == Some(true)` (unknown does not suspend).
    pub(crate) secure_input: bool,
    /// Core-reported focused fullscreen game pause.
    pub(crate) game_paused: bool,
}

impl SamplingGate {
    /// True when the host suspends sampling (locked/unknown session, Secure
    /// Input active, or a focused fullscreen game).
    pub(crate) fn suspended(self) -> bool {
        !self.session_unlocked || self.secure_input || self.game_paused
    }

    /// True only when sampling may run: focus-follows is on and nothing
    /// suspends it. While false the observer takes no hit test and posts no
    /// sample (including a pending retry).
    pub(crate) fn allowed(self) -> bool {
        self.focus_follows_cursor && !self.suspended()
    }
}

/// One sampled pointer observation: the point in carried integer space
/// plus the AX hit owner read at the same instant (`None` = honest miss).
/// `seq` is this observer's monotonic delivery identity, not a report
/// sequence; retries repeat it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CursorObservation {
    pub seq: u64,
    pub point: (i32, i32),
    /// Raw AX hit owner `(pid, wid)` at sample time; the engine attributes
    /// it against live retained truth, never this thread.
    pub owner: Option<(u32, u32)>,
}

/// One live sample plus its delivery state.
#[derive(Debug)]
struct LiveSample {
    observation: CursorObservation,
    /// When the hit owner was read (ms, process monotonic epoch).
    hit_ms: u64,
    /// True once the engine acknowledged this exact sequence.
    acked: bool,
}

/// Pure hover-observer state machine.
///
/// A tick reads the pointer position, decides whether a new sample is due,
/// and either reposts the one pending sample or posts a fresh one. Delivery
/// is proven only by the engine's per-sequence ack, so a refused post and an
/// unacknowledged post are the same state: pending, retried next tick.
#[derive(Debug, Default)]
pub(crate) struct CursorObserver {
    next_seq: u64,
    live: Option<LiveSample>,
}

impl CursorObserver {
    /// Advance one bounded tick.
    ///
    /// - `now_ms`: process monotonic clock (ms).
    /// - `gate`: whether sampling is allowed right now (core setting plus
    ///   host suspend conditions). A disallowed tick takes no hit test and
    ///   posts nothing; the pending sample (if any) stays pending.
    /// - `position`: the current pointer point, `None` when unreadable.
    /// - `hit`: AX hit test for one point, called only when a new sample is
    ///   due (an `Err` honest miss becomes `None`).
    /// - `acked_seq`: the engine-acknowledged sequence (0 when none).
    /// - `post`: submit one sample to the engine; rejection is expected and
    ///   simply leaves the sample pending for the next tick.
    pub(crate) fn tick<Hit, Post>(
        &mut self,
        now_ms: u64,
        gate: SamplingGate,
        position: Option<(f64, f64)>,
        hit: Hit,
        acked_seq: u64,
        post: Post,
    ) where
        Hit: FnOnce((f64, f64)) -> Option<(u32, u32)>,
        Post: FnOnce(CursorObservation),
    {
        // Resolve the in-flight sample first: the ack is per sequence, so a
        // later ack (or a duplicate delivery of this one) clears it too.
        if let Some(live) = self.live.as_mut()
            && !live.acked
            && acked_seq >= live.observation.seq
        {
            live.acked = true;
        }
        // Sampling disallowed (focus-follows off, locked/unknown session,
        // Secure Input, or a focused fullscreen game): no AX hit test and no
        // report, including a pending retry. The pending sample survives so
        // the next allowed tick resumes it.
        if !gate.allowed() {
            return;
        }
        // Bounded retry: one pending sample is reposted verbatim and no new
        // sample is taken until it is acknowledged.
        if let Some(live) = self.live.as_ref()
            && !live.acked
        {
            post(live.observation);
            return;
        }
        let Some((x, y)) = position else { return };
        let Some(point) = carried_point((x, y)) else {
            return;
        };
        // Resting pointer: nothing to say until the hit refresh elapses, so
        // a still pointer costs one report per refresh, not one per tick.
        if let Some(live) = self.live.as_ref()
            && live.observation.point == point
            && now_ms.saturating_sub(live.hit_ms) < HIT_REFRESH_MS
        {
            return;
        }
        // Movement or refresh: re-read the hit and post a new sample. The
        // pointer may have moved onto chrome/foreign/unreadable, which the
        // hit closure reports as `None` so the hit is honestly cleared.
        let owner = hit((x, y));
        self.next_seq += 1;
        let observation = CursorObservation {
            seq: self.next_seq,
            point,
            owner,
        };
        post(observation);
        self.live = Some(LiveSample {
            observation,
            hit_ms: now_ms,
            acked: false,
        });
    }
}

/// Convert one CG point into the report's carried integer space, refusing
/// non-finite and out-of-bound samples (the core's `valid_carried_point`
/// bound is inclusive, so a rounded edge value stays valid).
pub(crate) fn carried_point((x, y): (f64, f64)) -> Option<(i32, i32)> {
    if !x.is_finite() || !y.is_finite() || x.abs() > CURSOR_LIMIT || y.abs() > CURSOR_LIMIT {
        return None;
    }
    Some((x.round() as i32, y.round() as i32))
}

/// Live observer controller: the stop flag plus the running thread. Stop
/// (or drop) signals, unparks, and joins, so teardown never waits a full
/// tick and no sample races engine shutdown.
pub(crate) struct ObserverHandle {
    stop: Arc<AtomicBool>,
    join: Option<std::thread::JoinHandle<()>>,
}

impl ObserverHandle {
    /// Stop and join the observer. Idempotent; bounded by the current tick.
    pub(crate) fn stop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(join) = self.join.as_ref() {
            join.thread().unpark();
        }
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

impl Drop for ObserverHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Spawn the live hover observer (thread name `phase1-cursor`).
///
/// This is the only thread permitted to call the AX hit test on a cadence:
/// the main thread stays AppKit-only and the engine thread performs no AX.
/// A spawn denial is returned to the caller, which leaves hover unsampled
/// (tiling and focus are unaffected; the cursor pair simply never rides).
pub(crate) fn spawn(handle: crate::runtime::RuntimeHandle) -> std::io::Result<ObserverHandle> {
    let stop = Arc::new(AtomicBool::new(false));
    let thread_stop = Arc::clone(&stop);
    let join = std::thread::Builder::new()
        .name("phase1-cursor".to_owned())
        .spawn(move || observe(handle, thread_stop))?;
    Ok(ObserverHandle {
        stop,
        join: Some(join),
    })
}

/// One live-loop cycle: read the two gate atomics, then either idle, stop
/// at the host gate, or run one bounded observer tick. Returns the park
/// duration for the next wake.
///
/// The order is the contract: a disabled setting or a paused game returns
/// before ANY closure runs, so no host state is touched (no session/Secure
/// Input read, no pointer read, no ack mutex, no AX hit test, no post).
/// Focus-follows off idles at [`IDLE_PARK`] because nothing can change until
/// the engine republishes the setting; a paused game keeps the short tick so
/// resume stays responsive. When only the host gate suspends (locked/unknown
/// session or Secure Input) the session probes run but position/ack/hit/post
/// do not, so a pending sample is retained and retries verbatim on the first
/// allowed cycle.
#[allow(clippy::too_many_arguments)]
fn cycle<S, E, P, Hit, A, Post>(
    observer: &mut CursorObserver,
    now_ms: u64,
    focus_follows_cursor: bool,
    game_paused: bool,
    session_unlocked: S,
    secure_input: E,
    position: P,
    hit: Hit,
    acked_seq: A,
    post: Post,
) -> Duration
where
    S: FnOnce() -> bool,
    E: FnOnce() -> bool,
    P: FnOnce() -> Option<(f64, f64)>,
    Hit: FnOnce((f64, f64)) -> Option<(u32, u32)>,
    A: FnOnce() -> u64,
    Post: FnOnce(CursorObservation),
{
    if !focus_follows_cursor {
        return IDLE_PARK;
    }
    if game_paused {
        return TICK;
    }
    let gate = SamplingGate {
        focus_follows_cursor,
        session_unlocked: session_unlocked(),
        secure_input: secure_input(),
        game_paused,
    };
    // Host suspension: the pending sample (if any) stays pending and none
    // of the position/hit/ack/post closures below is reached.
    if gate.allowed() {
        observer.tick(now_ms, gate, position(), hit, acked_seq(), post);
    }
    TICK
}

/// Live loop: one cycle per wake, then park for the duration it returns.
/// Every AX read is bounded; the only other work is the non-blocking inbox
/// submit, and a disabled or suspended cycle does neither.
fn observe(handle: crate::runtime::RuntimeHandle, stop: Arc<AtomicBool>) {
    let mut observer = CursorObserver::default();
    while !stop.load(Ordering::SeqCst) {
        let (focus_follows_cursor, game_paused) = handle.cursor_gate.read();
        let park = cycle(
            &mut observer,
            crate::input_eligibility::now_ms(),
            focus_follows_cursor,
            game_paused,
            || crate::ffi::session_state().unlocked(),
            || crate::ffi::secure_input_known() == Some(true),
            crate::synth::cursor_position,
            |(x, y)| crate::ffi::ax_hit_test_owner(x, y).ok(),
            || handle.cursor_sample_ack(),
            |sample| {
                // Refusal leaves the sample pending: delivery is proven only
                // by the engine ack, never by the send itself.
                let _ = handle.submit_cursor_sample(sample.seq, sample.point, sample.owner);
            },
        );
        std::thread::park_timeout(park);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};

    /// Fake observer surface: fixed position/hit, recorded posts, and a
    /// selectable refusal (post refusal is indistinguishable from an
    /// unacknowledged post by design, so the tests assert the retry shape).
    struct Fake {
        position: Cell<Option<(f64, f64)>>,
        owner: Cell<Option<(u32, u32)>>,
        hit_reads: Cell<u32>,
        posts: RefCell<Vec<CursorObservation>>,
        accept: Cell<bool>,
        gate: Cell<SamplingGate>,
    }

    impl Fake {
        fn new(position: Option<(f64, f64)>, owner: Option<(u32, u32)>) -> Self {
            Self {
                position: Cell::new(position),
                owner: Cell::new(owner),
                hit_reads: Cell::new(0),
                posts: RefCell::new(Vec::new()),
                accept: Cell::new(true),
                gate: Cell::new(SamplingGate {
                    focus_follows_cursor: true,
                    session_unlocked: true,
                    secure_input: false,
                    game_paused: false,
                }),
            }
        }

        fn tick(&self, observer: &mut CursorObserver, now_ms: u64, acked: u64) {
            observer.tick(
                now_ms,
                self.gate.get(),
                self.position.get(),
                |_point| {
                    self.hit_reads.set(self.hit_reads.get() + 1);
                    self.owner.get()
                },
                acked,
                |sample| {
                    if self.accept.get() {
                        self.posts.borrow_mut().push(sample);
                    }
                },
            );
        }

        fn posts(&self) -> Vec<CursorObservation> {
            self.posts.borrow().clone()
        }
    }

    #[test]
    fn first_tick_samples_once_and_stays_quiet_while_resting() {
        let fake = Fake::new(Some((10.4, 20.6)), Some((7, 9)));
        let mut observer = CursorObserver::default();
        fake.tick(&mut observer, 1_000, 0);
        assert_eq!(
            fake.posts(),
            vec![CursorObservation {
                seq: 1,
                point: (10, 21),
                owner: Some((7, 9)),
            }]
        );
        // Acked, pointer unchanged, refresh not yet elapsed: nothing posts
        // and the hit is not re-read.
        fake.tick(&mut observer, 1_500, 1);
        fake.tick(&mut observer, 1_999, 1);
        assert_eq!(fake.posts().len(), 1);
        assert_eq!(fake.hit_reads.get(), 1);
    }

    #[test]
    fn unacked_sample_is_reposted_verbatim_without_taking_a_new_one() {
        let fake = Fake::new(Some((10.0, 20.0)), Some((7, 9)));
        let mut observer = CursorObserver::default();
        fake.tick(&mut observer, 1_000, 0);
        // Pointer moves while the first sample is unacknowledged, and the
        // hit owner changed: the retry still repeats the SAME sample.
        fake.position.set(Some((300.0, 400.0)));
        fake.owner.set(Some((8, 10)));
        fake.tick(&mut observer, 1_250, 0);
        fake.tick(&mut observer, 1_500, 0);
        assert_eq!(
            fake.posts(),
            vec![
                CursorObservation {
                    seq: 1,
                    point: (10, 20),
                    owner: Some((7, 9)),
                };
                3
            ]
        );
        assert_eq!(fake.hit_reads.get(), 1, "retries never re-read the hit");
        // The ack ends the retry; the next tick observes the movement.
        fake.tick(&mut observer, 1_750, 1);
        let posts = fake.posts();
        assert_eq!(posts.len(), 4);
        assert_eq!(posts[3].seq, 2);
        assert_eq!(posts[3].point, (300, 400));
        assert_eq!(posts[3].owner, Some((8, 10)));
    }

    #[test]
    fn refused_post_stays_pending_until_the_ack() {
        let fake = Fake::new(Some((10.0, 20.0)), Some((7, 9)));
        fake.accept.set(false);
        let mut observer = CursorObserver::default();
        fake.tick(&mut observer, 1_000, 0);
        assert!(fake.posts().is_empty(), "refused post delivers nothing");
        // The next accepted post is the same sample (same seq/point/owner).
        fake.accept.set(true);
        fake.position.set(Some((50.0, 60.0)));
        fake.tick(&mut observer, 1_250, 0);
        assert_eq!(
            fake.posts(),
            vec![CursorObservation {
                seq: 1,
                point: (10, 20),
                owner: Some((7, 9)),
            }]
        );
    }

    #[test]
    fn movement_or_one_second_refresh_re_reads_the_hit() {
        let fake = Fake::new(Some((10.0, 20.0)), Some((7, 9)));
        let mut observer = CursorObserver::default();
        fake.tick(&mut observer, 1_000, 0);
        // Refresh: same point, one second later, the hit is read again and
        // a new sequence posts (the core treats an identical pair as no
        // change).
        fake.tick(&mut observer, 2_000, 1);
        let posts = fake.posts();
        assert_eq!(posts.len(), 2);
        assert_eq!(posts[1].seq, 2);
        assert_eq!(posts[1].point, posts[0].point);
        assert_eq!(fake.hit_reads.get(), 2);
        // Movement within the refresh window posts immediately.
        fake.position.set(Some((11.0, 20.0)));
        fake.tick(&mut observer, 2_100, 2);
        let posts = fake.posts();
        assert_eq!(posts.len(), 3);
        assert_eq!(posts[2].seq, 3);
        assert_eq!(posts[2].point, (11, 20));
        assert_eq!(fake.hit_reads.get(), 3);
    }

    #[test]
    fn honest_miss_clears_the_hit_and_unreadable_position_is_skipped() {
        let fake = Fake::new(Some((10.0, 20.0)), Some((7, 9)));
        let mut observer = CursorObserver::default();
        fake.tick(&mut observer, 1_000, 0);
        // Moving onto chrome: the hit closure reports None (honest miss).
        fake.owner.set(None);
        fake.position.set(Some((20.0, 30.0)));
        fake.tick(&mut observer, 1_100, 1);
        assert_eq!(fake.posts().last().unwrap().owner, None);
        // Unreadable (or non-finite / out-of-bound) position posts nothing.
        let before = fake.posts().len();
        fake.position.set(None);
        fake.tick(&mut observer, 2_500, 2);
        fake.position.set(Some((f64::NAN, 0.0)));
        fake.tick(&mut observer, 2_750, 2);
        fake.position.set(Some((40_000.0, 0.0)));
        fake.tick(&mut observer, 3_000, 2);
        assert_eq!(fake.posts().len(), before);
    }

    #[test]
    fn carried_point_enforces_the_core_bound() {
        assert_eq!(carried_point((0.0, 0.0)), Some((0, 0)));
        assert_eq!(carried_point((-32768.0, 32768.0)), Some((-32768, 32768)));
        assert_eq!(carried_point((10.5, -10.5)), Some((11, -11)));
        assert_eq!(carried_point((f64::INFINITY, 0.0)), None);
        assert_eq!(carried_point((f64::NAN, 0.0)), None);
        assert_eq!(carried_point((32768.5, 0.0)), None);
        assert_eq!(carried_point((0.0, -32768.5)), None);
    }

    fn gate() -> SamplingGate {
        SamplingGate {
            focus_follows_cursor: true,
            session_unlocked: true,
            secure_input: false,
            game_paused: false,
        }
    }

    #[test]
    fn sampling_gate_is_allowed_only_when_enabled_and_clear() {
        assert!(gate().allowed());
        assert!(!gate().suspended());
        let disabled = SamplingGate {
            focus_follows_cursor: false,
            ..gate()
        };
        assert!(!disabled.allowed(), "default-off focus-follows disables");
        assert!(!disabled.suspended(), "off is not a host suspension");
        for suspended in [
            SamplingGate {
                session_unlocked: false,
                ..gate()
            },
            SamplingGate {
                secure_input: true,
                ..gate()
            },
            SamplingGate {
                game_paused: true,
                ..gate()
            },
        ] {
            assert!(suspended.suspended());
            assert!(!suspended.allowed());
        }
        // Unknown Secure Input is not active and never suspends.
        assert!(
            !SamplingGate {
                secure_input: false,
                ..gate()
            }
            .suspended()
        );
    }

    #[test]
    fn core_cursor_gate_reads_the_last_published_values() {
        let gate = CoreCursorGate::default();
        assert_eq!(gate.read(), (false, false), "default off, no pause");
        gate.publish(true, true);
        assert_eq!(gate.read(), (true, true));
        gate.publish(false, false);
        assert_eq!(gate.read(), (false, false));
    }

    /// Every closure the live loop can reach, counted independently, so a
    /// test can prove a short-circuited cycle touched no host state at all.
    struct Probes {
        session_unlocked: Cell<bool>,
        secure_input: Cell<bool>,
        position_value: Cell<Option<(f64, f64)>>,
        owner: Cell<Option<(u32, u32)>>,
        acked: Cell<u64>,
        accept: Cell<bool>,
        session_reads: Cell<u32>,
        secure_reads: Cell<u32>,
        position_reads: Cell<u32>,
        ack_reads: Cell<u32>,
        hit_reads: Cell<u32>,
        post_calls: Cell<u32>,
        posts: RefCell<Vec<CursorObservation>>,
    }

    impl Probes {
        fn new() -> Self {
            Self {
                session_unlocked: Cell::new(true),
                secure_input: Cell::new(false),
                position_value: Cell::new(Some((10.0, 20.0))),
                owner: Cell::new(Some((7, 9))),
                acked: Cell::new(0),
                accept: Cell::new(true),
                session_reads: Cell::new(0),
                secure_reads: Cell::new(0),
                position_reads: Cell::new(0),
                ack_reads: Cell::new(0),
                hit_reads: Cell::new(0),
                post_calls: Cell::new(0),
                posts: RefCell::new(Vec::new()),
            }
        }

        fn run(
            &self,
            observer: &mut CursorObserver,
            now_ms: u64,
            focus_follows_cursor: bool,
            game_paused: bool,
        ) -> Duration {
            cycle(
                observer,
                now_ms,
                focus_follows_cursor,
                game_paused,
                || {
                    self.session_reads.set(self.session_reads.get() + 1);
                    self.session_unlocked.get()
                },
                || {
                    self.secure_reads.set(self.secure_reads.get() + 1);
                    self.secure_input.get()
                },
                || {
                    self.position_reads.set(self.position_reads.get() + 1);
                    self.position_value.get()
                },
                |_point| {
                    self.hit_reads.set(self.hit_reads.get() + 1);
                    self.owner.get()
                },
                || {
                    self.ack_reads.set(self.ack_reads.get() + 1);
                    self.acked.get()
                },
                |sample| {
                    self.post_calls.set(self.post_calls.get() + 1);
                    if self.accept.get() {
                        self.posts.borrow_mut().push(sample);
                    }
                },
            )
        }

        fn reset(&self) {
            for counter in [
                &self.session_reads,
                &self.secure_reads,
                &self.position_reads,
                &self.ack_reads,
                &self.hit_reads,
                &self.post_calls,
            ] {
                counter.set(0);
            }
        }

        /// `[session, secure, position, ack, hit, post]`.
        fn counts(&self) -> [u32; 6] {
            [
                self.session_reads.get(),
                self.secure_reads.get(),
                self.position_reads.get(),
                self.ack_reads.get(),
                self.hit_reads.get(),
                self.post_calls.get(),
            ]
        }

        fn posts(&self) -> Vec<CursorObservation> {
            self.posts.borrow().clone()
        }
    }

    #[test]
    fn disabled_focus_follows_idles_without_probing_any_host_state() {
        let probes = Probes::new();
        let mut observer = CursorObserver::default();
        assert_eq!(probes.run(&mut observer, 1_000, false, false), IDLE_PARK);
        assert_eq!(probes.counts(), [0; 6], "no probe while disabled");
        // The pause cannot lift a disabled setting; it is still an idle park.
        assert_eq!(probes.run(&mut observer, 2_000, false, true), IDLE_PARK);
        assert_eq!(probes.counts(), [0; 6]);
        assert!(IDLE_PARK > TICK, "disabled wakes less often than the tick");
    }

    #[test]
    fn paused_game_keeps_the_short_tick_and_probes_nothing() {
        let probes = Probes::new();
        let mut observer = CursorObserver::default();
        assert_eq!(probes.run(&mut observer, 1_000, true, true), TICK);
        assert_eq!(
            probes.counts(),
            [0; 6],
            "no session/secure/position/ack/hit/post read while paused"
        );
    }

    #[test]
    fn host_suspension_reads_the_gate_probes_only_and_retains_the_pending_sample() {
        for (session_unlocked, secure_input) in [(false, false), (true, true)] {
            let probes = Probes::new();
            probes.accept.set(false);
            let mut observer = CursorObserver::default();
            // Refused post: seq 1 stays pending with its original hit.
            assert_eq!(probes.run(&mut observer, 1_000, true, false), TICK);
            assert!(probes.posts().is_empty(), "refused post leaves it pending");

            probes.reset();
            probes.session_unlocked.set(session_unlocked);
            probes.secure_input.set(secure_input);
            probes.accept.set(true);
            probes.position_value.set(Some((300.0, 400.0)));
            probes.owner.set(Some((8, 10)));
            assert_eq!(probes.run(&mut observer, 1_250, true, false), TICK);
            assert_eq!(
                probes.counts(),
                [1, 1, 0, 0, 0, 0],
                "session/secure consulted; position/ack/hit/post not"
            );
            assert!(probes.posts().is_empty(), "suspension posts nothing");

            probes.reset();
            probes.session_unlocked.set(true);
            probes.secure_input.set(false);
            assert_eq!(probes.run(&mut observer, 1_500, true, false), TICK);
            assert_eq!(
                probes.posts(),
                vec![CursorObservation {
                    seq: 1,
                    point: (10, 20),
                    owner: Some((7, 9)),
                }],
                "the retained sample retries verbatim on the first allowed cycle"
            );
            assert_eq!(
                probes.hit_reads.get(),
                0,
                "suspension never re-reads the hit"
            );
        }
    }

    #[test]
    fn focus_follows_off_takes_no_hit_and_posts_nothing() {
        let fake = Fake::new(Some((10.0, 20.0)), Some((7, 9)));
        fake.gate.set(SamplingGate {
            focus_follows_cursor: false,
            ..gate()
        });
        let mut observer = CursorObserver::default();
        // Every tick path: first sample, movement, and the 1 s refresh.
        fake.tick(&mut observer, 1_000, 0);
        fake.position.set(Some((50.0, 60.0)));
        fake.tick(&mut observer, 1_250, 0);
        fake.tick(&mut observer, 3_000, 0);
        assert_eq!(fake.hit_reads.get(), 0, "no AX hit test while off");
        assert!(fake.posts().is_empty(), "no OP_CURSOR report while off");
    }

    #[test]
    fn suspension_blocks_hit_tests_and_reports_until_clear() {
        let fake = Fake::new(Some((10.0, 20.0)), Some((7, 9)));
        let mut observer = CursorObserver::default();
        for suspended in [
            SamplingGate {
                session_unlocked: false,
                ..gate()
            },
            SamplingGate {
                secure_input: true,
                ..gate()
            },
            SamplingGate {
                game_paused: true,
                ..gate()
            },
        ] {
            fake.gate.set(suspended);
            fake.tick(&mut observer, 1_000, 0);
            assert_eq!(fake.hit_reads.get(), 0, "no hit while suspended");
            assert!(fake.posts().is_empty(), "no post while suspended");
        }
        // Clear: the observer resumes on the next tick.
        fake.gate.set(gate());
        fake.tick(&mut observer, 1_100, 0);
        assert_eq!(fake.hit_reads.get(), 1);
        assert_eq!(fake.posts().len(), 1);
        assert_eq!(fake.posts()[0].seq, 1);
    }

    #[test]
    fn pending_sample_survives_suspension_and_retries_when_clear() {
        let fake = Fake::new(Some((10.0, 20.0)), Some((7, 9)));
        fake.accept.set(false);
        let mut observer = CursorObserver::default();
        fake.tick(&mut observer, 1_000, 0);
        assert!(fake.posts().is_empty());
        // Pointer moves while suspended; the pending sample is not re-read
        // or posted, and the movement is not taken as a new sample.
        fake.gate.set(SamplingGate {
            session_unlocked: false,
            ..gate()
        });
        fake.position.set(Some((300.0, 400.0)));
        fake.accept.set(true);
        fake.tick(&mut observer, 1_250, 0);
        assert_eq!(fake.hit_reads.get(), 1);
        assert!(fake.posts().is_empty());
        // Clear: the SAME pending sample (seq 1, old point) retries verbatim.
        fake.gate.set(gate());
        fake.tick(&mut observer, 1_500, 0);
        assert_eq!(
            fake.posts(),
            vec![CursorObservation {
                seq: 1,
                point: (10, 20),
                owner: Some((7, 9)),
            }]
        );
        assert_eq!(fake.hit_reads.get(), 1, "retry never re-reads the hit");
    }
}

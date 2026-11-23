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
//!   engine acks that sequence. The live thread parks 250 ms between ticks
//!   and never blocks beyond the bounded AX call.
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

/// Hit-refresh interval while the pointer rests: the hit is re-read at
/// least this often so a window moving (or appearing) under a still
/// pointer is noticed, and the core's hover memory stays freshly stamped.
const HIT_REFRESH_MS: u64 = 1000;

/// Core carried-point bound (`valid_carried_point`): samples outside this
/// inclusive range are skipped rather than emitted unvalidated.
const CURSOR_LIMIT: f64 = 32768.0;

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
    /// - `position`: the current pointer point, `None` when unreadable.
    /// - `hit`: AX hit test for one point, called only when a new sample is
    ///   due (an `Err` honest miss becomes `None`).
    /// - `acked_seq`: the engine-acknowledged sequence (0 when none).
    /// - `post`: submit one sample to the engine; rejection is expected and
    ///   simply leaves the sample pending for the next tick.
    pub(crate) fn tick<Hit, Post>(
        &mut self,
        now_ms: u64,
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

/// Live loop: tick, then park one tick. Every AX read is bounded; the only
/// other work is the non-blocking inbox submit.
fn observe(handle: crate::runtime::RuntimeHandle, stop: Arc<AtomicBool>) {
    let mut observer = CursorObserver::default();
    while !stop.load(Ordering::SeqCst) {
        observer.tick(
            crate::input_eligibility::now_ms(),
            crate::synth::cursor_position(),
            |(x, y)| crate::ffi::ax_hit_test_owner(x, y).ok(),
            handle.cursor_sample_ack(),
            |sample| {
                // Refusal leaves the sample pending: delivery is proven only
                // by the engine ack, never by the send itself.
                let _ = handle.submit_cursor_sample(sample.seq, sample.point, sample.owner);
            },
        );
        std::thread::park_timeout(TICK);
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
    }

    impl Fake {
        fn new(position: Option<(f64, f64)>, owner: Option<(u32, u32)>) -> Self {
            Self {
                position: Cell::new(position),
                owner: Cell::new(owner),
                hit_reads: Cell::new(0),
                posts: RefCell::new(Vec::new()),
                accept: Cell::new(true),
            }
        }

        fn tick(&self, observer: &mut CursorObserver, now_ms: u64, acked: u64) {
            observer.tick(
                now_ms,
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
}

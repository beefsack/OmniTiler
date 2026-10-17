//! Event-driven visibility batch: post all, then confirm together.
//!
//! One overall deadline per phase covers post work plus confirmation. The
//! deadline starts before the native posts: a host that never confirms
//! must not stall the owner indefinitely. Success requires an observed
//! snapshot (`is_visible`), never a WinEvent alone. The waiter carries
//! only genuine host wakes with the remaining budget; there is no
//! periodic polling fallback and no new unbounded queue.

/// One overall deadline (ms) per hide/reveal phase, including post work.
/// The budget starts before native posts: a host that never confirms
/// must not stall the owner indefinitely.
pub(crate) const BATCH_DEADLINE_MS: u64 = 2000;

/// Per-window batch outcome. Observed success only; an event without a
/// matching snapshot never confirms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BatchOutcome {
    /// Snapshot observed the expected visibility within the deadline.
    Confirmed,
    /// The native post (`ShowWindowAsync`) reported failure. A failed
    /// post is final for that window even if it is later destroyed: the
    /// mapping is intentional, so a post failure never becomes a silent
    /// retire and never poisons confirmed peers.
    PostFailed,
    /// Deadline hit without observation; caller keeps existing
    /// uncertain/partial handling.
    Uncertain,
    /// Window destroyed mid-phase.
    Absent,
}

/// Owner wait signal for one batch wait call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BatchSignal {
    /// A relevant SHOW/HIDE notification was dispatched; caller rechecks
    /// snapshots. State alone decides: a signal without a matching
    /// snapshot never confirms.
    Notified,
    /// An unrelated message or spurious wake; caller waits again on the
    /// same fixed deadline without rechecking or resetting it.
    Unrelated,
    /// The genuine deadline expired.
    Timeout,
}

/// Injected native visibility boundary (fake in tests, `ShowWindowAsync`
/// plus `IsWindowVisible`/`IsWindow` in production).
pub(crate) trait BatchNative {
    /// Post one visibility effect without blocking. `true` mirrors a
    /// nonzero `ShowWindowAsync`; `false` is a per-window post failure
    /// that never poisons confirmed peers.
    fn post(&mut self, hwnd: u64, hide: bool) -> bool;
    /// Fresh visibility readback. `None` means verifiably destroyed
    /// (`IsWindow` false); production never inspects fabricated HWNDs.
    fn is_visible(&mut self, hwnd: u64) -> Option<bool>;
}

/// Injected monotonic clock plus event-driven owner wait. `wait` pumps
/// owner messages (delivering SHOW/HIDE callbacks) and blocks only for
/// the remaining genuine budget; it never slices a fixed poll.
pub(crate) trait BatchWaitClock {
    /// Current monotonic time (ms).
    fn now_ms(&mut self) -> u64;
    /// Pump and wait up to `remaining_ms` for a visibility wake.
    fn wait(&mut self, remaining_ms: u64) -> BatchSignal;
}

/// Post every `hwnds` entry, then confirm together inside one overall
/// deadline. `hide` selects the expected end state (`true` = hidden).
/// Posts run strictly before the first confirmation snapshot; the
/// initial snapshot covers events that arrived before the first wait,
/// rechecks run only after [`BatchSignal::Notified`], and one final
/// snapshot runs at the deadline. Unrelated wakes neither confirm nor
/// extend the deadline. Returns input order.
pub(crate) fn run_visibility_batch<D: BatchNative + BatchWaitClock>(
    hwnds: &[u64],
    hide: bool,
    deps: &mut D,
) -> Vec<(u64, BatchOutcome)> {
    let expect_visible = !hide;
    let start = deps.now_ms();
    let deadline = start.saturating_add(BATCH_DEADLINE_MS);

    let mut posted: Vec<(u64, bool)> = Vec::with_capacity(hwnds.len());
    for hwnd in hwnds {
        posted.push((*hwnd, deps.post(*hwnd, hide)));
    }

    // None = still pending confirmation. A failed post maps to
    // PostFailed up front and stays final even if the window is later
    // destroyed; only posted windows enter snapshot confirmation.
    let mut outcomes: Vec<Option<BatchOutcome>> = posted
        .iter()
        .map(|(_, ok)| {
            if *ok {
                None
            } else {
                Some(BatchOutcome::PostFailed)
            }
        })
        .collect();

    let snapshot = |deps: &mut D, outcomes: &mut [Option<BatchOutcome>]| {
        for (slot, (hwnd, _)) in outcomes.iter_mut().zip(posted.iter()) {
            if slot.is_some() {
                continue;
            }
            match deps.is_visible(*hwnd) {
                None => *slot = Some(BatchOutcome::Absent),
                Some(visible) if visible == expect_visible => {
                    *slot = Some(BatchOutcome::Confirmed);
                }
                Some(_) => {}
            }
        }
    };

    snapshot(deps, &mut outcomes);

    loop {
        if outcomes.iter().all(Option::is_some) {
            break;
        }
        let now = deps.now_ms();
        if now >= deadline {
            break;
        }
        let remaining = deadline - now;
        match deps.wait(remaining) {
            BatchSignal::Notified => snapshot(deps, &mut outcomes),
            BatchSignal::Unrelated => {}
            BatchSignal::Timeout => break,
        }
    }

    // Final deadline snapshot for anything still pending.
    snapshot(deps, &mut outcomes);
    for slot in outcomes.iter_mut() {
        if slot.is_none() {
            *slot = Some(BatchOutcome::Uncertain);
        }
    }

    posted
        .iter()
        .zip(outcomes)
        .map(|((hwnd, _), outcome)| (*hwnd, outcome.unwrap_or(BatchOutcome::Uncertain)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// Offline native-boundary fake: scripted `ShowWindowAsync` posts,
    /// visibility/`IsWindow` reads, wait events, and a fake monotonic
    /// clock. Drives the real batch state machine; no canned high-level
    /// outcomes, no real OS window inspection, no live effects.
    struct Fixture {
        visible: HashMap<u64, Option<bool>>,
        post_ok: HashMap<u64, bool>,
        post_log: Vec<u64>,
        post_args: Vec<(u64, bool)>,
        post_cost_ms: u64,
        snapshot_log: Vec<u64>,
        op_log: Vec<(char, u64)>,
        wait_calls: Vec<u64>,
        now: u64,
        schedule: Vec<(u64, u64, Option<bool>)>,
        spurious_notified: u32,
        unrelated: u32,
    }

    impl Fixture {
        fn new(hwnds: &[u64], visible: bool) -> Self {
            Self {
                visible: hwnds.iter().map(|h| (*h, Some(visible))).collect(),
                post_ok: HashMap::new(),
                post_log: Vec::new(),
                post_args: Vec::new(),
                post_cost_ms: 0,
                snapshot_log: Vec::new(),
                op_log: Vec::new(),
                wait_calls: Vec::new(),
                now: 1_000,
                schedule: Vec::new(),
                spurious_notified: 0,
                unrelated: 0,
            }
        }

        fn at(&mut self, delta_ms: u64, hwnd: u64, visible: Option<bool>) {
            self.schedule.push((self.now + delta_ms, hwnd, visible));
            self.schedule.sort_by_key(|row| row.0);
        }

        fn outcome_map(outcomes: &[(u64, BatchOutcome)]) -> HashMap<u64, BatchOutcome> {
            outcomes.iter().map(|(h, o)| (*h, *o)).collect()
        }
    }

    impl BatchNative for Fixture {
        fn post(&mut self, hwnd: u64, hide: bool) -> bool {
            self.post_log.push(hwnd);
            self.post_args.push((hwnd, hide));
            self.op_log.push(('p', hwnd));
            self.now += self.post_cost_ms;
            *self.post_ok.get(&hwnd).unwrap_or(&true)
        }

        fn is_visible(&mut self, hwnd: u64) -> Option<bool> {
            self.snapshot_log.push(hwnd);
            self.op_log.push(('s', hwnd));
            self.visible.get(&hwnd).copied().flatten()
        }
    }

    impl BatchWaitClock for Fixture {
        fn now_ms(&mut self) -> u64 {
            self.now
        }

        fn wait(&mut self, remaining_ms: u64) -> BatchSignal {
            self.wait_calls.push(remaining_ms);
            if self.unrelated > 0 {
                self.unrelated -= 1;
                self.now += 5;
                return BatchSignal::Unrelated;
            }
            if self.spurious_notified > 0 {
                self.spurious_notified -= 1;
                self.now += 5;
                return BatchSignal::Notified;
            }
            let deadline = self.now.saturating_add(remaining_ms);
            if let Some(pos) = self.schedule.iter().position(|row| row.0 <= deadline) {
                let (fire, hwnd, state) = self.schedule.remove(pos);
                self.now = fire.max(self.now);
                self.visible.insert(hwnd, state);
                return BatchSignal::Notified;
            }
            self.now = deadline;
            BatchSignal::Timeout
        }
    }

    #[test]
    fn posts_all_before_first_confirmation_snapshot() {
        let hwnds = [0xA1, 0xA2, 0xA3];
        let mut fx = Fixture::new(&hwnds, true);
        let out = run_visibility_batch(&hwnds, true, &mut fx);
        assert!(out.iter().all(|(_, o)| *o == BatchOutcome::Uncertain));
        assert_eq!(fx.post_log, hwnds);
        let last_post = fx
            .op_log
            .iter()
            .rposition(|(k, _)| *k == 'p')
            .expect("posts run");
        let first_snapshot = fx
            .op_log
            .iter()
            .position(|(k, _)| *k == 's')
            .expect("snapshots run");
        assert!(last_post < first_snapshot, "posts precede confirmations");
    }

    #[test]
    fn independent_windows_confirm_on_own_scripted_changes() {
        let hwnds = [0xB1, 0xB2];
        let mut fx = Fixture::new(&hwnds, true);
        fx.at(100, 0xB1, Some(false));
        fx.at(800, 0xB2, Some(false));
        let out = run_visibility_batch(&hwnds, true, &mut fx);
        let map = Fixture::outcome_map(&out);
        assert_eq!(map[&0xB1], BatchOutcome::Confirmed);
        assert_eq!(map[&0xB2], BatchOutcome::Confirmed);
        assert_eq!(out[0].0, 0xB1);
        assert_eq!(out[1].0, 0xB2);
        assert!(fx.now <= 1_000 + BATCH_DEADLINE_MS);
    }

    #[test]
    fn already_observed_before_wait_needs_no_wait() {
        let hwnds = [0xC1, 0xC2];
        let mut fx = Fixture::new(&hwnds, false);
        let out = run_visibility_batch(&hwnds, true, &mut fx);
        let map = Fixture::outcome_map(&out);
        assert_eq!(map[&0xC1], BatchOutcome::Confirmed);
        assert_eq!(map[&0xC2], BatchOutcome::Confirmed);
        assert!(fx.wait_calls.is_empty(), "no lost wake: no wait needed");
    }

    #[test]
    fn missing_confirmations_share_one_deadline() {
        let hwnds = [0xD1, 0xD2, 0xD3];
        let mut fx = Fixture::new(&hwnds, true);
        let start = fx.now;
        let out = run_visibility_batch(&hwnds, true, &mut fx);
        assert!(out.iter().all(|(_, o)| *o == BatchOutcome::Uncertain));
        assert_eq!(
            fx.now,
            start + BATCH_DEADLINE_MS,
            "one shared deadline, not N"
        );
        let total: u64 = fx.wait_calls.iter().sum();
        assert!(total <= BATCH_DEADLINE_MS, "waits stay within one budget");
    }

    #[test]
    fn failed_post_does_not_poison_confirmed_peers() {
        let hwnds = [0xE1, 0xE2];
        let mut fx = Fixture::new(&hwnds, true);
        fx.post_ok.insert(0xE2, false);
        fx.at(100, 0xE1, Some(false));
        let out = run_visibility_batch(&hwnds, true, &mut fx);
        let map = Fixture::outcome_map(&out);
        assert_eq!(map[&0xE1], BatchOutcome::Confirmed);
        assert_eq!(map[&0xE2], BatchOutcome::PostFailed);
    }

    #[test]
    fn event_without_state_never_confirms() {
        let hwnds = [0xF1];
        let mut fx = Fixture::new(&hwnds, true);
        fx.spurious_notified = 1;
        fx.at(300, 0xF1, Some(false));
        let out = run_visibility_batch(&hwnds, true, &mut fx);
        assert_eq!(out[0].1, BatchOutcome::Confirmed);
        assert!(fx.snapshot_log.len() >= 3, "spurious wake rechecked state");
        assert_eq!(fx.now, 1_000 + 300);
    }

    #[test]
    fn unrelated_events_neither_confirm_nor_reset_deadline() {
        let hwnds = [0x11, 0x12];
        let mut fx = Fixture::new(&hwnds, true);
        fx.unrelated = 3;
        fx.at(500, 0x11, Some(false));
        fx.at(600, 0x12, Some(false));
        let start = fx.now;
        let out = run_visibility_batch(&hwnds, true, &mut fx);
        let map = Fixture::outcome_map(&out);
        assert_eq!(map[&0x11], BatchOutcome::Confirmed);
        assert_eq!(map[&0x12], BatchOutcome::Confirmed);
        assert!(fx.now <= start + BATCH_DEADLINE_MS, "deadline never reset");
        assert!(
            fx.wait_calls.len() >= 4,
            "unrelated wakes loop without confirm"
        );
    }

    #[test]
    fn destroyed_window_reports_absent_while_peer_confirms() {
        let hwnds = [0x21, 0x22];
        let mut fx = Fixture::new(&hwnds, true);
        fx.at(100, 0x21, None);
        fx.at(200, 0x22, Some(false));
        let out = run_visibility_batch(&hwnds, true, &mut fx);
        assert_eq!(
            out,
            vec![
                (0x21, BatchOutcome::Absent),
                (0x22, BatchOutcome::Confirmed)
            ]
        );
    }

    #[test]
    fn reveal_direction_posts_show_and_confirms_only_visible() {
        let hwnds = [0x31, 0x32];
        let mut fx = Fixture::new(&hwnds, false);
        fx.at(100, 0x31, Some(true));
        let out = run_visibility_batch(&hwnds, false, &mut fx);
        assert_eq!(fx.post_args, vec![(0x31, false), (0x32, false)]);
        let map = Fixture::outcome_map(&out);
        assert_eq!(map[&0x31], BatchOutcome::Confirmed);
        assert_eq!(map[&0x32], BatchOutcome::Uncertain);
    }

    #[test]
    fn post_work_consumes_the_shared_budget() {
        let hwnds = [0x41, 0x42, 0x43];
        let mut fx = Fixture::new(&hwnds, false);
        fx.post_cost_ms = 10;
        let start = fx.now;
        let out = run_visibility_batch(&hwnds, true, &mut fx);
        assert!(out.iter().all(|(_, o)| *o == BatchOutcome::Confirmed));
        assert!(fx.wait_calls.is_empty(), "already observed: no wait needed");
        assert_eq!(
            fx.now,
            start + 3 * 10,
            "fake posts advance the shared clock"
        );
        assert!(fx.now < start + BATCH_DEADLINE_MS);
    }
}

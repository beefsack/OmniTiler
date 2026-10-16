use super::*;

/// Slow or hung managed windows supply unknown instead of blocking the loop.
const MIN_HINT_TIMEOUT_MS: u32 = 10;

/// Share a deadline across domains, reserving most of the poll interval for
/// other work. Native identity/frame calls and scheduling are not hard realtime.
const MIN_HINT_OP_BUDGET_MS: u64 = (TICK_POLL_MS * 2 / 5) as u64;

/// Last successful declared-hint state for one live member: the behavioral
/// fallback for a later native query Timeout. Keyed by full member identity
/// (`WindowKey`); `token` plus `tag` bind the opaque member-token/tag
/// lifecycle so HWND reuse or a new member never inherits. Replaced only by
/// the next successful query (usable hints store, successful-but-empty
/// clears); unperformed or failed queries never touch it. `at` stamps the
/// successful query for diagnostic age only and never expires anything.
#[derive(Debug, Clone)]
pub(super) struct LastGoodHint {
    pub(super) hints: WindowSizeHints,
    pub(super) token: String,
    pub(super) tag: String,
    pub(super) at: Instant,
}

/// Lightweight per-token Timeout-reuse report: whether the row's hints rode
/// the last-good fallback, the reused entry's age, the deciding tick, and
/// the lane reason (`fresh`, `reused`, `no-entry`, `no-hint`, `identity`,
/// `invalid`, `failed`, `budget`, `retained`, `retained-hint`). Read by
/// minimum-hint and per-window diagnostics; no log lines here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct HintFallbackUse {
    pub(super) reused: bool,
    pub(super) age_ms: u64,
    pub(super) tick: u64,
    pub(super) reason: &'static str,
}

/// Store one fresh usable declared hint for `key`, replacing any previous
/// entry immediately. `now` is the assembly tick time read before the queries
/// run, so the stored timestamp slightly predates query completion; it feeds
/// diagnostic age only. A fresh empty hint is contradictory (usable outcomes
/// never carry one) and clears instead, fail-closed.
pub(super) fn note_last_good(
    cache: &mut HashMap<crate::workspace::WindowKey, LastGoodHint>,
    key: &crate::workspace::WindowKey,
    token: &str,
    tag: &str,
    hints: WindowSizeHints,
    now: Instant,
) {
    if hints.is_empty() {
        cache.remove(key);
        return;
    }
    cache.insert(
        key.clone(),
        LastGoodHint {
            hints,
            token: token.to_owned(),
            tag: tag.to_owned(),
            at: now,
        },
    );
}

/// Drop any last-good entry for `key`: a successful-but-empty query
/// (`NoHint`) or a proven identity break must clear so a later Timeout cannot
/// revive stale hints. Unperformed queries (`Invalid`), failures, and budget
/// skips never call this: they preserve the entry.
pub(super) fn invalidate_last_good(
    cache: &mut HashMap<crate::workspace::WindowKey, LastGoodHint>,
    key: &crate::workspace::WindowKey,
) {
    cache.remove(key);
}

/// Timeout candidate for `key`: the stored hints plus diagnostic age when an
/// entry is still bound to the live member identity (stored token/tag equal
/// the live ones). Identity-moved entries drop here with no native read. The
/// caller still verifies the live lifetime tag before reusing: a same-process
/// HWND reuse keeps HWND/PID/creation, so the property check is the actual
/// reuse fence. Consulted on Timeout only: unperformed, failed, and
/// budget-skipped queries never reach here and never touch the entry.
/// Miss reasons are explicit: `no-entry` (absent or token/tag lifecycle
/// moved), `identity` reserved by the caller for a live lifetime-tag
/// mismatch.
pub(super) fn timeout_candidate(
    cache: &mut HashMap<crate::workspace::WindowKey, LastGoodHint>,
    key: &crate::workspace::WindowKey,
    token: &str,
    stored_tag: &str,
    now: Instant,
) -> std::result::Result<(WindowSizeHints, u64), &'static str> {
    let Some(entry) = cache.get(key) else {
        return Err("no-entry");
    };
    if entry.token != token || entry.tag != stored_tag {
        cache.remove(key);
        return Err("no-entry");
    }
    let age = now.saturating_duration_since(entry.at);
    Ok((
        entry.hints,
        age.as_millis().min(u128::from(u64::MAX)) as u64,
    ))
}

/// Test-controlled clock for diagnostic age and the diagnostics emission
/// throttle (unit tests only; compiled out of production builds).
/// Production always reads live time, so no timer, wait, or deadline changes
/// ride this seam.
pub(super) fn hint_now() -> Instant {
    #[cfg(test)]
    {
        if let Some(now) = INJECTED_NOW.with(|slot| slot.get()) {
            return now;
        }
    }
    Instant::now()
}

#[cfg(test)]
thread_local! {
    static INJECTED_NOW: std::cell::Cell<Option<Instant>> = const {
        std::cell::Cell::new(None)
    };
    static INJECTED_OUTCOMES: std::cell::RefCell<std::collections::HashMap<u64, HintOutcome>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

/// Pin the last-good clock (test-only). `None` restores the live clock.
/// Cleared by [`clear_test_hints`].
#[cfg(test)]
pub(super) fn inject_hint_now(now: Option<Instant>) {
    INJECTED_NOW.with(|slot| slot.set(now));
}

/// Inject one controlled fresh-query outcome for `hwnd` (test-only): any
/// outcome but `Hint` stands in for the OS answer with no hint and no native
/// call. An injected `Invalid` is an unperformed query (identity/measurement
/// failure, never a successful empty) and never clears last-good state; an
/// injected `NoHint` is a successful empty query and does clear it, exactly
/// like empty injected hints (see [`test_hint_for`]). Overwrites any previous
/// outcome for the same window; [`clear_test_hints`] resets all of them.
#[cfg(test)]
pub(super) fn inject_test_outcome(hwnd: u64, outcome: HintOutcome) {
    if outcome == HintOutcome::Hint {
        return;
    }
    INJECTED_OUTCOMES.with(|map| {
        map.borrow_mut().insert(hwnd, outcome);
    });
}
/// One minimum-hint query outcome for bounded summaries. `Hint` carries a
/// usable hint; `NoHint` is a successful query that reported no usable hint
/// (and therefore replaces last-good state); every other variant is a raw
/// unknown whose row carries no fresh hint. Only `Timeout` consults the
/// last-good fallback; unperformed, failed, and budget-skipped queries never
/// touch it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum HintOutcome {
    Hint,
    NoHint,
    Timeout,
    Failed,
    Invalid,
    Budget,
}

impl HintOutcome {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Hint => "hint",
            Self::NoHint => "no-hint",
            Self::Timeout => "timeout",
            Self::Failed => "failed",
            Self::Invalid => "invalid",
            Self::Budget => "budget",
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct HintStats {
    pub(super) queried: usize,
    pub(super) with_hint: usize,
    pub(super) timeout: usize,
    pub(super) failed: usize,
    pub(super) invalid: usize,
    pub(super) budget_skipped: usize,
}

impl HintStats {
    pub(super) fn note(&mut self, outcome: HintOutcome) {
        match outcome {
            HintOutcome::Hint => {
                self.queried += 1;
                self.with_hint += 1;
            }
            HintOutcome::NoHint => {
                // A successful query with no usable data summarizes with the
                // other unknown remainders; the log shape is unchanged and
                // per-row diagnostics keep the distinct `no-hint` outcome.
                self.queried += 1;
                self.invalid += 1;
            }
            HintOutcome::Timeout => {
                self.queried += 1;
                self.timeout += 1;
            }
            HintOutcome::Failed => {
                self.queried += 1;
                self.failed += 1;
            }
            HintOutcome::Invalid => {
                self.queried += 1;
                self.invalid += 1;
            }
            HintOutcome::Budget => {
                self.budget_skipped += 1;
            }
        }
    }
}

/// Fresh query deadline shared by all domains in this operation.
pub(super) struct HintCx {
    deadline: Instant,
}

impl HintCx {
    pub(super) fn new() -> Self {
        Self {
            deadline: Instant::now() + Duration::from_millis(MIN_HINT_OP_BUDGET_MS),
        }
    }

    pub(super) fn over_budget(&self) -> bool {
        Instant::now() >= self.deadline
    }

    pub(super) fn timeout_ms(&self) -> u32 {
        self.deadline
            .saturating_duration_since(Instant::now())
            .as_millis()
            .min(u128::from(MIN_HINT_TIMEOUT_MS)) as u32
    }
}

// Test-only controlled fresh-hint results, keyed by HWND (see
// `inject_test_hints`): unit tests inject already-converted production
// hints here so assembly exercises the real query call sites without native
// windows; ordinary tests leave the map empty and keep the fail-closed
// native behavior. Compiled out of non-test builds: no production path can
// observe it.
#[cfg(test)]
thread_local! {
    static INJECTED_HINTS: std::cell::RefCell<std::collections::HashMap<u64, WindowSizeHints>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

/// Inject one controlled fresh-hint result for `hwnd` (test-only). Overwrites
/// any previous injection for the same window; [`clear_test_hints`] resets
/// all of them. Tests must use distinct fake HWNDs per member and clear
/// before arranging.
#[cfg(test)]
pub(super) fn inject_test_hints(hwnd: u64, hints: WindowSizeHints) {
    INJECTED_HINTS.with(|map| {
        map.borrow_mut().insert(hwnd, hints);
    });
}

/// Drop every injected fresh-hint result and outcome plus the pinned clock
/// (test-only).
#[cfg(test)]
pub(super) fn clear_test_hints() {
    INJECTED_HINTS.with(|map| {
        map.borrow_mut().clear();
    });
    INJECTED_OUTCOMES.with(|map| {
        map.borrow_mut().clear();
    });
    INJECTED_NOW.with(|slot| slot.set(None));
}

/// Controlled fresh-hint result for `hwnd`, if injected (test-only). An
/// injected Timeout/Failed/Budget/Invalid outcome reads first with no hint
/// and no native call; otherwise non-empty hints read `Hint` and empty hints
/// read successful-empty `NoHint`.
#[cfg(test)]
pub(super) fn test_hint_for(hwnd: u64) -> Option<(WindowSizeHints, HintOutcome)> {
    if let Some(outcome) = INJECTED_OUTCOMES.with(|map| map.borrow().get(&hwnd).copied()) {
        return Some((WindowSizeHints::none(), outcome));
    }
    INJECTED_HINTS
        .with(|map| map.borrow().get(&hwnd).copied())
        .map(|hints| {
            let outcome = if hints.is_empty() {
                HintOutcome::NoHint
            } else {
                HintOutcome::Hint
            };
            (hints, outcome)
        })
}

impl TileLoop {
    /// Lightweight per-token Timeout-reuse report: `Some` when the token rode
    /// a fresh-query lane on a recent tick, `None` when it never did or was
    /// pruned with membership. Test-only reader; the stored
    /// `hint_fallback_uses` data is production state.
    #[cfg(test)]
    pub(super) fn hint_fallback_report(&self, token: &str) -> Option<HintFallbackUse> {
        self.hint_fallback_uses.get(token).copied()
    }
}

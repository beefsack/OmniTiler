use super::*;

/// Slow or hung managed windows supply unknown instead of blocking the loop.
const MIN_HINT_TIMEOUT_MS: u32 = 10;

/// Share a deadline across domains, reserving most of the poll interval for
/// other work. Native identity/frame calls and scheduling are not hard realtime.
const MIN_HINT_OP_BUDGET_MS: u64 = (TICK_POLL_MS * 2 / 5) as u64;
/// One minimum-hint query outcome for bounded summaries. `Hint` carries a
/// usable hint; every other variant maps to unknown (no hint).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum HintOutcome {
    Hint,
    Timeout,
    Failed,
    Invalid,
    Budget,
}

impl HintOutcome {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Hint => "hint",
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

/// Drop every injected fresh-hint result (test-only).
#[cfg(test)]
pub(super) fn clear_test_hints() {
    INJECTED_HINTS.with(|map| {
        map.borrow_mut().clear();
    });
}

/// Controlled fresh-hint result for `hwnd`, if injected (test-only). The
/// outcome mirrors the conversion rule: non-empty hints read `Hint`,
/// empty hints read `Invalid`.
#[cfg(test)]
pub(super) fn test_hint_for(hwnd: u64) -> Option<(WindowSizeHints, HintOutcome)> {
    INJECTED_HINTS
        .with(|map| map.borrow().get(&hwnd).copied())
        .map(|hints| {
            let outcome = if hints.is_empty() {
                HintOutcome::Invalid
            } else {
                HintOutcome::Hint
            };
            (hints, outcome)
        })
}

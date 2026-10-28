# Windows-host test flakes

## Scope and outcome

- Investigate the two failures in the Windows-host test-flakes backlog item,
  without weakening assertions or running live desktop tests.
- Delivered unit: fix the proven `scripted full row` cause and retain a
  deterministic regression. The panic-diagnostic failure remains unresolved;
  this is not completion of the entire backlog item.

## Scripted-row cause and fix

- `wrong_sid_arrival_is_unproven_without_replay` failed twice in 16 isolated
  sequential runs before the fix. A captured failure showed a condvar timeout
  with 67 microseconds still remaining on the monotonic deadline. Settlement
  then took an extra full-row read before its final deadline read, exhausting
  the four-row script. No shared test state was involved.
- This is a product wait-contract defect exposed by a strict fixture, not a
  reason to extend or repeat the scripted rows. `SpaceSelectLatch::wait` now
  checks its monotonic deadline before returning `TimedOut`, re-waiting any
  remainder while preserving generation and cancellation checks.
- Observed on the Windows condvar path. Equivalent early-return behavior on
  macOS has not been demonstrated; the deadline guard is platform-neutral.
- A private sleep/clock seam forces an early timeout and then the real bound,
  without real sleeps or scheduler-dependent elapsed-time assertions. With the
  old timeout condition it fails deterministically (one sleep instead of two);
  with the fix it passes. All original scripted-test assertions are unchanged.
- After the initial fix: 120/120 parallel and 16/16 sequential wrong-SID
  executions passed. After the final seam: 32/32 each passed for wrong-SID,
  multiple-wrong, no-event, and deterministic early-timeout regression tests.

## Panic-diagnostic investigation: unresolved

- The proposed concurrent-foreign-panic explanation was disproved. Rust holds
  the hook read lock through callback execution; restoring with `set_hook`
  takes the write lock and waits for in-flight callbacks. The test drops its
  guard before both file snapshots, so unrelated caught panics cannot append
  through that guard between the snapshots. The isolation changes based on
  this explanation were removed.
- An original-order race experiment ran 200 iterations with foreign caught
  panics and no snapshot mismatch. The unchanged six-test module passed 16/16
  invocations at one test thread and 16/16 at eight threads. These are
  nonreproduction counts, not proof of a fix.
- Ignored file-write/read errors can deterministically produce the test's
  failure when file access is fault-injected. That establishes sensitivity,
  not the historical cause. No speculative IO retry or weaker assertion was
  added. Temporary experiments were removed.
- Needed to continue: the historical failing assertion and output (or a fresh
  captured failure). The change record that first reported the failure names
  only the test, not its assertion.

## Verification

- Passed: `mise exec -- cargo test --locked -p tiler-core -p tiler-protocol
  -p tiler-windows` (57 suites, zero failures; Windows library 1207 passed,
  one ignored; core library 1016 passed).
- Passed: `mise exec -- cargo clippy --locked -p tiler-core -p tiler-protocol
  -p tiler-windows --all-targets -- -D warnings`.
- Passed: `mise exec -- cargo fmt --all -- --check` and `git diff --check`.
- Targeted macOS-crate offline tests passed on Windows. Broader diagnostic
  sweeps before/after the initial fix had the same 19 pre-existing
  environment-inapplicable failures (nine in the Dock-swipe module); no full
  macOS-suite success is claimed.
- Final independent review approved the scoped latch fix with no blocking
  findings. A timing assumption in an earlier regression was removed before
  acceptance; the retained regression is fully scripted.

# Hermetic preimage and launcher teardown flakes

## Scope and acceptance

Reproduce the private-bus planner test's temporary-directory failure and the
first-run KDE launcher gate teardown failure. Fix the underlying allocation or
ordering issue without retrying failures or changing production policy. Keep the
frozen fixtures unchanged. A fix requires a failing pre-change reproduction,
passing focused tests, and a counted passing stress run covering that condition.
If a failure cannot be reproduced, retain it as a known issue with the attempts
and results.

## Approach

- Planner test fixture: determine whether the PID-plus-wall-clock allocation
  can collide and whether that yields the reported panic. If reproduced, remove
  the source of collision and preserve the underlying error for diagnosis.
- Launcher gate: isolate the teardown assertion that can race, then verify its
  ordering in the isolated shell harness; no live desktop interaction.

The original C1 launcher gate logged two failures: `dev-off` could not
signal the recorded Planner (`No such process`), then the persisted state
remained. The isolated harness's D-Bus and fake `/proc` can still describe
the old Planner after reload has waited for its real process to exit and
before the background replacement announces ownership. The reproduction
must distinguish that handoff from a live Planner dying during teardown.

## Planner outcome

With the old allocator, forcing two calls to use the same timestamp in one
process reproduced `AlreadyExists`, which the helper discarded before the
test panicked. An unforced 1,600-allocation contention run did not fail. The
historical panic omitted the underlying error, so the old log cannot prove
that the same collision caused that run.

The root name now includes a process-local sequence in addition to PID and
timestamp. It needs no retry and retains the original private permissions.
Every caller reports the I/O error instead of discarding it.
`cargo test -p omnitiler --lib planner_service::tests` passed all 34 tests,
including 1,600 concurrent, fixed-clock allocations checked for uniqueness
and permissions.

## Launcher outcome

Holding the mock replacement until the first owner query after the old
Planner's exit made reload record the dead PID: the mock still advertised
the old D-Bus owner and its synthetic `/proc` entry still verified. `dev-off`
then failed with `No such process` and left the state, matching the original
gate. This was a harness handoff bug, not evidence of a production race.

The mock now reports the name unowned between the old process's exit and
atomic publication of the new PID. The launcher gate asserts that reload
records the replacement and that teardown actually stops it.
The gated reproduction now passes, as did 20 stress runs (five batches of
four parallel runs) and the final `bash scripts/dev-loop-split.test.sh`
gate (712 checks). The four frozen fixture trees remain byte-identical to
`ff6eaddc`.

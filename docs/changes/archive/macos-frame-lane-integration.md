# macOS Frame-Lane Integration

## Goal and acceptance

Restore the locked macOS integration gate after Controlled frame-attempt feedback was enabled. Keep first-write, freshness, mid-pass safety, and clamp-settlement contracts intact; correct integration fakes only where they diverge from AX behavior or assert a superseded contract. No shared core changes or native acceptance claim.

## Approach and verification

Classify each reported failure before changing its fixture or adapter. Apply scoped Rust formatting to the changed code, then run the locked macOS library and integration tests, macOS Clippy, scoped formatting check, and staged and unstaged whitespace checks. The pre-existing Windows log-sink warnings and macOS restart-fixture type-complexity diagnostic are outside this change.

## Diagnosis

The failed convergence and freshness assertions were preceded by successful
fixture writes that did not publish readback or update the subsequent owner
facts. With Layout settlement enabled, the next accepted report legitimately
held the unchanged desired/observed pair. The freshness traces show the
missing commands were never emitted, rather than dropped in the worker
queue. The mid-pass case did reach a setter: the first size leg succeeded,
and the later size and position legs were fenced. Its oracle confused the
first-size flag with the final-size flag.

The first focused pass left only the reseed fixture red (83 passed, one
failed). The attempted artificial drift arrived in the report that settled
an earlier write and was legitimately suppressed. The test must give both
owner readers the same real retained frame and prove that the post-delete
plan converges exactly to it, rather than requiring a redundant AX call.

## Outcome

The locked macOS library and integration tests passed. The convergence
fixtures now report retained AX frames, and an integration case verifies
that a real one-point clamp holds across unrelated reports and re-arms on
an observed frame change. The corrections did not change production
behavior. Native maximize and half-snap acceptance remain separate.

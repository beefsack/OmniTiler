# KDE v4 command surface cleanup

## Goal and scope

- Apply the approved incremental removal policy to obsolete adapter interfaces exposed by the command-ingress audit.
- Correct `cmdFocus` to v4 by-id focus, remove uncalled `cmdSwitchView` and the retired `operation` type arm, and cover by-id focus with the real decoder.
- Retain immutable fixtures, active migration records and both enabled stage5 proofs. No dedicated legacy-code sweep.

## Acceptance and approach

1. Verify that removed constructors/arms have no live construction path; remove newly unreachable local branches only where that proof applies.
2. Preserve current command behavior and the v4 contract; run adapter typecheck and the full adapter suite without new skips.
3. Record the approved incremental removal decision beside the implementation order and archive this note after verification.

## Accepted outcome and evidence

- `cmdFocus` now emits exactly `{op:"focus", win}` and is covered through the genuine v4 decoder in the all-builder regression.
- Removed `cmdSwitchView`, the `switch-view` and `operation` union arms, and their unreachable command-context/window-target reader branches. Source search found no production constructor caller or remaining construction of the removed forms; native view setters are unrelated and unchanged.
- Updated the by-id queue-codec test and removed retired names from the actuation vocabulary oracle. Typecheck caught one old direction-bearing test literal, repaired without changing the test's semantics.
- Direct verification: adapter `npm run typecheck` and full `npm test` pass (985 tests, zero skips), including the poison-head regression. No immutable fixture, active migration record or stage5 proof was removed or disabled.
- The incremental-removal decision is recorded in place beside the implementation order in `docs/decisions/contract.md` (User 2026-11-07). This was a bounded cleanup of proved-obsolete interfaces, not a separate sweep.
- Command-ingress stall behavior was fixed separately. Native arrival, resize diagnosis and other migration slices are unchanged.

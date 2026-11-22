# PW-EV-02: Maximize-to-false state lane

Completed. The [protocol review ledger](../protocol-correctness-review.md#pw-ev-02--consolidate-maximize-to-false) records the disposition.

The approved option uses the existing controlled `state.maximized = false` target for directional and born clears. The executed publication `rev` and native facts return as `kind:"state"` evidence. Core retains the continuation and its provenance on that binding; requester operation identity, settlement `kind`, and authority do not move to the adapter.

Only automatic clears carry `automaticClear:true` on the maximized-false target. The marker keeps the existing fresh safety/homing/frame checks and synchronous setter without preimage storage. Ordinary explicit maximize-false targets omit it and keep the durable preimage executor and its own checks. `cmdSeq` alone cannot distinguish the two requester-bearing targets, and requester results do not travel in the apply reply. The marker is never echoed in feedback.

## Scope and acceptance

1. Cut over core and Rust codecs/consumers, preserving directional and born permissions and completion policy. Keep early clear settlement ordering: ordinary state evaluation requires immediate post-state proof where a directional clear can complete asynchronously on fresh observations. Reject the removed wire spelling.
2. Cut over KDE actuation, spend tracking and reporting; regenerate affected active vectors using the established recorders. Historical fixture families stay byte-identical to `ff6eaddc`; deferred Windows and macOS ports are outside this change. PW-OUT-05 settlement taxonomy and PW-BR-04 clock policy are not changed.
3. Review the full cutover and verify regression red/green evidence for directional clear, born clear and old-kind refusal; Rust format, scoped clippy and all-target tests, raw evaluator tests, fresh raw/broker example builds, KDE typecheck and test suite, frozen fixture diff and whitespace check.

## Accepted Rust evidence

Directional, born, and ordinary explicit state evidence share the normalized
state lane. Automatic provenance is correlated before continuation; ordinary
evidence keeps its original settlement phase. Publication follows the original
tile, exception, and held-born permission routes. A held geometry row cannot
authorize directional clear and can still authorize an eligible born clear.

The Rust permission group passed 121 tests; the broker born group passed 16.
All-target Rust compilation passed for the three in-scope crates. Active vectors
and the integrated gate are not covered by those checks.

## Accepted KDE and active vectors

KDE reports only the state attempt and fences the synchronous setter again
after native reads so a reentrant apply cannot actuate the same reply twice.
The renderer projection filters out the automatic target without filtering
ordinary state targets. The projection and reentrant refusal regressions
passed. The KDE suite passed 1,483 tests with one skipped.

The active typed reply, Stage5 full and s2, KDE byte, and KDE migration
recorders were used to refresh the affected vectors. The active report
inventory keeps the legacy clear attempt as a negative witness. The four
frozen fixture families remain byte-identical to the baseline.

## Final verification

Directional and born state-lane feedback and strict rejection of `kind:"clear"`
attempts failed before the cutover and passed after. Independent review led to
three corrections: spend tracking lives through same-revision delivery
retirement but resets on a strictly newer accepted reply; replacing a retained
born clear deliberately settles its original commandless operation; and
automatic targets are decorated at the original publication sites without a
parallel per-row route allocation. Focused tests cover reentry, replay, revision
advance, projection rejection, and born collisions.

Formatting and warning-denied scoped Clippy passed. Scoped all-target Rust
tests passed (3,575 tests across 63 suites), the raw evaluator passed, and both
bridge examples built offline in a fresh Cargo target. KDE typecheck and
tests passed (1,485 passed, one skipped). Frozen fixtures and whitespace
checks passed. Windows native typed callers still reference the removed lane
and are deferred; no ignored ports were built or changed.

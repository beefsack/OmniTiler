# KDE D7 explicit-tile precondition

## Goal and scope

- Diagnose the fixed-client explicit-tile precondition after workspace enable
  offline, with zero or two intervening observations. Retain full reports and
  replies, known-set bindings, and exact reachable refusal predicates.
- Preserve fail-closed behavior. No delay workaround, live testing, controller
  replacement acceptance, protocol change, or inference about missing historic
  payloads.

## Acceptance and approach

- Trace every `partial-observation` site reachable by explicit unfloat through
  KDE and core, distinguishing defensive/unreachable guards.
- Exercise production KDE capture/entry with renewed wrappers, QML array-likes,
  native properties and genuine fixed hints against the real retained engine.
- Pin complete-report success and incomplete-report safety behavior with
  discriminating regressions. A local defect may be corrected without weakening
  safety; core/protocol-shape or public-behavior changes require a decision.

## Bounded work and verification

- Source trace: float-to-tile checks target binding and maximize state before
  nested fresh admission checks complete known/observed sets and peer bindings.
- Regressions live in `kwin/tests/world-fixed-tile-precondition.test.ts` and
  `crates/tiler-core/src/fixed_size_admission_tests.rs`.

## Exact refusal sites

- KDE `world-native.ts:665-746` submits `operation/toggle-float`; its local
  busy, unreadable-frame and maximize-clear fences do not emit
  `partial-observation`. `world_operations.rs:1332-1409` carries the operation
  through `Engine::handle`; `engine.rs:1384-1388` converges before the toggle.
- For a retained float, `propose_toggle_float` in
  `crates/tiler-core/src/session/ops/float.rs` refuses at:
  - Line 33: missing target.
  - Line 56: floating flag or output/workspace binding mismatch.
  - Line 79: target still maximized.
- Nested `propose_admit` in
  `crates/tiler-core/src/session/ops/lifecycle.rs` refuses at:
  - Line 258: observed IDs differ from retained IDs plus the subject.
  - Line 306: subject binding mismatch (sticky is a discriminating example).
  - Line 310: any peer's stored flags/homing fail the known-member binding.
- The same admission's line 265 is shadowed by set equality; line 496 is
  shadowed by subject binding on this route. Neither can race an immutable
  observation. `float.rs:130` and remove-leg `lifecycle.rs:975/987` belong to
  tiled-to-float, not an explicit tile of a retained float.
- Engine forwarding sites are `engine.rs:3721-3724` and `3850-3853`.
  The relocation arm catches partial errors for rebuild, rather than forwarding
  them from its other-error branch. A separate fallback at `3798-3801` emits
  `partial-observation` when a converged domain has no usable candidate and a
  concrete seed order; it refuses rather than reseeding. This is not the
  stable retained-float case. The other direct engine emitters, lines 2486
  and 3419, belong to reconciliation and migration, respectively.
- The wire command is a toggle, not a boolean tile setter. If core membership
  selects the opposite direction, its additional emitters are `float.rs:130`
  (missing target), `lifecycle.rs:975` (remove known-set mismatch) and
  `lifecycle.rs:987` (remove member-binding mismatch). These are not reached
  with the proven fixed-float precondition in the producer regressions.

## Accepted offline evidence

- Real production KDE entry/capture and raw broker normalization feed the
  retained `WorldEngine` through `raw_world_eval`; no Planner result is mocked.
  Renewed wrappers, QML array-likes, native properties and finite equal fixed
  hints reproduce the producer boundary, not real compositor settlement.
- One enable report leaves the fixed client automatically floating and the
  ordinary peer tiled. Explicit tile succeeds with zero or exactly two
  intervening observation-only reports; both logical/native member bindings
  and the exact two-window set remain stable. The tile override persists
  disjointly from floats after the post-command observation.
- Complete calls, command payloads, capture envelopes, stamped reports, raw and
  bound replies, take results and intent-store writes are retained unsliced in
  each test's diagnostics, including successful runs.
- Nine core single-fact mutations isolate the six reachable session guards:
  missing target, stale target floating/homing, maximized target, missing/extra
  peer, stale peer flags/homing and sticky subject. Each refuses without a
  staged plan/override or changed tiled membership; restored complete facts
  accept in the same session. Separate complete controls pass directly and
  after two converging observations.
- An unreadable-frame observation-only producer report returns a held scope
  (`incomplete-inventory`) with zero geometry/membership writes and recovers
  on complete facts. This is not an explicit-tile refusal reproduction and
  does not identify the historic trigger.
- The initial store-write oracle incorrectly required an override in early
  empty writes; only the final settled write is relevant. An initial
  incomplete-report oracle also expected rejection instead of a held scope.
  Tightening the enable test then exposed a harness-only error: running
  scheduled callbacks before the immediate press dispatched an extra observe.
  The repair issues the command directly after enable; it adds no delay,
  retry, hidden report or product workaround.

## Verification

- Sequential commands, each prefixed `devenv shell --impure --`:
  - `npm test --prefix kwin`: 838 passed, zero failures after the harness repair.
  - `npm run typecheck --prefix kwin`: passed.
  - `cargo test -p tiler-core -p tiler-windows -p tiler-protocol`: passed,
    including the new exact-refusal regression.
  - `cargo clippy --workspace --exclude tiler-macos --all-targets -- -D warnings`:
    passed.
  - `cargo fmt --all -- --check`: passed after a mechanical line-wrap fix.
  - `just check-portable`: passed.
- `devenv shell --impure -- git diff --check`: passed.

## Outcome

- No product defect with complete facts reproduced; intentionally incomplete or
  inconsistent session facts correctly fail closed. No production/core/protocol
  changes or new product decision.
- Historic product-versus-fixture classification and its exact refusal site
  remain unresolved: missing request/reply data cannot be reconstructed from
  current passing sequences. Keep that evidence gap and clean controller-restart
  acceptance open; the backlog item is not closed. Do not infer that intervening
  changes fixed the historic run.
- Next evidence needs a complete failing operation/report and retained member
  bindings to select the exact predicate above. Tiled-send held-scope diagnosis
  remains a separate item.

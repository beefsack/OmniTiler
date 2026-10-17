# Windows admission CI compatibility

- Goal: restore Windows gates after best-effort admission without changing core
  policy or weakening preview/restart safety coverage.
- Scope: Windows offline report fixtures and stale preview/restart assertions;
  no runtime changes or full Windows admission adoption.
- Acceptance: locked portable build/tests and strict all-target clippy, all-crate
  formatting, whitespace checks and Windows CI pass; report macOS CI separately.

## Findings and approach

- Windows CI first fails compilation: two test-only world-report constructors
  lack the new admission fields. Concurrent correction `26c88a8d` supplied the
  approved shared defaults before this test-only correction was integrated.
- `preview_sticky_journey_agrees_with_drop`: stale fixture selection. Admission
  order, topology and rectangles are unchanged; geometry-vector order changed.
  Pick the gap deterministically by both coordinates, preserving the first-contact
  versus sticky-band and preview/drop assertions.
- `rejected_hydration_holds_float_writes_for_one_observation` and
  `next_consistent_observation_revalidates_after_rejection`: stale expectations of
  the fixed-anchor duplicate-window rejection. Core now correctly admits the
  complete mixed roster atomically; assert adoption, float write exclusion and
  stable repeat/revalidation. Retain genuine rejection/rollback coverage.
- No core change is required. Windows runtime admission recovery and settings
  adoption remain outside this compatibility correction.
- All three failures are stale test expectations (category a), not core bugs or
  newly introduced runtime adapter gaps. The two missing fields are likewise
  mechanical offline-fixture compatibility, not runtime adoption.
- A missing-monitor fixture separately preserves the observation-scoped float
  hold/rollback barrier; successful revalidation must produce a plan containing
  only the plain tiled remainder, not pass vacuously on rejection.

## Verification

- Initial portable test reproduction: core/protocol/FFI pass; Windows has exactly
  the three failures above after the two missing-field compile fixes.
- Existing macOS CI passed on the failing admission run; no macOS edits planned.
- Locked native four-package build/tests and strict all-target clippy pass;
  `cargo fmt --all -- --check` and `git diff --check` pass. Tests: 2,714 passed,
  zero failed, one pre-existing ignored test. Windows library: 1,132 passed.
- The Windows CI five-package build/clippy also pass locally. Adding the macOS
  tests on native Windows exposes eight existing POSIX-helper lookup failures;
  this does not test native macOS behavior. Hosted CI verification remains needed.
- No runtime code changed, so no live smoke was required. No admission-policy
  decision or additional adoption backlog line is introduced by this test fix.
- Hosted CI after publication passed all jobs (`c75fe675`).

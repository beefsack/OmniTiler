# Windows born-maximized shared policy

## Goal and acceptance

- Complete Windows handoff item 23 by consuming
  `tiler_core::born_maximized::BornMaximized` directly, preserving behavior.
- Preserve schema-v1 `core.born_maximized`, exact tokens, default, strict
  invalid-file refusal/last-good adoption, labels/tooltips and subsequent-only
  lifetime-bound admission. Preserve native eligibility, fullscreen/fixed
  gates, one-shot clear attempts, independent born/retile latches and later
  user maximize behavior.
- Preserve item 13/D7 hydrated fixed-origin override routing under both
  values, including hint loss. That existing routing remains tentative,
  pending user review and native verification.
- Consume `test-fixtures/born-maximized.json` in Windows adapter tests and
  retain existing admission/settings/retile regressions.

## Approach and bounded work

- Replace adapter-local normalization/default/token and clear/keep policy
  with core calls; retain host-owned identity, eligibility and actuation.
- Audit core/protocol changes since `2c05ee5` for missing Windows wiring;
  repair straightforward compile/behavior integration and report material
  behavior or architecture questions separately.
- Inspect the implementation diff and run native MSVC build/test, workspace
  fmt, allowlisted strict clippy and diff checks. Verification is offline;
  physical launch/settings/restart/game checks retain their existing status.
- Record accepted evidence and delivery pointers, then archive this record.

## Decisions and implementation

- No new product decision is required for the behavior-preserving replacement.
- Tentative, pending user review: expose the core mode through
  `CoreSettings::born_maximized_mode()`, matching existing typed setting
  accessors. Keep-overlay predicates and the Settings radio read delegate
  to core parsing and `should_clear_at_admission`; native eligibility and
  actuation remain adapter-owned.
- Saved-file validation uses exact `parse_wire`, never fallback normalization.
  Missing values still default; invalid strings and non-string saved values
  still refuse adoption. Labels, tooltips, schema and error wording are intact.
- `tiling_sys.rs` is unchanged. Hydrated fixed-origin overrides still bypass
  born clear and reserve a tiled overlay under both values, including hint
  loss. Existing D7 status remains tentative, pending user review.

## Shared compatibility audit

`git log --oneline 2c05ee5..origin/main -- crates/tiler-core crates/tiler-protocol`
identified five shared changes; all were already present in the Windows base.

| Commit | Windows integration result |
| --- | --- |
| `637a690` shared born policy | Consumed directly in this delivery; no protocol change. |
| `4dffb05` D23 restore/close | Additive `closed_windows` already wired in event builders and production close draining by `07e6a16`; no omission found. |
| `9407c53` D16 combo sends | Core eligibility widening adds no fields; existing `ab60fc5` Windows event/observation wiring covers it. |
| `37f67e2` unsafe guardrail | Linux/test guardrails require no Windows adapter change. |
| `f2169d6` Windows event initialization | Cross-output `closed_windows` test initialization already present. |

No additional compile/behavior wiring or non-trivial shared-compatibility
follow-up was needed. Existing native D16/D23 checks remain pending.

## Verification and outcome

- Two new Windows conformance tests consume all eight shared fixture rows:
  settings/default/validation and ordinary eligible admission clear/keep.
  They distinguish core fallback normalization from strict Windows saved-file
  refusal, including missing/null/number/boolean inputs.
- Existing setting adoption, independent born/retile latches, one-shot clear,
  fullscreen/floating/fixed exclusions, reserved Engine slot and durable
  fixed-origin override regressions pass in targeted native offline runs.
- Review found no behavior or contract change. Completeness findings were
  resolved: Settings radio delegation and explicit strict non-string file
  refusal coverage. Initial full strict clippy found one boolean assertion
  style violation in the new test; repaired without changing its predicate.
- Native MSVC Rust 1.99.0 (`x86_64-pc-windows-msvc`) locked allowlisted
  build/test passed: 1,847 passed, 0 failed, 1 ignored (+2 conformance tests
  over the 1,845 baseline). Workspace fmt, allowlisted all-targets strict
  clippy (`-D warnings`) and `git diff --check` passed after the assertion
  repair and formatting. No dependency changes or live effects.
- Item 23 is complete offline; Windows also closes the core lift's prior
  native compile gap. KDE retains its conformance-pinned TypeScript copy
  under option C. Publication CI is checked against the delivered head.

## Pending checks

- No live testing in this delivery. Browser session restore, detailed Settings
  adoption/Apply/Revert, hydrated fixed override restart/maximize, game,
  DPI and multi-output journeys retain their prior native-pending status.
- R-SPC-13's native discriminator remains TBD; the existing fixed-origin
  routing is preserved, not promoted to a new accepted product outcome.

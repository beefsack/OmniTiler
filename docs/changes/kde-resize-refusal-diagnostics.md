# KDE keyboard resize refusal diagnostics

## Goal and scope

- Diagnose intermittent keyboard resize refusals without changing behavior,
  public interfaces or requester-visible `operation-refused` results.
- Preserve the detailed internal refusal and correlate it with the command,
  observed and retained focus, geometry, hints and bounded layout context.
- Distinguish legitimate missing-neighbor, outer-edge, minimum/maximum and
  exhausted-limit outcomes from defects before changing policy or feedback.
- Arrival, focus feedback, state targets and scaled metrics are separate work.

## Evidence and limits

- Initial dogfood summaries contain 30 resize dispatches: seven succeed and 23
  return the generic refusal. Five directional moves also refuse. The log does
  not preserve enough context to recover their detailed reasons retrospectively.
- Two additional resize presses encounter the existing single-flight busy gate.
  Seven stale presentation coverage refusals recover; two opaque intent seed
  reads take the existing degraded corrupt-data path.
- Native-effect fact changes coincide with selected/hidden presentation churn,
  but that correlation does not establish a resize cause. No defect is yet proved.

## Plan and acceptance

1. Add bounded internal diagnostics at the real v4 rejection boundary before
   detailed reasons are collapsed; retain the unchanged requester result.
2. Verify correlation, sanitization, truncation, populated context and genuine
   v4 refusal behavior; run full adapter and scoped Rust gates without new skips.
3. Classify reasons from a user re-check with real keyboard shortcuts, including
   after a populated selection. Preserve the complete `just dev` combined log.
   A direct-ingress nested run would not exercise the shortcut callbacks.
4. Leave classification pending until that evidence arrives; do not block the
   arrival slice. A feedback behavior choice remains undecided rather than
   silently changing refusal semantics.

## Current state

- The v4 and retained legacy rejection boundaries now emit one best-effort
  `omnitiler:resize-refused` line before the detailed kind is collapsed. Request,
  command sequence, subject, direction, mode, repeat index, observed focus,
  retained focus leaf, root layout, geometry and min/max hints are correlated.
  The requester result and command behavior are unchanged.
- Diagnostics sanitize identifiers and tokens, omit titles and process metadata,
  and cap output at 960 bytes with an explicit cut marker. They show at most four
  staged domain rows with total/shown counts; these are not the selected pair.
  Nested layout groups are not expanded; the root summary alone cannot classify
  every nested-neighbor case.
- Eight new regressions pass, including a genuine typed-v4 singleton refusal,
  actual internal `unchanged` versus wire `operation-refused`, unknown subject,
  staged focus/geometry/hints, non-resize silence, sanitization and both row/byte
  truncation. Initial test-only compilation omissions and a formatter argument
  lint were corrected; no refusal semantics changed.
- Full adapter typecheck/build/test passes: 990 tests, zero skips. Scoped core,
  protocol and service tests pass with 1,970 core unit tests, zero ignored tests,
  actual private-bus ownership/storage/queue/wait outcomes and enabled stage5
  proofs. Scoped locked build, strict all-target clippy and formatting pass.
- Source confirms the stderr route into the `just dev` combined log:
  `emit_resize_refusal` writes Planner stderr; `just/linux.just` launches the
  Planner with stdout/stderr merged into its log, then tails every line through
  the `[planner]` prefix and combined-log `tee`, without a Planner-side filter.
  Expected evidence is `[planner] omnitiler:resize-refused ...` beside the command
  dispatch/result. No logging change is needed.
- The offline diagnostics increment is ready independently of native evidence.
  Independent integrated review found no in-scope blocker in the diagnostics or
  unchanged requester mapping. Detailed classification awaits the user re-check.
  This note stays active. No root-cause or resize defect fix is claimed.

## Subsequent input check not run

- No resize shortcut was delivered: fixture workspace staging was not
  established, and no synthetic keyboard/mouse injection tool was available in
  the development environment. No physical input was tested either.
- No new refusal diagnostic or classification evidence was collected. Direct
  command ingress would not establish shortcut-callback or physical-key
  acceptance. The real-key re-check remains pending; no resize defect is claimed.

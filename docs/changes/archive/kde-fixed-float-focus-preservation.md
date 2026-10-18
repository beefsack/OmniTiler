# KDE automatic fixed-float focus preservation

## Goal and scope

- Preserve native focus on automatic fixed-size floats when reconcile replies
  carry retained tiled-survivor focus, using the existing exact-reference
  effective-floating classification.
- D8 remains unchanged: automatic floats receive no geometry, focus, stacking
  or keep-above writes. Explicit tile overrides and close-survivor refocus
  retain their existing behavior. No shared-core or stacking-policy redesign.
- Add correlated automatic-focus attempt/result diagnostics without implying
  native completion beyond the available setter evidence.

## Acceptance and verification

- Add regressions before the fix and demonstrate the focus-theft failure.
- Cover active automatic float plus tiled survivor, retained classification
  after hint loss, native-reference replacement, explicit tile override and
  close-survivor refocus. Assert all activation writes, not just writes to the
  floating subject, and preserve its geometry and keep-above state.
- Full KWin tests, type checking and build pass through normal project commands.
- Review the diff independently; document diagnostic keys in the dev guide.
- Keep the existing P0 backlog item with an offline-delivery outcome and the
  Steam dialog/Alt+Tab native result pending user retest.

## Approach

- Establish real-planner regression evidence, then replace the raw floating
  predicate in the reconcile focus-preservation guard with effective floating.
- Log automatic-focus attempt and setter result with correlation, kind, target,
  reason and outcome; retain existing skip diagnostics.
- Inspect the diff, run full offline verification, resolve review findings and
  archive this note with the accepted evidence.

## Outcome

- Delivered offline 2026-10-17: reconcile focus preservation now uses the existing
  effective-floating classifier, including exact native-reference matching for
  automatic fixed-size floats. Existing manual/sticky skip reasons remain;
  automatic floats report `focus-skipped reason=automatic-active`.
- Automatic reconcile/update-gaps setter attempts emit correlated
  `automatic-focus-attempt` and `automatic-focus-result` diagnostics, with target,
  reason and setter outcome. Acceptance is not proof of native focus. The dev
  guide documents the keys. Explicit focus behavior and D8 remain unchanged;
  no decisions update is needed.
- Tests-first RED: focused file 43 tests, 37 passed, 6 failed before product
  changes. Three cases reproduced sibling activation despite an active automatic
  float; three exposed missing attempt/result diagnostics.
- Final GREEN: focused file 44/44; full KWin suite `npm --prefix kwin test`
  1574/1574 across 227 suites, zero failed/skipped/cancelled. Typecheck
  `npm --prefix kwin run typecheck` and `just build-kwin-script` passed.
  Focused verification bundles `kwin/tests/fixed-size-admission.test.ts` with
  the existing esbuild trace-test flags and runs the resulting Node test file.
- Regressions cover all-window activation silence while an automatic float is
  active, hint-loss retention, fresh fixed and resizable native references,
  explicit tile overrides, close-survivor activation recovery, and accepted,
  refused and throwing automatic setters. Automatic floats receive no geometry
  or keep-above writes.
- Independent review accepted the production diff and final regression cases.
  Initial review prompted stronger resizable-replacement and close-recovery
  combinations, both now covered. `git diff --check` passed.
- The existing P0 backlog item remains delivered pending user live retest.
  Attribution of the reported native Steam stacking incident remains unconfirmed.
  The discriminating retest is the Steam loading dialog and Alt+Tab with a
  terminal on the same workspace under `just dev trace`: the dialog should keep
  focus, with `automatic-active` skips instead of attempts to activate a sibling.

## Restoration verification 2026-10-18

- Restored onto current main with best-effort admission and capacity-float
  handling intact. No behavioral drift repair was needed; the newcomer timing
  probe was isolated from this change before verification.
- Full KWin suite: 1833/1833 across 275 suites, no failed, skipped or cancelled
  tests. `npm --prefix kwin run typecheck`, `just build-kwin-script` and
  `git diff --check` passed. Independent review accepted the current diff.
- Initial planner-dependent tests were blocked by a missing cached Rust crate;
  fetching the locked dependencies restored the checks without source or lock
  changes. The final full-suite run passed.
- User retest 2026-10-17 found focus preservation improved, with reconcile skips
  and no automatic focus attempts. Residual native drop-behind is unattributed.
  The backlog's NEXT stacking-policy decision remains separate and unimplemented.
- No new live evidence was collected. The `update-gaps` diagnostic branch shares
  the reviewed automatic-setter path but has no dedicated regression case.

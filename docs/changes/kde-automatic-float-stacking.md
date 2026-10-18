# KDE automatic fixed-float stacking

## Goal and boundaries

- Automatic fixed-size floats stay above tiled windows using KDE's existing
  keep-above ownership mechanism, without geometry or focus writes.
- User decision 2026-10-18: an automatic float with keep-below already set
  receives no stacking writes. Pre-existing keep-above remains untouched.
  Manual Meta+G keep-below clearing/restoration remains unchanged.
- Tentative: user/app keep-below set after admission wins; release only
  project-owned keep-above. Game safety wins on conflict.
- No Windows/macOS code or shared-core behavior changes are intended.

## Acceptance and approach

- Confirm public Wayland keepBelow scripting support in the pinned KWin source.
- Demonstrate regression failures before production edits; cover admission,
  retained classification, ownership, explicit tile/unfloat, teardown/restart,
  later keep-below, and fullscreen safety.
- Reuse the existing keep-above restoration mechanism without changing manual
  float policy. Inspect ownership across exact native-reference replacement.
- Run the KWin suite, typecheck, build, and independent review.
- Capture and restore the exact live baseline. Exercise disposable fixed-size
  floats above tiles with tiled focus, Alt+Tab, tile/unfloat, pre-existing flags,
  teardown/restart, fullscreen and modal legs. Real games remain user-observed
  acceptance, with exact steps recorded; unsupported outcomes remain TBD.
- Amend D8, REQ-SPC-04h and R-FLT-12 after verification; retain only accepted
  evidence and remaining native/user checks in the outcome.
- Port regression requirements, not the discarded PlanAdapter implementation:
  pre-existing above gets zero writes/restoration; pre-existing below gets zero
  stacking writes through admission, tile and teardown; later below releases
  owned above without writes; closed/replaced refs receive no cleanup writes.
  Only the fresh ref may acquire ownership. Explicit unfloat releases owned
  above once, tiles once with suppression and never writes below.
- Production-entry tests assert automatic keep-above exactly once, with zero
  frame, focus or keep-below writes; retained/fullscreen observations add no
  stacking writes. Mocks mirror KWin above/below exclusivity.
- Scope lifecycle tests to the selected world-route unload policy. Legacy
  disable-path restoration is not evidence or a decision for script unload.

## Current state

- Initial investigation found the manual path deliberately clears keep-below;
  automatic floats require a separate eligibility guard, not a manual-policy
  change. The option A decision above resolves this distinction.
- The unrelated Custom Tile preflight rejected default persisted tiling entries;
  that harness is not evidence for this change. No host mutation or cleanup
  occurred during investigation.
- Implementation attempt targeted the legacy PlanAdapter. Independent review
  found that production entry starts only the world adapter, so that attempt
  does not implement the requested shipped behavior. Its passing legacy tests
  are not production acceptance. That diff was discarded unlanded.
- The focus preservation fix `0bc59d07` also lives in `PlanAdapter`; no
  equivalent world-route guard was found, so shipped automatic-float focus
  preservation is unverified.
- Production also lacks an established unload-time restoration callback. The
  legacy disable tests do not demonstrate actual controller-unload restoration.
  Production world-path integration and lifecycle ownership must be scoped
  before further implementation or live testing. No live mutation occurred.
- Next: implement on the world route beside the manual/sticky keep-above
  handling in `kwin/src/world-native.ts`; decide unload restoration
  (recommendation: match Meta+G world-route behavior); verify or port
  automatic-float focus preservation; then the live legs above.

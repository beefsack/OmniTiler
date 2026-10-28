# KDE automatic fixed-float stacking

## Goal and boundaries

- Automatic fixed-size floats stay above tiled windows using KDE's existing
  keep-above ownership mechanism, without geometry or focus writes.
- User decision 2026-10-18: an automatic float with keep-below already set
  receives no stacking writes. Pre-existing keep-above remains untouched.
  Manual Meta+G keep-below clearing/restoration remains unchanged.
- User 2026-10-29 approves: user/app keep-below set after admission wins; release only
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

## Implementation and decisions, 2026-10-19

- `entry.ts` starts only the world adapter. The discarded legacy stacking
  attempt and `0bc59d07` are not shipped-route evidence; no legacy source was
  changed here.
- `world-native.ts` now maintains automatic keep-above separately from sticky
  ownership. Pre-existing above/below is untouched. All writes resolve a live
  stable ID and recheck native overlays, never a stored wrapper; QML array-like
  inventory is covered. Closed references receive no cleanup writes.
- Explicit tile releases owned above; the existing suppress pin prevents
  reacquisition. No automatic keep-below, frame or activation setter is added.
- `world-apply.ts` preserves focused automatic floats against `af-` admission
  effects without suppressing explicit `fx-` commands. Automatic fixed targets
  themselves refuse admission activation, including a real-shaped floating
  reply row. Correlated attempt/result diagnostics separate setter acceptance
  from native active readback.
- User 2026-10-29 approves: later below or external above-clear wins for
  the live window; loss of automatic classification restores owned above once.
  Same-scope focused fullscreen suspends owned above and exit reacquires it;
  live overlay targets receive no stacking writes, including restoration.
- User 2026-10-29 approves: explicit `handle.stop()` best-effort restores
  safe owned above, retaining refused ownership for another explicit call. This
  is not actual KWin unload restoration.
- Source checked locally at KWin `v6.7.5`: `src/window.h` exposes writable,
  script-readable `keepBelow`; `src/scripting/scripting.cpp` exposes only selected
  globals and deletes scripts in `AbstractScript::stop` / `unloadScript`, with no
  JavaScript stop hook. `entry.ts` does not call `handle.stop()`.

## Verification

- Initial regressions: 15 failures before implementation, then 16 passes.
- Review identified explicit-focus suppression, stale overlay writes,
  classification-loss cleanup and conflated focus diagnostics; corrected
  fixtures reproduced nine failures, then 28 passed. Unused stored references
  were removed. The actual-unload ownership finding remains unresolved.
- Native admission exposed a further producer mismatch: the first helper's
  `windows: []` reply missed the floating row plus `af-` intent. A real-shaped
  regression reproduced one forbidden activation; a separate regression
  reproduced ownership loss after refused explicit stop. Both fail before the
  corrections and pass after. Suppressed-tile and ordinary/capacity admission
  activation remain covered.
- Final integrated gates: KWin **1951 passed**, Rust workspace **3353 passed**,
  zero failures; typecheck, script build, Rust formatting and strict all-target
  workspace Clippy pass. These gates include the existing surrounding changes.

## Native samples and restoration limits

- Genuine fixed-size Qt windowed stand-in: fixed hints verified, frame unchanged
  over two samples, keep-above acquired once and retained. **Not accepted**:
  the sampled build issued one admission-focus write. The offline correction
  above was made afterwards and has not been repeated natively.
- Focused fullscreen editor stand-in: fullscreen, focus and frame retained;
  no controller emissions during the sampled interval. The prior float had
  already closed: this does **not** prove concurrent float/fullscreen suspension
  or game coverage safety.
- Genuine Qt window-modal dialog plus disposable terminal on the same
  workspace: modal/above flags retained and two API focus switches produced
  all-zero controller actuation summaries. This is an Alt+Tab-style focus
  cycle, not physical shortcut or visible stacking-order proof.
- Pre-existing above/below, later below, explicit tile, restart and concurrent
  game-suspension coverage are offline only. Real-game and Steam acceptance
  remain pending.
- Captured configuration bytes, shortcut tuples, float-store absence, loaded
  effects, desktop inventory and protected placement were retained. Final
  protected frame/state/membership/output/focus, shortcut tuples and effects
  match; all disposable windows closed, controller unloaded, Planner unowned,
  no keep-above survivor. Script configuration bytes match; float store remains
  logically empty (initial file absence was not independently rechecked).
- **Full restoration not established**: pointer was not captured, per-output
  view reads were null, and the run proceeded despite those baseline gaps.
  Configuration bytes differ by retirement of two original empty desktops;
  surviving IDs/names/order match, original count does not. No exact baseline
  or full live acceptance is claimed. No further native mutation followed.
- Correlated summaries and native readbacks were retained, but complete raw
  request/reply capture was not established. Summary logs cannot replace that
  gate. Restore/capture preconditions must be repaired before another live leg.

## Blocker and exact next action

- User 2026-10-29 approves the [core shape](kde-world-v2-port.md#recommended-core-shape-alternatives-and-cutover);
  the unload/restart ownership blocker below remains separate and open.
- Matching existing world-route unload behavior is **clearly unsafe**, not an
  accepted tentative policy: unloading with a surviving owned float leaves
  keep-above behind; restart treats it as pre-existing above and cannot suspend
  it for a later fullscreen game without violating option A.
- Parked question: **Should project-owned automatic-float keep-above be tracked
  by a lifecycle owner outside the script so it can be restored on
  unload/restart, while pre-existing keep-above remains untouched?** No recovery
  owner, persistence scheme or architectural expansion is selected.
- Resolved 2026-10-19: both retired empty desktops were recreated at their captured positions with captured names/order and new native IDs; protected terminal membership/frame/focus stayed unchanged and the controller stayed stopped; only replacement IDs and default tiling groups changed in configuration.
- After resolution: implement and prove actual unload/restart ownership, repair
  complete baseline/wire capture, repeat fixed-window/no-focus, concurrent
  fullscreen and modal/terminal legs on the corrected build. Leave the user's
  Steam retest pending. Keep this record active; acceptance is incomplete.

# KDE immediate unfloat re-tile

## Goal and scope

Fix Meta+G unfloat on the full-state reply route so the focused window
immediately occupies its tile, without requiring a later focus change.
Preserve existing float behavior and host safety fences. No new product
behavior was added. The original correction was offline-only; physical UF
acceptance is recorded below.

## Acceptance

- Diagnose the reported sequence from existing development logs and source.
- Add a producer-shaped regression that fails before the correction.
- Apply the smallest causal core/KDE correction and document any learned host
  shape at its point of use.
- Pass the Rust workspace tests and KWin suite; retain exact pass counts.
- Record the offline result and the remaining user-owned native re-check.

## Delivered correction

- Settle intentional-float departures and explicit unfloat tile overrides before
  the post-accept capture in `kwin/src/world-entry.ts`. This lets the same reply
  apply the tile frame, including for fixed-size clients.
- Keep float arrivals and re-float override withdrawal after capture: explicit
  float placement still needs the pre-command non-floating classification.
  No extra enumeration, duplicated classifier or relaxed safety gate was added.
- The ordering invariant is documented at the point of use. No core, protocol,
  Windows or macOS changes were required; no product decision changed.

## Evidence and outcome

- User report (2026-10-17): with two tiled terminals, float succeeds; unfloat
  opens the correct gap but leaves the toggled window at its floating frame
  until focus moves to the other terminal.
- Source diagnosis: the accepted reply clears intentional-float membership and
  records explicit tile overrides after the fresh observation is captured.
  Application still sees stale floating/fit-excluded flags and holds the tile
  frame; the sibling reflows. A later capture clears the stale flags.
- Existing log summaries show accepted revisions with geometry writes and holds,
  consistent with the source diagnosis; they contain no per-window payload
  proving which frame was held.
- Full-state route context: [record](core-full-state-replies.md).
- `kwin/tests/world-unfloat-retile.test.ts` drives the production entry with QML
  array-likes, renewed native wrappers, braced UUIDs, native property names and
  Qt zero/unbounded hints. Before the production correction, both unfloat tests
  failed at the retained floating frame; the fullscreen guard test passed.
  After correction, all three pass. The ordinary sequence asserts both float
  placement and immediate unfloat with no intervening event, plus no writes on
  a converged repeat. The fixed-size case proves the immediate write attempt;
  a native client may still clamp its requested tile frame.
- Final offline gates (2026-10-17): `cargo test --workspace` **2888 passed,
  0 failed, 80 test-result groups**; KWin `npm test` **1772 passed, 0 failed,
  266 suites**; KWin `npm run typecheck` and `git diff --check` pass.
- At original offline completion, native acceptance remained pending; offline
  geometry setters do not establish native compositor/client behavior.

## Original user re-check

1. Load the updated script with `just dev-off`, then `just dev` (foreground)
   or `just dev-on` (detached). `just reload` replaces only the Planner and
   does not load this script correction.
2. On one workspace with two tiled terminals, focus one and press Meta+G.
   Confirm it floats and the other terminal fills the available tile area.
3. Without changing focus, press Meta+G again. Confirm the toggled terminal
   immediately occupies its tile and the other terminal reflows; no vacant
   tile should remain waiting for a focus change.
4. Repeat the toggle pair, then change focus. Confirm there is no delayed snap
    into a previously vacant tile.

## Native callback observation, 2026-10-17/18

- Two KDE editor clients completed two callback-driven float/unfloat pairs,
  with immediate native tile occupation after each unfloat and stable geometry
  across subsequent focus changes. Physical Meta+G delivery was not tested.
- The fixture changed a pre-existing empty desktop's backing identity and
  continued after that baseline surprise. Functional observations therefore do
  not complete clean physical acceptance. The changed inventory was subsequently
  captured as the new fixture baseline (tentative, pending user review).
  See the [acceptance continuation](../kde-live-acceptance.md) for evidence,
  offline gates, retained configuration differences and subsequent live checks.

## Backlog detail moved 2026-11-01

### Original backlog lines 812-818

````markdown
- P0 | KDE Meta+G unfloat does not re-tile until focus changes | Fixed offline
  2026-10-17, pending user re-check. The apply observation retained stale
  floating/fit-excluded flags after accepted unfloat; settle float departures
  and tile overrides before capture while preserving float placement timing.
  Producer-shaped production-entry regressions fail before the correction and
  pass after it; Rust workspace **2888 passed**, KWin **1772 passed**.
  [Evidence and script-restart re-check](changes/archive/kde-unfloat-immediate-retile.md).
````

## Physical UF acceptance, 2026-11-16

- **UF: PASS.** Two normal resizable Wayland fixture windows completed two
  physical Meta+G float/unfloat pairs in the real KDE session. Each unfloat
  re-tiled B and reflowed A before any focus change. This closes the bounded
  immediate-unfloat defect, not maximized/fixed-client permutations in check 7.
- Evidence below is retained externally in `physical-kde-uf-evidence/`.
  `native-b-focused.json:10-56` and `fixture-b-focused.json:1` join both caller
  IDs, sentinel titles and fixture PID one-to-one to native UUIDs, with B focused.
  Testing used an appended desktop, fixture-only management, isolated settings
  with hover disabled, and isolated intent/preimage storage
  (`harness-execution-receipts.txt:1-5`, `permissions-provenance.txt:1-6`).

| Evidence ladder | First unfloat | Repeated unfloat |
| --- | --- | --- |
| Real key input | `leg-1b-unfloat-input.txt:1`; executed `ydotool` Meta+G command in `harness-execution-receipts.txt:10-11` | `leg-2b-unfloat-input.txt:1`; executed command in `harness-execution-receipts.txt:16-17` |
| KWin callback, queue and dispatch | `leg-1b-unfloat-journal.log:1-4`, command 2, request `src-d-um85by` | `leg-2b-unfloat-journal.log:1-4`, command 4, request `src-h-1clzmgb` |
| Planner queue, capture and accepted reply | `planner-dev.log:125-138`, revision 13 | `planner-dev.log:181-194`, revision 17 |
| Tiled reply and frame attempts | `leg-1b-unfloat-journal.log:12-15,29-32` | `leg-2b-unfloat-journal.log:12-15,29-32` |
| Native frame change and independent readback | `leg-1b-unfloat-journal.log:37,39`; `leg-1b-unfloat-native.json:10-56` | `leg-2b-unfloat-journal.log:37,39`; `leg-2b-unfloat-native.json:10-56` |

- In both unfloat replies, B's target/native frame is `(1028,52,1012,1092)`
  and A's is `(8,52,1012,1092)`. Before each unfloat B occupies the floating
  `(414,270,1219,655)` frame and A fills `(8,52,2032,1092)`. Float transitions
  are witnessed in `full-run-journal.log:401-437,535-573`.
- No-focus-change proof is continuous, not equal endpoints alone:
  `watch-helper.js:24-54` connects native frame and workspace activation signals;
  `full-run-journal.log:267,272,370` shows working setup activation callbacks.
  The final activation is B at line 370; no activation occurs during the four
  physical legs at lines 401-670. Both unfloat replies also carry `focusReq=0`.
- Objective callback-to-native-frame wall-clock intervals are B **25/27 ms**
  and A **35/36 ms**. Monotonic journal receipt intervals corroborate B
  **24.979/27.393 ms**, A **35.526/36.767 ms**
  (`unfloat-monotonic-journal.log:16,47-48,57,88-89`). These include logging
  delay, not compositor presentation/animation completion. Perceived smoothness
  remains a user judgment.
- No product failure reproduced; no source correction or new regression was
  needed. Setup corrected one desktop argument name and one empty-stdout helper
  parser error before the accepted sequence; neither supplied a focus workaround.
- Teardown evidence: logs copied before controller stop
  (`harness-execution-receipts.txt:19-23`, `teardown-receipts.txt:14-28`);
  all 14 helpers unloaded and absent (`helper-lifecycle.log:1-14`); controller
  unloaded and Planner stopped (`dev-off.txt:1,38`, `dev-status-after.txt:1-9`);
  fixture shutdown/owned PID absence (`fixture-shutdown.json:1`,
  `started-pid-absence.txt:1-2`). Socket absence, no actual input-daemon match
  apart from the checking shell, and unlocked state are retained in
  `teardown-absence-receipts.txt:1-12`. The detached route's runtime dev-log
  copies predate this run; current-run evidence is the journal and Planner log.
- Current offline verification: Rust format, three-package all-target build and
  strict clippy, permitted tests **3,455 passed / 0 failed** (71 summaries);
  KWin typecheck and **1,351 passed / 0 failed** (199 suites); launcher checks
  **703 + 204 passed / 0 failed**. No source or fixture bytes changed for UF.
